import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { installWallpaper } from "./WallpaperEditor.api";

// Mirrors `is_valid_wallpaper_id` in src-tauri/src/config.rs: must be a valid
// hostname fragment used in the custom protocol host.
const NAME_RE = /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/;

type Stage =
  | { kind: "name" }
  | { kind: "installing"; progress: ProgressState }
  | { kind: "done" }
  | { kind: "error"; message: string };

type ProgressState =
  | {
      phase: "progress";
      bytesDone: number;
      bytesTotal: number | null;
      filesDone: number;
      filesTotal: number | null;
    }
  | { phase: "validate" };

type ProgressEvent =
  | {
      phase: "progress";
      install_id: string;
      bytes_done: number;
      bytes_total: number | null;
      files_done: number;
      files_total: number | null;
    }
  | { phase: "validate"; install_id: string }
  | { phase: "done"; install_id: string }
  | { phase: "error"; install_id: string; message: string };

interface Props {
  sourceUrl: string | null;
  onClose: () => void;
  onInstalled?: () => void;
}

// Recover a usable URL from the custom `underpane+https://` scheme; a `file://`
// source is already usable as-is.
function innerUrl(sourceUrl: string): string {
  return sourceUrl.replace(/^underpane\+/, "");
}

function sanitize(s: string): string {
  return (
    s
      .toLowerCase()
      .replace(/[^a-z0-9-]+/g, "-")
      .replace(/^-+|-+$/g, "") || "wallpaper"
  );
}

function deriveDefaultName(sourceUrl: string): string {
  try {
    const u = new URL(innerUrl(sourceUrl));
    const filename = decodeURIComponent(
      u.pathname.split("/").filter(Boolean).pop() ?? "",
    );
    return sanitize(filename.split("_")[0] ?? "");
  } catch {
    return "wallpaper";
  }
}

function humanBytes(n: number | null | undefined): string {
  if (n == null) return "?";
  const units = ["B", "KiB", "MiB", "GiB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export function InstallWallpaperDialog({
  sourceUrl,
  onClose,
  onInstalled,
}: Props) {
  const open = sourceUrl !== null;
  const [name, setName] = useState("");
  const [stage, setStage] = useState<Stage>({ kind: "name" });
  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    if (!sourceUrl) return;
    setName(deriveDefaultName(sourceUrl));
    setStage({ kind: "name" });
  }, [sourceUrl]);

  useEffect(() => {
    if (!open && unlistenRef.current) {
      unlistenRef.current();
      unlistenRef.current = null;
    }
  }, [open]);

  const handleClose = useCallback(() => {
    if (unlistenRef.current) {
      unlistenRef.current();
      unlistenRef.current = null;
    }
    onClose();
  }, [onClose]);

  const nameError = useMemo(() => {
    if (!name) return "Name required";
    if (!NAME_RE.test(name)) {
      return "Allowed: lowercase letters, digits, '-' (cannot start or end with '-')";
    }
    return null;
  }, [name]);

  const zipUrl = sourceUrl ? innerUrl(sourceUrl) : "";

  const beginInstall = useCallback(async () => {
    if (!sourceUrl) return;
    const installId = crypto.randomUUID();

    const unlisten = await listen<ProgressEvent>("install-progress", (e) => {
      const payload = e.payload;
      if (payload.install_id !== installId) return;
      if (payload.phase === "progress") {
        setStage((prev) => {
          if (prev.kind !== "installing") return prev;
          return {
            ...prev,
            progress: {
              phase: "progress",
              bytesDone: payload.bytes_done,
              bytesTotal: payload.bytes_total,
              filesDone: payload.files_done,
              filesTotal: payload.files_total,
            },
          };
        });
      } else if (payload.phase === "validate") {
        setStage((prev) => {
          if (prev.kind !== "installing") return prev;
          return { ...prev, progress: { phase: "validate" } };
        });
      } else if (payload.phase === "done") {
        setStage({ kind: "done" });
        onInstalled?.();
      } else if (payload.phase === "error") {
        setStage({ kind: "error", message: payload.message });
      }
    });
    unlistenRef.current = unlisten;

    setStage({
      kind: "installing",
      progress: {
        phase: "progress",
        bytesDone: 0,
        bytesTotal: null,
        filesDone: 0,
        filesTotal: null,
      },
    });

    try {
      await installWallpaper({ name, zipUrl, installId });
    } catch (e: any) {
      // Backstop for IPC-level failures; Rust normally emits an "error" event.
      setStage({ kind: "error", message: String(e?.message ?? e) });
    }
  }, [sourceUrl, name, zipUrl, onInstalled]);

  return (
    <Dialog
      open={open}
      onOpenChange={(v) => {
        if (!v) handleClose();
      }}
    >
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>Install wallpaper</DialogTitle>
          <DialogDescription className="break-all">
            From: <code className="text-xs">{zipUrl}</code>
          </DialogDescription>
        </DialogHeader>

        {stage.kind === "name" && (
          <div className="space-y-3">
            <Label htmlFor="wp-name">Install to folder</Label>
            <Input
              id="wp-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="font-mono"
            />
            {nameError && (
              <p className="text-sm text-destructive">{nameError}</p>
            )}
            <DialogFooter>
              <Button variant="outline" onClick={handleClose}>
                Cancel
              </Button>
              <Button onClick={beginInstall} disabled={!!nameError}>
                Install
              </Button>
            </DialogFooter>
          </div>
        )}

        {stage.kind === "installing" && (
          <div className="space-y-3">
            <p className="text-sm">
              Installing <b>{name}</b>…
            </p>
            {stage.progress.phase === "progress" ? (
              <>
                <progress
                  className="w-full"
                  value={
                    stage.progress.bytesTotal
                      ? stage.progress.bytesDone
                      : undefined
                  }
                  max={stage.progress.bytesTotal ?? undefined}
                />
                <p className="text-xs text-muted-foreground">
                  {humanBytes(stage.progress.bytesDone)} /{" "}
                  {humanBytes(stage.progress.bytesTotal)}
                  {stage.progress.filesTotal != null
                    ? ` · ${stage.progress.filesDone}/${stage.progress.filesTotal} files`
                    : ""}
                </p>
              </>
            ) : (
              <p className="text-sm text-muted-foreground">Validating…</p>
            )}
          </div>
        )}

        {stage.kind === "done" && (
          <div className="space-y-3">
            <p className="text-sm">
              Installed <b>{name}</b>.
            </p>
            <DialogFooter>
              <Button onClick={handleClose}>Close</Button>
            </DialogFooter>
          </div>
        )}

        {stage.kind === "error" && (
          <div className="space-y-3">
            <p className="text-sm text-destructive">{stage.message}</p>
            <DialogFooter>
              <Button variant="outline" onClick={handleClose}>
                Cancel
              </Button>
              <Button onClick={() => setStage({ kind: "name" })}>Retry</Button>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

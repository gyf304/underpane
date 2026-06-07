import { useState, useEffect, useCallback, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ColorPicker } from "@/components/ui/color-picker";
import { Settings, RotateCcw } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  type SystemConfig,
  type MonitorInfo,
  type WallpaperManifest,
  type WallpaperConfigSchema,
  type SaveStatus,
} from "./WallpaperEditor.types";
import {
  readConfig,
  writeConfig,
  listMonitors,
  wallpapers,
  openConfigFile,
  openWallpapersDir,
  getAutostart,
  setAutostart,
  pickFile,
  pickDirectory,
} from "./WallpaperEditor.api";
import { Quickstart, type QuickstartStepId } from "./Quickstart";
import { InstallWallpaperDialog } from "./InstallWallpaperDialog";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";

const QUICKSTART_SEEN_KEY = "underpane.quickstartSeen";
const DEFAULT_KEY = "default";
const HIGHLIGHT = "ring-2 ring-primary ring-offset-2 ring-offset-background";

const CANVAS_H = 192;

function MonitorCanvas({
  monitors,
  selectedId,
  onSelect,
}: {
  monitors: MonitorInfo[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [canvasW, setCanvasW] = useState(400);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    setCanvasW(el.offsetWidth);
    const ro = new ResizeObserver((entries) => {
      const e = entries[0];
      if (e) setCanvasW(e.contentRect.width);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  if (monitors.length === 0) {
    return (
      <div
        ref={containerRef}
        className="w-full rounded bg-muted"
        style={{ height: CANVAS_H }}
      />
    );
  }

  const PAD = 16;
  const minX = Math.min(...monitors.map((m) => m.position.x));
  const minY = Math.min(...monitors.map((m) => m.position.y));
  const maxX = Math.max(...monitors.map((m) => m.position.x + m.size.width));
  const maxY = Math.max(...monitors.map((m) => m.position.y + m.size.height));

  const scale = Math.min(
    (canvasW - PAD * 2) / (maxX - minX),
    (CANVAS_H - PAD * 2) / (maxY - minY),
  );
  const ox = (canvasW - (maxX - minX) * scale) / 2;
  const oy = (CANVAS_H - (maxY - minY) * scale) / 2;

  return (
    <div
      ref={containerRef}
      className="relative w-full overflow-hidden rounded bg-muted"
      style={{ height: CANVAS_H }}
    >
      {monitors.map((m) => {
        const l = ox + (m.position.x - minX) * scale;
        const t = oy + (m.position.y - minY) * scale;
        const w = m.size.width * scale;
        const h = m.size.height * scale;

        return (
          <button
            key={m.id}
            type="button"
            onClick={() => onSelect(m.id)}
            className={cn(
              "absolute flex flex-col items-center justify-center overflow-hidden rounded border bg-card transition-colors",
              selectedId === m.id
                ? "border-primary text-primary"
                : "border-border text-muted-foreground hover:border-foreground hover:text-foreground",
            )}
            style={{ left: l, top: t, width: w, height: h }}
          >
            <span className="text-xs font-medium leading-tight">{m.id}</span>
          </button>
        );
      })}
    </div>
  );
}

function ConfigFieldRow({
  fieldKey,
  field,
  value,
  onChange,
}: {
  fieldKey: string;
  field: WallpaperConfigSchema;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const id = `cfg-${fieldKey}`;
  const placeholder =
    "default" in field && field.default != null
      ? String(field.default)
      : undefined;
  // An unset value (null/undefined) already resolves to the default, so reverting
  // simply clears the override. Disable the control when nothing to revert.
  const isDefault = value == null;

  const labelRow = (
    <div className="flex items-center gap-1.5 flex-wrap">
      <Label
        htmlFor={id}
        className="cursor-pointer text-sm font-medium leading-none"
      >
        {field.name}
      </Label>
      <span className="rounded border px-1 py-px text-[10px] font-normal leading-tight text-muted-foreground">
        {field.group}
      </span>
      <code className="rounded bg-muted px-1.5 py-px font-mono text-[10px] leading-tight text-muted-foreground">
        {fieldKey}
      </code>
      <button
        type="button"
        aria-label="Revert to default"
        title="Revert to default"
        disabled={isDefault}
        onClick={() => onChange(undefined)}
        className="ml-auto inline-flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-30"
      >
        <RotateCcw className="size-3" />
      </button>
    </div>
  );

  if (field.type === "bool") {
    return (
      <div className="flex items-start justify-between gap-4">
        <div className="flex flex-1 flex-col gap-1">
          {labelRow}
          {field.description && (
            <p className="text-xs leading-snug text-muted-foreground">
              {field.description}
            </p>
          )}
        </div>
        <Switch
          id={id}
          checked={value != null ? Boolean(value) : Boolean(field.default)}
          onCheckedChange={onChange}
        />
      </div>
    );
  }

  if (field.type === "color") {
    return (
      <div className="space-y-1.5">
        {labelRow}
        {field.description && (
          <p className="text-xs leading-snug text-muted-foreground">
            {field.description}
          </p>
        )}
        <ColorPicker
          id={id}
          value={typeof value === "string" ? value : undefined}
          defaultValue={field.default}
          alpha={Boolean(field.alpha)}
          onChange={onChange}
        />
      </div>
    );
  }

  if (field.type === "file") {
    const path = typeof value === "string" ? value : "";
    const basename = path ? path.split(/[/\\]/).pop() || path : "";
    return (
      <div className="space-y-1.5">
        {labelRow}
        {field.description && (
          <p className="text-xs leading-snug text-muted-foreground">
            {field.description}
          </p>
        )}
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 shrink-0"
            onClick={async () => {
              const picked = await pickFile(field.extensions);
              if (picked) onChange(picked);
            }}
          >
            {t("editor.chooseFile")}
          </Button>
          <span
            className="truncate font-mono text-xs text-muted-foreground"
            title={path || undefined}
          >
            {basename || t("editor.noFileChosen")}
          </span>
        </div>
      </div>
    );
  }

  if (field.type === "directory") {
    const path = typeof value === "string" ? value : "";
    const basename = path
      ? path.split(/[/\\]/).filter(Boolean).pop() || path
      : "";
    return (
      <div className="space-y-1.5">
        {labelRow}
        {field.description && (
          <p className="text-xs leading-snug text-muted-foreground">
            {field.description}
          </p>
        )}
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 shrink-0"
            onClick={async () => {
              const picked = await pickDirectory();
              if (picked) onChange(picked);
            }}
          >
            {t("editor.chooseFolder")}
          </Button>
          <span
            className="truncate font-mono text-xs text-muted-foreground"
            title={path || undefined}
          >
            {basename || t("editor.noFolderChosen")}
          </span>
        </div>
      </div>
    );
  }

  const hasSlider =
    field.type === "number" && field.min != null && field.max != null;
  const sliderDisplay =
    value != null
      ? Number(value)
      : field.default != null
        ? Number(field.default)
        : field.type === "number"
          ? (field.min ?? 0)
          : 0;

  return (
    <div className="space-y-1.5">
      {labelRow}
      {field.description && (
        <p className="text-xs leading-snug text-muted-foreground">
          {field.description}
        </p>
      )}
      {hasSlider ? (
        <div className="flex items-center gap-2">
          <Slider
            min={field.min}
            max={field.max}
            step={field.step ?? 1}
            value={[sliderDisplay]}
            onValueChange={(v) => onChange(v[0])}
            className="h-2 flex-1 cursor-pointer accent-primary"
          />
          <Input
            id={id}
            type="number"
            min={field.min}
            max={field.max}
            step={field.step}
            className="h-8 w-20 shrink-0 font-mono text-sm"
            value={value != null ? String(value) : ""}
            placeholder={placeholder}
            onChange={(e) => {
              const v = e.target.value;
              onChange(v !== "" ? Number(v) : undefined);
            }}
          />
        </div>
      ) : (
        <Input
          id={id}
          type={field.type === "number" ? "number" : "text"}
          className="h-8 font-mono text-sm"
          value={value != null ? String(value) : ""}
          placeholder={placeholder}
          onChange={(e) => {
            const v = e.target.value;
            onChange(
              field.type === "number"
                ? v !== ""
                  ? Number(v)
                  : undefined
                : v !== ""
                  ? v
                  : undefined,
            );
          }}
        />
      )}
    </div>
  );
}

// ─── Status Badge ─────────────────────────────────────────────────────────────

const STATUS_VARIANTS = {
  idle: {
    labelKey: "status.idle",
    className: "bg-muted text-muted-foreground",
  },
  unsaved: {
    labelKey: "status.unsaved",
    className:
      "bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300",
  },
  saving: {
    labelKey: "status.saving",
    className: "animate-pulse bg-muted text-muted-foreground",
  },
  saved: {
    labelKey: "status.saved",
    className:
      "bg-green-100 text-green-800 dark:bg-green-950/60 dark:text-green-300",
  },
  error: {
    labelKey: "status.error",
    className: "bg-red-100 text-red-800 dark:bg-red-950/60 dark:text-red-300",
  },
} as const satisfies Record<
  SaveStatus,
  { labelKey: string; className: string }
>;

function StatusBadge({ status }: { status: SaveStatus }) {
  const { labelKey, className } = STATUS_VARIANTS[status];
  return (
    <span className={cn("rounded px-2 py-0.5 text-xs font-medium", className)}>
      {t(labelKey)}
    </span>
  );
}

// ─── WallpaperEditor ──────────────────────────────────────────────────────────

export function WallpaperEditor() {
  const [config, setConfig] = useState<SystemConfig | null>(null);
  const [monitorList, setMonitorList] = useState<MonitorInfo[]>([]);
  const [wallpaperMap, setWallpaperMap] = useState<
    Map<string, WallpaperManifest>
  >(new Map());
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [view, setView] = useState<"general" | "monitor">("monitor");
  const [status, setStatus] = useState<SaveStatus>("idle");
  const [loading, setLoading] = useState(true);
  const [autostart, setAutostartState] = useState<boolean | null>(null);
  const [savedAutostart, setSavedAutostart] = useState<boolean | null>(null);
  const [quickstartOpen, setQuickstartOpen] = useState(false);
  const [quickstartStep, setQuickstartStep] = useState<QuickstartStepId | null>(
    null,
  );
  const [installSourceUrl, setInstallSourceUrl] = useState<string | null>(null);

  const refreshWallpapers = useCallback(async () => {
    setWallpaperMap(await wallpapers());
  }, []);

  const selectMonitor = (id: string) => {
    setSelectedId(id);
    setView("monitor");
  };

  const handleQuickstartOpenChange = (open: boolean) => {
    setQuickstartOpen(open);
    if (!open) {
      setQuickstartStep(null);
      if (typeof localStorage !== "undefined") {
        localStorage.setItem(QUICKSTART_SEEN_KEY, "true");
      }
    }
  };

  // Track the active tour step so we can highlight its target and gate "Next"
  // until the user reaches the expected state themselves.
  const handleQuickstartStep = useCallback((id: QuickstartStepId) => {
    setQuickstartStep(id);
  }, []);

  useEffect(() => {
    if (
      typeof localStorage !== "undefined" &&
      localStorage.getItem(QUICKSTART_SEEN_KEY) !== "true"
    ) {
      setQuickstartOpen(true);
    }
  }, []);

  // Install requests from a deep-link or `.underpane` file open. The Rust side
  // delays its emit so a cold-start launch doesn't miss this listener.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ source_url: string }>("install-request", (e) => {
      setInstallSourceUrl(e.payload.source_url);
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    Promise.all([
      readConfig(),
      listMonitors(),
      wallpapers(),
      getAutostart(),
    ]).then(([cfg, monitors, wpMap, autostartEnabled]) => {
      setConfig(cfg);
      setMonitorList(monitors);
      setWallpaperMap(wpMap);
      setSelectedId(monitors[0]?.id ?? null);
      setAutostartState(autostartEnabled);
      setSavedAutostart(autostartEnabled);
      setLoading(false);
    });
  }, []);

  const handleAutostartChange = (enabled: boolean) => {
    setAutostartState(enabled);
    setStatus("unsaved");
  };

  const handleWallpaperChange = (
    monitorId: string,
    wallpaperId: string | null,
  ) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const monitors = { ...prev.monitors };
      if (wallpaperId) {
        monitors[monitorId] = { wallpaper: wallpaperId, config: {} };
      } else {
        delete monitors[monitorId];
      }
      return { ...prev, monitors };
    });
    setStatus("unsaved");
  };

  const handleFieldChange = (
    monitorId: string,
    key: string,
    value: unknown,
  ) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const m = prev.monitors?.[monitorId];
      if (!m) return prev;
      const newCfg = { ...m.config };
      if (value === undefined || value === null) {
        delete newCfg[key];
      } else {
        newCfg[key] = value;
      }
      return {
        ...prev,
        monitors: { ...prev.monitors, [monitorId]: { ...m, config: newCfg } },
      };
    });
    setStatus("unsaved");
  };

  const handleSave = async () => {
    if (!config || status !== "unsaved") return;
    setStatus("saving");
    try {
      await writeConfig(config);
      if (autostart !== null && autostart !== savedAutostart) {
        await setAutostart(autostart);
        setSavedAutostart(autostart);
      }
      setStatus("saved");
    } catch {
      setStatus("error");
    }
  };

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-sm text-muted-foreground">
          {t("editor.loading")}
        </span>
      </div>
    );
  }

  const selMonitor = selectedId ? config?.monitors?.[selectedId] : undefined;
  const selWallpaper = selMonitor?.wallpaper;
  const selManifest = selWallpaper ? wallpaperMap.get(selWallpaper) : null;

  const validFields: Array<[string, WallpaperConfigSchema]> = selManifest
    ? Object.entries(selManifest.config)
    : [];

  // Group fields by their `group` property
  const grouped = new Map<string, Array<[string, WallpaperConfigSchema]>>();
  for (const entry of validFields) {
    const g = entry[1].group;
    if (!grouped.has(g)) grouped.set(g, []);
    grouped.get(g)!.push(entry);
  }

  const NONE = "__none__";

  const isMonitorView = view === "monitor";

  // Gate the tour's "Next" button: each step waits until the user has reached
  // the expected state themselves before they can advance.
  const quickstartCanProceed = (() => {
    switch (quickstartStep) {
      case "monitors":
        return isMonitorView && selectedId !== null;
      case "wallpaper":
        return Boolean(selMonitor?.wallpaper);
      case "addWallpapers":
      case "autostart":
        return view === "general";
      default:
        return true;
    }
  })();

  return (
    <div className="flex h-full flex-row bg-background text-foreground">
      {/* Left panel — full height, monitor layout + list */}
      <div className="flex w-80 shrink-0 flex-col gap-3 overflow-y-auto border-r p-4">
        <span className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">
          {t("editor.monitors")}
        </span>
        <MonitorCanvas
          monitors={monitorList}
          selectedId={isMonitorView ? selectedId : null}
          onSelect={selectMonitor}
        />

        <div className="space-y-0.5">
          {monitorList.map((m) => {
            const mc = config?.monitors?.[m.id];
            const wpName = mc?.wallpaper
              ? (wallpaperMap.get(mc.wallpaper)?.name ?? mc.wallpaper)
              : "—";
            return (
              <button
                key={m.id}
                type="button"
                onClick={() => selectMonitor(m.id)}
                className={cn(
                  "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors",
                  isMonitorView && selectedId === m.id
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                <span className="w-5 shrink-0 font-mono text-xs font-semibold">
                  {m.id}
                </span>
                <span className="min-w-0 truncate text-xs">{wpName}</span>
              </button>
            );
          })}
          <div className="my-1 border-t" />
          {(() => {
            const dc = config?.monitors?.[DEFAULT_KEY];
            const wpName = dc?.wallpaper
              ? (wallpaperMap.get(dc.wallpaper)?.name ?? dc.wallpaper)
              : "—";
            return (
              <button
                type="button"
                onClick={() => selectMonitor(DEFAULT_KEY)}
                className={cn(
                  "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors",
                  isMonitorView && selectedId === DEFAULT_KEY
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                  quickstartStep === "monitors" && HIGHLIGHT,
                )}
              >
                <span className="w-5 shrink-0 font-mono text-xs font-semibold">
                  *
                </span>
                <span className="min-w-0 truncate text-xs">{wpName}</span>
              </button>
            );
          })()}
        </div>

        <div className="flex-1" />

        <button
          type="button"
          onClick={() => setView("general")}
          className={cn(
            "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm transition-colors",
            view === "general"
              ? "bg-accent text-accent-foreground"
              : "text-muted-foreground hover:bg-muted hover:text-foreground",
            view !== "general" &&
              (quickstartStep === "addWallpapers" ||
                quickstartStep === "autostart") &&
              HIGHLIGHT,
          )}
        >
          <Settings className="h-4 w-4 shrink-0" />
          <span className="text-xs font-semibold uppercase tracking-widest">
            {t("editor.generalSettings")}
          </span>
        </button>
      </div>

      {/* Right column */}
      <div className="flex flex-1 flex-col overflow-hidden">
        {/* Toolbar */}
        <div className="flex shrink-0 items-center gap-3 border-b px-4 py-2.5">
          <div className="flex-1" />
          <StatusBadge status={status} />
          <Button
            size="sm"
            disabled={status !== "unsaved"}
            onClick={handleSave}
            className={cn(quickstartStep === "config" && HIGHLIGHT)}
          >
            {t("editor.save")}
          </Button>
        </div>

        {/* Config editor */}
        <div className="flex flex-1 flex-col overflow-y-auto p-5">
          {view === "general" ? (
            <div className="max-w-lg space-y-4">
              <div className="space-y-1.5">
                <Label className="text-sm font-medium leading-none">
                  {t("general.configFile.label")}
                </Label>
                <p className="text-xs leading-snug text-muted-foreground">
                  {t("general.configFile.desc")}
                </p>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    openConfigFile().catch((err) =>
                      console.error("failed to open config file", err),
                    );
                  }}
                >
                  {t("general.configFile.button")}
                </Button>
              </div>
              <div className="space-y-1.5">
                <Label className="text-sm font-medium leading-none">
                  {t("general.wallpapersFolder.label")}
                </Label>
                <p className="text-xs leading-snug text-muted-foreground">
                  {t("general.wallpapersFolder.desc")}
                </p>
                <Button
                  size="sm"
                  variant="outline"
                  className={cn(
                    quickstartStep === "addWallpapers" && HIGHLIGHT,
                  )}
                  onClick={() => {
                    openWallpapersDir().catch((err) =>
                      console.error("failed to open wallpapers dir", err),
                    );
                  }}
                >
                  {t("general.wallpapersFolder.button")}
                </Button>
              </div>
              <div className="flex items-start justify-between gap-4">
                <div className="flex flex-1 flex-col gap-1">
                  <Label className="text-sm font-medium leading-none">
                    {t("general.autostart.label")}
                  </Label>
                  <p className="text-xs leading-snug text-muted-foreground">
                    {t("general.autostart.desc")}
                  </p>
                </div>
                <Switch
                  checked={autostart ?? false}
                  onCheckedChange={handleAutostartChange}
                  disabled={autostart === null}
                  className={cn(quickstartStep === "autostart" && HIGHLIGHT)}
                />
              </div>
              <div className="space-y-1.5">
                <Label className="text-sm font-medium leading-none">
                  {t("general.quickstart.label")}
                </Label>
                <p className="text-xs leading-snug text-muted-foreground">
                  {t("general.quickstart.desc")}
                </p>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setQuickstartOpen(true)}
                >
                  {t("general.quickstart.button")}
                </Button>
              </div>
            </div>
          ) : !selectedId ? (
            <p className="text-sm text-muted-foreground">
              {t("editor.selectMonitor")}
            </p>
          ) : (
            <div className="max-w-lg space-y-6">
              {/* Wallpaper selector */}
              <div className="space-y-1.5">
                <Label className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">
                  {t("editor.wallpaper")}
                </Label>
                <Select
                  value={selMonitor?.wallpaper ?? NONE}
                  onValueChange={(v) =>
                    handleWallpaperChange(selectedId, v === NONE ? null : v)
                  }
                >
                  <SelectTrigger
                    className={cn(
                      "h-8 w-full text-sm",
                      quickstartStep === "wallpaper" && HIGHLIGHT,
                    )}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={NONE}>
                      <span className="italic text-muted-foreground">
                        {t("editor.none")}
                      </span>
                    </SelectItem>
                    {Array.from(wallpaperMap.entries()).map(([id, wp]) => (
                      <SelectItem key={id} value={id}>
                        <span className="flex items-baseline gap-2">
                          <span>{wp.name}</span>
                          <span className="text-xs text-muted-foreground font-mono">
                            ({id})
                          </span>
                        </span>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {/* Config fields */}
              {selManifest && (
                <div className="space-y-4">
                  {grouped.size === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      {t("editor.noFields")}
                    </p>
                  ) : (
                    Array.from(grouped.entries()).map(([group, fields]) => (
                      <div key={group} className="space-y-3">
                        <div className="flex items-center gap-2">
                          <span className="shrink-0 text-xs font-semibold uppercase tracking-widest text-muted-foreground">
                            {group}
                          </span>
                          <div className="h-px flex-1 bg-border" />
                        </div>
                        {fields.map(([key, field]) => (
                          <ConfigFieldRow
                            key={key}
                            fieldKey={key}
                            field={field}
                            value={selMonitor?.config?.[key]}
                            onChange={(v) =>
                              handleFieldChange(selectedId, key, v)
                            }
                          />
                        ))}
                      </div>
                    ))
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      <Quickstart
        open={quickstartOpen}
        onOpenChange={handleQuickstartOpenChange}
        onStepChange={handleQuickstartStep}
        canProceed={quickstartCanProceed}
      />

      <InstallWallpaperDialog
        sourceUrl={installSourceUrl}
        onClose={() => setInstallSourceUrl(null)}
        onInstalled={refreshWallpapers}
      />
    </div>
  );
}

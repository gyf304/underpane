import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { t } from "./i18n";

export type QuickstartStepId =
  | "welcome"
  | "monitors"
  | "wallpaper"
  | "config"
  | "addWallpapers"
  | "autostart"
  | "done";

const STEPS = [
  {
    id: "welcome",
    titleKey: "quickstart.welcome.title",
    descKey: "quickstart.welcome.desc",
  },
  {
    id: "monitors",
    titleKey: "quickstart.monitors.title",
    descKey: "quickstart.monitors.desc",
  },
  {
    id: "wallpaper",
    titleKey: "quickstart.wallpaper.title",
    descKey: "quickstart.wallpaper.desc",
  },
  {
    id: "config",
    titleKey: "quickstart.config.title",
    descKey: "quickstart.config.desc",
  },
  {
    id: "addWallpapers",
    titleKey: "quickstart.addWallpapers.title",
    descKey: "quickstart.addWallpapers.desc",
  },
  {
    id: "autostart",
    titleKey: "quickstart.autostart.title",
    descKey: "quickstart.autostart.desc",
  },
  {
    id: "done",
    titleKey: "quickstart.done.title",
    descKey: "quickstart.done.desc",
  },
] as const;

export function Quickstart({
  open,
  onOpenChange,
  onStepChange,
  canProceed = true,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Called with the active step id whenever it changes (incl. on open) so the
   *  editor can highlight the step's target and report readiness. */
  onStepChange?: (id: QuickstartStepId) => void;
  /** When false, "Next" is disabled until the user reaches the step's expected
   *  state. Ignored on the final step (whose button just closes the tour). */
  canProceed?: boolean;
}) {
  const [step, setStep] = useState(0);

  useEffect(() => {
    if (open) setStep(0);
  }, [open]);

  useEffect(() => {
    if (open) onStepChange?.(STEPS[step]!.id);
  }, [open, step, onStepChange]);

  const isFirst = step === 0;
  const isLast = step === STEPS.length - 1;
  const current = STEPS[step]!;

  return (
    <Dialog open={open} onOpenChange={onOpenChange} modal={false}>
      <DialogContent
        showOverlay={false}
        className="top-auto bottom-4 left-auto right-4 translate-x-0 translate-y-0"
        onInteractOutside={(e) => e.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t(current.titleKey)}</DialogTitle>
          <DialogDescription>{t(current.descKey)}</DialogDescription>
        </DialogHeader>

        <div className="flex justify-center gap-1.5 py-2">
          {STEPS.map((s, i) => (
            <span
              key={s.id}
              className={cn(
                "h-1.5 w-1.5 rounded-full transition-colors",
                i === step ? "bg-primary" : "bg-muted",
              )}
            />
          ))}
        </div>

        <DialogFooter className="sm:justify-between">
          {!isLast ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => onOpenChange(false)}
            >
              {t("quickstart.skip")}
            </Button>
          ) : (
            <span />
          )}
          <div className="flex gap-2">
            {!isFirst && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => setStep((s) => s - 1)}
              >
                {t("quickstart.back")}
              </Button>
            )}
            <Button
              size="sm"
              disabled={!isLast && !canProceed}
              onClick={() =>
                isLast ? onOpenChange(false) : setStep((s) => s + 1)
              }
            >
              {isLast ? t("quickstart.done") : t("quickstart.next")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

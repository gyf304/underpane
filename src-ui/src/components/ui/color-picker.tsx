import * as React from "react";
import { Popover } from "radix-ui";

import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";

// ─── Color model ────────────────────────────────────────────────────────────
//
// We keep HSVA as the picker's internal source of truth while the user is
// interacting. Deriving HSV from the committed hex on every render loses
// information at extremes (hue is undefined for greys, saturation for black),
// which makes the cursors jump around. Internal state avoids that; we only
// re-derive from the prop when it changes from the outside.

type Hsva = { h: number; s: number; v: number; a: number };

const HEX_RE = /^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
const clamp01 = (n: number) => Math.max(0, Math.min(1, n));
const clamp255 = (n: number) => Math.max(0, Math.min(255, Math.round(n)));

function parseHexToHsva(s: string | undefined): Hsva | null {
  if (!s) return null;
  const m = HEX_RE.exec(s.trim());
  if (!m) return null;
  const hex = m[1]!;
  const r = parseInt(hex.slice(0, 2), 16);
  const g = parseInt(hex.slice(2, 4), 16);
  const b = parseInt(hex.slice(4, 6), 16);
  const a = hex.length === 8 ? parseInt(hex.slice(6, 8), 16) / 255 : 1;
  return { ...rgbToHsv(r, g, b), a };
}

function hsvaToHex(hsva: Hsva, withAlpha: boolean): string {
  const { r, g, b } = hsvToRgb(hsva.h, hsva.s, hsva.v);
  const f = (n: number) => clamp255(n).toString(16).padStart(2, "0");
  let hex = `#${f(r)}${f(g)}${f(b)}`;
  if (withAlpha) hex += f(hsva.a * 255);
  return hex;
}

function hsvToRgb(h: number, s: number, v: number): { r: number; g: number; b: number } {
  const i = Math.floor(h / 60) % 6;
  const f = h / 60 - Math.floor(h / 60);
  const p = v * (1 - s);
  const q = v * (1 - f * s);
  const t = v * (1 - (1 - f) * s);
  const [r, g, b] =
    i === 0 ? [v, t, p] :
    i === 1 ? [q, v, p] :
    i === 2 ? [p, v, t] :
    i === 3 ? [p, q, v] :
    i === 4 ? [t, p, v] :
              [v, p, q];
  return { r: r * 255, g: g * 255, b: b * 255 };
}

function rgbToHsv(r: number, g: number, b: number): { h: number; s: number; v: number } {
  const rn = r / 255, gn = g / 255, bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const d = max - min;
  let h = 0;
  if (d !== 0) {
    if (max === rn) h = ((gn - bn) / d) % 6;
    else if (max === gn) h = (bn - rn) / d + 2;
    else h = (rn - gn) / d + 4;
    h = (h * 60 + 360) % 360;
  }
  return { h, s: max === 0 ? 0 : d / max, v: max };
}

const rgbCss = (h: number, s: number, v: number) => {
  const { r, g, b } = hsvToRgb(h, s, v);
  return `rgb(${clamp255(r)}, ${clamp255(g)}, ${clamp255(b)})`;
};

const CHECKERBOARD =
  "linear-gradient(45deg, #ccc 25%, transparent 25%, transparent 75%, #ccc 75%), " +
  "linear-gradient(45deg, #ccc 25%, transparent 25%, transparent 75%, #ccc 75%)";

// ─── Drag handling ──────────────────────────────────────────────────────────
//
// Reports the pointer position within the element as fractions in [0, 1].

function useDrag(onChange: (fx: number, fy: number) => void) {
  const ref = React.useRef<HTMLDivElement>(null);

  const onPointerDown = React.useCallback(
    (e: React.PointerEvent) => {
      const el = ref.current;
      if (!el) return;
      e.preventDefault();

      const update = (cx: number, cy: number) => {
        const r = el.getBoundingClientRect();
        onChange(clamp01((cx - r.left) / r.width), clamp01((cy - r.top) / r.height));
      };
      update(e.clientX, e.clientY);

      const move = (ev: PointerEvent) => update(ev.clientX, ev.clientY);
      const up = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
        window.removeEventListener("pointercancel", up);
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
      window.addEventListener("pointercancel", up);
    },
    [onChange],
  );

  return { ref, onPointerDown };
}

const THUMB =
  "pointer-events-none absolute -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.4)]";

// ─── Saturation / Value square ──────────────────────────────────────────────

function SVSquare({ hsva, onChange }: { hsva: Hsva; onChange: (s: number, v: number) => void }) {
  const { ref, onPointerDown } = useDrag((fx, fy) => onChange(fx, 1 - fy));

  const onKeyDown = (e: React.KeyboardEvent) => {
    const step = e.shiftKey ? 0.1 : 0.02;
    if (e.key === "ArrowLeft") onChange(clamp01(hsva.s - step), hsva.v);
    else if (e.key === "ArrowRight") onChange(clamp01(hsva.s + step), hsva.v);
    else if (e.key === "ArrowUp") onChange(hsva.s, clamp01(hsva.v + step));
    else if (e.key === "ArrowDown") onChange(hsva.s, clamp01(hsva.v - step));
    else return;
    e.preventDefault();
  };

  return (
    <div
      ref={ref}
      role="slider"
      aria-label="Saturation and brightness"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(hsva.v * 100)}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
      className="relative h-40 w-full touch-none select-none rounded-md outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
      style={{
        backgroundColor: `hsl(${hsva.h}, 100%, 50%)`,
        backgroundImage:
          "linear-gradient(to top, black, transparent), linear-gradient(to right, white, transparent)",
      }}
    >
      <div
        className={cn(THUMB, "h-3.5 w-3.5")}
        style={{
          left: `${hsva.s * 100}%`,
          top: `${(1 - hsva.v) * 100}%`,
          backgroundColor: rgbCss(hsva.h, hsva.s, hsva.v),
        }}
      />
    </div>
  );
}

// ─── Horizontal slider (hue / alpha) ────────────────────────────────────────

function Slider1D({
  value,
  max,
  label,
  style,
  thumbColor,
  onChange,
}: {
  value: number; // 0..1 position
  max: number; // aria + step scale
  label: string;
  style: React.CSSProperties;
  thumbColor: string;
  onChange: (fraction: number) => void;
}) {
  const { ref, onPointerDown } = useDrag(fx => onChange(fx));

  const onKeyDown = (e: React.KeyboardEvent) => {
    const step = (e.shiftKey ? 10 : 1) / max;
    if (e.key === "ArrowLeft" || e.key === "ArrowDown") onChange(clamp01(value - step));
    else if (e.key === "ArrowRight" || e.key === "ArrowUp") onChange(clamp01(value + step));
    else return;
    e.preventDefault();
  };

  return (
    <div
      ref={ref}
      role="slider"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={Math.round(value * max)}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
      className="relative h-3.5 w-full touch-none select-none rounded-md outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
      style={style}
    >
      <div
        className={cn(THUMB, "top-1/2 h-4 w-4")}
        style={{ left: `${value * 100}%`, backgroundColor: thumbColor }}
      />
    </div>
  );
}

// ─── Hex text input ─────────────────────────────────────────────────────────

function HexInput({
  value,
  placeholder,
  onCommit,
  onClear,
  className,
  id,
}: {
  value: string;
  placeholder: string;
  onCommit: (hex: string) => void;
  onClear: () => void;
  className?: string;
  id?: string;
}) {
  const [draft, setDraft] = React.useState(value);
  React.useEffect(() => setDraft(value), [value]);

  const trimmed = draft.trim();
  const valid = HEX_RE.test(trimmed);

  const commit = () => {
    if (valid) onCommit(trimmed);
    else if (trimmed === "") onClear();
    else setDraft(value);
  };

  return (
    <Input
      id={id}
      type="text"
      spellCheck={false}
      autoComplete="off"
      value={draft}
      placeholder={placeholder}
      aria-invalid={!valid && trimmed.length > 0}
      className={cn("h-8 font-mono text-sm", className)}
      onChange={e => {
        const next = e.target.value;
        setDraft(next);
        const t = next.trim();
        if (HEX_RE.test(t)) onCommit(t);
        else if (t === "") onClear();
      }}
      onBlur={commit}
      onKeyDown={e => {
        if (e.key === "Enter") {
          commit();
          (e.target as HTMLInputElement).blur();
        } else if (e.key === "Escape") {
          setDraft(value);
          (e.target as HTMLInputElement).blur();
        }
      }}
    />
  );
}

// ─── ColorPicker ────────────────────────────────────────────────────────────

export function ColorPicker({
  value,
  defaultValue,
  onChange,
  alpha,
  id,
}: {
  value: string | undefined;
  defaultValue?: string;
  onChange: (v: string | undefined) => void;
  alpha: boolean;
  id?: string;
}) {
  const display = value ?? defaultValue;

  // Internal HSVA state is the source of truth during interaction.
  const [hsva, setHsva] = React.useState<Hsva>(
    () => parseHexToHsva(display) ?? { h: 0, s: 0, v: 0, a: 1 },
  );
  // Remember what we last emitted so external changes can be told apart from
  // our own round-tripped value (which we must not clobber, to keep cursors put).
  const lastEmitted = React.useRef<string | undefined>(undefined);

  React.useEffect(() => {
    if (display === lastEmitted.current) return;
    const next = parseHexToHsva(display);
    if (next) setHsva(next);
  }, [display]);

  const emit = React.useCallback(
    (next: Hsva) => {
      setHsva(next);
      const hex = hsvaToHex(next, alpha);
      lastEmitted.current = hex;
      onChange(hex);
    },
    [alpha, onChange],
  );

  const handleHex = React.useCallback(
    (hex: string) => {
      const next = parseHexToHsva(hex);
      if (next) emit(next);
    },
    [emit],
  );
  const handleClear = React.useCallback(() => {
    lastEmitted.current = undefined;
    onChange(undefined);
  }, [onChange]);

  const solid = rgbCss(hsva.h, hsva.s, hsva.v);

  const hueStyle: React.CSSProperties = {
    backgroundImage:
      "linear-gradient(to right, #f00 0%, #ff0 17%, #0f0 33%, #0ff 50%, #00f 67%, #f0f 83%, #f00 100%)",
  };
  const alphaStyle: React.CSSProperties = {
    backgroundColor: "#fff",
    backgroundImage: `linear-gradient(to right, transparent, ${solid}), ${CHECKERBOARD}`,
    backgroundSize: "100% 100%, 8px 8px, 8px 8px",
    backgroundPosition: "0 0, 0 0, 4px 4px",
  };

  return (
    <div className="flex items-center gap-2">
      <Popover.Root>
        <Popover.Trigger asChild>
          <button
            type="button"
            aria-label="Pick color"
            className={cn(
              "relative h-8 w-8 shrink-0 rounded-md border border-input shadow-xs outline-none transition-[box-shadow] focus-visible:ring-[3px] focus-visible:ring-ring/50",
              !display && "bg-[length:8px_8px] bg-[position:0_0,4px_4px]",
            )}
            style={display ? { backgroundColor: display } : { backgroundImage: CHECKERBOARD }}
          />
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content
            align="start"
            sideOffset={6}
            className="z-50 w-64 origin-(--radix-popover-content-transform-origin) rounded-md border bg-popover p-3 text-popover-foreground shadow-md outline-none data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95"
          >
            <div className="space-y-3">
              <SVSquare hsva={hsva} onChange={(s, v) => emit({ ...hsva, s, v })} />
              <Slider1D
                value={hsva.h / 360}
                max={360}
                label="Hue"
                style={hueStyle}
                thumbColor={`hsl(${hsva.h}, 100%, 50%)`}
                onChange={f => emit({ ...hsva, h: f * 360 })}
              />
              {alpha && (
                <Slider1D
                  value={hsva.a}
                  max={100}
                  label="Opacity"
                  style={alphaStyle}
                  thumbColor={solid}
                  onChange={a => emit({ ...hsva, a })}
                />
              )}
              <HexInput
                id={id}
                value={display ?? ""}
                placeholder={defaultValue ?? "#rrggbb"}
                onCommit={handleHex}
                onClear={handleClear}
                className="w-full"
              />
            </div>
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>

      <HexInput
        value={display ?? ""}
        placeholder={defaultValue ?? "#rrggbb"}
        onCommit={handleHex}
        onClear={handleClear}
        className="w-28 shrink-0"
      />
    </div>
  );
}

import { type ReactNode, useRef } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { Loader2 } from "lucide-react";

export function Switch({
  checked,
  onChange,
  disabled,
  pending,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  pending?: boolean;
  label?: string;
}) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onChange(!checked);
      }}
      className={`relative box-border h-6 w-10 shrink-0 rounded-full border-2 transition-colors disabled:opacity-40 ${
        checked ? "border-primary bg-primary" : "border-outline bg-surface-c-highest"
      } ${pending ? "pulse-pending" : ""}`}
    >
      {/* Thumb: centred with top/bottom:0 + margin:auto (no transform), moved via `left` —
          an animation library writing `transform` can never push it off-centre. Inner box is 36×20 px. */}
      <span
        className={`absolute inset-y-0 my-auto flex items-center justify-center rounded-full transition-all duration-200 ease-out ${
          checked ? "bg-on-primary" : "bg-outline"
        }`}
        style={checked ? { width: 16, height: 16, left: 18 } : { width: 12, height: 12, left: 4 }}
      >
        {pending && <Loader2 size={10} className="animate-spin text-primary" />}
      </span>
    </button>
  );
}

export interface SegOption<T extends string> {
  value: T;
  label: string;
  icon?: ReactNode;
}

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  pendingValue,
  disabled,
  layoutId,
  size = "md",
}: {
  options: SegOption<T>[];
  value: T | null;
  onChange: (v: T) => void;
  pendingValue?: T | null;
  disabled?: boolean;
  layoutId: string;
  size?: "sm" | "md";
}) {
  return (
    <div
      className={`relative grid rounded-full border border-outline-variant bg-surface-c-high p-0.5 ${disabled ? "opacity-40" : ""}`}
      style={{ gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))` }}
    >
      {options.map((o) => {
        const active = o.value === value;
        const pending = pendingValue === o.value;
        return (
          <button
            key={o.value}
            disabled={disabled}
            onClick={() => onChange(o.value)}
            className={`relative z-10 flex min-w-0 items-center justify-center gap-1 rounded-full font-semibold transition-colors ${
              size === "sm" ? "px-1 py-1 text-[10.5px]" : "px-1.5 py-2 text-[11px]"
            } ${active ? "text-on-primary" : "text-on-surface-variant hover:text-on-surface"} ${pending ? "pulse-pending" : ""}`}
          >
            {active && (
              <motion.span
                layoutId={layoutId}
                transition={{ type: "spring", stiffness: 500, damping: 38 }}
                className="absolute inset-0 -z-10 rounded-full bg-primary"
              />
            )}
            {pending && !active && <span className="absolute inset-0 -z-10 rounded-full bg-primary/25" />}
            {o.icon}
            <span className="truncate">{o.label}</span>
          </button>
        );
      })}
    </div>
  );
}

export function Slider({
  value,
  min,
  max,
  step = 1,
  onChange,
  onCommit,
  disabled,
  vertical,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  onCommit?: (v: number) => void;
  disabled?: boolean;
  vertical?: boolean;
}) {
  const fill = ((value - min) / (max - min)) * 100;
  return (
    <input
      type="range"
      className={vertical ? "m3 m3-vertical" : "m3"}
      min={min}
      max={max}
      step={step}
      value={value}
      disabled={disabled}
      style={{ ["--fill" as string]: `${fill}%` }}
      onChange={(e) => onChange(Number(e.target.value))}
      onMouseUp={(e) => onCommit?.(Number((e.target as HTMLInputElement).value))}
      onTouchEnd={(e) => onCommit?.(Number((e.target as HTMLInputElement).value))}
      onKeyUp={(e) => onCommit?.(Number((e.target as HTMLInputElement).value))}
    />
  );
}

export function Modal({
  open,
  title,
  children,
  onClose,
  actions,
}: {
  open: boolean;
  title: string;
  children: ReactNode;
  onClose: () => void;
  actions: ReactNode;
}) {
  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="absolute inset-0 z-40 flex items-center justify-center bg-black/60 p-5"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={onClose}
        >
          <motion.div
            initial={{ scale: 0.92, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.95, opacity: 0 }}
            className="w-full rounded-3xl border border-outline-variant bg-surface-c-high p-4 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="mb-2 text-sm font-semibold">{title}</h3>
            <div className="mb-4 text-xs leading-relaxed text-on-surface-variant">{children}</div>
            <div className="flex justify-end gap-2">{actions}</div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

export function Row({ title, subtitle, right, icon }: { title: string; subtitle?: string; right: ReactNode; icon?: ReactNode }) {
  return (
    <div className="flex items-center gap-3 py-2">
      {icon && <span className="text-on-surface-variant">{icon}</span>}
      <div className="min-w-0 flex-1">
        <div className="text-[12.5px] font-medium leading-tight">{title}</div>
        {subtitle && <div className="mt-0.5 text-[10.5px] leading-snug text-on-surface-variant">{subtitle}</div>}
      </div>
      {right}
    </div>
  );
}

/** Bipolar vertical slider (EQ band): fill grows from the 0 dB line towards the value. */
export function VSlider({
  value,
  min,
  max,
  step = 1,
  onChange,
  onCommit,
  disabled,
  height = 120,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  onCommit?: (v: number) => void;
  disabled?: boolean;
  height?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const last = useRef(value);
  const pct = (v: number) => ((v - min) / (max - min)) * 100;
  const zero = pct(Math.min(Math.max(0, min), max));
  const cur = pct(value);
  const fromEvent = (clientY: number) => {
    const r = ref.current!.getBoundingClientRect();
    const t = 1 - Math.min(1, Math.max(0, (clientY - r.top) / r.height));
    const raw = min + t * (max - min);
    return Math.round(raw / step) * step;
  };
  const set = (v: number) => {
    const c = Math.min(max, Math.max(min, Number(v.toFixed(2))));
    if (c !== last.current) {
      last.current = c;
      onChange(c);
    }
  };
  return (
    <div
      ref={ref}
      role="slider"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      tabIndex={disabled ? -1 : 0}
      className={`relative w-7 touch-none outline-none ${disabled ? "opacity-40" : "cursor-grab"}`}
      style={{ height }}
      onPointerDown={(e) => {
        if (disabled) return;
        (e.target as Element).setPointerCapture?.(e.pointerId);
        dragging.current = true;
        last.current = value;
        set(fromEvent(e.clientY));
      }}
      onPointerMove={(e) => dragging.current && set(fromEvent(e.clientY))}
      onPointerUp={() => {
        if (!dragging.current) return;
        dragging.current = false;
        onCommit?.(last.current);
      }}
      onKeyDown={(e) => {
        if (disabled) return;
        if (e.key === "ArrowUp" || e.key === "ArrowDown") {
          e.preventDefault();
          const v = Math.min(max, Math.max(min, value + (e.key === "ArrowUp" ? step : -step)));
          last.current = v;
          onChange(v);
          onCommit?.(v);
        }
      }}
    >
      <div className="absolute inset-y-0 left-1/2 w-1.5 -translate-x-1/2 rounded-full bg-surface-c-highest" />
      <div className="absolute left-1/2 h-px w-4 -translate-x-1/2 bg-outline" style={{ bottom: `${zero}%` }} />
      <div
        className="absolute left-1/2 w-1.5 -translate-x-1/2 rounded-full bg-primary"
        style={{ bottom: `${Math.min(zero, cur)}%`, height: `${Math.abs(cur - zero)}%` }}
      />
      <div
        className="absolute left-1/2 h-4 w-4 -translate-x-1/2 translate-y-1/2 rounded-full border-[3px] border-primary bg-on-surface shadow"
        style={{ bottom: `${cur}%` }}
      />
    </div>
  );
}

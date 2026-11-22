import { motion } from "framer-motion";
import type { ReactNode } from "react";
import { Zap, Ear, Package } from "lucide-react";
import type { BudBattery, DeviceState, WearPosition } from "@/types/earbuds";

function ringColor(level: number | null): string {
  if (level === null) return "rgb(var(--md-outline))";
  if (level <= 15) return "rgb(var(--md-danger))";
  if (level <= 30) return "rgb(var(--md-warning))";
  return "rgb(var(--md-primary))";
}

function ago(ts: number): string {
  const min = Math.max(0, Math.round((Date.now() - ts) / 60000));
  if (min < 1) return "только что";
  if (min < 60) return `${min} мин назад`;
  const h = Math.round(min / 60);
  return h < 24 ? `${h} ч назад` : `${Math.round(h / 24)} дн назад`;
}

export function Ring({ battery, label, size = 78, stale }: { battery: BudBattery; label: string; size?: number; stale?: boolean }) {
  const stroke = 7;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const lvl = battery.level ?? 0;
  return (
    <div className="relative" style={{ width: size, height: size }}>
      <svg width={size} height={size} className="-rotate-90">
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="rgb(var(--md-surface-container-highest))" strokeWidth={stroke} />
        <motion.circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke={ringColor(battery.level)}
          strokeOpacity={stale ? 0.45 : 1}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={c}
          initial={{ strokeDashoffset: c }}
          animate={{ strokeDashoffset: battery.level === null ? c : c * (1 - lvl / 100) }}
          transition={{ type: "spring", stiffness: 60, damping: 16 }}
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        <span className="flex items-center font-display text-[17px] font-bold leading-none">
          {battery.level === null ? "—" : battery.level}
          {battery.level !== null && <span className="text-[10px] font-medium text-on-surface-variant">%</span>}
        </span>
        <span className="mt-0.5 text-[10px] text-on-surface-variant">{label}</span>
      </div>
      {battery.charging && (
        <motion.span
          initial={{ scale: 0 }}
          animate={{ scale: 1 }}
          className="absolute -right-0.5 top-0.5 flex h-5 w-5 items-center justify-center rounded-full bg-success text-black shadow"
        >
          <Zap size={11} fill="currentColor" />
        </motion.span>
      )}
    </div>
  );
}

function WearBadge({ w }: { w: WearPosition }) {
  if (w === "unknown") return <span className="h-[18px]" />;
  const map: Record<Exclude<WearPosition, "unknown">, { t: string; cls: string; icon: ReactNode }> = {
    ear: { t: "В ухе", cls: "bg-secondary-container text-on-secondary-container", icon: <Ear size={10} /> },
    case: { t: "В кейсе", cls: "bg-surface-c-highest text-on-surface-variant", icon: <Package size={10} /> },
    out: { t: "Снят", cls: "bg-surface-c-highest text-on-surface-variant", icon: <Ear size={10} /> },
  };
  const m = map[w];
  return <span className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[9.5px] font-semibold ${m.cls}`}>{m.icon}{m.t}</span>;
}

export default function BatteryRings({ device }: { device: DeviceState }) {
  const b = device.battery;
  const caseStale = b.caseCachedAt !== null;
  return (
    <section className="card">
      <div className="grid grid-cols-3 place-items-center gap-1 pt-1">
        <div className="flex flex-col items-center gap-1.5">
          <Ring battery={b.left} label="Левый" />
          <WearBadge w={device.wear.left} />
        </div>
        <div className="flex flex-col items-center gap-1.5">
          <Ring battery={b.case} label="Кейс" stale={caseStale} />
          {caseStale && b.caseCachedAt ? (
            <span className="text-[9.5px] text-on-surface-variant">{ago(b.caseCachedAt)}</span>
          ) : (
            <span className="h-[18px]" />
          )}
        </div>
        <div className="flex flex-col items-center gap-1.5">
          <Ring battery={b.right} label="Правый" />
          <WearBadge w={device.wear.right} />
        </div>
      </div>
    </section>
  );
}

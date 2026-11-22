import { Gamepad2, Waves, Orbit, Link2 } from "lucide-react";
import type { ReactNode } from "react";
import { Segmented, Switch } from "./ui";
import { isPending, useEarbudsStore } from "@/store/useEarbudsStore";
import type { DeviceState } from "@/types/earbuds";

/** Compact toggle tile (half width). Title/subtitle are always bottom-aligned so a row of tiles lines up. */
function Tile({
  icon,
  title,
  subtitle,
  active,
  pending,
  onClick,
  wide,
}: {
  icon: ReactNode;
  title: string;
  subtitle: string;
  active: boolean;
  pending?: boolean;
  onClick: () => void;
  wide?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex h-[84px] flex-col justify-between rounded-[1.25rem] border p-2.5 text-left transition-colors ${wide ? "col-span-2" : ""} ${
        active ? "border-transparent bg-primary-container text-on-primary-container" : "border-outline-variant bg-surface-c hover:bg-surface-c-high"
      } ${pending ? "pulse-pending" : ""}`}
    >
      <span className={`flex h-7 w-7 items-center justify-center rounded-xl ${active ? "bg-primary text-on-primary" : "bg-surface-c-highest text-on-surface-variant"}`}>
        {icon}
      </span>
      <span className="block min-w-0">
        <span className="block truncate text-[12px] font-semibold leading-tight">{title}</span>
        <span className="block truncate text-[10px] leading-tight opacity-70">{subtitle}</span>
      </span>
    </button>
  );
}

/** Full-width card: header row (icon · title · switch) + a control row underneath. */
function WideCard({
  icon,
  title,
  subtitle,
  active,
  pending,
  right,
  children,
}: {
  icon: ReactNode;
  title: string;
  subtitle: string;
  active: boolean;
  pending?: boolean;
  right?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div
      className={`col-span-2 flex flex-col gap-2 rounded-[1.25rem] border p-2.5 transition-colors ${
        active ? "border-transparent bg-primary-container text-on-primary-container" : "border-outline-variant bg-surface-c"
      } ${pending ? "pulse-pending" : ""}`}
    >
      <div className="flex items-center gap-2.5">
        <span className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-xl ${active ? "bg-primary text-on-primary" : "bg-surface-c-highest text-on-surface-variant"}`}>
          {icon}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[12px] font-semibold leading-tight">{title}</span>
          <span className="block truncate text-[10px] leading-tight opacity-70">{subtitle}</span>
        </span>
        {right}
      </div>
      {children}
    </div>
  );
}

/** Discrete level picker (Ultra Bass 1…5 / BassWave −5…+5): one pill per step. */
function LevelPicker({ min, max, value, off, onPick }: { min: number; max: number; value: number | null; off?: boolean; onPick: (v: number) => void }) {
  const levels = Array.from({ length: max - min + 1 }, (_, i) => min + i);
  return (
    <div className="grid gap-1" style={{ gridTemplateColumns: `repeat(${levels.length}, minmax(0, 1fr))` }}>
      {levels.map((l) => {
        const on = !off && value !== null && (min < 0 ? (l === 0 ? value === 0 : Math.sign(l) === Math.sign(value) && Math.abs(l) <= Math.abs(value)) : l <= value);
        const exact = !off && l === value;
        return (
          <button
            key={l}
            onClick={() => onPick(l)}
            title={off ? "Включить на этом уровне" : undefined}
            className={`h-6 rounded-full font-mono text-[10px] font-semibold transition-colors ${
              exact ? "bg-primary text-on-primary" : on ? "bg-primary/35 text-on-surface" : "bg-surface-c-highest text-on-surface-variant hover:bg-surface-c-high"
            }`}
          >
            {min < 0 && l > 0 ? `+${l}` : l}
          </button>
        );
      })}
    </div>
  );
}

export default function QuickTilesGrid({ device }: { device: DeviceState }) {
  const send = useEarbudsStore((s) => s.send);
  const caps = device.caps;
  const bassCap = caps.bass;
  const spatialModes = caps.spatialModes;

  const small: ReactNode[] = [];
  if (caps.lowLatency) {
    small.push(
      <Tile
        key="ll"
        icon={<Gamepad2 size={15} />}
        title="Игровой режим"
        subtitle={device.lowLatency ? "Низкая задержка" : "Обычная задержка"}
        active={!!device.lowLatency}
        pending={isPending(device.pending, "lowLatency")}
        onClick={() => send({ type: "setLowLatency", enabled: !device.lowLatency })}
      />,
    );
  }
  if (caps.multipoint) {
    small.push(
      <Tile
        key="mp"
        icon={<Link2 size={15} />}
        title="Два устройства"
        subtitle={device.multipoint ? "Включено" : "Выключено"}
        active={!!device.multipoint}
        pending={isPending(device.pending, "multipoint")}
        onClick={() => send({ type: "setMultipoint", enabled: !device.multipoint })}
      />,
    );
  }
  if (spatialModes.length === 2) {
    const on = !!device.spatial && device.spatial !== spatialModes[0];
    small.push(
      <Tile
        key="sp"
        icon={<Orbit size={15} />}
        title="Пространственный звук"
        subtitle={on ? "Включено" : "Выключено"}
        active={on}
        pending={isPending(device.pending, "spatial")}
        onClick={() => send({ type: "setSpatial", mode: on ? spatialModes[0] : spatialModes[1] })}
      />,
    );
  }

  if (!small.length && !bassCap && spatialModes.length < 3) return null;

  return (
    <section className="grid grid-cols-2 gap-2">
      {small.map((t, i) => (i === small.length - 1 && small.length % 2 === 1 ? <div key="last" className="col-span-2 grid">{t}</div> : t))}

      {spatialModes.length > 2 && (
        <WideCard
          icon={<Orbit size={15} />}
          title="Пространственный звук"
          subtitle={device.spatial ?? "—"}
          active={!!device.spatial && device.spatial !== spatialModes[0]}
          pending={isPending(device.pending, "spatial")}
        >
          <Segmented
            size="sm"
            layoutId="spatial"
            options={spatialModes.map((m) => ({ value: m, label: m }))}
            value={device.spatial}
            onChange={(m) => m !== device.spatial && send({ type: "setSpatial", mode: m })}
          />
        </WideCard>
      )}

      {bassCap && (
        <WideCard
          icon={<Waves size={15} />}
          title={bassCap.kind === "bassWave" ? "BassWave™" : "Ultra Bass"}
          subtitle={device.bass.enabled ? `Усиление баса · уровень ${device.bass.level ?? "—"}` : "Усиление баса выключено"}
          active={!!device.bass.enabled}
          pending={isPending(device.pending, "bass")}
          right={
            <Switch
              label="Усиление баса"
              checked={!!device.bass.enabled}
              onChange={(v) => send({ type: "setBass", enabled: v, level: device.bass.level ?? (bassCap.min < 0 ? 0 : bassCap.min) })}
            />
          }
        >
          <LevelPicker
            min={bassCap.min}
            max={bassCap.max}
            value={device.bass.level}
            off={!device.bass.enabled}
            onPick={(v) => (v !== device.bass.level || !device.bass.enabled) && send({ type: "setBass", enabled: true, level: v })}
          />
        </WideCard>
      )}
    </section>
  );
}

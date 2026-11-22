import { AudioLines, Ear, VolumeX, Lock, Sparkles } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { Segmented, Switch } from "./ui";
import { isPending, useEarbudsStore } from "@/store/useEarbudsStore";
import type { AncLevel, AncMode, DeviceState } from "@/types/earbuds";

const LEVEL_LABEL: Record<AncLevel, string> = {
  high: "Макс",
  mid: "Средний",
  low: "Слабый",
  smart: "Умный",
  adaptive: "Адаптив",
};

const LEVEL_SHORT: Record<AncLevel, string> = {
  high: "Макс",
  mid: "Средн.",
  low: "Слаб.",
  smart: "Умный",
  adaptive: "Адапт.",
};

export default function NoiseControlCard({ device }: { device: DeviceState }) {
  const send = useEarbudsStore((s) => s.send);
  const caps = device.caps;
  const connected = device.connection === "connected";
  const pending = isPending(device.pending, "anc");
  const bothOut =
    caps.wearRequiredForAnc &&
    device.wear.left !== "ear" &&
    device.wear.right !== "ear" &&
    device.wear.left !== "unknown" &&
    device.wear.right !== "unknown";
  const locked = !connected || bothOut;

  const modes = [
    { value: "anc" as AncMode, label: "Шумодав", icon: <AudioLines size={12} /> },
    ...(caps.transparency ? [{ value: "transparency" as AncMode, label: "Прозрачность", icon: <Ear size={12} /> }] : []),
    { value: "off" as AncMode, label: "Выкл", icon: <VolumeX size={12} /> },
  ];

  const setMode = (mode: AncMode) => {
    if (mode === device.anc.mode) return;
    send({ type: "setAnc", mode, level: mode === "anc" ? (device.anc.level ?? caps.ancLevels[0] ?? null) : null });
  };

  return (
    <section className="card">
      <div className="card-title">
        Шумоподавление
        {pending && <span className="ml-auto text-[10px] normal-case tracking-normal text-primary pulse-pending">ожидание ACK…</span>}
      </div>
      <Segmented
        layoutId="anc-mode"
        options={modes}
        value={pending ? null : device.anc.mode}
        pendingValue={null}
        disabled={locked}
        onChange={setMode}
      />
      <AnimatePresence initial={false}>
        {device.anc.mode === "anc" && caps.ancLevels.length > 0 && (
          <motion.div initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0 }} className="overflow-hidden">
            <div className="pt-2">
              <Segmented
                size="sm"
                layoutId="anc-level"
                options={caps.ancLevels.map((l) => ({
                  value: l,
                  label: caps.ancLevels.length > 4 ? LEVEL_SHORT[l] : LEVEL_LABEL[l],
                  icon: caps.ancLevels.length <= 4 && (l === "adaptive" || l === "smart") ? <Sparkles size={11} /> : undefined,
                }))}
                value={pending ? null : device.anc.level}
                disabled={locked}
                onChange={(level) => send({ type: "setAnc", mode: "anc", level })}
              />
            </div>
          </motion.div>
        )}
      </AnimatePresence>
      {caps.personalizedAnc && (
        <div className="mt-2 flex items-center justify-between gap-2 px-1">
          <span className="text-[11px] text-on-surface-variant">Персонализированное ANC</span>
          <Switch
            checked={!!device.personalizedAnc}
            pending={isPending(device.pending, "personalizedAnc")}
            disabled={!connected}
            onChange={(v) => send({ type: "setPersonalizedAnc", enabled: v })}
          />
        </div>
      )}
      {bothOut && connected && (
        <div className="mt-2 flex items-center gap-1.5 rounded-xl bg-warning/15 px-2 py-1.5 text-[10.5px] text-warning">
          <Lock size={12} /> Наденьте наушник для переключения шумоподавления
        </div>
      )}
    </section>
  );
}

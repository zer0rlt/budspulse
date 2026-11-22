import { useState } from "react";
import { Ear } from "lucide-react";
import { Row, Segmented, Switch } from "./ui";
import { isPending, useEarbudsStore } from "@/store/useEarbudsStore";
import type { DeviceState, Side } from "@/types/earbuds";

/** Tab is only shown when the connected model exposes gestures or an in-ear sensor toggle. */
export function hasGesturesTab(device: DeviceState) {
  return device.caps.gestures.length > 0 || device.caps.inEarDetection;
}

export default function GesturesTab({ device }: { device: DeviceState }) {
  const send = useEarbudsStore((s) => s.send);
  const caps = device.caps;
  const [side, setSide] = useState<Side>("left");
  const slots = caps.gestures.filter((g) => g.side === side);
  const binding = (gesture: string) => device.gestures.find((g) => g.side === side && g.gesture === gesture)?.action;

  return (
    <>
      {caps.gestures.length > 0 && (
        <section className="card">
          <div className="card-title">Жесты сенсоров</div>
          <Segmented
            size="sm"
            layoutId="gesture-side"
            value={side}
            onChange={setSide}
            options={[
              { value: "left", label: "Левый наушник" },
              { value: "right", label: "Правый наушник" },
            ]}
          />
          <div className="mt-1 divide-y divide-outline-variant">
            {slots.map((g) => {
              const val = binding(g.gesture);
              const pend = isPending(device.pending, `gesture:${side}:${g.gesture}`);
              return (
                <label key={g.gesture} className={`flex items-center gap-2 py-2 ${pend ? "pulse-pending" : ""}`}>
                  <span className="min-w-0 flex-1 text-[12px] font-medium leading-tight">{g.label}</span>
                  <select
                    className="select !w-[178px] shrink-0"
                    value={val ?? ""}
                    onChange={(e) => send({ type: "setGesture", side, gesture: g.gesture, action: Number(e.target.value) })}
                  >
                    {val === undefined && <option value="">—</option>}
                    {caps.gestureActions.map((act) => (
                      <option key={act.id} value={act.id}>
                        {act.name}
                      </option>
                    ))}
                  </select>
                </label>
              );
            })}
          </div>
        </section>
      )}

      {caps.inEarDetection && (
        <section className="card !py-1">
          <Row
            icon={<Ear size={16} />}
            title="Датчик ношения"
            subtitle="Наушники сами ставят музыку на паузу, когда их снимают"
            right={
              <Switch
                checked={!!device.inEarDetection}
                pending={isPending(device.pending, "inEarDetection")}
                onChange={(v) => send({ type: "setInEarDetection", enabled: v })}
              />
            }
          />
        </section>
      )}
    </>
  );
}

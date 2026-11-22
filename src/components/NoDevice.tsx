import { Bluetooth, FlaskConical } from "lucide-react";
import { useEarbudsStore } from "@/store/useEarbudsStore";
import { UnpairButton } from "./UnpairDialog";

export default function NoDevice() {
  const snap = useEarbudsStore((s) => s.snap);
  const { connect, setDemo } = useEarbudsStore();
  if (!snap) return null;
  return (
    <div className="flex flex-col gap-3 pt-2">
      <div className="card flex flex-col items-center gap-2 py-6 text-center">
        <Bluetooth size={34} className="text-primary" />
        <div className="text-sm font-semibold">Нет подключённых наушников</div>
        <div className="px-4 text-[11px] text-on-surface-variant">
          Сначала сопрягите наушники Nothing, CMF или OnePlus в настройках Bluetooth системы, затем выберите их ниже.
        </div>
      </div>
      {snap.paired.length > 0 && (
        <div className="card">
          <div className="card-title">Сопряжённые</div>
          {snap.paired.map((p) => (
            <div key={p.id} className="flex items-center gap-1 rounded-xl hover:bg-surface-c-high">
              <button className="flex min-w-0 flex-1 items-center gap-2 px-1 py-2 text-left" onClick={() => connect(p.id)}>
                <Bluetooth size={14} className={p.connected ? "text-success" : "text-on-surface-variant"} />
                <span className="flex-1 truncate text-xs">{p.name}</span>
                <span className="text-[10px] text-on-surface-variant">{p.model ?? ""}</span>
              </button>
              <UnpairButton device={p} />
            </div>
          ))}
        </div>
      )}
      <div className="card">
        <div className="card-title">
          <FlaskConical size={13} /> Демо-режим
        </div>
        <div className="flex flex-wrap gap-1.5">
          {snap.demoModels.map((m) => (
            <button key={m.key} className="btn-tonal" onClick={() => setDemo(true, m.key)}>
              {m.name}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

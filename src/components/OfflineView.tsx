import { Bluetooth, BluetoothSearching, FlaskConical, Loader2, RefreshCw, TriangleAlert } from "lucide-react";
import { useEarbudsStore } from "@/store/useEarbudsStore";
import { isTauri } from "@/lib/backend";
import type { DeviceState } from "@/types/earbuds";

/**
 * Shown on the dashboard while the selected earbuds are not reachable.
 * BudsPulse only opens the control channel once the OS reports a Bluetooth link,
 * so "disconnected" here means "Windows/BlueZ says the earbuds are not connected to this PC".
 */
export default function OfflineView({ device }: { device: DeviceState }) {
  const snap = useEarbudsStore((s) => s.snap)!;
  const { connect, setDemo, backend } = useEarbudsStore();
  const setError = (e: unknown) => useEarbudsStore.setState({ error: String(e) });
  const others = snap.paired.filter((p) => p.id !== device.id);
  const c = device.connection;
  const connecting = c === "connecting" || c === "handshake";

  const openBt = () => backend?.openBluetoothSettings().catch(setError);

  return (
    <div className="flex flex-col gap-3 pt-1">
      <section className="card flex flex-col items-center gap-2 px-4 py-6 text-center">
        <span
          className={`flex h-14 w-14 items-center justify-center rounded-3xl ${
            c === "error" ? "bg-danger/15 text-danger" : connecting ? "bg-primary/15 text-primary" : "bg-surface-c-highest text-on-surface-variant"
          }`}
        >
          {connecting ? <Loader2 size={26} className="animate-spin" /> : c === "error" ? <TriangleAlert size={26} /> : <BluetoothSearching size={26} />}
        </span>
        <div className="text-[14px] font-semibold">
          {connecting ? "Подключаемся к наушникам…" : c === "error" ? "Нет связи с наушниками" : "Наушники не подключены к ПК"}
        </div>
        <div className="text-[11px] leading-relaxed text-on-surface-variant">
          {connecting ? (
            <>Bluetooth-соединение есть, открываем канал управления {device.name}.</>
          ) : c === "error" ? (
            <>
              Bluetooth-соединение есть, но канал управления не открылся.
              {device.lastError && <span className="mt-1 block font-mono text-[10px] text-danger">{device.lastError}</span>}
              <span className="mt-1 block">Если наушники подключены и к телефону, закройте на нём Nothing X / HeyMelody.</span>
            </>
          ) : (
            <>
              Достаньте {device.name} из кейса или подключите в настройках Bluetooth. BudsPulse подхватит их автоматически, как только
              система установит соединение.
            </>
          )}
        </div>
        <div className="mt-1 flex gap-2">
          {isTauri && (
            <button className="btn-tonal" onClick={openBt}>
              <Bluetooth size={13} /> Настройки Bluetooth
            </button>
          )}
          {c === "error" && (
            <button className="btn-outline" onClick={() => connect(device.id)}>
              <RefreshCw size={13} /> Повторить
            </button>
          )}
        </div>
      </section>

      {others.length > 0 && (
        <section className="card">
          <div className="card-title">Другие наушники</div>
          {others.map((p) => (
            <button
              key={p.id}
              className="flex w-full items-center gap-2 rounded-xl px-1 py-2 text-left hover:bg-surface-c-high"
              onClick={() => connect(p.id)}
            >
              <Bluetooth size={14} className={p.connected ? "text-success" : "text-on-surface-variant"} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-xs font-medium">{p.name}</span>
                <span className="block truncate text-[10px] text-on-surface-variant">{p.connected ? "подключены к ПК" : "не подключены"}</span>
              </span>
            </button>
          ))}
        </section>
      )}

      <section className="card">
        <div className="card-title">
          <FlaskConical size={13} /> Посмотреть без наушников
        </div>
        <div className="flex flex-wrap gap-1.5">
          {snap.demoModels.map((m) => (
            <button key={m.key} className="chip" onClick={() => setDemo(true, m.key)}>
              {m.name}
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}

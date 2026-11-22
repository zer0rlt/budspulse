import { useEffect, useRef, useState } from "react";
import { BellRing, Cpu, Eye, EyeOff, FileDown, Ruler, Trash2, Terminal, LogOut, RefreshCw, FlaskConical, Pause, Mic, Gamepad2, MessageSquare, Power, ChevronDown } from "lucide-react";
import { Modal, Row, Segmented, Switch } from "./ui";
import { isPending, useEarbudsStore } from "@/store/useEarbudsStore";
import { isTauri } from "@/lib/backend";
import type { DeviceState, FitResult, Side, SimAction } from "@/types/earbuds";

function FitBadge({ r, label }: { r: FitResult; label: string }) {
  const txt = r === null ? "—" : r === "good" ? "Хорошее прилегание" : "Подберите амбушюры";
  const cls = r === null ? "bg-surface-c-highest text-on-surface-variant" : r === "good" ? "bg-success/20 text-success" : "bg-warning/20 text-warning";
  return (
    <div className="flex flex-1 flex-col items-center gap-1">
      <span className="text-[10px] text-on-surface-variant">{label}</span>
      <span className={`rounded-full px-2 py-1 text-[10.5px] font-semibold ${cls}`}>{txt}</span>
    </div>
  );
}

function redactMac(mac: string) {
  const p = mac.split(":");
  return p.length === 6 ? `${p[0]}:${p[1]}:••:••:••:${p[5]}` : "••••";
}

export default function SettingsTab({ device }: { device: DeviceState | null }) {
  const { send, clearPackets, backend, setError } = {
    ...useEarbudsStore(),
    setError: (e: string) => useEarbudsStore.setState({ error: e }),
  };
  const packets = useEarbudsStore((s) => s.packets);
  const settings = useEarbudsStore((s) => s.snap!.settings);
  const updateSettings = useEarbudsStore((s) => s.updateSettings);
  const askUnpair = useEarbudsStore((s) => s.askUnpair);
  const pairedSelf = useEarbudsStore((s) => (s.snap!.demo || !device ? undefined : s.snap!.paired.find((p) => p.id === device.id)));
  const a = settings.automation;
  const connected = device?.connection === "connected";
  const [inspector, setInspector] = useState(false);
  const [findSide, setFindSide] = useState<Side | null>(null);
  const [hiResAsk, setHiResAsk] = useState<string | null>(null);
  const [showMac, setShowMac] = useState(false);
  const [exported, setExported] = useState<string | null>(null);
  const logRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  useEffect(() => {
    if (autoScroll && logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [packets, autoScroll]);

  return (
    <>
      {device && connected && (
        <>
      {device.caps.fitTest && (
        <section className="card">
          <div className="card-title">
            <Ruler size={13} /> Тест прилегания амбушюр
          </div>
          <div className="mb-2 flex gap-2">
            <FitBadge r={device.fit.left} label="Левый" />
            <FitBadge r={device.fit.right} label="Правый" />
          </div>
          <button className={`btn-filled w-full ${device.fit.running ? "pulse-pending" : ""}`} disabled={device.fit.running} onClick={() => send({ type: "startFitTest" })}>
            {device.fit.running ? "Идёт тест… вставьте оба наушника" : "Начать тест"}
          </button>
        </section>
      )}

      {device.caps.findBuds && (
        <section className="card">
          <div className="card-title">
            <BellRing size={13} /> Найти наушники
          </div>
          <div className="flex gap-2">
            {(["left", "right"] as Side[]).map((side) => {
              const ringing = device.findBuds[side];
              return (
                <button
                  key={side}
                  
                  className={`${ringing ? "btn-filled pulse-pending" : "btn-tonal"} flex-1`}
                  onClick={() => (ringing ? send({ type: "findBuds", side, enabled: false }) : setFindSide(side))}
                >
                  {ringing ? "Стоп" : side === "left" ? "Левый" : "Правый"}
                </button>
              );
            })}
          </div>
        </section>
      )}

      <section className="card">
        <div className="card-title">
          <Cpu size={13} /> Кодек и прошивка
        </div>
        {device.caps.hiResOptions.length > 0 && (
          <div className={`mb-2 ${isPending(device.pending, "hiRes") ? "pulse-pending" : ""}`}>
            <Segmented
              size="sm"
              layoutId="hires"
              options={device.caps.hiResOptions.map((o) => ({ value: o, label: o }))}
              value={device.hiRes}
              
              onChange={(v) => v !== device.hiRes && setHiResAsk(v)}
            />
          </div>
        )}
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[11px]">
          <dt className="text-on-surface-variant">Модель</dt>
          <dd className="truncate">{device.model} <span className="font-mono text-on-surface-variant">{device.modelCode}</span></dd>
          <dt className="text-on-surface-variant">Кодек (ОС)</dt>
          <dd>{device.codec ?? "—"}</dd>
          <dt className="text-on-surface-variant">Прошивка</dt>
          <dd className="font-mono">{device.firmware ?? "—"}</dd>
          <dt className="text-on-surface-variant">Серийный №</dt>
          <dd className="truncate font-mono">{device.serial ?? "—"}</dd>
          <dt className="text-on-surface-variant">MAC</dt>
          <dd className="flex items-center gap-1 font-mono">
            {showMac ? device.address : redactMac(device.address)}
            <button onClick={() => setShowMac((s) => !s)} className="text-on-surface-variant">
              {showMac ? <EyeOff size={12} /> : <Eye size={12} />}
            </button>
          </dd>
          <dt className="text-on-surface-variant">Транспорт</dt>
          <dd className="uppercase">{device.transport ?? "—"}</dd>
        </dl>
        <button className="btn-outline mt-2 w-full !py-1"  onClick={() => send({ type: "refresh" })}>
          <RefreshCw size={12} /> Перечитать состояние
        </button>
        {device.lastError && <div className="mt-2 rounded-xl bg-danger/15 px-2 py-1.5 text-[10.5px] text-danger">{device.lastError}</div>}
      </section>

      {device.transport === "simulator" && (
        <section className="card">
          <div className="card-title">
            <FlaskConical size={13} /> Симулятор: физический мир
          </div>
          <div className="grid grid-cols-2 gap-1.5">
            {(
              [
                ["toggleWearLeft", device.wear.left === "ear" ? "Снять левый" : "Надеть левый"],
                ["toggleWearRight", device.wear.right === "ear" ? "Снять правый" : "Надеть правый"],
                ["toggleCharging", "Зарядка вкл/выкл"],
                ["closeCase", "Убрать в кейс и закрыть"],
              ] as [SimAction, string][]
            ).map(([a, label]) => (
              <button key={a} className="btn-outline !py-1.5" onClick={() => backend?.simAction(a).catch((e) => setError(String(e)))}>
                {label}
              </button>
            ))}
          </div>
        </section>
      )}

        </>
      )}

      <section className="card !py-1">
        <div className="card-title !mb-0 pt-2">Автоматизация ПК</div>
        <Row
          icon={<Pause size={16} />}
          title="Автопауза при снятии наушника"
          subtitle="Windows Media / MPRIS — продолжит, если вернуть в течение 3 минут"
          right={<Switch checked={a.autoPause} onChange={(v) => updateSettings({ automation: { ...a, autoPause: v } })} />}
        />
        <Row
          icon={<Mic size={16} />}
          title="Прозрачность во время звонка"
          subtitle="Включается, пока микрофон занят (Discord, Telegram, Zoom)"
          right={<Switch checked={a.autoTransparencyOnMic} onChange={(v) => updateSettings({ automation: { ...a, autoTransparencyOnMic: v } })} />}
        />
        <Row
          icon={<Gamepad2 size={16} />}
          title="Игровой режим в играх"
          subtitle="Низкая задержка для полноэкранных приложений и Steam"
          right={<Switch checked={a.autoLowLatencyInGames} onChange={(v) => updateSettings({ automation: { ...a, autoLowLatencyInGames: v } })} />}
        />
      </section>

      <section className="card !py-1">
        <div className="card-title !mb-0 pt-2">Приложение</div>
        <Row
          icon={<Power size={16} />}
          title="Запускать вместе с системой"
          subtitle="Стартует свёрнутым в трей"
          right={<Switch checked={settings.autostart} onChange={(v) => updateSettings({ autostart: v })} />}
        />
        <Row
          icon={<MessageSquare size={16} />}
          title="Карточка при подключении"
          subtitle="Заряд наушников и кейса в углу экрана"
          right={<Switch checked={a.connectionPopup} onChange={(v) => updateSettings({ automation: { ...a, connectionPopup: v } })} />}
        />
      </section>

      <section className="card">
        <div className="card-title !mb-0">
          <button className="flex flex-1 items-center gap-1.5 text-left" onClick={() => setInspector((v) => !v)}>
            <Terminal size={13} /> Инспектор пакетов
            <ChevronDown size={13} className={`transition ${inspector ? "rotate-180" : ""}`} />
          </button>
          <span className="ml-auto flex gap-1 normal-case tracking-normal">
            <button className="rounded-full p-1 hover:bg-surface-c-high" title="Очистить" onClick={() => clearPackets()}>
              <Trash2 size={12} />
            </button>
            <button
              className="rounded-full p-1 hover:bg-surface-c-high"
              title="Экспорт диагностики JSON"
              onClick={async () => {
                try {
                  const path = await backend?.exportDiagnostics();
                  setExported(path ?? null);
                } catch (e) {
                  setError(String(e));
                }
              }}
            >
              <FileDown size={12} />
            </button>
          </span>
        </div>
        {inspector && (
        <div
          ref={logRef}
          onScroll={(e) => {
            const el = e.currentTarget;
            setAutoScroll(el.scrollHeight - el.scrollTop - el.clientHeight < 20);
          }}
          className="scroll-thin mt-2 h-48 select-text overflow-y-auto rounded-xl bg-surface-dim p-2 font-mono text-[9.5px] leading-[1.35]"
        >
          {packets.length === 0 && <div className="text-on-surface-variant">Пакетов пока нет.</div>}
          {packets.map((p, i) => (
            <div key={i} className="mb-1">
              <span className="text-on-surface-variant">{new Date(p.ts).toLocaleTimeString("ru-RU", { hour12: false })}.{String(p.ts % 1000).padStart(3, "0")} </span>
              <span className={p.dir === "tx" ? "font-bold text-primary" : "font-bold text-success"}>[{p.dir.toUpperCase()}]</span>{" "}
              <span className="text-on-surface">{p.desc}</span>
              <div className="break-all text-on-surface-variant">{p.hex}</div>
            </div>
          ))}
        </div>
        )}
        {exported && <div className="mt-1 break-all text-[10px] text-success">Сохранено: {exported}</div>}
      </section>

      {pairedSelf && (
        <button className="btn-outline text-danger" onClick={() => askUnpair(pairedSelf)}>
          <Trash2 size={13} /> Удалить {pairedSelf.name} из сопряжённых
        </button>
      )}

      {isTauri && (
        <button className="btn-outline mb-1" onClick={() => backend?.quit()}>
          <LogOut size={13} /> Выйти из BudsPulse
        </button>
      )}

      <Modal
        open={findSide !== null}
        title="Найти наушник"
        onClose={() => setFindSide(null)}
        actions={
          <>
            <button className="btn-outline" onClick={() => setFindSide(null)}>
              Отмена
            </button>
            <button
              className="btn-filled"
              onClick={() => {
                if (findSide) send({ type: "findBuds", side: findSide, enabled: true });
                setFindSide(null);
              }}
            >
              Воспроизвести звук
            </button>
          </>
        }
      >
        Наушник издаст громкий высокочастотный сигнал. <b>Обязательно выньте наушники из ушей</b>, прежде чем продолжить.
      </Modal>

      <Modal
        open={hiResAsk !== null}
        title="Сменить Hi-Res кодек?"
        onClose={() => setHiResAsk(null)}
        actions={
          <>
            <button className="btn-outline" onClick={() => setHiResAsk(null)}>
              Отмена
            </button>
            <button
              className="btn-filled"
              onClick={() => {
                if (hiResAsk) send({ type: "setHiRes", value: hiResAsk });
                setHiResAsk(null);
              }}
            >
              Переключить
            </button>
          </>
        }
      >
        Переключение на <b>{hiResAsk}</b> приведёт к перезагрузке/переподключению наушников. Звук прервётся на несколько секунд.
      </Modal>
    </>
  );
}

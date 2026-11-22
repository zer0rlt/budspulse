import { useState } from "react";
import { Bluetooth, ChevronDown, FlaskConical, Link2, Minus, RefreshCw, Power, Unplug } from "lucide-react";
import { AnimatePresence, motion } from "framer-motion";
import { useEarbudsStore } from "@/store/useEarbudsStore";
import { isTauri } from "@/lib/backend";
import { UnpairButton } from "./UnpairDialog";
import type { ThemeId } from "@/types/earbuds";

const THEMES: { id: ThemeId | null; label: string }[] = [
  { id: null, label: "Авто" },
  { id: "oneplus", label: "OnePlus" },
  { id: "nothing", label: "Nothing" },
  { id: "cmf", label: "CMF" },
];

const CONN_LABEL: Record<string, string> = {
  disconnected: "Отключено",
  connecting: "Подключение…",
  handshake: "Рукопожатие…",
  connected: "Подключено",
  error: "Ошибка",
};

export default function HeaderBar() {
  const snap = useEarbudsStore((s) => s.snap);
  const { setDemo, connect, disconnect, rescan, updateSettings } = useEarbudsStore();
  const backend = useEarbudsStore((s) => s.backend);
  const [menu, setMenu] = useState(false);
  const device = snap?.device;

  return (
    <header className="relative z-20 px-3 pb-2 pt-3" data-tauri-drag-region>
      <div className="flex items-center gap-2" data-tauri-drag-region>
        <button onClick={() => setMenu((m) => !m)} className="flex min-w-0 flex-1 items-center gap-2 text-left">
          <span
            className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-2xl ${
              device?.connection === "connected" ? "bg-primary text-on-primary" : "bg-surface-c-highest text-on-surface-variant"
            }`}
          >
            {snap?.demo ? <FlaskConical size={17} /> : <Bluetooth size={17} />}
          </span>
          <span className="min-w-0">
            <span className="flex items-center gap-1 truncate font-display text-[15px] font-semibold leading-tight">
              <span className="truncate">{device?.name ?? "BudsPulse"}</span>
              <ChevronDown size={14} className={`shrink-0 transition ${menu ? "rotate-180" : ""}`} />
            </span>
            <span className="flex items-center gap-1.5 whitespace-nowrap text-[10.5px] text-on-surface-variant">
              {device ? (
                <>
                  <span
                    className={`h-1.5 w-1.5 rounded-full ${
                      device.connection === "connected"
                        ? "bg-success"
                        : device.connection === "error"
                          ? "bg-danger"
                          : device.connection === "disconnected"
                            ? "bg-outline"
                            : "bg-warning pulse-pending"
                    }`}
                  />
                  {CONN_LABEL[device.connection]}
                  {device.transport && <span className="uppercase">· {device.transport === "simulator" ? "демо" : device.transport}</span>}
                  {device.modelCode && <span className="font-mono opacity-70">· {device.modelCode}</span>}
                </>
              ) : (
                "Наушники не выбраны"
              )}
            </span>
          </span>
        </button>
        <div className="flex shrink-0 items-center gap-1">
          {device?.codec && <span className="chip chip-active">{device.codec}</span>}
          {device?.multipoint && (
            <span className="chip" title="Двойное подключение активно">
              <Link2 size={11} />
            </span>
          )}
          {isTauri && (
            <button className="rounded-full p-1.5 text-on-surface-variant hover:bg-surface-c-high" onClick={() => backend?.hideWindow()} title="Свернуть">
              <Minus size={15} />
            </button>
          )}
        </div>
      </div>

      <AnimatePresence>
        {menu && snap && (
          <motion.div
            initial={{ opacity: 0, y: -6, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -6, scale: 0.98 }}
            className="absolute inset-x-3 top-[58px] rounded-3xl border border-outline-variant bg-surface-c-high p-2 shadow-2xl"
          >
            <div className="flex items-center justify-between px-2 pb-1 pt-1">
              <span className="card-title !mb-0">Сопряжённые устройства</span>
              <button className="rounded-full p-1 text-on-surface-variant hover:bg-surface-c-highest" onClick={() => rescan()} title="Обновить">
                <RefreshCw size={13} />
              </button>
            </div>
            <div className="max-h-40 overflow-y-auto scroll-thin">
              {snap.paired.length === 0 && (
                <div className="px-2 py-2 text-[11px] text-on-surface-variant">Поддерживаемые наушники не найдены среди сопряжённых.</div>
              )}
              {snap.paired.map((p) => {
                const active = !snap.demo && device?.id === p.id;
                return (
                  <div key={p.id} className={`flex items-center gap-1 rounded-2xl pr-1 hover:bg-surface-c-highest ${active ? "bg-secondary-container" : ""}`}>
                  <button
                    onClick={() => {
                      if (active) disconnect();
                      else connect(p.id);
                      setMenu(false);
                    }}
                    className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left"
                  >
                    <Bluetooth size={14} className={p.connected ? "text-success" : "text-on-surface-variant"} />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-xs font-medium">{p.name}</span>
                      <span className="block truncate text-[10px] text-on-surface-variant">
                        {p.model ?? "Неизвестная модель"} · {p.connected ? "подключено к ПК" : "не подключено"}
                      </span>
                    </span>
                    {active && <Unplug size={13} className="text-on-surface-variant" />}
                  </button>
                  <UnpairButton
                    device={p}
                    className="!p-1"
                  />
                  </div>
                );
              })}
            </div>

            <div className="mt-1 border-t border-outline-variant px-2 pt-2">
              <span className="card-title">Демо-режим (симулятор)</span>
              <div className="flex flex-wrap gap-1">
                {snap.demoModels.map((m) => {
                  const active = snap.demo && snap.settings.demoModel === m.key;
                  return (
                    <button
                      key={m.key}
                      className={`chip ${active ? "chip-active" : ""}`}
                      onClick={() => {
                        setDemo(true, m.key);
                        setMenu(false);
                      }}
                    >
                      {m.name}
                    </button>
                  );
                })}
                {snap.demo && (
                  <button className="chip" onClick={() => setDemo(false)}>
                    <Power size={11} /> Выкл
                  </button>
                )}
              </div>
            </div>

            <div className="mt-2 border-t border-outline-variant px-2 pt-2">
              <span className="card-title">Тема</span>
              <div className="flex flex-wrap gap-1 pb-1">
                {THEMES.map((t) => (
                  <button
                    key={t.label}
                    className={`chip ${snap.settings.themeOverride === t.id ? "chip-active" : ""}`}
                    onClick={() => updateSettings({ themeOverride: t.id })}
                  >
                    {t.label}
                  </button>
                ))}
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </header>
  );
}

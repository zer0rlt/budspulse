import { useEffect } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { Gauge, SlidersHorizontal, Hand, Settings2, X } from "lucide-react";
import { useEarbudsStore, useTheme, type TabId } from "./store/useEarbudsStore";
import { isTauri } from "./lib/backend";
import HeaderBar from "./components/HeaderBar";
import BatteryRings from "./components/BatteryRings";
import NoiseControlCard from "./components/NoiseControlCard";
import QuickTilesGrid from "./components/QuickTilesGrid";
import EqualizerTab from "./components/EqualizerTab";
import GesturesTab, { hasGesturesTab } from "./components/GesturesTab";
import SettingsTab from "./components/SettingsTab";
import NoDevice from "./components/NoDevice";
import OfflineView from "./components/OfflineView";
import UnpairDialog from "./components/UnpairDialog";

const TABS: { id: TabId; label: string; icon: typeof Gauge }[] = [
  { id: "dashboard", label: "Пульт", icon: Gauge },
  { id: "sound", label: "Звук", icon: SlidersHorizontal },
  { id: "gestures", label: "Жесты", icon: Hand },
  { id: "device", label: "Настройки", icon: Settings2 },
];

export default function App() {
  const init = useEarbudsStore((s) => s.init);
  const snap = useEarbudsStore((s) => s.snap);
  const tab = useEarbudsStore((s) => s.tab);
  const setTab = useEarbudsStore((s) => s.setTab);
  const error = useEarbudsStore((s) => s.error);
  const dismissError = useEarbudsStore((s) => s.dismissError);
  const theme = useTheme();
  const device = snap?.device ?? null;
  const connected = device?.connection === "connected";
  // only tabs that have something to show right now
  const tabs = TABS.filter(({ id }) => id === "dashboard" || id === "device" || (id === "sound" && !!device) || (id === "gestures" && connected && hasGesturesTab(device!)));
  const current: TabId = tabs.some((t) => t.id === tab) ? tab : "dashboard";

  useEffect(() => {
    init();
  }, [init]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  useEffect(() => {
    if (!isTauri) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") useEarbudsStore.getState().backend?.hideWindow();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div
      className={isTauri ? "h-full w-full p-0" : "h-[620px] w-[380px]"}
      onContextMenu={(e) => isTauri && e.preventDefault()}
    >
      <div className={`shell ${theme === "nothing" ? "dot-matrix" : ""}`}>
        <HeaderBar />
        <main className="scroll-thin flex-1 overflow-y-auto overflow-x-hidden px-3 pb-3">
          {!snap ? (
            <div className="flex h-full items-center justify-center text-sm text-on-surface-variant">Загрузка…</div>
          ) : (
            // plain CSS enter animation: never blocks mounting (framer's exit-wait could leave the
            // tab blank when the popover was hidden mid-transition and WebView2 paused rAF)
            <div key={current} className="tab-enter flex flex-col gap-3 pt-1">
                {current === "dashboard" &&
                  (!device ? (
                    <NoDevice />
                  ) : connected ? (
                    <>
                      <BatteryRings device={device} />
                      <NoiseControlCard device={device} />
                      <QuickTilesGrid device={device} />
                    </>
                  ) : (
                    <OfflineView device={device} />
                  ))}
                {current === "sound" && device && <EqualizerTab device={device} />}
                {current === "gestures" && device && <GesturesTab device={device} />}
                {current === "device" && <SettingsTab device={device} />}
            </div>
          )}
        </main>

        <UnpairDialog />

        <AnimatePresence>
          {error && (
            <motion.div
              initial={{ y: 40, opacity: 0 }}
              animate={{ y: 0, opacity: 1 }}
              exit={{ y: 40, opacity: 0 }}
              className="absolute inset-x-3 bottom-20 z-30 flex items-start gap-2 rounded-2xl bg-danger/90 px-3 py-2 text-xs text-white shadow-xl"
            >
              <span className="flex-1 break-words">{error}</span>
              <button onClick={dismissError} aria-label="Закрыть">
                <X size={14} />
              </button>
            </motion.div>
          )}
        </AnimatePresence>

        <nav className="grid gap-1 border-t border-outline-variant bg-surface-c px-2 pb-2 pt-1.5" style={{ gridTemplateColumns: `repeat(${tabs.length}, minmax(0, 1fr))` }}>
          {tabs.map(({ id, label, icon: Icon }) => {
            const active = current === id;
            return (
              <button key={id} onClick={() => setTab(id)} className="group flex flex-col items-center gap-0.5">
                <span
                  className={`flex h-7 w-14 items-center justify-center rounded-full transition-colors ${
                    active ? "bg-secondary-container text-on-secondary-container" : "text-on-surface-variant group-hover:bg-surface-c-high"
                  }`}
                >
                  <Icon size={17} strokeWidth={active ? 2.4 : 1.9} />
                </span>
                <span className={`text-[10.5px] ${active ? "font-semibold text-on-surface" : "text-on-surface-variant"}`}>{label}</span>
              </button>
            );
          })}
        </nav>
      </div>
    </div>
  );
}

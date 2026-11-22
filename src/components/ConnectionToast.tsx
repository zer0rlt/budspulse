import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { Bluetooth } from "lucide-react";
import { Ring } from "./BatteryRings";
import { getBackend, isTauri } from "@/lib/backend";
import type { ToastPayload } from "@/types/earbuds";

const SHOW_MS = 6000;

export default function ConnectionToast() {
  const [toast, setToast] = useState<ToastPayload | null>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let hideTimer: number | undefined;
    getBackend().then(async (b) => {
      unlisten = await b.onToast((t) => {
        document.documentElement.dataset.theme = t.device.vendor;
        setToast(t);
        setVisible(true);
        window.clearTimeout(hideTimer);
        hideTimer = window.setTimeout(() => setVisible(false), SHOW_MS);
      });
    });
    return () => {
      unlisten?.();
      window.clearTimeout(hideTimer);
    };
  }, []);

  useEffect(() => {
    if (!visible && toast && isTauri) {
      const t = window.setTimeout(async () => {
        const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
        getCurrentWebviewWindow().hide();
      }, 400);
      return () => window.clearTimeout(t);
    }
  }, [visible, toast]);

  const d = toast?.device;
  return (
    <div className="flex h-full w-full items-start justify-end p-2">
      <AnimatePresence>
        {visible && d && (
          <motion.div
            initial={{ x: 380, opacity: 0 }}
            animate={{ x: 0, opacity: 1 }}
            exit={{ x: 380, opacity: 0 }}
            transition={{ type: "spring", stiffness: 260, damping: 28 }}
            className="flex w-full items-center gap-3 rounded-[1.5rem] border border-outline-variant bg-surface/95 p-3 text-on-surface shadow-2xl"
            onClick={async () => {
              const b = await getBackend();
              await b.hideWindow().catch(() => undefined);
              setVisible(false);
            }}
          >
            <div className="flex w-[72px] shrink-0 flex-col items-center gap-1">
              <span className="flex h-11 w-11 items-center justify-center rounded-2xl bg-primary text-on-primary">
                <Bluetooth size={22} />
              </span>
              <span className="text-center text-[10px] leading-tight text-on-surface-variant">{toast.reason === "connected" ? "Подключено" : "Кейс открыт"}</span>
            </div>
            <div className="min-w-0 flex-1">
              <div className="mb-1 flex items-center gap-1.5">
                <span className="truncate font-display text-[13px] font-semibold">{d.name}</span>
                {d.codec && <span className="chip chip-active !px-1.5 !py-0 !text-[9px]">{d.codec}</span>}
              </div>
              <div className="flex justify-between">
                <Ring battery={d.battery.left} label="L" size={56} />
                <Ring battery={d.battery.case} label="Кейс" size={56} stale={d.battery.caseCachedAt !== null} />
                <Ring battery={d.battery.right} label="R" size={56} />
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

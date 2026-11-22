import { useState } from "react";
import { Loader2, Trash2 } from "lucide-react";
import { Modal } from "./ui";
import { useEarbudsStore } from "@/store/useEarbudsStore";
import type { PairedDevice } from "@/types/earbuds";

/** Small trash button for a paired-device row: asks for confirmation via {@link UnpairDialog}. */
export function UnpairButton({ device, className = "" }: { device: PairedDevice; className?: string }) {
  const askUnpair = useEarbudsStore((s) => s.askUnpair);
  return (
    <button
      className={`shrink-0 rounded-full p-1.5 text-on-surface-variant hover:bg-danger/15 hover:text-danger ${className}`}
      title="Удалить из сопряжённых"
      aria-label={`Удалить ${device.name} из сопряжённых`}
      onClick={(e) => {
        e.stopPropagation();
        askUnpair(device);
      }}
    >
      <Trash2 size={13} />
    </button>
  );
}

/** Confirmation dialog for removing a device from the OS list of paired Bluetooth devices. */
export default function UnpairDialog() {
  const target = useEarbudsStore((s) => s.unpairAsk);
  const askUnpair = useEarbudsStore((s) => s.askUnpair);
  const unpair = useEarbudsStore((s) => s.unpair);
  const [busy, setBusy] = useState(false);
  const close = () => !busy && askUnpair(null);

  return (
    <Modal
      open={target !== null}
      title="Удалить из сопряжённых?"
      onClose={close}
      actions={
        <>
          <button className="btn-outline" disabled={busy} onClick={close}>
            Отмена
          </button>
          <button
            className="btn-filled !bg-danger !text-white"
            disabled={busy}
            onClick={async () => {
              if (!target) return;
              setBusy(true);
              await unpair(target.id);
              setBusy(false);
              askUnpair(null);
            }}
          >
            {busy ? <Loader2 size={13} className="animate-spin" /> : <Trash2 size={13} />} Удалить
          </button>
        </>
      }
    >
      <b>{target?.name}</b> будет удалено из списка Bluetooth-устройств системы и отключится от ПК. Чтобы пользоваться им снова, наушники
      придётся заново сопрячь: переведите их в режим сопряжения (обычно — зажать кнопку на кейсе) и добавьте в настройках Bluetooth.
    </Modal>
  );
}

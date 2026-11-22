import { create } from "zustand";
import { getBackend, type Backend } from "@/lib/backend";
import type {
  DeviceCommand,
  HostEqProfile,
  HostEqStatus,
  PacketLog,
  PairedDevice,
  Settings,
  Snapshot,
  ThemeId,
} from "@/types/earbuds";

export type TabId = "dashboard" | "sound" | "gestures" | "device";

const MAX_PACKETS = 600;

interface EarbudsStore {
  backend: Backend | null;
  snap: Snapshot | null;
  packets: PacketLog[];
  tab: TabId;
  error: string | null;
  initialized: boolean;
  /** Paired device the user is about to remove (confirmation dialog open). */
  unpairAsk: PairedDevice | null;

  init(): Promise<void>;
  setTab(t: TabId): void;
  send(cmd: DeviceCommand): Promise<void>;
  connect(id: string): Promise<void>;
  disconnect(): Promise<void>;
  unpair(id: string): Promise<void>;
  askUnpair(device: PairedDevice | null): void;
  rescan(): Promise<void>;
  setDemo(enabled: boolean, model?: string | null): Promise<void>;
  updateSettings(patch: Partial<Settings>): Promise<void>;
  setHostEq(profile: HostEqProfile): Promise<HostEqStatus | null>;
  installHostEq(): Promise<void>;
  clearPackets(): Promise<void>;
  dismissError(): void;
}

let listenersBound = false;

export const useEarbudsStore = create<EarbudsStore>((set, get) => ({
  backend: null,
  snap: null,
  packets: [],
  tab: "dashboard",
  error: null,
  initialized: false,
  unpairAsk: null,

  async init() {
    if (get().initialized) return;
    set({ initialized: true });
    const backend = await getBackend();
    set({ backend });
    if (!listenersBound) {
      listenersBound = true;
      await backend.onSnapshot((snap) => set({ snap }));
      await backend.onNavigate((tab) => set({ tab: tab as TabId }));
      await backend.onPacket((p) =>
        set((s) => {
          const next = s.packets.length >= MAX_PACKETS ? s.packets.slice(-MAX_PACKETS + 1) : s.packets.slice();
          next.push(p);
          return { packets: next };
        }),
      );
    }
    const [snap, packets] = await Promise.all([backend.getSnapshot(), backend.getPackets()]);
    set({ snap, packets });
  },

  setTab: (tab) => set({ tab }),

  async send(cmd) {
    const b = get().backend;
    if (!b) return;
    try {
      await b.sendCommand(cmd);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  async connect(id) {
    try {
      await get().backend?.connect(id);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  async disconnect() {
    await get().backend?.disconnect();
  },

  async unpair(id) {
    try {
      await get().backend?.unpair(id);
    } catch (e) {
      set({ error: `Не удалось удалить устройство: ${e}` });
    }
  },

  askUnpair: (device) => set({ unpairAsk: device }),

  async rescan() {
    try {
      await get().backend?.rescan();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  async setDemo(enabled, model = null) {
    try {
      await get().backend?.setDemo(enabled, model);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  async updateSettings(patch) {
    const snap = get().snap;
    if (!snap) return;
    const settings: Settings = {
      ...snap.settings,
      ...patch,
      automation: { ...snap.settings.automation, ...(patch.automation ?? {}) },
    };
    set({ snap: { ...snap, settings } });
    try {
      await get().backend?.setSettings(settings);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  async setHostEq(profile) {
    const snap = get().snap;
    if (snap) set({ snap: { ...snap, hostEq: profile } });
    try {
      const status = (await get().backend?.setHostEq(profile)) ?? null;
      const s2 = get().snap;
      if (status && s2) set({ snap: { ...s2, hostEqStatus: status } });
      return status;
    } catch (e) {
      set({ error: String(e) });
      return null;
    }
  },

  async installHostEq() {
    try {
      const status = await get().backend?.installHostEq();
      const s2 = get().snap;
      if (status && s2) set({ snap: { ...s2, hostEqStatus: status } });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  async clearPackets() {
    await get().backend?.clearPackets();
    set({ packets: [] });
  },

  dismissError: () => set({ error: null }),
}));

export function useTheme(): ThemeId {
  return useEarbudsStore((s) => s.snap?.settings.themeOverride ?? s.snap?.device?.vendor ?? "nothing");
}

export function useDevice() {
  return useEarbudsStore((s) => s.snap?.device ?? null);
}

export function isPending(keys: string[] | undefined, key: string): boolean {
  return !!keys && keys.includes(key);
}

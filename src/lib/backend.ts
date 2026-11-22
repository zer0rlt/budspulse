// Thin bridge between React and the Rust backend. When the UI is opened in a plain
// browser (npm run dev without Tauri) a self-contained in-memory mock is used so the
// layout can be developed without a native shell. Inside Tauri the real backend is used.
import type {
  DeviceCommand,
  HostEqProfile,
  HostEqStatus,
  PacketLog,
  Settings,
  SimAction,
  Snapshot,
  ToastPayload,
} from "@/types/earbuds";

type Unlisten = () => void;

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

async function tauriListen<T>(event: string, cb: (payload: T) => void): Promise<Unlisten> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => cb(e.payload));
}

export interface Backend {
  getSnapshot(): Promise<Snapshot>;
  sendCommand(cmd: DeviceCommand): Promise<void>;
  connect(id: string): Promise<void>;
  disconnect(): Promise<void>;
  unpair(id: string): Promise<void>;
  rescan(): Promise<void>;
  setDemo(enabled: boolean, model: string | null): Promise<void>;
  setSettings(settings: Settings): Promise<void>;
  setHostEq(profile: HostEqProfile): Promise<HostEqStatus>;
  installHostEq(): Promise<HostEqStatus>;
  parsePeqText(text: string): Promise<HostEqProfile>;
  exportPeqText(profile: HostEqProfile): Promise<string>;
  getPackets(): Promise<PacketLog[]>;
  clearPackets(): Promise<void>;
  exportDiagnostics(): Promise<string>;
  hideWindow(): Promise<void>;
  simAction(action: SimAction): Promise<void>;
  onNavigate(cb: (tab: string) => void): Promise<Unlisten>;
  quit(): Promise<void>;
  openBluetoothSettings(): Promise<void>;
  onSnapshot(cb: (s: Snapshot) => void): Promise<Unlisten>;
  onPacket(cb: (p: PacketLog) => void): Promise<Unlisten>;
  onToast(cb: (t: ToastPayload) => void): Promise<Unlisten>;
}

const tauriBackend: Backend = {
  getSnapshot: () => tauriInvoke("get_snapshot"),
  sendCommand: (cmd) => tauriInvoke("send_command", { cmd }),
  connect: (id) => tauriInvoke("connect_device", { id }),
  disconnect: () => tauriInvoke("disconnect_device"),
  unpair: (id) => tauriInvoke("unpair_device", { id }),
  rescan: () => tauriInvoke("rescan_devices"),
  setDemo: (enabled, model) => tauriInvoke("set_demo", { enabled, model }),
  setSettings: (settings) => tauriInvoke("set_settings", { settings }),
  setHostEq: (profile) => tauriInvoke("set_host_eq", { profile }),
  installHostEq: () => tauriInvoke("install_host_eq"),
  parsePeqText: (text) => tauriInvoke("parse_peq_text", { text }),
  exportPeqText: (profile) => tauriInvoke("export_peq_text", { profile }),
  getPackets: () => tauriInvoke("get_packets"),
  clearPackets: () => tauriInvoke("clear_packets"),
  exportDiagnostics: () => tauriInvoke("export_diagnostics"),
  hideWindow: () => tauriInvoke("hide_popover"),
  simAction: (action) => tauriInvoke("sim_action", { action }),
  onNavigate: (cb) => tauriListen("navigate", cb),
  quit: () => tauriInvoke("quit_app"),
  openBluetoothSettings: () => tauriInvoke("open_bluetooth_settings"),
  onSnapshot: (cb) => tauriListen("snapshot", cb),
  onPacket: (cb) => tauriListen("packet", cb),
  onToast: (cb) => tauriListen("toast", cb),
};

let backendPromise: Promise<Backend> | null = null;

export function getBackend(): Promise<Backend> {
  if (!backendPromise) {
    backendPromise = isTauri
      ? Promise.resolve(tauriBackend)
      : import("./webMock").then((m) => m.createWebMock());
  }
  return backendPromise;
}

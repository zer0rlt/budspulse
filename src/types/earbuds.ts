// Unified contract shared with the Rust backend (src-tauri/src/state.rs + protocols/mod.rs).
// Field names are camelCase on both sides (serde rename_all = "camelCase").

export type Vendor = "nothing" | "cmf" | "oneplus";
export type ThemeId = Vendor;

export type ConnectionState = "disconnected" | "connecting" | "handshake" | "connected" | "error";
export type TransportKind = "rfcomm" | "ble" | "simulator";

export type AncMode = "off" | "transparency" | "anc";
export type AncLevel = "high" | "mid" | "low" | "smart" | "adaptive";
export type WearPosition = "ear" | "out" | "case" | "unknown";
export type Side = "left" | "right";
export type FitResult = "good" | "poor" | null;

export interface BudBattery {
  level: number | null; // 0..100
  charging: boolean;
}

export interface BatteryState {
  left: BudBattery;
  right: BudBattery;
  case: BudBattery;
  /** unix ms of the moment the case value was last really reported (set when value is cached/stale) */
  caseCachedAt: number | null;
}

export interface NamedOption {
  id: number;
  name: string;
}

export interface Capabilities {
  ancLevels: AncLevel[]; // empty => no strength selector
  transparency: boolean;
  eqPresets: NamedOption[];
  customEqBands: number[]; // centre frequencies of hardware custom EQ bands (empty => unsupported)
  customEqRange: [number, number]; // dB
  customEqPresetId: number | null; // preset id that activates custom EQ
  bass: { kind: "ultraBass" | "bassWave"; min: number; max: number } | null;
  spatialModes: string[]; // empty => unsupported
  lowLatency: boolean;
  inEarDetection: boolean;
  multipoint: boolean;
  fitTest: boolean;
  findBuds: boolean;
  gestures: GestureSlot[];
  gestureActions: NamedOption[];
  hiResOptions: string[]; // e.g. ["SBC/AAC","LDAC"]; empty => unsupported
  wearRequiredForAnc: boolean;
  personalizedAnc: boolean;
}

export interface GestureSlot {
  side: Side;
  gesture: string; // "double" | "triple" | "hold" | "doubleHold" | "single" | "slide"
  label: string;
}

export interface GestureBinding {
  side: Side;
  gesture: string;
  action: number;
}

export interface DeviceState {
  id: string; // MAC / simulator id
  name: string;
  vendor: Vendor;
  model: string; // human-readable model name
  modelCode: string; // e.g. B171, OPO-PRO4
  address: string;
  connection: ConnectionState;
  transport: TransportKind | null;
  lastError: string | null;

  battery: BatteryState;
  wear: { left: WearPosition; right: WearPosition };
  anc: { mode: AncMode | null; level: AncLevel | null };
  eq: { presetId: number | null; customGains: number[] };
  bass: { enabled: boolean | null; level: number | null };
  spatial: string | null;
  lowLatency: boolean | null;
  inEarDetection: boolean | null;
  multipoint: boolean | null;
  personalizedAnc: boolean | null;
  hiRes: string | null;
  codec: string | null; // active A2DP codec reported by OS
  firmware: string | null;
  serial: string | null;
  gestures: GestureBinding[];
  fit: { running: boolean; left: FitResult; right: FitResult };
  findBuds: { left: boolean; right: boolean };

  caps: Capabilities;
  /** feature keys currently waiting for hardware ACK (honest UI) */
  pending: string[];
}

export interface PairedDevice {
  id: string;
  name: string;
  address: string;
  vendor: Vendor | null;
  model: string | null;
  connected: boolean;
}

// ---------------- Host (PC) parametric EQ ----------------
export type FilterType = "PK" | "LSC" | "HSC" | "BP" | "NO";

export interface PeqBand {
  enabled: boolean;
  type: FilterType;
  freq: number;
  gain: number;
  q: number;
}

export interface HostEqProfile {
  name: string;
  enabled: boolean;
  preamp: number;
  autoPreamp: boolean;
  bands: PeqBand[];
}

export interface HostEqStatus {
  backend: "pipewire" | "equalizerApo" | "none";
  available: boolean;
  active: boolean;
  message: string;
  configPath: string | null;
}

// ---------------- Settings ----------------
export interface AutomationSettings {
  autoPause: boolean;
  autoTransparencyOnMic: boolean;
  autoLowLatencyInGames: boolean;
  connectionPopup: boolean;
}

export interface Settings {
  automation: AutomationSettings;
  themeOverride: ThemeId | null;
  autostart: boolean;
  demoModel: string | null; // simulator model key when demo mode is on
  eqMode: "hardware" | "host";
}

export interface PacketLog {
  ts: number;
  dir: "tx" | "rx";
  hex: string;
  desc: string;
}

export interface Snapshot {
  demo: boolean;
  demoModels: NamedKey[];
  paired: PairedDevice[];
  device: DeviceState | null;
  settings: Settings;
  hostEq: HostEqProfile;
  hostEqStatus: HostEqStatus;
  platform: "windows" | "linux" | "macos" | "unknown";
}

export interface NamedKey {
  key: string;
  name: string;
}

// ---------------- Commands sent to the backend ----------------
export type DeviceCommand =
  | { type: "setAnc"; mode: AncMode; level?: AncLevel | null }
  | { type: "setEqPreset"; id: number }
  | { type: "setCustomEq"; gains: number[] }
  | { type: "setBass"; enabled: boolean; level: number }
  | { type: "setSpatial"; mode: string }
  | { type: "setLowLatency"; enabled: boolean }
  | { type: "setInEarDetection"; enabled: boolean }
  | { type: "setMultipoint"; enabled: boolean }
  | { type: "setPersonalizedAnc"; enabled: boolean }
  | { type: "setHiRes"; value: string }
  | { type: "setGesture"; side: Side; gesture: string; action: number }
  | { type: "startFitTest" }
  | { type: "findBuds"; side: Side; enabled: boolean }
  | { type: "refresh" };

export type SimAction = "toggleWearLeft" | "toggleWearRight" | "closeCase" | "toggleCharging";

export interface ToastPayload {
  device: DeviceState;
  reason: "connected" | "caseOpened";
}

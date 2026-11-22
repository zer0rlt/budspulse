// Browser-only preview backend (used when the UI runs outside Tauri, e.g. `npm run dev`).
// The real hardware simulator lives in Rust (src-tauri/src/bt/simulator.rs) and speaks the
// actual wire protocols; this mock only mirrors state so the layout can be iterated quickly.
import type { Backend } from "./backend";
import type {
  Capabilities,
  DeviceCommand,
  DeviceState,
  HostEqProfile,
  HostEqStatus,
  PacketLog,
  Snapshot,
  ToastPayload,
} from "@/types/earbuds";
import { flatProfile } from "./biquad";

const nothingCaps: Capabilities = {
  ancLevels: ["high", "mid", "low", "adaptive"],
  transparency: true,
  eqPresets: [
    { id: 0, name: "Balanced" },
    { id: 3, name: "More Bass" },
    { id: 2, name: "More Treble" },
    { id: 1, name: "Voice" },
    { id: 5, name: "Custom" },
  ],
  customEqBands: [100, 1000, 10000],
  customEqRange: [-6, 6],
  customEqPresetId: 5,
  bass: { kind: "ultraBass", min: 1, max: 5 },
  spatialModes: [],
  lowLatency: true,
  inEarDetection: true,
  multipoint: true,
  fitTest: true,
  findBuds: true,
  gestures: [
    { side: "left", gesture: "triple", label: "Тройное сжатие" },
    { side: "left", gesture: "hold", label: "Сжать и удерживать" },
    { side: "right", gesture: "triple", label: "Тройное сжатие" },
    { side: "right", gesture: "hold", label: "Сжать и удерживать" },
  ],
  gestureActions: [
    { id: 1, name: "Нет действия" },
    { id: 8, name: "Предыдущий трек" },
    { id: 9, name: "Следующий трек" },
    { id: 11, name: "Голосовой ассистент" },
    { id: 18, name: "Громкость +" },
    { id: 19, name: "Громкость −" },
    { id: 10, name: "Шумодав / Прозрачность / Выкл" },
  ],
  hiResOptions: ["SBC/AAC", "LDAC"],
  wearRequiredForAnc: false,
  personalizedAnc: true,
};

const oneplusCaps: Capabilities = {
  ...nothingCaps,
  eqPresets: [
    { id: 0, name: "Balanced" },
    { id: 1, name: "Bold" },
    { id: 2, name: "Serenade" },
    { id: 3, name: "Bass" },
  ],
  customEqBands: [62, 250, 1000, 4000, 8000, 16000],
  customEqPresetId: null,
  bass: { kind: "bassWave", min: -5, max: 5 },
  spatialModes: ["Выкл", "Фиксированный", "Отслеживание головы"],
  hiResOptions: [],
  wearRequiredForAnc: true,
  personalizedAnc: false,
  fitTest: false,
};

function makeDevice(model: string): DeviceState {
  const op = model === "oneplus";
  return {
    id: `sim-${model}`,
    name: op ? "OnePlus Buds Pro 4" : "Nothing Ear",
    vendor: op ? "oneplus" : "nothing",
    model: op ? "OnePlus Buds Pro 4" : "Nothing Ear (2024)",
    modelCode: op ? "OPO-PRO4" : "B171",
    address: "2C:BE:EB:00:00:01",
    connection: "connected",
    transport: "simulator",
    lastError: null,
    battery: {
      left: { level: 86, charging: false },
      right: { level: 81, charging: false },
      case: { level: 64, charging: false },
      caseCachedAt: Date.now() - 12 * 60000,
    },
    wear: { left: "ear", right: "ear" },
    anc: { mode: "anc", level: "high" },
    eq: { presetId: 0, customGains: op ? [0, 0, 0, 0, 0, 0] : [0, 0, 0] },
    bass: { enabled: false, level: op ? 0 : 2 },
    spatial: op ? "Выкл" : null,
    lowLatency: false,
    inEarDetection: true,
    multipoint: true,
    personalizedAnc: false,
    hiRes: op ? null : "SBC/AAC",
    codec: op ? "LHDC 5.0" : "AAC",
    firmware: op ? "11.1.0.512" : "1.0.1.202",
    serial: "SH10237XXXXXX",
    gestures: [
      { side: "left", gesture: "triple", action: 8 },
      { side: "left", gesture: "hold", action: 10 },
      { side: "right", gesture: "triple", action: 9 },
      { side: "right", gesture: "hold", action: 10 },
    ],
    fit: { running: false, left: null, right: null },
    findBuds: { left: false, right: false },
    caps: op ? oneplusCaps : nothingCaps,
    pending: [],
  };
}

export function createWebMock(): Backend {
  let snap: Snapshot = {
    demo: true,
    demoModels: [
      { key: "nothing", name: "Nothing Ear (2024)" },
      { key: "oneplus", name: "OnePlus Buds Pro 4" },
    ],
    paired: [],
    device: makeDevice("nothing"),
    settings: {
      automation: { autoPause: true, autoTransparencyOnMic: false, autoLowLatencyInGames: false, connectionPopup: true },
      themeOverride: null,
      autostart: false,
      demoModel: "nothing",
      eqMode: "hardware",
    },
    hostEq: flatProfile(),
    hostEqStatus: { backend: "none", available: false, active: false, message: "Браузерный предпросмотр — аудио-движок недоступен", configPath: null },
    platform: "unknown",
  };
  const snapCbs: ((s: Snapshot) => void)[] = [];
  const pktCbs: ((p: PacketLog) => void)[] = [];
  const packets: PacketLog[] = [];
  const emit = () => snapCbs.forEach((cb) => cb(structuredClone(snap)));
  const log = (dir: "tx" | "rx", desc: string) => {
    const p: PacketLog = { ts: Date.now(), dir, desc, hex: "(web mock)" };
    packets.push(p);
    pktCbs.forEach((cb) => cb(p));
  };
  const patchDevice = (f: (d: DeviceState) => void) => {
    if (!snap.device) return;
    const d = structuredClone(snap.device);
    f(d);
    snap = { ...snap, device: d };
    emit();
  };
  const ack = (key: string, f: (d: DeviceState) => void) => {
    patchDevice((d) => d.pending.push(key));
    log("tx", `SET ${key}`);
    setTimeout(() => {
      patchDevice((d) => {
        f(d);
        d.pending = d.pending.filter((k) => k !== key);
      });
      log("rx", `ACK ${key}`);
    }, 220);
  };

  const apply = (c: DeviceCommand) => {
    switch (c.type) {
      case "setAnc":
        return ack("anc", (d) => {
          d.anc = { mode: c.mode, level: c.mode === "anc" ? (c.level ?? "high") : d.anc.level };
        });
      case "setEqPreset":
        return ack("eq", (d) => (d.eq.presetId = c.id));
      case "setCustomEq":
        return ack("customEq", (d) => {
          d.eq.customGains = c.gains;
          if (d.caps.customEqPresetId !== null) d.eq.presetId = d.caps.customEqPresetId;
        });
      case "setBass":
        return ack("bass", (d) => (d.bass = { enabled: c.enabled, level: c.level }));
      case "setSpatial":
        return ack("spatial", (d) => (d.spatial = c.mode));
      case "setLowLatency":
        return ack("lowLatency", (d) => (d.lowLatency = c.enabled));
      case "setInEarDetection":
        return ack("inEarDetection", (d) => (d.inEarDetection = c.enabled));
      case "setMultipoint":
        return ack("multipoint", (d) => (d.multipoint = c.enabled));
      case "setPersonalizedAnc":
        return ack("personalizedAnc", (d) => (d.personalizedAnc = c.enabled));
      case "setHiRes":
        return ack("hiRes", (d) => (d.hiRes = c.value));
      case "setGesture":
        return ack(`gesture:${c.side}:${c.gesture}`, (d) => {
          d.gestures = d.gestures.map((g) => (g.side === c.side && g.gesture === c.gesture ? { ...g, action: c.action } : g));
        });
      case "startFitTest":
        patchDevice((d) => (d.fit = { running: true, left: null, right: null }));
        setTimeout(() => patchDevice((d) => (d.fit = { running: false, left: "good", right: Math.random() > 0.5 ? "good" : "poor" })), 2500);
        return;
      case "findBuds":
        return patchDevice((d) => (d.findBuds[c.side] = c.enabled));
      case "refresh":
        return log("tx", "GET all");
    }
  };

  return {
    getSnapshot: async () => structuredClone(snap),
    sendCommand: async (c) => apply(c),
    connect: async () => undefined,
    disconnect: async () => {
      snap = { ...snap, device: null, demo: false };
      emit();
    },
    unpair: async (id) => {
      snap = { ...snap, paired: snap.paired.filter((p) => p.id !== id) };
      emit();
    },
    rescan: async () => undefined,
    setDemo: async (enabled, model) => {
      snap = {
        ...snap,
        demo: enabled,
        device: enabled ? makeDevice(model ?? "nothing") : null,
        settings: { ...snap.settings, demoModel: model },
      };
      emit();
    },
    setSettings: async (settings) => {
      snap = { ...snap, settings };
      emit();
    },
    setHostEq: async (profile: HostEqProfile): Promise<HostEqStatus> => {
      snap = { ...snap, hostEq: profile };
      return snap.hostEqStatus;
    },
    installHostEq: async () => snap.hostEqStatus,
    parsePeqText: async (text) => {
      const p = flatProfile("Imported");
      const bands = [...text.matchAll(/ON\s+(PK|LSC|HSC|BP|NO)\s+Fc\s+([\d.]+)\s*Hz(?:\s+Gain\s+(-?[\d.]+)\s*dB)?\s+Q\s+([\d.]+)/gi)].map((m) => ({
        enabled: true,
        type: m[1].toUpperCase() as HostEqProfile["bands"][number]["type"],
        freq: Number(m[2]),
        gain: Number(m[3] ?? 0),
        q: Number(m[4]),
      }));
      const pre = /Preamp:\s*(-?[\d.]+)/i.exec(text);
      return { ...p, bands: bands.length ? bands : p.bands, preamp: pre ? Number(pre[1]) : 0, autoPreamp: !pre };
    },
    exportPeqText: async (p) =>
      [`Preamp: ${p.preamp.toFixed(1)} dB`, ...p.bands.map((b, i) => `Filter ${i + 1}: ${b.enabled ? "ON" : "OFF"} ${b.type} Fc ${b.freq} Hz Gain ${b.gain.toFixed(1)} dB Q ${b.q.toFixed(2)}`)].join("\n"),
    getPackets: async () => packets.slice(),
    clearPackets: async () => {
      packets.length = 0;
    },
    exportDiagnostics: async () => "(браузерный режим — экспорт недоступен)",
    hideWindow: async () => undefined,
    simAction: async (a) => {
      patchDevice((d) => {
        if (a === "toggleWearLeft") d.wear.left = d.wear.left === "ear" ? "out" : "ear";
        if (a === "toggleWearRight") d.wear.right = d.wear.right === "ear" ? "out" : "ear";
        if (a === "toggleCharging") {
          d.battery.left.charging = !d.battery.left.charging;
          d.battery.right.charging = d.battery.left.charging;
        }
      });
    },
    onNavigate: async () => () => undefined,
    quit: async () => undefined,
    openBluetoothSettings: async () => undefined,
    onSnapshot: async (cb) => {
      snapCbs.push(cb);
      return () => snapCbs.splice(snapCbs.indexOf(cb), 1);
    },
    onPacket: async (cb) => {
      pktCbs.push(cb);
      return () => pktCbs.splice(pktCbs.indexOf(cb), 1);
    },
    onToast: async (_cb: (t: ToastPayload) => void) => () => undefined,
  };
}

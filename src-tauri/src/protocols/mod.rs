//! Protocol layer: vendor-neutral device state + the `EarbudDriver` trait.
//!
//! Drivers are *sans-IO*: they turn high level [`DeviceCommand`]s into wire frames and fold
//! incoming bytes into [`DeviceState`]. Transports (RFCOMM / BLE / simulator) only move bytes.
//! This makes every driver unit-testable against golden captures and lets the simulator
//! exercise the real encoders/decoders end-to-end.

pub mod nothing;
pub mod oneplus;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Vendor {
    Nothing,
    Cmf,
    Oneplus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Handshake,
    Connected,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportKind {
    Rfcomm,
    Ble,
    Simulator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AncMode {
    Off,
    Transparency,
    Anc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AncLevel {
    High,
    Mid,
    Low,
    /// firmware-chosen level (OnePlus "Smart")
    Smart,
    Adaptive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WearPosition {
    Ear,
    Out,
    Case,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitQuality {
    Good,
    Poor,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BudBattery {
    pub level: Option<u8>,
    pub charging: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BatteryState {
    pub left: BudBattery,
    pub right: BudBattery,
    pub case: BudBattery,
    /// unix ms of the last real case report when the shown value is a cached one
    pub case_cached_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedOption {
    pub id: u8,
    pub name: String,
}

pub fn opt(id: u8, name: &str) -> NamedOption {
    NamedOption { id, name: name.to_string() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BassKind {
    UltraBass,
    BassWave,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BassCap {
    pub kind: BassKind,
    pub min: i8,
    pub max: i8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GestureSlot {
    pub side: Side,
    pub gesture: String,
    pub label: String,
}

pub fn slot(side: Side, gesture: &str, label: &str) -> GestureSlot {
    GestureSlot { side, gesture: gesture.into(), label: label.into() }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GestureBinding {
    pub side: Side,
    pub gesture: String,
    pub action: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub anc_levels: Vec<AncLevel>,
    pub transparency: bool,
    pub eq_presets: Vec<NamedOption>,
    pub custom_eq_bands: Vec<u32>,
    pub custom_eq_range: (f32, f32),
    pub custom_eq_preset_id: Option<u8>,
    pub bass: Option<BassCap>,
    pub spatial_modes: Vec<String>,
    pub low_latency: bool,
    pub in_ear_detection: bool,
    pub multipoint: bool,
    pub fit_test: bool,
    pub find_buds: bool,
    pub gestures: Vec<GestureSlot>,
    pub gesture_actions: Vec<NamedOption>,
    pub hi_res_options: Vec<String>,
    pub wear_required_for_anc: bool,
    pub personalized_anc: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AncState {
    pub mode: Option<AncMode>,
    pub level: Option<AncLevel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EqState {
    pub preset_id: Option<u8>,
    pub custom_gains: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BassState {
    pub enabled: Option<bool>,
    pub level: Option<i8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WearState {
    pub left: WearPosition,
    pub right: WearPosition,
}

impl Default for WearState {
    fn default() -> Self {
        WearState { left: WearPosition::Unknown, right: WearPosition::Unknown }
    }
}

impl WearState {
    pub fn any_in_ear(&self) -> bool {
        self.left == WearPosition::Ear || self.right == WearPosition::Ear
    }
    pub fn known(&self) -> bool {
        self.left != WearPosition::Unknown || self.right != WearPosition::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FitState {
    pub running: bool,
    pub left: Option<FitQuality>,
    pub right: Option<FitQuality>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FindState {
    pub left: bool,
    pub right: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceState {
    pub id: String,
    pub name: String,
    pub vendor: Vendor,
    pub model: String,
    pub model_code: String,
    pub address: String,
    pub connection: ConnectionState,
    pub transport: Option<TransportKind>,
    pub last_error: Option<String>,

    pub battery: BatteryState,
    pub wear: WearState,
    pub anc: AncState,
    pub eq: EqState,
    pub bass: BassState,
    pub spatial: Option<String>,
    pub low_latency: Option<bool>,
    pub in_ear_detection: Option<bool>,
    pub multipoint: Option<bool>,
    pub personalized_anc: Option<bool>,
    pub hi_res: Option<String>,
    pub codec: Option<String>,
    pub firmware: Option<String>,
    pub serial: Option<String>,
    pub gestures: Vec<GestureBinding>,
    pub fit: FitState,
    pub find_buds: FindState,

    pub caps: Capabilities,
    pub pending: Vec<String>,
}

impl DeviceState {
    pub fn new(id: &str, name: &str, address: &str, model: &ModelInfo) -> Self {
        DeviceState {
            id: id.into(),
            name: name.into(),
            vendor: model.vendor,
            model: model.name.clone(),
            model_code: model.code.clone(),
            address: address.into(),
            connection: ConnectionState::Disconnected,
            transport: None,
            last_error: None,
            battery: BatteryState::default(),
            wear: WearState::default(),
            anc: AncState::default(),
            eq: EqState { preset_id: None, custom_gains: vec![0.0; model.caps.custom_eq_bands.len()] },
            bass: BassState::default(),
            spatial: None,
            low_latency: None,
            in_ear_detection: None,
            multipoint: None,
            personalized_anc: None,
            hi_res: None,
            codec: None,
            firmware: None,
            serial: None,
            gestures: vec![],
            fit: FitState::default(),
            find_buds: FindState::default(),
            caps: model.caps.clone(),
            pending: vec![],
        }
    }

    /// Switch model (e.g. after reading the real model id from the earbuds).
    pub fn set_model(&mut self, model: &ModelInfo) {
        self.vendor = model.vendor;
        self.model = model.name.clone();
        self.model_code = model.code.clone();
        self.caps = model.caps.clone();
        if self.eq.custom_gains.len() != model.caps.custom_eq_bands.len() {
            self.eq.custom_gains = vec![0.0; model.caps.custom_eq_bands.len()];
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DeviceCommand {
    SetAnc { mode: AncMode, level: Option<AncLevel> },
    SetEqPreset { id: u8 },
    SetCustomEq { gains: Vec<f32> },
    SetBass { enabled: bool, level: i8 },
    SetSpatial { mode: String },
    SetLowLatency { enabled: bool },
    SetInEarDetection { enabled: bool },
    SetMultipoint { enabled: bool },
    SetPersonalizedAnc { enabled: bool },
    SetHiRes { value: String },
    SetGesture { side: Side, gesture: String, action: u8 },
    StartFitTest,
    FindBuds { side: Side, enabled: bool },
    Refresh,
}

impl DeviceCommand {
    pub fn pending_key(&self) -> String {
        match self {
            DeviceCommand::SetAnc { .. } => "anc".into(),
            DeviceCommand::SetEqPreset { .. } => "eq".into(),
            DeviceCommand::SetCustomEq { .. } => "customEq".into(),
            DeviceCommand::SetBass { .. } => "bass".into(),
            DeviceCommand::SetSpatial { .. } => "spatial".into(),
            DeviceCommand::SetLowLatency { .. } => "lowLatency".into(),
            DeviceCommand::SetInEarDetection { .. } => "inEarDetection".into(),
            DeviceCommand::SetMultipoint { .. } => "multipoint".into(),
            DeviceCommand::SetPersonalizedAnc { .. } => "personalizedAnc".into(),
            DeviceCommand::SetHiRes { .. } => "hiRes".into(),
            DeviceCommand::SetGesture { side, gesture, .. } => {
                format!("gesture:{}:{}", if *side == Side::Left { "left" } else { "right" }, gesture)
            }
            DeviceCommand::StartFitTest => "fit".into(),
            DeviceCommand::FindBuds { side, .. } => format!("find:{}", if *side == Side::Left { "left" } else { "right" }),
            DeviceCommand::Refresh => "refresh".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    pub vendor: Vendor,
    pub code: String,
    pub name: String,
    pub caps: Capabilities,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ProtoError {
    #[error("функция не поддерживается этой моделью: {0}")]
    Unsupported(&'static str),
    #[error("недопустимое значение: {0}")]
    Invalid(String),
    #[error("наушники не надеты — смена режима шумоподавления невозможна")]
    NotWorn,
}

/// A frame to write, with a human readable description for the packet inspector.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub bytes: Vec<u8>,
    pub desc: String,
    /// pause after writing this frame (handshakes, BLE pacing)
    pub delay_after: Duration,
}

impl Outgoing {
    pub fn new(bytes: Vec<u8>, desc: impl Into<String>) -> Self {
        Outgoing { bytes, desc: desc.into(), delay_after: Duration::ZERO }
    }
    pub fn delay(mut self, ms: u64) -> Self {
        self.delay_after = Duration::from_millis(ms);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DriverEvent {
    /// a frame was decoded (for the packet inspector)
    Rx { bytes: Vec<u8>, desc: String },
    /// state changed and should be pushed to the UI
    Changed,
    /// session handshake finished; device is ready for queries/commands
    Ready,
    /// a command ACK / NACK
    Ack { key: String, ok: bool, message: Option<String> },
    /// wear sensors changed (for automation)
    Wear { before: WearState, after: WearState },
    /// the device asked us to send these frames (e.g. reply-to-push or follow-up reads)
    Send(Vec<Outgoing>),
}

/// Vendor protocol driver (sans-IO).
pub trait EarbudDriver: Send {
    fn vendor(&self) -> Vendor;
    /// Frames to send right after the transport opens (handshake). If empty, the session
    /// considers the device ready immediately.
    fn session_start(&mut self) -> Vec<Outgoing>;
    /// Whether the driver must see a reply before it is ready (otherwise Ready is implied
    /// after `session_start` frames are written).
    fn needs_handshake_reply(&self) -> bool {
        false
    }
    /// Queries issued once the session is ready.
    fn initial_queries(&mut self, state: &DeviceState) -> Vec<Outgoing>;
    /// Low-frequency polling (battery). Called every ~60 s.
    fn poll_queries(&mut self, state: &DeviceState) -> Vec<Outgoing>;
    /// Encode a user command. On success the driver registers a pending op that is
    /// resolved by `feed` (ACK) or expired by `tick`.
    fn encode(&mut self, cmd: &DeviceCommand, state: &mut DeviceState) -> Result<Vec<Outgoing>, ProtoError>;
    /// Feed raw bytes from the transport. May be called with partial frames.
    fn feed(&mut self, data: &[u8], state: &mut DeviceState) -> Vec<DriverEvent>;
    /// Called periodically (~250 ms) to expire pending operations.
    fn tick(&mut self, now: Instant, state: &mut DeviceState) -> Vec<DriverEvent>;
}

// ---------------------------------------------------------------------------------------------
// Honest-UI pending operations: state is only mutated once the hardware confirms.
// ---------------------------------------------------------------------------------------------

pub type Apply = Box<dyn FnOnce(&mut DeviceState) + Send>;

pub struct PendingOp {
    pub key: String,
    pub deadline: Instant,
    pub apply: Option<Apply>,
}

#[derive(Default)]
pub struct PendingTracker {
    ops: HashMap<String, PendingOp>,
}

pub const ACK_TIMEOUT: Duration = Duration::from_millis(2500);

impl PendingTracker {
    pub fn add(&mut self, state: &mut DeviceState, key: &str, apply: Apply) {
        self.ops.insert(key.to_string(), PendingOp { key: key.into(), deadline: Instant::now() + ACK_TIMEOUT, apply: Some(apply) });
        if !state.pending.iter().any(|k| k == key) {
            state.pending.push(key.to_string());
        }
    }

    pub fn add_with_timeout(&mut self, state: &mut DeviceState, key: &str, timeout: Duration, apply: Apply) {
        self.add(state, key, apply);
        if let Some(op) = self.ops.get_mut(key) {
            op.deadline = Instant::now() + timeout;
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.ops.contains_key(key)
    }

    /// Remove an op and hand back its intended change (caller decides whether to apply it).
    pub fn take(&mut self, state: &mut DeviceState, key: &str) -> Option<Apply> {
        let mut op = self.ops.remove(key)?;
        state.pending.retain(|k| k != key);
        op.apply.take()
    }

    /// Resolve with success: apply the intended change.
    pub fn confirm(&mut self, state: &mut DeviceState, key: &str) -> Option<DriverEvent> {
        let mut op = self.ops.remove(key)?;
        if let Some(f) = op.apply.take() {
            f(state);
        }
        state.pending.retain(|k| k != key);
        Some(DriverEvent::Ack { key: key.into(), ok: true, message: None })
    }

    /// Resolve with failure: drop the intended change.
    pub fn reject(&mut self, state: &mut DeviceState, key: &str, msg: &str) -> Option<DriverEvent> {
        self.ops.remove(key)?;
        state.pending.retain(|k| k != key);
        Some(DriverEvent::Ack { key: key.into(), ok: false, message: Some(msg.into()) })
    }

    /// Drop without event (e.g. device pushed the real state for this feature).
    pub fn forget(&mut self, state: &mut DeviceState, key: &str) {
        self.ops.remove(key);
        state.pending.retain(|k| k != key);
    }

    pub fn expire(&mut self, now: Instant, state: &mut DeviceState) -> Vec<DriverEvent> {
        let expired: Vec<String> = self.ops.values().filter(|o| o.deadline <= now).map(|o| o.key.clone()).collect();
        expired
            .into_iter()
            .filter_map(|k| self.reject(state, &k, "наушники не подтвердили команду (таймаут)"))
            .collect()
    }

    pub fn clear(&mut self, state: &mut DeviceState) {
        self.ops.clear();
        state.pending.clear();
    }

    pub fn keys(&self) -> Vec<String> {
        self.ops.keys().cloned().collect()
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

pub fn parse_hex(s: &str) -> Vec<u8> {
    s.split(|c: char| c.is_whitespace() || c == ':')
        .filter(|t| !t.is_empty())
        .map(|t| u8::from_str_radix(t, 16).expect("valid hex"))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Model registry
// ---------------------------------------------------------------------------------------------

pub fn all_models() -> Vec<ModelInfo> {
    let mut v = nothing::models();
    v.extend(oneplus::models());
    v
}

pub fn model_by_code(code: &str) -> Option<ModelInfo> {
    all_models().into_iter().find(|m| m.code.eq_ignore_ascii_case(code))
}

/// Best-effort identification from the Bluetooth name (refined later from the device itself).
pub fn identify_by_name(name: &str) -> Option<ModelInfo> {
    nothing::identify_by_name(name).or_else(|| oneplus::identify_by_name(name))
}

pub fn make_driver(model: &ModelInfo, transport: TransportKind) -> Box<dyn EarbudDriver> {
    match model.vendor {
        Vendor::Nothing | Vendor::Cmf => Box::new(nothing::NothingDriver::new()),
        Vendor::Oneplus => Box::new(oneplus::OnePlusDriver::new(transport)),
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

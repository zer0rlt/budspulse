//! Bluetooth transport layer and the per-device session actor.
//!
//! `EarbudTransport` moves raw bytes (RFCOMM socket, BLE GATT characteristic pair, or the
//! in-process hardware simulator). The session actor owns one transport + one sans-IO driver,
//! performs the handshake, forwards user commands, polls battery, and reconnects with
//! exponential backoff when the link drops.

#[cfg(target_os = "linux")]
pub mod linux;
pub mod simulator;
#[cfg(windows)]
pub mod windows;

use crate::protocols::{
    self, ConnectionState, DeviceCommand, DeviceState, DriverEvent, ModelInfo, Outgoing, TransportKind, Vendor,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Nothing / CMF control service (SPP-like, custom UUID).
pub const NOTHING_SPP_UUID: &str = "aeac4a03-dff5-498f-843a-34487cf133eb";
/// OnePlus / OPPO HeyMelody OPO service (RFCOMM service class + BLE primary service).
pub const OPO_SERVICE_UUID: &str = "0000079a-d102-11e1-9b23-00025b00a5a5";
/// Legacy OnePlus Buds Pro service UUID.
pub const OPO_LEGACY_UUID: &str = "00001107-d102-11e1-9b23-00025b00a5a5";
pub const OPO_BLE_WRITE_UUID: &str = "0100079a-d102-11e1-9b23-00025b00a5a5";
pub const OPO_BLE_NOTIFY_UUID: &str = "0200079a-d102-11e1-9b23-00025b00a5a5";
pub const OPO_BLE_TELEMETRY_SERVICE: &str = "fe2c1234-8366-4814-8eb0-01de32100bea";
pub const OPO_BLE_TELEMETRY_NOTIFY: &str = "fe2c123a-8366-4814-8eb0-01de32100bea";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    pub address: String,
    pub vendor: Option<Vendor>,
    pub model: Option<String>,
    pub connected: bool,
}

#[async_trait]
pub trait EarbudTransport: Send {
    fn kind(&self) -> TransportKind;
    async fn connect(&mut self) -> Result<(), String>;
    async fn disconnect(&mut self);
    async fn send_frame(&mut self, data: &[u8]) -> Result<(), String>;
    /// Stream of incoming byte chunks. Channel closes when the link is lost.
    fn subscribe_rx(&mut self) -> mpsc::Receiver<Vec<u8>>;
}

#[derive(Debug, Clone)]
pub enum TargetKind {
    Real,
    Simulated(simulator::SimModel),
}

#[derive(Debug, Clone)]
pub struct Target {
    pub id: String,
    pub name: String,
    pub address: String,
    pub model: ModelInfo,
    pub kind: TargetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketDir {
    Tx,
    Rx,
}

/// Where the session reports to (implemented by the Tauri app core; a recorder in tests).
pub trait SessionSink: Send + Sync {
    fn on_state(&self, state: &DeviceState);
    fn on_packet(&self, dir: PacketDir, bytes: &[u8], desc: &str);
    fn on_event(&self, event: &DriverEvent, state: &DeviceState);
}

#[derive(Debug)]
pub enum SessionCmd {
    Command(DeviceCommand),
    Stop,
}

pub struct SessionHandle {
    pub target_id: String,
    pub tx: mpsc::UnboundedSender<SessionCmd>,
    pub task: tokio::task::JoinHandle<()>,
}

impl SessionHandle {
    pub fn send(&self, cmd: DeviceCommand) -> bool {
        self.tx.send(SessionCmd::Command(cmd)).is_ok()
    }
    pub async fn stop(self) {
        let _ = self.tx.send(SessionCmd::Stop);
        let _ = tokio::time::timeout(Duration::from_secs(3), self.task).await;
    }
}

pub fn spawn_session(target: Target, sink: Arc<dyn SessionSink>) -> SessionHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let id = target.id.clone();
    let task = tokio::spawn(run_session(target, sink, rx));
    SessionHandle { target_id: id, tx, task }
}

pub async fn open_transport(target: &Target) -> Result<Box<dyn EarbudTransport>, String> {
    match &target.kind {
        TargetKind::Simulated(m) => {
            let mut t: Box<dyn EarbudTransport> = Box::new(simulator::SimulatorTransport::new(*m));
            t.connect().await?;
            Ok(t)
        }
        TargetKind::Real => {
            #[cfg(target_os = "linux")]
            {
                return linux::open(target).await;
            }
            #[cfg(windows)]
            {
                return windows::open(target).await;
            }
            #[allow(unreachable_code)]
            Err("Bluetooth не поддерживается на этой платформе".into())
        }
    }
}

pub async fn list_paired() -> Vec<PairedDevice> {
    #[cfg(target_os = "linux")]
    {
        return linux::list_paired().await.unwrap_or_default();
    }
    #[cfg(windows)]
    {
        return windows::list_paired().await.unwrap_or_default();
    }
    #[allow(unreachable_code)]
    Vec::new()
}

/// Removes the OS pairing of a device (it disappears from the system Bluetooth list).
pub async fn unpair(address: &str) -> Result<(), String> {
    let _ = address;
    #[cfg(target_os = "linux")]
    {
        return linux::unpair(address).await;
    }
    #[cfg(windows)]
    {
        return windows::unpair(address).await;
    }
    #[allow(unreachable_code)]
    Err("Bluetooth не поддерживается на этой платформе".into())
}

/// Active A2DP codec as reported by the OS audio stack (None if unknown / unsupported).
pub async fn active_codec(address: &str) -> Option<String> {
    let _ = address;
    #[cfg(target_os = "linux")]
    {
        let a = address.to_string();
        return tokio::task::spawn_blocking(move || crate::audio_eq::linux_pipewire::bluez_codec(&a)).await.ok().flatten();
    }
    #[allow(unreachable_code)]
    None
}

/// OS-level battery fallback (BlueZ Battery1 / Windows PnP) used until RFCOMM is up.
pub async fn os_battery(address: &str) -> Option<u8> {
    let _ = address;
    #[cfg(target_os = "linux")]
    {
        return linux::battery1(address).await;
    }
    #[cfg(windows)]
    {
        return windows::pnp_battery(address).await;
    }
    #[allow(unreachable_code)]
    None
}

/// Whether the OS currently has a Bluetooth link to the device (`None` = cannot tell).
/// This is a local query of the OS Bluetooth stack — no radio traffic to the earbuds.
pub async fn os_connected(address: &str) -> Option<bool> {
    let _ = address;
    #[cfg(target_os = "linux")]
    {
        return linux::os_connected(address).await;
    }
    #[cfg(windows)]
    {
        return windows::os_connected(address).await;
    }
    #[allow(unreachable_code)]
    None
}

enum Exit {
    Stop,
    Lost(String),
}

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(8);
const POLL_EVERY: Duration = Duration::from_secs(60);
const CODEC_EVERY: Duration = Duration::from_secs(10);
const OS_CHECK_EVERY: Duration = Duration::from_secs(3);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Sleep for `dur` while staying responsive: returns `true` if the session must stop.
async fn idle(dur: Duration, rx: &mut mpsc::UnboundedReceiver<SessionCmd>, state: &mut DeviceState, sink: &dyn SessionSink) -> bool {
    let wait = tokio::time::sleep(dur);
    tokio::pin!(wait);
    loop {
        tokio::select! {
            _ = &mut wait => return false,
            c = rx.recv() => match c {
                None | Some(SessionCmd::Stop) => return true,
                Some(SessionCmd::Command(_)) => {
                    state.last_error = Some("наушники не подключены к ПК".into());
                    sink.on_state(state);
                }
            }
        }
    }
}

fn set_disconnected(state: &mut DeviceState) {
    state.connection = ConnectionState::Disconnected;
    state.transport = None;
    state.pending.clear();
}

async fn run_session(target: Target, sink: Arc<dyn SessionSink>, mut rx: mpsc::UnboundedReceiver<SessionCmd>) {
    let mut state = DeviceState::new(&target.id, &target.name, &target.address, &target.model);
    let real = matches!(target.kind, TargetKind::Real);
    let mut backoff = Duration::from_secs(1);
    loop {
        // 1. Real earbuds: only talk to them when the OS says they are connected to this PC.
        if real && os_connected(&target.address).await == Some(false) {
            set_disconnected(&mut state);
            state.last_error = None;
            sink.on_state(&state);
            loop {
                if idle(OS_CHECK_EVERY, &mut rx, &mut state, sink.as_ref()).await {
                    sink.on_state(&state);
                    return;
                }
                if os_connected(&target.address).await != Some(false) {
                    break;
                }
            }
            backoff = Duration::from_secs(1);
            // let A2DP/HFP settle after the OS link comes up
            if idle(Duration::from_millis(1500), &mut rx, &mut state, sink.as_ref()).await {
                return;
            }
        }

        // 2. Open the control channel.
        state.connection = ConnectionState::Connecting;
        state.pending.clear();
        sink.on_state(&state);
        if real && state.battery.left.level.is_none() {
            if let Some(b) = os_battery(&target.address).await {
                state.battery.left.level = Some(b);
                state.battery.right.level = Some(b);
                sink.on_state(&state);
            }
        }
        match open_transport(&target).await {
            Err(e) => {
                if real && os_connected(&target.address).await == Some(false) {
                    continue; // went away while we were connecting — just wait for it again
                }
                log::warn!("connect {} failed: {e}", target.name);
                state.connection = ConnectionState::Error;
                state.last_error = Some(e);
                state.transport = None;
                sink.on_state(&state);
            }
            Ok(mut transport) => {
                backoff = Duration::from_secs(1);
                state.last_error = None;
                let exit = drive(&mut *transport, &mut state, &target, sink.as_ref(), &mut rx).await;
                transport.disconnect().await;
                match exit {
                    Exit::Stop => {
                        set_disconnected(&mut state);
                        sink.on_state(&state);
                        return;
                    }
                    Exit::Lost(e) => {
                        log::info!("link to {} lost: {e}", target.name);
                        if real && os_connected(&target.address).await == Some(false) {
                            continue; // buds were put in the case / disconnected: wait quietly
                        }
                        state.connection = ConnectionState::Error;
                        state.last_error = Some(e);
                        state.transport = None;
                        sink.on_state(&state);
                    }
                }
            }
        }
        // 3. Retry with backoff; stop early if the OS link disappears.
        let mut waited = Duration::ZERO;
        while waited < backoff {
            let step = OS_CHECK_EVERY.min(backoff - waited);
            if idle(step, &mut rx, &mut state, sink.as_ref()).await {
                set_disconnected(&mut state);
                sink.on_state(&state);
                return;
            }
            waited += step;
            if real && os_connected(&target.address).await == Some(false) {
                break;
            }
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn send_all(transport: &mut dyn EarbudTransport, sink: &dyn SessionSink, frames: Vec<Outgoing>) -> Result<(), String> {
    for o in frames {
        transport.send_frame(&o.bytes).await?;
        sink.on_packet(PacketDir::Tx, &o.bytes, &o.desc);
        if !o.delay_after.is_zero() {
            tokio::time::sleep(o.delay_after).await;
        }
    }
    Ok(())
}

async fn drive(
    transport: &mut dyn EarbudTransport,
    state: &mut DeviceState,
    target: &Target,
    sink: &dyn SessionSink,
    rx: &mut mpsc::UnboundedReceiver<SessionCmd>,
) -> Exit {
    let kind = transport.kind();
    let mut driver = protocols::make_driver(&target.model, kind);
    let mut incoming = transport.subscribe_rx();
    state.transport = Some(kind);
    state.connection = ConnectionState::Handshake;
    sink.on_state(state);

    let started = Instant::now();
    if let Err(e) = send_all(transport, sink, driver.session_start()).await {
        return Exit::Lost(e);
    }
    let mut ready = false;
    if !driver.needs_handshake_reply() {
        ready = true;
        state.connection = ConnectionState::Connected;
        sink.on_event(&DriverEvent::Ready, state);
        let q = driver.initial_queries(state);
        if let Err(e) = send_all(transport, sink, q).await {
            return Exit::Lost(e);
        }
        sink.on_state(state);
    }

    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_poll = Instant::now();
    let mut last_codec = Instant::now() - CODEC_EVERY;

    loop {
        tokio::select! {
            chunk = incoming.recv() => {
                let Some(chunk) = chunk else { return Exit::Lost("соединение закрыто наушниками".into()) };
                let events = driver.feed(&chunk, state);
                let mut to_send = Vec::new();
                for ev in &events {
                    match ev {
                        DriverEvent::Rx { bytes, desc } => sink.on_packet(PacketDir::Rx, bytes, desc),
                        DriverEvent::Ready if !ready => {
                            ready = true;
                            state.connection = ConnectionState::Connected;
                            to_send.extend(driver.initial_queries(state));
                        }
                        DriverEvent::Send(frames) => to_send.extend(frames.iter().cloned()),
                        DriverEvent::Ack { ok: false, message, .. } => state.last_error = message.clone(),
                        DriverEvent::Ack { ok: true, .. } => state.last_error = None,
                        _ => {}
                    }
                    sink.on_event(ev, state);
                }
                if !events.is_empty() {
                    sink.on_state(state);
                }
                if let Err(e) = send_all(transport, sink, to_send).await {
                    return Exit::Lost(e);
                }
            }
            cmd = rx.recv() => {
                match cmd {
                    None | Some(SessionCmd::Stop) => return Exit::Stop,
                    Some(SessionCmd::Command(c)) => {
                        if !ready {
                            state.last_error = Some("наушники ещё не готовы".into());
                            sink.on_state(state);
                            continue;
                        }
                        match driver.encode(&c, state) {
                            Ok(frames) => {
                                sink.on_state(state);
                                if let Err(e) = send_all(transport, sink, frames).await {
                                    return Exit::Lost(e);
                                }
                            }
                            Err(e) => {
                                state.last_error = Some(e.to_string());
                                sink.on_state(state);
                            }
                        }
                    }
                }
            }
            _ = tick.tick() => {
                let now = Instant::now();
                if !ready && now.duration_since(started) > HANDSHAKE_TIMEOUT {
                    return Exit::Lost("наушники не ответили на рукопожатие".into());
                }
                let events = driver.tick(now, state);
                if !events.is_empty() {
                    for ev in &events {
                        if let DriverEvent::Ack { ok: false, message, .. } = ev {
                            state.last_error = message.clone();
                        }
                        sink.on_event(ev, state);
                    }
                    sink.on_state(state);
                }
                if ready && now.duration_since(last_poll) >= POLL_EVERY {
                    last_poll = now;
                    let q = driver.poll_queries(state);
                    if let Err(e) = send_all(transport, sink, q).await {
                        return Exit::Lost(e);
                    }
                }
                if ready && matches!(target.kind, TargetKind::Real) && now.duration_since(last_codec) >= CODEC_EVERY {
                    last_codec = now;
                    let codec = active_codec(&target.address).await;
                    if codec.is_some() && codec != state.codec {
                        state.codec = codec;
                        sink.on_state(state);
                    }
                }
            }
        }
    }
}

/// Parse "AA:BB:CC:DD:EE:FF" into a u64 (Windows BluetoothAddress format).
pub fn mac_to_u64(mac: &str) -> Option<u64> {
    let parts: Vec<&str> = mac.split([':', '-']).collect();
    if parts.len() != 6 {
        return None;
    }
    let mut v = 0u64;
    for p in parts {
        v = (v << 8) | u64::from(u8::from_str_radix(p, 16).ok()?);
    }
    Some(v)
}

pub fn u64_to_mac(v: u64) -> String {
    (0..6).rev().map(|i| format!("{:02X}", (v >> (i * 8)) & 0xFF)).collect::<Vec<_>>().join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_roundtrip() {
        let m = "2C:BE:EB:00:12:34";
        let v = mac_to_u64(m).unwrap();
        assert_eq!(v, 0x2CBEEB001234);
        assert_eq!(u64_to_mac(v), m);
        assert_eq!(mac_to_u64("zz"), None);
    }
}

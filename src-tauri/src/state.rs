//! Application core: shared state, persistence, event emission to the UI, the session sink
//! (device state + packet log), connection toast and the automation host.

use crate::audio_eq::{HostEqEngine, HostEqProfile, HostEqStatus};
use crate::automation::{AutoEvent, AutomationHost, AutomationSettings};
use crate::bt::{self, simulator::SimModel, PacketDir, PairedDevice, SessionHandle, SessionSink, Target, TargetKind};
use crate::protocols::{self, hex, AncMode, ConnectionState, DeviceCommand, DeviceState, DriverEvent, Vendor};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

const MAX_PACKETS: usize = 2000;
const STORE_FILE: &str = "budspulse.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub automation: AutomationSettings,
    pub theme_override: Option<Vendor>,
    pub autostart: bool,
    pub demo_model: Option<String>,
    pub eq_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_device: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { automation: AutomationSettings::default(), theme_override: None, autostart: false, demo_model: None, eq_mode: "hardware".into(), last_device: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketLog {
    pub ts: u64,
    pub dir: String,
    pub hex: String,
    pub desc: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedKey {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub demo: bool,
    pub demo_models: Vec<NamedKey>,
    pub paired: Vec<PairedDevice>,
    pub device: Option<DeviceState>,
    pub settings: Settings,
    pub host_eq: HostEqProfile,
    pub host_eq_status: HostEqStatus,
    pub platform: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToastPayload {
    pub device: DeviceState,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct CaseCache {
    level: u8,
    ts: u64,
}

pub struct CoreData {
    pub demo: bool,
    pub device: Option<DeviceState>,
    pub paired: Vec<PairedDevice>,
    pub settings: Settings,
    pub host_eq: HostEqProfile,
    pub host_eq_status: HostEqStatus,
    pub packets: VecDeque<PacketLog>,
    case_cache: HashMap<String, CaseCache>,
    toast_shown_for: Option<String>,
    active_id: Option<String>,
}

pub struct Core {
    pub app: AppHandle,
    pub data: Mutex<CoreData>,
    pub session: tokio::sync::Mutex<Option<SessionHandle>>,
    session_tx: Mutex<Option<mpsc::UnboundedSender<bt::SessionCmd>>>,
    pub eq: Mutex<HostEqEngine>,
    auto_tx: mpsc::UnboundedSender<AutoEvent>,
    auto_rx: Mutex<Option<mpsc::UnboundedReceiver<AutoEvent>>>,
    /// tray refresh callback (installed by lib.rs)
    pub on_change: Mutex<Option<ChangeHook>>,
}

/// Called on every snapshot (tray icon / tooltip refresh).
pub type ChangeHook = Box<dyn Fn(Option<&DeviceState>, bool) + Send + Sync>;

pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    }
}

impl Core {
    pub fn new(app: AppHandle) -> Arc<Self> {
        let config_dir = app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir().join("budspulse"));
        let _ = std::fs::create_dir_all(&config_dir);
        let (auto_tx, auto_rx) = mpsc::unbounded_channel();
        let eq = HostEqEngine::new(config_dir);
        let status = eq.status();
        let core = Arc::new(Core {
            app,
            data: Mutex::new(CoreData {
                demo: false,
                device: None,
                paired: vec![],
                settings: Settings::default(),
                host_eq: HostEqProfile::default(),
                host_eq_status: status,
                packets: VecDeque::new(),
                case_cache: HashMap::new(),
                toast_shown_for: None,
                active_id: None,
            }),
            session: tokio::sync::Mutex::new(None),
            session_tx: Mutex::new(None),
            eq: Mutex::new(eq),
            auto_tx,
            auto_rx: Mutex::new(Some(auto_rx)),
            on_change: Mutex::new(None),
        });
        core.load();
        core
    }

    pub fn take_automation_rx(&self) -> Option<mpsc::UnboundedReceiver<AutoEvent>> {
        self.auto_rx.lock().take()
    }

    // ---------------------------------------------------------------- persistence

    fn load(&self) {
        use tauri_plugin_store::StoreExt;
        let Ok(store) = self.app.store(STORE_FILE) else { return };
        let mut d = self.data.lock();
        if let Some(v) = store.get("settings") {
            if let Ok(s) = serde_json::from_value::<Settings>(v) {
                d.settings = s;
            }
        }
        if let Some(v) = store.get("hostEq") {
            if let Ok(p) = serde_json::from_value::<HostEqProfile>(v) {
                d.host_eq = p.sanitized();
            }
        }
        if let Some(v) = store.get("caseCache") {
            if let Ok(c) = serde_json::from_value::<HashMap<String, CaseCache>>(v) {
                d.case_cache = c;
            }
        }
    }

    pub fn save(&self) {
        use tauri_plugin_store::StoreExt;
        let Ok(store) = self.app.store(STORE_FILE) else { return };
        let (s, eq, cc) = {
            let d = self.data.lock();
            (serde_json::to_value(&d.settings), serde_json::to_value(&d.host_eq), serde_json::to_value(&d.case_cache))
        };
        if let Ok(v) = s {
            store.set("settings", v);
        }
        if let Ok(v) = eq {
            store.set("hostEq", v);
        }
        if let Ok(v) = cc {
            store.set("caseCache", v);
        }
        let _ = store.save();
    }

    // ---------------------------------------------------------------- snapshot / events

    pub fn snapshot(&self) -> Snapshot {
        let d = self.data.lock();
        Snapshot {
            demo: d.demo,
            demo_models: SimModel::ALL.iter().map(|m| NamedKey { key: m.key().into(), name: m.display().into() }).collect(),
            paired: d.paired.clone(),
            device: d.device.clone(),
            settings: d.settings.clone(),
            host_eq: d.host_eq.clone(),
            host_eq_status: d.host_eq_status.clone(),
            platform: platform(),
        }
    }

    pub fn emit_snapshot(&self) {
        let snap = self.snapshot();
        if let Some(cb) = self.on_change.lock().as_ref() {
            cb(snap.device.as_ref(), snap.demo);
        }
        let _ = self.app.emit("snapshot", snap);
    }

    pub fn packets(&self) -> Vec<PacketLog> {
        self.data.lock().packets.iter().cloned().collect()
    }

    pub fn clear_packets(&self) {
        self.data.lock().packets.clear();
    }

    // ---------------------------------------------------------------- sessions

    pub async fn stop_session(&self) {
        *self.session_tx.lock() = None;
        if let Some(h) = self.session.lock().await.take() {
            h.stop().await;
        }
        self.data.lock().active_id = None;
    }

    pub async fn start_session(self: &Arc<Self>, target: Target, demo: bool) {
        self.stop_session().await;
        {
            let mut d = self.data.lock();
            d.demo = demo;
            d.toast_shown_for = None;
            d.active_id = Some(target.id.clone());
            let mut st = DeviceState::new(&target.id, &target.name, &target.address, &target.model);
            st.connection = ConnectionState::Connecting;
            self.apply_case_cache(&mut d.case_cache, &mut st);
            d.device = Some(st);
            if !demo {
                d.settings.last_device = Some(target.id.clone());
            }
        }
        self.save();
        self.emit_snapshot();
        let sink: Arc<dyn SessionSink> = self.clone();
        let h = bt::spawn_session(target, sink);
        *self.session_tx.lock() = Some(h.tx.clone());
        *self.session.lock().await = Some(h);
    }

    pub async fn connect_paired(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let paired = self.data.lock().paired.iter().find(|p| p.id == id).cloned();
        let p = paired.ok_or("устройство не найдено среди сопряжённых")?;
        let model = protocols::identify_by_name(&p.name)
            .or_else(|| p.model.as_deref().and_then(|m| protocols::all_models().into_iter().find(|x| x.name == m)))
            .ok_or("модель не поддерживается")?;
        self.start_session(Target { id: p.id.clone(), name: p.name.clone(), address: p.address.clone(), model, kind: TargetKind::Real }, false).await;
        Ok(())
    }

    /// Removes a paired device from the OS. If it is the active device, its session is stopped
    /// first (an open RFCOMM channel would keep the link alive) and it is forgotten as the last
    /// device, so auto-connect does not try to come back to it.
    pub async fn unpair(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let (address, active) = {
            let d = self.data.lock();
            let p = d.paired.iter().find(|p| p.id == id).ok_or("устройство не найдено среди сопряжённых")?;
            let active = !d.demo && d.device.as_ref().map(|dev| dev.id == id).unwrap_or(false);
            (p.address.clone(), active)
        };
        if active {
            self.stop_session().await;
            self.data.lock().device = None;
        }
        {
            let mut d = self.data.lock();
            if d.settings.last_device.as_deref() == Some(id) {
                d.settings.last_device = None;
            }
            d.case_cache.remove(id);
        }
        self.save();
        let result = bt::unpair(&address).await;
        self.rescan().await;
        result
    }

    pub async fn start_demo(self: &Arc<Self>, key: Option<&str>) {
        let m = key.and_then(SimModel::from_key).unwrap_or(SimModel::OnePlusBudsPro4);
        self.data.lock().settings.demo_model = Some(m.key().into());
        let target = Target { id: format!("sim-{}", m.key()), name: m.bt_name().into(), address: m.address().into(), model: m.model_info(), kind: TargetKind::Simulated(m) };
        self.start_session(target, true).await;
    }

    pub async fn stop_demo(self: &Arc<Self>) {
        self.stop_session().await;
        {
            let mut d = self.data.lock();
            d.demo = false;
            d.device = None;
        }
        self.emit_snapshot();
        let me = self.clone();
        tokio::spawn(async move { me.auto_connect().await });
    }

    pub fn send_command(&self, cmd: DeviceCommand) -> Result<(), String> {
        let guard = self.session_tx.lock();
        let tx = guard.as_ref().ok_or("наушники не подключены")?;
        tx.send(bt::SessionCmd::Command(cmd)).map_err(|_| "сессия завершена".to_string())
    }

    pub async fn rescan(self: &Arc<Self>) -> Vec<PairedDevice> {
        let list = bt::list_paired().await;
        self.data.lock().paired = list.clone();
        self.emit_snapshot();
        list
    }

    /// Connect to the last used device, else the first supported device connected to the PC.
    pub async fn auto_connect(self: &Arc<Self>) {
        if self.session.lock().await.is_some() {
            return;
        }
        let list = self.rescan().await;
        let last = self.data.lock().settings.last_device.clone();
        let pick = list
            .iter()
            .find(|p| Some(&p.id) == last.as_ref() && p.connected)
            .or_else(|| list.iter().find(|p| p.connected))
            .or_else(|| list.iter().find(|p| Some(&p.id) == last.as_ref()));
        if let Some(p) = pick.cloned() {
            let _ = self.connect_paired(&p.id).await;
        }
    }

    /// The active (real) device is not connected to the PC but another supported one is →
    /// switch to it. Never touches demo sessions or a device that is currently connected.
    pub async fn follow_os_connection(self: &Arc<Self>) {
        let (switch_to, demo) = {
            let d = self.data.lock();
            let active = d.device.as_ref();
            let active_os = active.and_then(|a| d.paired.iter().find(|p| p.id == a.id)).map(|p| p.connected).unwrap_or(false);
            let busy = active.map(|a| a.connection == ConnectionState::Connected).unwrap_or(false);
            let other = d.paired.iter().find(|p| p.connected && Some(&p.id) != active.map(|a| &a.id)).map(|p| p.id.clone());
            (if !active_os && !busy { other } else { None }, d.demo)
        };
        if let (Some(id), false) = (switch_to, demo) {
            log::info!("switching to OS-connected device {id}");
            let _ = self.connect_paired(&id).await;
        }
    }

    // ---------------------------------------------------------------- host EQ

    pub fn set_host_eq(&self, profile: HostEqProfile) -> HostEqStatus {
        let profile = profile.sanitized();
        let hint = self.data.lock().device.as_ref().map(|d| d.name.clone());
        let status = self.eq.lock().apply(&profile, hint.as_deref());
        {
            let mut d = self.data.lock();
            d.host_eq = profile;
            d.host_eq_status = status.clone();
        }
        self.save();
        status
    }

    pub fn install_host_eq(&self) -> HostEqStatus {
        let (profile, hint) = {
            let d = self.data.lock();
            (d.host_eq.clone(), d.device.as_ref().map(|x| x.name.clone()))
        };
        let status = self.eq.lock().install(&profile, hint.as_deref());
        self.data.lock().host_eq_status = status.clone();
        self.emit_snapshot();
        status
    }

    // ---------------------------------------------------------------- helpers

    fn apply_case_cache(&self, cache: &mut HashMap<String, CaseCache>, st: &mut DeviceState) {
        match st.battery.case.level {
            Some(l) if st.battery.case_cached_at.is_none() => {
                cache.insert(st.id.clone(), CaseCache { level: l, ts: protocols::now_ms() });
            }
            None => {
                if let Some(c) = cache.get(&st.id) {
                    st.battery.case.level = Some(c.level);
                    st.battery.case.charging = false;
                    st.battery.case_cached_at = Some(c.ts);
                }
            }
            _ => {}
        }
    }

    fn show_toast(&self, device: DeviceState, reason: &'static str) {
        if !self.data.lock().settings.automation.connection_popup {
            return;
        }
        let Some(w) = self.app.get_webview_window("toast") else { return };
        if let Ok(Some(mon)) = w.primary_monitor() {
            let scale = mon.scale_factor();
            let wa = mon.work_area();
            let (tw, th) = ((340.0 * scale) as i32, (150.0 * scale) as i32);
            let margin = (12.0 * scale) as i32;
            let x = wa.position.x + wa.size.width as i32 - tw - margin;
            // Windows: bottom-right above the taskbar (like system toasts); Linux/KDE: top-right
            let y = if cfg!(windows) { wa.position.y + wa.size.height as i32 - th - margin } else { wa.position.y + margin };
            let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        }
        let _ = w.set_always_on_top(true);
        let _ = w.show();
        let _ = self.app.emit_to("toast", "toast", ToastPayload { device, reason });
    }

    pub fn notify(&self, title: &str, body: &str) {
        use tauri_plugin_notification::NotificationExt;
        let _ = self.app.notification().builder().title(title).body(body).show();
    }

    pub fn device_snapshot(&self) -> Option<DeviceState> {
        self.data.lock().device.clone()
    }

    pub fn diagnostics_json(&self) -> serde_json::Value {
        let d = self.data.lock();
        serde_json::json!({
            "app": "BudsPulse",
            "version": env!("CARGO_PKG_VERSION"),
            "platform": platform(),
            "generatedAt": protocols::now_ms(),
            "demo": d.demo,
            "device": d.device,
            "paired": d.paired,
            "settings": d.settings,
            "hostEq": d.host_eq,
            "hostEqStatus": d.host_eq_status,
            "packets": d.packets,
        })
    }
}

impl SessionSink for Core {
    fn on_state(&self, state: &DeviceState) {
        let mut toast = None;
        {
            let mut d = self.data.lock();
            if d.active_id.as_deref() != Some(state.id.as_str()) {
                return; // late update from a session that was replaced
            }
            let mut st = state.clone();
            let mut cache = std::mem::take(&mut d.case_cache);
            self.apply_case_cache(&mut cache, &mut st);
            d.case_cache = cache;
            let was = d.device.as_ref().map(|x| x.connection);
            let battery_known = st.battery.left.level.or(st.battery.right.level).is_some();
            if st.connection == ConnectionState::Connected && battery_known && d.toast_shown_for.as_deref() != Some(&st.id) {
                d.toast_shown_for = Some(st.id.clone());
                toast = Some(st.clone());
            }
            if st.connection != ConnectionState::Connected && was == Some(ConnectionState::Connected) {
                d.toast_shown_for = None;
            }
            d.device = Some(st);
        }
        if let Some(t) = toast {
            self.show_toast(t, "connected");
        }
        self.emit_snapshot();
    }

    fn on_packet(&self, dir: PacketDir, bytes: &[u8], desc: &str) {
        let p = PacketLog { ts: protocols::now_ms(), dir: if dir == PacketDir::Tx { "tx" } else { "rx" }.into(), hex: hex(bytes), desc: desc.into() };
        {
            let mut d = self.data.lock();
            if d.packets.len() >= MAX_PACKETS {
                d.packets.pop_front();
            }
            d.packets.push_back(p.clone());
        }
        let _ = self.app.emit("packet", p);
    }

    fn on_event(&self, event: &DriverEvent, state: &DeviceState) {
        match event {
            DriverEvent::Wear { before, after } => {
                let _ = self.auto_tx.send(AutoEvent::Wear { before: before.clone(), after: after.clone() });
            }
            DriverEvent::Ready => log::info!("{} ready", state.name),
            _ => {}
        }
    }
}

impl AutomationHost for Core {
    fn automation_settings(&self) -> AutomationSettings {
        self.data.lock().settings.automation.clone()
    }
    fn current_device(&self) -> Option<DeviceState> {
        self.device_snapshot()
    }
    fn send_device_command(&self, cmd: DeviceCommand) {
        let _ = self.send_command(cmd);
    }
    fn notify(&self, title: &str, body: &str) {
        Core::notify(self, title, body)
    }
}

/// Next ANC mode for the middle-click cycle: ANC (High) ↔ Transparency.
pub fn cycle_anc(d: &DeviceState) -> DeviceCommand {
    match d.anc.mode {
        Some(AncMode::Anc) => DeviceCommand::SetAnc { mode: AncMode::Transparency, level: None },
        _ => DeviceCommand::SetAnc { mode: AncMode::Anc, level: d.caps.anc_levels.first().copied() },
    }
}

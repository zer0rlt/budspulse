//! End-to-end: session actor + real drivers + stateful simulator speaking the wire protocols.

use budspulse_lib::bt::simulator::{sim_action, SimAction, SimModel};
use budspulse_lib::bt::{self, PacketDir, SessionSink, Target, TargetKind};
use budspulse_lib::protocols::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Rec {
    last: Mutex<Option<DeviceState>>,
    tx: AtomicUsize,
    rx: AtomicUsize,
    wear_events: AtomicUsize,
}

impl SessionSink for Rec {
    fn on_state(&self, s: &DeviceState) {
        *self.last.lock().unwrap() = Some(s.clone());
    }
    fn on_packet(&self, dir: PacketDir, _b: &[u8], _d: &str) {
        match dir {
            PacketDir::Tx => self.tx.fetch_add(1, Ordering::SeqCst),
            PacketDir::Rx => self.rx.fetch_add(1, Ordering::SeqCst),
        };
    }
    fn on_event(&self, e: &DriverEvent, _s: &DeviceState) {
        if matches!(e, DriverEvent::Wear { .. }) {
            self.wear_events.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl Rec {
    fn get(&self) -> DeviceState {
        self.last.lock().unwrap().clone().expect("state")
    }
    async fn wait(&self, what: &str, f: impl Fn(&DeviceState) -> bool) -> DeviceState {
        let t0 = Instant::now();
        loop {
            if let Some(s) = self.last.lock().unwrap().clone() {
                if f(&s) {
                    return s;
                }
            }
            if t0.elapsed() > Duration::from_secs(12) {
                panic!("timeout waiting for {what}: {:#?}", self.get());
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    }
}

fn target(m: SimModel) -> Target {
    Target { id: format!("sim-{}", m.key()), name: m.bt_name().into(), address: m.address().into(), model: m.model_info(), kind: TargetKind::Simulated(m) }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn simulated_devices_end_to_end() {
    // ---------------- OnePlus Buds Pro 4 (OPO 0xAA)
    let rec = Arc::new(Rec::default());
    let h = bt::spawn_session(target(SimModel::OnePlusBudsPro4), rec.clone());
    let s = rec.wait("connected + battery", |s| s.connection == ConnectionState::Connected && s.battery.left.level.is_some() && s.anc.mode.is_some()).await;
    assert_eq!(s.model_code, "067814");
    assert_eq!(s.codec.as_deref(), Some("LHDC 5.0"));
    assert!(s.firmware.is_some());
    rec.wait("wear", |s| s.wear.left == WearPosition::Ear).await;

    h.send(DeviceCommand::SetAnc { mode: AncMode::Transparency, level: None });
    rec.wait("pending anc visible", |s| s.pending.iter().any(|k| k == "anc") || s.anc.mode == Some(AncMode::Transparency)).await;
    rec.wait("transparency confirmed", |s| s.anc.mode == Some(AncMode::Transparency) && s.pending.is_empty()).await;

    h.send(DeviceCommand::SetAnc { mode: AncMode::Anc, level: Some(AncLevel::Adaptive) });
    rec.wait("adaptive", |s| s.anc.level == Some(AncLevel::Adaptive) && s.anc.mode == Some(AncMode::Anc) && s.pending.is_empty()).await;

    h.send(DeviceCommand::SetEqPreset { id: 7 });
    rec.wait("dynaudio", |s| s.eq.preset_id == Some(7) && s.pending.is_empty()).await;

    let gains = vec![2.0, 1.0, 0.0, -1.0, 0.0, 1.0, 2.0, 3.0, 1.0, 0.0];
    h.send(DeviceCommand::SetCustomEq { gains: gains.clone() });
    let s = rec.wait("custom eq", |s| s.eq.custom_gains == gains && s.pending.is_empty() && s.caps.custom_eq_preset_id.is_some()).await;
    assert!(s.caps.eq_presets.iter().any(|p| p.name.contains("BudsPulse")));

    h.send(DeviceCommand::SetLowLatency { enabled: true });
    rec.wait("game mode", |s| s.low_latency == Some(true) && s.pending.is_empty()).await;

    h.send(DeviceCommand::SetBass { enabled: true, level: 3 });
    rec.wait("basswave", |s| s.bass.enabled == Some(true) && s.bass.level == Some(3) && s.pending.is_empty()).await;

    h.send(DeviceCommand::SetSpatial { mode: "Отслеживание головы".into() });
    rec.wait("spatial", |s| s.spatial.as_deref() == Some("Отслеживание головы")).await;

    // physical world: take both buds out → ANC refused (0x0E / local wear rule)
    assert!(sim_action(SimAction::ToggleWearLeft));
    rec.wait("left out", |s| s.wear.left == WearPosition::Out).await;
    assert!(sim_action(SimAction::ToggleWearRight));
    rec.wait("right out", |s| s.wear.right == WearPosition::Out).await;
    assert!(rec.wear_events.load(Ordering::SeqCst) >= 2);
    h.send(DeviceCommand::SetAnc { mode: AncMode::Off, level: None });
    let s = rec.wait("not worn error", |s| s.last_error.is_some()).await;
    assert_eq!(s.anc.mode, Some(AncMode::Anc), "state must not change when refused");

    // close the case → link drops → session reconnects automatically, case % cached by driver
    assert!(sim_action(SimAction::CloseCase));
    rec.wait("link lost", |s| s.connection != ConnectionState::Connected).await;
    let s = rec.wait("reconnected", |s| s.connection == ConnectionState::Connected && s.anc.mode == Some(AncMode::Anc)).await;
    assert!(s.eq.preset_id.is_some());
    assert!(rec.tx.load(Ordering::SeqCst) > 20 && rec.rx.load(Ordering::SeqCst) > 20);
    h.stop().await;

    // ---------------- Nothing Ear (2024) (0x55 + CRC16)
    let rec = Arc::new(Rec::default());
    let h = bt::spawn_session(target(SimModel::NothingEar2024), rec.clone());
    let s = rec.wait("nothing connected", |s| s.connection == ConnectionState::Connected && s.battery.case.level.is_some() && s.firmware.is_some() && s.serial.is_some()).await;
    assert_eq!(s.model_code, "B171");
    assert_eq!(s.gestures.len(), 8);
    h.send(DeviceCommand::SetAnc { mode: AncMode::Anc, level: Some(AncLevel::Low) });
    rec.wait("anc low", |s| s.anc.level == Some(AncLevel::Low) && s.pending.is_empty()).await;
    h.send(DeviceCommand::SetCustomEq { gains: vec![4.0, -2.0, 1.0] });
    rec.wait("custom eq", |s| s.eq.custom_gains == vec![4.0, -2.0, 1.0] && s.eq.preset_id == Some(5) && s.pending.is_empty()).await;
    h.send(DeviceCommand::SetBass { enabled: true, level: 4 });
    rec.wait("ultra bass", |s| s.bass.level == Some(4) && s.bass.enabled == Some(true) && s.pending.is_empty()).await;
    h.send(DeviceCommand::SetGesture { side: Side::Left, gesture: "double".into(), action: 18 });
    rec.wait("gesture", |s| s.gestures.iter().any(|g| g.side == Side::Left && g.gesture == "double" && g.action == 18) && s.pending.is_empty()).await;
    h.send(DeviceCommand::StartFitTest);
    rec.wait("fit result", |s| !s.fit.running && s.fit.left.is_some()).await;
    h.send(DeviceCommand::SetInEarDetection { enabled: false });
    rec.wait("in-ear off", |s| s.in_ear_detection == Some(false) && s.pending.is_empty()).await;
    h.stop().await;

    // ---------------- CMF Buds Pro 2 (listening modes, spatial)
    let rec = Arc::new(Rec::default());
    let h = bt::spawn_session(target(SimModel::CmfBudsPro2), rec.clone());
    rec.wait("cmf connected", |s| s.connection == ConnectionState::Connected && s.spatial.is_some()).await;
    h.send(DeviceCommand::SetEqPreset { id: 0 });
    rec.wait("dirac", |s| s.eq.preset_id == Some(0) && s.pending.is_empty()).await;
    h.send(DeviceCommand::SetSpatial { mode: "Фиксированный".into() });
    rec.wait("spatial fixed", |s| s.spatial.as_deref() == Some("Фиксированный") && s.pending.is_empty()).await;
    h.stop().await;
}

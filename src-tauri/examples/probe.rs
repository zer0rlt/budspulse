//! Hardware probe (no UI): lists paired supported earbuds with the OS connection state and,
//! optionally, opens a real session to one of them and prints every frame + the final state.
//!
//!   cargo run --example probe                     # list
//!   cargo run --example probe -- AA:BB:CC:DD:EE:FF [seconds]

use budspulse_lib::bt::{self, PacketDir, SessionSink, Target, TargetKind};
use budspulse_lib::protocols::{self, DeviceState, DriverEvent};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Print {
    last: Mutex<Option<DeviceState>>,
}

impl SessionSink for Print {
    fn on_state(&self, s: &DeviceState) {
        let prev = self.last.lock().unwrap().replace(s.clone());
        if prev.map(|p| p.connection != s.connection || p.last_error != s.last_error).unwrap_or(true) {
            println!("[state] {:?} {:?} err={:?}", s.connection, s.transport, s.last_error);
        }
    }
    fn on_packet(&self, dir: PacketDir, b: &[u8], d: &str) {
        println!("[{}] {d}\n      {}", if dir == PacketDir::Tx { "TX" } else { "RX" }, protocols::hex(b));
    }
    fn on_event(&self, e: &DriverEvent, _s: &DeviceState) {
        if let DriverEvent::Ack { key, ok, message } = e {
            println!("[ack] {key} ok={ok} {message:?}");
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let list = bt::list_paired().await;
    for p in &list {
        println!("{:<20} {}  model={:?}  os_connected={}", p.name, p.address, p.model, p.connected);
    }
    let Some(mac) = args.first() else { return };
    let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(15);
    let p = list.iter().find(|p| p.address.eq_ignore_ascii_case(mac)).expect("device not in paired list");
    let model = protocols::identify_by_name(&p.name).expect("supported model");
    let sink = Arc::new(Print { last: Mutex::new(None) });
    let h = bt::spawn_session(
        Target { id: p.id.clone(), name: p.name.clone(), address: p.address.clone(), model, kind: TargetKind::Real },
        sink.clone(),
    );
    tokio::time::sleep(Duration::from_secs(secs)).await;
    h.stop().await;
    let last = sink.last.lock().unwrap().clone();
    if let Some(s) = last {
        println!("\n=== final state ===\n{}", serde_json::to_string_pretty(&s).unwrap());
    }
}

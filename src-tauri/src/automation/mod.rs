//! "Android-on-PC" automation: wear-detection media pause/resume, auto-transparency while the
//! microphone is in use, and auto low-latency while a fullscreen game runs.

pub mod media_control;
pub mod mic_monitor;

use crate::protocols::{AncLevel, AncMode, ConnectionState, DeviceCommand, DeviceState, WearPosition, WearState};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutomationSettings {
    pub auto_pause: bool,
    pub auto_transparency_on_mic: bool,
    pub auto_low_latency_in_games: bool,
    pub connection_popup: bool,
}

impl Default for AutomationSettings {
    fn default() -> Self {
        AutomationSettings { auto_pause: true, auto_transparency_on_mic: false, auto_low_latency_in_games: false, connection_popup: true }
    }
}

pub trait AutomationHost: Send + Sync + 'static {
    fn automation_settings(&self) -> AutomationSettings;
    fn current_device(&self) -> Option<DeviceState>;
    fn send_device_command(&self, cmd: DeviceCommand);
    fn notify(&self, title: &str, body: &str);
}

#[derive(Debug, Clone)]
pub enum AutoEvent {
    Wear { before: WearState, after: WearState },
}

const RESUME_WINDOW: Duration = Duration::from_secs(180);

fn removed(before: WearPosition, after: WearPosition) -> bool {
    before == WearPosition::Ear && after != WearPosition::Ear && after != WearPosition::Unknown
}

#[derive(Default)]
struct Coordinator {
    paused: Option<(Instant, Vec<String>)>,
    mic_was_active: bool,
    saved_anc: Option<(AncMode, Option<AncLevel>)>,
    game_was_running: bool,
    we_enabled_low_latency: bool,
}

impl Coordinator {
    async fn on_wear(&mut self, host: &dyn AutomationHost, before: WearState, after: WearState) {
        if !host.automation_settings().auto_pause {
            return;
        }
        let out = removed(before.left, after.left) || removed(before.right, after.right);
        let back = (before.left != WearPosition::Ear && after.left == WearPosition::Ear) || (before.right != WearPosition::Ear && after.right == WearPosition::Ear);
        if out {
            let ids = media_control::pause_playing().await;
            if !ids.is_empty() {
                log::info!("auto-pause: paused {ids:?}");
                self.paused = Some((Instant::now(), ids));
            }
        } else if back && after.left != WearPosition::Out && after.right != WearPosition::Out {
            if let Some((t, ids)) = self.paused.take() {
                if t.elapsed() <= RESUME_WINDOW {
                    log::info!("auto-resume: {ids:?}");
                    media_control::resume(&ids).await;
                }
            }
        }
    }

    async fn poll(&mut self, host: &dyn AutomationHost) {
        let s = host.automation_settings();
        let dev = host.current_device().filter(|d| d.connection == ConnectionState::Connected);
        // --- microphone → transparency
        if s.auto_transparency_on_mic {
            let active = mic_monitor::microphone_active().await;
            if let Some(d) = &dev {
                if active && !self.mic_was_active && d.caps.transparency && d.anc.mode != Some(AncMode::Transparency) {
                    if let Some(m) = d.anc.mode {
                        self.saved_anc = Some((m, d.anc.level));
                    }
                    host.send_device_command(DeviceCommand::SetAnc { mode: AncMode::Transparency, level: None });
                    host.notify("Микрофон активен", "Включена прозрачность на время звонка");
                } else if !active && self.mic_was_active {
                    if let Some((m, l)) = self.saved_anc.take() {
                        host.send_device_command(DeviceCommand::SetAnc { mode: m, level: l });
                    }
                }
            }
            self.mic_was_active = active;
        } else {
            self.mic_was_active = false;
            self.saved_anc = None;
        }
        // --- fullscreen game → low latency
        if s.auto_low_latency_in_games {
            let game = mic_monitor::game_running().await;
            if let Some(d) = &dev {
                if d.caps.low_latency {
                    if game && !self.game_was_running && d.low_latency != Some(true) {
                        host.send_device_command(DeviceCommand::SetLowLatency { enabled: true });
                        self.we_enabled_low_latency = true;
                        host.notify("Игра запущена", "Включён игровой режим (низкая задержка)");
                    } else if !game && self.game_was_running && self.we_enabled_low_latency {
                        host.send_device_command(DeviceCommand::SetLowLatency { enabled: false });
                        self.we_enabled_low_latency = false;
                    }
                }
            }
            self.game_was_running = game;
        } else {
            self.game_was_running = false;
            self.we_enabled_low_latency = false;
        }
    }
}

pub fn spawn(host: Arc<dyn AutomationHost>, mut rx: mpsc::UnboundedReceiver<AutoEvent>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut c = Coordinator::default();
        let mut tick = tokio::time::interval(Duration::from_millis(1500));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                ev = rx.recv() => match ev {
                    Some(AutoEvent::Wear { before, after }) => c.on_wear(host.as_ref(), before, after).await,
                    None => break,
                },
                _ = tick.tick() => c.poll(host.as_ref()).await,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_detection() {
        assert!(removed(WearPosition::Ear, WearPosition::Out));
        assert!(removed(WearPosition::Ear, WearPosition::Case));
        assert!(!removed(WearPosition::Out, WearPosition::Ear));
        assert!(!removed(WearPosition::Ear, WearPosition::Unknown));
    }
}

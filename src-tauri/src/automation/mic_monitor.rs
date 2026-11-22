//! Activity detectors used by automation:
//! * microphone capture in progress (PipeWire `Stream/Input/Audio` nodes in state running on
//!   Linux; an active WASAPI capture session on any capture endpoint on Windows);
//! * a fullscreen game / Steam title running.

pub async fn microphone_active() -> bool {
    #[cfg(target_os = "linux")]
    {
        return tokio::task::spawn_blocking(linux::mic_active).await.unwrap_or(false);
    }
    #[cfg(windows)]
    {
        return tokio::task::spawn_blocking(win::mic_active).await.unwrap_or(false);
    }
    #[allow(unreachable_code)]
    false
}

pub async fn game_running() -> bool {
    #[cfg(target_os = "linux")]
    {
        return tokio::task::spawn_blocking(linux::game_running).await.unwrap_or(false);
    }
    #[cfg(windows)]
    {
        return tokio::task::spawn_blocking(win::fullscreen_app).await.unwrap_or(false);
    }
    #[allow(unreachable_code)]
    false
}

#[cfg(target_os = "linux")]
pub mod linux {
    use std::process::{Command, Stdio};

    /// Parse `pw-dump` output for capture streams that are running (peak meters excluded).
    pub fn mic_streams_running(dump: &serde_json::Value) -> bool {
        dump.as_array()
            .map(|objs| {
                objs.iter().any(|o| {
                    let info = match o.get("info") {
                        Some(i) => i,
                        None => return false,
                    };
                    let props = info.get("props");
                    let class = props.and_then(|p| p.get("media.class")).and_then(|v| v.as_str()).unwrap_or("");
                    let state = info.get("state").and_then(|v| v.as_str()).unwrap_or("");
                    let monitor = props.and_then(|p| p.get("stream.monitor")).and_then(|v| v.as_bool()).unwrap_or(false);
                    let name = props.and_then(|p| p.get("application.name")).and_then(|v| v.as_str()).unwrap_or("");
                    class == "Stream/Input/Audio" && state == "running" && !monitor && !name.contains("pavucontrol") && !name.contains("BudsPulse")
                })
            })
            .unwrap_or(false)
    }

    pub fn mic_active() -> bool {
        let Ok(out) = Command::new("pw-dump").arg("-N").stderr(Stdio::null()).output() else { return false };
        serde_json::from_slice::<serde_json::Value>(&out.stdout).map(|v| mic_streams_running(&v)).unwrap_or(false)
    }

    /// Steam games run under `reaper SteamLaunch AppId=…`; gamescope sessions count too.
    pub fn game_running() -> bool {
        let Ok(dir) = std::fs::read_dir("/proc") else { return false };
        for e in dir.flatten() {
            let name = e.file_name();
            let Some(pid) = name.to_str() else { continue };
            if !pid.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let Ok(cmd) = std::fs::read(format!("/proc/{pid}/cmdline")) else { continue };
            let cmd = String::from_utf8_lossy(&cmd).replace('\0', " ");
            if (cmd.contains("SteamLaunch") && cmd.contains("AppId=")) || cmd.starts_with("gamescope ") || cmd.contains("/gamescope ") {
                return true;
            }
        }
        false
    }
}

#[cfg(windows)]
pub mod win {
    use windows::core::Interface;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::Media::Audio::{
        eCapture, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowRect};

    pub fn mic_active() -> bool {
        // SAFETY: COM calls on a worker thread with MTA initialised; all pointers are owned wrappers
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return false };
            let Ok(devs) = en.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else { return false };
            let n = devs.GetCount().unwrap_or(0);
            let me = std::process::id();
            for i in 0..n {
                let Ok(dev) = devs.Item(i) else { continue };
                let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
                let Ok(list) = mgr.GetSessionEnumerator() else { continue };
                let count = list.GetCount().unwrap_or(0);
                for j in 0..count {
                    let Ok(ctl) = list.GetSession(j) else { continue };
                    if ctl.GetState().map(|s| s == AudioSessionStateActive).unwrap_or(false) {
                        let pid = ctl.cast::<IAudioSessionControl2>().and_then(|c2| c2.GetProcessId()).unwrap_or(0);
                        if pid != me {
                            return true;
                        }
                    }
                }
            }
            false
        }
    }

    pub fn fullscreen_app() -> bool {
        // SAFETY: plain Win32 queries
        unsafe {
            if let Ok(st) = SHQueryUserNotificationState() {
                if st == QUNS_RUNNING_D3D_FULL_SCREEN {
                    return true;
                }
            }
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return false;
            }
            let mut cls = [0u16; 64];
            let n = GetClassNameW(hwnd, &mut cls) as usize;
            let class = String::from_utf16_lossy(&cls[..n]);
            if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Windows.UI.Core.CoreWindow") {
                return false;
            }
            let mut r = RECT::default();
            if GetWindowRect(hwnd, &mut r).is_err() {
                return false;
            }
            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if !GetMonitorInfoW(mon, &mut mi).as_bool() {
                return false;
            }
            let m = mi.rcMonitor;
            r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
        }
    }
}

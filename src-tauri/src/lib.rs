//! BudsPulse — tray widget & companion app for Nothing, CMF and OnePlus earbuds.

pub mod audio_eq;
pub mod automation;
pub mod bt;
pub mod protocols;
pub mod state;
pub mod tray;

use crate::audio_eq::{HostEqProfile, HostEqStatus};
use crate::bt::simulator::{sim_action as sim_send, SimAction};
use crate::protocols::{AncMode, ConnectionState, DeviceCommand};
use crate::state::{Core, PacketLog, Settings, Snapshot};
use crate::tray::TrayAction;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State, WebviewWindow, WindowEvent};

type CoreState<'a> = State<'a, Arc<Core>>;

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

#[tauri::command]
fn get_snapshot(core: CoreState<'_>) -> Snapshot {
    core.snapshot()
}

#[tauri::command]
fn send_command(core: CoreState<'_>, cmd: DeviceCommand) -> Result<(), String> {
    core.send_command(cmd)
}

#[tauri::command]
async fn connect_device(core: CoreState<'_>, id: String) -> Result<(), String> {
    let core = core.inner().clone();
    core.connect_paired(&id).await
}

#[tauri::command]
async fn disconnect_device(core: CoreState<'_>) -> Result<(), String> {
    let core = core.inner().clone();
    core.stop_session().await;
    {
        let mut d = core.data.lock();
        d.demo = false;
        if let Some(dev) = d.device.as_mut() {
            dev.connection = ConnectionState::Disconnected;
            dev.transport = None;
            dev.pending.clear();
        }
        d.settings.last_device = None;
    }
    core.save();
    core.emit_snapshot();
    Ok(())
}

/// Removes the device from the OS list of paired Bluetooth devices.
#[tauri::command]
async fn unpair_device(core: CoreState<'_>, id: String) -> Result<(), String> {
    let core = core.inner().clone();
    core.unpair(&id).await
}

#[tauri::command]
async fn rescan_devices(core: CoreState<'_>) -> Result<(), String> {
    let core = core.inner().clone();
    core.rescan().await;
    Ok(())
}

#[tauri::command]
async fn set_demo(core: CoreState<'_>, enabled: bool, model: Option<String>) -> Result<(), String> {
    let core = core.inner().clone();
    if enabled {
        core.start_demo(model.as_deref()).await;
    } else {
        core.stop_demo().await;
    }
    Ok(())
}

#[tauri::command]
fn set_settings(app: AppHandle, core: CoreState<'_>, settings: Settings) -> Result<(), String> {
    let autostart_changed = {
        let mut d = core.data.lock();
        let changed = d.settings.autostart != settings.autostart;
        let last = d.settings.last_device.clone();
        d.settings = Settings { last_device: last, ..settings.clone() };
        changed
    };
    if autostart_changed {
        use tauri_plugin_autostart::ManagerExt;
        let al = app.autolaunch();
        let r = if settings.autostart { al.enable() } else { al.disable() };
        r.map_err(|e| format!("автозапуск: {e}"))?;
    }
    core.save();
    core.emit_snapshot();
    Ok(())
}

#[tauri::command]
fn set_host_eq(core: CoreState<'_>, profile: HostEqProfile) -> HostEqStatus {
    core.set_host_eq(profile)
}

#[tauri::command]
fn install_host_eq(core: CoreState<'_>) -> HostEqStatus {
    core.install_host_eq()
}

#[tauri::command]
fn parse_peq_text(text: String) -> HostEqProfile {
    audio_eq::parse_apo_text(&text)
}

#[tauri::command]
fn export_peq_text(profile: HostEqProfile) -> String {
    audio_eq::to_apo_text(&profile)
}

#[tauri::command]
fn get_packets(core: CoreState<'_>) -> Vec<PacketLog> {
    core.packets()
}

#[tauri::command]
fn clear_packets(core: CoreState<'_>) {
    core.clear_packets()
}

#[tauri::command]
fn export_diagnostics(core: CoreState<'_>) -> Result<String, String> {
    let dir = dirs::download_dir().or_else(dirs::desktop_dir).or_else(dirs::home_dir).ok_or("не найдена папка для сохранения")?;
    let path = dir.join(format!("budspulse-diagnostics-{}.json", protocols::now_ms() / 1000));
    let text = serde_json::to_string_pretty(&core.diagnostics_json()).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn sim_action(action: SimAction) -> Result<(), String> {
    if sim_send(action) {
        Ok(())
    } else {
        Err("симулятор не запущен".into())
    }
}

#[tauri::command]
fn hide_popover(window: WebviewWindow) {
    let _ = window.hide();
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    shutdown(&app);
    app.exit(0);
}

/// Opens the OS Bluetooth settings so the user can connect the earbuds to the PC.
#[tauri::command]
fn open_bluetooth_settings() -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:bluetooth"])
            .creation_flags(0x0800_0000)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let candidates: [&[&str]; 5] = [
            &["kcmshell6", "kcm_bluetooth"],
            &["systemsettings", "kcm_bluetooth"],
            &["gnome-control-center", "bluetooth"],
            &["blueman-manager"],
            &["blueberry"],
        ];
        for c in candidates {
            if std::process::Command::new(c[0]).args(&c[1..]).spawn().is_ok() {
                return Ok(());
            }
        }
        Err("не найдено приложение настроек Bluetooth (KDE / GNOME / Blueman)".into())
    }
}

// ---------------------------------------------------------------------------------------------
// Popover window
// ---------------------------------------------------------------------------------------------

static LAST_BLUR_HIDE: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_SHOW: Mutex<Option<Instant>> = Mutex::new(None);

/// Anchor the popover to the tray (Windows, once the tray position is known) and always keep it
/// inside the monitor work area (never under the taskbar / panel).
fn position_popover(w: &WebviewWindow) {
    use tauri_plugin_positioner::{Position, WindowExt};
    let tray_ok = cfg!(windows) && w.as_ref().window().move_window(Position::TrayCenter).is_ok();
    let Some(mon) = w.current_monitor().ok().flatten().or_else(|| w.primary_monitor().ok().flatten()) else { return };
    let wa = mon.work_area();
    let size = w.outer_size().unwrap_or(tauri::PhysicalSize::new(380, 620));
    let margin = (12.0 * mon.scale_factor()) as i32;
    let (min_x, min_y) = (wa.position.x + margin, wa.position.y + margin);
    let max_x = wa.position.x + wa.size.width as i32 - size.width as i32 - margin;
    let max_y = wa.position.y + wa.size.height as i32 - size.height as i32 - margin;
    let (x, y) = match (tray_ok, w.outer_position()) {
        (true, Ok(p)) => (p.x, p.y),
        // Windows: bottom-right above the taskbar; Linux (KDE/GNOME panels on top): top-right
        _ if cfg!(windows) => (max_x, max_y),
        _ => (max_x, min_y),
    };
    let _ = w.set_position(tauri::PhysicalPosition::new(x.clamp(min_x, max_x.max(min_x)), y.clamp(min_y, max_y.max(min_y))));
}

pub fn show_popover(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        position_popover(&w);
        *LAST_SHOW.lock() = Some(Instant::now());
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Popover auto-hides on blur, except right after it was shown (a launch from Explorer/autostart may
/// not get foreground rights and would otherwise vanish instantly) or when pinned for debugging.
fn should_hide_on_blur() -> bool {
    if cfg!(debug_assertions) || std::env::var_os("BUDSPULSE_PIN").is_some() {
        return false;
    }
    !LAST_SHOW.lock().map(|t| t.elapsed() < Duration::from_millis(1500)).unwrap_or(false)
}

pub fn toggle_popover(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    // clicking the tray icon blurs (and hides) the popover first — don't immediately re-open it
    if LAST_BLUR_HIDE.lock().map(|t| t.elapsed() < Duration::from_millis(300)).unwrap_or(false) {
        return;
    }
    show_popover(app);
}

fn shutdown(app: &AppHandle) {
    if let Some(core) = app.try_state::<Arc<Core>>() {
        core.save();
        core.eq.lock().shutdown();
        let c = core.inner().clone();
        tauri::async_runtime::block_on(async move {
            let _ = tokio::time::timeout(Duration::from_secs(2), c.stop_session()).await;
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Tray
// ---------------------------------------------------------------------------------------------

fn handle_tray_action(app: &AppHandle, a: TrayAction) {
    let Some(core) = app.try_state::<Arc<Core>>().map(|c| c.inner().clone()) else { return };
    let dev = core.device_snapshot();
    let connected = dev.as_ref().map(|d| d.connection == ConnectionState::Connected).unwrap_or(false);
    let send = |c: DeviceCommand| {
        if let Err(e) = core.send_command(c) {
            core.notify("BudsPulse", &e);
        }
    };
    match a {
        TrayAction::TogglePopover => toggle_popover(app),
        TrayAction::ShowPopover => show_popover(app),
        TrayAction::CycleAnc if connected => {
            let d = dev.unwrap();
            let cmd = state::cycle_anc(&d);
            let label = match &cmd {
                DeviceCommand::SetAnc { mode: AncMode::Transparency, .. } => "Прозрачность",
                _ => "Шумоподавление",
            };
            send(cmd);
            core.notify(&d.name, &format!("Режим: {label}"));
        }
        TrayAction::Anc if connected => {
            let lvl = dev.as_ref().and_then(|d| d.anc.level.or(d.caps.anc_levels.first().copied()));
            send(DeviceCommand::SetAnc { mode: AncMode::Anc, level: lvl })
        }
        TrayAction::Transparency if connected => send(DeviceCommand::SetAnc { mode: AncMode::Transparency, level: None }),
        TrayAction::AncOff if connected => send(DeviceCommand::SetAnc { mode: AncMode::Off, level: None }),
        TrayAction::ToggleGameMode if connected => {
            let on = dev.and_then(|d| d.low_latency).unwrap_or(false);
            send(DeviceCommand::SetLowLatency { enabled: !on })
        }
        TrayAction::FindBuds if connected => {
            show_popover(app);
            let _ = tauri::Emitter::emit(app, "navigate", "device");
        }
        TrayAction::ToggleDemo => {
            let demo = core.data.lock().demo;
            let model = core.data.lock().settings.demo_model.clone();
            tauri::async_runtime::spawn(async move {
                if demo {
                    core.stop_demo().await;
                } else {
                    core.start_demo(model.as_deref()).await;
                }
            });
        }
        TrayAction::Quit => {
            shutdown(app);
            app.exit(0);
        }
        _ => {}
    }
}

#[cfg(windows)]
fn setup_tray(app: &AppHandle, core: &Arc<Core>) -> tauri::Result<()> {
    use tauri::image::Image;
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let open = MenuItem::with_id(app, "show", "Открыть BudsPulse", true, None::<&str>)?;
    let anc = MenuItem::with_id(app, "anc", "Шумоподавление", true, None::<&str>)?;
    let tr = MenuItem::with_id(app, "transparency", "Прозрачность", true, None::<&str>)?;
    let off = MenuItem::with_id(app, "off", "Выкл", true, None::<&str>)?;
    let game = CheckMenuItem::with_id(app, "game", "Игровой режим", true, false, None::<&str>)?;
    let find = MenuItem::with_id(app, "find", "Найти наушники", true, None::<&str>)?;
    let demo = CheckMenuItem::with_id(app, "demo", "Демо-режим", true, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &sep1, &anc, &tr, &off, &game, &find, &sep2, &demo, &quit])?;

    let v = tray::view(None, false);
    TrayIconBuilder::with_id("main")
        .icon(Image::new_owned(v.rgba, tray::ICON_SIZE, tray::ICON_SIZE))
        .tooltip(&v.tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| {
            let a = match ev.id().as_ref() {
                "show" => TrayAction::ShowPopover,
                "anc" => TrayAction::Anc,
                "transparency" => TrayAction::Transparency,
                "off" => TrayAction::AncOff,
                "game" => TrayAction::ToggleGameMode,
                "find" => TrayAction::FindBuds,
                "demo" => TrayAction::ToggleDemo,
                "quit" => TrayAction::Quit,
                _ => return,
            };
            handle_tray_action(app, a);
        })
        .on_tray_icon_event(|tray, ev| {
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &ev);
            if let TrayIconEvent::Click { button, button_state: MouseButtonState::Up, .. } = ev {
                match button {
                    MouseButton::Left => handle_tray_action(tray.app_handle(), TrayAction::TogglePopover),
                    MouseButton::Middle => handle_tray_action(tray.app_handle(), TrayAction::CycleAnc),
                    _ => {}
                }
            }
        })
        .build(app)?;

    let app2 = app.clone();
    let last: Mutex<Option<tray::TrayView>> = Mutex::new(None);
    *core.on_change.lock() = Some(Box::new(move |dev, demo_on| {
        let v = tray::view(dev, demo_on);
        {
            let mut l = last.lock();
            if l.as_ref() == Some(&v) {
                return;
            }
            *l = Some(v.clone());
        }
        let _ = game.set_checked(v.game);
        let _ = demo.set_checked(v.demo);
        for item in [&anc, &tr, &off, &find] {
            let _ = item.set_enabled(v.connected);
        }
        let _ = game.set_enabled(v.connected);
        if let Some(t) = app2.tray_by_id("main") {
            let _ = t.set_icon(Some(Image::new_owned(v.rgba, tray::ICON_SIZE, tray::ICON_SIZE)));
            let _ = t.set_tooltip(Some(&v.tooltip));
        }
    }));
    Ok(())
}

#[cfg(target_os = "linux")]
fn setup_tray(app: &AppHandle, core: &Arc<Core>) -> tauri::Result<()> {
    let app2 = app.clone();
    let handler: tray::TrayHandler = Arc::new(move |a| {
        let app3 = app2.clone();
        let _ = app2.run_on_main_thread(move || handle_tray_action(&app3, a));
    });
    let handle: Arc<Mutex<Option<tray::sni::Handle>>> = Arc::new(Mutex::new(None));
    let h2 = handle.clone();
    tauri::async_runtime::spawn(async move {
        match tray::sni::spawn(tray::view(None, false), handler).await {
            Ok(h) => *h2.lock() = Some(h),
            Err(e) => log::error!("tray (StatusNotifierItem) unavailable: {e}"),
        }
    });
    let last: Mutex<Option<tray::TrayView>> = Mutex::new(None);
    *core.on_change.lock() = Some(Box::new(move |dev, demo_on| {
        let v = tray::view(dev, demo_on);
        {
            let mut l = last.lock();
            if l.as_ref() == Some(&v) {
                return;
            }
            *l = Some(v.clone());
        }
        let h = handle.lock().clone();
        if let Some(h) = h {
            tauri::async_runtime::spawn(async move { tray::sni::update(&h, v).await });
        }
    }));
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn setup_tray(_app: &AppHandle, _core: &Arc<Core>) -> tauri::Result<()> {
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------------------------

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,budspulse_lib=debug")).init();
    let args: Vec<String> = std::env::args().collect();
    let demo_flag = args.iter().any(|a| a == "--demo");
    let demo_model = args.iter().position(|a| a == "--demo-model").and_then(|i| args.get(i + 1).cloned());
    let start_hidden = args.iter().any(|a| a == "--minimized");
    // developer aid: open a given tab on start (dashboard | sound | gestures | device)
    let start_tab = args.iter().position(|a| a == "--tab").and_then(|i| args.get(i + 1).cloned());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| show_popover(app)))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            send_command,
            connect_device,
            disconnect_device,
            unpair_device,
            rescan_devices,
            set_demo,
            set_settings,
            set_host_eq,
            install_host_eq,
            parse_peq_text,
            export_peq_text,
            get_packets,
            clear_packets,
            export_diagnostics,
            sim_action,
            hide_popover,
            open_bluetooth_settings,
            quit_app
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let core = Core::new(handle.clone());
            app.manage(core.clone());
            setup_tray(&handle, &core)?;
            if let Some(rx) = core.take_automation_rx() {
                let host: Arc<dyn automation::AutomationHost> = core.clone();
                tauri::async_runtime::spawn(async move {
                    automation::spawn(host, rx);
                });
            }
            // re-apply the saved host EQ so it survives restarts
            {
                // autostart entry points at the exe path: refresh it in case the app was moved,
                // and reflect the real OS state if the user removed it elsewhere
                use tauri_plugin_autostart::ManagerExt;
                let want = core.data.lock().settings.autostart;
                let al = handle.autolaunch();
                if want {
                    let _ = al.enable();
                } else if al.is_enabled().unwrap_or(false) {
                    core.data.lock().settings.autostart = true;
                }
            }
            {
                let profile = core.data.lock().host_eq.clone();
                if profile.enabled {
                    let st = core.set_host_eq(profile);
                    core.data.lock().host_eq_status = st;
                }
            }
            let c2 = core.clone();
            let model = demo_model.clone();
            tauri::async_runtime::spawn(async move {
                if demo_flag || model.is_some() {
                    c2.start_demo(model.as_deref()).await;
                } else {
                    c2.auto_connect().await;
                }
                // keep the paired list fresh and follow whichever supported earbuds the OS has
                // connected (a local OS query — the earbuds themselves are not paged)
                loop {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    let idle = c2.session.lock().await.is_none();
                    if idle {
                        c2.auto_connect().await;
                    } else {
                        c2.rescan().await;
                        c2.follow_os_connection().await;
                    }
                }
            });
            if !start_hidden {
                show_popover(&handle);
            }
            if let Some(tab) = start_tab.clone() {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(2500)).await;
                    let _ = tauri::Emitter::emit(&h, "navigate", tab);
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(false) if window.label() == "main" => {
                // popover behaviour: hide when focus leaves (not in debug builds, to allow devtools)
                if should_hide_on_blur() {
                    *LAST_BLUR_HIDE.lock() = Some(Instant::now());
                    let _ = window.hide();
                }
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while building BudsPulse")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = &event {
                // keep running in the tray when windows close; only explicit quit exits
                if code.is_none() {
                    api.prevent_exit();
                }
            }
            let _ = app;
        });
}


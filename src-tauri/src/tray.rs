//! Dynamic tray icon renderer (two battery bars + ANC status dot), tooltip text and the
//! platform tray backends: Tauri `TrayIconBuilder` on Windows (Win32 notification area) and a
//! native StatusNotifierItem via `ksni` on Linux (KDE Plasma / GNOME AppIndicator), which —
//! unlike libappindicator — delivers left / middle click events.

use crate::protocols::{AncLevel, AncMode, ConnectionState, DeviceState};
use std::sync::Arc;

pub const ICON_SIZE: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    TogglePopover,
    ShowPopover,
    CycleAnc,
    Anc,
    Transparency,
    AncOff,
    ToggleGameMode,
    FindBuds,
    ToggleDemo,
    Quit,
}

pub type TrayHandler = Arc<dyn Fn(TrayAction) + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
pub struct TrayView {
    pub rgba: Vec<u8>,
    pub tooltip: String,
    pub demo: bool,
    pub game: bool,
    pub connected: bool,
}

pub fn view(device: Option<&DeviceState>, demo: bool) -> TrayView {
    TrayView {
        rgba: render_icon(device),
        tooltip: tooltip(device),
        demo,
        game: device.and_then(|d| d.low_latency).unwrap_or(false),
        connected: device.map(|d| d.connection == ConnectionState::Connected).unwrap_or(false),
    }
}

fn level_label(l: Option<AncLevel>) -> &'static str {
    match l {
        Some(AncLevel::High) => "High",
        Some(AncLevel::Mid) => "Mid",
        Some(AncLevel::Low) => "Low",
        Some(AncLevel::Smart) => "Smart",
        Some(AncLevel::Adaptive) => "Adaptive",
        None => "On",
    }
}

pub fn tooltip(device: Option<&DeviceState>) -> String {
    let Some(d) = device else { return "BudsPulse — наушники не выбраны".into() };
    if d.connection != ConnectionState::Connected {
        let st = match d.connection {
            ConnectionState::Connecting | ConnectionState::Handshake => "подключение…",
            ConnectionState::Error => "нет связи",
            _ => "отключено",
        };
        return format!("{} • {st}", d.name);
    }
    let pct = |v: Option<u8>, ch: bool| match v {
        Some(v) => format!("{v}%{}", if ch { "⚡" } else { "" }),
        None => "—".into(),
    };
    let b = &d.battery;
    let mut s = format!("{} • L: {} | R: {}", d.name, pct(b.left.level, b.left.charging), pct(b.right.level, b.right.charging));
    if b.case.level.is_some() {
        s.push_str(&format!(" | Case: {}", pct(b.case.level, b.case.charging)));
    }
    match d.anc.mode {
        Some(AncMode::Anc) => s.push_str(&format!(" • ANC: {}", level_label(d.anc.level))),
        Some(AncMode::Transparency) => s.push_str(" • Прозрачность"),
        Some(AncMode::Off) => s.push_str(" • ANC: Off"),
        None => {}
    }
    if let Some(c) = &d.codec {
        s.push_str(&format!(" • {c}"));
    }
    // Windows limits tooltips to 127 chars
    s.chars().take(127).collect()
}

// ---------------------------------------------------------------------------------------------
// Software rasteriser (4×4 supersampling)
// ---------------------------------------------------------------------------------------------

struct Canvas {
    w: u32,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(w: u32) -> Self {
        Canvas { w, px: vec![[0.0; 4]; (w * w) as usize] }
    }

    /// Paint `color` where `inside(x, y)` (sub-pixel coords) holds, with coverage AA.
    fn fill(&mut self, color: [f32; 4], inside: impl Fn(f32, f32) -> bool) {
        const S: u32 = 4;
        for y in 0..self.w {
            for x in 0..self.w {
                let mut hits = 0;
                for sy in 0..S {
                    for sx in 0..S {
                        let fx = x as f32 + (sx as f32 + 0.5) / S as f32;
                        let fy = y as f32 + (sy as f32 + 0.5) / S as f32;
                        if inside(fx, fy) {
                            hits += 1;
                        }
                    }
                }
                if hits == 0 {
                    continue;
                }
                let a = color[3] * hits as f32 / (S * S) as f32;
                let p = &mut self.px[(y * self.w + x) as usize];
                let out_a = a + p[3] * (1.0 - a);
                for c in 0..3 {
                    p[c] = if out_a > 0.0 { (color[c] * a + p[c] * p[3] * (1.0 - a)) / out_a } else { 0.0 };
                }
                p[3] = out_a;
            }
        }
    }

    fn rgba(&self) -> Vec<u8> {
        self.px.iter().flat_map(|p| [p[0], p[1], p[2], p[3]].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect()
    }
}

fn rounded_rect(x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> impl Fn(f32, f32) -> bool {
    let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0).max(0.0);
    move |x, y| {
        if x < x0 || x > x1 || y < y0 || y > y1 {
            return false;
        }
        let cx = x.clamp(x0 + r, x1 - r);
        let cy = y.clamp(y0 + r, y1 - r);
        (x - cx).powi(2) + (y - cy).powi(2) <= r * r
    }
}

fn rgb(hex: u32, a: f32) -> [f32; 4] {
    [((hex >> 16) & 0xFF) as f32 / 255.0, ((hex >> 8) & 0xFF) as f32 / 255.0, (hex & 0xFF) as f32 / 255.0, a]
}

/// 32×32 RGBA icon: L/R battery bars filled bottom-up and a colour-coded status dot
/// (cyan = ANC, amber = transparency, grey = off, dim outline = disconnected).
pub fn render_icon(device: Option<&DeviceState>) -> Vec<u8> {
    let n = ICON_SIZE as f32;
    let mut c = Canvas::new(ICON_SIZE);
    let connected = device.map(|d| d.connection == ConnectionState::Connected).unwrap_or(false);
    let outline = if connected { rgb(0xF4F4F5, 1.0) } else { rgb(0x9CA3AF, 0.85) };
    let bar_w = n * 0.28;
    let gap = n * 0.10;
    let top = n * 0.06;
    let bottom = n * 0.80;
    let lx0 = n * 0.5 - gap / 2.0 - bar_w;
    let rx0 = n * 0.5 + gap / 2.0;
    let stroke = n * 0.07;
    for (i, x0) in [lx0, rx0].into_iter().enumerate() {
        let x1 = x0 + bar_w;
        // outline ring
        let outer = rounded_rect(x0, top, x1, bottom, bar_w * 0.35);
        let inner = rounded_rect(x0 + stroke, top + stroke, x1 - stroke, bottom - stroke, bar_w * 0.22);
        c.fill(outline, |x, y| outer(x, y) && !inner(x, y));
        // fill level
        let lvl = device
            .filter(|_| connected)
            .and_then(|d| if i == 0 { d.battery.left.level } else { d.battery.right.level })
            .map(|v| v as f32 / 100.0);
        if let Some(l) = lvl {
            let fill_col = if l <= 0.15 { rgb(0xEF4444, 1.0) } else if l <= 0.30 { rgb(0xF59E0B, 1.0) } else { rgb(0xF4F4F5, 1.0) };
            let inner_top = top + stroke * 1.6;
            let inner_bottom = bottom - stroke * 1.6;
            let y_start = inner_bottom - (inner_bottom - inner_top) * l.max(0.08);
            let body = rounded_rect(x0 + stroke * 1.6, y_start, x1 - stroke * 1.6, inner_bottom, bar_w * 0.12);
            c.fill(fill_col, body);
        }
    }
    // status dot
    let dot = device.filter(|_| connected).map(|d| match d.anc.mode {
        Some(AncMode::Anc) => rgb(0x22D3EE, 1.0),
        Some(AncMode::Transparency) => rgb(0xF59E0B, 1.0),
        _ => rgb(0x9CA3AF, 1.0),
    });
    let (cx, cy, r) = (n * 0.5, n * 0.90, n * 0.10);
    match dot {
        Some(col) => c.fill(col, move |x, y| (x - cx).powi(2) + (y - cy).powi(2) <= r * r),
        None => c.fill(rgb(0x9CA3AF, 0.6), move |x, y| {
            let d2 = (x - cx).powi(2) + (y - cy).powi(2);
            d2 <= r * r && d2 >= (r * 0.55).powi(2)
        }),
    }
    c.rgba()
}

// ---------------------------------------------------------------------------------------------
// Linux: StatusNotifierItem via ksni
// ---------------------------------------------------------------------------------------------

#[cfg(target_os = "linux")]
pub mod sni {
    use super::{TrayAction, TrayHandler, TrayView, ICON_SIZE};
    use ksni::TrayMethods;

    pub struct SniTray {
        pub view: TrayView,
        pub handler: TrayHandler,
    }

    impl ksni::Tray for SniTray {
        fn id(&self) -> String {
            "budspulse".into()
        }
        fn title(&self) -> String {
            "BudsPulse".into()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            // SNI wants ARGB32 in network byte order
            let data = self.view.rgba.chunks(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
            vec![ksni::Icon { width: ICON_SIZE as i32, height: ICON_SIZE as i32, data }]
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip { title: "BudsPulse".into(), description: self.view.tooltip.clone(), icon_name: String::new(), icon_pixmap: vec![] }
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            (self.handler)(TrayAction::TogglePopover);
        }
        fn secondary_activate(&mut self, _x: i32, _y: i32) {
            (self.handler)(TrayAction::CycleAnc);
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::*;
            let item = |label: &str, a: TrayAction| -> ksni::MenuItem<Self> {
                StandardItem { label: label.into(), enabled: true, activate: Box::new(move |t: &mut Self| (t.handler)(a)), ..Default::default() }.into()
            };
            let connected = self.view.connected;
            let dev = |label: &str, a: TrayAction| -> ksni::MenuItem<Self> {
                StandardItem { label: label.into(), enabled: connected, activate: Box::new(move |t: &mut Self| (t.handler)(a)), ..Default::default() }.into()
            };
            vec![
                item("Открыть BudsPulse", TrayAction::ShowPopover),
                MenuItem::Separator,
                dev("Шумоподавление", TrayAction::Anc),
                dev("Прозрачность", TrayAction::Transparency),
                dev("Выкл", TrayAction::AncOff),
                CheckmarkItem {
                    label: "Игровой режим".into(),
                    enabled: connected,
                    checked: self.view.game,
                    activate: Box::new(|t: &mut Self| (t.handler)(TrayAction::ToggleGameMode)),
                    ..Default::default()
                }
                .into(),
                dev("Найти наушники", TrayAction::FindBuds),
                MenuItem::Separator,
                CheckmarkItem {
                    label: "Демо-режим".into(),
                    checked: self.view.demo,
                    activate: Box::new(|t: &mut Self| (t.handler)(TrayAction::ToggleDemo)),
                    ..Default::default()
                }
                .into(),
                item("Выход", TrayAction::Quit),
            ]
        }
    }

    pub type Handle = ksni::Handle<SniTray>;

    pub async fn spawn(view: TrayView, handler: TrayHandler) -> Result<Handle, String> {
        SniTray { view, handler }.spawn().await.map_err(|e| e.to_string())
    }

    pub async fn update(h: &Handle, view: TrayView) {
        let _ = h.update(move |t: &mut SniTray| t.view = view).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocols::{identify_by_name, BudBattery};

    #[test]
    fn icon_and_tooltip() {
        let m = identify_by_name("OnePlus Buds Pro 4").unwrap();
        let mut d = DeviceState::new("x", "OnePlus Buds Pro 4", "00:00:00:00:00:00", &m);
        d.connection = ConnectionState::Connected;
        d.battery.left = BudBattery { level: Some(90), charging: false };
        d.battery.right = BudBattery { level: Some(85), charging: false };
        d.battery.case = BudBattery { level: Some(70), charging: false };
        d.anc.mode = Some(AncMode::Anc);
        d.anc.level = Some(AncLevel::High);
        d.codec = Some("LHDC 5.0".into());
        assert_eq!(tooltip(Some(&d)), "OnePlus Buds Pro 4 • L: 90% | R: 85% | Case: 70% • ANC: High • LHDC 5.0");
        let px = render_icon(Some(&d));
        assert_eq!(px.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        // status dot centre is cyan
        let (x, y) = (ICON_SIZE / 2, (ICON_SIZE as f32 * 0.9) as u32);
        let i = ((y * ICON_SIZE + x) * 4) as usize;
        assert!(px[i + 2] > 200 && px[i] < 80, "dot not cyan: {:?}", &px[i..i + 4]);
        assert!(render_icon(None).iter().skip(3).step_by(4).any(|a| *a > 0));
    }
}

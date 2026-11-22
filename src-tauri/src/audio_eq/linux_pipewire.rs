//! PipeWire integration (Linux).
//!
//! * Live mode: a standalone `pipewire -c <generated.conf>` child process hosts a
//!   `libpipewire-module-filter-chain` sink ("BudsPulse EQ") made of builtin biquads.
//!   Slider moves only change Freq/Q/Gain controls via `pw-cli set-param … Props`,
//!   so they are heard instantly with no reload. Changing the band layout (count/types)
//!   respawns the child (~100 ms).
//! * The filter sink is made the default sink (`wpctl set-default`) while active; its
//!   playback side targets the previously selected output (usually the BT headphones).
//! * "Install" additionally writes the same filter-chain as a persistent drop-in to
//!   `~/.config/pipewire/pipewire.conf.d/99-budspulse-eq.conf`.

use super::{EqBackendKind, FilterType, HostEqProfile, HostEqStatus, PeqBand};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const SINK_NODE: &str = "effect_input.budspulse_eq";
pub const OUT_NODE: &str = "effect_output.budspulse_eq";

pub struct PipeWireEq {
    state_dir: PathBuf,
    child: Option<Child>,
    layout: Vec<FilterType>,
    target_sink: Option<String>,
    node_id: Option<u32>,
    last_error: Option<String>,
    active: bool,
}

fn have(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

impl PipeWireEq {
    pub fn new(state_dir: PathBuf) -> Self {
        PipeWireEq { state_dir, child: None, layout: vec![], target_sink: None, node_id: None, last_error: None, active: false }
    }

    fn available(&self) -> bool {
        have("pipewire") && have("pw-cli") && have("pw-dump")
    }

    pub fn status(&self) -> HostEqStatus {
        let available = self.available();
        HostEqStatus {
            backend: EqBackendKind::Pipewire,
            available,
            active: self.active,
            message: if !available {
                "не найдены pipewire / pw-cli / pw-dump — установите пакет pipewire (pipewire-bin)".into()
            } else if let Some(e) = &self.last_error {
                e.clone()
            } else if self.active {
                format!(
                    "фильтр «BudsPulse EQ» активен → {}",
                    self.target_sink.clone().unwrap_or_else(|| "устройство по умолчанию".into())
                )
            } else {
                "готов — включите эквалайзер".into()
            },
            config_path: Some(self.live_conf_path().to_string_lossy().into_owned()),
        }
    }

    fn live_conf_path(&self) -> PathBuf {
        self.state_dir.join("pipewire-budspulse-eq.conf")
    }

    pub fn apply(&mut self, profile: &HostEqProfile, _device_hint: Option<&str>) -> HostEqStatus {
        if !self.available() {
            return self.status();
        }
        if !profile.enabled {
            self.stop();
            self.last_error = None;
            return self.status();
        }
        let layout: Vec<FilterType> = profile.bands.iter().map(|b| b.kind).collect();
        let child_alive = self.child.as_mut().map(|c| matches!(c.try_wait(), Ok(None))).unwrap_or(false);
        if !child_alive || layout != self.layout {
            if let Err(e) = self.spawn(profile) {
                self.last_error = Some(e);
                self.active = false;
                return self.status();
            }
            self.layout = layout;
        } else if let Err(e) = self.set_params(profile) {
            self.last_error = Some(e);
            return self.status();
        }
        self.last_error = None;
        self.active = true;
        self.status()
    }

    pub fn install(&mut self, profile: &HostEqProfile, device_hint: Option<&str>) -> HostEqStatus {
        let Some(home) = dirs::config_dir() else { return self.status() };
        let dir = home.join("pipewire").join("pipewire.conf.d");
        let target = self.target_sink.clone().or_else(default_sink_name);
        let text = format!(
            "# BudsPulse persistent EQ — remove this file to uninstall\ncontext.modules = [\n{}\n]\n",
            filter_chain_module(profile, target.as_deref())
        );
        let res = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("99-budspulse-eq.conf"), text));
        if let Err(e) = res {
            self.last_error = Some(format!("не удалось записать drop-in: {e}"));
            return self.status();
        }
        // live instance keeps working until PipeWire restarts; avoid duplicates afterwards
        let st = self.apply(profile, device_hint);
        HostEqStatus { message: format!("{} · drop-in установлен (~/.config/pipewire/pipewire.conf.d/99-budspulse-eq.conf)", st.message), ..st }
    }

    fn spawn(&mut self, profile: &HostEqProfile) -> Result<(), String> {
        let previous_default = default_sink_name();
        self.stop_child();
        // remember the "real" output (never our own sink)
        if let Some(d) = previous_default.filter(|d| d != SINK_NODE) {
            self.target_sink = Some(d);
        }
        std::fs::create_dir_all(&self.state_dir).map_err(|e| e.to_string())?;
        let conf = standalone_conf(profile, self.target_sink.as_deref());
        std::fs::write(self.live_conf_path(), conf).map_err(|e| format!("запись конфигурации: {e}"))?;
        let child = Command::new("pipewire")
            .arg("-c")
            .arg(self.live_conf_path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("не удалось запустить pipewire: {e}"))?;
        self.child = Some(child);
        // wait for the node to appear
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(id) = find_node_id(SINK_NODE) {
                self.node_id = Some(id);
                break;
            }
            if Instant::now() > deadline {
                return Err("узел BudsPulse EQ не появился в графе PipeWire".into());
            }
            std::thread::sleep(Duration::from_millis(80));
        }
        if let Some(id) = self.node_id {
            let _ = Command::new("wpctl").args(["set-default", &id.to_string()]).status();
        }
        Ok(())
    }

    fn set_params(&mut self, profile: &HostEqProfile) -> Result<(), String> {
        let id = match self.node_id.filter(|_| true) {
            Some(id) => id,
            None => find_node_id(SINK_NODE).ok_or("узел BudsPulse EQ не найден")?,
        };
        self.node_id = Some(id);
        let props = props_pod(profile);
        let st = Command::new("pw-cli")
            .args(["set-param", &id.to_string(), "Props", &props])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            self.node_id = None;
            return Err("pw-cli set-param завершился с ошибкой".into());
        }
        Ok(())
    }

    fn stop_child(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.node_id = None;
    }

    fn stop(&mut self) {
        let was_running = self.child.is_some();
        self.stop_child();
        self.active = false;
        if was_running {
            if let Some(t) = &self.target_sink {
                if let Some(id) = find_node_id(t) {
                    let _ = Command::new("wpctl").args(["set-default", &id.to_string()]).status();
                }
            }
        }
    }

    pub fn shutdown(&mut self) {
        self.stop();
    }
}

impl Drop for PipeWireEq {
    fn drop(&mut self) {
        self.stop_child();
    }
}

fn band_controls(b: &PeqBand) -> (f64, f64, f64) {
    // disabled bands become transparent (0 dB); BP/NO have no gain control
    let gain = if b.enabled { b.gain } else { 0.0 };
    (b.freq, b.q, gain)
}

/// `{ params = [ "preamp:Gain" -3.0 "eq_band_1:Freq" 100.0 … ] }`
pub fn props_pod(p: &HostEqProfile) -> String {
    let mut s = String::from("{ params = [ ");
    s.push_str(&format!("\"preamp:Gain\" {:.2} ", p.preamp));
    for (i, b) in p.bands.iter().enumerate() {
        let (f, q, g) = band_controls(b);
        let n = format!("eq_band_{}", i + 1);
        s.push_str(&format!("\"{n}:Freq\" {f:.1} \"{n}:Q\" {q:.3} "));
        if b.kind.has_gain() {
            s.push_str(&format!("\"{n}:Gain\" {g:.2} "));
        }
    }
    s.push_str("] }");
    s
}

/// The filter-chain module block (used by both live and persistent configs).
pub fn filter_chain_module(p: &HostEqProfile, target: Option<&str>) -> String {
    let mut nodes = String::new();
    nodes.push_str(&format!(
        "          {{ type = builtin name = preamp label = bq_highshelf control = {{ \"Freq\" = 0.0 \"Q\" = 1.0 \"Gain\" = {:.2} }} }}\n",
        p.preamp
    ));
    for (i, b) in p.bands.iter().enumerate() {
        let (f, q, g) = band_controls(b);
        let ctrl = if b.kind.has_gain() {
            format!("\"Freq\" = {f:.1} \"Q\" = {q:.3} \"Gain\" = {g:.2}")
        } else {
            format!("\"Freq\" = {f:.1} \"Q\" = {q:.3}")
        };
        nodes.push_str(&format!(
            "          {{ type = builtin name = eq_band_{} label = {} control = {{ {ctrl} }} }}\n",
            i + 1,
            b.kind.pipewire_label()
        ));
    }
    let mut links = String::new();
    let mut prev = "preamp".to_string();
    for i in 0..p.bands.len() {
        let cur = format!("eq_band_{}", i + 1);
        links.push_str(&format!("          {{ output = \"{prev}:Out\" input = \"{cur}:In\" }}\n"));
        prev = cur;
    }
    let target_line = target.map(|t| format!("          target.object = \"{}\"\n", t.replace('"', ""))).unwrap_or_default();
    format!(
        r#"  {{ name = libpipewire-module-filter-chain
    flags = [ nofail ]
    args = {{
      node.description = "BudsPulse EQ"
      media.name       = "BudsPulse EQ"
      filter.graph = {{
        nodes = [
{nodes}        ]
        links = [
{links}        ]
      }}
      audio.channels = 2
      audio.position = [ FL FR ]
      capture.props = {{
          node.name   = "{SINK_NODE}"
          media.class = Audio/Sink
      }}
      playback.props = {{
          node.name    = "{OUT_NODE}"
          node.passive = true
{target_line}      }}
    }}
  }}"#
    )
}

/// Complete config for a standalone `pipewire -c` process (mirrors /usr/share/pipewire/filter-chain.conf).
pub fn standalone_conf(p: &HostEqProfile, target: Option<&str>) -> String {
    format!(
        r#"# Generated by BudsPulse (live EQ instance)
context.properties = {{
    log.level = 0
}}
context.spa-libs = {{
    audio.convert.* = audioconvert/libspa-audioconvert
    support.*       = support/libspa-support
}}
context.modules = [
  {{ name = libpipewire-module-rt
    args = {{ nice.level = -11 }}
    flags = [ ifexists nofail ]
  }}
  {{ name = libpipewire-module-protocol-native }}
  {{ name = libpipewire-module-client-node }}
  {{ name = libpipewire-module-adapter }}
{}
]
"#,
        filter_chain_module(p, target)
    )
}

fn pw_dump() -> Option<serde_json::Value> {
    let out = Command::new("pw-dump").arg("-N").stderr(Stdio::null()).output().ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

pub fn find_node_id(node_name: &str) -> Option<u32> {
    let v = pw_dump()?;
    v.as_array()?.iter().find_map(|o| {
        let props = o.get("info")?.get("props")?;
        (props.get("node.name")?.as_str()? == node_name).then(|| o.get("id")?.as_u64().map(|x| x as u32))?
    })
}

/// `default.audio.sink` from the "default" metadata object.
pub fn default_sink_name() -> Option<String> {
    let v = pw_dump()?;
    for o in v.as_array()? {
        if o.get("type")?.as_str()? != "PipeWire:Interface:Metadata" {
            continue;
        }
        if o.get("props").and_then(|p| p.get("metadata.name")).and_then(|n| n.as_str()) != Some("default") {
            continue;
        }
        for m in o.get("metadata")?.as_array()? {
            if m.get("key").and_then(|k| k.as_str()) == Some("default.audio.sink") {
                return m.get("value")?.get("name")?.as_str().map(|s| s.to_string());
            }
        }
    }
    None
}

/// Active A2DP codec of the BT sink for `address` (e.g. "LDAC", "AAC").
pub fn bluez_codec(address: &str) -> Option<String> {
    let v = pw_dump()?;
    let want = address.to_ascii_uppercase();
    v.as_array()?.iter().find_map(|o| {
        let props = o.get("info")?.get("props")?;
        let addr = props.get("api.bluez5.address")?.as_str()?.to_ascii_uppercase();
        if addr != want {
            return None;
        }
        let codec = props.get("api.bluez5.codec")?.as_str()?;
        Some(pretty_codec(codec))
    })
}

pub fn pretty_codec(c: &str) -> String {
    match c.to_ascii_lowercase().as_str() {
        "sbc" => "SBC".into(),
        "sbc_xq" => "SBC-XQ".into(),
        "aac" | "aac_eld" => "AAC".into(),
        "ldac" => "LDAC".into(),
        "aptx" => "aptX".into(),
        "aptx_hd" => "aptX HD".into(),
        "aptx_ll" => "aptX LL".into(),
        "lc3" => "LC3".into(),
        "lc3plus_hr" => "LC3plus".into(),
        "opus_05" | "opus_g" => "Opus".into(),
        "msbc" | "cvsd" => "HFP".into(),
        other if other.starts_with("lhdc") => {
            if other.contains("v5") {
                "LHDC 5.0".into()
            } else {
                "LHDC".into()
            }
        }
        other => other.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conf_contains_all_nodes_and_links() {
        let mut p = HostEqProfile::default();
        p.enabled = true;
        p.preamp = -3.0;
        p.bands[2].gain = 4.0;
        let c = standalone_conf(&p, Some("bluez_output.2C_BE_EB_75_61_09.1"));
        assert!(c.contains("label = bq_lowshelf"));
        assert!(c.contains("label = bq_highshelf control = { \"Freq\" = 0.0"));
        assert!(c.contains("eq_band_10"));
        assert!(c.contains("{ output = \"eq_band_9:Out\" input = \"eq_band_10:In\" }"));
        assert!(c.contains("target.object = \"bluez_output.2C_BE_EB_75_61_09.1\""));
        let pod = props_pod(&p);
        assert!(pod.starts_with("{ params = [ \"preamp:Gain\" -3.00"));
        assert!(pod.contains("\"eq_band_3:Gain\" 4.00"));
    }
}

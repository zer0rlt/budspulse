//! Host-side (PC) parametric equalizer: RBJ biquad maths, Equalizer-APO text format,
//! and the platform backends that make the EQ actually alter system audio
//! (PipeWire filter-chain on Linux, Equalizer APO on Windows).

#[cfg(target_os = "linux")]
pub mod linux_pipewire;
#[cfg(windows)]
pub mod windows_apo;

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

pub const SAMPLE_RATE: f64 = 48_000.0;
pub const MAX_BANDS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterType {
    #[serde(rename = "PK")]
    Peaking,
    #[serde(rename = "LSC")]
    LowShelf,
    #[serde(rename = "HSC")]
    HighShelf,
    #[serde(rename = "BP")]
    BandPass,
    #[serde(rename = "NO")]
    Notch,
}

impl FilterType {
    pub fn apo_code(self) -> &'static str {
        match self {
            FilterType::Peaking => "PK",
            FilterType::LowShelf => "LSC",
            FilterType::HighShelf => "HSC",
            FilterType::BandPass => "BP",
            FilterType::Notch => "NO",
        }
    }
    pub fn from_apo(code: &str) -> Option<Self> {
        Some(match code.to_ascii_uppercase().as_str() {
            "PK" | "PEQ" => FilterType::Peaking,
            "LSC" | "LS" | "LSQ" => FilterType::LowShelf,
            "HSC" | "HS" | "HSQ" => FilterType::HighShelf,
            "BP" => FilterType::BandPass,
            "NO" => FilterType::Notch,
            _ => return None,
        })
    }
    pub fn has_gain(self) -> bool {
        !matches!(self, FilterType::BandPass | FilterType::Notch)
    }
    /// PipeWire filter-chain builtin plugin label.
    pub fn pipewire_label(self) -> &'static str {
        match self {
            FilterType::Peaking => "bq_peaking",
            FilterType::LowShelf => "bq_lowshelf",
            FilterType::HighShelf => "bq_highshelf",
            FilterType::BandPass => "bq_bandpass",
            FilterType::Notch => "bq_notch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeqBand {
    pub enabled: bool,
    #[serde(rename = "type")]
    pub kind: FilterType,
    pub freq: f64,
    pub gain: f64,
    pub q: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEqProfile {
    pub name: String,
    pub enabled: bool,
    pub preamp: f64,
    pub auto_preamp: bool,
    pub bands: Vec<PeqBand>,
}

impl Default for HostEqProfile {
    fn default() -> Self {
        let freqs = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
        let n = freqs.len();
        HostEqProfile {
            name: "Flat".into(),
            enabled: false,
            preamp: 0.0,
            auto_preamp: true,
            bands: freqs
                .iter()
                .enumerate()
                .map(|(i, &f)| PeqBand {
                    enabled: true,
                    kind: if i == 0 {
                        FilterType::LowShelf
                    } else if i == n - 1 {
                        FilterType::HighShelf
                    } else {
                        FilterType::Peaking
                    },
                    freq: f,
                    gain: 0.0,
                    q: if i == 0 || i == n - 1 { 0.7 } else { 1.41 },
                })
                .collect(),
        }
    }
}

impl HostEqProfile {
    /// Clamp all values into safe ranges (also guards against malformed imports).
    pub fn sanitized(mut self) -> Self {
        self.bands.truncate(MAX_BANDS);
        for b in &mut self.bands {
            b.freq = b.freq.clamp(20.0, 20_000.0);
            b.gain = b.gain.clamp(-20.0, 20.0);
            b.q = b.q.clamp(0.1, 10.0);
            if !b.freq.is_finite() {
                b.freq = 1000.0;
            }
            if !b.gain.is_finite() {
                b.gain = 0.0;
            }
            if !b.q.is_finite() {
                b.q = 1.0;
            }
        }
        self.preamp = if self.preamp.is_finite() { self.preamp.clamp(-30.0, 10.0) } else { 0.0 };
        if self.auto_preamp {
            self.preamp = auto_preamp(&self.bands);
        }
        self
    }

    /// Bands that actually do something (disabled bands are skipped by the writers).
    pub fn active_bands(&self) -> impl Iterator<Item = &PeqBand> {
        self.bands.iter().filter(|b| b.enabled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum EqBackendKind {
    Pipewire,
    EqualizerApo,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostEqStatus {
    pub backend: EqBackendKind,
    pub available: bool,
    pub active: bool,
    pub message: String,
    pub config_path: Option<String>,
}

impl HostEqStatus {
    pub fn unsupported(msg: &str) -> Self {
        HostEqStatus { backend: EqBackendKind::None, available: false, active: false, message: msg.into(), config_path: None }
    }
}

// ---------------------------------------------------------------------------------------------
// Biquad maths (RBJ Audio EQ Cookbook)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a0: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coeffs {
    pub fn normalized(self) -> Coeffs {
        Coeffs { b0: self.b0 / self.a0, b1: self.b1 / self.a0, b2: self.b2 / self.a0, a0: 1.0, a1: self.a1 / self.a0, a2: self.a2 / self.a0 }
    }
}

pub fn coeffs(kind: FilterType, f0: f64, gain_db: f64, q: f64, fs: f64) -> Coeffs {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * f0.min(fs * 0.49) / fs;
    let (sw, cw) = w0.sin_cos();
    let alpha = sw / (2.0 * q.max(0.01));
    let sa = 2.0 * a.sqrt() * alpha;
    match kind {
        FilterType::Peaking => Coeffs { b0: 1.0 + alpha * a, b1: -2.0 * cw, b2: 1.0 - alpha * a, a0: 1.0 + alpha / a, a1: -2.0 * cw, a2: 1.0 - alpha / a },
        FilterType::LowShelf => Coeffs {
            b0: a * ((a + 1.0) - (a - 1.0) * cw + sa),
            b1: 2.0 * a * ((a - 1.0) - (a + 1.0) * cw),
            b2: a * ((a + 1.0) - (a - 1.0) * cw - sa),
            a0: (a + 1.0) + (a - 1.0) * cw + sa,
            a1: -2.0 * ((a - 1.0) + (a + 1.0) * cw),
            a2: (a + 1.0) + (a - 1.0) * cw - sa,
        },
        FilterType::HighShelf => Coeffs {
            b0: a * ((a + 1.0) + (a - 1.0) * cw + sa),
            b1: -2.0 * a * ((a - 1.0) + (a + 1.0) * cw),
            b2: a * ((a + 1.0) + (a - 1.0) * cw - sa),
            a0: (a + 1.0) - (a - 1.0) * cw + sa,
            a1: 2.0 * ((a - 1.0) - (a + 1.0) * cw),
            a2: (a + 1.0) - (a - 1.0) * cw - sa,
        },
        FilterType::BandPass => Coeffs { b0: alpha, b1: 0.0, b2: -alpha, a0: 1.0 + alpha, a1: -2.0 * cw, a2: 1.0 - alpha },
        FilterType::Notch => Coeffs { b0: 1.0, b1: -2.0 * cw, b2: 1.0, a0: 1.0 + alpha, a1: -2.0 * cw, a2: 1.0 - alpha },
    }
}

/// |H(e^jw)| in dB at frequency `f`.
pub fn magnitude_db(c: &Coeffs, f: f64, fs: f64) -> f64 {
    let w = 2.0 * PI * f / fs;
    let (s1, c1) = w.sin_cos();
    let (s2, c2) = (2.0 * w).sin_cos();
    let nr = c.b0 + c.b1 * c1 + c.b2 * c2;
    let ni = -(c.b1 * s1 + c.b2 * s2);
    let dr = c.a0 + c.a1 * c1 + c.a2 * c2;
    let di = -(c.a1 * s1 + c.a2 * s2);
    let mag2 = (nr * nr + ni * ni) / (dr * dr + di * di);
    10.0 * mag2.max(1e-12).log10()
}

pub fn band_db(b: &PeqBand, f: f64) -> f64 {
    if !b.enabled {
        return 0.0;
    }
    magnitude_db(&coeffs(b.kind, b.freq, b.gain, b.q, SAMPLE_RATE), f, SAMPLE_RATE)
}

pub fn log_freqs(n: usize) -> Vec<f64> {
    let (l0, l1) = (20f64.log10(), 20_000f64.log10());
    (0..n).map(|i| 10f64.powf(l0 + (l1 - l0) * i as f64 / (n.max(2) - 1) as f64)).collect()
}

/// Combined response (preamp + all enabled bands) in dB for each frequency.
pub fn response(profile: &HostEqProfile, freqs: &[f64]) -> Vec<f64> {
    freqs.iter().map(|&f| profile.preamp + profile.bands.iter().map(|b| band_db(b, f)).sum::<f64>()).collect()
}

/// Preamp = -(max positive gain of the summed curve), rounded to 0.1 dB, never positive.
pub fn auto_preamp(bands: &[PeqBand]) -> f64 {
    let peak = log_freqs(512)
        .iter()
        .map(|&f| bands.iter().map(|b| band_db(b, f)).sum::<f64>())
        .fold(0.0f64, f64::max);
    let v = -(peak * 10.0).round() / 10.0;
    if v == 0.0 {
        0.0
    } else {
        v
    }
}

// ---------------------------------------------------------------------------------------------
// Equalizer APO / AutoEq "ParametricEQ.txt" text format
// ---------------------------------------------------------------------------------------------

/// Parse `Preamp:` and `Filter N: ON PK Fc 100 Hz Gain 3 dB Q 1.41` lines (AutoEq, Squiglink, REW exports).
pub fn parse_apo_text(text: &str) -> HostEqProfile {
    let mut preamp: Option<f64> = None;
    let mut bands = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("preamp:") {
            preamp = rest.split_whitespace().next().and_then(|v| v.parse().ok());
            continue;
        }
        if !lower.starts_with("filter") {
            continue;
        }
        let Some((_, spec)) = line.split_once(':') else { continue };
        let toks: Vec<&str> = spec.split_whitespace().collect();
        if toks.len() < 2 {
            continue;
        }
        let enabled = toks[0].eq_ignore_ascii_case("ON");
        let Some(kind) = FilterType::from_apo(toks[1]) else { continue };
        let mut freq = None;
        let mut gain = 0.0;
        let mut q = match kind {
            FilterType::LowShelf | FilterType::HighShelf => 0.7,
            _ => 1.0,
        };
        let mut i = 2;
        while i + 1 < toks.len() {
            let val: Option<f64> = toks[i + 1].replace(',', ".").parse().ok();
            match toks[i].to_ascii_lowercase().as_str() {
                "fc" => freq = val,
                "gain" => gain = val.unwrap_or(0.0),
                "q" => q = val.unwrap_or(q),
                "bw" => {
                    // bandwidth in octaves -> Q
                    if let Some(bw) = val {
                        let p = 2f64.powf(bw);
                        q = p.sqrt() / (p - 1.0);
                    }
                }
                _ => {
                    i += 1;
                    continue;
                }
            }
            i += 2;
        }
        if let Some(freq) = freq {
            bands.push(PeqBand { enabled, kind, freq, gain, q });
        }
    }
    bands.sort_by(|a, b| a.freq.partial_cmp(&b.freq).unwrap_or(std::cmp::Ordering::Equal));
    let auto = preamp.is_none();
    HostEqProfile { name: "Imported".into(), enabled: true, preamp: preamp.unwrap_or(0.0), auto_preamp: auto, bands }.sanitized()
}

/// Emit Equalizer-APO compatible text (also accepted by AutoEq/Wavelet/Peace importers).
pub fn to_apo_text(p: &HostEqProfile) -> String {
    let mut s = format!("Preamp: {:.1} dB\n", p.preamp);
    for (i, b) in p.bands.iter().enumerate() {
        let state = if b.enabled { "ON" } else { "OFF" };
        if b.kind.has_gain() {
            s.push_str(&format!("Filter {}: {} {} Fc {:.0} Hz Gain {:.1} dB Q {:.2}\n", i + 1, state, b.kind.apo_code(), b.freq, b.gain, b.q));
        } else {
            s.push_str(&format!("Filter {}: {} {} Fc {:.0} Hz Q {:.2}\n", i + 1, state, b.kind.apo_code(), b.freq, b.q));
        }
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Platform engine facade
// ---------------------------------------------------------------------------------------------

pub struct HostEqEngine {
    #[cfg(target_os = "linux")]
    inner: linux_pipewire::PipeWireEq,
    #[cfg(windows)]
    inner: windows_apo::ApoEq,
}

impl HostEqEngine {
    pub fn new(config_dir: std::path::PathBuf) -> Self {
        let _ = &config_dir;
        HostEqEngine {
            #[cfg(target_os = "linux")]
            inner: linux_pipewire::PipeWireEq::new(config_dir),
            #[cfg(windows)]
            inner: windows_apo::ApoEq::new(),
        }
    }

    /// Apply in real time (called on every slider change, throttled by the UI).
    pub fn apply(&mut self, profile: &HostEqProfile, device_hint: Option<&str>) -> HostEqStatus {
        let _ = (profile, device_hint);
        #[cfg(any(target_os = "linux", windows))]
        {
            return self.inner.apply(profile, device_hint);
        }
        #[allow(unreachable_code)]
        HostEqStatus::unsupported("Системный эквалайзер не поддерживается на этой ОС")
    }

    /// One-time setup that may need elevated rights / persistent config (APO ACL, PipeWire drop-in).
    pub fn install(&mut self, profile: &HostEqProfile, device_hint: Option<&str>) -> HostEqStatus {
        let _ = (profile, device_hint);
        #[cfg(any(target_os = "linux", windows))]
        {
            return self.inner.install(profile, device_hint);
        }
        #[allow(unreachable_code)]
        HostEqStatus::unsupported("Системный эквалайзер не поддерживается на этой ОС")
    }

    pub fn status(&self) -> HostEqStatus {
        #[cfg(any(target_os = "linux", windows))]
        {
            return self.inner.status();
        }
        #[allow(unreachable_code)]
        HostEqStatus::unsupported("Системный эквалайзер не поддерживается на этой ОС")
    }

    pub fn shutdown(&mut self) {
        #[cfg(any(target_os = "linux", windows))]
        self.inner.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band(kind: FilterType, freq: f64, gain: f64, q: f64) -> PeqBand {
        PeqBand { enabled: true, kind, freq, gain, q }
    }

    #[test]
    fn peaking_hits_gain_at_center() {
        let b = band(FilterType::Peaking, 1000.0, 6.0, 1.41);
        assert!((band_db(&b, 1000.0) - 6.0).abs() < 0.01);
        assert!(band_db(&b, 20.0).abs() < 0.05);
        assert!(band_db(&b, 19_000.0).abs() < 0.2);
    }

    #[test]
    fn shelves_reach_plateau() {
        let ls = band(FilterType::LowShelf, 105.0, -7.7, 0.7);
        assert!((band_db(&ls, 20.0) + 7.7).abs() < 0.5);
        assert!(band_db(&ls, 5000.0).abs() < 0.1);
        let hs = band(FilterType::HighShelf, 10_000.0, 4.7, 0.7);
        assert!((band_db(&hs, 20_000.0) - 4.7).abs() < 0.7);
        assert!(band_db(&hs, 100.0).abs() < 0.05);
    }

    #[test]
    fn notch_and_bandpass_shapes() {
        let no = band(FilterType::Notch, 1000.0, 0.0, 5.0);
        assert!(band_db(&no, 1000.0) < -40.0);
        let bp = band(FilterType::BandPass, 1000.0, 0.0, 1.0);
        assert!(band_db(&bp, 1000.0).abs() < 0.01);
        assert!(band_db(&bp, 50.0) < -20.0);
    }

    #[test]
    fn auto_preamp_prevents_clipping() {
        let bands = vec![band(FilterType::Peaking, 100.0, 6.0, 1.0), band(FilterType::Peaking, 120.0, 3.0, 1.0)];
        let pre = auto_preamp(&bands);
        let p = HostEqProfile { name: "t".into(), enabled: true, preamp: pre, auto_preamp: false, bands };
        let max = response(&p, &log_freqs(1024)).into_iter().fold(f64::MIN, f64::max);
        assert!(pre < -8.0 && pre > -10.0, "pre={pre}");
        assert!(max <= 0.06, "max={max}");
        assert_eq!(auto_preamp(&[band(FilterType::Peaking, 100.0, -6.0, 1.0)]), 0.0);
    }

    #[test]
    fn apo_roundtrip() {
        let txt = "Preamp: -6.1 dB\nFilter 1: ON LSC Fc 105 Hz Gain -7.7 dB Q 0.70\nFilter 2: ON PK Fc 4401 Hz Gain 20.0 dB Q 0.64\nFilter 3: OFF PK Fc 253 Hz Gain -4.5 dB Q 0.28\nFilter 4: ON NO Fc 6000 Hz Q 4\n";
        let p = parse_apo_text(txt);
        assert_eq!(p.bands.len(), 4);
        assert_eq!(p.preamp, -6.1);
        assert!(!p.auto_preamp);
        assert_eq!(p.bands[0].kind, FilterType::LowShelf);
        assert!(!p.bands[1].enabled); // 253 Hz sorted second
        assert_eq!(p.bands[3].kind, FilterType::Notch);
        let again = parse_apo_text(&to_apo_text(&p));
        assert_eq!(again.bands, p.bands);
    }

    #[test]
    fn bandwidth_converted_to_q() {
        let p = parse_apo_text("Filter: ON PK Fc 1000 Hz Gain 3 dB BW 1");
        assert!((p.bands[0].q - 1.414).abs() < 0.01);
    }
}

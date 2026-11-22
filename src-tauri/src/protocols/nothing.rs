//! Nothing / CMF by Nothing control protocol (`0x55` framed, RFCOMM service
//! `aeac4a03-dff5-498f-843a-34487cf133eb`). See `research/nothing_protocol.md`.
//!
//! Frame: `[55][CTRL u16 LE][CMD u16 LE][LEN u16 LE][FSN][payload…][CRC16 LE if CTRL&0x20]`
//! Host frames use CTRL `0x0160`. Replies to GET `0xCxxx` come back as `0x4xxx`, ACKs to SET
//! `0xFxxx` as `0x7xxx`, unsolicited pushes as `0xExxx`/`0x6xxx` (CTRL `0x0100`, no CRC).

use super::*;
use std::time::Instant;

pub const SOF: u8 = 0x55;
pub const CTRL_HOST: u16 = 0x0160;
pub const CTRL_CRC: u16 = 0x0020;

// GET
pub const GET_PROTOCOL: u16 = 0xC001;
pub const GET_SERIAL: u16 = 0xC006;
pub const GET_BATTERY: u16 = 0xC007;
pub const GET_WEAR: u16 = 0xC00A;
pub const GET_IN_EAR: u16 = 0xC00E;
pub const GET_GESTURES: u16 = 0xC018;
pub const GET_MODEL: u16 = 0xC01C;
pub const GET_ANC: u16 = 0xC01E;
pub const GET_EQ: u16 = 0xC01F;
pub const GET_PERSONAL_ANC: u16 = 0xC020;
pub const GET_DUAL: u16 = 0xC027;
pub const GET_CODEC: u16 = 0xC029;
pub const GET_LATENCY: u16 = 0xC041;
pub const GET_FIRMWARE: u16 = 0xC042;
pub const GET_CUSTOM_EQ: u16 = 0xC044;
pub const GET_ADV_EQ: u16 = 0xC04C;
pub const GET_BASS: u16 = 0xC04E;
pub const GET_SPATIAL: u16 = 0xC04F;
pub const GET_LISTENING: u16 = 0xC050;
// SET
pub const SET_ACTIVATE: u16 = 0xF001;
pub const SET_RING: u16 = 0xF002;
pub const SET_GESTURE: u16 = 0xF003;
pub const SET_IN_EAR: u16 = 0xF004;
pub const SET_TIME: u16 = 0xF00A;
pub const SET_ANC: u16 = 0xF00F;
pub const SET_EQ: u16 = 0xF010;
pub const SET_PERSONAL_ANC: u16 = 0xF011;
pub const SET_FIT_TEST: u16 = 0xF014;
pub const SET_DUAL: u16 = 0xF01A;
pub const SET_CODEC: u16 = 0xF01C;
pub const SET_LISTENING: u16 = 0xF01D;
pub const SET_LATENCY: u16 = 0xF040;
pub const SET_CUSTOM_EQ: u16 = 0xF041;
pub const SET_ADV_EQ: u16 = 0xF04F;
pub const SET_BASS: u16 = 0xF051;
pub const SET_SPATIAL: u16 = 0xF052;
// PUSH
pub const EVT_BATTERY: u16 = 0xE001;
pub const EVT_WEAR: u16 = 0xE002;
pub const EVT_ANC: u16 = 0xE003;
pub const EVT_FIT: u16 = 0xE00D;
pub const EVT_ANC_ALT: u16 = 0x6003;

/// CRC-16/MODBUS (init 0xFFFF, reflected poly 0xA001, no final xor).
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

/// Build a frame. CRC is appended iff `ctrl & 0x20`.
pub fn build_frame(ctrl: u16, cmd: u16, fsn: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(10 + payload.len());
    f.push(SOF);
    f.extend_from_slice(&ctrl.to_le_bytes());
    f.extend_from_slice(&cmd.to_le_bytes());
    f.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    f.push(fsn);
    f.extend_from_slice(payload);
    if ctrl & CTRL_CRC != 0 {
        let c = crc16(&f);
        f.extend_from_slice(&c.to_le_bytes());
    }
    f
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub ctrl: u16,
    pub cmd: u16,
    pub fsn: u8,
    pub payload: Vec<u8>,
    pub raw: Vec<u8>,
}

impl Frame {
    pub fn status_ok(&self) -> bool {
        self.ctrl & 0x001F == 0
    }
}

/// Streaming splitter: tolerant to garbage, partial frames and several frames per chunk.
#[derive(Default)]
pub struct Splitter {
    buf: Vec<u8>,
}

impl Splitter {
    pub fn push(&mut self, data: &[u8]) -> Vec<Frame> {
        self.buf.extend_from_slice(data);
        if self.buf.len() > 4096 {
            let cut = self.buf.len() - 4096;
            self.buf.drain(..cut);
        }
        let mut out = Vec::new();
        loop {
            match self.buf.iter().position(|&b| b == SOF) {
                None => {
                    self.buf.clear();
                    break;
                }
                Some(p) if p > 0 => {
                    self.buf.drain(..p);
                }
                _ => {}
            }
            if self.buf.len() < 8 {
                break;
            }
            let ctrl = u16::from_le_bytes([self.buf[1], self.buf[2]]);
            let len = u16::from_le_bytes([self.buf[5], self.buf[6]]) as usize;
            let has_crc = ctrl & CTRL_CRC != 0;
            let total = 8 + len + if has_crc { 2 } else { 0 };
            if len > 2048 {
                self.buf.drain(..1);
                continue;
            }
            if self.buf.len() < total {
                break;
            }
            let raw: Vec<u8> = self.buf[..total].to_vec();
            if has_crc {
                let want = crc16(&raw[..8 + len]);
                let got = u16::from_le_bytes([raw[8 + len], raw[9 + len]]);
                if want != got {
                    self.buf.drain(..1);
                    continue;
                }
            }
            self.buf.drain(..total);
            out.push(Frame {
                ctrl,
                cmd: u16::from_le_bytes([raw[3], raw[4]]),
                fsn: raw[7],
                payload: raw[8..8 + len].to_vec(),
                raw,
            });
        }
        out
    }
}

pub fn cmd_name(cmd: u16) -> &'static str {
    match cmd | 0x8000 {
        0xC001 => "Версия протокола",
        0xC006 => "Серийный номер",
        0xC007 => "Батарея",
        0xC00A => "Статус ношения",
        0xC00E => "Датчик ношения",
        0xC018 => "Жесты",
        0xC01C => "Модель",
        0xC01E => "Шумоподавление",
        0xC01F => "EQ пресет",
        0xC020 => "Персональное ANC",
        0xC027 => "Двойное подключение",
        0xC029 => "Hi-Res кодек",
        0xC041 => "Низкая задержка",
        0xC042 => "Прошивка",
        0xC044 => "Свой EQ",
        0xC04C => "Advanced EQ",
        0xC04E => "Ultra Bass",
        0xC04F => "Пространственный звук",
        0xC050 => "Режим прослушивания (CMF)",
        0xF001 => "Активация сессии",
        0xF002 => "Найти наушники",
        0xF003 => "Жест",
        0xF004 => "Датчик ношения",
        0xF00A => "Время UTC",
        0xF00F => "Шумоподавление",
        0xF010 => "EQ пресет",
        0xF011 => "Персональное ANC",
        0xF014 => "Тест прилегания",
        0xF01A => "Двойное подключение",
        0xF01C => "Hi-Res кодек",
        0xF01D => "Режим прослушивания (CMF)",
        0xF040 => "Низкая задержка",
        0xF041 => "Свой EQ",
        0xF04F => "Advanced EQ",
        0xF051 => "Ultra Bass",
        0xF052 => "Пространственный звук",
        0xE001 => "Батарея (push)",
        0xE002 => "Ношение/кейс (push)",
        0xE003 => "Шумоподавление (push)",
        0xE00D => "Тест прилегания (результат)",
        0xE00E => "Устройство мультипоинта (push)",
        _ => "?",
    }
}

pub fn describe(cmd: u16, host: bool) -> String {
    let kind = match cmd >> 12 {
        0xC => "GET",
        0xF => "SET",
        0x4 => "REPLY",
        0x7 => "ACK",
        0xE | 0x6 => "PUSH",
        _ => "?",
    };
    let _ = host;
    format!("{kind} 0x{cmd:04X} {}", cmd_name(cmd))
}

// ---------------------------------------------------------------------------------------------
// Models & capabilities
// ---------------------------------------------------------------------------------------------

const NOTHING_GESTURE_ACTIONS: &[(u8, &str)] = &[
    (1, "Нет действия"),
    (2, "Пауза / воспроизведение"),
    (8, "Предыдущий трек"),
    (9, "Следующий трек"),
    (11, "Голосовой ассистент"),
    (18, "Громкость +"),
    (19, "Громкость −"),
    (10, "Шумодав ↔ Прозрачность ↔ Выкл"),
    (20, "Шумодав ↔ Выкл"),
    (21, "Прозрачность ↔ Выкл"),
    (22, "Шумодав ↔ Прозрачность"),
];

fn bud_gestures(single_word: &str) -> Vec<GestureSlot> {
    let mut v = Vec::new();
    for side in [Side::Left, Side::Right] {
        v.push(slot(side, "double", &format!("Двойное {single_word}")));
        v.push(slot(side, "triple", &format!("Тройное {single_word}")));
        v.push(slot(side, "hold", "Зажать"));
        v.push(slot(side, "doubleHold", "Двойное + зажать"));
    }
    v
}

pub fn gesture_type_code(g: &str) -> Option<u8> {
    Some(match g {
        "single" => 1,
        "double" => 2,
        "triple" => 3,
        "hold" => 7,
        "doubleHold" => 9,
        _ => return None,
    })
}

pub fn gesture_type_name(c: u8) -> Option<&'static str> {
    Some(match c {
        1 => "single",
        2 => "double",
        3 => "triple",
        7 => "hold",
        9 => "doubleHold",
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqScheme {
    /// Nothing: `0xF010`/`0xC01F`, presets 0..3, 5 = custom
    NothingPreset,
    /// CMF: `0xF01D`/`0xC050` "listening mode", 0 Dirac … 6 custom
    CmfListening,
}

#[derive(Debug, Clone)]
pub struct NothingModelSpec {
    pub code: &'static str,
    pub name: &'static str,
    pub vendor: Vendor,
    pub eq: EqScheme,
    pub custom_eq_treble_hz: Option<f32>,
    pub anc_levels: &'static [AncLevel],
    pub ultra_bass: bool,
    pub bass_three_stage: bool,
    pub spatial: bool,
    pub fit_test: bool,
    pub hi_res: &'static [&'static str],
    pub gestures: bool,
    pub advanced_eq: bool,
}

const ALL_NC: &[AncLevel] = &[AncLevel::High, AncLevel::Mid, AncLevel::Low, AncLevel::Adaptive];

pub const SPECS: &[NothingModelSpec] = &[
    NothingModelSpec {
        code: "B171",
        name: "Nothing Ear (2024)",
        vendor: Vendor::Nothing,
        eq: EqScheme::NothingPreset,
        custom_eq_treble_hz: Some(3400.0),
        anc_levels: ALL_NC,
        ultra_bass: true,
        bass_three_stage: false,
        spatial: false,
        fit_test: true,
        hi_res: &["AAC", "LHDC", "LDAC"],
        gestures: true,
        advanced_eq: true,
    },
    NothingModelSpec {
        code: "B172",
        name: "CMF Buds Pro 2",
        vendor: Vendor::Cmf,
        eq: EqScheme::CmfListening,
        custom_eq_treble_hz: Some(6900.0),
        anc_levels: ALL_NC,
        ultra_bass: true,
        bass_three_stage: false,
        spatial: true,
        fit_test: true,
        hi_res: &["AAC", "LDAC"],
        gestures: true,
        advanced_eq: false,
    },
    NothingModelSpec {
        code: "B187",
        name: "CMF Buds Pro 2",
        vendor: Vendor::Cmf,
        eq: EqScheme::CmfListening,
        custom_eq_treble_hz: Some(6900.0),
        anc_levels: ALL_NC,
        ultra_bass: true,
        bass_three_stage: false,
        spatial: true,
        fit_test: true,
        hi_res: &["AAC", "LDAC"],
        gestures: true,
        advanced_eq: false,
    },
    NothingModelSpec {
        code: "B195",
        name: "CMF Buds Pro 2",
        vendor: Vendor::Cmf,
        eq: EqScheme::CmfListening,
        custom_eq_treble_hz: Some(6900.0),
        anc_levels: ALL_NC,
        ultra_bass: true,
        bass_three_stage: true,
        spatial: true,
        fit_test: true,
        hi_res: &["AAC", "LDAC"],
        gestures: true,
        advanced_eq: false,
    },
    NothingModelSpec {
        code: "B155",
        name: "Nothing Ear (2)",
        vendor: Vendor::Nothing,
        eq: EqScheme::NothingPreset,
        custom_eq_treble_hz: Some(3400.0),
        anc_levels: ALL_NC,
        ultra_bass: false,
        bass_three_stage: false,
        spatial: false,
        fit_test: true,
        hi_res: &["AAC", "LHDC"],
        gestures: true,
        advanced_eq: false,
    },
    NothingModelSpec {
        code: "B162",
        name: "Nothing Ear (a)",
        vendor: Vendor::Nothing,
        eq: EqScheme::NothingPreset,
        custom_eq_treble_hz: Some(3400.0),
        anc_levels: ALL_NC,
        ultra_bass: true,
        bass_three_stage: false,
        spatial: false,
        fit_test: true,
        hi_res: &["AAC", "LDAC"],
        gestures: true,
        advanced_eq: false,
    },
    NothingModelSpec {
        code: "B181",
        name: "Nothing Ear (1)",
        vendor: Vendor::Nothing,
        eq: EqScheme::NothingPreset,
        custom_eq_treble_hz: None,
        anc_levels: &[AncLevel::High, AncLevel::Low],
        ultra_bass: false,
        bass_three_stage: false,
        spatial: false,
        fit_test: false,
        hi_res: &[],
        gestures: true,
        advanced_eq: false,
    },
];

fn spec_for(code: &str) -> Option<&'static NothingModelSpec> {
    let code = if code.eq_ignore_ascii_case("B183") { "B162" } else { code };
    SPECS.iter().find(|s| s.code.eq_ignore_ascii_case(code))
}

pub fn caps_for(spec: &NothingModelSpec) -> Capabilities {
    let eq_presets = match spec.eq {
        EqScheme::NothingPreset => vec![opt(0, "Balanced"), opt(3, "More Bass"), opt(2, "More Treble"), opt(1, "Voice"), opt(5, "Custom")],
        EqScheme::CmfListening => vec![
            opt(0, "Dirac Opteo"),
            opt(1, "Rock"),
            opt(2, "Electronic"),
            opt(3, "Pop"),
            opt(4, "Enhance Vocals"),
            opt(5, "Classical"),
            opt(6, "Custom"),
        ],
    };
    let custom_id = match spec.eq {
        EqScheme::NothingPreset => 5,
        EqScheme::CmfListening => 6,
    };
    Capabilities {
        anc_levels: spec.anc_levels.to_vec(),
        transparency: true,
        eq_presets,
        custom_eq_bands: if spec.custom_eq_treble_hz.is_some() {
            vec![140, 980, spec.custom_eq_treble_hz.unwrap_or(3400.0) as u32]
        } else {
            vec![]
        },
        custom_eq_range: (-6.0, 6.0),
        custom_eq_preset_id: spec.custom_eq_treble_hz.map(|_| custom_id),
        bass: spec.ultra_bass.then_some(BassCap { kind: BassKind::UltraBass, min: 1, max: if spec.bass_three_stage { 3 } else { 5 } }),
        spatial_modes: if spec.spatial { vec!["Выкл".into(), "Фиксированный".into()] } else { vec![] },
        low_latency: true,
        in_ear_detection: true,
        multipoint: spec.code != "B181",
        fit_test: spec.fit_test,
        find_buds: true,
        gestures: if spec.gestures {
            bud_gestures(if spec.vendor == Vendor::Cmf { "касание" } else { "сжатие" })
        } else {
            vec![]
        },
        gesture_actions: NOTHING_GESTURE_ACTIONS
            .iter()
            .filter(|(c, _)| spec.vendor == Vendor::Cmf || *c != 2)
            .map(|(c, n)| opt(*c, n))
            .collect(),
        hi_res_options: spec.hi_res.iter().map(|s| s.to_string()).collect(),
        wear_required_for_anc: false,
        personalized_anc: false,
    }
}

pub fn models() -> Vec<ModelInfo> {
    SPECS
        .iter()
        .map(|s| ModelInfo { vendor: s.vendor, code: s.code.into(), name: s.name.into(), caps: caps_for(s) })
        .collect()
}

pub fn identify_by_name(name: &str) -> Option<ModelInfo> {
    let n = name.trim().to_ascii_lowercase();
    let code = if n.ends_with("cmf buds pro 2") {
        "B172"
    } else if n.ends_with("nothing ear (2)") {
        "B155"
    } else if n.ends_with("nothing ear (a)") {
        "B162"
    } else if n.ends_with("nothing ear (1)") || n == "ear (1)" {
        "B181"
    } else if n.ends_with("nothing ear") {
        "B171"
    } else if n.starts_with("cmf ") || n.starts_with("nothing ") {
        // unknown member of the family: closest generic profile, refined by 0xC01C
        if n.starts_with("cmf") {
            "B172"
        } else {
            "B171"
        }
    } else {
        return None;
    };
    model_by_code_local(code)
}

fn model_by_code_local(code: &str) -> Option<ModelInfo> {
    spec_for(code).map(|s| ModelInfo { vendor: s.vendor, code: s.code.into(), name: s.name.into(), caps: caps_for(s) })
}

pub fn anc_mode_byte(mode: AncMode, level: Option<AncLevel>) -> u8 {
    match mode {
        AncMode::Off => 0x05,
        AncMode::Transparency => 0x07,
        AncMode::Anc => match level.unwrap_or(AncLevel::High) {
            AncLevel::High => 0x01,
            AncLevel::Mid => 0x02,
            AncLevel::Low => 0x03,
            AncLevel::Adaptive | AncLevel::Smart => 0x04,
        },
    }
}

pub fn anc_from_byte(b: u8) -> Option<(AncMode, Option<AncLevel>)> {
    Some(match b {
        0x01 => (AncMode::Anc, Some(AncLevel::High)),
        0x02 => (AncMode::Anc, Some(AncLevel::Mid)),
        0x03 => (AncMode::Anc, Some(AncLevel::Low)),
        0x04 => (AncMode::Anc, Some(AncLevel::Adaptive)),
        0x05 => (AncMode::Off, None),
        0x07 => (AncMode::Transparency, None),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Custom EQ (53-byte float32 parametric packet)
// ---------------------------------------------------------------------------------------------

/// Decode a gain float as written by the vendor app (handles the sign-magnitude integer quirk).
pub fn decode_gain(b: [u8; 4]) -> f32 {
    if b[1] == 0 && b[2] == 0 && b[3] == 0 {
        let mag = (b[0] & 0x7F) as f32;
        return if b[0] & 0x80 != 0 { -mag } else { mag };
    }
    let v = f32::from_le_bytes(b);
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Build the 53-byte custom EQ payload. `gains` = [bass, mid, treble] in dB.
pub fn custom_eq_payload(gains: [f32; 3], treble_hz: f32) -> Vec<u8> {
    let [bass, mid, treble] = gains;
    let total = -(bass.max(mid).max(treble).max(0.0));
    let total = if total == 0.0 { -0.0f32 } else { total };
    let mut p = Vec::with_capacity(53);
    p.push(3);
    p.extend_from_slice(&total.to_le_bytes());
    let band = |p: &mut Vec<u8>, kind: u8, gain: f32, freq: f32, q: f32| {
        p.push(kind);
        p.extend_from_slice(&gain.to_le_bytes());
        p.extend_from_slice(&freq.to_le_bytes());
        p.extend_from_slice(&q.to_le_bytes());
    };
    band(&mut p, 1, mid, 980.0, 0.7);
    band(&mut p, 2, treble, treble_hz, 1.0);
    band(&mut p, 0, bass, 140.0, 0.8);
    p.extend_from_slice(&[0u8; 9]);
    p
}

/// Patch gains into a template read from the device (keeps vendor freq/Q).
pub fn patch_custom_eq(template: &[u8], gains: [f32; 3]) -> Option<Vec<u8>> {
    if template.len() < 44 || template[0] != 3 {
        return None;
    }
    let mut p = template.to_vec();
    let [bass, mid, treble] = gains;
    let total = -(bass.max(mid).max(treble).max(0.0));
    let total = if total == 0.0 { -0.0f32 } else { total };
    p[1..5].copy_from_slice(&total.to_le_bytes());
    p[6..10].copy_from_slice(&mid.to_le_bytes());
    p[19..23].copy_from_slice(&treble.to_le_bytes());
    p[32..36].copy_from_slice(&bass.to_le_bytes());
    Some(p)
}

/// Returns [bass, mid, treble].
pub fn parse_custom_eq(payload: &[u8]) -> Option<[f32; 3]> {
    if payload.len() < 36 {
        return None;
    }
    let g = |o: usize| decode_gain([payload[o], payload[o + 1], payload[o + 2], payload[o + 3]]);
    Some([g(32), g(6), g(19)])
}

// ---------------------------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------------------------

pub struct NothingDriver {
    fsn: u8,
    split: Splitter,
    pending: PendingTracker,
    spec: Option<&'static NothingModelSpec>,
    custom_eq_template: Option<Vec<u8>>,
    last_gesture_key: Option<String>,
    case_last_seen: Option<u64>,
    fit_deadline: Option<Instant>,
    ring_off_at: [Option<Instant>; 2],
    model_confirmed: bool,
}

impl Default for NothingDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl NothingDriver {
    pub fn new() -> Self {
        NothingDriver {
            fsn: 0,
            split: Splitter::default(),
            pending: PendingTracker::default(),
            spec: None,
            custom_eq_template: None,
            last_gesture_key: None,
            case_last_seen: None,
            fit_deadline: None,
            ring_off_at: [None, None],
            model_confirmed: false,
        }
    }

    fn next_fsn(&mut self) -> u8 {
        self.fsn = if self.fsn >= 250 { 1 } else { self.fsn + 1 };
        self.fsn
    }

    fn frame(&mut self, cmd: u16, payload: &[u8]) -> Outgoing {
        let fsn = self.next_fsn();
        let bytes = build_frame(CTRL_HOST, cmd, fsn, payload);
        Outgoing::new(bytes, describe(cmd, true)).delay(60)
    }

    fn spec(&self, state: &DeviceState) -> Option<&'static NothingModelSpec> {
        self.spec.or_else(|| spec_for(&state.model_code))
    }

    fn eq_query(&mut self, state: &DeviceState) -> Outgoing {
        match self.spec(state).map(|s| s.eq) {
            Some(EqScheme::CmfListening) => self.frame(GET_LISTENING, &[]),
            _ => self.frame(GET_EQ, &[]),
        }
    }

    fn feature_queries(&mut self, state: &DeviceState) -> Vec<Outgoing> {
        let spec = self.spec(state);
        let mut q = vec![self.frame(GET_BATTERY, &[]), self.frame(GET_ANC, &[]), self.eq_query(state)];
        if spec.map(|s| s.custom_eq_treble_hz.is_some()).unwrap_or(true) {
            q.push(self.frame(GET_CUSTOM_EQ, &[]));
        }
        if spec.map(|s| s.ultra_bass).unwrap_or(true) {
            q.push(self.frame(GET_BASS, &[]));
        }
        q.push(self.frame(GET_LATENCY, &[]));
        q.push(self.frame(GET_IN_EAR, &[]));
        q.push(self.frame(GET_GESTURES, &[]));
        if spec.map(|s| !s.hi_res.is_empty()).unwrap_or(false) {
            q.push(self.frame(GET_CODEC, &[]));
        }
        q.push(self.frame(GET_DUAL, &[]));
        if spec.map(|s| s.spatial).unwrap_or(false) {
            q.push(self.frame(GET_SPATIAL, &[]));
        }
        q.push(self.frame(GET_WEAR, &[]));
        q
    }

    fn set_key(cmd: u16) -> Option<&'static str> {
        Some(match cmd | 0x8000 {
            SET_ANC => "anc",
            SET_EQ | SET_LISTENING => "eq",
            SET_CUSTOM_EQ => "customEq",
            SET_BASS => "bass",
            SET_LATENCY => "lowLatency",
            SET_IN_EAR => "inEarDetection",
            SET_SPATIAL => "spatial",
            SET_CODEC => "hiRes",
            SET_DUAL => "multipoint",
            SET_PERSONAL_ANC => "personalizedAnc",
            _ => return None,
        })
    }

    fn parse_battery(&mut self, p: &[u8], state: &mut DeviceState) {
        let Some(&n) = p.first() else { return };
        let mut seen_case = false;
        let mut left = None;
        let mut right = None;
        for i in 0..n as usize {
            let Some(e) = p.get(1 + i * 2..3 + i * 2) else { break };
            let b = BudBattery { level: Some(e[1] & 0x7F), charging: e[1] & 0x80 != 0 };
            match e[0] {
                0x02 => left = Some(b),
                0x03 => right = Some(b),
                0x04 => {
                    seen_case = true;
                    state.battery.case = b;
                    state.battery.case_cached_at = None;
                    self.case_last_seen = Some(now_ms());
                }
                0x06 => {
                    left = Some(b);
                    right = Some(b);
                }
                _ => {}
            }
        }
        if left.is_some() || right.is_some() || seen_case {
            state.battery.left = left.unwrap_or_default();
            state.battery.right = right.unwrap_or_default();
        }
        if !seen_case && state.battery.case.level.is_some() {
            state.battery.case.charging = false;
            if state.battery.case_cached_at.is_none() {
                state.battery.case_cached_at = Some(self.case_last_seen.unwrap_or_else(now_ms));
            }
        }
    }

    fn parse_wear(&mut self, p: &[u8], state: &mut DeviceState) -> Option<DriverEvent> {
        let before = state.wear.clone();
        let n = *p.first()? as usize;
        if p.len() < 1 + n * 2 || n == 0 {
            return None;
        }
        for i in 0..n {
            let (t, v) = (p[1 + i * 2], p[2 + i * 2]);
            let pos = if v & 0x04 != 0 {
                WearPosition::Ear
            } else if v & 0x01 != 0 {
                WearPosition::Case
            } else {
                WearPosition::Out
            };
            match t {
                0x02 => state.wear.left = pos,
                0x03 => state.wear.right = pos,
                _ => {}
            }
        }
        (before != state.wear).then(|| DriverEvent::Wear { before, after: state.wear.clone() })
    }

    fn handle(&mut self, f: &Frame, state: &mut DeviceState, ev: &mut Vec<DriverEvent>) {
        let p = &f.payload[..];
        match f.cmd {
            0x4001 => {}
            0x401C if p.len() >= 2 => {
                let code = format!("{:04X}", u16::from_le_bytes([p[0], p[1]]));
                if let Some(spec) = spec_for(&code) {
                    let eq_changed = spec_for(&state.model_code).map(|s| s.eq) != Some(spec.eq);
                    let changed = state.model_code != spec.code;
                    self.spec = Some(spec);
                    if changed {
                        state.set_model(&ModelInfo { vendor: spec.vendor, code: spec.code.into(), name: spec.name.into(), caps: caps_for(spec) });
                        let mut q = Vec::new();
                        if eq_changed {
                            q.push(self.eq_query(state));
                        }
                        if spec.spatial {
                            q.push(self.frame(GET_SPATIAL, &[]));
                        }
                        if !q.is_empty() {
                            ev.push(DriverEvent::Send(q));
                        }
                    }
                } else {
                    // unknown model code: keep capabilities guessed from the name, show the real code
                    state.model_code = code;
                }
                self.model_confirmed = true;
            }
            0x4042 => state.firmware = Some(String::from_utf8_lossy(p).trim_matches(char::from(0)).trim().to_string()),
            0x4006 => {
                let text = String::from_utf8_lossy(p.get(1..).unwrap_or_default()).to_string();
                for line in text.lines() {
                    let parts: Vec<&str> = line.trim().split(',').collect();
                    if parts.len() >= 3 && parts[1].trim() == "4" && !parts[2].trim().is_empty() {
                        state.serial = Some(parts[2].trim().to_string());
                        break;
                    }
                }
            }
            0x4007 | EVT_BATTERY => self.parse_battery(p, state),
            0x400A => {
                if let Some(e) = self.parse_wear(p, state) {
                    ev.push(e);
                }
            }
            EVT_WEAR => {
                // case/wear changed → re-read the authoritative status and battery
                let q = vec![self.frame(GET_WEAR, &[]), self.frame(GET_BATTERY, &[])];
                ev.push(DriverEvent::Send(q));
            }
            0x401E | EVT_ANC | EVT_ANC_ALT => {
                // triplets [type, value, 0]; type 1 = current mode
                let mode = p.chunks(3).find(|c| c.len() >= 2 && c[0] == 1).map(|c| c[1]).or_else(|| p.get(1).copied());
                if let Some((m, l)) = mode.and_then(anc_from_byte) {
                    self.report("anc", state, ev, |s| {
                        s.anc.mode = Some(m);
                        if l.is_some() {
                            s.anc.level = l;
                        }
                    });
                }
            }
            0x401F | 0x4050 => {
                if let Some(&id) = p.first() {
                    let id = if f.cmd == 0x4050 && id == 7 { 0 } else { id };
                    self.report("eq", state, ev, |s| s.eq.preset_id = Some(id));
                }
            }
            0x4044 => {
                if let Some(g) = parse_custom_eq(p) {
                    if p.len() >= 44 {
                        self.custom_eq_template = Some(p.to_vec());
                    }
                    self.report("customEq", state, ev, |s| s.eq.custom_gains = g.to_vec());
                }
            }
            0x404E => {
                if p.len() >= 2 {
                    let three = self.spec(state).map(|s| s.bass_three_stage).unwrap_or(false);
                    let level = if three { (p[1] / 5 + 1).min(3) as i8 } else { ((p[1] as i8 + 1) / 2).clamp(1, 5) };
                    let enabled = p[0] == 1;
                    self.report("bass", state, ev, |s| s.bass = BassState { enabled: Some(enabled), level: Some(level) });
                }
            }
            0x4041 => {
                if let Some(&b) = p.first() {
                    self.report("lowLatency", state, ev, |s| s.low_latency = Some(b == 1));
                }
            }
            0x400E => {
                if let Some(&b) = p.get(2).or_else(|| p.first()) {
                    self.report("inEarDetection", state, ev, |s| s.in_ear_detection = Some(b == 1));
                }
            }
            0x4018 => {
                if let Some(&n) = p.first() {
                    let mut list = Vec::new();
                    for i in 0..n as usize {
                        let Some(r) = p.get(1 + i * 4..5 + i * 4) else { break };
                        let side = match r[0] {
                            0x02 => Side::Left,
                            0x03 => Side::Right,
                            _ => continue,
                        };
                        if let Some(g) = gesture_type_name(r[2]) {
                            list.push(GestureBinding { side, gesture: g.into(), action: r[3] });
                        }
                    }
                    match self.last_gesture_key.take() {
                        Some(k) => self.report(&k, state, ev, |s| s.gestures = list),
                        None => state.gestures = list,
                    }
                }
            }
            0x4029 => {
                if let Some(&b) = p.first() {
                    let name = match b {
                        1 => "LHDC",
                        2 => "LDAC",
                        _ => "AAC",
                    };
                    self.report("hiRes", state, ev, |s| s.hi_res = Some(name.into()));
                }
            }
            0x4027 => {
                if let Some(&b) = p.first() {
                    self.report("multipoint", state, ev, |s| s.multipoint = Some(b == 1));
                }
            }
            0x404F => {
                if let Some(&b) = p.first() {
                    let name = if b == 0 { "Выкл" } else { "Фиксированный" };
                    self.report("spatial", state, ev, |s| s.spatial = Some(name.into()));
                }
            }
            0x4020 => {
                if let Some(&b) = p.first() {
                    self.report("personalizedAnc", state, ev, |s| s.personalized_anc = Some(b == 1));
                }
            }
            EVT_FIT => {
                let q = |b: Option<&u8>| match b {
                    Some(0) => Some(FitQuality::Good),
                    Some(1) => Some(FitQuality::Poor),
                    _ => None,
                };
                state.fit = FitState { running: false, left: q(p.first()), right: q(p.get(1)) };
                self.fit_deadline = None;
                self.pending.forget(state, "fit");
            }
            cmd if cmd >> 12 == 0x7 => {
                // ACK for SET 0xF0xx
                let set = cmd | 0x8000;
                let gesture_key = (set == SET_GESTURE).then(|| self.last_gesture_key.clone()).flatten();
                if let Some(key) = Self::set_key(set).map(|k| k.to_string()).or(gesture_key) {
                    let key = key.as_str();
                    if f.status_ok() {
                        if let Some(e) = self.pending.confirm(state, key) {
                            ev.push(e);
                        }
                    } else if let Some(e) = self.pending.reject(state, key, "наушники отклонили команду") {
                        ev.push(e);
                    }
                    if set == SET_GESTURE && f.status_ok() {
                        self.last_gesture_key = None;
                    }
                }
                if set == SET_LATENCY {
                    if let Some(&b) = p.first() {
                        state.low_latency = Some(b == 1);
                    }
                }
            }
            _ => {}
        }
    }

    /// A GET reply / push for a feature arrived. Apply the reported (true) value; if the
    /// feature was pending, the op is resolved: OK when the device now reports exactly what
    /// was requested, otherwise a NACK ("not applied").
    fn report(&mut self, key: &str, state: &mut DeviceState, ev: &mut Vec<DriverEvent>, apply: impl FnOnce(&mut DeviceState)) {
        if let Some(intended_fn) = self.pending.take(state, key) {
            let mut intended = state.clone();
            intended_fn(&mut intended);
            apply(state);
            intended.pending = state.pending.clone();
            let ok = intended == *state;
            ev.push(DriverEvent::Ack {
                key: key.into(),
                ok,
                message: (!ok).then(|| "наушники не применили изменение".to_string()),
            });
        } else {
            apply(state);
        }
    }
}

impl EarbudDriver for NothingDriver {
    fn vendor(&self) -> Vendor {
        Vendor::Nothing
    }

    fn session_start(&mut self) -> Vec<Outgoing> {
        vec![self.frame(GET_PROTOCOL, &[]), self.frame(SET_ACTIVATE, &[])]
    }

    fn initial_queries(&mut self, state: &DeviceState) -> Vec<Outgoing> {
        self.spec = spec_for(&state.model_code);
        let secs = (now_ms() / 1000) as u32;
        let mut q = vec![self.frame(GET_MODEL, &[]), self.frame(GET_FIRMWARE, &[]), self.frame(GET_SERIAL, &[])];
        if state.model_code != "B181" {
            q.push(self.frame(SET_TIME, &secs.to_be_bytes()));
        }
        q.extend(self.feature_queries(state));
        q
    }

    fn poll_queries(&mut self, _state: &DeviceState) -> Vec<Outgoing> {
        vec![self.frame(GET_BATTERY, &[]), self.frame(GET_WEAR, &[])]
    }

    fn encode(&mut self, cmd: &DeviceCommand, state: &mut DeviceState) -> Result<Vec<Outgoing>, ProtoError> {
        let spec = self.spec(state);
        let key = cmd.pending_key();
        let mut out = Vec::new();
        match cmd.clone() {
            DeviceCommand::SetAnc { mode, level } => {
                if mode == AncMode::Anc {
                    if let Some(l) = level {
                        if !state.caps.anc_levels.contains(&l) {
                            return Err(ProtoError::Unsupported("уровень шумоподавления"));
                        }
                    }
                }
                let lvl = if mode == AncMode::Anc { level.or(state.anc.level).or(Some(AncLevel::High)) } else { None };
                let b = anc_mode_byte(mode, lvl);
                out.push(self.frame(SET_ANC, &[0x01, b, 0x00]));
                out.push(self.frame(GET_ANC, &[]));
                self.pending.add(state, &key, Box::new(move |s| {
                    s.anc.mode = Some(mode);
                    if lvl.is_some() {
                        s.anc.level = lvl;
                    }
                }));
            }
            DeviceCommand::SetEqPreset { id } => {
                if !state.caps.eq_presets.iter().any(|p| p.id == id) {
                    return Err(ProtoError::Invalid(format!("пресет {id}")));
                }
                match spec.map(|s| s.eq).unwrap_or(EqScheme::NothingPreset) {
                    EqScheme::NothingPreset => {
                        if spec.map(|s| s.advanced_eq).unwrap_or(false) {
                            out.push(self.frame(SET_ADV_EQ, &[0x00, 0x00]));
                        }
                        out.push(self.frame(SET_EQ, &[id, 0x00]));
                        out.push(self.frame(GET_EQ, &[]));
                    }
                    EqScheme::CmfListening => {
                        out.push(self.frame(SET_LISTENING, &[id, 0x00]));
                        out.push(self.frame(GET_LISTENING, &[]));
                    }
                }
                self.pending.add(state, &key, Box::new(move |s| s.eq.preset_id = Some(id)));
            }
            DeviceCommand::SetCustomEq { gains } => {
                let Some(treble_hz) = spec.and_then(|s| s.custom_eq_treble_hz) else {
                    return Err(ProtoError::Unsupported("свой эквалайзер"));
                };
                if gains.len() != 3 {
                    return Err(ProtoError::Invalid("нужно 3 полосы".into()));
                }
                let g = [gains[0].clamp(-6.0, 6.0).round(), gains[1].clamp(-6.0, 6.0).round(), gains[2].clamp(-6.0, 6.0).round()];
                let custom_id = state.caps.custom_eq_preset_id.unwrap_or(5);
                if state.eq.preset_id != Some(custom_id) {
                    out.extend(self.encode(&DeviceCommand::SetEqPreset { id: custom_id }, state)?);
                }
                let payload = self
                    .custom_eq_template
                    .as_deref()
                    .and_then(|t| patch_custom_eq(t, g))
                    .unwrap_or_else(|| custom_eq_payload(g, treble_hz));
                out.push(self.frame(SET_CUSTOM_EQ, &payload));
                out.push(self.frame(GET_CUSTOM_EQ, &[]));
                let gv = g.to_vec();
                self.pending.add(state, &key, Box::new(move |s| s.eq.custom_gains = gv));
            }
            DeviceCommand::SetBass { enabled, level } => {
                let Some(cap) = state.caps.bass.clone() else { return Err(ProtoError::Unsupported("Ultra Bass")) };
                let level = level.clamp(cap.min, cap.max);
                let three = spec.map(|s| s.bass_three_stage).unwrap_or(false);
                let wire = if three { ((level - 1) * 5) as u8 } else { (level * 2) as u8 };
                out.push(self.frame(SET_BASS, &[u8::from(enabled), wire]));
                out.push(self.frame(GET_BASS, &[]));
                self.pending.add(state, &key, Box::new(move |s| s.bass = BassState { enabled: Some(enabled), level: Some(level) }));
            }
            DeviceCommand::SetLowLatency { enabled } => {
                out.push(self.frame(SET_LATENCY, &[if enabled { 0x01 } else { 0x02 }, 0x00]));
                out.push(self.frame(GET_LATENCY, &[]));
                self.pending.add(state, &key, Box::new(move |s| s.low_latency = Some(enabled)));
            }
            DeviceCommand::SetInEarDetection { enabled } => {
                out.push(self.frame(SET_IN_EAR, &[0x01, 0x01, u8::from(enabled)]));
                out.push(self.frame(GET_IN_EAR, &[]));
                self.pending.add(state, &key, Box::new(move |s| s.in_ear_detection = Some(enabled)));
            }
            DeviceCommand::SetSpatial { mode } => {
                if !state.caps.spatial_modes.contains(&mode) {
                    return Err(ProtoError::Unsupported("пространственный звук"));
                }
                let b = if mode == "Выкл" { 0 } else { 1 };
                out.push(self.frame(SET_SPATIAL, &[b, 0x00]));
                out.push(self.frame(GET_SPATIAL, &[]));
                self.pending.add(state, &key, Box::new(move |s| s.spatial = Some(mode)));
            }
            DeviceCommand::SetMultipoint { enabled } => {
                out.push(self.frame(SET_DUAL, &[u8::from(enabled)]));
                out.push(self.frame(GET_DUAL, &[]));
                self.pending.add(state, &key, Box::new(move |s| s.multipoint = Some(enabled)));
            }
            DeviceCommand::SetPersonalizedAnc { enabled } => {
                if !state.caps.personalized_anc {
                    return Err(ProtoError::Unsupported("персональное ANC"));
                }
                out.push(self.frame(SET_PERSONAL_ANC, &[u8::from(enabled)]));
                out.push(self.frame(GET_PERSONAL_ANC, &[]));
                self.pending.add(state, &key, Box::new(move |s| s.personalized_anc = Some(enabled)));
            }
            DeviceCommand::SetHiRes { value } => {
                let idx = match value.as_str() {
                    "LHDC" => 1,
                    "LDAC" => 2,
                    _ => 0,
                };
                if !state.caps.hi_res_options.contains(&value) {
                    return Err(ProtoError::Unsupported("Hi-Res кодек"));
                }
                out.push(self.frame(SET_CODEC, &[idx]));
                self.pending.add(state, &key, Box::new(move |s| s.hi_res = Some(value)));
            }
            DeviceCommand::SetGesture { side, gesture, action } => {
                let t = gesture_type_code(&gesture).ok_or_else(|| ProtoError::Invalid(gesture.clone()))?;
                let dev = if side == Side::Left { 0x02 } else { 0x03 };
                out.push(self.frame(SET_GESTURE, &[0x01, dev, 0x01, t, action]));
                out.push(self.frame(GET_GESTURES, &[]));
                self.last_gesture_key = Some(key.clone());
                let g2 = gesture.clone();
                self.pending.add(state, &key, Box::new(move |s| {
                    if let Some(b) = s.gestures.iter_mut().find(|b| b.side == side && b.gesture == g2) {
                        b.action = action;
                    } else {
                        s.gestures.push(GestureBinding { side, gesture: g2, action });
                    }
                }));
            }
            DeviceCommand::StartFitTest => {
                if !state.caps.fit_test {
                    return Err(ProtoError::Unsupported("тест прилегания"));
                }
                out.push(self.frame(SET_FIT_TEST, &[0x01]));
                state.fit = FitState { running: true, left: None, right: None };
                self.fit_deadline = Some(Instant::now() + std::time::Duration::from_secs(20));
            }
            DeviceCommand::FindBuds { side, enabled } => {
                let dev = if side == Side::Left { 0x02 } else { 0x03 };
                out.push(self.frame(SET_RING, &[dev, u8::from(enabled)]));
                let idx = if side == Side::Left { 0 } else { 1 };
                // fire-and-forget: the bud rings immediately; auto-stop after 30 s for safety
                match side {
                    Side::Left => state.find_buds.left = enabled,
                    Side::Right => state.find_buds.right = enabled,
                }
                self.ring_off_at[idx] = enabled.then(|| Instant::now() + std::time::Duration::from_secs(30));
            }
            DeviceCommand::Refresh => {
                out.push(self.frame(GET_MODEL, &[]));
                out.push(self.frame(GET_FIRMWARE, &[]));
                out.extend(self.feature_queries(state));
            }
        }
        Ok(out)
    }

    fn feed(&mut self, data: &[u8], state: &mut DeviceState) -> Vec<DriverEvent> {
        let mut ev = Vec::new();
        for f in self.split.push(data) {
            ev.push(DriverEvent::Rx { bytes: f.raw.clone(), desc: describe(f.cmd, false) });
            self.handle(&f, state, &mut ev);
            ev.push(DriverEvent::Changed);
        }
        ev
    }

    fn tick(&mut self, now: Instant, state: &mut DeviceState) -> Vec<DriverEvent> {
        let mut ev = self.pending.expire(now, state);
        if let Some(d) = self.fit_deadline {
            if now >= d {
                self.fit_deadline = None;
                state.fit.running = false;
                ev.push(DriverEvent::Ack { key: "fit".into(), ok: false, message: Some("тест прилегания не завершился — вставьте оба наушника".into()) });
            }
        }
        for (i, side) in [(0usize, Side::Left), (1, Side::Right)] {
            if let Some(t) = self.ring_off_at[i] {
                if now >= t {
                    self.ring_off_at[i] = None;
                    let frames = self.encode(&DeviceCommand::FindBuds { side, enabled: false }, state).unwrap_or_default();
                    ev.push(DriverEvent::Send(frames));
                }
            }
        }
        if !ev.is_empty() {
            ev.push(DriverEvent::Changed);
        }
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden(s: &str) -> Vec<u8> {
        parse_hex(s)
    }

    #[test]
    fn crc_check_value() {
        assert_eq!(crc16(b"123456789"), 0x4B37);
    }

    #[test]
    fn golden_host_frames_from_captures() {
        // Bharadwaj Raju HCI captures (Nothing Ear (2)) + corrected ANC table
        let cases: &[(u16, u8, &[u8], &str)] = &[
            (SET_ANC, 0xcb, &[1, 7, 0], "55 60 01 0f f0 03 00 cb 01 07 00 c5 af"),
            (SET_ANC, 0xcd, &[1, 5, 0], "55 60 01 0f f0 03 00 cd 01 05 00 c4 47"),
            (SET_ANC, 0xcf, &[1, 1, 0], "55 60 01 0f f0 03 00 cf 01 01 00 c7 3f"),
            (SET_ANC, 0xd5, &[1, 2, 0], "55 60 01 0f f0 03 00 d5 01 02 00 c0 d7"),
            (SET_ANC, 0xd7, &[1, 3, 0], "55 60 01 0f f0 03 00 d7 01 03 00 c0 ff"),
            (SET_ANC, 0xdd, &[1, 4, 0], "55 60 01 0f f0 03 00 dd 01 04 00 c1 17"),
            (GET_ANC, 0x0c, &[3], "55 60 01 1e c0 01 00 0c 03 98 19"),
            (SET_LATENCY, 0x27, &[1, 0], "55 60 01 40 f0 02 00 27 01 00 97 f7"),
            (SET_LATENCY, 0x28, &[2, 0], "55 60 01 40 f0 02 00 28 02 00 a7 04"),
            (SET_IN_EAR, 0x26, &[1, 1, 1], "55 60 01 04 f0 03 00 26 01 01 01 73 10"),
            (SET_IN_EAR, 0x25, &[1, 1, 0], "55 60 01 04 f0 03 00 25 01 01 00 b2 94"),
            (SET_FIT_TEST, 0x2a, &[1], "55 60 01 14 f0 01 00 2a 01 43 16"),
            (GET_GESTURES, 0x51, &[], "55 60 01 18 c0 00 00 51 39 21"),
            (GET_FIRMWARE, 0x03, &[], "55 60 01 42 c0 00 00 03 e0 d1"),
            (GET_SERIAL, 0x05, &[], "55 60 01 06 c0 00 00 05 90 dc"),
            (SET_GESTURE, 0x59, &[1, 2, 1, 3, 9], "55 60 01 03 f0 05 00 59 01 02 01 03 09 de 71"),
            (GET_MODEL, 0x01, &[], "55 60 01 1c c0 00 00 01 c8 dd"),
            (SET_BASS, 0x01, &[1, 6], "55 60 01 51 f0 02 00 01 01 06 f7 3f"),
            (SET_LISTENING, 0x01, &[0, 0], "55 60 01 1d f0 02 00 01 00 00 fb 69"),
            (SET_RING, 0x01, &[2, 1], "55 60 01 02 f0 02 00 01 02 01 d5 08"),
        ];
        for (cmd, fsn, payload, hex) in cases {
            assert_eq!(build_frame(CTRL_HOST, *cmd, *fsn, payload), golden(hex), "cmd {cmd:04X}");
        }
    }

    #[test]
    fn custom_eq_payload_matches_reference() {
        // research §10: B171 bass+3 / mid 0 / treble −3
        let p = custom_eq_payload([3.0, 0.0, -3.0], 3400.0);
        assert_eq!(p.len(), 53);
        let f = build_frame(CTRL_HOST, SET_CUSTOM_EQ, 0x01, &p);
        assert_eq!(f, golden("55 60 01 41 f0 35 00 01 03 00 00 40 c0 01 00 00 00 00 00 00 75 44 33 33 33 3f 02 00 00 40 c0 00 80 54 45 00 00 80 3f 00 00 00 40 40 00 00 0c 43 cd cc 4c 3f 00 00 00 00 00 00 00 00 00 2a a2"));
        let flat = build_frame(CTRL_HOST, SET_CUSTOM_EQ, 0x01, &custom_eq_payload([0.0, 0.0, 0.0], 6900.0));
        assert_eq!(&flat[flat.len() - 2..], &[0x67, 0x5c]);
        assert_eq!(parse_custom_eq(&p), Some([3.0, 0.0, -3.0]));
        // sign-magnitude integer quirk
        assert_eq!(decode_gain([0x83, 0, 0, 0]), -3.0);
        assert_eq!(decode_gain([0x02, 0, 0, 0]), 2.0);
    }

    #[test]
    fn splitter_handles_garbage_partial_and_pushes_without_crc() {
        let mut s = Splitter::default();
        // device reply with CRC (firmware) split in two chunks + garbage in front
        let reply = build_frame(0x0160, 0x4042, 0x03, b"1.0.1.101");
        let mut stream = vec![0x00, 0xAB];
        stream.extend_from_slice(&reply[..5]);
        assert!(s.push(&stream).is_empty());
        // fit-test push (no CRC, CTRL 0x0100) concatenated right after the reply
        let mut rest = reply[5..].to_vec();
        rest.extend(golden("55 00 01 0d e0 02 00 00 01 00"));
        let frames = s.push(&rest);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].cmd, 0x4042);
        assert_eq!(frames[0].payload, b"1.0.1.101");
        assert_eq!(frames[1].cmd, EVT_FIT);
        assert_eq!(frames[1].payload, vec![1, 0]);
        // corrupted CRC is dropped
        let mut bad = build_frame(0x0160, 0x4007, 1, &[1, 2, 50]);
        let n = bad.len();
        bad[n - 1] ^= 0xFF;
        assert!(s.push(&bad).is_empty());
    }

    fn b171_state() -> DeviceState {
        let m = identify_by_name("Nothing Ear").unwrap();
        assert_eq!(m.code, "B171");
        let mut st = DeviceState::new("x", "Nothing Ear", "00:00:00:00:00:00", &m);
        st.connection = ConnectionState::Connected;
        st
    }

    #[test]
    fn parses_battery_anc_model_and_caches_case() {
        let mut d = NothingDriver::new();
        let mut st = b171_state();
        let mut stream = build_frame(0x0160, 0x4007, 1, &[3, 2, 0x55, 3, 0xD0, 4, 64]);
        stream.extend(build_frame(0x0160, 0x401E, 2, &[1, 7, 0, 2, 4, 0]));
        stream.extend(build_frame(0x0160, 0x401C, 3, &[0x87, 0xB1]));
        d.feed(&stream, &mut st);
        assert_eq!(st.battery.left, BudBattery { level: Some(85), charging: false });
        assert_eq!(st.battery.right, BudBattery { level: Some(80), charging: true });
        assert_eq!(st.battery.case.level, Some(64));
        assert_eq!(st.anc.mode, Some(AncMode::Transparency));
        assert_eq!(st.model_code, "B187");
        assert_eq!(st.vendor, Vendor::Cmf);
        assert_eq!(st.caps.eq_presets[0].name, "Dirac Opteo");
        // push without case entry → case cached
        d.feed(&build_frame(0x0100, EVT_BATTERY, 0, &[2, 2, 0x50, 3, 0x50]), &mut st);
        assert_eq!(st.battery.case.level, Some(64));
        assert!(st.battery.case_cached_at.is_some());
    }

    #[test]
    fn anc_set_is_pending_until_hardware_confirms() {
        let mut d = NothingDriver::new();
        let mut st = b171_state();
        st.anc.mode = Some(AncMode::Off);
        let frames = d.encode(&DeviceCommand::SetAnc { mode: AncMode::Anc, level: Some(AncLevel::Adaptive) }, &mut st).unwrap();
        assert_eq!(frames[0].bytes[3..5], [0x0F, 0xF0]);
        assert_eq!(frames[0].bytes[8..11], [0x01, 0x04, 0x00]);
        assert_eq!(st.anc.mode, Some(AncMode::Off), "must not change before ACK");
        assert_eq!(st.pending, vec!["anc".to_string()]);
        let ev = d.feed(&build_frame(0x0160, 0x700F, frames[0].bytes[7], &[]), &mut st);
        assert!(ev.iter().any(|e| matches!(e, DriverEvent::Ack { ok: true, .. })));
        assert_eq!(st.anc.mode, Some(AncMode::Anc));
        assert_eq!(st.anc.level, Some(AncLevel::Adaptive));
        assert!(st.pending.is_empty());
    }

    #[test]
    fn readback_confirms_when_no_ack_arrives() {
        let mut d = NothingDriver::new();
        let mut st = b171_state();
        d.encode(&DeviceCommand::SetLowLatency { enabled: true }, &mut st).unwrap();
        assert!(st.pending.contains(&"lowLatency".to_string()));
        d.feed(&build_frame(0x0160, 0x4041, 9, &[1, 0]), &mut st);
        assert_eq!(st.low_latency, Some(true));
        assert!(st.pending.is_empty());
    }

    #[test]
    fn pending_times_out() {
        let mut d = NothingDriver::new();
        let mut st = b171_state();
        d.encode(&DeviceCommand::SetEqPreset { id: 3 }, &mut st).unwrap();
        let ev = d.tick(Instant::now() + ACK_TIMEOUT * 2, &mut st);
        assert!(ev.iter().any(|e| matches!(e, DriverEvent::Ack { ok: false, .. })));
        assert!(st.pending.is_empty());
        assert_eq!(st.eq.preset_id, None);
    }

    #[test]
    fn gestures_and_fit_parse() {
        let mut d = NothingDriver::new();
        let mut st = b171_state();
        let payload = parse_hex("08 02 01 02 09 03 01 02 09 02 01 03 08 03 01 03 08 02 01 07 16 03 01 07 16 02 01 09 01 03 01 09 01");
        d.feed(&build_frame(0x0160, 0x4018, 1, &payload), &mut st);
        assert_eq!(st.gestures.len(), 8);
        assert!(st.gestures.contains(&GestureBinding { side: Side::Right, gesture: "hold".into(), action: 0x16 }));
        st.fit.running = true;
        d.feed(&golden("55 00 01 0d e0 02 00 00 01 00"), &mut st);
        assert_eq!(st.fit, FitState { running: false, left: Some(FitQuality::Poor), right: Some(FitQuality::Good) });
    }

    #[test]
    fn wear_status_parse_emits_event() {
        let mut d = NothingDriver::new();
        let mut st = b171_state();
        let ev = d.feed(&build_frame(0x0160, 0x400A, 1, &[2, 2, 0x84, 3, 0x81]), &mut st);
        assert_eq!(st.wear.left, WearPosition::Ear);
        assert_eq!(st.wear.right, WearPosition::Case);
        assert!(ev.iter().any(|e| matches!(e, DriverEvent::Wear { .. })));
    }

    #[test]
    fn identify_names() {
        assert_eq!(identify_by_name("CMF Buds Pro 2").unwrap().code, "B172");
        assert_eq!(identify_by_name("Nothing Ear (a)").unwrap().code, "B162");
        assert_eq!(identify_by_name("Nothing Ear").unwrap().code, "B171");
        assert!(identify_by_name("OnePlus Buds Pro 4").is_none());
    }
}

//! OnePlus / OPPO / realme HeyMelody control protocol ("OPOv1", `0xAA` framing).
//! Same framing on RFCOMM (service `0000079A-D102-11E1-9B23-00025B00A5A5`) and BLE GATT.
//! See `research/oneplus_protocol.md` for sources and evidence levels.
//!
//! Frame: `[AA][TotalLen LEB128 = 7+PayLen][slice=00][00][cmd u16 LE][seq][PayLen u16 LE][payload]`
//! No checksum. Reply cmd = request | 0x8000, seq echoed; pushes are `0x0204` with seq `FF`.
//! There is no authentication: the session starts with the `0x0100` capability handshake, and
//! the host must answer the buds' `0x05xx` requests (notably `0x0500` time).

use super::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const SOF: u8 = 0xAA;

pub const CMD_HANDSHAKE: u16 = 0x0100;
pub const CMD_PRODUCT_ID: u16 = 0x0103;
pub const CMD_FIRMWARE: u16 = 0x0105;
pub const CMD_BATTERY: u16 = 0x0106;
pub const CMD_GESTURES: u16 = 0x0108;
pub const CMD_WEAR: u16 = 0x0109;
pub const CMD_ANC_GET: u16 = 0x010C;
pub const CMD_SWITCHES_GET: u16 = 0x010D;
pub const CMD_EQ_GET: u16 = 0x010F;
pub const CMD_CODEC_GET: u16 = 0x0114;
pub const CMD_CUSTOM_EQ_LIST: u16 = 0x0122;
pub const CMD_BASSWAVE_GET: u16 = 0x0124;
pub const CMD_SPATIAL_GET: u16 = 0x012A;
pub const CMD_EVENTS_LIST: u16 = 0x0200;
pub const CMD_SUBSCRIBE_ONE: u16 = 0x0201;
pub const CMD_PUSH: u16 = 0x0204;
pub const CMD_SUBSCRIBE: u16 = 0x0205;
pub const CMD_FIND: u16 = 0x0400;
pub const CMD_GESTURE_SET: u16 = 0x0401;
pub const CMD_SWITCH_SET: u16 = 0x0403;
pub const CMD_ANC_SET: u16 = 0x0404;
pub const CMD_FIT_TEST: u16 = 0x0405;
pub const CMD_EQ_SET: u16 = 0x0406;
pub const CMD_MULTIPOINT_OP: u16 = 0x0413;
pub const CMD_CUSTOM_EQ_SET: u16 = 0x0418;
pub const CMD_BASSWAVE_SET: u16 = 0x041B;
pub const CMD_SPATIAL_SET: u16 = 0x0422;
pub const CMD_TIME_REQ: u16 = 0x0500;
pub const CMD_SWITCH_PUSH: u16 = 0x0503;
pub const CMD_EQ_PUSH: u16 = 0x0504;
pub const CMD_SPATIAL_PUSH: u16 = 0x0510;

pub const F_WEAR_DETECT: u8 = 0x04;
pub const F_GAME: u8 = 0x06;
pub const F_MULTIPOINT: u8 = 0x11;
pub const F_SPATIAL: u8 = 0x1B;
pub const F_BASSWAVE: u8 = 0x1D;
pub const F_GAME_ALT: u8 = 0x28;

pub const PUSH_SEQ: u8 = 0xFF;

fn leb128(mut v: usize, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

pub fn build(cmd: u16, seq: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = vec![SOF];
    leb128(7 + payload.len(), &mut f);
    f.extend_from_slice(&[0x00, 0x00]);
    f.extend_from_slice(&cmd.to_le_bytes());
    f.push(seq);
    f.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    f.extend_from_slice(payload);
    f
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub cmd: u16,
    pub seq: u8,
    pub payload: Vec<u8>,
    pub raw: Vec<u8>,
}

/// Streaming de-framer (handles concatenated/partial frames, LEB128 lengths, slices).
#[derive(Default)]
pub struct Splitter {
    buf: Vec<u8>,
    slices: Vec<u8>,
}

impl Splitter {
    pub fn push(&mut self, data: &[u8]) -> Vec<Frame> {
        self.buf.extend_from_slice(data);
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
            // LEB128 total length (max 3 bytes)
            let mut total: usize = 0;
            let mut nlen = 0;
            let mut complete = false;
            for i in 0..3 {
                let Some(&b) = self.buf.get(1 + i) else { break };
                total |= ((b & 0x7F) as usize) << (7 * i);
                nlen = i + 1;
                if b & 0x80 == 0 {
                    complete = true;
                    break;
                }
            }
            if !complete {
                if self.buf.len() >= 4 {
                    self.buf.drain(..1);
                    continue;
                }
                break;
            }
            let hdr = 1 + nlen;
            if !(4..=2048).contains(&total) {
                self.buf.drain(..1);
                continue;
            }
            if self.buf.len() < hdr + total {
                break;
            }
            let raw: Vec<u8> = self.buf[..hdr + total].to_vec();
            self.buf.drain(..hdr + total);
            let body = &raw[hdr..];
            let slice = body[0] & 0x07;
            if (1..=3).contains(&slice) {
                // sliced message: data after [slice][fsn]
                if slice == 1 {
                    self.slices.clear();
                }
                self.slices.extend_from_slice(body.get(2..).unwrap_or_default());
                if slice == 3 {
                    let joined = std::mem::take(&mut self.slices);
                    if joined.len() >= 5 {
                        let cmd = u16::from_le_bytes([joined[0], joined[1]]);
                        let seq = joined[2];
                        let plen = u16::from_le_bytes([joined[3], joined[4]]) as usize;
                        let payload = joined[5..].iter().copied().take(plen).collect();
                        out.push(Frame { cmd, seq, payload, raw: raw.clone() });
                    }
                }
                continue;
            }
            if total < 7 {
                continue;
            }
            let cmd = u16::from_le_bytes([body[2], body[3]]);
            let seq = body[4];
            let plen = u16::from_le_bytes([body[5], body[6]]) as usize;
            let avail = &body[7..];
            let payload = avail[..plen.min(avail.len())].to_vec();
            out.push(Frame { cmd, seq, payload, raw });
        }
        out
    }
}

pub fn cmd_name(cmd: u16) -> &'static str {
    match cmd & 0x7FFF {
        0x0100 => "Рукопожатие / возможности",
        0x0103 => "ID продукта",
        0x0105 => "Прошивка",
        0x0106 => "Батарея",
        0x0108 => "Жесты",
        0x0109 => "Ношение",
        0x010C => "Шумоподавление",
        0x010D => "Переключатели функций",
        0x010F => "EQ пресет",
        0x0114 => "Кодек",
        0x0122 => "Свои EQ",
        0x0124 => "BassWave уровень",
        0x012A => "Пространственный звук",
        0x0200 => "Список событий",
        0x0201 | 0x0205 => "Подписка на события",
        0x0204 => "Событие",
        0x0400 => "Найти наушники",
        0x0401 => "Жест",
        0x0403 => "Переключатель функции",
        0x0404 => "Шумоподавление",
        0x0405 => "Тест прилегания",
        0x0406 => "EQ пресет",
        0x0413 => "Мультипоинт",
        0x0418 => "Свой EQ",
        0x041B => "BassWave уровень",
        0x0422 => "Пространственный звук",
        0x0500 => "Запрос времени",
        0x0503 => "Функция изменена",
        0x0504 => "EQ изменён",
        0x0510 => "Простр. звук изменён",
        _ => "?",
    }
}

pub fn describe(f_cmd: u16, seq: u8, payload: &[u8]) -> String {
    let base = f_cmd & 0x7FFF;
    let kind = if f_cmd == CMD_PUSH && seq == PUSH_SEQ {
        let ev = match payload.first() {
            Some(1) => "батарея",
            Some(2) => "ношение",
            Some(3) => "ANC",
            Some(4) => "тест прилегания",
            Some(5) => "игровой режим",
            Some(6) => "устройства",
            Some(0xF1) => "жест",
            Some(0xF2) => "подключения",
            _ => "?",
        };
        return format!("PUSH 0x0204 Событие: {ev}");
    } else if f_cmd & 0x8000 != 0 {
        "REPLY"
    } else {
        match base >> 8 {
            0x01 => "GET",
            0x02 => "EVT",
            0x04 => "SET",
            0x05 => "REQ",
            _ => "?",
        }
    };
    format!("{kind} 0x{f_cmd:04X} {}", cmd_name(base))
}

// ---------------------------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AncTree {
    /// Buds 3/4, Pro 3/4, Nord Buds 3 Pro: SET Off 0x01 ANC 0x02 Transparency 0x04, levels 0x10.. ,
    /// Adaptive bit 11; reports use child bits (0x0008 Off, 0x0100 Transparency…)
    Buds4,
    /// Original OnePlus Buds Pro: Off 01, Transparency 02, light 04, deep 08, smart 10 (set = report)
    BudsPro1,
}

#[derive(Debug)]
pub struct OpoSpec {
    pub pid: &'static str,
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub tree: AncTree,
    pub levels: &'static [AncLevel],
    pub eq: &'static [(u8, &'static str)],
    pub custom_bands: &'static [u32],
    pub status_query: &'static [u8],
    pub spatial_head: bool,
    pub utc_tz: bool,
    pub pinch: bool,
    /// game mode switch id preferred on this model
    pub game_switch: u8,
}

const BANDS6: &[u32] = &[62, 250, 1000, 4000, 8000, 16000];
const BANDS10: &[u32] = &[31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
const LV4: &[AncLevel] = &[AncLevel::High, AncLevel::Mid, AncLevel::Low, AncLevel::Smart];
const LV5: &[AncLevel] = &[AncLevel::High, AncLevel::Mid, AncLevel::Low, AncLevel::Smart, AncLevel::Adaptive];

pub const SPECS: &[OpoSpec] = &[
    OpoSpec {
        pid: "067814",
        name: "OnePlus Buds Pro 4",
        aliases: &["OnePlus Buds Bohr"],
        tree: AncTree::Buds4,
        levels: LV5,
        eq: &[(0, "Balanced"), (1, "Bold"), (2, "Serenade"), (3, "Bass"), (7, "Dynaudio")],
        custom_bands: BANDS10,
        status_query: &[0x05, 0x04, 0x0B, 0x0F, 0x11, 0x13, 0x18, 0x06, 0x1B, 0x1D, 0x1C, 0x1E, 0x27, 0x28, 0x32, 0x37, 0x3B, 0x3A],
        spatial_head: true,
        utc_tz: true,
        pinch: true,
        game_switch: F_GAME,
    },
    OpoSpec {
        pid: "064014",
        name: "OnePlus Buds Pro 3",
        aliases: &[],
        tree: AncTree::Buds4,
        levels: LV4,
        eq: &[(0, "Balanced"), (1, "Bold"), (2, "Serenade"), (3, "Bass"), (7, "Dynaudio")],
        custom_bands: BANDS6,
        status_query: &[0x05, 0x04, 0x0B, 0x11, 0x13, 0x18, 0x06, 0x1D],
        spatial_head: true,
        utc_tz: false,
        pinch: true,
        game_switch: F_GAME,
    },
    OpoSpec {
        pid: "063C14",
        name: "OnePlus Buds 3",
        aliases: &[],
        tree: AncTree::Buds4,
        levels: LV4,
        eq: &[(0, "Balanced"), (1, "Bass"), (2, "Clear Vocals"), (3, "Bold")],
        custom_bands: BANDS6,
        status_query: &[0x05, 0x04, 0x0B, 0x11, 0x18, 0x06, 0x1B, 0x1D],
        spatial_head: false,
        utc_tz: false,
        pinch: false,
        game_switch: F_GAME,
    },
    OpoSpec {
        pid: "064414",
        name: "OnePlus Nord Buds 3 Pro",
        aliases: &[],
        tree: AncTree::Buds4,
        levels: LV4,
        eq: &[(3, "Balanced"), (4, "Clear Vocals"), (5, "Bass")],
        custom_bands: BANDS6,
        status_query: &[0x05, 0x04, 0x0B, 0x0C, 0x11, 0x18, 0x06, 0x1B, 0x1D],
        spatial_head: false,
        utc_tz: false,
        pinch: false,
        game_switch: F_GAME,
    },
    OpoSpec {
        pid: "065414",
        name: "OnePlus Buds 4",
        aliases: &[],
        tree: AncTree::Buds4,
        levels: LV5,
        eq: &[(0, "Balanced"), (1, "Clear Vocals"), (2, "Bass")],
        custom_bands: BANDS6,
        status_query: &[0x05, 0x04, 0x0B, 0x11, 0x18, 0x06, 0x1B, 0x1D, 0x1C],
        spatial_head: false,
        utc_tz: false,
        pinch: false,
        game_switch: F_GAME,
    },
    OpoSpec {
        pid: "060C14",
        name: "OnePlus Buds Pro",
        aliases: &[],
        tree: AncTree::BudsPro1,
        levels: &[AncLevel::High, AncLevel::Low, AncLevel::Smart],
        eq: &[],
        custom_bands: &[],
        status_query: &[0x04, 0x06],
        spatial_head: false,
        utc_tz: false,
        pinch: false,
        game_switch: F_GAME,
    },
];

const GESTURE_FNS: &[(u8, &str)] = &[
    (0x00, "Нет действия"),
    (0x01, "Пауза / воспроизведение"),
    (0x05, "Предыдущий трек"),
    (0x06, "Следующий трек"),
    (0x03, "Голосовой ассистент"),
    (0x0B, "Громкость +"),
    (0x0C, "Громкость −"),
    (0x08, "Цикл шумоподавления"),
    (0x11, "Игровой режим"),
    (0x0D, "Переключить устройство"),
    (0x12, "Zen Mode Air"),
];

pub fn gesture_act(g: &str) -> Option<u8> {
    Some(match g {
        "single" => 1,
        "double" => 2,
        "triple" => 3,
        "hold" => 4,
        "slide" => 5,
        "longHold" => 6,
        _ => return None,
    })
}

pub fn gesture_name(a: u8) -> Option<&'static str> {
    Some(match a {
        1 => "single",
        2 => "double",
        3 => "triple",
        4 => "hold",
        5 => "slide",
        6 => "longHold",
        _ => return None,
    })
}

pub const SPATIAL_NAMES: [&str; 3] = ["Выкл", "Фиксированный", "Отслеживание головы"];

pub fn caps_for(s: &OpoSpec) -> Capabilities {
    let word = if s.pinch { "сжатие" } else { "касание" };
    let mut gestures = Vec::new();
    if s.tree == AncTree::Buds4 {
        for side in [Side::Left, Side::Right] {
            gestures.push(slot(side, "double", &format!("Двойное {word}")));
            gestures.push(slot(side, "triple", &format!("Тройное {word}")));
            gestures.push(slot(side, "hold", "Зажать"));
        }
    }
    Capabilities {
        anc_levels: s.levels.to_vec(),
        transparency: true,
        eq_presets: s.eq.iter().map(|(i, n)| opt(*i, n)).collect(),
        custom_eq_bands: s.custom_bands.to_vec(),
        custom_eq_range: (-6.0, 6.0),
        custom_eq_preset_id: None,
        bass: (!s.custom_bands.is_empty()).then_some(BassCap { kind: BassKind::BassWave, min: -5, max: 5 }),
        spatial_modes: if s.spatial_head {
            SPATIAL_NAMES.iter().map(|x| x.to_string()).collect()
        } else if s.tree == AncTree::Buds4 {
            SPATIAL_NAMES[..2].iter().map(|x| x.to_string()).collect()
        } else {
            vec![]
        },
        low_latency: true,
        in_ear_detection: true,
        multipoint: s.tree == AncTree::Buds4,
        fit_test: s.tree == AncTree::Buds4,
        find_buds: true,
        gestures,
        gesture_actions: GESTURE_FNS.iter().map(|(c, n)| opt(*c, n)).collect(),
        // Hi-Res (feature 0x18) is refused for a PC link (only honoured when the phone plays LHDC/LDAC)
        hi_res_options: vec![],
        wear_required_for_anc: true,
        personalized_anc: false,
    }
}

fn info(s: &OpoSpec) -> ModelInfo {
    ModelInfo { vendor: Vendor::Oneplus, code: s.pid.into(), name: s.name.into(), caps: caps_for(s) }
}

pub fn generic_model() -> ModelInfo {
    let mut caps = caps_for(&SPECS[0]);
    caps.anc_levels.clear();
    caps.eq_presets.clear();
    caps.custom_eq_bands.clear();
    caps.bass = None;
    caps.spatial_modes.clear();
    caps.gestures.clear();
    caps.fit_test = false;
    ModelInfo { vendor: Vendor::Oneplus, code: "OPO".into(), name: "HeyMelody-совместимые".into(), caps }
}

pub fn models() -> Vec<ModelInfo> {
    SPECS.iter().map(info).collect()
}

pub fn spec_by_pid(pid: &str) -> Option<&'static OpoSpec> {
    SPECS.iter().find(|s| s.pid.eq_ignore_ascii_case(pid))
}

pub fn identify_by_name(name: &str) -> Option<ModelInfo> {
    let n = name.trim();
    if let Some(s) = SPECS.iter().find(|s| s.name.eq_ignore_ascii_case(n) || s.aliases.iter().any(|a| a.eq_ignore_ascii_case(n))) {
        return Some(info(s));
    }
    let l = n.to_ascii_lowercase();
    if l.starts_with("oneplus") || l.starts_with("oppo enco") || l.starts_with("realme buds") || l.contains("nord buds") {
        return Some(generic_model());
    }
    None
}

// ---------------------------------------------------------------------------------------------
// ANC encoding
// ---------------------------------------------------------------------------------------------

/// SET mask (Buds4 tree) for a mode/level.
pub fn anc_set_bit(tree: AncTree, mode: AncMode, level: Option<AncLevel>) -> u32 {
    match tree {
        AncTree::Buds4 => match mode {
            AncMode::Off => 1 << 0,
            AncMode::Transparency => 1 << 2,
            AncMode::Anc => match level {
                None => 1 << 1,
                Some(AncLevel::High) => 1 << 4,
                Some(AncLevel::Mid) => 1 << 5,
                Some(AncLevel::Low) => 1 << 6,
                Some(AncLevel::Smart) => 1 << 7,
                Some(AncLevel::Adaptive) => 1 << 11,
            },
        },
        AncTree::BudsPro1 => match mode {
            AncMode::Off => 0x01,
            AncMode::Transparency => 0x02,
            AncMode::Anc => match level {
                Some(AncLevel::Low) => 0x04,
                Some(AncLevel::Smart) | Some(AncLevel::Adaptive) => 0x10,
                _ => 0x08,
            },
        },
    }
}

/// Minimal little-endian byte encoding of `1 << bit` (bit/8 + 1 bytes).
pub fn mask_bytes(mask: u32) -> Vec<u8> {
    let bit = 31 - mask.leading_zeros();
    let n = (bit / 8 + 1) as usize;
    mask.to_le_bytes()[..n].to_vec()
}

pub fn decode_report(tree: AncTree, mask: u32) -> Option<(AncMode, Option<AncLevel>)> {
    match tree {
        AncTree::Buds4 => {
            if mask & 0x0300 != 0 {
                Some((AncMode::Transparency, None))
            } else if mask & 0x7800 != 0 {
                Some((AncMode::Anc, Some(AncLevel::Adaptive)))
            } else if mask & 0x0080 != 0 {
                Some((AncMode::Anc, Some(AncLevel::Smart)))
            } else if mask & 0x0040 != 0 {
                Some((AncMode::Anc, Some(AncLevel::Low)))
            } else if mask & 0x0020 != 0 {
                Some((AncMode::Anc, Some(AncLevel::Mid)))
            } else if mask & 0x0010 != 0 {
                Some((AncMode::Anc, Some(AncLevel::High)))
            } else if mask & 0x0008 != 0 || mask == 0x0001 {
                Some((AncMode::Off, None))
            } else if mask & 0x0002 != 0 {
                Some((AncMode::Anc, None))
            } else if mask & 0x0004 != 0 {
                Some((AncMode::Transparency, None))
            } else {
                None
            }
        }
        AncTree::BudsPro1 => match mask {
            0x01 => Some((AncMode::Off, None)),
            0x02 => Some((AncMode::Transparency, None)),
            0x04 => Some((AncMode::Anc, Some(AncLevel::Low))),
            0x08 => Some((AncMode::Anc, Some(AncLevel::High))),
            0x10 => Some((AncMode::Anc, Some(AncLevel::Smart))),
            _ => None,
        },
    }
}

fn le_mask(b: &[u8]) -> u32 {
    b.iter().take(4).enumerate().fold(0u32, |acc, (i, &x)| acc | (x as u32) << (8 * i))
}

/// Custom EQ preset as stored on the buds.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomEq {
    pub selected: bool,
    pub id: u8,
    pub name: String,
    pub bands: Vec<(u16, i8)>,
}

pub fn parse_custom_eq_list(p: &[u8]) -> Vec<CustomEq> {
    let mut out = Vec::new();
    if p.len() < 2 || p[0] != 0 {
        return out;
    }
    let count = p[1] as usize;
    let mut i = 2;
    for _ in 0..count {
        let Some(hdr) = p.get(i..i + 5) else { break };
        let (selected, id, name_len) = (hdr[0] != 0, hdr[3], hdr[4] as usize);
        i += 5;
        let Some(name) = p.get(i..i + name_len) else { break };
        let name = String::from_utf8_lossy(name).into_owned();
        i += name_len;
        let Some(&n) = p.get(i) else { break };
        i += 1;
        let mut bands = Vec::new();
        for _ in 0..n {
            let Some(b) = p.get(i..i + 3) else { break };
            bands.push((u16::from_le_bytes([b[0], b[1]]), b[2] as i8));
            i += 3;
        }
        out.push(CustomEq { selected, id, name, bands });
    }
    out
}

pub fn custom_eq_payload(action: u8, id: u8, name: &str, bands: &[(u16, i8)]) -> Vec<u8> {
    let mut p = vec![action, 0xFA, 0x06, id, name.len() as u8];
    p.extend_from_slice(name.as_bytes());
    p.push(bands.len() as u8);
    for (f, g) in bands {
        p.extend_from_slice(&f.to_le_bytes());
        p.push(*g as u8);
    }
    p
}

// ---------------------------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------------------------

const OUR_EQ_NAME: &str = "BudsPulse";
const ANC_TIMEOUT: Duration = Duration::from_millis(4500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Handshake,
    Identify,
    Running,
}

pub struct OnePlusDriver {
    transport: TransportKind,
    seq: u8,
    split: Splitter,
    pending: PendingTracker,
    phase: Phase,
    started: Option<Instant>,
    ready_at: Option<Instant>,
    bitmap: Vec<u8>,
    spec: Option<&'static OpoSpec>,
    reads_sent: bool,
    /// request seq → pending key (resolved by the 0x84xx write reply)
    seq_keys: HashMap<u8, String>,
    /// requested ANC state, confirmed only by a report (push 03 or 0x810C)
    anc_intent: Option<(AncMode, Option<AncLevel>)>,
    custom_eq: Vec<CustomEq>,
    eq_create_gains: Option<Vec<i8>>,
    switches: HashMap<u8, bool>,
    case_last_seen: Option<u64>,
    fit_deadline: Option<Instant>,
}

impl OnePlusDriver {
    pub fn new(transport: TransportKind) -> Self {
        OnePlusDriver {
            transport,
            seq: 0,
            split: Splitter::default(),
            pending: PendingTracker::default(),
            phase: Phase::Handshake,
            started: None,
            ready_at: None,
            bitmap: vec![],
            spec: None,
            reads_sent: false,
            seq_keys: HashMap::new(),
            anc_intent: None,
            custom_eq: vec![],
            eq_create_gains: None,
            switches: HashMap::new(),
            case_last_seen: None,
            fit_deadline: None,
        }
    }

    fn next_seq(&mut self) -> u8 {
        self.seq = if self.seq >= 0xBF { 1 } else { self.seq + 1 };
        self.seq
    }

    fn msg(&mut self, cmd: u16, payload: &[u8]) -> Outgoing {
        let seq = self.next_seq();
        let pace = if self.transport == TransportKind::Ble { 250 } else { 180 };
        Outgoing::new(build(cmd, seq, payload), describe(cmd, seq, payload)).delay(pace)
    }

    fn keyed(&mut self, cmd: u16, payload: &[u8], key: &str) -> Outgoing {
        let o = self.msg(cmd, payload);
        self.seq_keys.insert(o.bytes[o.bytes.len() - payload.len() - 3], key.to_string());
        o
    }

    fn has_bit(&self, bit: usize) -> bool {
        if self.bitmap.is_empty() {
            return true;
        }
        self.bitmap.get(bit / 8).map(|b| b & (1 << (bit % 8)) != 0).unwrap_or(false)
    }

    fn tree(&self) -> AncTree {
        self.spec.map(|s| s.tree).unwrap_or(AncTree::Buds4)
    }

    fn game_switch(&self) -> u8 {
        match self.spec {
            Some(s) if s.pid == "064014" => F_GAME,
            Some(s) => s.game_switch,
            None => F_GAME,
        }
    }

    fn reads(&mut self, state: &DeviceState) -> Vec<Outgoing> {
        let sq: Vec<u8> = self.spec.map(|s| s.status_query.to_vec()).unwrap_or_else(|| vec![0x04, 0x06, 0x11, 0x1D]);
        let mut payload = vec![sq.len() as u8];
        payload.extend_from_slice(&sq);
        let mut q = vec![self.msg(CMD_SWITCHES_GET, &payload), self.msg(CMD_ANC_GET, &[0x01, 0x01])];
        if state.caps.anc_levels.contains(&AncLevel::Smart) {
            q.push(self.msg(CMD_ANC_GET, &[0x04, 0x01]));
        }
        q.push(self.msg(CMD_BATTERY, &[]));
        if self.has_bit(4) {
            q.push(self.msg(CMD_WEAR, &[]));
        }
        if self.has_bit(10) && !state.caps.eq_presets.is_empty() {
            q.push(self.msg(CMD_EQ_GET, &[]));
        }
        if self.has_bit(34) && !state.caps.custom_eq_bands.is_empty() {
            q.push(self.msg(CMD_CUSTOM_EQ_LIST, &[]));
        }
        if self.has_bit(38) && state.caps.bass.is_some() {
            q.push(self.msg(CMD_BASSWAVE_GET, &[]));
        }
        if self.has_bit(47) && !state.caps.spatial_modes.is_empty() {
            q.push(self.msg(CMD_SPATIAL_GET, &[]));
        }
        if self.has_bit(0) {
            q.push(self.msg(CMD_FIRMWARE, &[]));
        }
        q.push(self.msg(CMD_CODEC_GET, &[]));
        if self.has_bit(3) && !state.caps.gestures.is_empty() {
            q.push(self.msg(CMD_GESTURES, &[]));
        }
        q
    }

    fn subscribe(&mut self, offered: &[u8]) -> Vec<Outgoing> {
        let wanted: Vec<u8> = offered.iter().copied().filter(|e| [1u8, 2, 3, 4, 5, 6].contains(e)).collect();
        if wanted.is_empty() {
            return vec![];
        }
        if self.has_bit(22) {
            let mut p = vec![wanted.len() as u8];
            p.extend_from_slice(&wanted);
            vec![self.msg(CMD_SUBSCRIBE, &p)]
        } else {
            wanted.iter().map(|e| self.msg(CMD_SUBSCRIBE_ONE, &[*e])).collect()
        }
    }

    fn start_running(&mut self, state: &DeviceState, offered: &[u8]) -> Vec<Outgoing> {
        self.phase = Phase::Running;
        if self.reads_sent {
            return vec![];
        }
        self.reads_sent = true;
        let mut out = self.subscribe(offered);
        out.extend(self.reads(state));
        out
    }

    fn answer_request(&mut self, f: &Frame) -> Option<Outgoing> {
        let reply = f.cmd | 0x8000;
        let payload: Vec<u8> = match f.cmd {
            CMD_TIME_REQ => {
                let mut p = vec![0x00];
                p.extend_from_slice(&((now_ms() / 1000) as u32).to_le_bytes());
                if self.spec.map(|s| s.utc_tz).unwrap_or(false) {
                    p.extend_from_slice(&local_utc_offset_secs().to_le_bytes());
                }
                p
            }
            0x050C => vec![0x00, 0x01, 0x00],
            0x051C => vec![0x00, 0x00, 0x00],
            0x051D => vec![0x01],
            0x0501 | 0x0502 | CMD_SWITCH_PUSH | CMD_EQ_PUSH | 0x0506 | 0x0508 | CMD_SPATIAL_PUSH => return None,
            c if c >> 8 == 0x05 => vec![0x01],
            _ => return None,
        };
        Some(Outgoing::new(build(reply, f.seq, &payload), describe(reply, f.seq, &payload)))
    }

    fn apply_battery(&mut self, entries: &[u8], state: &mut DeviceState) {
        let Some(&n) = entries.first() else { return };
        let mut seen_case = false;
        let (mut l, mut r) = (None, None);
        for i in 0..n as usize {
            let Some(e) = entries.get(1 + 2 * i..3 + 2 * i) else { break };
            let lvl = e[1] & 0x7F;
            let b = BudBattery { level: (lvl > 0).then_some(lvl), charging: e[1] & 0x80 != 0 };
            match e[0] {
                1 => l = Some(b),
                2 => r = Some(b),
                3
                    if b.level.is_some() => {
                        seen_case = true;
                        state.battery.case = b;
                        state.battery.case_cached_at = None;
                        self.case_last_seen = Some(now_ms());
                    }
                _ => {}
            }
        }
        if let Some(b) = l {
            state.battery.left = b;
        }
        if let Some(b) = r {
            state.battery.right = b;
        }
        if !seen_case && state.battery.case.level.is_some() && state.battery.case_cached_at.is_none() {
            state.battery.case.charging = false;
            state.battery.case_cached_at = Some(self.case_last_seen.unwrap_or_else(now_ms));
        }
    }

    fn apply_wear(&mut self, entries: &[u8], state: &mut DeviceState) -> Option<DriverEvent> {
        let n = *entries.first()? as usize;
        let before = state.wear.clone();
        for i in 0..n {
            let Some(e) = entries.get(1 + 2 * i..3 + 2 * i) else { break };
            let pos = if e[1] & 0x02 != 0 {
                WearPosition::Ear
            } else if e[1] & 0x01 != 0 {
                WearPosition::Out
            } else {
                WearPosition::Case
            };
            match e[0] {
                1 => state.wear.left = pos,
                2 => state.wear.right = pos,
                _ => {}
            }
        }
        (before != state.wear).then(|| DriverEvent::Wear { before, after: state.wear.clone() })
    }

    fn apply_anc_report(&mut self, mask: u32, state: &mut DeviceState, ev: &mut Vec<DriverEvent>) {
        let Some((mode, level)) = decode_report(self.tree(), mask) else { return };
        state.anc.mode = Some(mode);
        if level.is_some() {
            state.anc.level = level;
        }
        if let Some((im, il)) = self.anc_intent {
            let matches = im == mode && (im != AncMode::Anc || il.is_none() || level.is_none() || il == level);
            if matches {
                self.anc_intent = None;
                self.pending.forget(state, "anc");
                ev.push(DriverEvent::Ack { key: "anc".into(), ok: true, message: None });
            }
        }
    }

    fn apply_switches(&mut self, pairs: &[u8], state: &mut DeviceState, ev: &mut Vec<DriverEvent>) {
        for c in pairs.chunks(2) {
            if c.len() < 2 {
                break;
            }
            let (id, on) = (c[0], c[1] != 0);
            self.switches.insert(id, on);
            let key = match id {
                F_WEAR_DETECT => {
                    state.in_ear_detection = Some(on);
                    "inEarDetection"
                }
                F_GAME | F_GAME_ALT if id == self.game_switch() || !self.switches.contains_key(&self.game_switch()) => {
                    state.low_latency = Some(on);
                    "lowLatency"
                }
                F_MULTIPOINT => {
                    state.multipoint = Some(on);
                    "multipoint"
                }
                F_BASSWAVE => {
                    state.bass.enabled = Some(on);
                    "bass"
                }
                F_SPATIAL if !self.has_bit(47) => {
                    state.spatial = Some(if on { SPATIAL_NAMES[1] } else { SPATIAL_NAMES[0] }.into());
                    "spatial"
                }
                _ => continue,
            };
            if self.pending.has(key) && !self.seq_keys.values().any(|k| k == key) {
                self.pending.forget(state, key);
                ev.push(DriverEvent::Ack { key: key.into(), ok: true, message: None });
            }
        }
    }

    fn handle(&mut self, f: &Frame, state: &mut DeviceState, ev: &mut Vec<DriverEvent>) {
        if f.cmd >> 8 == 0x05 && f.cmd & 0x8000 == 0 {
            if let Some(a) = self.answer_request(f) {
                ev.push(DriverEvent::Send(vec![a]));
            }
        }
        let p = &f.payload[..];
        match f.cmd {
            0x8100 => {
                self.bitmap = p.get(1..).unwrap_or_default().to_vec();
                if self.phase == Phase::Handshake {
                    self.phase = Phase::Identify;
                    self.ready_at = Some(Instant::now());
                    ev.push(DriverEvent::Ready);
                }
            }
            0x8103 if p.len() >= 4 => {
                let pid = format!("{:02X}{:02X}{:02X}", p[3], p[2], p[1]);
                if let Some(spec) = spec_by_pid(&pid) {
                    // the BT name wins when it maps to a known model (HeyMelody rule)
                    if self.spec.is_none() || spec_by_pid(&state.model_code).is_none() {
                        self.spec = Some(spec);
                        if state.model_code != spec.pid {
                            state.set_model(&info(spec));
                        }
                    }
                } else if self.spec.is_none() {
                    state.model_code = pid;
                }
            }
            0x8200 => {
                let offered: Vec<u8> = if p.len() >= 2 { p[2..].iter().copied().take(p[1] as usize).collect() } else { vec![] };
                let out = self.start_running(state, &offered);
                if !out.is_empty() {
                    ev.push(DriverEvent::Send(out));
                }
            }
            0x8105 if p.len() >= 2 => {
                let text = String::from_utf8_lossy(&p[2..]).to_string();
                let parts: Vec<&str> = text.split(',').map(|s| s.trim()).collect();
                let mut fw: Vec<(u8, String)> = parts
                    .chunks(3)
                    .filter(|c| c.len() == 3 && c[1] == "2")
                    .filter_map(|c| c[0].parse::<u8>().ok().map(|part| (part, c[2].to_string())))
                    .collect();
                fw.sort_by_key(|x| x.0);
                if !fw.is_empty() {
                    state.firmware = Some(fw.into_iter().map(|x| x.1).collect::<Vec<_>>().join("."));
                } else if !text.is_empty() {
                    state.firmware = Some(text);
                }
            }
            0x8106 if p.first() == Some(&0) => self.apply_battery(&p[1..], state),
            0x8109 => {
                // some replies omit the status byte
                let body = if !p.is_empty() && p.len() == 1 + 2 * p[0] as usize { p } else { p.get(1..).unwrap_or_default() };
                if let Some(e) = self.apply_wear(body, state) {
                    ev.push(e);
                }
            }
            0x810C if p.len() >= 4 && p[0] == 0 => {
                if p[1] == 0x01 {
                    self.apply_anc_report(le_mask(&p[3..]), state, ev);
                } else if p[1] == 0x04 && state.anc.level == Some(AncLevel::Smart) {
                    // level chosen by Smart: informational only
                }
            }
            0x810D if p.len() >= 2 && p[0] == 0 => {
                let n = p[1] as usize;
                let pairs = p.get(2..2 + 2 * n).unwrap_or(&p[2..]).to_vec();
                self.apply_switches(&pairs, state, ev);
            }
            0x810F if p.len() >= 2 && p[0] == 0 => {
                let id = p[1];
                state.eq.preset_id = Some(id);
                self.pending.forget(state, "eq");
            }
            0x8114 if p.len() >= 2 && p[0] == 0 => {
                state.codec = Some(
                    match p[1] {
                        1 => "SBC",
                        2 => "AAC",
                        3 => "LDAC",
                        7 => "LHDC",
                        8 => "LHDC 5.0",
                        _ => return,
                    }
                    .into(),
                );
            }
            0x8122 => {
                self.custom_eq = parse_custom_eq_list(p);
                // expose custom presets as selectable presets
                state.caps.eq_presets.retain(|o| self.spec.map(|s| s.eq.iter().any(|(i, _)| *i == o.id)).unwrap_or(true));
                for c in &self.custom_eq {
                    if !state.caps.eq_presets.iter().any(|o| o.id == c.id) {
                        state.caps.eq_presets.push(opt(c.id, &format!("★ {}", c.name)));
                    }
                }
                let ours = self.custom_eq.iter().find(|c| c.name == OUR_EQ_NAME).or_else(|| self.custom_eq.iter().find(|c| c.selected));
                if let Some(c) = ours {
                    state.caps.custom_eq_preset_id = Some(c.id);
                    let bands = &state.caps.custom_eq_bands;
                    state.eq.custom_gains =
                        bands.iter().map(|f| c.bands.iter().find(|b| b.0 as u32 == *f).map(|b| b.1 as f32).unwrap_or(0.0)).collect();
                    if c.selected {
                        state.eq.preset_id = Some(c.id);
                    }
                }
            }
            0x8124 if p.len() >= 4 && p[0] == 0 => {
                state.bass.level = Some(p[3] as i8);
            }
            0x812A if p.len() >= 2 && p[0] == 0 => {
                state.spatial = SPATIAL_NAMES.get(p[1] as usize).map(|s| s.to_string());
            }
            0x8108 if p.len() >= 2 && p[0] == 0 => {
                let n = p[1] as usize;
                let mut list = Vec::new();
                for i in 0..n {
                    let Some(r) = p.get(2 + 4 * i..6 + 4 * i) else { break };
                    if r[1] != 0x01 {
                        continue;
                    }
                    let Some(g) = gesture_name(r[2]) else { continue };
                    let sides: &[Side] = match r[0] {
                        1 => &[Side::Left],
                        2 => &[Side::Right],
                        4 => &[Side::Left, Side::Right],
                        _ => &[],
                    };
                    for s in sides {
                        list.push(GestureBinding { side: *s, gesture: g.into(), action: r[3] });
                    }
                }
                state.gestures = list;
                for k in self.pending.keys() {
                    if k.starts_with("gesture:") {
                        self.pending.forget(state, &k);
                        ev.push(DriverEvent::Ack { key: k, ok: true, message: None });
                    }
                }
            }
            CMD_PUSH if !p.is_empty() => {
                let body = &p[1..];
                match p[0] {
                    0x01 => self.apply_battery(body, state),
                    0x02 => {
                        if let Some(e) = self.apply_wear(body, state) {
                            ev.push(e);
                        }
                    }
                    0x03 if body.len() >= 3 && body[0] == 0x01 && body[1] == 0x01 => self.apply_anc_report(le_mask(&body[2..]), state, ev),
                    0x04 => {
                        let mut fit = FitState { running: false, left: None, right: None };
                        for c in body.chunks(2) {
                            if c.len() < 2 {
                                break;
                            }
                            let q = if c[1] == 1 { FitQuality::Good } else { FitQuality::Poor };
                            match c[0] {
                                1 => fit.left = Some(q),
                                2 => fit.right = Some(q),
                                _ => {}
                            }
                        }
                        state.fit = fit;
                        self.fit_deadline = None;
                    }
                    0x05 => {
                        if let Some(&s) = body.last() {
                            state.low_latency = Some(s != 0);
                            self.pending.forget(state, "lowLatency");
                        }
                    }
                    0xF2 => ev.push(DriverEvent::Send(vec![Outgoing::new(build(0x8204, f.seq, &[0x00, 0xF2]), "REPLY 0x8204 подтверждение F2")])),
                    0xF4 => {
                        let text = String::from_utf8_lossy(body);
                        let reply: Vec<u8> = if body.is_empty() {
                            vec![0x00, 0xF4]
                        } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                            let mut r = vec![0x00, 0xF4];
                            let cmd = v.get("cmd").cloned().unwrap_or(serde_json::Value::Null);
                            r.extend_from_slice(format!("{{\"cmd\":{cmd}}}").as_bytes());
                            r
                        } else {
                            vec![0x01, 0xF4]
                        };
                        ev.push(DriverEvent::Send(vec![Outgoing::new(build(0x8204, f.seq, &reply), "REPLY 0x8204 подтверждение F4")]));
                    }
                    _ => {}
                }
            }
            CMD_SWITCH_PUSH if !p.is_empty() => {
                let n = p[0] as usize;
                let pairs = p.get(1..1 + 2 * n).unwrap_or(&p[1..]).to_vec();
                self.apply_switches(&pairs, state, ev);
            }
            CMD_EQ_PUSH if !p.is_empty() => {
                state.eq.preset_id = Some(p[0]);
                self.pending.forget(state, "eq");
            }
            CMD_SPATIAL_PUSH if !p.is_empty() => {
                state.spatial = SPATIAL_NAMES.get(p[0] as usize).map(|s| s.to_string());
            }
            cmd if cmd & 0xFF00 == 0x8400 => self.handle_write_reply(f, state, ev),
            _ => {}
        }
    }

    fn handle_write_reply(&mut self, f: &Frame, state: &mut DeviceState, ev: &mut Vec<DriverEvent>) {
        let status = f.payload.first().copied().unwrap_or(0);
        if f.cmd == 0x8418 && status == 0 {
            // custom EQ created → save & select it with our gains
            if let (Some(gains), Some(&id)) = (self.eq_create_gains.take(), f.payload.get(1)) {
                let bands: Vec<(u16, i8)> = state.caps.custom_eq_bands.iter().zip(gains.iter()).map(|(f, g)| (*f as u16, *g)).collect();
                let payload = custom_eq_payload(0x02, id, OUR_EQ_NAME, &bands);
                let mut out = vec![self.keyed(CMD_CUSTOM_EQ_SET, &payload, "customEq")];
                out.push(self.msg(CMD_CUSTOM_EQ_LIST, &[]));
                ev.push(DriverEvent::Send(out));
                self.seq_keys.remove(&f.seq);
                return;
            }
        }
        let Some(key) = self.seq_keys.remove(&f.seq) else { return };
        if f.cmd == 0x8404 {
            // ANC: the ack is not proof — wait for the report
            match status {
                0x00 => {
                    let q = self.msg(CMD_ANC_GET, &[0x01, 0x01]).delay(0);
                    ev.push(DriverEvent::Send(vec![q]));
                }
                0x0E => {
                    self.anc_intent = None;
                    if let Some(e) = self.pending.reject(state, &key, "наденьте хотя бы один наушник — без этого шумоподавление не переключается") {
                        ev.push(e);
                    }
                }
                s => {
                    self.anc_intent = None;
                    if let Some(e) = self.pending.reject(state, &key, &format!("наушники отклонили команду (статус 0x{s:02X})")) {
                        ev.push(e);
                    }
                }
            }
            return;
        }
        let e = match status {
            0x00 => self.pending.confirm(state, &key),
            0x02 => self.pending.reject(state, &key, "наушники сейчас отказали (условие не выполнено)"),
            s => self.pending.reject(state, &key, &format!("функция не поддерживается (статус 0x{s:02X})")),
        };
        if let Some(e) = e {
            ev.push(e);
        }
        if key.starts_with("gesture:") || key == "customEq" {
            // verify by reading back
            let q = if key == "customEq" { self.msg(CMD_CUSTOM_EQ_LIST, &[]) } else { self.msg(CMD_GESTURES, &[]) };
            ev.push(DriverEvent::Send(vec![q]));
        }
    }
}

/// Local UTC offset in seconds (for the Pro 4 time answer). Uses the TZ offset the OS reports.
pub fn local_utc_offset_secs() -> i32 {
    #[cfg(windows)]
    {
        use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
        let mut tzi = TIME_ZONE_INFORMATION::default();
        // SAFETY: valid out pointer
        let r = unsafe { GetTimeZoneInformation(&mut tzi) };
        let dst = if r == 2 { tzi.DaylightBias } else { tzi.StandardBias };
        return -(tzi.Bias + dst) * 60;
    }
    #[cfg(unix)]
    {
        // SAFETY: localtime_r with valid pointers
        unsafe {
            let t = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            libc::localtime_r(&t, &mut tm);
            return tm.tm_gmtoff as i32;
        }
    }
    #[allow(unreachable_code)]
    0
}

impl EarbudDriver for OnePlusDriver {
    fn vendor(&self) -> Vendor {
        Vendor::Oneplus
    }

    fn session_start(&mut self) -> Vec<Outgoing> {
        self.started = Some(Instant::now());
        self.phase = Phase::Handshake;
        let mut hs = self.msg(CMD_HANDSHAKE, &[]);
        hs.delay_after = Duration::from_millis(200);
        vec![hs]
    }

    fn needs_handshake_reply(&self) -> bool {
        true
    }

    fn initial_queries(&mut self, state: &DeviceState) -> Vec<Outgoing> {
        if self.spec.is_none() {
            self.spec = spec_by_pid(&state.model_code);
        }
        vec![self.msg(CMD_PRODUCT_ID, &[]), self.msg(CMD_EVENTS_LIST, &[])]
    }

    fn poll_queries(&mut self, _state: &DeviceState) -> Vec<Outgoing> {
        vec![self.msg(CMD_BATTERY, &[])]
    }

    fn encode(&mut self, cmd: &DeviceCommand, state: &mut DeviceState) -> Result<Vec<Outgoing>, ProtoError> {
        let key = cmd.pending_key();
        let mut out = Vec::new();
        match cmd.clone() {
            DeviceCommand::SetAnc { mode, level } => {
                if state.wear.known() && !state.wear.any_in_ear() {
                    return Err(ProtoError::NotWorn);
                }
                let level = if mode == AncMode::Anc { level.or(state.anc.level) } else { None };
                if let Some(l) = level {
                    if !state.caps.anc_levels.contains(&l) {
                        return Err(ProtoError::Unsupported("уровень шумоподавления"));
                    }
                }
                let mask = anc_set_bit(self.tree(), mode, level);
                let mut p = vec![0x01, 0x01];
                p.extend(mask_bytes(mask));
                out.push(self.keyed(CMD_ANC_SET, &p, &key));
                self.anc_intent = Some((mode, level));
                self.pending.add_with_timeout(state, &key, ANC_TIMEOUT, Box::new(move |s| {
                    s.anc.mode = Some(mode);
                    if level.is_some() {
                        s.anc.level = level;
                    }
                }));
            }
            DeviceCommand::SetEqPreset { id } => {
                if !state.caps.eq_presets.iter().any(|p| p.id == id) {
                    return Err(ProtoError::Invalid(format!("пресет {id}")));
                }
                out.push(self.keyed(CMD_EQ_SET, &[id], &key));
                self.pending.add(state, &key, Box::new(move |s| s.eq.preset_id = Some(id)));
            }
            DeviceCommand::SetCustomEq { gains } => {
                let bands = state.caps.custom_eq_bands.clone();
                if bands.is_empty() {
                    return Err(ProtoError::Unsupported("свой эквалайзер"));
                }
                if gains.len() != bands.len() {
                    return Err(ProtoError::Invalid(format!("нужно {} полос", bands.len())));
                }
                let g: Vec<i8> = gains.iter().map(|x| x.clamp(-6.0, 6.0).round() as i8).collect();
                let existing = self.custom_eq.iter().find(|c| c.name == OUR_EQ_NAME).map(|c| c.id);
                match existing {
                    Some(id) => {
                        let wire: Vec<(u16, i8)> = bands.iter().zip(g.iter()).map(|(f, v)| (*f as u16, *v)).collect();
                        out.push(self.keyed(CMD_CUSTOM_EQ_SET, &custom_eq_payload(0x02, id, OUR_EQ_NAME, &wire), &key));
                        let gv: Vec<f32> = g.iter().map(|x| *x as f32).collect();
                        self.pending.add(state, &key, Box::new(move |s| {
                            s.eq.custom_gains = gv;
                            s.eq.preset_id = Some(id);
                        }));
                    }
                    None => {
                        if self.custom_eq.len() >= 3 {
                            return Err(ProtoError::Invalid("в наушниках уже 3 своих пресета — удалите один в HeyMelody".into()));
                        }
                        let wire: Vec<(u16, i8)> = bands.iter().zip(g.iter()).map(|(f, v)| (*f as u16, *v)).collect();
                        out.push(self.msg(CMD_CUSTOM_EQ_SET, &custom_eq_payload(0x01, 0x00, OUR_EQ_NAME, &wire)));
                        self.eq_create_gains = Some(g.clone());
                        let gv: Vec<f32> = g.iter().map(|x| *x as f32).collect();
                        self.pending.add_with_timeout(state, &key, Duration::from_secs(5), Box::new(move |s| s.eq.custom_gains = gv));
                    }
                }
            }
            DeviceCommand::SetBass { enabled, level } => {
                if state.caps.bass.is_none() {
                    return Err(ProtoError::Unsupported("BassWave"));
                }
                let level = level.clamp(-5, 5);
                if state.bass.enabled != Some(enabled) {
                    out.push(self.keyed(CMD_SWITCH_SET, &[F_BASSWAVE, u8::from(enabled)], &key));
                }
                if enabled && state.bass.level != Some(level) && self.has_bit(38) {
                    let o = if out.is_empty() { self.keyed(CMD_BASSWAVE_SET, &[0xFB, 0x05, level as u8], &key) } else { self.msg(CMD_BASSWAVE_SET, &[0xFB, 0x05, level as u8]) };
                    out.push(o);
                }
                if out.is_empty() {
                    return Ok(out);
                }
                self.pending.add(state, &key, Box::new(move |s| s.bass = BassState { enabled: Some(enabled), level: Some(level) }));
            }
            DeviceCommand::SetLowLatency { enabled } => {
                let id = self.game_switch();
                out.push(self.keyed(CMD_SWITCH_SET, &[id, u8::from(enabled)], &key));
                self.pending.add(state, &key, Box::new(move |s| s.low_latency = Some(enabled)));
            }
            DeviceCommand::SetInEarDetection { enabled } => {
                out.push(self.keyed(CMD_SWITCH_SET, &[F_WEAR_DETECT, u8::from(enabled)], &key));
                self.pending.add(state, &key, Box::new(move |s| s.in_ear_detection = Some(enabled)));
            }
            DeviceCommand::SetMultipoint { enabled } => {
                out.push(self.keyed(CMD_SWITCH_SET, &[F_MULTIPOINT, u8::from(enabled)], &key));
                out.push(self.msg(CMD_MULTIPOINT_OP, &[0x08, 0x00, if enabled { 0x00 } else { 0x01 }]));
                self.pending.add(state, &key, Box::new(move |s| s.multipoint = Some(enabled)));
            }
            DeviceCommand::SetSpatial { mode } => {
                let idx = state.caps.spatial_modes.iter().position(|m| *m == mode).ok_or(ProtoError::Unsupported("пространственный звук"))?;
                if self.has_bit(47) {
                    out.push(self.keyed(CMD_SPATIAL_SET, &[idx as u8], &key));
                } else {
                    out.push(self.keyed(CMD_SWITCH_SET, &[F_SPATIAL, u8::from(idx > 0)], &key));
                }
                self.pending.add(state, &key, Box::new(move |s| s.spatial = Some(mode)));
            }
            DeviceCommand::SetGesture { side, gesture, action } => {
                let act = gesture_act(&gesture).ok_or_else(|| ProtoError::Invalid(gesture.clone()))?;
                let dev = if side == Side::Left { 0x01 } else { 0x02 };
                out.push(self.keyed(CMD_GESTURE_SET, &[0x01, dev, 0x01, act, action], &key));
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
                if !state.caps.fit_test || !self.has_bit(9) {
                    return Err(ProtoError::Unsupported("тест прилегания"));
                }
                out.push(self.msg(CMD_FIT_TEST, &[0x01]));
                state.fit = FitState { running: true, left: None, right: None };
                self.fit_deadline = Some(Instant::now() + Duration::from_secs(20));
            }
            DeviceCommand::FindBuds { enabled, .. } => {
                // OPO rings both buds; no side byte
                out.push(self.keyed(CMD_FIND, &[u8::from(enabled)], "find:both"));
                self.pending.add(state, "find:both", Box::new(move |s| {
                    s.find_buds.left = enabled;
                    s.find_buds.right = enabled;
                }));
            }
            DeviceCommand::SetHiRes { .. } => return Err(ProtoError::Unsupported("Hi-Res с ПК (наушники принимают его только от телефона)")),
            DeviceCommand::SetPersonalizedAnc { .. } => return Err(ProtoError::Unsupported("персональное ANC")),
            DeviceCommand::Refresh => out.extend(self.reads(state)),
        }
        Ok(out)
    }

    fn feed(&mut self, data: &[u8], state: &mut DeviceState) -> Vec<DriverEvent> {
        let mut ev = Vec::new();
        for f in self.split.push(data) {
            ev.push(DriverEvent::Rx { bytes: f.raw.clone(), desc: describe(f.cmd, f.seq, &f.payload) });
            self.handle(&f, state, &mut ev);
            ev.push(DriverEvent::Changed);
        }
        ev
    }

    fn tick(&mut self, now: Instant, state: &mut DeviceState) -> Vec<DriverEvent> {
        let mut ev = Vec::new();
        // some firmwares (and Gadgetbridge-style hosts) work without the 0x0100 reply
        if self.phase == Phase::Handshake {
            if let Some(t) = self.started {
                if now.duration_since(t) > Duration::from_millis(2500) {
                    self.phase = Phase::Identify;
                    self.ready_at = Some(now);
                    ev.push(DriverEvent::Ready);
                }
            }
        }
        if self.phase == Phase::Identify && !self.reads_sent {
            if let Some(t) = self.ready_at {
                if now.duration_since(t) > Duration::from_millis(2500) {
                    let out = self.start_running(state, &[1, 2, 3, 4, 5, 6]);
                    ev.push(DriverEvent::Send(out));
                }
            }
        }
        let expired = self.pending.expire(now, state);
        if expired.iter().any(|e| matches!(e, DriverEvent::Ack { key, .. } if key == "anc")) {
            self.anc_intent = None;
            ev.push(DriverEvent::Send(vec![self.msg(CMD_ANC_GET, &[0x01, 0x01])]));
        }
        ev.extend(expired);
        if let Some(d) = self.fit_deadline {
            if now >= d {
                self.fit_deadline = None;
                state.fit.running = false;
                ev.push(DriverEvent::Ack { key: "fit".into(), ok: false, message: Some("тест прилегания не завершился".into()) });
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

    fn st(name: &str) -> DeviceState {
        let m = identify_by_name(name).expect("known model");
        let mut s = DeviceState::new("id", name, "00:00:00:00:00:00", &m);
        s.connection = ConnectionState::Connected;
        s
    }

    #[test]
    fn golden_frames() {
        assert_eq!(build(CMD_HANDSHAKE, 0x23, &[]), parse_hex("AA 07 00 00 00 01 23 00 00"));
        // the draft's "REGISTER token" is the answer to the buds' time request
        assert_eq!(build(0x8500, 0x41, &[0x00, 0xB5, 0x50, 0xA0, 0x69]), parse_hex("AA 0C 00 00 00 85 41 05 00 00 B5 50 A0 69"));
        assert_eq!(build(CMD_BATTERY, 0x25, &[]), parse_hex("AA 07 00 00 06 01 25 00 00"));
        assert_eq!(build(CMD_SUBSCRIBE, 0x11, &[6, 1, 2, 3, 4, 8, 0x0B]), parse_hex("AA 0E 00 00 05 02 11 07 00 06 01 02 03 04 08 0B"));
        assert_eq!(build(CMD_BASSWAVE_SET, 0x11, &[0xFB, 0x05, 0x03]), parse_hex("AA 0A 00 00 1B 04 11 03 00 FB 05 03"));
        assert_eq!(build(CMD_GESTURE_SET, 0x11, &[1, 1, 1, 2, 0x11]), parse_hex("AA 0C 00 00 01 04 11 05 00 01 01 01 02 11"));
        let rock: Vec<(u16, i8)> = vec![(62, 4), (250, 2), (1000, 0), (4000, 1), (8000, 3), (16000, 2)];
        assert_eq!(
            build(CMD_CUSTOM_EQ_SET, 0x11, &custom_eq_payload(0x01, 0x00, "Rock", &rock)),
            parse_hex("AA 23 00 00 18 04 11 1C 00 01 FA 06 00 04 52 6F 63 6B 06 3E 00 04 FA 00 02 E8 03 00 A0 0F 01 40 1F 03 80 3E 02")
        );
        // LEB128 for large frames
        let big = build(0x0418, 1, &[0u8; 124]);
        assert_eq!(&big[..3], &[0xAA, 0x83, 0x01]);
        assert_eq!(Splitter::default().push(&big).len(), 1);
    }

    #[test]
    fn anc_masks() {
        let t = AncTree::Buds4;
        let enc = |m, l| {
            let mut p = vec![1, 1];
            p.extend(mask_bytes(anc_set_bit(t, m, l)));
            build(CMD_ANC_SET, 0x11, &p)
        };
        assert_eq!(enc(AncMode::Off, None), parse_hex("AA 0A 00 00 04 04 11 03 00 01 01 01"));
        assert_eq!(enc(AncMode::Anc, None), parse_hex("AA 0A 00 00 04 04 11 03 00 01 01 02"));
        assert_eq!(enc(AncMode::Transparency, None), parse_hex("AA 0A 00 00 04 04 11 03 00 01 01 04"));
        assert_eq!(enc(AncMode::Anc, Some(AncLevel::High)), parse_hex("AA 0A 00 00 04 04 11 03 00 01 01 10"));
        assert_eq!(enc(AncMode::Anc, Some(AncLevel::Smart)), parse_hex("AA 0A 00 00 04 04 11 03 00 01 01 80"));
        assert_eq!(enc(AncMode::Anc, Some(AncLevel::Adaptive)), parse_hex("AA 0B 00 00 04 04 11 04 00 01 01 00 08"));
        assert_eq!(decode_report(t, 0x0008), Some((AncMode::Off, None)));
        assert_eq!(decode_report(t, 0x0100), Some((AncMode::Transparency, None)));
        assert_eq!(decode_report(t, 0x0020), Some((AncMode::Anc, Some(AncLevel::Mid))));
        assert_eq!(decode_report(t, 0x2000), Some((AncMode::Anc, Some(AncLevel::Adaptive))));
    }

    #[test]
    fn splitter_skips_stray_byte_and_splits_concatenated() {
        let mut s = Splitter::default();
        let mut stream = parse_hex("AA 07 00 00 00 01 23 00 00 12");
        stream.extend(parse_hex("AA 0F 00 00 06 81 25 08 00 00 03 01 64 02 64 03 00 AA 0C 00"));
        let f = s.push(&stream);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].cmd, 0x0100);
        assert_eq!(f[1].cmd, 0x8106);
        assert_eq!(f[1].seq, 0x25);
        let rest = s.push(&parse_hex("00 04 02 FF 05 00 03 01 01 08 00"));
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].cmd, CMD_PUSH);
        assert_eq!(rest[0].seq, PUSH_SEQ);
    }

    #[test]
    fn session_flow_and_pushes() {
        let mut d = OnePlusDriver::new(TransportKind::Rfcomm);
        let mut s = st("OnePlus Buds Pro 4");
        assert_eq!(s.model_code, "067814");
        d.session_start();
        let ev = d.feed(&build(0x8100, 1, &parse_hex("00 FF 77 5A EA 67 0E 20 07")), &mut s);
        assert!(ev.contains(&DriverEvent::Ready));
        d.initial_queries(&s);
        d.feed(&build(0x8103, 2, &[0x00, 0x14, 0x78, 0x06]), &mut s);
        let ev = d.feed(&build(0x8200, 3, &parse_hex("00 09 01 02 03 04 08 0B F1 F2 F3")), &mut s);
        let sent: Vec<&Outgoing> = ev.iter().filter_map(|e| if let DriverEvent::Send(v) = e { Some(v) } else { None }).flatten().collect();
        assert!(sent[0].bytes.ends_with(&[0x04, 0x01, 0x02, 0x03, 0x04]), "count-first subscribe: {:02X?}", sent[0].bytes);
        assert!(sent.iter().any(|o| o.bytes[4..6] == [0x0D, 0x01]));
        // battery reply + push
        d.feed(&parse_hex("AA 0F 00 00 06 81 F0 08 00 00 03 01 64 02 E4 03 50"), &mut s);
        assert_eq!(s.battery.right, BudBattery { level: Some(100), charging: true });
        assert_eq!(s.battery.case.level, Some(80));
        d.feed(&parse_hex("AA 0B 00 00 04 02 FF 04 00 01 01 01 5A"), &mut s);
        assert_eq!(s.battery.left.level, Some(90));
        assert!(s.battery.case_cached_at.is_some(), "case missing → cached");
        // wear push
        let ev = d.feed(&parse_hex("AA 0F 00 00 04 02 FF 08 00 02 03 01 03 02 03 03 04"), &mut s);
        assert_eq!(s.wear.left, WearPosition::Ear);
        assert!(ev.iter().any(|e| matches!(e, DriverEvent::Wear { .. })));
        // ANC push Off
        d.feed(&parse_hex("AA 0C 00 00 04 02 FF 05 00 03 01 01 08 00"), &mut s);
        assert_eq!(s.anc.mode, Some(AncMode::Off));
        // time request is answered with u32 time + tz (Pro 4)
        let ev = d.feed(&build(CMD_TIME_REQ, 0x41, &[]), &mut s);
        let ans = ev.iter().find_map(|e| if let DriverEvent::Send(v) = e { v.first().cloned() } else { None }).unwrap();
        assert_eq!(&ans.bytes[..9], &parse_hex("AA 10 00 00 00 85 41 09 00")[..]);
        // F2 push must be acknowledged
        let ev = d.feed(&build(CMD_PUSH, 0xFF, &[0xF2, 0x00]), &mut s);
        assert!(ev.iter().any(|e| matches!(e, DriverEvent::Send(v) if v[0].bytes == build(0x8204, 0xFF, &[0, 0xF2]))));
    }

    #[test]
    fn anc_waits_for_report_and_handles_not_worn() {
        let mut d = OnePlusDriver::new(TransportKind::Rfcomm);
        let mut s = st("OnePlus Buds Pro 3");
        s.anc.mode = Some(AncMode::Off);
        let out = d.encode(&DeviceCommand::SetAnc { mode: AncMode::Transparency, level: None }, &mut s).unwrap();
        let seq = out[0].bytes[6];
        assert_eq!(out[0].bytes[9..], [0x01, 0x01, 0x04]);
        // ack 00 → still pending, re-query issued
        let ev = d.feed(&build(0x8404, seq, &[0x00]), &mut s);
        assert!(ev.iter().any(|e| matches!(e, DriverEvent::Send(_))));
        assert_eq!(s.pending, vec!["anc".to_string()]);
        assert_eq!(s.anc.mode, Some(AncMode::Off));
        // report → confirmed
        let ev = d.feed(&parse_hex("AA 0C 00 00 04 02 FF 05 00 03 01 01 00 01"), &mut s);
        assert!(ev.iter().any(|e| matches!(e, DriverEvent::Ack { ok: true, .. })));
        assert_eq!(s.anc.mode, Some(AncMode::Transparency));
        assert!(s.pending.is_empty());
        // status 0E → rejected
        let out = d.encode(&DeviceCommand::SetAnc { mode: AncMode::Anc, level: Some(AncLevel::High) }, &mut s).unwrap();
        let ev = d.feed(&build(0x8404, out[0].bytes[6], &[0x0E]), &mut s);
        assert!(ev.iter().any(|e| matches!(e, DriverEvent::Ack { ok: false, .. })));
        assert_eq!(s.anc.mode, Some(AncMode::Transparency));
        // both buds out → refused locally
        s.wear = WearState { left: WearPosition::Out, right: WearPosition::Case };
        assert_eq!(d.encode(&DeviceCommand::SetAnc { mode: AncMode::Off, level: None }, &mut s), Err(ProtoError::NotWorn));
    }

    #[test]
    fn switches_eq_and_custom_eq_create_flow() {
        let mut d = OnePlusDriver::new(TransportKind::Rfcomm);
        let mut s = st("OnePlus Buds Pro 3");
        d.spec = spec_by_pid("064014");
        d.feed(&build(0x810D, 0, &parse_hex("00 08 05 00 04 01 0B 01 11 01 13 00 18 00 06 00 1D 01")), &mut s);
        assert_eq!(s.in_ear_detection, Some(true));
        assert_eq!(s.multipoint, Some(true));
        assert_eq!(s.low_latency, Some(false));
        assert_eq!(s.bass.enabled, Some(true));
        // game mode write → ack → confirmed
        let out = d.encode(&DeviceCommand::SetLowLatency { enabled: true }, &mut s).unwrap();
        assert_eq!(out[0].bytes[9..], [F_GAME, 1]);
        d.feed(&build(0x8403, out[0].bytes[6], &[0x00]), &mut s);
        assert_eq!(s.low_latency, Some(true));
        // EQ preset Dynaudio
        let out = d.encode(&DeviceCommand::SetEqPreset { id: 7 }, &mut s).unwrap();
        d.feed(&build(0x8406, out[0].bytes[6], &[0x00]), &mut s);
        assert_eq!(s.eq.preset_id, Some(7));
        // custom EQ: no existing → create, buds return id 4 → save&select sent
        let out = d.encode(&DeviceCommand::SetCustomEq { gains: vec![1.0, 2.0, 0.0, -1.0, 3.0, 6.0] }, &mut s).unwrap();
        assert_eq!(out[0].bytes[9], 0x01);
        let ev = d.feed(&build(0x8418, out[0].bytes[6], &[0x00, 0x04]), &mut s);
        let save = ev.iter().find_map(|e| if let DriverEvent::Send(v) = e { v.first().cloned() } else { None }).unwrap();
        assert_eq!(save.bytes[9..13], [0x02, 0xFA, 0x06, 0x04]);
        d.feed(&build(0x8418, save.bytes[6], &[0x00]), &mut s);
        assert_eq!(s.eq.custom_gains, vec![1.0, 2.0, 0.0, -1.0, 3.0, 6.0]);
        // list parse exposes the preset
        let mut list = vec![0x00, 0x01, 0x01, 0xFA, 0x06, 0x04, 9];
        list.extend_from_slice(b"BudsPulse");
        list.push(6);
        for (f, g) in [(62u16, 1i8), (250, 2), (1000, 0), (4000, -1), (8000, 3), (16000, 6)] {
            list.extend_from_slice(&f.to_le_bytes());
            list.push(g as u8);
        }
        d.feed(&build(0x8122, 9, &list), &mut s);
        assert_eq!(s.caps.custom_eq_preset_id, Some(4));
        assert_eq!(s.eq.preset_id, Some(4));
        assert!(s.caps.eq_presets.iter().any(|o| o.id == 4));
    }

    #[test]
    fn firmware_and_gestures() {
        let mut d = OnePlusDriver::new(TransportKind::Rfcomm);
        let mut s = st("OnePlus Buds 3");
        let mut p = vec![0x00, 0x05];
        p.extend_from_slice(b"1,1,01,1,2,127,2,1,01,2,2,127,3,2,101");
        d.feed(&build(0x8105, 1, &p), &mut s);
        assert_eq!(s.firmware.as_deref(), Some("127.127.101"));
        d.feed(&build(0x8108, 2, &parse_hex("00 03 01 01 02 06 02 01 02 05 04 01 04 08")), &mut s);
        assert!(s.gestures.contains(&GestureBinding { side: Side::Left, gesture: "double".into(), action: 6 }));
        assert!(s.gestures.contains(&GestureBinding { side: Side::Right, gesture: "hold".into(), action: 8 }));
    }

    #[test]
    fn identify() {
        assert_eq!(identify_by_name("OnePlus Buds Pro 4").unwrap().code, "067814");
        assert_eq!(identify_by_name("OnePlus Buds Bohr").unwrap().code, "067814");
        assert_eq!(identify_by_name("OnePlus Nord Buds 3 Pro").unwrap().caps.eq_presets[0].id, 3);
        assert_eq!(identify_by_name("OPPO Enco X3").unwrap().code, "OPO");
    }
}

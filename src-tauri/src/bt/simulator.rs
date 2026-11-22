//! Stateful hardware simulator ("Demo mode").
//!
//! Each virtual device speaks the *real* wire protocol: host frames are parsed with the same
//! splitters the drivers use, and replies/pushes are produced with the vendor frame builders.
//! So demo mode exercises the full stack (driver encode → transport → device → driver decode),
//! including the honest-UI ACK flow, pushes, the OnePlus time request and link loss when the
//! case lid closes.

use super::EarbudTransport;
use crate::protocols::{nothing, oneplus, ModelInfo, Side, TransportKind};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SimModel {
    OnePlusBudsPro4,
    OnePlusBudsPro3,
    NothingEar2024,
    CmfBudsPro2,
}

impl SimModel {
    pub const ALL: [SimModel; 4] = [SimModel::OnePlusBudsPro4, SimModel::OnePlusBudsPro3, SimModel::NothingEar2024, SimModel::CmfBudsPro2];

    pub fn key(self) -> &'static str {
        match self {
            SimModel::OnePlusBudsPro4 => "oneplus-buds-pro-4",
            SimModel::OnePlusBudsPro3 => "oneplus-buds-pro-3",
            SimModel::NothingEar2024 => "nothing-ear-2024",
            SimModel::CmfBudsPro2 => "cmf-buds-pro-2",
        }
    }

    pub fn from_key(k: &str) -> Option<SimModel> {
        SimModel::ALL.into_iter().find(|m| m.key() == k)
    }

    /// Bluetooth name the virtual device advertises.
    pub fn bt_name(self) -> &'static str {
        match self {
            SimModel::OnePlusBudsPro4 => "OnePlus Buds Pro 4",
            SimModel::OnePlusBudsPro3 => "OnePlus Buds Pro 3",
            SimModel::NothingEar2024 => "Nothing Ear",
            SimModel::CmfBudsPro2 => "CMF Buds Pro 2",
        }
    }

    pub fn display(self) -> &'static str {
        match self {
            SimModel::OnePlusBudsPro4 => "OnePlus Buds Pro 4",
            SimModel::OnePlusBudsPro3 => "OnePlus Buds Pro 3",
            SimModel::NothingEar2024 => "Nothing Ear (2024)",
            SimModel::CmfBudsPro2 => "CMF Buds Pro 2",
        }
    }

    pub fn model_info(self) -> ModelInfo {
        crate::protocols::identify_by_name(self.bt_name()).expect("simulated models are in the registry")
    }

    pub fn address(self) -> &'static str {
        match self {
            SimModel::OnePlusBudsPro4 => "5A:1B:2C:3D:4E:04",
            SimModel::OnePlusBudsPro3 => "5A:1B:2C:3D:4E:03",
            SimModel::NothingEar2024 => "2C:BE:EB:00:B1:71",
            SimModel::CmfBudsPro2 => "2C:BE:EB:00:B1:72",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SimAction {
    ToggleWearLeft,
    ToggleWearRight,
    CloseCase,
    ToggleCharging,
}

fn control() -> &'static Mutex<Option<mpsc::UnboundedSender<SimAction>>> {
    static C: OnceLock<Mutex<Option<mpsc::UnboundedSender<SimAction>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

/// Deliver a physical-world action to the running simulated device.
pub fn sim_action(a: SimAction) -> bool {
    control().lock().ok().and_then(|g| g.as_ref().map(|tx| tx.send(a).is_ok())).unwrap_or(false)
}

fn store() -> &'static Mutex<HashMap<SimModel, Device>> {
    static S: OnceLock<Mutex<HashMap<SimModel, Device>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

// ---------------------------------------------------------------------------------------------
// Shared physical state
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Physical {
    left: u8,
    right: u8,
    case: u8,
    charging: bool,
    left_in_ear: bool,
    right_in_ear: bool,
}

impl Default for Physical {
    fn default() -> Self {
        Physical { left: 87, right: 82, case: 64, charging: false, left_in_ear: true, right_in_ear: true }
    }
}

/// Output of the device model: (delay, bytes) or a link drop.
enum Out {
    Bytes(Duration, Vec<u8>),
    Drop(Duration),
}

fn ms(v: u64) -> Duration {
    Duration::from_millis(v)
}

enum Device {
    Nothing(NothingSim),
    OnePlus(OnePlusSim),
}

impl Device {
    fn new(m: SimModel) -> Self {
        match m {
            SimModel::NothingEar2024 => Device::Nothing(NothingSim::new(0xB171)),
            SimModel::CmfBudsPro2 => Device::Nothing(NothingSim::new(0xB172)),
            SimModel::OnePlusBudsPro4 => Device::OnePlus(OnePlusSim::new([0x14, 0x78, 0x06], true)),
            SimModel::OnePlusBudsPro3 => Device::OnePlus(OnePlusSim::new([0x14, 0x40, 0x06], false)),
        }
    }
    fn on_connect(&mut self) -> Vec<Out> {
        match self {
            Device::Nothing(_) => vec![],
            Device::OnePlus(d) => d.on_connect(),
        }
    }
    fn on_host(&mut self, b: &[u8]) -> Vec<Out> {
        match self {
            Device::Nothing(d) => d.on_host(b),
            Device::OnePlus(d) => d.on_host(b),
        }
    }
    fn on_action(&mut self, a: SimAction) -> Vec<Out> {
        match self {
            Device::Nothing(d) => d.on_action(a),
            Device::OnePlus(d) => d.on_action(a),
        }
    }
    fn periodic(&mut self) -> Vec<Out> {
        match self {
            Device::Nothing(d) => d.periodic(),
            Device::OnePlus(d) => d.periodic(),
        }
    }
}

fn drain(p: &mut Physical) {
    if p.charging {
        p.left = (p.left + 1).min(100);
        p.right = (p.right + 1).min(100);
    } else {
        if p.left_in_ear && p.left > 5 {
            p.left -= 1;
        }
        if p.right_in_ear && p.right > 5 {
            p.right -= 1;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Nothing / CMF virtual device
// ---------------------------------------------------------------------------------------------

struct NothingSim {
    code: u16,
    split: nothing::Splitter,
    phys: Physical,
    anc: u8,
    last_nc: u8,
    eq: u8,
    custom: Vec<u8>,
    bass: (u8, u8),
    latency: u8,
    in_ear: u8,
    gestures: Vec<[u8; 4]>,
    codec: u8,
    dual: u8,
    spatial: u8,
}

impl NothingSim {
    fn new(code: u16) -> Self {
        let treble = if code == 0xB171 { 3400.0 } else { 6900.0 };
        NothingSim {
            code,
            split: nothing::Splitter::default(),
            phys: Physical::default(),
            anc: 0x01,
            last_nc: 0x01,
            eq: 0,
            custom: nothing::custom_eq_payload([0.0, 0.0, 0.0], treble),
            bass: (0, 4),
            latency: 2,
            in_ear: 1,
            gestures: vec![[2, 1, 2, 9], [3, 1, 2, 9], [2, 1, 3, 8], [3, 1, 3, 8], [2, 1, 7, 0x16], [3, 1, 7, 0x16], [2, 1, 9, 1], [3, 1, 9, 1]],
            codec: 0,
            dual: 1,
            spatial: 0,
        }
    }

    fn reply(&self, cmd: u16, fsn: u8, payload: &[u8]) -> Out {
        Out::Bytes(ms(35), nothing::build_frame(0x0160, cmd & 0x7FFF, fsn, payload))
    }

    fn ack(&self, cmd: u16, fsn: u8) -> Out {
        Out::Bytes(ms(25), nothing::build_frame(0x0160, (cmd & 0x0FFF) | 0x7000, fsn, &[]))
    }

    fn push(&self, cmd: u16, payload: &[u8], delay: u64) -> Out {
        Out::Bytes(ms(delay), nothing::build_frame(0x0100, cmd, 0, payload))
    }

    fn battery(&self) -> Vec<u8> {
        let ch = if self.phys.charging { 0x80 } else { 0 };
        vec![3, 2, self.phys.left | ch, 3, self.phys.right | ch, 4, self.phys.case]
    }

    fn wear(&self) -> Vec<u8> {
        let v = |e: bool| if e { 0x84 } else { 0x80 };
        vec![2, 2, v(self.phys.left_in_ear), 3, v(self.phys.right_in_ear)]
    }

    fn on_host(&mut self, data: &[u8]) -> Vec<Out> {
        let mut out = Vec::new();
        for f in self.split.push(data) {
            let p = &f.payload;
            let c = f.cmd;
            let r = |s: &Self, pl: &[u8]| s.reply(c, f.fsn, pl);
            match c {
                nothing::GET_PROTOCOL => out.push(r(self, b"5.2")),
                nothing::GET_MODEL => out.push(r(self, &self.code.to_le_bytes())),
                nothing::GET_FIRMWARE => out.push(r(self, if self.code == 0xB171 { b"1.0.1.202" } else { b"1.0.1.74" })),
                nothing::GET_SERIAL => {
                    let mut pl = vec![0x09];
                    pl.extend_from_slice(b"2,1,1\n2,2,1.0.1.202\n2,4,SH10237107001234\n3,4,SH10237107001235\n");
                    out.push(r(self, &pl));
                }
                nothing::GET_BATTERY => out.push(r(self, &self.battery())),
                nothing::GET_WEAR => out.push(r(self, &self.wear())),
                nothing::GET_ANC => out.push(r(self, &[1, self.anc, 0, 2, self.last_nc, 0])),
                nothing::GET_EQ if self.code == 0xB171 => out.push(r(self, &[self.eq])),
                nothing::GET_LISTENING if self.code != 0xB171 => out.push(r(self, &[self.eq])),
                nothing::GET_CUSTOM_EQ => out.push(r(self, &self.custom.clone())),
                nothing::GET_BASS => out.push(r(self, &[self.bass.0, self.bass.1])),
                nothing::GET_LATENCY => out.push(r(self, &[self.latency, 0])),
                nothing::GET_IN_EAR => out.push(r(self, &[1, 1, self.in_ear])),
                nothing::GET_GESTURES => {
                    let mut pl = vec![self.gestures.len() as u8];
                    for g in &self.gestures {
                        pl.extend_from_slice(g);
                    }
                    out.push(r(self, &pl));
                }
                nothing::GET_CODEC => out.push(r(self, &[self.codec])),
                nothing::GET_DUAL => out.push(r(self, &[self.dual])),
                nothing::GET_SPATIAL => out.push(r(self, &[self.spatial, 0])),
                nothing::GET_ADV_EQ | nothing::GET_PERSONAL_ANC => out.push(r(self, &[0])),
                nothing::SET_ANC if p.len() >= 2 => {
                    if nothing::anc_from_byte(p[1]).is_some() {
                        self.anc = p[1];
                        if (1..=4).contains(&p[1]) {
                            self.last_nc = p[1];
                        }
                        out.push(self.ack(c, f.fsn));
                    } else {
                        out.push(Out::Bytes(ms(20), nothing::build_frame(0x0161, 0x700F, f.fsn, &[])));
                    }
                }
                nothing::SET_EQ | nothing::SET_LISTENING if !p.is_empty() => {
                    self.eq = p[0];
                    out.push(self.ack(c, f.fsn));
                }
                nothing::SET_CUSTOM_EQ if p.len() >= 44 => {
                    self.custom = p.clone();
                    out.push(self.ack(c, f.fsn));
                }
                nothing::SET_BASS if p.len() >= 2 => {
                    self.bass = (p[0], p[1]);
                    out.push(self.ack(c, f.fsn));
                }
                nothing::SET_LATENCY if !p.is_empty() => {
                    self.latency = p[0];
                    out.push(Out::Bytes(ms(25), nothing::build_frame(0x0160, 0x7040, f.fsn, &[p[0], 0])));
                }
                nothing::SET_IN_EAR if p.len() >= 3 => {
                    self.in_ear = p[2];
                    out.push(self.ack(c, f.fsn));
                }
                nothing::SET_GESTURE if p.len() >= 5 => {
                    let rec = [p[1], p[2], p[3], p[4]];
                    match self.gestures.iter_mut().find(|g| g[0] == rec[0] && g[2] == rec[2]) {
                        Some(g) => *g = rec,
                        None => self.gestures.push(rec),
                    }
                    out.push(self.ack(c, f.fsn));
                }
                nothing::SET_CODEC if !p.is_empty() => {
                    self.codec = p[0];
                    out.push(self.ack(c, f.fsn));
                    // codec switch reboots the buds
                    out.push(Out::Drop(ms(600)));
                }
                nothing::SET_DUAL if !p.is_empty() => {
                    self.dual = p[0];
                    out.push(self.ack(c, f.fsn));
                }
                nothing::SET_SPATIAL if !p.is_empty() => {
                    self.spatial = p[0];
                    out.push(self.ack(c, f.fsn));
                }
                nothing::SET_FIT_TEST => {
                    out.push(self.ack(c, f.fsn));
                    let right = if self.phys.right_in_ear { 0 } else { 2 };
                    let left = if self.phys.left_in_ear { if self.phys.left.is_multiple_of(2) { 0 } else { 1 } } else { 2 };
                    out.push(self.push(nothing::EVT_FIT, &[left, right], 3200));
                }
                nothing::SET_RING | nothing::SET_TIME | nothing::SET_ACTIVATE | nothing::SET_ADV_EQ | nothing::SET_PERSONAL_ANC => {
                    out.push(self.ack(c, f.fsn))
                }
                _ => {
                    // unknown GET → empty reply (as CMF firmware does)
                    if c >> 12 == 0xC {
                        out.push(r(self, &[]));
                    }
                }
            }
        }
        out
    }

    fn on_action(&mut self, a: SimAction) -> Vec<Out> {
        match a {
            SimAction::ToggleWearLeft => self.phys.left_in_ear = !self.phys.left_in_ear,
            SimAction::ToggleWearRight => self.phys.right_in_ear = !self.phys.right_in_ear,
            SimAction::ToggleCharging => self.phys.charging = !self.phys.charging,
            SimAction::CloseCase => {
                self.phys.left_in_ear = false;
                self.phys.right_in_ear = false;
                return vec![self.push(nothing::EVT_WEAR, &[1, 4, 1], 50), self.push(nothing::EVT_BATTERY, &[2, 2, self.phys.left, 3, self.phys.right], 300), Out::Drop(ms(1500))];
            }
        }
        vec![self.push(nothing::EVT_WEAR, &[2, 2, 0, 3, 0], 60), self.push(nothing::EVT_BATTERY, &self.battery(), 120)]
    }

    fn periodic(&mut self) -> Vec<Out> {
        drain(&mut self.phys);
        vec![self.push(nothing::EVT_BATTERY, &self.battery(), 0)]
    }
}

// ---------------------------------------------------------------------------------------------
// OnePlus virtual device
// ---------------------------------------------------------------------------------------------

struct OnePlusSim {
    pid: [u8; 3],
    pro4: bool,
    split: oneplus::Splitter,
    phys: Physical,
    /// current report mask
    anc_report: u16,
    last_level_bit: u16,
    eq: u8,
    custom: Vec<oneplus::CustomEq>,
    basswave: i8,
    switches: BTreeMap<u8, u8>,
    spatial: u8,
    gestures: Vec<[u8; 4]>,
    time_answered: bool,
}

impl OnePlusSim {
    fn new(pid: [u8; 3], pro4: bool) -> Self {
        let mut switches = BTreeMap::new();
        for (k, v) in [(0x05, 0), (0x04, 1), (0x0B, 0), (0x11, 1), (0x13, 0), (0x18, 0), (0x06, 0), (0x1D, 0)] {
            switches.insert(k, v);
        }
        if pro4 {
            switches.insert(0x1B, 0);
            switches.insert(0x28, 0);
        }
        OnePlusSim {
            pid,
            pro4,
            split: oneplus::Splitter::default(),
            phys: Physical::default(),
            anc_report: 0x0010,
            last_level_bit: 0x0010,
            eq: 0,
            custom: vec![],
            basswave: 0,
            switches,
            spatial: 0,
            gestures: vec![[1, 1, 2, 6], [2, 1, 2, 6], [1, 1, 3, 5], [2, 1, 3, 5], [1, 1, 4, 8], [2, 1, 4, 8]],
            time_answered: false,
        }
    }

    fn reply(cmd: u16, seq: u8, payload: &[u8]) -> Out {
        Out::Bytes(ms(30), oneplus::build(cmd | 0x8000, seq, payload))
    }

    fn push(payload: &[u8], delay: u64) -> Out {
        Out::Bytes(ms(delay), oneplus::build(oneplus::CMD_PUSH, oneplus::PUSH_SEQ, payload))
    }

    fn battery_entries(&self) -> Vec<u8> {
        let ch = if self.phys.charging { 0x80 } else { 0 };
        vec![3, 1, self.phys.left | ch, 2, self.phys.right | ch, 3, self.phys.case]
    }

    fn wear_entries(&self) -> Vec<u8> {
        let v = |e: bool| if e { 0x03 } else { 0x01 };
        vec![3, 1, v(self.phys.left_in_ear), 2, v(self.phys.right_in_ear), 3, 0x04]
    }

    fn on_connect(&mut self) -> Vec<Out> {
        self.time_answered = false;
        // the buds ask the host for the time shortly after the link opens
        vec![Out::Bytes(ms(450), oneplus::build(oneplus::CMD_TIME_REQ, 0x41, &[]))]
    }

    fn anc_push(&self, delay: u64) -> Out {
        let mut pl = vec![0x03, 0x01, 0x01];
        pl.extend_from_slice(&self.anc_report.to_le_bytes());
        Self::push(&pl, delay)
    }

    fn on_host(&mut self, data: &[u8]) -> Vec<Out> {
        let mut out = Vec::new();
        for f in self.split.push(data) {
            let p = &f.payload;
            let (c, s) = (f.cmd, f.seq);
            match c {
                oneplus::CMD_HANDSHAKE => out.push(Self::reply(c, s, &crate::protocols::parse_hex("00 FF 77 5A EA 67 0E 20 07"))),
                oneplus::CMD_PRODUCT_ID => out.push(Self::reply(c, s, &[0x00, self.pid[0], self.pid[1], self.pid[2]])),
                oneplus::CMD_EVENTS_LIST => out.push(Self::reply(c, s, &crate::protocols::parse_hex("00 09 01 02 03 04 08 0B F1 F2 F3"))),
                oneplus::CMD_SUBSCRIBE => {
                    let n = p.first().copied().unwrap_or(0);
                    let mut pl = vec![0x01, n];
                    for id in p.iter().skip(1) {
                        pl.extend_from_slice(&[*id, 0x00]);
                    }
                    out.push(Self::reply(c, s, &pl));
                }
                oneplus::CMD_SWITCHES_GET => {
                    let ids: Vec<u8> = p.iter().skip(1).copied().collect();
                    let known: Vec<(u8, u8)> = ids.iter().filter_map(|i| self.switches.get(i).map(|v| (*i, *v))).collect();
                    let mut pl = vec![0x00, known.len() as u8];
                    for (i, v) in known {
                        pl.extend_from_slice(&[i, v]);
                    }
                    out.push(Self::reply(c, s, &pl));
                }
                oneplus::CMD_ANC_GET if p.len() >= 2 => {
                    let mask = if p[0] == 0x04 { self.last_level_bit } else { self.anc_report };
                    let mut pl = vec![0x00, p[0], p[1]];
                    pl.extend_from_slice(&mask.to_le_bytes());
                    out.push(Self::reply(c, s, &pl));
                }
                oneplus::CMD_ANC_SET if p.len() >= 3 && p[0] == 0x01 => {
                    if !self.phys.left_in_ear && !self.phys.right_in_ear {
                        out.push(Self::reply(c, s, &[0x0E]));
                        continue;
                    }
                    let mut mb = [0u8; 4];
                    for (i, b) in p[2..].iter().take(4).enumerate() {
                        mb[i] = *b;
                    }
                    let set = u32::from_le_bytes(mb);
                    let report: u16 = match set {
                        0x01 => 0x0008,
                        0x02 => self.last_level_bit,
                        0x04 => 0x0100,
                        0x10 | 0x20 | 0x40 | 0x80 => set as u16,
                        0x0800 if self.pro4 => 0x0800,
                        _ => {
                            out.push(Self::reply(c, s, &[0x01]));
                            continue;
                        }
                    };
                    if (0x10..=0x80).contains(&report) || report == 0x0800 {
                        self.last_level_bit = report;
                    }
                    self.anc_report = report;
                    out.push(Self::reply(c, s, &[0x00]));
                    out.push(self.anc_push(1100));
                }
                oneplus::CMD_BATTERY => {
                    let mut pl = vec![0x00];
                    pl.extend(self.battery_entries());
                    out.push(Self::reply(c, s, &pl));
                }
                oneplus::CMD_WEAR => {
                    let mut pl = vec![0x00];
                    pl.extend(self.wear_entries());
                    out.push(Self::reply(c, s, &pl));
                }
                oneplus::CMD_EQ_GET => out.push(Self::reply(c, s, &[0x00, self.eq])),
                oneplus::CMD_EQ_SET if !p.is_empty() => {
                    self.eq = p[0];
                    out.push(Self::reply(c, s, &[0x00]));
                    out.push(Out::Bytes(ms(300), oneplus::build(oneplus::CMD_EQ_PUSH, 0xFF, &[p[0]])));
                }
                oneplus::CMD_CUSTOM_EQ_LIST => {
                    let mut pl = vec![0x00, self.custom.len() as u8];
                    for e in &self.custom {
                        pl.extend_from_slice(&[u8::from(self.eq == e.id), 0xFA, 0x06, e.id, e.name.len() as u8]);
                        pl.extend_from_slice(e.name.as_bytes());
                        pl.push(e.bands.len() as u8);
                        for (fr, g) in &e.bands {
                            pl.extend_from_slice(&fr.to_le_bytes());
                            pl.push(*g as u8);
                        }
                    }
                    out.push(Self::reply(c, s, &pl));
                }
                oneplus::CMD_CUSTOM_EQ_SET if p.len() >= 6 => {
                    let action = p[0];
                    let name_len = p[4] as usize;
                    let name = String::from_utf8_lossy(p.get(5..5 + name_len).unwrap_or_default()).into_owned();
                    let mut bands = Vec::new();
                    if let Some(&n) = p.get(5 + name_len) {
                        for i in 0..n as usize {
                            let o = 6 + name_len + i * 3;
                            if let Some(b) = p.get(o..o + 3) {
                                bands.push((u16::from_le_bytes([b[0], b[1]]), b[2] as i8));
                            }
                        }
                    }
                    match action {
                        0x01 => {
                            let id = 4 + self.custom.len() as u8;
                            self.custom.push(oneplus::CustomEq { selected: false, id, name, bands });
                            out.push(Self::reply(c, s, &[0x00, id]));
                        }
                        0x02 => {
                            let id = p[3];
                            if let Some(e) = self.custom.iter_mut().find(|e| e.id == id) {
                                e.bands = bands;
                                self.eq = id;
                                out.push(Self::reply(c, s, &[0x00]));
                            } else {
                                out.push(Self::reply(c, s, &[0x01]));
                            }
                        }
                        0x03 => {
                            self.custom.retain(|e| e.id != p[3]);
                            self.eq = 0;
                            out.push(Self::reply(c, s, &[0x00]));
                        }
                        _ => out.push(Self::reply(c, s, &[0x01])),
                    }
                }
                oneplus::CMD_BASSWAVE_GET => out.push(Self::reply(c, s, &[0x00, 0xFB, 0x05, self.basswave as u8])),
                oneplus::CMD_BASSWAVE_SET if p.len() >= 3 => {
                    self.basswave = p[2] as i8;
                    out.push(Self::reply(c, s, &[0x00]));
                }
                oneplus::CMD_SWITCH_SET if p.len() >= 2 => {
                    if let std::collections::btree_map::Entry::Occupied(mut e) = self.switches.entry(p[0]) {
                        e.insert(p[1]);
                        out.push(Self::reply(c, s, &[0x00]));
                        out.push(Out::Bytes(ms(500), oneplus::build(oneplus::CMD_SWITCH_PUSH, 0xFF, &[1, p[0], p[1]])));
                        if p[0] == oneplus::F_GAME {
                            out.push(Self::push(&[0x05, p[1]], 900));
                        }
                    } else {
                        out.push(Self::reply(c, s, &[0x01]));
                    }
                }
                oneplus::CMD_MULTIPOINT_OP | oneplus::CMD_FIND => out.push(Self::reply(c, s, &[0x00])),
                oneplus::CMD_SPATIAL_GET => out.push(Self::reply(c, s, &[0x00, self.spatial])),
                oneplus::CMD_SPATIAL_SET if !p.is_empty() => {
                    self.spatial = p[0];
                    out.push(Self::reply(c, s, &[0x00]));
                    out.push(Out::Bytes(ms(400), oneplus::build(oneplus::CMD_SPATIAL_PUSH, 0xFF, &[p[0]])));
                }
                oneplus::CMD_FIRMWARE => {
                    let mut pl = vec![0x00, 0x05];
                    pl.extend_from_slice(if self.pro4 { b"1,2,16.1.0.12,2,2,16.1.0.12,3,2,16.0.0.8" } else { b"1,2,11.1.0.512,2,2,11.1.0.512,3,2,11.0.0.300" });
                    out.push(Self::reply(c, s, &pl));
                }
                oneplus::CMD_CODEC_GET => out.push(Self::reply(c, s, &[0x00, if self.pro4 { 0x08 } else { 0x07 }])),
                oneplus::CMD_GESTURES => {
                    let mut pl = vec![0x00, self.gestures.len() as u8];
                    for g in &self.gestures {
                        pl.extend_from_slice(g);
                    }
                    out.push(Self::reply(c, s, &pl));
                }
                oneplus::CMD_GESTURE_SET if p.len() >= 5 => {
                    let rec = [p[1], p[2], p[3], p[4]];
                    match self.gestures.iter_mut().find(|g| g[0] == rec[0] && g[2] == rec[2]) {
                        Some(g) => *g = rec,
                        None => self.gestures.push(rec),
                    }
                    out.push(Self::reply(c, s, &[0x00]));
                }
                oneplus::CMD_FIT_TEST => {
                    out.push(Self::reply(c, s, &[0x00]));
                    let q = |e: bool, lvl: u8| if !e { 6 } else if lvl.is_multiple_of(3) { 0 } else { 1 };
                    out.push(Self::push(&[0x04, 1, q(self.phys.left_in_ear, self.phys.left), 2, q(self.phys.right_in_ear, 1)], 4000));
                }
                0x8500 => self.time_answered = true,
                0x8204 => {}
                c if c & 0x8000 == 0 && c >> 8 == 0x01 => out.push(Self::reply(c, s, &[0x01])),
                _ => {}
            }
        }
        out
    }

    fn on_action(&mut self, a: SimAction) -> Vec<Out> {
        match a {
            SimAction::ToggleWearLeft => self.phys.left_in_ear = !self.phys.left_in_ear,
            SimAction::ToggleWearRight => self.phys.right_in_ear = !self.phys.right_in_ear,
            SimAction::ToggleCharging => self.phys.charging = !self.phys.charging,
            SimAction::CloseCase => {
                self.phys.left_in_ear = false;
                self.phys.right_in_ear = false;
                return vec![
                    Self::push(&[0x02, 3, 1, 0x00, 2, 0x00, 3, 0x00], 50),
                    Self::push(&[0x01, 3, 1, 0, 2, 0, 3, 0], 400),
                    Out::Drop(ms(1800)),
                ];
            }
        }
        let mut w = vec![0x02];
        w.extend(self.wear_entries());
        let mut b = vec![0x01];
        b.extend(self.battery_entries());
        vec![Self::push(&w, 60), Self::push(&b, 150)]
    }

    fn periodic(&mut self) -> Vec<Out> {
        drain(&mut self.phys);
        let mut b = vec![0x01];
        b.extend(self.battery_entries());
        let mut v = vec![Self::push(&b, 0)];
        if !self.time_answered {
            log::warn!("simulator: host never answered the 0x0500 time request");
        }
        v.truncate(1);
        v
    }
}

// ---------------------------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------------------------

pub struct SimulatorTransport {
    model: SimModel,
    to_dev: Option<mpsc::UnboundedSender<Vec<u8>>>,
    rx: Option<mpsc::Receiver<Vec<u8>>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl SimulatorTransport {
    pub fn new(model: SimModel) -> Self {
        SimulatorTransport { model, to_dev: None, rx: None, task: None }
    }
}

async fn device_task(model: SimModel, mut from_host: mpsc::UnboundedReceiver<Vec<u8>>, mut actions: mpsc::UnboundedReceiver<SimAction>, to_host: mpsc::Sender<Vec<u8>>) {
    let mut dev = store().lock().ok().and_then(|mut s| s.remove(&model)).unwrap_or_else(|| Device::new(model));
    let mut queue: BTreeMap<(Instant, u64), Option<Vec<u8>>> = BTreeMap::new();
    let mut n: u64 = 0;
    let mut schedule = |queue: &mut BTreeMap<(Instant, u64), Option<Vec<u8>>>, outs: Vec<Out>| {
        for o in outs {
            n += 1;
            match o {
                Out::Bytes(d, b) => queue.insert((Instant::now() + d, n), Some(b)),
                Out::Drop(d) => queue.insert((Instant::now() + d, n), None),
            };
        }
    };
    schedule(&mut queue, dev.on_connect());
    let mut periodic = tokio::time::interval(Duration::from_secs(30));
    periodic.tick().await;
    loop {
        let next = queue.keys().next().map(|k| k.0);
        let sleep = async {
            match next {
                Some(t) => tokio::time::sleep_until(tokio::time::Instant::from_std(t)).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            m = from_host.recv() => match m {
                Some(b) => {
                    let outs = dev.on_host(&b);
                    schedule(&mut queue, outs);
                }
                None => break,
            },
            Some(a) = actions.recv() => {
                let outs = dev.on_action(a);
                schedule(&mut queue, outs);
            }
            _ = periodic.tick() => {
                let outs = dev.periodic();
                schedule(&mut queue, outs);
            }
            _ = sleep => {
                if let Some((k, v)) = queue.pop_first() {
                    let _ = k;
                    match v {
                        Some(bytes) => {
                            if to_host.send(bytes).await.is_err() {
                                break;
                            }
                        }
                        None => break, // link drop (case closed / reboot)
                    }
                }
            }
        }
    }
    if let Ok(mut s) = store().lock() {
        // a closed case means the buds are back in the case next time; reopening puts them back in the ears
        match &mut dev {
            Device::Nothing(d) => {
                d.phys.left_in_ear = true;
                d.phys.right_in_ear = true;
                d.split = nothing::Splitter::default();
            }
            Device::OnePlus(d) => {
                d.phys.left_in_ear = true;
                d.phys.right_in_ear = true;
                d.split = oneplus::Splitter::default();
            }
        }
        s.insert(model, dev);
    }
}

#[async_trait]
impl EarbudTransport for SimulatorTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Simulator
    }

    async fn connect(&mut self) -> Result<(), String> {
        let (to_dev, from_host) = mpsc::unbounded_channel();
        let (to_host, rx) = mpsc::channel(256);
        let (atx, arx) = mpsc::unbounded_channel();
        if let Ok(mut c) = control().lock() {
            *c = Some(atx);
        }
        // emulate RFCOMM connect latency
        tokio::time::sleep(Duration::from_millis(350)).await;
        self.task = Some(tokio::spawn(device_task(self.model, from_host, arx, to_host)));
        self.to_dev = Some(to_dev);
        self.rx = Some(rx);
        Ok(())
    }

    async fn disconnect(&mut self) {
        self.to_dev = None;
        if let Some(t) = self.task.take() {
            let _ = tokio::time::timeout(Duration::from_secs(1), t).await;
        }
    }

    async fn send_frame(&mut self, data: &[u8]) -> Result<(), String> {
        self.to_dev.as_ref().ok_or("симулятор не подключён")?.send(data.to_vec()).map_err(|_| "симулятор отключился".to_string())
    }

    fn subscribe_rx(&mut self) -> mpsc::Receiver<Vec<u8>> {
        self.rx.take().unwrap_or_else(|| mpsc::channel(1).1)
    }
}

pub fn side_action(side: Side) -> SimAction {
    match side {
        Side::Left => SimAction::ToggleWearLeft,
        Side::Right => SimAction::ToggleWearRight,
    }
}

//! `Misc/Part.cpp` + `Containers/NotePool.cpp`: one instrument (16 kit
//! items × ADD/SUB/PAD engines), its note pool, poly/mono/legato modes,
//! portamento and the three instrument effects.

use std::sync::Arc;

use super::ad_params::AdParams;
use super::adnote::{AdNote, NoteInit};
use super::controller::{Controller, ControllerCfg};
use super::effects::{EffectCfg, EffectMgr};
use super::lfo::AbsTime;
use super::microtonal::Microtonal;
use super::pad::{PadNote, PadParams};
use super::sub::{SubNote, SubParams};
use super::synthnote::{LegatoParams, PortaRef, PortaSlot, Portamento};
use super::util::*;
use super::xml::{Version, XmlNode};
use super::{NoteCtx, NoteScratch};
use super::fft::Fft;
use super::oscilgen::OscScratch;

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct KitItem {
    pub enabled: bool,
    pub muted: bool,
    pub minkey: u8,
    pub maxkey: u8,
    pub name: String,
    pub sendto: u8,
    pub ad_enabled: bool,
    pub sub_enabled: bool,
    pub pad_enabled: bool,
    pub ad: Option<Arc<AdParams>>,
    pub sub: Option<Arc<SubParams>>,
    pub pad: Option<Arc<PadParams>>,
}

impl KitItem {
    fn new(enabled: bool, oscilsize: usize) -> Self {
        let mut k = Self {
            enabled: false,
            muted: false,
            minkey: 0,
            maxkey: 127,
            name: String::new(),
            sendto: 0,
            ad_enabled: false,
            sub_enabled: false,
            pad_enabled: false,
            ad: None,
            sub: None,
            pad: None,
        };
        k.set_enabled(enabled, oscilsize);
        k
    }

    /// `setkititemstatus`.
    pub fn set_enabled(&mut self, on: bool, oscilsize: usize) {
        self.enabled = on;
        if on {
            if self.ad.is_none() {
                self.ad = Some(Arc::new(AdParams::new(oscilsize)));
            }
            if self.sub.is_none() {
                self.sub = Some(Arc::new(SubParams::new()));
            }
            if self.pad.is_none() {
                self.pad = Some(Arc::new(PadParams::new(oscilsize)));
            }
        } else {
            self.ad = None;
            self.sub = None;
            self.pad = None;
            self.name.clear();
        }
    }

    pub fn active(&self) -> bool {
        self.ad_enabled || self.sub_enabled || self.pad_enabled
    }

    pub fn valid_note(&self, note: u8) -> bool {
        !self.muted && self.enabled && note >= self.minkey && note <= self.maxkey
    }

    fn sendto(&self) -> u8 {
        self.sendto.min(NUM_PART_EFX as u8)
    }
}

#[derive(Clone, Debug)]
pub struct PartParams {
    pub enabled: bool,
    pub volume: f32,
    pub panning: u8,
    pub minkey: u8,
    pub maxkey: u8,
    pub keyshift: u8,
    pub rcvchn: u8,
    pub velsns: u8,
    pub veloffs: u8,
    pub noteon: bool,
    pub polymode: bool,
    pub legatomode: bool,
    pub keylimit: u8,
    pub voicelimit: u8,
    pub ctl: ControllerCfg,
    pub name: String,
    pub author: String,
    pub comments: String,
    pub info_type: i32,
    pub kitmode: u8,
    pub drummode: bool,
    pub kit: Vec<KitItem>,
    pub efx: Vec<EffectCfg>,
    pub efxroute: [u8; NUM_PART_EFX],
    pub efxbypass: [bool; NUM_PART_EFX],
    pub const_power: bool,
}

pub fn volume127_to_db(v: u8) -> f32 {
    (v as f32 - 96.0) / 96.0 * 40.0
}

fn clamp_volume_db(v: f32) -> f32 {
    let v = if (v - 50.0).abs() < 0.001 { 0.0 } else { v };
    v.clamp(-40.0, 13.333)
}

impl PartParams {
    pub fn new(oscilsize: usize) -> Self {
        let mut kit: Vec<KitItem> = (0..NUM_KIT_ITEMS).map(|_| KitItem::new(false, oscilsize)).collect();
        kit[0].set_enabled(true, oscilsize);
        kit[0].ad_enabled = true;
        Self {
            enabled: false,
            volume: 0.0,
            panning: 64,
            minkey: 0,
            maxkey: 127,
            keyshift: 64,
            rcvchn: 0,
            velsns: 64,
            veloffs: 64,
            noteon: true,
            polymode: true,
            legatomode: false,
            keylimit: 15,
            voicelimit: 0,
            ctl: ControllerCfg::default(),
            name: String::new(),
            author: String::new(),
            comments: String::new(),
            info_type: 0,
            kitmode: 0,
            drummode: false,
            kit,
            efx: (0..NUM_PART_EFX).map(|_| EffectCfg::default()).collect(),
            efxroute: [0; NUM_PART_EFX],
            efxbypass: [false; NUM_PART_EFX],
            const_power: true,
        }
    }

    /// Reset just the instrument (`defaultsinstrument`).
    pub fn defaults_instrument(&mut self, oscilsize: usize) {
        let fresh = Self::new(oscilsize);
        self.name.clear();
        self.author.clear();
        self.comments.clear();
        self.info_type = 0;
        self.kitmode = 0;
        self.drummode = false;
        self.kit = fresh.kit;
        self.efx = fresh.efx;
        self.efxroute = [0; NUM_PART_EFX];
        self.efxbypass = [false; NUM_PART_EFX];
    }

    pub fn is_mono(&self) -> bool {
        !self.polymode && !self.legatomode
    }
    pub fn is_legato(&self) -> bool {
        self.legatomode && !self.drummode
    }

    pub fn add_instrument_xml(&self, x: &mut XmlNode) {
        let info = x.child("INFO");
        info.add_parstr("name", &self.name);
        info.add_parstr("author", &self.author);
        info.add_parstr("comments", &self.comments);
        info.add_par("type", self.info_type);
        let k = x.child("INSTRUMENT_KIT");
        k.add_par("kit_mode", self.kitmode as i32);
        k.add_parbool("drum_mode", self.drummode);
        for (i, it) in self.kit.iter().enumerate() {
            let n = k.child_id("INSTRUMENT_KIT_ITEM", i);
            n.add_parbool("enabled", it.enabled);
            if !it.enabled {
                continue;
            }
            n.add_parstr("name", &it.name);
            n.add_parbool("muted", it.muted);
            n.add_par("min_key", it.minkey as i32);
            n.add_par("max_key", it.maxkey as i32);
            n.add_par("send_to_instrument_effect", it.sendto as i32);
            n.add_parbool("add_enabled", it.ad_enabled);
            if let (true, Some(a)) = (it.ad_enabled, it.ad.as_ref()) {
                a.add_to_xml(n.child("ADD_SYNTH_PARAMETERS"));
            }
            n.add_parbool("sub_enabled", it.sub_enabled);
            if let (true, Some(s)) = (it.sub_enabled, it.sub.as_ref()) {
                s.add_to_xml(n.child("SUB_SYNTH_PARAMETERS"));
            }
            n.add_parbool("pad_enabled", it.pad_enabled);
            if let (true, Some(p)) = (it.pad_enabled, it.pad.as_ref()) {
                p.add_to_xml(n.child("PAD_SYNTH_PARAMETERS"));
            }
        }
        let e = x.child("INSTRUMENT_EFFECTS");
        for n in 0..NUM_PART_EFX {
            let f = e.child_id("INSTRUMENT_EFFECT", n);
            self.efx[n].add_to_xml(f.child("EFFECT"), true);
            f.add_par("route", self.efxroute[n] as i32);
            f.add_parbool("bypass", self.efxbypass[n]);
        }
    }

    pub fn get_instrument_xml(&mut self, x: &XmlNode, ver: Version, oscilsize: usize) {
        self.const_power = ver >= Version(3, 0, 7);
        if let Some(info) = x.branch("INFO") {
            self.name = info.parstr("name").unwrap_or_default();
            self.author = info.parstr("author").unwrap_or_default();
            self.comments = info.parstr("comments").unwrap_or_default();
            self.info_type = info.par("type", self.info_type, 0, 16);
        }
        if let Some(k) = x.branch("INSTRUMENT_KIT") {
            self.kitmode = k.par127("kit_mode", self.kitmode);
            self.drummode = k.parbool("drum_mode", self.drummode);
            for i in 0..NUM_KIT_ITEMS {
                let Some(n) = k.branch_id("INSTRUMENT_KIT_ITEM", i) else { continue };
                let it = &mut self.kit[i];
                if i != 0 {
                    let en = n.parbool("enabled", it.enabled);
                    if en != it.enabled {
                        it.set_enabled(en, oscilsize);
                    }
                }
                it.name = n.parstr("name").unwrap_or_default();
                it.muted = n.parbool("muted", it.muted);
                it.minkey = n.par127("min_key", it.minkey);
                it.maxkey = n.par127("max_key", it.maxkey);
                it.sendto = n.par127("send_to_instrument_effect", it.sendto);
                it.ad_enabled = n.parbool("add_enabled", it.ad_enabled);
                if let Some(a) = n.branch("ADD_SYNTH_PARAMETERS") {
                    let mut p = AdParams::new(oscilsize);
                    p.get_from_xml(a, ver, oscilsize);
                    it.ad = Some(Arc::new(p));
                }
                it.sub_enabled = n.parbool("sub_enabled", it.sub_enabled);
                if let Some(s) = n.branch("SUB_SYNTH_PARAMETERS") {
                    let mut p = SubParams::new();
                    p.get_from_xml(s, ver);
                    it.sub = Some(Arc::new(p));
                }
                it.pad_enabled = n.parbool("pad_enabled", it.pad_enabled);
                if let Some(s) = n.branch("PAD_SYNTH_PARAMETERS") {
                    let mut p = PadParams::new(oscilsize);
                    p.get_from_xml(s, ver, oscilsize);
                    it.pad = Some(Arc::new(p));
                }
            }
        }
        if let Some(e) = x.branch("INSTRUMENT_EFFECTS") {
            for n in 0..NUM_PART_EFX {
                let Some(f) = e.branch_id("INSTRUMENT_EFFECT", n) else { continue };
                if let Some(ef) = f.branch("EFFECT") {
                    self.efx[n] = EffectCfg::default();
                    self.efx[n].get_from_xml(ef, ver, true);
                }
                self.efxroute[n] = (f.par("route", self.efxroute[n] as i32, 0, NUM_PART_EFX as i32) as u8).min(2);
                self.efxbypass[n] = f.parbool("bypass", self.efxbypass[n]);
            }
        }
    }

    pub fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_parbool("enabled", self.enabled);
        x.add_parreal("volume", self.volume);
        x.add_par("panning", self.panning as i32);
        x.add_par("min_key", self.minkey as i32);
        x.add_par("max_key", self.maxkey as i32);
        x.add_par("key_shift", self.keyshift as i32);
        x.add_par("rcv_chn", self.rcvchn as i32);
        x.add_par("velocity_sensing", self.velsns as i32);
        x.add_par("velocity_offset", self.veloffs as i32);
        x.add_parbool("note_on", self.noteon);
        x.add_parbool("poly_mode", self.polymode);
        x.add_par("legato_mode", self.legatomode as i32);
        x.add_par("key_limit", self.keylimit as i32);
        x.add_par("voice_limit", self.voicelimit as i32);
        self.add_instrument_xml(x.child("INSTRUMENT"));
        self.ctl.add_to_xml(x.child("CONTROLLER"));
    }

    pub fn get_from_xml(&mut self, x: &XmlNode, ver: Version, oscilsize: usize) {
        self.const_power = ver >= Version(3, 0, 7);
        self.enabled = x.parbool("enabled", self.enabled);
        self.volume = clamp_volume_db(if x.has_parreal("volume") {
            x.parreal("volume", self.volume)
        } else {
            volume127_to_db(x.par127("volume", 96))
        });
        self.panning = x.par127("panning", self.panning);
        self.minkey = x.par127("min_key", self.minkey);
        self.maxkey = x.par127("max_key", self.maxkey);
        self.keyshift = x.par127("key_shift", self.keyshift);
        self.rcvchn = x.par127("rcv_chn", self.rcvchn);
        self.velsns = x.par127("velocity_sensing", self.velsns);
        self.veloffs = x.par127("velocity_offset", self.veloffs);
        self.noteon = x.parbool("note_on", self.noteon);
        self.polymode = x.parbool("poly_mode", self.polymode);
        self.legatomode = x.parbool("legato_mode", self.legatomode);
        if !self.legatomode {
            self.legatomode = x.par127("legato_mode", self.legatomode as u8) != 0;
        }
        self.keylimit = x.par127("key_limit", self.keylimit).min(POLYPHONY as u8);
        self.voicelimit = x.par127("voice_limit", self.voicelimit);
        if let Some(i) = x.branch("INSTRUMENT") {
            self.get_instrument_xml(i, ver, oscilsize);
        }
        if let Some(c) = x.branch("CONTROLLER") {
            self.ctl.get_from_xml(c);
            if ver < Version(3, 0, 7) && !self.polymode {
                self.ctl.portamento_auto = true;
            }
        }
    }

    /// Rebuild oscillator spectra and PAD tables (after load).
    pub fn prepare(&mut self, fft: &mut Fft, sr: f32) {
        for it in self.kit.iter_mut() {
            if let Some(a) = it.ad.as_mut() {
                Arc::make_mut(a).prepare(fft);
            }
            if let Some(p) = it.pad.as_mut() {
                if it.pad_enabled || p.samples.is_empty() {
                    let pm = Arc::make_mut(p);
                    if it.pad_enabled {
                        pm.apply_parameters(sr);
                    } else {
                        pm.oscil.prepare(fft);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Voice {
    Ad(Box<AdNote>),
    Sub(Box<SubNote>),
    Pad(Box<PadNote>),
}

#[derive(Clone, Debug)]
struct SynthNote {
    v: Voice,
    kit: usize,
}

const KEY_PLAYING: u8 = 1;
const KEY_RELEASED_AND_SUSTAINED: u8 = 2;
const KEY_RELEASED: u8 = 3;
const KEY_ENTOMBED: u8 = 4;
const KEY_LATCHED: u8 = 5;
const SUSTAIN_BIT: u8 = 0x08;
const NOTE_MASK: u8 = 0x07;
const EXPECTED_USAGE: usize = 3;

#[derive(Clone, Debug)]
struct NoteDesc {
    age: u32,
    note: u8,
    sendto: u8,
    status: u8,
    legato_mirror: bool,
    porta: Option<usize>,
    notes: Vec<SynthNote>,
}

impl NoteDesc {
    fn st(&self) -> u8 {
        self.status & NOTE_MASK
    }
    fn playing(&self) -> bool {
        self.st() == KEY_PLAYING
    }
    fn sustained(&self) -> bool {
        self.st() == KEY_RELEASED_AND_SUSTAINED
    }
    fn released(&self) -> bool {
        self.st() == KEY_RELEASED
    }
    fn entombed(&self) -> bool {
        self.st() == KEY_ENTOMBED
    }
    fn latched(&self) -> bool {
        self.st() == KEY_LATCHED
    }
    fn dying(&self) -> bool {
        self.entombed() || self.released()
    }
    fn set_status(&mut self, s: u8) {
        self.status &= !NOTE_MASK;
        self.status |= NOTE_MASK & s;
    }
    fn can_sustain(&self) -> bool {
        self.status & SUSTAIN_BIT == 0
    }
}

/// What the part needs from the master for one call.
pub struct PartEnv<'a> {
    pub synth: &'a SynthT,
    pub time: &'a AbsTime,
    pub micro: &'a Microtonal,
    pub master_keyshift: i32,
    pub changed: bool,
    pub cubic: bool,
    pub fft: &'a mut Fft,
    pub osc: &'a mut OscScratch,
    pub scratch: &'a mut NoteScratch,
}

pub struct PartRt {
    pub ctl: Controller,
    pool: Vec<NoteDesc>,
    efx: Vec<EffectMgr>,
    monomem: [i16; 256],
    monomem_data: [(u8, f32); 256],
    lastnote: i32,
    oldfreq_log2: f32,
    oldportamentofreq_log2: f32,
    oldportamento: Option<usize>,
    legatoportamento: Option<usize>,
    lastlegatomodevalid: bool,
    killallnotes: bool,
    portas: Vec<PortaSlot>,
    porta_refs: Vec<u32>,
    porta_gen: u32,
    pub outl: Vec<f32>,
    pub outr: Vec<f32>,
    fxin_l: Vec<Vec<f32>>,
    fxin_r: Vec<Vec<f32>>,
    silent: bool,
    /// Volume in dB as changed by MIDI CC 7 (`None` = the parameter).
    cc_volume_db: Option<f32>,
    pub peak: [f32; 2],
    pub active_notes: [bool; 128],
}

impl PartRt {
    pub fn new(sr: f32, bs: usize, cfg: &ControllerCfg) -> Self {
        Self {
            ctl: Controller::new(cfg.clone()),
            pool: Vec::with_capacity(POLYPHONY),
            efx: (0..NUM_PART_EFX).map(|_| EffectMgr::new(true, sr, bs)).collect(),
            monomem: [-1; 256],
            monomem_data: [(0, 0.0); 256],
            lastnote: -1,
            oldfreq_log2: -1.0,
            oldportamentofreq_log2: 0.0,
            oldportamento: None,
            legatoportamento: None,
            lastlegatomodevalid: false,
            killallnotes: false,
            portas: vec![PortaSlot::default(); POLYPHONY],
            porta_refs: vec![0; POLYPHONY],
            porta_gen: 1,
            outl: vec![0.0; bs],
            outr: vec![0.0; bs],
            fxin_l: (0..=NUM_PART_EFX).map(|_| vec![0.0; bs]).collect(),
            fxin_r: (0..=NUM_PART_EFX).map(|_| vec![0.0; bs]).collect(),
            silent: false,
            cc_volume_db: None,
            peak: [0.0; 2],
            active_notes: [false; 128],
        }
    }

    /// Apply edited parameters (effects and controller configuration).
    pub fn apply_params(&mut self, pp: &PartParams, tempo: f32) {
        for n in 0..NUM_PART_EFX {
            self.efx[n].const_power = pp.const_power;
            self.efx[n].dryonly = pp.efxroute[n] == 2;
            self.efx[n].apply(&pp.efx[n], tempo);
        }
        self.ctl.set_cfg(&pp.ctl);
        self.cc_volume_db = None;
    }

    pub fn gain(&self, pp: &PartParams) -> f32 {
        let vdb = clamp_volume_db(self.cc_volume_db.unwrap_or(pp.volume));
        db2rap(vdb) * self.ctl.expression_relvolume
    }

    pub fn panning(&self, pp: &PartParams) -> f32 {
        (pp.panning as f32 / 127.0 + self.ctl.pan).clamp(0.0, 1.0)
    }

    pub fn note_count(&self) -> usize {
        self.pool.iter().map(|d| d.notes.len()).sum()
    }

    // ---- portamento slots ----

    fn porta_alloc(&mut self, p: Portamento) -> Option<usize> {
        let i = self.portas.iter().position(|s| !s.live)?;
        self.porta_gen = self.porta_gen.wrapping_add(1);
        self.portas[i] = PortaSlot { generation: self.porta_gen, live: true, p };
        self.porta_refs[i] = 0;
        Some(i)
    }

    fn porta_ref(&self, i: Option<usize>) -> Option<PortaRef> {
        i.map(|i| PortaRef { idx: i, generation: self.portas[i].generation })
    }

    fn porta_incref(&mut self, i: Option<usize>) {
        if let Some(i) = i {
            self.porta_refs[i] += 1;
        }
    }

    fn porta_decref(&mut self, i: Option<usize>) {
        let Some(i) = i else { return };
        if self.porta_refs[i] > 0 {
            self.porta_refs[i] -= 1;
        }
        if self.porta_refs[i] == 0 && self.portas[i].live {
            self.porta_release(i);
        }
    }

    /// The `PortamentoRealtime` cleanup lambda.
    fn porta_release(&mut self, i: usize) {
        if self.oldportamento == Some(i) {
            if self.portas[i].p.active {
                self.oldportamentofreq_log2 += self.portas[i].p.freqdelta_log2;
            }
            self.oldportamento = None;
        }
        if self.legatoportamento == Some(i) {
            self.legatoportamento = None;
        }
        self.portas[i].live = false;
    }

    // ---- note pool ----

    fn existing_running(&self) -> usize {
        let mut running = [false; 256];
        let mut n = 0;
        for d in self.pool.iter() {
            if !d.playing() && !d.sustained() && !d.latched() {
                continue;
            }
            if running[d.note as usize] {
                continue;
            }
            running[d.note as usize] = true;
            n += 1;
        }
        n
    }

    fn synth_full(&self, count: usize) -> bool {
        let used: usize = self.pool.iter().map(|d| d.notes.len()).sum();
        POLYPHONY * EXPECTED_USAGE - used < count
    }

    fn kit_usage(pp: &PartParams, note: u8) -> usize {
        let non_kit = pp.kitmode == 0;
        let single = pp.kitmode == 2;
        let mut usage = 0;
        for it in pp.kit.iter() {
            if !non_kit && !it.valid_note(note) {
                continue;
            }
            usage += it.ad_enabled as usize + it.sub_enabled as usize + it.pad_enabled as usize;
            if non_kit || (single && it.active()) {
                break;
            }
        }
        usage
    }

    fn insert_note(&mut self, note: u8, sendto: u8, sn: SynthNote, porta: Option<usize>, legato: bool) {
        let merge = self.pool.last().map(|d| d.age == 0 && d.note == note && d.sendto == sendto && d.playing() && d.legato_mirror == legato && d.can_sustain()).unwrap_or(false);
        if merge {
            let old = self.pool.last().unwrap().porta;
            if old != porta {
                self.porta_incref(porta);
                self.porta_decref(old);
            }
            let d = self.pool.last_mut().unwrap();
            d.notes.push(sn);
            d.status = KEY_PLAYING;
            d.legato_mirror = legato;
            d.porta = porta;
        } else if self.pool.len() < POLYPHONY {
            self.porta_incref(porta);
            self.pool.push(NoteDesc { age: 0, note, sendto, status: KEY_PLAYING, legato_mirror: legato, porta, notes: vec![sn] });
        }
    }

    fn kill_desc(&mut self, i: usize) {
        let d = self.pool.remove(i);
        self.porta_decref(d.porta);
    }

    pub fn effects_note_on(&mut self) {
        for e in self.efx.iter_mut() {
            e.note_on();
        }
    }

    pub fn effects_transport(&mut self, playing: bool, beat: f64, tempo: f32) {
        for e in self.efx.iter_mut() {
            e.set_transport(playing, beat, tempo);
        }
    }

    pub fn kill_all_notes(&mut self) {
        while !self.pool.is_empty() {
            self.kill_desc(self.pool.len() - 1);
        }
    }

    fn release_desc(d: &mut NoteDesc, pp: &PartParams, cx: &NoteCtx) {
        d.set_status(KEY_RELEASED);
        for sn in d.notes.iter_mut() {
            let it = &pp.kit[sn.kit];
            match &mut sn.v {
                Voice::Ad(n) => {
                    if let Some(p) = it.ad.as_deref() {
                        n.releasekey(p, cx);
                    }
                }
                Voice::Sub(n) => n.releasekey(),
                Voice::Pad(n) => {
                    if let Some(p) = it.pad.as_deref() {
                        n.releasekey(p, cx);
                    }
                }
            }
        }
    }

    fn entomb_desc(d: &mut NoteDesc) {
        d.set_status(KEY_ENTOMBED);
        for sn in d.notes.iter_mut() {
            match &mut sn.v {
                Voice::Ad(n) => n.entomb(),
                Voice::Sub(n) => n.entomb(),
                Voice::Pad(n) => n.entomb(),
            }
        }
    }

    fn running_voices(&self) -> usize {
        self.pool.iter().filter(|d| !d.entombed()).count()
    }

    fn enforce_key_limit(&mut self, limit: usize) {
        if self.existing_running() <= limit {
            return;
        }
        let mut to_kill: Option<usize> = None;
        let mut oldest = 0u32;
        for (i, nd) in self.pool.iter().enumerate() {
            match to_kill {
                None => {
                    oldest = nd.age;
                    to_kill = Some(i);
                }
                Some(k) => {
                    let tk = &self.pool[k];
                    if tk.dying() && nd.playing() {
                        oldest = nd.age;
                        to_kill = Some(i);
                    } else if nd.age > oldest && !(tk.playing() && nd.dying()) {
                        oldest = nd.age;
                        to_kill = Some(i);
                    }
                }
            }
        }
        if let Some(k) = to_kill {
            if self.pool[k].dying() || self.pool[k].sustained() {
                self.kill_desc(k);
            } else {
                Self::entomb_desc(&mut self.pool[k]);
            }
        }
    }

    fn limit_voice(&mut self, preferred: i32) {
        let mut best: [Option<usize>; 8] = [None; 8];
        for (i, nd) in self.pool.iter().enumerate() {
            let cls = if nd.released() {
                0
            } else if nd.sustained() {
                2
            } else if nd.latched() {
                4
            } else if nd.playing() {
                6
            } else {
                continue;
            };
            if best[cls].map(|b| nd.age > self.pool[b].age).unwrap_or(true) {
                best[cls] = Some(i);
            }
            if nd.note as i32 == preferred && best[cls + 1].map(|b| self.pool[b].age != 0).unwrap_or(true) {
                best[cls + 1] = Some(i);
            }
        }
        let order = [1, 0, 3, 2, 5, 4, 7, 6];
        if let Some(k) = order.iter().find_map(|&o| best[o]) {
            Self::entomb_desc(&mut self.pool[k]);
        }
    }

    fn limit_voices(&mut self, pp: &PartParams, new_note: i32) {
        let mut vl = pp.voicelimit as i32;
        if vl == 0 {
            return;
        }
        if new_note >= 0 {
            vl -= 1;
        }
        let mut to_kill = self.running_voices() as i32 - vl;
        while to_kill > 0 {
            self.limit_voice(new_note);
            to_kill -= 1;
        }
    }

    // ---- monomem ----

    fn monomem_push(&mut self, note: u8) {
        if self.monomem.contains(&(note as i16)) {
            return;
        }
        for i in (0..255).rev() {
            self.monomem[i + 1] = self.monomem[i];
        }
        self.monomem[0] = note as i16;
    }
    fn monomem_pop(&mut self, note: u8) {
        if let Some(p) = self.monomem.iter().rposition(|&n| n == note as i16) {
            for i in p..255 {
                self.monomem[i] = self.monomem[i + 1];
            }
            self.monomem[255] = -1;
        }
    }
    fn monomem_empty(&self) -> bool {
        self.monomem[0] == -1
    }
    fn monomem_clear(&mut self) {
        self.monomem = [-1; 256];
    }

    // ---- events ----

    pub fn velocity(pp: &PartParams, velocity: u8) -> f32 {
        let v = vel_f(velocity as f32 / 127.0, pp.velsns);
        (v + (pp.veloffs as f32 - 64.0) / 64.0).clamp(0.0, 1.0)
    }

    pub fn note_log2(pp: &PartParams, note: u8, env: &PartEnv) -> Option<f32> {
        let f = note as f32 / 12.0;
        if pp.drummode {
            return Some(f + 440f32.log2() - 69.0 / 12.0);
        }
        env.micro.note_log2(f, pp.keyshift as i32 - 64 + env.master_keyshift)
    }

    fn make_ctx<'a>(
        synth: &'a SynthT,
        time: &'a AbsTime,
        ctl: &'a Controller,
        portas: &'a [PortaSlot],
        pp: &PartParams,
        changed: bool,
        cubic: bool,
        fft: &'a mut Fft,
        osc: &'a mut OscScratch,
    ) -> NoteCtx<'a> {
        NoteCtx { synth, time, ctl, changed, const_power: pp.const_power, cubic_interpolation: cubic, portas, fft, osc }
    }

    pub fn note_on(&mut self, pp: &PartParams, note: u8, velocity: u8, env: &mut PartEnv) -> bool {
        self.active_notes[note as usize & 127] = true;
        match Self::note_log2(pp, note, env) {
            Some(f) => self.note_on_internal(pp, note, velocity, f, env),
            None => false,
        }
    }

    fn note_on_internal(&mut self, pp: &PartParams, note: u8, velocity: u8, freq_log2: f32, env: &mut PartEnv) -> bool {
        let legato_mode = pp.is_legato() && !(pp.legatomode && pp.polymode && !pp.drummode);
        let mono_mode = pp.is_mono();
        let is_running = self.existing_running() > 0;
        let doing_legato = is_running && legato_mode && self.lastlegatomodevalid;
        if !pp.noteon
            || note < pp.minkey
            || note > pp.maxkey
            || self.pool.len() >= POLYPHONY
            || self.synth_full(Self::kit_usage(pp, note))
        {
            return false;
        }
        if mono_mode || legato_mode {
            self.monomem_push(note);
            self.monomem_data[note as usize] = (velocity, freq_log2);
        } else if !self.monomem_empty() {
            self.monomem_clear();
        }
        if mono_mode || (legato_mode && !doing_legato) {
            let ctl = &self.ctl;
            let cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
            for d in self.pool.iter_mut() {
                if d.playing() || d.sustained() || d.latched() {
                    Self::release_desc(d, pp, &cx);
                }
            }
        }
        self.lastlegatomodevalid = legato_mode;
        let vel = Self::velocity(pp, velocity);
        self.lastnote = note as i32;
        if self.oldfreq_log2 < 0.0 {
            self.oldfreq_log2 = freq_log2;
        }
        if let Some(o) = self.oldportamento {
            if self.portas[o].live && self.portas[o].p.active {
                self.oldportamentofreq_log2 += self.portas[o].p.freqdelta_log2;
            }
        }
        let porta = Portamento::new(&self.ctl, env.synth, is_running, self.oldfreq_log2, self.oldportamentofreq_log2, freq_log2);
        let mut slot = None;
        if porta.active {
            if doing_legato && self.legatoportamento.is_some() {
                let i = self.legatoportamento.unwrap();
                self.portas[i].p = porta;
                slot = Some(i);
            } else {
                slot = self.porta_alloc(porta);
                if doing_legato {
                    self.legatoportamento = slot;
                }
            }
        }
        self.oldfreq_log2 = freq_log2;
        self.oldportamentofreq_log2 = freq_log2;
        self.oldportamento = slot;
        let pref = self.porta_ref(slot);
        if doing_legato {
            let lp = LegatoParams { velocity: vel, portamento: pref, note_log2_freq: freq_log2, externcall: true, seed: prng() };
            self.apply_legato(pp, note, &lp, slot, env);
            return true;
        }
        self.legatoportamento = None;
        if pp.polymode {
            let ctl = &self.ctl;
            let cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
            for d in self.pool.iter_mut() {
                if d.note == note {
                    d.status |= SUSTAIN_BIT;
                    if d.sustained() {
                        Self::release_desc(d, pp, &cx);
                    }
                }
            }
        }
        for i in 0..NUM_KIT_ITEMS {
            let it = &pp.kit[i];
            if pp.kitmode != 0 && !it.valid_note(note) {
                continue;
            }
            let seed = prng();
            let sendto = if pp.kitmode != 0 { it.sendto() } else { 0 };
            self.limit_voices(pp, note as i32);
            let ini = NoteInit { velocity: vel, portamento: pref, note_log2_freq: freq_log2, quiet: false, seed };
            let new_notes = {
                let ctl = &self.ctl;
                let mut cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
                let mut v = Vec::new();
                if it.ad_enabled {
                    if let Some(p) = it.ad.as_deref() {
                        v.push(SynthNote { v: Voice::Ad(Box::new(AdNote::new(p, &ini, &mut cx))), kit: i });
                    }
                }
                if it.sub_enabled {
                    if let Some(p) = it.sub.as_deref() {
                        v.push(SynthNote { v: Voice::Sub(Box::new(SubNote::new(p, &ini, &mut cx))), kit: i });
                    }
                }
                if it.pad_enabled {
                    if let Some(p) = it.pad.as_deref() {
                        v.push(SynthNote { v: Voice::Pad(Box::new(PadNote::new(p, &ini, &mut cx))), kit: i });
                    }
                }
                v
            };
            for sn in new_notes {
                self.insert_note(note, sendto, sn, slot, false);
            }
            if pp.kitmode == 0 || (pp.kitmode == 2 && it.active()) {
                break;
            }
        }
        if legato_mode {
            self.upgrade_to_legato(pp, env);
        }
        let kl = if pp.keylimit == 0 { POLYPHONY - 5 } else { pp.keylimit as usize };
        if self.existing_running() >= kl {
            self.enforce_key_limit(kl);
        }
        true
    }

    fn upgrade_to_legato(&mut self, pp: &PartParams, env: &mut PartEnv) {
        let mut clones = Vec::new();
        {
            let ctl = &self.ctl;
            let mut cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
            for d in self.pool.iter() {
                if !d.playing() {
                    continue;
                }
                for sn in d.notes.iter() {
                    let it = &pp.kit[sn.kit];
                    let lg = match &sn.v {
                        Voice::Ad(n) => n.legato,
                        Voice::Sub(n) => n.legato,
                        Voice::Pad(n) => n.legato,
                    };
                    let ini = NoteInit { velocity: lg.vel, portamento: lg.portamento, note_log2_freq: lg.note_log2_freq, quiet: true, seed: lg.seed };
                    let v = match &sn.v {
                        Voice::Ad(_) => it.ad.as_deref().map(|p| Voice::Ad(Box::new(AdNote::new(p, &ini, &mut cx)))),
                        Voice::Sub(_) => it.sub.as_deref().map(|p| Voice::Sub(Box::new(SubNote::new(p, &ini, &mut cx)))),
                        Voice::Pad(_) => it.pad.as_deref().map(|p| Voice::Pad(Box::new(PadNote::new(p, &ini, &mut cx)))),
                    };
                    if let Some(v) = v {
                        clones.push((d.note, d.sendto, SynthNote { v, kit: sn.kit }));
                    }
                }
            }
        }
        for (note, sendto, sn) in clones {
            self.insert_note(note, sendto, sn, None, true);
        }
    }

    fn apply_legato(&mut self, pp: &PartParams, note: u8, lp: &LegatoParams, slot: Option<usize>, env: &mut PartEnv) {
        let mut swaps = Vec::new();
        for (i, d) in self.pool.iter_mut().enumerate() {
            if d.dying() {
                continue;
            }
            d.note = note;
            if !d.legato_mirror && slot.is_some() && d.porta != slot {
                swaps.push((i, d.porta));
                d.porta = slot;
            }
        }
        for (_, old) in swaps.iter() {
            self.porta_incref(slot);
            self.porta_decref(*old);
        }
        let ctl = &self.ctl;
        let mut cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
        for d in self.pool.iter_mut() {
            if d.dying() {
                continue;
            }
            for sn in d.notes.iter_mut() {
                let it = &pp.kit[sn.kit];
                match &mut sn.v {
                    Voice::Ad(n) => {
                        if let Some(p) = it.ad.as_deref() {
                            n.legatonote(p, lp, &mut cx);
                        }
                    }
                    Voice::Sub(n) => {
                        if let Some(p) = it.sub.as_deref() {
                            n.legatonote(p, lp, &mut cx);
                        }
                    }
                    Voice::Pad(n) => {
                        if let Some(p) = it.pad.as_deref() {
                            n.legatonote(p, lp, &mut cx);
                        }
                    }
                }
            }
        }
    }

    pub fn note_off(&mut self, pp: &PartParams, note: u8, env: &mut PartEnv) {
        self.active_notes[note as usize & 127] = false;
        if !self.monomem_empty() {
            self.monomem_pop(note);
        }
        let mono_or_legato = pp.is_mono() || pp.is_legato();
        let mut i = 0;
        while i < self.pool.len() {
            if self.pool[i].note != note || !self.pool[i].playing() {
                i += 1;
                continue;
            }
            if !self.ctl.sustain {
                if mono_or_legato && !self.monomem_empty() {
                    self.mono_mem_renote(pp, env);
                } else {
                    let ctl = &self.ctl;
                    let cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
                    Self::release_desc(&mut self.pool[i], pp, &cx);
                }
            } else if self.pool[i].can_sustain() {
                self.pool[i].set_status(KEY_RELEASED_AND_SUSTAINED);
            } else {
                let ctl = &self.ctl;
                let cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
                Self::release_desc(&mut self.pool[i], pp, &cx);
            }
            i += 1;
        }
    }

    fn mono_mem_renote(&mut self, pp: &PartParams, env: &mut PartEnv) {
        let n = self.monomem[0];
        if n < 0 {
            return;
        }
        let n = n as u8;
        self.monomem_pop(n);
        let (vel, f) = self.monomem_data[n as usize];
        self.note_on_internal(pp, n, vel, f, env);
    }

    pub fn release_all_keys(&mut self, pp: &PartParams, env: &mut PartEnv) {
        self.monomem_clear();
        let ctl = &self.ctl;
        let cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
        for d in self.pool.iter_mut() {
            if !d.playing() {
                continue;
            }
            if !ctl.sustain || !d.can_sustain() {
                Self::release_desc(d, pp, &cx);
            } else {
                d.set_status(KEY_RELEASED_AND_SUSTAINED);
            }
        }
    }

    fn release_sustained_keys(&mut self, pp: &PartParams, env: &mut PartEnv) {
        if (pp.is_mono() || pp.is_legato()) && !self.monomem_empty() && self.monomem[0] as i32 != self.lastnote {
            self.mono_mem_renote(pp, env);
        }
        let ctl = &self.ctl;
        let cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
        for d in self.pool.iter_mut() {
            if d.sustained() {
                Self::release_desc(d, pp, &cx);
            }
        }
    }

    pub fn poly_aftertouch(&mut self, pp: &PartParams, note: u8, velocity: u8, env: &mut PartEnv) {
        if !pp.noteon || note < pp.minkey || note > pp.maxkey || pp.drummode {
            return;
        }
        let velocity = velocity.max(1);
        if !pp.polymode {
            self.monomem_data[note as usize].0 = velocity;
        }
        let vel = Self::velocity(pp, velocity);
        let ctl = &self.ctl;
        let mut cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
        for d in self.pool.iter_mut() {
            if d.note != note || !d.playing() {
                continue;
            }
            for sn in d.notes.iter_mut() {
                let it = &pp.kit[sn.kit];
                macro_rules! setvel {
                    ($n:expr, $p:expr) => {{
                        $n.legato.set_silent(true);
                        let lg = $n.legato;
                        let lp = LegatoParams { velocity: vel, portamento: lg.portamento, note_log2_freq: lg.note_log2_freq, externcall: true, seed: lg.seed };
                        $n.legatonote($p, &lp, &mut cx);
                        $n.legato.set_decounter(0);
                    }};
                }
                match &mut sn.v {
                    Voice::Ad(n) => {
                        if let Some(p) = it.ad.as_deref() {
                            setvel!(n, p);
                        }
                    }
                    Voice::Sub(n) => {
                        if let Some(p) = it.sub.as_deref() {
                            setvel!(n, p);
                        }
                    }
                    Voice::Pad(n) => {
                        if let Some(p) = it.pad.as_deref() {
                            setvel!(n, p);
                        }
                    }
                }
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        self.killallnotes = true;
    }

    /// `Part::SetController`.
    pub fn set_controller(&mut self, pp: &PartParams, cc: u32, val: i32, env: &mut PartEnv) {
        match cc {
            1000 => self.ctl.setpitchwheel(val),
            11 => self.ctl.setexpression(val),
            65 => self.ctl.setportamento(val),
            10 => self.ctl.setpanning(val),
            74 => self.ctl.setfiltercutoff(val),
            71 => self.ctl.setfilterq(val),
            75 => self.ctl.setbandwidth(val),
            1 => self.ctl.setmodwheel(val),
            76 => self.ctl.setfmamp(val),
            7 => {
                self.ctl.setvolume(val);
                if self.ctl.cfg.volume_receive {
                    self.cc_volume_db = Some(volume127_to_db((self.ctl.volume * 127.0) as u8));
                }
            }
            64 => {
                self.ctl.setsustain(val);
                if !self.ctl.sustain {
                    self.release_sustained_keys(pp, env);
                }
            }
            120 => self.all_notes_off(),
            121 => {
                self.ctl.resetall();
                self.release_sustained_keys(pp, env);
                self.cc_volume_db = None;
            }
            123 => self.release_all_keys(pp, env),
            77 => self.ctl.setresonancecenter(val),
            78 => self.ctl.setresonancebw(val),
            _ => {}
        }
    }

    /// `ComputePartSmps`.
    pub fn compute(&mut self, pp: &PartParams, env: &mut PartEnv) {
        let bs = env.synth.buffersize;
        if !pp.enabled && !self.killallnotes {
            if !self.silent {
                self.outl.fill(0.0);
                self.outr.fill(0.0);
                self.silent = true;
            }
            return;
        }
        self.silent = false;
        for n in 0..=NUM_PART_EFX {
            self.fxin_l[n].fill(0.0);
            self.fxin_r[n].fill(0.0);
        }
        let mut tl = [0.0f32; super::filter::MAX_BUF];
        let mut tr = [0.0f32; super::filter::MAX_BUF];
        let mut di = 0;
        while di < self.pool.len() {
            self.pool[di].age = self.pool[di].age.wrapping_add(1);
            let sendto = self.pool[di].sendto as usize;
            let mut ni = 0;
            while ni < self.pool[di].notes.len() {
                let finished = {
                    let ctl = &self.ctl;
                    let mut cx = Self::make_ctx(env.synth, env.time, ctl, &self.portas, pp, env.changed, env.cubic, env.fft, env.osc);
                    let sn = &mut self.pool[di].notes[ni];
                    let it = &pp.kit[sn.kit.min(NUM_KIT_ITEMS - 1)];
                    match &mut sn.v {
                        Voice::Ad(n) => match it.ad.as_deref() {
                            Some(p) => {
                                n.noteout(p, &mut tl, &mut tr, &mut cx, env.scratch);
                                n.finished()
                            }
                            None => true,
                        },
                        Voice::Sub(n) => match it.sub.as_deref() {
                            Some(p) => {
                                n.noteout(p, &mut tl, &mut tr, &mut cx);
                                n.finished()
                            }
                            None => true,
                        },
                        Voice::Pad(n) => match it.pad.as_deref() {
                            Some(p) => {
                                n.noteout(p, &mut tl, &mut tr, &mut cx);
                                n.finished()
                            }
                            None => true,
                        },
                    }
                };
                let (fl, fr) = (&mut self.fxin_l[sendto.min(NUM_PART_EFX)], &mut self.fxin_r[sendto.min(NUM_PART_EFX)]);
                for i in 0..bs {
                    fl[i] += tl[i];
                    fr[i] += tr[i];
                }
                if finished {
                    self.pool[di].notes.remove(ni);
                } else {
                    ni += 1;
                }
            }
            if let Some(s) = self.pool[di].porta {
                self.portas[s].p.update();
            }
            if self.pool[di].notes.is_empty() {
                self.kill_desc(di);
            } else {
                di += 1;
            }
        }
        for n in 0..NUM_PART_EFX {
            if !pp.efxbypass[n] {
                let (a, b) = self.fxin_l.split_at_mut(n + 1);
                let (c, d) = self.fxin_r.split_at_mut(n + 1);
                let (l, r) = (&mut a[n], &mut c[n]);
                self.efx[n].out(l, r);
                if pp.efxroute[n] == 2 {
                    for i in 0..bs {
                        b[0][i] += self.efx[n].efxoutl[i];
                        d[0][i] += self.efx[n].efxoutr[i];
                    }
                }
            }
            let routeto = if pp.efxroute[n] == 0 { n + 1 } else { NUM_PART_EFX };
            for i in 0..bs {
                let (l, r) = (self.fxin_l[n][i], self.fxin_r[n][i]);
                self.fxin_l[routeto][i] += l;
                self.fxin_r[routeto][i] += r;
            }
        }
        self.outl.copy_from_slice(&self.fxin_l[NUM_PART_EFX][..bs]);
        self.outr.copy_from_slice(&self.fxin_r[NUM_PART_EFX][..bs]);
        if self.killallnotes {
            for i in 0..bs {
                let t = (bs - i) as f32 / bs as f32;
                self.outl[i] *= t;
                self.outr[i] *= t;
            }
            self.kill_all_notes();
            self.monomem_clear();
            self.killallnotes = false;
            for e in self.efx.iter_mut() {
                e.cleanup();
            }
        }
    }

    /// Clear everything (instrument load).
    pub fn cleanup(&mut self) {
        self.kill_all_notes();
        self.outl.fill(0.0);
        self.outr.fill(0.0);
        self.ctl.resetall();
        for e in self.efx.iter_mut() {
            e.cleanup();
        }
        self.monomem_clear();
        self.oldfreq_log2 = -1.0;
        self.oldportamento = None;
        self.legatoportamento = None;
        self.active_notes = [false; 128];
    }
}

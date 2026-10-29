//! `Misc/Master.cpp`: 16 parts, 8 insertion effects, 4 system effects,
//! master volume and tuning; `.xmz` (whole setup) and `.xiz`
//! (instrument) files.
//!
//! `Patch` is the immutable parameter snapshot (shared with the editor
//! through `Arc`); `Engine` owns all real-time state and renders fixed
//! `buffersize` blocks.

use std::sync::Arc;

use super::effects::{EffectCfg, EffectMgr};
use super::fft::Fft;
use super::filter::ValueSmoothing;
use super::lfo::AbsTime;
use super::microtonal::Microtonal;
use super::oscilgen::OscScratch;
use super::part::{PartEnv, PartParams, PartRt, volume127_to_db};
use super::util::*;
use super::xml::{Version, XmlDoc, XmlNode};
use super::NoteScratch;

pub const NUM_MIDI_PARTS: usize = 16;

#[derive(Clone, Debug)]
pub struct Patch {
    pub volume: f32,
    pub keyshift: u8,
    pub parts: Vec<Arc<PartParams>>,
    pub sysefx: Vec<EffectCfg>,
    /// [efx][part]
    pub sysefxvol: [[u8; NUM_MIDI_PARTS]; NUM_SYS_EFX],
    /// [from][to]
    pub sysefxsend: [[u8; NUM_SYS_EFX]; NUM_SYS_EFX],
    pub insefx: Vec<EffectCfg>,
    pub insparts: [i32; NUM_INS_EFX],
    pub micro: Microtonal,
    pub const_power: bool,
    pub oscilsize: usize,
}

impl Patch {
    pub fn new(oscilsize: usize) -> Self {
        let mut parts: Vec<Arc<PartParams>> = (0..NUM_MIDI_PARTS)
            .map(|i| {
                let mut p = PartParams::new(oscilsize);
                p.rcvchn = (i % 16) as u8;
                Arc::new(p)
            })
            .collect();
        Arc::make_mut(&mut parts[0]).enabled = true;
        Self {
            volume: f32::from_bits(0xC0D5_5556),
            keyshift: 64,
            parts,
            sysefx: (0..NUM_SYS_EFX).map(|_| EffectCfg::default()).collect(),
            sysefxvol: [[0; NUM_MIDI_PARTS]; NUM_SYS_EFX],
            sysefxsend: [[0; NUM_SYS_EFX]; NUM_SYS_EFX],
            insefx: (0..NUM_INS_EFX).map(|_| EffectCfg::default()).collect(),
            insparts: [-1; NUM_INS_EFX],
            micro: Microtonal::default(),
            const_power: true,
            oscilsize,
        }
    }

    /// Prepare every oscillator and PADsynth table (heavy: run off the
    /// audio thread).
    pub fn prepare(&mut self, sr: f32) {
        let mut fft = Fft::new(self.oscilsize);
        for p in self.parts.iter_mut() {
            Arc::make_mut(p).prepare(&mut fft, sr);
        }
    }

    pub fn to_xml(&self) -> XmlDoc {
        let mut doc = XmlDoc::new();
        let m = doc.root.child("MASTER");
        m.add_parreal("volume", self.volume);
        m.add_par("key_shift", self.keyshift as i32);
        m.add_parbool("nrpn_receive", true);
        self.micro.add_to_xml(m.child("MICROTONAL"));
        for (i, p) in self.parts.iter().enumerate() {
            p.add_to_xml(m.child_id("PART", i));
        }
        let se = m.child("SYSTEM_EFFECTS");
        for n in 0..NUM_SYS_EFX {
            let e = se.child_id("SYSTEM_EFFECT", n);
            self.sysefx[n].add_to_xml(e.child("EFFECT"), false);
            for p in 0..NUM_MIDI_PARTS {
                e.child_id("VOLUME", p).add_par("vol", self.sysefxvol[n][p] as i32);
            }
            for t in (n + 1)..NUM_SYS_EFX {
                e.child_id("SENDTO", t).add_par("send_vol", self.sysefxsend[n][t] as i32);
            }
        }
        let ie = m.child("INSERTION_EFFECTS");
        for n in 0..NUM_INS_EFX {
            let e = ie.child_id("INSERTION_EFFECT", n);
            e.add_par("part", self.insparts[n]);
            self.insefx[n].add_to_xml(e.child("EFFECT"), true);
        }
        doc
    }

    /// Load a `.xmz` master file.
    pub fn from_xmz(doc: &XmlDoc, oscilsize: usize) -> Result<Self, String> {
        let m = doc.root.branch("MASTER").ok_or("not a ZynAddSubFX master file (no MASTER)")?;
        let ver = doc.version;
        let mut p = Self::new(oscilsize);
        p.const_power = ver >= Version(3, 0, 7);
        p.volume = if m.has_parreal("volume") { m.parreal("volume", p.volume) } else { volume127_to_db(m.par127("volume", 0)) };
        p.keyshift = m.par127("key_shift", p.keyshift);
        Arc::make_mut(&mut p.parts[0]).enabled = false;
        for i in 0..NUM_MIDI_PARTS {
            let Some(x) = m.branch_id("PART", i) else { continue };
            let pp = Arc::make_mut(&mut p.parts[i]);
            pp.get_from_xml(x, ver, oscilsize);
        }
        if let Some(x) = m.branch("MICROTONAL") {
            p.micro.get_from_xml(x);
        }
        if let Some(se) = m.branch("SYSTEM_EFFECTS") {
            for n in 0..NUM_SYS_EFX {
                let Some(e) = se.branch_id("SYSTEM_EFFECT", n) else { continue };
                if let Some(ef) = e.branch("EFFECT") {
                    p.sysefx[n].get_from_xml(ef, ver, false);
                }
                for pi in 0..NUM_MIDI_PARTS {
                    if let Some(v) = e.branch_id("VOLUME", pi) {
                        p.sysefxvol[n][pi] = v.par127("vol", p.sysefxvol[n][pi]);
                    }
                }
                for t in (n + 1)..NUM_SYS_EFX {
                    if let Some(v) = e.branch_id("SENDTO", t) {
                        p.sysefxsend[n][t] = v.par127("send_vol", p.sysefxsend[n][t]);
                    }
                }
            }
        }
        if let Some(ie) = m.branch("INSERTION_EFFECTS") {
            for n in 0..NUM_INS_EFX {
                let Some(e) = ie.branch_id("INSERTION_EFFECT", n) else { continue };
                p.insparts[n] = e.par("part", p.insparts[n], -2, NUM_MIDI_PARTS as i32);
                if let Some(ef) = e.branch("EFFECT") {
                    p.insefx[n].get_from_xml(ef, ver, true);
                }
            }
        }
        Ok(p)
    }

    /// Load a `.xiz` instrument into part `npart` (keeps part settings).
    pub fn load_xiz(&mut self, doc: &XmlDoc, npart: usize) -> Result<(), String> {
        let ins = doc.root.branch("INSTRUMENT").ok_or("not a ZynAddSubFX instrument (no INSTRUMENT)")?;
        let os = self.oscilsize;
        let pp = Arc::make_mut(&mut self.parts[npart]);
        pp.defaults_instrument(os);
        pp.get_instrument_xml(ins, doc.version, os);
        Ok(())
    }

    /// Instrument file for one part.
    pub fn xiz_of(&self, npart: usize) -> XmlDoc {
        let mut doc = XmlDoc::new();
        self.parts[npart].add_instrument_xml(doc.root.child("INSTRUMENT"));
        doc
    }

    /// Parse any Zyn file (master or instrument) into a patch.
    pub fn from_bytes(bytes: &[u8], oscilsize: usize) -> Result<Self, String> {
        let doc = XmlDoc::parse(bytes)?;
        if doc.root.branch("MASTER").is_some() {
            Self::from_xmz(&doc, oscilsize)
        } else {
            let mut p = Self::new(oscilsize);
            p.load_xiz(&doc, 0)?;
            Ok(p)
        }
    }

    pub fn sysefxvol_gain(v: u8) -> f32 {
        0.1f32.powf((1.0 - v as f32 / 96.0) * 2.0)
    }
}

/// MIDI-ish events the engine understands.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    NoteOn { ch: u8, note: u8, vel: u8 },
    NoteOff { ch: u8, note: u8 },
    PolyAftertouch { ch: u8, note: u8, vel: u8 },
    Controller { ch: u8, cc: u32, val: i32 },
    AllSoundOff,
}

pub struct Engine {
    pub synth: SynthT,
    pub time: AbsTime,
    pub patch: Arc<Patch>,
    parts: Vec<PartRt>,
    sysefx: Vec<EffectMgr>,
    insefx: Vec<EffectMgr>,
    part_smooth_l: Vec<ValueSmoothing>,
    part_smooth_r: Vec<ValueSmoothing>,
    smoothing: ValueSmoothing,
    fft: Fft,
    osc: OscScratch,
    scratch: NoteScratch,
    outl: Vec<f32>,
    outr: Vec<f32>,
    /// Samples of the current block not yet handed out.
    pending: usize,
    changed: bool,
    pub cubic: bool,
    pub tempo: f32,
    pub peak: [f32; 2],
    /// Copy of part 0's raw output (before part gain), for tests.
    pub debug_part: Option<(Vec<f32>, Vec<f32>)>,
}

impl Engine {
    pub fn new(sr: u32, buffersize: usize, patch: Arc<Patch>) -> Self {
        let synth = SynthT::new(sr, buffersize, patch.oscilsize);
        let srf = sr as f32;
        let parts = patch.parts.iter().map(|p| PartRt::new(srf, buffersize, &p.ctl)).collect();
        let mk = |s: &mut ValueSmoothing| {
            s.sample_rate(srf);
            s.reset_on_next_apply(true);
        };
        let mut part_smooth_l = vec![ValueSmoothing::default(); NUM_MIDI_PARTS];
        let mut part_smooth_r = vec![ValueSmoothing::default(); NUM_MIDI_PARTS];
        part_smooth_l.iter_mut().for_each(mk);
        part_smooth_r.iter_mut().for_each(mk);
        let mut smoothing = ValueSmoothing::default();
        mk(&mut smoothing);
        let mut e = Self {
            synth,
            time: AbsTime::new(synth.dt()),
            patch: patch.clone(),
            parts,
            sysefx: (0..NUM_SYS_EFX).map(|_| EffectMgr::new(false, srf, buffersize)).collect(),
            insefx: (0..NUM_INS_EFX).map(|_| EffectMgr::new(true, srf, buffersize)).collect(),
            part_smooth_l,
            part_smooth_r,
            smoothing,
            fft: Fft::new(patch.oscilsize),
            osc: OscScratch::new(patch.oscilsize),
            scratch: NoteScratch::new(),
            outl: vec![0.0; buffersize],
            outr: vec![0.0; buffersize],
            pending: 0,
            changed: false,
            cubic: false,
            tempo: 120.0,
            peak: [0.0; 2],
            debug_part: None,
        };
        e.apply_params();
        e
    }

    fn apply_params(&mut self) {
        let p = self.patch.clone();
        for (i, pr) in self.parts.iter_mut().enumerate() {
            pr.apply_params(&p.parts[i], self.tempo);
        }
        for n in 0..NUM_SYS_EFX {
            self.sysefx[n].const_power = p.const_power;
            self.sysefx[n].apply(&p.sysefx[n], self.tempo);
        }
        for n in 0..NUM_INS_EFX {
            self.insefx[n].const_power = p.const_power;
            self.insefx[n].apply(&p.insefx[n], self.tempo);
        }
    }

    /// Swap in an edited patch; running notes keep playing.
    pub fn set_patch(&mut self, patch: Arc<Patch>) {
        self.patch = patch;
        self.changed = true;
        self.apply_params();
    }

    /// Swap in a different sound: silence everything first.
    pub fn load_patch(&mut self, patch: Arc<Patch>) {
        for p in self.parts.iter_mut() {
            p.cleanup();
        }
        for e in self.sysefx.iter_mut().chain(self.insefx.iter_mut()) {
            e.cleanup();
        }
        self.patch = patch;
        self.changed = true;
        self.apply_params();
    }

    pub fn part_rt(&self, i: usize) -> &PartRt {
        &self.parts[i]
    }

    pub fn active_notes(&self, part: usize) -> usize {
        self.parts[part].note_count()
    }

    pub fn event(&mut self, ev: Event) {
        let patch = self.patch.clone();
        let master_keyshift = patch.keyshift as i32 - 64;
        let mut env = PartEnv {
            synth: &self.synth,
            time: &self.time,
            micro: &patch.micro,
            master_keyshift,
            changed: self.changed,
            cubic: self.cubic,
            fft: &mut self.fft,
            osc: &mut self.osc,
            scratch: &mut self.scratch,
        };
        for (i, pr) in self.parts.iter_mut().enumerate() {
            let pp = &patch.parts[i];
            match ev {
                Event::NoteOn { ch, note, vel } => {
                    if ch == pp.rcvchn && pp.enabled {
                        if vel == 0 {
                            pr.note_off(pp, note, &mut env);
                        } else {
                            pr.note_on(pp, note, vel, &mut env);
                        }
                    }
                }
                Event::NoteOff { ch, note } => {
                    if ch == pp.rcvchn {
                        pr.note_off(pp, note, &mut env);
                    }
                }
                Event::PolyAftertouch { ch, note, vel } => {
                    if ch == pp.rcvchn {
                        pr.poly_aftertouch(pp, note, vel, &mut env);
                    }
                }
                Event::Controller { ch, cc, val } => {
                    if ch == pp.rcvchn {
                        pr.set_controller(pp, cc, val, &mut env);
                    }
                }
                Event::AllSoundOff => pr.all_notes_off(),
            }
        }
    }

    /// `Master::AudioOut`: one engine buffer.
    fn render_block(&mut self) {
        let bs = self.synth.buffersize;
        let patch = self.patch.clone();
        self.time.tempo = self.tempo.round().max(1.0) as u32;
        self.outl.fill(0.0);
        self.outr.fill(0.0);
        {
            let master_keyshift = patch.keyshift as i32 - 64;
            let mut env = PartEnv {
                synth: &self.synth,
                time: &self.time,
                micro: &patch.micro,
                master_keyshift,
                changed: self.changed,
                cubic: self.cubic,
                fft: &mut self.fft,
                osc: &mut self.osc,
                scratch: &mut self.scratch,
            };
            for (i, pr) in self.parts.iter_mut().enumerate() {
                pr.compute(&patch.parts[i], &mut env);
            }
        }
        self.changed = false;
        if let Some((l, r)) = self.debug_part.as_mut() {
            l.extend_from_slice(&self.parts[0].outl);
            r.extend_from_slice(&self.parts[0].outr);
        }
        for n in 0..NUM_INS_EFX {
            let ip = patch.insparts[n];
            if ip >= 0 && (ip as usize) < NUM_MIDI_PARTS && patch.parts[ip as usize].enabled {
                let pr = &mut self.parts[ip as usize];
                self.insefx[n].out(&mut pr.outl, &mut pr.outr);
            }
        }
        let mut gainbuf = [0.0f32; super::filter::MAX_BUF];
        for i in 0..NUM_MIDI_PARTS {
            let pp = &patch.parts[i];
            if !pp.enabled {
                continue;
            }
            let pr = &mut self.parts[i];
            let g = pr.gain(pp);
            let pan = pr.panning(pp);
            let (mut vl, mut vr) = (g, g);
            if patch.const_power {
                vr *= pan.sqrt();
                vl *= (1.0 - pan).sqrt();
            } else if pan < 0.5 {
                vr *= pan * 2.0;
            } else {
                vl *= (1.0 - pan) * 2.0;
            }
            if self.part_smooth_l[i].apply(&mut gainbuf[..bs], vl) {
                for k in 0..bs {
                    pr.outl[k] *= gainbuf[k];
                }
            } else {
                for k in 0..bs {
                    pr.outl[k] *= vl;
                }
            }
            if self.part_smooth_r[i].apply(&mut gainbuf[..bs], vr) {
                for k in 0..bs {
                    pr.outr[k] *= gainbuf[k];
                }
            } else {
                for k in 0..bs {
                    pr.outr[k] *= vr;
                }
            }
            let (mut pl, mut prr) = (0.0f32, 0.0f32);
            for k in 0..bs {
                pl = pl.max(pr.outl[k].abs());
                prr = prr.max(pr.outr[k].abs());
            }
            pr.peak = [pr.peak[0].max(pl), pr.peak[1].max(prr)];
        }
        let mut tl = [0.0f32; super::filter::MAX_BUF];
        let mut tr = [0.0f32; super::filter::MAX_BUF];
        let (tl, tr) = (&mut tl[..bs], &mut tr[..bs]);
        for n in 0..NUM_SYS_EFX {
            if patch.sysefx[n].kind == 0 {
                continue;
            }
            tl.fill(0.0);
            tr.fill(0.0);
            for i in 0..NUM_MIDI_PARTS {
                if patch.sysefxvol[n][i] == 0 || !patch.parts[i].enabled {
                    continue;
                }
                let v = Patch::sysefxvol_gain(patch.sysefxvol[n][i]);
                let pr = &self.parts[i];
                for k in 0..bs {
                    tl[k] += pr.outl[k] * v;
                    tr[k] += pr.outr[k] * v;
                }
            }
            for from in 0..n {
                if patch.sysefxsend[from][n] != 0 {
                    let v = Patch::sysefxvol_gain(patch.sysefxsend[from][n]);
                    for k in 0..bs {
                        tl[k] += self.sysefx[from].efxoutl[k] * v;
                        tr[k] += self.sysefx[from].efxoutr[k] * v;
                    }
                }
            }
            self.sysefx[n].out(tl, tr);
            let ov = self.sysefx[n].sysefx_volume();
            for k in 0..bs {
                self.outl[k] += tl[k] * ov;
                self.outr[k] += tr[k] * ov;
            }
        }
        for i in 0..NUM_MIDI_PARTS {
            if patch.parts[i].enabled {
                let pr = &self.parts[i];
                for k in 0..bs {
                    self.outl[k] += pr.outl[k];
                    self.outr[k] += pr.outr[k];
                }
            }
        }
        for n in 0..NUM_INS_EFX {
            if patch.insparts[n] == -2 {
                self.insefx[n].out(&mut self.outl, &mut self.outr);
            }
        }
        let vol = db2rap(patch.volume);
        if self.smoothing.apply(&mut gainbuf[..bs], vol) {
            for k in 0..bs {
                self.outl[k] *= gainbuf[k];
                self.outr[k] *= gainbuf[k];
            }
        } else {
            for k in 0..bs {
                self.outl[k] *= vol;
                self.outr[k] *= vol;
            }
        }
        for k in 0..bs {
            self.peak[0] = self.peak[0].max(self.outl[k].abs());
            self.peak[1] = self.peak[1].max(self.outr[k].abs());
        }
        self.time.frames += 1;
    }

    /// Fill `l`/`r` (any length), rendering engine blocks as needed —
    /// like Zyn's plugin wrappers, events land on block boundaries.
    pub fn render(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        let bs = self.synth.buffersize;
        let mut i = 0;
        while i < n {
            if self.pending == 0 {
                self.render_block();
                self.pending = bs;
            }
            let start = bs - self.pending;
            let take = self.pending.min(n - i);
            l[i..i + take].copy_from_slice(&self.outl[start..start + take]);
            r[i..i + take].copy_from_slice(&self.outr[start..start + take]);
            self.pending -= take;
            i += take;
        }
    }

    pub fn take_peaks(&mut self) -> ([f32; 2], Vec<[f32; 2]>) {
        let m = self.peak;
        self.peak = [0.0; 2];
        let p = self
            .parts
            .iter_mut()
            .map(|p| {
                let v = p.peak;
                p.peak = [0.0; 2];
                v
            })
            .collect();
        (m, p)
    }

    pub fn all_sound_off(&mut self) {
        for p in self.parts.iter_mut() {
            p.all_notes_off();
        }
    }
}

#[allow(dead_code)]
fn _assert_send() {
    fn s<T: Send>() {}
    s::<Engine>();
    s::<Patch>();
    let _ = XmlNode::new("x");
}

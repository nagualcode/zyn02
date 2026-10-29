//! `Synth/SynthNote.cpp` (legato fades, per-note random) and
//! `Synth/Portamento.cpp`.

use super::controller::Controller;
use super::util::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct Portamento {
    pub active: bool,
    pub freqdelta_log2: f32,
    x: f32,
    dx: f32,
    origfreqdelta_log2: f32,
}

impl Portamento {
    pub fn new(ctl: &Controller, synth: &SynthT, is_running_note: bool, oldfreq_log2: f32, oldportamentofreq_log2: f32, newfreq_log2: f32) -> Self {
        let mut p = Self::default();
        let c = &ctl.cfg;
        if !ctl.portamento {
            return p;
        }
        if c.portamento_auto && !is_running_note {
            return p;
        }
        if oldfreq_log2 == newfreq_log2 {
            return p;
        }
        let mut t = 100f32.powf(c.portamento_time as f32 / 127.0) / 50.0;
        let delta = oldportamentofreq_log2 - newfreq_log2;
        let absdelta = delta.abs();
        let absnote = (oldfreq_log2 - newfreq_log2).abs();
        if c.portamento_proportional {
            let absdeltaf = 2f32.powf(absdelta);
            t *= (absdeltaf / (c.portamento_proprate as f32 / 127.0 * 3.0 + 0.05))
                .powf(c.portamento_propdepth as f32 / 127.0 * 1.6 + 0.2);
        }
        let uds = c.portamento_updowntimestretch;
        if uds >= 64 && newfreq_log2 < oldfreq_log2 {
            if uds == 127 {
                return p;
            }
            t *= 0.1f32.powf((uds as f32 - 64.0) / 63.0);
        }
        if uds < 64 && newfreq_log2 > oldfreq_log2 {
            if uds == 0 {
                return p;
            }
            t *= 0.1f32.powf((64.0 - uds as f32) / 64.0);
        }
        let thr = c.portamento_pitchthresh as f32 / 12.0;
        if !c.portamento_pitchthreshtype && absnote - 0.00001 > thr {
            return p;
        }
        if c.portamento_pitchthreshtype && absnote + 0.00001 < thr {
            return p;
        }
        p.x = 0.0;
        p.dx = synth.buffersize_f / (t * synth.samplerate_f);
        p.origfreqdelta_log2 = delta;
        p.freqdelta_log2 = delta;
        p.active = true;
        p
    }

    pub fn update(&mut self) {
        if !self.active {
            return;
        }
        self.x += self.dx;
        if self.x > 1.0 {
            self.x = 1.0;
            self.active = false;
        }
        self.freqdelta_log2 = (1.0 - self.x) * self.origfreqdelta_log2;
    }
}

/// Handle to a part-level portamento slot (Zyn shares one
/// `PortamentoRealtime` between the notes of a key press).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortaRef {
    pub idx: usize,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PortaSlot {
    pub generation: u32,
    pub live: bool,
    pub p: Portamento,
}

pub fn porta_read(slots: &[PortaSlot], r: PortaRef) -> Option<Portamento> {
    let s = slots.get(r.idx)?;
    (s.live && s.generation == r.generation).then_some(s.p)
}

#[derive(Clone, Copy, Debug)]
pub struct LegatoParams {
    pub velocity: f32,
    pub portamento: Option<PortaRef>,
    pub note_log2_freq: f32,
    pub externcall: bool,
    pub seed: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Msg {
    Norm,
    FadeIn,
    FadeOut,
    CatchUp,
    ToNorm,
}

#[derive(Clone, Copy, Debug)]
pub struct Legato {
    msg: Msg,
    fade_length: i32,
    fade_step: f32,
    fade_m: f32,
    decounter: i32,
    pub vel: f32,
    pub portamento: Option<PortaRef>,
    pub note_log2_freq: f32,
    pub seed: u32,
    lastfreq_log2: f32,
    pub silent: bool,
}

impl Legato {
    pub fn new(synth: &SynthT, vel: f32, portamento: Option<PortaRef>, note_log2_freq: f32, quiet: bool, seed: u32) -> Self {
        let fade_length = ((synth.samplerate_f * 0.005) as i32).max(1);
        Self {
            msg: Msg::Norm,
            fade_length,
            fade_step: 1.0 / fade_length as f32,
            fade_m: 0.0,
            decounter: -10,
            vel,
            portamento,
            note_log2_freq,
            seed,
            lastfreq_log2: note_log2_freq,
            silent: quiet,
        }
    }

    pub fn set_silent(&mut self, s: bool) {
        self.silent = s;
    }

    pub fn set_decounter(&mut self, d: i32) {
        self.decounter = d;
    }

    /// Returns true when the note must not be retuned yet (fade-out first).
    pub fn update(&mut self, p: &LegatoParams) -> bool {
        if p.externcall {
            self.msg = Msg::Norm;
        }
        if self.msg != Msg::CatchUp {
            self.lastfreq_log2 = self.note_log2_freq;
            self.vel = p.velocity;
            self.portamento = p.portamento;
            self.note_log2_freq = p.note_log2_freq;
            if self.msg == Msg::Norm {
                if self.silent {
                    self.fade_m = 0.0;
                    self.msg = Msg::FadeIn;
                } else {
                    self.fade_m = 1.0;
                    self.msg = Msg::FadeOut;
                    return true;
                }
            }
            if self.msg == Msg::ToNorm {
                self.msg = Msg::Norm;
            }
        }
        false
    }

    /// Apply fades; returns a retune request for the note when due.
    pub fn apply(&mut self, outl: &mut [f32], outr: &mut [f32]) -> Option<LegatoParams> {
        let n = outl.len();
        if self.silent && self.msg != Msg::FadeIn {
            outl.fill(0.0);
            outr.fill(0.0);
        }
        match self.msg {
            Msg::CatchUp => {
                if self.decounter == -10 {
                    self.decounter = self.fade_length;
                }
                for _ in 0..n {
                    self.decounter -= 1;
                    if self.decounter < 1 {
                        self.decounter = -10;
                        self.msg = Msg::ToNorm;
                        return Some(LegatoParams {
                            velocity: self.vel,
                            portamento: self.portamento,
                            note_log2_freq: self.note_log2_freq,
                            externcall: false,
                            seed: self.seed,
                        });
                    }
                }
            }
            Msg::FadeIn => {
                if self.decounter == -10 {
                    self.decounter = self.fade_length;
                }
                self.silent = false;
                for i in 0..n {
                    self.decounter -= 1;
                    if self.decounter < 1 {
                        self.decounter = -10;
                        self.msg = Msg::Norm;
                        break;
                    }
                    self.fade_m += self.fade_step;
                    outl[i] *= self.fade_m;
                    outr[i] *= self.fade_m;
                }
            }
            Msg::FadeOut => {
                if self.decounter == -10 {
                    self.decounter = self.fade_length;
                }
                for i in 0..n {
                    self.decounter -= 1;
                    if self.decounter < 1 {
                        outl[i..].fill(0.0);
                        outr[i..].fill(0.0);
                        self.silent = true;
                        self.decounter = self.fade_length;
                        self.msg = Msg::CatchUp;
                        let catchup = 2.0 * self.note_log2_freq - self.lastfreq_log2;
                        return Some(LegatoParams {
                            velocity: self.vel,
                            portamento: self.portamento,
                            note_log2_freq: catchup,
                            externcall: false,
                            seed: self.seed,
                        });
                    }
                    self.fade_m -= self.fade_step;
                    outl[i] *= self.fade_m;
                    outr[i] *= self.fade_m;
                }
            }
            _ => {}
        }
        None
    }
}

/// `SynthNote::getRandomFloat` (note-local LCG, full 32-bit state).
#[inline]
pub fn note_random_float(state: &mut u32) -> f32 {
    prng_r(state) as f32 / 2_147_483_520.0
}

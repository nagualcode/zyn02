//! `Synth/ModFilter.cpp`: a note filter driven by envelope, LFO,
//! velocity and keyboard tracking.

use super::envelope::Envelope;
use super::filter::{Filter, FilterParams};
use super::lfo::{AbsTime, Lfo, LfoParams};
use super::envelope::EnvelopeParams;
use super::util::*;

#[derive(Clone, Debug)]
pub struct ModFilter {
    pub left: Filter,
    pub right: Option<Filter>,
    pub env: Option<Envelope>,
    pub lfo: Option<Lfo>,
    category: u8,
    note_freq: f32,
    tracking: f32,
    base_q: f32,
    base_freq: f32,
    sense: f32,
    sr: f32,
    bufsize: usize,
}

impl ModFilter {
    pub fn new(p: &FilterParams, sr: f32, bufsize: usize, stereo: bool, notefreq: f32) -> Self {
        Self {
            left: Filter::generate(p, sr, bufsize),
            right: stereo.then(|| Filter::generate(p, sr, bufsize)),
            env: None,
            lfo: None,
            category: p.category,
            note_freq: notefreq,
            tracking: p.getfreqtracking(notefreq),
            base_q: p.getq(),
            base_freq: p.getfreq(),
            sense: 0.0,
            sr,
            bufsize,
        }
    }

    pub fn with_env(mut self, p: &EnvelopeParams, basefreq: f32, dt: f32) -> Self {
        self.env = Some(Envelope::new(p, basefreq, dt));
        self
    }

    pub fn with_lfo(mut self, p: &LfoParams, basefreq: f32, t: &AbsTime) -> Self {
        self.lfo = Some(Lfo::new(p, basefreq, t));
        self
    }

    fn param_update(f: &mut Filter, p: &FilterParams, category: u8, sr: f32, bufsize: usize) {
        if category != p.category {
            *f = Filter::generate(p, sr, bufsize);
            return;
        }
        match f {
            Filter::Sv(sv) => {
                sv.settype(p.ftype as i32);
                sv.setstages(p.stages as i32);
            }
            Filter::Analog(an) => {
                an.settype(p.ftype as i32);
                an.setstages(p.stages as i32);
                an.setgain(p.getgain());
            }
            Filter::Moog(mg) => {
                mg.settype(p.ftype);
                mg.setgain(p.getgain());
            }
            Filter::Comb(cb) => {
                cb.settype(p.ftype);
                cb.setgain(p.getgain());
                cb.sethpf(p.hpf);
                cb.setlpf(p.lpf);
            }
            Filter::Formant(_) => {}
        }
    }

    /// `update(relfreq, relq)`. `changed` = parameters were edited this
    /// frame (Zyn's `last_update_timestamp == time`).
    pub fn update(
        &mut self,
        relfreq: f32,
        relq: f32,
        p: &FilterParams,
        lfop: Option<&LfoParams>,
        t: &AbsTime,
        changed: bool,
    ) {
        if changed {
            let (sr, bs, cat) = (self.sr, self.bufsize, self.category);
            Self::param_update(&mut self.left, p, cat, sr, bs);
            if let Some(r) = self.right.as_mut() {
                Self::param_update(r, p, cat, sr, bs);
            }
            self.category = p.category;
            self.base_freq = p.getfreq();
            self.base_q = p.getq();
            self.tracking = p.getfreqtracking(self.note_freq);
        }
        let e = self.env.as_mut().map(|e| e.envout()).unwrap_or(0.0);
        let l = match (self.lfo.as_mut(), lfop) {
            (Some(l), Some(lp)) => l.lfoout(lp, t),
            _ => 0.0,
        };
        let fc = self.base_freq + self.sense + e + l;
        let fc_mod = fc + relfreq + self.tracking;
        let hz = Filter::getrealfreq(fc_mod);
        let q = self.base_q * relq;
        self.left.setfreq_and_q(hz, q);
        if let Some(r) = self.right.as_mut() {
            r.setfreq_and_q(hz, q);
        }
    }

    pub fn update_note_freq(&mut self, f: f32, p: &FilterParams) {
        self.note_freq = f;
        self.tracking = p.getfreqtracking(f);
    }

    pub fn update_sense(&mut self, velocity: f32, scale: u8, func: u8) {
        let vs = scale as f32 / 127.0;
        self.sense = vs * 6.0 * (vel_f(velocity, func) - 1.0);
    }

    pub fn filter(&mut self, l: &mut [f32], r: Option<&mut [f32]>) {
        self.left.filterout(l);
        if let (Some(f), Some(r)) = (self.right.as_mut(), r) {
            f.filterout(r);
        }
    }
}


//! `Synth/Resonance.cpp`: a spectral resonance curve applied to the
//! oscillator's harmonics (ADsynth) or to the PADsynth profile.

use super::fft::C32;
use super::util::*;
use super::xml::XmlNode;

pub const N_RES_POINTS: usize = 256;

#[derive(Clone, Debug)]
pub struct Resonance {
    pub enabled: bool,
    pub points: [u8; N_RES_POINTS],
    pub max_db: u8,
    pub centerfreq: u8,
    pub octavesfreq: u8,
    pub protect_fundamental: bool,
}

impl Default for Resonance {
    fn default() -> Self {
        Self { enabled: false, points: [64; N_RES_POINTS], max_db: 20, centerfreq: 64, octavesfreq: 64, protect_fundamental: false }
    }
}

impl Resonance {
    pub fn getcenterfreq(&self) -> f32 {
        10000.0 * 10f32.powf(-(1.0 - self.centerfreq as f32 / 127.0) * 2.0)
    }
    pub fn getoctavesfreq(&self) -> f32 {
        0.25 + 10.0 * self.octavesfreq as f32 / 127.0
    }
    pub fn getfreqx(&self, x: f32) -> f32 {
        let octf = 2f32.powf(self.getoctavesfreq());
        self.getcenterfreq() / octf.sqrt() * octf.powf(x.clamp(0.0, 1.0))
    }
    pub fn getfreqpos(&self, freq: f32) -> f32 {
        (freq.ln() - self.getfreqx(0.0).ln()) / 2f32.ln() / self.getoctavesfreq()
    }

    fn upper(&self) -> f32 {
        (*self.points.iter().max().unwrap_or(&0) as f32).max(1.0)
    }

    fn lookup(&self, x: f32, upper: f32) -> f32 {
        let dx = x - x.floor();
        let kx1 = (x.floor() as i64).clamp(0, N_RES_POINTS as i64 - 1) as usize;
        let kx2 = (kx1 + 1).min(N_RES_POINTS - 1);
        let y = (self.points[kx1] as f32 * (1.0 - dx) + self.points[kx2] as f32 * dx - upper) / 127.0;
        10f32.powf(y * self.max_db as f32 / 20.0)
    }

    /// `applyres` on bins `1..n`; `ctlcenter`/`ctlbw` come from MIDI.
    pub fn applyres(&self, n: usize, fftdata: &mut [C32], freq: f32, ctlcenter: f32, ctlbw: f32) {
        if !self.enabled {
            return;
        }
        let l1 = (self.getfreqx(0.0) * ctlcenter).ln();
        let l2 = 2f32.ln() * self.getoctavesfreq() * ctlbw;
        let upper = self.upper();
        for i in 1..n {
            let x = ((freq * i as f32).ln() - l1) / l2;
            let x = x.max(0.0) * N_RES_POINTS as f32;
            let mut y = self.lookup(x, upper);
            if self.protect_fundamental && i == 1 {
                y = 1.0;
            }
            fftdata[i] *= y;
        }
    }

    pub fn getfreqresponse(&self, freq: f32, ctlcenter: f32, ctlbw: f32) -> f32 {
        let l1 = (self.getfreqx(0.0) * ctlcenter).ln();
        let l2 = 2f32.ln() * self.getoctavesfreq() * ctlbw;
        let x = ((freq.ln() - l1) / l2).max(0.0) * N_RES_POINTS as f32;
        self.lookup(x, self.upper())
    }

    pub fn smooth(&mut self) {
        let mut old = self.points[0] as f32;
        for i in 0..N_RES_POINTS {
            old = old * 0.4 + self.points[i] as f32 * 0.6;
            self.points[i] = old as u8;
        }
        old = self.points[N_RES_POINTS - 1] as f32;
        for i in (1..N_RES_POINTS).rev() {
            old = old * 0.4 + self.points[i] as f32 * 0.6;
            self.points[i] = ((old as i32) + 1).min(127) as u8;
        }
    }

    pub fn randomize(&mut self, kind: i32) {
        let mut r = (rnd() * 127.0) as u8;
        for i in 0..N_RES_POINTS {
            self.points[i] = r;
            if rnd() < 0.1 && kind == 0 {
                r = (rnd() * 127.0) as u8;
            }
            if rnd() < 0.3 && kind == 1 {
                r = (rnd() * 127.0) as u8;
            }
            if kind == 2 {
                r = (rnd() * 127.0) as u8;
            }
        }
        self.smooth();
    }

    pub fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_parbool("enabled", self.enabled);
        x.add_par("max_db", self.max_db as i32);
        x.add_par("center_freq", self.centerfreq as i32);
        x.add_par("octaves_freq", self.octavesfreq as i32);
        x.add_parbool("protect_fundamental_frequency", self.protect_fundamental);
        x.add_par("resonance_points", N_RES_POINTS as i32);
        for (i, p) in self.points.iter().enumerate() {
            x.child_id("RESPOINT", i).add_par("val", *p as i32);
        }
    }

    pub fn get_from_xml(&mut self, x: &XmlNode) {
        self.enabled = x.parbool("enabled", self.enabled);
        self.max_db = x.par127("max_db", self.max_db);
        self.centerfreq = x.par127("center_freq", self.centerfreq);
        self.octavesfreq = x.par127("octaves_freq", self.octavesfreq);
        self.protect_fundamental = x.parbool("protect_fundamental_frequency", self.protect_fundamental);
        for i in 0..N_RES_POINTS {
            if let Some(n) = x.branch_id("RESPOINT", i) {
                self.points[i] = n.par127("val", self.points[i]);
            }
        }
    }
}

//! `Params/EnvelopeParams.cpp` + `Synth/Envelope.cpp`.

use super::util::*;
use super::xml::{Version, XmlNode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvMode {
    AdsrLin = 1,
    AdsrDb = 2,
    AsrFreqLfo = 3,
    AdsrFilter = 4,
    AsrBw = 5,
}

/// Consumer locations (`Params/Presets.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loc {
    AdGlobalAmp,
    AdGlobalFreq,
    AdGlobalFilter,
    AdVoiceAmp,
    AdVoiceFreq,
    AdVoiceFilter,
    AdVoiceFmAmp,
    AdVoiceFmFreq,
    SubFreq,
    SubFilter,
    SubBandwidth,
    InEffect,
    Unspecified,
    DynFilter(u8),
}

pub fn dt_real(v: f32) -> f32 {
    (2f32.powf(v / 127.0 * 12.0) - 1.0) / 100.0
}

pub fn dt_127(v: f32) -> i32 {
    (((v * 100.0 + 1.0).log2() * 127.0 / 12.0).round() as i32).clamp(0, 127)
}

#[derive(Clone, Debug)]
pub struct EnvelopeParams {
    pub mode: EnvMode,
    pub free_mode: bool,
    pub points: u8,
    pub sustain: u8,
    pub dt: [f32; MAX_ENVELOPE_POINTS],
    pub val: [u8; MAX_ENVELOPE_POINTS],
    pub stretch: u8,
    pub forced_release: bool,
    pub linear: bool,
    pub repeating: bool,
    pub a_dt: f32,
    pub d_dt: f32,
    pub r_dt: f32,
    pub a_val: u8,
    pub d_val: u8,
    pub s_val: u8,
    pub r_val: u8,
}

impl EnvelopeParams {
    fn base(stretch: u8, forced_release: bool) -> Self {
        let mut dt = [dt_real(32.0); MAX_ENVELOPE_POINTS];
        dt[0] = 0.0;
        Self {
            mode: EnvMode::AdsrLin,
            free_mode: true,
            points: 1,
            sustain: 1,
            dt,
            val: [64; MAX_ENVELOPE_POINTS],
            stretch,
            forced_release,
            linear: false,
            repeating: false,
            a_dt: 0.009,
            d_dt: 0.009,
            r_dt: 0.009,
            a_val: 64,
            d_val: 64,
            s_val: 64,
            r_val: 64,
        }
    }

    /// `EnvelopeParams(stretch, forcedrelease)` followed by `init(loc)`.
    pub fn new(stretch: u8, forced_release: bool, loc: Loc) -> Self {
        let mut e = Self::base(stretch, forced_release);
        match loc {
            Loc::AdGlobalAmp => e.adsr_init_db(0.0, 0.127, 127, 0.041),
            Loc::AdGlobalFreq => e.asr_init(64, 0.254, 64, 0.499),
            Loc::AdGlobalFilter | Loc::SubFilter => e.adsr_init_filter(64, 0.127, 64, 0.970, 0.499, 64),
            Loc::AdVoiceAmp => e.adsr_init_db(0.0, 6.978, 127, 6.978),
            Loc::AdVoiceFreq => e.asr_init(30, 0.127, 64, 0.499),
            Loc::AdVoiceFilter => e.adsr_init_filter(90, 0.970, 40, 0.970, 0.009, 40),
            Loc::AdVoiceFmFreq => e.asr_init(20, 3.620, 40, 1.876),
            Loc::AdVoiceFmAmp => e.adsr_init(1.876, 3.620, 127, 6.978),
            Loc::SubFreq => e.asr_init(30, 0.254, 64, 0.499),
            Loc::SubBandwidth => e.asr_init_bw(100, 0.970, 64, 0.499),
            _ => {}
        }
        e
    }

    fn adsr_init(&mut self, a: f32, d: f32, s: u8, r: f32) {
        self.mode = EnvMode::AdsrLin;
        self.a_dt = a;
        self.d_dt = d;
        self.s_val = s;
        self.r_dt = r;
        self.free_mode = false;
        self.convert_to_free();
    }
    fn adsr_init_db(&mut self, a: f32, d: f32, s: u8, r: f32) {
        self.adsr_init(a, d, s, r);
        self.mode = EnvMode::AdsrDb;
        self.convert_to_free();
    }
    fn asr_init(&mut self, a_val: u8, a: f32, r_val: u8, r: f32) {
        self.mode = EnvMode::AsrFreqLfo;
        self.a_val = a_val;
        self.a_dt = a;
        self.r_val = r_val;
        self.r_dt = r;
        self.free_mode = false;
        self.convert_to_free();
    }
    fn adsr_init_filter(&mut self, a_val: u8, a: f32, d_val: u8, d: f32, r: f32, r_val: u8) {
        self.mode = EnvMode::AdsrFilter;
        self.a_val = a_val;
        self.a_dt = a;
        self.d_val = d_val;
        self.d_dt = d;
        self.r_dt = r;
        self.r_val = r_val;
        self.free_mode = false;
        self.convert_to_free();
    }
    fn asr_init_bw(&mut self, a_val: u8, a: f32, r_val: u8, r: f32) {
        self.asr_init(a_val, a, r_val, r);
        self.mode = EnvMode::AsrBw;
    }

    pub fn convert_to_free(&mut self) {
        match self.mode {
            EnvMode::AdsrLin | EnvMode::AdsrDb => {
                self.points = 4;
                self.sustain = 2;
                self.val[0] = 0;
                self.dt[1] = self.a_dt;
                self.val[1] = 127;
                self.dt[2] = self.d_dt;
                self.val[2] = self.s_val;
                self.dt[3] = self.r_dt;
                self.val[3] = 0;
            }
            EnvMode::AsrFreqLfo | EnvMode::AsrBw => {
                self.points = 3;
                self.sustain = 1;
                self.val[0] = self.a_val;
                self.dt[1] = self.a_dt;
                self.val[1] = 64;
                self.dt[2] = self.r_dt;
                self.val[2] = self.r_val;
            }
            EnvMode::AdsrFilter => {
                self.points = 4;
                self.sustain = 2;
                self.val[0] = self.a_val;
                self.dt[1] = self.a_dt;
                self.val[1] = self.d_val;
                self.dt[2] = self.d_dt;
                self.val[2] = 64;
                self.dt[3] = self.r_dt;
                self.val[3] = self.r_val;
            }
        }
    }

    pub fn env_db2rap(db: f32) -> f32 {
        ((10f32.powf(db / 20.0) as f64 - 0.01).max(0.0) / 0.99) as f32
    }

    pub fn env_rap2db(rap: f32) -> f32 {
        // `rap * 0.99f + 0.01`: the sum is done in double.
        20.0 * (((rap * 0.99) as f64 + 0.01) as f32).log10()
    }

    pub fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_parbool("free_mode", self.free_mode);
        x.add_par("env_points", self.points as i32);
        x.add_par("env_sustain", self.sustain as i32);
        x.add_par("env_stretch", self.stretch as i32);
        x.add_parbool("forced_release", self.forced_release);
        x.add_parbool("linear_envelope", self.linear);
        x.add_parbool("repeating_envelope", self.repeating);
        x.add_parreal("A_dt", self.a_dt);
        x.add_parreal("D_dt", self.d_dt);
        x.add_parreal("R_dt", self.r_dt);
        x.add_par("A_val", self.a_val as i32);
        x.add_par("D_val", self.d_val as i32);
        x.add_par("S_val", self.s_val as i32);
        x.add_par("R_val", self.r_val as i32);
        for i in 0..self.points as usize {
            let p = x.child_id("POINT", i);
            if i != 0 {
                p.add_parreal("dt", self.dt[i]);
            }
            p.add_par("val", self.val[i] as i32);
        }
    }

    pub fn get_from_xml(&mut self, x: &XmlNode, ver: Version) {
        self.free_mode = x.parbool("free_mode", self.free_mode);
        self.points = x.par127("env_points", self.points);
        self.sustain = x.par127("env_sustain", self.sustain);
        self.stretch = x.par127("env_stretch", self.stretch);
        self.forced_release = x.parbool("forced_release", self.forced_release);
        self.linear = x.parbool("linear_envelope", self.linear);
        self.repeating = x.parbool("repeating_envelope", self.repeating);
        let mismatch = ver < Version(2, 4, 4) && self.mode == EnvMode::AdsrDb;
        let fix = |input: u8| -> u8 {
            if mismatch {
                (127.0 * (0.5 * (0.01 + 0.99 * 100f32.powf(input as f32 / 127.0 - 1.0)).log10() + 1.0)).round() as u8
            } else {
                input
            }
        };
        if !x.has_parreal("A_dt") {
            self.a_dt = dt_real(x.par127("A_dt", 0) as f32);
            self.d_dt = dt_real(x.par127("D_dt", 0) as f32);
            self.r_dt = dt_real(x.par127("R_dt", 0) as f32);
        } else {
            self.a_dt = x.parreal("A_dt", self.a_dt);
            self.d_dt = x.parreal("D_dt", self.d_dt);
            self.r_dt = x.parreal("R_dt", self.r_dt);
        }
        self.a_val = fix(x.par127("A_val", self.a_val));
        self.d_val = fix(x.par127("D_val", self.d_val));
        self.s_val = fix(x.par127("S_val", self.s_val));
        self.r_val = fix(x.par127("R_val", self.r_val));
        for i in 0..(self.points as usize).min(MAX_ENVELOPE_POINTS) {
            let Some(p) = x.branch_id("POINT", i) else { continue };
            if i != 0 {
                if !p.has_parreal("dt") {
                    let d = p.par127("dt", dt_127(self.dt[i]) as u8);
                    self.dt[i] = dt_real(d as f32);
                } else {
                    self.dt[i] = p.parreal("dt", self.dt[i]);
                }
            }
            self.val[i] = fix(p.par127("val", self.val[i]));
        }
        if !self.free_mode {
            self.convert_to_free();
        }
    }
}

/// Running envelope (`Synth/Envelope.cpp`).
#[derive(Clone, Debug)]
pub struct Envelope {
    envpoints: usize,
    envsustain: i32,
    envdt: [f32; MAX_ENVELOPE_POINTS],
    envval: [f32; MAX_ENVELOPE_POINTS],
    envstretch: f32,
    linearenvelope: bool,
    forcedrelease: bool,
    repeating: bool,
    mode: EnvMode,
    currentpoint: usize,
    keyreleased: bool,
    envfinish: bool,
    t: f32,
    inct: f32,
    envoutval: f32,
}

impl Envelope {
    pub fn new(pars: &EnvelopeParams, basefreq: f32, bufferdt: f32) -> Self {
        let mut pars_free;
        let pars = if !pars.free_mode {
            pars_free = pars.clone();
            pars_free.convert_to_free();
            &pars_free
        } else {
            pars
        };
        let envpoints = (pars.points as usize).min(MAX_ENVELOPE_POINTS);
        let envsustain = if pars.sustain == 0 { -1 } else { pars.sustain as i32 };
        let envstretch = (440.0 / basefreq).powf(pars.stretch as f32 / 64.0);
        let linearenvelope = pars.linear;
        let mut mode = pars.mode;
        if mode == EnvMode::AdsrLin && !linearenvelope {
            mode = EnvMode::AdsrDb;
        }
        if mode == EnvMode::AdsrDb && linearenvelope {
            mode = EnvMode::AdsrLin;
        }
        let mut envdt = [0.0; MAX_ENVELOPE_POINTS];
        let mut envval = [0.0; MAX_ENVELOPE_POINTS];
        for i in 0..MAX_ENVELOPE_POINTS {
            let dtstretched = pars.dt[i] * envstretch;
            envdt[i] = if dtstretched > bufferdt { bufferdt / dtstretched } else { 2.0 };
            let v = pars.val[i] as f32;
            envval[i] = match mode {
                EnvMode::AdsrDb => (1.0 - v / 127.0) * -40.0,
                EnvMode::AsrFreqLfo => {
                    let x = (2f32.powf(6.0 * (v - 64.0).abs() / 64.0) - 1.0) * 100.0;
                    if pars.val[i] < 64 { -x } else { x }
                }
                EnvMode::AdsrFilter => (v - 64.0) / 64.0 * 6.0,
                EnvMode::AsrBw => (v - 64.0) / 64.0 * 10.0,
                EnvMode::AdsrLin => v / 127.0,
            };
        }
        envdt[0] = 1.0;
        Self {
            envpoints,
            envsustain,
            envdt,
            envval,
            envstretch,
            linearenvelope,
            forcedrelease: pars.forced_release,
            repeating: pars.repeating,
            mode,
            currentpoint: 1,
            keyreleased: false,
            envfinish: false,
            t: 0.0,
            inct: envdt[1],
            envoutval: 0.0,
        }
    }

    pub fn release_key(&mut self) {
        if self.keyreleased {
            return;
        }
        self.keyreleased = true;
        if self.forcedrelease {
            self.t = 0.0;
        }
    }

    pub fn force_finish(&mut self) {
        self.envfinish = true;
    }

    pub fn finished(&self) -> bool {
        self.envfinish
    }

    pub fn envout(&mut self) -> f32 {
        if self.envfinish {
            self.envoutval = self.envval[self.envpoints.max(1) - 1];
            return self.envoutval;
        }
        let sus = self.envsustain;
        if sus >= 0 && self.currentpoint == (sus + 1) as usize && !self.keyreleased {
            let s = sus as usize;
            self.envoutval = self.envval[s];
            let zerorelease = (s..self.envpoints).all(|i| self.envval[i] == -40.0);
            if zerorelease && matches!(self.mode, EnvMode::AdsrLin | EnvMode::AdsrDb) {
                self.envfinish = true;
            }
            return self.envoutval;
        }
        if self.keyreleased && self.forcedrelease {
            let releaseindex = if sus < 0 { self.envpoints - 1 } else { (sus + 1) as usize };
            let releaseindex = releaseindex.min(MAX_ENVELOPE_POINTS - 1);
            let out = if self.envdt[releaseindex] < 0.000_000_01 {
                self.envval[releaseindex]
            } else {
                self.envoutval + (self.envval[releaseindex] - self.envoutval) * self.t
            };
            self.t += self.envdt[releaseindex];
            if self.t >= 1.0 {
                self.currentpoint = (sus + 2).max(0) as usize;
                self.forcedrelease = false;
                self.t = 0.0;
                self.inct = self.envdt[self.currentpoint.min(MAX_ENVELOPE_POINTS - 1)];
                if self.currentpoint >= self.envpoints || sus < 0 {
                    self.envfinish = true;
                }
            }
            return out;
        }
        let cp = self.currentpoint.min(MAX_ENVELOPE_POINTS - 1);
        let out = if self.inct >= 1.0 {
            self.envval[cp]
        } else {
            self.envval[cp - 1] + (self.envval[cp] - self.envval[cp - 1]) * self.t
        };
        self.t += self.inct;
        if self.t >= 1.0 {
            if self.currentpoint + 1 >= self.envpoints {
                self.envfinish = true;
            } else if self.repeating && sus >= 0 && self.currentpoint == sus as usize && !self.keyreleased {
                self.envval[0] = self.envval[self.currentpoint];
                self.currentpoint = 1;
            } else {
                self.currentpoint += 1;
            }
            self.t = 0.0;
            self.inct = self.envdt[self.currentpoint.min(MAX_ENVELOPE_POINTS - 1)];
        }
        self.envoutval = out;
        out
    }

    pub fn envout_db(&mut self) -> f32 {
        if self.linearenvelope {
            return self.envout();
        }
        let out;
        if self.currentpoint == 1 && (!self.keyreleased || !self.forcedrelease) {
            let v1 = EnvelopeParams::env_db2rap(self.envval[0]);
            let v2 = EnvelopeParams::env_db2rap(self.envval[1]);
            let mut o = v1 + (v2 - v1) * self.t;
            self.t += self.inct;
            if self.t >= 1.0 {
                self.t = 0.0;
                self.inct = self.envdt[2];
                self.currentpoint += 1;
                o = v2;
            }
            self.envoutval = EnvelopeParams::env_rap2db(o);
            out = self.envoutval;
        } else {
            out = self.envout();
        }
        EnvelopeParams::env_db2rap(out)
    }

    #[allow(dead_code)]
    pub fn stretch(&self) -> f32 {
        self.envstretch
    }
}

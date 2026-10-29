//! Effects: `Effects/{EffectMgr,EffectLFO,Reverb,Echo,Chorus,Phaser,
//! Alienwah,Distortion,EQ,DynamicFilter}.cpp` and `DSP/Unison.cpp`.
//!
//! Every effect keeps Zyn's 0..127 parameter table (`changepar` /
//! `getpar`) and its factory presets, so `.xiz`/`.xmz` effect sections
//! load unchanged.

use super::envelope::Loc;
use super::filter::{AnalogFilter, Filter, FilterParams, basefreq_from_old, baseq_from_old};
use super::util::*;
use super::waveshape::wave_shape_smps;
use super::xml::{Version, XmlNode};

pub const EFFECT_NAMES: [&str; 11] =
    ["None", "Reverb", "Echo", "Chorus", "Phaser", "AlienWah", "Distortion", "EQ", "DynFilter", "Sympathetic", "Reverse"];

/// Parameters every effect shares (`Effect` base class).
#[derive(Clone, Debug)]
struct Base {
    insertion: bool,
    sr: f32,
    bs: usize,
    ppanning: u8,
    pangain_l: f32,
    pangain_r: f32,
    plrcross: u8,
    lrcross: f32,
    volume: f32,
    outvolume: f32,
}

impl Base {
    fn new(insertion: bool, sr: f32, bs: usize) -> Self {
        let mut b = Self {
            insertion,
            sr,
            bs,
            ppanning: 64,
            pangain_l: 0.0,
            pangain_r: 0.0,
            plrcross: 0,
            lrcross: 0.0,
            volume: 1.0,
            outvolume: 1.0,
        };
        b.setpanning(64);
        b
    }
    fn setpanning(&mut self, p: u8) {
        self.ppanning = p;
        let t = if p > 0 { (p as f32 - 1.0) / 126.0 } else { 0.0 };
        self.pangain_l = (t * PI / 2.0).cos();
        self.pangain_r = ((1.0 - t) * PI / 2.0).cos();
    }
    fn setlrcross(&mut self, p: u8) {
        self.plrcross = p;
        self.lrcross = p as f32 / 127.0;
    }
}

// ---------------------------------------------------------------------------
// EffectLFO
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct EffectLfo {
    pfreq: u8,
    prandomness: u8,
    plfotype: u8,
    pstereo: u8,
    xl: f32,
    xr: f32,
    incx: f32,
    ampl1: f32,
    ampl2: f32,
    ampr1: f32,
    ampr2: f32,
    lfornd: f32,
    lfotype: u8,
    sr: f32,
    bs: f32,
}

impl EffectLfo {
    fn new(sr: f32, bs: usize) -> Self {
        let mut l = Self {
            pfreq: 40,
            prandomness: 0,
            plfotype: 0,
            pstereo: 64,
            xl: 0.0,
            xr: 0.0,
            incx: 0.0,
            ampl1: rnd(),
            ampl2: rnd(),
            ampr1: rnd(),
            ampr2: rnd(),
            lfornd: 0.0,
            lfotype: 0,
            sr,
            bs: bs as f32,
        };
        l.updateparams();
        l
    }
    fn updateparams(&mut self) {
        let f = (2f32.powf(self.pfreq as f32 / 127.0 * 10.0) - 1.0) * 0.03;
        self.incx = (f.abs() * self.bs / self.sr).min(0.499_999_999);
        self.lfornd = (self.prandomness as f32 / 127.0).min(1.0);
        if self.plfotype > 1 {
            self.plfotype = 1;
        }
        self.lfotype = self.plfotype;
        self.xr = self.xl + (self.pstereo as f32 - 64.0) / 127.0 + 1.0;
        self.xr -= self.xr.floor();
    }
    fn shape(&self, x: f32) -> f32 {
        let x = x % 1.0;
        match self.lfotype {
            1 => {
                if x > 0.0 && x < 0.25 {
                    4.0 * x
                } else if x > 0.25 && x < 0.75 {
                    2.0 - 4.0 * x
                } else {
                    4.0 * x - 4.0
                }
            }
            _ => (x * 2.0 * PI).cos(),
        }
    }
    fn out(&mut self, phase_offset: f32) -> (f32, f32) {
        let mut o = self.shape(self.xl + phase_offset);
        o *= self.ampl1 + self.xl * (self.ampl2 - self.ampl1);
        let l = (o + 1.0) * 0.5;
        if phase_offset == 0.0 {
            self.xl += self.incx;
            if self.xl > 1.0 {
                self.xl -= 1.0;
                self.ampl1 = self.ampl2;
                self.ampl2 = (1.0 - self.lfornd) + self.lfornd * rnd();
            }
        }
        let mut o = self.shape(self.xr + phase_offset);
        o *= self.ampr1 + self.xr * (self.ampr2 - self.ampr1);
        let r = (o + 1.0) * 0.5;
        if phase_offset == 0.0 {
            self.xr += self.incx;
            if self.xr > 1.0 {
                self.xr -= 1.0;
                self.ampr1 = self.ampr2;
                self.ampr2 = (1.0 - self.lfornd) + self.lfornd * rnd();
            }
        }
        (l, r)
    }
    fn set(&mut self, npar: i32, v: u8) {
        match npar {
            2 => self.pfreq = v,
            3 => self.prandomness = v,
            4 => self.plfotype = v,
            5 => self.pstereo = v,
            _ => {}
        }
        self.updateparams();
    }
    fn get(&self, npar: i32) -> u8 {
        match npar {
            2 => self.pfreq,
            3 => self.prandomness,
            4 => self.plfotype,
            5 => self.pstereo,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Unison (for the reverb "bandwidth" type)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
struct UVoice {
    step: f32,
    position: f32,
    realpos1: f32,
    realpos2: f32,
    relative_amplitude: f32,
}

#[derive(Clone, Debug)]
struct Unison {
    uv: Vec<UVoice>,
    base_freq: f32,
    update_period: i32,
    update_k: i32,
    max_delay: i32,
    delay_k: i32,
    first_time: bool,
    delay_buffer: Vec<f32>,
    amplitude_samples: f32,
    bandwidth_cents: f32,
    sr: f32,
}

impl Unison {
    fn new(update_period: i32, max_delay_sec: f32, sr: f32) -> Self {
        let max_delay = ((sr * max_delay_sec) as i32 + 1).max(10);
        let mut u = Self {
            uv: Vec::new(),
            base_freq: 1.0,
            update_period,
            update_k: 0,
            max_delay,
            delay_k: 0,
            first_time: false,
            delay_buffer: vec![0.0; max_delay as usize],
            amplitude_samples: 0.0,
            bandwidth_cents: 10.0,
            sr,
        };
        u.set_size(1);
        u
    }
    fn set_size(&mut self, n: usize) {
        let n = n.max(1);
        self.uv = (0..n)
            .map(|_| UVoice { position: rnd() * 1.8 - 0.9, relative_amplitude: 1.0, ..Default::default() })
            .collect();
        self.first_time = true;
        self.update_parameters();
    }
    fn set_base_frequency(&mut self, f: f32) {
        self.base_freq = f;
        self.update_parameters();
    }
    fn set_bandwidth(&mut self, bw: f32) {
        self.bandwidth_cents = bw.clamp(0.0, 1200.0);
        self.update_parameters();
    }
    fn update_parameters(&mut self) {
        let ips = self.sr / self.update_period as f32;
        for v in self.uv.iter_mut() {
            let base = 2f32.powf(rnd() * 2.0 - 1.0);
            v.relative_amplitude = base;
            let period = base / self.base_freq;
            let mut m = 4.0 / (period * ips);
            if rnd() < 0.5 {
                m = -m;
            }
            v.step = m;
        }
        let max_speed = 2f32.powf(self.bandwidth_cents / 1200.0);
        self.amplitude_samples = 0.125 * (max_speed - 1.0) * self.sr / self.base_freq;
        if self.amplitude_samples >= (self.max_delay - 1) as f32 {
            self.amplitude_samples = (self.max_delay - 2) as f32;
        }
        self.update_data();
    }
    fn update_data(&mut self) {
        for v in self.uv.iter_mut() {
            let mut pos = v.position + v.step;
            let mut step = v.step;
            if pos <= -1.0 {
                pos = -1.0;
                step = -step;
            } else if pos >= 1.0 {
                pos = 1.0;
                step = -step;
            }
            let vib = (pos - 0.333_333_34 * pos * pos * pos) * 1.5;
            let nv = 1.0 + 0.5 * (vib + 1.0) * self.amplitude_samples * v.relative_amplitude;
            if self.first_time {
                v.realpos1 = nv;
                v.realpos2 = nv;
            } else {
                v.realpos1 = v.realpos2;
                v.realpos2 = nv;
            }
            v.position = pos;
            v.step = step;
        }
        self.first_time = false;
    }
    fn process(&mut self, buf: &mut [f32]) {
        let volume = 1.0 / (self.uv.len() as f32).sqrt();
        let xstep = 1.0 / self.update_period as f32;
        let mut xpos = self.update_k as f32 * xstep;
        let md = self.max_delay;
        for s in buf.iter_mut() {
            let k = self.update_k;
            self.update_k += 1;
            if k >= self.update_period {
                self.update_data();
                self.update_k = 0;
                xpos = 0.0;
            }
            xpos += xstep;
            let input = *s;
            let mut out = 0.0;
            let mut sign = 1.0;
            for v in self.uv.iter() {
                let vpos = v.realpos1 * (1.0 - xpos) + v.realpos2 * xpos;
                let pos = (self.delay_k + md) as f32 - vpos - 1.0;
                let mut posi = f2i(pos);
                let mut posn = posi + 1;
                if posi >= md {
                    posi -= md;
                }
                if posn >= md {
                    posn -= md;
                }
                let posi = posi.rem_euclid(md) as usize;
                let posn = posn.rem_euclid(md) as usize;
                let pf = pos - pos.floor();
                out += ((1.0 - pf) * self.delay_buffer[posi] + pf * self.delay_buffer[posn]) * sign;
                sign = -sign;
            }
            *s = out * volume;
            self.delay_buffer[self.delay_k as usize] = input;
            self.delay_k += 1;
            if self.delay_k >= md {
                self.delay_k = 0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reverb
// ---------------------------------------------------------------------------

const REV_COMBS: usize = 8;
const REV_APS: usize = 4;

#[derive(Clone, Debug)]
struct Reverb {
    b: Base,
    pvolume: u8,
    ptime: u8,
    pidelay: u8,
    pidelayfb: u8,
    plpf: u8,
    phpf: u8,
    plohidamp: u8,
    ptype: u8,
    proomsize: u8,
    pbandwidth: u8,
    lohifb: f32,
    idelayfb: f32,
    roomsize: f32,
    rs: f32,
    comblen: [usize; REV_COMBS * 2],
    aplen: [usize; REV_APS * 2],
    comb: Vec<Vec<f32>>,
    combk: [usize; REV_COMBS * 2],
    combfb: [f32; REV_COMBS * 2],
    lpcomb: [f32; REV_COMBS * 2],
    ap: Vec<Vec<f32>>,
    apk: [usize; REV_APS * 2],
    idelay: Vec<f32>,
    idelayk: usize,
    lpf: Option<AnalogFilter>,
    hpf: Option<AnalogFilter>,
    bandwidth: Option<Box<Unison>>,
}

impl Reverb {
    const PRESETS: [[u8; 13]; 13] = [
        [80, 64, 63, 24, 0, 0, 0, 85, 5, 83, 1, 64, 20],
        [80, 64, 69, 35, 0, 0, 0, 127, 0, 71, 0, 64, 20],
        [80, 64, 69, 24, 0, 0, 0, 127, 75, 78, 1, 85, 20],
        [90, 64, 51, 10, 0, 0, 0, 127, 21, 78, 1, 64, 20],
        [90, 64, 53, 20, 0, 0, 0, 127, 75, 71, 1, 64, 20],
        [100, 64, 33, 0, 0, 0, 0, 127, 0, 106, 0, 30, 20],
        [100, 64, 21, 26, 0, 0, 0, 62, 0, 77, 1, 45, 20],
        [110, 64, 14, 0, 0, 0, 0, 127, 5, 71, 0, 25, 20],
        [85, 80, 84, 20, 42, 0, 0, 51, 0, 78, 1, 105, 20],
        [95, 64, 26, 60, 71, 0, 0, 114, 0, 64, 1, 64, 20],
        [90, 64, 40, 88, 71, 0, 0, 114, 0, 88, 1, 64, 20],
        [90, 64, 93, 15, 0, 0, 0, 114, 0, 77, 0, 95, 20],
        [90, 64, 111, 30, 0, 0, 0, 114, 90, 74, 1, 80, 20],
    ];

    fn new(b: Base) -> Self {
        let mut comblen = [0; REV_COMBS * 2];
        for c in comblen.iter_mut() {
            *c = 800 + (rnd() * 1400.0) as usize;
        }
        let mut aplen = [0; REV_APS * 2];
        for a in aplen.iter_mut() {
            *a = 500 + (rnd() * 500.0) as usize;
        }
        let mut r = Self {
            b,
            pvolume: 48,
            ptime: 64,
            pidelay: 40,
            pidelayfb: 0,
            plpf: 127,
            phpf: 0,
            plohidamp: 80,
            ptype: 1,
            proomsize: 64,
            pbandwidth: 30,
            lohifb: 0.0,
            idelayfb: 0.0,
            roomsize: 1.0,
            rs: 1.0,
            comblen,
            aplen,
            comb: vec![Vec::new(); REV_COMBS * 2],
            combk: [0; REV_COMBS * 2],
            combfb: [-0.97; REV_COMBS * 2],
            lpcomb: [0.0; REV_COMBS * 2],
            ap: vec![Vec::new(); REV_APS * 2],
            apk: [0; REV_APS * 2],
            idelay: Vec::new(),
            idelayk: 0,
            lpf: None,
            hpf: None,
            bandwidth: None,
        };
        for i in 0..REV_COMBS * 2 {
            r.comb[i] = vec![0.0; r.comblen[i]];
        }
        for i in 0..REV_APS * 2 {
            r.ap[i] = vec![0.0; r.aplen[i]];
        }
        r
    }

    fn presetpar(&self, np: u8, n: usize) -> u8 {
        if (np as usize) < 13 && n < 13 {
            let v = Self::PRESETS[np as usize][n];
            if n == 0 && self.b.insertion {
                return v / 2;
            }
            return v;
        }
        0
    }

    fn cleanup(&mut self) {
        for i in 0..REV_COMBS * 2 {
            self.lpcomb[i] = 0.0;
            self.comb[i].fill(0.0);
        }
        for a in self.ap.iter_mut() {
            a.fill(0.0);
        }
        self.idelay.fill(0.0);
        if let Some(h) = self.hpf.as_mut() {
            h.cleanup();
        }
        if let Some(l) = self.lpf.as_mut() {
            l.cleanup();
        }
    }

    fn processmono(&mut self, ch: usize, out: &mut [f32], inbuf: &[f32]) {
        let n = self.b.bs;
        for j in REV_COMBS * ch..REV_COMBS * (ch + 1) {
            let len = self.comblen[j];
            let mut ck = self.combk[j];
            let mut lp = self.lpcomb[j];
            let fbk = self.combfb[j];
            let c = &mut self.comb[j];
            for i in 0..n {
                let mut fbout = c[ck] * fbk;
                fbout = fbout * (1.0 - self.lohifb) + lp * self.lohifb;
                lp = fbout;
                c[ck] = inbuf[i] + fbout;
                out[i] += fbout;
                ck += 1;
                if ck >= len {
                    ck = 0;
                }
            }
            self.combk[j] = ck;
            self.lpcomb[j] = lp;
        }
        for j in REV_APS * ch..REV_APS * (ch + 1) {
            let len = self.aplen[j];
            let mut ak = self.apk[j];
            let a = &mut self.ap[j];
            for o in out.iter_mut().take(n) {
                let tmp = a[ak];
                a[ak] = 0.7 * tmp + *o;
                *o = tmp - 0.7 * a[ak];
                ak += 1;
                if ak >= len {
                    ak = 0;
                }
            }
            self.apk[j] = ak;
        }
    }

    fn out(&mut self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32]) {
        if self.pvolume == 0 && self.b.insertion {
            return;
        }
        let n = self.b.bs;
        let mut inbuf = [0.0f32; super::filter::MAX_BUF];
        for i in 0..n {
            inbuf[i] = (l[i] + r[i]) / 2.0;
        }
        if self.idelay.len() > 1 {
            let len = self.idelay.len();
            for s in inbuf.iter_mut().take(n) {
                let tmp = *s + self.idelay[self.idelayk] * self.idelayfb;
                *s = self.idelay[self.idelayk];
                self.idelay[self.idelayk] = tmp;
                self.idelayk += 1;
                if self.idelayk >= len {
                    self.idelayk = 0;
                }
            }
        }
        if let Some(u) = self.bandwidth.as_mut() {
            u.process(&mut inbuf[..n]);
        }
        if let Some(f) = self.lpf.as_mut() {
            f.filterout(&mut inbuf[..n]);
        }
        if let Some(f) = self.hpf.as_mut() {
            f.filterout(&mut inbuf[..n]);
        }
        self.processmono(0, ol, &inbuf[..n]);
        self.processmono(1, or, &inbuf[..n]);
        let mut lvol = self.rs / REV_COMBS as f32 * self.b.pangain_l;
        let mut rvol = self.rs / REV_COMBS as f32 * self.b.pangain_r;
        if self.b.insertion {
            lvol *= 2.0;
            rvol *= 2.0;
        }
        for i in 0..n {
            ol[i] *= lvol;
            or[i] *= rvol;
        }
    }

    fn setvolume(&mut self, v: u8) {
        self.pvolume = v;
        if !self.b.insertion {
            self.b.outvolume = if v == 0 { 0.0 } else { 0.01f32.powf(1.0 - v as f32 / 127.0) * 4.0 };
            self.b.volume = 1.0;
        } else {
            self.b.volume = v as f32 / 127.0;
            self.b.outvolume = self.b.volume;
            if v == 0 {
                self.cleanup();
            }
        }
    }

    fn settime(&mut self, v: u8) {
        self.ptime = v;
        let t = 60f32.powf(v as f32 / 127.0) - 0.97;
        for i in 0..REV_COMBS * 2 {
            self.combfb[i] = -(self.comblen[i] as f32 / self.b.sr * 0.001f32.ln() / t).exp();
        }
    }

    fn setlohidamp(&mut self, v: u8) {
        self.plohidamp = v.max(64);
        if self.plohidamp == 64 {
            self.lohifb = 0.0;
        } else {
            let x = ((self.plohidamp as f32 - 64.0) / 64.1).abs();
            self.lohifb = x * x;
        }
    }

    fn setidelay(&mut self, v: u8) {
        self.pidelay = v;
        let delay = (50.0 * v as f32 / 127.0).powi(2) - 1.0;
        let nl = (self.b.sr * delay / 1000.0) as i32;
        if nl == self.idelay.len() as i32 {
            return;
        }
        self.idelay.clear();
        if nl > 1 {
            self.idelayk = 0;
            self.idelay = vec![0.0; nl as usize];
        }
    }

    fn sethpf(&mut self, v: u8) {
        self.phpf = v;
        if v == 0 {
            self.hpf = None;
        } else {
            let fr = ((v as f32 / 127.0).sqrt() * 10000f32.ln()).exp() + 20.0;
            match self.hpf.as_mut() {
                Some(h) => h.setfreq(fr),
                None => self.hpf = Some(AnalogFilter::new(3, fr, 1.0, 0, self.b.sr, self.b.bs)),
            }
        }
    }

    fn setlpf(&mut self, v: u8) {
        self.plpf = v;
        if v == 127 {
            self.lpf = None;
        } else {
            let fr = ((v as f32 / 127.0).sqrt() * 25000f32.ln()).exp() + 40.0;
            match self.lpf.as_mut() {
                Some(l) => l.setfreq(fr),
                None => self.lpf = Some(AnalogFilter::new(2, fr, 1.0, 0, self.b.sr, self.b.bs)),
            }
        }
    }

    fn settype(&mut self, v: u8) {
        const COMBT: [usize; REV_COMBS] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        const APT: [usize; REV_APS] = [225, 341, 441, 556];
        self.ptype = v.min(2);
        let adj = self.b.sr / 44100.0;
        for i in 0..REV_COMBS * 2 {
            let mut tmp = if self.ptype == 0 { 800.0 + (rnd() * 1400.0) as i32 as f32 } else { COMBT[i % REV_COMBS] as f32 };
            tmp *= self.roomsize;
            if i > REV_COMBS {
                tmp += 23.0;
            }
            tmp *= adj;
            if tmp < 10.0 {
                tmp = 10.0;
            }
            self.combk[i] = 0;
            self.lpcomb[i] = 0.0;
            if self.comblen[i] != tmp as usize || self.comb[i].is_empty() {
                self.comblen[i] = tmp as usize;
                self.comb[i] = vec![0.0; self.comblen[i]];
            }
        }
        for i in 0..REV_APS * 2 {
            let mut tmp = if self.ptype == 0 { (500 + (rnd() * 500.0) as i32) as f32 } else { APT[i % REV_APS] as f32 };
            tmp *= self.roomsize;
            if i > REV_APS {
                tmp += 23.0;
            }
            tmp *= adj;
            if tmp < 10.0 {
                tmp = 10.0;
            }
            self.apk[i] = 0;
            if self.aplen[i] != tmp as usize || self.ap[i].is_empty() {
                self.aplen[i] = tmp as usize;
                self.ap[i] = vec![0.0; self.aplen[i]];
            }
        }
        self.bandwidth = None;
        if self.ptype == 2 {
            let mut u = Unison::new((self.b.bs / 4 + 1) as i32, 2.0, self.b.sr);
            u.set_size(50);
            u.set_base_frequency(1.0);
            self.bandwidth = Some(Box::new(u));
            let pb = self.pbandwidth;
            self.setbandwidth(pb);
        }
        let t = self.ptime;
        self.settime(t);
        self.cleanup();
    }

    fn setroomsize(&mut self, v: u8) {
        self.proomsize = if v == 0 { 64 } else { v };
        let mut rs = (self.proomsize as f32 - 64.0) / 64.0;
        if rs > 0.0 {
            rs *= 2.0;
        }
        self.roomsize = 10f32.powf(rs);
        self.rs = self.roomsize.sqrt();
        let t = self.ptype;
        self.settype(t);
    }

    fn setbandwidth(&mut self, v: u8) {
        self.pbandwidth = v;
        let x = v as f32 / 127.0;
        if let Some(u) = self.bandwidth.as_mut() {
            u.set_bandwidth(x * x * 200.0);
        }
    }

    fn changepar(&mut self, n: i32, v: u8) {
        match n {
            0 => self.setvolume(v),
            1 => self.b.setpanning(v),
            2 => self.settime(v),
            3 => self.setidelay(v),
            4 => {
                self.pidelayfb = v;
                self.idelayfb = v as f32 / 128.0;
            }
            7 => self.setlpf(v),
            8 => self.sethpf(v),
            9 => self.setlohidamp(v),
            10 => self.settype(v),
            11 => self.setroomsize(v),
            12 => self.setbandwidth(v),
            _ => {}
        }
    }

    fn getpar(&self, n: i32) -> u8 {
        match n {
            0 => self.pvolume,
            1 => self.b.ppanning,
            2 => self.ptime,
            3 => self.pidelay,
            4 => self.pidelayfb,
            7 => self.plpf,
            8 => self.phpf,
            9 => self.plohidamp,
            10 => self.ptype,
            11 => self.proomsize,
            12 => self.pbandwidth,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Echo
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Echo {
    b: Base,
    pvolume: u8,
    pdelay: u8,
    plrdelay: u8,
    pfb: u8,
    phidamp: u8,
    lrdelay: f32,
    avg_delay: f32,
    fb: f32,
    hidamp: f32,
    dl: Vec<f32>,
    dr: Vec<f32>,
    old_l: f32,
    old_r: f32,
    pos_l: usize,
    pos_r: usize,
    delta_l: i32,
    delta_r: i32,
    ndelta_l: i32,
    ndelta_r: i32,
}

impl Echo {
    const PRESETS: [[u8; 7]; 9] = [
        [67, 64, 35, 64, 30, 59, 0],
        [67, 64, 21, 64, 30, 59, 0],
        [67, 75, 60, 64, 30, 59, 10],
        [67, 60, 44, 64, 30, 0, 0],
        [67, 60, 102, 50, 30, 82, 48],
        [67, 64, 44, 17, 0, 82, 24],
        [81, 60, 46, 118, 100, 68, 18],
        [81, 60, 26, 100, 127, 67, 36],
        [62, 64, 28, 64, 100, 90, 55],
    ];
    fn new(b: Base) -> Self {
        let len = (2.0 * b.sr) as usize;
        let mut e = Self {
            b,
            pvolume: 50,
            pdelay: 60,
            plrdelay: 100,
            pfb: 40,
            phidamp: 60,
            lrdelay: 0.0,
            avg_delay: 0.0,
            fb: 0.0,
            hidamp: 0.0,
            dl: vec![0.0; len],
            dr: vec![0.0; len],
            old_l: 0.0,
            old_r: 0.0,
            pos_l: 0,
            pos_r: 0,
            delta_l: 1,
            delta_r: 1,
            ndelta_l: 1,
            ndelta_r: 1,
        };
        e.initdelays();
        e
    }
    fn presetpar(&self, np: u8, n: usize) -> u8 {
        if (np as usize) < 9 && n < 7 {
            let v = Self::PRESETS[np as usize][n];
            if n == 0 && self.b.insertion {
                return v / 2;
            }
            return v;
        }
        0
    }
    fn cleanup(&mut self) {
        self.dl.fill(0.0);
        self.dr.fill(0.0);
        self.old_l = 0.0;
        self.old_r = 0.0;
    }
    fn initdelays(&mut self) {
        self.cleanup();
        let dl = self.avg_delay - self.lrdelay;
        let dr = self.avg_delay + self.lrdelay;
        self.ndelta_l = ((dl * self.b.sr) as i32).max(1);
        self.ndelta_r = ((dr * self.b.sr) as i32).max(1);
        self.delta_l = self.ndelta_l;
        self.delta_r = self.ndelta_r;
    }
    fn out(&mut self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32]) {
        let len = self.dl.len();
        let lrc = self.b.lrcross;
        for i in 0..self.b.bs {
            let mut ldl = self.dl[self.pos_l];
            let mut rdl = self.dr[self.pos_r];
            ldl = ldl * (1.0 - lrc) + rdl * lrc;
            rdl = rdl * (1.0 - lrc) + ldl * lrc;
            ol[i] = ldl * 2.0;
            or[i] = rdl * 2.0;
            ldl = l[i] * self.b.pangain_l - ldl * self.fb;
            rdl = r[i] * self.b.pangain_r - rdl * self.fb;
            self.old_l = ldl * self.hidamp + self.old_l * (1.0 - self.hidamp);
            self.dl[(self.pos_l + self.delta_l as usize) % len] = self.old_l;
            self.old_r = rdl * self.hidamp + self.old_r * (1.0 - self.hidamp);
            self.dr[(self.pos_r + self.delta_r as usize) % len] = self.old_r;
            self.pos_l = (self.pos_l + 1) % len;
            self.pos_r = (self.pos_r + 1) % len;
            self.delta_l = (15 * self.delta_l + self.ndelta_l) / 16;
            self.delta_r = (15 * self.delta_r + self.ndelta_r) / 16;
        }
    }
    fn setvolume(&mut self, v: u8) {
        self.pvolume = v;
        if !self.b.insertion {
            self.b.outvolume = if v == 0 { 0.0 } else { 0.01f32.powf(1.0 - v as f32 / 127.0) * 4.0 };
            self.b.volume = 1.0;
        } else {
            self.b.volume = v as f32 / 127.0;
            self.b.outvolume = self.b.volume;
        }
        if v == 0 {
            self.cleanup();
        }
    }
    fn changepar(&mut self, n: i32, v: u8) {
        match n {
            0 => self.setvolume(v),
            1 => self.b.setpanning(v),
            2 => {
                self.pdelay = v;
                self.avg_delay = v as f32 / 127.0 * 1.5;
                self.initdelays();
            }
            3 => {
                self.plrdelay = v;
                let mut t = (2f32.powf((v as f32 - 64.0).abs() / 64.0 * 9.0) - 1.0) / 1000.0;
                if v < 64 {
                    t = -t;
                }
                self.lrdelay = t;
                self.initdelays();
            }
            4 => self.b.setlrcross(v),
            5 => {
                self.pfb = v;
                self.fb = v as f32 / 128.0;
            }
            6 => {
                self.phidamp = v;
                self.hidamp = 1.0 - v as f32 / 127.0;
            }
            _ => {}
        }
    }
    fn getpar(&self, n: i32) -> u8 {
        match n {
            0 => self.pvolume,
            1 => self.b.ppanning,
            2 => self.pdelay,
            3 => self.plrdelay,
            4 => self.b.plrcross,
            5 => self.pfb,
            6 => self.phidamp,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Chorus
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Chorus {
    b: Base,
    lfo: EffectLfo,
    pvolume: u8,
    pdepth: u8,
    pdelay: u8,
    pfb: u8,
    pflangemode: u8,
    poutsub: u8,
    depth: f32,
    delay: f32,
    fb: f32,
    maxdelay: usize,
    dsl: Vec<f32>,
    dsr: Vec<f32>,
    dlk: usize,
    drk: usize,
    dl_new: f32,
    dr_new: f32,
    dl_new2: f32,
    dr_new2: f32,
    dl_new3: f32,
    dr_new3: f32,
}

impl Chorus {
    const PRESETS: [[u8; 12]; 12] = [
        [64, 64, 50, 0, 0, 90, 40, 85, 64, 119, 0, 0],
        [64, 64, 45, 0, 0, 98, 56, 90, 64, 19, 0, 0],
        [64, 64, 29, 0, 1, 42, 97, 95, 90, 127, 0, 0],
        [64, 64, 26, 0, 0, 42, 115, 18, 90, 127, 0, 0],
        [64, 64, 29, 117, 0, 50, 115, 9, 31, 127, 0, 1],
        [64, 64, 57, 0, 0, 60, 23, 3, 62, 0, 0, 0],
        [64, 64, 33, 34, 1, 40, 35, 3, 109, 0, 0, 0],
        [64, 64, 53, 34, 1, 94, 35, 3, 54, 0, 0, 1],
        [64, 64, 40, 0, 1, 62, 12, 19, 97, 0, 0, 0],
        [64, 64, 55, 105, 0, 24, 39, 19, 17, 0, 0, 1],
        [127, 64, 68, 25, 1, 24, 35, 55, 64, 0, 2, 0],
        [127, 64, 55, 25, 1, 24, 32, 55, 80, 0, 3, 0],
    ];
    fn new(b: Base) -> Self {
        let maxdelay = (250.0 / 1000.0 * b.sr) as usize;
        let lfo = EffectLfo::new(b.sr, b.bs);
        Self {
            b,
            lfo,
            pvolume: 0,
            pdepth: 0,
            pdelay: 0,
            pfb: 0,
            pflangemode: 0,
            poutsub: 0,
            depth: 0.0,
            delay: 0.0,
            fb: 0.0,
            maxdelay,
            dsl: vec![0.0; maxdelay],
            dsr: vec![0.0; maxdelay],
            dlk: 0,
            drk: 0,
            dl_new: 0.0,
            dr_new: 0.0,
            dl_new2: 0.0,
            dr_new2: 0.0,
            dl_new3: 0.0,
            dr_new3: 0.0,
        }
    }
    fn after_ctor(&mut self) {
        self.changepar(1, 64);
        let (l, r) = self.lfo.out(0.0);
        self.dl_new = self.getdelay(l);
        self.dr_new = self.getdelay(r);
        self.cleanup();
    }
    fn presetpar(&self, np: u8, n: usize) -> u8 {
        if (np as usize) < 12 && n < 12 { Self::PRESETS[np as usize][n] } else { 0 }
    }
    fn getdelay(&self, x: f32) -> f32 {
        let r = if self.pflangemode == 1 { 0.0 } else { (self.delay + x * self.depth) * self.b.sr };
        if r + 0.5 >= self.maxdelay as f32 { self.maxdelay as f32 - 1.0 } else { r }
    }
    fn cleanup(&mut self) {
        self.dsl.fill(0.0);
        self.dsr.fill(0.0);
    }
    fn sample(line: &[f32], md: usize, mdel: f32, dk: usize) -> f32 {
        let pos = dk as f32 - mdel + (md * 2) as f32;
        cinterpolate(line, pos)
    }
    fn out(&mut self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32]) {
        let (dlh, drh) = (self.dl_new, self.dr_new);
        let (a, b) = self.lfo.out(0.0);
        self.dl_new = self.getdelay(a);
        self.dr_new = self.getdelay(b);
        let mut fbc = self.fb;
        let (mut dlh2, mut drh2, mut dlh3, mut drh3) = (0.0, 0.0, 0.0, 0.0);
        if self.pflangemode == 3 {
            dlh2 = self.dl_new2;
            drh2 = self.dr_new2;
            let (a, b) = self.lfo.out(0.5);
            self.dl_new2 = self.getdelay(a);
            self.dr_new2 = self.getdelay(b);
            fbc /= 2.0;
        }
        if self.pflangemode == 2 {
            dlh2 = self.dl_new2;
            drh2 = self.dr_new2;
            let (a, b) = self.lfo.out(0.333_333_34);
            self.dl_new2 = self.getdelay(a);
            self.dr_new2 = self.getdelay(b);
            dlh3 = self.dl_new3;
            drh3 = self.dr_new3;
            let (a, b) = self.lfo.out(0.666_666_7);
            self.dl_new3 = self.getdelay(a);
            self.dr_new3 = self.getdelay(b);
            fbc /= 3.53;
        }
        let bs = self.b.bs;
        let bsf = bs as f32;
        let md = self.maxdelay;
        let lrc = self.b.lrcross;
        for i in 0..bs {
            let il = l[i] * (1.0 - lrc) + r[i] * lrc;
            let ir = r[i] * (1.0 - lrc) + l[i] * lrc;
            let w = (bs - i) as f32;
            let fi = i as f32;
            self.dlk += 1;
            if self.dlk >= md {
                self.dlk = 0;
            }
            let mut out = Self::sample(&self.dsl, md, (dlh * w + self.dl_new * fi) / bsf, self.dlk);
            match self.pflangemode {
                3 => out += Self::sample(&self.dsl, md, (dlh2 * w + self.dl_new2 * fi) / bsf, self.dlk),
                2 => {
                    out += Self::sample(&self.dsl, md, (dlh2 * w + self.dl_new2 * fi) / bsf, self.dlk);
                    out += Self::sample(&self.dsl, md, (dlh3 * w + self.dl_new3 * fi) / bsf, self.dlk);
                    out *= 0.85;
                }
                _ => {}
            }
            self.dsl[self.dlk] = il + out * fbc;
            ol[i] = out;
            self.drk += 1;
            if self.drk >= md {
                self.drk = 0;
            }
            let mut out = Self::sample(&self.dsr, md, (drh * w + self.dr_new * fi) / bsf, self.drk);
            match self.pflangemode {
                3 => out += Self::sample(&self.dsr, md, (drh2 * w + self.dr_new2 * fi) / bsf, self.drk),
                2 => {
                    out += Self::sample(&self.dsr, md, (drh2 * w + self.dr_new2 * fi) / bsf, self.drk);
                    out += Self::sample(&self.dsr, md, (drh3 * w + self.dr_new3 * fi) / bsf, self.drk);
                    out *= 0.85;
                }
                _ => {}
            }
            self.dsr[self.drk] = ir + out * fbc;
            or[i] = out;
        }
        let s = if self.poutsub != 0 { -1.0 } else { 1.0 };
        for i in 0..bs {
            ol[i] *= self.b.pangain_l * s;
            or[i] *= self.b.pangain_r * s;
        }
    }
    fn changepar(&mut self, n: i32, v: u8) {
        match n {
            0 => {
                self.pvolume = v;
                self.b.outvolume = v as f32 / 127.0;
                self.b.volume = if !self.b.insertion { 1.0 } else { self.b.outvolume };
            }
            1 => self.b.setpanning(v),
            2..=5 => self.lfo.set(n, v),
            6 => {
                self.pdepth = v;
                self.depth = (8f32.powf(v as f32 / 127.0 * 2.0) - 1.0) / 1000.0;
            }
            7 => {
                self.pdelay = v;
                self.delay = (10f32.powf(v as f32 / 127.0 * 2.0) - 1.0) / 1000.0;
            }
            8 => {
                self.pfb = v;
                self.fb = (v as f32 - 64.0) / 64.1;
            }
            9 => self.b.setlrcross(v),
            10 => {
                self.lfo.updateparams();
                self.pflangemode = v.min(3);
            }
            11 => self.poutsub = v.min(1),
            _ => {}
        }
    }
    fn getpar(&self, n: i32) -> u8 {
        match n {
            0 => self.pvolume,
            1 => self.b.ppanning,
            2..=5 => self.lfo.get(n),
            6 => self.pdepth,
            7 => self.pdelay,
            8 => self.pfb,
            9 => self.b.plrcross,
            10 => self.pflangemode,
            11 => self.poutsub,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Phaser
// ---------------------------------------------------------------------------

const MAX_PHASER_STAGES: usize = 12;
const PH_ONE: f32 = 0.99999;
const PH_ZERO: f32 = 0.00001;

#[derive(Clone, Debug)]
struct Phaser {
    b: Base,
    lfo: EffectLfo,
    pvolume: u8,
    pdistortion: u8,
    pdepth: u8,
    pwidth: u8,
    pfb: u8,
    poffset: u8,
    pstages: usize,
    poutsub: u8,
    pphase: u8,
    phyper: u8,
    panalog: bool,
    old_l: Vec<f32>,
    old_r: Vec<f32>,
    xn1_l: Vec<f32>,
    xn1_r: Vec<f32>,
    yn1_l: Vec<f32>,
    yn1_r: Vec<f32>,
    oldgain: (f32, f32),
    fb: (f32, f32),
    feedback: f32,
    depth: f32,
    width: f32,
    phase: f32,
    distortion: f32,
    offsetpct: f32,
    offset: [f32; 12],
    rmin: f32,
    rmx: f32,
    cfs: f32,
    invperiod: f32,
}

impl Phaser {
    const PRESETS: [[u8; 15]; 12] = [
        [64, 64, 36, 0, 0, 64, 110, 64, 1, 0, 0, 20, 0, 0, 0],
        [64, 64, 35, 0, 0, 88, 40, 64, 3, 0, 0, 20, 0, 0, 0],
        [64, 64, 31, 0, 0, 66, 68, 107, 2, 0, 0, 20, 0, 0, 0],
        [39, 64, 22, 0, 0, 66, 67, 10, 5, 0, 1, 20, 0, 0, 0],
        [64, 64, 20, 0, 1, 110, 67, 78, 10, 0, 0, 20, 0, 0, 0],
        [64, 64, 53, 100, 0, 58, 37, 78, 3, 0, 0, 20, 0, 0, 0],
        [64, 64, 14, 0, 1, 64, 64, 40, 4, 10, 0, 110, 1, 20, 1],
        [64, 64, 14, 5, 1, 64, 70, 40, 6, 10, 0, 110, 1, 20, 1],
        [64, 64, 9, 0, 0, 64, 60, 40, 8, 10, 0, 40, 0, 20, 1],
        [64, 64, 14, 10, 0, 64, 45, 80, 7, 10, 1, 110, 1, 20, 1],
        [25, 64, 127, 10, 0, 64, 25, 16, 8, 100, 0, 25, 0, 20, 1],
        [64, 64, 1, 10, 1, 64, 70, 40, 12, 10, 0, 110, 1, 20, 1],
    ];
    fn new(b: Base) -> Self {
        let lfo = EffectLfo::new(b.sr, b.bs);
        let rmin = 625.0f32;
        let rmax = 22000.0f32;
        let c = 0.000_000_05f32;
        let cfs = 2.0 * b.sr * c;
        let invperiod = 1.0 / b.bs as f32;
        Self {
            b,
            lfo,
            pvolume: 0,
            pdistortion: 0,
            pdepth: 0,
            pwidth: 0,
            pfb: 0,
            poffset: 0,
            pstages: 1,
            poutsub: 0,
            pphase: 0,
            phyper: 0,
            panalog: false,
            old_l: vec![0.0; 2],
            old_r: vec![0.0; 2],
            xn1_l: vec![0.0; 1],
            xn1_r: vec![0.0; 1],
            yn1_l: vec![0.0; 1],
            yn1_r: vec![0.0; 1],
            oldgain: (0.0, 0.0),
            fb: (0.0, 0.0),
            feedback: 0.0,
            depth: 0.0,
            width: 0.0,
            phase: 0.0,
            distortion: 0.0,
            offsetpct: 0.0,
            offset: [
                -0.250_930_3,
                0.940_892_4,
                0.998,
                -0.348_618_2,
                -0.276_254_5,
                -0.521_578_5,
                0.250_930_3,
                -0.940_892_4,
                -0.998,
                0.348_618_2,
                0.276_254_5,
                0.521_578_5,
            ],
            rmin,
            rmx: rmin / rmax,
            cfs,
            invperiod,
        }
    }
    fn presetpar(&self, np: u8, n: usize) -> u8 {
        if (np as usize) < 12 && n < 15 { Self::PRESETS[np as usize][n] } else { 0 }
    }
    fn cleanup(&mut self) {
        self.fb = (0.0, 0.0);
        self.oldgain = (0.0, 0.0);
        self.old_l.fill(0.0);
        self.old_r.fill(0.0);
        self.xn1_l.fill(0.0);
        self.xn1_r.fill(0.0);
        self.yn1_l.fill(0.0);
        self.yn1_r.fill(0.0);
    }
    #[allow(clippy::too_many_arguments)]
    fn apply_analog(&self, mut x: f32, g: f32, fb: f32, hpf: &mut f32, yn1: &mut [f32], xn1: &mut [f32]) -> f32 {
        for j in 0..self.pstages {
            let mis = 1.0 + self.offsetpct * self.offset[j];
            let d = (1.0 + 2.0 * (0.25 + g) * *hpf * *hpf * self.distortion) * mis;
            let rconst = 1.0 + mis * self.rmx;
            let bb = (rconst - g) / (d * self.rmin);
            let gain = (self.cfs - bb) / (self.cfs + bb);
            yn1[j] = gain * (x + yn1[j]) - xn1[j];
            *hpf = yn1[j] + (1.0 - gain) * xn1[j];
            xn1[j] = x;
            x = yn1[j];
            if j == 1 {
                x += fb;
            }
        }
        x
    }
    fn apply_normal(stages: usize, mut x: f32, g: f32, old: &mut [f32]) -> f32 {
        for o in old.iter_mut().take(stages * 2) {
            let tmp = *o;
            *o = g * tmp + x;
            x = tmp - g * *o;
        }
        x
    }
    fn out(&mut self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32]) {
        let bs = self.b.bs;
        if self.panalog {
            let (lv, rv) = self.lfo.out(0.0);
            let mut ml = (lv * self.width + (self.depth - 0.5)).clamp(PH_ZERO, PH_ONE);
            let mut mr = (rv * self.width + (self.depth - 0.5)).clamp(PH_ZERO, PH_ONE);
            if self.phyper != 0 {
                ml *= ml;
                mr *= mr;
            }
            ml = (1.0 - ml).sqrt();
            mr = (1.0 - mr).sqrt();
            let dr = (mr - self.oldgain.1) * self.invperiod;
            let dl = (ml - self.oldgain.0) * self.invperiod;
            let (mut gl, mut gr) = self.oldgain;
            self.oldgain = (ml, mr);
            let (mut hl, mut hr) = (0.0f32, 0.0f32);
            let mut yl = std::mem::take(&mut self.yn1_l);
            let mut xl = std::mem::take(&mut self.xn1_l);
            let mut yr = std::mem::take(&mut self.yn1_r);
            let mut xr = std::mem::take(&mut self.xn1_r);
            for i in 0..bs {
                gl += dl;
                gr += dr;
                let a = self.apply_analog(l[i] * self.b.pangain_l, gl, self.fb.0, &mut hl, &mut yl, &mut xl);
                let b = self.apply_analog(r[i] * self.b.pangain_r, gr, self.fb.1, &mut hr, &mut yr, &mut xr);
                self.fb = (a * self.feedback, b * self.feedback);
                ol[i] = a;
                or[i] = b;
            }
            self.yn1_l = yl;
            self.xn1_l = xl;
            self.yn1_r = yr;
            self.xn1_r = xr;
        } else {
            let (lv, rv) = self.lfo.out(0.0);
            let shape = |v: f32| ((v * 2.0).exp() - 1.0) / (2f32.exp() - 1.0);
            let mut gl = shape(lv);
            let mut gr = shape(rv);
            gl = (1.0 - self.phase * (1.0 - self.depth) - (1.0 - self.phase) * gl * self.depth).clamp(PH_ZERO, PH_ONE);
            gr = (1.0 - self.phase * (1.0 - self.depth) - (1.0 - self.phase) * gr * self.depth).clamp(PH_ZERO, PH_ONE);
            for i in 0..bs {
                let x = i as f32 / bs as f32;
                let x1 = 1.0 - x;
                let xl = l[i] * self.b.pangain_l + self.fb.0;
                let xr = r[i] * self.b.pangain_r + self.fb.1;
                let g0 = gl * x + self.oldgain.0 * x1;
                let g1 = gr * x + self.oldgain.1 * x1;
                let mut a = Self::apply_normal(self.pstages, xl, g0, &mut self.old_l);
                let mut b = Self::apply_normal(self.pstages, xr, g1, &mut self.old_r);
                let c = self.b.lrcross;
                let (ta, tb) = (a, b);
                a = ta * (1.0 - c) + tb * c;
                b = tb * (1.0 - c) + ta * c;
                self.fb = (a * self.feedback, b * self.feedback);
                ol[i] = a;
                or[i] = b;
            }
            self.oldgain = (gl, gr);
        }
        if self.poutsub != 0 {
            for i in 0..bs {
                ol[i] = -ol[i];
                or[i] = -or[i];
            }
        }
    }
    fn setstages(&mut self, v: u8) {
        self.pstages = (v as usize).clamp(1, MAX_PHASER_STAGES);
        let s = self.pstages;
        self.old_l = vec![0.0; s * 2];
        self.old_r = vec![0.0; s * 2];
        self.xn1_l = vec![0.0; s];
        self.xn1_r = vec![0.0; s];
        self.yn1_l = vec![0.0; s];
        self.yn1_r = vec![0.0; s];
        self.cleanup();
    }
    fn changepar(&mut self, n: i32, v: u8) {
        match n {
            0 => {
                self.pvolume = v;
                self.b.outvolume = v as f32 / 127.0;
                self.b.volume = if !self.b.insertion { 1.0 } else { self.b.outvolume };
            }
            1 => self.b.setpanning(v),
            2..=5 => self.lfo.set(n, v),
            6 => {
                self.pdepth = v;
                self.depth = v as f32 / 127.0;
            }
            7 => {
                self.pfb = v;
                self.feedback = (v as f32 - 64.0) / 64.2;
            }
            8 => self.setstages(v),
            9 => {
                self.b.setlrcross(v);
                self.poffset = v;
                self.offsetpct = v as f32 / 127.0;
            }
            10 => self.poutsub = v.min(1),
            11 => {
                self.pphase = v;
                self.phase = v as f32 / 127.0;
                self.pwidth = v;
                self.width = v as f32 / 127.0;
            }
            12 => self.phyper = v.min(1),
            13 => {
                self.pdistortion = v;
                self.distortion = v as f32 / 127.0;
            }
            14 => self.panalog = v != 0,
            _ => {}
        }
    }
    fn getpar(&self, n: i32) -> u8 {
        match n {
            0 => self.pvolume,
            1 => self.b.ppanning,
            2..=5 => self.lfo.get(n),
            6 => self.pdepth,
            7 => self.pfb,
            8 => self.pstages as u8,
            9 => self.b.plrcross,
            10 => self.poutsub,
            11 => self.pphase,
            12 => self.phyper,
            13 => self.pdistortion,
            14 => self.panalog as u8,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Alienwah
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
struct Cpx(f32, f32);

impl Cpx {
    fn mul(self, o: Cpx) -> Cpx {
        Cpx(self.0 * o.0 - self.1 * o.1, self.0 * o.1 + self.1 * o.0)
    }
}

#[derive(Clone, Debug)]
struct Alienwah {
    b: Base,
    lfo: EffectLfo,
    pvolume: u8,
    pdepth: u8,
    pfb: u8,
    pdelay: usize,
    pphase: u8,
    depth: f32,
    fb: f32,
    phase: f32,
    oldl: Vec<Cpx>,
    oldr: Vec<Cpx>,
    oldk: usize,
    oldclfol: Cpx,
    oldclfor: Cpx,
}

impl Alienwah {
    const PRESETS: [[u8; 11]; 4] = [
        [127, 64, 70, 0, 0, 62, 60, 105, 25, 0, 64],
        [127, 64, 73, 106, 0, 101, 60, 105, 17, 0, 64],
        [127, 64, 63, 0, 1, 100, 112, 105, 31, 0, 42],
        [93, 64, 25, 0, 1, 66, 101, 11, 47, 0, 86],
    ];
    fn new(b: Base) -> Self {
        let lfo = EffectLfo::new(b.sr, b.bs);
        Self {
            b,
            lfo,
            pvolume: 0,
            pdepth: 0,
            pfb: 0,
            pdelay: 1,
            pphase: 0,
            depth: 0.0,
            fb: 0.0,
            phase: 0.0,
            oldl: vec![Cpx::default(); 1],
            oldr: vec![Cpx::default(); 1],
            oldk: 0,
            oldclfol: Cpx::default(),
            oldclfor: Cpx::default(),
        }
    }
    fn after_ctor(&mut self) {
        self.cleanup();
        self.oldclfol = Cpx(self.fb, 0.0);
        self.oldclfor = Cpx(self.fb, 0.0);
    }
    fn presetpar(&self, np: u8, n: usize) -> u8 {
        if (np as usize) < 4 && n < 11 {
            let v = Self::PRESETS[np as usize][n];
            if n == 0 && !self.b.insertion {
                return v / 2;
            }
            return v;
        }
        0
    }
    fn cleanup(&mut self) {
        self.oldl.fill(Cpx::default());
        self.oldr.fill(Cpx::default());
        self.oldk = 0;
    }
    fn out(&mut self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32]) {
        let (mut lfol, mut lfor) = self.lfo.out(0.0);
        lfol *= self.depth * PI * 2.0;
        lfor *= self.depth * PI * 2.0;
        let fb = self.fb;
        let clfol = Cpx((lfol + self.phase).cos() * fb, (lfol + self.phase).sin() * fb);
        let clfor = Cpx((lfor + self.phase).cos() * fb, (lfor + self.phase).sin() * fb);
        let bs = self.b.bs;
        let lrc = self.b.lrcross;
        for i in 0..bs {
            let x = i as f32 / bs as f32;
            let x1 = 1.0 - x;
            let t = Cpx(clfol.0 * x + self.oldclfol.0 * x1, clfol.1 * x + self.oldclfol.1 * x1);
            let mut o = t.mul(self.oldl[self.oldk]);
            o.0 += (1.0 - fb.abs()) * l[i] * self.b.pangain_l;
            self.oldl[self.oldk] = o;
            let lv = o.0 * 10.0 * (fb + 0.1);
            let t = Cpx(clfor.0 * x + self.oldclfor.0 * x1, clfor.1 * x + self.oldclfor.1 * x1);
            let mut o = t.mul(self.oldr[self.oldk]);
            o.0 += (1.0 - fb.abs()) * r[i] * self.b.pangain_r;
            self.oldr[self.oldk] = o;
            let rv = o.0 * 10.0 * (fb + 0.1);
            self.oldk += 1;
            if self.oldk >= self.pdelay {
                self.oldk = 0;
            }
            ol[i] = lv * (1.0 - lrc) + rv * lrc;
            or[i] = rv * (1.0 - lrc) + lv * lrc;
        }
        self.oldclfol = clfol;
        self.oldclfor = clfor;
    }
    fn changepar(&mut self, n: i32, v: u8) {
        match n {
            0 => {
                self.pvolume = v;
                self.b.outvolume = v as f32 / 127.0;
                self.b.volume = if !self.b.insertion { 1.0 } else { self.b.outvolume };
            }
            1 => self.b.setpanning(v),
            2..=5 => self.lfo.set(n, v),
            6 => {
                self.pdepth = v;
                self.depth = v as f32 / 127.0;
            }
            7 => {
                self.pfb = v;
                let mut f = ((v as f32 - 64.0) / 64.1).abs().sqrt();
                if f < 0.4 {
                    f = 0.4;
                }
                if v < 64 {
                    f = -f;
                }
                self.fb = f;
            }
            8 => {
                self.pdelay = (v as usize).clamp(1, 100);
                self.oldl = vec![Cpx::default(); self.pdelay];
                self.oldr = vec![Cpx::default(); self.pdelay];
                self.cleanup();
            }
            9 => self.b.setlrcross(v),
            10 => {
                self.pphase = v;
                self.phase = (v as f32 - 64.0) / 64.0 * PI;
            }
            _ => {}
        }
    }
    fn getpar(&self, n: i32) -> u8 {
        match n {
            0 => self.pvolume,
            1 => self.b.ppanning,
            2..=5 => self.lfo.get(n),
            6 => self.pdepth,
            7 => self.pfb,
            8 => self.pdelay as u8,
            9 => self.b.plrcross,
            10 => self.pphase,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Distortion
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Distortion {
    b: Base,
    pvolume: u8,
    pdrive: u8,
    plevel: u8,
    ptype: u8,
    pnegate: u8,
    plpf: u8,
    phpf: u8,
    pstereo: u8,
    pprefiltering: bool,
    pfuncpar: u8,
    poffset: u8,
    lpfl: AnalogFilter,
    lpfr: AnalogFilter,
    hpfl: AnalogFilter,
    hpfr: AnalogFilter,
}

impl Distortion {
    const PRESETS: [[u8; 13]; 6] = [
        [127, 64, 35, 56, 70, 0, 0, 96, 0, 0, 0, 32, 64],
        [127, 64, 35, 29, 75, 1, 0, 127, 0, 0, 0, 32, 64],
        [64, 64, 35, 75, 80, 5, 0, 127, 105, 1, 0, 32, 64],
        [64, 64, 35, 85, 62, 1, 0, 127, 118, 1, 0, 32, 64],
        [127, 64, 35, 63, 75, 2, 0, 55, 0, 0, 0, 32, 64],
        [127, 64, 35, 88, 75, 4, 0, 127, 0, 1, 0, 32, 64],
    ];
    fn new(b: Base) -> Self {
        let (sr, bs) = (b.sr, b.bs);
        Self {
            b,
            pvolume: 50,
            pdrive: 90,
            plevel: 64,
            ptype: 0,
            pnegate: 0,
            plpf: 127,
            phpf: 0,
            pstereo: 0,
            pprefiltering: false,
            pfuncpar: 32,
            poffset: 64,
            lpfl: AnalogFilter::new(2, 22000.0, 1.0, 0, sr, bs),
            lpfr: AnalogFilter::new(2, 22000.0, 1.0, 0, sr, bs),
            hpfl: AnalogFilter::new(3, 20.0, 1.0, 0, sr, bs),
            hpfr: AnalogFilter::new(3, 20.0, 1.0, 0, sr, bs),
        }
    }
    fn presetpar(&self, np: u8, n: usize) -> u8 {
        if (np as usize) < 6 && n < 13 {
            let v = Self::PRESETS[np as usize][n];
            if n == 0 && !self.b.insertion {
                return ((2 * v as u32) / 3) as u8;
            }
            return v;
        }
        0
    }
    fn cleanup(&mut self) {
        self.lpfl.cleanup();
        self.hpfl.cleanup();
        self.lpfr.cleanup();
        self.hpfr.cleanup();
    }
    fn applyfilters(&mut self, ol: &mut [f32], or: &mut [f32]) {
        if self.plpf != 127 {
            self.lpfl.filterout(ol);
        }
        if self.phpf != 0 {
            self.hpfl.filterout(ol);
        }
        if self.pstereo != 0 {
            if self.plpf != 127 {
                self.lpfr.filterout(or);
            }
            if self.phpf != 0 {
                self.hpfr.filterout(or);
            }
        }
    }
    fn out(&mut self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32]) {
        let bs = self.b.bs;
        let mut iv = 5f32.powf((self.pdrive as f32 - 32.0) / 127.0);
        if self.pnegate != 0 {
            iv = -iv;
        }
        if self.pstereo != 0 {
            for i in 0..bs {
                ol[i] = l[i] * iv * self.b.pangain_l;
                or[i] = r[i] * iv * self.b.pangain_r;
            }
        } else {
            for i in 0..bs {
                ol[i] = (l[i] * self.b.pangain_l + r[i] * self.b.pangain_r) * iv;
            }
        }
        if self.pprefiltering {
            self.applyfilters(&mut ol[..bs], &mut or[..bs]);
        }
        wave_shape_smps(&mut ol[..bs], self.ptype + 1, self.pdrive, self.poffset, self.pfuncpar);
        if self.pstereo != 0 {
            wave_shape_smps(&mut or[..bs], self.ptype + 1, self.pdrive, self.poffset, self.pfuncpar);
        }
        if !self.pprefiltering {
            self.applyfilters(&mut ol[..bs], &mut or[..bs]);
        }
        if self.pstereo == 0 {
            or[..bs].copy_from_slice(&ol[..bs]);
        }
        let level = db2rap(60.0 * self.plevel as f32 / 127.0 - 40.0);
        let c = self.b.lrcross;
        for i in 0..bs {
            let (lo, ro) = (ol[i], or[i]);
            let a = lo * (1.0 - c) + ro * c;
            let b = ro * (1.0 - c) + lo * c;
            ol[i] = a * 2.0 * level;
            or[i] = b * 2.0 * level;
        }
    }
    fn changepar(&mut self, n: i32, v: u8) {
        match n {
            0 => {
                self.pvolume = v;
                if !self.b.insertion {
                    self.b.outvolume = 0.01f32.powf(1.0 - v as f32 / 127.0) * 4.0;
                    self.b.volume = 1.0;
                } else {
                    self.b.volume = v as f32 / 127.0;
                    self.b.outvolume = self.b.volume;
                }
                if v == 0 {
                    self.cleanup();
                }
            }
            1 => self.b.setpanning(v),
            2 => self.b.setlrcross(v),
            3 => self.pdrive = v,
            4 => self.plevel = v,
            5 => self.ptype = v.min(16),
            6 => self.pnegate = v.min(1),
            7 => {
                self.plpf = v;
                let fr = ((v as f32 / 127.0).sqrt() * 25000f32.ln()).exp() + 40.0;
                self.lpfl.setfreq(fr);
                self.lpfr.setfreq(fr);
            }
            8 => {
                self.phpf = v;
                let fr = ((v as f32 / 127.0).sqrt() * 25000f32.ln()).exp() + 20.0;
                self.hpfl.setfreq(fr);
                self.hpfr.setfreq(fr);
            }
            9 => self.pstereo = v.min(1),
            10 => self.pprefiltering = v != 0,
            11 => self.pfuncpar = v,
            12 => self.poffset = v,
            _ => {}
        }
    }
    fn getpar(&self, n: i32) -> u8 {
        match n {
            0 => self.pvolume,
            1 => self.b.ppanning,
            2 => self.b.plrcross,
            3 => self.pdrive,
            4 => self.plevel,
            5 => self.ptype,
            6 => self.pnegate,
            7 => self.plpf,
            8 => self.phpf,
            9 => self.pstereo,
            10 => self.pprefiltering as u8,
            11 => self.pfuncpar,
            12 => self.poffset,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// EQ
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct EqBand {
    ptype: u8,
    pfreq: u8,
    pgain: u8,
    pq: u8,
    pstages: u8,
    l: AnalogFilter,
    r: AnalogFilter,
}

#[derive(Clone, Debug)]
struct Eq {
    b: Base,
    pvolume: u8,
    bands: Vec<EqBand>,
}

impl Eq {
    fn new(b: Base) -> Self {
        let bands = (0..MAX_EQ_BANDS)
            .map(|_| EqBand {
                ptype: 0,
                pfreq: 0,
                pgain: 0,
                pq: 0,
                pstages: 0,
                l: AnalogFilter::new(6, 1000.0, 1.0, 0, b.sr, b.bs),
                r: AnalogFilter::new(6, 1000.0, 1.0, 0, b.sr, b.bs),
            })
            .collect();
        Self { b, pvolume: 50, bands }
    }
    fn presetpar(&self, np: u8, n: usize) -> u8 {
        if np < 2 && n < 1 {
            return 67;
        }
        if (10..10 + MAX_EQ_BANDS * 5).contains(&n) {
            return [0, 64, 64, 64, 0][n % 5];
        }
        0
    }
    fn cleanup(&mut self) {
        for b in self.bands.iter_mut() {
            b.l.cleanup();
            b.r.cleanup();
        }
    }
    fn out(&mut self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32]) {
        let bs = self.b.bs;
        for i in 0..bs {
            ol[i] = l[i] * self.b.volume;
            or[i] = r[i] * self.b.volume;
        }
        for b in self.bands.iter_mut() {
            if b.ptype == 0 {
                continue;
            }
            b.l.filterout(&mut ol[..bs]);
            b.r.filterout(&mut or[..bs]);
        }
    }
    fn changepar(&mut self, n: i32, v: u8) {
        if n == 0 {
            self.pvolume = v;
            self.b.outvolume = 0.005f32.powf(1.0 - v as f32 / 127.0) * 10.0;
            self.b.volume = if !self.b.insertion { 1.0 } else { self.b.outvolume };
        }
        if n < 10 {
            return;
        }
        let nb = ((n - 10) / 5) as usize;
        if nb >= MAX_EQ_BANDS {
            return;
        }
        let b = &mut self.bands[nb];
        match n % 5 {
            0 => {
                b.ptype = if v > 9 { 0 } else { v };
                if b.ptype != 0 {
                    b.l.settype(v as i32 - 1);
                    b.r.settype(v as i32 - 1);
                }
            }
            1 => {
                b.pfreq = v;
                let t = 600.0 * 30f32.powf((v as f32 - 64.0) / 64.0);
                b.l.setfreq(t);
                b.r.setfreq(t);
            }
            2 => {
                b.pgain = v;
                let t = 30.0 * (v as f32 - 64.0) / 64.0;
                b.l.setgain(t);
                b.r.setgain(t);
            }
            3 => {
                b.pq = v;
                let t = 30f32.powf((v as f32 - 64.0) / 64.0);
                b.l.setq(t);
                b.r.setq(t);
            }
            _ => {
                b.pstages = v.min(MAX_FILTER_STAGES as u8 - 1);
                b.l.setstages(v as i32);
                b.r.setstages(v as i32);
            }
        }
    }
    fn getpar(&self, n: i32) -> u8 {
        if n == 0 {
            return self.pvolume;
        }
        if n < 10 {
            return 0;
        }
        let nb = ((n - 10) / 5) as usize;
        if nb >= MAX_EQ_BANDS {
            return 0;
        }
        let b = &self.bands[nb];
        match n % 5 {
            0 => b.ptype,
            1 => b.pfreq,
            2 => b.pgain,
            3 => b.pq,
            _ => b.pstages,
        }
    }
    pub fn freqresponse(&mut self, freq: f32) -> f32 {
        let mut resp = 1.0;
        for b in self.bands.iter_mut() {
            if b.ptype == 0 {
                continue;
            }
            b.l.refresh();
            resp *= b.l.h(freq);
        }
        rap2db(resp * self.b.outvolume)
    }
}

// ---------------------------------------------------------------------------
// DynamicFilter
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct DynFilter {
    b: Base,
    lfo: EffectLfo,
    pvolume: u8,
    pdepth: u8,
    pampsns: u8,
    pampsnsinv: u8,
    pampsmooth: u8,
    depth: f32,
    ampsns: f32,
    ampsmooth: f32,
    ms1: f32,
    ms2: f32,
    ms3: f32,
    ms4: f32,
    fp: FilterParams,
    fl: Option<Filter>,
    fr: Option<Filter>,
}

impl DynFilter {
    const PRESETS: [[u8; 10]; 5] = [
        [110, 64, 80, 0, 0, 64, 0, 90, 0, 60],
        [110, 64, 70, 0, 0, 80, 70, 0, 0, 60],
        [100, 64, 30, 0, 0, 50, 80, 0, 0, 60],
        [110, 64, 80, 0, 0, 64, 0, 64, 0, 60],
        [127, 64, 50, 0, 0, 96, 64, 0, 0, 60],
    ];
    fn new(b: Base, fp: FilterParams) -> Self {
        let lfo = EffectLfo::new(b.sr, b.bs);
        Self {
            b,
            lfo,
            pvolume: 110,
            pdepth: 0,
            pampsns: 90,
            pampsnsinv: 0,
            pampsmooth: 60,
            depth: 0.0,
            ampsns: 0.0,
            ampsmooth: 0.0,
            ms1: 0.0,
            ms2: 0.0,
            ms3: 0.0,
            ms4: 0.0,
            fp,
            fl: None,
            fr: None,
        }
    }
    fn presetpar(&self, np: u8, n: usize) -> u8 {
        if (np as usize) < 5 && n < 10 {
            let v = Self::PRESETS[np as usize][n];
            if n == 0 && !self.b.insertion {
                return v / 2;
            }
            return v;
        }
        0
    }
    fn reinit(&mut self) {
        self.fl = Some(Filter::generate(&self.fp, self.b.sr, self.b.bs));
        self.fr = Some(Filter::generate(&self.fp, self.b.sr, self.b.bs));
    }
    fn cleanup(&mut self) {
        self.reinit();
        self.ms1 = 0.0;
        self.ms2 = 0.0;
        self.ms3 = 0.0;
        self.ms4 = 0.0;
    }
    /// `setfilterpreset`.
    fn filter_preset(np: u8) -> FilterParams {
        let mut f = FilterParams::new(Loc::InEffect);
        // Formants are random in `defaults()`; keep Zyn's behaviour.
        f.basefreq = basefreq_from_old(64);
        f.baseq = baseq_from_old(64);
        let gain64 = 0.0;
        match np {
            0 => {
                f.category = 0;
                f.ftype = 2;
                f.basefreq = basefreq_from_old(45);
                f.baseq = baseq_from_old(64);
                f.stages = 1;
                f.gain = gain64;
            }
            1 => {
                f.category = 2;
                f.ftype = 0;
                f.basefreq = basefreq_from_old(72);
                f.baseq = baseq_from_old(64);
                f.stages = 0;
                f.gain = gain64;
            }
            2 => {
                f.category = 0;
                f.ftype = 4;
                f.basefreq = basefreq_from_old(64);
                f.baseq = baseq_from_old(64);
                f.stages = 2;
                f.gain = gain64;
            }
            3 => {
                f.category = 1;
                f.ftype = 0;
                f.basefreq = basefreq_from_old(50);
                f.baseq = baseq_from_old(70);
                f.stages = 1;
                f.gain = gain64;
                f.sequencesize = 2;
                let v = [[(34, 127), (99, 122), (108, 112)], [(61, 127), (71, 121), (99, 117)]];
                for (j, vow) in v.iter().enumerate() {
                    for (i, (fr, a)) in vow.iter().enumerate() {
                        f.vowels[j][i].freq = *fr;
                        f.vowels[j][i].amp = *a;
                        f.vowels[j][i].q = 64;
                    }
                }
            }
            4 => {
                f.category = 1;
                f.ftype = 0;
                f.basefreq = basefreq_from_old(64);
                f.baseq = baseq_from_old(70);
                f.stages = 1;
                f.gain = gain64;
                f.sequencesize = 2;
                f.numformants = 2;
                f.vowelclearness = 0;
                let v = [[(70, 127), (80, 122)], [(20, 127), (100, 121)]];
                for (j, vow) in v.iter().enumerate() {
                    for (i, (fr, a)) in vow.iter().enumerate() {
                        f.vowels[j][i].freq = *fr;
                        f.vowels[j][i].amp = *a;
                        f.vowels[j][i].q = 64;
                    }
                }
            }
            _ => {}
        }
        f
    }
    fn out(&mut self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32]) {
        if self.fl.is_none() {
            self.reinit();
        }
        let (mut lfol, mut lfor) = self.lfo.out(0.0);
        lfol *= self.depth * 5.0;
        lfor *= self.depth * 5.0;
        let freq = self.fp.getfreq();
        let q = self.fp.getq();
        let bs = self.b.bs;
        for i in 0..bs {
            ol[i] = l[i];
            or[i] = r[i];
            let x = (l[i].abs() + r[i].abs()) * 0.5;
            self.ms1 = ((self.ms1 * (1.0 - self.ampsmooth) + x * self.ampsmooth) as f64 + 1e-10) as f32;
        }
        let a2 = self.ampsmooth.powf(0.2) * 0.3;
        self.ms2 = self.ms2 * (1.0 - a2) + self.ms1 * a2;
        self.ms3 = self.ms3 * (1.0 - a2) + self.ms2 * a2;
        self.ms4 = self.ms4 * (1.0 - a2) + self.ms3 * a2;
        let rms = self.ms4.sqrt() * self.ampsns;
        let frl = Filter::getrealfreq(freq + lfol + rms);
        let frr = Filter::getrealfreq(freq + lfor + rms);
        if let (Some(fl), Some(fr)) = (self.fl.as_mut(), self.fr.as_mut()) {
            fl.setfreq_and_q(frl, q);
            fr.setfreq_and_q(frr, q);
            fl.filterout(&mut ol[..bs]);
            fr.filterout(&mut or[..bs]);
        }
        for i in 0..bs {
            ol[i] *= self.b.pangain_l;
            or[i] *= self.b.pangain_r;
        }
    }
    fn setampsns(&mut self) {
        self.ampsns = (self.pampsns as f32 / 127.0).powf(2.5) * 10.0;
        if self.pampsnsinv != 0 {
            self.ampsns = -self.ampsns;
        }
        self.ampsmooth = (-(self.pampsmooth as f32) / 127.0 * 10.0).exp() * 0.99;
    }
    fn changepar(&mut self, n: i32, v: u8) {
        match n {
            0 => {
                self.pvolume = v;
                self.b.outvolume = v as f32 / 127.0;
                self.b.volume = if !self.b.insertion { 1.0 } else { self.b.outvolume };
            }
            1 => self.b.setpanning(v),
            2..=5 => self.lfo.set(n, v),
            6 => {
                self.pdepth = v;
                self.depth = (v as f32 / 127.0).powi(2);
            }
            7 => {
                self.pampsns = v;
                self.setampsns();
            }
            8 => {
                self.pampsnsinv = v;
                self.setampsns();
            }
            9 => {
                self.pampsmooth = v;
                self.setampsns();
            }
            _ => {}
        }
    }
    fn getpar(&self, n: i32) -> u8 {
        match n {
            0 => self.pvolume,
            1 => self.b.ppanning,
            2..=5 => self.lfo.get(n),
            6 => self.pdepth,
            7 => self.pampsns,
            8 => self.pampsnsinv,
            9 => self.pampsmooth,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Effect (polymorphic)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Effect {
    Reverb(Box<Reverb>),
    Echo(Box<Echo>),
    Chorus(Box<Chorus>),
    Phaser(Box<Phaser>),
    Alienwah(Box<Alienwah>),
    Distortion(Box<Distortion>),
    Eq(Box<Eq>),
    DynFilter(Box<DynFilter>),
}

macro_rules! each {
    ($s:expr, $e:ident => $body:expr) => {
        match $s {
            Effect::Reverb($e) => $body,
            Effect::Echo($e) => $body,
            Effect::Chorus($e) => $body,
            Effect::Phaser($e) => $body,
            Effect::Alienwah($e) => $body,
            Effect::Distortion($e) => $body,
            Effect::Eq($e) => $body,
            Effect::DynFilter($e) => $body,
        }
    };
}

impl Effect {
    /// Construct like the C++ constructors do (each ends by loading
    /// preset 0). `protect_filter` keeps `fp` for the dynamic filter.
    fn new(kind: u8, insertion: bool, sr: f32, bs: usize, fp: &FilterParams, protect_filter: bool) -> Option<Self> {
        let b = Base::new(insertion, sr, bs);
        let mut e = match kind {
            1 => Effect::Reverb(Box::new(Reverb::new(b))),
            2 => Effect::Echo(Box::new(Echo::new(b))),
            3 => Effect::Chorus(Box::new(Chorus::new(b))),
            4 => Effect::Phaser(Box::new(Phaser::new(b))),
            5 => Effect::Alienwah(Box::new(Alienwah::new(b))),
            6 => Effect::Distortion(Box::new(Distortion::new(b))),
            7 => Effect::Eq(Box::new(Eq::new(b))),
            8 => Effect::DynFilter(Box::new(DynFilter::new(b, fp.clone()))),
            _ => return None,
        };
        e.setpreset(0, protect_filter);
        match &mut e {
            Effect::Chorus(c) => c.after_ctor(),
            Effect::Alienwah(a) => a.after_ctor(),
            _ => e.cleanup(),
        }
        Some(e)
    }
    fn base(&self) -> &Base {
        each!(self, e => &e.b)
    }
    fn changepar(&mut self, n: i32, v: u8) {
        each!(self, e => e.changepar(n, v))
    }
    fn getpar(&self, n: i32) -> u8 {
        each!(self, e => e.getpar(n))
    }
    fn presetpar(&self, np: u8, n: usize) -> u8 {
        each!(self, e => e.presetpar(np, n))
    }
    fn num_presets(&self) -> u8 {
        match self {
            Effect::Reverb(_) => 13,
            Effect::Echo(_) => 9,
            Effect::Chorus(_) => 12,
            Effect::Phaser(_) => 12,
            Effect::Alienwah(_) => 4,
            Effect::Distortion(_) => 6,
            Effect::Eq(_) => 2,
            Effect::DynFilter(_) => 5,
        }
    }
    fn cleanup(&mut self) {
        each!(self, e => e.cleanup())
    }
    /// `setpreset`.
    fn setpreset(&mut self, np: u8, protect_filter: bool) {
        let np = np.min(self.num_presets() - 1);
        for n in 0..128 {
            let v = self.presetpar(np, n);
            self.changepar(n as i32, v);
        }
        match self {
            Effect::Distortion(d) => d.cleanup(),
            Effect::DynFilter(d) => {
                if !protect_filter {
                    d.fp = DynFilter::filter_preset(np);
                    d.reinit();
                }
            }
            _ => {}
        }
    }
    fn out(&mut self, l: &[f32], r: &[f32], ol: &mut [f32], or: &mut [f32]) {
        each!(self, e => e.out(l, r, ol, or))
    }
}

// ---------------------------------------------------------------------------
// EffectMgr
// ---------------------------------------------------------------------------

/// Saved effect slot configuration (what the XML holds).
#[derive(Clone, Debug)]
pub struct EffectCfg {
    pub kind: u8,
    pub preset: u8,
    /// -1 = parameter default for the preset.
    pub settings: [i16; 128],
    pub filter: FilterParams,
    pub numerator: i32,
    pub denominator: i32,
}

impl Default for EffectCfg {
    fn default() -> Self {
        Self { kind: 0, preset: 0, settings: [-1; 128], filter: FilterParams::new(Loc::InEffect), numerator: 0, denominator: 4 }
    }
}

impl EffectCfg {
    /// Change the effect type, filling in the preset 0 values.
    pub fn set_kind(&mut self, kind: u8, insertion: bool) {
        self.kind = kind;
        self.preset = 0;
        self.settings = [-1; 128];
        if kind == 8 {
            self.filter = DynFilter::filter_preset(0);
        }
        self.resolve(insertion);
    }

    /// Load a factory preset into the settings.
    pub fn set_preset(&mut self, preset: u8, insertion: bool) {
        self.preset = preset;
        self.settings = [-1; 128];
        if self.kind == 8 {
            self.filter = DynFilter::filter_preset(preset);
        }
        self.resolve(insertion);
    }

    /// Replace -1 entries with the effect's actual values.
    pub fn resolve(&mut self, insertion: bool) {
        if let Some(mut e) = Effect::new(self.kind, insertion, 44100.0, 256, &self.filter, true) {
            e.setpreset(self.preset, true);
            for n in 0..128 {
                if self.settings[n] >= 0 {
                    e.changepar(n as i32, self.settings[n] as u8);
                }
            }
            for n in 0..128 {
                self.settings[n] = e.getpar(n as i32) as i16;
            }
        }
    }

    pub fn num_presets(&self) -> u8 {
        Effect::new(self.kind, true, 44100.0, 256, &self.filter, true).map(|e| e.num_presets()).unwrap_or(0)
    }

    pub fn par(&self, n: usize) -> u8 {
        self.settings[n].max(0) as u8
    }

    pub fn add_to_xml(&self, x: &mut XmlNode, insertion: bool) {
        x.add_par("type", self.kind as i32);
        if self.kind == 0 {
            return;
        }
        x.add_par("preset", self.preset as i32);
        let e = Effect::new(self.kind, insertion, 44100.0, 256, &self.filter, true);
        let p = x.child("EFFECT_PARAMETERS");
        for n in 0..128 {
            let par = self.settings[n];
            let def = e.as_ref().map(|e| e.presetpar(self.preset, n) as i16).unwrap_or(-1);
            if par < 0 || par == def {
                continue;
            }
            p.child_id("par_no", n).add_par("par", par as i32);
        }
        if self.kind == 8 {
            self.filter.add_to_xml(p.child("FILTER"));
        }
        x.add_par("numerator", self.numerator);
        x.add_par("denominator", self.denominator);
    }

    pub fn get_from_xml(&mut self, x: &XmlNode, ver: Version, insertion: bool) {
        self.kind = x.par127("type", self.kind);
        if self.kind == 0 {
            return;
        }
        self.preset = x.par127("preset", self.preset);
        if self.kind == 8 {
            self.filter = DynFilter::filter_preset(self.preset);
        }
        if let Some(p) = x.branch("EFFECT_PARAMETERS") {
            for n in 0..128 {
                match p.branch_id("par_no", n) {
                    Some(c) => self.settings[n] = c.par127("par", 0) as i16,
                    None => {
                        self.settings[n] = if ver < Version(3, 0, 6) && (self.kind != 6 || n < 11) { 0 } else { -1 };
                    }
                }
            }
            if let Some(f) = p.branch("FILTER") {
                self.filter.get_from_xml(f, ver);
            }
        }
        self.numerator = x.par("numerator", self.numerator, 0, 99);
        self.denominator = x.par("denominator", self.denominator, 1, 99);
        self.resolve(insertion);
    }
}

/// Running effect slot.
#[derive(Clone, Debug)]
pub struct EffectMgr {
    pub insertion: bool,
    pub dryonly: bool,
    pub const_power: bool,
    kind: u8,
    preset: u8,
    settings: [i16; 128],
    efx: Option<Effect>,
    pub efxoutl: Vec<f32>,
    pub efxoutr: Vec<f32>,
    sr: f32,
    bs: usize,
}

impl EffectMgr {
    pub fn new(insertion: bool, sr: f32, bs: usize) -> Self {
        Self {
            insertion,
            dryonly: false,
            const_power: true,
            kind: 0,
            preset: 0,
            settings: [-1; 128],
            efx: None,
            efxoutl: vec![0.0; bs],
            efxoutr: vec![0.0; bs],
            sr,
            bs,
        }
    }

    pub fn kind(&self) -> u8 {
        self.kind
    }

    /// Same construction sequence as Zyn's `changeeffectrt` +
    /// `changepresetrt` (used to compare against the original).
    pub fn for_test(kind: u8, preset: u8, insertion: bool, sr: f32, bs: usize) -> Self {
        let mut m = Self::new(insertion, sr, bs);
        m.kind = kind;
        m.preset = preset;
        let fp = FilterParams::new(Loc::InEffect);
        m.efx = Effect::new(kind, insertion, sr, bs, &fp, false);
        if let Some(e) = m.efx.as_mut() {
            e.setpreset(preset, false);
        }
        m
    }

    /// Bring the running effect in line with `cfg`; only rebuilds when
    /// type/preset/filter change, otherwise applies parameter deltas.
    pub fn apply(&mut self, cfg: &EffectCfg, tempo: f32) {
        let rebuild = cfg.kind != self.kind || cfg.preset != self.preset || self.efx.is_none() != (cfg.kind == 0 || cfg.kind > 8);
        let filter_changed = match &self.efx {
            Some(Effect::DynFilter(d)) => format!("{:?}", d.fp) != format!("{:?}", cfg.filter),
            _ => false,
        };
        if rebuild || filter_changed {
            self.kind = cfg.kind;
            self.preset = cfg.preset;
            self.efxoutl.fill(0.0);
            self.efxoutr.fill(0.0);
            self.efx = Effect::new(cfg.kind, self.insertion, self.sr, self.bs, &cfg.filter, true);
            if let Some(e) = self.efx.as_mut() {
                e.setpreset(cfg.preset, true);
            }
            self.settings = [-2; 128];
        }
        let mut settings = cfg.settings;
        if cfg.numerator > 0 && cfg.denominator > 0 {
            match cfg.kind {
                2 | 10 => settings[2] = ((20320.0 / tempo) * (cfg.numerator as f32 / cfg.denominator as f32)).round() as i16,
                3 | 4 | 5 | 8 => {
                    let f = tempo * cfg.denominator as f32 / (240.0 * cfg.numerator as f32);
                    settings[2] = (((f / 0.03) + 1.0).ln() / LOG_2 * 12.7).round().clamp(0.0, 127.0) as i16;
                }
                _ => {}
            }
        }
        if let Some(e) = self.efx.as_mut() {
            for n in 0..128 {
                if settings[n] != self.settings[n] {
                    let v = if settings[n] < 0 { e.presetpar(cfg.preset, n) } else { settings[n] as u8 };
                    e.changepar(n as i32, v);
                }
            }
        }
        self.settings = settings;
    }

    pub fn cleanup(&mut self) {
        if let Some(e) = self.efx.as_mut() {
            e.cleanup();
        }
    }

    /// `EffectMgr::out`: processes `l`/`r` in place.
    pub fn out(&mut self, l: &mut [f32], r: &mut [f32]) {
        let bs = self.bs;
        let Some(e) = self.efx.as_mut() else {
            if !self.insertion {
                l[..bs].fill(0.0);
                r[..bs].fill(0.0);
                self.efxoutl.fill(0.0);
                self.efxoutr.fill(0.0);
            }
            return;
        };
        self.efxoutl.fill(0.0);
        self.efxoutr.fill(0.0);
        e.out(&l[..bs], &r[..bs], &mut self.efxoutl, &mut self.efxoutr);
        let volume = e.base().volume;
        if self.kind == 7 {
            l[..bs].copy_from_slice(&self.efxoutl);
            r[..bs].copy_from_slice(&self.efxoutr);
            return;
        }
        if self.insertion {
            let (v1, mut v2) = if self.const_power {
                ((1.0 - volume).sqrt(), volume.sqrt())
            } else if volume < 0.5 {
                (1.0, volume * 2.0)
            } else {
                ((1.0 - volume) * 2.0, 1.0)
            };
            if self.kind == 1 || self.kind == 2 {
                v2 *= v2;
            }
            if self.dryonly {
                for i in 0..bs {
                    l[i] *= v1;
                    r[i] *= v1;
                    self.efxoutl[i] *= v2;
                    self.efxoutr[i] *= v2;
                }
            } else {
                for i in 0..bs {
                    l[i] = l[i] * v1 + self.efxoutl[i] * v2;
                    r[i] = r[i] * v1 + self.efxoutr[i] * v2;
                }
            }
        } else {
            for i in 0..bs {
                self.efxoutl[i] *= 2.0 * volume;
                self.efxoutr[i] *= 2.0 * volume;
                l[i] = self.efxoutl[i];
                r[i] = self.efxoutr[i];
            }
        }
    }

    /// `sysefxgetvolume`.
    pub fn sysefx_volume(&self) -> f32 {
        self.efx.as_ref().map(|e| e.base().outvolume).unwrap_or(1.0)
    }

    pub fn eq_response(&mut self, freq: f32) -> f32 {
        match self.efx.as_mut() {
            Some(Effect::Eq(e)) => e.freqresponse(freq),
            _ => 0.0,
        }
    }
}

/// Names of the factory presets, per effect type.
pub fn preset_names(kind: u8) -> &'static [&'static str] {
    match kind {
        1 => &[
            "Cathedral 1", "Cathedral 2", "Cathedral 3", "Hall 1", "Hall 2", "Room 1", "Room 2", "Basement", "Tunnel",
            "Echoed 1", "Echoed 2", "Very Long 1", "Very Long 2",
        ],
        2 => &["Echo 1", "Echo 2", "Echo 3", "Simple Echo", "Canyon", "Panning Echo 1", "Panning Echo 2", "Panning Echo 3", "Feedback Echo"],
        3 => &[
            "Chorus 1", "Chorus 2", "Chorus 3", "Celeste 1", "Celeste 2", "Flange 1", "Flange 2", "Flange 3", "Flange 4",
            "Flange 5", "Ensemble", "Dual",
        ],
        4 => &[
            "Phaser 1", "Phaser 2", "Phaser 3", "Phaser 4", "Phaser 5", "Phaser 6", "APhaser 1", "APhaser 2", "APhaser 3",
            "APhaser 4", "APhaser 5", "APhaser 6",
        ],
        5 => &["AlienWah 1", "AlienWah 2", "AlienWah 3", "AlienWah 4"],
        6 => &["Overdrive 1", "Overdrive 2", "A. Exciter 1", "A. Exciter 2", "Guitar Amp", "Quantisize"],
        7 => &["EQ 1", "EQ 2"],
        8 => &["WahWah", "AutoWah", "Sweep", "VocalMorph 1", "VocalMorph 2"],
        _ => &[],
    }
}

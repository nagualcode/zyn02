//! Filters: `Params/FilterParams.cpp` and `DSP/{Filter,AnalogFilter,
//! SVFilter,MoogFilter,CombFilter,FormantFilter,Value_Smoothing_Filter}`.

use super::envelope::Loc;
use super::util::*;
use super::xml::{Version, XmlNode};

/// Largest engine buffer we support (stack scratch size).
pub const MAX_BUF: usize = 1024;

#[derive(Clone, Copy, Debug, Default)]
pub struct Formant {
    pub freq: u8,
    pub amp: u8,
    pub q: u8,
}

#[derive(Clone, Debug)]
pub struct FilterParams {
    pub category: u8,
    pub ftype: u8,
    pub stages: u8,
    pub basefreq: f32,
    pub baseq: f32,
    pub freqtracking: f32,
    pub gain: f32,
    pub numformants: u8,
    pub formantslowness: u8,
    pub vowels: [[Formant; FF_MAX_FORMANTS]; FF_MAX_VOWELS],
    pub sequencesize: u8,
    pub sequence: [u8; FF_MAX_SEQUENCE],
    pub sequencestretch: u8,
    pub sequencereversed: bool,
    pub centerfreq: u8,
    pub octavesfreq: u8,
    pub vowelclearness: u8,
    pub hpf: u8,
    pub lpf: u8,
}

pub fn basefreq_from_old(pfreq: i32) -> f32 {
    2f32.powf((pfreq as f32 / 64.0 - 1.0) * 5.0 + 9.965_784)
}

pub fn baseq_from_old(pq: i32) -> f32 {
    ((pq as f32 / 127.0).powi(2) * 1000f32.ln()).exp() - 0.9
}

impl FilterParams {
    pub fn with(ftype: u8, freq: u8, q: u8) -> Self {
        let mut vowels = [[Formant::default(); FF_MAX_FORMANTS]; FF_MAX_VOWELS];
        for v in vowels.iter_mut() {
            for f in v.iter_mut() {
                *f = Formant { freq: (rnd() * 127.0) as u8, q: 64, amp: 127 };
            }
        }
        let mut sequence = [0; FF_MAX_SEQUENCE];
        for (i, s) in sequence.iter_mut().enumerate() {
            *s = (i % FF_MAX_VOWELS) as u8;
        }
        Self {
            category: 0,
            ftype,
            stages: 0,
            basefreq: basefreq_from_old(freq as i32),
            baseq: baseq_from_old(q as i32),
            freqtracking: 0.0,
            gain: 0.0,
            numformants: 3,
            formantslowness: 64,
            vowels,
            sequencesize: 3,
            sequence,
            sequencestretch: 40,
            sequencereversed: false,
            centerfreq: 64,
            octavesfreq: 64,
            vowelclearness: 64,
            hpf: 0,
            lpf: 127,
        }
    }

    pub fn new(loc: Loc) -> Self {
        match loc {
            Loc::AdGlobalFilter | Loc::SubFilter => Self::with(2, 127, 40),
            Loc::AdVoiceFilter => Self::with(2, 127, 60),
            _ => Self::with(0, 64, 64),
        }
    }

    pub fn getfreq(&self) -> f32 {
        self.basefreq.log2() - 1000f32.log2()
    }
    pub fn getq(&self) -> f32 {
        self.baseq
    }
    pub fn getfreqtracking(&self, notefreq: f32) -> f32 {
        (notefreq / 440.0).log2() * (self.freqtracking / 100.0)
    }
    pub fn getgain(&self) -> f32 {
        self.gain
    }
    pub fn getcenterfreq(&self) -> f32 {
        10000.0 * 10f32.powf(-(1.0 - self.centerfreq as f32 / 127.0) * 2.0)
    }
    pub fn getoctavesfreq(&self) -> f32 {
        0.25 + 10.0 * self.octavesfreq as f32 / 127.0
    }
    pub fn getfreqx(&self, x: f32) -> f32 {
        let x = x.min(1.0);
        let octf = 2f32.powf(self.getoctavesfreq());
        self.getcenterfreq() / octf.sqrt() * octf.powf(x)
    }
    pub fn getformantfreq(&self, f: u8) -> f32 {
        self.getfreqx(f as f32 / 127.0)
    }
    pub fn getformantamp(&self, a: u8) -> f32 {
        0.1f32.powf((1.0 - a as f32 / 127.0) * 4.0)
    }
    pub fn getformantq(&self, q: u8) -> f32 {
        25f32.powf((q as f32 - 32.0) / 64.0)
    }

    pub fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_par("category", self.category as i32);
        x.add_par("type", self.ftype as i32);
        x.add_parreal("basefreq", self.basefreq);
        x.add_parreal("baseq", self.baseq);
        x.add_par("stages", self.stages as i32);
        x.add_parreal("freq_tracking", self.freqtracking);
        x.add_parreal("gain", self.gain);
        let f = x.child("FORMANT_FILTER");
        f.add_par("num_formants", self.numformants as i32);
        f.add_par("formant_slowness", self.formantslowness as i32);
        f.add_par("vowel_clearness", self.vowelclearness as i32);
        f.add_par("center_freq", self.centerfreq as i32);
        f.add_par("octaves_freq", self.octavesfreq as i32);
        for (nv, v) in self.vowels.iter().enumerate() {
            let vn = f.child_id("VOWEL", nv);
            for (nf, fm) in v.iter().enumerate() {
                let n = vn.child_id("FORMANT", nf);
                n.add_par("freq", fm.freq as i32);
                n.add_par("amp", fm.amp as i32);
                n.add_par("q", fm.q as i32);
            }
        }
        f.add_par("sequence_size", self.sequencesize as i32);
        f.add_par("sequence_stretch", self.sequencestretch as i32);
        f.add_parbool("sequence_reversed", self.sequencereversed);
        for (i, s) in self.sequence.iter().enumerate() {
            f.child_id("SEQUENCE_POS", i).add_par("vowel_id", *s as i32);
        }
    }

    pub fn get_from_xml(&mut self, x: &XmlNode, ver: Version) {
        let upgrade = ver < Version(3, 0, 2) && x.parreal("basefreq", -1.0) < 0.0;
        self.category = x.par127("category", self.category);
        self.ftype = x.par127("type", self.ftype);
        self.stages = x.par127("stages", self.stages);
        if upgrade {
            self.basefreq = basefreq_from_old(x.par127("freq", 0) as i32);
            self.baseq = baseq_from_old(x.par127("q", 0) as i32);
            self.gain = (x.par127("gain", 0) as f32 / 64.0 - 1.0) * 30.0;
            self.freqtracking = 100.0 * (x.par127("freq_track", 0) as f32 - 64.0) / 64.0;
        } else {
            self.basefreq = x.parreal("basefreq", 1000.0);
            self.baseq = x.parreal("baseq", 10.0);
            self.gain = x.parreal("gain", 0.0);
            self.freqtracking = x.parreal("freq_tracking", 0.0);
        }
        self.basefreq = self.basefreq.max(31.25);
        if let Some(f) = x.branch("FORMANT_FILTER") {
            self.numformants = f.par127("num_formants", self.numformants);
            self.formantslowness = f.par127("formant_slowness", self.formantslowness);
            self.vowelclearness = f.par127("vowel_clearness", self.vowelclearness);
            self.centerfreq = f.par127("center_freq", self.centerfreq);
            self.octavesfreq = f.par127("octaves_freq", self.octavesfreq);
            for nv in 0..FF_MAX_VOWELS {
                let Some(v) = f.branch_id("VOWEL", nv) else { continue };
                for nf in 0..FF_MAX_FORMANTS {
                    let Some(n) = v.branch_id("FORMANT", nf) else { continue };
                    let fm = &mut self.vowels[nv][nf];
                    fm.freq = n.par127("freq", fm.freq);
                    fm.amp = n.par127("amp", fm.amp);
                    fm.q = n.par127("q", fm.q);
                }
            }
            self.sequencesize = f.par127("sequence_size", self.sequencesize);
            self.sequencestretch = f.par127("sequence_stretch", self.sequencestretch);
            self.sequencereversed = f.parbool("sequence_reversed", self.sequencereversed);
            for i in 0..FF_MAX_SEQUENCE {
                let Some(n) = f.branch_id("SEQUENCE_POS", i) else { continue };
                self.sequence[i] = n.par("vowel_id", self.sequence[i] as i32, 0, FF_MAX_VOWELS as i32 - 1) as u8;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Value_Smoothing_Filter
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct ValueSmoothing {
    w: f32,
    g1: f32,
    g2: f32,
    t: f32,
    cutoff: f32,
    reset_on_next: bool,
}

impl Default for ValueSmoothing {
    fn default() -> Self {
        Self { w: 0.0, g1: 0.0, g2: 0.0, t: 0.0001, cutoff: 10.0, reset_on_next: false }
    }
}

impl ValueSmoothing {
    pub fn sample_rate(&mut self, n: f32) {
        self.w = self.cutoff / (n * 0.05);
    }
    pub fn thresh(&mut self, t: f32) {
        self.t = t;
    }
    pub fn cutoff(&mut self, c: f32) {
        self.cutoff = c;
    }
    pub fn reset(&mut self, v: f32) {
        self.g1 = v;
        self.g2 = v;
    }
    pub fn reset_on_next_apply(&mut self, v: bool) {
        self.reset_on_next = v;
    }
    pub fn apply(&mut self, dst: &mut [f32], gt: f32) -> bool {
        if self.reset_on_next {
            self.reset(gt);
            self.reset_on_next = false;
            return false;
        }
        if gt == self.g2 {
            return false;
        }
        let a = 0.07f32;
        let b = 1.0 + a;
        let gm = b * gt;
        let (mut g1, mut g2) = (self.g1, self.g2);
        for d in dst.iter_mut() {
            g1 += self.w * (gm - g1 - a * g2);
            g2 += self.w * (g1 - g2);
            *d = g2;
        }
        g2 += 1e-10;
        if (gt - g2).abs() < self.t {
            g2 = gt;
        }
        self.g1 = g1;
        self.g2 = g2;
        true
    }
}

// ---------------------------------------------------------------------------
// AnalogFilter
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
pub struct Coeff {
    pub c: [f32; 3],
    pub d: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default)]
struct Stage {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

const MAX_FREQ: f32 = 20000.0;

#[derive(Clone, Debug)]
pub struct AnalogFilter {
    ftype: i32,
    stages: i32,
    freq: f32,
    q: f32,
    newq: f32,
    gain: f32,
    recompute: bool,
    order: i32,
    coeff: Coeff,
    history: [Stage; MAX_FILTER_STAGES + 1],
    freq_smoothing: ValueSmoothing,
    before_first_tick: bool,
    pub outgain: f32,
    sr: f32,
    bufsize: usize,
}

impl AnalogFilter {
    pub fn new(ftype: u8, ffreq: f32, fq: f32, fstages: u8, sr: f32, bufsize: usize) -> Self {
        let mut f = Self {
            ftype: ftype as i32,
            stages: (fstages as i32).min(MAX_FILTER_STAGES as i32),
            freq: ffreq,
            q: fq,
            newq: fq,
            gain: 1.0,
            recompute: true,
            order: 0,
            coeff: Coeff::default(),
            history: [Stage::default(); MAX_FILTER_STAGES + 1],
            freq_smoothing: ValueSmoothing::default(),
            before_first_tick: true,
            outgain: 1.0,
            sr,
            bufsize,
        };
        f.setfreq_and_q(ffreq, fq);
        f.coeff.d[0] = 0.0;
        f.outgain = 1.0;
        f.freq_smoothing.sample_rate(sr / 8.0);
        f.freq_smoothing.thresh(2.0);
        f.before_first_tick = true;
        f
    }

    pub fn cleanup(&mut self) {
        self.history = [Stage::default(); MAX_FILTER_STAGES + 1];
    }

    pub fn compute_coeff(ftype: i32, cutoff: f32, q: f32, stages: i32, gain: f32, fs: f32, order: &mut i32) -> Coeff {
        let mut co = Coeff::default();
        let mut zerocoefs = false;
        let half = fs / 2.0;
        let mut freq = cutoff;
        if freq > half - 500.0 {
            freq = half - 500.0;
            zerocoefs = true;
        }
        if freq < 0.1 {
            freq = 0.1;
        }
        let q = q.max(0.0);
        let (mut tmpq, tmpgain) = if stages == 0 {
            (q, gain)
        } else {
            (if q > 1.0 { q.powf(1.0 / (stages + 1) as f32) } else { q }, gain.powf(1.0 / (stages + 1) as f32))
        };
        let c = &mut co.c;
        let d = &mut co.d;
        let omega = 2.0 * PI * freq / fs;
        let sn = omega.sin();
        let cs = omega.cos();
        match ftype {
            0 => {
                let tmp = if !zerocoefs { (-2.0 * PI * freq / fs).exp() } else { 0.0 };
                c[0] = 1.0 - tmp;
                c[1] = 0.0;
                c[2] = 0.0;
                d[1] = tmp;
                d[2] = 0.0;
                *order = 1;
            }
            1 => {
                let tmp = if !zerocoefs { (-2.0 * PI * freq / fs).exp() } else { 0.0 };
                c[0] = (1.0 + tmp) / 2.0;
                c[1] = -(1.0 + tmp) / 2.0;
                c[2] = 0.0;
                d[1] = tmp;
                d[2] = 0.0;
                *order = 1;
            }
            2 => {
                if !zerocoefs {
                    let alpha = sn / (2.0 * tmpq);
                    let tmp = 1.0 + alpha;
                    c[1] = (1.0 - cs) / tmp;
                    c[0] = c[1] / 2.0;
                    c[2] = c[1] / 2.0;
                    d[1] = -2.0 * cs / tmp * -1.0;
                    d[2] = (1.0 - alpha) / tmp * -1.0;
                } else {
                    c[0] = 1.0;
                }
                *order = 2;
            }
            3 => {
                if !zerocoefs {
                    let alpha = sn / (2.0 * tmpq);
                    let tmp = 1.0 + alpha;
                    c[0] = (1.0 + cs) / 2.0 / tmp;
                    c[1] = -(1.0 + cs) / tmp;
                    c[2] = (1.0 + cs) / 2.0 / tmp;
                    // `- 1e-6` is a double subtraction in Zyn.
                    d[1] = ((-2.0 * cs / tmp * -1.0) as f64 - 1e-6) as f32;
                    d[2] = (((1.0 - alpha) / tmp * -1.0) as f64 - 1e-6) as f32;
                }
                *order = 2;
            }
            4 => {
                if !zerocoefs {
                    let alpha = sn / (2.0 * tmpq);
                    let tmp = 1.0 + alpha;
                    c[0] = alpha / tmp * (tmpq + 1.0).sqrt();
                    c[1] = 0.0;
                    c[2] = -alpha / tmp * (tmpq + 1.0).sqrt();
                    d[1] = -2.0 * cs / tmp * -1.0;
                    d[2] = (1.0 - alpha) / tmp * -1.0;
                }
                *order = 2;
            }
            5 => {
                if !zerocoefs {
                    let alpha = sn / (2.0 * tmpq.sqrt());
                    let tmp = 1.0 + alpha;
                    c[0] = 1.0 / tmp;
                    c[1] = -2.0 * cs / tmp;
                    c[2] = 1.0 / tmp;
                    d[1] = -2.0 * cs / tmp * -1.0;
                    d[2] = (1.0 - alpha) / tmp * -1.0;
                } else {
                    c[0] = 1.0;
                }
                *order = 2;
            }
            6 => {
                if !zerocoefs {
                    tmpq *= 3.0;
                    let alpha = sn / (2.0 * tmpq);
                    let tmp = 1.0 + alpha / tmpgain;
                    c[0] = (1.0 + alpha * tmpgain) / tmp;
                    c[1] = (-2.0 * cs) / tmp;
                    c[2] = (1.0 - alpha * tmpgain) / tmp;
                    d[1] = -2.0 * cs / tmp * -1.0;
                    d[2] = (1.0 - alpha / tmpgain) / tmp * -1.0;
                } else {
                    c[0] = 1.0;
                }
                *order = 2;
            }
            7 => {
                if !zerocoefs {
                    tmpq = tmpq.sqrt();
                    let beta = tmpgain.sqrt() / tmpq;
                    let tgp1 = tmpgain + 1.0;
                    let tgm1 = tmpgain - 1.0;
                    let tmp = tgp1 + tgm1 * cs + beta * sn;
                    c[0] = tmpgain * (tgp1 - tgm1 * cs + beta * sn) / tmp;
                    c[1] = 2.0 * tmpgain * (tgm1 - tgp1 * cs) / tmp;
                    c[2] = tmpgain * (tgp1 - tgm1 * cs - beta * sn) / tmp;
                    d[1] = -2.0 * (tgm1 + tgp1 * cs) / tmp * -1.0;
                    d[2] = (tgp1 + tgm1 * cs - beta * sn) / tmp * -1.0;
                } else {
                    c[0] = tmpgain;
                }
                *order = 2;
            }
            8 => {
                if !zerocoefs {
                    tmpq = tmpq.sqrt();
                    let beta = tmpgain.sqrt() / tmpq;
                    let tgp1 = tmpgain + 1.0;
                    let tgm1 = tmpgain - 1.0;
                    let tmp = tgp1 - tgm1 * cs + beta * sn;
                    c[0] = tmpgain * (tgp1 + tgm1 * cs + beta * sn) / tmp;
                    c[1] = -2.0 * tmpgain * (tgm1 + tgp1 * cs) / tmp;
                    c[2] = tmpgain * (tgp1 + tgm1 * cs - beta * sn) / tmp;
                    d[1] = 2.0 * (tgm1 - tgp1 * cs) / tmp * -1.0;
                    d[2] = (tgp1 - tgm1 * cs - beta * sn) / tmp * -1.0;
                } else {
                    c[0] = 1.0;
                }
                *order = 2;
            }
            _ => {
                c[0] = 1.0;
                *order = 1;
            }
        }
        co
    }

    fn computefiltercoefs(&mut self, freq: f32, q: f32) {
        let mut order = self.order;
        self.coeff = Self::compute_coeff(self.ftype, freq, q, self.stages, self.gain, self.sr, &mut order);
        self.order = order;
    }

    pub fn setfreq(&mut self, frequency: f32) {
        let mut frequency = frequency.clamp(0.1, MAX_FREQ);
        if frequency.is_nan() {
            frequency = 0.1;
        }
        let frequency = frequency.ceil();
        if (frequency - self.freq).abs() >= 1.0 {
            self.freq = frequency;
            self.recompute = true;
        }
        if self.recompute {
            self.q = self.newq;
        }
        if self.before_first_tick {
            self.freq_smoothing.reset(self.freq);
            self.before_first_tick = false;
        }
    }

    pub fn setfreq_and_q(&mut self, frequency: f32, q: f32) {
        self.newq = q;
        let cur = self.q;
        if cur == 0.0 || q == 0.0 || (if cur > q { cur / q } else { q / cur }) as f64 > 1.1 {
            self.recompute = true;
        }
        self.setfreq(frequency);
    }

    pub fn setq(&mut self, q: f32) {
        self.newq = q;
        self.q = q;
        self.computefiltercoefs(self.freq, self.q);
    }

    pub fn settype(&mut self, t: i32) {
        self.ftype = t;
        self.computefiltercoefs(self.freq, self.q);
    }

    pub fn setgain(&mut self, db: f32) {
        self.gain = db2rap(db);
        self.computefiltercoefs(self.freq, self.q);
    }

    pub fn setstages(&mut self, s: i32) {
        let s = s.min(MAX_FILTER_STAGES as i32 - 1);
        if s != self.stages {
            self.stages = s;
            self.cleanup();
            self.computefiltercoefs(self.freq, self.q);
        }
    }

    /// One first-order sample (used by the comb filter's damping).
    pub fn filter_sample(&mut self, smp: f32) -> f32 {
        if self.recompute {
            self.computefiltercoefs(self.freq, self.q);
            self.recompute = false;
        }
        let h = &mut self.history[0];
        let y0 = smp * self.coeff.c[0] + h.x1 * self.coeff.c[1] + h.y1 * self.coeff.d[1];
        h.y1 = y0;
        h.x1 = smp;
        y0
    }

    fn singlefilterout(&mut self, smp: &mut [f32], stage: usize, f: f32) {
        if self.recompute {
            self.computefiltercoefs(f, self.q);
            self.recompute = false;
        }
        let co = self.coeff;
        let h = &mut self.history[stage];
        if self.order == 1 {
            for s in smp.iter_mut() {
                let y0 = *s * co.c[0] + h.x1 * co.c[1] + h.y1 * co.d[1];
                h.y1 = y0;
                h.x1 = *s;
                *s = y0;
            }
        } else if self.order == 2 {
            let k = [co.c[0], co.c[1], co.c[2], co.d[1], co.d[2]];
            let mut w = [h.x1, h.x2, h.y1, h.y2];
            for pair in smp.chunks_mut(2) {
                // AnalogBiquadFilterA
                w[3] = pair[0] * k[0] + w[0] * k[1] + w[1] * k[2] + w[2] * k[3] + w[3] * k[4];
                w[1] = pair[0];
                pair[0] = w[3];
                if pair.len() > 1 {
                    // AnalogBiquadFilterB
                    w[2] = pair[1] * k[0] + w[1] * k[1] + w[0] * k[2] + w[3] * k[3] + w[2] * k[4];
                    w[0] = pair[1];
                    pair[1] = w[2];
                }
            }
            h.x1 = w[0];
            h.x2 = w[1];
            h.y1 = w[2];
            h.y2 = w[3];
        }
    }

    pub fn filterout(&mut self, smp: &mut [f32]) {
        let n = self.bufsize.min(smp.len());
        let fbs = n / 8;
        let mut freqbuf = [0.0f32; MAX_BUF / 8];
        let target = self.freq;
        if self.freq_smoothing.apply(&mut freqbuf[..fbs], target) {
            for i in 0..=(self.stages as usize) {
                for j in 0..fbs {
                    self.recompute = true;
                    let f = freqbuf[j];
                    self.singlefilterout(&mut smp[j * 8..j * 8 + 8], i, f);
                }
            }
        } else {
            for i in 0..=(self.stages as usize) {
                self.singlefilterout(&mut smp[..n], i, target);
            }
        }
        let g = self.outgain;
        for s in smp[..n].iter_mut() {
            *s *= g;
        }
    }

    /// Magnitude response, for the editor's filter graph.
    pub fn h(&self, freq: f32) -> f32 {
        let fr = freq / self.sr * PI * 2.0;
        let c = &self.coeff;
        let (mut x, mut y) = (c.c[0], 0.0);
        for n in 1..3 {
            x += (n as f32 * fr).cos() * c.c[n];
            y -= (n as f32 * fr).sin() * c.c[n];
        }
        let mut h = x * x + y * y;
        let (mut x, mut y) = (1.0, 0.0);
        for n in 1..3 {
            x -= (n as f32 * fr).cos() * c.d[n];
            y += (n as f32 * fr).sin() * c.d[n];
        }
        h /= x * x + y * y;
        h.powf((self.stages as f32 + 1.0) / 2.0)
    }

    /// Make sure coefficients are current (for `h`).
    pub fn refresh(&mut self) {
        let (f, q) = (self.freq, self.q);
        self.computefiltercoefs(f, q);
        self.recompute = false;
    }
}

// ---------------------------------------------------------------------------
// SVFilter
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
struct SvStage {
    low: f32,
    high: f32,
    band: f32,
    notch: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct SvPar {
    f: f32,
    q: f32,
    q_sqrt: f32,
}

#[derive(Clone, Debug)]
pub struct SvFilter {
    ftype: i32,
    stages: i32,
    freq: f32,
    q: f32,
    gain: f32,
    st: [SvStage; MAX_FILTER_STAGES + 1],
    par: SvPar,
    freq_smoothing: ValueSmoothing,
    pub outgain: f32,
    sr: f32,
    bufsize: usize,
}

impl SvFilter {
    pub fn new(ftype: u8, ffreq: f32, fq: f32, fstages: u8, sr: f32, bufsize: usize) -> Self {
        let mut f = Self {
            ftype: ftype as i32,
            stages: (fstages as i32).min(MAX_FILTER_STAGES as i32),
            freq: ffreq,
            q: fq,
            gain: 1.0,
            st: [SvStage::default(); MAX_FILTER_STAGES + 1],
            par: SvPar::default(),
            freq_smoothing: ValueSmoothing::default(),
            outgain: 1.0,
            sr,
            bufsize,
        };
        f.setfreq_and_q(ffreq, fq);
        f.freq_smoothing.reset(ffreq);
        f.freq_smoothing.sample_rate(sr);
        f
    }

    fn computefiltercoefs(&mut self) {
        self.par.f = (self.freq / self.sr * 4.0).min(0.99999);
        self.par.q = 1.0 - self.q.sqrt().atan() * 2.0 / PI;
        self.par.q = self.par.q.powf(1.0 / (self.stages + 1) as f32);
        self.par.q_sqrt = self.par.q.sqrt();
    }

    pub fn setfreq(&mut self, frequency: f32) {
        self.freq = frequency.max(0.1);
        self.computefiltercoefs();
    }
    pub fn setfreq_and_q(&mut self, frequency: f32, q: f32) {
        self.q = q;
        self.setfreq(frequency);
    }
    pub fn setq(&mut self, q: f32) {
        self.q = q;
        self.computefiltercoefs();
    }
    pub fn setgain(&mut self, db: f32) {
        self.gain = db2rap(db);
        self.computefiltercoefs();
    }
    pub fn settype(&mut self, t: i32) {
        self.ftype = t;
        self.computefiltercoefs();
    }
    pub fn setstages(&mut self, s: i32) {
        let s = s.min(MAX_FILTER_STAGES as i32 - 1);
        if s != self.stages {
            self.stages = s;
            self.st = [SvStage::default(); MAX_FILTER_STAGES + 1];
            self.computefiltercoefs();
        }
    }

    fn single(st: &mut SvStage, par: SvPar, ftype: i32, smp: &mut [f32]) {
        for s in smp.iter_mut() {
            st.low += par.f * st.band;
            st.high = par.q_sqrt * *s - st.low - par.q * st.band;
            st.band = par.f * st.high + st.band;
            st.notch = st.high + st.low;
            *s = match ftype {
                1 => st.high,
                2 => st.band,
                3 => st.notch,
                _ => st.low,
            };
        }
    }

    pub fn filterout(&mut self, smp: &mut [f32]) {
        let n = self.bufsize.min(smp.len());
        let mut freqbuf = [0.0f32; MAX_BUF];
        let target = self.freq;
        if self.freq_smoothing.apply(&mut freqbuf[..n], target) {
            let mut i = 0;
            while i < n {
                self.freq = freqbuf[i];
                self.computefiltercoefs();
                let end = (i + 8).min(n);
                for j in 0..=(self.stages as usize) {
                    Self::single(&mut self.st[j], self.par, self.ftype, &mut smp[i..end]);
                }
                i += 8;
            }
            self.freq = freqbuf[n - 1];
            self.computefiltercoefs();
        } else {
            for j in 0..=(self.stages as usize) {
                Self::single(&mut self.st[j], self.par, self.ftype, &mut smp[..n]);
            }
        }
        let g = self.outgain;
        for s in smp[..n].iter_mut() {
            *s *= g;
        }
    }
}

// ---------------------------------------------------------------------------
// MoogFilter
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct MoogFilter {
    sr: f32,
    gain: f32,
    feedback_gain: f32,
    a: [f32; 5],
    state: [f32; 4],
    passband_comp: f32,
    c: f32,
    ct2: f32,
    cp2: f32,
    cp3: f32,
    cp4: f32,
    pub outgain: f32,
    bufsize: usize,
}

#[inline]
fn tanh_x(x: f32) -> f32 {
    let x2 = x * x;
    x * (105.0 + 10.0 * x2) / (105.0 + (45.0 + x2) * x2)
}

impl MoogFilter {
    pub fn new(ftype: u8, ffreq: f32, fq: f32, sr: f32, bufsize: usize) -> Self {
        let mut f = Self {
            sr,
            gain: 1.0,
            feedback_gain: 0.0,
            a: [0.0; 5],
            state: [0.0001; 4],
            passband_comp: 1.0,
            c: 0.0,
            ct2: 0.0,
            cp2: 0.0,
            cp3: 0.0,
            cp4: 0.0,
            outgain: 1.0,
            bufsize,
        };
        f.setfreq_and_q(ffreq / sr, fq);
        f.settype(ftype);
        f
    }

    #[inline]
    fn tanh_xdiv_x(x: f32) -> f32 {
        let x = x + 0.1;
        let x2 = x * x;
        1.0 - 0.35 * x2 + 0.06 * x2 * x2
    }

    #[inline]
    fn step(&mut self, input: f32) -> f32 {
        let s = &mut self.state;
        let c = self.c;
        let gm0 = Self::tanh_xdiv_x(s[0]);
        let d0 = 1.0 / (1.0 + c * gm0);
        let dp1 = 1.0 / (1.0 + c);
        let dp2 = dp1 * dp1;
        let dp3 = dp2 * dp1;
        let y3e = self.cp4 * d0 * gm0 * dp3 * input
            + self.cp3 * gm0 * dp3 * d0 * s[0]
            + self.cp2 * dp3 * s[1]
            + c * dp2 * s[2]
            + dp1 * s[3];
        let u = input - tanh_x(self.feedback_gain * (y3e - 0.5 * input));
        let y0 = gm0 * d0 * (s[0] + c * u);
        let y1 = dp1 * (s[1] + c * y0);
        let y2 = dp1 * (s[2] + c * y1);
        let y3 = dp1 * (s[3] + c * y2);
        s[0] += self.ct2 * (u - y0);
        s[1] += self.ct2 * (y0 - y1);
        s[2] += self.ct2 * (y1 - y2);
        s[3] += self.ct2 * (y2 - y3);
        let a = &self.a;
        a[0] * u + a[1] * y0 + a[2] * y1 + a[3] * y2 + a[4] * y3
    }

    pub fn filterout(&mut self, smp: &mut [f32]) {
        let n = self.bufsize.min(smp.len());
        for s in smp[..n].iter_mut() {
            let v = tanh_x(*s * self.gain);
            *s = self.step(v) * self.outgain;
        }
    }

    pub fn setfreq_and_q(&mut self, frequency: f32, q: f32) {
        self.setfreq(frequency / self.sr);
        self.setq(q);
    }

    /// Normalised frequency (cycles per sample), as in Zyn.
    pub fn setfreq(&mut self, ff: f32) {
        let x = PI * ff;
        let x2 = x * x;
        self.c = (x + 0.15 * x2 + 0.3 * x2 * x2).clamp(0.0006, 1.5);
        self.ct2 = self.c * 2.0;
        self.cp2 = self.c * self.c;
        self.cp3 = self.cp2 * self.c;
        self.cp4 = self.cp2 * self.cp2;
    }

    pub fn setq(&mut self, q: f32) {
        self.feedback_gain = (q / 1000.0).cbrt() * 4.0 + 0.3;
        self.passband_comp = 1.0 + self.feedback_gain.clamp(0.0, 1.0);
    }

    pub fn setgain(&mut self, db: f32) {
        self.gain = db2rap(db);
    }

    pub fn settype(&mut self, t: u8) {
        self.a = match t {
            0 => [1.0, -4.0, 6.0, -4.0, 1.0],
            1 => [0.0, 0.0, 4.0, -8.0, 4.0],
            _ => [0.0, 0.0, 0.0, 0.0, self.passband_comp],
        };
    }
}

// ---------------------------------------------------------------------------
// CombFilter
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct CombFilter {
    input: Vec<f32>,
    output: Vec<f32>,
    gain: f32,
    q: f32,
    ftype: u8,
    plpf: u8,
    phpf: u8,
    gainfwd: f32,
    gainbwd: f32,
    delay: f32,
    lpf_delay: f32,
    lpf: Option<AnalogFilter>,
    hpf: Option<AnalogFilter>,
    input_index: usize,
    output_index: usize,
    mem_size: usize,
    pub outgain: f32,
    sr: f32,
    bufsize: usize,
}

impl CombFilter {
    pub fn new(ftype: u8, ffreq: f32, fq: f32, sr: f32, bufsize: usize, plpf: u8, phpf: u8) -> Self {
        let mem_size = (sr / 25.0).ceil() as usize + bufsize + 2;
        let mut f = Self {
            input: vec![0.0; mem_size],
            output: vec![0.0; mem_size],
            gain: 1.0,
            q: fq,
            ftype,
            plpf: 127,
            phpf: 0,
            gainfwd: 0.0,
            gainbwd: 0.0,
            delay: 0.0,
            lpf_delay: 0.0,
            lpf: None,
            hpf: None,
            input_index: 0,
            output_index: 0,
            mem_size,
            outgain: 1.0,
            sr,
            bufsize,
        };
        f.setfreq_and_q(ffreq, fq);
        f.settype(ftype);
        f.setlpf(plpf);
        f.sethpf(phpf);
        f
    }

    fn sample_lerp(&self, buf: &[f32], pos: f32) -> f32 {
        let poshi = pos as usize;
        let poslo = pos - poshi as f32;
        let next = (poshi + 1) % self.mem_size;
        buf[poshi] + poslo * (buf[next] - buf[poshi])
    }

    pub fn filterout(&mut self, smp: &mut [f32]) {
        let n = self.bufsize.min(smp.len());
        let ms = self.mem_size;
        let end = ms - self.input_index;
        if end < n {
            self.input[self.input_index..].copy_from_slice(&smp[..end]);
            self.input[..n - end].copy_from_slice(&smp[end..n]);
        } else {
            self.input[self.input_index..self.input_index + n].copy_from_slice(&smp[..n]);
        }
        for i in 0..n {
            let ipos = (self.input_index as f32 + i as f32 - self.delay + ms as f32) % ms as f32;
            let opos = (self.output_index as f32 - self.delay + ms as f32) % ms as f32;
            let mut fb = tanh_x(self.gainfwd * self.sample_lerp(&self.input, ipos) - self.gainbwd * self.sample_lerp(&self.output, opos));
            if let Some(l) = self.lpf.as_mut() {
                fb = l.filter_sample(fb);
            }
            if let Some(h) = self.hpf.as_mut() {
                fb = h.filter_sample(fb);
            }
            smp[i] = smp[i] * self.gain + fb;
            self.output[self.output_index] = smp[i];
            smp[i] *= self.outgain;
            self.output_index = (self.output_index + 1) % ms;
        }
        self.input_index = (self.input_index + n) % ms;
    }

    pub fn setfreq_and_q(&mut self, freq: f32, q: f32) {
        self.setfreq(freq);
        self.setq(q);
    }
    pub fn setfreq(&mut self, freq: f32) {
        let ff = freq.clamp(25.0, 40000.0);
        self.delay = self.sr / ff - self.lpf_delay;
    }
    pub fn setq(&mut self, q: f32) {
        self.q = (0.0015 * q).cbrt();
        self.settype(self.ftype);
    }
    pub fn setgain(&mut self, db: f32) {
        self.gain = db2rap(db);
    }
    pub fn settype(&mut self, t: u8) {
        self.ftype = t;
        let q = self.q;
        (self.gainfwd, self.gainbwd) = match t {
            1 => (q, 0.0),
            2 => (q, q),
            3 => (0.0, -q),
            4 => (-q, 0.0),
            5 => (-q, -q),
            _ => (0.0, q),
        };
    }
    pub fn sethpf(&mut self, p: u8) {
        if self.phpf == p {
            return;
        }
        self.phpf = p;
        if p == 0 {
            self.hpf = None;
        } else {
            let fr = ((p as f32 / 127.0).sqrt() * 10000f32.ln()).exp() + 20.0;
            match self.hpf.as_mut() {
                Some(h) => h.setfreq(fr),
                None => self.hpf = Some(AnalogFilter::new(1, fr, 1.0, 0, self.sr, self.bufsize)),
            }
        }
    }
    pub fn setlpf(&mut self, p: u8) {
        if self.plpf == p {
            return;
        }
        self.plpf = p;
        if p == 127 {
            self.lpf = None;
            self.lpf_delay = 0.0;
        } else {
            let fr = ((p as f32 / 127.0).sqrt() * 25000f32.ln()).exp() + 40.0;
            match self.lpf.as_mut() {
                Some(l) => l.setfreq(fr),
                None => self.lpf = Some(AnalogFilter::new(0, fr, 1.0, 0, self.sr, self.bufsize)),
            }
            let a = (-2.0 * PI * fr / self.sr).exp();
            self.lpf_delay = a / (1.0 - a);
        }
    }
}

// ---------------------------------------------------------------------------
// FormantFilter
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
struct FPar {
    freq: f32,
    amp: f32,
    q: f32,
}

#[derive(Clone, Debug)]
pub struct FormantFilter {
    formant: Vec<AnalogFilter>,
    formantpar: [[FPar; FF_MAX_FORMANTS]; FF_MAX_VOWELS],
    current: [FPar; FF_MAX_FORMANTS],
    amp_smoothing: [ValueSmoothing; FF_MAX_FORMANTS],
    sequence: [u8; FF_MAX_SEQUENCE],
    sequencesize: usize,
    numformants: usize,
    firsttime: bool,
    oldinput: f32,
    slowinput: f32,
    qfactor: f32,
    formantslowness: f32,
    oldqfactor: f32,
    vowelclearness: f32,
    sequencestretch: f32,
    pub outgain: f32,
    bufsize: usize,
}

impl FormantFilter {
    pub fn new(p: &FilterParams, sr: f32, bufsize: usize) -> Self {
        let numformants = (p.numformants as usize).min(FF_MAX_FORMANTS);
        let formant = (0..numformants).map(|_| AnalogFilter::new(4, 1000.0, 10.0, p.stages, sr, bufsize)).collect();
        let mut formantpar = [[FPar::default(); FF_MAX_FORMANTS]; FF_MAX_VOWELS];
        for j in 0..FF_MAX_VOWELS {
            for i in 0..numformants {
                let v = p.vowels[j][i];
                formantpar[j][i] = FPar { freq: p.getformantfreq(v.freq), amp: p.getformantamp(v.amp), q: p.getformantq(v.q) };
            }
        }
        let mut amp_smoothing = [ValueSmoothing::default(); FF_MAX_FORMANTS];
        for s in amp_smoothing.iter_mut() {
            s.sample_rate(sr);
            s.reset(1.0);
        }
        let mut current = [FPar::default(); FF_MAX_FORMANTS];
        for c in current.iter_mut().take(numformants) {
            *c = FPar { freq: 1000.0, amp: 1.0, q: 2.0 };
        }
        let sequencesize = (p.sequencesize as usize).clamp(1, FF_MAX_SEQUENCE);
        let mut sequencestretch = 0.1f32.powf((p.sequencestretch as f32 - 32.0) / 48.0);
        if p.sequencereversed {
            sequencestretch *= -1.0;
        }
        Self {
            formant,
            formantpar,
            current,
            amp_smoothing,
            sequence: p.sequence,
            sequencesize,
            numformants,
            firsttime: true,
            oldinput: -1.0,
            slowinput: 0.0,
            qfactor: p.getq(),
            formantslowness: (1.0 - p.formantslowness as f32 / 128.0).powi(3),
            oldqfactor: p.getq(),
            vowelclearness: 10f32.powf((p.vowelclearness as f32 - 32.0) / 48.0),
            sequencestretch,
            outgain: db2rap(p.getgain()),
            bufsize,
        }
    }

    fn setpos(&mut self, frequency: f32) {
        let input = frequency.ln() / 2f32.ln() - 9.965_784;
        if self.firsttime {
            self.slowinput = input;
        } else {
            self.slowinput = self.slowinput * (1.0 - self.formantslowness) + input * self.formantslowness;
        }
        if (self.oldinput - input).abs() < 0.001
            && (self.slowinput - input).abs() < 0.001
            && (self.qfactor - self.oldqfactor).abs() < 0.001
        {
            self.firsttime = false;
            return;
        }
        self.oldinput = input;
        let mut pos = input * self.sequencestretch;
        pos -= pos.floor();
        let p2 = f2i(pos * self.sequencesize as f32).clamp(0, self.sequencesize as i32 - 1);
        let mut p1 = p2 - 1;
        if p1 < 0 {
            p1 += self.sequencesize as i32;
        }
        pos *= self.sequencesize as f32;
        pos -= pos.floor();
        pos = ((pos * 2.0 - 1.0) * self.vowelclearness).atan() / self.vowelclearness.atan();
        pos = (pos + 1.0) * 0.5;
        let p1 = (self.sequence[p1 as usize] as usize).min(FF_MAX_VOWELS - 1);
        let p2 = (self.sequence[p2 as usize] as usize).min(FF_MAX_VOWELS - 1);
        let fs = self.formantslowness;
        for i in 0..self.numformants {
            let a = self.formantpar[p1][i];
            let b = self.formantpar[p2][i];
            let tf = a.freq * (1.0 - pos) + b.freq * pos;
            let ta = a.amp * (1.0 - pos) + b.amp * pos;
            let tq = a.q * (1.0 - pos) + b.q * pos;
            let c = &mut self.current[i];
            if self.firsttime {
                *c = FPar { freq: tf, amp: ta, q: tq };
            } else {
                c.freq = c.freq * (1.0 - fs) + tf * fs;
                c.amp = c.amp * (1.0 - fs) + ta * fs;
                c.q = c.q * (1.0 - fs) + tq * fs;
            }
            let (f, q) = (c.freq, c.q * self.qfactor);
            self.formant[i].setfreq_and_q(f, q);
        }
        self.firsttime = false;
        self.oldqfactor = self.qfactor;
    }

    pub fn setfreq(&mut self, f: f32) {
        self.setpos(f);
    }
    pub fn setq(&mut self, q: f32) {
        self.qfactor = q;
        for i in 0..self.numformants {
            let v = self.qfactor * self.current[i].q;
            self.formant[i].setq(v);
        }
    }
    pub fn setfreq_and_q(&mut self, f: f32, q: f32) {
        self.qfactor = q;
        self.setpos(f);
    }

    pub fn filterout(&mut self, smp: &mut [f32]) {
        let n = self.bufsize.min(smp.len());
        let mut inbuf = [0.0f32; MAX_BUF];
        inbuf[..n].copy_from_slice(&smp[..n]);
        smp[..n].fill(0.0);
        let mut fbuf = [0.0f32; MAX_BUF];
        let mut tmp = [0.0f32; MAX_BUF];
        for j in 0..self.numformants {
            for i in 0..n {
                tmp[i] = inbuf[i] * self.outgain;
            }
            self.formant[j].filterout(&mut tmp[..n]);
            let amp = self.current[j].amp;
            if self.amp_smoothing[j].apply(&mut fbuf[..n], amp) {
                for i in 0..n {
                    smp[i] += tmp[i] * fbuf[i];
                }
            } else {
                for i in 0..n {
                    smp[i] += tmp[i] * amp;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Filter (the polymorphic `Filter*` of Zyn)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Filter {
    Analog(AnalogFilter),
    Formant(Box<FormantFilter>),
    Sv(SvFilter),
    Moog(MoogFilter),
    Comb(Box<CombFilter>),
}

impl Filter {
    /// `Filter::generate`.
    pub fn generate(p: &FilterParams, sr: f32, bufsize: usize) -> Self {
        let ft = p.ftype;
        match p.category {
            1 => Filter::Formant(Box::new(FormantFilter::new(p, sr, bufsize))),
            2 => {
                let mut f = SvFilter::new(ft, 1000.0, p.getq(), p.stages, sr, bufsize);
                f.outgain = db2rap(p.getgain());
                if f.outgain > 1.0 {
                    f.outgain = f.outgain.sqrt();
                }
                Filter::Sv(f)
            }
            3 => {
                let mut f = MoogFilter::new(ft, 1000.0, p.getq(), sr, bufsize);
                f.setgain(p.getgain());
                Filter::Moog(f)
            }
            4 => {
                let mut f = CombFilter::new(ft, 1000.0, p.getq(), sr, bufsize, p.lpf, p.hpf);
                f.setgain(p.getgain());
                Filter::Comb(Box::new(f))
            }
            _ => {
                let mut f = AnalogFilter::new(ft, 1000.0, p.getq(), p.stages, sr, bufsize);
                if (6..=8).contains(&ft) {
                    f.setgain(p.getgain());
                } else {
                    f.outgain = db2rap(p.getgain());
                }
                Filter::Analog(f)
            }
        }
    }

    pub fn getrealfreq(freqpitch: f32) -> f32 {
        2f32.powf(freqpitch + 9.965_784)
    }

    pub fn filterout(&mut self, smp: &mut [f32]) {
        match self {
            Filter::Analog(f) => f.filterout(smp),
            Filter::Formant(f) => f.filterout(smp),
            Filter::Sv(f) => f.filterout(smp),
            Filter::Moog(f) => f.filterout(smp),
            Filter::Comb(f) => f.filterout(smp),
        }
    }

    pub fn setfreq(&mut self, freq: f32) {
        match self {
            Filter::Analog(f) => f.setfreq(freq),
            Filter::Formant(f) => f.setfreq(freq),
            Filter::Sv(f) => f.setfreq(freq),
            Filter::Moog(f) => f.setfreq(freq),
            Filter::Comb(f) => f.setfreq(freq),
        }
    }

    pub fn setfreq_and_q(&mut self, freq: f32, q: f32) {
        match self {
            Filter::Analog(f) => f.setfreq_and_q(freq, q),
            Filter::Formant(f) => f.setfreq_and_q(freq, q),
            Filter::Sv(f) => f.setfreq_and_q(freq, q),
            Filter::Moog(f) => f.setfreq_and_q(freq, q),
            Filter::Comb(f) => f.setfreq_and_q(freq, q),
        }
    }

    pub fn setq(&mut self, q: f32) {
        match self {
            Filter::Analog(f) => f.setq(q),
            Filter::Formant(f) => f.setq(q),
            Filter::Sv(f) => f.setq(q),
            Filter::Moog(f) => f.setq(q),
            Filter::Comb(f) => f.setq(q),
        }
    }

    pub fn setgain(&mut self, db: f32) {
        match self {
            Filter::Analog(f) => f.setgain(db),
            Filter::Formant(_) => {}
            Filter::Sv(f) => f.setgain(db),
            Filter::Moog(f) => f.setgain(db),
            Filter::Comb(f) => f.setgain(db),
        }
    }
}

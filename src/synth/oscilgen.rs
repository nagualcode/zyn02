//! `Synth/OscilGen.cpp`: the harmonic oscillator designer behind every
//! ADsynth voice and the PADsynth harmonic profile.
//!
//! The spectrum is built once from the parameters (`prepare`, done off the
//! audio thread when a patch is loaded or edited) and then rendered per
//! note by `get`, which applies phase/amplitude randomness, adaptive
//! harmonics, band-limiting and resonance before the inverse FFT.

use super::fft::{C32, Fft};
use super::resonance::Resonance;
use super::util::*;
use super::waveshape::wave_shape_smps;
use super::xml::XmlNode;

#[derive(Clone, Debug)]
pub struct OscilParams {
    pub hmag: [u8; MAX_AD_HARMONICS],
    pub hphase: [u8; MAX_AD_HARMONICS],
    pub hmagtype: u8,
    pub basefunc: u8,
    pub basefuncpar: u8,
    pub basefuncmodulation: u8,
    pub basefuncmodulationpar1: u8,
    pub basefuncmodulationpar2: u8,
    pub basefuncmodulationpar3: u8,
    pub waveshaping: u8,
    pub waveshapingfunction: u8,
    pub filtertype: u8,
    pub filterpar1: u8,
    pub filterpar2: u8,
    pub filterbeforews: bool,
    pub satype: u8,
    pub sapar: u8,
    pub harmonicshift: i32,
    pub harmonicshiftfirst: bool,
    pub modulation: u8,
    pub modulationpar1: u8,
    pub modulationpar2: u8,
    pub modulationpar3: u8,
    pub rand: u8,
    pub amprandpower: u8,
    pub amprandtype: u8,
    pub adaptiveharmonics: u8,
    pub adaptiveharmonicsbasefreq: u8,
    pub adaptiveharmonicspower: u8,
    pub adaptiveharmonicspar: u8,
    /// User base function (base_function 127): bins 0..oscilsize/2.
    pub custom_base: Vec<C32>,
}

impl OscilParams {
    pub fn new(ad_vs_pad: bool) -> Self {
        let mut hmag = [64; MAX_AD_HARMONICS];
        hmag[0] = 127;
        Self {
            hmag,
            hphase: [64; MAX_AD_HARMONICS],
            hmagtype: 0,
            basefunc: 0,
            basefuncpar: 64,
            basefuncmodulation: 0,
            basefuncmodulationpar1: 64,
            basefuncmodulationpar2: 64,
            basefuncmodulationpar3: 32,
            waveshaping: 64,
            waveshapingfunction: 0,
            filtertype: 0,
            filterpar1: 64,
            filterpar2: 64,
            filterbeforews: false,
            satype: 0,
            sapar: 64,
            harmonicshift: 0,
            harmonicshiftfirst: false,
            modulation: 0,
            modulationpar1: 64,
            modulationpar2: 64,
            modulationpar3: 32,
            rand: if ad_vs_pad { 127 } else { 64 },
            amprandpower: 64,
            amprandtype: 0,
            adaptiveharmonics: 0,
            adaptiveharmonicsbasefreq: 128,
            adaptiveharmonicspower: 100,
            adaptiveharmonicspar: 50,
            custom_base: Vec::new(),
        }
    }

    pub fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_par("harmonic_mag_type", self.hmagtype as i32);
        x.add_par("base_function", self.basefunc as i32);
        x.add_par("base_function_par", self.basefuncpar as i32);
        x.add_par("base_function_modulation", self.basefuncmodulation as i32);
        x.add_par("base_function_modulation_par1", self.basefuncmodulationpar1 as i32);
        x.add_par("base_function_modulation_par2", self.basefuncmodulationpar2 as i32);
        x.add_par("base_function_modulation_par3", self.basefuncmodulationpar3 as i32);
        x.add_par("modulation", self.modulation as i32);
        x.add_par("modulation_par1", self.modulationpar1 as i32);
        x.add_par("modulation_par2", self.modulationpar2 as i32);
        x.add_par("modulation_par3", self.modulationpar3 as i32);
        x.add_par("wave_shaping", self.waveshaping as i32);
        x.add_par("wave_shaping_function", self.waveshapingfunction as i32);
        x.add_par("filter_type", self.filtertype as i32);
        x.add_par("filter_par1", self.filterpar1 as i32);
        x.add_par("filter_par2", self.filterpar2 as i32);
        x.add_par("filter_before_wave_shaping", self.filterbeforews as i32);
        x.add_par("spectrum_adjust_type", self.satype as i32);
        x.add_par("spectrum_adjust_par", self.sapar as i32);
        x.add_par("rand", self.rand as i32);
        x.add_par("amp_rand_type", self.amprandtype as i32);
        x.add_par("amp_rand_power", self.amprandpower as i32);
        x.add_par("harmonic_shift", self.harmonicshift);
        x.add_parbool("harmonic_shift_first", self.harmonicshiftfirst);
        x.add_par("adaptive_harmonics", self.adaptiveharmonics as i32);
        x.add_par("adaptive_harmonics_base_frequency", self.adaptiveharmonicsbasefreq as i32);
        x.add_par("adaptive_harmonics_power", self.adaptiveharmonicspower as i32);
        x.add_par("adaptive_harmonics_par", self.adaptiveharmonicspar as i32);
        let h = x.child("HARMONICS");
        for n in 0..MAX_AD_HARMONICS {
            if self.hmag[n] == 64 && self.hphase[n] == 64 {
                continue;
            }
            let c = h.child_id("HARMONIC", n + 1);
            c.add_par("mag", self.hmag[n] as i32);
            c.add_par("phase", self.hphase[n] as i32);
        }
        if self.basefunc == 127 && !self.custom_base.is_empty() {
            let b = x.child("BASE_FUNCTION");
            for i in 1..self.custom_base.len() {
                let v = self.custom_base[i];
                if v.im.abs() > 1e-6 || v.re.abs() > 1e-6 {
                    let c = b.child_id("BF_HARMONIC", i);
                    c.add_parreal("cos", v.re);
                    c.add_parreal("sin", v.im);
                }
            }
        }
    }

    pub fn get_from_xml(&mut self, x: &XmlNode, oscilsize: usize) {
        self.hmagtype = x.par127("harmonic_mag_type", self.hmagtype);
        self.basefunc = x.par127("base_function", self.basefunc);
        self.basefuncpar = x.par127("base_function_par", self.basefuncpar);
        self.basefuncmodulation = x.par127("base_function_modulation", self.basefuncmodulation);
        self.basefuncmodulationpar1 = x.par127("base_function_modulation_par1", self.basefuncmodulationpar1);
        self.basefuncmodulationpar2 = x.par127("base_function_modulation_par2", self.basefuncmodulationpar2);
        self.basefuncmodulationpar3 = x.par127("base_function_modulation_par3", self.basefuncmodulationpar3);
        self.modulation = x.par127("modulation", self.modulation);
        self.modulationpar1 = x.par127("modulation_par1", self.modulationpar1);
        self.modulationpar2 = x.par127("modulation_par2", self.modulationpar2);
        self.modulationpar3 = x.par127("modulation_par3", self.modulationpar3);
        self.waveshaping = x.par127("wave_shaping", self.waveshaping);
        self.waveshapingfunction = x.par127("wave_shaping_function", self.waveshapingfunction);
        self.filtertype = x.par127("filter_type", self.filtertype);
        self.filterpar1 = x.par127("filter_par1", self.filterpar1);
        self.filterpar2 = x.par127("filter_par2", self.filterpar2);
        self.filterbeforews = x.par127("filter_before_wave_shaping", self.filterbeforews as u8) != 0;
        self.satype = x.par127("spectrum_adjust_type", self.satype);
        self.sapar = x.par127("spectrum_adjust_par", self.sapar);
        self.rand = x.par127("rand", self.rand);
        self.amprandtype = x.par127("amp_rand_type", self.amprandtype);
        self.amprandpower = x.par127("amp_rand_power", self.amprandpower);
        self.harmonicshift = x.par("harmonic_shift", self.harmonicshift, -64, 64);
        self.harmonicshiftfirst = x.parbool("harmonic_shift_first", self.harmonicshiftfirst);
        self.adaptiveharmonics = x.par("adaptive_harmonics", self.adaptiveharmonics as i32, 0, 127) as u8;
        self.adaptiveharmonicsbasefreq = x.par("adaptive_harmonics_base_frequency", self.adaptiveharmonicsbasefreq as i32, 0, 255) as u8;
        self.adaptiveharmonicspower = x.par("adaptive_harmonics_power", self.adaptiveharmonicspower as i32, 0, 200) as u8;
        self.adaptiveharmonicspar = x.par("adaptive_harmonics_par", self.adaptiveharmonicspar as i32, 0, 100) as u8;
        if let Some(h) = x.branch("HARMONICS") {
            self.hmag[0] = 64;
            self.hphase[0] = 64;
            for n in 0..MAX_AD_HARMONICS {
                let Some(c) = h.branch_id("HARMONIC", n + 1) else { continue };
                self.hmag[n] = c.par127("mag", 64);
                self.hphase[n] = c.par127("phase", 64);
            }
        }
        if let Some(b) = x.branch("BASE_FUNCTION") {
            let mut v = vec![C32::new(0.0, 0.0); oscilsize / 2];
            for (i, slot) in v.iter_mut().enumerate().skip(1) {
                if let Some(c) = b.branch_id("BF_HARMONIC", i) {
                    *slot = C32::new(c.parreal("cos", 0.0), c.parreal("sin", 0.0));
                }
            }
            normalize_freqs(&mut v);
            self.custom_base = v;
        }
    }
}

// ---------------------------------------------------------------------------
// spectrum helpers
// ---------------------------------------------------------------------------

#[inline]
fn polar(mag: f32, phase: f32) -> C32 {
    C32::new(mag * phase.cos(), mag * phase.sin())
}

/// Phase as from a sine: `arg(complex(imag, real))`.
#[inline]
fn sarg(c: C32) -> f32 {
    c.re.atan2(c.im)
}

/// Normalise bins `0..len` so the largest magnitude is 1.
fn normalize_freqs(f: &mut [C32]) {
    let mut m = 0.0f32;
    for c in f.iter() {
        m = m.max(c.norm_sqr());
    }
    let m = m.sqrt();
    if m < 1e-8 {
        return;
    }
    for c in f.iter_mut() {
        *c /= m;
    }
}

fn rms_normalize(f: &mut [C32]) {
    let sum: f32 = f.iter().skip(1).map(|c| c.norm_sqr()).sum();
    if sum < 0.000_001 {
        return;
    }
    let g = 1.0 / sum.sqrt();
    for c in f.iter_mut().skip(1) {
        *c *= g;
    }
}

fn normalize_smps(s: &mut [f32]) {
    let mut m = s.iter().fold(0.0f32, |a, v| a.max(v.abs()));
    if m < 0.00001 {
        m = 1.0;
    }
    for v in s.iter_mut() {
        *v /= m;
    }
}

// ---------------------------------------------------------------------------
// base functions
// ---------------------------------------------------------------------------

fn fmod1(x: f32) -> f32 {
    x % 1.0
}

fn power_cosinus_32(x: f32, power: f64) -> f32 {
    let mut xi = ((x - x.floor()) as f64 * 4_294_967_296.0) as u64 as u32;
    match xi {
        0xFFFF_FFFF | 0 => return 1.0,
        0x3FFF_FFFF | 0x4000_0000 | 0xBFFF_FFFF | 0xC000_0000 => return 0.0,
        0x7FFF_FFFF | 0x8000_0000 => return -1.0,
        _ => {}
    }
    let mut mask: u32 = 1 << 31;
    while mask != 1 {
        if xi & mask != 0 {
            xi ^= mask - 1;
        }
        mask /= 2;
    }
    let mut num: u32 = 0;
    while num != 30 {
        if xi & (1 << num) != 0 {
            num += 1;
            break;
        }
        num += 1;
    }
    let mut r = 0.0f64;
    while num < 30 {
        if xi & (1 << num) != 0 {
            r = ((1.0 - r) / 2.0).powf(power);
        } else {
            r = ((1.0 + r) / 2.0).powf(power);
        }
        num += 1;
    }
    if xi & (1 << 30) != 0 {
        r = -r;
    }
    r as f32
}

pub fn base_function(func: u8, x: f32, a: f32) -> Option<f32> {
    let mut x = x;
    let mut a = a;
    Some(match func {
        1 => {
            // triangle
            x = fmod1(x + 0.25);
            a = 1.0 - a;
            if a < 0.00001 {
                a = 0.00001;
            }
            x = if x < 0.5 { x * 4.0 - 1.0 } else { (1.0 - x) * 4.0 - 1.0 };
            x /= -a;
            x.clamp(-1.0, 1.0)
        }
        2 => {
            if fmod1(x) < a {
                -1.0
            } else {
                1.0
            }
        }
        3 => {
            a = a.clamp(0.00001, 0.99999);
            x = fmod1(x);
            if x < a { x / a * 2.0 - 1.0 } else { (1.0 - x) / (1.0 - a) * 2.0 - 1.0 }
        }
        4 => {
            x = fmod1(x);
            a = a.clamp(0.00001, 0.99999);
            x.powf(((a - 0.5) * 10.0).exp()) * 2.0 - 1.0
        }
        5 => {
            x = fmod1(x) * 2.0 - 1.0;
            if a < 0.00001 {
                a = 0.00001;
            }
            (-x * x * ((a * 8.0).exp() + 5.0)).exp() * 2.0 - 1.0
        }
        6 => {
            a = a.clamp(0.00001, 0.99999);
            a = a * 2.0 - 1.0;
            x = ((x + 0.5) * 2.0 * PI).cos() - a;
            if x < 0.0 {
                x = 0.0;
            }
            x / (1.0 - a) * 2.0 - 1.0
        }
        7 => {
            x = fmod1(x);
            a = a.clamp(0.00001, 0.99999);
            (x.powf(((a - 0.5) * 5.0).exp()) * PI).sin() * 2.0 - 1.0
        }
        8 => {
            if a < 0.00001 {
                a = 0.00001;
            }
            x = (fmod1(x) - 0.5) * ((a - 0.5) * 128f32.ln()).exp();
            x = x.clamp(-0.5, 0.5);
            (x * PI * 2.0).sin()
        }
        9 => {
            x = fmod1(x + 0.5) * 2.0 - 1.0;
            a = (a - 0.5) * 4.0;
            if a > 0.0 {
                a *= 2.0;
            }
            a = 3f32.powf(a);
            let mut b = x.abs().powf(a);
            if x < 0.0 {
                b = -b;
            }
            -(b * PI).sin()
        }
        10 => {
            x = fmod1(x) * 2.0 * PI;
            a = (a - 0.5) * 4.0;
            if a < 0.0 {
                a *= 2.0;
            }
            a = 3f32.powf(a);
            (x / 2.0).sin() * (a * x * x).sin()
        }
        11 => {
            x = fmod1(x + 0.5) * 2.0 - 1.0;
            a = (a - 0.5) * 9.0;
            a = 3f32.powf(a);
            let mut b = x.abs().powf(a);
            if x < 0.0 {
                b = -b;
            }
            -(b * PI).sin().powi(2)
        }
        12 => {
            a = a * a * a * 30.0 + 1.0;
            ((x * 2.0 - 1.0).acos() * a).cos()
        }
        13 => {
            a = a * a * a * a * 160.0 + 0.001;
            -((x * 2.0 * PI).sin() * a).atan()
        }
        14 => {
            let b = a * 0.66666;
            let bd = b as f64;
            if x < 0.5 {
                if (x as f64) < 0.5 - bd / 2.0 {
                    0.0
                } else {
                    x = (x + b / 2.0 - 0.5) * (2.0 / b);
                    x * (2.0 / b)
                }
            } else if (x as f64) > 0.5 + bd / 2.0 {
                0.0
            } else {
                x = (x - 0.5) * (2.0 / b);
                (1.0 - x) * (2.0 / b)
            }
        }
        15 => {
            let b = 2.0 - a * 2.0;
            x *= 4.0;
            if x < 2.0 {
                x -= 1.0;
                if x < -b || x > b { 0.0 } else { (1.0 - x * x / (b * b)).sqrt() }
            } else {
                x -= 3.0;
                if x < -b || x > b { 0.0 } else { -(1.0 - x * x / (b * b)).sqrt() }
            }
        }
        16 => power_cosinus_32(x + 0.75, 2.0 * a as f64),
        _ => return None,
    })
}

pub const BASE_FUNCTION_NAMES: [&str; 17] = [
    "Sine", "Triangle", "Pulse", "Saw", "Power", "Gauss", "Diode", "AbsSine", "PulseSine", "StretchSine", "Chirp",
    "AbsStretchSine", "Chebyshev", "Sqr", "Spike", "Circle", "PowerSinus",
];

pub const FILTER_NAMES: [&str; 15] = [
    "None", "LP1", "HP1a", "HP1b", "BP1", "BS1", "LP2", "HP2", "BP2", "BS2", "Cos", "Sin", "LowShelf", "S", "LPSK",
];

pub const WAVESHAPE_NAMES: [&str; 18] = [
    "None", "Atan", "Asym1", "Pow", "Sine", "Qnts", "Zigzg", "Lmt", "LmtU", "LmtL", "ILmt", "Clip", "Asym2", "Pow2",
    "Sgm", "Tanh", "Cubic", "Square",
];

fn osc_filter(func: u8, i: u32, mut par: f32, par2: f32) -> f32 {
    let fi = i as f32;
    match func {
        1 => {
            let mut gain = (1.0 - par * par * par * 0.99).powf(fi);
            let tmp = par2 * par2 * par2 * par2 * 0.5 + 0.0001;
            if gain < tmp {
                gain = gain.powf(10.0) / tmp.powf(9.0);
            }
            gain
        }
        2 => {
            let gain = 1.0 - (1.0 - par * par).powf(fi + 1.0);
            gain.powf(par2 * 2.0 + 0.1)
        }
        3 => {
            if par < 0.2 {
                par = par * 0.25 + 0.15;
            }
            let gain = 1.0 - (1.0 - par * par * 0.999 + 0.001).powf(fi * 0.05 * fi + 1.0);
            gain.powf(5f32.powf(par2 * 2.0))
        }
        4 => {
            let mut gain = fi + 1.0 - 2f32.powf((1.0 - par) * 7.5);
            gain = 1.0 / (1.0 + gain * gain / (fi + 1.0));
            gain = gain.powf(5f32.powf(par2 * 2.0));
            gain.max(1e-5)
        }
        5 => {
            let mut gain = fi + 1.0 - 2f32.powf((1.0 - par) * 7.5);
            gain = ((gain / (fi / 10.0 + 1.0)).atan() / 1.57).powi(6);
            gain.powf(par2 * par2 * 3.9 + 0.1)
        }
        6 => (if fi + 1.0 > 2f32.powf((1.0 - par) * 10.0) { 0.0 } else { 1.0 }) * par2 + (1.0 - par2),
        7 => {
            if par == 1.0 {
                return 1.0;
            }
            (if fi + 1.0 > 2f32.powf((1.0 - par) * 7.0) { 1.0 } else { 0.0 }) * par2 + (1.0 - par2)
        }
        8 => {
            let lim = (i / 2 + 1) as f32;
            (if (2f32.powf((1.0 - par) * 7.0) - fi).abs() > lim { 0.0 } else { 1.0 }) * par2 + (1.0 - par2)
        }
        9 => {
            let lim = (i / 2 + 1) as f32;
            (if (2f32.powf((1.0 - par) * 7.0) - fi).abs() < lim { 0.0 } else { 1.0 }) * par2 + (1.0 - par2)
        }
        10 | 11 => {
            let mut tmp = 5f32.powf(par2 * 2.0 - 1.0);
            tmp = (fi / 32.0).powf(tmp) * 32.0;
            let v = par2 * 127.0;
            if v + 0.01 > 64.0 && v - 0.01 < 64.0 {
                tmp = fi;
            }
            let g = if func == 10 { (par * par * PI / 2.0 * tmp).cos() } else { (par * par * PI / 2.0 * tmp).sin() };
            g * g
        }
        12 => {
            let p2 = 1.0 - par + 0.2;
            let x = (fi / (64.0 * p2 * p2)).clamp(0.0, 1.0);
            let tmp = (1.0 - par2).powi(2);
            (x * PI).cos() * (1.0 - tmp) + 1.01 + tmp
        }
        13 => {
            let tmp = 2f32.powf((1.0 - par) * 7.2) as i32 as u32;
            if i == tmp { 2f32.powf(par2 * par2 * 8.0) } else { 1.0 }
        }
        14 => {
            let t2pf = 2.0 * PI * (1.05 - par) * 64.0;
            let s = C32::new(0.0, 2.0 * PI * fi);
            let vout = t2pf * t2pf;
            let vin = s * s + s * t2pf / (par2 + 2.0 * par * par2 + 0.5) + C32::new(t2pf * t2pf, 0.0);
            let num = vout * vout * vout;
            (C32::new(num, 0.0) / (vin * vin * vin)).norm()
        }
        _ => 1.0,
    }
}

// ---------------------------------------------------------------------------
// OscilGen
// ---------------------------------------------------------------------------

/// Scratch for `get`, owned by the audio-thread engine.
#[derive(Clone, Debug)]
pub struct OscScratch {
    pub out: Vec<C32>,
    pub inf: Vec<C32>,
    pub smps: Vec<f32>,
}

impl OscScratch {
    pub fn new(oscilsize: usize) -> Self {
        Self {
            out: vec![C32::new(0.0, 0.0); oscilsize / 2 + 1],
            inf: vec![C32::new(0.0, 0.0); oscilsize / 2 + 1],
            smps: vec![0.0; oscilsize],
        }
    }
}

/// Parameters plus the prepared spectrum.
#[derive(Clone, Debug)]
pub struct OscilGen {
    pub p: OscilParams,
    pub ad_vs_pad: bool,
    oscilsize: usize,
    /// Prepared harmonic spectrum (bins 0..oscilsize/2).
    pub freqs: Vec<C32>,
    /// Base function spectrum.
    pub basefreqs: Vec<C32>,
}

impl OscilGen {
    pub fn new(oscilsize: usize, ad_vs_pad: bool) -> Self {
        Self {
            p: OscilParams::new(ad_vs_pad),
            ad_vs_pad,
            oscilsize,
            freqs: vec![C32::new(0.0, 0.0); oscilsize / 2 + 1],
            basefreqs: vec![C32::new(0.0, 0.0); oscilsize / 2 + 1],
        }
    }

    pub fn oscilsize(&self) -> usize {
        self.oscilsize
    }

    /// Time-domain base function (`getbasefunction`).
    pub fn base_function_samples(&self, out: &mut [f32], fft: &mut Fft) {
        let p = &self.p;
        let n = self.oscilsize;
        let par = if p.basefuncpar == 64 { 0.5 } else { (p.basefuncpar as f32 + 0.5) / 128.0 };
        let mut p1 = p.basefuncmodulationpar1 as f32 / 127.0;
        let p2 = p.basefuncmodulationpar2 as f32 / 127.0;
        let mut p3 = p.basefuncmodulationpar3 as f32 / 127.0;
        match p.basefuncmodulation {
            1 => {
                p1 = (2f32.powf(p1 * 5.0) - 1.0) / 10.0;
                p3 = (2f32.powf(p3 * 5.0) - 1.0).floor();
                if p3 < 0.9999 {
                    p3 = -1.0;
                }
            }
            2 => {
                p1 = (2f32.powf(p1 * 5.0) - 1.0) / 10.0;
                p3 = 1.0 + (2f32.powf(p3 * 5.0) - 1.0).floor();
            }
            3 => {
                p1 = (2f32.powf(p1 * 7.0) - 1.0) / 10.0;
                p3 = 0.01 + (2f32.powf(p3 * 16.0) - 1.0) / 10.0;
            }
            _ => {}
        }
        // user function: time-domain custom base
        let mut user = Vec::new();
        if p.basefunc == 127 && !p.custom_base.is_empty() {
            user = vec![0.0; n];
            fft.freqs2smps(&p.custom_base, &mut user);
        }
        for i in 0..n {
            let mut t = i as f32 / n as f32;
            match p.basefuncmodulation {
                1 => t = t * p3 + ((t + p2) * 2.0 * PI).sin() * p1,
                2 => t += ((t * p3 + p2) * 2.0 * PI).sin() * p1,
                3 => t += ((1.0 - ((t + p2) * 2.0 * PI).cos()) * 0.5).powf(p3) * p1,
                4 => {
                    t = t * 2f32.powf(p.basefuncmodulationpar1 as f32 / 32.0 + p.basefuncmodulationpar2 as f32 / 2048.0) + p3
                }
                _ => {}
            }
            t -= t.floor();
            out[i] = if let Some(v) = base_function(p.basefunc, t, par) {
                v
            } else if p.basefunc == 0 {
                -(2.0 * PI * i as f32 / n as f32).sin()
            } else if !user.is_empty() {
                cinterpolate(&user, n as f32 * (t + 1.0) - 1.0)
            } else {
                0.0
            };
        }
    }

    fn changebasefunction(&mut self, fft: &mut Fft) {
        let h = self.oscilsize / 2;
        if self.p.basefunc != 0 {
            if self.p.basefunc == 127 {
                self.basefreqs.iter_mut().for_each(|c| *c = C32::new(0.0, 0.0));
                let l = self.p.custom_base.len().min(h);
                self.basefreqs[..l].copy_from_slice(&self.p.custom_base[..l]);
            } else {
                let mut tmp = vec![0.0; self.oscilsize];
                self.base_function_samples(&mut tmp, fft);
                fft.smps2freqs(&tmp, &mut self.basefreqs);
            }
            self.basefreqs[0] = C32::new(0.0, 0.0);
        } else {
            self.basefreqs.iter_mut().for_each(|c| *c = C32::new(0.0, 0.0));
        }
    }

    fn oscilfilter(&self, freqs: &mut [C32]) {
        if self.p.filtertype == 0 {
            return;
        }
        let par = 1.0 - self.p.filterpar1 as f32 / 128.0;
        let par2 = self.p.filterpar2 as f32 / 127.0;
        let h = self.oscilsize / 2;
        for i in 1..h {
            freqs[i] *= osc_filter(self.p.filtertype, i as u32, par, par2);
        }
        normalize_freqs(&mut freqs[..h]);
    }

    fn waveshape(&self, freqs: &mut [C32], fft: &mut Fft) {
        if self.p.waveshapingfunction == 0 {
            return;
        }
        let n = self.oscilsize;
        freqs[0] = C32::new(0.0, 0.0);
        for i in 1..n / 8 {
            let gain = i as f32 / (n as f32 / 8.0);
            freqs[n / 2 - i] *= gain;
        }
        let mut tmp = vec![0.0; n];
        fft.freqs2smps(freqs, &mut tmp);
        normalize_smps(&mut tmp);
        wave_shape_smps(&mut tmp, self.p.waveshapingfunction, self.p.waveshaping, 64, 0);
        fft.smps2freqs(&tmp, freqs);
    }

    fn modulation(&self, freqs: &mut [C32], fft: &mut Fft) {
        let p = &self.p;
        if p.modulation == 0 {
            return;
        }
        let n = self.oscilsize;
        let mut m1 = p.modulationpar1 as f32 / 127.0;
        let m2 = 0.5 - p.modulationpar2 as f32 / 127.0;
        let mut m3 = p.modulationpar3 as f32 / 127.0;
        match p.modulation {
            1 => {
                m1 = (2f32.powf(m1 * 7.0) - 1.0) / 100.0;
                m3 = (2f32.powf(m3 * 5.0) - 1.0).floor();
                if m3 < 0.9999 {
                    m3 = -1.0;
                }
            }
            2 => {
                m1 = (2f32.powf(m1 * 7.0) - 1.0) / 100.0;
                m3 = 1.0 + (2f32.powf(m3 * 5.0) - 1.0).floor();
            }
            3 => {
                m1 = (2f32.powf(m1 * 9.0) - 1.0) / 100.0;
                m3 = 0.01 + (2f32.powf(m3 * 16.0) - 1.0) / 10.0;
            }
            _ => {}
        }
        freqs[0] = C32::new(0.0, 0.0);
        for i in 1..n / 8 {
            let t = i as f32 / (n as f32 / 8.0);
            freqs[n / 2 - i] *= t;
        }
        let mut tmp = vec![0.0; n];
        fft.freqs2smps(freqs, &mut tmp);
        normalize_smps(&mut tmp);
        let mut inp = vec![0.0; n + 2];
        inp[..n].copy_from_slice(&tmp);
        inp[n] = tmp[0];
        inp[n + 1] = tmp[1];
        for i in 0..n {
            let mut t = i as f32 / n as f32;
            match p.modulation {
                1 => t = t * m3 + ((t + m2) * 2.0 * PI).sin() * m1,
                2 => t += ((t * m3 + m2) * 2.0 * PI).sin() * m1,
                3 => t += ((1.0 - ((t + m2) * 2.0 * PI).cos()) * 0.5).powf(m3) * m1,
                _ => {}
            }
            t = (t - t.floor()) * n as f32;
            let poshi = (t as usize).min(n);
            let poslo = t - t.floor();
            tmp[i] = inp[poshi] * (1.0 - poslo) + inp[poshi + 1] * poslo;
        }
        fft.smps2freqs(&tmp, freqs);
    }

    fn spectrumadjust(&self, freqs: &mut [C32]) {
        let p = &self.p;
        if p.satype == 0 {
            return;
        }
        let mut par = p.sapar as f32 / 127.0;
        match p.satype {
            1 => {
                par = 1.0 - par * 2.0;
                par = if par >= 0.0 { 5f32.powf(par) } else { 8f32.powf(par) };
            }
            2 | 3 => par = 10f32.powf((1.0 - par) * 3.0) * 0.001,
            _ => {}
        }
        let h = self.oscilsize / 2;
        normalize_freqs(&mut freqs[..h]);
        for c in freqs[..h].iter_mut() {
            let mut mag = c.norm();
            let phase = std::f32::consts::FRAC_PI_2 - sarg(*c);
            match p.satype {
                1 => mag = mag.powf(par),
                2 => {
                    if mag < par {
                        mag = 0.0;
                    }
                }
                3 => {
                    mag /= par;
                    if mag > 1.0 {
                        mag = 1.0;
                    }
                }
                _ => {}
            }
            *c = polar(mag, phase);
        }
    }

    fn shiftharmonics(&self, freqs: &mut [C32]) {
        if self.p.harmonicshift == 0 {
            return;
        }
        let h = self.oscilsize / 2;
        let hs = -self.p.harmonicshift;
        if hs > 0 {
            for i in (0..=(h as i32 - 2)).rev() {
                let oldh = i - hs;
                let v = if oldh < 0 { C32::new(0.0, 0.0) } else { freqs[(oldh + 1) as usize] };
                freqs[(i + 1) as usize] = v;
            }
        } else {
            for i in 0..(h as i32 - 1) {
                let oldh = i + hs.abs();
                let v = if oldh >= h as i32 - 1 {
                    C32::new(0.0, 0.0)
                } else {
                    let v = freqs[(oldh + 1) as usize];
                    if v.norm() < 0.000_001 { C32::new(0.0, 0.0) } else { v }
                };
                freqs[(i + 1) as usize] = v;
            }
        }
        freqs[0] = C32::new(0.0, 0.0);
    }

    /// Build the spectrum from the parameters (`prepare`).
    pub fn prepare(&mut self, fft: &mut Fft) {
        self.changebasefunction(fft);
        let p = &self.p;
        let h = self.oscilsize / 2;
        let mut hmag = [0.0f32; MAX_AD_HARMONICS];
        let mut hphase = [0.0f32; MAX_AD_HARMONICS];
        for i in 0..MAX_AD_HARMONICS {
            hphase[i] = (p.hphase[i] as f32 - 64.0) / 64.0 * PI / (i + 1) as f32;
            let hn = 1.0 - (p.hmag[i] as f32 / 64.0 - 1.0).abs();
            hmag[i] = match p.hmagtype {
                1 => (hn * 0.01f32.ln()).exp(),
                2 => (hn * 0.001f32.ln()).exp(),
                3 => (hn * 0.0001f32.ln()).exp(),
                4 => (hn * 0.00001f32.ln()).exp(),
                _ => 1.0 - hn,
            };
            if p.hmag[i] < 64 {
                hmag[i] = -hmag[i];
            }
            if p.hmag[i] == 64 {
                hmag[i] = 0.0;
            }
        }
        let mut freqs = vec![C32::new(0.0, 0.0); h + 1];
        if p.basefunc == 0 {
            for i in 0..MAX_AD_HARMONICS - 1 {
                if i + 1 >= h {
                    break;
                }
                let ph = hphase[i] * (i + 1) as f32;
                freqs[i + 1] = C32::new(-hmag[i] * ph.sin() / 2.0, hmag[i] * ph.cos() / 2.0);
            }
        } else {
            for j in 0..MAX_AD_HARMONICS {
                if p.hmag[j] == 64 {
                    continue;
                }
                for i in 1..h {
                    let k = i * (j + 1);
                    if k >= h {
                        break;
                    }
                    freqs[k] += self.basefreqs[i] * polar(hmag[j], hphase[j] * k as f32);
                }
            }
        }
        if p.harmonicshiftfirst {
            self.shiftharmonics(&mut freqs);
        }
        if p.filterbeforews {
            self.oscilfilter(&mut freqs);
            self.waveshape(&mut freqs, fft);
        } else {
            self.waveshape(&mut freqs, fft);
            self.oscilfilter(&mut freqs);
        }
        self.modulation(&mut freqs, fft);
        self.spectrumadjust(&mut freqs);
        if !self.p.harmonicshiftfirst {
            self.shiftharmonics(&mut freqs);
        }
        freqs[0] = C32::new(0.0, 0.0);
        freqs[h] = C32::new(0.0, 0.0);
        self.freqs = freqs;
    }

    fn adaptiveharmonic(&self, f: &mut [C32], inf: &mut [C32], freq: f32) {
        let p = &self.p;
        if p.adaptiveharmonics == 0 {
            return;
        }
        let freq = if freq < 1.0 { 440.0 } else { freq };
        let h = self.oscilsize / 2;
        inf[..h].copy_from_slice(&f[..h]);
        f[..h].iter_mut().for_each(|c| *c = C32::new(0.0, 0.0));
        inf[0] = C32::new(0.0, 0.0);
        let basefreq = 30.0 * 10f32.powf(p.adaptiveharmonicsbasefreq as f32 / 128.0);
        let power = (p.adaptiveharmonicspower as f32 + 1.0) / 101.0;
        let mut rap = (freq / basefreq).powf(power);
        let mut down = false;
        if rap > 1.0 {
            rap = 1.0 / rap;
            down = true;
        }
        for i in 0..h.saturating_sub(2) {
            let high = (i as f32 * rap) as usize;
            let low = (i as f32 * rap) % 1.0;
            if high >= h - 2 {
                break;
            }
            if down {
                f[high] += inf[i] * (1.0 - low);
                f[high + 1] += inf[i] * low;
            } else {
                f[i] = inf[high] * (1.0 - low) + inf[high + 1] * low;
            }
        }
        if !down {
            f[0] *= rap;
        }
        let f0 = f[0];
        f[1] += f0;
        f[0] = C32::new(0.0, 0.0);
    }

    fn adaptiveharmonicpostprocess(&self, f: &mut [C32], inf: &mut [C32]) {
        let p = &self.p;
        if p.adaptiveharmonics <= 1 {
            return;
        }
        let size = f.len();
        let mut par = p.adaptiveharmonicspar as f32 * 0.01;
        par = 1.0 - (1.0 - par).powf(1.5);
        for i in 0..size {
            inf[i] = f[i] * par;
            f[i] *= 1.0 - par;
        }
        if p.adaptiveharmonics == 2 {
            for i in (0..size).step_by(2) {
                f[i] += inf[i];
            }
        } else {
            let nh = (p.adaptiveharmonics as usize - 3) / 2 + 2;
            let sub_vs_add = (p.adaptiveharmonics as usize - 3) % 2;
            if sub_vs_add == 0 {
                for i in 0..size {
                    if (i + 1) % nh == 0 {
                        f[i] += inf[i];
                    }
                }
            } else {
                for i in 0..(size / nh).saturating_sub(1) {
                    f[(i + 1) * nh - 1] += inf[i];
                }
            }
        }
    }

    /// Render one period (`get`). Returns the start offset for block-type
    /// phase randomness. With `ad_vs_pad` and `freq > 0.1`, `smps[i-1]`
    /// receives the magnitude of harmonic `i` instead of samples.
    #[allow(clippy::too_many_arguments)]
    pub fn get(
        &self,
        smps: &mut [f32],
        freq_hz: f32,
        res: Option<(&Resonance, f32, f32)>,
        randseed: u32,
        sc: &mut OscScratch,
        fft: &mut Fft,
        samplerate: f32,
    ) -> i32 {
        let p = &self.p;
        let n = self.oscilsize;
        let h = n / 2;
        let realrnd = prng();
        sprng(randseed);
        let mut outpos = ((rnd() * 2.0 - 1.0) * n as f32 * (p.rand as f32 - 64.0) / 64.0) as i32;
        outpos = (outpos + 2 * n as i32).rem_euclid(n as i32);

        let out = &mut sc.out;
        out.iter_mut().for_each(|c| *c = C32::new(0.0, 0.0));
        let mut nyquist = (0.5 * samplerate / freq_hz.abs()) as i64 + 2;
        if self.ad_vs_pad {
            nyquist = h as i64;
        }
        let nyquist = nyquist.min(h as i64).max(0) as usize;
        {
            let lim = if p.adaptiveharmonics != 0 { h } else { nyquist };
            for i in 1..lim.saturating_sub(1) {
                out[i] = self.freqs[i];
            }
            self.adaptiveharmonic(out, &mut sc.inf, freq_hz);
            self.adaptiveharmonicpostprocess(&mut out[1..h], &mut sc.inf);
        }
        if p.adaptiveharmonics != 0 {
            for c in out[nyquist..h].iter_mut() {
                *c = C32::new(0.0, 0.0);
            }
        }
        if p.rand > 64 && freq_hz >= 0.0 && !self.ad_vs_pad {
            let r = PI * ((p.rand as f32 - 64.0) / 64.0).powi(2);
            for i in 1..nyquist.saturating_sub(1) {
                out[i] *= polar(1.0, r * i as f32 * rnd());
            }
        }
        if freq_hz > 0.1 && !self.ad_vs_pad {
            let mut power = p.amprandpower as f32 / 127.0;
            let normalize = 1.0 / (1.2 - power);
            match p.amprandtype {
                1 => {
                    power = power * 2.0 - 0.5;
                    power = 15f32.powf(power);
                    for i in 1..nyquist.saturating_sub(1) {
                        out[i] *= rnd().powf(power) * normalize;
                    }
                }
                2 => {
                    power = power * 2.0 - 0.5;
                    power = 15f32.powf(power) * 2.0;
                    let rndfreq = 2.0 * PI * rnd();
                    for i in 1..nyquist.saturating_sub(1) {
                        out[i] *= (i as f32 * rndfreq).sin().abs().powf(power) * normalize;
                    }
                }
                _ => {}
            }
        }
        if freq_hz > 0.1 {
            if let Some((r, c, b)) = res {
                r.applyres(nyquist.saturating_sub(1), out, freq_hz, c, b);
            }
        }
        rms_normalize(&mut out[..h]);
        if self.ad_vs_pad && freq_hz > 0.1 {
            for i in 1..h {
                smps[i - 1] = out[i].norm();
            }
        } else {
            fft.freqs2smps(out, &mut sc.smps);
            for i in 0..n {
                smps[i] = sc.smps[i] * 0.25;
            }
        }
        sprng(realrnd.wrapping_add(1));
        if p.rand < 64 { outpos } else { 0 }
    }

    /// Harmonic magnitudes for the editor (`getspectrum`, what = 0).
    pub fn spectrum(&self, n: usize) -> Vec<f32> {
        let n = n.min(self.oscilsize / 2);
        (0..n).map(|i| if i == 0 { 0.0 } else { self.freqs[i].norm() }).collect()
    }
}

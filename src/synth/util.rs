//! Globals and helpers from `globals.h` / `Misc/Util.cpp`.

use std::cell::Cell;
use std::f32::consts::PI as PI_F;

pub const PI: f32 = PI_F;
pub const LOG_2: f32 = 0.693_147_2;
pub const LOG_10: f32 = 2.302_585_1;

pub const MAX_AD_HARMONICS: usize = 128;
pub const MAX_SUB_HARMONICS: usize = 64;
pub const PAD_MAX_SAMPLES: usize = 64;
pub const NUM_VOICES: usize = 8;
pub const POLYPHONY: usize = 60;
pub const NUM_SYS_EFX: usize = 4;
pub const NUM_INS_EFX: usize = 8;
pub const NUM_PART_EFX: usize = 3;
pub const NUM_KIT_ITEMS: usize = 16;
pub const VELOCITY_MAX_SCALE: f32 = 8.0;
pub const MAX_EQ_BANDS: usize = 8;
pub const MAX_FILTER_STAGES: usize = 5;
pub const FF_MAX_VOWELS: usize = 6;
pub const FF_MAX_FORMANTS: usize = 12;
pub const FF_MAX_SEQUENCE: usize = 8;
pub const MAX_REV_DELAY_SECONDS: f32 = 4.0;
pub const MAX_ENVELOPE_POINTS: usize = 40;
pub const FADEIN_ADJUSTMENT_SCALE: i32 = 20;
pub const AMPLITUDE_INTERPOLATION_THRESHOLD: f32 = 0.0001;

/// Engine-wide constants (`SYNTH_T`).
#[derive(Clone, Copy, Debug)]
pub struct SynthT {
    pub samplerate: u32,
    pub buffersize: usize,
    pub oscilsize: usize,
    pub samplerate_f: f32,
    pub halfsamplerate_f: f32,
    pub buffersize_f: f32,
    pub oscilsize_f: f32,
}

impl SynthT {
    pub fn new(samplerate: u32, buffersize: usize, oscilsize: usize) -> Self {
        Self {
            samplerate,
            buffersize,
            oscilsize,
            samplerate_f: samplerate as f32,
            halfsamplerate_f: samplerate as f32 / 2.0,
            buffersize_f: buffersize as f32,
            oscilsize_f: oscilsize as f32,
        }
    }
    #[inline]
    pub fn dt(&self) -> f32 {
        self.buffersize_f / self.samplerate_f
    }
}

impl Default for SynthT {
    fn default() -> Self {
        Self::new(44100, 256, 1024)
    }
}

#[inline]
pub fn above_amplitude_threshold(a: f32, b: f32) -> bool {
    (2.0 * (b - a).abs() / (b + a + 0.000_000_000_1).abs()) > AMPLITUDE_INTERPOLATION_THRESHOLD
}

#[inline]
pub fn interpolate_amplitude(a: f32, b: f32, x: usize, size: usize) -> f32 {
    a + (b - a) * x as f32 / size as f32
}

#[inline]
pub fn db2rap(db: f32) -> f32 {
    (db * LOG_10 / 20.0).exp()
}

#[inline]
pub fn rap2db(rap: f32) -> f32 {
    20.0 * rap.ln() / LOG_10
}

/// `F2I`: floor for positive and negative values (as Zyn rounds).
#[inline]
pub fn f2i(f: f32) -> i32 {
    if f > 0.0 { f as i32 } else { (f - 1.0) as i32 }
}

// ---------------------------------------------------------------------------
// Random numbers: Zyn's LCG, so randomised patches behave the same way.
// ---------------------------------------------------------------------------

thread_local! {
    static PRNG_STATE: Cell<u32> = const { Cell::new(0) };
}

#[inline]
pub fn prng_r(p: &mut u32) -> u32 {
    *p = p.wrapping_mul(1_103_515_245).wrapping_add(12345);
    *p
}

#[inline]
pub fn prng() -> u32 {
    PRNG_STATE.with(|s| {
        let mut v = s.get();
        let r = prng_r(&mut v);
        s.set(v);
        r & 0x7fff_ffff
    })
}

pub fn sprng(p: u32) {
    PRNG_STATE.with(|s| s.set(p));
}

const INT32_MAX_FLOAT: f32 = 2_147_483_520.0; // 0x7fffff80

/// `RND`: uniform in 0..1.
#[inline]
pub fn rnd() -> f32 {
    prng() as f32 / INT32_MAX_FLOAT
}

// ---------------------------------------------------------------------------

pub fn vel_f(velocity: f32, scaling: u8) -> f32 {
    let x = VELOCITY_MAX_SCALE.powf((64.0 - scaling as f32) / 64.0);
    if scaling == 127 || velocity > 0.99 { 1.0 } else { velocity.powf(x) }
}

pub fn getdetune(kind: u8, coarsedetune: u16, finedetune: u16) -> f32 {
    let mut octave = (coarsedetune / 1024) as i32;
    if octave >= 8 {
        octave -= 16;
    }
    let octdet = octave as f32 * 1200.0;
    let mut cdetune = (coarsedetune % 1024) as i32;
    if cdetune > 512 {
        cdetune -= 1024;
    }
    let fdetune = finedetune as i32 - 8192;
    let (mut cdet, mut findet) = match kind {
        2 => ((cdetune as f32 * 10.0).abs(), (fdetune as f32 / 8192.0).abs() * 10.0),
        3 => ((cdetune as f32 * 100.0).abs(), 10f32.powf((fdetune as f32 / 8192.0).abs() * 3.0) / 10.0 - 0.1),
        4 => (
            (cdetune as f32 * 701.955).abs(),
            (2f32.powf((fdetune as f32 / 8192.0).abs() * 12.0) - 1.0) / 4095.0 * 1200.0,
        ),
        _ => ((cdetune as f32 * 50.0).abs(), (fdetune as f32 / 8192.0).abs() * 35.0),
    };
    if finedetune < 8192 {
        findet = -findet;
    }
    if cdetune < 0 {
        cdet = -cdet;
    }
    octdet + cdet + findet
}

#[inline]
pub fn interpolate(data: &[f32], pos: f32) -> f32 {
    let l = pos as usize;
    let r = l + 1;
    let rightness = pos - l as f32;
    data[l] + (data[r] - data[l]) * rightness
}

#[inline]
pub fn cinterpolate(data: &[f32], pos: f32) -> f32 {
    let len = data.len();
    let i_pos = pos as usize;
    let l = i_pos % len;
    let r = if l + 1 < len { l + 1 } else { 0 };
    let rightness = pos - i_pos as f32;
    data[l] + (data[r] - data[l]) * rightness
}

/// `smooth_float`: one-pole-ish smoothing used by some effects.
#[derive(Clone, Copy, Default, Debug)]
pub struct SmoothFloat {
    init: bool,
    curr: f32,
    next: f32,
}

impl SmoothFloat {
    pub fn new(v: f32) -> Self {
        Self { init: true, curr: v, next: v }
    }
    #[inline]
    pub fn get(&mut self) -> f32 {
        let delta = (self.next - self.curr) / 32.0;
        self.curr += delta;
        self.curr
    }
    pub fn set(&mut self, v: f32) {
        if self.init {
            self.next = v;
        } else {
            self.next = v;
            self.curr = v;
            self.init = true;
        }
    }
    pub fn is_set(&self) -> bool {
        self.init
    }
}

#[inline]
pub fn limit<T: PartialOrd>(v: T, lo: T, hi: T) -> T {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

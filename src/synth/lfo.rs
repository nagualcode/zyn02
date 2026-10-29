//! `Params/LFOParams.cpp` + `Synth/LFO.cpp`.

use super::envelope::Loc;
use super::util::*;
use super::xml::{Version, XmlNode};

/// Frame clock (`Misc/Time.h`): one frame = one engine buffer.
#[derive(Clone, Copy, Debug)]
pub struct AbsTime {
    pub frames: i64,
    pub dt: f32,
    pub tempo: u32,
}

impl AbsTime {
    pub fn new(dt: f32) -> Self {
        Self { frames: 0, dt, tempo: 120 }
    }
    #[inline]
    pub fn time(&self) -> i64 {
        self.frames
    }
    #[inline]
    pub fn frames_per_sec(&self) -> f32 {
        1.0 / self.dt
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LfoKind {
    Freq,
    Amp,
    Filter,
    Unspecified,
}

#[derive(Clone, Debug)]
pub struct LfoParams {
    pub kind: LfoKind,
    pub freq: f32,
    pub intensity: u8,
    pub startphase: u8,
    pub cutoff: u8,
    pub lfotype: u8,
    pub randomness: u8,
    pub freqrand: u8,
    pub delay: f32,
    pub fadein: f32,
    pub fadeout: f32,
    pub continous: bool,
    pub stretch: u8,
    pub numerator: i32,
    pub denominator: i32,
}

impl LfoParams {
    pub fn new(loc: Loc) -> Self {
        let (freq, intensity, startphase, delay, kind) = match loc {
            Loc::AdGlobalAmp => (6.49, 0, 64, 0.0, LfoKind::Amp),
            Loc::AdGlobalFreq => (3.71, 0, 64, 0.0, LfoKind::Freq),
            Loc::AdGlobalFilter => (6.49, 0, 64, 0.0, LfoKind::Filter),
            Loc::AdVoiceAmp => (11.25, 32, 64, 0.94, LfoKind::Amp),
            Loc::AdVoiceFreq => (1.19, 40, 0, 0.0, LfoKind::Freq),
            Loc::AdVoiceFilter => (1.19, 20, 64, 0.0, LfoKind::Filter),
            _ => (2.65, 0, 0, 0.0, LfoKind::Unspecified),
        };
        Self {
            kind,
            freq,
            intensity,
            startphase,
            cutoff: 127,
            lfotype: 0,
            randomness: 0,
            freqrand: 0,
            delay,
            fadein: 0.0,
            fadeout: 10.0,
            continous: false,
            stretch: 64,
            numerator: 0,
            denominator: 4,
        }
    }

    pub fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_parreal("freq", self.freq);
        x.add_par("intensity", self.intensity as i32);
        x.add_par("start_phase", self.startphase as i32);
        x.add_par("cutoff", self.cutoff as i32);
        x.add_par("lfo_type", self.lfotype as i32);
        x.add_par("randomness_amplitude", self.randomness as i32);
        x.add_par("randomness_frequency", self.freqrand as i32);
        x.add_parreal("delay", self.delay);
        x.add_parreal("fadein", self.fadein);
        x.add_parreal("fadeout", self.fadeout);
        x.add_par("stretch", self.stretch as i32);
        x.add_parbool("continous", self.continous);
        x.add_par("numerator", self.numerator);
        x.add_par("denominator", self.denominator);
    }

    pub fn get_from_xml(&mut self, x: &XmlNode, ver: Version) {
        if ver < Version(3, 0, 4) {
            self.freq = (2f32.powf(10.0 * x.parreal_lim("freq", self.freq, 0.0, 1.0)) - 1.0) / 12.0;
        } else {
            self.freq = x.parreal("freq", self.freq);
        }
        self.intensity = x.par127("intensity", self.intensity);
        self.startphase = x.par127("start_phase", self.startphase);
        self.cutoff = x.par127("cutoff", self.cutoff);
        self.lfotype = x.par127("lfo_type", self.lfotype);
        self.randomness = x.par127("randomness_amplitude", self.randomness);
        self.freqrand = x.par127("randomness_frequency", self.freqrand);
        if x.has_parreal("delay") {
            self.delay = x.parreal("delay", self.delay);
        } else {
            let d = x.par127("delay", (self.delay as i32 as f32 * 127.0 / 4.0) as u8);
            self.delay = 4.0 * d as f32 / 127.0;
        }
        if x.has_parreal("fadein") {
            self.fadein = x.parreal("fadein", self.fadein);
        }
        if x.has_parreal("fadeout") {
            self.fadeout = x.parreal("fadeout", self.fadeout);
        }
        self.stretch = x.par127("stretch", self.stretch);
        self.continous = x.parbool("continous", self.continous);
        self.numerator = x.par("numerator", self.numerator, 0, 99);
        self.denominator = x.par("denominator", self.denominator, 0, 99);
    }

    fn intensity_value(&self) -> f32 {
        match self.kind {
            LfoKind::Amp => self.intensity as f32 / 127.0,
            LfoKind::Filter => self.intensity as f32 / 127.0 * 4.0,
            _ => 2f32.powf(self.intensity as f32 / 127.0 * 11.0) - 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Delaying,
    FadingIn,
    Running,
    FadingOut,
}

#[derive(Clone, Debug)]
pub struct Lfo {
    state: State,
    tempo: u32,
    phase: f32,
    phase_inc: f32,
    incrnd: f32,
    nextincrnd: f32,
    amp1: f32,
    amp2: f32,
    first_half: i32,
    last_random: f32,
    z1: f32,
    z2: f32,
    lfointensity: f32,
    lfornd: f32,
    lfofreqrnd: f32,
    delay_frame: i64,
    fade_in_duration: i64,
    fade_in_timestamp: i64,
    release_timestamp: i64,
    fade_out_duration: i64,
    ramp_up: f32,
    ramp_down: f32,
    ramp_on_release: f32,
    out_start_value: f32,
    wave_shape: u8,
    deterministic: bool,
    dt: f32,
    basefreq: f32,
    a0: f32,
    a1: f32,
    a2: f32,
    b1: f32,
    b2: f32,
    cutoff: u8,
}

impl Lfo {
    /// Placeholder that consumes no random numbers.
    pub fn idle() -> Self {
        let p = LfoParams::new(Loc::Unspecified);
        let t = AbsTime::new(1.0);
        Self {
            state: State::Running,
            tempo: 120,
            phase: 0.0,
            phase_inc: 0.0,
            incrnd: 1.0,
            nextincrnd: 1.0,
            amp1: 1.0,
            amp2: 1.0,
            first_half: -1,
            last_random: 0.0,
            z1: 0.0,
            z2: 0.0,
            lfointensity: 0.0,
            lfornd: 0.0,
            lfofreqrnd: 0.0,
            delay_frame: t.time(),
            fade_in_duration: 0,
            fade_in_timestamp: 0,
            release_timestamp: 0,
            fade_out_duration: 0,
            ramp_up: 0.0,
            ramp_down: 1.0,
            ramp_on_release: 0.0,
            out_start_value: 0.0,
            wave_shape: p.lfotype,
            deterministic: true,
            dt: t.dt,
            basefreq: 440.0,
            a0: 0.0,
            a1: 0.0,
            a2: 0.0,
            b1: 0.0,
            b2: 0.0,
            cutoff: 127,
        }
    }

    pub fn new(p: &LfoParams, basefreq: f32, t: &AbsTime) -> Self {
        let delta_frames = p.delay * t.frames_per_sec();
        let mut l = Self {
            state: State::Delaying,
            tempo: t.tempo,
            phase: 0.0,
            phase_inc: 0.0,
            incrnd: 1.0,
            nextincrnd: 1.0,
            amp1: 1.0,
            amp2: 1.0,
            first_half: -1,
            last_random: 0.0,
            z1: 0.0,
            z2: 0.0,
            lfointensity: 0.0,
            lfornd: 0.0,
            lfofreqrnd: 0.0,
            delay_frame: t.time() + delta_frames as i64,
            fade_in_duration: 0,
            fade_in_timestamp: 0,
            release_timestamp: 0,
            fade_out_duration: 0,
            ramp_up: 0.0,
            ramp_down: 1.0,
            ramp_on_release: 0.0,
            out_start_value: 0.0,
            wave_shape: 0,
            deterministic: p.freqrand == 0,
            dt: t.dt,
            basefreq,
            a0: 0.000_750_891_46,
            a1: 0.001_501_782_9,
            a2: 0.000_750_891_46,
            b1: -1.519_121_4,
            b2: 0.522_124_9,
            cutoff: 127,
        };
        l.update_pars(p, t);
        if !p.continous {
            l.phase = if p.startphase == 0 { rnd() } else { 0.0 };
        } else {
            l.phase = (t.time() as f32 * l.phase_inc) % 1.0;
        }
        l.lfornd = (p.randomness as f32 / 127.0).clamp(0.0, 1.0);
        l.lfofreqrnd = (p.freqrand as f32 / 127.0).powi(2) * 4.0;
        l.lfointensity = p.intensity_value();
        if matches!(p.kind, LfoKind::Freq | LfoKind::Unspecified) {
            l.phase -= 0.25;
        }
        l.amp1 = (1.0 - l.lfornd) + l.lfornd * rnd();
        l.amp2 = (1.0 - l.lfornd) + l.lfornd * rnd();
        l.compute_next_freq_rnd(p);
        l.compute_next_freq_rnd(p);
        l
    }

    fn update_pars(&mut self, p: &LfoParams, t: &AbsTime) {
        self.wave_shape = p.lfotype;
        let stretch = if p.stretch == 0 { 1 } else { p.stretch };
        let lfostretch = (self.basefreq / 440.0).powf((stretch as f32 - 64.0) / 63.0);
        let lfofreq = if p.numerator == 0 || p.denominator == 0 {
            p.freq * lfostretch
        } else {
            self.tempo = t.tempo;
            t.tempo as f32 * p.denominator as f32 / (240.0 * p.numerator as f32)
        };
        self.phase_inc = lfofreq.abs() * self.dt;
        if self.phase_inc > 0.499_999_99 {
            self.phase_inc = 0.499_999_999;
        }
    }

    fn biquad(&mut self, p: &LfoParams, input: f32) -> f32 {
        if p.cutoff != self.cutoff {
            self.cutoff = p.cutoff;
            if self.cutoff != 127 {
                let fc = (self.cutoff as f32 + 7.0) * (self.cutoff as f32 + 7.0) / 450.56;
                let k = (PI * (fc * self.dt).clamp(0.001, 0.4)).tan();
                let norm = 1.0 / (1.0 + k / 0.7071 + k * k);
                self.a0 = k * k * norm;
                self.a1 = 2.0 * self.a0;
                self.a2 = self.a0;
                self.b1 = 2.0 * (k * k - 1.0) * norm;
                self.b2 = (1.0 - k / 0.7071 + k * k) * norm;
            }
        }
        if self.cutoff == 127 {
            input
        } else {
            let output = (input * self.a0 + self.z1).clamp(-1.0, 1.0);
            self.z1 = input * self.a1 + self.z2 - self.b1 * output;
            self.z2 = input * self.a2 - self.b2 * output;
            output
        }
    }

    fn base_out(&mut self, p: &LfoParams, shape: u8, phase: f32) -> f32 {
        match shape {
            1 => {
                if (0.0..0.25).contains(&phase) {
                    4.0 * phase
                } else if phase > 0.25 && phase < 0.75 {
                    2.0 - 4.0 * phase
                } else {
                    4.0 * phase - 4.0
                }
            }
            2 => {
                let o = if phase < 0.5 { -1.0 } else { 1.0 };
                self.biquad(p, o)
            }
            3 => (phase - 0.5) * 2.0,
            4 => (0.5 - phase) * 2.0,
            5 => 0.05f32.powf(phase) * 2.0 - 1.0,
            6 => 0.001f32.powf(phase) * 2.0 - 1.0,
            7 => {
                let fh = (phase < 0.5) as i32;
                if fh != self.first_half {
                    self.first_half = fh;
                    self.last_random = 2.0 * rnd() - 1.0;
                }
                let r = self.last_random;
                self.biquad(p, r)
            }
            _ => (phase * 2.0 * PI).cos(),
        }
    }

    pub fn release_key(&mut self, p: &LfoParams, t: &AbsTime) {
        if p.fadeout == 10.0 {
            self.fade_out_duration = 0;
            return;
        }
        self.ramp_on_release = self.ramp_up;
        self.out_start_value *= 1.0 - self.ramp_on_release;
        self.release_timestamp = t.time();
        self.fade_out_duration = (p.fadeout * t.frames_per_sec()) as i64;
        self.state = State::FadingOut;
    }

    pub fn lfoout(&mut self, p: &LfoParams, t: &AbsTime) -> f32 {
        self.update_pars(p, t);
        self.lfointensity = p.intensity_value();
        if p.numerator != 0 && p.denominator != 0 && self.tempo != t.tempo {
            self.tempo = t.tempo;
            let f = t.tempo as f32 * p.denominator as f32 / (240.0 * p.numerator as f32);
            self.phase_inc = f.abs() * self.dt;
        }
        let pws = fmod1(self.phase + (p.startphase as f32 + 63.0) / 127.0);
        let shape = self.wave_shape;
        let mut out = self.base_out(p, shape, pws);
        if shape == 0 || shape == 1 {
            out *= self.lfointensity * (self.amp1 + pws * (self.amp2 - self.amp1));
        } else {
            out *= self.lfointensity * self.amp2;
        }
        match self.state {
            State::Delaying => {
                self.out_start_value = out;
                if t.time() < self.delay_frame {
                    return out;
                }
                self.fade_in_timestamp = t.time();
                self.fade_in_duration = (p.fadein * t.frames_per_sec()) as i64;
                self.state = State::FadingIn;
            }
            State::FadingIn => {
                if self.fade_in_duration != 0 && self.ramp_up < 1.0 {
                    self.ramp_up = (t.time() - self.fade_in_timestamp) as f32 / self.fade_in_duration as f32;
                    self.ramp_up *= self.ramp_up;
                } else {
                    self.ramp_up = 1.0;
                    self.state = State::Running;
                }
                out *= self.ramp_up;
                out += self.out_start_value * (1.0 - self.ramp_up);
            }
            State::FadingOut => {
                if self.fade_out_duration != 0 && self.ramp_down != 0.0 {
                    self.ramp_down = 1.0 - (t.time() - self.release_timestamp) as f32 / self.fade_out_duration as f32;
                    self.ramp_down *= self.ramp_down;
                } else {
                    self.ramp_down = 0.0;
                }
                out *= self.ramp_on_release * self.ramp_down;
                out += self.out_start_value * self.ramp_down;
            }
            State::Running => {}
        }
        if self.deterministic {
            self.phase += self.phase_inc;
        } else {
            let tmp = self.incrnd * (1.0 - self.phase) + self.nextincrnd * self.phase;
            self.phase += self.phase_inc * tmp;
        }
        if self.phase >= 1.0 {
            self.phase %= 1.0;
            self.amp1 = self.amp2;
            self.amp2 = (1.0 - self.lfornd) + self.lfornd * rnd();
            self.compute_next_freq_rnd(p);
        }
        out
    }

    pub fn amplfoout(&mut self, p: &LfoParams, t: &AbsTime) -> f32 {
        let o = self.lfoout(p, t);
        (1.0 - self.lfointensity + o).clamp(-1.0, 1.0)
    }

    fn compute_next_freq_rnd(&mut self, p: &LfoParams) {
        if self.deterministic {
            return;
        }
        self.incrnd = self.nextincrnd;
        self.lfofreqrnd = (p.freqrand as f32 / 127.0).powi(2) * 4.0;
        // `lfofreqrnd*(RND*2.0f - 1.0)`: the 1.0 is a double in Zyn.
        let r = (self.lfofreqrnd as f64 * ((rnd() * 2.0) as f64 - 1.0)) as f32;
        self.nextincrnd = 2f32.powf(r);
    }
}

/// C `fmodf(x, 1)` (keeps the sign of x).
#[inline]
fn fmod1(x: f32) -> f32 {
    x % 1.0
}

//! `Synth/ADnote.cpp`: one ADsynth note — up to 8 voices, each an
//! oscillator (with unison, FM/PM/ring/mix/PWM/sync modulation, its own
//! filter, envelopes and LFOs), summed through the global filter, amp
//! envelope and punch.

use super::ad_params::{AdParams, FmType};
use super::envelope::Envelope;
use super::lfo::Lfo;
use super::modfilter::ModFilter;
use super::synthnote::{Legato, LegatoParams, PortaRef, note_random_float, porta_read};
#[allow(unused_imports)]
use PortaRef as _;
use super::util::*;
use super::{NoteCtx, NoteScratch};

pub const OSCIL_SMP_EXTRA_SAMPLES: usize = 5;
const FM_AMP_MULTIPLIER: f32 = 14.712_806;

const SINC_KERNEL: [f32; 19] = [
    0.001_059_625_7,
    0.004_273_442,
    0.003_546_606_3,
    -0.014_555_484,
    -0.047_893_213,
    -0.050_800_02,
    0.046_798_47,
    0.261_064_67,
    0.496_480_23,
    0.600_051_35,
    0.496_480_23,
    0.261_064_67,
    0.046_798_47,
    -0.050_800_02,
    -0.047_893_213,
    -0.014_555_484,
    0.003_546_606_3,
    0.004_273_442,
    0.001_059_625_7,
];

#[derive(Clone, Debug, Default)]
struct Voice {
    enabled: bool,
    unison_size: usize,
    unison_stereo_spread: f32,
    unison_base_freq_rap: Vec<f32>,
    unison_freq_rap: Vec<f32>,
    unison_invert_phase: Vec<bool>,
    vib_step: Vec<f32>,
    vib_position: Vec<f32>,
    vib_amplitude: f32,

    oscfreqhi: Vec<i32>,
    oscfreqlo: Vec<f32>,
    oscfreqhi_fm: Vec<i32>,
    oscfreqlo_fm: Vec<f32>,
    oscposhi: Vec<i32>,
    oscposlo: Vec<f32>,
    oscposhi_fm: Vec<i32>,
    oscposlo_fm: Vec<f32>,
    fm_old_smp: Vec<f32>,

    osc_smp: Vec<f32>,
    fm_smp: Option<Vec<f32>>,
    voice_out: Option<Vec<f32>>,

    fixedfreq: bool,
    fixedfreq_et: u8,
    detune: f32,
    fine_detune: f32,
    fm_detune: f32,
    bend_adjust: f32,
    offset_hz: f32,
    aa_enabled: bool,
    phase_offset: i32,
    noisetype: u8,
    filterbypass: bool,
    filter_fc_ctl_bypass: bool,
    volume: f32,
    panning: f32,

    freq_lfo: Option<Lfo>,
    freq_env: Option<Envelope>,
    amp_lfo: Option<Lfo>,
    amp_env: Option<Envelope>,
    filter: Option<ModFilter>,

    fm_enabled: Option<FmType>,
    fm_freq_fixed: bool,
    fm_voice: i32,
    fm_volume: f32,
    fm_freq_env: Option<Envelope>,
    fm_amp_env: Option<Envelope>,
    sync_enabled: bool,

    oldamplitude: f32,
    newamplitude: f32,
    fm_oldamplitude: f32,
    fm_newamplitude: f32,
    firsttick: bool,
    delay_ticks: i32,
    pinking: [f32; 14],
}

impl Voice {
    fn fm(&self) -> FmType {
        self.fm_enabled.unwrap_or(FmType::None)
    }
}

#[derive(Clone, Debug)]
struct Punch {
    enabled: bool,
    initialvalue: f32,
    dt: f32,
    t: f32,
}

#[derive(Clone, Debug)]
pub struct AdNote {
    voices: Vec<Voice>,
    stereo: bool,
    note_log2_freq: f32,
    velocity: f32,
    portamento: Option<super::synthnote::PortaRef>,
    initial_seed: u32,
    current_prng_state: u32,
    enabled: bool,
    max_unison: usize,
    bandwidth_detune_multiplier: f32,
    g_detune: f32,
    g_panning: f32,
    g_fadein_adjustment: f32,
    g_volume: f32,
    punch: Punch,
    g_freq_env: Envelope,
    g_freq_lfo: Lfo,
    g_amp_env: Envelope,
    g_amp_lfo: Lfo,
    g_filter: ModFilter,
    globaloldamplitude: f32,
    globalnewamplitude: f32,
    osc_seed: [u32; NUM_VOICES],
    fm_seed: [u32; NUM_VOICES],
    pub legato: Legato,
}

pub struct NoteInit {
    pub velocity: f32,
    pub portamento: Option<super::synthnote::PortaRef>,
    pub note_log2_freq: f32,
    pub quiet: bool,
    pub seed: u32,
}

impl AdNote {
    pub fn new(pars: &AdParams, ini: &NoteInit, cx: &mut NoteCtx) -> Self {
        let synth = *cx.synth;
        let g = &pars.global;
        let mut seed_state = ini.seed;
        let basefreq = 2f32.powf(ini.note_log2_freq);
        let g_panning = if g.panning == 0 { note_random_float(&mut seed_state) } else { g.panning as f32 / 128.0 };
        let mut fadein = g.fadein_adjustment as f32 / FADEIN_ADJUSTMENT_SCALE as f32;
        fadein *= fadein;
        let punch = if g.punch_strength != 0 {
            let time = 10f32.powf(3.0 * g.punch_time as f32 / 127.0) / 10000.0;
            let stretch = (440.0 / basefreq).powf(g.punch_stretch as f32 / 64.0);
            Punch {
                enabled: true,
                t: 1.0,
                initialvalue: (10f32.powf(1.5 * g.punch_strength as f32 / 127.0) - 1.0) * vel_f(ini.velocity, g.punch_velocity_sensing),
                dt: 1.0 / (time * synth.samplerate_f * stretch),
            }
        } else {
            Punch { enabled: false, t: 0.0, initialvalue: 0.0, dt: 0.0 }
        };
        let dt = synth.dt();
        // Global envelopes/LFOs are built in `initparameters`, after the
        // voices, so the random sequence matches Zyn's.
        let g_filter = ModFilter::new(&g.filter, synth.samplerate_f, synth.buffersize, g.stereo, basefreq);
        let mut n = Self {
            voices: vec![Voice::default(); NUM_VOICES],
            stereo: g.stereo,
            note_log2_freq: ini.note_log2_freq,
            velocity: ini.velocity,
            portamento: ini.portamento,
            initial_seed: ini.seed,
            current_prng_state: seed_state,
            enabled: true,
            max_unison: 1,
            bandwidth_detune_multiplier: pars.bandwidth_detune_multiplier(),
            g_detune: getdetune(g.detune_type, g.coarse_detune, g.detune),
            g_panning,
            g_fadein_adjustment: fadein,
            g_volume: 0.0,
            punch,
            g_freq_env: Envelope::new(&g.freq_envelope, basefreq, dt),
            g_freq_lfo: Lfo::idle(),
            g_amp_env: Envelope::new(&g.amp_envelope, basefreq, dt),
            g_amp_lfo: Lfo::idle(),
            g_filter,
            globaloldamplitude: 0.0,
            globalnewamplitude: 0.0,
            osc_seed: [1; NUM_VOICES],
            fm_seed: [1; NUM_VOICES],
            legato: Legato::new(&synth, ini.velocity, ini.portamento, ini.note_log2_freq, ini.quiet, ini.seed),
        };
        for nv in 0..NUM_VOICES {
            n.setup_voice(pars, nv, cx);
        }
        n.max_unison = n.voices.iter().map(|v| v.unison_size).max().unwrap_or(1).max(1);
        n.initparameters(pars, cx);
        n
    }

    fn get_random_float(&mut self) -> f32 {
        note_random_float(&mut self.current_prng_state)
    }

    fn setup_voice(&mut self, pars: &AdParams, nv: usize, cx: &mut NoteCtx) {
        let synth = *cx.synth;
        let param = &pars.voices[nv];
        self.osc_seed[nv] = prng();
        {
            let v = &mut self.voices[nv];
            v.pinking = [0.0; 14];
            v.fm_voice = -1;
            v.unison_size = 1;
            if !param.enabled {
                v.enabled = false;
                return;
            }
            v.aa_enabled = param.aa_enabled;
            v.bend_adjust = (param.bend_adjust as i32 - 64) as f32 / 24.0;
            let ov = (param.offset_hz as f32 - 64.0) / 64.0;
            v.offset_hz = 15.0 * (ov * ov.abs().sqrt());
            v.unison_stereo_spread = param.unison_stereo_spread as f32 / 127.0;
        }
        let unison = self.setup_voice_unison(pars, nv, &synth);
        {
            let v = &mut self.voices[nv];
            v.oscfreqhi = vec![0; unison];
            v.oscfreqlo = vec![0.0; unison];
            v.oscfreqhi_fm = vec![0; unison];
            v.oscfreqlo_fm = vec![0.0; unison];
            v.oscposhi = vec![0; unison];
            v.oscposlo = vec![0.0; unison];
            v.oscposhi_fm = vec![0; unison];
            v.oscposlo_fm = vec![0.0; unison];
            v.enabled = true;
            v.fixedfreq = param.fixed_freq;
            v.fixedfreq_et = param.fixed_freq_et;
        }
        self.setup_voice_detune(pars, nv);
        let os = synth.oscilsize;
        let mut vc = nv;
        if param.ext_oscil != -1 {
            vc = param.ext_oscil as usize;
        }
        if !pars.global.hrandgrouping {
            self.osc_seed[vc] = prng();
        }
        let basefreq = self.getvoicebasefreq(nv, 0.0, cx);
        let mut smp = vec![0.0; os + OSCIL_SMP_EXTRA_SAMPLES];
        let res = if param.resonance { Some((&pars.global.reson, cx.ctl.rescenter_relcenter, cx.ctl.resbw_relbw)) } else { None };
        let mut start = pars.voices[vc].oscil.get(&mut smp, basefreq, res, self.osc_seed[vc], cx.osc, cx.fft, synth.samplerate_f);
        for i in 0..OSCIL_SMP_EXTRA_SAMPLES {
            smp[os + i] = smp[i];
        }
        let v = &mut self.voices[nv];
        v.osc_smp = smp;
        v.phase_offset = ((param.oscil_phase as f32 - 64.0) / 128.0 * os as f32 + os as f32 * 4.0) as i32;
        start += v.phase_offset;
        let mut kth = start;
        for k in 0..unison {
            v.oscposhi[k] = kth.rem_euclid(os as i32);
            kth = start + (rnd() * param.unison_phase_randomness as f32 / 127.0 * (os as f32 - 1.0)) as i32;
        }
        v.freq_lfo = None;
        v.freq_env = None;
        v.amp_lfo = None;
        v.amp_env = None;
        v.filter = None;
        v.filterbypass = param.filter_bypass;
        v.filter_fc_ctl_bypass = param.filter_fcctl_bypass;
        self.setup_voice_mod(pars, nv, true, cx);
        let v = &mut self.voices[nv];
        v.sync_enabled = param.sync_enabled;
        v.fm_voice = param.fm_voice;
        v.fm_freq_env = None;
        v.fm_amp_env = None;
        v.fm_old_smp = vec![0.0; unison];
        v.firsttick = true;
        v.delay_ticks = (((param.delay as f32 / 127.0 * 50f32.ln()).exp() - 1.0) / synth.buffersize_f / 10.0 * synth.samplerate_f) as i32;
    }

    fn setup_voice_unison(&mut self, pars: &AdParams, nv: usize, synth: &SynthT) -> usize {
        let param = &pars.voices[nv];
        let v = &mut self.voices[nv];
        let mut unison = (param.unison_size as usize).max(1);
        let is_pwm = param.fm_enabled == FmType::PwMod;
        if param.vtype != 0 {
            unison = unison.min(2);
        } else if is_pwm {
            unison = (unison * 2).min(64);
        }
        v.unison_size = unison;
        v.unison_base_freq_rap = vec![0.0; unison];
        v.unison_freq_rap = vec![0.0; unison];
        v.unison_invert_phase = vec![false; unison];
        let spread = param.unison_frequency_spread_cents();
        let real_spread = 2f32.powf(spread * 0.5 / 1200.0);
        let vib_a = param.unison_vibratto as f32 / 127.0;
        let true_unison = unison / if is_pwm { 2 } else { 1 };
        match true_unison {
            1 => v.unison_base_freq_rap[0] = 1.0,
            2 => {
                v.unison_base_freq_rap[0] = 1.0 / real_spread;
                v.unison_base_freq_rap[1] = real_spread;
            }
            _ => {
                let mut vals = vec![0.0f32; true_unison];
                let (mut mn, mut mx) = (-1e-6f32, 1e-6f32);
                for (k, val) in vals.iter_mut().enumerate() {
                    let step = (k as f32 / (true_unison - 1) as f32) * 2.0 - 1.0;
                    *val = step + (rnd() * 2.0 - 1.0) / (true_unison - 1) as f32;
                    mn = mn.min(*val);
                    mx = mx.max(*val);
                }
                let diff = mx - mn;
                for k in 0..true_unison {
                    let u = (vals[k] - (mx + mn) * 0.5) / diff;
                    v.unison_base_freq_rap[k] = 2f32.powf(spread * u / 1200.0);
                }
            }
        }
        if is_pwm {
            for i in (0..true_unison).rev() {
                let r = v.unison_base_freq_rap[i];
                v.unison_base_freq_rap[2 * i + 1] = r;
                v.unison_base_freq_rap[2 * i] = r;
            }
        }
        if unison > 2 || (!is_pwm && unison > 1) {
            for r in v.unison_base_freq_rap.iter_mut() {
                *r = 1.0 + (*r - 1.0) * (1.0 - vib_a);
            }
        }
        v.vib_step = vec![0.0; unison];
        v.vib_position = vec![0.0; unison];
        v.vib_amplitude = (real_spread - 1.0) * vib_a;
        let incs = synth.samplerate_f / synth.buffersize_f;
        let vib_speed = param.unison_vibratto_speed as f32 / 127.0;
        let base_period = 0.25 * 2f32.powf((1.0 - vib_speed) * 4.0);
        for k in 0..unison {
            v.vib_position[k] = rnd() * 1.8 - 0.9;
            let period = base_period * 2f32.powf(rnd() * 2.0 - 1.0);
            let m = if rnd() < 0.5 { -1.0 } else { 1.0 } * 4.0 / (period * incs);
            v.vib_step[k] = m;
            if is_pwm {
                let mut i = 0;
                while i + 1 < unison {
                    v.vib_step[i + 1] = v.vib_step[i];
                    v.vib_position[i + 1] = v.vib_position[i];
                    i += 2;
                }
            }
        }
        if unison <= 2 {
            if is_pwm && unison > 1 {
                v.vib_step[1] = 0.0;
                v.vib_position[1] = 0.0;
            }
            if is_pwm || unison == 1 {
                v.vib_step[0] = 0.0;
                v.vib_position[0] = 0.0;
                v.vib_amplitude = 0.0;
            }
        }
        v.unison_invert_phase[0] = false;
        if unison != 1 {
            let inv = param.unison_invert_phase as usize;
            match inv {
                0 => v.unison_invert_phase.iter_mut().for_each(|x| *x = false),
                1 => {
                    for x in v.unison_invert_phase.iter_mut() {
                        *x = rnd() > 0.5;
                    }
                }
                _ => {
                    for (k, x) in v.unison_invert_phase.iter_mut().enumerate() {
                        *x = k % inv == 0;
                    }
                }
            }
        }
        unison
    }

    fn setup_voice_detune(&mut self, pars: &AdParams, nv: usize) {
        let p = &pars.voices[nv];
        let gt = pars.global.detune_type;
        let v = &mut self.voices[nv];
        let dt = if p.detune_type != 0 { p.detune_type } else { gt };
        v.detune = getdetune(dt, p.coarse_detune, 8192);
        v.fine_detune = getdetune(dt, 0, p.detune);
        let fdt = if p.fm_detune_type != 0 { p.fm_detune_type } else { gt };
        v.fm_detune = getdetune(fdt, p.fm_coarse_detune, p.fm_detune);
    }

    fn fm_osc_get(&mut self, pars: &AdParams, nv: usize, cx: &mut NoteCtx) {
        let synth = *cx.synth;
        let param = &pars.voices[nv];
        let os = synth.oscilsize;
        let fm = self.voices[nv].fm();
        self.fm_seed[nv] = prng();
        let mut vc = nv;
        if param.ext_fm_oscil != -1 {
            vc = param.ext_fm_oscil as usize;
        }
        let mut tmp = 1.0;
        if pars.voices[vc].fm_oscil.p.adaptiveharmonics != 0 || fm == FmType::Mix || fm == FmType::RingMod {
            tmp = self.getfmvoicebasefreq(nv, cx);
        }
        if !pars.global.hrandgrouping {
            self.fm_seed[vc] = prng();
        }
        let mut smp = vec![0.0; os + OSCIL_SMP_EXTRA_SAMPLES];
        let off = pars.voices[vc].fm_oscil.get(&mut smp, tmp, None, self.fm_seed[vc], cx.osc, cx.fft, synth.samplerate_f);
        for i in 0..OSCIL_SMP_EXTRA_SAMPLES {
            smp[os + i] = smp[i];
        }
        let add = ((param.fm_oscil_phase as f32 - 64.0) / 128.0 * os as f32 + os as f32 * 4.0) as i32;
        let v = &mut self.voices[nv];
        for k in 0..v.unison_size {
            v.oscposhi_fm[k] = (v.oscposhi[k] + off).rem_euclid(os as i32);
            v.oscposhi_fm[k] = (v.oscposhi_fm[k] + add).rem_euclid(os as i32);
        }
        v.fm_smp = Some(smp);
    }

    fn setup_voice_mod(&mut self, pars: &AdParams, nv: usize, first_run: bool, cx: &mut NoteCtx) {
        let param = &pars.voices[nv];
        {
            let v = &mut self.voices[nv];
            v.fm_enabled = Some(if param.vtype != 0 { FmType::None } else { param.fm_enabled });
            v.fm_freq_fixed = param.fm_fixed_freq;
        }
        let v = &self.voices[nv];
        if !first_run && (v.fm() != FmType::None || v.sync_enabled) && v.fm_smp.is_none() && v.fm_voice < 0 {
            self.fm_osc_get(pars, nv, cx);
        }
        let vbf = self.getvoicebasefreq(nv, 0.0, cx);
        let mut damp = (440.0 / vbf).powf(param.fm_volume_damp as f32 / 64.0 - 1.0);
        let fmv = param.fm_volume / 100.0;
        let v = &mut self.voices[nv];
        let vol = match v.fm() {
            FmType::PhaseMod | FmType::PwMod => {
                damp = (440.0 / vbf).powf(param.fm_volume_damp as f32 / 64.0);
                ((fmv * FM_AMP_MULTIPLIER).exp() - 1.0) * damp * 4.0
            }
            FmType::FreqMod => ((fmv * FM_AMP_MULTIPLIER).exp() - 1.0) * damp * 4.0,
            _ => fmv * damp.min(1.0),
        };
        v.fm_volume = vol * vel_f(self.velocity, param.fm_velocity_scale_function);
    }

    fn initparameters(&mut self, pars: &AdParams, cx: &mut NoteCtx) {
        let synth = *cx.synth;
        let basefreq = 2f32.powf(self.note_log2_freq);
        let g = &pars.global;
        let dt = synth.dt();
        self.g_freq_env = Envelope::new(&g.freq_envelope, basefreq, dt);
        self.g_freq_lfo = Lfo::new(&g.freq_lfo, basefreq, cx.time);
        self.g_amp_env = Envelope::new(&g.amp_envelope, basefreq, dt);
        self.g_amp_lfo = Lfo::new(&g.amp_lfo, basefreq, cx.time);
        self.g_volume = db2rap(g.volume) * vel_f(self.velocity, g.amp_velocity_scale_function);
        self.g_filter.env = Some(Envelope::new(&g.filter_envelope, basefreq, dt));
        self.g_filter.lfo = Some(Lfo::new(&g.filter_lfo, basefreq, cx.time));
        self.g_filter.update_sense(self.velocity, g.filter_velocity_scale, g.filter_velocity_scale_function);
        self.g_amp_env.envout_db();
        self.globalnewamplitude =
            self.g_volume * self.g_amp_env.envout_db() * self.g_amp_lfo.amplfoout(&g.amp_lfo, cx.time);
        for i in 0..NUM_VOICES {
            if self.voices[i].fm_voice >= i as i32 {
                self.voices[i].fm_voice = -1;
            }
        }
        for nv in 0..NUM_VOICES {
            if !self.voices[nv].enabled {
                continue;
            }
            let param = &pars.voices[nv];
            let rnd_pan = if param.panning == 0 { Some(self.get_random_float()) } else { None };
            let filter_freq = if param.filter.category == 4 { self.getvoicebasefreq(nv, 0.0, cx) } else { basefreq };
            let vel = self.velocity;
            let v = &mut self.voices[nv];
            v.noisetype = param.vtype;
            v.volume = db2rap(param.volume) * vel_f(vel, param.amp_velocity_scale_function);
            if param.volume == -60.0 {
                v.volume = 0.0;
            }
            if param.volume_minus {
                v.volume = -v.volume;
            }
            v.panning = rnd_pan.unwrap_or(param.panning as f32 / 128.0);
            v.newamplitude = 1.0;
            if param.amp_envelope_enabled {
                let mut e = Envelope::new(&param.amp_envelope, basefreq, dt);
                e.envout_db();
                v.newamplitude *= e.envout_db();
                v.amp_env = Some(e);
            }
            if param.amp_lfo_enabled {
                let mut l = Lfo::new(&param.amp_lfo, basefreq, cx.time);
                v.newamplitude *= l.amplfoout(&param.amp_lfo, cx.time);
                v.amp_lfo = Some(l);
            }
            if param.freq_envelope_enabled {
                v.freq_env = Some(Envelope::new(&param.freq_envelope, basefreq, dt));
            }
            if param.freq_lfo_enabled {
                v.freq_lfo = Some(Lfo::new(&param.freq_lfo, basefreq, cx.time));
            }
            if param.filter_enabled {
                let mut f = ModFilter::new(&param.filter, synth.samplerate_f, synth.buffersize, self.stereo, filter_freq);
                f.update_sense(vel, param.filter_velocity_scale, param.filter_velocity_scale_function);
                if param.filter_envelope_enabled {
                    f = f.with_env(&param.filter_envelope, basefreq, dt);
                }
                if param.filter_lfo_enabled {
                    f = f.with_lfo(&param.filter_lfo, basefreq, cx.time);
                }
                v.filter = Some(f);
            }
            let need_fm = v.fm() != FmType::None && v.fm_voice < 0;
            if need_fm {
                self.fm_osc_get(pars, nv, cx);
            }
            let v = &mut self.voices[nv];
            if param.fm_freq_envelope_enabled {
                v.fm_freq_env = Some(Envelope::new(&param.fm_freq_envelope, basefreq, dt));
            }
            v.fm_newamplitude = v.fm_volume * cx.ctl.fmamp_relamp;
            if param.fm_amp_envelope_enabled {
                let mut e = Envelope::new(&param.fm_amp_envelope, basefreq, dt);
                v.fm_newamplitude *= e.envout_db();
                v.fm_amp_env = Some(e);
            }
        }
        let bs = synth.buffersize;
        for nv in 0..NUM_VOICES {
            let used = ((nv + 1)..NUM_VOICES).any(|i| self.voices[i].fm_voice == nv as i32);
            if used {
                self.voices[nv].voice_out = Some(vec![0.0; bs]);
            }
        }
    }

    pub fn legatonote(&mut self, pars: &AdParams, lp: &LegatoParams, cx: &mut NoteCtx) {
        if self.legato.update(lp) {
            return;
        }
        self.portamento = lp.portamento;
        self.note_log2_freq = lp.note_log2_freq;
        self.initial_seed = lp.seed;
        self.current_prng_state = lp.seed;
        self.velocity = lp.velocity.min(1.0);
        let basefreq = 2f32.powf(self.note_log2_freq);
        let g = &pars.global;
        self.g_detune = getdetune(g.detune_type, g.coarse_detune, g.detune);
        self.bandwidth_detune_multiplier = pars.bandwidth_detune_multiplier();
        if g.panning != 0 {
            self.g_panning = g.panning as f32 / 128.0;
        }
        self.g_filter.update_sense(self.velocity, g.filter_velocity_scale, g.filter_velocity_scale_function);
        for nv in 0..NUM_VOICES {
            if !self.voices[nv].enabled {
                continue;
            }
            let p = &pars.voices[nv];
            self.voices[nv].fixedfreq = p.fixed_freq;
            self.voices[nv].fixedfreq_et = p.fixed_freq_et;
            self.setup_voice_detune(pars, nv);
            let vel = self.velocity;
            let v = &mut self.voices[nv];
            if let Some(f) = v.filter.as_mut() {
                f.update_sense(vel, p.filter_velocity_scale, p.filter_velocity_scale_function);
            }
            v.filterbypass = p.filter_bypass;
            v.filter_fc_ctl_bypass = p.filter_fcctl_bypass;
            v.fm_voice = p.fm_voice;
            let vbf = self.getvoicebasefreq(nv, 0.0, cx);
            let v = &mut self.voices[nv];
            let mut damp = (440.0 / vbf).powf(p.fm_volume_damp as f32 / 64.0 - 1.0);
            let vol = match v.fm() {
                FmType::PhaseMod | FmType::PwMod => {
                    damp = (440.0 / vbf).powf(p.fm_volume_damp as f32 / 64.0);
                    ((p.fm_volume / 100.0 * FM_AMP_MULTIPLIER).exp() - 1.0) * damp * 4.0
                }
                FmType::FreqMod => ((p.fm_volume / 100.0 * FM_AMP_MULTIPLIER).exp() - 1.0) * damp * 4.0,
                _ => p.fm_volume / 100.0 * damp.min(1.0),
            };
            v.fm_volume = vol * vel_f(vel, p.fm_velocity_scale_function);
        }
        self.g_volume = db2rap(g.volume) * vel_f(self.velocity, g.amp_velocity_scale_function);
        self.g_filter.update_sense(self.velocity, g.filter_velocity_scale, g.filter_velocity_scale_function);
        self.g_filter.update_note_freq(basefreq, &g.filter);
        for i in 0..NUM_VOICES {
            if self.voices[i].fm_voice >= i as i32 {
                self.voices[i].fm_voice = -1;
            }
        }
        for nv in 0..NUM_VOICES {
            if !self.voices[nv].enabled {
                continue;
            }
            let p = &pars.voices[nv];
            let rnd_pan = if p.panning == 0 { Some(self.get_random_float()) } else { None };
            let vel = self.velocity;
            let v = &mut self.voices[nv];
            v.noisetype = p.vtype;
            v.volume = db2rap(p.volume) * vel_f(vel, p.amp_velocity_scale_function);
            if p.volume == -60.0 {
                v.volume = 0.0;
            }
            if p.volume_minus {
                v.volume = -v.volume;
            }
            v.aa_enabled = p.aa_enabled;
            v.panning = rnd_pan.unwrap_or(p.panning as f32 / 128.0);
            v.newamplitude = 1.0;
            if p.amp_envelope_enabled {
                if let Some(e) = v.amp_env.as_mut() {
                    v.newamplitude *= e.envout_db();
                }
            }
            if p.amp_lfo_enabled {
                if let Some(l) = v.amp_lfo.as_mut() {
                    v.newamplitude *= l.amplfoout(&p.amp_lfo, cx.time);
                }
            }
            if let Some(f) = v.filter.as_mut() {
                f.update_sense(vel, p.filter_velocity_scale, p.filter_velocity_scale_function);
                f.update_note_freq(basefreq, &p.filter);
            }
            if (v.fm() != FmType::None || v.sync_enabled) && v.fm_voice < 0 {
                self.fm_seed[nv] = prng();
                let mut vc = nv;
                if p.ext_fm_oscil != -1 {
                    vc = p.ext_fm_oscil as usize;
                }
                if !g.hrandgrouping {
                    self.fm_seed[vc] = prng();
                }
            }
            let v = &mut self.voices[nv];
            v.fm_newamplitude = v.fm_volume * cx.ctl.fmamp_relamp;
            if p.fm_amp_envelope_enabled {
                if let Some(e) = v.fm_amp_env.as_mut() {
                    v.fm_newamplitude *= e.envout_db();
                }
            }
        }
    }

    fn getvoicebasefreq(&self, nv: usize, adjust_log2: f32, cx: &NoteCtx) -> f32 {
        let v = &self.voices[nv];
        let detune = v.detune / 100.0
            + v.fine_detune / 100.0 * cx.ctl.bandwidth_relbw * self.bandwidth_detune_multiplier
            + self.g_detune / 100.0;
        if !v.fixedfreq {
            2f32.powf(self.note_log2_freq + detune / 12.0 + adjust_log2)
        } else {
            let et = v.fixedfreq_et as i32;
            let mut fl = 440f32.log2();
            if et != 0 {
                let tmp = (self.note_log2_freq - fl) * (2f32.powf((et - 1) as f32 / 63.0) - 1.0);
                if et <= 64 {
                    fl += tmp;
                } else {
                    fl += tmp * 3f32.log2();
                }
            }
            2f32.powf(fl + detune / 12.0 + adjust_log2)
        }
    }

    fn getfmvoicebasefreq(&self, nv: usize, cx: &NoteCtx) -> f32 {
        self.getvoicebasefreq(nv, self.voices[nv].fm_detune / 1200.0, cx)
    }

    fn compute_unison_freq_rap(&mut self, nv: usize, cx: &NoteCtx) {
        let relbw = cx.ctl.bandwidth_relbw * self.bandwidth_detune_multiplier;
        let v = &mut self.voices[nv];
        if v.unison_size == 1 {
            v.unison_freq_rap[0] = 1.0;
            return;
        }
        for k in 0..v.unison_size {
            let mut pos = v.vib_position[k];
            let mut step = v.vib_step[k];
            pos += step;
            if pos <= -1.0 {
                pos = -1.0;
                step = -step;
            }
            if pos >= 1.0 {
                pos = 1.0;
                step = -step;
            }
            let vib = (pos - 0.333_333_34 * pos * pos * pos) * 1.5;
            v.unison_freq_rap[k] = 1.0 + ((v.unison_base_freq_rap[k] - 1.0) + vib * v.vib_amplitude) * relbw;
            v.vib_position[k] = pos;
            v.vib_step[k] = step;
        }
    }

    fn setfreq(&mut self, nv: usize, in_freq: f32, synth: &SynthT) {
        let v = &mut self.voices[nv];
        for k in 0..v.unison_size {
            let freq = in_freq.abs() * v.unison_freq_rap[k];
            let speed = (freq * synth.oscilsize_f / synth.samplerate_f).min(synth.oscilsize_f);
            v.oscfreqhi[k] = f2i(speed);
            v.oscfreqlo[k] = speed - speed.floor();
        }
    }

    fn setfreq_fm(&mut self, nv: usize, in_freq: f32, synth: &SynthT) {
        let v = &mut self.voices[nv];
        for k in 0..v.unison_size {
            let freq = in_freq.abs() * v.unison_freq_rap[k];
            let speed = (freq * synth.oscilsize_f / synth.samplerate_f).min(synth.samplerate_f);
            v.oscfreqhi_fm[k] = f2i(speed);
            v.oscfreqlo_fm[k] = speed - speed.floor();
        }
    }

    fn computecurrentparameters(&mut self, pars: &AdParams, cx: &mut NoteCtx) {
        let synth = *cx.synth;
        let ctl = cx.ctl;
        let relfreq = ctl.filtercutoff_relfreq;
        let g = &pars.global;
        let globalpitch = 0.01
            * (self.g_freq_env.envout() + self.g_freq_lfo.lfoout(&g.freq_lfo, cx.time) * ctl.modwheel_relmod);
        self.globaloldamplitude = self.globalnewamplitude;
        self.globalnewamplitude =
            self.g_volume * self.g_amp_env.envout_db() * self.g_amp_lfo.amplfoout(&g.amp_lfo, cx.time);
        self.g_filter.update(relfreq, ctl.filterq_relq, &g.filter, Some(&g.filter_lfo), cx.time, cx.changed);

        let mut porta = 0.0;
        if let Some(r) = self.portamento {
            match porta_read(cx.portas, r) {
                Some(p) => {
                    porta = p.freqdelta_log2;
                    if !p.active {
                        self.portamento = None;
                    }
                }
                None => self.portamento = None,
            }
        }
        for nv in 0..NUM_VOICES {
            if !self.voices[nv].enabled {
                continue;
            }
            self.voices[nv].delay_ticks -= 1;
            if self.voices[nv].delay_ticks > 0 {
                continue;
            }
            self.compute_unison_freq_rap(nv, cx);
            let p = &pars.voices[nv];
            let v = &mut self.voices[nv];
            v.oldamplitude = v.newamplitude;
            v.newamplitude = 1.0;
            if let Some(e) = v.amp_env.as_mut() {
                v.newamplitude *= e.envout_db();
            }
            if let Some(l) = v.amp_lfo.as_mut() {
                v.newamplitude *= l.amplfoout(&p.amp_lfo, cx.time);
            }
            if let Some(f) = v.filter.as_mut() {
                let vr = if !v.filter_fc_ctl_bypass { relfreq } else { 0.0 };
                f.update(vr, ctl.filterq_relq, &p.filter, Some(&p.filter_lfo), cx.time, cx.changed);
            }
            if v.noisetype == 0 {
                let mut voicepitch = 0.0;
                if let Some(l) = v.freq_lfo.as_mut() {
                    voicepitch += l.lfoout(&p.freq_lfo, cx.time) / 100.0 * ctl.bandwidth_relbw;
                }
                if let Some(e) = v.freq_env.as_mut() {
                    voicepitch += e.envout() / 100.0;
                }
                let bend = v.bend_adjust;
                let offset = v.offset_hz;
                let mut voicefreq = self.getvoicebasefreq(nv, porta + (voicepitch + globalpitch) / 12.0, cx);
                voicefreq *= ctl.pitch_relfreq.powf(bend);
                self.setfreq(nv, voicefreq + offset, &synth);
                let v = &mut self.voices[nv];
                if v.fm() != FmType::None || v.sync_enabled {
                    let mut fmrel = v.fm_detune / 100.0;
                    if let Some(e) = v.fm_freq_env.as_mut() {
                        let ev = e.envout();
                        if nv == 0 && std::env::var("ZDEBUG").is_ok() {
                            eprintln!("ENV {:e} {:08x} det {:08x}", ev, ev.to_bits(), v.fm_detune.to_bits());
                        }
                        fmrel += ev / 100.0;
                    }
                    let fmfreq = if v.fm_freq_fixed { 2f32.powf(fmrel / 12.0) * 440.0 } else { 2f32.powf(fmrel / 12.0) * voicefreq };
                    self.setfreq_fm(nv, fmfreq, &synth);
                    let v = &mut self.voices[nv];
                    v.fm_oldamplitude = v.fm_newamplitude;
                    v.fm_newamplitude = v.fm_volume * ctl.fmamp_relamp;
                    if let Some(e) = v.fm_amp_env.as_mut() {
                        v.fm_newamplitude *= e.envout_db();
                    }
                    if nv == 0 && std::env::var("ZDEBUG").is_ok() {
                        eprintln!("FM vf {:08x} fmf {:08x} hi {} lo {:08x} amp {:08x}", voicefreq.to_bits(), fmfreq.to_bits(), v.oscfreqhi_fm[0], v.oscfreqlo_fm[0].to_bits(), v.fm_newamplitude.to_bits());
                    }
                }
            }
        }
    }

    // ---------------- oscillators ----------------

    fn osc_linear(v: &mut Voice, tw: &mut [Vec<f32>], bs: usize, os: usize) {
        let mask = os as i32 - 1;
        for k in 0..v.unison_size {
            let mut poshi = v.oscposhi[k];
            let mut poslo = (v.oscposlo[k] * 16_777_216.0) as i32;
            let freqhi = v.oscfreqhi[k];
            let freqlo = (v.oscfreqlo[k] * 16_777_216.0) as i32;
            let smps = &v.osc_smp;
            let t = &mut tw[k];
            for s in t.iter_mut().take(bs) {
                let ph = poshi as usize;
                *s = (smps[ph] * (0x0100_0000 - poslo) as f32 + smps[ph + 1] * poslo as f32) / 16_777_216.0;
                poslo += freqlo;
                poshi += freqhi + (poslo >> 24);
                poslo &= 0xff_ffff;
                poshi &= mask;
            }
            v.oscposhi[k] = poshi;
            v.oscposlo[k] = poslo as f32 / 16_777_216.0;
        }
    }

    fn osc_sinc(v: &mut Voice, tw: &mut [Vec<f32>], bs: usize, os: usize) {
        let mask = os as i32 - 1;
        let half = (SINC_KERNEL.len() as i32 - 1) / 2;
        for k in 0..v.unison_size {
            let mut poshi = v.oscposhi[k];
            let mut poslo = (v.oscposlo[k] * (1 << 24) as f32) as i32;
            let freqhi = v.oscfreqhi[k];
            let freqlo = (v.oscfreqlo[k] * (1 << 24) as f32) as i32;
            let ofhi = v.oscfreqhi[k] / 2;
            let oflo = ((v.oscfreqlo[k] / 2.0) * (1 << 24) as f32) as i32;
            let smps = &v.osc_smp;
            let t = &mut tw[k];
            for s in t.iter_mut().take(bs) {
                let mut olo = poslo - half * oflo;
                let uflow = olo >> 24;
                let mut ohi = poshi - half * ofhi - ((0 - uflow) & 0xff);
                olo &= 0xff_ffff;
                ohi &= mask;
                let mut out = 0.0;
                for kv in SINC_KERNEL.iter() {
                    let ph = ohi as usize;
                    out += kv * (smps[ph] * ((1 << 24) - olo) as f32 + smps[ph + 1] * olo as f32) / (1 << 24) as f32;
                    olo += oflo;
                    ohi += ofhi + (olo >> 24);
                    olo &= 0xff_ffff;
                    ohi &= mask;
                }
                poslo += freqlo;
                poshi += freqhi + (poslo >> 24);
                poslo &= 0xff_ffff;
                poshi &= mask;
                *s = out;
            }
            v.oscposhi[k] = poshi;
            v.oscposlo[k] = poslo as f32 / (1 << 24) as f32;
        }
    }

    fn osc_sync(&mut self, nv: usize, tw: &mut [Vec<f32>], bs: usize, os: usize) {
        let mask = os as i32 - 1;
        let fmvoice = self.voices[nv].fm_voice;
        let src: Option<Vec<f32>> = if fmvoice >= 0 { self.voices[fmvoice as usize].voice_out.clone() } else { None };
        let v = &mut self.voices[nv];
        v.fm_newamplitude = v.fm_newamplitude.min(1.0);
        v.fm_oldamplitude = v.fm_oldamplitude.min(1.0);
        if fmvoice >= 0 {
            for k in 0..v.unison_size {
                match &src {
                    Some(s) => tw[k][..bs].copy_from_slice(&s[..bs]),
                    None => tw[k][..bs].fill(0.0),
                }
            }
        } else {
            for k in 0..v.unison_size {
                let mut poshi = v.oscposhi_fm[k];
                let mut poslo = (v.oscposlo_fm[k] * (1 << 24) as f32) as i32;
                let freqhi = v.oscfreqhi_fm[k];
                let freqlo = (v.oscfreqlo_fm[k] * (1 << 24) as f32) as i32;
                let t = &mut tw[k];
                match v.fm_smp.as_ref() {
                    Some(smps) => {
                        for s in t.iter_mut().take(bs) {
                            let ph = poshi as usize;
                            *s = (smps[ph] * ((1 << 24) - poslo) as f32 + smps[ph + 1] * poslo as f32) / (1 << 24) as f32;
                            poslo += freqlo;
                            if poslo >= 1 << 24 {
                                poslo &= 0xff_ffff;
                                poshi += 1;
                            }
                            poshi += freqhi;
                            poshi &= mask;
                        }
                    }
                    None => t[..bs].fill(0.0),
                }
                v.oscposhi_fm[k] = poshi;
                v.oscposlo_fm[k] = poslo as f32 / (1 << 24) as f32;
            }
        }
        let fmnone = v.fm() == FmType::None;
        for k in 0..v.unison_size {
            let mut poshi = v.oscposhi[k];
            let mut poslo = (v.oscposlo[k] * 16_777_216.0) as i32;
            let freqhi = v.oscfreqhi[k];
            let freqlo = (v.oscfreqlo[k] * 16_777_216.0) as i32;
            let smps = &v.osc_smp;
            let t = &mut tw[k];
            let mut fmold = v.fm_old_smp[k];
            for i in 0..bs {
                let x = t[i];
                if ((x > 0.0 && fmold <= 0.0) || (x >= 0.0 && fmold < 0.0))
                    && ((poshi as f32 / os as f32) < v.fm_newamplitude || !fmnone)
                {
                    fmold = x;
                    let ph = poshi as usize;
                    t[i] = ((smps[ph] * (0x0100_0000 - poslo) as f32 + smps[ph + 1] * poslo as f32) / 16_777_216.0 + smps[0]) / 2.0;
                    poslo = 0;
                    poshi = 0;
                    continue;
                }
                fmold = x;
                let ph = poshi as usize;
                t[i] = (smps[ph] * (0x0100_0000 - poslo) as f32 + smps[ph + 1] * poslo as f32) / 16_777_216.0;
                poslo += freqlo;
                poshi += freqhi + (poslo >> 24);
                poslo &= 0xff_ffff;
                poshi &= mask;
            }
            v.oscposhi[k] = poshi;
            v.oscposlo[k] = poslo as f32 / 16_777_216.0;
            v.fm_old_smp[k] = fmold;
        }
    }

    fn base_osc(&mut self, nv: usize, tw: &mut [Vec<f32>], bs: usize, os: usize) {
        if self.voices[nv].sync_enabled {
            self.osc_sync(nv, tw, bs, os);
        } else {
            Self::osc_linear(&mut self.voices[nv], tw, bs, os);
        }
    }

    fn osc_mix(&mut self, nv: usize, tw: &mut [Vec<f32>], bs: usize, os: usize, ring: bool) {
        self.base_osc(nv, tw, bs, os);
        let mask = os as i32 - 1;
        let fmvoice = self.voices[nv].fm_voice;
        let src: Option<Vec<f32>> = if fmvoice >= 0 { self.voices[fmvoice as usize].voice_out.clone() } else { None };
        let v = &mut self.voices[nv];
        v.fm_newamplitude = v.fm_newamplitude.min(1.0);
        v.fm_oldamplitude = v.fm_oldamplitude.min(1.0);
        let (oa, na) = (v.fm_oldamplitude, v.fm_newamplitude);
        if fmvoice >= 0 {
            let zero = vec![0.0; bs];
            let s = src.as_deref().unwrap_or(&zero);
            for k in 0..v.unison_size {
                let t = &mut tw[k];
                for i in 0..bs {
                    let amp = interpolate_amplitude(oa, na, i, bs);
                    if ring {
                        t[i] *= (1.0 - amp) + amp * s[i];
                    } else {
                        t[i] = t[i] * (1.0 - amp) + amp * s[i];
                    }
                }
            }
        } else {
            for k in 0..v.unison_size {
                let mut poshi = v.oscposhi_fm[k];
                let mut poslo = v.oscposlo_fm[k];
                let freqhi = v.oscfreqhi_fm[k];
                let freqlo = v.oscfreqlo_fm[k];
                let t = &mut tw[k];
                let Some(fm) = v.fm_smp.as_ref() else { continue };
                for i in 0..bs {
                    let amp = interpolate_amplitude(oa, na, i, bs);
                    let ph = poshi as usize;
                    let m = fm[ph] * (1.0 - poslo) + fm[ph + 1] * poslo;
                    if ring {
                        t[i] *= m * amp + (1.0 - amp);
                    } else {
                        t[i] = t[i] * (1.0 - amp) + amp * m;
                    }
                    poslo += freqlo;
                    if poslo >= 1.0 {
                        poslo -= 1.0;
                        poshi += 1;
                    }
                    poshi += freqhi;
                    poshi &= mask;
                }
                v.oscposhi_fm[k] = poshi;
                v.oscposlo_fm[k] = poslo;
            }
        }
    }

    fn osc_fm(&mut self, nv: usize, tw: &mut [Vec<f32>], bs: usize, synth: &SynthT, mode: FmType) {
        let os = synth.oscilsize;
        let mask = os as i32 - 1;
        let fmvoice = self.voices[nv].fm_voice;
        let src: Option<Vec<f32>> = if fmvoice >= 0 { self.voices[fmvoice as usize].voice_out.clone() } else { None };
        let pwm = mode == FmType::PwMod;
        let v = &mut self.voices[nv];
        if fmvoice >= 0 {
            for k in 0..v.unison_size {
                let t = &mut tw[k];
                match &src {
                    Some(s) => {
                        if pwm && (k & 1) == 1 {
                            for i in 0..bs {
                                t[i] = -s[i];
                            }
                        } else {
                            t[..bs].copy_from_slice(&s[..bs]);
                        }
                    }
                    None => t[..bs].fill(0.0),
                }
            }
        } else {
            for k in 0..v.unison_size {
                let mut poshi = v.oscposhi_fm[k];
                let mut poslo = (v.oscposlo_fm[k] * (1 << 24) as f32) as i32;
                let freqhi = v.oscfreqhi_fm[k];
                let freqlo = (v.oscfreqlo_fm[k] * (1 << 24) as f32) as i32;
                let t = &mut tw[k];
                let Some(smps) = v.fm_smp.as_ref() else {
                    t[..bs].fill(0.0);
                    continue;
                };
                for s in t.iter_mut().take(bs) {
                    let ph = poshi as usize;
                    *s = (smps[ph] * ((1 << 24) - poslo) as f32 + smps[ph + 1] * poslo as f32) / (1 << 24) as f32;
                    if pwm && (k & 1) == 1 {
                        *s = -*s;
                    }
                    poslo += freqlo;
                    if poslo >= 1 << 24 {
                        poslo &= 0xff_ffff;
                        poshi += 1;
                    }
                    poshi += freqhi;
                    poshi &= mask;
                }
                v.oscposhi_fm[k] = poshi;
                v.oscposlo_fm[k] = poslo as f32 / (1 << 24) as f32;
            }
        }
        let (oa, na) = (v.fm_oldamplitude, v.fm_newamplitude);
        if above_amplitude_threshold(oa, na) {
            for k in 0..v.unison_size {
                for i in 0..bs {
                    tw[k][i] *= interpolate_amplitude(oa, na, i, bs);
                }
            }
        } else {
            for k in 0..v.unison_size {
                for s in tw[k].iter_mut().take(bs) {
                    *s *= na;
                }
            }
        }
        if mode == FmType::FreqMod {
            let norm = synth.oscilsize_f / 262_144.0 * 44100.0 / synth.samplerate_f;
            for k in 0..v.unison_size {
                let mut fmold = v.fm_old_smp[k];
                for s in tw[k].iter_mut().take(bs) {
                    fmold += *s * norm;
                    *s = fmold;
                }
                v.fm_old_smp[k] = fmold;
            }
        } else {
            let norm = synth.oscilsize_f / 262_144.0;
            for k in 0..v.unison_size {
                for s in tw[k].iter_mut().take(bs) {
                    *s *= norm;
                }
            }
        }
        for k in 0..v.unison_size {
            let smps = &v.osc_smp;
            let t = &mut tw[k];
            let mut fmold = v.fm_old_smp[k];
            let mut poshi = v.oscposhi[k];
            let mut poslo = (v.oscposlo[k] * (1 << 24) as f32) as i32;
            let freqhi = v.oscfreqhi[k];
            let freqlo = (v.oscfreqlo[k] * (1 << 24) as f32) as i32;
            for s in t.iter_mut().take(bs) {
                if v.sync_enabled && ((*s > 0.0 && fmold <= 0.0) || (*s >= 0.0 && fmold < 0.0)) {
                    poshi = 0;
                    poslo = 0;
                }
                fmold = *s;
                let modhi = f2i(*s);
                let mut modlo = *s - modhi as f32;
                if modlo < 0.0 {
                    modlo += 1.0;
                }
                let mut carhi = poshi + modhi;
                let mut carlo = (poslo as f32 + modlo) as i32;
                if pwm && (k & 1) == 1 {
                    carhi += v.phase_offset;
                }
                if carlo >= 1 << 24 {
                    carhi += 1;
                    carlo &= 0xff_ffff;
                }
                carhi &= mask;
                let ph = carhi as usize;
                *s = (smps[ph] * ((1 << 24) - carlo) as f32 + smps[ph + 1] * carlo as f32) / (1 << 24) as f32;
                poslo += freqlo;
                if poslo >= 1 << 24 {
                    poslo &= 0xff_ffff;
                    poshi += 1;
                }
                poshi += freqhi;
                poshi &= mask;
            }
            v.oscposhi[k] = poshi;
            v.oscposlo[k] = poslo as f32 / (1 << 24) as f32;
            if nv == 0 && k == 0 && std::env::var("ZDEBUG").is_ok() {
                eprintln!("CAR poshi {} poslo {:08x} fmold {:08x} tw {:08x}", poshi, v.oscposlo[k].to_bits(), v.fm_old_smp[k].to_bits(), tw[0][bs - 1].to_bits());
            }
        }
    }

    // ---------------- output ----------------

    /// `noteout`: renders one engine buffer into `outl`/`outr`.
    pub fn noteout(&mut self, pars: &AdParams, outl: &mut [f32], outr: &mut [f32], cx: &mut NoteCtx, sc: &mut NoteScratch) {
        let synth = *cx.synth;
        let bs = synth.buffersize;
        let os = synth.oscilsize;
        outl[..bs].fill(0.0);
        outr[..bs].fill(0.0);
        if !self.enabled {
            return;
        }
        sc.bypassl[..bs].fill(0.0);
        sc.bypassr[..bs].fill(0.0);
        for nv in 0..NUM_VOICES {
            if !self.voices[nv].enabled || self.voices[nv].delay_ticks > 0 {
                continue;
            }
            self.setup_voice_detune(pars, nv);
            self.setup_voice_mod(pars, nv, false, cx);
        }
        self.computecurrentparameters(pars, cx);
        let stereo = self.stereo;
        for nv in 0..NUM_VOICES {
            if !self.voices[nv].enabled || self.voices[nv].delay_ticks > 0 {
                continue;
            }
            let tw = &mut sc.tmpwave_unison;
            match self.voices[nv].noisetype {
                0 => match self.voices[nv].fm() {
                    FmType::Mix => self.osc_mix(nv, tw, bs, os, false),
                    FmType::RingMod => self.osc_mix(nv, tw, bs, os, true),
                    m @ (FmType::FreqMod | FmType::PhaseMod | FmType::PwMod) => self.osc_fm(nv, tw, bs, &synth, m),
                    FmType::None => {
                        if self.voices[nv].aa_enabled {
                            Self::osc_sinc(&mut self.voices[nv], tw, bs, os);
                        } else {
                            self.base_osc(nv, tw, bs, os);
                        }
                    }
                },
                1 => {
                    for k in 0..self.voices[nv].unison_size {
                        for s in tw[k].iter_mut().take(bs) {
                            *s = rnd() * 2.0 - 1.0;
                        }
                    }
                }
                2 => {
                    let v = &mut self.voices[nv];
                    for k in 0..v.unison_size {
                        let base = if k > 0 { 7 } else { 0 };
                        let f = &mut v.pinking[base..base + 7];
                        for s in tw[k].iter_mut().take(bs) {
                            let white = (rnd() - 0.5) / 4.0;
                            f[0] = 0.99886 * f[0] + white * 0.055_517_9;
                            f[1] = 0.99332 * f[1] + white * 0.075_075_9;
                            f[2] = 0.96900 * f[2] + white * 0.153_852;
                            f[3] = 0.86650 * f[3] + white * 0.310_485_6;
                            f[4] = 0.55000 * f[4] + white * 0.532_952_2;
                            f[5] = -0.7616 * f[5] - white * 0.016_898;
                            *s = f[0] + f[1] + f[2] + f[3] + f[4] + f[5] + f[6] + white * 0.5362;
                            f[6] = white * 0.115_926;
                        }
                    }
                }
                _ => {
                    for k in 0..self.voices[nv].unison_size {
                        tw[k][..bs].fill(1.0);
                    }
                }
            }
            // mix sub-voices
            let tl = &mut sc.tmpwavel[..bs];
            let tr = &mut sc.tmpwaver[..bs];
            tl.fill(0.0);
            tr.fill(0.0);
            let v = &mut self.voices[nv];
            let us = v.unison_size;
            let is_pwm = v.fm() == FmType::PwMod;
            for k in 0..us {
                let t = &tw[k];
                if stereo {
                    let mut sp = 0.0;
                    if is_pwm {
                        if us > 2 {
                            sp = (k / 2) as f32 / (us / 2 - 1) as f32 * 2.0 - 1.0;
                        }
                    } else if us > 1 {
                        sp = k as f32 / (us - 1) as f32 * 2.0 - 1.0;
                    }
                    let spread = v.unison_stereo_spread * 2.0;
                    if spread > 1.0 {
                        let s1 = if sp >= 0.0 { 1.0 } else { -1.0 };
                        sp = (2.0 - spread) * sp + (spread - 1.0) * s1;
                    } else {
                        sp *= spread;
                    }
                    if us == 1 || (is_pwm && us == 2) {
                        sp = 0.0;
                    }
                    let pan = (sp + 1.0) * 0.5;
                    let (mut lv, mut rv) = if cx.const_power { ((1.0 - pan).sqrt(), pan.sqrt()) } else { (((1.0 - pan) * 2.0).min(1.0), (pan * 2.0).min(1.0)) };
                    if v.unison_invert_phase[k] {
                        lv = -lv;
                        rv = -rv;
                    }
                    for i in 0..bs {
                        tl[i] += t[i] * lv;
                        tr[i] += t[i] * rv;
                    }
                } else {
                    for i in 0..bs {
                        tl[i] += t[i];
                    }
                }
            }
            let ua = 1.0 / (us as f32).sqrt();
            let oldam = v.oldamplitude * ua;
            let newam = v.newamplitude * ua;
            if above_amplitude_threshold(oldam, newam) {
                let mut rest = bs;
                if newam > oldam && newam - oldam > 0.25 {
                    rest = 10.min(bs);
                    for i in 0..bs - rest {
                        tl[i] *= oldam;
                        if stereo {
                            tr[i] *= oldam;
                        }
                    }
                }
                for i in 0..rest {
                    let amp = interpolate_amplitude(oldam, newam, i, rest);
                    tl[i + bs - rest] *= amp;
                    if stereo {
                        tr[i + bs - rest] *= amp;
                    }
                }
            } else {
                for i in 0..bs {
                    tl[i] *= newam;
                    if stereo {
                        tr[i] *= newam;
                    }
                }
            }
            if v.firsttick {
                let fa = self.g_fadein_adjustment;
                fadein_with(tl, fa);
                if stereo {
                    fadein_with(tr, fa);
                }
                self.voices[nv].firsttick = false;
            }
            let v = &mut self.voices[nv];
            if let Some(f) = v.filter.as_mut() {
                if stereo {
                    f.filter(tl, Some(tr));
                } else {
                    f.filter(tl, None);
                }
            }
            let env_done = v.amp_env.as_ref().map(|e| e.finished()).unwrap_or(false);
            if env_done {
                for i in 0..bs {
                    let g = 1.0 - i as f32 / synth.buffersize_f;
                    tl[i] *= g;
                    if stereo {
                        tr[i] *= g;
                    }
                }
            }
            if let Some(vo) = v.voice_out.as_mut() {
                for i in 0..bs {
                    vo[i] = if stereo { tl[i] + tr[i] } else { tl[i] };
                }
            }
            let (vol, pan) = (v.volume, v.panning);
            let (dl, dr): (&mut [f32], &mut [f32]) = if !v.filterbypass { (&mut outl[..bs], &mut outr[..bs]) } else { (&mut sc.bypassl[..bs], &mut sc.bypassr[..bs]) };
            if stereo {
                for i in 0..bs {
                    dl[i] += tl[i] * vol * (1.0 - pan) * 2.0;
                    dr[i] += tr[i] * vol * pan * 2.0;
                }
            } else {
                for i in 0..bs {
                    dl[i] += tl[i] * vol;
                }
            }
            if env_done {
                self.kill_voice(nv, bs);
            }
        }
        if stereo {
            self.g_filter.filter(&mut outl[..bs], Some(&mut outr[..bs]));
        } else {
            self.g_filter.filter(&mut outl[..bs], None);
            let (l, r) = (&outl[..bs], &mut outr[..bs]);
            r.copy_from_slice(l);
            let bl = sc.bypassl[..bs].to_vec();
            sc.bypassr[..bs].copy_from_slice(&bl);
        }
        for i in 0..bs {
            outl[i] += sc.bypassl[i];
            outr[i] += sc.bypassr[i];
        }
        let (oa, na, pan) = (self.globaloldamplitude, self.globalnewamplitude, self.g_panning);
        if above_amplitude_threshold(oa, na) {
            for i in 0..bs {
                let t = interpolate_amplitude(oa, na, i, bs);
                outl[i] *= t * (1.0 - pan);
                outr[i] *= t * pan;
            }
        } else {
            for i in 0..bs {
                outl[i] *= na * (1.0 - pan);
                outr[i] *= na * pan;
            }
        }
        if self.punch.enabled {
            for i in 0..bs {
                let pa = self.punch.initialvalue * self.punch.t + 1.0;
                outl[i] *= pa;
                outr[i] *= pa;
                self.punch.t -= self.punch.dt;
                if self.punch.t < 0.0 {
                    self.punch.enabled = false;
                    break;
                }
            }
        }
        if let Some(lp) = self.legato.apply(&mut outl[..bs], &mut outr[..bs]) {
            self.legatonote(pars, &lp, cx);
        }
        if self.g_amp_env.finished() {
            for i in 0..bs {
                let t = 1.0 - i as f32 / synth.buffersize_f;
                outl[i] *= t;
                outr[i] *= t;
            }
            self.kill_note();
        }
    }

    fn kill_voice(&mut self, nv: usize, bs: usize) {
        let v = &mut self.voices[nv];
        v.enabled = false;
        v.freq_env = None;
        v.freq_lfo = None;
        v.amp_env = None;
        v.amp_lfo = None;
        v.filter = None;
        v.fm_freq_env = None;
        v.fm_amp_env = None;
        if let Some(vo) = v.voice_out.as_mut() {
            vo[..bs].fill(0.0);
        }
    }

    fn kill_note(&mut self) {
        self.enabled = false;
    }

    pub fn releasekey(&mut self, pars: &AdParams, cx: &NoteCtx) {
        for v in self.voices.iter_mut() {
            if !v.enabled {
                continue;
            }
            if let Some(e) = v.amp_env.as_mut() {
                e.release_key();
            }
            if let Some(e) = v.freq_env.as_mut() {
                e.release_key();
            }
            if let Some(f) = v.filter.as_mut() {
                if let Some(e) = f.env.as_mut() {
                    e.release_key();
                }
            }
            if let Some(e) = v.fm_freq_env.as_mut() {
                e.release_key();
            }
            if let Some(e) = v.fm_amp_env.as_mut() {
                e.release_key();
            }
        }
        let g = &pars.global;
        self.g_freq_env.release_key();
        if let Some(e) = self.g_filter.env.as_mut() {
            e.release_key();
        }
        self.g_amp_env.release_key();
        self.g_freq_lfo.release_key(&g.freq_lfo, cx.time);
        if let Some(l) = self.g_filter.lfo.as_mut() {
            l.release_key(&g.filter_lfo, cx.time);
        }
        self.g_amp_lfo.release_key(&g.amp_lfo, cx.time);
    }

    pub fn finished(&self) -> bool {
        !self.enabled
    }

    /// `entomb`: force the global envelope to finish (note stealing).
    pub fn entomb(&mut self) {
        self.g_amp_env.force_finish();
    }

    pub fn max_unison(&self) -> usize {
        self.max_unison
    }
}

fn fadein_with(smps: &mut [f32], adj: f32) {
    let n = smps.len();
    let mut zc = 0;
    for i in 1..n {
        if smps[i - 1] < 0.0 && smps[i] > 0.0 {
            zc += 1;
        }
    }
    let mut tmp = (n as f32 - 1.0) / (zc + 1) as f32 / 3.0;
    if tmp < 8.0 {
        tmp = 8.0;
    }
    tmp *= adj;
    let fl = (f2i(tmp).max(0) as usize).min(n);
    for (i, s) in smps.iter_mut().enumerate().take(fl) {
        *s *= 0.5 - (i as f32 / fl as f32 * PI).cos() * 0.5;
    }
}

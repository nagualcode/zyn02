//! PADsynth: `Params/PADnoteParameters.cpp` + `Synth/PADnote.cpp`.
//!
//! Each harmonic of the oscillator is spread over a band (the "profile"),
//! the whole spectrum gets random phases and a single huge inverse FFT
//! makes a seamless looping wavetable. Tables are built off the audio
//! thread whenever the parameters change.

use std::sync::Arc;

use super::envelope::{Envelope, EnvelopeParams, Loc};
use super::fft::{C32, Fft};
use super::filter::FilterParams;
use super::lfo::{Lfo, LfoParams};
use super::modfilter::ModFilter;
use super::oscilgen::{OscScratch, OscilGen};
use super::resonance::Resonance;
use super::synthnote::{Legato, LegatoParams, porta_read};
use super::util::*;
use super::xml::{Version, XmlNode};
use super::NoteCtx;
use super::adnote::NoteInit;

#[derive(Clone, Debug)]
pub struct PadSample {
    pub smp: Arc<Vec<f32>>,
    pub size: usize,
    pub basefreq: f32,
}

#[derive(Clone, Debug)]
pub struct PadParams {
    pub mode: u8,
    pub hp_base_type: u8,
    pub hp_base_par1: u8,
    pub hp_freqmult: u8,
    pub hp_mod_par1: u8,
    pub hp_mod_freq: u8,
    pub hp_width: u8,
    pub hp_amp_type: u8,
    pub hp_amp_mode: u8,
    pub hp_amp_par1: u8,
    pub hp_amp_par2: u8,
    pub hp_autoscale: bool,
    pub hp_onehalf: u8,
    pub bandwidth: i32,
    pub bwscale: u8,
    pub hrpos_type: u8,
    pub hrpos_par1: u8,
    pub hrpos_par2: u8,
    pub hrpos_par3: u8,
    pub q_samplesize: u8,
    pub q_basenote: u8,
    pub q_oct: u8,
    pub q_smpoct: u8,
    pub stereo: bool,
    pub fixed_freq: bool,
    pub fixed_freq_et: u8,
    pub bend_adjust: u8,
    pub offset_hz: u8,
    pub detune: u16,
    pub coarse_detune: u16,
    pub detune_type: u8,
    pub freq_envelope: EnvelopeParams,
    pub freq_lfo: LfoParams,
    pub volume: u8,
    pub panning: u8,
    pub amp_velocity_scale_function: u8,
    pub amp_envelope: EnvelopeParams,
    pub amp_lfo: LfoParams,
    pub fadein_adjustment: u8,
    pub punch_strength: u8,
    pub punch_time: u8,
    pub punch_stretch: u8,
    pub punch_velocity_sensing: u8,
    pub filter_velocity_scale: u8,
    pub filter_velocity_scale_function: u8,
    pub filter: FilterParams,
    pub filter_envelope: EnvelopeParams,
    pub filter_lfo: LfoParams,
    pub oscil: OscilGen,
    pub resonance: Resonance,
    /// Generated wavetables (not saved).
    pub samples: Vec<PadSample>,
}

impl PadParams {
    pub fn new(oscilsize: usize) -> Self {
        Self {
            mode: 0,
            hp_base_type: 0,
            hp_base_par1: 80,
            hp_freqmult: 0,
            hp_mod_par1: 0,
            hp_mod_freq: 30,
            hp_width: 127,
            hp_amp_type: 0,
            hp_amp_mode: 0,
            hp_amp_par1: 80,
            hp_amp_par2: 64,
            hp_autoscale: true,
            hp_onehalf: 0,
            bandwidth: 500,
            bwscale: 0,
            hrpos_type: 0,
            hrpos_par1: 0,
            hrpos_par2: 0,
            hrpos_par3: 0,
            q_samplesize: 3,
            q_basenote: 4,
            q_oct: 3,
            q_smpoct: 2,
            stereo: true,
            fixed_freq: false,
            fixed_freq_et: 0,
            bend_adjust: 88,
            offset_hz: 64,
            detune: 8192,
            coarse_detune: 0,
            detune_type: 1,
            freq_envelope: EnvelopeParams::new(0, false, Loc::AdGlobalFreq),
            freq_lfo: LfoParams::new(Loc::AdGlobalFreq),
            volume: 90,
            panning: 64,
            amp_velocity_scale_function: 64,
            amp_envelope: EnvelopeParams::new(64, true, Loc::AdGlobalAmp),
            amp_lfo: LfoParams::new(Loc::AdGlobalAmp),
            fadein_adjustment: FADEIN_ADJUSTMENT_SCALE as u8,
            punch_strength: 0,
            punch_time: 60,
            punch_stretch: 64,
            punch_velocity_sensing: 72,
            filter_velocity_scale: 0,
            filter_velocity_scale_function: 64,
            filter: FilterParams::new(Loc::AdGlobalFilter),
            filter_envelope: EnvelopeParams::new(0, true, Loc::AdGlobalFilter),
            filter_lfo: LfoParams::new(Loc::AdGlobalFilter),
            oscil: OscilGen::new(oscilsize, true),
            resonance: Resonance::default(),
            samples: Vec::new(),
        }
    }

    /// Bandwidth in cents (`setPbandwidth`).
    pub fn bandwidth_cents(&self) -> f32 {
        let r = (self.bandwidth as f32 / 1000.0).powf(1.1);
        10f32.powf(r * 4.0) * 0.25
    }

    /// Harmonic profile; returns the bandwidth adjustment.
    pub fn getprofile(&self, smp: &mut [f32]) -> f32 {
        let size = smp.len();
        smp.fill(0.0);
        let supersample = 16;
        let basepar = 2f32.powf((1.0 - self.hp_base_par1 as f32 / 127.0) * 12.0);
        let freqmult = (2f32.powf(self.hp_freqmult as f32 / 127.0 * 5.0) + 0.000_001).floor();
        let modfreq = (2f32.powf(self.hp_mod_freq as f32 / 127.0 * 5.0) + 0.000_001).floor();
        let modpar1 = (self.hp_mod_par1 as f32 / 127.0).powi(4) * 5.0 / modfreq.sqrt();
        let amppar1 = 2f32.powf((self.hp_amp_par1 as f32 / 127.0).powi(2) * 10.0) - 0.999;
        let amppar2 = (1.0 - self.hp_amp_par2 as f32 / 127.0) * 0.998 + 0.001;
        let width = (150.0 / (self.hp_width as f32 + 22.0)).powi(2);
        for i in 0..size * supersample {
            let mut makezero = false;
            let mut x = i as f32 / (size * supersample) as f32;
            let mut origx = x;
            x = (x - 0.5) * width + 0.5;
            if x < 0.0 {
                x = 0.0;
                makezero = true;
            } else if x > 1.0 {
                x = 1.0;
                makezero = true;
            }
            match self.hp_onehalf {
                1 => x = x * 0.5 + 0.5,
                2 => x *= 0.5,
                _ => {}
            }
            let xb = x;
            x *= freqmult;
            x += (xb * 3.141_592_6 * modfreq).sin() * modpar1;
            x = (x + 1000.0) % 1.0 * 2.0 - 1.0;
            let mut f = match self.hp_base_type {
                1 => {
                    if (-(x * x) * basepar).exp() < 0.4 {
                        0.0
                    } else {
                        1.0
                    }
                }
                2 => (-(x.abs()) * basepar.sqrt()).exp(),
                _ => (-(x * x) * basepar).exp(),
            };
            if makezero {
                f = 0.0;
            }
            origx = origx * 2.0 - 1.0;
            let amp = match self.hp_amp_type {
                1 => (-(origx * origx) * 10.0 * amppar1).exp(),
                2 => 0.5 * (1.0 + (3.141_592_6 * origx * (amppar1 * 4.0 + 1.0).sqrt()).cos()),
                3 => 1.0 / ((origx * (amppar1 * 2.0 + 0.8)).powf(14.0) + 1.0),
                _ => 1.0,
            };
            let mut fs = f;
            if self.hp_amp_type != 0 {
                match self.hp_amp_mode {
                    0 => fs = amp * (1.0 - amppar2) + fs * amppar2,
                    1 => fs *= amp * (1.0 - amppar2) + amppar2,
                    2 => fs /= amp + amppar2.powi(4) * 20.0 + 0.0001,
                    3 => fs = amp / (fs + amppar2.powi(4) * 20.0 + 0.0001),
                    _ => {}
                }
            }
            smp[i / supersample] += fs / supersample as f32;
        }
        let mut mx = 0.0f32;
        for s in smp.iter_mut() {
            if *s < 0.0 {
                *s = 0.0;
            }
            mx = mx.max(*s);
        }
        if mx < 0.00001 {
            mx = 1.0;
        }
        for s in smp.iter_mut() {
            *s /= mx;
        }
        if !self.hp_autoscale {
            return 0.5;
        }
        let mut sum = 0.0;
        let mut i = 0;
        while i < size / 2 - 2 {
            sum += smp[i] * smp[i] + smp[size - i - 1] * smp[size - i - 1];
            if sum >= 4.0 {
                break;
            }
            i += 1;
        }
        1.0 - 2.0 * i as f32 / size as f32
    }

    pub fn get_nhr(&self, n: i32) -> f32 {
        let par1 = 10f32.powf(-(1.0 - self.hrpos_par1 as f32 / 255.0) * 3.0);
        let par2 = self.hrpos_par2 as f32 / 255.0;
        let nf = n as f32;
        let n0 = nf - 1.0;
        let result = match self.hrpos_type {
            1 => {
                let th = (par2 * par2 * 100.0) as i32 + 1;
                if n < th { nf } else { 1.0 + n0 + (n0 - th as f32 + 1.0) * par1 * 8.0 }
            }
            2 => {
                let th = (par2 * par2 * 100.0) as i32 + 1;
                if n < th { nf } else { 1.0 + n0 - (n0 - th as f32 + 1.0) * par1 * 0.90 }
            }
            3 => {
                let t = par1 * 100.0 + 1.0;
                (n0 / t).powf(1.0 - par2 * 0.8) * t + 1.0
            }
            4 => n0 * (1.0 - par1) + (n0 * 0.1).powf(par2 * 3.0 + 1.0) * par1 * 10.0 + 1.0,
            5 => n0 + (n0 * par2 * par2 * PI * 0.999).sin() * par1.sqrt() * 2.0 + 1.0,
            6 => {
                let t = (par2 * 2.0).powi(2) + 0.1;
                n0 * (1.0 + par1 * (n0 * 0.8).powf(t)).powf(t) + 1.0
            }
            7 => (nf + self.hrpos_par1 as f32 / 255.0) / (self.hrpos_par1 as f32 / 255.0 + 1.0),
            _ => nf,
        };
        let par3 = self.hrpos_par3 as f32 / 255.0;
        let ir = (result + 0.5).floor();
        ir + (1.0 - par3) * (result - ir)
    }

    fn bwscale_power(&self) -> f32 {
        match self.bwscale {
            0 => 1.0,
            1 => 0.0,
            2 => 0.25,
            3 => 0.5,
            4 => 0.75,
            5 => 1.5,
            6 => 2.0,
            7 => -0.5,
            _ => 1.0,
        }
    }

    fn harmonics(&self, basefreq: f32, sr: f32, sc: &mut OscScratch, fft: &mut Fft) -> Vec<f32> {
        let os = self.oscil.oscilsize();
        let mut h = vec![0.0f32; os];
        self.oscil.get(&mut h, basefreq, None, 1, sc, fft, sr);
        // `normalize_max` (sic: compares with the index, as Zyn does)
        let mut mx = 0.0f32;
        for (i, v) in h[..os / 2].iter().enumerate() {
            if *v > i as f32 {
                mx = *v;
            }
        }
        if mx > 0.000_001 {
            for v in h[..os / 2].iter_mut() {
                *v /= mx;
            }
        }
        h
    }

    #[allow(clippy::too_many_arguments)]
    fn spectrum_bandwidth(&self, spectrum: &mut [f32], basefreq: f32, profile: &[f32], bwadjust: f32, sr: f32, sc: &mut OscScratch, fft: &mut Fft) {
        let size = spectrum.len();
        spectrum.fill(0.0);
        let os = self.oscil.oscilsize();
        let harmonics = self.harmonics(basefreq, sr, sc, fft);
        let power = self.bwscale_power();
        let bwcents = self.bandwidth_cents();
        let ps = profile.len();
        for nh in 1..os / 2 {
            let realfreq = self.get_nhr(nh as i32) * basefreq;
            if realfreq > sr * 0.49999 || realfreq < 20.0 {
                break;
            }
            if harmonics[nh - 1] < 1e-4 {
                continue;
            }
            let bw = ((2f32.powf(bwcents / 1200.0) - 1.0) * basefreq / bwadjust) * (realfreq / basefreq).powf(power);
            let ibw = (bw / (sr * 0.5) * size as f32) as i64 + 1;
            let mut amp = harmonics[nh - 1];
            if self.resonance.enabled {
                amp *= self.resonance.getfreqresponse(realfreq, 1.0, 1.0);
            }
            if ibw > ps as i64 {
                let rap = (ps as f32 / ibw as f32).sqrt();
                let cfreq = (realfreq / (sr * 0.5) * size as f32) as i64 - ibw / 2;
                for i in 0..ibw {
                    let src = (i as f32 * rap * rap) as usize;
                    let sp = i + cfreq;
                    if sp < 0 {
                        continue;
                    }
                    if sp >= size as i64 {
                        break;
                    }
                    spectrum[sp as usize] += amp * profile[src.min(ps - 1)] * rap;
                }
            } else {
                let rap = (ibw as f32 / ps as f32).sqrt();
                let ibase = realfreq / (sr * 0.5) * size as f32;
                for (i, pv) in profile.iter().enumerate() {
                    let idf = (i as f32 / ps as f32 - 0.5) * ibw as f32;
                    let fsum = idf + ibase;
                    let sp = fsum as i64;
                    let fsp = fsum - sp as f32;
                    if sp <= 0 {
                        continue;
                    }
                    if sp >= size as i64 - 1 {
                        break;
                    }
                    let sp = sp as usize;
                    spectrum[sp] += amp * pv * rap * (1.0 - fsp);
                    spectrum[sp + 1] += amp * pv * rap * fsp;
                }
            }
        }
    }

    fn spectrum_other(&self, spectrum: &mut [f32], basefreq: f32, sr: f32, sc: &mut OscScratch, fft: &mut Fft) {
        let size = spectrum.len();
        spectrum.fill(0.0);
        let os = self.oscil.oscilsize();
        let harmonics = self.harmonics(basefreq, sr, sc, fft);
        for nh in 1..os / 2 {
            let realfreq = self.get_nhr(nh as i32) * basefreq;
            if realfreq > sr * 0.49999 || realfreq < 20.0 {
                break;
            }
            let mut amp = harmonics[nh - 1];
            if self.resonance.enabled {
                amp *= self.resonance.getfreqresponse(realfreq, 1.0, 1.0);
            }
            let cfreq = (realfreq / (sr * 0.5) * size as f32) as usize;
            if cfreq < size {
                spectrum[cfreq] = amp + 1e-9;
            }
        }
        if self.mode == 2 {
            let mut old = 0usize;
            for k in 1..size {
                if spectrum[k] > 1e-10 || k == size - 1 {
                    let delta = k - old;
                    let (v1, v2) = (spectrum[old], spectrum[k]);
                    let idelta = 1.0 / delta as f32;
                    for i in 0..delta {
                        let x = idelta * i as f32;
                        spectrum[old + i] = v1 * (1.0 - x) + v2 * x;
                    }
                    old = k;
                }
            }
        }
    }

    /// `applyparameters`: (re)build the wavetables. Heavy — never call on
    /// the audio thread.
    pub fn apply_parameters(&mut self, sr: f32) {
        let samplesize = 1usize << (self.q_samplesize as usize + 14);
        let spectrumsize = samplesize / 2;
        let mut profile = vec![0.0f32; 512];
        let bwadjust = self.getprofile(&mut profile);
        let mut basefreq = 65.406 * 2f32.powf((self.q_basenote / 2) as f32);
        if self.q_basenote % 2 == 1 {
            basefreq *= 1.5;
        }
        let mut samplemax = self.q_oct as i32 + 1;
        let smpoct = match self.q_smpoct {
            5 => 6,
            6 => 12,
            v => v as i32,
        };
        if smpoct != 0 {
            samplemax *= smpoct;
        } else {
            samplemax = samplemax / 2 + 1;
        }
        let samplemax = (samplemax.max(1) as usize).min(PAD_MAX_SAMPLES);
        let adj: Vec<f32> = (0..samplemax).map(|n| (self.q_oct as f32 + 1.0) * n as f32 / samplemax as f32).collect();

        let os = self.oscil.oscilsize();
        let mut offt = Fft::new(os);
        self.oscil.prepare(&mut offt);

        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(samplemax).max(1);
        let this = &*self;
        let results: Vec<(usize, PadSample)> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let adj = &adj;
                    let profile = &profile;
                    s.spawn(move || {
                        let mut out = Vec::new();
                        let mut big = Fft::new(samplesize);
                        let mut offt = Fft::new(os);
                        let mut sc = OscScratch::new(os);
                        let mut spectrum = vec![0.0f32; spectrumsize];
                        let mut freqs = vec![C32::new(0.0, 0.0); spectrumsize + 1];
                        sprng(0x1234_5678u32.wrapping_add(t as u32 * 7919));
                        for n in (t..samplemax).step_by(threads) {
                            let bfa = 2f32.powf(adj[n] - adj[samplemax - 1] * 0.5);
                            if this.mode == 0 {
                                this.spectrum_bandwidth(&mut spectrum, basefreq * bfa, profile, bwadjust, sr, &mut sc, &mut offt);
                            } else {
                                this.spectrum_other(&mut spectrum, basefreq * bfa, sr, &mut sc, &mut offt);
                            }
                            freqs[0] = C32::new(0.0, 0.0);
                            for i in 1..spectrumsize {
                                let ph = rnd() * 2.0 * PI;
                                freqs[i] = C32::new(spectrum[i] * ph.cos(), spectrum[i] * ph.sin());
                            }
                            let mut smp = vec![0.0f32; samplesize + 5];
                            big.freqs2smps(&freqs, &mut smp[..samplesize]);
                            let mut rms: f32 = smp[..samplesize].iter().map(|v| v * v).sum::<f32>().sqrt();
                            if rms < 0.000_001 {
                                rms = 1.0;
                            }
                            rms *= (262_144.0 / samplesize as f32).sqrt();
                            for v in smp[..samplesize].iter_mut() {
                                *v *= 1.0 / rms * 50.0;
                            }
                            for i in 0..5 {
                                smp[i + samplesize] = smp[i];
                            }
                            out.push((n, PadSample { smp: Arc::new(smp), size: samplesize, basefreq: basefreq * bfa }));
                        }
                        out
                    })
                })
                .collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        });
        let mut samples: Vec<Option<PadSample>> = vec![None; samplemax];
        for (n, s) in results {
            samples[n] = Some(s);
        }
        self.samples = samples.into_iter().flatten().collect();
    }

    pub fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_parbool("stereo", self.stereo);
        x.add_par("mode", self.mode as i32);
        x.add_par("bandwidth", self.bandwidth);
        x.add_par("bandwidth_scale", self.bwscale as i32);
        let h = x.child("HARMONIC_PROFILE");
        h.add_par("base_type", self.hp_base_type as i32);
        h.add_par("base_par1", self.hp_base_par1 as i32);
        h.add_par("frequency_multiplier", self.hp_freqmult as i32);
        h.add_par("modulator_par1", self.hp_mod_par1 as i32);
        h.add_par("modulator_frequency", self.hp_mod_freq as i32);
        h.add_par("width", self.hp_width as i32);
        h.add_par("amplitude_multiplier_type", self.hp_amp_type as i32);
        h.add_par("amplitude_multiplier_mode", self.hp_amp_mode as i32);
        h.add_par("amplitude_multiplier_par1", self.hp_amp_par1 as i32);
        h.add_par("amplitude_multiplier_par2", self.hp_amp_par2 as i32);
        h.add_parbool("autoscale", self.hp_autoscale);
        h.add_par("one_half", self.hp_onehalf as i32);
        self.oscil.p.add_to_xml(x.child("OSCIL"));
        self.resonance.add_to_xml(x.child("RESONANCE"));
        let p = x.child("HARMONIC_POSITION");
        p.add_par("type", self.hrpos_type as i32);
        p.add_par("parameter1", self.hrpos_par1 as i32);
        p.add_par("parameter2", self.hrpos_par2 as i32);
        p.add_par("parameter3", self.hrpos_par3 as i32);
        let q = x.child("SAMPLE_QUALITY");
        q.add_par("samplesize", self.q_samplesize as i32);
        q.add_par("basenote", self.q_basenote as i32);
        q.add_par("octaves", self.q_oct as i32);
        q.add_par("samples_per_octave", self.q_smpoct as i32);
        let a = x.child("AMPLITUDE_PARAMETERS");
        a.add_par("volume", self.volume as i32);
        a.add_par("panning", self.panning as i32);
        a.add_par("velocity_sensing", self.amp_velocity_scale_function as i32);
        a.add_par("fadein_adjustment", self.fadein_adjustment as i32);
        a.add_par("punch_strength", self.punch_strength as i32);
        a.add_par("punch_time", self.punch_time as i32);
        a.add_par("punch_stretch", self.punch_stretch as i32);
        a.add_par("punch_velocity_sensing", self.punch_velocity_sensing as i32);
        self.amp_envelope.add_to_xml(a.child("AMPLITUDE_ENVELOPE"));
        self.amp_lfo.add_to_xml(a.child("AMPLITUDE_LFO"));
        let f = x.child("FREQUENCY_PARAMETERS");
        f.add_par("fixed_freq", self.fixed_freq as i32);
        f.add_par("fixed_freq_et", self.fixed_freq_et as i32);
        f.add_par("bend_adjust", self.bend_adjust as i32);
        f.add_par("offset_hz", self.offset_hz as i32);
        f.add_par("detune", self.detune as i32);
        f.add_par("coarse_detune", self.coarse_detune as i32);
        f.add_par("detune_type", self.detune_type as i32);
        self.freq_envelope.add_to_xml(f.child("FREQUENCY_ENVELOPE"));
        self.freq_lfo.add_to_xml(f.child("FREQUENCY_LFO"));
        let fl = x.child("FILTER_PARAMETERS");
        fl.add_par("velocity_sensing_amplitude", self.filter_velocity_scale as i32);
        fl.add_par("velocity_sensing", self.filter_velocity_scale_function as i32);
        self.filter.add_to_xml(fl.child("FILTER"));
        self.filter_envelope.add_to_xml(fl.child("FILTER_ENVELOPE"));
        self.filter_lfo.add_to_xml(fl.child("FILTER_LFO"));
    }

    pub fn get_from_xml(&mut self, x: &XmlNode, ver: Version, oscilsize: usize) {
        self.stereo = x.parbool("stereo", self.stereo);
        self.mode = x.par127("mode", 0).min(2);
        self.bandwidth = x.par("bandwidth", self.bandwidth, 0, 1000);
        self.bwscale = x.par127("bandwidth_scale", self.bwscale);
        if let Some(h) = x.branch("HARMONIC_PROFILE") {
            self.hp_base_type = h.par127("base_type", self.hp_base_type);
            self.hp_base_par1 = h.par127("base_par1", self.hp_base_par1);
            self.hp_freqmult = h.par127("frequency_multiplier", self.hp_freqmult);
            self.hp_mod_par1 = h.par127("modulator_par1", self.hp_mod_par1);
            self.hp_mod_freq = h.par127("modulator_frequency", self.hp_mod_freq);
            self.hp_width = h.par127("width", self.hp_width);
            self.hp_amp_type = h.par127("amplitude_multiplier_type", self.hp_amp_type);
            self.hp_amp_mode = h.par127("amplitude_multiplier_mode", self.hp_amp_mode);
            self.hp_amp_par1 = h.par127("amplitude_multiplier_par1", self.hp_amp_par1);
            self.hp_amp_par2 = h.par127("amplitude_multiplier_par2", self.hp_amp_par2);
            self.hp_autoscale = h.parbool("autoscale", self.hp_autoscale);
            self.hp_onehalf = h.par127("one_half", self.hp_onehalf);
        }
        if let Some(o) = x.branch("OSCIL") {
            self.oscil.p.get_from_xml(o, oscilsize);
        }
        if let Some(r) = x.branch("RESONANCE") {
            self.resonance.get_from_xml(r);
        }
        if let Some(p) = x.branch("HARMONIC_POSITION") {
            self.hrpos_type = p.par127("type", self.hrpos_type);
            self.hrpos_par1 = p.par("parameter1", self.hrpos_par1 as i32, 0, 255) as u8;
            self.hrpos_par2 = p.par("parameter2", self.hrpos_par2 as i32, 0, 255) as u8;
            self.hrpos_par3 = p.par("parameter3", self.hrpos_par3 as i32, 0, 255) as u8;
        }
        if let Some(q) = x.branch("SAMPLE_QUALITY") {
            self.q_samplesize = q.par127("samplesize", self.q_samplesize).min(6);
            self.q_basenote = q.par127("basenote", self.q_basenote);
            self.q_oct = q.par127("octaves", self.q_oct);
            self.q_smpoct = q.par127("samples_per_octave", self.q_smpoct);
        }
        if let Some(a) = x.branch("AMPLITUDE_PARAMETERS") {
            self.volume = a.par127("volume", self.volume);
            self.panning = a.par127("panning", self.panning);
            self.amp_velocity_scale_function = a.par127("velocity_sensing", self.amp_velocity_scale_function);
            self.fadein_adjustment = a.par127("fadein_adjustment", self.fadein_adjustment);
            self.punch_strength = a.par127("punch_strength", self.punch_strength);
            self.punch_time = a.par127("punch_time", self.punch_time);
            self.punch_stretch = a.par127("punch_stretch", self.punch_stretch);
            self.punch_velocity_sensing = a.par127("punch_velocity_sensing", self.punch_velocity_sensing);
            if let Some(e) = a.branch("AMPLITUDE_ENVELOPE") {
                self.amp_envelope.get_from_xml(e, ver);
            }
            if let Some(l) = a.branch("AMPLITUDE_LFO") {
                self.amp_lfo.get_from_xml(l, ver);
            }
        }
        if let Some(f) = x.branch("FREQUENCY_PARAMETERS") {
            self.fixed_freq = f.par("fixed_freq", self.fixed_freq as i32, 0, 1) != 0;
            self.fixed_freq_et = f.par127("fixed_freq_et", self.fixed_freq_et);
            self.bend_adjust = f.par127("bend_adjust", self.bend_adjust);
            self.offset_hz = f.par127("offset_hz", self.offset_hz);
            self.detune = f.par("detune", self.detune as i32, 0, 16383) as u16;
            self.coarse_detune = f.par("coarse_detune", self.coarse_detune as i32, 0, 16383) as u16;
            self.detune_type = f.par127("detune_type", self.detune_type);
            if let Some(e) = f.branch("FREQUENCY_ENVELOPE") {
                self.freq_envelope.get_from_xml(e, ver);
            }
            if let Some(l) = f.branch("FREQUENCY_LFO") {
                self.freq_lfo.get_from_xml(l, ver);
            }
        }
        if let Some(f) = x.branch("FILTER_PARAMETERS") {
            self.filter_velocity_scale = f.par127("velocity_sensing_amplitude", self.filter_velocity_scale);
            self.filter_velocity_scale_function = f.par127("velocity_sensing", self.filter_velocity_scale_function);
            if let Some(n) = f.branch("FILTER") {
                self.filter.get_from_xml(n, ver);
            }
            if let Some(n) = f.branch("FILTER_ENVELOPE") {
                self.filter_envelope.get_from_xml(n, ver);
            }
            if let Some(n) = f.branch("FILTER_LFO") {
                self.filter_lfo.get_from_xml(n, ver);
            }
        }
    }
}

// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PadNote {
    portamento: Option<super::synthnote::PortaRef>,
    velocity: f32,
    finished: bool,
    note_log2_freq: f32,
    bend_adjust: f32,
    offset_hz: f32,
    firsttime: bool,
    realfreq: f32,
    detune: f32,
    nsample: usize,
    poshi_l: usize,
    poshi_r: usize,
    poslo: f32,
    panning: f32,
    fadein_adjustment: f32,
    punch_enabled: bool,
    punch_initialvalue: f32,
    punch_dt: f32,
    punch_t: f32,
    volume: f32,
    freq_env: Option<Envelope>,
    freq_lfo: Option<Lfo>,
    amp_env: Option<Envelope>,
    amp_lfo: Option<Lfo>,
    filter: Option<ModFilter>,
    globaloldamplitude: f32,
    globalnewamplitude: f32,
    /// Wavetable kept alive while the note plays.
    table: Option<PadSample>,
    pub legato: Legato,
}

impl PadNote {
    pub fn new(pars: &PadParams, ini: &NoteInit, cx: &mut NoteCtx) -> Self {
        let mut n = Self {
            portamento: ini.portamento,
            velocity: ini.velocity,
            finished: false,
            note_log2_freq: ini.note_log2_freq,
            bend_adjust: 0.0,
            offset_hz: 0.0,
            firsttime: true,
            realfreq: 440.0,
            detune: 0.0,
            nsample: 0,
            poshi_l: 0,
            poshi_r: 0,
            poslo: 0.0,
            panning: 0.5,
            fadein_adjustment: 1.0,
            punch_enabled: false,
            punch_initialvalue: 0.0,
            punch_dt: 0.0,
            punch_t: 0.0,
            volume: 0.0,
            freq_env: None,
            freq_lfo: None,
            amp_env: None,
            amp_lfo: None,
            filter: None,
            globaloldamplitude: 0.0,
            globalnewamplitude: 0.0,
            table: None,
            legato: Legato::new(cx.synth, ini.velocity, ini.portamento, ini.note_log2_freq, ini.quiet, ini.seed),
        };
        n.setup(pars, ini.velocity, ini.portamento, ini.note_log2_freq, false, cx);
        n
    }

    fn setup(&mut self, pars: &PadParams, vel: f32, porta: Option<super::synthnote::PortaRef>, note_log2_freq: f32, legato: bool, cx: &mut NoteCtx) {
        let synth = *cx.synth;
        self.portamento = porta;
        self.velocity = vel;
        self.finished = false;
        if !pars.fixed_freq {
            self.note_log2_freq = note_log2_freq;
        } else {
            let et = pars.fixed_freq_et as i32;
            let mut fl = 440f32.log2();
            if et != 0 {
                let tmp = (note_log2_freq - fl) * (2f32.powf((et - 1) as f32 / 63.0) - 1.0);
                if et <= 64 {
                    fl += tmp;
                } else {
                    fl += tmp * 3f32.log2();
                }
            }
            self.note_log2_freq = fl;
        }
        let basefreq = 2f32.powf(self.note_log2_freq);
        self.bend_adjust = (pars.bend_adjust as i32 - 64) as f32 / 24.0;
        let ov = (pars.offset_hz as f32 - 64.0) / 64.0;
        self.offset_hz = 15.0 * (ov * ov.abs().sqrt());
        if !legato {
            self.firsttime = true;
        }
        self.realfreq = basefreq;
        if !legato {
            self.detune = getdetune(pars.detune_type, pars.coarse_detune, pars.detune);
        }
        let log2freq = self.note_log2_freq + self.detune / 1200.0;
        self.nsample = 0;
        if let Some(s0) = pars.samples.first() {
            let mut mindist = (log2freq - (s0.basefreq + 0.0001).log2()).abs();
            for (i, s) in pars.samples.iter().enumerate().skip(1) {
                let d = (log2freq - (s.basefreq + 0.0001).log2()).abs();
                if d < mindist {
                    self.nsample = i;
                    mindist = d;
                }
            }
        }
        self.table = pars.samples.get(self.nsample).cloned();
        let size = self.table.as_ref().map(|t| t.size).unwrap_or(1).max(1);
        if !legato {
            self.poshi_l = (rnd() * (size - 1) as f32) as usize;
            self.poshi_r = if pars.stereo { (self.poshi_l + size / 2) % size } else { self.poshi_l };
            self.poslo = 0.0;
        }
        if pars.panning != 0 {
            self.panning = pars.panning as f32 / 128.0;
        } else if !legato {
            self.panning = rnd();
        }
        let dt = synth.dt();
        if !legato {
            let mut fa = pars.fadein_adjustment as f32 / FADEIN_ADJUSTMENT_SCALE as f32;
            fa *= fa;
            self.fadein_adjustment = fa;
            if pars.punch_strength != 0 {
                self.punch_enabled = true;
                self.punch_t = 1.0;
                self.punch_initialvalue = (10f32.powf(1.5 * pars.punch_strength as f32 / 127.0) - 1.0) * vel_f(vel, pars.punch_velocity_sensing);
                let time = 10f32.powf(3.0 * pars.punch_time as f32 / 127.0) / 10000.0;
                let freq = 2f32.powf(note_log2_freq);
                let stretch = (440.0 / freq).powf(pars.punch_stretch as f32 / 64.0);
                self.punch_dt = 1.0 / (time * synth.samplerate_f * stretch);
            } else {
                self.punch_enabled = false;
            }
            self.freq_env = Some(Envelope::new(&pars.freq_envelope, basefreq, dt));
            self.freq_lfo = Some(Lfo::new(&pars.freq_lfo, basefreq, cx.time));
            self.amp_env = Some(Envelope::new(&pars.amp_envelope, basefreq, dt));
            self.amp_lfo = Some(Lfo::new(&pars.amp_lfo, basefreq, cx.time));
        }
        self.volume = 4.0 * 0.1f32.powf(3.0 * (1.0 - pars.volume as f32 / 96.0)) * vel_f(vel, pars.amp_velocity_scale_function);
        if !legato {
            let e = self.amp_env.as_mut().unwrap();
            e.envout_db();
            let a = self.volume * e.envout_db() * self.amp_lfo.as_mut().unwrap().amplfoout(&pars.amp_lfo, cx.time);
            self.globaloldamplitude = a;
            self.globalnewamplitude = a;
            self.filter = Some(
                ModFilter::new(&pars.filter, synth.samplerate_f, synth.buffersize, true, basefreq)
                    .with_env(&pars.filter_envelope, basefreq, dt)
                    .with_lfo(&pars.filter_lfo, basefreq, cx.time),
            );
        }
        if let Some(f) = self.filter.as_mut() {
            f.update_sense(vel, pars.filter_velocity_scale, pars.filter_velocity_scale_function);
            f.update_note_freq(basefreq, &pars.filter);
        }
        if self.table.is_none() {
            self.finished = true;
        }
    }

    fn computecurrentparameters(&mut self, pars: &PadParams, cx: &mut NoteCtx) {
        let ctl = cx.ctl;
        let fe = self.freq_env.as_mut().map(|e| e.envout()).unwrap_or(0.0);
        let fl = self.freq_lfo.as_mut().map(|l| l.lfoout(&pars.freq_lfo, cx.time)).unwrap_or(0.0);
        let globalpitch = 0.01 * (fe + fl * ctl.modwheel_relmod + self.detune);
        self.globaloldamplitude = self.globalnewamplitude;
        let ae = self.amp_env.as_mut().map(|e| e.envout_db()).unwrap_or(0.0);
        let al = self.amp_lfo.as_mut().map(|l| l.amplfoout(&pars.amp_lfo, cx.time)).unwrap_or(1.0);
        self.globalnewamplitude = self.volume * ae * al;
        if let Some(f) = self.filter.as_mut() {
            f.update(ctl.filtercutoff_relfreq, ctl.filterq_relq, &pars.filter, Some(&pars.filter_lfo), cx.time, cx.changed);
        }
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
        self.realfreq = 2f32.powf(self.note_log2_freq + globalpitch / 12.0 + porta) * ctl.pitch_relfreq.powf(self.bend_adjust) + self.offset_hz;
    }

    pub fn noteout(&mut self, pars: &PadParams, outl: &mut [f32], outr: &mut [f32], cx: &mut NoteCtx) {
        let synth = *cx.synth;
        let bs = synth.buffersize;
        self.computecurrentparameters(pars, cx);
        let Some(table) = self.table.as_ref() else {
            outl[..bs].fill(0.0);
            outr[..bs].fill(0.0);
            return;
        };
        let smps = &table.smp;
        let size = table.size;
        let freqrap = self.realfreq / table.basefreq;
        let freqhi = freqrap.floor().max(0.0) as usize;
        let freqlo = freqrap - freqrap.floor();
        let cubic = cx.cubic_interpolation;
        for i in 0..bs {
            self.poshi_l += freqhi;
            self.poshi_r += freqhi;
            self.poslo += freqlo;
            if self.poslo >= 1.0 {
                self.poshi_l += 1;
                self.poshi_r += 1;
                self.poslo -= 1.0;
            }
            if self.poshi_l >= size {
                self.poshi_l %= size;
            }
            if self.poshi_r >= size {
                self.poshi_r %= size;
            }
            let p = self.poslo;
            if cubic {
                let c = |h: usize| {
                    let (xm1, x0, x1, x2) = (smps[h], smps[h + 1], smps[h + 2], smps[h + 3]);
                    let a = (3.0 * (x0 - x1) - xm1 + x2) * 0.5;
                    let b = 2.0 * x1 + xm1 - (5.0 * x0 + x2) * 0.5;
                    let cc = (x1 - xm1) * 0.5;
                    ((a * p + b) * p + cc) * p + x0
                };
                outl[i] = c(self.poshi_l);
                outr[i] = c(self.poshi_r);
            } else {
                outl[i] = smps[self.poshi_l] * (1.0 - p) + smps[self.poshi_l + 1] * p;
                outr[i] = smps[self.poshi_r] * (1.0 - p) + smps[self.poshi_r + 1] * p;
            }
        }
        if self.firsttime {
            fadein(&mut outl[..bs], self.fadein_adjustment);
            fadein(&mut outr[..bs], self.fadein_adjustment);
            self.firsttime = false;
        }
        if let Some(f) = self.filter.as_mut() {
            f.filter(&mut outl[..bs], Some(&mut outr[..bs]));
        }
        if self.punch_enabled {
            for i in 0..bs {
                let pa = self.punch_initialvalue * self.punch_t + 1.0;
                outl[i] *= pa;
                outr[i] *= pa;
                self.punch_t -= self.punch_dt;
                if self.punch_t < 0.0 {
                    self.punch_enabled = false;
                    break;
                }
            }
        }
        let (pl, pr) = if cx.const_power { ((1.0 - self.panning).sqrt(), self.panning.sqrt()) } else { (1.0 - self.panning, self.panning) };
        let (oa, na) = (self.globaloldamplitude, self.globalnewamplitude);
        if above_amplitude_threshold(oa, na) {
            for i in 0..bs {
                let t = interpolate_amplitude(oa, na, i, bs);
                outl[i] *= t * pl;
                outr[i] *= t * pr;
            }
        } else {
            for i in 0..bs {
                outl[i] *= na * pl;
                outr[i] *= na * pr;
            }
        }
        if let Some(lp) = self.legato.apply(&mut outl[..bs], &mut outr[..bs]) {
            self.legatonote(pars, &lp, cx);
        }
        if self.amp_env.as_ref().map(|e| e.finished()).unwrap_or(true) {
            for i in 0..bs {
                let t = 1.0 - i as f32 / synth.buffersize_f;
                outl[i] *= t;
                outr[i] *= t;
            }
            self.finished = true;
        }
    }

    pub fn legatonote(&mut self, pars: &PadParams, lp: &LegatoParams, cx: &mut NoteCtx) {
        if self.legato.update(lp) {
            return;
        }
        self.setup(pars, lp.velocity, lp.portamento, lp.note_log2_freq, true, cx);
    }

    pub fn releasekey(&mut self, pars: &PadParams, cx: &NoteCtx) {
        if let Some(e) = self.freq_env.as_mut() {
            e.release_key();
        }
        if let Some(f) = self.filter.as_mut() {
            if let Some(e) = f.env.as_mut() {
                e.release_key();
            }
            if let Some(l) = f.lfo.as_mut() {
                l.release_key(&pars.filter_lfo, cx.time);
            }
        }
        if let Some(e) = self.amp_env.as_mut() {
            e.release_key();
        }
        if let Some(l) = self.freq_lfo.as_mut() {
            l.release_key(&pars.freq_lfo, cx.time);
        }
        if let Some(l) = self.amp_lfo.as_mut() {
            l.release_key(&pars.amp_lfo, cx.time);
        }
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn entomb(&mut self) {
        if let Some(e) = self.amp_env.as_mut() {
            e.force_finish();
        }
    }
}

fn fadein(smps: &mut [f32], adj: f32) {
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

//! `Params/ADnoteParameters.cpp`.

use super::envelope::{EnvelopeParams, Loc};
use super::filter::FilterParams;
use super::lfo::LfoParams;
use super::oscilgen::OscilGen;
use super::resonance::Resonance;
use super::util::*;
use super::xml::{Version, XmlNode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FmType {
    None = 0,
    Mix = 1,
    RingMod = 2,
    PhaseMod = 3,
    FreqMod = 4,
    PwMod = 5,
}

impl FmType {
    pub fn from_i(v: i32) -> Self {
        match v {
            1 => FmType::Mix,
            2 => FmType::RingMod,
            3 => FmType::PhaseMod,
            4 => FmType::FreqMod,
            5 => FmType::PwMod,
            _ => FmType::None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AdGlobal {
    pub stereo: bool,
    pub detune: u16,
    pub coarse_detune: u16,
    pub detune_type: u8,
    pub freq_envelope: EnvelopeParams,
    pub freq_lfo: LfoParams,
    pub bandwidth: u8,
    pub volume: f32,
    pub panning: u8,
    pub amp_velocity_scale_function: u8,
    pub amp_envelope: EnvelopeParams,
    pub amp_lfo: LfoParams,
    pub fadein_adjustment: u8,
    pub punch_strength: u8,
    pub punch_time: u8,
    pub punch_stretch: u8,
    pub punch_velocity_sensing: u8,
    pub hrandgrouping: bool,
    pub filter_velocity_scale: u8,
    pub filter_velocity_scale_function: u8,
    pub filter: FilterParams,
    pub filter_envelope: EnvelopeParams,
    pub filter_lfo: LfoParams,
    pub reson: Resonance,
}

impl AdGlobal {
    pub fn new() -> Self {
        Self {
            stereo: true,
            detune: 8192,
            coarse_detune: 0,
            detune_type: 1,
            freq_envelope: EnvelopeParams::new(0, false, Loc::AdGlobalFreq),
            freq_lfo: LfoParams::new(Loc::AdGlobalFreq),
            bandwidth: 64,
            volume: 8.29,
            panning: 64,
            amp_velocity_scale_function: 64,
            amp_envelope: EnvelopeParams::new(64, true, Loc::AdGlobalAmp),
            amp_lfo: LfoParams::new(Loc::AdGlobalAmp),
            fadein_adjustment: FADEIN_ADJUSTMENT_SCALE as u8,
            punch_strength: 0,
            punch_time: 60,
            punch_stretch: 64,
            punch_velocity_sensing: 72,
            hrandgrouping: false,
            filter_velocity_scale: 0,
            filter_velocity_scale_function: 64,
            filter: FilterParams::new(Loc::AdGlobalFilter),
            filter_envelope: EnvelopeParams::new(0, true, Loc::AdGlobalFilter),
            filter_lfo: LfoParams::new(Loc::AdGlobalFilter),
            reson: Resonance::default(),
        }
    }

    fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_parbool("stereo", self.stereo);
        let a = x.child("AMPLITUDE_PARAMETERS");
        a.add_parreal("volume", self.volume);
        a.add_par("panning", self.panning as i32);
        a.add_par("velocity_sensing", self.amp_velocity_scale_function as i32);
        a.add_par("fadein_adjustment", self.fadein_adjustment as i32);
        a.add_par("punch_strength", self.punch_strength as i32);
        a.add_par("punch_time", self.punch_time as i32);
        a.add_par("punch_stretch", self.punch_stretch as i32);
        a.add_par("punch_velocity_sensing", self.punch_velocity_sensing as i32);
        a.add_par("harmonic_randomness_grouping", self.hrandgrouping as i32);
        self.amp_envelope.add_to_xml(a.child("AMPLITUDE_ENVELOPE"));
        self.amp_lfo.add_to_xml(a.child("AMPLITUDE_LFO"));
        let f = x.child("FREQUENCY_PARAMETERS");
        f.add_par("detune", self.detune as i32);
        f.add_par("coarse_detune", self.coarse_detune as i32);
        f.add_par("detune_type", self.detune_type as i32);
        f.add_par("bandwidth", self.bandwidth as i32);
        self.freq_envelope.add_to_xml(f.child("FREQUENCY_ENVELOPE"));
        self.freq_lfo.add_to_xml(f.child("FREQUENCY_LFO"));
        let fl = x.child("FILTER_PARAMETERS");
        fl.add_par("velocity_sensing_amplitude", self.filter_velocity_scale as i32);
        fl.add_par("velocity_sensing", self.filter_velocity_scale_function as i32);
        self.filter.add_to_xml(fl.child("FILTER"));
        self.filter_envelope.add_to_xml(fl.child("FILTER_ENVELOPE"));
        self.filter_lfo.add_to_xml(fl.child("FILTER_LFO"));
        self.reson.add_to_xml(x.child("RESONANCE"));
    }

    fn get_from_xml(&mut self, x: &XmlNode, ver: Version) {
        self.stereo = x.parbool("stereo", self.stereo);
        if let Some(a) = x.branch("AMPLITUDE_PARAMETERS") {
            let up305 = ver < Version(3, 0, 5);
            let up303 = ver < Version(3, 0, 3) || !a.has_parreal("volume");
            if up303 {
                let vol = a.par127("volume", 0) as f32;
                self.volume = 12.0412 + 60.0 * (vol / 96.0 - 1.0);
            } else if up305 {
                self.volume = 12.0412 + a.parreal("volume", self.volume);
            } else {
                self.volume = a.parreal("volume", self.volume);
            }
            self.panning = a.par127("panning", self.panning);
            self.amp_velocity_scale_function = a.par127("velocity_sensing", self.amp_velocity_scale_function);
            self.fadein_adjustment = a.par127("fadein_adjustment", self.fadein_adjustment);
            self.punch_strength = a.par127("punch_strength", self.punch_strength);
            self.punch_time = a.par127("punch_time", self.punch_time);
            self.punch_stretch = a.par127("punch_stretch", self.punch_stretch);
            self.punch_velocity_sensing = a.par127("punch_velocity_sensing", self.punch_velocity_sensing);
            self.hrandgrouping = a.par("harmonic_randomness_grouping", self.hrandgrouping as i32, 0, 1) != 0;
            if let Some(e) = a.branch("AMPLITUDE_ENVELOPE") {
                self.amp_envelope.get_from_xml(e, ver);
            }
            if let Some(l) = a.branch("AMPLITUDE_LFO") {
                self.amp_lfo.get_from_xml(l, ver);
            }
        }
        if let Some(f) = x.branch("FREQUENCY_PARAMETERS") {
            self.detune = f.par("detune", self.detune as i32, 0, 16383) as u16;
            self.coarse_detune = f.par("coarse_detune", self.coarse_detune as i32, 0, 16383) as u16;
            self.detune_type = f.par127("detune_type", self.detune_type);
            self.bandwidth = f.par127("bandwidth", self.bandwidth);
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
        if let Some(r) = x.branch("RESONANCE") {
            self.reson.get_from_xml(r);
        }
    }
}

#[derive(Clone, Debug)]
pub struct AdVoice {
    pub enabled: bool,
    pub unison_size: u8,
    pub unison_frequency_spread: u8,
    pub unison_stereo_spread: u8,
    pub unison_vibratto: u8,
    pub unison_vibratto_speed: u8,
    pub unison_invert_phase: u8,
    pub unison_phase_randomness: u8,
    pub vtype: u8,
    pub delay: u8,
    pub resonance: bool,
    pub filter_bypass: bool,
    pub filter_fcctl_bypass: bool,
    pub ext_oscil: i32,
    pub ext_fm_oscil: i32,
    pub oscil_phase: u8,
    pub fm_oscil_phase: u8,
    pub fixed_freq: bool,
    pub fixed_freq_et: u8,
    pub bend_adjust: u8,
    pub offset_hz: u8,
    pub detune: u16,
    pub coarse_detune: u16,
    pub detune_type: u8,
    pub volume: f32,
    pub volume_minus: bool,
    pub aa_enabled: bool,
    pub panning: u8,
    pub amp_velocity_scale_function: u8,
    pub amp_envelope_enabled: bool,
    pub amp_envelope: EnvelopeParams,
    pub amp_lfo_enabled: bool,
    pub amp_lfo: LfoParams,
    pub freq_envelope_enabled: bool,
    pub freq_envelope: EnvelopeParams,
    pub freq_lfo_enabled: bool,
    pub freq_lfo: LfoParams,
    pub filter_enabled: bool,
    pub filter: FilterParams,
    pub filter_envelope_enabled: bool,
    pub filter_envelope: EnvelopeParams,
    pub filter_lfo_enabled: bool,
    pub filter_lfo: LfoParams,
    pub filter_velocity_scale: u8,
    pub filter_velocity_scale_function: u8,
    pub fm_enabled: FmType,
    pub fm_fixed_freq: bool,
    pub fm_voice: i32,
    pub fm_volume: f32,
    pub fm_volume_damp: u8,
    pub fm_detune: u16,
    pub fm_coarse_detune: u16,
    pub fm_detune_type: u8,
    pub fm_freq_envelope_enabled: bool,
    pub fm_freq_envelope: EnvelopeParams,
    pub fm_amp_envelope_enabled: bool,
    pub fm_amp_envelope: EnvelopeParams,
    pub fm_velocity_scale_function: u8,
    pub sync_enabled: bool,
    pub oscil: OscilGen,
    pub fm_oscil: OscilGen,
}

impl AdVoice {
    pub fn new(oscilsize: usize) -> Self {
        Self {
            enabled: false,
            unison_size: 1,
            unison_frequency_spread: 60,
            unison_stereo_spread: 64,
            unison_vibratto: 64,
            unison_vibratto_speed: 64,
            unison_invert_phase: 0,
            unison_phase_randomness: 127,
            vtype: 0,
            delay: 0,
            resonance: true,
            filter_bypass: false,
            filter_fcctl_bypass: false,
            ext_oscil: -1,
            ext_fm_oscil: -1,
            oscil_phase: 64,
            fm_oscil_phase: 64,
            fixed_freq: false,
            fixed_freq_et: 0,
            bend_adjust: 88,
            offset_hz: 64,
            detune: 8192,
            coarse_detune: 0,
            detune_type: 0,
            volume: -60.0 * (1.0 - 100.0 / 127.0),
            volume_minus: false,
            aa_enabled: false,
            panning: 64,
            amp_velocity_scale_function: 127,
            amp_envelope_enabled: false,
            amp_envelope: EnvelopeParams::new(64, true, Loc::AdVoiceAmp),
            amp_lfo_enabled: false,
            amp_lfo: LfoParams::new(Loc::AdVoiceAmp),
            freq_envelope_enabled: false,
            freq_envelope: EnvelopeParams::new(0, false, Loc::AdVoiceFreq),
            freq_lfo_enabled: false,
            freq_lfo: LfoParams::new(Loc::AdVoiceFreq),
            filter_enabled: false,
            filter: FilterParams::new(Loc::AdVoiceFilter),
            filter_envelope_enabled: false,
            filter_envelope: EnvelopeParams::new(0, false, Loc::AdVoiceFilter),
            filter_lfo_enabled: false,
            filter_lfo: LfoParams::new(Loc::AdVoiceFilter),
            filter_velocity_scale: 0,
            filter_velocity_scale_function: 64,
            fm_enabled: FmType::None,
            fm_fixed_freq: false,
            fm_voice: -1,
            fm_volume: 70.0,
            fm_volume_damp: 64,
            fm_detune: 8192,
            fm_coarse_detune: 0,
            fm_detune_type: 0,
            fm_freq_envelope_enabled: false,
            fm_freq_envelope: EnvelopeParams::new(0, false, Loc::AdVoiceFmFreq),
            fm_amp_envelope_enabled: false,
            fm_amp_envelope: EnvelopeParams::new(64, true, Loc::AdVoiceFmAmp),
            fm_velocity_scale_function: 64,
            sync_enabled: false,
            oscil: OscilGen::new(oscilsize, false),
            fm_oscil: OscilGen::new(oscilsize, false),
        }
    }

    pub fn unison_frequency_spread_cents(&self) -> f32 {
        (self.unison_frequency_spread as f32 / 127.0 * 2.0).powi(2) * 50.0
    }

    fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_par("type", self.vtype as i32);
        x.add_par("unison_size", self.unison_size as i32);
        x.add_par("unison_frequency_spread", self.unison_frequency_spread as i32);
        x.add_par("unison_stereo_spread", self.unison_stereo_spread as i32);
        x.add_par("unison_vibratto", self.unison_vibratto as i32);
        x.add_par("unison_vibratto_speed", self.unison_vibratto_speed as i32);
        x.add_par("unison_invert_phase", self.unison_invert_phase as i32);
        x.add_par("unison_phase_randomness", self.unison_phase_randomness as i32);
        x.add_par("delay", self.delay as i32);
        x.add_parbool("resonance", self.resonance);
        x.add_par("ext_oscil", self.ext_oscil);
        x.add_par("ext_fm_oscil", self.ext_fm_oscil);
        x.add_par("oscil_phase", self.oscil_phase as i32);
        x.add_par("oscil_fm_phase", self.fm_oscil_phase as i32);
        x.add_parbool("filter_enabled", self.filter_enabled);
        x.add_parbool("filter_bypass", self.filter_bypass);
        x.add_parbool("filter_fcctl_bypass", self.filter_fcctl_bypass);
        x.add_par("fm_enabled", self.fm_enabled as i32);
        x.add_parbool("sync_enabled", self.sync_enabled);
        self.oscil.p.add_to_xml(x.child("OSCIL"));
        let a = x.child("AMPLITUDE_PARAMETERS");
        a.add_par("panning", self.panning as i32);
        a.add_parreal("volume", self.volume);
        a.add_parbool("volume_minus", self.volume_minus);
        a.add_par("velocity_sensing", self.amp_velocity_scale_function as i32);
        a.add_parbool("amp_envelope_enabled", self.amp_envelope_enabled);
        self.amp_envelope.add_to_xml(a.child("AMPLITUDE_ENVELOPE"));
        a.add_parbool("amp_lfo_enabled", self.amp_lfo_enabled);
        self.amp_lfo.add_to_xml(a.child("AMPLITUDE_LFO"));
        let f = x.child("FREQUENCY_PARAMETERS");
        f.add_parbool("fixed_freq", self.fixed_freq);
        f.add_par("fixed_freq_et", self.fixed_freq_et as i32);
        f.add_par("bend_adjust", self.bend_adjust as i32);
        f.add_par("offset_hz", self.offset_hz as i32);
        f.add_par("detune", self.detune as i32);
        f.add_par("coarse_detune", self.coarse_detune as i32);
        f.add_par("detune_type", self.detune_type as i32);
        f.add_parbool("freq_envelope_enabled", self.freq_envelope_enabled);
        self.freq_envelope.add_to_xml(f.child("FREQUENCY_ENVELOPE"));
        f.add_parbool("freq_lfo_enabled", self.freq_lfo_enabled);
        self.freq_lfo.add_to_xml(f.child("FREQUENCY_LFO"));
        let fl = x.child("FILTER_PARAMETERS");
        fl.add_par("velocity_sensing_amplitude", self.filter_velocity_scale as i32);
        fl.add_par("velocity_sensing", self.filter_velocity_scale_function as i32);
        self.filter.add_to_xml(fl.child("FILTER"));
        fl.add_parbool("filter_envelope_enabled", self.filter_envelope_enabled);
        self.filter_envelope.add_to_xml(fl.child("FILTER_ENVELOPE"));
        fl.add_parbool("filter_lfo_enabled", self.filter_lfo_enabled);
        self.filter_lfo.add_to_xml(fl.child("FILTER_LFO"));
        let fm = x.child("FM_PARAMETERS");
        fm.add_par("input_voice", self.fm_voice);
        fm.add_parreal("volume", self.fm_volume);
        fm.add_par("volume_damp", self.fm_volume_damp as i32);
        fm.add_par("velocity_sensing", self.fm_velocity_scale_function as i32);
        fm.add_parbool("amp_envelope_enabled", self.fm_amp_envelope_enabled);
        self.fm_amp_envelope.add_to_xml(fm.child("AMPLITUDE_ENVELOPE"));
        let m = fm.child("MODULATOR");
        m.add_par("detune", self.fm_detune as i32);
        m.add_par("coarse_detune", self.fm_coarse_detune as i32);
        m.add_par("detune_type", self.fm_detune_type as i32);
        m.add_parbool("freq_envelope_enabled", self.fm_freq_envelope_enabled);
        m.add_parbool("fixed_freq", self.fm_fixed_freq);
        self.fm_freq_envelope.add_to_xml(m.child("FREQUENCY_ENVELOPE"));
        self.fm_oscil.p.add_to_xml(m.child("OSCIL"));
    }

    fn get_from_xml(&mut self, x: &XmlNode, nvoice: usize, ver: Version, oscilsize: usize) {
        self.enabled = x.parbool("enabled", false);
        self.unison_size = x.par127("unison_size", self.unison_size).clamp(1, 50);
        self.unison_frequency_spread = x.par127("unison_frequency_spread", self.unison_frequency_spread);
        self.unison_stereo_spread = x.par127("unison_stereo_spread", self.unison_stereo_spread);
        self.unison_vibratto = x.par127("unison_vibratto", self.unison_vibratto);
        self.unison_vibratto_speed = x.par127("unison_vibratto_speed", self.unison_vibratto_speed);
        self.unison_invert_phase = x.par127("unison_invert_phase", self.unison_invert_phase);
        self.unison_phase_randomness = x.par127("unison_phase_randomness", self.unison_phase_randomness);
        self.vtype = x.par127("type", self.vtype);
        self.delay = x.par127("delay", self.delay);
        self.resonance = x.parbool("resonance", self.resonance);
        self.ext_oscil = x.par("ext_oscil", -1, -1, nvoice as i32 - 1);
        self.ext_fm_oscil = x.par("ext_fm_oscil", -1, -1, nvoice as i32 - 1);
        self.oscil_phase = x.par127("oscil_phase", self.oscil_phase);
        self.fm_oscil_phase = x.par127("oscil_fm_phase", self.fm_oscil_phase);
        self.filter_enabled = x.parbool("filter_enabled", self.filter_enabled);
        self.filter_bypass = x.parbool("filter_bypass", self.filter_bypass);
        self.filter_fcctl_bypass = x.parbool("filter_fcctl_bypass", self.filter_fcctl_bypass);
        self.fm_enabled = FmType::from_i(x.par127("fm_enabled", self.fm_enabled as u8) as i32);
        self.sync_enabled = x.parbool("sync_enabled", self.sync_enabled);
        if let Some(o) = x.branch("OSCIL") {
            self.oscil.p.get_from_xml(o, oscilsize);
        }
        if let Some(a) = x.branch("AMPLITUDE_PARAMETERS") {
            self.panning = a.par127("panning", self.panning);
            if ver < Version(3, 0, 3) || !a.has_parreal("volume") {
                let vol = a.par127("volume", 0) as f32;
                self.volume = 60.0 * (vol / 127.0 - 1.0);
            } else {
                self.volume = a.parreal("volume", self.volume);
            }
            self.volume_minus = a.parbool("volume_minus", self.volume_minus);
            self.amp_velocity_scale_function = a.par127("velocity_sensing", self.amp_velocity_scale_function);
            self.amp_envelope_enabled = a.parbool("amp_envelope_enabled", self.amp_envelope_enabled);
            if let Some(e) = a.branch("AMPLITUDE_ENVELOPE") {
                self.amp_envelope.get_from_xml(e, ver);
            }
            self.amp_lfo_enabled = a.parbool("amp_lfo_enabled", self.amp_lfo_enabled);
            if let Some(l) = a.branch("AMPLITUDE_LFO") {
                self.amp_lfo.get_from_xml(l, ver);
            }
        }
        if let Some(f) = x.branch("FREQUENCY_PARAMETERS") {
            self.fixed_freq = f.parbool("fixed_freq", self.fixed_freq);
            self.fixed_freq_et = f.par127("fixed_freq_et", self.fixed_freq_et);
            self.bend_adjust = f.par127("bend_adjust", self.bend_adjust);
            self.offset_hz = f.par127("offset_hz", self.offset_hz);
            self.detune = f.par("detune", self.detune as i32, 0, 16383) as u16;
            self.coarse_detune = f.par("coarse_detune", self.coarse_detune as i32, 0, 16383) as u16;
            self.detune_type = f.par127("detune_type", self.detune_type);
            self.freq_envelope_enabled = f.parbool("freq_envelope_enabled", self.freq_envelope_enabled);
            if let Some(e) = f.branch("FREQUENCY_ENVELOPE") {
                self.freq_envelope.get_from_xml(e, ver);
            }
            self.freq_lfo_enabled = f.parbool("freq_lfo_enabled", self.freq_lfo_enabled);
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
            self.filter_envelope_enabled = f.parbool("filter_envelope_enabled", self.filter_envelope_enabled);
            if let Some(n) = f.branch("FILTER_ENVELOPE") {
                self.filter_envelope.get_from_xml(n, ver);
            }
            self.filter_lfo_enabled = f.parbool("filter_lfo_enabled", self.filter_lfo_enabled);
            if let Some(n) = f.branch("FILTER_LFO") {
                self.filter_lfo.get_from_xml(n, ver);
            }
        }
        if let Some(fm) = x.branch("FM_PARAMETERS") {
            let up303 = ver < Version(3, 0, 3) || fm.parreal("volume", -1.0) < 0.0;
            self.fm_voice = fm.par("input_voice", self.fm_voice, -1, nvoice as i32 - 1);
            if up303 {
                self.fm_volume = 100.0 * fm.par127("volume", 0) as f32 / 127.0;
            } else {
                self.fm_volume = fm.parreal("volume", self.fm_volume);
            }
            self.fm_volume_damp = fm.par127("volume_damp", self.fm_volume_damp);
            self.fm_velocity_scale_function = fm.par127("velocity_sensing", self.fm_velocity_scale_function);
            self.fm_amp_envelope_enabled = fm.parbool("amp_envelope_enabled", self.fm_amp_envelope_enabled);
            if let Some(e) = fm.branch("AMPLITUDE_ENVELOPE") {
                self.fm_amp_envelope.get_from_xml(e, ver);
            }
            if let Some(m) = fm.branch("MODULATOR") {
                self.fm_detune = m.par("detune", self.fm_detune as i32, 0, 16383) as u16;
                self.fm_coarse_detune = m.par("coarse_detune", self.fm_coarse_detune as i32, 0, 16383) as u16;
                self.fm_detune_type = m.par127("detune_type", self.fm_detune_type);
                self.fm_freq_envelope_enabled = m.parbool("freq_envelope_enabled", self.fm_freq_envelope_enabled);
                self.fm_fixed_freq = m.parbool("fixed_freq", self.fm_fixed_freq);
                if let Some(e) = m.branch("FREQUENCY_ENVELOPE") {
                    self.fm_freq_envelope.get_from_xml(e, ver);
                }
                if let Some(o) = m.branch("OSCIL") {
                    self.fm_oscil.p.get_from_xml(o, oscilsize);
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct AdParams {
    pub global: AdGlobal,
    pub voices: Vec<AdVoice>,
}

impl AdParams {
    pub fn new(oscilsize: usize) -> Self {
        let mut voices: Vec<AdVoice> = (0..NUM_VOICES).map(|_| AdVoice::new(oscilsize)).collect();
        voices[0].enabled = true;
        Self { global: AdGlobal::new(), voices }
    }

    pub fn bandwidth_detune_multiplier(&self) -> f32 {
        let bw = (self.global.bandwidth as f32 - 64.0) / 64.0;
        2f32.powf(bw * bw.abs().powf(0.2) * 5.0)
    }

    pub fn add_to_xml(&self, x: &mut XmlNode) {
        self.global.add_to_xml(x);
        for (n, v) in self.voices.iter().enumerate() {
            let vx = x.child_id("VOICE", n);
            vx.add_parbool("enabled", v.enabled);
            v.add_to_xml(vx);
        }
    }

    pub fn get_from_xml(&mut self, x: &XmlNode, ver: Version, oscilsize: usize) {
        self.global.get_from_xml(x, ver);
        for n in 0..NUM_VOICES {
            self.voices[n].enabled = false;
            if let Some(v) = x.branch_id("VOICE", n) {
                self.voices[n].get_from_xml(v, n, ver, oscilsize);
            }
        }
    }

    /// Rebuild every oscillator spectrum (after load or edit).
    pub fn prepare(&mut self, fft: &mut super::fft::Fft) {
        for v in self.voices.iter_mut() {
            v.oscil.prepare(fft);
            v.fm_oscil.prepare(fft);
        }
    }
}

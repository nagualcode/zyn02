//! SUBsynth: `Params/SUBnoteParameters.cpp` + `Synth/SUBnote.cpp`.
//! White noise through banks of band-pass filters, one bank per harmonic.

use super::envelope::{Envelope, EnvelopeParams, Loc};
use super::filter::FilterParams;
use super::modfilter::ModFilter;
use super::synthnote::{Legato, LegatoParams, porta_read};
use super::util::*;
use super::xml::{Version, XmlNode};
use super::NoteCtx;
use super::adnote::NoteInit;

#[derive(Clone, Debug)]
pub struct SubParams {
    pub stereo: bool,
    pub volume: f32,
    pub panning: u8,
    pub amp_velocity_scale_function: f32,
    pub amp_envelope: EnvelopeParams,
    pub detune: u16,
    pub coarse_detune: u16,
    pub detune_type: u8,
    pub bend_adjust: u8,
    pub offset_hz: u8,
    pub freq_envelope_enabled: bool,
    pub freq_envelope: EnvelopeParams,
    pub bandwidth_envelope_enabled: bool,
    pub bandwidth_envelope: EnvelopeParams,
    pub global_filter_enabled: bool,
    pub global_filter: FilterParams,
    pub global_filter_velocity_scale: u8,
    pub global_filter_velocity_scale_function: u8,
    pub global_filter_envelope: EnvelopeParams,
    pub fixed_freq: bool,
    pub fixed_freq_et: u8,
    pub overtone_spread_type: u8,
    pub overtone_spread_par1: u8,
    pub overtone_spread_par2: u8,
    pub overtone_spread_par3: u8,
    pub overtone_freq_mult: [f32; MAX_SUB_HARMONICS],
    pub numstages: u8,
    pub bandwidth: u8,
    pub hmagtype: u8,
    pub hmag: [u8; MAX_SUB_HARMONICS],
    pub hrelbw: [u8; MAX_SUB_HARMONICS],
    pub bwscale: u8,
    pub start: u8,
}

impl SubParams {
    pub fn new() -> Self {
        let mut hmag = [0; MAX_SUB_HARMONICS];
        hmag[0] = 127;
        let mut s = Self {
            stereo: true,
            volume: 0.0,
            panning: 64,
            amp_velocity_scale_function: 70.86,
            amp_envelope: EnvelopeParams::new(64, true, Loc::AdGlobalAmp),
            detune: 8192,
            coarse_detune: 0,
            detune_type: 1,
            bend_adjust: 88,
            offset_hz: 64,
            freq_envelope_enabled: false,
            freq_envelope: EnvelopeParams::new(64, false, Loc::SubFreq),
            bandwidth_envelope_enabled: false,
            bandwidth_envelope: EnvelopeParams::new(64, false, Loc::SubBandwidth),
            global_filter_enabled: false,
            global_filter: FilterParams::new(Loc::SubFilter),
            global_filter_velocity_scale: 0,
            global_filter_velocity_scale_function: 64,
            global_filter_envelope: EnvelopeParams::new(0, true, Loc::SubFilter),
            fixed_freq: false,
            fixed_freq_et: 0,
            overtone_spread_type: 0,
            overtone_spread_par1: 0,
            overtone_spread_par2: 0,
            overtone_spread_par3: 0,
            overtone_freq_mult: [1.0; MAX_SUB_HARMONICS],
            numstages: 2,
            bandwidth: 40,
            hmagtype: 0,
            hmag,
            hrelbw: [64; MAX_SUB_HARMONICS],
            bwscale: 64,
            start: 1,
        };
        s.update_frequency_multipliers();
        s
    }

    pub fn active_harmonics(&self) -> ([usize; MAX_SUB_HARMONICS], usize) {
        let mut pos = [0; MAX_SUB_HARMONICS];
        let mut n = 0;
        for (i, m) in self.hmag.iter().enumerate() {
            if *m != 0 {
                pos[n] = i;
                n += 1;
            }
        }
        (pos, n)
    }

    pub fn convert_bandwidth(bw: i32, stages: i32, freq: f32, scale: i32, relbw: i32) -> f32 {
        let mut b = 10f32.powf((bw as f32 - 127.0) / 127.0 * 4.0) * stages as f32;
        b *= (1000.0 / freq).powf((scale as f32 - 64.0) / 64.0 * 3.0);
        b *= 100f32.powf((relbw as f32 - 64.0) / 64.0);
        b.min(25.0)
    }

    pub fn convert_harmonic_mag(mag: i32, kind: i32) -> f32 {
        let h = 1.0 - mag as f32 / 127.0;
        match kind {
            1 => (h * 0.01f32.ln()).exp(),
            2 => (h * 0.001f32.ln()).exp(),
            3 => (h * 0.0001f32.ln()).exp(),
            4 => (h * 0.00001f32.ln()).exp(),
            _ => 1.0 - h,
        }
    }

    pub fn update_frequency_multipliers(&mut self) {
        let par1 = self.overtone_spread_par1 as f32 / 255.0;
        let par1pow = 10f32.powf(-(1.0 - par1) * 3.0);
        let par2 = self.overtone_spread_par2 as f32 / 255.0;
        let par3 = 1.0 - self.overtone_spread_par3 as f32 / 255.0;
        for n in 0..MAX_SUB_HARMONICS {
            let nf = n as f32;
            let n1 = nf + 1.0;
            let result = match self.overtone_spread_type {
                1 => {
                    let th = ((100.0 * par2 * par2) as i32 + 1) as f32;
                    if n1 < th { n1 } else { n1 + 8.0 * (n1 - th) * par1pow }
                }
                2 => {
                    let th = ((100.0 * par2 * par2) as i32 + 1) as f32;
                    if n1 < th { n1 } else { n1 + 0.9 * (th - n1) * par1pow }
                }
                3 => {
                    let t = par1pow * 100.0 + 1.0;
                    (nf / t).powf(1.0 - 0.8 * par2) * t + 1.0
                }
                4 => nf * (1.0 - par1pow) + (0.1 * nf).powf(3.0 * par2 + 1.0) * 10.0 * par1pow + 1.0,
                5 => n1 + 2.0 * (nf * par2 * par2 * PI * 0.999).sin() * par1pow.sqrt(),
                6 => {
                    let t = (2.0 * par2).powi(2) + 0.1;
                    nf * (par1 * (0.8 * nf).powf(t) + 1.0).powf(t) + 1.0
                }
                7 => (n1 + par1) / (par1 + 1.0),
                _ => n1,
            };
            let ir = (result + 0.5).floor();
            self.overtone_freq_mult[n] = ir + par3 * (result - ir);
        }
    }

    pub fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_par("num_stages", self.numstages as i32);
        x.add_par("harmonic_mag_type", self.hmagtype as i32);
        x.add_par("start", self.start as i32);
        let h = x.child("HARMONICS");
        for i in 0..MAX_SUB_HARMONICS {
            if self.hmag[i] == 0 {
                continue;
            }
            let c = h.child_id("HARMONIC", i);
            c.add_par("mag", self.hmag[i] as i32);
            c.add_par("relbw", self.hrelbw[i] as i32);
        }
        let a = x.child("AMPLITUDE_PARAMETERS");
        a.add_parbool("stereo", self.stereo);
        a.add_parreal("volume", self.volume);
        a.add_par("panning", self.panning as i32);
        a.add_parreal("velocity_sensing", self.amp_velocity_scale_function);
        self.amp_envelope.add_to_xml(a.child("AMPLITUDE_ENVELOPE"));
        let f = x.child("FREQUENCY_PARAMETERS");
        f.add_parbool("fixed_freq", self.fixed_freq);
        f.add_par("fixed_freq_et", self.fixed_freq_et as i32);
        f.add_par("bend_adjust", self.bend_adjust as i32);
        f.add_par("offset_hz", self.offset_hz as i32);
        f.add_par("detune", self.detune as i32);
        f.add_par("coarse_detune", self.coarse_detune as i32);
        f.add_par("overtone_spread_type", self.overtone_spread_type as i32);
        f.add_par("overtone_spread_par1", self.overtone_spread_par1 as i32);
        f.add_par("overtone_spread_par2", self.overtone_spread_par2 as i32);
        f.add_par("overtone_spread_par3", self.overtone_spread_par3 as i32);
        f.add_par("detune_type", self.detune_type as i32);
        f.add_par("bandwidth", self.bandwidth as i32);
        f.add_par("bandwidth_scale", self.bwscale as i32);
        f.add_parbool("freq_envelope_enabled", self.freq_envelope_enabled);
        self.freq_envelope.add_to_xml(f.child("FREQUENCY_ENVELOPE"));
        f.add_parbool("band_width_envelope_enabled", self.bandwidth_envelope_enabled);
        self.bandwidth_envelope.add_to_xml(f.child("BANDWIDTH_ENVELOPE"));
        let fl = x.child("FILTER_PARAMETERS");
        fl.add_parbool("enabled", self.global_filter_enabled);
        self.global_filter.add_to_xml(fl.child("FILTER"));
        fl.add_par("filter_velocity_sensing", self.global_filter_velocity_scale_function as i32);
        fl.add_par("filter_velocity_sensing_amplitude", self.global_filter_velocity_scale as i32);
        self.global_filter_envelope.add_to_xml(fl.child("FILTER_ENVELOPE"));
    }

    pub fn get_from_xml(&mut self, x: &XmlNode, ver: Version) {
        self.numstages = x.par127("num_stages", self.numstages);
        self.hmagtype = x.par127("harmonic_mag_type", self.hmagtype);
        self.start = x.par127("start", self.start);
        if let Some(h) = x.branch("HARMONICS") {
            self.hmag[0] = 0;
            for i in 0..MAX_SUB_HARMONICS {
                let Some(c) = h.branch_id("HARMONIC", i) else { continue };
                self.hmag[i] = c.par127("mag", self.hmag[i]);
                self.hrelbw[i] = c.par127("relbw", self.hrelbw[i]);
            }
        }
        if let Some(a) = x.branch("AMPLITUDE_PARAMETERS") {
            self.stereo = a.parbool("stereo", self.stereo);
            if ver < Version(3, 0, 3) || !a.has_parreal("volume") {
                let vol = a.par127("volume", 0) as f32;
                self.volume = 60.0 * (vol / 96.0 - 1.0);
            } else {
                self.volume = a.parreal("volume", self.volume);
            }
            self.panning = a.par127("panning", self.panning);
            if ver < Version(3, 0, 3) || a.parreal("velocity_sensing", -1.0) < 0.0 {
                self.amp_velocity_scale_function = 100.0 * a.par127("velocity_sensing", 0) as f32 / 127.0;
            } else {
                self.amp_velocity_scale_function = a.parreal("velocity_sensing", self.amp_velocity_scale_function);
            }
            if let Some(e) = a.branch("AMPLITUDE_ENVELOPE") {
                self.amp_envelope.get_from_xml(e, ver);
            }
        }
        if let Some(f) = x.branch("FREQUENCY_PARAMETERS") {
            self.fixed_freq = f.parbool("fixed_freq", self.fixed_freq);
            self.fixed_freq_et = f.par127("fixed_freq_et", self.fixed_freq_et);
            self.bend_adjust = f.par127("bend_adjust", self.bend_adjust);
            self.offset_hz = f.par127("offset_hz", self.offset_hz);
            self.detune = f.par("detune", self.detune as i32, 0, 16383) as u16;
            self.coarse_detune = f.par("coarse_detune", self.coarse_detune as i32, 0, 16383) as u16;
            self.overtone_spread_type = f.par127("overtone_spread_type", self.overtone_spread_type);
            self.overtone_spread_par1 = f.par("overtone_spread_par1", self.overtone_spread_par1 as i32, 0, 255) as u8;
            self.overtone_spread_par2 = f.par("overtone_spread_par2", self.overtone_spread_par2 as i32, 0, 255) as u8;
            self.overtone_spread_par3 = f.par("overtone_spread_par3", self.overtone_spread_par3 as i32, 0, 255) as u8;
            self.update_frequency_multipliers();
            self.detune_type = f.par127("detune_type", self.detune_type);
            self.bandwidth = f.par127("bandwidth", self.bandwidth);
            self.bwscale = f.par127("bandwidth_scale", self.bwscale);
            self.freq_envelope_enabled = f.parbool("freq_envelope_enabled", self.freq_envelope_enabled);
            if let Some(e) = f.branch("FREQUENCY_ENVELOPE") {
                self.freq_envelope.get_from_xml(e, ver);
            }
            self.bandwidth_envelope_enabled = f.parbool("band_width_envelope_enabled", self.bandwidth_envelope_enabled);
            if let Some(e) = f.branch("BANDWIDTH_ENVELOPE") {
                self.bandwidth_envelope.get_from_xml(e, ver);
            }
        }
        if let Some(f) = x.branch("FILTER_PARAMETERS") {
            self.global_filter_enabled = f.parbool("enabled", self.global_filter_enabled);
            if let Some(n) = f.branch("FILTER") {
                self.global_filter.get_from_xml(n, ver);
            }
            self.global_filter_velocity_scale_function = f.par127("filter_velocity_sensing", self.global_filter_velocity_scale_function);
            self.global_filter_velocity_scale = f.par127("filter_velocity_sensing_amplitude", self.global_filter_velocity_scale);
            if let Some(e) = f.branch("FILTER_ENVELOPE") {
                self.global_filter_envelope.get_from_xml(e, ver);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct BpFilter {
    freq: f32,
    bw: f32,
    amp: f32,
    a1: f32,
    a2: f32,
    b0: f32,
    b2: f32,
    xn1: f32,
    xn2: f32,
    yn1: f32,
    yn2: f32,
}

#[derive(Clone, Debug)]
pub struct SubNote {
    velocity: f32,
    portamento: Option<super::synthnote::PortaRef>,
    enabled: bool,
    volume: f32,
    panning: f32,
    numstages: usize,
    stereo: bool,
    start: u8,
    firsttick: bool,
    note_log2_freq: f32,
    bend_adjust: f32,
    offset_hz: f32,
    numharmonics: usize,
    firstnumharmonics: usize,
    lfilter: Vec<BpFilter>,
    rfilter: Vec<BpFilter>,
    overtone_rolloff: [f32; MAX_SUB_HARMONICS],
    overtone_freq: [f32; MAX_SUB_HARMONICS],
    oldreduceamp: f32,
    oldpitchwheel: i32,
    oldbandwidth: i32,
    oldamplitude: f32,
    newamplitude: f32,
    filterupdate: bool,
    amp_env: Option<Envelope>,
    freq_env: Option<Envelope>,
    bw_env: Option<Envelope>,
    global_filter: Option<ModFilter>,
    pub legato: Legato,
}

impl SubNote {
    pub fn new(pars: &SubParams, ini: &NoteInit, cx: &mut NoteCtx) -> Self {
        let mut n = Self {
            velocity: ini.velocity,
            portamento: ini.portamento,
            enabled: true,
            volume: 0.0,
            panning: 0.5,
            numstages: 1,
            stereo: true,
            start: 1,
            firsttick: true,
            note_log2_freq: ini.note_log2_freq,
            bend_adjust: 0.0,
            offset_hz: 0.0,
            numharmonics: 0,
            firstnumharmonics: 0,
            lfilter: Vec::new(),
            rfilter: Vec::new(),
            overtone_rolloff: [0.0; MAX_SUB_HARMONICS],
            overtone_freq: [0.0; MAX_SUB_HARMONICS],
            oldreduceamp: 1.0,
            oldpitchwheel: 0,
            oldbandwidth: 64,
            oldamplitude: 0.0,
            newamplitude: 0.0,
            filterupdate: false,
            amp_env: None,
            freq_env: None,
            bw_env: None,
            global_filter: None,
            legato: Legato::new(cx.synth, ini.velocity, ini.portamento, ini.note_log2_freq, ini.quiet, ini.seed),
        };
        n.setup(pars, ini.velocity, ini.portamento, ini.note_log2_freq, false, cx);
        n
    }

    fn computerolloff(&self, freq: f32, sr: f32) -> f32 {
        let (ll, lw, uw) = (10.0f32, 10.0f32, 200.0f32);
        let ul = sr / 2.0;
        if freq > ll + lw && freq < ul - uw {
            return 1.0;
        }
        if freq <= ll || freq >= ul {
            return 0.0;
        }
        if freq <= ll + lw {
            return (1.0 - (PI * (freq - ll) / lw).cos()) / 2.0;
        }
        (1.0 - (PI * (freq - ul) / uw).cos()) / 2.0
    }

    fn computefiltercoefs(f: &mut BpFilter, freq: f32, bw: f32, gain: f32, sr: f32) {
        let freq = freq.min(sr / 2.0 - 200.0);
        let omega = 2.0 * PI * freq / sr;
        let sn = omega.sin();
        let cs = omega.cos();
        let mut alpha = sn * (LOG_2 / 2.0 * bw * omega / sn).sinh();
        if alpha > 1.0 {
            alpha = 1.0;
        }
        if alpha > bw {
            alpha = bw;
        }
        f.b0 = alpha / (1.0 + alpha) * f.amp * gain;
        f.b2 = -alpha / (1.0 + alpha) * f.amp * gain;
        f.a1 = -2.0 * cs / (1.0 + alpha);
        f.a2 = (1.0 - alpha) / (1.0 + alpha);
    }

    fn initfilter(&mut self, idx: usize, right: bool, freq: f32, bw: f32, amp: f32, mag: f32, automation: bool, sr: f32) {
        let start = self.start;
        let f = if right { &mut self.rfilter[idx] } else { &mut self.lfilter[idx] };
        if !automation {
            f.xn1 = 0.0;
            f.xn2 = 0.0;
            if start == 0 {
                f.yn1 = 0.0;
                f.yn2 = 0.0;
            } else {
                let mut a = 0.1 * mag;
                let p = rnd() * 2.0 * PI;
                if start == 1 {
                    a *= rnd();
                }
                f.yn1 = a * p.cos();
                f.yn2 = a * (p + freq * 2.0 * PI / sr).cos();
                if freq > sr * 0.96 {
                    f.yn1 = 0.0;
                    f.yn2 = 0.0;
                }
            }
        }
        f.amp = amp;
        f.freq = freq;
        f.bw = bw;
        if !automation {
            Self::computefiltercoefs(f, freq, bw, 1.0, sr);
        } else {
            self.filterupdate = true;
        }
    }

    fn setup_filters(&mut self, pars: &SubParams, basefreq: f32, pos: &[usize], automation: bool, sr: f32) -> f32 {
        let mut reduceamp = 0.0;
        for n in 0..self.numharmonics {
            let freq = basefreq * pars.overtone_freq_mult[pos[n]];
            self.overtone_freq[n] = freq;
            self.overtone_rolloff[n] = self.computerolloff(freq, sr);
            let bw = SubParams::convert_bandwidth(pars.bandwidth as i32, self.numstages as i32, freq, pars.bwscale as i32, pars.hrelbw[pos[n]] as i32);
            let hgain = SubParams::convert_harmonic_mag(pars.hmag[pos[n]] as i32, pars.hmagtype as i32);
            let gain = hgain * (1500.0 / (bw * freq)).sqrt();
            reduceamp += hgain;
            for nph in 0..self.numstages {
                let amp = if nph == 0 { gain } else { 1.0 };
                let idx = nph + n * self.numstages;
                self.initfilter(idx, false, freq + self.offset_hz, bw, amp, hgain, automation, sr);
                if self.stereo {
                    self.initfilter(idx, true, freq + self.offset_hz, bw, amp, hgain, automation, sr);
                }
            }
        }
        if reduceamp < 0.001 {
            reduceamp = 1.0;
        }
        reduceamp
    }

    fn setup(&mut self, pars: &SubParams, vel: f32, porta: Option<super::synthnote::PortaRef>, note_log2_freq: f32, legato: bool, cx: &mut NoteCtx) {
        let synth = *cx.synth;
        self.velocity = vel;
        self.portamento = porta;
        self.enabled = true;
        self.volume = 10f32.powf(pars.volume / 20.0);
        self.volume *= vel_f(vel, pars.amp_velocity_scale_function as u8);
        if pars.panning != 0 {
            self.panning = pars.panning as f32 / 127.0;
        } else if !legato {
            self.panning = rnd();
        }
        if !legato {
            self.numstages = pars.numstages as usize;
            self.stereo = pars.stereo;
            self.start = pars.start;
            self.firsttick = true;
        }
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
        self.bend_adjust = (pars.bend_adjust as i32 - 64) as f32 / 24.0;
        let ov = (pars.offset_hz as f32 - 64.0) / 64.0;
        self.offset_hz = 15.0 * (ov * ov.abs().sqrt());
        let detune = getdetune(pars.detune_type, pars.coarse_detune, pars.detune);
        self.note_log2_freq += detune / 1200.0;
        let basefreq = 2f32.powf(self.note_log2_freq);
        let (pos, harmonics) = pars.active_harmonics();
        if !legato {
            self.numharmonics = harmonics;
            self.firstnumharmonics = harmonics;
        } else {
            self.numharmonics = harmonics.min(self.firstnumharmonics);
        }
        if self.numharmonics == 0 {
            self.enabled = false;
            return;
        }
        if !legato {
            let n = self.numstages * self.numharmonics;
            self.lfilter = vec![BpFilter::default(); n];
            if self.stereo {
                self.rfilter = vec![BpFilter::default(); n];
            }
        }
        let reduceamp = self.setup_filters(pars, basefreq, &pos, legato, synth.samplerate_f);
        self.oldreduceamp = reduceamp;
        self.volume /= reduceamp;
        self.oldpitchwheel = 0;
        self.oldbandwidth = 64;
        let freq = 2f32.powf(note_log2_freq);
        let ff = if !pars.fixed_freq { basefreq } else { basefreq / 440.0 * freq };
        if !legato {
            self.initparameters(pars, ff, cx);
        } else if let Some(g) = self.global_filter.as_mut() {
            g.update_note_freq(ff, &pars.global_filter);
            g.update_sense(self.velocity, pars.global_filter_velocity_scale, pars.global_filter_velocity_scale_function);
        }
    }

    fn initparameters(&mut self, pars: &SubParams, freq: f32, cx: &mut NoteCtx) {
        let synth = *cx.synth;
        let dt = synth.dt();
        self.amp_env = Some(Envelope::new(&pars.amp_envelope, freq, dt));
        if pars.freq_envelope_enabled {
            self.freq_env = Some(Envelope::new(&pars.freq_envelope, freq, dt));
        }
        if pars.bandwidth_envelope_enabled {
            self.bw_env = Some(Envelope::new(&pars.bandwidth_envelope, freq, dt));
        }
        if pars.global_filter_enabled {
            let mut f = ModFilter::new(&pars.global_filter, synth.samplerate_f, synth.buffersize, self.stereo, freq)
                .with_env(&pars.global_filter_envelope, freq, dt);
            f.update_sense(self.velocity, pars.global_filter_velocity_scale, pars.global_filter_velocity_scale_function);
            self.global_filter = Some(f);
        }
        self.computecurrentparameters(pars, cx);
        self.oldamplitude = self.newamplitude;
    }

    fn computecurrentparameters(&mut self, pars: &SubParams, cx: &mut NoteCtx) {
        let synth = *cx.synth;
        let ctl = cx.ctl;
        let sr = synth.samplerate_f;
        if cx.changed {
            let (pos, harmonics) = pars.active_harmonics();
            let delta = harmonics != self.numharmonics;
            if delta {
                self.numharmonics = harmonics;
                self.firstnumharmonics = harmonics;
                let n = self.numstages * self.numharmonics;
                self.lfilter = vec![BpFilter::default(); n];
                if self.stereo {
                    self.rfilter = vec![BpFilter::default(); n];
                }
            }
            let basefreq = 2f32.powf(self.note_log2_freq);
            let reduceamp = self.setup_filters(pars, basefreq, &pos, !delta, sr);
            self.volume = self.volume * self.oldreduceamp / reduceamp;
            self.oldreduceamp = reduceamp;
        }
        if self.freq_env.is_some()
            || self.bw_env.is_some()
            || self.oldpitchwheel != ctl.pitchwheel_data
            || self.oldbandwidth != ctl.bandwidth_data
            || self.portamento.is_some()
            || self.filterupdate
        {
            let mut envfreq = 1.0;
            let mut envbw = 1.0;
            if let Some(e) = self.freq_env.as_mut() {
                envfreq = 2f32.powf(e.envout() / 1200.0);
            }
            envfreq *= ctl.pitch_relfreq.powf(self.bend_adjust);
            if let Some(r) = self.portamento {
                match porta_read(cx.portas, r) {
                    Some(p) => {
                        envfreq *= 2f32.powf(p.freqdelta_log2);
                        if !p.active {
                            self.portamento = None;
                        }
                    }
                    None => self.portamento = None,
                }
            }
            if let Some(e) = self.bw_env.as_mut() {
                envbw = 2f32.powf(e.envout());
            }
            envbw *= ctl.bandwidth_relbw;
            for n in 0..self.numharmonics {
                self.overtone_rolloff[n] = self.computerolloff(self.overtone_freq[n] * envfreq, sr);
            }
            let gain = 1.0 / (envbw * envfreq).sqrt();
            let ns = self.numstages;
            for bank in [&mut self.lfilter, &mut self.rfilter] {
                for (i, f) in bank.iter_mut().enumerate() {
                    let g = if i % ns == 0 { gain } else { 1.0 };
                    let (fq, bw) = (f.freq * envfreq, f.bw * envbw);
                    Self::computefiltercoefs(f, fq, bw, g, sr);
                }
            }
            self.oldbandwidth = ctl.bandwidth_data;
            self.oldpitchwheel = ctl.pitchwheel_data;
            self.filterupdate = false;
        }
        let a = self.amp_env.as_mut().map(|e| e.envout_db()).unwrap_or(0.0);
        self.newamplitude = self.volume * a * 2.0;
        if let Some(g) = self.global_filter.as_mut() {
            g.update(ctl.filtercutoff_relfreq, ctl.filterq_relq, &pars.global_filter, None, cx.time, cx.changed);
        }
    }

    fn run_bank(bank: &mut [BpFilter], numharmonics: usize, numstages: usize, rolloff: &[f32], out: &mut [f32], bs: usize) {
        let mut rndbuf = [0.0f32; super::filter::MAX_BUF];
        for r in rndbuf.iter_mut().take(bs) {
            *r = rnd() * 2.0 - 1.0;
        }
        let mut tmp = [0.0f32; super::filter::MAX_BUF];
        for n in 0..numharmonics {
            tmp[..bs].copy_from_slice(&rndbuf[..bs]);
            for nph in 0..numstages {
                let f = &mut bank[nph + n * numstages];
                let c = [f.b0, f.b2, -f.a1, -f.a2];
                let mut w = [f.xn1, f.xn2, f.yn1, f.yn2];
                for pair in tmp[..bs].chunks_mut(2) {
                    w[3] = pair[0] * c[0] + w[1] * c[1] + w[2] * c[2] + w[3] * c[3];
                    w[1] = pair[0];
                    pair[0] = w[3];
                    if pair.len() > 1 {
                        w[2] = pair[1] * c[0] + w[0] * c[1] + w[3] * c[2] + w[2] * c[3];
                        w[0] = pair[1];
                        pair[1] = w[2];
                    }
                }
                f.xn1 = w[0];
                f.xn2 = w[1];
                f.yn1 = w[2];
                f.yn2 = w[3];
            }
            let ro = rolloff[n];
            for i in 0..bs {
                out[i] += tmp[i] * ro;
            }
        }
    }

    pub fn noteout(&mut self, pars: &SubParams, outl: &mut [f32], outr: &mut [f32], cx: &mut NoteCtx) {
        let synth = *cx.synth;
        let bs = synth.buffersize;
        outl[..bs].fill(0.0);
        outr[..bs].fill(0.0);
        if !self.enabled {
            return;
        }
        let (nh, ns) = (self.numharmonics, self.numstages);
        let ro = self.overtone_rolloff;
        if self.stereo {
            Self::run_bank(&mut self.lfilter, nh, ns, &ro, outl, bs);
            Self::run_bank(&mut self.rfilter, nh, ns, &ro, outr, bs);
            if let Some(g) = self.global_filter.as_mut() {
                g.filter(&mut outl[..bs], Some(&mut outr[..bs]));
            }
        } else {
            Self::run_bank(&mut self.lfilter, nh, ns, &ro, outl, bs);
            if let Some(g) = self.global_filter.as_mut() {
                g.filter(&mut outl[..bs], None);
            }
            outr[..bs].copy_from_slice(&outl[..bs]);
        }
        if self.firsttick {
            let n = 10.min(bs);
            for i in 0..n {
                let a = 0.5 - 0.5 * (i as f32 / n as f32 * PI).cos();
                outl[i] *= a;
                outr[i] *= a;
            }
            self.firsttick = false;
        }
        let (pl, pr) = if cx.const_power { ((1.0 - self.panning).sqrt(), self.panning.sqrt()) } else { (1.0 - self.panning, self.panning) };
        let (oa, na) = (self.oldamplitude, self.newamplitude);
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
        self.oldamplitude = self.newamplitude;
        self.computecurrentparameters(pars, cx);
        if let Some(lp) = self.legato.apply(&mut outl[..bs], &mut outr[..bs]) {
            self.legatonote(pars, &lp, cx);
        }
        if self.amp_env.as_ref().map(|e| e.finished()).unwrap_or(true) {
            for i in 0..bs {
                let t = 1.0 - i as f32 / synth.buffersize_f;
                outl[i] *= t;
                outr[i] *= t;
            }
            self.enabled = false;
        }
    }

    pub fn legatonote(&mut self, pars: &SubParams, lp: &LegatoParams, cx: &mut NoteCtx) {
        if self.legato.update(lp) {
            return;
        }
        self.setup(pars, lp.velocity, lp.portamento, lp.note_log2_freq, true, cx);
    }

    pub fn releasekey(&mut self) {
        if let Some(e) = self.amp_env.as_mut() {
            e.release_key();
        }
        if let Some(e) = self.freq_env.as_mut() {
            e.release_key();
        }
        if let Some(e) = self.bw_env.as_mut() {
            e.release_key();
        }
        if let Some(f) = self.global_filter.as_mut() {
            if let Some(e) = f.env.as_mut() {
                e.release_key();
            }
        }
    }

    pub fn finished(&self) -> bool {
        !self.enabled
    }

    pub fn entomb(&mut self) {
        if let Some(e) = self.amp_env.as_mut() {
            e.force_finish();
        }
    }
}

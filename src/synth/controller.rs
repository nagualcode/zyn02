//! `Params/Controller.cpp`: MIDI controller state of a part.

use super::xml::XmlNode;

/// The saved controller configuration.
#[derive(Clone, Debug)]
pub struct ControllerCfg {
    pub bendrange: i32,
    pub bendrange_down: i32,
    pub bend_split: bool,
    pub expression_receive: bool,
    pub panning_depth: u8,
    pub filtercutoff_depth: u8,
    pub filterq_depth: u8,
    pub bandwidth_depth: u8,
    pub bandwidth_exponential: bool,
    pub modwheel_depth: u8,
    pub modwheel_exponential: bool,
    pub fmamp_receive: bool,
    pub volume_receive: bool,
    pub sustain_receive: bool,
    pub portamento_receive: bool,
    pub portamento_auto: bool,
    pub portamento_time: u8,
    pub portamento_pitchthresh: u8,
    pub portamento_pitchthreshtype: bool,
    pub portamento_portamento: bool,
    pub portamento_updowntimestretch: u8,
    pub portamento_proportional: bool,
    pub portamento_proprate: u8,
    pub portamento_propdepth: u8,
    pub resonancecenter_depth: u8,
    pub resonancebw_depth: u8,
}

impl Default for ControllerCfg {
    fn default() -> Self {
        Self {
            bendrange: 200,
            bendrange_down: 0,
            bend_split: false,
            expression_receive: true,
            panning_depth: 64,
            filtercutoff_depth: 64,
            filterq_depth: 64,
            bandwidth_depth: 64,
            bandwidth_exponential: false,
            modwheel_depth: 80,
            modwheel_exponential: false,
            fmamp_receive: true,
            volume_receive: true,
            sustain_receive: true,
            portamento_receive: true,
            portamento_auto: true,
            portamento_time: 64,
            portamento_pitchthresh: 3,
            portamento_pitchthreshtype: true,
            portamento_portamento: false,
            portamento_updowntimestretch: 64,
            portamento_proportional: false,
            portamento_proprate: 80,
            portamento_propdepth: 90,
            resonancecenter_depth: 64,
            resonancebw_depth: 64,
        }
    }
}

impl ControllerCfg {
    pub fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_par("pitchwheel_bendrange", self.bendrange);
        x.add_par("pitchwheel_bendrange_down", self.bendrange_down);
        x.add_parbool("pitchwheel_split", self.bend_split);
        x.add_parbool("expression_receive", self.expression_receive);
        x.add_par("panning_depth", self.panning_depth as i32);
        x.add_par("filter_cutoff_depth", self.filtercutoff_depth as i32);
        x.add_par("filter_q_depth", self.filterq_depth as i32);
        x.add_par("bandwidth_depth", self.bandwidth_depth as i32);
        x.add_par("mod_wheel_depth", self.modwheel_depth as i32);
        x.add_parbool("mod_wheel_exponential", self.modwheel_exponential);
        x.add_parbool("fm_amp_receive", self.fmamp_receive);
        x.add_parbool("volume_receive", self.volume_receive);
        x.add_parbool("sustain_receive", self.sustain_receive);
        x.add_parbool("portamento_receive", self.portamento_receive);
        x.add_par("portamento_time", self.portamento_time as i32);
        x.add_par("portamento_pitchthresh", self.portamento_pitchthresh as i32);
        x.add_par("portamento_pitchthreshtype", self.portamento_pitchthreshtype as i32);
        x.add_par("portamento_portamento", self.portamento_portamento as i32);
        x.add_parbool("portamento_auto", self.portamento_auto);
        x.add_par("portamento_updowntimestretch", self.portamento_updowntimestretch as i32);
        x.add_par("portamento_proportional", self.portamento_proportional as i32);
        x.add_par("portamento_proprate", self.portamento_proprate as i32);
        x.add_par("portamento_propdepth", self.portamento_propdepth as i32);
        x.add_par("resonance_center_depth", self.resonancecenter_depth as i32);
        x.add_par("resonance_bandwidth_depth", self.resonancebw_depth as i32);
    }

    pub fn get_from_xml(&mut self, x: &XmlNode) {
        self.bendrange = x.par("pitchwheel_bendrange", self.bendrange, -6400, 6400);
        self.bendrange_down = x.par("pitchwheel_bendrange_down", self.bendrange_down, -6400, 6400);
        self.bend_split = x.parbool("pitchwheel_split", self.bend_split);
        self.expression_receive = x.parbool("expression_receive", self.expression_receive);
        self.panning_depth = x.par127("panning_depth", self.panning_depth);
        self.filtercutoff_depth = x.par127("filter_cutoff_depth", self.filtercutoff_depth);
        self.filterq_depth = x.par127("filter_q_depth", self.filterq_depth);
        self.bandwidth_depth = x.par127("bandwidth_depth", self.bandwidth_depth);
        self.modwheel_depth = x.par127("mod_wheel_depth", self.modwheel_depth);
        self.modwheel_exponential = x.parbool("mod_wheel_exponential", self.modwheel_exponential);
        self.fmamp_receive = x.parbool("fm_amp_receive", self.fmamp_receive);
        self.volume_receive = x.parbool("volume_receive", self.volume_receive);
        self.sustain_receive = x.parbool("sustain_receive", self.sustain_receive);
        self.portamento_receive = x.parbool("portamento_receive", self.portamento_receive);
        self.portamento_auto = x.parbool("portamento_auto", self.portamento_auto);
        self.portamento_time = x.par127("portamento_time", self.portamento_time);
        self.portamento_pitchthresh = x.par127("portamento_pitchthresh", self.portamento_pitchthresh);
        self.portamento_pitchthreshtype = x.par("portamento_pitchthreshtype", self.portamento_pitchthreshtype as i32, 0, 1) != 0;
        self.portamento_portamento = x.par("portamento_portamento", self.portamento_portamento as i32, 0, 1) != 0;
        self.portamento_updowntimestretch = x.par127("portamento_updowntimestretch", self.portamento_updowntimestretch);
        self.portamento_proportional = x.par("portamento_proportional", self.portamento_proportional as i32, 0, 1) != 0;
        self.portamento_proprate = x.par127("portamento_proprate", self.portamento_proprate);
        self.portamento_propdepth = x.par127("portamento_propdepth", self.portamento_propdepth);
        self.resonancecenter_depth = x.par127("resonance_center_depth", self.resonancecenter_depth);
        self.resonancebw_depth = x.par127("resonance_bandwidth_depth", self.resonancebw_depth);
    }
}

/// Live controller values derived from MIDI data and the configuration.
#[derive(Clone, Debug)]
pub struct Controller {
    pub cfg: ControllerCfg,
    pub pitchwheel_data: i32,
    pub pitch_relfreq: f32,
    pub expression_data: i32,
    pub expression_relvolume: f32,
    pub panning_data: i32,
    pub pan: f32,
    pub filtercutoff_data: i32,
    pub filtercutoff_relfreq: f32,
    pub filterq_data: i32,
    pub filterq_relq: f32,
    pub bandwidth_data: i32,
    pub bandwidth_relbw: f32,
    pub modwheel_data: i32,
    pub modwheel_relmod: f32,
    pub fmamp_data: i32,
    pub fmamp_relamp: f32,
    pub volume_data: i32,
    pub volume: f32,
    pub sustain_data: i32,
    pub sustain: bool,
    pub portamento_data: i32,
    pub portamento: bool,
    pub rescenter_data: i32,
    pub rescenter_relcenter: f32,
    pub resbw_data: i32,
    pub resbw_relbw: f32,
}

impl Controller {
    pub fn new(cfg: ControllerCfg) -> Self {
        let portamento = cfg.portamento_portamento;
        let mut c = Self {
            cfg,
            pitchwheel_data: 0,
            pitch_relfreq: 1.0,
            expression_data: 127,
            expression_relvolume: 1.0,
            panning_data: 64,
            pan: 0.0,
            filtercutoff_data: 64,
            filtercutoff_relfreq: 0.0,
            filterq_data: 64,
            filterq_relq: 1.0,
            bandwidth_data: 64,
            bandwidth_relbw: 1.0,
            modwheel_data: 64,
            modwheel_relmod: 1.0,
            fmamp_data: 127,
            fmamp_relamp: 1.0,
            volume_data: 127,
            volume: 1.0,
            sustain_data: 0,
            sustain: false,
            portamento_data: 0,
            portamento,
            rescenter_data: 64,
            rescenter_relcenter: 1.0,
            resbw_data: 64,
            resbw_relbw: 1.0,
        };
        c.setportamento(0);
        c.resetall();
        c
    }

    /// Apply a new configuration, keeping the live MIDI data.
    pub fn set_cfg(&mut self, cfg: &ControllerCfg) {
        self.cfg = cfg.clone();
        self.portamento = cfg.portamento_portamento;
        self.recompute();
    }

    pub fn recompute(&mut self) {
        self.setpitchwheel(self.pitchwheel_data);
        self.setexpression(self.expression_data);
        self.setpanning(self.panning_data);
        self.setfiltercutoff(self.filtercutoff_data);
        self.setfilterq(self.filterq_data);
        self.setbandwidth(self.bandwidth_data);
        self.setmodwheel(self.modwheel_data);
        self.setfmamp(self.fmamp_data);
        self.setvolume(self.volume_data);
        self.setsustain(self.sustain_data);
        self.setresonancecenter(self.rescenter_data);
        self.setresonancebw(self.resbw_data);
    }

    pub fn resetall(&mut self) {
        self.setpitchwheel(0);
        self.setexpression(127);
        self.setpanning(64);
        self.setfiltercutoff(64);
        self.setfilterq(64);
        self.setbandwidth(64);
        self.setmodwheel(64);
        self.setfmamp(127);
        self.setvolume(127);
        self.setsustain(0);
        self.setresonancecenter(64);
        self.setresonancebw(64);
    }

    pub fn setpitchwheel(&mut self, value: i32) {
        self.pitchwheel_data = value;
        let mut cents = value as f32 / 8192.0;
        if self.cfg.bend_split && cents < 0.0 {
            cents *= self.cfg.bendrange_down as f32;
        } else {
            cents *= self.cfg.bendrange as f32;
        }
        self.pitch_relfreq = 2f32.powf(cents / 1200.0);
    }
    pub fn setexpression(&mut self, v: i32) {
        self.expression_data = v;
        self.expression_relvolume = if self.cfg.expression_receive { v as f32 / 127.0 } else { 1.0 };
    }
    pub fn setpanning(&mut self, v: i32) {
        self.panning_data = v;
        self.pan = (v as f32 / 128.0 - 0.5) * (self.cfg.panning_depth as f32 / 64.0);
    }
    pub fn setfiltercutoff(&mut self, v: i32) {
        self.filtercutoff_data = v;
        self.filtercutoff_relfreq = (v as f32 - 64.0) * self.cfg.filtercutoff_depth as f32 / 4096.0 * 3.321_928;
    }
    pub fn setfilterq(&mut self, v: i32) {
        self.filterq_data = v;
        self.filterq_relq = 30f32.powf((v as f32 - 64.0) / 64.0 * (self.cfg.filterq_depth as f32 / 64.0));
    }
    pub fn setbandwidth(&mut self, v: i32) {
        self.bandwidth_data = v;
        let depth = self.cfg.bandwidth_depth as f32;
        if !self.cfg.bandwidth_exponential {
            let mut tmp = 25f32.powf((depth / 127.0).powf(1.5)) - 1.0;
            if v < 64 && self.cfg.bandwidth_depth >= 64 {
                tmp = 1.0;
            }
            self.bandwidth_relbw = ((v as f32 / 64.0 - 1.0) * tmp + 1.0).max(0.01);
        } else {
            self.bandwidth_relbw = 25f32.powf((v as f32 - 64.0) / 64.0 * (depth / 64.0));
        }
    }
    pub fn setmodwheel(&mut self, v: i32) {
        self.modwheel_data = v;
        let depth = self.cfg.modwheel_depth as f32;
        if !self.cfg.modwheel_exponential {
            let mut tmp = 25f32.powf((depth / 127.0).powf(1.5) * 2.0) / 25.0;
            if v < 64 && self.cfg.modwheel_depth >= 64 {
                tmp = 1.0;
            }
            self.modwheel_relmod = ((v as f32 / 64.0 - 1.0) * tmp + 1.0).max(0.0);
        } else {
            self.modwheel_relmod = 25f32.powf((v as f32 - 64.0) / 64.0 * (depth / 80.0));
        }
    }
    pub fn setfmamp(&mut self, v: i32) {
        self.fmamp_data = v;
        self.fmamp_relamp = if self.cfg.fmamp_receive { v as f32 / 127.0 } else { 1.0 };
    }
    pub fn setvolume(&mut self, v: i32) {
        self.volume_data = v;
        self.volume = if self.cfg.volume_receive { v as f32 / 127.0 } else { 1.0 };
    }
    pub fn setsustain(&mut self, v: i32) {
        self.sustain_data = v;
        self.sustain = self.cfg.sustain_receive && v >= 64;
    }
    pub fn setportamento(&mut self, v: i32) {
        self.portamento_data = v;
        if self.cfg.portamento_receive {
            self.portamento = v >= 64;
        }
    }
    pub fn setresonancecenter(&mut self, v: i32) {
        self.rescenter_data = v;
        self.rescenter_relcenter = 3f32.powf((v as f32 - 64.0) / 64.0 * (self.cfg.resonancecenter_depth as f32 / 64.0));
    }
    pub fn setresonancebw(&mut self, v: i32) {
        self.resbw_data = v;
        self.resbw_relbw = 1.5f32.powf((v as f32 - 64.0) / 64.0 * (self.cfg.resonancebw_depth as f32 / 127.0));
    }
}

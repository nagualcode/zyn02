//! `Misc/Microtonal.cpp`: tuning (12-TET by default, Scala-style scales
//! and keyboard mappings from .xmz files).

use super::xml::XmlNode;

pub const MAX_OCTAVE_SIZE: usize = 128;

#[derive(Clone, Copy, Debug)]
pub struct Degree {
    pub tuning_log2: f32,
    pub kind: u8,
    pub x1: i32,
    pub x2: i32,
}

#[derive(Clone, Debug)]
pub struct Microtonal {
    pub invertupdown: bool,
    pub invertupdowncenter: u8,
    pub octavesize: usize,
    pub enabled: bool,
    pub anote: u8,
    pub afreq: f32,
    pub scaleshift: u8,
    pub firstkey: u8,
    pub lastkey: u8,
    pub middlenote: u8,
    pub mapsize: usize,
    pub mappingenabled: bool,
    pub mapping: [i16; 128],
    pub octave: Vec<Degree>,
    pub globalfinedetune: u8,
    pub name: String,
    pub comment: String,
}

impl Default for Microtonal {
    fn default() -> Self {
        let mut octave: Vec<Degree> = (0..MAX_OCTAVE_SIZE)
            .map(|i| Degree { tuning_log2: ((i % 12) + 1) as f32 / 12.0, kind: 1, x1: ((i % 12 + 1) * 100) as i32, x2: 0 })
            .collect();
        octave[11] = Degree { tuning_log2: 1.0, kind: 2, x1: 2, x2: 1 };
        let mut mapping = [0i16; 128];
        for (i, m) in mapping.iter_mut().enumerate() {
            *m = i as i16;
        }
        Self {
            invertupdown: false,
            invertupdowncenter: 60,
            octavesize: 12,
            enabled: false,
            anote: 69,
            afreq: 440.0,
            scaleshift: 64,
            firstkey: 0,
            lastkey: 127,
            middlenote: 60,
            mapsize: 12,
            mappingenabled: false,
            mapping,
            octave,
            globalfinedetune: 64,
            name: "12tET".into(),
            comment: "Equal Temperament 12 notes per octave".into(),
        }
    }
}

impl Microtonal {
    /// `updatenotefreq_log2`; `None` when the key is unmapped.
    pub fn note_log2(&self, note_log2_freq: f32, keyshift: i32) -> Option<f32> {
        let mut note = (12.0 * note_log2_freq).round() as i32;
        let mut freq_log2 = note_log2_freq;
        if self.invertupdown && (!self.mappingenabled || !self.enabled) {
            note = self.invertupdowncenter as i32 * 2 - note;
            freq_log2 = self.invertupdowncenter as f32 * (2.0 / 12.0) - freq_log2;
        }
        let gfd = (self.globalfinedetune as f32 - 64.0) / 1200.0;
        if !self.enabled {
            freq_log2 += (keyshift - self.anote as i32) as f32 / 12.0;
        } else {
            let os = self.octavesize.max(1) as i32;
            let oct = |i: i32| self.octave[i.rem_euclid(MAX_OCTAVE_SIZE as i32) as usize].tuning_log2;
            let scaleshift = (self.scaleshift as i32 - 64 + os * 100) % os;
            let rap_keyshift = if keyshift != 0 {
                let kskey = (keyshift + os * 100) % os;
                let ksoct = (keyshift + os * 100) / os - 100;
                (if kskey == 0 { 0.0 } else { oct(kskey - 1) }) + oct(os - 1) * ksoct as f32
            } else {
                0.0
            };
            if self.mappingenabled {
                if note < self.firstkey as i32 || note > self.lastkey as i32 {
                    return None;
                }
                let ms = self.mapsize.max(1) as i32;
                let mut tmp = self.anote as i32 - self.middlenote as i32;
                let minus = tmp < 0;
                if minus {
                    tmp = -tmp;
                }
                let mut deltanote = 0;
                for i in 0..tmp {
                    if self.mapping[(i % ms) as usize] >= 0 {
                        deltanote += 1;
                    }
                }
                let mut rap_anote = if deltanote == 0 { 0.0 } else { oct((deltanote - 1) % os) + oct(os - 1) * ((deltanote - 1) / os) as f32 };
                if minus {
                    rap_anote = -rap_anote;
                }
                let mut degoct = (note - self.middlenote as i32 + ms * 200) / ms - 200;
                let degkey_i = (note - self.middlenote as i32 + ms * 100) % ms;
                let mut degkey = self.mapping[degkey_i as usize] as i32;
                if degkey < 0 {
                    return None;
                }
                if self.invertupdown {
                    degkey = os - degkey - 1;
                    degoct = -degoct;
                }
                degkey += scaleshift;
                degoct += degkey / os;
                degkey %= os;
                freq_log2 = (if degkey == 0 { 0.0 } else { oct(degkey - 1) }) + oct(os - 1) * degoct as f32 - rap_anote;
            } else {
                let nt = note - self.anote as i32 + scaleshift;
                let ntkey = (nt + os * 100) % os;
                let ntoct = (nt - ntkey) / os;
                freq_log2 = oct((ntkey + os - 1) % os) + oct(os - 1) * (if ntkey != 0 { ntoct } else { ntoct - 1 }) as f32;
            }
            if scaleshift != 0 {
                freq_log2 -= oct(scaleshift - 1);
            }
            freq_log2 += rap_keyshift;
        }
        freq_log2 += self.afreq.log2();
        freq_log2 += gfd;
        Some(freq_log2)
    }

    pub fn add_to_xml(&self, x: &mut XmlNode) {
        x.add_parstr("name", &self.name);
        x.add_parstr("comment", &self.comment);
        x.add_parbool("invert_up_down", self.invertupdown);
        x.add_par("invert_up_down_center", self.invertupdowncenter as i32);
        x.add_parbool("enabled", self.enabled);
        x.add_par("global_fine_detune", self.globalfinedetune as i32);
        x.add_par("a_note", self.anote as i32);
        x.add_parreal("a_freq", self.afreq);
        if !self.enabled {
            return;
        }
        let s = x.child("SCALE");
        s.add_par("scale_shift", self.scaleshift as i32);
        s.add_par("first_key", self.firstkey as i32);
        s.add_par("last_key", self.lastkey as i32);
        s.add_par("middle_note", self.middlenote as i32);
        let o = s.child("OCTAVE");
        o.add_par("octave_size", self.octavesize as i32);
        for i in 0..self.octavesize {
            let d = o.child_id("DEGREE", i);
            let g = self.octave[i];
            if g.kind == 1 {
                d.add_parreal("cents", 2f32.powf(g.tuning_log2));
            }
            if g.kind == 2 {
                d.add_par("numerator", g.x1);
                d.add_par("denominator", g.x2);
            }
        }
        let k = s.child("KEYBOARD_MAPPING");
        k.add_par("map_size", self.mapsize as i32);
        k.add_par("mapping_enabled", self.mappingenabled as i32);
        for i in 0..self.mapsize {
            k.child_id("KEYMAP", i).add_par("degree", self.mapping[i] as i32);
        }
    }

    pub fn get_from_xml(&mut self, x: &XmlNode) {
        if let Some(n) = x.parstr("name") {
            self.name = n;
        }
        if let Some(c) = x.parstr("comment") {
            self.comment = c;
        }
        self.invertupdown = x.parbool("invert_up_down", self.invertupdown);
        self.invertupdowncenter = x.par127("invert_up_down_center", self.invertupdowncenter);
        self.enabled = x.parbool("enabled", self.enabled);
        self.globalfinedetune = x.par127("global_fine_detune", self.globalfinedetune);
        self.anote = x.par127("a_note", self.anote);
        self.afreq = x.parreal_lim("a_freq", self.afreq, 1.0, 10000.0);
        if let Some(s) = x.branch("SCALE") {
            self.scaleshift = s.par127("scale_shift", self.scaleshift);
            self.firstkey = s.par127("first_key", self.firstkey);
            self.lastkey = s.par127("last_key", self.lastkey);
            self.middlenote = s.par127("middle_note", self.middlenote);
            if let Some(o) = s.branch("OCTAVE") {
                self.octavesize = (o.par127("octave_size", self.octavesize as u8) as usize).clamp(1, MAX_OCTAVE_SIZE);
                for i in 0..self.octavesize {
                    let Some(d) = o.branch_id("DEGREE", i) else { continue };
                    let g = &mut self.octave[i];
                    g.x2 = 0;
                    g.tuning_log2 = d.parreal("cents", 2f32.powf(g.tuning_log2)).log2();
                    g.x1 = d.par("numerator", g.x1, 0, 65535);
                    g.x2 = d.par("denominator", g.x2, 0, 65535);
                    if g.x2 != 0 {
                        g.kind = 2;
                        g.tuning_log2 = (g.x1 as f32 / g.x2 as f32).log2();
                    } else {
                        g.kind = 1;
                        let c = g.tuning_log2 * 1200.0;
                        g.x1 = c.floor() as i32;
                        g.x2 = ((c - g.x1 as f32) * 1.0e6).floor() as i32;
                    }
                }
            }
            if let Some(k) = s.branch("KEYBOARD_MAPPING") {
                self.mapsize = (k.par127("map_size", self.mapsize as u8) as usize).clamp(1, 128);
                self.mappingenabled = k.par127("mapping_enabled", self.mappingenabled as u8) != 0;
                for i in 0..self.mapsize {
                    let Some(m) = k.branch_id("KEYMAP", i) else { continue };
                    self.mapping[i] = m.par("degree", self.mapping[i] as i32, -1, 127) as i16;
                }
            }
        }
    }
}

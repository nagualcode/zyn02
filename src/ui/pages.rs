//! Editor pages. Each page works on a clone of the part / engine
//! parameters and commits through `Shared::edit` when a widget reports a
//! change (the audio thread then picks the new patch up).

use std::sync::Arc;

use egui::{Color32, Ui, vec2};

use super::w::theme::*;
use super::w::*;
use super::{UiState, with_fft};
use crate::state::Shared;
use crate::synth::ad_params::{AdParams, AdVoice, FmType};
use crate::synth::effects::{EFFECT_NAMES, EffectCfg, preset_names};
use crate::synth::master::{NUM_MIDI_PARTS, Patch};
use crate::synth::oscilgen::{BASE_FUNCTION_NAMES, FILTER_NAMES, OscilGen, WAVESHAPE_NAMES};
use crate::synth::pad::PadParams;
use crate::synth::part::PartParams;
use crate::synth::sub::SubParams;
use crate::synth::util::*;

fn row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        add(ui);
    });
}

// ---------------------------------------------------------------------------
// Part page
// ---------------------------------------------------------------------------

pub fn part_page(ui: &mut Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState) {
    let pi = st.part;
    let mut pp = (*patch.parts[pi]).clone();
    let mut ch = false;
    let mut kit_changed = false;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            panel(ui, "PART", FX, |ui| {
                row(ui, |ui| {
                    ch |= toggle(ui, "ENABLED", &mut pp.enabled, FX);
                    ch |= toggle(ui, "NOTE ON", &mut pp.noteon, FX);
                    let mut mode = if pp.polymode { 0 } else if pp.legatomode { 2 } else { 1 };
                    if choice(ui, "pmode", "Mode", &mut mode, &["Poly", "Mono", "Legato"], 70.0) {
                        pp.polymode = mode == 0;
                        pp.legatomode = mode == 2;
                        ch = true;
                    }
                });
                row(ui, |ui| {
                    ch |= knobf(ui, "Volume", &mut pp.volume, -40.0, 13.333, 0.0, &|v| format!("{v:+.1}dB"), FX);
                    ch |= knob(ui, "Pan", &mut pp.panning, 64, FX);
                    ch |= knob(ui, "Vel.Sns", &mut pp.velsns, 64, FX);
                    ch |= knob(ui, "Vel.Ofs", &mut pp.veloffs, 64, FX);
                    let mut ks = pp.keyshift as i32 - 64;
                    if knob_i(ui, "Transp.", &mut ks, -64, 63, 0, &|v| format!("{v:+}"), FX) {
                        pp.keyshift = (ks + 64) as u8;
                        ch = true;
                    }
                });
                row(ui, |ui| {
                    ch |= knob(ui, "Min Key", &mut pp.minkey, 0, FX);
                    ch |= knob(ui, "Max Key", &mut pp.maxkey, 127, FX);
                    ch |= knob_range(ui, "Key Lim", &mut pp.keylimit, 0, 60, 15, FX);
                    ch |= knob_range(ui, "Voice Lim", &mut pp.voicelimit, 0, 60, 0, FX);
                    let mut c = pp.rcvchn + 1;
                    if knob_range(ui, "MIDI Ch", &mut c, 1, 16, 1, FX) {
                        pp.rcvchn = c - 1;
                        ch = true;
                    }
                });
            });
            panel(ui, "CONTROLLERS", FX, |ui| {
                let c = &mut pp.ctl;
                row(ui, |ui| {
                    let mut b = c.bendrange;
                    if knob_i(ui, "Bend", &mut b, -6400, 6400, 200, &|v| format!("{:+.1}st", v as f32 / 100.0), FX) {
                        c.bendrange = b;
                        ch = true;
                    }
                    ch |= knob(ui, "Mod Dep", &mut c.modwheel_depth, 80, FX);
                    ch |= knob(ui, "Cut Dep", &mut c.filtercutoff_depth, 64, FX);
                    ch |= knob(ui, "Q Dep", &mut c.filterq_depth, 64, FX);
                    ch |= knob(ui, "BW Dep", &mut c.bandwidth_depth, 64, FX);
                    ch |= knob(ui, "Pan Dep", &mut c.panning_depth, 64, FX);
                });
                row(ui, |ui| {
                    ch |= toggle(ui, "PORTA", &mut c.portamento_portamento, FX);
                    ch |= toggle(ui, "AUTO", &mut c.portamento_auto, FX);
                    ch |= toggle(ui, "PROP", &mut c.portamento_proportional, FX);
                    ch |= toggle(ui, "SUSTAIN RX", &mut c.sustain_receive, FX);
                    ch |= toggle(ui, "EXP RX", &mut c.expression_receive, FX);
                });
                row(ui, |ui| {
                    ch |= knob(ui, "P.Time", &mut c.portamento_time, 64, FX);
                    ch |= knob(ui, "P.Up/Dn", &mut c.portamento_updowntimestretch, 64, FX);
                    ch |= knob(ui, "P.Thresh", &mut c.portamento_pitchthresh, 3, FX);
                    ch |= knob(ui, "P.Rate", &mut c.portamento_proprate, 80, FX);
                    ch |= knob(ui, "P.Depth", &mut c.portamento_propdepth, 90, FX);
                });
            });
        });
        ui.vertical(|ui| {
            panel(ui, "INSTRUMENT", FX, |ui| {
                ui.horizontal(|ui| {
                    small_label(ui, "Name");
                    let mut name = pp.name.clone();
                    if ui.add(egui::TextEdit::singleline(&mut name).desired_width(200.0)).changed() {
                        pp.name = name;
                        ch = true;
                    }
                });
                ui.horizontal(|ui| {
                    small_label(ui, "Author");
                    let mut a = pp.author.clone();
                    if ui.add(egui::TextEdit::singleline(&mut a).desired_width(200.0)).changed() {
                        pp.author = a;
                        ch = true;
                    }
                });
                if !pp.comments.is_empty() {
                    ui.label(egui::RichText::new(&pp.comments).size(9.5).color(DIM));
                }
                row(ui, |ui| {
                    ch |= choice(ui, "kitmode", "Kit mode", &mut pp.kitmode, &["Off", "Multi", "Single"], 70.0);
                    ch |= toggle(ui, "DRUM", &mut pp.drummode, FX);
                });
            });
            panel(ui, "KIT", FX, |ui| {
                egui::Grid::new("kitgrid").spacing(vec2(4.0, 3.0)).show(ui, |ui| {
                    for h in ["#", "On", "Name", "ADD", "SUB", "PAD", "Mute", "Min", "Max", "FX", ""] {
                        small_label(ui, h);
                    }
                    ui.end_row();
                    let n = if pp.kitmode == 0 { 1 } else { NUM_KIT_ITEMS };
                    for i in 0..n {
                        let it = &mut pp.kit[i];
                        ui.label(egui::RichText::new(format!("{}", i + 1)).size(10.0));
                        let mut en = it.enabled;
                        if i == 0 {
                            ui.label("");
                        } else if toggle(ui, "on", &mut en, FX) {
                            it.set_enabled(en, crate::state::OSCILSIZE);
                            kit_changed = true;
                        }
                        if it.enabled {
                            let mut nm = it.name.clone();
                            if ui.add(egui::TextEdit::singleline(&mut nm).desired_width(90.0)).changed() {
                                it.name = nm;
                                ch = true;
                            }
                            kit_changed |= toggle(ui, "A", &mut it.ad_enabled, ADD);
                            kit_changed |= toggle(ui, "S", &mut it.sub_enabled, SUB);
                            let was = it.pad_enabled;
                            kit_changed |= toggle(ui, "P", &mut it.pad_enabled, PAD);
                            if it.pad_enabled && !was && it.pad.as_ref().map(|p| p.samples.is_empty()).unwrap_or(false) {
                                st.pad_rebuild = Some((pi, i));
                            }
                            ch |= toggle(ui, "M", &mut it.muted, FX);
                            let mut mn = it.minkey as i32;
                            if ui.add(egui::DragValue::new(&mut mn).range(0..=127)).changed() {
                                it.minkey = mn as u8;
                                ch = true;
                            }
                            let mut mx = it.maxkey as i32;
                            if ui.add(egui::DragValue::new(&mut mx).range(0..=127)).changed() {
                                it.maxkey = mx as u8;
                                ch = true;
                            }
                            let mut s = it.sendto as i32;
                            if ui.add(egui::DragValue::new(&mut s).range(0..=3)).changed() {
                                it.sendto = s as u8;
                                ch = true;
                            }
                            if button(ui, "EDIT", st.kit == i, FX, vec2(34.0, 18.0)).clicked() {
                                st.kit = i;
                            }
                        }
                        ui.end_row();
                    }
                });
            });
        });
    });
    if ch || kit_changed {
        sh.edit(|p| p.parts[pi] = Arc::new(pp));
    }
}

// ---------------------------------------------------------------------------
// Oscillator
// ---------------------------------------------------------------------------

const MAG_TYPES: [&str; 5] = ["Linear", "-40dB", "-60dB", "-80dB", "-100dB"];
const SA_TYPES: [&str; 4] = ["None", "Pow", "ThrsD", "ThrsU"];
const MOD_TYPES: [&str; 4] = ["None", "Rev", "Sine", "Power"];
const BMOD_TYPES: [&str; 5] = ["None", "Rev", "Sine", "Power", "Chop"];
const ADAPT_TYPES: [&str; 9] = ["Off", "On", "Sq", "2xSub", "2xAdd", "3xSub", "3xAdd", "4xSub", "4xAdd"];

/// Full oscillator designer. Returns true when the spectrum must be rebuilt.
pub fn oscil(ui: &mut Ui, id: &str, o: &mut OscilGen, color: Color32, show_rand: bool) -> bool {
    let mut ch = false;
    let os = o.oscilsize();
    let mut wave = vec![0.0f32; os];
    with_fft(|fft| {
        let mut f = o.freqs.clone();
        f.iter_mut().for_each(|c| {
            let t = *c;
            *c = t;
        });
        fft.freqs2smps(&f, &mut wave);
    });
    let spec = o.spectrum(129);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                wave_plot(ui, &wave, vec2(200.0, 80.0), color);
                spectrum_plot(ui, &spec[1..], vec2(200.0, 80.0), color);
            });
            small_label(ui, "Harmonics - magnitude (drag, right-click resets)");
            ch |= bars(ui, (id, "mag"), &mut o.p.hmag[..64], 64, vec2(404.0, 70.0), color);
            small_label(ui, "Harmonics - phase");
            ch |= bars(ui, (id, "ph"), &mut o.p.hphase[..64], 64, vec2(404.0, 40.0), color.gamma_multiply(0.6));
        });
        ui.vertical(|ui| {
            let p = &mut o.p;
            row(ui, |ui| {
                let mut bf = if p.basefunc == 127 { 17 } else { p.basefunc.min(16) };
                let mut names: Vec<&str> = BASE_FUNCTION_NAMES.to_vec();
                names.push("User");
                if choice(ui, (id, "bf"), "Base", &mut bf, &names, 90.0) {
                    p.basefunc = if bf == 17 { 127 } else { bf };
                    ch = true;
                }
                ch |= knob(ui, "Par", &mut p.basefuncpar, 64, color);
                ch |= choice(ui, (id, "mt"), "Mag", &mut p.hmagtype, &MAG_TYPES, 60.0);
            });
            row(ui, |ui| {
                ch |= choice(ui, (id, "bm"), "B.Mod", &mut p.basefuncmodulation, &BMOD_TYPES, 60.0);
                ch |= knob(ui, "P1", &mut p.basefuncmodulationpar1, 64, color);
                ch |= knob(ui, "P2", &mut p.basefuncmodulationpar2, 64, color);
                ch |= knob(ui, "P3", &mut p.basefuncmodulationpar3, 32, color);
            });
            row(ui, |ui| {
                let mut ws = p.waveshapingfunction.min(17);
                if choice(ui, (id, "ws"), "Shaper", &mut ws, &WAVESHAPE_NAMES, 60.0) {
                    p.waveshapingfunction = ws;
                    ch = true;
                }
                ch |= knob(ui, "Drive", &mut p.waveshaping, 64, color);
                let mut ft = p.filtertype.min(14);
                if choice(ui, (id, "ft"), "Filter", &mut ft, &FILTER_NAMES, 60.0) {
                    p.filtertype = ft;
                    ch = true;
                }
                ch |= knob(ui, "F.1", &mut p.filterpar1, 64, color);
                ch |= knob(ui, "F.2", &mut p.filterpar2, 64, color);
                ch |= toggle(ui, "PRE", &mut p.filterbeforews, color);
            });
            row(ui, |ui| {
                ch |= choice(ui, (id, "sa"), "Sp.Adj", &mut p.satype, &SA_TYPES, 60.0);
                ch |= knob(ui, "SA.Par", &mut p.sapar, 64, color);
                ch |= choice(ui, (id, "mo"), "Mod", &mut p.modulation, &MOD_TYPES, 60.0);
                ch |= knob(ui, "M1", &mut p.modulationpar1, 64, color);
                ch |= knob(ui, "M2", &mut p.modulationpar2, 64, color);
                ch |= knob(ui, "M3", &mut p.modulationpar3, 32, color);
            });
            row(ui, |ui| {
                ch |= knob_i(ui, "H.Shift", &mut p.harmonicshift, -64, 64, 0, &|v| format!("{v:+}"), color);
                ch |= toggle(ui, "FIRST", &mut p.harmonicshiftfirst, color);
                let mut ad = p.adaptiveharmonics.min(8);
                if choice(ui, (id, "ah"), "Adaptive", &mut ad, &ADAPT_TYPES, 60.0) {
                    p.adaptiveharmonics = ad;
                    ch = true;
                }
                let mut bf = p.adaptiveharmonicsbasefreq;
                if knob_range(ui, "A.Base", &mut bf, 0, 255, 128, color) {
                    p.adaptiveharmonicsbasefreq = bf;
                    ch = true;
                }
                ch |= knob_range(ui, "A.Pow", &mut p.adaptiveharmonicspower, 0, 200, 100, color);
                ch |= knob_range(ui, "A.Par", &mut p.adaptiveharmonicspar, 0, 100, 50, color);
            });
            if show_rand {
                row(ui, |ui| {
                    ch |= knob(ui, "Rand", &mut p.rand, 64, color);
                    ch |= choice(ui, (id, "ar"), "Amp.Rnd", &mut p.amprandtype, &["None", "Pow", "Sin"], 60.0);
                    ch |= knob(ui, "AR.Pow", &mut p.amprandpower, 64, color);
                });
            }
        });
    });
    if ch {
        with_fft(|fft| o.prepare(fft));
    }
    ch
}

// ---------------------------------------------------------------------------
// ADDsynth
// ---------------------------------------------------------------------------

fn freq_controls(ui: &mut Ui, detune: &mut u16, coarse: &mut u16, dtype: &mut u8, id: &str, color: Color32, allow_global: bool) -> bool {
    let mut ch = false;
    row(ui, |ui| {
        let mut d = *detune as i32 - 8192;
        if knob_i(ui, "Detune", &mut d, -8192, 8191, 0, &|v| format!("{:+.0}", v as f32 / 81.92), color) {
            *detune = (d + 8192) as u16;
            ch = true;
        }
        let mut oct = (*coarse / 1024) as i32;
        if oct >= 8 {
            oct -= 16;
        }
        let mut cd = (*coarse % 1024) as i32;
        if cd > 512 {
            cd -= 1024;
        }
        let mut o2 = oct;
        if knob_i(ui, "Octave", &mut o2, -8, 7, 0, &|v| format!("{v:+}"), color) {
            oct = o2;
            ch = true;
        }
        let mut c2 = cd;
        if knob_i(ui, "Coarse", &mut c2, -64, 63, 0, &|v| format!("{v:+}"), color) {
            cd = c2;
            ch = true;
        }
        if ch {
            let o = if oct < 0 { oct + 16 } else { oct } as u16;
            let c = if cd < 0 { cd + 1024 } else { cd } as u16;
            *coarse = o * 1024 + c;
        }
        let opts: &[&str] = if allow_global { &["Global", "L35c", "L10c", "E100c", "E1200c"] } else { &["-", "L35c", "L10c", "E100c", "E1200c"] };
        ch |= choice(ui, (id, "dt"), "Det.Type", dtype, opts, 60.0);
    });
    ch
}

pub fn add_page(ui: &mut Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState) {
    let (pi, ki) = (st.part, st.kit);
    let Some(src) = patch.parts[pi].kit.get(ki).and_then(|k| k.ad.as_ref()) else {
        ui.label("This kit item has no ADDsynth parameters.");
        return;
    };
    let mut ad: AdParams = (**src).clone();
    let mut ch = false;
    ui.horizontal(|ui| {
        let lbl = if st.voice == usize::MAX { "GLOBAL" } else { "" };
        let _ = lbl;
        if button(ui, "GLOBAL", st.voice == usize::MAX, ADD, vec2(60.0, 20.0)).clicked() {
            st.voice = usize::MAX;
        }
        for v in 0..NUM_VOICES {
            let on = ad.voices[v].enabled;
            let txt = format!("{}{}", v + 1, if on { "*" } else { "" });
            if button(ui, &txt, st.voice == v, ADD, vec2(44.0, 20.0)).clicked() {
                st.voice = v;
            }
        }
    });
    ui.add_space(4.0);
    if st.voice == usize::MAX {
        let g = &mut ad.global;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                panel(ui, "AMPLITUDE", ADD, |ui| {
                    row(ui, |ui| {
                        ch |= knobf(ui, "Volume", &mut g.volume, -60.0, 20.0, 0.0, &|v| format!("{v:+.1}dB"), ADD);
                        ch |= knob(ui, "Pan", &mut g.panning, 64, ADD);
                        ch |= knob(ui, "Vel.Sns", &mut g.amp_velocity_scale_function, 64, ADD);
                        ch |= knob(ui, "Punch", &mut g.punch_strength, 0, ADD);
                        ch |= knob(ui, "P.Time", &mut g.punch_time, 60, ADD);
                        ch |= knob(ui, "P.Strch", &mut g.punch_stretch, 64, ADD);
                        ch |= knob(ui, "P.Vel", &mut g.punch_velocity_sensing, 72, ADD);
                    });
                    row(ui, |ui| {
                        ch |= toggle(ui, "STEREO", &mut g.stereo, ADD);
                        ch |= toggle(ui, "RND GROUP", &mut g.hrandgrouping, ADD);
                        ch |= knob(ui, "Fade-in", &mut g.fadein_adjustment, 20, ADD);
                    });
                    small_label(ui, "Envelope");
                    ch |= envelope(ui, "gae", &mut g.amp_envelope, ADD);
                    small_label(ui, "LFO");
                    ch |= lfo(ui, "gal", &mut g.amp_lfo, ADD);
                });
            });
            ui.vertical(|ui| {
                panel(ui, "FREQUENCY", ADD, |ui| {
                    ch |= freq_controls(ui, &mut g.detune, &mut g.coarse_detune, &mut g.detune_type, "gf", ADD, false);
                    row(ui, |ui| {
                        ch |= knob(ui, "Rel.BW", &mut g.bandwidth, 64, ADD);
                    });
                    small_label(ui, "Envelope");
                    ch |= envelope(ui, "gfe", &mut g.freq_envelope, ADD);
                    small_label(ui, "LFO");
                    ch |= lfo(ui, "gfl", &mut g.freq_lfo, ADD);
                });
            });
            ui.vertical(|ui| {
                panel(ui, "FILTER", ADD, |ui| {
                    ch |= filter(ui, "gflt", &mut g.filter, ADD);
                    row(ui, |ui| {
                        ch |= knob(ui, "V.Amt", &mut g.filter_velocity_scale, 0, ADD);
                        ch |= knob(ui, "V.Func", &mut g.filter_velocity_scale_function, 64, ADD);
                    });
                    small_label(ui, "Envelope");
                    ch |= envelope(ui, "gfle", &mut g.filter_envelope, ADD);
                    small_label(ui, "LFO");
                    ch |= lfo(ui, "gfll", &mut g.filter_lfo, ADD);
                });
                panel(ui, "RESONANCE", ADD, |ui| {
                    let r = &mut g.reson;
                    row(ui, |ui| {
                        ch |= toggle(ui, "ON", &mut r.enabled, ADD);
                        ch |= toggle(ui, "PROTECT 1st", &mut r.protect_fundamental, ADD);
                        ch |= knob(ui, "Max dB", &mut r.max_db, 20, ADD);
                        ch |= knob(ui, "Center", &mut r.centerfreq, 64, ADD);
                        ch |= knob(ui, "Octaves", &mut r.octavesfreq, 64, ADD);
                    });
                    ch |= bars(ui, "res", &mut r.points, 64, vec2(300.0, 50.0), ADD);
                });
            });
        });
    } else {
        let vi = st.voice;
        let nv = vi;
        let v: &mut AdVoice = &mut ad.voices[vi];
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                panel(ui, &format!("VOICE {}", vi + 1), ADD, |ui| {
                    row(ui, |ui| {
                        ch |= toggle(ui, "ENABLED", &mut v.enabled, ADD);
                        ch |= choice(ui, "vtype", "Type", &mut v.vtype, &["Sound", "White", "Pink", "DC"], 60.0);
                        ch |= toggle(ui, "RESON", &mut v.resonance, ADD);
                        ch |= toggle(ui, "AA", &mut v.aa_enabled, ADD);
                        let mut ext = (v.ext_oscil + 1) as u8;
                        let opts: Vec<String> = std::iter::once("Own".to_string()).chain((0..nv).map(|i| format!("V{}", i + 1))).collect();
                        let refs: Vec<&str> = opts.iter().map(|s| s.as_str()).collect();
                        if choice(ui, "extosc", "Oscil", &mut ext, &refs, 50.0) {
                            v.ext_oscil = ext as i32 - 1;
                            ch = true;
                        }
                    });
                    row(ui, |ui| {
                        ch |= knobf(ui, "Volume", &mut v.volume, -60.0, 0.0, -12.75, &|x| format!("{x:.1}dB"), ADD);
                        ch |= toggle(ui, "INV", &mut v.volume_minus, ADD);
                        ch |= knob(ui, "Pan", &mut v.panning, 64, ADD);
                        ch |= knob(ui, "Vel.Sns", &mut v.amp_velocity_scale_function, 127, ADD);
                        ch |= knob(ui, "Delay", &mut v.delay, 0, ADD);
                        ch |= knob(ui, "Phase", &mut v.oscil_phase, 64, ADD);
                    });
                    row(ui, |ui| {
                        ch |= knob_range(ui, "Unison", &mut v.unison_size, 1, 50, 1, ADD);
                        ch |= knob(ui, "U.Spread", &mut v.unison_frequency_spread, 60, ADD);
                        ch |= knob(ui, "U.Stereo", &mut v.unison_stereo_spread, 64, ADD);
                        ch |= knob(ui, "U.Vib", &mut v.unison_vibratto, 64, ADD);
                        ch |= knob(ui, "U.VSpd", &mut v.unison_vibratto_speed, 64, ADD);
                        ch |= knob(ui, "U.Phase", &mut v.unison_phase_randomness, 127, ADD);
                        ch |= knob_range(ui, "U.Invert", &mut v.unison_invert_phase, 0, 5, 0, ADD);
                    });
                    small_label(ui, "Frequency");
                    ch |= freq_controls(ui, &mut v.detune, &mut v.coarse_detune, &mut v.detune_type, "vf", ADD, true);
                    row(ui, |ui| {
                        ch |= toggle(ui, "FIXED", &mut v.fixed_freq, ADD);
                        ch |= knob(ui, "Eq.T", &mut v.fixed_freq_et, 0, ADD);
                        ch |= knob(ui, "Bend", &mut v.bend_adjust, 88, ADD);
                        ch |= knob(ui, "Offset", &mut v.offset_hz, 64, ADD);
                    });
                });
                panel(ui, "OSCILLATOR", ADD, |ui| {
                    if oscil(ui, "vosc", &mut v.oscil, ADD, true) {
                        ch = true;
                    }
                });
            });
            ui.vertical(|ui| {
                egui::ScrollArea::vertical().id_salt("vmod").max_height(560.0).show(ui, |ui| {
                    panel(ui, "MODULATOR", ADD, |ui| {
                        let mut fm = v.fm_enabled as u8;
                        row(ui, |ui| {
                            if choice(ui, "fmt", "Type", &mut fm, &["Off", "Mix", "Ring", "PM", "FM", "PWM"], 60.0) {
                                v.fm_enabled = FmType::from_i(fm as i32);
                                ch = true;
                            }
                            ch |= toggle(ui, "SYNC", &mut v.sync_enabled, ADD);
                            let mut src = (v.fm_voice + 1) as u8;
                            let opts: Vec<String> = std::iter::once("Osc".to_string()).chain((0..nv).map(|i| format!("V{}", i + 1))).collect();
                            let refs: Vec<&str> = opts.iter().map(|s| s.as_str()).collect();
                            if choice(ui, "fmsrc", "Source", &mut src, &refs, 50.0) {
                                v.fm_voice = src as i32 - 1;
                                ch = true;
                            }
                        });
                        row(ui, |ui| {
                            ch |= knobf(ui, "Volume", &mut v.fm_volume, 0.0, 100.0, 70.0, &|x| format!("{x:.0}"), ADD);
                            ch |= knob(ui, "Damp", &mut v.fm_volume_damp, 64, ADD);
                            ch |= knob(ui, "Vel.Sns", &mut v.fm_velocity_scale_function, 64, ADD);
                            ch |= knob(ui, "Phase", &mut v.fm_oscil_phase, 64, ADD);
                            ch |= toggle(ui, "FIXED", &mut v.fm_fixed_freq, ADD);
                        });
                        ch |= freq_controls(ui, &mut v.fm_detune, &mut v.fm_coarse_detune, &mut v.fm_detune_type, "fmf", ADD, true);
                        row(ui, |ui| {
                            ch |= toggle(ui, "AMP ENV", &mut v.fm_amp_envelope_enabled, ADD);
                            ch |= toggle(ui, "FREQ ENV", &mut v.fm_freq_envelope_enabled, ADD);
                        });
                        if v.fm_amp_envelope_enabled {
                            ch |= envelope(ui, "fmae", &mut v.fm_amp_envelope, ADD);
                        }
                        if v.fm_freq_envelope_enabled {
                            ch |= envelope(ui, "fmfe", &mut v.fm_freq_envelope, ADD);
                        }
                        if v.fm_enabled != FmType::None && v.fm_voice < 0 {
                            small_label(ui, "Modulator oscillator");
                            ch |= oscil(ui, "fmosc", &mut v.fm_oscil, ADD.gamma_multiply(0.7), true);
                        }
                    });
                    panel(ui, "VOICE AMP / FREQ / FILTER", ADD, |ui| {
                        row(ui, |ui| {
                            ch |= toggle(ui, "AMP ENV", &mut v.amp_envelope_enabled, ADD);
                            ch |= toggle(ui, "AMP LFO", &mut v.amp_lfo_enabled, ADD);
                            ch |= toggle(ui, "FREQ ENV", &mut v.freq_envelope_enabled, ADD);
                            ch |= toggle(ui, "FREQ LFO", &mut v.freq_lfo_enabled, ADD);
                        });
                        if v.amp_envelope_enabled {
                            small_label(ui, "Amp envelope");
                            ch |= envelope(ui, "vae", &mut v.amp_envelope, ADD);
                        }
                        if v.amp_lfo_enabled {
                            small_label(ui, "Amp LFO");
                            ch |= lfo(ui, "val", &mut v.amp_lfo, ADD);
                        }
                        if v.freq_envelope_enabled {
                            small_label(ui, "Freq envelope");
                            ch |= envelope(ui, "vfe", &mut v.freq_envelope, ADD);
                        }
                        if v.freq_lfo_enabled {
                            small_label(ui, "Freq LFO");
                            ch |= lfo(ui, "vfl", &mut v.freq_lfo, ADD);
                        }
                        row(ui, |ui| {
                            ch |= toggle(ui, "FILTER", &mut v.filter_enabled, ADD);
                            ch |= toggle(ui, "BYPASS GLOBAL", &mut v.filter_bypass, ADD);
                            ch |= toggle(ui, "IGNORE CC", &mut v.filter_fcctl_bypass, ADD);
                        });
                        if v.filter_enabled {
                            ch |= filter(ui, "vflt", &mut v.filter, ADD);
                            row(ui, |ui| {
                                ch |= toggle(ui, "F.ENV", &mut v.filter_envelope_enabled, ADD);
                                ch |= toggle(ui, "F.LFO", &mut v.filter_lfo_enabled, ADD);
                                ch |= knob(ui, "V.Amt", &mut v.filter_velocity_scale, 0, ADD);
                                ch |= knob(ui, "V.Func", &mut v.filter_velocity_scale_function, 64, ADD);
                            });
                            if v.filter_envelope_enabled {
                                ch |= envelope(ui, "vfle", &mut v.filter_envelope, ADD);
                            }
                            if v.filter_lfo_enabled {
                                ch |= lfo(ui, "vfll", &mut v.filter_lfo, ADD);
                            }
                        }
                    });
                });
            });
        });
    }
    if ch {
        sh.edit(|p| {
            let part = Arc::make_mut(&mut p.parts[pi]);
            part.kit[ki].ad = Some(Arc::new(ad));
        });
    }
}

// ---------------------------------------------------------------------------
// SUBsynth
// ---------------------------------------------------------------------------

pub fn sub_page(ui: &mut Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState) {
    let (pi, ki) = (st.part, st.kit);
    let Some(src) = patch.parts[pi].kit.get(ki).and_then(|k| k.sub.as_ref()) else {
        ui.label("This kit item has no SUBsynth parameters.");
        return;
    };
    let mut s: SubParams = (**src).clone();
    let mut ch = false;
    panel(ui, "HARMONICS", SUB, |ui| {
        small_label(ui, "Magnitude");
        ch |= bars(ui, "sm", &mut s.hmag, 0, vec2(760.0, 90.0), SUB);
        small_label(ui, "Relative bandwidth");
        ch |= bars(ui, "sb", &mut s.hrelbw, 64, vec2(760.0, 50.0), SUB.gamma_multiply(0.6));
    });
    ui.horizontal_top(|ui| {
        panel(ui, "AMPLITUDE", SUB, |ui| {
            row(ui, |ui| {
                ch |= knobf(ui, "Volume", &mut s.volume, -60.0, 0.0, 0.0, &|v| format!("{v:.1}dB"), SUB);
                ch |= knob(ui, "Pan", &mut s.panning, 64, SUB);
                ch |= knobf(ui, "Vel.Sns", &mut s.amp_velocity_scale_function, 0.0, 127.0, 70.86, &|v| format!("{v:.0}"), SUB);
                ch |= toggle(ui, "STEREO", &mut s.stereo, SUB);
            });
            ch |= envelope(ui, "sae", &mut s.amp_envelope, SUB);
        });
        panel(ui, "BANDWIDTH & FREQUENCY", SUB, |ui| {
            row(ui, |ui| {
                ch |= knob(ui, "Bandw.", &mut s.bandwidth, 40, SUB);
                ch |= knob(ui, "BW.Scale", &mut s.bwscale, 64, SUB);
                ch |= knob_range(ui, "Stages", &mut s.numstages, 1, 5, 2, SUB);
                ch |= choice(ui, "smt", "Mag", &mut s.hmagtype, &MAG_TYPES, 60.0);
                ch |= choice(ui, "sst", "Start", &mut s.start, &["Zero", "Rnd", "Max"], 50.0);
            });
            ch |= freq_controls(ui, &mut s.detune, &mut s.coarse_detune, &mut s.detune_type, "sf", SUB, false);
            row(ui, |ui| {
                ch |= toggle(ui, "FIXED", &mut s.fixed_freq, SUB);
                ch |= knob(ui, "Eq.T", &mut s.fixed_freq_et, 0, SUB);
                ch |= knob(ui, "Bend", &mut s.bend_adjust, 88, SUB);
                ch |= knob(ui, "Offset", &mut s.offset_hz, 64, SUB);
            });
            row(ui, |ui| {
                let mut t = s.overtone_spread_type;
                if choice(ui, "ost", "Overtones", &mut t, &["Harmonic", "ShiftU", "ShiftL", "PowerU", "PowerL", "Sine", "Power", "Shift"], 70.0) {
                    s.overtone_spread_type = t;
                    ch = true;
                }
                let mut a = s.overtone_spread_par1;
                let mut b = s.overtone_spread_par2;
                let mut c = s.overtone_spread_par3;
                let mut o = false;
                o |= knob_range(ui, "Par1", &mut a, 0, 255, 0, SUB);
                o |= knob_range(ui, "Par2", &mut b, 0, 255, 0, SUB);
                o |= knob_range(ui, "ForceH", &mut c, 0, 255, 0, SUB);
                if o || ch {
                    s.overtone_spread_par1 = a;
                    s.overtone_spread_par2 = b;
                    s.overtone_spread_par3 = c;
                    s.update_frequency_multipliers();
                    ch = true;
                }
            });
            row(ui, |ui| {
                ch |= toggle(ui, "FREQ ENV", &mut s.freq_envelope_enabled, SUB);
                ch |= toggle(ui, "BW ENV", &mut s.bandwidth_envelope_enabled, SUB);
            });
            if s.freq_envelope_enabled {
                ch |= envelope(ui, "sfe", &mut s.freq_envelope, SUB);
            }
            if s.bandwidth_envelope_enabled {
                ch |= envelope(ui, "sbe", &mut s.bandwidth_envelope, SUB);
            }
        });
        panel(ui, "FILTER", SUB, |ui| {
            ch |= toggle(ui, "ENABLED", &mut s.global_filter_enabled, SUB);
            if s.global_filter_enabled {
                ch |= filter(ui, "sflt", &mut s.global_filter, SUB);
                row(ui, |ui| {
                    ch |= knob(ui, "V.Amt", &mut s.global_filter_velocity_scale, 0, SUB);
                    ch |= knob(ui, "V.Func", &mut s.global_filter_velocity_scale_function, 64, SUB);
                });
                ch |= envelope(ui, "sfle", &mut s.global_filter_envelope, SUB);
            }
        });
    });
    if ch {
        sh.edit(|p| {
            let part = Arc::make_mut(&mut p.parts[pi]);
            part.kit[ki].sub = Some(Arc::new(s));
        });
    }
}

// ---------------------------------------------------------------------------
// PADsynth
// ---------------------------------------------------------------------------

pub fn pad_page(ui: &mut Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState) {
    let (pi, ki) = (st.part, st.kit);
    let Some(src) = patch.parts[pi].kit.get(ki).and_then(|k| k.pad.as_ref()) else {
        ui.label("This kit item has no PADsynth parameters.");
        return;
    };
    let mut p: PadParams = (**src).clone();
    let mut ch = false;
    let mut table = false;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            panel(ui, "HARMONIC PROFILE", PAD, |ui| {
                let mut prof = vec![0.0f32; 256];
                let bw = p.getprofile(&mut prof);
                wave_plot(ui, &prof.iter().map(|v| v * 2.0 - 1.0).collect::<Vec<_>>(), vec2(300.0, 70.0), PAD);
                small_label(ui, &format!("Perceived bandwidth {:.2}", bw));
                row(ui, |ui| {
                    table |= choice(ui, "pbt", "Base", &mut p.hp_base_type, &["Gauss", "Square", "DoubleExp"], 70.0);
                    table |= knob(ui, "Width", &mut p.hp_base_par1, 80, PAD);
                    table |= knob(ui, "Mult", &mut p.hp_freqmult, 0, PAD);
                    table |= knob(ui, "Mod", &mut p.hp_mod_par1, 0, PAD);
                    table |= knob(ui, "M.Freq", &mut p.hp_mod_freq, 30, PAD);
                    table |= knob(ui, "Size", &mut p.hp_width, 127, PAD);
                });
                row(ui, |ui| {
                    table |= choice(ui, "pat", "Amp", &mut p.hp_amp_type, &["Off", "Gauss", "Sine", "Flat"], 60.0);
                    table |= choice(ui, "pam", "A.Mode", &mut p.hp_amp_mode, &["Sum", "Mult", "Div1", "Div2"], 60.0);
                    table |= knob(ui, "A.Par1", &mut p.hp_amp_par1, 80, PAD);
                    table |= knob(ui, "A.Par2", &mut p.hp_amp_par2, 64, PAD);
                    table |= toggle(ui, "AUTOSCALE", &mut p.hp_autoscale, PAD);
                    table |= choice(ui, "poh", "Half", &mut p.hp_onehalf, &["Full", "Upper", "Lower"], 60.0);
                });
            });
            panel(ui, "SPECTRUM", PAD, |ui| {
                row(ui, |ui| {
                    table |= choice(ui, "pmode", "Mode", &mut p.mode, &["Bandwidth", "Discrete", "Continuous"], 80.0);
                    let mut bw = p.bandwidth;
                    if knob_i(ui, "Bandw.", &mut bw, 0, 1000, 500, &|v| format!("{v}"), PAD) {
                        p.bandwidth = bw;
                        table = true;
                    }
                    table |= choice(ui, "pbws", "BW Scale", &mut p.bwscale, &["Normal", "Equal", "Q.Down", "Q.Up", "H.Down", "Up", "2xUp", "Down"], 60.0);
                });
                row(ui, |ui| {
                    table |= choice(ui, "phr", "Overtones", &mut p.hrpos_type, &["Harmonic", "ShiftU", "ShiftL", "PowerU", "PowerL", "Sine", "Power", "Shift"], 70.0);
                    table |= knob_range(ui, "Par1", &mut p.hrpos_par1, 0, 255, 0, PAD);
                    table |= knob_range(ui, "Par2", &mut p.hrpos_par2, 0, 255, 0, PAD);
                    table |= knob_range(ui, "ForceH", &mut p.hrpos_par3, 0, 255, 0, PAD);
                });
                row(ui, |ui| {
                    table |= choice(ui, "pqs", "Sample", &mut p.q_samplesize, &["16k", "32k", "64k", "128k", "256k", "512k", "1M"], 55.0);
                    table |= choice(ui, "pqb", "Base", &mut p.q_basenote, &["C-2", "G-2", "C-3", "G-3", "C-4", "G-4", "C-5", "G-5", "G-6"], 55.0);
                    table |= choice(ui, "pqo", "Octaves", &mut p.q_oct, &["1", "2", "3", "4", "5", "6", "7", "8"], 45.0);
                    table |= choice(ui, "pqp", "Smp/Oct", &mut p.q_smpoct, &["0.5", "1", "2", "3", "4", "6", "12"], 45.0);
                });
                small_label(ui, &format!("{} wavetables{}", p.samples.len(), if sh.busy.load(std::sync::atomic::Ordering::Relaxed) > 0 { " - building..." } else { "" }));
            });
            panel(ui, "OSCILLATOR", PAD, |ui| {
                if oscil(ui, "posc", &mut p.oscil, PAD, false) {
                    table = true;
                }
            });
        });
        ui.vertical(|ui| {
            egui::ScrollArea::vertical().id_salt("padr").max_height(560.0).show(ui, |ui| {
                panel(ui, "AMPLITUDE", PAD, |ui| {
                    row(ui, |ui| {
                        ch |= knob(ui, "Volume", &mut p.volume, 90, PAD);
                        ch |= knob(ui, "Pan", &mut p.panning, 64, PAD);
                        ch |= knob(ui, "Vel.Sns", &mut p.amp_velocity_scale_function, 64, PAD);
                        ch |= knob(ui, "Punch", &mut p.punch_strength, 0, PAD);
                        ch |= knob(ui, "P.Time", &mut p.punch_time, 60, PAD);
                        ch |= toggle(ui, "STEREO", &mut p.stereo, PAD);
                    });
                    ch |= envelope(ui, "pae", &mut p.amp_envelope, PAD);
                    ch |= lfo(ui, "pal", &mut p.amp_lfo, PAD);
                });
                panel(ui, "FREQUENCY", PAD, |ui| {
                    ch |= freq_controls(ui, &mut p.detune, &mut p.coarse_detune, &mut p.detune_type, "pf", PAD, false);
                    row(ui, |ui| {
                        ch |= toggle(ui, "FIXED", &mut p.fixed_freq, PAD);
                        ch |= knob(ui, "Eq.T", &mut p.fixed_freq_et, 0, PAD);
                        ch |= knob(ui, "Bend", &mut p.bend_adjust, 88, PAD);
                        ch |= knob(ui, "Offset", &mut p.offset_hz, 64, PAD);
                    });
                    ch |= envelope(ui, "pfe", &mut p.freq_envelope, PAD);
                    ch |= lfo(ui, "pfl", &mut p.freq_lfo, PAD);
                });
                panel(ui, "FILTER", PAD, |ui| {
                    ch |= filter(ui, "pflt", &mut p.filter, PAD);
                    row(ui, |ui| {
                        ch |= knob(ui, "V.Amt", &mut p.filter_velocity_scale, 0, PAD);
                        ch |= knob(ui, "V.Func", &mut p.filter_velocity_scale_function, 64, PAD);
                    });
                    ch |= envelope(ui, "pfle", &mut p.filter_envelope, PAD);
                    ch |= lfo(ui, "pfll", &mut p.filter_lfo, PAD);
                });
            });
        });
    });
    if ch || table {
        sh.edit(|q| {
            let part = Arc::make_mut(&mut q.parts[pi]);
            part.kit[ki].pad = Some(Arc::new(p));
        });
        if table {
            st.pad_rebuild = Some((pi, ki));
        }
    }
}

// ---------------------------------------------------------------------------
// Effects
// ---------------------------------------------------------------------------

fn effect_param_names(kind: u8) -> Vec<(usize, &'static str)> {
    match kind {
        1 => vec![(0, "Amount"), (1, "Pan"), (2, "Time"), (3, "I.Delay"), (4, "I.Del.Fb"), (7, "LPF"), (8, "HPF"), (9, "Damp"), (11, "R.Size"), (12, "Bandw.")],
        2 => vec![(0, "Amount"), (1, "Pan"), (2, "Delay"), (3, "LR Delay"), (4, "LR Cross"), (5, "Feedb."), (6, "Damp")],
        3 => vec![(0, "Amount"), (1, "Pan"), (2, "Freq"), (3, "Rnd"), (5, "St.df"), (6, "Depth"), (7, "Delay"), (8, "Feedb."), (9, "LR Cross")],
        4 => vec![(0, "Amount"), (1, "Pan"), (2, "Freq"), (3, "Rnd"), (5, "St.df"), (6, "Depth"), (7, "Feedb."), (8, "Stages"), (9, "LR/Offs"), (11, "Phase"), (13, "Dist")],
        5 => vec![(0, "Amount"), (1, "Pan"), (2, "Freq"), (3, "Rnd"), (5, "St.df"), (6, "Depth"), (7, "Feedb."), (8, "Delay"), (9, "LR Cross"), (10, "Phase")],
        6 => vec![(0, "Amount"), (1, "Pan"), (2, "LR Cross"), (3, "Drive"), (4, "Level"), (7, "LPF"), (8, "HPF"), (11, "Shape"), (12, "Offset")],
        7 => vec![(0, "Gain")],
        8 => vec![(0, "Amount"), (1, "Pan"), (2, "Freq"), (3, "Rnd"), (5, "St.df"), (6, "LFO Dep"), (7, "Amp.Sns"), (9, "Smooth")],
        9 => vec![(0, "Amount"), (1, "Pan"), (2, "Q"), (3, "Drive"), (4, "Level"), (5, "Spread"), (7, "LPF"), (8, "HPF"), (9, "Unison"), (10, "Strings"), (11, "Base")],
        10 => vec![(0, "Amount"), (1, "Pan"), (2, "Length"), (4, "Phase"), (5, "X-Fade")],
        _ => vec![],
    }
}

/// One effect slot editor. Returns true on change.
fn effect_slot(ui: &mut Ui, id: &str, cfg: &mut EffectCfg, insertion: bool) -> bool {
    let mut ch = false;
    row(ui, |ui| {
        let mut k = cfg.kind.min(10);
        if choice(ui, (id, "kind"), "Effect", &mut k, &EFFECT_NAMES, 80.0) {
            cfg.set_kind(k, insertion);
            ch = true;
        }
        let names = preset_names(cfg.kind);
        if !names.is_empty() {
            let mut pr = cfg.preset.min(names.len() as u8 - 1);
            if choice(ui, (id, "pre"), "Preset", &mut pr, names, 90.0) {
                cfg.set_preset(pr, insertion);
                ch = true;
            }
        }
    });
    if cfg.kind == 0 {
        return ch;
    }
    let mut sets = cfg.settings;
    let mut pc = false;
    row(ui, |ui| {
        for (n, name) in effect_param_names(cfg.kind) {
            let mut v = sets[n].max(0) as u8;
            let d = v;
            if knob(ui, name, &mut v, d, FX) {
                sets[n] = v as i16;
                pc = true;
            }
        }
    });
    row(ui, |ui| {
        let mut toggles: Vec<(usize, &str, &[&str])> = Vec::new();
        match cfg.kind {
            1 => toggles.push((10, "Type", &["Random", "Freeverb", "Bandwidth"])),
            3 => {
                toggles.push((4, "LFO", &["Sine", "Tri"]));
                toggles.push((10, "Mode", &["Chorus", "Flange", "Triple", "Dual"]));
                toggles.push((11, "Subtract", &["Off", "On"]));
            }
            4 => {
                toggles.push((4, "LFO", &["Sine", "Tri"]));
                toggles.push((10, "Subtract", &["Off", "On"]));
                toggles.push((12, "Hyper", &["Off", "On"]));
                toggles.push((14, "Analog", &["Off", "On"]));
            }
            5 | 8 => toggles.push((4, "LFO", &["Sine", "Tri"])),
            9 => toggles.push((6, "Negate", &["Off", "On"])),
            10 => {
                toggles.push((3, "Stereo", &["Off", "On"]));
                toggles.push((6, "Sync", &["Note On", "On/Off", "Auto", "Host"]));
            }
            6 => {
                toggles.push((5, "Type", &WAVESHAPE_NAMES[1..]));
                toggles.push((6, "Negate", &["Off", "On"]));
                toggles.push((9, "Stereo", &["Off", "On"]));
                toggles.push((10, "PreFilt", &["Off", "On"]));
            }
            _ => {}
        }
        if cfg.kind == 8 {
            toggles.push((8, "Inv.Amp", &["Off", "On"]));
        }
        for (n, name, opts) in toggles {
            let mut v = (sets[n].max(0) as u8).min(opts.len() as u8 - 1);
            if choice(ui, (id, n), name, &mut v, opts, 60.0) {
                sets[n] = v as i16;
                pc = true;
            }
        }
    });
    if cfg.kind == 7 {
        for b in 0..MAX_EQ_BANDS {
            row(ui, |ui| {
                let base = 10 + b * 5;
                let mut t = sets[base].max(0) as u8;
                if choice(ui, (id, "eqt", b), &format!("Band {}", b + 1), &mut t, &["Off", "LP1", "HP1", "LP2", "HP2", "BP2", "N2", "Pk", "LSh", "HSh"], 50.0) {
                    sets[base] = t as i16;
                    pc = true;
                }
                if t != 0 {
                    for (k, nm) in [(1, "Freq"), (2, "Gain"), (3, "Q"), (4, "Stages")] {
                        let mut v = sets[base + k].max(0) as u8;
                        if knob(ui, nm, &mut v, 64, FX) {
                            sets[base + k] = v as i16;
                            pc = true;
                        }
                    }
                }
            });
        }
    }
    if cfg.kind == 8 {
        small_label(ui, "Filter");
        ch |= filter(ui, &format!("{id}flt"), &mut cfg.filter, FX);
    }
    if pc {
        cfg.settings = sets;
        ch = true;
    }
    ch
}

pub fn fx_page(ui: &mut Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState) {
    let pi = st.part;
    let mut p = patch.clone();
    let mut ch = false;
    egui::ScrollArea::vertical().id_salt("fxs").show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let pp = Arc::make_mut(&mut p.parts[pi]);
                for n in 0..NUM_PART_EFX {
                    panel(ui, &format!("PART {} - INSTRUMENT EFFECT {}", pi + 1, n + 1), FX, |ui| {
                        ch |= effect_slot(ui, &format!("pe{n}"), &mut pp.efx[n], true);
                        row(ui, |ui| {
                            ch |= choice(ui, ("route", n), "Route", &mut pp.efxroute[n], &["Next", "Out", "Dry+Out"], 70.0);
                            ch |= toggle(ui, "BYPASS", &mut pp.efxbypass[n], FX);
                        });
                    });
                }
            });
            ui.vertical(|ui| {
                for n in 0..NUM_SYS_EFX {
                    panel(ui, &format!("SYSTEM EFFECT {}", n + 1), FX, |ui| {
                        ch |= effect_slot(ui, &format!("se{n}"), &mut p.sysefx[n], false);
                        row(ui, |ui| {
                            ch |= knob(ui, &format!("Send P{}", pi + 1), &mut p.sysefxvol[n][pi], 0, FX);
                            for t in (n + 1)..NUM_SYS_EFX {
                                ch |= knob(ui, &format!(">Sys{}", t + 1), &mut p.sysefxsend[n][t], 0, FX);
                            }
                        });
                    });
                }
            });
            ui.vertical(|ui| {
                for n in 0..NUM_INS_EFX {
                    panel(ui, &format!("INSERTION EFFECT {}", n + 1), FX, |ui| {
                        let mut target = (p.insparts[n] + 2).clamp(0, NUM_MIDI_PARTS as i32 + 1) as u8;
                        let opts: Vec<String> = ["Master".to_string(), "Off".to_string()].into_iter().chain((1..=NUM_MIDI_PARTS).map(|i| format!("Part {i}"))).collect();
                        let refs: Vec<&str> = opts.iter().map(|s| s.as_str()).collect();
                        if choice(ui, ("ins", n), "Target", &mut target, &refs, 70.0) {
                            p.insparts[n] = target as i32 - 2;
                            ch = true;
                        }
                        if p.insparts[n] != -1 {
                            ch |= effect_slot(ui, &format!("ie{n}"), &mut p.insefx[n], true);
                        }
                    });
                }
            });
        });
    });
    if ch {
        sh.edit(|q| *q = p);
    }
}

// ---------------------------------------------------------------------------
// Master / tuning
// ---------------------------------------------------------------------------

pub fn master_page(ui: &mut Ui, sh: &Arc<Shared>, patch: &Patch) {
    let mut p = patch.clone();
    let mut ch = false;
    ui.horizontal_top(|ui| {
        panel(ui, "MASTER", FX, |ui| {
            row(ui, |ui| {
                ch |= knobf(ui, "Volume", &mut p.volume, -40.0, 13.33, -6.67, &|v| format!("{v:+.1}dB"), FX);
                let mut ks = p.keyshift as i32 - 64;
                if knob_i(ui, "Transp.", &mut ks, -64, 63, 0, &|v| format!("{v:+}"), FX) {
                    p.keyshift = (ks + 64) as u8;
                    ch = true;
                }
                ch |= toggle(ui, "CONST POWER PAN", &mut p.const_power, FX);
            });
        });
        panel(ui, "TUNING", FX, |ui| {
            let m = &mut p.micro;
            row(ui, |ui| {
                ch |= knobf(ui, "A Freq", &mut m.afreq, 400.0, 480.0, 440.0, &|v| format!("{v:.1}Hz"), FX);
                ch |= knob(ui, "A Note", &mut m.anote, 69, FX);
                ch |= knob(ui, "Fine", &mut m.globalfinedetune, 64, FX);
                ch |= toggle(ui, "MICROTONAL", &mut m.enabled, FX);
                ch |= toggle(ui, "INVERT", &mut m.invertupdown, FX);
            });
            ui.label(egui::RichText::new(format!("{} - {}", m.name, m.comment)).size(9.5).color(DIM));
        });
    });
    for (i, pp) in patch.parts.iter().enumerate() {
        if pp.enabled {
            let n = sh.part_notes[i].load(std::sync::atomic::Ordering::Relaxed);
            ui.label(egui::RichText::new(format!("Part {:2}: {} ({} notes, ch {})", i + 1, if pp.name.is_empty() { "-" } else { &pp.name }, n, pp.rcvchn + 1)).size(10.0).color(DIM));
        }
    }
    if ch {
        sh.edit(|q| *q = p);
    }
}

#[allow(dead_code)]
fn _unused(_: &PartParams) {}

//! Editor pages. Each page works on a clone of the part / engine
//! parameters and commits through `Shared::edit` when a widget reports a
//! change (the audio thread then picks the new patch up).
//!
//! The main area is 1124 x 466 px. Pages are laid out on a fixed grid:
//! three 370 px columns, or a 646 px oscillator column next to a 472 px
//! tabbed column, so nothing has to scroll.

use std::sync::Arc;

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::w::theme::*;
use super::w::*;
use super::{FxSel, UiState, with_fft};
use crate::state::Shared;
use crate::synth::ad_params::{AdParams, AdVoice, FmType};
use crate::synth::effects::{EFFECT_NAMES, EffectCfg, preset_names};
use crate::synth::master::{NUM_MIDI_PARTS, Patch};
use crate::synth::oscilgen::{BASE_FUNCTION_NAMES, FILTER_NAMES, OscilGen, WAVESHAPE_NAMES};
use crate::synth::pad::PadParams;
use crate::synth::sub::SubParams;
use crate::synth::util::*;

const COL3: f32 = 370.0;
const OSC_W: f32 = 646.0;
const SIDE_W: f32 = 472.0;
const FULL_W: f32 = 1124.0;

const DET_TYPES: [&str; 5] = ["L35c", "L10c", "E100c", "E1200c", "-"];

/// A panel whose first row is a tab strip; returns the selected tab.
fn tab_panel(ui: &mut Ui, title: &str, color: Color32, width: f32, height: f32, tabs: &[&str], marks: &[bool], sel: &mut usize, add: impl FnOnce(&mut Ui, usize)) {
    if *sel >= tabs.len() {
        *sel = 0;
    }
    panel_h(ui, title, color, width, Some(height), |ui| {
        let tw = ((width - 12.0) / tabs.len() as f32).min(100.0);
        if let Some(i) = segmented(ui, tabs, *sel, marks, color, tw, 18.0) {
            *sel = i;
        }
        ui.add_space(2.0);
        add(ui, *sel);
    });
}

fn freq_controls(ui: &mut Ui, detune: &mut u16, coarse: &mut u16, dtype: &mut u8, id: &str, color: Color32, allow_global: bool) -> bool {
    let mut ch = false;
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
    let mut c = false;
    c |= knob_i(ui, "Octave", &mut oct, -8, 7, 0, &|v| format!("{v:+}"), color);
    c |= knob_i(ui, "Coarse", &mut cd, -64, 63, 0, &|v| format!("{v:+}"), color);
    if c {
        let o = if oct < 0 { oct + 16 } else { oct } as u16;
        let cc = if cd < 0 { cd + 1024 } else { cd } as u16;
        *coarse = o * 1024 + cc;
        ch = true;
    }
    let opts: &[&str] = if allow_global { &["Global", "L35c", "L10c", "E100c", "E1200c"] } else { &["-", "L35c", "L10c", "E100c", "E1200c"] };
    let _ = DET_TYPES;
    ch |= choice(ui, (id, "dt"), "Det.Type", dtype, opts, 60.0);
    ch
}

fn fixed_controls(ui: &mut Ui, fixed: &mut bool, et: &mut u8, bend: &mut u8, offset: &mut u8, color: Color32) -> bool {
    let mut ch = false;
    ch |= toggle_cell(ui, "Fixed", "FIX", fixed, color);
    ch |= knob(ui, "Eq.T", et, 0, color);
    ch |= knob(ui, "Bend", bend, 88, color);
    ch |= knob(ui, "Offset", offset, 64, color);
    ch
}

// ---------------------------------------------------------------------------
// Part page
// ---------------------------------------------------------------------------

pub fn part_page(ui: &mut Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState) {
    let pi = st.part;
    let mut pp = (*patch.parts[pi]).clone();
    let mut ch = false;
    let mut kit_changed = false;
    let c = ZYN;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            panel(ui, "PART", c, COL3, |ui| {
                row(ui, |ui| {
                    ch |= toggle_pair(ui, ("ENABLED", &mut pp.enabled), ("NOTE ON", &mut pp.noteon), c);
                    let mut mode = if pp.polymode { 0 } else if pp.legatomode { 2 } else { 1 };
                    if choice(ui, "pmode", "Mode", &mut mode, &["Poly", "Mono", "Legato"], 58.0) {
                        pp.polymode = mode == 0;
                        pp.legatomode = mode == 2;
                        ch = true;
                    }
                    ch |= knobf(ui, "Volume", &mut pp.volume, -40.0, 13.333, 0.0, &|v| format!("{v:+.1}"), c);
                    ch |= knob(ui, "Pan", &mut pp.panning, 64, c);
                    ch |= knob(ui, "Vel.Sns", &mut pp.velsns, 64, c);
                    ch |= knob(ui, "Vel.Ofs", &mut pp.veloffs, 64, c);
                    let mut ks = pp.keyshift as i32 - 64;
                    if knob_i(ui, "Transp.", &mut ks, -64, 63, 0, &|v| format!("{v:+}"), c) {
                        pp.keyshift = (ks + 64) as u8;
                        ch = true;
                    }
                });
                row(ui, |ui| {
                    ch |= knob(ui, "Min Key", &mut pp.minkey, 0, c);
                    ch |= knob(ui, "Max Key", &mut pp.maxkey, 127, c);
                    ch |= knob_range(ui, "Key Lim", &mut pp.keylimit, 0, 60, 15, c);
                    ch |= knob_range(ui, "Voice Lim", &mut pp.voicelimit, 0, 60, 0, c);
                    let mut chn = pp.rcvchn + 1;
                    if knob_range(ui, "MIDI Ch", &mut chn, 1, 16, 1, c) {
                        pp.rcvchn = chn - 1;
                        ch = true;
                    }
                });
            });
            panel(ui, "CONTROLLERS", c, COL3, |ui| {
                let k = &mut pp.ctl;
                row(ui, |ui| {
                    let mut b = k.bendrange;
                    if knob_i(ui, "Bend", &mut b, -6400, 6400, 200, &|v| format!("{:+.1}", v as f32 / 100.0), c) {
                        k.bendrange = b;
                        ch = true;
                    }
                    ch |= knob(ui, "Mod Dep", &mut k.modwheel_depth, 80, c);
                    ch |= knob(ui, "Cut Dep", &mut k.filtercutoff_depth, 64, c);
                    ch |= knob(ui, "Q Dep", &mut k.filterq_depth, 64, c);
                    ch |= knob(ui, "BW Dep", &mut k.bandwidth_depth, 64, c);
                    ch |= knob(ui, "Pan Dep", &mut k.panning_depth, 64, c);
                    ch |= toggle_pair(ui, ("SUSTAIN", &mut k.sustain_receive), ("EXPR", &mut k.expression_receive), c);
                });
                row(ui, |ui| {
                    ch |= toggle_pair(ui, ("PORTA", &mut k.portamento_portamento), ("AUTO", &mut k.portamento_auto), c);
                    ch |= toggle_cell(ui, "Prop.", "PROP", &mut k.portamento_proportional, c);
                    ch |= knob(ui, "P.Time", &mut k.portamento_time, 64, c);
                    ch |= knob(ui, "P.Up/Dn", &mut k.portamento_updowntimestretch, 64, c);
                    ch |= knob(ui, "P.Thresh", &mut k.portamento_pitchthresh, 3, c);
                    ch |= knob(ui, "P.Rate", &mut k.portamento_proprate, 80, c);
                    ch |= knob(ui, "P.Depth", &mut k.portamento_propdepth, 90, c);
                });
            });
            panel(ui, "INSTRUMENT", c, COL3, |ui| {
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(40.0, 18.0), egui::Label::new(egui::RichText::new("Name").size(8.5).color(DIM)));
                    let mut name = pp.name.clone();
                    if ui.add(egui::TextEdit::singleline(&mut name).desired_width(300.0)).changed() {
                        pp.name = name;
                        ch = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(40.0, 18.0), egui::Label::new(egui::RichText::new("Author").size(8.5).color(DIM)));
                    let mut a = pp.author.clone();
                    if ui.add(egui::TextEdit::singleline(&mut a).desired_width(300.0)).changed() {
                        pp.author = a;
                        ch = true;
                    }
                });
                if !pp.comments.is_empty() {
                    ui.add(egui::Label::new(egui::RichText::new(&pp.comments).size(9.0).color(DIM)).wrap());
                }
                row(ui, |ui| {
                    ch |= choice(ui, "kitmode", "Kit mode", &mut pp.kitmode, &["Off", "Multi", "Single"], 64.0);
                    ch |= toggle_cell(ui, "Drums", "DRUM", &mut pp.drummode, c);
                });
            });
        });
        panel_h(ui, "KIT", c, FULL_W - COL3 - 6.0, Some(466.0), |ui| {
            if pp.kitmode == 0 {
                ui.label(egui::RichText::new("Kit mode is off: the instrument plays kit item 1. Switch Kit mode to Multi or Single to layer up to 16 items, each with its own ADD/SUB/PAD engines, key range and effect send.").size(9.5).color(DIM));
                ui.add_space(6.0);
            }
            egui::Grid::new("kitgrid").spacing(vec2(5.0, 3.0)).min_row_height(18.0).show(ui, |ui| {
                for h in ["#", "On", "Name", "Engines", "Mute", "Min key", "Max key", "FX", ""] {
                    small_label(ui, h);
                }
                ui.end_row();
                let n = if pp.kitmode == 0 { 1 } else { NUM_KIT_ITEMS };
                for i in 0..n {
                    let it = &mut pp.kit[i];
                    ui.label(egui::RichText::new(format!("{}", i + 1)).size(9.5).color(if st.kit == i { c } else { DIM }));
                    let mut en = it.enabled;
                    if i == 0 {
                        ui.label("");
                    } else if toggle(ui, "ON", &mut en, c) {
                        it.set_enabled(en, crate::state::OSCILSIZE);
                        kit_changed = true;
                    }
                    if it.enabled {
                        let mut nm = it.name.clone();
                        if ui.add(egui::TextEdit::singleline(&mut nm).desired_width(150.0)).changed() {
                            it.name = nm;
                            ch = true;
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            kit_changed |= toggle(ui, "ADD", &mut it.ad_enabled, ADD);
                            kit_changed |= toggle(ui, "SUB", &mut it.sub_enabled, SUB);
                            let was = it.pad_enabled;
                            kit_changed |= toggle(ui, "PAD", &mut it.pad_enabled, PAD);
                            if it.pad_enabled && !was && it.pad.as_ref().map(|p| p.samples.is_empty()).unwrap_or(false) {
                                st.pad_rebuild = Some((pi, i));
                            }
                        });
                        ch |= toggle(ui, "M", &mut it.muted, c);
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
                        if button(ui, "EDIT", st.kit == i, c, vec2(36.0, 16.0)).clicked() {
                            st.kit = i;
                        }
                    }
                    ui.end_row();
                }
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

fn oscil_wave(o: &OscilGen) -> Vec<f32> {
    let mut wave = vec![0.0f32; o.oscilsize()];
    with_fft(|fft| fft.freqs2smps(&o.freqs, &mut wave));
    wave
}

/// Oscillator designer: plots and harmonic bars on the left, the
/// generator parameters on the right. Returns true when the spectrum must
/// be rebuilt.
pub fn oscil(ui: &mut Ui, id: &str, o: &mut OscilGen, color: Color32, show_rand: bool, big: bool) -> bool {
    let k = if big { 1.45 } else { 1.0 };
    let mut ch = false;
    let wave = oscil_wave(o);
    let spec = o.spectrum(129);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 3.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 3.0);
            ui.horizontal(|ui| {
                wave_plot(ui, &wave, vec2(148.0, 56.0 * k), color);
                spectrum_plot(ui, &spec[1..], vec2(148.0, 56.0 * k), color);
            });
            ch |= bars(ui, "MAGNITUDE", &mut o.p.hmag[..64], 64, vec2(300.0, 84.0 * k), color);
            ch |= bars(ui, "PHASE", &mut o.p.hphase[..64], 64, vec2(300.0, 40.0 * k), color.gamma_multiply(0.6));
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 3.0);
            let p = &mut o.p;
            row(ui, |ui| {
                let mut bf = if p.basefunc == 127 { 17 } else { p.basefunc.min(16) };
                let mut names: Vec<&str> = BASE_FUNCTION_NAMES.to_vec();
                names.push("User");
                if choice(ui, (id, "bf"), "Base", &mut bf, &names, 72.0) {
                    p.basefunc = if bf == 17 { 127 } else { bf };
                    ch = true;
                }
                ch |= knob(ui, "Par", &mut p.basefuncpar, 64, color);
                ch |= choice(ui, (id, "mt"), "Mag", &mut p.hmagtype, &MAG_TYPES, 56.0);
                ch |= choice(ui, (id, "bm"), "B.Mod", &mut p.basefuncmodulation, &BMOD_TYPES, 54.0);
                ch |= knob(ui, "P1", &mut p.basefuncmodulationpar1, 64, color);
                ch |= knob(ui, "P2", &mut p.basefuncmodulationpar2, 64, color);
                ch |= knob(ui, "P3", &mut p.basefuncmodulationpar3, 32, color);
            });
            row(ui, |ui| {
                let mut ws = p.waveshapingfunction.min(17);
                if choice(ui, (id, "ws"), "Shaper", &mut ws, &WAVESHAPE_NAMES, 72.0) {
                    p.waveshapingfunction = ws;
                    ch = true;
                }
                ch |= knob(ui, "Drive", &mut p.waveshaping, 64, color);
                let mut ft = p.filtertype.min(14);
                if choice(ui, (id, "ft"), "Filter", &mut ft, &FILTER_NAMES, 56.0) {
                    p.filtertype = ft;
                    ch = true;
                }
                ch |= knob(ui, "F.1", &mut p.filterpar1, 64, color);
                ch |= knob(ui, "F.2", &mut p.filterpar2, 64, color);
                ch |= toggle_cell(ui, "Order", "PRE", &mut p.filterbeforews, color);
            });
            row(ui, |ui| {
                ch |= choice(ui, (id, "sa"), "Sp.Adjust", &mut p.satype, &SA_TYPES, 72.0);
                ch |= knob(ui, "SA.Par", &mut p.sapar, 64, color);
                ch |= choice(ui, (id, "mo"), "Mod", &mut p.modulation, &MOD_TYPES, 56.0);
                ch |= knob(ui, "M1", &mut p.modulationpar1, 64, color);
                ch |= knob(ui, "M2", &mut p.modulationpar2, 64, color);
                ch |= knob(ui, "M3", &mut p.modulationpar3, 32, color);
            });
            row(ui, |ui| {
                let mut ad = p.adaptiveharmonics.min(8);
                if choice(ui, (id, "ah"), "Adaptive", &mut ad, &ADAPT_TYPES, 72.0) {
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
                ch |= knob_i(ui, "H.Shift", &mut p.harmonicshift, -64, 64, 0, &|v| format!("{v:+}"), color);
                ch |= toggle_cell(ui, "Shift", "FIRST", &mut p.harmonicshiftfirst, color);
            });
            if show_rand {
                row(ui, |ui| {
                    ch |= choice(ui, (id, "ar"), "Amp.Rnd", &mut p.amprandtype, &["None", "Pow", "Sin"], 72.0);
                    ch |= knob(ui, "AR.Pow", &mut p.amprandpower, 64, color);
                    ch |= knob(ui, "Rand", &mut p.rand, 64, color);
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

pub fn add_page(ui: &mut Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState) {
    let (pi, ki) = (st.part, st.kit);
    let Some(src) = patch.parts[pi].kit.get(ki).and_then(|k| k.ad.as_ref()) else {
        ui.label("This kit item has no ADDsynth parameters.");
        return;
    };
    let mut ad: AdParams = (**src).clone();
    let ch = if st.voice == usize::MAX { add_global(ui, &mut ad, st) } else { add_voice(ui, &mut ad, st) };
    if ch {
        sh.edit(|p| {
            let part = Arc::make_mut(&mut p.parts[pi]);
            part.kit[ki].ad = Some(Arc::new(ad));
        });
    }
}

fn add_global(ui: &mut Ui, ad: &mut AdParams, st: &mut UiState) -> bool {
    let mut ch = false;
    let c = ADD;
    {
        let g = &mut ad.global;
        ui.horizontal_top(|ui| {
            panel_h(ui, "AMPLITUDE", c, COL3, Some(232.0), |ui| {
                row(ui, |ui| {
                    ch |= knobf(ui, "Volume", &mut g.volume, -60.0, 20.0, 0.0, &|v| format!("{v:+.1}"), c);
                    ch |= knob(ui, "Pan", &mut g.panning, 64, c);
                    ch |= knob(ui, "Vel.Sns", &mut g.amp_velocity_scale_function, 64, c);
                    vsep(ui);
                    ch |= knob(ui, "Punch", &mut g.punch_strength, 0, c);
                    ch |= knob(ui, "P.Time", &mut g.punch_time, 60, c);
                    ch |= knob(ui, "P.Strch", &mut g.punch_stretch, 64, c);
                    ch |= knob(ui, "P.Vel", &mut g.punch_velocity_sensing, 72, c);
                    vsep(ui);
                    ch |= knob(ui, "Fade-in", &mut g.fadein_adjustment, 20, c);
                });
                row(ui, |ui| {
                    ch |= toggle(ui, "STEREO", &mut g.stereo, c);
                    ch |= toggle(ui, "RND GROUP", &mut g.hrandgrouping, c);
                });
                ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "gae", &mut g.amp_envelope, c));
                ch |= tagged(ui, "LFO", c, |ui| lfo(ui, "gal", &mut g.amp_lfo, c));
            });
            panel_h(ui, "FREQUENCY", c, COL3, Some(232.0), |ui| {
                row(ui, |ui| {
                    ch |= freq_controls(ui, &mut g.detune, &mut g.coarse_detune, &mut g.detune_type, "gf", c, false);
                    vsep(ui);
                    ch |= knob(ui, "Rel.BW", &mut g.bandwidth, 64, c);
                });
                ui.add_space(19.0);
                ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "gfe", &mut g.freq_envelope, c));
                ch |= tagged(ui, "LFO", c, |ui| lfo(ui, "gfl", &mut g.freq_lfo, c));
            });
            panel_h(ui, "FILTER", c, COL3, Some(232.0), |ui| {
                let (vs, vf) = (&mut g.filter_velocity_scale, &mut g.filter_velocity_scale_function);
                ch |= filter(ui, "gflt", &mut g.filter, c, |ui| {
                    let mut x = knob(ui, "V.Amt", vs, 0, c);
                    x |= knob(ui, "V.Func", vf, 64, c);
                    x
                });
                ui.add_space(19.0);
                ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "gfle", &mut g.filter_envelope, c));
                ch |= tagged(ui, "LFO", c, |ui| lfo(ui, "gfll", &mut g.filter_lfo, c));
            });
        });
    }
    ch |= voice_cards(ui, ad, st);
    let r = &mut ad.global.reson;
    panel(ui, "RESONANCE", c, FULL_W, |ui| {
        row(ui, |ui| {
            ch |= toggle_pair(ui, ("ENABLED", &mut r.enabled), ("PROTECT 1st", &mut r.protect_fundamental), c);
            ch |= knob(ui, "Max dB", &mut r.max_db, 20, c);
            ch |= knob(ui, "Center", &mut r.centerfreq, 64, c);
            ch |= knob(ui, "Octaves", &mut r.octavesfreq, 64, c);
            let w = ui.available_width();
            ch |= bars(ui, "RESPONSE (drag to draw, right-click resets)", &mut r.points, 64, vec2(w, KH), if r.enabled { c } else { FAINT });
        });
    });
    ch
}

/// Overview of the 8 voices: waveform, on/off, the main levels and what
/// modulates them. Clicking the title opens the voice.
fn voice_cards(ui: &mut Ui, ad: &mut AdParams, st: &mut UiState) -> bool {
    let mut ch = false;
    let c = ADD;
    let waves: Vec<Vec<f32>> = (0..NUM_VOICES)
        .map(|i| {
            let v = &ad.voices[i];
            let src = if v.ext_oscil >= 0 && (v.ext_oscil as usize) < i { &ad.voices[v.ext_oscil as usize].oscil } else { &v.oscil };
            oscil_wave(src)
        })
        .collect();
    panel(ui, "VOICES", c, FULL_W, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let cw = (FULL_W - 12.0 - 7.0 * 6.0) / 8.0;
            for i in 0..NUM_VOICES {
                let v = &mut ad.voices[i];
                let on = v.enabled;
                let frame = egui::Frame::new().fill(if on { CONTROL.gamma_multiply(0.75) } else { PANEL2 }).corner_radius(CornerRadius::same(3)).inner_margin(egui::Margin::same(4));
                frame.show(ui, |ui| ui.vertical(|ui| {
                    ui.set_width(cw - 8.0);
                    ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                    ui.horizontal(|ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(cw - 8.0 - 34.0, 16.0), Sense::click());
                        let p = ui.painter();
                        if resp.hovered() {
                            p.rect_filled(r, CornerRadius::same(2), CONTROL_HOVER);
                        }
                        p.text(pos2(r.left() + 3.0, r.center().y), Align2::LEFT_CENTER, format!("VOICE {}", i + 1), font(9.0), if on { c } else { DIM });
                        p.text(pos2(r.right() - 3.0, r.center().y), Align2::RIGHT_CENTER, "EDIT ›", font(8.0), if resp.hovered() { TEXT } else { FAINT });
                        if resp.clicked() {
                            st.voice = i;
                        }
                        let mut en = v.enabled;
                        if toggle(ui, "ON", &mut en, c) {
                            v.enabled = en;
                            ch = true;
                        }
                    });
                    wave_plot(ui, &waves[i], vec2(cw - 8.0, 30.0), if on { c } else { FAINT });
                    row(ui, |ui| {
                        ch |= knobf(ui, "Volume", &mut v.volume, -60.0, 0.0, -12.75, &|x| format!("{x:.0}"), c);
                        ch |= knob(ui, "Pan", &mut v.panning, 64, c);
                        let mut d = v.detune as i32 - 8192;
                        if knob_i(ui, "Detune", &mut d, -8192, 8191, 0, &|x| format!("{:+.0}", x as f32 / 81.92), c) {
                            v.detune = (d + 8192) as u16;
                            ch = true;
                        }
                    });
                    let fm = ["", "Mix", "Ring", "PM", "FM", "PWM"][(v.fm_enabled as usize).min(5)];
                    let mut info = if v.unison_size > 1 { format!("unison {}", v.unison_size) } else { String::new() };
                    if !fm.is_empty() {
                        if !info.is_empty() {
                            info += " · ";
                        }
                        info += fm;
                    }
                    if v.ext_oscil >= 0 {
                        if !info.is_empty() {
                            info += " · ";
                        }
                        info += &format!("osc V{}", v.ext_oscil + 1);
                    }
                    ui.label(egui::RichText::new(if info.is_empty() { "-".to_string() } else { info }).size(8.0).color(DIM));
                }));
            }
        });
    });
    ch
}

fn add_voice(ui: &mut Ui, ad: &mut AdParams, st: &mut UiState) -> bool {
    let mut ch = false;
    let c = ADD;
    let vi = st.voice.min(NUM_VOICES - 1);
    let v: &mut AdVoice = &mut ad.voices[vi];
    panel(ui, &format!("VOICE {}", vi + 1), c, FULL_W, |ui| {
        row(ui, |ui| {
            ch |= toggle_cell(ui, "Voice", "ON", &mut v.enabled, c);
            ch |= choice(ui, "vtype", "Type", &mut v.vtype, &["Sound", "White", "Pink", "DC"], 56.0);
            let mut ext = (v.ext_oscil + 1) as u8;
            let opts: Vec<String> = std::iter::once("Own".to_string()).chain((0..vi).map(|i| format!("V{}", i + 1))).collect();
            let refs: Vec<&str> = opts.iter().map(|s| s.as_str()).collect();
            if choice(ui, "extosc", "Oscil", &mut ext, &refs, 48.0) {
                v.ext_oscil = ext as i32 - 1;
                ch = true;
            }
            ch |= toggle_pair(ui, ("RESON", &mut v.resonance), ("AA", &mut v.aa_enabled), c);
            vsep(ui);
            ch |= knobf(ui, "Volume", &mut v.volume, -60.0, 0.0, -12.75, &|x| format!("{x:.1}"), c);
            ch |= toggle_cell(ui, "Phase", "INV", &mut v.volume_minus, c);
            ch |= knob(ui, "Pan", &mut v.panning, 64, c);
            ch |= knob(ui, "Vel.Sns", &mut v.amp_velocity_scale_function, 127, c);
            ch |= knob(ui, "Delay", &mut v.delay, 0, c);
            ch |= knob(ui, "Osc.Ph", &mut v.oscil_phase, 64, c);
            vsep(ui);
            ch |= freq_controls(ui, &mut v.detune, &mut v.coarse_detune, &mut v.detune_type, "vf", c, true);
            ch |= fixed_controls(ui, &mut v.fixed_freq, &mut v.fixed_freq_et, &mut v.bend_adjust, &mut v.offset_hz, c);
            vsep(ui);
            ch |= knob_range(ui, "Unison", &mut v.unison_size, 1, 50, 1, c);
            ch |= knob(ui, "U.Spread", &mut v.unison_frequency_spread, 60, c);
            ch |= knob(ui, "U.Stereo", &mut v.unison_stereo_spread, 64, c);
            ch |= knob(ui, "U.Vib", &mut v.unison_vibratto, 64, c);
            ch |= knob(ui, "U.VSpd", &mut v.unison_vibratto_speed, 64, c);
            ch |= knob(ui, "U.Phase", &mut v.unison_phase_randomness, 127, c);
            ch |= knob_range(ui, "U.Invert", &mut v.unison_invert_phase, 0, 5, 0, c);
        });
    });
    let has_mod_osc = v.fm_enabled != FmType::None && v.fm_voice < 0;
    if !has_mod_osc {
        st.mod_osc = false;
    }
    ui.horizontal_top(|ui| {
        panel_h(ui, "OSCILLATOR", c, OSC_W, Some(466.0 - 68.0 - 6.0), |ui| {
            if has_mod_osc {
                let sel = st.mod_osc as usize;
                if let Some(i) = segmented(ui, &["CARRIER", "MODULATOR"], sel, &[], c, 90.0, 18.0) {
                    st.mod_osc = i == 1;
                }
                ui.add_space(2.0);
            }
            if st.mod_osc {
                ch |= oscil(ui, "fmosc", &mut v.fm_oscil, c.gamma_multiply(0.75), true, true);
            } else {
                ch |= oscil(ui, "vosc", &mut v.oscil, c, true, true);
            }
        });
        let marks = [v.fm_enabled != FmType::None, v.amp_envelope_enabled || v.amp_lfo_enabled, v.freq_envelope_enabled || v.freq_lfo_enabled, v.filter_enabled];
        tab_panel(ui, "VOICE MODULATION", c, SIDE_W, 466.0 - 68.0 - 6.0, &["MODULATOR", "AMP", "FREQ", "FILTER"], &marks, &mut st.tab, |ui, tab| match tab {
            0 => {
                let mut fm = v.fm_enabled as u8;
                row(ui, |ui| {
                    if choice(ui, "fmt", "Type", &mut fm, &["Off", "Mix", "Ring", "PM", "FM", "PWM"], 56.0) {
                        v.fm_enabled = FmType::from_i(fm as i32);
                        ch = true;
                    }
                    let mut src = (v.fm_voice + 1) as u8;
                    let opts: Vec<String> = std::iter::once("Osc".to_string()).chain((0..vi).map(|i| format!("V{}", i + 1))).collect();
                    let refs: Vec<&str> = opts.iter().map(|s| s.as_str()).collect();
                    if choice(ui, "fmsrc", "Source", &mut src, &refs, 48.0) {
                        v.fm_voice = src as i32 - 1;
                        ch = true;
                    }
                    ch |= toggle_cell(ui, "Hard", "SYNC", &mut v.sync_enabled, c);
                    vsep(ui);
                    ch |= knobf(ui, "Volume", &mut v.fm_volume, 0.0, 100.0, 70.0, &|x| format!("{x:.0}"), c);
                    ch |= knob(ui, "Damp", &mut v.fm_volume_damp, 64, c);
                    ch |= knob(ui, "Vel.Sns", &mut v.fm_velocity_scale_function, 64, c);
                    ch |= knob(ui, "Phase", &mut v.fm_oscil_phase, 64, c);
                });
                row(ui, |ui| {
                    ch |= freq_controls(ui, &mut v.fm_detune, &mut v.fm_coarse_detune, &mut v.fm_detune_type, "fmf", c, true);
                    ch |= toggle_cell(ui, "Fixed", "FIX", &mut v.fm_fixed_freq, c);
                });
                row(ui, |ui| {
                    ch |= toggle(ui, "AMP ENV", &mut v.fm_amp_envelope_enabled, c);
                    ch |= toggle(ui, "FREQ ENV", &mut v.fm_freq_envelope_enabled, c);
                });
                if v.fm_amp_envelope_enabled {
                    ch |= tagged(ui, "AMP", c, |ui| envelope(ui, "fmae", &mut v.fm_amp_envelope, c));
                }
                if v.fm_freq_envelope_enabled {
                    ch |= tagged(ui, "FREQ", c, |ui| envelope(ui, "fmfe", &mut v.fm_freq_envelope, c));
                }
                if v.fm_enabled != FmType::None && v.fm_voice < 0 {
                    ui.label(egui::RichText::new("The modulator oscillator is edited in the OSCILLATOR panel (MODULATOR tab).").size(8.5).color(DIM));
                }
            }
            1 => {
                row(ui, |ui| {
                    ch |= toggle(ui, "AMP ENVELOPE", &mut v.amp_envelope_enabled, c);
                    ch |= toggle(ui, "AMP LFO", &mut v.amp_lfo_enabled, c);
                });
                if v.amp_envelope_enabled {
                    ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "vae", &mut v.amp_envelope, c));
                }
                if v.amp_lfo_enabled {
                    ch |= tagged(ui, "LFO", c, |ui| lfo(ui, "val", &mut v.amp_lfo, c));
                }
            }
            2 => {
                row(ui, |ui| {
                    ch |= toggle(ui, "FREQ ENVELOPE", &mut v.freq_envelope_enabled, c);
                    ch |= toggle(ui, "FREQ LFO", &mut v.freq_lfo_enabled, c);
                });
                if v.freq_envelope_enabled {
                    ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "vfe", &mut v.freq_envelope, c));
                }
                if v.freq_lfo_enabled {
                    ch |= tagged(ui, "LFO", c, |ui| lfo(ui, "vfl", &mut v.freq_lfo, c));
                }
            }
            _ => {
                row(ui, |ui| {
                    ch |= toggle(ui, "FILTER", &mut v.filter_enabled, c);
                    ch |= toggle(ui, "BYPASS GLOBAL", &mut v.filter_bypass, c);
                    ch |= toggle(ui, "IGNORE CC", &mut v.filter_fcctl_bypass, c);
                    ch |= toggle(ui, "ENV", &mut v.filter_envelope_enabled, c);
                    ch |= toggle(ui, "LFO", &mut v.filter_lfo_enabled, c);
                });
                if v.filter_enabled {
                    let (vs, vf) = (&mut v.filter_velocity_scale, &mut v.filter_velocity_scale_function);
                    ch |= filter(ui, "vflt", &mut v.filter, c, |ui| {
                        let mut x = knob(ui, "V.Amt", vs, 0, c);
                        x |= knob(ui, "V.Func", vf, 64, c);
                        x
                    });
                    if v.filter_envelope_enabled {
                        ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "vfle", &mut v.filter_envelope, c));
                    }
                    if v.filter_lfo_enabled {
                        ch |= tagged(ui, "LFO", c, |ui| lfo(ui, "vfll", &mut v.filter_lfo, c));
                    }
                }
            }
        });
    });
    ch
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
    let c = SUB;
    panel(ui, "HARMONICS", c, FULL_W, |ui| {
        ch |= bars(ui, "MAGNITUDE (drag to draw, right-click resets)", &mut s.hmag, 0, vec2(FULL_W - 12.0, 104.0), c);
        ch |= bars(ui, "RELATIVE BANDWIDTH", &mut s.hrelbw, 64, vec2(FULL_W - 12.0, 46.0), c.gamma_multiply(0.6));
    });
    ui.horizontal_top(|ui| {
        panel_h(ui, "AMPLITUDE", c, COL3, Some(282.0), |ui| {
            row(ui, |ui| {
                ch |= knobf(ui, "Volume", &mut s.volume, -60.0, 0.0, 0.0, &|v| format!("{v:.1}"), c);
                ch |= knob(ui, "Pan", &mut s.panning, 64, c);
                ch |= knobf(ui, "Vel.Sns", &mut s.amp_velocity_scale_function, 0.0, 127.0, 70.86, &|v| format!("{v:.0}"), c);
                ch |= toggle_cell(ui, "Output", "STEREO", &mut s.stereo, c);
            });
            ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "sae", &mut s.amp_envelope, c));
        });
        panel_h(ui, "BANDWIDTH & FREQUENCY", c, COL3, Some(282.0), |ui| {
            row(ui, |ui| {
                ch |= knob(ui, "Bandw.", &mut s.bandwidth, 40, c);
                ch |= knob(ui, "BW.Scale", &mut s.bwscale, 64, c);
                ch |= knob_range(ui, "Stages", &mut s.numstages, 1, 5, 2, c);
                ch |= choice(ui, "smt", "Mag", &mut s.hmagtype, &MAG_TYPES, 56.0);
                ch |= choice(ui, "sst", "Start", &mut s.start, &["Zero", "Rnd", "Max"], 50.0);
            });
            row(ui, |ui| {
                ch |= freq_controls(ui, &mut s.detune, &mut s.coarse_detune, &mut s.detune_type, "sf", c, false);
                ch |= fixed_controls(ui, &mut s.fixed_freq, &mut s.fixed_freq_et, &mut s.bend_adjust, &mut s.offset_hz, c);
            });
            row(ui, |ui| {
                let mut t = s.overtone_spread_type;
                let mut o = choice(ui, "ost", "Overtones", &mut t, &["Harmonic", "ShiftU", "ShiftL", "PowerU", "PowerL", "Sine", "Power", "Shift"], 70.0);
                let mut a = s.overtone_spread_par1;
                let mut b = s.overtone_spread_par2;
                let mut cc = s.overtone_spread_par3;
                o |= knob_range(ui, "Par1", &mut a, 0, 255, 0, c);
                o |= knob_range(ui, "Par2", &mut b, 0, 255, 0, c);
                o |= knob_range(ui, "ForceH", &mut cc, 0, 255, 0, c);
                if o {
                    s.overtone_spread_type = t;
                    s.overtone_spread_par1 = a;
                    s.overtone_spread_par2 = b;
                    s.overtone_spread_par3 = cc;
                    s.update_frequency_multipliers();
                    ch = true;
                }
                vsep(ui);
                ch |= toggle_pair(ui, ("FREQ ENV", &mut s.freq_envelope_enabled), ("BW ENV", &mut s.bandwidth_envelope_enabled), c);
            });
            if s.freq_envelope_enabled {
                ch |= tagged(ui, "FREQ", c, |ui| envelope(ui, "sfe", &mut s.freq_envelope, c));
            }
            if s.bandwidth_envelope_enabled {
                ch |= tagged(ui, "BW", c, |ui| envelope(ui, "sbe", &mut s.bandwidth_envelope, c));
            }
        });
        panel_h(ui, "FILTER", c, COL3, Some(282.0), |ui| {
            row(ui, |ui| {
                ch |= toggle(ui, "FILTER ENABLED", &mut s.global_filter_enabled, c);
            });
            if s.global_filter_enabled {
                let (vs, vf) = (&mut s.global_filter_velocity_scale, &mut s.global_filter_velocity_scale_function);
                ch |= filter(ui, "sflt", &mut s.global_filter, c, |ui| {
                    let mut x = knob(ui, "V.Amt", vs, 0, c);
                    x |= knob(ui, "V.Func", vf, 64, c);
                    x
                });
                ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "sfle", &mut s.global_filter_envelope, c));
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
    let c = PAD;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            panel(ui, "HARMONIC PROFILE", c, OSC_W, |ui| {
                ui.horizontal_top(|ui| {
                    let mut prof = vec![0.0f32; 256];
                    let bw = p.getprofile(&mut prof);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        wave_plot(ui, &prof.iter().map(|v| v * 2.0 - 1.0).collect::<Vec<_>>(), vec2(200.0, 70.0), c);
                        small_label(ui, &format!("perceived bandwidth {:.2}", bw));
                    });
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        row(ui, |ui| {
                            table |= choice(ui, "pbt", "Base", &mut p.hp_base_type, &["Gauss", "Square", "DoubleExp"], 72.0);
                            table |= knob(ui, "Width", &mut p.hp_base_par1, 80, c);
                            table |= knob(ui, "Mult", &mut p.hp_freqmult, 0, c);
                            table |= knob(ui, "Mod", &mut p.hp_mod_par1, 0, c);
                            table |= knob(ui, "M.Freq", &mut p.hp_mod_freq, 30, c);
                            table |= knob(ui, "Size", &mut p.hp_width, 127, c);
                            table |= choice(ui, "poh", "Half", &mut p.hp_onehalf, &["Full", "Upper", "Lower"], 56.0);
                        });
                        row(ui, |ui| {
                            table |= choice(ui, "pat", "Amp", &mut p.hp_amp_type, &["Off", "Gauss", "Sine", "Flat"], 72.0);
                            table |= choice(ui, "pam", "A.Mode", &mut p.hp_amp_mode, &["Sum", "Mult", "Div1", "Div2"], 56.0);
                            table |= knob(ui, "A.Par1", &mut p.hp_amp_par1, 80, c);
                            table |= knob(ui, "A.Par2", &mut p.hp_amp_par2, 64, c);
                            table |= toggle_cell(ui, "Scale", "AUTO", &mut p.hp_autoscale, c);
                        });
                    });
                });
            });
            panel(ui, "SPECTRUM", c, OSC_W, |ui| {
                row(ui, |ui| {
                    table |= choice(ui, "pmode", "Mode", &mut p.mode, &["Bandwidth", "Discrete", "Continuous"], 80.0);
                    let mut bw = p.bandwidth;
                    if knob_i(ui, "Bandw.", &mut bw, 0, 1000, 500, &|v| format!("{v}"), c) {
                        p.bandwidth = bw;
                        table = true;
                    }
                    table |= choice(ui, "pbws", "BW Scale", &mut p.bwscale, &["Normal", "Equal", "Q.Down", "Q.Up", "H.Down", "Up", "2xUp", "Down"], 64.0);
                    vsep(ui);
                    table |= choice(ui, "phr", "Overtones", &mut p.hrpos_type, &["Harmonic", "ShiftU", "ShiftL", "PowerU", "PowerL", "Sine", "Power", "Shift"], 70.0);
                    table |= knob_range(ui, "Par1", &mut p.hrpos_par1, 0, 255, 0, c);
                    table |= knob_range(ui, "Par2", &mut p.hrpos_par2, 0, 255, 0, c);
                    table |= knob_range(ui, "ForceH", &mut p.hrpos_par3, 0, 255, 0, c);
                });
                row(ui, |ui| {
                    table |= choice(ui, "pqs", "Sample", &mut p.q_samplesize, &["16k", "32k", "64k", "128k", "256k", "512k", "1M"], 56.0);
                    table |= choice(ui, "pqb", "Base", &mut p.q_basenote, &["C-2", "G-2", "C-3", "G-3", "C-4", "G-4", "C-5", "G-5", "G-6"], 56.0);
                    table |= choice(ui, "pqo", "Octaves", &mut p.q_oct, &["1", "2", "3", "4", "5", "6", "7", "8"], 48.0);
                    table |= choice(ui, "pqp", "Smp/Oct", &mut p.q_smpoct, &["0.5", "1", "2", "3", "4", "6", "12"], 48.0);
                    let busy = sh.busy.load(std::sync::atomic::Ordering::Relaxed) > 0;
                    let (r, _) = ui.allocate_exact_size(vec2(200.0, KH), Sense::hover());
                    ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, format!("{} wavetables{}", p.samples.len(), if busy { "  ·  building..." } else { "" }), font(9.0), if busy { c } else { DIM });
                });
            });
            panel(ui, "OSCILLATOR", c, OSC_W, |ui| {
                if oscil(ui, "posc", &mut p.oscil, c, false, false) {
                    table = true;
                }
            });
        });
        let marks = [true, true, true];
        tab_panel(ui, "PAD SOUND", c, SIDE_W, 466.0, &["AMPLITUDE", "FREQUENCY", "FILTER"], &marks, &mut st.tab, |ui, tab| match tab {
            0 => {
                row(ui, |ui| {
                    ch |= knob(ui, "Volume", &mut p.volume, 90, c);
                    ch |= knob(ui, "Pan", &mut p.panning, 64, c);
                    ch |= knob(ui, "Vel.Sns", &mut p.amp_velocity_scale_function, 64, c);
                    vsep(ui);
                    ch |= knob(ui, "Punch", &mut p.punch_strength, 0, c);
                    ch |= knob(ui, "P.Time", &mut p.punch_time, 60, c);
                    ch |= knob(ui, "P.Strch", &mut p.punch_stretch, 64, c);
                    ch |= knob(ui, "P.Vel", &mut p.punch_velocity_sensing, 72, c);
                    ch |= toggle_cell(ui, "Output", "STEREO", &mut p.stereo, c);
                });
                ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "pae", &mut p.amp_envelope, c));
                ch |= tagged(ui, "LFO", c, |ui| lfo(ui, "pal", &mut p.amp_lfo, c));
            }
            1 => {
                row(ui, |ui| {
                    ch |= freq_controls(ui, &mut p.detune, &mut p.coarse_detune, &mut p.detune_type, "pf", c, false);
                    ch |= fixed_controls(ui, &mut p.fixed_freq, &mut p.fixed_freq_et, &mut p.bend_adjust, &mut p.offset_hz, c);
                });
                ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "pfe", &mut p.freq_envelope, c));
                ch |= tagged(ui, "LFO", c, |ui| lfo(ui, "pfl", &mut p.freq_lfo, c));
            }
            _ => {
                let (vs, vf) = (&mut p.filter_velocity_scale, &mut p.filter_velocity_scale_function);
                ch |= filter(ui, "pflt", &mut p.filter, c, |ui| {
                    let mut x = knob(ui, "V.Amt", vs, 0, c);
                    x |= knob(ui, "V.Func", vf, 64, c);
                    x
                });
                ch |= tagged(ui, "ENV", c, |ui| envelope(ui, "pfle", &mut p.filter_envelope, c));
                ch |= tagged(ui, "LFO", c, |ui| lfo(ui, "pfll", &mut p.filter_lfo, c));
            }
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

fn effect_choices(kind: u8) -> Vec<(usize, &'static str, &'static [&'static str])> {
    let mut t: Vec<(usize, &str, &[&str])> = Vec::new();
    match kind {
        1 => t.push((10, "Type", &["Random", "Freeverb", "Bandwidth"])),
        3 => {
            t.push((4, "LFO", &["Sine", "Tri"]));
            t.push((10, "Mode", &["Chorus", "Flange", "Triple", "Dual"]));
            t.push((11, "Subtract", &["Off", "On"]));
        }
        4 => {
            t.push((4, "LFO", &["Sine", "Tri"]));
            t.push((10, "Subtract", &["Off", "On"]));
            t.push((12, "Hyper", &["Off", "On"]));
            t.push((14, "Analog", &["Off", "On"]));
        }
        5 => t.push((4, "LFO", &["Sine", "Tri"])),
        8 => {
            t.push((4, "LFO", &["Sine", "Tri"]));
            t.push((8, "Inv.Amp", &["Off", "On"]));
        }
        9 => t.push((6, "Negate", &["Off", "On"])),
        10 => {
            t.push((3, "Stereo", &["Off", "On"]));
            t.push((6, "Sync", &["Note On", "On/Off", "Auto", "Host"]));
        }
        6 => {
            t.push((5, "Type", &WAVESHAPE_NAMES[1..]));
            t.push((6, "Negate", &["Off", "On"]));
            t.push((9, "Stereo", &["Off", "On"]));
            t.push((10, "PreFilt", &["Off", "On"]));
        }
        _ => {}
    }
    t
}

/// One effect slot editor (type, preset, parameters). Returns true on change.
fn effect_slot(ui: &mut Ui, id: &str, cfg: &mut EffectCfg, insertion: bool) -> bool {
    let mut ch = false;
    let c = FX;
    row(ui, |ui| {
        let mut k = cfg.kind.min(10);
        if choice(ui, (id, "kind"), "Effect", &mut k, &EFFECT_NAMES, 100.0) {
            cfg.set_kind(k, insertion);
            ch = true;
        }
        let names = preset_names(cfg.kind);
        if !names.is_empty() {
            let mut pr = cfg.preset.min(names.len() as u8 - 1);
            if choice(ui, (id, "pre"), "Preset", &mut pr, names, 110.0) {
                cfg.set_preset(pr, insertion);
                ch = true;
            }
        }
        if cfg.kind != 0 {
            vsep(ui);
            for (n, name, opts) in effect_choices(cfg.kind) {
                let mut v = (cfg.settings[n].max(0) as u8).min(opts.len() as u8 - 1);
                if choice(ui, (id, n), name, &mut v, opts, 64.0) {
                    cfg.settings[n] = v as i16;
                    ch = true;
                }
            }
        }
    });
    if cfg.kind == 0 {
        return ch;
    }
    row(ui, |ui| {
        for (n, name) in effect_param_names(cfg.kind) {
            let mut v = cfg.settings[n].max(0) as u8;
            let d = v;
            if knob(ui, name, &mut v, d, c) {
                cfg.settings[n] = v as i16;
                ch = true;
            }
        }
    });
    if cfg.kind == 7 {
        for half in 0..2 {
            row(ui, |ui| {
                for b in (half * 4)..(half * 4 + 4) {
                    let base = 10 + b * 5;
                    let mut t = cfg.settings[base].max(0) as u8;
                    if choice(ui, (id, "eqt", b), &format!("Band {}", b + 1), &mut t, &["Off", "LP1", "HP1", "LP2", "HP2", "BP2", "N2", "Pk", "LSh", "HSh"], 46.0) {
                        cfg.settings[base] = t as i16;
                        ch = true;
                    }
                    for (k, nm) in [(1, "Freq"), (2, "Gain"), (3, "Q"), (4, "St")] {
                        let mut v = cfg.settings[base + k].max(0) as u8;
                        if t != 0 {
                            if knob(ui, nm, &mut v, 64, c) {
                                cfg.settings[base + k] = v as i16;
                                ch = true;
                            }
                        } else {
                            ui.allocate_exact_size(vec2(KW, KH), Sense::hover());
                        }
                    }
                    if b % 4 != 3 {
                        vsep(ui);
                    }
                }
            });
        }
    }
    if cfg.kind == 8 {
        ch |= tagged(ui, "FILTER", c, |ui| filter(ui, &format!("{id}flt"), &mut cfg.filter, c, |_| false));
    }
    ch
}

fn slot_label(cfg: &EffectCfg) -> (String, String) {
    if cfg.kind == 0 {
        return ("-".into(), String::new());
    }
    let name = EFFECT_NAMES.get(cfg.kind as usize).copied().unwrap_or("?").to_string();
    let pre = preset_names(cfg.kind).get(cfg.preset as usize).copied().unwrap_or("").to_string();
    (name, pre)
}

pub fn fx_page(ui: &mut Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState) {
    let pi = st.part;
    let mut p = patch.clone();
    let mut ch = false;
    let c = FX;
    ui.horizontal_top(|ui| {
        // ---- rack ----
        panel_h(ui, "EFFECT RACK", c, 270.0, Some(466.0), |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let mut pick: Option<FxSel> = None;
            let mut slot = |ui: &mut Ui, sel: FxSel, idx: String, cfg: &EffectCfg, extra: String, active: bool| {
                let (r, resp) = ui.allocate_exact_size(vec2(258.0, 20.0), Sense::click());
                let on = st.fx == sel;
                let pp = ui.painter();
                pp.rect_filled(r, CornerRadius::same(3), if on { c.gamma_multiply(0.85) } else if resp.hovered() { CONTROL_HOVER } else { CONTROL });
                let (name, pre) = slot_label(cfg);
                let tc = if on { ON_TEXT } else if cfg.kind == 0 || !active { FAINT } else { TEXT };
                pp.text(pos2(r.left() + 6.0, r.center().y), Align2::LEFT_CENTER, idx, font(8.5), if on { ON_TEXT } else { DIM });
                pp.text(pos2(r.left() + 26.0, r.center().y), Align2::LEFT_CENTER, &name, font(9.5), tc);
                let nw = pp.layout_no_wrap(name.clone(), font(9.5), tc).size().x;
                pp.text(pos2(r.left() + 32.0 + nw, r.center().y), Align2::LEFT_CENTER, pre, font(8.5), if on { ON_TEXT } else { DIM });
                pp.text(pos2(r.right() - 6.0, r.center().y), Align2::RIGHT_CENTER, extra, font(8.5), if on { ON_TEXT } else { DIM });
                if cfg.kind != 0 && active && !on {
                    pp.circle_filled(pos2(r.left() + 17.0, r.center().y), 2.5, c);
                }
                if resp.clicked() {
                    pick = Some(sel);
                }
            };
            small_label(ui, &format!("PART {} INSTRUMENT EFFECTS", pi + 1));
            let pp = &p.parts[pi];
            for n in 0..NUM_PART_EFX {
                let route = ["next", "out", "dry+out"][(pp.efxroute[n] as usize).min(2)];
                let extra = if pp.efxbypass[n] { "bypass".to_string() } else { format!("to {route}") };
                slot(ui, (0, n), format!("{}", n + 1), &pp.efx[n], extra, !pp.efxbypass[n]);
            }
            ui.add_space(4.0);
            small_label(ui, "SYSTEM EFFECTS (sends)");
            for n in 0..NUM_SYS_EFX {
                let send = p.sysefxvol[n][pi];
                slot(ui, (1, n), format!("{}", n + 1), &p.sysefx[n], format!("P{} send {}", pi + 1, send), true);
            }
            ui.add_space(4.0);
            small_label(ui, "INSERTION EFFECTS");
            for n in 0..NUM_INS_EFX {
                let t = p.insparts[n];
                let extra = match t {
                    -1 => "off".to_string(),
                    -2 => "master".to_string(),
                    x => format!("part {}", x + 1),
                };
                slot(ui, (2, n), format!("{}", n + 1), &p.insefx[n], extra, t != -1);
            }
            if let Some(s) = pick {
                st.fx = s;
            }
        });

        // ---- editor ----
        let (kind, n) = st.fx;
        let title = match kind {
            0 => format!("PART {} · INSTRUMENT EFFECT {}", pi + 1, n + 1),
            1 => format!("SYSTEM EFFECT {}", n + 1),
            _ => format!("INSERTION EFFECT {}", n + 1),
        };
        panel_h(ui, &title, c, FULL_W - 276.0, Some(466.0), |ui| {
            match kind {
                0 => {
                    let n = n.min(NUM_PART_EFX - 1);
                    let pp = Arc::make_mut(&mut p.parts[pi]);
                    row(ui, |ui| {
                        ch |= choice(ui, ("route", n), "Route", &mut pp.efxroute[n], &["Next", "Out", "Dry+Out"], 70.0);
                        ch |= toggle_cell(ui, "Bypass", "BYPASS", &mut pp.efxbypass[n], c);
                    });
                    ch |= effect_slot(ui, &format!("pe{n}"), &mut pp.efx[n], true);
                }
                1 => {
                    let n = n.min(NUM_SYS_EFX - 1);
                    ch |= effect_slot(ui, &format!("se{n}"), &mut p.sysefx[n], false);
                    ui.add_space(4.0);
                    small_label(ui, "SEND FROM PARTS");
                    row(ui, |ui| {
                        for q in 0..NUM_MIDI_PARTS {
                            ch |= knob(ui, &format!("P{}", q + 1), &mut p.sysefxvol[n][q], 0, if q == pi { ZYN } else { c });
                        }
                    });
                    if n + 1 < NUM_SYS_EFX {
                        small_label(ui, "SEND TO NEXT SYSTEM EFFECTS");
                        row(ui, |ui| {
                            for t in (n + 1)..NUM_SYS_EFX {
                                ch |= knob(ui, &format!("> S{}", t + 1), &mut p.sysefxsend[n][t], 0, c);
                            }
                        });
                    }
                }
                _ => {
                    let n = n.min(NUM_INS_EFX - 1);
                    row(ui, |ui| {
                        let mut target = (p.insparts[n] + 2).clamp(0, NUM_MIDI_PARTS as i32 + 1) as u8;
                        let opts: Vec<String> = ["Master".to_string(), "Off".to_string()].into_iter().chain((1..=NUM_MIDI_PARTS).map(|i| format!("Part {i}"))).collect();
                        let refs: Vec<&str> = opts.iter().map(|s| s.as_str()).collect();
                        if choice(ui, ("ins", n), "Insert on", &mut target, &refs, 80.0) {
                            p.insparts[n] = target as i32 - 2;
                            ch = true;
                        }
                    });
                    if p.insparts[n] != -1 {
                        ch |= effect_slot(ui, &format!("ie{n}"), &mut p.insefx[n], true);
                    } else {
                        small_label(ui, "Choose where this effect is inserted to edit it.");
                    }
                }
            }
        });
    });
    if ch {
        sh.edit(|q| *q = p);
    }
}

// ---------------------------------------------------------------------------
// Mixer / master / tuning
// ---------------------------------------------------------------------------

pub fn master_page(ui: &mut Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState) {
    let mut p = patch.clone();
    let mut ch = false;
    let c = MIX;
    ui.horizontal_top(|ui| {
        panel(ui, "MASTER", c, COL3, |ui| {
            row(ui, |ui| {
                ch |= knobf(ui, "Volume", &mut p.volume, -40.0, 13.33, -6.67, &|v| format!("{v:+.1}"), c);
                let mut ks = p.keyshift as i32 - 64;
                if knob_i(ui, "Transp.", &mut ks, -64, 63, 0, &|v| format!("{v:+}"), c) {
                    p.keyshift = (ks + 64) as u8;
                    ch = true;
                }
                ch |= toggle_cell(ui, "Pan law", "CONST POWER", &mut p.const_power, c);
            });
        });
        panel(ui, "TUNING", c, FULL_W - COL3 - 6.0, |ui| {
            let m = &mut p.micro;
            row(ui, |ui| {
                ch |= knobf(ui, "A Freq", &mut m.afreq, 400.0, 480.0, 440.0, &|v| format!("{v:.1}"), c);
                ch |= knob(ui, "A Note", &mut m.anote, 69, c);
                ch |= knob(ui, "Fine", &mut m.globalfinedetune, 64, c);
                ch |= toggle_pair(ui, ("MICROTONAL", &mut m.enabled), ("INVERT", &mut m.invertupdown), c);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), KH), Sense::hover());
                let txt = if m.name.is_empty() && m.comment.is_empty() { "12-tone equal temperament".to_string() } else { format!("{}  ·  {}", m.name, m.comment) };
                ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, txt, font(9.5), DIM);
            });
        });
    });
    panel(ui, "MIXER", c, FULL_W, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let sw = (FULL_W - 12.0 - 15.0 * 4.0) / 16.0;
            for i in 0..NUM_MIDI_PARTS {
                let sel = st.part == i;
                let part = Arc::make_mut(&mut p.parts[i]);
                let en = part.enabled;
                let frame = egui::Frame::new().fill(if sel { CONTROL } else if en { PANEL2 } else { PANEL }).corner_radius(CornerRadius::same(3)).inner_margin(egui::Margin::same(3));
                frame.show(ui, |ui| ui.vertical(|ui| {
                    ui.set_width(sw - 6.0);
                    ui.spacing_mut().item_spacing = vec2(2.0, 3.0);
                    ui.vertical_centered(|ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(sw - 6.0, 30.0), Sense::click());
                        let pp = ui.painter();
                        pp.text(pos2(r.center().x, r.top() + 1.0), Align2::CENTER_TOP, format!("{}", i + 1), font(11.0), if sel { ZYN } else if en { TEXT } else { FAINT });
                        let nm = if part.name.is_empty() { "-".to_string() } else { part.name.clone() };
                        let nm: String = if nm.chars().count() > 10 { nm.chars().take(9).collect::<String>() + "…" } else { nm };
                        pp.text(pos2(r.center().x, r.bottom() - 1.0), Align2::CENTER_BOTTOM, nm, font(8.0), if en { DIM } else { FAINT });
                        if resp.clicked() {
                            st.part = i;
                            st.kit = 0;
                        }
                        let mut e = part.enabled;
                        if button(ui, if e { "ON" } else { "OFF" }, e, c, vec2(sw - 12.0, 16.0)).clicked() {
                            e = !e;
                            part.enabled = e;
                            ch = true;
                        }
                        ch |= knobf(ui, "Volume", &mut part.volume, -40.0, 13.333, 0.0, &|v| format!("{v:+.0}"), if en { c } else { FAINT });
                        ch |= knob(ui, "Pan", &mut part.panning, 64, if en { c } else { FAINT });
                        let mut chn = part.rcvchn + 1;
                        if knob_range(ui, "MIDI Ch", &mut chn, 1, 16, 1, if en { c } else { FAINT }) {
                            part.rcvchn = chn - 1;
                            ch = true;
                        }
                        let lvl_l = sh.part_peak[i * 2].get();
                        let lvl_r = sh.part_peak[i * 2 + 1].get();
                        let (mr, _) = ui.allocate_exact_size(vec2(sw - 12.0, 9.0), Sense::hover());
                        let half = Rect::from_min_size(mr.min, vec2(mr.width(), 4.0));
                        hbar(ui.painter(), half, lvl_l);
                        hbar(ui.painter(), half.translate(vec2(0.0, 5.0)), lvl_r);
                        let notes = sh.part_notes[i].load(std::sync::atomic::Ordering::Relaxed);
                        ui.label(egui::RichText::new(format!("{notes} notes")).size(7.5).color(if notes > 0 { METER_LO } else { FAINT }));
                    });
                    ui.add_space(2.0);
                    let (r, _) = ui.allocate_exact_size(vec2(sw - 6.0, 10.0), Sense::hover());
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, "SENDS", font(7.0), FAINT);
                    for s in 0..NUM_SYS_EFX {
                        let col = if p.sysefx[s].kind == 0 { FAINT } else { FX };
                        ch |= mini_slider(ui, &format!("S{}", s + 1), &mut p.sysefxvol[s][i], sw - 12.0, col);
                    }
                }));
            }
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(FULL_W - 12.0, 12.0), Sense::hover());
            ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(1.0_f32, LINE));
            ui.painter().text(pos2(r.left(), r.center().y + 2.0), Align2::LEFT_CENTER, "Click a strip's number to edit that part · sends feed the system effects (EFFECTS page)", font(8.5), FAINT);
        });
    });
    if ch {
        sh.edit(|q| *q = p);
    }
}

#[allow(dead_code)]
fn _unused(_: Color32, _: Rect) {}

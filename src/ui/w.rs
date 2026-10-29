//! Widgets bound to patch values (not host parameters). Each returns
//! `true` when the value changed, so pages can commit an edit.
//! Drag up/down to change, Shift for fine steps, double-click resets,
//! the wheel nudges.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::synth::envelope::{EnvMode, EnvelopeParams};
use crate::synth::filter::FilterParams;
use crate::synth::lfo::LfoParams;
use crate::synth::util::MAX_ENVELOPE_POINTS;

pub mod theme {
    use egui::Color32;
    pub const BG: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
    pub const PANEL: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
    pub const PANEL2: Color32 = Color32::from_rgb(0x2e, 0x2e, 0x2e);
    pub const LINE: Color32 = Color32::from_rgb(0x38, 0x38, 0x38);
    pub const HEADER: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
    pub const TEXT: Color32 = Color32::from_rgb(0xd8, 0xd8, 0xd8);
    pub const DIM: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x8a);
    pub const FAINT: Color32 = Color32::from_rgb(0x5c, 0x5c, 0x5c);
    pub const TRACK: Color32 = Color32::from_rgb(0x13, 0x13, 0x13);
    pub const CONTROL: Color32 = Color32::from_rgb(0x3b, 0x3b, 0x3b);
    pub const CONTROL_HOVER: Color32 = Color32::from_rgb(0x4a, 0x4a, 0x4a);
    pub const ON_TEXT: Color32 = Color32::from_rgb(0x16, 0x16, 0x16);
    /// ADDsynth: Ableton orange.
    pub const ADD: Color32 = Color32::from_rgb(0xff, 0x9a, 0x3c);
    /// SUBsynth: blue.
    pub const SUB: Color32 = Color32::from_rgb(0x5c, 0xc8, 0xf0);
    /// PADsynth: violet.
    pub const PAD: Color32 = Color32::from_rgb(0xb4, 0x8c, 0xf0);
    /// Effects / global: lime.
    pub const FX: Color32 = Color32::from_rgb(0xc8, 0xe0, 0x6a);
    /// Zyn's classic beige-yellow, used for the wordmark.
    pub const ZYN: Color32 = Color32::from_rgb(0xf0, 0xc8, 0x50);
    pub const METER_LO: Color32 = Color32::from_rgb(0x5f, 0xd0, 0x6a);
    pub const METER_MID: Color32 = Color32::from_rgb(0xf0, 0xd2, 0x4a);
    pub const METER_HI: Color32 = Color32::from_rgb(0xff, 0x55, 0x45);
}
use theme::*;

pub fn font(size: f32) -> FontId {
    FontId::proportional(size)
}

const KW: f32 = 46.0;
const KH: f32 = 58.0;

/// Generic rotary for a float in `min..=max`.
pub fn knobf(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32, default: f32, fmt: &dyn Fn(f32) -> String, color: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(KW, KH), Sense::click_and_drag());
    let mut t = ((*v - min) / (max - min)).clamp(0.0, 1.0);
    let old = *v;
    let acc = resp.id.with("acc");
    if resp.double_clicked() {
        *v = default;
        t = ((default - min) / (max - min)).clamp(0.0, 1.0);
    } else {
        if resp.drag_started() {
            ui.memory_mut(|m| m.data.insert_temp(acc, t));
        }
        if resp.dragged() {
            let fine = ui.input(|i| i.modifiers.shift);
            let d = -resp.drag_delta().y / 160.0 * if fine { 0.1 } else { 1.0 };
            let raw: f32 = ui.memory(|m| m.data.get_temp(acc)).unwrap_or(t);
            let raw = (raw + d).clamp(0.0, 1.0);
            ui.memory_mut(|m| m.data.insert_temp(acc, raw));
            t = raw;
            *v = min + t * (max - min);
        }
        if resp.hovered() && !resp.dragged() {
            let s = ui.input(|i| i.smooth_scroll_delta.y);
            if s.abs() > 0.5 {
                t = (t + s / 1500.0).clamp(0.0, 1.0);
                *v = min + t * (max - min);
            }
        }
    }
    draw_knob(ui, rect, &resp, label, t, min < 0.0 && max > 0.0 || (default - min).abs() > 1e-3 && (default - min) / (max - min) > 0.45 && (default - min) / (max - min) < 0.55, &fmt(*v), color);
    *v != old
}

fn draw_knob(ui: &Ui, rect: Rect, resp: &Response, label: &str, t: f32, bipolar: bool, text: &str, color: Color32) {
    let p = ui.painter();
    p.text(pos2(rect.center().x, rect.top()), Align2::CENTER_TOP, label, font(9.0), DIM);
    let r = 12.0;
    let c = pos2(rect.center().x, rect.top() + 13.0 + r + 2.0);
    const START: f32 = std::f32::consts::FRAC_PI_4 * 3.0;
    const SWEEP: f32 = std::f32::consts::FRAC_PI_2 * 3.0;
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        (0..=32).map(|i| {
            let a = START + SWEEP * (from + (to - from) * i as f32 / 32.0);
            c + vec2(a.cos(), a.sin()) * r
        }).collect()
    };
    p.add(Shape::line(arc(0.0, 1.0), Stroke::new(3.0_f32, TRACK)));
    let base = if bipolar { 0.5 } else { 0.0 };
    if (t - base).abs() > 0.002 {
        p.add(Shape::line(arc(base.min(t), base.max(t)), Stroke::new(3.0_f32, color)));
    }
    let active = resp.hovered() || resp.dragged();
    let a = START + SWEEP * t;
    p.circle_filled(c, r - 5.0, if active { CONTROL_HOVER } else { CONTROL });
    p.line_segment([c + vec2(a.cos(), a.sin()) * 2.0, c + vec2(a.cos(), a.sin()) * (r - 4.0)], Stroke::new(2.0_f32, TEXT));
    p.text(pos2(rect.center().x, rect.bottom()), Align2::CENTER_BOTTOM, text, font(9.0), if active { color } else { TEXT });
}

/// Rotary for Zyn's classic 0..127 byte parameters.
pub fn knob(ui: &mut Ui, label: &str, v: &mut u8, default: u8, color: Color32) -> bool {
    knob_range(ui, label, v, 0, 127, default, color)
}

pub fn knob_range(ui: &mut Ui, label: &str, v: &mut u8, min: u8, max: u8, default: u8, color: Color32) -> bool {
    let mut f = *v as f32;
    let fmt = |x: f32| format!("{}", x.round() as i32);
    if knobf(ui, label, &mut f, min as f32, max as f32, default as f32, &fmt, color) {
        let n = f.round().clamp(min as f32, max as f32) as u8;
        if n != *v {
            *v = n;
            return true;
        }
    }
    false
}

/// Integer rotary with a custom formatter.
pub fn knob_i(ui: &mut Ui, label: &str, v: &mut i32, min: i32, max: i32, default: i32, fmt: &dyn Fn(i32) -> String, color: Color32) -> bool {
    let mut f = *v as f32;
    let fm = |x: f32| fmt(x.round() as i32);
    if knobf(ui, label, &mut f, min as f32, max as f32, default as f32, &fm, color) {
        let n = f.round() as i32;
        if n != *v {
            *v = n;
            return true;
        }
    }
    false
}

pub fn button(ui: &mut Ui, text: &str, on: bool, color: Color32, size: Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if on { color } else if resp.hovered() { CONTROL_HOVER } else { CONTROL };
    ui.painter().rect_filled(rect, CornerRadius::same(2), fill);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font(9.5), if on { ON_TEXT } else { TEXT });
    resp
}

pub fn toggle(ui: &mut Ui, text: &str, v: &mut bool, color: Color32) -> bool {
    let w = (text.len() as f32 * 6.2 + 14.0).max(34.0);
    if button(ui, text, *v, color, vec2(w, 18.0)).clicked() {
        *v = !*v;
        return true;
    }
    false
}

/// Drop-down choice for an index.
pub fn choice(ui: &mut Ui, id: impl std::hash::Hash, label: &str, v: &mut u8, options: &[&str], width: f32) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(egui::RichText::new(label).size(9.0).color(DIM));
        let cur = options.get(*v as usize).copied().unwrap_or("?");
        egui::ComboBox::from_id_salt(id).selected_text(egui::RichText::new(cur).size(10.5)).width(width).show_ui(ui, |ui| {
            for (i, o) in options.iter().enumerate() {
                if ui.selectable_label(*v as usize == i, *o).clicked() && *v as usize != i {
                    *v = i as u8;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// Titled panel frame.
pub fn panel<R>(ui: &mut Ui, title: &str, color: Color32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let frame = egui::Frame::new().fill(PANEL).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin { left: 8, right: 8, top: 22, bottom: 6 });
    let r = frame.show(ui, |ui| add(ui));
    let rect = r.response.rect;
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), 17.0));
    let p = ui.painter();
    p.rect_filled(head, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, HEADER);
    p.rect_filled(Rect::from_min_size(head.min, vec2(3.0, head.height())), CornerRadius { nw: 4, ne: 0, sw: 0, se: 0 }, color);
    p.text(pos2(head.left() + 9.0, head.center().y), Align2::LEFT_CENTER, title, font(10.5), TEXT);
    r.inner
}

pub fn small_label(ui: &mut Ui, t: &str) {
    ui.label(egui::RichText::new(t).size(9.5).color(DIM));
}

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

fn env_value_norm(mode: EnvMode, v: u8) -> f32 {
    let _ = mode;
    v as f32 / 127.0
}

/// Envelope editor: graph (drag points in free mode) plus ADSR knobs.
pub fn envelope(ui: &mut Ui, id: &str, e: &mut EnvelopeParams, color: Color32) -> bool {
    let mut ch = false;
    ui.vertical(|ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(230.0, 70.0), Sense::click_and_drag());
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(2), TRACK);
        let n = (e.points as usize).clamp(1, MAX_ENVELOPE_POINTS);
        let mut total = 0.0f32;
        for i in 1..n {
            total += e.dt[i].max(0.0005).sqrt();
        }
        let total = total.max(1e-3);
        let mut pts = Vec::with_capacity(n);
        let mut x = 0.0f32;
        for i in 0..n {
            if i > 0 {
                x += e.dt[i].max(0.0005).sqrt();
            }
            let px = rect.left() + 4.0 + (rect.width() - 8.0) * x / total;
            let py = rect.bottom() - 4.0 - (rect.height() - 8.0) * env_value_norm(e.mode, e.val[i]);
            pts.push(pos2(px, py));
        }
        p.add(Shape::line(pts.clone(), Stroke::new(1.5_f32, color)));
        if e.sustain > 0 && (e.sustain as usize) < n {
            let sx = pts[e.sustain as usize].x;
            p.line_segment([pos2(sx, rect.top()), pos2(sx, rect.bottom())], Stroke::new(1.0_f32, FAINT));
        }
        for (i, q) in pts.iter().enumerate() {
            p.circle_filled(*q, if i == 0 { 2.5 } else { 3.0 }, TEXT);
        }
        // Drag points (only meaningful in free mode; dragging switches to it).
        let drag_id = resp.id.with("pt");
        if resp.drag_started() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let best = pts.iter().enumerate().min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos))).map(|(i, _)| i);
                ui.memory_mut(|m| m.data.insert_temp(drag_id, best.unwrap_or(0)));
            }
        }
        if resp.dragged() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let i: usize = ui.memory(|m| m.data.get_temp(drag_id)).unwrap_or(0);
                if i < n {
                    if !e.free_mode {
                        e.free_mode = true;
                    }
                    let nv = (((rect.bottom() - 4.0 - pos.y) / (rect.height() - 8.0)) * 127.0).round().clamp(0.0, 127.0) as u8;
                    if nv != e.val[i] {
                        e.val[i] = nv;
                        ch = true;
                    }
                    if i > 0 {
                        let d = resp.drag_delta().x / (rect.width() - 8.0) * total;
                        let s = (e.dt[i].max(0.0005).sqrt() + d).clamp(0.0, 4.0);
                        let ndt = s * s;
                        if (ndt - e.dt[i]).abs() > 1e-6 {
                            e.dt[i] = ndt;
                            ch = true;
                        }
                    }
                }
            }
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let secs = |v: f32| if v < 1.0 { format!("{:.0}ms", v * 1000.0) } else { format!("{v:.2}s") };
            let secs = &secs;
            if !e.free_mode {
                let mode = e.mode;
                let has_aval = matches!(mode, EnvMode::AsrFreqLfo | EnvMode::AsrBw | EnvMode::AdsrFilter);
                let mut c2 = false;
                if has_aval {
                    c2 |= knob(ui, "A.val", &mut e.a_val, 64, color);
                }
                c2 |= knobf(ui, "A.dt", &mut e.a_dt, 0.0, 41.0, 0.0, &|v| secs(v), color);
                if matches!(mode, EnvMode::AdsrFilter) {
                    c2 |= knob(ui, "D.val", &mut e.d_val, 64, color);
                }
                if matches!(mode, EnvMode::AdsrLin | EnvMode::AdsrDb | EnvMode::AdsrFilter) {
                    c2 |= knobf(ui, "D.dt", &mut e.d_dt, 0.0, 41.0, 0.0, &|v| secs(v), color);
                }
                if matches!(mode, EnvMode::AdsrLin | EnvMode::AdsrDb) {
                    c2 |= knob(ui, "S.val", &mut e.s_val, 127, color);
                }
                c2 |= knobf(ui, "R.dt", &mut e.r_dt, 0.0, 41.0, 0.0, &|v| secs(v), color);
                if has_aval {
                    c2 |= knob(ui, "R.val", &mut e.r_val, 64, color);
                }
                if c2 {
                    e.convert_to_free();
                    ch = true;
                }
            } else {
                let mut pts = e.points as i32;
                if knob_i(ui, "Points", &mut pts, 2, MAX_ENVELOPE_POINTS as i32, 4, &|v| v.to_string(), color) {
                    e.points = pts as u8;
                    ch = true;
                }
                let mut s = e.sustain as i32;
                if knob_i(ui, "Sustain", &mut s, 0, e.points as i32 - 1, 2, &|v| if v == 0 { "off".into() } else { v.to_string() }, color) {
                    e.sustain = s as u8;
                    ch = true;
                }
            }
            ch |= knob(ui, "Stretch", &mut e.stretch, 64, color);
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ch |= toggle(ui, "FREE", &mut e.free_mode, color);
            ch |= toggle(ui, "F.REL", &mut e.forced_release, color);
            if matches!(e.mode, EnvMode::AdsrLin | EnvMode::AdsrDb) {
                ch |= toggle(ui, "LIN", &mut e.linear, color);
            }
            ch |= toggle(ui, "REPEAT", &mut e.repeating, color);
            let _ = id;
        });
    });
    ch
}

// ---------------------------------------------------------------------------
// LFO
// ---------------------------------------------------------------------------

pub const LFO_TYPES: [&str; 8] = ["Sine", "Tri", "Square", "Ramp+", "Ramp-", "Exp1", "Exp2", "Random"];

pub fn lfo(ui: &mut Ui, id: &str, l: &mut LfoParams, color: Color32) -> bool {
    let mut ch = false;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ch |= knobf(ui, "Freq", &mut l.freq, 0.0, 85.25, 3.0, &|v| format!("{v:.2}Hz"), color);
            ch |= knob(ui, "Depth", &mut l.intensity, 0, color);
            ch |= knob(ui, "Start", &mut l.startphase, 64, color);
            ch |= knobf(ui, "Delay", &mut l.delay, 0.0, 4.0, 0.0, &|v| format!("{v:.2}s"), color);
            ch |= knob(ui, "Stretch", &mut l.stretch, 64, color);
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ch |= knob(ui, "A.Rnd", &mut l.randomness, 0, color);
            ch |= knob(ui, "F.Rnd", &mut l.freqrand, 0, color);
            ch |= knobf(ui, "FadeIn", &mut l.fadein, 0.0, 10.0, 0.0, &|v| format!("{v:.2}s"), color);
            ch |= knobf(ui, "FadeOut", &mut l.fadeout, 0.0, 10.0, 10.0, &|v| if v >= 10.0 { "off".into() } else { format!("{v:.2}s") }, color);
            ch |= knob(ui, "Cutoff", &mut l.cutoff, 127, color);
        });
        ui.horizontal(|ui| {
            ch |= choice(ui, (id, "type"), "Shape", &mut l.lfotype, &LFO_TYPES, 70.0);
            ch |= toggle(ui, "CONT", &mut l.continous, color);
            let mut num = l.numerator as u8;
            let mut den = l.denominator as u8;
            if knob_range(ui, "Sync N", &mut num, 0, 99, 0, color) {
                l.numerator = num as i32;
                ch = true;
            }
            if knob_range(ui, "Sync D", &mut den, 0, 99, 4, color) {
                l.denominator = den as i32;
                ch = true;
            }
        });
    });
    ch
}

// ---------------------------------------------------------------------------
// Filter
// ---------------------------------------------------------------------------

pub const FILTER_CATS: [&str; 5] = ["Analog", "Formant", "StVarF", "Moog", "Comb"];
pub const ANALOG_TYPES: [&str; 9] = ["LPF1", "HPF1", "LPF2", "HPF2", "BPF2", "NOTCH2", "PEAK", "LoShelf", "HiShelf"];
pub const SV_TYPES: [&str; 4] = ["LPF", "HPF", "BPF", "NOTCH"];
pub const MOOG_TYPES: [&str; 3] = ["HP", "BP", "LP"];
pub const COMB_TYPES: [&str; 6] = ["BWD", "FWD", "Both", "BWD-N", "FWD-N", "Both-N"];

pub fn filter(ui: &mut Ui, id: &str, f: &mut FilterParams, color: Color32) -> bool {
    let mut ch = false;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ch |= choice(ui, (id, "cat"), "Class", &mut f.category, &FILTER_CATS, 70.0);
            let types: &[&str] = match f.category {
                1 => &[],
                2 => &SV_TYPES,
                3 => &MOOG_TYPES,
                4 => &COMB_TYPES,
                _ => &ANALOG_TYPES,
            };
            if !types.is_empty() {
                if f.ftype as usize >= types.len() {
                    f.ftype = 0;
                }
                ch |= choice(ui, (id, "type"), "Type", &mut f.ftype, types, 70.0);
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut lf = f.basefreq.log2();
            if knobf(ui, "Cutoff", &mut lf, 31.25f32.log2(), 32000f32.log2(), 1000f32.log2(), &|v| {
                let hz = 2f32.powf(v);
                if hz >= 1000.0 { format!("{:.1}k", hz / 1000.0) } else { format!("{hz:.0}") }
            }, color) {
                f.basefreq = 2f32.powf(lf);
                ch = true;
            }
            let mut lq = f.baseq.max(0.1).log10();
            if knobf(ui, "Q", &mut lq, -1.0, 3.0, 0.0, &|v| format!("{:.2}", 10f32.powf(v)), color) {
                f.baseq = 10f32.powf(lq);
                ch = true;
            }
            ch |= knobf(ui, "Track", &mut f.freqtracking, -100.0, 100.0, 0.0, &|v| format!("{v:+.0}%"), color);
            ch |= knobf(ui, "Gain", &mut f.gain, -30.0, 30.0, 0.0, &|v| format!("{v:+.1}dB"), color);
            if f.category != 1 {
                let mut st = f.stages + 1;
                if knob_range(ui, "Stages", &mut st, 1, 5, 1, color) {
                    f.stages = st - 1;
                    ch = true;
                }
            }
        });
        if f.category == 1 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ch |= knob_range(ui, "Formants", &mut f.numformants, 1, 12, 3, color);
                ch |= knob(ui, "Slow", &mut f.formantslowness, 64, color);
                ch |= knob(ui, "Clear", &mut f.vowelclearness, 64, color);
                ch |= knob(ui, "Center", &mut f.centerfreq, 64, color);
                ch |= knob(ui, "Octaves", &mut f.octavesfreq, 64, color);
                ch |= knob_range(ui, "Seq", &mut f.sequencesize, 1, 8, 3, color);
                ch |= knob(ui, "Stretch", &mut f.sequencestretch, 40, color);
            });
            ui.horizontal(|ui| {
                small_label(ui, "Sequence:");
                for s in 0..f.sequencesize as usize {
                    let mut v = f.sequence[s];
                    if knob_range(ui, &format!("#{}", s + 1), &mut v, 0, 5, 0, color) {
                        f.sequence[s] = v;
                        ch = true;
                    }
                }
                ch |= toggle(ui, "REV", &mut f.sequencereversed, color);
            });
        }
        if f.category == 4 {
            ui.horizontal(|ui| {
                ch |= knob(ui, "LPF", &mut f.lpf, 127, color);
                ch |= knob(ui, "HPF", &mut f.hpf, 0, color);
            });
        }
    });
    ch
}

// ---------------------------------------------------------------------------
// Bar editors (oscillator harmonics, SUB harmonics, resonance)
// ---------------------------------------------------------------------------

/// Vertical bars for byte values. `center` = value drawn as empty (64 for
/// bipolar harmonics, 0 for SUB). Drag across bars to paint; right-click
/// resets a bar. Returns true on change.
pub fn bars(ui: &mut Ui, id: impl std::hash::Hash, vals: &mut [u8], center: u8, size: Vec2, color: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), TRACK);
    let n = vals.len();
    let w = rect.width() / n as f32;
    let cy = rect.bottom() - rect.height() * center as f32 / 127.0;
    if center > 0 {
        p.line_segment([pos2(rect.left(), cy), pos2(rect.right(), cy)], Stroke::new(1.0_f32, LINE));
    }
    for (i, v) in vals.iter().enumerate() {
        let x0 = rect.left() + i as f32 * w;
        let y = rect.bottom() - rect.height() * *v as f32 / 127.0;
        let (top, bot) = if y < cy { (y, cy) } else { (cy, y) };
        if *v != center {
            p.rect_filled(Rect::from_min_max(pos2(x0 + 0.5, top), pos2(x0 + w - 0.5, bot.max(top + 1.0))), CornerRadius::ZERO, color);
        }
    }
    let _ = id;
    let mut ch = false;
    if let Some(pos) = resp.interact_pointer_pos() {
        if rect.contains(pos) || resp.dragged() {
            let i = (((pos.x - rect.left()) / w).floor() as i64).clamp(0, n as i64 - 1) as usize;
            if resp.dragged() || resp.clicked() {
                let v = (((rect.bottom() - pos.y) / rect.height()) * 127.0).round().clamp(0.0, 127.0) as u8;
                if vals[i] != v {
                    vals[i] = v;
                    ch = true;
                }
            }
            if resp.secondary_clicked() && vals[i] != center {
                vals[i] = center;
                ch = true;
            }
            if resp.hovered() {
                p.text(pos2(rect.left() + 3.0, rect.top() + 2.0), Align2::LEFT_TOP, format!("#{} = {}", i + 1, vals[i]), font(9.0), DIM);
            }
        }
    }
    ch
}

/// Read-only plot of a waveform.
pub fn wave_plot(ui: &mut Ui, data: &[f32], size: Vec2, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), TRACK);
    p.line_segment([pos2(rect.left(), rect.center().y), pos2(rect.right(), rect.center().y)], Stroke::new(1.0_f32, LINE));
    if data.is_empty() {
        return;
    }
    let m = data.iter().fold(1e-6f32, |a, v| a.max(v.abs()));
    let n = data.len();
    let pts: Vec<Pos2> = (0..rect.width() as usize)
        .map(|x| {
            let i = x * n / rect.width() as usize;
            pos2(rect.left() + x as f32, rect.center().y - data[i.min(n - 1)] / m * (rect.height() * 0.45))
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(1.3_f32, color)));
}

/// Read-only spectrum bars (linear magnitudes, drawn in dB).
pub fn spectrum_plot(ui: &mut Ui, mags: &[f32], size: Vec2, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), TRACK);
    let m = mags.iter().fold(1e-9f32, |a, v| a.max(*v));
    let n = mags.len().max(1);
    let w = rect.width() / n as f32;
    for (i, v) in mags.iter().enumerate() {
        let db = 20.0 * (v / m + 1e-9).log10();
        let t = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        if t > 0.0 {
            let x = rect.left() + i as f32 * w;
            p.rect_filled(Rect::from_min_max(pos2(x, rect.bottom() - t * rect.height()), pos2(x + w.max(1.0) - 0.3, rect.bottom())), CornerRadius::ZERO, color);
        }
    }
}

/// Stereo peak meter (-48..+6 dBFS), vertical.
pub fn meter(ui: &Ui, rect: Rect, l: f32, r: f32) {
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), TRACK);
    let bw = (rect.width() - 3.0) / 2.0;
    for (k, v) in [l, r].iter().enumerate() {
        let db = 20.0 * v.max(1e-6).log10();
        let t = ((db + 48.0) / 54.0).clamp(0.0, 1.0);
        let x = rect.left() + 1.0 + k as f32 * (bw + 1.0);
        let col = if db > 0.0 { METER_HI } else if db > -12.0 { METER_MID } else { METER_LO };
        p.rect_filled(Rect::from_min_max(pos2(x, rect.bottom() - t * rect.height()), pos2(x + bw, rect.bottom())), CornerRadius::ZERO, col);
    }
    p.rect_stroke(rect, CornerRadius::same(2), Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
}

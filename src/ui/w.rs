//! Widgets bound to patch values (not host parameters). Each returns
//! `true` when the value changed, so pages can commit an edit.
//! Drag up/down to change, Shift for fine steps, double-click resets,
//! the wheel nudges.
//!
//! Everything sits on one grid: a control is a `KW` x `KH` cell with its
//! label on top and its value at the bottom, so rows of knobs, drop-downs
//! and switches line up without extra spacing.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::synth::envelope::{EnvMode, EnvelopeParams};
use crate::synth::filter::FilterParams;
use crate::synth::lfo::LfoParams;
use crate::synth::util::MAX_ENVELOPE_POINTS;

pub mod theme {
    use egui::Color32;
    pub const BG: Color32 = Color32::from_rgb(0x17, 0x17, 0x19);
    pub const PANEL: Color32 = Color32::from_rgb(0x21, 0x21, 0x24);
    pub const PANEL2: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1f);
    pub const LINE: Color32 = Color32::from_rgb(0x33, 0x33, 0x38);
    pub const HEADER: Color32 = Color32::from_rgb(0x29, 0x29, 0x2d);
    pub const TEXT: Color32 = Color32::from_rgb(0xdc, 0xdc, 0xe0);
    pub const DIM: Color32 = Color32::from_rgb(0x8c, 0x8c, 0x94);
    pub const FAINT: Color32 = Color32::from_rgb(0x58, 0x58, 0x60);
    pub const TRACK: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
    pub const CONTROL: Color32 = Color32::from_rgb(0x34, 0x34, 0x39);
    pub const CONTROL_HOVER: Color32 = Color32::from_rgb(0x44, 0x44, 0x4a);
    pub const ON_TEXT: Color32 = Color32::from_rgb(0x14, 0x14, 0x16);
    /// ADDsynth: warm orange.
    pub const ADD: Color32 = Color32::from_rgb(0xff, 0x9a, 0x3c);
    /// SUBsynth: cyan.
    pub const SUB: Color32 = Color32::from_rgb(0x4c, 0xc8, 0xf0);
    /// PADsynth: violet.
    pub const PAD: Color32 = Color32::from_rgb(0xb4, 0x8c, 0xf8);
    /// Effects: lime.
    pub const FX: Color32 = Color32::from_rgb(0xc4, 0xe0, 0x5c);
    /// Part / instrument: Zyn's classic beige-yellow (also the wordmark).
    pub const ZYN: Color32 = Color32::from_rgb(0xf0, 0xc8, 0x50);
    /// Master / mixer: teal.
    pub const MIX: Color32 = Color32::from_rgb(0x3c, 0xd0, 0xb0);
    /// Community banks in the browser.
    pub const COMMUNITY: Color32 = Color32::from_rgb(0xff, 0x6e, 0x8a);
    pub const METER_LO: Color32 = Color32::from_rgb(0x5f, 0xd0, 0x6a);
    pub const METER_MID: Color32 = Color32::from_rgb(0xf0, 0xd2, 0x4a);
    pub const METER_HI: Color32 = Color32::from_rgb(0xff, 0x55, 0x45);

    // Function colours: the same job has the same colour on every page.
    /// Amplitude sections.
    pub const AMP: Color32 = Color32::from_rgb(0xff, 0x78, 0x6a);
    /// Frequency sections.
    pub const FRQ: Color32 = Color32::from_rgb(0x6c, 0xa8, 0xff);
    /// Filter sections.
    pub const FLT: Color32 = Color32::from_rgb(0x72, 0xdc, 0x8e);
    /// Modulators and controllers.
    pub const MODC: Color32 = Color32::from_rgb(0xf2, 0x7c, 0xc4);
    /// Resonance.
    pub const RES: Color32 = Color32::from_rgb(0xf0, 0xc8, 0x50);

    /// Linear blend from `a` to `b`.
    pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
        let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
    }

    /// LFOs are the lighter shade of their section's colour.
    pub fn lfo_tint(c: Color32) -> Color32 {
        mix(c, Color32::WHITE, 0.42)
    }

    /// Each of the 16 parts has its own hue, starting at Zyn's yellow.
    pub fn part_color(i: usize) -> Color32 {
        let h = (0.125 + i as f32 / 16.0).fract();
        egui::ecolor::Hsva::new(h, 0.82, 0.92, 1.0).into()
    }

    /// Each effect type has its own colour (0 = empty slot).
    pub fn fx_color(kind: u8) -> Color32 {
        match kind {
            1 => Color32::from_rgb(0x8f, 0x9c, 0xff), // reverb
            2 => Color32::from_rgb(0x52, 0xd6, 0xc8), // echo
            3 => Color32::from_rgb(0x62, 0xc6, 0xff), // chorus
            4 => Color32::from_rgb(0xee, 0x7f, 0xd0), // phaser
            5 => Color32::from_rgb(0xb8, 0x8c, 0xff), // alienwah
            6 => Color32::from_rgb(0xff, 0x6f, 0x50), // distortion
            7 => Color32::from_rgb(0xe9, 0xd4, 0x5a), // EQ
            8 => Color32::from_rgb(0x8f, 0xe0, 0x6a), // dynamic filter
            9 => Color32::from_rgb(0xff, 0xb2, 0x7a), // sympathetic
            10 => FX,                                 // reverse
            _ => FAINT,
        }
    }
}
use theme::*;

pub fn font(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Control cell size.
pub const KW: f32 = 34.0;
pub const KH: f32 = 40.0;
const LABEL: f32 = 8.5;

// ---------------------------------------------------------------------------
// Knobs
// ---------------------------------------------------------------------------

/// Generic rotary for a float in `min..=max`.
pub fn knobf(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32, default: f32, fmt: &dyn Fn(f32) -> String, color: Color32) -> bool {
    knob_core(ui, label, v, min, max, default, fmt, color, true)
}

/// A byte rotary with no meaningful default (effect parameters, whose
/// defaults come from the preset): its value is never dimmed.
pub fn knob_live(ui: &mut Ui, label: &str, v: &mut u8, color: Color32) -> bool {
    let mut f = *v as f32;
    let d = f;
    let fmt = |x: f32| format!("{}", x.round() as i32);
    if knob_core(ui, label, &mut f, 0.0, 127.0, d, &fmt, color, false) {
        let n = f.round().clamp(0.0, 127.0) as u8;
        if n != *v {
            *v = n;
            return true;
        }
    }
    false
}

fn knob_core(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32, default: f32, fmt: &dyn Fn(f32) -> String, color: Color32, has_default: bool) -> bool {
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
    let dn = (default - min) / (max - min);
    let bipolar = (min < 0.0 && max > 0.0) || (dn > 0.45 && dn < 0.55);
    let edited = !has_default || (t - dn).abs() > 0.004;
    draw_knob(ui, rect, &resp, label, t, bipolar, &fmt(*v), color, edited);
    *v != old
}

/// `edited` = the value differs from its default: its readout is bright,
/// so what shapes a patch can be read at a glance.
pub fn draw_knob(ui: &Ui, rect: Rect, resp: &Response, label: &str, t: f32, bipolar: bool, text: &str, color: Color32, edited: bool) {
    let p = ui.painter();
    let active = resp.hovered() || resp.dragged();
    p.text(pos2(rect.center().x, rect.top()), Align2::CENTER_TOP, label, font(LABEL), if active { TEXT } else { DIM });
    let r = 9.5;
    let c = pos2(rect.center().x, rect.top() + 20.0);
    const START: f32 = std::f32::consts::FRAC_PI_4 * 3.0;
    const SWEEP: f32 = std::f32::consts::FRAC_PI_2 * 3.0;
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        (0..=24)
            .map(|i| {
                let a = START + SWEEP * (from + (to - from) * i as f32 / 24.0);
                c + vec2(a.cos(), a.sin()) * r
            })
            .collect()
    };
    p.add(Shape::line(arc(0.0, 1.0), Stroke::new(2.5_f32, TRACK)));
    let base = if bipolar { 0.5 } else { 0.0 };
    if (t - base).abs() > 0.002 {
        p.add(Shape::line(arc(base.min(t), base.max(t)), Stroke::new(2.5_f32, color)));
    }
    let a = START + SWEEP * t;
    p.circle_filled(c, r - 4.0, if active { CONTROL_HOVER } else { CONTROL });
    p.line_segment([c + vec2(a.cos(), a.sin()) * 1.5, c + vec2(a.cos(), a.sin()) * (r - 3.5)], Stroke::new(1.6_f32, TEXT));
    p.text(pos2(rect.center().x, rect.bottom()), Align2::CENTER_BOTTOM, text, font(LABEL), if active { color } else if edited { TEXT } else { mix(DIM, FAINT, 0.35) });
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

// ---------------------------------------------------------------------------
// Buttons, switches, drop-downs
// ---------------------------------------------------------------------------

pub fn button_face(p: &egui::Painter, rect: Rect, on: bool, hovered: bool, text: &str, size: f32, color: Color32) {
    let fill = if on { color } else if hovered { CONTROL_HOVER } else { CONTROL };
    p.rect_filled(rect, CornerRadius::same(3), fill);
    p.text(rect.center(), Align2::CENTER_CENTER, text, font(size), if on { ON_TEXT } else { mix(DIM, color, 0.45) });
}

pub fn button(ui: &mut Ui, text: &str, on: bool, color: Color32, size: Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    button_face(ui.painter(), rect, on, resp.hovered(), text, 9.0, color);
    resp
}

/// Outlined action button (file operations and the like).
pub fn action(ui: &mut Ui, text: &str, color: Color32, size: Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(3), if resp.hovered() { CONTROL_HOVER } else { CONTROL });
    p.rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.0_f32, color.gamma_multiply(if resp.hovered() { 0.9 } else { 0.45 })), StrokeKind::Inside);
    p.text(rect.center(), Align2::CENTER_CENTER, text, font(9.0), mix(TEXT, color, 0.55));
    resp
}

fn pill_width(text: &str) -> f32 {
    (text.len() as f32 * 5.6 + 12.0).max(26.0)
}

/// Small latching switch (16 px high) for rows of flags.
pub fn toggle(ui: &mut Ui, text: &str, v: &mut bool, color: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(pill_width(text), 16.0), Sense::click());
    button_face(ui.painter(), rect, *v, resp.hovered(), text, 8.5, color);
    if resp.clicked() {
        *v = !*v;
        return true;
    }
    false
}

/// A switch occupying a full control cell, so it lines up with knobs.
pub fn toggle_cell(ui: &mut Ui, label: &str, text: &str, v: &mut bool, color: Color32) -> bool {
    let w = pill_width(text).max(KW);
    let (rect, _) = ui.allocate_exact_size(vec2(w, KH), Sense::hover());
    ui.painter().text(pos2(rect.center().x, rect.top()), Align2::CENTER_TOP, label, font(LABEL), DIM);
    let b = Rect::from_center_size(pos2(rect.center().x, rect.top() + 21.0), vec2(w - 2.0, 16.0));
    let resp = ui.interact(b, ui.id().with(("tcell", label, text, rect.min.x as i32, rect.min.y as i32)), Sense::click());
    button_face(ui.painter(), b, *v, resp.hovered(), text, 8.5, color);
    if resp.clicked() {
        *v = !*v;
        return true;
    }
    false
}

/// Two switches stacked in one control cell.
pub fn toggle_pair(ui: &mut Ui, a: (&str, &mut bool), b: (&str, &mut bool), color: Color32) -> bool {
    let w = pill_width(a.0).max(pill_width(b.0));
    let (rect, _) = ui.allocate_exact_size(vec2(w, KH), Sense::hover());
    let mut ch = false;
    for (i, (text, v)) in [a, b].into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left(), rect.top() + 3.0 + i as f32 * 18.0), vec2(w, 16.0));
        let resp = ui.interact(r, ui.id().with(("tpair", text, rect.min.x as i32, rect.min.y as i32)), Sense::click());
        button_face(ui.painter(), r, *v, resp.hovered(), text, 8.5, color);
        if resp.clicked() {
            *v = !*v;
            ch = true;
        }
    }
    ch
}

/// Drop-down choice in a control cell (label on top).
pub fn choice(ui: &mut Ui, id: impl std::hash::Hash, label: &str, v: &mut u8, options: &[&str], width: f32) -> bool {
    let mut changed = false;
    let (rect, _) = ui.allocate_exact_size(vec2(width, KH), Sense::hover());
    ui.painter().text(pos2(rect.left() + 1.0, rect.top()), Align2::LEFT_TOP, label, font(LABEL), DIM);
    let r = Rect::from_min_size(pos2(rect.left(), rect.top() + 13.0), vec2(width, 17.0));
    let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::left_to_right(egui::Align::Center)));
    changed |= combo(&mut cui, id, v, options, width);
    changed
}

/// Bare drop-down (no label), 17 px high.
pub fn combo(ui: &mut Ui, id: impl std::hash::Hash, v: &mut u8, options: &[&str], width: f32) -> bool {
    let mut changed = false;
    let cur = options.get(*v as usize).copied().unwrap_or("?");
    ui.spacing_mut().interact_size.y = 17.0;
    ui.spacing_mut().button_padding = vec2(4.0, 1.0);
    egui::ComboBox::from_id_salt(id)
        .selected_text(egui::RichText::new(cur).size(9.5))
        .width(width - 2.0)
        .height(360.0)
        .show_ui(ui, |ui| {
            for (i, o) in options.iter().enumerate() {
                if ui.selectable_label(*v as usize == i, egui::RichText::new(*o).size(10.0)).clicked() && *v as usize != i {
                    *v = i as u8;
                    changed = true;
                }
            }
        });
    changed
}

/// Segmented tab strip; returns the clicked index. `marks[i]` draws a dot
/// (used for "this section is enabled").
pub fn segmented(ui: &mut Ui, items: &[&str], sel: usize, marks: &[bool], color: Color32, w: f32, h: f32) -> Option<usize> {
    segmented_c(ui, items, sel, marks, &[color], w, h)
}

/// `segmented` with one colour per item (the last one repeats).
pub fn segmented_c(ui: &mut Ui, items: &[&str], sel: usize, marks: &[bool], colors: &[Color32], w: f32, h: f32) -> Option<usize> {
    let mut out = None;
    let (rect, _) = ui.allocate_exact_size(vec2(w * items.len() as f32, h), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(3), TRACK);
    for (i, t) in items.iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left() + i as f32 * w, rect.top()), vec2(w, h)).shrink(1.0);
        let resp = ui.interact(r, ui.id().with(("seg", t, rect.min.x as i32, rect.min.y as i32, i)), Sense::click());
        let on = i == sel;
        let color = colors[i.min(colors.len() - 1)];
        let fill = if on { color } else if resp.hovered() { CONTROL_HOVER } else { PANEL };
        p.rect_filled(r, CornerRadius::same(2), fill);
        let tc = if on { ON_TEXT } else { mix(TEXT, color, 0.5) };
        let mark = marks.get(i).copied().unwrap_or(false);
        let tx = if mark { r.center().x + 3.0 } else { r.center().x };
        p.text(pos2(tx, r.center().y), Align2::CENTER_CENTER, *t, font(8.5), tc);
        if mark {
            let galley_w = t.len() as f32 * 2.6;
            p.circle_filled(pos2(tx - galley_w - 6.0, r.center().y), 2.2, if on { ON_TEXT } else { color });
        }
        if resp.clicked() {
            out = Some(i);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Panels and layout helpers
// ---------------------------------------------------------------------------

pub const HEAD_H: f32 = 17.0;

/// Titled panel with a fixed width. The colour stripe marks the engine.
pub fn panel<R>(ui: &mut Ui, title: &str, color: Color32, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    panel_h(ui, title, color, width, None, add)
}

/// Panel with fixed width and optional fixed height.
pub fn panel_h<R>(ui: &mut Ui, title: &str, color: Color32, width: f32, height: Option<f32>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let frame = egui::Frame::new().fill(mix(PANEL, color, 0.035)).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin { left: 6, right: 6, top: (HEAD_H + 5.0) as i8, bottom: 5 });
    let r = frame.show(ui, |ui| {
        ui.set_width(width - 12.0);
        if let Some(h) = height {
            ui.set_min_height(h - HEAD_H - 10.0);
        }
        ui.spacing_mut().item_spacing = vec2(2.0, 3.0);
        ui.vertical(|ui| {
            ui.set_width(width - 12.0);
            add(ui)
        })
        .inner
    });
    let rect = r.response.rect;
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), HEAD_H));
    let p = ui.painter();
    p.rect_filled(head, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, mix(HEADER, color, 0.2));
    p.rect_filled(Rect::from_min_size(head.min, vec2(3.0, head.height())), CornerRadius { nw: 4, ne: 0, sw: 0, se: 0 }, color);
    p.text(pos2(head.left() + 9.0, head.center().y), Align2::LEFT_CENTER, title, font(9.5), mix(TEXT, color, 0.6));
    r.inner
}

/// A block with a vertical tag on its left ("ENV", "LFO" ...).
pub fn tagged<R>(ui: &mut Ui, tag: &str, color: Color32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let r = ui.horizontal_top(|ui| {
        let (strip, _) = ui.allocate_exact_size(vec2(11.0, KH), Sense::hover());
        let inner = ui.vertical(|ui| add(ui));
        let h = inner.response.rect.height();
        let strip = Rect::from_min_size(strip.min, vec2(11.0, h));
        let p = ui.painter();
        p.rect_filled(Rect::from_min_size(strip.min + vec2(4.0, 0.0), vec2(2.0, h)), CornerRadius::same(1), color.gamma_multiply(0.55));
        let tc = mix(DIM, color, 0.7);
        let galley = p.layout_no_wrap(tag.to_string(), font(7.5), tc);
        let gw = galley.size().x;
        let pos = pos2(strip.left() - 1.0, strip.center().y + gw * 0.5);
        p.add(egui::epaint::TextShape::new(pos, galley, tc).with_angle(-std::f32::consts::FRAC_PI_2));
        inner.inner
    });
    r.inner
}

pub fn row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        add(ui);
    });
}

/// Thin vertical divider inside a row of cells.
pub fn vsep(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(7.0, KH), Sense::hover());
    ui.painter().line_segment([pos2(r.center().x, r.top() + 4.0), pos2(r.center().x, r.bottom() - 4.0)], Stroke::new(1.0_f32, LINE));
}

pub fn small_label(ui: &mut Ui, t: &str) {
    ui.label(egui::RichText::new(t).size(8.5).color(DIM));
}

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

/// Envelope editor: graph (drag points; dragging switches to free mode),
/// the ADSR knobs of its mode and a row of flags under the graph.
pub fn envelope(ui: &mut Ui, id: &str, e: &mut EnvelopeParams, color: Color32) -> bool {
    let mut ch = false;
    let _ = id;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ch |= env_graph(ui, e, color, vec2(96.0, KH));
            ui.add_space(2.0);
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
            ui.spacing_mut().item_spacing.x = 2.0;
            ch |= toggle(ui, "FREE", &mut e.free_mode, color);
            ch |= toggle(ui, "F.REL", &mut e.forced_release, color);
            if matches!(e.mode, EnvMode::AdsrLin | EnvMode::AdsrDb) {
                ch |= toggle(ui, "LIN", &mut e.linear, color);
            }
            ch |= toggle(ui, "REPEAT", &mut e.repeating, color);
        });
    });
    ch
}

fn env_graph(ui: &mut Ui, e: &mut EnvelopeParams, color: Color32, size: Vec2) -> bool {
    let mut ch = false;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(3), TRACK);
    let n = (e.points as usize).clamp(1, MAX_ENVELOPE_POINTS);
    let mut total = 0.0f32;
    for i in 1..n {
        total += e.dt[i].max(0.0005).sqrt();
    }
    let total = total.max(1e-3);
    let inner = rect.shrink(4.0);
    let mut pts = Vec::with_capacity(n);
    let mut x = 0.0f32;
    for i in 0..n {
        if i > 0 {
            x += e.dt[i].max(0.0005).sqrt();
        }
        pts.push(pos2(inner.left() + inner.width() * x / total, inner.bottom() - inner.height() * e.val[i] as f32 / 127.0));
    }
    // filled area under the curve
    if pts.len() > 1 {
        for w in pts.windows(2) {
            let quad = vec![w[0], w[1], pos2(w[1].x, inner.bottom()), pos2(w[0].x, inner.bottom())];
            p.add(Shape::convex_polygon(quad, color.gamma_multiply(0.14), Stroke::NONE));
        }
    }
    p.add(Shape::line(pts.clone(), Stroke::new(1.4_f32, color)));
    if e.sustain > 0 && (e.sustain as usize) < n {
        let sx = pts[e.sustain as usize].x;
        p.line_segment([pos2(sx, rect.top() + 2.0), pos2(sx, rect.bottom() - 2.0)], Stroke::new(1.0_f32, FAINT));
    }
    for q in pts.iter() {
        p.circle_filled(*q, 2.0, TEXT);
    }
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
                let nv = (((inner.bottom() - pos.y) / inner.height()) * 127.0).round().clamp(0.0, 127.0) as u8;
                if nv != e.val[i] {
                    e.val[i] = nv;
                    ch = true;
                }
                if i > 0 {
                    let d = resp.drag_delta().x / inner.width() * total;
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
    ch
}

// ---------------------------------------------------------------------------
// LFO
// ---------------------------------------------------------------------------

pub const LFO_TYPES: [&str; 8] = ["Sine", "Tri", "Square", "Ramp+", "Ramp-", "Exp1", "Exp2", "Random"];

pub fn lfo(ui: &mut Ui, id: &str, l: &mut LfoParams, color: Color32) -> bool {
    let mut ch = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        row(ui, |ui| {
            ch |= choice(ui, (id, "type"), "Shape", &mut l.lfotype, &LFO_TYPES, 60.0);
            ch |= knobf(ui, "Freq", &mut l.freq, 0.0, 85.25, 3.0, &|v| format!("{v:.2}"), color);
            ch |= knob(ui, "Depth", &mut l.intensity, 0, color);
            ch |= knob(ui, "Start", &mut l.startphase, 64, color);
            ch |= knobf(ui, "Delay", &mut l.delay, 0.0, 4.0, 0.0, &|v| format!("{v:.2}s"), color);
            ch |= knob(ui, "Stretch", &mut l.stretch, 64, color);
            ch |= knob(ui, "A.Rnd", &mut l.randomness, 0, color);
            ch |= knob(ui, "F.Rnd", &mut l.freqrand, 0, color);
        });
        row(ui, |ui| {
            ch |= toggle_cell(ui, "Mode", "CONT", &mut l.continous, color);
            ui.add_space(60.0 - KW - 2.0);
            ch |= knobf(ui, "FadeIn", &mut l.fadein, 0.0, 10.0, 0.0, &|v| format!("{v:.2}s"), color);
            ch |= knobf(ui, "FadeOut", &mut l.fadeout, 0.0, 10.0, 10.0, &|v| if v >= 10.0 { "off".into() } else { format!("{v:.2}s") }, color);
            ch |= knob(ui, "Cutoff", &mut l.cutoff, 127, color);
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

/// Filter editor. `extra` adds cells at the end of the main row (velocity
/// sensing and the like) so a whole filter fits in one row.
pub fn filter(ui: &mut Ui, id: &str, f: &mut FilterParams, color: Color32, extra: impl FnOnce(&mut Ui) -> bool) -> bool {
    let mut ch = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        row(ui, |ui| {
            // class + type stacked in one cell
            let (cell, _) = ui.allocate_exact_size(vec2(62.0, KH), Sense::hover());
            let top = Rect::from_min_size(cell.min + vec2(0.0, 2.0), vec2(62.0, 17.0));
            let bot = Rect::from_min_size(cell.min + vec2(0.0, 21.0), vec2(62.0, 17.0));
            let mut a = ui.new_child(egui::UiBuilder::new().max_rect(top).layout(egui::Layout::left_to_right(egui::Align::Center)));
            ch |= combo(&mut a, (id, "cat"), &mut f.category, &FILTER_CATS, 62.0);
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
                let mut b = ui.new_child(egui::UiBuilder::new().max_rect(bot).layout(egui::Layout::left_to_right(egui::Align::Center)));
                ch |= combo(&mut b, (id, "type"), &mut f.ftype, types, 62.0);
            }
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
            ch |= knobf(ui, "Gain", &mut f.gain, -30.0, 30.0, 0.0, &|v| format!("{v:+.1}"), color);
            if f.category != 1 {
                let mut st = f.stages + 1;
                if knob_range(ui, "Stages", &mut st, 1, 5, 1, color) {
                    f.stages = st - 1;
                    ch = true;
                }
            }
            ch |= extra(ui);
        });
        if f.category == 1 {
            row(ui, |ui| {
                ch |= knob_range(ui, "Formants", &mut f.numformants, 1, 12, 3, color);
                ch |= knob(ui, "Slow", &mut f.formantslowness, 64, color);
                ch |= knob(ui, "Clear", &mut f.vowelclearness, 64, color);
                ch |= knob(ui, "Center", &mut f.centerfreq, 64, color);
                ch |= knob(ui, "Octaves", &mut f.octavesfreq, 64, color);
                ch |= knob_range(ui, "Seq", &mut f.sequencesize, 1, 8, 3, color);
                ch |= knob(ui, "Seq.Str", &mut f.sequencestretch, 40, color);
                ch |= toggle_cell(ui, "Seq", "REV", &mut f.sequencereversed, color);
            });
            row(ui, |ui| {
                for s in 0..f.sequencesize as usize {
                    let mut v = f.sequence[s];
                    if knob_range(ui, &format!("Vow {}", s + 1), &mut v, 0, 5, 0, color) {
                        f.sequence[s] = v;
                        ch = true;
                    }
                }
            });
        }
        if f.category == 4 {
            row(ui, |ui| {
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
/// resets a bar. `title` is drawn inside the top-left corner.
pub fn bars(ui: &mut Ui, title: &str, vals: &mut [u8], center: u8, size: Vec2, color: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(3), TRACK);
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
    let mut label = title.to_string();
    let mut ch = false;
    if let Some(pos) = resp.interact_pointer_pos().or(resp.hover_pos()) {
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
            label = format!("{title}  #{} = {}", i + 1, vals[i]);
        }
    }
    p.text(pos2(rect.left() + 4.0, rect.top() + 2.0), Align2::LEFT_TOP, label, font(8.0), DIM);
    ch
}

/// Read-only plot of a waveform.
pub fn wave_plot(ui: &mut Ui, data: &[f32], size: Vec2, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    draw_wave(ui.painter(), rect, data, color);
}

pub fn draw_wave(p: &egui::Painter, rect: Rect, data: &[f32], color: Color32) {
    p.rect_filled(rect, CornerRadius::same(3), TRACK);
    p.line_segment([pos2(rect.left(), rect.center().y), pos2(rect.right(), rect.center().y)], Stroke::new(1.0_f32, LINE));
    if data.is_empty() {
        return;
    }
    let m = data.iter().fold(1e-6f32, |a, v| a.max(v.abs()));
    let n = data.len();
    let wpx = rect.width().max(1.0) as usize;
    let pts: Vec<Pos2> = (0..wpx)
        .map(|x| {
            let i = x * n / wpx;
            pos2(rect.left() + x as f32, rect.center().y - data[i.min(n - 1)] / m * (rect.height() * 0.42))
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(1.3_f32, color)));
}

/// Read-only spectrum bars (linear magnitudes, drawn in dB).
pub fn spectrum_plot(ui: &mut Ui, mags: &[f32], size: Vec2, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(3), TRACK);
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
}

/// Compact horizontal slider for a 0..127 value (mixer sends).
pub fn mini_slider(ui: &mut Ui, label: &str, v: &mut u8, width: f32, color: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 13.0), Sense::click_and_drag());
    let old = *v;
    if resp.double_clicked() {
        *v = 0;
    } else if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let acc = resp.id.with("acc");
        let raw: f32 = ui.memory(|m| m.data.get_temp(acc)).unwrap_or(*v as f32);
        let raw = (raw + resp.drag_delta().x * if fine { 0.1 } else { 1.0 } * 127.0 / (width - 18.0)).clamp(0.0, 127.0);
        ui.memory_mut(|m| m.data.insert_temp(acc, raw));
        *v = raw.round() as u8;
    } else if resp.drag_stopped() || !resp.dragged() {
        ui.memory_mut(|m| m.data.remove::<f32>(resp.id.with("acc")));
    }
    let p = ui.painter();
    let active = resp.hovered() || resp.dragged();
    p.text(pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, label, font(7.5), if active { TEXT } else { DIM });
    let track = Rect::from_min_max(pos2(rect.left() + 14.0, rect.center().y - 3.0), pos2(rect.right(), rect.center().y + 3.0));
    p.rect_filled(track, CornerRadius::same(2), TRACK);
    let t = *v as f32 / 127.0;
    if t > 0.0 {
        p.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * t, track.height())), CornerRadius::same(2), color);
    }
    if active {
        p.text(pos2(track.right() - 2.0, rect.center().y), Align2::RIGHT_CENTER, format!("{}", *v), font(7.5), TEXT);
    }
    *v != old
}

/// Horizontal level bar (mixer strips).
pub fn hbar(p: &egui::Painter, rect: Rect, level: f32) {
    p.rect_filled(rect, CornerRadius::same(1), TRACK);
    let db = 20.0 * level.max(1e-6).log10();
    let t = ((db + 48.0) / 54.0).clamp(0.0, 1.0);
    if t > 0.0 {
        let col = if db > 0.0 { METER_HI } else if db > -12.0 { METER_MID } else { METER_LO };
        p.rect_filled(Rect::from_min_size(rect.min, vec2(rect.width() * t, rect.height())), CornerRadius::same(1), col);
    }
}

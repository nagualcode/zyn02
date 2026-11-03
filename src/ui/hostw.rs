#![allow(dead_code)]
//! Ableton-flavoured widgets bound to truce parameters.
//!
//! Continuous widgets use the host edit-gesture protocol
//! (`begin_edit` / `set_param` / `end_edit`), so a drag records as one
//! automation gesture. Drag to change, Shift for fine control,
//! double-click to reset, mouse wheel for small steps.

use std::collections::HashMap;
use std::sync::OnceLock;

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use truce_core::editor::{PluginContext, PluginContextReadF32};
use truce_params::Params;

use crate::ZynParams;

pub mod theme {
    use egui::Color32;
    pub const BG: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
    pub const PANEL: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
    pub const PANEL_LINE: Color32 = Color32::from_rgb(0x38, 0x38, 0x38);
    pub const HEADER: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
    pub const TEXT: Color32 = Color32::from_rgb(0xd8, 0xd8, 0xd8);
    pub const DIM: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x8a);
    pub const FAINT: Color32 = Color32::from_rgb(0x5c, 0x5c, 0x5c);
    pub const TRACK: Color32 = Color32::from_rgb(0x13, 0x13, 0x13);
    pub const CONTROL: Color32 = Color32::from_rgb(0x3b, 0x3b, 0x3b);
    pub const CONTROL_HOVER: Color32 = Color32::from_rgb(0x4a, 0x4a, 0x4a);
    /// Channel I: Ableton orange.
    pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0x9a, 0x3c);
    /// Channel II: cool blue.
    pub const ACCENT2: Color32 = Color32::from_rgb(0x5c, 0xc8, 0xf0);
    /// Global sections.
    pub const ACCENT3: Color32 = Color32::from_rgb(0xc8, 0xe0, 0x6a);
    pub const ON_TEXT: Color32 = Color32::from_rgb(0x16, 0x16, 0x16);
    pub const CS_RED: Color32 = Color32::from_rgb(0xd2, 0x3a, 0x2c);
    pub const METER_LO: Color32 = Color32::from_rgb(0x5f, 0xd0, 0x6a);
    pub const METER_MID: Color32 = Color32::from_rgb(0xf0, 0xd2, 0x4a);
    pub const METER_HI: Color32 = Color32::from_rgb(0xff, 0x55, 0x45);
}
use theme::*;

pub type Ctx = PluginContext<ZynParams>;

#[derive(Clone, Copy)]
struct Meta {
    default_norm: f32,
    steps: Option<u32>,
    bipolar: bool,
}

fn meta(state: &Ctx, id: u32) -> Meta {
    static CACHE: OnceLock<HashMap<u32, Meta>> = OnceLock::new();
    let map = CACHE.get_or_init(|| {
        state
            .params()
            .param_infos()
            .iter()
            .map(|i| {
                let min = i.range.denormalize(0.0);
                (
                    i.id,
                    Meta {
                        default_norm: i.range.normalize(i.default_plain) as f32,
                        steps: i.range.step_count().map(|s| s.get()),
                        bipolar: min < 0.0,
                    },
                )
            })
            .collect()
    });
    map.get(&id).copied().unwrap_or(Meta { default_norm: 0.0, steps: None, bipolar: false })
}

pub fn small(size: f32) -> FontId {
    FontId::proportional(size)
}

fn snap(v: f32, steps: Option<u32>) -> f32 {
    match steps {
        Some(n) if n > 0 => (v * n as f32).round() / n as f32,
        _ => v,
    }
}

/// Shared drag/wheel/reset behaviour. `travel` = pixels for a full sweep.
fn drive(ui: &egui::Ui, state: &Ctx, resp: &egui::Response, id: u32, travel: f32, horizontal: bool) -> f32 {
    let m = meta(state, id);
    let mut value = state.get_param(id);
    let acc_id = resp.id.with("acc");

    if resp.double_clicked() {
        state.automate(id, f64::from(m.default_norm));
        return m.default_norm;
    }
    if resp.drag_started() {
        state.begin_edit(id);
        ui.memory_mut(|mem| mem.data.insert_temp(acc_id, value));
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let scale = if fine { 0.1 } else { 1.0 };
        let d = if horizontal { resp.drag_delta().x } else { -resp.drag_delta().y };
        let delta = d / travel * scale;
        let raw: f32 = ui.memory(|mem| mem.data.get_temp(acc_id)).unwrap_or(value);
        let raw = (raw + delta).clamp(0.0, 1.0);
        ui.memory_mut(|mem| mem.data.insert_temp(acc_id, raw));
        let snapped = snap(raw, m.steps);
        if (snapped - value).abs() > 1e-6 {
            value = snapped;
            state.set_param(id, f64::from(value));
        }
    }
    if resp.drag_stopped() {
        state.end_edit(id);
    }
    if resp.hovered() && !resp.dragged() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.5 {
            let step = match m.steps {
                Some(n) if n > 0 => scroll.signum() / n as f32,
                _ => scroll / 1500.0,
            };
            value = (value + step).clamp(0.0, 1.0);
            state.automate(id, f64::from(value));
        }
    }
    value
}

/// Vertical CS-80 lever. `ghost` shows a factory preset's value when the
/// channel plays a tone preset (the lever itself is then inactive, as on
/// the hardware); `color` is the cap stripe.
pub fn fader(ui: &mut egui::Ui, state: &Ctx, cell: Rect, id: impl Into<u32>, label: &str, color: Color32, ghost: Option<f32>) {
    let id = id.into();
    let m = meta(state, id);
    let p = ui.painter().clone();
    let dimmed = ghost.is_some();
    p.text(pos2(cell.center().x, cell.top()), Align2::CENTER_TOP, label, small(9.0), if dimmed { FAINT } else { DIM });

    let track = Rect::from_center_size(pos2(cell.center().x, (cell.top() + 14.0 + cell.bottom() - 14.0) * 0.5), vec2(5.0, cell.height() - 36.0));
    let hit = track.expand2(vec2(12.0, 6.0));
    let resp = ui.interact(hit, ui.id().with(("fader", id)), Sense::click_and_drag());
    let v = drive(ui, state, &resp, id, track.height(), false);

    for k in 0..=10 {
        let y = track.bottom() - track.height() * k as f32 / 10.0;
        let w = if k % 5 == 0 { 5.0 } else { 3.0 };
        p.line_segment([pos2(track.left() - 3.0 - w, y), pos2(track.left() - 3.0, y)], Stroke::new(1.0_f32, PANEL_LINE));
    }
    p.rect_filled(track, CornerRadius::same(2), TRACK);
    let base = if m.bipolar { 0.5 } else { 0.0 };
    let y_of = |t: f32| track.bottom() - track.height() * t;
    let fill = Rect::from_min_max(pos2(track.left(), y_of(v.max(base))), pos2(track.right(), y_of(v.min(base))));
    let col = if dimmed { color.gamma_multiply(0.25) } else { color.gamma_multiply(0.85) };
    p.rect_filled(fill, CornerRadius::same(2), col);

    let active = resp.hovered() || resp.dragged();
    let cap = Rect::from_center_size(pos2(track.center().x, y_of(v)), vec2(20.0, 8.0));
    let cap_col = if dimmed { CONTROL } else if active { Color32::WHITE } else { TEXT };
    p.rect_filled(cap, CornerRadius::same(2), cap_col);
    p.line_segment([pos2(cap.left() + 3.0, cap.center().y), pos2(cap.right() - 3.0, cap.center().y)], Stroke::new(1.5_f32, if dimmed { FAINT } else { color }));

    if let Some(g) = ghost {
        let gy = y_of(g);
        let gr = Rect::from_center_size(pos2(track.center().x, gy), vec2(20.0, 8.0));
        p.rect_stroke(gr, CornerRadius::same(2), Stroke::new(1.5_f32, color), StrokeKind::Middle);
        p.line_segment([pos2(gr.left() + 3.0, gy), pos2(gr.right() - 3.0, gy)], Stroke::new(1.5_f32, color));
    }

    let text = state.format_param(id);
    let tcol = if active { color } else if dimmed { FAINT } else { TEXT };
    p.text(pos2(cell.center().x, cell.bottom()), Align2::CENTER_BOTTOM, text, small(9.0), tcol);
}

/// Rotary dial (compact cell, same look as the patch knobs).
pub fn knob(ui: &mut egui::Ui, state: &Ctx, cell: Rect, id: impl Into<u32>, label: &str, color: Color32) {
    let id = id.into();
    let m = meta(state, id);
    let resp = ui.interact(cell, ui.id().with(("knob", id)), Sense::click_and_drag());
    let v = drive(ui, state, &resp, id, 160.0, false);
    let text = state.format_param(id);
    crate::ui::w::draw_knob(ui, cell, &resp, label, v, m.bipolar, &text, color, (v - m.default_norm).abs() > 0.004);
}

pub fn button_face(p: &egui::Painter, rect: Rect, on: bool, hovered: bool, text: &str, size: f32, color: Color32) {
    let fill = if on {
        color
    } else if hovered {
        CONTROL_HOVER
    } else {
        CONTROL
    };
    p.rect_filled(rect, CornerRadius::same(2), fill);
    p.text(rect.center(), Align2::CENTER_CENTER, text, small(size), if on { ON_TEXT } else { TEXT });
}

/// Latching on/off button for a `BoolParam`.
pub fn toggle(ui: &mut egui::Ui, state: &Ctx, rect: Rect, id: impl Into<u32>, text: &str, color: Color32) -> bool {
    let id = id.into();
    let on = state.get_param(id) > 0.5;
    let resp = ui.interact(rect, ui.id().with(("toggle", id)), Sense::click());
    if resp.clicked() {
        state.automate(id, if on { 0.0 } else { 1.0 });
    }
    button_face(ui.painter(), rect, on, resp.hovered(), text, 9.0, color);
    on
}

/// Segmented selector for an `EnumParam`; index 0 at the bottom when
/// vertical, like a panel switch.
pub fn selector(ui: &mut egui::Ui, state: &Ctx, rect: Rect, id: impl Into<u32>, labels: &[&str], vertical: bool, color: Color32) {
    let id = id.into();
    let n = labels.len();
    let cur = (state.get_param(id) * (n as f32 - 1.0)).round() as usize;
    let gap = 2.0;
    for (i, label) in labels.iter().enumerate() {
        let seg = if vertical {
            let h = (rect.height() - gap * (n as f32 - 1.0)) / n as f32;
            let top = rect.bottom() - (i as f32 + 1.0) * h - i as f32 * gap;
            Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), h))
        } else {
            let w = (rect.width() - gap * (n as f32 - 1.0)) / n as f32;
            Rect::from_min_size(pos2(rect.left() + i as f32 * (w + gap), rect.top()), vec2(w, rect.height()))
        };
        let resp = ui.interact(seg, ui.id().with(("sel", id, i)), Sense::click());
        if resp.clicked() && i != cur {
            state.automate(id, i as f64 / (n as f64 - 1.0));
        }
        button_face(ui.painter(), seg, i == cur, resp.hovered(), label, 8.5, color);
    }
}

/// Grid of buttons for an enum (the tone selector).
pub fn button_grid(ui: &mut egui::Ui, state: &Ctx, rect: Rect, id: impl Into<u32>, labels: &[&str], cols: usize, color: Color32) {
    let id = id.into();
    let n = labels.len();
    let cur = (state.get_param(id) * (n as f32 - 1.0)).round() as usize;
    let rows = n.div_ceil(cols);
    let gap = 2.0;
    let w = (rect.width() - gap * (cols as f32 - 1.0)) / cols as f32;
    let h = (rect.height() - gap * (rows as f32 - 1.0)) / rows as f32;
    for (i, label) in labels.iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        let seg = Rect::from_min_size(pos2(rect.left() + c as f32 * (w + gap), rect.top() + r as f32 * (h + gap)), vec2(w, h));
        let resp = ui.interact(seg, ui.id().with(("grid", id, i)), Sense::click());
        if resp.clicked() && i != cur {
            state.automate(id, i as f64 / (n as f64 - 1.0));
        }
        button_face(ui.painter(), seg, i == cur, resp.hovered(), label, 8.0, color);
    }
}

/// Section panel with a title bar; returns the content rect.
pub fn section(ui: &egui::Ui, rect: Rect, title: &str, color: Color32) -> Rect {
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(4), PANEL);
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), 18.0));
    p.rect_filled(head, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, HEADER);
    p.rect_filled(Rect::from_min_size(head.min, vec2(3.0, head.height())), CornerRadius { nw: 4, ne: 0, sw: 0, se: 0 }, color);
    p.text(pos2(head.left() + 9.0, head.center().y), Align2::LEFT_CENTER, title, FontId::proportional(10.5), TEXT);
    Rect::from_min_max(pos2(rect.left() + 6.0, head.bottom() + 6.0), pos2(rect.right() - 6.0, rect.bottom() - 6.0))
}

/// Stereo peak meter (-48..+6 dBFS).
pub fn meter(ui: &egui::Ui, rect: Rect, l: f32, r: f32) {
    let p = ui.painter();
    let db_to_y = |db: f32| {
        let t = ((db + 48.0) / 54.0).clamp(0.0, 1.0);
        rect.bottom() - t * rect.height()
    };
    let bar_w = (rect.width() - 12.0) / 2.0;
    for (k, level) in [l, r].into_iter().enumerate() {
        let x0 = rect.left() + k as f32 * (bar_w + 2.0);
        let bar = Rect::from_min_size(pos2(x0, rect.top()), vec2(bar_w, rect.height()));
        p.rect_filled(bar, CornerRadius::ZERO, TRACK);
        let db = 20.0 * level.max(1e-6).log10();
        let y = db_to_y(db);
        for (lo, hi, col) in [(-48.0, -12.0, METER_LO), (-12.0, -3.0, METER_MID), (-3.0, 6.0, METER_HI)] {
            let top = db_to_y(hi).max(y);
            let bottom = db_to_y(lo);
            if top < bottom {
                p.rect_filled(Rect::from_min_max(pos2(x0, top), pos2(x0 + bar_w, bottom)), CornerRadius::ZERO, col);
            }
        }
    }
    for db in [0.0, -12.0, -24.0, -36.0] {
        let y = db_to_y(db);
        p.line_segment([pos2(rect.right() - 8.0, y), pos2(rect.right() - 4.0, y)], Stroke::new(1.0_f32, DIM));
    }
}

/// CS-80 ribbon (slide controller): relative operation. Touching does
/// nothing; sliding right raises the pitch (full length = one octave),
/// sliding left lowers it toward 0 Hz; letting go returns to centre.
pub fn ribbon(ui: &mut egui::Ui, state: &Ctx, rect: Rect, id: impl Into<u32>, color: Color32) {
    let id = id.into();
    let resp = ui.interact(rect, ui.id().with(("ribbon", id)), Sense::drag());
    let start_id = resp.id.with("start");
    if resp.drag_started() {
        state.begin_edit(id);
        if let Some(pos) = resp.interact_pointer_pos() {
            ui.memory_mut(|m| m.data.insert_temp(start_id, pos.x));
        }
    }
    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let x0: f32 = ui.memory(|m| m.data.get_temp(start_id)).unwrap_or(pos.x);
        let r = ((pos.x - x0) / rect.width()).clamp(-1.0, 1.0);
        state.set_param(id, f64::from(0.5 + 0.5 * r));
    }
    if resp.drag_stopped() {
        state.set_param(id, 0.5);
        state.end_edit(id);
    }
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(0x0e, 0x0e, 0x0e));
    // felt texture
    let n = (rect.width() / 6.0) as usize;
    for i in 0..n {
        let x = rect.left() + 3.0 + i as f32 * 6.0;
        p.line_segment([pos2(x, rect.top() + 4.0), pos2(x, rect.bottom() - 4.0)], Stroke::new(1.0_f32, Color32::from_rgb(0x1b, 0x1b, 0x1b)));
    }
    let v = state.get_param(id) * 2.0 - 1.0;
    let cx = rect.center().x;
    let x = cx + v * rect.width() * 0.5;
    p.line_segment([pos2(cx, rect.top() + 2.0), pos2(cx, rect.bottom() - 2.0)], Stroke::new(1.0_f32, FAINT));
    if v.abs() > 0.001 {
        p.rect_filled(Rect::from_min_max(pos2(cx.min(x), rect.center().y - 2.0), pos2(cx.max(x), rect.center().y + 2.0)), CornerRadius::same(1), color.gamma_multiply(0.7));
    }
    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        p.circle_filled(pos2(pos.x, rect.center().y), 6.0, color);
    }
    p.text(pos2(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER, "RIBBON", small(9.0), DIM);
    p.text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, state.format_param(id), small(9.0), if v.abs() > 0.001 { color } else { DIM });
}

/// Small round LED.
pub fn led(p: &egui::Painter, c: Pos2, level: f32, color: Color32) {
    p.circle_filled(c, 4.0, TRACK);
    if level > 0.002 {
        p.circle_filled(c, 3.5, color.gamma_multiply(0.25 + 0.75 * level.clamp(0.0, 1.0)));
    }
}

/// Plain clickable button; returns true on click.
pub fn action_button(ui: &mut egui::Ui, rect: Rect, key: &str, text: &str, color: Color32) -> bool {
    let resp = ui.interact(rect, ui.id().with(("action", key)), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), if resp.hovered() { CONTROL_HOVER } else { CONTROL });
    p.rect_stroke(rect, CornerRadius::same(2), Stroke::new(1.0_f32, color.gamma_multiply(0.6)), StrokeKind::Inside);
    p.text(rect.center(), Align2::CENTER_CENTER, text, small(8.5), TEXT);
    resp.clicked()
}

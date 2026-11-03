//! Editor: a compact, flat take on ZynAddSubFX's panels.
//!
//! - Header: wordmark, preset browser (bank / instrument, prev / next),
//!   the 16 parts (click to edit, double-click to enable, LED = activity),
//!   file buttons, output meter.
//! - Navigation row: PART / ADD / SUB / PAD / FX / MASTER, colour-coded per
//!   engine like Zyn's classic editor (ADD orange, SUB cyan, PAD violet),
//!   plus the voice and kit selectors of the current page.
//! - Main area: the page, laid out on a fixed grid so it needs no
//!   scrolling; the bank browser opens over it.
//! - Bottom: host performance controls by function, output meter.
//!
//! Colour carries meaning. Engines have a colour (ADD orange, SUB cyan, PAD
//! violet) and so does each job, on every page: amplitude coral, frequency
//! blue, filter green, modulation pink, with LFOs a lighter shade of their
//! section. Parts and effect types each have their own hue too, and a knob
//! at its default dims its readout, so edits stand out.

mod hostw;
mod pages;
pub mod w;

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use truce::prelude::*;
use truce_egui::EguiEditor;

use crate::ZynParams;
use crate::ZynParamsParamId as P;
use crate::state::{EXAMPLE_SETUPS, Shared, banks};
use crate::synth::fft::Fft;
use crate::synth::master::{Event, NUM_MIDI_PARTS, Patch};
use hostw::Ctx;
use w::theme::*;
use w::{action, button_face, font};

pub const WIDTH: u32 = 1140;
pub const HEIGHT: u32 = 602;

const MARGIN: f32 = 8.0;
const HEAD_H: f32 = 40.0;
const NAV_H: f32 = 24.0;
const BOT_H: f32 = 54.0;

thread_local! {
    static FFT: RefCell<Fft> = RefCell::new(Fft::new(crate::state::OSCILSIZE));
}

pub fn with_fft<R>(f: impl FnOnce(&mut Fft) -> R) -> R {
    FFT.with(|x| f(&mut x.borrow_mut()))
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    Part,
    #[default]
    Add,
    Sub,
    Pad,
    Fx,
    Master,
}

/// Effect slot selected in the rack: (0 = part, 1 = system, 2 = insertion, index).
pub type FxSel = (u8, usize);

#[derive(Clone)]
pub struct UiState {
    pub part: usize,
    pub kit: usize,
    pub voice: usize,
    pub page: Page,
    pub browse: bool,
    pub bank: usize,
    /// Last instrument loaded from the browser: (bank, item).
    pub current: Option<(usize, usize)>,
    pub filter: String,
    pub pad_rebuild: Option<(usize, usize)>,
    /// Tab of the right-hand panel on the voice and PAD pages.
    pub tab: usize,
    /// Oscillator panel: false = carrier, true = modulator.
    pub mod_osc: bool,
    pub fx: FxSel,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            part: 0,
            kit: 0,
            voice: usize::MAX,
            page: Page::Add,
            browse: false,
            bank: 0,
            current: None,
            filter: String::new(),
            pad_rebuild: None,
            tab: 0,
            mod_osc: false,
            fx: (0, 0),
        }
    }
}

/// Initial editor state for offscreen renders (documentation screenshots).
static START: std::sync::Mutex<Option<UiState>> = std::sync::Mutex::new(None);

#[doc(hidden)]
pub fn debug_start(page: u8, voice: Option<usize>, browse: bool, bank: usize, tab: usize, fx: FxSel) {
    let page = match page {
        0 => Page::Part,
        1 => Page::Add,
        2 => Page::Sub,
        3 => Page::Pad,
        4 => Page::Fx,
        _ => Page::Master,
    };
    let st = UiState { page, voice: voice.unwrap_or(usize::MAX), browse, bank, tab, fx, ..UiState::default() };
    *START.lock().unwrap() = Some(st);
}

pub fn editor(params: Arc<ZynParams>) -> Box<dyn Editor> {
    EguiEditor::new(params, (WIDTH, HEIGHT), draw).with_visuals(visuals()).into_editor()
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0_f32, LINE);
    v.override_text_color = Some(TEXT);
    v.widgets.inactive.bg_fill = CONTROL;
    v.widgets.inactive.weak_bg_fill = CONTROL;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.corner_radius = CornerRadius::same(3);
    v.widgets.hovered.weak_bg_fill = CONTROL_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.hovered.corner_radius = CornerRadius::same(3);
    v.widgets.active.corner_radius = CornerRadius::same(3);
    v.widgets.open.weak_bg_fill = CONTROL_HOVER;
    v.widgets.open.corner_radius = CornerRadius::same(3);
    v.selection.bg_fill = ZYN.gamma_multiply(0.45);
    v.extreme_bg_color = TRACK;
    v
}

fn draw(ui: &mut egui::Ui, s: &Ctx) {
    let sh: Arc<Shared> = s.params().shared.clone();
    let sid = egui::Id::new("zyn02-ui");
    let start = START.lock().ok().and_then(|g| g.clone());
    let mut st: UiState = ui.data_mut(|d| d.get_temp_mut_or_insert_with::<UiState>(sid, || start.unwrap_or_default()).clone());
    let full = ui.max_rect();
    let accent = page_color(&st);
    ui.painter().rect_filled(full, CornerRadius::ZERO, mix(BG, accent, 0.03));
    let patch = sh.patch();
    {
        let k = &patch.parts[st.part].kit;
        if st.kit >= k.len() {
            st.kit = 0;
        }
    }

    // ---- header ----
    let head = Rect::from_min_size(full.min, vec2(full.width(), HEAD_H));
    ui.painter().rect_filled(head, CornerRadius::ZERO, PANEL);
    ui.painter().line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.5_f32, accent));
    let mut hui = ui.new_child(egui::UiBuilder::new().max_rect(head.shrink2(vec2(10.0, 7.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    header(&mut hui, &sh, &patch, &mut st);

    // ---- bottom strip ----
    let bottom = Rect::from_min_max(pos2(full.left(), full.bottom() - BOT_H), full.max);
    ui.painter().rect_filled(bottom, CornerRadius::ZERO, PANEL);
    ui.painter().line_segment([bottom.left_top(), bottom.right_top()], Stroke::new(1.0_f32, LINE));
    perf_strip(ui, &sh, s, bottom.shrink2(vec2(MARGIN, 4.0)));

    // ---- navigation ----
    let nav = Rect::from_min_size(pos2(full.left() + MARGIN, head.bottom() + 6.0), vec2(full.width() - 2.0 * MARGIN, NAV_H));
    let mut nui = ui.new_child(egui::UiBuilder::new().max_rect(nav).layout(egui::Layout::left_to_right(egui::Align::Center)));
    nav_row(&mut nui, &patch, &mut st);

    // ---- page ----
    let main = Rect::from_min_max(pos2(full.left() + MARGIN, nav.bottom() + 6.0), pos2(full.right() - MARGIN, bottom.top() - 6.0));
    let mut mui = ui.new_child(egui::UiBuilder::new().max_rect(main).layout(egui::Layout::top_down(egui::Align::Min)));
    mui.set_clip_rect(main);
    if st.browse {
        browser(&mut mui, &sh, &patch, &mut st, main);
    } else {
        egui::ScrollArea::vertical().id_salt(("page", st.page as u8)).auto_shrink([false, false]).show(&mut mui, |ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            match st.page {
                Page::Part => pages::part_page(ui, &sh, &patch, &mut st),
                Page::Add => pages::add_page(ui, &sh, &patch, &mut st),
                Page::Sub => pages::sub_page(ui, &sh, &patch, &mut st),
                Page::Pad => pages::pad_page(ui, &sh, &patch, &mut st),
                Page::Fx => pages::fx_page(ui, &sh, &patch, &mut st),
                Page::Master => pages::master_page(ui, &sh, &patch, &mut st),
            }
        });
    }

    // PAD tables are rebuilt once the mouse is released.
    if let Some((p, k)) = st.pad_rebuild {
        if !ui.input(|i| i.pointer.any_down()) {
            sh.rebuild_pad(p, k);
            st.pad_rebuild = None;
        }
    }
    sh.flush_publish();
    sh.empty_trash();
    ui.data_mut(|d| d.insert_temp(sid, st));
    ui.ctx().request_repaint();
}

/// The colour of what is on screen.
fn page_color(st: &UiState) -> Color32 {
    match st.page {
        _ if st.browse => ZYN,
        Page::Part => part_color(st.part),
        Page::Add => ADD,
        Page::Sub => SUB,
        Page::Pad => PAD,
        Page::Fx => FX,
        Page::Master => MIX,
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn header(ui: &mut egui::Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState) {
    ui.spacing_mut().item_spacing.x = 4.0;
    let (r, _) = ui.allocate_exact_size(vec2(84.0, 26.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(Rect::from_min_size(r.min + vec2(0.0, 5.0), vec2(4.0, 16.0)), CornerRadius::same(1), ZYN);
    p.text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, "ZYN", font(19.0), TEXT);
    p.text(pos2(r.left() + 52.0, r.center().y), Align2::LEFT_CENTER, "02", font(19.0), ZYN);
    ui.add_space(6.0);

    // preset box
    let bl = banks();
    let (box_r, box_resp) = ui.allocate_exact_size(vec2(290.0, 26.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(box_r, CornerRadius::same(3), if box_resp.hovered() { CONTROL } else { TRACK });
    if st.browse {
        p.rect_stroke(box_r, CornerRadius::same(3), Stroke::new(1.0_f32, ZYN), StrokeKind::Inside);
    }
    // list icon
    for k in 0..3 {
        let y = box_r.top() + 9.0 + k as f32 * 4.0;
        p.line_segment([pos2(box_r.left() + 8.0, y), pos2(box_r.left() + 18.0, y)], Stroke::new(1.5_f32, if st.browse { ZYN } else { DIM }));
    }
    let name = &patch.parts[st.part].name;
    let name = if name.is_empty() { "(no name)" } else { name.as_str() };
    let bank_lbl = st.current.and_then(|(b, _)| bl.get(b)).map(|b| b.name.as_str()).unwrap_or("INSTRUMENT");
    p.text(pos2(box_r.left() + 26.0, box_r.center().y), Align2::LEFT_CENTER, bank_lbl.to_uppercase(), font(8.0), DIM);
    let lw = p.layout_no_wrap(bank_lbl.to_uppercase(), font(8.0), DIM).size().x;
    p.text(pos2(box_r.left() + 32.0 + lw, box_r.center().y), Align2::LEFT_CENTER, name, font(12.5), TEXT);
    if box_resp.clicked() {
        st.browse = !st.browse;
        if st.browse {
            if let Some((b, _)) = st.current {
                st.bank = b;
            }
        }
    }
    for (txt, d) in [("◀", -1i32), ("▶", 1)] {
        let (r, resp) = ui.allocate_exact_size(vec2(22.0, 26.0), Sense::click());
        button_face(ui.painter(), r, false, resp.hovered(), txt, 9.0, ZYN);
        if resp.clicked() {
            step_preset(sh, st, d);
        }
    }
    ui.add_space(10.0);

    // parts
    for i in 0..NUM_MIDI_PARTS {
        let en = patch.parts[i].enabled;
        let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 26.0), Sense::click());
        let sel = st.part == i;
        let hue = part_color(i);
        let fill = if sel { hue } else if resp.hovered() { CONTROL_HOVER } else if en { mix(CONTROL, hue, 0.3) } else { PANEL2 };
        ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
        ui.painter().text(rect.center() - vec2(0.0, 3.0), Align2::CENTER_CENTER, format!("{}", i + 1), font(9.0), if sel { ON_TEXT } else if en { mix(TEXT, hue, 0.5) } else { FAINT });
        let lvl = sh.part_peak[i * 2].get().max(sh.part_peak[i * 2 + 1].get());
        let led = if lvl > 0.001 { METER_LO } else if en { if sel { ON_TEXT.gamma_multiply(0.5) } else { FAINT } } else { TRACK };
        ui.painter().rect_filled(Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 4.5), vec2(8.0, 2.5)), CornerRadius::same(1), led);
        if resp.clicked() {
            st.part = i;
            st.kit = 0;
        }
        if resp.double_clicked() {
            sh.edit(|p| Arc::make_mut(&mut p.parts[i]).enabled = !en);
        }
        resp.on_hover_text(format!("Part {} {}  (double-click: on/off)", i + 1, if en { "" } else { "- off" }));
    }
    for x in sh.part_peak.iter() {
        x.set(x.get() * 0.85);
    }

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if action(ui, "PANIC", METER_HI, vec2(44.0, 22.0)).clicked() {
            sh.push_gui_event(Event::AllSoundOff);
        }
        if action(ui, "SAVE SETUP", MIX, vec2(66.0, 22.0)).clicked() {
            save_dialog(sh, None, patch);
        }
        if action(ui, "SAVE", ZYN, vec2(40.0, 22.0)).on_hover_text("Save the current part as .xiz").clicked() {
            save_dialog(sh, Some(st.part), patch);
        }
        if action(ui, "LOAD", ZYN, vec2(40.0, 22.0)).on_hover_text("Load .xiz into the current part, or a .xmz setup").clicked() {
            load_dialog(sh, st.part);
        }
        let status = sh.status.lock().map(|s| s.clone()).unwrap_or_default();
        if !status.is_empty() {
            ui.add(egui::Label::new(egui::RichText::new(status).size(9.0).color(ZYN)).truncate());
        }
    });
}

fn step_preset(sh: &Arc<Shared>, st: &mut UiState, d: i32) {
    let bl = banks();
    if bl.is_empty() {
        return;
    }
    let (b, i) = match st.current {
        Some((b, i)) => {
            let n = bl[b].items.len() as i32;
            (b, ((i as i32 + d).rem_euclid(n)) as usize)
        }
        None => (st.bank.min(bl.len() - 1), 0),
    };
    load_entry(sh, st, b, i);
}

fn load_dialog(sh: &Arc<Shared>, part: usize) {
    let file = rfd::FileDialog::new().add_filter("ZynAddSubFX", &["xiz", "xmz"]).pick_file();
    if let Some(f) = file {
        match std::fs::read(&f) {
            Ok(bytes) => {
                let name = crate::state::pretty_name(&f.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
                if f.extension().map(|e| e == "xmz").unwrap_or(false) {
                    sh.load_bytes(bytes, true);
                } else {
                    sh.load_instrument(bytes, part, name);
                }
            }
            Err(e) => sh.set_status(&format!("{e}")),
        }
    }
}

fn save_dialog(sh: &Arc<Shared>, part: Option<usize>, patch: &Patch) {
    let (ext, name) = match part {
        Some(p) => ("xiz", if patch.parts[p].name.is_empty() { "instrument".to_string() } else { patch.parts[p].name.clone() }),
        None => ("xmz", "setup".to_string()),
    };
    let file = rfd::FileDialog::new().add_filter("ZynAddSubFX", &[ext]).set_file_name(format!("{name}.{ext}")).save_file();
    if let Some(f) = file {
        let doc = match part {
            Some(p) => patch.xiz_of(p),
            None => patch.to_xml(),
        };
        if let Err(e) = std::fs::write(&f, doc.to_gzip()) {
            sh.set_status(&format!("{e}"));
        } else {
            sh.set_status(&format!("Saved {}", f.display()));
        }
    }
}

// ---------------------------------------------------------------------------
// Navigation row
// ---------------------------------------------------------------------------

fn nav_row(ui: &mut egui::Ui, patch: &Patch, st: &mut UiState) {
    let part = &patch.parts[st.part];
    let kit = &part.kit[st.kit];
    ui.spacing_mut().item_spacing.x = 2.0;
    let items: [(Page, &str, Color32, Option<bool>); 6] = [
        (Page::Part, "PART", part_color(st.part), None),
        (Page::Add, "ADD", ADD, Some(kit.ad_enabled)),
        (Page::Sub, "SUB", SUB, Some(kit.sub_enabled)),
        (Page::Pad, "PAD", PAD, Some(kit.pad_enabled)),
        (Page::Fx, "EFFECTS", FX, None),
        (Page::Master, "MIXER", MIX, None),
    ];
    let (bar, _) = ui.allocate_exact_size(vec2(6.0 * 74.0 + 4.0, NAV_H), Sense::hover());
    ui.painter().rect_filled(bar, CornerRadius::same(4), TRACK);
    for (i, (pg, name, col, en)) in items.into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(bar.left() + 2.0 + i as f32 * 74.0, bar.top() + 2.0), vec2(73.0, NAV_H - 4.0));
        let resp = ui.interact(r, ui.id().with(("tab", i)), Sense::click());
        let on = st.page == pg && !st.browse;
        let p = ui.painter();
        let live = en.unwrap_or(true);
        p.rect_filled(r, CornerRadius::same(3), if on { col } else if resp.hovered() { CONTROL } else if live { mix(PANEL, col, 0.12) } else { PANEL });
        let tc = if on { ON_TEXT } else { mix(TEXT, col, 0.6) };
        match en {
            Some(e) => {
                p.text(pos2(r.center().x + 4.0, r.center().y), Align2::CENTER_CENTER, name, font(9.5), if e || on { tc } else { FAINT });
                let dot = pos2(r.center().x - 15.0, r.center().y);
                if e {
                    p.circle_filled(dot, 3.0, if on { ON_TEXT } else { col });
                } else {
                    p.circle_stroke(dot, 2.5, Stroke::new(1.0_f32, if on { ON_TEXT } else { FAINT }));
                }
            }
            None => {
                p.text(r.center(), Align2::CENTER_CENTER, name, font(9.5), tc);
                if !on {
                    p.rect_filled(Rect::from_min_size(pos2(r.left() + 4.0, r.bottom() - 3.0), vec2(r.width() - 8.0, 1.5)), CornerRadius::same(1), col.gamma_multiply(0.5));
                }
            }
        }
        if resp.clicked() {
            st.page = pg;
            st.browse = false;
        }
    }
    ui.add_space(12.0);

    // voice selector (ADD)
    if st.page == Page::Add && !st.browse {
        if let Some(ad) = kit.ad.as_ref() {
            let mut labels = vec!["GLOBAL".to_string()];
            labels.extend((1..=8).map(|i| format!("V{i}")));
            let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
            let mut marks = vec![false];
            marks.extend(ad.voices.iter().map(|v| v.enabled));
            let sel = if st.voice == usize::MAX { 0 } else { st.voice + 1 };
            let (r, _) = ui.allocate_exact_size(vec2(0.0, NAV_H), Sense::hover());
            let _ = r;
            if let Some(i) = seg_var(ui, &refs, sel, &marks, ADD) {
                st.voice = if i == 0 { usize::MAX } else { i - 1 };
            }
            ui.add_space(12.0);
        }
    }

    // kit selector
    if part.kitmode != 0 && !st.browse && matches!(st.page, Page::Add | Page::Sub | Page::Pad | Page::Part) {
        ui.label(egui::RichText::new("KIT").size(8.5).color(DIM));
        for (i, it) in part.kit.iter().enumerate() {
            if i > 0 && !it.enabled {
                continue;
            }
            let (r, resp) = ui.allocate_exact_size(vec2(20.0, NAV_H - 4.0), Sense::click());
            button_face(ui.painter(), r, st.kit == i, resp.hovered(), &format!("{}", i + 1), 8.5, ZYN);
            if resp.clicked() {
                st.kit = i;
            }
        }
    }

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let mut crumb = format!("PART {}", st.part + 1);
        if part.kitmode != 0 {
            crumb += &format!("  ·  KIT {}", st.kit + 1);
        }
        if st.page == Page::Add && st.voice != usize::MAX {
            crumb += &format!("  ·  VOICE {}", st.voice + 1);
        }
        ui.label(egui::RichText::new(crumb).size(9.0).color(part_color(st.part)));
    });
}

/// Segmented control with per-segment width from its label.
fn seg_var(ui: &mut egui::Ui, items: &[&str], sel: usize, marks: &[bool], color: Color32) -> Option<usize> {
    let widths: Vec<f32> = items.iter().map(|t| (t.len() as f32 * 6.0 + 18.0).max(32.0)).collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (bar, _) = ui.allocate_exact_size(vec2(total, NAV_H), Sense::hover());
    ui.painter().rect_filled(bar, CornerRadius::same(4), TRACK);
    let mut x = bar.left() + 2.0;
    let mut out = None;
    for (i, t) in items.iter().enumerate() {
        let r = Rect::from_min_size(pos2(x, bar.top() + 2.0), vec2(widths[i] - 1.0, NAV_H - 4.0));
        x += widths[i];
        let resp = ui.interact(r, ui.id().with(("segv", *t, i)), Sense::click());
        let on = i == sel;
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(3), if on { color } else if resp.hovered() { CONTROL } else { PANEL });
        let mark = marks.get(i).copied().unwrap_or(false);
        let tx = if mark { r.center().x + 3.5 } else { r.center().x };
        p.text(pos2(tx, r.center().y), Align2::CENTER_CENTER, *t, font(9.0), if on { ON_TEXT } else { TEXT });
        if mark {
            p.circle_filled(pos2(r.left() + 7.0, r.center().y), 2.5, if on { ON_TEXT } else { color });
        }
        if resp.clicked() {
            out = Some(i);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Browser
// ---------------------------------------------------------------------------

fn browser(ui: &mut egui::Ui, sh: &Arc<Shared>, patch: &Patch, st: &mut UiState, area: Rect) {
    let bl = banks();
    if bl.is_empty() {
        return;
    }
    st.bank = st.bank.min(bl.len() - 1);
    let p = ui.painter().clone();
    p.rect_filled(area, CornerRadius::same(4), PANEL);

    // bank list
    let list = Rect::from_min_size(area.min, vec2(210.0, area.height()));
    p.rect_filled(list, CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 }, PANEL2);
    let mut lui = ui.new_child(egui::UiBuilder::new().max_rect(list.shrink2(vec2(6.0, 6.0))).layout(egui::Layout::top_down(egui::Align::Min)));
    egui::ScrollArea::vertical().id_salt("banklist").auto_shrink([false, false]).show(&mut lui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        let mut last_group = None;
        for (bi, b) in bl.iter().enumerate() {
            let group = if b.community { 0 } else if b.name == EXAMPLE_SETUPS { 2 } else { 1 };
            if last_group != Some(group) {
                if last_group.is_some() {
                    ui.add_space(6.0);
                }
                let (t, c) = match group {
                    0 => ("COMMUNITY", COMMUNITY),
                    1 => ("ZYNADDSUBFX", ZYN),
                    _ => ("SETUPS", MIX),
                };
                ui.label(egui::RichText::new(t).size(8.5).color(c));
                last_group = Some(group);
            }
            let on = st.bank == bi;
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::click());
            let pp = ui.painter();
            if on || resp.hovered() {
                pp.rect_filled(r, CornerRadius::same(3), if on { CONTROL_HOVER } else { CONTROL });
            }
            if on {
                let c = match group {
                    0 => COMMUNITY,
                    1 => ZYN,
                    _ => MIX,
                };
                pp.rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), CornerRadius::same(1), c);
            }
            pp.text(pos2(r.left() + 9.0, r.center().y), Align2::LEFT_CENTER, &b.name, font(10.0), if on { TEXT } else { TEXT.gamma_multiply(0.85) });
            pp.text(pos2(r.right() - 4.0, r.center().y), Align2::RIGHT_CENTER, format!("{}", b.items.len()), font(8.5), FAINT);
            if resp.clicked() {
                st.bank = bi;
            }
        }
    });

    // right side: title + search + grid
    let right = Rect::from_min_max(pos2(list.right() + 10.0, area.top() + 8.0), pos2(area.right() - 8.0, area.bottom() - 8.0));
    let mut rui = ui.new_child(egui::UiBuilder::new().max_rect(right).layout(egui::Layout::top_down(egui::Align::Min)));
    let q = st.filter.to_lowercase();
    rui.horizontal(|ui| {
        let b = &bl[st.bank];
        if q.is_empty() {
            ui.label(egui::RichText::new(&b.name).size(14.0).color(TEXT));
            let note = if b.community { community_note(&b.name) } else if b.name == EXAMPLE_SETUPS { "Multi-part setups (.xmz) - loading replaces all parts" } else { "ZynAddSubFX instrument bank (GPL)" };
            ui.label(egui::RichText::new(note).size(9.0).color(DIM));
        } else {
            ui.label(egui::RichText::new(format!("Search: \"{}\"", st.filter)).size(14.0).color(TEXT));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if action(ui, "CLOSE", ZYN, vec2(48.0, 20.0)).clicked() {
                st.browse = false;
            }
            ui.add(egui::TextEdit::singleline(&mut st.filter).hint_text("search all banks...").desired_width(180.0));
        });
    });
    rui.add_space(6.0);
    let cur_name = &patch.parts[st.part].name;
    egui::ScrollArea::vertical().id_salt(("grid", st.bank, q.is_empty())).auto_shrink([false, false]).show(&mut rui, |ui| {
        let cols = 4usize;
        let cw = (ui.available_width() - (cols as f32 - 1.0) * 4.0) / cols as f32;
        let mut hits: Vec<(usize, usize)> = Vec::new();
        if q.is_empty() {
            hits.extend((0..bl[st.bank].items.len()).map(|i| (st.bank, i)));
        } else {
            for (bi, b) in bl.iter().enumerate() {
                for (ii, it) in b.items.iter().enumerate() {
                    if it.name.to_lowercase().contains(&q) || b.name.to_lowercase().contains(&q) {
                        hits.push((bi, ii));
                    }
                }
            }
        }
        let mut load = None;
        for chunk in hits.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for &(bi, ii) in chunk {
                    let it = &bl[bi].items[ii];
                    let (r, resp) = ui.allocate_exact_size(vec2(cw, 20.0), Sense::click());
                    let on = st.current == Some((bi, ii)) || (st.current.is_none() && &it.name == cur_name);
                    let acc = if bl[bi].community { COMMUNITY } else if bl[bi].name == EXAMPLE_SETUPS { MIX } else { ZYN };
                    let pp = ui.painter();
                    pp.rect_filled(r, CornerRadius::same(3), if on { acc.gamma_multiply(0.85) } else if resp.hovered() { CONTROL_HOVER } else { CONTROL });
                    let label = if q.is_empty() { it.name.clone() } else { format!("{}  ·  {}", it.name, bl[bi].name) };
                    pp.text(pos2(r.left() + 7.0, r.center().y), Align2::LEFT_CENTER, label, font(9.5), if on { ON_TEXT } else { TEXT });
                    if resp.clicked() {
                        load = Some((bi, ii));
                    }
                }
            });
        }
        if hits.is_empty() {
            ui.label(egui::RichText::new("Nothing found.").size(10.0).color(DIM));
        }
        if let Some((bi, ii)) = load {
            load_entry(sh, st, bi, ii);
        }
    });
}

fn community_note(bank: &str) -> &'static str {
    if bank.contains("Yoshimi") {
        "From the Yoshimi banks - Will J. Godfrey, Chip Van Dan and others (GPL-2.0+)"
    } else {
        "Yingchun Soul / NeoSoundFonts - CC0, public domain"
    }
}

fn load_entry(sh: &Arc<Shared>, st: &mut UiState, bank: usize, item: usize) {
    let bl = banks();
    let Some(b) = bl.get(bank) else { return };
    let Some(it) = b.items.get(item) else { return };
    if b.name == EXAMPLE_SETUPS {
        sh.load_bytes(it.data.to_vec(), true);
        st.part = 0;
    } else {
        sh.load_instrument(it.data.to_vec(), st.part, it.name.clone());
    }
    st.current = Some((bank, item));
    st.kit = 0;
    st.voice = usize::MAX;
}

// ---------------------------------------------------------------------------
// Bottom strip
// ---------------------------------------------------------------------------

/// A labelled group of the strip: tinted, with a vertical tag.
fn strip_group(ui: &egui::Ui, rect: Rect, tag: &str, c: Color32) {
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(3), mix(PANEL2, c, 0.1));
    p.rect_filled(Rect::from_min_size(rect.min, vec2(2.0, rect.height())), CornerRadius::same(1), c);
    let g = p.layout_no_wrap(tag.to_string(), font(7.5), mix(DIM, c, 0.7));
    let pos = pos2(rect.left() + 4.0, rect.center().y + g.size().x * 0.5);
    p.add(egui::epaint::TextShape::new(pos, g, mix(DIM, c, 0.7)).with_angle(-std::f32::consts::FRAC_PI_2));
}

fn perf_strip(ui: &mut egui::Ui, sh: &Arc<Shared>, s: &Ctx, rect: Rect) {
    const TAG: f32 = 16.0;
    const GAP: f32 = 6.0;
    const W: f32 = 44.0;
    let groups: [(&str, Color32, &[(P, &str)]); 4] = [
        ("FILTER", FLT, &[(P::Cutoff, "Cutoff"), (P::Q, "Reso")]),
        ("MOD", MODC, &[(P::Bandwidth, "Bandw"), (P::Modwheel, "Mod"), (P::Fmamp, "FM Amt")]),
        ("RES", RES, &[(P::ResCenter, "Res.Ctr"), (P::ResBw, "Res.BW")]),
        ("AMP", AMP, &[(P::Expression, "Expr"), (P::Pan, "Pan")]),
    ];
    let mut x = rect.left();
    for (tag, c, knobs) in groups {
        let g = Rect::from_min_size(pos2(x, rect.top()), vec2(TAG + knobs.len() as f32 * W, rect.height()));
        strip_group(ui, g, tag, c);
        for (i, (id, label)) in knobs.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(g.left() + TAG + i as f32 * W + (W - w::KW) * 0.5, g.top() + 3.0), vec2(w::KW, w::KH));
            hostw::knob(ui, s, cell, *id, label, c);
        }
        x = g.right() + GAP;
    }

    let g = Rect::from_min_size(pos2(x, rect.top()), vec2(92.0, rect.height()));
    strip_group(ui, g, "GLIDE", FRQ);
    let t = Rect::from_center_size(pos2(g.center().x + 6.0, g.center().y), vec2(70.0, 18.0));
    hostw::toggle(ui, s, t, P::Portamento, "PORTAMENTO", FRQ);
    x = g.right() + GAP;

    // output: level knob, wide stereo meter, sounding notes
    let g = Rect::from_min_max(pos2(x, rect.top()), rect.right_bottom());
    strip_group(ui, g, "OUT", MIX);
    let cell = Rect::from_min_size(pos2(g.left() + TAG + (W - w::KW) * 0.5, g.top() + 3.0), vec2(w::KW, w::KH));
    hostw::knob(ui, s, cell, P::Output, "Output", MIX);
    let notes: u32 = sh.part_notes.iter().map(|n| n.load(Ordering::Relaxed)).sum();
    let txt = Rect::from_min_max(pos2(g.right() - 58.0, g.top()), g.right_bottom());
    ui.painter().text(pos2(txt.right() - 6.0, txt.center().y), Align2::RIGHT_CENTER, format!("{notes} notes"), font(9.0), if notes > 0 { MIX } else { FAINT });
    let m = Rect::from_min_max(pos2(g.left() + TAG + W + 4.0, g.center().y - 8.0), pos2(txt.left() - 4.0, g.center().y + 8.0));
    w::hbar(ui.painter(), Rect::from_min_size(m.min, vec2(m.width(), 7.5)), s.get_meter(P::MeterL));
    w::hbar(ui.painter(), Rect::from_min_size(pos2(m.left(), m.bottom() - 7.5), vec2(m.width(), 7.5)), s.get_meter(P::MeterR));
}

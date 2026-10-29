//! Editor: an Ableton-style flat take on ZynAddSubFX's panels.
//!
//! - Header: wordmark, the 16 parts (click to edit, LED = activity),
//!   instrument name, file buttons, output meter.
//! - Left: bank browser with the bundled ZynAddSubFX banks.
//! - Centre: pages - PART, ADD, SUB, PAD, EFFECTS, MASTER - colour-coded
//!   per engine like Zyn's classic editor (ADD orange, SUB blue, PAD
//!   violet).
//! - Bottom: host performance controls and a playable keyboard.

mod hostw;
mod pages;
pub mod w;

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, pos2, vec2};
use truce::prelude::*;
use truce_egui::EguiEditor;

use crate::ZynParams;
use crate::ZynParamsParamId as P;
use crate::state::{Shared, banks};
use crate::synth::fft::Fft;
use crate::synth::master::{Event, NUM_MIDI_PARTS};
use hostw::Ctx;
use w::theme::*;
use w::{button, font, meter};

pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 820;

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

#[derive(Clone)]
pub struct UiState {
    pub part: usize,
    pub kit: usize,
    pub voice: usize,
    pub page: Page,
    pub bank: String,
    pub filter: String,
    pub pad_rebuild: Option<(usize, usize)>,
    pub octave: i32,
    pub held: Option<u8>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            part: 0,
            kit: 0,
            voice: usize::MAX,
            page: Page::Add,
            bank: String::new(),
            filter: String::new(),
            pad_rebuild: None,
            octave: 4,
            held: None,
        }
    }
}

pub fn editor(params: Arc<ZynParams>) -> Box<dyn Editor> {
    EguiEditor::new(params, (WIDTH, HEIGHT), draw).with_visuals(visuals()).into_editor()
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.override_text_color = Some(TEXT);
    v.widgets.inactive.bg_fill = CONTROL;
    v.widgets.inactive.weak_bg_fill = CONTROL;
    v.widgets.hovered.weak_bg_fill = CONTROL_HOVER;
    v.selection.bg_fill = ADD.gamma_multiply(0.6);
    v.extreme_bg_color = TRACK;
    v
}

fn draw(ui: &mut egui::Ui, s: &Ctx) {
    let sh: Arc<Shared> = s.params().shared.clone();
    let sid = egui::Id::new("zyn02-ui");
    let mut st: UiState = ui.data_mut(|d| d.get_temp_mut_or_default::<UiState>(sid).clone());
    let full = ui.max_rect();
    ui.painter().rect_filled(full, CornerRadius::ZERO, BG);
    let patch = sh.patch();
    sh.selected_part.store(st.part as u32, Ordering::Relaxed);

    // ---- header ----
    let head = Rect::from_min_size(full.min, vec2(full.width(), 44.0));
    ui.painter().rect_filled(head, CornerRadius::ZERO, PANEL);
    ui.painter().line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0_f32, LINE));
    let mut hui = ui.new_child(egui::UiBuilder::new().max_rect(head.shrink2(vec2(10.0, 6.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    header(&mut hui, &sh, &patch, &mut st, s);

    // ---- bottom strip ----
    let bottom = Rect::from_min_max(pos2(full.left(), full.bottom() - 120.0), full.max);
    ui.painter().rect_filled(bottom, CornerRadius::ZERO, PANEL);
    ui.painter().line_segment([bottom.left_top(), bottom.right_top()], Stroke::new(1.0_f32, LINE));
    perf_strip(ui, s, Rect::from_min_size(bottom.min + vec2(10.0, 8.0), vec2(560.0, 104.0)));
    keyboard(ui, &sh, &patch, &mut st, Rect::from_min_max(pos2(bottom.left() + 580.0, bottom.top() + 10.0), pos2(bottom.right() - 10.0, bottom.bottom() - 10.0)));

    // ---- browser ----
    let side = Rect::from_min_max(pos2(full.left(), head.bottom()), pos2(full.left() + 230.0, bottom.top()));
    ui.painter().rect_filled(side, CornerRadius::ZERO, PANEL2);
    let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(side.shrink(8.0)).layout(egui::Layout::top_down(egui::Align::Min)));
    browser(&mut bui, &sh, &mut st);

    // ---- page ----
    let main = Rect::from_min_max(pos2(side.right() + 10.0, head.bottom() + 8.0), pos2(full.right() - 10.0, bottom.top() - 8.0));
    let mut mui = ui.new_child(egui::UiBuilder::new().max_rect(main).layout(egui::Layout::top_down(egui::Align::Min)));
    tabs(&mut mui, &patch, &mut st);
    mui.add_space(6.0);
    egui::ScrollArea::both().id_salt("page").auto_shrink([false, false]).show(&mut mui, |ui| match st.page {
        Page::Part => pages::part_page(ui, &sh, &patch, &mut st),
        Page::Add => pages::add_page(ui, &sh, &patch, &mut st),
        Page::Sub => pages::sub_page(ui, &sh, &patch, &mut st),
        Page::Pad => pages::pad_page(ui, &sh, &patch, &mut st),
        Page::Fx => pages::fx_page(ui, &sh, &patch, &mut st),
        Page::Master => pages::master_page(ui, &sh, &patch),
    });

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

fn header(ui: &mut egui::Ui, sh: &Arc<Shared>, patch: &crate::synth::master::Patch, st: &mut UiState, s: &Ctx) {
    let (r, _) = ui.allocate_exact_size(vec2(110.0, 30.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(Rect::from_min_size(r.min + vec2(0.0, 6.0), vec2(5.0, 18.0)), CornerRadius::same(1), ZYN);
    p.text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, "ZYN", font(20.0), TEXT);
    p.text(pos2(r.left() + 58.0, r.center().y), Align2::LEFT_CENTER, "02", font(20.0), ZYN);
    ui.add_space(6.0);
    for i in 0..NUM_MIDI_PARTS {
        let en = patch.parts[i].enabled;
        let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
        let sel = st.part == i;
        let fill = if sel { ZYN } else if resp.hovered() { CONTROL_HOVER } else { CONTROL };
        ui.painter().rect_filled(rect, CornerRadius::same(2), fill);
        ui.painter().text(rect.center() - vec2(0.0, 2.0), Align2::CENTER_CENTER, format!("{}", i + 1), font(10.0), if sel { ON_TEXT } else if en { TEXT } else { FAINT });
        let lvl = sh.part_peak[i * 2].get().max(sh.part_peak[i * 2 + 1].get());
        let led = if lvl > 0.001 { METER_LO } else if en { FAINT } else { TRACK };
        ui.painter().circle_filled(pos2(rect.center().x, rect.bottom() - 4.0), 2.0, led);
        if resp.clicked() {
            st.part = i;
            st.kit = 0;
        }
        if resp.double_clicked() {
            sh.edit(|p| Arc::make_mut(&mut p.parts[i]).enabled = !en);
        }
    }
    for x in sh.part_peak.iter() {
        x.set(x.get() * 0.85);
    }
    ui.add_space(8.0);
    let name = &patch.parts[st.part].name;
    ui.label(egui::RichText::new(if name.is_empty() { "(no name)" } else { name }).size(14.0).color(TEXT));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let (mr, _) = ui.allocate_exact_size(vec2(14.0, 30.0), Sense::hover());
        meter(ui, mr, s.get_meter(P::MeterL), s.get_meter(P::MeterR));
        if button(ui, "SAVE SETUP", false, FX, vec2(76.0, 22.0)).clicked() {
            save_dialog(sh, None, patch);
        }
        if button(ui, "SAVE INSTR.", false, FX, vec2(76.0, 22.0)).clicked() {
            save_dialog(sh, Some(st.part), patch);
        }
        if button(ui, "LOAD", false, FX, vec2(50.0, 22.0)).clicked() {
            load_dialog(sh, st.part);
        }
        if button(ui, "PANIC", false, METER_HI, vec2(50.0, 22.0)).clicked() {
            sh.push_gui_event(Event::AllSoundOff);
        }
        let status = sh.status.lock().map(|s| s.clone()).unwrap_or_default();
        if !status.is_empty() {
            ui.label(egui::RichText::new(status).size(10.0).color(ZYN));
        }
    });
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

fn save_dialog(sh: &Arc<Shared>, part: Option<usize>, patch: &crate::synth::master::Patch) {
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

fn tabs(ui: &mut egui::Ui, patch: &crate::synth::master::Patch, st: &mut UiState) {
    let kit = &patch.parts[st.part].kit[st.kit];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let items: [(Page, &str, egui::Color32, bool); 6] = [
            (Page::Part, "PART", FX, true),
            (Page::Add, "ADDsynth", ADD, kit.ad_enabled),
            (Page::Sub, "SUBsynth", SUB, kit.sub_enabled),
            (Page::Pad, "PADsynth", PAD, kit.pad_enabled),
            (Page::Fx, "EFFECTS", FX, true),
            (Page::Master, "MASTER", FX, true),
        ];
        for (pg, name, col, on) in items {
            let label = if on || matches!(pg, Page::Part | Page::Fx | Page::Master) { name.to_string() } else { format!("{name} (off)") };
            if button(ui, &label, st.page == pg, col, vec2(96.0, 22.0)).clicked() {
                st.page = pg;
            }
        }
        ui.add_space(12.0);
        ui.label(egui::RichText::new(format!("Part {} | Kit {}", st.part + 1, st.kit + 1)).size(10.5).color(DIM));
    });
}

fn browser(ui: &mut egui::Ui, sh: &Arc<Shared>, st: &mut UiState) {
    ui.label(egui::RichText::new("BANKS").size(11.0).color(TEXT));
    ui.add(egui::TextEdit::singleline(&mut st.filter).hint_text("search...").desired_width(210.0));
    ui.add_space(4.0);
    let q = st.filter.to_lowercase();
    egui::ScrollArea::vertical().id_salt("banks").auto_shrink([false, false]).show(ui, |ui| {
        for (bank, items) in banks().iter() {
            if !q.is_empty() {
                for it in items.iter().filter(|it| it.name.to_lowercase().contains(&q)) {
                    let r = ui.selectable_label(false, egui::RichText::new(format!("{}  |  {bank}", it.name)).size(10.5));
                    if r.clicked() {
                        load_entry(sh, st, bank, it);
                    }
                }
                continue;
            }
            let open = st.bank == *bank;
            let r = ui.selectable_label(open, egui::RichText::new(format!("{} {}", if open { "-" } else { "+" }, bank)).size(11.0));
            if r.clicked() {
                st.bank = if open { String::new() } else { bank.clone() };
            }
            if open {
                ui.indent(bank, |ui| {
                    for it in items.iter() {
                        if ui.selectable_label(false, egui::RichText::new(&it.name).size(10.5).color(DIM)).clicked() {
                            load_entry(sh, st, bank, it);
                        }
                    }
                });
            }
        }
    });
}

fn load_entry(sh: &Arc<Shared>, st: &mut UiState, bank: &str, it: &crate::state::BankEntry) {
    if bank == "Example Setups" {
        sh.load_bytes(it.data.to_vec(), true);
        st.part = 0;
    } else {
        sh.load_instrument(it.data.to_vec(), st.part, it.name.clone());
    }
    st.kit = 0;
    st.voice = usize::MAX;
}

fn perf_strip(ui: &mut egui::Ui, s: &Ctx, rect: Rect) {
    let ids = [
        (P::Cutoff, "CUTOFF"),
        (P::Q, "RESO"),
        (P::Bandwidth, "BANDW"),
        (P::Modwheel, "MOD"),
        (P::Fmamp, "FM AMT"),
        (P::ResCenter, "RES CTR"),
        (P::ResBw, "RES BW"),
        (P::Expression, "EXPR"),
        (P::Pan, "PAN"),
        (P::Output, "OUTPUT"),
    ];
    let w = rect.width() / ids.len() as f32;
    for (i, (id, label)) in ids.into_iter().enumerate() {
        let cell = Rect::from_min_size(rect.min + vec2(i as f32 * w, 0.0), vec2(w, 64.0));
        hostw::knob(ui, s, cell, id, label, ZYN);
    }
    let t = Rect::from_min_size(rect.min + vec2(0.0, 78.0), vec2(70.0, 20.0));
    hostw::toggle(ui, s, t, P::Portamento, "PORTAMENTO", ZYN);
}

fn keyboard(ui: &mut egui::Ui, sh: &Arc<Shared>, patch: &crate::synth::master::Patch, st: &mut UiState, rect: Rect) {
    let ch = patch.parts[st.part].rcvchn;
    let octaves = 4;
    let nwhite = octaves * 7;
    let ctrl = Rect::from_min_size(rect.min, vec2(36.0, rect.height()));
    let kb = Rect::from_min_max(pos2(rect.left() + 40.0, rect.top()), rect.max);
    // octave buttons
    let up = Rect::from_min_size(ctrl.min, vec2(36.0, 22.0));
    let dn = Rect::from_min_size(ctrl.min + vec2(0.0, 26.0), vec2(36.0, 22.0));
    for (r, d, t) in [(up, 1, "OCT+"), (dn, -1, "OCT-")] {
        let resp = ui.interact(r, ui.id().with(t), Sense::click());
        hostw::button_face(ui.painter(), r, false, resp.hovered(), t, 9.0, ZYN);
        if resp.clicked() {
            st.octave = (st.octave + d).clamp(0, 8);
        }
    }
    ui.painter().text(pos2(ctrl.center().x, ctrl.top() + 64.0), Align2::CENTER_CENTER, format!("C{}", st.octave - 1), font(10.0), DIM);
    let base = (st.octave * 12) as u8;
    let ww = kb.width() / nwhite as f32;
    let resp = ui.interact(kb, ui.id().with("kb"), Sense::click_and_drag());
    let white_off = [0, 2, 4, 5, 7, 9, 11];
    let black: [(usize, u8); 5] = [(0, 1), (1, 3), (3, 6), (4, 8), (5, 10)];
    let p = ui.painter();
    let mut hit: Option<u8> = None;
    let pos = resp.interact_pointer_pos();
    // black keys first for hit-testing
    let mut black_rects = Vec::new();
    for o in 0..octaves {
        for (wi, off) in black {
            let x = kb.left() + (o * 7 + wi + 1) as f32 * ww;
            let r = Rect::from_min_size(pos2(x - ww * 0.3, kb.top()), vec2(ww * 0.6, kb.height() * 0.6));
            black_rects.push((r, base + (o * 12) as u8 + off));
        }
    }
    if let Some(pp) = pos {
        if resp.is_pointer_button_down_on() {
            hit = black_rects.iter().find(|(r, _)| r.contains(pp)).map(|x| x.1);
            if hit.is_none() && kb.contains(pp) {
                let wi = ((pp.x - kb.left()) / ww) as usize;
                hit = Some(base + (wi / 7 * 12) as u8 + white_off[wi % 7]);
            }
        }
    }
    for wi in 0..nwhite {
        let note = base + (wi / 7 * 12) as u8 + white_off[wi % 7];
        let r = Rect::from_min_size(pos2(kb.left() + wi as f32 * ww, kb.top()), vec2(ww - 1.0, kb.height()));
        let on = sh.keys[note as usize & 127].load(Ordering::Relaxed);
        p.rect_filled(r, CornerRadius::same(2), if on { ZYN } else { egui::Color32::from_rgb(0xd0, 0xd0, 0xcc) });
        if wi % 7 == 0 {
            p.text(pos2(r.center().x, r.bottom() - 8.0), Align2::CENTER_CENTER, format!("C{}", (note / 12) as i32 - 1), font(8.0), FAINT);
        }
    }
    for (r, note) in black_rects {
        let on = sh.keys[note as usize & 127].load(Ordering::Relaxed);
        p.rect_filled(r, CornerRadius::same(2), if on { ZYN } else { egui::Color32::from_rgb(0x20, 0x20, 0x20) });
    }
    if hit != st.held {
        if let Some(n) = st.held {
            sh.push_gui_event(Event::NoteOff { ch, note: n });
        }
        if let Some(n) = hit {
            sh.push_gui_event(Event::NoteOn { ch, note: n, vel: 100 });
        }
        st.held = hit;
    }
}

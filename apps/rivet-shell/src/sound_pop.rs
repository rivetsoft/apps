//! The Sound popup, opened from the panel's volume item: the output's volume and the outputs to
//! choose from, the same for the microphone, and each app playing with its own volume and the
//! output it plays on. Read and changed through audio.rs.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;

use crate::audio::{Audio, DevKind, Device, Job, Target};
use crate::conn::{head, note, row, section};
use crate::glyph;
use crate::quick::tool_button;
use crate::ui::{Act, Kit, Model, find_app, one_line, ui_font};

const HEAD: f32 = 46.0;
const SLIDER: f32 = 36.0;
const SECTION: f32 = 26.0;
const ROW: f32 = 42.0;
const APP: f32 = 54.0;
const NOTE: f32 = 40.0;
const ERR: f32 = 30.0;
const GAP: f32 = 6.0;

/// The mixer app to open from the popup, when installed.
const MIXERS: [&str; 3] = ["pavucontrol", "org.pulseaudio.pavucontrol", "com.saivert.pwvucontrol"];

pub fn height(m: &Model) -> f32 {
    let a = &m.extra.audio;
    let mut h = 28.0 + HEAD + 6.0;
    if !a.read {
        return h + NOTE;
    }
    if a.outputs.is_empty() && a.inputs.is_empty() {
        h += NOTE;
    }
    if !a.outputs.is_empty() {
        h += SLIDER + SECTION + a.outputs.len() as f32 * ROW + GAP;
    }
    if !a.inputs.is_empty() {
        h += SECTION + SLIDER + a.inputs.len() as f32 * ROW + GAP;
    }
    if !a.apps.is_empty() {
        h += SECTION + a.apps.len() as f32 * APP;
    }
    if a.error.is_some() {
        h += ERR;
    }
    h
}

/// A drag's key: which volume.
fn key(t: Target) -> u64 {
    match t {
        Target::Output(i) => i as u64,
        Target::Input(i) => (1 << 32) | i as u64,
        Target::App(i) => (2 << 32) | i as u64,
    }
}

/// A volume slider, 0–100 (more than 100 shows full); Some(new value) while dragged.
fn slider(ui: &mut egui::Ui, r: Rect, k: u64, value: u32, drag: &mut Option<(u64, u32)>, accent: Color32) -> Option<u32> {
    let v = ui.visuals().clone();
    let resp = ui.interact(r, egui::Id::unique(("sound-slider", k)), Sense::click_and_drag());
    let track = Rect::from_center_size(r.center(), Vec2::new(r.width() - 12.0, 4.0));
    let mut out = None;
    if resp.is_pointer_button_down_on()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let nv = (((p.x - track.left()) / track.width()).clamp(0.0, 1.0) * 100.0).round() as u32;
        if *drag != Some((k, nv)) {
            *drag = Some((k, nv));
            out = Some(nv);
        }
    } else if drag.is_some_and(|d| d.0 == k) {
        *drag = None;
    }
    let shown = drag.filter(|d| d.0 == k).map(|d| d.1).unwrap_or(value);
    let frac = shown.min(100) as f32 / 100.0;
    let p = ui.painter();
    p.rect_filled(track, 2.0, mix(v.panel_fill, v.text_color(), 0.16));
    p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * frac, track.height())), 2.0, accent);
    let knob = Rect::from_center_size(Pos2::new(track.left() + track.width() * frac, track.center().y), Vec2::new(12.0, 18.0));
    let hot = resp.hovered() || resp.dragged();
    p.rect_filled(knob, 6.0, v.panel_fill);
    p.rect_stroke(knob, 6.0, Stroke::new(1.5, if hot { accent } else { mix(v.panel_fill, v.text_color(), 0.45) }), egui::StrokeKind::Inside);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    out
}

/// A row with a symbol (click: mute), a slider and the percent.
#[allow(clippy::too_many_arguments)]
fn volume_row(ui: &mut egui::Ui, t: Target, d: &Device, input: bool, drag: &mut Option<(u64, u32)>, accent: Color32, acts: &mut Vec<Act>) {
    let v = ui.visuals().clone();
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), SLIDER), Sense::hover());
    let ir = Rect::from_center_size(Pos2::new(r.left() + 16.0, r.center().y), Vec2::new(30.0, 28.0));
    let col = if d.muted { v.weak_text_color() } else { v.text_color() };
    let tip = match (input, d.muted) {
        (false, false) => "Mute",
        (true, false) => "Mute the microphone",
        _ => "Unmute",
    };
    if tool_button(ui, ir, &format!("sound-mute-{}", key(t)), tip, &|p, c, _| if input { glyph::mic(p, c, col, d.muted) } else { glyph::speaker(p, c + Vec2::new(2.0, 0.0), d.muted, col) }) {
        acts.push(Act::Audio(Job::Mute(t)));
    }
    let shown = drag.filter(|x| x.0 == key(t)).map(|x| x.1).unwrap_or(d.volume);
    let label = if d.muted { "Muted".to_string() } else { format!("{shown}%") };
    ui.painter().text(Pos2::new(r.right() - 2.0, r.center().y), Align2::RIGHT_CENTER, label, ui_font(12.5), v.weak_text_color());
    let sr = Rect::from_min_max(Pos2::new(ir.right() + 8.0, r.top()), Pos2::new(r.right() - 50.0, r.bottom()));
    if let Some(nv) = slider(ui, sr, key(t), d.volume, drag, accent) {
        acts.push(Act::Audio(Job::Volume(t, nv)));
    }
}

/// A device's symbol.
fn dev_glyph(p: &egui::Painter, c: Pos2, kind: DevKind, col: Color32) {
    let s = Stroke::new(1.5, col);
    match kind {
        DevKind::Speaker => glyph::speaker(p, c + Vec2::new(2.0, 0.0), false, col),
        DevKind::Mic => glyph::mic(p, c, col, false),
        DevKind::Bluetooth => glyph::bluetooth(p, c, col, true),
        DevKind::Network => glyph::wifi(p, c, col),
        DevKind::Screen => {
            let r = Rect::from_center_size(c - Vec2::new(0.0, 1.5), Vec2::new(16.0, 11.0));
            p.rect_stroke(r, 2.0, s, egui::StrokeKind::Inside);
            p.line_segment([Pos2::new(c.x - 4.0, r.bottom() + 3.0), Pos2::new(c.x + 4.0, r.bottom() + 3.0)], s);
        }
        DevKind::Headphones | DevKind::Headset => {
            // A band over the head, and the two ear cups.
            let mut pts = Vec::new();
            for i in 0..=16 {
                let a = std::f32::consts::PI * (1.0 + i as f32 / 16.0);
                pts.push(c + Vec2::new(a.cos() * 7.0, a.sin() * 7.0 + 2.0));
            }
            p.add(egui::Shape::line(pts, s));
            for x in [-7.0, 7.0] {
                p.rect_filled(Rect::from_center_size(c + Vec2::new(x, 4.0), Vec2::new(3.5, 7.0)), 1.5, col);
            }
            if kind == DevKind::Headset {
                p.line_segment([c + Vec2::new(-7.0, 7.0), c + Vec2::new(-2.0, 9.0)], s);
            }
        }
    }
}

fn device_rows(ui: &mut egui::Ui, list: &[Device], input: bool, accent: Color32, acts: &mut Vec<Act>) {
    let v = ui.visuals().clone();
    for d in list {
        let sub = if d.sub.is_empty() { if d.default { "In use" } else { "" } } else { d.sub.as_str() };
        let (r, resp) = row(ui, d.default, &d.label, sub, None, 8.0);
        dev_glyph(ui.painter(), Pos2::new(r.left() + 17.0, r.center().y), d.kind, if d.default { accent } else { v.text_color() });
        let tip = if d.default { "In use" } else if input { "Use this microphone" } else { "Play sound here" };
        if resp.on_hover_text(tip).clicked() && !d.default {
            acts.push(Act::Audio(Job::Default { input, name: d.name.clone() }));
        }
    }
}

pub fn popup(ui: &mut egui::Ui, drag: &mut Option<(u64, u32)>, m: &mut Model, kit: &mut Kit) {
    let accent = kit.accent(m.s);
    let v = ui.visuals().clone();
    ui.spacing_mut().item_spacing.y = 0.0;
    let a: Audio = m.extra.audio.clone();
    let mut acts: Vec<Act> = Vec::new();
    let sub = match a.default_output() {
        Some(d) if d.muted => format!("{} · muted", d.label),
        Some(d) => d.label.clone(),
        None if a.read => "No output".into(),
        None => String::new(),
    };
    let (tool, _) = head(ui, "Sound", &sub, None, accent);
    // The full mixer, at the right of the title.
    let mixer = MIXERS.iter().find(|id| find_app(m.apps, id).is_some()).copied();
    if let Some(app) = mixer {
        let r = tool.translate(Vec2::new(40.0, 0.0));
        if tool_button(ui, r, "sound-mixer", "Sound settings", &|p, c, col| glyph::sliders(p, c, col)) {
            acts.push(Act::Launch(app.to_string()));
        }
    }
    if !a.read {
        note(ui, "Reading…");
        m.acts.extend(acts);
        return;
    }
    if a.outputs.is_empty() && a.inputs.is_empty() {
        note(ui, "No sound devices");
    }
    let room = ui.available_height();
    egui::ScrollArea::vertical().id_salt("sound-list").auto_shrink([false, true]).max_height(room).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if let Some(d) = a.default_output() {
            volume_row(ui, Target::Output(d.index), d, false, drag, accent, &mut acts);
        }
        if !a.outputs.is_empty() {
            section(ui, "Output");
            device_rows(ui, &a.outputs, false, accent, &mut acts);
            ui.add_space(GAP);
        }
        if !a.inputs.is_empty() {
            section(ui, "Input");
            if let Some(d) = a.default_input() {
                volume_row(ui, Target::Input(d.index), d, true, drag, accent, &mut acts);
            } else {
                ui.add_space(SLIDER);
            }
            device_rows(ui, &a.inputs, true, accent, &mut acts);
            ui.add_space(GAP);
        }
        if !a.apps.is_empty() {
            section(ui, "Apps");
            for s in &a.apps {
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), APP), Sense::hover());
                let p = ui.painter().clone();
                // The app's icon (click: mute it), its name and what it plays.
                let ir = Rect::from_center_size(Pos2::new(r.left() + 16.0, r.top() + 27.0), Vec2::splat(30.0));
                // Its desktop entry's icon (by its icon or program name), else the theme's.
                let tex = match find_app(m.apps, &s.icon).or_else(|| m.apps.iter().find(|x| x.name.eq_ignore_ascii_case(&s.app))) {
                    Some(app) => kit.app_icon(ui.ctx(), app, 22.0),
                    None => kit.icons.texture(ui.ctx(), &[s.icon.as_str(), "audio-x-generic"], (22.0 * ui.ctx().pixels_per_point()) as u32),
                };
                let muted = s.muted;
                let col = if muted { v.weak_text_color() } else { v.text_color() };
                if tool_button(ui, ir, &format!("sound-app-{}", s.index), if muted { "Unmute" } else { "Mute" }, &|p, c, _| match &tex {
                    Some(t) => {
                        let tint = if muted { Color32::from_white_alpha(90) } else { Color32::WHITE };
                        p.image(t.id(), Rect::from_center_size(c, Vec2::splat(22.0)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
                    }
                    None => glyph::speaker(p, c, muted, col),
                }) {
                    acts.push(Act::Audio(Job::Mute(Target::App(s.index))));
                }
                let x = r.left() + 40.0;
                // The output it plays on: a small choice at the right when there are several.
                let mut right = r.right() - 2.0;
                if a.outputs.len() > 1 {
                    let i = a.outputs.iter().position(|d| d.index == s.device);
                    let cur = i.map(|i| a.outputs[i].label.clone()).unwrap_or_default();
                    let next = &a.outputs[i.map(|i| (i + 1) % a.outputs.len()).unwrap_or(0)];
                    let g = one_line(&p, &cur, ui_font(11.5), v.text_color(), 110.0);
                    let cr = Rect::from_min_max(Pos2::new(r.right() - g.size().x - 16.0, r.top() + 4.0), Pos2::new(r.right(), r.top() + 24.0));
                    let resp = ui.interact(cr, egui::Id::unique(("sound-app-out", s.index)), Sense::click());
                    p.rect_filled(cr, 10.0, mix(v.panel_fill, v.text_color(), if resp.hovered() { 0.14 } else { 0.07 }));
                    p.galley(cr.center() - g.size() / 2.0, g, v.text_color());
                    let tip = format!("Plays on {cur} · click to move it to {}", next.label);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() {
                        acts.push(Act::Audio(Job::Move { app: s.index, device: next.index }));
                    }
                    right = cr.left() - 6.0;
                }
                let name = if s.title.is_empty() || s.title == s.app { s.app.clone() } else { format!("{} · {}", s.app, s.title) };
                let g = one_line(&p, &name, ui_font(12.5), v.text_color(), right - x);
                p.galley(Pos2::new(x, r.top() + 14.0 - g.size().y / 2.0), g, v.text_color());
                let sr = Rect::from_min_max(Pos2::new(x - 6.0, r.top() + 28.0), Pos2::new(r.right() - 44.0, r.bottom() - 4.0));
                if let Some(nv) = slider(ui, sr, key(Target::App(s.index)), s.volume, drag, accent) {
                    acts.push(Act::Audio(Job::Volume(Target::App(s.index), nv)));
                }
                let shown = drag.filter(|x| x.0 == key(Target::App(s.index))).map(|x| x.1).unwrap_or(s.volume);
                p.text(Pos2::new(r.right() - 2.0, sr.center().y), Align2::RIGHT_CENTER, if muted { "Muted".to_string() } else { format!("{shown}%") }, ui_font(12.0), v.weak_text_color());
            }
        }
        if let Some(e) = &a.error {
            let danger = rivet_look::theme::status(ui.ctx()).danger;
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ERR), Sense::hover());
            let g = one_line(ui.painter(), e, ui_font(12.0), danger, r.width() - 8.0);
            ui.painter().galley(Pos2::new(r.left() + 4.0, r.center().y - g.size().y / 2.0), g, danger);
        }
    });
    m.acts.extend(acts);
}

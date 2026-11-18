//! The control center (who's logged in, sliders, quick toggles, what's playing, the
//! notifications), the notification cards in the corner, and the on-screen display.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{Mode, r_lg, r_md};

use crate::glyph;
use crate::notes::Note;
use crate::sys::Net;
use crate::ui::{Act, Kit, Model, OsdKind, Popup, PopupKind, ago, card, on, one_line, ui_font, wrapped};

/// The cards' surface: its width, and the room around the cards for shadows.
pub const CARDS_W: f32 = 380.0;
const CARD_PAD: f32 = 10.0;
const CARD_GAP: f32 = 8.0;
/// The on-screen display's surface.
pub const OSD_W: f32 = 300.0;
pub const OSD_H: f32 = 64.0;

// ------------------------------------------------------------------ notification cards

fn card_height(n: &Note) -> f32 {
    let base = if n.body.is_empty() { 62.0 } else { 84.0 };
    base + if n.buttons().next().is_some() { 36.0 } else { 0.0 }
}

/// The cards' surface height for these notifications.
pub fn cards_height(notes: &[Note]) -> f32 {
    if notes.is_empty() {
        return 1.0;
    }
    CARD_PAD * 2.0 + notes.iter().map(card_height).sum::<f32>() + CARD_GAP * (notes.len() as f32 - 1.0)
}

/// Notifications in the corner, newest on top.
pub fn cards(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let full = ui.max_rect();
    let p = ui.painter().clone();
    let mut y = full.top() + CARD_PAD;
    let danger = rivet_look::theme::status(ui.ctx()).danger;
    for n in &m.extra.cards {
        let r = Rect::from_min_size(Pos2::new(full.left() + CARD_PAD, y), Vec2::new(full.width() - CARD_PAD * 2.0, card_height(n)));
        y = r.bottom() + CARD_GAP;
        let resp = ui.interact(r, egui::Id::unique(("card", n.id)), Sense::click());
        card(&p, r, &v, 0.97, r_lg());
        if n.urgency >= 2 {
            p.rect_filled(Rect::from_min_size(r.min + Vec2::new(0.0, 14.0), Vec2::new(3.0, r.height() - 28.0)), 2.0, danger);
        }
        if resp.hovered() {
            p.rect_filled(r.shrink(1.0), r_lg(), mix(v.panel_fill, v.text_color(), 0.04));
        }
        note_body(ui, &p, n, r.shrink2(Vec2::new(14.0, 12.0)), m, kit, accent, false);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            m.acts.push(Act::NoteClick(n.id));
        }
    }
}

/// A notification in `r`: icon, app and time, title, text, its buttons, and a close button
/// (`history`: removes it from the history; else takes the card off the screen).
#[allow(clippy::too_many_arguments)]
fn note_body(ui: &mut egui::Ui, p: &egui::Painter, n: &Note, r: Rect, m: &mut Model, kit: &mut Kit, accent: Color32, history: bool) {
    let v = ui.visuals().clone();
    let icon_px = if history { 26.0 } else { 34.0 };
    let ir = Rect::from_min_size(r.min + Vec2::new(0.0, 2.0), Vec2::splat(icon_px));
    match kit.note_icon(ui.ctx(), n, m.apps, icon_px) {
        Some(t) => {
            p.image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            p.circle_filled(ir.center(), icon_px / 2.0, accent.gamma_multiply(0.25));
            glyph::bell(p, ir.center(), accent, false);
        }
    }
    let tx = ir.right() + 12.0;
    let tw = r.right() - tx - 6.0;
    let weak = v.weak_text_color();
    let head = format!("{} · {}", if n.app.is_empty() { "Notification" } else { n.app.as_str() }, ago(n.at));
    let g = one_line(p, &head, ui_font(11.5), weak, tw - 20.0);
    p.galley(Pos2::new(tx, r.top()), g, weak);
    let g = one_line(p, &n.summary, ui_font(14.0), v.strong_text_color(), tw);
    let mut yy = r.top() + 17.0;
    let gh = g.size().y;
    p.galley(Pos2::new(tx, yy), g, v.strong_text_color());
    yy += gh + 2.0;
    if !n.body.is_empty() {
        let g = wrapped(p, &n.body, ui_font(12.5), v.text_color(), tw, if history { 1 } else { 2 });
        p.galley(Pos2::new(tx, yy), g, v.text_color());
    }
    // Close.
    let xr = Rect::from_center_size(Pos2::new(r.right() - 6.0, r.top() + 7.0), Vec2::splat(20.0));
    let xresp = ui.interact(xr, egui::Id::unique(("note-x", n.id, history)), Sense::click());
    if xresp.hovered() {
        p.circle_filled(xr.center(), 10.0, mix(v.panel_fill, v.text_color(), 0.12));
    }
    glyph::x(p, xr.center(), 3.5, weak);
    if xresp.on_hover_text(if history { "Remove" } else { "Hide" }).clicked() {
        m.acts.push(if history { Act::NoteRemove(n.id) } else { Act::NoteHide(n.id) });
    }
    // The app's buttons.
    if !history {
        let mut bx = tx;
        for (k, label) in n.buttons().take(3) {
            let g = p.layout_no_wrap(label.clone(), ui_font(12.5), v.text_color());
            let br = Rect::from_min_size(Pos2::new(bx, r.bottom() - 28.0), Vec2::new(g.size().x + 24.0, 28.0));
            if br.right() > r.right() {
                break;
            }
            let resp = ui.interact(br, egui::Id::unique(("note-btn", n.id, k)), Sense::click());
            p.rect_filled(br, r_md(), mix(v.panel_fill, v.text_color(), if resp.hovered() { 0.16 } else { 0.08 }));
            p.galley(br.center() - g.size() / 2.0, g, v.text_color());
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                m.acts.push(Act::NoteAction(n.id, k.clone()));
            }
            bx = br.right() + 6.0;
        }
    }
}

// ------------------------------------------------------------------ on-screen display

/// Volume or brightness as it changes: a small pill with a bar.
pub fn osd(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let Some((kind, value, muted)) = m.extra.osd else { return };
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let r = ui.max_rect().shrink(8.0);
    let p = ui.painter().clone();
    card(&p, r, &v, 0.97, (r.height() / 2.0) as u8);
    let fg = v.text_color();
    let c = Pos2::new(r.left() + 26.0, r.center().y);
    match kind {
        OsdKind::Volume => glyph::speaker(&p, c + Vec2::new(2.0, 0.0), muted, fg),
        OsdKind::Mic => glyph::mic(&p, c, fg, muted),
        OsdKind::Brightness => glyph::sun(&p, c, fg),
    }
    let text = if muted { "Muted".to_string() } else { format!("{value}%") };
    let g = p.layout_no_wrap(text, ui_font(13.0), fg);
    p.galley(Pos2::new(r.right() - 18.0 - g.size().x, r.center().y - g.size().y / 2.0), g, fg);
    let track = Rect::from_min_max(Pos2::new(r.left() + 48.0, r.center().y - 3.0), Pos2::new(r.right() - 64.0, r.center().y + 3.0));
    p.rect_filled(track, 3.0, mix(v.panel_fill, fg, 0.14));
    let frac = if muted { 0.0 } else { (value as f32 / 100.0).clamp(0.0, 1.0) };
    p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * frac, track.height())), 3.0, accent);
}

// ------------------------------------------------------------------ the control center

/// A quick-settings tile: what it is, whether it's on, and what clicking does.
struct Tile {
    title: &'static str,
    sub: String,
    on: bool,
    glyph: fn(&egui::Painter, Pos2, Color32, bool),
    act: Act,
}

fn tiles(m: &Model) -> Vec<Tile> {
    let st = m.status;
    let ex = m.extra;
    let mut t = Vec::new();
    if let Some(on) = st.wifi_on {
        let sub = match (&st.net, on) {
            (_, false) => "Off".to_string(),
            (Net::Wifi(ssid), true) if !ssid.is_empty() => ssid.clone(),
            (Net::Wired, true) => "On · wired in use".into(),
            _ => "On".into(),
        };
        t.push(Tile { title: "Wi-Fi", sub, on, glyph: |p, c, col, _| glyph::wifi(p, c, col), act: Act::Wifi(!on) });
    }
    if let Some(on) = st.bluetooth {
        t.push(Tile { title: "Bluetooth", sub: if on { "On" } else { "Off" }.into(), on, glyph: glyph::bluetooth, act: Act::Bluetooth(!on) });
    }
    if let Some(on) = ex.night {
        t.push(Tile { title: "Night Light", sub: if on { format!("{} K", m.s.night_kelvin) } else { "Off".into() }, on, glyph: glyph::moon, act: Act::NightLight(!on) });
    }
    if ex.notifying {
        t.push(Tile { title: "Do Not Disturb", sub: if ex.dnd { "On" } else { "Off" }.into(), on: ex.dnd, glyph: |p, c, col, _| glyph::bell(p, c, col, true), act: Act::Dnd(!ex.dnd) });
    }
    if let Some(on) = ex.awake {
        t.push(Tile { title: "Keep Awake", sub: if on { "Screen stays on" } else { "Off" }.into(), on, glyph: glyph::cup, act: Act::KeepAwake(!on) });
    }
    if let Some(pr) = &st.profile {
        let sub = match pr.as_str() {
            "power-saver" => "Power saver",
            "performance" => "Performance",
            _ => "Balanced",
        };
        t.push(Tile { title: "Power Mode", sub: sub.into(), on: pr != "balanced", glyph: glyph::bolt, act: Act::NextProfile });
    }
    if let Some((_, muted)) = st.mic {
        t.push(Tile { title: "Microphone", sub: if muted { "Muted" } else { "On" }.into(), on: !muted, glyph: |p, c, col, on| glyph::mic(p, c, col, !on), act: Act::MicMute });
    }
    let dark = m.s.color_mode == Mode::Dark;
    t.push(Tile { title: "Dark Style", sub: if dark { "On" } else { "Off" }.into(), on: dark, glyph: |p, c, col, _| glyph::contrast(p, c, col), act: Act::DarkStyle(!dark) });
    t
}

const NOTE_ROW: f32 = 64.0;

/// The control center's height for what it shows.
pub fn height(m: &Model) -> f32 {
    let sliders = m.status.volume.is_some() as usize + m.status.brightness.is_some() as usize;
    let rows = tiles(m).len().div_ceil(2);
    let media = if m.status.media.is_some() { 88.0 } else { 0.0 };
    let notes = if m.extra.notifying { 56.0 + if m.extra.notes.is_empty() { 40.0 } else { m.extra.notes.len().min(4) as f32 * (NOTE_ROW + 6.0) } } else { 0.0 };
    28.0 + 52.0 + 14.0 + sliders as f32 * 34.0 + 12.0 + rows as f32 * 62.0 + media + notes
}

/// A slider; `which` names it while it's held. Returns the new value while it's dragged.
fn slider(ui: &mut egui::Ui, which: u8, value: u32, drag: &mut Option<(u8, u32)>, accent: Color32) -> Option<u32> {
    let v = ui.visuals().clone();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click_and_drag());
    let track = Rect::from_center_size(r.center(), Vec2::new(r.width() - 16.0, 6.0));
    let mut out = None;
    if resp.is_pointer_button_down_on()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let nv = (((p.x - track.left()) / track.width()).clamp(0.0, 1.0) * 100.0).round() as u32;
        if *drag != Some((which, nv)) {
            *drag = Some((which, nv));
            out = Some(nv);
        }
    } else if drag.is_some_and(|d| d.0 == which) {
        *drag = None;
    }
    let shown = drag.filter(|d| d.0 == which).map(|d| d.1).unwrap_or(value);
    let frac = shown.min(100) as f32 / 100.0;
    let p = ui.painter();
    p.rect_filled(track, 3.0, mix(v.panel_fill, v.text_color(), 0.14));
    p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * frac, track.height())), 3.0, accent);
    let knob = Pos2::new(track.left() + track.width() * frac, track.center().y);
    p.circle_filled(knob, if resp.hovered() || resp.dragged() { 9.0 } else { 8.0 }, accent);
    p.circle_stroke(knob, 8.0, Stroke::new(2.0, v.panel_fill));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    out
}

/// One slider row: a symbol (a button when `click`), the slider, and the value.
#[allow(clippy::too_many_arguments)]
fn slider_row(ui: &mut egui::Ui, which: u8, value: u32, muted: bool, draw: &dyn Fn(&egui::Painter, Pos2, Color32), click: Option<(Act, &str)>, drag: &mut Option<(u8, u32)>, accent: Color32, acts: &mut Vec<Act>) -> Option<u32> {
    let v = ui.visuals().clone();
    let (fg, weak) = (v.text_color(), v.weak_text_color());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (gr, gresp) = ui.allocate_exact_size(Vec2::splat(30.0), if click.is_some() { Sense::click() } else { Sense::hover() });
        if gresp.hovered() && click.is_some() {
            ui.painter().circle_filled(gr.center(), 15.0, mix(v.panel_fill, fg, 0.1));
        }
        draw(ui.painter(), gr.center(), if muted { weak } else { fg });
        if let Some((a, tip)) = click
            && gresp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked()
        {
            acts.push(a);
        }
        let lw = 44.0;
        let out = ui.allocate_ui(Vec2::new(ui.available_width() - lw - 8.0, 28.0), |ui| slider(ui, which, value, drag, accent)).inner;
        let shown = drag.filter(|d| d.0 == which).map(|d| d.1).unwrap_or(value);
        let label = if muted { "Muted".to_string() } else { format!("{shown}%") };
        let (lr, _) = ui.allocate_exact_size(Vec2::new(lw, 28.0), Sense::hover());
        ui.painter().text(Pos2::new(lr.right(), lr.center().y), Align2::RIGHT_CENTER, label, ui_font(12.5), weak);
        out
    })
    .inner
}

pub fn control_center(ui: &mut egui::Ui, drag: &mut Option<(u8, u32)>, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let fg = v.text_color();
    let weak = v.weak_text_color();
    let w = ui.available_width();
    let mut acts: Vec<Act> = Vec::new();
    // ---- who, and how long the computer's been up; settings and power
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 52.0), Sense::hover());
    let p = ui.painter().clone();
    let face = Pos2::new(r.left() + 24.0, r.center().y);
    p.circle_filled(face, 22.0, accent);
    let initial = m.extra.user.chars().next().unwrap_or('?').to_uppercase().to_string();
    p.text(face, Align2::CENTER_CENTER, initial, ui_font(19.0), on(accent));
    let name = if m.extra.user.is_empty() { "You".to_string() } else { m.extra.user.clone() };
    let g = one_line(&p, &name, ui_font(15.0), v.strong_text_color(), w - 150.0);
    p.galley(Pos2::new(face.x + 34.0, r.center().y - 19.0), g, v.strong_text_color());
    let up = crate::sys::uptime();
    let up_text = if up >= 86400 { format!("Up {} d {} h", up / 86400, up / 3600 % 24) } else { format!("Up {} h {} min", up / 3600, up / 60 % 60) };
    p.text(Pos2::new(face.x + 34.0, r.center().y + 10.0), Align2::LEFT_CENTER, up_text, ui_font(12.5), weak);
    let round_btn = |ui: &mut egui::Ui, r: Rect, id: &str| -> egui::Response {
        let resp = ui.interact(r, egui::Id::unique(("cc", id)), Sense::click());
        ui.painter().circle_filled(r.center(), r.width() / 2.0, mix(v.panel_fill, fg, if resp.hovered() { 0.16 } else { 0.08 }));
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    };
    let pr = Rect::from_center_size(Pos2::new(r.right() - 18.0, r.center().y), Vec2::splat(36.0));
    if round_btn(ui, pr, "power").on_hover_text("Lock, suspend, restart, shut down").clicked() {
        acts.push(Act::Open(Popup::at(PopupKind::Power { confirm: None }, pr.center().x, m.output)));
    }
    glyph::power(&p, pr.center(), fg);
    let sr = pr.translate(Vec2::new(-44.0, 0.0));
    if round_btn(ui, sr, "settings").on_hover_text("Settings").clicked() {
        acts.push(Act::Close);
        acts.push(Act::Settings);
    }
    glyph::gear(&p, sr.center(), fg);
    ui.add_space(14.0);
    // ---- sliders
    if let Some((vol, muted)) = m.status.volume
        && let Some(nv) = slider_row(ui, 0, vol, muted, &|p, c, col| glyph::speaker(p, c + Vec2::new(2.0, 0.0), muted, col), Some((Act::Mute, if muted { "Unmute" } else { "Mute" })), drag, accent, &mut acts)
    {
        acts.push(Act::SetVolume(nv));
    }
    if let Some(b) = m.status.brightness
        && let Some(nv) = slider_row(ui, 1, b, false, &|p, c, col| glyph::sun(p, c, col), None, drag, accent, &mut acts)
    {
        acts.push(Act::SetBrightness(nv));
    }
    ui.add_space(12.0);
    // ---- toggles, two to a row
    let tw = (w - 8.0) / 2.0;
    for pair in tiles(m).chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for t in pair {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(tw, 54.0), Sense::click());
                let (fill, col, subcol) = if t.on {
                    let fill = if resp.hovered() { mix(accent, fg, 0.12) } else { accent };
                    (fill, on(accent), on(accent).gamma_multiply(0.8))
                } else {
                    (mix(v.panel_fill, fg, if resp.hovered() { 0.13 } else { 0.07 }), fg, weak)
                };
                let p = ui.painter();
                p.rect_filled(r, r_lg(), fill);
                (t.glyph)(p, Pos2::new(r.left() + 24.0, r.center().y), col, t.on);
                p.text(Pos2::new(r.left() + 46.0, r.center().y - 9.0), Align2::LEFT_CENTER, t.title, ui_font(13.0), col);
                let g = one_line(p, &t.sub, ui_font(11.5), subcol, r.width() - 54.0);
                p.galley(Pos2::new(r.left() + 46.0, r.center().y + 2.0), g, subcol);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    acts.push(t.act.clone());
                }
            }
        });
        ui.add_space(8.0);
    }
    // ---- what's playing
    if let Some(md) = m.status.media.clone() {
        ui.add_space(4.0);
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 70.0), Sense::hover());
        let p = ui.painter().clone();
        p.rect_filled(r, r_lg(), mix(v.panel_fill, accent, 0.14));
        let tw = r.width() - 150.0;
        let g = one_line(&p, if md.title.is_empty() { "Playing" } else { &md.title }, ui_font(14.0), v.strong_text_color(), tw);
        p.galley(Pos2::new(r.left() + 16.0, r.top() + 15.0), g, v.strong_text_color());
        let sub = if md.artist.is_empty() { md.player.clone() } else { md.artist.clone() };
        let g = one_line(&p, &sub, ui_font(12.5), weak, tw);
        p.galley(Pos2::new(r.left() + 16.0, r.top() + 37.0), g, weak);
        for (k, (cmd, tip)) in [("previous", "Previous"), ("play-pause", if md.playing { "Pause" } else { "Play" }), ("next", "Next")].into_iter().enumerate() {
            let c = Pos2::new(r.right() - 112.0 + k as f32 * 40.0, r.center().y);
            let br = Rect::from_center_size(c, Vec2::splat(if k == 1 { 38.0 } else { 32.0 }));
            let resp = ui.interact(br, egui::Id::unique(("media", cmd)), Sense::click());
            if k == 1 {
                p.circle_filled(c, 19.0, if resp.hovered() { mix(accent, fg, 0.15) } else { accent });
                glyph::play(&p, c, !md.playing, on(accent));
            } else {
                if resp.hovered() {
                    p.circle_filled(c, 16.0, mix(v.panel_fill, fg, 0.12));
                }
                glyph::skip(&p, c, k == 2, fg);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() {
                acts.push(Act::Media(cmd));
            }
        }
        ui.add_space(14.0);
    }
    m.acts.extend(acts);
    // ---- notifications
    if m.extra.notifying {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Notifications").strong().size(14.0));
            if !m.extra.notes.is_empty() {
                ui.label(egui::RichText::new(m.extra.notes.len().to_string()).color(weak).size(12.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear All").clicked() {
                        m.acts.push(Act::ClearNotes);
                    }
                });
            }
        });
        ui.add_space(6.0);
        if m.extra.notes.is_empty() {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(if m.extra.dnd { "Nothing new · Do Not Disturb is on" } else { "Nothing new" }).color(weak));
        } else {
            let notes = m.extra.notes.clone();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for n in &notes {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), NOTE_ROW), Sense::hover());
                    let p = ui.painter().clone();
                    p.rect_filled(r, r_md(), mix(v.panel_fill, fg, 0.05));
                    note_body(ui, &p, n, r.shrink2(Vec2::new(12.0, 9.0)), m, kit, accent, true);
                    ui.add_space(6.0);
                }
            });
        }
    }
}

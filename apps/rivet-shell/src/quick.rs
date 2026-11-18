//! Quick settings, in the Rivet apps' own look rather than a phone's: a title row with
//! toolbar-style buttons, sections under small folding headings like Rivet Files' sidebar,
//! thin sliders, toggles that light up the way a selected item does in the apps (a soft
//! accent tint with an accent edge, never a solid block), and what's playing as a plain row
//! with toolbar buttons. (The notifications have their own panel: notes_pop.rs.)

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{Mode, r_md, r_sm, selected_tile};

use crate::glyph;
use crate::sys::Net;
use crate::ui::{Act, Kit, Model, Popup, PopupKind, one_line, ui_font};

const HEAD: f32 = 56.0;
const FOLD: f32 = 30.0;
const SLIDER: f32 = 40.0;
const TOGGLE: f32 = 52.0;
const MEDIA: f32 = 58.0;
const GAP: f32 = 12.0;
/// Between toggles, across and down.
const TGAP: f32 = 8.0;
/// The card's padding (ui.rs gives quick settings this much).
pub const PAD: f32 = 18.0;

/// A toggle: what it is, whether it's on, and what clicking does.
struct Toggle {
    title: &'static str,
    sub: String,
    on: bool,
    glyph: fn(&egui::Painter, Pos2, Color32, bool),
    act: Act,
}

fn toggles(m: &Model) -> Vec<Toggle> {
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
        t.push(Toggle { title: "Wi-Fi", sub, on, glyph: |p, c, col, _| glyph::wifi(p, c, col), act: Act::Wifi(!on) });
    }
    if let Some(on) = st.bluetooth {
        t.push(Toggle { title: "Bluetooth", sub: if on { "On" } else { "Off" }.into(), on, glyph: glyph::bluetooth, act: Act::Bluetooth(!on) });
    }
    if let Some(on) = ex.night {
        t.push(Toggle { title: "Night Light", sub: if on { format!("{} K", m.s.night_kelvin) } else { "Off".into() }, on, glyph: glyph::moon, act: Act::NightLight(!on) });
    }
    if ex.notifying {
        t.push(Toggle { title: "Do Not Disturb", sub: if ex.dnd { "On" } else { "Off" }.into(), on: ex.dnd, glyph: |p, c, col, _| glyph::bell(p, c, col, true), act: Act::Dnd(!ex.dnd) });
    }
    if let Some(on) = ex.awake {
        t.push(Toggle { title: "Keep Awake", sub: if on { "Screen stays on" } else { "Off" }.into(), on, glyph: glyph::cup, act: Act::KeepAwake(!on) });
    }
    if let Some(pr) = &st.profile {
        let sub = match pr.as_str() {
            "power-saver" => "Power saver",
            "performance" => "Performance",
            _ => "Balanced",
        };
        t.push(Toggle { title: "Power Mode", sub: sub.into(), on: pr != "balanced", glyph: glyph::bolt, act: Act::NextProfile });
    }
    if let Some((_, muted)) = st.mic {
        t.push(Toggle { title: "Microphone", sub: if muted { "Muted" } else { "On" }.into(), on: !muted, glyph: |p, c, col, on| glyph::mic(p, c, col, !on), act: Act::MicMute });
    }
    let dark = m.s.color_mode == Mode::Dark;
    t.push(Toggle { title: "Dark Style", sub: if dark { "On" } else { "Off" }.into(), on: dark, glyph: |p, c, col, _| glyph::contrast(p, c, col), act: Act::DarkStyle(!dark) });
    t
}

fn fold_id(name: &str) -> egui::Id {
    egui::Id::unique(("quick-fold", name))
}

fn folded(ctx: &egui::Context, name: &str) -> bool {
    ctx.data(|d| d.get_temp::<bool>(fold_id(name))).unwrap_or(false)
}

/// The sections shown, with whether each is folded.
fn sections(m: &Model, ctx: &egui::Context) -> Vec<(&'static str, bool)> {
    let mut s = Vec::new();
    if m.status.volume.is_some() || m.status.brightness.is_some() {
        s.push(("Sound and Display", folded(ctx, "Sound and Display")));
    }
    s.push(("Toggles", folded(ctx, "Toggles")));
    if m.status.media.is_some() {
        s.push(("Now Playing", folded(ctx, "Now Playing")));
    }
    s
}

/// Quick settings' height for what it shows.
pub fn height(m: &Model, ctx: &egui::Context) -> f32 {
    let mut h = 2.0 * PAD + HEAD;
    for (name, folded) in sections(m, ctx) {
        h += FOLD + GAP;
        if folded {
            continue;
        }
        h += match name {
            "Sound and Display" => (m.status.volume.is_some() as usize + m.status.brightness.is_some() as usize) as f32 * SLIDER,
            "Toggles" => toggles(m).len().div_ceil(2) as f32 * (TOGGLE + TGAP),
            _ => MEDIA,
        } + 8.0;
    }
    h
}

/// A small folding heading, like the sidebar's "My Computer".
fn heading(ui: &mut egui::Ui, name: &str, extra: Option<(&str, Act)>, acts: &mut Vec<Act>) -> bool {
    let v = ui.visuals().clone();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), FOLD), Sense::click());
    let open = !folded(ui.ctx(), name);
    let weak = v.weak_text_color();
    let col = if resp.hovered() { v.text_color() } else { weak };
    // ▸ / ▾
    let c = Pos2::new(r.left() + 7.0, r.center().y);
    let tri = if open { vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)] } else { vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(3.0, 0.0), c + Vec2::new(-2.0, 4.0)] };
    ui.painter().add(egui::Shape::convex_polygon(tri, col, Stroke::NONE));
    ui.painter().text(Pos2::new(r.left() + 18.0, r.center().y), Align2::LEFT_CENTER, name, ui_font(12.5), col);
    if let Some((label, act)) = extra {
        let g = ui.painter().layout_no_wrap(label.to_string(), ui_font(12.0), v.text_color());
        let br = Rect::from_min_max(Pos2::new(r.right() - g.size().x - 16.0, r.top() + 3.0), Pos2::new(r.right(), r.bottom() - 3.0));
        let bresp = ui.interact(br, egui::Id::unique(("quick-head-btn", name)), Sense::click());
        if bresp.hovered() {
            ui.painter().rect_filled(br, r_sm(), mix(v.panel_fill, v.text_color(), 0.09));
        }
        ui.painter().galley(br.center() - g.size() / 2.0, g, if bresp.hovered() { v.strong_text_color() } else { weak });
        if bresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            acts.push(act);
            return open;
        }
    }
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(fold_id(name), open));
    }
    open
}

/// A toolbar-style button (a hover fill, like Rivet Files' toolbar), with a drawn symbol.
pub fn tool_button(ui: &mut egui::Ui, r: Rect, id: &str, tip: &str, draw: &dyn Fn(&egui::Painter, Pos2, Color32)) -> bool {
    let v = ui.visuals().clone();
    let resp = ui.interact(r, egui::Id::unique(("quick-tool", id)), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, r_sm(), mix(v.panel_fill, v.text_color(), 0.09));
    }
    draw(ui.painter(), r.center(), if resp.hovered() { v.strong_text_color() } else { v.text_color() });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked()
}

/// A thin slider like the apps' zoom slider: a fine track, the part up to the value in the
/// accent colour, an outlined handle. Returns the new value while it's dragged.
fn slider(ui: &mut egui::Ui, which: u8, value: u32, drag: &mut Option<(u8, u32)>, accent: Color32, width: f32) -> Option<u32> {
    let v = ui.visuals().clone();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::click_and_drag());
    let track = Rect::from_center_size(r.center(), Vec2::new(r.width() - 12.0, 4.0));
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
    p.rect_filled(track, 2.0, mix(v.panel_fill, v.text_color(), 0.16));
    p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * frac, track.height())), 2.0, accent);
    let knob = Rect::from_center_size(Pos2::new(track.left() + track.width() * frac, track.center().y), Vec2::new(12.0, 18.0));
    let hot = resp.hovered() || resp.dragged();
    p.rect_filled(knob, 6.0, v.panel_fill);
    p.rect_stroke(knob, 6.0, Stroke::new(1.5, if hot { accent } else { mix(v.panel_fill, v.text_color(), 0.45) }), egui::StrokeKind::Inside);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    out
}

#[allow(clippy::too_many_arguments)]
fn slider_row(ui: &mut egui::Ui, which: u8, value: u32, muted: bool, draw: &dyn Fn(&egui::Painter, Pos2, Color32), click: Option<(Act, &str)>, drag: &mut Option<(u8, u32)>, accent: Color32, acts: &mut Vec<Act>) -> Option<u32> {
    let v = ui.visuals().clone();
    let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), SLIDER), Sense::hover());
    let ir = Rect::from_center_size(Pos2::new(row.left() + 16.0, row.center().y), Vec2::new(30.0, 28.0));
    let col = if muted { v.weak_text_color() } else { v.text_color() };
    match click {
        Some((a, tip)) => {
            if tool_button(ui, ir, &format!("slider-{which}"), tip, &|p, c, _| draw(p, c, col)) {
                acts.push(a);
            }
        }
        None => draw(ui.painter(), ir.center(), col),
    }
    let shown = drag.filter(|d| d.0 == which).map(|d| d.1).unwrap_or(value);
    let label = if muted { "Muted".to_string() } else { format!("{shown}%") };
    ui.painter().text(Pos2::new(row.right() - 2.0, row.center().y), Align2::RIGHT_CENTER, label, ui_font(12.5), v.weak_text_color());
    let sr = Rect::from_min_max(Pos2::new(ir.right() + 8.0, row.top()), Pos2::new(row.right() - 50.0, row.bottom()));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(sr).layout(egui::Layout::left_to_right(egui::Align::Center)));
    slider(&mut child, which, value, drag, accent, sr.width())
}

pub fn quick_settings(ui: &mut egui::Ui, drag: &mut Option<(u8, u32)>, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let (fg, weak) = (v.text_color(), v.weak_text_color());
    let w = ui.available_width();
    let mut acts: Vec<Act> = Vec::new();
    ui.spacing_mut().item_spacing.y = 0.0;
    // ---- the title row: who, how long up; settings and power as toolbar buttons
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, HEAD), Sense::hover());
    let p = ui.painter().clone();
    p.text(Pos2::new(r.left() + 2.0, r.top() + 14.0), Align2::LEFT_CENTER, "Quick Settings", ui_font(15.0), v.strong_text_color());
    let up = crate::sys::uptime();
    let up_text = if up >= 86400 { format!("up {} d {} h", up / 86400, up / 3600 % 24) } else { format!("up {} h {} min", up / 3600, up / 60 % 60) };
    let who = if m.extra.user.is_empty() { up_text } else { format!("{} · {up_text}", m.extra.user) };
    let g = one_line(&p, &who, ui_font(12.0), weak, w - 90.0);
    p.galley(Pos2::new(r.left() + 2.0, r.top() + 29.0), g, weak);
    let pr = Rect::from_center_size(Pos2::new(r.right() - 16.0, r.top() + 21.0), Vec2::new(32.0, 32.0));
    if tool_button(ui, pr, "power", "Lock, suspend, restart, shut down", &|p, c, col| glyph::power(p, c, col)) {
        acts.push(Act::Open(Popup::at(PopupKind::Power { confirm: None }, pr.center().x, m.output)));
    }
    let sr = pr.translate(Vec2::new(-38.0, 0.0));
    if tool_button(ui, sr, "settings", "Settings", &|p, c, col| glyph::gear(p, c, col)) {
        acts.push(Act::Close);
        acts.push(Act::Settings);
    }
    // A hairline under the title, as under the apps' toolbars.
    p.line_segment([Pos2::new(r.left(), r.bottom() - 1.0), Pos2::new(r.right(), r.bottom() - 1.0)], Stroke::new(1.0, mix(v.panel_fill, fg, 0.08)));
    for (name, _) in sections(m, ui.ctx()) {
        ui.add_space(GAP);
        if !heading(ui, name, None, &mut acts) {
            continue;
        }
        match name {
            "Sound and Display" => {
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
            }
            "Toggles" => {
                let tw = (w - TGAP) / 2.0;
                for pair in toggles(m).chunks(2) {
                    let (row, _) = ui.allocate_exact_size(Vec2::new(w, TOGGLE + TGAP), Sense::hover());
                    for (k, t) in pair.iter().enumerate() {
                        let r = Rect::from_min_size(Pos2::new(row.left() + k as f32 * (tw + TGAP), row.top()), Vec2::new(tw, TOGGLE));
                        let resp = ui.interact(r, egui::Id::unique(("quick-toggle", t.title)), Sense::click());
                        // On: lit like a selected item in the apps; off: a quiet row.
                        if t.on {
                            let (fill, stroke) = selected_tile(&v, v.panel_fill);
                            ui.painter().rect_filled(r, r_md(), if resp.hovered() { mix(fill, fg, 0.05) } else { fill });
                            ui.painter().rect_stroke(r, r_md(), stroke, egui::StrokeKind::Inside);
                        } else {
                            ui.painter().rect_filled(r, r_md(), mix(v.panel_fill, fg, if resp.hovered() { 0.09 } else { 0.04 }));
                        }
                        let icol = if t.on { accent } else { weak };
                        (t.glyph)(ui.painter(), Pos2::new(r.left() + 22.0, r.center().y), icol, t.on);
                        let tcol = if t.on { v.strong_text_color() } else { fg };
                        ui.painter().text(Pos2::new(r.left() + 44.0, r.center().y - 9.0), Align2::LEFT_CENTER, t.title, ui_font(13.0), tcol);
                        // Wi-Fi and Bluetooth: a › at the right opens their list.
                        let more = match t.title {
                            "Wi-Fi" => Some((PopupKind::Network { ask: None }, "Networks")),
                            "Bluetooth" => Some((PopupKind::Bluetooth, "Devices")),
                            _ => None,
                        };
                        let room = if more.is_some() { 26.0 } else { 0.0 };
                        let g = one_line(ui.painter(), &t.sub, ui_font(11.5), weak, r.width() - 52.0 - room);
                        ui.painter().galley(Pos2::new(r.left() + 44.0, r.center().y + 2.0), g, weak);
                        if let Some((kind, tip)) = more {
                            let mr = Rect::from_min_max(Pos2::new(r.right() - room - 2.0, r.top() + 6.0), Pos2::new(r.right() - 6.0, r.bottom() - 6.0));
                            let mresp = ui.interact(mr, egui::Id::unique(("quick-more", t.title)), Sense::click());
                            if mresp.hovered() {
                                ui.painter().rect_filled(mr, r_sm(), mix(v.panel_fill, fg, 0.10));
                            }
                            let c = mr.center();
                            let col = if mresp.hovered() { v.strong_text_color() } else { weak };
                            ui.painter().add(egui::Shape::line(vec![c + Vec2::new(-2.0, -4.5), c + Vec2::new(2.5, 0.0), c + Vec2::new(-2.0, 4.5)], Stroke::new(1.6, col)));
                            if mresp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() {
                                acts.push(Act::Open(Popup::at(kind, mr.center().x, m.output)));
                                continue;
                            }
                        }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            acts.push(t.act.clone());
                        }
                    }
                }
            }
            _ => {
                let Some(md) = m.status.media.clone() else { continue };
                let (r, _) = ui.allocate_exact_size(Vec2::new(w, MEDIA), Sense::hover());
                let p = ui.painter().clone();
                glyph::play(&p, Pos2::new(r.left() + 16.0, r.center().y), !md.playing, accent);
                let tw = r.width() - 150.0;
                let g = one_line(&p, if md.title.is_empty() { "Playing" } else { &md.title }, ui_font(13.5), v.strong_text_color(), tw);
                p.galley(Pos2::new(r.left() + 38.0, r.top() + 10.0), g, v.strong_text_color());
                let sub = if md.artist.is_empty() { md.player.clone() } else { md.artist.clone() };
                let g = one_line(&p, &sub, ui_font(12.0), weak, tw);
                p.galley(Pos2::new(r.left() + 38.0, r.top() + 31.0), g, weak);
                for (k, (cmd, tip)) in [("previous", "Previous"), ("play-pause", if md.playing { "Pause" } else { "Play" }), ("next", "Next")].into_iter().enumerate() {
                    let br = Rect::from_center_size(Pos2::new(r.right() - 92.0 + k as f32 * 36.0, r.center().y), Vec2::new(30.0, 30.0));
                    let clicked = match k {
                        1 => tool_button(ui, br, cmd, tip, &|p, c, col| glyph::play(p, c, !md.playing, col)),
                        _ => tool_button(ui, br, cmd, tip, &|p, c, col| glyph::skip(p, c, k == 2, col)),
                    };
                    if clicked {
                        acts.push(Act::Media(cmd));
                    }
                }
            }
        }
        ui.add_space(8.0);
    }
    m.acts.extend(acts);
}

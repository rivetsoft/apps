//! The player's popup, opened from the Now Playing item: the cover, the track and where it's
//! from, bars moving with the sound, how far in it is (click to jump), and previous, play or
//! pause, and next.

use rivet_look::a11y::{Access, Role};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::r_md;

use crate::glyph;
use crate::quick::tool_button;
use crate::ui::{Act, Kit, Model, on, one_line, ui_font};

pub const WIDTH: f32 = 372.0;
pub const PAD: f32 = 18.0;
const COVER: f32 = 92.0;
const VIZ: f32 = 46.0;
const PROGRESS: f32 = 30.0;
const CONTROLS: f32 = 44.0;
const EMPTY: f32 = 96.0;

pub fn height(m: &Model) -> f32 {
    if m.status.media.is_none() {
        return EMPTY;
    }
    let viz = if m.extra.viz.is_some() { VIZ + 14.0 } else { 0.0 };
    let progress = if m.status.media_pos.1 > 0 { PROGRESS + 6.0 } else { 0.0 };
    PAD * 2.0 + COVER + 14.0 + viz + progress + CONTROLS
}

/// 3:07, or 1:02:45 past the hour.
pub fn clock(secs: u32) -> String {
    if secs >= 3600 { format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60) } else { format!("{}:{:02}", secs / 60, secs % 60) }
}

pub fn popup(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let weak = v.weak_text_color();
    let area = ui.max_rect();
    let p = ui.painter().clone();
    let Some(md) = m.status.media.clone() else {
        p.text(area.center(), Align2::CENTER_CENTER, "Nothing is playing.", ui_font(13.5), weak);
        return;
    };
    let accent = kit.tone(m.s, crate::style::Slot::Active);

    // The cover (else a tile with a play symbol), and the track beside it.
    let cr = Rect::from_min_size(area.min, Vec2::splat(COVER));
    match (!md.art.is_empty()).then(|| kit.named_icon(ui.ctx(), &md.art, COVER)).flatten() {
        Some(t) => crate::bar::rounded_image(&p, &t, cr, r_md() as f32, Color32::WHITE),
        None => {
            p.rect_filled(cr, r_md(), mix(v.panel_fill, accent, 0.2));
            glyph::play(&p, cr.center(), true, accent);
        }
    }
    let tx = cr.right() + 14.0;
    let tw = area.right() - tx;
    let lines = [(if md.title.is_empty() { "Playing" } else { md.title.as_str() }, 15.5, v.strong_text_color()), (md.artist.as_str(), 13.0, v.text_color()), (md.album.as_str(), 12.5, weak)];
    let mut y = cr.top() + 6.0;
    for (text, size, col) in lines.into_iter().filter(|l| !l.0.is_empty()) {
        let g = one_line(&p, text, ui_font(size), col, tw);
        let h = g.size().y;
        p.galley(Pos2::new(tx, y), g, col);
        y += h + 4.0;
    }
    // Which app it's from, at the cover's foot.
    let g = one_line(&p, &md.player, ui_font(11.5), weak, tw);
    p.galley(Pos2::new(tx, cr.bottom() - g.size().y - 2.0), g, weak);
    let mut y = cr.bottom() + 14.0;

    if let Some(bands) = &m.extra.viz {
        let vr = Rect::from_min_size(Pos2::new(area.left(), y), Vec2::new(area.width(), VIZ));
        crate::bar::viz_bars(&p, vr, bands, if md.playing { accent } else { mix(v.panel_fill, v.text_color(), 0.25) });
        y += VIZ + 14.0;
    }

    let (pos, len) = m.status.media_pos;
    if len > 0 {
        let row = Rect::from_min_size(Pos2::new(area.left(), y), Vec2::new(area.width(), PROGRESS));
        let track = Rect::from_min_max(Pos2::new(row.left(), row.top() + 4.0), Pos2::new(row.right(), row.top() + 8.0));
        let resp = ui.interact(track.expand2(Vec2::new(0.0, 7.0)), egui::Id::unique("media-seek"), Sense::click()).role(Role::Slider, "Position");
        p.rect_filled(track, 2.0, mix(v.panel_fill, v.text_color(), 0.14));
        let part = (pos as f32 / len as f32).clamp(0.0, 1.0);
        p.rect_filled(Rect::from_min_max(track.min, Pos2::new(track.left() + track.width() * part, track.bottom())), 2.0, accent);
        if resp.hovered() {
            p.circle_filled(Pos2::new(track.left() + track.width() * part, track.center().y), 5.0, accent);
        }
        if let Some(at) = resp.interact_pointer_pos().filter(|_| resp.clicked()) {
            let to = ((at.x - track.left()) / track.width()).clamp(0.0, 1.0) * len as f32;
            m.acts.push(Act::MediaSeek(to as u32));
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        p.text(Pos2::new(row.left(), row.bottom() - 7.0), Align2::LEFT_CENTER, clock(pos), ui_font(11.5), weak);
        p.text(Pos2::new(row.right(), row.bottom() - 7.0), Align2::RIGHT_CENTER, clock(len), ui_font(11.5), weak);
        y += PROGRESS + 6.0;
    }

    // Previous, play or pause (on the accent), next.
    let c = Pos2::new(area.center().x, y + CONTROLS / 2.0);
    for (dx, cmd, tip, forward) in [(-58.0, "previous", "Previous", false), (58.0, "next", "Next", true)] {
        let br = Rect::from_center_size(c + Vec2::new(dx, 0.0), Vec2::splat(36.0));
        if tool_button(ui, br, cmd, tip, &|p, c, col| glyph::skip(p, c, forward, col)) {
            m.acts.push(Act::Media(cmd));
        }
    }
    let br = Rect::from_center_size(c, Vec2::splat(CONTROLS));
    let resp = ui.interact(br, egui::Id::unique("media-play"), Sense::click()).role(Role::Button, if md.playing { "Pause" } else { "Play" });
    p.circle_filled(c, CONTROLS / 2.0, if resp.hovered() { mix(accent, v.text_color(), 0.12) } else { accent });
    glyph::play(&p, c + Vec2::new(if md.playing { 0.0 } else { 1.0 }, 0.0), !md.playing, on(accent));
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if md.playing { "Pause" } else { "Play" }).clicked() {
        m.acts.push(Act::Media("play-pause"));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn times() {
        assert_eq!(super::clock(187), "3:07");
        assert_eq!(super::clock(3765), "1:02:45");
    }
}

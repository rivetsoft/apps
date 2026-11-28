//! The screen's face: the time and date, who it is, the password field, what the password
//! check said, and suspend, restart and shut down. Drawn over the (blurred) wallpaper, the same
//! on every screen, by the lock screen and the login screen alike.

use rivet_look::a11y::{Access, Role};
use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use eframe::egui::{self, Align2, Color32, FontId, Galley, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::fonts::ui_font;

/// The login screen's hint while there's more than one account and nothing is typed.
pub const ANOTHER: &str = "Press Tab to sign in as someone else";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockAct {
    Suspend,
    Restart,
    ShutDown,
}

impl LockAct {
    fn label(self) -> &'static str {
        match self {
            LockAct::Suspend => "Suspend",
            LockAct::Restart => "Restart",
            LockAct::ShutDown => "Shut Down",
        }
    }

    /// What systemctl is asked to do.
    pub fn systemctl(self) -> &'static str {
        match self {
            LockAct::Suspend => "suspend",
            LockAct::Restart => "reboot",
            LockAct::ShutDown => "poweroff",
        }
    }
}

/// What the face shows.
pub struct View<'a> {
    /// The person's name.
    pub name: &'a str,
    /// Their picture (square), else their initial.
    pub avatar: Option<&'a egui::TextureHandle>,
    /// Characters typed so far (shown as dots).
    pub typed: usize,
    pub caps_lock: bool,
    /// Checking the password.
    pub busy: bool,
    /// A message under the field: (text, is it a problem).
    pub message: Option<(&'a str, bool)>,
    /// How long ago a wrong password was turned away (the field shakes for a moment).
    pub since_error: Option<Duration>,
    /// Suspend, restart and shut down.
    pub power: bool,
    /// A power button clicked once, waiting for the second click.
    pub confirm: Option<LockAct>,
    pub accent: Color32,
    pub clock_format: &'a str,
}

const FIELD_W: f32 = 300.0;
const FIELD_H: f32 = 44.0;

/// Draw the face over whatever is already there; what was clicked comes back.
pub fn draw(ui: &mut egui::Ui, v: &View) -> Vec<LockAct> {
    let mut acts = Vec::new();
    let full = ui.max_rect();
    let p = ui.painter().clone();
    let white = Color32::WHITE;
    let soft = Color32::from_white_alpha(190);
    // A veil over the wallpaper, darker at the bottom where the field is.
    p.rect_filled(full, 0.0, Color32::from_black_alpha(70));
    let steps = 24;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let band = Rect::from_min_max(Pos2::new(full.left(), full.top() + full.height() * (0.45 + 0.55 * t)), Pos2::new(full.right(), full.top() + full.height() * (0.45 + 0.55 * (t + 1.0 / steps as f32))));
        p.rect_filled(band, 0.0, Color32::from_black_alpha((t * 70.0) as u8));
    }

    // The time and date, a third of the way down.
    let now = Local::now();
    let small = full.height() < 500.0;
    let cy = full.top() + full.height() * if small { 0.24 } else { 0.30 };
    let time = now.format(time_part(v.clock_format)).to_string();
    p.text(Pos2::new(full.center().x, cy), Align2::CENTER_CENTER, time, ui_font(if small { 64.0 } else { 104.0 }), white);
    p.text(Pos2::new(full.center().x, cy + if small { 52.0 } else { 78.0 }), Align2::CENTER_CENTER, now.format("%A, %B %-d").to_string(), ui_font(if small { 16.0 } else { 20.0 }), soft);

    // Who, then the field.
    let fy = full.top() + full.height() * if small { 0.66 } else { 0.64 };
    let ac = Pos2::new(full.center().x, fy - 104.0);
    let ar = 36.0;
    match v.avatar {
        Some(t) => {
            // Round: drawn as a mesh of the circle's slices.
            let mut mesh = egui::Mesh::with_texture(t.id());
            let n = 64;
            mesh.vertices.push(egui::epaint::Vertex { pos: ac, uv: Pos2::new(0.5, 0.5), color: white });
            for i in 0..=n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let (s, c) = a.sin_cos();
                mesh.vertices.push(egui::epaint::Vertex { pos: ac + Vec2::new(c, s) * ar, uv: Pos2::new(0.5 + c * 0.5, 0.5 + s * 0.5), color: white });
            }
            for i in 0..n as u32 {
                mesh.add_triangle(0, i + 1, i + 2);
            }
            p.add(egui::Shape::mesh(mesh));
        }
        None => {
            p.circle_filled(ac, ar, mix(v.accent, Color32::BLACK, 0.25));
            let initial = v.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            p.text(ac, Align2::CENTER_CENTER, initial, ui_font(30.0), white);
        }
    }
    p.circle_stroke(ac, ar + 1.0, Stroke::new(2.0, Color32::from_white_alpha(60)));
    p.text(Pos2::new(full.center().x, ac.y + ar + 20.0), Align2::CENTER_CENTER, v.name, ui_font(17.0), white);

    // A wrong password shakes the field side to side for a moment.
    let shake = v.since_error.map(|d| d.as_secs_f32()).filter(|t| *t < 0.45).map(|t| (t * 40.0).sin() * 10.0 * (1.0 - t / 0.45)).unwrap_or(0.0);
    let field = Rect::from_center_size(Pos2::new(full.center().x + shake, fy), Vec2::new(FIELD_W, FIELD_H));
    let error = v.since_error.is_some_and(|d| d < Duration::from_secs(2));
    p.rect_filled(field, FIELD_H / 2.0, Color32::from_white_alpha(28));
    let edge = if error { Color32::from_rgb(240, 110, 110) } else if v.busy { Color32::from_white_alpha(70) } else { v.accent };
    p.rect_stroke(field, FIELD_H / 2.0, Stroke::new(if error { 2.0 } else { 1.5 }, edge), egui::StrokeKind::Inside);
    if v.busy {
        // Three dots taking turns while the password is checked.
        let t = ui.input(|i| i.time) as f32;
        for i in 0..3 {
            let a = 0.35 + 0.65 * (((t * 4.0 - i as f32 * 0.6).sin() + 1.0) / 2.0);
            p.circle_filled(Pos2::new(field.center().x + (i as f32 - 1.0) * 14.0, field.center().y), 4.0, Color32::from_white_alpha((a * 255.0) as u8));
        }
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    } else if v.typed == 0 {
        p.text(field.center(), Align2::CENTER_CENTER, "Password", ui_font(14.5), Color32::from_white_alpha(140));
    } else {
        // One dot per character, as many as fit.
        let room = ((field.width() - 40.0) / 14.0) as usize;
        let n = v.typed.min(room);
        let x0 = field.center().x - (n as f32 - 1.0) * 7.0;
        for i in 0..n {
            p.circle_filled(Pos2::new(x0 + i as f32 * 14.0, field.center().y), 4.5, white);
        }
    }
    // Under it: what the check said, or Caps Lock.
    let mut y = field.bottom() + 22.0;
    if let Some((m, bad)) = v.message {
        let col = if bad { Color32::from_rgb(255, 150, 150) } else { soft };
        let g = one_line(&p, m, ui_font(13.5), col, FIELD_W + 160.0);
        p.galley(Pos2::new(full.center().x - g.size().x / 2.0, y - g.size().y / 2.0), g, col);
        y += 22.0;
    }
    if v.caps_lock {
        p.text(Pos2::new(full.center().x, y), Align2::CENTER_CENTER, "Caps Lock is on", ui_font(13.0), Color32::from_rgb(255, 210, 120));
    }

    // Power, at the bottom right: a second click within a few seconds does it.
    if v.power {
        let size = 44.0;
        let gap = 12.0;
        let all = [LockAct::Suspend, LockAct::Restart, LockAct::ShutDown];
        let mut x = full.right() - 28.0 - size / 2.0;
        for a in all.iter().rev() {
            let c = Pos2::new(x, full.bottom() - 28.0 - size / 2.0);
            let r = Rect::from_center_size(c, Vec2::splat(size));
            let resp = ui.interact(r, egui::Id::unique(("lock-power", *a as u8)), Sense::click()).role(Role::Button, a.label());
            let asking = v.confirm == Some(*a);
            let fill = if asking { mix(v.accent, Color32::BLACK, 0.1) } else if resp.hovered() { Color32::from_white_alpha(50) } else { Color32::from_white_alpha(24) };
            p.circle_filled(c, size / 2.0, fill);
            match a {
                LockAct::Suspend => glyph::moon(&p, c, white, false),
                LockAct::Restart => glyph::refresh(&p, c, white),
                LockAct::ShutDown => glyph::power(&p, c, white),
            }
            if asking {
                let label = format!("{}? Click again", a.label());
                p.text(Pos2::new(full.right() - 28.0, c.y - size / 2.0 - 16.0), Align2::RIGHT_CENTER, label, ui_font(13.0), white);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(a.label()).clicked() {
                acts.push(*a);
            }
            x -= size + gap;
        }
    }
    acts
}

/// The time from the panel's clock format (the date is shown on its own line): the part with
/// the hour and minutes, else 24-hour time.
fn time_part(fmt: &str) -> &'static str {
    if fmt.contains("%I") || fmt.contains("%l") || fmt.contains("%p") || fmt.contains("%r") { "%-I:%M" } else { "%H:%M" }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    rivet_look::palette::mix(a, b, t)
}

/// Text on one line, cut with an ellipsis where it doesn't fit.
fn one_line(p: &egui::Painter, text: &str, font: FontId, color: Color32, max_w: f32) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.replace('\n', " "), egui::TextFormat { font_id: font, color, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: max_w.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    p.layout_job(job)
}

/// The power buttons' symbols, drawn with lines (Rivet Shell draws the same ones elsewhere).
pub mod glyph {
    use eframe::egui::{self, Color32, Pos2, Shape, Stroke, Vec2};

    fn arc(p: &egui::Painter, c: Pos2, r: f32, from: f32, to: f32, s: Stroke) {
        let n = 14;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = from + (to - from) * i as f32 / n as f32;
                c + Vec2::new(a.cos(), a.sin()) * r
            })
            .collect();
        p.add(Shape::line(pts, s));
    }

    pub fn power(p: &egui::Painter, c: Pos2, col: Color32) {
        let s = Stroke::new(1.7, col);
        use std::f32::consts::PI;
        arc(p, c + Vec2::new(0.0, 1.0), 6.5, -PI / 2.0 + 0.7, 3.0 * PI / 2.0 - 0.7, s);
        p.line_segment([c + Vec2::new(0.0, -7.0), c + Vec2::new(0.0, 0.5)], s);
    }

    pub fn moon(p: &egui::Painter, c: Pos2, col: Color32, _on: bool) {
        use std::f32::consts::PI;
        let s = Stroke::new(1.7, col);
        arc(p, c, 7.0, PI * 0.15, PI * 1.85, s);
        arc(p, c + Vec2::new(3.6, -2.4), 5.2, PI * 0.42, PI * 1.25, s);
    }

    pub fn refresh(p: &egui::Painter, c: Pos2, col: Color32) {
        use std::f32::consts::PI;
        arc(p, c, 6.0, -PI * 0.3, PI * 1.45, Stroke::new(1.6, col));
        let tip = c + Vec2::new((-PI * 0.3).cos(), (-PI * 0.3).sin()) * 6.0;
        p.add(Shape::convex_polygon(vec![tip + Vec2::new(-3.6, -2.4), tip + Vec2::new(1.8, -3.6), tip + Vec2::new(0.6, 2.2)], col, Stroke::NONE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_clocks_hours() {
        assert_eq!(time_part("%a %b %-d  %H:%M"), "%H:%M");
        assert_eq!(time_part("%-I:%M %p"), "%-I:%M");
    }
}

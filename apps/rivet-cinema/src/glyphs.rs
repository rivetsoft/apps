//! The player's own symbols (play, pause, volume…), drawn as shapes so they are sharp at any
//! size and take the theme's colors.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use rivet_look::a11y::{Access, Role};
use rivet_look::palette;
use rivet_look::theme::r_sm;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum G {
    Play,
    Pause,
    Previous,
    Next,
    /// Loudness 0 (no waves), 1 or 2.
    Volume(u8),
    Muted,
    Fullscreen,
    Windowed,
    Queue,
    /// Keep on top: a small window over a big one.
    OnTop,
    Repeat,
    RepeatOne,
    Shuffle,
    Music,
    Close,
}

/// Draw a symbol filling `r` (kept square, centered).
pub fn draw(p: &Painter, r: Rect, g: G, c: Color32) {
    let s = r.width().min(r.height());
    let o = r.center();
    let at = |x: f32, y: f32| o + Vec2::new(x * s / 2.0, y * s / 2.0);
    let line = Stroke::new((s / 11.0).max(1.4), c);
    let fill = |pts: Vec<Pos2>| Shape::convex_polygon(pts, c, Stroke::NONE);
    let bar = |a: Pos2, b: Pos2| Shape::rect_filled(Rect::from_two_pos(a, b), s * 0.06, c);
    let arrow_head = |tip: Pos2, dx: f32, dy: f32| Shape::line(vec![tip + Vec2::new(-dx - dy, dx - dy) * s * 0.2, tip, tip + Vec2::new(-dx + dy, -dx - dy) * s * 0.2], line);
    match g {
        G::Play => {
            p.add(fill(vec![at(-0.55, -0.78), at(0.82, 0.0), at(-0.55, 0.78)]));
        }
        G::Pause => {
            p.add(bar(at(-0.62, -0.72), at(-0.16, 0.72)));
            p.add(bar(at(0.16, -0.72), at(0.62, 0.72)));
        }
        G::Previous => {
            p.add(bar(at(-0.78, -0.66), at(-0.5, 0.66)));
            p.add(fill(vec![at(0.75, -0.66), at(0.75, 0.66), at(-0.42, 0.0)]));
        }
        G::Next => {
            p.add(bar(at(0.5, -0.66), at(0.78, 0.66)));
            p.add(fill(vec![at(-0.75, -0.66), at(0.42, 0.0), at(-0.75, 0.66)]));
        }
        G::Volume(_) | G::Muted => {
            p.add(bar(at(-0.82, -0.3), at(-0.4, 0.3)));
            p.add(fill(vec![at(-0.5, -0.3), at(0.0, -0.75), at(0.0, 0.75), at(-0.5, 0.3)]));
            match g {
                G::Muted => {
                    p.line_segment([at(0.3, -0.3), at(0.85, 0.3)], line);
                    p.line_segment([at(0.3, 0.3), at(0.85, -0.3)], line);
                }
                G::Volume(level) => {
                    for (i, radius) in [0.38f32, 0.72].into_iter().enumerate() {
                        if usize::from(level) > i {
                            let arc: Vec<Pos2> = (-4..=4).map(|k| k as f32 * 0.16).map(|a: f32| at(radius * a.cos(), radius * a.sin())).collect();
                            p.add(Shape::line(arc, line));
                        }
                    }
                }
                _ => {}
            }
        }
        G::Fullscreen | G::Windowed => {
            let out = g == G::Fullscreen;
            for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let pts = if out { vec![at(sx * 0.78, sy * 0.28), at(sx * 0.78, sy * 0.78), at(sx * 0.28, sy * 0.78)] } else { vec![at(sx * 0.28, sy * 0.78), at(sx * 0.28, sy * 0.28), at(sx * 0.78, sy * 0.28)] };
                p.add(Shape::line(pts, line));
            }
        }
        G::Queue => {
            for (y, end) in [(-0.55f32, 0.78f32), (0.0, 0.78), (0.55, 0.2)] {
                p.line_segment([at(-0.78, y), at(end, y)], line);
            }
            p.add(fill(vec![at(0.42, 0.3), at(0.85, 0.55), at(0.42, 0.8)]));
        }
        G::OnTop => {
            p.rect_stroke(Rect::from_two_pos(at(-0.82, -0.62), at(0.82, 0.62)), s * 0.08, line, egui::StrokeKind::Middle);
            p.add(bar(at(0.0, 0.0), at(0.58, 0.4)));
        }
        G::Repeat | G::RepeatOne => {
            p.add(Shape::line(vec![at(-0.75, 0.15), at(-0.75, -0.42), at(0.62, -0.42)], line));
            p.add(arrow_head(at(0.66, -0.42), 1.0, 0.0));
            p.add(Shape::line(vec![at(0.75, -0.15), at(0.75, 0.42), at(-0.62, 0.42)], line));
            p.add(arrow_head(at(-0.66, 0.42), -1.0, 0.0));
            if g == G::RepeatOne {
                p.text(at(0.0, 0.0), egui::Align2::CENTER_CENTER, "1", egui::FontId::proportional(s * 0.5), c);
            }
        }
        G::Shuffle => {
            p.add(Shape::line(vec![at(-0.82, -0.45), at(-0.35, -0.45), at(0.3, 0.45), at(0.72, 0.45)], line));
            p.add(Shape::line(vec![at(-0.82, 0.45), at(-0.35, 0.45), at(0.3, -0.45), at(0.72, -0.45)], line));
            p.add(arrow_head(at(0.78, 0.45), 1.0, 0.0));
            p.add(arrow_head(at(0.78, -0.45), 1.0, 0.0));
        }
        G::Music => {
            p.circle_filled(at(-0.3, 0.5), s * 0.2, c);
            p.add(bar(at(-0.12, -0.75), at(0.02, 0.5)));
            p.add(Shape::line(vec![at(-0.05, -0.72), at(0.55, -0.45), at(0.55, -0.15)], line));
        }
        G::Close => {
            p.line_segment([at(-0.55, -0.55), at(0.55, 0.55)], line);
            p.line_segment([at(-0.55, 0.55), at(0.55, -0.55)], line);
        }
    }
}

/// A square button showing a symbol; `on` draws it in the accent (a mode that is on).
pub fn button(ui: &mut egui::Ui, g: G, size: f32, tip: &str, on: bool, accent: Color32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let v = ui.visuals().clone();
    if resp.hovered() {
        ui.painter().rect_filled(r, r_sm(), palette::mix(v.panel_fill, v.text_color(), 0.1));
    }
    draw(ui.painter(), r.shrink(size * 0.27), g, if on { accent } else { v.text_color() });
    let name = tip.split(" (").next().and_then(|t| t.split(':').next()).unwrap_or(tip);
    let resp = resp.role(Role::Button, name).described(tip);
    let resp = if matches!(g, G::Queue | G::OnTop | G::Shuffle | G::Repeat | G::RepeatOne) { resp.toggled(on) } else { resp };
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip)
}

/// The big round play or pause button.
pub fn big_button(ui: &mut egui::Ui, playing: bool, size: f32, accent: Color32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let fill = if resp.hovered() { palette::mix(accent, Color32::WHITE, 0.12) } else { accent };
    ui.painter().circle_filled(r.center(), size / 2.0, fill);
    let g = if playing { G::Pause } else { G::Play };
    let inner = Rect::from_center_size(r.center() + Vec2::new(if playing { 0.0 } else { size * 0.03 }, 0.0), Vec2::splat(size * 0.4));
    draw(ui.painter(), inner, g, rivet_look::theme::StatusColors::on(accent));
    let resp = resp.role(Role::Button, if playing { "Pause" } else { "Play" }).described("Space");
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if playing { "Pause (Space)" } else { "Play (Space)" })
}

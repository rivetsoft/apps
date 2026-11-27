//! The shell's small drawn symbols (so they look alike on every icon theme).

use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, Vec2};

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

pub fn grid(p: &egui::Painter, c: Pos2, col: Color32) {
    for i in -1..=1 {
        for j in -1..=1 {
            p.circle_filled(c + Vec2::new(i as f32 * 5.0, j as f32 * 5.0), 1.7, col);
        }
    }
}

pub fn power(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.7, col);
    use std::f32::consts::PI;
    arc(p, c + Vec2::new(0.0, 1.0), 6.5, -PI / 2.0 + 0.7, 3.0 * PI / 2.0 - 0.7, s);
    p.line_segment([c + Vec2::new(0.0, -7.0), c + Vec2::new(0.0, 0.5)], s);
}

pub fn gear(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.7, col);
    p.circle_stroke(c, 5.2, s);
    p.circle_filled(c, 1.8, col);
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        let d = Vec2::new(a.cos(), a.sin());
        p.line_segment([c + d * 5.2, c + d * 7.6], Stroke::new(2.2, col));
    }
}

/// The quick settings item's symbol, as chosen.
pub fn quick(p: &egui::Painter, c: Pos2, col: Color32, icon: crate::config::QuickIcon) {
    use crate::config::QuickIcon;
    match icon {
        QuickIcon::Sliders => sliders(p, c, col),
        QuickIcon::Gear => gear(p, c, col),
        // Two switches: the upper one on (filled, knob to the right), the lower one off.
        QuickIcon::Toggles => {
            let s = Stroke::new(1.5, col);
            let size = Vec2::new(15.0, 7.0);
            let on = Rect::from_center_size(c + Vec2::new(0.0, -4.5), size);
            let off = Rect::from_center_size(c + Vec2::new(0.0, 4.5), size);
            p.rect_filled(on, 3.5, col);
            p.circle_filled(on.right_center() - Vec2::new(3.5, 0.0), 2.0, Color32::from_black_alpha(150));
            p.rect_stroke(off.shrink(0.75), 2.75, s, egui::StrokeKind::Middle);
            p.circle_filled(off.left_center() + Vec2::new(3.5, 0.0), 1.5, col);
        }
        // Four tiles, the first lit.
        QuickIcon::Tiles => {
            let s = Stroke::new(1.5, col);
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let r = Rect::from_center_size(c + Vec2::new(dx * 4.5, dy * 4.5), Vec2::splat(7.0));
                if dx < 0.0 && dy < 0.0 {
                    p.rect_filled(r, 1.5, col);
                } else {
                    p.rect_stroke(r.shrink(0.75), 0.75, s, egui::StrokeKind::Middle);
                }
            }
        }
    }
}

/// Three sliders: quick settings.
pub fn sliders(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    for (dy, knob) in [(-5.0, 3.0), (0.0, -3.5), (5.0, 1.0)] {
        p.line_segment([c + Vec2::new(-7.5, dy), c + Vec2::new(7.5, dy)], s);
        p.circle_filled(c + Vec2::new(knob, dy), 2.4, col);
    }
}

pub fn bell(p: &egui::Painter, c: Pos2, col: Color32, off: bool) {
    let s = Stroke::new(1.6, col);
    use std::f32::consts::PI;
    arc(p, c + Vec2::new(0.0, -1.5), 4.5, PI, 2.0 * PI, s);
    p.line_segment([c + Vec2::new(-4.5, -1.5), c + Vec2::new(-5.5, 4.0)], s);
    p.line_segment([c + Vec2::new(4.5, -1.5), c + Vec2::new(5.5, 4.0)], s);
    p.line_segment([c + Vec2::new(-7.0, 4.0), c + Vec2::new(7.0, 4.0)], s);
    p.circle_filled(c + Vec2::new(0.0, 6.5), 1.5, col);
    p.circle_filled(c + Vec2::new(0.0, -7.0), 1.1, col);
    if off {
        p.line_segment([c + Vec2::new(-7.0, -7.0), c + Vec2::new(7.0, 7.0)], Stroke::new(1.8, col));
    }
}

pub fn sun(p: &egui::Painter, c: Pos2, col: Color32) {
    p.circle_stroke(c, 3.6, Stroke::new(1.6, col));
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        let d = Vec2::new(a.cos(), a.sin());
        p.line_segment([c + d * 5.8, c + d * 7.8], Stroke::new(1.5, col));
    }
}

pub fn moon(p: &egui::Painter, c: Pos2, col: Color32, _on: bool) {
    use std::f32::consts::PI;
    let s = Stroke::new(1.7, col);
    arc(p, c, 7.0, PI * 0.15, PI * 1.85, s);
    arc(p, c + Vec2::new(3.6, -2.4), 5.2, PI * 0.42, PI * 1.25, s);
}

/// A push pin, upright; filled when pinned.
pub fn pin(p: &egui::Painter, c: Pos2, col: Color32, filled: bool) {
    let s = Stroke::new(1.4, col);
    let body = vec![c + Vec2::new(-2.5, -7.0), c + Vec2::new(2.5, -7.0), c + Vec2::new(2.5, -2.5), c + Vec2::new(5.0, 1.0), c + Vec2::new(-5.0, 1.0), c + Vec2::new(-2.5, -2.5)];
    if filled {
        p.add(Shape::convex_polygon(body, col, Stroke::NONE));
    } else {
        p.add(Shape::closed_line(body, s));
    }
    p.line_segment([c + Vec2::new(0.0, 1.0), c + Vec2::new(0.0, 7.5)], s);
}

/// A four-pointed sparkle and a small one: AI. Curved sides, drawn as an outline.
pub fn spark(p: &egui::Painter, c: Pos2, col: Color32) {
    let star = |c: Pos2, r: f32| -> Vec<Pos2> {
        // Each side bows in towards the middle (a quadratic curve through a point near it).
        let tips = [Vec2::new(0.0, -r), Vec2::new(r, 0.0), Vec2::new(0.0, r), Vec2::new(-r, 0.0)];
        let mut pts = Vec::new();
        for i in 0..4 {
            let (a, b) = (tips[i], tips[(i + 1) % 4]);
            let k = (a + b) * 0.16;
            for s in 0..6 {
                let t = s as f32 / 6.0;
                pts.push(c + a * (1.0 - t) * (1.0 - t) + k * 2.0 * t * (1.0 - t) + b * t * t);
            }
        }
        pts
    };
    p.add(Shape::closed_line(star(c + Vec2::new(-1.5, 1.5), 7.0), Stroke::new(1.5, col)));
    p.add(Shape::convex_polygon(star(c + Vec2::new(5.5, -5.5), 2.8).into_iter().step_by(6).collect(), col, Stroke::NONE));
}

pub fn bluetooth(p: &egui::Painter, c: Pos2, col: Color32, _on: bool) {
    let pts = vec![c + Vec2::new(-4.5, -4.0), c + Vec2::new(4.0, 3.5), c + Vec2::new(0.0, 7.5), c + Vec2::new(0.0, -7.5), c + Vec2::new(4.0, -3.5), c + Vec2::new(-4.5, 4.0)];
    p.add(Shape::line(pts, Stroke::new(1.6, col)));
}

pub fn cup(p: &egui::Painter, c: Pos2, col: Color32, _on: bool) {
    let s = Stroke::new(1.6, col);
    let body = vec![c + Vec2::new(-6.0, -2.0), c + Vec2::new(-5.0, 5.5), c + Vec2::new(3.0, 5.5), c + Vec2::new(4.0, -2.0), c + Vec2::new(-6.0, -2.0)];
    p.add(Shape::line(body, s));
    use std::f32::consts::PI;
    arc(p, c + Vec2::new(4.5, 1.2), 2.6, -PI / 2.0, PI / 2.0, s);
    for dx in [-3.0, 1.0] {
        p.line_segment([c + Vec2::new(dx, -7.5), c + Vec2::new(dx, -4.5)], Stroke::new(1.4, col));
    }
}

/// A window over another (floating windows): the front one filled when it is on.
pub fn floating(p: &egui::Painter, c: Pos2, col: Color32, on: bool) {
    let back = egui::Rect::from_min_size(c + Vec2::new(-7.0, -7.0), Vec2::new(9.5, 8.0));
    let front = egui::Rect::from_min_size(c + Vec2::new(-2.5, -1.0), Vec2::new(9.5, 8.0));
    p.rect_stroke(back, 1.5, Stroke::new(1.5, col), egui::StrokeKind::Middle);
    p.rect_filled(front.expand(1.5), 2.5, if on { col } else { Color32::TRANSPARENT });
    p.rect_stroke(front, 1.5, Stroke::new(1.5, col), egui::StrokeKind::Middle);
}

pub fn bolt(p: &egui::Painter, c: Pos2, col: Color32, _on: bool) {
    let pts = vec![c + Vec2::new(1.5, -8.0), c + Vec2::new(-5.0, 1.0), c + Vec2::new(-0.5, 1.0), c + Vec2::new(-1.5, 8.0), c + Vec2::new(5.0, -1.0), c + Vec2::new(0.5, -1.0), c + Vec2::new(1.5, -8.0)];
    p.add(Shape::line(pts, Stroke::new(1.5, col)));
}

/// A shield (a VPN): filled when it is on.
pub fn vpn(p: &egui::Painter, c: Pos2, col: Color32, on: bool) {
    let pts = vec![c + Vec2::new(0.0, -7.5), c + Vec2::new(6.0, -5.0), c + Vec2::new(6.0, 0.5), c + Vec2::new(3.5, 5.0), c + Vec2::new(0.0, 7.5), c + Vec2::new(-3.5, 5.0), c + Vec2::new(-6.0, 0.5), c + Vec2::new(-6.0, -5.0)];
    if on {
        p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    } else {
        p.add(Shape::closed_line(pts, Stroke::new(1.5, col)));
    }
}

pub fn mic(p: &egui::Painter, c: Pos2, col: Color32, muted: bool) {
    let s = Stroke::new(1.6, col);
    p.rect_stroke(Rect::from_center_size(c + Vec2::new(0.0, -2.5), Vec2::new(5.5, 10.0)), 2.75, s, egui::StrokeKind::Middle);
    use std::f32::consts::PI;
    arc(p, c + Vec2::new(0.0, -1.0), 6.0, 0.0, PI, s);
    p.line_segment([c + Vec2::new(0.0, 5.0), c + Vec2::new(0.0, 8.0)], s);
    if muted {
        p.line_segment([c + Vec2::new(-7.0, -7.0), c + Vec2::new(7.0, 7.0)], Stroke::new(1.8, col));
    }
}

pub fn contrast(p: &egui::Painter, c: Pos2, col: Color32) {
    use std::f32::consts::PI;
    p.circle_stroke(c, 7.0, Stroke::new(1.6, col));
    let pts: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = -PI / 2.0 + PI * i as f32 / 12.0;
            c + Vec2::new(a.cos(), a.sin()) * 7.0
        })
        .collect();
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
}

/// A play triangle, or pause bars.
pub fn play(p: &egui::Painter, c: Pos2, play: bool, col: Color32) {
    if play {
        p.add(Shape::convex_polygon(vec![c + Vec2::new(-3.5, -5.5), c + Vec2::new(5.5, 0.0), c + Vec2::new(-3.5, 5.5)], col, Stroke::NONE));
    } else {
        for dx in [-2.8, 2.8] {
            p.rect_filled(Rect::from_center_size(c + Vec2::new(dx, 0.0), Vec2::new(2.8, 10.0)), 1.0, col);
        }
    }
}

pub fn skip(p: &egui::Painter, c: Pos2, forward: bool, col: Color32) {
    let d = if forward { 1.0 } else { -1.0 };
    p.add(Shape::convex_polygon(vec![c + Vec2::new(-4.0 * d, -5.0), c + Vec2::new(3.0 * d, 0.0), c + Vec2::new(-4.0 * d, 5.0)], col, Stroke::NONE));
    p.rect_filled(Rect::from_center_size(c + Vec2::new(4.5 * d, 0.0), Vec2::new(2.0, 10.0)), 0.5, col);
}

pub fn x(p: &egui::Painter, c: Pos2, r: f32, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.line_segment([c + Vec2::new(-r, -r), c + Vec2::new(r, r)], s);
    p.line_segment([c + Vec2::new(r, -r), c + Vec2::new(-r, r)], s);
}

pub fn check(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.8, col);
    p.line_segment([c + Vec2::new(-4.5, 0.0), c + Vec2::new(-1.5, 3.0)], s);
    p.line_segment([c + Vec2::new(-1.5, 3.0), c + Vec2::new(4.5, -3.5)], s);
}

/// A battery, 20 wide with its tip, centred on `c`.
pub fn battery(p: &egui::Painter, c: Pos2, pct: u32, charging: bool, col: Color32, accent: Color32) {
    let body = Rect::from_center_size(c - Vec2::new(1.0, 0.0), Vec2::new(17.0, 10.0));
    p.rect_stroke(body, 2.5, Stroke::new(1.5, col), egui::StrokeKind::Inside);
    p.rect_filled(Rect::from_min_size(Pos2::new(body.right() + 1.0, c.y - 2.5), Vec2::new(2.0, 5.0)), 1.0, col);
    let inner = body.shrink(2.5);
    let w = inner.width() * (pct as f32 / 100.0).clamp(0.05, 1.0);
    p.rect_filled(Rect::from_min_size(inner.min, Vec2::new(w, inner.height())), 1.0, if charging { accent } else { col });
}

pub fn wifi(p: &egui::Painter, c: Pos2, col: Color32) {
    use std::f32::consts::PI;
    let base = c + Vec2::new(0.0, 5.0);
    for r in [5.0, 9.0] {
        arc(p, base, r, -PI * 0.78, -PI * 0.22, Stroke::new(1.7, col));
    }
    p.circle_filled(base + Vec2::new(0.0, -1.0), 1.7, col);
}

/// Wi-Fi with its strength (0–100): the arcs it doesn't reach are faint.
pub fn wifi_signal(p: &egui::Painter, c: Pos2, col: Color32, signal: u8) {
    use std::f32::consts::PI;
    let base = c + Vec2::new(0.0, 5.0);
    let faint = col.gamma_multiply(0.28);
    for (r, need) in [(5.0, 35), (9.0, 65)] {
        arc(p, base, r, -PI * 0.78, -PI * 0.22, Stroke::new(1.7, if signal >= need { col } else { faint }));
    }
    p.circle_filled(base + Vec2::new(0.0, -1.0), 1.7, col);
}

/// A small padlock (a secured network).
pub fn lock(p: &egui::Painter, c: Pos2, col: Color32) {
    use std::f32::consts::PI;
    p.rect_filled(Rect::from_center_size(c + Vec2::new(0.0, 1.5), Vec2::new(7.0, 5.0)), 1.0, col);
    arc(p, c + Vec2::new(0.0, -1.0), 2.3, PI, 2.0 * PI, Stroke::new(1.2, col));
    for dx in [-2.3, 2.3] {
        p.line_segment([c + Vec2::new(dx, -1.0), c + Vec2::new(dx, 0.0)], Stroke::new(1.2, col));
    }
}

/// Look again (a circling arrow).
pub fn refresh(p: &egui::Painter, c: Pos2, col: Color32) {
    use std::f32::consts::PI;
    arc(p, c, 6.0, -PI * 0.3, PI * 1.45, Stroke::new(1.6, col));
    let tip = c + Vec2::new((-PI * 0.3).cos(), (-PI * 0.3).sin()) * 6.0;
    p.add(Shape::convex_polygon(vec![tip + Vec2::new(-3.6, -2.4), tip + Vec2::new(1.8, -3.6), tip + Vec2::new(0.6, 2.2)], col, Stroke::NONE));
}

pub fn wired(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    p.rect_stroke(Rect::from_center_size(c + Vec2::new(0.0, 2.0), Vec2::new(14.0, 8.0)), 1.5, s, egui::StrokeKind::Inside);
    p.line_segment([c + Vec2::new(0.0, -2.0), c + Vec2::new(0.0, -7.0)], s);
    for dx in [-3.5, 0.0, 3.5] {
        p.line_segment([c + Vec2::new(dx, 1.0), c + Vec2::new(dx, 3.5)], Stroke::new(1.2, col));
    }
}

pub fn speaker(p: &egui::Painter, c: Pos2, muted: bool, col: Color32) {
    let pts = vec![c + Vec2::new(-7.0, -2.5), c + Vec2::new(-4.0, -2.5), c + Vec2::new(0.0, -6.0), c + Vec2::new(0.0, 6.0), c + Vec2::new(-4.0, 2.5), c + Vec2::new(-7.0, 2.5)];
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    let s = Stroke::new(1.5, col);
    if muted {
        p.line_segment([c + Vec2::new(3.0, -3.0), c + Vec2::new(8.0, 3.0)], s);
        p.line_segment([c + Vec2::new(8.0, -3.0), c + Vec2::new(3.0, 3.0)], s);
    } else {
        use std::f32::consts::PI;
        arc(p, c, 4.0, -PI / 3.5, PI / 3.5, s);
        arc(p, c, 7.5, -PI / 3.5, PI / 3.5, s);
    }
}

pub fn clock(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    p.circle_stroke(c, 7.0, s);
    p.line_segment([c, c + Vec2::new(0.0, -4.5)], s);
    p.line_segment([c, c + Vec2::new(3.5, 1.5)], s);
}

/// A chip: processor and memory.
pub fn chip(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.rect_stroke(Rect::from_center_size(c, Vec2::splat(10.0)), 2.0, s, egui::StrokeKind::Middle);
    p.rect_filled(Rect::from_center_size(c, Vec2::splat(4.0)), 1.0, col);
    for d in [-3.0, 0.0, 3.0] {
        for (a, b) in [(Vec2::new(d, -5.0), Vec2::new(d, -7.5)), (Vec2::new(d, 5.0), Vec2::new(d, 7.5)), (Vec2::new(-5.0, d), Vec2::new(-7.5, d)), (Vec2::new(5.0, d), Vec2::new(7.5, d))] {
            p.line_segment([c + a, c + b], Stroke::new(1.2, col));
        }
    }
}

/// Lines of text: a window's title.
pub fn title(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    p.rect_stroke(Rect::from_center_size(c, Vec2::new(16.0, 12.0)), 2.0, s, egui::StrokeKind::Middle);
    p.line_segment([c + Vec2::new(-8.0, -2.5), c + Vec2::new(8.0, -2.5)], s);
    p.line_segment([c + Vec2::new(-4.5, 2.0), c + Vec2::new(4.5, 2.0)], Stroke::new(1.3, col));
}

/// Three pills: workspaces.
pub fn pills(p: &egui::Painter, c: Pos2, col: Color32, accent: Color32) {
    p.rect_filled(Rect::from_center_size(c + Vec2::new(-5.0, 0.0), Vec2::new(8.0, 6.0)), 3.0, accent);
    p.circle_filled(c + Vec2::new(3.0, 0.0), 2.6, col);
    p.circle_filled(c + Vec2::new(8.5, 0.0), 2.6, col);
}

/// The system tray: an up chevron over a row of three small app squares.
pub fn tray(p: &egui::Painter, c: Pos2, col: Color32, accent: Color32) {
    let s = Stroke::new(1.6, col);
    p.line_segment([c + Vec2::new(-4.0, -3.0), c + Vec2::new(0.0, -7.0)], s);
    p.line_segment([c + Vec2::new(0.0, -7.0), c + Vec2::new(4.0, -3.0)], s);
    for (i, dx) in [-6.0f32, 0.0, 6.0].into_iter().enumerate() {
        p.rect_filled(Rect::from_center_size(c + Vec2::new(dx, 4.0), Vec2::splat(4.5)), 1.2, if i == 1 { accent } else { col });
    }
}

/// A dock: three app squares on a shelf.
pub fn dock(p: &egui::Painter, c: Pos2, col: Color32, accent: Color32) {
    for (i, dx) in [-6.0f32, 0.0, 6.0].into_iter().enumerate() {
        p.rect_filled(Rect::from_center_size(c + Vec2::new(dx, -1.5), Vec2::splat(4.5)), 1.2, if i == 0 { accent } else { col });
    }
    p.line_segment([c + Vec2::new(-9.0, 3.5), c + Vec2::new(9.0, 3.5)], Stroke::new(1.6, col));
}

/// A puzzle piece of sorts: a plugin.
pub fn plugin(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.rect_stroke(Rect::from_center_size(c + Vec2::new(-1.0, 1.0), Vec2::splat(11.0)), 2.0, s, egui::StrokeKind::Middle);
    p.circle_stroke(c + Vec2::new(4.5, -4.5), 2.5, s);
}

/// A panel item's symbol, by id.
pub fn item(p: &egui::Painter, id: &str, c: Pos2, col: Color32, accent: Color32, quick_icon: crate::config::QuickIcon) {
    match id {
        "launcher" => grid(p, c, col),
        "workspaces" => pills(p, c, col, accent),
        "window-title" => title(p, c, col),
        "clock" => clock(p, c, col),
        "media" => play(p, c, true, col),
        "system" => chip(p, c, col),
        "volume" => speaker(p, c + Vec2::new(1.5, 0.0), false, col),
        "brightness" => sun(p, c, col),
        "network" => wifi(p, c, col),
        "bluetooth" => bluetooth(p, c, col, true),
        "ai" => spark(p, c, col),
        "weather" => crate::weather_ui::icon(p, c, 18.0, crate::weather::Sky::PartlyCloudy, true, col, accent),
        "battery" => battery(p, c, 70, false, col, accent),
        "notifications" => bell(p, c, col, false),
        "quick-settings" => quick(p, c, col, quick_icon),
        "power" => power(p, c, col),
        "tray" => tray(p, c, col, accent),
        "dock" => dock(p, c, col, accent),
        s if s.starts_with("spacer") => {
            // Room between two ends.
            let st = Stroke::new(1.5, col);
            for x in [-6.5, 6.5] {
                p.line_segment([c + Vec2::new(x, -5.0), c + Vec2::new(x, 5.0)], st);
            }
            p.line_segment([c + Vec2::new(-3.5, 0.0), c + Vec2::new(3.5, 0.0)], Stroke::new(1.5, col.gamma_multiply(0.55)));
        }
        _ => plugin(p, c, col),
    }
}

/// A package: a box seen from above a corner, `r` from its middle to a point.
pub fn package(p: &egui::Painter, c: Pos2, r: f32, col: Color32) {
    let s = Stroke::new((r * 0.22).clamp(1.0, 1.5), col);
    let (w, h) = (r * 0.87, r * 0.5);
    let top = c + Vec2::new(0.0, -r);
    let (ur, lr, bot) = (c + Vec2::new(w, -h), c + Vec2::new(w, h), c + Vec2::new(0.0, r));
    let (ll, ul) = (c + Vec2::new(-w, h), c + Vec2::new(-w, -h));
    p.add(Shape::closed_line(vec![top, ur, lr, bot, ll, ul], s));
    // The edges that meet at the near corner.
    p.line_segment([ul, c], s);
    p.line_segment([ur, c], s);
    p.line_segment([c, bot], s);
}

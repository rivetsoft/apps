//! Drawing the marks. The window and the saved picture are drawn by the same code: only the
//! view differs (the window's zoom and place, or one pixel to a point for the file).

use std::collections::HashMap;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2, epaint};

use crate::doc::{Kind, Mark};

/// Where the picture's pixels land: pixel (0, 0) at `origin`, `zoom` points to a pixel.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub origin: Pos2,
    pub zoom: f32,
}

impl View {
    pub fn to_screen(self, p: Pos2) -> Pos2 {
        self.origin + p.to_vec2() * self.zoom
    }

    pub fn to_picture(self, p: Pos2) -> Pos2 {
        ((p - self.origin) / self.zoom).to_pos2()
    }

    pub fn rect(self, r: Rect) -> Rect {
        Rect::from_min_max(self.to_screen(r.min), self.to_screen(r.max))
    }
}

/// The blurred patches as textures, made once for each place a patch has been.
#[derive(Default)]
pub struct Patches {
    made: HashMap<[usize; 4], egui::TextureHandle>,
}

impl Patches {
    /// The squares a patch is made of: bigger for a bigger patch, never small enough to read.
    fn block(area: [usize; 4]) -> usize {
        let short = (area[2] - area[0]).min(area[3] - area[1]);
        (short / 8).clamp(8, 28)
    }

    fn get(&mut self, ctx: &egui::Context, base: &egui::ColorImage, area: [usize; 4]) -> Option<egui::TextureHandle> {
        if area[2] <= area[0] || area[3] <= area[1] {
            return None;
        }
        Some(self.made.entry(area).or_insert_with(|| ctx.load_texture("rivet-snip-patch", rivet_kit::scale::mosaic(base, area, Self::block(area)), egui::TextureOptions::NEAREST)).clone())
    }

    /// Drop the patches no mark uses any more.
    pub fn keep(&mut self, marks: &[Mark], base: &egui::ColorImage) {
        let used: Vec<[usize; 4]> = marks.iter().filter(|m| m.kind == Kind::Blur).map(|m| area_of(m, base)).collect();
        self.made.retain(|k, _| used.contains(k));
    }
}

/// The whole pixels a blurred patch covers.
fn area_of(m: &Mark, base: &egui::ColorImage) -> [usize; 4] {
    let r = Rect::from_two_pos(m.a, m.b);
    let [w, h] = base.size;
    let x = |v: f32| (v.round().max(0.0) as usize).min(w);
    let y = |v: f32| (v.round().max(0.0) as usize).min(h);
    [x(r.min.x), y(r.min.y), x(r.max.x), y(r.max.y)]
}

/// Black or white, whichever reads on `c`.
fn on(c: Color32) -> Color32 {
    if rivet_look::palette::luminance(c) > 0.55 { Color32::from_rgb(20, 20, 24) } else { Color32::WHITE }
}

/// How big a text mark is when drawn, in the picture's pixels.
pub fn text_size(ctx: &egui::Context, m: &Mark) -> Vec2 {
    let text = if m.text.is_empty() { " " } else { m.text.as_str() };
    ctx.fonts_mut(|f| f.layout_no_wrap(text.to_string(), FontId::proportional(m.text_size()), m.color)).size()
}

/// Draw one mark.
pub fn mark(p: &egui::Painter, v: &View, m: &Mark, base: &egui::ColorImage, patches: &mut Patches) {
    let w = (m.width * v.zoom).max(1.0);
    let stroke = Stroke::new(w, m.color);
    let (a, b) = (v.to_screen(m.a), v.to_screen(m.b));
    match m.kind {
        Kind::Line => {
            p.line_segment([a, b], stroke);
        }
        Kind::Arrow => {
            let d = b - a;
            let len = d.length();
            if len < 0.5 {
                return;
            }
            let dir = d / len;
            // The head grows with the line's thickness, but never past half a short arrow.
            let head = (w * 4.0 + 6.0 * v.zoom).min(len * 0.5);
            let side = dir.rot90() * head * 0.5;
            let neck = b - dir * head;
            p.line_segment([a, neck + dir * (head * 0.2)], stroke);
            p.add(epaint::PathShape::convex_polygon(vec![b, neck + side, neck - side], m.color, Stroke::NONE));
        }
        Kind::Rect => {
            p.rect_stroke(Rect::from_two_pos(a, b), (w * 0.75).min(6.0 * v.zoom), stroke, egui::StrokeKind::Middle);
        }
        Kind::Ellipse => {
            let r = Rect::from_two_pos(a, b);
            p.add(epaint::EllipseShape::stroke(r.center(), r.size() / 2.0, stroke));
        }
        Kind::Pen | Kind::Marker => {
            let pts: Vec<Pos2> = m.pts.iter().map(|q| v.to_screen(*q)).collect();
            // A marker is wide and see-through, so what it covers still reads.
            let (w, color) = if m.kind == Kind::Marker { (w * 4.0, m.color.gamma_multiply(0.38)) } else { (w, m.color) };
            if pts.len() == 1 {
                p.circle_filled(pts[0], w / 2.0, color);
            } else {
                p.add(epaint::PathShape::line(pts, Stroke::new(w, color)));
            }
        }
        Kind::Text => {
            p.text(a, Align2::LEFT_TOP, &m.text, FontId::proportional(m.text_size() * v.zoom), m.color);
        }
        Kind::Step => {
            let r = m.step_radius() * v.zoom;
            p.circle_filled(a, r, m.color);
            p.text(a, Align2::CENTER_CENTER, m.n.to_string(), FontId::proportional(m.text_size() * v.zoom), on(m.color));
        }
        Kind::Blur => {
            let area = area_of(m, base);
            if let Some(tex) = patches.get(p.ctx(), base, area) {
                let r = Rect::from_min_max(Pos2::new(area[0] as f32, area[1] as f32), Pos2::new(area[2] as f32, area[3] as f32));
                p.image(tex.id(), v.rect(r), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
        }
    }
}

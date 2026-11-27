//! The picture stage shared by Rivet Long Shot and the light box (Rivet Files' and Rivet FTP's
//! Preview): a picture fitted to its space, zoomed with the wheel, a pinch or a double-click,
//! moved by dragging, and turned or flipped as it is drawn (nothing is written to the file).
//!
//! The app owns a `Stage` per window and calls `show` with the texture to draw each frame.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};

use crate::picture::Orient;

/// The furthest in: 32 screen pixels for each pixel of the picture.
const MAX_ZOOM: f32 = 32.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    /// The whole picture in its space, never enlarged past its own size.
    Fit,
    /// Screen pixels for each pixel of the picture (1.0 is 100%).
    Scale(f32),
}

pub struct Stage {
    pub zoom: Zoom,
    /// Where the picture's middle is, from the middle of its space, in points.
    pan: Vec2,
    /// How it's turned for viewing.
    pub orient: Orient,
    /// The last frame's space and the scale "Fit" came to there, for zooming from the keyboard.
    last: Option<(Rect, f32)>,
}

impl Default for Stage {
    fn default() -> Self {
        Stage { zoom: Zoom::Fit, pan: Vec2::ZERO, orient: Orient::NONE, last: None }
    }
}

/// What to draw: the picture's texture and the picture's own size in pixels (upright, before
/// the stage turns it). The texture may be smaller than that.
pub struct Shown<'a> {
    pub texture: &'a egui::TextureHandle,
    pub size: [u32; 2],
    /// Draw a checkerboard under it, so see-through parts read as see-through.
    pub alpha: bool,
    /// A drawing: fitting may enlarge it past its own size.
    pub vector: bool,
}

impl Stage {
    /// Back to fitting, upright.
    pub fn reset(&mut self) {
        self.zoom = Zoom::Fit;
        self.pan = Vec2::ZERO;
        self.orient = Orient::NONE;
    }

    /// Fit the picture in its space again.
    pub fn fit(&mut self) {
        self.zoom = Zoom::Fit;
        self.pan = Vec2::ZERO;
    }

    /// Screen pixels per picture pixel as last drawn (for "100%" and the like).
    pub fn shown_scale(&self) -> Option<f32> {
        match self.zoom {
            Zoom::Scale(s) => Some(s),
            Zoom::Fit => self.last.map(|(_, f)| f),
        }
    }

    /// Show the picture at its own size, one screen pixel for each of its pixels.
    pub fn actual_size(&mut self) {
        self.zoom = Zoom::Scale(1.0);
        self.pan = Vec2::ZERO;
    }

    /// Zoom in (above 1) or out (below 1) around the middle of the space.
    pub fn zoom_by(&mut self, factor: f32) {
        if let Some((area, _)) = self.last {
            self.zoom_around(factor, area.center(), area);
        }
    }

    pub fn turn_right(&mut self) {
        self.orient = self.orient.turned_right();
        self.fit();
    }

    pub fn turn_left(&mut self) {
        self.orient = self.orient.turned_left();
        self.fit();
    }

    pub fn flip_across(&mut self) {
        self.orient = self.orient.flipped_across();
    }

    pub fn flip_down(&mut self) {
        self.orient = self.orient.flipped_down();
    }

    /// The scale that fits the picture in `area`, as screen pixels per picture pixel, never
    /// past the picture's own size unless it's a drawing.
    fn fit_scale(area: Rect, size: Vec2, ppp: f32, vector: bool) -> f32 {
        let fit = (area.width() * ppp / size.x).min(area.height() * ppp / size.y);
        if vector { fit.min(MAX_ZOOM) } else { fit.min(1.0) }
    }

    fn zoom_around(&mut self, factor: f32, at: Pos2, area: Rect) {
        let Some((_, fit)) = self.last else { return };
        let now = self.shown_scale().unwrap_or(fit);
        let next = (now * factor).clamp(fit.min(1.0) * 0.5, MAX_ZOOM);
        if next <= fit * 1.001 && factor < 1.0 {
            self.fit();
            return;
        }
        // Keep the point under the pointer where it is.
        let middle = area.center() + self.pan;
        let middle = at - (at - middle) * (next / now);
        self.pan = middle - area.center();
        self.zoom = Zoom::Scale(next);
    }

    /// The picture's size in points at `scale`, as turned.
    fn shown_size(&self, size: [u32; 2], scale: f32, ppp: f32) -> Vec2 {
        let (w, h) = if self.orient.swaps() { (size[1], size[0]) } else { (size[0], size[1]) };
        Vec2::new(w as f32, h as f32) * scale / ppp
    }

    /// Draw the picture in `area` and take its pointer input: the wheel and pinches zoom, a
    /// drag moves a picture bigger than its space, a double-click goes between fitting and
    /// 100%. A click on the picture is in the response, for the app to use.
    pub fn show(&mut self, ui: &mut egui::Ui, area: Rect, shown: Shown) -> egui::Response {
        let ppp = ui.ctx().pixels_per_point();
        let upright = if self.orient.swaps() { Vec2::new(shown.size[1] as f32, shown.size[0] as f32) } else { Vec2::new(shown.size[0] as f32, shown.size[1] as f32) };
        let fit = Self::fit_scale(area, upright.max(Vec2::splat(1.0)), ppp, shown.vector);
        self.last = Some((area, fit));
        let resp = ui.interact(area, ui.scope_id().with("rivet-stage"), Sense::click_and_drag());

        if resp.hovered() {
            let (pinch, wheel) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
            let factor = pinch * (wheel.y / 240.0).exp();
            if (factor - 1.0).abs() > 1e-4 {
                let at = resp.hover_pos().unwrap_or(area.center());
                self.zoom_around(factor, at, area);
            }
        }
        if resp.double_clicked() {
            let at = resp.interact_pointer_pos().unwrap_or(area.center());
            if self.zoom == Zoom::Fit && fit < 0.999 {
                self.zoom_around(1.0 / fit, at, area);
                self.zoom = Zoom::Scale(1.0);
            } else {
                self.fit();
            }
        }
        let scale = self.shown_scale().unwrap_or(fit);
        let size = self.shown_size(shown.size, scale, ppp);
        if resp.dragged() && (size.x > area.width() || size.y > area.height()) {
            self.pan += resp.drag_delta();
        }
        // A picture smaller than its space stays in the middle; a bigger one can't leave a gap.
        let room = ((size - area.size()) / 2.0).max(Vec2::ZERO);
        self.pan = self.pan.clamp(-room, room);
        if room.x > 0.0 || room.y > 0.0 {
            ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else if resp.hovered() { egui::CursorIcon::Grab } else { egui::CursorIcon::Default });
        }

        let rect = Rect::from_center_size(area.center() + self.pan, size);
        // Whole pixels at the edges, so a picture at 100% is sharp.
        let rect = Rect::from_min_size((rect.min * ppp).round() / ppp, size);
        let painter = ui.painter().with_clip_rect(area);
        if shown.alpha {
            painter.add(egui::Shape::mesh(Self::checker(ui.visuals(), rect.intersect(area))));
        }
        painter.add(egui::Shape::mesh(self.mesh(rect, shown.texture.id())));
        resp
    }

    /// The picture as two triangles, its corners taken from the texture in turned order.
    fn mesh(&self, rect: Rect, tex: egui::TextureId) -> egui::Mesh {
        let uv = [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)];
        // Which corner of the texture shows at each corner of the screen: top left, top right,
        // bottom right, bottom left.
        let mut at = [0usize, 1, 2, 3];
        for _ in 0..self.orient.turns % 4 {
            at = [at[3], at[0], at[1], at[2]];
        }
        if self.orient.mirror {
            at = [at[1], at[0], at[3], at[2]];
        }
        let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
        let mut mesh = egui::Mesh::with_texture(tex);
        for (i, pos) in corners.into_iter().enumerate() {
            mesh.vertices.push(egui::epaint::Vertex { pos, uv: uv[at[i]], color: Color32::WHITE });
        }
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        mesh
    }

    /// Squares of two of the theme's grays over `r`, under see-through pictures.
    fn checker(v: &egui::Visuals, r: Rect) -> egui::Mesh {
        const CELL: f32 = 10.0;
        let (a, b) = (rivet_look::palette::mix(v.extreme_bg_color, v.text_color(), 0.10), rivet_look::palette::mix(v.extreme_bg_color, v.text_color(), 0.18));
        let mut mesh = egui::Mesh::default();
        mesh.add_colored_rect(r, a);
        if r.width() <= 0.0 || r.height() <= 0.0 {
            return mesh;
        }
        let (cols, rows) = ((r.width() / CELL).ceil() as usize, (r.height() / CELL).ceil() as usize);
        for y in 0..rows {
            for x in (y % 2..cols).step_by(2) {
                let min = r.min + Vec2::new(x as f32, y as f32) * CELL;
                mesh.add_colored_rect(Rect::from_min_size(min, Vec2::splat(CELL)).intersect(r), b);
            }
        }
        mesh
    }
}

/// How a picture's texture is filtered: smooth both ways, with smaller copies for zooming out
/// so a big photo fitted to the window doesn't shimmer.
pub const TEXTURE_OPTIONS: egui::TextureOptions = egui::TextureOptions { magnification: egui::TextureFilter::Linear, minification: egui::TextureFilter::Linear, wrap_mode: egui::TextureWrapMode::ClampToEdge, mipmap_mode: Some(egui::TextureFilter::Linear) };

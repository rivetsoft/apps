//! `--screenshot`, drawn in memory.
//!
//! A real window only gets new frames while the compositor is showing it: with the screen off,
//! locked or on another workspace, Wayland (and Xwayland) stop asking for frames and a window-
//! based screenshot never comes. So checks don't use a window at all: the app runs its usual
//! frames here, against a 1100×700 "window" that exists only in memory, and the last frame is
//! drawn with a small software renderer (egui's triangles, textures and premultiplied blending,
//! as its GPU painter does) and saved as a PNG. Nothing can take focus, and nothing waits for
//! the screen.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, TextureId, Vec2, ViewportCommand, ViewportId};
use eframe::epaint::{ClippedPrimitive, ImageData, Mesh, Primitive};

/// Give up after this long (a check stops a program at 90 s).
const LIMIT: Duration = Duration::from_secs(60);

/// Run the app's frames in memory until it closes itself (after saving its screenshot).
pub fn run(size: [f32; 2], make: impl FnOnce(&eframe::CreationContext) -> Box<dyn eframe::App>) {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = make(&cc);
    let mut frame = eframe::Frame::_new_kittest();
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(size[0], size[1]));
    let mut textures: HashMap<TextureId, ColorImage> = HashMap::new();
    let mut pending_events: Vec<egui::Event> = Vec::new();
    let start = Instant::now();
    loop {
        let mut raw = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(start.elapsed().as_secs_f64()),
            predicted_dt: 1.0 / 60.0,
            focused: false,
            events: std::mem::take(&mut pending_events),
            ..Default::default()
        };
        if let Some(info) = raw.viewports.get_mut(&ViewportId::ROOT) {
            info.native_pixels_per_point = Some(1.0);
            info.inner_rect = Some(screen);
            info.outer_rect = Some(screen);
            info.focused = Some(false);
        }
        app.raw_input_hook(&ctx, &mut raw);
        let mut out = ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx, &mut frame);
            app.ui(ui, &mut frame);
        });
        // Keep the textures the frame uses (fonts, icons, thumbnails). The update is handled
        // right here, in full; epaint panics if one is dropped unread.
        let mut delta = std::mem::take(&mut out.textures_delta);
        let freed: Vec<TextureId> = delta.free.iter().copied().collect();
        for (id, deltas) in &delta.set {
            for d in deltas {
                let ImageData::Color(img) = &d.image;
                match d.pos {
                    None => {
                        textures.insert(*id, (**img).clone());
                    }
                    Some([x, y]) => {
                        if let Some(t) = textures.get_mut(id) {
                            for row in 0..img.size[1] {
                                for col in 0..img.size[0] {
                                    let (tx, ty) = (x + col, y + row);
                                    if tx < t.size[0] && ty < t.size[1] {
                                        t.pixels[ty * t.size[0] + tx] = img.pixels[row * img.size[0] + col];
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        delta.clear();
        let mut close = false;
        let commands = out.viewport_output.get(&ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default();
        for cmd in commands {
            match cmd {
                ViewportCommand::Close => close = true,
                ViewportCommand::Screenshot(callback) => {
                    let prims = ctx.tessellate(out.shapes.clone(), out.pixels_per_point);
                    let clear = app.clear_color(&ctx.global_style().visuals);
                    let image = render(&prims, &textures, [size[0] as usize, size[1] as usize], clear);
                    callback.complete(Arc::new(image));
                }
                _ => {}
            }
        }
        // Freed after this frame is drawn, as a GPU painter does.
        for id in &freed {
            textures.remove(id);
        }
        if close || start.elapsed() > LIMIT {
            if !close {
                eprintln!("screenshot timed out");
            }
            return;
        }
        // About 60 frames a second, so background work (icons, thumbnails) has time to land.
        std::thread::sleep(Duration::from_millis(12));
        pending_events.clear();
    }
}

// ------------------------------------------------------------------ software renderer

/// Draw tessellated egui output into an image: premultiplied colours, blended in gamma space
/// with "source over", like egui's own painters.
fn render(prims: &[ClippedPrimitive], textures: &HashMap<TextureId, ColorImage>, size: [usize; 2], clear: [f32; 4]) -> ColorImage {
    let [w, h] = size;
    // The clear colour is in gamma space (0–1), like the frame buffer.
    let mut px: Vec<[f32; 4]> = vec![[clear[0], clear[1], clear[2], 1.0].map(|v| v.clamp(0.0, 1.0) * 255.0); w * h];
    for p in prims {
        let Primitive::Mesh(mesh) = &p.primitive else { continue };
        let clip = p.clip_rect.intersect(Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32)));
        if clip.width() <= 0.0 || clip.height() <= 0.0 {
            continue;
        }
        draw_mesh(&mut px, w, h, mesh, clip, textures.get(&mesh.texture_id));
    }
    let pixels = px.iter().map(|p| Color32::from_rgba_premultiplied(p[0] as u8, p[1] as u8, p[2] as u8, p[3] as u8)).collect();
    ColorImage::new([w, h], pixels)
}

fn draw_mesh(px: &mut [[f32; 4]], w: usize, h: usize, mesh: &Mesh, clip: Rect, tex: Option<&ColorImage>) {
    for tri in mesh.indices.as_chunks::<3>().0 {
        let v = [&mesh.vertices[tri[0] as usize], &mesh.vertices[tri[1] as usize], &mesh.vertices[tri[2] as usize]];
        let (a, b, c) = (v[0].pos, v[1].pos, v[2].pos);
        let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if area.abs() < 1e-6 {
            continue;
        }
        let min_x = a.x.min(b.x).min(c.x).max(clip.min.x).floor().max(0.0) as usize;
        let max_x = (a.x.max(b.x).max(c.x).min(clip.max.x).ceil() as usize).min(w);
        let min_y = a.y.min(b.y).min(c.y).max(clip.min.y).floor().max(0.0) as usize;
        let max_y = (a.y.max(b.y).max(c.y).min(clip.max.y).ceil() as usize).min(h);
        for y in min_y..max_y {
            let py = y as f32 + 0.5;
            if py < clip.min.y || py > clip.max.y {
                continue;
            }
            for x in min_x..max_x {
                let pxf = x as f32 + 0.5;
                if pxf < clip.min.x || pxf > clip.max.x {
                    continue;
                }
                // Edge tests, computed the same way for both triangles that share an edge, so a
                // pixel on that edge belongs to exactly one of them (the "top-left" rule).
                // Otherwise see-through shapes made of two triangles draw a darker seam.
                let p = egui::Pos2::new(pxf, py);
                let (e0, e1, e2) = (edge(b, c, p, area), edge(c, a, p, area), edge(a, b, p, area));
                if !(inside(e0, b, c, area) && inside(e1, c, a, area) && inside(e2, a, b, area)) {
                    continue;
                }
                let (w0, w1) = (e0 / area.abs(), e1 / area.abs());
                let w2 = 1.0 - w0 - w1;
                let mut col = [0.0f32; 4];
                for (i, vert) in v.iter().enumerate() {
                    let wt = [w0, w1, w2][i];
                    let cc = vert.color;
                    col[0] += cc.r() as f32 * wt;
                    col[1] += cc.g() as f32 * wt;
                    col[2] += cc.b() as f32 * wt;
                    col[3] += cc.a() as f32 * wt;
                }
                if let Some(t) = tex {
                    let u = v[0].uv.x * w0 + v[1].uv.x * w1 + v[2].uv.x * w2;
                    let vv = v[0].uv.y * w0 + v[1].uv.y * w1 + v[2].uv.y * w2;
                    let s = sample(t, u, vv);
                    for k in 0..4 {
                        col[k] = col[k] * s[k] / 255.0;
                    }
                }
                // Source over, premultiplied.
                let d = &mut px[y * w + x];
                let inv = 1.0 - col[3] / 255.0;
                for k in 0..4 {
                    d[k] = (col[k] + d[k] * inv).min(255.0);
                }
            }
        }
    }
}

/// How far `p` is inside the edge a→b (positive inside), for a triangle of signed `area`.
/// The two endpoints are always taken in the same order, so neighbours sharing the edge get
/// exactly opposite values.
fn edge(a: Pos2, b: Pos2, p: Pos2, area: f32) -> f32 {
    let raw = |a: Pos2, b: Pos2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let flip = (a.x, a.y) > (b.x, b.y);
    let e = if flip { -raw(b, a) } else { raw(a, b) };
    if area > 0.0 { e } else { -e }
}

/// Inside, or exactly on a top or left edge.
fn inside(e: f32, a: Pos2, b: Pos2, area: f32) -> bool {
    if e != 0.0 {
        return e > 0.0;
    }
    // Direction of the edge as the triangle (made counter-clockwise) walks it.
    let (dx, dy) = if area > 0.0 { (b.x - a.x, b.y - a.y) } else { (a.x - b.x, a.y - b.y) };
    dy < 0.0 || (dy == 0.0 && dx > 0.0)
}

/// Bilinear texture lookup (clamped), premultiplied values 0–255.
fn sample(t: &ColorImage, u: f32, v: f32) -> [f32; 4] {
    let (tw, th) = (t.size[0] as f32, t.size[1] as f32);
    let x = (u * tw - 0.5).clamp(0.0, tw - 1.0);
    let y = (v * th - 0.5).clamp(0.0, th - 1.0);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(t.size[0] - 1), (y0 + 1).min(t.size[1] - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let get = |xx: usize, yy: usize| {
        let p = t.pixels[yy * t.size[0] + xx];
        [p.r() as f32, p.g() as f32, p.b() as f32, p.a() as f32]
    };
    let (a, b, c, d) = (get(x0, y0), get(x1, y0), get(x0, y1), get(x1, y1));
    let mut out = [0.0; 4];
    for k in 0..4 {
        let top = a[k] + (b[k] - a[k]) * fx;
        let bot = c[k] + (d[k] - c[k]) * fx;
        out[k] = top + (bot - top) * fy;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_a_plain_rectangle_over_the_background() {
        let mut mesh = Mesh::default();
        mesh.add_colored_rect(Rect::from_min_max(Pos2::new(2.0, 2.0), Pos2::new(6.0, 6.0)), Color32::from_rgb(255, 0, 0));
        let prims = vec![ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(mesh) }];
        let img = render(&prims, &HashMap::new(), [8, 8], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(img.pixels[3 * 8 + 3], Color32::from_rgb(255, 0, 0));
        assert_eq!(img.pixels[0], Color32::from_rgb(0, 0, 0));
    }
}

#[cfg(test)]
mod seam_tests {
    use super::*;

    #[test]
    fn two_see_through_triangles_leave_no_seam() {
        // A half-transparent square made of two triangles: every pixel gets exactly one coat.
        let mut mesh = Mesh::default();
        mesh.add_colored_rect(Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(16.0, 16.0)), Color32::from_black_alpha(128));
        let prims = vec![ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(mesh) }];
        let img = render(&prims, &HashMap::new(), [16, 16], [1.0, 1.0, 1.0, 1.0]);
        let first = img.pixels[0];
        assert!(img.pixels.iter().all(|p| *p == first), "a seam: {:?}", img.pixels.iter().filter(|p| **p != first).count());
    }
}

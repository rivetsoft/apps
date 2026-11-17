//! A small software renderer for egui: tessellated triangles, textures and premultiplied "source
//! over" blending in gamma space, as egui's GPU painters do. Used for in-memory screenshots and
//! by Rivet Shell, which draws its panel, dock and widgets straight into Wayland shared-memory
//! buffers (no GPU, no window).

use std::collections::HashMap;

use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, TextureId, TexturesDelta, Vec2};
use eframe::epaint::{ClippedPrimitive, ImageData, Mesh, Primitive};

/// The textures a context has handed out (fonts, icons, pictures), kept up to date.
#[derive(Default)]
pub struct Textures {
    map: HashMap<TextureId, ColorImage>,
    to_free: Vec<TextureId>,
}

impl Textures {
    /// Take in a frame's texture changes (before drawing it).
    pub fn update(&mut self, delta: &TexturesDelta) {
        for id in self.to_free.drain(..) {
            self.map.remove(&id);
        }
        for (id, deltas) in &delta.set {
            for d in deltas {
                let ImageData::Color(img) = &d.image;
                match d.pos {
                    None => {
                        self.map.insert(*id, (**img).clone());
                    }
                    Some([x, y]) => {
                        if let Some(t) = self.map.get_mut(id) {
                            for row in 0..img.size[1] {
                                for col in 0..img.size[0] {
                                    let (tx, ty) = (x + col, y + row);
                                    if tx < t.size[0] && ty < t.size[1] {
                                        t.pixels[ty * t.size[0] + tx] =
                                            img.pixels[row * img.size[0] + col];
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Freed after this frame is drawn, as a GPU painter does.
        self.to_free.extend(delta.free.iter().copied());
    }

    pub fn get(&self, id: &TextureId) -> Option<&ColorImage> {
        self.map.get(id)
    }
}

/// Byte order of a pixel buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// R, G, B, A (images).
    Rgba,
    /// B, G, R, A: Wayland's ARGB8888 in memory (little-endian).
    Bgra,
}

/// Draw `prims` over `buf` (premultiplied, `w`×`h`, 4 bytes a pixel in `order`). `scale` turns
/// egui points into pixels. The buffer's contents are the background: clear it first for a
/// transparent surface, or fill it with a picture to draw on top of.
pub fn draw(
    buf: &mut [u8],
    w: usize,
    h: usize,
    order: Order,
    prims: &[ClippedPrimitive],
    textures: &Textures,
    scale: f32,
) {
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32));
    for p in prims {
        let Primitive::Mesh(mesh) = &p.primitive else {
            continue;
        };
        let clip = Rect::from_min_max(
            (p.clip_rect.min.to_vec2() * scale).to_pos2(),
            (p.clip_rect.max.to_vec2() * scale).to_pos2(),
        )
        .intersect(screen);
        if clip.width() <= 0.0 || clip.height() <= 0.0 {
            continue;
        }
        draw_mesh(
            buf,
            w,
            h,
            order,
            mesh,
            clip,
            textures.get(&mesh.texture_id),
            scale,
        );
    }
}

/// Draw into a new image over a solid colour (0–1, gamma space): screenshots.
pub fn render(
    prims: &[ClippedPrimitive],
    textures: &Textures,
    size: [usize; 2],
    clear: [f32; 4],
) -> ColorImage {
    let [w, h] = size;
    let c = clear.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let a = c[3] as f32 / 255.0;
    let px = [
        (c[0] as f32 * a) as u8,
        (c[1] as f32 * a) as u8,
        (c[2] as f32 * a) as u8,
        c[3],
    ];
    let mut buf: Vec<u8> = px.iter().copied().cycle().take(w * h * 4).collect();
    draw(&mut buf, w, h, Order::Rgba, prims, textures, 1.0);
    let pixels = buf
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3]))
        .collect();
    ColorImage::new([w, h], pixels)
}

#[allow(clippy::too_many_arguments)]
fn draw_mesh(
    buf: &mut [u8],
    w: usize,
    h: usize,
    order: Order,
    mesh: &Mesh,
    clip: Rect,
    tex: Option<&ColorImage>,
    scale: f32,
) {
    let (ri, bi) = match order {
        Order::Rgba => (0, 2),
        Order::Bgra => (2, 0),
    };
    for tri in mesh.indices.as_chunks::<3>().0 {
        let v = [
            &mesh.vertices[tri[0] as usize],
            &mesh.vertices[tri[1] as usize],
            &mesh.vertices[tri[2] as usize],
        ];
        let at = |p: Pos2| Pos2::new(p.x * scale, p.y * scale);
        let (a, b, c) = (at(v[0].pos), at(v[1].pos), at(v[2].pos));
        let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if area.abs() < 1e-6 {
            continue;
        }
        let min_x = a.x.min(b.x).min(c.x).max(clip.min.x).floor().max(0.0) as usize;
        let max_x = (a.x.max(b.x).max(c.x).min(clip.max.x).ceil() as usize).min(w);
        let min_y = a.y.min(b.y).min(c.y).max(clip.min.y).floor().max(0.0) as usize;
        let max_y = (a.y.max(b.y).max(c.y).min(clip.max.y).ceil() as usize).min(h);
        // A flat-coloured, untextured triangle (most of a UI): one colour for every pixel.
        let flat = v[0].color == v[1].color && v[1].color == v[2].color;
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
                let (e0, e1, e2) = (
                    edge(b, c, p, area),
                    edge(c, a, p, area),
                    edge(a, b, p, area),
                );
                if !(inside(e0, b, c, area) && inside(e1, c, a, area) && inside(e2, a, b, area)) {
                    continue;
                }
                let (w0, w1) = (e0 / area.abs(), e1 / area.abs());
                let w2 = 1.0 - w0 - w1;
                let mut col = if flat {
                    let cc = v[0].color;
                    [cc.r() as f32, cc.g() as f32, cc.b() as f32, cc.a() as f32]
                } else {
                    let mut col = [0.0f32; 4];
                    for (i, vert) in v.iter().enumerate() {
                        let wt = [w0, w1, w2][i];
                        let cc = vert.color;
                        col[0] += cc.r() as f32 * wt;
                        col[1] += cc.g() as f32 * wt;
                        col[2] += cc.b() as f32 * wt;
                        col[3] += cc.a() as f32 * wt;
                    }
                    col
                };
                if let Some(t) = tex {
                    let u = v[0].uv.x * w0 + v[1].uv.x * w1 + v[2].uv.x * w2;
                    let vv = v[0].uv.y * w0 + v[1].uv.y * w1 + v[2].uv.y * w2;
                    let s = sample(t, u, vv);
                    for k in 0..4 {
                        col[k] = col[k] * s[k] / 255.0;
                    }
                }
                if col[3] <= 0.0 {
                    continue;
                }
                // Source over, premultiplied.
                let i = (y * w + x) * 4;
                let d = &mut buf[i..i + 4];
                let inv = 1.0 - col[3] / 255.0;
                d[ri] = (col[0] + d[ri] as f32 * inv).min(255.0) as u8;
                d[1] = (col[1] + d[1] as f32 * inv).min(255.0) as u8;
                d[bi] = (col[2] + d[bi] as f32 * inv).min(255.0) as u8;
                d[3] = (col[3] + d[3] as f32 * inv).min(255.0) as u8;
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
    let (dx, dy) = if area > 0.0 {
        (b.x - a.x, b.y - a.y)
    } else {
        (a.x - b.x, a.y - b.y)
    };
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

    fn rect(r: Rect, c: Color32) -> Vec<ClippedPrimitive> {
        let mut mesh = Mesh::default();
        mesh.add_colored_rect(r, c);
        vec![ClippedPrimitive {
            clip_rect: Rect::EVERYTHING,
            primitive: Primitive::Mesh(mesh),
        }]
    }

    #[test]
    fn draws_a_plain_rectangle_over_the_background() {
        let prims = rect(
            Rect::from_min_max(Pos2::new(2.0, 2.0), Pos2::new(6.0, 6.0)),
            Color32::from_rgb(255, 0, 0),
        );
        let img = render(&prims, &Textures::default(), [8, 8], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(img.pixels[3 * 8 + 3], Color32::from_rgb(255, 0, 0));
        assert_eq!(img.pixels[0], Color32::from_rgb(0, 0, 0));
    }

    #[test]
    fn two_see_through_triangles_leave_no_seam() {
        // A half-transparent square made of two triangles: every pixel gets exactly one coat.
        let prims = rect(
            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(16.0, 16.0)),
            Color32::from_black_alpha(128),
        );
        let img = render(&prims, &Textures::default(), [16, 16], [1.0, 1.0, 1.0, 1.0]);
        let first = img.pixels[0];
        assert!(
            img.pixels.iter().all(|p| *p == first),
            "a seam: {:?}",
            img.pixels.iter().filter(|p| **p != first).count()
        );
    }

    #[test]
    fn bgra_at_double_scale_on_a_transparent_buffer() {
        let prims = rect(
            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(2.0, 2.0)),
            Color32::from_rgb(255, 0, 0),
        );
        let mut buf = vec![0u8; 8 * 8 * 4];
        draw(
            &mut buf,
            8,
            8,
            Order::Bgra,
            &prims,
            &Textures::default(),
            2.0,
        );
        // 2 points at scale 2: 4 pixels, red in the third byte.
        assert_eq!(
            &buf[(3 * 8 + 3) * 4..(3 * 8 + 3) * 4 + 4],
            &[0, 0, 255, 255]
        );
        assert_eq!(&buf[(5 * 8 + 5) * 4..(5 * 8 + 5) * 4 + 4], &[0, 0, 0, 0]);
    }
}

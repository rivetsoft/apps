//! A small software renderer for egui: tessellated triangles, textures and premultiplied "source
//! over" blending in gamma space, as egui's GPU painters do. Used for in-memory screenshots and
//! by Rivet Shell, which draws its panel, dock and widgets straight into Wayland shared-memory
//! buffers (no GPU, no window).

use std::collections::HashMap;

use eframe::egui::{Color32, ColorImage, Pos2, Rect, TextureId, TexturesDelta, Vec2};
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
                                        t.pixels[ty * t.size[0] + tx] = img.pixels[row * img.size[0] + col];
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
pub fn draw(buf: &mut [u8], w: usize, h: usize, order: Order, prims: &[ClippedPrimitive], textures: &Textures, scale: f32) {
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32));
    for p in prims {
        let Primitive::Mesh(mesh) = &p.primitive else { continue };
        let clip = Rect::from_min_max((p.clip_rect.min.to_vec2() * scale).to_pos2(), (p.clip_rect.max.to_vec2() * scale).to_pos2()).intersect(screen);
        if clip.width() <= 0.0 || clip.height() <= 0.0 {
            continue;
        }
        draw_mesh(buf, w, h, order, mesh, clip, textures.get(&mesh.texture_id), scale);
    }
}

/// Draw into a new image over a solid colour (0–1, gamma space): screenshots.
pub fn render(prims: &[ClippedPrimitive], textures: &Textures, size: [usize; 2], clear: [f32; 4]) -> ColorImage {
    let [w, h] = size;
    let c = clear.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let a = c[3] as f32 / 255.0;
    let px = [(c[0] as f32 * a) as u8, (c[1] as f32 * a) as u8, (c[2] as f32 * a) as u8, c[3]];
    let mut buf: Vec<u8> = px.iter().copied().cycle().take(w * h * 4).collect();
    draw(&mut buf, w, h, Order::Rgba, prims, textures, 1.0);
    let pixels = buf.as_chunks::<4>().0.iter().map(|p| Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3])).collect();
    ColorImage::new([w, h], pixels)
}

/// One edge of a triangle as a line equation, `e = a·x + b·y + c`, positive inside. It's built
/// from the edge's two ends in a fixed order, so the two triangles that share an edge get exactly
/// opposite values and a pixel on it belongs to one of them only (the "top-left" rule decides
/// ties); otherwise see-through shapes made of two triangles would show a seam.
#[derive(Clone, Copy)]
struct Edge {
    a: f32,
    b: f32,
    c: f32,
    /// A pixel exactly on the edge counts as inside.
    tie: bool,
}

impl Edge {
    fn new(p: Pos2, q: Pos2, area: f32) -> Edge {
        let flip = (p.x, p.y) > (q.x, q.y);
        let (s, t) = if flip { (q, p) } else { (p, q) };
        // raw(x, y) = (t.x - s.x)(y - s.y) - (t.y - s.y)(x - s.x)
        let (dx, dy) = (t.x - s.x, t.y - s.y);
        let sign = if flip { -1.0 } else { 1.0 } * if area > 0.0 { 1.0 } else { -1.0 };
        // Direction of the edge as the triangle (made counter-clockwise) walks it.
        let (wx, wy) = if area > 0.0 { (q.x - p.x, q.y - p.y) } else { (p.x - q.x, p.y - q.y) };
        Edge { a: -dy * sign, b: dx * sign, c: (dy * s.x - dx * s.y) * sign, tie: wy < 0.0 || (wy == 0.0 && wx > 0.0) }
    }

    #[inline(always)]
    fn inside(e: f32, tie: bool) -> bool {
        e > 0.0 || (e == 0.0 && tie)
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_mesh(buf: &mut [u8], w: usize, h: usize, order: Order, mesh: &Mesh, clip: Rect, tex: Option<&ColorImage>, scale: f32) {
    let (ri, bi) = match order {
        Order::Rgba => (0, 2),
        Order::Bgra => (2, 0),
    };
    // Pixel centres inside the clip, as columns.
    let cx0 = (clip.min.x - 0.5).ceil().max(0.0) as usize;
    let cx1 = (((clip.max.x - 0.5).floor() + 1.0).max(0.0) as usize).min(w);
    for tri in mesh.indices.as_chunks::<3>().0 {
        let v = [&mesh.vertices[tri[0] as usize], &mesh.vertices[tri[1] as usize], &mesh.vertices[tri[2] as usize]];
        let at = |p: Pos2| Pos2::new(p.x * scale, p.y * scale);
        let (pa, pb, pc) = (at(v[0].pos), at(v[1].pos), at(v[2].pos));
        let area = (pb.x - pa.x) * (pc.y - pa.y) - (pb.y - pa.y) * (pc.x - pa.x);
        if area.abs() < 1e-6 {
            continue;
        }
        let flat = v[0].color == v[1].color && v[1].color == v[2].color;
        // Fully see-through and untextured: nothing to draw.
        if flat && tex.is_none() && v[0].color.a() == 0 {
            continue;
        }
        let x0 = (pa.x.min(pb.x).min(pc.x).floor().max(0.0) as usize).max(cx0);
        let x1 = (pa.x.max(pb.x).max(pc.x).ceil().max(0.0) as usize).min(cx1);
        let y0 = pa.y.min(pb.y).min(pc.y).max(clip.min.y).floor().max(0.0) as usize;
        let y1 = (pa.y.max(pb.y).max(pc.y).min(clip.max.y).ceil().max(0.0) as usize).min(h);
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        // e0 weighs vertex a (the edge opposite it), e1 vertex b, e2 vertex c.
        let edges = [Edge::new(pb, pc, area), Edge::new(pc, pa, area), Edge::new(pa, pb, area)];
        let inv_area = 1.0 / area.abs();
        let cc = v[0].color;
        let flat_col = [cc.r() as f32, cc.g() as f32, cc.b() as f32, cc.a() as f32];
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            if py < clip.min.y || py > clip.max.y {
                continue;
            }
            // On this row each edge is a·x + k. Narrow the row to where all three can be inside
            // (a pixel of slack each side; the exact test below decides).
            let ks = [edges[0].b * py + edges[0].c, edges[1].b * py + edges[1].c, edges[2].b * py + edges[2].c];
            let (mut lo, mut hi) = (x0 as f32, x1 as f32);
            let mut skip = false;
            for (e, k) in edges.iter().zip(ks) {
                if e.a > 0.0 {
                    lo = lo.max((-k / e.a - 0.5).floor() - 1.0);
                } else if e.a < 0.0 {
                    hi = hi.min((-k / e.a - 0.5).ceil() + 2.0);
                } else if k < 0.0 || (k == 0.0 && !e.tie) {
                    skip = true;
                }
            }
            if skip || lo >= hi {
                continue;
            }
            let row = y * w;
            let inside_at = |x: usize| {
                let px = x as f32 + 0.5;
                Edge::inside(edges[0].a * px + ks[0], edges[0].tie) && Edge::inside(edges[1].a * px + ks[1], edges[1].tie) && Edge::inside(edges[2].a * px + ks[2], edges[2].tie)
            };
            // A solid, untextured triangle (most of a UI): find where the row's run starts and
            // ends (a triangle's pixels on a row are one run), then fill it with integer maths.
            if flat && tex.is_none() {
                let (mut sx, mut ex) = (lo as usize, hi as usize);
                while sx < ex && !inside_at(sx) {
                    sx += 1;
                }
                while ex > sx && !inside_at(ex - 1) {
                    ex -= 1;
                }
                if sx >= ex {
                    continue;
                }
                let px = [cc.r(), cc.g(), cc.b(), cc.a()];
                let mut out = [0u8; 4];
                out[ri] = px[0];
                out[1] = px[1];
                out[bi] = px[2];
                out[3] = px[3];
                let span = &mut buf[(row + sx) * 4..(row + ex) * 4];
                if px[3] == 255 {
                    for d in span.as_chunks_mut::<4>().0 {
                        *d = out;
                    }
                } else {
                    let inv = 255 - px[3] as u32;
                    for d in span.as_chunks_mut::<4>().0 {
                        for k in 0..4 {
                            d[k] = (out[k] as u32 + (d[k] as u32 * inv + 127) / 255).min(255) as u8;
                        }
                    }
                }
                continue;
            }
            for x in lo as usize..hi as usize {
                let px = x as f32 + 0.5;
                let e0 = edges[0].a * px + ks[0];
                let e1 = edges[1].a * px + ks[1];
                let e2 = edges[2].a * px + ks[2];
                if !(Edge::inside(e0, edges[0].tie) && Edge::inside(e1, edges[1].tie) && Edge::inside(e2, edges[2].tie)) {
                    continue;
                }
                let (w0, w1) = (e0 * inv_area, e1 * inv_area);
                let w2 = 1.0 - w0 - w1;
                let mut col = if flat {
                    flat_col
                } else {
                    let mut col = [0.0f32; 4];
                    for (vert, wt) in v.iter().zip([w0, w1, w2]) {
                        let c = vert.color;
                        col[0] += c.r() as f32 * wt;
                        col[1] += c.g() as f32 * wt;
                        col[2] += c.b() as f32 * wt;
                        col[3] += c.a() as f32 * wt;
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
                let i = (row + x) * 4;
                let d = &mut buf[i..i + 4];
                if col[3] >= 255.0 {
                    d[ri] = col[0] as u8;
                    d[1] = col[1] as u8;
                    d[bi] = col[2] as u8;
                    d[3] = 255;
                    continue;
                }
                // Source over, premultiplied.
                let inv = 1.0 - col[3] / 255.0;
                d[ri] = (col[0] + d[ri] as f32 * inv).min(255.0) as u8;
                d[1] = (col[1] + d[1] as f32 * inv).min(255.0) as u8;
                d[bi] = (col[2] + d[bi] as f32 * inv).min(255.0) as u8;
                d[3] = (col[3] + d[3] as f32 * inv).min(255.0) as u8;
            }
        }
    }
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
        vec![ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(mesh) }]
    }

    #[test]
    fn draws_a_plain_rectangle_over_the_background() {
        let prims = rect(Rect::from_min_max(Pos2::new(2.0, 2.0), Pos2::new(6.0, 6.0)), Color32::from_rgb(255, 0, 0));
        let img = render(&prims, &Textures::default(), [8, 8], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(img.pixels[3 * 8 + 3], Color32::from_rgb(255, 0, 0));
        assert_eq!(img.pixels[0], Color32::from_rgb(0, 0, 0));
        // Exactly the rectangle's pixels: 2..6 in both directions.
        let red = img.pixels.iter().filter(|p| **p == Color32::from_rgb(255, 0, 0)).count();
        assert_eq!(red, 16);
    }

    #[test]
    fn two_see_through_triangles_leave_no_seam() {
        // A half-transparent square made of two triangles: every pixel gets exactly one coat.
        let prims = rect(Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(16.0, 16.0)), Color32::from_black_alpha(128));
        let img = render(&prims, &Textures::default(), [16, 16], [1.0, 1.0, 1.0, 1.0]);
        let first = img.pixels[0];
        assert!(img.pixels.iter().all(|p| *p == first), "a seam: {:?}", img.pixels.iter().filter(|p| **p != first).count());
    }

    #[test]
    fn a_slanted_triangle_fills_what_it_covers() {
        // A right triangle over half of a 20×20 square: about 200 pixels, none outside.
        let mut mesh = Mesh::default();
        let c = Color32::from_rgb(0, 0, 255);
        mesh.colored_vertex(Pos2::new(0.0, 0.0), c);
        mesh.colored_vertex(Pos2::new(20.0, 0.0), c);
        mesh.colored_vertex(Pos2::new(0.0, 20.0), c);
        mesh.add_triangle(0, 1, 2);
        let prims = vec![ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(mesh) }];
        let img = render(&prims, &Textures::default(), [24, 24], [0.0, 0.0, 0.0, 1.0]);
        let n = img.pixels.iter().filter(|p| **p == c).count();
        assert!((190..=210).contains(&n), "{n}");
        assert_eq!(img.pixels[19 * 24 + 19], Color32::from_rgb(0, 0, 0));
        assert_eq!(img.pixels[24 + 1], c);
    }

    #[test]
    fn bgra_at_double_scale_on_a_transparent_buffer() {
        let prims = rect(Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(2.0, 2.0)), Color32::from_rgb(255, 0, 0));
        let mut buf = vec![0u8; 8 * 8 * 4];
        draw(&mut buf, 8, 8, Order::Bgra, &prims, &Textures::default(), 2.0);
        // 2 points at scale 2: 4 pixels, red in the third byte.
        assert_eq!(&buf[(3 * 8 + 3) * 4..(3 * 8 + 3) * 4 + 4], &[0, 0, 255, 255]);
        assert_eq!(&buf[(5 * 8 + 5) * 4..(5 * 8 + 5) * 4 + 4], &[0, 0, 0, 0]);
    }

    /// `cargo test --release -p rivet-kit time_full_screen -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn time_full_screen() {
        let mut mesh = Mesh::default();
        mesh.add_colored_rect(Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1280.0, 720.0)), Color32::from_black_alpha(70));
        mesh.add_colored_rect(Rect::from_min_max(Pos2::new(300.0, 100.0), Pos2::new(900.0, 600.0)), Color32::from_rgb(40, 40, 44));
        let prims = vec![ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(mesh) }];
        let mut buf = vec![0u8; 2560 * 1440 * 4];
        let t = std::time::Instant::now();
        for _ in 0..5 {
            draw(&mut buf, 2560, 1440, Order::Bgra, &prims, &Textures::default(), 2.0);
        }
        eprintln!("TIME per frame: {:?}", t.elapsed() / 5);
    }
}

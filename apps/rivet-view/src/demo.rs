//! Made-up pictures for screenshot runs: landscapes painted from a few numbers, a starry
//! night, an animation and a drawing, with made-up camera details. Nothing in them is anyone's.

use std::path::{Path, PathBuf};

use image::{Rgb, RgbImage};

use crate::save::{V, app1, tiff};

type C = [f32; 3];

fn hash(n: i32, seed: u32) -> f32 {
    let mut x = (n as u32).wrapping_mul(0x9E37_79B1) ^ seed.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// Smooth noise along a line, between 0 and 1.
fn noise(x: f32, seed: u32) -> f32 {
    let i = x.floor() as i32;
    let t = x - x.floor();
    let t = t * t * (3.0 - 2.0 * t);
    hash(i, seed) * (1.0 - t) + hash(i + 1, seed) * t
}

fn ridge(x: f32, seed: u32) -> f32 {
    (0..5).map(|o| noise(x * (1 << o) as f32, seed + o) / (1 << o) as f32).sum::<f32>() / 1.9375
}

fn mix(a: C, b: C, t: f32) -> C {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn hex(h: u32) -> C {
    [((h >> 16) & 255) as f32, ((h >> 8) & 255) as f32, (h & 255) as f32]
}

struct Layer {
    /// Height of the ridge's foot and its swing, as parts of the picture's height.
    base: f32,
    swing: f32,
    /// Ridges across the picture.
    width: f32,
    color: C,
    seed: u32,
    /// Snow on peaks higher than this part of the height (0: none).
    snow: f32,
}

struct Scene {
    sky: [C; 3],
    sun: Option<(f32, f32, f32, C)>,
    layers: Vec<Layer>,
    /// Water from this part of the height down.
    water: Option<f32>,
    /// Ripples of sand instead of plain ground (dunes).
    sand: bool,
}

fn paint(w: u32, h: u32, s: &Scene) -> RgbImage {
    let (wf, hf) = (w as f32, h as f32);
    let horizon = s.water.unwrap_or(1.0);
    // Each layer's ridge line, per column.
    let lines: Vec<Vec<f32>> = s.layers.iter().map(|l| (0..w).map(|x| hf * (1.0 - l.base - l.swing * ridge(x as f32 / wf * l.width, l.seed))).collect()).collect();
    let above = |x: u32, y: f32| -> C {
        let t = y / (hf * horizon);
        let mut c = if t < 0.55 { mix(s.sky[0], s.sky[1], t / 0.55) } else { mix(s.sky[1], s.sky[2], (t - 0.55) / 0.45) };
        if let Some((sx, sy, r, sc)) = s.sun {
            let d = ((x as f32 - sx * wf).powi(2) + (y - sy * hf).powi(2)).sqrt() / (r * hf);
            c = mix(c, sc, (1.0 - (d - 1.0) * 0.12).clamp(0.0, 1.0) * if d < 1.0 { 1.0 } else { 0.35 });
            c = mix(c, [255.0, 250.0, 235.0], if d < 1.0 { 0.85 } else { 0.0 });
        }
        for (l, line) in s.layers.iter().zip(&lines) {
            let top = line[x as usize];
            if y >= top {
                let depth = ((y - top) / (hf * 0.25)).clamp(0.0, 1.0);
                let mut lc = mix(l.color, mix(l.color, [0.0, 0.0, 0.0], 0.45), depth);
                // Snow on what rises above the snow line, deeper on the higher peaks.
                let snowline = hf * (1.0 - l.snow);
                if l.snow > 0.0 && top < snowline {
                    let deep = (snowline - top) * 0.7 * (0.6 + 0.8 * noise(x as f32 * 0.03, l.seed + 9));
                    if y - top < deep {
                        lc = mix([236.0, 240.0, 248.0], lc, ((y - top) / deep.max(1.0)).powi(3));
                    }
                }
                if s.sand {
                    let ripple = ((x as f32 * 0.035 + y * 0.18 + 6.0 * noise(x as f32 * 0.004, l.seed)).sin() * 0.5 + 0.5) * 0.12;
                    lc = mix(lc, [255.0, 236.0, 200.0], ripple);
                }
                c = lc;
            }
        }
        c
    };
    RgbImage::from_fn(w, h, |x, y| {
        let yf = y as f32;
        let c = if yf < hf * horizon {
            above(x, yf)
        } else {
            // The water: what is above, upside down, darker, broken up by ripples.
            let wl = hf * horizon;
            let ripple = (noise(yf * 0.35, 77) - 0.5) * 18.0 * ((yf - wl) / (hf - wl) + 0.2) + (x as f32 * 0.02 + yf * 0.9).sin() * 2.0;
            let mirrored = (2.0 * wl - yf + ripple).clamp(0.0, wl - 1.0);
            let c = above((x as f32 + ripple).clamp(0.0, wf - 1.0) as u32, mirrored);
            mix(c, [10.0, 22.0, 46.0], 0.35 + 0.3 * (yf - wl) / (hf - wl))
        };
        // A soft dark edge, as a lens gives.
        let (dx, dy) = (x as f32 / wf - 0.5, yf / hf - 0.5);
        let v = 1.0 - (dx * dx + dy * dy) * 0.55;
        Rgb([(c[0] * v) as u8, (c[1] * v) as u8, (c[2] * v) as u8])
    })
}

fn night(w: u32, h: u32) -> RgbImage {
    let (wf, hf) = (w as f32, h as f32);
    let hills: Vec<f32> = (0..w).map(|x| hf * (0.82 - 0.12 * ridge(x as f32 / wf * 3.0, 41))).collect();
    let mut img = RgbImage::from_fn(w, h, |x, y| {
        let t = y as f32 / hf;
        let mut c = mix(hex(0x06081a), hex(0x2a1d4f), t);
        // The milky band across the sky.
        let band = ((x as f32 / wf - 0.2) * 0.9 - (t - 0.15)).abs();
        c = mix(c, hex(0x6c5a9a), (0.16 - band).max(0.0) * 2.2 * noise(x as f32 * 0.01 + t * 9.0, 5));
        if y as f32 > hills[x as usize] {
            c = hex(0x05060d);
        }
        Rgb([c[0] as u8, c[1] as u8, c[2] as u8])
    });
    for i in 0..1400 {
        let (x, y) = ((hash(i, 1) * wf) as u32, (hash(i, 2) * hf * 0.8) as u32);
        if (y as f32) < hills[x as usize] - 2.0 {
            let b = 120.0 + 135.0 * hash(i, 3).powi(3);
            img.put_pixel(x, y, Rgb([b as u8, b as u8, (b * 1.05).min(255.0) as u8]));
            if hash(i, 4) > 0.97 && x + 1 < w && y + 1 < h {
                for (dx, dy) in [(1, 0), (0, 1), (1, 1)] {
                    img.put_pixel(x + dx, y + dy, Rgb([220, 225, 255]));
                }
            }
        }
    }
    img
}

/// Eight dots going round, as an animated GIF.
fn spinner(path: &Path) -> std::io::Result<()> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let f = std::fs::File::create(path)?;
    let mut enc = GifEncoder::new(f);
    let _ = enc.set_repeat(Repeat::Infinite);
    let n = 12;
    for k in 0..n {
        let img = image::RgbaImage::from_fn(240, 240, |x, y| {
            let (px, py) = (x as f32 - 120.0, y as f32 - 120.0);
            let mut c = [0x28u8, 0x1d, 0x24, 255];
            for d in 0..8 {
                let a = d as f32 / 8.0 * std::f32::consts::TAU;
                let (cx, cy) = (a.cos() * 70.0, a.sin() * 70.0);
                if (px - cx).powi(2) + (py - cy).powi(2) < 18.0f32.powi(2) {
                    let lit = ((d + n - k) % 8) as f32 / 8.0;
                    let c0 = mix(hex(0x5a3048), hex(0xd53f8c), 1.0 - lit);
                    c = [c0[0] as u8, c0[1] as u8, c0[2] as u8, 255];
                }
            }
            image::Rgba(c)
        });
        enc.encode_frame(image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(90, 1))).map_err(std::io::Error::other)?;
    }
    Ok(())
}

const BADGE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
  <circle cx="100" cy="100" r="88" fill="#0084b8" stroke="#24121b" stroke-width="8"/>
  <path d="M40 132 78 82 104 112 122 94 160 132Z" fill="#fff270" stroke="#24121b" stroke-width="6" stroke-linejoin="round"/>
  <circle cx="132" cy="66" r="14" fill="#d53f8c" stroke="#24121b" stroke-width="6"/>
</svg>
"##;

/// A JPEG with made-up camera details.
fn jpeg(path: &Path, img: &RgbImage, camera: (&str, &str, &str), shot: (u32, u32, u32, u32), taken: &str) -> std::io::Result<()> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88).encode_image(img).map_err(std::io::Error::other)?;
    let bytes = out.into_inner();
    let (exposure, f10, iso, focal) = shot;
    let block = tiff(
        vec![(0x010F, V::Ascii(camera.0.into())), (0x0110, V::Ascii(camera.1.into())), (0x0112, V::Short(1)), (0x0131, V::Ascii("Rivet View sample".into()))],
        vec![
            (0x829A, V::Rational(vec![(1, exposure)])),
            (0x829D, V::Rational(vec![(f10, 10)])),
            (0x8827, V::Short(iso as u16)),
            (0x9003, V::Ascii(taken.into())),
            (0x9209, V::Short(16)),
            (0x920A, V::Rational(vec![(focal, 1)])),
            (0xA434, V::Ascii(camera.2.into())),
        ],
        Vec::new(),
    );
    let mut file = bytes[..2].to_vec();
    file.extend(app1(&block));
    file.extend(&bytes[2..]);
    std::fs::write(path, file)
}

/// Fill `dir` with the sample pictures and return them by name.
pub fn make(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let (w, h) = (1800, 1200);
    let cam = ("Rivet", "Rivet R1", "Rivet 24–70 mm f/2.8");
    let coast = Scene {
        sky: [hex(0x3b2a6b), hex(0xd35a7a), hex(0xffb36b)],
        sun: Some((0.62, 0.6, 0.06, hex(0xffd27a))),
        layers: vec![
            Layer { base: 0.36, swing: 0.16, width: 2.0, color: hex(0x5a3a6e), seed: 3, snow: 0.0 },
            Layer { base: 0.34, swing: 0.1, width: 4.0, color: hex(0x3a2348), seed: 8, snow: 0.0 },
        ],
        water: Some(0.66),
        sand: false,
    };
    let lake = Scene {
        sky: [hex(0x2f6fb5), hex(0x8ec3ea), hex(0xe4f1fa)],
        sun: None,
        layers: vec![
            Layer { base: 0.42, swing: 0.4, width: 2.5, color: hex(0x7d8fae), seed: 21, snow: 0.62 },
            Layer { base: 0.38, swing: 0.2, width: 4.5, color: hex(0x3f6150), seed: 22, snow: 0.0 },
            Layer { base: 0.36, swing: 0.06, width: 9.0, color: hex(0x24432f), seed: 23, snow: 0.0 },
        ],
        water: Some(0.64),
        sand: false,
    };
    let dunes = Scene {
        sky: [hex(0x4a8fcf), hex(0xa7d3ef), hex(0xf4e3c4)],
        sun: Some((0.2, 0.18, 0.045, hex(0xfff4d6))),
        layers: vec![
            Layer { base: 0.42, swing: 0.2, width: 1.6, color: hex(0xd99a5b), seed: 31, snow: 0.0 },
            Layer { base: 0.28, swing: 0.22, width: 2.2, color: hex(0xe9b071), seed: 32, snow: 0.0 },
            Layer { base: 0.1, swing: 0.18, width: 2.8, color: hex(0xf0c185), seed: 33, snow: 0.0 },
        ],
        water: None,
        sand: true,
    };
    let meadow = Scene {
        sky: [hex(0x4f8fd8), hex(0x9fcbf0), hex(0xe9f4f0)],
        sun: Some((0.8, 0.16, 0.04, hex(0xfffbe8))),
        layers: vec![
            Layer { base: 0.4, swing: 0.12, width: 2.0, color: hex(0x6f9f62), seed: 51, snow: 0.0 },
            Layer { base: 0.3, swing: 0.1, width: 3.0, color: hex(0x4f8a3f), seed: 52, snow: 0.0 },
            Layer { base: 0.12, swing: 0.08, width: 5.0, color: hex(0x3c7a2e), seed: 53, snow: 0.0 },
        ],
        water: None,
        sand: false,
    };
    let mut made = Vec::new();
    let mut add = |name: &str| {
        let p = dir.join(name);
        made.push(p.clone());
        p
    };
    jpeg(&add("alpine lake.jpg"), &paint(w, h, &lake), cam, (320, 80, 100, 35), "2026:06:21 09:14:03")?;
    jpeg(&add("coast at dusk.jpg"), &paint(w, h, &coast), cam, (125, 40, 400, 50), "2026:08:02 20:41:27")?;
    paint(w, h, &dunes).save(add("dunes.webp")).map_err(std::io::Error::other)?;
    spinner(&add("loading dots.gif"))?;
    jpeg(&add("meadow.jpg"), &paint(w, h, &meadow), cam, (500, 56, 100, 24), "2026:05:14 13:05:52")?;
    night(w, h).save(add("night sky.png")).map_err(std::io::Error::other)?;
    std::fs::write(add("trail badge.svg"), BADGE)?;
    Ok(made)
}

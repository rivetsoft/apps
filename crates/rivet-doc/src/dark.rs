//! Dark pages: a page drawn light on dark in the window's own colors, keeping what color says.
//! Each color's lightness is turned round (in Oklab, so the change looks even to the eye) while
//! its hue and strength stay: black text turns light, white paper turns dark, a red heading
//! stays red and a blue link stays blue. Paper and ink then take the theme's background and
//! text colors. Pictures on the page are put back as they were, so photos aren't negatives.

use crate::{Area, Bitmap};

/// The theme's colors for the paper (the background) and the ink (the text).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DarkLook {
    pub paper: [u8; 3],
    pub ink: [u8; 3],
}

/// 6 bits a channel: 262,144 colors worked out once per look, then a lookup per pixel.
const BITS: u32 = 6;

pub struct Table {
    look: DarkLook,
    map: Vec<[u8; 3]>,
}

fn to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

fn oklab(rgb: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = rgb.map(to_linear);
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn from_oklab([ll, a, b]: [f32; 3]) -> [u8; 3] {
    let l = (ll + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (ll - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (ll - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        to_srgb(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
        to_srgb(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
        to_srgb(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
    ]
}

/// One color on a dark page.
pub fn convert(look: &DarkLook, rgb: [u8; 3]) -> [u8; 3] {
    let [l, a, b] = oklab(rgb);
    let (paper, ink) = (oklab(look.paper), oklab(look.ink));
    // 0 where the page was white, 1 where it was black.
    let t = (1.0 - l).clamp(0.0, 1.0);
    // Grays take the theme's tint; colors keep their own.
    let gray = 1.0 - ((a * a + b * b).sqrt() / 0.08).min(1.0);
    from_oklab([
        paper[0] + (ink[0] - paper[0]) * t,
        a + (paper[1] + (ink[1] - paper[1]) * t) * gray,
        b + (paper[2] + (ink[2] - paper[2]) * t) * gray,
    ])
}

impl Table {
    pub fn new(look: DarkLook) -> Table {
        let n = 1u32 << BITS;
        let step = |i: u32| ((i * 255 + (n - 1) / 2) / (n - 1)) as u8;
        let mut map = Vec::with_capacity((n * n * n) as usize);
        for r in 0..n {
            for g in 0..n {
                for b in 0..n {
                    map.push(convert(&look, [step(r), step(g), step(b)]));
                }
            }
        }
        Table { look, map }
    }

    pub fn look(&self) -> DarkLook {
        self.look
    }

    /// Turn a rendered page dark, then put back the pictures. `page_px` is the whole page's
    /// size in pixels and `origin` where this bitmap starts in it (it can be a part of the
    /// page); `page_pt` is the page's size in points, which `pictures` are measured in.
    pub fn apply(&self, bmp: &mut Bitmap, page_px: [u32; 2], origin: [u32; 2], page_pt: [f32; 2], pictures: &[Area]) {
        let original = bmp.rgba.clone();
        let shift = 8 - BITS;
        for px in bmp.rgba.as_chunks_mut::<4>().0 {
            let i = (((px[0] >> shift) as usize) << (2 * BITS)) | (((px[1] >> shift) as usize) << BITS) | (px[2] >> shift) as usize;
            let [r, g, b] = self.map[i];
            px[0] = r;
            px[1] = g;
            px[2] = b;
        }
        let page_area = page_pt[0] * page_pt[1];
        let (sx, sy) = (page_px[0] as f32 / page_pt[0].max(1.0), page_px[1] as f32 / page_pt[1].max(1.0));
        let w = bmp.width as i64;
        for a in pictures {
            // A picture over nearly all of the page is a scan: that is text, so it stays dark.
            if (a[2] - a[0]) * (a[3] - a[1]) > page_area * 0.9 {
                continue;
            }
            let x0 = ((a[0] * sx) as i64 - origin[0] as i64).clamp(0, w);
            let x1 = ((a[2] * sx).ceil() as i64 - origin[0] as i64).clamp(0, w);
            let y0 = ((a[1] * sy) as i64 - origin[1] as i64).clamp(0, bmp.height as i64);
            let y1 = ((a[3] * sy).ceil() as i64 - origin[1] as i64).clamp(0, bmp.height as i64);
            for y in y0..y1 {
                let (s, e) = (((y * w + x0) * 4) as usize, ((y * w + x1) * 4) as usize);
                bmp.rgba[s..e].copy_from_slice(&original[s..e]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOK: DarkLook = DarkLook { paper: [0x1e, 0x13, 0x1a], ink: [0xe8, 0xdd, 0xe3] };

    fn lum(c: [u8; 3]) -> u32 {
        c[0] as u32 * 3 + c[1] as u32 * 6 + c[2] as u32
    }

    #[test]
    fn paper_and_ink_take_the_theme() {
        let paper = convert(&LOOK, [255, 255, 255]);
        let ink = convert(&LOOK, [0, 0, 0]);
        for (got, want) in [(paper, LOOK.paper), (ink, LOOK.ink)] {
            for k in 0..3 {
                assert!((got[k] as i32 - want[k] as i32).abs() <= 3, "{got:?} {want:?}");
            }
        }
    }

    #[test]
    fn colors_keep_their_hue() {
        let red = convert(&LOOK, [200, 30, 30]);
        assert!(red[0] > red[1] + 40 && red[0] > red[2] + 40, "{red:?}");
        let blue = convert(&LOOK, [20, 60, 200]);
        assert!(blue[2] > blue[0] + 40, "{blue:?}");
        // Dark text turns light; a pale highlight turns dark.
        assert!(lum(convert(&LOOK, [40, 40, 40])) > lum(convert(&LOOK, [250, 240, 150])));
    }

    #[test]
    fn pictures_stay_as_they_were() {
        let t = Table::new(LOOK);
        let mut bmp = Bitmap { width: 10, height: 10, rgba: vec![255; 400] };
        t.apply(&mut bmp, [10, 10], [0, 0], [10.0, 10.0], &[[0.0, 0.0, 5.0, 5.0]]);
        assert_eq!(&bmp.rgba[0..4], &[255, 255, 255, 255]);
        assert_ne!(bmp.rgba[(9 * 10 + 9) * 4], 255);
    }
}

//! Scaling pictures for the screen. Here rather than in the apps because this crate is built
//! optimised even in debug builds, and the scaler is only fast when it is.

use fast_image_resize as fr;

/// An RGBA picture (`iw`×`ih`) scaled to cover `w`×`h`, cropping what overflows (centred), as
/// opaque BGRA bytes for a Wayland buffer.
pub fn cover_bgra(rgba: Vec<u8>, iw: u32, ih: u32, w: u32, h: u32) -> Option<Vec<u8>> {
    if w == 0 || h == 0 {
        return None;
    }
    let src = fr::images::Image::from_vec_u8(iw, ih, rgba, fr::PixelType::U8x4).ok()?;
    let mut dst = fr::images::Image::new(w, h, fr::PixelType::U8x4);
    let how = fr::ResizeOptions::new().resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Bilinear)).fit_into_destination(Some((0.5, 0.5))).use_alpha(false);
    fr::Resizer::new().resize(&src, &mut dst, &how).ok()?;
    let mut out = dst.into_vec();
    for p in out.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
        p[3] = 255;
    }
    Some(out)
}

/// The part of a picture inside `[x0, y0, x1, y1]` (pixels, clamped to the picture).
pub fn crop(img: &eframe::egui::ColorImage, area: [usize; 4]) -> eframe::egui::ColorImage {
    let [iw, ih] = img.size;
    let (x0, y0) = (area[0].min(iw), area[1].min(ih));
    let (x1, y1) = (area[2].clamp(x0, iw), area[3].clamp(y0, ih));
    let mut pixels = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for y in y0..y1 {
        pixels.extend_from_slice(&img.pixels[y * iw + x0..y * iw + x1]);
    }
    eframe::egui::ColorImage::new([x1 - x0, y1 - y0], pixels)
}

/// The part of a picture inside `area`, in squares of `block` pixels that each take the average
/// colour of what they cover: what is under them can no longer be read.
pub fn mosaic(img: &eframe::egui::ColorImage, area: [usize; 4], block: usize) -> eframe::egui::ColorImage {
    let mut out = crop(img, area);
    let [w, h] = out.size;
    let block = block.max(1);
    let mut by = 0;
    while by < h {
        let bh = block.min(h - by);
        let mut bx = 0;
        while bx < w {
            let bw = block.min(w - bx);
            let mut sum = [0u32; 4];
            for y in by..by + bh {
                for p in &out.pixels[y * w + bx..y * w + bx + bw] {
                    for (s, c) in sum.iter_mut().zip(p.to_array()) {
                        *s += c as u32;
                    }
                }
            }
            let n = (bw * bh) as u32;
            let c = eframe::egui::Color32::from_rgba_premultiplied((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, (sum[3] / n) as u8);
            for y in by..by + bh {
                out.pixels[y * w + bx..y * w + bx + bw].fill(c);
            }
            bx += bw;
        }
        by += bh;
    }
    out
}

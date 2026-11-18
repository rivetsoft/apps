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
    for p in out.chunks_exact_mut(4) {
        p.swap(0, 2);
        p[3] = 255;
    }
    Some(out)
}

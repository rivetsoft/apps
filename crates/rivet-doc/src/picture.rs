//! A single picture as a one-page document, and the drawing of pictures at any size that comics
//! use too.

use std::path::Path;

use image::imageops::FilterType;
use image::RgbaImage;

use crate::{Bitmap, Document, Kind, OpenError};

/// A picture drawn `width` × `height`, or the part of that in `clip`.
pub(crate) fn draw(img: &RgbaImage, width: u32, height: u32, clip: Option<[u32; 4]>) -> Bitmap {
    let [x0, y0, x1, y1] = clip.unwrap_or([0, 0, width, height]);
    let (w, h) = (x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1));
    let (sx, sy) = (img.width() as f32 / width.max(1) as f32, img.height() as f32 / height.max(1) as f32);
    // The part of the picture that shows, then scaled: never the whole picture at a huge size.
    let (cx, cy) = ((x0 as f32 * sx) as u32, (y0 as f32 * sy) as u32);
    let cw = ((w as f32 * sx).ceil() as u32).clamp(1, img.width().saturating_sub(cx).max(1));
    let ch = ((h as f32 * sy).ceil() as u32).clamp(1, img.height().saturating_sub(cy).max(1));
    let part = image::imageops::crop_imm(img, cx, cy, cw, ch).to_image();
    let filter = if w < cw { FilterType::Triangle } else { FilterType::CatmullRom };
    let out = if (w, h) == (cw, ch) { part } else { image::imageops::resize(&part, w, h, filter) };
    Bitmap { width: w, height: h, rgba: out.into_raw() }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<RgbaImage, String> {
    image::load_from_memory(bytes).map(|i| i.to_rgba8()).map_err(|e| format!("The picture couldn't be read ({e})."))
}

pub struct Picture {
    img: RgbaImage,
}

pub fn open(path: &Path) -> Result<Picture, OpenError> {
    let bytes = std::fs::read(path).map_err(|e| OpenError::Failed(format!("It couldn't be read ({e}).")))?;
    decode(&bytes).map(|img| Picture { img }).map_err(OpenError::Failed)
}

impl Document for Picture {
    fn kind(&self) -> Kind {
        Kind::Picture
    }

    fn page_count(&self) -> usize {
        1
    }

    fn page_sizes(&mut self) -> Vec<[f32; 2]> {
        // A picture's pixels count as points: at 100 % it is shown at its own size.
        vec![[self.img.width() as f32, self.img.height() as f32]]
    }

    fn render(&mut self, _page: usize, width: u32, height: u32, clip: Option<[u32; 4]>) -> Result<Bitmap, String> {
        Ok(draw(&self.img, width, height, clip))
    }
}

//! Saving a turned picture. A JPEG keeps every pixel: only its EXIF orientation (the note that
//! says which way is up) changes, written in place or added. PNG, WebP, TIFF, BMP, QOI, TGA and
//! PNM pictures are written again in their own format with the same pixels. The rest (GIF,
//! animations, drawings, HEIC, AVIF, camera raw files) can only be turned for viewing.
//!
//! The new file is written beside the old one and then put in its place, so a failure part way
//! leaves the picture as it was.

use std::path::{Path, PathBuf};

use rivet_media::picture::Orient;

/// What saving a turned picture would do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum How {
    /// Change the JPEG's orientation note.
    Note,
    /// Write the pixels again in this format.
    Rewrite(image::ImageFormat),
    /// It can't be saved turned.
    No,
}

pub fn how(path: &Path, animated: bool) -> How {
    if animated {
        return How::No;
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "jpe" | "jfif" => How::Note,
        "png" => How::Rewrite(image::ImageFormat::Png),
        "webp" => How::Rewrite(image::ImageFormat::WebP),
        "tif" | "tiff" => How::Rewrite(image::ImageFormat::Tiff),
        "bmp" | "dib" => How::Rewrite(image::ImageFormat::Bmp),
        "qoi" => How::Rewrite(image::ImageFormat::Qoi),
        "tga" => How::Rewrite(image::ImageFormat::Tga),
        "pnm" | "pbm" | "pgm" | "ppm" | "pam" => How::Rewrite(image::ImageFormat::Pnm),
        _ => How::No,
    }
}

/// Save `path` turned by `turn` (what was done in the window, on top of the orientation the file
/// already had, `had`).
pub fn save(path: &Path, had: Orient, turn: Orient, animated: bool) -> Result<(), String> {
    match how(path, animated) {
        How::Note => {
            let bytes = std::fs::read(path).map_err(|e| format!("The picture couldn't be read again ({e})."))?;
            let out = with_orientation(&bytes, had.then(turn).exif()).ok_or("This JPEG's details are laid out in a way that can't be changed safely, so it was left as it was.")?;
            replace(path, &out)
        }
        How::Rewrite(format) => {
            let reader = image::ImageReader::open(path).map_err(|e| format!("The picture couldn't be read again ({e})."))?.with_guessed_format().map_err(|e| e.to_string())?;
            let mut dec = reader.into_decoder().map_err(|e| format!("The picture couldn't be read again ({e})."))?;
            use image::ImageDecoder;
            let own = dec.orientation().map(|o| Orient::from_exif(o.to_exif() as u16)).unwrap_or_default();
            let img = image::DynamicImage::from_decoder(dec).map_err(|e| format!("The picture couldn't be read again ({e})."))?;
            let img = own.then(turn).apply(img);
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, format).map_err(|e| format!("The turned picture couldn't be written ({e})."))?;
            replace(path, &out.into_inner())
        }
        How::No => Err("This kind of picture can't be saved turned.".into()),
    }
}

/// Write `bytes` beside `path`, keep its permissions, then put it in its place.
fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp: PathBuf = path.with_file_name(format!(".{name}.rivet-view-saving"));
    let fail = |e: std::io::Error| {
        let _ = std::fs::remove_file(&tmp);
        match e.kind() {
            std::io::ErrorKind::PermissionDenied => "You don't have permission to change this picture.".to_string(),
            _ => format!("The turned picture couldn't be saved ({e})."),
        }
    };
    std::fs::write(&tmp, bytes).map_err(fail)?;
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, path).map_err(fail)
}

// ------------------------------------------------------------------ EXIF in a JPEG

/// A value in a TIFF field (the layout EXIF is written in).
pub enum V {
    Short(u16),
    Long(u32),
    Ascii(String),
    Rational(Vec<(u32, u32)>),
}

impl V {
    /// (type, count, bytes), big-endian.
    fn raw(&self) -> (u16, u32, Vec<u8>) {
        match self {
            V::Short(v) => (3, 1, v.to_be_bytes().to_vec()),
            V::Long(v) => (4, 1, v.to_be_bytes().to_vec()),
            V::Ascii(s) => {
                let mut b = s.as_bytes().to_vec();
                b.push(0);
                (2, b.len() as u32, b)
            }
            V::Rational(v) => (5, v.len() as u32, v.iter().flat_map(|(n, d)| n.to_be_bytes().into_iter().chain(d.to_be_bytes())).collect()),
        }
    }
}

fn ifd_len(entries: &[(u16, V)]) -> u32 {
    let data: u32 = entries.iter().map(|(_, v)| v.raw().2.len() as u32).filter(|n| *n > 4).map(|n| n + n % 2).sum();
    2 + 12 * entries.len() as u32 + 4 + data
}

fn ifd_bytes(entries: &[(u16, V)], at: u32) -> Vec<u8> {
    let mut out = (entries.len() as u16).to_be_bytes().to_vec();
    let mut data = Vec::new();
    let data_at = at + 2 + 12 * entries.len() as u32 + 4;
    for (tag, v) in entries {
        let (ty, count, mut b) = v.raw();
        out.extend(tag.to_be_bytes());
        out.extend(ty.to_be_bytes());
        out.extend(count.to_be_bytes());
        if b.len() <= 4 {
            b.resize(4, 0);
            out.extend(b);
        } else {
            out.extend((data_at + data.len() as u32).to_be_bytes());
            data.extend(&b);
            if data.len() % 2 == 1 {
                data.push(0);
            }
        }
    }
    out.extend(0u32.to_be_bytes());
    out.extend(data);
    out
}

/// An EXIF block: the main fields, and the camera's and the place's in their own lists.
pub fn tiff(mut main: Vec<(u16, V)>, camera: Vec<(u16, V)>, place: Vec<(u16, V)>) -> Vec<u8> {
    if !camera.is_empty() {
        main.push((0x8769, V::Long(0)));
    }
    if !place.is_empty() {
        main.push((0x8825, V::Long(0)));
    }
    main.sort_by_key(|(t, _)| *t);
    let mut camera = camera;
    camera.sort_by_key(|(t, _)| *t);
    let mut place = place;
    place.sort_by_key(|(t, _)| *t);
    let camera_at = 8 + ifd_len(&main);
    let place_at = camera_at + if camera.is_empty() { 0 } else { ifd_len(&camera) };
    for (t, v) in main.iter_mut() {
        match t {
            0x8769 => *v = V::Long(camera_at),
            0x8825 => *v = V::Long(place_at),
            _ => {}
        }
    }
    let mut out = b"MM\0\x2a\0\0\0\x08".to_vec();
    out.extend(ifd_bytes(&main, 8));
    if !camera.is_empty() {
        out.extend(ifd_bytes(&camera, camera_at));
    }
    if !place.is_empty() {
        out.extend(ifd_bytes(&place, place_at));
    }
    out
}

/// A JPEG's APP1 segment carrying an EXIF block.
pub fn app1(tiff: &[u8]) -> Vec<u8> {
    let mut out = vec![0xFF, 0xE1];
    out.extend(((2 + 6 + tiff.len()) as u16).to_be_bytes());
    out.extend(b"Exif\0\0");
    out.extend(tiff);
    out
}

/// The JPEG with its orientation set to `value`: changed where it is, added to the EXIF block
/// it has, or in a new EXIF block. None when the file isn't laid out as expected.
pub fn with_orientation(jpeg: &[u8], value: u16) -> Option<Vec<u8>> {
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return None;
    }
    // Walk the segments before the picture data, looking for EXIF.
    let mut i = 2;
    let mut after_app0 = 2;
    while i + 4 <= jpeg.len() && jpeg[i] == 0xFF {
        let marker = jpeg[i + 1];
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        if len < 2 || i + 2 + len > jpeg.len() {
            return None;
        }
        let body = &jpeg[i + 4..i + 2 + len];
        if marker == 0xE0 && after_app0 == 2 {
            after_app0 = i + 2 + len;
        }
        if marker == 0xE1 && body.starts_with(b"Exif\0\0") {
            let tiff = patched(&body[6..], value)?;
            if tiff.len() + 8 > 0xFFFF {
                return None;
            }
            let mut out = jpeg[..i].to_vec();
            out.extend(app1(&tiff));
            out.extend(&jpeg[i + 2 + len..]);
            return Some(out);
        }
        i += 2 + len;
    }
    // No EXIF yet: a block with only the orientation, after the JFIF header if there is one.
    let mut out = jpeg[..after_app0].to_vec();
    out.extend(app1(&tiff(vec![(0x0112, V::Short(value))], Vec::new(), Vec::new())));
    out.extend(&jpeg[after_app0..]);
    Some(out)
}

/// An EXIF block with its orientation set: in place when it has one, else with its main
/// fields written again at its end, the orientation among them. Everything else stays where it
/// was, so every offset in it still points at the same thing.
fn patched(tiff: &[u8], value: u16) -> Option<Vec<u8>> {
    let big = match tiff.get(0..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |b: &[u8], at: usize| -> Option<u16> {
        let s: [u8; 2] = b.get(at..at + 2)?.try_into().ok()?;
        Some(if big { u16::from_be_bytes(s) } else { u16::from_le_bytes(s) })
    };
    let u32_at = |b: &[u8], at: usize| -> Option<u32> {
        let s: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
        Some(if big { u32::from_be_bytes(s) } else { u32::from_le_bytes(s) })
    };
    let w16 = |v: u16| if big { v.to_be_bytes() } else { v.to_le_bytes() };
    let w32 = |v: u32| if big { v.to_be_bytes() } else { v.to_le_bytes() };
    let ifd = u32_at(tiff, 4)? as usize;
    let n = u16_at(tiff, ifd)? as usize;
    let entries = ifd + 2;
    tiff.get(entries..entries + 12 * n + 4)?;
    let mut out = tiff.to_vec();
    for k in 0..n {
        let e = entries + 12 * k;
        if u16_at(tiff, e)? == 0x0112 {
            // A SHORT, kept in the field itself.
            if u16_at(tiff, e + 2)? != 3 {
                return None;
            }
            out[e + 8..e + 10].copy_from_slice(&w16(value));
            return Some(out);
        }
    }
    let mut list: Vec<[u8; 12]> = (0..n).map(|k| tiff[entries + 12 * k..entries + 12 * k + 12].try_into().unwrap()).collect();
    let mut field = [0u8; 12];
    field[0..2].copy_from_slice(&w16(0x0112));
    field[2..4].copy_from_slice(&w16(3));
    field[4..8].copy_from_slice(&w32(1));
    field[8..10].copy_from_slice(&w16(value));
    list.push(field);
    list.sort_by_key(|f| u16_at(f, 0).unwrap_or(0));
    let next = u32_at(tiff, entries + 12 * n)?;
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let at = out.len() as u32;
    out.extend(w16(list.len() as u16));
    for f in list {
        out.extend(f);
    }
    out.extend(w32(next));
    out[4..8].copy_from_slice(&w32(at));
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg() -> Vec<u8> {
        let img = image::RgbImage::from_fn(8, 4, |x, _| image::Rgb([x as u8 * 30, 0, 0]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }

    fn orientation(bytes: &[u8]) -> u32 {
        let ex = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)).unwrap();
        ex.get_field(exif::Tag::Orientation, exif::In::PRIMARY).unwrap().value.get_uint(0).unwrap()
    }

    #[test]
    fn a_jpeg_without_exif_gets_one() {
        let out = with_orientation(&jpeg(), 6).unwrap();
        assert_eq!(orientation(&out), 6);
        // The picture data is the same bytes.
        assert!(out.ends_with(&jpeg()[20..]));
        assert!(image::load_from_memory(&out).is_ok());
    }

    #[test]
    fn an_orientation_already_there_is_changed_in_place() {
        let once = with_orientation(&jpeg(), 6).unwrap();
        let twice = with_orientation(&once, 3).unwrap();
        assert_eq!(orientation(&twice), 3);
        assert_eq!(once.len(), twice.len());
    }

    #[test]
    fn exif_without_an_orientation_keeps_its_other_fields() {
        let block = tiff(vec![(0x010F, V::Ascii("Rivet".into())), (0x0110, V::Ascii("Model R1".into()))], vec![(0x829A, V::Rational(vec![(1, 250)]))], Vec::new());
        let j = jpeg();
        let mut with = j[..2].to_vec();
        with.extend(app1(&block));
        with.extend(&j[2..]);
        let out = with_orientation(&with, 8).unwrap();
        assert_eq!(orientation(&out), 8);
        let ex = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&out)).unwrap();
        assert_eq!(ex.get_field(exif::Tag::Model, exif::In::PRIMARY).unwrap().display_value().to_string(), "\"Model R1\"");
        assert!(ex.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY).is_some());
    }

    #[test]
    fn a_png_is_written_again_turned() {
        let dir = std::env::temp_dir().join(format!("rivet-view-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("wide.png");
        image::RgbaImage::new(6, 2).save(&p).unwrap();
        save(&p, Orient::NONE, Orient::NONE.turned_right(), false).unwrap();
        assert_eq!(image::image_dimensions(&p).unwrap(), (2, 6));
        assert_eq!(how(&dir.join("x.gif"), false), How::No);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

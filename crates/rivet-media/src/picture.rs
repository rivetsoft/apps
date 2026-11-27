//! Pictures at full size for a viewer: every frame of an animation, the way the camera held
//! it (its orientation), and what the camera wrote down (EXIF). Slow: call `decode` from a
//! thread.
//!
//! PNG, JPEG, WebP, GIF, TIFF, BMP, ICO, TGA, QOI and the PNM family are read here. SVG is
//! drawn with resvg. HEIC, AVIF and JPEG XL are handed to a program the computer already has
//! (libheif's `heif-dec`, ImageMagick or ffmpeg), so nothing under another license is built in.
//! Camera raw files show the full-size JPEG the camera keeps inside them.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use eframe::egui;
use image::{AnimationDecoder, DynamicImage, ImageDecoder};

/// Files bigger than this aren't opened (a picture this big is almost certainly something else).
const MAX_FILE: u64 = 1 << 30;
/// An animation's frames stop being kept past this many bytes; the rest of it is left out.
const MAX_ANIMATION_BYTES: usize = 768 << 20;

/// The endings of the files a viewer shows, in lowercase.
pub const EXTENSIONS: &[&str] = &[
    "png", "apng", "jpg", "jpeg", "jpe", "jfif", "webp", "gif", "tif", "tiff", "bmp", "dib", "ico", "tga", "qoi", "pnm", "pbm", "pgm", "ppm", "pam", "svg", "svgz", "heic", "heif", "hif", "avif", "jxl",
    // Camera raw files.
    "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2", "raf", "pef", "srw", "rwl", "3fr", "erf", "kdc", "mrw", "x3f", "iiq",
];

/// The kinds of file a viewer opens, as the desktop names them (for a `.desktop` file's
/// `MimeType=`, and for asking which app opens pictures).
pub const MIME_TYPES: &[&str] = &[
    "image/png", "image/apng", "image/vnd.mozilla.apng", "image/jpeg", "image/pjpeg", "image/webp", "image/gif", "image/tiff", "image/bmp", "image/x-bmp", "image/x-ms-bmp", "image/x-icon", "image/vnd.microsoft.icon", "image/x-tga", "image/x-targa", "image/qoi", "image/x-portable-anymap", "image/x-portable-bitmap", "image/x-portable-graymap", "image/x-portable-pixmap", "image/svg+xml", "image/svg+xml-compressed", "image/heic", "image/heif", "image/heic-sequence", "image/avif", "image/jxl", "image/x-adobe-dng", "image/x-canon-cr2", "image/x-canon-cr3", "image/x-canon-crw", "image/x-nikon-nef", "image/x-nikon-nrw", "image/x-sony-arw", "image/x-sony-srf", "image/x-sony-sr2", "image/x-olympus-orf", "image/x-panasonic-rw2", "image/x-fuji-raf", "image/x-pentax-pef", "image/x-samsung-srw", "image/x-dcraw",
];

/// Whether this file is something a viewer shows, from its name.
pub fn is_picture(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn ext_of(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// How a picture is turned and mirrored: first turned clockwise a quarter at a time, then
/// mirrored left to right. Every combination of turns and flips comes out as one of these
/// eight, and so does each orientation a camera can write in EXIF.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Orient {
    pub turns: u8,
    pub mirror: bool,
}

impl Orient {
    pub const NONE: Orient = Orient { turns: 0, mirror: false };

    /// From an EXIF orientation (1 to 8; anything else is upright).
    pub fn from_exif(v: u16) -> Orient {
        let (turns, mirror) = match v {
            2 => (0, true),
            3 => (2, false),
            4 => (2, true),
            5 => (1, true),
            6 => (1, false),
            7 => (3, true),
            8 => (3, false),
            _ => (0, false),
        };
        Orient { turns, mirror }
    }

    pub fn exif(self) -> u16 {
        match (self.turns % 4, self.mirror) {
            (0, false) => 1,
            (0, true) => 2,
            (2, false) => 3,
            (2, true) => 4,
            (1, true) => 5,
            (1, false) => 6,
            (3, true) => 7,
            _ => 8,
        }
    }

    /// This, and then `next`.
    pub fn then(self, next: Orient) -> Orient {
        // A mirror before a turn is the same as the turn the other way before the mirror.
        if self.mirror {
            Orient { turns: (self.turns + 4 - next.turns % 4) % 4, mirror: !next.mirror }
        } else {
            Orient { turns: (self.turns + next.turns) % 4, mirror: next.mirror }
        }
    }

    pub fn turned_right(self) -> Orient {
        self.then(Orient { turns: 1, mirror: false })
    }

    pub fn turned_left(self) -> Orient {
        self.then(Orient { turns: 3, mirror: false })
    }

    pub fn flipped_across(self) -> Orient {
        self.then(Orient { turns: 0, mirror: true })
    }

    pub fn flipped_down(self) -> Orient {
        self.then(Orient { turns: 2, mirror: true })
    }

    /// Whether width and height trade places.
    pub fn swaps(self) -> bool {
        self.turns % 2 == 1
    }

    pub fn is_none(self) -> bool {
        self.turns.is_multiple_of(4) && !self.mirror
    }

    /// The pixels turned and mirrored this way.
    pub fn apply(self, img: DynamicImage) -> DynamicImage {
        let img = match self.turns % 4 {
            1 => img.rotate90(),
            2 => img.rotate180(),
            3 => img.rotate270(),
            _ => img,
        };
        if self.mirror { img.fliph() } else { img }
    }
}

/// One frame, and how long it stays up (zero for a still picture).
pub struct Frame {
    pub image: egui::ColorImage,
    pub delay: Duration,
}

/// A picture ready to show.
pub struct Picture {
    pub frames: Vec<Frame>,
    /// The picture's own size in pixels, upright (the frames may be smaller, to fit what the
    /// graphics card holds).
    pub width: u32,
    pub height: u32,
    /// "JPEG", "PNG"…
    pub format: String,
    /// The orientation the file says it has, already applied to the frames.
    pub orient: Orient,
    /// "RGB, 8-bit" and the like, when known.
    pub color: Option<String>,
    pub alpha: bool,
    /// A drawing (SVG): fitting may enlarge it, since it stays sharp.
    pub vector: bool,
    /// What the camera wrote down, as label and value, in the order a person reads them.
    pub exif: Vec<(String, String)>,
    /// Said under the picture when it's shown smaller than it is, or with something left out.
    pub note: Option<String>,
}

impl Picture {
    /// Bytes its frames take.
    pub fn bytes(&self) -> usize {
        self.frames.iter().map(|f| f.image.pixels.len() * 4).sum()
    }

    pub fn animated(&self) -> bool {
        self.frames.len() > 1
    }
}

/// Read and decode a picture, every frame no bigger than `max_side` on its longest edge.
pub fn decode(path: &Path, max_side: u32) -> Result<Picture, String> {
    let max_side = max_side.max(64);
    let size = std::fs::metadata(path).map_err(|e| why_unreadable(&e))?.len();
    if size > MAX_FILE {
        return Err("This file is too big to open as a picture.".into());
    }
    let bytes = std::fs::read(path).map_err(|e| why_unreadable(&e))?;
    let ext = ext_of(path);
    let exif = read_exif(&bytes);
    let mut pic = match ext.as_str() {
        "svg" | "svgz" => svg(&bytes, max_side)?,
        "heic" | "heif" | "hif" | "avif" | "jxl" => outside(path, &ext, max_side)?,
        e if is_raw(e) => raw(&bytes, max_side)?,
        _ => match image::guess_format(&bytes) {
            Ok(image::ImageFormat::Gif) => animation(Kind::Gif, &bytes, max_side)?,
            Ok(image::ImageFormat::WebP) => animation(Kind::WebP, &bytes, max_side)?,
            Ok(image::ImageFormat::Png) => animation(Kind::Png, &bytes, max_side)?,
            Ok(image::ImageFormat::Avif) => outside(path, "avif", max_side)?,
            _ => still(&bytes, max_side)?,
        },
    };
    if let Some(ex) = &exif {
        pic.exif = camera_facts(ex);
    }
    Ok(pic)
}

fn why_unreadable(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "This picture isn't there any more.".into(),
        std::io::ErrorKind::PermissionDenied => "You don't have permission to open this picture.".into(),
        _ => format!("This picture couldn't be read ({e})."),
    }
}

fn color_of(img: &DynamicImage) -> (Option<String>, bool) {
    use image::ColorType as C;
    let (model, bits) = match img.color() {
        C::L8 => ("Grayscale", 8),
        C::La8 => ("Grayscale with transparency", 8),
        C::Rgb8 => ("RGB", 8),
        C::Rgba8 => ("RGB with transparency", 8),
        C::L16 => ("Grayscale", 16),
        C::La16 => ("Grayscale with transparency", 16),
        C::Rgb16 => ("RGB", 16),
        C::Rgba16 => ("RGB with transparency", 16),
        C::Rgb32F => ("RGB", 32),
        C::Rgba32F => ("RGB with transparency", 32),
        _ => return (None, img.color().has_alpha()),
    };
    (Some(format!("{model}, {bits}-bit")), img.color().has_alpha())
}

/// The picture as egui pixels, shrunk to `max_side` when it is bigger.
fn to_color_image(img: DynamicImage, max_side: u32) -> egui::ColorImage {
    let img = if img.width() > max_side || img.height() > max_side { img.thumbnail(max_side, max_side) } else { img };
    let rgba = img.into_rgba8();
    egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw())
}

fn shrunk_note(w: u32, h: u32, max_side: u32) -> Option<String> {
    (w > max_side || h > max_side).then(|| "Shown smaller than it is: it's bigger than the graphics card can show at once.".to_string())
}

fn format_name(f: image::ImageFormat) -> String {
    use image::ImageFormat as F;
    match f {
        F::Png => "PNG",
        F::Jpeg => "JPEG",
        F::Gif => "GIF",
        F::WebP => "WebP",
        F::Pnm => "PNM",
        F::Tiff => "TIFF",
        F::Tga => "TGA",
        F::Bmp => "BMP",
        F::Ico => "ICO",
        F::Qoi => "QOI",
        F::Avif => "AVIF",
        _ => "Picture",
    }
    .to_string()
}

/// A picture with one frame, turned the way its file says.
fn still(bytes: &[u8], max_side: u32) -> Result<Picture, String> {
    let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let format = reader.format().map(format_name).ok_or("This kind of picture can't be opened.")?;
    let mut dec = reader.into_decoder().map_err(|e| format!("This picture couldn't be read ({e})."))?;
    let orient = dec.orientation().map(|o| Orient::from_exif(o.to_exif() as u16)).unwrap_or_default();
    let img = DynamicImage::from_decoder(dec).map_err(|e| format!("This picture is damaged or incomplete ({e})."))?;
    let img = orient.apply(img);
    Ok(finish(img, format, orient, max_side))
}

fn finish(img: DynamicImage, format: String, orient: Orient, max_side: u32) -> Picture {
    let (w, h) = (img.width(), img.height());
    let (color, alpha) = color_of(&img);
    let image = to_color_image(img, max_side);
    Picture { frames: vec![Frame { image, delay: Duration::ZERO }], width: w, height: h, format, orient, color, alpha, vector: false, exif: Vec::new(), note: shrunk_note(w, h, max_side) }
}

enum Kind {
    Gif,
    WebP,
    Png,
}

/// GIF, WebP and PNG: every frame when it moves, else the one picture.
fn animation(kind: Kind, bytes: &[u8], max_side: u32) -> Result<Picture, String> {
    let frames = match kind {
        Kind::Gif => image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).ok().map(|d| (d.into_frames(), "GIF")),
        Kind::WebP => match image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)) {
            Ok(d) if d.has_animation() => Some((d.into_frames(), "WebP")),
            _ => None,
        },
        Kind::Png => match image::codecs::png::PngDecoder::new(Cursor::new(bytes)) {
            Ok(d) if d.is_apng().unwrap_or(false) => d.apng().ok().map(|a| (a.into_frames(), "APNG")),
            _ => None,
        },
    };
    let Some((frames, format)) = frames else { return still(bytes, max_side) };
    let mut out = Vec::new();
    let (mut total, mut cut) = (0usize, false);
    let mut size = (0, 0);
    for f in frames {
        let Ok(f) = f else { break };
        let (n, d) = f.delay().numer_denom_ms();
        let ms = n.checked_div(d).map_or(100, |ms| ms.max(20));
        let buf = f.into_buffer();
        size = (buf.width(), buf.height());
        let image = to_color_image(DynamicImage::ImageRgba8(buf), max_side);
        total += image.pixels.len() * 4;
        if total > MAX_ANIMATION_BYTES && !out.is_empty() {
            cut = true;
            break;
        }
        out.push(Frame { image, delay: Duration::from_millis(ms as u64) });
    }
    if out.len() <= 1 {
        return still(bytes, max_side);
    }
    let note = if cut { Some(format!("Only the first {} frames are shown: the animation is too long to keep in memory.", out.len())) } else { shrunk_note(size.0, size.1, max_side) };
    Ok(Picture { frames: out, width: size.0, height: size.1, format: format.into(), orient: Orient::NONE, color: Some("RGB with transparency, 8-bit".into()), alpha: true, vector: false, exif: Vec::new(), note })
}

/// A drawing, at a size that stays sharp up close: twice its own, or at least 2048 pixels
/// across, within `max_side`.
fn svg(bytes: &[u8], max_side: u32) -> Result<Picture, String> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(bytes, &opt).map_err(|e| format!("This drawing couldn't be read ({e})."))?;
    let size = tree.size();
    let (w, h) = (size.width().max(1.0), size.height().max(1.0));
    let long = w.max(h);
    let target = (long * 2.0).max(2048.0).min(max_side.min(4096) as f32);
    let scale = target / long;
    let (pw, ph) = ((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph).ok_or("This drawing is too big to show.")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let image = egui::ColorImage::from_rgba_premultiplied([pw as usize, ph as usize], pixmap.data());
    Ok(Picture {
        frames: vec![Frame { image, delay: Duration::ZERO }],
        width: w.round() as u32,
        height: h.round() as u32,
        format: "SVG drawing".into(),
        orient: Orient::NONE,
        color: None,
        alpha: true,
        vector: true,
        exif: Vec::new(),
        note: None,
    })
}

fn has(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(bin)).find(|c| c.is_file()))
}

/// HEIC, AVIF and JPEG XL, through a program that reads them, as a PNG.
fn outside(path: &Path, ext: &str, max_side: u32) -> Result<Picture, String> {
    let format = match ext {
        "avif" => "AVIF",
        "jxl" => "JPEG XL",
        _ => "HEIC",
    };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("rivet-picture-{}-{stamp}.png", std::process::id()));
    let to_file = |tool: PathBuf| -> Option<Vec<u8>> {
        let ok = Command::new(tool).arg(path).arg(&tmp).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        let png = if ok { std::fs::read(&tmp).ok() } else { None };
        let _ = std::fs::remove_file(&tmp);
        png
    };
    let piped = |mut c: Command| -> Option<Vec<u8>> {
        let out = c.stdin(Stdio::null()).stderr(Stdio::null()).stdout(Stdio::piped()).output().ok()?;
        (out.status.success() && !out.stdout.is_empty()).then_some(out.stdout)
    };
    let mut png: Option<Vec<u8>> = None;
    // libheif's and libjxl's own tools write a file.
    let own: &[&str] = if ext == "jxl" { &["djxl"] } else { &["heif-dec", "heif-convert"] };
    for tool in own {
        if png.is_none() && let Some(t) = has(tool) {
            png = to_file(t);
        }
    }
    for magick in ["magick", "convert"] {
        if png.is_none() && let Some(t) = has(magick) {
            let mut c = Command::new(t);
            c.arg(format!("{}[0]", path.display())).arg("png:-");
            png = piped(c);
        }
    }
    if png.is_none() && let Some(t) = has("ffmpeg") {
        let mut c = Command::new(t);
        c.args(["-v", "error", "-i"]).arg(path).args(["-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-"]);
        png = piped(c);
    }
    let Some(png) = png else {
        return Err(format!("Install libheif's tools, ImageMagick or ffmpeg to open {format} pictures."));
    };
    let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png).map_err(|_| format!("This {format} picture couldn't be read."))?;
    Ok(finish(img, format.into(), Orient::NONE, max_side))
}

fn is_raw(ext: &str) -> bool {
    matches!(ext, "dng" | "cr2" | "cr3" | "crw" | "nef" | "nrw" | "arw" | "srf" | "sr2" | "orf" | "rw2" | "raf" | "pef" | "srw" | "rwl" | "3fr" | "erf" | "kdc" | "mrw" | "x3f" | "iiq")
}

/// A camera raw file, by the biggest JPEG the camera put inside it (most keep one at full
/// size, for their own screen). The raw pixels themselves aren't developed.
fn raw(bytes: &[u8], max_side: u32) -> Result<Picture, String> {
    let mut best: Option<(u64, usize)> = None;
    let mut looked = 0;
    let mut i = 0;
    while i + 3 < bytes.len() && looked < 64 {
        if bytes[i] == 0xFF && bytes[i + 1] == 0xD8 && bytes[i + 2] == 0xFF {
            looked += 1;
            let r = image::ImageReader::with_format(Cursor::new(&bytes[i..]), image::ImageFormat::Jpeg);
            if let Ok((w, h)) = r.into_dimensions() {
                let area = w as u64 * h as u64;
                if best.is_none_or(|(a, _)| area > a) {
                    best = Some((area, i));
                }
            }
        }
        i += 1;
    }
    let Some((_, at)) = best else { return Err("This raw file has no picture inside that can be shown.".into()) };
    let orient = read_exif(bytes).and_then(|e| e.get_field(exif::Tag::Orientation, exif::In::PRIMARY).and_then(|f| f.value.get_uint(0))).map(|v| Orient::from_exif(v as u16)).unwrap_or_default();
    let img = image::load_from_memory_with_format(&bytes[at..], image::ImageFormat::Jpeg).map_err(|_| "The picture inside this raw file couldn't be read.".to_string())?;
    let mut p = finish(orient.apply(img), "Camera raw (its preview)".into(), orient, max_side);
    p.color = None;
    Ok(p)
}

fn read_exif(bytes: &[u8]) -> Option<exif::Exif> {
    exif::Reader::new().read_from_container(&mut Cursor::new(bytes)).ok()
}

/// Text from an EXIF field, without the quotes and padding cameras leave in.
fn text(ex: &exif::Exif, tag: exif::Tag) -> Option<String> {
    let f = ex.get_field(tag, exif::In::PRIMARY)?;
    let s = match &f.value {
        exif::Value::Ascii(v) => v.iter().map(|b| String::from_utf8_lossy(b).to_string()).collect::<Vec<_>>().join(" "),
        _ => f.display_value().with_unit(ex).to_string(),
    };
    let s = s.trim_matches(|c: char| c == '"' || c.is_whitespace() || c == '\0').to_string();
    (!s.is_empty()).then_some(s)
}

fn rational(ex: &exif::Exif, tag: exif::Tag) -> Option<f64> {
    match &ex.get_field(tag, exif::In::PRIMARY)?.value {
        exif::Value::Rational(v) => v.first().map(|r| r.to_f64()),
        exif::Value::SRational(v) => v.first().map(|r| r.to_f64()),
        _ => None,
    }
}

/// "51.50070° N"
fn coordinate(ex: &exif::Exif, tag: exif::Tag, side: exif::Tag) -> Option<String> {
    let exif::Value::Rational(v) = &ex.get_field(tag, exif::In::PRIMARY)?.value else { return None };
    if v.len() < 3 {
        return None;
    }
    let deg = v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.0;
    Some(format!("{deg:.5}° {}", text(ex, side).unwrap_or_default()).trim().to_string())
}

/// What a person wants to know about how the picture was taken.
fn camera_facts(ex: &exif::Exif) -> Vec<(String, String)> {
    use exif::Tag as T;
    let make = text(ex, T::Make);
    let model = text(ex, T::Model);
    let camera = match (make, model) {
        (Some(m), Some(md)) if md.to_lowercase().starts_with(&m.to_lowercase()) => Some(md),
        (Some(m), Some(md)) => Some(format!("{m} {md}")),
        (m, md) => m.or(md),
    };
    let focal = rational(ex, T::FocalLength).map(|f| format!("{} mm", trim_num(f)));
    let film = ex.get_field(T::FocalLengthIn35mmFilm, exif::In::PRIMARY).and_then(|f| f.value.get_uint(0)).filter(|v| *v > 0);
    let location = match (coordinate(ex, T::GPSLatitude, T::GPSLatitudeRef), coordinate(ex, T::GPSLongitude, T::GPSLongitudeRef)) {
        (Some(lat), Some(lon)) => Some(format!("{lat}, {lon}")),
        _ => None,
    };
    let rows = [
        ("Camera", camera),
        ("Lens", text(ex, T::LensModel)),
        ("Taken", text(ex, T::DateTimeOriginal).or_else(|| text(ex, T::DateTime)).map(|d| tidy_date(&d))),
        ("Exposure", rational(ex, T::ExposureTime).map(|t| if t > 0.0 && t < 0.5 { format!("1/{} s", (1.0 / t).round()) } else { format!("{} s", trim_num(t)) })),
        ("Aperture", rational(ex, T::FNumber).map(|f| format!("f/{}", trim_num(f)))),
        ("ISO", text(ex, T::PhotographicSensitivity)),
        (
            "Focal length",
            match (focal, film) {
                (Some(f), Some(e)) => Some(format!("{f} ({e} mm full-frame)")),
                (f, _) => f,
            },
        ),
        ("Flash", ex.get_field(T::Flash, exif::In::PRIMARY).and_then(|f| f.value.get_uint(0)).map(|v| if v & 1 == 1 { "Fired".to_string() } else { "Didn't fire".to_string() })),
        ("Location", location),
        ("Software", text(ex, T::Software)),
        ("Artist", text(ex, T::Artist)),
        ("Copyright", text(ex, T::Copyright)),
    ];
    rows.into_iter().filter_map(|(k, v)| Some((k.to_string(), v?))).collect()
}

/// 2.80 → "2.8", 35.0 → "35"
fn trim_num(f: f64) -> String {
    let s = format!("{f:.1}");
    s.strip_suffix(".0").map(String::from).unwrap_or(s)
}

/// "2026:05:14 18:42:07" → "2026-05-14 18:42"
fn tidy_date(d: &str) -> String {
    let d = d.trim();
    if d.len() >= 16 && d.as_bytes()[4] == b':' && d.as_bytes()[7] == b':' {
        format!("{}-{}-{} {}", &d[0..4], &d[5..7], &d[8..10], &d[11..16])
    } else {
        d.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_exif_orientation_comes_back() {
        for v in 1..=8 {
            assert_eq!(Orient::from_exif(v).exif(), v);
        }
    }

    #[test]
    fn turns_and_flips_add_up() {
        let o = Orient::NONE;
        assert_eq!(o.turned_right().turned_right().turned_right().turned_right(), o);
        assert_eq!(o.turned_right().turned_left(), o);
        assert_eq!(o.flipped_across().flipped_across(), o);
        assert_eq!(o.flipped_down().flipped_down(), o);
        // Mirrored, then turned right, is a turn left before the mirror.
        assert_eq!(o.flipped_across().turned_right(), Orient { turns: 3, mirror: true });
        // A flip down is a flip across and a half turn.
        assert_eq!(o.flipped_down(), o.flipped_across().turned_right().turned_right());
        // A camera held sideways (6), turned back left, is upright.
        assert_eq!(Orient::from_exif(6).turned_left(), o);
    }

    /// Turning the pixels by a combined orientation matches turning them step by step.
    #[test]
    fn pixels_turn_as_described() {
        let mut img = image::RgbaImage::new(3, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.put_pixel(2, 1, image::Rgba([0, 0, 255, 255]));
        let img = DynamicImage::ImageRgba8(img);
        let mut o = Orient::NONE;
        let mut by_steps = img.clone();
        for step in 0..6 {
            (o, by_steps) = match step % 3 {
                0 => (o.flipped_across(), by_steps.fliph()),
                1 => (o.turned_right(), by_steps.rotate90()),
                _ => (o.flipped_down(), by_steps.flipv()),
            };
            assert_eq!(o.apply(img.clone()).to_rgba8(), by_steps.to_rgba8(), "step {step}");
        }
    }

    #[test]
    fn dates_and_numbers_read_plainly() {
        assert_eq!(tidy_date("2026:05:14 18:42:07"), "2026-05-14 18:42");
        assert_eq!(trim_num(2.8), "2.8");
        assert_eq!(trim_num(35.0), "35");
    }
}

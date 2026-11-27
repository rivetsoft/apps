//! PDFs, through PDFium. PDFium is loaded when the first PDF opens, from the first of: the file
//! RIVET_PDFIUM names, a copy beside the program (lib/rivet-docs/libpdfium.so next to or above
//! its folder, where the release archive, the AppImage and `rivet-dev pdfium` put it), or the
//! system's own. Everything here runs on the worker thread.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};

use pdfium_render::prelude::*;

use crate::{Area, Bitmap, Document, Info, Kind, Link, OpenError, OutlineItem, PageText, Target};

const LIB: &str = "libpdfium.so";

/// Where PDFium would come from, without loading it.
#[derive(Clone, Debug, PartialEq)]
pub enum PdfiumStatus {
    /// A copy shipped with the program (or named by RIVET_PDFIUM).
    Bundled(PathBuf),
    /// The system's, found in one of its library folders.
    System(PathBuf),
    Missing,
}

/// The copies beside the program, in the order they're tried.
fn bundled() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = std::env::var_os("RIVET_PDFIUM").map(PathBuf::from) {
        out.push(p);
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        out.push(dir.join("../lib/rivet-docs").join(LIB));
        out.push(dir.join("lib/rivet-docs").join(LIB));
    }
    out
}

/// Where PDFium is, as the Properties window and the "can't open PDFs" message say it.
pub fn pdfium_status() -> PdfiumStatus {
    if let Some(p) = bundled().into_iter().find(|p| p.is_file()) {
        return PdfiumStatus::Bundled(p);
    }
    let mut dirs: Vec<PathBuf> = std::env::var("LD_LIBRARY_PATH").unwrap_or_default().split(':').filter(|d| !d.is_empty()).map(PathBuf::from).collect();
    dirs.extend(["/usr/lib64", "/usr/lib", "/usr/local/lib", "/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu", "/usr/lib/pdfium"].map(PathBuf::from));
    match dirs.into_iter().map(|d| d.join(LIB)).find(|p| p.is_file()) {
        Some(p) => PdfiumStatus::System(p),
        None => PdfiumStatus::Missing,
    }
}

thread_local! {
    /// PDFium, loaded once on the thread that uses it, and kept for the life of the program.
    static PDFIUM: OnceCell<Result<&'static Pdfium, String>> = const { OnceCell::new() };
}

fn pdfium() -> Result<&'static Pdfium, String> {
    PDFIUM.with(|cell| {
        cell.get_or_init(|| {
            let mut tried = Vec::new();
            for p in bundled().into_iter().filter(|p| p.is_file()) {
                match Pdfium::bind_to_library(&p) {
                    Ok(b) => return Ok(&*Box::leak(Box::new(Pdfium::new(b)))),
                    Err(e) => tried.push(format!("{}: {e:?}", p.display())),
                }
            }
            match Pdfium::bind_to_system_library() {
                Ok(b) => Ok(&*Box::leak(Box::new(Pdfium::new(b)))),
                Err(e) => {
                    tried.push(format!("system: {e:?}"));
                    Err(tried.join("; "))
                }
            }
        })
        .clone()
    })
}

pub struct Pdf {
    doc: PdfDocument<'static>,
    sizes: Vec<[f32; 2]>,
}

pub fn open(path: &Path, password: Option<&str>) -> Result<Pdf, OpenError> {
    let pdfium = pdfium().map_err(OpenError::NoPdfium)?;
    let doc = pdfium.load_pdf_from_file(path, password).map_err(|e| match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => OpenError::NeedsPassword { wrong: password.is_some() },
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => OpenError::Failed("It isn't a PDF, or it is damaged.".into()),
        other => OpenError::Failed(format!("PDFium couldn't read it ({other:?}).")),
    })?;
    let sizes = doc.pages().page_sizes().map(|v| v.iter().map(|r| [r.width().value, r.height().value]).collect()).unwrap_or_default();
    Ok(Pdf { doc, sizes })
}

impl Pdf {
    fn page_height(&self, page: usize) -> f32 {
        self.sizes.get(page).map_or(792.0, |s| s[1])
    }

    /// A rectangle from PDFium's corner (bottom left, y up) to ours (top left, y down).
    fn area(&self, page: usize, r: &PdfRect) -> Area {
        let h = self.page_height(page);
        [r.left().value, h - r.top().value, r.right().value, h - r.bottom().value]
    }

    fn outline_from(&self, first: Option<PdfBookmark<'_>>, depth: usize) -> Vec<OutlineItem> {
        let mut out = Vec::new();
        let mut at = first;
        // A broken outline can loop round on itself: stop at a sensible size.
        while let Some(b) = at {
            if out.len() > 5000 || depth > 32 {
                break;
            }
            let page = b.destination().and_then(|d| d.page_index().ok()).or_else(|| {
                b.action().and_then(|a| a.as_local_destination_action().and_then(|l| l.destination().ok()).and_then(|d| d.page_index().ok()))
            });
            out.push(OutlineItem {
                title: b.title().unwrap_or_default().trim().to_string(),
                page: page.map(|p| p as usize),
                children: self.outline_from(b.first_child(), depth + 1),
            });
            at = b.next_sibling();
        }
        out
    }
}

impl Document for Pdf {
    fn kind(&self) -> Kind {
        Kind::Pdf
    }

    fn page_count(&self) -> usize {
        self.doc.pages().len() as usize
    }

    fn page_sizes(&mut self) -> Vec<[f32; 2]> {
        self.sizes.clone()
    }

    fn render(&mut self, page: usize, width: u32, height: u32, clip: Option<[u32; 4]>) -> Result<Bitmap, String> {
        let p = self.doc.pages().get(page as PdfPageIndex).map_err(|e| format!("{e:?}"))?;
        let [x0, y0, x1, y1] = clip.unwrap_or([0, 0, width, height]);
        let (w, h) = (x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1));
        let mut bitmap = PdfBitmap::empty(w as Pixels, h as Pixels, PdfBitmapFormat::BGRA).map_err(|e| format!("{e:?}"))?;
        let config = PdfRenderConfig::new()
            .set_target_size(width as Pixels, height as Pixels)
            .set_origin(-(x0 as Pixels), -(y0 as Pixels))
            .set_clear_color(PdfColor::WHITE)
            .render_form_data(true)
            .render_annotations(true)
            .set_reverse_byte_order(true);
        p.render_into_bitmap_with_config(&mut bitmap, &config).map_err(|e| format!("{e:?}"))?;
        let mut rgba = bitmap.as_rgba_bytes();
        // PDFium's rows can be longer than the picture; egui wants them exact.
        let stride = rgba.len() / h as usize;
        if stride != w as usize * 4 {
            rgba = rgba.chunks(stride).flat_map(|row| row[..w as usize * 4].to_vec()).collect();
        }
        for px in rgba.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        Ok(Bitmap { width: w, height: h, rgba })
    }

    fn outline(&mut self) -> Vec<OutlineItem> {
        let first = self.doc.bookmarks().root();
        self.outline_from(first, 0)
    }

    fn text(&mut self, page: usize) -> Option<PageText> {
        let p = self.doc.pages().get(page as PdfPageIndex).ok()?;
        let text = p.text().ok()?;
        let mut out = PageText::default();
        let mut last: Area = [0.0; 4];
        for ch in text.chars().iter() {
            let Some(c) = ch.unicode_char() else { continue };
            let generated = ch.is_generated().unwrap_or(false);
            let b = match ch.loose_bounds() {
                Ok(r) if !generated => self.area(page, &r),
                _ => [last[2], last[1], last[2], last[1]],
            };
            if b[2] > b[0] {
                last = b;
            }
            out.chars.push(c);
            out.boxes.push(b);
        }
        Some(out)
    }

    fn links(&mut self, page: usize) -> Vec<Link> {
        let Ok(p) = self.doc.pages().get(page as PdfPageIndex) else { return Vec::new() };
        let mut out = Vec::new();
        for link in p.links().iter() {
            let Ok(rect) = link.rect() else { continue };
            let area = self.area(page, &rect);
            // Where a destination goes: its page, and how far down it when it says.
            let place = |d: &PdfDestination| -> Option<(usize, Option<f32>)> {
                let i = d.page_index().ok()? as usize;
                let y = match d.view_settings() {
                    Ok(PdfDestinationViewSettings::SpecificCoordinatesAndZoom(_, Some(y), _)) => Some((self.page_height(i) - y.value).max(0.0)),
                    _ => None,
                };
                Some((i, y))
            };
            let dest = link.destination().and_then(|d| place(&d)).or_else(|| link.action().and_then(|a| a.as_local_destination_action().and_then(|l| l.destination().ok().and_then(|d| place(&d)))));
            let to = if let Some((i, y)) = dest {
                Target::Page(i, y)
            } else if let Some(uri) = link.action().and_then(|a| a.as_uri_action().and_then(|u| u.uri().ok())) {
                Target::Url(uri)
            } else {
                continue;
            };
            out.push(Link { area, to });
        }
        out
    }

    fn pictures(&mut self, page: usize) -> Vec<Area> {
        let Ok(p) = self.doc.pages().get(page as PdfPageIndex) else { return Vec::new() };
        p.objects().iter().filter(|o| o.object_type() == PdfPageObjectType::Image).filter_map(|o| o.bounds().ok()).map(|q| self.area(page, &q.to_rect())).collect()
    }

    fn info(&mut self) -> Info {
        let m = self.doc.metadata();
        let tag = |t| m.get(t).map(|v| v.value().trim().to_string()).filter(|v| !v.is_empty());
        let version = match self.doc.version() {
            PdfDocumentVersion::Pdf1_0 => Some("1.0"),
            PdfDocumentVersion::Pdf1_1 => Some("1.1"),
            PdfDocumentVersion::Pdf1_2 => Some("1.2"),
            PdfDocumentVersion::Pdf1_3 => Some("1.3"),
            PdfDocumentVersion::Pdf1_4 => Some("1.4"),
            PdfDocumentVersion::Pdf1_5 => Some("1.5"),
            PdfDocumentVersion::Pdf1_6 => Some("1.6"),
            PdfDocumentVersion::Pdf1_7 => Some("1.7"),
            PdfDocumentVersion::Pdf2_0 => Some("2.0"),
            _ => None,
        };
        Info {
            title: tag(PdfDocumentMetadataTagType::Title),
            author: tag(PdfDocumentMetadataTagType::Author),
            subject: tag(PdfDocumentMetadataTagType::Subject),
            producer: tag(PdfDocumentMetadataTagType::Producer),
            version: version.map(String::from),
        }
    }
}

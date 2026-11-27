//! Comic book archives: a zip (.cbz) read here, or a rar or 7z (.cbr, .cb7) taken apart by the
//! system's archive tools into a folder of this program's own, removed when the comic closes.
//! The pages are the pictures inside, in the order their names count.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use image::RgbaImage;

use crate::picture::{decode, draw};
use crate::{Bitmap, Document, Kind, OpenError, PICTURE_ENDS, natural_cmp};

enum Source {
    Zip(zip::ZipArchive<File>),
    /// Taken apart into a folder of our own (the first), the pages under the second.
    Folder(PathBuf, PathBuf),
}

pub struct Comic {
    source: Source,
    /// The pages' names in the archive, in order.
    pages: Vec<String>,
    sizes: Vec<[f32; 2]>,
    /// The last few pages read, so zooming doesn't read a page again.
    decoded: VecDeque<(usize, RgbaImage)>,
}

fn is_picture(name: &str) -> bool {
    let lower = name.to_lowercase();
    !lower.starts_with("__macosx/") && !lower.rsplit('/').next().unwrap_or("").starts_with('.') && PICTURE_ENDS.iter().any(|e| lower.ends_with(&format!(".{e}")))
}

/// A picture's width and height from its first bytes.
fn dimensions(head: &[u8]) -> Option<[f32; 2]> {
    let r = image::ImageReader::new(std::io::Cursor::new(head)).with_guessed_format().ok()?;
    r.into_dimensions().ok().map(|(w, h)| [w as f32, h as f32])
}

pub fn open(path: &Path) -> Result<Comic, OpenError> {
    let zipped = File::open(path).ok().and_then(|f| zip::ZipArchive::new(f).ok());
    let mut comic = match zipped {
        Some(mut z) => {
            let mut pages: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|n| n.into_owned())).filter(|n| is_picture(n)).collect();
            pages.sort_by(|a, b| natural_cmp(a, b));
            let mut sizes = Vec::with_capacity(pages.len());
            for name in &pages {
                let mut head = Vec::new();
                if let Ok(f) = z.by_name(name) {
                    let _ = f.take(256 * 1024).read_to_end(&mut head);
                }
                sizes.push(dimensions(&head).unwrap_or([0.0, 0.0]));
            }
            Comic { source: Source::Zip(z), pages, sizes, decoded: VecDeque::new() }
        }
        None => {
            let dir = crate::scratch_dir("comic");
            std::fs::create_dir_all(&dir).map_err(|e| OpenError::Failed(format!("There's no room to open it ({e}).")))?;
            let made = rivet_archive::extract(path, &dir, &AtomicBool::new(false), &rivet_archive::free_name).map_err(|e| {
                let _ = std::fs::remove_dir_all(&dir);
                OpenError::Failed(format!("The pages couldn't be taken out ({e})."))
            })?;
            let made = if made.is_dir() { made } else { dir.clone() };
            let mut pages = Vec::new();
            walk(&made, &made, &mut pages);
            pages.sort_by(|a, b| natural_cmp(a, b));
            let sizes = pages
                .iter()
                .map(|p| {
                    let mut head = Vec::new();
                    let _ = File::open(made.join(p)).and_then(|f| f.take(256 * 1024).read_to_end(&mut head));
                    dimensions(&head).unwrap_or([0.0, 0.0])
                })
                .collect();
            Comic { source: Source::Folder(dir, made), pages, sizes, decoded: VecDeque::new() }
        }
    };
    if comic.pages.is_empty() {
        return Err(OpenError::Failed("There are no pictures in it.".into()));
    }
    // A page whose size couldn't be read takes the size of the first one that could.
    let known = comic.sizes.iter().copied().find(|s| s[0] > 0.0).unwrap_or([1000.0, 1500.0]);
    for s in comic.sizes.iter_mut().filter(|s| s[0] <= 0.0) {
        *s = known;
    }
    Ok(comic)
}

/// The pictures under a folder, as paths relative to `top`.
fn walk(top: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(top, &p, out);
        } else if let Ok(rel) = p.strip_prefix(top) {
            let rel = rel.to_string_lossy().into_owned();
            if is_picture(&rel) {
                out.push(rel);
            }
        }
    }
}

impl Comic {
    fn page(&mut self, i: usize) -> Result<&RgbaImage, String> {
        if let Some(at) = self.decoded.iter().position(|(p, _)| *p == i) {
            let hit = self.decoded.remove(at).expect("present");
            self.decoded.push_front(hit);
            return Ok(&self.decoded[0].1);
        }
        let name = self.pages.get(i).ok_or("There's no such page.")?;
        let mut bytes = Vec::new();
        match &mut self.source {
            Source::Zip(z) => {
                z.by_name(name).map_err(|e| e.to_string())?.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            }
            Source::Folder(_, root) => bytes = std::fs::read(root.join(name)).map_err(|e| e.to_string())?,
        }
        let img = decode(&bytes)?;
        self.decoded.push_front((i, img));
        self.decoded.truncate(3);
        Ok(&self.decoded[0].1)
    }
}

impl Drop for Comic {
    fn drop(&mut self) {
        if let Source::Folder(scratch, _) = &self.source {
            let _ = std::fs::remove_dir_all(scratch);
        }
    }
}

impl Document for Comic {
    fn kind(&self) -> Kind {
        Kind::Comic
    }

    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn page_sizes(&mut self) -> Vec<[f32; 2]> {
        self.sizes.clone()
    }

    fn render(&mut self, page: usize, width: u32, height: u32, clip: Option<[u32; 4]>) -> Result<Bitmap, String> {
        let img = self.page(page)?;
        Ok(draw(img, width, height, clip))
    }
}

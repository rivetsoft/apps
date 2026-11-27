//! Page-based documents with no interface: PDFs (through PDFium), comic archives (.cbz, .cbr,
//! .cb7) and single pictures, all as pages of a known size that can be drawn at any size, with
//! the text on them, their links and the document's outline. Rivet Docs is the window around
//! it; Rivet Files can use it for previews.
//!
//! PDFium is not safe to call from two threads at once, so every document lives on one thread,
//! the [worker], and the window talks to it with messages.

pub mod dark;
pub mod find;
pub mod worker;

mod comic;
mod pdf;
mod picture;

use std::path::{Path, PathBuf};

pub use pdf::{pdfium_status, PdfiumStatus};

/// A rectangle on a page, in points from the page's top-left corner (y grows downwards), before
/// any rotation: [left, top, right, bottom].
pub type Area = [f32; 4];

/// What sort of document it is: it changes the defaults (fit width or fit page) and what
/// can be done with it (no text in a comic, dark pages only for documents).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Pdf,
    Comic,
    Picture,
}

/// A rendered page (or part of one): RGBA pixels, row after row.
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A link on a page: where it is and where it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub area: Area,
    pub to: Target,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A page of this document (counted from 0), and how far down it, in points, when known.
    Page(usize, Option<f32>),
    Url(String),
}

/// An entry of the outline (a PDF's bookmarks): its title, the page it goes to and the entries
/// under it.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineItem {
    pub title: String,
    pub page: Option<usize>,
    pub children: Vec<OutlineItem>,
}

/// The text of a page: every character in reading order, with the box it is drawn in. Spaces
/// and line ends PDFium adds between words have an empty box at the end of the word before.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageText {
    pub chars: Vec<char>,
    pub boxes: Vec<Area>,
}

impl PageText {
    /// The characters from `from` to `to` (not included) as text to copy, with line ends as
    /// the document has them.
    pub fn slice(&self, from: usize, to: usize) -> String {
        let to = to.min(self.chars.len());
        let s: String = self.chars[from.min(to)..to].iter().filter(|c| **c != '\r').collect();
        s.trim_end().to_string()
    }

    /// The character nearest to a point: the one under it, else the closest on the line it is
    /// level with, else the closest at all.
    pub fn nearest(&self, x: f32, y: f32) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for (i, b) in self.boxes.iter().enumerate() {
            if b[2] <= b[0] || b[3] <= b[1] {
                continue;
            }
            if x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3] {
                return Some(i);
            }
            let dx = if x < b[0] { b[0] - x } else if x > b[2] { x - b[2] } else { 0.0 };
            let dy = if y < b[1] { b[1] - y } else if y > b[3] { y - b[3] } else { 0.0 };
            // Being on the same line counts for much more than being near sideways.
            let d = dx + dy * 8.0;
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, i));
            }
        }
        best.map(|(_, i)| i)
    }

    /// The word around a character, as [start, end).
    pub fn word(&self, i: usize) -> (usize, usize) {
        let w = |c: char| c.is_alphanumeric() || c == '\'' || c == '’' || c == '-';
        if i >= self.chars.len() || !w(self.chars[i]) {
            return (i, (i + 1).min(self.chars.len()));
        }
        let mut a = i;
        while a > 0 && w(self.chars[a - 1]) {
            a -= 1;
        }
        let mut b = i;
        while b < self.chars.len() && w(self.chars[b]) {
            b += 1;
        }
        (a, b)
    }

    /// The line around a character, as [start, end).
    pub fn line(&self, i: usize) -> (usize, usize) {
        let br = |c: char| c == '\n' || c == '\r';
        let mut a = i.min(self.chars.len());
        while a > 0 && !br(self.chars[a - 1]) {
            a -= 1;
        }
        let mut b = i;
        while b < self.chars.len() && !br(self.chars[b]) {
            b += 1;
        }
        (a, b)
    }

    /// The boxes to light up for characters [from, to): one per run of characters on a line.
    pub fn areas(&self, from: usize, to: usize) -> Vec<Area> {
        let mut out: Vec<Area> = Vec::new();
        for b in self.boxes.iter().take(to.min(self.boxes.len())).skip(from) {
            if b[2] <= b[0] || b[3] <= b[1] {
                continue;
            }
            if let Some(last) = out.last_mut() {
                let h = (last[3] - last[1]).min(b[3] - b[1]);
                let overlap = last[3].min(b[3]) - last[1].max(b[1]);
                // Same line: they overlap by most of their height and this one follows on.
                if overlap > h * 0.5 && b[0] >= last[0] - 1.0 {
                    *last = [last[0].min(b[0]), last[1].min(b[1]), last[2].max(b[2]), last[3].max(b[3])];
                    continue;
                }
            }
            out.push(*b);
        }
        out
    }
}

/// What a document says about itself.
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub producer: Option<String>,
    /// "1.7" for a PDF.
    pub version: Option<String>,
}

/// Why a document didn't open.
#[derive(Clone, Debug, PartialEq)]
pub enum OpenError {
    /// It is locked with a password (and the one given, if any, was wrong).
    NeedsPassword { wrong: bool },
    /// A PDF, and PDFium couldn't be loaded.
    NoPdfium(String),
    /// Anything else, as a sentence.
    Failed(String),
}

/// A document open on the worker thread.
pub trait Document {
    fn kind(&self) -> Kind;
    fn page_count(&self) -> usize;
    /// Every page's width and height in points, before rotation.
    fn page_sizes(&mut self) -> Vec<[f32; 2]>;
    /// The page drawn `width` × `height` pixels, or the part of it in `clip` (pixels of
    /// that size: [left, top, right, bottom]).
    fn render(&mut self, page: usize, width: u32, height: u32, clip: Option<[u32; 4]>) -> Result<Bitmap, String>;
    fn outline(&mut self) -> Vec<OutlineItem> {
        Vec::new()
    }
    fn text(&mut self, _page: usize) -> Option<PageText> {
        None
    }
    fn links(&mut self, _page: usize) -> Vec<Link> {
        Vec::new()
    }
    /// Where the pictures on a page are, so dark pages can leave them as they are.
    fn pictures(&mut self, _page: usize) -> Vec<Area> {
        Vec::new()
    }
    fn info(&mut self) -> Info {
        Info::default()
    }
}

/// The comic archive endings this crate opens.
pub const COMIC_ENDS: [&str; 3] = ["cbz", "cbr", "cb7"];
/// The picture endings this crate opens as a one-page document.
pub const PICTURE_ENDS: [&str; 7] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "jfif"];

fn ending(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Whether this crate can open the file, by its name (and, for a PDF without the ending, its
/// first bytes).
pub fn can_open(path: &Path) -> bool {
    let e = ending(path);
    e == "pdf" || COMIC_ENDS.contains(&e.as_str()) || PICTURE_ENDS.contains(&e.as_str()) || starts_pdf(path)
}

fn starts_pdf(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 5];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == b"%PDF-"
}

/// Open a document. Call it on the worker thread: the document stays there.
pub fn open(path: &Path, password: Option<&str>) -> Result<Box<dyn Document>, OpenError> {
    if !path.is_file() {
        return Err(OpenError::Failed("There is no such file.".into()));
    }
    let e = ending(path);
    if COMIC_ENDS.contains(&e.as_str()) {
        return comic::open(path).map(|d| Box::new(d) as Box<dyn Document>);
    }
    if PICTURE_ENDS.contains(&e.as_str()) {
        return picture::open(path).map(|d| Box::new(d) as Box<dyn Document>);
    }
    pdf::open(path, password).map(|d| Box::new(d) as Box<dyn Document>)
}

/// A folder of its own for this process under the cache folder, for pages taken out of an
/// archive; removed by the comic that made it.
pub(crate) fn scratch_dir(tag: &str) -> PathBuf {
    let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache"));
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    cache.join("rivet-docs").join(format!("{tag}-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)))
}

/// Names in the order a person counts them: "page 2" before "page 10".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n.push(c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    m.push(d);
                    y.next();
                }
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let o = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PageText {
        // "Hello world\nNext" on two lines, ten points high, six wide a character.
        let text = "Hello world\nNext";
        let mut boxes = Vec::new();
        let (mut x, mut y) = (10.0, 10.0);
        for c in text.chars() {
            if c == '\n' {
                boxes.push([x, y, x, y]);
                x = 10.0;
                y += 14.0;
                continue;
            }
            boxes.push([x, y, x + 6.0, y + 10.0]);
            x += 6.0;
        }
        PageText { chars: text.chars().collect(), boxes }
    }

    #[test]
    fn words_lines_and_areas() {
        let t = sample();
        assert_eq!(t.word(2), (0, 5));
        assert_eq!(t.slice(6, 11), "world");
        assert_eq!(t.line(13), (12, 16));
        assert_eq!(t.areas(0, 16).len(), 2);
        assert_eq!(t.areas(6, 11), vec![[46.0, 10.0, 76.0, 20.0]]);
        assert_eq!(t.nearest(48.0, 15.0), Some(6));
        assert_eq!(t.nearest(500.0, 26.0), Some(15));
    }

    #[test]
    fn counts_like_people() {
        let mut v = vec!["page 10.png", "page 2.png", "Page 1.png", "cover.jpg"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["cover.jpg", "Page 1.png", "page 2.png", "page 10.png"]);
    }
}

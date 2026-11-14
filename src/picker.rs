//! The file picker: Rivet's window answering another app's "Open" or "Save" request.
//!
//! `rivet --portal` (see `portal.rs`) receives requests from the desktop portal and starts
//! `rivet --pick` for each one. The request goes to the picker on stdin as `key=value` lines;
//! the picker prints `path=…` lines (and `filter=N`) on stdout and exits with 0 when something was
//! chosen, 1 when cancelled.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::model::Entry;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    /// Pick one or more existing files (or folders, with `directory`).
    #[default]
    Open,
    /// Pick where to save one file: a folder and a name.
    Save,
    /// Pick a folder to save several named files into.
    SaveFiles,
}

/// One entry in the "file type" list ("Images: *.png, image/jpeg").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeFilter {
    pub name: String,
    /// Name patterns, lower-case ("*.png").
    pub globs: Vec<String>,
    /// MIME types, maybe with a wildcard ("image/*").
    pub mimes: Vec<String>,
}

impl TypeFilter {
    pub fn matches(&self, e: &Entry) -> bool {
        if self.globs.is_empty() && self.mimes.is_empty() {
            return true;
        }
        let name = e.name.to_lowercase();
        self.globs.iter().any(|g| crate::mime::glob_match(g, &name))
            || self.mimes.iter().any(|m| match m.strip_suffix("/*") {
                Some(top) => e.mime.split('/').next() == Some(top),
                None => *m == e.mime,
            })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub mode: Mode,
    pub title: String,
    /// The accept button's label ("Open", "Save", "Attach"…), empty for the default.
    pub accept: String,
    pub multiple: bool,
    pub directory: bool,
    pub folder: Option<PathBuf>,
    /// Suggested file name (Save).
    pub name: String,
    pub filters: Vec<TypeFilter>,
    pub current_filter: Option<usize>,
    /// The names to save (SaveFiles).
    pub files: Vec<String>,
}

/// Keep a value on one line of the request text.
fn one_line(s: &str) -> String {
    s.replace(['\n', '\r', '\t'], " ")
}

impl Request {
    pub fn to_text(&self) -> String {
        let mut t = String::new();
        let mode = match self.mode {
            Mode::Open => "open",
            Mode::Save => "save",
            Mode::SaveFiles => "savefiles",
        };
        t.push_str(&format!("mode={mode}\n"));
        t.push_str(&format!("title={}\n", one_line(&self.title)));
        t.push_str(&format!("accept={}\n", one_line(&self.accept)));
        t.push_str(&format!("multiple={}\n", self.multiple as u8));
        t.push_str(&format!("directory={}\n", self.directory as u8));
        if let Some(f) = &self.folder {
            t.push_str(&format!("folder={}\n", one_line(&f.to_string_lossy())));
        }
        t.push_str(&format!("name={}\n", one_line(&self.name)));
        for f in &self.filters {
            let mut line = one_line(&f.name);
            for g in &f.globs {
                line.push_str(&format!("\tglob:{}", one_line(g)));
            }
            for m in &f.mimes {
                line.push_str(&format!("\tmime:{}", one_line(m)));
            }
            t.push_str(&format!("filter={line}\n"));
        }
        if let Some(c) = self.current_filter {
            t.push_str(&format!("current_filter={c}\n"));
        }
        for f in &self.files {
            t.push_str(&format!("file={}\n", one_line(f)));
        }
        t
    }

    pub fn parse(text: &str) -> Self {
        let mut r = Request::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "mode" => {
                    r.mode = match v {
                        "save" => Mode::Save,
                        "savefiles" => Mode::SaveFiles,
                        _ => Mode::Open,
                    }
                }
                "title" => r.title = v.to_string(),
                "accept" => r.accept = v.to_string(),
                "multiple" => r.multiple = v == "1",
                "directory" => r.directory = v == "1",
                "folder" if !v.is_empty() => r.folder = Some(PathBuf::from(v)),
                "name" => r.name = v.to_string(),
                "filter" => {
                    let mut parts = v.split('\t');
                    let mut f = TypeFilter { name: parts.next().unwrap_or_default().to_string(), ..Default::default() };
                    for p in parts {
                        if let Some(g) = p.strip_prefix("glob:") {
                            f.globs.push(g.to_lowercase());
                        } else if let Some(m) = p.strip_prefix("mime:") {
                            f.mimes.push(m.to_lowercase());
                        }
                    }
                    r.filters.push(f);
                }
                "current_filter" => r.current_filter = v.parse().ok(),
                "file" => r.files.push(v.to_string()),
                _ => {}
            }
        }
        if r.current_filter.is_some_and(|c| c >= r.filters.len()) {
            r.current_filter = None;
        }
        r
    }

    /// Only folders matter: choosing a folder to open, or where several files go.
    pub fn folders_only(&self) -> bool {
        (self.mode == Mode::Open && self.directory) || self.mode == Mode::SaveFiles
    }

    pub fn accept_label(&self) -> String {
        if !self.accept.trim().is_empty() {
            return self.accept.trim().to_string();
        }
        match (self.mode, self.directory) {
            (Mode::Open, true) => "Select".into(),
            (Mode::Open, false) => "Open".into(),
            _ => "Save".into(),
        }
    }

    pub fn window_title(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.trim().to_string();
        }
        match (self.mode, self.directory) {
            (Mode::Open, true) => "Select a Folder".into(),
            (Mode::Open, false) => if self.multiple { "Open Files" } else { "Open File" }.into(),
            (Mode::Save, _) => "Save File".into(),
            (Mode::SaveFiles, _) => "Save Files".into(),
        }
    }
}

/// What the picker hands back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Answer {
    pub paths: Vec<PathBuf>,
    pub filter: Option<usize>,
}

impl Answer {
    pub fn to_text(&self) -> String {
        let mut t = String::new();
        for p in &self.paths {
            t.push_str(&format!("path={}\n", p.to_string_lossy()));
        }
        if let Some(f) = self.filter {
            t.push_str(&format!("filter={f}\n"));
        }
        t
    }

    pub fn parse(text: &str) -> Self {
        let mut a = Answer::default();
        for line in text.lines() {
            if let Some(p) = line.strip_prefix("path=") {
                let p = PathBuf::from(p);
                if p.is_absolute() {
                    a.paths.push(p);
                }
            } else if let Some(f) = line.strip_prefix("filter=") {
                a.filter = f.parse().ok();
            }
        }
        a
    }
}

// ------------------------------------------------------------------ this process as a picker

static REQUEST: OnceLock<Request> = OnceLock::new();
/// The file type chosen in the picker's list (an index into the request's filters).
static CHOSEN_FILTER: Mutex<Option<usize>> = Mutex::new(None);
/// What was picked, once accepted (None until then = cancelled).
static ANSWER: Mutex<Option<Answer>> = Mutex::new(None);

/// Become a picker for this request (`rivet --pick`).
pub fn begin(r: Request) {
    *CHOSEN_FILTER.lock().unwrap_or_else(|e| e.into_inner()) = r.current_filter.or(if r.filters.is_empty() { None } else { Some(0) });
    let _ = REQUEST.set(r);
}

/// The request, when this process is a picker.
pub fn request() -> Option<&'static Request> {
    REQUEST.get()
}

pub fn active() -> bool {
    REQUEST.get().is_some()
}

pub fn chosen_filter() -> Option<usize> {
    *CHOSEN_FILTER.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_chosen_filter(i: Option<usize>) {
    *CHOSEN_FILTER.lock().unwrap_or_else(|e| e.into_inner()) = i;
}

/// Does the picker list this item? (Folders always show, so you can move around.)
pub fn shows(e: &Entry) -> bool {
    let Some(r) = REQUEST.get() else { return true };
    if e.is_dir {
        return true;
    }
    if r.folders_only() {
        return false;
    }
    match chosen_filter().and_then(|i| r.filters.get(i)) {
        Some(f) => f.matches(e),
        None => true,
    }
}

pub fn finish(a: Answer) {
    *ANSWER.lock().unwrap_or_else(|e| e.into_inner()) = Some(a);
}

pub fn answer() -> Option<Answer> {
    ANSWER.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The folder to start in: the request's, else the last one used (if it still exists), else Home.
pub fn start_folder(r: &Request) -> PathBuf {
    r.folder
        .clone()
        .filter(|f| f.is_dir())
        .or_else(|| std::fs::read_to_string(last_folder_file()).ok().map(|t| PathBuf::from(t.trim())).filter(|p| p.is_dir()))
        .unwrap_or_else(crate::places::home)
}

pub fn remember_folder(dir: &Path) {
    let f = last_folder_file();
    if let Some(p) = f.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let _ = std::fs::write(f, dir.to_string_lossy().as_bytes());
}

fn last_folder_file() -> PathBuf {
    crate::places::state_home().join("rivet/picker-folder")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips() {
        let r = Request {
            mode: Mode::Save,
            title: "Export\nimage".into(),
            accept: "Export".into(),
            multiple: false,
            directory: false,
            folder: Some("/home/me/Pictures".into()),
            name: "photo.png".into(),
            filters: vec![
                TypeFilter { name: "PNG".into(), globs: vec!["*.png".into()], mimes: vec![] },
                TypeFilter { name: "Images".into(), globs: vec![], mimes: vec!["image/*".into()] },
            ],
            current_filter: Some(1),
            files: vec![],
        };
        let back = Request::parse(&r.to_text());
        assert_eq!(back.title, "Export image");
        assert_eq!(back.filters, r.filters);
        assert_eq!(back.current_filter, Some(1));
        assert_eq!(back.folder, r.folder);
        assert_eq!(back.mode, Mode::Save);
    }

    #[test]
    fn answer_round_trips_and_drops_relative_paths() {
        let a = Answer { paths: vec!["/tmp/a b.txt".into(), "relative".into()], filter: Some(0) };
        let back = Answer::parse(&a.to_text());
        assert_eq!(back.paths, vec![PathBuf::from("/tmp/a b.txt")]);
        assert_eq!(back.filter, Some(0));
    }
}

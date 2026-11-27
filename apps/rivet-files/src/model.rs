//! Folder contents, tabs and navigation history.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Instant, SystemTime};

use crate::fsops;
// Natural sort is shared with Rivet Long Shot, which steps through a folder in this order.
pub use rivet_xdg::order::natural_cmp;
use rivet_xdg::order::natural_cmp_bytes;
use crate::mime::MimeDb;
use crate::places;
use crate::settings::{Settings, SortKey};

#[derive(Clone, Debug, PartialEq)]
pub enum Location {
    Dir(PathBuf),
    Trash,
    Recent,
    Network,
    Search { root: PathBuf, query: String },
}

impl Location {
    pub fn title(&self) -> String {
        match self {
            Location::Dir(p) => {
                if *p == places::home() {
                    "Home".into()
                } else if p == Path::new("/") {
                    "File System".into()
                } else {
                    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string())
                }
            }
            Location::Trash => "Trash".into(),
            Location::Recent => "Recent".into(),
            Location::Network => "Network".into(),
            Location::Search { query, .. } => format!("Search: {query}"),
        }
    }
    pub fn dir(&self) -> Option<&Path> {
        match self {
            Location::Dir(p) => Some(p),
            _ => None,
        }
    }
}

/// Numbers that change whenever a selection or a folder's shown items do, so what's worked
/// out from them (the status bar, Peek's summary) is kept until then. One counter for all
/// tabs, so a number never means two different states.
static CHANGES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_change() -> u64 {
    CHANGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The selected paths, and a number that changes whenever they do.
#[derive(Clone, Debug, Default)]
pub struct Selection {
    set: HashSet<PathBuf>,
    change: u64,
}

impl Selection {
    pub fn change(&self) -> u64 {
        self.change
    }
    pub fn contains(&self, p: &Path) -> bool {
        self.set.contains(p)
    }
    pub fn len(&self) -> usize {
        self.set.len()
    }
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }
    pub fn iter(&self) -> std::collections::hash_set::Iter<'_, PathBuf> {
        self.set.iter()
    }
    pub fn insert(&mut self, p: PathBuf) -> bool {
        self.change = next_change();
        self.set.insert(p)
    }
    pub fn remove(&mut self, p: &Path) -> bool {
        self.change = next_change();
        self.set.remove(p)
    }
    pub fn clear(&mut self) {
        if !self.set.is_empty() {
            self.change = next_change();
            self.set.clear();
        }
    }
    pub fn retain(&mut self, f: impl FnMut(&PathBuf) -> bool) {
        let before = self.set.len();
        self.set.retain(f);
        if self.set.len() != before {
            self.change = next_change();
        }
    }
}

impl FromIterator<PathBuf> for Selection {
    fn from_iter<I: IntoIterator<Item = PathBuf>>(iter: I) -> Self {
        Selection { set: iter.into_iter().collect(), change: next_change() }
    }
}

impl<'a> IntoIterator for &'a Selection {
    type Item = &'a PathBuf;
    type IntoIter = std::collections::hash_set::Iter<'a, PathBuf>;
    fn into_iter(self) -> Self::IntoIter {
        self.set.iter()
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    /// The name in lower case, made once, for sorting and the filter.
    pub lower: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub broken_link: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub mime: String,
    pub hidden: bool,
    pub mode: u32,
    pub uid: u32,
    pub child_count: Option<usize>,
    pub trash_origin: Option<(PathBuf, String)>,
}

impl Entry {
    pub fn from_path(path: PathBuf, db: &MimeDb) -> Option<Entry> {
        let lmeta = std::fs::symlink_metadata(&path).ok()?;
        let is_link = lmeta.file_type().is_symlink();
        let meta = if is_link { std::fs::metadata(&path).ok() } else { Some(lmeta.clone()) };
        let broken_link = is_link && meta.is_none();
        let meta = meta.unwrap_or(lmeta);
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
        let is_dir = meta.is_dir();
        let mode = meta.permissions().mode();
        let executable = !is_dir && mode & 0o111 != 0;
        let mime = if broken_link {
            "inode/symlink".to_string()
        } else if !is_dir && meta.len() == 0 {
            // Empty file: nothing to sniff, so don't open it — the name decides.
            db.guess_by_name(&path).unwrap_or_else(|| "application/x-zerosize".into())
        } else {
            db.guess(&path, is_dir, executable)
        };
        let child_count = if is_dir {
            std::fs::read_dir(&path).ok().map(|r| r.count())
        } else {
            None
        };
        Some(Entry {
            hidden: name.starts_with('.') || name.ends_with('~'),
            lower: name.to_lowercase(),
            name,
            is_dir,
            is_link,
            broken_link,
            size: meta.len(),
            modified: meta.modified().ok(),
            mime,
            mode,
            uid: meta.uid(),
            child_count,
            trash_origin: None,
            path,
        })
    }

    pub fn is_executable(&self) -> bool {
        !self.is_dir && self.mode & 0o111 != 0
    }
}

pub struct Tab {
    pub loc: Location,
    pub back: Vec<Location>,
    pub forward: Vec<Location>,
    pub entries: Vec<Entry>,
    /// Indices into `entries` after hiding/filtering/sorting.
    pub visible: Vec<usize>,
    pub selected: Selection,
    pub anchor: Option<usize>,
    pub cursor: Option<usize>,
    pub filter: String,
    /// How many items the active filter (global / folder) hides here.
    pub filtered_out: usize,
    pub error: Option<String>,
    pub dir_mtime: Option<SystemTime>,
    pub last_check: Instant,
    pub scroll_to: Option<PathBuf>,
    pub search_rx: Option<Receiver<Vec<PathBuf>>>,
    pub searching: bool,
    pub rubber: Option<eframe::egui::Pos2>,
    /// Finding the selection without walking the whole folder (see `selected_entries`): each
    /// path's hash → its index in `entries`, and each entry's place in `visible`. Built by
    /// `resort`; the path hash map only when `entries` changed.
    by_hash: HashMap<u64, u32>,
    hasher: std::hash::RandomState,
    indexed: Option<usize>,
    vis_pos: Vec<u32>,
    /// Changes whenever `entries` or `visible` do (see `selection_key`).
    view_change: u64,
}

impl Tab {
    pub fn new(loc: Location) -> Self {
        Tab {
            loc,
            back: vec![],
            forward: vec![],
            entries: vec![],
            visible: vec![],
            selected: Selection::default(),
            anchor: None,
            cursor: None,
            filter: String::new(),
            filtered_out: 0,
            error: None,
            dir_mtime: None,
            last_check: Instant::now(),
            scroll_to: None,
            search_rx: None,
            searching: false,
            rubber: None,
            by_hash: HashMap::new(),
            hasher: std::hash::RandomState::new(),
            indexed: None,
            vis_pos: Vec::new(),
            view_change: 0,
        }
    }

    pub fn reload(&mut self, db: &MimeDb, settings: &Settings) {
        self.error = None;
        let mut entries = Vec::new();
        match &self.loc {
            Location::Dir(p) => {
                self.dir_mtime = std::fs::metadata(p).and_then(|m| m.modified()).ok();
                match std::fs::read_dir(p) {
                    Ok(rd) => {
                        let paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
                        entries = inspect_all(paths, db);
                    }
                    Err(e) => {
                        self.error = Some(match e.kind() {
                            std::io::ErrorKind::PermissionDenied => "You do not have permission to view this folder.".into(),
                            std::io::ErrorKind::NotFound => "This folder no longer exists.".into(),
                            _ => format!("Could not open this folder: {e}"),
                        })
                    }
                }
            }
            Location::Trash => {
                let files = places::trash_dir().join("files");
                self.dir_mtime = std::fs::metadata(&files).and_then(|m| m.modified()).ok();
                if let Ok(rd) = std::fs::read_dir(&files) {
                    for e in rd.flatten() {
                        if let Some(mut en) = Entry::from_path(e.path(), db) {
                            en.trash_origin = fsops::trash_info(&en.path);
                            en.hidden = false;
                            entries.push(en);
                        }
                    }
                }
            }
            Location::Recent => {
                self.dir_mtime = None;
                for p in places::recent_files() {
                    if let Some(mut en) = Entry::from_path(p, db) {
                        en.hidden = false;
                        entries.push(en);
                    }
                }
            }
            Location::Network => {
                self.dir_mtime = None;
                for p in places::network_places() {
                    if let Some(mut en) = Entry::from_path(p, db) {
                        en.hidden = false;
                        entries.push(en);
                    }
                }
            }
            Location::Search { .. } => {
                // Results arrive from the search thread; keep current list.
                return;
            }
        }
        self.entries = entries;
        self.indexed = None;
        if !self.selected.is_empty() {
            let keep: HashSet<&Path> = self.entries.iter().map(|e| e.path.as_path()).collect();
            self.selected.retain(|p| keep.contains(p.as_path()));
        }
        self.resort(db, settings);
        self.last_check = Instant::now();
    }

    pub fn resort(&mut self, db: &MimeDb, s: &Settings) {
        let f = self.filter.to_lowercase();
        let show_hidden = s.show_hidden || matches!(self.loc, Location::Search { .. });
        // The filter (global / folder / folder & subfolders) that applies here, if any.
        let rule = self.loc.dir().and_then(|d| s.filters.active_for(d)).map(|(_, r)| r.clone());
        let now = std::time::SystemTime::now();
        let mut hidden_by_rule = 0;
        let mut vis: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i];
                if !((show_hidden || !e.hidden) && (f.is_empty() || e.lower.contains(&f))) {
                    return false;
                }
                // Picking for another app: only what it can use (and folders to get there).
                if !crate::picker::shows(e) {
                    return false;
                }
                if let Some(r) = &rule
                    && !r.matches(e, now) {
                        hidden_by_rule += 1;
                        return false;
                    }
                true
            })
            .collect();
        self.filtered_out = hidden_by_rule;
        let entries = &self.entries;
        let recent = matches!(self.loc, Location::Recent);
        // Sorting by type: each entry's type description as its place in the sorted list of
        // descriptions, so comparing two is comparing two numbers.
        let mut type_rank: Vec<u32> = Vec::new();
        if s.sort == SortKey::Type {
            let mut described: HashMap<&str, String> = HashMap::new();
            for &i in &vis {
                let m = entries[i].mime.as_str();
                described.entry(m).or_insert_with(|| db.describe(m));
            }
            let mut order: Vec<&String> = described.values().collect();
            order.sort();
            order.dedup();
            let rank: HashMap<&str, u32> = described.iter().map(|(m, d)| (*m, order.binary_search(&d).unwrap_or(0) as u32)).collect();
            type_rank = entries.iter().map(|e| rank.get(e.mime.as_str()).copied().unwrap_or(0)).collect();
        }
        // The sort reads compact copies of the names and folder flags rather than the entries:
        // a big folder's entries don't fit in the processor's cache, and a sort visits them in
        // no order, so reading them was most of its time.
        let mut names: Vec<u8> = Vec::new();
        let mut span = vec![(0u32, 0u32); entries.len()];
        let mut dir = vec![false; entries.len()];
        for &i in &vis {
            let k = entries[i].lower.trim_start_matches('.').as_bytes();
            span[i] = (names.len() as u32, k.len() as u32);
            names.extend_from_slice(k);
            dir[i] = entries[i].is_dir;
        }
        let name = |i: usize| {
            let (start, len) = span[i];
            &names[start as usize..(start + len) as usize]
        };
        vis.sort_by(|&a, &b| {
            if s.folders_first && dir[a] != dir[b] && !recent {
                return dir[b].cmp(&dir[a]);
            }
            let (x, y) = (&entries[a], &entries[b]);
            let o = match s.sort {
                SortKey::Name => natural_cmp_bytes(name(a), name(b)),
                SortKey::Size => {
                    if x.is_dir && y.is_dir {
                        x.child_count.cmp(&y.child_count)
                    } else {
                        x.size.cmp(&y.size)
                    }
                }
                SortKey::Type => type_rank[a].cmp(&type_rank[b]),
                SortKey::Modified => x.modified.cmp(&y.modified),
            }
            .then_with(|| natural_cmp_bytes(name(a), name(b)));
            if s.sort_desc { o.reverse() } else { o }
        });
        self.visible = vis;
        self.view_change = next_change();
        self.index();
    }

    /// For the benchmark: put `entries` in a scrambled order, as a folder whose files were
    /// made in no particular order reads back, so that sorting is timed on unsorted input.
    pub fn scramble(&mut self) {
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        for i in (1..self.entries.len()).rev() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.entries.swap(i, (x % (i as u64 + 1)) as usize);
        }
        self.indexed = None;
    }

    /// Bring the selection index up to date. `entries` is replaced only by `reload` (which
    /// marks the index stale) and otherwise only cleared or added to (which changes its length).
    fn index(&mut self) {
        if self.indexed != Some(self.entries.len()) {
            self.by_hash.clear();
            self.by_hash.reserve(self.entries.len());
            for (i, e) in self.entries.iter().enumerate() {
                // On the rare clash the first keeps the slot; lookups check the path and fall
                // back to walking the folder.
                self.by_hash.entry(self.hasher.hash_one(&e.path)).or_insert(i as u32);
            }
            self.indexed = Some(self.entries.len());
        }
        self.vis_pos.clear();
        self.vis_pos.resize(self.entries.len(), u32::MAX);
        for (pos, &i) in self.visible.iter().enumerate() {
            self.vis_pos[i] = pos as u32;
        }
    }

    /// The selected entries in folder order, found through the index: `None` when it can't
    /// answer (a hash clash, or the folder changed since it was built).
    fn selected_by_index(&self) -> Option<Vec<&Entry>> {
        let mut found: Vec<(u32, &Entry)> = Vec::with_capacity(self.selected.len());
        for p in &self.selected {
            // Not in the map: not in this folder at all.
            let Some(&i) = self.by_hash.get(&self.hasher.hash_one(p)) else { continue };
            let e = self.entries.get(i as usize).filter(|e| e.path == *p)?;
            let pos = *self.vis_pos.get(i as usize)?;
            if pos != u32::MAX {
                found.push((pos, e));
            }
        }
        found.sort_unstable_by_key(|&(pos, _)| pos);
        Some(found.into_iter().map(|(_, e)| e).collect())
    }

    pub fn visible_index_of(&self, p: &Path) -> Option<usize> {
        if self.indexed == Some(self.entries.len())
            && let Some(&i) = self.by_hash.get(&self.hasher.hash_one(p))
            && self.entries.get(i as usize).is_some_and(|e| e.path == p)
            && let Some(&pos) = self.vis_pos.get(i as usize)
        {
            return (pos != u32::MAX).then_some(pos as usize);
        }
        self.visible.iter().position(|&i| self.entries[i].path == p)
    }

    /// Changes whenever the selection or the items it's taken from do: a key for keeping
    /// what's worked out from the selection.
    pub fn selection_key(&self) -> (u64, u64) {
        (self.selected.change(), self.view_change)
    }

    /// The selected items in the order they're shown. A small selection is found through the
    /// index; a large one (most of the folder) by walking the folder once.
    pub fn selected_entries(&self) -> Vec<&Entry> {
        if self.selected.is_empty() {
            return Vec::new();
        }
        if self.selected.len() <= self.visible.len() / 4 + 64
            && self.indexed == Some(self.entries.len())
            && let Some(v) = self.selected_by_index()
        {
            return v;
        }
        self.visible
            .iter()
            .map(|&i| &self.entries[i])
            .filter(|e| self.selected.contains(&e.path))
            .collect()
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_entries().iter().map(|e| e.path.clone()).collect()
    }

    /// Re-read the folder if it changed on disk (cheap polling instead of a watcher).
    /// The folder this tab shows, if it lives on disk (for the watcher).
    pub fn watched_dir(&self) -> Option<PathBuf> {
        match &self.loc {
            Location::Dir(p) => Some(p.clone()),
            Location::Trash => Some(places::trash_dir().join("files")),
            _ => None,
        }
    }

    pub fn needs_refresh(&mut self) -> bool {
        self.last_check = Instant::now();
        let p = match &self.loc {
            Location::Dir(p) => p.clone(),
            Location::Trash => places::trash_dir().join("files"),
            _ => return false,
        };
        let m = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
        m != self.dir_mtime
    }
}

/// Inspect many paths, spread over the CPU cores for big folders.
pub fn inspect_all(paths: Vec<PathBuf>, db: &MimeDb) -> Vec<Entry> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(16);
    if paths.len() < 2_000 || threads < 2 {
        return paths.into_iter().filter_map(|p| Entry::from_path(p, db)).collect();
    }
    let chunk = paths.len().div_ceil(threads);
    std::thread::scope(|s| {
        let handles: Vec<_> = paths
            .chunks(chunk)
            .map(|part| s.spawn(move || part.iter().filter_map(|p| Entry::from_path(p.clone(), db)).collect::<Vec<_>>()))
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

/// Recursive name search below `root`, streamed back in batches.
pub fn spawn_search(root: PathBuf, query: String, show_hidden: bool, ctx: eframe::egui::Context) -> Receiver<Vec<PathBuf>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let q = query.to_lowercase();
        let mut stack = vec![root];
        let mut batch = Vec::new();
        let mut total = 0;
        let mut last = Instant::now();
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !show_hidden && name.starts_with('.') {
                    continue;
                }
                let ft = e.file_type().ok();
                if ft.map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push(e.path());
                }
                if name.to_lowercase().contains(&q) {
                    batch.push(e.path());
                    total += 1;
                }
            }
            if !batch.is_empty() && last.elapsed().as_millis() > 150 {
                if tx.send(std::mem::take(&mut batch)).is_err() {
                    return;
                }
                ctx.request_repaint();
                last = Instant::now();
            }
            if total > 5000 {
                break;
            }
        }
        let _ = tx.send(batch);
        ctx.request_repaint();
    });
    rx
}

#[cfg(test)]
mod tests {
    #[test]
    fn natural_order() {
        let mut v = vec!["file10", "File2", "file1", ".hidden", "alpha"];
        v.sort_by(|a, b| super::natural_cmp(a, b));
        assert_eq!(v, vec!["alpha", "file1", "File2", "file10", ".hidden"]);
    }
    #[test]
    fn natural_order_without_allocating() {
        use std::cmp::Ordering::*;
        let c = rivet_xdg::order::natural_cmp_lower;
        assert_eq!(c("a01", "a1"), Equal);
        assert_eq!(c("a007b", "a7c"), Less);
        assert_eq!(c("v2.10", "v2.9"), Greater);
        assert_eq!(c("photo", "photo 1"), Less);
        assert_eq!(c("élan", "zebra"), Greater);
        assert_eq!(c("éa", "éb"), Less);
        assert_eq!(c("", ""), Equal);
    }
    #[test]
    fn selection_found_through_the_index() {
        use super::*;
        let dir = std::env::temp_dir().join(format!("rivet-sel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["b.txt", "a.txt", "c.txt", ".hidden"] {
            std::fs::write(dir.join(n), "x").unwrap();
        }
        let db = crate::mime::MimeDb::load();
        let s = crate::settings::Settings::default();
        let mut t = Tab::new(Location::Dir(dir.clone()));
        t.reload(&db, &s);
        let names = |t: &Tab| t.selected_entries().iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        // In the order shown, whatever order they were chosen in; hidden ones left out.
        let before = t.selection_key();
        t.selected = [".hidden", "c.txt", "a.txt"].iter().map(|n| dir.join(n)).collect();
        assert_ne!(t.selection_key(), before);
        assert_eq!(names(&t), ["a.txt", "c.txt"]);
        assert_eq!(t.visible_index_of(&dir.join("c.txt")), Some(2));
        assert_eq!(t.visible_index_of(&dir.join(".hidden")), None);
        // Items added later (search results arrive like this) are found too.
        let late = dir.join("d.txt");
        std::fs::write(&late, "x").unwrap();
        t.entries.push(Entry::from_path(late.clone(), &db).unwrap());
        t.resort(&db, &s);
        t.selected.insert(late);
        assert_eq!(names(&t), ["a.txt", "c.txt", "d.txt"]);
        // Everything selected.
        t.selected = t.visible.iter().map(|&i| t.entries[i].path.clone()).collect();
        assert_eq!(names(&t), ["a.txt", "b.txt", "c.txt", "d.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn mime_guess() {
        let db = crate::mime::MimeDb::load();
        let g = |n: &str| db.guess(std::path::Path::new(n), false, false);
        assert_eq!(g("/nonexistent/x.png"), "image/png");
        assert_eq!(g("/nonexistent/archive.tar.gz"), "application/x-compressed-tar");
        assert!(crate::mime::glob_match("*.[ch]", "main.c"));
        assert!(db.icon_names("image/png").iter().any(|n| n == "image-x-generic"));
    }
}

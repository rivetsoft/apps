//! The play queue: the files to play in order, which one is playing, repeat and shuffle, and
//! turning what was opened (files, folders, playlists) into that list.

use std::path::{Path, PathBuf};

use rivet_xdg::mime::MimeDb;
use rivet_xdg::order::natural_cmp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    #[default]
    Off,
    /// The whole queue, from the top again after the last.
    All,
    /// The file playing, over and over.
    One,
}

impl Repeat {
    pub fn key(self) -> &'static str {
        match self {
            Repeat::Off => "off",
            Repeat::All => "all",
            Repeat::One => "one",
        }
    }

    pub fn from_key(k: &str) -> Repeat {
        match k {
            "all" => Repeat::All,
            "one" => Repeat::One,
            _ => Repeat::Off,
        }
    }

    pub fn next(self) -> Repeat {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }
}

#[derive(Default)]
pub struct Queue {
    items: Vec<PathBuf>,
    current: Option<usize>,
    pub repeat: Repeat,
    /// The order played in while shuffle is on: indexes into `items`.
    order: Option<Vec<usize>>,
    seed: u64,
}

impl Queue {
    pub fn items(&self) -> &[PathBuf] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.current.and_then(|i| self.items.get(i)).map(PathBuf::as_path)
    }

    pub fn shuffled(&self) -> bool {
        self.order.is_some()
    }

    /// Put files at the end; returns the index of the first one added.
    pub fn add(&mut self, paths: Vec<PathBuf>) -> usize {
        let first = self.items.len();
        self.items.extend(paths);
        if let Some(order) = &mut self.order {
            order.extend(first..self.items.len());
        }
        first
    }

    pub fn set_current(&mut self, i: usize) {
        if i < self.items.len() {
            self.current = Some(i);
        }
    }

    /// Turn shuffle on (a fresh random order, the file playing first) or off.
    pub fn set_shuffle(&mut self, on: bool, seed: u64) {
        if !on {
            self.order = None;
            return;
        }
        self.seed = seed | 1;
        let mut order: Vec<usize> = (0..self.items.len()).collect();
        // Fisher–Yates with a small xorshift: good enough to mix a playlist.
        for i in (1..order.len()).rev() {
            let j = (self.rand() % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        if let Some(c) = self.current
            && let Some(at) = order.iter().position(|&i| i == c)
        {
            order.remove(at);
            order.insert(0, c);
        }
        self.order = Some(order);
    }

    fn rand(&mut self) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }

    /// The play order: shuffled, or as listed.
    fn order(&self) -> Vec<usize> {
        self.order.clone().unwrap_or_else(|| (0..self.items.len()).collect())
    }

    /// What plays after the current file when it ends, or None at the end of the queue.
    /// `by_hand`: the person asked for the next one, so "repeat one" moves on too.
    pub fn next(&self, by_hand: bool) -> Option<usize> {
        let c = self.current?;
        if self.repeat == Repeat::One && !by_hand {
            return Some(c);
        }
        let order = self.order();
        let at = order.iter().position(|&i| i == c)?;
        match order.get(at + 1) {
            Some(&i) => Some(i),
            None if self.repeat != Repeat::Off => order.first().copied(),
            None => None,
        }
    }

    /// The one before, wrapping round when repeating.
    pub fn previous(&self) -> Option<usize> {
        let c = self.current?;
        let order = self.order();
        let at = order.iter().position(|&i| i == c)?;
        if at > 0 {
            Some(order[at - 1])
        } else if self.repeat != Repeat::Off {
            order.last().copied()
        } else {
            None
        }
    }

    pub fn remove(&mut self, i: usize) {
        if i >= self.items.len() {
            return;
        }
        self.items.remove(i);
        let fix = |j: usize| if j > i { j - 1 } else { j };
        if let Some(order) = &mut self.order {
            order.retain(|&j| j != i);
            for j in order.iter_mut() {
                *j = fix(*j);
            }
        }
        self.current = match self.current {
            Some(c) if c == i => None,
            Some(c) => Some(fix(c)),
            None => None,
        };
    }

    /// Move a file to another place in the list.
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.items.len() || to >= self.items.len() || from == to {
            return;
        }
        let item = self.items.remove(from);
        self.items.insert(to, item);
        let map = |j: usize| {
            if j == from {
                to
            } else if from < to && j > from && j <= to {
                j - 1
            } else if to < from && j >= to && j < from {
                j + 1
            } else {
                j
            }
        };
        self.current = self.current.map(map);
        if let Some(order) = &mut self.order {
            for j in order.iter_mut() {
                *j = map(*j);
            }
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current = None;
        if self.order.is_some() {
            self.order = Some(Vec::new());
        }
    }
}

/// Whether a file of this type is something to play.
pub fn playable(mime: &str) -> bool {
    (mime.starts_with("video/") || mime.starts_with("audio/")) && !is_playlist_type(mime)
}

fn is_playlist_type(mime: &str) -> bool {
    matches!(mime, "audio/x-mpegurl" | "audio/mpegurl" | "application/vnd.apple.mpegurl" | "application/x-mpegurl" | "audio/x-scpls" | "application/pls+xml")
}

fn is_playlist(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref(), Some("m3u" | "m3u8" | "pls"))
}

/// The files in a playlist (`.m3u`, `.m3u8`, `.pls`), resolved against its folder.
pub fn read_playlist(path: &Path, text: &str) -> Vec<PathBuf> {
    let dir = path.parent().unwrap_or(Path::new("/"));
    let pls = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pls"));
    text.lines()
        .map(str::trim)
        .filter_map(|l| {
            if pls {
                let (k, v) = l.split_once('=')?;
                k.to_lowercase().starts_with("file").then_some(v.trim())
            } else {
                (!l.is_empty() && !l.starts_with('#')).then_some(l)
            }
        })
        .filter(|l| !l.contains("://") || l.starts_with("file://"))
        .map(|l| {
            let p = PathBuf::from(l.strip_prefix("file://").map(rivet_xdg::places::percent_decode).unwrap_or_else(|| l.to_string()));
            if p.is_absolute() { p } else { dir.join(p) }
        })
        .collect()
}

/// Everything to play among what was opened: files as they are, the playable files of a
/// folder (its own by name, then each folder inside), and the files a playlist names.
/// Reads folders, so run it on a thread.
pub fn expand(paths: &[PathBuf], db: &MimeDb) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            walk(p, db, &mut out, 0);
        } else if is_playlist(p) {
            let text = std::fs::read_to_string(p).unwrap_or_default();
            out.extend(read_playlist(p, &text).into_iter().filter(|f| f.is_file()));
        } else {
            out.push(p.clone());
        }
    }
    out
}

fn walk(dir: &Path, db: &MimeDb, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 8 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            dirs.push((name, path));
        } else if db.guess_by_name(&path).is_some_and(|m| playable(&m)) {
            files.push((name, path));
        }
    }
    files.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    dirs.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    out.extend(files.into_iter().map(|(_, p)| p));
    for (_, d) in dirs {
        walk(&d, db, out, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue(n: usize) -> Queue {
        let mut q = Queue::default();
        q.add((0..n).map(|i| PathBuf::from(format!("/m/{i}.flac"))).collect());
        q.set_current(0);
        q
    }

    #[test]
    fn next_and_previous_follow_repeat() {
        let mut q = queue(3);
        assert_eq!(q.previous(), None);
        q.set_current(2);
        assert_eq!(q.next(false), None);
        q.repeat = Repeat::All;
        assert_eq!(q.next(false), Some(0));
        q.set_current(0);
        assert_eq!(q.previous(), Some(2));
        q.repeat = Repeat::One;
        assert_eq!(q.next(false), Some(0));
        assert_eq!(q.next(true), Some(1));
    }

    #[test]
    fn shuffle_plays_everything_once_starting_with_the_current() {
        let mut q = queue(20);
        q.set_current(7);
        q.set_shuffle(true, 12345);
        let mut seen = vec![7];
        while let Some(i) = q.next(false) {
            seen.push(i);
            q.set_current(i);
        }
        let mut sorted = seen.clone();
        sorted.sort();
        assert_eq!(sorted, (0..20).collect::<Vec<_>>());
        assert_eq!(seen[0], 7);
        q.set_shuffle(false, 0);
        assert!(!q.shuffled());
    }

    #[test]
    fn removing_and_moving_keep_the_current_file() {
        let mut q = queue(5);
        q.set_current(3);
        q.remove(1);
        assert_eq!(q.current_path(), Some(Path::new("/m/3.flac")));
        q.move_item(2, 0);
        assert_eq!(q.current(), Some(0));
        assert_eq!(q.items()[0], PathBuf::from("/m/3.flac"));
        q.move_item(0, 3);
        assert_eq!(q.current_path(), Some(Path::new("/m/3.flac")));
        q.remove(3);
        assert_eq!(q.current(), None);
    }

    #[test]
    fn playlists() {
        let m3u = "#EXTM3U\n#EXTINF:12,One\none.flac\n/abs/two.ogg\nhttp://radio.example/stream\nfile:///abs/three%203.mp3\n";
        assert_eq!(read_playlist(Path::new("/music/list.m3u"), m3u), vec![PathBuf::from("/music/one.flac"), PathBuf::from("/abs/two.ogg"), PathBuf::from("/abs/three 3.mp3")]);
        let pls = "[playlist]\nFile1=a.mp3\nTitle1=A\nFile2=/b.mp3\nNumberOfEntries=2\n";
        assert_eq!(read_playlist(Path::new("/m/x.pls"), pls), vec![PathBuf::from("/m/a.mp3"), PathBuf::from("/b.mp3")]);
        assert!(playable("video/mp4") && playable("audio/flac") && !playable("audio/x-mpegurl") && !playable("image/png"));
    }
}

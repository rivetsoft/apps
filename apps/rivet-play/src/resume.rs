//! Where each file was left, so it starts there next time. One line per file in
//! `~/.local/state/rivet-play/positions`: a key for the file, the position in seconds and when it
//! was saved. Only the newest 500 are kept.

use std::path::{Path, PathBuf};

const KEEP: usize = 500;

/// A file's key: its path and size, hashed (FNV-1a), so a replaced file starts from the top.
pub fn key(path: &Path, size: u64) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in path.as_os_str().as_encoded_bytes().iter().chain(size.to_le_bytes().iter()) {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Whether a place in a file is worth coming back to: in a file over a minute long, past the
/// first 30 seconds and before the last 5%.
pub fn worth_keeping(pos: f64, length: f64) -> bool {
    length > 60.0 && pos > 30.0 && pos < length * 0.95
}

#[derive(Default)]
pub struct Positions {
    file: Option<PathBuf>,
    /// Key, seconds in, saved at (seconds since 1970); newest last.
    list: Vec<(String, f64, u64)>,
    changed: bool,
}

impl Positions {
    pub fn load() -> Positions {
        let file = rivet_xdg::places::state_home().join("rivet-play/positions");
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        Positions { file: Some(file), ..Positions::parse(&text) }
    }

    pub fn parse(text: &str) -> Positions {
        let mut list: Vec<(String, f64, u64)> = text
            .lines()
            .filter_map(|l| {
                let mut w = l.split_whitespace();
                let key = w.next()?.to_string();
                let pos = w.next()?.parse().ok()?;
                let at = w.next().and_then(|a| a.parse().ok()).unwrap_or(0);
                Some((key, pos, at))
            })
            .collect();
        list.sort_by_key(|e| e.2);
        Positions { file: None, list, changed: false }
    }

    pub fn text(&self) -> String {
        self.list.iter().map(|(k, p, a)| format!("{k} {p:.1} {a}\n")).collect()
    }

    pub fn get(&self, key: &str) -> Option<f64> {
        self.list.iter().find(|e| e.0 == key).map(|e| e.1)
    }

    /// Note where a file is; forgets it when the place isn't worth keeping (near the end).
    pub fn remember(&mut self, key: &str, pos: f64, length: f64, now: u64) {
        self.list.retain(|e| e.0 != key);
        if worth_keeping(pos, length) {
            self.list.push((key.to_string(), pos, now));
            if self.list.len() > KEEP {
                self.list.drain(..self.list.len() - KEEP);
            }
        }
        self.changed = true;
    }

    pub fn forget(&mut self, key: &str) {
        let before = self.list.len();
        self.list.retain(|e| e.0 != key);
        self.changed |= self.list.len() != before;
    }

    /// Write the file on a thread, if anything changed.
    pub fn save(&mut self) {
        if !self.changed {
            return;
        }
        self.changed = false;
        let Some(file) = self.file.clone() else { return };
        let text = self.text();
        std::thread::spawn(move || {
            if let Some(d) = file.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let tmp = file.with_extension("new");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &file);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        assert!(!worth_keeping(20.0, 600.0));
        assert!(worth_keeping(31.0, 600.0));
        assert!(!worth_keeping(580.0, 600.0));
        assert!(!worth_keeping(40.0, 50.0));
    }

    #[test]
    fn round_trip_and_limits() {
        let mut p = Positions::default();
        let k = key(Path::new("/v/film.mkv"), 1234);
        assert_ne!(k, key(Path::new("/v/film.mkv"), 1235));
        p.remember(&k, 754.25, 3600.0, 10);
        p.remember("other", 5.0, 3600.0, 11);
        assert_eq!(p.get("other"), None);
        let back = Positions::parse(&p.text());
        assert!((back.get(&k).unwrap() - 754.25).abs() < 0.1);
        for i in 0..600 {
            p.remember(&format!("k{i}"), 100.0, 1000.0, 100 + i);
        }
        assert_eq!(p.list.len(), KEEP);
        assert_eq!(p.get("k0"), None);
        assert_eq!(p.get("k599"), Some(100.0));
        p.remember("k599", 990.0, 1000.0, 1000);
        assert_eq!(p.get("k599"), None);
    }
}

//! Where each file was left: its page, zoom, view, rotation and sidebar, so it opens there next
//! time. ~/.local/state/rivet-docs/places, a tab-separated line per file, kept to the 1,000 files
//! opened last. A file that changed since (another size or time) keeps its page and nothing else.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub path: PathBuf,
    pub size: u64,
    pub modified: u64,
    pub page: usize,
    /// "width", "page", or a scale such as "1.5".
    pub zoom: String,
    pub continuous: bool,
    pub rotation: u8,
    pub sidebar: String,
    pub opened: u64,
}

const KEEP: usize = 1000;

fn file() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    base.join("rivet-docs/places")
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The file's size and when it last changed, in seconds.
pub fn stamp(path: &Path) -> (u64, u64) {
    std::fs::metadata(path).map(|m| (m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()))).unwrap_or((0, 0))
}

pub fn parse(text: &str) -> Vec<Place> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 9 {
                return None;
            }
            Some(Place {
                path: PathBuf::from(f[0]),
                size: f[1].parse().ok()?,
                modified: f[2].parse().ok()?,
                page: f[3].parse().ok()?,
                zoom: f[4].to_string(),
                continuous: f[5] != "single",
                rotation: f[6].parse::<u8>().ok()? % 4,
                sidebar: f[7].to_string(),
                opened: f[8].parse().unwrap_or(0),
            })
        })
        .collect()
}

pub fn text(places: &[Place]) -> String {
    places
        .iter()
        .filter(|p| !p.path.to_string_lossy().contains(['\t', '\n']))
        .map(|p| format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", p.path.display(), p.size, p.modified, p.page, p.zoom, if p.continuous { "continuous" } else { "single" }, p.rotation, p.sidebar, p.opened))
        .collect()
}

/// Where this file was left, if it was opened before. If it changed since, only the page.
pub fn find(path: &Path) -> Option<Place> {
    let all = parse(&std::fs::read_to_string(file()).ok()?);
    let mut p = all.into_iter().find(|p| p.path == path)?;
    let (size, modified) = stamp(path);
    if (size, modified) != (p.size, p.modified) {
        p.zoom = String::new();
        p.sidebar = String::new();
        p.rotation = 0;
    }
    Some(p)
}

/// Remember where this file was left, through a temporary file so a crash never loses the rest.
pub fn remember(mut place: Place) {
    let path = file();
    let mut all = std::fs::read_to_string(&path).map(|t| parse(&t)).unwrap_or_default();
    all.retain(|p| p.path != place.path);
    place.opened = now();
    all.insert(0, place);
    all.sort_by_key(|p| std::cmp::Reverse(p.opened));
    all.truncate(KEEP);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("new");
    if std::fs::write(&tmp, text(&all)).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let p = Place { path: "/home/sam/Guide.pdf".into(), size: 1200, modified: 99, page: 12, zoom: "1.25".into(), continuous: false, rotation: 1, sidebar: "outline".into(), opened: 5 };
        assert_eq!(parse(&text(std::slice::from_ref(&p))), vec![p]);
        assert!(parse("broken line\n").is_empty());
    }
}

//! The pictures in a folder, in the order Rivet Files shows them: by name (numbers counted as
//! numbers), size, kind or the time they were changed, either way round. Rivet Files says which
//! when it opens a picture here (`--sort`); anything else gets Files' own default, by name.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rivet_xdg::mime::MimeDb;
use rivet_xdg::order::natural_cmp;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Type,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Order {
    pub key: SortKey,
    pub desc: bool,
}

impl SortKey {
    pub fn parse(s: &str) -> Option<SortKey> {
        Some(match s {
            "name" => SortKey::Name,
            "size" => SortKey::Size,
            "type" => SortKey::Type,
            "modified" => SortKey::Modified,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub mime: String,
}

impl Item {
    pub fn of(path: &Path, db: Option<&MimeDb>) -> Item {
        let meta = std::fs::metadata(path).ok();
        Item {
            path: path.to_path_buf(),
            name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: meta.and_then(|m| m.modified().ok()),
            mime: db.and_then(|d| d.guess_by_name(path)).unwrap_or_else(|| "image/png".into()),
        }
    }
}

/// The pictures in `dir`, sorted. Hidden ones are left out, except `current` (the one opened)
/// when it is hidden itself.
pub fn list(dir: &Path, current: Option<&Path>, order: Order, db: &MimeDb) -> Vec<Item> {
    let Ok(rd) = std::fs::read_dir(dir) else { return current.map(|c| vec![Item::of(c, Some(db))]).unwrap_or_default() };
    let mut items: Vec<Item> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| rivet_media::picture::is_picture(p))
        .filter(|p| Some(p.as_path()) == current || !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
        .filter(|p| p.is_file())
        .map(|p| Item::of(&p, Some(db)))
        .collect();
    sort(&mut items, order, db);
    items
}

pub fn sort(items: &mut [Item], order: Order, db: &MimeDb) {
    items.sort_by(|a, b| {
        let o = match order.key {
            SortKey::Name => std::cmp::Ordering::Equal,
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Type => db.describe(&a.mime).cmp(&db.describe(&b.mime)),
            SortKey::Modified => a.modified.cmp(&b.modified),
        }
        .then_with(|| natural_cmp(&a.name, &b.name));
        if order.desc { o.reverse() } else { o }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, size: u64) -> Item {
        Item { path: PathBuf::from(name), name: name.into(), size, modified: None, mime: "image/png".into() }
    }

    #[test]
    fn sorted_as_files_sorts() {
        let db = MimeDb::load();
        let mut v = vec![item("beach 10.jpg", 5), item("Beach 2.jpg", 9), item("alps.jpg", 7)];
        sort(&mut v, Order::default(), &db);
        assert_eq!(v.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["alps.jpg", "Beach 2.jpg", "beach 10.jpg"]);
        sort(&mut v, Order { key: SortKey::Size, desc: true }, &db);
        assert_eq!(v.iter().map(|i| i.size).collect::<Vec<_>>(), [9, 7, 5]);
    }

    #[test]
    fn only_pictures_and_hidden_only_when_opened() {
        let dir = std::env::temp_dir().join(format!("rivet-longshot-list-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["a.png", "b.JPG", ".c.png", "notes.txt"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        let db = MimeDb::load();
        let names = |v: Vec<Item>| v.into_iter().map(|i| i.name).collect::<Vec<_>>();
        assert_eq!(names(list(&dir, None, Order::default(), &db)), ["a.png", "b.JPG"]);
        assert_eq!(names(list(&dir, Some(&dir.join(".c.png")), Order::default(), &db)), ["a.png", "b.JPG", ".c.png"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

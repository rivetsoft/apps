use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::SystemTime;

/// A file's bytes, read once and shared by everything in the process that asks for the same
/// file while it is unchanged: a font loaded by every egui context, say, is in memory once.
/// Kept only while someone holds it.
pub fn shared_file(path: &Path) -> Option<Arc<Vec<u8>>> {
    type Key = (PathBuf, Option<SystemTime>, u64);
    static FILES: Mutex<Option<HashMap<Key, Weak<Vec<u8>>>>> = Mutex::new(None);
    let meta = std::fs::metadata(path).ok()?;
    let key = (path.to_path_buf(), meta.modified().ok(), meta.len());
    let mut g = FILES.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(bytes) = map.get(&key).and_then(Weak::upgrade) {
        return Some(bytes);
    }
    map.retain(|_, w| w.strong_count() > 0);
    let bytes = Arc::new(std::fs::read(path).ok()?);
    map.insert(key, Arc::downgrade(&bytes));
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_file_is_read_once_while_held() {
        let p = std::env::temp_dir().join(format!("rivet-mem-shared-{}", std::process::id()));
        std::fs::write(&p, b"font bytes").unwrap();
        let a = shared_file(&p).unwrap();
        let b = shared_file(&p).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(a.as_slice(), b"font bytes");
        std::fs::write(&p, b"a newer font").unwrap();
        let c = shared_file(&p).unwrap();
        assert_eq!(c.as_slice(), b"a newer font");
        let _ = std::fs::remove_file(&p);
        assert!(shared_file(&p).is_none());
    }
}

//! A workspace: a folder and the files in it (skipping version control and build output), for
//! the sidebar's tree and the fuzzy file picker.

use std::path::{Path, PathBuf};

/// Folders that are never listed: version control, dependencies and build output.
const SKIP: [&str; 9] = [".git", ".hg", ".svn", "node_modules", "target", "__pycache__", ".venv", "venv", ".cache"];

/// At most this many files are listed (a huge folder stays usable).
pub const MAX_FILES: usize = 20_000;

/// Every file under `root` as a path inside it ("src/main.rs"), sorted, and whether the list
/// was cut short at [`MAX_FILES`].
pub fn scan(root: &Path) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut cut = false;
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !SKIP.contains(&name.as_str()) {
                    stack.push(e.path());
                }
            } else if ft.is_file() || ft.is_symlink() {
                if out.len() >= MAX_FILES {
                    cut = true;
                    break;
                }
                if let Ok(rel) = e.path().strip_prefix(root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    out.sort_by_key(|a| a.to_lowercase());
    (out, cut)
}

/// The folders of a file list ("src", "src/gui"), sorted.
pub fn folders(files: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in files {
        let mut p = f.as_str();
        while let Some((parent, _)) = p.rsplit_once('/') {
            if !out.iter().any(|x| x == parent) {
                out.push(parent.to_string());
            }
            p = parent;
        }
    }
    out.sort_by_key(|a| a.to_lowercase());
    out
}

/// A file's text when it can be edited here: not too big, and text (UTF-8 without NUL bytes).
pub fn read_text(p: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > 20 * 1024 * 1024 {
        return Err("it's over 20 MB".into());
    }
    let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
    if bytes.contains(&0) {
        return Err("it isn't a text file".into());
    }
    String::from_utf8(bytes).map_err(|_| "it isn't UTF-8 text".into())
}

/// Write a file without leaving it half-written (a temporary file, then a rename), keeping its
/// permissions.
pub fn write_text(p: &Path, text: &str) -> std::io::Result<()> {
    let tmp: PathBuf = p.with_file_name(format!(".{}.rivet-edit-tmp", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    std::fs::write(&tmp, text)?;
    if let Ok(m) = std::fs::metadata(p) {
        let _ = std::fs::set_permissions(&tmp, m.permissions());
    }
    std::fs::rename(&tmp, p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_skipping_build_output() {
        let t = std::env::temp_dir().join(format!("rivet-edit-ws-{}", std::process::id()));
        for f in ["src/main.rs", "src/gui/draw.rs", "README.md", "target/debug/x", ".git/HEAD", "node_modules/a/b.js"] {
            let p = t.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "x").unwrap();
        }
        let (files, cut) = scan(&t);
        assert_eq!(files, ["README.md", "src/gui/draw.rs", "src/main.rs"]);
        assert!(!cut);
        assert_eq!(folders(&files), ["src", "src/gui"]);
        std::fs::write(t.join("bin.dat"), [0u8, 1, 2]).unwrap();
        assert!(read_text(&t.join("bin.dat")).is_err());
        write_text(&t.join("README.md"), "hello").unwrap();
        assert_eq!(read_text(&t.join("README.md")).unwrap(), "hello");
        let _ = std::fs::remove_dir_all(&t);
    }
}

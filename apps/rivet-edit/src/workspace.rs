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

/// The largest file opened in a tab. The editor lays out the whole file at once, which takes
/// about 280 bytes of memory per character, so a 19 MB file runs out of memory. Until it lays
/// out only the lines on screen, larger files aren't opened. Find in Files still searches them.
const OPEN_MAX_BYTES: u64 = 8 << 20;
const OPEN_MAX_LINES: usize = 200_000;

/// A file's text to open in a tab: as `read_text`, and not more than the editor can lay out.
pub fn open_text(p: &Path) -> Result<String, String> {
    if std::fs::metadata(p).is_ok_and(|m| m.len() > OPEN_MAX_BYTES) {
        return Err("it's over 8 MB, more than Rivet Edit can open for now".into());
    }
    let text = read_text(p)?;
    if text.bytes().filter(|&b| b == b'\n').count() >= OPEN_MAX_LINES {
        return Err("it has over 200,000 lines, more than Rivet Edit can open for now".into());
    }
    Ok(text)
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

/// Move to the desktop's Trash (~/.local/share/Trash, the freedesktop way) so it can be
/// restored from any file manager. Only within one disk (a rename).
pub fn trash(src: &Path) -> Result<(), String> {
    use rivet_xdg::places;
    let tdir = places::trash_dir();
    let (files, info) = (tdir.join("files"), tdir.join("info"));
    std::fs::create_dir_all(&files).and_then(|_| std::fs::create_dir_all(&info)).map_err(|e| e.to_string())?;
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !src.is_dir() => (s.to_string(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    let mut tname = name.clone();
    let mut i = 2;
    while files.join(&tname).symlink_metadata().is_ok() || info.join(format!("{tname}.trashinfo")).exists() {
        tname = format!("{stem}.{i}{ext}");
        i += 1;
    }
    let abs = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    let date = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
    let info_path = info.join(format!("{tname}.trashinfo"));
    std::fs::write(&info_path, format!("[Trash Info]\nPath={}\nDeletionDate={date}\n", places::percent_encode_path(&abs))).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(src, files.join(&tname)) {
        let _ = std::fs::remove_file(&info_path);
        return Err(if e.raw_os_error() == Some(18) { "it's on another disk than the Trash".into() } else { e.to_string() });
    }
    Ok(())
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

    #[test]
    fn files_too_large_to_lay_out_are_not_opened() {
        let t = std::env::temp_dir().join(format!("rivet-edit-big-{}", std::process::id()));
        std::fs::create_dir_all(&t).unwrap();
        std::fs::write(t.join("lines"), "\n".repeat(OPEN_MAX_LINES)).unwrap();
        std::fs::write(t.join("bytes"), "x".repeat(OPEN_MAX_BYTES as usize + 1)).unwrap();
        std::fs::write(t.join("fine"), "a\n".repeat(OPEN_MAX_LINES - 1)).unwrap();
        assert!(open_text(&t.join("lines")).is_err());
        assert!(open_text(&t.join("bytes")).is_err());
        assert!(read_text(&t.join("bytes")).is_ok());
        assert!(open_text(&t.join("fine")).is_ok());
        let _ = std::fs::remove_dir_all(&t);
    }
}

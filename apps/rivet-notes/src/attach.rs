//! Attachments: files dropped on a note are copied into the vault's attachments folder and
//! linked from the note with an ordinary Markdown link (a picture link for pictures), relative
//! to the note so any Markdown app finds them.

use std::path::{Component, Path, PathBuf};

const PICTURES: [&str; 9] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif", "tif"];

pub fn is_picture(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| PICTURES.contains(&e.to_lowercase().as_str()))
}

/// Copy `file` into `dest` (made if needed) under a name not taken yet ("photo 2.png").
/// A file already inside `dest` is linked where it is.
pub fn copy_in(file: &Path, dest: &Path) -> std::io::Result<PathBuf> {
    if file.parent() == Some(dest) {
        return Ok(file.to_path_buf());
    }
    std::fs::create_dir_all(dest)?;
    let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let ext = file.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut to = dest.join(format!("{stem}{ext}"));
    let mut n = 2;
    while to.exists() {
        to = dest.join(format!("{stem} {n}{ext}"));
        n += 1;
    }
    std::fs::copy(file, &to)?;
    Ok(to)
}

/// `to` as a path from the folder `from` ("../Attachments/a.png"), with `/` between parts.
pub fn relative(to: &Path, from: &Path) -> String {
    let t: Vec<Component> = to.components().collect();
    let f: Vec<Component> = from.components().collect();
    let common = t.iter().zip(&f).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = std::iter::repeat_n("..".to_string(), f.len() - common).collect();
    parts.extend(t[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    parts.join("/")
}

/// The Markdown linking `file` from a note in `note_dir`: `![name](path)` for a picture, else
/// `[name](path)`. Spaces in the path are written `%20`.
pub fn markdown(file: &Path, note_dir: &Path) -> String {
    let name = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let path = relative(file, note_dir).replace(' ', "%20");
    if is_picture(file) { format!("![{name}]({path})") } else { format!("[{name}]({path})") }
}

/// `body` with `block` put in at byte `at` on lines of its own.
pub fn insert_block(body: &str, at: usize, block: &str) -> String {
    let at = at.min(body.len());
    let before = &body[..at];
    let after = &body[at..];
    let lead = if before.is_empty() || before.ends_with('\n') { "" } else { "\n" };
    let trail = if after.starts_with('\n') { "" } else { "\n" };
    format!("{before}{lead}{block}{trail}{after}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_relative_to_the_note() {
        let v = Path::new("/v");
        assert_eq!(markdown(&v.join("Attachments/My photo.png"), &v.join("Projects")), "![My photo](../Attachments/My%20photo.png)");
        assert_eq!(markdown(&v.join("Attachments/doc.pdf"), v), "[doc](Attachments/doc.pdf)");
        assert_eq!(insert_block("a\nb", 1, "X"), "a\nX\nb");
        assert_eq!(insert_block("", 0, "X"), "X\n");
        assert_eq!(insert_block("ab", 1, "X"), "a\nX\nb");
    }

    #[test]
    fn copies_without_overwriting() {
        let t = std::env::temp_dir().join(format!("rivet-attach-test-{}", std::process::id()));
        let src = t.join("in");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.png"), b"x").unwrap();
        let dest = t.join("Attachments");
        assert_eq!(copy_in(&src.join("a.png"), &dest).unwrap(), dest.join("a.png"));
        assert_eq!(copy_in(&src.join("a.png"), &dest).unwrap(), dest.join("a 2.png"));
        let _ = std::fs::remove_dir_all(&t);
    }
}

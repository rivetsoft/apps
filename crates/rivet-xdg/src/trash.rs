//! The desktop's trash (`~/.local/share/Trash`), as the freedesktop trash spec has it, so
//! other file managers can see and restore what is thrown away here, and the other way round.

use std::path::{Path, PathBuf};

use crate::places::{self, percent_encode_path, split_name, trash_dir};

/// Move `src` to the trash, with the note saying where it came from and when. `mv` does the
/// moving (an app with its own, that shows progress, passes it); the note is taken back if it
/// fails. Returns where it now is in the trash.
pub fn trash_with(src: &Path, mv: impl FnOnce(&Path, &Path) -> std::io::Result<()>) -> std::io::Result<PathBuf> {
    let tdir = trash_dir();
    let files = tdir.join("files");
    let info = tdir.join("info");
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&info)?;
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let (stem, ext) = split_name(&name, src.is_dir());
    let mut tname = name.clone();
    let mut i = 2;
    while files.join(&tname).symlink_metadata().is_ok() || info.join(format!("{tname}.trashinfo")).exists() {
        tname = format!("{stem}.{i}{ext}");
        i += 1;
    }
    let parent = src.parent().unwrap_or(Path::new("/"));
    let abs = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf()).join(&name);
    let content = format!("[Trash Info]\nPath={}\nDeletionDate={}\n", percent_encode_path(&abs), local_now());
    let info_path = info.join(format!("{tname}.trashinfo"));
    std::fs::write(&info_path, content)?;
    let trashed = files.join(&tname);
    if let Err(e) = mv(src, &trashed) {
        let _ = std::fs::remove_file(&info_path);
        return Err(e);
    }
    Ok(trashed)
}

/// Move `src` to the trash: renamed into it, or copied and then removed when it is on another
/// drive.
pub fn trash(src: &Path) -> std::io::Result<PathBuf> {
    trash_with(src, |from, to| match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy_all(from, to)?;
            remove_all(from)
        }
        Err(e) => Err(e),
    })
}

/// Delete everything in the trash, for good.
pub fn empty() -> std::io::Result<()> {
    let t = trash_dir();
    for sub in ["files", "info"] {
        if let Ok(rd) = std::fs::read_dir(t.join(sub)) {
            for e in rd.flatten() {
                let _ = remove_all(&e.path());
            }
        }
    }
    Ok(())
}

/// Whether anything is in the trash.
pub fn is_empty() -> bool {
    places::trash_is_empty()
}

fn remove_all(p: &Path) -> std::io::Result<()> {
    if std::fs::symlink_metadata(p)?.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) }
}

fn copy_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(src)?, dst)
    } else if meta.is_dir() {
        std::fs::create_dir(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_all(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst).map(|_| ())
    }
}

/// The local time now as the spec writes it: `2026-11-23T21:43:05`.
fn local_now() -> String {
    // SAFETY: localtime_r only writes into the tm it is given.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_time_reads_as_the_spec_writes_it() {
        let t = super::local_now();
        assert_eq!(t.len(), 19);
        assert_eq!(&t[10..11], "T");
    }
}

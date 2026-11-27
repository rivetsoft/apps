//! Compress and extract, using the archive tools the system already has
//! (zip/unzip, tar with gzip/xz/zstd, 7z). Runs in the background like copying,
//! shows progress in the status bar, and can be undone. The work itself is the
//! `rivet-archive` crate's, which Rivet Vice uses too.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub use rivet_archive::{Format, is_archive, is_archive_name, suggest_name};

use crate::fsops::{self, OpKind, Progress};

/// Make `archive` from `sources` (all in one folder).
pub fn compress(sources: &[PathBuf], archive: &Path, format: Format, progress: &Progress) -> Result<(), String> {
    rivet_archive::compress(sources, archive, format, &progress.cancel)
}

/// Unpack `archive` into `into`: a single top-level item lands there as itself, anything
/// else in a folder named after the archive. Returns what was made.
pub fn extract(archive: &Path, into: &Path, progress: &Progress) -> Result<PathBuf, String> {
    rivet_archive::extract(archive, into, &progress.cancel, &fsops::unique_target)
}

/// Rivet Vice, the archive manager, when it is installed: beside this program (the same
/// build or package), else on the path. Looked for once.
pub fn vice() -> Option<&'static Path> {
    static FOUND: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| rivet_xdg::launch::rivet_program("rivet-vice"))
        .as_deref()
}

/// Open an archive in Rivet Vice, to see what is in it before taking anything out.
pub fn open_in_vice(archive: &Path) -> Result<(), String> {
    let vice = vice().ok_or("Rivet Vice isn't installed")?;
    let mut child = std::process::Command::new(vice).arg(archive).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().map_err(|e| format!("Rivet Vice couldn't be started: {e}"))?;
    // Waited for on a thread of its own, so it doesn't linger as a finished process.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Compress in the background. `archive` is the full path of the archive to make.
pub fn spawn_compress(sources: Vec<PathBuf>, archive: PathBuf, format: Format) -> Arc<Progress> {
    let p = Arc::new(Progress::blank(OpKind::Compress, 1, 0));
    let pp = p.clone();
    std::thread::spawn(move || {
        *pp.current.lock().unwrap_or_else(|e| e.into_inner()) = name_of(&archive);
        match compress(&sources, &archive, format, &pp) {
            Ok(()) => {
                pp.created.lock().unwrap_or_else(|e| e.into_inner()).push(archive.clone());
                pp.done.lock().unwrap_or_else(|e| e.into_inner()).push((sources[0].clone(), archive));
            }
            Err(e) if e == "Cancelled" => {}
            Err(e) => pp.errors.lock().unwrap_or_else(|e| e.into_inner()).push(format!("{}: {e}", name_of(&archive))),
        }
        pp.done_items.fetch_add(1, Ordering::Relaxed);
        pp.finished.store(true, Ordering::Release);
    });
    p
}

/// Extract each archive into `into` in the background.
pub fn spawn_extract(archives: Vec<PathBuf>, into: PathBuf) -> Arc<Progress> {
    let p = Arc::new(Progress::blank(OpKind::Extract, archives.len(), 0));
    let pp = p.clone();
    std::thread::spawn(move || {
        for a in archives {
            if pp.cancel.load(Ordering::Relaxed) {
                break;
            }
            *pp.current.lock().unwrap_or_else(|e| e.into_inner()) = name_of(&a);
            match extract(&a, &into, &pp) {
                Ok(made) => {
                    pp.created.lock().unwrap_or_else(|e| e.into_inner()).push(made.clone());
                    pp.done.lock().unwrap_or_else(|e| e.into_inner()).push((a, made));
                }
                Err(e) if e == "Cancelled" => {}
                Err(e) => pp.errors.lock().unwrap_or_else(|e| e.into_inner()).push(format!("{}: {e}", name_of(&a))),
            }
            pp.done_items.fetch_add(1, Ordering::Relaxed);
        }
        pp.finished.store(true, Ordering::Release);
    });
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn wait(p: &Progress) {
        while !p.finished.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Compress then extract every available format; a folder of loose files
    /// comes back in its own folder, a single folder comes back as itself.
    #[test]
    fn round_trip() {
        let base = std::env::temp_dir().join(format!("rivet-archive-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        std::fs::create_dir_all(src.join("Folder/inner")).unwrap();
        std::fs::write(src.join("Folder/inner/a.txt"), "hello").unwrap();
        std::fs::write(src.join("b.txt"), "world").unwrap();
        for f in Format::ALL.into_iter().filter(|f| f.available()) {
            let out = base.join(format!("out-{}", f.ext().trim_matches('.').replace('.', "-")));
            std::fs::create_dir_all(&out).unwrap();
            // one folder
            let arc = out.join(format!("Folder{}", f.ext()));
            let p = spawn_compress(vec![src.join("Folder")], arc.clone(), f);
            wait(&p);
            assert!(p.errors.lock().unwrap_or_else(|e| e.into_inner()).is_empty(), "{f:?}: {:?}", p.errors.lock().unwrap_or_else(|e| e.into_inner()));
            let x = base.join(format!("x1-{}", f.ext().trim_matches('.').replace('.', "-")));
            std::fs::create_dir_all(&x).unwrap();
            let e = spawn_extract(vec![arc], x.clone());
            wait(&e);
            assert!(e.errors.lock().unwrap_or_else(|e| e.into_inner()).is_empty(), "{f:?}: {:?}", e.errors.lock().unwrap_or_else(|e| e.into_inner()));
            assert_eq!(std::fs::read_to_string(x.join("Folder/inner/a.txt")).unwrap(), "hello", "{f:?}");
            // two items → a folder named after the archive
            let arc2 = out.join(format!("Both{}", f.ext()));
            let p = spawn_compress(vec![src.join("Folder"), src.join("b.txt")], arc2.clone(), f);
            wait(&p);
            let e = spawn_extract(vec![arc2], x.clone());
            wait(&e);
            assert_eq!(std::fs::read_to_string(x.join("Both/b.txt")).unwrap(), "world", "{f:?}");
            // no temporary folders left behind
            assert!(std::fs::read_dir(&x).unwrap().flatten().all(|d| !d.file_name().to_string_lossy().starts_with(".rivet-extract")));
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}

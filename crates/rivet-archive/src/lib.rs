//! Archives, with no user interface: what is in one, compressing and extracting, using the
//! archive tools the system already has (zip/unzip, tar with gzip/xz/zstd, 7z). Rivet Files'
//! Compress and Extract and Rivet Vice's window both work through this, so they open and make
//! the same things. Everything here runs a program and waits: call it from a thread, and set
//! the stop flag to end it early (it then fails with "Cancelled").

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

mod list;
pub use list::{Entry, extract_some, list};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Zip,
    TarGz,
    TarXz,
    TarZst,
    SevenZ,
}

impl Format {
    pub const ALL: [Format; 5] = [Format::Zip, Format::TarGz, Format::TarXz, Format::TarZst, Format::SevenZ];

    pub fn ext(self) -> &'static str {
        match self {
            Format::Zip => ".zip",
            Format::TarGz => ".tar.gz",
            Format::TarXz => ".tar.xz",
            Format::TarZst => ".tar.zst",
            Format::SevenZ => ".7z",
        }
    }

    /// A short hint shown under the choice.
    pub fn hint(self) -> &'static str {
        match self {
            Format::Zip => "Opens almost anywhere, including Windows and macOS",
            Format::TarGz => "The common Linux format, fast",
            Format::TarXz => "Smaller files, slower to make",
            Format::TarZst => "Small and very fast",
            Format::SevenZ => "Smallest files, needs 7-Zip to open",
        }
    }

    pub fn available(self) -> bool {
        match self {
            Format::Zip => has("zip"),
            Format::TarGz => has("tar") && has("gzip"),
            Format::TarXz => has("tar") && has("xz"),
            Format::TarZst => has("tar") && has("zstd"),
            Format::SevenZ => seven_zip().is_some(),
        }
    }

    pub fn from_id(id: &str) -> Format {
        Format::ALL.into_iter().find(|f| f.ext().trim_start_matches('.') == id).unwrap_or(Format::Zip)
    }
}

/// Whether a program is on the path.
pub fn has(bin: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}

pub(crate) fn seven_zip() -> Option<&'static str> {
    ["7z", "7zz", "7za"].into_iter().find(|b| has(b))
}

/// The archive endings Rivet can open, longest first so ".tar.gz" wins over ".gz".
const ARCHIVE_EXTS: &[&str] = &[
    ".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst", ".tar.lz", ".tar.lzma", ".tar.z", ".tgz", ".txz", ".tbz", ".tbz2", ".tzst", ".tar",
    ".zip", ".jar", ".7z", ".rar", ".gz", ".xz", ".bz2", ".zst", ".lz", ".lzma", ".iso", ".cab", ".deb", ".rpm",
];

/// The archive ending of a file name, if it is an archive.
pub fn archive_ext(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    ARCHIVE_EXTS.iter().copied().find(|e| lower.ends_with(e) && lower.len() > e.len())
}

/// Whether a file name has an archive ending (no disk access).
pub fn is_archive_name(name: &str) -> bool {
    archive_ext(name).is_some()
}

pub fn is_archive(path: &Path) -> bool {
    path.is_file() && path.file_name().and_then(|n| n.to_str()).and_then(archive_ext).is_some()
}

/// "photos.tar.gz" → "photos".
pub fn stem(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    match archive_ext(&name) {
        Some(e) => name[..name.len() - e.len()].to_string(),
        None => name,
    }
}

/// A suggested archive name (without ending) for these items.
pub fn suggest_name(paths: &[PathBuf]) -> String {
    match paths {
        [one] => {
            let name = one.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if one.is_dir() {
                name
            } else {
                one.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or(name)
            }
        }
        _ => paths
            .first()
            .and_then(|p| p.parent())
            .and_then(|d| d.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Archive".into()),
    }
}

/// Run a tool, stopping it if the operation is cancelled. Errors carry the tool's message.
pub(crate) fn run(mut cmd: Command, cancel: &AtomicBool) -> Result<(), String> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Could not start the archive tool: {e}"))?;
    let stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            use std::io::Read;
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let msg = reader.join().unwrap_or_default();
            if status.success() {
                return Ok(());
            }
            let line = msg.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("The archive tool failed").to_string();
            return Err(line);
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cancelled".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Make `archive` from `sources` (all in one folder).
pub fn compress(sources: &[PathBuf], archive: &Path, format: Format, cancel: &AtomicBool) -> Result<(), String> {
    let dir = sources.first().and_then(|p| p.parent()).ok_or("Nothing to compress")?;
    let names: Vec<String> = sources.iter().filter_map(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).collect();
    let mut cmd;
    match format {
        Format::Zip => {
            cmd = Command::new("zip");
            cmd.args(["-r", "-q", "-y"]).arg(archive).arg("--").args(&names);
        }
        Format::SevenZ => {
            cmd = Command::new(seven_zip().ok_or("7-Zip is not installed")?);
            cmd.args(["a", "-bd", "-y"]).arg(archive).arg("--").args(&names);
        }
        Format::TarGz | Format::TarXz | Format::TarZst => {
            cmd = Command::new("tar");
            let flag = match format {
                Format::TarGz => "--gzip",
                Format::TarXz => "--xz",
                _ => "--zstd",
            };
            cmd.args([flag, "-cf"]).arg(archive).arg("--").args(&names);
        }
    }
    cmd.current_dir(dir);
    let r = run(cmd, cancel);
    if r.is_err() {
        let _ = std::fs::remove_file(archive);
    }
    r
}

/// Unpack `archive` into `into`. Like most file managers: an archive with a single
/// top-level item puts just that item there; anything else gets its own folder
/// named after the archive. Returns what was made. `free` gives a name that isn't taken, in a
/// folder, for something that would land on one that is (`free_name` is the plain one).
pub fn extract(archive: &Path, into: &Path, cancel: &AtomicBool, free: &dyn Fn(&Path, &Path) -> PathBuf) -> Result<PathBuf, String> {
    let name = archive.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let ext = archive_ext(&name).ok_or("This is not an archive Rivet knows")?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let tmp = into.join(format!(".rivet-extract-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("Could not write here: {e}"))?;
    let result = (|| {
        let single_stream = matches!(ext, ".gz" | ".xz" | ".bz2" | ".zst" | ".lz" | ".lzma");
        if single_stream {
            // A single compressed file (not a tar): decompress it to a file.
            let tool = match ext {
                ".gz" => "gzip",
                ".xz" | ".lzma" => "xz",
                ".bz2" => "bzip2",
                ".zst" => "zstd",
                _ => "lzip",
            };
            let out = tmp.join(stem(archive));
            let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            let mut cmd = Command::new(tool);
            cmd.arg("-dc").arg(archive).stdout(file).stdin(Stdio::null()).stderr(Stdio::piped());
            let status = cmd.status().map_err(|e| format!("Could not start {tool}: {e}"))?;
            return if status.success() { Ok(()) } else { Err(format!("{tool} could not read the file")) };
        }
        let is_tar = ext.starts_with(".tar") || matches!(ext, ".tgz" | ".txz" | ".tbz" | ".tbz2" | ".tzst");
        let mut cmd;
        if is_tar && has("tar") {
            cmd = Command::new("tar");
            cmd.arg("-xf").arg(archive).arg("-C").arg(&tmp);
        } else if matches!(ext, ".zip" | ".jar") && has("unzip") {
            cmd = Command::new("unzip");
            cmd.args(["-q", "-o"]).arg(archive).arg("-d").arg(&tmp);
        } else if let Some(z) = seven_zip() {
            cmd = Command::new(z);
            cmd.args(["x", "-bd", "-y"]).arg(format!("-o{}", tmp.display())).arg(archive);
        } else if has("bsdtar") {
            cmd = Command::new("bsdtar");
            cmd.arg("-xf").arg(archive).arg("-C").arg(&tmp);
        } else {
            return Err("No tool to open this kind of archive is installed (try installing 7-Zip)".into());
        }
        run(cmd, cancel)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let items: Vec<PathBuf> = std::fs::read_dir(&tmp).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).collect();
    let made = if items.len() == 1 {
        let only = &items[0];
        let target = free(into, only);
        std::fs::rename(only, &target).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir(&tmp);
        target
    } else {
        let target = free(into, &into.join(stem(archive)));
        std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
        target
    };
    Ok(made)
}


/// A name that isn't taken in `dir` for what `src` is called: its own, else with a number
/// after it ("Photos 2", "notes 2.txt").
pub fn free_name(dir: &Path, src: &Path) -> PathBuf {
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let taken = |p: &Path| p.exists() || p.symlink_metadata().is_ok();
    let first = dir.join(&name);
    if !taken(&first) {
        return first;
    }
    // The ending stays last, so the file still opens with what it should; an archive's whole
    // ending (".tar.gz") counts as one.
    let (stem, ext) = match archive_ext(&name) {
        Some(e) => name.split_at(name.len() - e.len()),
        None if src.is_dir() => (name.as_str(), ""),
        None => name.rfind('.').filter(|i| *i > 0).map_or((name.as_str(), ""), |i| name.split_at(i)),
    };
    (2..).map(|i| dir.join(format!("{stem} {i}{ext}"))).find(|p| !taken(p)).unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(stem(Path::new("/x/photos.tar.gz")), "photos");
        assert_eq!(stem(Path::new("/x/a.b.zip")), "a.b");
        assert!(archive_ext("x.TGZ").is_some());
        assert!(archive_ext("notes.txt").is_none());
        assert!(archive_ext(".zip").is_none());
        assert_eq!(suggest_name(&[PathBuf::from("/x/report.pdf")]), "report");
    }

    /// Compress, list and extract every available format; a folder of loose files comes back
    /// in its own folder, a single folder comes back as itself.
    #[test]
    fn round_trip() {
        let go = AtomicBool::new(false);
        let base = std::env::temp_dir().join(format!("rivet-archive-crate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        std::fs::create_dir_all(src.join("Folder/inner")).unwrap();
        std::fs::write(src.join("Folder/inner/a.txt"), "hello").unwrap();
        std::fs::write(src.join("Folder/two words.txt"), "spaced").unwrap();
        std::fs::write(src.join("b.txt"), "world").unwrap();
        for f in Format::ALL.into_iter().filter(|f| f.available()) {
            let tag = f.ext().trim_matches('.').replace('.', "-");
            let out = base.join(format!("out-{tag}"));
            std::fs::create_dir_all(&out).unwrap();
            // One folder comes back as itself.
            let arc = out.join(format!("Folder{}", f.ext()));
            compress(&[src.join("Folder")], &arc, f, &go).unwrap_or_else(|e| panic!("{f:?}: {e}"));
            let x = base.join(format!("x-{tag}"));
            std::fs::create_dir_all(&x).unwrap();
            assert_eq!(extract(&arc, &x, &go, &free_name).unwrap(), x.join("Folder"), "{f:?}");
            assert_eq!(std::fs::read_to_string(x.join("Folder/inner/a.txt")).unwrap(), "hello", "{f:?}");
            // A second time, beside the first.
            assert_eq!(extract(&arc, &x, &go, &free_name).unwrap(), x.join("Folder 2"), "{f:?}");
            // What is in it, folders and files, with sizes.
            let l = list(&arc, &go).unwrap_or_else(|e| panic!("{f:?}: {e}"));
            let find = |p: &str| l.iter().find(|e| e.path == p).unwrap_or_else(|| panic!("{f:?}: no {p} in {l:?}"));
            assert!(find("Folder").dir && find("Folder/inner").dir, "{f:?}");
            assert_eq!((find("Folder/inner/a.txt").size, find("Folder/inner/a.txt").dir), (5, false), "{f:?}");
            assert_eq!(find("Folder/two words.txt").size, 6, "{f:?}");
            assert!(find("Folder/inner/a.txt").modified > 1_000_000_000, "{f:?}: {:?}", find("Folder/inner/a.txt"));
            // Only some of it.
            let some = base.join(format!("some-{tag}"));
            std::fs::create_dir_all(&some).unwrap();
            extract_some(&arc, &["Folder/two words.txt".to_string()], &some, &go).unwrap_or_else(|e| panic!("{f:?}: {e}"));
            assert_eq!(std::fs::read_to_string(some.join("Folder/two words.txt")).unwrap(), "spaced", "{f:?}");
            assert!(!some.join("Folder/inner").exists(), "{f:?}");
            // Two items come back in a folder named after the archive.
            let arc2 = out.join(format!("Both{}", f.ext()));
            compress(&[src.join("Folder"), src.join("b.txt")], &arc2, f, &go).unwrap();
            extract(&arc2, &x, &go, &free_name).unwrap();
            assert_eq!(std::fs::read_to_string(x.join("Both/b.txt")).unwrap(), "world", "{f:?}");
            // No temporary folders left behind.
            assert!(std::fs::read_dir(&x).unwrap().flatten().all(|d| !d.file_name().to_string_lossy().starts_with(".rivet-extract")));
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn stopping_ends_it() {
        let stop = AtomicBool::new(true);
        let dir = std::env::temp_dir();
        assert_eq!(compress(&[dir.join("no-such-file-for-rivet")], &dir.join("rivet-never.zip"), Format::Zip, &stop).err().as_deref().map(|e| e == "Cancelled" || !e.is_empty()), Some(true));
    }
}

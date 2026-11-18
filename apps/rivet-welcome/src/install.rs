//! Installing or updating an app from its release: download the archive and its checksum, check
//! one against the other, unpack it, and run the installer that comes in it (the same
//! `install.sh` people run by hand: the program goes in ~/.local/bin and the app in the
//! launcher). Everything happens in the cache folder, which is cleared afterwards.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::catalog::{self, Entry};
use crate::releases::{Release, Version};

/// Where a job is.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Downloading { got: u64, total: u64 },
    Checking,
    Installing,
    Done(Version),
    Failed(String),
}

impl Stage {
    pub fn finished(&self) -> bool {
        matches!(self, Stage::Done(_) | Stage::Failed(_))
    }

    /// For a progress bar: 0..1 (the download is most of the time).
    pub fn fraction(&self) -> f32 {
        match self {
            Stage::Downloading { got, total } if *total > 0 => {
                0.85 * (*got as f32 / *total as f32).min(1.0)
            }
            Stage::Downloading { .. } => 0.0,
            Stage::Checking => 0.88,
            Stage::Installing => 0.94,
            Stage::Done(_) | Stage::Failed(_) => 1.0,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Stage::Downloading { got, total } if *total > 0 => {
                format!("Downloading… {} of {}", size(*got), size(*total))
            }
            Stage::Downloading { .. } => "Downloading…".into(),
            Stage::Checking => "Checking the download…".into(),
            Stage::Installing => "Installing…".into(),
            Stage::Done(v) => format!("Installed {v}"),
            Stage::Failed(e) => e.clone(),
        }
    }
}

/// "4.2 MB".
pub fn size(b: u64) -> String {
    match b {
        0..1_000 => format!("{b} B"),
        1_000..1_000_000 => format!("{:.0} kB", b as f64 / 1e3),
        _ => format!("{:.1} MB", b as f64 / 1e6),
    }
}

pub type Progress = Arc<Mutex<Stage>>;

/// Install this release on a thread of its own; `wake` is called whenever the stage changes.
pub fn start(e: &Entry, r: &Release, wake: impl Fn() + Send + 'static) -> Progress {
    let p: Progress = Arc::new(Mutex::new(Stage::Downloading { got: 0, total: 0 }));
    let (e, r, p2) = (e.clone(), r.clone(), p.clone());
    std::thread::Builder::new()
        .name("rivet-welcome install".into())
        .spawn(move || {
            let set = |s: Stage| {
                if let Ok(mut g) = p2.lock() {
                    *g = s;
                }
                wake();
            };
            let end = match run(&e, &r, &set) {
                Ok(v) => Stage::Done(v),
                Err(err) => Stage::Failed(err),
            };
            set(end);
        })
        .ok();
    p
}

fn work_dir(e: &Entry) -> PathBuf {
    rivet_xdg::places::cache_home().join(format!("rivet-welcome/downloads/{}", e.id))
}

/// The whole job, blocking. `set` hears each stage. The installed version when it worked.
pub fn run(e: &Entry, r: &Release, set: &dyn Fn(Stage)) -> Result<Version, String> {
    let want = catalog::tag_version(&e.tag_prefix, &r.tag)
        .ok_or("that release isn't one of this app's")?;
    let (asset, sum) = catalog::archive(e, r).ok_or_else(|| {
        format!(
            "{} has no download for this computer ({})",
            r.name,
            catalog::arch()
        )
    })?;
    let dir = work_dir(e);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("couldn't make {}: {err}", dir.display()))?;
    let result = (|| {
        let file = dir.join(&asset.name);
        download(&asset.url, &file, asset.size, set)?;
        set(Stage::Checking);
        if let Some(sum) = sum {
            let sum_file = dir.join(&sum.name);
            download(&sum.url, &sum_file, 0, &|_| {})?;
            verify(&file, &sum_file)?;
        }
        set(Stage::Installing);
        let unpacked = dir.join("unpacked");
        std::fs::create_dir_all(&unpacked).map_err(|err| err.to_string())?;
        quiet(
            Command::new("tar")
                .arg("-xzf")
                .arg(&file)
                .arg("-C")
                .arg(&unpacked),
            "unpack the download",
        )?;
        let installer =
            find_installer(&unpacked).ok_or("the download has no installer (install.sh) in it")?;
        let folder = installer.parent().unwrap_or(&unpacked).to_path_buf();
        quiet(
            Command::new("sh").arg(&installer).current_dir(&folder),
            "install it",
        )?;
        // The program that's there now should be the new one.
        let bin = crate::local::bin_home().join(&e.bin);
        match crate::local::version_of(&bin) {
            Some(v) if v == want => Ok(v),
            Some(v) => Err(format!(
                "installed, but it says it's {v} rather than {want}"
            )),
            None => Err(format!("installed, but {} doesn't start", bin.display())),
        }
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Download with curl, telling `set` how much has come (by watching the file grow).
fn download(url: &str, to: &Path, total: u64, set: &dyn Fn(Stage)) -> Result<(), String> {
    let mut child = Command::new("curl")
        .args(["-fsSL", "--retry", "2", "--connect-timeout", "20", "-o"])
        .arg(to)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                "curl isn't installed (it's used to download)".to_string()
            } else {
                format!("couldn't run curl: {err}")
            }
        })?;
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|err| err.to_string())? {
            break s;
        }
        let got = std::fs::metadata(to).map(|m| m.len()).unwrap_or(0);
        set(Stage::Downloading { got, total });
        std::thread::sleep(Duration::from_millis(120));
    };
    if !status.success() {
        let mut err = String::new();
        if let Some(e) = child.stderr.as_mut() {
            let _ = std::io::Read::read_to_string(e, &mut err);
        }
        let line = err
            .lines()
            .last()
            .unwrap_or("")
            .trim()
            .trim_start_matches("curl: ")
            .to_string();
        return Err(if line.is_empty() {
            "the download failed".into()
        } else {
            format!("the download failed ({line})")
        });
    }
    set(Stage::Downloading { got: total, total });
    Ok(())
}

/// The archive's SHA-256 against the one published with it.
fn verify(file: &Path, sum_file: &Path) -> Result<(), String> {
    let want = std::fs::read_to_string(sum_file)
        .ok()
        .and_then(|t| t.split_whitespace().next().map(str::to_lowercase))
        .ok_or("the checksum file is empty")?;
    let out = Command::new("sha256sum")
        .arg(file)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("couldn't run sha256sum: {err}"))?;
    let got = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    if got.is_empty() || got != want {
        return Err(
            "the download doesn't match its checksum (it may be damaged); nothing was installed"
                .into(),
        );
    }
    Ok(())
}

/// The installer: in the archive's one folder, or at its top.
fn find_installer(dir: &Path) -> Option<PathBuf> {
    let top = dir.join("install.sh");
    if top.is_file() {
        return Some(top);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|d| d.path().join("install.sh"))
        .find(|p| p.is_file())
}

/// Run a command; on failure, its last line of output says why.
fn quiet(cmd: &mut Command, what: &str) -> Result<(), String> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("couldn't {what}: {err}"))?;
    if out.status.success() {
        return Ok(());
    }
    let text =
        String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string();
    Err(if line.is_empty() {
        format!("couldn't {what}")
    } else {
        format!("couldn't {what}: {line}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums() {
        let dir = std::env::temp_dir().join(format!("rivet-welcome-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.tar.gz");
        std::fs::write(&f, b"hello").unwrap();
        let s = dir.join("a.tar.gz.sha256");
        std::fs::write(
            &s,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  a.tar.gz\n",
        )
        .unwrap();
        assert!(verify(&f, &s).is_ok());
        std::fs::write(&s, "00  a.tar.gz\n").unwrap();
        assert!(verify(&f, &s).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stage_labels() {
        assert_eq!(
            Stage::Downloading {
                got: 2_500_000,
                total: 9_000_000
            }
            .label(),
            "Downloading… 2.5 MB of 9.0 MB"
        );
        assert!(Stage::Checking.fraction() > Stage::Downloading { got: 1, total: 2 }.fraction());
    }
}

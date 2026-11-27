//! Installing or updating an app from its release: download the archive and its checksum, check
//! one against the other, unpack it, and run the installer that comes in it (the same
//! `install.sh` people run by hand: the program goes in ~/.local/bin and the app in the
//! launcher). Everything happens in the cache folder, which is cleared afterwards.
//!
//! What can go wrong is handled on the way: a dropped connection is tried again and carries on
//! from where it stopped (a half download is kept for the next try too), a download that doesn't
//! match its checksum is thrown away, a full disk or a folder that isn't the user's is found
//! before anything is touched, and an installer that fails puts the previous version back.
//! One app is installed by one Rivet Welcome at a time (a lock in the cache folder), so the tray
//! and the window can't both be at it.

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::catalog::{self, Entry};
use crate::local;
use crate::releases::{Asset, Release, Version};

/// Where a job is.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Downloading {
        got: u64,
        total: u64,
    },
    /// The connection dropped: waiting to try again (`attempt` of `of`), from `got`.
    Retrying {
        attempt: u32,
        of: u32,
        got: u64,
        total: u64,
    },
    Checking,
    Installing,
    /// The installer failed: the previous version is going back.
    RollingBack,
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
            Stage::Downloading { got, total } | Stage::Retrying { got, total, .. }
                if *total > 0 =>
            {
                0.85 * (*got as f32 / *total as f32).min(1.0)
            }
            Stage::Downloading { .. } | Stage::Retrying { .. } => 0.0,
            Stage::Checking => 0.88,
            Stage::Installing => 0.94,
            Stage::RollingBack => 0.97,
            Stage::Done(_) | Stage::Failed(_) => 1.0,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Stage::Downloading { got, total } if *total > 0 => {
                format!("Downloading… {} of {}", size(*got), size(*total))
            }
            Stage::Downloading { .. } => "Downloading…".into(),
            Stage::Retrying { attempt, of, .. } => {
                format!("The connection dropped. Trying again ({attempt} of {of})…")
            }
            Stage::Checking => "Checking the download…".into(),
            Stage::Installing => "Installing…".into(),
            Stage::RollingBack => "That didn't work. Putting the previous version back…".into(),
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

/// Held while an app is being installed, updated or removed. It goes when this is dropped, or
/// when the program holding it ends however it ends.
pub struct Lock(#[allow(dead_code)] std::fs::File);

/// The app's lock, or why it can't be had (another Rivet Welcome is working on that app).
pub fn lock(id: &str) -> Result<Lock, String> {
    let dir = rivet_xdg::places::cache_home().join("rivet-welcome/locks");
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("couldn't make {}: {err}", dir.display()))?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(format!("{id}.lock")))
        .map_err(|err| format!("couldn't make its lock file: {err}"))?;
    // SAFETY: flock on a file descriptor this File owns.
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(
            "another Rivet Welcome is installing or removing it right now; try again when that's done"
                .into(),
        );
    }
    Ok(Lock(f))
}

fn work_dir(e: &Entry) -> PathBuf {
    rivet_xdg::places::cache_home().join(format!("rivet-welcome/downloads/{}", e.id))
}

/// How many times a dropped download is tried before giving up.
const TRIES: u32 = 4;

/// The longest an app's installer is given (a release's takes a second or two).
const INSTALLER_TIME: Duration = Duration::from_secs(300);

/// The whole job, blocking. `set` hears each stage. The installed version when it worked.
pub fn run(e: &Entry, r: &Release, set: &dyn Fn(Stage)) -> Result<Version, String> {
    let want = catalog::tag_version(&e.tag_prefix, &r.tag)
        .ok_or("that release isn't one of this app's")?;
    let (asset, sum) = catalog::archive(e, r).ok_or_else(|| {
        format!(
            "{} has no download for this computer ({}), so nothing was installed",
            r.name,
            catalog::arch()
        )
    })?;
    let sum = sum.ok_or_else(|| {
        format!(
            "{} has no checksum to check the download against, so nothing was installed",
            r.name
        )
    })?;
    let _lock = lock(&e.id)?;
    let dir = work_dir(e);
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("couldn't make {}: {err}", dir.display()))?;
    // Whatever is there from another release goes; a half download of this one stays, to carry on.
    if let Ok(list) = std::fs::read_dir(&dir) {
        for f in list.flatten() {
            if !f.file_name().to_string_lossy().starts_with(&asset.name) {
                let p = f.path();
                let _ = std::fs::remove_dir_all(&p).or_else(|_| std::fs::remove_file(&p));
            }
        }
    }
    let bin_dir = local::bin_home();
    room(&dir, &bin_dir, asset.size)?;
    let file = dir.join(&asset.name);
    // A download that stopped keeps what came, so the next try carries on from there.
    fetch(asset, &file, set)?;
    let result = install_from(e, &dir, &file, sum, &want, set);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Checked, unpacked and installed, with the previous version put back if the installer fails.
fn install_from(
    e: &Entry,
    dir: &Path,
    file: &Path,
    sum: &Asset,
    want: &Version,
    set: &dyn Fn(Stage),
) -> Result<Version, String> {
    set(Stage::Checking);
    let sum_file = dir.join(&sum.name);
    fetch(sum, &sum_file, &|_| {})?;
    verify(file, &sum_file)?;
    set(Stage::Installing);
    let unpacked = dir.join("unpacked");
    std::fs::create_dir_all(&unpacked).map_err(|err| err.to_string())?;
    quiet(
        Command::new("tar")
            .arg("-xzf")
            .arg(file)
            .arg("-C")
            .arg(&unpacked),
        "unpack the download",
    )?;
    let installer =
        find_installer(&unpacked).ok_or("the download has no installer (install.sh) in it")?;
    let folder = installer.parent().unwrap_or(&unpacked).to_path_buf();
    let backup = Backup::take(e, &dir.join("backup"))?;
    let bin = local::bin_home().join(&e.bin);
    let outcome = run_installer(&installer, &folder, &dir.join("install.log")).and_then(|()| {
        // The program that's there now should be the new one.
        match local::version_of(&bin) {
            Some(v) if v == *want => Ok(v),
            Some(v) => Err(format!(
                "it installed, but the program says it's {v} rather than {want}"
            )),
            None => Err(format!("it installed, but {} doesn't start", bin.display())),
        }
    });
    outcome.map_err(|err| {
        set(Stage::RollingBack);
        backup.restore(e, err)
    })
}

/// Free bytes for this user on the disk that holds `p`.
fn free_bytes(p: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs fills the struct it's given; the path is a valid C string.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0)
        .then(|| s.f_bavail as u64 * s.f_frsize as u64)
}

fn device(p: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(p).ok().map(|m| m.dev())
}

/// Space and permission, checked before anything is downloaded: the download and what's in it
/// in the cache (about four times the archive), and the program where it goes (three times).
fn room(cache: &Path, bin_dir: &Path, archive: u64) -> Result<(), String> {
    let not_yours = || {
        format!(
            "{} isn't yours to change, so nothing was installed. Rivet Welcome installs apps for you alone, in a folder you own",
            bin_dir.display()
        )
    };
    std::fs::create_dir_all(bin_dir).map_err(|_| not_yours())?;
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(bin_dir.as_os_str().as_bytes()).map_err(|_| not_yours())?;
    // SAFETY: access only reads the path.
    if unsafe { libc::access(c.as_ptr(), libc::W_OK) } != 0 {
        return Err(not_yours());
    }
    const SLACK: u64 = 8_000_000;
    let (need_cache, need_bin) = (archive * 4 + SLACK, archive * 3 + SLACK);
    let short = |dir: &Path, need: u64, free: u64| {
        format!(
            "there isn't enough free space in {}, so nothing was installed: it needs about {} and {} is free",
            dir.display(),
            size(need),
            size(free)
        )
    };
    if device(cache).is_some() && device(cache) == device(bin_dir) {
        if let Some(free) = free_bytes(cache)
            && free < need_cache + need_bin
        {
            return Err(short(cache, need_cache + need_bin, free));
        }
        return Ok(());
    }
    for (d, need) in [(cache, need_cache), (bin_dir, need_bin)] {
        if let Some(free) = free_bytes(d)
            && free < need
        {
            return Err(short(d, need, free));
        }
    }
    Ok(())
}

/// Why one try at a download didn't work.
enum Fail {
    /// The network: worth trying again, from where it stopped.
    Again(String),
    /// Trying again won't help.
    Stop(String),
}

/// Download an asset to `to` (by way of `to.part`), trying again when the connection drops and
/// carrying on from where it stopped. A half download is kept when it gives up, for next time.
fn fetch(asset: &Asset, to: &Path, set: &dyn Fn(Stage)) -> Result<(), String> {
    let part = to.with_file_name(format!("{}.part", asset.name));
    let total = asset.size;
    for attempt in 1..=TRIES {
        match fetch_once(&asset.url, &part, total, set) {
            Ok(()) => {
                return std::fs::rename(&part, to)
                    .map_err(|err| format!("couldn't keep the download: {err}"));
            }
            Err(Fail::Stop(e)) => return Err(e),
            Err(Fail::Again(e)) if attempt == TRIES => {
                return Err(format!(
                    "the download kept stopping ({e}); try again later, and it carries on from where it stopped"
                ));
            }
            Err(Fail::Again(_)) => {
                let got = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
                set(Stage::Retrying {
                    attempt: attempt + 1,
                    of: TRIES,
                    got,
                    total,
                });
                std::thread::sleep(Duration::from_secs(2 * attempt as u64));
            }
        }
    }
    unreachable!("the last try returns")
}

fn fetch_once(url: &str, part: &Path, total: u64, set: &dyn Fn(Stage)) -> Result<(), Fail> {
    let len = || std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let mut have = len();
    if total > 0 && have > total {
        let _ = std::fs::remove_file(part);
        have = 0;
    }
    if total > 0 && have == total {
        return Ok(());
    }
    let mut cmd = Command::new("curl");
    // Fail on HTTP errors (nothing is written then), and give up on a connection that has
    // sent less than 1 kB a second for 30 seconds: it's tried again from where it stopped.
    cmd.args([
        "-fsSL",
        "--connect-timeout",
        "20",
        "--speed-limit",
        "1024",
        "--speed-time",
        "30",
        "-w",
        "%{http_code}",
        "-A",
        concat!("rivet-welcome/", env!("CARGO_PKG_VERSION")),
    ]);
    if have > 0 {
        cmd.args(["-C", "-"]);
    }
    let mut child = cmd
        .arg("-o")
        .arg(part)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Fail::Stop(crate::releases::no_curl(e)))?;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {}
            Err(err) => return Err(Fail::Stop(err.to_string())),
        }
        set(Stage::Downloading { got: len(), total });
        std::thread::sleep(Duration::from_millis(120));
    };
    let read = |p: Option<&mut dyn std::io::Read>| {
        let mut s = String::new();
        if let Some(r) = p {
            let _ = r.read_to_string(&mut s);
        }
        s
    };
    let code: u16 = read(child.stdout.as_mut().map(|r| r as _))
        .trim()
        .parse()
        .unwrap_or(0);
    let err = read(child.stderr.as_mut().map(|r| r as _));
    set(Stage::Downloading { got: len(), total });
    if !status.success() {
        return Err(match (status.code(), code) {
            // Written nothing more: the disk is full.
            (Some(23), _) => {
                let _ = std::fs::remove_file(part);
                Fail::Stop(format!(
                    "the disk filled up while downloading, so nothing was installed: free some space in {} and try again",
                    part.parent()
                        .map(|d| d.display().to_string())
                        .unwrap_or_default()
                ))
            }
            // The server can't carry on from the middle, or the half download doesn't fit:
            // start again from nothing.
            (Some(33), _) | (_, 416) => {
                let _ = std::fs::remove_file(part);
                Fail::Again("starting again from the beginning".into())
            }
            (_, 404 | 410) => Fail::Stop(
                "the download is missing from the release, so nothing was installed".into(),
            ),
            (_, 408 | 429 | 500..=599) => Fail::Again(format!("the server answered {code}")),
            (_, 400..=499) => Fail::Stop(format!(
                "the server refused the download ({code}), so nothing was installed"
            )),
            (Some(6 | 7 | 18 | 28 | 35 | 52 | 55 | 56 | 92), _) => Fail::Again(
                crate::releases::curl_failure(status.code(), &err, "the download server"),
            ),
            (c, _) => Fail::Stop(crate::releases::curl_failure(
                c,
                &err,
                "the download server",
            )),
        });
    }
    let got = len();
    if total > 0 && got != total {
        if got > total {
            let _ = std::fs::remove_file(part);
        }
        return Err(Fail::Again(format!(
            "it came to {} rather than {}",
            size(got),
            size(total)
        )));
    }
    Ok(())
}

/// The archive's SHA-256 against the one published with it. A download that doesn't match is
/// thrown away with the rest of the work folder.
fn verify(file: &Path, sum_file: &Path) -> Result<(), String> {
    let want = std::fs::read_to_string(sum_file)
        .ok()
        .and_then(|t| t.split_whitespace().next().map(str::to_lowercase))
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("the checksum published with it isn't one, so nothing was installed")?;
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
            "the download doesn't match its checksum, so nothing was installed. It may have been damaged on the way: try again"
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

/// Run the archive's installer, into the same folder Rivet Welcome looks in, given a few
/// minutes. Its output goes to `log`; when it fails, the last line says why.
fn run_installer(installer: &Path, folder: &Path, log: &Path) -> Result<(), String> {
    let out = std::fs::File::create(log).map_err(|err| format!("couldn't install it: {err}"))?;
    let err = out
        .try_clone()
        .map_err(|err| format!("couldn't install it: {err}"))?;
    let mut child = Command::new("sh")
        .arg(installer)
        .current_dir(folder)
        .env("XDG_BIN_HOME", local::bin_home())
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|err| format!("couldn't start its installer: {err}"))?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() < INSTALLER_TIME => {
                std::thread::sleep(Duration::from_millis(100))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("its installer didn't finish in 5 minutes".into());
            }
        }
    };
    if status.success() {
        return Ok(());
    }
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let line = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string();
    Err(if line.contains("No space left") {
        "the disk is full, so it couldn't be installed".into()
    } else if line.is_empty() {
        "its installer stopped with an error".into()
    } else {
        format!("its installer stopped with an error ({line})")
    })
}

/// What the app had on this computer before an install, kept to put back if it fails.
struct Backup {
    /// Each file and where its copy is.
    saved: Vec<(PathBuf, PathBuf)>,
    /// The version that was there; None when it wasn't installed.
    had: Option<Version>,
    existed: bool,
}

impl Backup {
    fn take(e: &Entry, into: &Path) -> Result<Backup, String> {
        let bin = local::bin_home().join(&e.bin);
        let existed = bin.is_file();
        let mut files = local::app_files(e);
        if existed {
            files.insert(0, bin.clone());
        }
        std::fs::create_dir_all(into).map_err(|err| format!("couldn't keep a copy: {err}"))?;
        let mut saved = Vec::new();
        for (i, f) in files.into_iter().enumerate() {
            let copy = into.join(i.to_string());
            std::fs::copy(&f, &copy).map_err(|err| {
                format!(
                    "couldn't keep a copy of {} to go back to, so nothing was changed: {err}",
                    f.display()
                )
            })?;
            saved.push((f, copy));
        }
        Ok(Backup {
            saved,
            had: existed.then(|| local::version_of(&bin)).flatten(),
            existed,
        })
    }

    /// Put things back as they were, and say so after `err` (what went wrong).
    fn restore(self, e: &Entry, err: String) -> String {
        let cleared = local::clear(e);
        if !self.existed {
            return match cleared {
                Ok(()) => format!("{err}. Nothing was left installed"),
                Err(c) => format!("{err}. Some of it is still there ({c})"),
            };
        }
        let mut failed = None;
        for (orig, copy) in &self.saved {
            // A copy beside it, then renamed over: never half a file where the program goes.
            let tmp = orig.with_file_name(format!(
                ".{}.rivet-welcome-restore",
                orig.file_name().unwrap_or_default().to_string_lossy()
            ));
            let r = orig
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::copy(copy, &tmp))
                .and_then(|_| std::fs::rename(&tmp, orig));
            if let Err(x) = r {
                let _ = std::fs::remove_file(&tmp);
                failed.get_or_insert(format!("{}: {x}", orig.display()));
            }
        }
        let had = self
            .had
            .map(|v| format!("{} {v}", e.title))
            .unwrap_or_else(|| e.title.clone());
        match failed {
            None => format!("{err}. {had} is back as it was"),
            Some(x) => {
                format!("{err}. Putting {had} back didn't work either ({x}): install it again")
            }
        }
    }
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
    Err(if line.contains("No space left") {
        format!("couldn't {what}: the disk is full")
    } else if line.is_empty() {
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
        // Upper case is the same sum.
        std::fs::write(
            &s,
            "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824  a.tar.gz\n",
        )
        .unwrap();
        assert!(verify(&f, &s).is_ok());
        let wrong = "1cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  a.tar.gz\n";
        std::fs::write(&s, wrong).unwrap();
        assert!(verify(&f, &s).unwrap_err().contains("doesn't match"));
        // Not a checksum at all (an error page, an empty file).
        for junk in ["00  a.tar.gz\n", "", "<html>Not Found</html>"] {
            std::fs::write(&s, junk).unwrap();
            assert!(verify(&f, &s).unwrap_err().contains("isn't one"), "{junk}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn locks_keep_one_at_a_time() {
        let id = format!("test-{}", std::process::id());
        let first = lock(&id).unwrap();
        assert!(lock(&id).is_err());
        drop(first);
        assert!(lock(&id).is_ok());
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
        let r = Stage::Retrying {
            attempt: 2,
            of: 4,
            got: 1,
            total: 2,
        };
        assert!(r.label().contains("2 of 4"));
        assert_eq!(
            r.fraction(),
            Stage::Downloading { got: 1, total: 2 }.fraction()
        );
    }
}

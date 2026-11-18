//! What's installed on this computer: each app's program (in ~/.local/bin, where the release
//! archives' installers put them, or anywhere on PATH) and the version it says it is.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::catalog::Entry;
use crate::releases::Version;

#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub path: PathBuf,
    pub version: Option<Version>,
    /// In ~/.local/bin, so Rivet Welcome can update and remove it. Otherwise it came from
    /// somewhere else (a distribution package, AUR, a build) and is left alone.
    pub ours: bool,
}

/// Where programs are installed (the release archives' installers use the same folder).
pub fn bin_home() -> PathBuf {
    std::env::var_os("XDG_BIN_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| rivet_xdg::places::home().join(".local/bin"))
}

/// Is the install folder on PATH (so the apps run from a terminal too)?
pub fn bin_home_on_path() -> bool {
    let home = bin_home();
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == home))
}

fn is_exe(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The program: in ~/.local/bin first, else on PATH.
pub fn find(bin: &str) -> Option<PathBuf> {
    let mine = bin_home().join(bin);
    if is_exe(&mine) {
        return Some(mine);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| is_exe(p))
}

/// What `PROGRAM --version` says, given a few seconds. None when it isn't a Rivet app.
pub fn version_of(path: &Path) -> Option<Version> {
    let mut child = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(4) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(child.stdout.as_mut()?, &mut out).ok()?;
    parse_version_line(&out)
}

/// "Rivet Files 0.7.0" or "rivet-notes 0.2.0" → the version (another program that happens to
/// have the same name doesn't say "rivet").
pub fn parse_version_line(out: &str) -> Option<Version> {
    let line = out.lines().next()?.trim();
    if !line.to_lowercase().contains("rivet") {
        return None;
    }
    line.split_whitespace().last().and_then(Version::parse)
}

/// Checks: `RIVET_WELCOME_INSTALLED=files=0.6.0,notes=0.2.0` pretends those are installed.
fn pretend(e: &Entry) -> Option<Option<Installed>> {
    let spec = std::env::var("RIVET_WELCOME_INSTALLED").ok()?;
    let found = spec
        .split(',')
        .filter_map(|p| p.split_once('='))
        .find(|(id, _)| *id == e.id)
        .map(|(_, v)| Installed {
            path: bin_home().join(&e.bin),
            version: Version::parse(v),
            ours: true,
        });
    Some(found)
}

/// Is this app installed, and which version? (Runs the program: keep it off the UI thread.)
pub fn detect(e: &Entry) -> Option<Installed> {
    if let Some(p) = pretend(e) {
        return p;
    }
    let path = find(&e.bin)?;
    let version = version_of(&path);
    // A program by that name that isn't a Rivet app isn't this app.
    version.as_ref()?;
    let ours = path.parent() == Some(bin_home().as_path());
    Some(Installed {
        path,
        version,
        ours,
    })
}

/// Start the app (detached; it outlives this window).
pub fn open(e: &Entry, path: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let mut cmd = Command::new(path);
    cmd.args(&e.open_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Its own session, so closing this window doesn't take it along.
    // SAFETY: setsid is async-signal-safe, so it may run between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| format!("couldn't start it: {e}"))?;
    // Collected when it ends, so it doesn't linger as a zombie while this program runs.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Take the app off this computer: its program in ~/.local/bin, the launcher entries that run
/// it, and its icons. Settings and documents stay.
pub fn remove(e: &Entry) -> Result<(), String> {
    let bin = bin_home().join(&e.bin);
    if bin.exists() {
        std::fs::remove_file(&bin)
            .map_err(|err| format!("couldn't remove {}: {err}", bin.display()))?;
    }
    let data = rivet_xdg::places::data_home();
    let apps = data.join("applications");
    let quoted = format!("\"{}\"", bin.display());
    let plain = bin.display().to_string();
    if let Ok(dir) = std::fs::read_dir(&apps) {
        for f in dir.flatten() {
            let p = f.path();
            if p.extension().is_some_and(|x| x == "desktop")
                && let Ok(text) = std::fs::read_to_string(&p)
                && text.lines().any(|l| {
                    l.starts_with("Exec=")
                        && (l.contains(&quoted)
                            || l[5..].split_whitespace().next() == Some(plain.as_str()))
                })
            {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    let icons = data.join("icons/hicolor");
    if let Ok(sizes) = std::fs::read_dir(&icons) {
        for s in sizes.flatten() {
            for ext in ["svg", "png"] {
                let _ = std::fs::remove_file(s.path().join(format!("apps/{}.{ext}", e.dist)));
            }
        }
    }
    let _ = Command::new("update-desktop-database")
        .arg(&apps)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_lines() {
        assert_eq!(
            parse_version_line("Rivet Files 0.7.0\n"),
            Version::parse("0.7.0")
        );
        assert_eq!(
            parse_version_line("rivet-notes 0.2.0"),
            Version::parse("0.2.0")
        );
        assert_eq!(
            parse_version_line("rivet-shell 0.1.0-beta.1"),
            Version::parse("0.1.0-beta.1")
        );
        assert_eq!(parse_version_line("rivet 2.1 (some other program)"), None);
        assert_eq!(parse_version_line("GNU coreutils 9.4"), None);
        assert_eq!(parse_version_line(""), None);
    }
}

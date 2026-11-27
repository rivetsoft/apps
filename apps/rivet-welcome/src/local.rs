//! What's installed on this computer: each app's program (in ~/.local/bin, where the release
//! archives' installers put them, or anywhere on PATH) and the version it says it is.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::catalog::{Entry, Kind};
use crate::releases::Version;

#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub path: PathBuf,
    pub version: Option<Version>,
    /// In ~/.local/bin, so Rivet Welcome can update and remove it. Otherwise it came from
    /// somewhere else (a distribution package, AUR, a build) and is left alone.
    pub ours: bool,
    /// Set for Rivet Welcome when it isn't installed anywhere and this is it, running.
    pub running: Running,
}

/// How this program is running, when it wasn't found installed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Running {
    /// Found installed: the usual case.
    No,
    /// From an AppImage: one file, nothing installed.
    AppImage,
    /// From a folder that isn't on PATH (an unpacked download, a build).
    Folder,
}

/// Where programs are installed (the release archives' installers use the same folder).
pub fn bin_home() -> PathBuf {
    std::env::var_os("XDG_BIN_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| rivet_xdg::places::home().join(".local/bin"))
}

/// This program's file. When Rivet Welcome has updated itself, Linux calls the file it started
/// from "… (deleted)": the new one now in its place is the one to point launchers at.
pub fn this_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    match exe.to_str().and_then(|s| s.strip_suffix(" (deleted)")) {
        Some(p) => Some(PathBuf::from(p)),
        None => Some(exe),
    }
}

/// The file this program started from has been replaced or removed since (an update).
pub fn replaced() -> bool {
    std::fs::read_link("/proc/self/exe").is_ok_and(|p| p.to_string_lossy().ends_with(" (deleted)"))
}

/// Where an install that isn't Rivet Welcome's came from, which says where to update it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Flatpak,
    /// The system's own folders (/usr/bin): a distribution package or the AUR.
    Package,
    /// Anywhere else on PATH (/usr/local/bin, a build).
    Elsewhere,
}

impl Installed {
    pub fn source(&self) -> Source {
        let p = self.path.to_string_lossy();
        if p.contains("/flatpak/") {
            Source::Flatpak
        } else if ["/usr/bin/", "/bin/", "/usr/sbin/", "/usr/games/"]
            .iter()
            .any(|d| p.starts_with(d))
        {
            Source::Package
        } else {
            Source::Elsewhere
        }
    }

    /// For someone deciding where to update it: "from Flatpak" and so on.
    pub fn origin(&self) -> String {
        match self.source() {
            Source::Flatpak => "from Flatpak".into(),
            Source::Package => "from your system's packages".into(),
            Source::Elsewhere => format!(
                "in {}",
                self.path
                    .parent()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default()
            ),
        }
    }

    /// Why Rivet Welcome won't touch it, and where to update it instead.
    pub fn left_alone(&self) -> String {
        match self.source() {
            Source::Flatpak => "it came from Flatpak, so update it there".into(),
            Source::Package => "it came from your system's packages, so update it there".into(),
            Source::Elsewhere => format!(
                "it was installed in {} some other way, so Rivet Welcome leaves it as it is",
                self.path
                    .parent()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default()
            ),
        }
    }
}

/// The app as a Flatpak (Rivet Files is on Flathub as io.github.rivetsoft.rivet-files). Its
/// program isn't on PATH under the app's name, so it's looked for where Flatpak exports it.
fn flatpak(e: &Entry) -> Option<PathBuf> {
    let id = format!("io.github.rivetsoft.{}", e.dist);
    [
        rivet_xdg::places::data_home().join("flatpak/exports/bin"),
        PathBuf::from("/var/lib/flatpak/exports/bin"),
    ]
    .into_iter()
    .map(|d| d.join(&id))
    .find(|p| is_exe(p))
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
        .map(|(_, v)| match v {
            // `welcome=appimage`: Rivet Welcome as it is when run from an AppImage.
            "appimage" => Installed {
                path: "/home/you/Rivet_Welcome.AppImage".into(),
                version: Version::parse(env!("CARGO_PKG_VERSION")),
                ours: false,
                running: Running::AppImage,
            },
            _ => Installed {
                path: bin_home().join(&e.bin),
                version: Version::parse(v),
                ours: true,
                running: Running::No,
            },
        });
    Some(found)
}

/// Is this app installed, and which version? (Runs the program: keep it off the UI thread.)
pub fn detect(e: &Entry) -> Option<Installed> {
    if let Some(p) = pretend(e) {
        return p;
    }
    let found = find(&e.bin).and_then(|path| {
        let version = version_of(&path);
        // A program by that name that isn't a Rivet app isn't this app.
        version.as_ref()?;
        let ours = path.parent() == Some(bin_home().as_path());
        Some(Installed {
            path,
            version,
            ours,
            running: Running::No,
        })
    });
    // A Flatpak can take longer than the few seconds given to say its version: it's installed
    // either way.
    let found = found.or_else(|| {
        flatpak(e).map(|path| Installed {
            version: version_of(&path),
            path,
            ours: false,
            running: Running::No,
        })
    });
    // Rivet Welcome is here even when it isn't installed: someone is looking at it.
    found.or_else(|| (e.kind == Kind::Welcome).then(this_program).flatten())
}

/// This program as it is running now: the AppImage it came in, or the file itself.
fn this_program() -> Option<Installed> {
    let exe = std::env::current_exe().ok()?;
    let (path, running) = match appimage(&exe) {
        Some(file) => (file, Running::AppImage),
        None => (exe, Running::Folder),
    };
    Some(Installed {
        path,
        version: Version::parse(env!("CARGO_PKG_VERSION")),
        ours: false,
        running,
    })
}

/// The AppImage file this program was started from. Both variables are checked against where
/// the program is, since a program started by another AppImage inherits that one's.
fn appimage(exe: &Path) -> Option<PathBuf> {
    let file = PathBuf::from(std::env::var_os("APPIMAGE")?);
    let dir = PathBuf::from(std::env::var_os("APPDIR")?);
    (file.is_absolute() && exe.starts_with(&dir)).then_some(file)
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

/// The app's own files outside its program: the launcher entries that run it, and its icons.
pub fn app_files(e: &Entry) -> Vec<PathBuf> {
    let bin = bin_home().join(&e.bin);
    let data = rivet_xdg::places::data_home();
    let quoted = format!("\"{}\"", bin.display());
    let plain = bin.display().to_string();
    let mut out = Vec::new();
    if let Ok(dir) = std::fs::read_dir(data.join("applications")) {
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
                out.push(p);
            }
        }
    }
    if let Ok(sizes) = std::fs::read_dir(data.join("icons/hicolor")) {
        for s in sizes.flatten() {
            for ext in ["svg", "png"] {
                let p = s.path().join(format!("apps/{}.{ext}", e.dist));
                if p.is_file() {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Take the app's program, launcher entries and icons away (the caller holds its lock).
pub fn clear(e: &Entry) -> Result<(), String> {
    let files = app_files(e);
    let bin = bin_home().join(&e.bin);
    if bin.exists() {
        std::fs::remove_file(&bin)
            .map_err(|err| format!("couldn't remove {}: {err}", bin.display()))?;
    }
    for f in files {
        let _ = std::fs::remove_file(f);
    }
    let _ = Command::new("update-desktop-database")
        .arg(rivet_xdg::places::data_home().join("applications"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

/// Take the app off this computer: its program in ~/.local/bin, the launcher entries that run
/// it, and its icons. Settings and documents stay.
pub fn remove(e: &Entry) -> Result<(), String> {
    // Not while it's being installed or updated.
    let _lock = crate::install::lock(&e.id)?;
    clear(e)?;
    // Rivet Welcome itself: the tray no longer starts at sign-in.
    if e.kind == Kind::Welcome {
        crate::settings::set_autostart(false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_installs_came_from() {
        let at = |p: &str| Installed {
            path: p.into(),
            version: None,
            ours: false,
            running: Running::No,
        };
        assert_eq!(at("/usr/bin/rivet").source(), Source::Package);
        assert_eq!(
            at("/var/lib/flatpak/exports/bin/io.github.rivetsoft.rivet-files").source(),
            Source::Flatpak
        );
        assert_eq!(at("/usr/local/bin/rivet-shell").source(), Source::Elsewhere);
        assert!(
            at("/usr/local/bin/rivet-shell")
                .origin()
                .contains("/usr/local/bin")
        );
    }

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

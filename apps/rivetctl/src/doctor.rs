//! `rivetctl doctor`: what's missing for the apps to install and run here, each with what to do
//! about it. It reads and runs `ldd`; it changes nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rivet_install::catalog::Kind;
use rivet_install::local;
use rivet_install::state::Row;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Ok,
    /// Something works less well, or one command won't.
    Warn,
    /// An app won't start, or nothing can be installed.
    Fail,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub level: Level,
    pub what: String,
    /// What to do about it; empty when it's fine.
    pub fix: String,
}

fn check(level: Level, what: impl Into<String>, fix: impl Into<String>) -> Check {
    Check {
        level,
        what: what.into(),
        fix: fix.into(),
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

/// The system's libraries by file name (`ldconfig -p`), else a look in the usual folders.
fn library(names: &[&str]) -> Option<String> {
    let cache = ["ldconfig", "/sbin/ldconfig", "/usr/sbin/ldconfig"]
        .iter()
        .find_map(|l| {
            let out = Command::new(l)
                .arg("-p")
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).to_string())
        });
    for n in names {
        if cache.as_ref().is_some_and(|c| {
            c.lines()
                .any(|l| l.trim_start().starts_with(&format!("{n} ")))
        }) {
            return Some(n.to_string());
        }
        let dirs = [
            "/usr/lib64",
            "/usr/lib",
            "/lib64",
            "/lib",
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib/aarch64-linux-gnu",
        ];
        if dirs.iter().any(|d| Path::new(d).join(n).exists()) {
            return Some(n.to_string());
        }
    }
    None
}

/// The libraries a program is linked to that aren't here.
fn missing_linked(bin: &Path) -> Vec<String> {
    let Ok(out) = Command::new("ldd")
        .arg(bin)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains("not found"))
        .filter_map(|l| l.split_whitespace().next().map(String::from))
        .collect()
}

/// Rivet Reader's PDFium, where the release archive or a build from source puts it.
fn pdfium(bin: &Path) -> Option<PathBuf> {
    let dir = bin.parent()?;
    [
        dir.join("../lib/rivet-reader/libpdfium.so"),
        dir.join("lib/rivet-reader/libpdfium.so"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

pub fn run(rows: &[Row]) -> Vec<Check> {
    let mut out = Vec::new();
    let missing: Vec<&str> = ["curl", "tar", "sha256sum"]
        .into_iter()
        .filter(|p| !on_path(p))
        .collect();
    out.push(if missing.is_empty() {
        check(
            Level::Ok,
            "curl, tar and sha256sum are here to download and install apps",
            "",
        )
    } else {
        check(
            Level::Fail,
            format!(
                "{} missing, so nothing can be installed",
                missing.join(" and ")
            ),
            format!(
                "Install {} with your system's package manager.",
                missing.join(" and ")
            ),
        )
    });
    let missing: Vec<&str> = ["git", "cargo"]
        .into_iter()
        .filter(|p| !on_path(p))
        .collect();
    out.push(if missing.is_empty() {
        check(
            Level::Ok,
            "git and cargo are here to build apps from source",
            "",
        )
    } else {
        check(
            Level::Warn,
            format!(
                "{} missing, so rivetctl build won't work",
                missing.join(" and ")
            ),
            "Install git, and Rust from rustup.rs, to build apps from source.",
        )
    });
    let bin = local::bin_home();
    out.push(if local::bin_home_on_path() {
        check(Level::Ok, format!("{} is on your PATH", bin.display()), "")
    } else {
        check(
            Level::Warn,
            format!(
                "{} isn't on your PATH, so the apps don't start from a terminal by name",
                bin.display()
            ),
            format!(
                "Add {} to PATH in your shell's startup file.",
                bin.display()
            ),
        )
    });
    let windowed = rows
        .iter()
        .any(|r| r.installed.as_ref().is_some_and(|i| i.ours) && r.entry.kind != Kind::Command);
    if windowed {
        for (names, what) in [
            (
                &["libxkbcommon.so.0"][..],
                "the keyboard library (libxkbcommon)",
            ),
            (
                &["libEGL.so.1", "libGL.so.1"][..],
                "OpenGL (libEGL or libGL)",
            ),
        ] {
            out.push(match library(names) {
                Some(_) => check(
                    Level::Ok,
                    format!("{what} is here for the apps' windows"),
                    "",
                ),
                None => check(
                    Level::Fail,
                    format!("{what} is missing, so the apps' windows won't open"),
                    format!("Install {} with your system's package manager.", names[0]),
                ),
            });
        }
    }
    for r in rows {
        let Some(i) = r.installed.as_ref().filter(|i| i.ours) else {
            continue;
        };
        let title = &r.entry.title;
        let gone = missing_linked(&i.path);
        if !gone.is_empty() {
            out.push(check(
                Level::Fail,
                format!("{title} won't start: {} missing", gone.join(", ")),
                format!(
                    "Install the package that has {} with your system's package manager.",
                    gone.join(" and ")
                ),
            ));
        }
        if r.entry.id == "cinema" {
            out.push(match library(&["libmpv.so.2", "libmpv.so.1"]) {
                Some(l) => check(Level::Ok, format!("{title} plays through {l}"), ""),
                None => check(
                    Level::Fail,
                    format!("{title} can't play anything: libmpv is missing"),
                    "Install mpv-libs (Fedora), libmpv2 (Debian, Ubuntu) or mpv (Arch).",
                ),
            });
        }
        if r.entry.id == "reader" {
            out.push(match pdfium(&i.path).map(|p| p.display().to_string()).or_else(|| library(&["libpdfium.so"])) {
                Some(p) => check(Level::Ok, format!("{title} opens PDFs with {p}"), ""),
                None => check(Level::Warn, format!("{title} can't open PDFs: PDFium isn't beside it"), "Install Rivet Reader again (rivetctl install reader --held) to bring its PDFium back."),
            });
        }
        let launcher = matches!(r.entry.kind, Kind::App | Kind::Shell | Kind::Welcome);
        if launcher
            && !local::app_files(&r.entry)
                .iter()
                .any(|f| f.extension().is_some_and(|x| x == "desktop"))
        {
            out.push(check(
                Level::Warn,
                format!("{title} isn't in your app launcher"),
                format!(
                    "Run {} --install-desktop-entry, or install it again.",
                    i.path.display()
                ),
            ));
        }
    }
    out
}

pub fn json(checks: &[Check]) -> Value {
    let level = |l| match l {
        Level::Ok => "ok",
        Level::Warn => "warning",
        Level::Fail => "problem",
    };
    Value::Array(
        checks
            .iter()
            .map(|c| json!({ "level": level(c.level), "what": c.what, "fix": c.fix }))
            .collect(),
    )
}

/// For the terminal, one check a line, fixes under the ones that need them.
pub fn text(checks: &[Check]) -> String {
    let mut s = String::new();
    for c in checks {
        let mark = match c.level {
            Level::Ok => "ok     ",
            Level::Warn => "warning",
            Level::Fail => "problem",
        };
        s += &format!("{mark}  {}\n", c.what);
        if !c.fix.is_empty() {
            s += &format!("         {}\n", c.fix);
        }
    }
    s
}

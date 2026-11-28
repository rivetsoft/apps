//! Building an app from source at a branch, tag or commit, and installing it as
//! `utilities/rivet-dev install` does: the repository's own installer at that commit builds it
//! with cargo, puts the program in ~/.local/bin and adds its launcher entry, and fetches anything
//! it needs beside it (Rivet Reader's pinned PDFium). One clone is kept in the cache folder and
//! reused, with its build folder, so a second build is quicker.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rivet_install::catalog::Entry;
use rivet_install::{install, local, releases};

/// Where the source is kept between builds.
pub fn source_dir() -> PathBuf {
    rivet_xdg::places::cache_home().join("rivetctl/source")
}

fn have(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// `git -C DIR ARGS`, its output, or what went wrong.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("couldn't run git: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(err
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or("git failed")
        .trim()
        .to_string())
}

/// "fetch@main" → ("fetch", "main"); no ref is main.
pub fn split(spec: &str) -> (&str, &str) {
    match spec.split_once('@') {
        Some((app, r)) if !r.is_empty() => (app, r),
        Some((app, _)) => (app, "main"),
        None => (spec, "main"),
    }
}

/// The clone, up to date, with `reference` checked out: its commit.
fn checkout(dir: &Path, reference: &str, log: &mut dyn FnMut(&str)) -> Result<String, String> {
    if !dir.join(".git").is_dir() {
        let url = format!("https://github.com/{}.git", releases::repo());
        log(&format!("Getting the source from {url}…"));
        if let Some(p) = dir.parent() {
            std::fs::create_dir_all(p)
                .map_err(|e| format!("couldn't make {}: {e}", p.display()))?;
        }
        let ok = Command::new("git")
            .args([
                "clone",
                "--quiet",
                "--filter=blob:none",
                "--no-checkout",
                &url,
            ])
            .arg(dir)
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("couldn't run git: {e}"))?;
        if !ok.success() {
            let _ = std::fs::remove_dir_all(dir);
            return Err(format!("couldn't get the source from {url}"));
        }
    } else {
        log("Getting the newest source…");
    }
    git(
        dir,
        &[
            "fetch",
            "--quiet",
            "--prune",
            "--force",
            "--tags",
            "origin",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
    )?;
    let resolve = |r: &str| {
        git(
            dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{r}^{{commit}}"),
            ],
        )
        .ok()
    };
    let commit = resolve(&format!("origin/{reference}"))
        .or_else(|| resolve(&format!("refs/tags/{reference}")))
        .or_else(|| resolve(reference))
        .or_else(|| {
            // A commit no branch has any more: ask for it by name.
            git(dir, &["fetch", "--quiet", "origin", reference]).ok()?;
            resolve("FETCH_HEAD")
        })
        .ok_or_else(|| {
            format!(
                "{reference} isn't a branch, tag or commit of {}",
                releases::repo()
            )
        })?;
    git(
        dir,
        &["checkout", "--quiet", "--force", "--detach", &commit],
    )?;
    Ok(commit)
}

/// Build `e` at `reference` and install it. Its output goes to `log` line by line when given,
/// else straight to this terminal. The program's version and the commit when it worked.
pub fn build(
    e: &Entry,
    reference: &str,
    log: Option<&mut dyn FnMut(&str)>,
) -> Result<(String, String), String> {
    for p in ["git", "cargo"] {
        if !have(p) {
            return Err(match p {
                "cargo" => "building from source needs Rust's cargo: install it from rustup.rs, then try again".into(),
                _ => "building from source needs git: install it, then try again".into(),
            });
        }
    }
    let mut quiet = |_: &str| {};
    let (log, to_terminal): (&mut dyn FnMut(&str), bool) = match log {
        Some(l) => (l, false),
        None => (&mut quiet, true),
    };
    let say = |log: &mut dyn FnMut(&str), s: &str| {
        if to_terminal {
            eprintln!("{s}");
        } else {
            log(s);
        }
    };
    let _lock = install::lock(&e.id)?;
    let dir = source_dir();
    let mut relay = |s: &str| say(log, s);
    let commit = checkout(&dir, reference, &mut relay)?;
    let short = commit.get(..7).unwrap_or(&commit).to_string();
    say(
        log,
        &format!("Building {} from {reference} ({short})…", e.title),
    );
    let mut cmd = Command::new("sh");
    cmd.arg("utilities/rivet-dev")
        .args(["install", &e.id])
        .current_dir(&dir)
        .env("XDG_BIN_HOME", local::bin_home())
        .stdin(Stdio::null());
    let ok = if to_terminal {
        cmd.status()
            .map_err(|err| format!("couldn't start the build: {err}"))?
            .success()
    } else {
        use std::io::BufRead;
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("couldn't start the build: {err}"))?;
        // Both streams into one, a line at a time.
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let readers: Vec<_> = [
            child
                .stdout
                .take()
                .map(|o| Box::new(o) as Box<dyn std::io::Read + Send>),
            child.stderr.take().map(|o| Box::new(o) as _),
        ]
        .into_iter()
        .flatten()
        .map(|r| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for l in std::io::BufReader::new(r).lines().map_while(Result::ok) {
                    let _ = tx.send(l);
                }
            })
        })
        .collect();
        drop(tx);
        for l in rx {
            log(&l);
        }
        for r in readers {
            let _ = r.join();
        }
        child.wait().map_err(|err| err.to_string())?.success()
    };
    if !ok {
        return Err(format!(
            "{} didn't build from {reference} ({short}); the lines above say why",
            e.title
        ));
    }
    let bin = local::bin_home().join(&e.bin);
    let v = local::version_of(&bin)
        .ok_or_else(|| format!("it built, but {} doesn't start", bin.display()))?;
    Ok((v.to_string(), commit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs() {
        assert_eq!(split("fetch@main"), ("fetch", "main"));
        assert_eq!(
            split("reader@rivet-reader-v0.1.0"),
            ("reader", "rivet-reader-v0.1.0")
        );
        assert_eq!(split("notes"), ("notes", "main"));
        assert_eq!(split("notes@"), ("notes", "main"));
    }
}

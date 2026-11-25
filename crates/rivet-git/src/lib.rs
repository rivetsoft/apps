//! Git for the Rivet apps, with no user interface. Rivet Edit's Git panel and gutter use it, and
//! a Git app of its own can later be built on the same pieces.
//!
//! It runs the `git` program, as VS Code does, rather than reading the repository itself. That
//! way the person's own settings, hooks, credential helpers, signing and attribute filters all
//! apply, and nothing here has to track Git's on-disk formats.
//!
//! - [`Repo`]: find a repository, and the commands run on it: status, stage, unstage,
//!   discard, commit, branches, fetch, pull and push, the log, and file text from the index or
//!   a commit.
//! - [`status`]: what `git status` reports, read into types.
//! - [`diff`]: which lines changed between two texts, as hunks. The gutter uses it to compare
//!   the text being edited with the staged copy, and to stage or revert one hunk.
//!
//! Everything here blocks until git finishes. Apps call it from a thread of their own.

pub mod diff;
pub mod status;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use diff::{Hunk, HunkKind};
pub use status::{Branch, Change, Entry, Status};

/// What went wrong running git: its own words when it gave some.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// A repository's working tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    /// The top of the working tree. Paths given to and returned by this crate are inside it,
    /// written with `/`.
    pub root: PathBuf,
    /// The `.git` folder (somewhere else for a linked worktree or a submodule).
    pub git_dir: PathBuf,
}

/// A commit in the log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub id: String,
    pub short: String,
    pub author: String,
    pub email: String,
    /// Seconds since 1970.
    pub time: i64,
    pub subject: String,
}

/// A branch to switch to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchRef {
    /// "main", or "origin/main" for a remote one.
    pub name: String,
    pub remote: bool,
    /// The one checked out.
    pub current: bool,
    /// Its last commit's subject.
    pub subject: String,
    /// When its last commit was made (seconds since 1970).
    pub time: i64,
}

/// Whether the `git` program can be run.
pub fn available() -> bool {
    Command::new("git").arg("--version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn git(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.current_dir(dir)
        // Paths as they are, not quoted with octal escapes.
        .args(["-c", "core.quotepath=off"])
        // Never wait for a password typed in a terminal that isn't there.
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    c
}

/// Run git and return what it printed, or its complaint.
fn run(mut c: Command) -> Result<Vec<u8>> {
    let out = c.output().map_err(|e| Error(if e.kind() == std::io::ErrorKind::NotFound { "Git isn't installed".into() } else { format!("Couldn't run git: {e}") }))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(Error(complaint(&out.stderr, &out.stdout)))
    }
}

/// The useful part of git's error output: its "fatal:" or "error:" lines, else the last line.
fn complaint(stderr: &[u8], stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(if stderr.iter().all(u8::is_ascii_whitespace) { stdout } else { stderr }).into_owned();
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let picked: Vec<&str> = lines.iter().copied().filter(|l| l.starts_with("fatal:") || l.starts_with("error:")).collect();
    let words = if picked.is_empty() { lines.last().copied().unwrap_or("git failed").to_string() } else { picked.join(" ") };
    let words = words.trim_start_matches("fatal: ").trim_start_matches("error: ");
    let mut s = words.to_string();
    if let Some(c) = s.get(..1) {
        s.replace_range(..1, &c.to_uppercase());
    }
    s
}

fn text(b: Vec<u8>) -> String {
    String::from_utf8(b).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

impl Repo {
    /// The repository `path` is in (a file or a folder), if any.
    pub fn discover(path: &Path) -> Option<Repo> {
        let dir = if path.is_dir() { path } else { path.parent()? };
        let mut c = git(dir);
        c.args(["rev-parse", "--show-toplevel", "--absolute-git-dir"]);
        let out = text(run(c).ok()?);
        let mut lines = out.lines();
        let root = PathBuf::from(lines.next()?);
        let git_dir = PathBuf::from(lines.next()?);
        Some(Repo { root, git_dir })
    }

    /// Make a new repository in `dir`.
    pub fn init(dir: &Path) -> Result<Repo> {
        let mut c = git(dir);
        c.arg("init");
        run(c)?;
        Repo::discover(dir).ok_or_else(|| Error("Git made the repository but can't find it".into()))
    }

    fn cmd(&self) -> Command {
        git(&self.root)
    }

    fn run<I, S>(&self, args: I) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut c = self.cmd();
        c.args(args);
        run(c)
    }

    /// `path` (anywhere) as a path inside the repository, if it is in it.
    pub fn rel(&self, path: &Path) -> Option<String> {
        let r = path.strip_prefix(&self.root).ok()?;
        Some(r.to_string_lossy().replace('\\', "/"))
    }

    /// A stamp that changes whenever the index (what's staged) or HEAD changes: when the
    /// staged copies of files need reading again.
    pub fn stamp(&self) -> Option<std::time::SystemTime> {
        let m = |p: PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let idx = m(self.git_dir.join("index"));
        let head = m(self.git_dir.join("HEAD"));
        idx.max(head)
    }

    /// The branch, and every changed, new and conflicted file.
    pub fn status(&self) -> Result<Status> {
        let mut c = self.cmd();
        // Status refreshes the index's file dates, which takes its lock. Don't, so a status
        // running now and then never makes the person's own git command fail.
        c.env("GIT_OPTIONAL_LOCKS", "0");
        c.args(["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"]);
        Ok(status::parse(&run(c)?))
    }

    /// A file's staged text (what a commit would take). None when it isn't in the index.
    pub fn index_text(&self, path: &str) -> Option<String> {
        self.show(&format!(":0:{path}"))
    }

    /// A file's text at a commit ("HEAD", a branch, an id). None when it wasn't there.
    pub fn text_at(&self, rev: &str, path: &str) -> Option<String> {
        self.show(&format!("{rev}:{path}"))
    }

    fn show(&self, spec: &str) -> Option<String> {
        let b = self.run(["cat-file", "blob", spec]).ok()?;
        String::from_utf8(b).ok()
    }

    /// Stage these files as they are on disk (new, changed or deleted).
    pub fn stage(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        self.run(["add", "-A", "--"].iter().map(|s| s.to_string()).chain(paths.iter().cloned())).map(drop)
    }

    /// Stage every change, new files too.
    pub fn stage_all(&self) -> Result<()> {
        self.run(["add", "-A"]).map(drop)
    }

    /// Take these files out of what's staged (their changes stay on disk).
    pub fn unstage(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        if self.has_head() {
            self.run(["reset", "-q", "HEAD", "--"].iter().map(|s| s.to_string()).chain(paths.iter().cloned())).map(drop)
        } else {
            // No commit yet: unstaging is taking them out of the index.
            self.run(["rm", "-q", "--cached", "-r", "--"].iter().map(|s| s.to_string()).chain(paths.iter().cloned())).map(drop)
        }
    }

    /// Unstage everything.
    pub fn unstage_all(&self) -> Result<()> {
        if self.has_head() { self.run(["reset", "-q"]).map(drop) } else { self.run(["rm", "-q", "--cached", "-r", "."]).map(drop) }
    }

    /// Put these tracked files back as staged, losing their changes on disk. New files are
    /// left alone; see [`Repo::remove_untracked`].
    pub fn discard(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        self.run(["checkout", "-q", "--"].iter().map(|s| s.to_string()).chain(paths.iter().cloned())).map(drop)
    }

    /// Delete these new (untracked) files.
    pub fn remove_untracked(&self, paths: &[String]) -> Result<()> {
        for p in paths {
            let full = self.root.join(p);
            let r = if full.is_dir() { std::fs::remove_dir_all(&full) } else { std::fs::remove_file(&full) };
            r.map_err(|e| Error(format!("Couldn't delete {p}: {e}")))?;
        }
        Ok(())
    }

    /// Write `text` as the staged copy of `path`, as if it had been saved and added. Staging one
    /// hunk is this, with [`diff::apply`].
    pub fn set_index_text(&self, path: &str, text: &str) -> Result<()> {
        use std::io::Write;
        let mode = self.index_mode(path).unwrap_or_else(|| "100644".into());
        let mut c = self.cmd();
        // --path: the file's attributes apply (line endings and other filters).
        c.args(["hash-object", "-w", "--stdin", "--path", path]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c.spawn().map_err(|e| Error(format!("Couldn't run git: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).map_err(|e| Error(format!("Couldn't hand the text to git: {e}")))?;
        }
        let out = child.wait_with_output().map_err(|e| Error(e.to_string()))?;
        if !out.status.success() {
            return Err(Error(complaint(&out.stderr, &out.stdout)));
        }
        let id = text_trim(out.stdout);
        self.run(["update-index", "--add", "--cacheinfo", &format!("{mode},{id},{path}")]).map(drop)
    }

    /// The file's mode in the index ("100644", "100755", "120000").
    fn index_mode(&self, path: &str) -> Option<String> {
        let out = text(self.run(["ls-files", "-s", "--", path]).ok()?);
        out.split_whitespace().next().map(str::to_string)
    }

    /// Whether there is a commit yet.
    pub fn has_head(&self) -> bool {
        self.run(["rev-parse", "-q", "--verify", "HEAD"]).is_ok()
    }

    /// Commit what's staged. `amend` replaces the last commit instead.
    pub fn commit(&self, message: &str, amend: bool) -> Result<()> {
        if message.trim().is_empty() && !amend {
            return Err(Error("Write a message for the commit first".into()));
        }
        let mut args = vec!["commit".to_string(), "-q".into()];
        if amend {
            args.push("--amend".into());
        }
        if message.trim().is_empty() {
            args.push("--no-edit".into());
        } else {
            args.push("-m".into());
            args.push(message.to_string());
        }
        self.run(args).map(drop)
    }

    /// The last commit's message (to edit when amending).
    pub fn last_message(&self) -> Option<String> {
        Some(text(self.run(["log", "-1", "--format=%B"]).ok()?).trim_end().to_string())
    }

    /// Undo the last commit, keeping its changes staged.
    pub fn undo_commit(&self) -> Result<()> {
        self.run(["reset", "-q", "--soft", "HEAD~1"]).map(drop)
    }

    /// Local branches, then remote ones, most recently used first.
    pub fn branches(&self) -> Result<Vec<BranchRef>> {
        let out = text(self.run(["for-each-ref", "--sort=-committerdate", "--format=%(HEAD)%00%(refname)%00%(refname:short)%00%(committerdate:unix)%00%(subject)", "refs/heads", "refs/remotes"])?);
        let mut v: Vec<BranchRef> = out
            .lines()
            .filter_map(|l| {
                let mut f = l.split('\0');
                let head = f.next()?;
                let full = f.next()?;
                let short = f.next()?;
                let time = f.next()?.parse().unwrap_or(0);
                let subject = f.next().unwrap_or("").to_string();
                let remote = full.starts_with("refs/remotes/");
                // origin/HEAD only points at another.
                if remote && full.ends_with("/HEAD") {
                    return None;
                }
                Some(BranchRef { name: short.to_string(), remote, current: head == "*", subject, time })
            })
            .collect();
        v.sort_by_key(|b| b.remote);
        Ok(v)
    }

    /// Switch to a branch. A remote one ("origin/x") makes a local "x" that follows it.
    pub fn checkout(&self, name: &str, remote: bool) -> Result<()> {
        if remote {
            let local = name.split_once('/').map(|(_, b)| b).unwrap_or(name);
            if self.run(["rev-parse", "-q", "--verify", &format!("refs/heads/{local}")]).is_ok() {
                return self.run(["switch", "-q", local]).map(drop);
            }
            return self.run(["switch", "-q", "--track", name]).map(drop);
        }
        self.run(["switch", "-q", name]).map(drop)
    }

    /// Make a branch here and switch to it.
    pub fn create_branch(&self, name: &str) -> Result<()> {
        self.run(["switch", "-q", "-c", name]).map(drop)
    }

    /// Fetch from every remote.
    pub fn fetch(&self) -> Result<()> {
        self.run(["fetch", "--all", "--prune", "-q"]).map(drop)
    }

    /// Pull the branch's upstream.
    pub fn pull(&self) -> Result<()> {
        self.run(["pull", "-q"]).map(drop)
    }

    /// Push the branch; one with no upstream yet is published to `origin` (or the only remote).
    pub fn push(&self, branch: &Branch) -> Result<()> {
        if branch.upstream.is_some() {
            return self.run(["push", "-q"]).map(drop);
        }
        let Some(name) = &branch.head else { return Err(Error("Not on a branch".into())) };
        let remotes = text(self.run(["remote"])?);
        let remote = remotes.lines().find(|r| *r == "origin").or_else(|| remotes.lines().next()).ok_or_else(|| Error("There's no remote to push to".into()))?.to_string();
        self.run(["push", "-q", "-u", &remote, name]).map(drop)
    }

    /// The last `n` commits on HEAD (or `path`'s, when given).
    pub fn log(&self, n: usize, path: Option<&str>) -> Result<Vec<Commit>> {
        let mut args = vec!["log".to_string(), format!("-n{n}"), "--format=%H%x00%h%x00%an%x00%ae%x00%at%x00%s%x1e".into()];
        if let Some(p) = path {
            args.push("--".into());
            args.push(p.to_string());
        }
        let out = match self.run(args) {
            Ok(o) => text(o),
            // No commits yet.
            Err(_) if !self.has_head() => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        Ok(out
            .split('\x1e')
            .filter_map(|r| {
                let mut f = r.trim_start_matches('\n').split('\0');
                Some(Commit { id: f.next()?.to_string(), short: f.next()?.to_string(), author: f.next()?.to_string(), email: f.next()?.to_string(), time: f.next()?.parse().ok()?, subject: f.next()?.to_string() })
            })
            .collect())
    }
}

fn text_trim(b: Vec<u8>) -> String {
    text(b).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new repository in a temporary folder, with a name and email to commit with.
    pub(crate) fn scratch(name: &str) -> Repo {
        let dir = std::env::temp_dir().join(format!("rivet-git-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let r = Repo::init(&dir).unwrap();
        r.run(["config", "user.name", "Test"]).unwrap();
        r.run(["config", "user.email", "test@example.com"]).unwrap();
        r.run(["config", "commit.gpgsign", "false"]).unwrap();
        r
    }

    #[test]
    fn complaint_keeps_fatal_lines() {
        assert_eq!(complaint(b"hint: x\nfatal: not a git repository\n", b""), "Not a git repository");
        assert_eq!(complaint(b"", b"nothing to commit, working tree clean\n"), "Nothing to commit, working tree clean");
    }

    #[test]
    fn stage_commit_and_status() {
        if !available() {
            return;
        }
        let r = scratch("flow");
        std::fs::write(r.root.join("a.txt"), "one\ntwo\n").unwrap();
        let s = r.status().unwrap();
        assert_eq!(s.entries.len(), 1);
        assert!(s.entries[0].untracked());
        assert!(s.branch.oid.is_none());
        r.stage(&["a.txt".into()]).unwrap();
        assert_eq!(r.status().unwrap().entries[0].index, Change::Added);
        r.commit("first", false).unwrap();
        assert!(r.status().unwrap().entries.is_empty());
        assert_eq!(r.log(5, None).unwrap()[0].subject, "first");

        // A change on disk, then one hunk of it staged by writing the index.
        std::fs::write(r.root.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        let s = r.status().unwrap();
        assert_eq!(s.entries[0].worktree, Change::Modified);
        assert_eq!(r.index_text("a.txt").unwrap(), "one\ntwo\n");
        r.set_index_text("a.txt", "zero\none\ntwo\n").unwrap();
        assert_eq!(r.index_text("a.txt").unwrap(), "zero\none\ntwo\n");
        let e = &r.status().unwrap().entries[0];
        assert_eq!((e.index, e.worktree), (Change::Modified, Change::Modified));
        r.unstage(&["a.txt".into()]).unwrap();
        r.discard(&["a.txt".into()]).unwrap();
        assert_eq!(std::fs::read_to_string(r.root.join("a.txt")).unwrap(), "one\ntwo\n");

        r.create_branch("feature").unwrap();
        let b = r.branches().unwrap();
        assert!(b.iter().any(|b| b.name == "feature" && b.current));
        assert_eq!(r.status().unwrap().branch.head.as_deref(), Some("feature"));
        let _ = std::fs::remove_dir_all(&r.root);
    }
}

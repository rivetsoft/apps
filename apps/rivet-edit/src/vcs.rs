//! Git in Rivet Edit: the workspace's repository, kept up to date in the background.
//!
//! The git work itself lives in the shared `rivet-git` crate, which has no user interface, so a
//! Git app can be built on it later. This file holds what's particular to the editor:
//! - when to ask for the status again;
//! - running commands off the window's thread;
//! - the staged copy of each open file;
//! - the hunks between that copy and the text being edited, which the gutter draws.
//!
//! The Git panel and the gutter marks are drawn in `gui/scm.rs`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::SystemTime;

use rivet_git::{Change, Hunk, Repo, Status};

/// Files bigger than this get no gutter marks (comparing them on every edit would be slow).
const MAX_DIFF: usize = 1_500_000;

/// How often the status is asked for again while the window has the keyboard (seconds). VS
/// Code watches the files instead; asking now and then is simpler, and cheap for git.
const STATUS_EVERY: f64 = 3.0;

/// A command for git, run in the background.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Op {
    Init(PathBuf),
    Stage(Vec<String>),
    Unstage(Vec<String>),
    StageAll,
    UnstageAll,
    /// Put tracked files back as staged, and delete new ones.
    Discard { tracked: Vec<String>, untracked: Vec<String> },
    /// Commit what's staged; `all` stages every change first.
    Commit { message: String, amend: bool, all: bool },
    UndoCommit,
    Checkout { name: String, remote: bool },
    CreateBranch(String),
    Fetch,
    Pull,
    Push,
    /// Pull, then push.
    Sync,
    /// Write a file's staged copy (staging or unstaging one hunk).
    SetIndex { path: String, text: String, what: &'static str },
}

impl Op {
    /// What's happening, for the status bar ("Pushing…").
    pub fn doing(&self) -> &'static str {
        match self {
            Op::Init(_) => "Making a repository…",
            Op::Stage(_) | Op::StageAll => "Staging…",
            Op::Unstage(_) | Op::UnstageAll => "Unstaging…",
            Op::Discard { .. } => "Discarding changes…",
            Op::Commit { .. } => "Committing…",
            Op::UndoCommit => "Undoing the last commit…",
            Op::Checkout { .. } | Op::CreateBranch(_) => "Switching branch…",
            Op::Fetch => "Fetching…",
            Op::Pull => "Pulling…",
            Op::Push => "Pushing…",
            Op::Sync => "Syncing…",
            Op::SetIndex { .. } => "Staging…",
        }
    }

    /// Network commands: they can take a while, and say so.
    fn remote(&self) -> bool {
        matches!(self, Op::Fetch | Op::Pull | Op::Push | Op::Sync)
    }
}

/// What a background job finished with.
enum Done {
    /// The status (or why not), for this repository.
    Status(PathBuf, rivet_git::Result<Status>),
    /// A repository found for a workspace (or none).
    Found(PathBuf, Option<Repo>),
    /// Staged copies read for this stamp: (path in the repository, its text).
    Bases(PathBuf, Option<SystemTime>, Vec<(String, Option<Arc<String>>)>),
    /// A command finished: what it was, and how it went.
    Op(Op, rivet_git::Result<String>),
    /// The branches, for the branch list.
    Branches(PathBuf, rivet_git::Result<Vec<rivet_git::BranchRef>>),
}

/// The marks one open file shows in the gutter.
#[derive(Default)]
pub(crate) struct Gutter {
    /// The staged copy the text was compared with.
    base: Option<Arc<String>>,
    /// The text the hunks were worked out for.
    seen: String,
    pub hunks: Vec<Hunk>,
}

impl Gutter {
    /// The hunks for `text` against `base`, working them out again only when either changed.
    pub fn update(&mut self, text: &str, base: Option<&Arc<String>>) {
        let same_base = match (&self.base, base) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same_base && self.seen == text {
            return;
        }
        self.base = base.cloned();
        self.seen.clear();
        self.seen.push_str(text);
        self.hunks = match base {
            Some(b) if text.len() <= MAX_DIFF && b.len() <= MAX_DIFF => rivet_git::diff::hunks(b, text),
            _ => Vec::new(),
        };
    }

    pub fn base(&self) -> Option<&Arc<String>> {
        self.base.as_ref()
    }
}

pub(crate) struct Git {
    /// The workspace's repository, once found.
    pub repo: Option<Repo>,
    /// The folder the repository was looked for in (looked for again when it changes).
    looked: Option<PathBuf>,
    pub looking: bool,
    pub status: Option<Status>,
    /// Why the last status failed.
    pub error: Option<String>,
    status_busy: bool,
    status_at: f64,
    /// The staged copy of open files, by path in the repository, read for `bases_stamp`. None:
    /// the file isn't tracked.
    pub bases: HashMap<String, Option<Arc<String>>>,
    bases_stamp: Option<SystemTime>,
    bases_busy: bool,
    /// The command running, if any.
    pub running: Option<Op>,
    /// The commit message being written.
    pub message: String,
    /// Commit replaces the last commit.
    pub amend: bool,
    /// The branch list, when asked for.
    pub branches: Option<Vec<rivet_git::BranchRef>>,
    branches_busy: bool,
    /// The workspace's changed files by their path in it, and the folders holding them.
    pub marks: HashMap<String, Change>,
    pub marked_dirs: HashSet<String>,
    /// Sections of the panel folded shut (by title).
    pub folded: HashSet<&'static str>,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    ctx: Option<eframe::egui::Context>,
    /// Run everything right away on the window's thread (screenshots, so the picture is ready).
    pub sync: bool,
}

impl Default for Git {
    fn default() -> Self {
        let (tx, rx) = channel();
        Git {
            repo: None,
            looked: None,
            looking: false,
            status: None,
            error: None,
            status_busy: false,
            status_at: f64::NEG_INFINITY,
            bases: HashMap::new(),
            bases_stamp: None,
            bases_busy: false,
            running: None,
            message: String::new(),
            amend: false,
            branches: None,
            branches_busy: false,
            marks: HashMap::new(),
            marked_dirs: HashSet::new(),
            folded: HashSet::new(),
            tx,
            rx,
            ctx: None,
            sync: false,
        }
    }
}

/// What finished commands tell the window: a message for the status bar (and whether it's
/// bad), and files whose text changed on disk.
#[derive(Default)]
pub(crate) struct News {
    pub note: Option<(String, bool)>,
    pub committed: bool,
}

impl Git {
    fn spawn(&self, f: impl FnOnce() -> Done + Send + 'static) {
        if self.sync {
            let _ = self.tx.send(f());
            return;
        }
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f());
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    /// Forget the repository (another workspace was opened). What's still running reports to
    /// nobody: each answer says which folder it was for.
    fn reset(&mut self) {
        self.repo = None;
        self.looking = false;
        self.status = None;
        self.error = None;
        self.status_busy = false;
        self.bases.clear();
        self.bases_stamp = None;
        self.bases_busy = false;
        self.message.clear();
        self.amend = false;
        self.branches = None;
        self.branches_busy = false;
        self.marks.clear();
        self.marked_dirs.clear();
        self.poke();
    }

    /// Ask for the status on the next tick (after a save, or something done outside).
    pub fn poke(&mut self) {
        self.status_at = f64::NEG_INFINITY;
    }

    /// The repository's path for a file, when it's in it.
    pub fn rel(&self, p: &Path) -> Option<String> {
        self.repo.as_ref()?.rel(p)
    }

    /// Each frame: find the repository for the workspace, ask for the status now and then,
    /// read the staged copies the open files need, and take in what finished. Returns what the
    /// window should know.
    pub fn tick(&mut self, ctx: &eframe::egui::Context, root: Option<&Path>, open: &[PathBuf], focused: bool) -> News {
        self.ctx = Some(ctx.clone());
        let mut news = News::default();
        // Another workspace: forget this one, and look for its repository.
        if root != self.looked.as_deref() {
            self.reset();
            self.looked = root.map(Path::to_path_buf);
            if let Some(r) = root {
                let r = r.to_path_buf();
                self.looking = true;
                self.spawn(move || Done::Found(r.clone(), Repo::discover(&r)));
            }
        }
        while let Ok(d) = self.rx.try_recv() {
            match d {
                Done::Found(r, repo) => {
                    if self.looked.as_deref() == Some(&r) {
                        self.looking = false;
                        self.repo = repo;
                        self.poke();
                    }
                }
                Done::Status(r, s) => {
                    self.status_busy = false;
                    if self.repo.as_ref().is_some_and(|x| x.root == r) {
                        match s {
                            Ok(s) => {
                                self.error = None;
                                if self.status.as_ref() != Some(&s) {
                                    self.status = Some(s);
                                    self.remark(root);
                                }
                            }
                            Err(e) => self.error = Some(e.0),
                        }
                    }
                }
                Done::Bases(r, stamp, got) => {
                    self.bases_busy = false;
                    if self.repo.as_ref().is_none_or(|x| x.root != r) {
                        continue;
                    }
                    if stamp != self.bases_stamp {
                        self.bases.clear();
                        self.bases_stamp = stamp;
                    }
                    self.bases.extend(got);
                }
                Done::Op(op, r) => {
                    self.running = None;
                    match r {
                        Ok(words) => {
                            if matches!(op, Op::Commit { .. }) {
                                self.message.clear();
                                self.amend = false;
                                news.committed = true;
                            }
                            if !words.is_empty() {
                                news.note = Some((words, false));
                            }
                        }
                        Err(e) => news.note = Some((e.0, true)),
                    }
                    self.branches = None;
                    self.poke();
                }
                Done::Branches(r, b) => {
                    if self.repo.as_ref().is_some_and(|x| x.root == r) {
                        self.branches_busy = false;
                        match b {
                            Ok(b) => self.branches = Some(b),
                            Err(e) => news.note = Some((e.0, true)),
                        }
                    }
                }
            }
        }
        let Some(repo) = self.repo.clone() else { return news };
        let now = ctx.input(|i| i.time);
        if !self.status_busy && self.running.is_none() && (now - self.status_at > STATUS_EVERY && (focused || self.status.is_none()) || self.status_at == f64::NEG_INFINITY) {
            self.status_at = now;
            self.status_busy = true;
            let r = repo.clone();
            self.spawn(move || Done::Status(r.root.clone(), r.status()));
        }
        if !self.sync {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(STATUS_EVERY));
        }
        // The staged copies: read again when the index or HEAD moved, and for files opened since.
        if !self.bases_busy && self.running.is_none() {
            let stamp = repo.stamp();
            let fresh = stamp == self.bases_stamp;
            let want: Vec<String> = open.iter().filter_map(|p| repo.rel(p)).filter(|r| !fresh || !self.bases.contains_key(r)).collect();
            if !want.is_empty() {
                self.bases_busy = true;
                let r = repo.clone();
                self.spawn(move || Done::Bases(r.root.clone(), stamp, want.into_iter().map(|p| (p.clone(), r.index_text(&p).map(Arc::new))).collect()));
            } else if !fresh && open.is_empty() {
                self.bases.clear();
                self.bases_stamp = stamp;
            }
        }
        news
    }

    /// The tree's marks again, from the status: each changed file by its path in the
    /// workspace, and the folders above them.
    fn remark(&mut self, root: Option<&Path>) {
        self.marks.clear();
        self.marked_dirs.clear();
        let (Some(repo), Some(st), Some(root)) = (&self.repo, &self.status, root) else { return };
        for e in &st.entries {
            let Ok(rel) = repo.root.join(&e.path).strip_prefix(root).map(|p| p.to_string_lossy().replace('\\', "/")) else { continue };
            let mut p = rel.as_str();
            while let Some((parent, _)) = p.rsplit_once('/') {
                if !self.marked_dirs.insert(parent.to_string()) {
                    break;
                }
                p = parent;
            }
            self.marks.insert(rel, e.overall());
        }
    }

    /// Run a command in the background (one at a time).
    pub fn run(&mut self, op: Op) -> bool {
        if self.running.is_some() {
            return false;
        }
        let repo = self.repo.clone();
        if repo.is_none() && !matches!(op, Op::Init(_)) {
            return false;
        }
        let branch = self.status.as_ref().map(|s| s.branch.clone()).unwrap_or_default();
        self.running = Some(op.clone());
        let o = op.clone();
        if let Op::Init(dir) = &op {
            let dir = dir.clone();
            self.spawn(move || {
                let r = Repo::init(&dir).map(|_| "Made a Git repository".to_string());
                Done::Op(o, r)
            });
            // Look for it again once made.
            self.looked = None;
            return true;
        }
        let repo = repo.unwrap_or_else(|| unreachable!());
        self.spawn(move || {
            let r = run_op(&repo, &op, &branch);
            Done::Op(o, r)
        });
        true
    }

    /// Ask for the branch list (for the branch picker), unless it's on its way.
    pub fn load_branches(&mut self) {
        if self.branches_busy {
            return;
        }
        if let Some(r) = self.repo.clone() {
            self.branches_busy = true;
            self.spawn(move || Done::Branches(r.root.clone(), r.branches()));
        }
    }

    /// Whether a network command is running (the sync button spins).
    pub fn syncing(&self) -> bool {
        self.running.as_ref().is_some_and(Op::remote)
    }
}

fn run_op(repo: &Repo, op: &Op, branch: &rivet_git::Branch) -> rivet_git::Result<String> {
    let n = |v: &[String]| if v.len() == 1 { v[0].rsplit('/').next().unwrap_or(&v[0]).to_string() } else { format!("{} files", v.len()) };
    Ok(match op {
        Op::Init(_) => unreachable!(),
        Op::Stage(p) => {
            repo.stage(p)?;
            format!("Staged {}", n(p))
        }
        Op::Unstage(p) => {
            repo.unstage(p)?;
            format!("Unstaged {}", n(p))
        }
        Op::StageAll => {
            repo.stage_all()?;
            "Staged every change".into()
        }
        Op::UnstageAll => {
            repo.unstage_all()?;
            "Unstaged everything".into()
        }
        Op::Discard { tracked, untracked } => {
            repo.discard(tracked)?;
            repo.remove_untracked(untracked)?;
            let all: Vec<String> = tracked.iter().chain(untracked).cloned().collect();
            format!("Discarded the changes to {}", n(&all))
        }
        Op::Commit { message, amend, all } => {
            if *all {
                repo.stage_all()?;
            }
            repo.commit(message, *amend)?;
            let first = message.lines().next().unwrap_or("").trim();
            if *amend { format!("Amended the last commit: {first}") } else { format!("Committed: {first}") }
        }
        Op::UndoCommit => {
            repo.undo_commit()?;
            "Undid the last commit; its changes are staged".into()
        }
        Op::Checkout { name, remote } => {
            repo.checkout(name, *remote)?;
            format!("On {}", if *remote { name.split_once('/').map(|x| x.1).unwrap_or(name) } else { name })
        }
        Op::CreateBranch(name) => {
            repo.create_branch(name)?;
            format!("Made the branch {name} and switched to it")
        }
        Op::Fetch => {
            repo.fetch()?;
            "Fetched".into()
        }
        Op::Pull => {
            repo.pull()?;
            "Pulled".into()
        }
        Op::Push => {
            repo.push(branch)?;
            "Pushed".into()
        }
        Op::Sync => {
            if branch.upstream.is_some() {
                repo.pull()?;
            }
            repo.push(branch)?;
            "Synced".into()
        }
        Op::SetIndex { path, text, what } => {
            repo.set_index_text(path, text)?;
            what.to_string()
        }
    })
}

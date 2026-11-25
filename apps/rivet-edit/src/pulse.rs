//! The checks made every few seconds: open files changed on disk, the workspace's file list and
//! Git's status. They run on a thread of their own and wake the window only when one finds
//! something new, so an idle window, even with a large file open, draws nothing.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use eframe::egui;
use rivet_git::{Repo, Status};

/// What the window wants watched, as of its last frame.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Watch {
    /// The open files, with their dates as read.
    pub files: Vec<(PathBuf, Option<SystemTime>)>,
    /// The workspace, when its file list is kept up to date (not for huge folders).
    pub root: Option<PathBuf>,
    /// The repository, while the window has the focus and no Git command runs.
    pub repo: Option<Repo>,
}

pub(crate) enum Found {
    /// An open file's date isn't the one it was read with.
    Disk,
    /// The workspace's files (and whether the list was cut short).
    Files(PathBuf, Vec<String>, bool),
}

/// Where Git's status goes (`crate::vcs::Git::status_reply`).
pub(crate) type StatusReply = Box<dyn Fn(PathBuf, rivet_git::Result<Status>) + Send>;

pub(crate) struct Pulse {
    watch: Arc<Mutex<Watch>>,
    rx: Receiver<Found>,
}

impl Pulse {
    pub fn start(ctx: egui::Context, status: StatusReply) -> Pulse {
        let watch = Arc::new(Mutex::new(Watch::default()));
        let (tx, rx) = channel();
        let w = watch.clone();
        std::thread::spawn(move || run(&w, &tx, &status, &ctx));
        Pulse { watch, rx }
    }

    pub fn watch(&self, w: Watch) {
        if let Ok(mut cur) = self.watch.lock()
            && *cur != w
        {
            *cur = w;
        }
    }

    pub fn found(&self) -> Vec<Found> {
        self.rx.try_iter().collect()
    }
}

fn run(watch: &Arc<Mutex<Watch>>, tx: &Sender<Found>, status: &StatusReply, ctx: &egui::Context) {
    let mut disk_told: Option<Vec<(PathBuf, Option<SystemTime>)>> = None;
    let mut files_seen: Option<(PathBuf, Vec<String>, bool)> = None;
    let mut status_seen: Option<(PathBuf, rivet_git::Result<Status>)> = None;
    for tick in 1u64.. {
        std::thread::sleep(Duration::from_secs(1));
        // The window has closed.
        if Arc::strong_count(watch) == 1 {
            return;
        }
        let Ok(w) = watch.lock().map(|w| w.clone()) else { return };
        let mut found = false;
        if tick % 2 == 0 {
            let changed: Vec<_> = w.files.iter().filter_map(|(p, read)| Some(std::fs::metadata(p).and_then(|m| m.modified()).ok()).filter(|now| now != read).map(|now| (p.clone(), now))).collect();
            // Said once per change: a file that can't be read again keeps its old date.
            if changed.is_empty() {
                disk_told = None;
            } else if disk_told.as_ref() != Some(&changed) {
                disk_told = Some(changed);
                found |= tx.send(Found::Disk).is_ok();
            }
        }
        if tick % 5 == 0
            && let Some(root) = w.root
        {
            let (files, cut) = crate::workspace::scan(&root);
            let now = (root, files, cut);
            if files_seen.as_ref() != Some(&now) {
                files_seen = Some(now.clone());
                found |= tx.send(Found::Files(now.0, now.1, now.2)).is_ok();
            }
        }
        if tick % 3 == 0
            && let Some(repo) = w.repo
        {
            let now = (repo.root.clone(), repo.status());
            if status_seen.as_ref() != Some(&now) {
                status_seen = Some(now.clone());
                status(now.0, now.1);
                found = true;
            }
        }
        if found {
            ctx.request_repaint();
        }
    }
}

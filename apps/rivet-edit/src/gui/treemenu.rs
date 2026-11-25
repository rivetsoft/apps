//! The sidebar's right-click menu: on a file, a folder, or the empty space below the tree
//! (the workspace itself). New files and folders, rename, move to the Trash, copy the path,
//! search in a folder, and open it in the file manager or a terminal.

use std::path::{Path, PathBuf};

use eframe::egui;

use super::draw::{item, menu_style};
use super::*;

/// What was chosen. Paths are inside the workspace ("" is the workspace itself).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TreeAct {
    Open(String),
    NewFile(String),
    NewFolder(String),
    Rename(String),
    Delete(String, bool),
    CopyPath(String),
    CopyRel(String),
    Reveal(String),
    Terminal(String),
    Search(String),
    /// A folder as the workspace of a new window.
    NewWindow(String),
    CollapseAll,
    Refresh,
}

/// The menu for a row (`rel`, a folder or not), or for the workspace when `rel` is empty.
pub(crate) fn tree_menu(ui: &mut egui::Ui, rel: &str, is_dir: bool, out: &mut Option<TreeAct>) {
    menu_style(ui);
    let r = rel.to_string();
    let here = if is_dir { r.clone() } else { r.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default() };
    let whole = rel.is_empty();
    let mut pick = |ui: &mut egui::Ui, label: &str, keys: &str, act: TreeAct| {
        if item(ui, label, keys, true) {
            *out = Some(act);
        }
    };
    if !is_dir && !whole {
        pick(ui, "Open", "", TreeAct::Open(r.clone()));
        ui.separator();
    }
    pick(ui, "New File…", "", TreeAct::NewFile(here.clone()));
    pick(ui, "New Folder…", "", TreeAct::NewFolder(here.clone()));
    if !whole {
        ui.separator();
        pick(ui, "Rename…", "", TreeAct::Rename(r.clone()));
        pick(ui, "Move to Trash…", "", TreeAct::Delete(r.clone(), is_dir));
    }
    ui.separator();
    pick(ui, "Copy Path", "", TreeAct::CopyPath(r.clone()));
    if !whole {
        pick(ui, "Copy Relative Path", "", TreeAct::CopyRel(r.clone()));
    }
    ui.separator();
    if is_dir || whole {
        pick(ui, if whole { "Search the Folder" } else { "Search in This Folder" }, if whole { "Shift+Ctrl+F" } else { "" }, TreeAct::Search(r.clone()));
    }
    pick(ui, if is_dir || whole { "Open in File Manager" } else { "Show in File Manager" }, "", TreeAct::Reveal(if is_dir || whole { r.clone() } else { here.clone() }));
    pick(ui, "Open Terminal Here", "", TreeAct::Terminal(here));
    if is_dir && !whole {
        pick(ui, "Open in New Window", "", TreeAct::NewWindow(r.clone()));
    }
    if whole {
        ui.separator();
        pick(ui, "Collapse All Folders", "", TreeAct::CollapseAll);
        pick(ui, "Reload Files", "", TreeAct::Refresh);
    }
}

/// Whether files may be changed (never in checks, unless they ask).
pub(crate) fn may_write() -> bool {
    !crate::settings::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed) || std::env::var_os("RIVET_EDIT_WRITE").is_some()
}

/// A name typed for a file or folder: no slashes at the ends, no "..".
fn clean(name: &str) -> Option<String> {
    let n = name.trim().trim_matches('/').to_string();
    (!n.is_empty() && !n.split('/').any(|p| p == ".." || p == ".")).then_some(n)
}

impl EditApp {
    fn abs(&self, rel: &str) -> Option<PathBuf> {
        let root = self.root.clone()?;
        Some(if rel.is_empty() { root } else { root.join(rel) })
    }

    pub(crate) fn tree_act(&mut self, ctx: &egui::Context, act: TreeAct) {
        let shot = self.shot.is_some();
        match act {
            TreeAct::Open(rel) => {
                if let Some(p) = self.abs(&rel) {
                    self.open_file(&p);
                }
            }
            TreeAct::NewFile(folder) => {
                self.open_dirs.insert(folder.clone());
                self.prompt = Some(Prompt::NewFile { folder, name: String::new() });
            }
            TreeAct::NewFolder(folder) => self.prompt = Some(Prompt::NewFolder { folder, name: String::new() }),
            TreeAct::Rename(rel) => {
                let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
                self.prompt = Some(Prompt::Rename { rel, name });
            }
            TreeAct::Delete(rel, dir) => self.prompt = Some(Prompt::Trash { rel, dir }),
            TreeAct::CopyPath(rel) | TreeAct::CopyRel(rel) if shot => self.note = Some((format!("Would copy {rel}"), false)),
            TreeAct::CopyPath(rel) => {
                if let Some(p) = self.abs(&rel) {
                    self.copy_out(ctx, p.display().to_string());
                }
            }
            TreeAct::CopyRel(rel) => self.copy_out(ctx, rel),
            TreeAct::Search(rel) => {
                self.search.scope = rel;
                self.search.rerun();
                self.open_search();
            }
            TreeAct::Reveal(rel) | TreeAct::Terminal(rel) if shot => self.note = Some((format!("Would open {}", if rel.is_empty() { "the workspace" } else { &rel }), false)),
            TreeAct::Reveal(rel) => {
                if let Some(p) = self.abs(&rel)
                    && let Err(e) = rivet_xdg::launch::open_url(&p.to_string_lossy())
                {
                    self.note = Some((format!("Couldn't open the file manager: {e}"), true));
                }
            }
            TreeAct::Terminal(rel) => {
                if let Some(p) = self.abs(&rel)
                    && let Err(e) = rivet_xdg::launch::open_terminal(&p)
                {
                    self.note = Some((format!("Couldn't open a terminal: {e}"), true));
                }
            }
            TreeAct::NewWindow(rel) => {
                if let Some(p) = self.abs(&rel) {
                    self.new_window(Some(p));
                }
            }
            TreeAct::CollapseAll => self.open_dirs.clear(),
            TreeAct::Refresh => self.rescan(),
        }
    }

    fn copy_out(&mut self, ctx: &egui::Context, text: String) {
        ctx.copy_text(text.clone());
        self.note = Some((format!("Copied {text}"), false));
    }

    /// Make a folder (and any folders on the way).
    pub(crate) fn make_folder(&mut self, folder: &str, name: &str) {
        let (Some(name), Some(base)) = (clean(name), self.abs(folder)) else { return };
        let p = base.join(&name);
        if !may_write() {
            return;
        }
        match std::fs::create_dir_all(&p) {
            Ok(()) => {
                let rel = if folder.is_empty() { name } else { format!("{folder}/{name}") };
                // Show it, and the folders above it.
                let mut acc = String::new();
                for part in rel.split('/') {
                    acc = if acc.is_empty() { part.to_string() } else { format!("{acc}/{part}") };
                    self.open_dirs.insert(acc.clone());
                }
                if !folder.is_empty() {
                    self.open_dirs.insert(folder.to_string());
                }
                self.rescan();
            }
            Err(e) => self.note = Some((format!("Couldn't make {}: {e}", p.display()), true)),
        }
    }

    /// Rename a file or folder in place (a new name, not a move); open tabs follow it.
    pub(crate) fn rename_path(&mut self, rel: &str, name: &str) {
        let Some(name) = clean(name).filter(|n| !n.contains('/')) else {
            self.note = Some(("A name can't have / in it".into(), true));
            return;
        };
        let Some(from) = self.abs(rel) else { return };
        let to = from.with_file_name(&name);
        if to == from || !may_write() {
            return;
        }
        if to.exists() {
            self.note = Some((format!("There's already a {name} there"), true));
            return;
        }
        match std::fs::rename(&from, &to) {
            Ok(()) => {
                self.follow_move(&from, &to);
                let new_rel = match rel.rsplit_once('/') {
                    Some((d, _)) => format!("{d}/{name}"),
                    None => name.clone(),
                };
                self.open_dirs = self.open_dirs.iter().map(|d| if d == rel || d.starts_with(&format!("{rel}/")) { format!("{new_rel}{}", &d[rel.len()..]) } else { d.clone() }).collect();
                self.rescan();
                self.note = Some((format!("Renamed to {name}"), false));
            }
            Err(e) => self.note = Some((format!("Couldn't rename: {e}"), true)),
        }
    }

    /// Tabs on files under `from` now point under `to`.
    fn follow_move(&mut self, from: &Path, to: &Path) {
        for t in &mut self.tabs {
            if let Some(p) = t.path.clone()
                && let Ok(rest) = p.strip_prefix(from)
            {
                t.path = Some(if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) });
            }
        }
    }

    /// Move to the Trash (the desktop's, so it can be restored); tabs on it close.
    pub(crate) fn trash_path(&mut self, rel: &str) {
        let Some(p) = self.abs(rel).filter(|_| !rel.is_empty()) else { return };
        if !may_write() {
            return;
        }
        match crate::workspace::trash(&p) {
            Ok(()) => {
                let inside: Vec<usize> = self.tabs.iter().enumerate().filter(|(_, t)| t.path.as_ref().is_some_and(|q| q.starts_with(&p))).map(|(i, _)| i).collect();
                for i in inside.into_iter().rev() {
                    self.close_tab(i, true);
                }
                self.rescan();
                self.note = Some((format!("Moved {} to the Trash", rel.rsplit('/').next().unwrap_or(rel)), false));
            }
            Err(e) => self.note = Some((format!("Couldn't move it to the Trash: {e}"), true)),
        }
    }
}

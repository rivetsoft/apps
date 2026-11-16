//! Editing a server file in a desktop editor, with saves going back to the server.
//!
//! The file is downloaded into a private folder of its own (`$XDG_RUNTIME_DIR/rivet-ftp/edit`,
//! readable only by you, and cleared when you log out) and opened in the editor. Rivet FTP then
//! watches that copy: once a save has settled, it checks that nobody changed the server's copy
//! since it was opened (asking first if they did) and uploads it over the original.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use eframe::egui;
use rivet_look::theme::{r_sm, status};
use rivet_xdg::launch::{self, DesktopApp};

use super::*;
use crate::listing::Entry;
use crate::sftp::{self, Request};

/// How long a changed file must stay unchanged before it's uploaded (editors may write in steps).
const SETTLE: f64 = 0.6;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum EditState {
    /// Downloading, then opening the editor.
    Opening,
    /// Watching for saves.
    Watching,
    /// Checking the server's copy before an upload.
    Checking,
    Uploading,
    /// Not connected to its server: saves wait for a reconnect.
    Paused,
    Failed(String),
}

pub(super) struct EditSession {
    /// The saved connection (or quick connection) it belongs to.
    pub conn: String,
    pub remote: String,
    pub local: PathBuf,
    /// The private folder holding the copy.
    folder: PathBuf,
    app: Option<DesktopApp>,
    pub state: EditState,
    /// The copy's date and size when last seen.
    seen: Option<(SystemTime, u64)>,
    /// When a change was first noticed (waiting for it to settle).
    changed_at: Option<f64>,
    /// A change is waiting to go up.
    dirty: bool,
    /// The server's copy as it was when opened or last saved: (date as listed, size).
    server: Option<(String, u64)>,
    /// Requests in flight on the transfer session.
    get_id: Option<u64>,
    put_id: Option<u64>,
    stat_id: Option<u64>,
    /// After an upload: a check that records the server's new date and size.
    refresh_id: Option<u64>,
    /// "Saved to the server at 14:03".
    pub saved_at: Option<String>,
}

fn file_info(p: &Path) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// A private folder for one edit: `$XDG_RUNTIME_DIR/rivet-ftp/edit/N-pid`, mode 0700.
fn edit_folder() -> Result<PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(rivet_xdg::places::cache_home);
    let root = base.join("rivet-ftp/edit");
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&root).map_err(|e| e.to_string())?;
    for n in 1.. {
        let dir = root.join(format!("{n}-{}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    unreachable!()
}

impl FtpApp {
    /// Installed apps (read once, when first needed).
    pub(super) fn apps(&mut self) -> &[DesktopApp] {
        if self.apps.is_none() {
            self.apps = Some(launch::all_apps());
        }
        self.apps.as_deref().unwrap_or(&[])
    }

    /// The editor for a file: the one chosen in Preferences, else the default app for its type,
    /// else the default text editor.
    pub(super) fn editor_for(&mut self, name: &str) -> Option<DesktopApp> {
        let mime = self.mime.guess_by_name(Path::new(name)).unwrap_or_else(|| "text/plain".into());
        let chosen = self.s.editor.clone();
        let apps = self.apps();
        if !chosen.is_empty()
            && let Some(a) = apps.iter().find(|a| a.id == chosen)
        {
            return Some(a.clone());
        }
        launch::default_app(apps, &mime).or_else(|| launch::default_app(apps, "text/plain")).cloned()
    }

    /// Apps that can open this file, for Open With.
    pub(super) fn editors_for(&mut self, name: &str) -> Vec<DesktopApp> {
        let mime = self.mime.guess_by_name(Path::new(name)).unwrap_or_else(|| "text/plain".into());
        let apps = self.apps();
        let mut v: Vec<DesktopApp> = launch::apps_for(apps, &mime).into_iter().cloned().collect();
        for a in launch::apps_for(apps, "text/plain") {
            if !v.iter().any(|x| x.id == a.id) {
                v.push(a.clone());
            }
        }
        v.truncate(12);
        v
    }

    /// Download a server file and open it in an editor; saves go back to the server.
    pub(super) fn edit_remote(&mut self, e: &Entry, app: Option<DesktopApp>) {
        if e.is_dir {
            return;
        }
        let Some(conn) = self.conn.clone() else { return };
        let remote = sftp::join(&self.remote.path, &e.name);
        // Already open: bring it back up in the editor.
        if let Some(ed) = self.edits.iter().find(|x| x.conn == conn.name && x.remote == remote && x.state != EditState::Opening)
            && let Some(a) = app.clone().or_else(|| ed.app.clone())
        {
            let _ = launch::launch_app(&a, std::slice::from_ref(&ed.local));
            return;
        }
        let app = app.or_else(|| self.editor_for(&e.name));
        if app.is_none() {
            self.note = Some((format!("No app opens \"{}\". Choose one with Open With, or set an editor in Preferences.", e.name), true));
            return;
        }
        let folder = match edit_folder() {
            Ok(f) => f,
            Err(err) => {
                self.note = Some((format!("Couldn't make a folder for the copy: {err}"), true));
                return;
            }
        };
        let local = folder.join(&e.name);
        let Some(t) = self.transfer.as_mut() else { return };
        let get_id = t.send(Request::Get(remote.clone(), local.clone(), false));
        self.edits.push(EditSession {
            conn: conn.name.clone(),
            remote,
            local,
            folder,
            app,
            state: EditState::Opening,
            seen: None,
            changed_at: None,
            dirty: false,
            server: Some((e.modified.clone(), e.size)),
            get_id: Some(get_id),
            put_id: None,
            stat_id: None,
            refresh_id: None,
            saved_at: None,
        });
        self.note = Some((format!("Opening {}…", e.name), false));
    }

    /// Answers from the transfer session that belong to edits. Returns the rest.
    pub(super) fn edit_replies(&mut self, done: Vec<(u64, Option<String>)>, stats: Vec<(u64, Option<Entry>)>) -> Vec<(u64, Option<String>)> {
        let mut rest = Vec::new();
        let mut notes: Vec<(String, bool)> = Vec::new();
        let mut refresh = false;
        for (id, err) in done {
            // Request numbers are per connection: only this tab's edits.
            let here = self.conn.as_ref().map(|c| c.name.clone());
            let Some(ed) = self.edits.iter_mut().find(|e| Some(&e.conn) == here.as_ref() && (e.get_id == Some(id) || e.put_id == Some(id) || e.stat_id == Some(id) || e.refresh_id == Some(id))) else {
                rest.push((id, err));
                continue;
            };
            let name = file_name(&ed.remote);
            if ed.get_id == Some(id) {
                ed.get_id = None;
                match err {
                    Some(e) => ed.state = EditState::Failed(format!("Couldn't download it: {}", first_line(&e))),
                    None => {
                        ed.seen = file_info(&ed.local);
                        ed.state = EditState::Watching;
                        if let Some(a) = &ed.app {
                            match launch::launch_app(a, std::slice::from_ref(&ed.local)) {
                                Ok(()) => notes.push((format!("Editing {name} in {}. Saves go back to the server.", a.name), false)),
                                Err(e) => ed.state = EditState::Failed(format!("Couldn't open {}: {e}", a.name)),
                            }
                        }
                    }
                }
            } else if ed.put_id == Some(id) {
                ed.put_id = None;
                match err {
                    Some(e) => ed.state = EditState::Failed(format!("Couldn't save to the server: {}", first_line(&e))),
                    None => {
                        ed.state = EditState::Watching;
                        ed.saved_at = Some(chrono::Local::now().format("%H:%M").to_string());
                        notes.push((format!("Saved {name} to the server."), false));
                        refresh |= sftp::parent(&ed.remote) == self.remote.path;
                        // Learn the server's new date and size, for the next save's check.
                        if let Some(t) = self.transfer.as_mut() {
                            ed.refresh_id = Some(t.send(Request::Stat(ed.remote.clone())));
                        }
                    }
                }
            } else {
                // A check that failed (couldn't list the file).
                ed.stat_id = None;
                ed.refresh_id = None;
                if let Some(e) = err {
                    ed.state = EditState::Failed(format!("Couldn't check the server's copy: {}", first_line(&e)));
                }
            }
        }
        for (id, entry) in stats {
            let here = self.conn.as_ref().map(|c| c.name.clone());
            let Some(i) = self.edits.iter().position(|e| Some(&e.conn) == here.as_ref() && (e.stat_id == Some(id) || e.refresh_id == Some(id))) else { continue };
            let ed = &mut self.edits[i];
            if ed.refresh_id == Some(id) {
                ed.refresh_id = None;
                ed.server = entry.map(|e| (e.modified, e.size));
                continue;
            }
            ed.stat_id = None;
            let now = entry.as_ref().map(|e| (e.modified.clone(), e.size));
            if now.is_some() && ed.server.is_some() && now != ed.server {
                // Someone changed it on the server since it was opened (or last saved).
                ed.state = EditState::Watching;
                self.prompt = Some(Prompt::EditConflict(i));
            } else {
                self.upload_edit(i);
            }
        }
        for n in notes {
            self.note = Some(n);
        }
        if refresh {
            self.reload_remote();
        }
        rest
    }

    /// Send the edited copy over the server's file.
    pub(super) fn upload_edit(&mut self, i: usize) {
        let Some(t) = self.transfer.as_mut() else { return };
        let ed = &mut self.edits[i];
        ed.dirty = false;
        ed.state = EditState::Uploading;
        ed.put_id = Some(t.send(Request::Put(ed.local.clone(), ed.remote.clone(), false)));
    }

    /// Keep an eye on the copies: a settled change is checked against the server, then uploaded.
    pub(super) fn watch_edits(&mut self, ctx: &egui::Context) {
        if self.edits.is_empty() {
            return;
        }
        let now = ctx.input(|i| i.time);
        ctx.request_repaint_after(std::time::Duration::from_millis(400));
        let current = self.conn.as_ref().map(|c| c.name.clone());
        let live = self.transfer.is_some();
        // Connections open in another tab: that tab looks after their edits.
        let elsewhere: Vec<String> = self.tabs.iter().filter_map(|t| t.conn_name()).collect();
        for i in 0..self.edits.len() {
            let ed = &mut self.edits[i];
            let here = live && current.as_deref() == Some(ed.conn.as_str());
            if !here && elsewhere.contains(&ed.conn) {
                continue;
            }
            match (&ed.state, here) {
                (EditState::Paused, true) => ed.state = EditState::Watching,
                (EditState::Opening | EditState::Checking | EditState::Uploading, false) => {
                    ed.state = EditState::Paused;
                    (ed.get_id, ed.put_id, ed.stat_id, ed.refresh_id) = (None, None, None, None);
                    if ed.seen.is_none() {
                        ed.state = EditState::Failed("Disconnected before it opened".into());
                    }
                }
                (EditState::Watching, false) => ed.state = EditState::Paused,
                _ => {}
            }
            if !matches!(ed.state, EditState::Watching | EditState::Paused | EditState::Failed(_)) || ed.seen.is_none() {
                continue;
            }
            let info = file_info(&ed.local);
            if info.is_some() && info != ed.seen {
                ed.seen = info;
                ed.changed_at = Some(now);
                ed.dirty = true;
                // A new save after a failed one: try again.
                if matches!(ed.state, EditState::Failed(_)) {
                    ed.state = EditState::Watching;
                }
            }
            let settled = ed.changed_at.is_some_and(|t| now - t >= SETTLE);
            if ed.dirty && settled && ed.state == EditState::Watching && here {
                ed.changed_at = None;
                ed.state = EditState::Checking;
                if let Some(t) = self.transfer.as_mut() {
                    ed.stat_id = Some(t.send(Request::Stat(ed.remote.clone())));
                }
            }
        }
    }

    /// Stop watching an edit and delete its private copy.
    pub(super) fn stop_edit(&mut self, i: usize) {
        if i >= self.edits.len() {
            return;
        }
        let ed = self.edits.remove(i);
        if ed.folder.starts_with(std::env::temp_dir()) || ed.folder.to_string_lossy().contains("/rivet-ftp/edit/") {
            let _ = std::fs::remove_dir_all(&ed.folder);
        }
        if let Some(Prompt::EditConflict(j)) = &self.prompt {
            if *j == i {
                self.prompt = None;
            } else if *j > i {
                self.prompt = Some(Prompt::EditConflict(j - 1));
            }
        }
    }

    /// Try a failed save again.
    fn retry_edit(&mut self, i: usize) {
        let ed = &mut self.edits[i];
        ed.state = EditState::Watching;
        ed.dirty = true;
        ed.changed_at = Some(0.0);
    }

    /// The files open in editors, above the transfers.
    pub(super) fn edits_panel(&mut self, ui: &mut egui::Ui) {
        if self.edits.is_empty() {
            return;
        }
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let weak = v.weak_text_color();
        let mut stop = None;
        let mut retry = None;
        let mut reopen = None;
        for (i, ed) in self.edits.iter().enumerate() {
            ui.horizontal(|ui| {
                let name = file_name(&ed.remote);
                ui.label(egui::RichText::new("Editing").color(v.selection.bg_fill).strong());
                ui.label(&name).on_hover_text(format!("{} on {}\nCopy: {}", ed.remote, ed.conn, ed.local.display()));
                let app = ed.app.as_ref().map(|a| format!("in {}", a.name)).unwrap_or_default();
                ui.label(egui::RichText::new(format!("{app} · {}", ed.conn)).color(weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if ui.add(egui::Button::new("Stop").corner_radius(r_sm())).on_hover_text("Stop sending saves to the server and delete the copy").clicked() {
                        stop = Some(i);
                    }
                    match &ed.state {
                        EditState::Opening => {
                            ui.add(egui::Spinner::new().size(13.0));
                            ui.label(egui::RichText::new("Opening").color(weak));
                        }
                        EditState::Checking | EditState::Uploading => {
                            ui.add(egui::Spinner::new().size(13.0));
                            ui.label(egui::RichText::new("Saving to the server").color(weak));
                        }
                        EditState::Watching => {
                            if ui.add(egui::Button::new("Open").corner_radius(r_sm())).on_hover_text("Bring it up in the editor again").clicked() {
                                reopen = Some(i);
                            }
                            let text = match &ed.saved_at {
                                Some(t) => format!("Saved to the server at {t}"),
                                None => "Saves go to the server".into(),
                            };
                            ui.label(egui::RichText::new(text).color(weak));
                        }
                        EditState::Paused => {
                            let text = if ed.dirty { "Changes wait: reconnect to save them" } else { "Reconnect to save changes" };
                            ui.label(egui::RichText::new(text).color(status(&ctx).warning));
                        }
                        EditState::Failed(e) => {
                            if ed.seen.is_some() && ui.add(egui::Button::new("Try Again").corner_radius(r_sm())).clicked() {
                                retry = Some(i);
                            }
                            ui.label(egui::RichText::new("Not saved").color(status(&ctx).danger)).on_hover_text(e);
                        }
                    }
                });
            });
        }
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(2.0);
        if let Some(i) = reopen
            && let Some(a) = self.edits[i].app.clone()
        {
            let _ = launch::launch_app(&a, std::slice::from_ref(&self.edits[i].local));
        }
        if let Some(i) = retry {
            self.retry_edit(i);
        }
        if let Some(i) = stop {
            self.stop_edit(i);
        }
    }
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

use super::logic::first_line;

impl Drop for FtpApp {
    /// Closing the window deletes the editing copies, except ones holding changes that didn't
    /// reach the server (those stay, so nothing typed is lost).
    fn drop(&mut self) {
        for ed in &self.edits {
            let unsaved = ed.dirty || matches!(ed.state, EditState::Failed(_) | EditState::Checking | EditState::Uploading | EditState::Paused);
            if unsaved {
                eprintln!("rivet-ftp: unsaved changes to {} are kept in {}", ed.remote, ed.local.display());
            } else if ed.folder.to_string_lossy().contains("/rivet-ftp/edit/") {
                let _ = std::fs::remove_dir_all(&ed.folder);
            }
        }
    }
}

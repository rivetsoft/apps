//! Preview with Rivet's light box (Space, or right-click → Preview): pictures, videos, music
//! covers and code, stepping through the folder with ← / →. Files on this computer are shown
//! as they are; server files are downloaded first into a private temporary folder (deleted
//! when the light box closes).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;
use rivet_look::{builtin_icons, highlight, palette};
use rivet_media::lightbox::{self, Event, Item, LightBox, Look, Opener};

use super::*;
use crate::sftp::{self, Request};

/// Server files bigger than this aren't downloaded just to preview them.
const MAX_REMOTE: u64 = 200 * 1024 * 1024;

pub(super) struct PreviewState {
    pub lb: LightBox,
    side: Side,
    /// Server previews: the folder holding the downloaded copies.
    folder: Option<PathBuf>,
    /// Downloads in flight: request id → local copy.
    gets: HashMap<u64, PathBuf>,
    /// Local copy → server path, for the ones not downloaded yet.
    remote_of: HashMap<PathBuf, String>,
    /// Local copy → the listing entry (to open it in an editor).
    entries: HashMap<PathBuf, crate::listing::Entry>,
}

impl Drop for PreviewState {
    fn drop(&mut self) {
        if let Some(f) = &self.folder
            && f.to_string_lossy().contains("/rivet-ftp/preview-")
        {
            let _ = std::fs::remove_dir_all(f);
        }
    }
}

fn preview_folder() -> Result<PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(rivet_xdg::places::cache_home);
    let root = base.join("rivet-ftp");
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&root).map_err(|e| e.to_string())?;
    for n in 1.. {
        let dir = root.join(format!("preview-{}-{n}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    unreachable!()
}

impl FtpApp {
    fn mime_of(&self, name: &str) -> String {
        self.mime.guess_by_name(Path::new(name)).unwrap_or_else(|| "application/octet-stream".into())
    }

    /// Open the light box on the selection of a pane.
    pub(super) fn open_preview(&mut self, side: Side) {
        let pane = self.pane_ref(side);
        let sel = pane.selected_entries();
        let Some(first) = sel.iter().find(|e| !e.is_dir).cloned() else { return };
        let entries: Vec<crate::listing::Entry> = pane.entries.iter().filter(|e| !e.is_dir && lightbox::viewable(&self.mime_of(&e.name))).cloned().collect();
        if !entries.iter().any(|e| e.name == first.name) {
            self.note = Some((format!("There's no preview for \"{}\".", first.name), false));
            return;
        }
        let (folder, base) = match side {
            Side::Local => (None, PathBuf::from(&self.local.path)),
            Side::Remote => match preview_folder() {
                Ok(f) => (Some(f.clone()), f),
                Err(e) => {
                    self.note = Some((format!("Couldn't make a folder for the preview: {e}"), true));
                    return;
                }
            },
        };
        let mut remote_of = HashMap::new();
        let mut by_path = HashMap::new();
        let items: Vec<Item> = entries
            .iter()
            .map(|e| {
                let path = base.join(&e.name);
                if side == Side::Remote {
                    remote_of.insert(path.clone(), sftp::join(&self.remote.path, &e.name));
                }
                by_path.insert(path.clone(), e.clone());
                Item { path, name: e.name.clone(), mime: self.mime_of(&e.name), size: e.size, ready: side == Side::Local }
            })
            .collect();
        let idx = entries.iter().position(|e| e.name == first.name).unwrap_or(0);
        self.preview = Some(PreviewState { lb: LightBox::new(items, idx), side, folder, gets: HashMap::new(), remote_of, entries: by_path });
        self.fetch_preview();
    }

    /// Server previews: download the current file (and the next one) if not done yet.
    fn fetch_preview(&mut self) {
        let Some(p) = self.preview.as_mut() else { return };
        if p.side != Side::Remote {
            return;
        }
        let idx = p.lb.index();
        let wanted: Vec<Item> = [idx, idx + 1].iter().filter_map(|i| p.lb.items().get(*i).cloned()).collect();
        let Some(t) = self.transfer.as_mut() else { return };
        for it in wanted {
            if it.size > MAX_REMOTE {
                continue;
            }
            if let Some(remote) = p.remote_of.remove(&it.path) {
                let id = t.send(Request::Get(remote, it.path.clone(), false));
                p.gets.insert(id, it.path.clone());
            }
        }
    }

    /// Transfer answers for preview downloads. Returns the rest.
    pub(super) fn preview_replies(&mut self, done: Vec<(u64, Option<String>)>) -> Vec<(u64, Option<String>)> {
        // The light box belongs to the open tab.
        if self.in_background {
            return done;
        }
        let Some(p) = self.preview.as_mut() else { return done };
        let mut rest = Vec::new();
        for (id, err) in done {
            match p.gets.remove(&id) {
                Some(path) => {
                    if err.is_none() {
                        p.lb.set_ready(&path);
                    } else {
                        // Shown as "no preview" rather than spinning forever.
                        let _ = std::fs::write(&path, b"");
                        p.lb.set_ready(&path);
                    }
                }
                None => rest.push((id, err)),
            }
        }
        rest
    }

    /// Draw the light box (if open) and act on it.
    pub(super) fn preview_window(&mut self, ctx: &egui::Context) {
        let Some(p) = self.preview.as_ref() else { return };
        let side = p.side;
        let cur = p.lb.current().clone();
        let (_, accent, _) = self.look();
        let v = ctx.global_style().visuals.clone();
        let code_bg = palette::mix(v.panel_fill, v.extreme_bg_color, 0.55);
        let look = Look {
            accent,
            code_colors: highlight::colors(v.text_color(), code_bg, accent, builtin_icons::derive_secondary(accent), builtin_icons::derive_tertiary(accent)),
            code_bg,
        };
        // The open button: the file's app here; the editor for server files (saves go back).
        let app = match side {
            Side::Local => {
                let apps = self.apps().to_vec();
                rivet_xdg::launch::default_app(&apps, &cur.mime).cloned()
            }
            Side::Remote => self.editor_for(&cur.name),
        };
        let icon = app.as_ref().and_then(|a| {
            let px = (18.0 * ctx.pixels_per_point()) as u32;
            if a.icon.starts_with('/') { self.icons.texture_from_path(ctx, Path::new(&a.icon), px) } else { self.icons.texture(ctx, &[a.icon.as_str(), "application-x-executable"], px) }
        });
        let label = match (&app, side) {
            (Some(a), Side::Local) => format!("Open with {}", a.name),
            (Some(a), Side::Remote) => format!("Edit in {}", a.name),
            (None, _) => "Open".into(),
        };
        let opener = Opener { label, icon, choose: false };
        let Some(p) = self.preview.as_mut() else { return };
        let events = p.lb.show(ctx, &look, &opener);
        let cur = p.lb.current().clone();
        let entry = p.entries.get(&cur.path).cloned();
        for ev in events {
            match ev {
                Event::Moved(_) => {
                    if let Some(e) = &entry {
                        self.select_only(side, &e.name.clone());
                    }
                    self.fetch_preview();
                }
                Event::Open => match side {
                    Side::Local => {
                        let apps = self.apps().to_vec();
                        if let Err(e) = rivet_xdg::launch::open_default(&apps, &cur.mime, &cur.path) {
                            self.note = Some((format!("Couldn't open it: {e}"), true));
                        }
                    }
                    Side::Remote => {
                        if let Some(e) = &entry {
                            self.edit_remote(e, None);
                        }
                    }
                },
                Event::ChooseApp => {}
                Event::Close => self.preview = None,
            }
        }
    }
}

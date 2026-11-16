//! Connecting, browsing, transfers and file actions.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

use eframe::egui::{self, Key};
use rivet_connections::{Connection, Protocol, Store};
use rivet_look::icons::{self, IconTheme};
use rivet_look::{fonts, palette, theme};
use rivet_xdg::mime::MimeDb;

use super::*;
use crate::settings::OnExists;
use crate::listing::{self, Entry};
use crate::sftp::{self, Reply, Request, Session};

impl FtpApp {
    pub fn new(cc: &eframe::CreationContext, open: Option<String>, screenshot: Option<std::path::PathBuf>) -> FtpApp {
        let (pal, watched) = palette::load();
        let home = rivet_xdg::places::home();
        let mut app = FtpApp {
            store: Store::load(),
            mime: MimeDb::load(),
            icons: IconTheme::new(&icons::detect_system_theme()),
            palette: pal,
            look_key: None,
            font_set: false,
            local: Pane { path: home.to_string_lossy().into_owned(), ..Default::default() },
            remote: Pane::default(),
            focus: Side::Local,
            s: if screenshot.is_some() { Settings::default() } else { Settings::load() },
            remote_home: None,
            focus_quick: false,
            prefs_open: false,
            prefs_page: super::prefs::Page::Appearance,
            prefs_win: rivet_look::windows::WinState::new(10, eframe::egui::Vec2::new(800.0, 620.0)),
            keys_open: false,
            keys_win: rivet_look::windows::WinState::new(11, eframe::egui::Vec2::new(480.0, 520.0)),
            app_icon: None,
            palette_watch: watched.into_iter().map(|p| { let t = mtime(&p); (p, t) }).collect(),
            palette_checked: 0.0,
            conn: None,
            browse: None,
            transfer: None,
            connecting: false,
            pending: HashMap::new(),
            list_req: None,
            queue: Vec::new(),
            quick: String::new(),
            note: None,
            form: None,
            prompt: None,
            shot: screenshot.map(rivet_kit::Shot::new),
            hook_send: std::env::var("RIVET_FTP_SEND").ok(),
            keys: Vec::new(),
            keys_rx: None,
            keys_asked: false,
            new_key: None,
            shot_wait: 0,
        };
        if screenshot_mode(&app) {
            crate::settings::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if app.s.remember_local && std::path::Path::new(&app.s.last_local).is_dir() {
            app.local.path = app.s.last_local.clone();
        }
        // Test hooks for checks: start in this local folder; a style; a Preferences page.
        if let Ok(p) = std::env::var("RIVET_FTP_LOCAL") {
            app.local.path = p;
        }
        match std::env::var("RIVET_FTP_THEME").as_deref() {
            Ok("light") => (app.s.theme, app.s.follow_desktop_colors) = (theme::Mode::Light, false),
            Ok("dark") => (app.s.theme, app.s.follow_desktop_colors) = (theme::Mode::Dark, false),
            _ => {}
        }
        if let Ok(p) = std::env::var("RIVET_FTP_PREFS") {
            app.prefs_open = true;
            app.prefs_page = match p.as_str() {
                "transfers" => super::prefs::Page::Transfers,
                "connections" => super::prefs::Page::Connections,
                "keys" => super::prefs::Page::Keys,
                "about" => super::prefs::Page::About,
                _ => super::prefs::Page::Appearance,
            };
        }
        app.keys_open = std::env::var_os("RIVET_FTP_KEYS").is_some();
        if std::env::var_os("RIVET_FTP_NEWKEY").is_some() {
            app.new_key = Some(NewKey::new(false));
        }
        if std::env::var_os("RIVET_FTP_NO_MENUBAR").is_some() {
            app.s.show_menubar = false;
        }
        app.reload_local();
        if let Some(name) = open {
            match app.store.find(&name).cloned() {
                Some(c) => app.connect(c, &cc.egui_ctx),
                None => app.note = Some((format!("No saved connection called \"{name}\"."), true)),
            }
        }
        // Test hook for checks: open the connection editor.
        if std::env::var_os("RIVET_FTP_FORM").is_some() {
            app.form = Some(Form::blank());
        }
        app
    }

    // ---------------------------------------------------------------- look

    pub(super) fn apply_look(&mut self, ctx: &egui::Context) {
        if !self.font_set {
            self.font_set = true;
            let mut defs = egui::FontDefinitions::default();
            if let Some((_, path)) = fonts::resolve("")
                && let Ok(bytes) = std::fs::read(&path)
            {
                defs.font_data.insert("ui-font".into(), Arc::new(egui::FontData::from_owned(bytes)));
                defs.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "ui-font".into());
            }
            ctx.set_fonts(defs);
        }
        let size = if self.s.text_size > 0.0 { self.s.text_size } else { fonts::desktop_font_cached().map(|(_, pt)| fonts::points_to_ui(pt)).unwrap_or(fonts::BASE_SIZE) };
        fonts::set_scale(size / fonts::BASE_SIZE);
        let (mode, accent, pal) = self.look();
        let pal = pal.cloned();
        let key = format!("{mode:?}{accent:?}{pal:?}{size}");
        if self.look_key.as_ref() == Some(&key) {
            return;
        }
        self.look_key = Some(key);
        theme::apply(ctx, &theme::Look { mode, accent, palette: pal.as_ref(), terminal: false });
    }

    /// Follow the desktop's colors live: every two seconds, see whether a color file changed.
    pub(super) fn watch_palette(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.palette_checked < 2.0 && self.palette_checked > 0.0 {
            return;
        }
        self.palette_checked = now;
        let changed = self.palette_watch.iter().any(|(p, t)| mtime(p) != *t);
        if changed {
            let (pal, watched) = palette::load();
            self.palette = pal;
            self.palette_watch = watched.into_iter().map(|p| { let t = mtime(&p); (p, t) }).collect();
        }
        if self.s.follow_desktop_colors && self.shot.is_none() {
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }
    }

    pub(super) fn icon(&mut self, ctx: &egui::Context, e: &Entry) -> Option<egui::TextureHandle> {
        let px = (16.0 * ctx.pixels_per_point()).round() as u32;
        if e.is_dir {
            return self.icons.texture(ctx, &["folder", "inode-directory"], px);
        }
        let mime = self.mime.guess_by_name(Path::new(&e.name)).unwrap_or_else(|| "application/octet-stream".into());
        let names = self.mime.icon_names(&mime);
        let mut refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        refs.push("text-x-generic");
        self.icons.texture(ctx, &refs, px)
    }

    pub(super) fn named_icon(&mut self, ctx: &egui::Context, names: &[&str]) -> Option<egui::TextureHandle> {
        let px = (16.0 * ctx.pixels_per_point()).round() as u32;
        self.icons.texture(ctx, names, px)
    }

    // ---------------------------------------------------------------- connecting

    pub(super) fn connect(&mut self, c: Connection, ctx: &egui::Context) {
        self.disconnect();
        if c.protocol != Protocol::Sftp {
            self.note = Some((format!("\"{}\" is saved. FTP and FTPS browsing comes next; for now Rivet FTP connects over SFTP.", c.name), true));
            return;
        }
        if let Some(l) = &c.local {
            let p = expand(l);
            if p.is_dir() {
                self.local.path = p.to_string_lossy().into_owned();
                self.reload_local();
            }
        }
        self.browse = Some(Session::open(&c, ctx.clone()));
        self.connecting = true;
        self.remote = Pane { loading: true, sort: self.remote.sort, ..Default::default() };
        self.note = Some((format!("Connecting to {}…", c.target()), false));
        self.conn = Some(c);
    }

    pub(super) fn disconnect(&mut self) {
        self.browse = None;
        self.transfer = None;
        self.conn = None;
        self.connecting = false;
        self.pending.clear();
        self.list_req = None;
        self.remote = Pane { sort: self.remote.sort, ..Default::default() };
        self.remote_home = None;
        for j in &mut self.queue {
            if j.state == JobState::Waiting {
                j.state = JobState::Failed("Disconnected".into());
            }
        }
    }

    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        let mut replies = Vec::new();
        if let Some(s) = &self.browse {
            while let Ok(r) = s.rx.try_recv() {
                replies.push(r);
            }
        }
        for r in replies {
            match r {
                Reply::Connected(home) => {
                    self.connecting = false;
                    self.remote_home = Some(home.clone());
                    let start = self.conn.as_ref().and_then(|c| c.remote.clone()).unwrap_or(home);
                    if let Some(c) = &self.conn {
                        self.note = Some((format!("Connected to {}.", c.target()), false));
                        // Transfers get their own session, so browsing never waits behind them.
                        self.transfer = Some(Session::open(c, ctx.clone()));
                    }
                    self.open_remote(start);
                }
                Reply::Listing(path, entries) => {
                    if path == self.remote.path {
                        let hidden = self.s.show_hidden;
                        let mut entries: Vec<Entry> = entries.into_iter().filter(|e| hidden || !e.name.starts_with('.')).collect();
                        listing::sort_by(&mut entries, self.remote.sort);
                        self.remote.entries = entries;
                        let names: BTreeSet<String> = self.remote.entries.iter().map(|e| e.name.clone()).collect();
                        self.remote.selected.retain(|n| names.contains(n));
                        self.remote.loading = false;
                        self.remote.error = None;
                        if let Some(spec) = self.hook_send.take() {
                            self.run_send_hook(&spec);
                        }
                    }
                }
                Reply::Done(id, err) => {
                    let what = self.pending.remove(&id);
                    if Some(id) == self.list_req {
                        self.list_req = None;
                        self.remote.loading = false;
                        if let Some(e) = err {
                            self.remote.error = Some(e);
                            self.remote.entries.clear();
                        }
                        continue;
                    }
                    match err {
                        Some(e) => self.note = Some((format!("{} failed: {}", what.unwrap_or_else(|| "That".into()), first_line(&e)), true)),
                        None => {
                            if let Some(w) = what {
                                self.note = Some((w, false));
                            }
                        }
                    }
                    self.reload_remote();
                }
                Reply::Closed(why) => {
                    let name = self.conn.as_ref().map(|c| c.name.clone()).unwrap_or_default();
                    self.disconnect();
                    self.note = Some((if name.is_empty() { why } else { format!("{name}: {why}") }, true));
                }
            }
        }
        // The transfer session.
        let mut done = Vec::new();
        let mut closed = None;
        if let Some(s) = &self.transfer {
            while let Ok(r) = s.rx.try_recv() {
                match r {
                    Reply::Done(id, err) => done.push((id, err)),
                    Reply::Closed(why) => closed = Some(why),
                    _ => {}
                }
            }
        }
        let (mut local, mut remote) = (false, false);
        for (id, err) in done {
            if let Some(j) = self.queue.iter_mut().find(|j| j.id == id) {
                j.state = match err {
                    None => JobState::Done,
                    Some(e) => JobState::Failed(first_line(&e).to_string()),
                };
                match j.dir {
                    Dir::Up => remote |= j.to_dir == self.remote.path,
                    Dir::Down => local |= j.to_dir == self.local.path,
                }
            }
        }
        if self.s.clear_finished {
            self.queue.retain(|j| j.state != JobState::Done);
        }
        if let Some(why) = closed {
            self.transfer = None;
            for j in &mut self.queue {
                if j.state == JobState::Waiting {
                    j.state = JobState::Failed(why.clone());
                }
            }
        }
        if local {
            self.reload_local();
        }
        if remote {
            self.reload_remote();
        }
        if self.queue.iter().any(|j| j.state == JobState::Waiting) {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    // ---------------------------------------------------------------- navigation

    pub(super) fn reload_local(&mut self) {
        match listing::read_local(Path::new(&self.local.path), self.s.show_hidden) {
            Ok(mut v) => {
                listing::sort_by(&mut v, self.local.sort);
                self.local.entries = v;
                self.local.error = None;
            }
            Err(e) => {
                self.local.entries.clear();
                self.local.error = Some(e);
            }
        }
        let names: BTreeSet<String> = self.local.entries.iter().map(|e| e.name.clone()).collect();
        self.local.selected.retain(|n| names.contains(n));
        if self.local.cursor.is_some_and(|c| c >= self.local.entries.len()) {
            self.local.cursor = None;
        }
    }

    pub(super) fn reload_remote(&mut self) {
        if self.remote.path.is_empty() {
            return;
        }
        let p = self.remote.path.clone();
        if let Some(s) = self.browse.as_mut() {
            self.list_req = Some(s.send(Request::List(p)));
            self.remote.loading = true;
        }
    }

    pub(super) fn reload(&mut self, side: Side) {
        match side {
            Side::Local => self.reload_local(),
            Side::Remote => self.reload_remote(),
        }
    }

    pub(super) fn open_local(&mut self, path: String) {
        let old = std::mem::replace(&mut self.local.path, path);
        self.local.selected.clear();
        self.local.cursor = None;
        self.reload_local();
        if self.s.remember_local && self.local.error.is_none() {
            self.s.last_local = self.local.path.clone();
            self.s.save();
        }
        // Coming up out of a folder: select it.
        let oldp = Path::new(&old);
        if oldp.parent().is_some_and(|p| p.to_string_lossy() == self.local.path.as_str())
            && let Some(name) = oldp.file_name().map(|n| n.to_string_lossy().into_owned())
        {
            self.select_only(Side::Local, &name);
        }
    }

    pub(super) fn open_remote(&mut self, path: String) {
        if self.browse.is_none() {
            return;
        }
        self.remote.path = path;
        self.remote.selected.clear();
        self.remote.cursor = None;
        self.remote.entries.clear();
        self.remote.error = None;
        self.reload_remote();
    }

    pub(super) fn go_up(&mut self, side: Side) {
        match side {
            Side::Local => {
                if let Some(p) = Path::new(&self.local.path).parent() {
                    self.open_local(p.to_string_lossy().into_owned());
                }
            }
            Side::Remote => {
                if !self.remote.path.is_empty() && self.remote.path != "/" {
                    let p = sftp::parent(&self.remote.path);
                    self.open_remote(p);
                }
            }
        }
    }

    pub(super) fn pane(&mut self, side: Side) -> &mut Pane {
        match side {
            Side::Local => &mut self.local,
            Side::Remote => &mut self.remote,
        }
    }

    pub(super) fn pane_ref(&self, side: Side) -> &Pane {
        match side {
            Side::Local => &self.local,
            Side::Remote => &self.remote,
        }
    }

    /// Open a folder, or send a file to the other side.
    pub(super) fn activate(&mut self, side: Side, e: &Entry) {
        if e.is_dir || (e.is_link && side == Side::Remote) {
            match side {
                Side::Local => {
                    let p = Path::new(&self.local.path).join(&e.name);
                    self.open_local(p.to_string_lossy().into_owned());
                }
                Side::Remote => {
                    let p = sftp::join(&self.remote.path, &e.name);
                    self.open_remote(p);
                }
            }
        } else {
            self.transfer_items(side, vec![e.clone()]);
        }
    }

    pub(super) fn select_only(&mut self, side: Side, name: &str) {
        let p = self.pane(side);
        p.selected.clear();
        p.selected.insert(name.to_string());
        p.cursor = p.entries.iter().position(|e| e.name == name);
        p.anchor = p.cursor;
        p.scroll_to_cursor = true;
    }

    // ---------------------------------------------------------------- transfers

    /// Send items from `from` to the folder shown on the other side, asking first if any
    /// names are already there.
    pub(super) fn transfer_items(&mut self, from: Side, items: Vec<Entry>) {
        if items.is_empty() {
            return;
        }
        if self.transfer.is_none() {
            self.note = Some(("Connect to a server first.".into(), true));
            return;
        }
        let to_pane = self.pane_ref(from.other());
        let clashes: Vec<String> = items.iter().filter(|i| to_pane.entries.iter().any(|e| e.name == i.name)).map(|i| i.name.clone()).collect();
        let pending = Pending {
            dir: if from == Side::Local { Dir::Up } else { Dir::Down },
            items,
            from_dir: self.pane_ref(from).path.clone(),
            to_dir: to_pane.path.clone(),
            clashes,
        };
        match (pending.clashes.is_empty(), self.s.on_exists) {
            (true, _) | (false, OnExists::Replace) => self.enqueue(pending, true),
            (false, OnExists::Skip) => {
                let n = pending.clashes.len();
                self.note = Some((format!("Skipped {n} item{} that {} already there.", if n == 1 { "" } else { "s" }, if n == 1 { "was" } else { "were" }), false));
                self.enqueue(pending, false);
            }
            (false, OnExists::Ask) => self.prompt = Some(Prompt::Replace(pending)),
        }
    }

    pub(super) fn enqueue(&mut self, p: Pending, replace: bool) {
        let Some(t) = self.transfer.as_mut() else { return };
        for e in p.items {
            if !replace && p.clashes.contains(&e.name) {
                continue;
            }
            let (req, to, local_target) = match p.dir {
                Dir::Up => {
                    let src = Path::new(&p.from_dir).join(&e.name);
                    let dst = sftp::join(&p.to_dir, &e.name);
                    (Request::Put(src, dst.clone(), e.is_dir), dst, None)
                }
                Dir::Down => {
                    let src = sftp::join(&p.from_dir, &e.name);
                    let dst = Path::new(&p.to_dir).join(&e.name);
                    (Request::Get(src, dst.clone(), e.is_dir), dst.to_string_lossy().into_owned(), (!e.is_dir).then_some(dst))
                }
            };
            let id = t.send(req);
            self.queue.push(Job { id, dir: p.dir, name: e.name.clone(), to, to_dir: p.to_dir.clone(), is_dir: e.is_dir, size: e.size, local_target, state: JobState::Waiting });
        }
    }

    // ---------------------------------------------------------------- file actions

    pub(super) fn new_folder(&mut self, side: Side, name: &str) {
        let name = name.trim();
        if name.is_empty() || name.contains('/') {
            self.note = Some(("A folder name can't be empty or contain /.".into(), true));
            return;
        }
        match side {
            Side::Local => match std::fs::create_dir(Path::new(&self.local.path).join(name)) {
                Ok(()) => {
                    self.reload_local();
                    self.select_only(Side::Local, name);
                }
                Err(e) => self.note = Some((format!("Couldn't make the folder: {e}"), true)),
            },
            Side::Remote => {
                let p = sftp::join(&self.remote.path, name);
                if let Some(s) = self.browse.as_mut() {
                    let id = s.send(Request::Mkdir(p));
                    self.pending.insert(id, format!("Made the folder \"{name}\""));
                }
            }
        }
    }

    pub(super) fn rename(&mut self, side: Side, old: &str, new: &str) {
        let new = new.trim();
        if new.is_empty() || new.contains('/') || new == old {
            return;
        }
        if self.pane_ref(side).entries.iter().any(|e| e.name == new) {
            self.note = Some((format!("\"{new}\" already exists there."), true));
            return;
        }
        match side {
            Side::Local => {
                let dir = Path::new(&self.local.path);
                match std::fs::rename(dir.join(old), dir.join(new)) {
                    Ok(()) => {
                        self.reload_local();
                        self.select_only(Side::Local, new);
                    }
                    Err(e) => self.note = Some((format!("Couldn't rename: {e}"), true)),
                }
            }
            Side::Remote => {
                let (a, b) = (sftp::join(&self.remote.path, old), sftp::join(&self.remote.path, new));
                if let Some(s) = self.browse.as_mut() {
                    let id = s.send(Request::Rename(a, b));
                    self.pending.insert(id, format!("Renamed to \"{new}\""));
                }
            }
        }
    }

    pub(super) fn delete_remote(&mut self, items: Vec<Entry>) {
        let n = items.len();
        let dir = self.remote.path.clone();
        let Some(s) = self.browse.as_mut() else { return };
        for (i, e) in items.iter().enumerate() {
            let id = s.send(Request::Remove(sftp::join(&dir, &e.name), e.is_dir && !e.is_link));
            let what = match (i + 1 == n, n) {
                (true, 1) => format!("Deleted \"{}\"", e.name),
                (true, _) => format!("Deleted {n} items"),
                _ => format!("Deleting \"{}\"", e.name),
            };
            self.pending.insert(id, what);
        }
    }

    fn run_send_hook(&mut self, spec: &str) {
        for part in spec.split(';') {
            let Some((dir, names)) = part.split_once(':') else { continue };
            let side = if dir.trim() == "up" { Side::Local } else { Side::Remote };
            let wanted: Vec<&str> = names.split(',').map(str::trim).collect();
            let items: Vec<Entry> = self.pane_ref(side).entries.iter().filter(|e| wanted.contains(&e.name.as_str())).cloned().collect();
            self.transfer_items(side, items);
        }
    }

    /// Read the SSH keys in the background, once (until `reload_keys`).
    pub(super) fn want_keys(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.keys_rx
            && let Ok(keys) = rx.try_recv()
        {
            self.keys = keys;
            self.keys_rx = None;
        }
        if self.keys_asked {
            return;
        }
        self.keys_asked = true;
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(rivet_connections::keys::list());
            ctx.request_repaint();
        });
        self.keys_rx = Some(rx);
    }

    pub(super) fn reload_keys(&mut self, ctx: &egui::Context) {
        self.keys_asked = false;
        self.want_keys(ctx);
    }

    /// The saved connections that log in with this key.
    pub(super) fn key_users(&self, key: &rivet_connections::keys::SshKey) -> Vec<String> {
        let p = key.path.to_string_lossy();
        self.store.items.iter().filter(|c| c.key.as_deref().is_some_and(|k| expand(k).to_string_lossy() == p)).map(|c| c.name.clone()).collect()
    }

    pub(super) fn toggle_hidden(&mut self) {
        self.s.show_hidden = !self.s.show_hidden;
        self.s.save();
        self.reload_local();
        self.reload_remote();
    }

    // ---------------------------------------------------------------- keys

    pub(super) fn keys(&mut self, ctx: &egui::Context) {
        if self.form.is_some() || self.prompt.is_some() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let side = self.focus;
        let (pressed, mods) = ctx.input(|i| {
            let keys: Vec<Key> = [Key::ArrowUp, Key::ArrowDown, Key::Enter, Key::Home, Key::End]
                .into_iter()
                .filter(|k| i.key_pressed(*k))
                .collect();
            (keys, i.modifiers)
        });
        for k in pressed {
            let n = self.pane_ref(side).entries.len();
            match k {
                Key::ArrowDown | Key::ArrowUp | Key::Home | Key::End if n > 0 && !mods.alt => {
                    let p = self.pane(side);
                    let cur = p.cursor.unwrap_or(0);
                    let next = match k {
                        Key::ArrowDown if p.cursor.is_some() => (cur + 1).min(n - 1),
                        Key::ArrowUp => cur.saturating_sub(1),
                        Key::End => n - 1,
                        _ => 0,
                    };
                    if mods.shift {
                        let a = p.anchor.unwrap_or(cur);
                        p.selected = (a.min(next)..=a.max(next)).map(|i| p.entries[i].name.clone()).collect();
                    } else {
                        p.selected = [p.entries[next].name.clone()].into();
                        p.anchor = Some(next);
                    }
                    p.cursor = Some(next);
                    p.scroll_to_cursor = true;
                }
                Key::Enter => {
                    let sel = self.pane_ref(side).selected_entries();
                    if sel.len() == 1 {
                        self.activate(side, &sel[0]);
                    } else if !sel.is_empty() {
                        self.transfer_items(side, sel);
                    }
                }
                _ => {}
            }
        }
    }
}

pub(super) fn first_line(s: &str) -> &str {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or(s)
}

pub(super) fn expand(p: &str) -> std::path::PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => rivet_xdg::places::home().join(rest),
        None if p == "~" => rivet_xdg::places::home(),
        None => std::path::PathBuf::from(p),
    }
}

fn mtime(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn screenshot_mode(app: &FtpApp) -> bool {
    app.shot.is_some()
}

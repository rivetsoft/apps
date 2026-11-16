//! Everything the menus and keyboard shortcuts can do, the menu bar, and the menu button shown
//! when the menu bar is hidden.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Stroke, Vec2};
use rivet_look::theme::Mode;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Cmd {
    NewConnection,
    QuickConnect,
    Disconnect,
    Preferences,
    Quit,
    SelectAll,
    Rename,
    Delete,
    NewFolder,
    ToggleMenubar,
    ToggleSidebar,
    ToggleQueue,
    ToggleHidden,
    Refresh,
    Style(Mode),
    Up,
    LocalHome,
    ServerHome,
    SwitchPane,
    TypePath,
    Upload,
    Download,
    ClearFinished,
    Shortcuts,
    About,
}

const CTRL: Modifiers = Modifiers::COMMAND;
const CTRL_SHIFT: Modifiers = Modifiers { shift: true, ..Modifiers::COMMAND };
const ALT: Modifiers = Modifiers::ALT;

/// Shortcuts that work anywhere in the window (also while a text field has the keyboard).
pub(super) const GLOBAL: &[(Modifiers, Key, Cmd)] = &[
    (CTRL, Key::N, Cmd::NewConnection),
    (CTRL, Key::K, Cmd::QuickConnect),
    (CTRL_SHIFT, Key::D, Cmd::Disconnect),
    (CTRL, Key::Comma, Cmd::Preferences),
    (CTRL, Key::Q, Cmd::Quit),
    (CTRL, Key::M, Cmd::ToggleMenubar),
    (Modifiers::NONE, Key::F9, Cmd::ToggleSidebar),
    (Modifiers::NONE, Key::F8, Cmd::ToggleQueue),
    (CTRL, Key::H, Cmd::ToggleHidden),
    (Modifiers::NONE, Key::F5, Cmd::Refresh),
    (ALT, Key::Home, Cmd::LocalHome),
    (CTRL, Key::L, Cmd::TypePath),
    (CTRL, Key::U, Cmd::Upload),
    (CTRL, Key::D, Cmd::Download),
    (Modifiers::NONE, Key::F1, Cmd::Shortcuts),
    (CTRL_SHIFT, Key::N, Cmd::NewFolder),
];

/// Shortcuts for the file lists (only when no text field has the keyboard).
pub(super) const LIST: &[(Modifiers, Key, Cmd)] = &[
    (CTRL, Key::A, Cmd::SelectAll),
    (Modifiers::NONE, Key::F2, Cmd::Rename),
    (Modifiers::NONE, Key::Delete, Cmd::Delete),
    (Modifiers::NONE, Key::Backspace, Cmd::Up),
    (ALT, Key::ArrowUp, Cmd::Up),
    (Modifiers::NONE, Key::Tab, Cmd::SwitchPane),
];

/// The shortcut shown beside a command in the menus.
pub(super) fn shortcut_for(cmd: Cmd) -> String {
    let Some((m, k, _)) = GLOBAL.iter().chain(LIST).find(|(_, _, c)| *c == cmd) else { return String::new() };
    let mut s = String::new();
    if m.command {
        s.push_str("Ctrl+");
    }
    if m.shift {
        s.push_str("Shift+");
    }
    if m.alt {
        s.push_str("Alt+");
    }
    s.push_str(match k {
        Key::Comma => ",",
        Key::ArrowUp => "Up",
        other => other.name(),
    });
    s
}

impl FtpApp {
    pub(super) fn run(&mut self, cmd: Cmd, ctx: &egui::Context) {
        let side = self.focus;
        match cmd {
            Cmd::NewConnection => self.form = Some(Form::blank()),
            Cmd::QuickConnect => self.focus_quick = true,
            Cmd::Disconnect => {
                if let Some(c) = &self.conn {
                    let name = c.name.clone();
                    self.disconnect();
                    self.note = Some((format!("Disconnected from {name}."), false));
                }
            }
            Cmd::Preferences => self.prefs_open = !self.prefs_open,
            Cmd::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Cmd::SelectAll => {
                let p = self.pane(side);
                p.selected = p.entries.iter().map(|e| e.name.clone()).collect();
            }
            Cmd::Rename => {
                let sel = self.pane_ref(side).selected_entries();
                if sel.len() == 1 && (side == Side::Local || self.browse.is_some()) {
                    self.prompt = Some(Prompt::Rename(side, sel[0].name.clone(), sel[0].name.clone()));
                }
            }
            Cmd::Delete => {
                let sel = self.remote.selected_entries();
                if side == Side::Remote && !sel.is_empty() {
                    if self.s.confirm_delete {
                        self.prompt = Some(Prompt::Delete(sel));
                    } else {
                        self.delete_remote(sel);
                    }
                }
            }
            Cmd::NewFolder => {
                if side == Side::Local || self.browse.is_some() {
                    self.prompt = Some(Prompt::NewFolder(side, String::new()));
                }
            }
            Cmd::ToggleMenubar => {
                self.s.show_menubar = !self.s.show_menubar;
                self.s.save();
            }
            Cmd::ToggleSidebar => {
                self.s.show_sidebar = !self.s.show_sidebar;
                self.s.save();
            }
            Cmd::ToggleQueue => {
                self.s.show_queue = !self.s.show_queue;
                self.s.save();
            }
            Cmd::ToggleHidden => self.toggle_hidden(),
            Cmd::Refresh => self.reload(side),
            Cmd::Style(m) => {
                self.s.theme = m;
                self.s.save();
            }
            Cmd::Up => self.go_up(side),
            Cmd::LocalHome => {
                self.focus = Side::Local;
                self.open_local(rivet_xdg::places::home().to_string_lossy().into_owned());
            }
            Cmd::ServerHome => {
                if let Some(h) = self.remote_home.clone() {
                    self.focus = Side::Remote;
                    self.open_remote(h);
                }
            }
            Cmd::SwitchPane => self.focus = side.other(),
            Cmd::TypePath => {
                let p = self.pane(side);
                if !p.path.is_empty() {
                    p.editing = Some(p.path.clone());
                }
            }
            Cmd::Upload => {
                let items = self.local.selected_entries();
                self.transfer_items(Side::Local, items);
            }
            Cmd::Download => {
                let items = self.remote.selected_entries();
                self.transfer_items(Side::Remote, items);
            }
            Cmd::ClearFinished => self.queue.retain(|j| j.state == JobState::Waiting),
            Cmd::Shortcuts => self.keys_open = !self.keys_open,
            Cmd::About => {
                self.prefs_page = super::prefs::Page::About;
                self.prefs_open = true;
            }
        }
    }

    /// Shortcuts: the global ones always, the list ones when no text field or dialog has the
    /// keyboard.
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.form.is_some() || self.prompt.is_some() {
            return;
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        let mut hits = Vec::new();
        ctx.input_mut(|i| {
            for (m, k, c) in GLOBAL {
                if i.consume_shortcut(&KeyboardShortcut::new(*m, *k)) {
                    hits.push(*c);
                }
            }
            if !typing {
                for (m, k, c) in LIST {
                    if i.consume_shortcut(&KeyboardShortcut::new(*m, *k)) {
                        hits.push(*c);
                    }
                }
            }
        });
        for c in hits {
            self.run(c, ctx);
        }
    }

    /// A menu item that runs a command, with its shortcut beside it.
    fn item(&mut self, ui: &mut egui::Ui, label: &str, cmd: Cmd, enabled: bool) {
        let sc = shortcut_for(cmd);
        let mut b = egui::Button::new(label);
        if !sc.is_empty() {
            b = b.shortcut_text(sc);
        }
        if ui.add_enabled(enabled, b).clicked() {
            let ctx = ui.ctx().clone();
            self.run(cmd, &ctx);
            ui.close();
        }
    }

    /// A menu item with a check box in front (like Rivet's).
    fn toggle(&mut self, ui: &mut egui::Ui, on: bool, label: &str, cmd: Cmd) {
        let sc = shortcut_for(cmd);
        let mut b = egui::Button::new(format!("        {label}"));
        if !sc.is_empty() {
            b = b.shortcut_text(sc);
        }
        let resp = ui.add(b);
        let v = ui.visuals();
        let r = Rect::from_center_size(Pos2::new(resp.rect.left() + 13.0, resp.rect.center().y), Vec2::splat(13.0));
        if on {
            ui.painter().rect_filled(r, 3.0, v.selection.bg_fill);
            let c = r.center();
            let s = Stroke::new(1.6, v.selection.stroke.color);
            ui.painter().line_segment([c + Vec2::new(-3.5, 0.0), c + Vec2::new(-1.0, 2.8)], s);
            ui.painter().line_segment([c + Vec2::new(-1.0, 2.8), c + Vec2::new(3.8, -2.8)], s);
        } else {
            ui.painter().rect_stroke(r, 3.0, Stroke::new(1.2, v.weak_text_color()), egui::StrokeKind::Inside);
        }
        if resp.clicked() {
            let ctx = ui.ctx().clone();
            self.run(cmd, &ctx);
            ui.close();
        }
    }

    pub(super) fn menus(&mut self, ui: &mut egui::Ui) {
        let connected = self.browse.is_some() && !self.connecting;
        let sel_local = !self.local.selected.is_empty();
        let sel_remote = !self.remote.selected.is_empty();
        let sel_focus = !self.pane_ref(self.focus).selected.is_empty();
        let can_edit = self.focus == Side::Local || connected;
        ui.menu_button("File", |ui| {
            menu_style(ui);
            self.item(ui, "New Connection…", Cmd::NewConnection, true);
            self.item(ui, "Quick Connect", Cmd::QuickConnect, true);
            ui.menu_button("Connect To", |ui| {
                menu_style(ui);
                if self.store.items.is_empty() {
                    ui.label(egui::RichText::new("No saved connections").weak());
                }
                for c in self.store.items.clone() {
                    if ui.button(&c.name).clicked() {
                        let ctx = ui.ctx().clone();
                        self.connect(c, &ctx);
                        ui.close();
                    }
                }
            });
            self.item(ui, "Disconnect", Cmd::Disconnect, self.conn.is_some());
            ui.separator();
            self.item(ui, "Preferences", Cmd::Preferences, true);
            ui.separator();
            self.item(ui, "Quit", Cmd::Quit, true);
        });
        ui.menu_button("Edit", |ui| {
            menu_style(ui);
            self.item(ui, "Select All", Cmd::SelectAll, true);
            ui.separator();
            self.item(ui, "New Folder…", Cmd::NewFolder, can_edit);
            self.item(ui, "Rename…", Cmd::Rename, can_edit && self.pane_ref(self.focus).selected.len() == 1);
            self.item(ui, "Delete…", Cmd::Delete, self.focus == Side::Remote && connected && sel_focus);
        });
        ui.menu_button("View", |ui| {
            menu_style(ui);
            self.toggle(ui, self.s.show_menubar, "Menu Bar", Cmd::ToggleMenubar);
            self.toggle(ui, self.s.show_sidebar, "Connections", Cmd::ToggleSidebar);
            self.toggle(ui, self.s.show_queue, "Transfers", Cmd::ToggleQueue);
            self.toggle(ui, self.s.show_hidden, "Hidden Files", Cmd::ToggleHidden);
            ui.separator();
            ui.menu_button("Style", |ui| {
                menu_style(ui);
                for (m, label) in [(Mode::System, "Follow System"), (Mode::Light, "Light"), (Mode::Dark, "Dark")] {
                    let on = self.s.theme == m;
                    self.toggle(ui, on, label, Cmd::Style(m));
                }
            });
            ui.separator();
            self.item(ui, "Refresh", Cmd::Refresh, true);
        });
        ui.menu_button("Go", |ui| {
            menu_style(ui);
            self.item(ui, "Up", Cmd::Up, true);
            self.item(ui, "Home Folder", Cmd::LocalHome, true);
            self.item(ui, "Server Home", Cmd::ServerHome, connected);
            ui.separator();
            self.item(ui, "Type a Path…", Cmd::TypePath, true);
            self.item(ui, "Other Pane", Cmd::SwitchPane, true);
        });
        ui.menu_button("Transfer", |ui| {
            menu_style(ui);
            self.item(ui, "Upload Selected", Cmd::Upload, connected && sel_local);
            self.item(ui, "Download Selected", Cmd::Download, connected && sel_remote);
            ui.separator();
            let finished = self.queue.iter().any(|j| j.state != JobState::Waiting);
            self.item(ui, "Clear Finished", Cmd::ClearFinished, finished);
        });
        ui.menu_button("Help", |ui| {
            menu_style(ui);
            self.item(ui, "Keyboard Shortcuts", Cmd::Shortcuts, true);
            self.item(ui, "About Rivet FTP", Cmd::About, true);
        });
    }
}

pub(super) fn menu_style(ui: &mut egui::Ui) {
    ui.set_min_width(220.0);
    ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
    ui.spacing_mut().item_spacing.y = 3.0;
}

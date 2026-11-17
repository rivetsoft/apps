//! The editor's right-click menu: Go to Definition, Find References and the rest of the Go
//! commands, then cut / copy / paste, selecting matches, comments, find and search.
//!
//! Right-clicking outside the selection moves the cursor there first (so Go to Definition is
//! about the word clicked). The editing items are sent as the keys they stand for at the start
//! of the next frame, so they go through exactly the same paths as typing them (multiple
//! cursors and Vim keys included).

use eframe::egui::{self, Key, Modifiers};

use super::draw::{item, menu_style};
use super::navigate::NavCmd;
use super::*;
use crate::find::Query;

/// What was chosen in the menu.
#[derive(Clone, Debug, PartialEq)]
enum Act {
    Nav(NavCmd),
    Cut,
    Copy,
    Paste,
    Keys(Modifiers, Key),
    Find,
    SearchFolder(String),
    Commands,
}

impl EditApp {
    /// A right-click on the text: move the cursor to the click when it's outside the selection.
    pub(crate) fn context_click(&mut self, ctx: &egui::Context, id: egui::Id, ch: usize, sel: (usize, usize)) {
        let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
        if ch < a || ch > b || a == b {
            editing::set_primary(ctx, id, crate::ops::Sel::new(ch, ch));
            if let Some(t) = self.tabs.get_mut(self.active) {
                t.sels.clear();
            }
        }
        ctx.memory_mut(|m| m.request_focus(id));
    }

    /// The menu itself, on the editor's response.
    pub(crate) fn context_menu(&mut self, resp: &egui::Response, id: egui::Id, word: Option<String>, has_sel: bool) {
        let mut act: Option<Act> = None;
        let lsp = self.lsp_status.as_ref().is_some_and(|(_, _, trouble)| !trouble);
        let vim_normal = self.s.vim_keys && self.vim.mode != crate::vim::Mode::Insert;
        let on_word = word.is_some();
        resp.context_menu(|ui| {
            menu_style(ui);
            if item(ui, "Go to Definition", "F12", on_word) {
                act = Some(Act::Nav(NavCmd::Definition));
            }
            if item(ui, "Find References", "Shift+F12", on_word) {
                act = Some(Act::Nav(NavCmd::References));
            }
            if lsp && item(ui, "Show What It Is", if self.s.vim_keys { "K" } else { "" }, on_word) {
                act = Some(Act::Nav(NavCmd::Hover));
            }
            let can_back = self.nav_can(true);
            let can_fwd = self.nav_can(false);
            if can_back || can_fwd {
                if item(ui, "Back", "Alt+Left", can_back) {
                    act = Some(Act::Nav(NavCmd::Back));
                }
                if item(ui, "Forward", "Alt+Right", can_fwd) {
                    act = Some(Act::Nav(NavCmd::Forward));
                }
            }
            ui.separator();
            if item(ui, "Cut", "Ctrl+X", has_sel) {
                act = Some(Act::Cut);
            }
            if item(ui, "Copy", "Ctrl+C", has_sel) {
                act = Some(Act::Copy);
            }
            if item(ui, "Paste", "Ctrl+V", true) {
                act = Some(Act::Paste);
            }
            ui.separator();
            if item(ui, "Select All", "Ctrl+A", true) {
                act = Some(Act::Keys(Modifiers::COMMAND, Key::A));
            }
            // Ctrl+D is Vim's half page in Normal mode; the rest work in either.
            if !vim_normal && item(ui, "Add Next Match", "Ctrl+D", on_word || has_sel) {
                act = Some(Act::Keys(Modifiers::COMMAND, Key::D));
            }
            if item(ui, "Select Every Match", "Shift+Ctrl+L", on_word || has_sel) {
                act = Some(Act::Keys(Modifiers::COMMAND | Modifiers::SHIFT, Key::L));
            }
            if item(ui, "Comment Out", "Ctrl+/", true) {
                act = Some(Act::Keys(Modifiers::COMMAND, Key::Slash));
            }
            ui.separator();
            if item(ui, "Find", "Ctrl+F", true) {
                act = Some(Act::Find);
            }
            let label = match &word {
                Some(w) => format!("Search the Folder for “{}”", w.chars().take(24).collect::<String>()),
                None => "Search the Folder".to_string(),
            };
            if item(ui, &label, "Shift+Ctrl+F", self.root.is_some()) {
                act = Some(Act::SearchFolder(word.clone().unwrap_or_default()));
            }
            ui.separator();
            if item(ui, "Commands…", "Shift+Ctrl+P", true) {
                act = Some(Act::Commands);
            }
        });
        let Some(act) = act else { return };
        let ctx = resp.ctx.clone();
        ctx.memory_mut(|m| m.request_focus(id));
        match act {
            Act::Nav(c) => self.nav.pending = Some(c),
            Act::Cut => self.inject.push(egui::Event::Cut),
            Act::Copy => self.inject.push(egui::Event::Copy),
            Act::Paste => match self.clipboard_text() {
                Some(t) if !t.is_empty() => self.inject.push(egui::Event::Paste(t)),
                _ => self.note = Some(("Nothing to paste".into(), false)),
            },
            Act::Keys(modifiers, key) => self.inject.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }),
            Act::Find => self.find_pending = Some(false),
            Act::SearchFolder(w) => {
                if !w.is_empty() {
                    self.search.q = Query { text: w, case: true, word: true, regex: false };
                }
                self.open_search();
            }
            Act::Commands => self.open_quick(quick::QuickKind::Commands),
        }
        ctx.request_repaint();
    }

    /// The clipboard's text (never read in checks).
    fn clipboard_text(&self) -> Option<String> {
        if self.shot.is_some() {
            return std::env::var("RIVET_EDIT_CLIPBOARD").ok();
        }
        arboard::Clipboard::new().ok()?.get_text().ok()
    }
}

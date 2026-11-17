//! Going places in code: Go to Definition (F12, Ctrl+click, Vim's gd), Find References
//! (Shift+F12, Vim's gr), what the server says about the thing under the pointer (rest the
//! pointer on it, or Vim's K), and Back / Forward (Alt+Left / Alt+Right) through the places
//! jumped from.
//!
//! The language's server answers when there is one; without one, Go to Definition looks for
//! the name after `fn`, `def`, `function`, `class`, `struct`… in the folder's files.

use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;

use eframe::egui::{self, Key, Modifiers, Pos2, Vec2};
use rivet_look::theme::r_md;

use super::*;
use crate::find::{self, Query};
use crate::lsp::{self, Ask};

/// Something asked for, done when the editor next draws (it needs the cursor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavCmd {
    Definition,
    References,
    Hover,
    Back,
    Forward,
}

/// One place in the list of definitions or references.
#[derive(Clone, Debug)]
pub(crate) struct Spot {
    pub path: PathBuf,
    /// Where in the file (a character index).
    pub ch: usize,
    pub line: usize,
    /// The line, trimmed.
    pub preview: String,
}

/// What the server says about something, shown in a box over it.
pub(crate) struct HoverBox {
    seq: u64,
    /// The word it's about (character range in the open file).
    word: (usize, usize),
    /// Where to show it: just above the word.
    at: Pos2,
    text: Option<String>,
    /// From Vim's K: stays until the cursor moves.
    pinned: bool,
}

#[derive(Default)]
pub(crate) struct Nav {
    pub pending: Option<NavCmd>,
    seq: u64,
    /// Waiting for the server: (request, what was asked, the word, its positions in UTF-8).
    waiting: Option<(u64, Ask, String, bool)>,
    /// The search for a definition without a server.
    fallback: Option<(String, Receiver<Vec<Spot>>)>,
    back: Vec<(PathBuf, usize)>,
    fwd: Vec<(PathBuf, usize)>,
    /// The list shown in the quick panel, and its title.
    pub list: Vec<Spot>,
    pub list_title: String,
    hover: Option<HoverBox>,
    /// The pointer resting on a word: (its range, since when).
    rest: Option<((usize, usize), f64)>,
}

fn editor_id(path: &Option<PathBuf>) -> egui::Id {
    egui::Id::unique(("edit-body", path.clone()))
}

/// `rel` for a path in the workspace, else its name.
fn short(root: &Option<PathBuf>, p: &Path) -> String {
    match root.as_ref().and_then(|r| p.strip_prefix(r).ok()) {
        Some(rel) => rel.to_string_lossy().into_owned(),
        None => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
    }
}

impl EditApp {
    /// F12, Shift+F12, Alt+Left and Alt+Right.
    pub(crate) fn nav_keys(&mut self, ctx: &egui::Context) {
        if self.quick.is_some() || self.prompt.is_some() || self.prefs_open {
            return;
        }
        // Shift+F12 first: a plain F12 check would take it too.
        let (refs, def, back, fwd) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::SHIFT, Key::F12),
                i.consume_key(Modifiers::NONE, Key::F12),
                i.consume_key(Modifiers::ALT, Key::ArrowLeft),
                i.consume_key(Modifiers::ALT, Key::ArrowRight),
            )
        });
        let cmd = if refs {
            Some(NavCmd::References)
        } else if def {
            Some(NavCmd::Definition)
        } else if back {
            Some(NavCmd::Back)
        } else if fwd {
            Some(NavCmd::Forward)
        } else {
            None
        };
        if cmd.is_some() {
            self.nav.pending = cmd;
        }
    }

    /// The open file's path and cursor (a character index).
    fn cursor_now(&self, ctx: &egui::Context) -> Option<(Option<PathBuf>, usize)> {
        let t = self.tabs.get(self.active)?;
        let st = egui::TextEdit::load_state(ctx, editor_id(&t.path))?;
        Some((t.path.clone(), st.cursor.char_range().map(|r| r.primary.index.0).unwrap_or(0)))
    }

    /// Each frame: do what was asked, and take in a search's answer.
    pub(crate) fn nav_tick(&mut self, ctx: &egui::Context) {
        if let Some((word, rx)) = &self.nav.fallback {
            match rx.try_recv() {
                Ok(spots) => {
                    let word = word.clone();
                    self.nav.fallback = None;
                    self.show_spots(spots, &format!("Definitions of {word}"), &format!("No definition found for {word}"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(60)),
                Err(_) => self.nav.fallback = None,
            }
        }
        match self.nav.pending {
            Some(NavCmd::Back) => {
                self.nav.pending = None;
                self.go_history(ctx, true);
            }
            Some(NavCmd::Forward) => {
                self.nav.pending = None;
                self.go_history(ctx, false);
            }
            Some(c @ (NavCmd::Definition | NavCmd::References)) => {
                self.nav.pending = None;
                self.ask_at_cursor(ctx, if c == NavCmd::Definition { Ask::Definition } else { Ask::References });
            }
            _ => {}
        }
    }

    fn ask_at_cursor(&mut self, ctx: &egui::Context, what: Ask) {
        let Some((path, cur)) = self.cursor_now(ctx) else { return };
        let Some(t) = self.tabs.get(self.active) else { return };
        let chars: Vec<char> = t.text.chars().collect();
        let Some((a, b)) = crate::ops::word_at(&chars, cur) else {
            self.note = Some(("Put the cursor on a name first".into(), false));
            return;
        };
        let word: String = chars[a..b].iter().collect();
        let lang = if self.s.lsp { t.lang_now() } else { None };
        let text = t.text.clone();
        if let (Some(p), Some(l)) = (&path, lang) {
            let root = self.lsp_root(p);
            self.nav.seq += 1;
            let seq = self.nav.seq;
            if self.lsp.ask(lsp::Doc { path: p, root: &root, lang: l, text: &text }, cur, what, seq) {
                let utf8 = self.lsp.utf8(l, &root);
                self.nav.waiting = Some((seq, what, word.clone(), utf8));
                self.note = Some((format!("Looking for {}{word}…", if what == Ask::References { "uses of " } else { "" }), false));
                return;
            }
        }
        match what {
            Ask::Definition => self.find_definition_by_name(&word, &text, path),
            _ => {
                // Without a server: every whole-word use, as a folder search.
                self.search.q = Query { text: word.clone(), case: true, word: true, regex: false };
                self.open_search();
                self.note = Some((format!("No language server: searching the folder for {word}"), false));
            }
        }
    }

    /// The server's answer for a definition or references.
    pub(crate) fn nav_places(&mut self, seq: u64, places: Vec<lsp::Place>) {
        let Some((s, what, word, utf8)) = self.nav.waiting.clone() else { return };
        if s != seq {
            return;
        }
        self.nav.waiting = None;
        if places.is_empty() && what == Ask::Definition {
            // The server knows of none: look by name instead.
            let (text, path) = self.tabs.get(self.active).map(|t| (t.text.clone(), t.path.clone())).unwrap_or_default();
            self.find_definition_by_name(&word, &text, path);
            return;
        }
        // Each place as a character index, with its line.
        let mut texts: HashMap<PathBuf, String> = HashMap::new();
        let mut spots = Vec::new();
        for pl in places {
            if !texts.contains_key(&pl.path) {
                let text = match self.tabs.iter().find(|t| t.path.as_deref() == Some(pl.path.as_path())) {
                    Some(t) => t.text.clone(),
                    None => crate::workspace::read_text(&pl.path).unwrap_or_default(),
                };
                texts.insert(pl.path.clone(), text);
            }
            let text = &texts[&pl.path];
            let ch = lsp::from_pos(text, pl.start.0, pl.start.1, utf8);
            let preview = text.lines().nth(pl.start.0 as usize).unwrap_or("").trim().chars().take(140).collect();
            spots.push(Spot { path: pl.path, ch, line: pl.start.0 as usize + 1, preview });
        }
        let title = if what == Ask::References { format!("Uses of {word}") } else { format!("Definitions of {word}") };
        self.show_spots(spots, &title, &format!("Nothing found for {word}"));
    }

    /// One place: go there. Several: list them.
    fn show_spots(&mut self, spots: Vec<Spot>, title: &str, none: &str) {
        match spots.len() {
            0 => self.note = Some((none.to_string(), true)),
            1 => {
                self.note = None;
                let s = &spots[0];
                let (p, ch) = (s.path.clone(), s.ch);
                self.jump_to(p, ch);
            }
            n => {
                self.note = None;
                self.nav.list = spots;
                self.nav.list_title = format!("{title} ({n})");
                self.open_quick(quick::QuickKind::Places);
            }
        }
    }

    /// Go to a place, remembering where we were (for Back).
    pub(crate) fn jump_to(&mut self, path: PathBuf, ch: usize) {
        if let Some(t) = self.tabs.get(self.active)
            && let Some(p) = t.path.clone()
        {
            let here = self.vim_or_text_cursor(&p);
            self.nav.back.push((p, here));
            if self.nav.back.len() > 100 {
                self.nav.back.remove(0);
            }
        }
        self.nav.fwd.clear();
        self.go_place(path, ch);
    }

    fn go_place(&mut self, path: PathBuf, ch: usize) {
        self.open_file(&path);
        self.select_after = Some((ch, ch));
        self.reveal = true;
    }

    /// Where the cursor is in a tab (remembered by its text box).
    fn vim_or_text_cursor(&self, p: &Path) -> usize {
        self.last_cursor.get(p).copied().unwrap_or(0)
    }

    fn go_history(&mut self, ctx: &egui::Context, back: bool) {
        let Some((p, ch)) = (if back { self.nav.back.pop() } else { self.nav.fwd.pop() }) else {
            self.note = Some((if back { "Nowhere to go back to" } else { "Nowhere to go forward to" }.into(), false));
            return;
        };
        if let Some((Some(here), cur)) = self.cursor_now(ctx) {
            if back {
                self.nav.fwd.push((here, cur));
            } else {
                self.nav.back.push((here, cur));
            }
        }
        self.go_place(p, ch);
    }

    /// Without a server: where the name follows `fn`, `def`, `class`, `struct`… (or starts a
    /// C function), in the open file first, then across the folder.
    fn find_definition_by_name(&mut self, word: &str, text: &str, path: Option<PathBuf>) {
        let w = regex::escape(word);
        let pattern = format!(r"(?m)(?:\b(?:fn|def|function|func|class|struct|enum|union|trait|type|interface|impl|mod|module|const|static|let|var|val|macro_rules!|#define)\s+(?:[A-Za-z_]\w*\s*[*&]*\s+)?{w}\b|^[A-Za-z_][\w \t*&:<>,]*[\s*&]{w}\s*\([^;\n]*$)");
        let Ok(re) = regex::Regex::new(&pattern) else { return };
        // The open file first (it may have unsaved changes).
        let mut here = Vec::new();
        if let Some(p) = &path {
            for m in re.find_iter(text).take(50) {
                let line = text[..m.start()].matches('\n').count();
                let at = m.as_str().rfind(word).map(|k| m.start() + k).unwrap_or(m.start());
                let ch = text[..at].chars().count();
                here.push(Spot { path: p.clone(), ch, line: line + 1, preview: text.lines().nth(line).unwrap_or("").trim().chars().take(140).collect() });
            }
        }
        if !here.is_empty() || self.root.is_none() {
            self.show_spots(here, &format!("Definitions of {word}"), &format!("No definition found for {word}"));
            return;
        }
        let (Some(root), files) = (self.root.clone(), self.files.clone()) else { return };
        let q = Query { text: pattern, case: true, word: false, regex: true };
        let word_s = word.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let stop = AtomicBool::new(false);
            let spots = match find::search_files(&root, &files, &q, 50, &stop) {
                Ok((hits, _)) => hits
                    .into_iter()
                    .flat_map(|f| {
                        let word = word_s.clone();
                        f.hits.into_iter().map(move |h| {
                            let k = h.hit.rfind(&word).map(|b| h.hit[..b].chars().count()).unwrap_or(0);
                            Spot { path: f.path.clone(), ch: h.at.0 + k, line: h.line, preview: format!("{}{}{}", h.pre, h.hit, h.post).trim().trim_start_matches('…').to_string() }
                        })
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
            let _ = tx.send(spots);
        });
        self.note = Some((format!("Looking for {word} in the folder…"), false));
        self.nav.fallback = Some((word.to_string(), rx));
    }

    /// The quick panel's list of places: rows for it.
    pub(crate) fn spot_rows(&mut self, ctx: &egui::Context, which: &[usize]) -> Vec<rivet_look::quick::QuickRow> {
        let root = self.root.clone();
        let list: Vec<Spot> = which.iter().filter_map(|i| self.nav.list.get(*i).cloned()).collect();
        list.iter()
            .map(|s| {
                let rel = short(&root, &s.path);
                let name = s.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let icon = self.file_icon(ctx, &name, 24.0);
                rivet_look::quick::QuickRow::Item { icon, name: format!("{rel}:{}", s.line), sub: s.preview.clone() }
            })
            .collect()
    }

    // ---------------------------------------------------------------- hover

    /// After the text box: the pointer resting on a word asks the server about it; Vim's K asks
    /// about the word at the cursor. `probe` is the character under the pointer and where its
    /// word starts on screen.
    pub(crate) fn hover_tick(&mut self, ctx: &egui::Context, probe: Option<(usize, Pos2)>, caret: Option<(usize, Pos2)>, typed: bool) {
        let now = ctx.input(|i| i.time);
        let Some(t) = self.tabs.get(self.active) else { return };
        let (Some(path), Some(lang)) = (t.path.clone(), if self.s.lsp { t.lang_now() } else { None }) else {
            self.nav.hover = None;
            return;
        };
        let root = self.lsp_root(&path);
        // K (Vim) or the Show Hover command: about the word at the cursor, until it moves.
        if self.nav.pending == Some(NavCmd::Hover) {
            self.nav.pending = None;
            if let Some((cur, at)) = caret {
                let chars: Vec<char> = t.text.chars().collect();
                if let Some(word) = crate::ops::word_at(&chars, cur) {
                    self.ask_hover(word, at, true, &path, &root, lang);
                } else {
                    self.note = Some(("Put the cursor on a name first".into(), false));
                }
            }
            return;
        }
        if let Some(h) = &self.nav.hover {
            let gone = if h.pinned { typed || caret.is_none_or(|(c, _)| c < h.word.0 || c > h.word.1) } else { typed || probe.is_none_or(|(c, _)| c < h.word.0 || c >= h.word.1) };
            if gone {
                self.nav.hover = None;
            } else {
                return;
            }
        }
        let Some((ch, pos)) = probe else {
            self.nav.rest = None;
            return;
        };
        if !self.lsp.ready(lang, &root) {
            return;
        }
        match self.nav.rest {
            Some((w, since)) if ch >= w.0 && ch < w.1 => {
                if now - since >= 0.55 {
                    self.nav.rest = None;
                    self.ask_hover(w, pos, false, &path, &root, lang);
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(120));
                }
            }
            _ => {
                let chars: Vec<char> = t.text.chars().collect();
                self.nav.rest = crate::ops::word_at(&chars, ch).filter(|(a, b)| ch >= *a && ch < *b).map(|w| (w, now));
                if self.nav.rest.is_some() {
                    ctx.request_repaint_after(std::time::Duration::from_millis(600));
                }
            }
        }
    }

    fn ask_hover(&mut self, word: (usize, usize), at: Pos2, pinned: bool, path: &Path, root: &Path, lang: crate::syntax::Lang) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let text = t.text.clone();
        self.nav.seq += 1;
        let seq = self.nav.seq;
        if self.lsp.ask(lsp::Doc { path, root, lang, text: &text }, word.0, Ask::Hover, seq) {
            self.nav.hover = Some(HoverBox { seq, word, at, text: None, pinned });
        } else if pinned {
            self.note = Some(("No language server to ask".into(), false));
        }
    }

    pub(crate) fn hover_answer(&mut self, seq: u64, text: String) {
        if let Some(h) = self.nav.hover.as_mut()
            && h.seq == seq
        {
            if text.trim().is_empty() {
                if h.pinned {
                    self.note = Some(("Nothing to show here".into(), false));
                }
                self.nav.hover = None;
            } else {
                h.text = Some(text);
            }
        }
    }

    /// The box over the word.
    pub(crate) fn hover_box(&self, ctx: &egui::Context, code_font: &egui::FontId) {
        let Some(h) = &self.nav.hover else { return };
        let Some(text) = &h.text else { return };
        let v = ctx.global_style().visuals.clone();
        let lines: Vec<&str> = text.lines().take(24).collect();
        egui::Area::new(egui::Id::unique("edit-hover"))
            .order(egui::Order::Tooltip)
            .fixed_pos(h.at + Vec2::new(-6.0, -6.0))
            .pivot(egui::Align2::LEFT_BOTTOM)
            .constrain(true)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.set_max_width(560.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    // The signature (before the first blank line) in the code font, the rest as text.
                    let mut in_code = true;
                    for l in lines {
                        if l.trim().is_empty() {
                            if in_code {
                                ui.add_space(4.0);
                                ui.separator();
                            }
                            in_code = false;
                            ui.add_space(2.0);
                            continue;
                        }
                        if in_code {
                            ui.add(egui::Label::new(egui::RichText::new(l).font(egui::FontId::monospace((code_font.size - 1.0).max(10.0))).color(v.strong_text_color())).wrap());
                        } else {
                            ui.add(egui::Label::new(egui::RichText::new(l.trim_start_matches('#').trim()).color(v.text_color())).wrap());
                        }
                    }
                });
            });
    }
}

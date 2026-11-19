//! The editor's own keys (VS Code's), multiple cursors, the find bar (Ctrl+F, Ctrl+H) and the
//! workspace search in the sidebar (Shift+Ctrl+F).
//!
//! The text box knows one cursor. With more than one, typing, deleting, moving and copying are
//! done here for all of them (the events are taken before the text box sees them), the main one
//! is handed to the text box, and the others are drawn on top.

use std::sync::Arc;

use eframe::egui::{self, Align2, Color32, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{r_sm, status};
use rivet_look::{fonts, palette};

use super::*;
use crate::find::{self, FileHits, Query};
use crate::ops::{self, Move, Sel};

/// The find bar over the editor.
#[derive(Default)]
pub(crate) struct FindBar {
    pub q: Query,
    pub replace: String,
    pub show_replace: bool,
    pub focus: bool,
    /// Matches for (text, query), as character ranges.
    key: u64,
    pub matches: Vec<(usize, usize)>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Side {
    #[default]
    Files,
    Search,
}

/// A finished workspace search: the files with matches and whether it stopped at the limit.
type SearchResult = Result<(Vec<FileHits>, bool), String>;

/// The workspace search.
#[derive(Default)]
pub(crate) struct Search {
    pub q: Query,
    pub replace: String,
    pub show_replace: bool,
    pub focus: bool,
    edited_at: f64,
    /// The query last searched for, and the answer coming.
    ran: Option<Query>,
    rx: Option<std::sync::mpsc::Receiver<SearchResult>>,
    /// Set to stop the running search (a newer query replaces it).
    stop: Arc<std::sync::atomic::AtomicBool>,
    pub results: Vec<FileHits>,
    pub cut: bool,
    pub error: Option<String>,
    pub folded: HashSet<String>,
    /// Only files in this folder (inside the workspace; empty for all of it).
    pub scope: String,
}

impl Search {
    /// Search again (the scope or files changed).
    pub(crate) fn rerun(&mut self) {
        self.ran = None;
    }
}

/// Where the editor's text was laid out last frame (for Alt+click).
pub(crate) struct LastLayout {
    pub galley: Arc<egui::Galley>,
    pub pos: Pos2,
    pub rect: Rect,
}

fn hash_of(text: &str, q: &Query) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    q.hash(&mut h);
    h.finish()
}

impl std::hash::Hash for Query {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        (&self.text, self.case, self.word, self.regex).hash(h);
    }
}

/// The text box's selection as a [`Sel`].
pub(crate) fn primary(ctx: &egui::Context, id: egui::Id) -> Sel {
    egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range()).map(|r| Sel::new(r.secondary.index.0, r.primary.index.0)).unwrap_or_default()
}

/// Every cursor in a tab, the main one last.
pub(crate) fn sels_now(ctx: &egui::Context, id: egui::Id, t: &Tab) -> Vec<Sel> {
    if t.sels.len() >= 2 { t.sels.clone() } else { vec![primary(ctx, id)] }
}

/// Hand a selection to the text box (and the keyboard to it).
pub(crate) fn set_primary(ctx: &egui::Context, id: egui::Id, s: Sel) {
    let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(s.anchor), egui::text::CCursor::new(s.head))));
    st.store(ctx, id);
    // Asking again would reset what the text box keeps (Esc and Tab) for a frame.
    if !ctx.memory(|m| m.has_focus(id)) {
        ctx.memory_mut(|m| m.request_focus(id));
    }
}

/// A small toggle with a short label ("Aa", "ab", ".*").
fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str, tip: &str) -> bool {
    let v = ui.visuals().clone();
    let b = egui::Button::new(egui::RichText::new(label).font(egui::FontId::monospace(12.0)).color(if *on { v.selection.stroke.color } else { v.weak_text_color() }))
        .fill(if *on { v.selection.bg_fill } else { Color32::TRANSPARENT })
        .min_size(Vec2::new(26.0, 22.0))
        .corner_radius(r_sm());
    let r = ui.add(b).on_hover_text(tip);
    if r.clicked() {
        *on = !*on;
    }
    r.clicked()
}

/// A text field in the bars.
fn field(ui: &mut egui::Ui, text: &mut String, hint: &str, id: egui::Id, width: f32, bad: bool) -> egui::Response {
    let v = ui.visuals().clone();
    let focused = ui.memory(|m| m.has_focus(id));
    let stroke = if bad { Stroke::new(1.3, status(ui.ctx()).danger) } else if focused { Stroke::new(1.3, v.selection.bg_fill) } else { Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color) };
    egui::Frame::NONE
        .fill(palette::mix(v.extreme_bg_color, v.text_color(), 0.05))
        .stroke(stroke)
        .corner_radius(r_sm())
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| ui.add(egui::TextEdit::singleline(text).id(id).frame(egui::Frame::NONE).margin(egui::Margin::ZERO).font(fonts::ui_font(13.0)).min_size(Vec2::new(0.0, 18.0)).align(Align2::LEFT_CENTER).hint_text(hint).desired_width(width)))
        .inner
}

/// A small arrow that shows or hides the replace row.
fn chevron(ui: &mut egui::Ui, open: bool, tip: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(20.0, 26.0), Sense::click());
    let v = ui.visuals().clone();
    if resp.hovered() {
        ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
    }
    let c = r.center();
    let pts = if open { vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)] } else { vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, 4.0), c + Vec2::new(3.0, 0.0)] };
    ui.painter().add(egui::Shape::convex_polygon(pts, v.weak_text_color(), Stroke::NONE));
    resp.on_hover_text(tip).clicked()
}

impl EditApp {
    // ---------------------------------------------------------------- keys

    /// The editor's keys while it has the keyboard; returns the selections to show (several
    /// when there are multiple cursors).
    pub(crate) fn editor_keys(&mut self, ctx: &egui::Context, id: egui::Id) {
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let ind = t.indent;
        let focused = ctx.memory(|m| m.has_focus(id));
        // A click without Alt in the text: back to one cursor.
        let press = ctx.input(|i| if i.pointer.primary_pressed() { i.pointer.interact_pos().map(|p| (p, i.modifiers.alt)) } else { None });
        if let (Some((p, alt)), Some(l)) = (press, &self.last_layout)
            && l.rect.contains(p)
        {
            if alt {
                // Alt+click: another cursor there (the text box moves its own to the click).
                let c = l.galley.cursor_from_pos(p - l.pos).index.0;
                let mut sels = if t.sels.len() >= 2 { t.sels.clone() } else { vec![primary(ctx, id)] };
                if !sels.iter().any(|s| s.is_empty() && s.head == c) {
                    sels.push(Sel::caret(c));
                }
                t.sels = sels;
                self.alt_click = true;
            } else {
                t.sels.clear();
            }
        }
        if !focused {
            return;
        }
        let cmd = Modifiers::COMMAND;
        let shift = Modifiers { shift: true, ..Modifiers::COMMAND };
        let alt = Modifiers::ALT;
        let shift_alt = Modifiers { shift: true, alt: true, ..Default::default() };
        let cmd_alt = Modifiers { alt: true, ..Modifiers::COMMAND };
        let k = |m: Modifiers, key: Key| KeyboardShortcut::new(m, key);
        #[derive(PartialEq)]
        enum Op {
            Next,
            All,
            Above,
            Below,
            MoveUp,
            MoveDown,
            CopyUp,
            CopyDown,
            DeleteLines,
            Comment,
            Indent,
            Outdent,
            LineBelow,
            LineAbove,
            SelectLine,
        }
        let op = ctx.input_mut(|i| {
            // The more specific ones (more modifiers) first.
            [
                (k(shift, Key::L), Op::All),
                (k(shift, Key::K), Op::DeleteLines),
                (k(shift, Key::Enter), Op::LineAbove),
                (k(cmd_alt, Key::ArrowUp), Op::Above),
                (k(cmd_alt, Key::ArrowDown), Op::Below),
                (k(shift_alt, Key::ArrowUp), Op::CopyUp),
                (k(shift_alt, Key::ArrowDown), Op::CopyDown),
                (k(alt, Key::ArrowUp), Op::MoveUp),
                (k(alt, Key::ArrowDown), Op::MoveDown),
                (k(cmd, Key::D), Op::Next),
                (k(cmd, Key::Slash), Op::Comment),
                (k(cmd, Key::CloseBracket), Op::Indent),
                (k(cmd, Key::OpenBracket), Op::Outdent),
                (k(cmd, Key::Enter), Op::LineBelow),
                (k(cmd, Key::L), Op::SelectLine),
            ]
            .into_iter()
            .find(|(s, _)| i.consume_shortcut(s))
            .map(|(_, o)| o)
        });
        let is_command = op.is_some();
        let multi = t.sels.len() >= 2;
        let mut sels = if multi { t.sels.clone() } else { vec![primary(ctx, id)] };
        let mut chars: Vec<char> = t.text.chars().collect();
        let before_len = chars.len();
        let mut changed = false;
        let mut moved = false;
        if let Some(op) = op {
            let comment = t.comment();
            match op {
                Op::Next => ops::add_next_occurrence(&chars, &mut sels),
                Op::All => ops::select_all_occurrences(&chars, &mut sels),
                Op::Above => ops::add_cursor_vertical(&chars, &mut sels, false),
                Op::Below => ops::add_cursor_vertical(&chars, &mut sels, true),
                Op::SelectLine => ops::select_lines(&chars, &mut sels),
                Op::MoveUp => ops::move_lines(&mut chars, &mut sels, false),
                Op::MoveDown => ops::move_lines(&mut chars, &mut sels, true),
                Op::CopyUp => ops::duplicate_lines(&mut chars, &mut sels, false),
                Op::CopyDown => ops::duplicate_lines(&mut chars, &mut sels, true),
                Op::DeleteLines => ops::delete_lines(&mut chars, &mut sels),
                Op::Comment => {
                    if let Some(tok) = comment {
                        ops::toggle_comment(&mut chars, &mut sels, tok);
                    }
                }
                Op::Indent => ops::indent_lines(&mut chars, &mut sels, ind, false),
                Op::Outdent => ops::indent_lines(&mut chars, &mut sels, ind, true),
                Op::LineBelow => ops::insert_line(&mut chars, &mut sels, false),
                Op::LineAbove => ops::insert_line(&mut chars, &mut sels, true),
            }
            changed = chars.len() != before_len || chars.iter().copied().ne(t.text.chars());
            moved = true;
        } else if multi {
            // Several cursors: typing, deleting, moving and copying for all of them.
            let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());
            let mut keep_events = Vec::new();
            for ev in events {
                let mut used = true;
                match &ev {
                    egui::Event::Text(s) => ops::insert(&mut chars, &mut sels, s),
                    egui::Event::Paste(s) => {
                        // One line per cursor when the counts match (as VS Code does).
                        let lines: Vec<&str> = s.split('\n').collect();
                        if lines.len() == sels.len() {
                            let mut order: Vec<usize> = (0..sels.len()).collect();
                            order.sort_by_key(|i| sels[*i].lo());
                            let mut per = vec![String::new(); sels.len()];
                            for (k, i) in order.iter().enumerate() {
                                per[*i] = lines[k].to_string();
                            }
                            let pairs: Vec<(Sel, String)> = sels.iter().copied().zip(per).collect();
                            ops::insert_each(&mut chars, &mut sels, |_, s| pairs.iter().find(|(x, _)| *x == s).map(|(_, t)| t.clone()).unwrap_or_default());
                        } else {
                            ops::insert(&mut chars, &mut sels, s);
                        }
                    }
                    egui::Event::Copy => ctx.copy_text(ops::selected_text(&chars, &sels)),
                    egui::Event::Cut => {
                        ctx.copy_text(ops::selected_text(&chars, &sels));
                        ops::insert(&mut chars, &mut sels, "");
                    }
                    egui::Event::Key { key, pressed: true, modifiers, .. } => {
                        let ext = modifiers.shift;
                        let word = modifiers.command;
                        match key {
                            Key::Backspace => ops::delete(&mut chars, &mut sels, false),
                            Key::Delete => ops::delete(&mut chars, &mut sels, true),
                            Key::Enter => ops::newline(&mut chars, &mut sels),
                            Key::Tab => ops::insert(&mut chars, &mut sels, &ind.unit()),
                            Key::ArrowLeft => ops::move_sels(&chars, &mut sels, if word { Move::WordLeft } else { Move::Left }, ext),
                            Key::ArrowRight => ops::move_sels(&chars, &mut sels, if word { Move::WordRight } else { Move::Right }, ext),
                            Key::ArrowUp => ops::move_sels(&chars, &mut sels, Move::Up, ext),
                            Key::ArrowDown => ops::move_sels(&chars, &mut sels, Move::Down, ext),
                            Key::Home => ops::move_sels(&chars, &mut sels, Move::Home, ext),
                            Key::End => ops::move_sels(&chars, &mut sels, Move::End, ext),
                            Key::Escape => {
                                let m = *sels.last().unwrap();
                                sels = vec![m];
                            }
                            // Undo, select all and the like: back to one cursor, then the text box.
                            Key::Z | Key::Y | Key::A if modifiers.command => {
                                let m = *sels.last().unwrap();
                                sels = vec![m];
                                used = false;
                            }
                            _ => used = false,
                        }
                    }
                    egui::Event::Key { key: Key::Backspace | Key::Delete | Key::Enter | Key::Tab | Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End, .. } => {}
                    _ => used = false,
                }
                if !used {
                    keep_events.push(ev);
                }
            }
            ctx.input_mut(|i| i.events = keep_events);
            changed = chars.iter().copied().ne(t.text.chars());
            moved = true;
        }
        if !moved {
            return;
        }
        ops::normalize(&mut sels);
        if changed {
            if is_command {
                t.hist.command();
            }
            t.text = chars.into_iter().collect();
        }
        let main = *sels.last().unwrap_or(&Sel::default());
        t.sels = if sels.len() >= 2 { sels } else { Vec::new() };
        set_primary(ctx, id, main);
        self.reveal = true;
    }

    /// Ctrl+Z, Shift+Ctrl+Z and Ctrl+Y in the editor (when `keys`), and Undo and Redo from
    /// the menus. Anything changed since the last frame is recorded first.
    pub(crate) fn undo_keys(&mut self, ctx: &egui::Context, id: egui::Id, keys: bool) {
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let now = sels_now(ctx, id, t);
        t.hist.observe(&t.text, &now);
        let mut asked: Vec<bool> = self.undo_asked.take().into_iter().collect();
        if keys && ctx.memory(|m| m.has_focus(id)) {
            let k = |m, key| egui::KeyboardShortcut::new(m, key);
            ctx.input_mut(|i| {
                // Shift+Ctrl+Z first: Ctrl+Z would match it too.
                loop {
                    if i.consume_shortcut(&k(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)) || i.consume_shortcut(&k(Modifiers::COMMAND, Key::Y)) {
                        asked.push(true);
                    } else if i.consume_shortcut(&k(Modifiers::COMMAND, Key::Z)) {
                        asked.push(false);
                    } else {
                        break;
                    }
                }
            });
        }
        let mut last = None;
        for redo in asked {
            let sels = if redo { t.hist.redo(&mut t.text) } else { t.hist.undo(&mut t.text) };
            if sels.is_some() {
                last = sels;
            }
        }
        let Some(sels) = last else { return };
        let main = sels.last().copied().unwrap_or_default();
        t.sels = if sels.len() >= 2 { sels } else { Vec::new() };
        t.preview = false;
        set_primary(ctx, id, main);
        self.complete = None;
        self.reveal = true;
    }

    // ---------------------------------------------------------------- find bar

    pub(crate) fn open_find(&mut self, ctx: &egui::Context, replace: bool) {
        let mut bar = self.find.take().unwrap_or_default();
        // The selected text (one line) as the query.
        if let Some(t) = self.tabs.get(self.active) {
            let id = egui::Id::unique(("edit-body", t.path.clone()));
            let s = primary(ctx, id);
            if !s.is_empty() {
                let sel: String = t.text.chars().skip(s.lo()).take(s.hi() - s.lo()).collect();
                if !sel.contains('\n') {
                    bar.q.text = sel;
                }
            }
        }
        bar.show_replace = replace || bar.show_replace && self.find.is_some();
        bar.show_replace = replace;
        bar.focus = true;
        self.find = Some(bar);
    }

    /// Select the next (or previous) match after the cursor.
    pub(crate) fn find_step(&mut self, ctx: &egui::Context, back: bool) {
        let Some(bar) = &self.find else { return };
        let Some(t) = self.tabs.get(self.active) else { return };
        if bar.matches.is_empty() {
            return;
        }
        let id = egui::Id::unique(("edit-body", t.path.clone()));
        let cur = primary(ctx, id);
        let m = &bar.matches;
        let pick = if back {
            m.iter().rev().find(|(a, _)| *a < cur.lo()).or(m.last())
        } else {
            m.iter().find(|(a, _)| *a > cur.lo() || (*a == cur.lo() && cur.is_empty())).or(m.first())
        };
        if let Some(&(a, b)) = pick {
            self.select_after = Some((a, b));
        }
    }

    fn update_matches(&mut self) {
        let Some(bar) = self.find.as_mut() else { return };
        let Some(t) = self.tabs.get(self.active) else {
            bar.matches.clear();
            return;
        };
        let key = hash_of(&t.text, &bar.q);
        if key == bar.key {
            return;
        }
        bar.key = key;
        match find::find(&t.text, &bar.q) {
            Ok(r) => {
                bar.matches = find::char_ranges(&t.text, &r);
                bar.error = None;
            }
            Err(e) => {
                bar.matches.clear();
                bar.error = Some(e);
            }
        }
    }

    /// The find bar, over the top of the editor.
    pub(crate) fn find_bar(&mut self, ui: &mut egui::Ui) {
        self.update_matches();
        let ctx = ui.ctx().clone();
        let Some(mut bar) = self.find.take() else { return };
        let v = ui.visuals().clone();
        let weak = v.weak_text_color();
        let cur = self.tabs.get(self.active).map(|t| primary(&ctx, egui::Id::unique(("edit-body", t.path.clone())))).unwrap_or_default();
        let at = bar.matches.iter().position(|(a, b)| *a == cur.lo() && *b == cur.hi());
        let (mut next, mut prev, mut close, mut rep_one, mut rep_all) = (false, false, false, false, false);
        let find_id = egui::Id::unique("find-field");
        let rep_id = egui::Id::unique("replace-field");
        egui::Frame::NONE.fill(palette::mix(v.extreme_bg_color, v.text_color(), 0.03)).inner_margin(egui::Margin { left: 12, right: 12, top: 6, bottom: 6 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                // Show / hide replace.
                if chevron(ui, bar.show_replace, "Replace (Ctrl+H)") {
                    bar.show_replace = !bar.show_replace;
                }
                let r = field(ui, &mut bar.q.text, "Find", find_id, 220.0, bar.error.is_some());
                if bar.focus {
                    r.request_focus();
                    let mut st = egui::TextEdit::load_state(&ctx, find_id).unwrap_or_default();
                    let n = bar.q.text.chars().count();
                    st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(n))));
                    st.store(&ctx, find_id);
                    bar.focus = false;
                }
                if r.has_focus() || r.lost_focus() {
                    let (enter, shift_enter, esc) = ctx.input_mut(|i| {
                        (
                            i.consume_key(Modifiers::NONE, Key::Enter),
                            i.consume_key(Modifiers::SHIFT, Key::Enter),
                            i.consume_key(Modifiers::NONE, Key::Escape),
                        )
                    });
                    if enter {
                        next = true;
                        r.request_focus();
                    }
                    if shift_enter {
                        prev = true;
                        r.request_focus();
                    }
                    close |= esc;
                }
                toggle(ui, &mut bar.q.case, "Aa", "Match case");
                toggle(ui, &mut bar.q.word, "ab", "Whole words");
                toggle(ui, &mut bar.q.regex, ".*", "Regular expression");
                ui.add_space(6.0);
                let label = match (&bar.error, bar.matches.len(), at) {
                    (Some(_), _, _) => "Not valid".to_string(),
                    (None, 0, _) if bar.q.text.is_empty() => String::new(),
                    (None, 0, _) => "No results".to_string(),
                    (None, n, Some(i)) => format!("{} of {n}", i + 1),
                    (None, n, None) => format!("{n} found"),
                };
                let col = if bar.error.is_some() || (bar.matches.is_empty() && !bar.q.text.is_empty()) { status(&ctx).danger } else { weak };
                let lr = ui.label(egui::RichText::new(label).font(fonts::ui_font(12.0)).color(col));
                if let Some(e) = &bar.error {
                    lr.on_hover_text(e);
                }
                let nav = |ui: &mut egui::Ui, up: bool, tip: &str| -> bool {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(24.0, 24.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                    }
                    let c = r.center();
                    let d = if up { -1.0 } else { 1.0 };
                    let s = Stroke::new(1.5, v.text_color());
                    ui.painter().line_segment([c + Vec2::new(-4.5, -2.0 * d), c + Vec2::new(0.0, 2.5 * d)], s);
                    ui.painter().line_segment([c + Vec2::new(0.0, 2.5 * d), c + Vec2::new(4.5, -2.0 * d)], s);
                    resp.on_hover_text(tip).clicked()
                };
                prev |= nav(ui, true, "Previous (Shift+Enter, Shift+F3)");
                next |= nav(ui, false, "Next (Enter, F3)");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("×").size(15.0).color(weak)).frame_when_inactive(false).min_size(Vec2::splat(24.0))).on_hover_text("Close (Esc)").clicked() {
                        close = true;
                    }
                });
            });
            if bar.show_replace {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add_space(24.0);
                    let r = field(ui, &mut bar.replace, "Replace", rep_id, 220.0, false);
                    if r.has_focus() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                        rep_one = true;
                        r.request_focus();
                    }
                    if r.has_focus() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                        close = true;
                    }
                    if ui.add_enabled(!bar.matches.is_empty(), egui::Button::new("Replace").corner_radius(r_sm())).on_hover_text("Replace this one and go to the next (Enter)").clicked() {
                        rep_one = true;
                    }
                    if ui.add_enabled(!bar.matches.is_empty(), egui::Button::new("Replace All").corner_radius(r_sm())).clicked() {
                        rep_all = true;
                    }
                });
            }
        });
        let q = bar.q.clone();
        let with = bar.replace.clone();
        let matches = bar.matches.clone();
        if !close {
            self.find = Some(bar);
        } else if let Some(t) = self.tabs.get(self.active) {
            // Back to the text, the found text still selected.
            ctx.memory_mut(|m| m.request_focus(egui::Id::unique(("edit-body", t.path.clone()))));
        }
        if rep_all && let Some(t) = self.tabs.get_mut(self.active) {
            if let Ok((text, n)) = find::replace_all(&t.text, &q, &with) {
                t.text = text;
                t.sels.clear();
                self.note = Some((format!("Replaced {n}"), false));
            }
        } else if rep_one {
            // The match that's selected, replaced; then the next one.
            if let (Some(i), Some(t)) = (at, self.tabs.get_mut(self.active)) {
                let (a, b) = matches[i];
                let ba = t.text.char_indices().nth(a).map(|(x, _)| x).unwrap_or(t.text.len());
                let bb = t.text.char_indices().nth(b).map(|(x, _)| x).unwrap_or(t.text.len());
                let rep = find::replacement(&t.text, ba..bb, &q, &with);
                let n = rep.chars().count();
                t.text.replace_range(ba..bb, &rep);
                let id = egui::Id::unique(("edit-body", t.path.clone()));
                set_primary(&ctx, id, Sel::caret(a + n));
                self.update_matches();
            }
            self.find_step(&ctx, false);
        } else if next || prev {
            self.find_step(&ctx, prev);
        }
    }

    /// Outlines around the matches, and the extra cursors and selections, over the text.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_marks(painter: &egui::Painter, galley: &egui::Galley, pos: Pos2, clip: Rect, sels: &[Sel], matches: &[(usize, usize)], current: Sel, v: &egui::Visuals, accent: Color32) {
        let at = |c: usize| galley.pos_from_cursor(egui::text::CCursor::new(c)).translate(pos.to_vec2());
        let span = |a: usize, b: usize| -> Vec<Rect> {
            let (ra, rb) = (at(a), at(b));
            if (ra.center().y - rb.center().y).abs() < 1.0 {
                vec![Rect::from_min_max(ra.left_top(), rb.left_bottom().max(ra.left_bottom() + Vec2::new(2.0, 0.0)))]
            } else {
                let right = pos.x + galley.rect.width();
                vec![Rect::from_min_max(ra.left_top(), Pos2::new(right, ra.bottom())), Rect::from_min_max(Pos2::new(pos.x, ra.bottom()), Pos2::new(right, rb.top())), Rect::from_min_max(Pos2::new(pos.x, rb.top()), rb.left_bottom())]
            }
        };
        // Matches: only those on screen (they're in order).
        for &(a, b) in matches.iter().take(find::MAX_MATCHES) {
            let r = at(a);
            if r.bottom() < clip.top() {
                continue;
            }
            if r.top() > clip.bottom() {
                break;
            }
            let on = current.lo() == a && current.hi() == b;
            for rr in span(a, b) {
                painter.rect_filled(rr, 2.0, accent.gamma_multiply(if on { 0.35 } else { 0.16 }));
                painter.rect_stroke(rr, 2.0, Stroke::new(1.0, accent.gamma_multiply(if on { 0.9 } else { 0.45 })), egui::StrokeKind::Outside);
            }
        }
        // The other cursors (the text box draws the main one).
        if sels.len() >= 2 {
            for s in &sels[..sels.len() - 1] {
                if !s.is_empty() {
                    for rr in span(s.lo(), s.hi()) {
                        painter.rect_filled(rr, 0.0, v.selection.bg_fill.gamma_multiply(0.6));
                    }
                }
                let c = at(s.head);
                if clip.intersects(c) {
                    painter.line_segment([c.center_top(), c.center_bottom()], Stroke::new(2.0, v.text_cursor.stroke.color));
                }
            }
        }
    }

    // ---------------------------------------------------------------- workspace search

    pub(crate) fn open_search(&mut self) {
        self.side = Side::Search;
        self.s.show_sidebar = true;
        self.search.focus = true;
    }

    /// Start a search when the query has settled, and take in a finished one.
    fn search_tick(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if let Some(rx) = &self.search.rx {
            match rx.try_recv() {
                Ok(Ok((r, cut))) => {
                    self.search.results = r;
                    self.search.cut = cut;
                    self.search.error = None;
                    self.search.rx = None;
                }
                Ok(Err(e)) => {
                    self.search.results.clear();
                    self.search.error = Some(e);
                    self.search.rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(80)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.search.rx = None,
            }
        }
        let changed = self.search.ran.as_ref() != Some(&self.search.q);
        if changed {
            // A newer query: stop the search still running rather than waiting for it.
            if self.search.rx.take().is_some() {
                self.search.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if now - self.search.edited_at < 0.25 {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
                return;
            }
            self.search.ran = Some(self.search.q.clone());
            if self.search.q.text.is_empty() {
                self.search.results.clear();
                self.search.error = None;
                return;
            }
            let scope = self.search.scope.clone();
            let files: Vec<String> = if scope.is_empty() { self.files.clone() } else { self.files.iter().filter(|f| f.starts_with(&format!("{scope}/"))).cloned().collect() };
            let (Some(root), q) = (self.root.clone(), self.search.q.clone()) else { return };
            let (tx, rx) = std::sync::mpsc::channel();
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.search.stop = stop.clone();
            std::thread::spawn(move || {
                let r = find::search_files(&root, &files, &q, 2000, &stop);
                if !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = tx.send(r);
                }
            });
            self.search.rx = Some(rx);
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
    }

    /// The Search view in the sidebar.
    pub(crate) fn search_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.search_tick(&ctx);
        let v = ui.visuals().clone();
        let weak = v.weak_text_color();
        let before = self.search.q.clone();
        let sid = egui::Id::unique("search-field");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if chevron(ui, self.search.show_replace, "Replace in files") {
                self.search.show_replace = !self.search.show_replace;
            }
            let w = (ui.available_width() - 20.0).max(80.0);
            let r = field(ui, &mut self.search.q.text, "Search", sid, w, self.search.error.is_some());
            if self.search.focus {
                r.request_focus();
                self.search.focus = false;
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.add_space(21.0);
            toggle(ui, &mut self.search.q.case, "Aa", "Match case");
            toggle(ui, &mut self.search.q.word, "ab", "Whole words");
            toggle(ui, &mut self.search.q.regex, ".*", "Regular expression");
        });
        if self.search.q != before {
            self.search.edited_at = ctx.input(|i| i.time);
        }
        let mut replace_all = false;
        if self.search.show_replace {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.add_space(21.0);
                let w = (ui.available_width() - 90.0).max(60.0);
                field(ui, &mut self.search.replace, "Replace", egui::Id::unique("search-replace"), w, false);
                replace_all = ui.add_enabled(!self.search.results.is_empty(), egui::Button::new("Replace All").corner_radius(r_sm())).clicked();
            });
        }
        // Searching one folder: say which, with a way back to all of them.
        if !self.search.scope.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(21.0);
                ui.label(egui::RichText::new(format!("In {}/", self.search.scope)).small().color(weak));
                if ui.small_button("×").on_hover_text("Search every folder").clicked() {
                    self.search.scope.clear();
                    self.search.rerun();
                }
            });
        }
        ui.add_space(4.0);
        let n: usize = self.search.results.iter().map(|f| f.hits.len()).sum();
        let summary = if let Some(e) = &self.search.error {
            e.clone()
        } else if self.search.rx.is_some() {
            "Searching…".into()
        } else if self.search.q.text.is_empty() {
            if self.root.is_some() { "Type to search every file in the folder".into() } else { "Open a folder to search it".into() }
        } else if n == 0 {
            "No results".into()
        } else {
            format!("{n}{} result{} in {} file{}", if self.search.cut { "+" } else { "" }, if n == 1 { "" } else { "s" }, self.search.results.len(), if self.search.results.len() == 1 { "" } else { "s" })
        };
        ui.label(egui::RichText::new(summary).small().color(if self.search.error.is_some() { status(&ctx).danger } else { weak }));
        ui.add_space(4.0);
        let mut open: Option<(PathBuf, (usize, usize))> = None;
        let mut fold: Option<String> = None;
        // One row per file and per match; only the rows in view are drawn.
        let mut rows: Vec<(usize, Option<usize>)> = Vec::new();
        for (k, f) in self.search.results.iter().enumerate() {
            rows.push((k, None));
            if !self.search.folded.contains(&f.rel) {
                rows.extend((0..f.hits.len()).map(|j| (k, Some(j))));
            }
        }
        let f12 = fonts::ui_font(12.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().id_salt("search-results").auto_shrink([false, false]).show_rows(ui, 23.0, rows.len(), |ui, range| {
            for &(k, j) in &rows[range] {
                let icon = if j.is_none() {
                    let name = self.search.results[k].rel.rsplit('/').next().unwrap_or_default().to_string();
                    self.file_icon(&ctx, &name, 14.0)
                } else {
                    None
                };
                let f = &self.search.results[k];
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 23.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                }
                let Some(j) = j else {
                    let folded = self.search.folded.contains(&f.rel);
                    let c = Pos2::new(r.left() + 8.0, r.center().y);
                    let pts = if !folded { vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)] } else { vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, 4.0), c + Vec2::new(3.0, 0.0)] };
                    ui.painter().add(egui::Shape::convex_polygon(pts, weak, Stroke::NONE));
                    let name = f.rel.rsplit('/').next().unwrap_or(&f.rel);
                    if let Some(t) = &icon {
                        rivet_look::theme::paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(r.left() + 24.0, r.center().y), Vec2::splat(14.0)), Color32::WHITE);
                    }
                    let g = ui.painter().layout_no_wrap(name.to_string(), fonts::ui_font(12.5), v.text_color());
                    let gw = g.size().x;
                    ui.painter().galley(Pos2::new(r.left() + 36.0, r.center().y - g.size().y / 2.0), g, v.text_color());
                    let dir = f.rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
                    ui.painter().with_clip_rect(r.shrink2(Vec2::new(30.0, 0.0))).text(Pos2::new(r.left() + 42.0 + gw, r.center().y), Align2::LEFT_CENTER, dir, fonts::ui_font(11.5), weak);
                    ui.painter().text(Pos2::new(r.right() - 6.0, r.center().y), Align2::RIGHT_CENTER, f.hits.len().to_string(), fonts::ui_font(11.0), weak);
                    if resp.clicked() {
                        fold = Some(f.rel.clone());
                    }
                    continue;
                };
                // The line around the match (cut short when searching), the match marked.
                let h = &f.hits[j];
                let mut job = egui::text::LayoutJob::default();
                job.append(&h.pre, 0.0, egui::TextFormat { font_id: f12.clone(), color: weak, ..Default::default() });
                job.append(&h.hit, 0.0, egui::TextFormat { font_id: f12.clone(), color: v.strong_text_color(), background: v.selection.bg_fill.gamma_multiply(0.45), ..Default::default() });
                job.append(&h.post, 0.0, egui::TextFormat { font_id: f12.clone(), color: weak, ..Default::default() });
                job.wrap = egui::text::TextWrapping::truncate_at_width(r.width() - 30.0);
                let g = ui.painter().layout_job(job);
                ui.painter().galley(Pos2::new(r.left() + 26.0, r.center().y - g.size().y / 2.0), g, v.text_color());
                if resp.on_hover_text(format!("{}:{}", f.rel, h.line)).clicked() {
                    open = Some((f.path.clone(), h.at));
                }
            }
        });
        if let Some(rel) = fold
            && !self.search.folded.remove(&rel)
        {
            self.search.folded.insert(rel);
        }
        if let Some((p, range)) = open {
            self.preview_file(&p);
            self.select_after = Some(range);
        }
        if replace_all {
            let files = self.search.results.len();
            self.prompt = Some(Prompt::ReplaceInFiles { matches: n, files });
        }
    }

    /// Replace the workspace search's matches in every file (open ones in their tabs, which
    /// then need saving; the others on disk).
    pub(crate) fn replace_in_files(&mut self) {
        let q = self.search.q.clone();
        let with = self.search.replace.clone();
        let mut done = 0;
        let mut failed = Vec::new();
        for f in self.search.results.clone() {
            if let Some(t) = self.tabs.iter_mut().find(|t| t.path.as_deref() == Some(f.path.as_path())) {
                if let Ok((text, n)) = find::replace_all(&t.text, &q, &with) {
                    t.text = text;
                    done += n;
                }
                continue;
            }
            match crate::workspace::read_text(&f.path).map_err(|e| e.to_string()).and_then(|text| find::replace_all(&text, &q, &with)) {
                Ok((text, n)) => match crate::workspace::write_text(&f.path, &text) {
                    Ok(()) => done += n,
                    Err(_) => failed.push(f.rel.clone()),
                },
                Err(_) => failed.push(f.rel.clone()),
            }
        }
        self.note = Some(if failed.is_empty() { (format!("Replaced {done}"), false) } else { (format!("Replaced {done}; couldn't change {}", failed.join(", ")), true) });
        self.search.ran = None;
    }
}


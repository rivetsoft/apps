//! Suggestions as you type (from the language's server, else the words of the file), and the
//! server's errors and warnings: wavy lines under them, the message under the pointer, a count in
//! the status bar.

use eframe::egui::{self, Align2, Color32, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::a11y::{Access, Role};
use rivet_look::theme::{r_md, r_sm, r_xs, status};
use rivet_look::fonts;

use super::*;
use crate::lsp::{self, Item};
use crate::syntax::{Lang, Pick};

/// The suggestion list.
pub(crate) struct Complete {
    /// Where the word being completed starts (a character index).
    anchor: usize,
    items: Vec<Item>,
    /// Indices into `items` that match what's typed, best first.
    shown: Vec<usize>,
    pub sel: usize,
    /// The request the server's answer will carry.
    seq: u64,
    /// The server hasn't answered yet.
    waiting: bool,
    /// What was typed when `shown` was made (a change puts the choice back at the top).
    prefix: String,
    /// Under the cursor, on screen.
    at: Pos2,
    /// The server counts in UTF-8 bytes (else UTF-16).
    utf8: bool,
}

/// The server's state for the status bar: (words, tooltip, trouble).
pub(crate) type LspStatus = (String, String, bool);

/// A problem's place in the text (character indices), how bad (1 error … 4 hint) and what.
pub(crate) type DiagRange = (usize, usize, u8, String);

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Tab {
    /// The language it's shown as, when one with a server is known.
    pub fn lang_now(&self) -> Option<Lang> {
        match self.lang {
            Pick::Plain => None,
            Pick::Lang(l) => Some(l),
            Pick::Auto => crate::syntax::detect(&self.name(), &self.text),
        }
    }
}

/// Character index and byte offset where each line starts.
fn line_starts(text: &str) -> Vec<(usize, usize)> {
    let mut out = vec![(0, 0)];
    for (ci, (bi, c)) in text.char_indices().enumerate() {
        if c == '\n' {
            out.push((ci + 1, bi + 1));
        }
    }
    out
}

fn at(text: &str, starts: &[(usize, usize)], (line, col): (u32, u32), utf8: bool) -> usize {
    let Some(&(ci, bi)) = starts.get(line as usize) else { return text.chars().count() };
    let mut units = 0u32;
    let mut n = 0;
    for c in text[bi..].chars() {
        if units >= col || c == '\n' {
            break;
        }
        units += if utf8 { c.len_utf8() } else { c.len_utf16() } as u32;
        n += 1;
    }
    ci + n
}

impl EditApp {
    /// The folder a file's server works in: the workspace when the file's in it.
    pub(crate) fn lsp_root(&self, p: &Path) -> PathBuf {
        match &self.root {
            Some(r) if p.starts_with(r) => r.clone(),
            _ => p.parent().map(Path::to_path_buf).unwrap_or_default(),
        }
    }

    /// Each frame: take in what the servers said, keep the open file's server up to date, and
    /// note its state for the status bar.
    pub(crate) fn lsp_tick(&mut self) {
        for ev in self.lsp.poll() {
            match ev {
                lsp::Event::Completion { seq, items } => {
                    if let Some(c) = self.complete.as_mut()
                        && c.seq == seq
                    {
                        c.waiting = false;
                        if !items.is_empty() {
                            c.items = items;
                            c.prefix = "\u{0}".into();
                        }
                    }
                }
                lsp::Event::Places { seq, places } => self.nav_places(seq, places),
                lsp::Event::Hover { seq, text } => self.hover_answer(seq, text),
            }
        }
        let open: HashSet<PathBuf> = if self.s.lsp { self.tabs.iter().filter_map(|t| t.path.clone()).collect() } else { HashSet::new() };
        self.lsp.close_others(&open);
        self.lsp_status = None;
        if !self.s.lsp {
            return;
        }
        let Some(t) = self.tabs.get(self.active) else { return };
        let (Some(path), Some(lang)) = (t.path.clone(), t.lang_now()) else { return };
        let root = self.lsp_root(&path);
        let t = &self.tabs[self.active];
        self.lsp_status = Some(match self.lsp.touch(lsp::Doc { path: &path, root: &root, lang, text: &t.text, generation: t.text.generation() }) {
            lsp::State::Ready(p) => (p.to_string(), format!("{p} gives the suggestions and errors"), false),
            lsp::State::Starting(p) => (format!("{p}…"), format!("{p} is starting"), false),
            lsp::State::Failed(why) => ("No language server".into(), format!("{why}. Suggestions come from the file's own words."), true),
            lsp::State::None(l) => ("No language server".into(), format!("For suggestions and errors in {}, install {}. Until then, suggestions come from the file's own words.", l.name(), lsp::install_hint(l)), true),
        });
    }

    /// The open file's problems as character ranges.
    pub(crate) fn diag_ranges(&mut self) -> Vec<DiagRange> {
        if !self.s.lsp {
            return Vec::new();
        }
        let Some(t) = self.tabs.get(self.active) else { return Vec::new() };
        let (Some(path), Some(lang)) = (t.path.clone(), t.lang_now()) else { return Vec::new() };
        let Some(ds) = self.lsp.diags.get(&path).filter(|d| !d.is_empty()).cloned() else { return Vec::new() };
        let root = self.lsp_root(&path);
        let utf8 = self.lsp.utf8(lang, &root);
        let Some(t) = self.tabs.get(self.active) else { return Vec::new() };
        let starts = line_starts(&t.text);
        ds.iter().map(|d| (at(&t.text, &starts, d.start, utf8), at(&t.text, &starts, d.end, utf8), d.severity, d.message.clone())).collect()
    }

    /// Before the text box: the list's keys (Up, Down, Enter, Tab, Esc) while it's open, and
    /// Ctrl+Space to open it. Returns whether Ctrl+Space was pressed.
    pub(crate) fn complete_keys(&mut self, ctx: &egui::Context, id: egui::Id) -> bool {
        if !ctx.memory(|m| m.has_focus(id)) {
            self.complete = None;
            return false;
        }
        let ctrl_space = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Space));
        let Some(c) = self.complete.as_mut() else { return ctrl_space };
        let n = c.shown.len();
        if n == 0 {
            return ctrl_space;
        }
        let none = Modifiers::NONE;
        let (up, down, pgup, pgdn, enter, tab, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(none, Key::ArrowUp),
                i.consume_key(none, Key::ArrowDown),
                i.consume_key(none, Key::PageUp),
                i.consume_key(none, Key::PageDown),
                i.consume_key(none, Key::Enter),
                i.consume_key(none, Key::Tab),
                i.consume_key(none, Key::Escape),
            )
        });
        if up {
            c.sel = (c.sel + n - 1) % n;
        }
        if down {
            c.sel = (c.sel + 1) % n;
        }
        if pgup {
            c.sel = c.sel.saturating_sub(8);
        }
        if pgdn {
            c.sel = (c.sel + 8).min(n - 1);
        }
        let sel = c.sel;
        if esc {
            self.complete = None;
        } else if enter || tab {
            self.accept(ctx, id, sel);
        }
        ctrl_space
    }

    /// Put in the chosen suggestion, in place of the word typed so far.
    fn accept(&mut self, ctx: &egui::Context, id: egui::Id, k: usize) {
        let Some(c) = self.complete.take() else { return };
        let Some(item) = c.shown.get(k).and_then(|i| c.items.get(*i)).cloned() else { return };
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let Some(r) = st.cursor.char_range() else { return };
        let cur = r.primary.index.0;
        let start = item.range.map(|(s, _)| lsp::from_pos(&t.text, s.0, s.1, c.utf8)).filter(|s| *s <= cur && cur - *s <= 256).unwrap_or(c.anchor).min(cur);
        let byte = |ch: usize| t.text.char_indices().nth(ch).map(|(i, _)| i).unwrap_or(t.text.len());
        let (bs, be) = (byte(start), byte(cur));
        t.text.replace_range(bs..be, &item.insert);
        let to = start + item.insert.chars().count();
        st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(to))));
        st.store(ctx, id);
        t.preview = false;
    }

    /// After the text box: open, narrow or close the list for what was just typed.
    pub(crate) fn complete_after(&mut self, caret: Option<(usize, Pos2)>, typed: Option<char>, ctrl_space: bool) {
        let Some((cur, pos)) = caret else {
            self.complete = None;
            return;
        };
        let Some(t) = self.tabs.get(self.active) else { return };
        if t.sels.len() >= 2 {
            self.complete = None;
            return;
        }
        let lang = if self.s.lsp { t.lang_now() } else { None };
        let path = t.path.clone();
        let root = path.as_deref().map(|p| self.lsp_root(p));
        let triggers = match (lang, &root) {
            (Some(l), Some(r)) => self.lsp.triggers(l, r),
            _ => Vec::new(),
        };
        let trigger = typed.filter(|c| !is_word(*c) && triggers.contains(c));
        let wordy = typed.is_some_and(is_word);
        let open_now = ctrl_space || ((wordy || trigger.is_some()) && self.s.autocomplete);
        if self.complete.is_none() && !open_now {
            return;
        }
        let Some(t) = self.tabs.get(self.active) else { return };
        let before: Vec<char> = t.text.chars().take(cur).collect();
        let ws = cur - before.iter().rev().take_while(|c| is_word(**c)).count();
        let prefix: String = before[ws..].iter().collect();
        if open_now {
            let fresh = ctrl_space || self.complete.as_ref().is_none_or(|c| c.anchor != ws);
            if fresh {
                if prefix.is_empty() && trigger.is_none() && !ctrl_space {
                    self.complete = None;
                    return;
                }
                self.complete_seq += 1;
                let seq = self.complete_seq;
                let items = lsp::word_items(&t.text, &prefix, cur);
                let (text, generation) = (t.text.clone(), t.text.generation());
                let (asked, utf8) = match (lang, &path, &root) {
                    (Some(l), Some(p), Some(r)) => (self.lsp.complete(lsp::Doc { path: p, root: r, lang: l, text: &text, generation }, cur, trigger, seq), self.lsp.utf8(l, r)),
                    _ => (false, false),
                };
                self.complete = Some(Complete { anchor: ws, items, shown: Vec::new(), sel: 0, seq, waiting: asked, prefix: "\u{0}".into(), at: pos, utf8 });
            }
        } else if let Some(c) = &self.complete
            && (typed.is_some() || cur < c.anchor || ws != c.anchor)
        {
            // Something else typed, or the cursor left the word.
            self.complete = None;
            return;
        }
        let Some(c) = self.complete.as_mut() else { return };
        c.at = pos;
        if c.prefix != prefix {
            let mut ranked: Vec<(i32, usize)> = c.items.iter().enumerate().filter_map(|(i, it)| lsp::score(&it.filter, &prefix).map(|s| (s, i))).collect();
            let items = &c.items;
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| items[a.1].sort.cmp(&items[b.1].sort)));
            c.shown = ranked.into_iter().map(|(_, i)| i).take(200).collect();
            // Only what's typed already: nothing to add.
            if c.shown.len() == 1 && items[c.shown[0]].insert == prefix {
                c.shown.clear();
            }
            c.sel = 0;
            c.prefix = prefix;
        }
        if c.shown.is_empty() && !c.waiting {
            self.complete = None;
        }
    }

    /// The list, under the cursor.
    pub(crate) fn complete_popup(&mut self, ctx: &egui::Context, id: egui::Id, code_font: &egui::FontId, colors: &rivet_look::highlight::Colors) {
        let Some(c) = &self.complete else { return };
        if c.shown.is_empty() {
            return;
        }
        let v = ctx.global_style().visuals.clone();
        let weak = v.weak_text_color();
        const ROWS: usize = 10;
        let row_h = 24.0;
        let first = c.sel.saturating_sub(ROWS - 1).min(c.shown.len().saturating_sub(ROWS));
        let rows: Vec<(usize, Item)> = c.shown.iter().enumerate().skip(first).take(ROWS).map(|(k, i)| (k, c.items[*i].clone())).collect();
        let sel = c.sel;
        let prefix_len = c.prefix.chars().count();
        let prefix_lower = c.prefix.to_lowercase();
        let total = c.shown.len();
        let font = egui::FontId::monospace((code_font.size - 1.0).max(10.0));
        let mut pick = None;
        egui::Area::new(egui::Id::unique("edit-complete")).order(egui::Order::Foreground).fixed_pos(c.at + Vec2::new(-28.0, 4.0)).constrain(true).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).corner_radius(r_md()).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
                ui.set_width(440.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                for (k, it) in &rows {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
                    let on = *k == sel;
                    let resp = resp.role(Role::ListBoxOption, it.label.as_str()).selected(on).described(it.detail.as_str());
                    if on {
                        ui.painter().rect_filled(r, r_sm(), v.selection.bg_fill.gamma_multiply(0.28));
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                    }
                    // What kind it is: a little lettered badge.
                    let (letter, col) = kind_badge(it.kind, colors, weak);
                    let b = Rect::from_center_size(Pos2::new(r.left() + 14.0, r.center().y), Vec2::splat(16.0));
                    ui.painter().rect_filled(b, r_xs(), col.gamma_multiply(0.22));
                    ui.painter().text(b.center(), Align2::CENTER_CENTER, letter, fonts::ui_font(10.5), col);
                    // The name, the part typed so far brighter.
                    let mut job = egui::text::LayoutJob::default();
                    let strong = if on { v.strong_text_color() } else { v.text_color() };
                    let split = it.label.char_indices().nth(prefix_len).map(|(i, _)| i).unwrap_or(it.label.len());
                    let matched = it.label[..split].to_lowercase() == prefix_lower;
                    if matched && split > 0 {
                        job.append(&it.label[..split], 0.0, egui::TextFormat { font_id: font.clone(), color: if on { v.strong_text_color() } else { v.hyperlink_color }, ..Default::default() });
                        job.append(&it.label[split..], 0.0, egui::TextFormat { font_id: font.clone(), color: strong, ..Default::default() });
                    } else {
                        job.append(&it.label, 0.0, egui::TextFormat { font_id: font.clone(), color: strong, ..Default::default() });
                    }
                    job.wrap = egui::text::TextWrapping::truncate_at_width(r.width() * 0.6);
                    let g = ui.painter().layout_job(job);
                    let gw = g.size().x;
                    ui.painter().galley(Pos2::new(r.left() + 30.0, r.center().y - g.size().y / 2.0), g, strong);
                    if !it.detail.is_empty() {
                        let mut dj = egui::text::LayoutJob::single_section(it.detail.clone(), egui::TextFormat { font_id: fonts::ui_font(11.5), color: weak, ..Default::default() });
                        let room = (r.width() - gw - 52.0).max(0.0);
                        dj.wrap = egui::text::TextWrapping::truncate_at_width(room);
                        let dg = ui.painter().layout_job(dj);
                        ui.painter().galley(Pos2::new(r.right() - 8.0 - dg.size().x, r.center().y - dg.size().y / 2.0), dg, weak);
                    }
                    if resp.clicked() {
                        pick = Some(*k);
                    }
                }
                if total > ROWS {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(format!("{} of {total}", sel + 1)).font(fonts::ui_font(11.0)).color(weak));
                    });
                }
            });
        });
        if let Some(k) = pick {
            self.accept(ctx, id, k);
            ctx.memory_mut(|m| m.request_focus(id));
        }
    }
}

/// A suggestion's kind as a letter and a color from the code's colors.
fn kind_badge(kind: u32, c: &rivet_look::highlight::Colors, weak: Color32) -> (&'static str, Color32) {
    match kind {
        2 => ("m", c.function),
        3 => ("f", c.function),
        4 => ("c", c.function),
        5 | 10 => ("p", c.number),
        6 | 12 => ("v", c.number),
        7 => ("C", c.type_),
        8 => ("I", c.type_),
        9 => ("M", c.keyword),
        13 => ("E", c.type_),
        14 => ("k", c.keyword),
        15 => ("s", c.string),
        20 => ("e", c.type_),
        21 => ("K", c.number),
        22 => ("S", c.type_),
        25 => ("T", c.type_),
        0 => ("w", weak),
        _ => ("·", weak),
    }
}

/// Wavy lines under the problems (red errors, amber warnings, quiet hints) and the message
/// under the pointer.
pub(crate) fn draw_diags(ui: &egui::Ui, out: &egui::text_edit::TextEditOutput, text: &str, diags: &[DiagRange]) {
    if diags.is_empty() {
        return;
    }
    let st = status(ui.ctx());
    let clip = ui.clip_rect();
    let p = ui.painter().with_clip_rect(clip);
    let n = out.galley.text().chars().count();
    let to_screen = |ch: usize| out.galley.pos_from_cursor(egui::text::CCursor::new(ch.min(n))).translate(out.galley_pos.to_vec2());
    let mut hover: Option<String> = None;
    let pointer = ui.ctx().pointer_hover_pos().filter(|pp| out.response.response.rect.contains(*pp));
    let under = pointer.map(|pp| out.galley.cursor_from_pos(pp - out.galley_pos).index.0);
    // Line ends, to stop a line that runs on at the end of its first line.
    let ends: Vec<usize> = text.chars().enumerate().filter(|(_, c)| *c == '\n').map(|(i, _)| i).collect();
    for (a, b, sev, msg) in diags {
        let (a, b) = ((*a).min(n), (*b).min(n));
        let ra = to_screen(a);
        if ra.bottom() < clip.top() || ra.top() > clip.bottom() {
            continue;
        }
        let line_end = ends.iter().copied().find(|e| *e >= a).unwrap_or(n);
        let b2 = if b <= a { (a + 1).min(line_end.max(a + 1)) } else { b.min(line_end.max(a + 1)) };
        let rb = to_screen(b2);
        let x0 = ra.left();
        let x1 = if (rb.top() - ra.top()).abs() < 1.0 { rb.left().max(x0 + 6.0) } else { x0 + 12.0 };
        let col = match sev {
            1 => st.danger,
            2 => st.warning,
            _ => st.info.gamma_multiply(0.7),
        };
        let y = ra.bottom() - 1.5;
        let mut pts = Vec::new();
        let mut x = x0;
        let mut up = true;
        while x <= x1 {
            pts.push(Pos2::new(x, if up { y - 1.2 } else { y + 1.2 }));
            up = !up;
            x += 2.5;
        }
        if pts.len() >= 2 {
            p.add(egui::Shape::line(pts, Stroke::new(1.1, col)));
        }
        if let Some(u) = under
            && u >= a
            && u <= b2
        {
            hover = Some(match &hover {
                Some(h) => format!("{h}\n{msg}"),
                None => msg.clone(),
            });
        }
    }
    if let Some(msg) = hover {
        out.response.response.clone().on_hover_text_at_pointer(msg);
    }
}

/// The problems in the status bar: "2 errors · 1 warning".
pub(crate) fn diag_summary(diags: &[DiagRange]) -> Option<(String, bool)> {
    let errors = diags.iter().filter(|d| d.2 == 1).count();
    let warnings = diags.iter().filter(|d| d.2 == 2).count();
    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(format!("{errors} error{}", if errors == 1 { "" } else { "s" }));
    }
    if warnings > 0 {
        parts.push(format!("{warnings} warning{}", if warnings == 1 { "" } else { "s" }));
    }
    (!parts.is_empty()).then(|| (parts.join(" · "), errors > 0))
}

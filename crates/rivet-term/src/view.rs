//! The terminal as an egui widget: draws a session's screen (with scrollback and selection) and
//! sends it the keyboard and mouse while it has focus. Apps put their own shortcuts first
//! (Rivet's use Ctrl+Shift, so plain Ctrl+letters reach the program), then call [`show`].

pub use alacritty_terminal::grid::Scroll;
use alacritty_terminal::grid::Dimensions;
pub use alacritty_terminal::index::Point;
use alacritty_terminal::index::{Column, Direction, Line};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use eframe::egui::{self, Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;

use crate::{Session, Size, TermColors};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CursorStyle {
    #[default]
    Block,
    Beam,
    Underline,
}

impl CursorStyle {
    pub const ALL: [CursorStyle; 3] = [CursorStyle::Block, CursorStyle::Beam, CursorStyle::Underline];
    pub fn name(self) -> &'static str {
        match self {
            CursorStyle::Block => "block",
            CursorStyle::Beam => "beam",
            CursorStyle::Underline => "underline",
        }
    }
    pub fn from_name(s: &str) -> CursorStyle {
        Self::ALL.into_iter().find(|c| c.name() == s).unwrap_or_default()
    }
}

/// How the terminal looks.
#[derive(Clone, Debug)]
pub struct Style {
    pub font: FontId,
    pub colors: TermColors,
    pub cursor: CursorStyle,
    /// Room around the text, in points.
    pub padding: f32,
    /// Bold text in the bright colours (as many terminals do).
    pub bold_is_bright: bool,
    /// Flash on the bell.
    pub visual_bell: bool,
    /// Rounded corners of the background.
    pub radius: f32,
}

/// What happened while it was drawn.
#[derive(Default)]
pub struct Output {
    /// A link Ctrl+clicked.
    pub open_url: Option<String>,
    /// It was clicked (to make it the focused pane).
    pub clicked: bool,
    /// A right-click, where (for a menu).
    pub secondary: Option<Pos2>,
    /// The widget's response (for a context menu, drag ends…).
    pub response: Option<egui::Response>,
}

/// Remembered between frames (per terminal, in egui's memory).
#[derive(Clone, Default)]
struct Memo {
    focused: bool,
    wheel: f32,
    /// The mouse button a program was told is down (for its release).
    reported: Option<u8>,
}

/// The size of one cell for this font.
pub fn cell_size(ctx: &egui::Context, font: &FontId) -> Vec2 {
    let w = ctx.fonts_mut(|f| f.glyph_width(font, 'M'));
    let h = ctx.fonts_mut(|f| f.row_height(font));
    Vec2::new(w.max(1.0), h.ceil().max(1.0))
}

/// How many columns and lines fit in `rect`.
pub fn fit(ctx: &egui::Context, style: &Style, rect: Rect) -> Size {
    let c = cell_size(ctx, &style.font);
    let inner = rect.shrink(style.padding);
    let ppp = ctx.pixels_per_point();
    Size {
        cols: ((inner.width() / c.x).floor() as u16).max(2),
        lines: ((inner.height() / c.y).floor() as u16).max(1),
        cell_w: (c.x * ppp).round() as u16,
        cell_h: (c.y * ppp).round() as u16,
    }
}

/// Paste text the way the program wants it (bracketed when it asked).
pub fn paste(session: &Session, text: &str) {
    let bracketed = session.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
    // Line breaks as Enter; no escape sequences smuggled in.
    let clean = text.replace("\r\n", "\r").replace('\n', "\r").replace('\x1b', "");
    session.scroll(Scroll::Bottom);
    if bracketed {
        session.write(format!("\x1b[200~{clean}\x1b[201~").into_bytes());
    } else {
        session.write(clean.into_bytes());
    }
}

/// The selected text, if any.
pub fn selection_text(session: &Session) -> Option<String> {
    session.term.lock().selection_to_string().filter(|s| !s.is_empty())
}

pub fn clear_selection(session: &Session) {
    session.term.lock().selection = None;
}

/// Select everything (screen and scrollback).
pub fn select_all(session: &Session) {
    let mut t = session.term.lock();
    let top = t.grid().topmost_line();
    let bottom = t.grid().bottommost_line();
    let last = t.grid().last_column();
    let mut s = Selection::new(SelectionType::Simple, Point::new(top, Column(0)), Direction::Left);
    s.update(Point::new(bottom, last), Direction::Right);
    t.selection = Some(s);
}

/// Characters that mean something in a search pattern.
const SPECIAL: &[char] = &['\\', '.', '+', '*', '?', '(', ')', '|', '[', ']', '{', '}', '^', '$', '#', '&', '-', '~'];

/// Find `query` in the screen and scrollback (any case when it's all lower case), going up
/// (`back`) or down from `from` (the last match; the bottom when None). Selects the match,
/// scrolls to it, and returns where it starts.
pub fn find(session: &Session, query: &str, back: bool, from: Option<Point>) -> Option<Point> {
    use alacritty_terminal::index::Boundary;
    use alacritty_terminal::term::search::RegexSearch;
    if query.is_empty() {
        return None;
    }
    let mut pat = String::new();
    if !query.chars().any(char::is_uppercase) {
        pat.push_str("(?i)");
    }
    for c in query.chars() {
        if SPECIAL.contains(&c) {
            pat.push('\\');
        }
        pat.push(c);
    }
    let mut re = RegexSearch::new(&pat).ok()?;
    let mut t = session.term.lock();
    let origin = match from {
        Some(p) if back => p.sub(&*t, Boundary::None, 1),
        Some(p) => p.add(&*t, Boundary::None, 1),
        None => Point::new(t.grid().bottommost_line(), t.grid().last_column()),
    };
    let dir = if back { Direction::Left } else { Direction::Right };
    let m = t.search_next(&mut re, origin, dir, Direction::Left, None)?;
    let (a, b) = (*m.start(), *m.end());
    let mut s = Selection::new(SelectionType::Simple, a, Direction::Left);
    s.update(b, Direction::Right);
    t.selection = Some(s);
    t.scroll_to_point(a);
    Some(a)
}

/// Forget the scrollback (the screen stays).
pub fn clear_scrollback(session: &Session) {
    let mut t = session.term.lock();
    t.grid_mut().clear_history();
    t.scroll_display(Scroll::Bottom);
}

/// Keys as the bytes a terminal program expects.
pub fn encode_key(key: Key, m: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let ctrl = m.ctrl || m.command;
    let param = 1 + m.shift as u8 + (m.alt as u8) * 2 + (ctrl as u8) * 4;
    let esc = |s: &str| Some(s.as_bytes().to_vec());
    let csi_letter = |c: char| -> Option<Vec<u8>> {
        if param > 1 {
            esc(&format!("\x1b[1;{param}{c}"))
        } else if app_cursor {
            esc(&format!("\x1bO{c}"))
        } else {
            esc(&format!("\x1b[{c}"))
        }
    };
    let tilde = |n: u8| -> Option<Vec<u8>> { if param > 1 { esc(&format!("\x1b[{n};{param}~")) } else { esc(&format!("\x1b[{n}~")) } };
    let alt_prefix = |mut b: Vec<u8>| {
        if m.alt {
            b.insert(0, 0x1b);
        }
        Some(b)
    };
    match key {
        Key::Enter => alt_prefix(vec![b'\r']),
        Key::Backspace => alt_prefix(vec![if ctrl { 0x08 } else { 0x7f }]),
        Key::Tab if m.shift => esc("\x1b[Z"),
        Key::Tab => alt_prefix(vec![b'\t']),
        Key::Escape => alt_prefix(vec![0x1b]),
        Key::ArrowUp => csi_letter('A'),
        Key::ArrowDown => csi_letter('B'),
        Key::ArrowRight => csi_letter('C'),
        Key::ArrowLeft => csi_letter('D'),
        Key::Home => csi_letter('H'),
        Key::End => csi_letter('F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::F1 | Key::F2 | Key::F3 | Key::F4 => {
            let c = match key {
                Key::F1 => 'P',
                Key::F2 => 'Q',
                Key::F3 => 'R',
                _ => 'S',
            };
            if param > 1 { esc(&format!("\x1b[1;{param}{c}")) } else { esc(&format!("\x1bO{c}")) }
        }
        Key::F5 => tilde(15),
        Key::F6 => tilde(17),
        Key::F7 => tilde(18),
        Key::F8 => tilde(19),
        Key::F9 => tilde(20),
        Key::F10 => tilde(21),
        Key::F11 => tilde(23),
        Key::F12 => tilde(24),
        // Ctrl with a letter or symbol: the control character.
        k if ctrl => {
            let name = k.name();
            let c = name.chars().next()?;
            let b = if name.len() == 1 && c.is_ascii_alphabetic() {
                (c.to_ascii_uppercase() as u8) & 0x1f
            } else {
                match k {
                    Key::Space | Key::Num2 => 0,
                    Key::OpenBracket | Key::Num3 => 0x1b,
                    Key::Backslash | Key::Num4 => 0x1c,
                    Key::CloseBracket | Key::Num5 => 0x1d,
                    Key::Num6 => 0x1e,
                    Key::Slash | Key::Minus | Key::Num7 => 0x1f,
                    Key::Num8 => 0x7f,
                    _ => return None,
                }
            };
            alt_prefix(vec![b])
        }
        _ => None,
    }
}

/// A mouse report for a program that asked for them (SGR, or the old form).
fn mouse_report(mode: TermMode, button: u8, at: (usize, usize), pressed: bool, m: Modifiers) -> Vec<u8> {
    let b = button + if m.shift { 4 } else { 0 } + if m.alt { 8 } else { 0 } + if m.ctrl { 16 } else { 0 };
    let (x, y) = (at.0 + 1, at.1 + 1);
    if mode.contains(TermMode::SGR_MOUSE) {
        format!("\x1b[<{b};{x};{y}{}", if pressed { 'M' } else { 'm' }).into_bytes()
    } else {
        let b = if pressed { b } else { 3 + (b & !3) };
        vec![0x1b, b'[', b'M', 32 + b, (32 + x.min(223)) as u8, (32 + y.min(223)) as u8]
    }
}

/// A link around column `col` of `text`.
pub fn url_at(text: &str, col: usize) -> Option<(usize, usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    for scheme in ["https://", "http://", "file://", "ftp://", "mailto:"] {
        let sc: Vec<char> = scheme.chars().collect();
        let mut i = 0;
        while i + sc.len() <= chars.len() {
            if chars[i..i + sc.len()] == sc[..] {
                let mut j = i + sc.len();
                while j < chars.len() && !chars[j].is_whitespace() && !"<>\"'`{}|\\^".contains(chars[j]) {
                    j += 1;
                }
                // Not the punctuation after it.
                while j > i + sc.len() && ".,;:!?)]".contains(chars[j - 1]) {
                    // Keep a ) that closes one in the link.
                    if chars[j - 1] == ')' && chars[i..j].iter().filter(|c| **c == '(').count() >= chars[i..j].iter().filter(|c| **c == ')').count() {
                        break;
                    }
                    j -= 1;
                }
                if col >= i && col < j && j > i + sc.len() {
                    return Some((i, j, chars[i..j].iter().collect()));
                }
                i = j.max(i + 1);
            } else {
                i += 1;
            }
        }
    }
    None
}

/// Draw the terminal in `rect` and, while it has focus, send it the keyboard and mouse.
pub fn show(ui: &mut egui::Ui, id: egui::Id, rect: Rect, session: &mut Session, style: &Style) -> Output {
    let ctx = ui.ctx().clone();
    let mut out = Output::default();
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let cell = cell_size(&ctx, &style.font);
    let inner = rect.shrink(style.padding);
    // Fit the screen to the space.
    let size = fit(&ctx, style, rect);
    session.resize(size);
    if resp.clicked() || resp.drag_started() || resp.secondary_clicked() {
        resp.request_focus();
        out.clicked = true;
    }
    // Focused: the keyboard is here and the window is in front (else the cursor is an outline).
    let focused = resp.has_focus();
    if focused {
        // Tab, arrows and Esc belong to the program, not to moving between widgets.
        ui.memory_mut(|m| m.set_focus_lock_filter(id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true }));
    }
    let mut memo: Memo = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let mode = *session.term.lock().mode();
    // Focus in / out, for programs that asked.
    if memo.focused != focused {
        if mode.contains(TermMode::FOCUS_IN_OUT) {
            session.write(if focused { b"\x1b[I".to_vec() } else { b"\x1b[O".to_vec() });
        }
        memo.focused = focused;
    }
    // ---- the keyboard
    if focused && !session.exited {
        let app_cursor = mode.contains(TermMode::APP_CURSOR);
        let mods_now = ctx.input(|i| i.modifiers);
        let events = ctx.input(|i| i.events.clone());
        let mut typed = false;
        for e in events {
            let bytes: Option<Vec<u8>> = match e {
                egui::Event::Text(t) => {
                    let mut b = t.into_bytes();
                    if mods_now.alt {
                        b.insert(0, 0x1b);
                    }
                    Some(b)
                }
                egui::Event::Ime(egui::ImeEvent::Commit(t)) => Some(t.into_bytes()),
                // Ctrl+C, Ctrl+X and Ctrl+V reach egui as clipboard commands; a terminal sends
                // their control characters (copy and paste are Ctrl+Shift+C / V).
                egui::Event::Copy => Some(vec![0x03]),
                egui::Event::Cut => Some(vec![0x18]),
                egui::Event::Paste(_) => Some(vec![0x16]),
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    // Letters with only Alt come as text too.
                    let letter = key.name().len() == 1;
                    if letter && !(modifiers.ctrl || modifiers.command) {
                        None
                    } else {
                        encode_key(key, modifiers, app_cursor)
                    }
                }
                _ => None,
            };
            if let Some(b) = bytes {
                session.write(b);
                typed = true;
            }
        }
        if typed {
            session.scroll(Scroll::Bottom);
            clear_selection_if_typing(session);
        }
    }
    // ---- the mouse
    let to_cell = |p: Pos2| -> (usize, usize, Direction) {
        let x = ((p.x - inner.left()) / cell.x).max(0.0);
        let y = ((p.y - inner.top()) / cell.y).max(0.0);
        let col = (x.floor() as usize).min(size.cols as usize - 1);
        let row = (y.floor() as usize).min(size.lines as usize - 1);
        (col, row, if x.fract() < 0.5 { Direction::Left } else { Direction::Right })
    };
    let (mods, pointer, primary_down) = ctx.input(|i| (i.modifiers, i.pointer.hover_pos(), i.pointer.primary_down()));
    let reporting = mode.intersects(TermMode::MOUSE_MODE) && !mods.shift;
    let offset = session.term.lock().grid().display_offset() as i32;
    // Wheel: the program's (when it asked), arrows in full-screen programs, else scrollback.
    if resp.hovered() {
        let dy = ctx.input(|i| i.smooth_scroll_delta.y);
        if dy != 0.0 {
            memo.wheel += dy / cell.y;
            let lines = memo.wheel.trunc() as i32;
            memo.wheel -= lines as f32;
            if lines != 0 {
                if reporting {
                    if let Some(p) = pointer {
                        let (c, r, _) = to_cell(p);
                        for _ in 0..lines.abs().min(10) {
                            session.write(mouse_report(mode, if lines > 0 { 64 } else { 65 }, (c, r), true, mods));
                        }
                    }
                } else if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
                    let k = if lines > 0 { if mode.contains(TermMode::APP_CURSOR) { "\x1bOA" } else { "\x1b[A" } } else if mode.contains(TermMode::APP_CURSOR) { "\x1bOB" } else { "\x1b[B" };
                    session.write(k.repeat(lines.unsigned_abs().min(10) as usize).into_bytes());
                } else {
                    session.scroll(Scroll::Delta(lines * 3));
                }
            }
            ctx.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        }
    }
    // Links: with Ctrl held, underline the one under the pointer; Ctrl+click opens it.
    let mut link: Option<(usize, usize, usize)> = None;
    if mods.command
        && let Some(p) = pointer.filter(|p| inner.contains(*p))
    {
        let (c, r, _) = to_cell(p);
        let t = session.term.lock();
        let line = Line(r as i32 - offset);
        let row = &t.grid()[line];
        let text: String = (0..size.cols as usize).map(|k| row[Column(k)].c).collect();
        drop(t);
        if let Some((a, b, url)) = url_at(&text, c) {
            link = Some((r, a, b));
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                out.open_url = Some(url);
            }
        }
    }
    if reporting && link.is_none() {
        // Clicks go to the program.
        let pressed = ctx.input(|i| {
            i.events.iter().filter_map(|e| if let egui::Event::PointerButton { button, pressed, pos, .. } = e { Some((*button, *pressed, *pos)) } else { None }).collect::<Vec<_>>()
        });
        for (button, down, pos) in pressed {
            if !inner.contains(pos) && down {
                continue;
            }
            let b = match button {
                egui::PointerButton::Primary => 0,
                egui::PointerButton::Middle => 1,
                egui::PointerButton::Secondary => 2,
                _ => continue,
            };
            let (c, r, _) = to_cell(pos);
            session.write(mouse_report(mode, b, (c, r), down, mods));
            memo.reported = if down { Some(b) } else { None };
        }
        if let (Some(b), Some(p)) = (memo.reported, pointer)
            && resp.dragged()
            && mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
        {
            let (c, r, _) = to_cell(p);
            session.write(mouse_report(mode, b + 32, (c, r), true, mods));
        }
    } else if link.is_none() {
        // Selecting: drag for characters, double-click for a word, triple for lines.
        let at = |p: Pos2| {
            let (c, r, side) = to_cell(p);
            (Point::new(Line(r as i32 - offset), Column(c)), side)
        };
        if let Some(p) = resp.interact_pointer_pos() {
            let (pt, side) = at(p);
            if resp.triple_clicked() {
                session.term.lock().selection = Some(Selection::new(SelectionType::Lines, pt, side));
            } else if resp.double_clicked() {
                session.term.lock().selection = Some(Selection::new(SelectionType::Semantic, pt, side));
            } else if resp.drag_started_by(egui::PointerButton::Primary) {
                let ty = if mods.alt { SelectionType::Block } else { SelectionType::Simple };
                session.term.lock().selection = Some(Selection::new(ty, pt, side));
            } else if resp.dragged_by(egui::PointerButton::Primary) && primary_down {
                if let Some(s) = session.term.lock().selection.as_mut() {
                    s.update(pt, side);
                }
                // Past the top or bottom: scroll while selecting.
                if p.y < inner.top() {
                    session.scroll(Scroll::Delta(1));
                } else if p.y > inner.bottom() {
                    session.scroll(Scroll::Delta(-1));
                }
            } else if resp.clicked() {
                clear_selection(session);
            }
        }
    }
    if resp.secondary_clicked() {
        out.secondary = resp.interact_pointer_pos();
    }
    out.response = Some(resp.clone());
    ctx.data_mut(|d| d.insert_temp(id, memo));
    // ---- drawing
    let now = ctx.input(|i| i.time);
    draw(ui, rect, inner, cell, session, style, focused, link, now);
    if !focused && resp.hovered() && !reporting {
        ctx.set_cursor_icon(egui::CursorIcon::Text);
    }
    out
}

fn clear_selection_if_typing(session: &Session) {
    let mut t = session.term.lock();
    if t.selection.is_some() {
        t.selection = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(ui: &egui::Ui, rect: Rect, inner: Rect, cell: Vec2, session: &Session, style: &Style, focused: bool, link: Option<(usize, usize, usize)>, now: f64) {
    let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let c = &style.colors;
    let term = session.term.lock();
    let set = |k: usize| term.colors()[k];
    let content = term.renderable_content();
    let bg = c.resolve(alacritty_terminal::vte::ansi::Color::Named(alacritty_terminal::vte::ansi::NamedColor::Background), &set);
    p.rect_filled(rect, style.radius, bg);
    let offset = content.display_offset as i32;
    let sel = content.selection;
    let cursor = content.cursor;
    let show_cursor = content.mode.contains(TermMode::SHOW_CURSOR) && offset == 0;
    let font = &style.font;
    let mut italic = font.clone();
    italic.family = font.family.clone();
    let x0 = inner.left();
    // One run of text: consecutive cells with the same look.
    struct Run {
        row: usize,
        col: usize,
        text: String,
        fg: Color32,
        flags: Flags,
    }
    let mut runs: Vec<Run> = Vec::new();
    let mut cursor_cell: Option<(usize, usize, char, Color32, Color32, bool)> = None;
    for indexed in content.display_iter {
        let cell_ref = indexed.cell;
        let flags = cell_ref.flags;
        if flags.contains(Flags::WIDE_CHAR_SPACER) || flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
            continue;
        }
        let row = (indexed.point.line.0 + offset) as usize;
        let col = indexed.point.column.0;
        let mut fg = c.resolve(cell_ref.fg, &set);
        let mut cbg = c.resolve(cell_ref.bg, &set);
        if style.bold_is_bright
            && flags.contains(Flags::BOLD)
            && let alacritty_terminal::vte::ansi::Color::Named(n) = cell_ref.fg
            && (n as usize) < 8
        {
            fg = c.resolve(alacritty_terminal::vte::ansi::Color::Indexed(n as u8 + 8), &set);
        }
        if flags.contains(Flags::DIM) {
            fg = mix(fg, cbg, 0.4);
        }
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut cbg);
        }
        let selected = sel.is_some_and(|s| s.contains(indexed.point));
        if selected {
            cbg = c.selection;
        }
        let wide = flags.contains(Flags::WIDE_CHAR);
        let w = if wide { 2.0 } else { 1.0 };
        let pos = Pos2::new(x0 + col as f32 * cell.x, inner.top() + row as f32 * cell.y);
        if cbg != bg {
            p.rect_filled(Rect::from_min_size(pos, Vec2::new(cell.x * w, cell.y)), 0.0, cbg);
        }
        let is_cursor = show_cursor && cursor.point.line == indexed.point.line && cursor.point.column == indexed.point.column;
        if is_cursor {
            cursor_cell = Some((row, col, cell_ref.c, fg, cbg, wide));
        }
        let ch = cell_ref.c;
        if ch == ' ' || ch == '\0' || flags.contains(Flags::HIDDEN) {
            // Underlines still show under spaces.
            if flags.intersects(Flags::ALL_UNDERLINES) {
                p.line_segment([pos + Vec2::new(0.0, cell.y - 1.5), pos + Vec2::new(cell.x * w, cell.y - 1.5)], Stroke::new(1.0, fg));
            }
            continue;
        }
        let mut s = String::new();
        s.push(ch);
        if let Some(z) = cell_ref.zerowidth() {
            s.extend(z);
        }
        // Plain ASCII joins the run before it; anything else stands alone in its cell(s).
        let keep = ch.is_ascii() && !wide && cell_ref.zerowidth().is_none();
        if keep
            && let Some(r) = runs.last_mut()
            && r.row == row
            && r.col + r.text.chars().count() == col
            && r.fg == fg
            && r.flags == flags
            && r.text.is_ascii()
        {
            r.text.push(ch);
            continue;
        }
        runs.push(Run { row, col, text: s, fg, flags });
    }
    for r in &runs {
        let pos = Pos2::new(x0 + r.col as f32 * cell.x, inner.top() + r.row as f32 * cell.y);
        let n = r.text.chars().count();
        if r.text.is_ascii() {
            let fmt = egui::TextFormat { font_id: font.clone(), color: r.fg, italics: r.flags.contains(Flags::ITALIC), ..Default::default() };
            let job = egui::text::LayoutJob::single_section(r.text.clone(), fmt);
            let g = p.layout_job(job);
            p.galley(pos, g.clone(), r.fg);
            if r.flags.contains(Flags::BOLD) {
                p.galley(pos + Vec2::new(0.55, 0.0), g, r.fg);
            }
        } else {
            // A wide or unusual character: centred in its cells.
            let w = if r.flags.contains(Flags::WIDE_CHAR) { 2.0 } else { 1.0 };
            p.text(pos + Vec2::new(cell.x * w / 2.0, cell.y / 2.0), Align2::CENTER_CENTER, &r.text, font.clone(), r.fg);
        }
        let width = if r.text.is_ascii() { n as f32 } else if r.flags.contains(Flags::WIDE_CHAR) { 2.0 } else { 1.0 } * cell.x;
        if r.flags.intersects(Flags::ALL_UNDERLINES) {
            let y = pos.y + cell.y - 1.5;
            p.line_segment([Pos2::new(pos.x, y), Pos2::new(pos.x + width, y)], Stroke::new(1.0, r.fg));
            if r.flags.contains(Flags::DOUBLE_UNDERLINE) {
                p.line_segment([Pos2::new(pos.x, y - 2.5), Pos2::new(pos.x + width, y - 2.5)], Stroke::new(1.0, r.fg));
            }
        }
        if r.flags.contains(Flags::STRIKEOUT) {
            let y = pos.y + cell.y * 0.55;
            p.line_segment([Pos2::new(pos.x, y), Pos2::new(pos.x + width, y)], Stroke::new(1.0, r.fg));
        }
    }
    // The link under the pointer (Ctrl held).
    if let Some((row, a, b)) = link {
        let y = inner.top() + (row as f32 + 1.0) * cell.y - 1.0;
        p.line_segment([Pos2::new(x0 + a as f32 * cell.x, y), Pos2::new(x0 + b as f32 * cell.x, y)], Stroke::new(1.2, c.cursor));
    }
    // The cursor: solid when focused, an outline when not.
    if show_cursor {
        let (row, col, ch, fg, cbg, wide) = cursor_cell.unwrap_or(((cursor.point.line.0 + offset).max(0) as usize, cursor.point.column.0, ' ', c.fg, bg, false));
        let pos = Pos2::new(x0 + col as f32 * cell.x, inner.top() + row as f32 * cell.y);
        let w = if wide { cell.x * 2.0 } else { cell.x };
        let r = Rect::from_min_size(pos, Vec2::new(w, cell.y));
        let shape = match cursor.shape {
            alacritty_terminal::vte::ansi::CursorShape::Beam => CursorStyle::Beam,
            alacritty_terminal::vte::ansi::CursorShape::Underline => CursorStyle::Underline,
            alacritty_terminal::vte::ansi::CursorShape::Hidden => {
                drop(term);
                return;
            }
            _ => style.cursor,
        };
        if !focused {
            p.rect_stroke(r.shrink(0.5), 1.0, Stroke::new(1.0, c.cursor), egui::StrokeKind::Inside);
        } else {
            match shape {
                CursorStyle::Block => {
                    p.rect_filled(r, 1.0, c.cursor);
                    if ch != ' ' && ch != '\0' {
                        let on = if rivet_look::palette::luminance(c.cursor) > 0.55 { mix(cbg, Color32::BLACK, 0.6) } else { mix(fg, Color32::WHITE, 0.6) };
                        p.text(r.center(), Align2::CENTER_CENTER, ch, font.clone(), on);
                    }
                }
                CursorStyle::Beam => {
                    p.rect_filled(Rect::from_min_size(pos, Vec2::new(2.0, cell.y)), 0.0, c.cursor);
                }
                CursorStyle::Underline => {
                    p.rect_filled(Rect::from_min_size(pos + Vec2::new(0.0, cell.y - 2.0), Vec2::new(w, 2.0)), 0.0, c.cursor);
                }
            }
        }
    }
    // Scrolled back: where in the history, on the right edge.
    if offset > 0 {
        let total = term.grid().history_size() as f32 + term.grid().screen_lines() as f32;
        let h = (term.grid().screen_lines() as f32 / total * rect.height()).max(24.0);
        let top = rect.top() + (1.0 - (offset as f32 + term.grid().screen_lines() as f32) / total) * (rect.height() - h).max(0.0) / (1.0 - term.grid().screen_lines() as f32 / total).max(0.001);
        p.rect_filled(Rect::from_min_size(Pos2::new(rect.right() - 5.0, top.clamp(rect.top(), rect.bottom() - h)), Vec2::new(3.0, h)), 2.0, mix(bg, c.fg, 0.35));
        let label = format!("↑ {offset}");
        p.text(Pos2::new(rect.right() - 12.0, rect.top() + 8.0), Align2::RIGHT_TOP, label, FontId::proportional(11.0), mix(bg, c.fg, 0.55));
    }
    // The bell: a short flash.
    if style.visual_bell
        && let Some(t) = session.bell_at
    {
        let age = (now - t) as f32;
        if age < 0.25 {
            p.rect_filled(rect, style.radius, c.fg.gamma_multiply(0.08 * (1.0 - age / 0.25)));
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_links() {
        let none = Modifiers::NONE;
        assert_eq!(encode_key(Key::Enter, none, false), Some(b"\r".to_vec()));
        assert_eq!(encode_key(Key::ArrowUp, none, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode_key(Key::ArrowUp, none, true), Some(b"\x1bOA".to_vec()));
        assert_eq!(encode_key(Key::ArrowLeft, Modifiers::CTRL, false), Some(b"\x1b[1;5D".to_vec()));
        assert_eq!(encode_key(Key::D, Modifiers::CTRL, false), Some(vec![4]));
        assert_eq!(encode_key(Key::C, Modifiers::CTRL | Modifiers::ALT, false), Some(vec![0x1b, 3]));
        assert_eq!(encode_key(Key::Delete, none, false), Some(b"\x1b[3~".to_vec()));
        assert_eq!(encode_key(Key::F5, Modifiers::SHIFT, false), Some(b"\x1b[15;2~".to_vec()));
        assert_eq!(encode_key(Key::Tab, Modifiers::SHIFT, false), Some(b"\x1b[Z".to_vec()));
        assert_eq!(encode_key(Key::A, none, false), None);
        let t = "see https://example.com/a_(b). and more";
        assert_eq!(url_at(t, 6).map(|u| u.2), Some("https://example.com/a_(b)".into()));
        assert_eq!(url_at(t, 1), None);
        assert_eq!(mouse_report(TermMode::SGR_MOUSE, 0, (4, 2), true, Modifiers::NONE), b"\x1b[<0;5;3M".to_vec());
    }
}

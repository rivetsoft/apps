//! The Rivet Terminal window: tabs along the top, each holding one or more panes (terminals)
//! arranged by a layout (Tall, Fat, Grid, Side by Side, Stacked, or one at a time), like kitty's
//! windows and layouts. Built on the Rivet apps' shared look: theme and desktop colours, fonts,
//! icons, in-app windows and the quick panel. The terminal itself is `rivet_term`'s.

mod prefs;
mod quick;

use std::path::PathBuf;

use eframe::egui::{self, Align2, Color32, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::icons::{self, IconTheme};
use rivet_look::live::{Choice, LiveLook};
use rivet_look::theme::{r_lg, r_md, r_sm};
use rivet_look::{fonts, palette, windows};
use rivet_term::{Notice, Session, Size, Spawn, TermColors, view};

use crate::settings::Settings;

pub const ICON_SVG: &str = include_str!("../data/rivet-terminal.svg");

/// What to start with.
#[derive(Default)]
pub struct Start {
    pub cwd: Option<PathBuf>,
    pub program: Option<(String, Vec<String>)>,
    /// Checks: the demo project and script (instead of the person's shell).
    pub demo: Option<(PathBuf, PathBuf)>,
}

/// How a tab's panes are arranged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Layout {
    /// The first pane on the left, the others stacked on the right.
    #[default]
    Tall,
    /// The first pane on top, the others side by side below.
    Fat,
    Grid,
    SideBySide,
    Stacked,
    /// Only the focused pane (the others keep running).
    One,
}

impl Layout {
    pub const ALL: [Layout; 6] = [Layout::Tall, Layout::Fat, Layout::Grid, Layout::SideBySide, Layout::Stacked, Layout::One];
    pub fn name(self) -> &'static str {
        match self {
            Layout::Tall => "Tall",
            Layout::Fat => "Fat",
            Layout::Grid => "Grid",
            Layout::SideBySide => "Side by Side",
            Layout::Stacked => "Stacked",
            Layout::One => "One at a Time",
        }
    }
    fn next(self) -> Layout {
        let i = Self::ALL.iter().position(|l| *l == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
    /// Where each of `n` panes goes in `area` (None: hidden), `gap` apart.
    pub fn rects(self, area: Rect, n: usize, focus: usize, gap: f32) -> Vec<Option<Rect>> {
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![Some(area)];
        }
        let split = |r: Rect, parts: usize, across: bool| -> Vec<Rect> {
            let total = if across { r.width() } else { r.height() } - gap * (parts as f32 - 1.0);
            let each = total / parts as f32;
            (0..parts)
                .map(|i| {
                    let start = i as f32 * (each + gap);
                    if across { Rect::from_min_size(Pos2::new(r.left() + start, r.top()), Vec2::new(each, r.height())) } else { Rect::from_min_size(Pos2::new(r.left(), r.top() + start), Vec2::new(r.width(), each)) }
                })
                .collect()
        };
        match self {
            Layout::One => (0..n).map(|i| (i == focus).then_some(area)).collect(),
            Layout::SideBySide => split(area, n, true).into_iter().map(Some).collect(),
            Layout::Stacked => split(area, n, false).into_iter().map(Some).collect(),
            Layout::Tall | Layout::Fat => {
                let across = self == Layout::Tall;
                let halves = split(area, 2, across);
                let mut out = vec![Some(halves[0])];
                out.extend(split(halves[1], n - 1, !across).into_iter().map(Some));
                out
            }
            Layout::Grid => {
                let cols = (n as f32).sqrt().ceil() as usize;
                let rows = n.div_ceil(cols);
                let row_rects = split(area, rows, false);
                let mut out = Vec::new();
                for (r, rr) in row_rects.iter().enumerate() {
                    let in_row = (n - r * cols).min(cols);
                    out.extend(split(*rr, in_row, true).into_iter().map(Some));
                }
                out
            }
        }
    }
}

/// One terminal.
pub(crate) struct Pane {
    id: u64,
    pub(crate) session: Session,
}

/// One tab: its panes, which has focus, and how they're arranged.
pub(crate) struct TermTab {
    pub(crate) panes: Vec<Pane>,
    pub(crate) focus: usize,
    pub(crate) layout: Layout,
    /// What its tab says (updated now and then).
    label: String,
}

/// A question the window is asking.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Prompt {
    /// Close this tab (or the window) while something still runs?
    ClosePane { tab: usize, pane: usize, what: String },
    CloseTab { tab: usize, what: String },
    Quit { what: String },
}

/// Searching the focused pane's screen and scrollback.
#[derive(Default)]
pub(crate) struct FindBar {
    query: String,
    last: Option<view::Point>,
    focus: bool,
    missed: bool,
}

pub struct TermApp {
    pub(crate) s: Settings,
    look: LiveLook,
    icons: IconTheme,
    app_icon: Option<egui::TextureHandle>,
    pub(crate) families: Option<Vec<String>>,
    pub(crate) tabs: Vec<TermTab>,
    pub(crate) active: usize,
    next_id: u64,
    start: Start,
    pub(crate) quick: Option<(quick::QuickKind, rivet_look::quick::QuickPanel)>,
    pub(crate) prefs_open: bool,
    prefs_win: windows::WinState,
    pub(crate) prefs_page: prefs::Page,
    pub(crate) scheme_preview: Option<String>,
    pub(crate) note: Option<(String, f64)>,
    pub(crate) shot: Option<rivet_kit::Shot>,
    title: String,
    pub(crate) find: Option<FindBar>,
    pub(crate) prompt: Option<Prompt>,
    allow_close: bool,
    /// Ctrl+Shift+= / - changes, on top of the chosen size.
    pub(crate) zoom: f32,
    /// Give the focused pane the keyboard next frame.
    pub(crate) refocus: bool,
    labels_at: f64,
    /// Checks: search with the find bar's text at this frame (after the demo has drawn).
    find_at: Option<u32>,
    ctx: egui::Context,
}

impl TermApp {
    pub fn new(cc: &eframe::CreationContext, start: Start, screenshot: Option<PathBuf>) -> TermApp {
        let mut s = Settings::load();
        if screenshot.is_some() {
            crate::settings::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
            // Checks look the same everywhere.
            s = Settings { theme: s.theme, accent: s.accent, follow_desktop_colors: s.follow_desktop_colors, ..Settings::default() };
            s.builtin_icons = true;
        }
        match std::env::var("RIVET_TERM_THEME").as_deref() {
            Ok("light") => (s.theme, s.follow_desktop_colors) = (rivet_look::theme::Mode::Light, false),
            Ok("dark") => (s.theme, s.follow_desktop_colors) = (rivet_look::theme::Mode::Dark, false),
            _ => {}
        }
        // Checks: RIVET_TERM_ACCENT=rrggbb picks the accent (and stops following the desktop).
        if let Some(n) = std::env::var("RIVET_TERM_ACCENT").ok().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()) {
            s.accent = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            s.follow_desktop_colors = false;
        }
        if let Ok(n) = std::env::var("RIVET_TERM_SCHEME") {
            s.scheme = n;
        }
        if std::env::var_os("RIVET_TERM_MENUBAR").is_some() {
            s.show_menubar = true;
        }
        let prefs_page = std::env::var("RIVET_TERM_PREFS").map(|p| prefs::Page::from_name(&p)).unwrap_or_default();
        let mut app = TermApp {
            s,
            look: LiveLook::new(),
            icons: IconTheme::new(&icons::detect_system_theme()),
            app_icon: None,
            families: None,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            start,
            quick: None,
            prefs_open: std::env::var_os("RIVET_TERM_PREFS").is_some(),
            prefs_win: windows::WinState::new(1, Vec2::new(700.0, 520.0)),
            prefs_page,
            scheme_preview: None,
            note: None,
            shot: screenshot.map(rivet_kit::Shot::new),
            title: String::new(),
            find: None,
            prompt: None,
            allow_close: false,
            zoom: 0.0,
            refocus: true,
            labels_at: -10.0,
            find_at: None,
            ctx: cc.egui_ctx.clone(),
        };
        // Checks: RIVET_TERM_TABS=n tabs, RIVET_TERM_SPLIT=n panes in the first, RIVET_TERM_LAYOUT.
        let tabs: usize = std::env::var("RIVET_TERM_TABS").ok().and_then(|v| v.parse().ok()).unwrap_or(1).clamp(1, 8);
        for _ in 0..tabs {
            app.new_tab(None);
        }
        app.active = 0;
        let split: usize = std::env::var("RIVET_TERM_SPLIT").ok().and_then(|v| v.parse().ok()).unwrap_or(1).clamp(1, 6);
        for _ in 1..split {
            app.split();
        }
        if let Ok(l) = std::env::var("RIVET_TERM_LAYOUT")
            && let Some(t) = app.tabs.get_mut(0)
        {
            t.layout = Layout::ALL.into_iter().find(|x| x.name().eq_ignore_ascii_case(&l)).unwrap_or_default();
        }
        if let Some(t) = app.tabs.get_mut(0) {
            t.focus = 0;
        }
        if let Ok(q) = std::env::var("RIVET_TERM_FIND") {
            app.find = Some(FindBar { query: q, focus: true, ..Default::default() });
            app.find_at = Some(30);
        }
        if let Ok(q) = std::env::var("RIVET_TERM_QUICK") {
            app.open_quick(if q == "themes" { quick::QuickKind::Themes } else { quick::QuickKind::Commands });
        }
        app
    }

    pub(crate) fn choice(&self) -> Choice {
        Choice { mode: self.s.theme, accent: self.s.accent, follow_desktop_colors: self.s.follow_desktop_colors, text_size: self.s.text_size }
    }

    pub(crate) fn accent(&self) -> Color32 {
        self.look.accent(&self.choice())
    }

    pub(crate) fn palette(&self) -> Option<rivet_look::palette::Palette> {
        self.look.palette().cloned()
    }

    pub(crate) fn icon(&mut self, ctx: &egui::Context, names: &[&str], px: f32) -> Option<egui::TextureHandle> {
        self.icons.texture(ctx, names, (px * ctx.pixels_per_point()).round() as u32)
    }

    pub(crate) fn app_icon(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if self.app_icon.is_none() {
            let px = (96.0 * ctx.pixels_per_point()) as u32;
            self.app_icon = icons::render_svg(ICON_SVG.as_bytes(), px).map(|img| ctx.load_texture("rivet-terminal-icon", img, egui::TextureOptions::LINEAR));
        }
        self.app_icon.clone()
    }

    /// The scheme now (the one being previewed, else the chosen one).
    pub(crate) fn scheme(&self) -> Option<&'static rivet_look::schemes::Scheme> {
        let name = self.scheme_preview.as_deref().unwrap_or(&self.s.scheme);
        rivet_look::schemes::Scheme::by_name(name)
    }

    /// The terminal's colours: the scheme's, else the theme's.
    pub(crate) fn term_colors(&self, v: &egui::Visuals) -> TermColors {
        match self.scheme() {
            Some(s) => TermColors::from_scheme(s),
            None => {
                // The desktop palette's own secondary and tertiary when following it (a new
                // wallpaper changes all three), else ones made from the accent.
                let a = self.accent();
                let pal = self.palette().filter(|p| rivet_look::palette::follows(Some(p), self.s.follow_desktop_colors));
                let s = pal.as_ref().and_then(|p| p.secondary).unwrap_or_else(|| rivet_look::builtin_icons::derive_secondary(a));
                let t = pal.as_ref().and_then(|p| p.tertiary).unwrap_or_else(|| rivet_look::builtin_icons::derive_tertiary(a));
                TermColors::from_theme_trio(v.extreme_bg_color, v.text_color(), [a, s, t])
            }
        }
    }

    pub(crate) fn font(&self) -> egui::FontId {
        egui::FontId::monospace(((self.s.font_size + self.zoom).clamp(6.0, 48.0)) * fonts::scale())
    }

    pub(crate) fn flash(&mut self, text: impl Into<String>) {
        let now = self.ctx.input(|i| i.time);
        self.note = Some((text.into(), now));
    }

    // ---------------------------------------------------------------- tabs and panes

    fn spawn(&mut self, cwd: Option<PathBuf>) -> Option<Pane> {
        let index = self.tabs.iter().map(|t| t.panes.len()).sum::<usize>() + 1;
        let spec = match &self.start.demo {
            // Checks: the demo script, a different scene per pane.
            Some((root, script)) => Spawn { program: Some(("sh".into(), vec![script.display().to_string(), index.min(3).to_string()])), cwd: Some(root.clone()), env: Default::default(), scrollback: self.s.scrollback },
            None => {
                let program = self.start.program.clone().or_else(|| {
                    let mut words = self.s.shell.split_whitespace().map(String::from);
                    words.next().map(|p| (p, words.collect()))
                });
                let cwd = cwd.or_else(|| self.start.cwd.clone()).filter(|d| d.is_dir()).or_else(|| Some(rivet_xdg::places::home()));
                Spawn { program, cwd, env: Default::default(), scrollback: self.s.scrollback }
            }
        };
        let size = Size { cols: 80, lines: 24, cell_w: 8, cell_h: 16 };
        match Session::spawn(&spec, size, Some(self.ctx.clone())) {
            Ok(session) => {
                let id = self.next_id;
                self.next_id += 1;
                Some(Pane { id, session })
            }
            Err(e) => {
                self.flash(e);
                None
            }
        }
    }

    /// The focused pane's folder (new tabs and panes start there).
    fn here(&self) -> Option<PathBuf> {
        let t = self.tabs.get(self.active)?;
        t.panes.get(t.focus)?.session.cwd()
    }

    pub(crate) fn new_tab(&mut self, cwd: Option<PathBuf>) {
        if let Some(p) = self.spawn(cwd) {
            self.tabs.push(TermTab { panes: vec![p], focus: 0, layout: Layout::default(), label: String::new() });
            self.active = self.tabs.len() - 1;
            self.labels_at = -10.0;
            self.refocus = true;
        }
    }

    /// A new pane in this tab, next to the focused one.
    pub(crate) fn split(&mut self) {
        let here = self.here();
        if self.tabs.is_empty() {
            self.new_tab(here);
            return;
        }
        if let Some(p) = self.spawn(here)
            && let Some(t) = self.tabs.get_mut(self.active)
        {
            let at = (t.focus + 1).min(t.panes.len());
            t.panes.insert(at, p);
            t.focus = at;
            self.refocus = true;
        }
    }

    /// Close a pane (asking first when something runs in it).
    pub(crate) fn close_pane(&mut self, tab: usize, pane: usize, force: bool) {
        let Some(t) = self.tabs.get(tab) else { return };
        let Some(p) = t.panes.get(pane) else { return };
        if !force && self.s.confirm_close && p.session.busy() {
            let what = p.session.foreground().unwrap_or_else(|| "A program".into());
            self.prompt = Some(Prompt::ClosePane { tab, pane, what });
            return;
        }
        let t = &mut self.tabs[tab];
        t.panes.remove(pane);
        if t.panes.is_empty() {
            self.tabs.remove(tab);
            if self.active >= tab && self.active > 0 {
                self.active -= 1;
            }
        } else if t.focus >= t.panes.len() || t.focus > pane {
            t.focus = t.focus.saturating_sub(1).min(t.panes.len() - 1);
        }
        self.refocus = true;
        self.labels_at = -10.0;
        if self.tabs.is_empty() {
            self.allow_close = true;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(crate) fn close_tab(&mut self, tab: usize, force: bool) {
        let Some(t) = self.tabs.get(tab) else { return };
        if !force && self.s.confirm_close
            && let Some(p) = t.panes.iter().find(|p| p.session.busy())
        {
            let what = p.session.foreground().unwrap_or_else(|| "A program".into());
            self.prompt = Some(Prompt::CloseTab { tab, what });
            return;
        }
        self.tabs.remove(tab);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
        self.refocus = true;
        if self.tabs.is_empty() {
            self.allow_close = true;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(crate) fn focus_pane(&mut self, step: i32) {
        if let Some(t) = self.tabs.get_mut(self.active) {
            let n = t.panes.len() as i32;
            t.focus = ((t.focus as i32 + step).rem_euclid(n.max(1))) as usize;
            self.refocus = true;
        }
    }

    pub(crate) fn go_tab(&mut self, step: i32) {
        let n = self.tabs.len() as i32;
        if n > 0 {
            self.active = ((self.active as i32 + step).rem_euclid(n)) as usize;
            self.refocus = true;
        }
    }

    pub(crate) fn move_tab(&mut self, step: i32) {
        let n = self.tabs.len() as i32;
        let to = self.active as i32 + step;
        if n > 1 && (0..n).contains(&to) {
            self.tabs.swap(self.active, to as usize);
            self.active = to as usize;
        }
    }

    pub(crate) fn next_layout(&mut self) {
        if let Some(t) = self.tabs.get_mut(self.active) {
            t.layout = t.layout.next();
            let name = t.layout.name();
            self.flash(format!("Layout: {name}"));
        }
    }

    pub(crate) fn set_layout(&mut self, l: Layout) {
        if let Some(t) = self.tabs.get_mut(self.active) {
            t.layout = l;
        }
    }

    /// One pane at a time, or back to how it was.
    pub(crate) fn toggle_zoom(&mut self) {
        if let Some(t) = self.tabs.get_mut(self.active) {
            t.layout = if t.layout == Layout::One { Layout::Tall } else { Layout::One };
        }
    }

    pub(crate) fn focused(&self) -> Option<&Session> {
        let t = self.tabs.get(self.active)?;
        t.panes.get(t.focus).map(|p| &p.session)
    }

    /// A new window (a new copy of the program), in this folder.
    pub(crate) fn new_window(&mut self) {
        if self.shot.is_some() {
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let mut cmd = std::process::Command::new(exe);
        if let Some(d) = self.here() {
            cmd.arg("--directory").arg(d);
        }
        if let Err(e) = cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
            self.flash(format!("Couldn't open a window: {e}"));
        }
    }

    // ---------------------------------------------------------------- clipboard and find

    pub(crate) fn copy(&mut self) {
        let Some(text) = self.focused().and_then(view::selection_text) else {
            self.flash("Select some text first");
            return;
        };
        if self.shot.is_none() {
            self.ctx.copy_text(text);
        }
    }

    pub(crate) fn paste(&mut self) {
        if self.shot.is_some() {
            return;
        }
        let text = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok()).unwrap_or_default();
        if let (false, Some(s)) = (text.is_empty(), self.focused()) {
            view::paste(s, &text);
        }
    }

    pub(crate) fn select_all(&mut self) {
        if let Some(s) = self.focused() {
            view::select_all(s);
        }
    }

    pub(crate) fn clear_scrollback(&mut self) {
        if let Some(s) = self.focused() {
            view::clear_scrollback(s);
            // Ask the shell to draw its prompt again at the top.
            s.write(vec![0x0c]);
        }
    }

    pub(crate) fn open_find(&mut self) {
        let mut f = self.find.take().unwrap_or_default();
        f.focus = true;
        self.find = Some(f);
    }

    fn find_step(&mut self, back: bool) {
        let Some(mut f) = self.find.take() else { return };
        if let Some(s) = self.focused() {
            let found = view::find(s, &f.query, back, f.last).or_else(|| view::find(s, &f.query, back, None));
            f.missed = found.is_none() && !f.query.is_empty();
            f.last = found;
        }
        self.find = Some(f);
    }

    // ---------------------------------------------------------------- keys

    /// Ctrl+Shift+`key` (by the letter typed or the key's place, so any keyboard layout works).
    fn take(ctx: &egui::Context, mods: Modifiers, keys: &[Key]) -> bool {
        ctx.input_mut(|i| {
            let at = i.events.iter().position(|e| match e {
                egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } => {
                    let m = Modifiers { command: false, mac_cmd: false, ..*modifiers };
                    let want = Modifiers { command: false, mac_cmd: false, ..mods };
                    m == want && (keys.contains(key) || physical_key.is_some_and(|p| keys.contains(&p)))
                }
                _ => false,
            });
            if let Some(k) = at {
                i.events.remove(k);
                true
            } else {
                false
            }
        })
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.quick.is_some() || self.prompt.is_some() {
            return;
        }
        let cs = Modifiers::CTRL | Modifiers::SHIFT;
        let c = Modifiers::CTRL;
        let sh = Modifiers::SHIFT;
        type Act = fn(&mut TermApp);
        let table: &[(Modifiers, &[Key], Act)] = &[
            (cs, &[Key::T], |a| a.new_tab(a.here())),
            (cs, &[Key::Q], |a| a.close_tab(a.active, false)),
            (cs, &[Key::N], |a| a.new_window()),
            (cs, &[Key::Enter], |a| a.split()),
            (cs, &[Key::W], |a| a.close_pane(a.active, a.tabs.get(a.active).map(|t| t.focus).unwrap_or(0), false)),
            (cs, &[Key::CloseBracket, Key::CloseCurlyBracket], |a| a.focus_pane(1)),
            (cs, &[Key::OpenBracket, Key::OpenCurlyBracket], |a| a.focus_pane(-1)),
            (cs, &[Key::L], |a| a.next_layout()),
            (cs, &[Key::Z], |a| a.toggle_zoom()),
            (cs, &[Key::ArrowRight], |a| a.go_tab(1)),
            (cs, &[Key::ArrowLeft], |a| a.go_tab(-1)),
            (cs, &[Key::Tab], |a| a.go_tab(-1)),
            (c, &[Key::Tab], |a| a.go_tab(1)),
            (cs, &[Key::Period], |a| a.move_tab(1)),
            (cs, &[Key::Comma], |a| a.move_tab(-1)),
            (cs, &[Key::C], |a| a.copy()),
            (cs, &[Key::V], |a| a.paste()),
            (sh, &[Key::Insert], |a| a.paste()),
            (cs, &[Key::Equals, Key::Plus], |a| a.zoom = (a.zoom + 1.0).min(30.0)),
            (cs, &[Key::Minus], |a| a.zoom = (a.zoom - 1.0).max(-10.0)),
            (cs, &[Key::Backspace], |a| a.zoom = 0.0),
            (cs, &[Key::ArrowUp], |a| a.scroll(view::Scroll::Delta(1))),
            (cs, &[Key::ArrowDown], |a| a.scroll(view::Scroll::Delta(-1))),
            (cs, &[Key::PageUp], |a| a.scroll(view::Scroll::PageUp)),
            (cs, &[Key::PageDown], |a| a.scroll(view::Scroll::PageDown)),
            (sh, &[Key::PageUp], |a| a.scroll(view::Scroll::PageUp)),
            (sh, &[Key::PageDown], |a| a.scroll(view::Scroll::PageDown)),
            (cs, &[Key::Home], |a| a.scroll(view::Scroll::Top)),
            (cs, &[Key::End], |a| a.scroll(view::Scroll::Bottom)),
            (cs, &[Key::F], |a| a.open_find()),
            (cs, &[Key::K], |a| a.clear_scrollback()),
            (cs, &[Key::A], |a| a.select_all()),
            (cs, &[Key::P], |a| a.open_quick(quick::QuickKind::Commands)),
            (cs, &[Key::M], |a| {
                a.s.show_menubar = !a.s.show_menubar;
                a.s.save();
            }),
            (c, &[Key::Comma], |a| a.prefs_open = !a.prefs_open),
            (cs, &[Key::F11], |a| {
                let full = a.ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                a.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
            }),
        ];
        for (m, keys, act) in table {
            if Self::take(ctx, *m, keys) {
                act(self);
            }
        }
    }

    fn scroll(&mut self, s: view::Scroll) {
        if let Some(x) = self.focused() {
            x.scroll(s);
        }
    }

    // ---------------------------------------------------------------- drawing

    fn tab_label(&self, t: &TermTab) -> String {
        let Some(p) = t.panes.get(t.focus) else { return "Terminal".into() };
        if let Some(title) = p.session.title.as_ref().filter(|s| !s.trim().is_empty()) {
            return title.clone();
        }
        let home = rivet_xdg::places::home();
        let dir = p.session.cwd().map(|d| if d == home { "~".to_string() } else { d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "/".into()) });
        // The folder, and the program running in front when it isn't just the shell waiting.
        let running = if self.start.demo.is_some() { None } else { p.session.busy().then(|| p.session.foreground()).flatten() };
        match (running, dir) {
            (Some(f), Some(d)) => format!("{d} · {f}"),
            (Some(f), None) => f,
            (None, Some(d)) => d,
            _ => "Terminal".into(),
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let mut select = None;
        let mut close = None;
        let mut new = false;
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            if !self.s.show_menubar {
                let btn = egui::Button::new("").min_size(Vec2::new(34.0, 32.0)).frame_when_inactive(false).corner_radius(r_sm());
                let r = egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui| {
                    ui.set_min_width(240.0);
                    if ui.add(egui::Button::new("Show Menu Bar").shortcut_text("Ctrl+Shift+M")).clicked() {
                        self.s.show_menubar = true;
                        self.s.save();
                        ui.close();
                    }
                    ui.separator();
                    self.menus(ui);
                });
                let c = r.0.rect.center();
                for dy in [-4.5f32, 0.0, 4.5] {
                    ui.painter().line_segment([c + Vec2::new(-6.5, dy), c + Vec2::new(6.5, dy)], Stroke::new(1.8, v.text_color()));
                }
                ui.add_space(6.0);
            }
            // The tabs: the open one raised, like Rivet Edit's.
            let accent = self.accent();
            for (i, t) in self.tabs.iter().enumerate() {
                let on = i == self.active;
                let label = if t.label.is_empty() { "Terminal".to_string() } else { t.label.clone() };
                let shown: String = if label.chars().count() > 34 { format!("{}…", label.chars().take(33).collect::<String>()) } else { label.clone() };
                let g = ui.painter().layout_no_wrap(shown, fonts::ui_font(13.0), v.text_color());
                let extra = if t.panes.len() > 1 { 22.0 } else { 0.0 };
                // Roomy tabs: at least 170 points, the name with space either side.
                let w = (g.size().x + 64.0 + extra).max(170.0);
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
                if on {
                    ui.painter().rect_filled(r, r_md(), palette::mix(v.panel_fill, v.text_color(), if v.dark_mode { 0.09 } else { 0.07 }));
                    ui.painter().line_segment([r.left_bottom() + Vec2::new(10.0, -1.0), r.right_bottom() + Vec2::new(-10.0, -1.0)], Stroke::new(2.0, accent));
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, r_md(), palette::mix(v.panel_fill, v.text_color(), 0.05));
                }
                let fg = if on { v.strong_text_color() } else { v.weak_text_color() };
                ui.painter().galley(Pos2::new(r.left() + 16.0, r.center().y - g.size().y / 2.0), g, fg);
                if t.panes.len() > 1 {
                    // How many panes, as a small count.
                    let cx = r.right() - 38.0;
                    ui.painter().text(Pos2::new(cx, r.center().y), Align2::CENTER_CENTER, t.panes.len().to_string(), fonts::ui_font(10.5), v.weak_text_color());
                    ui.painter().rect_stroke(Rect::from_center_size(Pos2::new(cx, r.center().y), Vec2::new(16.0, 14.0)), 3.0, Stroke::new(1.0, v.weak_text_color().gamma_multiply(0.6)), egui::StrokeKind::Inside);
                }
                let xr = Rect::from_center_size(Pos2::new(r.right() - 15.0, r.center().y), Vec2::splat(18.0));
                let over_x = ui.rect_contains_pointer(xr);
                if on || resp.hovered() {
                    if over_x {
                        ui.painter().rect_filled(xr, r_sm(), v.widgets.hovered.weak_bg_fill);
                    }
                    ui.painter().text(xr.center(), Align2::CENTER_CENTER, "×", fonts::ui_font(15.0), fg);
                }
                let resp = resp.on_hover_text(label);
                if resp.clicked() {
                    if over_x { close = Some(i) } else { select = Some(i) }
                }
                if resp.middle_clicked() {
                    close = Some(i);
                }
            }
            // A new tab.
            let (r, resp) = ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
            }
            let c = r.center();
            ui.painter().line_segment([c + Vec2::new(-5.5, 0.0), c + Vec2::new(5.5, 0.0)], Stroke::new(1.6, v.text_color()));
            ui.painter().line_segment([c + Vec2::new(0.0, -5.5), c + Vec2::new(0.0, 5.5)], Stroke::new(1.6, v.text_color()));
            if resp.on_hover_text("New tab (Ctrl+Shift+T)").clicked() {
                new = true;
            }
            // On the right: a new pane, and the layout.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let layout = self.tabs.get(self.active).map(|t| t.layout).unwrap_or_default();
                let (r, resp) = ui.allocate_exact_size(Vec2::new(34.0, 30.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                }
                layout_glyph(ui.painter(), r.center(), layout, v.text_color());
                if resp.on_hover_text(format!("Layout: {} (Ctrl+Shift+L for the next)", layout.name())).clicked() {
                    self.next_layout();
                }
                let (r, resp) = ui.allocate_exact_size(Vec2::new(34.0, 30.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                }
                let b = Rect::from_center_size(r.center(), Vec2::new(16.0, 13.0));
                ui.painter().rect_stroke(b, 2.5, Stroke::new(1.5, v.text_color()), egui::StrokeKind::Inside);
                ui.painter().line_segment([Pos2::new(b.center().x, b.top()), Pos2::new(b.center().x, b.bottom())], Stroke::new(1.5, v.text_color()));
                if resp.on_hover_text("New pane beside this one (Ctrl+Shift+Enter)").clicked() {
                    self.split();
                }
            });
        });
        let _ = &ctx;
        if let Some(i) = select {
            self.active = i;
            self.refocus = true;
        }
        if let Some(i) = close {
            self.close_tab(i, false);
        }
        if new {
            let here = self.here();
            self.new_tab(here);
        }
    }

    pub(crate) fn menus(&mut self, ui: &mut egui::Ui) {
        let item = |ui: &mut egui::Ui, label: &str, keys: &str, on: bool| -> bool {
            let b = egui::Button::new(label);
            let b = if keys.is_empty() { b } else { b.shortcut_text(keys) };
            let r = ui.add_enabled(on, b).clicked();
            if r {
                ui.close();
            }
            r
        };
        let style = |ui: &mut egui::Ui| {
            ui.set_min_width(250.0);
            ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
            ui.spacing_mut().item_spacing.y = 3.0;
        };
        let has_sel = self.focused().and_then(view::selection_text).is_some();
        ui.menu_button("File", |ui| {
            style(ui);
            if item(ui, "New Tab", "Ctrl+Shift+T", true) {
                let h = self.here();
                self.new_tab(h);
            }
            if item(ui, "New Pane", "Ctrl+Shift+Enter", true) {
                self.split();
            }
            if item(ui, "New Window", "Ctrl+Shift+N", self.shot.is_none()) {
                self.new_window();
            }
            ui.separator();
            if item(ui, "Close Pane", "Ctrl+Shift+W", true) {
                let f = self.tabs.get(self.active).map(|t| t.focus).unwrap_or(0);
                self.close_pane(self.active, f, false);
            }
            if item(ui, "Close Tab", "Ctrl+Shift+Q", true) {
                self.close_tab(self.active, false);
            }
            ui.separator();
            if item(ui, "Quit", "", true) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.menu_button("Edit", |ui| {
            style(ui);
            if item(ui, "Copy", "Ctrl+Shift+C", has_sel) {
                self.copy();
            }
            if item(ui, "Paste", "Ctrl+Shift+V", true) {
                self.paste();
            }
            if item(ui, "Select All", "Ctrl+Shift+A", true) {
                self.select_all();
            }
            ui.separator();
            if item(ui, "Find…", "Ctrl+Shift+F", true) {
                self.open_find();
            }
            if item(ui, "Clear Scrollback", "Ctrl+Shift+K", true) {
                self.clear_scrollback();
            }
            ui.separator();
            if item(ui, "Preferences", "Ctrl+,", true) {
                self.prefs_open = true;
            }
        });
        ui.menu_button("View", |ui| {
            style(ui);
            if item(ui, "Next Layout", "Ctrl+Shift+L", true) {
                self.next_layout();
            }
            ui.menu_button("Layout", |ui| {
                style(ui);
                for l in Layout::ALL {
                    if item(ui, l.name(), "", true) {
                        self.set_layout(l);
                    }
                }
            });
            if item(ui, "One Pane at a Time", "Ctrl+Shift+Z", true) {
                self.toggle_zoom();
            }
            if item(ui, "Next Pane", "Ctrl+Shift+]", true) {
                self.focus_pane(1);
            }
            if item(ui, "Next Tab", "Ctrl+Shift+Right", self.tabs.len() > 1) {
                self.go_tab(1);
            }
            ui.separator();
            if item(ui, "Bigger Text", "Ctrl+Shift+=", true) {
                self.zoom += 1.0;
            }
            if item(ui, "Smaller Text", "Ctrl+Shift+-", true) {
                self.zoom -= 1.0;
            }
            if item(ui, "Usual Size", "Ctrl+Shift+Backspace", self.zoom != 0.0) {
                self.zoom = 0.0;
            }
            ui.separator();
            if item(ui, "Color Theme…", "", true) {
                self.open_quick(quick::QuickKind::Themes);
            }
            if item(ui, if self.s.show_menubar { "Hide Menu Bar" } else { "Show Menu Bar" }, "Ctrl+Shift+M", true) {
                self.s.show_menubar = !self.s.show_menubar;
                self.s.save();
            }
            if item(ui, "Full Screen", "Ctrl+Shift+F11", true) {
                let full = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
            }
        });
        ui.menu_button("Help", |ui| {
            style(ui);
            if item(ui, "All Commands", "Ctrl+Shift+P", true) {
                self.open_quick(quick::QuickKind::Commands);
            }
            if item(ui, "Keyboard Shortcuts", "", true) {
                self.prefs_page = prefs::Page::Keyboard;
                self.prefs_open = true;
            }
            if item(ui, "About Rivet Terminal", "", true) {
                self.prefs_page = prefs::Page::About;
                self.prefs_open = true;
            }
        });
    }

    /// The panes of the open tab, each a rounded card in the terminal's colours.
    fn panes(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let colors = self.term_colors(&v);
        let style = rivet_term::Style { font: self.font(), colors, cursor: self.s.cursor, padding: self.s.padding, bold_is_bright: self.s.bold_is_bright, visual_bell: self.s.visual_bell, radius: r_lg() as f32 };
        let area = ui.max_rect();
        let Some(t) = self.tabs.get(self.active) else { return };
        let rects = t.layout.rects(area, t.panes.len(), t.focus, 8.0);
        let focus = t.focus;
        let many = t.panes.len() > 1;
        let accent = self.accent();
        let mut clicked = None;
        let mut open_url = None;
        let mut menu: Option<(egui::Response, usize)> = None;
        let mut copied_on_select = None;
        // The focused pane has the keyboard whenever nothing else wants it (a field, a dialog).
        let idle = self.quick.is_none() && self.prompt.is_none() && ctx.memory(|m| m.focused().is_none());
        let refocus = std::mem::take(&mut self.refocus) || idle;
        let t = &mut self.tabs[self.active];
        for (i, (p, r)) in t.panes.iter_mut().zip(rects).enumerate() {
            let Some(r) = r else { continue };
            let id = egui::Id::unique(("rivet-term-pane", p.id));
            if refocus && i == focus {
                ctx.memory_mut(|m| m.request_focus(id));
            }
            let out = view::show(ui, id, r, &mut p.session, &style);
            if out.clicked {
                clicked = Some(i);
            }
            if out.open_url.is_some() {
                open_url = out.open_url;
            }
            if let Some(resp) = out.response {
                if resp.drag_stopped() {
                    copied_on_select = view::selection_text(&p.session);
                }
                menu = menu.or(Some((resp, i)).filter(|(r, _)| r.secondary_clicked() || egui::Popup::is_id_open(&ctx, egui::Popup::default_response_id(r))));
            }
            // With several panes, the focused one has a quiet accent edge; the others are dimmed.
            if many {
                if i == focus {
                    ui.painter().rect_stroke(r, r_lg(), Stroke::new(1.5, accent.gamma_multiply(0.75)), egui::StrokeKind::Inside);
                } else {
                    ui.painter().rect_filled(r, r_lg(), v.panel_fill.gamma_multiply(0.18));
                }
            }
            if p.session.exited {
                ui.painter().text(r.center_bottom() - Vec2::new(0.0, 14.0), Align2::CENTER_BOTTOM, "The program ended", fonts::ui_font(12.0), v.weak_text_color());
            }
        }
        if let Some(i) = clicked
            && let Some(t) = self.tabs.get_mut(self.active)
        {
            t.focus = i;
        }
        if let Some(url) = open_url
            && self.shot.is_none()
        {
            let _ = rivet_xdg::launch::open_url(&url);
        }
        if let Some(text) = copied_on_select.filter(|_| self.s.copy_on_select && self.shot.is_none()) {
            ctx.copy_text(text);
        }
        if let Some((resp, i)) = menu {
            if let Some(t) = self.tabs.get_mut(self.active) {
                t.focus = i;
            }
            self.pane_menu(&resp);
        }
    }

    /// The right-click menu on a pane.
    fn pane_menu(&mut self, resp: &egui::Response) {
        let has_sel = self.focused().and_then(view::selection_text).is_some();
        resp.context_menu(|ui| {
            ui.set_min_width(230.0);
            ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
            ui.spacing_mut().item_spacing.y = 3.0;
            let item = |ui: &mut egui::Ui, label: &str, keys: &str, on: bool| -> bool {
                let r = ui.add_enabled(on, egui::Button::new(label).shortcut_text(keys)).clicked();
                if r {
                    ui.close();
                }
                r
            };
            if item(ui, "Copy", "Ctrl+Shift+C", has_sel) {
                self.copy();
            }
            if item(ui, "Paste", "Ctrl+Shift+V", true) {
                self.paste();
            }
            if item(ui, "Select All", "Ctrl+Shift+A", true) {
                self.select_all();
            }
            ui.separator();
            if item(ui, "Find…", "Ctrl+Shift+F", true) {
                self.open_find();
            }
            if item(ui, "Clear Scrollback", "Ctrl+Shift+K", true) {
                self.clear_scrollback();
            }
            ui.separator();
            if item(ui, "New Pane", "Ctrl+Shift+Enter", true) {
                self.split();
            }
            if item(ui, "New Tab", "Ctrl+Shift+T", true) {
                let h = self.here();
                self.new_tab(h);
            }
            if item(ui, "Next Layout", "Ctrl+Shift+L", true) {
                self.next_layout();
            }
            ui.separator();
            if item(ui, "Close Pane", "Ctrl+Shift+W", true) {
                let f = self.tabs.get(self.active).map(|t| t.focus).unwrap_or(0);
                self.close_pane(self.active, f, false);
            }
        });
    }

    /// The find bar, over the top right of the focused pane.
    fn find_bar(&mut self, ctx: &egui::Context, area: Rect) {
        let Some(mut f) = self.find.take() else { return };
        let v = ctx.global_style().visuals.clone();
        let mut close = false;
        let mut step: Option<bool> = None;
        egui::Area::new(egui::Id::unique("rivet-term-find")).order(egui::Order::Foreground).fixed_pos(Pos2::new(area.right() - 372.0, area.top() + 10.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let id = egui::Id::unique("rivet-term-find-field");
                    let before = f.query.clone();
                    let r = ui.add(egui::TextEdit::singleline(&mut f.query).id(id).hint_text("Find in the scrollback").desired_width(220.0).margin(egui::Margin::symmetric(6, 3)));
                    if f.focus {
                        r.request_focus();
                        f.focus = false;
                    }
                    if f.query != before {
                        f.last = None;
                        step = Some(true);
                    }
                    let (enter, shift, esc) = ui.input(|i| (i.key_pressed(Key::Enter), i.modifiers.shift, i.key_pressed(Key::Escape)));
                    if r.lost_focus() && enter {
                        step = Some(!shift);
                        r.request_focus();
                    }
                    if esc {
                        close = true;
                    }
                    if f.missed {
                        ui.label(egui::RichText::new("None").small().color(rivet_look::theme::status(ui.ctx()).danger));
                    }
                    if ui.small_button("↑").on_hover_text("Older (Enter)").clicked() {
                        step = Some(true);
                    }
                    if ui.small_button("↓").on_hover_text("Newer (Shift+Enter)").clicked() {
                        step = Some(false);
                    }
                    if ui.small_button("×").on_hover_text("Close (Esc)").clicked() {
                        close = true;
                    }
                    let _ = &v;
                });
            });
        });
        if close {
            if let Some(s) = self.focused() {
                view::clear_selection(s);
            }
            self.refocus = true;
            return;
        }
        self.find = Some(f);
        if let Some(back) = step {
            self.find_step(back);
        }
    }

    fn prompts(&mut self, ctx: &egui::Context) {
        let Some(p) = self.prompt.clone() else { return };
        let (title, sub, yes) = match &p {
            Prompt::ClosePane { what, .. } => (format!("Close this pane while {what} is running?"), "It will be stopped.", "Close Pane"),
            Prompt::CloseTab { what, .. } => (format!("Close this tab while {what} is running?"), "Everything running in its panes will be stopped.", "Close Tab"),
            Prompt::Quit { what } => (format!("Quit while {what} is running?"), "Everything running in the window will be stopped.", "Quit"),
        };
        let mut answer = None;
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(24)).corner_radius(r_lg());
        egui::Modal::new(egui::Id::unique("rivet-term-prompt")).frame(frame).show(ctx, |ui| {
            ui.set_width(420.0);
            let v = ui.visuals().clone();
            ui.label(egui::RichText::new(&title).heading().color(v.strong_text_color()));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(sub).color(v.weak_text_color()));
            ui.add_space(18.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().button_padding = Vec2::new(16.0, 7.0);
                if ui.add(egui::Button::new(egui::RichText::new(yes).color(v.selection.stroke.color).strong()).fill(v.selection.bg_fill).corner_radius(r_sm())).clicked() {
                    answer = Some(true);
                }
                if ui.add(egui::Button::new("Cancel").corner_radius(r_sm())).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                self.prompt = None;
                match p {
                    Prompt::ClosePane { tab, pane, .. } => self.close_pane(tab, pane, true),
                    Prompt::CloseTab { tab, .. } => self.close_tab(tab, true),
                    Prompt::Quit { .. } => {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
            Some(false) => {
                self.prompt = None;
                self.refocus = true;
            }
            None => {}
        }
    }
}

/// A small drawing of a layout, for its button.
fn layout_glyph(p: &egui::Painter, c: Pos2, l: Layout, col: Color32) {
    let b = Rect::from_center_size(c, Vec2::new(16.0, 13.0));
    let s = Stroke::new(1.4, col);
    p.rect_stroke(b, 2.5, s, egui::StrokeKind::Inside);
    let (x, y) = (b.center().x, b.center().y);
    match l {
        Layout::Tall => {
            p.line_segment([Pos2::new(x, b.top()), Pos2::new(x, b.bottom())], s);
            p.line_segment([Pos2::new(x, y), Pos2::new(b.right(), y)], s);
        }
        Layout::Fat => {
            p.line_segment([Pos2::new(b.left(), y), Pos2::new(b.right(), y)], s);
            p.line_segment([Pos2::new(x, y), Pos2::new(x, b.bottom())], s);
        }
        Layout::Grid => {
            p.line_segment([Pos2::new(x, b.top()), Pos2::new(x, b.bottom())], s);
            p.line_segment([Pos2::new(b.left(), y), Pos2::new(b.right(), y)], s);
        }
        Layout::SideBySide => {
            for k in [1.0, 2.0] {
                let xx = b.left() + b.width() * k / 3.0;
                p.line_segment([Pos2::new(xx, b.top()), Pos2::new(xx, b.bottom())], s);
            }
        }
        Layout::Stacked => {
            for k in [1.0, 2.0] {
                let yy = b.top() + b.height() * k / 3.0;
                p.line_segment([Pos2::new(b.left(), yy), Pos2::new(b.right(), yy)], s);
            }
        }
        Layout::One => {}
    }
}

impl eframe::App for TermApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(shot) = &self.shot {
            // Pictures show the window as when it's in use (in front, the cursor solid).
            raw.focused = true;
            rivet_kit::scripted_click("RIVET_TERM", shot.frames(), raw);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let c = self.choice();
        // The terminal font is the monospace family; the interface follows the desktop.
        self.look.set_fonts(&ctx, "", "", &self.s.font);
        self.look.frame(&ctx, &c, self.shot.is_none());
        let kit = self.s.builtin_icons.then(|| {
            let v = ctx.global_style().visuals.clone();
            rivet_look::builtin_icons::BuiltinColors { accent: self.look.accent(&c), bg: v.extreme_bg_color, fg: v.text_color(), dark: v.dark_mode }
        });
        self.icons.set_builtin(kit);
        let now = ctx.input(|i| i.time);
        // What the programs said: titles, bells, the clipboard, ending.
        let v = ctx.global_style().visuals.clone();
        let colors = self.term_colors(&v);
        let mut ended: Vec<(usize, usize)> = Vec::new();
        let mut clip: Option<String> = None;
        for (ti, t) in self.tabs.iter_mut().enumerate() {
            for (pi, p) in t.panes.iter_mut().enumerate() {
                for n in p.session.poll(now, &colors) {
                    match n {
                        Notice::Exited => ended.push((ti, pi)),
                        Notice::Clipboard(text) => clip = Some(text),
                        Notice::Title(_) => self.labels_at = -10.0,
                        Notice::Bell => {}
                    }
                }
            }
        }
        if let Some(text) = clip.filter(|_| self.shot.is_none()) {
            ctx.copy_text(text);
        }
        // A program that ended takes its pane with it (the window, with the last one).
        for (ti, pi) in ended.into_iter().rev() {
            self.close_pane(ti, pi, true);
        }
        // Quitting while something runs: ask first.
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && self.s.confirm_close
            && let Some(p) = self.tabs.iter().flat_map(|t| t.panes.iter()).find(|p| p.session.busy())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            let what = p.session.foreground().unwrap_or_else(|| "a program".into());
            self.prompt = Some(Prompt::Quit { what });
        }
        self.shortcuts(&ctx);
        // Tab labels and the window title, now and then (they read /proc).
        if now - self.labels_at > 1.0 {
            self.labels_at = now;
            let labels: Vec<String> = self.tabs.iter().map(|t| self.tab_label(t)).collect();
            for (t, l) in self.tabs.iter_mut().zip(labels) {
                t.label = l;
            }
            if self.shot.is_none() {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
        }
        let title = match self.tabs.get(self.active) {
            Some(t) if !t.label.is_empty() => format!("{} – Rivet Terminal", t.label),
            _ => "Rivet Terminal".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        // The window: menu bar (optional), the tab row, the panes.
        if self.s.show_menubar {
            let bar = egui::Frame::NONE.fill(v.panel_fill).inner_margin(egui::Margin::symmetric(6, 2));
            egui::Panel::top("term-menubar").show_separator_line(false).frame(bar).show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| self.menus(ui));
            });
        }
        let chrome = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 10, right: 12, top: 6, bottom: 6 });
        egui::Panel::top("term-tabs").show_separator_line(false).frame(chrome).show(ui, |ui| self.toolbar(ui));
        let main = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 12, right: 12, top: 2, bottom: 12 });
        let mut area = Rect::NOTHING;
        egui::CentralPanel::default().frame(main).show(ui, |ui| {
            area = ui.max_rect();
            self.panes(ui);
        });
        // A short note (the layout changed, something went wrong).
        if let Some((text, at)) = &self.note {
            let age = now - at;
            if age < 2.2 {
                let a = (1.0 - ((age - 1.6) / 0.6).clamp(0.0, 1.0)) as f32;
                let g = ctx.fonts_mut(|f| f.layout_no_wrap(text.clone(), fonts::ui_font(12.5), v.strong_text_color()));
                let r = Rect::from_center_size(Pos2::new(area.center().x, area.bottom() - 30.0), g.size() + Vec2::new(28.0, 14.0));
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("rivet-term-note")));
                painter.rect_filled(r, r_md(), palette::mix(v.window_fill, v.text_color(), 0.08).gamma_multiply(a));
                painter.galley(r.center() - g.size() / 2.0, g, v.strong_text_color().gamma_multiply(a));
                ctx.request_repaint();
            } else {
                self.note = None;
            }
        }
        if let (Some(at), Some(shot)) = (self.find_at, &self.shot)
            && shot.frames() >= at
        {
            self.find_at = None;
            self.find_step(true);
        }
        self.find_bar(&ctx, area);
        self.quick_panel(&ctx);
        self.prompts(&ctx);
        if self.prefs_open {
            self.prefs_window(&ctx);
        }
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_place_every_pane() {
        let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 600.0));
        for l in Layout::ALL {
            for n in 1..=5 {
                let r = l.rects(area, n, 0, 8.0);
                assert_eq!(r.len(), n);
                let shown = r.iter().flatten().count();
                assert_eq!(shown, if l == Layout::One { 1 } else { n }, "{l:?} {n}");
                for x in r.iter().flatten() {
                    assert!(area.expand(0.5).contains_rect(*x), "{l:?} {n} {x:?}");
                }
            }
        }
        // Tall: the first on the left half.
        let r = Layout::Tall.rects(area, 3, 0, 8.0);
        assert!(r[0].unwrap().width() < 500.0 && r[1].unwrap().left() > 500.0);
        assert_eq!(Layout::One.next(), Layout::Tall);
    }
}

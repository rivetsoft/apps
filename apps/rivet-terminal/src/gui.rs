//! The Rivet Terminal window: tabs along the top, each holding one or more panes (terminals)
//! arranged by a layout (Tall, Fat, Grid, Side by Side, Stacked, or one at a time), like kitty's
//! windows and layouts. Built on the Rivet apps' shared look: theme and desktop colours, fonts,
//! icons, in-app windows and the quick panel. The terminal itself is `rivet_term`'s.

mod prefs;
mod quick;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Align2, Color32, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::icons::{self, IconTheme};
use rivet_look::live::{Choice, LiveLook};
use rivet_look::theme::{r_lg, r_md, r_sm};
use rivet_look::{fonts, palette, windows};
use rivet_term::{Notice, Session, Size, Spawn, TermColors, view};

use crate::settings::Settings;

pub const ICON_SVG: &str = include_str!("../data/rivet-terminal.svg");

/// What to start with: what a window's tabs and panes run.
#[derive(Clone, Default)]
pub struct Start {
    pub cwd: Option<PathBuf>,
    pub program: Option<(String, Vec<String>)>,
    /// Set for the programs (a window asked for by a later launch gets that launch's).
    pub env: std::collections::HashMap<String, String>,
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
    CloseWindow { what: String },
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

/// One window: its tabs, and what's open over them. Every window is drawn by the same code:
/// the one being drawn is `TermApp::w` while it draws.
pub(crate) struct Win {
    /// Its viewport (the first window is the root).
    vp: egui::ViewportId,
    /// How it was opened (kept the same, so egui doesn't reapply it).
    builder: egui::ViewportBuilder,
    /// What its new tabs and panes run.
    start: Start,
    pub(crate) tabs: Vec<TermTab>,
    pub(crate) active: usize,
    pub(crate) quick: Option<(quick::QuickKind, rivet_look::quick::QuickPanel)>,
    pub(crate) prefs_open: bool,
    prefs_win: windows::WinState,
    pub(crate) prefs_page: prefs::Page,
    pub(crate) note: Option<(String, f64)>,
    title: String,
    pub(crate) find: Option<FindBar>,
    pub(crate) prompt: Option<Prompt>,
    allow_close: bool,
    /// Give the focused pane the keyboard next frame.
    pub(crate) refocus: bool,
    labels_at: f64,
    /// Closed, or emptied by moving its last tab away: dropped after this frame.
    gone: bool,
}

impl Win {
    fn new(vp: egui::ViewportId, builder: egui::ViewportBuilder, start: Start) -> Win {
        Win {
            vp,
            builder,
            start,
            tabs: Vec::new(),
            active: 0,
            quick: None,
            prefs_open: false,
            prefs_win: windows::WinState::new(1, Vec2::new(700.0, 520.0)),
            prefs_page: prefs::Page::default(),
            note: None,
            title: String::new(),
            find: None,
            prompt: None,
            allow_close: false,
            refocus: true,
            labels_at: -10.0,
            gone: false,
        }
    }

    pub(crate) fn panes(&self) -> usize {
        self.tabs.iter().map(|t| t.panes.len()).sum()
    }

    /// Take a pane out (its program keeps running), closing its tab when it was the last.
    fn remove_pane(&mut self, tab: usize, pane: usize) -> Option<Pane> {
        let t = self.tabs.get_mut(tab)?;
        if pane >= t.panes.len() {
            return None;
        }
        let p = t.panes.remove(pane);
        if t.panes.is_empty() {
            self.tabs.remove(tab);
            if self.active >= tab && self.active > 0 {
                self.active -= 1;
            }
        } else if t.focus >= t.panes.len() || t.focus > pane {
            t.focus = t.focus.saturating_sub(1).min(t.panes.len() - 1);
        }
        self.after_removal();
        Some(p)
    }

    /// Take a tab out (its programs keep running).
    fn remove_tab(&mut self, tab: usize) -> Option<TermTab> {
        if tab >= self.tabs.len() {
            return None;
        }
        let t = self.tabs.remove(tab);
        if self.active > tab || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        self.after_removal();
        Some(t)
    }

    fn after_removal(&mut self) {
        self.refocus = true;
        self.labels_at = -10.0;
        if self.tabs.is_empty() {
            self.allow_close = true;
            self.gone = true;
        }
    }

    /// Where pane `id` is: its tab and place.
    fn find_pane(&self, id: u64) -> Option<(usize, usize)> {
        self.tabs.iter().enumerate().find_map(|(ti, t)| t.panes.iter().position(|p| p.id == id).map(|pi| (ti, pi)))
    }

    /// A short name for menus: its open tab.
    fn name(&self) -> String {
        match self.tabs.get(self.active) {
            Some(t) if !t.label.is_empty() => t.label.clone(),
            _ => "Terminal".into(),
        }
    }
}

/// A pane or tab dragged out of its window and let go, waiting to see where it landed.
#[derive(Clone, Copy, Debug)]
struct Flight {
    from: egui::ViewportId,
    /// The pane, or the tab holding it.
    pane: u64,
    whole_tab: bool,
    at: f64,
    /// The window it came from, its size: a new window gets the same.
    size: Vec2,
}

/// Where a pane or tab is moved: a new window, or one that's open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Dest {
    NewWindow,
    Window(egui::ViewportId),
}

pub struct TermApp {
    pub(crate) s: Settings,
    look: LiveLook,
    icons: IconTheme,
    app_icon: Option<egui::TextureHandle>,
    pub(crate) families: Option<Vec<String>>,
    /// The window being drawn (the root window outside of drawing).
    pub(crate) w: Win,
    /// The other windows.
    others: Vec<Win>,
    /// Windows to open at the start of the next frame.
    pending: Vec<Win>,
    next_id: u64,
    /// Checks: the demo project and script (instead of the person's shell).
    demo: Option<(PathBuf, PathBuf)>,
    pub(crate) scheme_preview: Option<String>,
    pub(crate) shot: Option<rivet_kit::Shot>,
    /// Ctrl+Shift+= / - changes, on top of the chosen size.
    pub(crate) zoom: f32,
    /// Checks: search with the find bar's text at this frame (after the demo has drawn).
    find_at: Option<u32>,
    /// Windows asked for by later launches.
    inbox: Option<std::sync::mpsc::Receiver<Start>>,
    flight: Option<Flight>,
    /// Quit was chosen: every window closes.
    quitting: bool,
    ctx: egui::Context,
}

impl TermApp {
    pub fn new(cc: &eframe::CreationContext, mut start: Start, screenshot: Option<PathBuf>, inbox: Option<std::os::unix::net::UnixListener>) -> TermApp {
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
        let demo = start.demo.take();
        let mut root = Win::new(egui::ViewportId::ROOT, egui::ViewportBuilder::default(), start);
        root.prefs_open = std::env::var_os("RIVET_TERM_PREFS").is_some();
        root.prefs_page = prefs_page;
        let inbox = inbox.map(|l| {
            let (tx, rx) = std::sync::mpsc::channel();
            crate::instance::serve(l, tx, cc.egui_ctx.clone());
            rx
        });
        let mut app = TermApp {
            s,
            look: LiveLook::new(),
            icons: IconTheme::new(&icons::detect_system_theme()),
            app_icon: None,
            families: None,
            w: root,
            others: Vec::new(),
            pending: Vec::new(),
            next_id: 1,
            demo,
            scheme_preview: None,
            shot: screenshot.map(rivet_kit::Shot::new),
            zoom: 0.0,
            find_at: None,
            inbox,
            flight: None,
            quitting: false,
            ctx: cc.egui_ctx.clone(),
        };
        // Checks: RIVET_TERM_TABS=n tabs, RIVET_TERM_SPLIT=n panes in the first, RIVET_TERM_LAYOUT.
        let tabs: usize = std::env::var("RIVET_TERM_TABS").ok().and_then(|v| v.parse().ok()).unwrap_or(1).clamp(1, 8);
        for _ in 0..tabs {
            app.new_tab(None);
        }
        app.w.active = 0;
        let split: usize = std::env::var("RIVET_TERM_SPLIT").ok().and_then(|v| v.parse().ok()).unwrap_or(1).clamp(1, 6);
        for _ in 1..split {
            app.split();
        }
        if let Ok(l) = std::env::var("RIVET_TERM_LAYOUT")
            && let Some(t) = app.w.tabs.get_mut(0)
        {
            t.layout = Layout::ALL.into_iter().find(|x| x.name().eq_ignore_ascii_case(&l)).unwrap_or_default();
        }
        if let Some(t) = app.w.tabs.get_mut(0) {
            t.focus = 0;
        }
        if let Ok(q) = std::env::var("RIVET_TERM_FIND") {
            app.w.find = Some(FindBar { query: q, focus: true, ..Default::default() });
            app.find_at = Some(30);
        }
        if let Ok(q) = std::env::var("RIVET_TERM_QUICK") {
            app.open_quick(if q == "themes" { quick::QuickKind::Themes } else { quick::QuickKind::Commands });
        }
        // Checks: RIVET_TERM_WINDOWS=n more windows (drawn inside the first in a screenshot).
        let more: usize = std::env::var("RIVET_TERM_WINDOWS").ok().and_then(|v| v.parse().ok()).unwrap_or(0).min(3);
        for _ in 0..more {
            app.new_window();
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
        self.w.note = Some((text.into(), now));
    }

    // ---------------------------------------------------------------- tabs and panes

    fn spawn(&mut self, cwd: Option<PathBuf>) -> Option<Pane> {
        let index = self.w.panes() + self.others.iter().map(Win::panes).sum::<usize>() + 1;
        let spec = match &self.demo {
            // Checks: the demo script, a different scene per pane.
            Some((root, script)) => Spawn { program: Some(("sh".into(), vec![script.display().to_string(), index.min(3).to_string()])), cwd: Some(root.clone()), env: Default::default(), scrollback: self.s.scrollback },
            None => {
                let program = self.w.start.program.clone().or_else(|| {
                    let mut words = self.s.shell.split_whitespace().map(String::from);
                    words.next().map(|p| (p, words.collect()))
                });
                let cwd = cwd.or_else(|| self.w.start.cwd.clone()).filter(|d| d.is_dir()).or_else(|| Some(rivet_xdg::places::home()));
                Spawn { program, cwd, env: self.w.start.env.clone(), scrollback: self.s.scrollback }
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
        let t = self.w.tabs.get(self.w.active)?;
        t.panes.get(t.focus)?.session.cwd()
    }

    pub(crate) fn new_tab(&mut self, cwd: Option<PathBuf>) {
        if let Some(p) = self.spawn(cwd) {
            self.w.tabs.push(TermTab { panes: vec![p], focus: 0, layout: Layout::default(), label: String::new() });
            self.w.active = self.w.tabs.len() - 1;
            self.w.labels_at = -10.0;
            self.w.refocus = true;
        }
    }

    /// A new pane in this tab, next to the focused one.
    pub(crate) fn split(&mut self) {
        let here = self.here();
        if self.w.tabs.is_empty() {
            self.new_tab(here);
            return;
        }
        if let Some(p) = self.spawn(here)
            && let Some(t) = self.w.tabs.get_mut(self.w.active)
        {
            let at = (t.focus + 1).min(t.panes.len());
            t.panes.insert(at, p);
            t.focus = at;
            self.w.refocus = true;
        }
    }

    /// Close a pane (asking first when something runs in it).
    pub(crate) fn close_pane(&mut self, tab: usize, pane: usize, force: bool) {
        let Some(t) = self.w.tabs.get(tab) else { return };
        let Some(p) = t.panes.get(pane) else { return };
        if !force && self.s.confirm_close && p.session.busy() {
            let what = p.session.foreground().unwrap_or_else(|| "A program".into());
            self.w.prompt = Some(Prompt::ClosePane { tab, pane, what });
            return;
        }
        drop(self.w.remove_pane(tab, pane));
    }

    pub(crate) fn close_tab(&mut self, tab: usize, force: bool) {
        let Some(t) = self.w.tabs.get(tab) else { return };
        if !force && self.s.confirm_close
            && let Some(p) = t.panes.iter().find(|p| p.session.busy())
        {
            let what = p.session.foreground().unwrap_or_else(|| "A program".into());
            self.w.prompt = Some(Prompt::CloseTab { tab, what });
            return;
        }
        drop(self.w.remove_tab(tab));
    }

    pub(crate) fn focus_pane(&mut self, step: i32) {
        if let Some(t) = self.w.tabs.get_mut(self.w.active) {
            let n = t.panes.len() as i32;
            t.focus = ((t.focus as i32 + step).rem_euclid(n.max(1))) as usize;
            self.w.refocus = true;
        }
    }

    pub(crate) fn go_tab(&mut self, step: i32) {
        let n = self.w.tabs.len() as i32;
        if n > 0 {
            self.w.active = ((self.w.active as i32 + step).rem_euclid(n)) as usize;
            self.w.refocus = true;
        }
    }

    pub(crate) fn move_tab(&mut self, step: i32) {
        let n = self.w.tabs.len() as i32;
        let to = self.w.active as i32 + step;
        if n > 1 && (0..n).contains(&to) {
            self.w.tabs.swap(self.w.active, to as usize);
            self.w.active = to as usize;
        }
    }

    pub(crate) fn next_layout(&mut self) {
        if let Some(t) = self.w.tabs.get_mut(self.w.active) {
            t.layout = t.layout.next();
            let name = t.layout.name();
            self.flash(format!("Layout: {name}"));
        }
    }

    pub(crate) fn set_layout(&mut self, l: Layout) {
        if let Some(t) = self.w.tabs.get_mut(self.w.active) {
            t.layout = l;
        }
    }

    /// One pane at a time, or back to how it was.
    pub(crate) fn toggle_zoom(&mut self) {
        if let Some(t) = self.w.tabs.get_mut(self.w.active) {
            t.layout = if t.layout == Layout::One { Layout::Tall } else { Layout::One };
        }
    }

    pub(crate) fn focused(&self) -> Option<&Session> {
        let t = self.w.tabs.get(self.w.active)?;
        t.panes.get(t.focus).map(|p| &p.session)
    }

    /// A new window, with a tab in this folder.
    pub(crate) fn new_window(&mut self) {
        let start = Start { cwd: self.here(), ..Default::default() };
        let size = self.ctx.input(|i| i.viewport().inner_rect.map(|r| r.size())).unwrap_or(Vec2::new(960.0, 620.0));
        self.open_window(start, Vec::new(), size);
    }

    /// Open a window running `start`, holding `tabs` (a new tab when there are none).
    fn open_window(&mut self, start: Start, tabs: Vec<TermTab>, size: Vec2) {
        let id = self.next_id;
        self.next_id += 1;
        let builder = egui::ViewportBuilder::default().with_title("Rivet Terminal").with_app_id("rivet-terminal").with_inner_size(size).with_min_inner_size([420.0, 260.0]);
        let mut win = Win::new(egui::ViewportId::from_hash_of(("rivet-term-window", id)), builder, start);
        win.tabs = tabs;
        std::mem::swap(&mut self.w, &mut win);
        if self.w.tabs.is_empty() {
            self.new_tab(None);
        }
        self.w.active = self.w.tabs.len().saturating_sub(1);
        std::mem::swap(&mut self.w, &mut win);
        self.pending.push(win);
        self.ctx.request_repaint_of(egui::ViewportId::ROOT);
    }

    /// The other windows, for a menu: each one's viewport and name.
    pub(crate) fn window_names(&self) -> Vec<(egui::ViewportId, String)> {
        self.others.iter().filter(|w| !w.gone).map(|w| (w.vp, w.name())).collect()
    }

    /// Every window, the one being drawn first.
    fn wins_mut(&mut self) -> impl Iterator<Item = &mut Win> {
        std::iter::once(&mut self.w).chain(self.others.iter_mut())
    }

    /// Move pane `id` (from whichever window has it) to `dest`. Its program keeps running.
    fn carry_pane(&mut self, id: u64, dest: Dest, size: Vec2) {
        if let Dest::Window(vp) = dest
            && self.wins_mut().any(|w| w.vp == vp && w.find_pane(id).is_some())
        {
            return; // already there
        }
        let Some(p) = self.wins_mut().find_map(|w| w.find_pane(id).and_then(|(t, i)| w.remove_pane(t, i))) else { return };
        match dest {
            Dest::Window(vp) => {
                let ctx = self.ctx.clone();
                if let Some(w) = self.wins_mut().find(|w| w.vp == vp && !w.gone) {
                    match w.tabs.get_mut(w.active) {
                        Some(t) => {
                            let at = (t.focus + 1).min(t.panes.len());
                            t.panes.insert(at, p);
                            t.focus = at;
                        }
                        None => w.tabs.push(TermTab { panes: vec![p], focus: 0, layout: Layout::default(), label: String::new() }),
                    }
                    w.refocus = true;
                    w.labels_at = -10.0;
                    ctx.request_repaint_of(vp);
                    return;
                }
                // The window went away meanwhile: a new one.
                self.open_window(Start::default(), vec![TermTab { panes: vec![p], focus: 0, layout: Layout::default(), label: String::new() }], size);
            }
            Dest::NewWindow => self.open_window(Start::default(), vec![TermTab { panes: vec![p], focus: 0, layout: Layout::default(), label: String::new() }], size),
        }
    }

    /// Move the tab holding pane `id` to `dest`, with all its panes.
    fn carry_tab(&mut self, id: u64, dest: Dest, size: Vec2) {
        if let Dest::Window(vp) = dest
            && self.wins_mut().any(|w| w.vp == vp && w.find_pane(id).is_some())
        {
            return;
        }
        let Some(t) = self.wins_mut().find_map(|w| w.find_pane(id).and_then(|(t, _)| w.remove_tab(t))) else { return };
        let ctx = self.ctx.clone();
        if let Dest::Window(vp) = dest
            && let Some(w) = self.wins_mut().find(|w| w.vp == vp && !w.gone)
        {
            w.tabs.push(t);
            w.active = w.tabs.len() - 1;
            w.refocus = true;
            w.labels_at = -10.0;
            ctx.request_repaint_of(vp);
            return;
        }
        self.open_window(Start::default(), vec![t], size);
    }

    /// The size of the window being drawn.
    fn win_size(&self) -> Vec2 {
        self.ctx.input(|i| i.viewport().inner_rect.map(|r| r.size())).unwrap_or(Vec2::new(960.0, 620.0))
    }

    /// Move the focused pane to `dest` (Detach Pane, Move Pane To).
    pub(crate) fn send_pane(&mut self, dest: Dest) {
        if let Some(id) = self.w.tabs.get(self.w.active).and_then(|t| t.panes.get(t.focus)).map(|p| p.id) {
            let size = self.win_size();
            self.carry_pane(id, dest, size);
        }
    }

    /// Move tab `tab` of this window to `dest`.
    pub(crate) fn send_tab(&mut self, tab: usize, dest: Dest) {
        if let Some(id) = self.w.tabs.get(tab).and_then(|t| t.panes.first()).map(|p| p.id) {
            let size = self.win_size();
            self.carry_tab(id, dest, size);
        }
    }

    /// Whether the focused pane can go elsewhere: there's something left behind, or a window to
    /// join.
    pub(crate) fn can_send_pane(&self) -> bool {
        self.w.panes() > 1 || !self.others.is_empty()
    }

    /// Quit: close every window (asking first when something runs in one).
    pub(crate) fn quit(&mut self, force: bool) {
        if !force && self.s.confirm_close {
            let busy = std::iter::once(&self.w).chain(self.others.iter()).flat_map(|w| w.tabs.iter()).flat_map(|t| t.panes.iter()).find(|p| p.session.busy());
            if let Some(p) = busy {
                let what = p.session.foreground().unwrap_or_else(|| "a program".into());
                self.w.prompt = Some(Prompt::Quit { what });
                return;
            }
        }
        self.quitting = true;
        self.ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
        self.ctx.request_repaint_of(egui::ViewportId::ROOT);
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
        let mut f = self.w.find.take().unwrap_or_default();
        f.focus = true;
        self.w.find = Some(f);
    }

    fn find_step(&mut self, back: bool) {
        let Some(mut f) = self.w.find.take() else { return };
        if let Some(s) = self.focused() {
            let found = view::find(s, &f.query, back, f.last).or_else(|| view::find(s, &f.query, back, None));
            f.missed = found.is_none() && !f.query.is_empty();
            f.last = found;
        }
        self.w.find = Some(f);
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
        if self.w.quick.is_some() || self.w.prompt.is_some() {
            return;
        }
        let cs = Modifiers::CTRL | Modifiers::SHIFT;
        let c = Modifiers::CTRL;
        let sh = Modifiers::SHIFT;
        type Act = fn(&mut TermApp);
        let table: &[(Modifiers, &[Key], Act)] = &[
            (cs, &[Key::T], |a| a.new_tab(a.here())),
            (cs, &[Key::Q], |a| a.close_tab(a.w.active, false)),
            (cs, &[Key::N], |a| a.new_window()),
            (cs, &[Key::Enter], |a| a.split()),
            (cs, &[Key::W], |a| a.close_pane(a.w.active, a.w.tabs.get(a.w.active).map(|t| t.focus).unwrap_or(0), false)),
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
            (c, &[Key::Comma], |a| a.w.prefs_open = !a.w.prefs_open),
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
        match t.panes.get(t.focus) {
            Some(p) => self.pane_label(p),
            None => "Terminal".into(),
        }
    }

    /// What a pane is showing: its program's title, else its folder and what runs in front.
    fn pane_label(&self, p: &Pane) -> String {
        if let Some(title) = p.session.title.as_ref().filter(|s| !s.trim().is_empty()) {
            return title.clone();
        }
        let home = rivet_xdg::places::home();
        let dir = p.session.cwd().map(|d| if d == home { "~".to_string() } else { d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "/".into()) });
        // The folder, and the program running in front when it isn't just the shell waiting.
        let running = if self.demo.is_some() { None } else { p.session.busy().then(|| p.session.foreground()).flatten() };
        match (running, dir) {
            (Some(f), Some(d)) => format!("{d} · {f}"),
            (Some(f), None) => f,
            (None, Some(d)) => d,
            _ => "Terminal".into(),
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
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
            // The tabs: the shared strip (Rivet Edit's and Rivet Files' too). Drag one to move it.
            let accent = self.accent();
            let tabs: Vec<rivet_look::tabs::Tab> = self
                .w
                .tabs
                .iter()
                .map(|t| {
                    let label = if t.label.is_empty() { "Terminal".to_string() } else { t.label.clone() };
                    rivet_look::tabs::Tab { tip: Some(label.clone()), label, badge: (t.panes.len() > 1).then(|| t.panes.len().to_string()), ..Default::default() }
                })
                .collect();
            // Room after it for + and, on the right, the pane and layout buttons.
            // A tab pulled down out of the strip and let go outside the window moves to the
            // window under the pointer, or to a new one.
            let out = rivet_look::tabs::TabStrip::new("term-tabs", accent).salt(self.w.vp).tear_off(true).widths(170.0, 300.0).reserve(120.0).show(ui, &tabs, self.w.active);
            for (i, resp) in out.responses.iter().enumerate() {
                self.tab_menu(resp, i);
            }
            if let Some((i, pos)) = out.dropped
                && !ui.ctx().content_rect().contains(pos)
                && let Some(id) = self.w.tabs.get(i).and_then(|t| t.panes.first()).map(|p| p.id)
            {
                self.flight = Some(Flight { from: self.w.vp, pane: id, whole_tab: true, at: ui.input(|i| i.time), size: self.win_size() });
            }
            for e in &out.events {
                match *e {
                    rivet_look::tabs::TabEvent::Swap(a, b) => self.w.tabs.swap(a, b),
                    rivet_look::tabs::TabEvent::Close(i) => close = Some(i),
                    rivet_look::tabs::TabEvent::Select(_) | rivet_look::tabs::TabEvent::Keep(_) => {}
                }
            }
            let now = out.active(self.w.active);
            if now != self.w.active {
                select = Some(now);
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
                let layout = self.w.tabs.get(self.w.active).map(|t| t.layout).unwrap_or_default();
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
        if let Some(i) = select {
            self.w.active = i;
            self.w.refocus = true;
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
            if item(ui, "New Window", "Ctrl+Shift+N", true) {
                self.new_window();
            }
            ui.separator();
            self.move_items(ui);
            ui.separator();
            if item(ui, "Close Pane", "Ctrl+Shift+W", true) {
                let f = self.w.tabs.get(self.w.active).map(|t| t.focus).unwrap_or(0);
                self.close_pane(self.w.active, f, false);
            }
            if item(ui, "Close Tab", "Ctrl+Shift+Q", true) {
                self.close_tab(self.w.active, false);
            }
            if item(ui, "Close Window", "", true) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ui.separator();
            if item(ui, "Quit", "", true) {
                self.quit(false);
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
                self.w.prefs_open = true;
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
            if item(ui, "Next Tab", "Ctrl+Shift+Right", self.w.tabs.len() > 1) {
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
                self.w.prefs_page = prefs::Page::Keyboard;
                self.w.prefs_open = true;
            }
            if item(ui, "About Rivet Terminal", "", true) {
                self.w.prefs_page = prefs::Page::About;
                self.w.prefs_open = true;
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
        let Some(t) = self.w.tabs.get(self.w.active) else { return };
        let rects = t.layout.rects(area, t.panes.len(), t.focus, 8.0);
        let focus = t.focus;
        let many = t.panes.len() > 1;
        // A pane can be dragged by its grip when there's somewhere for it to go.
        let grips = self.can_send_pane();
        let hover = ctx.input(|i| i.pointer.hover_pos());
        let mut held: Option<(usize, egui::Response)> = None;
        let shown: Vec<(usize, Rect)> = rects.iter().enumerate().filter_map(|(i, r)| r.map(|r| (i, r))).collect();
        let accent = self.accent();
        let mut clicked = None;
        let mut open_url = None;
        let mut menu: Option<(egui::Response, usize)> = None;
        let mut copied_on_select = None;
        // The focused pane has the keyboard whenever nothing else wants it (a field, a dialog).
        let idle = self.w.quick.is_none() && self.w.prompt.is_none() && ctx.memory(|m| m.focused().is_none());
        let refocus = std::mem::take(&mut self.w.refocus) || idle;
        let t = &mut self.w.tabs[self.w.active];
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
            // The grip: a small bar at the top middle, shown while the pointer is on the pane.
            if grips {
                let g = Rect::from_center_size(r.center_top() + Vec2::new(0.0, 7.0), Vec2::new(48.0, 14.0));
                let resp = ui.interact(g, egui::Id::unique(("rivet-term-grip", p.id)), Sense::drag());
                let dragging = resp.dragged();
                if dragging || hover.is_some_and(|h| r.contains(h)) {
                    let strong = resp.hovered() || dragging;
                    let bar = Rect::from_center_size(g.center(), Vec2::new(if strong { 32.0 } else { 26.0 }, 4.0));
                    ui.painter().rect_filled(bar, 2.0, if strong { accent } else { v.weak_text_color().gamma_multiply(0.7) });
                }
                if resp.hovered() && !dragging {
                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                }
                let resp = resp.on_hover_text("Drag onto another pane to swap them, or out of the window to move it");
                if resp.dragged() || resp.drag_stopped() {
                    held = Some((i, resp));
                }
            }
        }
        if let Some((i, resp)) = held {
            self.drag_pane(ui, i, &resp, &shown);
        }
        if let Some(i) = clicked
            && let Some(t) = self.w.tabs.get_mut(self.w.active)
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
            if let Some(t) = self.w.tabs.get_mut(self.w.active) {
                t.focus = i;
            }
            self.pane_menu(&resp);
        }
    }

    /// A pane dragged by its grip: a card follows the pointer, the pane under it is marked, and
    /// on release the two swap. Let go outside the window, it moves to the window there.
    fn drag_pane(&mut self, ui: &egui::Ui, i: usize, resp: &egui::Response, shown: &[(usize, Rect)]) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let accent = self.accent();
        let Some(pos) = ctx.input(|inp| inp.pointer.interact_pos().or(inp.pointer.latest_pos())) else { return };
        let inside = ctx.content_rect().contains(pos);
        let under = shown.iter().find(|(j, r)| *j != i && r.contains(pos)).map(|x| x.0);
        if resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            if let Some(j) = under.and_then(|j| shown.iter().find(|x| x.0 == j)) {
                ui.painter().rect_filled(j.1, r_lg(), accent.gamma_multiply(0.12));
                ui.painter().rect_stroke(j.1, r_lg(), Stroke::new(2.0, accent), egui::StrokeKind::Inside);
            }
            let label = self.w.tabs.get(self.w.active).and_then(|t| t.panes.get(i)).map(|p| self.pane_label(p)).unwrap_or_else(|| "Terminal".into());
            let card = Rect::from_center_size(pos + Vec2::new(0.0, 34.0), Vec2::new(180.0, 52.0));
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique(("rivet-term-pane-drag", self.w.vp))));
            let shadow = egui::epaint::Shadow { offset: [0, 5], blur: 16, spread: 0, color: Color32::from_black_alpha(70) };
            p.add(shadow.as_shape(card, r_md()));
            p.rect_filled(card, r_md(), v.extreme_bg_color);
            p.rect_stroke(card, r_md(), Stroke::new(1.5, accent), egui::StrokeKind::Inside);
            p.text(card.center(), Align2::CENTER_CENTER, label, fonts::ui_font(13.0), v.strong_text_color());
            ctx.request_repaint();
            return;
        }
        // Let go.
        let Some(t) = self.w.tabs.get_mut(self.w.active) else { return };
        let Some(id) = t.panes.get(i).map(|p| p.id) else { return };
        if !inside {
            self.flight = Some(Flight { from: self.w.vp, pane: id, whole_tab: false, at: ctx.input(|inp| inp.time), size: self.win_size() });
            ctx.request_repaint();
        } else if let Some(j) = under {
            t.panes.swap(i, j);
            t.focus = j;
            self.w.refocus = true;
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
            self.move_items(ui);
            ui.separator();
            if item(ui, "Close Pane", "Ctrl+Shift+W", true) {
                let f = self.w.tabs.get(self.w.active).map(|t| t.focus).unwrap_or(0);
                self.close_pane(self.w.active, f, false);
            }
        });
    }

    /// Detach Pane and Move Pane To, for the File menu and a pane's own menu.
    fn move_items(&mut self, ui: &mut egui::Ui) {
        let others = self.window_names();
        let detach = ui.add_enabled(self.w.panes() > 1, egui::Button::new("Detach Pane")).on_disabled_hover_text("This is the window's only pane");
        if detach.on_hover_text("Open this pane in a window of its own").clicked() {
            self.send_pane(Dest::NewWindow);
            ui.close();
        }
        if !others.is_empty() {
            ui.menu_button("Move Pane To", |ui| {
                ui.set_min_width(200.0);
                for (vp, name) in others {
                    if ui.button(name).clicked() {
                        self.send_pane(Dest::Window(vp));
                        ui.close();
                    }
                }
            });
        }
    }

    /// The right-click menu on a tab.
    fn tab_menu(&mut self, resp: &egui::Response, tab: usize) {
        resp.context_menu(|ui| {
            ui.set_min_width(200.0);
            ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
            ui.spacing_mut().item_spacing.y = 3.0;
            if ui.add_enabled(self.w.tabs.len() > 1, egui::Button::new("Detach Tab")).on_disabled_hover_text("This is the window's only tab").clicked() {
                self.send_tab(tab, Dest::NewWindow);
                ui.close();
            }
            let others = self.window_names();
            if !others.is_empty() {
                ui.menu_button("Move Tab To", |ui| {
                    ui.set_min_width(200.0);
                    for (vp, name) in others {
                        if ui.button(name).clicked() {
                            self.send_tab(tab, Dest::Window(vp));
                            ui.close();
                        }
                    }
                });
            }
            ui.separator();
            if ui.button("Close Tab").clicked() {
                self.close_tab(tab, false);
                ui.close();
            }
        });
    }

    /// The find bar, over the top right of the focused pane.
    fn find_bar(&mut self, ctx: &egui::Context, area: Rect) {
        let Some(mut f) = self.w.find.take() else { return };
        let v = ctx.global_style().visuals.clone();
        let mut close = false;
        let mut step: Option<bool> = None;
        egui::Area::new(egui::Id::unique(("rivet-term-find", self.w.vp))).order(egui::Order::Foreground).fixed_pos(Pos2::new(area.right() - 372.0, area.top() + 10.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let id = egui::Id::unique(("rivet-term-find-field", self.w.vp));
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
            self.w.refocus = true;
            return;
        }
        self.w.find = Some(f);
        if let Some(back) = step {
            self.find_step(back);
        }
    }

    fn prompts(&mut self, ctx: &egui::Context) {
        let Some(p) = self.w.prompt.clone() else { return };
        let (title, sub, yes) = match &p {
            Prompt::ClosePane { what, .. } => (format!("Close this pane while {what} is running?"), "It will be stopped.", "Close Pane"),
            Prompt::CloseTab { what, .. } => (format!("Close this tab while {what} is running?"), "Everything running in its panes will be stopped.", "Close Tab"),
            Prompt::CloseWindow { what } => (format!("Close this window while {what} is running?"), "Everything running in it will be stopped.", "Close Window"),
            Prompt::Quit { what } if self.others.is_empty() => (format!("Quit while {what} is running?"), "Everything running in the window will be stopped.", "Quit"),
            Prompt::Quit { what } => (format!("Quit while {what} is running?"), "Everything running in every window will be stopped.", "Quit"),
        };
        // Return is the highlighted button, as Escape is Cancel, unless Tab has put the keyboard
        // on Cancel. The key is taken so nothing behind the question sees it.
        let mut answer = (rivet_look::widgets::return_default(ctx) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter))).then_some(true);
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(24)).corner_radius(r_lg());
        egui::Modal::new(egui::Id::unique(("rivet-term-prompt", self.w.vp))).frame(frame).show(ctx, |ui| {
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
                self.w.prompt = None;
                match p {
                    Prompt::ClosePane { tab, pane, .. } => self.close_pane(tab, pane, true),
                    Prompt::CloseTab { tab, .. } => self.close_tab(tab, true),
                    Prompt::CloseWindow { .. } => {
                        self.w.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Prompt::Quit { .. } => self.quit(true),
                }
            }
            Some(false) => {
                self.w.prompt = None;
                self.w.refocus = true;
            }
            None => {}
        }
    }

    /// Draw the window in `self.w`, in the viewport being drawn.
    fn window(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        // What the programs said: titles, bells, the clipboard, ending.
        let v = ctx.global_style().visuals.clone();
        let colors = self.term_colors(&v);
        let mut ended: Vec<(usize, usize)> = Vec::new();
        let mut clip: Option<String> = None;
        let vp = self.w.vp;
        for (ti, t) in self.w.tabs.iter_mut().enumerate() {
            for (pi, p) in t.panes.iter_mut().enumerate() {
                p.session.show_in(vp);
                for n in p.session.poll(now, &colors) {
                    match n {
                        Notice::Exited => ended.push((ti, pi)),
                        Notice::Clipboard(text) => clip = Some(text),
                        Notice::Title(_) => self.w.labels_at = -10.0,
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
        // Closing the window while something runs: ask first. The first window can't close
        // while others are open (that ends the program): it takes over another's tabs instead.
        let root = self.w.vp == egui::ViewportId::ROOT;
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            let busy = self.w.tabs.iter().flat_map(|t| t.panes.iter()).find(|p| p.session.busy());
            if let (false, true, Some(p)) = (self.w.allow_close, self.s.confirm_close, busy) {
                let what = p.session.foreground().unwrap_or_else(|| "a program".into());
                self.w.prompt = Some(if self.others.is_empty() { Prompt::Quit { what } } else { Prompt::CloseWindow { what } });
                if root {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
            } else {
                self.w.gone = true;
                if root && !self.others.is_empty() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
            }
        }
        self.shortcuts(&ctx);
        // Tab labels and the window title, now and then (they read /proc).
        if now - self.w.labels_at > 1.0 {
            self.w.labels_at = now;
            let labels: Vec<String> = self.w.tabs.iter().map(|t| self.tab_label(t)).collect();
            for (t, l) in self.w.tabs.iter_mut().zip(labels) {
                t.label = l;
            }
            if self.shot.is_none() {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
        }
        let title = match self.w.tabs.get(self.w.active) {
            Some(t) if !t.label.is_empty() => format!("{} – Rivet Terminal", t.label),
            _ => "Rivet Terminal".into(),
        };
        if title != self.w.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.w.title = title;
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
        if let Some((text, at)) = &self.w.note {
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
                self.w.note = None;
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
        if self.w.prefs_open {
            self.prefs_window(&ctx);
        }
    }

    /// A pane or tab let go outside its window: it goes to the window the pointer is over now,
    /// else to a new window. Windows can't see where the others are, so the one it landed on is
    /// the one that sees the pointer come in; some compositors say so only once the pointer
    /// moves again after the button is up, so this waits a moment for that.
    fn land(&mut self, ctx: &egui::Context) {
        let Some(f) = self.flight else { return };
        let now = ctx.input(|i| i.time);
        let target = std::iter::once(&self.w).chain(self.others.iter()).filter(|w| w.vp != f.from && !w.gone).map(|w| w.vp).find(|vp| ctx.input_for(*vp, |i| i.pointer.hover_pos().is_some()));
        // The window it came from still has the pointer until the compositor looks again.
        let source_has_it = ctx.input_for(f.from, |i| i.pointer.hover_pos().is_some());
        let wait = if source_has_it { 1.5 } else { 0.4 };
        let dest = match target {
            Some(vp) => Dest::Window(vp),
            None if now - f.at < wait => {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
                return;
            }
            None => Dest::NewWindow,
        };
        self.flight = None;
        // Nowhere else to go, and nothing would be left behind: it stays.
        let from_alone = std::iter::once(&self.w).chain(self.others.iter()).find(|w| w.vp == f.from).is_some_and(|w| if f.whole_tab { w.tabs.len() == 1 } else { w.panes() == 1 });
        if dest == Dest::NewWindow && from_alone {
            return;
        }
        if f.whole_tab {
            self.carry_tab(f.pane, dest, f.size);
        } else {
            self.carry_pane(f.pane, dest, f.size);
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

/// The program's windows, for eframe. Each window other than the first is a viewport of its
/// own, drawn in its own frame (so a busy window doesn't redraw the others), and all of them
/// share one [`TermApp`].
pub struct Windows(Arc<Mutex<TermApp>>);

impl Windows {
    pub fn new(app: TermApp) -> Windows {
        Windows(Arc::new(Mutex::new(app)))
    }
}

/// Only eframe's thread draws, one window at a time, so the lock is never contended.
fn lock(m: &Mutex<TermApp>) -> std::sync::MutexGuard<'_, TermApp> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl eframe::App for Windows {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let app = lock(&self.0);
        if let Some(shot) = &app.shot {
            // Pictures show the window as when it's in use (in front, the cursor solid).
            raw.focused = true;
            rivet_kit::scripted_click("RIVET_TERM", shot.frames(), raw);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let shared = self.0.clone();
        lock(&self.0).root_frame(ui, &shared);
    }
}

impl TermApp {
    /// A frame of the first window. It also keeps the other windows open: a window whose
    /// viewport isn't shown here closes.
    fn root_frame(&mut self, ui: &mut egui::Ui, shared: &Arc<Mutex<TermApp>>) {
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
        // Windows asked for by later launches of the program.
        let asked: Vec<Start> = self.inbox.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for start in asked {
            self.open_window(start, Vec::new(), Vec2::new(960.0, 620.0));
        }
        self.others.append(&mut self.pending);
        self.window(ui);
        // Screenshots draw every window inside the first, right away.
        let embed = ctx.embed_viewports();
        for k in 0..self.others.len() {
            let (vp, builder) = (self.others[k].vp, self.others[k].builder.clone());
            if embed {
                std::mem::swap(&mut self.w, &mut self.others[k]);
                ctx.show_viewport_immediate(vp, builder, |ui, _| self.window(ui));
                std::mem::swap(&mut self.w, &mut self.others[k]);
            } else {
                let shared = shared.clone();
                ctx.show_viewport_deferred(vp, builder, move |ui, _| lock(&shared).child_frame(ui, vp));
            }
        }
        self.settle(&ctx);
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }

    /// A frame of another window.
    fn child_frame(&mut self, ui: &mut egui::Ui, vp: egui::ViewportId) {
        let Some(k) = self.others.iter().position(|w| w.vp == vp) else { return };
        std::mem::swap(&mut self.w, &mut self.others[k]);
        self.window(ui);
        std::mem::swap(&mut self.w, &mut self.others[k]);
        self.settle(&ui.ctx().clone());
    }

    /// After any window's frame: land what was dragged out, drop windows that closed, and when
    /// the first window closed while others are open, it takes over one of theirs.
    fn settle(&mut self, ctx: &egui::Context) {
        self.land(ctx);
        let before = self.others.len();
        self.others.retain(|w| !w.gone);
        if self.quitting {
            self.others.clear();
        }
        if self.others.len() != before {
            // The first window's frame stops showing them, which closes them.
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        if self.w.gone && !self.quitting {
            if self.others.is_empty() {
                self.w.allow_close = true;
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
            } else {
                let next = self.others.remove(0);
                let size = ctx.input_for(next.vp, |i| i.viewport().inner_rect.map(|r| r.size()));
                self.w = Win { vp: egui::ViewportId::ROOT, builder: self.w.builder.clone(), title: String::new(), refocus: true, ..next };
                if let Some(size) = size {
                    ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::InnerSize(size));
                }
            }
            ctx.request_repaint_of(egui::ViewportId::ROOT);
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

    fn pane(id: u64) -> Pane {
        let spawn = Spawn { program: Some(("sleep".into(), vec!["5".into()])), ..Default::default() };
        Pane { id, session: Session::spawn(&spawn, Size { cols: 80, lines: 24, cell_w: 8, cell_h: 16 }, None).unwrap() }
    }

    #[test]
    fn taking_panes_and_tabs_out_of_a_window() {
        let mut w = Win::new(egui::ViewportId::ROOT, egui::ViewportBuilder::default(), Start::default());
        w.tabs.push(TermTab { panes: vec![pane(1), pane(2), pane(3)], focus: 2, layout: Layout::Tall, label: String::new() });
        w.tabs.push(TermTab { panes: vec![pane(4)], focus: 0, layout: Layout::Tall, label: String::new() });
        w.active = 1;
        assert_eq!(w.find_pane(3), Some((0, 2)));
        // The focused pane goes: focus steps back to a pane that's there.
        assert_eq!(w.remove_pane(0, 2).map(|p| p.id), Some(3));
        assert_eq!(w.tabs[0].focus, 1);
        // A tab's last pane goes: the tab goes, and the open tab is still the same one.
        assert_eq!(w.remove_pane(1, 0).map(|p| p.id), Some(4));
        assert_eq!((w.tabs.len(), w.active, w.gone), (1, 0, false));
        assert_eq!(w.panes(), 2);
        // The last tab goes: the window is done.
        assert_eq!(w.remove_tab(0).map(|t| t.panes.len()), Some(2));
        assert!(w.gone && w.allow_close);
    }
}

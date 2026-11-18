//! The Rivet Welcome window: every Rivet app as a card, with what's installed, what's out, and a
//! button to install, update or open it; and a Settings page for the tray. It looks for new
//! releases as soon as it opens (showing the last answer until then), in the shared Rivet look.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::SystemTime;

use eframe::egui::{
    self, Align, Color32, Key, Layout, Modifiers, Rect, RichText, Sense, Stroke, Vec2,
};
use rivet_look::icons::{self, IconTheme};
use rivet_look::live::{Choice, LiveLook};
use rivet_look::theme::{Mode, r_md, r_sm, status};
use rivet_look::{fonts, palette};

use crate::catalog::Kind;
use crate::install::{self, Progress, Stage};
use crate::local;
use crate::releases::{self, Release};
use crate::settings::{self, Settings};
use crate::state::{self, Row};

pub const ICON_SVG: &str = include_str!("../data/rivet-welcome.svg");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Apps,
    Settings,
}

/// What a card asked for.
enum Act {
    Install(usize),
    Open(usize),
    AskRemove(usize),
    Remove(usize),
    CancelRemove,
    Notes(usize),
}

pub struct WelcomeApp {
    s: Settings,
    look: LiveLook,
    icons: IconTheme,
    shot: Option<rivet_kit::Shot>,
    ctx: egui::Context,
    rows: Vec<Row>,
    /// The apps have been looked for at least once (until then: "Looking…").
    loaded: bool,
    releases: Vec<Release>,
    checked: Option<SystemTime>,
    checking: bool,
    check_rx: Option<Receiver<Result<Vec<Release>, String>>>,
    rows_rx: Option<Receiver<Vec<Row>>>,
    error: Option<String>,
    /// Installs and updates under way, by app id.
    jobs: HashMap<String, Progress>,
    /// A line under a card (what went wrong, or what just happened), by app id.
    notes: HashMap<String, (String, bool)>,
    confirm_remove: Option<String>,
    view: View,
    textures: HashMap<String, egui::TextureHandle>,
    /// Another `rivet-welcome` asked this window to come forward.
    raise: Arc<AtomicBool>,
}

impl WelcomeApp {
    pub fn new(
        cc: &eframe::CreationContext,
        screenshot: Option<PathBuf>,
        raise: Arc<AtomicBool>,
    ) -> WelcomeApp {
        let mut s = Settings::load();
        let demo = screenshot.is_some();
        if demo {
            settings::NO_SAVE.store(true, Ordering::Relaxed);
            s = Settings {
                theme: s.theme,
                accent: s.accent,
                follow_desktop_colors: s.follow_desktop_colors,
                ..Settings::default()
            };
        }
        match std::env::var("RIVET_WELCOME_THEME").as_deref() {
            Ok("light") => (s.theme, s.follow_desktop_colors) = (Mode::Light, false),
            Ok("dark") => (s.theme, s.follow_desktop_colors) = (Mode::Dark, false),
            _ => {}
        }
        let view = if std::env::var("RIVET_WELCOME_VIEW").as_deref() == Ok("settings") {
            View::Settings
        } else {
            View::Apps
        };
        let mut app = WelcomeApp {
            s,
            look: LiveLook::new(),
            icons: IconTheme::new(&icons::detect_system_theme()),
            shot: screenshot.map(rivet_kit::Shot::new),
            ctx: cc.egui_ctx.clone(),
            rows: Vec::new(),
            loaded: false,
            releases: Vec::new(),
            checked: None,
            checking: false,
            check_rx: None,
            rows_rx: None,
            error: None,
            jobs: HashMap::new(),
            notes: HashMap::new(),
            confirm_remove: None,
            view,
            textures: HashMap::new(),
            raise,
        };
        if demo {
            // Checks: the releases in data/, a few apps "installed", and one installing.
            let list =
                releases::parse(include_str!("../data/demo-releases.json")).unwrap_or_default();
            if std::env::var_os("RIVET_WELCOME_INSTALLED").is_none() {
                // SAFETY: set before any other thread reads the environment.
                unsafe {
                    std::env::set_var(
                        "RIVET_WELCOME_INSTALLED",
                        "files=0.6.0,notes=0.1.0,terminal=0.1.0,welcome=0.1.0",
                    )
                };
            }
            app.rows = state::rows(&list);
            app.releases = list;
            app.loaded = true;
            app.checked = Some(SystemTime::now() - std::time::Duration::from_secs(180));
            if let Ok(id) = std::env::var("RIVET_WELCOME_JOB") {
                app.jobs.insert(
                    id,
                    Arc::new(std::sync::Mutex::new(Stage::Downloading {
                        got: 3_900_000,
                        total: 9_000_000,
                    })),
                );
            }
            return app;
        }
        if let Some((list, when)) = releases::load_cached() {
            app.checked = Some(when);
            app.releases = list;
        }
        app.refresh_rows();
        app.check();
        // The tray, when it's on (it quits at once if it's already running).
        if app.s.tray
            && let Ok(exe) = std::env::current_exe()
        {
            let _ = std::process::Command::new(exe)
                .arg("--tray")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map(|mut c| std::thread::spawn(move || c.wait()));
        }
        app
    }

    fn choice(&self) -> Choice {
        Choice {
            mode: self.s.theme,
            accent: self.s.accent,
            follow_desktop_colors: self.s.follow_desktop_colors,
            text_size: self.s.text_size,
        }
    }

    /// Look for releases (off this thread).
    fn check(&mut self) {
        if self.checking || self.shot.is_some() {
            return;
        }
        self.checking = true;
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(releases::check());
            ctx.request_repaint();
        });
        self.check_rx = Some(rx);
    }

    /// Ask every app its version again (after a check, an install, a removal).
    fn refresh_rows(&mut self) {
        if self.shot.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let (list, ctx) = (self.releases.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(state::rows(&list));
            ctx.request_repaint();
        });
        self.rows_rx = Some(rx);
    }

    fn poll(&mut self) {
        if let Some(rx) = &self.check_rx
            && let Ok(r) = rx.try_recv()
        {
            self.check_rx = None;
            self.checking = false;
            match r {
                Ok(list) => {
                    self.releases = list;
                    self.checked = Some(SystemTime::now());
                    self.error = None;
                    self.refresh_rows();
                }
                Err(e) => self.error = Some(e),
            }
        }
        if let Some(rx) = &self.rows_rx
            && let Ok(rows) = rx.try_recv()
        {
            self.rows_rx = None;
            self.rows = rows;
            self.loaded = true;
        }
        // Finished jobs: say how it went, and look again at what's installed.
        let done: Vec<(String, Stage)> = self
            .jobs
            .iter()
            .filter_map(|(id, p)| {
                p.lock()
                    .ok()
                    .filter(|s| s.finished())
                    .map(|s| (id.clone(), s.clone()))
            })
            .collect();
        if !done.is_empty() && self.shot.is_none() {
            for (id, stage) in done {
                self.jobs.remove(&id);
                let title = self
                    .rows
                    .iter()
                    .find(|r| r.entry.id == id)
                    .map(|r| r.entry.title.clone())
                    .unwrap_or_default();
                match stage {
                    Stage::Done(v) => {
                        let restart =
                            if self.rows.iter().any(|r| r.entry.id == id && r.is_welcome()) {
                                " (open it again to use the new version)"
                            } else {
                                ""
                            };
                        self.notes
                            .insert(id, (format!("{title} {v} is installed{restart}"), false));
                    }
                    Stage::Failed(e) => {
                        self.notes
                            .insert(id, (format!("Couldn't install {title}: {e}"), true));
                    }
                    _ => {}
                }
            }
            self.refresh_rows();
        }
        if !self.jobs.is_empty() {
            self.ctx
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    fn start_install(&mut self, i: usize) {
        let Some(row) = self.rows.get(i) else { return };
        let Some((rel, _)) = row.update().or_else(|| row.installable()) else {
            return;
        };
        if self.shot.is_some() || self.jobs.contains_key(&row.entry.id) {
            return;
        }
        let ctx = self.ctx.clone();
        let id = row.entry.id.clone();
        let p = install::start(&row.entry, rel, move || ctx.request_repaint());
        self.notes.remove(&id);
        self.jobs.insert(id, p);
    }

    fn update_all(&mut self) {
        let ids: Vec<usize> = (0..self.rows.len())
            .filter(|i| self.rows[*i].update().is_some())
            .collect();
        for i in ids {
            self.start_install(i);
        }
    }

    fn act(&mut self, a: Act) {
        match a {
            Act::Install(i) => self.start_install(i),
            Act::Open(i) => {
                if let Some(r) = self.rows.get(i)
                    && let Some(inst) = &r.installed
                    && self.shot.is_none()
                    && let Err(e) = local::open(&r.entry, &inst.path)
                {
                    self.notes.insert(
                        r.entry.id.clone(),
                        (format!("Couldn't open {}: {e}", r.entry.title), true),
                    );
                }
            }
            Act::AskRemove(i) => self.confirm_remove = self.rows.get(i).map(|r| r.entry.id.clone()),
            Act::CancelRemove => self.confirm_remove = None,
            Act::Remove(i) => {
                self.confirm_remove = None;
                if let Some(r) = self.rows.get(i).cloned()
                    && self.shot.is_none()
                {
                    match local::remove(&r.entry) {
                        Ok(()) => self.notes.insert(
                            r.entry.id.clone(),
                            (
                                format!(
                                    "{} is removed (its settings and your files are still here)",
                                    r.entry.title
                                ),
                                false,
                            ),
                        ),
                        Err(e) => self.notes.insert(
                            r.entry.id.clone(),
                            (format!("Couldn't remove {}: {e}", r.entry.title), true),
                        ),
                    };
                    self.refresh_rows();
                }
            }
            Act::Notes(i) => {
                if let Some(r) = self.rows.get(i)
                    && let Some((rel, _)) = &r.latest
                {
                    open_url(&rel.url);
                }
            }
        }
    }

    fn app_texture(
        &mut self,
        ctx: &egui::Context,
        i: usize,
        px: f32,
    ) -> Option<egui::TextureHandle> {
        let row = self.rows.get(i)?;
        let key = row.entry.id.clone();
        if let Some(t) = self.textures.get(&key) {
            return Some(t.clone());
        }
        let size = (px * ctx.pixels_per_point()).round() as u32;
        let tex = match row.entry.icon {
            Some(svg) => icons::render_svg(svg.as_bytes(), size).map(|img| {
                ctx.load_texture(format!("app-{key}"), img, egui::TextureOptions::LINEAR)
            }),
            // An app found in the releases: its icon from the theme once it's installed.
            None => {
                let dist = row.entry.dist.clone();
                self.icons.texture(
                    ctx,
                    &[&dist, "application-x-executable", "applications-other"],
                    size,
                )
            }
        }?;
        self.textures.insert(key, tex.clone());
        Some(tex)
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (check, close) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::R),
                i.consume_key(Modifiers::COMMAND, Key::Q)
                    || i.consume_key(Modifiers::COMMAND, Key::W),
            )
        });
        if check {
            self.check();
        }
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    // ---------------------------------------------------------------- the top bar

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let n_updates = state::updates(&self.rows).len();
        let wide = ui.available_width() >= 760.0;
        ui.horizontal(|ui| {
            let px = 44.0;
            if let Some(tex) = self.textures.get("welcome-logo").cloned().or_else(|| {
                let img = icons::render_svg(
                    ICON_SVG.as_bytes(),
                    (px * ui.ctx().pixels_per_point()) as u32,
                )?;
                let t = ui
                    .ctx()
                    .load_texture("welcome-logo", img, egui::TextureOptions::LINEAR);
                self.textures.insert("welcome-logo".into(), t.clone());
                Some(t)
            }) {
                ui.add(egui::Image::new(&tex).fit_to_exact_size(Vec2::splat(px)));
            }
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Rivet Welcome")
                        .font(fonts::ui_font(19.0))
                        .strong()
                        .color(v.strong_text_color()),
                );
                let line = if self.checking && self.checked.is_none() {
                    "Looking for the Rivet apps…".to_string()
                } else if let Some(e) = &self.error {
                    format!("Couldn't look for updates: {e}")
                } else {
                    let when = self
                        .checked
                        .map(|t| format!(" · checked {}", releases::ago(t)))
                        .unwrap_or_default();
                    match n_updates {
                        0 if self.rows.iter().any(|r| r.installed.is_some()) => {
                            format!("Everything is up to date{when}")
                        }
                        0 => format!("Pick the apps you'd like{when}"),
                        1 => format!("1 update is ready{when}"),
                        n => format!("{n} updates are ready{when}"),
                    }
                };
                let color = if self.error.is_some() {
                    status(ui.ctx()).danger
                } else {
                    v.weak_text_color()
                };
                ui.label(RichText::new(line).font(fonts::ui_font(12.5)).color(color));
            });
            if wide {
                ui.add_space(18.0);
                self.switch(ui);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if self.view == View::Apps {
                    if n_updates > 0 && primary(ui, &format!("Update All ({n_updates})")) {
                        self.update_all();
                    }
                    let label = if self.checking {
                        "Checking…"
                    } else {
                        "Check for Updates"
                    };
                    if ui
                        .add_enabled(
                            !self.checking,
                            egui::Button::new(label).corner_radius(r_sm()),
                        )
                        .on_hover_text("Ctrl+R")
                        .clicked()
                    {
                        self.check();
                    }
                    if self.checking {
                        ui.add(egui::Spinner::new().size(16.0));
                    }
                }
            });
        });
        if !wide {
            ui.add_space(10.0);
            self.switch(ui);
        }
    }

    /// Apps | Settings.
    fn switch(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        egui::Frame::NONE
            .fill(palette::mix(v.panel_fill, v.text_color(), 0.06))
            .corner_radius(r_md())
            .inner_margin(egui::Margin::same(3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (view, label) in [(View::Apps, "Apps"), (View::Settings, "Settings")] {
                        let on = self.view == view;
                        let text = RichText::new(label).color(if on {
                            v.selection.stroke.color
                        } else {
                            v.text_color()
                        });
                        let b = egui::Button::new(text)
                            .fill(if on {
                                v.selection.bg_fill
                            } else {
                                Color32::TRANSPARENT
                            })
                            .stroke(Stroke::NONE)
                            .corner_radius(r_sm())
                            .min_size(Vec2::new(72.0, 26.0));
                        if ui.add(b).clicked() {
                            self.view = view;
                        }
                    }
                });
            });
    }

    // ---------------------------------------------------------------- the apps

    fn apps_view(&mut self, ui: &mut egui::Ui) {
        if !self.loaded {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(22.0));
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Looking at what's installed…")
                        .color(ui.visuals().weak_text_color()),
                );
            });
            return;
        }
        let mut groups: [(&str, Vec<usize>); 4] = [
            ("Updates", vec![]),
            ("Installed", vec![]),
            ("Get More Apps", vec![]),
            ("Coming Soon", vec![]),
        ];
        for (i, r) in self.rows.iter().enumerate() {
            let g = if r.update().is_some()
                || (self.jobs.contains_key(&r.entry.id) && r.installed.is_some())
            {
                0
            } else if r.installed.is_some() {
                1
            } else if r.latest.is_some() {
                2
            } else {
                3
            };
            groups[g].1.push(i);
        }
        let mut acts = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let width = ui.available_width();
                let cols = if width >= 860.0 { 2 } else { 1 };
                let gap = 12.0;
                let card_w = ((width - gap * (cols as f32 - 1.0)) / cols as f32).floor();
                for (title, ids) in &groups {
                    if ids.is_empty() {
                        continue;
                    }
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(*title)
                            .font(fonts::ui_font(13.0))
                            .strong()
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(4.0);
                    for chunk in ids.chunks(cols) {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for &i in chunk {
                                if let Some(a) = self.card(ui, i, card_w) {
                                    acts.push(a);
                                }
                            }
                        });
                        ui.add_space(gap - ui.spacing().item_spacing.y);
                    }
                    ui.add_space(8.0);
                }
            });
        for a in acts {
            self.act(a);
        }
    }

    fn card(&mut self, ui: &mut egui::Ui, i: usize, width: f32) -> Option<Act> {
        let ctx = ui.ctx().clone();
        let tex = self.app_texture(&ctx, i, 48.0);
        let row = self.rows.get(i)?.clone();
        let v = ui.visuals().clone();
        let mut act = None;
        let fill = palette::mix(
            v.panel_fill,
            v.text_color(),
            if v.dark_mode { 0.05 } else { 0.035 },
        );
        egui::Frame::NONE
            .fill(fill)
            .corner_radius(r_md())
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(width - 28.0);
                ui.vertical(|ui| self.card_body(ui, i, &row, tex.as_ref(), &mut act));
            });
        act
    }

    fn card_body(
        &mut self,
        ui: &mut egui::Ui,
        i: usize,
        row: &Row,
        tex: Option<&egui::TextureHandle>,
        act: &mut Option<Act>,
    ) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let st = status(&ctx);
        let accent = self.look.accent(&self.choice());
        let job = self
            .jobs
            .get(&row.entry.id)
            .and_then(|p| p.lock().ok().map(|s| s.clone()));
        let note = self.notes.get(&row.entry.id).cloned();
        let confirming = self.confirm_remove.as_deref() == Some(row.entry.id.as_str());
        let soon = row.installed.is_none() && row.latest.is_none();
        {
            ui.horizontal_top(|ui| {
                let icon = 48.0;
                let (r, _) = ui.allocate_exact_size(Vec2::splat(icon), Sense::hover());
                if let Some(t) = tex {
                    let tint = if soon {
                        Color32::from_white_alpha(150)
                    } else {
                        Color32::WHITE
                    };
                    ui.painter().image(
                        t.id(),
                        r,
                        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        tint,
                    );
                }
                ui.add_space(4.0);
                let buttons_w = if soon { 0.0 } else { 196.0 };
                let text_w = (ui.available_width() - buttons_w).max(120.0);
                ui.vertical(|ui| {
                    ui.set_width(text_w);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&row.entry.title)
                                .font(fonts::ui_font(15.0))
                                .strong()
                                .color(v.strong_text_color()),
                        );
                        let (pill, color) = match &row.latest {
                            Some((r, _)) if r.prerelease => ("Alpha", st.warning),
                            Some(_) => ("Stable", st.success),
                            None => ("Not released yet", v.weak_text_color()),
                        };
                        if row.entry.kind == Kind::Shell {
                            tag(ui, "Desktop", st.info);
                        }
                        tag(ui, pill, color);
                    });
                    ui.add(
                        egui::Label::new(
                            RichText::new(&row.entry.blurb)
                                .font(fonts::ui_font(12.5))
                                .color(v.text_color()),
                        )
                        .wrap(),
                    );
                    ui.add_space(2.0);
                    let weak = |t: String| {
                        RichText::new(t)
                            .font(fonts::ui_font(12.0))
                            .color(v.weak_text_color())
                    };
                    match (&row.installed, &row.latest) {
                        (Some(inst), latest) => {
                            let have = inst
                                .version
                                .as_ref()
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "Installed".into());
                            if !inst.ours {
                                ui.label(weak(format!(
                                    "{have} · installed by your system ({})",
                                    inst.path.display()
                                )));
                            } else if let Some((_, new)) = row.update() {
                                ui.label(
                                    RichText::new(format!("{have} installed · {new} is out"))
                                        .font(fonts::ui_font(12.0))
                                        .color(accent),
                                );
                            } else if let Some((_, l)) = latest
                                && inst.version.as_ref().is_some_and(|v| v > l)
                            {
                                ui.label(weak(format!(
                                    "{have} · newer than the latest release ({l})"
                                )));
                            } else {
                                ui.label(weak(format!("{have} · up to date")));
                            }
                        }
                        (None, Some((r, ver))) => {
                            let size = crate::catalog::archive(&row.entry, r)
                                .map(|(a, _)| install::size(a.size))
                                .unwrap_or_default();
                            ui.label(weak(format!(
                                "{ver} · released {} · {size}",
                                releases::day(&r.published)
                            )));
                        }
                        (None, None) => {
                            ui.label(weak("Build it from source for now (see the README)".into()));
                        }
                    }
                    if row.entry.kind == Kind::Shell && row.installed.is_some() {
                        ui.label(weak(
                            "Start it from your compositor; Open shows its settings".into(),
                        ));
                    }
                });
                if soon {
                    return;
                }
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if job.is_some() {
                        ui.add_enabled(false, egui::Button::new("Working…").corner_radius(r_sm()));
                        return;
                    }
                    let ours = row.installed.as_ref().is_some_and(|i| i.ours);
                    // The "more" menu: release notes, removing.
                    if row.latest.is_some() || ours {
                        ui.menu_button(RichText::new("…").strong(), |ui| {
                            if row.latest.is_some() && ui.button("What's New").clicked() {
                                *act = Some(Act::Notes(i));
                                ui.close();
                            }
                            if ours && !row.is_welcome() && ui.button("Remove…").clicked() {
                                *act = Some(Act::AskRemove(i));
                                ui.close();
                            }
                        });
                    }
                    if let Some((_, new)) = row.update() {
                        if primary(ui, &format!("Update to {new}")) {
                            *act = Some(Act::Install(i));
                        }
                    } else if row.installed.is_none()
                        && row.latest.is_some()
                        && primary(ui, "Install")
                    {
                        *act = Some(Act::Install(i));
                    }
                    if row.installed.is_some() && !row.is_welcome() {
                        let label = if row.entry.kind == Kind::Shell {
                            "Settings"
                        } else {
                            "Open"
                        };
                        if ui
                            .add(egui::Button::new(label).corner_radius(r_sm()))
                            .clicked()
                        {
                            *act = Some(Act::Open(i));
                        }
                    }
                });
            });
            if let Some(stage) = &job {
                ui.add_space(8.0);
                bar(ui, stage.fraction(), accent);
                ui.add_space(2.0);
                ui.label(
                    RichText::new(stage.label())
                        .font(fonts::ui_font(12.0))
                        .color(v.weak_text_color()),
                );
            }
            if confirming {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Remove {}? Its settings and your files stay.",
                            row.entry.title
                        ))
                        .font(fonts::ui_font(12.5)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let b = egui::Button::new(
                            RichText::new("Remove")
                                .color(rivet_look::theme::StatusColors::on(st.danger)),
                        )
                        .fill(st.danger)
                        .corner_radius(r_sm());
                        if ui.add(b).clicked() {
                            *act = Some(Act::Remove(i));
                        }
                        if ui
                            .add(egui::Button::new("Cancel").corner_radius(r_sm()))
                            .clicked()
                        {
                            *act = Some(Act::CancelRemove);
                        }
                    });
                });
            }
            if let Some((text, bad)) = &note {
                ui.add_space(6.0);
                let c = if *bad { st.danger } else { st.success };
                ui.add(
                    egui::Label::new(RichText::new(text).font(fonts::ui_font(12.0)).color(c))
                        .wrap(),
                );
            }
        }
    }

    // ---------------------------------------------------------------- settings

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let before = self.s.clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = ui.available_width().min(680.0);
            let pad = ((ui.available_width() - w) / 2.0).max(0.0);
            ui.horizontal_top(|ui| {
                ui.add_space(pad);
                ui.vertical(|ui| {
                    ui.set_width(w);
                    let weak = |t: &str| RichText::new(t).font(fonts::ui_font(12.0)).color(v.weak_text_color());
                    section(ui, "Updates", |ui| {
                        ui.checkbox(&mut self.s.tray, "Keep Rivet Welcome in the tray");
                        ui.indent("tray-note", |ui| {
                            ui.visuals_mut().indent_has_left_vline = false;
                            ui.label(weak("It starts when you sign in, looks for new versions, and shows a dot when there are some. Click it to open this window."))
                        });
                        ui.add_space(6.0);
                        ui.add_enabled(self.s.tray, egui::Checkbox::new(&mut self.s.notify, "Send a notification when updates are ready"));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(self.s.tray, |ui| {
                                ui.label("Look for updates");
                                let name = |h: u32| match h {
                                    0 => "only when I sign in".to_string(),
                                    24 => "every day".to_string(),
                                    h => format!("every {h} hours"),
                                };
                                egui::ComboBox::from_id_salt("check-hours").selected_text(name(self.s.check_hours)).show_ui(ui, |ui| {
                                    for h in [0, 3, 6, 12, 24] {
                                        ui.selectable_value(&mut self.s.check_hours, h, name(h));
                                    }
                                });
                            });
                        });
                    });
                    section(ui, "Look", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Style");
                            for (m, label) in [(Mode::System, "Like the Desktop"), (Mode::Light, "Light"), (Mode::Dark, "Dark")] {
                                ui.selectable_value(&mut self.s.theme, m, label);
                            }
                        });
                        ui.checkbox(&mut self.s.follow_desktop_colors, "Use the desktop's colors");
                    });
                    section(ui, "Where Apps Go", |ui| {
                        let home = rivet_xdg::places::home();
                        let bin = local::bin_home();
                        let shown = bin.strip_prefix(&home).map(|p| format!("~/{}", p.display())).unwrap_or_else(|_| bin.display().to_string());
                        ui.label(format!("Programs go in {shown}, and each app is added to your app launcher. Nothing needs your password."));
                        if !local::bin_home_on_path() && self.shot.is_none() {
                            ui.add_space(4.0);
                            ui.label(RichText::new(format!("{shown} isn't on your PATH: the apps open from the launcher, but not by name in a terminal.")).font(fonts::ui_font(12.0)).color(status(ui.ctx()).warning));
                        }
                        ui.add_space(4.0);
                        ui.label(weak("Apps installed some other way (a distribution package, AUR, Flatpak) are shown but left to that."));
                    });
                    section(ui, "About", |ui| {
                        ui.label(format!("Rivet Welcome {}", env!("CARGO_PKG_VERSION")));
                        ui.label(weak(&format!("Releases come from github.com/{}, checked against their SHA-256 sums.", releases::repo())));
                        ui.add_space(6.0);
                        if ui.add(egui::Button::new("Open the Releases Page").corner_radius(r_sm())).clicked() {
                            open_url(&format!("https://github.com/{}/releases", releases::repo()));
                        }
                    });
                });
            });
        });
        if self.s != before {
            self.s.save();
            if self.s.tray != before.tray && self.shot.is_none() {
                if let Err(e) = settings::set_autostart(self.s.tray) {
                    self.error = Some(e);
                }
                // On: start it now. Off: it sees the settings change and goes.
                if self.s.tray
                    && let Ok(exe) = std::env::current_exe()
                {
                    let _ = std::process::Command::new(exe)
                        .arg("--tray")
                        .stdin(std::process::Stdio::null())
                        .spawn()
                        .map(|mut c| std::thread::spawn(move || c.wait()));
                }
            }
        }
    }
}

/// A filled accent button (the main thing to do).
fn primary(ui: &mut egui::Ui, text: &str) -> bool {
    let v = ui.visuals().clone();
    ui.add(
        egui::Button::new(RichText::new(text).color(v.selection.stroke.color))
            .fill(v.selection.bg_fill)
            .corner_radius(r_sm()),
    )
    .clicked()
}

/// A small coloured label ("Alpha", "Stable").
fn tag(ui: &mut egui::Ui, text: &str, color: Color32) {
    let v = ui.visuals();
    let bg = palette::mix(v.panel_fill, color, if v.dark_mode { 0.28 } else { 0.18 });
    let fg = palette::mix(
        color,
        v.strong_text_color(),
        if v.dark_mode { 0.35 } else { 0.25 },
    );
    egui::Frame::NONE
        .fill(bg)
        .corner_radius(r_sm())
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).font(fonts::ui_font(11.0)).color(fg));
        });
}

/// A thin progress bar.
fn bar(ui: &mut egui::Ui, f: f32, accent: Color32) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
    let v = ui.visuals();
    ui.painter()
        .rect_filled(r, 3.0, palette::mix(v.panel_fill, v.text_color(), 0.12));
    let mut done = r;
    done.set_width((r.width() * f.clamp(0.0, 1.0)).max(6.0));
    ui.painter().rect_filled(done, 3.0, accent);
}

/// A settings card with a heading.
fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let v = ui.visuals().clone();
    ui.add_space(6.0);
    ui.label(
        RichText::new(title)
            .font(fonts::ui_font(13.0))
            .strong()
            .color(v.weak_text_color()),
    );
    ui.add_space(4.0);
    let fill = palette::mix(
        v.panel_fill,
        v.text_color(),
        if v.dark_mode { 0.05 } else { 0.035 },
    );
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(r_md())
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(10.0);
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut c| std::thread::spawn(move || c.wait()));
}

impl eframe::App for WelcomeApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(shot) = &self.shot {
            raw.focused = true;
            rivet_kit::scripted_click("RIVET_WELCOME", shot.frames(), raw);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let c = self.choice();
        self.look.frame(&ctx, &c, self.shot.is_none());
        if self.raise.swap(false, Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.poll();
        self.shortcuts(&ctx);
        if self.checked.is_some() && self.shot.is_none() {
            // "checked 3 minutes ago" keeps up.
            ctx.request_repaint_after(std::time::Duration::from_secs(30));
        }
        let v = ctx.global_style().visuals.clone();
        let top = egui::Frame::new()
            .fill(v.panel_fill)
            .inner_margin(egui::Margin {
                left: 18,
                right: 18,
                top: 14,
                bottom: 10,
            });
        egui::Panel::top("welcome-top")
            .show_separator_line(false)
            .frame(top)
            .show(ui, |ui| self.top_bar(ui));
        let main = egui::Frame::new()
            .fill(v.panel_fill)
            .inner_margin(egui::Margin {
                left: 18,
                right: 18,
                top: 4,
                bottom: 12,
            });
        egui::CentralPanel::default()
            .frame(main)
            .show(ui, |ui| match self.view {
                View::Apps => self.apps_view(ui),
                View::Settings => self.settings_view(ui),
            });
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }
}

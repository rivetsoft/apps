//! `rivet-shell settings`: the shell's settings in a window like the other Rivet apps'
//! Preferences (a sidebar of pages, titled cards). Every change is saved at once; the running
//! shell notices the file change and follows.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use rivet_look::icons::IconTheme;
use rivet_look::live::LiveLook;
use rivet_look::theme::{Mode, r_md, r_xs, selected_tile};
use rivet_look::{fonts, palette};
use rivet_xdg::launch::DesktopApp;

use crate::config::{DockHide, Settings, Side, Transition};
use crate::ui::find_app;

mod displays;
mod item_opts;
mod launcher;
mod nav;
mod controls;
mod look_opts;
mod font_pick;
mod wallpaper;
mod idle_lock;
mod colors;
mod icon_pick;
mod palette_pick;
mod notes_preview;
mod panel_editor;
mod plugins;
mod search;
mod window_look;

pub const ICON_SVG: &str = include_str!("../data/rivet-shell.svg");

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Page {
    #[default]
    Wallpaper,
    Appearance,
    Panel,
    Dock,
    Widgets,
    Launcher,
    Notifications,
    Displays,
    Idle,
    Plugins,
    About,
}

impl Page {
    const ALL: [Page; 11] = [Page::Wallpaper, Page::Appearance, Page::Panel, Page::Dock, Page::Widgets, Page::Launcher, Page::Notifications, Page::Displays, Page::Idle, Page::Plugins, Page::About];
    fn label(self) -> &'static str {
        match self {
            Page::Wallpaper => "Wallpaper",
            Page::Appearance => "Appearance",
            Page::Panel => "Panels",
            Page::Dock => "Dock",
            Page::Widgets => "Widgets",
            Page::Launcher => "Launcher",
            Page::Notifications => "Notifications",
            Page::Displays => "Displays",
            Page::Idle => "Idle & Lock",
            Page::Plugins => "Plugins",
            Page::About => "About",
        }
    }
}

pub struct SettingsApp {
    s: Settings,
    /// The Displays page's screens and what's being changed.
    disp: displays::State,
    look: LiveLook,
    /// The settings file's time when last read or written here, and when it was last looked at:
    /// a change from elsewhere (the shell's own, `rivet-shell msg wallpaper random`) is taken in,
    /// not written over.
    s_mtime: Option<std::time::SystemTime>,
    s_checked: f64,
    icons: IconTheme,
    /// The icon theme setting `icons` was made for.
    icon_key: String,
    page: Page,
    apps: Vec<DesktopApp>,
    /// The wallpaper folder's pictures and thumbnails.
    picker: crate::wall_pick::Picker,
    /// The file picker's answer, on its way.
    #[allow(clippy::type_complexity)]
    picking: Option<Receiver<(crate::portal::Pick, Result<Option<PathBuf>, String>)>>,
    /// A folder being chosen for a plugin's setting: (plugin, key), and the picker's answer.
    #[allow(clippy::type_complexity)]
    plugin_pick: Option<(String, String, Receiver<Result<Option<PathBuf>, String>>)>,
    store: plugins::Store,
    /// The screen pictures are being chosen for (none: every screen without its own).
    wall_screen: String,
    add_filter: String,
    /// The screens panels can be put on (output names).
    screens: Vec<String>,
    /// The panel being edited on the Panels page, and the look its preview is drawn with.
    panel_sel: usize,
    preview_kit: Option<crate::ui::Kit>,
    note: Option<String>,
    app_icon: Option<egui::TextureHandle>,
    shot: Option<rivet_kit::Shot>,
    /// Checks: a folder of demo pictures (never the person's).
    demo_dir: Option<PathBuf>,
    /// The sidebar's search: what's typed, the result the arrow keys mark, the one last picked.
    search: String,
    search_sel: usize,
    search_chosen: Option<&'static str>,
    /// A panel item whose options to open (a search picked one of them).
    open_item: Option<String>,
    /// Said at the top of the Panels page (a search picked an item no panel has).
    note_panel: Option<String>,
}

impl SettingsApp {
    pub fn new(_cc: &eframe::CreationContext, shot: Option<PathBuf>) -> SettingsApp {
        let mut s = Settings::load();
        let mut demo_dir = None;
        let mut sub_sel = None;
        if shot.is_some() {
            crate::config::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
            rivet_media::thumbs::NO_WRITE.store(true, std::sync::atomic::Ordering::Relaxed);
            s = Settings::default();
            let d = std::env::temp_dir().join(format!("rivet-shell-demo-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&d);
            if let Some(p) = crate::wall::demo_png(&d) {
                // A few pictures, tinted differently.
                for (k, tint) in [(1, [1.0, 0.7, 0.6]), (2, [0.6, 1.0, 0.8]), (3, [0.7, 0.8, 1.2]), (4, [1.2, 1.0, 0.6]), (5, [0.9, 0.6, 1.1])] {
                    if let Ok(img) = image::open(&p) {
                        let mut img = img.to_rgba8();
                        for px in img.pixels_mut() {
                            for c in 0..3 {
                                px[c] = (px[c] as f32 * tint[c]).min(255.0) as u8;
                            }
                        }
                        let _ = img.save(d.join(format!("demo-{k}.png")));
                    }
                }
                s.wallpaper = p.to_string_lossy().into_owned();
            }
            s.wallpaper_dir = d.to_string_lossy().into_owned();
            if std::env::var("RIVET_SHELL_PANELS").as_deref() == Ok("2") {
                s.panels.push(crate::panels::PanelCfg { screen: "HDMI-A-1".into(), ..crate::panels::PanelCfg::new_bottom() });
            }
            // RIVET_SHELL_SPACER=40: a spacer that size after the first panel's first item.
            if let (Some(n), Some(p)) = (std::env::var("RIVET_SHELL_SPACER").ok().and_then(|n| n.parse::<u32>().ok()), s.panels.first_mut()) {
                p.zones[0].insert(1.min(p.zones[0].len()), format!("spacer:{n}"));
            }
            // RIVET_SHELL_SPACE=20:0: the first panel's offset from its edge, and its room before windows.
            if let (Some((o, g)), Some(p)) = (std::env::var("RIVET_SHELL_SPACE").ok().and_then(|v| v.split_once(':').map(|(o, g)| (o.parse().ok(), g.parse().ok()))), s.panels.first_mut()) {
                (p.offset, p.gap) = (o, g);
            }
            // RIVET_SHELL_FONTS=base:panel:launcher sets the three fonts (a part left empty follows).
            if let Ok(f) = std::env::var("RIVET_SHELL_FONTS") {
                let mut it = f.split(':').map(str::to_string);
                (s.font, s.launcher_font) = (it.next().unwrap_or_default(), { let p = it.next().unwrap_or_default(); if let Some(first) = s.panels.first_mut() { first.font = p; } it.next().unwrap_or_default() });
            }
            // RIVET_SHELL_SIZE=48:10: the first panel's thickness, and the room between its items.
            if let (Some((a, b)), Some(p)) = (std::env::var("RIVET_SHELL_SIZE").ok().and_then(|v| v.split_once(':').map(|(a, b)| (a.parse().ok(), b.parse().ok()))), s.panels.first_mut()) {
                (p.size, p.spacing) = (a, b);
            }
            // RIVET_SHELL_ROLES=3584e4:e5a50a: a style and accent of the shell's own, with this
            // secondary and tertiary (a part left empty is made from the accent).
            if let Ok(r) = std::env::var("RIVET_SHELL_ROLES") {
                let (a, b) = r.split_once(':').unwrap_or((&r, ""));
                let hex = |h: &str| u32::from_str_radix(h, 16).ok().filter(|_| h.len() == 6).map(|n| [(n >> 16) as u8, (n >> 8) as u8, n as u8]);
                (s.follow_desktop_colors, s.secondary, s.tertiary) = (false, hex(a), hex(b));
                s.palette.clear();
            }
            // RIVET_SHELL_COLORS=2: how much the secondary and tertiary colours are used.
            if let Some(n) = std::env::var("RIVET_SHELL_COLORS").ok().and_then(|n| n.parse().ok()) {
                s.color_use = n;
            }
            // RIVET_SHELL_EDGE=left, RIVET_SHELL_LEN=60, RIVET_SHELL_HIDE=auto shape the first panel.
            if let (Ok(e), Some(p)) = (std::env::var("RIVET_SHELL_EDGE"), s.panels.first_mut()) {
                p.edge = crate::config::Edge::from(&e);
            }
            if let (Ok(l), Some(p)) = (std::env::var("RIVET_SHELL_LEN"), s.panels.first_mut()) {
                p.length = l.parse().unwrap_or(100);
            }
            if let (Ok(h), Some(p)) = (std::env::var("RIVET_SHELL_HIDE"), s.panels.first_mut()) {
                p.hide = DockHide::from(&h);
            }
            // RIVET_SHELL_SUB=open|stay: a sub panel under the first panel, chosen on the Panels page.
            if let Ok(how) = std::env::var("RIVET_SHELL_SUB")
                && let Some(i) = crate::panels::add_sub(&mut s.panels, 0)
            {
                let list = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
                s.panels[i].zones = [list(&["dock"]), Vec::new(), list(&["system", "ai"])];
                if let Some(sub) = s.panels[i].sub.as_mut() {
                    (sub.stay, sub.open) = (how == "stay", how == "stay");
                }
                crate::panels::rename_sub(&mut s.panels, i, "Shortcuts");
                s.tidy_panels();
                sub_sel = Some(i);
            }
            demo_dir = Some(d);
            // RIVET_SHELL_TEXT=15 sets the text size.
            if let Some(pt) = std::env::var("RIVET_SHELL_TEXT").ok().and_then(|t| t.parse().ok()) {
                s.text_size = pt;
            }
            // RIVET_SHELL_ITEM_OPTS=ID opens an item's options; it's put on the panel first.
            if let Ok(id) = std::env::var("RIVET_SHELL_ITEM_OPTS")
                && let Some(p) = s.panels.first_mut()
                && !p.has(&id)
            {
                p.zones[2].insert(0, id);
            }
            // Checks: a chosen palette (its file isn't written in checks; `rivet-shell palette` does that).
            // Checks: an icon theme, by name.
            if let Ok(i) = std::env::var("RIVET_SHELL_ICONS") {
                s.icon_theme = i;
            }
            // Checks: the shell's own surface colour, dark and light alike (rrggbb).
            if let Some(n) = std::env::var("RIVET_SHELL_SURFACE").ok().and_then(|h| u32::from_str_radix(&h, 16).ok()) {
                let c = Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
                (s.surface_dark, s.surface_light) = (c, c);
            }
            if let Ok(p) = std::env::var("RIVET_SHELL_PALETTE") {
                s.choose_palette(&p);
            }
            match std::env::var("RIVET_SHELL_THEME").as_deref() {
                Ok("light") => (s.theme, s.follow_desktop_colors) = (Mode::Light, false),
                Ok("dark") => (s.theme, s.follow_desktop_colors) = (Mode::Dark, false),
                _ => {}
            }
            // Checks: groups on the first panel (as in its line of the settings file).
            if let Ok(g) = std::env::var("RIVET_SHELL_GROUPS") {
                s.panels[0].groups = g.split(';').map(crate::panels::Group::parse).collect();
            }
            // Checks: RIVET_SHELL_PLUGINS_ON=notes,disk turns those plugins on.
            if let Ok(p) = std::env::var("RIVET_SHELL_PLUGINS_ON") {
                s.plugins = p.split(',').map(String::from).collect();
                s.set_plugin("notes", "vault", "~/Documents/Notes");
            }
            // Checks: a look for the panels and dock (as in the settings file).
            if let Ok(l) = std::env::var("RIVET_SHELL_LOOK") {
                s.look = crate::style::Style::parse(&l, '=', ';');
            }
            // Checks: the notification cards' corner and width, and their look, as in the shell's pictures.
            if let Ok(o) = std::env::var("RIVET_SHELL_NOTE_STYLE") {
                let mut parts = o.split(',');
                s.note_place = crate::config::NotePlace::from(parts.next().unwrap_or_default());
                s.note_width = crate::config::LaunchSize::from(parts.next().unwrap_or_default());
            }
            if let Ok(l) = std::env::var("RIVET_SHELL_NOTE_LOOK") {
                s.note_look = crate::style::Style::parse(&l, '=', ';');
            }
            // Checks: the dock's own look, so its Reset buttons show (RIVET_SHELL_DOCK_LOOK=radius=4).
            if let Ok(l) = std::env::var("RIVET_SHELL_DOCK_LOOK") {
                s.dock_look = crate::style::Style::parse(&l, '=', ';');
            }
        }
        let shot_mode = shot.is_some();
        let page = match std::env::var("RIVET_SHELL_PAGE").as_deref() {
            Ok(p) => Page::ALL.into_iter().find(|x| x.label().eq_ignore_ascii_case(p)).unwrap_or_default(),
            _ => Page::default(),
        };
        let mut apps = rivet_xdg::launch::all_apps();
        apps.sort_by_key(|a| a.name.to_lowercase());
        apps.dedup_by(|a, b| a.id == b.id);
        SettingsApp {
            s,
            look: LiveLook::new(),
            s_mtime: crate::config::mtime(),
            s_checked: 0.0,
            icons: IconTheme::new(&rivet_look::icons::detect_system_theme()),
            icon_key: String::new(),
            page,
            apps,
            picker: Default::default(),
            picking: None,
            plugin_pick: None,
            // Checks: RIVET_SHELL_PLUGIN_REMOVING=ID shows the question Remove asks.
            store: plugins::Store::asking(std::env::var("RIVET_SHELL_PLUGIN_REMOVING").ok().filter(|_| shot.is_some())),
            wall_screen: std::env::var("RIVET_SHELL_WALL_SCREEN").ok().filter(|_| shot.is_some()).unwrap_or_default(),
            add_filter: String::new(),
            screens: if shot.is_some() { vec!["DP-1".into(), "HDMI-A-1".into()] } else { crate::sys::screens(crate::sys::Compositor::detect()) },
            panel_sel: sub_sel.unwrap_or(0),
            preview_kit: None,
            note: None,
            app_icon: None,
            shot: shot.map(rivet_kit::Shot::new),
            demo_dir,
            search: std::env::var("RIVET_SHELL_SEARCH").ok().filter(|_| shot_mode).unwrap_or_default(),
            search_sel: 0,
            search_chosen: None,
            open_item: None,
            note_panel: None,
            disp: Default::default(),
        }
    }

    fn app_texture(&mut self, ctx: &egui::Context, a: &DesktopApp, px: f32) -> Option<egui::TextureHandle> {
        let size = (px * ctx.pixels_per_point()).round() as u32;
        let own = if a.icon.starts_with('/') { self.icons.texture_from_path(ctx, std::path::Path::new(&a.icon), size) } else if a.icon.is_empty() { None } else { self.icons.texture(ctx, &[a.icon.as_str()], size) };
        own.or_else(|| self.icons.texture(ctx, &["application-x-executable"], size))
    }
}

impl Drop for SettingsApp {
    fn drop(&mut self) {
        if let Some(d) = &self.demo_dir {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

fn section(ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)) {
    let fill = ui.visuals().faint_bg_color;
    ui.label(egui::RichText::new(title).strong().size(15.0));
    ui.add_space(6.0);
    let fr = egui::Frame::NONE.fill(fill).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(20, 18)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        on_card(ui.visuals_mut(), fill);
        body(ui);
    });
    search::mark(ui, title, fr.response.rect);
    ui.add_space(28.0);
}

/// Controls on a card: wells, rails and outlines set from the card's own color, so fields,
/// sliders and buttons stand out from it whatever palette the desktop has. The theme's own
/// control colors are made from the window background and can land within a few shades of a card.
pub(super) fn on_card(v: &mut egui::Visuals, card: egui::Color32) {
    let fg = v.text_color();
    let at = |t: f32| palette::mix(card, fg, t);
    v.text_edit_bg_color = Some(at(0.06));
    let w = &mut v.widgets;
    w.inactive.bg_fill = at(0.2); // slider rails, checkbox boxes
    w.inactive.weak_bg_fill = at(0.07);
    w.inactive.bg_stroke = egui::Stroke::new(1.0, at(0.16));
    w.hovered.bg_fill = at(0.26);
    w.hovered.weak_bg_fill = at(0.12);
    w.hovered.bg_stroke = egui::Stroke::new(1.0, at(0.3));
    w.active.bg_fill = at(0.3);
    w.active.weak_bg_fill = at(0.16);
    w.active.bg_stroke = egui::Stroke::new(1.0, at(0.34));
}

/// Whether the compositor tells the shell which workspaces have windows (niri, Hyprland, mango),
/// which Smart Hide and the dock's open-app dots need. Screenshots draw as if it does.
pub(super) fn knows_windows() -> bool {
    use crate::sys::Compositor;
    matches!(Compositor::detect(), Compositor::Niri | Compositor::Hyprland | Compositor::Mango) || std::env::args().any(|a| a == "--screenshot")
}

fn hint(ui: &mut egui::Ui, text: &str) {
    // A choice whose label says it all passes "", and nothing is drawn.
    if text.is_empty() {
        return;
    }
    let weak = ui.visuals().weak_text_color();
    ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(weak)).wrap());
}

fn grid(ui: &mut egui::Ui, id: &str, body: &mut dyn FnMut(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([24.0, 14.0]).min_col_width(170.0).show(ui, |ui| body(ui));
}

impl eframe::App for SettingsApp {
    /// Checks: RIVET_SHELL_DRAG=x1,y1,x2,y2 presses at the first point, drags to the second and lets
    /// go (add ",hold" to keep holding, to see the drag mid-way).
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let Some(shot) = &self.shot else { return };
        let Ok(spec) = std::env::var("RIVET_SHELL_DRAG") else { return };
        let n: Vec<f32> = spec.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if n.len() < 4 {
            return;
        }
        let (a, b) = (Pos2::new(n[0], n[1]), Pos2::new(n[2], n[3]));
        let f = shot.frames();
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        match f {
            30 => raw.events.extend([egui::Event::PointerMoved(a), button(a, true)]),
            31..=60 => raw.events.push(egui::Event::PointerMoved(a + (b - a) * ((f - 30) as f32 / 30.0))),
            62 if !spec.ends_with("hold") => raw.events.push(button(b, false)),
            _ => {}
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let live = self.shot.is_none();
        // Changed from elsewhere since it was read here: read it again (twice a second, and not
        // mid-press, when a change of this window's own may be on its way).
        let now = ctx.input(|i| i.time);
        if live && now - self.s_checked >= 0.5 && !ctx.input(|i| i.pointer.any_down()) {
            self.s_checked = now;
            let m = crate::config::mtime();
            if m != self.s_mtime {
                self.s_mtime = m;
                self.s = Settings::load();
            }
        }
        if live {
            // Less often while another window has the keyboard: nothing here is being used.
            let focused = ctx.input(|i| i.focused);
            ctx.request_repaint_after(std::time::Duration::from_millis(if focused { 500 } else { 2000 }));
        }
        self.look.set_fonts(&ctx, &self.s.font, "", "");
        self.s.set_surfaces(&mut self.look);
        self.look.frame(&ctx, &self.s.choice(), live);
        if self.s.icon_theme != self.icon_key {
            self.icon_key = self.s.icon_theme.clone();
            self.icons = crate::ui::icon_theme(&self.s);
            self.preview_kit = None;
        }
        let v = ctx.global_style().visuals.clone();
        let accent = self.look.accent(&self.s.choice());
        self.icons.set_builtin(Some(rivet_look::builtin_icons::BuiltinColors { accent, bg: v.extreme_bg_color, fg: v.text_color(), dark: v.dark_mode }));
        if self.app_icon.is_none() {
            self.app_icon = rivet_look::icons::render_svg(ICON_SVG.as_bytes(), (96.0 * ctx.pixels_per_point()) as u32).map(|img| ctx.load_texture("rivet-shell-icon", img, egui::TextureOptions::LINEAR));
        }
        let before = self.s.clone();
        let side_fill = palette::mix(v.window_fill, v.panel_fill, 0.6);
        egui::Panel::left("shell-settings-pages").show_separator_line(false).resizable(false).exact_size(230.0).frame(egui::Frame::NONE.fill(side_fill).inner_margin(egui::Margin { left: 12, right: 12, top: 14, bottom: 8 })).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let field = search::field(ui, &mut self.search);
            ui.add_space(10.0);
            if !self.search.trim().is_empty() {
                if field.changed() {
                    self.search_sel = 0;
                }
                let hits = search::find(&self.search);
                egui::ScrollArea::vertical().id_salt("shell-search-results").auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(en) = search::results(ui, &hits, &mut self.search_sel, field.has_focus() || field.lost_focus(), self.search_chosen) {
                        self.go(ui.ctx(), en);
                    }
                });
                return;
            }
            self.search_chosen = None;
            for (name, pages) in nav::GROUPS {
                // A small heading over each group; About sits apart.
                if name.is_empty() {
                    ui.add_space(14.0);
                } else {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
                    ui.painter().text(Pos2::new(r.left() + 12.0, r.bottom() - 9.0), egui::Align2::LEFT_CENTER, name.to_uppercase(), fonts::ui_font(10.5), v.weak_text_color().gamma_multiply(0.8));
                }
                for p in *pages {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
                    let sel = *p == self.page;
                    if sel {
                        let (fill, stroke) = selected_tile(&v, side_fill);
                        ui.painter().rect_filled(r, r_md(), fill);
                        ui.painter().rect_stroke(r, r_md(), stroke, egui::StrokeKind::Inside);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, r_md(), palette::mix(side_fill, v.text_color(), 0.07));
                    }
                    // The symbol in the accent colour; on the chosen page, in the text's.
                    let fg = if sel { v.strong_text_color() } else { v.text_color() };
                    nav::icon(ui.painter(), *p, Pos2::new(r.left() + 21.0, r.center().y), if sel { fg } else { accent });
                    ui.painter().text(Pos2::new(r.left() + 42.0, r.center().y), egui::Align2::LEFT_CENTER, p.label(), fonts::ui_font(13.5), fg);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.page = *p;
                    }
                }
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(v.panel_fill)).show(ui, |ui| {
            // Checks: RIVET_SHELL_SCROLL=600 shows the page scrolled that far down.
            let mut area = egui::ScrollArea::vertical().id_salt(("shell-page", self.page as u8)).auto_shrink([false, false]);
            if let Some(y) = self.shot.as_ref().and_then(|_| std::env::var("RIVET_SHELL_SCROLL").ok()).and_then(|y| y.parse::<f32>().ok()) {
                area = area.vertical_scroll_offset(y);
            }
            area.show(ui, |ui| {
                egui::Frame::NONE.inner_margin(egui::Margin { left: 26, right: 22, top: 18, bottom: 22 }).show(ui, |ui| {
                    ui.set_width(ui.available_width().max(380.0));
                    ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
                    ui.spacing_mut().button_padding = Vec2::new(10.0, 4.0);
                    if self.page != Page::About {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 3.0;
                            ui.label(egui::RichText::new(self.page.label()).size(21.0).strong());
                            ui.label(egui::RichText::new(nav::about(self.page)).size(12.5).color(v.weak_text_color()));
                        });
                        ui.add_space(18.0);
                    }
                    match self.page {
                        Page::Wallpaper => self.wallpaper_page(ui, accent),
                        Page::Appearance => self.appearance_page(ui),
                        Page::Panel => self.panel_page(ui),
                        Page::Dock => self.dock_page(ui),
                        Page::Widgets => self.widgets_page(ui),
                        Page::Launcher => self.launcher_page(ui),
                        Page::Notifications => self.notifications_page(ui),
                        Page::Displays => self.displays_page(ui, accent),
                        Page::Idle => self.idle_page(ui),
                        Page::Plugins => self.plugins_page(ui),
                        Page::About => self.about_page(ui),
                    }
                });
            });
        });
        if self.page == Page::Plugins {
            self.plugin_overlay(&ctx);
        }
        if self.s != before {
            self.s.save();
            self.s_mtime = crate::config::mtime();
            // The shell looks for changes twice a second; tell it, so a change shows at once.
            if self.shot.is_none() {
                let _ = crate::ipc::send("changed");
            }
        }
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }
}

impl SettingsApp {
    /// Open a search result: its page, its card (outlined a moment), or its panel item's options.
    fn go(&mut self, ctx: &egui::Context, en: &'static search::Entry) {
        self.page = en.page;
        self.search_chosen = Some(en.title);
        self.note_panel = None;
        if let Some(id) = en.item {
            if let Some(i) = self.s.panels.iter().position(|p| p.has(id)) {
                self.panel_sel = i;
                self.open_item = Some(id.to_string());
            } else {
                self.note_panel = Some(format!("{} isn't on a panel. Add it with \"Add Item\" to change how it shows.", crate::panels::label(id)));
            }
        } else if !en.section.is_empty() {
            search::reveal(ctx, en.section);
        }
    }

    fn wallpaper_page(&mut self, ui: &mut egui::Ui, accent: Color32) {
        self.picture_section(ui, accent);
        section(ui, "Transition", &mut |ui| {
            grid(ui, "shell-wall-transition", &mut |ui| {
                ui.label("A new picture");
                let choices: Vec<(Transition, &str)> = Transition::ALL.iter().map(|t| (*t, t.label())).collect();
                panel_editor::segmented(ui, &mut self.s.wall_transition, &choices);
                ui.end_row();
                ui.label("Takes");
                ui.add_enabled_ui(self.s.wall_transition != Transition::None, |ui| {
                    let mut secs = self.s.wall_transition_ms as f32 / 1000.0;
                    if ui.add(egui::Slider::new(&mut secs, 0.2..=3.0).step_by(0.1).suffix(" s")).changed() {
                        self.s.wall_transition_ms = (secs * 1000.0).round() as u32;
                    }
                });
                ui.end_row();
            });
            ui.add_space(6.0);
            hint(ui, match self.s.wall_transition {
                Transition::None => "The new picture replaces the old at once.",
                Transition::Fade => "The new picture fades in over the old.",
                Transition::Wipe => "A soft edge sweeps across, left to right.",
                Transition::Slide => "The new picture comes in from the right, pushing the old one out.",
                Transition::Grow => "The new picture opens from the middle in a circle.",
            });
        });
    }

    fn appearance_page(&mut self, ui: &mut egui::Ui) {
        self.colors_preview(ui);
        self.colors_section(ui);
        section(ui, "Color use", &mut |ui| {
            grid(ui, "shell-color-use", &mut |ui| {
                ui.label("Other theme colors");
                panel_editor::segmented(ui, &mut self.s.color_use, &[(0, "Accent Only"), (1, "Some"), (2, "A Lot")]);
                ui.end_row();
            });
            ui.add_space(14.0);
            // What each colour is on at this setting, beside the colour itself.
            let colors = crate::style::Roles::new(&self.look, &self.s);
            let rows = [
                ("Accent", colors.primary, if self.s.color_use == 0 { "Everything highlighted" } else { "Everything else highlighted" }),
                ("Secondary", colors.secondary, match self.s.color_use {
                    0 => "",
                    1 => "Switches and quick settings that are on, the memory bar",
                    _ => "Switches and quick settings that are on, the memory bar, sliders, the selected item in a list",
                }),
                ("Tertiary", colors.tertiary, match self.s.color_use {
                    0 => "",
                    1 => "Notifications, the battery bar",
                    _ => "Notifications, the battery bar, running-app dots, Now Playing",
                }),
            ];
            let (text, weak) = (ui.visuals().text_color(), ui.visuals().weak_text_color());
            for (name, color, on) in rows {
                let used = !on.is_empty();
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                    if used {
                        ui.painter().circle_filled(r.center(), 7.0, color);
                    } else {
                        ui.painter().circle_stroke(r.center(), 6.5, egui::Stroke::new(1.0, weak));
                    }
                    ui.add_space(4.0);
                    let (nr, _) = ui.allocate_exact_size(Vec2::new(84.0, 16.0), Sense::hover());
                    ui.painter().text(nr.left_center(), egui::Align2::LEFT_CENTER, name, egui::TextStyle::Body.resolve(ui.style()), if used { text } else { weak });
                    ui.add(egui::Label::new(egui::RichText::new(if used { on } else { "Not used" }).color(weak)).wrap());
                });
                ui.add_space(4.0);
            }
            // Where the two colours come from: the theme, or (without them there) the accent.
            let own = self.s.own_roles();
            let from_theme = (own.0.is_some() && own.1.is_some()) || (self.s.follow_desktop_colors && self.look.palette().is_some_and(|p| p.secondary.is_some() && p.tertiary.is_some()));
            if self.s.color_use > 0 && self.s.follow_desktop_colors && palette::matugen_template_outdated() {
                ui.add_space(8.0);
                hint(ui, "Secondary and tertiary colors look close to the accent because Rivet's colors in matugen are out of date. Update them to get the full range.");
                ui.add_space(4.0);
                if controls::button(ui, controls::Icon::Reset, "Update Rivet's Colors").on_hover_text("Updates Rivet's colors in matugen and recolors the desktop now").clicked() {
                    self.note = Some(match palette::install_matugen_template() {
                        Ok(_) => {
                            let pic = std::path::PathBuf::from(self.s.matugen_picture());
                            if self.s.makes_colors() && pic.is_file() {
                                crate::wall::run_matugen(&pic, self.s.color_mode, &self.s.scheme_type, self.s.matugen_contrast, |_| {});
                                "Template updated. Remaking the colors now.".to_string()
                            } else {
                                "Template updated. The colors change the next time matugen runs.".to_string()
                            }
                        }
                        Err(e) => format!("Couldn't update matugen's template: {e}"),
                    });
                }
            } else if self.s.color_use > 0 && !from_theme && (self.s.follow_desktop_colors || !self.s.palette.is_empty()) {
                ui.add_space(8.0);
                hint(ui, "This theme has no secondary or tertiary color, so Rivet makes them from the accent.");
            }
        });
        section(ui, "Text", &mut |ui| {
            grid(ui, "shell-text", &mut |ui| {
                ui.label("Font");
                ui.horizontal(|ui| {
                    font_pick::font_pick(ui, "shell-font", &mut self.s.font, &format!("Like the Desktop ({})", font_pick::desktop_name()));
                    if !self.s.font.is_empty() && controls::reset(ui).clicked() {
                        self.s.font.clear();
                    }
                });
                ui.end_row();
                ui.label("Text size");
                panel_editor::segmented(ui, &mut self.s.text_size, &[(0.0, "Like the Desktop"), (12.5, "Small"), (13.5, "Medium"), (15.0, "Large")]);
                ui.end_row();
            });
        });
        self.icons_section(ui);
        self.look_section(ui);
        self.window_look_section(ui);
    }

    fn panel_page(&mut self, ui: &mut egui::Ui) {
        self.panels_editor(ui);
    }

    fn notifications_page(&mut self, ui: &mut egui::Ui) {
        section(ui, "Notifications", &mut |ui| {
            controls::master(ui, &mut self.s.notifications, "Show apps' notifications");
            hint(ui, "Past notifications stay in quick settings. If mako, dunst or another shell is running, it shows them instead.");
            controls::rule(ui);
            if self.s.notifications {
                self.notes_preview(ui);
                ui.add_space(10.0);
            }
            let on = self.s.notifications;
            let s = &mut self.s;
            ui.add_enabled_ui(on, |ui| {
                grid(ui, "shell-notes", &mut |ui| {
                    use crate::config::{LaunchSize, NotePlace};
                    ui.label("Where");
                    let places = [(NotePlace::ByPanel, "By the Panel"), (NotePlace::TopLeft, "Top Left"), (NotePlace::TopRight, "Top Right"), (NotePlace::BottomLeft, "Bottom Left"), (NotePlace::BottomRight, "Bottom Right")];
                    let shown = places.iter().find(|(p, _)| *p == s.note_place).map_or("", |(_, l)| l);
                    egui::ComboBox::from_id_salt("shell-note-place").selected_text(shown).width(200.0).show_ui(ui, |ui| {
                        for (p, l) in places {
                            ui.selectable_value(&mut s.note_place, p, l);
                        }
                    });
                    ui.end_row();
                    ui.label("Width");
                    panel_editor::segmented(ui, &mut s.note_width, &[(LaunchSize::Small, "Narrow"), (LaunchSize::Medium, "Medium"), (LaunchSize::Large, "Wide")]);
                    ui.end_row();
                    ui.label("Stays for");
                    ui.add(egui::Slider::new(&mut s.note_secs, 2..=60).suffix(" s"));
                    ui.end_row();
                    ui.label("At most");
                    ui.add(egui::Slider::new(&mut s.note_max, 1..=8).suffix(" on screen"));
                    ui.end_row();
                });
                ui.add_space(6.0);
                hint(ui, "Apps that ask for their own time keep it, and urgent ones stay until you close them.");
            });
        });
        self.note_look_section(ui);
        section(ui, "Volume and brightness pop-up", &mut |ui| {
            controls::master(ui, &mut self.s.osd, "Show volume and brightness as they change");
            controls::rule(ui);
            let on = self.s.osd;
            let s = &mut self.s;
            ui.add_enabled_ui(on, |ui| {
                controls::switch(ui, &mut s.osd_keys, "Show Caps Lock and Num Lock turning on and off");
                ui.add_space(8.0);
                grid(ui, "shell-osd", &mut |ui| {
                    use crate::config::{LaunchSize, OsdPlace};
                    use crate::style::Role;
                    ui.label("Where");
                    panel_editor::segmented(ui, &mut s.osd_place, &[(OsdPlace::Top, "Top"), (OsdPlace::Middle, "Middle"), (OsdPlace::Bottom, "Bottom")]);
                    ui.end_row();
                    ui.label("Size");
                    panel_editor::segmented(ui, &mut s.osd_size, &[(LaunchSize::Small, "Small"), (LaunchSize::Medium, "Medium"), (LaunchSize::Large, "Large")]);
                    ui.end_row();
                    ui.label("Bar color");
                    panel_editor::segmented(ui, &mut s.osd_color, &[(None, "Like Sliders"), (Some(Role::Primary), "Primary"), (Some(Role::Secondary), "Secondary"), (Some(Role::Tertiary), "Tertiary")]);
                    ui.end_row();
                    ui.label("Stays for");
                    ui.add(egui::Slider::new(&mut s.osd_secs, 0.5..=5.0).step_by(0.5).fixed_decimals(1).suffix(" s"));
                    ui.end_row();
                });
            });
        });
        section(ui, "Night Light", &mut |ui| {
            grid(ui, "shell-night", &mut |ui| {
                ui.label("Warmth");
                ui.add(egui::Slider::new(&mut self.s.night_kelvin, 2500..=6000).step_by(100.0).suffix(" K"));
                ui.end_row();
            });
            let tool = ["gammastep", "hyprsunset", "wlsunset"].into_iter().find(|t| crate::sys::has(t));
            hint(ui, &match tool {
                Some(_) => "Turn Night Light on in quick settings.".to_string(),
                None => "Needs gammastep, hyprsunset or wlsunset. Install one to use Night Light.".to_string(),
            });
        });
        section(ui, "Keys", &mut |ui| {
            hint(ui, "Bind these in your compositor's settings.");
            ui.add_space(6.0);
            for (cmd, what) in [
                ("rivet-shell msg launcher", "The launcher"),
                ("rivet-shell msg control-center", "Quick settings"),
                ("rivet-shell msg volume up | down | mute", "Volume"),
                ("rivet-shell msg brightness up | down", "Brightness"),
                ("rivet-shell msg media play-pause | next | previous", "Music and video"),
                ("rivet-shell msg dnd", "Do Not Disturb"),
                ("rivet-shell msg wallpaper random | next | previous", "Another wallpaper"),
                ("rivet-shell msg power", "The power menu"),
                ("rivet-shell msg lock", "Lock the screen"),
            ] {
                controls::command(ui, what, cmd);
            }
        });
    }

    fn dock_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let knows_windows = knows_windows();
        section(ui, "Dock", &mut |ui| {
            controls::master(ui, &mut self.s.dock, "Show the dock");
            controls::rule(ui);
            ui.add_enabled_ui(self.s.dock, |ui| {
                grid(ui, "shell-dock", &mut |ui| {
                    ui.label("Icon size");
                    ui.add(egui::Slider::new(&mut self.s.dock_icon, 28.0..=80.0).step_by(2.0).suffix(" pt"));
                    ui.end_row();
                    ui.label("Hide");
                    panel_editor::segmented(ui, &mut self.s.dock_hide, &[(DockHide::Never, "Never"), (DockHide::Auto, "Auto-Hide"), (DockHide::Smart, "Smart Hide")]);
                    ui.end_row();
                });
                hint(
                    ui,
                    match self.s.dock_hide {
                        DockHide::Never => "",
                        DockHide::Auto => "The dock slides away until you move the pointer to the bottom edge.",
                        DockHide::Smart => if knows_windows { "The dock shows on an empty workspace and slides away when a window opens." } else { "Smart Hide needs niri, Hyprland or mango; here it works like Auto-Hide." },
                    },
                );
                controls::rule(ui);
                ui.add_enabled_ui(self.s.dock_hide == DockHide::Never, |ui| {
                    controls::switch(ui, &mut self.s.dock_reserve, "Keep windows clear of the dock");
                });
                if knows_windows {
                    controls::switch(ui, &mut self.s.dock_running, "A dot under apps with windows open");
                    controls::switch(ui, &mut self.s.dock_open_apps, "Show open apps that aren't pinned");
                }
            });
        });
        self.dock_look_section(ui);
        let pinned: Vec<(String, Option<DesktopApp>)> = self.s.dock_apps.iter().map(|id| (id.clone(), find_app(&self.apps, id).cloned())).collect();
        let icons: Vec<Option<egui::TextureHandle>> = pinned.iter().map(|(_, a)| a.as_ref().and_then(|a| self.app_texture(&ctx, a, 24.0))).collect();
        let mut op: Option<(usize, i32)> = None;
        let mut add: Option<String> = None;
        let candidates: Vec<(String, String)> = self.apps.iter().filter(|a| !self.s.dock_apps.iter().any(|d| find_app(&self.apps, d).is_some_and(|x| x.id == a.id))).filter(|a| self.add_filter.is_empty() || a.name.to_lowercase().contains(&self.add_filter.to_lowercase())).take(200).map(|a| (a.id.trim_end_matches(".desktop").to_string(), a.name.clone())).collect();
        section(ui, "Pinned apps", &mut |ui| {
            let n = pinned.len();
            for (i, (id, a)) in pinned.iter().enumerate() {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                    if let Some(t) = &icons[i] {
                        ui.painter().image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    match a {
                        Some(a) => ui.label(&a.name),
                        None => ui.label(egui::RichText::new(format!("{id} (Not Installed)")).color(ui.visuals().weak_text_color())),
                    };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if rivet_look::widgets::trash_button(ui).on_hover_text("Unpin").clicked() {
                            op = Some((i, 0));
                        }
                        if rivet_look::widgets::arrow_button(ui, false, i + 1 < n).on_hover_text("Move Right").clicked() {
                            op = Some((i, 1));
                        }
                        if rivet_look::widgets::arrow_button(ui, true, i > 0).on_hover_text("Move Left").clicked() {
                            op = Some((i, -1));
                        }
                    });
                });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.add_filter).hint_text("Find an app to pin…").desired_width(200.0));
                egui::ComboBox::from_id_salt("shell-dock-add").selected_text("Pin…").width(220.0).height(360.0).show_ui(ui, |ui| {
                    for (id, name) in &candidates {
                        if ui.selectable_label(false, name).clicked() {
                            add = Some(id.clone());
                        }
                    }
                });
            });
        });
        if let Some((i, d)) = op {
            match d {
                0 => {
                    self.s.dock_apps.remove(i);
                }
                -1 => self.s.dock_apps.swap(i, i - 1),
                _ => self.s.dock_apps.swap(i, i + 1),
            }
        }
        if let Some(id) = add {
            self.s.dock_apps.push(id);
        }
    }

    fn widgets_page(&mut self, ui: &mut egui::Ui) {
        section(ui, "On the desktop", &mut |ui| {
            controls::switch(ui, &mut self.s.widget_clock, "A large clock and the date");
            controls::switch(ui, &mut self.s.widget_system, "Processor, memory and battery");
            controls::rule(ui);
            grid(ui, "shell-widgets", &mut |ui| {
                ui.label("Side");
                panel_editor::segmented(ui, &mut self.s.widget_side, &[(Side::Left, "Left"), (Side::Right, "Right")]);
                ui.end_row();
            });
            ui.add_space(8.0);
            hint(ui, "Drag a widget on the desktop to move it, or double-click it to put it back.");
            if self.s.widget_clock_at.is_some() || self.s.widget_system_at.is_some() {
                ui.add_space(6.0);
                if controls::button(ui, controls::Icon::Reset, "Reset Positions").on_hover_text("Puts the widgets back on the side chosen above").clicked() {
                    (self.s.widget_clock_at, self.s.widget_system_at) = (None, None);
                }
            }
        });
    }

    fn about_page(&mut self, ui: &mut egui::Ui) {
        let weak = ui.visuals().weak_text_color();
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            if let Some(t) = &self.app_icon {
                ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(96.0)));
            }
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Rivet Shell").size(22.0).strong());
            ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(weak));
            ui.add_space(14.0);
            // The description in a column of its own, 420 points wide, in the page's middle.
            let w = ui.available_width().min(420.0);
            ui.allocate_ui_with_layout(Vec2::new(w, 0.0), egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.set_width(w);
                ui.add(egui::Label::new("A panel, dock, desktop widgets and wallpaper for Wayland compositors that support layer-shell, styled to match the Rivet apps.").halign(egui::Align::Center).wrap());
            });
            ui.add_space(10.0);
            let keys = egui::RichText::new("Bind a key to open the launcher:  rivet-shell msg launcher").font(fonts::ui_font(12.0)).color(weak);
            ui.label(keys);
            ui.add_space(14.0);
            ui.hyperlink_to("github.com/ulinaaron/rivet-apps", "https://github.com/ulinaaron/rivet-apps");
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Licensed under MIT or Apache-2.0").small().color(weak));
        });
        let _ = r_xs();
    }
}

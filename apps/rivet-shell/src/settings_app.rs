//! `rivet-shell settings`: the shell's settings in a window like the other Rivet apps'
//! Preferences (a sidebar of pages, titled cards). Every change is saved at once; the running
//! shell notices the file change and follows.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::icons::IconTheme;
use rivet_look::live::LiveLook;
use rivet_look::theme::{ACCENTS, Mode, r_md, r_xs, selected_tile};
use rivet_look::{fonts, palette};
use rivet_xdg::launch::DesktopApp;

use crate::config::{DockHide, Settings, Side, Transition};
use crate::ui::find_app;

mod displays;
mod item_opts;
mod launcher;
mod nav;
mod look_opts;
mod font_pick;
mod wallpaper;
mod idle_lock;
mod palette_pick;
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
            Page::Wallpaper => "Theme",
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
    icons: IconTheme,
    page: Page,
    apps: Vec<DesktopApp>,
    /// The wallpaper folder's pictures and thumbnails.
    picker: crate::wall_pick::Picker,
    /// The file picker's answer, on its way.
    picking: Option<Receiver<(crate::portal::Pick, Result<Option<PathBuf>, String>)>>,
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
            // Checks: a look for the panels and dock (as in the settings file).
            if let Ok(l) = std::env::var("RIVET_SHELL_LOOK") {
                s.look = crate::style::Style::parse(&l, '=', ';');
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
            icons: IconTheme::new(&rivet_look::icons::detect_system_theme()),
            page,
            apps,
            picker: Default::default(),
            picking: None,
            wall_screen: std::env::var("RIVET_SHELL_WALL_SCREEN").ok().filter(|_| shot.is_some()).unwrap_or_default(),
            add_filter: String::new(),
            screens: if shot.is_some() { vec!["DP-1".into(), "HDMI-A-1".into()] } else { crate::sys::screens(crate::sys::Compositor::detect()) },
            panel_sel: 0,
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
    ui.label(egui::RichText::new(title).strong().size(14.5));
    ui.add_space(4.0);
    let fr = egui::Frame::NONE.fill(fill).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(18, 16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        body(ui);
    });
    search::mark(ui, title, fr.response.rect);
    ui.add_space(22.0);
}

fn hint(ui: &mut egui::Ui, text: &str) {
    let weak = ui.visuals().weak_text_color();
    ui.add(egui::Label::new(egui::RichText::new(text).small().color(weak)).wrap());
}

fn grid(ui: &mut egui::Ui, id: &str, body: &mut dyn FnMut(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([24.0, 16.0]).min_col_width(140.0).show(ui, |ui| body(ui));
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
        self.look.set_fonts(&ctx, &self.s.font, "", "");
        self.look.frame(&ctx, &self.s.choice(), live);
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
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 3.0;
                                ui.label(egui::RichText::new(self.page.label()).size(21.0).strong());
                                ui.label(egui::RichText::new(nav::about(self.page)).size(12.5).color(v.weak_text_color()));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let running = self.shot.is_some() || crate::ipc::running();
                                if running {
                                    // A small pill: a dot and the word.
                                    let ok = rivet_look::theme::status(ui.ctx()).success;
                                    let g = ui.painter().layout_no_wrap("Shell running".into(), fonts::ui_font(11.5), ok);
                                    let (r, _) = ui.allocate_exact_size(Vec2::new(g.size().x + 30.0, 24.0), Sense::hover());
                                    ui.painter().rect_filled(r, 12.0, palette::mix(v.panel_fill, ok, 0.13));
                                    ui.painter().circle_filled(Pos2::new(r.left() + 12.0, r.center().y), 3.0, ok);
                                    ui.painter().galley(Pos2::new(r.left() + 20.0, r.center().y - g.size().y / 2.0), g, ok);
                                } else if ui.button("Start the Shell").on_hover_text("Run rivet-shell in this session").clicked()
                                    && let Ok(exe) = std::env::current_exe()
                                {
                                    let _ = std::process::Command::new(exe).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
                                }
                            });
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
        // One source of colours at a time: the wallpaper's, turned on, ends a chosen palette.
        if self.s.matugen && !before.matugen {
            self.s.palette.clear();
        }
        if self.s != before {
            self.s.save();
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
        section(ui, "Colors from the wallpaper", &mut |ui| {
            ui.checkbox(&mut self.s.matugen, "Make the desktop's colors from the wallpaper (matugen)");
            hint(ui, "When the wallpaper changes, matugen makes a color scheme from it, and every app that follows matugen (the Rivet apps included) changes with it.");
            ui.add_space(10.0);
            ui.add_enabled_ui(self.s.matugen, |ui| {
                grid(ui, "shell-matugen", &mut |ui| {
                    ui.label("Scheme");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.s.color_mode, Mode::Dark, "Dark");
                        ui.selectable_value(&mut self.s.color_mode, Mode::Light, "Light");
                    });
                    ui.end_row();
                    ui.label("Style");
                    let shown = self.s.scheme_type.trim_start_matches("scheme-").replace('-', " ");
                    egui::ComboBox::from_id_salt("shell-scheme-type").selected_text(shown).width(200.0).show_ui(ui, |ui| {
                        for t in ["tonal-spot", "content", "expressive", "fidelity", "fruit-salad", "monochrome", "neutral", "rainbow", "vibrant"] {
                            ui.selectable_value(&mut self.s.scheme_type, format!("scheme-{t}"), t.replace('-', " "));
                        }
                    });
                    ui.end_row();
                    ui.label("Contrast");
                    ui.horizontal(|ui| {
                        ui.add(egui::Slider::new(&mut self.s.matugen_contrast, -1.0..=1.0).step_by(0.05).fixed_decimals(2)).on_hover_text("Lower for softer colors, higher for stronger ones; 0 is the scheme as designed");
                        if self.s.matugen_contrast != 0.0 && ui.small_button("Reset").clicked() {
                            self.s.matugen_contrast = 0.0;
                        }
                    });
                    ui.end_row();
                    // With pictures per screen: whose picture the colors come from.
                    if !self.s.wallpaper_screens.is_empty() || self.screens.len() > 1 {
                        ui.label("Made from");
                        let shown = if self.s.matugen_screen.is_empty() { "The shared picture".to_string() } else { format!("{}'s picture", self.s.matugen_screen) };
                        egui::ComboBox::from_id_salt("shell-matugen-screen").selected_text(shown).width(200.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.s.matugen_screen, String::new(), "The shared picture");
                            for name in self.wall_screens() {
                                let own = self.s.has_own_wallpaper(&name);
                                ui.selectable_value(&mut self.s.matugen_screen, name.clone(), if own { format!("{name}'s picture") } else { format!("{name} (shows the shared picture)") });
                            }
                        });
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                if ui.button("Add Rivet's Colors to matugen").on_hover_text("Adds Rivet's template to matugen's config, so matugen writes the color file Rivet apps read").clicked() {
                    self.note = Some(match palette::install_matugen_template() {
                        Ok(m) => m,
                        Err(e) => format!("Couldn't update matugen's config: {e}"),
                    });
                }
                if let Some(n) = &self.note {
                    hint(ui, n);
                }
            });
        });
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
        self.picture_section(ui, accent);
    }

    fn appearance_page(&mut self, ui: &mut egui::Ui) {
        section(ui, "Style and accent", &mut |ui| {
            ui.checkbox(&mut self.s.follow_desktop_colors, "Follow desktop colors (matugen)");
            ui.add_space(8.0);
            let manual = !self.s.follow_desktop_colors;
            grid(ui, "shell-look", &mut |ui| {
                ui.label("Style");
                ui.add_enabled_ui(manual, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.s.theme, Mode::System, "Follow system");
                        ui.selectable_value(&mut self.s.theme, Mode::Light, "Light");
                        ui.selectable_value(&mut self.s.theme, Mode::Dark, "Dark");
                    });
                });
                ui.end_row();
                ui.label("Accent");
                ui.add_enabled_ui(manual, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        for (c, name) in ACCENTS {
                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                            let col = Color32::from_rgb(c[0], c[1], c[2]);
                            ui.painter().circle_filled(r.center(), 9.0, if manual { col } else { col.gamma_multiply(0.4) });
                            if self.s.accent == c {
                                ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(2.0, ui.visuals().strong_text_color()));
                            }
                            if resp.on_hover_text(name).clicked() && manual {
                                self.s.accent = c;
                            }
                        }
                    });
                });
                ui.end_row();
                ui.label("Font");
                ui.horizontal(|ui| {
                    font_pick::font_pick(ui, "shell-font", &mut self.s.font, &format!("Like the desktop ({})", font_pick::desktop_name()));
                    if !self.s.font.is_empty() && ui.small_button("Reset").clicked() {
                        self.s.font.clear();
                    }
                });
                ui.end_row();
                ui.label("Text size");
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.s.text_size == 0.0, "Like the desktop").clicked() {
                        self.s.text_size = 0.0;
                    }
                    for (pt, label) in [(12.5, "Small"), (13.5, "Medium"), (15.0, "Large")] {
                        if ui.selectable_label(self.s.text_size == pt, label).clicked() {
                            self.s.text_size = pt;
                        }
                    }
                });
                ui.end_row();
            });
        });
        self.palette_section(ui);
        self.look_section(ui);
        self.window_look_section(ui);
    }

    fn panel_page(&mut self, ui: &mut egui::Ui) {
        self.panels_editor(ui);
    }

    fn notifications_page(&mut self, ui: &mut egui::Ui) {
        section(ui, "Notifications", &mut |ui| {
            ui.checkbox(&mut self.s.notifications, "Show apps' notifications");
            hint(ui, "Cards in the screen's corner, and a history in quick settings. If another notification program is running (mako, dunst or another shell), it keeps them instead.");
        });
        section(ui, "On-screen display", &mut |ui| {
            ui.checkbox(&mut self.s.osd, "Show volume and brightness as they change");
            ui.add_enabled(self.s.osd, egui::Checkbox::new(&mut self.s.osd_keys, "Show Caps Lock and Num Lock turning on and off"));
        });
        section(ui, "Night Light", &mut |ui| {
            grid(ui, "shell-night", &mut |ui| {
                ui.label("Warmth");
                ui.add(egui::Slider::new(&mut self.s.night_kelvin, 2500..=6000).step_by(100.0).suffix(" K"));
                ui.end_row();
            });
            let tool = ["gammastep", "hyprsunset", "wlsunset"].into_iter().find(|t| crate::sys::has(t));
            hint(ui, &match tool {
                Some(t) => format!("Turned on in quick settings; {t} makes the colors warmer."),
                None => "Needs gammastep, hyprsunset or wlsunset installed.".to_string(),
            });
        });
        section(ui, "Keys", &mut |ui| {
            hint(ui, "Bind these in your compositor's settings:");
            ui.add_space(4.0);
            let weak = ui.visuals().weak_text_color();
            for (cmd, what) in [
                ("rivet-shell msg launcher", "Apps"),
                ("rivet-shell msg control-center", "Quick settings"),
                ("rivet-shell msg volume up | down | mute", "Volume"),
                ("rivet-shell msg brightness up | down", "Brightness"),
                ("rivet-shell msg media play-pause | next | previous", "Music and video"),
                ("rivet-shell msg dnd", "Do Not Disturb"),
                ("rivet-shell msg wallpaper random | next | previous", "Another wallpaper from the folder"),
                ("rivet-shell msg power", "Lock, suspend, restart, shut down"),
                ("rivet-shell msg lock", "Lock the screen"),
            ] {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(cmd).monospace().size(12.0));
                    ui.label(egui::RichText::new(what).color(weak).size(12.0));
                });
            }
        });
    }

    fn dock_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        section(ui, "Dock", &mut |ui| {
            ui.checkbox(&mut self.s.dock, "Show the dock");
            ui.add_space(8.0);
            ui.add_enabled_ui(self.s.dock, |ui| {
                grid(ui, "shell-dock", &mut |ui| {
                    ui.label("Icon size");
                    ui.add(egui::Slider::new(&mut self.s.dock_icon, 28.0..=80.0).step_by(2.0).suffix(" pt"));
                    ui.end_row();
                    ui.label("Hide");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.s.dock_hide, DockHide::Never, "Never");
                        ui.selectable_value(&mut self.s.dock_hide, DockHide::Auto, "Auto-hide");
                        ui.selectable_value(&mut self.s.dock_hide, DockHide::Smart, "Smart hide");
                    });
                    ui.end_row();
                });
                hint(
                    ui,
                    match self.s.dock_hide {
                        DockHide::Never => "The dock always shows.",
                        DockHide::Auto => "The dock slides away; move the pointer to the bottom edge to bring it back.",
                        DockHide::Smart => "The dock shows while the workspace has no windows, and slides away once it has some; the bottom edge brings it back (niri, Hyprland; elsewhere like Auto-hide).",
                    },
                );
                ui.add_space(6.0);
                ui.add_enabled_ui(self.s.dock_hide == DockHide::Never, |ui| {
                    ui.checkbox(&mut self.s.dock_reserve, "Keep windows clear of the dock");
                });
                ui.checkbox(&mut self.s.dock_running, "A dot under apps with windows open (niri, Hyprland)");
                ui.checkbox(&mut self.s.dock_open_apps, "Open apps that aren't pinned show too (right-click one to pin it)");
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
                        None => ui.label(egui::RichText::new(format!("{id} (not installed)")).color(ui.visuals().weak_text_color())),
                    };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if rivet_look::widgets::trash_button(ui).on_hover_text("Unpin").clicked() {
                            op = Some((i, 0));
                        }
                        if rivet_look::widgets::arrow_button(ui, false, i + 1 < n).on_hover_text("Move right").clicked() {
                            op = Some((i, 1));
                        }
                        if rivet_look::widgets::arrow_button(ui, true, i > 0).on_hover_text("Move left").clicked() {
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
            ui.checkbox(&mut self.s.widget_clock, "A large clock and the date");
            ui.checkbox(&mut self.s.widget_system, "Processor, memory and battery");
            ui.add_space(8.0);
            grid(ui, "shell-widgets", &mut |ui| {
                ui.label("Side");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.s.widget_side, Side::Left, "Left");
                    ui.selectable_value(&mut self.s.widget_side, Side::Right, "Right");
                });
                ui.end_row();
            });
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
                ui.add(egui::Label::new("A panel, dock, desktop widgets and wallpaper for Wayland compositors with the layer-shell protocol, in the Rivet apps' look.").halign(egui::Align::Center).wrap());
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

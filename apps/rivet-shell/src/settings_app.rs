//! `rivet-shell settings`: the shell's settings in a window like the other Rivet apps'
//! Preferences (a sidebar of pages, titled cards). Every change is saved at once; the running
//! shell notices the file change and follows.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::icons::IconTheme;
use rivet_look::live::LiveLook;
use rivet_look::theme::{ACCENTS, Mode, r_md, r_sm, r_xs, selected_tile};
use rivet_look::{fonts, palette};
use rivet_xdg::launch::DesktopApp;

use crate::config::{DockHide, Settings, Side};
use crate::ui::find_app;

mod panel_editor;

pub const ICON_SVG: &str = include_str!("../data/rivet-shell.svg");

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Page {
    #[default]
    Wallpaper,
    Appearance,
    Panel,
    Dock,
    Widgets,
    Notifications,
    About,
}

impl Page {
    const ALL: [Page; 7] = [Page::Wallpaper, Page::Appearance, Page::Panel, Page::Dock, Page::Widgets, Page::Notifications, Page::About];
    fn label(self) -> &'static str {
        match self {
            Page::Wallpaper => "Wallpaper",
            Page::Appearance => "Appearance",
            Page::Panel => "Panels",
            Page::Dock => "Dock",
            Page::Widgets => "Widgets",
            Page::Notifications => "Notifications",
            Page::About => "About",
        }
    }
    fn icons(self) -> &'static [&'static str] {
        match self {
            Page::Wallpaper => &["preferences-desktop-wallpaper-symbolic", "image-x-generic-symbolic", "folder-pictures-symbolic"],
            Page::Appearance => &["preferences-desktop-appearance-symbolic", "applications-graphics-symbolic"],
            Page::Panel => &["view-more-horizontal-symbolic", "open-menu-symbolic"],
            Page::Dock => &["view-grid-symbolic", "view-app-grid-symbolic"],
            Page::Widgets => &["view-compact-symbolic", "preferences-system-symbolic"],
            Page::Notifications => &["preferences-system-notifications-symbolic", "notification-symbolic", "dialog-information-symbolic"],
            Page::About => &["help-about-symbolic", "dialog-information-symbolic"],
        }
    }
}

type Thumb = (PathBuf, Option<egui::ColorImage>);

pub struct SettingsApp {
    s: Settings,
    look: LiveLook,
    icons: IconTheme,
    page: Page,
    apps: Vec<DesktopApp>,
    pictures: Vec<PathBuf>,
    thumbs: Vec<(PathBuf, Option<egui::TextureHandle>)>,
    thumb_rx: Option<Receiver<Thumb>>,
    listed_dir: String,
    add_filter: String,
    /// The screens panels can be put on (output names).
    screens: Vec<String>,
    note: Option<String>,
    app_icon: Option<egui::TextureHandle>,
    shot: Option<rivet_kit::Shot>,
    /// Checks: a folder of demo pictures (never the person's).
    demo_dir: Option<PathBuf>,
}

impl SettingsApp {
    pub fn new(_cc: &eframe::CreationContext, shot: Option<PathBuf>) -> SettingsApp {
        let mut s = Settings::load();
        let mut demo_dir = None;
        if shot.is_some() {
            crate::config::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
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
            demo_dir = Some(d);
            match std::env::var("RIVET_SHELL_THEME").as_deref() {
                Ok("light") => (s.theme, s.follow_desktop_colors) = (Mode::Light, false),
                Ok("dark") => (s.theme, s.follow_desktop_colors) = (Mode::Dark, false),
                _ => {}
            }
        }
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
            pictures: Vec::new(),
            thumbs: Vec::new(),
            thumb_rx: None,
            listed_dir: String::new(),
            add_filter: String::new(),
            screens: if shot.is_some() { vec!["DP-1".into(), "HDMI-A-1".into()] } else { crate::sys::screens(crate::sys::Compositor::detect()) },
            note: None,
            app_icon: None,
            shot: shot.map(rivet_kit::Shot::new),
            demo_dir,
        }
    }

    fn icon(&mut self, ctx: &egui::Context, names: &[&str], px: f32) -> Option<egui::TextureHandle> {
        self.icons.texture(ctx, names, (px * ctx.pixels_per_point()).round() as u32)
    }

    fn app_texture(&mut self, ctx: &egui::Context, a: &DesktopApp, px: f32) -> Option<egui::TextureHandle> {
        let size = (px * ctx.pixels_per_point()).round() as u32;
        let own = if a.icon.starts_with('/') { self.icons.texture_from_path(ctx, std::path::Path::new(&a.icon), size) } else if a.icon.is_empty() { None } else { self.icons.texture(ctx, &[a.icon.as_str()], size) };
        own.or_else(|| self.icons.texture(ctx, &["application-x-executable"], size))
    }

    /// The wallpaper folder's pictures, their thumbnails made on a thread.
    fn list_pictures(&mut self) {
        if self.listed_dir == self.s.wallpaper_dir {
            return;
        }
        self.listed_dir = self.s.wallpaper_dir.clone();
        self.pictures = crate::wall::pictures(std::path::Path::new(&self.s.wallpaper_dir));
        self.pictures.truncate(400);
        self.thumbs = self.pictures.iter().map(|p| (p.clone(), None)).collect();
        let (tx, rx) = channel();
        let list = self.pictures.clone();
        std::thread::spawn(move || {
            for p in list {
                let t = crate::wall::thumbnail(&p, 320);
                if tx.send((p, t)).is_err() {
                    return;
                }
            }
        });
        self.thumb_rx = Some(rx);
    }

    fn take_thumbs(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.thumb_rx else { return };
        let mut got = false;
        while let Ok((p, img)) = rx.try_recv() {
            got = true;
            if let (Some(img), Some(slot)) = (img, self.thumbs.iter_mut().find(|(q, _)| *q == p)) {
                slot.1 = Some(ctx.load_texture(format!("thumb-{}", p.display()), img, egui::TextureOptions::LINEAR));
            }
        }
        if got || self.thumbs.iter().any(|t| t.1.is_none()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
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
    egui::Frame::NONE.fill(fill).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(18, 16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        body(ui);
    });
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let live = self.shot.is_none();
        self.look.set_fonts(&ctx, "", "", "");
        self.look.frame(&ctx, &self.s.choice(), live);
        let v = ctx.global_style().visuals.clone();
        let accent = self.look.accent(&self.s.choice());
        self.icons.set_builtin(Some(rivet_look::builtin_icons::BuiltinColors { accent, bg: v.extreme_bg_color, fg: v.text_color(), dark: v.dark_mode }));
        if self.app_icon.is_none() {
            self.app_icon = rivet_look::icons::render_svg(ICON_SVG.as_bytes(), (96.0 * ctx.pixels_per_point()) as u32).map(|img| ctx.load_texture("rivet-shell-icon", img, egui::TextureOptions::LINEAR));
        }
        let before = self.s.clone();
        let tab_icons: Vec<Option<egui::TextureHandle>> = Page::ALL.iter().map(|p| self.icon(&ctx, p.icons(), 16.0)).collect();
        let side_fill = palette::mix(v.window_fill, v.panel_fill, 0.6);
        egui::Panel::left("shell-settings-pages").show_separator_line(false).resizable(false).exact_size(190.0).frame(egui::Frame::NONE.fill(side_fill).inner_margin(egui::Margin { left: 10, right: 10, top: 14, bottom: 8 })).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (i, p) in Page::ALL.iter().enumerate() {
                if *p == Page::About {
                    ui.add_space(10.0);
                }
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
                let sel = *p == self.page;
                if sel {
                    let (fill, stroke) = selected_tile(&v, side_fill);
                    ui.painter().rect_filled(r, r_sm(), fill);
                    ui.painter().rect_stroke(r, r_sm(), stroke, egui::StrokeKind::Inside);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                }
                let fg = if sel { v.strong_text_color() } else { v.text_color() };
                if let Some(t) = &tab_icons[i] {
                    rivet_look::theme::paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(r.left() + 20.0, r.center().y), Vec2::splat(16.0)), fg);
                }
                ui.painter().text(Pos2::new(r.left() + 40.0, r.center().y), egui::Align2::LEFT_CENTER, p.label(), fonts::ui_font(13.5), fg);
                if resp.clicked() {
                    self.page = *p;
                }
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(v.panel_fill)).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt(("shell-page", self.page as u8)).auto_shrink([false, false]).show(ui, |ui| {
                egui::Frame::NONE.inner_margin(egui::Margin { left: 26, right: 22, top: 18, bottom: 22 }).show(ui, |ui| {
                    ui.set_width(ui.available_width().max(380.0));
                    ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
                    ui.spacing_mut().button_padding = Vec2::new(10.0, 4.0);
                    if self.page != Page::About {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(self.page.label()).size(20.0).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let running = self.shot.is_some() || crate::ipc::running();
                                if running {
                                    ui.label(egui::RichText::new("● Shell running").small().color(rivet_look::theme::status(ui.ctx()).success));
                                } else if ui.button("Start the Shell").on_hover_text("Run rivet-shell in this session").clicked()
                                    && let Ok(exe) = std::env::current_exe()
                                {
                                    let _ = std::process::Command::new(exe).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
                                }
                            });
                        });
                        ui.add_space(14.0);
                    }
                    match self.page {
                        Page::Wallpaper => self.wallpaper_page(ui, accent),
                        Page::Appearance => self.appearance_page(ui),
                        Page::Panel => self.panel_page(ui),
                        Page::Dock => self.dock_page(ui),
                        Page::Widgets => self.widgets_page(ui),
                        Page::Notifications => self.notifications_page(ui),
                        Page::About => self.about_page(ui),
                    }
                });
            });
        });
        if self.s != before {
            self.s.save();
        }
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }
}

impl SettingsApp {
    fn wallpaper_page(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let ctx = ui.ctx().clone();
        self.list_pictures();
        self.take_thumbs(&ctx);
        section(ui, "Picture", &mut |ui| {
            ui.horizontal(|ui| {
                ui.label("Folder");
                let r = ui.add(egui::TextEdit::singleline(&mut self.s.wallpaper_dir).desired_width(ui.available_width() - 80.0));
                if r.lost_focus() {
                    self.listed_dir.clear();
                }
                if ui.button("Look").clicked() {
                    self.listed_dir.clear();
                }
            });
            ui.add_space(10.0);
            if self.thumbs.is_empty() {
                hint(ui, "No pictures in this folder (JPEG, PNG or WebP).");
            }
            let w = ui.available_width();
            let cols = ((w + 10.0) / 170.0).floor().max(1.0) as usize;
            let tw = (w - (cols as f32 - 1.0) * 10.0) / cols as f32;
            let th = tw * 0.62;
            let mut pick = None;
            egui::Grid::new("shell-walls").spacing([10.0, 10.0]).show(ui, |ui| {
                for (k, (p, t)) in self.thumbs.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(tw, th), Sense::click());
                    let on = self.s.wallpaper == p.to_string_lossy();
                    match t {
                        Some(t) => {
                            // Cover the cell, cropping the picture's longer side.
                            let ts = t.size_vec2();
                            let (ax, ay) = (r.width() / r.height(), ts.x / ts.y);
                            let uv = if ay > ax { let f = ax / ay; Rect::from_min_max(Pos2::new((1.0 - f) / 2.0, 0.0), Pos2::new((1.0 + f) / 2.0, 1.0)) } else { let f = ay / ax; Rect::from_min_max(Pos2::new(0.0, (1.0 - f) / 2.0), Pos2::new(1.0, (1.0 + f) / 2.0)) };
                            ui.painter().add(egui::epaint::RectShape::filled(r, r_md(), Color32::WHITE).with_texture(t.id(), uv));
                        }
                        None => {
                            ui.painter().rect_filled(r, r_md(), ui.visuals().faint_bg_color);
                        }
                    }
                    if on {
                        ui.painter().rect_stroke(r, r_md(), Stroke::new(3.0, accent), egui::StrokeKind::Inside);
                    } else if resp.hovered() {
                        ui.painter().rect_stroke(r, r_md(), Stroke::new(2.0, ui.visuals().text_color().gamma_multiply(0.5)), egui::StrokeKind::Inside);
                    }
                    if resp.on_hover_text(p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).clicked() {
                        pick = Some(p.clone());
                    }
                    if (k + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
            if let Some(p) = pick {
                self.s.wallpaper = p.to_string_lossy().into_owned();
            }
            if !self.s.wallpaper.is_empty() {
                ui.add_space(8.0);
                if ui.button("No Wallpaper (a plain background)").clicked() {
                    self.s.wallpaper.clear();
                }
            }
        });
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
                ("rivet-shell msg power", "Lock, suspend, restart, shut down"),
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
            ui.set_max_width(420.0);
            ui.add(egui::Label::new("A panel, dock, desktop widgets and wallpaper for Wayland compositors with the layer-shell protocol, in the Rivet apps' look.").halign(egui::Align::Center).wrap());
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

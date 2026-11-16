//! Preferences, Keyboard Shortcuts and About: in-app windows drawn like Rivet's.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{self, ACCENTS, Mode, paint_tex, r_lg, r_md, r_sm, r_xs, selected_tile};
use rivet_look::{fonts, palette, windows};

use super::menus::{Cmd, GLOBAL, LIST, shortcut_for};
use super::*;
use crate::settings::OnExists;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Page {
    Appearance,
    Transfers,
    Connections,
    About,
}

impl Page {
    const ALL: [Page; 4] = [Page::Appearance, Page::Transfers, Page::Connections, Page::About];
    fn label(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::Transfers => "Transfers",
            Page::Connections => "Connections",
            Page::About => "About",
        }
    }
    fn icons(self) -> &'static [&'static str] {
        match self {
            Page::Appearance => &["preferences-desktop-appearance-symbolic", "applications-graphics-symbolic", "preferences-desktop-theme"],
            Page::Transfers => &["network-transmit-receive-symbolic", "emblem-synchronizing-symbolic", "view-refresh-symbolic"],
            Page::Connections => &["network-server-symbolic", "network-workgroup-symbolic", "folder-remote-symbolic"],
            Page::About => &["help-about-symbolic", "dialog-information-symbolic"],
        }
    }
}

pub const ICON_SVG: &str = include_str!("../../data/rivet-ftp.svg");

impl FtpApp {
    pub(super) fn app_icon(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if self.app_icon.is_none() {
            let px = (96.0 * ctx.pixels_per_point()) as u32;
            self.app_icon = rivet_look::icons::render_svg(ICON_SVG.as_bytes(), px).map(|img| ctx.load_texture("rivet-ftp-icon", img, egui::TextureOptions::LINEAR));
        }
        self.app_icon.clone()
    }

    fn chrome(&mut self, ctx: &egui::Context, title: &str, icon: &[&str]) -> windows::Chrome {
        windows::Chrome { title: title.into(), icon: self.named_icon(ctx, icon), close: self.named_icon(ctx, &["window-close-symbolic"]), flush: false }
    }

    pub(super) fn windows(&mut self, ctx: &egui::Context) {
        if self.prefs_open {
            self.prefs_window(ctx);
        }
        if self.keys_open {
            self.keys_window(ctx);
        }
    }

    fn prefs_window(&mut self, ctx: &egui::Context) {
        let mut chrome = self.chrome(ctx, "Preferences", &["preferences-system-symbolic", "emblem-system-symbolic"]);
        chrome.flush = true;
        let area = ctx.content_rect().shrink(6.0);
        let tab_icons: Vec<Option<egui::TextureHandle>> = Page::ALL.iter().map(|p| self.named_icon(ctx, p.icons())).collect();
        let about_icon = self.app_icon(ctx);
        let mut win = std::mem::replace(&mut self.prefs_win, windows::WinState::new(0, Vec2::ZERO));
        let page_now = self.prefs_page;
        let mut pick: Option<Page> = None;
        let before = self.s.clone();
        let shown = windows::show(ctx, &mut win, &chrome, area, Vec2::new(600.0, 380.0), |ui| {
            let v = ui.visuals().clone();
            let weak = v.weak_text_color();
            let full_h = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // ---- the pages
                let side_fill = palette::mix(v.window_fill, v.panel_fill, 0.6);
                egui::Frame::NONE
                    .fill(side_fill)
                    .corner_radius(egui::CornerRadius { sw: r_lg(), ..Default::default() })
                    .inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 4 })
                    .show(ui, |ui| {
                        ui.set_width(168.0);
                        ui.set_min_height(full_h - 10.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            for (i, p) in Page::ALL.iter().enumerate() {
                                if *p == Page::About {
                                    ui.add_space(10.0);
                                }
                                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
                                let sel = *p == page_now;
                                if sel {
                                    let (fill, stroke) = selected_tile(&v, side_fill);
                                    ui.painter().rect_filled(r, r_sm(), fill);
                                    ui.painter().rect_stroke(r, r_sm(), stroke, egui::StrokeKind::Inside);
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                                }
                                let fg = if sel { v.strong_text_color() } else { v.text_color() };
                                if let Some(t) = &tab_icons[i] {
                                    paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(r.left() + 20.0, r.center().y), Vec2::splat(16.0)), fg);
                                }
                                ui.painter().text(Pos2::new(r.left() + 40.0, r.center().y), egui::Align2::LEFT_CENTER, p.label(), fonts::ui_font(13.5), fg);
                                if resp.clicked() {
                                    pick = Some(*p);
                                }
                            }
                        });
                    });
                // ---- the page
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical().id_salt(("ftp-prefs", page_now as u8)).auto_shrink([false, false]).show(ui, |ui| {
                        egui::Frame::NONE.inner_margin(egui::Margin { left: 26, right: 22, top: 18, bottom: 22 }).show(ui, |ui| {
                            ui.set_width(ui.available_width().max(360.0));
                            ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
                            ui.spacing_mut().button_padding = Vec2::new(10.0, 4.0);
                            let card_fill = v.faint_bg_color;
                            let section = |ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                                ui.label(egui::RichText::new(title).strong().size(14.5));
                                ui.add_space(4.0);
                                egui::Frame::NONE.fill(card_fill).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(18, 16)).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    body(ui);
                                });
                                ui.add_space(22.0);
                            };
                            let hint = |ui: &mut egui::Ui, text: &str| {
                                ui.add(egui::Label::new(egui::RichText::new(text).small().color(weak)).wrap());
                            };
                            if page_now != Page::About {
                                ui.label(egui::RichText::new(page_now.label()).size(20.0).strong());
                                ui.add_space(14.0);
                            }
                            match page_now {
                                Page::Appearance => {
                                    let pal = self.palette.clone();
                                    section(ui, "Desktop colors", &mut |ui| {
                                        ui.checkbox(&mut self.s.follow_desktop_colors, egui::RichText::new("Follow desktop colors").strong());
                                        ui.add_space(4.0);
                                        match (&pal, self.s.follow_desktop_colors) {
                                            (Some(p), true) => {
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    for c in [p.accent, p.window_bg, p.view_bg, p.window_fg].into_iter().flatten() {
                                                        let (r, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                                                        ui.painter().rect_filled(r, r_xs(), c);
                                                        ui.painter().rect_stroke(r, r_xs(), Stroke::new(1.0, weak), egui::StrokeKind::Inside);
                                                    }
                                                    ui.add_space(4.0);
                                                    let name = p.source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                                    ui.label(egui::RichText::new(format!("from {name}")).small().color(weak)).on_hover_text(p.source.display().to_string());
                                                });
                                            }
                                            (None, true) => hint(ui, "No desktop color file found, so the style and accent below are used."),
                                            _ => hint(ui, "Uses the style and accent below. The same colors as Rivet when both follow your desktop."),
                                        }
                                    });
                                    let manual = !self.s.follow_desktop_colors || pal.is_none();
                                    section(ui, "Style and accent", &mut |ui| {
                                        egui::Grid::new("ftp-look").num_columns(2).spacing([24.0, 18.0]).min_col_width(110.0).show(ui, |ui| {
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
                                        });
                                        if !manual {
                                            ui.add_space(6.0);
                                            hint(ui, "Your desktop colors are in charge. Turn off Follow desktop colors to pick a style and accent here.");
                                        }
                                    });
                                    section(ui, "Text", &mut |ui| {
                                        egui::Grid::new("ftp-text").num_columns(2).spacing([24.0, 14.0]).min_col_width(110.0).show(ui, |ui| {
                                            ui.label("Text size");
                                            ui.horizontal(|ui| {
                                                let desktop = self.s.text_size == 0.0;
                                                if ui.selectable_label(desktop, "Like the desktop").clicked() {
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
                                    section(ui, "Window", &mut |ui| {
                                        ui.checkbox(&mut self.s.show_menubar, "Show the menu bar");
                                        ui.checkbox(&mut self.s.show_sidebar, "Show saved connections");
                                        ui.checkbox(&mut self.s.show_queue, "Show transfers");
                                        ui.checkbox(&mut self.s.show_hidden, "Show hidden files");
                                    });
                                }
                                Page::Transfers => {
                                    section(ui, "When a file is already there", &mut |ui| {
                                        ui.horizontal(|ui| {
                                            ui.selectable_value(&mut self.s.on_exists, OnExists::Ask, "Ask");
                                            ui.selectable_value(&mut self.s.on_exists, OnExists::Replace, "Replace it");
                                            ui.selectable_value(&mut self.s.on_exists, OnExists::Skip, "Skip it");
                                        });
                                        hint(ui, "Applies to files and folders with the same name in the folder you're sending to.");
                                    });
                                    section(ui, "Behavior", &mut |ui| {
                                        ui.checkbox(&mut self.s.double_click_sends, "Double-clicking a file sends it to the other side");
                                        ui.checkbox(&mut self.s.confirm_delete, "Ask before deleting on the server");
                                        ui.checkbox(&mut self.s.clear_finished, "Clear finished transfers from the list");
                                        ui.checkbox(&mut self.s.remember_local, "Start this computer's pane where you left it");
                                    });
                                }
                                Page::Connections => {
                                    let store_path = self.store.path.clone();
                                    let ssh_path = self.store.ssh_config_path();
                                    let n = self.store.items.len();
                                    section(ui, "Saved connections", &mut |ui| {
                                        ui.label(format!("{n} saved. They're kept in a plain text file that only you can read:"));
                                        ui.label(egui::RichText::new(tilde(&store_path)).monospace().color(weak));
                                        ui.add_space(4.0);
                                        hint(ui, "Passwords are never written there. SSH logins use your keys or ssh agent.");
                                        ui.add_space(6.0);
                                        if ui.button("New Connection…").clicked() {
                                            self.form = Some(Form::blank());
                                        }
                                    });
                                    section(ui, "Use them from a terminal", &mut |ui| {
                                        ui.label("Log in to a saved connection, starting in its server folder:");
                                        ui.label(egui::RichText::new("rivet-ftp ssh NAME").monospace());
                                        ui.add_space(8.0);
                                        ui.label("Or reach them by name from plain ssh, scp, rsync and git: add this line near the top of ~/.ssh/config.");
                                        let line = format!("Include {}", tilde(&ssh_path));
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(&line).monospace());
                                            if ui.small_button("Copy").clicked() {
                                                ui.ctx().copy_text(line.clone());
                                            }
                                        });
                                        hint(ui, "Rivet FTP rewrites that file whenever you save a connection.");
                                    });
                                }
                                Page::About => {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(10.0);
                                        if let Some(t) = &about_icon {
                                            ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(88.0)));
                                        }
                                        ui.add_space(6.0);
                                        ui.label(egui::RichText::new("Rivet FTP").size(22.0).strong());
                                        ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(weak));
                                        ui.add_space(10.0);
                                        ui.label("Transfer files to and from your servers, side by side.");
                                        ui.label(egui::RichText::new("Built on Rivet Files. No GTK or Qt.").color(weak));
                                        ui.add_space(10.0);
                                        ui.hyperlink_to("Source code", "https://github.com/ulinaaron/rivet-files");
                                    });
                                }
                            }
                        });
                    });
                });
            });
        });
        self.prefs_win = win;
        if let Some(p) = pick {
            self.prefs_page = p;
        }
        if !shown.open {
            self.prefs_open = false;
        }
        if self.s != before {
            if self.s.show_hidden != before.show_hidden {
                self.reload_local();
                self.reload_remote();
            }
            self.s.save();
        }
    }

    fn keys_window(&mut self, ctx: &egui::Context) {
        let chrome = self.chrome(ctx, "Keyboard Shortcuts", &["preferences-desktop-keyboard-shortcuts-symbolic", "input-keyboard-symbolic"]);
        let area = ctx.content_rect().shrink(6.0);
        let mut win = std::mem::replace(&mut self.keys_win, windows::WinState::new(1, Vec2::ZERO));
        let shown = windows::show(ctx, &mut win, &chrome, area, Vec2::new(440.0, 420.0), |ui| {
            let v = ui.visuals().clone();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut rows: Vec<(&str, String)> = vec![
                    ("Move through the list", "Up / Down".into()),
                    ("Extend the selection", "Shift+Up / Down".into()),
                    ("Open a folder, or send a file across", "Enter".into()),
                ];
                for (_, _, c) in GLOBAL.iter().chain(LIST) {
                    rows.push((cmd_label(*c), shortcut_for(*c)));
                }
                egui::Grid::new("ftp-keys").num_columns(2).spacing([28.0, 10.0]).striped(false).show(ui, |ui| {
                    for (what, keys) in rows {
                        ui.label(what);
                        egui::Frame::NONE.fill(v.faint_bg_color).corner_radius(r_xs()).inner_margin(egui::Margin::symmetric(8, 2)).show(ui, |ui| {
                            ui.label(egui::RichText::new(keys).monospace());
                        });
                        ui.end_row();
                    }
                });
            });
        });
        self.keys_win = win;
        if !shown.open {
            self.keys_open = false;
        }
    }

    /// The look, from the settings and (when followed) the desktop's colors.
    pub(super) fn look(&self) -> (theme::Mode, Color32, Option<&palette::Palette>) {
        let pal = if self.s.follow_desktop_colors { self.palette.as_ref() } else { None };
        let a = self.s.accent;
        let accent = pal.and_then(|p| p.accent).unwrap_or(Color32::from_rgb(a[0], a[1], a[2]));
        (self.s.theme, accent, pal)
    }
}

fn cmd_label(c: Cmd) -> &'static str {
    match c {
        Cmd::NewConnection => "New connection",
        Cmd::QuickConnect => "Quick connect",
        Cmd::Disconnect => "Disconnect",
        Cmd::Preferences => "Preferences",
        Cmd::Quit => "Quit",
        Cmd::SelectAll => "Select everything",
        Cmd::Rename => "Rename",
        Cmd::Delete => "Delete on the server",
        Cmd::NewFolder => "New folder",
        Cmd::ToggleMenubar => "Show or hide the menu bar",
        Cmd::ToggleSidebar => "Show or hide saved connections",
        Cmd::ToggleQueue => "Show or hide transfers",
        Cmd::ToggleHidden => "Show or hide hidden files",
        Cmd::Refresh => "Refresh",
        Cmd::Style(_) => "Style",
        Cmd::Up => "Up a folder",
        Cmd::LocalHome => "Home folder",
        Cmd::ServerHome => "Server home",
        Cmd::SwitchPane => "Other pane",
        Cmd::TypePath => "Type a path",
        Cmd::Upload => "Upload the selection",
        Cmd::Download => "Download the selection",
        Cmd::ClearFinished => "Clear finished transfers",
        Cmd::Shortcuts => "Keyboard shortcuts",
        Cmd::About => "About",
    }
}

fn tilde(p: &std::path::Path) -> String {
    let home = rivet_xdg::places::home();
    match p.strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

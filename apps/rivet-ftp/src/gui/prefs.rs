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
    Keys,
    About,
}

impl Page {
    const ALL: [Page; 5] = [Page::Appearance, Page::Transfers, Page::Connections, Page::Keys, Page::About];
    fn label(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::Transfers => "Transfers",
            Page::Connections => "Connections",
            Page::Keys => "SSH Keys",
            Page::About => "About",
        }
    }
    fn icons(self) -> &'static [&'static str] {
        match self {
            Page::Appearance => &["preferences-desktop-appearance-symbolic", "applications-graphics-symbolic", "preferences-desktop-theme"],
            Page::Transfers => &["network-transmit-receive-symbolic", "emblem-synchronizing-symbolic", "view-refresh-symbolic"],
            Page::Connections => &["network-server-symbolic", "network-workgroup-symbolic", "folder-remote-symbolic"],
            Page::Keys => &["dialog-password-symbolic", "channel-secure-symbolic", "security-high-symbolic", "changes-prevent-symbolic"],
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
        if page_now == Page::Keys || page_now == Page::Connections {
            self.want_keys(ctx);
        }
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
                            // A label on the left and its control on the right, like Rivet's rows.
                            let grid = |ui: &mut egui::Ui, id: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                                egui::Grid::new(id).num_columns(2).spacing([24.0, 18.0]).min_col_width(150.0).show(ui, |ui| body(ui));
                            };
                            let mut remove: Option<String> = None;
                            let mut edit: Option<String> = None;
                            let mut open: Option<String> = None;
                            match page_now {
                                Page::Appearance => {
                                    let pal = self.palette.clone();
                                    section(ui, "Desktop colors", &mut |ui| {
                                        ui.checkbox(&mut self.s.follow_desktop_colors, "Follow desktop colors");
                                        ui.add_space(6.0);
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
                                            (None, true) => hint(ui, "No desktop color file was found, so the style and accent below are used."),
                                            _ => hint(ui, "The style and accent below are used."),
                                        }
                                    });
                                    let manual = !self.s.follow_desktop_colors || pal.is_none();
                                    section(ui, "Style and accent", &mut |ui| {
                                        grid(ui, "ftp-look", &mut |ui| {
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
                                        if !manual {
                                            ui.add_space(10.0);
                                            hint(ui, "Your desktop colors are in charge. Turn off Follow desktop colors to choose a style and accent here.");
                                        }
                                    });
                                    section(ui, "Window", &mut |ui| {
                                        ui.checkbox(&mut self.s.show_menubar, "Menu bar");
                                        ui.checkbox(&mut self.s.show_sidebar, "Saved connections beside the panes");
                                        ui.checkbox(&mut self.s.show_queue, "Transfers under the panes");
                                        ui.checkbox(&mut self.s.show_hidden, "Hidden files");
                                    });
                                }
                                Page::Transfers => {
                                    section(ui, "Sending files", &mut |ui| {
                                        grid(ui, "ftp-send", &mut |ui| {
                                            ui.label("If it's already there");
                                            ui.horizontal(|ui| {
                                                ui.selectable_value(&mut self.s.on_exists, OnExists::Ask, "Ask");
                                                ui.selectable_value(&mut self.s.on_exists, OnExists::Replace, "Replace");
                                                ui.selectable_value(&mut self.s.on_exists, OnExists::Skip, "Skip");
                                            });
                                            ui.end_row();
                                            ui.label("Double-click a file");
                                            ui.horizontal(|ui| {
                                                ui.selectable_value(&mut self.s.double_click_sends, true, "Sends it across");
                                                ui.selectable_value(&mut self.s.double_click_sends, false, "Only selects it");
                                            });
                                            ui.end_row();
                                        });
                                        ui.add_space(14.0);
                                        ui.checkbox(&mut self.s.clear_finished, "Remove finished transfers from the list");
                                    });
                                    section(ui, "Deleting", &mut |ui| {
                                        ui.checkbox(&mut self.s.confirm_delete, "Ask before deleting files on the server");
                                        hint(ui, "Files deleted on a server can't be brought back.");
                                    });
                                    section(ui, "This computer", &mut |ui| {
                                        grid(ui, "ftp-local", &mut |ui| {
                                            ui.label("Start in");
                                            ui.horizontal(|ui| {
                                                ui.selectable_value(&mut self.s.remember_local, true, "Where you left off");
                                                ui.selectable_value(&mut self.s.remember_local, false, "Your home folder");
                                            });
                                            ui.end_row();
                                        });
                                        hint(ui, "A saved connection with its own local folder always opens there.");
                                    });
                                }
                                Page::Connections => {
                                    let items = self.store.items.clone();
                                    let current = self.conn.as_ref().map(|c| c.name.clone());
                                    let server_icon = self.named_icon(ui.ctx(), &["network-server-symbolic", "network-workgroup-symbolic"]);
                                    section(ui, "Saved connections", &mut |ui| {
                                        if items.is_empty() {
                                            hint(ui, "None yet.");
                                        }
                                        for (i, c) in items.iter().enumerate() {
                                            if i > 0 {
                                                ui.add_space(4.0);
                                                ui.separator();
                                                ui.add_space(4.0);
                                            }
                                            ui.horizontal(|ui| {
                                                if let Some(t) = &server_icon {
                                                    let (r, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                                                    paint_tex(ui.painter(), t, r, v.text_color());
                                                }
                                                ui.add_space(4.0);
                                                ui.vertical(|ui| {
                                                    ui.spacing_mut().item_spacing.y = 1.0;
                                                    ui.label(egui::RichText::new(&c.name).strong());
                                                    let kind = c.protocol.scheme().to_uppercase();
                                                    let key = match (&c.key, c.protocol) {
                                                        (Some(k), _) => format!(" · key {}", std::path::Path::new(k).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                                                        (None, rivet_connections::Protocol::Sftp) => " · usual keys".into(),
                                                        _ => String::new(),
                                                    };
                                                    ui.label(egui::RichText::new(format!("{kind} · {}{key}", c.target())).small().color(weak));
                                                });
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    if ui.button("Remove").clicked() {
                                                        remove = Some(c.name.clone());
                                                    }
                                                    if ui.button("Edit…").clicked() {
                                                        edit = Some(c.name.clone());
                                                    }
                                                    let here = current.as_deref() == Some(c.name.as_str());
                                                    if ui.add_enabled(!here, egui::Button::new(if here { "Connected" } else { "Connect" })).clicked() {
                                                        open = Some(c.name.clone());
                                                    }
                                                });
                                            });
                                        }
                                        ui.add_space(12.0);
                                        ui.horizontal(|ui| {
                                            if ui.button("New Connection…").clicked() {
                                                self.form = Some(Form::blank());
                                            }
                                        });
                                    });
                                    let store_path = tilde(&self.store.path);
                                    let ssh_path = tilde(&self.store.ssh_config_path());
                                    let example = items.iter().find(|c| c.ssh_config_block().is_some()).map(|c| c.alias()).unwrap_or_else(|| "NAME".into());
                                    section(ui, "From a terminal", &mut |ui| {
                                        grid(ui, "ftp-term", &mut |ui| {
                                            ui.label("Log in with ssh");
                                            chip(ui, &format!("rivet-ftp ssh {example}"));
                                            ui.end_row();
                                            ui.label("Use plain ssh, scp, rsync and git");
                                            chip(ui, &format!("Include {ssh_path}"));
                                            ui.end_row();
                                        });
                                        ui.add_space(10.0);
                                        hint(ui, "Put the Include line near the top of ~/.ssh/config, and saved connections answer to their short names. Rivet FTP rewrites that file whenever you save a connection.");
                                    });
                                    section(ui, "Where they're kept", &mut |ui| {
                                        ui.label(egui::RichText::new(&store_path).monospace().size(12.5));
                                        ui.add_space(2.0);
                                        hint(ui, "Plain text that only you can read. Passwords are never written there: SSH logins use your keys or ssh agent.");
                                    });
                                }
                                Page::Keys => {
                                    let keys = self.keys.clone();
                                    let loading = self.keys_rx.is_some();
                                    let sftp: Vec<rivet_connections::Connection> = self.store.items.iter().filter(|c| c.protocol == rivet_connections::Protocol::Sftp).cloned().collect();
                                    let key_icon = self.named_icon(ui.ctx(), &["dialog-password-symbolic", "channel-secure-symbolic", "security-high-symbolic"]);
                                    let users: Vec<Vec<String>> = keys.iter().map(|k| self.key_users(k)).collect();
                                    let mut copy_id: Option<String> = None;
                                    section(ui, "Your keys", &mut |ui| {
                                        if loading && keys.is_empty() {
                                            ui.horizontal(|ui| {
                                                ui.add(egui::Spinner::new().size(14.0));
                                                ui.label(egui::RichText::new("Looking in ~/.ssh…").color(weak));
                                            });
                                        } else if keys.is_empty() {
                                            ui.label("You don't have any SSH keys yet.");
                                            hint(ui, "A key lets you log in to servers without a password. Make one below, then add it to your servers.");
                                        }
                                        for (i, k) in keys.iter().enumerate() {
                                            if i > 0 {
                                                ui.add_space(6.0);
                                                ui.separator();
                                                ui.add_space(6.0);
                                            }
                                            ui.horizontal(|ui| {
                                                if let Some(t) = &key_icon {
                                                    let (r, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                                                    paint_tex(ui.painter(), t, r, v.text_color());
                                                }
                                                ui.add_space(4.0);
                                                // The buttons sit on the right; the text gets the rest.
                                                let text_w = (ui.available_width() - 270.0).max(160.0);
                                                ui.vertical(|ui| {
                                                    ui.set_width(text_w);
                                                    ui.spacing_mut().item_spacing.y = 3.0;
                                                    ui.horizontal(|ui| {
                                                        ui.spacing_mut().item_spacing.x = 6.0;
                                                        ui.label(egui::RichText::new(&k.name).strong());
                                                        badge(ui, &k.kind_label(), false);
                                                        match k.passphrase {
                                                            Some(true) => badge(ui, "Passphrase", false),
                                                            Some(false) => badge(ui, "No passphrase", false),
                                                            None => {}
                                                        }
                                                        if k.in_agent {
                                                            badge(ui, "In ssh agent", true);
                                                        }
                                                    });
                                                    let mut line = String::new();
                                                    if !k.comment.is_empty() {
                                                        line.push_str(&k.comment);
                                                    }
                                                    if !k.fingerprint.is_empty() {
                                                        if !line.is_empty() {
                                                            line.push_str("  ·  ");
                                                        }
                                                        line.push_str(&k.fingerprint);
                                                    }
                                                    ui.add(egui::Label::new(egui::RichText::new(line).small().color(weak)).truncate());
                                                    let used = &users[i];
                                                    let used_text = if used.is_empty() { "Not used by a saved connection".to_string() } else { format!("Used by {}", used.join(", ")) };
                                                    ui.label(egui::RichText::new(used_text).small().color(weak));
                                                });
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                if ui.add_enabled(k.public.is_some(), egui::Button::new("Copy Public Key")).on_hover_text("The public half is safe to share: paste it into a server's or a code host's list of keys").clicked()
                                                    && let Some(pk) = &k.public
                                                {
                                                    ui.ctx().copy_text(pk.clone());
                                                    self.note = Some((format!("Copied the public key of {}", k.name), false));
                                                }
                                                ui.add_enabled_ui(k.public.is_some() && !sftp.is_empty(), |ui| {
                                                    ui.menu_button("Add to Server", |ui| {
                                                        super::menus::menu_style(ui);
                                                        ui.label(egui::RichText::new("Opens a terminal: log in once with your password").small().color(weak));
                                                        ui.separator();
                                                        for c in &sftp {
                                                            if ui.button(&c.name).on_hover_text(c.target()).clicked() {
                                                                copy_id = Some(rivet_connections::keys::copy_id_command(k, c));
                                                                ui.close();
                                                            }
                                                        }
                                                    })
                                                    .response
                                                    .on_disabled_hover_text("Save an SFTP connection first");
                                                });
                                                });
                                            });
                                        }
                                        ui.add_space(14.0);
                                        ui.horizontal(|ui| {
                                            if ui.button("New Key…").clicked() {
                                                self.new_key = Some(NewKey::new(false));
                                            }
                                            if ui.button("Look Again").on_hover_text("Read ~/.ssh again").clicked() {
                                                let ctx = ui.ctx().clone();
                                                self.reload_keys(&ctx);
                                            }
                                        });
                                    });
                                    section(ui, "How keys are used", &mut |ui| {
                                        hint(ui, "Choose a key for each connection in its settings (Edit…). Without one, ssh tries your ssh agent and its usual keys.");
                                        ui.add_space(4.0);
                                        hint(ui, "Keys stay in ~/.ssh. Rivet FTP never sends a private key anywhere; Add to Server sends only the public half.");
                                    });
                                    if let Some(cmd) = copy_id
                                        && let Err(e) = rivet_xdg::launch::run_shell(&cmd, None, true)
                                    {
                                        self.note = Some((format!("Couldn't open a terminal: {e}"), true));
                                    }
                                }
                                Page::About => {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(24.0);
                                        if let Some(t) = &about_icon {
                                            ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(96.0)));
                                        }
                                        ui.add_space(10.0);
                                        ui.label(egui::RichText::new("Rivet FTP").size(22.0).strong());
                                        ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(weak));
                                        ui.add_space(14.0);
                                        ui.label("Transfer files to and from your servers, side by side.");
                                        ui.label(egui::RichText::new("Built on Rivet Files. No GTK or Qt.").color(weak));
                                        ui.add_space(14.0);
                                        ui.hyperlink_to("Source code", "https://github.com/ulinaaron/rivet-files");
                                    });
                                }
                            }
                            if let Some(name) = remove
                                && let Some(gone) = self.store.remove(&name)
                            {
                                self.note = Some(match self.store.save() {
                                    Ok(()) => (format!("Removed \"{}\".", gone.name), false),
                                    Err(e) => (format!("Couldn't save: {e}"), true),
                                });
                            }
                            if let Some(name) = edit
                                && let Some(c) = self.store.find(&name)
                            {
                                self.form = Some(Form::from(c));
                            }
                            if let Some(name) = open
                                && let Some(c) = self.store.find(&name).cloned()
                            {
                                let ctx = ui.ctx().clone();
                                self.connect(c, &ctx);
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

/// A small rounded tag ("ED25519", "In ssh agent"); `accent` colors it.
fn badge(ui: &mut egui::Ui, text: &str, accent: bool) {
    let v = ui.visuals().clone();
    let (fill, fg) = if accent { (palette::mix(v.faint_bg_color, v.selection.bg_fill, 0.35), v.text_color()) } else { (palette::mix(v.faint_bg_color, v.text_color(), 0.08), v.weak_text_color()) };
    egui::Frame::NONE.fill(fill).corner_radius(r_xs()).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| {
        ui.label(egui::RichText::new(text).size(11.0).color(fg));
    });
}

/// A command in a quiet box, with a Copy button beside it.
fn chip(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        let v = ui.visuals().clone();
        egui::Frame::NONE.fill(palette::mix(v.faint_bg_color, v.text_color(), 0.06)).corner_radius(r_sm()).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
            // Long paths are shortened (the whole line is in the tooltip and on Copy).
            ui.set_max_width(250.0);
            ui.add(egui::Label::new(egui::RichText::new(text).monospace().size(12.5)).truncate()).on_hover_text(text);
        });
        if ui.small_button("Copy").on_hover_text("Copy to the clipboard").clicked() {
            ui.ctx().copy_text(text.to_string());
        }
    });
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
        Cmd::EditConnection => "Edit the connection",
        Cmd::CopyLocation => "Copy the location",
        Cmd::SortBy(_) => "Sort",
        Cmd::SortReverse => "Reverse the order",
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

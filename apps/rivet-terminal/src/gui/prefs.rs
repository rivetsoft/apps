//! Preferences, drawn like the other Rivet apps': a sidebar of pages (Appearance, Terminal,
//! Behavior, Keyboard, About) beside the chosen page's settings in titled cards.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{ACCENTS, Mode, paint_tex, r_lg, r_md, r_sm, r_xs, selected_tile};
use rivet_look::{fonts, palette, windows};
use rivet_term::CursorStyle;

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Page {
    #[default]
    Appearance,
    Terminal,
    Behavior,
    Keyboard,
    About,
}

impl Page {
    const ALL: [Page; 5] = [Page::Appearance, Page::Terminal, Page::Behavior, Page::Keyboard, Page::About];
    fn label(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::Terminal => "Terminal",
            Page::Behavior => "Behavior",
            Page::Keyboard => "Keyboard",
            Page::About => "About",
        }
    }
    fn icons(self) -> &'static [&'static str] {
        match self {
            Page::Appearance => &["preferences-desktop-appearance-symbolic", "applications-graphics-symbolic"],
            Page::Terminal => &["utilities-terminal-symbolic", "terminal-symbolic"],
            Page::Behavior => &["preferences-system-symbolic", "emblem-system-symbolic"],
            Page::Keyboard => &["preferences-desktop-keyboard-shortcuts-symbolic", "input-keyboard-symbolic"],
            Page::About => &["help-about-symbolic", "dialog-information-symbolic"],
        }
    }
    pub(crate) fn from_name(s: &str) -> Page {
        Page::ALL.into_iter().find(|p| p.label().eq_ignore_ascii_case(s)).unwrap_or_default()
    }
}

/// The keys shown on the Keyboard page.
pub(crate) const KEYS: &[(&str, &[(&str, &str)])] = &[
    (
        "Tabs and panes",
        &[
            ("New tab", "Ctrl+Shift+T"),
            ("New pane beside this one", "Ctrl+Shift+Enter"),
            ("New window", "Ctrl+Shift+N"),
            ("Close the pane / the tab", "Ctrl+Shift+W / Ctrl+Shift+Q"),
            ("Next / previous tab", "Ctrl+Shift+Right / Left"),
            ("Move the tab", "Ctrl+Shift+. / Ctrl+Shift+,"),
            ("Next / previous pane", "Ctrl+Shift+] / Ctrl+Shift+["),
            ("Next layout", "Ctrl+Shift+L"),
            ("One pane at a time", "Ctrl+Shift+Z"),
        ],
    ),
    (
        "Text",
        &[
            ("Copy / paste", "Ctrl+Shift+C / Ctrl+Shift+V"),
            ("Select everything", "Ctrl+Shift+A"),
            ("Find in the scrollback", "Ctrl+Shift+F"),
            ("Clear the scrollback", "Ctrl+Shift+K"),
            ("Scroll a line / a page", "Ctrl+Shift+Up / Shift+Page Up"),
            ("To the top / the bottom", "Ctrl+Shift+Home / End"),
            ("Open a link", "Ctrl+click"),
            ("Select a word / a line", "Double / triple click"),
            ("Bigger / smaller / usual size", "Ctrl+Shift+= / - / Backspace"),
        ],
    ),
    ("Window", &[("All commands", "Ctrl+Shift+P"), ("Preferences", "Ctrl+,"), ("Menu bar", "Ctrl+Shift+M"), ("Full screen", "Ctrl+Shift+F11")]),
];

impl TermApp {
    pub(crate) fn prefs_window(&mut self, ctx: &egui::Context) {
        let chrome = windows::Chrome { title: "Preferences".into(), icon: self.icon(ctx, &["preferences-system-symbolic", "emblem-system-symbolic"], 16.0), close: self.icon(ctx, &["window-close-symbolic"], 14.0), flush: true };
        let area = ctx.content_rect().shrink(6.0);
        let app_icon = self.app_icon(ctx);
        let tab_icons: Vec<Option<egui::TextureHandle>> = Page::ALL.iter().map(|p| self.icon(ctx, p.icons(), 16.0)).collect();
        let pal = self.palette();
        let fams = self.families.get_or_insert_with(fonts::mono_families).clone();
        let system_mono = fonts::system_mono_name();
        let before = self.s.clone();
        let page_now = self.prefs_page;
        let mut pick: Option<Page> = None;
        let mut win = std::mem::replace(&mut self.prefs_win, windows::WinState::new(1, Vec2::ZERO));
        let shown = windows::show(ctx, &mut win, &chrome, area, Vec2::new(600.0, 380.0), |ui| {
            let v = ui.visuals().clone();
            let weak = v.weak_text_color();
            let full_h = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let side_fill = palette::mix(v.window_fill, v.panel_fill, 0.6);
                egui::Frame::NONE.fill(side_fill).corner_radius(egui::CornerRadius { sw: r_lg(), ..Default::default() }).inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 4 }).show(ui, |ui| {
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
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical().id_salt(("term-prefs", page_now as u8)).auto_shrink([false, false]).show(ui, |ui| {
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
                            let grid = |ui: &mut egui::Ui, id: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                                egui::Grid::new(id).num_columns(2).spacing([24.0, 18.0]).min_col_width(130.0).show(ui, |ui| body(ui));
                            };
                            if page_now != Page::About {
                                ui.label(egui::RichText::new(page_now.label()).size(20.0).strong());
                                ui.add_space(14.0);
                            }
                            match page_now {
                                Page::Appearance => {
                                    section(ui, "Desktop colors", &mut |ui| {
                                        ui.checkbox(&mut self.s.follow_desktop_colors, "Follow desktop colors");
                                        ui.add_space(6.0);
                                        match (&pal, rivet_look::palette::follows(pal.as_ref(), self.s.follow_desktop_colors)) {
                                            (Some(p), true) => {
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    for c in [p.accent, p.window_bg, p.view_bg, p.window_fg].into_iter().flatten() {
                                                        let (r, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                                                        ui.painter().rect_filled(r, r_xs(), c);
                                                        ui.painter().rect_stroke(r, r_xs(), Stroke::new(1.0, weak), egui::StrokeKind::Inside);
                                                    }
                                                    ui.add_space(4.0);
                                                    ui.label(egui::RichText::new(p.source_label()).small().color(weak)).on_hover_text(p.source.display().to_string());
                                                });
                                            }
                                            (None, true) => hint(ui, "No desktop color file was found, so the style and accent below are used."),
                                            _ => hint(ui, "The style and accent below are used."),
                                        }
                                    });
                                    let manual = rivet_look::palette::shown(pal.as_ref(), self.s.follow_desktop_colors).is_none();
                                    section(ui, "Style and accent", &mut |ui| {
                                        grid(ui, "term-look", &mut |ui| {
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
                                        hint(ui, "Hidden, its menus fold into the menu button. Ctrl+Shift+M switches.");
                                    });
                                }
                                Page::Terminal => {
                                    section(ui, "Colors", &mut |ui| {
                                        grid(ui, "term-scheme", &mut |ui| {
                                            ui.label("Color theme");
                                            let shown = if self.s.scheme.is_empty() { "Follow the app's colors".to_string() } else { self.s.scheme.clone() };
                                            egui::ComboBox::from_id_salt("term-scheme-pick").selected_text(shown).width(240.0).height(420.0).show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.s.scheme, String::new(), "Follow the app's colors");
                                                for dark in [true, false] {
                                                    ui.separator();
                                                    ui.label(egui::RichText::new(if dark { "DARK" } else { "LIGHT" }).small().color(weak));
                                                    for sc in rivet_look::schemes::SCHEMES.iter().filter(|s| s.dark == dark) {
                                                        ui.horizontal(|ui| {
                                                            let (r, _) = ui.allocate_exact_size(Vec2::new(46.0, 16.0), Sense::hover());
                                                            ui.painter().rect_filled(r, 3.0, sc.bg());
                                                            for (k, c) in sc.swatch().into_iter().enumerate() {
                                                                ui.painter().circle_filled(egui::pos2(r.left() + 8.0 + k as f32 * 10.0, r.center().y), 3.2, c);
                                                            }
                                                            ui.selectable_value(&mut self.s.scheme, sc.name.to_string(), sc.name);
                                                        });
                                                    }
                                                }
                                            });
                                            ui.end_row();
                                        });
                                        ui.add_space(10.0);
                                        ui.checkbox(&mut self.s.bold_is_bright, "Bold text in the bright colors");
                                    });
                                    section(ui, "Text", &mut |ui| {
                                        grid(ui, "term-text", &mut |ui| {
                                            ui.label("Font");
                                            let shown = if self.s.font.is_empty() { format!("System ({system_mono})") } else { self.s.font.clone() };
                                            egui::ComboBox::from_id_salt("term-font").selected_text(shown).width(240.0).show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.s.font, String::new(), format!("System ({system_mono})"));
                                                for f in &fams {
                                                    ui.selectable_value(&mut self.s.font, f.clone(), f.as_str());
                                                }
                                            });
                                            ui.end_row();
                                            ui.label("Size");
                                            ui.add(egui::Slider::new(&mut self.s.font_size, 7.0..=32.0).step_by(0.5).suffix(" pt"));
                                            ui.end_row();
                                            ui.label("Room around it");
                                            ui.add(egui::Slider::new(&mut self.s.padding, 0.0..=32.0).step_by(1.0).suffix(" pt"));
                                            ui.end_row();
                                            ui.label("Cursor");
                                            ui.horizontal(|ui| {
                                                for (c, l) in [(CursorStyle::Block, "Block"), (CursorStyle::Beam, "Beam"), (CursorStyle::Underline, "Underline")] {
                                                    ui.selectable_value(&mut self.s.cursor, c, l);
                                                }
                                            });
                                            ui.end_row();
                                        });
                                    });
                                }
                                Page::Behavior => {
                                    section(ui, "Starting", &mut |ui| {
                                        grid(ui, "term-shell", &mut |ui| {
                                            ui.label("Run");
                                            ui.add(egui::TextEdit::singleline(&mut self.s.shell).hint_text("Your login shell").desired_width(240.0));
                                            ui.end_row();
                                        });
                                        ui.add_space(6.0);
                                        hint(ui, "Empty runs your login shell. New tabs and panes start in the folder you're in.");
                                    });
                                    section(ui, "Scrollback", &mut |ui| {
                                        grid(ui, "term-scroll", &mut |ui| {
                                            ui.label("Lines kept");
                                            ui.add(egui::Slider::new(&mut self.s.scrollback, 1_000..=100_000).logarithmic(true));
                                            ui.end_row();
                                        });
                                        hint(ui, "For tabs and panes opened from now on.");
                                    });
                                    section(ui, "Selecting and the bell", &mut |ui| {
                                        ui.checkbox(&mut self.s.copy_on_select, "Copy text as soon as it's selected");
                                        ui.checkbox(&mut self.s.visual_bell, "Flash the pane on the bell");
                                    });
                                    section(ui, "Closing", &mut |ui| {
                                        ui.checkbox(&mut self.s.confirm_close, "Ask before closing while a program is running");
                                        hint(ui, "Only when something other than the shell is running (an editor, a build, ssh…).");
                                    });
                                }
                                Page::Keyboard => {
                                    for (title, rows) in KEYS {
                                        section(ui, title, &mut |ui| {
                                            egui::Grid::new(("term-keys", *title)).num_columns(2).spacing([28.0, 10.0]).min_col_width(190.0).show(ui, |ui| {
                                                for (what, keys) in rows.iter() {
                                                    ui.label(*what);
                                                    key_chip(ui, keys);
                                                    ui.end_row();
                                                }
                                            });
                                        });
                                    }
                                    hint(ui, "Plain Ctrl+letters go to the programs in the terminal (Ctrl+C stops one), so Rivet Terminal's own keys use Ctrl+Shift, as kitty's do.");
                                }
                                Page::About => {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(24.0);
                                        if let Some(t) = &app_icon {
                                            ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(96.0)));
                                        }
                                        ui.add_space(10.0);
                                        ui.label(egui::RichText::new("Rivet Terminal").size(22.0).strong());
                                        ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(weak));
                                        ui.add_space(14.0);
                                        ui.set_max_width(400.0);
                                        ui.add(egui::Label::new("A terminal with tabs and split panes, in the Rivet apps' look.").halign(egui::Align::Center).wrap());
                                        ui.add(egui::Label::new(egui::RichText::new("Terminal emulation by alacritty_terminal (Apache-2.0). Tabs, panes and layouts inspired by kitty.").small().color(weak)).halign(egui::Align::Center).wrap());
                                        ui.add_space(14.0);
                                        ui.hyperlink_to("github.com/rivetsoft/apps", "https://github.com/rivetsoft/apps");
                                        ui.add_space(6.0);
                                        ui.label(egui::RichText::new("Licensed under MIT or Apache-2.0").small().color(weak));
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
            self.refocus = true;
        }
        if self.s != before {
            self.s.save();
        }
    }
}

/// Keys in a quiet rounded box.
fn key_chip(ui: &mut egui::Ui, keys: &str) {
    let v = ui.visuals().clone();
    egui::Frame::NONE.fill(palette::mix(v.faint_bg_color, v.text_color(), 0.07)).corner_radius(r_xs()).inner_margin(egui::Margin::symmetric(7, 2)).show(ui, |ui| {
        ui.label(egui::RichText::new(keys).font(fonts::ui_font(11.5)).color(v.text_color()));
    });
}

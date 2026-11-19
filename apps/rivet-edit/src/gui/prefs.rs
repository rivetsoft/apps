//! Preferences, drawn like the other Rivet apps': a sidebar of pages (Appearance, Editor,
//! Window, Keyboard, About) beside the chosen page's settings in titled cards.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{ACCENTS, Mode, paint_tex, r_lg, r_md, r_sm, r_xs, selected_tile};
use rivet_look::{fonts, palette, windows};

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Page {
    #[default]
    Appearance,
    Editor,
    Window,
    Keyboard,
    About,
}

impl Page {
    const ALL: [Page; 5] = [Page::Appearance, Page::Editor, Page::Window, Page::Keyboard, Page::About];
    fn label(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::Editor => "Editor",
            Page::Window => "Window",
            Page::Keyboard => "Keyboard",
            Page::About => "About",
        }
    }
    fn icons(self) -> &'static [&'static str] {
        match self {
            Page::Appearance => &["preferences-desktop-appearance-symbolic", "applications-graphics-symbolic", "preferences-desktop-theme"],
            Page::Editor => &["accessories-text-editor-symbolic", "text-editor-symbolic", "document-edit-symbolic", "text-x-generic-symbolic"],
            Page::Window => &["preferences-system-windows-symbolic", "view-dual-symbolic", "window-new-symbolic", "view-grid-symbolic"],
            Page::Keyboard => &["preferences-desktop-keyboard-shortcuts-symbolic", "input-keyboard-symbolic"],
            Page::About => &["help-about-symbolic", "dialog-information-symbolic"],
        }
    }
    /// From a test hook's word (`editor`, `keyboard`…).
    pub(crate) fn from_name(s: &str) -> Page {
        Page::ALL.into_iter().find(|p| p.label().eq_ignore_ascii_case(s)).unwrap_or_default()
    }
}

/// The keys shown on the Keyboard page.
const KEYS: &[(&str, &[(&str, &str)])] = &[
    (
        "Files and commands",
        &[
            ("Go to a file", "Ctrl+P"),
            ("Run a command", "Shift+Ctrl+P"),
            ("Open a folder", "Ctrl+O"),
            ("New file", "Ctrl+N"),
            ("Save", "Ctrl+S"),
            ("Undo / redo", "Ctrl+Z / Shift+Ctrl+Z"),
            ("Close the tab", "Ctrl+W"),
            ("Go to a line", "Ctrl+G"),
            ("Go to Definition", "F12  or  Ctrl+click"),
            ("Find References", "Shift+F12"),
            ("Back / Forward", "Alt+Left / Alt+Right"),
            ("Sidebar", "F9"),
            ("Menu bar", "Ctrl+M"),
            ("Preferences", "Ctrl+,"),
        ],
    ),
    (
        "Find",
        &[
            ("Find in the file", "Ctrl+F"),
            ("Replace in the file", "Ctrl+H"),
            ("Next / previous match", "F3 / Shift+F3"),
            ("Search the folder", "Shift+Ctrl+F"),
        ],
    ),
    (
        "Cursors and lines",
        &[
            ("Add the next match", "Ctrl+D"),
            ("Every match", "Shift+Ctrl+L"),
            ("Another cursor", "Alt+click"),
            ("A cursor above / below", "Ctrl+Alt+Up / Down"),
            ("Back to one cursor", "Esc"),
            ("Move the lines", "Alt+Up / Down"),
            ("Copy the lines", "Shift+Alt+Up / Down"),
            ("Delete the lines", "Shift+Ctrl+K"),
            ("Select the line", "Ctrl+L"),
            ("Comment out", "Ctrl+/"),
            ("Indent / outdent", "Ctrl+] / Ctrl+["),
            ("New line below / above", "Ctrl+Enter / Shift+Ctrl+Enter"),
            ("Wrap long lines", "Alt+Z"),
            ("Suggestions", "Ctrl+Space"),
        ],
    ),
    (
        "Git",
        &[
            ("Git panel", "Shift+Ctrl+G"),
            ("Commit (in the message)", "Ctrl+Enter"),
            ("Next / previous change", "Alt+F5 / Shift+Alt+F5"),
        ],
    ),
    (
        "Vim keys (when on)",
        &[
            ("Insert / append / new line", "i a I A o O"),
            ("Back to Normal", "Esc  or  Ctrl+["),
            ("Move", "h j k l  w b e  0 ^ $  gg G  { }  %"),
            ("To a character", "f t F T  ; ,"),
            ("Delete / change / copy", "d c y + a motion, or dd cc yy"),
            ("Inside / around", "iw aw  i\" a\"  i( a(  i{ a{"),
            ("Put / undo / redo", "p P  u  Ctrl+R"),
            ("Repeat the last change", "."),
            ("Select", "v  V"),
            ("Search", "/ ?  n N  * #"),
            ("Definition, references, what it is", "gd  gr  K"),
            ("Save, close, a line", ":w  :q  :wq  :42"),
            ("Replace", ":s/old/new/g  :%s/old/new/g"),
        ],
    ),
];

impl EditApp {
    pub(crate) fn prefs_window(&mut self, ctx: &egui::Context) {
        let chrome = windows::Chrome {
            title: "Preferences".into(),
            icon: self.icon(ctx, &["preferences-system-symbolic", "emblem-system-symbolic"], 16.0),
            close: self.icon(ctx, &["window-close-symbolic"], 14.0),
            // The page sidebar runs to the window's edges.
            flush: true,
        };
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
                                    // About sits apart at the bottom of the list.
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
                    egui::ScrollArea::vertical().id_salt(("edit-prefs", page_now as u8)).auto_shrink([false, false]).show(ui, |ui| {
                        egui::Frame::NONE.inner_margin(egui::Margin { left: 26, right: 22, top: 18, bottom: 22 }).show(ui, |ui| {
                            ui.set_width(ui.available_width().max(360.0));
                            ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
                            ui.spacing_mut().button_padding = Vec2::new(10.0, 4.0);
                            let card_fill = v.faint_bg_color;
                            // One titled, roomy card per group of settings.
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
                            // A label on the left and its control on the right, like Rivet's rows.
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
                                            (None, true) => hint(ui, "No desktop colors were found, so the style and accent below apply."),
                                            _ => hint(ui, "The style and accent below apply."),
                                        }
                                    });
                                    let manual = rivet_look::palette::shown(pal.as_ref(), self.s.follow_desktop_colors).is_none();
                                    section(ui, "Style and accent", &mut |ui| {
                                        grid(ui, "edit-look", &mut |ui| {
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
                                            hint(ui, "Set by your desktop colors. Turn off Follow desktop colors to choose your own.");
                                        }
                                    });
                                    section(ui, "Icons", &mut |ui| {
                                        grid(ui, "edit-icons", &mut |ui| {
                                            ui.label("Icon theme");
                                            ui.horizontal(|ui| {
                                                ui.selectable_value(&mut self.s.builtin_icons, true, "Rivet's own");
                                                ui.selectable_value(&mut self.s.builtin_icons, false, "Desktop icon theme");
                                            });
                                            ui.end_row();
                                            ui.label("Files and folders");
                                            ui.horizontal(|ui| {
                                                ui.selectable_value(&mut self.s.typed_icons, true, "By type, in color");
                                                ui.selectable_value(&mut self.s.typed_icons, false, "From the icon theme");
                                            });
                                            ui.end_row();
                                        });
                                        ui.add_space(6.0);
                                        hint(ui, "By type gives each kind of file its own colored icon, tinted toward your accent. Desktop icon theme matches your other apps.");
                                    });
                                }
                                Page::Editor => {
                                    section(ui, "Color theme", &mut |ui| {
                                        grid(ui, "edit-scheme", &mut |ui| {
                                            ui.label("Code colors");
                                            let shown = if self.s.code_scheme.is_empty() { "Follow the app's colors".to_string() } else { self.s.code_scheme.clone() };
                                            egui::ComboBox::from_id_salt("code-scheme").selected_text(shown).width(240.0).height(420.0).show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.s.code_scheme, String::new(), "Follow the app's colors");
                                                for dark in [true, false] {
                                                    ui.separator();
                                                    ui.label(egui::RichText::new(if dark { "DARK" } else { "LIGHT" }).small().color(weak));
                                                    for sc in crate::schemes::SCHEMES.iter().filter(|s| s.dark == dark) {
                                                        ui.horizontal(|ui| {
                                                            // A little card of its colors.
                                                            let (r, _) = ui.allocate_exact_size(Vec2::new(46.0, 16.0), Sense::hover());
                                                            ui.painter().rect_filled(r, 3.0, sc.bg());
                                                            for (k, c) in sc.swatch().into_iter().enumerate() {
                                                                ui.painter().circle_filled(egui::pos2(r.left() + 8.0 + k as f32 * 10.0, r.center().y), 3.2, c);
                                                            }
                                                            ui.selectable_value(&mut self.s.code_scheme, sc.name.to_string(), sc.name);
                                                        });
                                                    }
                                                }
                                            });
                                            ui.end_row();
                                        });
                                        ui.add_space(6.0);
                                        hint(ui, "Changes only the code, not the tabs or sidebar. To preview themes live, press Shift+Ctrl+P and choose Color Theme.");
                                    });
                                    section(ui, "Text", &mut |ui| {
                                        grid(ui, "edit-text", &mut |ui| {
                                            ui.label("Font");
                                            let shown = if self.s.code_font.is_empty() { format!("System ({system_mono})") } else { self.s.code_font.clone() };
                                            egui::ComboBox::from_id_salt("code-font").selected_text(shown).width(240.0).show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.s.code_font, String::new(), format!("System ({system_mono})"));
                                                for f in &fams {
                                                    ui.selectable_value(&mut self.s.code_font, f.clone(), f.as_str());
                                                }
                                            });
                                            ui.end_row();
                                            ui.label("Size");
                                            ui.add(egui::Slider::new(&mut self.s.code_size, 8.0..=32.0).step_by(1.0).suffix(" pt"));
                                            ui.end_row();
                                            ui.label("Line height");
                                            ui.add(egui::Slider::new(&mut self.s.line_height, 1.0..=2.0).step_by(0.05).fixed_decimals(2).suffix("×"))
                                                .on_hover_text("1.00× is the font's own spacing; 1.50× adds half a line");
                                            ui.end_row();
                                        });
                                    });
                                    section(ui, "Indentation", &mut |ui| {
                                        ui.checkbox(&mut self.s.detect_indent, "Follow each file's own indentation");
                                        hint(ui, "Matches tabs or spaces, and their width, to each file you open. Click the indentation in the status bar to change one file.");
                                        ui.add_space(10.0);
                                        grid(ui, "edit-indent", &mut |ui| {
                                            ui.label("Otherwise indent with");
                                            ui.horizontal(|ui| {
                                                ui.selectable_value(&mut self.s.use_tabs, false, "Spaces");
                                                ui.selectable_value(&mut self.s.use_tabs, true, "Tabs");
                                            });
                                            ui.end_row();
                                            ui.label(if self.s.use_tabs { "Tab size" } else { "Spaces" });
                                            ui.horizontal(|ui| {
                                                for n in [2usize, 4, 8] {
                                                    ui.selectable_value(&mut self.s.tab_width, n, n.to_string());
                                                }
                                            });
                                            ui.end_row();
                                        });
                                    });
                                    section(ui, "Suggestions and errors", &mut |ui| {
                                        ui.checkbox(&mut self.s.autocomplete, "Suggest as you type");
                                        hint(ui, "Press Ctrl+Space for suggestions any time. Enter or Tab inserts one; Esc closes the list.");
                                        ui.add_space(8.0);
                                        ui.checkbox(&mut self.s.lsp, "Use language servers");
                                        hint(ui, "Underlines errors and gives smarter suggestions, using each language's installed server (rust-analyzer, clangd, pyright, gopls…). Without one, suggestions come from words in the file.");
                                    });
                                    section(ui, "Keys", &mut |ui| {
                                        ui.checkbox(&mut self.s.vim_keys, "Vim keys");
                                        hint(ui, "Normal, Insert and Visual modes, with counts, motions, text objects, the . repeat, search, and :w :q :s. The status bar shows the mode. The Keyboard page lists them all.");
                                    });
                                    section(ui, "Layout", &mut |ui| {
                                        ui.checkbox(&mut self.s.word_wrap, "Wrap long lines");
                                        ui.checkbox(&mut self.s.line_numbers, "Line numbers");
                                    });
                                }
                                Page::Window => {
                                    section(ui, "Show", &mut |ui| {
                                        ui.checkbox(&mut self.s.show_menubar, "Menu bar");
                                        hint(ui, "When off, the menus move into the menu button at the top left. Ctrl+M turns it on or off.");
                                        ui.add_space(6.0);
                                        ui.checkbox(&mut self.s.show_sidebar, "Sidebar (files, search and Git)");
                                        hint(ui, "F9 shows or hides it.");
                                        ui.add_space(6.0);
                                        ui.checkbox(&mut self.s.show_statusbar, "Status bar");
                                    });
                                    section(ui, "Recent folders and files", &mut |ui| {
                                        let n = self.s.recent.len();
                                        ui.horizontal(|ui| {
                                            ui.label(if n == 0 { "None yet.".to_string() } else { format!("{n} shown on the start page.") });
                                            if n > 0 && ui.button("Clear List").clicked() {
                                                self.s.recent.clear();
                                            }
                                        });
                                    });
                                }
                                Page::Keyboard => {
                                    for (title, rows) in KEYS {
                                        section(ui, title, &mut |ui| {
                                            egui::Grid::new(("edit-keys", *title)).num_columns(2).spacing([28.0, 10.0]).min_col_width(170.0).show(ui, |ui| {
                                                for (what, keys) in rows.iter() {
                                                    ui.label(*what);
                                                    key_chip(ui, keys);
                                                    ui.end_row();
                                                }
                                            });
                                        });
                                    }
                                }
                                Page::About => {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(24.0);
                                        if let Some(t) = &app_icon {
                                            ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(96.0)));
                                        }
                                        ui.add_space(10.0);
                                        ui.label(egui::RichText::new("Rivet Edit").size(22.0).strong());
                                        ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(weak));
                                        ui.add_space(14.0);
                                        ui.set_max_width(380.0);
                                        ui.add(egui::Label::new("A lightweight code editor for folders and files.").halign(egui::Align::Center).wrap());
                                        ui.add(
                                            egui::Label::new(egui::RichText::new("Tree-sitter colors for Rust, Python, JavaScript, TypeScript, JSON, shell, C, Go, HTML, CSS, TOML and YAML.").small().color(weak))
                                                .halign(egui::Align::Center)
                                                .wrap(),
                                        );
                                        ui.add_space(14.0);
                                        ui.hyperlink_to("github.com/ulinaaron/rivet-apps", "https://github.com/ulinaaron/rivet-apps");
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
        }
        if self.s != before {
            self.s.save();
        }
    }
}

/// Keys in a quiet rounded box ("Ctrl+D").
pub(crate) fn key_chip(ui: &mut egui::Ui, keys: &str) {
    let v = ui.visuals().clone();
    egui::Frame::NONE.fill(palette::mix(v.faint_bg_color, v.text_color(), 0.07)).corner_radius(r_xs()).inner_margin(egui::Margin::symmetric(7, 2)).show(ui, |ui| {
        ui.label(egui::RichText::new(keys).font(fonts::ui_font(11.5)).color(v.text_color()));
    });
}

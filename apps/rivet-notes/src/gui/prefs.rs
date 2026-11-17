//! Preferences, laid out like Rivet Files' and Rivet FTP's: a sidebar of pages on the left,
//! the page's settings on cards on the right.

use std::path::PathBuf;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{ACCENTS, Mode, paint_tex, r_lg, r_md, r_sm, selected_tile};
use rivet_look::{fonts, palette, windows};

use super::draw::font_pick;
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    Appearance,
    Editor,
    Sidebar,
    Vaults,
    Modules,
    Keyboard,
    About,
}

impl Page {
    const ALL: [Page; 7] = [Page::Appearance, Page::Editor, Page::Sidebar, Page::Vaults, Page::Modules, Page::Keyboard, Page::About];
    fn label(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::Editor => "Editor",
            Page::Sidebar => "Sidebar",
            Page::Vaults => "Vaults",
            Page::Modules => "Modules",
            Page::Keyboard => "Keyboard",
            Page::About => "About",
        }
    }
    fn icons(self) -> &'static [&'static str] {
        match self {
            Page::Appearance => &["preferences-desktop-appearance-symbolic", "applications-graphics-symbolic", "view-reveal-symbolic"],
            Page::Editor => &["document-edit-symbolic", "text-editor-symbolic"],
            Page::Sidebar => &["sidebar-show-symbolic", "view-sidebar-symbolic"],
            Page::Vaults => &["folder-symbolic", "folder-documents-symbolic"],
            Page::Modules => &["view-grid-symbolic", "application-x-addon-symbolic"],
            Page::Keyboard => &["preferences-desktop-keyboard-shortcuts-symbolic", "input-keyboard-symbolic", "edit-find-symbolic"],
            Page::About => &["help-about-symbolic", "dialog-information-symbolic"],
        }
    }
    pub(crate) fn from_name(n: &str) -> Page {
        Page::ALL.into_iter().find(|p| p.label().eq_ignore_ascii_case(n)).unwrap_or(Page::Appearance)
    }
}

/// The keys, for the Keyboard page.
const KEYS: [(&str, &[(&str, &str)]); 3] = [
    ("Notes", &[("New note", "Ctrl+N"), ("New folder", "Shift+Ctrl+N"), ("Daily note (Daily notes module)", "Ctrl+D"), ("Make a super todo (Super todos module)", "Shift+Ctrl+T"), ("Save now", "Ctrl+S"), ("Rename the note", "F2"), ("Move the note to the Trash", "Delete"), ("Search", "Ctrl+F"), ("Quick Open: go to a note", "Ctrl+P"), ("Quick Actions: every command", "Shift+Ctrl+P")]),
    ("View", &[("Reading view (Esc closes it)", "Ctrl+Space"), ("Switch folders / tags", "Ctrl+T"), ("Show or hide the sidebar", "F9"), ("Show or hide Peek", "F11"), ("Show or hide the menu bar", "Ctrl+M"), ("Preferences", "Ctrl+,")]),
    ("Formatting", &[]),
];

impl NotesApp {
    pub(super) fn prefs_window(&mut self, ctx: &egui::Context) {
        let mut chrome = windows::Chrome { title: "Preferences".into(), icon: self.icon(ctx, &["preferences-system-symbolic", "emblem-system-symbolic"], 16.0), close: self.icon(ctx, &["window-close-symbolic"], 14.0), flush: false };
        // The page sidebar runs to the window's edges.
        chrome.flush = true;
        let area = ctx.content_rect().shrink(6.0);
        let about_icon = self.app_icon(ctx);
        let tab_icons: Vec<Option<egui::TextureHandle>> = Page::ALL.iter().map(|p| self.icon(ctx, p.icons(), 16.0)).collect();
        if self.about_open {
            self.prefs_page = Page::About;
            self.about_open = false;
        }
        let page_now = self.prefs_page;
        let mut pick: Option<Page> = None;
        let mut win = std::mem::replace(&mut self.prefs_win, windows::WinState::new(0, Vec2::ZERO));
        let before = self.s.clone();
        let pal = self.look.palette().cloned();
        let mut open_vault: Option<PathBuf> = None;
        let mut forget: Option<PathBuf> = None;
        let mut ask_vault = false;
        let vault_folders = self.folders.clone();
        let fonts_lists = if page_now == Page::Editor { Some(self.families.get_or_insert_with(|| (fonts::families(), fonts::mono_families())).clone()) } else { None };
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
                    egui::ScrollArea::vertical().id_salt(("notes-prefs", page_now as u8)).auto_shrink([false, false]).show(ui, |ui| {
                        egui::Frame::NONE.inner_margin(egui::Margin { left: 26, right: 22, top: 18, bottom: 22 }).show(ui, |ui| {
                            ui.set_width(ui.available_width().max(360.0));
                            ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
                            ui.spacing_mut().button_padding = Vec2::new(10.0, 4.0);
                            let card = v.faint_bg_color;
                            let section = |ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                                ui.label(egui::RichText::new(title).strong().size(14.5));
                                ui.add_space(4.0);
                                egui::Frame::NONE.fill(card).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(18, 16)).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    body(ui);
                                });
                                ui.add_space(22.0);
                            };
                            let hint = |ui: &mut egui::Ui, text: &str| {
                                ui.add(egui::Label::new(egui::RichText::new(text).small().color(weak)).wrap());
                            };
                            let grid = |ui: &mut egui::Ui, id: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                                egui::Grid::new(id).num_columns(2).spacing([24.0, 16.0]).min_col_width(120.0).show(ui, |ui| body(ui));
                            };
                            if page_now != Page::About {
                                ui.label(egui::RichText::new(page_now.label()).size(20.0).strong());
                                ui.add_space(14.0);
                            }
                            match page_now {
                                Page::Appearance => {
                                    section(ui, "Desktop colors", &mut |ui| {
                                        ui.checkbox(&mut self.s.follow_desktop_colors, "Follow desktop colors");
                                        match (&pal, self.s.follow_desktop_colors) {
                                            (Some(p), true) => {
                                                ui.add_space(4.0);
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    for c in [p.accent, p.window_bg, p.view_bg, p.window_fg].into_iter().flatten() {
                                                        let (r, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                                                        ui.painter().rect_filled(r, rivet_look::theme::r_xs(), c);
                                                        ui.painter().rect_stroke(r, rivet_look::theme::r_xs(), Stroke::new(1.0, weak), egui::StrokeKind::Inside);
                                                    }
                                                    let name = p.source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                                    ui.label(egui::RichText::new(format!("from {name}")).small().color(weak));
                                                });
                                            }
                                            (None, true) => hint(ui, "No desktop color file was found, so the style and accent below are used."),
                                            _ => hint(ui, "The style and accent below are used."),
                                        }
                                    });
                                    let manual = !self.s.follow_desktop_colors || pal.is_none();
                                    section(ui, "Style and accent", &mut |ui| {
                                        grid(ui, "notes-look", &mut |ui| {
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
                                    section(ui, "Icons", &mut |ui| {
                                        grid(ui, "notes-icons", &mut |ui| {
                                            ui.label("Icon set");
                                            ui.horizontal(|ui| {
                                                ui.selectable_value(&mut self.s.builtin_icons, true, "Rivet's own").on_hover_text("Rivet's icon kit, colored from your theme");
                                                ui.selectable_value(&mut self.s.builtin_icons, false, "Desktop icon theme");
                                            });
                                            ui.end_row();
                                        });
                                    });
                                    section(ui, "Default note width", &mut |ui| {
                                        ui.horizontal(|ui| {
                                            ui.selectable_value(&mut self.s.centered, false, "Full width").on_hover_text("The text uses the whole window");
                                            ui.selectable_value(&mut self.s.centered, true, "Centered").on_hover_text("The text keeps to a readable column in the middle");
                                        });
                                        ui.add_space(4.0);
                                        hint(ui, "For every note, unless a note has its own width: choosing a width in Peek (F11) sets it for that note only.");
                                    });
                                    section(ui, "Window", &mut |ui| {
                                        ui.checkbox(&mut self.s.show_menubar, "Menu bar");
                                        ui.checkbox(&mut self.s.show_statusbar, "Status bar");
                                        ui.checkbox(&mut self.s.show_sidebar, "Sidebar");
                                        ui.checkbox(&mut self.s.show_peek, "Peek");
                                        ui.checkbox(&mut self.s.show_breadcrumbs, "Breadcrumbs above the note's title");
                                    });
                                }
                                Page::Editor => {
                                    let (fams, monos) = fonts_lists.clone().unwrap_or_default();
                                    let desktop = fonts::desktop_font_cached().map(|(f, _)| f).unwrap_or_else(|| "Sans".into());
                                    let system_mono = fonts::system_mono_name();
                                    section(ui, "Editing mode", &mut |ui| {
                                        ui.horizontal(|ui| {
                                            ui.selectable_value(&mut self.s.live_mode, false, "Source").on_hover_text("Markdown as written, with colors");
                                            ui.selectable_value(&mut self.s.live_mode, true, "Live").on_hover_text("Rendered, except the line you're editing");
                                        });
                                        ui.add_space(4.0);
                                        hint(ui, "Live: you still write Markdown, but each line shows rendered once you leave it.");
                                    });
                                    section(ui, "Attachments", &mut |ui| {
                                        grid(ui, "attach-grid", &mut |ui| {
                                            ui.label("Folder");
                                            ui.add(egui::TextEdit::singleline(&mut self.s.attachments_folder).hint_text("Top of the vault").desired_width(220.0));
                                            ui.end_row();
                                        });
                                        ui.add_space(4.0);
                                        hint(ui, "Files dropped on a note are copied here and linked where the cursor is; pictures show in the note.");
                                    });
                                    section(ui, "Fonts", &mut |ui| {
                                        grid(ui, "notes-fonts", &mut |ui| {
                                            ui.label("Interface");
                                            font_pick(ui, "f-ui", &mut self.s.ui_font, &format!("Like the desktop ({desktop})"), &fams);
                                            ui.end_row();
                                            ui.label("Editor");
                                            font_pick(ui, "f-ed", &mut self.s.editor_font, "Same as the interface", &fams);
                                            ui.end_row();
                                            ui.label("Monospace");
                                            font_pick(ui, "f-mono", &mut self.s.mono_font, &format!("System ({system_mono})"), &monos);
                                            ui.end_row();
                                            ui.label("Line spacing");
                                            ui.horizontal(|ui| {
                                                for (ls, label) in [(1.35, "Compact"), (1.6, "Comfortable"), (1.85, "Roomy")] {
                                                    if ui.selectable_label((self.s.line_spacing - ls).abs() < 0.01, label).clicked() {
                                                        self.s.line_spacing = ls;
                                                    }
                                                }
                                            });
                                            ui.end_row();
                                            ui.label("Editor size");
                                            ui.horizontal(|ui| {
                                                for (pt, label) in [(13.0, "Small"), (15.0, "Medium"), (17.0, "Large"), (20.0, "Larger")] {
                                                    if ui.selectable_label(self.s.editor_size == pt, label).clicked() {
                                                        self.s.editor_size = pt;
                                                    }
                                                }
                                            });
                                            ui.end_row();
                                        });
                                        ui.add_space(12.0);
                                        // A sample in the editor's fonts.
                                        egui::Frame::NONE.fill(v.extreme_bg_color).corner_radius(r_sm()).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            let size = self.s.editor_size * fonts::scale();
                                            ui.label(egui::RichText::new("Notes are written in this font.").font(egui::FontId::new(size, rivet_look::live::editor_family())));
                                            ui.label(egui::RichText::new("let code = \"in this one\";").font(egui::FontId::monospace(size * 0.92)).color(weak));
                                        });
                                        ui.add_space(6.0);
                                        hint(ui, "Monospace is for code blocks, `inline code` and other embedded text.");
                                    });
                                    section(ui, "Notes", &mut |ui| {
                                        ui.checkbox(&mut self.s.show_properties, "Show properties (the frontmatter) above the text");
                                        hint(ui, "Notes save a second after you stop typing.");
                                    });
                                }
                                Page::Sidebar => {
                                    section(ui, "Panels", &mut |ui| {
                                        use crate::layout::Layout;
                                        let stacked = self.s.layout.is_stacked();
                                        let shown = self.s.layout.slots[0].shown();
                                        ui.horizontal(|ui| {
                                            if ui.selectable_label(!stacked, "Tabs").on_hover_text("Folders and tags share the sidebar (Ctrl+T switches)").clicked() && stacked {
                                                self.s.layout = Layout { split: self.s.layout.split, ..Layout::tabs(shown) };
                                            }
                                            if ui.selectable_label(stacked, "Stacked").on_hover_text("Tags below folders").clicked() && !stacked {
                                                self.s.layout = Layout { split: self.s.layout.split, ..Layout::stacked() };
                                            }
                                        });
                                    });
                                }
                                Page::Modules => {
                                    section(ui, "Super todos", &mut |ui| {
                                        ui.checkbox(&mut self.s.super_todos, egui::RichText::new("Super todos").strong());
                                        ui.add_space(2.0);
                                        hint(ui, "A task (- [ ]) with a todo tag becomes a super todo: it gets a due date, priority and tags, and shows on the Todos page. Shift+Ctrl+T makes the line at the cursor one.");
                                        ui.add_space(10.0);
                                        ui.add_enabled_ui(self.s.super_todos, |ui| {
                                            grid(ui, "todo-grid", &mut |ui| {
                                                ui.label("Todo tags");
                                                let mut text = self.s.todo_tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(", ");
                                                if ui.add(egui::TextEdit::singleline(&mut text).hint_text("#task").desired_width(240.0)).changed() {
                                                    self.s.todo_tags = text.split([',', ' ']).map(|t| t.trim().trim_start_matches('#').to_string()).filter(|t| !t.is_empty()).collect();
                                                }
                                                ui.end_row();
                                            });
                                        });
                                    });
                                    section(ui, "Daily notes", &mut |ui| {
                                        ui.checkbox(&mut self.s.daily_notes, egui::RichText::new("Daily notes").strong());
                                        ui.add_space(2.0);
                                        hint(ui, "A note for each day, named by its date. Ctrl+D (or the + button) opens today's, making it if needed.");
                                        ui.add_space(10.0);
                                        ui.add_enabled_ui(self.s.daily_notes, |ui| {
                                            grid(ui, "daily-grid", &mut |ui| {
                                                ui.label("Folder");
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 4.0;
                                                    ui.add(egui::TextEdit::singleline(&mut self.s.daily_folder).hint_text("Top of the vault").desired_width(200.0));
                                                    let btn = egui::Button::new("Choose…").corner_radius(rivet_look::theme::r_sm());
                                                    egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui| {
                                                        ui.set_min_width(200.0);
                                                        if ui.button("Top of the vault").clicked() {
                                                            self.s.daily_folder.clear();
                                                            ui.close();
                                                        }
                                                        if !vault_folders.is_empty() {
                                                            ui.separator();
                                                        }
                                                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                                                            for f in &vault_folders {
                                                                if ui.button(f.as_str()).clicked() {
                                                                    self.s.daily_folder = f.clone();
                                                                    ui.close();
                                                                }
                                                            }
                                                        });
                                                    });
                                                });
                                                ui.end_row();
                                                ui.label("");
                                                let f = self.s.daily_folder.trim_matches('/');
                                                let name = format!("{}.md", chrono::Local::now().format("%Y-%m-%d"));
                                                let shown = if f.is_empty() { name } else { format!("{f}/{name}") };
                                                let fresh = !f.is_empty() && !vault_folders.iter().any(|x| x == f);
                                                ui.label(egui::RichText::new(format!("Today's note: {shown}{}", if fresh { " (the folder is made when needed)" } else { "" })).small().color(weak));
                                                ui.end_row();
                                            });
                                        });
                                    });
                                }
                                Page::Vaults => {
                                    let vaults = self.s.vaults.clone();
                                    let current = self.vault.clone();
                                    section(ui, "Your vaults", &mut |ui| {
                                        if vaults.is_empty() {
                                            ui.label(egui::RichText::new("None yet.").color(weak));
                                        }
                                        for (i, p) in vaults.iter().enumerate() {
                                            if i > 0 {
                                                ui.separator();
                                            }
                                            ui.horizontal(|ui| {
                                                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                                ui.vertical(|ui| {
                                                    ui.spacing_mut().item_spacing.y = 1.0;
                                                    ui.label(egui::RichText::new(name).strong());
                                                    ui.label(egui::RichText::new(p.display().to_string()).small().color(weak));
                                                });
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    if ui.button("Forget").on_hover_text("Take it off this list (the folder stays as it is)").clicked() {
                                                        forget = Some(p.clone());
                                                    }
                                                    let here = current.as_ref() == Some(p);
                                                    if ui.add_enabled(!here && p.is_dir(), egui::Button::new(if here { "Open" } else { "Switch" })).clicked() {
                                                        open_vault = Some(p.clone());
                                                    }
                                                });
                                            });
                                        }
                                        ui.add_space(12.0);
                                        if ui.button("Open Folder as Vault…").clicked() {
                                            ask_vault = true;
                                        }
                                    });
                                }
                                Page::Keyboard => {
                                    for (group, keys) in KEYS {
                                        let rows: Vec<(String, String)> = if group == "Formatting" {
                                            crate::format::Fmt::ALL.iter().map(|(_, l, k)| (l.to_string(), k.to_string())).collect()
                                        } else {
                                            keys.iter().map(|(l, k)| (l.to_string(), k.to_string())).collect()
                                        };
                                        section(ui, group, &mut |ui| {
                                            egui::Grid::new(("keys", group)).num_columns(2).spacing([28.0, 9.0]).min_col_width(220.0).show(ui, |ui| {
                                                for (what, k) in &rows {
                                                    ui.label(what);
                                                    egui::Frame::NONE.fill(v.extreme_bg_color).corner_radius(rivet_look::theme::r_xs()).inner_margin(egui::Margin::symmetric(8, 2)).show(ui, |ui| {
                                                        ui.label(egui::RichText::new(k).monospace());
                                                    });
                                                    ui.end_row();
                                                }
                                            });
                                            if group == "Formatting" {
                                                ui.add_space(8.0);
                                                hint(ui, "They work on the selection, or the word under the cursor; pressing one again takes the formatting off.");
                                            }
                                        });
                                    }
                                }
                                Page::About => {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(24.0);
                                        if let Some(t) = &about_icon {
                                            ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(96.0)));
                                        }
                                        ui.add_space(10.0);
                                        ui.label(egui::RichText::new("Rivet Notes").size(22.0).strong());
                                        ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(weak));
                                        ui.add_space(14.0);
                                        ui.label("Markdown notes in plain folders, with frontmatter and tags.");
                                        ui.label(egui::RichText::new("Built on Rivet Files. No GTK or Qt.").color(weak));
                                        ui.add_space(14.0);
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
        if let Some(p) = forget {
            self.s.vaults.retain(|x| *x != p);
        }
        if let Some(p) = open_vault {
            self.open_vault(p);
        }
        if ask_vault {
            self.prompt = Some(Prompt::OpenVault(String::new()));
        }
        if self.s != before {
            self.s.save();
        }
    }
}

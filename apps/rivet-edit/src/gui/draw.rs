//! Drawing the window: menus, toolbar, the workspace tree, tabs, the editor with line numbers,
//! the status bar, the welcome page and the questions.

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{paint_tex, r_lg, r_md, r_sm, selected_tile, status};
use rivet_look::{builtin_icons, fonts, highlight, palette};

use super::*;

const ROW_H: f32 = 26.0;

fn menu_style(ui: &mut egui::Ui) {
    ui.set_min_width(230.0);
    ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
    ui.spacing_mut().item_spacing.y = 3.0;
}

fn item(ui: &mut egui::Ui, label: &str, shortcut: &str, on: bool) -> bool {
    let b = egui::Button::new(label);
    let b = if shortcut.is_empty() { b } else { b.shortcut_text(shortcut) };
    let r = ui.add_enabled(on, b).clicked();
    if r {
        ui.close();
    }
    r
}

/// What the sidebar asked for.
enum Pick {
    File(String),
    Fold(String),
    NewHere(String),
}

impl EditApp {
    pub(super) fn draw(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let v = ctx.global_style().visuals.clone();
        if self.s.show_menubar {
            let bar = egui::Frame::NONE.fill(v.panel_fill).inner_margin(egui::Margin::symmetric(6, 2));
            egui::Panel::top("edit-menubar").show_separator_line(false).frame(bar).show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| self.menus(ui));
            });
        }
        let chrome = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 12, right: 12, top: 6, bottom: 8 });
        egui::Panel::top("edit-toolbar").show_separator_line(false).frame(chrome).show(ui, |ui| self.toolbar(ui));
        if self.s.show_statusbar {
            let st = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 14, right: 14, top: 4, bottom: 6 });
            egui::Panel::bottom("edit-status").show_separator_line(false).frame(st).show(ui, |ui| self.status_line(ui));
        }
        let side = self.s.show_sidebar && self.root.is_some();
        if side {
            let f = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 10, right: 8, top: 0, bottom: 4 });
            egui::Panel::left("edit-side").show_separator_line(false).frame(f).resizable(true).default_size(250.0).size_range(170.0..=480.0).show(ui, |ui| self.sidebar(ui));
        }
        let main = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: if side { 0 } else { 12 }, right: 12, top: 0, bottom: if self.s.show_statusbar { 0 } else { 12 } });
        egui::CentralPanel::default().frame(main).show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().rect_filled(r, r_lg(), v.extreme_bg_color);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::top_down(egui::Align::Min)));
            if self.tabs.is_empty() {
                self.welcome(&mut child);
            } else {
                self.tab_strip(&mut child);
                self.editor(&mut child);
            }
        });
        self.quick_panel(ctx);
        self.prompts(ctx);
    }

    // ---------------------------------------------------------------- menus and toolbar

    fn menus(&mut self, ui: &mut egui::Ui) {
        let has_tab = !self.tabs.is_empty();
        ui.menu_button("File", |ui| {
            menu_style(ui);
            if item(ui, "New File", "Ctrl+N", true) {
                self.new_tab();
            }
            if item(ui, "Open Folder…", "Ctrl+O", true) {
                self.open_quick(quick::QuickKind::Browse { files: false });
            }
            if item(ui, "Open File…", "", true) {
                self.open_quick(quick::QuickKind::Browse { files: true });
            }
            ui.menu_button("Open Recent", |ui| {
                menu_style(ui);
                if self.s.recent.is_empty() {
                    ui.label(egui::RichText::new("Nothing yet").weak());
                }
                for p in self.s.recent.clone() {
                    let home = rivet_xdg::places::home();
                    let shown = p.strip_prefix(&home).map(|r| format!("~/{}", r.display())).unwrap_or_else(|_| p.display().to_string());
                    if item(ui, &shown, "", true) {
                        if p.is_dir() {
                            self.open_folder(p);
                        } else {
                            self.open_file(&p);
                        }
                    }
                }
            });
            ui.separator();
            if item(ui, "Save", "Ctrl+S", has_tab) {
                self.save(self.active);
            }
            if item(ui, "Save All", "Shift+Ctrl+S", has_tab) {
                self.save_all();
            }
            ui.separator();
            if item(ui, "Close Tab", "Ctrl+W", has_tab) {
                self.close_tab(self.active, false);
            }
            if item(ui, "Quit", "Ctrl+Q", true) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.menu_button("Edit", |ui| {
            menu_style(ui);
            if item(ui, "Go to Line…", "Ctrl+G", has_tab) {
                self.open_quick(quick::QuickKind::Line);
            }
            ui.separator();
            if item(ui, "Preferences", "Ctrl+,", true) {
                self.prefs_open = true;
            }
        });
        ui.menu_button("View", |ui| {
            menu_style(ui);
            let before = self.s.clone();
            ui.checkbox(&mut self.s.show_sidebar, "Sidebar (F9)");
            ui.checkbox(&mut self.s.word_wrap, "Word Wrap (Alt+Z)");
            ui.checkbox(&mut self.s.line_numbers, "Line Numbers");
            ui.checkbox(&mut self.s.show_statusbar, "Status Bar");
            ui.checkbox(&mut self.s.show_menubar, "Menu Bar (Ctrl+M)");
            if self.s != before {
                self.s.save();
            }
            ui.separator();
            if item(ui, "Bigger Code", "Ctrl+=", true) {
                self.s.code_size = (self.s.code_size + 1.0).min(32.0);
                self.s.save();
            }
            if item(ui, "Smaller Code", "Ctrl+-", true) {
                self.s.code_size = (self.s.code_size - 1.0).max(8.0);
                self.s.save();
            }
        });
        ui.menu_button("Go", |ui| {
            menu_style(ui);
            if item(ui, "Go to File…", "Ctrl+P", self.root.is_some() || has_tab) {
                self.open_quick(quick::QuickKind::Files);
            }
            if item(ui, "Commands…", "Shift+Ctrl+P", true) {
                self.open_quick(quick::QuickKind::Commands);
            }
            if item(ui, "Next Tab", "Ctrl+Tab", self.tabs.len() > 1) {
                self.active = (self.active + 1) % self.tabs.len();
            }
        });
        ui.menu_button("Help", |ui| {
            menu_style(ui);
            if item(ui, "About Rivet Edit", "", true) {
                self.prefs_open = true;
            }
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let full = ui.max_rect();
        let mut left_end = full.left();
        let row = ui
            .horizontal(|ui| {
                ui.set_min_height(32.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                let flat = |ui: &mut egui::Ui, icon: &Option<egui::TextureHandle>, fallback: &str, tip: &str, on: bool| -> egui::Response {
                    let b = match icon {
                        Some(t) => egui::Button::image(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(16.0)).tint(ui.visuals().text_color())),
                        None => egui::Button::new(fallback),
                    };
                    ui.add_enabled(on, b.frame_when_inactive(false).min_size(Vec2::new(34.0, 32.0)).corner_radius(r_sm())).on_hover_text(tip)
                };
                if !self.s.show_menubar {
                    let btn = egui::Button::new("").min_size(Vec2::new(34.0, 32.0)).frame_when_inactive(false).corner_radius(r_sm());
                    let r = egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui| {
                        ui.set_min_width(220.0);
                        if ui.add(egui::Button::new("Show Menu Bar").shortcut_text("Ctrl+M")).clicked() {
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
                }
                let side_icon = self.icon(&ctx, &["sidebar-show-symbolic", "view-left-pane-symbolic"], 16.0);
                if flat(ui, &side_icon, "▯", "Sidebar (F9)", self.root.is_some()).clicked() {
                    self.s.show_sidebar = !self.s.show_sidebar;
                    self.s.save();
                }
                let add_icon = self.icon(&ctx, &["list-add-symbolic", "tab-new-symbolic"], 16.0);
                let r = flat(ui, &add_icon, "+", "New file (Ctrl+N)", true);
                if r.clicked() {
                    self.new_tab();
                }
                left_end = r.rect.right();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let cmd_icon = self.icon(&ctx, &["open-menu-symbolic", "view-more-symbolic"], 16.0);
                    if flat(ui, &cmd_icon, "⋯", "Commands (Shift+Ctrl+P)", true).clicked() {
                        self.open_quick(quick::QuickKind::Commands);
                    }
                    let folder_icon = self.icon(&ctx, &["folder-open-symbolic", "document-open-symbolic", "folder-symbolic"], 16.0);
                    if flat(ui, &folder_icon, "Open", "Open a folder (Ctrl+O)", true).clicked() {
                        self.open_quick(quick::QuickKind::Browse { files: false });
                    }
                });
            })
            .response
            .rect;
        // "Go to file", centered like the other apps' search, opening the fuzzy file picker.
        let cx = full.center().x;
        let w = ((cx - left_end - 12.0) * 2.0).min(520.0);
        if w < 140.0 {
            return;
        }
        let rect = Rect::from_center_size(Pos2::new(cx, row.center().y), Vec2::new(w, 32.0));
        let resp = ui.interact(rect, egui::Id::unique("edit-goto-box"), Sense::click());
        let fill = palette::mix(v.window_fill, v.text_color(), if resp.hovered() { 0.07 } else { 0.04 });
        let shadow = egui::Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(if v.dark_mode { 90 } else { 40 }) };
        ui.painter().add(shadow.as_shape(rect, r_md()));
        ui.painter().rect_filled(rect, r_md(), fill);
        if let Some(t) = self.icon(&ctx, &["edit-find-symbolic", "system-search-symbolic"], 14.0) {
            paint_tex(ui.painter(), &t, Rect::from_center_size(Pos2::new(rect.left() + 18.0, rect.center().y), Vec2::splat(14.0)), v.weak_text_color());
        }
        let label = match self.workspace_name() {
            Some(w) => format!("Go to a file in {w}…"),
            None => "Go to a file…".into(),
        };
        ui.painter().text(Pos2::new(rect.left() + 34.0, rect.center().y), Align2::LEFT_CENTER, label, fonts::ui_font(13.5), v.weak_text_color());
        ui.painter().text(Pos2::new(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, "Ctrl+P", fonts::ui_font(12.0), v.weak_text_color().gamma_multiply(0.8));
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.open_quick(quick::QuickKind::Files);
        }
    }

    // ---------------------------------------------------------------- the workspace tree

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.set_min_height(26.0);
            ui.label(egui::RichText::new(self.workspace_name().unwrap_or_default().to_uppercase()).font(fonts::ui_font(11.5)).strong().color(v.weak_text_color()));
        });
        if self.scan_running() && self.files.is_empty() {
            ui.label(egui::RichText::new("Listing files…").small().color(v.weak_text_color()));
        }
        if self.files_cut {
            ui.label(egui::RichText::new(format!("Showing the first {} files", workspace::MAX_FILES)).small().color(v.weak_text_color()));
        }
        let folder_icon = self.icon(&ctx, &["folder"], 16.0);
        let open_rel = self.tabs.get(self.active).and_then(|t| t.path.clone()).and_then(|p| self.root.as_ref().and_then(|r| p.strip_prefix(r).ok().map(|x| x.to_string_lossy().replace('\\', "/"))));
        // File icons, looked up once per kind of file.
        let names: Vec<String> = self.files.iter().map(|f| f.rsplit('/').next().unwrap_or(f).to_string()).collect();
        let mut icons: HashMap<String, Option<egui::TextureHandle>> = HashMap::new();
        for n in &names {
            let ext = n.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
            if let std::collections::hash_map::Entry::Vacant(e) = icons.entry(ext) {
                e.insert(self.file_icon(&ctx, n, 16.0));
            }
        }
        let mut pick = None;
        egui::ScrollArea::vertical().id_salt("edit-tree").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            tree(ui, "", 0, &self.folders, &self.files, &self.open_dirs, &folder_icon, &icons, open_rel.as_deref(), &mut pick);
            ui.allocate_space(Vec2::new(ui.available_width(), 30.0));
        });
        match pick {
            Some(Pick::File(rel)) => {
                if let Some(r) = self.root.clone() {
                    self.open_file(&r.join(rel));
                }
            }
            Some(Pick::Fold(d)) => {
                if !self.open_dirs.remove(&d) {
                    self.open_dirs.insert(d);
                }
            }
            Some(Pick::NewHere(d)) => self.prompt = Some(Prompt::NewFile { folder: d, name: String::new() }),
            None => {}
        }
    }

    // ---------------------------------------------------------------- tabs and the editor

    fn tab_strip(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let mut select = None;
        let mut close = None;
        let icons: Vec<Option<egui::TextureHandle>> = (0..self.tabs.len()).map(|i| self.file_icon(&ctx, &self.tabs[i].name(), 14.0)).collect();
        let strip = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), 38.0));
        ui.painter().line_segment([strip.left_bottom() + Vec2::new(12.0, 0.0), strip.right_bottom() - Vec2::new(12.0, 0.0)], Stroke::new(1.0, palette::mix(v.extreme_bg_color, v.text_color(), 0.08)));
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(strip.shrink2(Vec2::new(8.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Max)));
        egui::ScrollArea::horizontal().id_salt("edit-tabs").show(&mut inner, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, t) in self.tabs.iter().enumerate() {
                    let on = i == self.active;
                    let name = t.name();
                    let g = ui.painter().layout_no_wrap(name.clone(), fonts::ui_font(13.0), v.text_color());
                    let w = g.size().x + 64.0;
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::click());
                    if on {
                        ui.painter().rect_filled(r, egui::CornerRadius { nw: r_md(), ne: r_md(), sw: 0, se: 0 }, palette::mix(v.extreme_bg_color, v.text_color(), 0.06));
                        ui.painter().line_segment([r.left_bottom() + Vec2::new(4.0, -1.0), r.right_bottom() + Vec2::new(-4.0, -1.0)], Stroke::new(2.0, v.selection.bg_fill));
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
                    }
                    if let Some(tex) = &icons[i] {
                        paint_tex(ui.painter(), tex, Rect::from_center_size(Pos2::new(r.left() + 16.0, r.center().y), Vec2::splat(14.0)), Color32::WHITE);
                    }
                    let fg = if on { v.strong_text_color() } else { v.weak_text_color() };
                    ui.painter().galley(Pos2::new(r.left() + 30.0, r.center().y - g.size().y / 2.0), g, fg);
                    // Close: a dot while it has changes, × under the pointer.
                    let xr = Rect::from_center_size(Pos2::new(r.right() - 16.0, r.center().y), Vec2::splat(18.0));
                    let over_x = ui.rect_contains_pointer(xr);
                    if t.dirty() && !over_x {
                        ui.painter().circle_filled(xr.center(), 4.0, fg);
                    } else if on || resp.hovered() {
                        if over_x {
                            ui.painter().rect_filled(xr, r_sm(), v.widgets.hovered.weak_bg_fill);
                        }
                        ui.painter().text(xr.center(), Align2::CENTER_CENTER, "×", fonts::ui_font(15.0), fg);
                    }
                    let tip = t.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "Not saved yet".into());
                    let resp = resp.on_hover_text(tip);
                    if resp.clicked() {
                        if over_x { close = Some(i) } else { select = Some(i) }
                    }
                    if resp.middle_clicked() {
                        close = Some(i);
                    }
                }
            });
        });
        ui.advance_cursor_after_rect(strip);
        if let Some(i) = select {
            self.active = i;
        }
        if let Some(i) = close {
            self.close_tab(i, false);
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let accent = self.accent();
        let code_bg = v.extreme_bg_color;
        let colors = highlight::colors(v.text_color(), code_bg, accent, builtin_icons::derive_secondary(accent), builtin_icons::derive_tertiary(accent));
        let font = egui::FontId::monospace(self.s.code_size * fonts::scale());
        let (wrap, numbers, tab_width) = (self.s.word_wrap, self.s.line_numbers, self.s.tab_width);
        let goto = self.goto.take();
        let quick_open = self.quick.is_some() || self.prompt.is_some() || self.prefs_open;
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let name = t.name();
        let id = egui::Id::unique(("edit-body", t.path.clone()));
        // Tab puts in spaces (Shift+Tab is left to move the keyboard).
        if !quick_open && ctx.memory(|m| m.has_focus(id)) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Tab)) {
            let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
            if let Some(r) = st.cursor.char_range() {
                let (a, b) = (r.primary.index.0.min(r.secondary.index.0), r.primary.index.0.max(r.secondary.index.0));
                let byte = |c: usize| t.text.char_indices().nth(c).map(|(i, _)| i).unwrap_or(t.text.len());
                let (ba, bb) = (byte(a), byte(b));
                let line_start = t.text[..ba].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let col = t.text[line_start..ba].chars().count();
                let n = tab_width - col % tab_width;
                t.text.replace_range(ba..bb, &" ".repeat(n));
                st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(a + n))));
                st.store(&ctx, id);
            }
        }
        // Go to a line: the cursor there.
        if let Some(n) = goto {
            let c: usize = t.text.split_inclusive('\n').take(n.saturating_sub(1)).map(|l| l.chars().count()).sum();
            let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(c))));
            st.store(&ctx, id);
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let lines = t.text.split('\n').count();
        let digits = lines.to_string().len().max(3) as f32;
        let char_w = ctx.fonts_mut(|f| f.glyph_width(&font, '0'));
        let gutter = if numbers { digits * char_w + 28.0 } else { 14.0 };
        let Tab { text, cache, line_col, .. } = t;
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_w: f32| {
            let mut job = cache.job(buf.as_str(), &name, &colors, &font);
            job.wrap.max_width = if wrap { wrap_w } else { f32::INFINITY };
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let area = ui.available_rect_before_wrap();
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area.shrink2(Vec2::new(0.0, 4.0))).layout(egui::Layout::top_down(egui::Align::Min)));
        let scroll = if wrap { egui::ScrollArea::vertical() } else { egui::ScrollArea::both() };
        scroll.id_salt(("edit-scroll", id)).auto_shrink([false, false]).show(&mut body, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let left = ui.cursor().left();
                ui.add_space(gutter);
                let out = egui::TextEdit::multiline(text)
                    .id(id)
                    .code_editor()
                    .font(font.clone())
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin { left: 4, right: 16, top: 6, bottom: 6 })
                    .desired_width(if wrap { ui.available_width() } else { f32::INFINITY })
                    .desired_rows(40)
                    .lock_focus(true)
                    .layouter(&mut layouter)
                    .show(ui);
                // The cursor's line and column, for the status bar and the gutter.
                let cur = out.cursor_range.map(|r| r.primary.index.0);
                if let Some(c) = cur {
                    let before: String = text.chars().take(c).collect();
                    let line = before.matches('\n').count() + 1;
                    let col = before.rsplit('\n').next().map(|s| s.chars().count()).unwrap_or(0) + 1;
                    *line_col = (line, col);
                }
                // Line numbers (once per line, also when it wraps), the cursor's brighter.
                if numbers {
                    let p = ui.painter();
                    let mut n = 1;
                    let mut first = true;
                    for row in &out.galley.rows {
                        if first {
                            let y = out.galley_pos.y + row.pos.y + row.rect().height() / 2.0;
                            let on = n == line_col.0;
                            let col = if on { v.text_color() } else { v.weak_text_color().gamma_multiply(0.7) };
                            p.text(Pos2::new(left + gutter - 14.0, y), Align2::RIGHT_CENTER, n.to_string(), font.clone(), col);
                            if on && cur.is_some() {
                                let r = Rect::from_min_max(Pos2::new(left, out.galley_pos.y + row.pos.y), Pos2::new(ui.clip_rect().right(), out.galley_pos.y + row.pos.y + row.rect().height()));
                                p.rect_filled(r, 0.0, v.text_color().gamma_multiply(0.035));
                            }
                        }
                        first = row.ends_with_newline;
                        if row.ends_with_newline {
                            n += 1;
                        }
                    }
                }
                if goto.is_some() {
                    let cc = out.cursor_range.map(|r| r.primary).unwrap_or_default();
                    let r = out.galley.pos_from_cursor(cc).translate(out.galley_pos.to_vec2());
                    ui.scroll_to_rect(r.expand2(Vec2::new(0.0, 120.0)), Some(egui::Align::Center));
                }
            });
        });
    }

    fn status_line(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let weak = v.weak_text_color();
        let ws_icon = self.icon(&ctx, &["folder-symbolic"], 14.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if let Some(w) = self.workspace_name() {
                if let Some(t) = &ws_icon {
                    ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(14.0)).tint(weak));
                }
                ui.label(egui::RichText::new(w).color(weak)).on_hover_text(self.root.as_ref().map(|r| r.display().to_string()).unwrap_or_default());
            }
            if let Some((m, bad)) = &self.note {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(m).color(if *bad { status(&ctx).danger } else { weak }));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                if let Some(t) = self.tabs.get(self.active) {
                    ui.label(egui::RichText::new(crate::syntax::language_name(&t.name())).color(weak));
                    ui.label(egui::RichText::new(format!("Spaces: {}", self.s.tab_width)).color(weak));
                    ui.label(egui::RichText::new(format!("Ln {}, Col {}", t.line_col.0, t.line_col.1)).color(weak));
                }
            });
        });
    }

    // ---------------------------------------------------------------- welcome

    fn welcome(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let icon = self.app_icon(&ctx);
        let r = ui.max_rect();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(r.center(), Vec2::new(380.0, 420.0).min(r.size()))).layout(egui::Layout::top_down(egui::Align::Center)));
        let ui = &mut child;
        if let Some(t) = &icon {
            ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(72.0)));
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Rivet Edit").size(22.0).strong().color(v.strong_text_color()));
        ui.add_space(14.0);
        let row = |ui: &mut egui::Ui, label: &str, keys: &str| -> bool {
            let (rr, resp) = ui.allocate_exact_size(Vec2::new(300.0, 34.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rr, r_sm(), v.widgets.hovered.weak_bg_fill);
            }
            ui.painter().text(rr.left_center() + Vec2::new(12.0, 0.0), Align2::LEFT_CENTER, label, fonts::ui_font(14.0), v.hyperlink_color);
            ui.painter().text(rr.right_center() - Vec2::new(12.0, 0.0), Align2::RIGHT_CENTER, keys, fonts::ui_font(12.0), v.weak_text_color());
            resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
        };
        if self.root.is_some() {
            if row(ui, "Go to a file…", "Ctrl+P") {
                self.open_quick(quick::QuickKind::Files);
            }
        } else if row(ui, "Open a folder…", "Ctrl+O") {
            self.open_quick(quick::QuickKind::Browse { files: false });
        }
        if row(ui, "Open a file…", "") {
            self.open_quick(quick::QuickKind::Browse { files: true });
        }
        if row(ui, "New file", "Ctrl+N") {
            self.new_tab();
        }
        if row(ui, "Commands", "Shift+Ctrl+P") {
            self.open_quick(quick::QuickKind::Commands);
        }
        let recent: Vec<PathBuf> = self.s.recent.iter().filter(|p| Some(*p) != self.root.as_ref()).take(5).cloned().collect();
        if !recent.is_empty() {
            ui.add_space(18.0);
            ui.label(egui::RichText::new("RECENT").font(fonts::ui_font(11.0)).color(v.weak_text_color()));
            ui.add_space(4.0);
            for p in recent {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if row(ui, &name, if p.is_dir() { "folder" } else { "" }) {
                    if p.is_dir() {
                        self.open_folder(p);
                    } else {
                        self.open_file(&p);
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------- questions

    fn prompts(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.prompt.take() else { return };
        let mut next = Some(prompt);
        let mut act: Option<Act> = None;
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(24)).corner_radius(r_lg());
        let names: Vec<String> = self.tabs.iter().map(Tab::name).collect();
        let dirty: Vec<String> = self.tabs.iter().filter(|t| t.dirty()).map(Tab::name).collect();
        let ws = self.workspace_name().unwrap_or_default();
        egui::Modal::new(egui::Id::unique("edit-prompt")).frame(frame).show(ctx, |ui| {
            ui.set_width(440.0);
            let v = ui.visuals().clone();
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            let title = |ui: &mut egui::Ui, t: &str, sub: &str| {
                ui.label(egui::RichText::new(t).heading().color(v.strong_text_color()));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(sub).color(v.weak_text_color()));
                ui.add_space(16.0);
            };
            let buttons = |ui: &mut egui::Ui, labels: &[&str]| -> Option<usize> {
                let mut hit = None;
                ui.add_space(18.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().button_padding = Vec2::new(16.0, 7.0);
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (i, l) in labels.iter().enumerate() {
                        let b = if i == 0 { egui::Button::new(egui::RichText::new(*l).color(v.selection.stroke.color).strong()).fill(v.selection.bg_fill) } else { egui::Button::new(*l) };
                        if ui.add(b.corner_radius(r_sm())).clicked() {
                            hit = Some(i);
                        }
                    }
                });
                hit
            };
            let field = |ui: &mut egui::Ui, text: &mut String, hint: &str| -> bool {
                let id = egui::Id::unique(("edit-field", hint));
                let r = egui::Frame::NONE
                    .fill(palette::mix(v.window_fill, v.text_color(), 0.06))
                    .stroke(Stroke::new(1.5, if ui.memory(|m| m.has_focus(id)) { v.selection.bg_fill } else { Color32::TRANSPARENT }))
                    .corner_radius(r_md())
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| ui.add(egui::TextEdit::singleline(text).id(id).frame(egui::Frame::NONE).font(fonts::ui_font(14.5)).hint_text(hint).desired_width(f32::INFINITY)))
                    .inner;
                if !r.has_focus() && !r.lost_focus() {
                    r.request_focus();
                }
                r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
            };
            match next.take().unwrap() {
                Prompt::CloseTab(i) => {
                    let name = names.get(i).cloned().unwrap_or_default();
                    title(ui, &format!("Save changes to {name}?"), "They'll be lost if you don't.");
                    match buttons(ui, &["Save", "Don't Save", "Cancel"]) {
                        Some(0) => act = Some(Box::new(move |a: &mut EditApp| {
                            if a.save(i) {
                                a.close_tab(i, true);
                            }
                        })),
                        Some(1) => act = Some(Box::new(move |a: &mut EditApp| a.close_tab(i, true))),
                        Some(_) => {}
                        None if esc => {}
                        None => next = Some(Prompt::CloseTab(i)),
                    }
                }
                Prompt::Quit => {
                    title(ui, "Quit with changes not saved?", &dirty.join(", "));
                    match buttons(ui, &["Save All", "Quit Anyway", "Cancel"]) {
                        Some(0) => act = Some(Box::new(|a: &mut EditApp| {
                            a.save_all();
                            if !a.tabs.iter().any(Tab::dirty) {
                                a.allow_close = true;
                                a.quit();
                            }
                        })),
                        Some(1) => act = Some(Box::new(|a: &mut EditApp| {
                            a.allow_close = true;
                            a.quit();
                        })),
                        Some(_) => {}
                        None if esc => {}
                        None => next = Some(Prompt::Quit),
                    }
                }
                Prompt::SaveAs { tab, mut path } => {
                    title(ui, "Save As", "Where to save the new file (folders are made as needed).");
                    let enter = field(ui, &mut path, "~/file.txt");
                    match buttons(ui, &["Save", "Cancel"]) {
                        Some(0) => act = Some(save_as(tab, path.clone())),
                        Some(_) => {}
                        None if esc => {}
                        None if enter => act = Some(save_as(tab, path.clone())),
                        None => next = Some(Prompt::SaveAs { tab, path }),
                    }
                }
                Prompt::NewFile { folder, mut name } => {
                    let where_ = if folder.is_empty() { ws.clone() } else { format!("{ws}/{folder}") };
                    title(ui, "New File", &format!("In {where_}. Type a path like src/new.rs to make folders too."));
                    let enter = field(ui, &mut name, "name.rs");
                    match buttons(ui, &["Create", "Cancel"]) {
                        Some(0) => act = Some(new_file(folder.clone(), name.clone())),
                        Some(_) => {}
                        None if esc => {}
                        None if enter => act = Some(new_file(folder.clone(), name.clone())),
                        None => next = Some(Prompt::NewFile { folder, name }),
                    }
                }
            }
        });
        self.prompt = next;
        if let Some(f) = act {
            f(self);
        }
    }

    fn quit(&mut self) {
        // Asked from inside the window: close it (the close request is let through now).
        self.prompt = None;
        QUIT.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Something a question's answer does to the app.
type Act = Box<dyn FnOnce(&mut EditApp)>;

/// Set when the window should close (read each frame by `main`'s close check).
pub(crate) static QUIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn save_as(tab: usize, path: String) -> Act {
    Box::new(move |a: &mut EditApp| {
        let p = path.trim();
        let p = match p.strip_prefix("~/") {
            Some(rest) => rivet_xdg::places::home().join(rest),
            None => PathBuf::from(p),
        };
        if p.as_os_str().is_empty() {
            return;
        }
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Some(t) = a.tabs.get_mut(tab) {
            t.path = Some(p.clone());
        }
        if a.save(tab) {
            a.rescan();
        }
    })
}

fn new_file(folder: String, name: String) -> Act {
    Box::new(move |a: &mut EditApp| {
        let name = name.trim().trim_start_matches('/');
        let Some(root) = a.root.clone() else { return };
        if name.is_empty() || name.contains("..") {
            return;
        }
        let p = root.join(&folder).join(name);
        if p.exists() {
            a.open_file(&p);
            return;
        }
        if crate::settings::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed) && std::env::var_os("RIVET_EDIT_WRITE").is_none() {
            return;
        }
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        match std::fs::write(&p, "") {
            Ok(()) => {
                a.rescan();
                a.open_file(&p);
            }
            Err(e) => a.note = Some((format!("Couldn't make {}: {e}", p.display()), true)),
        }
    })
}

/// A folder (and what's in it) in the tree, then its files.
#[allow(clippy::too_many_arguments)]
fn tree(
    ui: &mut egui::Ui,
    folder: &str,
    depth: usize,
    folders: &[String],
    files: &[String],
    open_dirs: &HashSet<String>,
    folder_icon: &Option<egui::TextureHandle>,
    icons: &HashMap<String, Option<egui::TextureHandle>>,
    open: Option<&str>,
    pick: &mut Option<Pick>,
) {
    let parent_of = |f: &str| f.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
    let v = ui.visuals().clone();
    let indent = 14.0;
    for sub in folders.iter().filter(|f| parent_of(f) == folder) {
        let name = sub.rsplit('/').next().unwrap_or(sub);
        let is_open = open_dirs.contains(sub);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, r_sm(), v.widgets.hovered.weak_bg_fill);
        }
        let x = rect.left() + 6.0 + depth as f32 * indent;
        let c = Pos2::new(x + 4.0, rect.center().y);
        let pts = if is_open { vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)] } else { vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, 4.0), c + Vec2::new(3.0, 0.0)] };
        ui.painter().add(egui::Shape::convex_polygon(pts, v.weak_text_color(), Stroke::NONE));
        if let Some(t) = folder_icon {
            paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(x + 20.0, rect.center().y), Vec2::splat(16.0)), Color32::WHITE);
        }
        ui.painter().with_clip_rect(rect).text(Pos2::new(x + 32.0, rect.center().y), Align2::LEFT_CENTER, name, fonts::ui_font(13.0), v.text_color());
        if resp.clicked() {
            *pick = Some(Pick::Fold(sub.clone()));
        }
        resp.context_menu(|ui| {
            menu_style(ui);
            if item(ui, "New File Here…", "", true) {
                *pick = Some(Pick::NewHere(sub.clone()));
            }
        });
        if is_open {
            let top = rect.bottom();
            let guide = ui.painter().add(egui::Shape::Noop);
            tree(ui, sub, depth + 1, folders, files, open_dirs, folder_icon, icons, open, pick);
            let bottom = ui.min_rect().bottom();
            if bottom > top + 1.0 {
                let col = palette::mix(v.panel_fill, v.text_color(), if v.dark_mode { 0.16 } else { 0.20 });
                ui.painter().set(guide, egui::Shape::line_segment([Pos2::new(c.x, top + 2.0), Pos2::new(c.x, bottom - 4.0)], Stroke::new(1.0, col)));
            }
        }
    }
    for f in files.iter().filter(|f| parent_of(f) == folder) {
        let name = f.rsplit('/').next().unwrap_or(f);
        let on = open == Some(f.as_str());
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
        if on {
            let (fill, stroke) = selected_tile(&v, v.panel_fill);
            ui.painter().rect_filled(rect, r_sm(), fill);
            ui.painter().rect_stroke(rect, r_sm(), stroke, egui::StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, r_sm(), v.widgets.hovered.weak_bg_fill);
        }
        let x = rect.left() + 6.0 + depth as f32 * indent + 12.0;
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
        if let Some(Some(t)) = icons.get(&ext) {
            paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(x + 8.0, rect.center().y), Vec2::splat(16.0)), Color32::WHITE);
        }
        let fg = if on { v.strong_text_color() } else { v.text_color() };
        ui.painter().with_clip_rect(rect).text(Pos2::new(x + 22.0, rect.center().y), Align2::LEFT_CENTER, name, fonts::ui_font(13.0), fg);
        if resp.on_hover_text(f.as_str()).clicked() {
            *pick = Some(Pick::File(f.clone()));
        }
    }
}

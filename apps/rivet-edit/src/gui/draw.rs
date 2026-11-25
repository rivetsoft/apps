//! Drawing the window: menus, toolbar, the workspace tree, tabs, the editor with line numbers,
//! the status bar, the welcome page and the questions.

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{paint_tex, r_lg, r_md, r_sm, selected_tile, status};
use rivet_look::{builtin_icons, fonts, highlight, palette};

use super::*;

const ROW_H: f32 = 26.0;

pub(crate) fn menu_style(ui: &mut egui::Ui) {
    ui.set_min_width(230.0);
    ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
    ui.spacing_mut().item_spacing.y = 3.0;
}

pub(crate) fn item(ui: &mut egui::Ui, label: &str, shortcut: &str, on: bool) -> bool {
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
    /// Open it to keep (not as a preview).
    Keep(String),
    Fold(String),
    /// From the right-click menu.
    Menu(treemenu::TreeAct),
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
            if self.tabs.is_empty() {
                ui.painter().rect_filled(r, r_lg(), v.extreme_bg_color);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::top_down(egui::Align::Min)));
                self.welcome(&mut child);
            } else {
                // The tabs sit on the window, like the sidebar (the app's look); the code card
                // below them takes the color scheme's.
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::top_down(egui::Align::Min)));
                self.tab_strip(&mut child);
                let rest = Rect::from_min_max(Pos2::new(r.left(), child.cursor().top()), r.max);
                let mut code = ui.new_child(egui::UiBuilder::new().max_rect(rest).layout(egui::Layout::top_down(egui::Align::Min)));
                let fill = match self.scheme().map(|s| s.visuals(&v)) {
                    Some(sv) => {
                        let f = sv.extreme_bg_color;
                        code.style_mut().visuals = sv;
                        f
                    }
                    None => v.extreme_bg_color,
                };
                ui.painter().rect_filled(rest, r_lg(), fill);
                // The scroll fades would take the window's color, not the code's.
                code.spacing_mut().scroll.fade.strength = 0.0;
                self.editor(&mut code);
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
            if item(ui, "New Window", "Shift+Ctrl+N", self.shot.is_none()) {
                self.new_window(None);
            }
            if item(ui, "Open Folder…", "Ctrl+O", true) {
                self.open_quick(quick::QuickKind::Browse { files: false, window: false });
            }
            if item(ui, "Open Folder in New Window…", "", true) {
                self.open_quick(quick::QuickKind::Browse { files: false, window: true });
            }
            if item(ui, "Open File…", "", true) {
                self.open_quick(quick::QuickKind::Browse { files: true, window: false });
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
            if item(ui, "Undo", "Ctrl+Z", has_tab) {
                self.undo_asked = Some(false);
            }
            if item(ui, "Redo", "Shift+Ctrl+Z", has_tab) {
                self.undo_asked = Some(true);
            }
            ui.separator();
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
        ui.menu_button("Git", |ui| {
            menu_style(ui);
            let repo = self.git.repo.is_some();
            let busy = self.git.running.is_some();
            let up = self.git.status.as_ref().is_some_and(|s| s.branch.upstream.is_some());
            if item(ui, "Show Git", "Shift+Ctrl+G", self.root.is_some()) {
                self.show_git();
            }
            ui.separator();
            if item(ui, "Commit", "", repo && !busy) {
                self.commit();
            }
            if item(ui, "Stage This File", "", repo && !busy && has_tab) {
                self.stage_open(true);
            }
            if item(ui, "Unstage This File", "", repo && !busy && has_tab) {
                self.stage_open(false);
            }
            ui.separator();
            if item(ui, "Next Change", "Alt+F5", has_tab) {
                self.step_change(false);
            }
            if item(ui, "Previous Change", "Shift+Alt+F5", has_tab) {
                self.step_change(true);
            }
            ui.separator();
            if item(ui, "Switch Branch…", "", repo && !busy) {
                self.open_branches();
            }
            if item(ui, "Fetch", "", repo && !busy) {
                self.git.run(crate::vcs::Op::Fetch);
            }
            if item(ui, "Pull", "", repo && !busy && up) {
                self.git.run(crate::vcs::Op::Pull);
            }
            if item(ui, if up || !repo { "Push" } else { "Publish Branch" }, "", repo && !busy) {
                self.git.run(crate::vcs::Op::Push);
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
                left_end = ui.min_rect().right();
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
            ui.spacing_mut().item_spacing.x = 2.0;
            let changes = self.git.status.as_ref().map(|s| s.entries.len()).unwrap_or(0);
            let git_label = if changes > 0 { format!("GIT {changes}") } else { "GIT".to_string() };
            for (side, label, tip) in [(editing::Side::Files, "FILES", "The folder's files"), (editing::Side::Search, "SEARCH", "Search the folder (Shift+Ctrl+F)"), (editing::Side::Git, git_label.as_str(), "Changes, commits and branches (Shift+Ctrl+G)")] {
                let on = self.side == side;
                let col = if on { v.text_color() } else { v.weak_text_color() };
                let b = egui::Button::new(egui::RichText::new(label).font(fonts::ui_font(11.5)).strong().color(col)).frame_when_inactive(false).corner_radius(r_sm());
                let b = if on { b.fill(selected_tile(&v, v.panel_fill).0) } else { b };
                if ui.add(b).on_hover_text(tip).clicked() {
                    self.side = side;
                    if side == editing::Side::Search {
                        self.search.focus = true;
                    }
                }
            }
        });
        if self.side == editing::Side::Search {
            ui.add_space(4.0);
            self.search_panel(ui);
            return;
        }
        if self.side == editing::Side::Git {
            self.git_panel(ui);
            return;
        }
        if self.scan_running() && self.files.is_empty() {
            ui.label(egui::RichText::new("Listing files…").small().color(v.weak_text_color()));
        }
        if self.files_cut {
            ui.label(egui::RichText::new(format!("Showing the first {} files", workspace::MAX_FILES)).small().color(v.weak_text_color()));
        }
        let open_rel = self.tabs.get(self.active).and_then(|t| t.path.clone()).and_then(|p| self.root.as_ref().and_then(|r| p.strip_prefix(r).ok().map(|x| x.to_string_lossy().replace('\\', "/"))));
        // Icons for what's showing: folders by name (open or shut), files by name.
        let parent_of = |f: &str| f.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
        let shown = |f: &str, open_dirs: &HashSet<String>| {
            let p = parent_of(f);
            p.is_empty() || open_dirs.contains(&p)
        };
        let vis_folders: Vec<String> = self.folders.iter().filter(|f| shown(f, &self.open_dirs)).cloned().collect();
        let mut folder_icons: HashMap<String, Option<egui::TextureHandle>> = HashMap::new();
        for f in vis_folders {
            let name = f.rsplit('/').next().unwrap_or(&f).to_string();
            let open = self.open_dirs.contains(&f);
            let t = self.folder_icon(&ctx, &name, open, 16.0);
            folder_icons.insert(f, t);
        }
        let names: Vec<String> = self.files.iter().filter(|f| shown(f, &self.open_dirs)).map(|f| f.rsplit('/').next().unwrap_or(f).to_string()).collect();
        let mut icons: HashMap<String, Option<egui::TextureHandle>> = HashMap::new();
        for n in names {
            if let std::collections::hash_map::Entry::Vacant(e) = icons.entry(n) {
                let t = self.file_icon(&ctx, e.key(), 16.0);
                e.insert(t);
            }
        }
        let mut pick = None;
        egui::ScrollArea::vertical().id_salt("edit-tree").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let marks = Marks { files: &self.git.marks, dirs: &self.git.marked_dirs };
            tree(ui, "", 0, &self.folders, &self.files, &self.open_dirs, &folder_icons, &icons, open_rel.as_deref(), &marks, &mut pick);
            // The space below the tree: the workspace's own menu.
            let h = (ui.available_height()).max(30.0);
            let (_, below) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
            below.context_menu(|ui| {
                let mut act = None;
                treemenu::tree_menu(ui, "", true, &mut act);
                if let Some(a) = act {
                    pick = Some(Pick::Menu(a));
                }
            });
        });
        match pick {
            Some(Pick::File(rel)) => {
                if let Some(r) = self.root.clone() {
                    self.preview_file(&r.join(rel));
                }
            }
            Some(Pick::Keep(rel)) => {
                if let Some(r) = self.root.clone() {
                    self.open_file(&r.join(rel));
                }
            }
            Some(Pick::Fold(d)) => {
                if !self.open_dirs.remove(&d) {
                    self.open_dirs.insert(d);
                }
            }
            Some(Pick::Menu(a)) => self.tree_act(&ctx, a),
            None => {}
        }
    }

    // ---------------------------------------------------------------- tabs and the editor

    fn tab_strip(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let tabs: Vec<rivet_look::tabs::Tab> = (0..self.tabs.len())
            .map(|i| {
                let name = self.tabs[i].name();
                let icon = self.file_icon(&ctx, &name, 14.0);
                let t = &self.tabs[i];
                let mut tip = t.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "Not saved yet".into());
                if t.preview {
                    tip.push_str("\nA preview: double-click to keep it open");
                }
                rivet_look::tabs::Tab { label: name, tip: Some(tip), icon, italic: t.preview, dirty: t.dirty(), badge: None }
            })
            .collect();
        // The shared strip (Rivet Terminal's and Rivet Files' too), with a clear gap under it so
        // the open tab's rounded corners don't sit on the code card's.
        ui.add_space(2.0);
        let out = rivet_look::tabs::TabStrip::new("edit-tabs", v.selection.bg_fill).widths(0.0, 320.0).show(ui, &tabs, self.active);
        ui.add_space(8.0);
        let mut close = None;
        for e in &out.events {
            match *e {
                rivet_look::tabs::TabEvent::Swap(a, b) => self.tabs.swap(a, b),
                rivet_look::tabs::TabEvent::Keep(i) => {
                    if let Some(t) = self.tabs.get_mut(i) {
                        t.preview = false;
                    }
                }
                rivet_look::tabs::TabEvent::Close(i) => close = Some(i),
                rivet_look::tabs::TabEvent::Select(_) => {}
            }
        }
        self.active = out.active(self.active);
        if let Some(i) = close {
            self.close_tab(i, false);
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let accent = self.accent();
        let code_bg = v.extreme_bg_color;
        let colors = match self.scheme() {
            Some(s) => s.colors(),
            None => highlight::colors(v.text_color(), code_bg, accent, builtin_icons::derive_secondary(accent), builtin_icons::derive_tertiary(accent)),
        };
        let font = egui::FontId::monospace(self.s.code_size * fonts::scale());
        let (wrap, numbers) = (self.s.word_wrap, self.s.line_numbers);
        let line_height = self.s.line_height;
        let goto = self.goto.take();
        let quick_open = self.quick.is_some() || self.prompt.is_some() || self.prefs_open;
        if self.find.is_some() {
            self.find_bar(ui);
        }
        let Some(id) = self.tabs.get(self.active).map(|t| egui::Id::unique(("edit-body", t.path.clone()))) else { return };
        // The suggestion list's keys come first (Enter, Tab, arrows), then the editor's.
        let ctrl_space = if quick_open {
            self.complete = None;
            false
        } else {
            self.complete_keys(&ctx, id)
        };
        // Vim keys take what they need before anything else edits.
        if !quick_open {
            self.vim_keys(&ctx, id);
        }
        let vim_visual = matches!(self.vim.mode, crate::vim::Mode::Visual | crate::vim::Mode::VisualLine);
        let vim_head = self.vim.head;
        let vim_block = self.s.vim_keys && self.vim.mode != crate::vim::Mode::Insert && ctx.memory(|m| m.has_focus(id));
        // What was typed this frame (to open or narrow the suggestions).
        let typed: Option<char> = if quick_open { None } else { ctx.input(|i| i.events.iter().rev().find_map(|e| if let egui::Event::Text(t) = e { t.chars().last() } else { None })) };
        // Undo and redo (Vim's u and Ctrl+R come here as Ctrl+Z too), before the text box's own.
        self.undo_keys(&ctx, id, !quick_open);
        if !quick_open {
            self.editor_keys(&ctx, id);
        }
        let diags = self.diag_ranges();
        let mut caret: Option<(usize, Pos2)> = None;
        // The character under a resting pointer (for hover), and a Ctrl+click (Go to Definition).
        let mut probe: Option<(usize, Pos2)> = None;
        let mut ctrl_click = false;
        // A right-click on the text: the response (for its menu) and the character clicked.
        let mut right: Option<(egui::Response, Option<usize>, (usize, usize))> = None;
        // A found match or search result: select it and show it.
        let select = self.select_after.take();
        if let Some((a, b)) = select {
            editing::set_primary(&ctx, id, crate::ops::Sel::new(a, b));
            if let Some(t) = self.tabs.get_mut(self.active) {
                t.sels.clear();
            }
        }
        let reveal = std::mem::take(&mut self.reveal) || select.is_some();
        let find_matches: Vec<(usize, usize)> = self.find.as_ref().map(|f| f.matches.clone()).unwrap_or_default();
        let mut layout_out: Option<editing::LastLayout> = None;
        let alt_click = std::mem::take(&mut self.alt_click);
        let mark_color = accent;
        // Git: the changes since staged, and the change whose box is open.
        self.update_gutter();
        let open_path = self.tabs.get(self.active).and_then(|t| t.path.clone());
        let peek_k = self.peek.as_ref().filter(|p| open_path.as_ref() == Some(&p.path)).map(|p| p.hunk);
        let mut change_click: Option<usize> = None;
        let mut peek_at: Option<(Rect, f32, f32)> = None;
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let name = t.name();
        let (pick, ind) = (t.lang, t.indent);
        let multi_sels = t.sels.clone();
        // Tab puts in a tab or spaces, as the file is indented (Shift+Tab is left to move the
        // keyboard).
        if !quick_open && multi_sels.len() < 2 && ctx.memory(|m| m.has_focus(id)) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Tab)) {
            let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
            if let Some(r) = st.cursor.char_range() {
                let (a, b) = (r.primary.index.0.min(r.secondary.index.0), r.primary.index.0.max(r.secondary.index.0));
                let byte = |c: usize| t.text.char_indices().nth(c).map(|(i, _)| i).unwrap_or(t.text.len());
                let (ba, bb) = (byte(a), byte(b));
                let line_start = t.text[..ba].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let col = t.text[line_start..ba].chars().count();
                let put = if ind.tabs { "\t".to_string() } else { " ".repeat(ind.width - col % ind.width.max(1)) };
                let n = put.chars().count();
                t.text.replace_range(ba..bb, &put);
                st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(a + n))));
                st.store(&ctx, id);
            }
        }
        // Go to a line: the cursor there.
        if let Some(n) = goto {
            let c = t.text.line_start(n.clamp(1, t.text.line_count()));
            let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(c))));
            st.store(&ctx, id);
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let digits = t.text.line_count().to_string().len().max(3) as f32;
        let char_w = ctx.fonts_mut(|f| f.glyph_width(&font, '0'));
        let gutter = if numbers { digits * char_w + 28.0 } else { 14.0 };
        let Tab { text, cache, line_col, gutter: changes, .. } = t;
        let generation = text.generation_handle();
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_w: f32| cache.galley(ui, buf.as_str(), generation.load(std::sync::atomic::Ordering::Relaxed), &name, pick, &colors, &font, line_height, if wrap { wrap_w } else { f32::INFINITY });
        // egui's own undo would keep up to a hundred copies of the whole text, taken every
        // pause; Edit has its own (`History`), so the text box keeps the least it can.
        let undoer_set = id.with("rivet-undoer");
        if ctx.data(|d| d.get_temp::<bool>(undoer_set).is_none()) || egui::TextEdit::load_state(&ctx, id).is_none() {
            let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
            st.set_undoer(egui::util::undoer::Undoer::with_settings(egui::util::undoer::Settings { max_undos: 1, ..Default::default() }));
            st.store(&ctx, id);
            ctx.data_mut(|d| d.insert_temp(undoer_set, true));
        }
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
                    // Tab indents, and Esc stays in the text (Vim's Normal mode, closing
                    // suggestions) rather than taking the keyboard away.
                    .event_filter(egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true })
                    .layouter(&mut layouter)
                    .show(ui);
                // The cursor's line and column, for the status bar and the gutter.
                let cur = if vim_block && vim_visual { Some(vim_head) } else { out.cursor_range.map(|r| r.primary.index.0) };
                if let Some(c) = cur {
                    *line_col = text.line_col(c.min(text.char_count()));
                }
                // Line numbers (once per line, also when it wraps), the cursor's brighter. Only
                // those on screen are drawn: each is text to lay out, every frame.
                if numbers {
                    let p = ui.painter();
                    let clip = ui.clip_rect();
                    let mut n = 1;
                    let mut first = true;
                    for row in &out.galley.rows {
                        let top = out.galley_pos.y + row.pos.y;
                        if top > clip.bottom() {
                            break;
                        }
                        if first && top + row.rect().height() >= clip.top() {
                            let y = top + row.rect().height() / 2.0;
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
                // Git's marks between the numbers and the text; a click opens a change's box.
                let marks = EditApp::paint_changes(ui, &out.galley, out.galley_pos, ui.clip_rect(), left + gutter - 9.0, &changes.hunks, peek_k);
                for (k, r) in &marks {
                    let hit = ui.interact(r.expand2(Vec2::new(4.0, 0.0)), id.with(("git-change", *k)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    if hit.clicked() {
                        change_click = Some(*k);
                    }
                    if peek_k == Some(*k) {
                        peek_at = Some((*r, out.galley_pos.x, ui.clip_rect().right() - out.galley_pos.x - 24.0));
                    }
                }
                // Vim's Normal and Visual modes: a block cursor on the character.
                if vim_block && let Some(r) = out.cursor_range {
                    let at = if vim_visual { egui::text::CCursor::new(vim_head) } else { r.primary };
                    let cr = out.galley.pos_from_cursor(at).translate(out.galley_pos.to_vec2());
                    let w = ui.fonts_mut(|f| f.glyph_width(&font, 'M'));
                    let block = Rect::from_min_size(cr.min, Vec2::new(w, cr.height()));
                    ui.painter().rect_filled(block, 1.0, mark_color.gamma_multiply(0.55));
                }
                // The server's problems, with wavy lines.
                complete::draw_diags(ui, &out, text, &diags);
                if let Some(r) = out.cursor_range {
                    let cr = out.galley.pos_from_cursor(r.primary).translate(out.galley_pos.to_vec2());
                    caret = Some((r.primary.index.0, cr.left_bottom()));
                    // K's box sits above the cursor's line.
                    if let Some(c) = caret.as_mut() {
                        c.1 = cr.left_top();
                    }
                }
                let resp = &out.response.response;
                if let Some(pp) = ui.ctx().pointer_hover_pos().filter(|pp| resp.rect.contains(*pp) && ui.clip_rect().contains(*pp))
                    && !ui.input(|i| i.pointer.any_down())
                {
                    let c = out.galley.cursor_from_pos(pp - out.galley_pos);
                    let r = out.galley.pos_from_cursor(c).translate(out.galley_pos.to_vec2());
                    // Only over the text itself, not the empty space after a line.
                    if (pp.x - r.left()).abs() < 400.0 && pp.y >= r.top() - 2.0 && pp.y <= r.bottom() + 2.0 {
                        probe = Some((c.index.0, r.left_top()));
                    }
                }
                ctrl_click = resp.clicked() && ui.input(|i| i.modifiers.command);
                {
                    let sel = out.cursor_range.map(|r| (r.secondary.index.0, r.primary.index.0)).unwrap_or_default();
                    let at = if resp.secondary_clicked() { resp.interact_pointer_pos().map(|pp| out.galley.cursor_from_pos(pp - out.galley_pos).index.0) } else { None };
                    right = Some((resp.clone(), at, sel));
                }
                // Matches and the other cursors, over the text.
                let main = out.cursor_range.map(|r| crate::ops::Sel::new(r.secondary.index.0, r.primary.index.0)).unwrap_or_default();
                EditApp::draw_marks(ui.painter(), &out.galley, out.galley_pos, ui.clip_rect(), &multi_sels, &find_matches, main, &v, mark_color);
                let _ = alt_click;
                layout_out = Some(editing::LastLayout { galley: out.galley.clone(), pos: out.galley_pos, rect: out.response.response.rect });
                if goto.is_some() || reveal {
                    let cc = out.cursor_range.map(|r| r.primary).unwrap_or_default();
                    let r = out.galley.pos_from_cursor(cc).translate(out.galley_pos.to_vec2());
                    ui.scroll_to_rect(r.expand2(Vec2::new(0.0, 120.0)), Some(egui::Align::Center));
                }
            });
        });
        self.last_layout = layout_out;
        if let (Some(k), Some(p)) = (change_click, open_path) {
            self.peek = if peek_k == Some(k) { None } else { Some(super::scm::Peek { path: p, hunk: k }) };
        } else if let Some((r, x, w)) = peek_at {
            self.peek_box(&ctx, r, x, w, &font);
        }
        // What this frame changed, for undo.
        if let Some(t) = self.tabs.get_mut(self.active) {
            let now = editing::sels_now(&ctx, id, t);
            t.observe(&now);
        }
        if let (Some((c, _)), Some(p)) = (caret, self.tabs.get(self.active).and_then(|t| t.path.clone())) {
            self.last_cursor.insert(p, c);
        }
        if ctrl_click {
            self.nav.pending = Some(navigate::NavCmd::Definition);
        }
        // The right-click menu.
        if let Some((resp, at, sel)) = right {
            if let Some(ch) = at {
                self.context_click(&ctx, id, ch, sel);
            }
            let (word, has_sel) = match (self.tabs.get(self.active), egui::TextEdit::load_state(&ctx, id).and_then(|s| s.cursor.char_range())) {
                (Some(t), Some(r)) => {
                    let (a, b) = (r.primary.index.0.min(r.secondary.index.0), r.primary.index.0.max(r.secondary.index.0));
                    let chars: Vec<char> = t.text.chars().collect();
                    let word = if a != b {
                        let s: String = chars[a.min(chars.len())..b.min(chars.len())].iter().collect();
                        (!s.contains('\n') && !s.trim().is_empty()).then(|| s.trim().to_string())
                    } else {
                        crate::ops::word_at(&chars, a).map(|(x, y)| chars[x..y].iter().collect())
                    };
                    (word, a != b || t.sels.len() >= 2)
                }
                _ => (None, false),
            };
            self.context_menu(&resp, id, word, has_sel);
        }
        if !quick_open {
            let caret_now = if ctx.memory(|m| m.has_focus(id)) { caret } else { None };
            self.hover_tick(&ctx, probe, caret_now, typed.is_some());
            self.hover_box(&ctx, &font);
        }
        if !quick_open && ctx.memory(|m| m.has_focus(id)) {
            self.complete_after(caret, typed, ctrl_space);
            self.complete_popup(&ctx, id, &font, &colors);
        }
    }

    fn status_line(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut open_quick: Option<quick::QuickKind> = None;
        let mut open_branches = false;
        let v = ui.visuals().clone();
        let weak = v.weak_text_color();
        let ws_icon = self.icon(&ctx, &["folder-symbolic"], 14.0);
        let bar = ui.max_rect();
        let cmdline_open = self.vim_center_busy();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if let Some(w) = self.workspace_name() {
                if let Some(t) = &ws_icon {
                    ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(14.0)).tint(weak));
                }
                ui.label(egui::RichText::new(w).color(weak)).on_hover_text(self.root.as_ref().map(|r| r.display().to_string()).unwrap_or_default());
            }
            // The branch: click to switch.
            if let Some(st) = &self.git.status {
                let b = &st.branch;
                let mut words = b.label();
                if b.ahead > 0 || b.behind > 0 {
                    words.push_str(&format!("  {}{}", if b.behind > 0 { format!("↓{} ", b.behind) } else { String::new() }, if b.ahead > 0 { format!("↑{}", b.ahead) } else { String::new() }));
                }
                if let Some(op) = &self.git.running {
                    words = format!("{words}  ·  {}", op.doing());
                }
                ui.add_space(4.0);
                let g = ui.painter().layout_no_wrap(words, egui::TextStyle::Body.resolve(ui.style()), weak);
                let (r, resp) = ui.allocate_exact_size(g.size() + Vec2::new(30.0, 4.0), Sense::click());
                let col = if resp.hovered() {
                    ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                    v.text_color()
                } else {
                    weak
                };
                super::scm::glyph(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 11.0, r.center().y), Vec2::splat(12.0)), super::scm::Glyph::Branch, col);
                ui.painter().galley(Pos2::new(r.left() + 22.0, r.center().y - g.size().y / 2.0), g, col);
                if resp.on_hover_text("Current Git branch. Click to switch.").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    open_branches = true;
                }
            }
            if let Some((m, bad)) = self.note.as_ref().filter(|_| !cmdline_open) {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(m).color(if *bad { status(&ctx).danger } else { weak }));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let diags = self.diag_ranges();
                // Vim: the mode, and keys typed so far.
                self.vim_mode_tag(ui);
                if let Some(t) = self.tabs.get(self.active) {
                    // The language server, and the problems it found.
                    if let Some((words, tip, trouble)) = &self.lsp_status {
                        let col = if *trouble { weak.gamma_multiply(0.7) } else { weak };
                        ui.label(egui::RichText::new(words).color(col)).on_hover_text(tip);
                    }
                    if let Some((sum, errors)) = complete::diag_summary(&diags) {
                        let st = status(&ctx);
                        ui.label(egui::RichText::new(sum).color(if errors { st.danger } else { st.warning })).on_hover_text("Hover a wavy underline to read the problem");
                    }
                    // The language and the indentation: click either to change it.
                    let lang = crate::syntax::shown_name(&t.name(), &t.text, t.lang);
                    let tip = if t.lang == crate::syntax::Pick::Auto { "Detected from the file. Click to change." } else { "Set by you. Click to change." };
                    if status_button(ui, lang).on_hover_text(tip).clicked() {
                        open_quick = Some(quick::QuickKind::Language);
                    }
                    if status_button(ui, &t.indent.label()).on_hover_text("Indentation for this file. Click to change.").clicked() {
                        open_quick = Some(quick::QuickKind::Indent);
                    }
                    ui.label(egui::RichText::new(format!("Ln {}, Col {}", t.line_col.0, t.line_col.1)).color(weak));
                    if t.sels.len() >= 2 {
                        ui.label(egui::RichText::new(format!("{} cursors", t.sels.len())).color(weak)).on_hover_text("Esc returns to one cursor");
                    }
                }
            });
        });
        // Vim's : line, in the middle.
        self.vim_cmdline(ui, bar);
        if let Some(k) = open_quick {
            self.open_quick(k);
        }
        if open_branches {
            self.open_branches();
        }
    }

    // ---------------------------------------------------------------- welcome

    fn welcome(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let weak = v.weak_text_color();
        let icon = self.app_icon(&ctx);
        let named = |app: &mut Self, names: &[&str]| app.icon(&ctx, names, 16.0);
        // What to start with: (icon, label, keys, action).
        #[derive(Clone, Copy)]
        enum Go {
            Files,
            Folder,
            File,
            New,
            Commands,
            Prefs,
        }
        let mut start: Vec<(Option<egui::TextureHandle>, &str, &str, Go)> = Vec::new();
        if self.root.is_some() {
            start.push((named(self, &["edit-find-symbolic"]), "Go to a file", "Ctrl+P", Go::Files));
        }
        start.push((named(self, &["folder-symbolic"]), if self.root.is_some() { "Open another folder" } else { "Open a folder" }, "Ctrl+O", Go::Folder));
        start.push((named(self, &["document-edit-symbolic"]), "Open a file", "", Go::File));
        start.push((named(self, &["document-new-symbolic"]), "New file", "Ctrl+N", Go::New));
        start.push((named(self, &["utilities-terminal-symbolic"]), "Run a command", "Shift+Ctrl+P", Go::Commands));
        start.push((named(self, &["preferences-system-symbolic"]), "Preferences", "Ctrl+,", Go::Prefs));
        let home = rivet_xdg::places::home();
        let recent: Vec<(PathBuf, String, String, bool)> = self
            .s
            .recent
            .iter()
            .filter(|p| Some(*p) != self.root.as_ref() && p.exists())
            .take(6)
            .map(|p| {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                // Where it is: within the open folder, from home (~), or the whole path.
                let dir = p.parent().map(|d| match (self.root.as_ref().and_then(|r| d.strip_prefix(r).ok()), d.strip_prefix(&home)) {
                    (Some(rest), _) if rest.as_os_str().is_empty() => self.workspace_name().unwrap_or_default(),
                    (Some(rest), _) => rest.display().to_string(),
                    (None, Ok(rest)) if rest.as_os_str().is_empty() => "~".to_string(),
                    (None, Ok(rest)) => format!("~/{}", rest.display()),
                    (None, Err(_)) => d.display().to_string(),
                });
                (p.clone(), name, dir.unwrap_or_default(), p.is_dir())
            })
            .collect();
        let folder_icon = named(self, &["folder-symbolic"]);
        let recent_icons: Vec<Option<egui::TextureHandle>> = recent.iter().map(|(_, name, _, dir)| if *dir { folder_icon.clone() } else { self.file_icon(&ctx, name, 16.0) }).collect();
        let subtitle = match self.workspace_name() {
            Some(ws) => format!("{ws} · {} file{}", self.files.len(), if self.files.len() == 1 { "" } else { "s" }),
            None => "A lightweight code editor".to_string(),
        };

        let r = ui.max_rect();
        let two = r.width() >= 640.0 && !recent.is_empty();
        let col_w: f32 = 280.0;
        let width = if two { col_w * 2.0 + 40.0 } else { col_w.min(r.width() - 32.0) };
        let rows = start.len().max(if two { recent.len() } else { 0 }) + if two { 0 } else { recent.len() + 2 };
        let height = 120.0 + rows as f32 * 34.0 + 40.0;
        let top = (r.center().y - height / 2.0).max(r.top() + 24.0);
        let area = Rect::from_min_size(Pos2::new(r.center().x - width / 2.0, top), Vec2::new(width, r.bottom() - top));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
        let ui = &mut child;

        // The app: its icon, name, and the folder open.
        ui.horizontal(|ui| {
            if let Some(t) = &icon {
                ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(52.0)));
            }
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(egui::RichText::new("Rivet Edit").size(22.0).strong().color(v.strong_text_color()));
                ui.label(egui::RichText::new(subtitle).color(weak));
            });
        });
        ui.add_space(26.0);

        let heading = |ui: &mut egui::Ui, t: &str| {
            ui.label(egui::RichText::new(t).font(fonts::ui_font(11.5)).strong().color(weak));
            ui.add_space(2.0);
        };
        // One row: an icon, the label, and a weak note or keys on the right.
        let row = |ui: &mut egui::Ui, tex: &Option<egui::TextureHandle>, tint: bool, label: &str, note: &str, keys: &str| -> bool {
            let (rr, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rr, r_md(), v.widgets.hovered.weak_bg_fill);
            }
            if let Some(t) = tex {
                paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(rr.left() + 18.0, rr.center().y), Vec2::splat(16.0)), if tint { v.text_color() } else { Color32::WHITE });
            }
            let g = ui.painter().layout_no_wrap(label.to_string(), fonts::ui_font(13.5), v.text_color());
            let gw = g.size().x;
            ui.painter().galley(Pos2::new(rr.left() + 38.0, rr.center().y - g.size().y / 2.0), g, v.text_color());
            let mut right = rr.right() - 10.0;
            if !keys.is_empty() {
                let kg = ui.painter().layout_no_wrap(keys.to_string(), fonts::ui_font(11.5), weak);
                let kr = Rect::from_min_size(Pos2::new(right - kg.size().x - 14.0, rr.center().y - 10.0), Vec2::new(kg.size().x + 14.0, 20.0));
                ui.painter().rect_filled(kr, rivet_look::theme::r_xs(), palette::mix(v.extreme_bg_color, v.text_color(), 0.07));
                ui.painter().galley(Pos2::new(kr.left() + 7.0, kr.center().y - kg.size().y / 2.0), kg, weak);
                right = kr.left() - 8.0;
            }
            if !note.is_empty() {
                let x = rr.left() + 46.0 + gw;
                let mut job = egui::text::LayoutJob::single_section(note.to_string(), egui::TextFormat { font_id: fonts::ui_font(12.0), color: weak, ..Default::default() });
                job.wrap = egui::text::TextWrapping::truncate_at_width((right - x).max(0.0));
                let ng = ui.painter().layout_job(job);
                ui.painter().galley(Pos2::new(x, rr.center().y - ng.size().y / 2.0), ng, weak);
            }
            resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
        };
        let mut go: Option<Go> = None;
        let mut open: Option<(PathBuf, bool)> = None;
        let mut start_col = |ui: &mut egui::Ui| {
            heading(ui, "START");
            for (tex, label, keys, g) in &start {
                if row(ui, tex, true, label, "", keys) {
                    go = Some(*g);
                }
            }
        };
        let mut recent_col = |ui: &mut egui::Ui| {
            heading(ui, "RECENT");
            for (k, (p, name, dir, is_dir)) in recent.iter().enumerate() {
                if row(ui, &recent_icons[k], *is_dir, name, dir, "") {
                    open = Some((p.clone(), *is_dir));
                }
            }
        };
        if two {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 40.0;
                ui.allocate_ui_with_layout(Vec2::new(col_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    start_col(ui)
                });
                ui.allocate_ui_with_layout(Vec2::new(col_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    recent_col(ui)
                });
            });
        } else {
            ui.spacing_mut().item_spacing.y = 0.0;
            start_col(ui);
            if !recent.is_empty() {
                ui.add_space(22.0);
                recent_col(ui);
            }
        }
        match go {
            Some(Go::Files) => self.open_quick(quick::QuickKind::Files),
            Some(Go::Folder) => self.open_quick(quick::QuickKind::Browse { files: false, window: false }),
            Some(Go::File) => self.open_quick(quick::QuickKind::Browse { files: true, window: false }),
            Some(Go::New) => self.new_tab(),
            Some(Go::Commands) => self.open_quick(quick::QuickKind::Commands),
            Some(Go::Prefs) => self.prefs_open = true,
            None => {}
        }
        match open {
            Some((p, true)) => self.open_folder(p),
            Some((p, false)) => self.open_file(&p),
            None => {}
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
        // Return is the highlighted button when no field or button has the keyboard.
        let ret = rivet_look::widgets::return_default(ctx);
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
                hit.or(ret.then_some(0))
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
                    title(ui, &format!("Save changes to {name}?"), "Your changes will be lost if you don't save them.");
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
                    title(ui, "Save As", "Type where to save it. Missing folders are created.");
                    let enter = field(ui, &mut path, "~/file.txt");
                    match buttons(ui, &["Save", "Cancel"]) {
                        Some(0) => act = Some(save_as(tab, path.clone())),
                        Some(_) => {}
                        None if esc => {}
                        None if enter => act = Some(save_as(tab, path.clone())),
                        None => next = Some(Prompt::SaveAs { tab, path }),
                    }
                }
                Prompt::ReplaceInFiles { matches, files } => {
                    title(ui, &format!("Replace {matches} match{} in {files} file{}?", if matches == 1 { "" } else { "es" }, if files == 1 { "" } else { "s" }), "Open files change in their tabs, unsaved. Files that aren't open are saved right away.");
                    match buttons(ui, &["Replace", "Cancel"]) {
                        Some(0) => act = Some(Box::new(|a: &mut EditApp| a.replace_in_files())),
                        Some(_) => {}
                        None if esc => {}
                        None => next = Some(Prompt::ReplaceInFiles { matches, files }),
                    }
                }
                Prompt::NewFile { folder, mut name } => {
                    let where_ = if folder.is_empty() { ws.clone() } else { format!("{ws}/{folder}") };
                    title(ui, "New File", &format!("In {where_}. Type a path like src/new.rs to create folders too."));
                    let enter = field(ui, &mut name, "name.rs");
                    match buttons(ui, &["Create", "Cancel"]) {
                        Some(0) => act = Some(new_file(folder.clone(), name.clone())),
                        Some(_) => {}
                        None if esc => {}
                        None if enter => act = Some(new_file(folder.clone(), name.clone())),
                        None => next = Some(Prompt::NewFile { folder, name }),
                    }
                }
                Prompt::NewFolder { folder, mut name } => {
                    let where_ = if folder.is_empty() { ws.clone() } else { format!("{ws}/{folder}") };
                    title(ui, "New Folder", &format!("In {where_}. Type a/b to create both."));
                    let enter = field(ui, &mut name, "folder name");
                    let make = |folder: String, name: String| -> Act { Box::new(move |a: &mut EditApp| a.make_folder(&folder, &name)) };
                    match buttons(ui, &["Create", "Cancel"]) {
                        Some(0) => act = Some(make(folder.clone(), name.clone())),
                        Some(_) => {}
                        None if esc => {}
                        None if enter => act = Some(make(folder.clone(), name.clone())),
                        None => next = Some(Prompt::NewFolder { folder, name }),
                    }
                }
                Prompt::Rename { rel, mut name } => {
                    title(ui, "Rename", &format!("A new name for {rel}. Tabs open on it keep working."));
                    let enter = field(ui, &mut name, "new name");
                    let go = |rel: String, name: String| -> Act { Box::new(move |a: &mut EditApp| a.rename_path(&rel, &name)) };
                    match buttons(ui, &["Rename", "Cancel"]) {
                        Some(0) => act = Some(go(rel.clone(), name.clone())),
                        Some(_) => {}
                        None if esc => {}
                        None if enter => act = Some(go(rel.clone(), name.clone())),
                        None => next = Some(Prompt::Rename { rel, name }),
                    }
                }
                Prompt::Discard { tracked, untracked } => {
                    let all: Vec<&String> = tracked.iter().chain(&untracked).collect();
                    let name = |p: &String| p.rsplit('/').next().unwrap_or(p).to_string();
                    let (head, sub, yes) = match (tracked.len(), untracked.len()) {
                        (1, 0) => (format!("Discard the changes to {}?", name(&tracked[0])), "It goes back to its staged version. This can't be undone.".to_string(), "Discard Changes"),
                        (0, 1) => (format!("Delete {}?", name(&untracked[0])), "Git doesn't track it yet, so it can't be recovered.".to_string(), "Delete File"),
                        (t, 0) => (format!("Discard the changes to {t} files?"), "They go back to their staged versions. This can't be undone.".to_string(), "Discard Changes"),
                        (t, u) => (format!("Discard the changes to {} files?", all.len()), format!("{t} go back to their staged versions and {u} new file{} deleted. This can't be undone.", if u == 1 { " is" } else { "s are" }), "Discard All"),
                    };
                    title(ui, &head, &sub);
                    match buttons(ui, &[yes, "Cancel"]) {
                        Some(0) => act = Some(Box::new(move |a: &mut EditApp| {
                            a.git.run(crate::vcs::Op::Discard { tracked, untracked });
                        })),
                        Some(_) => {}
                        None if esc => {}
                        None => next = Some(Prompt::Discard { tracked, untracked }),
                    }
                }
                Prompt::Trash { rel, dir } => {
                    let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
                    title(ui, &format!("Move {name} to the Trash?"), if dir { "The folder and everything in it. You can restore it from the Trash." } else { "You can restore it from the Trash. Its tab, if open, closes." });
                    match buttons(ui, &["Move to Trash", "Cancel"]) {
                        Some(0) => {
                            let r = rel.clone();
                            act = Some(Box::new(move |a: &mut EditApp| a.trash_path(&r)));
                        }
                        Some(_) => {}
                        None if esc => {}
                        None => next = Some(Prompt::Trash { rel, dir }),
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

/// Git's marks for the tree: changed files (by path in the workspace), and folders with any.
struct Marks<'a> {
    files: &'a HashMap<String, rivet_git::Change>,
    dirs: &'a HashSet<String>,
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
    folder_icons: &HashMap<String, Option<egui::TextureHandle>>,
    icons: &HashMap<String, Option<egui::TextureHandle>>,
    open: Option<&str>,
    marks: &Marks,
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
        if let Some(Some(t)) = folder_icons.get(sub) {
            paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(x + 20.0, rect.center().y), Vec2::splat(16.0)), Color32::WHITE);
        }
        // A folder holding changes: a dot at the right, its name tinted.
        let changed = marks.dirs.contains(sub);
        let fg = if changed { palette::mix(v.text_color(), status(ui.ctx()).warning, 0.45) } else { v.text_color() };
        ui.painter().with_clip_rect(rect.with_max_x(rect.right() - 16.0)).text(Pos2::new(x + 32.0, rect.center().y), Align2::LEFT_CENTER, name, fonts::ui_font(13.0), fg);
        if changed {
            ui.painter().circle_filled(Pos2::new(rect.right() - 10.0, rect.center().y), 2.5, status(ui.ctx()).warning.gamma_multiply(0.8));
        }
        if resp.clicked() {
            *pick = Some(Pick::Fold(sub.clone()));
        }
        resp.context_menu(|ui| {
            let mut act = None;
            treemenu::tree_menu(ui, sub, true, &mut act);
            if let Some(a) = act {
                *pick = Some(Pick::Menu(a));
            }
        });
        if is_open {
            let top = rect.bottom();
            let guide = ui.painter().add(egui::Shape::Noop);
            tree(ui, sub, depth + 1, folders, files, open_dirs, folder_icons, icons, open, marks, pick);
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
        if let Some(Some(t)) = icons.get(name) {
            paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(x + 8.0, rect.center().y), Vec2::splat(16.0)), Color32::WHITE);
        }
        // Git: changed files tinted, with their letter at the right (as VS Code shows them).
        let mark = marks.files.get(f.as_str()).copied();
        let base = if on { v.strong_text_color() } else { v.text_color() };
        let fg = match mark {
            Some(c) => palette::mix(base, super::scm::change_color(ui.ctx(), c), 0.6),
            None => base,
        };
        let right = if mark.is_some() { rect.right() - 22.0 } else { rect.right() };
        ui.painter().with_clip_rect(rect.with_max_x(right)).text(Pos2::new(x + 22.0, rect.center().y), Align2::LEFT_CENTER, name, fonts::ui_font(13.0), fg);
        if let Some(c) = mark {
            ui.painter().text(Pos2::new(rect.right() - 11.0, rect.center().y), Align2::CENTER_CENTER, c.letter(), fonts::ui_font(11.5), super::scm::change_color(ui.ctx(), c));
        }
        let tip = match mark {
            Some(c) => format!("{f} · {}\nClick to preview · double-click or Ctrl+click to keep open", c.words()),
            None => format!("{f}\nClick to preview · double-click or Ctrl+click to keep open"),
        };
        let resp = resp.on_hover_text(tip);
        resp.context_menu(|ui| {
            let mut act = None;
            treemenu::tree_menu(ui, f, false, &mut act);
            if let Some(a) = act {
                *pick = Some(Pick::Menu(a));
            }
        });
        if resp.double_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.command)) {
            *pick = Some(Pick::Keep(f.clone()));
        } else if resp.clicked() {
            *pick = Some(Pick::File(f.clone()));
        }
    }
}

/// A quiet status bar word that can be clicked (a light box under the pointer).
fn status_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let v = ui.visuals().clone();
    let g = ui.painter().layout_no_wrap(text.to_string(), egui::TextStyle::Body.resolve(ui.style()), v.weak_text_color());
    let (r, resp) = ui.allocate_exact_size(g.size() + Vec2::new(12.0, 4.0), Sense::click());
    let fg = if resp.hovered() {
        ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
        v.text_color()
    } else {
        v.weak_text_color()
    };
    ui.painter().galley(r.center() - g.size() / 2.0, g, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

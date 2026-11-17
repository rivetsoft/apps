//! Drawing: the toolbar, the sidebar (folders or tags), the note (properties and text), the
//! welcome screen, the small questions and Preferences.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{paint_tex, r_lg, r_md, r_sm, selected_tile, status};
use rivet_look::{builtin_icons, fonts, highlight, palette};

use super::*;

const ROW_H: f32 = 28.0;

/// What a sidebar row (or its right-click menu) asks for.
enum Pick {
    Note(PathBuf),
    Fold(String),
    Folder(String),
    NewNote(String),
    NewFolder(String),
    Rename(PathBuf),
    Duplicate(PathBuf),
    Trash(PathBuf),
    CopyPath(PathBuf),
    ShowInFiles(PathBuf),
    /// Show only notes with this tag (in the search field).
    OnlyTag(String),
    /// Fold (true) or open everything in a panel.
    FoldAll(bool, Browse),
    Reload,
    /// Move a note or folder into a folder (vault-relative; empty: the top of the vault).
    MoveTo(PathBuf, String),
    /// A whole layout (from a menu).
    Preset(crate::layout::Layout),
    /// Show this tab.
    ShowTab(Browse),
    /// A panel dropped on a slot.
    MovePanel(Browse, usize, crate::layout::Zone),
    /// Fold or open a pane (its key).
    FoldSlot(String),
}

/// The panel being dragged by its title.
#[derive(Clone, Copy)]
struct PanelDrag(Browse);

/// Comfortable menu rows, like Rivet's right-click menus.
fn menu_style(ui: &mut egui::Ui) {
    ui.set_min_width(220.0);
    ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
    ui.spacing_mut().item_spacing.y = 3.0;
}

fn item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    let b = egui::Button::new(label);
    let b = if shortcut.is_empty() { b } else { b.shortcut_text(shortcut) };
    let r = ui.add(b).clicked();
    if r {
        ui.close();
    }
    r
}

/// The vault-relative folder a note or folder is in (for "here").
fn rel_folder(root: &Path, p: &Path, is_dir: bool) -> String {
    let dir = if is_dir { p } else { p.parent().unwrap_or(root) };
    dir.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default()
}

/// Right-click on a note.
fn note_menu(ui: &mut egui::Ui, root: &Path, n: &Note, pick: &mut Option<Pick>) {
    menu_style(ui);
    if item(ui, "Open", "") {
        *pick = Some(Pick::Note(n.path.clone()));
    }
    ui.separator();
    let here = rel_folder(root, &n.path, false);
    if item(ui, "New Note…", "Ctrl+N") {
        *pick = Some(Pick::NewNote(here.clone()));
    }
    if item(ui, "Create New Folder…", "Shift+Ctrl+N") {
        *pick = Some(Pick::NewFolder(here));
    }
    ui.separator();
    if item(ui, "Rename…", "F2") {
        *pick = Some(Pick::Rename(n.path.clone()));
    }
    if item(ui, "Duplicate", "") {
        *pick = Some(Pick::Duplicate(n.path.clone()));
    }
    if item(ui, "Move to Trash…", "Delete") {
        *pick = Some(Pick::Trash(n.path.clone()));
    }
    ui.separator();
    if item(ui, "Copy Location", "") {
        *pick = Some(Pick::CopyPath(n.path.clone()));
    }
    if item(ui, "Show in Folder", "") {
        *pick = Some(Pick::ShowInFiles(n.path.clone()));
    }
}

/// Right-click on a folder.
fn folder_menu(ui: &mut egui::Ui, root: &Path, rel: &str, open: bool, pick: &mut Option<Pick>) {
    menu_style(ui);
    let path = root.join(rel);
    if item(ui, if open { "Collapse" } else { "Expand" }, "") {
        *pick = Some(Pick::Fold(rel.to_string()));
    }
    ui.separator();
    if item(ui, "New Note…", "Ctrl+N") {
        *pick = Some(Pick::NewNote(rel.to_string()));
    }
    if item(ui, "Create New Folder…", "Shift+Ctrl+N") {
        *pick = Some(Pick::NewFolder(rel.to_string()));
    }
    ui.separator();
    if item(ui, "Rename…", "") {
        *pick = Some(Pick::Rename(path.clone()));
    }
    if item(ui, "Move to Trash…", "") {
        *pick = Some(Pick::Trash(path.clone()));
    }
    ui.separator();
    if item(ui, "Copy Location", "") {
        *pick = Some(Pick::CopyPath(path.clone()));
    }
    if item(ui, "Open in File Manager", "") {
        *pick = Some(Pick::ShowInFiles(path));
    }
}

impl NotesApp {
    pub(super) fn draw(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let v = ctx.global_style().visuals.clone();
        if self.s.show_menubar {
            let bar = egui::Frame::NONE.fill(v.panel_fill).inner_margin(egui::Margin::symmetric(6, 2));
            egui::Panel::top("menubar").show_separator_line(false).frame(bar).show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| self.menus(ui));
            });
        }
        let chrome = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 12, right: 12, top: 6, bottom: 8 });
        egui::Panel::top("toolbar").show_separator_line(false).frame(chrome).show(ui, |ui| self.toolbar(ui));
        let st = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 14, right: 14, top: 4, bottom: 6 });
        if self.s.show_statusbar {
            egui::Panel::bottom("status").show_separator_line(false).frame(st).show(ui, |ui| self.status_line(ui));
        }
        if self.s.show_sidebar && self.vault.is_some() {
            let side = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 10, right: 8, top: 0, bottom: 4 });
            egui::Panel::left("notes-side").show_separator_line(false).frame(side).resizable(true).default_size(260.0).size_range(190.0..=460.0).show(ui, |ui| self.sidebar(ui));
        }
        let peek = self.s.show_peek && self.vault.is_some();
        if peek {
            let side = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 2, right: 12, top: 0, bottom: 4 });
            egui::Panel::right("notes-peek").show_separator_line(false).frame(side).resizable(true).default_size(270.0).size_range(220.0..=420.0).show(ui, |ui| self.peek(ui));
        }
        let left = if self.s.show_sidebar && self.vault.is_some() { 0 } else { 12 };
        let right = if peek { 6 } else { 12 };
        let bottom = if self.s.show_statusbar { 0 } else { 12 };
        let main = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left, right, top: 0, bottom });
        egui::CentralPanel::default().frame(main).show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().rect_filled(r, r_lg(), v.extreme_bg_color);
            // The note's page; centered text keeps to a column in the middle of it.
            // The breadcrumbs sit near the page's top edge.
            let top = if self.s.show_breadcrumbs && self.doc.is_some() { 10.0 } else { 18.0 };
            let page = Rect::from_min_max(r.min + Vec2::new(28.0, top), r.max - Vec2::new(28.0, 18.0));
            self.page_rect = page;
            // The Todos page has its own width; a note its own or the default.
            let todos_shown = self.todo_page && self.s.super_todos;
            let centered = if todos_shown { self.s.todo_centered } else { self.doc.as_ref().is_some_and(|d| self.s.centered_for(&d.path)) };
            let col = if centered {
                let w = page.width().min(super::peek::CENTERED_WIDTH * fonts::scale().max(1.0));
                Rect::from_min_max(Pos2::new(page.center().x - w / 2.0, page.top()), Pos2::new(page.center().x + w / 2.0, page.bottom()))
            } else {
                page
            };
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(col).layout(egui::Layout::top_down(egui::Align::Min)));
            if self.vault.is_none() {
                self.welcome(&mut child);
            } else if self.todo_page && self.s.super_todos {
                self.todos_page(&mut child);
            } else if self.doc.is_some() {
                self.editor(&mut child);
            } else {
                let c = r.center();
                let weak = v.weak_text_color();
                child.painter().text(c - Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, "Pick a note on the left", fonts::ui_font(15.0), v.text_color());
                child.painter().text(c + Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, "or make a new one with Ctrl+N", fonts::ui_font(12.5), weak);
            }
        });
        // Without the status bar, a problem shows over the bottom of the window (click to hide).
        if !self.s.show_statusbar
            && let Some((m, true)) = self.note_msg.clone()
        {
            let r = egui::Area::new(egui::Id::unique("notes-problem")).order(egui::Order::Foreground).anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -18.0)).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
                    ui.label(egui::RichText::new(m).color(status(ctx).danger));
                });
            });
            if r.response.interact(Sense::click()).on_hover_text("Click to hide").clicked() {
                self.note_msg = None;
            }
        }
        self.reading_view(ctx);
        self.quick_panel(ctx);
        self.prompts(ctx);
        if self.prefs_open {
            self.prefs_window(ctx);
        }
    }

    // ---------------------------------------------------------------- toolbar

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let full = ui.max_rect();
        let mut left_end = full.left();
        let mut right_start = full.right();
        let row = ui
            .horizontal(|ui| {
                ui.set_min_height(32.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                if !self.s.show_menubar {
                    let fg = v.text_color();
                    let btn = egui::Button::new("").min_size(Vec2::new(34.0, 32.0)).frame_when_inactive(false).corner_radius(r_sm());
                    let r = egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui| {
                        ui.set_min_width(220.0);
                        if ui.add(egui::Button::new("Show Menubar").shortcut_text("Ctrl+M")).clicked() {
                            self.s.show_menubar = true;
                            self.s.save();
                            ui.close();
                        }
                        ui.separator();
                        self.menus(ui);
                    });
                    let c = r.0.rect.center();
                    for dy in [-4.5f32, 0.0, 4.5] {
                        ui.painter().line_segment([c + Vec2::new(-6.5, dy), c + Vec2::new(6.5, dy)], Stroke::new(1.8, fg));
                    }
                }
                // The left sidebar, right after the menu (Peek's button mirrors it at the far right).
                let side_icon = self.icon(&ctx, &["sidebar-show-symbolic", "view-left-pane-symbolic"], 16.0);
                if toggle_button(ui, &side_icon, "▯", "Sidebar (F9)", self.vault.is_some()).clicked() {
                    self.s.show_sidebar = !self.s.show_sidebar;
                    self.s.save();
                }
                // New note: a plus, after the sidebar button.
                let add_icon = self.icon(&ctx, &["list-add-symbolic", "tab-new-symbolic"], 16.0);
                let b = match &add_icon {
                    Some(t) => egui::Button::image(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(16.0)).tint(v.text_color())),
                    None => egui::Button::new(egui::RichText::new("+").size(18.0)),
                };
                let b = b.frame_when_inactive(false).min_size(Vec2::new(34.0, 32.0)).corner_radius(r_sm());
                let r = if self.s.daily_notes && self.vault.is_some() {
                    // With daily notes on, the plus offers both.
                    let (r, _) = egui::containers::menu::MenuButton::from_button(b).ui(ui, |ui| {
                        menu_style(ui);
                        ui.set_min_width(210.0);
                        if item(ui, "New Note", "Ctrl+N") {
                            self.prompt = Some(Prompt::NewNote { folder: self.current_folder.clone(), title: String::new() });
                        }
                        if item(ui, "Daily Note", "Ctrl+D") {
                            self.daily_note();
                        }
                    });
                    r.on_hover_text("New note (Ctrl+N) or daily note (Ctrl+D)")
                } else {
                    let r = ui.add_enabled(self.vault.is_some(), b).on_hover_text("New note (Ctrl+N)");
                    if r.clicked() {
                        self.prompt = Some(Prompt::NewNote { folder: self.current_folder.clone(), title: String::new() });
                    }
                    r
                };
                left_end = r.rect.right();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Peek, at the far right.
                    let peek_icon = self.icon(&ctx, &["sidebar-show-right-symbolic", "view-right-pane-symbolic", "dialog-information-symbolic"], 16.0);
                    if toggle_button(ui, &peek_icon, "i", "Peek: this note's details and display (F11)", self.vault.is_some()).clicked() {
                        self.s.show_peek = !self.s.show_peek;
                        self.s.save();
                    }
                    // The Todos page (with super todos on).
                    if self.s.super_todos {
                        let todo_icon = self.icon(&ctx, &["checkbox-checked-symbolic", "object-select-symbolic", "emblem-ok-symbolic"], 16.0);
                        let b = match &todo_icon {
                            Some(t) => egui::Button::image(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(16.0)).tint(v.text_color())),
                            None => egui::Button::new("Todos"),
                        };
                        let r = ui.add_enabled(self.vault.is_some(), b.frame_when_inactive(false).min_size(Vec2::new(34.0, 32.0)).corner_radius(r_sm()));
                        if self.todo_page && !r.hovered() {
                            ui.painter().rect_stroke(r.rect, r_sm(), v.widgets.hovered.bg_stroke, egui::StrokeKind::Inside);
                        }
                        if r.on_hover_text("Todos: every super todo in the vault").clicked() {
                            self.save_now();
                            self.todo_page = !self.todo_page;
                        }
                    }
                    // The rendered page.
                    let read_icon = self.icon(&ctx, &["view-reveal-symbolic", "document-open-symbolic", "x-office-document-symbolic"], 16.0);
                    let b = match &read_icon {
                        Some(t) => egui::Button::image(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(16.0)).tint(v.text_color())),
                        None => egui::Button::new("Read"),
                    };
                    if ui.add_enabled(self.doc.is_some(), b.frame_when_inactive(false).min_size(Vec2::new(34.0, 32.0)).corner_radius(r_sm())).on_hover_text("Reading view (Ctrl+Space)").clicked() {
                        self.toggle_reading(&ctx);
                    }
                    // The editing mode, beside it: a bolt in live mode, brackets in source mode.
                    let (names, fallback, tip): (&[&str], &str, &str) = if self.s.live_mode {
                        (&["rivet-live-symbolic", "weather-storm-symbolic"], "Live", "Live mode: rendered except the line you're on. Click for source mode")
                    } else {
                        (&["rivet-source-symbolic", "text-x-script-symbolic", "code-context-symbolic"], "</>", "Source mode: Markdown as written. Click for live mode")
                    };
                    let mode_icon = self.icon(&ctx, names, 16.0);
                    let b = match &mode_icon {
                        Some(t) => egui::Button::image(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(16.0)).tint(v.text_color())),
                        None => egui::Button::new(fallback),
                    };
                    let r = ui.add(b.frame_when_inactive(false).min_size(Vec2::new(34.0, 32.0)).corner_radius(r_sm()));
                    // The active mode keeps its outline (the one it has under the pointer).
                    if !r.hovered() {
                        ui.painter().rect_stroke(r.rect, r_sm(), v.widgets.hovered.bg_stroke, egui::StrokeKind::Inside);
                    }
                    if r.on_hover_text(tip).clicked() {
                        self.s.live_mode = !self.s.live_mode;
                        self.s.save();
                    }
                    right_start = ui.min_rect().left();
                });
            })
            .response
            .rect;
        // Search, centred on the window (and kept clear of the buttons either side), on the
        // location bar's raised surface. Icon, hint and text share one centred line.
        let cx = full.center().x;
        let room = ((cx - left_end).min(right_start - cx) - 12.0) * 2.0;
        let w = room.min(560.0);
        if w < 120.0 {
            return;
        }
        let rect = Rect::from_center_size(Pos2::new(cx, row.center().y), Vec2::new(w, 32.0));
        let fill = palette::mix(v.window_fill, v.text_color(), 0.04);
        let shadow = egui::Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(if v.dark_mode { 90 } else { 40 }) };
        let find = self.icon(&ctx, &["edit-find-symbolic", "system-search-symbolic"], 14.0);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
        // A focus ring while it's being typed in.
        if ui.ctx().memory(|m| m.has_focus(egui::Id::unique("notes-search"))) {
            ui.painter().rect_stroke(rect.expand(1.0), r_md(), Stroke::new(1.5, v.selection.bg_fill), egui::StrokeKind::Outside);
        }
        egui::Frame::NONE.fill(fill).shadow(shadow).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(10, 0)).show(&mut child, |ui| {
            ui.set_min_size(Vec2::new(w - 20.0, 32.0));
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if let Some(t) = &find {
                    ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(14.0)).tint(v.weak_text_color()));
                }
                let font = fonts::ui_font(13.5);
                ui.add_enabled(
                    self.vault.is_some(),
                    egui::TextEdit::singleline(&mut self.search)
                        .id(egui::Id::unique("notes-search"))
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .font(font)
                        .align(Align2::LEFT_CENTER)
                        .hint_text("Search notes, or #tag")
                        .desired_width(f32::INFINITY),
                );
            });
        });
    }

    fn status_line(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let vault_icon = self.icon(&ctx, &["vault-symbolic", "folder-symbolic"], 14.0);
        ui.horizontal(|ui| {
            if let Some(path) = self.vault.clone() {
                ui.spacing_mut().item_spacing.x = 6.0;
                if let Some(t) = &vault_icon {
                    ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(14.0)).tint(v.weak_text_color()));
                }
                ui.label(egui::RichText::new(self.vault_name()).color(v.weak_text_color())).on_hover_text(format!("{}\nSwitch vaults from File → Switch Vault", path.display()));
            }
            // Only problems show here (no activity); they stay until the next one.
            if let Some((m, true)) = &self.note_msg {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(m).color(status(&ctx).danger));
            }
            let todos_shown = self.todo_page && self.s.super_todos;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(d) = self.doc.as_ref().filter(|_| !todos_shown) {
                    let words = d.body.split_whitespace().count();
                    ui.label(egui::RichText::new(format!("{words} word{}", if words == 1 { "" } else { "s" })).color(v.weak_text_color()));
                }
            });
        });
    }

    // ---------------------------------------------------------------- sidebar

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let folder_icon = self.icon(&ctx, &["folder", "inode-directory"], 16.0);
        let note_icon = self.icon(&ctx, &["text-markdown", "text-x-markdown", "text-x-generic"], 16.0);
        let tag_icon = self.icon(&ctx, &["tag-symbolic", "bookmark-new-symbolic", "starred-symbolic"], 14.0);
        let open = self.doc.as_ref().map(|d| d.path.clone());
        let mut pick: Option<Pick> = None;
        let root = self.vault.clone().unwrap_or_default();
        let notes: Vec<Note> = self.notes.iter().filter(|n| self.matches(n)).cloned().collect();
        let searching = !self.search.trim().is_empty();
        let collapsed: BTreeSet<String> = self.s.collapsed.iter().cloned().collect();
        let data = SideData {
            notes,
            folders: self.folders.clone(),
            tags: self.tag_counts(),
            collapsed,
            open,
            root: root.clone(),
            current: self.current_folder.clone(),
            folder_icon,
            note_icon,
            tag_icon,
        };
        let layout = self.s.layout.clone();
        ui.add_space(2.0);
        if searching {
            plain_header(ui, &format!("{} found", data.notes.len()));
            egui::ScrollArea::vertical().id_salt("side-search").auto_shrink([false, false]).show(ui, |ui| {
                for n in &data.notes {
                    note_row(ui, n, 0, data.open.as_ref() == Some(&n.path), &data.note_icon, true, &mut pick, &data.root);
                }
            });
        } else {
            // The slots, top to bottom. Each has a title bar (tabs when it holds several panels),
            // then its panel; a line between two slots drags to share the room.
            let n = layout.slots.len();
            let total = ui.available_height();
            let header_h = 30.0;
            let handle_h = 8.0;
            let folded: Vec<bool> = layout.slots.iter().enumerate().map(|(i, s)| i > 0 && data.collapsed.contains(&crate::layout::fold_key(s))).collect();
            let last_h = if n > 1 && !folded[n - 1] { (total * layout.split).clamp(90.0, (total - 150.0).max(90.0)) } else { 0.0 };
            let mut rects: Vec<(Rect, Rect)> = Vec::new();
            for (i, slot) in layout.slots.iter().enumerate() {
                if i > 0 {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), handle_h), Sense::drag());
                    let v = ui.visuals();
                    let col = if resp.hovered() || resp.dragged() { v.selection.bg_fill } else { v.widgets.noninteractive.bg_stroke.color };
                    ui.painter().line_segment([Pos2::new(r.left() + 6.0, r.center().y), Pos2::new(r.right() - 6.0, r.center().y)], Stroke::new(if resp.dragged() { 2.0 } else { 1.0 }, col));
                    if n == 2 && !folded[1] {
                        let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeVertical).on_hover_text("Drag to share the room");
                        if resp.dragged() && total > 0.0 {
                            self.s.layout.split = (self.s.layout.split - resp.drag_delta().y / total).clamp(0.15, 0.85);
                        }
                        if resp.drag_stopped() {
                            self.s.save();
                        }
                    }
                }
                let fold = if i > 0 { Some(!folded[i]) } else { None };
                let hdr = slot_header(ui, i, slot, fold, n > 1, &mut pick);
                if folded[i] {
                    rects.push((hdr, hdr));
                    continue;
                }
                let kind = slot.shown();
                let mut sa = egui::ScrollArea::vertical().id_salt(("side-slot", crate::layout::id(kind))).auto_shrink([false, false]);
                if i + 1 < n {
                    // This slot gets what the last one leaves.
                    let h = (total - header_h * n as f32 - handle_h * (n - 1) as f32 - last_h).max(60.0);
                    sa = sa.max_height(h).min_scrolled_height(h);
                }
                let out = sa.show(ui, |ui| panel(ui, kind, &data, &mut pick));
                rects.push((hdr, out.inner_rect));
            }
            // A panel being dragged by its title: where it would land, and the drop.
            if let Some(drag) = egui::DragAndDrop::payload::<PanelDrag>(&ctx)
                && let Some(pos) = ctx.pointer_hover_pos()
            {
                use crate::layout::Zone;
                let v = ui.visuals().clone();
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("panel-drop")));
                let target = rects.iter().enumerate().find(|(_, (h, b))| h.union(*b).contains(pos)).map(|(i, (h, b))| {
                    let zone = if h.contains(pos) || h == b {
                        Zone::Join
                    } else if pos.y < b.top() + b.height() * 0.3 {
                        Zone::Above
                    } else if pos.y > b.bottom() - b.height() * 0.3 {
                        Zone::Below
                    } else {
                        Zone::Join
                    };
                    (i, *h, *b, zone)
                });
                if let Some((i, h, b, zone)) = target {
                    let mut test = layout.clone();
                    test.move_panel(drag.0, i, zone);
                    if test != layout {
                        let accent = v.selection.bg_fill;
                        match zone {
                            Zone::Join => {
                                let r = h.union(b).shrink(2.0);
                                painter.rect_filled(r, r_md(), accent.gamma_multiply(0.15));
                                painter.rect_stroke(r, r_md(), Stroke::new(2.0, accent), egui::StrokeKind::Inside);
                            }
                            Zone::Above => {
                                painter.line_segment([Pos2::new(h.left(), h.top() + 1.0), Pos2::new(h.right(), h.top() + 1.0)], Stroke::new(3.0, accent));
                            }
                            Zone::Below => {
                                painter.line_segment([Pos2::new(b.left(), b.bottom() - 1.0), Pos2::new(b.right(), b.bottom() - 1.0)], Stroke::new(3.0, accent));
                            }
                        }
                        if ctx.input(|i| i.pointer.any_released()) {
                            pick = Some(Pick::MovePanel(drag.0, i, zone));
                        }
                    }
                }
                // What's being carried, under the pointer.
                let text = match target.map(|t| t.3) {
                    Some(Zone::Join) => format!("{} (as a tab)", crate::layout::title(drag.0)),
                    Some(Zone::Above) => format!("{} (above)", crate::layout::title(drag.0)),
                    Some(Zone::Below) => format!("{} (below)", crate::layout::title(drag.0)),
                    None => crate::layout::title(drag.0).to_string(),
                };
                let g = painter.layout_no_wrap(text, fonts::ui_font(12.5), v.selection.stroke.color);
                let r = Rect::from_min_size(pos + Vec2::new(14.0, 10.0), g.size() + Vec2::new(16.0, 8.0));
                painter.rect_filled(r, r_sm(), v.selection.bg_fill);
                painter.galley(r.min + Vec2::new(8.0, 4.0), g, v.selection.stroke.color);
                if ctx.input(|i| i.pointer.any_released()) {
                    egui::DragAndDrop::clear_payload(&ctx);
                }
            }
        }
        drag_label(&ctx);
        match pick {
            Some(Pick::Note(p)) => self.open_note(&p),
            Some(Pick::Fold(k)) => {
                if let Some(i) = self.s.collapsed.iter().position(|x| *x == k) {
                    self.s.collapsed.remove(i);
                } else {
                    self.s.collapsed.push(k);
                }
                self.s.save();
            }
            Some(Pick::Folder(f)) => self.current_folder = f,
            Some(Pick::NewNote(f)) => self.prompt = Some(Prompt::NewNote { folder: f, title: String::new() }),
            Some(Pick::NewFolder(f)) => self.prompt = Some(Prompt::NewFolder { parent: f, name: String::new() }),
            Some(Pick::Rename(p)) => {
                let name = if p.is_dir() { p.file_name() } else { p.file_stem() }.map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                self.prompt = Some(Prompt::Rename { path: p, name });
            }
            Some(Pick::Duplicate(p)) => self.duplicate(&p),
            Some(Pick::Trash(p)) => self.prompt = Some(Prompt::Delete(p)),
            Some(Pick::CopyPath(p)) => {
                ctx.copy_text(p.display().to_string());
                self.note_msg = Some(("Location copied".into(), false));
            }
            Some(Pick::ShowInFiles(p)) => {
                let dir = if p.is_dir() { p } else { p.parent().map(Path::to_path_buf).unwrap_or(p) };
                if let Err(e) = rivet_xdg::launch::open_url(&dir.to_string_lossy()) {
                    self.note_msg = Some((format!("Couldn't open the folder: {e}"), true));
                }
            }
            Some(Pick::OnlyTag(t)) => self.search = format!("#{t}"),
            Some(Pick::FoldAll(fold, kind)) => {
                // Only that panel's groups: folders have plain names, tags start with #.
                let is_tag = |k: &String| k.starts_with('#');
                self.s.collapsed.retain(|k| k.starts_with("pane:") || if kind == Browse::Tags { !is_tag(k) } else { is_tag(k) });
                if fold {
                    match kind {
                        Browse::Folders => self.s.collapsed.extend(self.folders.clone()),
                        Browse::Tags => self.s.collapsed.extend(self.tag_counts().into_iter().map(|(t, _)| format!("#{}", t.to_lowercase())).chain(["#".to_string()])),
                    }
                }
                self.s.save();
            }
            Some(Pick::Preset(l)) => {
                let split = self.s.layout.split;
                self.s.layout = crate::layout::Layout { split, ..l };
                self.s.save();
            }
            Some(Pick::ShowTab(p)) => {
                self.s.layout.show(p);
                self.s.save();
            }
            Some(Pick::MovePanel(p, slot, zone)) => {
                self.s.layout.move_panel(p, slot, zone);
                self.s.save();
            }
            Some(Pick::FoldSlot(k)) => {
                if let Some(i) = self.s.collapsed.iter().position(|x| *x == k) {
                    self.s.collapsed.remove(i);
                } else {
                    self.s.collapsed.push(k);
                }
                self.s.save();
            }
            Some(Pick::Reload) => self.rescan(),
            Some(Pick::MoveTo(src, folder)) => self.move_into(&src, &folder),
            None => {}
        }
    }

    // ---------------------------------------------------------------- the note

    fn editor(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let weak = v.weak_text_color();
        let vault = self.vault.clone().unwrap_or_default();
        let (accent_p, accent_s, accent_t) = {
            let a = self.look.accent(&self.choice());
            (a, builtin_icons::derive_secondary(a), builtin_icons::derive_tertiary(a))
        };
        let colors = highlight::colors(v.text_color(), v.extreme_bg_color, accent_p, accent_s, accent_t);
        let show_props = self.s.show_properties;
        let editor_size = self.s.editor_size;
        let line_spacing = self.s.line_spacing;
        let now = ctx.input(|i| i.time);
        let mut edit: Option<EditCmd> = None;
        let mut fmt_pick: Option<crate::format::Fmt> = None;
        let mut sel_rect: Option<(Rect, Rect, Rect)> = None;
        let page = self.page_rect;
        let todos_on = self.s.super_todos;
        let todo_tags = self.todo_tags();
        let live_on = self.s.live_mode;
        let mut title_commit: Option<String> = None;
        let mut follow: Option<String> = None;
        let show_crumbs = self.s.show_breadcrumbs;
        let vault_name = self.vault_name();
        let vault_icon = self.icon(&ctx, &["vault-symbolic", "folder-symbolic"], 13.0);
        let prop_icons = super::props::PropIcons {
            tag: self.icon(&ctx, &["tag-symbolic"], 14.0),
            link: self.icon(&ctx, &["insert-link-symbolic", "emblem-symbolic-link"], 14.0),
        };
        let Some(d) = self.doc.as_mut() else { return };
        // ---- heading: the title, where it is
        let (title, _) = vault::note_title(d.fm.as_ref(), &d.path);
        // ---- breadcrumbs: where the note is (vault › folders › file), just to read
        if show_crumbs {
            let rel = d.path.strip_prefix(&vault).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
            let font = fonts::ui_font(12.5);
            let row = ui.horizontal(|ui| {
                ui.set_min_height(20.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                if let Some(t) = &vault_icon {
                    ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(13.0)).tint(weak));
                }
                ui.label(egui::RichText::new(&vault_name).font(font.clone()).color(weak));
                for (k, p) in parts.iter().enumerate() {
                    ui.label(egui::RichText::new("›").font(font.clone()).color(weak.gamma_multiply(0.7)));
                    let col = if k + 1 == parts.len() { v.text_color() } else { weak };
                    ui.label(egui::RichText::new(*p).font(font.clone()).color(col));
                }
            });
            row.response.on_hover_text(d.path.display().to_string());
            // A hairline under the bar sets it apart from the note.
            let y = ui.cursor().top() + 4.0;
            let r = ui.max_rect();
            ui.painter().line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], Stroke::new(1.0, palette::mix(v.extreme_bg_color, v.text_color(), 0.08)));
            ui.add_space(26.0);
        } else {
            ui.add_space(8.0);
        }
        // The title can be edited in place: Enter (or clicking away) keeps it, Esc puts it back.
        let title_id = egui::Id::unique("note-title");
        let mut buf = d.title_buf.clone().unwrap_or_else(|| title.clone());
        let tr = ui.add(
            egui::TextEdit::singleline(&mut buf)
                .id(title_id)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(egui::FontId::new(25.0 * fonts::scale(), rivet_look::live::editor_bold_family()))
                .text_color(v.strong_text_color())
                .hint_text("Untitled")
                .desired_width(f32::INFINITY),
        );
        let tr = if tr.has_focus() { tr } else { tr.on_hover_text("Click to rename the note") };
        if tr.changed() {
            d.title_buf = Some(buf.clone());
        }
        if tr.lost_focus() {
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                d.title_buf = None;
            } else if let Some(t) = d.title_buf.take() {
                title_commit = Some(t);
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    ctx.memory_mut(|m| m.request_focus(egui::Id::unique("note-body")));
                }
            }
        }
        ui.add_space(18.0);
        let mut changed = false;
        // ---- properties (the frontmatter)
        if show_props && super::props::block(ui, d, &prop_icons) {
            changed = true;
        }
        // ---- the text, colored like Markdown
        // The editor's own font; code (fenced blocks and `inline`) in the monospace one.
        let font = egui::FontId::new(editor_size * fonts::scale(), rivet_look::live::editor_family());
        let mono = egui::FontId::monospace(editor_size * 0.92 * fonts::scale());
        // Live mode: rendered except on the line being edited.
        let live_raw = if live_on { super::live_edit::raw_range(&ctx, egui::Id::unique("note-body"), &d.body) } else { None };
        // The line being edited keeps the look it had when the cursor came to it.
        match live_raw {
            Some((a, _)) => {
                let line = d.body[..a].matches('\n').count();
                if d.live_held.map(|h| h.0) != Some(line) {
                    let start = d.body[..a].rfind('\n').map(|i| i + 1).unwrap_or(0);
                    d.live_held = Some((line, crate::live::heading_level(d.body[start..].lines().next().unwrap_or(""))));
                }
            }
            None => d.live_held = None,
        }
        let live_held = d.live_held;
        // The text as it was before this frame's typing (to keep the edited line steady).
        let live_before = if live_on { d.body.clone() } else { String::new() };
        let live_look = super::live_edit::LiveLook {
            size: editor_size * fonts::scale(),
            line_spacing,
            text: v.text_color(),
            strong: v.strong_text_color(),
            weak,
            link: v.hyperlink_color,
            tag: accent_p,
            accent: accent_p,
            code_bg: palette::mix(v.extreme_bg_color, v.text_color(), 0.06),
            code: colors.string,
            todo_tags: if todos_on { todo_tags.clone() } else { Vec::new() },
        };
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            if live_on {
                let raw = super::live_edit::shift_raw(&live_before, buf.as_str(), live_raw);
                let mut job = super::live_edit::job(buf.as_str(), raw, live_held, &live_look, &colors);
                job.wrap.max_width = wrap;
                return ui.fonts_mut(|f| f.layout_job(job));
            }
            let mut job = highlight::layout(buf.as_str(), "note.md", &colors, font.clone());
            mono_code(&mut job, buf.as_str(), &mono);
            super::live_edit::color_blocks(&mut job, buf.as_str(), &colors, mono.size);
            // Roomier lines than the interface's (Preferences → Editor → Line spacing).
            for sec in &mut job.sections {
                sec.format.line_height = Some(sec.format.font_id.size * line_spacing);
            }
            job.wrap.max_width = wrap;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        // Formatting: keys while the text has the keyboard, or a menu's choice.
        let body_id = egui::Id::unique("note-body");
        // Checks: a selection made by RIVET_NOTES_SELECT keeps the keyboard in the text.
        if self.shot.is_some() && std::env::var_os("RIVET_NOTES_SELECT").is_some() && self.reading.is_none() {
            ctx.memory_mut(|m| m.request_focus(body_id));
        }
        let fmt = {
            use crate::format::Fmt;
            let cmd = egui::Modifiers::COMMAND;
            let shift = egui::Modifiers { shift: true, ..egui::Modifiers::COMMAND };
            let focused = ctx.memory(|m| m.has_focus(body_id));
            // Shifted keys first (a plain Ctrl pattern also matches with Shift held).
            let keys: [(egui::Modifiers, Key, Fmt); 15] = [
                (shift, Key::X, Fmt::Strike),
                (shift, Key::E, Fmt::CodeBlock),
                (shift, Key::Num8, Fmt::Bullet),
                (shift, Key::Num7, Fmt::Numbered),
                (shift, Key::Num9, Fmt::Task),
                (shift, Key::Q, Fmt::Quote),
                (cmd, Key::B, Fmt::Bold),
                (cmd, Key::I, Fmt::Italic),
                (cmd, Key::E, Fmt::Code),
                (cmd, Key::K, Fmt::Link),
                (cmd, Key::Num1, Fmt::Heading(1)),
                (cmd, Key::Num2, Fmt::Heading(2)),
                (cmd, Key::Num3, Fmt::Heading(3)),
                (cmd, Key::Num0, Fmt::Heading(0)),
                (cmd, Key::Enter, Fmt::ToggleDone),
            ];
            let pressed = if focused {
                ctx.input_mut(|i| keys.iter().find(|(m, k, _)| i.consume_shortcut(&egui::KeyboardShortcut::new(*m, *k))).map(|(_, _, f)| *f))
            } else {
                None
            };
            pressed.or(self.pending_fmt.take())
        };
        if let Some(f) = fmt {
            let mut state = egui::TextEdit::load_state(&ctx, body_id).unwrap_or_default();
            let len = d.body.chars().count();
            let (a, b) = state.cursor.char_range().map(|r| (r.primary.index.0, r.secondary.index.0)).unwrap_or((len, len));
            let (text, (na, nb)) = crate::format::apply(&d.body, (a, b), f);
            if text != d.body {
                d.body = text;
                changed = true;
            }
            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(na), egui::text::CCursor::new(nb))));
            state.store(&ctx, body_id);
            ctx.memory_mut(|m| m.request_focus(body_id));
        }
        let col = ui.available_rect_before_wrap();
        let outer = if page.is_positive() && page.width() > col.width() + 1.0 { Rect::from_min_max(Pos2::new(page.left(), col.top()), Pos2::new(page.right(), col.bottom())) } else { col };
        let pad = col.left() - outer.left();
        let mut scroll_ui = ui.new_child(egui::UiBuilder::new().max_rect(outer).layout(egui::Layout::top_down(egui::Align::Min)));
        egui::ScrollArea::vertical().id_salt("note-text").auto_shrink([false, false]).show(&mut scroll_ui, |ui| {
          ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(pad);
          ui.vertical(|ui| {
            ui.set_width(col.width());
            // Live mode: a press on a check box ticks it, and leaves the cursor (and the
            // keyboard) where they were, so the line stays rendered.
            let body_id = egui::Id::unique("note-body");
            let tick = ctx
                .input(|i| if i.pointer.primary_pressed() { i.pointer.interact_pos() } else { None })
                .and_then(|p| d.live_boxes.iter().find(|(r, _)| r.contains(p)).map(|(_, b)| b.clone()));
            // Note suggestions after `[[`: keys first (before the text sees them), and a clicked
            // suggestion from last frame goes in now.
            let focused_now = ctx.memory(|m| m.has_focus(body_id));
            let caret = if focused_now {
                egui::TextEdit::load_state(&ctx, body_id).and_then(|s| s.cursor.char_range()).map(|r| d.body.char_indices().nth(r.primary.index.0).map(|(b, _)| b).unwrap_or(d.body.len()))
            } else {
                None
            };
            let mut accept: Option<(usize, usize, String)> = self.link_pending.take();
            if accept.is_none()
                && let Some(c) = caret
                && let Some((start, typed)) = crate::links::typing_link(&d.body, c)
                && self.link_hide != Some(start)
            {
                let sugg = link_suggestions(&self.notes, &d.path, &typed);
                if !sugg.is_empty() {
                    let n = sugg.len();
                    let (up, down, enter, esc) = ctx.input_mut(|i| {
                        (
                            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                            i.consume_key(egui::Modifiers::NONE, Key::Enter) || i.consume_key(egui::Modifiers::NONE, Key::Tab),
                            i.consume_key(egui::Modifiers::NONE, Key::Escape),
                        )
                    });
                    if down {
                        self.link_sel = (self.link_sel + 1) % n;
                    }
                    if up {
                        self.link_sel = (self.link_sel + n - 1) % n;
                    }
                    if esc {
                        self.link_hide = Some(start);
                    }
                    if enter {
                        accept = Some((start, c, sugg[self.link_sel.min(n - 1)].clone()));
                    }
                }
            }
            if let Some((start, end, name)) = accept
                && start <= end
                && end <= d.body.len()
                && d.body.is_char_boundary(start)
                && d.body.is_char_boundary(end)
            {
                // The name, then `]]` (unless it's already there).
                let closed = d.body[end..].starts_with("]]");
                d.body.replace_range(start..end, &if closed { name.clone() } else { format!("{name}]]") });
                let after = d.body[..start].chars().count() + name.chars().count() + 2;
                let mut st = egui::TextEdit::load_state(&ctx, body_id).unwrap_or_default();
                st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(after))));
                st.store(&ctx, body_id);
                ctx.memory_mut(|m| m.request_focus(body_id));
                self.link_sel = 0;
                changed = true;
            }
            let keep = tick.as_ref().map(|_| (egui::TextEdit::load_state(&ctx, body_id), ctx.memory(|m| m.has_focus(body_id))));
            // Code block panels go under the text.
            let panels_under = ui.painter().add(egui::Shape::Noop);
            let glow_under = ui.painter().add(egui::Shape::Noop);
            let out = egui::TextEdit::multiline(&mut d.body)
                .id(egui::Id::unique("note-body"))
                .frame(egui::Frame::NONE)
                .desired_width(f32::INFINITY)
                .desired_rows(24)
                .lock_focus(true)
                .hint_text(egui::RichText::new("Write in Markdown…").size(editor_size * 1.06 * fonts::scale()).italics().color(weak))
                .layouter(&mut layouter)
                .show(ui);
            // Live mode: bullets, check boxes, quote bars and rules; a check box click ticks it.
            if let (Some(hit), Some((state, focused))) = (tick.clone(), keep) {
                match state {
                    Some(s) => s.store(&ctx, body_id),
                    None => egui::TextEdit::store_state(&ctx, body_id, egui::text_edit::TextEditState::default()),
                }
                if !focused {
                    ctx.memory_mut(|m| m.surrender_focus(body_id));
                }
                match hit {
                    super::live_edit::Hit::Check(b) => {
                        if let Some(c) = d.body.as_bytes().get(b).copied()
                            && matches!(c, b' ' | b'x' | b'X' | b'/')
                        {
                            d.body.replace_range(b..b + 1, if matches!(c, b'x' | b'X') { " " } else { "x" });
                            changed = true;
                        }
                    }
                    super::live_edit::Hit::Link(t) => follow = Some(t),
                    super::live_edit::Hit::Task(n, at) => {
                        self.task_pop = Some(super::todos::TaskPop { line: n, at, fresh: true, tag_input: String::new() });
                    }
                    super::live_edit::Hit::Copy(t) => {
                        ctx.copy_text(t);
                        self.note_msg = None;
                    }
                }
            }
            // Ctrl+click on a link (in either mode) follows it; Ctrl over one shows a hand.
            let ctrl = ctx.input(|i| i.modifiers.command);
            if ctrl
                && let Some(p) = ctx.pointer_hover_pos()
                && out.response.response.rect.contains(p)
            {
                let cc = out.galley.cursor_from_pos(p - out.galley_pos);
                let byte = d.body.char_indices().nth(cc.index.0).map(|(b, _)| b).unwrap_or(d.body.len());
                if let Some(l) = crate::links::link_at(&d.body, byte) {
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    if out.response.response.clicked() {
                        follow = Some(l.target);
                    }
                }
            }
            // The suggestion list, under the cursor.
            let caret_after = out.cursor_range.filter(|_| ctx.memory(|m| m.has_focus(body_id))).map(|r| r.primary);
            if let Some(cc) = caret_after {
                let c = d.body.char_indices().nth(cc.index.0).map(|(b, _)| b).unwrap_or(d.body.len());
                match crate::links::typing_link(&d.body, c) {
                    Some((start, typed)) if self.link_hide != Some(start) => {
                        let sugg = link_suggestions(&self.notes, &d.path, &typed);
                        if !sugg.is_empty() {
                            let at = out.galley.pos_from_cursor(cc).translate(out.galley_pos.to_vec2());
                            if let Some(name) = link_popup(&ctx, at.left_bottom() + Vec2::new(0.0, 4.0), &sugg, &mut self.link_sel) {
                                self.link_pending = Some((start, c, name));
                                ctx.request_repaint();
                            }
                        }
                    }
                    Some(_) => {}
                    None => {
                        self.link_hide = None;
                        self.link_sel = 0;
                    }
                }
            }
            d.live_boxes = super::live_edit::code_panels(ui, &d.body, &out, &live_look, panels_under);
            // ---- line handles: drag a line (with what's nested under it) somewhere else
            if !d.body.is_empty() {
                let gx = out.galley_pos.x;
                let clip = out.text_clip_rect;
                let right = out.response.response.rect.right();
                let starts: Vec<usize> = {
                    let mut v = vec![0usize];
                    let mut c = 0;
                    for l in d.body.split_inclusive('\n') {
                        c += l.chars().count();
                        v.push(c);
                    }
                    v
                };
                let n_lines = d.body.split('\n').count();
                let row_of = |n: usize| out.galley.pos_from_cursor(egui::text::CCursor::new(starts[n.min(starts.len() - 1)])).translate(out.galley_pos.to_vec2());
                let line_at = |y: f32| {
                    let cc = out.galley.cursor_from_pos(Vec2::new(0.0, y - out.galley_pos.y));
                    starts.iter().rposition(|&s| s <= cc.index.0).unwrap_or(0).min(n_lines - 1)
                };
                let mut started = None;
                // The handle under the pointer; while it's pressed, the same one stays.
                let down = ctx.input(|i| i.pointer.primary_down());
                let origin = ctx.input(|i| i.pointer.press_origin());
                let held = d.grip.filter(|(_, _, r)| down && origin.is_some_and(|o| r.contains(o)));
                let target = if d.line_drag.is_some() || self.task_pop.is_some() {
                    None
                } else if held.is_some() {
                    held
                } else if !down
                    && let Some(p) = ctx.pointer_hover_pos()
                    && p.y >= clip.top()
                    && p.y <= clip.bottom()
                    && p.x >= gx - 34.0
                    && p.x <= right
                {
                    let n = line_at(p.y);
                    let (a, b) = crate::blocks::block_at(&d.body, n);
                    let first = d.body.split('\n').nth(a).unwrap_or("");
                    let row = row_of(a);
                    let grip = Rect::from_center_size(Pos2::new(gx - 16.0, row.center().y), Vec2::new(16.0, 22.0));
                    (!first.trim().is_empty() && grip.top() >= clip.top() - 4.0 && grip.bottom() <= clip.bottom() + 4.0).then_some((a, b, grip))
                } else {
                    None
                };
                d.grip = target;
                // A press on the handle that moves a few points starts the drag.
                if let Some((a, b, _)) = held
                    && let (Some(o), Some(p)) = (origin, ctx.input(|i| i.pointer.latest_pos()))
                    && (p - o).length() > 4.0
                {
                    started = Some((a, b));
                }
                if let Some((a, b, grip)) = target {
                    {
                        egui::Area::new(egui::Id::unique("line-grip")).order(egui::Order::Foreground).fixed_pos(grip.min).show(&ctx, |ui| {
                            let (r, resp) = ui.allocate_exact_size(grip.size(), Sense::drag());
                            let hot = resp.hovered();
                            if hot {
                                ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            let col = if hot { v.text_color() } else { weak.gamma_multiply(0.7) };
                            for dy in [-5.0f32, 0.0, 5.0] {
                                for dx in [-2.5f32, 2.5] {
                                    ui.painter().circle_filled(r.center() + Vec2::new(dx, dy), 1.4, col);
                                }
                            }
                            resp.on_hover_text(if b > a { "Drag to move these lines" } else { "Drag to move this line" });
                        });
                    }
                }
                if let Some(s) = started {
                    d.line_drag = Some(s);
                }
                if let Some((a, b)) = d.line_drag {
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    let released = !ctx.input(|i| i.pointer.primary_down());
                    let p = ctx.input(|i| i.pointer.latest_pos());
                    // Where it would go: before or after the line under the pointer.
                    let gap = p.map(|p| {
                        let y = p.y.clamp(clip.top(), clip.bottom());
                        let n = line_at(y);
                        let row = row_of(n);
                        if y < row.center().y { n } else { crate::blocks::block_at(&d.body, n).1 + 1 }
                    });
                    if let Some(g) = gap {
                        let y = if g < n_lines { row_of(g).top() - 1.0 } else { row_of(n_lines - 1).bottom() + 1.0 };
                        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("line-drop")));
                        let ok = !(g >= a && g <= b + 1);
                        if ok {
                            let acc = v.selection.bg_fill;
                            painter.line_segment([Pos2::new(gx - 4.0, y), Pos2::new(right, y)], Stroke::new(2.0, acc));
                            painter.circle_stroke(Pos2::new(gx - 8.0, y), 3.5, Stroke::new(2.0, acc));
                        }
                        // What's moving, beside the pointer.
                        if let Some(p) = p {
                            let first = d.body.split('\n').nth(a).unwrap_or("").trim().to_string();
                            let label = if b > a { format!("{first}  +{}", b - a) } else { first };
                            let g = painter.layout_no_wrap(label.chars().take(60).collect(), fonts::ui_font(12.5), v.text_color());
                            let r = Rect::from_min_size(p + Vec2::new(14.0, 8.0), g.size() + Vec2::new(18.0, 10.0));
                            painter.add(egui::epaint::Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_black_alpha(70) }.as_shape(r, r_sm()));
                            painter.rect_filled(r, r_sm(), v.window_fill);
                            painter.rect_stroke(r, r_sm(), Stroke::new(1.0, v.selection.bg_fill), egui::StrokeKind::Inside);
                            painter.galley(r.min + Vec2::new(9.0, 5.0), g, v.text_color());
                        }
                    }
                    if released {
                        if let Some(g) = gap
                            && let Some(new) = crate::blocks::move_block(&d.body, a, b, g)
                        {
                            d.body = new;
                            changed = true;
                        }
                        d.line_drag = None;
                    }
                    ctx.request_repaint();
                }
            }
            // Super todos: a button beside the one at the cursor (or under the pointer).
            let caret_char = out.cursor_range.filter(|_| ctx.memory(|m| m.has_focus(body_id))).map(|r| r.primary.index.0);
            self.caret_state = caret_char.map(|c| (body_id, c));
            if todos_on {
                let caret_b = caret_char.map(|c| d.body.char_indices().nth(c).map(|(b, _)| b).unwrap_or(d.body.len()));
                d.live_boxes.extend(NotesApp::task_buttons(ui, &d.body, &out, &todo_tags, caret_b, weak));
                // A popover asked for by Shift+Ctrl+T opens under its line.
                if let Some(n) = self.task_pop_line.take() {
                    let start: usize = d.body.split_inclusive('\n').take(n).map(str::len).sum();
                    let row = out.galley.pos_from_cursor(egui::text::CCursor::new(d.body[..start.min(d.body.len())].chars().count())).translate(out.galley_pos.to_vec2());
                    let right = out.response.response.rect.right();
                    self.task_pop = Some(super::todos::TaskPop { line: n, at: Pos2::new(right - 240.0, row.bottom() + 6.0), fresh: true, tag_input: String::new() });
                }
            }
            if live_on {
                let raw = if tick.is_some() { live_raw } else { super::live_edit::shift_raw(&live_before, &d.body, live_raw) };
                let more = super::live_edit::decorate(ui, &d.body, raw, live_held, &out, &live_look, glow_under);
                d.live_boxes.extend(more);
            }
            // Where a selection sits on screen, for the format bar.
            if let Some(range) = out.cursor_range
                && !range.is_empty()
                && ui.ctx().memory(|m| m.has_focus(egui::Id::unique("note-body")))
            {
                let (lo, hi) = if range.primary.index.0 <= range.secondary.index.0 { (range.primary, range.secondary) } else { (range.secondary, range.primary) };
                let top = out.galley.pos_from_cursor(lo).translate(out.galley_pos.to_vec2());
                let bottom = out.galley.pos_from_cursor(hi).translate(out.galley_pos.to_vec2());
                sel_rect = Some((top, bottom, out.text_clip_rect));
            }
            let r = out.response.response;
            if r.changed() {
                changed = true;
            }
            // The standard editing menu. The command runs next frame, with the keyboard back
            // in the text (so the selection is kept).
            let has_sel = egui::TextEdit::load_state(ui.ctx(), r.id).and_then(|s| s.cursor.char_range()).is_some_and(|c| !c.is_empty());
            r.context_menu(|ui| {
                menu_style(ui);
                for (label, sc, c, on) in [
                    ("Undo", "Ctrl+Z", EditCmd::Undo, true),
                    ("Redo", "Shift+Ctrl+Z", EditCmd::Redo, true),
                ] {
                    if ui.add_enabled(on, egui::Button::new(label).shortcut_text(sc)).clicked() {
                        edit = Some(c);
                        ui.close();
                    }
                }
                ui.separator();
                for (label, sc, c, on) in [
                    ("Cut", "Ctrl+X", EditCmd::Cut, has_sel),
                    ("Copy", "Ctrl+C", EditCmd::Copy, has_sel),
                    ("Paste", "Ctrl+V", EditCmd::Paste, true),
                    ("Delete", "", EditCmd::Delete, has_sel),
                ] {
                    if ui.add_enabled(on, egui::Button::new(label).shortcut_text(sc)).clicked() {
                        edit = Some(c);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.add(egui::Button::new("Select All").shortcut_text("Ctrl+A")).clicked() {
                    edit = Some(EditCmd::SelectAll);
                    ui.close();
                }
                ui.separator();
                ui.menu_button("Format", |ui| {
                    menu_style(ui);
                    for (k, (f, label, sc)) in crate::format::Fmt::ALL.iter().enumerate() {
                        if [4, 5, 9, 14].contains(&k) {
                            ui.separator();
                        }
                        if ui.add(egui::Button::new(*label).shortcut_text(*sc)).clicked() {
                            fmt_pick = Some(*f);
                            ui.close();
                        }
                    }
                });
            });
          });
          });
        });
        if changed {
            d.dirty = true;
            d.edited_at = now;
        }
        // The format bar: over a selection while the text has the keyboard, and kept up while
        // the pointer is on it (a click there takes the keyboard from the text first).
        let keep = self.fmt_bar.is_some_and(|r| ctx.pointer_hover_pos().is_some_and(|p| r.expand(6.0).contains(p)));
        match (sel_rect, keep) {
            (Some((top, bottom, clip)), _) => {
                if let Some(f) = self.format_bar(&ctx, top, bottom, clip) {
                    fmt_pick = Some(f);
                }
            }
            (None, true) => {
                if let Some(r) = self.fmt_bar {
                    let (top, bottom) = (Rect::from_min_size(r.left_bottom() + Vec2::new(0.0, 8.0), Vec2::ZERO), Rect::from_min_size(r.left_bottom(), Vec2::ZERO));
                    if let Some(f) = self.format_bar(&ctx, top, bottom, ctx.content_rect()) {
                        fmt_pick = Some(f);
                    }
                }
            }
            (None, false) => self.fmt_bar = None,
        }
        if fmt_pick.is_some() {
            self.pending_fmt = fmt_pick;
            ctx.request_repaint();
        }
        if let Some(t) = title_commit {
            self.retitle(&t);
        }
        if todos_on {
            self.task_popover(&ctx);
        }
        if let Some(t) = follow {
            self.follow_link(&t);
        }
        if let Some(c) = edit {
            self.edit_cmd = Some(c);
            ctx.memory_mut(|m| m.request_focus(egui::Id::unique("note-body")));
            ctx.request_repaint();
        }
    }

    /// A small bar over the selected text: bold, italic, strikethrough, code, link, headings,
    /// list and quote. Returns the format clicked.
    fn format_bar(&mut self, ctx: &egui::Context, top: Rect, bottom: Rect, clip: Rect) -> Option<crate::format::Fmt> {
        use crate::format::Fmt;
        let v = ctx.global_style().visuals.clone();
        let list_icon = self.icon(ctx, &["view-list-symbolic"], 14.0);
        let quote_icon = self.icon(ctx, &["format-text-blockquote-symbolic", "mail-reply-sender-symbolic"], 14.0);
        let link_icon = self.icon(ctx, &["insert-link-symbolic"], 14.0);
        let size = Vec2::new(372.0, 34.0);
        // Above the selection's first line, or below its last when there's no room above.
        let mut pos = Pos2::new(top.left().clamp(clip.left(), (clip.right() - size.x).max(clip.left())), top.top() - size.y - 6.0);
        if pos.y < clip.top() {
            pos.y = bottom.bottom() + 6.0;
        }
        let pick = std::cell::Cell::new(None);
        let resp = egui::Area::new(egui::Id::unique("fmt-bar")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(4, 3)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    ui.spacing_mut().button_padding = Vec2::new(7.0, 4.0);
                    let fg = v.text_color();
                    let b = |ui: &mut egui::Ui, text: egui::RichText, tip: &str, f: Fmt| {
                        if ui.add(egui::Button::new(text).frame_when_inactive(false).min_size(Vec2::new(28.0, 26.0)).corner_radius(r_sm())).on_hover_text(tip).clicked() {
                            pick.set(Some(f));
                        }
                    };
                    let font = fonts::ui_font(14.0);
                    b(ui, egui::RichText::new("B").font(egui::FontId::new(font.size, rivet_look::live::editor_bold_family())).color(fg), "Bold (Ctrl+B)", Fmt::Bold);
                    b(ui, egui::RichText::new("I").font(font.clone()).italics().color(fg), "Italic (Ctrl+I)", Fmt::Italic);
                    b(ui, egui::RichText::new("S").font(font.clone()).strikethrough().color(fg), "Strikethrough (Shift+Ctrl+X)", Fmt::Strike);
                    b(ui, egui::RichText::new("</>").font(egui::FontId::monospace(12.5)).color(fg), "Code (Ctrl+E)", Fmt::Code);
                    let ib = |ui: &mut egui::Ui, icon: &Option<egui::TextureHandle>, fallback: &str, tip: &str, f: Fmt| {
                        let btn = match icon {
                            Some(t) => egui::Button::image(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(14.0)).tint(fg)),
                            None => egui::Button::new(egui::RichText::new(fallback).font(fonts::ui_font(12.5)).color(fg)),
                        };
                        if ui.add(btn.frame_when_inactive(false).min_size(Vec2::new(28.0, 26.0)).corner_radius(r_sm())).on_hover_text(tip).clicked() {
                            pick.set(Some(f));
                        }
                    };
                    ib(ui, &link_icon, "Link", "Link (Ctrl+K)", Fmt::Link);
                    ui.add_space(3.0);
                    let sep = ui.allocate_exact_size(Vec2::new(1.0, 18.0), Sense::hover()).0;
                    ui.painter().line_segment([sep.center_top(), sep.center_bottom()], Stroke::new(1.0, v.weak_text_color().gamma_multiply(0.5)));
                    ui.add_space(3.0);
                    for (n, f) in [("H1", Fmt::Heading(1)), ("H2", Fmt::Heading(2)), ("H3", Fmt::Heading(3))] {
                        b(ui, egui::RichText::new(n).font(fonts::ui_font(12.5)).strong().color(fg), &format!("Heading {} (Ctrl+{})", &n[1..], &n[1..]), f);
                    }
                    ui.add_space(3.0);
                    let sep = ui.allocate_exact_size(Vec2::new(1.0, 18.0), Sense::hover()).0;
                    ui.painter().line_segment([sep.center_top(), sep.center_bottom()], Stroke::new(1.0, v.weak_text_color().gamma_multiply(0.5)));
                    ui.add_space(3.0);
                    ib(ui, &list_icon, "List", "Bulleted list (Shift+Ctrl+8)", Fmt::Bullet);
                    ib(ui, &None, "1.", "Numbered list (Shift+Ctrl+7)", Fmt::Numbered);
                    ib(ui, &quote_icon, "Quote", "Quote (Shift+Ctrl+Q)", Fmt::Quote);
                });
            });
        });
        self.fmt_bar = Some(resp.response.rect);
        pick.get()
    }

    // ---------------------------------------------------------------- welcome

    fn welcome(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let icon = self.app_icon(&ctx);
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.16).max(20.0));
            if let Some(t) = &icon {
                ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(88.0)));
            }
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Rivet Notes").size(24.0).strong());
            ui.label(egui::RichText::new("A vault is a folder of Markdown notes. Open one to start.").color(v.weak_text_color()));
            ui.add_space(18.0);
            if ui.add(egui::Button::new(egui::RichText::new("Open Folder as Vault…").color(v.selection.stroke.color)).fill(v.selection.bg_fill).corner_radius(r_md()).min_size(Vec2::new(220.0, 36.0))).clicked() {
                self.prompt = Some(Prompt::OpenVault(String::new()));
            }
            let recent: Vec<PathBuf> = self.s.vaults.iter().filter(|p| p.is_dir()).cloned().collect();
            if !recent.is_empty() {
                ui.add_space(18.0);
                ui.label(egui::RichText::new("Recent vaults").small().color(v.weak_text_color()));
                for p in recent {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    if ui.link(name).on_hover_text(p.display().to_string()).clicked() {
                        self.open_vault(p);
                    }
                }
            }
        });
    }

    // ---------------------------------------------------------------- questions

    fn prompts(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.prompt.take() else { return };
        let mut next = None;
        // Icons for the New Note / New Folder headers (Rivet's kit, like its New Folder dialog).
        let px = 40.0;
        let note_icon = self.icon(ctx, &["text-markdown", "text-x-generic"], px);
        let folder_icon = self.icon(ctx, &["folder", "inode-directory"], px);
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(26)).corner_radius(r_lg());
        egui::Modal::new(egui::Id::unique("notes-prompt")).frame(frame).show(ctx, |ui| {
            ui.set_width(440.0);
            let v = ui.visuals().clone();
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            // A title with an icon and a line under it (where it goes).
            let header = |ui: &mut egui::Ui, icon: &Option<egui::TextureHandle>, title: &str, sub: &str| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if let Some(t) = icon {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                        paint_tex(ui.painter(), t, r, Color32::WHITE);
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.label(egui::RichText::new(title).heading().color(v.strong_text_color()));
                        ui.label(egui::RichText::new(sub).color(v.weak_text_color()));
                    });
                });
                ui.add_space(18.0);
            };
            // A raised field, outlined in the accent while typing (like Rivet's dialogs).
            let field = |ui: &mut egui::Ui, text: &mut String, hint: &str| -> bool {
                let id = egui::Id::unique(("prompt-field", hint));
                let focused = ui.memory(|m| m.has_focus(id));
                let fill = palette::mix(v.window_fill, v.text_color(), 0.06);
                let shadow = egui::Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(if v.dark_mode { 70 } else { 30 }) };
                let stroke = Stroke::new(1.5, if focused { v.selection.bg_fill } else { Color32::TRANSPARENT });
                let r = egui::Frame::NONE
                    .fill(fill)
                    .shadow(shadow)
                    .stroke(stroke)
                    .corner_radius(r_md())
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.add(egui::TextEdit::singleline(text).id(id).frame(egui::Frame::NONE).font(fonts::ui_font(15.0)).min_size(Vec2::new(0.0, 24.0)).align(Align2::LEFT_CENTER).hint_text(hint).desired_width(f32::INFINITY))
                    })
                    .inner;
                if !r.has_focus() && !r.lost_focus() {
                    r.request_focus();
                }
                r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
            };
            let buttons = |ui: &mut egui::Ui, ok: &str| -> (bool, bool) {
                let (mut a, mut b) = (false, false);
                ui.add_space(22.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().button_padding = Vec2::new(16.0, 7.0);
                    ui.spacing_mut().item_spacing.x = 8.0;
                    a = ui.add(egui::Button::new(egui::RichText::new(ok).color(v.selection.stroke.color).strong()).fill(v.selection.bg_fill).corner_radius(r_sm())).clicked();
                    b = ui.add(egui::Button::new("Cancel").corner_radius(r_sm())).clicked();
                });
                (a, b)
            };
            let place = |f: &str| if f.is_empty() { "the top of the vault".to_string() } else { format!("\"{f}\"") };
            match prompt {
                Prompt::NewNote { folder, mut title } => {
                    header(ui, &note_icon, "New Note", &format!("In {}", place(&folder)));
                    let enter = field(ui, &mut title, "Title");
                    let (ok, cancel) = buttons(ui, "Create");
                    if (ok || enter) && !title.trim().is_empty() {
                        self.new_note(&folder, &title);
                    } else if !(cancel || esc) {
                        next = Some(Prompt::NewNote { folder, title });
                    }
                }
                Prompt::NewFolder { parent, mut name } => {
                    header(ui, &folder_icon, "New Folder", &format!("In {}", place(&parent)));
                    let enter = field(ui, &mut name, "Folder name");
                    let (ok, cancel) = buttons(ui, "Create");
                    if (ok || enter) && !name.trim().is_empty() && !name.contains('/') {
                        if let Some(vlt) = &self.vault {
                            let p = vlt.join(&parent).join(name.trim());
                            if let Err(e) = std::fs::create_dir_all(&p) {
                                self.note_msg = Some((format!("Couldn't make the folder: {e}"), true));
                            }
                            self.current_folder = p.strip_prefix(vlt).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
                        }
                        self.rescan();
                    } else if !(cancel || esc) {
                        next = Some(Prompt::NewFolder { parent, name });
                    }
                }
                Prompt::Rename { path, mut name } => {
                    ui.heading("Rename");
                    ui.add_space(10.0);
                    let enter = field(ui, &mut name, "New name");
                    let (ok, cancel) = buttons(ui, "Rename");
                    if ok || enter {
                        self.rename(&path, &name);
                    } else if !(cancel || esc) {
                        next = Some(Prompt::Rename { path, name });
                    }
                }
                Prompt::Delete(path) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    ui.heading(format!("Move \"{name}\" to the Trash?"));
                    ui.add_space(6.0);
                    ui.label(if path.is_dir() { "The folder and every note in it go to the Trash. You can get them back from there." } else { "You can get it back from the Trash." });
                    let (mut ok, mut cancel) = (false, false);
                    ui.add_space(12.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().button_padding = Vec2::new(14.0, 6.0);
                        let danger = status(ui.ctx()).danger;
                        ok = ui.add(egui::Button::new(egui::RichText::new("Move to Trash").color(rivet_look::theme::StatusColors::on(danger))).fill(danger).corner_radius(r_sm())).clicked();
                        cancel = ui.add(egui::Button::new("Cancel").corner_radius(r_sm())).clicked();
                    });
                    if ok {
                        self.trash(&path);
                    } else if !(cancel || esc) {
                        next = Some(Prompt::Delete(path));
                    }
                }
                Prompt::OpenVault(mut text) => {
                    ui.heading("Open Folder as Vault");
                    ui.label(egui::RichText::new("Any folder works; its Markdown files become the vault's notes. Nothing is added to it.").color(v.weak_text_color()));
                    ui.add_space(10.0);
                    let enter = field(ui, &mut text, "~/Documents/Notes");
                    // Folders that match what's typed so far.
                    let typed = expand(&text);
                    let (dir, stem) = if text.ends_with('/') { (typed.clone(), String::new()) } else { (typed.parent().map(Path::to_path_buf).unwrap_or_default(), typed.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()) };
                    let mut sugg: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).filter(|p| {
                        let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                        !n.starts_with('.') && n.starts_with(&stem)
                    }).collect()).unwrap_or_default();
                    sugg.sort();
                    sugg.truncate(6);
                    for s in sugg {
                        if ui.link(s.display().to_string()).clicked() {
                            text = format!("{}/", s.display());
                        }
                    }
                    let (ok, cancel) = buttons(ui, "Open");
                    if (ok || enter) && typed.is_dir() {
                        self.open_vault(typed);
                    } else if (ok || enter) && !text.trim().is_empty() {
                        self.note_msg = Some((format!("There's no folder {}", typed.display()), true));
                        next = Some(Prompt::OpenVault(text));
                    } else if !(cancel || esc) {
                        next = Some(Prompt::OpenVault(text));
                    }
                }
                Prompt::Conflict => {
                    ui.heading("The note changed on disk");
                    ui.add_space(6.0);
                    ui.label("Another program changed this note while you had unsaved changes here.");
                    let (mut keep, mut theirs) = (false, false);
                    ui.add_space(12.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().button_padding = Vec2::new(14.0, 6.0);
                        keep = ui.add(egui::Button::new(egui::RichText::new("Keep Mine").color(v.selection.stroke.color)).fill(v.selection.bg_fill).corner_radius(r_sm())).clicked();
                        theirs = ui.add(egui::Button::new("Load Theirs").corner_radius(r_sm())).clicked();
                    });
                    if keep {
                        self.save_now();
                    } else if theirs {
                        if let Some(p) = self.doc.take().map(|d| d.path) {
                            self.open_note(&p);
                        }
                    } else {
                        next = Some(Prompt::Conflict);
                    }
                }
            }
        });
        if next.is_some() {
            self.prompt = next;
        }
    }

}

/// A font family picker: the default entry, then the installed families.
pub(super) fn font_pick(ui: &mut egui::Ui, id: &str, value: &mut String, default_label: &str, families: &[String]) {
    let shown = if value.is_empty() { default_label.to_string() } else { value.clone() };
    egui::ComboBox::from_id_salt(id).width(280.0).height(360.0).selected_text(shown).show_ui(ui, |ui| {
        menu_style(ui);
        if ui.selectable_label(value.is_empty(), default_label).clicked() {
            value.clear();
        }
        ui.separator();
        for f in families {
            if ui.selectable_label(value == f, f).clicked() {
                *value = f.clone();
            }
        }
    });
}

/// The byte ranges of code in Markdown: fenced blocks (with their fence lines) and `inline`
/// spans outside them.
fn code_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut pos = 0;
    let mut fence: Option<usize> = None;
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        let is_fence = t.starts_with("```") || t.starts_with("~~~");
        match (fence, is_fence) {
            (None, true) => fence = Some(pos),
            (Some(start), true) => {
                out.push(start..pos + line.len());
                fence = None;
            }
            (Some(_), false) => {}
            (None, false) => {
                let b = line.as_bytes();
                let mut i = 0;
                while i < b.len() {
                    if b[i] == b'`'
                        && let Some(j) = line[i + 1..].find('`')
                    {
                        out.push(pos + i..pos + i + 1 + j + 1);
                        i += j + 2;
                        continue;
                    }
                    i += 1;
                }
            }
        }
        pos += line.len();
    }
    if let Some(start) = fence {
        out.push(start..text.len());
    }
    out
}

/// Put the code parts of a laid-out note in the monospace font (colors kept).
fn mono_code(job: &mut egui::text::LayoutJob, text: &str, mono: &egui::FontId) {
    let ranges = code_ranges(text);
    if ranges.is_empty() {
        return;
    }
    let mut out = Vec::with_capacity(job.sections.len() + ranges.len() * 2);
    for sec in std::mem::take(&mut job.sections) {
        // Cut the section where code starts or ends inside it.
        let (start, end): (usize, usize) = (sec.byte_range.start.into(), sec.byte_range.end.into());
        let mut cuts = vec![start, end];
        for r in &ranges {
            for c in [r.start, r.end] {
                if c > start && c < end && text.is_char_boundary(c) {
                    cuts.push(c);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for (k, w) in cuts.windows(2).enumerate() {
            let mut part = sec.clone();
            part.byte_range = egui::text::ByteIndex::from(w[0])..egui::text::ByteIndex::from(w[1]);
            if k > 0 {
                part.leading_space = 0.0;
            }
            if ranges.iter().any(|r| r.start <= w[0] && w[1] <= r.end) {
                part.format.font_id = mono.clone();
            }
            out.push(part);
        }
    }
    job.sections = out;
}

/// `~/x` → the home folder's x.
fn expand(p: &str) -> PathBuf {
    let p = p.trim();
    match p.strip_prefix("~/") {
        Some(rest) => rivet_xdg::places::home().join(rest),
        None if p == "~" => rivet_xdg::places::home(),
        None => PathBuf::from(p),
    }
}

/// A note or folder being dragged in the sidebar.
#[derive(Clone)]
struct ItemDrag {
    path: PathBuf,
    name: String,
}

/// Start dragging a row; while something is dragged over it, `target`'s drop is shown on `rect`.
/// Returns true when it was dropped here.
fn drag_and_drop(ui: &egui::Ui, resp: &egui::Response, rect: Rect, me: &Path, name: &str, target: &Path) -> bool {
    if resp.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), ItemDrag { path: me.to_path_buf(), name: name.to_string() });
    }
    let Some(drag) = egui::DragAndDrop::payload::<ItemDrag>(ui.ctx()) else { return false };
    // A folder can't go into itself; a note dropped where it already is does nothing.
    let pointer = ui.ctx().pointer_hover_pos();
    let over = pointer.is_some_and(|p| rect.contains(p));
    let fits = !target.starts_with(&drag.path) && drag.path.parent() != Some(target);
    if over && fits {
        let v = ui.visuals();
        ui.painter().rect_filled(rect, r_sm(), v.selection.bg_fill.gamma_multiply(0.25));
        ui.painter().rect_stroke(rect, r_sm(), Stroke::new(1.5, v.selection.bg_fill), egui::StrokeKind::Inside);
        if ui.input(|i| i.pointer.any_released()) {
            return true;
        }
    }
    false
}

/// Notes to offer after `[[`, best first (the open note left out): their names as links use them.
fn link_suggestions(notes: &[Note], open: &Path, typed: &str) -> Vec<String> {
    let typed = typed.trim();
    let mut scored: Vec<(i32, &Note)> = notes
        .iter()
        .filter(|n| n.path != open)
        .filter_map(|n| rivet_look::quick::fuzzy_score(&n.title, typed).max(rivet_look::quick::fuzzy_score(&n.rel, typed).map(|s| s - 200)).map(|s| (s, n)))
        .collect();
    if typed.is_empty() {
        scored.sort_by_key(|s| std::cmp::Reverse(s.1.modified));
    } else {
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.to_lowercase().cmp(&b.1.title.to_lowercase())));
    }
    scored.into_iter().take(8).map(|(_, n)| n.title.clone()).collect()
}

/// The list of note suggestions at `pos`. Returns the one clicked.
fn link_popup(ctx: &egui::Context, pos: Pos2, sugg: &[String], sel: &mut usize) -> Option<String> {
    *sel = (*sel).min(sugg.len().saturating_sub(1));
    let mut picked = None;
    egui::Area::new(egui::Id::unique("link-suggestions")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).corner_radius(r_md()).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.set_width(260.0);
            let v = ui.visuals().clone();
            for (i, name) in sugg.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(260.0, 26.0), Sense::click());
                if resp.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                    *sel = i;
                }
                if i == *sel {
                    let (fill, stroke) = selected_tile(&v, v.window_fill);
                    ui.painter().rect_filled(r, r_sm(), fill);
                    ui.painter().rect_stroke(r, r_sm(), stroke, egui::StrokeKind::Inside);
                }
                ui.painter().with_clip_rect(r.shrink(4.0)).text(r.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, name, fonts::ui_font(13.0), v.text_color());
                if resp.clicked() {
                    picked = Some(name.clone());
                }
            }
            ui.add_space(2.0);
            ui.label(egui::RichText::new("Enter or Tab to link  ·  Esc to hide").size(11.0).color(v.weak_text_color()));
        });
    });
    picked
}

/// A flat toolbar button for showing or hiding a sidebar. It looks the same open or closed;
/// the sidebar itself shows which it is.
fn toggle_button(ui: &mut egui::Ui, icon: &Option<egui::TextureHandle>, fallback: &str, tip: &str, enabled: bool) -> egui::Response {
    let fg = ui.visuals().text_color();
    let b = match icon {
        Some(t) => egui::Button::image(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(16.0)).tint(fg)),
        None => egui::Button::new(fallback),
    };
    ui.add_enabled(enabled, b.frame_when_inactive(false).min_size(Vec2::new(34.0, 32.0)).corner_radius(r_sm())).on_hover_text(tip)
}

/// What's being dragged, under the pointer.
fn drag_label(ctx: &egui::Context) {
    let Some(drag) = egui::DragAndDrop::payload::<ItemDrag>(ctx) else { return };
    let Some(pos) = ctx.pointer_hover_pos() else { return };
    let v = ctx.global_style().visuals.clone();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("note-drag")));
    let g = painter.layout_no_wrap(drag.name.clone(), fonts::ui_font(12.5), v.selection.stroke.color);
    let r = Rect::from_min_size(pos + Vec2::new(14.0, 10.0), g.size() + Vec2::new(16.0, 8.0));
    painter.rect_filled(r, r_sm(), v.selection.bg_fill);
    painter.galley(r.min + Vec2::new(8.0, 4.0), g, v.selection.stroke.color);
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
}

/// What the sidebar's panels draw from.
struct SideData {
    notes: Vec<Note>,
    folders: Vec<String>,
    tags: Vec<(String, usize)>,
    collapsed: BTreeSet<String>,
    open: Option<PathBuf>,
    root: PathBuf,
    current: String,
    folder_icon: Option<egui::TextureHandle>,
    note_icon: Option<egui::TextureHandle>,
    tag_icon: Option<egui::TextureHandle>,
}

/// One panel's contents: the folder tree, or the tags with their notes.
fn panel(ui: &mut egui::Ui, kind: Browse, data: &SideData, pick: &mut Option<Pick>) {
    // Right-click empty space: this panel's menu.
    let bg = ui.interact(ui.max_rect(), egui::Id::unique(("notes-side-bg", kind == Browse::Tags)), Sense::click());
    let root = &data.root;
    bg.context_menu(|ui| {
        menu_style(ui);
        if item(ui, "New Note…", "Ctrl+N") {
            *pick = Some(Pick::NewNote(String::new()));
        }
        if item(ui, "Create New Folder…", "Shift+Ctrl+N") {
            *pick = Some(Pick::NewFolder(String::new()));
        }
        ui.separator();
        if item(ui, "Collapse All", "") {
            *pick = Some(Pick::FoldAll(true, kind));
        }
        if item(ui, "Expand All", "") {
            *pick = Some(Pick::FoldAll(false, kind));
        }
        ui.separator();
        if item(ui, "Reload Vault", "F5") {
            *pick = Some(Pick::Reload);
        }
        if item(ui, "Open in File Manager", "") {
            *pick = Some(Pick::ShowInFiles(root.clone()));
        }
    });
    let open = data.open.as_ref();
    match kind {
        Browse::Folders => {
            folder_tree(ui, "", 0, &data.folders, &data.notes, &data.collapsed, open, &data.folder_icon, &data.note_icon, pick, &data.current, root);
            // Space below the tree: drop here to move out to the top of the vault.
            let rest = ui.available_rect_before_wrap();
            let rest = Rect::from_min_max(rest.min, Pos2::new(rest.max.x, rest.max.y.max(rest.min.y + 40.0)));
            if let Some(drag) = egui::DragAndDrop::payload::<ItemDrag>(ui.ctx())
                && pick.is_none()
                && drag.path.parent() != Some(root.as_path())
                && ui.ctx().pointer_hover_pos().is_some_and(|p| rest.contains(p))
            {
                let v = ui.visuals();
                let r = rest.shrink2(Vec2::new(0.0, 2.0));
                ui.painter().rect_stroke(r, r_sm(), Stroke::new(1.5, v.selection.bg_fill), egui::StrokeKind::Inside);
                ui.painter().text(r.center_top() + Vec2::new(0.0, 20.0), Align2::CENTER_CENTER, "Move to the top of the vault", fonts::ui_font(12.5), v.selection.bg_fill);
                if ui.input(|i| i.pointer.any_released()) {
                    *pick = Some(Pick::MoveTo(drag.path.clone(), String::new()));
                }
            }
            ui.allocate_space(Vec2::new(ui.available_width(), 40.0));
        }
        Browse::Tags => {
            if data.tags.is_empty() {
                ui.add_space(6.0);
                ui.label(egui::RichText::new("No tags yet. Add them in a note's properties, or write #tag in its text.").small().weak());
            }
            let key = |t: &str| format!("#{}", t.to_lowercase());
            for (t, count) in &data.tags {
                let folded = data.collapsed.contains(&key(t));
                let h = group_header(ui, &format!("#{t}"), *count, !folded, &data.tag_icon);
                if h.clicked() {
                    *pick = Some(Pick::Fold(key(t)));
                }
                h.context_menu(|ui| {
                    menu_style(ui);
                    if item(ui, if folded { "Expand" } else { "Collapse" }, "") {
                        *pick = Some(Pick::Fold(key(t)));
                    }
                    if item(ui, &format!("Show Only #{t}"), "") {
                        *pick = Some(Pick::OnlyTag(t.clone()));
                    }
                    ui.separator();
                    if item(ui, "Collapse All", "") {
                        *pick = Some(Pick::FoldAll(true, kind));
                    }
                    if item(ui, "Expand All", "") {
                        *pick = Some(Pick::FoldAll(false, kind));
                    }
                });
                if !folded {
                    for n in data.notes.iter().filter(|n| n.tags.iter().any(|x| x.eq_ignore_ascii_case(t))) {
                        note_row(ui, n, 1, open == Some(&n.path), &data.note_icon, false, pick, root);
                    }
                }
            }
            let untagged: Vec<&Note> = data.notes.iter().filter(|n| n.tags.is_empty()).collect();
            if !untagged.is_empty() {
                let folded = data.collapsed.contains("#");
                if group_header(ui, "Untagged", untagged.len(), !folded, &None).clicked() {
                    *pick = Some(Pick::Fold("#".into()));
                }
                if !folded {
                    for n in untagged {
                        note_row(ui, n, 1, open == Some(&n.path), &data.note_icon, false, pick, root);
                    }
                }
            }
        }
    }
}

/// A plain title bar (search results).
fn plain_header(ui: &mut egui::Ui, title: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
    ui.painter().text(Pos2::new(rect.left() + 4.0, rect.center().y), Align2::LEFT_CENTER, title.to_uppercase(), fonts::ui_font(11.5), ui.visuals().weak_text_color());
}

/// A slot's title bar: its panel's name, or a tab for each of its panels. Titles drag (to move
/// the panel); a triangle folds the slot when `open` is given; ⋯ holds the layout choices.
/// Returns the bar's rectangle (a drop target).
fn slot_header(ui: &mut egui::Ui, index: usize, slot: &crate::layout::Slot, open: Option<bool>, stacked: bool, pick: &mut Option<Pick>) -> Rect {
    use crate::layout::{Layout, fold_key, title};
    let (rect, bar) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    let v = ui.visuals().clone();
    let weak = v.weak_text_color();
    let mut x = rect.left() + 4.0;
    if let Some(o) = open {
        let c = Pos2::new(x + 6.0, rect.center().y);
        let pts = if o { vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)] } else { vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, 4.0), c + Vec2::new(3.0, 0.0)] };
        ui.painter().add(egui::Shape::convex_polygon(pts, if bar.hovered() { v.strong_text_color() } else { weak }, Stroke::NONE));
        if bar.clicked() {
            *pick = Some(Pick::FoldSlot(fold_key(slot)));
        }
        x += 16.0;
    }
    let tabs = slot.panels.len() > 1;
    for (k, p) in slot.panels.iter().enumerate() {
        let on = k == slot.active;
        let text = title(*p).to_uppercase();
        let g = ui.painter().layout_no_wrap(text, fonts::ui_font(11.5), weak);
        let w = g.size().x + if tabs { 18.0 } else { 6.0 };
        let r = Rect::from_min_size(Pos2::new(x, rect.top() + 3.0), Vec2::new(w, rect.height() - 6.0));
        let resp = ui.interact(r, egui::Id::unique(("panel-title", index, k)), Sense::click_and_drag());
        let col = if on || !tabs { if resp.hovered() { v.strong_text_color() } else { v.text_color() } } else if resp.hovered() { v.text_color() } else { weak };
        if tabs && on {
            let (fill, stroke) = selected_tile(&v, v.panel_fill);
            ui.painter().rect_filled(r, r_sm(), fill);
            ui.painter().rect_stroke(r, r_sm(), stroke, egui::StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
        }
        let tx = if tabs { r.left() + 9.0 } else { r.left() + 3.0 };
        ui.painter().text(Pos2::new(tx, r.center().y), Align2::LEFT_CENTER, title(*p).to_uppercase(), fonts::ui_font(11.5), col);
        let resp = resp.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text(if tabs { "Click to show · drag to move this panel" } else { "Drag to move this panel" });
        if resp.clicked() && tabs {
            *pick = Some(Pick::ShowTab(*p));
        }
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), PanelDrag(*p));
        }
        if resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        x = r.right() + 4.0;
    }
    // The ⋯ menu (drawn dots), also on right-click.
    let shown = slot.shown();
    let dots = Rect::from_center_size(Pos2::new(rect.right() - 14.0, rect.center().y), Vec2::new(24.0, 22.0));
    let mut menu = |ui: &mut egui::Ui| {
        menu_style(ui);
        if ui.radio(!stacked, "Tabs").clicked() {
            *pick = Some(Pick::Preset(Layout::tabs(shown)));
            ui.close();
        }
        if ui.radio(stacked, "Stacked").clicked() {
            *pick = Some(Pick::Preset(Layout::stacked()));
            ui.close();
        }
        ui.separator();
        if item(ui, "Collapse All", "") {
            *pick = Some(Pick::FoldAll(true, shown));
        }
        if item(ui, "Expand All", "") {
            *pick = Some(Pick::FoldAll(false, shown));
        }
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(dots));
    let btn = egui::Button::new("").frame_when_inactive(false).min_size(dots.size()).corner_radius(r_sm());
    let (r, _) = egui::containers::menu::MenuButton::from_button(btn).ui(&mut child, &mut menu);
    for dx in [-5.0f32, 0.0, 5.0] {
        ui.painter().circle_filled(r.rect.center() + Vec2::new(dx, 0.0), 1.6, weak);
    }
    r.on_hover_text("Panel options");
    bar.context_menu(menu);
    rect
}

/// A folder (and what's in it) in the tree, then its sub-folders.
#[allow(clippy::too_many_arguments)]
fn folder_tree(
    ui: &mut egui::Ui,
    folder: &str,
    depth: usize,
    folders: &[String],
    notes: &[Note],
    collapsed: &BTreeSet<String>,
    open: Option<&PathBuf>,
    folder_icon: &Option<egui::TextureHandle>,
    note_icon: &Option<egui::TextureHandle>,
    pick: &mut Option<Pick>,
    current: &str,
    root: &Path,
) {
    let parent_of = |f: &str| f.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
    for sub in folders.iter().filter(|f| parent_of(f) == folder) {
        let name = sub.rsplit('/').next().unwrap_or(sub);
        let count = notes.iter().filter(|n| n.folder() == sub || n.folder().starts_with(&format!("{sub}/"))).count();
        let open_f = !collapsed.contains(sub);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click_and_drag());
        let here = root.join(sub);
        if drag_and_drop(ui, &resp, rect, &here, name, &here)
            && let Some(drag) = egui::DragAndDrop::payload::<ItemDrag>(ui.ctx())
        {
            *pick = Some(Pick::MoveTo(drag.path.clone(), sub.clone()));
        }
        let v = ui.visuals();
        if *current == **sub {
            let (fill, stroke) = selected_tile(v, v.panel_fill);
            ui.painter().rect_filled(rect, r_sm(), fill);
            ui.painter().rect_stroke(rect, r_sm(), stroke, egui::StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, r_sm(), v.widgets.hovered.weak_bg_fill);
        }
        let x = rect.left() + 6.0 + depth as f32 * 14.0;
        let c = Pos2::new(x + 4.0, rect.center().y);
        let col = v.text_color();
        let pts = if open_f { vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)] } else { vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, 4.0), c + Vec2::new(3.0, 0.0)] };
        ui.painter().add(egui::Shape::convex_polygon(pts, v.weak_text_color(), Stroke::NONE));
        if let Some(t) = folder_icon {
            paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(x + 20.0, rect.center().y), Vec2::splat(16.0)), Color32::WHITE);
        }
        let right = ui.painter().text(Pos2::new(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, count.to_string(), fonts::ui_font(11.5), v.weak_text_color());
        let text_rect = Rect::from_min_max(Pos2::new(x + 32.0, rect.top()), Pos2::new(right.left() - 6.0, rect.bottom()));
        ui.painter().with_clip_rect(text_rect).text(Pos2::new(text_rect.left(), rect.center().y), Align2::LEFT_CENTER, name, fonts::ui_font(13.5), col);
        // Clicking a folder makes it where new notes go; clicking it again (or its
        // triangle) folds or opens it.
        let on_triangle = ui.ctx().pointer_interact_pos().is_some_and(|p| p.x < x + 12.0);
        if resp.clicked() {
            *pick = Some(if *current == **sub || on_triangle { Pick::Fold(sub.clone()) } else { Pick::Folder(sub.clone()) });
        }
        resp.context_menu(|ui| folder_menu(ui, root, sub, open_f, pick));
        if open_f {
            // A faint guide down the left of the folder's contents, under their rows (so a
            // selected or hovered row covers it).
            let guide = ui.painter().add(egui::Shape::Noop);
            let top = rect.bottom();
            folder_tree(ui, sub, depth + 1, folders, notes, collapsed, open, folder_icon, note_icon, pick, current, root);
            let bottom = ui.min_rect().bottom();
            if bottom > top + 1.0 {
                let v = ui.visuals();
                let col = palette::mix(v.panel_fill, v.text_color(), if v.dark_mode { 0.16 } else { 0.20 });
                ui.painter().set(guide, egui::Shape::line_segment([Pos2::new(c.x, top + 2.0), Pos2::new(c.x, bottom - 4.0)], Stroke::new(1.0, col)));
            }
        }
    }
    for n in notes.iter().filter(|n| n.folder() == folder) {
        note_row(ui, n, depth, open == Some(&n.path), note_icon, false, pick, root);
    }
}

#[allow(clippy::too_many_arguments)]
fn note_row(ui: &mut egui::Ui, n: &Note, depth: usize, open: bool, icon: &Option<egui::TextureHandle>, show_folder: bool, pick: &mut Option<Pick>, root: &Path) {
    let h = if show_folder { 40.0 } else { ROW_H };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click_and_drag());
    let dir = n.path.parent().unwrap_or(root).to_path_buf();
    if drag_and_drop(ui, &resp, rect, &n.path, &n.title, &dir)
        && let Some(drag) = egui::DragAndDrop::payload::<ItemDrag>(ui.ctx())
    {
        *pick = Some(Pick::MoveTo(drag.path.clone(), n.folder().to_string()));
    }
    let v = ui.visuals();
    if open {
        ui.painter().rect_filled(rect, r_sm(), v.selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, r_sm(), v.widgets.hovered.weak_bg_fill);
    }
    let fg = if open { v.selection.stroke.color } else { v.text_color() };
    let x = rect.left() + 6.0 + depth as f32 * 14.0 + 12.0;
    if let Some(t) = icon {
        paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(x + 8.0, rect.center().y), Vec2::splat(16.0)), Color32::WHITE);
    }
    let text_rect = Rect::from_min_max(Pos2::new(x + 22.0, rect.top()), Pos2::new(rect.right() - 6.0, rect.bottom()));
    let mut job = egui::text::LayoutJob::simple_singleline(n.title.clone(), fonts::ui_font(13.5), fg);
    job.wrap = egui::text::TextWrapping::truncate_at_width(text_rect.width());
    let g = ui.painter().layout_job(job);
    let y = if show_folder { rect.top() + 12.0 } else { rect.center().y };
    ui.painter().galley(Pos2::new(text_rect.left(), y - g.size().y / 2.0), g, fg);
    if show_folder {
        let weak = if open { fg.gamma_multiply(0.8) } else { v.weak_text_color() };
        let f = if n.folder().is_empty() { "Top of the vault".to_string() } else { n.folder().to_string() };
        ui.painter().with_clip_rect(text_rect).text(Pos2::new(text_rect.left(), rect.top() + 29.0), Align2::LEFT_CENTER, f, fonts::ui_font(11.5), weak);
    }
    let resp = resp.on_hover_ui(|ui| {
        ui.label(egui::RichText::new(&n.title).strong());
        ui.label(egui::RichText::new(&n.rel).small().weak());
        if !n.tags.is_empty() {
            ui.label(n.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join("  "));
        }
        if !n.snippet.is_empty() {
            ui.label(egui::RichText::new(&n.snippet).weak());
        }
        if let Some(t) = n.modified {
            let t: chrono::DateTime<chrono::Local> = t.into();
            ui.label(egui::RichText::new(format!("Edited {}", t.format("%-d %b %Y, %H:%M"))).small().weak());
        }
    });
    if resp.clicked() {
        *pick = Some(Pick::Note(n.path.clone()));
    }
    resp.context_menu(|ui| note_menu(ui, root, n, pick));
}

/// A tag's header: triangle, name, count (like Rivet's sidebar sections).
fn group_header(ui: &mut egui::Ui, name: &str, count: usize, open: bool, icon: &Option<egui::TextureHandle>) -> egui::Response {
    ui.add_space(4.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
    let v = ui.visuals();
    let col = if resp.hovered() { v.strong_text_color() } else { v.text_color() };
    let c = Pos2::new(rect.left() + 10.0, rect.center().y);
    let pts = if open { vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)] } else { vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, 4.0), c + Vec2::new(3.0, 0.0)] };
    ui.painter().add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
    let mut x = rect.left() + 22.0;
    if let Some(t) = icon {
        paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(x + 7.0, rect.center().y), Vec2::splat(14.0)), v.selection.bg_fill);
        x += 20.0;
    }
    let n = ui.painter().text(Pos2::new(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, count.to_string(), fonts::ui_font(11.5), v.weak_text_color());
    let tr = Rect::from_min_max(Pos2::new(x, rect.top()), Pos2::new(n.left() - 6.0, rect.bottom()));
    ui.painter().with_clip_rect(tr).text(Pos2::new(tr.left(), rect.center().y), Align2::LEFT_CENTER, name, fonts::ui_font(13.5), col);
    resp
}

#[cfg(test)]
mod tests {
    use super::code_ranges;

    #[test]
    fn finds_code() {
        let t = "Text with `a` and `b c`.\n```rust\nfn x() {}\n```\nAfter ` lone\n";
        let r = code_ranges(t);
        let parts: Vec<&str> = r.iter().map(|r| &t[r.clone()]).collect();
        assert_eq!(parts, ["`a`", "`b c`", "```rust\nfn x() {}\n```\n"]);
        // An unclosed fence runs to the end.
        assert_eq!(code_ranges("a\n```\nb").first(), Some(&(2..7)));
    }
}

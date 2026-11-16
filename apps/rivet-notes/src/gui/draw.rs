//! Drawing: the toolbar, the sidebar (folders or tags), the note (properties and text), the
//! welcome screen, the small questions and Preferences.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{ACCENTS, Mode, paint_tex, r_lg, r_md, r_sm, r_xs, selected_tile, status};
use rivet_look::{builtin_icons, fonts, highlight, palette, windows};

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
        egui::Panel::bottom("status").show_separator_line(false).frame(st).show(ui, |ui| self.status_line(ui));
        if self.s.show_sidebar && self.vault.is_some() {
            let side = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 10, right: 8, top: 0, bottom: 4 });
            egui::Panel::left("notes-side").show_separator_line(false).frame(side).resizable(true).default_size(260.0).size_range(190.0..=460.0).show(ui, |ui| self.sidebar(ui));
        }
        let left = if self.s.show_sidebar && self.vault.is_some() { 0 } else { 12 };
        let main = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left, right: 12, top: 0, bottom: 0 });
        egui::CentralPanel::default().frame(main).show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().rect_filled(r, r_lg(), v.extreme_bg_color);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(Vec2::new(28.0, 18.0))).layout(egui::Layout::top_down(egui::Align::Min)));
            if self.vault.is_none() {
                self.welcome(&mut child);
            } else if self.doc.is_some() {
                self.editor(&mut child);
            } else {
                let c = r.center();
                let weak = v.weak_text_color();
                child.painter().text(c - Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, "Pick a note on the left", fonts::ui_font(15.0), v.text_color());
                child.painter().text(c + Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, "or make a new one with Ctrl+N", fonts::ui_font(12.5), weak);
            }
        });
        self.reading_view(ctx);
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
                    left_end = r.0.rect.right();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // New note, and the sidebar's tabs.
                    let new_icon = self.icon(&ctx, &["document-new-symbolic", "list-add-symbolic"], 16.0);
                    let b = match &new_icon {
                        Some(t) => egui::Button::image(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(16.0)).tint(v.text_color())),
                        None => egui::Button::new("New"),
                    };
                    if ui.add_enabled(self.vault.is_some(), b.frame_when_inactive(false).min_size(Vec2::new(34.0, 32.0)).corner_radius(r_sm())).on_hover_text("New note (Ctrl+N)").clicked() {
                        self.prompt = Some(Prompt::NewNote { folder: self.current_folder.clone(), title: String::new() });
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
                    if let Some(i) = self.s.layout.tabbed() {
                        ui.add_space(4.0);
                        let slot = self.s.layout.slots[i].clone();
                        for (k, p) in slot.panels.iter().enumerate().rev() {
                            if ui.selectable_label(k == slot.active, crate::layout::title(*p)).on_hover_text("Ctrl+T switches").clicked() {
                                self.s.layout.show(*p);
                                self.s.save();
                            }
                        }
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
        ui.horizontal(|ui| {
            let n = self.notes.len();
            if let Some(path) = &self.vault {
                ui.label(egui::RichText::new(self.vault_name()).color(v.text_color())).on_hover_text(format!("{}\nSwitch vaults from File → Switch Vault", path.display()));
                ui.label(egui::RichText::new(format!("·  {n} note{}", if n == 1 { "" } else { "s" })).color(v.weak_text_color()));
            }
            if let Some((m, bad)) = &self.note_msg {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(m).color(if *bad { status(&ctx).danger } else { v.weak_text_color() }));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(d) = &self.doc {
                    let words = d.body.split_whitespace().count();
                    let state = if d.dirty { "Saving…".to_string() } else { d.saved_at.as_ref().map(|t| format!("Saved at {t}")).unwrap_or_else(|| "Saved".into()) };
                    ui.label(egui::RichText::new(format!("{words} words  ·  {state}")).color(v.weak_text_color()));
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
        let now = ctx.input(|i| i.time);
        let mut edit: Option<EditCmd> = None;
        let Some(d) = self.doc.as_mut() else { return };
        // ---- heading: the title, where it is
        let title = d.fm.as_ref().and_then(|f| f.get("title")).map(|x| x.as_text()).filter(|t| !t.is_empty()).unwrap_or_else(|| d.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        ui.label(egui::RichText::new(&title).size(22.0).strong().color(v.strong_text_color()));
        let rel = d.path.strip_prefix(&vault).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        ui.label(egui::RichText::new(rel.replace('/', "  ›  ")).color(weak));
        ui.add_space(10.0);
        let mut changed = false;
        // ---- properties (the frontmatter)
        if show_props {
            if let Some(fm) = d.fm.as_mut() {
                egui::Frame::NONE.fill(v.faint_bg_color).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Properties").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if d.new_prop.is_none() && ui.small_button("Add").on_hover_text("Add a property").clicked() {
                                d.new_prop = Some(String::new());
                            }
                        });
                    });
                    ui.add_space(4.0);
                    let mut remove: Option<usize> = None;
                    {
                        for (i, (k, val)) in fm.fields.iter_mut().enumerate() {
                          ui.horizontal(|ui| {
                            // A fixed column for names; the value takes the room up to the × button.
                            let (lr, _) = ui.allocate_exact_size(Vec2::new(110.0, 26.0), Sense::hover());
                            ui.painter().with_clip_rect(lr).text(lr.left_center(), Align2::LEFT_CENTER, k.as_str(), fonts::ui_font(13.0), weak);
                            let value_w = (ui.available_width() - 34.0).max(80.0);
                            let is_tags = k.eq_ignore_ascii_case("tags") || k.eq_ignore_ascii_case("tag");
                            match val {
                                Value::List(items) if is_tags => {
                                    ui.allocate_ui_with_layout(Vec2::new(value_w, 26.0), egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true), |ui| {
                                        ui.set_width(value_w);
                                        let mut drop: Option<usize> = None;
                                        for (j, t) in items.iter().enumerate() {
                                            let chip = egui::Button::new(egui::RichText::new(format!("#{t}  ×")).size(12.5)).fill(palette::mix(v.faint_bg_color, v.selection.bg_fill, 0.35)).corner_radius(r_xs());
                                            if ui.add(chip).on_hover_text("Remove this tag").clicked() {
                                                drop = Some(j);
                                            }
                                        }
                                        if let Some(j) = drop {
                                            items.remove(j);
                                            changed = true;
                                        }
                                        let r = ui.add(egui::TextEdit::singleline(&mut d.new_tag).hint_text("add tag").desired_width(90.0));
                                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                            for t in d.new_tag.split([',', ' ']).map(|t| t.trim().trim_start_matches('#')).filter(|t| !t.is_empty()) {
                                                if !items.iter().any(|x| x.eq_ignore_ascii_case(t)) {
                                                    items.push(t.to_string());
                                                }
                                            }
                                            d.new_tag.clear();
                                            changed = true;
                                            r.request_focus();
                                        }
                                    });
                                }
                                Value::List(items) => {
                                    let mut text = items.join(", ");
                                    if ui.add(egui::TextEdit::singleline(&mut text).desired_width(value_w)).changed() {
                                        *items = text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                                        changed = true;
                                    }
                                }
                                Value::Text(t) => {
                                    if ui.add(egui::TextEdit::singleline(t).desired_width(value_w)).changed() {
                                        changed = true;
                                    }
                                }
                                Value::Raw(r) => {
                                    ui.set_max_width(value_w + 110.0);
                                    ui.add(egui::Label::new(egui::RichText::new(r.trim()).monospace().size(12.0).color(weak)).wrap()).on_hover_text("Kept as written; edit it in the file's text");
                                }
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("×").on_hover_text("Remove this property").clicked() {
                                    remove = Some(i);
                                }
                            });
                          });
                        }
                    }
                    if let Some(i) = remove {
                        fm.fields.remove(i);
                        changed = true;
                    }
                    if let Some(mut name) = d.new_prop.take() {
                        let mut done = false;
                        ui.horizontal(|ui| {
                            let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text("Property name").desired_width(160.0));
                            r.request_focus();
                            if r.lost_focus() {
                                let n = name.trim().replace(':', "");
                                if ui.input(|i| i.key_pressed(Key::Enter)) && !n.is_empty() && fm.get(&n).is_none() {
                                    let val = if n.eq_ignore_ascii_case("tags") { Value::List(Vec::new()) } else { Value::Text(String::new()) };
                                    fm.fields.push((n, val));
                                    changed = true;
                                }
                                done = true;
                            }
                        });
                        if !done {
                            d.new_prop = Some(name);
                        }
                    }
                });
                ui.add_space(12.0);
            } else if ui.small_button("Add Properties").on_hover_text("Start a frontmatter block for this note").clicked() {
                let mut fm = Frontmatter::default();
                fm.set("tags", Value::List(Vec::new()));
                d.fm = Some(fm);
                changed = true;
            }
        }
        // ---- the text, colored like Markdown
        // The editor's own font; code (fenced blocks and `inline`) in the monospace one.
        let font = egui::FontId::new(editor_size * fonts::scale(), rivet_look::live::editor_family());
        let mono = egui::FontId::monospace(editor_size * 0.92 * fonts::scale());
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            let mut job = highlight::layout(buf.as_str(), "note.md", &colors, font.clone());
            mono_code(&mut job, buf.as_str(), &mono);
            job.wrap.max_width = wrap;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        egui::ScrollArea::vertical().id_salt("note-text").auto_shrink([false, false]).show(ui, |ui| {
            let r = ui.add(
                egui::TextEdit::multiline(&mut d.body)
                    .id(egui::Id::unique("note-body"))
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY)
                    .desired_rows(24)
                    .lock_focus(true)
                    .hint_text("Write in Markdown…")
                    .layouter(&mut layouter),
            );
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
            });
        });
        if changed {
            d.dirty = true;
            d.edited_at = now;
        }
        if let Some(c) = edit {
            self.edit_cmd = Some(c);
            ctx.memory_mut(|m| m.request_focus(egui::Id::unique("note-body")));
            ctx.request_repaint();
        }
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
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(22)).corner_radius(r_lg());
        egui::Modal::new(egui::Id::unique("notes-prompt")).frame(frame).show(ctx, |ui| {
            ui.set_width(400.0);
            let v = ui.visuals().clone();
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            let field = |ui: &mut egui::Ui, text: &mut String, hint: &str| -> bool {
                let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY));
                if !r.has_focus() && !r.lost_focus() {
                    r.request_focus();
                }
                r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
            };
            let buttons = |ui: &mut egui::Ui, ok: &str| -> (bool, bool) {
                let (mut a, mut b) = (false, false);
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().button_padding = Vec2::new(14.0, 6.0);
                    a = ui.add(egui::Button::new(egui::RichText::new(ok).color(v.selection.stroke.color)).fill(v.selection.bg_fill).corner_radius(r_sm())).clicked();
                    b = ui.add(egui::Button::new("Cancel").corner_radius(r_sm())).clicked();
                });
                (a, b)
            };
            let place = |f: &str| if f.is_empty() { "the top of the vault".to_string() } else { format!("\"{f}\"") };
            match prompt {
                Prompt::NewNote { folder, mut title } => {
                    ui.heading("New Note");
                    ui.label(egui::RichText::new(format!("In {}", place(&folder))).color(v.weak_text_color()));
                    ui.add_space(10.0);
                    let enter = field(ui, &mut title, "Title");
                    let (ok, cancel) = buttons(ui, "Create");
                    if (ok || enter) && !title.trim().is_empty() {
                        self.new_note(&folder, &title);
                    } else if !(cancel || esc) {
                        next = Some(Prompt::NewNote { folder, title });
                    }
                }
                Prompt::NewFolder { parent, mut name } => {
                    ui.heading("New Folder");
                    ui.label(egui::RichText::new(format!("In {}", place(&parent))).color(v.weak_text_color()));
                    ui.add_space(10.0);
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

    // ---------------------------------------------------------------- Preferences

    fn prefs_window(&mut self, ctx: &egui::Context) {
        let chrome = windows::Chrome { title: "Preferences".into(), icon: self.icon(ctx, &["preferences-system-symbolic", "emblem-system-symbolic"], 16.0), close: self.icon(ctx, &["window-close-symbolic"], 14.0), flush: false };
        let area = ctx.content_rect().shrink(6.0);
        let about_icon = self.app_icon(ctx);
        let mut win = std::mem::replace(&mut self.prefs_win, windows::WinState::new(0, Vec2::ZERO));
        let before = self.s.clone();
        let pal = self.look.palette().cloned();
        let mut open_vault: Option<PathBuf> = None;
        let mut forget: Option<PathBuf> = None;
        let about = self.about_open;
        let shown = windows::show(ctx, &mut win, &chrome, area, Vec2::new(560.0, 380.0), |ui| {
            let v = ui.visuals().clone();
            let weak = v.weak_text_color();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Frame::NONE.inner_margin(egui::Margin { left: 18, right: 18, top: 10, bottom: 18 }).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
                    let card = v.faint_bg_color;
                    let section = |ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                        ui.label(egui::RichText::new(title).strong().size(14.5));
                        ui.add_space(4.0);
                        egui::Frame::NONE.fill(card).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(18, 16)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            body(ui);
                        });
                        ui.add_space(20.0);
                    };
                    if about {
                        ui.vertical_centered(|ui| {
                            if let Some(t) = &about_icon {
                                ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(88.0)));
                            }
                            ui.label(egui::RichText::new("Rivet Notes").size(22.0).strong());
                            ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(weak));
                            ui.add_space(6.0);
                            ui.label("Markdown notes in plain folders, with frontmatter and tags.");
                            ui.label(egui::RichText::new("Built on Rivet Files. No GTK or Qt.").color(weak));
                        });
                        ui.add_space(16.0);
                    }
                    section(ui, "Appearance", &mut |ui| {
                        ui.checkbox(&mut self.s.follow_desktop_colors, "Follow desktop colors");
                        let manual = !self.s.follow_desktop_colors || pal.is_none();
                        ui.add_space(6.0);
                        egui::Grid::new("notes-look").num_columns(2).spacing([24.0, 16.0]).min_col_width(110.0).show(ui, |ui| {
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
                            ui.label("Icons");
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut self.s.builtin_icons, true, "Rivet's own").on_hover_text("Rivet's icon kit, colored from your theme");
                                ui.selectable_value(&mut self.s.builtin_icons, false, "Desktop icon theme");
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
                    let vaults = self.s.vaults.clone();
                    let current = self.vault.clone();
                    let (fams, monos) = self.families.get_or_insert_with(|| (fonts::families(), fonts::mono_families())).clone();
                    let desktop = fonts::desktop_font_cached().map(|(f, _)| f).unwrap_or_else(|| "Sans".into());
                    let system_mono = fonts::system_mono_name();
                    section(ui, "Fonts", &mut |ui| {
                        egui::Grid::new("notes-fonts").num_columns(2).spacing([24.0, 14.0]).min_col_width(110.0).show(ui, |ui| {
                            ui.label("Interface");
                            font_pick(ui, "f-ui", &mut self.s.ui_font, &format!("Like the desktop ({desktop})"), &fams);
                            ui.end_row();
                            ui.label("Editor");
                            font_pick(ui, "f-ed", &mut self.s.editor_font, "Same as the interface", &fams);
                            ui.end_row();
                            ui.label("Monospace");
                            font_pick(ui, "f-mono", &mut self.s.mono_font, &format!("System ({system_mono})"), &monos);
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
                        ui.add_space(10.0);
                        // A sample in the editor's fonts.
                        egui::Frame::NONE.fill(v.extreme_bg_color).corner_radius(r_sm()).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let size = self.s.editor_size * fonts::scale();
                            ui.label(egui::RichText::new("Notes are written in this font.").font(egui::FontId::new(size, rivet_look::live::editor_family())));
                            ui.label(egui::RichText::new("let code = \"in this one\";").font(egui::FontId::monospace(size * 0.92)).color(weak));
                        });
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("Monospace is for code blocks, `inline code` and other embedded text.").small().color(weak));
                    });
                    section(ui, "Sidebar", &mut |ui| {
                        use crate::layout::Layout;
                        let stacked = self.s.layout.is_stacked();
                        let shown = self.s.layout.slots[0].shown();
                        if ui.radio(!stacked, "Folders and tags as tabs (switch with Ctrl+T)").clicked() {
                            self.s.layout = Layout { split: self.s.layout.split, ..Layout::tabs(shown) };
                        }
                        if ui.radio(stacked, "Stacked: each in its own pane").clicked() && !stacked {
                            self.s.layout = Layout { split: self.s.layout.split, ..Layout::stacked() };
                        }
                        ui.label(egui::RichText::new("You can also drag a panel's title in the sidebar: onto another panel to make tabs, or to its top or bottom edge to stack them.").small().color(weak));
                    });
                    section(ui, "Vaults", &mut |ui| {
                        if vaults.is_empty() {
                            ui.label(egui::RichText::new("None yet.").color(weak));
                        }
                        for p in &vaults {
                            ui.horizontal(|ui| {
                                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 1.0;
                                    ui.label(egui::RichText::new(name).strong());
                                    ui.label(egui::RichText::new(p.display().to_string()).small().color(weak));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Notes are plain Markdown files; properties are the frontmatter at the top of each one. Folders and notes starting with a dot (like .git) are left out.").small().color(weak));
                    });
                });
            });
        });
        self.prefs_win = win;
        if !shown.open {
            self.prefs_open = false;
            self.about_open = false;
        }
        if let Some(p) = forget {
            self.s.vaults.retain(|x| *x != p);
        }
        if let Some(p) = open_vault {
            self.open_vault(p);
        }
        if self.s != before {
            self.s.save();
        }
    }
}

/// A font family picker: the default entry, then the installed families.
fn font_pick(ui: &mut egui::Ui, id: &str, value: &mut String, default_label: &str, families: &[String]) {
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
        ui.label(egui::RichText::new("Panels").small().weak());
        if ui.radio(!stacked, "As tabs").clicked() {
            *pick = Some(Pick::Preset(Layout::tabs(shown)));
            ui.close();
        }
        if ui.radio(stacked, "Stacked: tags below folders").clicked() {
            *pick = Some(Pick::Preset(Layout::stacked()));
            ui.close();
        }
        ui.label(egui::RichText::new("Or drag a panel's title onto another panel").small().weak());
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
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
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
            folder_tree(ui, sub, depth + 1, folders, notes, collapsed, open, folder_icon, note_icon, pick, current, root);
        }
    }
    for n in notes.iter().filter(|n| n.folder() == folder) {
        note_row(ui, n, depth, open == Some(&n.path), note_icon, false, pick, root);
    }
}

#[allow(clippy::too_many_arguments)]
fn note_row(ui: &mut egui::Ui, n: &Note, depth: usize, open: bool, icon: &Option<egui::TextureHandle>, show_folder: bool, pick: &mut Option<Pick>, root: &Path) {
    let h = if show_folder { 40.0 } else { ROW_H };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
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

//! The file area: icon, list and compact views (columns are in columns.rs), rubber-band selection and context menus.

use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::path::PathBuf;
use std::sync::Arc;

use super::{r_lg, r_sm, r_xs, accent_color, fmt_time, paint_tex, Action, DragFiles, RivetApp};
use crate::fsops;
use crate::launch;
use crate::model::{Entry, Location};
use rivet_look::a11y::{Access, Role};
use crate::settings::{SortKey, ViewMode};

/// What the item menu needs to know about the selection, worked out once per selection rather
/// than every frame the menu is open.
pub(super) struct MenuSel {
    paths: Vec<PathBuf>,
    archives: Vec<PathBuf>,
    /// Whether every selected item is a picture, a video or audio (for the media tools).
    media: Option<crate::preview::Kind>,
}

/// The one selected item drawn on top with its full name: (cell, icon, icon rect, tile fill,
/// name, name position, name color, cut, entry).
type Expanded = (Rect, Option<egui::TextureHandle>, Rect, Color32, Arc<egui::Galley>, Pos2, Color32, bool, Entry);

impl RivetApp {
    /// A file as a screen reader reads it: its name, whether it's a folder, and whether it's
    /// selected. The one under the keyboard is the file list's active descendant, so moving
    /// with the arrows reads each file reached.
    pub(super) fn item_access(&self, resp: egui::Response, e: &Entry, selected: bool, vi: usize) -> egui::Response {
        let resp = resp.role(Role::ListBoxOption, e.name.as_str()).selected(selected);
        let resp = if e.is_dir { resp.described("Folder") } else { resp };
        if self.tab().cursor == Some(vi)
            && let Some(list) = self.file_list_focus
        {
            resp.ctx.accesskit_node_builder(list, |n| n.set_active_descendant(resp.id.accesskit_id()));
        }
        resp
    }

    pub(super) fn file_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> usize {
        let bg = ctx.global_style().visuals.extreme_bg_color;
        let mut cols = 1;
        // The file area is the lowest surface; it sits inside the chrome surface with a
        // rounded corner where the toolbar and the sidebar meet (no border lines).
        let chrome = ctx.global_style().visuals.panel_fill;
        // Round the bottom corners where a bar (status bar or toolbar) sits below the files.
        let round_bottom = (self.s.show_statusbar && !self.s.statusbar_top) || self.s.toolbar_bottom;
        // Round the right-hand corners too when a panel (sidebar or Peek) sits on the right.
        let right_panel = (self.s.show_sidebar && self.s.sidebar_right) || (self.s.show_peek && !self.s.peek_left);
        let r = if right_panel { r_lg() } else { 0 };
        let mut corner = egui::CornerRadius { nw: r_lg(), ne: r, sw: if round_bottom { r_lg() } else { 0 }, se: if round_bottom { r } else { 0 } };
        // Zen mode: the files sit on one rounded card with an empty border all round, which is
        // where the pointer goes to slide the hidden bars in.
        let mut outer = egui::Frame::NONE.fill(chrome);
        if self.s.zen {
            corner = egui::CornerRadius::same(r_lg());
            outer = outer.inner_margin(egui::Margin::same(super::zen::ZEN_GUTTER));
        }
        // Terminal mode: the files sit in a box titled with where you are.
        let term = self.s.terminal;
        let mut inner = egui::Frame::NONE.fill(bg).corner_radius(corner).inner_margin(egui::Margin { left: 6, top: 6, right: 0, bottom: 4 });
        if term {
            if !self.s.zen {
                outer = outer.inner_margin(egui::Margin { left: 3, right: 3, top: 9, bottom: 5 });
            }
            inner = inner.stroke(super::tui_line(&ctx.global_style().visuals)).inner_margin(egui::Margin { left: 6, top: 10, right: 1, bottom: 4 });
        }
        let mut box_rect = None;
        let split = self.split.is_some() && !crate::picker::active();
        let central = egui::CentralPanel::no_frame().frame(outer).show(ui, |ui| {
            if split {
                cols = self.split_panes(ui, ctx, inner, term);
                return;
            }
            let r = inner.show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            cols = self.pane_contents(ui, ctx, true);
            });
            box_rect = Some(r.response.rect);
        });
        if let (true, Some(b)) = (term, box_rect) {
            let title = match &self.tab().loc {
                crate::model::Location::Dir(p) => {
                    let home = crate::places::home();
                    match p.strip_prefix(&home) {
                        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
                        Ok(rest) => format!("~/{}", rest.display()),
                        Err(_) => p.display().to_string(),
                    }
                }
                other => other.title(),
            };
            super::tui_title(ui, b, &title);
        }
        // Remembered so the Filters panel can drop down over the files.
        self.files_rect = Some(central.response.rect);
        cols
    }

    /// One pane of files (the whole file area, or one side of split view): the strip, the
    /// Trash and empty notes, and the chosen view. `active` = this pane has the keyboard.
    pub(super) fn pane_contents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, active: bool) -> usize {
        let mut cols = 1;
        ui.set_min_size(ui.available_size());
        // Keyboard: the whole file list is one Tab stop (files themselves aren't), and while
        // it has focus the arrow keys move through the files instead of between widgets.
        // (In split view only the active pane takes the keyboard.)
        if active {
            let anchor = ui.interact(ui.max_rect(), egui::Id::unique("file-list-focus"), Sense::FOCUSABLE);
            let anchor = anchor.role(Role::ListBox, "Files");
            rivet_look::a11y::ring_inside(ui.ctx(), anchor.id);
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(anchor.id, egui::EventFilter { tab: false, horizontal_arrows: true, vertical_arrows: true, escape: false })
            });
            self.file_list_focus = Some(anchor.id);
        }
        self.filter_strip(ui);
        let loc = self.tab().loc.clone();
        if matches!(loc, Location::Trash) {
            egui::Frame::NONE.fill(ui.visuals().faint_bg_color).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Trash").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!self.tab().entries.is_empty(), egui::Button::new("Empty Trash")).clicked() {
                            self.pending.push(Action::EmptyTrash);
                        }
                        let any = !self.tab().selected.is_empty();
                        if ui.add_enabled(any, egui::Button::new("Restore Selected Items")).clicked() {
                            self.pending.push(Action::Restore);
                        }
                    });
                });
            });
        }
        if matches!(loc, Location::Network) {
            self.network_page(ui, ctx);
            return cols;
        }
        if let Some(err) = self.tab().error.clone() {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new(err).size(15.0).color(ui.visuals().weak_text_color()));
            });
            return cols;
        }
        // Column view draws its own empty folder (the parents' columns still show).
        if self.columns_shown() {
            self.columns_view(ui, ctx);
            return cols;
        }
        if self.tab().visible.is_empty() {
            let msg = match &loc {
                Location::Trash => "Trash is empty",
                Location::Network => "No network locations are mounted",
                Location::Search { .. } if self.tab().searching => "Searching…",
                Location::Search { .. } => "No matching files",
                _ if !self.tab().filter.is_empty() => "No items match the filter",
                _ => "Folder is empty",
            };
            let rect = ui.available_rect_before_wrap();
            let resp = ui.allocate_rect(rect, Sense::click());
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, msg, crate::fonts::ui_font(15.0), ui.visuals().weak_text_color());
            resp.context_menu(|ui| self.background_menu(ui));
            if let Some(p) = resp.dnd_release_payload::<DragFiles>()
                && let Some(d) = self.current_dir() {
                    self.pending.push(Action::Transfer(p.0.clone(), d, ctx.input(|i| i.modifiers.ctrl)));
                }
            return cols;
        }
        cols = match self.s.view {
            ViewMode::Icons => self.icon_view(ui, ctx),
            ViewMode::List => {
                self.list_view(ui, ctx);
                1
            }
            ViewMode::Compact => self.compact_view(ui, ctx),
            // Trash, Recent and searches aren't a folder with parents: they use the list.
            ViewMode::Columns => {
                self.list_view(ui, ctx);
                1
            }
        };
        cols
    }

    /// Background: click to clear, drag for rubber-band selection, context menu, drop target.
    pub(super) fn background(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, rect: Rect, resp: &egui::Response, item_rects: &[(usize, Rect)]) {
        let mods = ctx.input(|i| i.modifiers);
        if resp.clicked() && !(mods.command || mods.shift) {
            self.tab_mut().selected.clear();
        }
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = resp.interact_pointer_pos() {
                self.tab_mut().rubber = Some((p - rect.min).to_pos2());
                if !(mods.command || mods.shift) {
                    self.tab_mut().selected.clear();
                }
            }
        if let (Some(start), true) = (self.tab().rubber, resp.dragged_by(egui::PointerButton::Primary))
            && let Some(cur) = ctx.input(|i| i.pointer.latest_pos()) {
                let a = rect.min + start.to_vec2();
                let band = Rect::from_two_pos(a, cur);
                let accent = accent_color(&self.s);
                ui.painter().rect_filled(band, r_xs(), accent.gamma_multiply(0.18));
                ui.painter().rect_stroke(band, r_xs(), Stroke::new(1.0, accent), StrokeKind::Inside);
                let t = &mut self.tabs[self.active];
                let sel: crate::model::Selection = item_rects.iter().filter(|(_, r)| r.intersects(band)).map(|(i, _)| t.entries[t.visible[*i]].path.clone()).collect();
                t.selected = sel;
                let clip = ui.clip_rect();
                if cur.y > clip.bottom() - 20.0 {
                    ui.scroll_with_delta(Vec2::new(0.0, -12.0));
                } else if cur.y < clip.top() + 20.0 {
                    ui.scroll_with_delta(Vec2::new(0.0, 12.0));
                }
            }
        if resp.drag_stopped() {
            self.tab_mut().rubber = None;
        }
        resp.context_menu(|ui| self.background_menu(ui));
        if let Some(p) = resp.dnd_release_payload::<DragFiles>()
            && let Some(d) = self.current_dir() {
                self.pending.push(Action::Transfer(p.0.clone(), d, ctx.input(|i| i.modifiers.ctrl)));
            }
    }

    fn name_galley(&self, ui: &egui::Ui, name: &str, width: f32, rows: usize, color: Color32) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::simple(name.to_string(), crate::fonts::ui_font(13.0), color, width);
        job.wrap.max_rows = rows;
        job.wrap.overflow_character = Some('…');
        job.halign = egui::Align::Center;
        ui.painter().layout_job(job)
    }

    pub(super) fn tint_for(&self, e: &Entry) -> Color32 {
        let cut = self.clipboard.as_ref().is_some_and(|(f, c)| *c && f.contains(&e.path));
        if cut || e.hidden { Color32::from_white_alpha(140) } else { Color32::WHITE }
    }

    pub(super) fn tooltip(&self, e: &Entry) -> String {
        let mut s = format!("{}\n{}", e.name, self.mime.describe(&e.mime));
        if e.is_dir {
            if let Some(c) = e.child_count {
                s.push_str(&format!(" — {c} item{}", if c == 1 { "" } else { "s" }));
            }
        } else {
            s.push_str(&format!(" — {}", fsops::human_size(e.size)));
        }
        if let Some((orig, date)) = &e.trash_origin {
            s.push_str(&format!("\nFrom: {}\nDeleted: {date}", orig.parent().map(|p| p.display().to_string()).unwrap_or_default()));
        }
        s
    }

    fn icon_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> usize {
        let z = self.s.zoom;
        let cell_w = (z + 44.0).max(96.0);
        let text_rows = 2;
        let cell_h = z + 16.0 + 16.0 * text_rows as f32;
        let mut cols = 1;
        let scroll_to = self.tab_mut().scroll_to.take();
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt(("iconview", self.active, self.pane_salt)).show(ui, |ui| {
            let width = ui.available_width();
            cols = ((width - 8.0) / cell_w).floor().max(1.0) as usize;
            let gap = ((width - 8.0) - cols as f32 * cell_w) / cols as f32;
            let n = self.tab().visible.len();
            let rows = n.div_ceil(cols);
            let total_h = (rows as f32 * cell_h + 16.0).max(ui.available_height());
            let (rect, bg_resp) = ui.allocate_exact_size(Vec2::new(width, total_h), Sense::CLICK | Sense::DRAG);
            let clip = ui.clip_rect();
            let item_rects: Vec<(usize, Rect)> = (0..n)
                .map(|vi| {
                    let (r, c) = (vi / cols, vi % cols);
                    let min = rect.min + Vec2::new(4.0 + c as f32 * (cell_w + gap) + gap / 2.0, 8.0 + r as f32 * cell_h);
                    (vi, Rect::from_min_size(min, Vec2::new(cell_w, cell_h - 4.0)))
                })
                .collect();
            self.background(ui, ctx, rect, &bg_resp, &item_rects);
            // One tile style for hover and selection: a rounded box around icon + name.
            // Only a single selected item grows to show its full name (drawn on top).
            let single = self.tab().selected.len() == 1;
            let mut expanded: Option<Expanded> = None;
            for (vi, item) in item_rects {
                let e = {
                    let t = self.tab();
                    &t.entries[t.visible[vi]]
                };
                if scroll_to.as_ref() == Some(&e.path) {
                    ui.scroll_to_rect(item, None);
                }
                if !item.intersects(clip) {
                    continue;
                }
                let e = e.clone();
                let selected = self.tab().selected.contains(&e.path);
                let resp = ui.interact(item, Id::unique(("item", self.pane_salt, &e.path)), Sense::CLICK | Sense::DRAG);
                let resp = self.item_access(resp, &e, selected, vi);
                let v = ui.visuals().clone();
                let icon_rect = Rect::from_center_size(Pos2::new(item.center().x, item.top() + 4.0 + z / 2.0), Vec2::splat(z));
                let text_pos = Pos2::new(item.center().x, icon_rect.bottom() + 6.0);
                let tile = item.shrink(1.0);
                let (sel_fill, sel_stroke) = super::selected_tile(&v, v.extreme_bg_color);
                let fg = v.strong_text_color();
                let tint = self.tint_for(&e);
                let tex = self.entry_texture(ctx, &e, z);
                if selected && single {
                    // Drawn after everything else so the full name can overlap the next row.
                    let galley = self.name_galley(ui, &e.name, cell_w - 8.0, usize::MAX, fg);
                    let bottom = (text_pos.y + galley.size().y + 6.0).max(tile.bottom());
                    expanded = Some((Rect::from_min_max(tile.min, Pos2::new(tile.right(), bottom)), tex, icon_rect, tint, galley, text_pos, fg, e.is_link, e.clone()));
                } else {
                    if selected {
                        ui.painter().rect_filled(tile, r_sm(), sel_fill);
                        ui.painter().rect_stroke(tile, r_sm(), sel_stroke, StrokeKind::Inside);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(tile, r_sm(), v.widgets.hovered.weak_bg_fill.gamma_multiply(0.8));
                    }
                    if let Some(t) = &tex {
                        let painter = ui.painter().clone();
                        self.icon_lift(ctx, &painter, &e, t, icon_rect, v.dark_mode);
                        paint_tex(ui.painter(), t, icon_rect, tint);
                    }
                    if e.is_link {
                        let er = Rect::from_min_size(icon_rect.right_bottom() - Vec2::splat(z * 0.35), Vec2::splat(z * 0.35));
                        if let Some(t) = self.icon(ctx, &["emblem-symbolic-link", "emblem-shared"], er.width()) {
                            paint_tex(ui.painter(), &t, er, Color32::WHITE);
                        }
                    }
                    let galley = self.name_galley(ui, &e.name, cell_w - 8.0, text_rows, if selected { fg } else { v.text_color() });
                    ui.painter().galley(text_pos, galley, fg);
                }
                self.drop_highlight(ui, item, &resp, e.is_dir);
                self.click_item(vi, ctx, &resp);
                self.dnd_item(ctx, vi, &resp);
                let resp = resp.on_hover_text_at_pointer(self.tooltip(&e));
                resp.context_menu(|ui| self.item_menu(ui, &e));
            }
            if let Some((tile, tex, icon_rect, tint, galley, pos, fg, is_link, ent)) = expanded {
                let v = ui.visuals().clone();
                let p = ui.painter();
                // Opaque base first so neighbours underneath don't show through.
                p.rect_filled(tile, r_sm(), v.extreme_bg_color);
                let (fill, stroke) = super::selected_tile(&v, v.extreme_bg_color);
                p.rect_filled(tile, r_sm(), fill);
                p.rect_stroke(tile, r_sm(), stroke, StrokeKind::Inside);
                if let Some(t) = &tex {
                    let painter = p.clone();
                    self.icon_lift(ctx, &painter, &ent, t, icon_rect, v.dark_mode);
                    paint_tex(&painter, t, icon_rect, tint);
                }
                if is_link {
                    let z = icon_rect.width();
                    let er = Rect::from_min_size(icon_rect.right_bottom() - Vec2::splat(z * 0.35), Vec2::splat(z * 0.35));
                    if let Some(t) = self.icon(ctx, &["emblem-symbolic-link", "emblem-shared"], er.width()) {
                        paint_tex(ui.painter(), &t, er, Color32::WHITE);
                    }
                }
                ui.painter().galley(pos, galley, fg);
            }
        });
        cols
    }

    fn list_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let z = self.s.list_zoom;
        let row_h = (z + 6.0).max(24.0);
        let header_h = 26.0;
        let in_trash = matches!(self.tab().loc, Location::Trash);
        let in_search = matches!(self.tab().loc, Location::Search { .. } | Location::Recent);
        let full_w = ui.available_width();
        // Size, Type, Date and Location share what Name's 160 points leave. In a narrow pane
        // (a small window, or Peek open) they narrow down to 60% and their text ends in an
        // ellipsis; past that the least needed are left out (Type, Location, then Size), so
        // the ones shown stay readable and none runs off the edge.
        let mut widths: [f32; 4] = [90.0, 150.0, 150.0, if in_trash || in_search { 220.0 } else { 0.0 }];
        let room = (full_w - 16.0 - 160.0).max(0.0);
        for i in [1, 3, 0, 2] {
            if widths.iter().sum::<f32>() * 0.6 <= room {
                break;
            }
            widths[i] = 0.0;
        }
        let fit = (room / widths.iter().sum::<f32>().max(1.0)).min(1.0);
        let [size_w, type_w, date_w, extra_w] = widths.map(|w| w * fit);
        let name_w = (full_w - 16.0 - size_w - type_w - date_w - extra_w).max(0.0);
        let mut columns: Vec<(&str, Option<SortKey>, f32)> = vec![("Name", Some(SortKey::Name), name_w), ("Size", Some(SortKey::Size), size_w), ("Type", Some(SortKey::Type), type_w)];
        if in_trash {
            columns.push(("Original Location", None, extra_w));
            columns.push(("Deleted", None, date_w));
        } else {
            if in_search {
                columns.push(("Location", None, extra_w));
            }
            columns.push(("Date Modified", Some(SortKey::Modified), date_w));
        }
        columns.retain(|c| c.2 > 0.0);

        let (hrect, _) = ui.allocate_exact_size(Vec2::new(full_w, header_h), Sense::hover());
        let v = ui.visuals().clone();
        ui.painter().rect_filled(hrect.shrink2(Vec2::new(4.0, 1.0)), r_sm(), v.faint_bg_color);
        let mut x = hrect.left() + 4.0;
        for (i, (label, key, w)) in columns.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(x, hrect.top()), Vec2::new(*w, header_h));
            let resp = ui.interact(r, Id::unique(("hdr", self.pane_salt, i)), Sense::click());
            if resp.hovered() && key.is_some() {
                ui.painter().rect_filled(r, 0.0, v.widgets.hovered.weak_bg_fill);
            }
            let sorted = *key == Some(self.s.sort);
            let mut job = egui::text::LayoutJob::simple_singleline(label.to_string(), crate::fonts::ui_font(12.5), v.strong_text_color());
            job.wrap = egui::text::TextWrapping::truncate_at_width((w - if sorted { 32.0 } else { 12.0 }).max(10.0));
            let g = ui.painter().layout_job(job);
            let tr = Rect::from_min_size(Pos2::new(r.left() + 8.0, r.center().y - g.size().y / 2.0), g.size());
            ui.painter().with_clip_rect(r.intersect(ui.clip_rect())).galley(tr.min, g, v.strong_text_color());
            if sorted {
                // Sort direction arrow, drawn so it never depends on the font having the glyph.
                let c = Pos2::new(tr.right() + 10.0, r.center().y);
                let pts = if self.s.sort_desc {
                    vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)]
                } else {
                    vec![c + Vec2::new(-4.0, 2.0), c + Vec2::new(4.0, 2.0), c + Vec2::new(0.0, -3.0)]
                };
                ui.painter().add(egui::Shape::convex_polygon(pts, v.strong_text_color(), Stroke::NONE));
            }
            if resp.clicked()
                && let Some(k) = key {
                    if self.s.sort == *k {
                        self.s.sort_desc = !self.s.sort_desc;
                    } else {
                        self.s.sort = *k;
                        self.s.sort_desc = false;
                    }
                    self.resort_all();
                }
            x += w;
        }

        let scroll_to = self.tab_mut().scroll_to.take();
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt(("listview", self.active, self.pane_salt)).show(ui, |ui| {
            let n = self.tab().visible.len();
            let width = ui.available_width();
            let total_h = (n as f32 * row_h + 8.0).max(ui.available_height());
            let (rect, bg_resp) = ui.allocate_exact_size(Vec2::new(width, total_h), Sense::CLICK | Sense::DRAG);
            let clip = ui.clip_rect();
            // Only the name column is "the item"; the rest of the row allows rubber-band selection.
            let item_rects: Vec<(usize, Rect)> = (0..n).map(|vi| (vi, Rect::from_min_size(rect.min + Vec2::new(4.0, vi as f32 * row_h), Vec2::new(name_w - 8.0, row_h)))).collect();
            self.background(ui, ctx, rect, &bg_resp, &item_rects);
            for (vi, hit) in item_rects {
                let row = Rect::from_min_size(rect.min + Vec2::new(0.0, vi as f32 * row_h), Vec2::new(width, row_h));
                let e = {
                    let t = self.tab();
                    &t.entries[t.visible[vi]]
                };
                if scroll_to.as_ref() == Some(&e.path) {
                    ui.scroll_to_rect(row, None);
                }
                if !row.intersects(clip) {
                    continue;
                }
                let e = e.clone();
                let selected = self.tab().selected.contains(&e.path);
                let resp = ui.interact(hit, Id::unique(("item", self.pane_salt, &e.path)), Sense::CLICK | Sense::DRAG);
                let resp = self.item_access(resp, &e, selected, vi);
                // Rows are inset and rounded like every other item.
                let pill = row.shrink2(Vec2::new(4.0, 0.0));
                if selected {
                    ui.painter().rect_filled(pill, r_sm(), v.selection.bg_fill);
                } else if resp.hovered() {
                    ui.painter().rect_filled(pill, r_sm(), v.widgets.hovered.weak_bg_fill);
                } else if vi % 2 == 1 {
                    ui.painter().rect_filled(pill, r_sm(), v.faint_bg_color);
                }
                self.drop_highlight(ui, row, &resp, e.is_dir);
                let fg = if selected { v.selection.stroke.color } else { v.text_color() };
                let weak = if selected { v.selection.stroke.color } else { v.weak_text_color() };
                let icon_rect = Rect::from_min_size(Pos2::new(row.left() + 10.0, row.center().y - z / 2.0), Vec2::splat(z));
                let tint = self.tint_for(&e);
                if selected {
                    // On the highlight: the icon's own shape in one solid colour.
                    if let Some(t) = self.entry_mask(ctx, &e, z) {
                        paint_tex(ui.painter(), &t, icon_rect, v.selection.stroke.color);
                    }
                } else if let Some(t) = self.entry_texture(ctx, &e, z) {
                    let painter = ui.painter().clone();
                    self.icon_lift(ctx, &painter, &e, &t, icon_rect, v.dark_mode);
                    paint_tex(ui.painter(), &t, icon_rect, tint);
                }
                if e.is_link {
                    let er = Rect::from_min_size(icon_rect.right_bottom() - Vec2::splat(z * 0.5), Vec2::splat(z * 0.5));
                    if let Some(t) = self.icon(ctx, &["emblem-symbolic-link"], er.width()) {
                        paint_tex(ui.painter(), &t, er, Color32::WHITE);
                    }
                }
                let painter = ui.painter().with_clip_rect(row.intersect(clip));
                let cell = |x: f32, w: f32, text: String, color: Color32| {
                    if w <= 0.0 {
                        return;
                    }
                    let mut job = egui::text::LayoutJob::simple_singleline(text, crate::fonts::ui_font(13.0), color);
                    job.wrap = egui::text::TextWrapping::truncate_at_width((w - 12.0).max(10.0));
                    let g = painter.layout_job(job);
                    painter.galley(Pos2::new(x, row.center().y - g.size().y / 2.0), g, color);
                };
                cell(icon_rect.right() + 8.0, name_w - z - 12.0, e.name.clone(), fg);
                let mut x = row.left() + 4.0 + name_w;
                let size_txt = if e.is_dir {
                    e.child_count.map(|c| format!("{c} item{}", if c == 1 { "" } else { "s" })).unwrap_or_else(|| "—".into())
                } else {
                    fsops::human_size(e.size)
                };
                cell(x + 8.0, size_w, size_txt, weak);
                x += size_w;
                cell(x + 8.0, type_w, self.mime.describe(&e.mime), weak);
                x += type_w;
                if in_trash {
                    let (orig, date) = e.trash_origin.clone().unwrap_or_default();
                    cell(x + 8.0, extra_w, orig.parent().map(|p| p.display().to_string()).unwrap_or_default(), weak);
                    x += extra_w;
                    cell(x + 8.0, date_w, date, weak);
                } else {
                    if in_search {
                        cell(x + 8.0, extra_w, e.path.parent().map(|p| p.display().to_string()).unwrap_or_default(), weak);
                        x += extra_w;
                    }
                    cell(x + 8.0, date_w, fmt_time(e.modified), weak);
                }
                self.click_item(vi, ctx, &resp);
                self.dnd_item(ctx, vi, &resp);
                resp.context_menu(|ui| self.item_menu(ui, &e));
            }
        });
    }

    fn compact_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> usize {
        let z = self.s.list_zoom.min(32.0);
        let row_h = (z + 6.0).max(24.0);
        let col_w = 230.0;
        let mut rows_per_col = 1;
        let scroll_to = self.tab_mut().scroll_to.take();
        egui::ScrollArea::horizontal().auto_shrink([false, false]).id_salt(("compact", self.active, self.pane_salt)).show(ui, |ui| {
            let height = ui.available_height();
            rows_per_col = ((height - 8.0) / row_h).floor().max(1.0) as usize;
            let n = self.tab().visible.len();
            let cols = n.div_ceil(rows_per_col);
            let total_w = (cols as f32 * col_w + 8.0).max(ui.available_width());
            let (rect, bg_resp) = ui.allocate_exact_size(Vec2::new(total_w, height), Sense::CLICK | Sense::DRAG);
            let clip = ui.clip_rect();
            let item_rects: Vec<(usize, Rect)> = (0..n)
                .map(|vi| {
                    let (c, r) = (vi / rows_per_col, vi % rows_per_col);
                    (vi, Rect::from_min_size(rect.min + Vec2::new(4.0 + c as f32 * col_w, 4.0 + r as f32 * row_h), Vec2::new(col_w - 6.0, row_h)))
                })
                .collect();
            self.background(ui, ctx, rect, &bg_resp, &item_rects);
            for (vi, item) in item_rects {
                let e = {
                    let t = self.tab();
                    &t.entries[t.visible[vi]]
                };
                if scroll_to.as_ref() == Some(&e.path) {
                    ui.scroll_to_rect(item, None);
                }
                if !item.intersects(clip) {
                    continue;
                }
                let e = e.clone();
                let selected = self.tab().selected.contains(&e.path);
                let resp = ui.interact(item, Id::unique(("item", self.pane_salt, &e.path)), Sense::CLICK | Sense::DRAG);
                let resp = self.item_access(resp, &e, selected, vi);
                let v = ui.visuals().clone();
                if selected {
                    ui.painter().rect_filled(item, r_sm(), v.selection.bg_fill);
                } else if resp.hovered() {
                    ui.painter().rect_filled(item, r_sm(), v.widgets.hovered.weak_bg_fill);
                }
                self.drop_highlight(ui, item, &resp, e.is_dir);
                let fg = if selected { v.selection.stroke.color } else { v.text_color() };
                let icon_rect = Rect::from_min_size(Pos2::new(item.left() + 6.0, item.center().y - z / 2.0), Vec2::splat(z));
                let tint = self.tint_for(&e);
                if selected {
                    // On the highlight: the icon's own shape in one solid colour.
                    if let Some(t) = self.entry_mask(ctx, &e, z) {
                        paint_tex(ui.painter(), &t, icon_rect, v.selection.stroke.color);
                    }
                } else if let Some(t) = self.entry_texture(ctx, &e, z) {
                    let painter = ui.painter().clone();
                    self.icon_lift(ctx, &painter, &e, &t, icon_rect, v.dark_mode);
                    paint_tex(ui.painter(), &t, icon_rect, tint);
                }
                let mut job = egui::text::LayoutJob::simple_singleline(e.name.clone(), crate::fonts::ui_font(13.0), fg);
                job.wrap = egui::text::TextWrapping::truncate_at_width(item.right() - icon_rect.right() - 12.0);
                let g = ui.painter().layout_job(job);
                ui.painter().galley(Pos2::new(icon_rect.right() + 6.0, item.center().y - g.size().y / 2.0), g, fg);
                self.click_item(vi, ctx, &resp);
                self.dnd_item(ctx, vi, &resp);
                let resp = resp.on_hover_text_at_pointer(self.tooltip(&e));
                resp.context_menu(|ui| self.item_menu(ui, &e));
            }
        });
        rows_per_col
    }

    pub(super) fn drag_overlay(&mut self, ctx: &egui::Context) {
        let Some(p) = egui::DragAndDrop::payload::<DragFiles>(ctx) else { return };
        let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) else { return };
        let n = p.0.len();
        let label = if n == 1 { p.0[0].file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default() } else { format!("{n} items") };
        let copy = ctx.input(|i| i.modifiers.ctrl);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::unique("dragtip")));
        let text = format!("{} {label}", if copy { "+ Copy" } else { "Move" });
        let tip_fg = ctx.global_style().visuals.selection.stroke.color;
        let g = painter.layout_no_wrap(text, crate::fonts::ui_font(13.0), tip_fg);
        let r = Rect::from_min_size(pos + Vec2::new(14.0, 10.0), g.size() + Vec2::new(14.0, 8.0));
        painter.rect_filled(r, r_sm(), accent_color(&self.s));
        painter.galley(r.min + Vec2::new(7.0, 4.0), g, tip_fg);
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    // -------------------------------------------------------------- context menus

    pub(super) fn item_menu(&mut self, ui: &mut egui::Ui, e: &Entry) {
        Self::comfy_menu(ui);
        if crate::picker::active() {
            self.picker_item_menu(ui, e);
            return;
        }
        let in_trash = matches!(self.tab().loc, Location::Trash);
        if !self.tab().selected.contains(&e.path) {
            let t = self.tab_mut();
            t.selected.clear();
            t.selected.insert(e.path.clone());
        }
        let key = self.tab().selection_key();
        if self.menu_sel.as_ref().is_none_or(|(k, _)| *k != key) {
            let entries = self.tab().selected_entries();
            let mimes: Vec<&str> = entries.iter().map(|x| x.mime.as_str()).collect();
            let media = crate::media::common_kind(&mimes);
            let paths: Vec<PathBuf> = entries.iter().map(|x| x.path.clone()).collect();
            let archives = paths.iter().filter(|p| crate::archive::is_archive(p)).cloned().collect();
            self.menu_sel = Some((key, Arc::new(MenuSel { paths, archives, media })));
        }
        let Some((_, menu)) = &self.menu_sel else { return };
        let menu = menu.clone();
        let sel = &menu.paths;
        if in_trash {
            if ui.button("Restore").clicked() {
                self.pending.push(Action::Restore);
            }
            if ui.button("Delete Permanently").clicked() {
                self.pending.push(Action::Delete);
            }
            ui.separator();
            if ui.button("Properties").clicked() {
                self.pending.push(Action::Properties(Some(e.path.clone())));
            }
            return;
        }
        if e.is_dir {
            if ui.button("Open").clicked() {
                self.pending.push(Action::Open(e.path.clone()));
            }
            if ui.button("Open in New Tab").clicked() {
                self.pending.push(Action::OpenInNewTab(e.path.clone()));
            }
            if ui.button("Open in Terminal").clicked() {
                self.pending.push(Action::Terminal(e.path.clone()));
            }
        } else {
            let default = launch::default_app(&self.apps, &e.mime).map(|a| a.name.clone());
            let label = match &default {
                Some(n) => format!("Open With {n}"),
                None => "Open".into(),
            };
            if ui.button(label).clicked() {
                for p in sel {
                    self.pending.push(Action::Open(p.clone()));
                }
            }
            if Self::quick_viewable(e) && ui.add(egui::Button::new("Preview").shortcut_text("Space")).clicked() {
                self.pending.push(Action::QuickView(e.path.clone()));
            }
            // A picture, with Rivet Long Shot installed and not already what Open uses.
            if sel.len() == 1
                && rivet_media::picture::is_picture(&e.path)
                && launch::default_app(&self.apps, &e.mime).is_none_or(|a| a.id != "rivet-longshot.desktop")
                && Self::rivet_longshot().is_some()
                && ui.button("Open in Rivet Long Shot").clicked()
            {
                let p = e.path.clone();
                self.open_in_view(&p);
                ui.close();
            }
            if e.is_executable() && ui.button("Run").clicked() {
                self.pending.push(Action::Run(e.path.clone()));
            }
            ui.menu_button("Open With", |ui| {
                Self::comfy_menu(ui);
                let apps: Vec<(usize, String)> = launch::apps_for(&self.apps, &e.mime)
                    .into_iter()
                    .filter_map(|a| self.apps.iter().position(|x| x.id == a.id).map(|i| (i, a.name.clone())))
                    .take(15)
                    .collect();
                for (i, name) in apps {
                    if ui.button(name).clicked() {
                        self.pending.push(Action::Launch(i, sel.clone()));
                    }
                }
                ui.separator();
                if ui.button("Other Application…").clicked() {
                    self.pending.push(Action::OpenWithDialog(sel.clone()));
                }
            });
        }
        ui.separator();
        let archives = &menu.archives;
        if !archives.is_empty() && archives.len() == sel.len() && ui.button("Extract Here").clicked() {
            self.pending.push(Action::ExtractHere(archives.clone()));
        }
        // One archive, with Rivet Vice installed: look inside before taking anything out.
        if let [one] = archives.as_slice()
            && sel.len() == 1
            && crate::archive::vice().is_some()
            && ui.button("Open in Rivet Vice").clicked()
        {
            if let Err(e) = crate::archive::open_in_vice(one) {
                eprintln!("rivet: {e}");
            }
            ui.close();
        }
        // Media tools (ffmpeg): only when every selected file is a picture, video or audio.
        if self.media_tools_on()
            && let Some(kind) = menu.media
        {
            if ui.button("Convert To…").clicked() {
                self.pending.push(Action::Convert(sel.clone()));
            }
            if kind == crate::preview::Kind::Image && ui.button("Resize…").clicked() {
                self.pending.push(Action::Resize(sel.clone()));
            }
        }
        if ui.button("Compress…").clicked() {
            self.pending.push(Action::Compress(sel.clone()));
        }
        ui.separator();
        if ui.button("Cut").clicked() {
            self.pending.push(Action::Cut);
        }
        if ui.button("Copy").clicked() {
            self.pending.push(Action::Copy);
        }
        // Split view: straight to the folder in the other pane.
        if let Some(other) = self.other_pane_dir().filter(|_| !in_trash) {
            if ui.button("Copy to Other Pane").clicked() {
                self.pending.push(Action::Transfer(sel.clone(), other.clone(), true));
            }
            if ui.button("Move to Other Pane").clicked() {
                self.pending.push(Action::Transfer(sel.clone(), other, false));
            }
        }
        if e.is_dir && (self.clipboard.is_some() || crate::wlclip::others_have_files()) && ui.button("Paste Into Folder").clicked() {
            self.pending.push(Action::Paste(Some(e.path.clone())));
        }
        if ui.button("Copy Location").clicked() {
            self.pending.push(Action::CopyLocation);
        }
        ui.separator();
        if sel.len() == 1 && ui.add(egui::Button::new("Rename…").shortcut_text("F2")).clicked() {
            self.pending.push(Action::Rename(e.path.clone()));
        }
        if sel.len() > 1 && self.s.bulk_rename && ui.add(egui::Button::new("Bulk Rename…").shortcut_text("F2")).clicked() {
            self.pending.push(Action::BulkRename(sel.clone()));
        }
        if ui.button("Move to Trash").clicked() {
            self.pending.push(Action::Trash);
        }
        if ui.button("Delete Permanently").clicked() {
            self.pending.push(Action::Delete);
        }
        if e.is_dir {
            ui.separator();
            if ui.button("Add to Bookmarks").clicked() {
                self.pending.push(Action::AddBookmark(e.path.clone()));
            }
        }
        if matches!(self.tab().loc, Location::Search { .. } | Location::Recent)
            && let Some(parent) = e.path.parent()
                && ui.button("Open Containing Folder").clicked() {
                    self.pending.push(Action::Navigate(Location::Dir(parent.to_path_buf())));
                }
        self.action_items(ui, false);
        ui.separator();
        if ui.button("Properties").clicked() {
            self.pending.push(Action::Properties(Some(e.path.clone())));
        }
    }

    /// The person's own actions that fit this right-click (see actions.rs), grouped into their
    /// submenus. `background`: right-click on empty space.
    fn action_items(&mut self, ui: &mut egui::Ui, background: bool) {
        if !self.s.custom_actions {
            return;
        }
        let sel = if background { Vec::new() } else { self.tab().selected_entries().into_iter().cloned().collect::<Vec<_>>() };
        let items: Vec<crate::actions::Item> = sel.iter().map(|e| crate::actions::Item { path: &e.path, is_dir: e.is_dir, mime: &e.mime }).collect();
        let fitting: Vec<crate::actions::CustomAction> = crate::actions::all().into_iter().filter(|a| a.applies(&items)).collect();
        if fitting.is_empty() {
            return;
        }
        ui.separator();
        let paths: Vec<std::path::PathBuf> = sel.iter().map(|e| e.path.clone()).collect();
        let folder = self.current_dir();
        let mut chosen: Option<crate::actions::CustomAction> = None;
        let mut subs: Vec<String> = Vec::new();
        for a in &fitting {
            match &a.submenu {
                Some(s) => {
                    if !subs.contains(s) {
                        subs.push(s.clone());
                    }
                }
                None => {
                    let ctx = ui.ctx().clone();
                    let tex = a.icon.as_deref().and_then(|i| self.icon(&ctx, &[i, &format!("{i}-symbolic")], 16.0));
                    let resp = match &tex {
                        Some(t) => ui.add(egui::Button::image_and_text(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(16.0)), a.name.as_str())),
                        None => ui.button(&a.name),
                    };
                    let resp = if a.comment.is_empty() { resp } else { resp.on_hover_text(&a.comment) };
                    if resp.clicked() {
                        chosen = Some(a.clone());
                    }
                }
            }
        }
        for s in subs {
            ui.menu_button(&s, |ui| {
                Self::comfy_menu(ui);
                for a in fitting.iter().filter(|a| a.submenu.as_deref() == Some(s.as_str())) {
                    if ui.button(&a.name).clicked() {
                        chosen = Some(a.clone());
                    }
                }
            });
        }
        if let Some(a) = chosen
            && let Err(e) = a.run(&paths, folder.as_deref())
        {
            self.message(&format!("Could not run “{}”", a.name), &e);
        }
    }

    pub(super) fn background_menu(&mut self, ui: &mut egui::Ui) {
        Self::comfy_menu(ui);
        if crate::picker::active() {
            self.picker_background_menu(ui);
            return;
        }
        if matches!(self.tab().loc, Location::Trash) {
            if ui.button("Empty Trash").clicked() {
                self.pending.push(Action::EmptyTrash);
            }
            return;
        }
        let Some(dir) = self.current_dir() else { return };
        if ui.button("Create New Folder").clicked() {
            self.pending.push(Action::NewFolder);
        }
        ui.menu_button("Create New Document", |ui| {
                Self::comfy_menu(ui);
            if ui.button("Empty Document").clicked() {
                self.pending.push(Action::NewFile(None));
            }
            if let Some(tdir) = self.user_dirs.templates.clone()
                && let Ok(rd) = std::fs::read_dir(&tdir) {
                    let mut items: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
                    items.sort();
                    if !items.is_empty() {
                        ui.separator();
                    }
                    for t in items.into_iter().take(20) {
                        let label = t.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if ui.button(label).clicked() {
                            self.pending.push(Action::NewFile(Some(t)));
                        }
                    }
                }
        });
        if ui.button("Open in Terminal").clicked() {
            self.pending.push(Action::Terminal(dir.clone()));
        }
        ui.separator();
        if ui.add_enabled(self.clipboard.is_some() || crate::wlclip::others_have_files(), egui::Button::new("Paste")).clicked() {
            self.pending.push(Action::Paste(None));
        }
        if ui.button("Select All").clicked() {
            self.pending.push(Action::SelectAll);
        }
        ui.separator();
        ui.menu_button("Sort Items", |ui| {
            Self::comfy_menu(ui);
            self.sort_menu(ui)
        });
        if Self::menu_toggle(ui, &mut self.s.show_hidden, "Show Hidden Files", "Ctrl+H") {
            self.resort_all();
        }
        if ui.button("Add to Bookmarks").clicked() {
            self.pending.push(Action::AddBookmark(dir.clone()));
        }
        self.action_items(ui, true);
        ui.separator();
        if ui.button("Properties").clicked() {
            self.pending.push(Action::Properties(Some(dir)));
        }
    }
}

//! Column view (Miller columns): the folders above this one side by side, this folder's
//! column (it has the keyboard and works like the list), and on the right what the chosen
//! item holds: a folder's contents, or Peek for a file (when the Peek panel is closed).
//!
//! Everything follows the tab's location, so history, tabs, split view and the path bar work
//! as in every other view: clicking in another column goes to that folder with the clicked
//! item chosen.

use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Vec2};
use std::path::{Path, PathBuf};

use super::{paint_tex, r_sm, Action, DragFiles, RivetApp};
use crate::model::{Entry, Location, Tab};
use crate::settings::ViewMode;

/// Width of a column (the last one takes whatever room is left).
const COL_W: f32 = 240.0;
/// The line between columns.
const DIVIDER: f32 = 1.0;
/// Other folders' listings kept between frames before the cache starts over.
const CACHE_MAX: usize = 48;

/// What a click in another column asks for, done once the columns are drawn.
#[derive(Default)]
struct Jump {
    /// Go to this folder, choosing this item in it.
    to: Option<(PathBuf, Option<PathBuf>)>,
    /// Then open this (a folder goes one column further).
    open: Option<PathBuf>,
}

impl RivetApp {
    /// Column view is on and this tab shows a folder (Trash, Recent and searches use the list).
    pub(super) fn columns_shown(&self) -> bool {
        self.s.view == ViewMode::Columns && self.tab().loc.dir().is_some()
    }

    /// ←: back to the parent folder's column, with this folder chosen in it.
    pub(super) fn column_left(&mut self) {
        if self.current_dir().is_some_and(|d| d.parent().is_some()) {
            self.go_up();
        }
    }

    /// →: into the chosen folder's column, on its first item.
    pub(super) fn column_right(&mut self) {
        let t = self.tab();
        let cur = t.cursor.filter(|&c| c < t.visible.len()).map(|c| &t.entries[t.visible[c]]);
        let chosen = cur.or_else(|| {
            let sel = t.selected_entries();
            if sel.len() == 1 { Some(sel[0]) } else { None }
        });
        let Some(dir) = chosen.filter(|e| e.is_dir).map(|e| e.path.clone()) else { return };
        self.navigate(Location::Dir(dir));
        let t = self.tab_mut();
        if let Some(&first) = t.visible.first() {
            let p = t.entries[first].path.clone();
            t.selected.insert(p.clone());
            t.cursor = Some(0);
            t.anchor = Some(0);
            t.scroll_to = Some(p);
        }
    }

    /// The folders shown left of `dir`, outermost first: from Home down to the parent, or
    /// from / when `dir` is outside Home.
    fn column_parents(dir: &Path) -> Vec<PathBuf> {
        let home = crate::places::home();
        let root: &Path = if dir.starts_with(&home) { &home } else { Path::new("/") };
        let mut v: Vec<PathBuf> = dir.ancestors().skip(1).take_while(|a| a.starts_with(root)).map(Path::to_path_buf).collect();
        v.reverse();
        v
    }

    /// Another folder's listing, sorted and filtered like the tab's own; read again when the
    /// folder changes on disk (checked at most once a second). Taken out of the cache while
    /// it's drawn; put it back with `column_cache.insert`.
    fn take_listing(&mut self, dir: &Path) -> Tab {
        if let Some(mut t) = self.column_cache.remove(dir)
            && (t.last_check.elapsed().as_millis() < 1000 || !t.needs_refresh())
        {
            return t;
        }
        let mut t = Tab::new(Location::Dir(dir.to_path_buf()));
        t.reload(&self.mime, &self.s);
        t
    }

    fn column_jump(&mut self, dir: PathBuf, select: Option<PathBuf>) {
        self.navigate(Location::Dir(dir));
        let t = self.tab_mut();
        t.selected.clear();
        t.anchor = None;
        t.cursor = None;
        if let Some(p) = select {
            t.selected.insert(p.clone());
            t.cursor = t.visible_index_of(&p);
            t.anchor = t.cursor;
            t.scroll_to = Some(p);
        }
    }

    pub(super) fn columns_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(dir) = self.current_dir() else { return };
        let parents = Self::column_parents(&dir);
        let chosen: Option<Entry> = {
            let sel = self.tab().selected_entries();
            if sel.len() == 1 { Some(sel[0].clone()) } else { None }
        };
        // The column right of this folder's: a chosen folder's contents, or Peek for a chosen
        // file (unless the Peek panel already shows it).
        let next_col = match &chosen {
            Some(e) if e.is_dir => true,
            Some(_) => !self.s.show_peek,
            None => false,
        };
        let z = self.s.list_zoom.min(32.0);
        let row_h = (z + 6.0).max(24.0);
        let count = parents.len() + 1 + next_col as usize;
        let mut jump = Jump::default();
        let mut shown: Vec<Rect> = Vec::new();
        let full = ui.available_size();
        let height = full.y;
        // When the folder or the chosen item changes, bring this column and the next into view.
        let seen_id = Id::unique(("columns-seen", self.pane_salt, self.active));
        let key = Id::unique((&dir, chosen.as_ref().map(|e| &e.path)));
        let changed = ui.data(|d| d.get_temp::<Id>(seen_id)) != Some(key);
        ui.data_mut(|d| d.insert_temp(seen_id, key));
        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .id_salt(("columns", self.active, self.pane_salt))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.horizontal_top(|ui| {
                    for i in 0..count {
                        let last = i + 1 == count;
                        // The last column fills the rest of the window instead of leaving a gap.
                        let used = i as f32 * (COL_W + DIVIDER);
                        let w = if last { (full.x - used).max(COL_W) } else { COL_W };
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, height), Sense::hover());
                        let mut col = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)).id_salt(("column", self.pane_salt, i)));
                        col.set_clip_rect(rect.intersect(ui.clip_rect()));
                        if i < parents.len() {
                            let next = parents.get(i + 1).unwrap_or(&dir).clone();
                            self.side_column(&mut col, ctx, &parents[i], Some(&next), z, row_h, &mut jump);
                        } else if i == parents.len() {
                            self.this_column(&mut col, ctx, z, row_h);
                        } else if let Some(e) = chosen.as_ref().filter(|e| e.is_dir) {
                            self.side_column(&mut col, ctx, &e.path, None, z, row_h, &mut jump);
                        } else {
                            self.file_column(&mut col, ctx);
                        }
                        shown.push(rect);
                        if !last {
                            let (line, _) = ui.allocate_exact_size(Vec2::new(DIVIDER, height), Sense::hover());
                            let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                            ui.painter().vline(line.center().x, line.y_range(), Stroke::new(DIVIDER, stroke.color));
                        }
                    }
                });
                if changed {
                    let here = shown[parents.len()..].iter().fold(Rect::NOTHING, |a, r| a.union(*r));
                    ui.scroll_to_rect(here, None);
                }
            });
        if self.column_cache.len() > CACHE_MAX {
            self.column_cache.clear();
        }
        if let Some((d, sel)) = jump.to {
            self.column_jump(d, sel);
        }
        if let Some(p) = jump.open {
            self.pending.push(Action::Open(p));
        }
    }

    /// One row: icon, name, and a › for folders. `on_path`: this folder's contents are the
    /// next column.
    #[allow(clippy::too_many_arguments)]
    fn column_row(&mut self, ui: &egui::Ui, ctx: &egui::Context, row: Rect, e: &Entry, selected: bool, on_path: bool, hovered: bool, z: f32) {
        let v = ui.visuals().clone();
        let pill = row.shrink2(Vec2::new(4.0, 0.0));
        if selected {
            ui.painter().rect_filled(pill, r_sm(), v.selection.bg_fill);
        } else if on_path {
            ui.painter().rect_filled(pill, r_sm(), v.selection.bg_fill.gamma_multiply(0.3));
        } else if hovered {
            ui.painter().rect_filled(pill, r_sm(), v.widgets.hovered.weak_bg_fill);
        }
        let fg = if selected { v.selection.stroke.color } else { v.text_color() };
        let icon_rect = Rect::from_min_size(Pos2::new(pill.left() + 6.0, row.center().y - z / 2.0), Vec2::splat(z));
        if selected {
            if let Some(t) = self.entry_mask(ctx, e, z) {
                paint_tex(ui.painter(), &t, icon_rect, fg);
            }
        } else if let Some(t) = self.entry_texture(ctx, e, z) {
            let painter = ui.painter().clone();
            self.icon_lift(ctx, &painter, e, &t, icon_rect, v.dark_mode);
            paint_tex(ui.painter(), &t, icon_rect, self.tint_for(e));
        }
        if e.is_link {
            let er = Rect::from_min_size(icon_rect.right_bottom() - Vec2::splat(z * 0.5), Vec2::splat(z * 0.5));
            if let Some(t) = self.icon(ctx, &["emblem-symbolic-link"], er.width()) {
                paint_tex(ui.painter(), &t, er, Color32::WHITE);
            }
        }
        let chevron = if e.is_dir { 16.0 } else { 0.0 };
        let mut job = egui::text::LayoutJob::simple_singleline(e.name.clone(), crate::fonts::ui_font(13.0), fg);
        job.wrap = egui::text::TextWrapping::truncate_at_width((pill.right() - icon_rect.right() - 14.0 - chevron).max(10.0));
        let g = ui.painter().layout_job(job);
        ui.painter().galley(Pos2::new(icon_rect.right() + 7.0, row.center().y - g.size().y / 2.0), g, fg);
        if e.is_dir {
            // Drawn, so it never depends on the font having the glyph.
            let c = Pos2::new(pill.right() - 10.0, row.center().y);
            let color = if selected { fg } else { v.weak_text_color() };
            ui.painter().line(vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(2.0, 0.0), c + Vec2::new(-2.0, 4.0)], Stroke::new(1.4, color));
        }
    }

    /// A quiet note in the middle of an empty column.
    fn column_note(ui: &egui::Ui, rect: Rect, text: &str) {
        ui.painter().text(rect.center_top() + Vec2::new(0.0, 40.0), Align2::CENTER_CENTER, text, crate::fonts::ui_font(13.0), ui.visuals().weak_text_color());
    }

    /// This folder's column: selection, rubber band, drag and drop and menus, like the list.
    fn this_column(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, z: f32, row_h: f32) {
        let scroll_to = self.tab_mut().scroll_to.take();
        let dir = self.current_dir();
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt(("col-here", self.pane_salt, &dir)).show(ui, |ui| {
            let n = self.tab().visible.len();
            let width = ui.available_width();
            let total_h = (n as f32 * row_h + 8.0).max(ui.available_height());
            let (rect, bg_resp) = ui.allocate_exact_size(Vec2::new(width, total_h), Sense::CLICK | Sense::DRAG);
            let clip = ui.clip_rect();
            let item_rects: Vec<(usize, Rect)> = (0..n).map(|vi| (vi, Rect::from_min_size(rect.min + Vec2::new(0.0, 4.0 + vi as f32 * row_h), Vec2::new(width, row_h)))).collect();
            self.background(ui, ctx, rect, &bg_resp, &item_rects);
            if n == 0 {
                let msg = if self.tab().filter.is_empty() { "Folder is empty" } else { "No items match the filter" };
                Self::column_note(ui, rect, msg);
            }
            for (vi, row) in item_rects {
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
                let resp = ui.interact(row.shrink2(Vec2::new(4.0, 0.0)), Id::unique(("item", self.pane_salt, &e.path)), Sense::CLICK | Sense::DRAG);
                self.column_row(ui, ctx, row, &e, selected, false, resp.hovered(), z);
                self.drop_highlight(ui, row.shrink2(Vec2::new(4.0, 0.0)), &resp, e.is_dir);
                self.click_item(vi, ctx, &resp);
                self.dnd_item(ctx, vi, &resp);
                let resp = resp.on_hover_text_at_pointer(self.tooltip(&e));
                resp.context_menu(|ui| self.item_menu(ui, &e));
            }
        });
    }

    /// A parent folder's column (`next`: the item on the way to this folder), or the chosen
    /// folder's (`next` = None). A click goes to that folder with the item chosen; a double
    /// click (or one click, with single-click opening) opens it.
    #[allow(clippy::too_many_arguments)]
    fn side_column(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, dir: &Path, next: Option<&Path>, z: f32, row_h: f32, jump: &mut Jump) {
        let listing = self.take_listing(dir);
        let m = ctx.input(|i| i.modifiers);
        let target = next.map(Path::to_path_buf);
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt(("col", self.pane_salt, dir)).show(ui, |ui| {
            let n = listing.visible.len();
            let width = ui.available_width();
            let total_h = (n as f32 * row_h + 8.0).max(ui.available_height());
            let (rect, bg) = ui.allocate_exact_size(Vec2::new(width, total_h), Sense::click());
            if bg.clicked() {
                jump.to = Some((dir.to_path_buf(), None));
            }
            if let Some(p) = bg.dnd_release_payload::<DragFiles>() {
                self.pending.push(Action::Transfer(p.0.clone(), dir.to_path_buf(), m.ctrl));
            }
            if let Some(err) = &listing.error {
                Self::column_note(ui, rect, if err.contains("permission") { "No permission" } else { "Can't be read" });
            } else if n == 0 {
                Self::column_note(ui, rect, "Folder is empty");
            }
            let clip = ui.clip_rect();
            // The item on the way here stays in view in a long parent folder.
            let shown_id = ui.scope_id().with(("shown", &target));
            let first_show = ui.data(|d| d.get_temp::<bool>(shown_id)).is_none();
            for (vi, &i) in listing.visible.iter().enumerate() {
                let row = Rect::from_min_size(rect.min + Vec2::new(0.0, 4.0 + vi as f32 * row_h), Vec2::new(width, row_h));
                let e = &listing.entries[i];
                let on_path = target.as_deref() == Some(e.path.as_path());
                if on_path && first_show {
                    ui.scroll_to_rect(row, Some(egui::Align::Center));
                }
                if !row.intersects(clip) {
                    continue;
                }
                let hit = row.shrink2(Vec2::new(4.0, 0.0));
                let resp = ui.interact(hit, Id::unique(("col-item", self.pane_salt, &e.path)), Sense::CLICK | Sense::DRAG);
                self.column_row(ui, ctx, row, e, false, on_path, resp.hovered(), z);
                self.drop_highlight(ui, hit, &resp, e.is_dir);
                let open = if self.s.single_click { resp.clicked() && !m.command && !m.shift } else { resp.double_clicked() };
                if resp.clicked_by(egui::PointerButton::Middle) {
                    if e.is_dir {
                        self.pending.push(Action::OpenInNewTab(e.path.clone()));
                    }
                } else if resp.clicked() || resp.secondary_clicked() {
                    jump.to = Some((dir.to_path_buf(), Some(e.path.clone())));
                }
                if open {
                    jump.to = Some((dir.to_path_buf(), Some(e.path.clone())));
                    jump.open = Some(e.path.clone());
                }
                if resp.drag_started() {
                    resp.dnd_set_drag_payload(DragFiles(vec![e.path.clone()]));
                }
                if e.is_dir
                    && let Some(p) = resp.dnd_release_payload::<DragFiles>()
                    && !p.0.contains(&e.path)
                {
                    self.pending.push(Action::Transfer(p.0.clone(), e.path.clone(), m.ctrl));
                }
                resp.on_hover_text_at_pointer(self.tooltip(e));
            }
            ui.data_mut(|d| d.insert_temp(shown_id, true));
        });
        self.column_cache.insert(dir.to_path_buf(), listing);
    }

    /// A chosen file's column: Peek, drawn here while the Peek panel is closed (never both:
    /// Peek has one preview and one video player).
    fn file_column(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Frame::NONE.inner_margin(egui::Margin { left: 10, right: 12, top: 8, bottom: 10 }).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt(("col-peek", self.pane_salt)).show(ui, |ui| self.peek_contents(ui, ctx));
        });
    }
}

//! Drawing: the toolbar, the connection list, the two panes and the transfer queue.

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_connections::Protocol;
use rivet_look::fonts;
use rivet_look::theme::{r_lg, r_md, r_sm, status};

use super::logic::{expand, first_line};
use super::menus::menu_style;
use super::*;
use crate::listing;
use crate::sftp;

const ROW_H: f32 = 26.0;

impl FtpApp {
    /// The bottom line when the transfer list is hidden: the last message and a count.
    pub(super) fn status_line(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        ui.horizontal(|ui| {
            let waiting = self.queue.iter().filter(|j| j.state == JobState::Waiting).count();
            if waiting > 0 {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(format!("{waiting} transfer{} to go", if waiting == 1 { "" } else { "s" }));
            }
            if !self.edits.is_empty() {
                let n = self.edits.len();
                ui.label(egui::RichText::new(format!("Editing {n} file{}", if n == 1 { "" } else { "s" })).color(v.selection.bg_fill));
            }
            if let Some((msg, bad)) = &self.note {
                let c = if *bad { status(&ctx).danger } else { v.weak_text_color() };
                ui.label(egui::RichText::new(first_line(msg)).color(c)).on_hover_text(msg.as_str());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new("Transfers").frame(false)).on_hover_text("Show transfers (F8)").clicked() {
                    self.s.show_queue = true;
                    self.s.save();
                }
            });
        });
    }

    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Connections").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.icon_button(ui, &["list-add-symbolic", "list-add"], "+", "New connection").clicked() {
                    self.form = Some(Form::blank());
                }
            });
        });
        ui.add_space(6.0);
        if self.store.items.is_empty() {
            ui.label(egui::RichText::new("No saved connections yet. Add one with ＋, or run rivet-ftp add in a terminal.").small().weak());
        }
        let server_icon = self.named_icon(&ctx, &["network-server", "folder-remote", "network-workgroup"]);
        let items = self.store.items.clone();
        let cats = self.store.categories();
        // Connections without a category first, then each category under a header that folds.
        let mut groups: Vec<(Option<String>, Vec<rivet_connections::Connection>)> = vec![(None, items.iter().filter(|c| c.category.is_none()).cloned().collect())];
        for k in &cats {
            groups.push((Some(k.clone()), items.iter().filter(|c| c.category.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(k))).cloned().collect()));
        }
        let mut move_to: Option<(String, Option<String>)> = None;
        let mut ask_new: Option<String> = None;
        let mut rename: Option<String> = None;
        let mut new_in: Option<String> = None;
        let mut fold: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            for (cat, conns) in &groups {
                if let Some(k) = cat {
                    let open = !self.s.collapsed.iter().any(|x| x.eq_ignore_ascii_case(k));
                    let here = self.conn.as_ref().is_some_and(|c| conns.iter().any(|x| x.name == c.name));
                    let resp = category_header(ui, k, conns.len(), open, here);
                    if resp.clicked() {
                        fold = Some(k.clone());
                    }
                    resp.context_menu(|ui| {
                        menu_style(ui);
                        if ui.button(if open { "Collapse" } else { "Expand" }).clicked() {
                            fold = Some(k.clone());
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("New Connection Here…").clicked() {
                            new_in = Some(k.clone());
                            ui.close();
                        }
                        if ui.button("Rename Category…").clicked() {
                            rename = Some(k.clone());
                            ui.close();
                        }
                    });
                    if !open {
                        continue;
                    }
                }
                for c in conns {
                    let current = self.conn.as_ref().is_some_and(|k| k.name == c.name);
                    // One line: the name, or user@server for a connection that wasn't given one.
                    // Everything else is in the tooltip, so the sidebar can stay narrow.
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
                    let v = ui.visuals();
                    if current {
                        ui.painter().rect_filled(rect, r_md(), v.selection.bg_fill);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, r_md(), v.widgets.hovered.weak_bg_fill);
                    }
                    let fg = if current { v.selection.stroke.color } else { v.text_color() };
                    if let Some(t) = &server_icon {
                        let r = Rect::from_center_size(Pos2::new(rect.left() + 16.0, rect.center().y), Vec2::splat(16.0));
                        ui.painter().image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    let text_rect = Rect::from_min_max(Pos2::new(rect.left() + 32.0, rect.top()), Pos2::new(rect.right() - 8.0, rect.bottom()));
                    let title = c.title();
                    let galley = ui.painter().layout_no_wrap(title.clone(), fonts::ui_font(13.5), fg);
                    let cut = galley.size().x > text_rect.width();
                    // Too long for the sidebar: fade it out at the edge rather than spill over.
                    let galley = if cut {
                        let mut job = egui::text::LayoutJob::single_section(title, egui::TextFormat::simple(fonts::ui_font(13.5), fg));
                        job.wrap = egui::text::TextWrapping::truncate_at_width(text_rect.width());
                        ui.painter().layout_job(job)
                    } else {
                        galley
                    };
                    ui.painter().with_clip_rect(text_rect).galley(Pos2::new(text_rect.left(), rect.center().y - galley.size().y / 2.0), galley, fg);
                    let resp = resp.on_hover_ui(|ui| connection_tooltip(ui, c));
                    // Open in another tab: a dot, like the tab's own.
                    if !current && self.tab_with(&c.name).is_some() {
                        ui.painter().circle_filled(Pos2::new(rect.right() - 10.0, rect.center().y), 3.5, status(&ctx).success);
                    }
                    if resp.clicked() && !current {
                        self.open_connection(c.clone(), &ctx, false);
                    }
                    resp.context_menu(|ui| {
                        menu_style(ui);
                        if ui.button("Connect").clicked() {
                            self.open_connection(c.clone(), &ctx, false);
                            ui.close();
                        }
                        if self.tab_with(&c.name).is_none() && ui.button("Connect in New Tab").clicked() {
                            self.open_connection(c.clone(), &ctx, true);
                            ui.close();
                        }
                        if ui.button("Edit…").clicked() {
                            self.form = Some(Form::from(c));
                            ui.close();
                        }
                        if c.protocol == Protocol::Sftp && ui.button("Copy ssh command").clicked() {
                            let cmd = format!("rivet-ftp ssh {}", c.alias());
                            ui.ctx().copy_text(cmd.clone());
                            self.note = Some((format!("Copied: {cmd}"), false));
                            ui.close();
                        }
                        ui.menu_button("Move to Category", |ui| {
                            menu_style(ui);
                            if ui.selectable_label(c.category.is_none(), "None").clicked() {
                                move_to = Some((c.name.clone(), None));
                                ui.close();
                            }
                            if !cats.is_empty() {
                                ui.separator();
                            }
                            for k in &cats {
                                let on = c.category.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(k));
                                if ui.selectable_label(on, k).clicked() {
                                    move_to = Some((c.name.clone(), Some(k.clone())));
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button("New Category…").clicked() {
                                ask_new = Some(c.name.clone());
                                ui.close();
                            }
                        });
                        ui.separator();
                        if ui.button("Remove").clicked() {
                            if let Some(gone) = self.store.remove(&c.name) {
                                self.note = Some(match self.store.save() {
                                    Ok(()) => (format!("Removed \"{}\".", gone.name), false),
                                    Err(e) => (format!("Couldn't save: {e}"), true),
                                });
                            }
                            ui.close();
                        }
                    });
                    ui.add_space(2.0);
                }
            }
        });
        if let Some(k) = fold {
            if let Some(i) = self.s.collapsed.iter().position(|x| x.eq_ignore_ascii_case(&k)) {
                self.s.collapsed.remove(i);
            } else {
                self.s.collapsed.push(k);
            }
            self.s.save();
        }
        if let Some((conn, cat)) = move_to {
            self.set_category(&conn, cat);
        }
        if let Some(conn) = ask_new {
            self.prompt = Some(Prompt::NewCategory(conn, String::new()));
        }
        if let Some(k) = rename {
            self.prompt = Some(Prompt::RenameCategory(k.clone(), k));
        }
        if let Some(k) = new_in {
            let mut f = Form::blank();
            f.category = k;
            self.form = Some(f);
        }
    }

    pub(super) fn draw_pane(&mut self, ui: &mut egui::Ui, side: Side, rect: Rect) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        ui.painter().rect_filled(rect, r_lg(), v.extreme_bg_color);
        if self.focus == side {
            ui.painter().rect_stroke(rect, r_lg(), Stroke::new(1.0, v.selection.bg_fill.gamma_multiply(0.55)), egui::StrokeKind::Inside);
        }
        let inner = rect.shrink2(Vec2::new(10.0, 8.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
        let ui = &mut child;

        // Header: which side, up, refresh, and the folder.
        let title = match side {
            Side::Local => "This computer".to_string(),
            Side::Remote => self.conn.as_ref().map(|c| c.title()).unwrap_or_else(|| "Server".into()),
        };
        let live = side == Side::Local || (self.browse.is_some() && !self.connecting);
        ui.horizontal(|ui| {
            // The server's state, as a dot before its name: green connected, amber
            // connecting, grey not connected.
            if side == Side::Remote {
                let (dot, state) = match (&self.conn, self.connecting) {
                    (Some(_), true) => (status(&ctx).warning, "Connecting…"),
                    (Some(_), false) => (status(&ctx).success, "Connected"),
                    (None, _) => (v.weak_text_color(), "Not connected"),
                };
                let (r, resp) = ui.allocate_exact_size(Vec2::new(12.0, 18.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, dot);
                let tip = match &self.conn {
                    Some(c) => format!("{state}: {}", c.url()),
                    None => state.to_string(),
                };
                let title_resp = ui.label(egui::RichText::new(title).strong());
                resp.union(title_resp).on_hover_text(tip);
            } else {
                ui.label(egui::RichText::new(title).strong());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if live {
                    if self.icon_button(ui, &["view-refresh-symbolic", "view-refresh"], "Refresh", "Refresh (F5)").clicked() {
                        self.reload(side);
                    }
                    if self.icon_button(ui, &["go-up-symbolic", "go-up"], "Up", "Up a folder (Backspace)").clicked() {
                        self.go_up(side);
                    }
                }
            });
        });
        ui.add_space(2.0);
        if live {
            self.path_bar(ui, side);
        }
        ui.add_space(6.0);

        // Column headings.
        let row_w = ui.available_width();
        let cols = (row_w - 190.0, 80.0);
        let head_y = ui.cursor().top() + 9.0;
        let weak = v.weak_text_color();
        let x0 = inner.left();
        if live {
            use crate::listing::{Sort, SortCol};
            let sort = self.pane_ref(side).sort;
            let mut pick: Option<SortCol> = None;
            let cells = [
                (SortCol::Name, "Name", Rect::from_min_max(Pos2::new(x0 + 22.0, head_y - 10.0), Pos2::new(x0 + cols.0 - 8.0, head_y + 10.0)), false),
                (SortCol::Size, "Size", Rect::from_min_max(Pos2::new(x0 + cols.0, head_y - 10.0), Pos2::new(x0 + cols.0 + cols.1 + 4.0, head_y + 10.0)), true),
                (SortCol::Modified, "Modified", Rect::from_min_max(Pos2::new(x0 + cols.0 + cols.1 + 10.0, head_y - 10.0), Pos2::new(inner.right(), head_y + 10.0)), false),
            ];
            for (col, label, r, right) in cells {
                let resp = ui.interact(r, egui::Id::unique(("sort", side as u8, label)), Sense::click());
                let on = sort.col == col;
                let c = if on || resp.hovered() { v.text_color() } else { weak };
                // Right-aligned headings leave room on their right for the arrow.
                let (pos, align) = if right { (Pos2::new(r.right() - if on { 16.0 } else { 4.0 }, head_y), Align2::RIGHT_CENTER) } else { (Pos2::new(r.left() + 6.0, head_y), Align2::LEFT_CENTER) };
                let tr = ui.painter().text(pos, align, label, fonts::ui_font(11.5), c);
                if on {
                    // A small drawn triangle: up for ascending, down for descending.
                    let cx = tr.right() + 8.0;
                    let pts = if sort.desc {
                        vec![Pos2::new(cx - 3.5, head_y - 1.5), Pos2::new(cx + 3.5, head_y - 1.5), Pos2::new(cx, head_y + 2.5)]
                    } else {
                        vec![Pos2::new(cx - 3.5, head_y + 2.0), Pos2::new(cx + 3.5, head_y + 2.0), Pos2::new(cx, head_y - 2.0)]
                    };
                    ui.painter().add(egui::Shape::convex_polygon(pts, c, Stroke::NONE));
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    pick = Some(col);
                }
            }
            if let Some(col) = pick {
                let p = self.pane(side);
                p.sort = if p.sort.col == col { Sort { col, desc: !p.sort.desc } } else { Sort { col, desc: col != SortCol::Name } };
                let order = p.sort;
                crate::listing::sort_by(&mut p.entries, order);
                p.cursor = None;
            }
        }
        ui.allocate_space(Vec2::new(row_w, 20.0));

        let footer_h = 32.0;
        let list_rect = Rect::from_min_max(Pos2::new(inner.left(), ui.cursor().top()), Pos2::new(inner.right(), inner.bottom() - footer_h));

        // Empty states.
        let pane = self.pane_ref(side);
        let message: Option<(String, Color32)> = if side == Side::Remote && self.browse.is_none() {
            Some(("Not connected\n\nPick a saved connection on the left,\nor type an address above.".into(), weak))
        } else if pane.loading && pane.entries.is_empty() {
            Some((if self.connecting { "Connecting…".into() } else { "Loading…".into() }, weak))
        } else if let Some(e) = &pane.error {
            Some((first_line(e).to_string(), status(&ctx).danger))
        } else if pane.entries.is_empty() {
            Some(("This folder is empty".into(), weak))
        } else {
            None
        };
        // The list area: click to focus (and clear the selection), right-click for the menu.
        let bg = ui.interact(list_rect, egui::Id::unique(("pane-bg", side as u8)), Sense::click());
        if bg.clicked() {
            self.focus = side;
            self.pane(side).selected.clear();
        }
        if bg.secondary_clicked() {
            self.focus = side;
            self.pane(side).selected.clear();
        }
        bg.context_menu(|ui| self.pane_menu(ui, side));
        if let Some((m, c)) = message {
            ui.painter().text(list_rect.center(), Align2::CENTER_CENTER, m, fonts::ui_font(13.0), c);
        } else {
            self.rows(ui, side, list_rect, cols);
        }

        // Dropped here from the other pane: send it across.
        if let Some(p) = egui::DragAndDrop::payload::<DragItems>(&ctx)
            && p.from != side
            && ctx.pointer_hover_pos().is_some_and(|pos| list_rect.contains(pos))
        {
            ui.painter().rect_stroke(list_rect.expand(3.0), r_md(), Stroke::new(2.0, v.selection.bg_fill), egui::StrokeKind::Outside);
            if ctx.input(|i| i.pointer.any_released()) {
                egui::DragAndDrop::clear_payload(&ctx);
                self.transfer_items(p.from, p.items.clone());
            }
        }

        // Footer: counts and the send button.
        let foot = Rect::from_min_max(Pos2::new(inner.left(), inner.bottom() - footer_h + 4.0), inner.max);
        let pane = self.pane_ref(side);
        let sel = pane.selected.len();
        let count = match (sel, pane.entries.len()) {
            (0, 1) => "1 item".to_string(),
            (0, n) => format!("{n} items"),
            (s, n) => format!("{s} of {n} selected"),
        };
        if self.browse.is_some() || side == Side::Local {
            ui.painter().text(Pos2::new(foot.left() + 2.0, foot.center().y), Align2::LEFT_CENTER, count, fonts::ui_font(12.0), weak);
        }
        let (label, arrow): (&str, &[&str]) = match side {
            Side::Local => ("Upload", &["go-next-symbolic", "go-next"]),
            Side::Remote => ("Download", &["go-previous-symbolic", "go-previous"]),
        };
        let arrow_tex = self.icon_named(&ctx, arrow);
        let can = sel > 0 && self.transfer.is_some();
        let br = Rect::from_min_size(Pos2::new(foot.right() - 116.0, foot.top()), Vec2::new(116.0, foot.height() - 2.0));
        let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(br));
        let (fill, text_c) = if can { (v.selection.bg_fill, v.selection.stroke.color) } else { (v.widgets.inactive.weak_bg_fill, weak) };
        let tip = if self.transfer.is_none() { "Connect to a server first" } else { "Select files first" };
        let text = egui::RichText::new(label).color(text_c);
        let button = match &arrow_tex {
            Some(t) => {
                let img = egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(14.0)).tint(text_c);
                if side == Side::Local { egui::Button::new(text).right_text(egui::Atom::from(img)) } else { egui::Button::image_and_text(img, text) }
            }
            None => egui::Button::new(text),
        };
        let b = bui.add_enabled(can, button.fill(fill).corner_radius(r_sm()).min_size(br.size()));
        if b.on_disabled_hover_text(tip).clicked() {
            let items = self.pane_ref(side).selected_entries();
            self.transfer_items(side, items);
        }
    }

    fn path_bar(&mut self, ui: &mut egui::Ui, side: Side) {
        let v = ui.visuals().clone();
        let ctx = ui.ctx().clone();
        let mut go: Option<String> = None;
        egui::Frame::new().fill(v.text_edit_bg_color()).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let pane = self.pane(side);
            if let Some(text) = pane.editing.as_mut() {
                let r = ui.add(egui::TextEdit::singleline(text).frame(egui::Frame::NONE).desired_width(f32::INFINITY));
                if r.lost_focus() {
                    if ui.input(|i| i.key_pressed(Key::Enter)) {
                        go = Some(text.clone());
                    }
                    pane.editing = None;
                } else if !r.has_focus() {
                    r.request_focus();
                }
            } else {
                // The folder as steps, like Rivet's path bar: click one to go there; the current
                // folder is an accent pill; the empty end switches to typing a path.
                let path = pane.path.clone();
                let home = rivet_xdg::places::home().to_string_lossy().into_owned();
                let mut crumbs: Vec<(String, String, &str)> = Vec::new();
                let rest = if side == Side::Local && (path == home || path.starts_with(&format!("{home}/"))) {
                    crumbs.push(("Home".into(), home.clone(), "user-home"));
                    path[home.len()..].to_string()
                } else {
                    crumbs.push((if side == Side::Local { "File System".into() } else { "/".into() }, "/".into(), if side == Side::Local { "drive-harddisk" } else { "network-server" }));
                    path.clone()
                };
                let mut acc = crumbs[0].1.trim_end_matches('/').to_string();
                for part in rest.split('/').filter(|s| !s.is_empty()) {
                    acc.push('/');
                    acc.push_str(part);
                    crumbs.push((part.to_string(), acc.clone(), ""));
                }
                let last = crumbs.len() - 1;
                let icons: Vec<Option<egui::TextureHandle>> = crumbs.iter().map(|(_, _, i)| if i.is_empty() { None } else { self.named_icon(&ctx, &[&format!("{i}-symbolic"), i]) }).collect();
                let pane = self.pane(side);
                egui::ScrollArea::horizontal()
                    .id_salt(("crumbs", side as u8))
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .stick_to_right(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (i, (label, target, _)) in crumbs.iter().enumerate() {
                                let sel = i == last;
                                let fg = if sel { v.selection.stroke.color } else { v.text_color() };
                                let text = if sel { egui::RichText::new(label).strong().color(fg) } else { egui::RichText::new(label) };
                                let btn = match &icons[i] {
                                    Some(t) => egui::Button::image_and_text(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(14.0)).tint(fg), text),
                                    None => egui::Button::new(text),
                                };
                                let resp = ui.add(btn.selected(sel).frame_when_inactive(sel).min_size(Vec2::new(0.0, 22.0)).corner_radius(r_sm()));
                                if resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if resp.clicked() {
                                    go = Some(target.clone());
                                }
                            }
                            let rest = ui.allocate_response(Vec2::new(ui.available_width().max(24.0), 22.0), Sense::click()).on_hover_cursor(egui::CursorIcon::Text);
                            if rest.clicked() {
                                pane.editing = Some(path.clone());
                            }
                        });
                    });
            }
        });
        if let Some(p) = go {
            self.focus = side;
            match side {
                Side::Local => {
                    let p = expand(p.trim());
                    if p.is_dir() {
                        self.open_local(p.to_string_lossy().into_owned());
                    } else {
                        self.note = Some((format!("There's no folder {}", p.display()), true));
                    }
                }
                Side::Remote => {
                    let p = p.trim().to_string();
                    let p = if p.starts_with('/') { p } else { sftp::join(&self.remote.path, &p) };
                    self.open_remote(p);
                }
            }
        }
    }

    fn rows(&mut self, ui: &mut egui::Ui, side: Side, list_rect: Rect, cols: (f32, f32)) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let (name_w, size_w) = cols;
        let entries = self.pane_ref(side).entries.clone();
        let focused = self.focus == side;
        let scroll_to = {
            let p = self.pane(side);
            let s = if p.scroll_to_cursor { p.cursor } else { None };
            p.scroll_to_cursor = false;
            s
        };
        let mut lui = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
        let mut clicked: Option<(usize, egui::Modifiers)> = None;
        let mut activated: Option<usize> = None;
        let mut drag_start: Option<usize> = None;
        egui::ScrollArea::vertical().id_salt(("rows", side as u8)).auto_shrink([false, false]).show_rows(&mut lui, ROW_H, entries.len(), |ui, range| {
            for i in range {
                let e = &entries[i];
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click_and_drag());
                if scroll_to == Some(i) {
                    ui.scroll_to_rect(rect, None);
                }
                let selected = self.pane_ref(side).selected.contains(&e.name);
                if selected {
                    let fill = if focused { v.selection.bg_fill } else { v.selection.bg_fill.gamma_multiply(0.4) };
                    ui.painter().rect_filled(rect, r_sm(), fill);
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, r_sm(), v.widgets.hovered.weak_bg_fill);
                }
                let fg = if selected && focused { v.selection.stroke.color } else { v.text_color() };
                let weak = if selected && focused { fg.gamma_multiply(0.8) } else { v.weak_text_color() };
                if let Some(t) = self.icon(&ctx, e) {
                    let r = Rect::from_center_size(Pos2::new(rect.left() + 14.0, rect.center().y), Vec2::splat(16.0));
                    ui.painter().image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                let name_rect = Rect::from_min_max(Pos2::new(rect.left() + 28.0, rect.top()), Pos2::new(rect.left() + name_w - 8.0, rect.bottom()));
                let galley = ui.painter().layout_no_wrap(e.name.clone(), fonts::ui_font(13.0), fg);
                ui.painter().with_clip_rect(name_rect).galley(Pos2::new(name_rect.left(), rect.center().y - galley.size().y / 2.0), galley, fg);
                if !e.is_dir {
                    ui.painter().text(Pos2::new(rect.left() + name_w + size_w, rect.center().y), Align2::RIGHT_CENTER, listing::human_size(e.size), fonts::ui_font(12.0), weak);
                }
                ui.painter().text(Pos2::new(rect.left() + name_w + size_w + 16.0, rect.center().y), Align2::LEFT_CENTER, &e.modified, fonts::ui_font(12.0), weak);
                if resp.double_clicked() {
                    activated = Some(i);
                } else if resp.clicked() {
                    clicked = Some((i, ui.input(|inp| inp.modifiers)));
                } else if resp.secondary_clicked() {
                    // Right-click acts on the selection; a row outside it becomes the selection
                    // first (right away, so the menu below already names it).
                    self.focus = side;
                    if !selected {
                        let p = self.pane(side);
                        p.selected = [e.name.clone()].into();
                        p.anchor = Some(i);
                        p.cursor = Some(i);
                    }
                }
                if resp.drag_started() {
                    drag_start = Some(i);
                }
                resp.context_menu(|ui| self.pane_menu(ui, side));
            }
        });
        if let Some((i, m)) = clicked {
            self.focus = side;
            let p = self.pane(side);
            let name = entries[i].name.clone();
            if m.command {
                if !p.selected.remove(&name) {
                    p.selected.insert(name);
                }
                p.anchor = Some(i);
            } else if m.shift {
                let a = p.anchor.unwrap_or(i);
                p.selected = (a.min(i)..=a.max(i)).map(|k| entries[k].name.clone()).collect();
            } else {
                p.selected = [name].into();
                p.anchor = Some(i);
            }
            p.cursor = Some(i);
        }
        if let Some(i) = activated {
            self.focus = side;
            let e = &entries[i];
            if e.is_dir || (e.is_link && side == Side::Remote) || self.s.double_click_sends {
                self.activate(side, e);
            }
        }
        if let Some(i) = drag_start {
            let p = self.pane(side);
            if !p.selected.contains(&entries[i].name) {
                p.selected = [entries[i].name.clone()].into();
            }
            let items = p.selected_entries();
            egui::DragAndDrop::set_payload(&ctx, DragItems { from: side, items });
        }
        // While dragging: what's being carried, under the pointer.
        if let Some(p) = egui::DragAndDrop::payload::<DragItems>(&ctx)
            && p.from == side
            && let Some(pos) = ctx.pointer_hover_pos()
        {
            let text = if p.items.len() == 1 { p.items[0].name.clone() } else { format!("{} items", p.items.len()) };
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("drag-label")));
            let g = painter.layout_no_wrap(text, fonts::ui_font(12.5), v.selection.stroke.color);
            let r = Rect::from_min_size(pos + Vec2::new(14.0, 10.0), g.size() + Vec2::new(16.0, 8.0));
            painter.rect_filled(r, r_sm(), v.selection.bg_fill);
            painter.galley(r.min + Vec2::new(8.0, 4.0), g, v.selection.stroke.color);
        }
    }

    /// The right-click menu: on the selection when there is one, else on the folder.
    pub(super) fn pane_menu(&mut self, ui: &mut egui::Ui, side: Side) {
        use super::menus::Cmd;
        menu_style(ui);
        let ctx = ui.ctx().clone();
        let sel = self.pane_ref(side).selected_entries();
        let connected = self.transfer.is_some();
        let one_file = sel.len() == 1 && !sel[0].is_dir;
        let can = side == Side::Local || connected;
        if !sel.is_empty() {
            // ---- look at it
            if sel.iter().any(|e| !e.is_dir) && (side == Side::Local || connected) && ui.add(egui::Button::new("Preview").shortcut_text("Space")).clicked() {
                self.open_preview(side);
                ui.close();
            }
            // ---- open
            if sel.len() == 1 && sel[0].is_dir && ui.button("Open").clicked() {
                self.activate(side, &sel[0]);
                ui.close();
            }
            if side == Side::Local && one_file && ui.button("Open").clicked() {
                let path = std::path::Path::new(&self.local.path).join(&sel[0].name);
                let mime = self.mime.guess_by_name(&path).unwrap_or_else(|| "application/octet-stream".into());
                let apps = self.apps().to_vec();
                if let Err(e) = rivet_xdg::launch::open_default(&apps, &mime, &path) {
                    self.note = Some((format!("Couldn't open it: {e}"), true));
                }
                ui.close();
            }
            if side == Side::Remote && one_file && connected {
                if ui.add(egui::Button::new("Open in Editor").shortcut_text("F4")).on_hover_text("Saves go straight back to the server").clicked() {
                    self.edit_remote(&sel[0], None);
                    ui.close();
                }
                let apps = self.editors_for(&sel[0].name);
                ui.menu_button("Open With", |ui| {
                    menu_style(ui);
                    if apps.is_empty() {
                        ui.label(egui::RichText::new("No apps for this kind of file").weak());
                    }
                    for a in apps {
                        if ui.button(&a.name).clicked() {
                            self.edit_remote(&sel[0], Some(a.clone()));
                            ui.close();
                        }
                    }
                });
            }
            ui.separator();
            // ---- send across
            let label = match (side, sel.len()) {
                (Side::Local, 1) => format!("Upload \"{}\"", sel[0].name),
                (Side::Local, n) => format!("Upload {n} Items"),
                (Side::Remote, 1) => format!("Download \"{}\"", sel[0].name),
                (Side::Remote, n) => format!("Download {n} Items"),
            };
            let sc = if side == Side::Local { "Ctrl+U" } else { "Ctrl+D" };
            if ui.add_enabled(connected, egui::Button::new(label).shortcut_text(sc)).on_disabled_hover_text("Connect to a server first").clicked() {
                self.transfer_items(side, sel.clone());
                ui.close();
            }
            ui.separator();
            // ---- about it
            if ui.button("Copy Location").clicked() {
                self.run(Cmd::CopyLocation, &ctx);
                ui.close();
            }
            if ui.add(egui::Button::new("Properties").shortcut_text("Ctrl+I")).clicked() {
                self.show_info(side);
                ui.close();
            }
            ui.separator();
            // ---- change it
            if ui.add_enabled(sel.len() == 1 && can, egui::Button::new("Rename…").shortcut_text("F2")).clicked() {
                self.run(Cmd::Rename, &ctx);
                ui.close();
            }
            if side == Side::Remote && ui.add_enabled(connected, egui::Button::new("Delete from Server…").shortcut_text("Delete")).clicked() {
                self.run(Cmd::Delete, &ctx);
                ui.close();
            }
            ui.separator();
        }
        // ---- the folder
        if ui.add_enabled(can, egui::Button::new("Create New Folder…").shortcut_text("Shift+Ctrl+N")).clicked() {
            self.prompt = Some(Prompt::NewFolder(side, String::new()));
            ui.close();
        }
        if ui.add_enabled(can, egui::Button::new("Reload").shortcut_text("F5")).clicked() {
            self.reload(side);
            ui.close();
        }
        if sel.is_empty() && ui.add_enabled(can, egui::Button::new("Select All").shortcut_text("Ctrl+A")).clicked() {
            self.focus = side;
            self.run(Cmd::SelectAll, &ctx);
            ui.close();
        }
        ui.separator();
        let mut hidden = self.s.show_hidden;
        if ui.checkbox(&mut hidden, "Show Hidden Files").clicked() {
            self.toggle_hidden();
            ui.close();
        }
    }

    pub(super) fn queue_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let waiting = self.queue.iter().filter(|j| j.state == JobState::Waiting).count();
            let failed = self.queue.iter().filter(|j| matches!(j.state, JobState::Failed(_))).count();
            let head = if waiting > 0 {
                format!("Transfers · {waiting} to go")
            } else if !self.queue.is_empty() {
                format!("Transfers · all done{}", if failed > 0 { format!(", {failed} failed") } else { String::new() })
            } else {
                "Transfers".into()
            };
            ui.label(egui::RichText::new(head).strong());
            if let Some((msg, bad)) = &self.note {
                ui.add_space(12.0);
                let c = if *bad { status(&ctx).danger } else { v.weak_text_color() };
                ui.label(egui::RichText::new(first_line(msg)).color(c)).on_hover_text(msg.as_str());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.queue.iter().any(|j| j.state != JobState::Waiting) && ui.add(egui::Button::new("Clear finished").corner_radius(r_sm())).clicked() {
                    self.queue.retain(|j| j.state == JobState::Waiting);
                }
            });
        });
        ui.add_space(4.0);
        self.edits_panel(ui);
        let running = self.queue.iter().find(|j| j.state == JobState::Waiting).map(|j| j.id);
        // The running one: note when it started, and its speed over the last few seconds.
        let mut progress: std::collections::HashMap<u64, (u64, f64)> = std::collections::HashMap::new();
        if let Some(j) = self.queue.iter_mut().find(|j| Some(j.id) == running) {
            let now = std::time::Instant::now();
            j.started.get_or_insert(now);
            if let Some(n) = j.local_target.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()) {
                if j.samples.back().is_none_or(|(t, _)| now.duration_since(*t).as_millis() >= 250) {
                    j.samples.push_back((now, n));
                }
                while j.samples.len() > 2 && j.samples.front().is_some_and(|(t, _)| now.duration_since(*t).as_secs_f64() > 4.0) {
                    j.samples.pop_front();
                }
                let speed = match (j.samples.front(), j.samples.back()) {
                    (Some((t0, b0)), Some((t1, b1))) if t1 > t0 => (b1.saturating_sub(*b0)) as f64 / t1.duration_since(*t0).as_secs_f64(),
                    _ => 0.0,
                };
                progress.insert(j.id, (n, speed));
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
        let mut retry: Option<usize> = None;
        let mut arrows = std::collections::HashMap::new();
        for n in [["go-up-symbolic", "go-up"], ["go-down-symbolic", "go-down"]] {
            arrows.insert(n[0], self.icon_named(&ctx, &n));
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.queue.is_empty() {
                ui.label(egui::RichText::new("Drag files between the panes, double-click a file, or use Upload and Download.").weak());
            }
            for (idx, j) in self.queue.iter().enumerate().rev() {
                ui.horizontal(|ui| {
                    let names: &[&str] = if j.dir == Dir::Up { &["go-up-symbolic", "go-up"] } else { &["go-down-symbolic", "go-down"] };
                    match arrows.get(names[0]).cloned().flatten() {
                        Some(t) => {
                            ui.add(egui::Image::from_texture(&t).fit_to_exact_size(Vec2::splat(14.0)).tint(v.selection.bg_fill));
                        }
                        None => {
                            ui.label(egui::RichText::new(if j.dir == Dir::Up { "Up" } else { "Down" }).color(v.selection.bg_fill));
                        }
                    }
                    // What and where: "report.pdf to /var/www", the full paths on hover.
                    let dest = if j.dir == Dir::Up { j.to_dir.clone() } else { tilde_path(&j.to_dir) };
                    ui.label(&j.name).on_hover_text(format!("From {}\nTo {}", j.from, j.to));
                    ui.label(egui::RichText::new(format!("to {dest}")).weak()).on_hover_text(&j.to);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match &j.state {
                        JobState::Done => {
                            let mut text = if j.is_dir { "Done".to_string() } else { format!("Done · {}", listing::human_size(j.size)) };
                            if let Some(t) = j.took {
                                text.push_str(&format!(" in {}", fmt_secs(t.as_secs_f64())));
                                if !j.is_dir && t.as_secs_f64() > 0.2 {
                                    text.push_str(&format!(" · {}/s", listing::human_size((j.size as f64 / t.as_secs_f64()) as u64)));
                                }
                            }
                            ui.label(egui::RichText::new(text).color(status(&ctx).success));
                        }
                        JobState::Failed(e) => {
                            if self.transfer.is_some() && ui.add(egui::Button::new("Retry").corner_radius(r_sm())).clicked() {
                                retry = Some(idx);
                            }
                            ui.label(egui::RichText::new("Failed").color(status(&ctx).danger)).on_hover_text(e);
                        }
                        JobState::Waiting if Some(j.id) == running => {
                            let elapsed = j.started.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
                            match (j.local_target.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()), progress.get(&j.id)) {
                                // Downloads: how much has arrived, the speed and the time left.
                                (Some(n), Some(&(_, speed))) if j.size > 0 => {
                                    let mut text = format!("{} of {}", listing::human_size(n.min(j.size)), listing::human_size(j.size));
                                    if speed > 0.0 {
                                        text.push_str(&format!(" · {}/s · {} left", listing::human_size(speed as u64), fmt_secs((j.size.saturating_sub(n)) as f64 / speed)));
                                    }
                                    ui.label(egui::RichText::new(text).weak());
                                    ui.add(egui::ProgressBar::new((n as f32 / j.size as f32).clamp(0.0, 1.0)).desired_width(140.0).corner_radius(r_sm()));
                                }
                                // Uploads and folders: sftp doesn't report how far it got, so the time so far.
                                _ => {
                                    let what = if j.dir == Dir::Up { "Uploading" } else { "Downloading" };
                                    let size = if j.is_dir { String::new() } else { format!(" {}", listing::human_size(j.size)) };
                                    ui.label(egui::RichText::new(format!("{what}{size} · {}", fmt_secs(elapsed))).weak());
                                    ui.add(egui::Spinner::new().size(14.0));
                                }
                            }
                        }
                        JobState::Waiting => {
                            ui.label(egui::RichText::new("Waiting").weak());
                        }
                    });
                });
            }
        });
        if let Some(i) = retry {
            self.retry_job(i);
        }
    }
}

impl FtpApp {
    /// A symbolic icon from the icon theme, for tinting.
    pub(super) fn icon_named(&mut self, ctx: &egui::Context, names: &[&str]) -> Option<egui::TextureHandle> {
        self.named_icon(ctx, names)
    }

    /// A small flat button with a symbolic icon (or `fallback` text without one), like
    /// Rivet's toolbar buttons.
    fn icon_button(&mut self, ui: &mut egui::Ui, names: &[&str], fallback: &str, tip: &str) -> egui::Response {
        let ctx = ui.ctx().clone();
        let fg = ui.visuals().text_color();
        let b = match self.named_icon(&ctx, names) {
            Some(t) => egui::Button::image(egui::Image::from_texture(&t).fit_to_exact_size(Vec2::splat(16.0)).tint(fg)),
            None => egui::Button::new(fallback),
        };
        ui.add(b.frame_when_inactive(false).min_size(Vec2::new(26.0, 24.0)).corner_radius(r_sm())).on_hover_text(tip)
    }
}


/// A category's header in the sidebar, like Rivet's section headers: a triangle that points
/// down when open, the name, and how many connections are in it (a dot when one is connected).
fn category_header(ui: &mut egui::Ui, name: &str, count: usize, open: bool, connected: bool) -> egui::Response {
    ui.add_space(8.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
    let v = ui.visuals();
    let col = if resp.hovered() { v.strong_text_color() } else { v.text_color() };
    let c = Pos2::new(rect.left() + 10.0, rect.center().y);
    let pts = if open {
        vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)]
    } else {
        vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, 4.0), c + Vec2::new(3.0, 0.0)]
    };
    ui.painter().add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
    let right = rect.right() - 8.0;
    let n = ui.painter().text(Pos2::new(right, rect.center().y), Align2::RIGHT_CENTER, count.to_string(), fonts::ui_font(12.0), v.weak_text_color());
    if connected && !open {
        ui.painter().circle_filled(Pos2::new(n.left() - 10.0, rect.center().y), 3.5, status(ui.ctx()).success);
    }
    let text_rect = Rect::from_min_max(Pos2::new(rect.left() + 22.0, rect.top()), Pos2::new(n.left() - 18.0, rect.bottom()));
    ui.painter().with_clip_rect(text_rect).text(Pos2::new(text_rect.left(), rect.center().y), Align2::LEFT_CENTER, name, fonts::ui_font(13.5), col);
    ui.add_space(1.0);
    resp.on_hover_text(if open { "Click to fold up · right-click for more" } else { "Click to open · right-click for more" })
}

/// Everything about a saved connection, for its tooltip.
fn connection_tooltip(ui: &mut egui::Ui, c: &rivet_connections::Connection) {
    let weak = ui.visuals().weak_text_color();
    ui.label(egui::RichText::new(c.title()).strong());
    ui.add_space(2.0);
    egui::Grid::new(("conn-tip", c.name.as_str())).num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
        let mut row = |k: &str, v: String| {
            ui.label(egui::RichText::new(k).color(weak));
            ui.label(v);
            ui.end_row();
        };
        row("Server", c.target());
        row("Type", format!("{}, port {}", c.protocol.scheme().to_uppercase(), c.port()));
        if let Some(k) = &c.key {
            row("SSH key", std::path::Path::new(k).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| k.clone()));
        }
        if let Some(r) = &c.remote {
            row("Server folder", r.clone());
        }
        if let Some(l) = &c.local {
            row("Local folder", l.clone());
        }
    });
    ui.add_space(2.0);
    ui.label(egui::RichText::new(c.url()).small().color(weak));
}

/// 75 → "1 min 15 s", 4 → "4 s", 0.3 → "under a second".
fn fmt_secs(s: f64) -> String {
    if !s.is_finite() || s < 1.0 {
        return "under a second".into();
    }
    let s = s.round() as u64;
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} min {} s", s / 60, s % 60),
        _ => format!("{} h {} min", s / 3600, (s % 3600) / 60),
    }
}

/// A local path with the home folder as ~.
fn tilde_path(p: &str) -> String {
    let home = rivet_xdg::places::home().to_string_lossy().into_owned();
    match p.strip_prefix(&home) {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
        _ => p.to_string(),
    }
}

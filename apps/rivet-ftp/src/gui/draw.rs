//! Drawing: the toolbar, the connection list, the two panes and the transfer queue.

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_connections::{Connection, Protocol};
use rivet_look::fonts;
use rivet_look::theme::{r_lg, r_md, r_sm, status};

use super::logic::{expand, first_line};
use super::menus::menu_style;
use super::*;
use crate::listing;
use crate::sftp;

const ROW_H: f32 = 26.0;

impl FtpApp {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            // With the menu bar hidden, its menus live behind a button (like Rivet's).
            if !self.s.show_menubar {
                let tex = self.named_icon(&ctx, &["open-menu-symbolic", "application-menu-symbolic", "view-more-symbolic"]);
                let b = match &tex {
                    Some(t) => egui::Button::image(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(16.0)).tint(v.text_color())),
                    None => egui::Button::new("Menu"),
                };
                egui::containers::menu::MenuButton::from_button(b.min_size(Vec2::new(30.0, 30.0)).corner_radius(r_sm())).ui(ui, |ui| {
                    menu_style(ui);
                    self.menus(ui);
                });
            }
            if !self.s.show_sidebar && self.icon_button(ui, &["sidebar-show-symbolic", "view-dual-symbolic"], "Connections", "Show saved connections (F9)").clicked() {
                self.s.show_sidebar = true;
                self.s.save();
            }
            // Quick connect: an address, used without saving it. Same height and surface as
            // Rivet's location bar.
            let field_w = (ui.available_width() - 320.0).clamp(240.0, 560.0);
            let mut go = false;
            let server = self.named_icon(&ctx, &["network-server-symbolic", "network-workgroup-symbolic"]);
            egui::Frame::new().fill(v.text_edit_bg_color()).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
                ui.set_width(field_w);
                ui.horizontal(|ui| {
                    if let Some(t) = &server {
                        ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(14.0)).tint(v.weak_text_color()));
                    }
                    let r = ui.add(egui::TextEdit::singleline(&mut self.quick).id(egui::Id::unique("quick-connect")).frame(egui::Frame::NONE).hint_text("user@server  or  sftp://user@server:port/folder").desired_width(f32::INFINITY));
                    if self.focus_quick {
                        self.focus_quick = false;
                        r.request_focus();
                    }
                    go = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                });
            });
            let busy = self.connecting;
            let can = !busy && !self.quick.trim().is_empty();
            let (fill, fg) = if can { (v.selection.bg_fill, v.selection.stroke.color) } else { (v.widgets.inactive.weak_bg_fill, v.weak_text_color()) };
            let connect = egui::Button::new(egui::RichText::new("Connect").color(fg)).fill(fill).corner_radius(r_md()).min_size(Vec2::new(84.0, 32.0));
            if ui.add_enabled(can, connect).on_hover_text("Connect without saving (Ctrl+K to type an address)").clicked() || (go && can) {
                match Connection::from_url("Quick connect", self.quick.trim()) {
                    Ok(c) => self.connect(c, &ctx),
                    Err(e) => self.note = Some((e, true)),
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.conn.is_some() {
                    let tex = self.named_icon(&ctx, &["network-offline-symbolic", "process-stop-symbolic", "window-close-symbolic"]);
                    let b = match &tex {
                        Some(t) => egui::Button::image_and_text(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(14.0)).tint(v.text_color()), "Disconnect"),
                        None => egui::Button::new("Disconnect"),
                    };
                    if ui.add(b.corner_radius(r_md()).min_size(Vec2::new(0.0, 32.0))).on_hover_text("Disconnect (Ctrl+Shift+D)").clicked() {
                        self.run(super::menus::Cmd::Disconnect, &ctx);
                    }
                    // A quick connection can be kept.
                    if let Some(c) = self.conn.clone().filter(|c| self.store.find(&c.name).is_none())
                        && ui.add(egui::Button::new("Save…").corner_radius(r_md()).min_size(Vec2::new(0.0, 32.0))).on_hover_text("Save this connection").clicked()
                    {
                        let mut f = Form::from(&c);
                        f.original = None;
                        f.name = c.host.clone();
                        self.form = Some(f);
                    }
                }
                // Where we are: a pill with a status dot.
                let (dot, text) = match (&self.conn, self.connecting) {
                    (Some(c), true) => (status(&ctx).warning, format!("Connecting to {}…", c.name)),
                    (Some(c), false) => (status(&ctx).success, c.name.clone()),
                    (None, _) => (v.weak_text_color(), "Not connected".to_string()),
                };
                let galley = ui.painter().layout_no_wrap(text, fonts::ui_font(13.0), v.text_color());
                let (r, _) = ui.allocate_exact_size(Vec2::new(galley.size().x + 34.0, 30.0), Sense::hover());
                ui.painter().rect_filled(r, r_md(), v.faint_bg_color);
                ui.painter().circle_filled(Pos2::new(r.left() + 14.0, r.center().y), 4.0, dot);
                ui.painter().galley(Pos2::new(r.left() + 26.0, r.center().y - galley.size().y / 2.0), galley, v.text_color());
            });
        });
    }

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
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            for c in &items {
                let current = self.conn.as_ref().is_some_and(|k| k.name == c.name);
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::click());
                let v = ui.visuals();
                if current {
                    ui.painter().rect_filled(rect, r_md(), v.selection.bg_fill);
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, r_md(), v.widgets.hovered.weak_bg_fill);
                }
                let fg = if current { v.selection.stroke.color } else { v.text_color() };
                let weak = if current { fg.gamma_multiply(0.8) } else { v.weak_text_color() };
                if let Some(t) = &server_icon {
                    let r = Rect::from_center_size(Pos2::new(rect.left() + 18.0, rect.center().y), Vec2::splat(16.0));
                    ui.painter().image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                let x = rect.left() + 36.0;
                ui.painter().text(Pos2::new(x, rect.top() + 13.0), Align2::LEFT_CENTER, &c.name, fonts::ui_font(13.5), fg);
                let mut addr = c.target();
                if c.protocol != Protocol::Sftp {
                    addr = format!("{} · {addr}", c.protocol.scheme().to_uppercase());
                }
                ui.painter().text(Pos2::new(x, rect.top() + 29.0), Align2::LEFT_CENTER, addr, fonts::ui_font(11.5), weak);
                let resp = resp.on_hover_text(c.url());
                if resp.clicked() && !current {
                    self.connect(c.clone(), &ctx);
                }
                resp.context_menu(|ui| {
                    menu_style(ui);
                    if ui.button("Connect").clicked() {
                        self.connect(c.clone(), &ctx);
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
        });
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
            Side::Remote => self.conn.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "Server".into()),
        };
        let live = side == Side::Local || (self.browse.is_some() && !self.connecting);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).strong());
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
        let mut menu = false;
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
                    if !selected {
                        clicked = Some((i, egui::Modifiers::NONE));
                    }
                    menu = true;
                }
                if resp.drag_started() {
                    drag_start = Some(i);
                }
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
        if menu {
            self.focus = side;
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

    pub(super) fn pane_menu(&mut self, ui: &mut egui::Ui, side: Side) {
        menu_style(ui);
        let sel = self.pane_ref(side).selected_entries();
        let connected = self.transfer.is_some();
        if !sel.is_empty() {
            let label = match (side, sel.len()) {
                (Side::Local, 1) => format!("Upload \"{}\"", sel[0].name),
                (Side::Local, n) => format!("Upload {n} items"),
                (Side::Remote, 1) => format!("Download \"{}\"", sel[0].name),
                (Side::Remote, n) => format!("Download {n} items"),
            };
            if ui.add_enabled(connected, egui::Button::new(label)).clicked() {
                self.transfer_items(side, sel.clone());
                ui.close();
            }
            ui.separator();
            if sel.len() == 1 && ui.button("Rename…").clicked() {
                self.prompt = Some(Prompt::Rename(side, sel[0].name.clone(), sel[0].name.clone()));
                ui.close();
            }
            if side == Side::Remote && sel.len() == 1 && !sel[0].is_dir && connected {
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
                ui.separator();
            }
            if side == Side::Remote && ui.button("Delete…").clicked() {
                let ctx = ui.ctx().clone();
                self.run(super::menus::Cmd::Delete, &ctx);
                ui.close();
            }
            if side == Side::Remote && sel.len() == 1 && ui.button("Copy address").clicked() {
                if let Some(c) = &self.conn {
                    let mut c2 = c.clone();
                    c2.remote = Some(sftp::join(&self.remote.path, &sel[0].name));
                    ui.ctx().copy_text(c2.url());
                }
                ui.close();
            }
            ui.separator();
        }
        let can = side == Side::Local || connected;
        if ui.add_enabled(can, egui::Button::new("New folder…")).clicked() {
            self.prompt = Some(Prompt::NewFolder(side, String::new()));
            ui.close();
        }
        if ui.add_enabled(can, egui::Button::new("Refresh")).clicked() {
            self.reload(side);
            ui.close();
        }
        let mut hidden = self.s.show_hidden;
        if ui.checkbox(&mut hidden, "Show hidden files").clicked() {
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
        let mut arrows = std::collections::HashMap::new();
        for n in [["go-up-symbolic", "go-up"], ["go-down-symbolic", "go-down"]] {
            arrows.insert(n[0], self.icon_named(&ctx, &n));
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.queue.is_empty() {
                ui.label(egui::RichText::new("Drag files between the panes, double-click a file, or use Upload and Download.").weak());
            }
            for j in self.queue.iter().rev() {
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
                    ui.label(&j.name).on_hover_text(&j.to);
                    ui.label(egui::RichText::new(if j.is_dir { "folder".to_string() } else { listing::human_size(j.size) }).weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match &j.state {
                        JobState::Done => {
                            ui.label(egui::RichText::new("Done").color(status(&ctx).success));
                        }
                        JobState::Failed(e) => {
                            ui.label(egui::RichText::new("Failed").color(status(&ctx).danger)).on_hover_text(e);
                        }
                        JobState::Waiting if Some(j.id) == running => {
                            // Downloads show how much has arrived so far.
                            match j.local_target.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()) {
                                Some(n) if j.size > 0 => {
                                    ui.add(egui::ProgressBar::new((n as f32 / j.size as f32).clamp(0.0, 1.0)).desired_width(140.0).corner_radius(r_sm()));
                                }
                                _ => {
                                    ui.add(egui::Spinner::new().size(14.0));
                                    ui.label(egui::RichText::new(if j.dir == Dir::Up { "Uploading" } else { "Downloading" }).weak());
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


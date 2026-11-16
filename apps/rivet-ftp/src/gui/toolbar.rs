//! The toolbar, laid out from the person's list like Rivet's: buttons on the left, the
//! quick-connect field taking the room in the middle, more buttons on the right. Right-click it
//! to customize. With the menu bar hidden, a menu button holds every menu.

use eframe::egui::{self, Color32, Key, Pos2, Sense, Stroke, Vec2};
use rivet_connections::Connection;
use rivet_look::fonts;
use rivet_look::theme::{r_md, r_sm, selected_tile, status};

use super::menus::{Cmd, menu_style};
use super::*;
use crate::settings::Tool;

impl FtpApp {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut layout = self.s.toolbar.clone();
        // The menus must stay reachable: without a menu bar, the menu button is always there.
        if !self.s.show_menubar && !layout.contains(&Tool::Menu) {
            layout.insert(0, Tool::Menu);
        }
        let split = layout.iter().position(|t| *t == Tool::QuickConnect).unwrap_or(layout.len());
        let (left, right) = (layout[..split].to_vec(), layout[(split + 1).min(layout.len())..].to_vec());
        let bg = ui.interact(ui.max_rect(), egui::Id::unique("ftp-toolbar-bg"), Sense::click());
        bg.context_menu(|ui| {
            menu_style(ui);
            if ui.button("Customize Toolbar…").clicked() {
                self.prefs_page = super::prefs::Page::Toolbar;
                self.prefs_open = true;
                ui.close();
            }
            if ui.button("Reset Toolbar").clicked() {
                self.s.toolbar = Tool::default_layout();
                self.s.save();
                ui.close();
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            for t in &left {
                self.tool(ui, &ctx, *t);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Right to left: drawn in reverse so the saved order reads left to right; the
                // field, drawn last, takes whatever room is left.
                for t in right.iter().rev() {
                    self.tool(ui, &ctx, *t);
                }
                if split < layout.len() {
                    self.quick_connect(ui, &ctx);
                }
            });
        });
    }

    /// The quick-connect field and its Connect button (drawn right to left).
    fn quick_connect(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let v = ui.visuals().clone();
        let busy = self.connecting;
        let can = !busy && !self.quick.trim().is_empty();
        let (fill, fg) = if can { (v.selection.bg_fill, v.selection.stroke.color) } else { (v.widgets.inactive.weak_bg_fill, v.weak_text_color()) };
        let connect = egui::Button::new(egui::RichText::new("Connect").color(fg)).fill(fill).corner_radius(r_md()).min_size(Vec2::new(80.0, 32.0));
        let clicked = ui.add_enabled(can, connect).on_hover_text("Connect without saving (Ctrl+K to type an address)").clicked();
        // A link: the field takes an address (the server icon is the Saved Connections button).
        let server = self.named_icon(ctx, &["insert-link-symbolic", "emblem-symbolic-link", "network-workgroup-symbolic"]);
        let mut go = false;
        // The same raised surface as Rivet's location bar.
        let elevated = rivet_look::palette::mix(v.window_fill, v.text_color(), 0.04);
        let shadow = egui::Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(if v.dark_mode { 90 } else { 40 }) };
        let w = ui.available_width().max(160.0);
        ui.allocate_ui_with_layout(Vec2::new(w, 32.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            egui::Frame::NONE.fill(elevated).shadow(shadow).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
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
        });
        if clicked || (go && can) {
            match Connection::from_url("Quick connect", self.quick.trim()) {
                Ok(c) => self.connect(c, ctx),
                Err(e) => self.note = Some((e, true)),
            }
        }
    }

    /// One toolbar item.
    fn tool(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, t: Tool) {
        let connected = self.browse.is_some() && !self.connecting;
        let focus = self.focus;
        match t {
            Tool::QuickConnect => {}
            Tool::Separator => {
                let (r, _) = ui.allocate_exact_size(Vec2::new(8.0, 22.0), Sense::hover());
                let c = ui.visuals().weak_text_color().gamma_multiply(0.5);
                ui.painter().line_segment([Pos2::new(r.center().x, r.top()), Pos2::new(r.center().x, r.bottom())], Stroke::new(1.0, c));
            }
            Tool::Menu => {
                if self.s.show_menubar {
                    return;
                }
                self.mini_menu(ui);
            }
            Tool::Status => self.status_pill(ui, ctx),
            Tool::Disconnect => {
                if self.tool_button(ui, t, self.conn.is_some(), false, "Disconnect (Shift+Ctrl+D)").clicked() {
                    self.run(Cmd::Disconnect, ctx);
                }
            }
            Tool::Connections => {
                if self.tool_button(ui, t, true, self.s.show_sidebar, "Saved connections (F9)").clicked() {
                    self.run(Cmd::ToggleSidebar, ctx);
                }
            }
            Tool::Transfers => {
                if self.tool_button(ui, t, true, self.s.show_queue, "Transfers (F8)").clicked() {
                    self.run(Cmd::ToggleQueue, ctx);
                }
            }
            Tool::ShowHidden => {
                if self.tool_button(ui, t, true, self.s.show_hidden, "Show hidden files (Ctrl+H)").clicked() {
                    self.run(Cmd::ToggleHidden, ctx);
                }
            }
            Tool::NewConnection => {
                if self.tool_button(ui, t, true, false, "New connection (Ctrl+N)").clicked() {
                    self.run(Cmd::NewConnection, ctx);
                }
            }
            Tool::Up => {
                if self.tool_button(ui, t, focus == Side::Local || connected, false, "Up a folder in the active pane (Backspace)").clicked() {
                    self.run(Cmd::Up, ctx);
                }
            }
            Tool::Refresh => {
                if self.tool_button(ui, t, focus == Side::Local || connected, false, "Reload the active pane (F5)").clicked() {
                    self.run(Cmd::Refresh, ctx);
                }
            }
            Tool::Home => {
                if self.tool_button(ui, t, true, false, "Your home folder (Alt+Home)").clicked() {
                    self.run(Cmd::LocalHome, ctx);
                }
            }
            Tool::ServerHome => {
                if self.tool_button(ui, t, connected, false, "The server's home folder").clicked() {
                    self.run(Cmd::ServerHome, ctx);
                }
            }
            Tool::NewFolder => {
                if self.tool_button(ui, t, focus == Side::Local || connected, false, "New folder in the active pane (Shift+Ctrl+N)").clicked() {
                    self.run(Cmd::NewFolder, ctx);
                }
            }
            Tool::Upload => {
                let can = connected && !self.local.selected.is_empty();
                if self.tool_button(ui, t, can, false, "Upload the selection (Ctrl+U)").clicked() {
                    self.run(Cmd::Upload, ctx);
                }
            }
            Tool::Download => {
                let can = connected && !self.remote.selected.is_empty();
                if self.tool_button(ui, t, can, false, "Download the selection (Ctrl+D)").clicked() {
                    self.run(Cmd::Download, ctx);
                }
            }
            Tool::EditRemote => {
                let can = connected && self.remote.selected.len() == 1 && self.remote.selected_entries().first().is_some_and(|e| !e.is_dir);
                if self.tool_button(ui, t, can, false, "Open the server file in an editor (F4)").clicked() {
                    self.focus = Side::Remote;
                    self.run(Cmd::EditRemote, ctx);
                }
            }
            Tool::Info => {
                let can = !self.pane_ref(focus).selected.is_empty() && (focus == Side::Local || connected);
                if self.tool_button(ui, t, can, false, "Info and permissions (Ctrl+I)").clicked() {
                    self.run(Cmd::Info, ctx);
                }
            }
            Tool::Preferences => {
                if self.tool_button(ui, t, true, self.prefs_open, "Preferences (Ctrl+,)").clicked() {
                    self.run(Cmd::Preferences, ctx);
                }
            }
        }
    }

    /// A flat icon button like Rivet's toolbar buttons; `on` shows it as switched on.
    fn tool_button(&mut self, ui: &mut egui::Ui, t: Tool, enabled: bool, on: bool, tip: &str) -> egui::Response {
        let (names, fallback) = t.icons();
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let fg = v.text_color();
        let b = match self.named_icon(&ctx, names) {
            Some(tex) => egui::Button::image(egui::Image::from_texture(&tex).fit_to_exact_size(Vec2::splat(16.0)).tint(fg)),
            None => egui::Button::new(fallback),
        };
        // A switched-on button sits on the same tile as a selected item.
        let under = ui.painter().add(egui::Shape::Noop);
        let resp = ui.add_enabled(enabled, b.frame_when_inactive(false).min_size(Vec2::new(34.0, 32.0)).corner_radius(r_sm()));
        if on {
            let (fill, stroke) = selected_tile(&v, v.panel_fill);
            ui.painter().set(under, egui::Shape::Vec(vec![egui::Shape::rect_filled(resp.rect, r_sm(), fill), egui::Shape::rect_stroke(resp.rect, r_sm(), stroke, egui::StrokeKind::Inside)]));
        }
        resp.on_hover_text(tip).on_disabled_hover_text(tip)
    }

    /// Where we are: a pill with a status dot.
    fn status_pill(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let v = ui.visuals().clone();
        let (dot, text) = match (&self.conn, self.connecting) {
            (Some(c), true) => (status(ctx).warning, format!("Connecting to {}…", c.name)),
            (Some(c), false) => (status(ctx).success, c.name.clone()),
            (None, _) => (v.weak_text_color(), "Not connected".to_string()),
        };
        let galley = ui.painter().layout_no_wrap(text, fonts::ui_font(13.0), v.text_color());
        let (r, resp) = ui.allocate_exact_size(Vec2::new(galley.size().x + 34.0, 30.0), Sense::hover());
        ui.painter().rect_filled(r, r_md(), v.faint_bg_color);
        ui.painter().circle_filled(Pos2::new(r.left() + 14.0, r.center().y), 4.0, dot);
        ui.painter().galley(Pos2::new(r.left() + 26.0, r.center().y - galley.size().y / 2.0), galley, v.text_color());
        if let Some(c) = &self.conn {
            resp.on_hover_text(c.url());
        }
    }

    /// The menu bar folded into one button (three bars, the same in every icon theme), like
    /// Rivet's: Show Menubar first, then every menu as a submenu.
    fn mini_menu(&mut self, ui: &mut egui::Ui) {
        let fg = ui.visuals().text_color();
        let btn = egui::Button::new("").min_size(Vec2::new(34.0, 32.0)).frame_when_inactive(false).corner_radius(r_sm());
        let r = egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui| {
            menu_style(ui);
            ui.spacing_mut().button_padding = Vec2::new(10.0, 6.0);
            ui.spacing_mut().item_spacing.y = 4.0;
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
        r.0.on_hover_text("Menu (Ctrl+M shows the menu bar again)");
    }
}

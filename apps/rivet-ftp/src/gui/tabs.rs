//! Tabs: one connection each, with its own two panes and transfer list. The open tab lives
//! in `FtpApp`'s own fields (so the rest of the window code needn't know about tabs); the
//! others wait in `tabs` and are swapped in when chosen. Background tabs keep working: their
//! transfers finish and their replies are handled every frame.

use std::collections::HashMap;

use eframe::egui::{self, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_connections::Connection;
use eframe::egui::accesskit::Role;
use rivet_look::a11y::Access;
use rivet_look::fonts;
use rivet_look::theme::{r_sm, status};

use super::*;

/// Everything that belongs to one tab.
#[derive(Default)]
pub(super) struct ConnTab {
    conn: Option<Connection>,
    browse: Option<Session>,
    transfer: Option<Session>,
    connecting: bool,
    pending: HashMap<u64, String>,
    list_req: Option<u64>,
    local: Pane,
    remote: Pane,
    remote_home: Option<String>,
    secret: Option<crate::sftp::Secret>,
    remember: Vec<(String, rivet_connections::secrets::Kind, String)>,
    queue: Vec<Job>,
    focus: Option<Side>,
    note: Option<(String, bool)>,
    /// A question that came up while the tab was in the background (shown when it's opened).
    asked: Option<Prompt>,
}

impl ConnTab {
    pub(super) fn conn_name(&self) -> Option<String> {
        self.conn.as_ref().map(|c| c.name.clone())
    }
}

impl FtpApp {
    /// Swap tab `i`'s state with the window's (twice gives back what was there).
    fn exchange(&mut self, i: usize) {
        let t = &mut self.tabs[i];
        std::mem::swap(&mut self.conn, &mut t.conn);
        std::mem::swap(&mut self.browse, &mut t.browse);
        std::mem::swap(&mut self.transfer, &mut t.transfer);
        std::mem::swap(&mut self.connecting, &mut t.connecting);
        std::mem::swap(&mut self.pending, &mut t.pending);
        std::mem::swap(&mut self.list_req, &mut t.list_req);
        std::mem::swap(&mut self.local, &mut t.local);
        std::mem::swap(&mut self.remote, &mut t.remote);
        std::mem::swap(&mut self.remote_home, &mut t.remote_home);
        std::mem::swap(&mut self.secret, &mut t.secret);
        std::mem::swap(&mut self.remember, &mut t.remember);
        std::mem::swap(&mut self.queue, &mut t.queue);
        std::mem::swap(&mut self.note, &mut t.note);
        let f = t.focus.take().unwrap_or(Side::Local);
        t.focus = Some(std::mem::replace(&mut self.focus, f));
    }

    pub(super) fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Show tab `j`.
    pub(super) fn switch_tab(&mut self, j: usize) {
        if j == self.active || j >= self.tabs.len() {
            return;
        }
        // The light box and dialogs belong to the tab they were opened in.
        self.preview = None;
        self.prompt = None;
        self.exchange(self.active);
        self.exchange(j);
        self.active = j;
        // A question it asked while in the background.
        if let Some(p) = self.tabs[j].asked.take() {
            self.prompt = Some(p);
        }
    }

    /// A new tab (not connected), starting in the same local folder as this one.
    pub(super) fn new_tab(&mut self) {
        let path = self.local.path.clone();
        self.tabs.push(ConnTab::default());
        let j = self.tabs.len() - 1;
        self.switch_tab(j);
        self.local.path = path;
        self.reload_local();
    }

    pub(super) fn close_tab(&mut self, i: usize) {
        if self.tabs.len() <= 1 {
            // The last tab stays; closing it just disconnects.
            self.disconnect();
            return;
        }
        if i == self.active {
            self.disconnect();
            self.preview = None;
            self.prompt = None;
            // Bring a neighbour in, then drop the old slot.
            let j = if i + 1 < self.tabs.len() { i + 1 } else { i - 1 };
            self.exchange(j);
            self.tabs.remove(i);
            self.active = if j > i { j - 1 } else { j };
        } else {
            // Its sessions close when it's dropped.
            self.tabs.remove(i);
            if i < self.active {
                self.active -= 1;
            }
        }
    }

    /// Connect from the sidebar or a menu: switch to it if it's already open in a tab; use
    /// this tab if it's free; otherwise open a new tab for it.
    pub(super) fn open_connection(&mut self, c: Connection, ctx: &egui::Context, new_tab: bool) {
        if let Some(i) = self.tab_with(&c.name) {
            self.switch_tab(i);
            return;
        }
        if new_tab || self.conn.is_some() {
            self.new_tab();
        }
        self.connect(c, ctx);
    }

    /// The tab with this saved connection open.
    pub(super) fn tab_with(&self, name: &str) -> Option<usize> {
        if self.conn.as_ref().is_some_and(|c| c.name == name) {
            return Some(self.active);
        }
        self.tabs.iter().position(|t| t.conn.as_ref().is_some_and(|c| c.name == name))
    }

    /// Let the background tabs handle their replies (transfers finishing, logins, drops).
    pub(super) fn poll_background(&mut self, ctx: &egui::Context) {
        for i in 0..self.tabs.len() {
            if i == self.active || self.tabs[i].browse.is_none() && self.tabs[i].transfer.is_none() {
                continue;
            }
            let prompt = self.prompt.take();
            self.exchange(i);
            self.in_background = true;
            self.poll(ctx);
            self.watch_edits(ctx);
            self.in_background = false;
            // A question raised in the background (a password, replace?) waits for its tab.
            let asked = self.prompt.take();
            self.exchange(i);
            self.prompt = prompt;
            if asked.is_some() {
                self.tabs[i].asked = asked;
            }
        }
    }

    /// The tab strip, shown when there's more than one tab (like Rivet's).
    pub(super) fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut pick = None;
        let mut close = None;
        let mut add = false;
        ui.scope_builder(rivet_look::a11y::area(Role::TabList, "Connections"), |ui| ui.horizontal(|ui| {
            let n = self.tabs.len();
            let w = ((ui.available_width() - 40.0) / n as f32).clamp(90.0, 240.0);
            for i in 0..n {
                let sel = i == self.active;
                let (conn, connecting, waiting, asking) = if sel {
                    (self.conn.clone(), self.connecting, self.queue.iter().filter(|j| j.state == JobState::Waiting).count(), false)
                } else {
                    let t = &self.tabs[i];
                    (t.conn.clone(), t.connecting, t.queue.iter().filter(|j| j.state == JobState::Waiting).count(), t.asked.is_some())
                };
                let title = conn.as_ref().map(|c| c.title()).unwrap_or_else(|| "Not connected".into());
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
                let resp = resp.role(Role::Tab, title.as_str()).selected(sel);
                let v = ui.visuals().clone();
                let bg = if sel { v.extreme_bg_color } else if resp.hovered() { v.widgets.hovered.weak_bg_fill } else { v.panel_fill };
                ui.painter().rect_filled(rect, CornerRadius { nw: r_sm(), ne: r_sm(), sw: 0, se: 0 }, bg);
                if sel {
                    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(2.0, v.selection.bg_fill));
                }
                // The connection's state, as in the server pane's heading.
                let dot = match (&conn, connecting || asking) {
                    (Some(_), true) => status(&ctx).warning,
                    (Some(_), false) => status(&ctx).success,
                    (None, _) => v.weak_text_color(),
                };
                ui.painter().circle_filled(Pos2::new(rect.left() + 13.0, rect.center().y), 3.5, dot);
                let close_rect = Rect::from_center_size(Pos2::new(rect.right() - 14.0, rect.center().y), Vec2::splat(16.0));
                let col = if sel { v.strong_text_color() } else { v.text_color() };
                // Background transfers still running: a count before the close button.
                let mut right = close_rect.left() - 4.0;
                if waiting > 0 && !sel {
                    let g = ui.painter().layout_no_wrap(waiting.to_string(), fonts::ui_font(11.5), v.selection.bg_fill);
                    right -= g.size().x;
                    ui.painter().galley(Pos2::new(right, rect.center().y - g.size().y / 2.0), g, v.selection.bg_fill);
                    right -= 6.0;
                }
                let mut job = egui::text::LayoutJob::simple_singleline(title.clone(), fonts::ui_font(13.0), col);
                job.wrap = egui::text::TextWrapping::truncate_at_width(right - rect.left() - 24.0);
                let galley = ui.painter().layout_job(job);
                ui.painter().galley(Pos2::new(rect.left() + 22.0, rect.center().y - galley.size().y / 2.0), galley, col);
                let cl = ui.interact(close_rect, Id::unique(("ftp-tabclose", i)), Sense::click()).role(Role::Button, format!("Close {title}"));
                if cl.hovered() {
                    ui.painter().circle_filled(close_rect.center(), 8.0, v.widgets.hovered.bg_fill);
                }
                let c = close_rect.center();
                let s = Stroke::new(1.2, col);
                ui.painter().line_segment([c + Vec2::new(-3.5, -3.5), c + Vec2::new(3.5, 3.5)], s);
                ui.painter().line_segment([c + Vec2::new(3.5, -3.5), c + Vec2::new(-3.5, 3.5)], s);
                let mut tip = match &conn {
                    Some(c) => format!("{}\n{}", c.title(), c.url()),
                    None => "Not connected".into(),
                };
                if asking {
                    tip.push_str("\nWaiting for your answer: open the tab");
                }
                if waiting > 0 && !sel {
                    tip.push_str(&format!("\n{waiting} transfer{} still going", if waiting == 1 { "" } else { "s" }));
                }
                let resp = resp.on_hover_text(tip);
                if cl.clicked() || resp.clicked_by(egui::PointerButton::Middle) {
                    close = Some(i);
                } else if resp.clicked() {
                    pick = Some(i);
                }
            }
            if ui.button("+").on_hover_text("New tab (Ctrl+T)").role(Role::Button, "New tab").clicked() {
                add = true;
            }
        }));
        if let Some(i) = close {
            self.close_tab(i);
        } else if let Some(i) = pick {
            self.switch_tab(i);
        } else if add {
            self.new_tab();
        }
    }
}

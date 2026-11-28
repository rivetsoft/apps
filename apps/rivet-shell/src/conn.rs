//! The Network and Bluetooth popups, opened from their panel buttons (and from the Wi-Fi and
//! Bluetooth tiles in quick settings): a title row with an on/off switch, the networks or
//! devices as a list in the quick settings' look, and a way out to the full settings app.

use rivet_look::a11y::{Access, Role};
use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_md, r_sm, selected_tile};

use crate::glyph;
use crate::quick::tool_button;
use crate::radio::{BtDevice, Job, WifiNet};
use crate::sys::Net;
use crate::ui::{Act, Kit, Model, PopupKind, find_app, one_line, ui_font};

const HEAD: f32 = 46.0;
const ROW: f32 = 42.0;
const SECTION: f32 = 26.0;
const ASK: f32 = 44.0;
const NOTE: f32 = 40.0;
const ERR: f32 = 34.0;
const FOOT: f32 = 34.0;
/// Rows shown before the list scrolls.
const MAX_ROWS: f32 = 7.0;

const NET_APP: &str = "nm-connection-editor";
const BT_APP: &str = "blueman-manager";

/// The popup's height for what it shows.
pub fn height(kind: &PopupKind, m: &Model) -> f32 {
    let r = &m.extra.radios;
    let mut h = 28.0 + HEAD + 6.0;
    match kind {
        PopupKind::Network { ask } => {
            if m.status.net == Net::Wired {
                h += ROW;
            }
            h += match (m.status.wifi_on, r.wifi.is_empty()) {
                (Some(true), false) => (r.wifi.len() as f32).min(MAX_ROWS) * ROW + if ask.is_some() { ASK } else { 0.0 },
                _ => NOTE,
            };
            if has_app(m, NET_APP) {
                h += FOOT;
            }
        }
        _ => {
            h += match m.status.bluetooth {
                Some(true) => {
                    let (mine, other) = split(&r.bt);
                    SECTION * 2.0 + (mine.len().max(1) as f32 * ROW + other.len().max(1) as f32 * ROW).min(MAX_ROWS * ROW)
                }
                _ => NOTE,
            };
            if has_app(m, BT_APP) {
                h += FOOT;
            }
        }
    }
    if r.error.is_some() {
        h += ERR;
    }
    h
}

/// A settings app: installed with a desktop entry, or only as a program (nm-connection-editor
/// often comes without one).
fn has_app(m: &Model, app: &str) -> bool {
    static ON_PATH: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    let on_path = ON_PATH.get_or_init(|| {
        let dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
        [NET_APP, BT_APP].into_iter().filter(|a| dirs.iter().any(|d| d.join(a).is_file())).collect()
    });
    find_app(m.apps, app).is_some() || on_path.contains(&app)
}

fn split(bt: &[BtDevice]) -> (Vec<&BtDevice>, Vec<&BtDevice>) {
    bt.iter().partition(|d| d.paired)
}

/// An on/off switch; true when clicked.
fn switch(ui: &mut egui::Ui, r: Rect, id: &str, on: bool, accent: Color32) -> bool {
    let v = ui.visuals().clone();
    let resp = ui.interact(r, egui::Id::unique(("conn-switch", id)), Sense::click()).role(Role::Switch, id).toggled(on);
    let p = ui.painter();
    let track = if on { accent } else { mix(v.panel_fill, v.text_color(), if resp.hovered() { 0.26 } else { 0.18 }) };
    p.rect_filled(r, r.height() / 2.0, track);
    let k = r.height() - 4.0;
    let x = if on { r.right() - 2.0 - k / 2.0 } else { r.left() + 2.0 + k / 2.0 };
    p.circle_filled(Pos2::new(x, r.center().y), k / 2.0, if on { crate::ui::on(accent) } else { v.panel_fill });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if on { "Turn off" } else { "Turn on" }).clicked()
}

/// A small text button at the right of a row; true when clicked.
fn text_button(ui: &mut egui::Ui, row: Rect, right: f32, id: &str, label: &str) -> (bool, f32) {
    let v = ui.visuals().clone();
    let g = ui.painter().layout_no_wrap(label.to_string(), ui_font(12.0), v.text_color());
    let br = Rect::from_min_max(Pos2::new(right - g.size().x - 16.0, row.center().y - 12.0), Pos2::new(right, row.center().y + 12.0));
    let resp = ui.interact(br, egui::Id::unique(("conn-btn", id)), Sense::click()).role(Role::Button, label);
    ui.painter().rect_filled(br, r_sm(), mix(v.panel_fill, v.text_color(), if resp.hovered() { 0.14 } else { 0.08 }));
    ui.painter().galley(br.center() - g.size() / 2.0, g, if resp.hovered() { v.strong_text_color() } else { v.text_color() });
    (resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked(), br.left())
}

/// The title row: name, a line under it, a switch, and toolbar buttons to the switch's left.
pub(crate) fn head(ui: &mut egui::Ui, title: &str, sub: &str, switch_on: Option<bool>, accent: Color32) -> (Rect, bool) {
    let v = ui.visuals().clone();
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, HEAD), Sense::hover());
    let p = ui.painter().clone();
    p.text(Pos2::new(r.left() + 2.0, r.top() + 13.0), Align2::LEFT_CENTER, title, ui_font(14.5), v.strong_text_color());
    let g = one_line(&p, sub, ui_font(12.0), v.weak_text_color(), w - 120.0);
    p.galley(Pos2::new(r.left() + 2.0, r.top() + 26.0), g, v.weak_text_color());
    let sr = Rect::from_center_size(Pos2::new(r.right() - 20.0, r.top() + 18.0), Vec2::new(36.0, 20.0));
    let flipped = switch_on.is_some_and(|on| switch(ui, sr, title, on, accent));
    p.line_segment([Pos2::new(r.left(), r.bottom() - 1.0), Pos2::new(r.right(), r.bottom() - 1.0)], Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.08)));
    ui.add_space(6.0);
    (Rect::from_center_size(Pos2::new(sr.left() - 20.0, sr.center().y), Vec2::splat(30.0)), flipped)
}

/// A line of quiet text where a list would be.
pub(crate) fn note(ui: &mut egui::Ui, text: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), NOTE), Sense::hover());
    ui.painter().text(Pos2::new(r.left() + 4.0, r.center().y), Align2::LEFT_CENTER, text, ui_font(12.5), ui.visuals().weak_text_color());
}

/// A list row's background and its two lines; returns the row and its response.
pub(crate) fn row(ui: &mut egui::Ui, selected: bool, title: &str, sub: &str, sub_col: Option<Color32>, room_right: f32) -> (Rect, egui::Response) {
    let v = ui.visuals().clone();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::click());
    let resp = resp.role(Role::ListBoxOption, title).selected(selected).described(sub).on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    if selected {
        let (fill, stroke) = selected_tile(&v, v.panel_fill);
        p.rect_filled(r.shrink2(Vec2::new(0.0, 1.0)), r_md(), fill);
        p.rect_stroke(r.shrink2(Vec2::new(0.0, 1.0)), r_md(), stroke, egui::StrokeKind::Inside);
    } else if resp.hovered() || ui.rect_contains_pointer(r) {
        p.rect_filled(r.shrink2(Vec2::new(0.0, 1.0)), r_md(), mix(v.panel_fill, v.text_color(), 0.06));
    }
    let tw = r.width() - 40.0 - room_right;
    let col = if selected { v.strong_text_color() } else { v.text_color() };
    let g = one_line(p, title, ui_font(13.0), col, tw);
    p.galley(Pos2::new(r.left() + 36.0, r.top() + 6.0), g, col);
    let weak = v.weak_text_color();
    let g = one_line(p, sub, ui_font(11.5), sub_col.unwrap_or(weak), tw);
    p.galley(Pos2::new(r.left() + 36.0, r.top() + 23.0), g, sub_col.unwrap_or(weak));
    (r, resp)
}

/// What went wrong last, and the way to the full settings app.
fn foot(ui: &mut egui::Ui, m: &mut Model, app: &str, label: &str) {
    let v = ui.visuals().clone();
    if let Some(e) = &m.extra.radios.error {
        let danger = rivet_look::theme::status(ui.ctx()).danger;
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ERR), Sense::hover());
        let g = one_line(ui.painter(), e, ui_font(12.0), danger, r.width() - 8.0);
        ui.painter().galley(Pos2::new(r.left() + 4.0, r.center().y - g.size().y / 2.0), g, danger);
    }
    if has_app(m, app) {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), FOOT), Sense::click());
        let resp = resp.role(Role::Button, "Open settings");
        let col = if resp.hovered() { v.strong_text_color() } else { v.text_color() };
        if resp.hovered() {
            ui.painter().rect_filled(r.shrink2(Vec2::new(0.0, 2.0)), r_sm(), mix(v.panel_fill, v.text_color(), 0.07));
        }
        glyph::gear(ui.painter(), Pos2::new(r.left() + 14.0, r.center().y), col);
        ui.painter().text(Pos2::new(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, label, ui_font(12.5), col);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            m.acts.push(if find_app(m.apps, app).is_some() { Act::Launch(app.to_string()) } else { Act::Spawn(app.to_string()) });
        }
    }
}

// ------------------------------------------------------------------ Wi-Fi

pub fn network(ui: &mut egui::Ui, ask: &mut Option<(String, String)>, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    ui.spacing_mut().item_spacing.y = 0.0;
    let r = m.extra.radios.clone();
    let on = m.status.wifi_on;
    let sub = match (&m.status.net, on) {
        (Net::Wifi(ssid), _) if !ssid.is_empty() => format!("Connected to {ssid}"),
        (_, None) => "Wi-Fi isn't available".into(),
        (Net::Wired, Some(false)) => "Wired · Wi-Fi off".into(),
        (_, Some(false)) => "Off".into(),
        (Net::Wired, _) => "Wired".into(),
        _ => "Not connected".into(),
    };
    let (tool, flipped) = head(ui, "Wi-Fi", &sub, on, kit.tone(m.s, crate::style::Slot::Toggle));
    let accent = kit.tone(m.s, crate::style::Slot::Active);
    if flipped {
        m.acts.push(Act::Wifi(!on.unwrap_or(false)));
    }
    if on == Some(true) && tool_button(ui, tool, "wifi-rescan", "Look for networks again", &|p, c, col| glyph::refresh(p, c, col)) {
        m.acts.push(Act::Radio(Job::WifiRescan));
    }
    if m.status.net == Net::Wired {
        let (rr, _) = row(ui, true, "Wired", "Connected", None, 0.0);
        glyph::wired(ui.painter(), Pos2::new(rr.left() + 17.0, rr.center().y), accent);
    }
    match on {
        None => return foot(ui, m, NET_APP, "Network Settings…"),
        Some(false) => note(ui, "Turn Wi-Fi on to see networks"),
        Some(true) if r.wifi.is_empty() => note(ui, if r.wifi_read { "No networks found" } else { "Looking for networks…" }),
        Some(true) => {
            let list_h = (r.wifi.len() as f32).min(MAX_ROWS) * ROW + if ask.is_some() { ASK } else { 0.0 };
            let room = (ui.available_height() - if has_app(m, NET_APP) { FOOT } else { 0.0 } - if r.error.is_some() { ERR } else { 0.0 }).max(ROW);
            egui::ScrollArea::vertical().id_salt("wifi-list").auto_shrink([false, true]).max_height(list_h.min(room)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for n in &r.wifi {
                    wifi_row(ui, n, &r.busy, ask, accent, m);
                    if ask.as_ref().is_some_and(|a| a.0 == n.ssid) {
                        password_row(ui, ask, m, &v);
                    }
                }
            });
        }
    }
    foot(ui, m, NET_APP, "Network Settings…");
}

fn wifi_row(ui: &mut egui::Ui, n: &WifiNet, busy: &Option<String>, ask: &mut Option<(String, String)>, accent: Color32, m: &mut Model) {
    let v = ui.visuals().clone();
    let joining = busy.as_deref() == Some(n.ssid.as_str());
    let sub = if joining {
        "Connecting…"
    } else if n.active {
        "Connected"
    } else if n.saved {
        "Saved"
    } else if n.secure {
        "Secured"
    } else {
        "Open"
    };
    let (r, resp) = row(ui, n.active, &n.ssid, sub, joining.then_some(accent), 90.0);
    let p = ui.painter().clone();
    glyph::wifi_signal(&p, Pos2::new(r.left() + 17.0, r.center().y), if n.active { accent } else { v.text_color() }, n.signal);
    let hovered = ui.rect_contains_pointer(r);
    let mut button_clicked = false;
    if hovered && !joining && (n.active || n.saved) {
        let (clicked, _) = text_button(ui, r, r.right() - 6.0, &format!("wifi-a-{}", n.ssid), if n.active { "Disconnect" } else { "Forget" });
        if clicked {
            button_clicked = true;
            m.acts.push(Act::Radio(if n.active { Job::WifiDisconnect } else { Job::WifiForget(n.ssid.clone()) }));
        }
    } else if n.secure {
        glyph::lock(&p, Pos2::new(r.right() - 16.0, r.center().y), v.weak_text_color());
    }
    if resp.clicked() && !button_clicked && !joining && !n.active {
        if n.saved || !n.secure {
            *ask = None;
            m.acts.push(Act::Radio(Job::WifiConnect { ssid: n.ssid.clone(), password: None }));
        } else if ask.as_ref().is_some_and(|a| a.0 == n.ssid) {
            *ask = None;
        } else {
            *ask = Some((n.ssid.clone(), String::new()));
        }
    }
}

/// The password for a new secured network, under its row.
fn password_row(ui: &mut egui::Ui, ask: &mut Option<(String, String)>, m: &mut Model, v: &egui::Visuals) {
    let Some((ssid, pw)) = ask.as_mut() else { return };
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, ASK), Sense::hover());
    let field = Rect::from_min_max(Pos2::new(r.left() + 36.0, r.top() + 6.0), Pos2::new(r.right() - 64.0, r.bottom() - 6.0));
    ui.painter().rect_filled(field, r_md(), mix(v.panel_fill, v.text_color(), 0.07));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field.shrink2(Vec2::new(10.0, 6.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let te = child.add(egui::TextEdit::singleline(pw).id(egui::Id::unique(("wifi-pw", ssid.as_str()))).password(true).frame(egui::Frame::NONE).font(ui_font(13.0)).hint_text("Password").desired_width(f32::INFINITY)).accessible_name("Password");
    if !te.has_focus() {
        te.request_focus();
    }
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    let esc = ui.input(|i| i.key_pressed(Key::Escape));
    let (join, _) = text_button(ui, r, r.right() - 4.0, "wifi-join", "Join");
    if esc {
        *ask = None;
    } else if (join || enter) && !pw.is_empty() {
        m.acts.push(Act::Radio(Job::WifiConnect { ssid: ssid.clone(), password: Some(pw.clone()) }));
        *ask = None;
    }
}

// ------------------------------------------------------------------ Bluetooth

pub fn bluetooth(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let r = m.extra.radios.clone();
    let on = m.status.bluetooth;
    let connected = r.bt.iter().filter(|d| d.connected).count();
    let sub = match on {
        None => "No Bluetooth adapter".to_string(),
        Some(false) => "Off".into(),
        Some(true) if r.bt_scanning => "Looking for devices…".into(),
        Some(true) if connected == 1 => format!("Connected to {}", r.bt.iter().find(|d| d.connected).map(|d| d.name.as_str()).unwrap_or("a device")),
        Some(true) if connected > 1 => format!("{connected} devices connected"),
        Some(true) => "On".into(),
    };
    let (tool, flipped) = head(ui, "Bluetooth", &sub, on, kit.tone(m.s, crate::style::Slot::Toggle));
    let accent = kit.tone(m.s, crate::style::Slot::Active);
    if flipped {
        m.acts.push(Act::Bluetooth(!on.unwrap_or(false)));
    }
    if on == Some(true) && !r.bt_scanning && tool_button(ui, tool, "bt-scan", "Look for devices", &|p, c, col| glyph::refresh(p, c, col)) {
        m.acts.push(Act::Radio(Job::BtScan));
    }
    match on {
        None => {}
        Some(false) => note(ui, "Turn Bluetooth on to see devices"),
        Some(true) => {
            let (mine, other) = split(&r.bt);
            let list_h = (mine.len().max(1) as f32 * ROW + other.len().max(1) as f32 * ROW).min(MAX_ROWS * ROW) + SECTION * 2.0;
            let room = (ui.available_height() - if has_app(m, BT_APP) { FOOT } else { 0.0 } - if r.error.is_some() { ERR } else { 0.0 }).max(ROW);
            egui::ScrollArea::vertical().id_salt("bt-list").auto_shrink([false, true]).max_height(list_h.min(room)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                section(ui, "My Devices");
                if mine.is_empty() {
                    note_row(ui, if r.bt_read { "None paired yet" } else { "…" });
                }
                for d in &mine {
                    bt_row(ui, d, &r.busy, accent, m);
                }
                section(ui, "Other Devices");
                if other.is_empty() {
                    note_row(ui, if r.bt_scanning || !r.bt_read { "Looking…" } else { "None found · make the device discoverable, then look again" });
                }
                for d in &other {
                    bt_row(ui, d, &r.busy, accent, m);
                }
            });
        }
    }
    foot(ui, m, BT_APP, "Bluetooth Settings…");
}

pub(crate) fn section(ui: &mut egui::Ui, name: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), SECTION), Sense::hover());
    ui.painter().text(Pos2::new(r.left() + 4.0, r.center().y + 2.0), Align2::LEFT_CENTER, name, ui_font(12.0), ui.visuals().weak_text_color());
}

fn note_row(ui: &mut egui::Ui, text: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::hover());
    let g = one_line(ui.painter(), text, ui_font(12.0), ui.visuals().weak_text_color(), r.width() - 40.0);
    ui.painter().galley(Pos2::new(r.left() + 36.0, r.center().y - g.size().y / 2.0), g, ui.visuals().weak_text_color());
}

fn bt_row(ui: &mut egui::Ui, d: &BtDevice, busy: &Option<String>, accent: Color32, m: &mut Model) {
    let v = ui.visuals().clone();
    let working = busy.as_deref() == Some(d.mac.as_str());
    let sub = match (working, d.connected, d.paired) {
        (true, true, _) => "Disconnecting…",
        (true, false, true) => "Connecting…",
        (true, false, false) => "Pairing…",
        (false, true, _) => "Connected",
        (false, false, true) => "Not connected",
        (false, false, false) => "Click to pair",
    };
    let (r, resp) = row(ui, d.connected, &d.name, sub, working.then_some(accent), 100.0);
    glyph::bluetooth(ui.painter(), Pos2::new(r.left() + 17.0, r.center().y), if d.connected { accent } else { v.text_color() }, d.connected);
    let mut button_clicked = false;
    if ui.rect_contains_pointer(r) && !working && d.paired {
        let (clicked, left) = text_button(ui, r, r.right() - 6.0, &format!("bt-a-{}", d.mac), if d.connected { "Disconnect" } else { "Forget" });
        if clicked {
            button_clicked = true;
            m.acts.push(Act::Radio(if d.connected { Job::BtDisconnect(d.mac.clone()) } else { Job::BtForget(d.mac.clone()) }));
        }
        if d.connected {
            let (clicked, _) = text_button(ui, r, left - 4.0, &format!("bt-f-{}", d.mac), "Forget");
            if clicked {
                button_clicked = true;
                m.acts.push(Act::Radio(Job::BtForget(d.mac.clone())));
            }
        }
    }
    if resp.clicked() && !button_clicked && !working && !d.connected {
        m.acts.push(Act::Radio(Job::BtConnect(d.mac.clone())));
    }
}

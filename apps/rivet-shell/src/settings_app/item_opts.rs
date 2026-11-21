//! The options behind an item's gear in Settings, Panels: one card per item, laid out the same
//! way: a header (the item's symbol and name), then sections of picture tiles (each choice drawn
//! as it shows in the panel), segmented switches and on/off rows.

use chrono::format::{Item, StrftimeItems};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_lg, r_md, r_sm, selected_tile};

use super::panel_editor::segmented;
use crate::config::{PowerStyle, Settings, Show, TrayColor, WsStyle};
use crate::glyph;
use crate::panels::label;

const WIDTH: f32 = 300.0;

fn ui_font(size: f32) -> egui::FontId {
    rivet_look::fonts::ui_font(size)
}

/// The items with options.
pub fn has_options(id: &str) -> bool {
    matches!(id, "launcher" | "clock" | "workspaces" | "ai" | "weather" | "media" | "volume" | "brightness" | "battery" | "tray" | "dock" | "power" | "quick-settings") || id.starts_with("plugin:")
}

/// The choice between icons in their own colours and in one of the panel's.
fn icon_tiles(ui: &mut egui::Ui, value: &mut TrayColor, labels: [&str; 3], accent: Color32) {
    let choices: Vec<(TrayColor, &str)> = TrayColor::ALL.into_iter().zip(labels).collect();
    tiles(ui, value, &choices, accent, move |p, r, c, col| {
        let own = [Color32::from_rgb(0x58, 0x65, 0xf2), Color32::from_rgb(0x2e, 0xa0, 0x43), Color32::from_rgb(0xe6, 0x61, 0x00)];
        for (i, o) in own.into_iter().enumerate() {
            let pc = Pos2::new(r.center().x + (i as f32 - 1.0) * 16.0, r.center().y);
            let fill = match c {
                TrayColor::Own => o,
                TrayColor::Text => col,
                TrayColor::Highlight => accent,
            };
            p.circle_filled(pc, 5.5, fill);
            p.circle_filled(pc, 2.0, mix(fill, Color32::BLACK, 0.45));
        }
    });
}

fn tip(id: &str) -> &'static str {
    match id {
        "launcher" => "The launcher's symbol",
        "clock" => "How the clock shows",
        "workspaces" => "How the workspaces show",
        "ai" => "What the AI item reads and shows",
        "weather" => "The weather's place and units",
        "tray" => "How apps' tray icons show",
        "dock" => "What the dock item shows",
        "media" => "What Now Playing shows and does",
        "volume" => "How the volume shows",
        "brightness" => "How the brightness shows",
        "battery" => "How the battery shows",
        "power" => "How the power menu shows",
        "quick-settings" => "The quick settings' symbol",
        _ => "Options",
    }
}

/// The gear on an item's row, centred on `c`, and its options card. `open` opens it (checks).
pub fn gear(ui: &mut egui::Ui, c: Pos2, salt: (usize, usize), id: &str, s: &mut Settings, accent: Color32, open: bool) {
    let v = ui.visuals().clone();
    let r = Rect::from_center_size(c, Vec2::splat(24.0));
    let resp = ui.interact(r, ui.scope_id().with(("item-gear", salt)), Sense::click());
    let popup = egui::Popup::from_toggle_button_response(&resp)
        .layout(egui::Layout::top_down(egui::Align::Min))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .gap(6.0)
        .width(WIDTH)
        .frame(egui::Frame::popup(ui.style()).corner_radius(r_lg()).inner_margin(egui::Margin::same(16)));
    let popup = if open {
        ui.scroll_to_rect(r.expand(120.0), Some(egui::Align::Center));
        popup.open_memory(Some(egui::SetOpenCommand::Bool(true)))
    } else {
        popup
    };
    let shown = popup.is_open();
    if shown || resp.hovered() {
        ui.painter().circle_filled(c, 11.0, if shown { mix(v.panel_fill, accent, 0.22) } else { mix(v.panel_fill, v.text_color(), 0.12) });
    }
    glyph::gear(ui.painter(), c, if shown { accent } else { v.weak_text_color() });
    popup.show(|ui| {
        ui.set_width(WIDTH);
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
        head(ui, id, accent, s.quick_icon);
        match id {
            "launcher" => launcher(ui, s, accent),
            "clock" => clock(ui, s),
            "workspaces" => workspaces(ui, s, accent),
            "ai" => ai(ui, s),
            "weather" => super::weather_opts::options(ui, s, WIDTH),
            plug if plug.starts_with("plugin:") => {
                let pid = &plug["plugin:".len()..];
                section(ui, "Icon");
                let mut color = s.plugin_icon(pid);
                icon_tiles(ui, &mut color, ["Colored", "Solid", "Highlight"], accent);
                if color != s.plugin_icon(pid) {
                    s.set_plugin(pid, crate::plugins::ICON_COLOR, if color == TrayColor::Own { "" } else { color.name() });
                }
                note(ui, "Solid draws the icon in the panel's text color. Highlight draws it in the accent color.");
            }
            "tray" => {
                section(ui, "Icons");
                icon_tiles(ui, &mut s.tray_color, TrayColor::ALL.map(TrayColor::label), accent);
                note(ui, "Solid draws icons in the panel's text color. Highlight draws them in the accent color.");
                section(ui, "Shows");
                toggle(ui, "Show passive icons", Some("Icons for apps with nothing to tell you right now"), &mut s.tray_passive, accent);
                note(ui, "Some older apps' icons only show if xembed-sni-proxy is installed.");
            }
            "dock" => {
                section(ui, "Shows");
                toggle(ui, "Open apps too", Some("After the pinned ones"), &mut s.dock_open_apps, accent);
                toggle(ui, "A dot under open apps", None, &mut s.dock_running, accent);
                note(ui, "Shows the dock's apps. Pinning or moving one here changes the dock too.");
            }
            "media" => media(ui, s, accent),
            "volume" => {
                section(ui, "Shows");
                show_tiles(ui, &mut s.vol_show, accent, |p, c, col| glyph::speaker(p, c, false, col), "64%");
                section(ui, "Behavior");
                toggle(ui, "Scroll to change the volume", Some("Over the item, in steps of 5%"), &mut s.vol_scroll, accent);
                note(ui, "Click to mute or unmute. The volume slider is in quick settings.");
            }
            "brightness" => {
                section(ui, "Shows");
                show_tiles(ui, &mut s.bright_show, accent, glyph::sun, "80%");
                section(ui, "Behavior");
                toggle(ui, "Scroll to change the brightness", Some("Over the item"), &mut s.bright_scroll, accent);
                note(ui, "Shows only on screens with adjustable brightness, such as a laptop's.");
            }
            "battery" => {
                section(ui, "Shows");
                show_tiles(ui, &mut s.batt_show, accent, move |p, c, col| glyph::battery(p, c, 72, false, col, accent), "72%");
                section(ui, "Behavior");
                toggle(ui, "Hide while charged", Some("Hidden when plugged in and full"), &mut s.batt_hide_full, accent);
                note(ui, "Turns red at 15% when not charging.");
            }
            "power" => power(ui, s, accent),
            "quick-settings" => {
                section(ui, "Symbol");
                let choices: Vec<(crate::config::QuickIcon, &str)> = crate::config::QuickIcon::ALL.iter().map(|c| (*c, c.label())).collect();
                tiles(ui, &mut s.quick_icon, &choices, accent, |p, r, c, col| glyph::quick(p, r.center(), col, c));
            }
            _ => {}
        }
    });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip(id));
}

/// The card's top: the item's symbol in a tinted disc, its name, and a rule under them.
fn head(ui: &mut egui::Ui, id: &str, accent: Color32, quick: crate::config::QuickIcon) {
    let v = ui.visuals().clone();
    let (r, _) = ui.allocate_exact_size(Vec2::new(WIDTH, 36.0), Sense::hover());
    let p = ui.painter();
    let c = Pos2::new(r.left() + 17.0, r.center().y);
    p.circle_filled(c, 17.0, mix(v.window_fill, accent, 0.16));
    glyph::item(p, id, c, accent, accent, quick);
    p.text(Pos2::new(r.left() + 44.0, r.top() + 10.0), Align2::LEFT_CENTER, label(id), ui_font(14.5), v.strong_text_color());
    p.text(Pos2::new(r.left() + 44.0, r.top() + 27.0), Align2::LEFT_CENTER, "Panel item options", ui_font(11.5), v.weak_text_color());
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(WIDTH, 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, mix(v.window_fill, v.text_color(), 0.1));
}

/// A section's heading: small, spaced capitals.
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    let col = ui.visuals().weak_text_color();
    ui.label(egui::RichText::new(title.to_uppercase()).font(ui_font(10.5)).extra_letter_spacing(0.8).color(col));
}

/// A line of small print.
fn note(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    let col = ui.visuals().weak_text_color();
    ui.add(egui::Label::new(egui::RichText::new(text).font(ui_font(11.5)).color(col)).wrap());
}

/// A row with a name (and a line under it) on the left and an on/off switch on the right; the
/// whole row toggles.
fn toggle(ui: &mut egui::Ui, title: &str, sub: Option<&str>, value: &mut bool, accent: Color32) {
    let v = ui.visuals().clone();
    let h = if sub.is_some() { 42.0 } else { 28.0 };
    let (r, resp) = ui.allocate_exact_size(Vec2::new(WIDTH, h), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r.expand2(Vec2::new(6.0, 0.0)), r_sm(), mix(v.window_fill, v.text_color(), 0.06));
    }
    match sub {
        Some(sub) => {
            p.text(Pos2::new(r.left(), r.top() + 13.0), Align2::LEFT_CENTER, title, ui_font(13.0), v.text_color());
            p.text(Pos2::new(r.left(), r.top() + 30.0), Align2::LEFT_CENTER, sub, ui_font(11.5), v.weak_text_color());
        }
        None => {
            p.text(Pos2::new(r.left(), r.center().y), Align2::LEFT_CENTER, title, ui_font(13.0), v.text_color());
        }
    }
    let sw = Rect::from_center_size(Pos2::new(r.right() - 18.0, r.center().y), Vec2::new(36.0, 20.0));
    let track = if *value { accent } else { mix(v.window_fill, v.text_color(), if resp.hovered() { 0.26 } else { 0.18 }) };
    p.rect_filled(sw, 10.0, track);
    let k = sw.height() - 4.0;
    let x = if *value { sw.right() - 2.0 - k / 2.0 } else { sw.left() + 2.0 + k / 2.0 };
    p.circle_filled(Pos2::new(x, sw.center().y), k / 2.0, if *value { crate::ui::on(accent) } else { v.window_fill });
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        *value = !*value;
    }
}

/// A row of picture tiles, one per choice: a little drawing of it over its name.
fn tiles<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, choices: &[(T, &str)], accent: Color32, draw: impl Fn(&egui::Painter, Rect, T, Color32)) {
    let v = ui.visuals().clone();
    let gap = 8.0;
    let n = choices.len() as f32;
    let w = (WIDTH - gap * (n - 1.0)) / n;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (val, name) in choices {
            let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 66.0), Sense::click());
            let on = *value == *val;
            let p = ui.painter();
            if on {
                let (fill, stroke) = selected_tile(&v, v.window_fill);
                p.rect_filled(r, r_md(), fill);
                p.rect_stroke(r, r_md(), stroke, egui::StrokeKind::Inside);
            } else {
                p.rect_filled(r, r_md(), mix(v.window_fill, v.text_color(), if resp.hovered() { 0.1 } else { 0.05 }));
            }
            // The drawing, on a strip like the panel's.
            let strip = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 22.0), Vec2::new(w - 14.0, 26.0));
            p.rect_filled(strip, r_sm(), mix(v.panel_fill, v.text_color(), 0.04));
            draw(p, strip, *val, v.text_color());
            p.text(Pos2::new(r.center().x, r.bottom() - 13.0), Align2::CENTER_CENTER, *name, ui_font(11.5), if on { v.strong_text_color() } else { v.weak_text_color() });
            if on {
                p.circle_filled(r.right_top() + Vec2::new(-9.0, 9.0), 3.0, accent);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                *value = *val;
            }
        }
    });
}

/// The icon / percent / both tiles of a level item.
fn show_tiles(ui: &mut egui::Ui, value: &mut Show, accent: Color32, icon: impl Fn(&egui::Painter, Pos2, Color32), text: &str) {
    let choices: Vec<(Show, &str)> = Show::ALL.iter().map(|s| (*s, s.label())).collect();
    tiles(ui, value, &choices, accent, |p, r, show, col| {
        let g = p.layout_no_wrap(text.to_string(), ui_font(12.0), col);
        let c = r.center();
        match show {
            Show::Icon => icon(p, c, col),
            Show::Text => {
                p.galley(c - g.size() / 2.0, g, col);
            }
            Show::Both => {
                let w = 24.0 + g.size().x;
                let x = c.x - w / 2.0;
                icon(p, Pos2::new(x + 9.0, c.y), col);
                p.galley(Pos2::new(x + 24.0, c.y - g.size().y / 2.0), g, col);
            }
        }
    });
}

fn valid(f: &str) -> bool {
    !f.trim().is_empty() && !StrftimeItems::new(f).any(|i| matches!(i, Item::Error))
}

/// The launcher's symbol: the grid, the system's logo, or an icon or picture of one's own.
fn launcher(ui: &mut egui::Ui, s: &mut Settings, accent: Color32) {
    use crate::config::LauncherIcon;
    use std::sync::{Arc, Mutex, mpsc};
    type Asked = Arc<Mutex<mpsc::Receiver<Result<Option<std::path::PathBuf>, String>>>>;
    section(ui, "Symbol");
    let choices: Vec<(LauncherIcon, &str)> = LauncherIcon::ALL.iter().map(|c| (*c, c.label())).collect();
    tiles(ui, &mut s.launcher_icon, &choices, accent, |p, r, c, col| {
        let at = r.center();
        match c {
            LauncherIcon::Grid => glyph::grid(p, at, col),
            // A logo of sorts: a ring with a dot.
            LauncherIcon::System => {
                p.circle_stroke(at, 7.5, egui::Stroke::new(1.6, col));
                p.circle_filled(at, 2.8, col);
            }
            // A picture: a frame, a sun and a hill.
            LauncherIcon::Custom => {
                let f = Rect::from_center_size(at, Vec2::new(19.0, 15.0));
                p.rect_stroke(f, 2.5, egui::Stroke::new(1.5, col), egui::StrokeKind::Middle);
                p.circle_filled(f.left_top() + Vec2::new(5.5, 5.0), 1.6, col);
                p.line(vec![f.left_bottom() + Vec2::new(3.0, -3.0), f.center() + Vec2::new(-1.0, 0.0), f.center() + Vec2::new(3.0, 3.5), f.right_bottom() + Vec2::new(-3.0, -6.0)], egui::Stroke::new(1.5, col));
            }
        }
    });
    match s.launcher_icon {
        LauncherIcon::Grid => {}
        LauncherIcon::System => note(ui, "Your system's logo, or the grid if the icon theme has none."),
        LauncherIcon::Custom => {
            // The file picker's answer, asked for on a thread (kept with the card).
            let key = egui::Id::unique("launcher-icon-pick");
            let asked: Option<Asked> = ui.data(|d| d.get_temp(key));
            if let Some(rx) = &asked {
                match rx.lock().ok().and_then(|r| r.try_recv().ok()) {
                    Some(answer) => {
                        if let Ok(Some(p)) = answer {
                            s.launcher_custom = p.to_string_lossy().into_owned();
                        }
                        ui.data_mut(|d| d.remove::<Asked>(key));
                    }
                    None => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if super::controls::button_if(ui, asked.is_none(), super::controls::Icon::Folder, "Choose…").clicked() {
                    let (tx, rx) = mpsc::channel();
                    let start = crate::wall::expand(s.launcher_custom.trim()).parent().map(std::path::Path::to_path_buf);
                    std::thread::spawn(move || {
                        let _ = tx.send(rivet_xdg::portal::choose(rivet_xdg::portal::Pick::Icon, "Launcher Icon", start.as_deref()));
                    });
                    ui.data_mut(|d| d.insert_temp::<Asked>(key, Arc::new(Mutex::new(rx))));
                }
                ui.add(egui::TextEdit::singleline(&mut s.launcher_custom).hint_text("Icon name or picture").desired_width(f32::INFINITY));
            });
            note(ui, "An icon name such as start-here, or a picture. It keeps its own colors.");
        }
    }
}

fn clock(ui: &mut egui::Ui, s: &mut Settings) {
    const PRESETS: [&str; 6] = ["%H:%M", "%a %b %-d  %H:%M", "%A %-d %B  %H:%M", "%-I:%M %p", "%a %b %-d  %-I:%M %p", "%Y-%m-%d  %H:%M"];
    let v = ui.visuals().clone();
    let accent = v.selection.bg_fill;
    let now = chrono::Local::now();
    section(ui, "Format");
    for f in PRESETS {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(WIDTH, 30.0), Sense::click());
        let on = s.clock_format == f;
        let p = ui.painter();
        if on {
            p.rect_filled(r.expand2(Vec2::new(6.0, 0.0)), r_sm(), mix(v.window_fill, accent, 0.14));
        } else if resp.hovered() {
            p.rect_filled(r.expand2(Vec2::new(6.0, 0.0)), r_sm(), mix(v.window_fill, v.text_color(), 0.06));
        }
        p.text(Pos2::new(r.left() + 4.0, r.center().y), Align2::LEFT_CENTER, now.format(f).to_string(), ui_font(13.0), if on { v.strong_text_color() } else { v.text_color() });
        if on {
            glyph::check(p, Pos2::new(r.right() - 10.0, r.center().y), accent);
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            s.clock_format = f.to_string();
        }
    }
    // Any strftime pattern; taken only while it's one chrono can show.
    section(ui, "Custom");
    let key = egui::Id::unique("shell-clock-custom");
    let mut text = ui.ctx().data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| s.clock_format.clone());
    let edit = egui::TextEdit::singleline(&mut text).desired_width(WIDTH).hint_text("%a %-d  %H:%M").font(egui::TextStyle::Monospace);
    let resp = ui.add(edit);
    if resp.changed() && valid(&text) {
        s.clock_format = text.clone();
    }
    if !resp.has_focus() && !resp.changed() && s.clock_format != text {
        // Picked from the list above: the field follows.
        text = s.clock_format.clone();
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, text.clone()));
    if valid(&text) {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("Shows as  {}", now.format(&text))).font(ui_font(12.5)));
        note(ui, "%H hour  ·  %-I:%M %p 12-hour  ·  %M minute  ·  %a day  ·  %b month  ·  %-d date  ·  %Y year");
    } else {
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Not a pattern the clock can show").font(ui_font(11.5)).color(rivet_look::theme::status(ui.ctx()).danger));
    }
}

fn media(ui: &mut egui::Ui, s: &mut Settings, accent: Color32) {
    use crate::config::MediaText;
    section(ui, "Shows");
    ui.add_space(4.0);
    egui::ComboBox::from_id_salt("media-text").width(WIDTH - 8.0).selected_text(s.media_text.label()).show_ui(ui, |ui| {
        for t in MediaText::ALL {
            ui.selectable_value(&mut s.media_text, t, t.label());
        }
    });
    if s.media_text != MediaText::None {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Widest").font(ui_font(13.0)));
            ui.add(egui::Slider::new(&mut s.media_width, 80..=480).step_by(10.0).suffix(" pt"));
        });
    }
    ui.add_space(4.0);
    toggle(ui, "Cover", Some("In place of the play symbol"), &mut s.media_art, accent);
    toggle(ui, "Previous and next buttons", None, &mut s.media_controls, accent);
    if crate::sys::has("pw-record") {
        toggle(ui, "Visualizer", Some("Bars that move with the sound"), &mut s.media_viz, accent);
    } else {
        note(ui, "Install pw-record (part of PipeWire) to show a visualizer.");
    }
    toggle(ui, "Hide while paused", None, &mut s.media_hide_paused, accent);
    section(ui, "A click");
    segmented(ui, &mut s.media_click_plays, &[(false, "Opens the Player"), (true, "Plays or Pauses")]);
    note(ui, "A middle click always plays or pauses.");
    section(ui, "Behavior");
    toggle(ui, "Scroll to change track", Some("Up for the previous one, down for the next"), &mut s.media_scroll, accent);
}

fn workspaces(ui: &mut egui::Ui, s: &mut Settings, accent: Color32) {
    section(ui, "Style");
    let choices: Vec<(WsStyle, &str)> = WsStyle::ALL.iter().map(|w| (*w, w.label())).collect();
    tiles(ui, &mut s.ws_style, &choices, accent, |p, r, style, col| ws_mini(p, r, style, col, accent));
    if s.ws_style != WsStyle::Dots {
        section(ui, "Each shows its");
        segmented(ui, &mut s.ws_names, &[(true, "Name"), (false, "Number")]);
        note(ui, "Workspaces without a name show their number.");
    }
    section(ui, "Behavior");
    toggle(ui, "Hide empty workspaces", Some("The one you're on always shows"), &mut s.ws_hide_empty, accent);
    toggle(ui, "Scroll to switch", Some("One workspace per step"), &mut s.ws_scroll, accent);
}

fn power(ui: &mut egui::Ui, s: &mut Settings, accent: Color32) {
    section(ui, "Menu style");
    let choices: Vec<(PowerStyle, &str)> = PowerStyle::ALL.iter().map(|w| (*w, w.label())).collect();
    tiles(ui, &mut s.power_style, &choices, accent, |p, r, style, col| {
        let c = r.center();
        let soft = col.gamma_multiply(0.3);
        match style {
            PowerStyle::Menu => {
                for dy in [-7.0, 0.0, 7.0] {
                    p.circle_filled(c + Vec2::new(-16.0, dy), 2.0, col);
                    p.rect_filled(Rect::from_min_size(c + Vec2::new(-10.0, dy - 1.5), Vec2::new(28.0, 3.0)), 1.5, soft);
                }
            }
            PowerStyle::Grid => {
                for (dx, dy) in [(-13.0, -6.0), (0.0, -6.0), (13.0, -6.0), (-6.5, 6.0), (6.5, 6.0)] {
                    p.rect_filled(Rect::from_center_size(c + Vec2::new(dx, dy), Vec2::new(11.0, 10.0)), 2.5, soft);
                    p.circle_filled(c + Vec2::new(dx, dy), 1.8, col);
                }
            }
            PowerStyle::Row => {
                for dx in [-21.0, -7.0, 7.0, 21.0] {
                    p.rect_filled(Rect::from_center_size(c + Vec2::new(dx, 0.0), Vec2::splat(11.0)), 2.5, soft);
                    p.circle_filled(c + Vec2::new(dx, 0.0), 1.8, col);
                }
            }
        }
    });
    note(ui, match s.power_style {
        PowerStyle::Menu => "A list under the item.",
        PowerStyle::Grid => "Large tiles in the middle of the screen, with your name and the computer's uptime.",
        PowerStyle::Row => "A compact row of symbols under the item.",
    });
    section(ui, "Behavior");
    toggle(ui, "Ask before leaving", Some("Log out, restart and shut down need a second click"), &mut s.power_confirm, accent);
    section(ui, "Shows");
    for (_, name, label, _) in crate::power_ui::ACTIONS {
        let mut on = !s.power_hide.iter().any(|h| h == name);
        let was = on;
        toggle(ui, label, None, &mut on, accent);
        // The last one showing stays.
        if on && !was {
            s.power_hide.retain(|h| h != name);
        } else if !on && was && s.power_hide.len() + 1 < crate::power_ui::ACTIONS.len() {
            s.power_hide.push(name.to_string());
        }
    }
}

/// Three workspaces (the second active) in a style, small.
fn ws_mini(p: &egui::Painter, r: Rect, style: WsStyle, col: Color32, accent: Color32) {
    let c = r.center();
    let weak = col.gamma_multiply(0.45);
    match style {
        WsStyle::Pills => {
            let ws = [(-20.0, 12.0, false), (0.0, 20.0, true), (20.0, 12.0, false)];
            for (dx, w, on) in ws {
                p.rect_filled(Rect::from_center_size(c + Vec2::new(dx, 0.0), Vec2::new(w, 12.0)), 6.0, if on { accent } else { col.gamma_multiply(0.28) });
            }
        }
        WsStyle::Squares => {
            for (k, dx) in [-16.0f32, 0.0, 16.0].into_iter().enumerate() {
                p.rect_filled(Rect::from_center_size(c + Vec2::new(dx, 0.0), Vec2::splat(12.0)), 3.5, if k == 1 { accent } else { col.gamma_multiply(0.28) });
            }
        }
        WsStyle::Dots => {
            p.circle_filled(c + Vec2::new(-16.0, 0.0), 3.0, col.gamma_multiply(0.75));
            p.rect_filled(Rect::from_center_size(c, Vec2::new(16.0, 6.0)), 3.0, accent);
            p.circle_stroke(c + Vec2::new(16.0, 0.0), 2.6, Stroke::new(1.2, weak));
        }
        WsStyle::Numbers => {
            for (k, dx) in [-16.0f32, 0.0, 16.0].into_iter().enumerate() {
                let on = k == 1;
                p.text(c + Vec2::new(dx, -1.0), Align2::CENTER_CENTER, (k + 1).to_string(), ui_font(if on { 13.0 } else { 12.0 }), if on { accent } else { weak });
                if on {
                    p.rect_filled(Rect::from_center_size(c + Vec2::new(dx, 8.0), Vec2::new(9.0, 2.0)), 1.0, accent);
                }
            }
        }
    }
}

fn ai(ui: &mut egui::Ui, s: &mut Settings) {
    let accent = ui.visuals().selection.bg_fill;
    section(ui, "Read the usage of");
    for (pid, pname) in crate::ai::PROVIDERS {
        let mut on = s.ai_providers.iter().any(|x| x == pid);
        let was = on;
        toggle(ui, pname, None, &mut on, accent);
        if on != was {
            if on {
                s.ai_providers.push(pid.to_string());
                // Kept in the providers' order.
                s.ai_providers.sort_by_key(|x| crate::ai::PROVIDERS.iter().position(|p| p.0 == x));
            } else {
                s.ai_providers.retain(|x| x != pid);
            }
        }
    }
    section(ui, "The panel shows");
    segmented(ui, &mut s.ai_left, &[(false, "Used"), (true, "Left")]);
    ui.add_space(4.0);
    let n = s.ai_pins.len();
    if n > 0 {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{n} pinned limit{} first", if n == 1 { "" } else { "s" })).font(ui_font(12.5)));
            if super::controls::button(ui, super::controls::Icon::Off, "Unpin All").clicked() {
                s.ai_pins.clear();
            }
        });
        note(ui, "With none pinned, show:");
    } else {
        note(ui, "Pin limits from the item's popup. With none pinned, show:");
    }
    egui::ComboBox::from_id_salt("ai-bar").width(WIDTH - 8.0).selected_text(if s.ai_bar == "auto" { "The Most Used Limit" } else { crate::ai::name(&s.ai_bar) }).show_ui(ui, |ui| {
        ui.selectable_value(&mut s.ai_bar, "auto".to_string(), "The Most Used Limit");
        for (pid, pname) in crate::ai::PROVIDERS {
            ui.selectable_value(&mut s.ai_bar, pid.to_string(), format!("{pname}'s Most Used Limit"));
        }
    });
    note(
        ui,
        &format!("Claude, Codex and Grok use the sign-in from their own apps. For OpenRouter or DeepSeek, add your key to {} as OPENROUTER_API_KEY or DEEPSEEK_API_KEY.", crate::ai::env_file().display()),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn clock_patterns() {
        assert!(super::valid("%H:%M"));
        assert!(!super::valid("%Q"));
        assert!(!super::valid("  "));
    }
}

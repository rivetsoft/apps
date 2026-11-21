//! Searching the settings from the sidebar: an index of every setting (its page, the card it's
//! in, other words people may type for it), matched word by word as the query is typed. Picking
//! a result opens its page, scrolls to its card and outlines it for a moment; a panel item's
//! options open its gear's card.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_md, r_sm, selected_tile};

use super::Page;

/// A setting that can be found.
pub struct Entry {
    pub page: Page,
    /// The card it's in (a `section` title on its page; empty: the page's top).
    pub section: &'static str,
    pub title: &'static str,
    /// Other words for it.
    pub words: &'static str,
    /// A panel item whose options hold it.
    pub item: Option<&'static str>,
}

const fn e(page: Page, section: &'static str, title: &'static str, words: &'static str) -> Entry {
    Entry { page, section, title, words, item: None }
}

const fn item(id: &'static str, title: &'static str, words: &'static str) -> Entry {
    Entry { page: Page::Panel, section: "", title, words, item: Some(id) }
}

pub const INDEX: &[Entry] = &[
    // Wallpaper
    e(Page::Wallpaper, "Wallpaper", "Wallpaper", "background picture image photo desktop"),
    e(Page::Wallpaper, "Wallpaper", "Wallpaper folder", "directory pictures path choose picker browse recent"),
    e(Page::Wallpaper, "Wallpaper", "Choose a picture", "file picker browse any image"),
    e(Page::Wallpaper, "Wallpaper", "No wallpaper", "plain background solid colour color none"),
    e(Page::Appearance, "Colors", "Colors from the wallpaper", "matugen colours dynamic material you scheme generate"),
    e(Page::Appearance, "Colors", "Color scheme: dark or light", "matugen colours mode theme"),
    e(Page::Appearance, "Colors", "Color scheme style", "matugen tonal spot vibrant expressive monochrome rainbow fidelity neutral content fruit salad"),
    e(Page::Wallpaper, "Wallpaper", "Wallpaper per screen", "monitor output display each different own picture multi"),
    e(Page::Wallpaper, "Transition", "Wallpaper transition", "animation fade wipe slide grow duration change effect"),
    e(Page::Appearance, "Colors", "Color contrast", "matugen colours stronger softer"),
    e(Page::Appearance, "Colors", "Screen the colors are made from", "matugen colours primary monitor output extract source"),
    e(Page::Appearance, "Colors", "Add Rivet's colors to matugen", "template config install"),
    // Appearance
    e(Page::Appearance, "Colors", "Where the colors come from", "wallpaper palette accent follow desktop colours matugen system source theme"),
    e(Page::Appearance, "Colors", "Light or dark style", "theme mode dark light system appearance"),
    e(Page::Appearance, "Colors", "Accent color", "colour highlight tint"),
    e(Page::Appearance, "Colors", "Color palette", "colours scheme theme preset built-in nord dracula gruvbox catppuccin solarized tokyo night one dark without matugen"),
    e(Page::Appearance, "Colors", "Surface color", "background colour surface black slate navy paper custom own tint base"),
    e(Page::Appearance, "Icons", "Icon theme", "icons apps tray papirus adwaita breeze system installed set"),
    e(Page::Appearance, "Color use", "Secondary and tertiary colors", "other colourful colorful palette how much accent only some a lot"),
    e(Page::Appearance, "Text", "Font", "typeface family text lettering"),
    e(Page::Appearance, "Text", "Text size", "font scale bigger smaller large small zoom"),
    e(Page::Appearance, "Panels and dock", "Panel and dock look", "corners radius rounded opacity transparency outline border"),
    e(Page::Appearance, "Panels and dock", "Panel popups float or attach", "popup widget float floating attached adhere joined drawer calendar quick settings menu"),
    e(Page::Appearance, "Panels and dock", "Panel and dock colors", "background tint primary secondary tertiary highlight colour"),
    // Panels
    e(Page::Panel, "", "Panels", "bar top bottom taskbar status"),
    e(Page::Displays, "", "Displays", "screens monitors outputs arrangement position"),
    e(Page::Displays, "", "Resolution and refresh rate", "display monitor mode hz hertz size"),
    e(Page::Displays, "", "Display scale", "monitor scaling hidpi fractional zoom percent"),
    e(Page::Displays, "", "Rotate a screen", "display monitor rotation portrait turn vrr variable refresh"),
    e(Page::Panel, "", "Panel position", "edge top bottom left right side vertical move"),
    e(Page::Panel, "Look", "Panel offset and room before windows", "margin distance from the screen edge reserved space gap exclusive zone"),
    e(Page::Panel, "", "Spacer", "panel item gap empty room space between items"),
    e(Page::Panel, "", "Panel length", "percent width height short narrow part of the screen placed start center end"),
    e(Page::Panel, "", "Hide the panel", "auto-hide smart hide slide away panel hiding"),
    e(Page::Panel, "", "Floating or edge to edge", "panel style rounded margin full width"),
    e(Page::Panel, "", "Panel screen", "monitor output display every screen"),
    e(Page::Panel, "", "Add or remove a panel", "new duplicate second panel"),
    e(Page::Panel, "", "Sub panels", "sub panel drawer shelf extra hidden panel roll out reveal stay open attach key binding"),
    e(Page::Panel, "", "Panel items", "zones left center centre right drag reorder widgets add launcher network bluetooth media notifications power"),
    item("launcher", "Launcher icon", "apps menu start button symbol logo distro distributor grid custom picture image"),
    item("clock", "Clock format", "time date 24 hour 12 hour am pm strftime custom seconds"),
    item("workspaces", "Workspaces style", "pills dots numbers names hide empty scroll switch"),
    item("ai", "AI usage", "claude codex grok openrouter deepseek limits quota usage pin keys"),
    item("weather", "Weather in the panel", "forecast temperature place city town location celsius fahrenheit units rain"),
    item("volume", "Volume in the panel", "sound audio speaker icon percent scroll mute"),
    item("brightness", "Brightness in the panel", "screen backlight icon percent scroll"),
    item("battery", "Battery in the panel", "power charge icon percent hide charged full"),
    item("power", "Power menu style", "shut down shutdown restart reboot log out lock suspend menu grid row confirm ask"),
    item("dock", "Dock in the panel", "pinned apps taskbar launcher favorites favourites open running panel item"),
    item("tray", "System tray", "status icons notifier appindicator tray recolor recolour monochrome passive idle"),
    // Dock
    e(Page::Dock, "Dock", "Show the dock", "dock enable disable taskbar"),
    e(Page::Dock, "Dock", "Dock icon size", "bigger smaller scale"),
    e(Page::Dock, "Dock", "Hide the dock", "auto-hide autohide smart hide"),
    e(Page::Dock, "Dock", "Keep windows clear of the dock", "reserve space exclusive zone overlap"),
    e(Page::Dock, "Dock", "Dots under running apps", "indicator open windows"),
    e(Page::Dock, "Dock", "Show open apps in the dock", "running unpinned windows"),
    e(Page::Dock, "Look", "Dock look", "corners radius opacity background tint highlight outline colour primary secondary tertiary"),
    e(Page::Dock, "Pinned apps", "Pinned apps", "favourites favorites add remove reorder pin unpin"),
    // Widgets
    e(Page::Launcher, "Layout", "Launcher as a grid or a list", "apps menu view rows icons tiles"),
    e(Page::Launcher, "Layout", "Launcher in the middle or from the panel", "center centre attach affix panel position place where opens"),
    item("quick-settings", "Quick settings symbol", "icon sliders switches toggles tiles gear control center"),
    e(Page::Launcher, "Text", "Launcher font", "typeface family text lettering own"),
    e(Page::Launcher, "Layout", "Launcher size", "small medium large width height bigger"),
    e(Page::Launcher, "Layout", "Launcher icon size", "apps icons bigger smaller"),
    e(Page::Launcher, "As you type", "Answer sums in the launcher", "calculator calc math"),
    e(Page::Launcher, "Behind the launcher", "Dim the screen behind the launcher", "darken overlay background"),
    e(Page::Launcher, "Keys", "Open the launcher with a key", "bind shortcut hotkey msg"),
    e(Page::Plugins, "Installed", "Plugins", "extensions addons add-ons third-party official panel item install enable"),
    e(Page::Plugins, "Hooks", "Hooks", "run a command script event on lock unlock idle wallpaper notification workspace media"),
    e(Page::Widgets, "On the desktop", "Desktop clock", "widget date large"),
    e(Page::Idle, "Passwords", "Ask for the password for administrator rights", "polkit authentication agent prompt pkexec sudo root admin authorize"),
    e(Page::Widgets, "On the desktop", "System widget", "processor cpu memory ram battery"),
    e(Page::Widgets, "On the desktop", "Widgets side", "left right"),
    e(Page::Widgets, "On the desktop", "Weather widget", "forecast temperature rain sky"),
    e(Page::Widgets, "Weather", "Weather place and units", "city town location celsius fahrenheit forecast"),
    // Notifications
    e(Page::Notifications, "Notifications", "Notifications", "popups toasts cards history mako dunst corner position place width timeout duration seconds how many"),
    e(Page::Notifications, "Notification look", "Notification look", "cards corners radius opacity color background outline border style"),
    e(Page::Notifications, "Volume and brightness pop-up", "Volume and brightness pop-up", "osd on-screen display indicator popup position place top middle bottom size color primary secondary tertiary duration caps num lock"),
    e(Page::Notifications, "Night Light", "Night light", "warmth kelvin temperature blue light gammastep hyprsunset wlsunset"),
    e(Page::Notifications, "Keys", "Keyboard shortcuts", "keys bind hotkeys commands msg launcher"),
    // About
    e(Page::About, "", "About Rivet Shell", "version license"),
];

/// Where an entry is, for people: "Dock › Pinned apps".
pub fn place(en: &Entry) -> String {
    match (en.item, en.section) {
        (Some(id), _) => format!("{} › {}", Page::Panel.label(), crate::panels::label(id)),
        (None, "") => en.page.label().to_string(),
        (None, s) if s == en.title || s == en.page.label() => en.page.label().to_string(),
        (None, s) => format!("{} › {s}", en.page.label()),
    }
}

/// The entries matching `query`, best first: every word of it is found in the entry, and
/// matches in the title rank above matches in its other words.
pub fn find(query: &str) -> Vec<&'static Entry> {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(i32, usize, &Entry)> = INDEX
        .iter()
        .enumerate()
        .filter_map(|(i, en)| {
            let title = en.title.to_lowercase();
            let rest = format!("{} {} {} {}", en.page.label(), en.section, en.words, en.item.map(crate::panels::label).unwrap_or_default()).to_lowercase();
            let mut score = 0;
            for w in &words {
                if title.split(|c: char| !c.is_alphanumeric()).any(|t| t.starts_with(w.as_str())) {
                    score += 4;
                } else if title.contains(w.as_str()) {
                    score += 3;
                } else if rest.split(|c: char| !c.is_alphanumeric()).any(|t| t.starts_with(w.as_str())) {
                    score += 2;
                } else if rest.contains(w.as_str()) {
                    score += 1;
                } else {
                    return None;
                }
            }
            if title.starts_with(&words[0]) {
                score += 2;
            }
            Some((score, i, en))
        })
        .collect();
    hits.sort_by_key(|h| (-h.0, h.1));
    hits.into_iter().map(|h| h.2).collect()
}

/// The card being shown after a search: its title and when it was picked.
#[derive(Clone)]
struct Reveal {
    section: String,
    at: f64,
    scrolled: bool,
}

fn reveal_id() -> egui::Id {
    egui::Id::unique("shell-settings-reveal")
}

/// Show the card titled `section` on the next page drawn.
pub fn reveal(ctx: &egui::Context, section: &str) {
    let at = ctx.input(|i| i.time);
    ctx.data_mut(|d| d.insert_temp(reveal_id(), Reveal { section: section.to_string(), at, scrolled: false }));
}

const GLOW: f64 = 1.8;

/// After a card titled `title` is drawn at `r`: scroll to it and outline it when it's the one
/// a search picked.
pub fn mark(ui: &mut egui::Ui, title: &str, r: Rect) {
    let ctx = ui.ctx().clone();
    let Some(mut rv) = ctx.data(|d| d.get_temp::<Reveal>(reveal_id())) else { return };
    if rv.section != title {
        return;
    }
    if !rv.scrolled {
        ui.scroll_to_rect(r.expand(30.0), Some(egui::Align::TOP));
        rv.scrolled = true;
        ctx.data_mut(|d| d.insert_temp(reveal_id(), rv.clone()));
    }
    let t = ctx.input(|i| i.time) - rv.at;
    if t > GLOW {
        ctx.data_mut(|d| d.remove::<Reveal>(reveal_id()));
        return;
    }
    // Holds, then fades.
    let a = if t < GLOW * 0.5 { 1.0 } else { (1.0 - (t - GLOW * 0.5) / (GLOW * 0.5)) as f32 };
    let accent = ui.visuals().selection.bg_fill;
    ui.painter().rect_stroke(r.expand(3.0), r_md(), Stroke::new(2.0, accent.gamma_multiply(a)), egui::StrokeKind::Outside);
    ctx.request_repaint();
}

/// The sidebar's search field. Ctrl+F focuses it; Escape clears it.
pub fn field(ui: &mut egui::Ui, query: &mut String) -> egui::Response {
    let v = ui.visuals().clone();
    let id = egui::Id::unique("shell-settings-search");
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
        ui.memory_mut(|m| m.request_focus(id));
    }
    let focused = ui.memory(|m| m.has_focus(id));
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::hover());
    let fill = if focused { v.extreme_bg_color } else { mix(v.extreme_bg_color, v.text_color(), 0.04) };
    ui.painter().rect_filled(r, r_sm(), fill);
    let stroke = if focused { Stroke::new(1.5, v.selection.bg_fill) } else { Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.14)) };
    ui.painter().rect_stroke(r, r_sm(), stroke, egui::StrokeKind::Inside);
    magnifier(ui.painter(), Pos2::new(r.left() + 15.0, r.center().y), v.weak_text_color());
    let clear = !query.is_empty();
    let text_r = Rect::from_min_max(Pos2::new(r.left() + 28.0, r.top()), Pos2::new(r.right() - if clear { 26.0 } else { 8.0 }, r.bottom()));
    let edit = egui::TextEdit::singleline(query).id(id).frame(egui::Frame::NONE).hint_text("Search settings").font(egui::FontId::new(13.0, egui::FontFamily::Proportional)).align(egui::Align2::LEFT_CENTER).desired_width(text_r.width());
    let resp = ui.put(text_r, edit);
    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        query.clear();
    }
    if clear {
        let xr = Rect::from_center_size(Pos2::new(r.right() - 14.0, r.center().y), Vec2::splat(20.0));
        let x_resp = ui.interact(xr, id.with("clear"), Sense::click());
        if x_resp.hovered() {
            ui.painter().circle_filled(xr.center(), 9.0, mix(fill, v.text_color(), 0.12));
        }
        crate::glyph::x(ui.painter(), xr.center(), 3.5, v.weak_text_color());
        if x_resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Clear").clicked() {
            query.clear();
            ui.memory_mut(|m| m.request_focus(id));
        }
    }
    resp
}

fn magnifier(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.circle_stroke(c + Vec2::new(-1.5, -1.5), 4.5, s);
    p.line_segment([c + Vec2::new(1.8, 1.8), c + Vec2::new(5.0, 5.0)], s);
}

/// The results under the field; returns the one picked (clicked, or Enter for the one marked).
/// `sel` is the marked one, moved with the arrow keys while the field has focus.
pub fn results(ui: &mut egui::Ui, hits: &[&'static Entry], sel: &mut usize, keys: bool, chosen: Option<&'static str>) -> Option<&'static Entry> {
    let v = ui.visuals().clone();
    let side = ui.visuals().panel_fill;
    let mut picked = None;
    if hits.is_empty() {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("No settings match").color(v.weak_text_color()).size(12.5));
        return None;
    }
    *sel = (*sel).min(hits.len() - 1);
    if keys {
        ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowDown) {
                *sel = (*sel + 1).min(hits.len() - 1);
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                *sel = sel.saturating_sub(1);
            }
            if i.key_pressed(egui::Key::Enter) {
                picked = Some(hits[*sel]);
            }
        });
    }
    let n = hits.len();
    ui.label(egui::RichText::new(if n == 1 { "1 result".to_string() } else { format!("{n} results") }).color(v.weak_text_color()).size(11.5));
    for (k, en) in hits.iter().enumerate() {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::click());
        let on = chosen == Some(en.title);
        if on {
            let (fill, stroke) = selected_tile(&v, side);
            ui.painter().rect_filled(r, r_sm(), fill);
            ui.painter().rect_stroke(r, r_sm(), stroke, egui::StrokeKind::Inside);
        } else if resp.hovered() || (keys && k == *sel) {
            ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
        }
        let p = ui.painter();
        let g = crate::ui::one_line(p, en.title, rivet_look::fonts::ui_font(13.0), v.text_color(), r.width() - 20.0);
        p.galley(Pos2::new(r.left() + 10.0, r.top() + 6.0), g, if on { v.strong_text_color() } else { v.text_color() });
        let g = crate::ui::one_line(p, &place(en), rivet_look::fonts::ui_font(11.5), v.weak_text_color(), r.width() - 20.0);
        p.galley(Pos2::new(r.left() + 10.0, r.top() + 24.0), g, v.weak_text_color());
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            *sel = k;
            picked = Some(*en);
        }
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds() {
        let first = |q: &str| find(q).first().map(|e| e.title);
        assert_eq!(first("accent"), Some("Accent color"));
        assert_eq!(first("dock size"), Some("Dock icon size"));
        assert_eq!(first("24 hour"), Some("Clock format"));
        assert_eq!(first("autohide"), Some("Hide the dock"));
        assert!(find("battery").iter().any(|e| e.item == Some("battery")));
        assert!(find("zzz").is_empty());
        assert!(find("  ").is_empty());
    }

    #[test]
    fn sections_exist() {
        // Every card an entry points to is one a page draws (checked against the source).
        let src = [include_str!("../settings_app.rs"), include_str!("wallpaper.rs"), include_str!("look_opts.rs"), include_str!("launcher.rs"), include_str!("plugins.rs"), include_str!("colors.rs"), include_str!("icon_pick.rs"), include_str!("idle_lock.rs")].concat();
        for en in INDEX.iter().filter(|e| !e.section.is_empty()) {
            assert!(src.contains(&format!("section(ui, \"{}\"", en.section)), "no card titled {}", en.section);
        }
    }
}

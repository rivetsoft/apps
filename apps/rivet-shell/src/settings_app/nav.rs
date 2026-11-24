//! The Settings window's sidebar: the pages in groups, each with a symbol drawn in the accent
//! colour (in the text's colour on the chosen page), and the line under a page's title.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::{fonts, palette, theme::r_md};

use super::search;

use super::Page;
use crate::glyph;

/// The pages under a small heading each; About sits apart at the end.
pub(super) const GROUPS: &[(&str, &[Page])] = &[("Connections", &[Page::Network, Page::Bluetooth, Page::Vpn]), ("Look", &[Page::Wallpaper, Page::Appearance, Page::Applications]), ("Desktop", &[Page::Panel, Page::Dock, Page::Widgets, Page::Launcher]), ("System", &[Page::Displays, Page::Sound, Page::Notifications, Page::Idle, Page::Plugins, Page::Softcore]), ("", &[Page::About])];

/// What a page is for, under its title.
pub(super) fn about(page: Page) -> &'static str {
    match page {
        Page::Wallpaper => "The desktop picture and how a new one appears",
        Page::Appearance => "Colors, text, and the look of panels and windows",
        Page::Applications => "Other apps that take the theme's colors",
        Page::Network => "Wired connections, and Wi-Fi networks to join or forget",
        Page::Bluetooth => "Devices to pair, connect and forget",
        Page::Vpn => "VPN connections, Tailscale and ZeroTier",
        Page::Sound => "Where sound plays and records, and how loud each app is",
        Page::Panel => "The bars along the screen's edges and the items on each",
        Page::Dock => "Pinned and open apps along the bottom",
        Page::Widgets => "A clock, system readings and the weather on the desktop",
        Page::Launcher => "How the launcher looks and what it answers as you type",
        Page::Notifications => "Notifications, on-screen volume and brightness, and Night Light",
        Page::Displays => "Resolution, scale, rotation and arrangement of each screen",
        Page::Idle => "What happens when the computer is idle, and the lock screen",
        Page::Plugins => "Plugins that add to the shell, and commands that run on events",
        Page::Softcore => "The compositor's windows, columns, keyboard, pointer and key bindings",
        Page::About => "",
    }
}

/// A page's symbol, 16 points across, around `c`.
pub(super) fn icon(p: &egui::Painter, page: Page, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    let screen = Rect::from_center_size(c, Vec2::new(15.0, 12.0));
    match page {
        Page::Wallpaper => {
            // A picture: a sun over hills.
            p.rect_stroke(screen, 2.5, s, egui::StrokeKind::Inside);
            p.circle_filled(c + Vec2::new(-3.0, -2.0), 1.6, col);
            p.add(egui::Shape::line(vec![c + Vec2::new(-5.5, 4.0), c + Vec2::new(-1.0, 0.5), c + Vec2::new(1.5, 2.5), c + Vec2::new(3.5, 0.5), c + Vec2::new(6.0, 3.5)], s));
        }
        Page::Appearance => glyph::contrast(p, c, col),
        Page::Applications => {
            // A window with a swatch of colour in it.
            p.rect_stroke(screen, 2.5, s, egui::StrokeKind::Inside);
            p.line_segment([Pos2::new(screen.left() + 1.0, screen.top() + 3.5), Pos2::new(screen.right() - 1.0, screen.top() + 3.5)], s);
            p.circle_filled(c + Vec2::new(0.0, 2.0), 2.4, col);
        }
        Page::Network => glyph::wifi(p, c - Vec2::new(0.0, 1.0), col),
        Page::Bluetooth => glyph::bluetooth(p, c, col, true),
        Page::Sound => glyph::speaker(p, c, false, col),
        Page::Vpn => glyph::vpn(p, c, col, false),
        Page::Panel => {
            // A screen with a bar along its top.
            p.rect_stroke(screen, 2.5, s, egui::StrokeKind::Inside);
            p.rect_filled(Rect::from_min_size(screen.min + Vec2::new(2.5, 2.5), Vec2::new(10.0, 2.5)), 1.0, col);
        }
        Page::Dock => {
            // A screen with a dock at the bottom.
            p.rect_stroke(screen, 2.5, s, egui::StrokeKind::Inside);
            p.rect_filled(Rect::from_center_size(Pos2::new(c.x, screen.bottom() - 3.75), Vec2::new(7.0, 2.5)), 1.25, col);
        }
        Page::Widgets => {
            // Tiles: a tall one and two small.
            p.rect_stroke(Rect::from_min_max(c + Vec2::new(-7.0, -6.5), c + Vec2::new(-0.5, 6.5)), 2.0, s, egui::StrokeKind::Inside);
            p.rect_stroke(Rect::from_min_max(c + Vec2::new(1.5, -6.5), c + Vec2::new(7.0, -0.5)), 2.0, s, egui::StrokeKind::Inside);
            p.rect_filled(Rect::from_min_max(c + Vec2::new(1.5, 1.5), c + Vec2::new(7.0, 6.5)), 2.0, col);
        }
        Page::Launcher => glyph::grid(p, c, col),
        Page::Notifications => glyph::bell(p, c, col, false),
        Page::Displays => {
            // A screen on its stand.
            p.rect_stroke(Rect::from_center_size(c - Vec2::new(0.0, 1.5), Vec2::new(15.0, 10.5)), 2.5, s, egui::StrokeKind::Inside);
            p.line_segment([c + Vec2::new(-3.5, 6.5), c + Vec2::new(3.5, 6.5)], s);
        }
        Page::Idle => {
            // A lock: its shackle behind its body.
            p.rect_stroke(Rect::from_center_size(c + Vec2::new(0.0, -2.5), Vec2::new(8.0, 9.0)), 4.0, s, egui::StrokeKind::Inside);
            p.rect_filled(Rect::from_center_size(c + Vec2::new(0.0, 2.5), Vec2::new(12.0, 8.0)), 2.0, col);
        }
        Page::Plugins => {
            // A puzzle piece, near enough: a block with a knob on top and one at its side.
            p.rect_filled(Rect::from_min_max(c + Vec2::new(-6.5, -2.5), c + Vec2::new(4.0, 7.0)), 2.0, col);
            p.circle_filled(c + Vec2::new(-1.25, -3.5), 2.75, col);
            p.circle_filled(c + Vec2::new(5.0, 2.25), 2.75, col);
        }
        Page::Softcore => {
            // Columns side by side, the one in the middle focused.
            p.rect_stroke(Rect::from_min_max(c + Vec2::new(-7.5, -5.5), c + Vec2::new(-4.0, 5.5)), 1.5, s, egui::StrokeKind::Inside);
            p.rect_filled(Rect::from_min_max(c + Vec2::new(-2.5, -6.5), c + Vec2::new(2.5, 6.5)), 1.5, col);
            p.rect_stroke(Rect::from_min_max(c + Vec2::new(4.0, -5.5), c + Vec2::new(7.5, 5.5)), 1.5, s, egui::StrokeKind::Inside);
        }
        Page::About => {
            p.circle_stroke(c, 6.75, s);
            p.circle_filled(c + Vec2::new(0.0, -3.0), 1.0, col);
            p.line_segment([c + Vec2::new(0.0, -0.5), c + Vec2::new(0.0, 3.5)], s);
        }
    }
}

/// A page's symbol drawn `scale` times its size, around `c`.
fn big_icon(p: &egui::Painter, page: Page, c: Pos2, col: Color32, scale: f32) {
    let grow = egui::emath::TSTransform::new(c.to_vec2() * (1.0 - scale), scale);
    // The shapes' clip is scaled along with them, so it starts out shrunk by as much.
    let p = p.with_clip_rect(grow.inverse() * p.clip_rect());
    let first = p.add(egui::Shape::Noop);
    icon(&p, page, c, col);
    let last = p.add(egui::Shape::Noop);
    p.ctx().graphics_mut(|g| g.entry(p.layer_id()).transform_range(first, last, grow));
}

/// The overview: a search field over every page as a symbol and its name, group by group.
/// Gives the page pressed.
pub(super) fn overview(ui: &mut egui::Ui, query: &mut String, accent: Color32) -> Option<Page> {
    const TILE: Vec2 = Vec2::new(112.0, 98.0);
    let v = ui.visuals().clone();
    let mut picked = None;
    egui::ScrollArea::vertical().id_salt("shell-overview").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        // As many tiles across as fit, up to six, in a column down the middle of the window.
        let across = (((ui.available_width() - 48.0) / TILE.x).floor() as usize).clamp(2, 6);
        let wide = across as f32 * TILE.x;
        let left = ui.max_rect().left() + ((ui.available_width() - wide) / 2.0).max(0.0);
        ui.add_space(28.0);
        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
        let field = Rect::from_center_size(bar.center(), Vec2::new(wide.min(340.0), 32.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(field), |ui| search::field(ui, query));
        ui.add_space(14.0);
        // About has no group of its own in the sidebar; here it ends the last one.
        let mut groups: Vec<(&str, Vec<Page>)> = Vec::new();
        for (name, pages) in GROUPS {
            let shown = pages.iter().copied().filter(|p| p.shown());
            match groups.last_mut() {
                Some(last) if name.is_empty() => last.1.extend(shown),
                _ => groups.push((name, shown.collect())),
            }
        }
        for (name, pages) in groups {
            let (head, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
            ui.painter().text(Pos2::new(left + 14.0, head.bottom() - 10.0), egui::Align2::LEFT_CENTER, name.to_uppercase(), fonts::ui_font(10.5), v.weak_text_color().gamma_multiply(0.8));
            for row in pages.chunks(across) {
                let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), TILE.y), Sense::hover());
                for (i, page) in row.iter().enumerate() {
                    let r = Rect::from_min_size(Pos2::new(left + i as f32 * TILE.x, line.top()), TILE).shrink(3.0);
                    let resp = ui.interact(r, ui.scope_id().with(("overview", *page as u8)), Sense::click());
                    if resp.hovered() || resp.has_focus() {
                        ui.painter().rect_filled(r, r_md(), palette::mix(v.panel_fill, v.text_color(), 0.07));
                    }
                    let c = Pos2::new(r.center().x, r.top() + 36.0);
                    // A tile of the accent, its symbol in the colour that reads on it.
                    ui.painter().rect_filled(Rect::from_center_size(c, Vec2::splat(46.0)), 12.0, accent);
                    big_icon(ui.painter(), *page, c, v.selection.stroke.color, 1.6);
                    ui.painter().text(Pos2::new(r.center().x, r.bottom() - 16.0), egui::Align2::CENTER_CENTER, page.label(), fonts::ui_font(12.5), v.text_color());
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        picked = Some(*page);
                    }
                }
            }
        }
        ui.add_space(24.0);
    });
    picked
}

/// The sidebar's way back to the overview. True when pressed.
pub(super) fn all_settings(ui: &mut egui::Ui, side: Color32) -> bool {
    let v = ui.visuals().clone();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, r_md(), palette::mix(side, v.text_color(), 0.07));
    }
    let c = Pos2::new(r.left() + 21.0, r.center().y);
    let s = Stroke::new(1.5, v.weak_text_color());
    ui.painter().add(egui::Shape::line(vec![c + Vec2::new(2.0, -4.5), c + Vec2::new(-2.5, 0.0), c + Vec2::new(2.0, 4.5)], s));
    ui.painter().text(Pos2::new(r.left() + 42.0, r.center().y), egui::Align2::LEFT_CENTER, "All Settings", fonts::ui_font(13.5), v.text_color());
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

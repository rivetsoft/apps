//! The Settings window's sidebar: the pages in groups, each with a symbol drawn in the accent
//! colour (in the text's colour on the chosen page), and the line under a page's title.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

use super::Page;
use crate::glyph;

/// The pages under a small heading each; About sits apart at the end.
pub(super) const GROUPS: &[(&str, &[Page])] = &[("Look", &[Page::Wallpaper, Page::Appearance]), ("Desktop", &[Page::Panel, Page::Dock, Page::Widgets, Page::Launcher]), ("System", &[Page::Displays, Page::Notifications, Page::Idle, Page::Plugins]), ("", &[Page::About])];

/// What a page is for, under its title.
pub(super) fn about(page: Page) -> &'static str {
    match page {
        Page::Wallpaper => "The desktop picture and how a new one appears",
        Page::Appearance => "Colors, text, and the look of panels and windows",
        Page::Panel => "The bars along the screen's edges and the items on each",
        Page::Dock => "Pinned and open apps along the bottom",
        Page::Widgets => "A clock, system readings and the weather on the desktop",
        Page::Launcher => "How the launcher looks and what it answers as you type",
        Page::Notifications => "Notifications, on-screen volume and brightness, and Night Light",
        Page::Displays => "Resolution, scale, rotation and arrangement of each screen",
        Page::Idle => "What happens when the computer is idle, and the lock screen",
        Page::Plugins => "Plugins that add to the shell, and commands that run on events",
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
        Page::About => {
            p.circle_stroke(c, 6.75, s);
            p.circle_filled(c + Vec2::new(0.0, -3.0), 1.0, col);
            p.line_segment([c + Vec2::new(0.0, -0.5), c + Vec2::new(0.0, 3.5)], s);
        }
    }
}

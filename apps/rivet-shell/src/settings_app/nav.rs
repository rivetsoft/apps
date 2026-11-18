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
        Page::Wallpaper => "The wallpaper, and the colors made from it",
        Page::Appearance => "Light or dark, the accent, text size, and how panels and windows look",
        Page::Panel => "The bars along the screen's edges, and what's on each",
        Page::Dock => "Pinned and open apps along the bottom",
        Page::Widgets => "The clock and the system's readings on the desktop",
        Page::Launcher => "The app launcher: a grid or a list, its size, and what it answers as you type",
        Page::Notifications => "Notification cards, the volume and brightness display, night light",
        Page::Displays => "Each screen's resolution, scale and rotation, and where they sit",
        Page::Idle => "What happens as the computer sits untouched, and the lock screen",
        Page::Plugins => "Programs that add to the shell, and your own commands for what happens",
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

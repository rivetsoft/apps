//! The look every Rivet app shares: the theme built from the light/dark choice, the accent
//! and the desktop's palette; one scale of corner radii (square in terminal mode); and the
//! info / danger / warning / success colours.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::palette::{self, Palette, mix};

/// The accent colours every Rivet app offers, with their names.
pub const ACCENTS: [([u8; 3], &str); 9] = [
    ([0x35, 0x84, 0xe4], "Blue"),
    ([0x5e, 0x9c, 0x3c], "Green"),
    ([0x1f, 0x9e, 0x9a], "Teal"),
    ([0x91, 0x41, 0xac], "Purple"),
    ([0xd5, 0x3f, 0x8c], "Pink"),
    ([0xe5, 0xa5, 0x0a], "Yellow"),
    ([0xe6, 0x61, 0x00], "Orange"),
    ([0xc0, 0x1c, 0x28], "Red"),
    ([0x77, 0x76, 0x7b], "Grey"),
];

/// Light, dark, or whatever the desktop prefers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    System,
    Light,
    Dark,
}

/// What an app chooses; `apply` turns it into egui styles for both light and dark.
#[derive(Clone, Copy, Debug)]
pub struct Look<'a> {
    pub mode: Mode,
    pub accent: Color32,
    /// The desktop's generated colours, when the app follows them.
    pub palette: Option<&'a Palette>,
    /// Terminal mode: flat, boxed, square.
    pub terminal: bool,
}

/// One radius scale for the whole app. Nested shapes use the next step down by the
/// padding between them, so corners stay concentric (e.g. a MD bar with 4 px
/// padding holds SM pills).
/// Corner radii. Terminal mode squares everything off (see `set_square`).
static SQUARE: AtomicBool = AtomicBool::new(false);
pub fn set_square(on: bool) {
    SQUARE.store(on, Ordering::Relaxed);
}
pub fn square() -> bool {
    SQUARE.load(Ordering::Relaxed)
}
fn radius(r: u8) -> u8 {
    if square() { 0 } else { r }
}
pub fn r_xs() -> u8 { radius(4) } // tiny things: swatches, selection band
pub fn r_sm() -> u8 { radius(6) } // controls: buttons, pills, tabs, items, list rows
pub fn r_md() -> u8 { radius(10) } // bars and cards: location bar, sidebar rows, menus, settings cards
pub fn r_lg() -> u8 { radius(14) } // big surfaces: the file area, in-app windows


/// Info / danger / warning / success: a fixed base hue (70%) tinted by the theme accent (30%),
/// so action colors belong to the current palette.
#[derive(Clone, Copy, Debug)]
pub struct StatusColors {
    pub info: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub success: Color32,
}

impl StatusColors {
    pub fn from_theme(theme: Color32) -> Self {
        let m = |b: Color32| mix(b, theme, 0.3);
        StatusColors {
            info: m(Color32::from_rgb(0x35, 0x84, 0xe4)),
            danger: m(Color32::from_rgb(0xe0, 0x1b, 0x24)),
            warning: m(Color32::from_rgb(0xe5, 0xa5, 0x0a)),
            success: m(Color32::from_rgb(0x26, 0xa2, 0x69)),
        }
    }

    /// Readable text on a filled button of this color.
    pub fn on(c: Color32) -> Color32 {
        if palette::luminance(c) > 0.62 { Color32::from_gray(20) } else { Color32::WHITE }
    }
}

/// The current status colors (set whenever the visuals change).
pub fn status(ctx: &egui::Context) -> StatusColors {
    ctx.data(|d| d.get_temp::<StatusColors>(egui::Id::unique("rivet-status-colors")))
        .unwrap_or_else(|| StatusColors::from_theme(Color32::from_rgb(0x35, 0x84, 0xe4)))
}

/// Set egui's light and dark styles from a look. A generated palette is made for one mode,
/// so when there is one, the app follows that mode.
pub fn apply(ctx: &egui::Context, look: &Look) {
    ctx.set_theme(match (look.palette, look.mode) {
        (Some(p), _) => {
            if p.is_dark() { egui::ThemePreference::Dark } else { egui::ThemePreference::Light }
        }
        (None, Mode::System) => egui::ThemePreference::System,
        (None, Mode::Light) => egui::ThemePreference::Light,
        (None, Mode::Dark) => egui::ThemePreference::Dark,
    });
    let accent = look.accent;
    let pal = look.palette;
    let status = StatusColors::from_theme(accent);
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("rivet-status-colors"), status));
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let dark = theme == egui::Theme::Dark;
        // Start from a clean style each time so colours from an old palette never linger.
        let mut style = egui::Style { visuals: if dark { egui::Visuals::dark() } else { egui::Visuals::light() }, ..Default::default() };
        style.visuals.dark_mode = dark;
        ctx.set_style_of(theme, style);
        ctx.style_mut_of(theme, |st| {
            let v = &mut st.visuals;
            v.selection.bg_fill = accent;
            v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
            v.hyperlink_color = accent;
            v.error_fg_color = status.danger;
            v.warn_fg_color = status.warning;
            if dark {
                // Elevation by surface: content (lowest) < chrome < pop-ups.
                v.extreme_bg_color = Color32::from_rgb(0x24, 0x24, 0x24);
                v.panel_fill = Color32::from_rgb(0x30, 0x30, 0x30);
                v.window_fill = Color32::from_rgb(0x3a, 0x3a, 0x3a);
                v.faint_bg_color = Color32::from_rgb(0x2b, 0x2b, 0x2b);
            } else {
                // Light: a slightly deeper chrome and darker secondary text, so
                // edges, labels and pale icons keep enough contrast on white.
                v.extreme_bg_color = Color32::WHITE;
                v.panel_fill = Color32::from_rgb(0xea, 0xea, 0xec);
                v.window_fill = Color32::from_rgb(0xfb, 0xfb, 0xfc);
                v.faint_bg_color = Color32::from_rgb(0xf3, 0xf3, 0xf5);
                v.weak_text_color = Some(Color32::from_gray(104));
                v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xe2, 0xe2, 0xe5);
                // Checkboxes, slider rails and other controls need a visible well on white.
                v.widgets.inactive.bg_fill = Color32::from_rgb(0xd3, 0xd3, 0xd8);
                v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xdc, 0xdc, 0xe0);
            }
            for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                w.corner_radius = CornerRadius::same(r_sm());
            }
            v.window_corner_radius = CornerRadius::same(r_lg());
            v.menu_corner_radius = CornerRadius::same(r_md());
            // No outlines: pop-ups stand out by their raised surface and shadow.
            v.window_stroke = Stroke::NONE;
            if let Some(p) = pal.filter(|p| p.is_dark() == dark) {
                let bg = p.window_bg.or(p.view_bg).unwrap_or(v.panel_fill);
                let fg = p.window_fg.or(p.view_fg).unwrap_or(if dark { Color32::from_gray(225) } else { Color32::from_gray(30) });
                let view = p.view_bg.unwrap_or(bg);
                // Toolbar, sidebar, menu bar and status bar share one raised surface.
                v.panel_fill = p.headerbar_bg.or(p.sidebar_bg).unwrap_or_else(|| mix(bg, fg, 0.05));
                v.extreme_bg_color = view;
                v.faint_bg_color = mix(view, fg, 0.035);
                if v.panel_fill == view {
                    v.panel_fill = mix(view, fg, 0.05); // keep the two levels distinguishable
                }
                let chrome = v.panel_fill;
                v.window_fill = p.popover_bg.filter(|c| *c != chrome && *c != view).unwrap_or_else(|| mix(chrome, fg, 0.07));
                v.text_edit_bg_color = Some(mix(view, fg, 0.05));
                // Secondary text: lighter than the main text, but readable on light schemes too.
                v.weak_text_color = Some(mix(fg, view, if dark { 0.4 } else { 0.32 }));
                v.selection.stroke = Stroke::new(1.0, p.accent_fg.unwrap_or(if palette::luminance(accent) > 0.6 { Color32::BLACK } else { Color32::WHITE }));
                v.widgets.noninteractive.bg_fill = bg;
                v.widgets.noninteractive.weak_bg_fill = bg;
                v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, mix(bg, fg, 0.12));
                for (w, t) in [(&mut v.widgets.inactive, 0.08), (&mut v.widgets.hovered, 0.15), (&mut v.widgets.active, 0.22), (&mut v.widgets.open, 0.12)] {
                    w.bg_fill = mix(bg, fg, t);
                    w.weak_bg_fill = mix(bg, fg, t);
                }
                for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                    w.fg_stroke.color = fg;
                }
                v.widgets.hovered.bg_stroke = Stroke::new(1.0, mix(bg, fg, 0.25));
                if let Some(d) = p.destructive {
                    v.error_fg_color = d;
                }
            }
            if look.terminal {
                // Terminal mode: one flat background, thin box lines, nothing floating.
                let fg = v.text_color();
                v.panel_fill = v.extreme_bg_color;
                v.faint_bg_color = v.extreme_bg_color;
                let line = mix(v.extreme_bg_color, fg, 0.28);
                v.window_stroke = Stroke::new(1.0, line);
                v.window_shadow = egui::Shadow::NONE;
                v.popup_shadow = egui::Shadow::NONE;
                v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line);
                v.widgets.inactive.bg_stroke = Stroke::new(1.0, line);
                v.window_fill = mix(v.extreme_bg_color, fg, 0.04);
            }
            if crate::a11y::high_contrast() {
                // Higher contrast: text at full strength, secondary text close to it, and every
                // edge drawn.
                let fg = if v.dark_mode { Color32::from_gray(248) } else { Color32::from_gray(8) };
                let bg = v.panel_fill;
                for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                    w.fg_stroke.color = fg;
                }
                v.weak_text_color = Some(mix(fg, bg, 0.2));
                let edge = Stroke::new(1.0, mix(bg, fg, 0.65));
                v.window_stroke = edge;
                v.widgets.noninteractive.bg_stroke = edge;
                v.widgets.inactive.bg_stroke = edge;
                v.widgets.hovered.bg_stroke = Stroke::new(1.5, fg);
                v.widgets.active.bg_stroke = Stroke::new(1.5, fg);
                v.widgets.open.bg_stroke = edge;
            }
            if crate::a11y::reduced_motion() {
                st.animation_time = 0.0;
                st.scroll_animation = egui::style::ScrollAnimation::none();
            }
            st.spacing.item_spacing = Vec2::new(6.0, 4.0);
            st.spacing.button_padding = Vec2::new(6.0, 3.0);
            st.spacing.interact_size.y = 24.0;
            // A little more room inside every pop-up menu.
            st.spacing.menu_margin = egui::Margin::same(8);
            for (ts, size) in [
                (egui::TextStyle::Body, 13.5),
                (egui::TextStyle::Button, 13.5),
                (egui::TextStyle::Small, 11.0),
                (egui::TextStyle::Heading, 18.0),
            ] {
                if let Some(f) = st.text_styles.get_mut(&ts) {
                    f.size = size * crate::fonts::scale();
                }
            }
        });
    }
}

/// Paint a texture fitted (aspect kept) and centred in `rect`, tinted.
pub fn paint_tex(painter: &egui::Painter, tex: &egui::TextureHandle, rect: Rect, tint: Color32) {
    let size = tex.size_vec2();
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let r = Rect::from_center_size(rect.center(), size * scale);
    painter.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
}

/// The tile behind a selected item on a surface `bg`: a light accent tint and an accent edge.
pub fn selected_tile(v: &egui::Visuals, bg: Color32) -> (Color32, Stroke) {
    (mix(bg, v.selection.bg_fill, 0.30), Stroke::new(1.0, v.selection.bg_fill.gamma_multiply(0.65)))
}

//! The part of a person's settings the lock screen and the login screen use: the style and
//! accent (and the secondary and tertiary colors and surfaces that go with them), the font and
//! text size, the clock, and whether the wallpaper is blurred and the power buttons shown.
//!
//! As text it's Rivet Shell's settings lines (`key=value`) for those keys only, so a look file
//! left for the greeter reads the same whichever program wrote it, and unknown lines are
//! skipped.

use eframe::egui::{self, Color32};
use rivet_look::live::{Choice, LiveLook};
use rivet_look::theme::Mode;

/// The keys a look file holds: how the lock screen looks, nothing private.
pub const KEYS: &[&str] = &["theme", "accent", "secondary", "tertiary", "color_use", "text_size", "font", "clock_format", "lock_blur", "lock_power", "surface_dark", "surface_light", "surface_tint"];

/// The usual surface grays, dark and light.
pub const SURFACES: [[u8; 3]; 2] = [[0x30, 0x30, 0x30], [0xea, 0xea, 0xec]];

#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub theme: Mode,
    pub accent: [u8; 3],
    pub secondary: Option<[u8; 3]>,
    pub tertiary: Option<[u8; 3]>,
    /// How much Rivet Shell uses the secondary and tertiary colors (kept as it came).
    pub color_use: u8,
    /// Text size in points; 0 follows the desktop's.
    pub text_size: f32,
    /// The font's family name; empty: the desktop's.
    pub font: String,
    /// The panel clock's format; the screen takes its hours from it.
    pub clock_format: String,
    /// The wallpaper blurred behind the screen.
    pub blur: bool,
    /// Suspend, restart and shut down on the screen.
    pub power: bool,
    pub surface_dark: Option<[u8; 3]>,
    pub surface_light: Option<[u8; 3]>,
    pub surface_tint: bool,
    /// The desktop's colors, followed live, rather than these. Not in the look file.
    pub follow_desktop_colors: bool,
    /// The surfaces made from the accent apply (Rivet Shell's own colors are in use). Not in
    /// the look file.
    pub own_surfaces: bool,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            theme: Mode::System,
            accent: [0xe6, 0x61, 0x00],
            secondary: None,
            tertiary: None,
            color_use: 1,
            text_size: 0.0,
            font: String::new(),
            clock_format: "%a %b %-d  %H:%M".into(),
            blur: true,
            power: true,
            surface_dark: None,
            surface_light: None,
            surface_tint: false,
            follow_desktop_colors: true,
            own_surfaces: false,
        }
    }
}

fn rgb_from(v: &str) -> Option<[u8; 3]> {
    let n = u32::from_str_radix(v.strip_prefix('#').filter(|h| h.len() == 6)?, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn rgb_text(c: Option<[u8; 3]>) -> String {
    c.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}")).unwrap_or_default()
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::System => "system",
        Mode::Light => "light",
        Mode::Dark => "dark",
    }
}

/// A surface color in a dark style or a light one: the chosen one (else the usual gray), with
/// a little of the accent when tinted. None: the usual, untouched.
pub fn surface(accent: [u8; 3], chosen: Option<[u8; 3]>, tint: bool, dark: bool) -> Option<Color32> {
    if chosen.is_none() && !tint {
        return None;
    }
    let [r, g, b] = chosen.unwrap_or(SURFACES[if dark { 0 } else { 1 }]);
    let base = Color32::from_rgb(r, g, b);
    if !tint {
        return Some(base);
    }
    let [r, g, b] = accent;
    Some(rivet_look::palette::mix(base, Color32::from_rgb(r, g, b), if dark { 0.12 } else { 0.07 }))
}

impl Look {
    /// A look from a look file's text, over the defaults.
    pub fn from_text(text: &str) -> Look {
        let mut l = Look::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            let b = v == "true";
            match k.trim() {
                "theme" => {
                    l.theme = match v {
                        "light" => Mode::Light,
                        "dark" => Mode::Dark,
                        _ => Mode::System,
                    }
                }
                "accent" => {
                    if let Some(c) = rgb_from(v) {
                        l.accent = c;
                    }
                }
                "secondary" => l.secondary = rgb_from(v),
                "tertiary" => l.tertiary = rgb_from(v),
                "color_use" => l.color_use = v.parse::<u8>().unwrap_or(1),
                "text_size" => l.text_size = v.parse::<f32>().unwrap_or(0.0).clamp(0.0, 24.0),
                "font" => l.font = v.replace([';', '=', '\n', '\t'], "").trim().to_string(),
                "clock_format" => l.clock_format = v.to_string(),
                "lock_blur" => l.blur = b,
                "lock_power" => l.power = b,
                "surface_dark" => l.surface_dark = rgb_from(v),
                "surface_light" => l.surface_light = rgb_from(v),
                "surface_tint" => l.surface_tint = b,
                _ => {}
            }
        }
        l
    }

    /// The look file's text: the keys in `KEYS`, one to a line.
    pub fn text(&self) -> String {
        let [r, g, b] = self.accent;
        format!(
            "theme={}\naccent=#{r:02x}{g:02x}{b:02x}\ntext_size={}\nclock_format={}\nfont={}\nsurface_dark={}\nsurface_light={}\nsurface_tint={}\ncolor_use={}\nsecondary={}\ntertiary={}\nlock_blur={}\nlock_power={}\n",
            mode_name(self.theme),
            self.text_size,
            self.clock_format.replace('\n', " "),
            self.font,
            rgb_text(self.surface_dark),
            rgb_text(self.surface_light),
            self.surface_tint,
            self.color_use,
            rgb_text(self.secondary),
            rgb_text(self.tertiary),
            self.blur,
            self.power
        )
    }

    pub fn choice(&self) -> Choice {
        Choice { mode: self.theme, accent: self.accent, follow_desktop_colors: self.follow_desktop_colors, text_size: self.text_size }
    }

    /// The surface color for the dark style or the light one (see `surface`).
    pub fn surface(&self, dark: bool) -> Option<Color32> {
        surface(self.accent, if dark { self.surface_dark } else { self.surface_light }, self.surface_tint, dark)
    }
}

/// A look applied to one surface's egui context, frame by frame.
pub struct Theme {
    live_look: LiveLook,
    live: bool,
}

impl Theme {
    /// `live`: follow the desktop's colors as they change (a real screen, not a screenshot).
    pub fn new(live: bool) -> Theme {
        Theme { live_look: LiveLook::new().without_watcher(), live }
    }

    /// Call first in every frame.
    pub fn begin(&mut self, ctx: &egui::Context, l: &Look) {
        self.live_look.set_fonts(ctx, &l.font, "", "");
        if l.own_surfaces {
            let [r, g, b] = l.accent;
            let key = format!("{:?}{:?}{:?}{}", l.accent, l.surface_dark, l.surface_light, l.surface_tint);
            let css = |dark| rivet_look::palette::own_css(dark, Color32::from_rgb(r, g, b), l.surface(dark));
            self.live_look.set_own(&key, || [rivet_look::palette::from_css(&css(true)), rivet_look::palette::from_css(&css(false))]);
        } else {
            self.live_look.set_own("", || [None, None]);
        }
        self.live_look.frame(ctx, &l.choice(), self.live);
    }

    pub fn accent(&self, l: &Look) -> Color32 {
        self.live_look.accent(&l.choice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips() {
        let l = Look { theme: Mode::Dark, accent: [200, 40, 120], secondary: Some([0, 132, 184]), tertiary: None, color_use: 2, text_size: 12.5, font: "Inter".into(), clock_format: "%-I:%M %p".into(), blur: false, power: false, surface_dark: Some([1, 2, 3]), surface_light: None, surface_tint: true, ..Look::default() };
        assert_eq!(Look::from_text(&l.text()), l);
    }

    #[test]
    fn text_holds_only_its_keys() {
        for line in Look::default().text().lines() {
            let k = line.split_once('=').unwrap().0;
            assert!(KEYS.contains(&k), "{k}");
        }
        assert_eq!(Look::default().text().lines().count(), KEYS.len());
    }

    /// A look file Rivet Shell 0.1.2 left: other keys skipped, the rest read as it meant them.
    #[test]
    fn reads_what_rivet_shell_left() {
        let l = Look::from_text("theme=light\naccent=#3584e4\nfollow_desktop_colors=false\nwallpaper=/x.png\nclock_format=%H.%M\nlock_blur=false\n");
        assert_eq!((l.theme, l.accent, l.clock_format.as_str(), l.blur, l.power), (Mode::Light, [0x35, 0x84, 0xe4], "%H.%M", false, true));
        assert!(l.follow_desktop_colors);
    }

    #[test]
    fn surfaces() {
        assert_eq!(surface([0, 0, 0], None, false, true), None);
        assert_eq!(surface([0, 0, 0], Some([10, 20, 30]), false, true), Some(Color32::from_rgb(10, 20, 30)));
        assert!(surface([255, 0, 0], None, true, true).is_some_and(|c| c.r() > 0x30));
    }
}

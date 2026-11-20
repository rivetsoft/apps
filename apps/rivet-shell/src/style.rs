//! How the shell's surfaces look (corner radius, opacity, background and highlight colours),
//! set once on the Appearance page and cascading to every panel and the dock: each of those
//! can take its own value for any of them, and otherwise follows Appearance.
//!
//! In the settings file a look is one line of the values set, e.g.
//! `look=radius=14;opacity=0.92;fill=secondary;tint=0.3;highlight=tertiary;border=true;popups=attached`;
//! a panel's sits in its own line as `look=radius:8,fill:primary`.

use eframe::egui::{self, Color32, ecolor::Hsva};
use rivet_look::live::LiveLook;
use rivet_look::palette::{luminance, mix};

use crate::config::Settings;

/// A colour from the theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Role {
    /// The theme's own panel colour (as a background), or the accent (as a highlight).
    #[default]
    Surface,
    Primary,
    Secondary,
    Tertiary,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Surface, Role::Primary, Role::Secondary, Role::Tertiary];
    pub const ACCENTS: [Role; 3] = [Role::Primary, Role::Secondary, Role::Tertiary];
    pub fn name(self) -> &'static str {
        match self {
            Role::Surface => "surface",
            Role::Primary => "primary",
            Role::Secondary => "secondary",
            Role::Tertiary => "tertiary",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Role::Surface => "Surface",
            Role::Primary => "Primary",
            Role::Secondary => "Secondary",
            Role::Tertiary => "Tertiary",
        }
    }
    pub fn from(v: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.name() == v)
    }
}

/// A look; a value left out follows the level above (a panel or the dock: Appearance;
/// Appearance: `BASE`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Style {
    /// Corner radius in points (a floating panel's and the dock's).
    pub radius: Option<f32>,
    /// How solid the background is, 0.3 to 1.
    pub opacity: Option<f32>,
    /// The background's colour.
    pub fill: Option<Role>,
    /// How much of the fill's colour the background takes (Surface: none), 0 to 1.
    pub tint: Option<f32>,
    /// Active workspaces, the dock's dots, the battery charging…
    pub highlight: Option<Role>,
    /// A thin outline.
    pub border: Option<bool>,
    /// A panel's popups open joined to it (else floating a little off it).
    pub attach: Option<bool>,
}

/// What a look comes to once every level is applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolved {
    pub radius: f32,
    pub opacity: f32,
    pub fill: Role,
    pub tint: f32,
    pub highlight: Role,
    pub border: bool,
    pub attach: bool,
}

pub const BASE: Resolved = Resolved { radius: 14.0, opacity: 0.92, fill: Role::Surface, tint: 0.3, highlight: Role::Primary, border: true, attach: false };

impl Style {
    /// This look with `o`'s values where it has them.
    pub fn over(self, o: Style) -> Style {
        Style {
            radius: o.radius.or(self.radius),
            opacity: o.opacity.or(self.opacity),
            fill: o.fill.or(self.fill),
            tint: o.tint.or(self.tint),
            highlight: o.highlight.or(self.highlight),
            border: o.border.or(self.border),
            attach: o.attach.or(self.attach),
        }
    }

    /// Filled in from `base` where this look leaves a value out.
    pub fn on(self, base: Resolved) -> Resolved {
        Resolved {
            radius: self.radius.unwrap_or(base.radius),
            opacity: self.opacity.unwrap_or(base.opacity),
            fill: self.fill.unwrap_or(base.fill),
            tint: self.tint.unwrap_or(base.tint),
            highlight: self.highlight.unwrap_or(base.highlight),
            border: self.border.unwrap_or(base.border),
            attach: self.attach.unwrap_or(base.attach),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Style::default()
    }

    /// `key<kv>value<sep>…`, the values set.
    pub fn text(&self, kv: char, sep: char) -> String {
        let mut out: Vec<String> = Vec::new();
        if let Some(r) = self.radius {
            out.push(format!("radius{kv}{r}"));
        }
        if let Some(o) = self.opacity {
            out.push(format!("opacity{kv}{o}"));
        }
        if let Some(f) = self.fill {
            out.push(format!("fill{kv}{}", f.name()));
        }
        if let Some(t) = self.tint {
            out.push(format!("tint{kv}{t}"));
        }
        if let Some(h) = self.highlight {
            out.push(format!("highlight{kv}{}", h.name()));
        }
        if let Some(b) = self.border {
            out.push(format!("border{kv}{b}"));
        }
        if let Some(a) = self.attach {
            out.push(format!("popups{kv}{}", if a { "attached" } else { "float" }));
        }
        out.join(&sep.to_string())
    }

    pub fn parse(v: &str, kv: char, sep: char) -> Style {
        let mut s = Style::default();
        let num = |x: &str, lo: f32, hi: f32| x.trim().parse::<f32>().ok().filter(|n| n.is_finite()).map(|n| n.clamp(lo, hi));
        for part in v.split(sep) {
            let Some((k, x)) = part.split_once(kv) else { continue };
            let x = x.trim();
            match k.trim() {
                "radius" => s.radius = num(x, 0.0, 24.0),
                "opacity" => s.opacity = num(x, 0.3, 1.0),
                "fill" => s.fill = Role::from(x),
                "tint" => s.tint = num(x, 0.0, 1.0),
                "highlight" => s.highlight = Role::from(x).filter(|r| *r != Role::Surface),
                "border" => s.border = Some(x == "true"),
                "popups" => s.attach = Some(x == "attached"),
                _ => {}
            }
        }
        s
    }
}

/// What a coloured part of the shell is, which decides the colour it takes once the shell is
/// set to use more than the primary (`Settings::color_use`: 0 primary only, 1 some, 2 a lot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// A switch or a quick setting that's on.
    Toggle,
    /// Notifications: the bell's dot, a notification's own symbol, Do Not Disturb.
    Notify,
    /// The second and third of a set of bars (the system widget's memory and battery).
    Bar2,
    Bar3,
    /// A slider's filled part (volume, brightness, the on-screen display).
    Slider,
    /// The thing in use in a list (the Wi-Fi network joined, the sound device, a Bluetooth device).
    Active,
    /// A running app's dot, and what's playing.
    Dot,
}

/// How much the shell uses the secondary and tertiary colours.
pub const COLOR_USE_MAX: u8 = 2;

impl Slot {
    pub fn role(self, color_use: u8) -> Role {
        let (from, role) = match self {
            Slot::Toggle => (1, Role::Secondary),
            Slot::Notify => (1, Role::Tertiary),
            Slot::Bar2 => (1, Role::Secondary),
            Slot::Bar3 => (1, Role::Tertiary),
            Slot::Slider => (2, Role::Secondary),
            Slot::Active => (2, Role::Secondary),
            Slot::Dot => (2, Role::Tertiary),
        };
        if color_use >= from { role } else { Role::Primary }
    }
}

impl Settings {
    /// Appearance's look for the shell's surfaces.
    pub fn base_look(&self) -> Resolved {
        self.look.on(BASE)
    }
    /// A panel's look.
    pub fn panel_look(&self, idx: usize) -> Resolved {
        let own = self.panels.get(idx).map(|p| p.look).unwrap_or_default();
        self.look.over(own).on(BASE)
    }
    /// Put the panel list right after a change (see `panels::tidy`), and work out how far each
    /// drawer reaches under its host's corners: the host's radius, when it's a floating bar.
    pub fn tidy_panels(&mut self) {
        crate::panels::tidy(&mut self.panels);
        for i in 0..self.panels.len() {
            let host = self.panels[i].sub.as_ref().filter(|s| s.drawer).and_then(|s| s.host).filter(|h| self.panels[*h].floating);
            self.panels[i].lead = host.map_or(0.0, |h| radius(&self.panel_look(h)).nw as f32);
        }
    }
    pub fn dock_look(&self) -> Resolved {
        self.look.over(self.dock_look).on(BASE)
    }
    /// The notification cards' look.
    pub fn note_look(&self) -> Resolved {
        self.look.over(self.note_look).on(BASE)
    }
}

/// The theme's primary, secondary and tertiary colours: the desktop's (matugen) when followed,
/// the ones chosen with a style and accent of the shell's own, else made from the accent
/// (secondary: a quieter primary; tertiary: its hue turned a sixth, softened).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roles {
    pub primary: Color32,
    pub secondary: Color32,
    pub tertiary: Color32,
}

impl Roles {
    pub fn new(look: &LiveLook, s: &Settings) -> Roles {
        let choice = s.choice();
        let primary = look.accent(&choice);
        let pal = rivet_look::palette::shown(look.palette(), choice.follow_desktop_colors);
        let hsva = Hsva::from(primary);
        // With the shell's own style and accent, the two are the ones chosen or made from the
        // accent: never a theme file's, which may be left from colours used before.
        let (theme_secondary, theme_tertiary) = if s.own_colors() { s.own_roles() } else { (pal.and_then(|p| p.secondary), pal.and_then(|p| p.tertiary)) };
        let secondary = theme_secondary.unwrap_or_else(|| Color32::from(Hsva { s: hsva.s * 0.4, ..hsva }));
        let tertiary = theme_tertiary.unwrap_or_else(|| Color32::from(Hsva { h: (hsva.h + 1.0 / 6.0).fract(), s: hsva.s * 0.6, v: hsva.v * 0.85, ..hsva }));
        Roles { primary, secondary, tertiary }
    }

    /// A part's colour, by how much the shell uses the secondary and tertiary.
    pub fn tone(&self, slot: Slot, color_use: u8) -> Color32 {
        self.get(slot.role(color_use), self.primary)
    }

    /// A role's colour (`surface`: what Surface stands for).
    pub fn get(&self, role: Role, surface: Color32) -> Color32 {
        match role {
            Role::Surface => surface,
            Role::Primary => self.primary,
            Role::Secondary => self.secondary,
            Role::Tertiary => self.tertiary,
        }
    }
}

/// A surface's visuals with its look's background (and text that reads on it), and its
/// highlight colour.
pub fn visuals(v: &egui::Visuals, look: &Resolved, roles: &Roles) -> (egui::Visuals, Color32) {
    let mut out = v.clone();
    if look.fill != Role::Surface && look.tint > 0.0 {
        let fill = mix(v.panel_fill, roles.get(look.fill, v.panel_fill), look.tint);
        out.panel_fill = fill;
        let text = v.text_color();
        if (luminance(fill) - luminance(text)).abs() < 0.45 {
            let t = crate::ui::on(fill);
            out.override_text_color = Some(t);
            out.weak_text_color = Some(mix(t, fill, 0.35));
        }
    }
    let highlight = roles.get(look.highlight, roles.primary);
    (out, highlight)
}

/// The corner radius to draw with (square in terminal mode).
pub fn radius(look: &Resolved) -> egui::CornerRadius {
    if rivet_look::theme::square() { egui::CornerRadius::ZERO } else { egui::CornerRadius::from(look.radius.round().clamp(0.0, 255.0) as u8) }
}

/// The radius of things inside a surface (hover fills…), kept concentric with it.
pub fn inner_radius(look: &Resolved, pad: f32) -> f32 {
    if rivet_look::theme::square() { 0.0 } else { (look.radius - pad).clamp(0.0, rivet_look::theme::r_md() as f32) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this session's theme gives (`cargo test live_roles -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn live_roles() {
        let s = Settings::load();
        let look = LiveLook::new();
        let r = Roles::new(&look, &s);
        let pal = look.palette();
        println!("primary {:?}\nsecondary {:?} (from the theme: {})\ntertiary {:?} (from the theme: {})\ncolor_use {}", r.primary, r.secondary, pal.is_some_and(|p| p.secondary.is_some()), r.tertiary, pal.is_some_and(|p| p.tertiary.is_some()), s.color_use);
    }

    #[test]
    fn slots_take_more_colours_as_asked() {
        for s in [Slot::Toggle, Slot::Notify, Slot::Bar2, Slot::Bar3, Slot::Slider, Slot::Active, Slot::Dot] {
            assert_eq!(s.role(0), Role::Primary);
            assert_ne!(s.role(2), Role::Primary);
        }
        assert_eq!((Slot::Toggle.role(1), Slot::Slider.role(1), Slot::Slider.role(2)), (Role::Secondary, Role::Primary, Role::Secondary));
    }

    #[test]
    fn cascades_and_round_trips() {
        let app = Style { radius: Some(4.0), fill: Some(Role::Secondary), ..Style::default() };
        let panel = Style { fill: Some(Role::Tertiary), border: Some(false), attach: Some(true), ..Style::default() };
        let r = app.over(panel).on(BASE);
        assert_eq!((r.radius, r.fill, r.border, r.opacity), (4.0, Role::Tertiary, false, BASE.opacity));
        assert_eq!(Style::parse(&app.text('=', ';'), '=', ';'), app);
        assert_eq!(Style::parse(&panel.text(':', ','), ':', ','), panel);
        assert_eq!(Style::parse("radius=99;highlight=surface", '=', ';').radius, Some(24.0));
        assert!(Style::parse("highlight=surface", '=', ';').is_empty());
    }
}

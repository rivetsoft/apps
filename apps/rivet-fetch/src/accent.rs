//! The desktop's accent, read from the same GTK colour files and in the same order as
//! `rivet_look::palette`, which can't be used here because it brings the whole windowing stack
//! into a terminal program.

use std::collections::HashMap;
use std::path::PathBuf;

/// The colour names an accent goes by, best first, as `rivet_look::palette` reads them.
const NAMES: [&str; 4] = ["accent_bg_color", "accent_color", "theme_selected_bg_color", "primary"];

fn candidates() -> Vec<PathBuf> {
    let c = rivet_xdg::places::config_home();
    ["rivet/palette.css", "rivet/colors.css", "gtk-4.0/gtk.css", "gtk-4.0/colors.css", "gtk-3.0/gtk.css", "gtk-3.0/colors.css"].iter().map(|f| c.join(f)).collect()
}

pub fn load() -> Option<(u8, u8, u8)> {
    candidates().iter().find_map(|p| from_css(&std::fs::read_to_string(p).ok()?))
}

/// The accent among a file's `@define-color name value;` lines.
pub fn from_css(css: &str) -> Option<(u8, u8, u8)> {
    let mut defs = HashMap::new();
    for stmt in css.split(';') {
        let Some(i) = stmt.find("@define-color") else { continue };
        let mut parts = stmt[i + 13..].split_whitespace();
        if let (Some(name), Some(value)) = (parts.next(), parts.next()) {
            defs.insert(name.to_string(), value.to_string());
        }
    }
    NAMES.iter().find_map(|n| resolve(defs.get(*n)?, &defs, 0))
}

fn resolve(value: &str, defs: &HashMap<String, String>, depth: usize) -> Option<(u8, u8, u8)> {
    if let Some(name) = value.strip_prefix('@') {
        return if depth < 8 { resolve(defs.get(name)?, defs, depth + 1) } else { None };
    }
    let hex = value.strip_prefix('#')?;
    let byte = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        3 => Some((byte(&hex[0..1].repeat(2))?, byte(&hex[1..2].repeat(2))?, byte(&hex[2..3].repeat(2))?)),
        6 | 8 => Some((byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_accent() {
        assert_eq!(from_css("@define-color accent_bg_color #ffb1c3;\n@define-color x #000;"), Some((0xff, 0xb1, 0xc3)));
        assert_eq!(from_css("@define-color blue #36c; @define-color accent_color @blue;"), Some((0x33, 0x66, 0xcc)));
        assert_eq!(from_css("@define-color window_bg_color #000;"), None);
    }
}

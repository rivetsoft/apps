//! Qt apps in the desktop's colors. Qt has no color files of its own to follow, so Rivet keeps
//! a color scheme in the format qt6ct and qt5ct read (the Qt settings tools, which plug into
//! every Qt app through the platform theme named in `QT_QPA_PLATFORMTHEME`), written from the
//! same palette Rivet's apps show. Each tool reloads its settings folder a few seconds after a
//! file in it changes, so open Qt apps change color too.
//!
//! Rivet's scheme is a file of its own in each tool's folder. The tool's own settings file
//! (`qt6ct.conf`) is written only when there is none yet, or when the person asks: the two
//! keys that pick Rivet's scheme are set and everything else in it stays.

use std::path::{Path, PathBuf};

use eframe::egui::Color32;

use crate::palette::{Palette, luminance, mix};
use crate::places;

/// The name of Rivet's scheme in each tool's folder.
pub const FILE: &str = "rivet-colors.conf";

/// The platform theme to name in `QT_QPA_PLATFORMTHEME`. qt6ct answers to this name as well
/// as its own, so one value reaches Qt 5 apps through qt5ct and Qt 6 apps through qt6ct.
pub const THEME: &str = "qt5ct";

/// The two tools: their folder and settings file under the config folder, and their plugin.
const TOOLS: [(&str, &str); 2] = [("qt6ct", "libqt6ct.so"), ("qt5ct", "libqt5ct.so")];

fn dir(tool: &str) -> PathBuf {
    places::config_home().join(tool)
}

/// Rivet's scheme in this tool's folder.
pub fn scheme_path(tool: &str) -> PathBuf {
    dir(tool).join(FILE)
}

fn conf_path(tool: &str) -> PathBuf {
    dir(tool).join(format!("{tool}.conf"))
}

/// Folders Qt looks for plugins in: `QT_PLUGIN_PATH`, then where distributions put Qt 5's
/// and Qt 6's.
fn plugin_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::env::var_os("QT_PLUGIN_PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for lib in ["/usr/lib64", "/usr/lib", "/usr/local/lib64", "/usr/local/lib"] {
        let lib = Path::new(lib);
        let mut roots = vec![lib.to_path_buf()];
        // Debian and Ubuntu keep them under the architecture's own folder.
        if let Ok(rd) = std::fs::read_dir(lib) {
            roots.extend(rd.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with("-linux-gnu"))));
        }
        for r in roots {
            v.push(r.join("qt6/plugins"));
            v.push(r.join("qt5/plugins"));
        }
    }
    v
}

/// Which of the two tools have their plugin installed.
pub fn installed() -> Vec<&'static str> {
    let dirs = plugin_dirs();
    TOOLS.iter().filter(|(_, lib)| dirs.iter().any(|d| d.join("platformthemes").join(lib).is_file())).map(|(t, _)| *t).collect()
}

/// The keys of one `[section]` in an ini file's text, with their values.
fn section<'a>(text: &'a str, name: &str) -> Vec<(&'a str, &'a str)> {
    let mut inside = false;
    let mut out = Vec::new();
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            inside = l == format!("[{name}]");
        } else if inside && let Some((k, v)) = l.split_once('=') {
            out.push((k.trim(), v.trim()));
        }
    }
    out
}

/// The text with these keys set in `[section]`: replaced where they are, added at the end of
/// the section where they aren't, and the section added where there is none. Every other line
/// stays as it is.
fn with_keys(text: &str, name: &str, keys: &[(&str, &str)]) -> String {
    let header = format!("[{name}]");
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let Some(start) = lines.iter().position(|l| l.trim() == header) else {
        let mut t = text.to_string();
        if !t.is_empty() && !t.ends_with('\n') {
            t.push('\n');
        }
        if !t.is_empty() {
            t.push('\n');
        }
        t.push_str(&header);
        t.push('\n');
        for (k, v) in keys {
            t.push_str(&format!("{k}={v}\n"));
        }
        return t;
    };
    let end = lines[start + 1..].iter().position(|l| l.trim().starts_with('[')).map(|i| start + 1 + i).unwrap_or(lines.len());
    let mut last = start;
    let mut missing = Vec::new();
    for (k, v) in keys {
        match (start + 1..end).find(|&i| lines[i].split_once('=').is_some_and(|(key, _)| key.trim() == *k)) {
            Some(i) => {
                lines[i] = format!("{k}={v}");
                last = last.max(i);
            }
            None => missing.push(format!("{k}={v}")),
        }
    }
    if let Some(i) = (start + 1..end).rev().find(|&i| !lines[i].trim().is_empty()) {
        last = last.max(i);
    }
    for (n, l) in missing.into_iter().enumerate() {
        lines.insert(last + 1 + n, l);
    }
    let mut t = lines.join("\n");
    t.push('\n');
    t
}

/// Whether a tool's settings pick Rivet's scheme.
fn picks_ours(conf: &str, ours: &Path) -> bool {
    let keys = section(conf, "Appearance");
    let get = |k: &str| keys.iter().rev().find(|(key, _)| *key == k).map(|(_, v)| *v);
    let path = get("color_scheme_path").map(|p| match p.strip_prefix("~/") {
        Some(rest) => places::home().join(rest),
        None => PathBuf::from(p),
    });
    get("custom_palette") == Some("true") && path.as_deref() == Some(ours)
}

/// Where the installed tools stand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    /// Neither tool is installed.
    Missing,
    /// Each installed tool picks Rivet's scheme.
    Picked,
    /// One has a scheme of its own chosen, or another left unchosen in a settings file that
    /// was already there: Rivet changes it only when asked (`pick`).
    Other,
}

pub fn state() -> State {
    let tools = installed();
    if tools.is_empty() {
        return State::Missing;
    }
    let all = tools.iter().all(|t| std::fs::read_to_string(conf_path(t)).is_ok_and(|c| picks_ours(&c, &scheme_path(t))));
    if all { State::Picked } else { State::Other }
}

/// Point each installed tool's settings at Rivet's scheme. Only the two keys that choose a
/// scheme change; the file is made when there is none.
pub fn pick() -> Result<(), String> {
    for tool in installed() {
        let file = conf_path(tool);
        // Through a link, so settings kept elsewhere (a dotfiles folder) stay linked.
        let file = std::fs::canonicalize(&file).unwrap_or(file);
        let old = std::fs::read_to_string(&file).unwrap_or_default();
        let ours = scheme_path(tool);
        if picks_ours(&old, &ours) {
            continue;
        }
        let text = with_keys(&old, "Appearance", &[("custom_palette", "true"), ("color_scheme_path", &ours.to_string_lossy())]);
        if let Some(d) = file.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(())
}

/// Bring Rivet's scheme up to date with this palette, in each installed tool's folder, and
/// pick it in a tool that has no settings file yet. A file is written only when it changes,
/// by renaming a finished one into place: the tools reload when their folder changes.
pub fn write(p: &Palette) -> std::io::Result<()> {
    let text = scheme(p);
    for tool in installed() {
        let path = scheme_path(tool);
        if !std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
            std::fs::create_dir_all(dir(tool))?;
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, &text)?;
            std::fs::rename(&tmp, &path)?;
        }
        if !conf_path(tool).exists() {
            std::fs::write(conf_path(tool), with_keys("", "Appearance", &[("custom_palette", "true"), ("color_scheme_path", &path.to_string_lossy())]))?;
        }
    }
    Ok(())
}

/// Take Rivet's scheme away again (Qt apps turned off in Rivet Shell's settings). A tool still
/// pointed at it falls back to its own colors.
pub fn remove() {
    for (tool, _) in TOOLS {
        let _ = std::fs::remove_file(scheme_path(tool));
    }
}

/// The platform theme a Rivet session should name for the Qt apps it starts, when Rivet's
/// scheme is in use: a tool is installed, Rivet's scheme is there and picked. Never on KDE
/// Plasma, whose own platform theme already colors Qt apps from Plasma's settings.
pub fn session_theme() -> Option<&'static str> {
    let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|d| d.eq_ignore_ascii_case("kde")));
    if kde {
        return None;
    }
    let tools = installed();
    let used = tools.iter().any(|t| {
        let ours = scheme_path(t);
        ours.is_file() && std::fs::read_to_string(conf_path(t)).is_ok_and(|c| picks_ours(&c, &ours))
    });
    used.then_some(THEME)
}

/// A palette as a qt5ct and qt6ct color scheme: Qt's color roles in Qt's order, for windows
/// that are active, inactive and disabled. Qt 5 reads the first 21 and Qt 6 all 22 (Accent).
pub fn scheme(p: &Palette) -> String {
    let dark = p.is_dark();
    let (white, black) = (Color32::WHITE, Color32::BLACK);
    let window = p.window_bg.or(p.view_bg).unwrap_or(if dark { Color32::from_gray(0x30) } else { Color32::from_rgb(0xea, 0xea, 0xec) });
    let window_text = p.window_fg.or(p.view_fg).unwrap_or(if luminance(window) < 0.5 { Color32::from_gray(0xd6) } else { Color32::from_gray(0x2a) });
    let base = p.view_bg.unwrap_or(window);
    let text = p.view_fg.unwrap_or(window_text);
    let accent = p.accent.unwrap_or(Color32::from_rgb(0x35, 0x84, 0xe4));
    let on_accent = p.accent_fg.unwrap_or(if luminance(accent) > 0.6 { black } else { white });
    let button = mix(window, window_text, if dark { 0.1 } else { 0.06 });
    let light = mix(button, white, if dark { 0.12 } else { 0.7 });
    let shade = mix(button, black, if dark { 0.45 } else { 0.3 });
    let tooltip = p.popover_bg.unwrap_or(button);
    let visited = p.tertiary.unwrap_or(mix(accent, window_text, 0.35));
    // Window text, button, light, midlight, dark, mid, text, bright text, button text, base,
    // window, shadow, highlight, highlighted text, link, visited link, alternate base, no role,
    // tooltip base, tooltip text, placeholder text, accent.
    let active = [
        window_text, button, light, mix(button, light, 0.5), shade, mix(button, shade, 0.5), text, white, window_text, base,
        window, black, accent, on_accent, accent, visited, mix(base, text, 0.04), window,
        tooltip, window_text, mix(text, base, 0.45), accent,
    ];
    let mut disabled = active;
    let faint = |c: Color32, bg: Color32| mix(c, bg, 0.55);
    disabled[0] = faint(window_text, window);
    disabled[6] = faint(text, base);
    disabled[8] = faint(window_text, button);
    disabled[12] = mix(window, window_text, 0.25);
    disabled[13] = faint(window_text, disabled[12]);
    disabled[20] = faint(disabled[20], base);
    let line = |cs: &[Color32; 22]| cs.iter().map(|c| format!("#ff{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())).collect::<Vec<_>>().join(", ");
    format!(
        "# Rivet writes this file from the desktop's colors (Rivet Shell's settings, Applications). Changes here are lost.\n[ColorScheme]\nactive_colors={}\ninactive_colors={}\ndisabled_colors={}\n",
        line(&active),
        line(&active),
        line(&disabled)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_set_in_their_section_and_the_rest_stays() {
        let old = "[Appearance]\ncustom_palette=false\nstyle=Breeze\n\n[Fonts]\nfixed=\"Mono,10\"\n";
        let new = with_keys(old, "Appearance", &[("custom_palette", "true"), ("color_scheme_path", "/x/rivet-colors.conf")]);
        assert_eq!(new, "[Appearance]\ncustom_palette=true\nstyle=Breeze\ncolor_scheme_path=/x/rivet-colors.conf\n\n[Fonts]\nfixed=\"Mono,10\"\n");
        assert!(picks_ours(&new, Path::new("/x/rivet-colors.conf")));
        assert!(!picks_ours(old, Path::new("/x/rivet-colors.conf")));
        assert_eq!(with_keys("", "Appearance", &[("a", "1")]), "[Appearance]\na=1\n");
        assert_eq!(with_keys("[Fonts]\nx=1", "Appearance", &[("a", "1")]), "[Fonts]\nx=1\n\n[Appearance]\na=1\n");
    }

    #[test]
    fn a_scheme_elsewhere_is_not_ours() {
        let c = "[Appearance]\ncustom_palette=true\ncolor_scheme_path=/usr/share/qt6ct/colors/darker.conf\n";
        assert!(!picks_ours(c, Path::new("/x/rivet-colors.conf")));
    }

    #[test]
    fn the_scheme_has_every_role_from_the_palette() {
        let p = crate::palette::from_css("@define-color accent_bg_color #eeb4e9; @define-color accent_fg_color #4a204b; @define-color window_bg_color #241e23; @define-color window_fg_color #ebdfe6; @define-color view_bg_color #171216; @define-color view_fg_color #ebdfe6;").unwrap();
        let s = scheme(&p);
        let roles = |key: &str| s.lines().find_map(|l| l.strip_prefix(key)).unwrap().split(", ").map(String::from).collect::<Vec<_>>();
        let active = roles("active_colors=");
        assert_eq!(active.len(), 22);
        assert_eq!((active[10].as_str(), active[9].as_str(), active[12].as_str(), active[13].as_str(), active[21].as_str()), ("#ff241e23", "#ff171216", "#ffeeb4e9", "#ff4a204b", "#ffeeb4e9"));
        assert_eq!(roles("disabled_colors=").len(), 22);
        assert!(!s.contains("colour"));
    }
}

//! The compositor's window look (gaps, borders, corners, inactive opacity, shadows), set from
//! Rivet Shell's settings: the shell writes one file of overrides in the compositor's own
//! format, next to its config, and the config includes it.
//!
//! - niri: `rivet-shell.kdl`, by `include "rivet-shell.kdl"` (niri 25.11 or later); niri reads
//!   it again by itself.
//! - Hyprland: `rivet-shell.conf` by `source = …` from hyprland.conf, or `rivet-shell.lua` by
//!   `require("rivet-shell")` from hyprland.lua; then `hyprctl reload`.
//! - mango: `rivet-shell.conf`, by `source=…` from config.conf, or from config.toml's
//!   `source = […]`; then `mmsg dispatch reload_config`.
//!
//! - Softcore: `rivet-shell.settings`, by `include=rivet-shell.settings` from its settings
//!   file; Softcore reads it again by itself.
//!
//! The override file is written whenever the look changes. The include line is added to the
//! person's config only when they ask (a copy of the config is kept first, once, as
//! `<config>.rivet-backup`), at its end so it wins over what's above it. Turned off, the file
//! keeps only a comment, so the include stays valid.

use std::path::{Path, PathBuf};

use crate::config::Settings;
use crate::displays::Display;
use crate::sys::Compositor;

/// What the overrides set.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// Between windows, and between windows and the screen's edges.
    pub gaps_in: u32,
    pub gaps_out: u32,
    /// The border's width (0: none).
    pub border: u32,
    pub radius: u32,
    /// The focused window's border and the others' (None: the compositor's own colours).
    pub colors: Option<([u8; 3], [u8; 3])>,
    /// Unfocused windows' opacity, 0.5–1.
    pub inactive_opacity: f32,
    pub shadows: bool,
}

impl Look {
    /// From the settings; `accent` and `dark` are the shell's look now (borders can follow them).
    pub fn from(s: &Settings, accent: [u8; 3], dark: bool) -> Look {
        let inactive = if dark { [0x45, 0x45, 0x4a] } else { [0xc4, 0xc4, 0xc8] };
        Look {
            gaps_in: s.comp_gaps_in,
            gaps_out: s.comp_gaps_out,
            border: s.comp_border,
            radius: s.comp_radius,
            colors: s.comp_accent_borders.then_some((accent, inactive)),
            inactive_opacity: s.comp_inactive_opacity.clamp(0.5, 1.0),
            shadows: s.comp_shadows,
        }
    }
}

/// The config format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Kdl,
    /// hyprlang (hyprland.conf).
    HyprConf,
    HyprLua,
    /// mango's key=value (config.conf).
    MangoConf,
    MangoToml,
    /// Softcore's key=value settings.
    Softcore,
}

/// Where the compositor's config is, and ours.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub comp: Compositor,
    pub format: Format,
    /// The config that includes ours.
    pub main: PathBuf,
    /// Our file.
    pub ours: PathBuf,
}

fn config_home() -> PathBuf {
    rivet_xdg::places::config_home()
}

/// The running compositor's target (None: not one the shell can set the look of).
pub fn target(comp: Compositor) -> Option<Target> {
    let cfg = config_home();
    match comp {
        Compositor::Niri => {
            let main = std::env::var_os("NIRI_CONFIG").map(PathBuf::from).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| cfg.join("niri/config.kdl"));
            let ours = main.parent().unwrap_or(Path::new(".")).join("rivet-shell.kdl");
            Some(Target { comp, format: Format::Kdl, main, ours })
        }
        Compositor::Hyprland => {
            let dir = cfg.join("hypr");
            // The Lua config wins where there is one.
            let lua = dir.join("hyprland.lua");
            Some(if lua.is_file() {
                Target { comp, format: Format::HyprLua, main: lua, ours: dir.join("rivet-shell.lua") }
            } else {
                Target { comp, format: Format::HyprConf, main: dir.join("hyprland.conf"), ours: dir.join("rivet-shell.conf") }
            })
        }
        Compositor::Mango => {
            let dir = cfg.join("mango");
            // mango prefers config.conf, then config.toml.
            let conf = dir.join("config.conf");
            let toml = dir.join("config.toml");
            let (format, main) = if !conf.is_file() && toml.is_file() { (Format::MangoToml, toml) } else { (Format::MangoConf, conf) };
            Some(Target { comp, format, main, ours: dir.join("rivet-shell.conf") })
        }
        Compositor::Softcore => {
            let main = std::env::var_os("SOFTCORE_CONFIG").map(PathBuf::from).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| cfg.join("softcore/settings"));
            let ours = main.parent().unwrap_or(Path::new(".")).join("rivet-shell.settings");
            Some(Target { comp, format: Format::Softcore, main, ours })
        }
        Compositor::Sway | Compositor::Other => None,
    }
}

fn hex(c: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

const HEADER: &str = "Rivet Shell's window look and displays. Written by Rivet Shell (Settings: Appearance, Displays): changes here are replaced.";

/// The overrides file's text (`look` None: turned off, a comment only).
pub fn render(format: Format, look: Option<&Look>) -> String {
    let comment = match format {
        Format::HyprLua => "--",
        Format::Kdl => "//",
        _ => "#",
    };
    let mut t = format!("{comment} {HEADER}\n");
    let Some(l) = look else {
        t.push_str(&format!("{comment} The window look is turned off.\n"));
        return t;
    };
    let op = format!("{:.2}", l.inactive_opacity);
    match format {
        Format::Kdl => {
            t.push_str("\nlayout {\n");
            t.push_str(&format!("    gaps {}\n", l.gaps_in));
            // niri's gaps are the same at the edges; struts add to them there.
            let s = l.gaps_out.saturating_sub(l.gaps_in);
            t.push_str(&format!("    struts {{\n        left {s}\n        right {s}\n        top {s}\n        bottom {s}\n    }}\n"));
            // The border is the focus indicator: the focus ring stays off so there aren't two.
            t.push_str("    focus-ring {\n        off\n    }\n");
            if l.border == 0 {
                t.push_str("    border {\n        off\n    }\n");
            } else {
                t.push_str(&format!("    border {{\n        on\n        width {}\n", l.border));
                if let Some((a, i)) = l.colors {
                    t.push_str(&format!("        active-color \"#{}\"\n        inactive-color \"#{}\"\n", hex(a), hex(i)));
                }
                t.push_str("    }\n");
            }
            t.push_str(&format!("    shadow {{\n        {}\n    }}\n", if l.shadows { "on" } else { "off" }));
            t.push_str("}\n");
            t.push_str(&format!("\nwindow-rule {{\n    geometry-corner-radius {}\n    clip-to-geometry true\n}}\n", l.radius));
            if l.inactive_opacity < 1.0 {
                t.push_str(&format!("\nwindow-rule {{\n    match is-active=false\n    opacity {op}\n}}\n"));
            }
        }
        Format::HyprConf => {
            t.push_str(&format!("\ngeneral {{\n    gaps_in = {}\n    gaps_out = {}\n    border_size = {}\n", l.gaps_in, l.gaps_out, l.border));
            if let Some((a, i)) = l.colors {
                t.push_str(&format!("    col.active_border = rgb({})\n    col.inactive_border = rgb({})\n", hex(a), hex(i)));
            }
            t.push_str("}\n");
            t.push_str(&format!("\ndecoration {{\n    rounding = {}\n    inactive_opacity = {op}\n    shadow {{\n        enabled = {}\n    }}\n}}\n", l.radius, l.shadows));
        }
        Format::HyprLua => {
            t.push_str(&format!("\nhl.config({{\n    general = {{\n        gaps_in = {},\n        gaps_out = {},\n        border_size = {},\n", l.gaps_in, l.gaps_out, l.border));
            if let Some((a, i)) = l.colors {
                t.push_str(&format!("        col = {{\n            active_border = \"rgb({})\",\n            inactive_border = \"rgb({})\",\n        }},\n", hex(a), hex(i)));
            }
            t.push_str("    },\n");
            t.push_str(&format!("    decoration = {{\n        rounding = {},\n        inactive_opacity = {op},\n        shadow = {{\n            enabled = {},\n        }},\n    }},\n}})\n", l.radius, l.shadows));
        }
        Format::Softcore => {
            t.push_str(&format!("\ngaps={}\ngaps_out={}\nborder={}\nradius={}\ninactive_opacity={op}\nshadows={}\n", l.gaps_in, l.gaps_out, l.border, l.radius, l.shadows));
            // Left out, Softcore's borders follow the Rivet accent by themselves.
            if let Some((a, i)) = l.colors {
                t.push_str(&format!("border_active=#{}\nborder_inactive=#{}\n", hex(a), hex(i)));
            }
        }
        Format::MangoConf | Format::MangoToml => {
            // Sourced files are read by their own extension, so the key=value form serves both.
            t.push('\n');
            for k in ["gap_inner_horizontal", "gap_inner_vertical"] {
                t.push_str(&format!("{k}={}\n", l.gaps_in));
            }
            for k in ["gap_outer_horizontal", "gap_outer_vertical"] {
                t.push_str(&format!("{k}={}\n", l.gaps_out));
            }
            t.push_str(&format!("border_px={}\nborder_radius={}\nunfocused_opacity={op}\nshadows={}\n", l.border, l.radius, u8::from(l.shadows)));
            if let Some((a, i)) = l.colors {
                t.push_str(&format!("focus_color=0x{}ff\nborder_color=0x{}ff\n", hex(a), hex(i)));
            }
        }
    }
    t
}

/// The line that includes our file from the main config.
pub fn include_line(t: &Target) -> String {
    match t.format {
        Format::Kdl => format!("include \"{}\"", file_name(&t.ours)),
        Format::HyprConf => format!("source = {}", t.ours.display()),
        Format::HyprLua => "require(\"rivet-shell\")".into(),
        Format::MangoConf => format!("source={}", t.ours.display()),
        Format::MangoToml => format!("\"{}\"", t.ours.display()),
        Format::Softcore => format!("include={}", file_name(&t.ours)),
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Whether the main config includes ours (a line, not commented out, that names it).
pub fn included(t: &Target) -> bool {
    let Ok(text) = std::fs::read_to_string(&t.main) else { return false };
    let name = file_name(&t.ours);
    let stem = name.rsplit_once('.').map(|s| s.0).unwrap_or(&name).to_string();
    text.lines().map(str::trim).filter(|l| !l.starts_with("//") && !l.starts_with('#') && !l.starts_with("--")).any(|l| match t.format {
        Format::HyprLua => l.contains(&format!("\"{stem}\"")) || l.contains(&format!("'{stem}'")),
        // An include of it, not any line that happens to name the shell (spawn=rivet-shell).
        Format::Softcore => l.split_once('=').is_some_and(|(k, v)| k.trim() == "include" && v.trim().ends_with(&name)),
        _ => l.contains(&name),
    })
}

/// Add the include to the main config (a copy kept first), and write our file before it so
/// the include never points at nothing.
pub fn add_include(t: &Target, look: Option<&Look>, displays: &[Display]) -> Result<(), String> {
    write_ours(t, look, displays)?;
    if included(t) {
        return Ok(());
    }
    let text = match std::fs::read_to_string(&t.main) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("couldn't read {}: {e}", t.main.display())),
    };
    if !text.is_empty() {
        let backup = PathBuf::from(format!("{}.rivet-backup", t.main.display()));
        if !backup.exists() {
            std::fs::copy(&t.main, &backup).map_err(|e| format!("couldn't keep a copy as {}: {e}", backup.display()))?;
        }
    }
    let new = with_include(t, &text);
    if let Some(d) = t.main.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    write_atomic(&t.main, &new)?;
    reload(t.comp);
    Ok(())
}

/// The main config's text with our include added.
fn with_include(t: &Target, text: &str) -> String {
    let line = include_line(t);
    let mut out = text.to_string();
    if t.format == Format::MangoToml {
        return toml_source(text, &line);
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    let c = if t.format == Format::HyprLua {
        "--"
    } else if t.format == Format::Kdl {
        "//"
    } else {
        "#"
    };
    out.push_str(&format!("\n{c} Rivet Shell's window look and displays (Settings). Last, so it wins.\n{line}\n"));
    out
}

/// config.toml: our path added last to [global]'s `source` list, or a new list.
fn toml_source(text: &str, item: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let global = lines.iter().position(|l| l.trim() == "[global]");
    // The [global] table runs to the next header.
    let (start, end) = match global {
        Some(g) => (g + 1, lines.iter().skip(g + 1).position(|l| l.trim_start().starts_with('[') && !l.contains('=')).map(|e| g + 1 + e).unwrap_or(lines.len())),
        None => (0, 0),
    };
    let src = (start..end).find(|&i| {
        let l = lines[i].trim_start();
        (l.starts_with("source ") || l.starts_with("source=")) && l.contains('[')
    });
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    match (global, src) {
        (_, Some(i)) => {
            // Close of the list: on that line or a later one.
            let close = (i..end).find(|&j| out[j].contains(']')).unwrap_or(i);
            let l = &out[close];
            let at = l.rfind(']').unwrap_or(l.len());
            let before = l[..at].trim_end();
            let empty = before.ends_with('[') || (close != i && before.is_empty());
            let sep = if empty || before.ends_with(',') { "" } else { ", " };
            out[close] = format!("{before}{sep}{item}{}", &l[at..]);
        }
        (Some(g), None) => out.insert(g + 1, format!("source = [{item}]")),
        (None, None) => {
            out.insert(0, String::new());
            out.insert(0, format!("source = [{item}]"));
            out.insert(0, "[global]".into());
        }
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

fn write_atomic(p: &Path, text: &str) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.rivet-tmp", p.display()));
    std::fs::write(&tmp, text).map_err(|e| format!("couldn't write {}: {e}", p.display()))?;
    std::fs::rename(&tmp, p).map_err(|e| format!("couldn't write {}: {e}", p.display()))
}

/// Write our file when it would change; true when it did.
fn write_ours(t: &Target, look: Option<&Look>, displays: &[Display]) -> Result<bool, String> {
    let text = format!("{}{}", render(t.format, look), crate::displays::render(t.format, displays));
    if std::fs::read_to_string(&t.ours).is_ok_and(|old| old == text) {
        return Ok(false);
    }
    if let Some(d) = t.ours.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    write_atomic(&t.ours, &text)?;
    Ok(true)
}

/// Bring the overrides file up to date with the settings, and have the compositor read it
/// again when it changed and is included. Nothing happens for other compositors, or when the
/// shell never wrote the file and the look is off.
pub fn apply(comp: Compositor, s: &Settings, accent: [u8; 3], dark: bool) -> Result<(), String> {
    let Some(t) = target(comp) else { return Ok(()) };
    if !s.comp_on && s.displays.is_empty() && !t.ours.exists() {
        return Ok(());
    }
    let look = s.comp_on.then(|| Look::from(s, accent, dark));
    if write_ours(&t, look.as_ref(), &s.displays)? && included(&t) {
        reload(comp);
    }
    Ok(())
}

/// apply() with the shell's look now: its accent (the desktop's colours when followed) and
/// whether it's dark.
pub fn apply_now(comp: Compositor, s: &Settings) -> Result<(), String> {
    let (accent, dark) = shell_look(s);
    apply(comp, s, accent, dark)
}

pub fn shell_look(s: &Settings) -> ([u8; 3], bool) {
    let live = rivet_look::live::LiveLook::new();
    let a = live.accent(&s.choice());
    let pal = rivet_look::palette::shown(live.palette(), s.follow_desktop_colors);
    let dark = pal.map(|p| p.is_dark()).unwrap_or(s.theme != rivet_look::theme::Mode::Light);
    ([a.r(), a.g(), a.b()], dark)
}

/// Have the compositor read its config again (niri does by itself).
pub fn reload(comp: Compositor) {
    match comp {
        Compositor::Hyprland => crate::sys::spawn("hyprctl", &["reload"]),
        Compositor::Mango => crate::sys::spawn("mmsg", &["dispatch", "reload_config"]),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> Look {
        Look { gaps_in: 8, gaps_out: 12, border: 2, radius: 10, colors: Some(([0xe6, 0x61, 0x00], [0x45, 0x45, 0x4a])), inactive_opacity: 0.9, shadows: true }
    }

    #[test]
    fn renders_each_format() {
        let k = render(Format::Kdl, Some(&look()));
        assert!(k.contains("gaps 8") && k.contains("left 4") && k.contains("active-color \"#e66100\"") && k.contains("geometry-corner-radius 10") && k.contains("opacity 0.90"));
        let h = render(Format::HyprConf, Some(&look()));
        assert!(h.contains("gaps_out = 12") && h.contains("col.active_border = rgb(e66100)") && h.contains("rounding = 10"));
        let l = render(Format::HyprLua, Some(&look()));
        assert!(l.starts_with("--") && l.contains("hl.config({") && l.contains("active_border = \"rgb(e66100)\""));
        let sc = render(Format::Softcore, Some(&look()));
        assert!(sc.starts_with('#') && sc.contains("gaps=8\ngaps_out=12\nborder=2\nradius=10\ninactive_opacity=0.90\nshadows=true") && sc.contains("border_active=#e66100"));
        let m = render(Format::MangoConf, Some(&look()));
        assert!(m.contains("gap_outer_vertical=12") && m.contains("border_px=2") && m.contains("focus_color=0xe66100ff") && m.contains("shadows=1"));
        // Off: a comment only.
        assert!(render(Format::Kdl, None).lines().all(|l| l.starts_with("//")));
        // No border in niri: off, not a zero width.
        assert!(render(Format::Kdl, Some(&Look { border: 0, ..look() })).contains("border {\n        off"));
    }

    #[test]
    fn adds_to_toml_source() {
        let item = "\"/h/.config/mango/rivet-shell.conf\"";
        let a = toml_source("[global]\nblur = 0\n\n[bind.default]\n\"SUPER+r\" = \"reload_config\"\n", item);
        assert!(a.starts_with("[global]\nsource = [\"/h/.config/mango/rivet-shell.conf\"]\nblur = 0"));
        let b = toml_source("[global]\nsource = [\"./env.toml\", \"./bind.toml\"]\n", item);
        assert!(b.contains("source = [\"./env.toml\", \"./bind.toml\", \"/h/.config/mango/rivet-shell.conf\"]"));
        let c = toml_source("[global]\nsource = [\n  \"./env.toml\",\n]\n", item);
        assert!(c.contains("  \"./env.toml\",\n\"/h/.config/mango/rivet-shell.conf\"]"));
        let d = toml_source("blur = 0\n", item);
        assert!(d.starts_with("[global]\nsource = ["));
    }

    #[test]
    fn softcore_is_included_by_its_own_line() {
        let dir = std::env::temp_dir().join(format!("rivet-comp-softcore-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let t = Target { comp: Compositor::Softcore, format: Format::Softcore, main: dir.join("settings"), ours: dir.join("rivet-shell.settings") };
        // A line that only names the shell is not the include.
        std::fs::write(&t.main, "gaps=4\nspawn=rivet-shell\n").unwrap();
        assert!(!included(&t));
        add_include(&t, Some(&look()), &[]).unwrap();
        add_include(&t, Some(&look()), &[]).unwrap();
        let text = std::fs::read_to_string(&t.main).unwrap();
        assert_eq!(text.matches("include=rivet-shell.settings").count(), 1);
        assert!(text.ends_with("include=rivet-shell.settings\n") && included(&t) && std::fs::read_to_string(&t.ours).unwrap().contains("gaps=8"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With Softcore running: turn its first display and scale it, through the files, as
    /// the Displays page does. `cargo test -p rivet-shell softcore_live -- --ignored`
    #[test]
    #[ignore]
    fn softcore_live() {
        let t = target(Compositor::Softcore).unwrap();
        let mut displays: Vec<Display> = crate::displays::read(Compositor::Softcore).unwrap().into_iter().map(|o| o.now).collect();
        displays[0].scale = 1.25;
        displays[0].rotate = 90;
        add_include(&t, Some(&look()), &displays).unwrap();
        println!("{}\n{}", std::fs::read_to_string(&t.main).unwrap(), std::fs::read_to_string(&t.ours).unwrap());
    }

    #[test]
    fn includes_once_and_finds_it() {
        let dir = std::env::temp_dir().join(format!("rivet-comp-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let t = Target { comp: Compositor::Niri, format: Format::Kdl, main: dir.join("config.kdl"), ours: dir.join("rivet-shell.kdl") };
        std::fs::write(&t.main, "layout {\n    gaps 4\n}\n").unwrap();
        assert!(!included(&t));
        add_include(&t, Some(&look()), &[]).unwrap();
        add_include(&t, Some(&look()), &[]).unwrap();
        let text = std::fs::read_to_string(&t.main).unwrap();
        assert_eq!(text.matches("include \"rivet-shell.kdl\"").count(), 1);
        assert!(included(&t) && t.ours.is_file() && dir.join("config.kdl.rivet-backup").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

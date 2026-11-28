//! `rivet-greeter sync`: leave this person's wallpaper and colors for the login screen from a
//! desktop without Rivet Shell (which does it by itself whenever they change). The colors are
//! the desktop's, as every Rivet app reads them (rivet_look::palette): the accent, secondary
//! and tertiary, and dark or light from the window color. Run it again after changing the
//! wallpaper, or from the wallpaper program's own hook.

use std::path::PathBuf;
use std::process::ExitCode;

use rivet_lockscreen::Look;
use rivet_look::palette::Palette;
use rivet_look::theme::Mode;

/// The look to leave: the desktop's colors over the defaults, and the clock's format.
fn look(palette: Option<&Palette>, clock: Option<&str>) -> Look {
    let rgb = |c: eframe::egui::Color32| [c.r(), c.g(), c.b()];
    let mut l = Look::default();
    if let Some(p) = palette {
        l.theme = if p.is_dark() { Mode::Dark } else { Mode::Light };
        if let Some(a) = p.accent {
            l.accent = rgb(a);
        }
        l.secondary = p.secondary.map(rgb);
        l.tertiary = p.tertiary.map(rgb);
    }
    if let Some(c) = clock.filter(|c| !c.trim().is_empty()) {
        l.clock_format = c.to_string();
    }
    l
}

/// A path as typed: `~/` at the start is the home folder.
fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(rest),
        None => PathBuf::from(p),
    }
}

pub fn run(args: &[String]) -> ExitCode {
    let mut wallpaper = None;
    let mut name = None;
    let mut clock = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let slot = match a.as_str() {
            "--wallpaper" => &mut wallpaper,
            "--name" => &mut name,
            "--clock" => &mut clock,
            other => {
                eprintln!("rivet-greeter sync doesn't know {other:?} (rivet-greeter --help)");
                return ExitCode::FAILURE;
            }
        };
        let Some(v) = it.next() else {
            eprintln!("{a} needs a value (rivet-greeter --help)");
            return ExitCode::FAILURE;
        };
        *slot = Some(v.clone());
    }
    let wallpaper = wallpaper.map(|w| expand(&w));
    if let Some(w) = wallpaper.as_ref().filter(|w| !w.is_file()) {
        eprintln!("There's no picture at {}. Give the path of the wallpaper's picture file.", w.display());
        return ExitCode::FAILURE;
    }
    let (palette, _) = rivet_look::palette::load();
    let look = look(palette.as_ref(), clock.as_deref());
    let name = name.unwrap_or_else(rivet_lockscreen::handoff::full_name);
    match rivet_lockscreen::handoff::publish(&look, &name, wallpaper.as_deref()) {
        Ok(()) => {
            let at = rivet_lockscreen::handoff::dir().join(rivet_lockscreen::login_name());
            let what = if wallpaper.is_some() { "wallpaper and colors" } else { "colors" };
            println!("The login screen now has your {what} (left in {}).", at.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Color32;

    #[test]
    fn takes_the_desktops_colors() {
        let p = Palette { accent: Some(Color32::from_rgb(0x35, 0x84, 0xe4)), window_bg: Some(Color32::from_rgb(250, 250, 250)), tertiary: Some(Color32::from_rgb(1, 2, 3)), ..Palette::default() };
        let l = look(Some(&p), Some("%-I:%M %p"));
        assert_eq!((l.theme, l.accent, l.secondary, l.tertiary, l.clock_format.as_str()), (Mode::Light, [0x35, 0x84, 0xe4], None, Some([1, 2, 3]), "%-I:%M %p"));
        assert_eq!(look(None, None), Look::default());
    }

    #[test]
    fn home_is_expanded() {
        assert!(!expand("~/Pictures/a.png").starts_with("~"));
        assert_eq!(expand("/a.png"), PathBuf::from("/a.png"));
    }
}

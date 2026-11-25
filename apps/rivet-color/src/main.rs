//! Rivet Color: pick a color from anywhere on the screen, copy it in the format you need, see
//! its shades and the colors that go with it, and keep colors in palettes. The window is
//! `gui`; `rivet-color pick` picks and copies with no window, for a key binding; and
//! `rivet-color shell-plugin` is Rivet Shell's Color plugin.

mod color;
mod gui;
mod overlay;
mod pick;
mod shell;
mod store;

use std::path::PathBuf;
use std::process::ExitCode;

use color::{Format, Rgb, parse};

const HELP: &str = "Rivet Color: pick colors from the screen and keep them in palettes

Usage:
  rivet-color                   Open the window, on the color picked last
  rivet-color COLOR             Open the window on a color (#d53f8c, rgb(213, 63, 140)…)
  rivet-color pick [--format hex|rgb|hsl|oklch]
                                Pick a color from the screen with no window: copy it,
                                keep it in the history and print it
  rivet-color convert COLOR [--format …]
                                Print a color in another format
  rivet-color shell-plugin      Rivet Shell's Color plugin (the shell starts it)
  rivet-color --install-desktop-entry
                                Add Rivet Color to the app launcher (for this user)
  rivet-color -h, --help        This help
  rivet-color -V, --version     The version

Copies are written the way chosen in the window (Copy as), unless --format says otherwise.
Picking uses niri's own picker on niri, hyprpicker where it's installed, grim and slurp on
other compositors that let programs read the screen, and the desktop's picker on GNOME
and KDE.";

/// The launcher entry and icon, in the user's data folder, running this program.
fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-color.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-color.svg");
    let text = include_str!("../data/rivet-color.desktop").replace("Exec=rivet-color", &format!("Exec=\"{}\"", exe.display()));
    for (p, body) in [(&entry, text.as_str()), (&icon, gui::ICON_SVG)] {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(p, body).map_err(|e| format!("couldn't write {}: {e}", p.display()))?;
    }
    let _ = std::process::Command::new("update-desktop-database").arg(data.join("applications")).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    Ok(entry)
}

fn fail(e: impl std::fmt::Display) -> ExitCode {
    eprintln!("rivet-color: {e}");
    ExitCode::FAILURE
}

/// Pick with no window: copy, remember and print.
fn pick_now(format: Format) -> ExitCode {
    match pick::pick() {
        Ok(Some(c)) => {
            let text = c.format(format);
            store::remember(c);
            let copied = pick::copy(&text);
            if !copied {
                eprintln!("rivet-color: couldn't copy it (install wl-clipboard)");
            }
            // A key binding has no window to show the answer in: a short notice says what was
            // copied, with a square of the color as its icon.
            let summary = if copied { format!("Copied {text}") } else { format!("Picked {text}") };
            let body = if copied { "" } else { "Install wl-clipboard to copy it too." };
            let _ = rivet_xdg::notify::notify("Rivet Color", &shell::swatch(c), &summary, body, 2500, true);
            println!("{text}");
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::from(2),
        Err(e) => {
            let _ = rivet_xdg::notify::notify("Rivet Color", "rivet-color", "Couldn't pick a color", &e, -1, false);
            fail(e)
        }
    }
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut screenshot = None;
    if let Some(i) = args.iter().position(|a| a == "--screenshot") {
        args.remove(i);
        if i < args.len() {
            screenshot = Some(PathBuf::from(args.remove(i)));
        }
    }
    let mut format = None;
    if let Some(i) = args.iter().position(|a| a == "--format" || a.starts_with("--format=")) {
        let word = match args.remove(i).split_once('=') {
            Some((_, w)) => w.to_string(),
            None if i < args.len() => args.remove(i),
            None => String::new(),
        };
        match Format::from_id(&word) {
            Some(f) => format = Some(f),
            None => return fail("--format takes hex, rgb, hsl or oklch"),
        }
    }
    let format = || format.unwrap_or_else(store::load_format);
    let mut start: Option<Rgb> = None;
    match args.first().map(String::as_str) {
        Some("-h" | "--help" | "help") => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version" | "version") => {
            println!("rivet-color {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("--install-desktop-entry") => {
            return match install_desktop_entry() {
                Ok(p) => {
                    println!("Added {}", p.display());
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            };
        }
        Some("pick") => return pick_now(format()),
        Some("shell-plugin") => return shell::run(),
        Some("convert") => {
            let text = args[1..].join(" ");
            return match parse(&text) {
                Some(c) => {
                    println!("{}", c.format(format()));
                    ExitCode::SUCCESS
                }
                None => fail(format!("{text:?} isn't a color (try #d53f8c, rgb(213, 63, 140) or hsl(329, 64%, 54%))")),
            };
        }
        Some(other) => match parse(&args.join(" ")) {
            Some(c) => start = Some(c),
            None => return fail(format!("unknown option or color {other:?} (see --help)")),
        },
        None => {}
    }
    // Test hook: RIVET_COLOR_SCENE=loupe draws the picker's loupe instead of the window
    // (RIVET_COLOR_SCALE sets the screen's scale, 1 by default).
    if let Some(path) = screenshot.as_ref().filter(|_| std::env::var("RIVET_COLOR_SCENE").as_deref() == Ok("loupe")) {
        let scale = std::env::var("RIVET_COLOR_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
        return overlay::shot(path, scale).map(|_| ExitCode::SUCCESS).unwrap_or_else(fail);
    }
    if screenshot.is_some() {
        store::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
        let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        });
        rivet_kit::headless::run(size.unwrap_or([960.0, 680.0]), move |cc| Box::new(gui::ColorApp::new(cc, start, screenshot)));
        return ExitCode::SUCCESS;
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet Color").with_app_id("rivet-color").with_inner_size([960.0, 680.0]).with_min_inner_size([680.0, 460.0]);
    if let Some(img) = rivet_look::icons::render_svg(gui::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width: 128, height: 128 });
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    match eframe::run_native("Rivet Color", options, Box::new(move |cc| Ok(Box::new(gui::ColorApp::new(cc, start, None))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(format!("couldn't open the window: {e}")),
    }
}

//! Rivet Shell: a panel, dock, desktop widgets and wallpaper for Wayland compositors, with
//! settings, in the Rivet apps' look.

mod ai;
mod ai_pop;
mod audio;
mod auth;
mod bar;
mod cc;
mod comp;
mod displays;
mod config;
mod conn;
mod dock_ui;
mod glyph;
mod guard;
mod idle;
mod ipc;
mod launcher;
mod lock;
mod lock_ui;
mod notes;
mod notes_pop;
mod panels;
mod plugins;
mod portal;
mod power_ui;
mod quick;
mod radio;
mod settings_app;
mod shot;
mod sound_pop;
mod style;
mod sys;
mod tray;
mod tray_ui;
mod ui;
mod wall;
mod wall_pick;
mod wl;

use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "Rivet Shell: a panel, dock, desktop widgets and wallpaper for Wayland

Usage:
  rivet-shell                       Start the shell in this Wayland session
  rivet-shell settings              The settings window
  rivet-shell colors PICTURE        Make the desktop's colors from a picture with matugen
  rivet-shell palette [NAME|none]   Use a built-in palette for the colors instead (no name:
                                    list them)
  rivet-shell ai                    AI plan usage, as the panel's AI item reads it
  rivet-shell lock                  Lock the screen (the shell does this itself when idle
                                    and before sleep); --preview shows it without locking
  rivet-shell msg COMMAND           Tell the running shell: launcher, control-center, calendar,
                                    power, network, bluetooth, ai, sound (each opens or closes it); volume up|down|mute|N,
                                    mic mute, brightness up|down|N, media play-pause|next|previous,
                                    dnd, night-light, keep-awake (each turns it on or off);
                                    lock, unlock, settings, wallpaper PATH|random|next|previous,
                                    reload, quit
  rivet-shell --install-desktop-entry
                                    Add Rivet Shell Settings to the app launcher (for this user)
  rivet-shell -h, --help            This help
  rivet-shell -V, --version         The version

Start it from your compositor (niri: spawn-at-startup \"rivet-shell\"; Hyprland: exec-once =
rivet-shell; Sway: exec rivet-shell) and bind a key to `rivet-shell msg launcher`. It needs the
layer-shell protocol (niri, Hyprland, Sway, river, labwc, Wayfire, COSMIC; not GNOME), and
matugen for colors from the wallpaper.";

fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-shell-settings.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-shell.svg");
    let text = include_str!("../data/rivet-shell-settings.desktop").replace("Exec=rivet-shell", &format!("Exec=\"{}\"", exe.display()));
    for (p, body) in [(&entry, text.as_str()), (&icon, settings_app::ICON_SVG)] {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(p, body).map_err(|e| format!("couldn't write {}: {e}", p.display()))?;
    }
    let _ = std::process::Command::new("update-desktop-database").arg(data.join("applications")).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    Ok(entry)
}

fn settings_window(screenshot: Option<PathBuf>) -> ExitCode {
    if screenshot.is_some() {
        rivet_kit::headless::run([960.0, 680.0], move |cc| Box::new(settings_app::SettingsApp::new(cc, screenshot)));
        return ExitCode::SUCCESS;
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet Shell Settings").with_app_id("rivet-shell-settings").with_inner_size([960.0, 680.0]).with_min_inner_size([640.0, 420.0]);
    if let Some(img) = rivet_look::icons::render_svg(settings_app::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width: 128, height: 128 });
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    match eframe::run_native("Rivet Shell Settings", options, Box::new(|cc| Ok(Box::new(settings_app::SettingsApp::new(cc, None))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-shell: couldn't open the settings window: {e}");
            ExitCode::FAILURE
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
    match args.first().map(String::as_str) {
        Some("-h" | "--help" | "help") => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
        Some("-V" | "--version" | "version") => {
            println!("rivet-shell {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--install-desktop-entry") => match install_desktop_entry() {
            Ok(p) => {
                println!("Added {}", p.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("rivet-shell: {e}");
                ExitCode::FAILURE
            }
        },
        Some("settings") => settings_window(screenshot),
        Some("lock") => match screenshot {
            Some(p) => lock::screenshot(p),
            None => lock::run(args.iter().any(|a| a == "--preview")),
        },
        Some("ai") => {
            print!("{}", ai::report());
            ExitCode::SUCCESS
        }
        Some("colors") => {
            // matugen on a picture, as the shell runs it, with the result shown.
            let Some(pic) = args.get(1).map(PathBuf::from) else {
                eprintln!("rivet-shell: colors PICTURE");
                return ExitCode::FAILURE;
            };
            let s = config::Settings::load();
            let (tx, rx) = std::sync::mpsc::channel();
            wall::run_matugen(&pic, s.color_mode, &s.scheme_type, s.matugen_contrast, move |r| {
                let _ = tx.send(r);
            });
            match rx.recv() {
                Ok(Ok(())) => {
                    println!("Colors made from {}", pic.display());
                    ExitCode::SUCCESS
                }
                Ok(Err(e)) => {
                    eprintln!("rivet-shell: matugen: {e}");
                    ExitCode::FAILURE
                }
                Err(_) => ExitCode::FAILURE,
            }
        }
        // The template matugen fills to make Rivet's colors (for scripts that make them
        // somewhere other than the person's own folders).
        Some("matugen-template") => {
            print!("{}", rivet_look::palette::MATUGEN_TEMPLATE);
            ExitCode::SUCCESS
        }
        Some("palette") => {
            use rivet_look::schemes::{SCHEMES, Scheme};
            let name = args[1..].join(" ");
            if name.is_empty() {
                for s in &SCHEMES {
                    println!("{}", s.name);
                }
                return ExitCode::SUCCESS;
            }
            let mut s = config::Settings::load();
            if name.eq_ignore_ascii_case("none") {
                s.choose_palette("");
            } else if Scheme::by_name(&name).is_some() {
                s.choose_palette(&name);
            } else {
                eprintln!("rivet-shell: no palette named \"{name}\" (rivet-shell palette lists them)");
                return ExitCode::FAILURE;
            }
            s.save();
            let _ = ipc::send("changed");
            ExitCode::SUCCESS
        }
        Some("msg") => {
            let line = args[1..].join(" ");
            if line.is_empty() {
                eprintln!("rivet-shell: msg what? (launcher, settings, wallpaper PATH, reload, quit)");
                return ExitCode::FAILURE;
            }
            match ipc::send(&line) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("rivet-shell: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(other) => {
            eprintln!("rivet-shell: unknown command {other} (see --help)");
            ExitCode::FAILURE
        }
        None if screenshot.is_some() => {
            let path = screenshot.unwrap_or_default();
            // RIVET_SHOT_SIZE=WxH for documentation pictures (1100x700 by default).
            let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
                let (w, h) = v.split_once('x')?;
                Some([w.parse().ok()?, h.parse().ok()?])
            });
            rivet_kit::headless::run(size.unwrap_or([1100.0, 700.0]), move |cc| Box::new(shot::ShotApp::new(cc, path)));
            ExitCode::SUCCESS
        }
        None => {
            if ipc::running() {
                eprintln!("rivet-shell: already running in this session (rivet-shell msg quit stops it)");
                return ExitCode::FAILURE;
            }
            // The watcher: it starts the shell proper, and again if that one crashes
            // (RIVET_SHELL_NO_WATCH=1 runs the shell directly, for a debugger).
            if !guard::is_child() && std::env::var_os("RIVET_SHELL_NO_WATCH").is_none() {
                return guard::watch();
            }
            guard::log_panics();
            // A check of the watcher: RIVET_SHELL_CRASH=1 stops the shell as a bug would.
            if std::env::var_os("RIVET_SHELL_CRASH").is_some() {
                panic!("asked to crash (RIVET_SHELL_CRASH)");
            }
            let (tx, rx) = calloop::channel::channel::<wl::Msg>();
            let itx = tx.clone();
            if let Err(e) = ipc::listen(move |line| {
                let _ = itx.send(wl::Msg::Ipc(line));
            }) {
                eprintln!("rivet-shell: {e}");
            }
            let r = wl::run(config::Settings::load(), rx, tx);
            ipc::cleanup();
            match r {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("rivet-shell: {e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

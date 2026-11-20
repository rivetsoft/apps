//! Rivet Shell: a panel, dock, desktop widgets and wallpaper for Wayland compositors, with
//! settings, in the Rivet apps' look.

mod ai;
mod ai_pop;
mod audio;
mod auth_pop;
mod auth;
mod bar;
mod cc;
mod comp;
mod displays;
mod config;
mod conn;
mod dock_ui;
mod glyph;
mod greeter;
mod guard;
mod idle;
mod ipc;
mod launcher;
mod lock;
mod lock_ui;
mod media_pop;
mod notes;
mod notes_pop;
mod panels;
mod plugin_store;
mod plugins;
mod polkit;
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
mod viz;
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
  rivet-shell plugin list|available|install NAME|remove NAME
                                    Plugins from other people: the ones installed, the ones
                                    shared, and adding or removing one. install also takes a
                                    .tar.gz, a plugin's folder or an address
  rivet-shell ai                    AI plan usage, as the panel's AI item reads it
  rivet-shell lock                  Lock the screen (the shell does this itself when idle
                                    and before sleep); --preview shows it without locking
  rivet-shell greeter               The lock screen as greetd's login screen, over the
                                    wallpaper chosen last; --setup (with sudo) prepares it
  rivet-shell msg COMMAND           Tell the running shell: launcher, control-center, calendar,
                                    power, network, bluetooth, ai, sound, media (each opens or closes it); volume up|down|mute|N,
                                    mic mute, brightness up|down|N, media play-pause|next|previous,
                                    dnd, night-light, keep-awake (each turns it on or off);
                                    subpanel NAME (opens or closes that sub panel; add open,
                                    close or stay); lock, unlock, settings,
                                    wallpaper PATH|random|next|previous, reload, quit
  rivet-shell --install-desktop-entry
                                    Add Rivet Shell Settings to the app launcher (for this user)
  rivet-shell --install-service     Start the shell with your session as a systemd service
                                    (for this user); --remove-service takes it out again
  rivet-shell -h, --help            This help
  rivet-shell -V, --version         The version

Start it from your compositor (niri: spawn-at-startup \"rivet-shell\"; Hyprland: exec-once =
rivet-shell; Sway: exec rivet-shell), or as a service with --install-service where the
compositor starts systemd's graphical session (niri-session, uwsm). Bind a key to
`rivet-shell msg launcher`. It needs the
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

const SERVICE: &str = "rivet-shell.service";

fn systemctl(args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new("systemctl").arg("--user").args(args).output().map_err(|_| "systemctl isn't installed, so there's no service to set up here".to_string())?;
    if out.status.success() { Ok(()) } else { Err(format!("systemctl --user {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim())) }
}

/// Write the service for this user (with this program's own path) and turn it on, so the
/// shell starts with the graphical session.
fn install_service() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let unit = rivet_xdg::places::config_home().join("systemd/user").join(SERVICE);
    // Quoted as systemd reads it, for a path with spaces.
    let quoted = format!("\"{}\"", exe.display().to_string().replace('\\', "\\\\").replace('"', "\\\""));
    let text = include_str!("../data/rivet-shell.service").replace("ExecStart=rivet-shell", &format!("ExecStart={quoted}"));
    if let Some(d) = unit.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&unit, text).map_err(|e| format!("couldn't write {}: {e}", unit.display()))?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", SERVICE])?;
    Ok(unit)
}

fn remove_service() -> Result<PathBuf, String> {
    let unit = rivet_xdg::places::config_home().join("systemd/user").join(SERVICE);
    if !unit.exists() {
        return Err("the service isn't installed for this user".into());
    }
    // Turned off, not stopped: a shell on screen stays until the session ends.
    let _ = systemctl(&["disable", SERVICE]);
    std::fs::remove_file(&unit).map_err(|e| format!("couldn't remove {}: {e}", unit.display()))?;
    systemctl(&["daemon-reload"])?;
    Ok(unit)
}

fn settings_window(screenshot: Option<PathBuf>) -> ExitCode {
    if screenshot.is_some() {
        // Checks: RIVET_SHELL_WIN=960x2400 sizes the window, to see a long page in one picture.
        let size = std::env::var("RIVET_SHELL_WIN").ok().and_then(|v| v.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?]))).unwrap_or([960.0, 680.0]);
        rivet_kit::headless::run(size, move |cc| Box::new(settings_app::SettingsApp::new(cc, screenshot)));
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

/// `rivet-shell plugin …`: the plugins installed and shared, and adding or removing one.
fn plugin_command(args: &[String]) -> ExitCode {
    let fail = |e: String| {
        eprintln!("rivet-shell: {e}");
        ExitCode::FAILURE
    };
    let on = config::Settings::load().plugins;
    match (args.first().map(String::as_str), args.get(1)) {
        (Some("list"), _) => {
            for m in plugins::found() {
                let from = if m.official { "official" } else { "third-party" };
                println!("{:<14} {:<20} {:<12} {}", m.id, m.name, from, if on.contains(&m.id) { "on" } else { "off" });
            }
            ExitCode::SUCCESS
        }
        (Some("available"), _) => match plugin_store::fetch() {
            Ok(list) => {
                let have = plugins::found();
                for l in &list {
                    let mark = if have.iter().any(|m| m.id == l.id) { "  (installed)" } else { "" };
                    println!("{:<14} {:<20} {:<8} {}{mark}", l.id, l.name, l.version, l.description);
                }
                if list.is_empty() {
                    println!("No plugins are shared yet.");
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        },
        (Some("install"), Some(what)) => {
            // A name from the list, else a file, a folder or an address.
            let local = what.contains('/') || what.contains('.') || std::path::Path::new(what).exists();
            let done = if local {
                plugin_store::install_from(what)
            } else {
                plugin_store::fetch().and_then(|list| match list.iter().find(|l| &l.id == what) {
                    Some(l) => plugin_store::install(l),
                    None => Err(format!("no plugin called {what} is shared (see rivet-shell plugin available)")),
                })
            };
            match done {
                Ok(m) => {
                    let _ = ipc::send("changed");
                    let next = if on.contains(&m.id) { "" } else { " Turn it on in Settings, Plugins." };
                    println!("Installed {} in {}.{next}", m.name, m.dir.display());
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        (Some("remove"), Some(id)) => match plugin_store::remove(id) {
            Ok(()) => {
                let _ = ipc::send("changed");
                println!("Removed {id}.");
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        },
        _ => fail("plugin what? (list, available, install NAME, remove NAME)".into()),
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
        Some("--install-service") => match install_service() {
            Ok(p) => {
                println!("Added {}\nRivet Shell now starts with your session. Take its line out of your compositor's settings, then log in again or run: systemctl --user start {SERVICE}", p.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("rivet-shell: {e}");
                ExitCode::FAILURE
            }
        },
        Some("--remove-service") => match remove_service() {
            Ok(p) => {
                println!("Removed {}\nStart Rivet Shell from your compositor's settings from now on.", p.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("rivet-shell: {e}");
                ExitCode::FAILURE
            }
        },
        Some("settings") => settings_window(screenshot),
        Some("greeter") => match (screenshot, args.iter().any(|a| a == "--setup")) {
            (Some(p), _) => lock::screenshot(p),
            (None, true) => greeter::setup(),
            (None, false) => lock::greet(),
        },
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
        Some("plugin") => plugin_command(&args[1..]),
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
                // Its own number, so the service doesn't keep trying.
                return ExitCode::from(3);
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

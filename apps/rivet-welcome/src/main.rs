//! Rivet Welcome: installs and updates the Rivet apps from their GitHub releases, and keeps a
//! tray icon that looks for new ones.

mod gui;
mod settings;
mod tray;

// The engine, shared with rivetctl.
use rivet_install::{catalog, install, local, releases, state};

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const HELP: &str = "Rivet Welcome: install and update the Rivet apps

Usage:
  rivet-welcome                     Open the window (and the tray, when it's on)
  rivet-welcome --tray              Only the tray icon: it looks for updates when it starts
                                    and every few hours, and opens the window when clicked
  rivet-welcome check               Look for updates and list them (exits 2 when there are some)
  rivet-welcome list                Every app, with what's installed and what's released
  rivet-welcome install APP...      Install or update apps by name (files, notes, edit, …)
  rivet-welcome update              Install every update
  rivet-welcome remove APP...       Remove apps it installed (their settings and files stay)
  rivet-welcome --install-desktop-entry
                                    Add Rivet Welcome to the app launcher (for this user)
  rivet-welcome -h, --help          This help
  rivet-welcome -V, --version       The version

Apps are installed for you alone (no password): programs in ~/.local/bin, and the apps in
your launcher, from the archives on https://github.com/rivetsoft/apps/releases.";

/// The window's name on the session bus: a second `rivet-welcome` asks it to come forward.
const WINDOW_NAME: &str = "io.github.ulinaaron.RivetWelcome";
const WINDOW_PATH: &str = "/io/github/ulinaaron/RivetWelcome";

struct Raise(
    Arc<AtomicBool>,
    Arc<std::sync::Mutex<Option<eframe::egui::Context>>>,
);

#[zbus::interface(name = "io.github.ulinaaron.RivetWelcome1")]
impl Raise {
    fn show(&self) {
        self.0.store(true, Ordering::Relaxed);
        if let Ok(c) = self.1.lock()
            && let Some(ctx) = c.as_ref()
        {
            ctx.request_repaint();
        }
    }
}

/// The launcher entry and icon, in the user's data folder, running this program.
fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = local::this_exe().ok_or("couldn't find this program")?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-welcome.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-welcome.svg");
    let text = include_str!("../data/rivet-welcome.desktop")
        .replace("Exec=rivet-welcome", &format!("Exec=\"{}\"", exe.display()));
    for (p, body) in [(&entry, text.as_str()), (&icon, gui::ICON_SVG)] {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(p, body).map_err(|e| format!("couldn't write {}: {e}", p.display()))?;
    }
    // The tray starts this copy at sign-in from now on.
    if settings::Settings::load().tray {
        settings::set_autostart(true)?;
    }
    let _ = std::process::Command::new("update-desktop-database")
        .arg(data.join("applications"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    Ok(entry)
}

/// The list, from GitHub (or the last one, when GitHub can't be reached).
fn releases_now() -> Vec<releases::Release> {
    match releases::check() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("rivet-welcome: {e}");
            releases::load_cached().map(|(l, _)| l).unwrap_or_default()
        }
    }
}

fn cli_list() -> ExitCode {
    let list = releases_now();
    for r in state::rows(&list, settings::Settings::load().channel) {
        let have = match &r.installed {
            Some(i) => {
                let v = i
                    .version
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "?".into());
                if i.ours {
                    v
                } else if i.running != local::Running::No {
                    format!("{v} (running from {})", i.path.display())
                } else {
                    format!("{v} ({}: {})", i.origin(), i.path.display())
                }
            }
            None => "-".into(),
        };
        let out = r
            .latest
            .as_ref()
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| "not released".into());
        let mark = if r.update().is_some() {
            "  ← update"
        } else {
            ""
        };
        println!(
            "{:<10} {:<16} installed {:<12} released {out}{mark}",
            r.entry.id, r.entry.title, have
        );
    }
    ExitCode::SUCCESS
}

fn cli_check() -> ExitCode {
    let list = match releases::check() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("rivet-welcome: {e}");
            return ExitCode::FAILURE;
        }
    };
    let rows = state::rows(&list, settings::Settings::load().channel);
    let ups = state::updates(&rows);
    if ups.is_empty() {
        println!("The Rivet apps are up to date.");
        return ExitCode::SUCCESS;
    }
    for r in ups {
        if let (Some(i), Some((_, v))) = (&r.installed, r.update()) {
            let have = i
                .version
                .as_ref()
                .map(|v| v.label())
                .unwrap_or_else(|| "?".into());
            println!("{}: {have} → {}", r.entry.title, v.label());
        }
    }
    ExitCode::from(2)
}

/// Install (or update) these apps, or every update when `names` is empty.
fn cli_install(names: &[String]) -> ExitCode {
    let list = releases_now();
    let rows = state::rows(&list, settings::Settings::load().channel);
    let todo: Vec<&state::Row> = if names.is_empty() {
        state::updates(&rows)
    } else {
        let mut out = Vec::new();
        for n in names {
            let n = n.to_lowercase();
            match rows.iter().find(|r| {
                r.entry.id == n
                    || r.entry.bin == n
                    || r.entry.dist == n
                    || r.entry.title.to_lowercase() == n
            }) {
                Some(r) => out.push(r),
                None => {
                    eprintln!("rivet-welcome: no app called {n} (see rivet-welcome list)");
                    return ExitCode::FAILURE;
                }
            }
        }
        out
    };
    if todo.is_empty() {
        println!("Nothing to update.");
        return ExitCode::SUCCESS;
    }
    let mut failed = false;
    for r in todo {
        if r.installed.as_ref().is_some_and(|i| !i.ours) && r.install_copy().is_none() {
            eprintln!(
                "{}: not updated here, {}",
                r.entry.title,
                r.installed
                    .as_ref()
                    .map(|i| i.left_alone())
                    .unwrap_or_default()
            );
            continue;
        }
        let Some((rel, v)) = r.update().or(r.latest.as_ref()) else {
            eprintln!("{}: not released yet", r.entry.title);
            failed = true;
            continue;
        };
        if r.install_copy().is_none()
            && r.installed.as_ref().and_then(|i| i.version.as_ref()) == Some(v)
        {
            println!("{} {} is already installed.", r.entry.title, v.label());
            continue;
        }
        println!("Installing {} {}…", r.entry.title, v.label());
        match install::run(&r.entry, rel, &|_| {}) {
            Ok(v) => println!("Installed {} {}.", r.entry.title, v.label()),
            Err(e) => {
                eprintln!("{}: {e}", r.entry.title);
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn cli_remove(names: &[String]) -> ExitCode {
    if names.is_empty() {
        eprintln!("rivet-welcome: remove which app? (see rivet-welcome list)");
        return ExitCode::FAILURE;
    }
    let mut failed = false;
    for n in names {
        let n = n.to_lowercase();
        let Some(e) =
            catalog::with_discovered(&releases::load_cached().map(|(l, _)| l).unwrap_or_default())
                .into_iter()
                .find(|e| e.id == n || e.bin == n || e.dist == n)
        else {
            eprintln!("rivet-welcome: no app called {n} (see rivet-welcome list)");
            failed = true;
            continue;
        };
        if !local::bin_home().join(&e.bin).exists() {
            eprintln!(
                "{}: not installed here (in {})",
                e.title,
                local::bin_home().display()
            );
            failed = true;
            continue;
        }
        match local::remove(&e) {
            Ok(()) => println!("Removed {}.", e.title),
            Err(err) => {
                eprintln!("{}: {err}", e.title);
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    rivet_install::set_host(rivet_install::Host {
        title: "Rivet Welcome",
        version: env!("CARGO_PKG_VERSION"),
        app: "welcome",
    });
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
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version" | "version") => {
            println!("rivet-welcome {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("--install-desktop-entry") => {
            return match install_desktop_entry() {
                Ok(p) => {
                    println!("Added {}", p.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("rivet-welcome: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("--tray" | "tray") => return tray::run(),
        Some("check") => return cli_check(),
        Some("list") => return cli_list(),
        Some("install") => {
            if args.len() < 2 {
                eprintln!("rivet-welcome: install which app? (see rivet-welcome list)");
                return ExitCode::FAILURE;
            }
            return cli_install(&args[1..]);
        }
        Some("update") => return cli_install(&[]),
        Some("remove") => return cli_remove(&args[1..]),
        Some(other) => {
            eprintln!("rivet-welcome: unknown option {other} (see --help)");
            return ExitCode::FAILURE;
        }
        None => {}
    }
    let raise = Arc::new(AtomicBool::new(false));
    if screenshot.is_some() {
        let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        });
        rivet_kit::headless::run(size.unwrap_or([1000.0, 720.0]), move |cc| {
            Box::new(gui::WelcomeApp::new(cc, screenshot, raise))
        });
        return ExitCode::SUCCESS;
    }
    // One window: a second start brings the first forward instead.
    let ctx_slot: Arc<std::sync::Mutex<Option<eframe::egui::Context>>> = Arc::default();
    let _bus = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.serve_at(WINDOW_PATH, Raise(raise.clone(), ctx_slot.clone())))
        .and_then(|b| b.build())
        .ok()
        .and_then(|conn| {
            use zbus::fdo::{RequestNameFlags, RequestNameReply};
            match conn.request_name_with_flags(WINDOW_NAME, RequestNameFlags::DoNotQueue.into()) {
                Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {
                    Some(Ok(conn))
                }
                // zbus answers a name someone else holds with NameTaken, not Exists.
                Ok(_) | Err(zbus::Error::NameTaken) => Some(Err(conn)),
                Err(_) => None,
            }
        });
    if let Some(Err(conn)) = &_bus {
        let _ = conn.call_method(
            Some(WINDOW_NAME),
            WINDOW_PATH,
            Some("io.github.ulinaaron.RivetWelcome1"),
            "Show",
            &(),
        );
        println!("Rivet Welcome is already open.");
        return ExitCode::SUCCESS;
    }
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Rivet Welcome")
        .with_app_id("rivet-welcome")
        .with_inner_size([1000.0, 720.0])
        .with_min_inner_size([520.0, 380.0]);
    if let Some(img) = rivet_look::icons::render_svg(gui::ICON_SVG.as_bytes(), 128) {
        let rgba = img
            .pixels
            .iter()
            .flat_map(|p| p.to_srgba_unmultiplied())
            .collect();
        viewport = viewport.with_icon(eframe::egui::IconData {
            rgba,
            width: 128,
            height: 128,
        });
    }
    let options = eframe::NativeOptions {
        viewport,
        persist_window: false,
        ..Default::default()
    };
    match eframe::run_native(
        "Rivet Welcome",
        options,
        Box::new(move |cc| {
            if let Ok(mut c) = ctx_slot.lock() {
                *c = Some(cc.egui_ctx.clone());
            }
            Ok(Box::new(gui::WelcomeApp::new(cc, None, raise)))
        }),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-welcome: couldn't open the window: {e}");
            ExitCode::FAILURE
        }
    }
}

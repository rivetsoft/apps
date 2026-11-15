//! Rivet Files — a classic Linux file manager (in the family of Nemo, Thunar and Caja),
//! written in Rust with egui and no GTK or Qt.
//!
//! Usage: `rivet --help`.

mod app;
mod actions;
mod archive;
mod cli;
mod crash;
mod bench;
mod builtin_icons;
mod fonts;
mod filters;
mod fsops;
mod headless;
mod highlight;
mod icons;
mod launch;
mod desktop;
mod media;
mod mime;
mod nerd;
mod picker;
mod portal;
mod portal_setup;
mod network;
mod model;
mod palette;
mod places;
mod rename;
mod preview;
mod settings;
mod thumbs;
mod undo;
mod watch;
mod wlclip;

use eframe::egui;
use std::path::PathBuf;

/// `--measure SECS`: run normally (window unfocused, nothing saved) and quit after SECS.
pub static MEASURE_SECS: std::sync::OnceLock<f64> = std::sync::OnceLock::new();

pub fn measuring() -> bool {
    MEASURE_SECS.get().is_some()
}

/// When the process started (for RIVET_FRAME_STATS).
pub static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn main() -> eframe::Result {
    STARTED.get_or_init(std::time::Instant::now);
    // A bug that stops Rivet leaves a report on this computer (shown on the next start).
    crash::install();
    let mut screenshot: Option<PathBuf> = None;
    let mut start: Option<PathBuf> = None;
    let mut decorations: Option<bool> = None;
    let mut view: Option<String> = None;
    let mut tree = false;
    let mut hide_menubar = false;
    let mut pick = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            // The desktop's file picker service (started by the session bus; see portal.rs).
            "--portal" => {
                if let Err(e) = portal::run() {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            // One picker window; the request comes on stdin (see picker.rs).
            "--pick" => pick = true,
            // Make Rivet the desktop's file picker, or stop (same as Preferences → System).
            "--file-picker" => {
                let r = match args.next().as_deref() {
                    Some("on") => portal_setup::enable().map(|_| "Rivet is now the file picker.".to_string()),
                    Some("off") => portal_setup::disable().map(|_| "Rivet is no longer the file picker.".to_string()),
                    Some("status") | None => Ok(match portal_setup::status() {
                        portal_setup::Status::On => "File picker: Rivet".into(),
                        portal_setup::Status::Off(Some(o)) => format!("File picker: {o}"),
                        portal_setup::Status::Off(None) => "File picker: another app".into(),
                        portal_setup::Status::Unavailable(why) => format!("File picker: unavailable ({why})"),
                    }),
                    Some(other) => Err(format!("Unknown choice “{other}”: use on, off or status")),
                };
                match r {
                    Ok(msg) => println!("{msg}"),
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                }
                return Ok(());
            }
            "--measure" => {
                let secs = args.next().and_then(|v| v.parse().ok()).unwrap_or(15.0);
                let _ = MEASURE_SECS.set(secs);
            }
            "--no-decorations" | "--csd" => decorations = Some(false),
            "--decorations" => decorations = Some(true),
            "--view" => view = args.next(),
            "--tree" => tree = true,
            "--hide-menubar" => hide_menubar = true,
            "--home" => start = Some(places::home()),
            "--export-icons" => {
                // Render the logo to PNGs in the freedesktop layout: DIR/hicolor/NxN/apps/rivet-files.png
                let dir = PathBuf::from(args.next().unwrap_or_else(|| "data/icons".into()));
                for px in [16u32, 24, 32, 48, 64, 128, 256, 512] {
                    let img = icons::render_svg(desktop::ICON_SVG.as_bytes(), px).expect("logo renders");
                    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                    let out = dir.join(format!("hicolor/{px}x{px}/apps/rivet-files.png"));
                    std::fs::create_dir_all(out.parent().unwrap()).expect("create icon folder");
                    image::save_buffer(&out, &rgba, px, px, image::ExtendedColorType::Rgba8).expect("write png");
                    println!("{}", out.display());
                }
                let svg = dir.join("hicolor/scalable/apps/rivet-files.svg");
                std::fs::create_dir_all(svg.parent().unwrap()).expect("create icon folder");
                std::fs::write(&svg, desktop::ICON_SVG).expect("write svg");
                println!("{}", svg.display());
                return Ok(());
            }
            "--bench" => {
                bench::run(args.by_ref().map(PathBuf::from).collect());
                return Ok(());
            }
            "--install-desktop-entry" => {
                match desktop::install() {
                    Ok(p) => println!("Installed {}", p.display()),
                    Err(e) => eprintln!("Could not install the desktop entry: {e}"),
                }
                return Ok(());
            }
            "-V" | "--version" => {
                println!("Rivet Files {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                print!("{}", cli::usage());
                return Ok(());
            }
            _ if a.starts_with("--screenshot=") => screenshot = Some(PathBuf::from(&a["--screenshot=".len()..])),
            _ => {
                let p = if let Some(s) = a.strip_prefix("file://") {
                    PathBuf::from(places::percent_decode(s))
                } else if a == "~" {
                    places::home()
                } else if let Some(r) = a.strip_prefix("~/") {
                    places::home().join(r)
                } else {
                    PathBuf::from(&a)
                };
                start = Some(p);
            }
        }
    }

    if screenshot.is_some() || measuring() {
        filters::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
        thumbs::NO_WRITE.store(true, std::sync::atomic::Ordering::Relaxed);
        crash::NO_WRITE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // Checks use the default look so they are comparable from run to run.
    // Test hook for checks: a picker started by the portal service runs in screenshot mode.
    if pick && screenshot.is_none() {
        screenshot = std::env::var_os("RIVET_PICK_SCREENSHOT").map(PathBuf::from);
    }
    // Test hook: RIVET_LOAD_SETTINGS=1 makes a check use the saved settings (point
    // XDG_CONFIG_HOME at a copy; checks never save).
    let checking = (screenshot.is_some() || measuring()) && std::env::var_os("RIVET_LOAD_SETTINGS").is_none();
    let mut settings = if checking { settings::Settings::default() } else { settings::Settings::load() };
    // Screenshots for checks show a neutral sample folder, never the person's own files,
    // unless a folder was named on purpose.
    let mut demo: Option<PathBuf> = None;
    if screenshot.is_some() && start.is_none() && !pick && std::env::var_os("RIVET_REAL_HOME").is_none() {
        demo = places::make_demo_dir();
        start = demo.clone();
    }
    if let Some(d) = decorations {
        settings.decorations = d;
    }
    // Test hook for checks: try a toolbar layout without touching saved settings.
    if let Ok(t) = std::env::var("RIVET_TOOLBAR") {
        settings.toolbar = settings::parse_toolbar(&t);
    }
    if std::env::var("RIVET_ICONS").as_deref() == Ok("builtin") {
        settings.builtin_icons = true;
    }
    if let Some(v) = std::env::var("RIVET_SIDEBAR_ICON").ok().and_then(|v| v.parse::<f32>().ok()) {
        settings.sidebar_icon = v;
    }
    if let Ok(f) = std::env::var("RIVET_FONT") {
        settings.ui_font = f;
    }
    if let Some(sz) = std::env::var("RIVET_FONT_SIZE").ok().and_then(|v| v.parse().ok()) {
        settings.ui_font_size = sz;
    }
    if let Ok(c) = std::env::var("RIVET_ICON_COLOR") {
        settings.icon_color = settings::IconColor::from_id(&c);
    }
    // Test hooks for checks: the Peek panel ("1", "left") and a right-hand sidebar.
    match std::env::var("RIVET_PEEK").as_deref() {
        Ok("left") => {
            settings.show_peek = true;
            settings.peek_left = true;
        }
        Ok(_) => settings.show_peek = true,
        Err(_) => {}
    }
    if let Some(h) = std::env::var("RIVET_PEEK_PREVIEW_H").ok().and_then(|v| v.parse::<f32>().ok()) {
        settings.peek_preview_h = h;
    }
    // Test hook for checks: RIVET_THEME=light|dark (ignores the desktop palette).
    match std::env::var("RIVET_THEME").as_deref() {
        Ok("light") => {
            settings.theme = settings::ThemeChoice::Light;
            settings.follow_desktop_colors = false;
        }
        Ok("dark") => {
            settings.theme = settings::ThemeChoice::Dark;
            settings.follow_desktop_colors = false;
        }
        _ => {}
    }
    // Test hooks: toolbar at the bottom, status bar at the top.
    if std::env::var_os("RIVET_TOOLBAR_BOTTOM").is_some() {
        settings.toolbar_bottom = true;
    }
    if std::env::var_os("RIVET_NO_STATUSBAR").is_some() {
        settings.show_statusbar = false;
    }
    if std::env::var_os("RIVET_VIM").is_some() {
        settings.vim_keys = true;
    }
    if std::env::var_os("RIVET_TERMINAL").is_some() {
        settings.terminal = true;
    }
    if let Ok(f) = std::env::var("RIVET_MONO_FONT") {
        settings.mono_font = f;
    }
    if std::env::var_os("RIVET_ZEN").is_some() {
        settings.zen = true;
    }
    if std::env::var_os("RIVET_STATUS_TOP").is_some() {
        settings.statusbar_top = true;
    }
    if std::env::var_os("RIVET_SIDEBAR_RIGHT").is_some() {
        settings.sidebar_right = true;
    }
    if hide_menubar {
        settings.show_menubar = false;
    }
    if tree {
        settings.show_sidebar = true;
        settings.sidebar_tree = true;
    }
    match view.as_deref() {
        Some("list") => settings.view = settings::ViewMode::List,
        Some("compact") => settings.view = settings::ViewMode::Compact,
        Some("icons") => settings.view = settings::ViewMode::Icons,
        _ => {}
    }
    if screenshot.is_some() || measuring() {
        // Checks always use a predictable size and the default look.
        settings.window_w = 1100.0;
        settings.window_h = 700.0;
    }

    // A picker: read the request, start in its folder.
    let pick_title = if pick {
        use std::io::Read;
        let mut text = String::new();
        let _ = std::io::stdin().read_to_string(&mut text);
        // Test hook for checks: RIVET_PICK_REQUEST holds the request when there is no stdin.
        if text.trim().is_empty() {
            text = std::env::var("RIVET_PICK_REQUEST").unwrap_or_default().replace("\\n", "\n").replace("\\t", "\t");
        }
        let r = picker::Request::parse(&text);
        if start.is_none() {
            start = Some(picker::start_folder(&r));
        }
        if screenshot.is_none() && !measuring() {
            settings.window_w = 920.0;
            settings.window_h = 600.0;
        }
        // A picker, not the file manager: no menu bar, status bar, Peek, tabs or Zen; a short
        // toolbar; the list view. None of this is saved.
        use settings::ToolItem::*;
        settings.toolbar = vec![Back, Forward, Up, PathBar, Search, NewFolder, ShowHidden, ViewList, ViewIcons];
        settings.show_menubar = false;
        settings.show_statusbar = false;
        settings.show_peek = false;
        settings.zen = false;
        settings.view = settings::ViewMode::List;
        settings.sidebar_width = settings.sidebar_width.min(180.0);
        let t = r.window_title();
        picker::begin(r);
        Some(t)
    } else {
        None
    };

    // Flatpak sets this to the app id so the desktop matches the window.
    let app_id = std::env::var("FLATPAK_ID").unwrap_or_else(|_| "rivet-files".into());
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(pick_title.unwrap_or_else(|| "Rivet".into()))
        .with_app_id(app_id)
        .with_inner_size([settings.window_w, settings.window_h])
        .with_min_inner_size([520.0, 340.0])
        .with_decorations(settings.decorations);
    if let Some(img) = icons::render_svg(desktop::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(egui::IconData { rgba, width: 128, height: 128 });
    }
    if screenshot.is_some() || measuring() {
        // A fixed size (min = max) so tiling window managers float the window at
        // exactly this size instead of stretching it to fill a tall tile.
        let size = [settings.window_w, settings.window_h];
        viewport = viewport.with_active(false).with_min_inner_size(size).with_max_inner_size(size).with_resizable(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }

    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };

    // winit panics from inside xkbcommon-dl when libxkbcommon-x11 is missing.
    // Tell the person which package to install instead.
    if let Err(msg) = cli::ensure_gui_libraries() {
        eprintln!("{msg}");
        std::process::exit(1);
    }

    // Screenshots are drawn in memory: a window only gets frames while the screen shows it.
    if screenshot.is_some() && std::env::var_os("RIVET_SHOT_WINDOW").is_none() {
        let size = [settings.window_w, settings.window_h];
        headless::run(size, move |cc| Box::new(app::RivetApp::new(cc, settings, start, screenshot, demo)));
        if picker::active() {
            std::process::exit(if picker::answer().is_some() { 0 } else { 1 });
        }
        return Ok(());
    }

    let result = eframe::run_native(
        "Rivet",
        options,
        Box::new(move |cc| Ok(Box::new(app::RivetApp::new(cc, settings, start, screenshot, demo)))),
    );
    // A picker reports what was chosen on stdout (exit 0), or exits 1 when cancelled.
    if picker::active() {
        match picker::answer() {
            Some(a) => {
                print!("{}", a.to_text());
                std::process::exit(0);
            }
            None => std::process::exit(1),
        }
    }
    result
}

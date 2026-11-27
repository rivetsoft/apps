//! Rivet View: open a picture and step through the rest of its folder, with zoom, turning,
//! slideshows, camera details and animations. Reading pictures is the `rivet-media` crate's
//! work, and the stage that zooms and turns them is the one Rivet Files' Preview uses too.

mod demo;
mod folder;
mod gui;
mod loader;
mod save;
mod settings;

use std::path::PathBuf;
use std::process::ExitCode;

/// Where crash reports go, and the issue form they're shared on.
pub const CRASH: rivet_xdg::crash::App = rivet_xdg::crash::App { name: "Rivet View", id: "view", state: "rivet-view", version: env!("CARGO_PKG_VERSION"), repo: env!("CARGO_PKG_REPOSITORY") };

const HELP: &str = "Rivet View: look through the pictures in a folder

Usage:
  rivet-view                        Open with no picture: open one, or drop one in
  rivet-view PICTURE                Open a picture, and step through the rest of its folder
  rivet-view FOLDER                 Open the first picture in a folder
  rivet-view --sort name|size|type|modified [--desc] PICTURE
                                    Step through the folder in this order (Rivet Files
                                    passes the order it shows the folder in)
  rivet-view --slideshow PICTURE    Start a slideshow from this picture
  rivet-view --install-desktop-entry
                                    Add Rivet View to the app launcher (for this user)
  rivet-view -h, --help             This help
  rivet-view -V, --version          The version

Keys: Left and Right step, + and - zoom, 0 fits, 1 shows 100%, R and Shift+R turn, H and V
flip, F5 starts a slideshow, F11 fills the screen, I shows the details, Delete moves the
picture to the trash.";

/// The launcher entry and icon, in the user's data folder, running this program.
fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-view.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-view.svg");
    let text = include_str!("../data/rivet-view.desktop").replace("Exec=rivet-view", &format!("Exec=\"{}\"", exe.display()));
    for (p, body) in [(&entry, text.as_str()), (&icon, gui::ICON_SVG)] {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(p, body).map_err(|e| format!("couldn't write {}: {e}", p.display()))?;
    }
    let _ = std::process::Command::new("update-desktop-database").arg(data.join("applications")).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    Ok(entry)
}

/// A path as it was handed over: a file manager may give an address.
fn path_of(arg: &str) -> PathBuf {
    let p = PathBuf::from(arg.strip_prefix("file://").map(rivet_xdg::places::percent_decode).unwrap_or_else(|| arg.to_string()));
    std::path::absolute(&p).unwrap_or(p)
}

fn main() -> ExitCode {
    rivet_kit::mem::alloc::setup();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut screenshot = None;
    if let Some(i) = args.iter().position(|a| a == "--screenshot") {
        args.remove(i);
        if i < args.len() {
            screenshot = Some(PathBuf::from(args.remove(i)));
        }
    }
    let mut start = gui::Start::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" | "help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" | "version" => {
                println!("rivet-view {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--install-desktop-entry" => {
                return match install_desktop_entry() {
                    Ok(p) => {
                        println!("Added {}", p.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("rivet-view: {e}");
                        ExitCode::FAILURE
                    }
                };
            }
            "--sort" => {
                i += 1;
                match args.get(i).and_then(|s| folder::SortKey::parse(s)) {
                    Some(k) => start.order.key = k,
                    None => {
                        eprintln!("rivet-view: --sort takes name, size, type or modified");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--desc" => start.order.desc = true,
            "--slideshow" => start.slideshow = true,
            other if other.starts_with('-') && other.len() > 1 => {
                eprintln!("rivet-view: unknown option {other} (see --help)");
                return ExitCode::FAILURE;
            }
            other => start.path = Some(path_of(other)),
        }
        i += 1;
    }
    if screenshot.is_some() {
        rivet_xdg::crash::NO_WRITE.store(true, std::sync::atomic::Ordering::Relaxed);
        settings::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
        let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        });
        rivet_kit::headless::run(size.unwrap_or([1100.0, 720.0]), move |cc| Box::new(gui::ViewApp::new(cc, start, screenshot)));
        return ExitCode::SUCCESS;
    }
    rivet_xdg::crash::install(CRASH);
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet View").with_app_id("rivet-view").with_inner_size([1100.0, 720.0]).with_min_inner_size([480.0, 320.0]);
    if let Some(img) = rivet_look::icons::render_svg(gui::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width: 128, height: 128 });
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    match eframe::run_native("Rivet View", options, Box::new(move |cc| Ok(Box::new(gui::ViewApp::new(cc, start, None))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-view: couldn't open the window: {e}");
            ExitCode::FAILURE
        }
    }
}

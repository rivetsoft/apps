//! Rivet Cinema: plays video and music files and folders, with a queue, previews on the seek
//! bar, tracks and subtitles, and where each file was left. Playback is libmpv's, loaded at run
//! time through rivet-media; this is the window around it and its place on the desktop.

mod glyphs;
mod gui;
mod mpris;
mod previews;
mod queue;
mod resume;
mod settings;
mod video;

use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "Rivet Cinema: play videos and music

Usage:
  rivet-cinema                        Open with nothing playing: open files, or drop them in
  rivet-cinema FILE|FOLDER|PLAYLIST…  Play these (a folder plays everything in it). With
                                    Rivet Cinema already open, they are added to its queue
  rivet-cinema --new-window FILE…     Play these in a window of their own
  rivet-cinema --install-desktop-entry
                                    Add Rivet Cinema to the app launcher (for this user)
  rivet-cinema -h, --help             This help
  rivet-cinema -V, --version          The version

Rivet Cinema plays through libmpv. Install mpv-libs (Fedora), libmpv2 (Debian, Ubuntu)
or mpv (Arch) if it isn't there.";

/// The launcher entry and icon, in the user's data folder, running this program.
fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-cinema.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-cinema.svg");
    let text = include_str!("../data/rivet-cinema.desktop").replace("Exec=rivet-cinema", &format!("Exec=\"{}\"", exe.display()));
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
    let mut files = Vec::new();
    let mut new_window = false;
    for a in &args {
        match a.as_str() {
            "-h" | "--help" | "help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" | "version" => {
                println!("rivet-cinema {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--install-desktop-entry" => {
                return match install_desktop_entry() {
                    Ok(p) => {
                        println!("Added {}", p.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("rivet-cinema: {e}");
                        ExitCode::FAILURE
                    }
                };
            }
            "--new-window" => new_window = true,
            other if other.starts_with('-') && other.len() > 1 => {
                eprintln!("rivet-cinema: unknown option {other} (see --help)");
                return ExitCode::FAILURE;
            }
            other => files.push(path_of(other)),
        }
    }
    if screenshot.is_some() {
        let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        });
        rivet_kit::headless::run(size.unwrap_or([1100.0, 680.0]), move |cc| Box::new(gui::PlayApp::new(cc, Vec::new(), screenshot)));
        return ExitCode::SUCCESS;
    }
    // One Rivet Cinema at a time: files opened while it runs join its queue.
    if !files.is_empty() && !new_window && mpris::send_to_running(&files) {
        return ExitCode::SUCCESS;
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet Cinema").with_app_id("rivet-cinema").with_inner_size([1100.0, 680.0]).with_min_inner_size([360.0, 300.0]);
    if let Some(img) = rivet_look::icons::render_svg(gui::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width: 128, height: 128 });
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, renderer: eframe::Renderer::Glow, ..Default::default() };
    match eframe::run_native("Rivet Cinema", options, Box::new(move |cc| Ok(Box::new(gui::PlayApp::new(cc, files, None))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-cinema: couldn't open the window: {e}");
            ExitCode::FAILURE
        }
    }
}

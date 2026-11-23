//! Rivet Snip: clip and mark up a screenshot, then copy or save it. A compositor that takes
//! its own screenshots hands the picture over as a file or on standard input; anywhere else
//! `rivet-snip capture` takes one first.

mod capture;
mod doc;
mod export;
mod gui;
mod paint;
mod settings;

use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "Rivet Snip: clip and mark up screenshots

Usage:
  rivet-snip FILE                   Open a picture (PNG, JPEG or WebP) to clip and mark up
  rivet-snip -                      Read the picture from standard input
  rivet-snip capture [--delay SECONDS]
                                    Take a picture of the screen and open it, after a wait
                                    if asked for
  rivet-snip --clipboard            Open the picture on the clipboard
  rivet-snip                        Open with no picture: take one, open one or paste one
  rivet-snip --install-desktop-entry
                                    Add Rivet Snip to the app launcher (for this user)
  rivet-snip -h, --help             This help
  rivet-snip -V, --version          The version

Options:
  --replace                         Save writes over FILE. Without it, Save makes a new file
                                    beside FILE and the original is left as it was.

For a compositor: run `rivet-snip FILE --replace` on the screenshot it saved, or pipe the PNG
to `rivet-snip -`. In the window: Ctrl+C copies the picture, Ctrl+S saves it.";

/// Where crash reports go, and the issue form they're shared on.
pub const CRASH: rivet_xdg::crash::App = rivet_xdg::crash::App { name: "Rivet Snip", id: "snip", state: "rivet-snip", version: env!("CARGO_PKG_VERSION"), repo: env!("CARGO_PKG_REPOSITORY") };

/// The launcher entry and icon, in the user's data folder, running this program.
fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-snip.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-snip.svg");
    let text = include_str!("../data/rivet-snip.desktop").replace("Exec=rivet-snip", &format!("Exec=\"{}\"", exe.display()));
    for (p, body) in [(&entry, text.as_str()), (&icon, gui::ICON_SVG)] {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(p, body).map_err(|e| format!("couldn't write {}: {e}", p.display()))?;
    }
    let _ = std::process::Command::new("update-desktop-database").arg(data.join("applications")).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    Ok(entry)
}

fn main() -> ExitCode {
    rivet_xdg::crash::install(CRASH);
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut screenshot = None;
    if let Some(i) = args.iter().position(|a| a == "--screenshot") {
        args.remove(i);
        if i < args.len() {
            screenshot = Some(PathBuf::from(args.remove(i)));
        }
    }
    let mut start = gui::Start::default();
    let (mut take, mut delay, mut from_clipboard, mut from_stdin) = (false, 0.0f32, false, false);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" | "help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" | "version" => {
                println!("rivet-snip {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--install-desktop-entry" => {
                return match install_desktop_entry() {
                    Ok(p) => {
                        println!("Added {}", p.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("rivet-snip: {e}");
                        ExitCode::FAILURE
                    }
                };
            }
            "capture" => take = true,
            "--delay" => {
                i += 1;
                match args.get(i).and_then(|v| v.parse::<f32>().ok()).filter(|v| (0.0..=60.0).contains(v)) {
                    Some(v) => delay = v,
                    None => {
                        eprintln!("rivet-snip: --delay takes a number of seconds, up to 60");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--clipboard" => from_clipboard = true,
            "--replace" => start.replace = true,
            "-" => from_stdin = true,
            other if other.starts_with('-') => {
                eprintln!("rivet-snip: unknown option {other} (see --help)");
                return ExitCode::FAILURE;
            }
            // A file handed over by a file manager may come as an address.
            other => start.source = Some(PathBuf::from(other.strip_prefix("file://").map(rivet_xdg::places::percent_decode).unwrap_or_else(|| other.to_string()))),
        }
        i += 1;
    }
    if screenshot.is_some() {
        rivet_xdg::crash::NO_WRITE.store(true, std::sync::atomic::Ordering::Relaxed);
        // A check may be given a picture of its own; otherwise the window draws a sample one.
        if let Some(file) = &start.source {
            start.picture = Some(capture::open(file).map_err(|e| format!("{e}.")));
        }
        let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        });
        rivet_kit::headless::run(size.unwrap_or([1100.0, 700.0]), move |cc| Box::new(gui::SnipApp::new(cc, start, screenshot)));
        return ExitCode::SUCCESS;
    }
    // The picture is read before the window opens, so a screenshot never has the window in it.
    if take {
        if delay > 0.0 {
            std::thread::sleep(std::time::Duration::from_secs_f32(delay));
        }
        start.picture = Some(capture::screen().map_err(|e| format!("Couldn't take a screenshot: {e}.")));
        start.source = None;
        // A picture of the whole screen is most often wanted for one part of it.
        start.crop_first = true;
    } else if from_stdin {
        start.picture = Some(capture::stdin().map_err(|e| format!("Couldn't open the picture: {e}.")));
        start.source = None;
    } else if from_clipboard {
        start.picture = Some(capture::clipboard().map_err(|e| format!("Couldn't open the clipboard's picture: {e}.")));
        start.source = None;
    } else if let Some(file) = &start.source {
        start.picture = Some(capture::open(file).map_err(|e| format!("{e}.")));
    }
    if start.source.is_none() {
        start.replace = false;
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet Snip").with_app_id("rivet-snip").with_inner_size([1100.0, 760.0]).with_min_inner_size([640.0, 420.0]);
    if let Some(img) = rivet_look::icons::render_svg(gui::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width: 128, height: 128 });
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    match eframe::run_native("Rivet Snip", options, Box::new(move |cc| Ok(Box::new(gui::SnipApp::new(cc, start, None))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-snip: couldn't open the window: {e}");
            ExitCode::FAILURE
        }
    }
}

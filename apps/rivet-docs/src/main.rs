//! Rivet Docs: read PDFs and comics. The documents are rivet-doc's, on a thread of their own;
//! this is the window around them.

mod gui;
mod places;
mod settings;
mod view;

use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "Rivet Docs: read PDFs and comics

Usage:
  rivet-docs                    Open with no document: open one, or drop one in
  rivet-docs FILE               Open a PDF, a comic (cbz, cbr, cb7) or a picture
  rivet-docs --install-desktop-entry
                                Add Rivet Docs to the app launcher (for this user)
  rivet-docs -h, --help         This help
  rivet-docs -V, --version      The version

A file opens on the page it was left at.";

/// The launcher entry and icon, in the user's data folder, running this program.
fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-docs.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-docs.svg");
    let text = include_str!("../data/rivet-docs.desktop").replace("Exec=rivet-docs", &format!("Exec=\"{}\"", exe.display()));
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
    let mut file = None;
    for a in &args {
        match a.as_str() {
            "-h" | "--help" | "help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" | "version" => {
                println!("rivet-docs {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--install-desktop-entry" => {
                return match install_desktop_entry() {
                    Ok(p) => {
                        println!("Added {}", p.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("rivet-docs: {e}");
                        ExitCode::FAILURE
                    }
                };
            }
            other if other.starts_with('-') && other.len() > 1 => {
                eprintln!("rivet-docs: unknown option {other} (see --help)");
                return ExitCode::FAILURE;
            }
            other => file = Some(path_of(other)),
        }
    }
    // Checks open RIVET_DOCS_FILE when no file is named.
    if screenshot.is_some() && file.is_none() {
        file = std::env::var_os("RIVET_DOCS_FILE").map(PathBuf::from);
    }
    if screenshot.is_some() {
        let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        });
        rivet_kit::headless::run(size.unwrap_or([1100.0, 760.0]), move |cc| Box::new(gui::DocsApp::new(cc, file, screenshot)));
        return ExitCode::SUCCESS;
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet Docs").with_app_id("rivet-docs").with_inner_size([1100.0, 780.0]).with_min_inner_size([560.0, 400.0]);
    if let Some(img) = rivet_look::icons::render_svg(gui::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width: 128, height: 128 });
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    match eframe::run_native("Rivet Docs", options, Box::new(move |cc| Ok(Box::new(gui::DocsApp::new(cc, file, None))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-docs: couldn't open the window: {e}");
            ExitCode::FAILURE
        }
    }
}

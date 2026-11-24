//! Rivet Vice: open an archive and see what is in it, take out all or some of it, and make
//! new archives. The work is the `rivet-archive` crate's, which Rivet Files' Compress and
//! Extract use too; this is the window around it, and a command line for other programs.

mod gui;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use rivet_archive::Format;

const HELP: &str = "Rivet Vice: open archives, extract from them and make new ones

Usage:
  rivet-vice                        Open with no archive: open one, or drop files in
  rivet-vice ARCHIVE                Open an archive and show what is in it
  rivet-vice FILE… (not archives)   Open ready to compress them
  rivet-vice --extract-here ARCHIVE…
                                    Extract each archive beside itself, with no window
  rivet-vice --extract-to FOLDER ARCHIVE…
                                    Extract each archive into FOLDER, with no window
  rivet-vice --compress [--format zip|tar.gz|tar.xz|tar.zst|7z] FILE…
                                    Make one archive of the files, beside the first of
                                    them, with no window, and print where it is
  rivet-vice --install-desktop-entry
                                    Add Rivet Vice to the app launcher (for this user)
  rivet-vice -h, --help             This help
  rivet-vice -V, --version          The version

An archive with one thing at its top is extracted as that thing; one with several gets a
folder named after the archive. Nothing already there is written over: a name that is
taken gets a number after it.";

/// The launcher entry and icon, in the user's data folder, running this program.
fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-vice.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-vice.svg");
    let text = include_str!("../data/rivet-vice.desktop").replace("Exec=rivet-vice", &format!("Exec=\"{}\"", exe.display()));
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

/// The work with no window: extract or compress, saying what was made.
fn headless(extract_to: Option<Option<PathBuf>>, compress: Option<Format>, files: &[PathBuf]) -> ExitCode {
    let go = AtomicBool::new(false);
    if files.is_empty() {
        eprintln!("rivet-vice: nothing was named (see --help)");
        return ExitCode::FAILURE;
    }
    let mut failed = false;
    if let Some(format) = compress {
        if !format.available() {
            eprintln!("rivet-vice: the tool that makes {} archives isn't installed", format.ext());
            return ExitCode::FAILURE;
        }
        let dir = files[0].parent().map(PathBuf::from).unwrap_or_default();
        let archive = rivet_archive::free_name(&dir, &dir.join(format!("{}{}", rivet_archive::suggest_name(files), format.ext())));
        match rivet_archive::compress(files, &archive, format, &go) {
            Ok(()) => println!("{}", archive.display()),
            Err(e) => {
                eprintln!("rivet-vice: {e}");
                failed = true;
            }
        }
    } else if let Some(to) = extract_to {
        for a in files {
            if !a.is_file() {
                eprintln!("rivet-vice: {}: there is no such file", a.display());
                failed = true;
                continue;
            }
            let into = to.clone().or_else(|| a.parent().map(PathBuf::from)).unwrap_or_default();
            match std::fs::create_dir_all(&into).map_err(|e| e.to_string()).and_then(|_| rivet_archive::extract(a, &into, &go, &rivet_archive::free_name)) {
                Ok(made) => println!("{}", made.display()),
                Err(e) => {
                    eprintln!("rivet-vice: {}: {e}", a.display());
                    failed = true;
                }
            }
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
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
    let mut files: Vec<PathBuf> = Vec::new();
    // None: a window. Some(None): extract beside each archive. Some(Some(folder)): into it.
    let mut extract_to: Option<Option<PathBuf>> = None;
    let (mut compress, mut format) = (false, None);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" | "help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" | "version" => {
                println!("rivet-vice {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--install-desktop-entry" => {
                return match install_desktop_entry() {
                    Ok(p) => {
                        println!("Added {}", p.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("rivet-vice: {e}");
                        ExitCode::FAILURE
                    }
                };
            }
            "--extract-here" => extract_to = Some(None),
            "--extract-to" => {
                i += 1;
                match args.get(i) {
                    Some(dir) => extract_to = Some(Some(path_of(dir))),
                    None => {
                        eprintln!("rivet-vice: --extract-to takes a folder");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--compress" => compress = true,
            "--format" => {
                i += 1;
                let id = args.get(i).map(|f| f.trim_start_matches('.')).unwrap_or_default();
                match Format::ALL.into_iter().find(|f| f.ext().trim_start_matches('.') == id) {
                    Some(f) => format = Some(f),
                    None => {
                        eprintln!("rivet-vice: --format takes zip, tar.gz, tar.xz, tar.zst or 7z");
                        return ExitCode::FAILURE;
                    }
                }
            }
            other if other.starts_with('-') && other.len() > 1 => {
                eprintln!("rivet-vice: unknown option {other} (see --help)");
                return ExitCode::FAILURE;
            }
            other => files.push(path_of(other)),
        }
        i += 1;
    }
    if compress || extract_to.is_some() {
        return headless(extract_to, compress.then(|| format.unwrap_or(Format::Zip)), &files);
    }
    // One archive opens; anything else is something to compress.
    let start = match files.as_slice() {
        [] => gui::Start::Empty,
        [one] if rivet_archive::is_archive(one) => gui::Start::Open(one.clone()),
        _ => gui::Start::Compress(files),
    };
    if screenshot.is_some() {
        let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        });
        rivet_kit::headless::run(size.unwrap_or([900.0, 600.0]), move |cc| Box::new(gui::ViceApp::new(cc, start, screenshot)));
        return ExitCode::SUCCESS;
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet Vice").with_app_id("rivet-vice").with_inner_size([900.0, 620.0]).with_min_inner_size([520.0, 360.0]);
    if let Some(img) = rivet_look::icons::render_svg(gui::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width: 128, height: 128 });
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    match eframe::run_native("Rivet Vice", options, Box::new(move |cc| Ok(Box::new(gui::ViceApp::new(cc, start, None))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-vice: couldn't open the window: {e}");
            ExitCode::FAILURE
        }
    }
}

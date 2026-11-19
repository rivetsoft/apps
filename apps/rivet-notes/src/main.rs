//! Rivet Notes: Markdown notes in plain folders (vaults), with frontmatter and tags.

mod gui;
mod attach;
mod blocks;
mod format;
mod layout;
mod links;
mod live;
mod props;
mod render;
mod settings;
mod shell;
mod slash;
mod tasks;
mod vault;

use std::path::PathBuf;
use std::process::ExitCode;

/// The note to open at the start (`--note`), a path inside the vault.
pub static START_NOTE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

const HELP: &str = "Rivet Notes: Markdown notes in plain folders, with frontmatter and tags

Usage:
  rivet-notes [FOLDER]        Open the window (on FOLDER as the vault, or the last one)
  rivet-notes [FOLDER] --note PATH
                              …on that note (PATH from the top of the vault)
  rivet-notes new [--vault FOLDER] [TITLE]
                              Make a note and open it
  rivet-notes tags [FOLDER]   List the tags in a vault, with how many notes have each
  rivet-notes shell-plugin    Run as Rivet Shell's Notes plugin (the shell starts this)
  rivet-notes --install-desktop-entry
                              Add Rivet Notes to the app launcher (for this user)
  rivet-notes -h, --help      This help
  rivet-notes -V, --version   The version

A vault is any folder: its .md files are the notes, its sub-folders their folders.
Folders and files starting with a dot (.git, .obsidian) are left out.";

/// Sample notes for screenshot runs (checks never show the person's own vaults).
fn sample_vault() -> Option<PathBuf> {
    // A plain vault name ("Notes") for the window's title, status bar and breadcrumbs.
    let root = std::env::temp_dir().join(format!("rivet-notes-sample-{}", std::process::id())).join("Notes");
    let files: [(&str, &str); 6] = [
        ("Welcome.md", "---\ntitle: Welcome\ntags: [start]\ncreated: 2026-11-16 09:00\n---\n# Welcome\n\nThis vault is just a folder of **Markdown** files. Properties live in each note's *frontmatter*.\n\n## Getting started\n\n- Browse by folder or by tag\n- Search with Ctrl+F, or jump with `Ctrl+P`\n- [x] Open the vault\n- [ ] Write a [[Inbox]] note\n\n> Notes are plain files, so any app can read them.\n\n---\n\nMore at [the docs](https://example.org/rivet). #start\n"),
        ("Inbox.md", "Ideas to sort later. #todo\n\n- Look at sync options\n- Try the light theme\n\n- [ ] Plain todo, not a super one\n- [ ] Back up the vault #task #chores priority:high\n- [/] Reply to Sam #task #people due:2026-11-17\n- [x] Pick a theme #task\n"),
        ("Projects/Rivet FTP.md", "---\ntitle: Rivet FTP\ntags: [work, rivet]\nstatus: active\ndue: 2026-12-01\npriority: 2\nshipped: false\nrepo: https://example.org/rivet-ftp\naliases: [FTP client, Transfers]\n---\n# Rivet FTP\n\nNext: FTP and FTPS support.\n\n- [ ] Ship FTPS #task #work due:2026-11-20 priority:medium\n- [ ] Write the release notes #task #work due:2026-11-10\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n"),
        ("Projects/Rivet Notes.md", "---\ntitle: Rivet Notes\ntags: [work, rivet, notes]\nstatus: planning\n---\n# Rivet Notes\n\nVaults, frontmatter, folders and tags. Shares its look with [[Rivet FTP]].\n"),
        ("Journal/2026-11-16.md", "---\ntags: [journal]\nmood: good\n---\n# Sunday\n\nWrote the notes app. #rivet\n\nStarted from [[Welcome]], next up [[Rivet FTP]].\n"),
        ("Reading/Books.md", "---\ntitle: Books\ntags:\n  - reading\n  - lists\n---\n# Books\n\n1. A Pattern Language\n2. The Design of Everyday Things\n"),
    ];
    for (rel, text) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent()?).ok()?;
        std::fs::write(&p, text).ok()?;
    }
    Some(root)
}

/// The launcher entry and icon, in the user's data folder, running this program.
fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-notes.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-notes.svg");
    let text = include_str!("../data/rivet-notes.desktop").replace("Exec=rivet-notes", &format!("Exec=\"{}\"", exe.display()));
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
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut screenshot = None;
    if let Some(i) = args.iter().position(|a| a == "--screenshot") {
        args.remove(i);
        if i < args.len() {
            screenshot = Some(PathBuf::from(args.remove(i)));
        }
    }
    // --note PATH and --vault FOLDER, anywhere.
    let mut take = |flag: &str| {
        let i = args.iter().position(|a| a == flag)?;
        args.remove(i);
        (i < args.len()).then(|| args.remove(i))
    };
    let (note, vault_arg) = (take("--note"), take("--vault"));
    if let Some(n) = note {
        let _ = START_NOTE.set(n);
    }
    let home_path = |a: &str| match a.strip_prefix("~/") {
        Some(rest) => rivet_xdg::places::home().join(rest),
        None => PathBuf::from(a),
    };
    match args.first().map(String::as_str) {
        Some("shell-plugin") => return shell::run(),
        Some("new") => {
            let dir = vault_arg.as_deref().map(home_path).or_else(|| settings::Settings::load().vaults.into_iter().next());
            let Some(dir) = dir.filter(|d| d.is_dir()) else {
                eprintln!("rivet-notes: which vault? (rivet-notes new --vault FOLDER TITLE)");
                return ExitCode::FAILURE;
            };
            let title = args[1..].join(" ");
            match vault::create_note(&dir, if title.trim().is_empty() { "Untitled" } else { &title }) {
                Ok(p) => {
                    let _ = START_NOTE.set(p.strip_prefix(&dir).unwrap_or(&p).to_string_lossy().into_owned());
                    args = vec![dir.to_string_lossy().into_owned()];
                }
                Err(e) => {
                    eprintln!("rivet-notes: couldn't make the note: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        _ => {}
    }
    match args.first().map(String::as_str) {
        Some("-h" | "--help" | "help") => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version" | "version") => {
            println!("rivet-notes {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("--install-desktop-entry") => {
            return match install_desktop_entry() {
                Ok(p) => {
                    println!("Added {}", p.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("rivet-notes: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("tags") => {
            let dir = args.get(1).map(PathBuf::from).or_else(|| settings::Settings::load().vaults.into_iter().next());
            let Some(dir) = dir.filter(|d| d.is_dir()) else {
                eprintln!("rivet-notes: which vault? (rivet-notes tags FOLDER)");
                return ExitCode::FAILURE;
            };
            let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
            for n in vault::scan(&dir) {
                for t in n.tags {
                    *counts.entry(t).or_default() += 1;
                }
            }
            let mut v: Vec<(String, usize)> = counts.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            for (t, n) in v {
                println!("{n:>4}  #{t}");
            }
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let mut open = args.first().map(|a| home_path(a)).or_else(|| vault_arg.as_deref().map(home_path));
    if screenshot.is_some() {
        if open.is_none() && std::env::var_os("RIVET_NOTES_EMPTY").is_none() {
            open = sample_vault();
        }
        // RIVET_SHOT_SIZE=WxH for documentation pictures (1100x700 by default).
        let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        });
        rivet_kit::headless::run(size.unwrap_or([1100.0, 700.0]), move |cc| Box::new(gui::NotesApp::new(cc, open, screenshot)));
        return ExitCode::SUCCESS;
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet Notes").with_app_id("rivet-notes").with_inner_size([1100.0, 720.0]).with_min_inner_size([620.0, 400.0]);
    if let Some(img) = rivet_look::icons::render_svg(gui::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width: 128, height: 128 });
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    match eframe::run_native("Rivet Notes", options, Box::new(move |cc| Ok(Box::new(gui::NotesApp::new(cc, open, None))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-notes: couldn't open the window: {e}");
            ExitCode::FAILURE
        }
    }
}

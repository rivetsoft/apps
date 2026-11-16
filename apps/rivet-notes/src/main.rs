//! Rivet Notes: Markdown notes in plain folders (vaults), with frontmatter and tags.

mod gui;
mod format;
mod layout;
mod live;
mod props;
mod render;
mod settings;
mod vault;

use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "Rivet Notes: Markdown notes in plain folders, with frontmatter and tags

Usage:
  rivet-notes [FOLDER]        Open the window (on FOLDER as the vault, or the last one)
  rivet-notes tags [FOLDER]   List the tags in a vault, with how many notes have each
  rivet-notes -h, --help      This help
  rivet-notes -V, --version   The version

A vault is any folder: its .md files are the notes, its sub-folders their folders.
Folders and files starting with a dot (.git, .obsidian) are left out.";

/// Sample notes for screenshot runs (checks never show the person's own vaults).
fn sample_vault() -> Option<PathBuf> {
    let root = std::env::temp_dir().join(format!("rivet-notes-sample-{}", std::process::id()));
    let files: [(&str, &str); 6] = [
        ("Welcome.md", "---\ntitle: Welcome\ntags: [start]\ncreated: 2026-11-16 09:00\n---\n# Welcome\n\nThis vault is just a folder of **Markdown** files. Properties live in each note's *frontmatter*.\n\n## Getting started\n\n- Browse by folder or by tag\n- Search with Ctrl+F, or jump with `Ctrl+P`\n- [x] Open the vault\n- [ ] Write a [[Inbox]] note\n\n> Notes are plain files, so any app can read them.\n\n---\n\nMore at [the docs](https://example.org/rivet). #start\n"),
        ("Inbox.md", "Ideas to sort later. #todo\n\n- Look at sync options\n- Try the light theme\n"),
        ("Projects/Rivet FTP.md", "---\ntitle: Rivet FTP\ntags: [work, rivet]\nstatus: active\ndue: 2026-12-01\npriority: 2\nshipped: false\nrepo: https://example.org/rivet-ftp\naliases: [FTP client, Transfers]\n---\n# Rivet FTP\n\nNext: FTP and FTPS support.\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n"),
        ("Projects/Rivet Notes.md", "---\ntitle: Rivet Notes\ntags: [work, rivet, notes]\nstatus: planning\n---\n# Rivet Notes\n\nVaults, frontmatter, folders and tags.\n"),
        ("Journal/2026-11-16.md", "---\ntags: [journal]\nmood: good\n---\n# Sunday\n\nWrote the notes app. #rivet\n"),
        ("Reading/Books.md", "---\ntitle: Books\ntags:\n  - reading\n  - lists\n---\n# Books\n\n1. A Pattern Language\n2. The Design of Everyday Things\n"),
    ];
    for (rel, text) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent()?).ok()?;
        std::fs::write(&p, text).ok()?;
    }
    Some(root)
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
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version" | "version") => {
            println!("rivet-notes {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
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
    let mut open = args.first().map(|a| match a.strip_prefix("~/") {
        Some(rest) => rivet_xdg::places::home().join(rest),
        None => PathBuf::from(a),
    });
    if screenshot.is_some() {
        if open.is_none() && std::env::var_os("RIVET_NOTES_EMPTY").is_none() {
            open = sample_vault();
        }
        rivet_kit::headless::run([1100.0, 700.0], move |cc| Box::new(gui::NotesApp::new(cc, open, screenshot)));
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

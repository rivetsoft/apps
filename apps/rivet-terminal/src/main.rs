//! Rivet Terminal: a terminal with tabs and split panes, in the Rivet apps' look.

mod gui;
mod instance;
mod settings;

use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "Rivet Terminal: a terminal with tabs and split panes

Usage:
  rivet-terminal                    Open a window with your shell
  rivet-terminal -d, --directory DIR
                                    Start in DIR
  rivet-terminal -e PROGRAM [ARGS...]
                                    Run PROGRAM instead of the shell (everything after -e)
  rivet-terminal --separate         Open a window that doesn't join the Rivet Terminal already
                                    running (its tabs and panes can't move to the others)
  rivet-terminal --install-desktop-entry
                                    Add Rivet Terminal to the app launcher (for this user)
  rivet-terminal -h, --help         This help
  rivet-terminal -V, --version      The version

In the window: Ctrl+Shift+T a new tab, Ctrl+Shift+Enter a new pane, Ctrl+Shift+P every command.";

/// A small project and a script that draws a believable session, for screenshot runs (checks
/// never show the person's own shell, files or history). The argument picks which pane.
fn demo() -> Option<(PathBuf, PathBuf)> {
    let base = std::env::temp_dir().join(format!("rivet-terminal-demo-{}", std::process::id()));
    let root = base.join("hello-rivet");
    for (rel, text) in [("Cargo.toml", "[package]\nname = \"hello-rivet\"\n"), ("README.md", "# Hello, Rivet\n"), ("src/main.rs", "fn main() {}\n"), ("web/app.js", "\n"), ("scripts/build.sh", "\n")] {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent()?).ok()?;
        std::fs::write(&p, text).ok()?;
    }
    let script = base.join("demo.sh");
    std::fs::write(&script, DEMO).ok()?;
    Some((root, script))
}

const DEMO: &str = r#"#!/bin/sh
e=$(printf '\033')
p() { printf '%s[1;32mrivet%s[0m@%s[1;34mdemo%s[0m %s[36m~/hello-rivet%s[0m %s[33m(main)%s[0m %s[1;35m>%s[0m %s' "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$1"; }
case "$1" in
2)
  p 'cat src/words.rs'; echo
  printf '%s[35mpub fn%s[0m %s[34mis_long%s[0m(word: &%s[33mstr%s[0m) -> %s[33mbool%s[0m {\n' "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e"
  printf '    word.chars().count() > %s[33m8%s[0m\n}\n' "$e" "$e"
  p 'cargo test'; echo
  printf '%s[1;32m   Compiling%s[0m hello-rivet v0.1.0\n' "$e" "$e"
  printf '%s[1;32m    Finished%s[0m test profile in 0.61s\n' "$e" "$e"
  printf 'running 3 tests\n'
  printf 'test words::long_words ... %s[32mok%s[0m\n' "$e" "$e"
  printf 'test words::short_words ... %s[32mok%s[0m\n' "$e" "$e"
  printf 'test counts_the ... %s[32mok%s[0m\n\n' "$e" "$e"
  printf 'test result: %s[32mok%s[0m. 3 passed; 0 failed\n' "$e" "$e"
  p '' ;;
3)
  p 'git status --short'; echo
  printf ' %s[31mM%s[0m src/main.rs\n %s[31mM%s[0m web/app.js\n%s[32m??%s[0m scripts/release\n' "$e" "$e" "$e" "$e" "$e" "$e"
  p '' ;;
*)
  p 'ls'; echo
  printf 'Cargo.toml  README.md  %s[1;34mscripts%s[0m  %s[1;34msrc%s[0m  %s[1;34mweb%s[0m\n' "$e" "$e" "$e" "$e" "$e" "$e"
  p 'cargo run'; echo
  printf '%s[1;32m   Compiling%s[0m hello-rivet v0.1.0 (~/hello-rivet)\n' "$e" "$e"
  printf '%s[1;32m    Finished%s[0m `dev` profile [unoptimized + debuginfo] target(s) in 0.42s\n' "$e" "$e"
  printf '%s[1;32m     Running%s[0m `target/debug/hello-rivet`\n' "$e" "$e"
  printf '9 words, %s[1mthe%s[0m appears 2 times\n' "$e" "$e"
  p 'git log --oneline -4'; echo
  printf '%s[33ma1b2c3d%s[0m %s[36m(HEAD -> %s[32mmain%s[36m)%s[0m Count words in the browser too\n' "$e" "$e" "$e" "$e" "$e" "$e"
  printf '%s[33m9f8e7d6%s[0m Add a long-word check\n' "$e" "$e"
  printf '%s[33m5c4b3a2%s[0m Read the text from a file\n' "$e" "$e"
  printf '%s[33m1a2b3c4%s[0m First commit\n' "$e" "$e"
  p 'colors'; echo
  for i in 0 1 2 3 4 5 6 7; do printf '%s[4%sm    %s[0m' "$e" "$i" "$e"; done; echo
  for i in 0 1 2 3 4 5 6 7; do printf '%s[10%sm    %s[0m' "$e" "$i" "$e"; done; echo
  printf 'Docs: %s[4;34mhttps://github.com/rivetsoft/apps%s[0m\n' "$e" "$e"
  p '' ;;
esac
exec sleep 600
"#;

/// The launcher entry and icon, in the user's data folder, running this program.
fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-terminal.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-terminal.svg");
    let text = include_str!("../data/rivet-terminal.desktop").replace("Exec=rivet-terminal", &format!("Exec=\"{}\"", exe.display()));
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
    let mut separate = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" | "help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" | "version" => {
                println!("rivet-terminal {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--install-desktop-entry" => {
                return match install_desktop_entry() {
                    Ok(p) => {
                        println!("Added {}", p.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("rivet-terminal: {e}");
                        ExitCode::FAILURE
                    }
                };
            }
            "--separate" => separate = true,
            "-d" | "--directory" | "--working-directory" => {
                i += 1;
                start.cwd = args.get(i).map(PathBuf::from);
            }
            "-e" | "--" => {
                if i + 1 < args.len() {
                    start.program = Some((args[i + 1].clone(), args[i + 2..].to_vec()));
                }
                break;
            }
            other if !other.starts_with('-') && std::path::Path::new(other).is_dir() => start.cwd = Some(PathBuf::from(other)),
            other => {
                eprintln!("rivet-terminal: unknown option {other} (see --help)");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }
    if screenshot.is_some() {
        start.demo = demo();
        let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        });
        rivet_kit::headless::run(size.unwrap_or([1100.0, 700.0]), move |cc| Box::new(gui::Windows::new(gui::TermApp::new(cc, start, screenshot, None))));
        return ExitCode::SUCCESS;
    }
    // A Rivet Terminal is already running: it opens the window, so tabs and panes can move
    // between all of them.
    if !separate && instance::send(&start) {
        return ExitCode::SUCCESS;
    }
    let inbox = if separate { None } else { instance::listen() };
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet Terminal").with_app_id("rivet-terminal").with_inner_size([960.0, 620.0]).with_min_inner_size([420.0, 260.0]);
    if let Some(img) = rivet_look::icons::render_svg(gui::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width: 128, height: 128 });
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    match eframe::run_native("Rivet Terminal", options, Box::new(move |cc| Ok(Box::new(gui::Windows::new(gui::TermApp::new(cc, start, None, inbox)))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-terminal: couldn't open the window: {e}");
            ExitCode::FAILURE
        }
    }
}

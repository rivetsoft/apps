//! Rivet Greeter: a login screen for greetd, drawn as Rivet Shell's lock screen is
//! (rivet-lockscreen), over the wallpaper and in the colors each person chose last.
//!
//! greetd starts it in a compositor that shows one program (cage, Sway, Softcore's
//! `--greeter`), as its own user. It lists the accounts that can sign in, shows whoever changed
//! their wallpaper last, and signs in through greetd (greetd.rs), which starts the person's
//! session when this program leaves. What each person looks like on it comes from the shared
//! folder their session fills (rivet_lockscreen::handoff): Rivet Shell does it by itself, and
//! `rivet-greeter sync` does it from any other desktop (sync.rs).

mod greetd;
mod setup;
mod sync;

use std::path::PathBuf;
use std::process::ExitCode;

use rivet_lockscreen::screen::{self, Mode, Screen};

const HELP: &str = "\
rivet-greeter: a login screen for greetd, in Rivet Shell's look

Usage:
  rivet-greeter                    The login screen (greetd runs this, in cage, Sway or Softcore)
  sudo rivet-greeter --setup       Make the folder each person's wallpaper and look are left in,
                                   and print the lines for greetd's settings
  rivet-greeter sync [OPTIONS]     Leave your wallpaper and colors for the login screen, from a
                                   desktop without Rivet Shell (it does this by itself)
      --wallpaper PICTURE          The picture to show behind it
      --name NAME                  The name to show (your account's full name otherwise)
      --clock FORMAT               The clock's format, such as %-I:%M %p for 12-hour time
  rivet-greeter --screenshot FILE  Save a picture of the login screen, with made-up details
  -h, --help                       This
  -V, --version                    The version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    if args.iter().any(|a| a == "--setup") {
        return setup::run();
    }
    if let Some(path) = after("--screenshot") {
        return screenshot(PathBuf::from(path));
    }
    match args.first().map(String::as_str) {
        None => greet(),
        Some("sync") => sync::run(&args[1..]),
        Some("-h" | "--help" | "help") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some("-V" | "--version") => {
            println!("rivet-greeter {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("rivet-greeter doesn't know {other:?} (rivet-greeter --help)");
            ExitCode::FAILURE
        }
    }
}

/// The login screen: sign in, and greetd starts the session when this leaves.
fn greet() -> ExitCode {
    let screen = Screen { mode: Mode::Greet, people: rivet_lockscreen::handoff::people(), check: greetd::sign_in, on_locked: |_| {}, app_id: "rivet-greeter", title: "Rivet Greeter" };
    match screen::run(screen) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("rivet-greeter: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `--screenshot FILE`: the login screen over a made-up wallpaper. Hooks: RIVET_GREETER_THEME
/// (light or dark), RIVET_GREETER_ACCENT (a color like #3584e4), RIVET_GREETER_WALL (a picture
/// to show instead of the made-up wallpaper), and the face's own
/// (RIVET_LOCK_TYPED, RIVET_LOCK_ERROR, RIVET_LOCK_CONFIRM; see rivet_lockscreen::shot).
fn screenshot(path: PathBuf) -> ExitCode {
    let mut look = rivet_lockscreen::Look::default();
    if let Ok(t) = std::env::var("RIVET_GREETER_THEME") {
        look.theme = if t == "light" { rivet_look::theme::Mode::Light } else { rivet_look::theme::Mode::Dark };
    }
    if let Some(c) = std::env::var("RIVET_GREETER_ACCENT").ok().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()) {
        look.accent = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
        look.follow_desktop_colors = false;
        look.own_surfaces = true;
    }
    rivet_lockscreen::shot::run(path, look, true, std::env::var_os("RIVET_GREETER_WALL").map(PathBuf::from));
    ExitCode::SUCCESS
}

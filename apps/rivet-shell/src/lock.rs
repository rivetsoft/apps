//! `rivet-shell lock`: the lock screen, its own program (as swaylock and hyprlock are), so the
//! session stays locked even if the shell stops. The screen itself (session lock, the face,
//! its look) is rivet-lockscreen's, shared with Rivet Greeter; this side gives it the person's
//! settings, checks the password with PAM (auth.rs), and tells the shell and logind it's locked.
//!
//! If it dies while locked, the compositor keeps the screens locked; the shell starts it again
//! (it sees $XDG_RUNTIME_DIR/rivet-shell.locked still there). SIGUSR1 unlocks (what
//! `loginctl unlock-session` asks of the shell). `rivet-shell lock --preview` shows it over the
//! desktop without locking (Escape leaves), to try the look and the password safely.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use rivet_lockscreen::screen::{self, Mode, Screen};

use crate::config::Settings;

/// Left in the runtime directory while the screens are locked by this program.
pub fn marker() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("rivet-shell.locked")
}

/// Exit codes the shell tells apart.
pub const EXIT_TAKEN: u8 = screen::EXIT_TAKEN;

/// The person's picture: ~/.face, else AccountsService's.
pub(crate) fn avatar(login: &str) -> Option<eframe::egui::ColorImage> {
    rivet_lockscreen::avatar(&faces(login))
}

fn faces(login: &str) -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    vec![home.join(".face"), home.join(".face.icon"), rivet_lockscreen::accounts_face(login)]
}

/// Lock the session (or, with `preview`, only show the lock screen) until the password is given.
pub fn run(preview: bool) -> std::process::ExitCode {
    crate::guard::log_panics();
    let s = Settings::load();
    let login = crate::auth::login_name();
    let me = rivet_lockscreen::Person { name: crate::sys::user_name(), look: s.lock_look(), wallpaper: (!s.wallpaper.is_empty()).then(|| PathBuf::from(&s.wallpaper)), faces: faces(&login), session: Vec::new(), login };
    let screen = Screen { mode: if preview { Mode::Preview } else { Mode::Lock }, people: vec![me], check: |user, pw, _| crate::auth::check(user, pw), on_locked, app_id: "rivet-shell-lock", title: "Rivet Shell" };
    match screen::run(screen) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(e) => {
            eprintln!("rivet-shell lock: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// The screens locked or unlocked: the mark the shell looks for, and logind's hint for other
/// programs that ask it.
fn on_locked(on: bool) {
    if on {
        let _ = std::fs::write(marker(), std::process::id().to_string());
    } else {
        let _ = std::fs::remove_file(marker());
    }
    std::thread::spawn(move || {
        if let Ok(c) = zbus::blocking::Connection::system() {
            let _ = c.call_method(Some("org.freedesktop.login1"), "/org/freedesktop/login1/session/auto", Some("org.freedesktop.login1.Session"), "SetLockedHint", &(on,));
        }
    });
}

/// `rivet-shell lock --screenshot FILE`: the lock screen over the demo wallpaper, with made-up
/// state. Hooks: RIVET_LOCK_TYPED=N (dots), RIVET_LOCK_ERROR=1 (a wrong password),
/// RIVET_LOCK_CONFIRM=1 (Shut Down asking for a second click), RIVET_SHELL_THEME, RIVET_SHELL_ACCENT.
pub fn screenshot(path: PathBuf) -> std::process::ExitCode {
    crate::config::NO_SAVE.store(true, Ordering::Relaxed);
    let mut s = Settings::default();
    if let Ok(t) = std::env::var("RIVET_SHELL_THEME") {
        s.theme = if t == "light" { rivet_look::theme::Mode::Light } else { rivet_look::theme::Mode::Dark };
    }
    if let Some(c) = std::env::var("RIVET_SHELL_ACCENT").ok().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()) {
        s.accent = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
        s.follow_desktop_colors = false;
    }
    rivet_lockscreen::shot::run(path, s.lock_look(), false, None);
    std::process::ExitCode::SUCCESS
}

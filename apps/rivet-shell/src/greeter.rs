//! The login screen's side of the shell. The greeter itself is its own program now, Rivet
//! Greeter (`rivet-greeter`); the shell only leaves each person's wallpaper and look in the
//! folder it reads (rivet_lockscreen::handoff) whenever they change.
//!
//! `rivet-shell greeter`, which Rivet Shell 0.1.2 was itself, still works for a while so no
//! one's greetd settings break: it runs rivet-greeter with the same words, or says how to get it.

use std::path::PathBuf;

use crate::config::Settings;

pub fn dir() -> PathBuf {
    rivet_lockscreen::handoff::dir()
}

/// Leave this person's wallpaper, look, name, picture and session for the greeter, on a thread
/// (the picture takes a moment). `colors`: the accent, secondary and tertiary as shown now, so
/// colors made from the wallpaper reach the greeter too. Nothing happens without the folder.
pub fn publish(s: &Settings, colors: [[u8; 3]; 3]) {
    if !dir().is_dir() {
        return;
    }
    let mut look = s.lock_look();
    look.accent = colors[0];
    look.secondary = Some(colors[1]);
    look.tertiary = Some(colors[2]);
    let wallpaper = PathBuf::from(s.matugen_picture());
    let name = crate::sys::user_name();
    std::thread::spawn(move || {
        let _ = rivet_lockscreen::handoff::publish(&look, &name, Some(&wallpaper));
    });
}

/// Rivet Greeter: beside this program, else on the path.
fn rivet_greeter() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|p| Some(p.parent()?.join("rivet-greeter")));
    let path = std::env::var_os("PATH").unwrap_or_else(|| "/usr/local/bin:/usr/bin".into());
    beside.into_iter().chain(std::env::split_paths(&path).map(|d| d.join("rivet-greeter"))).find(|p| p.is_file())
}

/// `rivet-shell greeter [WORDS]`: `rivet-greeter [WORDS]`, which took its place.
pub fn run(args: &[String]) -> std::process::ExitCode {
    let Some(greeter) = rivet_greeter() else {
        eprintln!(
            "The login screen moved out of Rivet Shell into its own program, Rivet Greeter, which isn't\n\
             installed. Build it with `cargo build --release -p rivet-greeter` and put rivet-greeter in\n\
             /usr/local/bin, then change `rivet-shell greeter` to `rivet-greeter` in /etc/greetd/config.toml."
        );
        return std::process::ExitCode::FAILURE;
    };
    use std::os::unix::process::CommandExt;
    let e = std::process::Command::new(&greeter).args(args).exec();
    eprintln!("Couldn't start {} ({e})", greeter.display());
    std::process::ExitCode::FAILURE
}

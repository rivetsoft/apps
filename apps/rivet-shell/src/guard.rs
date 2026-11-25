//! Keeping the shell up. `rivet-shell` itself only watches: it starts the real shell as a
//! child and starts it again when that one crashes (a panic, or a fault in a library), so
//! the panels, wallpaper and notifications come back in a moment instead of staying gone
//! until the next login. A shell that's told to quit, or that can't start at all, ends it.
//!
//! Every panic is written to `crash.log` in the state folder (`~/.local/state/rivet-shell`)
//! with where it happened, so there's something to send along with a report.

use std::io::Write;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

/// Set for the child: it is the shell, not the watcher.
pub const CHILD: &str = "RIVET_SHELL_CHILD";
/// A shell that ran this long had started properly; an earlier crash counts towards giving up.
const SETTLED: Duration = Duration::from_secs(20);
/// Crashes in a row, each before `SETTLED`, before the watcher gives up.
const TRIES: u32 = 6;
const LOG_MAX: u64 = 256 * 1024;

pub fn is_child() -> bool {
    std::env::var_os(CHILD).is_some()
}

pub fn log_path() -> PathBuf {
    rivet_xdg::places::state_home().join("rivet-shell/crash.log")
}

fn now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // UTC, without a date library: days since 1970 to a calendar date (civil-from-days).
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Add an entry to the crash log (which starts over once it's grown large).
pub fn log(text: &str) {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).map(|m| m.len() > LOG_MAX).unwrap_or(false) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{}] rivet-shell {}\n{text}\n", now(), env!("CARGO_PKG_VERSION"));
    }
}

/// Write panics to the crash log as well as to the terminal. One on the main thread ends
/// the shell (and the watcher starts it again); one on a helper thread ends only that helper,
/// as before, so a picture or a plugin that can't be read doesn't take the panels with it.
pub fn log_panics() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let trace = std::backtrace::Backtrace::force_capture();
        log(&format!("panic on thread \"{}\": {info}\n{trace}", thread.name().unwrap_or("unnamed")));
        before(info);
    }));
}

/// How the child ended: asked to (or unable to start), or by crashing.
fn crashed(status: std::process::ExitStatus) -> Option<String> {
    match (status.code(), status.signal()) {
        // 0: quit. 1: it said why it couldn't run (no Wayland session, no layer-shell…).
        (Some(0 | 1), _) => None,
        // Rust's exit code for a panic on the main thread.
        (Some(101), _) => Some("a panic".into()),
        (Some(c), _) => Some(format!("exit code {c}")),
        // Stopped from outside (logout, kill, the compositor going away).
        (None, Some(libc::SIGTERM | libc::SIGINT | libc::SIGHUP | libc::SIGKILL | libc::SIGPIPE)) => None,
        (None, Some(s)) => Some(format!("signal {s}")),
        (None, None) => None,
    }
}

/// Run the shell as a child until it ends on purpose.
pub fn watch() -> ExitCode {
    let Ok(exe) = std::env::current_exe() else {
        eprintln!("rivet-shell: can't find its own program to start");
        return ExitCode::FAILURE;
    };
    let mut quick = 0u32;
    loop {
        let started = Instant::now();
        let mut cmd = Command::new(&exe);
        cmd.env(CHILD, "1");
        // The shell goes when the watcher does (a kill of the watcher isn't left half done).
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let status = match cmd.status() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("rivet-shell: couldn't start ({e})");
                return ExitCode::FAILURE;
            }
        };
        let Some(why) = crashed(status) else {
            return if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
        };
        quick = if started.elapsed() < SETTLED { quick + 1 } else { 0 };
        if quick >= TRIES {
            let line = format!("the shell stopped ({why}) {TRIES} times in a row soon after starting; not starting it again");
            eprintln!("rivet-shell: {line} (see {})", log_path().display());
            log(&line);
            return ExitCode::FAILURE;
        }
        // The compositor went away with it: nothing to come back to.
        if !wayland_up() {
            return ExitCode::FAILURE;
        }
        let wait = Duration::from_millis(300 * 2u64.pow(quick.min(6)));
        let line = format!("the shell stopped ({why}); starting it again in {:.1} s", wait.as_secs_f32());
        eprintln!("rivet-shell: {line}");
        log(&line);
        std::thread::sleep(wait);
    }
}

/// Whether the session's Wayland socket still answers.
fn wayland_up() -> bool {
    let name = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    let path = if name.starts_with('/') { PathBuf::from(name) } else { std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_default().join(name) };
    std::os::unix::net::UnixStream::connect(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::ExitStatus;

    #[test]
    fn tells_a_crash_from_a_stop() {
        // Wait statuses: an exit code is in the high byte, a signal in the low bits.
        let code = |c: i32| ExitStatus::from_raw(c << 8);
        assert_eq!(crashed(code(0)), None);
        assert_eq!(crashed(code(1)), None);
        assert!(crashed(code(101)).is_some());
        assert_eq!(crashed(ExitStatus::from_raw(libc::SIGTERM)), None);
        assert!(crashed(ExitStatus::from_raw(libc::SIGSEGV)).is_some());
        assert!(crashed(ExitStatus::from_raw(libc::SIGABRT)).is_some());
    }

    #[test]
    fn dates_the_log() {
        assert!(now().ends_with("UTC") && now().starts_with("20"));
    }
}

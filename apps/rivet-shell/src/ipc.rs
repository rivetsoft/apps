//! Talking to a running shell: `rivet-shell msg launcher` (bind it to a key in your compositor),
//! `msg settings`, `msg wallpaper PATH`, `msg reload`, `msg quit`. One line over a Unix socket in
//! the session's runtime folder; one shell per Wayland session.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub fn socket() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    dir.join(format!("rivet-shell-{}.sock", display.replace('/', "_")))
}

/// Send a command to the running shell.
pub fn send(line: &str) -> Result<(), String> {
    let mut s = UnixStream::connect(socket()).map_err(|_| "Rivet Shell isn't running in this session".to_string())?;
    writeln!(s, "{line}").map_err(|e| e.to_string())
}

/// Whether a shell already answers in this session.
pub fn running() -> bool {
    UnixStream::connect(socket()).is_ok()
}

/// Listen for commands on a thread; each line goes to `on_line`.
pub fn listen(on_line: impl Fn(String) + Send + 'static) -> Result<(), String> {
    let path = socket();
    // A socket left by a shell that didn't close cleanly.
    if path.exists() && !running() {
        let _ = std::fs::remove_file(&path);
    }
    let l = UnixListener::bind(&path).map_err(|e| format!("couldn't listen at {}: {e}", path.display()))?;
    std::thread::Builder::new()
        .name("rivet-shell commands".into())
        .spawn(move || {
            for c in l.incoming().flatten() {
                let mut line = String::new();
                if BufReader::new(c).read_line(&mut line).is_ok() && !line.trim().is_empty() {
                    on_line(line.trim().to_string());
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Remove the socket when the shell ends.
pub fn cleanup() {
    let _ = std::fs::remove_file(socket());
}

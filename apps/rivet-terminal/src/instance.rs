//! One Rivet Terminal per desktop session, so its windows can trade tabs and panes: a shell
//! keeps running when its pane moves, which only works inside one process.
//!
//! The first copy listens on a socket in the runtime folder. A later launch sends it what to
//! open (folder, program, environment), gets "ok" back and exits; the running copy opens the
//! window. With no answer, the later launch opens its own window as before.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::gui::Start;

/// The socket, one per user and display (a second login on another display gets its own).
fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join(format!("rivet-{}", std::env::var("USER").unwrap_or_default())));
    let display = std::env::var("WAYLAND_DISPLAY").or_else(|_| std::env::var("DISPLAY")).unwrap_or_default();
    let display: String = display.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    dir.join(format!("rivet-terminal-{display}.sock"))
}

/// Fields, each `kind\0value\0`.
fn encode(start: &Start) -> Vec<u8> {
    let mut out = Vec::new();
    let mut put = |k: &str, v: &str| {
        out.extend_from_slice(k.as_bytes());
        out.push(0);
        out.extend_from_slice(v.as_bytes());
        out.push(0);
    };
    if let Some(d) = &start.cwd {
        put("dir", &d.to_string_lossy());
    }
    if let Some((p, args)) = &start.program {
        put("exec", p);
        for a in args {
            put("arg", a);
        }
    }
    for (k, v) in &start.env {
        put("env", &format!("{k}={v}"));
    }
    out
}

fn decode(bytes: &[u8]) -> Start {
    let mut start = Start::default();
    let fields: Vec<String> = bytes.split(|b| *b == 0).map(|f| String::from_utf8_lossy(f).into_owned()).collect();
    for pair in fields.chunks(2) {
        let [k, v] = pair else { break };
        match k.as_str() {
            "dir" => start.cwd = Some(PathBuf::from(v)),
            "exec" => start.program = Some((v.clone(), Vec::new())),
            "arg" => {
                if let Some((_, args)) = &mut start.program {
                    args.push(v.clone());
                }
            }
            "env" => {
                if let Some((k, v)) = v.split_once('=') {
                    start.env.insert(k.to_string(), v.to_string());
                }
            }
            _ => {}
        }
    }
    start
}

/// Ask a running Rivet Terminal to open this window. True when it said it would.
pub fn send(start: &Start) -> bool {
    let Ok(mut s) = UnixStream::connect(socket_path()) else { return false };
    let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let mut with_env = start.clone();
    with_env.env = std::env::vars().collect::<HashMap<_, _>>();
    // Meant for this launch's own window, not for the shell in it.
    for k in ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"] {
        with_env.env.remove(k);
    }
    if s.write_all(&encode(&with_env)).is_err() || s.shutdown(std::net::Shutdown::Write).is_err() {
        return false;
    }
    let mut answer = String::new();
    s.read_to_string(&mut answer).is_ok() && answer.trim() == "ok"
}

/// Become the copy others send their windows to (None when the socket can't be made).
pub fn listen() -> Option<UnixListener> {
    let path = socket_path();
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // A socket left by a copy that ended: nothing answers it any more.
    if path.exists() && UnixStream::connect(&path).is_err() {
        let _ = std::fs::remove_file(&path);
    }
    UnixListener::bind(&path).ok()
}

/// Take requests on a thread, passing each on and waking the window.
pub fn serve(listener: UnixListener, tx: Sender<Start>, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut s) = conn else { continue };
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let mut bytes = Vec::new();
            if s.read_to_end(&mut bytes).is_err() {
                continue;
            }
            if tx.send(decode(&bytes)).is_err() {
                break;
            }
            let _ = s.write_all(b"ok\n");
            ctx.request_repaint();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_reads_back() {
        let mut s = Start { cwd: Some("/tmp/a b".into()), program: Some(("htop".into(), vec!["-d".into(), "5".into()])), ..Default::default() };
        s.env.insert("PATH".into(), "/usr/bin:/bin".into());
        s.env.insert("EMPTY".into(), String::new());
        let back = decode(&encode(&s));
        assert_eq!(back.cwd, s.cwd);
        assert_eq!(back.program, s.program);
        assert_eq!(back.env, s.env);
    }
}

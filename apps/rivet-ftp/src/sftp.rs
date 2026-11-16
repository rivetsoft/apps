//! A server session through the system's own `sftp` program, the way Rivet drives `gio`: nothing
//! extra is linked, and your ssh keys, agent, `~/.ssh/config` and known_hosts all apply.
//!
//! One `sftp` process runs in batch mode for the whole session. Each request is written to it
//! as commands followed by `!echo` with a marker; everything up to the marker is that request's
//! output. Commands start with `-` so a failure is reported rather than ending the session.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui;
use rivet_connections::Connection;

use crate::listing::Entry;

const MARK: &str = "@@rivet-ftp-done@@";

#[derive(Debug)]
pub enum Request {
    List(String),
    Mkdir(String),
    Rename(String, String),
    /// A file or a folder (with everything in it).
    Remove(String, bool),
    /// Server path → local path.
    Get(String, PathBuf, bool),
    /// Local path → server path.
    Put(PathBuf, String, bool),
}

#[derive(Debug)]
pub enum Reply {
    /// Logged in; the server's starting folder.
    Connected(String),
    Listing(String, Vec<Entry>),
    /// Request number, and an error if it failed.
    Done(u64, Option<String>),
    /// The session ended (could not log in, or the connection dropped).
    Closed(String),
}

pub struct Session {
    tx: Sender<(u64, Request)>,
    pub rx: Receiver<Reply>,
    next: u64,
}

impl Session {
    /// Log in on a background thread; `Connected` or `Closed` arrives on `rx`.
    pub fn open(conn: &Connection, ctx: egui::Context) -> Session {
        let (tx, req_rx) = channel::<(u64, Request)>();
        let (reply_tx, rx) = channel();
        let conn = conn.clone();
        std::thread::spawn(move || {
            let send = |r: Reply| {
                let _ = reply_tx.send(r);
                ctx.request_repaint();
            };
            let mut proc = match Proc::start(&conn) {
                Ok(p) => p,
                Err(e) => return send(Reply::Closed(e)),
            };
            let home = match proc.run(&["pwd".into()]) {
                Ok(out) => out.iter().find_map(|l| l.strip_prefix("Remote working directory: ").map(str::to_string)).unwrap_or_else(|| "/".into()),
                Err(e) => return send(Reply::Closed(e)),
            };
            send(Reply::Connected(home));
            for (id, req) in req_rx {
                let result = match req {
                    Request::List(path) => match proc.list(&path) {
                        Ok(entries) => {
                            send(Reply::Listing(path, entries));
                            Ok(())
                        }
                        Err(e) => Err(e),
                    },
                    Request::Mkdir(p) => proc.simple(&format!("-mkdir {}", q(&p))),
                    Request::Rename(a, b) => proc.simple(&format!("-rename {} {}", q(&a), q(&b))),
                    Request::Remove(p, dir) => proc.remove(&p, dir),
                    Request::Get(remote, local, dir) => proc.simple(&format!("-get{} -p {} {}", if dir { " -r" } else { "" }, q(&remote), q(&local.to_string_lossy()))),
                    Request::Put(local, remote, dir) => proc.simple(&format!("-put{} -p {} {}", if dir { " -r" } else { "" }, q(&local.to_string_lossy()), q(&remote))),
                };
                match result {
                    Err(e) if proc.dead() => return send(Reply::Closed(e)),
                    r => send(Reply::Done(id, r.err())),
                }
            }
            proc.quit();
        });
        Session { tx, rx, next: 1 }
    }

    /// Queue a request; returns its number (matched by `Reply::Done`).
    pub fn send(&mut self, r: Request) -> u64 {
        let id = self.next;
        self.next += 1;
        let _ = self.tx.send((id, r));
        id
    }
}

struct Proc {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<std::io::PipeReader>,
}

impl Proc {
    fn start(conn: &Connection) -> Result<Proc, String> {
        // Log in without a remote folder; folders are always given in full.
        let mut c = conn.clone();
        c.remote = None;
        let mut args: Vec<String> = vec!["-b".into(), "-".into(), "-o".into(), "BatchMode=yes".into(), "-o".into(), "ConnectTimeout=15".into(), "-o".into(), "ServerAliveInterval=20".into()];
        // A new server is trusted the first time (like ssh's accept-new); a changed key is refused.
        args.extend(["-o".into(), "StrictHostKeyChecking=accept-new".into()]);
        // Test hook: extra ssh options, e.g. a throwaway known_hosts file.
        if let Ok(extra) = std::env::var("RIVET_FTP_SSH_ARGS") {
            args.extend(extra.split_whitespace().map(str::to_string));
        }
        args.extend(c.sftp_args()?);
        let (reader, writer) = std::io::pipe().map_err(|e| e.to_string())?;
        let mut cmd = Command::new("sftp");
        cmd.args(&args).stdin(Stdio::piped()).stdout(writer.try_clone().map_err(|e| e.to_string())?).stderr(writer);
        let mut child = cmd.spawn().map_err(|e| format!("couldn't start sftp ({e}); is OpenSSH installed?"))?;
        drop(cmd); // closes our copies of the write end, so a dead sftp ends the reads
        let stdin = child.stdin.take().ok_or("no stdin")?;
        Ok(Proc { child, stdin, out: BufReader::new(reader) })
    }

    fn dead(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(None))
    }

    /// Run commands; their output lines (without the echoed commands).
    fn run(&mut self, cmds: &[String]) -> Result<Vec<String>, String> {
        let mut script = String::new();
        for c in cmds {
            script.push_str(c);
            script.push('\n');
        }
        script.push_str(&format!("!echo {MARK}\n"));
        self.stdin.write_all(script.as_bytes()).and_then(|_| self.stdin.flush()).map_err(|_| self.ended(Vec::new()))?;
        let mut lines = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            match self.out.read_line(&mut line) {
                Ok(0) | Err(_) => return Err(self.ended(lines)),
                Ok(_) => {}
            }
            let l = line.trim_end_matches(['\n', '\r']);
            if l == MARK {
                return Ok(lines);
            }
            if l.starts_with("sftp> ") || l.starts_with("Warning: Permanently added") {
                continue;
            }
            lines.push(l.to_string());
        }
    }

    /// The session ended: say why, in words.
    fn ended(&mut self, lines: Vec<String>) -> String {
        let _ = self.child.wait();
        let text = lines.join("\n");
        if text.contains("Permission denied") {
            "The server refused the login. Check the user name and key, and that the key was added to the server (Preferences, SSH Keys, Add to Server). A key with a passphrase needs to be in your ssh agent first (ssh-add).".into()
        } else if text.contains("Host key verification failed") || text.contains("REMOTE HOST IDENTIFICATION HAS CHANGED") {
            "The server's identity has changed since you last connected, so Rivet FTP stopped. If you expected this, remove the old entry from ~/.ssh/known_hosts.".into()
        } else if text.contains("Could not resolve hostname") {
            "Couldn't find that server. Check the address.".into()
        } else if text.contains("Connection refused") {
            "The server refused the connection. Check the address and port.".into()
        } else if text.contains("timed out") {
            "The server didn't answer in time.".into()
        } else if text.trim().is_empty() {
            "The connection closed.".into()
        } else {
            text.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("The connection closed.").to_string()
        }
    }

    /// A command whose only output, if any, is an error (or progress words we ignore).
    fn simple(&mut self, cmd: &str) -> Result<(), String> {
        let out = self.run(&[cmd.to_string()])?;
        let errors: Vec<&String> = out
            .iter()
            .filter(|l| !l.trim().is_empty())
            .filter(|l| !["Fetching ", "Uploading ", "Entering ", "Retrieving ", "Removing ", "Renaming "].iter().any(|p| l.starts_with(p)))
            .collect();
        if errors.is_empty() { Ok(()) } else { Err(errors.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")) }
    }

    fn list(&mut self, path: &str) -> Result<Vec<Entry>, String> {
        let out = self.run(&[format!("-ls -la {}", q(path))])?;
        let mut entries = Vec::new();
        let mut errors = Vec::new();
        let mut saw_self = false;
        for l in &out {
            match crate::listing::parse_ls_line(l, path) {
                Some(e) if e.name == "." || e.name == ".." => saw_self = true,
                Some(e) => entries.push(e),
                None if !l.trim().is_empty() => errors.push(l.clone()),
                None => {}
            }
        }
        if entries.is_empty() && !saw_self && !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        Ok(entries)
    }

    /// sftp has no recursive delete: empty a folder first, deepest first.
    fn remove(&mut self, path: &str, dir: bool) -> Result<(), String> {
        if !dir {
            return self.simple(&format!("-rm {}", q(path)));
        }
        for e in self.list(path)? {
            let child = join(path, &e.name);
            self.remove(&child, e.is_dir && !e.is_link)?;
        }
        self.simple(&format!("-rmdir {}", q(path)))
    }

    fn quit(mut self) {
        let _ = self.stdin.write_all(b"bye\n");
        let _ = self.child.wait();
    }
}

/// A path as one sftp argument: every character that sftp would treat as a separator, quote or
/// pattern is escaped, so names such as `odd name [1].txt` are taken literally.
fn q(p: &str) -> String {
    let mut s = String::with_capacity(p.len() + 8);
    for ch in p.chars() {
        if !(ch.is_alphanumeric() || "/._-+,:@%=~".contains(ch)) {
            s.push('\\');
        }
        s.push(ch);
    }
    s
}

/// `/a/b` + `c` → `/a/b/c`
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

/// The folder above (`/` stays `/`).
pub fn parent(path: &str) -> String {
    let t = path.trim_end_matches('/');
    match t.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => t[..i].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_and_paths() {
        assert_eq!(q("/srv/odd name [1].txt"), r"/srv/odd\ name\ \[1\].txt");
        assert_eq!(q("it's \"x\""), r#"it\'s\ \"x\""#);
        assert_eq!(join("/", "a"), "/a");
        assert_eq!(join("/a", "b"), "/a/b");
        assert_eq!(parent("/a/b/"), "/a");
        assert_eq!(parent("/a"), "/");
        assert_eq!(parent("/"), "/");
    }
}

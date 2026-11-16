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

/// A password and/or key passphrase for one login. Handed to ssh through its askpass hook
/// (this program, run with RIVET_FTP_ASKPASS set), never on a command line.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret {
    pub password: Option<String>,
    pub passphrase: Option<String>,
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret {{ password: {}, passphrase: {} }}", if self.password.is_some() { "…" } else { "none" }, if self.passphrase.is_some() { "…" } else { "none" })
    }
}

/// When ssh runs this program as its askpass helper: print the asked-for secret and exit.
/// Returns false when this isn't an askpass run.
pub fn askpass_main() -> bool {
    if std::env::var_os("RIVET_FTP_ASKPASS").is_none() {
        return false;
    }
    let prompt = std::env::args().nth(1).unwrap_or_default().to_lowercase();
    let answer = if prompt.contains("passphrase") {
        std::env::var("RIVET_FTP_SECRET_KEY").ok()
    } else if prompt.contains("password") {
        std::env::var("RIVET_FTP_SECRET_PW").ok()
    } else if prompt.contains("yes/no") {
        // Host-key questions: new hosts are accepted by ssh itself (accept-new); anything else is a no.
        Some("no".into())
    } else {
        None
    };
    match answer {
        Some(a) => {
            println!("{a}");
            std::process::exit(0)
        }
        None => std::process::exit(1),
    }
}

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
    /// One file's size and date (answered with `Reply::Stat`).
    Stat(String),
    /// Permissions (octal); with `true`, for everything inside a folder too.
    Chmod(String, u32, bool),
}

#[derive(Debug)]
pub enum Reply {
    /// Logged in; the server's starting folder.
    Connected(String),
    Listing(String, Vec<Entry>),
    /// Request number, and an error if it failed.
    Done(u64, Option<String>),
    /// A `Stat` answer: the file, or None when it isn't there.
    Stat(u64, Option<Entry>),
    /// The server wants a password or passphrase (or refused the one given).
    AuthFailed(String),
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
    pub fn open(conn: &Connection, ctx: egui::Context, secret: Option<Secret>) -> Session {
        let (tx, req_rx) = channel::<(u64, Request)>();
        let (reply_tx, rx) = channel();
        let conn = conn.clone();
        std::thread::spawn(move || {
            let send = |r: Reply| {
                let _ = reply_tx.send(r);
                ctx.request_repaint();
            };
            let mut proc = match Proc::start(&conn, secret.as_ref()) {
                Ok(p) => p,
                Err(e) => return send(Reply::Closed(e)),
            };
            let home = match proc.run(&["pwd".into()]) {
                Ok(out) => out.iter().find_map(|l| l.strip_prefix("Remote working directory: ").map(str::to_string)).unwrap_or_else(|| "/".into()),
                Err(e) => match e.strip_prefix(AUTH) {
                    Some(why) => return send(Reply::AuthFailed(why.to_string())),
                    None => return send(Reply::Closed(e)),
                },
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
                    Request::Chmod(path, mode, recursive) => proc.chmod(&path, mode, recursive),
                    Request::Stat(path) => match proc.stat(&path) {
                        Ok(e) => {
                            send(Reply::Stat(id, e));
                            continue;
                        }
                        Err(e) => Err(e),
                    },
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
    fn start(conn: &Connection, secret: Option<&Secret>) -> Result<Proc, String> {
        // Log in without a remote folder; folders are always given in full.
        let mut c = conn.clone();
        c.remote = None;
        // Without a secret nothing may be asked (BatchMode); with one, ssh asks this program.
        let batch = if secret.is_some() { "BatchMode=no" } else { "BatchMode=yes" };
        // BatchMode goes before -b: sftp's -b adds "BatchMode yes" of its own, and ssh keeps the
        // first value it's given.
        let mut args: Vec<String> = vec!["-o".into(), batch.into(), "-b".into(), "-".into(), "-o".into(), "ConnectTimeout=15".into(), "-o".into(), "ServerAliveInterval=20".into()];
        if secret.is_some() {
            args.extend(["-o".into(), "NumberOfPasswordPrompts=1".into()]);
        }
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
        if let Some(sec) = secret {
            let me = std::env::current_exe().map_err(|e| e.to_string())?;
            // A rebuilt program shows as "… (deleted)"; the file at that path is the new one.
            let me = PathBuf::from(me.to_string_lossy().trim_end_matches(" (deleted)").to_string());
            cmd.env("SSH_ASKPASS", me).env("SSH_ASKPASS_REQUIRE", "force").env("RIVET_FTP_ASKPASS", "1");
            if let Some(pw) = &sec.password {
                cmd.env("RIVET_FTP_SECRET_PW", pw);
            }
            if let Some(pp) = &sec.passphrase {
                cmd.env("RIVET_FTP_SECRET_KEY", pp);
            }
        }
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
            format!("{AUTH}The server refused the login. Check the user name, password or key, and that the key was added to the server (Preferences, SSH Keys, Add to Server).")
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

    /// Permissions for a file, or a folder and everything in it. Inside a folder, sub-folders
    /// also get "open" (x) wherever they can be read, so they stay usable.
    fn chmod(&mut self, path: &str, mode: u32, recursive: bool) -> Result<(), String> {
        if !recursive {
            return self.simple(&format!("-chmod {mode:o} {}", q(path)));
        }
        let dir_mode = mode | ((mode & 0o444) >> 2);
        self.simple(&format!("-chmod {dir_mode:o} {}", q(path)))?;
        for e in self.list(path)? {
            let child = join(path, &e.name);
            if e.is_dir && !e.is_link {
                self.chmod(&child, mode, true)?;
            } else if !e.is_link {
                self.simple(&format!("-chmod {mode:o} {}", q(&child)))?;
            }
        }
        Ok(())
    }

    /// One file: listed on its own, so its size and date are fresh.
    fn stat(&mut self, path: &str) -> Result<Option<Entry>, String> {
        let out = self.run(&[format!("-ls -la {}", q(path))])?;
        let dir = parent(path);
        let name = path.rsplit('/').next().unwrap_or(path);
        if let Some(e) = out.iter().filter_map(|l| crate::listing::parse_ls_line(l, &dir)).find(|e| e.name == name) {
            return Ok(Some(e));
        }
        if out.iter().any(|l| l.contains("not found")) {
            return Ok(None);
        }
        Err(out.iter().find(|l| !l.trim().is_empty()).cloned().unwrap_or_else(|| "couldn't read the file's details".into()))
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

/// Marks a refused login in an error message (turned into `Reply::AuthFailed`).
const AUTH: &str = "\u{1}auth:";

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

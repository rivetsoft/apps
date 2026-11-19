//! Rivet FTP as a Rivet Shell plugin (`rivet-ftp shell-plugin`, started by the shell's FTP
//! plugin and kept running): a panel item whose menu lists the saved connections by group, and
//! an entry for each in the launcher (`@ftp` there shows only these). Choosing one opens a new
//! Rivet FTP window connected to it.
//!
//! The shell's side of it is in its PLUGINS.md: a JSON line out for what to show, a JSON line
//! in for each thing that happens.

use std::io::BufRead;
use std::process::ExitCode;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use rivet_connections::{Connection, Store};
use serde_json::{Value, json};

/// How often the connections file is looked at again.
const EVERY: Duration = Duration::from_secs(5);

/// A connection's second line: how and where (`sftp · me@example.com`), with its folder.
fn about(c: &Connection) -> String {
    let mut s = format!("{} · {}", c.protocol.scheme(), c.target());
    if let Some(r) = c.remote.as_deref().filter(|r| !r.is_empty()) {
        s.push_str(&format!(" · {r}"));
    }
    s
}

/// What the shell shows: the panel item, its menu (the connections under their groups, the
/// ungrouped first) and the launcher's entries.
fn status(items: &[Connection]) -> Value {
    let row = |c: &Connection| json!({"id": format!("open:{}", c.name), "label": c.title(), "sub": about(c)});
    let mut menu: Vec<Value> = items.iter().filter(|c| c.category.is_none()).map(row).collect();
    let mut groups: Vec<&str> = Vec::new();
    for c in items {
        if let Some(g) = c.category.as_deref()
            && !groups.contains(&g)
        {
            groups.push(g);
        }
    }
    for g in groups {
        menu.push(json!({"heading": true, "label": g}));
        menu.extend(items.iter().filter(|c| c.category.as_deref() == Some(g)).map(row));
    }
    if items.is_empty() {
        menu.push(json!({"label": "No saved connections", "quiet": true}));
    }
    menu.push(json!({"heading": true, "label": ""}));
    menu.push(json!({"id": "window", "label": "Open Rivet FTP", "icon": "rivet-ftp"}));
    let launcher: Vec<Value> = items.iter().map(|c| json!({"id": format!("open:{}", c.name), "label": c.title(), "sub": about(c), "icon": "rivet-ftp"})).collect();
    let tooltip = match items.len() {
        0 => "No connections".to_string(),
        1 => "1 connection".to_string(),
        n => format!("{n} connections"),
    };
    json!({"icon": "rivet-ftp", "text": "", "tooltip": tooltip, "menu": menu, "launcher": launcher})
}

/// A new Rivet FTP window, connected to `name` (or just the window).
fn open(name: Option<&str>) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut c = std::process::Command::new(exe);
    if let Some(n) = name {
        c.args(["open", n]);
    }
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
    if let Ok(mut child) = c.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// What a line from the shell asks for: Some(None) the window, Some(Some(name)) a connection.
fn wanted(line: &str) -> Option<Option<String>> {
    let ev: Value = serde_json::from_str(line).ok()?;
    let s = |k: &str| ev.get(k).and_then(Value::as_str).unwrap_or("");
    if !matches!(s("event"), "menu" | "launch") {
        return None;
    }
    match s("id") {
        "window" => Some(None),
        id => id.strip_prefix("open:").map(|n| Some(n.to_string())),
    }
}

pub fn run() -> ExitCode {
    let (tx, rx) = channel::<Option<String>>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(Some(line)).is_err() {
                return;
            }
        }
        // The shell closed our input: it's done with us.
        let _ = tx.send(None);
    });
    let mut last = String::new();
    loop {
        let text = status(&Store::load().items).to_string();
        if text != last {
            println!("{text}");
            last = text;
        }
        match rx.recv_timeout(EVERY) {
            Ok(Some(line)) => {
                if let Some(what) = wanted(&line) {
                    open(what.as_deref());
                }
            }
            Ok(None) | Err(RecvTimeoutError::Disconnected) => return ExitCode::SUCCESS,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_connections_by_group() {
        let mut work = Connection::from_url("Work site", "sftp://me@example.com/var/www").unwrap();
        work.category = Some("Clients".into());
        let home = Connection::from_url("NAS", "ftp://nas.local").unwrap();
        let mut other = Connection::from_url("Shop", "ftps://shop@example.org").unwrap();
        other.category = Some("Clients".into());
        let s = status(&[work, home, other]);
        let menu = s["menu"].as_array().unwrap();
        let labels: Vec<&str> = menu.iter().map(|r| r["label"].as_str().unwrap_or("")).collect();
        assert_eq!(labels, ["NAS", "Clients", "Work site", "Shop", "", "Open Rivet FTP"]);
        assert_eq!((menu[2]["id"].as_str(), menu[2]["sub"].as_str()), (Some("open:Work site"), Some("sftp · me@example.com · /var/www")));
        assert_eq!(s["launcher"].as_array().unwrap().len(), 3);
        assert_eq!(s["tooltip"], "3 connections");
        assert_eq!(status(&[])["menu"][0]["label"], "No saved connections");
        assert_eq!(wanted(r#"{"event":"launch","id":"open:Work site","query":""}"#), Some(Some("Work site".into())));
        assert_eq!(wanted(r#"{"event":"menu","id":"window"}"#), Some(None));
        assert_eq!(wanted(r#"{"event":"click","button":"left"}"#), None);
    }
}

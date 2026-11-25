//! Language servers (LSP): completion and errors from the server a language has installed
//! (rust-analyzer, clangd, pyright, typescript-language-server, gopls…). Each runs as its own
//! program, talked to over its standard input and output; nothing is downloaded.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::syntax::Lang;

/// The servers tried for a language, first found wins: (program, arguments, install hint).
fn candidates(lang: Lang) -> &'static [(&'static str, &'static [&'static str])] {
    match lang {
        Lang::Rust => &[("rust-analyzer", &[])],
        Lang::Python => &[("pyright-langserver", &["--stdio"]), ("basedpyright-langserver", &["--stdio"]), ("pylsp", &[]), ("jedi-language-server", &[])],
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => &[("typescript-language-server", &["--stdio"])],
        Lang::Go => &[("gopls", &[])],
        Lang::C => &[("clangd", &[])],
        Lang::Bash => &[("bash-language-server", &["start"])],
        Lang::Json => &[("vscode-json-language-server", &["--stdio"])],
        Lang::Html => &[("vscode-html-language-server", &["--stdio"])],
        Lang::Css => &[("vscode-css-language-server", &["--stdio"])],
        Lang::Toml => &[("taplo", &["lsp", "stdio"])],
        Lang::Yaml => &[("yaml-language-server", &["--stdio"])],
    }
}

/// What to install for a language without a server.
pub fn install_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::Rust => "rust-analyzer (rustup component add rust-analyzer)",
        Lang::Python => "pyright or python-lsp-server",
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => "typescript-language-server",
        Lang::Go => "gopls",
        Lang::C => "clangd",
        Lang::Bash => "bash-language-server",
        Lang::Json | Lang::Html | Lang::Css => "vscode-langservers-extracted",
        Lang::Toml => "taplo",
        Lang::Yaml => "yaml-language-server",
    }
}

fn language_id(lang: Lang) -> &'static str {
    match lang {
        Lang::Rust => "rust",
        Lang::Python => "python",
        Lang::JavaScript => "javascript",
        Lang::TypeScript => "typescript",
        Lang::Tsx => "typescriptreact",
        Lang::Json => "json",
        Lang::Bash => "shellscript",
        Lang::C => "c",
        Lang::Go => "go",
        Lang::Html => "html",
        Lang::Css => "css",
        Lang::Toml => "toml",
        Lang::Yaml => "yaml",
    }
}

/// A program on the PATH (and the usual per-user folders a desktop launcher may leave out).
fn find_program(name: &str) -> Option<PathBuf> {
    let home = rivet_xdg::places::home();
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for extra in [".cargo/bin", ".local/bin", "go/bin", ".npm-global/bin", ".bun/bin", ".deno/bin"] {
        dirs.push(home.join(extra));
    }
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

// ---------------------------------------------------------------- positions and addresses

/// A file's address for the server (`file:///…`, odd characters escaped).
pub fn uri(p: &Path) -> String {
    let mut out = String::from("file://");
    for b in p.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(PathBuf::from(String::from_utf8_lossy(&out).into_owned()))
}

/// A character index in `text` as the server's (line, character): UTF-8 bytes or UTF-16 units.
pub fn to_pos(text: &str, ch: usize, utf8: bool) -> (u32, u32) {
    let (mut line, mut col) = (0u32, 0u32);
    for c in text.chars().take(ch) {
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += if utf8 { c.len_utf8() } else { c.len_utf16() } as u32;
        }
    }
    (line, col)
}

/// The server's (line, character) as a character index in `text` (clamped to the line).
pub fn from_pos(text: &str, line: u32, col: u32, utf8: bool) -> usize {
    let mut idx = 0usize;
    let mut l = 0u32;
    let mut it = text.chars().peekable();
    while l < line {
        match it.next() {
            Some('\n') => {
                l += 1;
                idx += 1;
            }
            Some(_) => idx += 1,
            None => return idx,
        }
    }
    let mut units = 0u32;
    while units < col {
        match it.next() {
            Some('\n') | None => break,
            Some(c) => {
                units += if utf8 { c.len_utf8() } else { c.len_utf16() } as u32;
                idx += 1;
            }
        }
    }
    idx
}

// ---------------------------------------------------------------- what comes back

/// One suggestion.
#[derive(Clone, Debug)]
pub struct Item {
    pub label: String,
    pub detail: String,
    /// LSP's kind number (3 function, 6 variable, 7 class, 14 keyword…); 0 for a word of the file.
    pub kind: u32,
    pub insert: String,
    /// The text it's matched against.
    pub filter: String,
    pub sort: String,
    /// The server's range to replace: (line, character) start and end.
    pub range: Option<((u32, u32), (u32, u32))>,
}

/// A problem the server found.
#[derive(Clone, Debug)]
pub struct Diag {
    pub start: (u32, u32),
    pub end: (u32, u32),
    /// 1 error, 2 warning, 3 information, 4 hint.
    pub severity: u8,
    pub message: String,
}

pub enum Event {
    Completion { seq: u64, items: Vec<Item> },
    /// Where something is defined, or used.
    Places { seq: u64, places: Vec<Place> },
    /// What the server says about what's under the pointer (plain text; empty when nothing).
    Hover { seq: u64, text: String },
}

/// A place in a file: (line, character) start and end, as the server counts.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub path: PathBuf,
    pub start: (u32, u32),
    pub end: (u32, u32),
}

/// What to ask the server about a place in the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    Definition,
    References,
    Hover,
}

fn places_from(v: &Value) -> Vec<Place> {
    let pos = |p: Option<&Value>| -> (u32, u32) { (p.and_then(|p| p.get("line")).and_then(Value::as_u64).unwrap_or(0) as u32, p.and_then(|p| p.get("character")).and_then(Value::as_u64).unwrap_or(0) as u32) };
    let one = |l: &Value| -> Option<Place> {
        // A Location (uri, range) or a LocationLink (targetUri, targetSelectionRange).
        let (u, r) = match l.get("targetUri") {
            Some(u) => (u, l.get("targetSelectionRange").or_else(|| l.get("targetRange"))?),
            None => (l.get("uri")?, l.get("range")?),
        };
        Some(Place { path: path_of(u.as_str()?)?, start: pos(r.get("start")), end: pos(r.get("end")) })
    };
    match v {
        Value::Array(a) => a.iter().filter_map(one).take(2000).collect(),
        Value::Object(_) => one(v).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Hover contents as plain text: MarkupContent, a MarkedString, or a list of them. Code fences
/// are dropped (their lines kept).
fn hover_text(v: &Value) -> String {
    fn piece(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Object(o) => o.get("value").and_then(Value::as_str).unwrap_or("").to_string(),
            Value::Array(a) => a.iter().map(piece).filter(|s| !s.trim().is_empty()).collect::<Vec<_>>().join("\n\n"),
            _ => String::new(),
        }
    }
    let raw = v.get("contents").map(piece).unwrap_or_default();
    let mut out: Vec<&str> = raw.lines().filter(|l| !l.trim_start().starts_with("```")).collect();
    while out.first().is_some_and(|l| l.trim().is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n").replace("\\_", "_").replace("&nbsp;", " ")
}

/// A completion item's text without snippet marks (`$1`, `${2:name}` → name).
fn strip_snippet(s: &str) -> String {
    let mut out = String::new();
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        match cs[i] {
            '\\' if i + 1 < cs.len() => {
                out.push(cs[i + 1]);
                i += 2;
            }
            '$' if i + 1 < cs.len() && cs[i + 1].is_ascii_digit() => {
                i += 1;
                while i < cs.len() && cs[i].is_ascii_digit() {
                    i += 1;
                }
            }
            '$' if i + 1 < cs.len() && cs[i + 1] == '{' => {
                // ${1:text} → text; ${1} → nothing.
                let mut j = i + 2;
                while j < cs.len() && cs[j].is_ascii_digit() {
                    j += 1;
                }
                let start = if j < cs.len() && cs[j] == ':' { j + 1 } else { j };
                let mut depth = 1;
                let mut k = start;
                while k < cs.len() {
                    if cs[k] == '{' {
                        depth += 1;
                    } else if cs[k] == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    k += 1;
                }
                out.push_str(&strip_snippet(&cs[start.min(k)..k].iter().collect::<String>()));
                i = k + 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn items_from(v: &Value) -> Vec<Item> {
    let list = match v {
        Value::Array(a) => a.clone(),
        Value::Object(_) => v.get("items").and_then(Value::as_array).cloned().unwrap_or_default(),
        _ => Vec::new(),
    };
    let pos = |p: &Value| -> Option<(u32, u32)> { Some((p.get("line")?.as_u64()? as u32, p.get("character")?.as_u64()? as u32)) };
    list.iter()
        .take(2000)
        .filter_map(|it| {
            let label = it.get("label")?.as_str()?.to_string();
            let snippet = it.get("insertTextFormat").and_then(Value::as_u64) == Some(2);
            let edit = it.get("textEdit");
            let (text, range) = match edit {
                Some(e) => {
                    let r = e.get("range").or_else(|| e.get("replace")).or_else(|| e.get("insert"));
                    let range = r.and_then(|r| Some((pos(r.get("start")?)?, pos(r.get("end")?)?)));
                    (e.get("newText").and_then(Value::as_str).unwrap_or(&label).to_string(), range)
                }
                None => (it.get("insertText").and_then(Value::as_str).unwrap_or(&label).to_string(), None),
            };
            let insert = if snippet { strip_snippet(&text) } else { text };
            let detail = it.get("detail").and_then(Value::as_str).or_else(|| it.get("labelDetails").and_then(|d| d.get("description")).and_then(Value::as_str)).unwrap_or("").lines().next().unwrap_or("").to_string();
            Some(Item {
                filter: it.get("filterText").and_then(Value::as_str).unwrap_or(&label).to_string(),
                sort: it.get("sortText").and_then(Value::as_str).unwrap_or(&label).to_string(),
                kind: it.get("kind").and_then(Value::as_u64).unwrap_or(1) as u32,
                label: label.trim().to_string(),
                detail,
                insert,
                range,
            })
        })
        .collect()
}

// ---------------------------------------------------------------- one server

enum Pending {
    Init,
    Completion(u64),
    Places(u64),
    Hover(u64),
}

pub struct Server {
    pub program: &'static str,
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    rx: Receiver<Option<Value>>,
    next_id: i64,
    pending: HashMap<i64, Pending>,
    /// Answered `initialize`.
    pub ready: bool,
    /// Ended (or never started properly): why.
    pub failed: Option<String>,
    pub utf8: bool,
    pub triggers: Vec<char>,
    /// Open documents by address.
    docs: HashMap<String, Open>,
    /// Sent before `initialize` was answered.
    queued: Vec<Value>,
}

fn send(stdin: &Arc<Mutex<ChildStdin>>, msg: &Value) -> bool {
    let body = msg.to_string();
    let Ok(mut w) = stdin.lock() else { return false };
    write!(w, "Content-Length: {}\r\n\r\n{body}", body.len()).and_then(|_| w.flush()).is_ok()
}

/// A document the server has open.
struct Open {
    version: i32,
    /// The edit last sent.
    sent: u64,
    /// The newest edit seen, and when.
    seen: u64,
    seen_at: Instant,
    /// When the first edit not sent yet was seen.
    waiting: Option<Instant>,
}

/// Typing sends the text once it pauses this long…
const QUIET: Duration = Duration::from_millis(250);
/// …or at least this often while it doesn't. Each send is the whole file, and the server
/// works through each one.
const MOST: Duration = Duration::from_secs(1);

impl Server {
    fn start(program: &'static str, args: &[&str], root: &Path, ctx: Option<eframe::egui::Context>) -> Result<Server, String> {
        let exe = find_program(program).ok_or_else(|| format!("{program} isn't installed"))?;
        let mut child = Command::new(exe)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("{program} couldn't start: {e}"))?;
        let stdin = Arc::new(Mutex::new(child.stdin.take().ok_or("no input")?));
        let stdout = child.stdout.take().ok_or("no output")?;
        let (tx, rx): (Sender<Option<Value>>, Receiver<Option<Value>>) = channel();
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            loop {
                let mut len = 0usize;
                loop {
                    let mut line = String::new();
                    match r.read_line(&mut line) {
                        Ok(0) | Err(_) => {
                            let _ = tx.send(None);
                            if let Some(c) = &ctx {
                                c.request_repaint();
                            }
                            return;
                        }
                        Ok(_) => {}
                    }
                    let l = line.trim();
                    if l.is_empty() {
                        break;
                    }
                    if let Some(v) = l.strip_prefix("Content-Length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut buf = vec![0u8; len];
                if r.read_exact(&mut buf).is_err() {
                    let _ = tx.send(None);
                    return;
                }
                if let Ok(v) = serde_json::from_slice::<Value>(&buf) {
                    if tx.send(Some(v)).is_err() {
                        return;
                    }
                    // Wake the window to take it in.
                    if let Some(c) = &ctx {
                        c.request_repaint();
                    }
                }
            }
        });
        let mut s = Server { program, child, stdin, rx, next_id: 1, pending: HashMap::new(), ready: false, failed: None, utf8: false, triggers: Vec::new(), docs: HashMap::new(), queued: Vec::new() };
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let params = json!({
            "processId": std::process::id(),
            "clientInfo": { "name": "Rivet Edit", "version": env!("CARGO_PKG_VERSION") },
            "rootUri": uri(root),
            "rootPath": root.to_string_lossy(),
            "workspaceFolders": [{ "uri": uri(root), "name": name }],
            "capabilities": {
                "general": { "positionEncodings": ["utf-8", "utf-16"] },
                "workspace": { "configuration": true, "workspaceFolders": true },
                "textDocument": {
                    "synchronization": { "didSave": true, "dynamicRegistration": false },
                    "completion": {
                        "completionItem": { "snippetSupport": false, "insertReplaceSupport": false, "labelDetailsSupport": true },
                        "contextSupport": true
                    },
                    "publishDiagnostics": { "relatedInformation": false },
                    "definition": { "linkSupport": true },
                    "references": {},
                    "hover": { "contentFormat": ["plaintext", "markdown"] }
                },
                "window": { "workDoneProgress": false }
            }
        });
        s.request_now("initialize", params, Pending::Init);
        Ok(s)
    }

    fn request_now(&mut self, method: &str, params: Value, kind: Pending) {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, kind);
        send(&self.stdin, &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
    }

    fn notify(&mut self, method: &str, params: Value) {
        let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        if self.ready {
            send(&self.stdin, &msg);
        } else {
            self.queued.push(msg);
        }
    }

    /// Give the server this document's latest text: `now` before a question about it, else once
    /// typing pauses. While it waits, how long until it's due.
    fn sync(&mut self, path: &Path, lang: Lang, text: &str, generation: u64, now: bool) -> Option<Duration> {
        let u = uri(path);
        let at = Instant::now();
        let Some(d) = self.docs.get_mut(&u) else {
            self.docs.insert(u.clone(), Open { version: 1, sent: generation, seen: generation, seen_at: at, waiting: None });
            self.notify("textDocument/didOpen", json!({ "textDocument": { "uri": u, "languageId": language_id(lang), "version": 1, "text": text } }));
            return None;
        };
        if d.sent == generation {
            d.waiting = None;
            return None;
        }
        if d.seen != generation {
            d.seen = generation;
            d.seen_at = at;
        }
        let waiting = *d.waiting.get_or_insert(at);
        let left = QUIET.saturating_sub(at - d.seen_at).min(MOST.saturating_sub(at - waiting));
        if !now && !left.is_zero() {
            return Some(left);
        }
        d.version += 1;
        d.sent = generation;
        d.waiting = None;
        let ver = d.version;
        self.notify("textDocument/didChange", json!({ "textDocument": { "uri": u, "version": ver }, "contentChanges": [{ "text": text }] }));
        None
    }

    /// Take in what the server sent; completions go to `out`, problems into `diags`.
    fn poll(&mut self, out: &mut Vec<Event>, diags: &mut HashMap<PathBuf, Vec<Diag>>) {
        while let Ok(msg) = self.rx.try_recv() {
            let Some(msg) = msg else {
                if self.failed.is_none() {
                    self.failed = Some(if self.ready { format!("{} stopped", self.program) } else { format!("{} couldn't start", self.program) });
                }
                self.ready = false;
                return;
            };
            let id = msg.get("id").cloned();
            match (msg.get("method").and_then(Value::as_str), id) {
                // A question from the server: answer it (blank answers are fine).
                (Some(method), Some(id)) => {
                    let result = if method == "workspace/configuration" {
                        let n = msg.pointer("/params/items").and_then(Value::as_array).map(|a| a.len()).unwrap_or(1);
                        Value::Array(vec![Value::Null; n])
                    } else if method == "workspace/workspaceFolders" {
                        Value::Array(Vec::new())
                    } else {
                        Value::Null
                    };
                    send(&self.stdin, &json!({ "jsonrpc": "2.0", "id": id, "result": result }));
                }
                (Some("textDocument/publishDiagnostics"), None) => {
                    let Some(p) = msg.pointer("/params/uri").and_then(Value::as_str).and_then(path_of) else { continue };
                    let pos = |v: Option<&Value>| -> (u32, u32) { (v.and_then(|v| v.get("line")).and_then(Value::as_u64).unwrap_or(0) as u32, v.and_then(|v| v.get("character")).and_then(Value::as_u64).unwrap_or(0) as u32) };
                    let list = msg.pointer("/params/diagnostics").and_then(Value::as_array).cloned().unwrap_or_default();
                    let ds = list
                        .iter()
                        .map(|d| Diag {
                            start: pos(d.pointer("/range/start")),
                            end: pos(d.pointer("/range/end")),
                            severity: d.get("severity").and_then(Value::as_u64).unwrap_or(1) as u8,
                            message: d.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
                        })
                        .collect();
                    diags.insert(p, ds);
                }
                (Some(_), None) => {}
                (None, Some(id)) => {
                    let Some(id) = id.as_i64() else { continue };
                    match self.pending.remove(&id) {
                        Some(Pending::Init) => {
                            let caps = msg.pointer("/result/capabilities").cloned().unwrap_or(Value::Null);
                            if msg.get("error").is_some() {
                                self.failed = Some(format!("{} refused to start", self.program));
                                continue;
                            }
                            self.utf8 = caps.get("positionEncoding").and_then(Value::as_str) == Some("utf-8");
                            self.triggers = caps.pointer("/completionProvider/triggerCharacters").and_then(Value::as_array).map(|a| a.iter().filter_map(|c| c.as_str()?.chars().next()).collect()).unwrap_or_default();
                            send(&self.stdin, &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
                            self.ready = true;
                            for m in std::mem::take(&mut self.queued) {
                                send(&self.stdin, &m);
                            }
                        }
                        Some(Pending::Completion(seq)) => {
                            let items = msg.get("result").map(items_from).unwrap_or_default();
                            out.push(Event::Completion { seq, items });
                        }
                        Some(Pending::Places(seq)) => {
                            let places = msg.get("result").map(places_from).unwrap_or_default();
                            out.push(Event::Places { seq, places });
                        }
                        Some(Pending::Hover(seq)) => {
                            let text = msg.get("result").map(hover_text).unwrap_or_default();
                            out.push(Event::Hover { seq, text });
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.ready {
            send(&self.stdin, &json!({ "jsonrpc": "2.0", "id": 0, "method": "shutdown" }));
            send(&self.stdin, &json!({ "jsonrpc": "2.0", "method": "exit" }));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------- every server

/// The running servers, one per (program, folder).
#[derive(Default)]
pub struct Lsp {
    servers: HashMap<(&'static str, PathBuf), Server>,
    /// Languages with no server installed (not looked for again).
    missing: HashSet<Lang>,
    /// Problems by file, as last reported.
    pub diags: HashMap<PathBuf, Vec<Diag>>,
    /// The window, woken when a server says something.
    pub ctx: Option<eframe::egui::Context>,
}

/// A file as a server sees it.
pub struct Doc<'a> {
    pub path: &'a Path,
    pub root: &'a Path,
    pub lang: Lang,
    pub text: &'a str,
    /// The text's edit number (`crate::text::Text::generation`).
    pub generation: u64,
}

/// The language server state for the status bar.
pub enum State {
    None(Lang),
    Starting(&'static str),
    Ready(&'static str),
    Failed(String),
}

impl Lsp {
    pub fn new(ctx: Option<eframe::egui::Context>) -> Lsp {
        Lsp { ctx, ..Default::default() }
    }

    fn key(&mut self, lang: Lang, root: &Path) -> Option<(&'static str, PathBuf)> {
        if self.missing.contains(&lang) {
            return None;
        }
        if let Some(k) = self.servers.keys().find(|(p, r)| r == root && candidates(lang).iter().any(|(c, _)| c == p)) {
            return Some(k.clone());
        }
        for (prog, args) in candidates(lang) {
            if find_program(prog).is_none() {
                continue;
            }
            if let Ok(s) = Server::start(prog, args, root, self.ctx.clone()) {
                let k = (*prog, root.to_path_buf());
                self.servers.insert(k.clone(), s);
                return Some(k);
            }
        }
        self.missing.insert(lang);
        None
    }

    /// Make sure a server knows the file (starting one when needed); its state.
    pub fn touch(&mut self, doc: Doc) -> State {
        let Doc { path, root, lang, text, generation } = doc;
        let Some(k) = self.key(lang, root) else { return State::None(lang) };
        let Some(s) = self.servers.get_mut(&k) else { return State::None(lang) };
        if let Some(f) = &s.failed {
            return State::Failed(f.clone());
        }
        if let Some(wait) = s.sync(path, lang, text, generation, false)
            && let Some(c) = &self.ctx
        {
            c.request_repaint_after(wait);
        }
        if s.ready { State::Ready(s.program) } else { State::Starting(s.program) }
    }

    /// The server's characters that open suggestions ("." "::" …).
    pub fn triggers(&self, lang: Lang, root: &Path) -> Vec<char> {
        self.servers.iter().find(|((p, r), _)| r == root && candidates(lang).iter().any(|(c, _)| c == p)).map(|(_, s)| s.triggers.clone()).unwrap_or_default()
    }

    fn server(&mut self, lang: Lang, root: &Path) -> Option<&mut Server> {
        self.servers.iter_mut().find(|((p, r), s)| r == root && s.failed.is_none() && candidates(lang).iter().any(|(c, _)| c == p)).map(|(_, s)| s)
    }

    /// Whether positions are counted in UTF-8 bytes for this language's server.
    pub fn utf8(&mut self, lang: Lang, root: &Path) -> bool {
        self.server(lang, root).map(|s| s.utf8).unwrap_or(false)
    }

    /// Ask for suggestions at `ch` (a character index); the answer comes back as `seq`.
    pub fn complete(&mut self, doc: Doc, ch: usize, trigger: Option<char>, seq: u64) -> bool {
        let Doc { path, root, lang, text, generation } = doc;
        let Some(s) = self.server(lang, root) else { return false };
        if !s.ready {
            return false;
        }
        s.sync(path, lang, text, generation, true);
        let (line, character) = to_pos(text, ch, s.utf8);
        let context = match trigger {
            Some(c) => json!({ "triggerKind": 2, "triggerCharacter": c.to_string() }),
            None => json!({ "triggerKind": 1 }),
        };
        s.request_now("textDocument/completion", json!({ "textDocument": { "uri": uri(path) }, "position": { "line": line, "character": character }, "context": context }), Pending::Completion(seq));
        true
    }

    /// Ask where the thing at `ch` is defined or used, or about it (hover); the answer comes
    /// back as `seq`. False when there's no server ready.
    pub fn ask(&mut self, doc: Doc, ch: usize, what: Ask, seq: u64) -> bool {
        let Doc { path, root, lang, text, generation } = doc;
        let Some(s) = self.server(lang, root) else { return false };
        if !s.ready {
            return false;
        }
        s.sync(path, lang, text, generation, true);
        let (line, character) = to_pos(text, ch, s.utf8);
        let mut params = json!({ "textDocument": { "uri": uri(path) }, "position": { "line": line, "character": character } });
        let (method, pending) = match what {
            Ask::Definition => ("textDocument/definition", Pending::Places(seq)),
            Ask::References => {
                params["context"] = json!({ "includeDeclaration": true });
                ("textDocument/references", Pending::Places(seq))
            }
            Ask::Hover => ("textDocument/hover", Pending::Hover(seq)),
        };
        s.request_now(method, params, pending);
        true
    }

    /// Whether a server for this language is running and ready.
    pub fn ready(&mut self, lang: Lang, root: &Path) -> bool {
        self.server(lang, root).is_some_and(|s| s.ready)
    }

    pub fn saved(&mut self, path: &Path) {
        let u = uri(path);
        for s in self.servers.values_mut() {
            if s.docs.contains_key(&u) {
                s.notify("textDocument/didSave", json!({ "textDocument": { "uri": u } }));
            }
        }
    }

    /// Tell the servers about files no longer open.
    pub fn close_others(&mut self, open: &HashSet<PathBuf>) {
        let keep: HashSet<String> = open.iter().map(|p| uri(p)).collect();
        for s in self.servers.values_mut() {
            let gone: Vec<String> = s.docs.keys().filter(|u| !keep.contains(*u)).cloned().collect();
            for u in gone {
                s.docs.remove(&u);
                s.notify("textDocument/didClose", json!({ "textDocument": { "uri": u } }));
            }
        }
        self.diags.retain(|p, _| open.contains(p));
    }

    /// Take in every server's messages.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        for s in self.servers.values_mut() {
            s.poll(&mut out, &mut self.diags);
        }
        out
    }
}

// ---------------------------------------------------------------- words of the file

/// Suggestions from the words already in the file (when there's no server, or it has none).
pub fn word_items(text: &str, prefix: &str, skip_at: usize) -> Vec<Item> {
    if prefix.chars().count() < 2 {
        return Vec::new();
    }
    let lower = prefix.to_lowercase();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut start = None;
    for (i, (b, c)) in text.char_indices().chain(std::iter::once((text.len(), ' '))).enumerate() {
        let w = c.is_alphanumeric() || c == '_';
        match (w, start) {
            (true, None) => start = Some((i, b)),
            (false, Some((si, sb))) => {
                start = None;
                let word = &text[sb..b];
                // Not the word being typed.
                if si <= skip_at && skip_at <= i {
                    continue;
                }
                if word.len() > prefix.len() && word.to_lowercase().starts_with(&lower) && !word.chars().next().is_some_and(|c| c.is_ascii_digit()) && seen.insert(word) {
                    out.push(Item { label: word.into(), detail: String::new(), kind: 0, insert: word.into(), filter: word.into(), sort: word.into(), range: None });
                    if out.len() >= 200 {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// How well `filter` matches what's typed: None when it doesn't (letters in order, any case).
pub fn score(filter: &str, typed: &str) -> Option<i32> {
    if typed.is_empty() {
        return Some(0);
    }
    let f: Vec<char> = filter.chars().flat_map(char::to_lowercase).collect();
    let t: Vec<char> = typed.chars().flat_map(char::to_lowercase).collect();
    if f.starts_with(&t) {
        return Some(1000 - f.len() as i32 + if filter.starts_with(typed) { 50 } else { 0 });
    }
    let mut j = 0;
    let mut gaps = 0;
    for c in &f {
        if j < t.len() && *c == t[j] {
            j += 1;
        } else if j > 0 && j < t.len() {
            gaps += 1;
        }
    }
    (j == t.len() && t.first() == f.first()).then_some(500 - gaps - f.len() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_and_addresses() {
        let t = "ab\nçd😀x\n";
        assert_eq!(to_pos(t, 7, false), (1, 5));
        assert_eq!(to_pos(t, 7, true), (1, 8));
        assert_eq!(from_pos(t, 1, 5, false), 7);
        assert_eq!(from_pos(t, 1, 8, true), 7);
        assert_eq!(from_pos(t, 0, 99, false), 2);
        let p = Path::new("/home/a b/ça.rs");
        assert_eq!(uri(p), "file:///home/a%20b/%C3%A7a.rs");
        assert_eq!(path_of(&uri(p)).unwrap(), p);
    }

    #[test]
    fn places_and_hover() {
        let p = places_from(&json!([{ "uri": "file:///a%20b/x.rs", "range": { "start": { "line": 3, "character": 4 }, "end": { "line": 3, "character": 9 } } }]));
        assert_eq!(p, vec![Place { path: PathBuf::from("/a b/x.rs"), start: (3, 4), end: (3, 9) }]);
        let l = places_from(&json!([{ "targetUri": "file:///y.c", "targetRange": { "start": { "line": 0, "character": 0 } }, "targetSelectionRange": { "start": { "line": 2, "character": 7 }, "end": { "line": 2, "character": 9 } } }]));
        assert_eq!(l[0].start, (2, 7));
        assert_eq!(places_from(&Value::Null), vec![]);
        assert_eq!(hover_text(&json!({ "contents": { "kind": "markdown", "value": "```rust\nfn len(&self) -> usize\n```\n\nThe length." } })), "fn len(&self) -> usize\n\nThe length.");
        assert_eq!(hover_text(&json!({ "contents": [{ "language": "c", "value": "int x" }, "a value"] })), "int x\n\na value");
    }

    #[test]
    fn snippets_words_and_scores() {
        assert_eq!(strip_snippet("push(${1:value})$0"), "push(value)");
        assert_eq!(strip_snippet("fn ${1:name}(${2})"), "fn name()");
        let w = word_items("counts count_words cou counts", "cou", 21);
        assert_eq!(w.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), vec!["counts", "count_words"]);
        assert!(score("count_words", "cw").is_some());
        assert!(score("counts", "xo").is_none());
        assert!(score("counts", "cou").unwrap() > score("count_words", "cou").unwrap());
        let items = items_from(&json!({ "items": [{ "label": "len", "kind": 2, "detail": "fn(&self) -> usize", "insertText": "len()", "insertTextFormat": 1 }] }));
        assert_eq!((items[0].insert.as_str(), items[0].kind), ("len()", 2));
    }
}

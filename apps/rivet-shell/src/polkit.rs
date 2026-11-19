//! Asking for a password when an app needs an administrator's say-so (installing software,
//! changing system settings, `pkexec`): Rivet Shell registers as the session's polkit
//! authentication agent, so those requests open the shell's own dialog. When another agent is
//! already registered for the session, or there is no polkit, the shell leaves it be.
//!
//! polkit calls `BeginAuthentication` and waits for the reply until the person has been
//! checked. The checking is done by polkit's own helper (`polkit-agent-helper-1`, or its socket
//! where the distribution runs it that way): the shell hands it the request's cookie, passes
//! PAM's questions and the person's answers back and forth, and the helper tells polkit the
//! verdict. The shell never sees whether a password was right beyond SUCCESS or FAILURE.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

const AUTHORITY: &str = "org.freedesktop.PolicyKit1";
const AUTHORITY_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const AUTHORITY_IFACE: &str = "org.freedesktop.PolicyKit1.Authority";
const AGENT_IFACE: &str = "org.freedesktop.PolicyKit1.AuthenticationAgent";
const AGENT_PATH: &str = "/org/rivet/Shell/PolkitAgent";
const HELPER_SOCKET: &str = "/run/polkit/agent-helper.socket";
/// Where distributions put the helper.
const HELPERS: [&str; 7] = [
    "/usr/lib/polkit-1/polkit-agent-helper-1",
    "/usr/libexec/polkit-agent-helper-1",
    "/usr/libexec/polkit-1/polkit-agent-helper-1",
    "/usr/lib64/polkit-1/polkit-agent-helper-1",
    "/usr/lib/policykit-1/polkit-agent-helper-1",
    "/usr/lib/x86_64-linux-gnu/polkit-1/polkit-agent-helper-1",
    "/run/wrappers/bin/polkit-agent-helper-1",
];

/// What the agent's threads tell the shell (through `Agent::on_event`).
pub enum Event {
    Begin(Request),
    /// polkit no longer wants this one answered (the app went away, or it timed out).
    Cancel(String),
    /// From the helper of try number `.0`.
    Helper(u64, Said),
}

/// One thing asked of the person.
pub struct Request {
    cookie: String,
    message: String,
    /// Whose password will do, by login name.
    users: Vec<String>,
    /// polkit's call, answered once the person is checked or has said no.
    call: zbus::Message,
}

/// A line from the helper.
#[derive(Clone, Debug, PartialEq)]
pub enum Said {
    /// PAM asks for something: its words, and whether what's typed may show.
    Prompt(String, bool),
    Info(String),
    Error(String),
    Done(bool),
}

/// What the dialog shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    /// What polkit says is being asked for.
    pub message: String,
    /// Whose password, when it isn't the person's own.
    pub other_user: Option<String>,
    /// What to type ("Password"); None until the helper asks.
    pub prompt: Option<String>,
    /// Whether what's typed may show.
    pub echo: bool,
    /// A line under the field: (text, is it a problem).
    pub note: Option<(String, bool)>,
    /// Waiting on the helper.
    pub busy: bool,
    /// Goes up when the field should empty (a new request, or another try).
    pub round: u64,
}

/// The helper for one try: where answers go, and how to stop it.
struct Session {
    input: Box<dyn Write + Send>,
    child: Option<Child>,
    sock: Option<UnixStream>,
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(s) = &self.sock {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            // Collected off this thread: the helper may take a moment to go.
            std::thread::spawn(move || {
                let _ = c.wait();
            });
        }
    }
}

struct Current {
    req: Request,
    user: String,
    session: Option<Session>,
    view: View,
}

pub struct Agent {
    conn: Connection,
    send: std::sync::Arc<dyn Fn(Event) + Send + Sync>,
    queue: VecDeque<Request>,
    current: Option<Current>,
    /// Counts tries, so a line from a helper that's been replaced is dropped.
    tries: u64,
}

impl Agent {
    /// Register as this session's agent. `send` gets each event (from other threads); hand
    /// them back through `on_event` on the shell's own thread.
    pub fn start(send: impl Fn(Event) + Send + Sync + 'static) -> Result<Agent, String> {
        let conn = Connection::system().map_err(|e| format!("no system bus ({e})"))?;
        let session = session_id(&conn).ok_or("couldn't tell which login session this is")?;
        let send: std::sync::Arc<dyn Fn(Event) + Send + Sync> = std::sync::Arc::new(send);
        // Listening before registering, so the first request isn't missed.
        let calls = zbus::blocking::MessageIterator::from(&conn);
        let subject = ("unix-session", HashMap::from([("session-id", Value::from(session.as_str()))]));
        let locale = std::env::var("LANG").ok().filter(|l| !l.is_empty()).unwrap_or_else(|| "en_US.UTF-8".into());
        conn.call_method(Some(AUTHORITY), AUTHORITY_PATH, Some(AUTHORITY_IFACE), "RegisterAuthenticationAgent", &(subject, locale, AGENT_PATH)).map_err(|e| match &e {
            zbus::Error::MethodError(_, Some(why), _) => why.clone(),
            _ => e.to_string(),
        })?;
        let (c, s) = (conn.clone(), send.clone());
        std::thread::Builder::new().name("rivet-shell polkit".into()).spawn(move || listen(&c, calls, &*s)).map_err(|e| e.to_string())?;
        Ok(Agent { conn, send, queue: VecDeque::new(), current: None, tries: 0 })
    }

    /// What the dialog shows now (None: nothing is being asked).
    pub fn view(&self) -> Option<View> {
        self.current.as_ref().map(|c| c.view.clone())
    }

    /// Take an event in. True when what the dialog shows changed.
    pub fn on_event(&mut self, e: Event) -> bool {
        match e {
            Event::Begin(r) => {
                self.queue.push_back(r);
                self.next()
            }
            Event::Cancel(cookie) => {
                self.queue.retain(|r| {
                    let keep = r.cookie != cookie;
                    if !keep {
                        refuse(&self.conn, &r.call);
                    }
                    keep
                });
                if self.current.as_ref().is_some_and(|c| c.req.cookie == cookie) {
                    self.cancel();
                    return true;
                }
                false
            }
            Event::Helper(n, said) => {
                if n != self.tries {
                    return false;
                }
                let Some(c) = self.current.as_mut() else { return false };
                match said {
                    Said::Prompt(text, echo) => {
                        c.view.prompt = Some(text.trim().trim_end_matches(':').trim().to_string());
                        c.view.echo = echo;
                        c.view.busy = false;
                    }
                    Said::Info(t) => c.view.note = Some((t, false)),
                    Said::Error(t) => c.view.note = Some((t, true)),
                    Said::Done(true) => {
                        // The helper has told polkit; the call's reply lets polkit carry on.
                        if let Some(c) = self.current.take() {
                            let _ = self.conn.reply(&c.req.call.header(), &());
                        }
                        self.next();
                    }
                    Said::Done(false) => {
                        // Turned away: ask again, saying so.
                        self.begin_try(Some("That password wasn't right. Try again."));
                    }
                }
                true
            }
        }
    }

    /// The person's answer to what the helper asked.
    pub fn answer(&mut self, text: &str) {
        let Some(c) = self.current.as_mut() else { return };
        let Some(s) = c.session.as_mut() else { return };
        let mut line = text.as_bytes().to_vec();
        line.push(b'\n');
        let sent = s.input.write_all(&line).and_then(|_| s.input.flush());
        line.iter_mut().for_each(|b| *b = 0);
        c.view.busy = true;
        c.view.note = None;
        if sent.is_err() {
            self.begin_try(Some("Couldn't check the password. Try again."));
        }
    }

    /// The person said no (or polkit took the request back): polkit is told, and the next
    /// request, if one waits, is shown.
    pub fn cancel(&mut self) {
        if let Some(c) = self.current.take() {
            refuse(&self.conn, &c.req.call);
        }
        self.next();
    }

    /// Show the next request waiting, when none is showing.
    fn next(&mut self) -> bool {
        if self.current.is_some() {
            return false;
        }
        let Some(req) = self.queue.pop_front() else { return false };
        let me = crate::auth::login_name();
        // The person's own password when it will do, else the first administrator's.
        let user = if req.users.contains(&me) { me.clone() } else { req.users.first().cloned().unwrap_or_default() };
        if user.is_empty() {
            refuse(&self.conn, &req.call);
            return self.next();
        }
        let view = View { message: req.message.clone(), other_user: (user != me).then(|| user.clone()), busy: true, ..View::default() };
        self.current = Some(Current { req, user, session: None, view });
        self.begin_try(None);
        true
    }

    /// Start the helper for the current request (again, after a wrong password).
    fn begin_try(&mut self, note: Option<&str>) {
        self.tries += 1;
        let n = self.tries;
        let Some(c) = self.current.as_mut() else { return };
        c.session = None;
        c.view.round = n;
        c.view.prompt = None;
        c.view.busy = true;
        c.view.note = note.map(|t| (t.to_string(), true));
        let send = self.send.clone();
        match start_helper(&c.user, &c.req.cookie, move |said| send(Event::Helper(n, said))) {
            Ok(s) => c.session = Some(s),
            Err(e) => {
                eprintln!("rivet-shell: password prompt: {e}");
                self.cancel();
            }
        }
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        if let Some(c) = self.current.take() {
            refuse(&self.conn, &c.req.call);
        }
        for r in self.queue.drain(..) {
            refuse(&self.conn, &r.call);
        }
    }
}

/// Answer polkit's call with "cancelled".
fn refuse(conn: &Connection, call: &zbus::Message) {
    let _ = conn.reply_error(&call.header(), "org.freedesktop.PolicyKit1.Error.Cancelled", &"Cancelled");
}

/// The login session to answer for: the one this process is in, else (started as a service,
/// outside any session) the person's graphical session.
fn session_id(conn: &Connection) -> Option<String> {
    if let Some(id) = std::env::var("XDG_SESSION_ID").ok().filter(|s| !s.is_empty()) {
        return Some(id);
    }
    let m = conn.call_method(Some("org.freedesktop.login1"), "/org/freedesktop/login1/user/self", Some("org.freedesktop.DBus.Properties"), "Get", &("org.freedesktop.login1.User", "Display")).ok()?;
    let v: OwnedValue = m.body().deserialize().ok()?;
    let (id, _path): (String, zbus::zvariant::OwnedObjectPath) = v.try_into().ok()?;
    Some(id).filter(|s| !s.is_empty())
}

type BeginBody = (String, String, String, HashMap<String, String>, String, Vec<(String, HashMap<String, OwnedValue>)>);

/// polkit's calls, on a thread: each becomes an event. Only polkit itself is listened to, so
/// no other program can put a password dialog on the screen.
fn listen(conn: &Connection, calls: zbus::blocking::MessageIterator, send: &(dyn Fn(Event) + Send + Sync)) {
    for msg in calls {
        let Ok(msg) = msg else { break };
        let h = msg.header();
        if h.message_type() != zbus::message::Type::MethodCall || h.interface().map(|i| i.as_str()) != Some(AGENT_IFACE) {
            continue;
        }
        let from = h.sender().map(|s| s.to_string()).unwrap_or_default();
        let polkit: String = conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetNameOwner", &(AUTHORITY,)).ok().and_then(|m| m.body().deserialize().ok()).unwrap_or_default();
        if from.is_empty() || from != polkit {
            let _ = conn.reply_error(&h, "org.freedesktop.DBus.Error.AccessDenied", &"Only polkit may ask");
            continue;
        }
        match h.member().map(|m| m.as_str()) {
            Some("BeginAuthentication") => match msg.body().deserialize::<BeginBody>() {
                Ok((_action, message, _icon, _details, cookie, identities)) => {
                    let users = identities.iter().filter(|(kind, _)| kind == "unix-user").filter_map(|(_, d)| u32::try_from(d.get("uid")?.clone()).ok()).filter_map(user_name).collect();
                    send(Event::Begin(Request { cookie, message, users, call: msg.clone() }));
                }
                Err(e) => {
                    let _ = conn.reply_error(&h, "org.freedesktop.DBus.Error.InvalidArgs", &e.to_string());
                }
            },
            Some("CancelAuthentication") => {
                if let Ok(cookie) = msg.body().deserialize::<String>() {
                    send(Event::Cancel(cookie));
                }
                let _ = conn.reply(&h, &());
            }
            _ => {
                let _ = conn.reply_error(&h, "org.freedesktop.DBus.Error.UnknownMethod", &"No such method");
            }
        }
    }
}

fn user_name(uid: u32) -> Option<String> {
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut out: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: `pw` and `buf` outlive the call, and the name is copied out before they go.
    let r = unsafe { libc::getpwuid_r(uid, &mut pw, buf.as_mut_ptr(), buf.len(), &mut out) };
    if r != 0 || out.is_null() || pw.pw_name.is_null() {
        return None;
    }
    Some(unsafe { std::ffi::CStr::from_ptr(pw.pw_name) }.to_string_lossy().into_owned())
}

/// Start polkit's helper for `user` and `cookie`: through its socket where there is one, else
/// the program. Each thing it says goes to `said`, from a thread.
fn start_helper(user: &str, cookie: &str, said: impl Fn(Said) + Send + 'static) -> Result<Session, String> {
    let (mut session, output): (Session, Box<dyn std::io::Read + Send>) = match Path::new(HELPER_SOCKET).exists().then(|| UnixStream::connect(HELPER_SOCKET)).and_then(Result::ok) {
        Some(sock) => {
            let (input, output) = (sock.try_clone().map_err(|e| e.to_string())?, sock.try_clone().map_err(|e| e.to_string())?);
            let mut s = Session { input: Box::new(input), child: None, sock: Some(sock) };
            s.input.write_all(format!("{user}\n").as_bytes()).map_err(|e| e.to_string())?;
            (s, Box::new(output))
        }
        None => {
            let helper = HELPERS.iter().find(|p| Path::new(p).exists()).ok_or("polkit's helper program isn't installed")?;
            let mut child = Command::new(helper).arg(user).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("couldn't start {helper}: {e}"))?;
            let (input, output) = (child.stdin.take().ok_or("no way to answer the helper")?, child.stdout.take().ok_or("no way to hear the helper")?);
            (Session { input: Box::new(input), child: Some(child), sock: None }, Box::new(output))
        }
    };
    session.input.write_all(format!("{cookie}\n").as_bytes()).and_then(|_| session.input.flush()).map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("rivet-shell polkit helper".into())
        .spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if let Some(s) = parse(&line) {
                    let done = matches!(s, Said::Done(_));
                    said(s);
                    if done {
                        return;
                    }
                }
            }
            // Gone without a verdict.
            said(Said::Done(false));
        })
        .map_err(|e| e.to_string())?;
    Ok(session)
}

/// One of the helper's lines.
fn parse(line: &str) -> Option<Said> {
    let line = unescape(line);
    let rest = |prefix: &str| line.strip_prefix(prefix).map(|s| s.trim().to_string());
    if let Some(t) = rest("PAM_PROMPT_ECHO_OFF ") {
        Some(Said::Prompt(t, false))
    } else if let Some(t) = rest("PAM_PROMPT_ECHO_ON ") {
        Some(Said::Prompt(t, true))
    } else if let Some(t) = rest("PAM_ERROR_MSG ") {
        Some(Said::Error(t))
    } else if let Some(t) = rest("PAM_TEXT_INFO ") {
        Some(Said::Info(t))
    } else if line.starts_with("SUCCESS") {
        Some(Said::Done(true))
    } else if line.starts_with("FAILURE") {
        Some(Said::Done(false))
    } else {
        None
    }
}

/// The helper escapes its lines as C strings (`\n`, `\"`, `\303`…); this is the way back.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' || i + 1 >= b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        match b[i] {
            b'0'..=b'7' => {
                let mut v = 0u32;
                let mut n = 0;
                while n < 3 && i < b.len() && (b'0'..=b'7').contains(&b[i]) {
                    v = v * 8 + (b[i] - b'0') as u32;
                    i += 1;
                    n += 1;
                }
                out.push(v as u8);
                continue;
            }
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A made-up request, for pictures.
pub fn demo() -> View {
    View { message: "Authentication is required to install software".into(), prompt: Some("Password".into()), round: 1, ..View::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_helpers_lines() {
        assert_eq!(parse("PAM_PROMPT_ECHO_OFF Password: "), Some(Said::Prompt("Password:".into(), false)));
        assert_eq!(parse("PAM_PROMPT_ECHO_ON login:"), Some(Said::Prompt("login:".into(), true)));
        assert_eq!(parse("PAM_TEXT_INFO Place your finger on the reader"), Some(Said::Info("Place your finger on the reader".into())));
        assert_eq!(parse("PAM_ERROR_MSG No"), Some(Said::Error("No".into())));
        assert_eq!(parse("SUCCESS"), Some(Said::Done(true)));
        assert_eq!(parse("FAILURE"), Some(Said::Done(false)));
        assert_eq!(parse("something else"), None);
    }

    #[test]
    fn undoes_the_helpers_escapes() {
        assert_eq!(unescape(r#"Mot de passe\302\240: \"x\"\\"#), "Mot de passe\u{a0}: \"x\"\\");
        assert_eq!(unescape(r"a\nb\tc"), "a\nb\tc");
    }

    /// Registers as this session's agent for real, has polkit ask (with `pkcheck`), and stops
    /// once the helper asks for the password, so no wrong password is ever tried:
    /// `cargo test -- --ignored polkit`. Fails where another agent is running.
    #[test]
    #[ignore]
    fn live() {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let mut agent = Agent::start(move |e| {
            let _ = tx.lock().unwrap().send(e);
        })
        .expect("registered");
        let me = std::process::id().to_string();
        let mut check = Command::new("pkcheck").args(["--action-id", "org.freedesktop.policykit.exec", "--process", &me, "--allow-user-interaction"]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("pkcheck");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut asked = false;
        while std::time::Instant::now() < until && !asked {
            if let Ok(e) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
                agent.on_event(e);
            }
            asked = agent.view().is_some_and(|v| v.prompt.is_some());
        }
        let v = agent.view().expect("polkit asked");
        assert!(asked, "the helper asked for something: {v:?}");
        assert!(!v.message.is_empty());
        assert!(!v.echo);
        agent.cancel();
        assert!(agent.view().is_none());
        // Cancelled, so pkcheck comes back saying no.
        assert!(!check.wait().expect("pkcheck ended").success());
    }
}

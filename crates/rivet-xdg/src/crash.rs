//! Crash reports, kept on this computer until the person chooses to share one.
//!
//! When an app panics (a bug), a plain-text report is written to
//! `~/.local/state/<app>/crashes/crash-YYYYMMDD-HHMMSS.txt`: the version, what went wrong and
//! where, the stack, and the system. Anything that could say who the person is (their home
//! folder, user name, computer name, their own files and the servers they use) is taken out
//! before it's written, so what they see is exactly what they'd share. Nothing is sent anywhere:
//! the next start shows the report once (`rivet_look::crash`), and sharing it opens a GitHub
//! issue, filled in, for the person to check and send from their own account.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// The app a report is for.
#[derive(Clone, Copy, Debug)]
pub struct App {
    /// "Rivet Notes"
    pub name: &'static str,
    /// The issue form and label it's filed under: "notes" uses `crash-notes.yml`.
    pub id: &'static str,
    /// Its folder under the state folder: "rivet-notes" (Rivet Files' is "rivet").
    pub state: &'static str,
    pub version: &'static str,
    /// The project's GitHub page (`env!("CARGO_PKG_REPOSITORY")`).
    pub repo: &'static str,
}

/// How many reports to keep (older ones are removed when a new one is written).
const KEEP: usize = 20;

/// Set for screenshot and measuring runs: a check that panics prints to the terminal but never
/// writes into the person's own crash reports (RIVET_CRASH_TEST still writes, to test them).
pub static NO_WRITE: AtomicBool = AtomicBool::new(false);

static APP: OnceLock<App> = OnceLock::new();

pub fn dir(app: &App) -> PathBuf {
    crate::places::state_home().join(app.state).join("crashes")
}

/// Write a report whenever the app panics, then carry on as before (the message still goes to
/// the terminal).
pub fn install(app: App) {
    let _ = APP.set(app);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let what = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".into());
        let place = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "unknown".into());
        let stack = std::backtrace::Backtrace::force_capture();
        if let Some(app) = APP.get()
            && (!NO_WRITE.load(Ordering::Relaxed) || std::env::var_os("RIVET_CRASH_TEST").is_some())
        {
            let _ = write_report(app, &thread, &what, &place, &trim_stack(&stack.to_string()));
        }
        previous(info);
    }));
}

/// The stack from where the panic was raised: the panic machinery's own frames dropped, and at
/// most 40 frames kept.
fn trim_stack(stack: &str) -> String {
    let lines: Vec<&str> = stack.lines().collect();
    // Frames are "  N: name" lines, each followed by "at file:line" lines.
    let is_frame = |l: &str| l.trim_start().split_once(':').is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()));
    let machinery = |l: &str| {
        ["panicking", "begin_panic", "panic_fmt", "rust_begin_unwind", "rust_end_short_backtrace", "crash::install", "alloc::boxed::Box<dyn", "backtrace::Backtrace"]
            .iter()
            .any(|m| l.contains(m))
    };
    // The top of the stack is the panic machinery (and this hook); start at the first frame after it.
    let mut start = 0;
    if lines.iter().take_while(|l| !is_frame(l) || machinery(l)).any(|l| is_frame(l)) {
        start = lines.iter().position(|l| is_frame(l) && !machinery(l)).unwrap_or(0);
    }
    let mut out = Vec::new();
    let mut frames = 0;
    for l in &lines[start..] {
        if is_frame(l) {
            frames += 1;
            if frames > 40 {
                out.push("   …");
                break;
            }
        }
        out.push(l);
    }
    out.join("\n")
}

fn write_report(app: &App, thread: &str, what: &str, place: &str, stack: &str) -> std::io::Result<PathBuf> {
    let d = dir(app);
    std::fs::create_dir_all(&d)?;
    let (stamp, time) = now();
    let mut path = d.join(format!("crash-{stamp}.txt"));
    // Two panics in one second (say, two threads): keep both.
    let mut n = 2;
    while path.exists() {
        path = d.join(format!("crash-{stamp}-{n}.txt"));
        n += 1;
    }
    let text = format!(
        "{name} {version} crash report\n\
         Time:    {time}\n\
         Thread:  {thread}\n\
         Where:   {place}\n\
         What:    {what}\n\
         System:  {system}\n\
         Session: {session}\n\
         Package: {package}\n\
         \n\
         Stack:\n{stack}\n",
        name = app.name,
        version = app.version,
        system = system(),
        session = session(),
        package = package(),
    );
    std::fs::write(&path, scrub(&text, &Private::here()))?;
    prune(app);
    Ok(path)
}

/// ("20261120-142501", "2026-11-20 14:25:01") in local time.
fn now() -> (String, String) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as libc::time_t).unwrap_or(0);
    // SAFETY: localtime_r only writes into the tm it's given.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    let (y, mo, d, h, mi, s) = (tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec);
    (format!("{y}{mo:02}{d:02}-{h:02}{mi:02}{s:02}"), format!("{y}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}"))
}

/// "Fedora Linux 44 (Workstation Edition), Linux 7.2.15, x86_64"
fn system() -> String {
    let os = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())))
        .unwrap_or_else(|| "Linux".into());
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").map(|k| k.trim().to_string()).unwrap_or_default();
    format!("{os}, Linux {kernel}, {}", std::env::consts::ARCH)
}

/// "wayland, niri": which display system and desktop.
fn session() -> String {
    let kind = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown desktop".into());
    format!("{kind}, {desktop}")
}

/// How the app was installed: Flatpak, AppImage or a plain binary.
fn package() -> &'static str {
    if crate::places::in_flatpak() {
        "Flatpak"
    } else if std::env::var_os("APPDIR")
        // An AppImage that started the app passes APPDIR on; only count it when the app runs from inside.
        .is_some_and(|d| std::env::current_exe().is_ok_and(|e| e.starts_with(&d)))
    {
        "AppImage"
    } else {
        "binary"
    }
}

/// What would say who someone is: taken out of every report.
struct Private {
    home: String,
    words: Vec<(String, &'static str)>,
}

impl Private {
    fn here() -> Self {
        let mut words = Vec::new();
        let user = std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).unwrap_or_default();
        words.push((user, "user"));
        words.push((hostname(), "computer"));
        // Longest first, so a computer named after its owner ("sam-laptop") goes as a whole.
        words.sort_by_key(|(w, _)| std::cmp::Reverse(w.len()));
        Private { home: crate::places::home().to_string_lossy().into_owned(), words }
    }
}

fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most buf.len() bytes into buf.
    if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
        return String::new();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// The report with anything personal replaced: file paths outside the system's own folders
/// become `<path>`, web and server addresses `<address>`, and the user and computer names
/// `<user>` and `<computer>`. Places in the apps' own code and the libraries they use stay, as
/// they're what makes a report useful.
fn scrub(text: &str, p: &Private) -> String {
    // Where the apps were built (on the builder's computer): file names in the stack start with it.
    let built = env!("CARGO_MANIFEST_DIR").trim_end_matches("/crates/rivet-xdg");
    let mut t = text.replace(&format!("{built}/"), "");
    if !p.home.is_empty() && p.home != "/" {
        t = t.replace(&p.home, "~");
    }
    t = t.lines().map(scrub_line).collect::<Vec<_>>().join("\n");
    if text.ends_with('\n') {
        t.push('\n');
    }
    for (word, stand_in) in &p.words {
        if word.len() >= 3 {
            t = replace_word(&t, word, &format!("<{stand_in}>"));
        }
    }
    t
}

/// Libraries the apps are built from: kept from the crate's own folder on.
fn library_path(path: &str) -> Option<String> {
    for at in ["/registry/src/", "/git/checkouts/"] {
        if let Some(i) = path.find(at) {
            let rest = &path[i + at.len()..];
            // Past the index's folder ("index.crates.io-1949cf8c6b5b557f/").
            return rest.split_once('/').map(|(_, r)| r.to_string());
        }
    }
    path.strip_prefix("/rustc/").map(|_| path.to_string())
}

fn scrub_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let boundary = |c: char| c.is_whitespace() || "\"'`()[]{}<>=,;".contains(c);
    while i < chars.len() {
        let starts = i == 0 || boundary(chars[i - 1]) || chars[i - 1] == ':';
        let rest: String = chars[i..].iter().collect();
        let path = starts && (rest.starts_with('/') || rest.starts_with("~/"));
        let address = starts && rest.split(|c: char| boundary(c)).next().is_some_and(|w| w.contains("://") || (w.contains('@') && w.contains('.')));
        if !path && !address {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // The token, carried across spaces in folder names ("~/My Notes/a.md") while what follows
        // the space still looks like part of a path.
        let mut end = i;
        loop {
            while end < chars.len() && !boundary(chars[end]) {
                end += 1;
            }
            let next: String = chars[end..].iter().skip(1).take_while(|c| !boundary(**c)).collect();
            let next = next.trim_end_matches([':', '.', ',']);
            // More folders, or the rest of a file's name ("diary 2026.md").
            let more = next.contains('/') && !next.starts_with('/') || next.contains('.');
            if path && end < chars.len() && chars[end] == ' ' && more {
                end += 1;
            } else {
                break;
            }
        }
        // A colon or full stop after the path is the sentence's, not the path's.
        while end > i + 1 && matches!(chars[end - 1], ':' | '.' | ',') {
            end -= 1;
        }
        let token: String = chars[i..end].iter().collect();
        // Line and column after a source file ("src/x.rs:12:5") aren't private.
        let (file, place) = match token.find(".rs:") {
            Some(at) => token.split_at(at + 3),
            None => (token.as_str(), ""),
        };
        if address {
            out.push_str("<address>");
        } else if let Some(lib) = library_path(file) {
            out.push_str(&lib);
            out.push_str(place);
        } else if ["/usr/", "/proc/", "/sys/", "/dev/", "/etc/", "/lib", "/bin/"].iter().any(|s| file.starts_with(s)) && !file.contains(".rs") {
            out.push_str(&token);
        } else {
            out.push_str("<path>");
            out.push_str(place);
        }
        i = end;
    }
    out
}

/// Replaces `word` where it stands alone (not inside a longer word).
fn replace_word(text: &str, word: &str, with: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    let alnum = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    while let Some(at) = rest.find(word) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + word.len()..].chars().next();
        out.push_str(&rest[..at]);
        out.push_str(if alnum(before) || alnum(after) { word } else { with });
        rest = &rest[at + word.len()..];
    }
    out.push_str(rest);
    out
}

/// The saved reports, newest first.
pub fn reports(app: &App) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir(app))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt")))
        .collect();
    v.sort();
    v.reverse();
    v
}

fn prune(app: &App) {
    for old in reports(app).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(old);
    }
}

/// The newest report not shown yet, if any.
pub fn unseen(app: &App) -> Option<Report> {
    let newest = reports(app).into_iter().next()?;
    let name = newest.file_name()?.to_string_lossy().to_string();
    let seen = std::fs::read_to_string(dir(app).join("seen")).unwrap_or_default();
    if seen.trim() == name {
        return None;
    }
    Report::load(&newest)
}

/// Remember that this report has been shown, so it isn't shown again.
pub fn mark_seen(app: &App, report: &Report) {
    if let Some(name) = report.path.file_name() {
        let _ = std::fs::write(dir(app).join("seen"), name.to_string_lossy().as_bytes());
    }
}

pub struct Report {
    pub path: PathBuf,
    pub text: String,
    /// The window's own thread stopped (the app closed), rather than a background task.
    pub closed: bool,
    /// Where in the code it happened ("apps/rivet-notes/src/gui.rs:120:9"), for an issue title.
    /// (The message itself can contain names, so it stays out of the title.)
    pub place: String,
}

impl Report {
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        let field = |k: &str| text.lines().find_map(|l| l.strip_prefix(k).map(|v| v.trim().to_string())).unwrap_or_default();
        let closed = field("Thread:") == "main";
        let place = field("Where:");
        Some(Self { path: path.to_path_buf(), text, closed, place })
    }

    /// A new GitHub issue with this report filled in, on the app's crash form (which labels it
    /// with the app), for the person to look over and send. A report too long for a link is cut
    /// at the stack; `full` says whether it all fit.
    pub fn issue_url(&self, app: &App) -> (String, bool) {
        let title = format!("Crash in {} {} at {}", app.name, app.version, self.place);
        // Browsers and GitHub turn away links much past 8,000 characters.
        const ROOM: usize = 7000;
        let mut body = self.text.clone();
        let mut full = true;
        while encode(&body).len() > ROOM {
            full = false;
            let keep = body.lines().count().saturating_sub(4).max(8);
            body = body.lines().take(keep).collect::<Vec<_>>().join("\n");
            if keep == 8 {
                break;
            }
        }
        if !full {
            body.push_str("\n   … (the rest was cut to fit; paste the full report if you can)");
        }
        let url = format!("{}/issues/new?template=crash-{}.yml&title={}&version={}&report={}", app.repo.trim_end_matches('/'), app.id, encode(&title), encode(app.version), encode(&body));
        (url, full)
    }
}

/// Percent-encoding for a link's query.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_starts_at_app_code() {
        let stack = "   0: rivet::crash::install::{closure#0}\n             at src/crash.rs:30:21\n   1: std::panicking::panic_with_hook\n   2: core::panicking::panic_fmt\n   3: rivet::app::RivetApp::ui\n             at src/app.rs:10:5\n   4: std::panicking::try\n   5: main";
        let t = trim_stack(stack);
        assert!(t.starts_with("   3: rivet::app::RivetApp::ui"), "{t}");
        // Frames further down that mention panicking (Rust's own startup) are kept.
        assert!(t.contains("std::panicking::try"));
    }

    #[test]
    fn personal_details_are_taken_out() {
        let p = Private { home: "/home/sam".into(), words: vec![("sam".into(), "user"), ("sams-laptop".into(), "computer")] };
        let text = "What:    couldn't open /home/sam/My Notes/diary 2026.md: not found\n\
                    What:    no route to sftp://sam@example.org:22 for sam@example.org\n\
                    Session: wayland, niri on sams-laptop; samples kept\n\
                    \x20            at /home/sam/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-0.37.0/src/ui.rs:12:5\n\
                    \x20            at /rustc/abc123/library/core/src/option.rs:2010:5\n\
                    \x20            at apps/rivet-notes/src/gui.rs:120:9\n\
                    \x20            at /media/sam/Disk/x.txt and /usr/share/fonts/a.ttf\n";
        let t = scrub(text, &p);
        assert!(!t.contains("sam/") && !t.contains("diary") && !t.contains("Notes") && !t.contains("example.org") && !t.contains("laptop"), "{t}");
        assert!(t.contains("couldn't open <path>: not found"), "{t}");
        assert!(t.contains("no route to <address> for <address>"), "{t}");
        assert!(t.contains("niri on <computer>; samples kept"), "{t}");
        assert!(t.contains("at egui-0.37.0/src/ui.rs:12:5"), "{t}");
        assert!(t.contains("at /rustc/abc123/library/core/src/option.rs:2010:5"), "{t}");
        assert!(t.contains("at apps/rivet-notes/src/gui.rs:120:9"), "{t}");
        assert!(t.contains("at <path> and /usr/share/fonts/a.ttf"), "{t}");
    }

    #[test]
    fn issue_link_fits() {
        let app = App { name: "Rivet Notes", id: "notes", state: "rivet-notes", version: "0.1.0", repo: "https://github.com/o/r" };
        let stack: String = (0..400).map(|i| format!("  {i}: some::frame::name\n             at apps/x/src/y.rs:{i}:1\n")).collect();
        let r = Report { path: PathBuf::new(), text: format!("Rivet Notes 0.1.0 crash report\nWhere:   apps/x/src/y.rs:1:1\n\nStack:\n{stack}"), closed: true, place: "apps/x/src/y.rs:1:1".into() };
        let (url, full) = r.issue_url(&app);
        assert!(!full && url.len() < 7600, "{}", url.len());
        assert!(url.starts_with("https://github.com/o/r/issues/new?template=crash-notes.yml&title=Crash%20in%20Rivet%20Notes%200.1.0%20at%20apps%2Fx%2Fsrc%2Fy.rs%3A1%3A1&version=0.1.0&report="));
    }
}

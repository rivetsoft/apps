//! Rivet Shell as a greeter for greetd: the lock screen's face on the login screen, over the
//! wallpaper the person chose last (`rivet-shell greeter`, drawn by lock.rs).
//!
//! greetd runs its greeter as its own user, which can't read people's home folders. So each
//! person's shell leaves what the greeter needs in a shared folder, `/var/lib/rivet-greeter`,
//! made once by `sudo rivet-shell greeter --setup`: a copy of the wallpaper, the look (accent,
//! style, clock), the name, the picture and the session to start. The folder is like /tmp
//! (anyone may add a folder, only its owner may change it), and the greeter only trusts a
//! person's folder when that person owns it. With no folder, the shell leaves nothing.
//!
//! Signing in speaks greetd's protocol over $GREETD_SOCK: JSON messages, each after its length.

use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::Settings;

/// The shared folder (RIVET_GREETER_DIR names another, for checks and trying it out).
pub const DIR: &str = "/var/lib/rivet-greeter";

pub fn dir() -> PathBuf {
    std::env::var_os("RIVET_GREETER_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DIR))
}

/// The settings the greeter takes from a person: how the lock screen looks, nothing private.
const LOOK_KEYS: &[&str] = &["theme", "accent", "secondary", "tertiary", "color_use", "text_size", "font", "clock_format", "lock_blur", "lock_power", "surface_dark", "surface_light", "surface_tint"];

// ------------------------------------------------------------------ the shell's side

/// Leave this person's wallpaper, look, name, picture and session for the greeter, on a thread
/// (the picture takes a moment). `colors`: the accent, secondary and tertiary as shown now, so
/// colours made from the wallpaper reach the greeter too. Nothing happens without the folder.
pub fn publish(s: &Settings, colors: [[u8; 3]; 3]) {
    let root = dir();
    if !root.is_dir() {
        return;
    }
    let login = crate::auth::login_name();
    if login.is_empty() {
        return;
    }
    let mut look = s.clone();
    look.follow_desktop_colors = false;
    look.palette.clear();
    look.accent = colors[0];
    look.secondary = Some(colors[1]);
    look.tertiary = Some(colors[2]);
    let text: String = look.text().lines().filter(|l| l.split_once('=').is_some_and(|(k, _)| LOOK_KEYS.contains(&k))).map(|l| format!("{l}\n")).collect();
    let wallpaper = PathBuf::from(s.matugen_picture());
    std::thread::spawn(move || {
        let dir = root.join(&login);
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
        let write = |name: &str, body: &[u8]| {
            // Through a temporary file, so the greeter never reads half of one.
            let tmp = dir.join(format!(".{name}.new"));
            if std::fs::write(&tmp, body).is_ok() {
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644));
                let _ = std::fs::rename(&tmp, dir.join(name));
            }
        };
        write("look", text.as_bytes());
        write("name", crate::sys::user_name().as_bytes());
        if let Some(cmd) = current_session() {
            write("session", cmd.as_bytes());
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        if let Some(face) = [home.join(".face"), home.join(".face.icon")].iter().find(|p| p.is_file()).and_then(|p| small_png(p, 256, 256)) {
            write("face.png", &face);
        }
        // The wallpaper last: its time is when this person last changed it.
        if wallpaper.is_file()
            && let Some(png) = small_png(&wallpaper, 2560, 1600)
        {
            write("wallpaper.png", &png);
        }
    });
}

/// A picture made no bigger than `w` by `h`, as PNG.
fn small_png(path: &Path, w: u32, h: u32) -> Option<Vec<u8>> {
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let img = if img.width() > w || img.height() > h { img.resize(w, h, image::imageops::FilterType::Triangle) } else { img };
    let mut out = std::io::Cursor::new(Vec::new());
    img.to_rgb8().write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// The installed Wayland sessions: (desktop names, the command), from their .desktop files.
fn sessions() -> Vec<(Vec<String>, String)> {
    let mut files: Vec<PathBuf> = ["/usr/share/wayland-sessions", "/usr/local/share/wayland-sessions"].iter().filter_map(|d| std::fs::read_dir(d).ok()).flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "desktop")).collect();
    files.sort();
    files
        .iter()
        .filter_map(|p| {
            let text = std::fs::read_to_string(p).ok()?;
            let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(k)?.strip_prefix('=').map(str::trim).map(String::from));
            let exec = get("Exec")?;
            let names = get("DesktopNames").unwrap_or_default().split(';').filter(|x| !x.is_empty()).map(String::from).collect();
            Some((names, exec))
        })
        .collect()
}

/// The command of the session running now, by its desktop name.
fn current_session() -> Option<String> {
    let now = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let now: Vec<&str> = now.split(':').filter(|x| !x.is_empty()).collect();
    sessions().into_iter().find(|(names, _)| names.iter().any(|n| now.iter().any(|c| c.eq_ignore_ascii_case(n)))).map(|(_, exec)| exec)
}

// ------------------------------------------------------------------ the greeter's side

/// Someone who can sign in, with what their shell left for the greeter.
pub struct Person {
    pub login: String,
    pub name: String,
    pub settings: Settings,
    pub wallpaper: Option<PathBuf>,
    pub face: Option<PathBuf>,
    pub session: Vec<String>,
    /// When the wallpaper was last left: the newest comes first.
    changed: Option<SystemTime>,
}

/// The people who can sign in (accounts with a home and a real shell), the one who changed
/// their wallpaper last first.
pub fn people() -> Vec<Person> {
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let fallback = sessions().into_iter().next().map(|(_, e)| e).unwrap_or_default();
    let mut out: Vec<Person> = passwd
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            let (login, uid, gecos, shell) = (*f.first()?, f.get(2)?.parse::<u32>().ok()?, *f.get(4)?, *f.get(6)?);
            if !(1000..60000).contains(&uid) || shell.ends_with("nologin") || shell.ends_with("false") {
                return None;
            }
            let dir = dir().join(login);
            // Only a folder its owner made counts: anyone could make one under another's name.
            let own = std::fs::metadata(&dir).is_ok_and(|m| m.is_dir() && m.uid() == uid);
            let file = |n: &str| Some(dir.join(n)).filter(|p| own && std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.uid() == uid));
            let read = |n: &str| file(n).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
            let wallpaper = file("wallpaper.png");
            let changed = wallpaper.as_ref().and_then(|p| std::fs::metadata(p).ok()?.modified().ok());
            let name = Some(read("name").trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| gecos.split(',').next().filter(|g| !g.trim().is_empty()).unwrap_or(login).trim().to_string());
            let session = Some(read("session").trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| fallback.clone());
            Some(Person { login: login.to_string(), name, settings: Settings::from_text(&read("look")), wallpaper, face: file("face.png"), session: command(&session), changed })
        })
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.changed));
    out
}

/// A .desktop Exec line as a command: its field codes (%f, %U…) left out.
fn command(exec: &str) -> Vec<String> {
    exec.split_whitespace().filter(|w| !(w.len() == 2 && w.starts_with('%'))).map(String::from).collect()
}

fn send(sock: &mut UnixStream, msg: &serde_json::Value) -> Result<serde_json::Value, String> {
    let body = msg.to_string();
    sock.write_all(&(body.len() as u32).to_ne_bytes()).and_then(|_| sock.write_all(body.as_bytes())).map_err(|e| format!("greetd stopped answering ({e})"))?;
    let mut len = [0u8; 4];
    sock.read_exact(&mut len).map_err(|e| format!("greetd stopped answering ({e})"))?;
    let mut buf = vec![0u8; u32::from_ne_bytes(len) as usize];
    sock.read_exact(&mut buf).map_err(|e| format!("greetd stopped answering ({e})"))?;
    serde_json::from_slice(&buf).map_err(|e| format!("greetd's answer didn't make sense ({e})"))
}

/// Sign in as `login` with `password`, then have greetd start `session`. On success greetd
/// starts it when the greeter leaves; on failure the attempt is cancelled, ready for another.
pub fn sign_in(login: &str, password: &str, session: &[String]) -> Result<(), String> {
    let path = std::env::var("GREETD_SOCK").map_err(|_| "Not started by greetd, so there's no one to sign in with".to_string())?;
    sign_in_at(Path::new(&path), login, password, session)
}

fn sign_in_at(path: &Path, login: &str, password: &str, session: &[String]) -> Result<(), String> {
    use serde_json::json;
    let mut sock = UnixStream::connect(path).map_err(|e| format!("Can't reach greetd ({e})"))?;
    let mut reply = send(&mut sock, &json!({ "type": "create_session", "username": login }))?;
    let mut answered = false;
    loop {
        match reply["type"].as_str() {
            Some("success") => break,
            Some("auth_message") => {
                // The password once; information and errors are acknowledged with no answer.
                let secret = matches!(reply["auth_message_type"].as_str(), Some("secret" | "visible"));
                let response = if secret && !answered {
                    answered = true;
                    json!(password)
                } else if secret {
                    // A second question (a code, say) this screen can't ask.
                    let _ = send(&mut sock, &json!({ "type": "cancel_session" }));
                    return Err("This account asks for more than a password".into());
                } else {
                    serde_json::Value::Null
                };
                reply = send(&mut sock, &json!({ "type": "post_auth_message_response", "response": response }))?;
            }
            _ => {
                let _ = send(&mut sock, &json!({ "type": "cancel_session" }));
                return Err(if reply["error_type"].as_str() == Some("auth_error") { "Wrong password".into() } else { reply["description"].as_str().unwrap_or("greetd refused").to_string() });
            }
        }
    }
    if session.is_empty() {
        let _ = send(&mut sock, &json!({ "type": "cancel_session" }));
        return Err("No session to start: install a Wayland desktop".into());
    }
    let reply = send(&mut sock, &json!({ "type": "start_session", "cmd": session, "env": [] }))?;
    if reply["type"].as_str() == Some("success") { Ok(()) } else { Err(reply["description"].as_str().unwrap_or("greetd couldn't start the session").to_string()) }
}

// ------------------------------------------------------------------ setting it up

/// `sudo rivet-shell greeter --setup`: make the shared folder, and say how to use the greeter.
pub fn setup() -> std::process::ExitCode {
    let root = dir();
    let root = root.as_path();
    if let Err(e) = std::fs::create_dir_all(root).and_then(|_| std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o1777))) {
        eprintln!("Couldn't make {} ({e}). Run this with sudo.", root.display());
        return std::process::ExitCode::FAILURE;
    }
    let me = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "rivet-shell".into());
    println!(
        "Made {}. Each person's Rivet Shell now leaves its wallpaper and look there.\n\n\
         To use the greeter, set greetd's default session in /etc/greetd/config.toml, with a\n\
         compositor to show it in. With cage:\n\n\
         [default_session]\n\
         command = \"cage -s -- {me} greeter\"\n\
         user = \"greeter\"\n\n\
         Or with Sway, a file with `exec \"{me} greeter; swaymsg exit\"` in it, run as\n\
         `sway --config FILE`. Then change your wallpaper (or restart Rivet Shell) so it leaves\n\
         yours, and restart greetd.",
        root.display()
    );
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// A greetd that wants `password` and answers each message as it would.
    fn fake_greetd(path: &Path, password: &'static str) -> std::thread::JoinHandle<Vec<String>> {
        let listener = UnixListener::bind(path).unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            loop {
                let mut len = [0u8; 4];
                if s.read_exact(&mut len).is_err() {
                    return seen;
                }
                let mut buf = vec![0u8; u32::from_ne_bytes(len) as usize];
                s.read_exact(&mut buf).unwrap();
                let m: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                let kind = m["type"].as_str().unwrap().to_string();
                let reply = match kind.as_str() {
                    "create_session" => serde_json::json!({ "type": "auth_message", "auth_message_type": "secret", "auth_message": "Password:" }),
                    "post_auth_message_response" if m["response"] == password => serde_json::json!({ "type": "success" }),
                    "post_auth_message_response" => serde_json::json!({ "type": "error", "error_type": "auth_error", "description": "pam" }),
                    _ => serde_json::json!({ "type": "success" }),
                };
                seen.push(kind);
                let body = reply.to_string();
                s.write_all(&(body.len() as u32).to_ne_bytes()).unwrap();
                s.write_all(body.as_bytes()).unwrap();
            }
        })
    }

    #[test]
    fn signs_in_and_starts_the_session() {
        let dir = std::env::temp_dir().join(format!("rivet-greetd-ok-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let sock = dir.join("sock");
        let server = fake_greetd(&sock, "right");
        assert_eq!(sign_in_at(&sock, "sam", "right", &["niri-session".into()]), Ok(()));
        assert_eq!(server.join().unwrap(), ["create_session", "post_auth_message_response", "start_session"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wrong_password_cancels() {
        let dir = std::env::temp_dir().join(format!("rivet-greetd-bad-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let sock = dir.join("sock");
        let server = fake_greetd(&sock, "right");
        assert_eq!(sign_in_at(&sock, "sam", "wrong", &["niri-session".into()]), Err("Wrong password".into()));
        assert_eq!(server.join().unwrap(), ["create_session", "post_auth_message_response", "cancel_session"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The shell leaves this person's wallpaper and look, and the greeter finds them. Ignored:
    /// it needs the person running it to have a normal account (CI may run as root).
    #[test]
    #[ignore]
    fn publish_then_find() {
        let dir = std::env::temp_dir().join(format!("rivet-greeter-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wall = crate::wall::demo_png(&dir).unwrap();
        // SAFETY: no other test reads this variable.
        unsafe { std::env::set_var("RIVET_GREETER_DIR", &dir) };
        let s = Settings { wallpaper: wall.to_string_lossy().into(), clock_format: "%H.%M".into(), ..Settings::default() };
        publish(&s, [[200, 40, 120], [0, 132, 184], [255, 242, 112]]);
        let login = crate::auth::login_name();
        let left = dir.join(&login).join("wallpaper.png");
        for _ in 0..100 {
            if left.is_file() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let me = people().into_iter().find(|p| p.login == login).expect("this account");
        assert_eq!(me.wallpaper.as_deref(), Some(left.as_path()));
        assert_eq!((me.settings.accent, me.settings.secondary, me.settings.clock_format.as_str()), ([200, 40, 120], Some([0, 132, 184]), "%H.%M"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exec_lines_lose_field_codes() {
        assert_eq!(command("Hyprland %U --x"), ["Hyprland", "--x"]);
    }
}

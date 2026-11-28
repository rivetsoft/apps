//! The folder each person's session leaves what the login screen needs in.
//!
//! greetd runs its greeter as its own user, which can't read people's home folders. So each
//! person's session leaves what the greeter needs in a shared folder, `/var/lib/rivet-greeter`,
//! made once by `sudo rivet-greeter --setup`: a copy of the wallpaper, the look (accent, style,
//! clock), the name, the picture and the session to start, in a folder named for their login.
//! The folder is like /tmp (anyone may add a folder, only its owner may change it), and the
//! greeter only trusts a person's folder, and the files in it, when that person owns them. With
//! no shared folder, nothing is left.
//!
//! Rivet Shell leaves them whenever the wallpaper or its colors change; `rivet-greeter sync`
//! does it from any other desktop.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::{Look, Person};

/// The shared folder (RIVET_GREETER_DIR names another, for checks and trying it out).
pub const DIR: &str = "/var/lib/rivet-greeter";

pub fn dir() -> PathBuf {
    std::env::var_os("RIVET_GREETER_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DIR))
}

// ------------------------------------------------------------------ the session's side

/// Leave this person's look, name, picture and session for the greeter, and a copy of the
/// wallpaper when there is one. Blocks while the pictures are made smaller (a moment), so
/// call it on a thread. Err says why nothing was left.
pub fn publish(look: &Look, name: &str, wallpaper: Option<&Path>) -> Result<(), String> {
    let root = dir();
    if !root.is_dir() {
        return Err(format!("There's no {} yet: run sudo rivet-greeter --setup first", root.display()));
    }
    let login = crate::login_name();
    if login.is_empty() {
        return Err("Couldn't tell who is signed in".into());
    }
    let dir = root.join(&login);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't make {} ({e})", dir.display()))?;
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
    let write = |name: &str, body: &[u8]| -> Result<(), String> {
        // Through a temporary file, so the greeter never reads half of one.
        let tmp = dir.join(format!(".{name}.new"));
        std::fs::write(&tmp, body).map_err(|e| format!("Couldn't write {} ({e})", tmp.display()))?;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644));
        std::fs::rename(&tmp, dir.join(name)).map_err(|e| format!("Couldn't write {name} ({e})"))
    };
    write("look", look.text().as_bytes())?;
    write("name", name.as_bytes())?;
    if let Some(cmd) = current_session() {
        write("session", cmd.as_bytes())?;
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if let Some(face) = [home.join(".face"), home.join(".face.icon")].iter().find(|p| p.is_file()).and_then(|p| small_png(p, 256, 256)) {
        write("face.png", &face)?;
    }
    // The wallpaper last: its time is when this person last changed it.
    if let Some(w) = wallpaper.filter(|w| w.is_file()) {
        let png = small_png(w, 2560, 1600).ok_or_else(|| format!("Couldn't open {} as a picture", w.display()))?;
        write("wallpaper.png", &png)?;
    }
    Ok(())
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

/// The full name of who's running this, from the account (else the login).
pub fn full_name() -> String {
    let login = crate::login_name();
    let full = std::fs::read_to_string("/etc/passwd").ok().and_then(|t| t.lines().find(|l| l.split(':').next() == Some(login.as_str())).and_then(|l| l.split(':').nth(4).map(|g| g.split(',').next().unwrap_or("").trim().to_string())));
    full.filter(|f| !f.is_empty()).unwrap_or(login)
}

// ------------------------------------------------------------------ the greeter's side

/// The people who can sign in (accounts with a home and a real shell), the one who changed
/// their wallpaper last first.
pub fn people() -> Vec<Person> {
    people_in(&std::fs::read_to_string("/etc/passwd").unwrap_or_default(), &dir())
}

fn people_in(passwd: &str, root: &Path) -> Vec<Person> {
    let fallback = sessions().into_iter().next().map(|(_, e)| e).unwrap_or_default();
    let mut out: Vec<(Person, Option<SystemTime>)> = passwd
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            let (login, uid, gecos, shell) = (*f.first()?, f.get(2)?.parse::<u32>().ok()?, *f.get(4)?, *f.get(6)?);
            if !(1000..60000).contains(&uid) || shell.ends_with("nologin") || shell.ends_with("false") {
                return None;
            }
            let dir = root.join(login);
            // Only a folder its owner made counts: anyone could make one under another's name.
            let own = std::fs::metadata(&dir).is_ok_and(|m| m.is_dir() && m.uid() == uid);
            let file = |n: &str| Some(dir.join(n)).filter(|p| own && std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.uid() == uid));
            let read = |n: &str| file(n).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
            let wallpaper = file("wallpaper.png");
            let changed = wallpaper.as_ref().and_then(|p| std::fs::metadata(p).ok()?.modified().ok());
            let name = Some(read("name").trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| gecos.split(',').next().filter(|g| !g.trim().is_empty()).unwrap_or(login).trim().to_string());
            let session = Some(read("session").trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| fallback.clone());
            let faces = file("face.png").into_iter().chain([crate::accounts_face(login)]).collect();
            Some((Person { login: login.to_string(), name, look: Look::from_text(&read("look")), wallpaper, faces, session: command(&session) }, changed))
        })
        .collect();
    out.sort_by_key(|(_, changed)| std::cmp::Reverse(*changed));
    out.into_iter().map(|(p, _)| p).collect()
}

/// A .desktop Exec line as a command: its field codes (%f, %U…) left out.
fn command(exec: &str) -> Vec<String> {
    exec.split_whitespace().filter(|w| !(w.len() == 2 && w.starts_with('%'))).map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> (String, u32) {
        (crate::login_name(), unsafe { libc::getuid() })
    }

    /// The session leaves this person's wallpaper and look, and the greeter finds them. Skipped
    /// as root (CI may run as root, and root has no account the greeter lists).
    #[test]
    fn publish_then_find() {
        let (login, uid) = me();
        if uid == 0 {
            return;
        }
        let dir = std::env::temp_dir().join(format!("rivet-greeter-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wall = crate::demo_wallpaper(&dir).unwrap();
        // SAFETY: no other test reads this variable.
        unsafe { std::env::set_var("RIVET_GREETER_DIR", &dir) };
        let look = Look { accent: [200, 40, 120], secondary: Some([0, 132, 184]), clock_format: "%H.%M".into(), ..Look::default() };
        publish(&look, "Sam Rivers", Some(&wall)).unwrap();
        let passwd = format!("{login}:x:{uid}:{uid}:Someone Else:/home/{login}:/bin/bash\nnobody:x:65534:65534::/:/sbin/nologin\n");
        let people = people_in(&passwd, &dir);
        assert_eq!(people.len(), 1);
        let p = &people[0];
        assert_eq!(p.wallpaper.as_deref(), Some(dir.join(&login).join("wallpaper.png").as_path()));
        assert_eq!((p.name.as_str(), p.look.accent, p.look.secondary, p.look.clock_format.as_str()), ("Sam Rivers", [200, 40, 120], Some([0, 132, 184]), "%H.%M"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder under someone's name that someone else owns is not trusted: their account is
    /// listed with nothing from it.
    #[test]
    fn a_folder_someone_else_owns_is_ignored() {
        let (_, uid) = me();
        let dir = std::env::temp_dir().join(format!("rivet-greeter-other-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sam")).unwrap();
        std::fs::write(dir.join("sam/name"), "Not Sam").unwrap();
        std::fs::write(dir.join("sam/look"), "accent=#ff0000\n").unwrap();
        // The folder is this account's, so for an account with any other uid it isn't trusted.
        let other = if uid == 1000 { 1001 } else { 1000 };
        let people = people_in(&format!("sam:x:{other}:{other}:Sam Rivers,,,:/home/sam:/bin/zsh\n"), &dir);
        assert_eq!(people.len(), 1);
        assert_eq!((people[0].name.as_str(), people[0].look.accent, people[0].wallpaper.is_none()), ("Sam Rivers", Look::default().accent, true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exec_lines_lose_field_codes() {
        assert_eq!(command("Hyprland %U --x"), ["Hyprland", "--x"]);
    }
}

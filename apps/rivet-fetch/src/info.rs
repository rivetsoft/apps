//! What rivetfetch knows about the computer: one function per module, each reading the kernel's
//! files or asking a program, and each returning nothing when it can't find out.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One line beside the logo.
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    /// Empty for lines with no key (the title, the separator, the colours).
    pub key: String,
    pub value: String,
    /// A percentage shown after the value, coloured by how full it is.
    pub percent: Option<f64>,
    /// Text after the percentage.
    pub after: Option<String>,
}

impl Fact {
    fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Fact { key: key.into(), value: value.into(), percent: None, after: None }
    }
}

/// Every module, in the order shown when the settings don't choose.
pub const MODULES: [(&str, &str); 24] = [
    ("title", "user and host name"),
    ("separator", "a line under the title"),
    ("os", "the system's name and version"),
    ("host", "the computer's model"),
    ("kernel", "the kernel's version"),
    ("uptime", "how long since the computer started"),
    ("packages", "installed packages, by package manager"),
    ("shell", "the shell rivetfetch runs in"),
    ("display", "each screen's size and refresh rate"),
    ("wm", "the window manager or desktop"),
    ("theme", "the GTK theme"),
    ("icons", "the icon theme"),
    ("font", "the interface font"),
    ("cursor", "the cursor theme"),
    ("terminal", "the terminal rivetfetch runs in"),
    ("cpu", "the processor"),
    ("gpu", "the graphics cards"),
    ("memory", "memory in use"),
    ("swap", "swap in use"),
    ("disk", "space used on the root drive"),
    ("battery", "the battery's charge"),
    ("locale", "the language setting"),
    ("rivet", "which Rivet apps are installed"),
    ("colors", "the terminal's 16 colours"),
];

/// What's shown when the settings don't choose: the essentials. The rest is a `modules=` away.
pub const DEFAULT: &str = "os,host,kernel,uptime,packages,shell,wm,cpu,gpu,memory";

pub fn known(name: &str) -> bool {
    name == "break" || MODULES.iter().any(|(n, _)| *n == name)
}

/// The facts of one module. `colors` and `separator` are drawn by the caller.
pub fn gather(module: &str, gtk: &Gtk) -> Vec<Fact> {
    if demo_mode() {
        return demo(module);
    }
    let one = |key: &str, v: Option<String>| v.map(|v| vec![Fact::new(key, v)]).unwrap_or_default();
    match module {
        "title" => vec![Fact::new("", title())],
        "os" => one("OS", os()),
        "host" => one("Host", host()),
        "kernel" => one("Kernel", kernel()),
        "uptime" => one("Uptime", uptime()),
        "packages" => one("Packages", packages()),
        "shell" => one("Shell", shell()),
        "display" => display(),
        "wm" => one("WM", wm()),
        "theme" => one("Theme", gtk.theme.clone()),
        "icons" => one("Icons", gtk.icons.clone()),
        "font" => one("Font", gtk.font.clone()),
        "cursor" => one("Cursor", gtk.cursor.clone()),
        "terminal" => one("Terminal", terminal()),
        "cpu" => one("CPU", cpu()),
        "gpu" => gpu().into_iter().map(|g| Fact::new("GPU", g)).collect(),
        "memory" => meminfo("MemTotal", "MemAvailable").map(|(u, t)| vec![used("Memory", u, t)]).unwrap_or_default(),
        "swap" => meminfo("SwapTotal", "SwapFree").filter(|(_, t)| *t > 0).map(|(u, t)| vec![used("Swap", u, t)]).unwrap_or_default(),
        "disk" => disk(),
        "battery" => battery(),
        "locale" => one("Locale", ["LC_ALL", "LANG"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))),
        "rivet" => one("Rivet", rivet()),
        _ => Vec::new(),
    }
}

/// Checks and pictures: RIVET_FETCH_DEMO=1 shows the facts of a made-up computer instead of this
/// one's, so a screenshot shows nobody's machine.
pub fn demo_mode() -> bool {
    std::env::var_os("RIVET_FETCH_DEMO").is_some_and(|v| !v.is_empty())
}

fn demo(module: &str) -> Vec<Fact> {
    const GIB: u64 = 1 << 30;
    let one = |key: &str, v: &str| vec![Fact::new(key, v)];
    match module {
        "title" => vec![Fact::new("", title())],
        "os" => one("OS", "Arch Linux x86_64"),
        "host" => one("Host", "Laptop 13"),
        "kernel" => one("Kernel", "Linux 6.17.8-arch1-1"),
        "uptime" => one("Uptime", "3 hours, 12 mins"),
        "packages" => one("Packages", "1184 (pacman), 12 (flatpak)"),
        "shell" => one("Shell", "bash 5.3.3"),
        "display" => one("Display (eDP-1)", "2880x1920 in 14\", 120 Hz [Built-in]"),
        "wm" => one("WM", "softcore (Wayland)"),
        "theme" => one("Theme", "adw-gtk3-dark"),
        "icons" => one("Icons", "Papirus-Dark"),
        "font" => one("Font", "Inter (11pt)"),
        "cursor" => one("Cursor", "Bibata-Modern-Classic"),
        "terminal" => one("Terminal", "rivet-terminal"),
        "cpu" => one("CPU", "AMD Ryzen 7 7840U (16) @ 5.13 GHz"),
        "gpu" => one("GPU", "AMD Radeon 780M [Integrated]"),
        "memory" => vec![used("Memory", 7_344_394_240, 32_943_521_792)],
        "swap" => vec![used("Swap", 0, 8 * GIB)],
        "disk" => vec![Fact { after: Some(" - btrfs".into()), ..used("Disk (/)", 201_234_567_168, 998_052_462_592) }],
        "battery" => vec![Fact { key: "Battery".into(), value: "[Discharging]".into(), percent: Some(86.0), after: None }],
        "locale" => one("Locale", "en_US.UTF-8"),
        "rivet" => one("Rivet", "Files, Shell, Edit, Terminal, Notes, FTP, Snip (Shell running)"),
        _ => Vec::new(),
    }
}

fn used(key: &str, used: u64, total: u64) -> Fact {
    Fact { key: key.into(), value: format!("{} / {}", bytes(used), bytes(total)), percent: Some(used as f64 * 100.0 / total.max(1) as f64), after: None }
}

pub fn bytes(b: u64) -> String {
    if b == 0 {
        return "0 B".into();
    }
    let gib = b as f64 / (1u64 << 30) as f64;
    if gib >= 1.0 { format!("{gib:.2} GiB") } else { format!("{:.2} MiB", b as f64 / (1u64 << 20) as f64) }
}

fn read(path: impl AsRef<Path>) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn uname() -> Option<libc::utsname> {
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    (unsafe { libc::uname(&mut u) } == 0).then_some(u)
}

fn field(f: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = f.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).to_string()
}

pub fn user_name() -> String {
    if demo_mode() {
        return "rivet".into();
    }
    std::env::var("USER").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| {
        let pw = unsafe { libc::getpwuid(libc::getuid()) };
        if pw.is_null() { "user".into() } else { unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }.to_string_lossy().to_string() }
    })
}

pub fn host_name() -> String {
    if demo_mode() {
        return "demo".into();
    }
    uname().map(|u| field(&u.nodename)).filter(|h| !h.is_empty()).unwrap_or_else(|| "localhost".into())
}

fn title() -> String {
    format!("{}@{}", user_name(), host_name())
}

fn os() -> Option<String> {
    let text = read("/etc/os-release").or_else(|| read("/usr/lib/os-release"))?;
    let get = |k: &str| {
        text.lines().find_map(|l| l.strip_prefix(k)?.strip_prefix('=').map(|v| v.trim_matches(['"', '\'']).to_string())).filter(|v| !v.is_empty())
    };
    let name = get("PRETTY_NAME").or_else(|| get("NAME"))?;
    let arch = uname().map(|u| field(&u.machine)).unwrap_or_default();
    Some(format!("{name} {arch}").trim().to_string())
}

fn host() -> Option<String> {
    // Placeholders firmware makers leave in.
    const JUNK: [&str; 6] = ["to be filled", "system product name", "default string", "not applicable", "none", "o.e.m"];
    let good = |s: &String| !JUNK.iter().any(|j| s.to_lowercase().contains(j));
    let dmi = Path::new("/sys/class/dmi/id");
    if let Some(name) = read(dmi.join("product_name")).filter(good) {
        // Some makers put the model in the version instead (Lenovo's "ThinkPad X1 Carbon").
        let version = read(dmi.join("product_version")).filter(good).filter(|v| v.chars().any(|c| c.is_alphabetic()) && !name.contains(v.as_str()));
        return Some(match version {
            Some(v) if v.len() > name.len() => v,
            _ => name,
        });
    }
    read("/sys/firmware/devicetree/base/model").map(|m| m.trim_end_matches('\0').to_string())
}

fn kernel() -> Option<String> {
    let u = uname()?;
    Some(format!("{} {}", field(&u.sysname), field(&u.release)))
}

fn uptime() -> Option<String> {
    let secs: f64 = read("/proc/uptime")?.split_whitespace().next()?.parse().ok()?;
    Some(duration(secs as u64))
}

pub fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, secs / 3600 % 24, secs / 60 % 60);
    let unit = |n: u64, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if d > 0 {
        parts.push(unit(d, "day"));
    }
    if h > 0 {
        parts.push(unit(h, "hour"));
    }
    if m > 0 || parts.is_empty() {
        parts.push(unit(m, "min"));
    }
    parts.join(", ")
}

fn count_dirs(dir: impl AsRef<Path>, skip: &[&str]) -> usize {
    std::fs::read_dir(dir).map(|rd| rd.flatten().filter(|e| e.path().is_dir() && !skip.contains(&e.file_name().to_string_lossy().as_ref())).count()).unwrap_or(0)
}

/// Flatpak refs under an installation: each app or runtime branch counts once.
fn flatpak_refs(root: &Path) -> usize {
    let mut n = 0;
    for kind in ["app", "runtime"] {
        let Ok(names) = std::fs::read_dir(root.join(kind)) else { continue };
        for name in names.flatten() {
            let Ok(arches) = std::fs::read_dir(name.path()) else { continue };
            for arch in arches.flatten() {
                n += count_dirs(arch.path(), &[]);
            }
        }
    }
    n
}

fn packages() -> Option<String> {
    let home = rivet_xdg::places::home();
    let mut found: Vec<(usize, &str)> = Vec::new();
    let mut add = |n: usize, name: &'static str| {
        if n > 0 {
            found.push((n, name));
        }
    };
    add(rpm(), "rpm");
    add(read("/var/lib/dpkg/status").map(|s| s.lines().filter(|l| *l == "Status: install ok installed").count()).unwrap_or(0), "dpkg");
    add(count_dirs("/var/lib/pacman/local", &[]), "pacman");
    add(read("/lib/apk/db/installed").map(|s| s.lines().filter(|l| l.starts_with("P:")).count()).unwrap_or(0), "apk");
    add(read_dir_files("/var/db/xbps", "pkgdb-").map(|s| s.matches("<key>pkgver</key>").count()).unwrap_or(0), "xbps");
    add(flatpak_refs(Path::new("/var/lib/flatpak")) + flatpak_refs(&rivet_xdg::places::data_home().join("flatpak")), "flatpak");
    add(count_dirs("/snap", &["bin", "README"]).max(count_dirs("/var/lib/snapd/snap", &["bin", "README"])), "snap");
    let brew = Path::new("/home/linuxbrew/.linuxbrew");
    add(count_dirs(brew.join("Cellar"), &[]) + count_dirs(brew.join("Caskroom"), &[]), "brew");
    add(nix_profile(&home), "nix");
    let appimages = ["Applications", "AppImages", ".local/bin"]
        .iter()
        .filter_map(|d| std::fs::read_dir(home.join(d)).ok())
        .flat_map(|rd| rd.flatten())
        .filter(|e| e.file_name().to_string_lossy().to_lowercase().ends_with(".appimage"))
        .count();
    add(appimages, "appimage");
    if found.is_empty() {
        return None;
    }
    Some(found.iter().map(|(n, name)| format!("{n} ({name})")).collect::<Vec<_>>().join(", "))
}

/// The first file in a folder whose name starts with `prefix`, read.
fn read_dir_files(dir: &str, prefix: &str) -> Option<String> {
    let e = std::fs::read_dir(dir).ok()?.flatten().find(|e| e.file_name().to_string_lossy().starts_with(prefix))?;
    read(e.path())
}

/// Packages in the user's nix profile, when there is one.
fn nix_profile(home: &Path) -> usize {
    read(home.join(".nix-profile/manifest.json")).map(|s| s.matches("\"storePaths\"").count()).unwrap_or(0)
}

/// RPM's count: a quick query of its database when sqlite3 is installed, else `rpm -qa`, which
/// takes a few seconds, so its answer is kept until the database changes.
fn rpm() -> usize {
    let db = Path::new("/var/lib/rpm/rpmdb.sqlite");
    if !db.exists() && !Path::new("/var/lib/rpm/Packages").exists() {
        return 0;
    }
    // The keys rpm imports show up as packages; fastfetch and `dnf list` leave them out.
    let sql = "select count(*) from Packages where hnum not in (select hnum from Name where key = 'gpg-pubkey')";
    if db.exists()
        && let Some(n) = run("sqlite3", &["-readonly", &db.to_string_lossy(), sql]).and_then(|s| s.parse().ok())
    {
        return n;
    }
    let stamp = std::fs::metadata(if db.exists() { db } else { Path::new("/var/lib/rpm/Packages") })
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cache = rivet_xdg::places::cache_home().join("rivet-fetch/rpm");
    if let Some((s, n)) = read(&cache).and_then(|c| c.split_once(' ').map(|(a, b)| (a.to_string(), b.to_string())))
        && s == stamp.to_string()
        && let Ok(n) = n.parse()
    {
        return n;
    }
    let n = run("rpm", &["-qa", "--qf", "%{NAME}\n"]).map(|s| s.lines().filter(|l| *l != "gpg-pubkey").count()).unwrap_or(0);
    if n > 0 {
        let _ = std::fs::create_dir_all(cache.parent().unwrap());
        let _ = std::fs::write(&cache, format!("{stamp} {n}"));
    }
    n
}

/// A process's parent, name and program path.
fn process(pid: u32) -> Option<(u32, String, PathBuf)> {
    let stat = read(format!("/proc/{pid}/stat"))?;
    // The name sits in parentheses and may hold spaces, so read past the last one.
    let after = &stat[stat.rfind(')')? + 2..];
    let ppid = after.split_whitespace().nth(1)?.parse().ok()?;
    let name = read(format!("/proc/{pid}/comm")).unwrap_or_default();
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).unwrap_or_default();
    Some((ppid, name, exe))
}

const SHELLS: [&str; 12] = ["sh", "bash", "zsh", "fish", "dash", "ksh", "mksh", "tcsh", "csh", "nu", "elvish", "xonsh"];
/// Programs between a shell and its terminal that aren't the terminal.
const WRAPPERS: [&str; 10] = ["sudo", "su", "doas", "script", "env", "time", "nohup", "flatpak-spawn", "systemd-run", "run0"];

/// The ancestors of this process, nearest first: (pid, name, program).
fn ancestors() -> Vec<(u32, String, PathBuf)> {
    let mut out = Vec::new();
    let mut pid = std::os::unix::process::parent_id();
    while pid > 1 && out.len() < 32 {
        let Some((ppid, name, exe)) = process(pid) else { break };
        out.push((pid, name, exe));
        pid = ppid;
    }
    out
}

fn is_shell(name: &str) -> bool {
    let n = name.trim_start_matches('-');
    SHELLS.contains(&n)
}

fn shell() -> Option<String> {
    // The shell this was run from, as fastfetch does; the login shell when run some other way.
    let (name, exe) = ancestors()
        .into_iter()
        .find(|(_, n, _)| is_shell(n))
        .map(|(_, n, e)| (n.trim_start_matches('-').to_string(), e))
        .or_else(|| {
            let s = PathBuf::from(std::env::var("SHELL").ok()?);
            Some((s.file_name()?.to_string_lossy().to_string(), s))
        })?;
    let version = match name.as_str() {
        "bash" | "zsh" | "fish" | "ksh" | "nu" | "elvish" | "xonsh" | "tcsh" => {
            let program = if exe.as_os_str().is_empty() { PathBuf::from(&name) } else { exe };
            run(&program.to_string_lossy(), &["--version"]).and_then(|v| version_in(&v))
        }
        _ => None,
    };
    Some(match version {
        Some(v) => format!("{name} {v}"),
        None => name,
    })
}

/// The first version number in a program's `--version` text.
pub fn version_in(text: &str) -> Option<String> {
    let line = text.lines().next()?;
    line.split(|c: char| c.is_whitespace() || c == ',')
        .map(|w| w.trim_start_matches('v'))
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()) && w.contains('.'))
        .map(|w| w.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<String>().trim_end_matches('.').to_string())
}

fn display() -> Vec<Fact> {
    crate::outputs::screens()
        .into_iter()
        .filter(|s| s.width > 0)
        .map(|s| {
            let key = if s.name.is_empty() { "Display".to_string() } else { format!("Display ({})", s.name) };
            let mut v = format!("{}x{}", s.width, s.height);
            if s.inches > 0 {
                v.push_str(&format!(" in {}\"", s.inches));
            }
            if s.refresh > 0 {
                v.push_str(&format!(", {} Hz", (s.refresh as f64 / 1000.0).round()));
            }
            v.push_str(if s.builtin { " [Built-in]" } else { " [External]" });
            Fact::new(key, v)
        })
        .collect()
}

fn wm() -> Option<String> {
    let name = ["XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP", "DESKTOP_SESSION"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .map(|v| v.split(':').next().unwrap_or(&v).to_string())
        .or_else(compositor_process)?;
    let session = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        "Wayland"
    } else if std::env::var_os("DISPLAY").is_some() {
        "X11"
    } else {
        return Some(name);
    };
    Some(format!("{name} ({session})"))
}

/// A known window manager among the running programs, for sessions that don't name themselves.
fn compositor_process() -> Option<String> {
    const KNOWN: [&str; 14] = ["niri", "Hyprland", "sway", "mango", "river", "labwc", "wayfire", "kwin_wayland", "gnome-shell", "i3", "bspwm", "openbox", "xfwm4", "awesome"];
    std::fs::read_dir("/proc").ok()?.flatten().find_map(|e| {
        let name = read(e.path().join("comm"))?;
        KNOWN.contains(&name.as_str()).then_some(name)
    })
}

/// The GTK look, read once for the four modules that show it.
#[derive(Default, Debug)]
pub struct Gtk {
    pub theme: Option<String>,
    pub icons: Option<String>,
    pub font: Option<String>,
    pub cursor: Option<String>,
}

impl Gtk {
    /// GNOME's settings when the desktop keeps them (GTK reads those first under Wayland),
    /// else GTK's own settings files.
    pub fn read() -> Gtk {
        let mut g = Gtk::default();
        if let Some(text) = run("gsettings", &["list-recursively", "org.gnome.desktop.interface"]) {
            for line in text.lines() {
                let mut parts = line.splitn(3, ' ');
                let (Some(_), Some(key), Some(value)) = (parts.next(), parts.next(), parts.next()) else { continue };
                let value = Some(value.trim_matches('\'').to_string()).filter(|v| !v.is_empty());
                match key {
                    "gtk-theme" => g.theme = value,
                    "icon-theme" => g.icons = value,
                    "font-name" => g.font = value,
                    "cursor-theme" => g.cursor = value,
                    _ => {}
                }
            }
        }
        let config = rivet_xdg::places::config_home();
        for file in ["gtk-4.0/settings.ini", "gtk-3.0/settings.ini"] {
            let Some(text) = read(config.join(file)) else { continue };
            for line in text.lines() {
                let Some((k, v)) = line.split_once('=') else { continue };
                let v = Some(v.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty());
                let slot = match k.trim() {
                    "gtk-theme-name" => &mut g.theme,
                    "gtk-icon-theme-name" => &mut g.icons,
                    "gtk-font-name" => &mut g.font,
                    "gtk-cursor-theme-name" => &mut g.cursor,
                    _ => continue,
                };
                if slot.is_none() {
                    *slot = v;
                }
            }
        }
        g.font = g.font.map(|f| font_label(&f));
        g
    }
}

/// "Cantarell 11" as "Cantarell (11pt)".
pub fn font_label(f: &str) -> String {
    match f.rsplit_once(' ') {
        Some((name, size)) if size.parse::<f32>().is_ok() => format!("{name} ({size}pt)"),
        _ => f.to_string(),
    }
}

fn terminal() -> Option<String> {
    if let Ok(t) = std::env::var("TERM_PROGRAM")
        && !t.is_empty()
    {
        let v = std::env::var("TERM_PROGRAM_VERSION").unwrap_or_default();
        return Some(format!("{t} {v}").trim().to_string());
    }
    // The first ancestor past the shells and the programs that wrap them.
    let list = ancestors();
    let past_shell = list.iter().position(|(_, n, _)| is_shell(n)).unwrap_or(0);
    let (_, name, _) = list[past_shell..].iter().find(|(_, n, _)| !is_shell(n) && !WRAPPERS.contains(&n.as_str()))?;
    Some(match name.as_str() {
        "login" | "agetty" => read_tty().unwrap_or_else(|| "console".into()),
        "sshd" | "sshd-session" => "SSH session".into(),
        "rivet-terminal" => "Rivet Terminal".into(),
        "gnome-terminal-" | "gnome-terminal-server" => "GNOME Terminal".into(),
        other => other.to_string(),
    })
}

fn read_tty() -> Option<String> {
    std::fs::read_link("/proc/self/fd/0").ok().map(|p| p.to_string_lossy().to_string()).filter(|p| p.starts_with("/dev/tty"))
}

fn cpu() -> Option<String> {
    let info = read("/proc/cpuinfo")?;
    let get = |k: &str| info.lines().find_map(|l| l.split_once(':').filter(|(a, _)| a.trim() == k).map(|(_, b)| b.trim().to_string()));
    let mut model = get("model name").or_else(|| get("Hardware")).or_else(|| get("cpu model")).or_else(|| uname().map(|u| field(&u.machine)))?;
    // Older Intel names carry their base clock; the top clock is shown instead.
    if let Some(i) = model.find(" @ ") {
        model.truncate(i);
    }
    let model = model.replace(" CPU", "").split_whitespace().collect::<Vec<_>>().join(" ");
    let threads = info.lines().filter(|l| l.starts_with("processor")).count();
    let max_khz = std::fs::read_dir("/sys/devices/system/cpu")
        .ok()
        .into_iter()
        .flat_map(|rd| rd.flatten())
        .filter_map(|e| read(e.path().join("cpufreq/cpuinfo_max_freq"))?.parse::<u64>().ok())
        .chain(cpuid_max_mhz().map(|m| m * 1000))
        .max();
    let mut s = model;
    if threads > 0 {
        s.push_str(&format!(" ({threads})"));
    }
    if let Some(k) = max_khz {
        s.push_str(&format!(" @ {:.2} GHz", k as f64 / 1e6));
    }
    Some(s)
}

/// The processor's own top clock in MHz. The kernel's figure drops to the base clock while
/// turbo is switched off, so the larger of the two is shown.
#[cfg(target_arch = "x86_64")]
fn cpuid_max_mhz() -> Option<u64> {
    use std::arch::x86_64::__cpuid;
    #[allow(unused_unsafe)]
    let (top, leaf) = unsafe { (__cpuid(0).eax, __cpuid(0x16)) };
    (top >= 0x16 && leaf.ebx & 0xffff > 0).then_some((leaf.ebx & 0xffff) as u64)
}

#[cfg(not(target_arch = "x86_64"))]
fn cpuid_max_mhz() -> Option<u64> {
    None
}

/// Where distributions keep the PCI name list.
const PCI_IDS: [&str; 4] = ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids", "/usr/share/pci.ids", "/usr/share/pciids/pci.ids"];

fn gpu() -> Vec<String> {
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else { return Vec::new() };
    let mut cards: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("card") && !n.contains('-')))
        .filter_map(|p| p.join("device").canonicalize().ok())
        .collect();
    cards.sort();
    cards.dedup();
    let ids = PCI_IDS.iter().find_map(read);
    cards
        .iter()
        .filter_map(|dev| {
            let vendor = read(dev.join("vendor"))?.trim_start_matches("0x").to_lowercase();
            let device = read(dev.join("device"))?.trim_start_matches("0x").to_lowercase();
            Some(gpu_name(ids.as_deref().unwrap_or(""), &vendor, &device))
        })
        .collect()
}

/// A graphics card's name from the PCI list: the marketing name in brackets when there is one,
/// with the maker in front.
pub fn gpu_name(ids: &str, vendor: &str, device: &str) -> String {
    let maker = match vendor {
        "8086" => "Intel".to_string(),
        "1002" | "1022" => "AMD".to_string(),
        "10de" => "NVIDIA".to_string(),
        _ => String::new(),
    };
    let mut in_vendor = false;
    let mut vendor_name = String::new();
    let mut found = None;
    for line in ids.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if !line.starts_with('\t') {
            in_vendor = line.starts_with(vendor);
            if in_vendor {
                vendor_name = line[vendor.len()..].trim().to_string();
            }
            if found.is_some() {
                break;
            }
            continue;
        }
        if in_vendor && !line.starts_with("\t\t") && line[1..].starts_with(device) {
            found = Some(line[1 + device.len()..].trim().to_string());
        }
    }
    let maker = if maker.is_empty() { vendor_name } else { maker };
    let Some(name) = found else {
        return format!("{} {vendor}:{device}", if maker.is_empty() { "GPU" } else { &maker });
    };
    let name = match (name.find('['), name.rfind(']')) {
        (Some(a), Some(b)) if b > a => name[a + 1..b].to_string(),
        _ => name,
    };
    if maker.is_empty() || name.to_lowercase().starts_with(&maker.to_lowercase()) { name } else { format!("{maker} {name}") }
}

/// (used, total) bytes from two /proc/meminfo lines, used being total less the other.
fn meminfo(total_key: &str, free_key: &str) -> Option<(u64, u64)> {
    let text = read("/proc/meminfo")?;
    let get = |k: &str| -> Option<u64> { text.lines().find_map(|l| l.strip_prefix(k)?.strip_prefix(':')?.split_whitespace().next()?.parse::<u64>().ok()) };
    let total = get(total_key)? * 1024;
    let free = get(free_key)? * 1024;
    Some((total.saturating_sub(free), total))
}

fn disk() -> Vec<Fact> {
    let Some((total, free)) = rivet_xdg::places::disk_space(Path::new("/")) else { return Vec::new() };
    let fs = read("/proc/self/mounts").and_then(|m| m.lines().find_map(|l| {
        let p: Vec<&str> = l.split_whitespace().collect();
        (p.get(1) == Some(&"/")).then(|| p.get(2).map(|s| s.to_string())).flatten()
    }));
    let mut f = used("Disk (/)", total.saturating_sub(free), total);
    f.after = fs.map(|fs| format!(" - {fs}"));
    vec![f]
}

fn battery() -> Vec<Fact> {
    let Ok(rd) = std::fs::read_dir("/sys/class/power_supply") else { return Vec::new() };
    let mut supplies: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    supplies.sort();
    let ac = supplies.iter().any(|p| read(p.join("type")).as_deref() == Some("Mains") && read(p.join("online")).as_deref() == Some("1"));
    supplies
        .iter()
        .filter(|p| read(p.join("type")).as_deref() == Some("Battery") && read(p.join("scope")).as_deref() != Some("Device"))
        .filter_map(|p| {
            let capacity: f64 = read(p.join("capacity"))?.parse().ok()?;
            let status = match read(p.join("status")).as_deref() {
                Some("Charging") => "Charging",
                Some("Discharging") => "Discharging",
                _ if ac => "AC Connected",
                _ => "",
            };
            let key = read(p.join("model_name")).map(|m| format!("Battery ({m})")).unwrap_or_else(|| "Battery".into());
            let value = if status.is_empty() { String::new() } else { format!("[{status}]") };
            Some(Fact { key, value, percent: Some(capacity), after: None })
        })
        .collect()
}

/// The Rivet apps on this computer, by the names people know them by.
const RIVET_APPS: [(&str, &str); 8] = [
    ("rivet", "Files"),
    ("rivet-shell", "Shell"),
    ("rivet-edit", "Edit"),
    ("rivet-terminal", "Terminal"),
    ("rivet-notes", "Notes"),
    ("rivet-ftp", "FTP"),
    ("rivet-snip", "Snip"),
    ("rivet-welcome", "Welcome"),
];

fn rivet() -> Option<String> {
    let path: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    let installed: Vec<&str> = RIVET_APPS.iter().filter(|(bin, _)| path.iter().any(|d| d.join(bin).is_file())).map(|(_, name)| *name).collect();
    if installed.is_empty() {
        return None;
    }
    let shell_running = std::fs::read_dir("/proc").ok()?.flatten().any(|e| read(e.path().join("comm")).as_deref() == Some("rivet-shell"));
    let mut s = installed.join(", ");
    if shell_running {
        s.push_str(" (Shell running)");
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_like_fastfetch() {
        assert_eq!(duration(30), "0 mins");
        assert_eq!(duration(49 * 60), "49 mins");
        assert_eq!(duration(86400 + 3600 + 60), "1 day, 1 hour, 1 min");
    }

    #[test]
    fn versions_come_out_of_banners() {
        assert_eq!(version_in("zsh 5.9 (x86_64-redhat-linux-gnu)").as_deref(), Some("5.9"));
        assert_eq!(version_in("GNU bash, version 5.2.37(1)-release (x86_64)").as_deref(), Some("5.2.37"));
        assert_eq!(version_in("fish, version 4.0.2").as_deref(), Some("4.0.2"));
    }

    #[test]
    fn gpu_names_use_the_bracketed_name() {
        let ids = "8086  Intel Corporation\n\t7d55  Meteor Lake-P [Intel Arc Graphics]\n\t\t1028 0c95  sub\n1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t73bf  Navi 21 [Radeon RX 6800/6800 XT / 6900 XT]\n";
        assert_eq!(gpu_name(ids, "8086", "7d55"), "Intel Arc Graphics");
        assert_eq!(gpu_name(ids, "1002", "73bf"), "AMD Radeon RX 6800/6800 XT / 6900 XT");
        assert_eq!(gpu_name(ids, "10de", "ffff"), "NVIDIA 10de:ffff");
    }

    #[test]
    fn fonts_show_their_size() {
        assert_eq!(font_label("Cantarell 10"), "Cantarell (10pt)");
        assert_eq!(font_label("Inter"), "Inter");
    }

    #[test]
    fn every_default_module_is_known() {
        assert!(DEFAULT.split(',').all(known));
    }
}

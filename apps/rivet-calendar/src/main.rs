//! Rivet Calendar: events kept in plain `.ics` folders, by day, week, month or as a list.

mod app;
mod settings;
mod shell;

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::NaiveDate;

const HELP: &str = "Rivet Calendar: events kept in plain .ics folders

Usage:
  rivet-calendar                    Open the window on today
  rivet-calendar --date 2026-11-24  …on that day
  rivet-calendar --date DAY --event UID
                                    …with that event open
  rivet-calendar webcal://ADDRESS   Open the window, asking to subscribe to that feed
  rivet-calendar FILE.ics           Open the window, asking which calendar to add its events to
  rivet-calendar list [DAYS]        Print the events of the next DAYS days (7 if not given)
  rivet-calendar subscribe URL [NAME]
                                    Subscribe to a calendar feed (webcal:// or https://)
  rivet-calendar refresh            Read every subscribed feed again now
  rivet-calendar account add URL USER [NAME]
                                    Add a CalDAV account (the password is read from input)
  rivet-calendar account add google|microsoft [NAME] [--client-id ID]
                                    Add a Google or Microsoft account, signing in in the
                                    browser; with your own client ID (a Google client
                                    secret is then read from input)
  rivet-calendar account [list]     The accounts
  rivet-calendar account remove ID  Remove one (its calendars here go to the Trash)
  rivet-calendar sync               Sync every calendar on a server now
  rivet-calendar shell-plugin       Run as Rivet Shell's Calendar plugin (the shell starts this)
  rivet-calendar --install-desktop-entry
                                    Add Rivet Calendar to the app launcher (for this user)
  rivet-calendar -h, --help         This help
  rivet-calendar -V, --version      The version

Each calendar is a folder of .ics files in ~/.local/share/rivet-calendar (the layout
vdirsyncer and khal use), with its name and colour in the files displayname and color.
Subscriptions are read again every 6 hours while the window or the panel's plugin runs.";

/// The launcher entry and icon, in the user's data folder, running this program.
fn install_desktop_entry() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let data = rivet_xdg::places::data_home();
    let entry = data.join("applications/rivet-calendar.desktop");
    let icon = data.join("icons/hicolor/scalable/apps/rivet-calendar.svg");
    let text = include_str!("../data/rivet-calendar.desktop").replace("Exec=rivet-calendar", &format!("Exec=\"{}\"", exe.display()));
    for (p, body) in [(&entry, text.as_str()), (&icon, app::ICON_SVG)] {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(p, body).map_err(|e| format!("couldn't write {}: {e}", p.display()))?;
    }
    let _ = std::process::Command::new("update-desktop-database").arg(data.join("applications")).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    Ok(entry)
}

/// The next days' events, one a line, for a terminal or a script.
fn list(days: i64) -> ExitCode {
    let s = settings::Settings::load();
    let store = rivet_cal::Store::open(&s.root());
    let today = chrono::Local::now().date_naive();
    let mut last = None;
    for o in store.occurrences(today, today + chrono::Duration::days(days), &s.hidden) {
        let Some(ev) = store.event(&o) else { continue };
        if last != Some(o.start.date()) {
            last = Some(o.start.date());
            println!("{}", o.start.format("%A %-d %B"));
        }
        let when = if o.all_day { "all day".to_string() } else { format!("{}-{}", app::clock(o.start.time(), s.clock_24), app::clock(o.end.time(), s.clock_24)) };
        println!("  {when:<13} {}  ({})", ev.summary, store.calendars[o.cal].name);
    }
    ExitCode::SUCCESS
}

/// Bring a new account's calendars down, saying how each went.
fn first_sync(dirs: &[PathBuf]) -> ExitCode {
    let mut failed = false;
    for d in dirs {
        let name = std::fs::read_to_string(d.join("displayname")).unwrap_or_default();
        match rivet_cal::caldav::sync(d) {
            Ok(s) => println!("{}: {} events", name.trim(), s.down),
            Err(e) => {
                failed = true;
                eprintln!("{}: {e}", name.trim());
            }
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// Where crash reports go, and the issue form they're shared on.
pub const CRASH: rivet_xdg::crash::App = rivet_xdg::crash::App { name: "Rivet Calendar", id: "calendar", state: "rivet-calendar", version: env!("CARGO_PKG_VERSION"), repo: env!("CARGO_PKG_REPOSITORY") };

fn main() -> ExitCode {
    rivet_xdg::crash::install(CRASH);
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut take = |flag: &str| {
        let i = args.iter().position(|a| a == flag)?;
        args.remove(i);
        (i < args.len()).then(|| args.remove(i))
    };
    let screenshot = take("--screenshot").map(PathBuf::from);
    let date = take("--date").and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let event = take("--event");
    let client_id = take("--client-id");
    match args.first().map(String::as_str) {
        Some("shell-plugin") => return shell::run(),
        Some("list") => return list(args.get(1).and_then(|n| n.parse().ok()).unwrap_or(7)),
        Some("-h" | "--help" | "help") => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version" | "version") => {
            println!("rivet-calendar {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("--install-desktop-entry") => {
            return match install_desktop_entry() {
                Ok(p) => {
                    println!("Added {}", p.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("rivet-calendar: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("subscribe") => {
            let Some(url) = args.get(1) else {
                eprintln!("rivet-calendar: subscribe to what? (rivet-calendar subscribe URL [NAME])");
                return ExitCode::FAILURE;
            };
            let root = settings::Settings::load().root();
            return match rivet_cal::feed::subscribe(&root, url, &args[2..].join(" "), app::COLORS[1]) {
                Ok(dir) => {
                    println!("Subscribed: {}", dir.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("rivet-calendar: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("account") if args.get(1).map(String::as_str) == Some("add") && args.get(2).and_then(|k| rivet_cal::oauth::Provider::from_key(k)).is_some() => {
            let Some(p) = args.get(2).and_then(|k| rivet_cal::oauth::Provider::from_key(k)) else { return ExitCode::FAILURE };
            let own = client_id.map(|id| {
                let mut secret = String::new();
                if p.wants_secret() {
                    if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                        eprint!("Client secret: ");
                    }
                    let _ = std::io::stdin().read_line(&mut secret);
                }
                rivet_cal::oauth::Client { id, secret: secret.trim().to_string() }
            });
            eprintln!("Sign in to {} in the browser that opens.", p.name());
            let stop = std::sync::atomic::AtomicBool::new(false);
            return match rivet_cal::caldav::add_signed_in(&settings::Settings::load().root(), p, own, &args[3..].join(" "), &stop) {
                Ok(dirs) => {
                    println!("Added {} calendar{}", dirs.len(), if dirs.len() == 1 { "" } else { "s" });
                    first_sync(&dirs)
                }
                Err(e) => {
                    eprintln!("rivet-calendar: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("account") if args.get(1).map(String::as_str) == Some("add") => {
            let (Some(url), Some(user)) = (args.get(2), args.get(3)) else {
                eprintln!("rivet-calendar: account add URL USER [NAME] (the password is read from input)");
                return ExitCode::FAILURE;
            };
            // The password from input (a line typed, or piped in): never on the command line.
            let mut pass = String::new();
            if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                eprint!("Password for {user}: ");
            }
            let _ = std::io::stdin().read_line(&mut pass);
            let root = settings::Settings::load().root();
            return match rivet_cal::caldav::add_account(&root, url, user, pass.trim_end_matches(['\r', '\n']), &args[4..].join(" ")) {
                Ok(dirs) => {
                    println!("Added {} calendar{}", dirs.len(), if dirs.len() == 1 { "" } else { "s" });
                    first_sync(&dirs)
                }
                Err(e) => {
                    eprintln!("rivet-calendar: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("account") if args.get(1).map(String::as_str) == Some("remove") => {
            let Some(id) = args.get(2) else {
                eprintln!("rivet-calendar: account remove ID (rivet-calendar account list)");
                return ExitCode::FAILURE;
            };
            return match rivet_cal::caldav::remove_account(&settings::Settings::load().root(), id) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("rivet-calendar: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("account") => {
            for a in rivet_cal::caldav::accounts() {
                match a.provider {
                    Some(p) => println!("{}  {} ({} at {}{})", a.id, a.name, a.user, p.name(), if a.client.is_empty() { "" } else { ", own client ID" }),
                    None => println!("{}  {} ({} at {})", a.id, a.name, a.user, a.url),
                }
            }
            return ExitCode::SUCCESS;
        }
        Some("sync") => {
            let (done, problems) = rivet_cal::caldav::sync_all(&settings::Settings::load().root());
            println!("{} down, {} up, {} removed", done.down, done.up, done.removed);
            for p in &problems {
                eprintln!("{p}");
            }
            return if problems.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
        }
        Some("refresh") => {
            let root = settings::Settings::load().root();
            let mut failed = false;
            for c in rivet_cal::Store::open(&root).calendars.iter().filter(|c| c.feed.is_some()) {
                match rivet_cal::feed::refresh(&c.dir) {
                    Ok(n) => println!("{}: {n} events", c.name),
                    Err(e) => {
                        failed = true;
                        eprintln!("{}: {e}", c.name);
                    }
                }
            }
            return if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS };
        }
        // A webcal:// link to subscribe to, or an .ics file to add: the window, asking.
        Some(_) => {}
        None => {}
    }
    let open = args.first().cloned();
    if screenshot.is_some() {
        rivet_xdg::crash::NO_WRITE.store(true, std::sync::atomic::Ordering::Relaxed);
        // RIVET_SHOT_SIZE=WxH for documentation pictures (1100x720 by default).
        let size = std::env::var("RIVET_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        });
        rivet_kit::headless::run(size.unwrap_or([1100.0, 720.0]), move |cc| Box::new(app::CalendarApp::new(cc, date, event, open, screenshot)));
        return ExitCode::SUCCESS;
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet Calendar").with_app_id("rivet-calendar").with_inner_size([1100.0, 740.0]).with_min_inner_size([640.0, 460.0]);
    if let Some(img) = rivet_look::icons::render_svg(app::ICON_SVG.as_bytes(), 128) {
        let rgba = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        viewport = viewport.with_icon(eframe::egui::IconData { rgba, width: 128, height: 128 });
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    match eframe::run_native("Rivet Calendar", options, Box::new(move |cc| Ok(Box::new(app::CalendarApp::new(cc, date, event, open, None))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-calendar: couldn't open the window: {e}");
            ExitCode::FAILURE
        }
    }
}

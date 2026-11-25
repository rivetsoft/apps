//! Rivet Calendar's window: a toolbar (today, back and on, the view), a sidebar (a small month
//! and the calendars), the days as a month, a week, a day or a list, and an editor for an event
//! in a window of its own inside the app.

use std::path::PathBuf;

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_cal::oauth::{Client, Provider};
use rivet_cal::{Event, Occurrence, Store, When};
use rivet_look::fonts::ui_font;
use rivet_look::live::{Choice, LiveLook};
use rivet_look::palette::mix;
use rivet_look::theme::{r_md, r_sm, r_xs};
use rivet_look::windows::{self, WinState};

use crate::settings::{Settings, View};

pub const ICON_SVG: &str = include_str!("../data/rivet-calendar.svg");

/// Colours for calendars that have none, and to choose from for a new one.
pub const COLORS: [[u8; 3]; 8] = [[0xe0, 0x57, 0x3f], [0x72, 0x87, 0xfd], [0x40, 0xa0, 0x2b], [0xdf, 0x8e, 0x1d], [0x88, 0x39, 0xef], [0x17, 0x92, 0x99], [0xea, 0x76, 0xcb], [0x7c, 0x7f, 0x93]];

/// How tall an hour is in the week and day views.
const HOUR: f32 = 48.0;
const GUTTER: f32 = 56.0;
const CHIP: f32 = 20.0;

/// A time as the person likes it: `14:00` or `2:00 PM`.
pub fn clock(t: NaiveTime, h24: bool) -> String {
    if h24 { t.format("%H:%M").to_string() } else { t.format("%-I:%M %p").to_string() }
}

/// A time typed in: `9`, `9:30`, `0930`, `9am`, `2:30 pm`.
fn parse_time(s: &str) -> Option<NaiveTime> {
    let t = s.trim().to_lowercase().replace(' ', "");
    let (t, pm, am) = match (t.strip_suffix("pm"), t.strip_suffix("am")) {
        (Some(r), _) => (r.to_string(), true, false),
        (_, Some(r)) => (r.to_string(), false, true),
        _ => (t, false, false),
    };
    let (h, m) = match t.split_once(':').or_else(|| t.split_once('.')) {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None if t.len() > 2 => (t[..t.len() - 2].parse().ok()?, t[t.len() - 2..].parse().ok()?),
        None => (t.parse().ok()?, 0),
    };
    let h = match (pm, am, h) {
        (true, _, 12) => 12,
        (true, _, h) => h + 12,
        (_, true, 12) => 0,
        (_, _, h) => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

/// A date typed in: `2026-11-24` (or with slashes).
fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim().replace('/', "-");
    NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Black or white, whichever reads on `bg`.
fn ink_on(bg: Color32) -> Color32 {
    let l = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if l > 160.0 { Color32::from_rgb(0x1c, 0x1c, 0x1f) } else { Color32::WHITE }
}

/// The first day of the week `day` is in.
fn week_of(day: NaiveDate, monday_first: bool) -> NaiveDate {
    let back = if monday_first { day.weekday().num_days_from_monday() } else { day.weekday().num_days_from_sunday() };
    day - Duration::days(back as i64)
}

/// How an event repeats, as the editor offers it.
#[derive(Clone, Debug, PartialEq)]
enum Repeat {
    Never,
    Daily,
    Weekly,
    /// Monday to Friday.
    Weekdays,
    Monthly,
    Yearly,
    /// A rule the editor doesn't make itself: kept as it is.
    Other(String),
}

impl Repeat {
    fn from_rule(r: Option<&str>) -> Repeat {
        match r.map(|r| r.trim().trim_start_matches("RRULE:").to_ascii_uppercase()) {
            None => Repeat::Never,
            Some(r) => match r.as_str() {
                "FREQ=DAILY" => Repeat::Daily,
                "FREQ=WEEKLY" => Repeat::Weekly,
                "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR" | "FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR" => Repeat::Weekdays,
                "FREQ=MONTHLY" => Repeat::Monthly,
                "FREQ=YEARLY" => Repeat::Yearly,
                _ => Repeat::Other(r),
            },
        }
    }

    fn rule(&self) -> Option<String> {
        match self {
            Repeat::Never => None,
            Repeat::Daily => Some("FREQ=DAILY".into()),
            Repeat::Weekly => Some("FREQ=WEEKLY".into()),
            Repeat::Weekdays => Some("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR".into()),
            Repeat::Monthly => Some("FREQ=MONTHLY".into()),
            Repeat::Yearly => Some("FREQ=YEARLY".into()),
            Repeat::Other(r) => Some(r.clone()),
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Repeat::Never => "Doesn't repeat",
            Repeat::Daily => "Every day",
            Repeat::Weekly => "Every week",
            Repeat::Weekdays => "Every weekday",
            Repeat::Monthly => "Every month",
            Repeat::Yearly => "Every year",
            // A rule made in another program (every second Tuesday…), kept as it is.
            Repeat::Other(_) => "Custom",
        }
    }
}

/// The event being made or changed.
struct Editor {
    /// The event as it was (None: a new one), its calendar, and the occurrence clicked.
    was: Option<(Event, PathBuf, When)>,
    /// The occurrence's start and end, as shown when it opened (moves are worked out from it).
    shown_start: NaiveDateTime,
    title: String,
    location: String,
    notes: String,
    all_day: bool,
    start_date: String,
    start_time: String,
    end_date: String,
    end_time: String,
    cal: PathBuf,
    repeat: Repeat,
    /// For a repeating event: the change is for every occurrence (else only the one clicked).
    every: bool,
    /// Asking before deleting.
    confirm: bool,
    problem: String,
    focus: bool,
}

/// A calendar being made, or renamed and recoloured.
struct CalEdit {
    dir: Option<PathBuf>,
    name: String,
    color: [u8; 3],
    /// Subscribing: the feed's address (None: a calendar of one's own).
    feed: Option<String>,
}

/// An account being added: a CalDAV server and a password, or Google or Microsoft, signed in to
/// in the browser.
#[derive(Default)]
struct AccountEdit {
    provider: Option<Provider>,
    url: String,
    user: String,
    pass: String,
    name: String,
    /// Signing in as one's own registered app instead of Rivet's.
    own: bool,
    client_id: String,
    client_secret: String,
}

impl AccountEdit {
    fn new(provider: Option<Provider>) -> AccountEdit {
        // With no client ID of Rivet's in this build, one's own is the only way.
        let own = provider.is_some_and(|p| p.builtin().is_none());
        AccountEdit { provider, own, ..Default::default() }
    }
}

/// An `.ics` file being put in a calendar.
struct Import {
    file: String,
    text: String,
    count: usize,
    cal: PathBuf,
}

/// A span's occurrences, as worked out for one version of the calendars and the ones hidden.
type OccKey = ((NaiveDate, NaiveDate, u64, Vec<String>), Vec<Occurrence>);

/// Something running on a thread (a feed being read), and what it'll say.
type Job = std::sync::mpsc::Receiver<Result<String, String>>;

/// The calendars, read on a thread so the window never waits on a folder of thousands of events.
/// It's asked to look again every few seconds and after each change made here; it answers with
/// the calendars read again, only when something changed, tagged with the change it was asked
/// after (an answer older than a change made here since is dropped, and the next one has it).
struct Loader {
    ask: std::sync::mpsc::Sender<u64>,
    got: std::sync::mpsc::Receiver<(u64, Store)>,
}

impl Loader {
    fn start(root: PathBuf, ctx: egui::Context) -> Loader {
        let (ask, asks) = std::sync::mpsc::channel::<u64>();
        let (send, got) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut store = Store::open(&root);
            // The first time: a calendar to put things in.
            if store.calendars.is_empty() && store.create_calendar("Personal", COLORS[0]).is_err() {
                store = Store::open(&root);
            }
            let mut stamp = Store::stamp(&root);
            if send.send((0, store.clone())).is_err() {
                return;
            }
            ctx.request_repaint();
            while let Ok(mut generation) = asks.recv() {
                // Several asks waiting are one look.
                while let Ok(g) = asks.try_recv() {
                    generation = generation.max(g);
                }
                let now = Store::stamp(&root);
                if now == stamp {
                    continue;
                }
                stamp = now;
                store.reload();
                if send.send((generation, store.clone())).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
        Loader { ask, got }
    }
}

pub struct CalendarApp {
    s: Settings,
    look: LiveLook,
    store: Store,
    root: PathBuf,
    /// Reading the calendars (None in checks, which read their sample at once).
    loader: Option<Loader>,
    /// Whether the calendars have been read the first time.
    loaded: bool,
    /// Changes made here, counted, so an answer from the loader from before one is dropped.
    generation: u64,
    /// Bumped whenever the calendars may differ, so the occurrences worked out are worked out again.
    version: u64,
    /// Occurrences already worked out: by span, version and the calendars hidden.
    occ_cache: std::cell::RefCell<Vec<OccKey>>,
    checked: f64,
    /// The day the views are around (the one picked).
    day: NaiveDate,
    /// The small month's first day.
    mini: NaiveDate,
    editor: Option<Editor>,
    editor_win: WinState,
    cal_edit: Option<CalEdit>,
    cal_win: WinState,
    /// Asking before a calendar goes to the trash: its folder and name.
    trash_ask: Option<(PathBuf, String)>,
    import: Option<Import>,
    /// What's being done on a thread, and what it'll say when done.
    job: Option<(String, Job)>,
    /// When subscriptions were last looked at to see which are due.
    feeds_checked: f64,
    /// A word from the last thing done ("Holidays: 24 events").
    note: Option<String>,
    account_edit: Option<AccountEdit>,
    /// Waiting for a sign-in in the browser: set to stop waiting.
    signing_in: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Asking before an account is removed: its id and name.
    account_ask: Option<(String, String)>,
    /// When the calendars on servers were last synced, and a sync asked for soon (a change here).
    synced_at: f64,
    sync_soon: Option<f64>,
    /// An event to open once the window is up (`--event`).
    open_uid: Option<String>,
    /// The week and day views scrolled to the morning once.
    scrolled: bool,
    problem: Option<String>,
    shot: Option<rivet_kit::Shot>,
}

/// Sample calendars for screenshot runs (checks never show the person's own), around today.
fn sample_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("rivet-calendar-sample-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut store = Store::open(&root);
    let today = Local::now().date_naive();
    let at = |d: i64, h: u32, m: u32| When::Floating((today + Duration::days(d)).and_hms_opt(h, m, 0).unwrap());
    let day = |d: i64| When::Date(today + Duration::days(d));
    if let (Ok(personal), Ok(work), Ok(home)) = (store.create_calendar("Personal", COLORS[0]), store.create_calendar("Work", COLORS[1]), store.create_calendar("Family", COLORS[2])) {
        let mut add = |dir: &PathBuf, title: &str, s: When, e: When, rule: Option<&str>, place: &str| {
            let mut ev = Event::new(title, s, e);
            ev.rrule = rule.map(String::from);
            ev.location = place.into();
            let _ = store.save(dir, &ev);
        };
        add(&work, "Standup", at(-(today.weekday().num_days_from_monday() as i64), 9, 30), at(-(today.weekday().num_days_from_monday() as i64), 9, 45), Some("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"), "");
        add(&work, "Design review", at(0, 13, 0), at(0, 14, 30), None, "Room 4");
        add(&work, "Release planning", at(2, 10, 0), at(2, 11, 30), None, "");
        add(&personal, "Lunch with Sam", at(0, 12, 0), at(0, 13, 0), None, "Café Lumen");
        add(&personal, "Gym", at(1, 18, 0), at(1, 19, 0), Some("FREQ=WEEKLY"), "");
        add(&personal, "Dentist", at(5, 8, 30), at(5, 9, 15), None, "");
        add(&home, "Trip to the coast", day(8), day(11), None, "");
        add(&home, "Mum's birthday", day(-3), day(-2), Some("FREQ=YEARLY"), "");
        add(&home, "Bins out", day(3), day(4), Some("FREQ=WEEKLY"), "");
        // One invitation in two calendars, as a server puts it in each: shown once.
        let t = |h: u32| (today + Duration::days(1)).and_hms_opt(h, 0, 0).unwrap().format("%Y%m%dT%H%M%S").to_string();
        let shared = format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:offsite@example.org\r\nSUMMARY:Team offsite\r\nDTSTART:{}\r\nDTEND:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n", t(11), t(13));
        for dir in [&work, &home] {
            let _ = std::fs::write(dir.join("offsite.ics"), &shared);
        }
    }
    // A subscription, made from text (checks never download).
    if let Ok(dir) = store.create_calendar("Holidays", COLORS[3]) {
        let d = |n: i64| (today + Duration::days(n)).format("%Y%m%d").to_string();
        let text = format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:h1\r\nSUMMARY:Founders' Day\r\nDTSTART;VALUE=DATE:{}\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:h2\r\nSUMMARY:Harvest Festival\r\nDTSTART;VALUE=DATE:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n", d(2), d(13));
        let _ = std::fs::write(dir.join(".rivet-feed"), "url=webcal://example.org/holidays.ics\n");
        let _ = std::fs::write(dir.join(".read-only"), "");
        let _ = rivet_cal::feed::apply(&dir, &text);
    }
    root
}

impl CalendarApp {
    /// `open`: a feed's address to subscribe to, or an `.ics` file to put in a calendar.
    pub fn new(_cc: &eframe::CreationContext, date: Option<NaiveDate>, event: Option<String>, open: Option<String>, screenshot: Option<PathBuf>) -> CalendarApp {
        let checking = screenshot.is_some();
        if checking {
            crate::settings::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let mut s = if checking { Settings::default() } else { Settings::load() };
        // Checks: RIVET_CALENDAR_VIEW=day|week|month|agenda.
        if let Some(v) = std::env::var("RIVET_CALENDAR_VIEW").ok().filter(|_| checking) {
            s.view = View::from(&v);
        }
        let root = if checking { sample_root() } else { s.root() };
        // Checks read their sample now; a window reads the calendars on a thread.
        let (store, loader) = if checking { (Store::open(&root), None) } else { (Store { root: root.clone(), calendars: Vec::new() }, Some(Loader::start(root.clone(), _cc.egui_ctx.clone()))) };
        let day = date.unwrap_or_else(|| Local::now().date_naive());
        // Checks: RIVET_CALENDAR_EDIT=1 opens the editor on the first event today.
        let open_uid = event.or_else(|| std::env::var_os("RIVET_CALENDAR_EDIT").filter(|_| checking).map(|_| String::new()));
        let mut app = CalendarApp {
            loaded: loader.is_none(),
            loader,
            generation: 0,
            version: 0,
            occ_cache: Default::default(),
            s,
            look: LiveLook::new(),
            store,
            root,
            checked: 0.0,
            day,
            mini: day.with_day(1).unwrap_or(day),
            editor: None,
            editor_win: WinState::new(41, Vec2::new(460.0, 520.0)),
            cal_edit: open.as_deref().filter(|o| is_feed_address(o)).map(|u| CalEdit { dir: None, name: String::new(), color: COLORS[1], feed: Some(u.to_string()) }),
            cal_win: WinState::new(42, Vec2::new(380.0, 250.0)),
            trash_ask: None,
            import: None,
            job: None,
            // Subscriptions are looked at a little after starting (not before the first frame).
            feeds_checked: if checking { f64::INFINITY } else { -1e9 },
            note: None,
            account_edit: None,
            signing_in: None,
            account_ask: None,
            synced_at: if checking { f64::INFINITY } else { -1e9 },
            sync_soon: None,
            open_uid,
            scrolled: false,
            problem: None,
            shot: screenshot.map(rivet_kit::Shot::new),
        };
        if let Some(path) = open.filter(|o| !is_feed_address(o)) {
            app.ask_import(&path);
        }
        // Checks: RIVET_CALENDAR_ACCOUNT=1 opens the add-account dialog for a CalDAV server;
        // =google and =microsoft for those, and =google-own with one's own client ID shown.
        if checking && let Some(which) = std::env::var_os("RIVET_CALENDAR_ACCOUNT") {
            let which = which.to_string_lossy();
            let provider = Provider::from_key(which.trim_end_matches("-own"));
            let mut ae = AccountEdit::new(provider);
            if provider.is_none() {
                (ae.url, ae.user, ae.pass) = ("nextcloud.example.org".into(), "sam".into(), "secret".into());
            }
            if which.ends_with("-own") {
                ae.own = true;
                ae.client_id = "1234567890-abc.apps.googleusercontent.com".into();
                ae.client_secret = "secret".into();
            }
            app.account_edit = Some(ae);
        }
        // Checks: RIVET_CALENDAR_SUBSCRIBE=1 opens the subscribe dialog.
        if checking && std::env::var_os("RIVET_CALENDAR_SUBSCRIBE").is_some() {
            app.cal_edit = Some(CalEdit { dir: None, name: String::new(), color: COLORS[1], feed: Some(String::new()) });
        }
        app
    }

    /// Open the import dialog on an `.ics` file.
    fn ask_import(&mut self, path: &str) {
        let path = path.strip_prefix("file://").map(rivet_xdg::places::percent_decode).unwrap_or_else(|| path.to_string());
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let count = rivet_cal::feed::split(&text).len();
                let Some((cal, _)) = self.writable().into_iter().next() else {
                    self.problem = Some("Every calendar here can only be read. Make one with + beside Calendars, then open the file again.".into());
                    return;
                };
                if count == 0 {
                    self.problem = Some(format!("{} has no events in it.", file_label(&path)));
                    return;
                }
                self.import = Some(Import { file: file_label(&path), text, count, cal });
            }
            Err(e) => self.problem = Some(format!("Couldn't open {}: {e}.", file_label(&path))),
        }
    }

    /// Read due subscriptions again on a thread, now and then.
    fn tend_feeds(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.job.is_none() && now - self.feeds_checked > 600.0 && self.store.calendars.iter().any(|c| c.feed.is_some()) {
            self.feeds_checked = now;
            let root = self.root.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let changed = rivet_cal::feed::refresh_due(&root);
                let _ = tx.send(Ok(if changed { "Subscriptions updated".into() } else { String::new() }));
                ctx.request_repaint();
            });
            self.job = Some((String::new(), rx));
        }
        // Calendars on servers: every 5 minutes, and a few seconds after a change here.
        let has_accounts = self.store.calendars.iter().any(|c| c.account.is_some());
        let soon = self.sync_soon.is_some_and(|t| now >= t);
        if self.job.is_none() && has_accounts && (soon || now - self.synced_at > 300.0) {
            self.synced_at = now;
            self.sync_soon = None;
            let root = self.root.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let (done, problems) = rivet_cal::caldav::sync_all(&root);
                let _ = tx.send(if problems.is_empty() { Ok(if done == rivet_cal::caldav::Synced::default() { String::new() } else { "Synced".into() }) } else { Err(format!("Couldn't sync {}", problems.join("; "))) });
                ctx.request_repaint();
            });
            self.job = Some((String::new(), rx));
        }
        if let Some((_, rx)) = &self.job
            && let Ok(r) = rx.try_recv()
        {
            self.job = None;
            match r {
                Ok(n) => {
                    let added = n.starts_with("Added");
                    self.note = Some(n).filter(|n| !n.is_empty());
                    if self.cal_edit.as_ref().is_some_and(|c| c.feed.is_some() && c.dir.is_none()) {
                        self.cal_edit = None;
                    }
                    if added {
                        self.account_edit = None;
                        // Its calendars are brought down now, in the background.
                        self.sync_soon = Some(0.0);
                    }
                    self.signing_in = None;
                }
                Err(e) => {
                    self.signing_in = None;
                    self.problem = Some(e);
                }
            }
            self.changed();
        }
    }

    /// Run `work` on a thread, saying `what` meanwhile.
    fn run(&mut self, ctx: &egui::Context, what: &str, work: impl FnOnce() -> Result<String, String> + Send + 'static) {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work());
            ctx.request_repaint();
        });
        self.job = Some((what.to_string(), rx));
    }

    fn choice(&self) -> Choice {
        Choice { mode: self.s.theme, accent: self.s.accent, follow_desktop_colors: self.s.follow_desktop_colors, text_size: self.s.text_size }
    }

    /// The colours of the other calendars an occurrence is also in (three at most).
    fn also_colors(&self, o: &Occurrence) -> Vec<Color32> {
        o.also.iter().take(3).map(|&c| self.color_of(c)).collect()
    }

    /// For an event in several calendars: which ones.
    fn in_calendars(&self, o: &Occurrence) -> Option<String> {
        if o.also.is_empty() {
            return None;
        }
        let names: Vec<&str> = std::iter::once(o.cal).chain(o.also.iter().copied()).filter_map(|c| self.store.calendars.get(c)).map(|c| c.name.as_str()).collect();
        let (last, rest) = names.split_last()?;
        Some(format!("In {} and {last}", rest.join(", ")))
    }

    /// Stripes down the right of a filled shape, one in each other calendar's colour.
    fn also_stripes(&self, p: &egui::Painter, r: Rect, o: &Occurrence) {
        let clip = p.with_clip_rect(r);
        for (k, c) in self.also_colors(o).into_iter().enumerate() {
            let x = r.right() - 4.0 * (k as f32 + 1.0);
            clip.rect_filled(Rect::from_min_max(Pos2::new(x, r.top()), Pos2::new(x + 4.0, r.bottom())), 0.0, c);
            clip.vline(x, r.y_range(), Stroke::new(1.0, Color32::from_black_alpha(40)));
        }
    }

    fn color_of(&self, cal: usize) -> Color32 {
        rgb(self.store.calendars.get(cal).and_then(|c| c.color).unwrap_or(COLORS[cal % COLORS.len()]))
    }

    fn writable(&self) -> Vec<(PathBuf, String)> {
        self.store.calendars.iter().filter(|c| !c.read_only).map(|c| (c.dir.clone(), c.name.clone())).collect()
    }

    /// A span's occurrences, worked out once for each version of the calendars (a frame asks
    /// for the same ones several times, and thousands of events take a while).
    fn occurrences(&self, from: NaiveDate, to: NaiveDate) -> Vec<Occurrence> {
        let key = (from, to, self.version, self.s.hidden.clone());
        if let Some((_, v)) = self.occ_cache.borrow().iter().find(|(k, _)| *k == key) {
            return v.clone();
        }
        let v = self.store.occurrences(from, to, &self.s.hidden);
        let mut cache = self.occ_cache.borrow_mut();
        cache.retain(|(k, _)| k.2 == self.version && k.3 == self.s.hidden);
        if cache.len() >= 8 {
            cache.remove(0);
        }
        cache.push((key, v.clone()));
        v
    }

    /// Something changed here (an event saved, a calendar made), or on disk by a job: the
    /// occurrences are worked out again, and the loader looks at the folder now.
    fn changed(&mut self) {
        self.generation += 1;
        self.version += 1;
        if let Some(l) = &self.loader {
            let _ = l.ask.send(self.generation);
        }
    }

    /// Move the view on (or back) by one of its own steps.
    fn step(&mut self, by: i64) {
        self.day = match self.s.view {
            View::Day => self.day + Duration::days(by),
            View::Week => self.day + Duration::days(7 * by),
            View::Month | View::Agenda => {
                let m = self.day.year() * 12 + self.day.month0() as i32 + by as i32;
                let (y, mo) = (m.div_euclid(12), m.rem_euclid(12) as u32 + 1);
                NaiveDate::from_ymd_opt(y, mo, 1).map(|f| f.with_day(self.day.day().min(days_in(y, mo))).unwrap_or(f)).unwrap_or(self.day)
            }
        };
        self.mini = self.day.with_day(1).unwrap_or(self.day);
    }

    fn set_view(&mut self, v: View) {
        if self.s.view != v {
            self.s.view = v;
            self.scrolled = false;
            self.s.save();
        }
    }

    /// The editor on a new event: at `start` (a whole day when there's no time).
    fn new_event(&mut self, day: NaiveDate, at: Option<NaiveTime>) {
        let Some((dir, _)) = self.writable().into_iter().next() else {
            self.problem = Some("Every calendar here can only be read. Make one with + beside Calendars.".into());
            return;
        };
        let start = at.unwrap_or(NaiveTime::from_hms_opt(9, 0, 0).unwrap_or(NaiveTime::MIN));
        let end = start + Duration::hours(1);
        let h24 = self.s.clock_24;
        self.editor = Some(Editor {
            was: None,
            shown_start: day.and_time(start),
            title: String::new(),
            location: String::new(),
            notes: String::new(),
            all_day: at.is_none() && self.s.view == View::Month,
            start_date: day.format("%Y-%m-%d").to_string(),
            start_time: clock(start, h24),
            end_date: day.format("%Y-%m-%d").to_string(),
            end_time: clock(end, h24),
            cal: dir,
            repeat: Repeat::Never,
            every: true,
            confirm: false,
            problem: String::new(),
            focus: true,
        });
    }

    /// The editor on an occurrence of an event.
    fn edit(&mut self, o: &Occurrence) {
        let Some(ev) = self.store.event(o).cloned() else { return };
        let Some(cal) = self.store.calendars.get(o.cal) else { return };
        if cal.read_only {
            self.problem = Some(format!("“{}” can only be read: its events can't be changed here.", cal.name));
        }
        let h24 = self.s.clock_24;
        // A whole-day event's last day is the day before its end.
        let last = if o.all_day { (o.end - Duration::days(1)).date().max(o.start.date()) } else { o.end.date() };
        let master = if ev.recurrence_id.is_some() {
            cal.items[o.item].events.iter().find(|e| e.uid == ev.uid && e.recurrence_id.is_none()).map(|e| e.rrule.clone())
        } else {
            None
        };
        self.editor = Some(Editor {
            shown_start: o.start,
            title: ev.summary.clone(),
            location: ev.location.clone(),
            notes: ev.description.clone(),
            all_day: o.all_day,
            start_date: o.start.date().format("%Y-%m-%d").to_string(),
            start_time: clock(o.start.time(), h24),
            end_date: last.format("%Y-%m-%d").to_string(),
            end_time: clock(o.end.time(), h24),
            cal: cal.dir.clone(),
            repeat: Repeat::from_rule(master.flatten().as_deref().or(ev.rrule.as_deref())),
            every: !o.repeats,
            confirm: false,
            problem: String::new(),
            focus: false,
            was: Some((ev, cal.dir.clone(), o.original.clone())),
        });
    }

    /// Save what the editor holds. Gives why it couldn't.
    fn save_editor(&mut self) -> Result<(), String> {
        let Some(ed) = &self.editor else { return Ok(()) };
        let sd = parse_date(&ed.start_date).ok_or("The start date isn't one: write it as 2026-11-24.")?;
        let ed_date = parse_date(&ed.end_date).ok_or("The end date isn't one: write it as 2026-11-24.")?;
        let (start, end) = if ed.all_day {
            if ed_date < sd {
                return Err("It ends before it starts: pick a later end date.".into());
            }
            (When::Date(sd), When::Date(ed_date + Duration::days(1)))
        } else {
            let st = parse_time(&ed.start_time).ok_or("The start time isn't one: write it as 9:30 or 14:00.")?;
            let et = parse_time(&ed.end_time).ok_or("The end time isn't one: write it as 9:30 or 14:00.")?;
            let (s, e) = (sd.and_time(st), ed_date.and_time(et));
            if e < s {
                return Err("It ends before it starts: pick a later end.".into());
            }
            (When::from_local(s), When::from_local(e))
        };
        let title = ed.title.trim().to_string();
        let fill = |ev: &mut Event| {
            ev.summary = title.clone();
            ev.location = ed.location.trim().to_string();
            ev.description = ed.notes.trim_end().to_string();
        };
        let result = match &ed.was {
            None => {
                let mut ev = Event::new(&title, start, end);
                fill(&mut ev);
                ev.rrule = ed.repeat.rule();
                self.store.save(&ed.cal, &ev)
            }
            Some((ev, dir, original)) => {
                let repeating = ev.rrule.is_some() || ev.recurrence_id.is_some();
                if repeating && !ed.every {
                    // Only this one: a changed occurrence beside the series.
                    let mut one = ev.clone();
                    fill(&mut one);
                    one.rrule = None;
                    one.exdates.clear();
                    one.rdates.clear();
                    one.recurrence_id = Some(ev.recurrence_id.clone().unwrap_or_else(|| original.clone()));
                    one.start = start;
                    one.end = end;
                    self.store.save(dir, &one)
                } else {
                    // The series (or the one event): moved by as much as the occurrence was.
                    let master = self.store.calendars.iter().find(|c| &c.dir == dir).and_then(|c| c.items.iter().flat_map(|i| i.events.iter()).find(|e| e.uid == ev.uid && e.recurrence_id.is_none())).cloned().unwrap_or_else(|| ev.clone());
                    let mut m = master.clone();
                    fill(&mut m);
                    m.rrule = ed.repeat.rule();
                    let moved = start.local() - ed.shown_start;
                    let length = end.local() - start.local();
                    let new_start = master.start.local() + moved;
                    if ed.all_day {
                        m.start = When::Date(new_start.date());
                        m.end = When::Date((new_start + length).date().max(new_start.date() + Duration::days(1)));
                    } else {
                        m.start = keep_kind(&master.start, new_start);
                        m.end = keep_kind(&master.start, new_start + length);
                    }
                    if ed.cal != *dir {
                        m.recurrence_id = None;
                        self.store.save(&ed.cal, &m)
                    } else {
                        self.store.save(dir, &m)
                    }
                }
            }
        };
        result.map_err(|e| format!("Couldn't save the event: {e}"))
    }

    fn delete_editor(&mut self, every: bool) -> Result<(), String> {
        let Some(Editor { was: Some((ev, dir, original)), .. }) = &self.editor else { return Ok(()) };
        let repeating = ev.rrule.is_some() || ev.recurrence_id.is_some();
        let r = if repeating && !every { self.store.delete_occurrence(dir, &ev.uid, ev.recurrence_id.as_ref().unwrap_or(original)) } else { self.store.delete(dir, &ev.uid) };
        r.map_err(|e| format!("Couldn't delete the event: {e}"))
    }

    fn keys(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() {
            return;
        }
        if self.editor.is_some() || self.cal_edit.is_some() || self.trash_ask.is_some() || self.import.is_some() || self.account_edit.is_some() || self.account_ask.is_some() {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.editor = None;
                self.cal_edit = None;
                self.trash_ask = None;
                self.import = None;
                self.account_edit = None;
                self.account_ask = None;
                if let Some(flag) = self.signing_in.take() {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            return;
        }
        let k = |key: Key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
        if k(Key::T) {
            self.day = Local::now().date_naive();
            self.mini = self.day.with_day(1).unwrap_or(self.day);
        }
        if k(Key::ArrowLeft) || k(Key::J) {
            self.step(-1);
        }
        if k(Key::ArrowRight) || k(Key::K) {
            self.step(1);
        }
        for (key, v) in [(Key::D, View::Day), (Key::W, View::Week), (Key::M, View::Month), (Key::A, View::Agenda)] {
            if k(key) {
                self.set_view(v);
            }
        }
        if k(Key::N) || ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::N)) {
            self.new_event(self.day, None);
        }
    }
}

/// A new start or end (`t`, on this computer's clock) in the form the event's start was written
/// in: an event in a named zone stays in it, one in UTC stays in UTC.
fn keep_kind(like: &When, t: NaiveDateTime) -> When {
    let here = chrono::TimeZone::from_local_datetime(&Local, &t).earliest();
    match (like, here) {
        (When::Floating(_), _) => When::Floating(t),
        (When::Zoned(_, z), Some(h)) => match rivet_cal::event::zone(z) {
            Some(tz) => When::Zoned(h.with_timezone(&tz).naive_local(), z.clone()),
            None => When::from_local(t),
        },
        (When::Utc(_), Some(h)) => When::Utc(h.with_timezone(&chrono::Utc)),
        _ => When::from_local(t),
    }
}

fn days_in(y: i32, m: u32) -> u32 {
    let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) };
    next.and_then(|n| n.pred_opt()).map(|d| d.day()).unwrap_or(28)
}

// ------------------------------------------------------------------ drawing

impl eframe::App for CalendarApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let c = self.choice();
        self.look.set_fonts(&ctx, "", "", "");
        self.look.frame(&ctx, &c, self.shot.is_none());
        // Changes made elsewhere (another program, a sync) show within seconds.
        let now = ctx.input(|i| i.time);
        if let Some(l) = &self.loader {
            while let Ok((generation, store)) = l.got.try_recv() {
                // The first reading is always taken (nothing made here can be in it yet).
                if generation >= self.generation || !self.loaded {
                    self.store = store;
                    self.loaded = true;
                    self.version += 1;
                }
            }
            if now - self.checked > 3.0 {
                self.checked = now;
                let _ = l.ask.send(self.generation);
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(3));
        if self.loaded
            && let Some(uid) = self.open_uid.take()
        {
            let found = self.occurrences(self.day, self.day + Duration::days(1)).into_iter().find(|o| uid.is_empty() || self.store.event(o).is_some_and(|e| e.uid == uid));
            if let Some(o) = found {
                self.edit(&o);
            }
        }
        self.tend_feeds(&ctx);
        self.keys(&ctx);
        let v = ctx.global_style().visuals.clone();
        let accent = v.selection.bg_fill;
        let side_fill = mix(v.window_fill, v.panel_fill, 0.6);
        egui::Panel::top("calendar-toolbar").show_separator_line(false).frame(egui::Frame::NONE.fill(v.panel_fill).inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 8 })).show(ui, |ui| self.toolbar(ui, accent));
        if self.s.show_sidebar {
            egui::Panel::left("calendar-sidebar").show_separator_line(false).resizable(false).exact_size(232.0).frame(egui::Frame::NONE.fill(side_fill).inner_margin(egui::Margin { left: 12, right: 12, top: 12, bottom: 10 })).show(ui, |ui| self.sidebar(ui, accent, side_fill));
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(v.panel_fill).inner_margin(egui::Margin { left: 10, right: 12, top: 4, bottom: 10 })).show(ui, |ui| {
            if let Some(p) = self.problem.clone() {
                let r = egui::Frame::NONE.fill(rivet_look::theme::status(ui.ctx()).warning.gamma_multiply(0.18)).corner_radius(r_sm()).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(p);
                        ui.button("Dismiss").clicked()
                    })
                    .inner
                });
                if r.inner {
                    self.problem = None;
                }
                ui.add_space(6.0);
            }
            match self.s.view {
                View::Month => self.month(ui, accent),
                View::Week => self.days(ui, accent, 7),
                View::Day => self.days(ui, accent, 1),
                View::Agenda => self.agenda(ui, accent),
            }
        });
        self.editor_window(&ctx, accent);
        self.calendar_window(&ctx, accent);
        self.trash_window(&ctx);
        self.import_window(&ctx, accent);
        self.account_window(&ctx, accent);
        self.account_remove_window(&ctx);
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }
}

impl CalendarApp {
    fn toolbar(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let v = ui.visuals().clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
            if ui.button("Today").on_hover_text("T").clicked() {
                self.day = Local::now().date_naive();
                self.mini = self.day.with_day(1).unwrap_or(self.day);
            }
            if ui.button("‹").on_hover_text("Back (Left)").clicked() {
                self.step(-1);
            }
            if ui.button("›").on_hover_text("On (Right)").clicked() {
                self.step(1);
            }
            ui.add_space(6.0);
            let title = match self.s.view {
                View::Month | View::Agenda => self.day.format("%B %Y").to_string(),
                View::Day => self.day.format("%A, %-d %B %Y").to_string(),
                View::Week => {
                    let a = week_of(self.day, self.s.monday_first);
                    let b = a + Duration::days(6);
                    if a.month() == b.month() { format!("{}–{} {}", a.day(), b.day(), b.format("%B %Y")) } else { format!("{} – {}", a.format("%-d %b"), b.format("%-d %b %Y")) }
                }
            };
            ui.label(egui::RichText::new(title).font(ui_font(18.0)).color(v.strong_text_color()));
            // What's being done, or was just done.
            let reading = (!self.loaded).then(|| "Reading your calendars…".to_string());
            let status = reading.or_else(|| self.job.as_ref().map(|(w, _)| w.clone()).filter(|w| !w.is_empty())).or_else(|| self.note.clone());
            if let Some(t) = status {
                ui.add_space(10.0);
                if ui.add(egui::Label::new(egui::RichText::new(t).size(12.5).color(v.weak_text_color())).sense(Sense::click())).on_hover_text("Click to dismiss").clicked() {
                    self.note = None;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let new = egui::Button::new(egui::RichText::new("New Event").color(rivet_look::theme::StatusColors::on(accent))).fill(accent);
                if ui.add(new).on_hover_text("N").clicked() {
                    self.new_event(self.day, None);
                }
                ui.add_space(8.0);
                // The views, as one row of choices.
                for (view, label, key) in [(View::Agenda, "Agenda", "A"), (View::Month, "Month", "M"), (View::Week, "Week", "W"), (View::Day, "Day", "D")] {
                    if ui.selectable_label(self.s.view == view, label).on_hover_text(key).clicked() {
                        self.set_view(view);
                    }
                }
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, accent: Color32, side: Color32) {
        let v = ui.visuals().clone();
        // ---- the small month
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(self.mini.format("%B %Y").to_string()).font(ui_font(13.5)).color(v.strong_text_color()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if ui.small_button("›").clicked() {
                    self.mini = add_months(self.mini, 1);
                }
                if ui.small_button("‹").clicked() {
                    self.mini = add_months(self.mini, -1);
                }
            });
        });
        ui.add_space(4.0);
        let w = ui.available_width();
        let cell = w / 7.0;
        let first = week_of(self.mini, self.s.monday_first);
        let busy: std::collections::HashSet<NaiveDate> = self.occurrences(first, first + Duration::days(42)).iter().flat_map(|o| o.days().collect::<Vec<_>>()).collect();
        let (head, _) = ui.allocate_exact_size(Vec2::new(w, 18.0), Sense::hover());
        for i in 0..7 {
            let d = first + Duration::days(i);
            let name = d.format("%a").to_string();
            ui.painter().text(Pos2::new(head.left() + cell * (i as f32 + 0.5), head.center().y), Align2::CENTER_CENTER, &name[..1], ui_font(10.5), v.weak_text_color());
        }
        let today = Local::now().date_naive();
        for row in 0..6 {
            let (line, _) = ui.allocate_exact_size(Vec2::new(w, 26.0), Sense::hover());
            for col in 0..7 {
                let d = first + Duration::days(row * 7 + col);
                let r = Rect::from_center_size(Pos2::new(line.left() + cell * (col as f32 + 0.5), line.center().y), Vec2::splat(24.0));
                let resp = ui.interact(r, egui::Id::unique(("mini", d)), Sense::click());
                let p = ui.painter();
                if d == today {
                    p.circle_filled(r.center(), 11.0, accent);
                } else if d == self.day {
                    p.circle_filled(r.center(), 11.0, mix(side, v.text_color(), 0.14));
                } else if resp.hovered() {
                    p.circle_filled(r.center(), 11.0, mix(side, v.text_color(), 0.07));
                }
                let ink = if d == today { rivet_look::theme::StatusColors::on(accent) } else if d.month() != self.mini.month() { v.weak_text_color().gamma_multiply(0.6) } else { v.text_color() };
                p.text(r.center(), Align2::CENTER_CENTER, d.day().to_string(), ui_font(11.5), ink);
                if busy.contains(&d) && d != today {
                    p.circle_filled(Pos2::new(r.center().x, r.bottom() - 2.0), 1.6, accent);
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.day = d;
                    if d.month() != self.mini.month() {
                        self.mini = d.with_day(1).unwrap_or(d);
                    }
                }
            }
        }
        ui.add_space(14.0);
        // ---- the calendars
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("CALENDARS").font(ui_font(10.5)).color(v.weak_text_color().gamma_multiply(0.85)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let plus = ui.small_button("+").on_hover_text("New Calendar or Subscription");
                let color = COLORS[self.store.calendars.len() % COLORS.len()];
                egui::Popup::menu(&plus).show(|ui| {
                    if ui.button("New Calendar…").clicked() {
                        self.cal_edit = Some(CalEdit { dir: None, name: String::new(), color, feed: None });
                        ui.close();
                    }
                    if ui.button("Subscribe to a Feed…").clicked() {
                        self.cal_edit = Some(CalEdit { dir: None, name: String::new(), color, feed: Some(String::new()) });
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Add a Google Account…").clicked() {
                        self.account_edit = Some(AccountEdit::new(Some(Provider::Google)));
                        ui.close();
                    }
                    if ui.button("Add a Microsoft Account…").clicked() {
                        self.account_edit = Some(AccountEdit::new(Some(Provider::Microsoft)));
                        ui.close();
                    }
                    if ui.button("Add a CalDAV Account…").clicked() {
                        self.account_edit = Some(AccountEdit::new(None));
                        ui.close();
                    }
                });
            });
        });
        ui.add_space(2.0);
        let mut toggle: Option<String> = None;
        let mut edit: Option<usize> = None;
        let mut refresh: Option<PathBuf> = None;
        let mut trash: Option<usize> = None;
        let mut sync_now = false;
        let mut remove_account: Option<String> = None;
        let accounts = rivet_cal::caldav::accounts();
        for (i, cal) in self.store.calendars.iter().enumerate() {
            let shown = !self.s.hidden.contains(&cal.id);
            let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, r_md(), mix(side, v.text_color(), 0.06));
            }
            let c = self.color_of(i);
            let bx = Rect::from_center_size(Pos2::new(r.left() + 16.0, r.center().y), Vec2::splat(14.0));
            if shown {
                ui.painter().rect_filled(bx, r_xs(), c);
                ui.painter().add(egui::Shape::line(vec![bx.center() + Vec2::new(-3.5, 0.0), bx.center() + Vec2::new(-1.0, 2.8), bx.center() + Vec2::new(3.8, -2.8)], Stroke::new(1.6, ink_on(c))));
            } else {
                ui.painter().rect_stroke(bx, r_xs(), Stroke::new(1.5, c), egui::StrokeKind::Inside);
            }
            // A subscription says so; another calendar that can't be written too.
            let name = if cal.feed.is_some() || !cal.read_only { cal.name.clone() } else { format!("{} (read only)", cal.name) };
            let g = ui.painter().layout_no_wrap(name, ui_font(13.0), if shown { v.text_color() } else { v.weak_text_color() });
            ui.painter().galley(Pos2::new(bx.right() + 10.0, r.center().y - g.size().y / 2.0), g, v.text_color());
            if cal.feed.is_some() {
                // A subscription: a small mark at the end of its row.
                let at = Pos2::new(r.right() - 12.0, r.center().y);
                // Two arcs and a dot: the usual sign for a feed.
                for rad in [3.5f32, 6.5] {
                    let arc = (0..=8).map(|j| at + Vec2::new(-3.0, 3.0) + Vec2::angled(-std::f32::consts::FRAC_PI_2 + j as f32 / 8.0 * std::f32::consts::FRAC_PI_2) * rad).collect();
                    ui.painter().add(egui::Shape::line(arc, Stroke::new(1.3, v.weak_text_color())));
                }
                ui.painter().circle_filled(at + Vec2::new(-3.0, 3.0), 1.3, v.weak_text_color());
            }
            let tip = match (&cal.feed, &cal.account) {
                (Some(url), _) => {
                    let when = rivet_cal::feed::fetched(&cal.dir).and_then(|t| t.elapsed().ok()).map(|a| ago(a.as_secs())).unwrap_or_else(|| "not yet".into());
                    format!("Subscribed to {url}\nUpdated {when}")
                }
                (None, Some(id)) => {
                    let name = accounts.iter().find(|a| &a.id == id).map(|a| a.name.clone()).unwrap_or_else(|| id.clone());
                    format!("Synced with {name}")
                }
                _ => String::new(),
            };
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            let resp = if tip.is_empty() { resp } else { resp.on_hover_text(tip) };
            if resp.clicked() {
                toggle = Some(cal.id.clone());
            }
            resp.context_menu(|ui| {
                if ui.button("Rename or Recolor…").clicked() {
                    edit = Some(i);
                    ui.close();
                }
                if cal.feed.is_some() && ui.button("Refresh Now").clicked() {
                    refresh = Some(cal.dir.clone());
                    ui.close();
                }
                if cal.account.is_some() && ui.button("Sync Now").clicked() {
                    sync_now = true;
                    ui.close();
                }
                ui.separator();
                match &cal.account {
                    // A server's calendar goes with its account (deleting it on the server is for the server).
                    Some(id) => {
                        if ui.button("Remove Account…").clicked() {
                            remove_account = Some(id.clone());
                            ui.close();
                        }
                    }
                    None => {
                        if ui.button(if cal.feed.is_some() { "Unsubscribe…" } else { "Move to Trash…" }).clicked() {
                            trash = Some(i);
                            ui.close();
                        }
                    }
                }
            });
        }
        if let Some(id) = toggle {
            match self.s.hidden.iter().position(|h| *h == id) {
                Some(i) => {
                    self.s.hidden.remove(i);
                }
                None => self.s.hidden.push(id),
            }
            self.s.save();
        }
        if let Some(i) = edit {
            let c = &self.store.calendars[i];
            self.cal_edit = Some(CalEdit { dir: Some(c.dir.clone()), name: c.name.clone(), color: c.color.unwrap_or(COLORS[i % COLORS.len()]), feed: None });
        }
        if let Some(i) = trash {
            let c = &self.store.calendars[i];
            self.trash_ask = Some((c.dir.clone(), c.name.clone()));
        }
        if sync_now {
            self.sync_soon = Some(ui.ctx().input(|i| i.time));
        }
        if let Some(id) = remove_account {
            let name = accounts.iter().find(|a| a.id == id).map(|a| a.name.clone()).unwrap_or_else(|| id.clone());
            self.account_ask = Some((id, name));
        }
        if let Some(dir) = refresh
            && self.job.is_none()
        {
            let name = self.store.calendars.iter().find(|c| c.dir == dir).map(|c| c.name.clone()).unwrap_or_default();
            let ctx = ui.ctx().clone();
            self.run(&ctx, &format!("Refreshing {name}…"), move || rivet_cal::feed::refresh(&dir).map(|n| format!("{name}: {}", events_word(n))));
        }
        if self.store.calendars.is_empty() {
            ui.label(egui::RichText::new("No calendars yet: make one with +.").color(v.weak_text_color()).size(12.5));
        }
    }

    /// An event's chip in a month cell or the all-day strip.
    fn chip(&self, ui: &mut egui::Ui, r: Rect, o: &Occurrence, id: egui::Id) -> egui::Response {
        let v = ui.visuals().clone();
        let resp = ui.interact(r, id, Sense::click());
        let c = self.color_of(o.cal);
        let title = self.store.event(o).map(|e| if e.summary.trim().is_empty() { "Untitled event".to_string() } else { e.summary.clone() }).unwrap_or_default();
        let p = ui.painter();
        if o.all_day {
            p.rect_filled(r, r_xs(), if resp.hovered() { mix(c, Color32::WHITE, 0.12) } else { c });
            self.also_stripes(p, r, o);
            let g = one_line(p, &title, 11.5, ink_on(c), r.width() - 10.0 - 4.0 * o.also.len().min(3) as f32);
            p.galley(Pos2::new(r.left() + 6.0, r.center().y - g.size().y / 2.0), g, ink_on(c));
        } else {
            if resp.hovered() {
                p.rect_filled(r, r_xs(), mix(v.panel_fill, v.text_color(), 0.07));
            }
            // A dot for each calendar it's in, overlapping, its own on top.
            let dots: Vec<Color32> = std::iter::once(c).chain(self.also_colors(o)).collect();
            let more = 4.0 * (dots.len() - 1) as f32;
            for (k, d) in dots.iter().enumerate().rev() {
                let at = Pos2::new(r.left() + 6.0 + 4.0 * k as f32, r.center().y);
                if k > 0 {
                    p.circle_filled(at, 4.0, v.panel_fill);
                }
                p.circle_filled(at, 3.0, *d);
            }
            let time = clock(o.start.time(), self.s.clock_24);
            let tg = p.layout_no_wrap(time, ui_font(11.0), v.weak_text_color());
            let tw = tg.size().x + more;
            p.galley(Pos2::new(r.left() + 13.0 + more, r.center().y - tg.size().y / 2.0), tg, v.weak_text_color());
            let g = one_line(p, &title, 11.5, v.text_color(), r.width() - 18.0 - tw);
            p.galley(Pos2::new(r.left() + 17.0 + tw, r.center().y - g.size().y / 2.0), g, v.text_color());
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        match self.in_calendars(o) {
            Some(t) => resp.on_hover_text(t),
            None => resp,
        }
    }

    fn month(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let v = ui.visuals().clone();
        let first_of = self.day.with_day(1).unwrap_or(self.day);
        let first = week_of(first_of, self.s.monday_first);
        let all = self.occurrences(first, first + Duration::days(42));
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, Sense::hover());
        let head = 26.0;
        let (cw, ch) = (area.width() / 7.0, (area.height() - head) / 6.0);
        let line = Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.09));
        let today = Local::now().date_naive();
        for i in 0..7 {
            let d = first + Duration::days(i);
            ui.painter().text(Pos2::new(area.left() + cw * (i as f32 + 0.5), area.top() + head / 2.0), Align2::CENTER_CENTER, d.format("%a").to_string(), ui_font(11.5), v.weak_text_color());
        }
        let mut open: Option<Occurrence> = None;
        let mut new_on: Option<NaiveDate> = None;
        let mut go_day: Option<NaiveDate> = None;
        for row in 0..6 {
            for col in 0..7 {
                let d = first + Duration::days(row * 7 + col);
                let r = Rect::from_min_size(Pos2::new(area.left() + cw * col as f32, area.top() + head + ch * row as f32), Vec2::new(cw, ch));
                let resp = ui.interact(r, egui::Id::unique(("month-cell", d)), Sense::click());
                let p = ui.painter();
                if d == self.day {
                    p.rect_filled(r.shrink(1.0), r_sm(), mix(v.panel_fill, accent, 0.08));
                }
                p.rect_stroke(r, 0.0, line, egui::StrokeKind::Inside);
                // The day's number: today's on the accent.
                let n = Pos2::new(r.left() + 16.0, r.top() + 14.0);
                if d == today {
                    p.circle_filled(n, 11.0, accent);
                }
                let ink = if d == today { rivet_look::theme::StatusColors::on(accent) } else if d.month() != first_of.month() { v.weak_text_color().gamma_multiply(0.6) } else { v.text_color() };
                p.text(n, Align2::CENTER_CENTER, d.day().to_string(), ui_font(12.0), ink);
                let mine: Vec<&Occurrence> = all.iter().filter(|o| o.days().any(|x| x == d)).collect();
                let room = ((r.height() - 30.0) / (CHIP + 2.0)).floor().max(0.0) as usize;
                let fits = if mine.len() > room { room.saturating_sub(1) } else { mine.len() };
                for (k, o) in mine.iter().take(fits).enumerate() {
                    let cr = Rect::from_min_size(Pos2::new(r.left() + 4.0, r.top() + 28.0 + k as f32 * (CHIP + 2.0)), Vec2::new(r.width() - 8.0, CHIP));
                    if self.chip(ui, cr, o, egui::Id::unique(("chip", d, o.cal, o.item, o.ev, o.start))).clicked() {
                        open = Some((*o).clone());
                    }
                }
                if mine.len() > fits {
                    let mr = Rect::from_min_size(Pos2::new(r.left() + 4.0, r.top() + 28.0 + fits as f32 * (CHIP + 2.0)), Vec2::new(r.width() - 8.0, CHIP));
                    let more = ui.interact(mr, egui::Id::unique(("more", d)), Sense::click());
                    ui.painter().text(Pos2::new(mr.left() + 6.0, mr.center().y), Align2::LEFT_CENTER, format!("{} more", mine.len() - fits), ui_font(11.5), if more.hovered() { v.text_color() } else { v.weak_text_color() });
                    if more.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        go_day = Some(d);
                    }
                }
                if resp.double_clicked() {
                    new_on = Some(d);
                } else if resp.clicked() {
                    self.day = d;
                }
            }
        }
        if let Some(o) = open {
            self.edit(&o);
        }
        if let Some(d) = new_on {
            self.day = d;
            self.new_event(d, None);
        }
        if let Some(d) = go_day {
            self.day = d;
            self.set_view(View::Day);
        }
    }

    /// The week (`n` = 7) or one day: a strip of whole-day events, then the hours.
    fn days(&mut self, ui: &mut egui::Ui, accent: Color32, n: i64) {
        let v = ui.visuals().clone();
        let first = if n == 7 { week_of(self.day, self.s.monday_first) } else { self.day };
        let all = self.occurrences(first, first + Duration::days(n));
        let today = Local::now().date_naive();
        let line = Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.09));
        let w = ui.available_width();
        let col = (w - GUTTER) / n as f32;
        // ---- the days' names
        let (head, _) = ui.allocate_exact_size(Vec2::new(w, 44.0), Sense::hover());
        for i in 0..n {
            let d = first + Duration::days(i);
            let x = head.left() + GUTTER + col * (i as f32 + 0.5);
            let p = ui.painter();
            p.text(Pos2::new(x, head.top() + 11.0), Align2::CENTER_CENTER, d.format("%a").to_string(), ui_font(11.0), v.weak_text_color());
            let c = Pos2::new(x, head.top() + 30.0);
            if d == today {
                p.circle_filled(c, 13.0, accent);
            }
            p.text(c, Align2::CENTER_CENTER, d.day().to_string(), ui_font(15.0), if d == today { rivet_look::theme::StatusColors::on(accent) } else { v.text_color() });
        }
        // ---- whole-day events
        let whole: Vec<&Occurrence> = all.iter().filter(|o| o.all_day).collect();
        let rows = (0..n).map(|i| whole.iter().filter(|o| o.days().any(|x| x == first + Duration::days(i))).count()).max().unwrap_or(0).min(4);
        let mut open: Option<Occurrence> = None;
        if rows > 0 {
            let (strip, _) = ui.allocate_exact_size(Vec2::new(w, rows as f32 * (CHIP + 2.0) + 6.0), Sense::hover());
            ui.painter().text(Pos2::new(strip.left() + GUTTER - 8.0, strip.top() + CHIP / 2.0 + 2.0), Align2::RIGHT_CENTER, "All day", ui_font(10.5), v.weak_text_color());
            for i in 0..n {
                let d = first + Duration::days(i);
                for (k, o) in whole.iter().filter(|o| o.days().any(|x| x == d)).take(rows).enumerate() {
                    let r = Rect::from_min_size(Pos2::new(strip.left() + GUTTER + col * i as f32 + 2.0, strip.top() + 2.0 + k as f32 * (CHIP + 2.0)), Vec2::new(col - 4.0, CHIP));
                    if self.chip(ui, r, o, egui::Id::unique(("allday", d, o.cal, o.item, o.ev))).clicked() {
                        open = Some((*o).clone());
                    }
                }
            }
            ui.painter().hline(strip.x_range(), strip.bottom(), line);
        }
        // ---- the hours
        let mut new_at: Option<NaiveDateTime> = None;
        let mut area = egui::ScrollArea::vertical().id_salt(("calendar-hours", n)).auto_shrink([false, false]);
        if !self.scrolled {
            self.scrolled = true;
            area = area.vertical_scroll_offset(HOUR * 7.5);
        }
        area.show(ui, |ui| {
            let (grid, _) = ui.allocate_exact_size(Vec2::new(w, HOUR * 24.0), Sense::hover());
            for h in 0..24 {
                let y = grid.top() + h as f32 * HOUR;
                ui.painter().hline(egui::Rangef::new(grid.left() + GUTTER, grid.right()), y, line);
                if h > 0 {
                    let label = clock(NaiveTime::from_hms_opt(h, 0, 0).unwrap_or(NaiveTime::MIN), self.s.clock_24);
                    ui.painter().text(Pos2::new(grid.left() + GUTTER - 8.0, y), Align2::RIGHT_CENTER, label, ui_font(10.5), v.weak_text_color());
                }
            }
            for i in 0..n {
                let d = first + Duration::days(i);
                let x0 = grid.left() + GUTTER + col * i as f32;
                ui.painter().vline(x0, grid.y_range(), line);
                let colr = Rect::from_min_size(Pos2::new(x0, grid.top()), Vec2::new(col, grid.height()));
                // Double-click an empty hour: a new event there.
                let bg = ui.interact(colr, egui::Id::unique(("day-col", d)), Sense::click());
                if bg.double_clicked()
                    && let Some(p) = bg.interact_pointer_pos()
                {
                    let mins = (((p.y - grid.top()) / HOUR * 60.0) / 30.0).floor() as i64 * 30;
                    new_at = Some(d.and_time(NaiveTime::MIN) + Duration::minutes(mins.clamp(0, 23 * 60 + 30)));
                }
                // The day's timed events, side by side where they overlap.
                let (lo, hi) = (d.and_time(NaiveTime::MIN), (d + Duration::days(1)).and_time(NaiveTime::MIN));
                let mine: Vec<&Occurrence> = all.iter().filter(|o| !o.all_day && o.start < hi && (o.end > lo || o.start >= lo)).collect();
                for (o, slot, of) in side_by_side(&mine, lo, hi) {
                    let s = o.start.max(lo);
                    // Never shorter than a line of text.
                    let e = o.end.min(hi).max(s + Duration::minutes(28));
                    let y0 = grid.top() + (s - lo).num_minutes() as f32 / 60.0 * HOUR;
                    let y1 = grid.top() + (e - lo).num_minutes() as f32 / 60.0 * HOUR;
                    let wid = (col - 6.0) / of as f32;
                    let r = Rect::from_min_max(Pos2::new(x0 + 3.0 + wid * slot as f32, y0 + 1.0), Pos2::new(x0 + 3.0 + wid * (slot + 1) as f32 - 2.0, y1 - 1.0));
                    let resp = ui.interact(r, egui::Id::unique(("block", d, o.cal, o.item, o.ev, o.start)), Sense::click());
                    let c = self.color_of(o.cal);
                    let p = ui.painter();
                    p.rect_filled(r, r_sm(), if resp.hovered() { mix(c, Color32::WHITE, 0.12) } else { c.gamma_multiply(0.92) });
                    self.also_stripes(p, r.shrink(1.0), o);
                    let ink = ink_on(c);
                    let title = self.store.event(o).map(|e| if e.summary.trim().is_empty() { "Untitled event".to_string() } else { e.summary.clone() }).unwrap_or_default();
                    let clip = p.with_clip_rect(r.shrink(1.0));
                    let g = one_line(&clip, &title, 12.0, ink, r.width() - 10.0);
                    let gh = g.size().y;
                    // A short one: its title in the middle of it, its time left out.
                    let top = if r.height() < 34.0 { r.center().y - gh / 2.0 } else { r.top() + 4.0 };
                    clip.galley(Pos2::new(r.left() + 6.0, top), g, ink);
                    if r.height() >= 34.0 {
                        let when = format!("{} – {}", clock(o.start.time(), self.s.clock_24), clock(o.end.time(), self.s.clock_24));
                        clip.text(Pos2::new(r.left() + 6.0, r.top() + 6.0 + gh), Align2::LEFT_TOP, when, ui_font(10.5), ink.gamma_multiply(0.85));
                    }
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    let resp = match self.in_calendars(o) {
                        Some(t) => resp.on_hover_text(t),
                        None => resp,
                    };
                    if resp.clicked() {
                        open = Some((*o).clone());
                    }
                }
                // Now, as a line across today.
                if d == today {
                    let t = Local::now().naive_local();
                    let y = grid.top() + (t - lo).num_minutes() as f32 / 60.0 * HOUR;
                    let red = rivet_look::theme::status(ui.ctx()).danger;
                    ui.painter().hline(egui::Rangef::new(x0, x0 + col), y, Stroke::new(2.0, red));
                    ui.painter().circle_filled(Pos2::new(x0, y), 4.0, red);
                }
            }
        });
        if let Some(o) = open {
            self.edit(&o);
        }
        if let Some(t) = new_at {
            self.day = t.date();
            self.new_event(t.date(), Some(t.time()));
        }
    }

    fn agenda(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let v = ui.visuals().clone();
        const DAYS: i64 = 60;
        let all = self.occurrences(self.day, self.day + Duration::days(DAYS));
        let mut open: Option<Occurrence> = None;
        egui::ScrollArea::vertical().id_salt("calendar-agenda").auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(760.0));
            if all.is_empty() {
                ui.add_space(30.0);
                ui.label(egui::RichText::new(format!("Nothing in the {DAYS} days from {}. Press N to add an event.", self.day.format("%-d %B"))).color(v.weak_text_color()));
                return;
            }
            let today = Local::now().date_naive();
            let mut last: Option<NaiveDate> = None;
            for o in &all {
                let d = o.start.date().max(self.day);
                if last != Some(d) {
                    last = Some(d);
                    ui.add_space(10.0);
                    let label = if d == today { format!("Today · {}", d.format("%A %-d %B")) } else { d.format("%A %-d %B").to_string() };
                    ui.label(egui::RichText::new(label).font(ui_font(13.0)).color(if d == today { accent } else { v.strong_text_color() }));
                    ui.add_space(2.0);
                }
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, r_sm(), mix(v.panel_fill, v.text_color(), 0.06));
                }
                // Its calendar's colour as a bar, split between the calendars it's in.
                let colors: Vec<Color32> = std::iter::once(self.color_of(o.cal)).chain(self.also_colors(o)).collect();
                let bar = Rect::from_min_size(Pos2::new(r.left() + 8.0, r.top() + 8.0), Vec2::new(4.0, r.height() - 16.0));
                let part = bar.height() / colors.len() as f32;
                for (k, c) in colors.iter().enumerate() {
                    let seg = Rect::from_min_size(Pos2::new(bar.left(), bar.top() + part * k as f32), Vec2::new(4.0, part));
                    ui.painter().rect_filled(seg, if colors.len() == 1 { 2.0 } else { 0.0 }, *c);
                }
                let when = if o.all_day { "All day".to_string() } else { format!("{} – {}", clock(o.start.time(), self.s.clock_24), clock(o.end.time(), self.s.clock_24)) };
                ui.painter().text(Pos2::new(r.left() + 22.0, r.center().y), Align2::LEFT_CENTER, &when, ui_font(12.0), v.weak_text_color());
                if let Some(ev) = self.store.event(o) {
                    let title = if ev.summary.trim().is_empty() { "Untitled event" } else { ev.summary.as_str() };
                    let g = one_line(ui.painter(), title, 13.5, v.text_color(), r.width() - 160.0);
                    let gw = g.size().x;
                    ui.painter().galley(Pos2::new(r.left() + 140.0, r.center().y - g.size().y / 2.0), g, v.text_color());
                    if !ev.location.is_empty() {
                        let lg = one_line(ui.painter(), &ev.location, 12.0, v.weak_text_color(), r.width() - 160.0 - gw - 12.0);
                        ui.painter().galley(Pos2::new(r.left() + 152.0 + gw, r.center().y - lg.size().y / 2.0), lg, v.weak_text_color());
                    }
                }
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                let resp = match self.in_calendars(o) {
                    Some(t) => resp.on_hover_text(t),
                    None => resp,
                };
                if resp.clicked() {
                    open = Some(o.clone());
                }
            }
        });
        if let Some(o) = open {
            self.edit(&o);
        }
    }

    fn editor_window(&mut self, ctx: &egui::Context, accent: Color32) {
        if self.editor.is_none() {
            return;
        }
        let title = if self.editor.as_ref().is_some_and(|e| e.was.is_some()) { "Event" } else { "New Event" };
        let chrome = windows::Chrome { title: title.into(), icon: None, close: None, flush: false };
        let area = ctx.content_rect().shrink(16.0);
        let writable = self.writable();
        let h24 = self.s.clock_24;
        let read_only = self.editor.as_ref().and_then(|e| e.was.as_ref()).is_some_and(|(_, dir, _)| self.store.calendars.iter().any(|c| &c.dir == dir && c.read_only));
        let mut act: Option<&'static str> = None;
        let mut win = std::mem::replace(&mut self.editor_win, WinState::new(41, Vec2::ZERO));
        let Some(ed) = self.editor.as_mut() else { return };
        let shown = windows::show(ctx, &mut win, &chrome, area, Vec2::new(380.0, 420.0), |ui| {
            let v = ui.visuals().clone();
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            let title_field = ui.add(egui::TextEdit::singleline(&mut ed.title).hint_text("Title").font(ui_font(17.0)).desired_width(f32::INFINITY));
            if ed.focus {
                ed.focus = false;
                title_field.request_focus();
            }
            ui.add_space(2.0);
            egui::Grid::new("event-fields").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("All day");
                ui.checkbox(&mut ed.all_day, "");
                ui.end_row();
                ui.label("Starts");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut ed.start_date).desired_width(104.0).hint_text("2026-11-24"));
                    if !ed.all_day {
                        ui.add(egui::TextEdit::singleline(&mut ed.start_time).desired_width(74.0).hint_text(if h24 { "09:00" } else { "9:00 AM" }));
                    }
                });
                ui.end_row();
                ui.label("Ends");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut ed.end_date).desired_width(104.0).hint_text("2026-11-24"));
                    if !ed.all_day {
                        ui.add(egui::TextEdit::singleline(&mut ed.end_time).desired_width(74.0).hint_text(if h24 { "10:00" } else { "10:00 AM" }));
                    }
                });
                ui.end_row();
                ui.label("Calendar");
                let name = writable.iter().find(|(d, _)| *d == ed.cal).map(|(_, n)| n.clone()).unwrap_or_else(|| "Read-only calendar".into());
                egui::ComboBox::from_id_salt("event-calendar").selected_text(name).width(200.0).show_ui(ui, |ui| {
                    for (dir, n) in &writable {
                        ui.selectable_value(&mut ed.cal, dir.clone(), n);
                    }
                });
                ui.end_row();
                ui.label("Repeats");
                egui::ComboBox::from_id_salt("event-repeat").selected_text(ed.repeat.label()).width(200.0).show_ui(ui, |ui| {
                    let mut options = vec![Repeat::Never, Repeat::Daily, Repeat::Weekdays, Repeat::Weekly, Repeat::Monthly, Repeat::Yearly];
                    if let Repeat::Other(r) = &ed.repeat {
                        options.push(Repeat::Other(r.clone()));
                    }
                    for r in options {
                        let label = r.label();
                        ui.selectable_value(&mut ed.repeat, r, label);
                    }
                });
                ui.end_row();
                ui.label("Place");
                ui.add(egui::TextEdit::singleline(&mut ed.location).desired_width(f32::INFINITY).hint_text("Optional"));
                ui.end_row();
            });
            ui.add(egui::TextEdit::multiline(&mut ed.notes).desired_rows(4).desired_width(f32::INFINITY).hint_text("Notes"));
            // A repeating event: is this change for all of them or this one?
            let repeating = ed.was.as_ref().is_some_and(|(e, _, _)| e.rrule.is_some() || e.recurrence_id.is_some());
            if repeating {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Change").color(v.weak_text_color()));
                    ui.selectable_value(&mut ed.every, false, "This One");
                    ui.selectable_value(&mut ed.every, true, "Every One");
                });
            }
            if !ed.problem.is_empty() {
                ui.label(egui::RichText::new(&ed.problem).color(rivet_look::theme::status(ui.ctx()).danger).size(12.5));
            }
            ui.add_space(4.0);
            if ed.confirm {
                let what = if ed.title.trim().is_empty() { "this event".to_string() } else { format!("“{}”", ed.title.trim()) };
                let (head, body) = if repeating && !ed.every { (format!("Delete this occurrence of {what}?"), "The others stay. This can't be undone.") } else { (format!("Delete {what}?"), "It's removed from the calendar for good.") };
                ui.label(egui::RichText::new(head).font(ui_font(14.0)).color(v.strong_text_color()));
                ui.label(egui::RichText::new(body).color(v.weak_text_color()).size(12.5));
                ui.horizontal(|ui| {
                    if rivet_look::widgets::danger_button(ui, "Delete Event").clicked() {
                        act = Some("delete");
                    }
                    if ui.button("Keep It").clicked() {
                        ed.confirm = false;
                    }
                });
                return;
            }
            ui.horizontal(|ui| {
                if ed.was.is_some() && !read_only && ui.button("Delete…").clicked() {
                    ed.confirm = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = egui::Button::new(egui::RichText::new("Save").color(rivet_look::theme::StatusColors::on(accent))).fill(accent);
                    if ui.add_enabled(!read_only, save).clicked() {
                        act = Some("save");
                    }
                    if ui.button("Cancel").clicked() {
                        act = Some("cancel");
                    }
                });
            });
        });
        self.editor_win = win;
        if !shown.open {
            self.editor = None;
            return;
        }
        match act {
            Some("save") => match self.save_editor() {
                Ok(()) => {
                    self.editor = None;
                    self.sync_soon = Some(ctx.input(|i| i.time) + 3.0);
                    self.changed();
                }
                Err(p) => {
                    if let Some(ed) = self.editor.as_mut() {
                        ed.problem = p;
                    }
                }
            },
            Some("delete") => {
                let every = self.editor.as_ref().is_some_and(|e| e.every);
                match self.delete_editor(every) {
                    Ok(()) => {
                        self.editor = None;
                        self.sync_soon = Some(ctx.input(|i| i.time) + 3.0);
                        self.changed();
                    }
                    Err(p) => {
                        if let Some(ed) = self.editor.as_mut() {
                            ed.problem = p;
                            ed.confirm = false;
                        }
                    }
                }
            }
            Some("cancel") => self.editor = None,
            _ => {}
        }
    }

    fn calendar_window(&mut self, ctx: &egui::Context, accent: Color32) {
        let Some(ce) = self.cal_edit.as_mut() else { return };
        let title = match (&ce.dir, &ce.feed) {
            (Some(_), _) => "Calendar",
            (None, Some(_)) => "Subscribe to a Feed",
            (None, None) => "New Calendar",
        };
        let chrome = windows::Chrome { title: title.into(), icon: None, close: None, flush: false };
        let busy = self.job.is_some();
        let area = ctx.content_rect().shrink(16.0);
        let mut act = None;
        let shown = windows::show(ctx, &mut self.cal_win, &chrome, area, Vec2::new(320.0, 200.0), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);
            if let Some(url) = &mut ce.feed {
                ui.add(egui::TextEdit::singleline(url).hint_text("webcal:// or https:// address").desired_width(f32::INFINITY));
            }
            let hint = if ce.feed.is_some() { "Name (the feed's own when left empty)" } else { "Name" };
            ui.add(egui::TextEdit::singleline(&mut ce.name).hint_text(hint).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                for c in COLORS {
                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                    ui.painter().circle_filled(r.center(), 10.0, rgb(c));
                    if ce.color == c {
                        ui.painter().circle_stroke(r.center(), 12.0, Stroke::new(2.0, ui.visuals().text_color()));
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        ce.color = c;
                    }
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = match (&ce.dir, &ce.feed) {
                    (Some(_), _) => "Save",
                    (None, Some(_)) if busy => "Subscribing…",
                    (None, Some(_)) => "Subscribe",
                    (None, None) => "Make Calendar",
                };
                let ok = egui::Button::new(egui::RichText::new(label).color(rivet_look::theme::StatusColors::on(accent))).fill(accent);
                let ready = match &ce.feed {
                    Some(u) if ce.dir.is_none() => !u.trim().is_empty() && !busy,
                    _ => !ce.name.trim().is_empty(),
                };
                if ui.add_enabled(ready, ok).clicked() {
                    act = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    act = Some(false);
                }
            });
        });
        if !shown.open || act == Some(false) {
            self.cal_edit = None;
            return;
        }
        // Subscribing reads the feed first, on a thread; the dialog stays until it's done.
        if act == Some(true)
            && let Some(CalEdit { dir: None, feed: Some(url), name, color }) = self.cal_edit.as_ref().map(|c| CalEdit { dir: c.dir.clone(), feed: c.feed.clone(), name: c.name.clone(), color: c.color })
        {
            let root = self.root.clone();
            self.run(ctx, "Subscribing…", move || rivet_cal::feed::subscribe(&root, &url, &name, color).map(|dir| {
                let n = std::fs::read_dir(&dir).map(|r| r.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "ics")).count()).unwrap_or(0);
                format!("Subscribed: {}", events_word(n))
            }));
            return;
        }
        if act == Some(true)
            && let Some(ce) = self.cal_edit.take()
        {
            let r = match &ce.dir {
                Some(d) => self.store.set_calendar(d, ce.name.trim(), ce.color),
                None => self.store.create_calendar(ce.name.trim(), ce.color).map(|_| ()),
            };
            if let Err(e) = r {
                self.problem = Some(format!("Couldn't save the calendar: {e}"));
            }
            self.changed();
        }
    }
}

/// An address to subscribe to (else it's a file to import).
pub fn is_feed_address(s: &str) -> bool {
    let s = s.trim().to_ascii_lowercase();
    s.starts_with("webcal://") || s.starts_with("webcals://") || s.starts_with("https://") || s.starts_with("http://")
}

/// A file's name, for saying which.
fn file_label(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

fn events_word(n: usize) -> String {
    if n == 1 { "1 event".into() } else { format!("{n} events") }
}

/// How long ago, in a few words.
fn ago(secs: u64) -> String {
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86_400),
    }
}

impl CalendarApp {
    /// Asking before a calendar goes to the trash.
    fn trash_window(&mut self, ctx: &egui::Context) {
        let Some((dir, name)) = self.trash_ask.clone() else { return };
        let feed = self.store.calendars.iter().any(|c| c.dir == dir && c.feed.is_some());
        let chrome = windows::Chrome { title: if feed { "Unsubscribe".into() } else { "Move to Trash".into() }, icon: None, close: None, flush: false };
        let mut win = WinState::new(43, Vec2::new(380.0, 170.0));
        let mut act = None;
        let shown = windows::show(ctx, &mut win, &chrome, ctx.content_rect().shrink(16.0), Vec2::new(340.0, 150.0), |ui| {
            let v = ui.visuals().clone();
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            let (head, body) = if feed { (format!("Unsubscribe from “{name}”?"), "Its events go from your calendar. Subscribe again to have them back.") } else { (format!("Move “{name}” to the Trash?"), "Its events go with it. Put it back from the Trash to have them again.") };
            ui.label(egui::RichText::new(head).font(ui_font(14.5)).color(v.strong_text_color()));
            ui.label(egui::RichText::new(body).color(v.weak_text_color()));
            ui.add_space(4.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if rivet_look::widgets::danger_button(ui, if feed { "Unsubscribe" } else { "Move to Trash" }).clicked() {
                    act = Some(true);
                }
                if ui.button("Keep It").clicked() {
                    act = Some(false);
                }
            });
        });
        if !shown.open || act == Some(false) {
            self.trash_ask = None;
        }
        if act == Some(true) {
            self.trash_ask = None;
            if let Err(e) = self.store.trash_calendar(&dir) {
                self.problem = Some(format!("Couldn't move “{name}” to the Trash: {e}"));
            }
            self.s.hidden.retain(|h| Some(h.as_str()) != dir.file_name().and_then(|n| n.to_str()));
            self.changed();
        }
    }

    /// Adding an account.
    fn account_window(&mut self, ctx: &egui::Context, accent: Color32) {
        let busy = self.job.is_some();
        let waiting = self.signing_in.is_some();
        let Some(ae) = self.account_edit.as_mut() else { return };
        let title = match ae.provider {
            Some(p) => format!("Add a {} Account", p.name()),
            None => "Add a CalDAV Account".into(),
        };
        let chrome = windows::Chrome { title, icon: None, close: None, flush: false };
        // As tall as its fields: a server's four, a name alone, or a name and a client.
        let tall = match ae.provider {
            None => 300.0,
            Some(p) if ae.own && p.builtin().is_some() => 290.0,
            Some(_) if ae.own => 260.0,
            Some(_) => 220.0,
        };
        let mut win = WinState::new(45, Vec2::new(440.0, tall));
        let mut act = None;
        let shown = windows::show(ctx, &mut win, &chrome, ctx.content_rect().shrink(16.0), Vec2::new(400.0, tall), |ui| {
            let v = ui.visuals().clone();
            let weak = |ui: &mut egui::Ui, text: &str| {
                ui.label(egui::RichText::new(text).color(v.weak_text_color()).size(12.5));
            };
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);
            let ready = match ae.provider {
                None => {
                    egui::Grid::new("account-fields").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                        ui.label("Server");
                        ui.add(egui::TextEdit::singleline(&mut ae.url).hint_text("nextcloud.example.org").desired_width(260.0));
                        ui.end_row();
                        ui.label("User name");
                        ui.add(egui::TextEdit::singleline(&mut ae.user).desired_width(260.0));
                        ui.end_row();
                        ui.label("Password");
                        ui.add(egui::TextEdit::singleline(&mut ae.pass).password(true).desired_width(260.0));
                        ui.end_row();
                        ui.label("Name");
                        ui.add(egui::TextEdit::singleline(&mut ae.name).hint_text("Optional").desired_width(260.0));
                        ui.end_row();
                    });
                    weak(ui, "For iCloud and Fastmail, use an app password made on their website. The password is kept in your keyring.");
                    !ae.url.trim().is_empty() && !ae.user.trim().is_empty() && !ae.pass.is_empty()
                }
                Some(p) => {
                    let builtin = p.builtin().is_some();
                    ui.add_enabled_ui(!waiting, |ui| {
                        egui::Grid::new("account-fields").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                            ui.label("Name");
                            ui.add(egui::TextEdit::singleline(&mut ae.name).hint_text("Your address, if left empty").desired_width(270.0));
                            ui.end_row();
                            if builtin {
                                ui.label("");
                                ui.checkbox(&mut ae.own, "Sign in with my own client ID");
                                ui.end_row();
                            }
                            if ae.own {
                                ui.label("Client ID");
                                ui.add(egui::TextEdit::singleline(&mut ae.client_id).desired_width(270.0));
                                ui.end_row();
                                if p.wants_secret() {
                                    ui.label("Client secret");
                                    ui.add(egui::TextEdit::singleline(&mut ae.client_secret).password(true).desired_width(270.0));
                                    ui.end_row();
                                }
                            }
                        });
                    });
                    if waiting {
                        weak(ui, "Finish signing in in your browser. Rivet Calendar is waiting for it.");
                    } else if !builtin {
                        weak(ui, &format!("This copy of Rivet Calendar has no {} client ID of its own, so it signs in with one you registered.", p.name()));
                    } else {
                        weak(ui, &format!("Your browser opens to sign in to {}. Rivet Calendar then syncs your calendars with it.", p.name()));
                    }
                    !ae.own || (!ae.client_id.trim().is_empty() && (!p.wants_secret() || !ae.client_secret.trim().is_empty()))
                }
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = match (ae.provider, busy) {
                    (None, true) => "Adding…",
                    (None, false) => "Add Account",
                    (Some(_), true) => "Signing In…",
                    (Some(_), false) => "Sign In",
                };
                let ok = egui::Button::new(egui::RichText::new(label).color(rivet_look::theme::StatusColors::on(accent))).fill(accent);
                if ui.add_enabled(!busy && ready, ok).clicked() {
                    act = Some(true);
                }
                if ui.button(if waiting { "Stop" } else { "Cancel" }).clicked() {
                    act = Some(false);
                }
            });
        });
        if act == Some(false) && waiting {
            // Stop waiting for the browser; the dialog stays for another try.
            if let Some(flag) = self.signing_in.take() {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            return;
        }
        if !shown.open || act == Some(false) {
            if let Some(flag) = self.signing_in.take() {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            self.account_edit = None;
            return;
        }
        if act == Some(true)
            && let Some(ae) = self.account_edit.as_ref()
        {
            let root = self.root.clone();
            let name = ae.name.clone();
            let added = |d: Vec<PathBuf>| format!("Added {} calendar{}", d.len(), if d.len() == 1 { "" } else { "s" });
            match ae.provider {
                None => {
                    let (url, user, pass) = (ae.url.clone(), ae.user.clone(), ae.pass.clone());
                    self.run(ctx, "Adding the account…", move || rivet_cal::caldav::add_account(&root, &url, &user, &pass, &name).map(added));
                }
                Some(p) => {
                    let own = ae.own.then(|| Client { id: ae.client_id.clone(), secret: ae.client_secret.clone() });
                    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    self.signing_in = Some(flag.clone());
                    self.run(ctx, "Waiting for the browser…", move || rivet_cal::caldav::add_signed_in(&root, p, own, &name, &flag).map(added));
                }
            }
        }
    }

    /// Asking before an account (and its calendars here) is removed.
    fn account_remove_window(&mut self, ctx: &egui::Context) {
        let Some((id, name)) = self.account_ask.clone() else { return };
        let chrome = windows::Chrome { title: "Remove Account".into(), icon: None, close: None, flush: false };
        let mut win = WinState::new(46, Vec2::new(400.0, 170.0));
        let mut act = None;
        let shown = windows::show(ctx, &mut win, &chrome, ctx.content_rect().shrink(16.0), Vec2::new(360.0, 150.0), |ui| {
            let v = ui.visuals().clone();
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            ui.label(egui::RichText::new(format!("Remove “{name}”?")).font(ui_font(14.5)).color(v.strong_text_color()));
            ui.label(egui::RichText::new("Its calendars here go to the Trash. On the server they stay as they are.").color(v.weak_text_color()));
            ui.add_space(4.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if rivet_look::widgets::danger_button(ui, "Remove Account").clicked() {
                    act = Some(true);
                }
                if ui.button("Keep It").clicked() {
                    act = Some(false);
                }
            });
        });
        if !shown.open || act == Some(false) {
            self.account_ask = None;
        }
        if act == Some(true) {
            self.account_ask = None;
            if let Err(e) = rivet_cal::caldav::remove_account(&self.root, &id) {
                self.problem = Some(format!("Couldn't remove “{name}”: {e}"));
            }
            self.changed();
        }
    }

    /// Putting an `.ics` file's events in a calendar.
    fn import_window(&mut self, ctx: &egui::Context, accent: Color32) {
        if self.import.is_none() {
            return;
        }
        let writable = self.writable();
        let mut act = None;
        let mut win = WinState::new(44, Vec2::new(380.0, 190.0));
        let Some(im) = self.import.as_mut() else { return };
        let chrome = windows::Chrome { title: "Add to Calendar".into(), icon: None, close: None, flush: false };
        let shown = windows::show(ctx, &mut win, &chrome, ctx.content_rect().shrink(16.0), Vec2::new(340.0, 170.0), |ui| {
            let v = ui.visuals().clone();
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);
            ui.label(egui::RichText::new(format!("{} from {}", events_word(im.count), im.file)).font(ui_font(14.5)).color(v.strong_text_color()));
            ui.horizontal(|ui| {
                ui.label("Into");
                let name = writable.iter().find(|(d, _)| *d == im.cal).map(|(_, n)| n.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("import-calendar").selected_text(name).width(220.0).show_ui(ui, |ui| {
                    for (dir, n) in &writable {
                        ui.selectable_value(&mut im.cal, dir.clone(), n);
                    }
                });
            });
            ui.label(egui::RichText::new("An event already in that calendar is replaced by the file's.").color(v.weak_text_color()).size(12.5));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = egui::Button::new(egui::RichText::new("Add Events").color(rivet_look::theme::StatusColors::on(accent))).fill(accent);
                if ui.add(ok).clicked() {
                    act = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    act = Some(false);
                }
            });
        });
        if !shown.open || act == Some(false) {
            self.import = None;
        }
        if act == Some(true)
            && let Some(im) = self.import.take()
        {
            match self.store.import(&im.cal, &im.text) {
                Ok(n) => {
                    self.note = Some(format!("Added {}", events_word(n)));
                    self.sync_soon = Some(ctx.input(|i| i.time) + 3.0);
                }
                Err(e) => self.problem = Some(format!("Couldn't add the events: {e}")),
            }
            self.changed();
        }
    }
}

fn add_months(first: NaiveDate, by: i32) -> NaiveDate {
    let m = first.year() * 12 + first.month0() as i32 + by;
    NaiveDate::from_ymd_opt(m.div_euclid(12), m.rem_euclid(12) as u32 + 1, 1).unwrap_or(first)
}

/// Text on one line, cut with … to fit.
fn one_line(p: &egui::Painter, text: &str, size: f32, color: Color32, max: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.replace('\n', " "), egui::TextFormat { font_id: ui_font(size), color, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: max.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    p.layout_job(job)
}

/// A day's timed events placed in columns: each with its column and how many share its group
/// (events that overlap one another, directly or through others).
fn side_by_side<'a>(evs: &[&'a Occurrence], lo: NaiveDateTime, hi: NaiveDateTime) -> Vec<(&'a Occurrence, usize, usize)> {
    let mut sorted: Vec<&Occurrence> = evs.to_vec();
    sorted.sort_by_key(|o| (o.start, std::cmp::Reverse(o.end)));
    let end_of = |o: &Occurrence| o.end.min(hi).max(o.start.max(lo) + Duration::minutes(28));
    let mut out = Vec::new();
    let mut group: Vec<(&Occurrence, usize)> = Vec::new();
    let mut cols: Vec<NaiveDateTime> = Vec::new();
    let mut group_end = lo;
    let flush = |group: &mut Vec<(&'a Occurrence, usize)>, cols: &mut Vec<NaiveDateTime>, out: &mut Vec<(&'a Occurrence, usize, usize)>| {
        let n = cols.len().max(1);
        out.extend(group.drain(..).map(|(o, c)| (o, c, n)));
        cols.clear();
    };
    for o in sorted {
        let s = o.start.max(lo);
        if !group.is_empty() && s >= group_end {
            flush(&mut group, &mut cols, &mut out);
        }
        let c = match cols.iter().position(|e| *e <= s) {
            Some(c) => c,
            None => {
                cols.push(s);
                cols.len() - 1
            }
        };
        cols[c] = end_of(o);
        group_end = group_end.max(end_of(o));
        group.push((o, c));
    }
    flush(&mut group, &mut cols, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday;

    #[test]
    fn times_read_as_people_type_them() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0);
        assert_eq!(parse_time("9"), t(9, 0));
        assert_eq!(parse_time("9:30"), t(9, 30));
        assert_eq!(parse_time("0930"), t(9, 30));
        assert_eq!(parse_time("2:30 pm"), t(14, 30));
        assert_eq!(parse_time("12am"), t(0, 0));
        assert_eq!(parse_time("12 PM"), t(12, 0));
        assert_eq!(parse_time("25:00"), None);
    }

    #[test]
    fn weeks_start_on_the_chosen_day() {
        let wed = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        assert_eq!(week_of(wed, true).weekday(), Weekday::Mon);
        assert_eq!(week_of(wed, false).weekday(), Weekday::Sun);
    }

    #[test]
    fn overlapping_events_share_the_width() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        let o = |h: u32, e: u32| Occurrence { cal: 0, item: 0, ev: 0, start: d.and_hms_opt(h, 0, 0).unwrap(), end: d.and_hms_opt(e, 0, 0).unwrap(), all_day: false, original: When::Date(d), repeats: false, also: Vec::new() };
        let (a, b, c) = (o(9, 11), o(10, 12), o(13, 14));
        let placed = side_by_side(&[&a, &b, &c], d.and_time(NaiveTime::MIN), (d + Duration::days(1)).and_time(NaiveTime::MIN));
        let get = |x: &Occurrence| placed.iter().find(|p| std::ptr::eq(p.0, x)).map(|p| (p.1, p.2)).unwrap();
        assert_eq!(get(&a), (0, 2));
        assert_eq!(get(&b), (1, 2));
        assert_eq!(get(&c), (0, 1));
    }

    #[test]
    fn repeat_choices_keep_rules_they_dont_make() {
        assert_eq!(Repeat::from_rule(Some("FREQ=WEEKLY")), Repeat::Weekly);
        assert_eq!(Repeat::from_rule(Some("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR")), Repeat::Weekdays);
        let odd = Repeat::from_rule(Some("FREQ=MONTHLY;BYDAY=2TU"));
        assert_eq!(odd.rule().as_deref(), Some("FREQ=MONTHLY;BYDAY=2TU"));
    }
}

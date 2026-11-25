//! Subscribed calendars: an iCalendar feed at an address (`webcal://`, `https://`), kept as a
//! calendar folder like any other, but read-only and refreshed from its address. The folder
//! holds `.rivet-feed` (the address, and how often to look again), `.read-only`, and one `.ics`
//! file per event, as the layout has it. A refresh writes only the files that changed and takes
//! away the events the feed no longer has.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use crate::ics::{self, Component, Prop};

/// How often a feed is looked at again, unless its file says.
pub const EVERY: Duration = Duration::from_secs(6 * 3600);
/// The largest feed read (a whole year of a busy shared calendar fits well within it).
const MOST_BYTES: &str = "30000000";

/// A subscribed calendar's settings, from its `.rivet-feed`.
#[derive(Clone, Debug, PartialEq)]
pub struct Feed {
    pub url: String,
    pub every: Duration,
}

/// The feed a calendar folder follows, if it's one.
pub fn of(dir: &Path) -> Option<Feed> {
    let text = std::fs::read_to_string(dir.join(".rivet-feed")).ok()?;
    let mut feed = Feed { url: String::new(), every: EVERY };
    for line in text.lines() {
        match line.split_once('=') {
            Some(("url", v)) => feed.url = v.trim().to_string(),
            Some(("every_hours", v)) => feed.every = v.trim().parse::<u64>().map(|h| Duration::from_secs(h.max(1) * 3600)).unwrap_or(EVERY),
            _ => {}
        }
    }
    (!feed.url.is_empty()).then_some(feed)
}

/// When a feed was last read from its address.
pub fn fetched(dir: &Path) -> Option<SystemTime> {
    std::fs::metadata(dir.join(".rivet-feed-fetched")).and_then(|m| m.modified()).ok()
}

/// Whether it's time to look at a feed again.
pub fn due(dir: &Path) -> bool {
    let Some(f) = of(dir) else { return false };
    fetched(dir).and_then(|t| t.elapsed().ok()).is_none_or(|age| age >= f.every)
}

/// The address as curl takes it: `webcal://` is plain https.
pub fn http_url(url: &str) -> String {
    let u = url.trim();
    for (from, to) in [("webcals://", "https://"), ("webcal://", "https://")] {
        if let Some(rest) = u.strip_prefix(from) {
            return format!("{to}{rest}");
        }
    }
    u.to_string()
}

/// The feed's text, downloaded with curl. Says why it couldn't be.
pub fn download(url: &str) -> Result<String, String> {
    let url = http_url(url);
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("That isn't a web address: it should start with webcal:// or https://.".into());
    }
    let out = Command::new("curl")
        .args(["-sSfL", "--max-time", "30", "--max-filesize", MOST_BYTES, "-H", "Accept: text/calendar, */*", "--", &url])
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "Feeds are read with curl, which isn't installed. Install curl to subscribe.".to_string())?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        let why = why.trim().trim_start_matches("curl: ").trim_start_matches(|c: char| c == '(' || c.is_ascii_digit() || c == ')').trim();
        return Err(if why.is_empty() { "The address didn't answer.".into() } else { format!("The address didn't give a calendar ({why}).") });
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if !text.contains("BEGIN:VCALENDAR") {
        return Err("The address answered, but not with a calendar.".into());
    }
    Ok(text)
}

/// A feed's own name and colour, when it gives them (`X-WR-CALNAME`, `X-APPLE-CALENDAR-COLOR`).
pub fn title(text: &str) -> (Option<String>, Option<[u8; 3]>) {
    let Some(cal) = ics::parse(text).into_iter().find(|c| c.name == "VCALENDAR") else { return (None, None) };
    let name = Some(cal.text("X-WR-CALNAME")).filter(|n| !n.trim().is_empty());
    let color = cal.prop("X-APPLE-CALENDAR-COLOR").and_then(|p| crate::store::parse_color(&p.value));
    (name, color)
}

/// A file's worth of events for each UID (an event and its changed occurrences together), with
/// the time zones and calendar properties around them.
pub fn split(text: &str) -> BTreeMap<String, Component> {
    let mut out = BTreeMap::new();
    let Some(cal) = ics::parse(text).into_iter().find(|c| c.name == "VCALENDAR") else { return out };
    let zones: Vec<Component> = cal.subs("VTIMEZONE").cloned().collect();
    let mut groups: BTreeMap<String, Vec<Component>> = BTreeMap::new();
    for ev in cal.subs.iter().filter(|c| c.name == "VEVENT") {
        let uid = ev.text("UID");
        // No UID (some feeds leave it out): one from what it is, so it's the same next time.
        let uid = if uid.trim().is_empty() {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            ev.write().hash(&mut h);
            format!("{:x}@feed", h.finish())
        } else {
            uid
        };
        let mut ev = ev.clone();
        ev.set(Prop::new("UID", uid.clone()));
        groups.entry(uid).or_default().push(ev);
    }
    for (uid, events) in groups {
        let mut w = Component::new("VCALENDAR");
        w.props = cal.props.iter().filter(|p| matches!(p.name.as_str(), "VERSION" | "PRODID" | "CALSCALE")).cloned().collect();
        if w.prop("VERSION").is_none() {
            w.set(Prop::new("VERSION", "2.0"));
        }
        // Only the zones its events name.
        let named: Vec<String> = events.iter().flat_map(|e| e.props.iter().filter_map(|p| p.param("TZID").map(String::from))).collect();
        w.subs.extend(zones.iter().filter(|z| named.contains(&z.text("TZID"))).cloned());
        w.subs.extend(events);
        out.insert(uid, w);
    }
    out
}

/// Put a feed's events in its folder: a file for each, written only when it changed, and the
/// files of events it no longer has taken away. Gives how many events it has.
pub fn apply(dir: &Path, text: &str) -> std::io::Result<usize> {
    let files = split(text);
    let mut keep: Vec<PathBuf> = Vec::new();
    for (uid, comp) in &files {
        let path = dir.join(crate::store::file_name(uid));
        let body = comp.write();
        if std::fs::read_to_string(&path).ok().as_deref() != Some(body.as_str()) {
            crate::store::write_atomic(&path, &body)?;
        }
        keep.push(path);
    }
    for e in std::fs::read_dir(dir)?.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "ics") && !keep.contains(&p) {
            std::fs::remove_file(&p)?;
        }
    }
    let _ = std::fs::write(dir.join(".rivet-feed-fetched"), "");
    Ok(files.len())
}

/// Read a feed again from its address. Two at once (the app and the panel's plugin) don't both
/// read it: the second finds it busy and leaves it.
pub fn refresh(dir: &Path) -> Result<usize, String> {
    let f = of(dir).ok_or("This calendar isn't a subscription.")?;
    let busy = dir.join(".rivet-feed-busy");
    // Left behind by one that stopped half way: taken after five minutes.
    if std::fs::metadata(&busy).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a > Duration::from_secs(300)) {
        let _ = std::fs::remove_file(&busy);
    }
    if std::fs::OpenOptions::new().write(true).create_new(true).open(&busy).is_err() {
        return Err("It's being refreshed already.".into());
    }
    let result = download(&f.url).and_then(|text| apply(dir, &text).map_err(|e| format!("Couldn't save the calendar: {e}")));
    if result.is_err() {
        // Tried: not again until it's due, so an address that's down isn't asked every minute.
        let _ = std::fs::write(dir.join(".rivet-feed-fetched"), "");
    }
    let _ = std::fs::remove_file(&busy);
    result
}

/// Refresh every subscription under `root` that's due. Gives whether any changed.
pub fn refresh_due(root: &Path) -> bool {
    let mut changed = false;
    for d in std::fs::read_dir(root).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        if due(&d) {
            changed |= refresh(&d).is_ok();
        }
    }
    changed
}

/// Make a subscribed calendar: its folder, then the feed read into it. Gives its folder.
pub fn subscribe(root: &Path, url: &str, name: &str, color: [u8; 3]) -> Result<PathBuf, String> {
    let text = download(url)?;
    let (own, own_color) = title(&text);
    let name = if name.trim().is_empty() { own.unwrap_or_else(|| "Subscription".into()) } else { name.trim().to_string() };
    let mut store = crate::Store::open(root);
    let dir = store.create_calendar(&name, own_color.filter(|_| color == [0, 0, 0]).unwrap_or(color)).map_err(|e| format!("Couldn't make the calendar: {e}"))?;
    let saved = (|| -> std::io::Result<()> {
        std::fs::write(dir.join(".rivet-feed"), format!("url={}\nevery_hours={}\n", url.trim(), EVERY.as_secs() / 3600))?;
        std::fs::write(dir.join(".read-only"), "")?;
        apply(&dir, &text).map(|_| ())
    })();
    saved.map_err(|e| format!("Couldn't save the calendar: {e}"))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Example//Holidays//EN\r\nX-WR-CALNAME:Holidays\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/Paris\r\nEND:VTIMEZONE\r\nBEGIN:VTIMEZONE\r\nTZID:Asia/Tokyo\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:a\r\nSUMMARY:New Year\r\nDTSTART;VALUE=DATE:20270101\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Meeting\r\nDTSTART;TZID=Europe/Paris:20261124T090000\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nRECURRENCE-ID;TZID=Europe/Paris:20261201T090000\r\nSUMMARY:Meeting (moved)\r\nDTSTART;TZID=Europe/Paris:20261201T100000\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:No uid\r\nDTSTART;VALUE=DATE:20261225\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn a_feed_splits_into_a_file_per_event() {
        let files = split(FEED);
        assert_eq!(files.len(), 3);
        let b = &files["b"];
        assert_eq!(b.subs("VEVENT").count(), 2);
        // Only the zone it names.
        assert_eq!(b.subs("VTIMEZONE").map(|z| z.text("TZID")).collect::<Vec<_>>(), ["Europe/Paris"]);
        assert_eq!(files["a"].subs("VTIMEZONE").count(), 0);
        assert_eq!(title(FEED).0.as_deref(), Some("Holidays"));
        // The one with no UID gets the same one each time.
        assert_eq!(split(FEED).keys().collect::<Vec<_>>(), files.keys().collect::<Vec<_>>());
    }

    #[test]
    fn applying_again_takes_away_what_left_the_feed() {
        let dir = std::env::temp_dir().join(format!("rivet-cal-feed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(apply(&dir, FEED).unwrap(), 3);
        let a = dir.join("a.ics");
        let before = std::fs::metadata(&a).unwrap().modified().unwrap();
        let smaller = FEED.replace("BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:New Year\r\nDTSTART;VALUE=DATE:20270101\r\nEND:VEVENT\r\n", "");
        assert_eq!(apply(&dir, &smaller).unwrap(), 2);
        assert!(!a.exists());
        // Unchanged files aren't written again.
        let b = dir.join("b.ics");
        let b_before = std::fs::metadata(&b).unwrap().modified().unwrap();
        apply(&dir, &smaller).unwrap();
        assert_eq!(std::fs::metadata(&b).unwrap().modified().unwrap(), b_before);
        let _ = before;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn webcal_is_https() {
        assert_eq!(http_url(" webcal://example.org/a.ics "), "https://example.org/a.ics");
        assert_eq!(http_url("https://example.org/a.ics"), "https://example.org/a.ics");
    }
}

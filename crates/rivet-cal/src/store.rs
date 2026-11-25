//! Calendars on disk, in the layout vdirsyncer and khal use: a folder of calendars, each a
//! folder of `.ics` files with one event (and its changed occurrences) in each. A calendar's name
//! and colour are the files `displayname` and `color` beside its events. Any program that speaks
//! the layout (vdirsyncer for a CalDAV server, khal in a terminal) sees the same calendars.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

use crate::event::{Event, When};
use crate::expand::{Hit, hits};
use crate::ics::{self, Component, Prop};

/// One `.ics` file: the calendar component it holds, and its events.
#[derive(Clone, Debug)]
pub struct Item {
    pub path: PathBuf,
    pub mtime: Option<SystemTime>,
    /// The `VCALENDAR` around the events (its time zones and the like) without them, kept for
    /// writing back.
    pub wrapper: Component,
    pub events: Vec<Event>,
    /// Each event's start and end on this computer's clock, worked out once when it's read: a
    /// span of days then passes over thousands of events comparing times, not converting zones.
    pub spans: Vec<(NaiveDateTime, NaiveDateTime)>,
}

#[derive(Clone, Debug)]
pub struct Calendar {
    /// Its folder's name.
    pub id: String,
    pub dir: PathBuf,
    pub name: String,
    pub color: Option<[u8; 3]>,
    /// Can't be changed here (a subscription, or a folder that can't be written).
    pub read_only: bool,
    /// The address it's subscribed to, when it's a subscription.
    pub feed: Option<String>,
    /// The account it syncs with, when it's on a server.
    pub account: Option<String>,
    /// Shared, so a copy of the calendars (as one thread hands another) copies no events.
    pub items: Vec<std::sync::Arc<Item>>,
}

/// An occurrence on the calendar, with where its event is.
#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence {
    pub cal: usize,
    pub item: usize,
    pub ev: usize,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub all_day: bool,
    /// Its start in the event's own terms, which names it among a repeating event's.
    pub original: When,
    pub repeats: bool,
    /// The other calendars the same event is in at the same time (an invitation in several
    /// people's calendars), shown as this one.
    pub also: Vec<usize>,
}

impl Occurrence {
    /// The days it covers (an all-day or overnight event covers more than one).
    pub fn days(&self) -> impl Iterator<Item = NaiveDate> {
        let first = self.start.date();
        // Ending at midnight is the day before; a moment is its one day.
        let last = if self.end > self.start { (self.end - chrono::Duration::seconds(1)).date() } else { first };
        first.iter_days().take_while(move |d| *d <= last)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Store {
    pub root: PathBuf,
    pub calendars: Vec<Calendar>,
}

/// Where Rivet keeps calendars unless told otherwise.
pub fn default_root() -> PathBuf {
    rivet_xdg::places::data_home().join("rivet-calendar")
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// `#rrggbb` (or `#rrggbbaa`, as some servers write).
pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() < 6 {
        return None;
    }
    let n = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some([n(0)?, n(2)?, n(4)?])
}

fn read_item(path: &Path) -> Option<Item> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut wrapper = ics::parse(&text).into_iter().find(|c| c.name == "VCALENDAR")?;
    let events: Vec<Event> = wrapper.subs("VEVENT").filter_map(Event::from_component).collect();
    // Each event keeps its own component, so the wrapper keeps only the rest (time zones and the
    // like): every write puts the events back in from the events. Held twice, a big calendar
    // took twice the memory.
    wrapper.subs.retain(|c| c.name != "VEVENT");
    let spans = events.iter().map(|e| (e.start.local(), e.end.local())).collect();
    Some(Item { path: path.to_path_buf(), mtime: mtime(path), wrapper, events, spans })
}

fn read_calendar(dir: &Path) -> Calendar {
    let mut c = read_calendar_shell(dir);
    c.items = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ics"))).filter_map(|p| read_item(&p)).map(std::sync::Arc::new).collect();
    c.items.sort_by(|a, b| a.path.cmp(&b.path));
    c
}

/// A file name for an event: its UID, with what can't go in a name replaced.
pub(crate) fn file_name(uid: &str) -> String {
    let safe: String = uid.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.@".contains(c) { c } else { '_' }).collect();
    format!("{}.ics", safe.chars().take(120).collect::<String>())
}

/// Write a file whole or not at all: to a new file beside it, then moved over it.
pub(crate) fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("ics.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

impl Store {
    /// The calendars under `root` (each folder in it is one). A missing root has none.
    pub fn open(root: &Path) -> Store {
        let mut s = Store { root: root.to_path_buf(), calendars: Vec::new() };
        s.reload();
        s
    }

    /// Read the calendars again: only files that changed are read again.
    pub fn reload(&mut self) {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&self.root).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))).collect();
        dirs.sort();
        let old = std::mem::take(&mut self.calendars);
        for dir in dirs {
            let mut cal = match old.iter().find(|c| c.dir == dir) {
                Some(prev) => read_calendar_meta(&dir, prev),
                None => read_calendar(&dir),
            };
            cal.items.sort_by(|a, b| a.path.cmp(&b.path));
            self.calendars.push(cal);
        }
        // Named calendars first by name, the rest after.
        self.calendars.sort_by_key(|c| c.name.to_lowercase());
    }

    /// Something in the folder changed since this was read: the files' times, in one number.
    pub fn stamp(root: &Path) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for d in std::fs::read_dir(root).into_iter().flatten().flatten() {
            d.path().hash(&mut h);
            mtime(&d.path()).hash(&mut h);
            for f in std::fs::read_dir(d.path()).into_iter().flatten().flatten() {
                mtime(&f.path()).hash(&mut h);
            }
        }
        h.finish()
    }

    /// The occurrences between two days (`to` not included), earliest first; whole-day ones first
    /// on a day. A repeating event's changed occurrence replaces the one it changes. Cancelled
    /// events, and calendars in `hidden`, are left out.
    pub fn occurrences(&self, from: NaiveDate, to: NaiveDate, hidden: &[String]) -> Vec<Occurrence> {
        let (lo, hi) = (from.and_time(NaiveTime::MIN), to.and_time(NaiveTime::MIN));
        let mut out = Vec::new();
        for (ci, cal) in self.calendars.iter().enumerate() {
            if hidden.contains(&cal.id) {
                continue;
            }
            for (ii, item) in cal.items.iter().enumerate() {
                // The occurrences a changed one takes the place of.
                let moved: Vec<NaiveDateTime> = item.events.iter().filter_map(|e| e.recurrence_id.as_ref()).map(When::local).collect();
                for (ei, ev) in item.events.iter().enumerate() {
                    if ev.cancelled() {
                        continue;
                    }
                    // Most events are single ones, nowhere near the span: passed over on their times.
                    if let Some(&(s, e)) = item.spans.get(ei) {
                        let single = ev.rrule.is_none() && ev.rdates.is_empty();
                        let near = if e > s { e > lo && s < hi } else { s >= lo && s < hi };
                        if (single && !near) || (ev.rdates.is_empty() && s >= hi) {
                            continue;
                        }
                    }
                    let repeats = ev.rrule.is_some() || ev.recurrence_id.is_some();
                    for Hit { start, end, original } in hits(ev, lo, hi) {
                        if ev.recurrence_id.is_none() && ev.rrule.is_some() && moved.contains(&original.local()) {
                            continue;
                        }
                        out.push(Occurrence { cal: ci, item: ii, ev: ei, start, end, all_day: ev.all_day(), original, repeats, also: Vec::new() });
                    }
                }
            }
        }
        out.sort_by_key(|o| (o.start.date(), !o.all_day, o.start, o.end));
        self.roll_up(out)
    }

    /// The same event at the same time in more than one calendar, as one occurrence: the one in
    /// a calendar that can be changed when there is one, else the first, with the others in
    /// `also`. Events are the same by UID (an invitation keeps its UID in every calendar it's in).
    fn roll_up(&self, list: Vec<Occurrence>) -> Vec<Occurrence> {
        let mut out: Vec<Occurrence> = Vec::with_capacity(list.len());
        let mut seen: std::collections::HashMap<(&str, NaiveDateTime, NaiveDateTime), usize> = std::collections::HashMap::new();
        for o in list {
            let uid = self.event(&o).map(|e| e.uid.as_str()).unwrap_or("");
            let key = (uid, o.start, o.end);
            match seen.get(&key) {
                Some(&k) if !uid.is_empty() && out[k].cal != o.cal && !out[k].also.contains(&o.cal) => {
                    let kept = &mut out[k];
                    if self.calendars[kept.cal].read_only && !self.calendars[o.cal].read_only {
                        // The one that can be changed is the one shown; the other joins the rest.
                        let was = kept.cal;
                        let also = std::mem::take(&mut kept.also);
                        *kept = Occurrence { also, ..o };
                        kept.also.push(was);
                    } else {
                        kept.also.push(o.cal);
                    }
                }
                _ => {
                    seen.insert(key, out.len());
                    out.push(o);
                }
            }
        }
        out
    }

    pub fn event(&self, o: &Occurrence) -> Option<&Event> {
        self.calendars.get(o.cal)?.items.get(o.item)?.events.get(o.ev)
    }

    /// Make a calendar: a folder under the root, with its name and colour.
    pub fn create_calendar(&mut self, name: &str, color: [u8; 3]) -> std::io::Result<PathBuf> {
        let base: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' }).collect();
        let base = if base.trim_matches('-').is_empty() { "calendar".to_string() } else { base.trim_matches('-').to_string() };
        let mut dir = self.root.join(&base);
        let mut n = 2;
        while dir.exists() {
            dir = self.root.join(format!("{base}-{n}"));
            n += 1;
        }
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("displayname"), name)?;
        std::fs::write(dir.join("color"), format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]))?;
        self.reload();
        Ok(dir)
    }

    /// Change a calendar's name or colour (its files beside the events).
    pub fn set_calendar(&mut self, dir: &Path, name: &str, color: [u8; 3]) -> std::io::Result<()> {
        std::fs::write(dir.join("displayname"), name)?;
        std::fs::write(dir.join("color"), format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]))?;
        self.reload();
        Ok(())
    }

    /// Save an event to a calendar. One with the same UID there (in any file) is replaced in its
    /// file, along with that file's other parts; a new one gets a file of its own. Moving an event
    /// to another calendar takes it out of the one it was in.
    pub fn save(&mut self, cal_dir: &Path, ev: &Event) -> std::io::Result<()> {
        // Out of any other calendar it was in.
        for c in self.calendars.iter().filter(|c| c.dir != cal_dir) {
            if let Some(it) = c.items.iter().find(|i| i.events.iter().any(|e| e.uid == ev.uid)) {
                std::fs::remove_file(&it.path)?;
            }
        }
        let existing = self.calendars.iter().find(|c| c.dir == cal_dir).and_then(|c| c.items.iter().find(|i| i.events.iter().any(|e| e.uid == ev.uid)));
        let (path, mut wrapper, mut events) = match existing {
            Some(it) => (it.path.clone(), it.wrapper.clone(), it.events.clone()),
            None => (cal_dir.join(file_name(&ev.uid)), new_wrapper(), Vec::new()),
        };
        match events.iter().position(|e| e.uid == ev.uid && e.recurrence_id == ev.recurrence_id) {
            Some(i) => events[i] = ev.clone(),
            None => events.push(ev.clone()),
        }
        wrapper.subs.retain(|c| c.name != "VEVENT");
        wrapper.subs.extend(events.iter().map(Event::to_component));
        std::fs::create_dir_all(cal_dir)?;
        write_atomic(&path, &wrapper.write())?;
        self.reload();
        Ok(())
    }

    /// Put the events of an `.ics` file's text in a calendar: each with the UID of one already
    /// there replaces it (in its file), the rest get files of their own. Gives how many.
    pub fn import(&mut self, cal_dir: &Path, text: &str) -> std::io::Result<usize> {
        let groups = crate::feed::split(text);
        for (uid, comp) in &groups {
            let existing = self.calendars.iter().find(|c| c.dir == cal_dir).and_then(|c| c.items.iter().find(|i| i.events.iter().any(|e| &e.uid == uid))).map(|i| i.path.clone());
            write_atomic(&existing.unwrap_or_else(|| cal_dir.join(file_name(uid))), &comp.write())?;
        }
        self.reload();
        Ok(groups.len())
    }

    /// Move a calendar's folder to the desktop's trash (it can be put back from there).
    pub fn trash_calendar(&mut self, dir: &Path) -> std::io::Result<()> {
        rivet_xdg::trash::trash(dir)?;
        self.reload();
        Ok(())
    }

    /// Delete an event, every occurrence of it (its file goes when nothing else is in it).
    pub fn delete(&mut self, cal_dir: &Path, uid: &str) -> std::io::Result<()> {
        let Some(it) = self.calendars.iter().find(|c| c.dir == cal_dir).and_then(|c| c.items.iter().find(|i| i.events.iter().any(|e| e.uid == uid))) else { return Ok(()) };
        let rest: Vec<Event> = it.events.iter().filter(|e| e.uid != uid).cloned().collect();
        if rest.is_empty() {
            std::fs::remove_file(&it.path)?;
        } else {
            let mut w = it.wrapper.clone();
            w.subs.retain(|c| c.name != "VEVENT");
            w.subs.extend(rest.iter().map(Event::to_component));
            write_atomic(&it.path, &w.write())?;
        }
        self.reload();
        Ok(())
    }

    /// Delete one occurrence of a repeating event: an exception on the event, and a change made
    /// to that occurrence goes too.
    pub fn delete_occurrence(&mut self, cal_dir: &Path, uid: &str, original: &When) -> std::io::Result<()> {
        let Some(it) = self.calendars.iter().find(|c| c.dir == cal_dir).and_then(|c| c.items.iter().find(|i| i.events.iter().any(|e| e.uid == uid))) else { return Ok(()) };
        let mut events: Vec<Event> = it.events.iter().filter(|e| !(e.uid == uid && e.recurrence_id.as_ref().is_some_and(|r| r.local() == original.local()))).cloned().collect();
        if let Some(master) = events.iter_mut().find(|e| e.uid == uid && e.recurrence_id.is_none()) {
            master.exdates.push(original.clone());
        }
        let mut w = it.wrapper.clone();
        w.subs.retain(|c| c.name != "VEVENT");
        w.subs.extend(events.iter().map(Event::to_component));
        write_atomic(&it.path, &w.write())?;
        self.reload();
        Ok(())
    }
}

/// A calendar read again, reusing the files that haven't changed since `prev` was read.
fn read_calendar_meta(dir: &Path, prev: &Calendar) -> Calendar {
    let fresh_list: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ics"))).collect();
    let before: std::collections::HashMap<&Path, &std::sync::Arc<Item>> = prev.items.iter().map(|i| (i.path.as_path(), i)).collect();
    let items = fresh_list
        .into_iter()
        .filter_map(|p| match before.get(p.as_path()) {
            Some(i) if i.mtime.is_some() && i.mtime == mtime(&p) => Some(std::sync::Arc::clone(i)),
            _ => read_item(&p).map(std::sync::Arc::new),
        })
        .collect();
    let mut c = read_calendar_shell(dir);
    c.items = items;
    c
}

/// A calendar's name, colour and whether it can be written, without its events.
fn read_calendar_shell(dir: &Path) -> Calendar {
    let id = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let meta = |f: &str| std::fs::read_to_string(dir.join(f)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let read_only = dir.join(".read-only").exists() || std::fs::metadata(dir).is_ok_and(|m| m.permissions().readonly());
    let feed = crate::feed::of(dir).map(|f| f.url);
    let account = crate::caldav::link(dir).map(|l| l.account);
    Calendar { name: meta("displayname").unwrap_or_else(|| id.clone()), color: meta("color").as_deref().and_then(parse_color), id, dir: dir.to_path_buf(), read_only, feed, account, items: Vec::new() }
}

pub(crate) fn new_wrapper() -> Component {
    let mut c = Component::new("VCALENDAR");
    c.set(Prop::new("VERSION", "2.0"));
    c.set(Prop::new("PRODID", "-//Rivet//Rivet Calendar//EN"));
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rivet-cal-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    #[test]
    fn saves_finds_and_deletes_events() {
        let root = tmp("save");
        let mut s = Store::open(&root);
        let dir = s.create_calendar("Personal", [0x72, 0x87, 0xfd]).unwrap();
        assert_eq!(s.calendars[0].name, "Personal");
        assert_eq!(s.calendars[0].color, Some([0x72, 0x87, 0xfd]));
        let ev = Event::new("Lunch", When::Floating(day(24).and_hms_opt(12, 0, 0).unwrap()), When::Floating(day(24).and_hms_opt(13, 0, 0).unwrap()));
        s.save(&dir, &ev).unwrap();
        let found = s.occurrences(day(1), day(30), &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(s.event(&found[0]).unwrap().summary, "Lunch");
        // Saved again with a change: still one, changed.
        let mut ev2 = s.event(&found[0]).unwrap().clone();
        ev2.summary = "Long lunch".into();
        s.save(&dir, &ev2).unwrap();
        let found = s.occurrences(day(1), day(30), &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(s.event(&found[0]).unwrap().summary, "Long lunch");
        // Hidden calendars are left out.
        assert!(s.occurrences(day(1), day(30), &[s.calendars[0].id.clone()]).is_empty());
        s.delete(&dir, &ev.uid).unwrap();
        assert!(s.occurrences(day(1), day(30), &[]).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_same_event_in_several_calendars_shows_once() {
        let root = tmp("rollup");
        let mut s = Store::open(&root);
        let (a, b, c) = (s.create_calendar("Alice", [1, 1, 1]).unwrap(), s.create_calendar("Bob", [2, 2, 2]).unwrap(), s.create_calendar("Carol", [3, 3, 3]).unwrap());
        let at = |h: u32| When::Floating(day(24).and_hms_opt(h, 0, 0).unwrap());
        // Copies of one invitation, as a server puts them in each calendar (saving here would move
        // it from one calendar to another).
        let file = |ev: &Event| {
            let mut cal = new_wrapper();
            cal.subs.push(ev.to_component());
            cal.write()
        };
        let meeting = Event::new("Planning", at(9), at(10));
        std::fs::write(a.join("p.ics"), file(&meeting)).unwrap();
        std::fs::write(b.join("p.ics"), file(&meeting)).unwrap();
        // Alice's copy can only be looked at: one that can be changed is the one shown.
        std::fs::write(a.join(".read-only"), "").unwrap();
        // The same event at another time in Carol's calendar is a different occurrence.
        let mut moved = meeting.clone();
        (moved.start, moved.end) = (at(11), at(12));
        std::fs::write(c.join("p.ics"), file(&moved)).unwrap();
        std::fs::write(a.join("q.ics"), file(&Event::new("Alone", at(9), at(10)))).unwrap();
        s.reload();
        let found = s.occurrences(day(24), day(25), &[]);
        let name = |i: usize| s.calendars[i].name.as_str();
        let planning: Vec<(&str, Vec<&str>)> = found.iter().filter(|o| s.event(o).unwrap().summary == "Planning").map(|o| (name(o.cal), o.also.iter().map(|&i| name(i)).collect())).collect();
        assert_eq!(planning, vec![("Bob", vec!["Alice"]), ("Carol", vec![])]);
        assert_eq!(found.len(), 3);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn one_occurrence_can_be_moved_or_taken_away() {
        let root = tmp("repeat");
        let mut s = Store::open(&root);
        let dir = s.create_calendar("Work", [1, 2, 3]).unwrap();
        let start = When::Floating(day(2).and_hms_opt(9, 0, 0).unwrap());
        let mut ev = Event::new("Standup", start, When::Floating(day(2).and_hms_opt(9, 15, 0).unwrap()));
        ev.rrule = Some("FREQ=WEEKLY".into());
        s.save(&dir, &ev).unwrap();
        // Mondays the 2nd to the 23rd (the 30th is past the span's end).
        assert_eq!(s.occurrences(day(1), day(30), &[]).len(), 4);
        // The 9th moved to 10 o'clock.
        let mut moved = ev.clone();
        moved.rrule = None;
        moved.recurrence_id = Some(When::Floating(day(9).and_hms_opt(9, 0, 0).unwrap()));
        moved.start = When::Floating(day(9).and_hms_opt(10, 0, 0).unwrap());
        moved.end = When::Floating(day(9).and_hms_opt(10, 15, 0).unwrap());
        s.save(&dir, &moved).unwrap();
        let found = s.occurrences(day(1), day(30), &[]);
        assert_eq!(found.len(), 4);
        assert!(found.iter().any(|o| o.start == day(9).and_hms_opt(10, 0, 0).unwrap()));
        assert!(!found.iter().any(|o| o.start == day(9).and_hms_opt(9, 0, 0).unwrap()));
        // The 16th taken away.
        s.delete_occurrence(&dir, &ev.uid, &When::Floating(day(16).and_hms_opt(9, 0, 0).unwrap())).unwrap();
        assert_eq!(s.occurrences(day(1), day(30), &[]).len(), 3);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_overnight_event_covers_both_days() {
        let o = Occurrence { cal: 0, item: 0, ev: 0, start: day(24).and_hms_opt(22, 0, 0).unwrap(), end: day(25).and_hms_opt(2, 0, 0).unwrap(), all_day: false, original: When::Date(day(24)), repeats: false, also: Vec::new() };
        assert_eq!(o.days().collect::<Vec<_>>(), vec![day(24), day(25)]);
        let whole = Occurrence { start: day(24).and_time(NaiveTime::MIN), end: day(25).and_time(NaiveTime::MIN), all_day: true, ..o };
        assert_eq!(whole.days().collect::<Vec<_>>(), vec![day(24)]);
    }
}

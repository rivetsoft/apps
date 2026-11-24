//! An event, as read from a `VEVENT` and written back into it: the parts Rivet shows and edits,
//! over the component itself, so everything else it held (alarms, attendees, a server's own
//! properties) is kept.

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

use crate::ics::{Component, Prop};

/// A moment as iCalendar writes it.
#[derive(Clone, Debug, PartialEq)]
pub enum When {
    /// A whole day (`VALUE=DATE`).
    Date(NaiveDate),
    /// A time wherever the reader is (no zone).
    Floating(NaiveDateTime),
    Utc(DateTime<Utc>),
    /// A time in a named zone (`TZID=`).
    Zoned(NaiveDateTime, String),
}

impl When {
    /// Read a `DTSTART`, `DTEND`, `RECURRENCE-ID` or one value of an `EXDATE`.
    pub fn parse(value: &str, tzid: Option<&str>) -> Option<When> {
        let v = value.trim();
        if v.len() == 8 {
            return NaiveDate::parse_from_str(v, "%Y%m%d").ok().map(When::Date);
        }
        if let Some(u) = v.strip_suffix('Z') {
            return NaiveDateTime::parse_from_str(u, "%Y%m%dT%H%M%S").ok().map(|t| When::Utc(t.and_utc()));
        }
        let t = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
        Some(match tzid {
            Some(z) if !z.is_empty() => When::Zoned(t, z.to_string()),
            _ => When::Floating(t),
        })
    }

    /// Read a property holding one moment.
    pub fn from_prop(p: &Prop) -> Option<When> {
        When::parse(p.value.split(',').next()?, p.param("TZID"))
    }

    pub fn is_date(&self) -> bool {
        matches!(self, When::Date(_))
    }

    /// The zone it names, when one Rivet knows (IANA names, as almost every program writes).
    pub fn zone(&self) -> Option<chrono_tz::Tz> {
        match self {
            When::Zoned(_, z) => zone(z),
            When::Utc(_) => Some(chrono_tz::UTC),
            _ => None,
        }
    }

    /// The same moment on this computer's clock. A zone Rivet doesn't know reads as floating.
    pub fn local(&self) -> NaiveDateTime {
        match self {
            When::Date(d) => d.and_time(NaiveTime::MIN),
            When::Floating(t) => *t,
            When::Utc(t) => t.with_timezone(&Local).naive_local(),
            When::Zoned(t, z) => match zone(z).and_then(|tz| tz.from_local_datetime(t).earliest()) {
                Some(at) => at.with_timezone(&Local).naive_local(),
                None => *t,
            },
        }
    }

    /// As a property: `DTSTART;VALUE=DATE:20261124`, `DTSTART;TZID=…:…`, `…Z`.
    pub fn prop(&self, name: &str) -> Prop {
        match self {
            When::Date(d) => Prop::new(name, d.format("%Y%m%d").to_string()).with_param("VALUE", "DATE"),
            When::Floating(t) => Prop::new(name, t.format("%Y%m%dT%H%M%S").to_string()),
            When::Utc(t) => Prop::new(name, t.format("%Y%m%dT%H%M%SZ").to_string()),
            When::Zoned(t, z) => Prop::new(name, t.format("%Y%m%dT%H%M%S").to_string()).with_param("TZID", z),
        }
    }

    /// A time on this computer's clock as Rivet writes it: in this computer's zone by name when
    /// it has one (so a repeating event keeps its hour through daylight saving), else floating.
    pub fn from_local(t: NaiveDateTime) -> When {
        match local_zone_name() {
            Some(z) => When::Zoned(t, z),
            None => When::Floating(t),
        }
    }
}

/// A zone by its IANA name, also as some programs write it (`/Europe/Paris`, with a prefix).
pub fn zone(name: &str) -> Option<chrono_tz::Tz> {
    let n = name.trim().trim_start_matches('/');
    n.parse().ok().or_else(|| {
        // `/mozilla.org/20050126_1/Europe/Paris`: the last two parts.
        let parts: Vec<&str> = n.split('/').collect();
        (parts.len() > 2).then(|| parts[parts.len() - 2..].join("/")).and_then(|t| t.parse().ok())
    })
}

/// This computer's zone by its IANA name: `TZ`, else where `/etc/localtime` points.
pub fn local_zone_name() -> Option<String> {
    if let Ok(tz) = std::env::var("TZ") {
        let tz = tz.trim_start_matches(':');
        if zone(tz).is_some() {
            return Some(tz.to_string());
        }
    }
    let target = std::fs::read_link("/etc/localtime").ok()?;
    let s = target.to_string_lossy();
    let name = s.split("zoneinfo/").nth(1)?;
    zone(name).map(|_| name.to_string())
}

/// `DURATION`: `P1D`, `PT1H30M`, `-PT15M`, `P2W`.
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let s = s.strip_prefix('P')?;
    let (mut total, mut num, mut in_time) = (0i64, String::new(), false);
    for ch in s.chars() {
        match ch {
            'T' => in_time = true,
            '0'..='9' => num.push(ch),
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += n * match (ch, in_time) {
                    ('W', _) => 7 * 86_400,
                    ('D', _) => 86_400,
                    ('H', true) => 3600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
            }
        }
    }
    Some(Duration::seconds(if neg { -total } else { total }))
}

/// An event: one `VEVENT`. A repeating event's single changed occurrences are events of their
/// own, with the same `uid` and a `recurrence_id`.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub uid: String,
    pub summary: String,
    pub location: String,
    pub description: String,
    pub start: When,
    /// Where it ends (exclusive), from `DTEND` or `DURATION`.
    pub end: When,
    /// `RRULE`, as written (`FREQ=WEEKLY;BYDAY=MO`).
    pub rrule: Option<String>,
    pub exdates: Vec<When>,
    pub rdates: Vec<When>,
    pub recurrence_id: Option<When>,
    /// `CANCELLED`, `TENTATIVE` or `CONFIRMED`.
    pub status: String,
    /// The component itself, for everything else.
    pub comp: Component,
}

impl Event {
    /// A new event with a fresh UID.
    pub fn new(summary: &str, start: When, end: When) -> Event {
        let mut comp = Component::new("VEVENT");
        let uid = new_uid();
        comp.set(Prop::new("UID", uid.clone()));
        Event { uid, summary: summary.to_string(), location: String::new(), description: String::new(), start, end, rrule: None, exdates: Vec::new(), rdates: Vec::new(), recurrence_id: None, status: String::new(), comp }
    }

    /// Read one, or None when it has no start.
    pub fn from_component(c: &Component) -> Option<Event> {
        let start = When::from_prop(c.prop("DTSTART")?)?;
        let end = match (c.prop("DTEND").and_then(When::from_prop), c.prop("DURATION").and_then(|p| parse_duration(&p.value))) {
            (Some(e), _) => e,
            (None, Some(d)) => add(&start, d),
            // No end: a day for a whole-day event, else a moment.
            (None, None) if start.is_date() => add(&start, Duration::days(1)),
            (None, None) => start.clone(),
        };
        let list = |name: &str| -> Vec<When> { c.all(name).flat_map(|p| p.value.split(',').filter_map(|v| When::parse(v, p.param("TZID"))).collect::<Vec<_>>()).collect() };
        Some(Event {
            uid: c.text("UID"),
            summary: c.text("SUMMARY"),
            location: c.text("LOCATION"),
            description: c.text("DESCRIPTION"),
            start,
            end,
            rrule: c.prop("RRULE").map(|p| p.value.clone()).filter(|r| !r.is_empty()),
            exdates: list("EXDATE"),
            rdates: list("RDATE"),
            recurrence_id: c.prop("RECURRENCE-ID").and_then(When::from_prop),
            status: c.text("STATUS").to_ascii_uppercase(),
            comp: c.clone(),
        })
    }

    /// The component, with what was edited put in.
    pub fn to_component(&self) -> Component {
        let mut c = self.comp.clone();
        c.set(Prop::new("UID", self.uid.clone()));
        c.set(Prop::new("DTSTAMP", Utc::now().format("%Y%m%dT%H%M%SZ").to_string()));
        c.set_text("SUMMARY", &self.summary);
        c.set_text("LOCATION", &self.location);
        c.set_text("DESCRIPTION", &self.description);
        c.set(self.start.prop("DTSTART"));
        c.remove("DURATION");
        c.set(self.end.prop("DTEND"));
        match &self.rrule {
            Some(r) => c.set(Prop::new("RRULE", r.clone())),
            None => c.remove("RRULE"),
        }
        c.remove("EXDATE");
        for x in &self.exdates {
            c.props.push(x.prop("EXDATE"));
        }
        if let Some(r) = &self.recurrence_id {
            c.set(r.prop("RECURRENCE-ID"));
        }
        if !c.props.iter().any(|p| p.name == "CREATED") {
            c.set(Prop::new("CREATED", Utc::now().format("%Y%m%dT%H%M%SZ").to_string()));
        }
        c.set(Prop::new("LAST-MODIFIED", Utc::now().format("%Y%m%dT%H%M%SZ").to_string()));
        c
    }

    pub fn all_day(&self) -> bool {
        self.start.is_date()
    }

    pub fn cancelled(&self) -> bool {
        self.status == "CANCELLED"
    }

    /// How long one occurrence lasts.
    pub fn length(&self) -> Duration {
        (self.end.local() - self.start.local()).max(Duration::zero())
    }
}

/// A moment moved on by a length, kept in the same form.
pub fn add(w: &When, d: Duration) -> When {
    match w {
        When::Date(x) => When::Date(*x + Duration::days(d.num_days().max(0))),
        When::Floating(t) => When::Floating(*t + d),
        When::Utc(t) => When::Utc(*t + d),
        When::Zoned(t, z) => When::Zoned(*t + d, z.clone()),
    }
}

/// A UID no other event will have: the time, this program and a counter.
pub fn new_uid() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    format!("{:x}-{:x}-{:x}@rivet", now.as_nanos(), std::process::id(), N.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_kind_of_time() {
        assert_eq!(When::parse("20261124", None), Some(When::Date(NaiveDate::from_ymd_opt(2026, 11, 24).unwrap())));
        assert!(matches!(When::parse("20261124T090000Z", None), Some(When::Utc(_))));
        assert!(matches!(When::parse("20261124T090000", Some("Europe/Paris")), Some(When::Zoned(_, _))));
        assert!(matches!(When::parse("20261124T090000", None), Some(When::Floating(_))));
        assert_eq!(zone("/mozilla.org/20050126_1/Europe/Paris"), Some(chrono_tz::Europe::Paris));
    }

    #[test]
    fn durations_read_as_written() {
        assert_eq!(parse_duration("PT1H30M"), Some(Duration::minutes(90)));
        assert_eq!(parse_duration("-PT15M"), Some(Duration::minutes(-15)));
        assert_eq!(parse_duration("P2W"), Some(Duration::days(14)));
        assert_eq!(parse_duration("P1DT2H"), Some(Duration::hours(26)));
    }

    #[test]
    fn an_event_keeps_what_it_doesnt_edit() {
        let text = "BEGIN:VEVENT\r\nUID:x\r\nSUMMARY:Old\r\nDTSTART;VALUE=DATE:20261124\r\nX-SERVER-THING:keep me\r\nBEGIN:VALARM\r\nTRIGGER:-PT10M\r\nEND:VALARM\r\nEND:VEVENT\r\n";
        let c = crate::ics::parse(text).remove(0);
        let mut ev = Event::from_component(&c).unwrap();
        assert!(ev.all_day());
        assert_eq!(ev.end, When::Date(NaiveDate::from_ymd_opt(2026, 11, 25).unwrap()));
        ev.summary = "New".into();
        let back = ev.to_component();
        assert_eq!(back.text("SUMMARY"), "New");
        assert_eq!(back.text("X-SERVER-THING"), "keep me");
        assert_eq!(back.subs("VALARM").count(), 1);
    }
}

//! The occurrences of an event in a span of time: once for most, each time it comes round for
//! a repeating one (`RRULE`, less `EXDATE`, plus `RDATE`), worked out in the event's own zone so
//! a weekly meeting keeps its hour through daylight saving, then put on this computer's clock.

use chrono::{DateTime, Local, NaiveDateTime, NaiveTime, TimeZone};
use rrule::{RRule, Tz, Unvalidated};

use crate::event::{Event, When};

/// The most occurrences one event gives in a span (a rule that repeats every minute stops here).
const MOST: u16 = 1500;

/// One occurrence: where it is on this computer's clock, and the start it had in the event's own
/// terms (what `RECURRENCE-ID` and `EXDATE` name it by).
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub original: When,
}

/// The zone a repeating event is worked out in.
fn rule_zone(w: &When) -> Tz {
    match w.zone() {
        Some(z) => Tz::Tz(z),
        None => Tz::Local(Local),
    }
}

/// A moment in the rule's zone.
fn in_zone(w: &When, tz: &Tz) -> Option<DateTime<Tz>> {
    match w {
        When::Utc(t) => Some(t.with_timezone(tz)),
        When::Date(d) => tz.from_local_datetime(&d.and_time(NaiveTime::MIN)).earliest(),
        When::Floating(t) | When::Zoned(t, _) => tz.from_local_datetime(t).earliest(),
    }
}

/// A moment from the rule back in the start's own form.
fn same_kind(start: &When, at: &DateTime<Tz>) -> When {
    match start {
        When::Date(_) => When::Date(at.date_naive()),
        When::Floating(_) => When::Floating(at.naive_local()),
        When::Utc(_) => When::Utc(at.with_timezone(&chrono::Utc)),
        When::Zoned(_, z) => When::Zoned(at.naive_local(), z.clone()),
    }
}

/// The occurrences that overlap `from`..`to` (this computer's clock). An occurrence with no
/// length counts when it starts in the span.
pub fn hits(ev: &Event, from: NaiveDateTime, to: NaiveDateTime) -> Vec<Hit> {
    let length = ev.length();
    let overlaps = |s: NaiveDateTime, e: NaiveDateTime| if e > s { e > from && s < to } else { s >= from && s < to };
    let Some(rule) = ev.rrule.as_deref() else {
        let (s, e) = (ev.start.local(), ev.end.local());
        return if overlaps(s, e) { vec![Hit { start: s, end: e, original: ev.start.clone() }] } else { Vec::new() };
    };
    let tz = rule_zone(&ev.start);
    let Some(dtstart) = in_zone(&ev.start, &tz) else { return Vec::new() };
    let Some(set) = rule_set(rule, dtstart, &tz) else {
        // A rule that can't be read: at least the first occurrence.
        let (s, e) = (ev.start.local(), ev.end.local());
        return if overlaps(s, e) { vec![Hit { start: s, end: e, original: ev.start.clone() }] } else { Vec::new() };
    };
    let set = ev.exdates.iter().filter_map(|x| in_zone(x, &tz)).fold(set, |s, x| s.exdate(x));
    let set = ev.rdates.iter().filter_map(|x| in_zone(x, &tz)).fold(set, |s, x| s.rdate(x));
    // From far enough back to catch one that started earlier and is still going.
    let (Some(lo), Some(hi)) = (Local.from_local_datetime(&(from - length - chrono::Duration::seconds(1))).earliest(), Local.from_local_datetime(&to).earliest()) else { return Vec::new() };
    let found = set.after(lo.with_timezone(&tz)).before(hi.with_timezone(&tz)).all(MOST);
    found
        .dates
        .iter()
        .filter_map(|at| {
            let original = same_kind(&ev.start, at);
            let s = original.local();
            let e = s + length;
            overlaps(s, e).then_some(Hit { start: s, end: e, original })
        })
        .collect()
}

/// The rule built from its text. Programs write `UNTIL` in UTC, as a date, or in the event's
/// zone; it's read in the start's zone and handed on in UTC, the one form the library takes for
/// every kind of start.
fn rule_set(rule: &str, dtstart: DateTime<Tz>, tz: &Tz) -> Option<rrule::RRuleSet> {
    let mut parts: Vec<String> = Vec::new();
    let mut until: Option<DateTime<Tz>> = None;
    for part in rule.trim().trim_start_matches("RRULE:").split(';').filter(|p| !p.is_empty()) {
        match part.split_once('=') {
            Some((k, v)) if k.eq_ignore_ascii_case("UNTIL") => {
                // A date means through the end of that day.
                until = match When::parse(v, None)? {
                    When::Date(d) => tz.from_local_datetime(&d.and_hms_opt(23, 59, 59)?).earliest(),
                    When::Utc(t) => Some(t.with_timezone(tz)),
                    w => in_zone(&w, tz),
                };
            }
            _ => parts.push(part.to_string()),
        }
    }
    let mut r: RRule<Unvalidated> = parts.join(";").parse().ok()?;
    if let Some(u) = until {
        r = r.until(u.with_timezone(&Tz::UTC));
    }
    r.build(dtstart).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(y: i32, m: u32, d: u32, h: u32) -> NaiveDateTime {
        day(y, m, d).and_hms_opt(h, 0, 0).unwrap()
    }

    fn event(start: When, end: When, rule: Option<&str>) -> Event {
        let mut e = Event::new("x", start, end);
        e.rrule = rule.map(String::from);
        e
    }

    #[test]
    fn a_single_event_is_found_once_where_it_overlaps() {
        let e = event(When::Floating(at(2026, 11, 24, 9)), When::Floating(at(2026, 11, 24, 10)), None);
        assert_eq!(hits(&e, at(2026, 11, 24, 0), at(2026, 11, 25, 0)).len(), 1);
        assert!(hits(&e, at(2026, 11, 25, 0), at(2026, 11, 26, 0)).is_empty());
    }

    #[test]
    fn a_weekly_event_comes_round_less_its_exceptions() {
        let mut e = event(When::Floating(at(2026, 11, 2, 9)), When::Floating(at(2026, 11, 2, 10)), Some("FREQ=WEEKLY;BYDAY=MO"));
        e.exdates.push(When::Floating(at(2026, 11, 16, 9)));
        let found = hits(&e, at(2026, 11, 1, 0), at(2026, 12, 1, 0));
        let days: Vec<u32> = found.iter().map(|h| chrono::Datelike::day(&h.start)).collect();
        assert_eq!(days, vec![2, 9, 23, 30]);
    }

    #[test]
    fn until_written_in_utc_or_as_a_date_ends_the_rule() {
        let e = event(When::Zoned(at(2026, 11, 2, 9), "Europe/Paris".into()), When::Zoned(at(2026, 11, 2, 10), "Europe/Paris".into()), Some("FREQ=DAILY;UNTIL=20261104T235959Z"));
        assert_eq!(hits(&e, at(2026, 10, 1, 0), at(2027, 1, 1, 0)).len(), 3);
        let d = event(When::Date(day(2026, 11, 2)), When::Date(day(2026, 11, 3)), Some("FREQ=DAILY;UNTIL=20261105"));
        assert_eq!(hits(&d, at(2026, 10, 1, 0), at(2027, 1, 1, 0)).len(), 4);
    }

    #[test]
    fn a_whole_day_event_that_repeats_yearly_stays_on_its_date() {
        let e = event(When::Date(day(2020, 12, 25)), When::Date(day(2020, 12, 26)), Some("FREQ=YEARLY"));
        let found = hits(&e, at(2026, 12, 1, 0), at(2027, 1, 1, 0));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].original, When::Date(day(2026, 12, 25)));
        assert_eq!(found[0].end, at(2026, 12, 26, 0));
    }

    #[test]
    fn a_long_event_from_before_the_span_still_shows() {
        let e = event(When::Date(day(2026, 11, 20)), When::Date(day(2026, 11, 30)), None);
        assert_eq!(hits(&e, at(2026, 11, 24, 0), at(2026, 11, 25, 0)).len(), 1);
    }
}

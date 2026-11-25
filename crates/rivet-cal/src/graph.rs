//! Microsoft calendars (Outlook.com, Microsoft 365), through Microsoft Graph, which has no
//! CalDAV. Each event is turned into an `.ics` file and back, so a Microsoft calendar is a folder
//! like any other, kept in `.rivet-caldav` and synced by the same rules: what changed there comes
//! down, what changed here goes up, and when both changed, Microsoft's version is kept.
//!
//! Graph's events aren't iCalendar: Rivet keeps what it shows (title, times, place, notes,
//! whether it repeats and how) and leaves the rest (attendees, reminders, categories) as it is in
//! Microsoft. A repeating event's changed and deleted occurrences are read from the event's
//! occurrences over the months around today.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{DateTime, Datelike, Local, Months, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::{Value, json};

use crate::caldav::{Link, Login, Remote, Synced, hash, request};
use crate::event::{Event, When};
use crate::oauth::encode;

const GRAPH: &str = "https://graph.microsoft.com/v1.0";
/// An event's occurrences as read to find its changed and deleted ones.
const INSTANCE: &str = "id,changeKey,seriesMasterId,type,subject,start,end,isAllDay,originalStart,location,bodyPreview,isCancelled,showAs";
const EVENT: &str = "id,changeKey,iCalUId,subject,body,location,start,end,isAllDay,recurrence,type,isCancelled,showAs";

/// The zone times are asked for and sent in: this computer's.
fn zone() -> String {
    crate::event::local_zone_name().unwrap_or_else(|| "UTC".into())
}

/// A request to Graph, with a JSON answer. A missing thing is `404` and `Null`, for the caller.
fn call(method: &str, url: &str, login: &Login, body: Option<&Value>) -> Result<(u16, Value), String> {
    let prefer = format!("outlook.timezone=\"{}\", outlook.body-content-type=\"text\"", zone());
    let text = body.map(Value::to_string);
    let mut headers = vec![("Accept", "application/json"), ("Prefer", prefer.as_str())];
    if body.is_some() {
        headers.push(("Content-Type", "application/json"));
    }
    let r = request(method, url, login, &headers, text.as_deref())?;
    let v: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
    match r.status {
        200..=299 => Ok((r.status, v)),
        404 | 410 => Ok((404, Value::Null)),
        401 => Err("Microsoft didn't take the sign-in.".into()),
        403 => Err("Microsoft wouldn't let this account do that.".into()),
        0 => Err("Microsoft didn't answer.".into()),
        s => Err(format!("Microsoft answered {s}{}", v["error"]["message"].as_str().map(|m| format!(": {m}")).unwrap_or_else(|| ".".into()))),
    }
}

/// Every page of a list.
fn all(url: &str, login: &Login) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let mut next = url.to_string();
    loop {
        let (status, v) = call("GET", &next, login, None)?;
        if status == 404 {
            return Err("Microsoft has no such calendar any more.".into());
        }
        out.extend(v["value"].as_array().cloned().unwrap_or_default());
        match v["@odata.nextLink"].as_str() {
            Some(n) => next = n.to_string(),
            None => return Ok(out),
        }
    }
}

/// The account's address and its calendars.
pub fn calendars(login: &Login) -> Result<(String, Vec<Remote>), String> {
    let (_, me) = call("GET", &format!("{GRAPH}/me?$select=mail,userPrincipalName"), login, None)?;
    let user = me["mail"].as_str().or(me["userPrincipalName"].as_str()).unwrap_or("").to_string();
    let mut out = Vec::new();
    for c in all(&format!("{GRAPH}/me/calendars?$select=id,name,hexColor,canEdit&$top=100"), login)? {
        let Some(id) = c["id"].as_str() else { continue };
        out.push(Remote {
            url: format!("{GRAPH}/me/calendars/{}", encode(id)),
            name: c["name"].as_str().unwrap_or("Calendar").to_string(),
            color: c["hexColor"].as_str().and_then(crate::store::parse_color),
            read_only: c["canEdit"].as_bool() == Some(false),
        });
    }
    if out.is_empty() {
        return Err("Microsoft has no calendars for this account.".into());
    }
    Ok((user, out))
}

/// The months a repeating event's occurrences are read over: from two months back to over a
/// year ahead, moving a month at a time (so the record of them changes once a month, not daily).
fn window() -> (NaiveDate, NaiveDate) {
    let today = Local::now().date_naive();
    let first = today.with_day(1).unwrap_or(today);
    (first.checked_sub_months(Months::new(2)).unwrap_or(first), first.checked_add_months(Months::new(14)).unwrap_or(first))
}

fn span_query() -> String {
    let (a, b) = window();
    format!("startDateTime={a}T00:00:00&endDateTime={b}T00:00:00")
}

/// A repeating event's occurrences in the window.
fn instances_of(id: &str, login: &Login) -> Result<Vec<Value>, String> {
    all(&format!("{GRAPH}/me/events/{}/instances?{}&$select={INSTANCE}&$top=500", encode(id), span_query()), login)
}

/// What a sync compares to see whether an event changed there: its change key, and for a
/// repeating one each occurrence's too (changing one occurrence doesn't change the series').
fn version(change_key: &str, instances: &[Value]) -> String {
    if instances.is_empty() {
        return change_key.to_string();
    }
    let mut keys: Vec<String> = instances.iter().map(|i| format!("{}:{}", i["id"].as_str().unwrap_or(""), i["changeKey"].as_str().unwrap_or(""))).collect();
    keys.sort();
    format!("{change_key}+{}", hash(&keys.join(",")))
}

/// A Graph time (`{"dateTime": "2026-11-24T09:00:00.0000000", "timeZone": "…"}`) on this
/// computer's clock. Times are asked for in this computer's zone; one in another is moved to it.
fn time_of(v: &Value) -> Option<NaiveDateTime> {
    let text = v["dateTime"].as_str()?;
    let t = NaiveDateTime::parse_from_str(text.get(..19)?, "%Y-%m-%dT%H:%M:%S").ok()?;
    let tz = v["timeZone"].as_str().unwrap_or("");
    if tz.eq_ignore_ascii_case("UTC") || tz.eq_ignore_ascii_case("tzone://Microsoft/Utc") {
        return Some(t.and_utc().with_timezone(&Local).naive_local());
    }
    match crate::event::zone(tz) {
        Some(z) if Some(tz.to_string()) != crate::event::local_zone_name() => Some(z.from_local_datetime(&t).earliest()?.with_timezone(&Local).naive_local()),
        _ => Some(t),
    }
}

/// An occurrence's start before it was changed (`originalStart`, in UTC).
fn original_of(i: &Value, all_day: bool) -> Option<When> {
    let text = i["originalStart"].as_str()?;
    let utc = DateTime::parse_from_rfc3339(text).ok()?.with_timezone(&Utc);
    Some(if all_day { When::Date(utc.with_timezone(&Local).date_naive()) } else { When::Utc(utc) })
}

/// An event from Graph's JSON: a series, a single one, or one changed occurrence (`original`).
fn event_of(v: &Value, original: Option<When>) -> Option<Event> {
    let all_day = v["isAllDay"].as_bool() == Some(true);
    let (s, e) = (time_of(&v["start"])?, time_of(&v["end"])?);
    let (start, end) = if all_day { (When::Date(s.date()), When::Date(e.date().max(s.date() + chrono::Duration::days(1)))) } else { (When::from_local(s), When::from_local(e)) };
    let mut ev = Event::new(v["subject"].as_str().unwrap_or(""), start, end);
    ev.uid = v["iCalUId"].as_str().or(v["id"].as_str()).unwrap_or("").to_string();
    ev.location = v["location"]["displayName"].as_str().unwrap_or("").to_string();
    ev.description = v["body"]["content"].as_str().or(v["bodyPreview"].as_str()).unwrap_or("").trim().to_string();
    ev.status = if v["isCancelled"].as_bool() == Some(true) {
        "CANCELLED".into()
    } else if v["showAs"].as_str() == Some("tentative") {
        "TENTATIVE".into()
    } else {
        String::new()
    };
    ev.recurrence_id = original;
    Some(ev)
}

const DAYS: [(&str, &str); 7] = [("MO", "monday"), ("TU", "tuesday"), ("WE", "wednesday"), ("TH", "thursday"), ("FR", "friday"), ("SA", "saturday"), ("SU", "sunday")];
const INDEX: [(i32, &str); 5] = [(1, "first"), (2, "second"), (3, "third"), (4, "fourth"), (-1, "last")];

/// Graph's `recurrence` as an `RRULE`.
fn rrule_of(r: &Value, start: &When) -> Option<String> {
    let p = &r["pattern"];
    let code = |d: &Value| DAYS.iter().find(|(_, n)| Some(*n) == d.as_str()).map(|(c, _)| *c);
    let days: Vec<&str> = p["daysOfWeek"].as_array().into_iter().flatten().filter_map(code).collect();
    let index = INDEX.iter().find(|(_, n)| Some(*n) == p["index"].as_str()).map(|(i, _)| *i).unwrap_or(1);
    let relative = |parts: &mut Vec<String>| {
        if days.len() == 1 {
            parts.push(format!("BYDAY={index}{}", days[0]));
        } else {
            parts.push(format!("BYDAY={};BYSETPOS={index}", days.join(",")));
        }
    };
    let mut parts: Vec<String> = Vec::new();
    match p["type"].as_str()? {
        "daily" => parts.push("FREQ=DAILY".into()),
        "weekly" => {
            parts.push("FREQ=WEEKLY".into());
            if !days.is_empty() {
                parts.push(format!("BYDAY={}", days.join(",")));
            }
            if let Some(w) = code(&p["firstDayOfWeek"]) {
                parts.push(format!("WKST={w}"));
            }
        }
        "absoluteMonthly" => parts.push(format!("FREQ=MONTHLY;BYMONTHDAY={}", p["dayOfMonth"].as_u64()?)),
        "relativeMonthly" => {
            parts.push("FREQ=MONTHLY".into());
            relative(&mut parts);
        }
        "absoluteYearly" => parts.push(format!("FREQ=YEARLY;BYMONTH={};BYMONTHDAY={}", p["month"].as_u64()?, p["dayOfMonth"].as_u64()?)),
        "relativeYearly" => {
            parts.push(format!("FREQ=YEARLY;BYMONTH={}", p["month"].as_u64()?));
            relative(&mut parts);
        }
        _ => return None,
    }
    let interval = p["interval"].as_u64().unwrap_or(1);
    if interval > 1 {
        parts.push(format!("INTERVAL={interval}"));
    }
    let range = &r["range"];
    match range["type"].as_str() {
        Some("numbered") => parts.push(format!("COUNT={}", range["numberOfOccurrences"].as_u64().unwrap_or(1).max(1))),
        Some("endDate") => {
            let end = NaiveDate::parse_from_str(range["endDate"].as_str()?, "%Y-%m-%d").ok()?;
            // Through the end of that day: a date for a whole-day event, else that moment in UTC.
            if start.is_date() {
                parts.push(format!("UNTIL={}", end.format("%Y%m%d")));
            } else {
                let last = Local.from_local_datetime(&end.and_hms_opt(23, 59, 59)?).earliest()?.with_timezone(&Utc);
                parts.push(format!("UNTIL={}", last.format("%Y%m%dT%H%M%SZ")));
            }
        }
        _ => {}
    }
    Some(parts.join(";"))
}

/// An `RRULE` as Graph's `recurrence`, or why Outlook can't repeat an event that way.
fn recurrence_of(rule: &str, start: &When) -> Result<Value, String> {
    let no = || "Outlook can't repeat an event that way: change how it repeats, or make it in a calendar of your own.".to_string();
    let mut m: HashMap<String, String> = HashMap::new();
    for part in rule.trim().trim_start_matches("RRULE:").split(';') {
        if let Some((k, v)) = part.split_once('=') {
            m.insert(k.trim().to_ascii_uppercase(), v.trim().to_ascii_uppercase());
        }
    }
    let first = start.local().date();
    let name = |code: &str| DAYS.iter().find(|(c, _)| *c == code).map(|(_, n)| *n);
    // BYDAY's days, and the ordinal in front of them if there is one (`2TU`, `-1FR`).
    let mut ordinal: Option<i32> = m.get("BYSETPOS").and_then(|p| p.parse().ok());
    let mut days: Vec<&str> = Vec::new();
    for d in m.get("BYDAY").map(|s| s.split(',').collect::<Vec<_>>()).unwrap_or_default() {
        let at = d.len().saturating_sub(2);
        if at > 0 {
            ordinal = Some(d[..at].parse().map_err(|_| no())?);
        }
        days.push(name(&d[at..]).ok_or_else(no)?);
    }
    let index = |o: i32| INDEX.iter().find(|(i, _)| *i == o).map(|(_, n)| *n).ok_or_else(no);
    let one = |k: &str, default: u32| -> Result<u32, String> {
        match m.get(k) {
            Some(v) if v.contains(',') => Err(no()),
            Some(v) => v.parse().map_err(|_| no()),
            None => Ok(default),
        }
    };
    let weekday = name(&first.weekday().to_string().to_ascii_uppercase()[..2]).unwrap_or("monday");
    let interval: u32 = m.get("INTERVAL").and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
    let pattern = match m.get("FREQ").map(String::as_str) {
        Some("DAILY") if days.is_empty() => json!({ "type": "daily", "interval": interval }),
        Some("DAILY") | Some("WEEKLY") => {
            let wkst = m.get("WKST").and_then(|w| name(w)).unwrap_or("monday");
            let days = if days.is_empty() { vec![weekday] } else { days };
            json!({ "type": "weekly", "interval": interval, "daysOfWeek": days, "firstDayOfWeek": wkst })
        }
        Some("MONTHLY") => match ordinal {
            Some(o) if !days.is_empty() => json!({ "type": "relativeMonthly", "interval": interval, "daysOfWeek": days, "index": index(o)? }),
            _ => json!({ "type": "absoluteMonthly", "interval": interval, "dayOfMonth": one("BYMONTHDAY", first.day())? }),
        },
        Some("YEARLY") => {
            let month = one("BYMONTH", first.month())?;
            match ordinal {
                Some(o) if !days.is_empty() => json!({ "type": "relativeYearly", "interval": interval, "month": month, "daysOfWeek": days, "index": index(o)? }),
                _ => json!({ "type": "absoluteYearly", "interval": interval, "month": month, "dayOfMonth": one("BYMONTHDAY", first.day())? }),
            }
        }
        _ => return Err(no()),
    };
    let mut range = json!({ "type": "noEnd", "startDate": first.to_string(), "recurrenceTimeZone": zone() });
    if let Some(c) = m.get("COUNT") {
        range["type"] = "numbered".into();
        range["numberOfOccurrences"] = c.parse::<u32>().map_err(|_| no())?.into();
    } else if let Some(u) = m.get("UNTIL") {
        let last = match When::parse(u, None).ok_or_else(no)? {
            When::Date(d) => d,
            w => w.local().date(),
        };
        range["type"] = "endDate".into();
        range["endDate"] = last.to_string().into();
    }
    Ok(json!({ "pattern": pattern, "range": range }))
}

/// A time as Graph takes it: in its own zone when that's one Graph knows by name, else on this
/// computer's clock.
fn when_json(w: &When) -> Value {
    let t = |t: NaiveDateTime| t.format("%Y-%m-%dT%H:%M:%S").to_string();
    match w {
        When::Date(d) => json!({ "dateTime": t(d.and_time(chrono::NaiveTime::MIN)), "timeZone": zone() }),
        When::Utc(u) => json!({ "dateTime": t(u.naive_utc()), "timeZone": "UTC" }),
        When::Zoned(at, z) if crate::event::zone(z).is_some() => json!({ "dateTime": t(*at), "timeZone": z }),
        w => json!({ "dateTime": t(w.local()), "timeZone": zone() }),
    }
}

/// What Rivet sends for an event: what it shows and edits, and nothing else.
fn body_of(ev: &Event) -> Value {
    json!({
        "subject": ev.summary,
        "body": { "contentType": "text", "content": ev.description },
        "location": { "displayName": ev.location },
        "start": when_json(&ev.start),
        "end": when_json(&ev.end),
        "isAllDay": ev.all_day(),
    })
}

/// An event's `.ics` file from Graph: the event, and for a series its changed occurrences, and
/// its deleted ones as `EXDATE`s.
fn to_ics(m: &Value, instances: &[Value]) -> Option<String> {
    let mut master = event_of(m, None)?;
    let mut changed = Vec::new();
    if m["type"].as_str() == Some("seriesMaster") {
        master.rrule = rrule_of(&m["recurrence"], &master.start);
        let all_day = master.all_day();
        let mut present: HashSet<NaiveDateTime> = HashSet::new();
        for i in instances {
            match i["type"].as_str() {
                Some("occurrence") => {
                    if let Some(t) = time_of(&i["start"]) {
                        present.insert(if all_day { t.date().and_time(chrono::NaiveTime::MIN) } else { t });
                    }
                }
                Some("exception") => {
                    let Some(orig) = original_of(i, all_day) else { continue };
                    present.insert(orig.local());
                    if let Some(mut ev) = event_of(i, Some(orig)) {
                        ev.uid = master.uid.clone();
                        changed.push(ev);
                    }
                }
                _ => {}
            }
        }
        // An occurrence the rule has and Microsoft hasn't was deleted. The window's first and last
        // days are left alone, where an occurrence may only partly be in it.
        if master.rrule.is_some() {
            let (a, b) = window();
            let (from, to) = ((a + chrono::Duration::days(1)).and_time(chrono::NaiveTime::MIN), (b - chrono::Duration::days(1)).and_time(chrono::NaiveTime::MIN));
            for h in crate::expand::hits(&master, from, to) {
                if h.start >= from && !present.contains(&h.original.local()) {
                    master.exdates.push(h.original);
                }
            }
        }
    }
    let mut cal = crate::store::new_wrapper();
    cal.subs.push(master.to_component());
    cal.subs.extend(changed.iter().map(Event::to_component));
    Some(cal.write())
}

/// Bring one event down into `file`. Gone from Microsoft: gone here.
fn download(dir: &Path, l: &mut Link, login: &Login, id: &str, file: &str) -> Result<(), String> {
    let (status, m) = call("GET", &format!("{GRAPH}/me/events/{}?$select={EVENT}", encode(id)), login, None)?;
    if status == 404 {
        let _ = std::fs::remove_file(dir.join(file));
        l.items.remove(file);
        return Ok(());
    }
    let instances = if m["type"].as_str() == Some("seriesMaster") { instances_of(id, login)? } else { Vec::new() };
    let Some(text) = to_ics(&m, &instances) else { return Ok(()) };
    crate::store::write_atomic(&dir.join(file), &text).map_err(|e| e.to_string())?;
    l.items.insert(file.to_string(), (id.to_string(), version(m["changeKey"].as_str().unwrap_or(""), &instances), hash(&text)));
    Ok(())
}

/// Send a file up: as a new event (`id` None) or over the one it is. Its changed and deleted
/// occurrences go up too. Gives the event's id.
fn push(l: &Link, login: &Login, text: &str, id: Option<&str>) -> Result<String, String> {
    let events: Vec<Event> = crate::ics::parse(text).iter().flat_map(|c| c.subs("VEVENT").filter_map(Event::from_component).collect::<Vec<_>>()).collect();
    let master = events.iter().find(|e| e.recurrence_id.is_none()).ok_or("An event here has no start.")?;
    let mut body = body_of(master);
    match &master.rrule {
        Some(r) => body["recurrence"] = recurrence_of(r, &master.start)?,
        None if id.is_some() => body["recurrence"] = Value::Null,
        None => {}
    }
    let id = match id {
        Some(id) => {
            let (status, _) = call("PATCH", &format!("{GRAPH}/me/events/{}", encode(id)), login, Some(&body))?;
            if status == 404 {
                return Err("gone".into());
            }
            id.to_string()
        }
        None => {
            let (_, v) = call("POST", &format!("{}/events", l.url), login, Some(&body))?;
            v["id"].as_str().ok_or("Microsoft didn't take the new event.")?.to_string()
        }
    };
    let changed: Vec<&Event> = events.iter().filter(|e| e.recurrence_id.is_some()).collect();
    if master.rrule.is_some() && (!master.exdates.is_empty() || !changed.is_empty()) {
        let all_day = master.all_day();
        let instances = instances_of(&id, login)?;
        // Each occurrence by the start it had before any change.
        let at = |i: &Value| match i["type"].as_str() {
            Some("exception") => original_of(i, all_day).map(|w| w.local()),
            _ => time_of(&i["start"]).map(|t| if all_day { t.date().and_time(chrono::NaiveTime::MIN) } else { t }),
        };
        for x in &master.exdates {
            if let Some(i) = instances.iter().find(|i| at(i) == Some(x.local())) {
                call("DELETE", &format!("{GRAPH}/me/events/{}", encode(i["id"].as_str().unwrap_or(""))), login, None)?;
            }
        }
        for ev in changed {
            let rid = ev.recurrence_id.as_ref().map(When::local);
            let Some(i) = instances.iter().find(|i| at(i) == rid) else { continue };
            let same = event_of(i, None).is_some_and(|there| there.summary == ev.summary && there.location == ev.location && there.start.local() == ev.start.local() && there.end.local() == ev.end.local());
            if !same {
                call("PATCH", &format!("{GRAPH}/me/events/{}", encode(i["id"].as_str().unwrap_or(""))), login, Some(&body_of(ev)))?;
            }
        }
    }
    Ok(id)
}

/// The file a Microsoft event is kept in here (its ids are long, and not made for file names).
fn file_for(id: &str) -> String {
    format!("ms-{}.ics", hash(id))
}

/// Sync one calendar folder with its Microsoft calendar.
pub(crate) fn sync_inner(dir: &Path, l: &mut Link, login: &Login) -> Result<Synced, String> {
    let mut done = Synced::default();
    // ---- what Microsoft has: id → version
    let events = all(&format!("{}/events?$select=id,changeKey,type&$top=200", l.url), login)?;
    let mut by_series: HashMap<String, Vec<Value>> = HashMap::new();
    if events.iter().any(|e| e["type"].as_str() == Some("seriesMaster")) {
        for i in all(&format!("{}/calendarView?{}&$select={INSTANCE}&$top=500", l.url, span_query()), login)? {
            if let Some(m) = i["seriesMasterId"].as_str() {
                by_series.entry(m.to_string()).or_default().push(i);
            }
        }
    }
    let remote: Vec<(String, String)> = events
        .iter()
        .filter_map(|e| {
            let id = e["id"].as_str()?.to_string();
            let key = e["changeKey"].as_str().unwrap_or("");
            let v = if e["type"].as_str() == Some("seriesMaster") { version(key, by_series.get(&id).map(Vec::as_slice).unwrap_or(&[])) } else { key.to_string() };
            Some((id, v))
        })
        .collect();
    // ---- what's here: file → its text
    let local: HashMap<String, String> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "ics"))
        .filter_map(|e| Some((e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).ok()?)))
        .collect();
    let by_id: HashMap<String, String> = l.items.iter().map(|(f, (h, _, _))| (h.clone(), f.clone())).collect();
    let mut fetch: Vec<(String, String)> = Vec::new(); // (id, file)
    for (id, ver) in &remote {
        let Some(file) = by_id.get(id).cloned() else {
            fetch.push((id.clone(), file_for(id)));
            done.down += 1;
            continue;
        };
        let (_, old_ver, old_hash) = l.items.get(&file).cloned().unwrap_or_default();
        match local.get(&file) {
            // Changed there: brought down (Microsoft's wins over a change here).
            _ if *ver != old_ver => {
                fetch.push((id.clone(), file));
                done.down += 1;
            }
            Some(text) if hash(text) != old_hash => {
                match push(l, login, text, Some(id)) {
                    Err(e) if e == "gone" => {}
                    r => {
                        r?;
                    }
                }
                fetch.push((id.clone(), file));
                done.up += 1;
            }
            Some(_) => {}
            // Deleted here: deleted there.
            None => {
                call("DELETE", &format!("{GRAPH}/me/events/{}", encode(id)), login, None)?;
                l.items.remove(&file);
                done.removed += 1;
            }
        }
    }
    // Gone from Microsoft: gone here, unless changed here since (then it goes back up).
    let there: HashSet<&String> = remote.iter().map(|(r, _)| r).collect();
    let gone: Vec<String> = l.items.iter().filter(|(_, (h, _, _))| !there.contains(h)).map(|(f, _)| f.clone()).collect();
    for file in gone {
        let (_, _, old_hash) = l.items.remove(&file).unwrap_or_default();
        match local.get(&file) {
            Some(text) if hash(text) != old_hash => {
                let id = push(l, login, text, None)?;
                fetch.push((id, file));
                done.up += 1;
            }
            Some(_) => {
                let _ = std::fs::remove_file(dir.join(&file));
                done.removed += 1;
            }
            None => {}
        }
    }
    // New here: sent up, then brought back down as Microsoft keeps it.
    let fetching: HashSet<String> = fetch.iter().map(|(_, f)| f.clone()).collect();
    let mut new: Vec<(&String, &String)> = local.iter().filter(|(f, _)| !l.items.contains_key(*f) && !fetching.contains(*f)).collect();
    new.sort();
    for (file, text) in new {
        let id = push(l, login, text, None)?;
        fetch.push((id, file.clone()));
        done.up += 1;
    }
    for (id, file) in &fetch {
        download(dir, l, login, id, file)?;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_rules_go_both_ways() {
        let start = When::Floating(NaiveDate::from_ymd_opt(2026, 11, 24).unwrap().and_hms_opt(9, 0, 0).unwrap());
        for rule in ["FREQ=WEEKLY;BYDAY=MO,WE;WKST=MO;INTERVAL=2", "FREQ=MONTHLY;BYDAY=-1FR", "FREQ=YEARLY;BYMONTH=11;BYMONTHDAY=24;COUNT=5", "FREQ=DAILY", "FREQ=MONTHLY;BYMONTHDAY=24"] {
            let r = recurrence_of(rule, &start).unwrap();
            let back = rrule_of(&r, &start).unwrap();
            let again = recurrence_of(&back, &start).unwrap();
            assert_eq!(r["pattern"], again["pattern"], "{rule} came back as {back}");
            assert_eq!(r["range"], again["range"], "{rule} came back as {back}");
        }
        // A weekday rule, as Rivet's editor writes it, is Outlook's weekly on five days.
        let r = recurrence_of("FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR", &start).unwrap();
        assert_eq!(r["pattern"]["type"], "weekly");
        assert_eq!(r["pattern"]["daysOfWeek"].as_array().unwrap().len(), 5);
        assert!(recurrence_of("FREQ=HOURLY", &start).is_err());
        assert!(recurrence_of("FREQ=MONTHLY;BYMONTHDAY=1,15", &start).is_err());
    }

    #[test]
    fn a_series_with_a_changed_and_a_deleted_occurrence_reads_as_ics() {
        // Weekly on Tuesdays at 9 in this computer's zone, around now so it's in the window.
        let (a, _) = window();
        let first = (0..7).map(|k| a + chrono::Duration::days(k + 7)).find(|d| d.weekday() == chrono::Weekday::Tue).unwrap();
        let at = |d: NaiveDate, h: u32| d.and_hms_opt(h, 0, 0).unwrap();
        let tz = zone();
        let t = |x: NaiveDateTime| json!({ "dateTime": x.format("%Y-%m-%dT%H:%M:%S.0000000").to_string(), "timeZone": tz });
        let master = json!({
            "id": "AAA", "changeKey": "k1", "iCalUId": "uid-1", "type": "seriesMaster", "subject": "Stand-up",
            "start": t(at(first, 9)), "end": t(at(first, 10)), "isAllDay": false,
            "location": { "displayName": "Room 2" }, "body": { "content": "Notes" },
            "recurrence": { "pattern": { "type": "weekly", "interval": 1, "daysOfWeek": ["tuesday"], "firstDayOfWeek": "monday" }, "range": { "type": "numbered", "numberOfOccurrences": 4, "startDate": first.to_string() } }
        });
        let week = |n: i64| first + chrono::Duration::weeks(n);
        let utc = |x: NaiveDateTime| Local.from_local_datetime(&x).earliest().unwrap().with_timezone(&Utc).format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let instances = vec![
            json!({ "id": "i0", "changeKey": "c", "type": "occurrence", "start": t(at(week(0), 9)), "end": t(at(week(0), 10)) }),
            // Week 1 moved to 11, week 2 deleted.
            json!({ "id": "i1", "changeKey": "c", "type": "exception", "subject": "Stand-up (late)", "start": t(at(week(1), 11)), "end": t(at(week(1), 12)), "originalStart": utc(at(week(1), 9)) }),
            json!({ "id": "i3", "changeKey": "c", "type": "occurrence", "start": t(at(week(3), 9)), "end": t(at(week(3), 10)) }),
        ];
        let text = to_ics(&master, &instances).unwrap();
        let comps = crate::ics::parse(&text);
        let evs: Vec<Event> = comps[0].subs("VEVENT").filter_map(Event::from_component).collect();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].uid, "uid-1");
        assert_eq!(evs[0].rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=TU;WKST=MO;COUNT=4"));
        assert_eq!(evs[0].exdates.iter().map(When::local).collect::<Vec<_>>(), vec![at(week(2), 9)]);
        assert_eq!(evs[0].location, "Room 2");
        assert_eq!(evs[1].recurrence_id.as_ref().map(When::local), Some(at(week(1), 9)));
        assert_eq!(evs[1].start.local(), at(week(1), 11));
        // Read as a folder would be: three occurrences, the moved one at 11.
        let root = std::env::temp_dir().join(format!("rivet-graph-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut store = crate::Store::open(&root);
        let dir = store.create_calendar("Work", [1, 2, 3]).unwrap();
        std::fs::write(dir.join("ms-x.ics"), &text).unwrap();
        store.reload();
        let found = store.occurrences(first, week(5), &[]);
        assert_eq!(found.iter().map(|o| o.start).collect::<Vec<_>>(), vec![at(week(0), 9), at(week(1), 11), at(week(3), 9)]);
        let _ = std::fs::remove_dir_all(&root);
        assert!(version("k1", &instances).starts_with("k1+"));
        assert_eq!(version("k1", &[]), "k1");
    }
}

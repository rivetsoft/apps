//! Rivet Calendar as a Rivet Shell plugin (`rivet-calendar shell-plugin`, started by the shell's
//! Calendar plugin and kept running): no item on the panel, but the events of the month the
//! shell's calendar shows, for it to mark the days that have some and list a day's. Clicking one
//! there opens Rivet Calendar on it.
//!
//! The shell tells it which month it shows (`month`, with `from` and `to`), and it answers with
//! `events`: one entry a day an event covers, so a three-day event marks all three. The folder
//! is read again when anything in it changes, so an event saved anywhere shows within seconds
//! (one edited in place by another program within a few minutes);
//! subscriptions are read again from their addresses when they're due, and calendars on servers
//! synced every ten minutes.

use std::io::BufRead;
use std::process::ExitCode;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate};
use rivet_cal::{Store, Watch};
use serde_json::{Value, json};

use crate::settings::Settings;

/// How often the folder is looked at for changes (the calendar folders' times only: see `Watch`).
const EVERY: Duration = Duration::from_secs(4);
/// How often subscriptions are looked at to see which are due (each is read every few hours).
const FEEDS: Duration = Duration::from_secs(600);
/// Entries sent at most (a span that full is one too busy to mark day by day anyway).
const MOST: usize = 1500;

/// The month around a day, with the days of the weeks either side that a month grid shows.
fn around(day: NaiveDate) -> (NaiveDate, NaiveDate) {
    let first = day.with_day(1).unwrap_or(day);
    (first - chrono::Duration::days(7), first + chrono::Duration::days(45))
}

/// What the shell is sent for a span: its events, one entry a day each covers.
fn events(store: &Store, hidden: &[String], from: NaiveDate, to: NaiveDate, clock_24: bool) -> Value {
    let mut out = Vec::new();
    for o in store.occurrences(from, to, hidden) {
        let Some(ev) = store.event(&o) else { continue };
        let cal = &store.calendars[o.cal];
        let color = cal.color.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}")).unwrap_or_default();
        for day in o.days().filter(|d| *d >= from && *d < to) {
            // The time on the day it starts; whole days, and the days after the first, have none.
            let time = if o.all_day || day != o.start.date() { String::new() } else { crate::app::clock(o.start.time(), clock_24) };
            out.push(json!({
                "id": format!("{}|{}", day.format("%Y-%m-%d"), ev.uid),
                "day": day.format("%Y-%m-%d").to_string(),
                "title": if ev.summary.trim().is_empty() { "Untitled event".to_string() } else { ev.summary.clone() },
                "time": time,
                "all_day": o.all_day,
                "color": color,
            }));
            if out.len() >= MOST {
                return Value::Array(out);
            }
        }
    }
    Value::Array(out)
}

pub fn run() -> ExitCode {
    let s = Settings::load();
    let root = s.root();
    let mut store = Store::open(&root);
    let mut watch = Watch::new(&root);
    let mut span = around(Local::now().date_naive());
    // Subscriptions are read again here too, on a thread, so they're current with no window open.
    let reading = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut looked = std::time::Instant::now() - FEEDS;
    let say = |store: &Store, span: (NaiveDate, NaiveDate)| {
        let s = Settings::load();
        let msg = json!({ "hidden": true, "events": events(store, &s.hidden, span.0, span.1, s.clock_24) });
        println!("{msg}");
    };
    say(&store, span);
    // What the shell says comes in on a thread, so the folder can be looked at between.
    let (tx, rx) = channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    loop {
        match rx.recv_timeout(EVERY) {
            Ok(line) => {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                let f = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                match f("event").as_str() {
                    "month" => {
                        let day = |k: &str| NaiveDate::parse_from_str(&f(k), "%Y-%m-%d").ok();
                        if let (Some(a), Some(b)) = (day("from"), day("to")) {
                            span = (a, b);
                            say(&store, span);
                        }
                    }
                    // A day's event clicked in the shell: Rivet Calendar on it.
                    "open" => {
                        let id = f("id");
                        let (day, uid) = id.split_once('|').unwrap_or((&id, ""));
                        let exe = std::env::current_exe().unwrap_or_else(|_| "rivet-calendar".into());
                        let mut cmd = std::process::Command::new(exe);
                        cmd.arg("--date").arg(day);
                        if !uid.is_empty() {
                            cmd.arg("--event").arg(uid);
                        }
                        let _ = cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
                    }
                    _ => {}
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if looked.elapsed() >= FEEDS && !reading.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    looked = std::time::Instant::now();
                    let (root, reading) = (root.clone(), reading.clone());
                    std::thread::spawn(move || {
                        rivet_cal::feed::refresh_due(&root);
                        // Calendars on servers too, so a change made on a phone shows here.
                        if !rivet_cal::caldav::accounts().is_empty() {
                            rivet_cal::caldav::sync_all(&root);
                        }
                        reading.store(false, std::sync::atomic::Ordering::SeqCst);
                    });
                }
                if watch.changed() {
                    store.reload();
                    say(&store, span);
                }
            }
            // The shell closed our input: it's done with us.
            Err(RecvTimeoutError::Disconnected) => return ExitCode::SUCCESS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_event_marks_each_of_its_days() {
        let root = std::env::temp_dir().join(format!("rivet-calendar-shell-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut store = Store::open(&root);
        let dir = store.create_calendar("Trips", [10, 20, 30]).unwrap();
        let d = |n: u32| NaiveDate::from_ymd_opt(2026, 11, n).unwrap();
        let ev = rivet_cal::Event::new("Away", rivet_cal::When::Date(d(10)), rivet_cal::When::Date(d(13)));
        store.save(&dir, &ev).unwrap();
        let sent = events(&store, &[], d(1), d(30), true);
        let days: Vec<&str> = sent.as_array().unwrap().iter().map(|e| e["day"].as_str().unwrap()).collect();
        assert_eq!(days, vec!["2026-11-10", "2026-11-11", "2026-11-12"]);
        assert_eq!(sent[0]["color"], "#0a141e");
        let _ = std::fs::remove_dir_all(&root);
    }
}

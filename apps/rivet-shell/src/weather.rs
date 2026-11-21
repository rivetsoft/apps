//! The weather for one place: what it is now, the next hours and the coming days, read from
//! Open-Meteo (no account or key) with curl. Nothing is asked for until a place is chosen and
//! something shows the weather: the panel's Weather item or the desktop widget.
//!
//! Readings are kept in metric; [`Units`] turns them into what's shown, so changing units needs
//! no new reading. Drawing is in `weather_ui`.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Degrees and wind speeds as they're shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    Metric,
    Imperial,
}

impl Units {
    pub fn name(self) -> &'static str {
        match self {
            Units::Metric => "metric",
            Units::Imperial => "imperial",
        }
    }
    pub fn parse(v: &str) -> Option<Units> {
        match v {
            "metric" => Some(Units::Metric),
            "imperial" => Some(Units::Imperial),
            _ => None,
        }
    }
    /// What this computer's language settings measure in: Fahrenheit where it's the custom.
    pub fn local() -> Units {
        let loc = ["LC_ALL", "LC_MEASUREMENT", "LANG"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty())).unwrap_or_default();
        Units::of_locale(&loc)
    }
    fn of_locale(loc: &str) -> Units {
        let region = loc.split(['.', '@']).next().and_then(|l| l.split_once('_')).map(|(_, r)| r).unwrap_or("");
        if matches!(region, "US" | "LR" | "MM") { Units::Imperial } else { Units::Metric }
    }
    /// A temperature read in °C, as "12°".
    pub fn temp(self, c: f32) -> String {
        let t = match self {
            Units::Metric => c,
            Units::Imperial => c * 9.0 / 5.0 + 32.0,
        };
        // Never "-0°".
        format!("{:.0}°", if t.round() == 0.0 { 0.0 } else { t })
    }
    /// A wind speed read in km/h, as "13 km/h".
    pub fn wind(self, kmh: f32) -> String {
        match self {
            Units::Metric => format!("{kmh:.0} km/h"),
            Units::Imperial => format!("{:.0} mph", kmh * 0.621_371),
        }
    }
}

/// What the sky is doing, from the service's weather code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sky {
    Clear,
    PartlyCloudy,
    Cloudy,
    Fog,
    Drizzle,
    Rain,
    Snow,
    Thunder,
}

impl Sky {
    /// From a WMO weather code (the ones Open-Meteo gives).
    pub fn of(code: u16) -> Sky {
        match code {
            0 | 1 => Sky::Clear,
            2 => Sky::PartlyCloudy,
            3 => Sky::Cloudy,
            45 | 48 => Sky::Fog,
            51..=57 => Sky::Drizzle,
            61..=67 | 80..=82 => Sky::Rain,
            71..=77 | 85 | 86 => Sky::Snow,
            95..=99 => Sky::Thunder,
            _ => Sky::Cloudy,
        }
    }
}

/// A weather code in words.
pub fn words(code: u16) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80 | 81 => "Showers",
        82 => "Heavy showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Cloudy",
    }
}

/// Now. Temperatures in °C, wind in km/h.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Now {
    pub temp: f32,
    pub feels: f32,
    pub code: u16,
    pub day: bool,
    pub wind: f32,
    pub humidity: u8,
}

/// One of the coming hours ("14" for two in the afternoon, the place's own time).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hour {
    pub hour: u8,
    pub temp: f32,
    pub code: u16,
    pub day: bool,
}

/// One day, today first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Day {
    /// "2026-11-21".
    pub date: String,
    pub code: u16,
    pub high: f32,
    pub low: f32,
    /// The chance of rain or snow, in percent.
    pub wet: u8,
}

impl Day {
    /// "Today", else the weekday ("Mon").
    pub fn name(&self, first: bool) -> String {
        if first {
            return "Today".into();
        }
        chrono::NaiveDate::parse_from_str(&self.date, "%Y-%m-%d").map(|d| d.format("%a").to_string()).unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum State {
    /// No place chosen yet.
    #[default]
    NoPlace,
    /// A place is chosen and its first reading is on the way.
    Reading,
    Ok,
    /// The last try failed (what to tell the person). An earlier reading, if any, is kept.
    Failed(String),
}

/// What's known about the weather, for drawing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Weather {
    pub state: State,
    pub place: String,
    pub now: Option<Now>,
    pub hours: Vec<Hour>,
    pub days: Vec<Day>,
    /// When it was read (seconds since 1970; 0: never).
    pub at: i64,
}

impl Weather {
    /// What to say in place of the weather when there's none to show.
    pub fn message(&self) -> Option<&str> {
        if self.now.is_some() {
            return None;
        }
        Some(match &self.state {
            State::NoPlace => "Choose a place in Settings.",
            State::Failed(why) => why,
            _ => "Reading the weather…",
        })
    }
}

/// A place the search found.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    /// "Portland, Oregon, United States".
    pub name: String,
    pub lat: f32,
    pub lon: f32,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// GET `url`: its text (None: curl is missing, the network is down, or the service refused).
fn get(url: &str) -> Option<String> {
    let out = Command::new("curl").args(["-sfL", "--max-time", "15", "--max-filesize", "400000", "--", url]).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn has_curl() -> bool {
    Command::new("curl").arg("--version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

fn forecast_url(lat: f32, lon: f32) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.3}&longitude={lon:.3}&current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day&hourly=temperature_2m,weather_code,is_day&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max&timezone=auto&forecast_days=7&forecast_hours=12"
    )
}

/// The service's answer as readings (None: not an answer it gives).
fn parse(text: &str) -> Option<(Now, Vec<Hour>, Vec<Day>)> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let num = |v: &serde_json::Value| v.as_f64().map(|n| n as f32);
    let c = v.get("current")?;
    let now = Now {
        temp: num(c.get("temperature_2m")?)?,
        feels: c.get("apparent_temperature").and_then(num).unwrap_or(f32::NAN),
        code: c.get("weather_code")?.as_u64()? as u16,
        day: c.get("is_day").and_then(|d| d.as_u64()) != Some(0),
        wind: c.get("wind_speed_10m").and_then(num).unwrap_or(0.0),
        humidity: c.get("relative_humidity_2m").and_then(|h| h.as_u64()).unwrap_or(0).min(100) as u8,
    };
    let list = |part: &str, key: &str| v.get(part).and_then(|p| p.get(key)).and_then(|a| a.as_array()).cloned().unwrap_or_default();
    let (times, temps, codes, days_on) = (list("hourly", "time"), list("hourly", "temperature_2m"), list("hourly", "weather_code"), list("hourly", "is_day"));
    let hours = times
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            // "2026-11-21T14:00".
            let hour = t.as_str()?.split_once('T')?.1.get(..2)?.parse::<u8>().ok()?;
            Some(Hour { hour, temp: num(temps.get(i)?)?, code: codes.get(i)?.as_u64()? as u16, day: days_on.get(i).and_then(|d| d.as_u64()) != Some(0) })
        })
        .collect();
    let (dates, codes, highs, lows, wets) = (list("daily", "time"), list("daily", "weather_code"), list("daily", "temperature_2m_max"), list("daily", "temperature_2m_min"), list("daily", "precipitation_probability_max"));
    let days = dates
        .iter()
        .enumerate()
        .filter_map(|(i, d)| Some(Day { date: d.as_str()?.to_string(), code: codes.get(i)?.as_u64()? as u16, high: num(highs.get(i)?)?, low: num(lows.get(i)?)?, wet: wets.get(i).and_then(|w| w.as_u64()).unwrap_or(0).min(100) as u8 }))
        .collect();
    Some((now, hours, days))
}

/// Places called `name`, the likeliest first (for Settings; waits on the network, so call it
/// from a thread). Err: what to tell the person.
pub fn find(name: &str) -> Result<Vec<Place>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(Vec::new());
    }
    if !has_curl() {
        return Err("Install curl to look up places.".into());
    }
    let text = get(&format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=6&language=en&format=json", encode(name))).ok_or("Couldn't reach the weather service. Check the connection and try again.")?;
    Ok(parse_places(&text))
}

fn parse_places(text: &str) -> Vec<Place> {
    let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let Some(list) = v.get("results").and_then(|r| r.as_array()) else { return Vec::new() };
    list.iter()
        .filter_map(|r| {
            let text = |k: &str| r.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let town = text("name");
            // "Berlin, State of Berlin, Germany", without saying a name twice.
            let mut parts = vec![town.clone()];
            for more in [text("admin1"), text("country")] {
                if !more.is_empty() && !parts.contains(&more) {
                    parts.push(more);
                }
            }
            let (lat, lon) = (r.get("latitude")?.as_f64()? as f32, r.get("longitude")?.as_f64()? as f32);
            (!town.is_empty() && lat.abs() <= 90.0 && lon.abs() <= 180.0).then(|| Place { name: parts.join(", "), lat, lon })
        })
        .collect()
}

/// `text` for a web address's query.
fn encode(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

// ------------------------------------------------------------------ the thread

/// The place chosen: its name, and its latitude and longitude (None: no place yet).
type Chosen = (String, Option<(f32, f32)>);

#[derive(Clone)]
pub struct WeatherLive {
    state: Arc<Mutex<Weather>>,
    generation: Arc<AtomicU64>,
    /// Something shows the weather (else nothing is read).
    on: Arc<AtomicBool>,
    place: Arc<Mutex<Chosen>>,
    poke: Arc<Mutex<Sender<()>>>,
}

impl WeatherLive {
    pub fn get(&self) -> Weather {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
    /// Whether anything shows the weather, and the place chosen.
    pub fn set(&self, on: bool, name: &str, at: Option<(f32, f32)>) {
        let was = self.on.swap(on, Ordering::Relaxed);
        let new = (name.to_string(), at);
        let changed = self.place.lock().map(|mut p| std::mem::replace(&mut *p, new.clone()) != new).unwrap_or(false);
        if changed {
            // Another place's weather isn't this one's.
            self.edit(|w| *w = Weather { place: new.0.clone(), state: if at.is_some() { State::Reading } else { State::NoPlace }, ..Default::default() });
        }
        if (on && !was) || changed {
            self.refresh();
        }
    }
    /// Read again now.
    pub fn refresh(&self) {
        if let Ok(p) = self.poke.lock() {
            let _ = p.send(());
        }
    }
    fn edit(&self, f: impl FnOnce(&mut Weather)) {
        let changed = match self.state.lock() {
            Ok(mut s) => {
                let before = s.clone();
                f(&mut s);
                *s != before
            }
            Err(_) => false,
        };
        if changed {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// A reading every 15 minutes; a failed one is tried again after a minute.
const EVERY: u64 = 15 * 60;
const RETRY: u64 = 60;

pub fn start(wake: impl Fn() + Send + 'static) -> WeatherLive {
    let (poke, poked) = channel::<()>();
    let live = WeatherLive { state: Arc::default(), generation: Arc::default(), on: Arc::default(), place: Arc::default(), poke: Arc::new(Mutex::new(poke)) };
    let l = live.clone();
    let _ = std::thread::Builder::new().name("rivet-shell weather".into()).spawn(move || {
        let mut wait = EVERY;
        loop {
            let (name, at) = l.place.lock().map(|p| p.clone()).unwrap_or_default();
            if l.on.load(Ordering::Relaxed)
                && let Some((lat, lon)) = at
            {
                let before = l.generation();
                let read = get(&forecast_url(lat, lon)).and_then(|t| parse(&t));
                // The place changed while this was read: it's the old place's weather.
                let same = l.place.lock().map(|p| p.1 == at).unwrap_or(false);
                if same {
                    wait = if read.is_some() { EVERY } else { RETRY };
                    l.edit(|w| match read {
                        Some((now, hours, days)) => *w = Weather { state: State::Ok, place: name, now: Some(now), hours, days, at: self::now() },
                        None => w.state = State::Failed(if has_curl() { "Couldn't reach the weather service." } else { "Install curl to read the weather." }.into()),
                    });
                }
                if l.generation() != before {
                    wake();
                }
            }
            match poked.recv_timeout(Duration::from_secs(wait)) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => while poked.try_recv().is_ok() {},
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    live
}

/// A reading for checks and Settings' previews.
pub fn sample() -> Weather {
    let hour = |hour, temp, code, day| Hour { hour, temp, code, day };
    let day = |date: &str, code, high, low, wet| Day { date: date.into(), code, high, low, wet };
    Weather {
        state: State::Ok,
        place: "Portland, Oregon, United States".into(),
        now: Some(Now { temp: 11.4, feels: 9.2, code: 2, day: true, wind: 14.0, humidity: 71 }),
        hours: vec![hour(14, 11.4, 2, true), hour(15, 11.9, 2, true), hour(16, 11.2, 3, true), hour(17, 10.1, 61, false), hour(18, 9.4, 63, false), hour(19, 8.8, 61, false), hour(20, 8.1, 3, false), hour(21, 7.6, 0, false)],
        days: vec![day("2026-11-21", 2, 12.4, 6.1, 20), day("2026-11-22", 63, 10.2, 5.4, 85), day("2026-11-23", 3, 9.8, 4.0, 30), day("2026-11-24", 0, 11.1, 2.2, 0), day("2026-11-25", 71, 3.5, -1.2, 60), day("2026-11-26", 95, 8.0, 3.1, 70), day("2026-11-27", 45, 7.4, 2.9, 10)],
        at: now() - 240,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_services_answer() {
        let text = r#"{"current":{"time":"2026-11-21T08:30","temperature_2m":1.3,"apparent_temperature":-2.6,"relative_humidity_2m":93,"weather_code":3,"wind_speed_10m":12.6,"is_day":1},
            "hourly":{"time":["2026-11-21T08:00","2026-11-21T09:00"],"temperature_2m":[1.3,1.4],"weather_code":[3,61],"is_day":[1,0]},
            "daily":{"time":["2026-11-21","2026-11-22"],"weather_code":[80,71],"temperature_2m_max":[6.4,4.7],"temperature_2m_min":[0.6,-1.9],"precipitation_probability_max":[53,null]}}"#;
        let (now, hours, days) = parse(text).unwrap();
        assert_eq!((now.temp, now.code, now.day, now.humidity), (1.3, 3, true, 93));
        assert_eq!(hours, [Hour { hour: 8, temp: 1.3, code: 3, day: true }, Hour { hour: 9, temp: 1.4, code: 61, day: false }]);
        assert_eq!(days[1], Day { date: "2026-11-22".into(), code: 71, high: 4.7, low: -1.9, wet: 0 });
        assert_eq!((days[0].name(true), days[1].name(false)), ("Today".to_string(), "Sun".to_string()));
        assert!(parse("{}").is_none() && parse("not json").is_none() && parse(r#"{"error":true,"reason":"x"}"#).is_none());
    }

    /// Asks the real service (needs the network): `cargo test -p rivet-shell weather -- --ignored`.
    #[test]
    #[ignore]
    fn live_reading() {
        let places = find("Berlin").unwrap();
        assert!(places[0].name.starts_with("Berlin"), "{places:?}");
        let (now, hours, days) = parse(&get(&forecast_url(places[0].lat, places[0].lon)).unwrap()).unwrap();
        assert!((-60.0..60.0).contains(&now.temp) && hours.len() == 12 && days.len() == 7, "{now:?} {} {}", hours.len(), days.len());
    }

    #[test]
    fn places_found() {
        let text = r#"{"results":[{"name":"Berlin","latitude":52.52437,"longitude":13.41053,"country":"Germany","admin1":"State of Berlin"},
            {"name":"Luxembourg","latitude":49.61,"longitude":6.13,"country":"Luxembourg","admin1":"Luxembourg"},{"name":"","latitude":1,"longitude":1},{"name":"Nowhere"}]}"#;
        let found = parse_places(text);
        assert_eq!(found.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Berlin, State of Berlin, Germany", "Luxembourg"]);
        assert!(parse_places("{}").is_empty() && parse_places("").is_empty());
        assert_eq!(encode("São Paulo & co"), "S%C3%A3o%20Paulo%20%26%20co");
    }

    #[test]
    fn units_and_skies() {
        assert_eq!((Units::Metric.temp(11.4), Units::Imperial.temp(0.0), Units::Metric.temp(-0.3)), ("11°".to_string(), "32°".to_string(), "0°".to_string()));
        assert_eq!((Units::Metric.wind(14.2), Units::Imperial.wind(16.1)), ("14 km/h".to_string(), "10 mph".to_string()));
        assert_eq!((Units::of_locale("en_US.UTF-8"), Units::of_locale("de_DE.UTF-8"), Units::of_locale("C")), (Units::Imperial, Units::Metric, Units::Metric));
        assert_eq!((Sky::of(0), Sky::of(2), Sky::of(63), Sky::of(81), Sky::of(75), Sky::of(96), Sky::of(1234)), (Sky::Clear, Sky::PartlyCloudy, Sky::Rain, Sky::Rain, Sky::Snow, Sky::Thunder, Sky::Cloudy));
        assert_eq!(Weather::default().message(), Some("Choose a place in Settings."));
        assert_eq!(sample().message(), None);
    }
}

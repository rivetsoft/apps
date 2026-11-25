//! AI plan usage for the panel's AI item: how much of each limit is used and when it resets,
//! for the AI tools signed in on this computer, and whether Claude Code is running.
//!
//! - Claude: the usage Claude Code keeps in `~/.claude.json` (every Claude Code session writes
//!   it), so reading it costs nothing. Only when that's over 15 minutes old (use from another
//!   computer or claude.ai) does the thread ask Anthropic's usage service with Claude Code's
//!   login: at most once a minute, backing off after a refusal.
//! - Codex and Grok: their own logins (`~/.codex/auth.json`, `~/.grok/auth.json`).
//! - OpenRouter and DeepSeek: API keys from the environment, or from
//!   `~/.config/rivet-shell/ai.env` (a desktop session doesn't see a terminal's exports).
//! - Hermes Agent: not a plan with limits, but what it did over the last week, from
//!   `hermes insights --source acp --days 7`. Only when the `hermes` program is installed, and
//!   at most every 10 minutes (it takes a couple of seconds).
//!
//! Logins are only read, never refreshed or written. Keys go to curl on its standard input,
//! never as arguments other programs could see.
//!
//! Env: CLAUDE_CONFIG_DIR, CODEX_HOME, GROK_HOME, OPENROUTER_API_KEY, DEEPSEEK_API_KEY.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

/// The providers: id, name.
pub const PROVIDERS: &[(&str, &str)] = &[("claude", "Claude"), ("codex", "Codex"), ("grok", "Grok"), ("openrouter", "OpenRouter"), ("deepseek", "DeepSeek")];

/// Hermes Agent: shown when it's installed, with its week's activity rather than limits.
pub const HERMES: (&str, &str) = ("hermes", "Hermes");

pub fn name(id: &str) -> &'static str {
    PROVIDERS.iter().chain([&HERMES]).find(|p| p.0 == id).map(|p| p.1).unwrap_or("AI")
}

/// One limit: "5-hour", "Weekly"…
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Window {
    pub name: String,
    /// Percent used, 0–100.
    pub used: f32,
    /// When it resets (unix seconds; 0: not known).
    pub reset: i64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum State {
    /// Not looked at yet.
    #[default]
    Unknown,
    Ok,
    /// Not signed in or no key: how to set it up.
    Missing(String),
    /// Asked and it failed: why.
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Provider {
    pub id: &'static str,
    pub state: State,
    /// The plan ("Max 5x", "Plus"…).
    pub plan: String,
    pub windows: Vec<Window>,
    /// Money left, for pay-as-you-go accounts ("$4.20 left of $10.00").
    pub balance: Option<String>,
    /// Figures that aren't limits, as label and value ("Sessions", "83"): Hermes' week.
    pub facts: Vec<(String, String)>,
    /// When the numbers were read (unix seconds).
    pub at: i64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Quotas {
    /// The enabled providers, in PROVIDERS' order.
    pub providers: Vec<Provider>,
    /// Claude Code: installed (a ~/.claude folder or the program), sessions running, and
    /// whether one wrote to its transcript in the last two minutes.
    pub claude_code: bool,
    pub claude_sessions: u32,
    pub claude_busy: bool,
}

impl Quotas {
    pub fn get(&self, id: &str) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == id)
    }
    /// What the panel shows: the most used limit of `which` ("auto": of any provider), and
    /// its provider.
    pub fn headline(&self, which: &str) -> Option<(&Provider, &Window)> {
        self.providers
            .iter()
            .filter(|p| p.state == State::Ok && (which == "auto" || p.id == which))
            .flat_map(|p| p.windows.iter().map(move |w| (p, w)))
            .max_by(|a, b| a.1.used.total_cmp(&b.1.used))
    }
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

// ------------------------------------------------------------------ the thread

#[derive(Clone)]
pub struct AiLive {
    state: Arc<Mutex<Quotas>>,
    generation: Arc<AtomicU64>,
    /// A panel shows the AI item (else nothing is read).
    on: Arc<AtomicBool>,
    enabled: Arc<Mutex<Vec<String>>>,
    poke: Arc<Mutex<Sender<bool>>>,
}

impl AiLive {
    pub fn get(&self) -> Quotas {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
    /// Whether a panel shows the item, and which providers are turned on.
    pub fn set(&self, on: bool, enabled: &[String]) {
        let was = self.on.swap(on, Ordering::Relaxed);
        let changed = self.enabled.lock().map(|mut e| std::mem::replace(&mut *e, enabled.to_vec()) != enabled).unwrap_or(false);
        if (on && !was) || changed {
            self.refresh(false);
        }
    }
    /// Read again now; `all`: ask the services too, not only the local files.
    pub fn refresh(&self, all: bool) {
        if let Ok(p) = self.poke.lock() {
            let _ = p.send(all);
        }
    }
    fn edit(&self, f: impl FnOnce(&mut Quotas)) -> bool {
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
        changed
    }
}

/// Local files every 15 s; the services every 2 minutes; Hermes every 10.
const LOOK: u64 = 15;
const ASK: i64 = 120;
const ASK_HERMES: i64 = 600;

pub fn start(wake: impl Fn() + Send + 'static) -> AiLive {
    let (poke, poked) = channel::<bool>();
    let live = AiLive { state: Arc::default(), generation: Arc::default(), on: Arc::default(), enabled: Arc::default(), poke: Arc::new(Mutex::new(poke)) };
    let l = live.clone();
    std::thread::Builder::new()
        .name("rivet-shell ai".into())
        .spawn(move || {
            let mut claude = ClaudePoll::default();
            let mut asked: i64 = 0;
            let mut asked_hermes: i64 = 0;
            let mut force = false;
            loop {
                if l.on.load(Ordering::Relaxed) {
                    let enabled = l.enabled.lock().map(|e| e.clone()).unwrap_or_default();
                    let t = now();
                    let ask = force || t - asked >= ASK;
                    if ask {
                        asked = t;
                    }
                    let ask_hermes = force || t - asked_hermes >= ASK_HERMES;
                    if ask_hermes && on_hermes(&enabled) {
                        asked_hermes = t;
                    }
                    let old = l.get();
                    let keep = |id: &str| old.get(id).cloned();
                    let on = |id: &str| enabled.iter().any(|e| e == id);
                    // The services one each on their own thread, so a slow one doesn't hold up the rest.
                    let mut results: Vec<Provider> = std::thread::scope(|sc| {
                        let jobs: Vec<_> = PROVIDERS
                            .iter()
                            .chain([&HERMES])
                            .filter(|(id, _)| *id != "claude" && on(id))
                            .map(|&(id, _)| {
                                let prev = keep(id);
                                let now = if id == HERMES.0 { ask_hermes } else { ask };
                                sc.spawn(move || if now || prev.is_none() { fetch(id) } else { prev.unwrap_or_default() })
                            })
                            .collect();
                        jobs.into_iter().map(|j| j.join().unwrap_or_default()).collect()
                    });
                    if on("claude") {
                        results.insert(0, claude.read(force));
                    }
                    let (code, sessions, busy) = claude_code();
                    if l.edit(|q| {
                        q.providers = results;
                        q.claude_code = code;
                        q.claude_sessions = sessions;
                        q.claude_busy = busy;
                    }) {
                        wake();
                    }
                }
                force = false;
                let wait = if l.on.load(Ordering::Relaxed) { Duration::from_secs(LOOK) } else { Duration::from_secs(3600) };
                match poked.recv_timeout(wait) {
                    Ok(all) => force = all,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .ok();
    live
}

fn on_hermes(enabled: &[String]) -> bool {
    enabled.iter().any(|e| e == HERMES.0)
}

// ------------------------------------------------------------------ keys and requests

/// An environment variable, else the same name in ~/.config/rivet-shell/ai.env
/// (`NAME=value` or `export NAME=value` lines).
pub fn key(name: &str) -> Option<String> {
    if let Ok(v) = std::env::var(name)
        && !v.trim().is_empty()
    {
        return Some(v.trim().to_string());
    }
    let text = std::fs::read_to_string(env_file()).ok()?;
    text.lines().find_map(|l| {
        let l = l.trim();
        let l = l.strip_prefix("export ").unwrap_or(l);
        let (k, v) = l.split_once('=')?;
        (k.trim() == name).then(|| v.trim().trim_matches(['"', '\'']).to_string()).filter(|v| !v.is_empty())
    })
}

pub fn env_file() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-shell/ai.env")
}

/// GET `url` with `headers` (given to curl on its standard input): the HTTP status (0: no
/// answer) and the body.
fn get(url: &str, headers: &[String]) -> (u16, String) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let child = Command::new("curl")
        .args(["-s", "-m", "15", "-w", "\n%{http_code}", "-H", "@-", "-H", "Accept: application/json", url])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return (0, String::new()) };
    if let Some(mut i) = child.stdin.take() {
        let _ = i.write_all(headers.join("\n").as_bytes());
    }
    let Ok(out) = child.wait_with_output() else { return (0, String::new()) };
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    match text.rsplit_once('\n') {
        Some((body, code)) => (code.trim().parse().unwrap_or(0), body.to_string()),
        None => (text.trim().parse().unwrap_or(0), String::new()),
    }
}

fn read_json(p: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        Value::Object(o) => num(o.get("val")),
        _ => None,
    }
}

/// A time as unix seconds: seconds, milliseconds or an RFC 3339 string.
fn time(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::String(s)) => chrono::DateTime::parse_from_rfc3339(s).map(|d| d.timestamp()).unwrap_or_else(|_| s.parse::<f64>().map(secs).unwrap_or(0)),
        Some(v) => num(Some(v)).map(secs).unwrap_or(0),
        None => 0,
    }
}

fn secs(n: f64) -> i64 {
    if n >= 1e11 { (n / 1000.0) as i64 } else { n as i64 }
}

fn http_error(code: u16, service: &str) -> State {
    State::Error(match code {
        0 => format!("Couldn't reach {service}."),
        401 | 403 => format!("{service} turned the login down."),
        429 => format!("{service} asks to wait; trying again soon."),
        c => format!("{service} answered HTTP {c}."),
    })
}

/// A window that has reset since it was read shows as unused.
fn settle(mut w: Vec<Window>) -> Vec<Window> {
    let t = now();
    for x in &mut w {
        if x.reset > 0 && x.reset <= t {
            x.used = 0.0;
            x.reset = 0;
        }
    }
    w
}

// ------------------------------------------------------------------ Claude

fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".claude"))
}

fn claude_global() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(d) => PathBuf::from(d).join(".claude.json"),
        None => home().join(".claude.json"),
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// The usage service's answer (also what Claude Code caches) as windows. Prefers the
/// "limits" list, which has the model-scoped weekly limits too.
fn claude_windows(u: &Value) -> Vec<Window> {
    if let Some(list) = u.get("limits").and_then(Value::as_array).filter(|l| !l.is_empty()) {
        return list
            .iter()
            .filter_map(|l| {
                let used = num(l.get("percent"))?;
                let kind = l.get("kind").and_then(Value::as_str).unwrap_or("");
                let name = match kind {
                    "session" => "5-hour".to_string(),
                    "weekly_all" => "Weekly".to_string(),
                    "weekly_scoped" => {
                        let s = l.pointer("/scope/model/display_name").or_else(|| l.pointer("/scope/surface/display_name")).and_then(Value::as_str).unwrap_or("scoped");
                        format!("Weekly · {s}")
                    }
                    k => k.replace('_', " "),
                };
                Some(Window { name, used: used.clamp(0.0, 100.0) as f32, reset: time(l.get("resets_at")) })
            })
            .collect();
    }
    [("5-hour", "five_hour"), ("Weekly", "seven_day")]
        .iter()
        .filter_map(|(name, k)| {
            let w = u.get(*k)?;
            Some(Window { name: name.to_string(), used: num(w.get("utilization"))?.clamp(0.0, 100.0) as f32, reset: time(w.get("resets_at")) })
        })
        .collect()
}

fn claude_plan(g: &Value) -> String {
    let a = g.get("oauthAccount");
    let s = |k: &str| a.and_then(|a| a.get(k)).and_then(Value::as_str).unwrap_or("");
    let tier = format!("{} {}", s("userRateLimitTier"), s("organizationRateLimitTier"));
    if tier.contains("max_20x") {
        return "Max 20x".into();
    }
    if tier.contains("max_5x") {
        return "Max 5x".into();
    }
    match s("organizationType").trim_start_matches("claude_") {
        "pro" => "Pro".into(),
        "max" => "Max".into(),
        "team" => "Team".into(),
        "enterprise" => "Enterprise".into(),
        "free" => "Free".into(),
        _ => String::new(),
    }
}

/// Claude's numbers, and when the usage service was last asked (so it's asked rarely).
#[derive(Default)]
struct ClaudePoll {
    checked: i64,
    retry: i64,
    /// The service's last answer: when, and its windows.
    got: Option<(i64, String, Vec<Window>)>,
}

impl ClaudePoll {
    fn read(&mut self, force: bool) -> Provider {
        let mut p = Provider { id: "claude", ..Default::default() };
        let g = read_json(&claude_global());
        let account = g.as_ref().and_then(|g| g.pointer("/oauthAccount/accountUuid")).and_then(Value::as_str).unwrap_or("").to_string();
        p.plan = g.as_ref().map(claude_plan).unwrap_or_default();
        // Claude Code's own copy, when it's this account's.
        let cached = g.as_ref().and_then(|g| g.get("cachedUsageUtilization")).filter(|c| c.get("accountUuid").and_then(Value::as_str) == Some(account.as_str())).and_then(|c| {
            let at = time(c.get("fetchedAtMs"));
            let w = claude_windows(c.get("utilization")?);
            (!w.is_empty()).then_some((at, w))
        });
        let mine = self.got.as_ref().filter(|g| g.1 == account).map(|g| (g.0, g.2.clone()));
        let newest = match (cached, mine) {
            (Some(a), Some(b)) => Some(if b.0 > a.0 { b } else { a }),
            (a, b) => a.or(b),
        };
        let t = now();
        let age = newest.as_ref().map(|n| t - n.0).unwrap_or(i64::MAX);
        let token = read_json(&claude_dir().join(".credentials.json")).and_then(|c| c.pointer("/claudeAiOauth/accessToken").and_then(Value::as_str).map(String::from));
        let mut err = None;
        if let Some(token) = &token
            && !account.is_empty()
            && t >= self.retry
            && t - self.checked >= 60
            && (age >= 900 || force && age >= 60)
        {
            self.checked = t;
            let (code, body) = get("https://api.anthropic.com/api/oauth/usage", &[format!("Authorization: Bearer {token}"), "anthropic-beta: oauth-2025-04-20".into(), "User-Agent: rivet-shell".into()]);
            match code {
                200..=299 => match serde_json::from_str::<Value>(&body).map(|v| claude_windows(&v)) {
                    Ok(w) if !w.is_empty() => self.got = Some((t, account.clone(), w)),
                    _ => err = Some(State::Error("Couldn't read Claude's usage.".into())),
                },
                401 => {
                    self.retry = t + 300;
                    err = Some(State::Error("Claude's login has expired. Starting Claude Code renews it.".into()));
                }
                403 => {
                    self.retry = t + 3600;
                    err = Some(http_error(code, "Claude"));
                }
                429 => {
                    self.retry = t + 300;
                    err = Some(http_error(code, "Claude"));
                }
                c => err = Some(http_error(c, "Claude")),
            }
        }
        let newest = match (newest, self.got.as_ref().filter(|g| g.1 == account)) {
            (Some(a), Some(b)) if b.0 > a.0 => Some((b.0, b.2.clone())),
            (None, Some(b)) => Some((b.0, b.2.clone())),
            (a, _) => a,
        };
        match newest {
            Some((at, w)) => {
                p.state = State::Ok;
                p.at = at;
                p.windows = settle(w);
            }
            None => {
                p.state = match (err, g.is_some() && !account.is_empty()) {
                    (Some(e), _) => e,
                    (None, true) => State::Missing("No usage yet: send Claude Code a message.".into()),
                    (None, false) => State::Missing("Sign in by running claude in a terminal.".into()),
                }
            }
        }
        p
    }
}

/// Claude Code: installed, how many sessions are running, and whether one is working now (a
/// transcript written in the last two minutes).
fn claude_code() -> (bool, u32, bool) {
    let dir = claude_dir();
    let installed = dir.is_dir() || claude_global().is_file();
    let sessions = std::fs::read_dir("/proc")
        .map(|d| d.flatten().filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit())).filter(|e| std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "claude")).count() as u32)
        .unwrap_or(0);
    let busy = sessions > 0 && {
        let since = std::time::SystemTime::now() - Duration::from_secs(120);
        let fresh = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).is_ok_and(|t| t >= since);
        std::fs::read_dir(dir.join("projects")).into_iter().flatten().flatten().any(|proj| std::fs::read_dir(proj.path()).into_iter().flatten().flatten().any(|f| f.file_name().to_string_lossy().ends_with(".jsonl") && fresh(&f.path())))
    };
    (installed || sessions > 0, sessions, busy)
}

// ------------------------------------------------------------------ the others

fn fetch(id: &'static str) -> Provider {
    let mut p = Provider { id, at: now(), ..Default::default() };
    p.state = match id {
        "codex" => codex(&mut p),
        "grok" => grok(&mut p),
        "openrouter" => openrouter(&mut p),
        "deepseek" => deepseek(&mut p),
        "hermes" => hermes(&mut p),
        _ => State::Missing(String::new()),
    };
    p
}

fn codex(p: &mut Provider) -> State {
    let home = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".codex"));
    let auth = read_json(&home.join("auth.json"));
    let tok = |k: &str| auth.as_ref().and_then(|a| a.pointer(&format!("/tokens/{k}"))).and_then(Value::as_str).map(String::from);
    let Some(token) = tok("access_token") else { return State::Missing("Sign in with codex login.".into()) };
    let mut h = vec![format!("Authorization: Bearer {token}"), "User-Agent: codex-cli".into()];
    if let Some(a) = tok("account_id") {
        h.push(format!("ChatGPT-Account-Id: {a}"));
    }
    let (code, body) = get("https://chatgpt.com/backend-api/wham/usage", &h);
    if !(200..300).contains(&code) {
        return http_error(code, "Codex");
    }
    let Ok(v) = serde_json::from_str::<Value>(&body) else { return State::Error("Couldn't read Codex's usage.".into()) };
    let t = now();
    let window = |fallback: &str, w: Option<&Value>| -> Option<Window> {
        let w = w.filter(|w| w.is_object())?;
        let span = num(w.get("limit_window_seconds")).unwrap_or(0.0);
        let name = if fallback == "Code review" {
            fallback.to_string()
        } else if span >= 604800.0 {
            "Weekly".into()
        } else if span >= 14400.0 {
            "5-hour".into()
        } else {
            fallback.to_string()
        };
        let reset = match w.get("reset_at") {
            Some(r) if !r.is_null() => time(Some(r)),
            _ => num(w.get("reset_after_seconds")).map(|s| t + s as i64).unwrap_or(0),
        };
        Some(Window { name, used: num(w.get("used_percent"))?.clamp(0.0, 100.0) as f32, reset })
    };
    p.windows = settle([window("5-hour", v.pointer("/rate_limit/primary_window")), window("Weekly", v.pointer("/rate_limit/secondary_window")), window("Code review", v.pointer("/code_review_rate_limit/primary_window"))].into_iter().flatten().collect());
    p.plan = v.get("plan_type").and_then(Value::as_str).map(title).unwrap_or_default();
    if p.windows.is_empty() { State::Error("Codex showed no limits.".into()) } else { State::Ok }
}

fn grok(p: &mut Provider) -> State {
    let home = std::env::var_os("GROK_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".grok"));
    let token = read_json(&home.join("auth.json")).and_then(|a| a.as_object().and_then(|o| o.values().next()).and_then(|v| v.get("key")).and_then(Value::as_str).map(String::from));
    let Some(token) = token else { return State::Missing("Sign in with grok login.".into()) };
    let (code, body) = get("https://cli-chat-proxy.grok.com/v1/billing?format=credits", &[format!("Authorization: Bearer {token}"), "User-Agent: rivet-shell".into(), "x-grok-client-mode: cli".into()]);
    if !(200..300).contains(&code) {
        return http_error(code, "Grok");
    }
    let Some(c) = serde_json::from_str::<Value>(&body).ok().and_then(|v| v.get("config").cloned()) else { return State::Error("Couldn't read Grok's usage.".into()) };
    let reset = time(c.pointer("/currentPeriod/end").or_else(|| c.get("billingPeriodEnd")));
    let used = num(c.get("creditUsagePercent")).or_else(|| {
        let cap = num(c.get("onDemandCap")).unwrap_or(0.0);
        (cap > 0.0).then(|| 100.0 * num(c.get("onDemandUsed")).unwrap_or(0.0) / cap)
    });
    p.plan = if c.get("isUnifiedBillingUser").and_then(Value::as_bool) == Some(true) { "SuperGrok".into() } else { String::new() };
    match used {
        Some(u) => {
            p.windows = settle(vec![Window { name: "Billing period".into(), used: u.clamp(0.0, 100.0) as f32, reset }]);
            State::Ok
        }
        None => State::Error("Grok showed no limits.".into()),
    }
}

fn openrouter(p: &mut Provider) -> State {
    let Some(k) = key("OPENROUTER_API_KEY") else { return State::Missing("Set OPENROUTER_API_KEY.".into()) };
    let (code, body) = get("https://openrouter.ai/api/v1/credits", &[format!("Authorization: Bearer {k}")]);
    if !(200..300).contains(&code) {
        return http_error(code, "OpenRouter");
    }
    let v: Value = serde_json::from_str(&body).unwrap_or_default();
    let (Some(total), Some(used)) = (num(v.pointer("/data/total_credits")), num(v.pointer("/data/total_usage"))) else { return State::Error("Couldn't read OpenRouter's credits.".into()) };
    p.balance = Some(format!("${:.2} left of ${:.2}", total - used, total));
    if total > 0.0 {
        p.windows = vec![Window { name: "Credits".into(), used: (100.0 * used / total).clamp(0.0, 100.0) as f32, reset: 0 }];
    }
    State::Ok
}

fn deepseek(p: &mut Provider) -> State {
    let Some(k) = key("DEEPSEEK_API_KEY") else { return State::Missing("Set DEEPSEEK_API_KEY.".into()) };
    let (code, body) = get("https://api.deepseek.com/user/balance", &[format!("Authorization: Bearer {k}")]);
    if !(200..300).contains(&code) {
        return http_error(code, "DeepSeek");
    }
    let v: Value = serde_json::from_str(&body).unwrap_or_default();
    let parts: Vec<String> = v
        .get("balance_infos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|b| {
            let cur = b.get("currency").and_then(Value::as_str).unwrap_or("");
            let sym = match cur {
                "USD" => "$",
                "CNY" => "¥",
                _ => "",
            };
            format!("{sym}{:.2}{}", num(b.get("total_balance")).unwrap_or(0.0), if sym.is_empty() { format!(" {cur}") } else { String::new() })
        })
        .collect();
    if parts.is_empty() {
        return State::Error("Couldn't read DeepSeek's balance.".into());
    }
    p.balance = Some(format!("{} left", parts.join(" · ")));
    if v.get("is_available").and_then(Value::as_bool) == Some(false) {
        p.plan = "Not enough balance".into();
    }
    State::Ok
}

/// The `hermes` program: on the PATH, or where its installer puts it (a desktop session's PATH
/// often leaves out ~/.local/bin).
fn hermes_program() -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH").into_iter().flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).map(|d| d.join("hermes"));
    on_path.chain([home().join(".local/bin/hermes"), home().join(".hermes/bin/hermes")]).find(|p| p.is_file())
}

/// The week Hermes Agent had over ACP, from `hermes insights`, which only prints a report: its
/// Overview and Cost lines are read as "Label:  value" pairs.
fn hermes(p: &mut Provider) -> State {
    use std::process::{Command, Stdio};
    let Some(prog) = hermes_program() else { return State::Missing("Install Hermes Agent.".into()) };
    let child = Command::new(prog).args(["insights", "--source", "acp", "--days", "7"]).env("NO_COLOR", "1").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let Ok(mut child) = child else { return State::Error("Couldn't run Hermes Agent.".into()) };
    // Never wait on it for long: the other providers are read on the same pass.
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(30) => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return State::Error("Hermes Agent took too long to answer. Trying again in 10 minutes.".into());
            }
        }
    }
    let Ok(out) = child.wait_with_output() else { return State::Error("Couldn't read what Hermes Agent answered.".into()) };
    p.plan = "Last 7 days".into();
    p.facts = hermes_facts(&String::from_utf8_lossy(&out.stdout));
    State::Ok
}

/// The figures worth showing, in this order, from the report's text.
fn hermes_facts(text: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        // "Sessions:          83            Messages:        447": pairs split at the colons.
        let mut rest = line.trim();
        while let Some((label, after)) = rest.split_once(':') {
            let after = after.trim_start();
            let end = after.find("  ").unwrap_or(after.len());
            let (value, more) = after.split_at(end);
            found.push((label.trim().to_string(), value.trim().to_string()));
            rest = more.trim_start();
        }
    }
    let get = |k: &str| found.iter().find(|f| f.0 == k).map(|f| f.1.clone()).filter(|v| !v.is_empty());
    [("Sessions", "Sessions"), ("Messages", "Messages"), ("Tool calls", "Tool calls"), ("Total tokens", "Tokens"), ("Active time", "Active"), ("Estimated", "Cost")]
        .iter()
        .filter_map(|(k, label)| Some((label.to_string(), get(k)?)))
        .collect()
}

fn title(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// A pinned limit's key: "provider:limit name".
pub fn pin_key(provider: &str, window: &str) -> String {
    format!("{provider}:{}", window.replace(',', " "))
}

/// A limit's short name for the panel: "5h", "wk", "Fable"…
pub fn short(window: &str) -> String {
    match window {
        "5-hour" => "5h".into(),
        "Weekly" => "wk".into(),
        "Code review" => "rev".into(),
        "Billing period" => "bill".into(),
        "Credits" => "cr".into(),
        w => w.strip_prefix("Weekly · ").unwrap_or(w).to_string(),
    }
}

impl Quotas {
    /// The pinned limits found in what was read, in pin order.
    pub fn pinned<'a>(&'a self, pins: &[String]) -> Vec<(&'a Provider, &'a Window)> {
        pins.iter()
            .filter_map(|k| {
                let (pid, wn) = k.split_once(':')?;
                let p = self.providers.iter().find(|p| p.id == pid && p.state == State::Ok)?;
                Some((p, p.windows.iter().find(|w| pin_key(pid, &w.name) == *k || w.name == wn)?))
            })
            .collect()
    }
}

/// Every provider read once, as text (`rivet-shell ai`).
pub fn report() -> String {
    let mut out = String::new();
    let (code, sessions, busy) = claude_code();
    out.push_str(&format!("Claude Code: {}\n", if !code { "not found".to_string() } else { format!("{sessions} session{} running{}", if sessions == 1 { "" } else { "s" }, if busy { ", working now" } else { "" }) }));
    for &(id, _) in PROVIDERS.iter().chain([&HERMES]) {
        let p = if id == "claude" { ClaudePoll::default().read(false) } else { fetch(id) };
        if id == HERMES.0 && matches!(p.state, State::Missing(_)) {
            continue;
        }
        out.push_str(&format!("{}{}: ", name(id), if p.plan.is_empty() { String::new() } else { format!(" ({})", p.plan) }));
        match &p.state {
            State::Ok => {
                out.push_str(&format!("read {}\n", ago(p.at)));
                for w in &p.windows {
                    out.push_str(&format!("  {:<18} {:>5.1}% used{}\n", w.name, w.used, if w.reset > 0 { format!(", resets {}", until(w.reset)) } else { String::new() }));
                }
                if let Some(b) = &p.balance {
                    out.push_str(&format!("  {b}\n"));
                }
                for (k, v) in &p.facts {
                    out.push_str(&format!("  {k:<18} {v}\n"));
                }
            }
            State::Missing(why) | State::Error(why) => out.push_str(&format!("{why}\n")),
            State::Unknown => out.push('\n'),
        }
    }
    out
}

/// "in 2 h 10 min", "in 3 days", "in 40 min".
pub fn until(reset: i64) -> String {
    let d = reset - now();
    if d <= 0 {
        return "now".into();
    }
    let (days, h, m) = (d / 86400, d / 3600 % 24, (d / 60 % 60).max(if d < 60 { 1 } else { 0 }));
    if days >= 2 {
        format!("in {days} days")
    } else if days == 1 {
        format!("in 1 day {h} h")
    } else if h > 0 {
        format!("in {h} h {m} min")
    } else {
        format!("in {m} min")
    }
}

/// "just now", "4 min ago", "2 h ago".
pub fn ago(at: i64) -> String {
    let d = (now() - at).max(0);
    if d < 60 {
        "just now".into()
    } else if d < 3600 {
        format!("{} min ago", d / 60)
    } else {
        format!("{} h ago", d / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_claude_limits() {
        let v: Value = serde_json::from_str(r#"{"limits":[{"kind":"session","percent":2,"resets_at":"2999-11-18T11:09:59.561547+00:00"},{"kind":"weekly_all","percent":45.5,"resets_at":"2999-11-21T03:59:59+00:00"},{"kind":"weekly_scoped","percent":0,"resets_at":"2999-11-21T04:00:00+00:00","scope":{"model":{"display_name":"Fable"}}}]}"#).unwrap();
        let w = claude_windows(&v);
        assert_eq!(w.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(), ["5-hour", "Weekly", "Weekly · Fable"]);
        assert_eq!(w[1].used, 45.5);
        assert!(w[0].reset > 0);
        // The older shape.
        let v: Value = serde_json::from_str(r#"{"five_hour":{"utilization":12,"resets_at":"2999-01-01T00:00:00Z"},"seven_day":{"utilization":null}}"#).unwrap();
        assert_eq!(claude_windows(&v).len(), 1);
    }

    #[test]
    fn a_passed_reset_is_unused() {
        let w = settle(vec![Window { name: "5-hour".into(), used: 80.0, reset: 1 }]);
        assert_eq!((w[0].used, w[0].reset), (0.0, 0));
    }

    #[test]
    fn reads_hermes_insights() {
        let text = "  📋 Overview\n  ─────\n  Sessions:          83            Messages:        447\n  Tool calls:        306           User messages:   36\n  Total tokens:      6,238,657\n  Active time:       ~15h 36m      Avg session:     ~11m\n\n  💰 Cost\n  Estimated:          ~$3.55\n";
        let f = hermes_facts(text);
        assert_eq!(f.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>(), ["Sessions=83", "Messages=447", "Tool calls=306", "Tokens=6,238,657", "Active=~15h 36m", "Cost=~$3.55"]);
        assert!(hermes_facts("  No sessions found in the last 7 days (source: acp).\n").is_empty());
    }

    #[test]
    fn times() {
        assert_eq!(time(Some(&Value::from(1_700_000_000_000u64))), 1_700_000_000);
        assert_eq!(time(Some(&Value::from("2023-11-14T22:13:20Z"))), 1_700_000_000);
    }
}

//! Idle and locking, the shell's side: what happens as the computer sits untouched (dim, lock,
//! screens off, suspend: each after its own time, from the idle-notify protocol, which waits
//! while an app inhibits idle), starting the lock screen and starting it again if it dies,
//! locking before sleep (a logind delay inhibitor, let go once the screens are locked), and
//! `loginctl lock-session` / `unlock-session`.

use std::process::Child;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use calloop::channel::Sender;

use crate::config::Settings;
use crate::sys::{self, Compositor};
use crate::wl::Msg;

/// The steps of going idle, each with its own time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Dim,
    Lock,
    ScreenOff,
    Suspend,
}

impl Stage {
    pub const ALL: [Stage; 4] = [Stage::Dim, Stage::Lock, Stage::ScreenOff, Stage::Suspend];

    /// Its time in the settings, in seconds (0: never).
    pub fn secs(self, s: &Settings) -> u32 {
        match self {
            Stage::Dim => s.idle_dim,
            Stage::Lock => s.idle_lock,
            Stage::ScreenOff => s.idle_screen_off,
            Stage::Suspend => s.idle_suspend,
        }
    }
}

/// The settings that change the idle watch (when they change, it starts over).
pub fn key(s: &Settings) -> (bool, u32, u32, u32, u32) {
    (s.idle_on, s.idle_dim, s.idle_lock, s.idle_screen_off, s.idle_suspend)
}

/// Another program already watching for idle, which would do the same things twice.
pub fn other_daemon() -> Option<String> {
    const NAMES: [&str; 4] = ["hypridle", "swayidle", "xidlehook", "stasis"];
    let dir = std::fs::read_dir("/proc").ok()?;
    for e in dir.flatten() {
        if !e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Ok(c) = std::fs::read_to_string(e.path().join("comm")) {
            let c = c.trim();
            if NAMES.contains(&c) {
                return Some(c.to_string());
            }
        }
    }
    None
}

/// Turn the screens off or on again, the compositor's way. `outputs` are the screens' names
/// (mango sleeps each by name).
pub fn screens(c: Compositor, on: bool, outputs: &[String]) {
    match c {
        Compositor::Niri => sys::spawn("niri", &["msg", "action", if on { "power-on-monitors" } else { "power-off-monitors" }]),
        Compositor::Hyprland => sys::spawn("hyprctl", &["dispatch", "dpms", if on { "on" } else { "off" }]),
        Compositor::Mango => {
            for o in outputs {
                sys::spawn("mmsg", &["dispatch", &format!("{},{o}", if on { "wakeup_monitor" } else { "sleep_monitor" })]);
            }
        }
        Compositor::Sway => sys::spawn("swaymsg", &["output", "*", "power", if on { "on" } else { "off" }]),
        Compositor::Other => {}
    }
}

/// The lock screen program, while it runs.
#[derive(Default)]
pub struct Locker {
    child: Option<Child>,
    /// Starting it again after a crash: not more than a few times in a row.
    restarts: Vec<Instant>,
}

impl Locker {
    /// Lock now (nothing when already locked).
    pub fn lock(&mut self, s: &Settings) {
        if self.running() {
            return;
        }
        let mut cmd = if s.lock_cmd.trim().is_empty() {
            let Ok(exe) = std::env::current_exe() else { return };
            let mut c = std::process::Command::new(exe);
            c.arg("lock");
            c
        } else {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", s.lock_cmd.trim()]);
            c
        };
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
        match cmd.spawn() {
            Ok(c) => self.child = Some(c),
            Err(e) => eprintln!("rivet-shell: couldn't start the lock screen: {e}"),
        }
    }

    pub fn running(&mut self) -> bool {
        match &mut self.child {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Unlock (as `loginctl unlock-session` asks): SIGUSR1, which swaylock and hyprlock take too.
    pub fn unlock(&mut self) {
        if self.running()
            && let Some(c) = &self.child
        {
            unsafe {
                libc::kill(c.id() as i32, libc::SIGUSR1);
            }
        }
    }

    /// Now and then: the lock screen stopped while the screens are still locked (it crashed):
    /// start it again, so the password can still be given.
    pub fn check(&mut self, s: &Settings) {
        let Some(c) = &mut self.child else { return };
        let Ok(Some(status)) = c.try_wait() else { return };
        self.child = None;
        let crashed = !status.success() && status.code() != Some(crate::lock::EXIT_TAKEN as i32);
        if crashed && s.lock_cmd.trim().is_empty() && crate::lock::marker().exists() {
            let now = Instant::now();
            self.restarts.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
            if self.restarts.len() < 5 {
                self.restarts.push(now);
                eprintln!("rivet-shell: the lock screen stopped ({status}); starting it again");
                self.lock(s);
            }
        }
    }
}

/// A marker left by a lock screen that's no longer running (the shell started after a crash
/// of everything): taken away.
pub fn clear_stale_marker() {
    let m = crate::lock::marker();
    if let Some(pid) = std::fs::read_to_string(&m).ok().and_then(|t| t.trim().parse::<i32>().ok())
        && !std::path::Path::new(&format!("/proc/{pid}")).exists()
    {
        let _ = std::fs::remove_file(m);
    }
}

/// Watch logind on a thread: Lock and Unlock for this session (`loginctl lock-session`), and
/// the computer about to sleep. While `lock_on_sleep` holds, sleeping waits (a delay inhibitor,
/// a few seconds at most) until the lock screen has locked.
pub fn watch_logind(tx: Sender<Msg>, lock_on_sleep: Arc<AtomicBool>, own_locker: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        if let Err(e) = logind(&tx, &lock_on_sleep, &own_locker) {
            eprintln!("rivet-shell: logind: {e} (locking before sleep and loginctl lock-session won't work)");
        }
    });
}

fn logind(tx: &Sender<Msg>, lock_on_sleep: &AtomicBool, own_locker: &AtomicBool) -> Result<(), String> {
    use zbus::zvariant::{OwnedFd, OwnedObjectPath};
    let conn = zbus::blocking::Connection::system().map_err(|e| e.to_string())?;
    let manager = zbus::blocking::Proxy::new(&conn, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager").map_err(|e| e.to_string())?;
    let session: OwnedObjectPath = match std::env::var("XDG_SESSION_ID") {
        Ok(id) if !id.is_empty() => manager.call("GetSession", &(id,)),
        _ => manager.call("GetSessionByPID", &(std::process::id(),)),
    }
    .map_err(|e| format!("couldn't find this session ({e})"))?;
    let inhibit = || -> Option<OwnedFd> { manager.call("Inhibit", &("sleep", "Rivet Shell", "Locking the screen first", "delay")).ok() };
    let mut held = inhibit();
    let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).sender("org.freedesktop.login1").map_err(|e| e.to_string())?.build();
    let it = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, Some(64)).map_err(|e| e.to_string())?;
    for m in it {
        let Ok(m) = m else { continue };
        let h = m.header();
        let member = h.member().map(|x| x.to_string()).unwrap_or_default();
        let path = h.path().map(|p| p.to_string()).unwrap_or_default();
        match member.as_str() {
            "Lock" if path == session.as_str() => {
                let _ = tx.send(Msg::Ipc("lock".into()));
            }
            "Unlock" if path == session.as_str() => {
                let _ = tx.send(Msg::Ipc("unlock".into()));
            }
            "PrepareForSleep" => {
                let going: bool = m.body().deserialize().unwrap_or(false);
                if going {
                    if lock_on_sleep.load(Ordering::Relaxed) {
                        let _ = tx.send(Msg::Ipc("lock".into()));
                        // Rivet Shell's lock screen says when it has locked; another one gets a moment.
                        let until = Instant::now() + if own_locker.load(Ordering::Relaxed) { Duration::from_millis(3000) } else { Duration::from_millis(1000) };
                        while Instant::now() < until && !(own_locker.load(Ordering::Relaxed) && crate::lock::marker().exists()) {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        // A moment more for the locked screens to reach the display.
                        std::thread::sleep(Duration::from_millis(150));
                    }
                    held = None;
                } else if held.is_none() {
                    held = inhibit();
                }
            }
            _ => {}
        }
    }
    drop(held);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_read_their_settings() {
        let s = Settings { idle_dim: 10, idle_lock: 20, idle_screen_off: 30, idle_suspend: 0, ..Settings::default() };
        assert_eq!(Stage::ALL.map(|st| st.secs(&s)), [10, 20, 30, 0]);
    }
}

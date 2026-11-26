//! One shell (or program) in a pseudo-terminal: its screen and scrollback, kept up to date on
//! a reader thread, and what it asks of the window (a title, the bell, the clipboard).

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg, State};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty;
use eframe::egui;

/// The most scrollback a session keeps. Every line is stored at the pane's full width, about
/// 24 bytes a cell, so a 200-column pane at this limit can hold close to half a gigabyte.
pub const MAX_SCROLLBACK: usize = 100_000;

/// What to start, where, and how much to remember.
#[derive(Clone, Debug, Default)]
pub struct Spawn {
    /// The program and its arguments; None for the person's shell ($SHELL).
    pub program: Option<(String, Vec<String>)>,
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
    /// Lines of scrollback, up to [`MAX_SCROLLBACK`].
    pub scrollback: usize,
}

/// The size of the screen in cells (and of a cell, for programs that ask).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub lines: u16,
    pub cell_w: u16,
    pub cell_h: u16,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.lines as usize
    }
    fn screen_lines(&self) -> usize {
        self.lines as usize
    }
    fn columns(&self) -> usize {
        self.cols as usize
    }
}

impl From<Size> for WindowSize {
    fn from(s: Size) -> WindowSize {
        WindowSize { num_lines: s.lines, num_cols: s.cols, cell_width: s.cell_w, cell_height: s.cell_h }
    }
}

/// Passes the terminal's events to the window, waking it.
#[derive(Clone)]
pub struct Listener {
    tx: Sender<Event>,
    ctx: Option<egui::Context>,
    /// The window (viewport) the session is shown in, which is the one to wake.
    viewport: Arc<std::sync::Mutex<egui::ViewportId>>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.tx.send(event);
        if let Some(c) = &self.ctx {
            let vp = *self.viewport.lock().unwrap_or_else(|e| e.into_inner());
            c.request_repaint_of(vp);
        }
    }
}

/// Something the window should know about.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    Title(String),
    Bell,
    /// A program asked to put text on the clipboard (OSC 52).
    Clipboard(String),
    /// The program ended.
    Exited,
}

pub struct Session {
    pub term: Arc<FairMutex<Term<Listener>>>,
    sender: EventLoopSender,
    rx: Receiver<Event>,
    pub size: Size,
    /// The program's own title (OSC 0/2), when it set one.
    pub title: Option<String>,
    pub exited: bool,
    /// The program's process id, to find its current folder.
    pub pid: u32,
    /// The last bell, by the window's clock (for a short flash).
    pub bell_at: Option<f64>,
    viewport: Arc<std::sync::Mutex<egui::ViewportId>>,
    /// The reader thread, which owns the other reference to `term` until it ends.
    reader: Option<JoinHandle<(EventLoop<tty::Pty, Listener>, State)>>,
}

impl Session {
    /// Start a program in a new pseudo-terminal of `size`.
    pub fn spawn(spawn: &Spawn, size: Size, ctx: Option<egui::Context>) -> Result<Session, String> {
        let viewport = Arc::new(std::sync::Mutex::new(egui::ViewportId::ROOT));
        let mut env = spawn.env.clone();
        env.entry("TERM".into()).or_insert_with(|| "xterm-256color".into());
        env.entry("COLORTERM".into()).or_insert_with(|| "truecolor".into());
        env.entry("TERM_PROGRAM".into()).or_insert_with(|| "rivet-terminal".into());
        let options = tty::Options {
            shell: spawn.program.clone().map(|(p, a)| tty::Shell::new(p, a)),
            working_directory: spawn.cwd.clone(),
            drain_on_exit: true,
            env,
        };
        let pty = tty::new(&options, size.into(), 0).map_err(|e| format!("couldn't start the shell: {e}"))?;
        let pid = pty.child().id();
        let (tx, rx) = channel();
        let listener = Listener { tx, ctx, viewport: viewport.clone() };
        let config = Config { scrolling_history: spawn.scrollback.clamp(100, MAX_SCROLLBACK), ..Default::default() };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, listener.clone())));
        let event_loop = EventLoop::new(term.clone(), listener, pty, true, false).map_err(|e| e.to_string())?;
        let sender = event_loop.channel();
        let reader = Some(event_loop.spawn());
        Ok(Session { term, sender, rx, size, title: None, exited: false, pid, bell_at: None, viewport, reader })
    }

    /// Say which window shows the session now, so its output wakes that one (an app with
    /// several windows).
    pub fn show_in(&self, vp: egui::ViewportId) {
        let mut v = self.viewport.lock().unwrap_or_else(|e| e.into_inner());
        if *v != vp {
            *v = vp;
        }
    }

    /// Send bytes to the program, as if typed.
    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let _ = self.sender.send(Msg::Input(bytes.into()));
    }

    /// Fit the screen to a new size.
    pub fn resize(&mut self, size: Size) {
        if size == self.size || size.cols < 2 || size.lines < 1 {
            return;
        }
        self.size = size;
        self.term.lock().resize(size);
        let _ = self.sender.send(Msg::Resize(size.into()));
    }

    pub fn scroll(&self, s: Scroll) {
        self.term.lock().scroll_display(s);
    }

    /// Take in the terminal's events: answer what it asks, and pass the rest on. `now` is the
    /// window's clock.
    pub fn poll(&mut self, now: f64, colors: &crate::TermColors) -> Vec<Notice> {
        let mut out = Vec::new();
        while let Ok(e) = self.rx.try_recv() {
            match e {
                Event::Title(t) => {
                    self.title = Some(t.clone());
                    out.push(Notice::Title(t));
                }
                Event::ResetTitle => self.title = None,
                Event::Bell => {
                    self.bell_at = Some(now);
                    out.push(Notice::Bell);
                }
                Event::PtyWrite(s) => self.write(s.into_bytes()),
                Event::ColorRequest(i, fmt) => {
                    let c = {
                        let term = self.term.lock();
                        let set = |k: usize| term.colors()[k];
                        let named = match i {
                            256 => alacritty_terminal::vte::ansi::Color::Named(alacritty_terminal::vte::ansi::NamedColor::Foreground),
                            257 => alacritty_terminal::vte::ansi::Color::Named(alacritty_terminal::vte::ansi::NamedColor::Background),
                            258 => alacritty_terminal::vte::ansi::Color::Named(alacritty_terminal::vte::ansi::NamedColor::Cursor),
                            k => alacritty_terminal::vte::ansi::Color::Indexed(k.min(255) as u8),
                        };
                        colors.resolve(named, &set)
                    };
                    self.write(fmt(alacritty_terminal::vte::ansi::Rgb { r: c.r(), g: c.g(), b: c.b() }).into_bytes());
                }
                Event::TextAreaSizeRequest(fmt) => self.write(fmt(self.size.into()).into_bytes()),
                Event::ClipboardStore(_, text) => out.push(Notice::Clipboard(text)),
                Event::ChildExit(_) | Event::Exit if !self.exited => {
                    self.exited = true;
                    out.push(Notice::Exited);
                }
                _ => {}
            }
        }
        out
    }

    /// The program's current folder (Linux: from /proc).
    pub fn cwd(&self) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{}/cwd", self.pid)).ok()
    }

    /// Whether a program other than the shell is running in front (worth asking before closing).
    pub fn busy(&self) -> bool {
        if self.exited {
            return false;
        }
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{}/stat", self.pid)) else { return false };
        let Some(close) = stat.rfind(')') else { return false };
        let tpgid: i32 = stat[close + 2..].split_whitespace().nth(5).and_then(|v| v.parse().ok()).unwrap_or(-1);
        tpgid > 0 && tpgid as u32 != self.pid
    }

    /// The name of what's running in front (the shell, or the program it started).
    pub fn foreground(&self) -> Option<String> {
        // The terminal's foreground process group, read from the shell's stat line.
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", self.pid)).ok()?;
        let rest = &stat[stat.rfind(')')? + 2..];
        let tpgid: i32 = rest.split_whitespace().nth(5)?.parse().ok()?;
        let pid = if tpgid > 0 { tpgid as u32 } else { self.pid };
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some(comm.trim().to_string())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
        // The shell gets a hangup, as when a terminal window closes.
        unsafe {
            libc::kill(self.pid as i32, libc::SIGHUP);
        }
        // A long scrollback is freed in many small pieces that the allocator keeps for itself, so
        // the app would stay at its largest size after the pane is gone. Once the reader thread
        // and this session have let go of the screen, ask for the free memory to go back to the
        // system (one trim for all the panes of a closing window).
        let Some(reader) = self.reader.take() else { return };
        let term = Arc::downgrade(&self.term);
        let _ = std::thread::Builder::new().name("terminal cleanup".into()).spawn(move || {
            drop(reader.join());
            for _ in 0..100 {
                if term.strong_count() == 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            rivet_mem::alloc::trim_soon();
        });
    }
}

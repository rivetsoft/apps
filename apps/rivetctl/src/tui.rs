//! The full-screen list: every app by category with what's installed and what each channel
//! offers, to install, update, remove and move between channels, with each job's progress.
//! Releases are read and installs run on threads of their own; the screen never waits on them.
//!
//! Checks set the scene with `RIVETCTL_SELECT=APP` (the app picked), `RIVETCTL_DIALOG=remove,
//! channel or help` (a dialog open over it) and `RIVETCTL_JOB=APP` (that app shown half
//! downloaded), beside rivet-install's `RIVET_WELCOME_RELEASES` and `RIVET_WELCOME_INSTALLED`.

use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Cell, Clear, Gauge, Paragraph, Row as TableRow, Table, TableState, Wrap,
};
use ratatui::{DefaultTerminal, Frame};
use rivet_install::channel::{self, Channel};
use rivet_install::install::Stage;
use rivet_install::state::Row;

use crate::apps::{self, Done, World};

enum Msg {
    World(World),
    Finished(String, Result<String, String>),
}

#[derive(Clone, Debug, PartialEq)]
enum Dialog {
    Remove(String),
    /// The app, and the choice the cursor is on (the three channels, then "follow").
    Channel(String, usize),
    Help,
}

struct Ui {
    world: Option<World>,
    held: bool,
    /// The app picked, by id.
    selected: Option<String>,
    jobs: HashMap<String, Arc<Mutex<Stage>>>,
    dialog: Option<Dialog>,
    /// The last thing that happened, for the line at the bottom.
    note: String,
    checking: bool,
    quit_asked: bool,
    tx: Sender<Msg>,
}

fn load(tx: &Sender<Msg>, held: bool, from: Option<Vec<rivet_install::releases::Release>>) {
    let tx = tx.clone();
    std::thread::spawn(move || {
        let w = match from {
            Some(r) => World::from(r, None, held),
            None => World::load(held),
        };
        let _ = tx.send(Msg::World(w));
    });
}

impl Ui {
    fn rows(&self) -> Vec<&Row> {
        self.world
            .as_ref()
            .map(|w| apps::ordered(&w.rows))
            .unwrap_or_default()
    }

    fn current(&self) -> Option<&Row> {
        let rows = self.rows();
        let id = self.selected.as_ref()?;
        rows.into_iter().find(|r| &r.entry.id == id)
    }

    fn move_by(&mut self, d: isize) {
        let ids: Vec<String> = self.rows().iter().map(|r| r.entry.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        let at = self
            .selected
            .as_ref()
            .and_then(|s| ids.iter().position(|i| i == s))
            .unwrap_or(0) as isize;
        let to = (at + d).clamp(0, ids.len() as isize - 1) as usize;
        self.selected = Some(ids[to].clone());
    }

    fn start(&mut self, r: &Row) {
        let id = r.entry.id.clone();
        if self.jobs.contains_key(&id) {
            return;
        }
        let p = Arc::new(Mutex::new(Stage::Downloading { got: 0, total: 0 }));
        self.jobs.insert(id.clone(), p.clone());
        let (r, tx) = (r.clone(), self.tx.clone());
        std::thread::spawn(move || {
            let set = |s: Stage| {
                if let Ok(mut g) = p.lock() {
                    *g = s;
                }
            };
            let out = match apps::install(&r, &set) {
                Ok(Done::Installed(v)) => Ok(format!("Installed {} {}.", r.entry.title, v.label())),
                Ok(Done::Already(v)) => Ok(format!(
                    "{} {} is already installed.",
                    r.entry.title,
                    v.label()
                )),
                Err(e) => Err(format!("{}: {e}", r.entry.title)),
            };
            let _ = tx.send(Msg::Finished(r.entry.id.clone(), out));
        });
    }

    fn refresh(&mut self) {
        let releases = self.world.as_ref().map(|w| w.releases.clone());
        load(&self.tx, self.held, releases);
    }

    /// One key. False when it's time to go.
    fn key(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        if code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
            return false;
        }
        if let Some(d) = self.dialog.clone() {
            self.dialog_key(d, code);
            return true;
        }
        let quitting = matches!(code, KeyCode::Char('q') | KeyCode::Esc);
        if quitting {
            if self.jobs.is_empty() || self.quit_asked {
                return false;
            }
            self.quit_asked = true;
            self.note = "An install is still running. Press q again to quit anyway, which can leave that app half installed.".into();
            return true;
        }
        self.quit_asked = false;
        match code {
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::PageUp => self.move_by(-10),
            KeyCode::PageDown => self.move_by(10),
            KeyCode::Home | KeyCode::Char('g') => self.move_by(-1000),
            KeyCode::End | KeyCode::Char('G') => self.move_by(1000),
            KeyCode::Char('?') => self.dialog = Some(Dialog::Help),
            KeyCode::Char('h') => {
                self.held = !self.held;
                self.note = if self.held {
                    "Showing the apps held back from release.".into()
                } else {
                    "Held apps hidden.".into()
                };
                self.refresh();
            }
            KeyCode::Char('R') | KeyCode::F(5) => {
                self.checking = true;
                self.note = "Looking for new releases…".into();
                load(&self.tx, self.held, None);
            }
            KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('u') => {
                if let Some(r) = self.current().cloned() {
                    if r.installed.is_some() && r.update().is_none() && r.install_copy().is_none() {
                        self.note = format!("{} is up to date.", r.entry.title);
                    } else if r.latest.is_none() {
                        self.note = format!(
                            "{} has nothing released on the {} channel yet.",
                            r.entry.title,
                            r.channel.name()
                        );
                    } else {
                        self.start(&r);
                    }
                }
            }
            KeyCode::Char('U') => {
                let ups: Vec<Row> = self
                    .world
                    .as_ref()
                    .map(|w| {
                        rivet_install::state::updates(&w.rows)
                            .into_iter()
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                if ups.is_empty() {
                    self.note = "Every app is up to date.".into();
                }
                for r in ups {
                    self.start(&r);
                }
            }
            KeyCode::Char('r') | KeyCode::Delete => {
                if let Some(r) = self.current() {
                    self.dialog = match &r.installed {
                        Some(i) if i.ours => Some(Dialog::Remove(r.entry.id.clone())),
                        Some(i) => {
                            self.note = format!(
                                "{} can't be removed here: {}.",
                                r.entry.title,
                                i.left_alone()
                            );
                            None
                        }
                        None => {
                            self.note = format!("{} isn't installed.", r.entry.title);
                            None
                        }
                    };
                }
            }
            KeyCode::Char('c') => {
                if let Some(r) = self.current() {
                    let at = if r.own_channel {
                        Channel::ALL
                            .iter()
                            .position(|c| *c == r.channel)
                            .unwrap_or(0)
                    } else {
                        3
                    };
                    self.dialog = Some(Dialog::Channel(r.entry.id.clone(), at));
                }
            }
            _ => {}
        }
        true
    }

    fn dialog_key(&mut self, d: Dialog, code: KeyCode) {
        match d {
            Dialog::Help => self.dialog = None,
            Dialog::Remove(id) => match code {
                KeyCode::Char('r') | KeyCode::Char('y') => {
                    self.dialog = None;
                    if let Some(r) = self.rows().into_iter().find(|r| r.entry.id == id).cloned() {
                        self.note = match apps::remove(&r) {
                            Ok(()) => format!(
                                "Removed {}. Its settings and documents are still there.",
                                r.entry.title
                            ),
                            Err(e) => format!("{}: {e}", r.entry.title),
                        };
                        self.refresh();
                    }
                }
                KeyCode::Esc | KeyCode::Char('k') | KeyCode::Char('n') | KeyCode::Char('q') => {
                    self.dialog = None
                }
                _ => {}
            },
            Dialog::Channel(id, at) => match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.dialog = Some(Dialog::Channel(id, at.saturating_sub(1)))
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.dialog = Some(Dialog::Channel(id, (at + 1).min(3)))
                }
                KeyCode::Esc | KeyCode::Char('q') => self.dialog = None,
                KeyCode::Enter => {
                    self.dialog = None;
                    let pick = Channel::ALL.get(at).copied();
                    let Some(r) = self.rows().into_iter().find(|r| r.entry.id == id).cloned()
                    else {
                        return;
                    };
                    if let Err(e) = channel::set_own(&id, pick) {
                        self.note = e;
                        return;
                    }
                    let Some(w) = self.world.take() else { return };
                    // The same releases, read again with the new channel.
                    let w = World::from(w.releases, w.offline, self.held);
                    let now = w.rows.iter().find(|x| x.entry.id == id).cloned();
                    self.world = Some(w);
                    let name = now.as_ref().map(|n| n.channel.name()).unwrap_or("");
                    self.note = match pick {
                        Some(_) => format!("{} follows {name} now.", r.entry.title),
                        None => format!(
                            "{} follows Rivet Welcome's channel ({name}) again.",
                            r.entry.title
                        ),
                    };
                    // Installed: move it to what the channel offers now.
                    if let Some(n) = now.filter(|n| n.installed.is_some() && n.update().is_some()) {
                        self.start(&n);
                    }
                }
                _ => {}
            },
        }
    }

    fn take(&mut self, m: Msg) {
        match m {
            Msg::World(w) => {
                if self.checking {
                    self.checking = false;
                    self.note = match &w.offline {
                        Some(e) => format!("{e}. These are the releases from the last check."),
                        None => match rivet_install::state::updates(&w.rows).len() {
                            0 => "Every app is up to date.".into(),
                            1 => "1 update.".into(),
                            n => format!("{n} updates."),
                        },
                    };
                } else if let Some(e) = &w.offline
                    && self.world.is_none()
                {
                    self.note = format!("{e}. These are the releases from the last check.");
                }
                let first = w
                    .rows
                    .first()
                    .map(|_| apps::ordered(&w.rows)[0].entry.id.clone());
                self.world = Some(w);
                if self.current().is_none() {
                    self.selected = first;
                }
            }
            Msg::Finished(id, out) => {
                self.jobs.remove(&id);
                self.note = out.unwrap_or_else(|e| e);
                self.refresh();
            }
        }
    }
}

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

fn accent() -> Style {
    Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn draw(f: &mut Frame, ui: &Ui) {
    let [top, list, detail, keys] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(4),
        Constraint::Length(4),
        Constraint::Length(1),
    ])
    .areas(f.area());
    let Some(w) = &ui.world else {
        f.render_widget(Paragraph::new("Reading the releases…").style(dim()), list);
        return;
    };
    let ups = rivet_install::state::updates(&w.rows).len();
    let mut head = vec![
        Span::styled(" rivetctl ", accent()),
        Span::raw(format!(
            " The Rivet apps   Channel: {}",
            channel::default().name()
        )),
    ];
    if ups > 0 {
        head.push(Span::styled(
            format!("   {ups} update{}", if ups == 1 { "" } else { "s" }),
            Style::new().fg(Color::Yellow),
        ));
    }
    if ui.held {
        head.push(Span::styled("   Held apps shown", dim()));
    }
    f.render_widget(Paragraph::new(Line::from(head)), top);

    let mut lines = Vec::new();
    let mut select = None;
    let mut group = "";
    for r in apps::ordered(&w.rows) {
        let cat = apps::category(&r.entry.id);
        if cat != group {
            group = cat;
            lines.push(
                TableRow::new(vec![Cell::from(Line::from(Span::styled(
                    cat,
                    Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                )))])
                .top_margin(u16::from(!lines.is_empty())),
            );
        }
        if ui.selected.as_ref() == Some(&r.entry.id) {
            select = Some(lines.len());
        }
        let [s, a, e] = w.offered(&r.entry);
        let have = match &r.installed {
            Some(i) if i.ours => Span::styled(
                apps::short(i.version.as_ref()),
                Style::new().fg(Color::Green),
            ),
            Some(i) => Span::styled(
                format!("{} elsewhere", apps::short(i.version.as_ref())),
                dim(),
            ),
            None => Span::styled("-", dim()),
        };
        let offer = |v: Option<&rivet_install::releases::Version>, c: Channel| {
            let style = if c == r.channel {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                dim()
            };
            Span::styled(apps::short(v), style)
        };
        let status = if let Some(p) = ui.jobs.get(&r.entry.id) {
            let pct = p.lock().map(|s| (s.fraction() * 100.0) as u32).unwrap_or(0);
            Span::styled(format!("{pct}%"), accent())
        } else if let Some((_, v)) = r.update() {
            Span::styled(
                format!("Update to {}", v.short()),
                Style::new().fg(Color::Yellow),
            )
        } else if r.entry.held {
            Span::styled("Held", dim())
        } else {
            Span::raw("")
        };
        let mut name = vec![Span::raw(format!("  {}", r.entry.title))];
        if r.own_channel {
            name.push(Span::styled("  (own channel)", dim()));
        }
        lines.push(TableRow::new(vec![
            Cell::from(Line::from(name)),
            Cell::from(have),
            Cell::from(offer(s.as_ref(), Channel::Stable)),
            Cell::from(offer(a.as_ref(), Channel::Pre)),
            Cell::from(offer(e.as_ref(), Channel::Edge)),
            Cell::from(status),
        ]));
    }
    let widths = [
        Constraint::Min(28),
        Constraint::Length(16),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(14),
        Constraint::Length(18),
    ];
    let header = TableRow::new(["App", "Installed", "Stable", "Alpha", "Edge", ""]).style(dim());
    let table = Table::new(lines, widths)
        .header(header)
        .row_highlight_style(Style::new().bg(Color::Indexed(238)))
        .block(Block::bordered().border_style(dim()));
    let mut state = TableState::default().with_selected(select);
    f.render_stateful_widget(table, list, &mut state);

    if let Some(r) = ui.current() {
        let [text, bar] =
            Layout::vertical([Constraint::Length(3), Constraint::Length(1)]).areas(detail);
        let follows = if r.own_channel {
            format!("Follows {} (its own channel).", r.channel.name())
        } else {
            format!("Follows {}.", r.channel.name())
        };
        let mut l = vec![
            Line::from(vec![
                Span::styled(
                    format!(" {}", r.entry.title),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {}", r.entry.bin), dim()),
            ]),
            Line::from(format!(" {}", r.entry.blurb)),
        ];
        let where_ = match &r.installed {
            Some(i) if !i.ours => format!(
                " {follows} Installed {}, so it's updated there.",
                i.origin()
            ),
            _ => format!(" {follows}"),
        };
        l.push(Line::styled(where_, dim()));
        f.render_widget(Paragraph::new(l).wrap(Wrap { trim: false }), text);
        if let Some(p) = ui.jobs.get(&r.entry.id) {
            let s = p.lock().map(|s| s.clone()).unwrap_or(Stage::Installing);
            f.render_widget(
                Gauge::default()
                    .ratio(f64::from(s.fraction()).clamp(0.0, 1.0))
                    .label(s.label())
                    .gauge_style(Style::new().fg(Color::Cyan).bg(Color::Black)),
                bar,
            );
        } else if !ui.note.is_empty() {
            f.render_widget(Paragraph::new(format!(" {}", ui.note)), bar);
        }
    }

    let hint = |k: &'static str, what: &'static str| {
        [
            Span::styled(format!(" {k} "), accent()),
            Span::raw(format!("{what}  ")),
        ]
    };
    let mut ks: Vec<Span> = Vec::new();
    for (k, what) in [
        ("Enter", "Install"),
        ("U", "Update All"),
        ("c", "Channel"),
        ("r", "Remove"),
        ("R", "Check"),
        ("h", "Held"),
        ("?", "Help"),
        ("q", "Quit"),
    ] {
        ks.extend(hint(k, what));
    }
    f.render_widget(Paragraph::new(Line::from(ks)), keys);

    match &ui.dialog {
        Some(Dialog::Remove(id)) => {
            let title = w
                .rows
                .iter()
                .find(|r| &r.entry.id == id)
                .map(|r| r.entry.title.as_str())
                .unwrap_or(id);
            let area = centered(f.area(), 66, 7);
            f.render_widget(Clear, area);
            let body = vec![
                Line::from(""),
                Line::from(" Its program, launcher entry and icons go. Its settings and"),
                Line::from(" documents stay."),
                Line::from(""),
                Line::from(vec![
                    Span::styled(" r ", accent()),
                    Span::raw("Remove   "),
                    Span::styled(" Esc ", accent()),
                    Span::raw("Keep It"),
                ]),
            ];
            f.render_widget(
                Paragraph::new(body).block(Block::bordered().title(format!(" Remove {title}? "))),
                area,
            );
        }
        Some(Dialog::Channel(id, at)) => {
            let Some(r) = w.rows.iter().find(|r| &r.entry.id == id) else {
                return;
            };
            let offered = w.offered(&r.entry);
            let area = centered(f.area(), 70, 10);
            f.render_widget(Clear, area);
            let default = channel::default();
            let mut body = vec![Line::from("")];
            let names = ["Stable", "Alpha", "Edge"];
            for (i, name) in names.iter().enumerate() {
                let v = apps::short(offered[i].as_ref());
                let line = format!("  {name:<10} {v}");
                body.push(if i == *at {
                    Line::styled(line, Style::new().bg(Color::Indexed(238)))
                } else {
                    Line::from(line)
                });
            }
            let follow = format!("  Follow Rivet Welcome ({})", default.name());
            body.push(if *at == 3 {
                Line::styled(follow, Style::new().bg(Color::Indexed(238)))
            } else {
                Line::from(follow)
            });
            body.push(Line::from(""));
            body.push(Line::styled(
                " Edge is built from the newest code every night and can break.",
                dim(),
            ));
            body.push(Line::from(vec![
                Span::styled(" Enter ", accent()),
                Span::raw("Switch   "),
                Span::styled(" Esc ", accent()),
                Span::raw("Cancel"),
            ]));
            f.render_widget(
                Paragraph::new(body)
                    .block(Block::bordered().title(format!(" Channel for {} ", r.entry.title))),
                area,
            );
        }
        Some(Dialog::Help) => {
            let area = centered(f.area(), 64, 16);
            f.render_widget(Clear, area);
            let rows = [
                ("Up, Down", "Pick an app"),
                ("Enter or i", "Install it, or install its update"),
                ("U", "Install every update"),
                ("c", "Choose its channel: stable, alpha or edge"),
                ("r", "Remove it (its settings and documents stay)"),
                ("R or F5", "Look for new releases"),
                ("h", "Show or hide the apps held back from release"),
                ("q", "Quit"),
            ];
            let mut body = vec![Line::from("")];
            for (k, what) in rows {
                body.push(Line::from(vec![
                    Span::styled(format!("  {k:<12}"), accent()),
                    Span::raw(what),
                ]));
            }
            body.push(Line::from(""));
            body.push(Line::styled(
                "  Rivet Welcome sees every change made here.",
                dim(),
            ));
            body.push(Line::styled("  Press any key to close.", dim()));
            f.render_widget(
                Paragraph::new(body).block(Block::bordered().title(" Keys ")),
                area,
            );
        }
        None => {}
    }
}

fn event_loop(term: &mut DefaultTerminal, ui: &mut Ui, rx: &Receiver<Msg>) -> std::io::Result<()> {
    loop {
        while let Ok(m) = rx.try_recv() {
            ui.take(m);
        }
        term.draw(|f| draw(f, ui))?;
        if event::poll(Duration::from_millis(120))?
            && let Event::Key(k) = event::read()?
            && k.kind == KeyEventKind::Press
            && !ui.key(k.code, k.modifiers)
        {
            return Ok(());
        }
    }
}

pub fn run(held: bool) -> ExitCode {
    // SAFETY: isatty only looks at the descriptors.
    if unsafe { libc::isatty(0) != 1 || libc::isatty(1) != 1 } {
        eprintln!(
            "rivetctl: the full-screen list needs a terminal; see rivetctl --help for the commands"
        );
        return ExitCode::FAILURE;
    }
    let (tx, rx) = channel();
    let mut ui = Ui {
        world: None,
        held,
        selected: std::env::var("RIVETCTL_SELECT").ok(),
        jobs: HashMap::new(),
        dialog: None,
        note: String::new(),
        checking: false,
        quit_asked: false,
        tx,
    };
    load(&ui.tx, held, None);
    // Checks: a dialog open, or an app half downloaded, once the list is in.
    if let Ok(Msg::World(w)) = rx.recv_timeout(Duration::from_secs(30)) {
        ui.take(Msg::World(w));
    }
    if let Ok(id) = std::env::var("RIVETCTL_JOB") {
        ui.jobs.insert(
            id,
            Arc::new(Mutex::new(Stage::Downloading {
                got: 4_200_000,
                total: 9_800_000,
            })),
        );
    }
    let id = ui.selected.clone().unwrap_or_default();
    ui.dialog = match std::env::var("RIVETCTL_DIALOG").as_deref() {
        Ok("remove") => Some(Dialog::Remove(id)),
        Ok("channel") => Some(Dialog::Channel(id, 2)),
        Ok("help") => Some(Dialog::Help),
        _ => None,
    };
    let mut term = ratatui::init();
    let out = event_loop(&mut term, &mut ui, &rx);
    ratatui::restore();
    match out {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivetctl: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ui() -> Ui {
        let list = rivet_install::releases::parse(include_str!(
            "../../rivet-welcome/data/demo-releases.json"
        ))
        .unwrap();
        let (tx, _rx) = channel();
        let mut u = Ui {
            world: None,
            held: false,
            selected: None,
            jobs: HashMap::new(),
            dialog: None,
            note: String::new(),
            checking: false,
            quit_asked: false,
            tx,
        };
        let mut w = World::from(list, None, false);
        // Not what this computer happens to have.
        for r in &mut w.rows {
            r.installed = None;
        }
        u.take(Msg::World(w));
        u
    }

    fn screen(u: &Ui) -> String {
        let mut t = Terminal::new(TestBackend::new(110, 40)).unwrap();
        t.draw(|f| draw(f, u)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    #[test]
    fn lists_apps_by_category_and_moves() {
        let mut u = ui();
        let s = screen(&u);
        assert!(
            s.contains("Files and documents") && s.contains("Rivet Files") && s.contains("Stable"),
            "{s}"
        );
        let first = u.selected.clone().unwrap();
        u.key(KeyCode::Down, KeyModifiers::NONE);
        assert_ne!(u.selected.as_ref(), Some(&first));
        u.key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(u.selected.as_ref(), Some(&first));
        // Held apps stay out.
        assert!(!s.contains("Rivet Long Shot"));
    }

    #[test]
    fn dialogs_open_and_close() {
        let mut u = ui();
        u.key(KeyCode::Char('c'), KeyModifiers::NONE);
        assert!(matches!(u.dialog, Some(Dialog::Channel(_, 3))));
        assert!(screen(&u).contains("Follow Rivet Welcome"));
        u.key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(u.dialog.is_none());
        u.key(KeyCode::Char('?'), KeyModifiers::NONE);
        assert!(screen(&u).contains("Keys"));
        u.key(KeyCode::Char('x'), KeyModifiers::NONE);
        assert!(u.dialog.is_none());
        // Nothing installed: nothing to remove, and it says so.
        u.key(KeyCode::Char('r'), KeyModifiers::NONE);
        assert!(u.dialog.is_none() && u.note.contains("isn't installed"));
        assert!(!u.key(KeyCode::Char('q'), KeyModifiers::NONE));
    }
}

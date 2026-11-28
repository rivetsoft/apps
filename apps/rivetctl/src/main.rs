//! rivetctl: installs, updates, removes and builds the Rivet apps from the terminal. Run bare,
//! it opens a full-screen list; with a command it does one thing and ends, for scripts; and
//! `rivetctl mcp` lets an AI assistant do the same through the Model Context Protocol. It runs
//! on rivet-install, Rivet Welcome's engine, so both read and change the same things.

mod apps;
mod build;
mod doctor;
mod mcp;
mod tui;

use std::process::ExitCode;

use rivet_install::install::Stage;
use serde_json::{Value, json};

use apps::{Done, World};

const HELP: &str = "rivetctl: install, update and build the Rivet apps

Usage:
  rivetctl                          The apps in a full-screen list, to install, update, remove
                                    and move between channels
  rivetctl list                     Every app, with what's installed and what each channel offers
  rivetctl install APP... [--channel stable|alpha|edge]
                                    Install apps, or update them; --channel gives them that
                                    channel from now on
  rivetctl update [APP...|--all]    Install updates: these apps', or every app's
  rivetctl remove APP...            Remove apps (their settings and files stay)
  rivetctl channel [APP [CHANNEL]]  Show the channels, or give an app its own: stable, alpha,
                                    edge, or default to follow Rivet Welcome's again
  rivetctl build APP[@REF]          Build an app from source at a branch, tag or commit (main
                                    when there's none) and install it
  rivetctl doctor                   What's missing for the apps to install and run here
  rivetctl mcp                      Serve the Model Context Protocol on standard input and
                                    output, for AI assistants
  rivetctl -h, --help               This help
  rivetctl -V, --version            The version

Options:
  --json     Print JSON instead (list, install, update, remove, channel, doctor)
  --held     Include the apps held back from release

Apps go in ~/.local/bin and your app launcher, for you alone, from
https://github.com/rivetsoft/apps/releases. Rivet Welcome sees everything rivetctl does.";

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("rivetctl: {msg}");
    ExitCode::FAILURE
}

fn stderr_is_terminal() -> bool {
    // SAFETY: isatty only looks at the descriptor.
    unsafe { libc::isatty(2) == 1 }
}

/// Progress on one line of the terminal, written over as it changes.
fn progress(title: &str) -> impl Fn(Stage) + '_ {
    let live = stderr_is_terminal();
    move |s: Stage| {
        if live && !s.finished() {
            eprint!("\r\x1b[K{title}: {}", s.label());
        }
    }
}

fn clear_progress() {
    if stderr_is_terminal() {
        eprint!("\r\x1b[K");
    }
}

struct Opts {
    json: bool,
    held: bool,
    all: bool,
    channel: Option<String>,
    words: Vec<String>,
}

fn parse(args: Vec<String>) -> Result<Opts, String> {
    let mut o = Opts {
        json: false,
        held: false,
        all: false,
        channel: None,
        words: Vec::new(),
    };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => o.json = true,
            "--held" => o.held = true,
            "--all" => o.all = true,
            "--help" | "--version" => {}
            "--channel" => {
                o.channel = Some(
                    it.next()
                        .ok_or("--channel needs one of stable, alpha or edge")?,
                )
            }
            s if s.starts_with("--channel=") => {
                o.channel = Some(s["--channel=".len()..].to_string())
            }
            s if s.starts_with('-') && s.len() > 1 && !matches!(s, "-h" | "-V") => {
                return Err(format!("unknown option {s} (see rivetctl --help)"));
            }
            _ => o.words.push(a),
        }
    }
    Ok(o)
}

fn print_json(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn cli_list(o: &Opts) -> ExitCode {
    let w = World::load(o.held);
    if let Some(e) = &w.offline {
        eprintln!("rivetctl: {e} (showing the releases from the last check)");
    }
    if o.json {
        print_json(&Value::Array(
            apps::ordered(&w.rows)
                .iter()
                .map(|r| apps::row_json(&w, r))
                .collect(),
        ));
        return ExitCode::SUCCESS;
    }
    let mut group = "";
    println!(
        "{:<10} {:<16} {:<15} {:<10} {:<10} {:<15} channel",
        "app", "", "installed", "stable", "alpha", "edge"
    );
    for r in apps::ordered(&w.rows) {
        let cat = apps::category(&r.entry.id);
        if cat != group {
            println!("\n{cat}");
            group = cat;
        }
        let have = match &r.installed {
            Some(i) if i.ours => apps::short(i.version.as_ref()),
            Some(i) => format!(
                "{} ({})",
                apps::short(i.version.as_ref()),
                if i.running == rivet_install::local::Running::No {
                    "elsewhere"
                } else {
                    "running"
                }
            ),
            None => "-".into(),
        };
        let [s, a, e] = w.offered(&r.entry);
        let mark = match r.update() {
            Some((_, v)) => format!("  update: {}", v.short()),
            None => String::new(),
        };
        let channel = if r.own_channel {
            format!("{} (its own)", r.channel.name())
        } else {
            r.channel.name().to_string()
        };
        println!(
            "{:<10} {:<16} {:<15} {:<10} {:<10} {:<15} {channel}{mark}",
            r.entry.id,
            r.entry.title,
            have,
            apps::short(s.as_ref()),
            apps::short(a.as_ref()),
            apps::short(e.as_ref()),
        );
    }
    ExitCode::SUCCESS
}

/// Install or update each app named, or every update when `names` is empty and `all`.
fn cli_install(o: &Opts, update_only: bool) -> ExitCode {
    if !update_only && o.words.is_empty() {
        return fail("install which app? (see rivetctl list)");
    }
    let mut w = World::load(o.held);
    if let Some(e) = &w.offline {
        eprintln!("rivetctl: {e} (using the releases from the last check)");
    }
    if let Some(c) = &o.channel {
        if rivet_install::channel::Channel::parse(c).is_none() {
            return fail(format!("{c} isn't a channel: use stable, alpha or edge"));
        }
        for n in &o.words {
            match w.find(n, o.held).and_then(|r| apps::set_channel(r, c)) {
                Ok(_) => {}
                Err(e) => return fail(e),
            }
        }
        w = World::from(w.releases, w.offline, o.held);
    }
    let todo: Vec<&rivet_install::state::Row> = if o.words.is_empty() {
        rivet_install::state::updates(&w.rows)
    } else {
        let mut out = Vec::new();
        for n in &o.words {
            match w.find(n, o.held) {
                Ok(r) if update_only && r.installed.is_none() => {
                    return fail(format!(
                        "{} isn't installed; rivetctl install {} installs it",
                        r.entry.title, r.entry.id
                    ));
                }
                Ok(r) => out.push(r),
                Err(e) => return fail(e),
            }
        }
        out
    };
    if todo.is_empty() && !o.json {
        println!("Every app is up to date.");
        return ExitCode::SUCCESS;
    }
    let mut failed = false;
    let mut report = Vec::new();
    for r in todo {
        let result = apps::install(r, &progress(&r.entry.title));
        clear_progress();
        match &result {
            Ok(Done::Installed(v)) if !o.json => {
                println!("Installed {} {}.", r.entry.title, v.label())
            }
            Ok(Done::Already(v)) if !o.json => {
                println!("{} {} is already installed.", r.entry.title, v.label())
            }
            Err(e) if !o.json => eprintln!("{}: {e}", r.entry.title),
            _ => {}
        }
        failed |= result.is_err();
        report.push(match result {
            Ok(Done::Installed(v)) => {
                json!({ "app": r.entry.id, "ok": true, "installed": v.to_string() })
            }
            Ok(Done::Already(v)) => {
                json!({ "app": r.entry.id, "ok": true, "already": v.to_string() })
            }
            Err(e) => json!({ "app": r.entry.id, "ok": false, "error": e }),
        });
    }
    if o.json {
        print_json(&Value::Array(report));
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn cli_remove(o: &Opts) -> ExitCode {
    if o.words.is_empty() {
        return fail("remove which app? (see rivetctl list)");
    }
    let w = World::cached(o.held);
    let mut failed = false;
    let mut report = Vec::new();
    for n in &o.words {
        let result = w
            .find(n, o.held)
            .and_then(|r| apps::remove(r).map(|()| r.entry.title.clone()));
        match &result {
            Ok(t) if !o.json => println!("Removed {t}."),
            Err(e) if !o.json => eprintln!("rivetctl: {e}"),
            _ => {}
        }
        failed |= result.is_err();
        report.push(match result {
            Ok(_) => json!({ "app": n, "ok": true }),
            Err(e) => json!({ "app": n, "ok": false, "error": e }),
        });
    }
    if o.json {
        print_json(&Value::Array(report));
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn cli_channel(o: &Opts) -> ExitCode {
    let default = rivet_install::channel::default();
    match o.words.as_slice() {
        [] => {
            let own = rivet_install::channel::own();
            if o.json {
                print_json(
                    &json!({ "default": default.name(), "apps": own.iter().map(|(k, v)| (k.clone(), json!(v.name()))).collect::<serde_json::Map<_, _>>() }),
                );
                return ExitCode::SUCCESS;
            }
            println!(
                "Every app follows {} (Rivet Welcome's channel).",
                default.name()
            );
            let mut own: Vec<_> = own.into_iter().collect();
            own.sort_by(|a, b| a.0.cmp(&b.0));
            for (id, c) in own {
                println!("{id} follows {} instead.", c.name());
            }
            ExitCode::SUCCESS
        }
        [app] => {
            let w = World::cached(o.held);
            match w.find(app, o.held) {
                Ok(r) if o.json => {
                    print_json(
                        &json!({ "app": r.entry.id, "channel": r.channel.name(), "own_channel": r.own_channel }),
                    );
                    ExitCode::SUCCESS
                }
                Ok(r) => {
                    let whose = if r.own_channel {
                        "its own"
                    } else {
                        "Rivet Welcome's"
                    };
                    println!("{} follows {} ({whose}).", r.entry.title, r.channel.name());
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        [app, c] => {
            let w = World::cached(o.held);
            let r = match w.find(app, o.held) {
                Ok(r) => r,
                Err(e) => return fail(e),
            };
            let set = match apps::set_channel(r, c) {
                Ok(s) => s,
                Err(e) => return fail(e),
            };
            let w = World::from(w.releases, None, o.held);
            let Ok(r) = w.find(app, o.held) else {
                return ExitCode::SUCCESS;
            };
            let update = r.update().map(|(_, v)| v.label());
            if o.json {
                print_json(
                    &json!({ "app": r.entry.id, "channel": r.channel.name(), "own_channel": set.is_some(), "update": update }),
                );
                return ExitCode::SUCCESS;
            }
            match set {
                Some(c) => println!("{} follows {} now.", r.entry.title, c.name()),
                None => println!(
                    "{} follows Rivet Welcome's channel ({}) again.",
                    r.entry.title,
                    r.channel.name()
                ),
            }
            if let Some(v) = update {
                println!("Run rivetctl update {} to install {v}.", r.entry.id);
            } else if r.installed.is_none() && r.latest.is_some() {
                println!("Run rivetctl install {} to install it.", r.entry.id);
            }
            ExitCode::SUCCESS
        }
        _ => fail("usage: rivetctl channel [APP [stable|alpha|edge|default]]"),
    }
}

fn cli_build(o: &Opts) -> ExitCode {
    let [spec] = o.words.as_slice() else {
        return fail("build which app? rivetctl build APP[@BRANCH, TAG or COMMIT]");
    };
    let (name, reference) = build::split(spec);
    let w = World::cached(o.held);
    let r = match w.find(name, o.held) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    match build::build(&r.entry, reference, None) {
        Ok((v, commit)) => {
            println!(
                "Installed {} {v}, built from {reference} ({}).",
                r.entry.title,
                commit.get(..7).unwrap_or(&commit)
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn cli_doctor(o: &Opts) -> ExitCode {
    let w = World::cached(true);
    let checks = doctor::run(&w.rows);
    if o.json {
        print_json(&doctor::json(&checks));
    } else {
        print!("{}", doctor::text(&checks));
    }
    if checks.iter().any(|c| c.level == doctor::Level::Fail) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    rivet_install::set_host(rivet_install::Host {
        title: "rivetctl",
        version: env!("CARGO_PKG_VERSION"),
        app: "rivetctl",
    });
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().filter(|a| !a.starts_with("--")).cloned();
    if command.is_some() {
        args.remove(0);
    }
    let o = match parse(args) {
        Ok(o) => o,
        Err(e) => return fail(e),
    };
    match command.as_deref() {
        Some("-h" | "help") => println!("{HELP}"),
        Some("-V" | "version") => println!("rivetctl {}", env!("CARGO_PKG_VERSION")),
        Some("list" | "ls") => return cli_list(&o),
        Some("install") => return cli_install(&o, false),
        Some("update" | "upgrade") => {
            let o = if o.all {
                Opts {
                    words: Vec::new(),
                    ..o
                }
            } else {
                o
            };
            return cli_install(&o, true);
        }
        Some("remove" | "uninstall") => return cli_remove(&o),
        Some("channel") => return cli_channel(&o),
        Some("build") => return cli_build(&o),
        Some("doctor") => return cli_doctor(&o),
        Some("mcp") => return mcp::serve(),
        Some(other) => return fail(format!("unknown command {other} (see rivetctl --help)")),
        None if std::env::args().any(|a| a == "--help") => println!("{HELP}"),
        None if std::env::args().any(|a| a == "--version") => {
            println!("rivetctl {}", env!("CARGO_PKG_VERSION"))
        }
        None => return tui::run(o.held),
    }
    ExitCode::SUCCESS
}

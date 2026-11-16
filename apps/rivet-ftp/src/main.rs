//! Rivet FTP: a two-pane file-transfer window (this computer | the server, with a transfer
//! queue), and a command line that shares its saved connections with the window and with
//! plain ssh.

mod gui;
mod listing;
mod sftp;

use std::path::PathBuf;
use std::process::ExitCode;

use rivet_connections::{Connection, Store};

const HELP: &str = "Rivet FTP: saved connections for SFTP, FTP and FTPS

Usage:
  rivet-ftp                          Open the window
  rivet-ftp open NAME                Open the window and connect
  rivet-ftp list                     Show saved connections
  rivet-ftp add NAME ADDRESS [options]
                                     Save a connection, for example
                                       rivet-ftp add \"Work site\" sftp://me@example.com/var/www
      --key FILE                     SSH key to log in with
      --local FOLDER                 Folder to open on this computer
      --note TEXT                    A note for yourself
      --replace                      Replace a connection with the same name
  rivet-ftp remove NAME              Forget a connection
  rivet-ftp show NAME                Everything saved about a connection
  rivet-ftp url NAME                 Its address, for other programs
  rivet-ftp ssh NAME [COMMAND...]    Log in with ssh, in the saved remote folder
                                     (or run COMMAND there)
  rivet-ftp sftp NAME                Open the sftp command line there
  rivet-ftp ssh-config               How to reach saved connections from plain ssh,
                                     scp, rsync and git by name

  ssh and sftp take --print to show the command instead of running it.
  NAME can be the name or its short ssh form (\"Work site\" or work-site).

Addresses look like sftp://user@server:port/folder, ftp://… or ftps://…; user@server
alone means SFTP. Passwords are never saved in the connections file
(~/.config/rivet-ftp/connections.conf); SSH logins use your keys or ssh agent.";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // --screenshot FILE.png: draw the window in memory, save a picture of it and exit (checks).
    let mut screenshot = None;
    if let Some(i) = args.iter().position(|a| a == "--screenshot") {
        args.remove(i);
        if i < args.len() {
            screenshot = Some(PathBuf::from(args.remove(i)));
        }
    }
    match args.first().map(String::as_str) {
        None => return window(None, screenshot),
        Some("open") => return window(args.get(1).cloned(), screenshot),
        _ => {}
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-ftp: {e}");
            ExitCode::FAILURE
        }
    }
}

fn window(open: Option<String>, screenshot: Option<PathBuf>) -> ExitCode {
    if screenshot.is_some() {
        rivet_kit::headless::run([1100.0, 700.0], move |cc| Box::new(gui::FtpApp::new(cc, open, screenshot)));
        return ExitCode::SUCCESS;
    }
    let viewport = eframe::egui::ViewportBuilder::default().with_title("Rivet FTP").with_app_id("rivet-ftp").with_inner_size([1100.0, 720.0]).with_min_inner_size([640.0, 420.0]);
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    match eframe::run_native("Rivet FTP", options, Box::new(move |cc| Ok(Box::new(gui::FtpApp::new(cc, open, None))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rivet-ftp: couldn't open the window: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let Some(cmd) = args.first() else { return Ok(()) };
    let rest = &args[1..];
    match cmd.as_str() {
        "help" | "-h" | "--help" => println!("{HELP}"),
        "-V" | "--version" | "version" => println!("rivet-ftp {}", env!("CARGO_PKG_VERSION")),
        "list" | "ls" => list(&Store::load()),
        "add" => add(rest)?,
        "remove" | "rm" | "delete" => {
            let name = one_name(rest, "remove")?;
            let mut store = Store::load();
            let gone = store.remove(name).ok_or_else(|| not_found(name, &store))?;
            store.save().map_err(|e| format!("couldn't save {}: {e}", store.path.display()))?;
            println!("Removed \"{}\".", gone.name);
        }
        "show" | "info" => {
            let store = Store::load();
            let c = find(&store, one_name(rest, "show")?)?;
            show(c);
        }
        "url" => {
            let store = Store::load();
            println!("{}", find(&store, one_name(rest, "url")?)?.url());
        }
        "ssh" | "sftp" => {
            let print = rest.iter().any(|a| a == "--print");
            let mut words: Vec<String> = rest.iter().filter(|a| *a != "--print").cloned().collect();
            if words.first().is_some_and(|w| w == "--") {
                words.remove(0);
            }
            let Some(name) = words.first().cloned() else { return Err(format!("which connection? (rivet-ftp {cmd} NAME)")) };
            let mut command = words[1..].to_vec();
            if command.first().is_some_and(|w| w == "--") {
                command.remove(0);
            }
            let store = Store::load();
            let c = find(&store, &name)?;
            let argv = if cmd == "ssh" { c.ssh_args(&command)? } else {
                if !command.is_empty() {
                    return Err("sftp takes no command; use rivet-ftp ssh NAME COMMAND".into());
                }
                c.sftp_args()?
            };
            if print {
                println!("{cmd} {}", argv.iter().map(|a| quote_for_display(a)).collect::<Vec<_>>().join(" "));
                return Ok(());
            }
            use std::os::unix::process::CommandExt;
            // Replaces this process, so the terminal belongs to ssh.
            let err = std::process::Command::new(cmd).args(&argv).exec();
            return Err(format!("couldn't start {cmd}: {err}"));
        }
        "ssh-config" => {
            let store = Store::load();
            // Make sure the file exists even before the first save.
            if !store.ssh_config_path().exists() && !store.items.is_empty() {
                let _ = store.save();
            }
            let n = store.items.iter().filter(|c| c.ssh_config_block().is_some()).count();
            println!("Rivet FTP keeps an ssh config with your {n} SFTP connection(s) at\n  {}\n", store.ssh_config_path().display());
            println!("To reach them by name from ssh, scp, rsync and git, add this line near the top of ~/.ssh/config:\n");
            println!("  Include {}\n", tilde(&store.ssh_config_path()));
            if let Some(c) = store.items.iter().find(|c| c.ssh_config_block().is_some()) {
                println!("Then, for example:  ssh {}", c.alias());
            }
        }
        other => return Err(format!("unknown command \"{other}\" (see rivet-ftp --help)")),
    }
    Ok(())
}

fn add(rest: &[String]) -> Result<(), String> {
    let mut pos = Vec::new();
    let (mut key, mut local, mut note, mut replace) = (None, None, None, false);
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        let mut value = |flag: &str| it.next().cloned().ok_or_else(|| format!("{flag} needs a value"));
        match a.as_str() {
            "--key" => key = Some(value("--key")?),
            "--local" => local = Some(value("--local")?),
            "--note" => note = Some(value("--note")?),
            "--replace" => replace = true,
            f if f.starts_with("--") => return Err(format!("unknown option {f}")),
            _ => pos.push(a.clone()),
        }
    }
    let [name, address] = pos.as_slice() else {
        return Err("usage: rivet-ftp add NAME ADDRESS (for example: rivet-ftp add \"Work site\" sftp://me@example.com/var/www)".into());
    };
    let mut c = Connection::from_url(name, address)?;
    if key.is_some() && c.protocol != rivet_connections::Protocol::Sftp {
        return Err("--key is for SFTP (SSH) connections".into());
    }
    // Keep paths under home as ~/…, so the file still works after a move to another account.
    c.key = key.map(|k| tilde(std::path::Path::new(&k)));
    c.local = local.map(|l| tilde(std::path::Path::new(&l)));
    c.note = note;
    let mut store = Store::load();
    store.add(c.clone(), replace)?;
    store.save().map_err(|e| format!("couldn't save {}: {e}", store.path.display()))?;
    println!("Saved \"{}\" ({}).", c.name, c.url());
    if c.ssh_config_block().is_some() {
        println!("Log in with:  rivet-ftp ssh {}", c.alias());
    }
    Ok(())
}

fn list(store: &Store) {
    if store.items.is_empty() {
        println!("No saved connections yet. Add one with:\n  rivet-ftp add NAME sftp://user@server/folder");
        return;
    }
    let w = store.items.iter().map(|c| c.name.chars().count()).max().unwrap_or(4).max(4);
    println!("{:<w$}  {:<5}  ADDRESS", "NAME", "TYPE");
    for c in &store.items {
        let mut addr = c.target();
        if c.port.is_some_and(|p| p != c.protocol.default_port()) {
            addr.push_str(&format!(":{}", c.port()));
        }
        if let Some(r) = &c.remote {
            addr.push_str(&format!("  {r}"));
        }
        println!("{:<w$}  {:<5}  {addr}", c.name, c.protocol.scheme());
    }
}

fn show(c: &Connection) {
    println!("{}", c.name);
    println!("  Address:      {}", c.url());
    println!("  Type:         {}", c.protocol.scheme().to_uppercase());
    println!("  Server:       {}  (port {})", c.host, c.port());
    if let Some(u) = &c.user {
        println!("  User:         {u}");
    }
    if let Some(k) = &c.key {
        println!("  SSH key:      {k}");
    }
    if let Some(r) = &c.remote {
        println!("  Remote:       {r}");
    }
    if let Some(l) = &c.local {
        println!("  Local:        {l}");
    }
    if let Some(n) = &c.note {
        println!("  Note:         {n}");
    }
    if c.ssh_config_block().is_some() {
        println!("  From a terminal: rivet-ftp ssh {}   (or ssh {} with the Include line; see rivet-ftp ssh-config)", c.alias(), c.alias());
    }
}

fn one_name<'a>(rest: &'a [String], cmd: &str) -> Result<&'a str, String> {
    match rest {
        [n] => Ok(n),
        [] => Err(format!("which connection? (rivet-ftp {cmd} NAME)")),
        _ => Err(format!("rivet-ftp {cmd} takes one name; put names with spaces in quotes")),
    }
}

fn find<'a>(store: &'a Store, name: &str) -> Result<&'a Connection, String> {
    store.find(name).ok_or_else(|| not_found(name, store))
}

fn not_found(name: &str, store: &Store) -> String {
    if store.items.is_empty() {
        format!("no connection called \"{name}\" (none saved yet)")
    } else {
        let names: Vec<&str> = store.items.iter().map(|c| c.name.as_str()).collect();
        format!("no connection called \"{name}\"; saved: {}", names.join(", "))
    }
}

fn quote_for_display(a: &str) -> String {
    if !a.is_empty() && a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./@:=+,".contains(c)) {
        a.to_string()
    } else {
        format!("'{}'", a.replace('\'', r"'\''"))
    }
}

fn tilde(p: &std::path::Path) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let s = p.to_string_lossy();
    match s.strip_prefix(&home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
        _ => s.into_owned(),
    }
}

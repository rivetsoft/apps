//! `sudo rivet-greeter --setup`: make the shared folder, and say how to have greetd use the
//! greeter.

use std::os::unix::fs::PermissionsExt;
use std::process::ExitCode;

/// How greetd should name this program: plain `rivet-greeter` when the system's own folders
/// have it (a package, or a link to a build), since greetd's user has nobody's own folders on
/// its path. Otherwise where this program is, in quotes when the path has a space, which would
/// split the command in two.
fn command_name() -> String {
    let here = std::env::current_exe().ok();
    let real = here.as_ref().and_then(|p| std::fs::canonicalize(p).ok());
    if ["/usr/bin/rivet-greeter", "/usr/local/bin/rivet-greeter"].iter().any(|p| std::fs::canonicalize(p).ok() == real && real.is_some()) {
        return "rivet-greeter".to_string();
    }
    quoted(&here.map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "rivet-greeter".into()))
}

/// A path as one word of a shell command.
fn quoted(path: &str) -> String {
    if path.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c)) { path.to_string() } else { format!("'{}'", path.replace('\'', "'\\''")) }
}

/// Whether greetd's settings still start the greeter through Rivet Shell.
fn old_command(config: &str) -> bool {
    config.lines().filter(|l| !l.trim_start().starts_with('#')).any(|l| l.contains("rivet-shell greeter") || l.contains("rivet-shell\" greeter") || l.contains("rivet-shell' greeter"))
}

pub fn run() -> ExitCode {
    let root = rivet_lockscreen::handoff::dir();
    let root = root.as_path();
    if let Err(e) = std::fs::create_dir_all(root).and_then(|_| std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o1777))) {
        eprintln!("Couldn't make {} ({e}). Run this with sudo.", root.display());
        return ExitCode::FAILURE;
    }
    let me = command_name();
    println!(
        "Made {}. Rivet Shell leaves each person's wallpaper and look there by itself; on\n\
         another desktop, run `rivet-greeter sync --wallpaper PICTURE` as yourself.\n\n\
         To use the greeter, set greetd's default session in /etc/greetd/config.toml, with a\n\
         compositor to show it in. With cage:\n\n\
         [default_session]\n\
         command = \"cage -s -- {me}\"\n\
         user = \"greeter\"\n\n\
         With Softcore: command = \"softcore --greeter {me}\"\n\n\
         Or with Sway, a file with `exec \"{me}; swaymsg exit\"` in it, run as\n\
         `sway --config FILE`. Then change your wallpaper (or run rivet-greeter sync) so it\n\
         leaves yours, and restart greetd.",
        root.display()
    );
    if std::fs::read_to_string("/etc/greetd/config.toml").is_ok_and(|c| old_command(&c)) {
        println!(
            "\nYour greetd settings still start the greeter with `rivet-shell greeter`. That keeps\n\
             working for now, through this program, but Rivet Shell will stop offering it: change\n\
             it to {me} as above."
        );
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_with_spaces_stay_one_word() {
        assert_eq!(quoted("/usr/bin/rivet-greeter"), "/usr/bin/rivet-greeter");
        assert_eq!(quoted("/home/sam/My Code/rivet-greeter"), "'/home/sam/My Code/rivet-greeter'");
        assert_eq!(quoted("/a/it's"), "'/a/it'\\''s'");
    }

    #[test]
    fn finds_the_old_command() {
        assert!(old_command("[default_session]\ncommand = \"cage -s -- rivet-shell greeter\"\n"));
        assert!(old_command("command = \"cage -s -- '/opt/my rivet/rivet-shell' greeter\"\n"));
        assert!(!old_command("# command = \"cage -s -- rivet-shell greeter\"\ncommand = \"cage -s -- rivet-greeter\"\n"));
    }
}

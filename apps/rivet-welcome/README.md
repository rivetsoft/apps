# <img src="data/rivet-welcome.svg" width="40" alt=""> Rivet Welcome

Installs and updates the Rivet apps, and tells you when there's something new. It's optional:
every app can still be installed by hand from its release archive. **Just started:** not
released yet; build it from source.

![Rivet Welcome with two updates and an install under way](../../docs/screenshots/welcome-wide.png)

- **Every app in one place.** Rivet Files, Notes, Edit, Terminal, FTP, Shell and Welcome
  itself, each with what's installed, what's out, and Install, Update, Open or Remove. An app
  released after this version (a tag like `rivet-something-v0.1.0` with a Linux archive) shows
  up too, so nothing waits for Rivet Welcome to learn its name.
- **Installs the way the archives do.** It downloads the release archive for this computer,
  checks it against the `.sha256` published with it, and runs the `install.sh` inside: the
  program goes in `~/.local/bin` and the app in your launcher. No password. Removing takes the
  program and its launcher entry and icon; settings and documents stay.
- **Leaves other installs alone.** An app installed by a package (AUR, a distribution, a
  build in `/usr/local/bin`) is shown with its version but not updated or removed here.
- **A tray icon.** It starts when you sign in and looks for releases straight away, then every
  few hours (Settings: every 3, 6 or 12 hours, daily, or only at sign-in). When there are
  updates it shows a dot and sends a notification once. Click it to open the window; its menu
  has Check for Updates and Install Updates. It works with any tray that speaks
  StatusNotifierItem: Rivet Shell, KDE, waybar and others. Turn it off in Settings.

## From a terminal

```sh
rivet-welcome                # the window (and the tray, when it's on)
rivet-welcome --tray         # only the tray
rivet-welcome list           # every app: installed and released versions
rivet-welcome check          # look for updates (exit status 2 when there are some)
rivet-welcome install notes edit
rivet-welcome update         # install every update
rivet-welcome remove ftp
```

It needs `curl`, `tar` and `sha256sum`, which nearly every Linux system has.

## Build and run

```sh
cargo build --release -p rivet-welcome
./target/release/rivet-welcome --install-desktop-entry   # add it to the launcher
./target/release/rivet-welcome
```

Or `utilities/rivet-dev install welcome`. Rivet Welcome can then install itself from its
releases into `~/.local/bin` like the others, and the tray's sign-in entry follows it.

## Where things are

| What | Where |
| --- | --- |
| Settings | `~/.config/rivet-welcome/settings` |
| The tray's sign-in entry | `~/.config/autostart/rivet-welcome-tray.desktop` |
| The last releases list | `~/.cache/rivet-welcome/releases.json` |
| Programs it installs | `~/.local/bin` (or `$XDG_BIN_HOME`) |

For testing: `RIVET_WELCOME_REPO=owner/name` looks at another repository's releases, and
`RIVET_WELCOME_RELEASES=FILE` reads the list from a file (GitHub's JSON) instead.

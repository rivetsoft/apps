<p>
  <img src="apps/rivet-files/data/rivet-files.svg" width="88" alt="Rivet Files icon">
  &nbsp;
  <img src="apps/rivet-ftp/data/rivet-ftp.svg" width="88" alt="Rivet FTP icon">
  &nbsp;
  <img src="apps/rivet-notes/data/rivet-notes.svg" width="88" alt="Rivet Notes icon">
  &nbsp;
  <img src="apps/rivet-edit/data/rivet-edit.svg" width="88" alt="Rivet Edit icon">
  &nbsp;
  <img src="apps/rivet-terminal/data/rivet-terminal.svg" width="88" alt="Rivet Terminal icon">
</p>

# Rivet Apps

[![CI](https://github.com/ulinaaron/rivet-apps/actions/workflows/ci.yml/badge.svg)](https://github.com/ulinaaron/rivet-apps/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/ulinaaron/rivet-apps?label=release)](https://github.com/ulinaaron/rivet-apps/releases/latest)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![Platform: Linux](https://img.shields.io/badge/platform-Linux-informational)](#the-apps)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust%20%2B%20egui-orange)](https://github.com/emilk/egui)
![No GTK or Qt](https://img.shields.io/badge/GTK%20%2F%20Qt-none-success)

A family of Linux desktop apps written in Rust, with no GTK or Qt. They share one look: your
desktop's colors, fonts and icon theme, light or dark, and the same menus, toolbars and
Preferences.

## The apps

### <img src="apps/rivet-files/data/rivet-files.svg" width="32" alt=""> Rivet Files

A file manager in the same vein as Dolphin, Nemo, Thunar and Caja. Minimal or packed with
panels, your choice: Peek previews, split view, filters, bulk rename, Quick Open, and it can be
your other apps' file picker.
**Stable:** released, with AppImage, archive, Flatpak and AUR packaging.
[More about Rivet Files](apps/rivet-files/README.md)

![Rivet Files showing a home folder](docs/screenshots/icons-wide.png)

### <img src="apps/rivet-ftp/data/rivet-ftp.svg" width="32" alt=""> Rivet FTP

A dual-pane file-transfer client: this computer on one side, the server on the other, and a
transfer queue. Saved connections in groups, tabs, editing server files in place, SSH keys, and
saved connections that also work from the command line and plain `ssh`.
**Early alpha, first release 0.1.0:** SFTP works; FTP and FTPS aren't built yet.
[More about Rivet FTP](apps/rivet-ftp/README.md)

![Rivet FTP connected to a server, with finished transfers](docs/screenshots/ftp-connected-wide.png)

### <img src="apps/rivet-notes/data/rivet-notes.svg" width="32" alt=""> Rivet Notes

Markdown notes in plain folders. Live editing, properties from the frontmatter, links between
notes with backlinks, folders and tags, a slash menu, and modules for daily notes and super
todos with a list or board.
**Early alpha, first release 0.1.0:** usable, but young.
[More about Rivet Notes](apps/rivet-notes/README.md)

![Rivet Notes in live mode, with Peek open](docs/screenshots/notes-live-wide.png)

### <img src="apps/rivet-edit/data/rivet-edit.svg" width="32" alt=""> Rivet Edit

A lightweight code editor: open a folder or single files, tree-sitter colors for a dozen
languages, Go to File (Ctrl+P) and Commands (Shift+Ctrl+P) on the same quick panel as the other
apps.
**Just started:** not released yet; build it from source.
[More about Rivet Edit](apps/rivet-edit/README.md)

![Rivet Edit with a Rust project open](docs/screenshots/edit-editor-wide.png)

### <img src="apps/rivet-terminal/data/rivet-terminal.svg" width="32" alt=""> Rivet Terminal

A terminal with tabs and split panes in layouts (Tall, Fat, Grid, Side by Side, Stacked), in
your desktop's colors or any of Rivet Edit's color themes. Its keys use Ctrl+Shift, so plain
Ctrl+letters reach your programs.
**Just started:** not released yet; build it from source.
[More about Rivet Terminal](apps/rivet-terminal/README.md)

![Rivet Terminal with three panes](docs/screenshots/terminal-wide.png)

> **About the alphas.** Rivet FTP and Rivet Notes are young: things may still change or break.
> Their releases are marked as pre-releases. Use them on servers and notes you have backups of.

## Install

Every app has its own releases on the [Releases page](https://github.com/ulinaaron/rivet-apps/releases),
told apart by name and tag:

| App | Tags | Downloads |
| --- | --- | --- |
| Rivet Files | `v0.7.0` | AppImage, archive, Flatpak bundle; AUR packages. See the [Rivet Files README](apps/rivet-files/README.md#-install) for the libraries the window needs. |
| Rivet FTP | `rivet-ftp-v0.1.0` | An archive (pre-release) |
| Rivet Notes | `rivet-notes-v0.1.0` | An archive (pre-release) |
| Rivet Edit | `rivet-edit-v…` | Not released yet: build from source |
| Rivet Terminal | `rivet-terminal-v…` | Not released yet: build from source |

For Rivet FTP or Rivet Notes, unpack the archive and run its installer, which puts the program in
`~/.local/bin` and adds it to your app launcher:

```sh
tar xzf rivet-notes-0.1.0-x86_64-linux.tar.gz
cd rivet-notes-0.1.0-x86_64-linux
./install.sh
```

Or build any of them from source (below).

## Build from source

You need Rust 1.88 or newer. From the top of the repository:

```sh
cargo build --release                    # Rivet Files (the default)
cargo build --release -p rivet-ftp       # Rivet FTP
cargo build --release -p rivet-notes     # Rivet Notes
cargo build --release -p rivet-edit      # Rivet Edit
cargo build --release -p rivet-terminal  # Rivet Terminal
cargo build --release --workspace        # everything

./target/release/rivet [FOLDER]
./target/release/rivet-ftp
./target/release/rivet-notes [VAULT]
./target/release/rivet-edit [FOLDER] [FILE...]
./target/release/rivet-terminal [-d FOLDER] [-e PROGRAM ARGS...]
```

`utilities/rivet-dev help` lists the developer tasks: QA, screenshots, packaging and releases.
Each app is released on its own, with its own version, changelog and tags (`v0.7.0` for Rivet
Files, `rivet-notes-v0.1.0`, `rivet-ftp-v0.1.0`). `utilities/rivet-dev apps` shows where each
one stands; then `bump notes` and `release notes` (or `ftp`, or `files`, the default).
`utilities/rivet-dev install notes` installs one for you from source.

## Repository layout

| Folder | What it is |
| --- | --- |
| `apps/rivet-files` | Rivet Files: its code, data, changelog and packaging (Flatpak, AUR) |
| `apps/rivet-ftp` | Rivet FTP (early alpha): code, data, changelog |
| `apps/rivet-notes` | Rivet Notes (early alpha): code, data, changelog |
| `apps/rivet-edit` | Rivet Edit (just started): code, data, changelog |
| `apps/rivet-terminal` | Rivet Terminal (just started): code, data, changelog |
| `crates/rivet-term` | Shared: the terminal (a shell in a pseudo-terminal, drawn as a widget); Rivet Terminal's, and later Rivet Edit's panel |
| `crates/rivet-xdg` | Shared: XDG folders, bookmarks and drives, file types, default apps and Open With |
| `crates/rivet-look` | Shared: the theme, desktop colors, fonts, icon themes, Rivet's own icons, code colors, color schemes, in-app windows |
| `crates/rivet-kit` | Shared: in-memory screenshots for checks |
| `crates/rivet-media` | Shared: previews, thumbnails and the light box |
| `crates/rivet-connections` | Shared: saved server connections (Rivet FTP's window, its command line, and `ssh`) |
| `utilities/` | Developer and release scripts (`utilities/rivet-dev`) |
| `docs/` | Documentation, written as wiki pages ([index](docs/README.md)) |

The top of the repository is only the Cargo workspace. CI builds, tests and lints all of it.

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

Rivet Edit's file and folder icons are from [Material Design Icons](https://pictogrammers.com/library/mdi/)
by Pictogrammers, under the Apache License 2.0.
Rivet Terminal's terminal emulation is [alacritty_terminal](https://github.com/alacritty/alacritty) (Apache License 2.0).

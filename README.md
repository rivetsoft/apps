<img src="apps/rivet-files/data/rivet-files.svg" width="96" alt="Rivet icon">

# Rivet Apps

[![CI](https://github.com/ulinaaron/rivet-files/actions/workflows/ci.yml/badge.svg)](https://github.com/ulinaaron/rivet-files/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/ulinaaron/rivet-files?label=release)](https://github.com/ulinaaron/rivet-files/releases/latest)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![Platform: Linux](https://img.shields.io/badge/platform-Linux-informational)](#the-apps)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust%20%2B%20egui-orange)](https://github.com/emilk/egui)
![No GTK or Qt](https://img.shields.io/badge/GTK%20%2F%20Qt-none-success)

A family of Linux desktop apps written in Rust, with no GTK or Qt. They share one look: your
desktop's colors, fonts and icon theme, light or dark, and the same menus, toolbars and
Preferences.

![Rivet Files showing a home folder](docs/screenshots/icons-wide.png)

## The apps

| App | What it is | Status |
| --- | --- | --- |
| [**Rivet Files**](apps/rivet-files/README.md) | A file manager in the same vein as Dolphin, Nemo, Thunar and Caja. Minimal or packed with panels, your choice. | **Stable.** Released, with AppImage, archive, Flatpak and AUR packaging. |
| [**Rivet FTP**](apps/rivet-ftp/README.md) | A dual-pane file-transfer client: this computer on one side, the server on the other, and a transfer queue. Saved connections also work from the command line and `ssh`. | **Early alpha.** Not ready for daily use. |
| [**Rivet Notes**](apps/rivet-notes/README.md) | Markdown notes in plain folders. Vaults, frontmatter as properties, notes by folder or by tag. | **Early alpha.** Not ready for daily use. |

> **About the alphas.** Rivet FTP and Rivet Notes are as early as it gets: features are missing,
> things change without notice, and there are no releases or packages yet. Build them from
> source to try them, on servers and notes you have backups of.

## Install

**Rivet Files**: grab it from the [Releases page](https://github.com/ulinaaron/rivet-files/releases)
(AppImage, archive or Flatpak bundle). The [Rivet Files README](apps/rivet-files/README.md#-install)
has the details and the libraries the window needs.

**Rivet FTP and Rivet Notes**: from source only, for now (see below).

## Build from source

You need Rust 1.88 or newer. From the top of the repository:

```sh
cargo build --release                    # Rivet Files (the default)
cargo build --release -p rivet-ftp       # Rivet FTP
cargo build --release -p rivet-notes     # Rivet Notes
cargo build --release --workspace        # everything

./target/release/rivet [FOLDER]
./target/release/rivet-ftp
./target/release/rivet-notes [VAULT]
```

`utilities/rivet-dev help` lists the developer tasks (QA, screenshots, releases, packaging);
they are for Rivet Files.

## Repository layout

| Folder | What it is |
| --- | --- |
| `apps/rivet-files` | Rivet Files: its code, data, changelog and packaging (Flatpak, AUR) |
| `apps/rivet-ftp` | Rivet FTP (early alpha) |
| `apps/rivet-notes` | Rivet Notes (early alpha) |
| `crates/rivet-xdg` | Shared: XDG folders, bookmarks and drives, file types, default apps and Open With |
| `crates/rivet-look` | Shared: the theme, desktop colors, fonts, icon themes, Rivet's own icons, code colors, in-app windows |
| `crates/rivet-kit` | Shared: in-memory screenshots for checks |
| `crates/rivet-media` | Shared: previews, thumbnails and the light box |
| `crates/rivet-connections` | Shared: saved server connections (Rivet FTP's window, its command line, and `ssh`) |
| `utilities/` | Developer and release scripts (`utilities/rivet-dev`) |
| `docs/` | Documentation, written as wiki pages ([index](docs/README.md)) |

The top of the repository is only the Cargo workspace. CI builds, tests and lints all of it.

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

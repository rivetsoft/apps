# Development

How to build the apps from source, where the code lives, and how releases are made. For the
conventions a change should follow (words, commits, changelogs), see [AGENTS.md](../AGENTS.md);
for how the apps share code, see [Shared crates](shared-crates.md).

## Building

You need Rust 1.88 or newer. The repository is one Cargo workspace. Plain `cargo build` builds
Rivet Files only, so name any other app with `-p`:

```sh
cargo build --release                    # Rivet Files (the program is `rivet`)
cargo build --release -p rivet-shell     # or rivet-ftp, rivet-notes, rivet-edit, rivet-terminal, rivet-welcome, rivet-snip, rivet-color
cargo build --release --workspace        # everything
```

The programs land in `target/release/`. Each app's README lists its own options and how to run
it:
[Shell](../apps/rivet-shell/README.md),
[Files](../apps/rivet-files/README.md),
[FTP](../apps/rivet-ftp/README.md),
[Notes](../apps/rivet-notes/README.md),
[Edit](../apps/rivet-edit/README.md),
[Terminal](../apps/rivet-terminal/README.md),
[Welcome](../apps/rivet-welcome/README.md),
[Snip](../apps/rivet-snip/README.md),
[Color](../apps/rivet-color/README.md).

`utilities/rivet-dev install notes` (or `ftp`, `files`…) builds an app and installs it for you,
in `~/.local/bin` and your app launcher.

### Debug builds

Debug builds (plain `cargo build`) are optimized enough to use every day: the workspace's own
crates at opt-level 1, dependencies at 2 (rivet-kit and rivet-cal at 3), with debug assertions
and debuginfo kept. Dependencies are compiled once and cached, so rebuilding after an edit
takes as long as it did unoptimized. Softcore's repository sets the same.

The first debug build after this change (November 2026) recompiles the workspace's crates once,
about a minute; dependencies aren't rebuilt. Measured on a 22-core machine, `-j 12`:

| | opt-level 0 (before) | opt-level 1 (now) | release |
| --- | ---: | ---: | ---: |
| Rebuild after editing `rivet-files` | 3.2–3.3 s | 3.2–3.4 s | |
| Rebuild after editing `rivet-shell` | 4.7–4.9 s | 4.8–5.0 s | |
| Rivet Shell, one panel draw (median) | 17–21 ms | 5.8–6.0 ms | 5.7–5.8 ms |
| Rivet Shell, idle CPU (6 windows, 40 s) | 2.8–3.0% | 1.45% | 1.3–1.4% |
| Rivet Files, sort 100,000 names | 1.08 s | 0.135 s | 0.12 s |
| Rivet Files, read 100,000 items | 1.33 s | 0.27 s | 0.24 s |

The one-time recompile took 75 s for Files and Shell together. A step through code in a
debugger may show some variables as optimized out; for one crate you are debugging, set
`[profile.dev.package.NAME] opt-level = 0` locally.

Release downloads (the archive Rivet Welcome installs, and the AppImage) are built with the
`dist` profile (fat LTO, one codegen unit: 13–20% smaller, about 30% longer to build) by
`utilities/rivet-dev release` and `appimage`. Builds from source (`cargo build --release`,
`utilities/rivet-dev install`, the AUR package and the Flatpak) keep the faster release profile.

## Checking a change

```sh
cargo test -p rivet-shell                                # one app's tests
cargo clippy --workspace --all-targets -- -D warnings    # what CI runs: warnings fail it
utilities/rivet-dev qa                                   # Rivet Files: every dialog and common workflow
```

Every app can draw itself into a PNG without a display (`--screenshot out.png`), and
`utilities/rivet-dev screenshots` (or `notes-`, `ftp-`, `edit-` or `shell-screenshots`)
rebuilds the pictures in `docs/screenshots/` from throwaway sample data. `utilities/rivet-dev help` lists
every developer task; the scripts behind it are in `utilities/lib/`.

## Releasing

Each app is released on its own, with its own version, changelog and tags: `v0.10.0` for Rivet
Files, `rivet-notes-v0.4.1` and so on. In short:

1. Keep notes under `## Unreleased` in the app's `CHANGELOG.md` as you work.
2. `utilities/rivet-dev bump notes patch` in a pull request, and merge it.
3. `utilities/rivet-dev ship notes` on `main`: tags and pushes, and GitHub builds a draft release.
4. `utilities/rivet-dev live notes` publishes it and rebuilds the website.

[Releasing](releasing.md) has every step, what a release contains, what Rivet Welcome reads from
it, and what to do when something goes wrong.

Release programs are linked with Zig against glibc 2.31, so they run on older systems such as
Ubuntu 22.04 and Debian 12. Rivet Files' packaging (AppImage, Flatpak, AUR) is described in
[its packaging notes](../apps/rivet-files/packaging/README.md).

## Repository layout

| Folder | What it is |
| --- | --- |
| `apps/rivet-shell` | Rivet Shell: the panel, dock, launcher, quick settings, widgets and wallpaper, and its Settings window |
| `apps/rivet-files` | Rivet Files, with its packaging (Flatpak, AUR) |
| `apps/rivet-ftp` | Rivet FTP |
| `apps/rivet-notes` | Rivet Notes |
| `apps/rivet-edit` | Rivet Edit |
| `apps/rivet-terminal` | Rivet Terminal |
| `apps/rivet-welcome` | Rivet Welcome: installs and updates the apps, with a tray icon |
| `apps/rivet-snip` | Rivet Snip: clips and marks up screenshots |
| `apps/rivet-color` | Rivet Color: picks colors from the screen, with shades, harmonies and palettes |
| `crates/rivet-look` | The shared look: the theme, desktop colors, fonts, icon themes, Rivet's own icons, code colors, color schemes, in-app windows |
| `crates/rivet-kit` | A software renderer for egui: screenshots, and Rivet Shell's surfaces |
| `crates/rivet-xdg` | XDG folders, bookmarks and drives, file types, default apps and Open With |
| `crates/rivet-media` | Previews, thumbnails and the light box |
| `crates/rivet-term` | The terminal: a shell in a pseudo-terminal, drawn as a widget |
| `crates/rivet-git` | Git with no interface, by running `git` |
| `crates/rivet-connections` | Saved server connections, shared by Rivet FTP's window, its command line and `ssh` |
| `utilities/` | Developer and release scripts (`utilities/rivet-dev`) |
| `docs/` | These pages ([index](README.md)) |

Each app folder has its own `README.md`, `CHANGELOG.md` and `data/` (its `.desktop` file and
icon). CI builds, tests and lints the whole workspace with `--locked`, so commit `Cargo.lock`
with any dependency change.

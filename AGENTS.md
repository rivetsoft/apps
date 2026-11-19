# Rivet: notes for agents

Rivet is an ecosystem of Linux desktop software in Rust and egui, with no GTK or Qt: a desktop
shell, a set of apps, and a helper that installs and updates them. Every piece works on its own
and is released on its own, but together they behave like one system. This file is what an
agent needs before changing anything; the README is the tour for people.

## The map

One Cargo workspace. Plain `cargo build` builds Rivet Files only; use `-p NAME` for another
app, or `--workspace` for everything.

| App | Program | What it is |
| --- | --- | --- |
| `apps/rivet-files` | `rivet` | The file manager. The stable one, with AppImage, Flatpak and AUR packaging. Can be the other apps' Open and Save window. |
| `apps/rivet-shell` | `rivet-shell` | The desktop for Wayland compositors with layer-shell (niri, Hyprland, Sway, mango): panels, dock, launcher, quick settings, notifications, lock screen, widgets, wallpaper. Unreleased; built from source. |
| `apps/rivet-edit` | `rivet-edit` | The code editor. |
| `apps/rivet-terminal` | `rivet-terminal` | Tabs and split panes around `crates/rivet-term`. |
| `apps/rivet-notes` | `rivet-notes` | Markdown notes in plain folders (vaults), with frontmatter, tags and tasks. |
| `apps/rivet-ftp` | `rivet-ftp` | A two-pane file-transfer client. SFTP works; FTP and FTPS are not built yet. |
| `apps/rivet-welcome` | `rivet-welcome` | Installs and updates the apps; a tray icon. |

| Crate | What it holds |
| --- | --- |
| `rivet-look` | The shared look: live desktop colours, fonts, icon themes, Rivet's own icons, code colours, the egui theme, in-app windows. |
| `rivet-kit` | Checks and screenshots: the software renderer that draws a window in memory. Built optimised even in debug builds, so put pixel-heavy code here. |
| `rivet-xdg` | Desktop plumbing with no interface: XDG folders, bookmarks, drives, file types, default apps, launching. |
| `rivet-media` | Previews: pictures, video frames, audio, text, thumbnails, the light box. |
| `rivet-term` | The terminal: a shell in a pseudo-terminal, drawn as an egui widget. |
| `rivet-connections` | Saved server connections, shared by Rivet FTP's window, its command line and plain `ssh`. |
| `rivet-git` | Git with no interface, by running the `git` program. |

Each app has its own `README.md`, `CHANGELOG.md` and `data/` (its `.desktop` file and icon).
Longer write-ups are in `docs/`. Rivet Shell's plugin API is in `apps/rivet-shell/PLUGINS.md`.

## How the pieces fit

- **One look.** Apps take colours, fonts and icons from `rivet-look`, never their own. When
  something should look the same in two apps, it belongs in `rivet-look`.
- **Colours come from the wallpaper.** Rivet Shell runs matugen on the picture chosen; every
  Rivet app watches the colour files and follows them live, with no restart.
- **The shell does not link the apps.** It reaches them as separate programs. An app joins the
  shell through a plugin: a folder with a `plugin.conf` whose program is the app itself
  (`rivet-notes shell-plugin`, `rivet-ftp shell-plugin`). The plugin speaks JSON lines; see
  PLUGINS.md before adding a capability, and add it to the API rather than special-casing one
  app.
- **Settings are plain text.** Each app keeps `key=value` lines in
  `~/.config/APP/settings`, hand-editable, with unknown keys skipped so a newer file still
  loads. Rivet Shell rereads its file when it changes; its Settings window is a separate
  process (`rivet-shell settings`) that saves on every change.
- **Rivet Shell is driven by messages.** `rivet-shell msg COMMAND` talks to the running shell
  over a socket (`launcher`, `lock`, `volume up`, `reload`…). Compositor key bindings use it.

## Building and checking

```sh
cargo build -p rivet-shell          # one app
cargo test -p rivet-shell           # its tests
cargo clippy --workspace --all-targets -- -D warnings   # what CI runs: warnings fail it
utilities/rivet-dev help            # releases, installs, screenshots, benchmarks
```

CI builds, tests and lints the whole workspace with `--locked`, so commit `Cargo.lock` with any
dependency change.

`cargo clippy` and `cargo check` do not rebuild the binary. Run `cargo build` before taking a
screenshot, or the picture shows the old code.

### Screenshots are the main way to see a change

Every app draws itself in memory and saves a PNG, without a display:

```sh
target/debug/rivet-shell --screenshot out.png             # the desktop
target/debug/rivet-shell settings --screenshot out.png    # the Settings window
target/debug/rivet-notes --screenshot out.png             # a sample vault
```

Environment variables set the scene: which popup is open, which page, a theme, a query. In
Rivet Shell they are `RIVET_SHELL_*` (`POPUP`, `PAGE`, `THEME`, `QUERY`, `GROUPS`, `PLUGIN`…),
read in `src/shot.rs` and `src/settings_app.rs`; the other apps have their own (`RIVET_NOTES_*`
and so on). When a feature can't be reached by an existing hook, add one beside the others
with a comment saying what it does.

Screenshot runs use sample data and never save settings. That makes them safe, and it also
means a screenshot proves how something draws, not that it works in a live session. Say which
of the two you checked.

## Conventions

**Code**
- Rust 2024, `rustfmt` with long lines; match the file you are in.
- Comments say what a thing is for or why it is so, in plain sentences, as the surrounding
  code does. No comments that restate the line.
- No new toolkit dependencies (GTK, Qt). Prefer a small amount of code over a large crate; a
  new dependency should earn its place, and goes in the crate that needs it.
- Work that takes time (reading folders, decoding pictures, running programs) happens on a
  thread and reports back; a frame never waits on it.
- A feature is general, not for one person or one machine: no hard-coded paths, compositors
  or hardware.

**Words in the interface**
- Plain, calm and direct. Say what a thing does, lead with the outcome, include the
  consequence. "No connections yet: add one in Rivet FTP", not "No items found".
- Sentence case for labels, title case for buttons and menu items as the apps already do; no
  exclamation marks, no "simply" or "just".
- One name per thing across the apps: vault (Notes), connection (FTP), panel, item, group,
  plugin, command (a launcher `@word`).

**Changelogs**
- Each app's `CHANGELOG.md` has a `## Unreleased` section at the top. Add a bullet there for
  anything a person would notice, written for that person: what they can now do, not what the
  code does. Check that the section exists before adding one; never make a second.
- Releasing (`utilities/rivet-dev bump` and `release`) turns that section into a version. Do
  not bump versions or tag by hand.

**Commits**
- One subject line: `App Name: what changed`, in plain words (`Rivet Shell: notifications get
  their own panel`). A short body when the why isn't obvious.
- Commit only the files of the change at hand. More than one session often works in this tree
  at once, on different apps: leave files you did not change out of the commit, and stage a
  shared file (such as `Cargo.lock`) only for your own lines.
- Do not push, tag or publish unless asked.

## Rivet Shell in particular

- `src/wl.rs` is the Wayland side: surfaces, the event loop, settings reloads, and `act()`,
  where every `Act` from the interface is carried out.
- `src/ui.rs` holds the model the drawing code reads, the `Act` and `PopupKind` enums, and
  `popup()`, which places and draws each popup. A new popup is a `PopupKind` variant, its
  drawing function, and its size in `popup()`.
- `src/bar.rs` draws a panel and its items; `src/panels.rs` is their configuration (zones,
  groups); `src/launcher.rs` the launcher; `src/quick.rs` quick settings.
- `src/config.rs` is the settings file: a field, its default, a line in the parser and a line
  in `text()`. The round-trip test there should cover a new field.
- `src/settings_app/` is the Settings window, one file per page. A new page also goes in
  `nav.rs` and `search.rs`.
- `src/plugins.rs` runs plugins and hooks; official plugins live in `data/plugins/`.
- The shell must keep working without optional programs (matugen, playerctl, a compositor it
  doesn't know): detect what is there and leave the rest out, without errors on screen.

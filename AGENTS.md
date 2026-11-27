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
| `apps/rivet-snip` | `rivet-snip` | Clips and marks up screenshots. A compositor hands it the picture as a file or on standard input; `rivet-snip capture` takes one itself anywhere else. Unreleased; built from source. |
| `apps/rivet-vice` | `rivet-vice` | The archive manager: opens an archive, shows what is in it, extracts all or some of it and makes new ones. Rivet Files' right-click menu opens an archive in it. Unreleased; built from source. |
| `apps/rivet-calendar` | `rivet-calendar` | A calendar kept in plain `.ics` folders (the vdir layout vdirsyncer and khal use): day, week, month and agenda views, an event editor, repeating events. Its shell plugin gives the panel calendar its events. Unreleased; built from source. |
| `apps/rivet-color` | `rivet-color` | The color picker: picks a color from the screen with its own loupe over a held-still screen (grim and layer-shell; else niri, hyprpicker, slurp or the portal) and copies it, with its shades, harmonies and contrast, a history and palettes kept as GIMP palette files. Its shell plugin puts the last color on the panel and `@color` in the launcher. Early alpha, released as pre-releases. |
| `apps/rivet-view` | `rivet-view` | The picture viewer: opens a picture and steps through its folder in Files' order, with zoom, turning (saved losslessly where it can be), slideshows, camera details and animations. Rivet Files hands pictures to it. Held back from release (`HOLD=1`); built from source. |
| `apps/rivet-fetch` | `rivetfetch` | A fastfetch-style summary of the computer in the terminal, beside the mascot drawn from his pixel art. No window, so it uses `rivet-xdg` but not `rivet-look` or `rivet-kit`. Unreleased; built from source. |

| Crate | What it holds |
| --- | --- |
| `rivet-look` | The shared look: live desktop colours, fonts, icon themes, Rivet's own icons, code colours, the egui theme, in-app windows. |
| `rivet-kit` | Checks and screenshots: the software renderer that draws a window in memory. Built optimised even in debug builds, so put pixel-heavy code here. Re-exports `rivet-mem` as `rivet_kit::mem`. |
| `rivet-mem` | Memory hygiene with no interface: the allocator's setup (call `alloc::setup()` first in `main`), handing freed memory back, caches kept to a size in bytes, file bytes read once per process, and how much memory a process uses. Numbers in `docs/memory.md`. |
| `rivet-xdg` | Desktop plumbing with no interface: XDG folders, bookmarks, drives, file types, default apps, launching, the trash, secrets in the keyring. |
| `rivet-media` | Previews: pictures, video frames, audio, text, thumbnails, the light box; full-size pictures with their frames and camera details (`picture`), and the stage that zooms and turns them (`stage`), shared by View and the light box. |
| `rivet-term` | The terminal: a shell in a pseudo-terminal, drawn as an egui widget. |
| `rivet-connections` | Saved server connections, shared by Rivet FTP's window, its command line and plain `ssh`. |
| `rivet-git` | Git with no interface, by running the `git` program. |
| `rivet-cal` | Calendars with no interface: the `.ics` folders, reading and writing iCalendar without losing what isn't used, repeating events, the occurrences in a span of days, subscribed feeds, and CalDAV, Google (CalDAV with a browser sign-in) and Microsoft (Graph) accounts synced both ways, all through `curl`. |
| `rivet-archive` | Archives with no interface, by running the system's archive tools: listing, extracting all or some, compressing. Used by Files and Vice. |

Each app has its own `README.md`, `CHANGELOG.md` and `data/` (its `.desktop` file and icon).
Longer write-ups are in `docs/`. Rivet Shell's plugin API is in `apps/rivet-shell/PLUGINS.md`.

## What the apps share, and how

Sharing happens in three ways. Know which one you are touching, because each has a different
blast radius: a crate change rebuilds every app that uses it, a file format change affects
programs already running, and a program-to-program change needs both ends updated.

### 1. Code, through the crates

No app depends on another app; they only depend on crates. Who uses what:

| Crate | Used by |
| --- | --- |
| `rivet-look`, `rivet-kit`, `rivet-xdg` | every app |
| `rivet-mem` | through `rivet-kit`, `rivet-look` and `rivet-media`; Fetch directly. `alloc::setup()`: Shell, Files, Terminal, View |
| `rivet-media` | Files, FTP, Shell, View |
| `rivet-term` | Terminal (and meant for Edit's panel) |
| `rivet-connections` | FTP |
| `rivet-git` | Edit (and meant for a Git app of its own) |
| `rivet-cal` | Calendar |

What "the same in every app" means in practice is a module of `rivet-look`:

- `live`: what every app does to look like the others: the desktop's font, the theme from a
  light or dark choice and an accent, and the desktop's colours followed live.
- `theme`, `palette`, `fonts`, `icons`: the egui theme, corner radii, status colours, the icon
  theme and Rivet's own icons. An app never picks a colour or radius of its own.
- `quick`: the quick panel (Ctrl+P: Quick Open, Quick Actions) and its fuzzy matching. Rivet
  Shell's launcher ranks with the same `fuzzy_score`.
- `windows`: windows inside the app (Preferences, About): title bar, drag, resize.
- `widgets`, `tabdrag`: small drawn controls, and dragging tabs to reorder them.
- `schemes`: colour schemes for code and terminals. Edit, Terminal and the Shell's palette
  picker all read the one list, which is how Terminal has Edit's themes.
- `highlight`: the light code colouring Files' Peek and other previews use.

Before writing something an app needs, look for it here. Before adding to one app something a
second app will want (a control, a dialog, a file-type rule, a way to launch things), put it in
the crate. A piece that is still written as part of one app's window needs lifting out first;
`docs/shared-crates.md` describes that work and what is still to move.

### 2. Files on disk

- **The desktop's colours.** GTK colour files (`@define-color name value;`), the kind matugen
  and other theme generators write. `rivet-look::palette` reads `~/.config/rivet/colors.css`
  first (a file just for Rivet, which matugen fills from the template
  `~/.config/matugen/templates/rivet-colors.css`), then the desktop's `gtk-4.0` and `gtk-3.0`
  colour files. Every app checks them every two seconds, so a new wallpaper recolours running
  apps with no restart, under Rivet Shell or any other desktop. Rivet Shell is one producer:
  it runs matugen on the wallpaper chosen. Changing a colour name means changing the reader
  and the template together.
- **Each app's own settings.** `key=value` lines, hand-editable, unknown keys skipped so a
  newer file still loads: `~/.config/rivet/settings` for Rivet Files (it is the `rivet`
  program), `~/.config/rivet-NAME/settings` for the rest. An app may read another's file only
  through code that owns the format: the Notes plugin reads Notes' settings because it is
  Notes.
- **Calendars.** `~/.local/share/rivet-calendar`, a folder per calendar with an `.ics` file per
  event and its name and colour in `displayname` and `color`, owned by `rivet-cal`. It's the
  layout vdirsyncer and khal use, so either can share the folders. A subscription's folder has
  `.rivet-feed`, a server calendar's (CalDAV, Google, Microsoft) `.rivet-caldav` (where it is on
  the server, and each file's state at the last sync); accounts are in
  `~/.config/rivet-calendar/accounts`, their passwords and sign-in tokens in the keyring. Rivet's
  own OAuth client IDs are compiled in from `RIVET_GOOGLE_CLIENT_ID`,
  `RIVET_GOOGLE_CLIENT_SECRET` and `RIVET_MICROSOFT_CLIENT_ID`, or from `keys.env` at the top
  of the workspace, which must never be committed (`docs/calendar-accounts.md`). The shell never reads any of it: Rivet Calendar's plugin sends the panel
  calendar its events.
- **Saved connections.** `~/.config/rivet-ftp/connections.conf`, owned by `rivet-connections`,
  read by Rivet FTP's window and command line, and written out as an ssh config so plain
  `ssh`, `scp`, `rsync` and `git` reach them by name. Passwords are never in it.
- **The desktop's own files**, through `rivet-xdg`: bookmarks (`gtk-3.0/bookmarks`), default
  apps (`mimeapps.list`), user folders, `.desktop` entries. Rivet reads and writes the standard
  ones, so other desktops' apps see the same bookmarks and defaults.

### 3. Programs talking to each other

- **The file picker.** Rivet Files can be the Open and Save window for every app on the
  desktop, as an xdg-desktop-portal backend (`apps/rivet-files/src/portal.rs`, `picker.rs`).
  Other Rivet apps ask the portal, not Rivet Files; Rivet Shell's folder and picture choosers
  do the same (`src/portal.rs`).
- **Shell plugins.** An app joins the shell through a plugin: a folder with a `plugin.conf`
  whose program is the app itself (`rivet-notes shell-plugin`, `rivet-ftp shell-plugin`, `rivet-edit shell-plugin`, `rivet-calendar shell-plugin`, `rivet-color shell-plugin`), kept
  running and speaking JSON lines. The shell shows what it says (a panel item, a menu, launcher
  entries under `@notes`, `@ftp` or `@edit`) and tells it what was chosen; the app then opens its own
  window (`rivet-notes FOLDER --note PATH`, `rivet-ftp open NAME`). Read PLUGINS.md before
  adding a capability, and add it to the API for every plugin rather than special-casing one
  app.
  Plugins from other people are listed in `plugins/registry.json`, which the shell
  (`src/plugin_store.rs`) and the website both read; a listing is added by pull request.
- **Messages to the shell.** `rivet-shell msg COMMAND` talks to the running shell over a
  socket (`launcher`, `lock`, `volume up`, `reload`…). Compositor key bindings use it, and so
  do plugins (`"run"`). The Settings window is a separate process (`rivet-shell settings`) that
  saves on every change and tells the shell.
- **Standard desktop services.** Notifications, the tray, media players and the lock screen
  use the freedesktop D-Bus interfaces, so Rivet Welcome's tray icon shows in any tray and any
  app's notifications show in Rivet Shell. Never invent a private channel where a standard one
  exists.
- **The dock** starts out with Files, Terminal, Edit and Notes pinned by their desktop ids,
  which is the only place the shell names the apps.

Every piece must still work alone: an app with no shell, the shell with none of the apps.
When one is missing, the feature that needs it is left out quietly.

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
- Load the `rivet-copy` skill (`.claude/skills/rivet-copy/`) before writing or changing any of
  these words. A description is one sentence; a longer explanation goes in
  `docs/guide-backlog.md`, which the user docs will be written from.

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

# Sharing Rivet's core with a second app

A plan for building Rivet FTP, a dual-pane file-transfer app (FTP, FTPS, SFTP), on the same
core as Rivet. It is based on how Rivet's ~23,000 lines depend on each other (November 2026).

Make it an original app with its own name, icon and look. Being in the same genre as
existing transfer clients is fine, but don't copy one of them.

## The short answer

About half of Rivet doesn't care that it's a file manager, and can move into shared crates
almost as is. The other half is the interface. It's worth sharing too, but each piece is
currently written as part of the Rivet window, so it needs a small refactor first.

| Proposed crate | What goes in it (today's modules) | Uses egui? | Effort |
|---|---|---|---|
| **rivet-xdg** | `places` (XDG folders, bookmarks, drives), `mime` (shared-mime-info), `launch` (default apps, Open With, terminal), `desktop` (.desktop files), `crash` (local crash reports) | no | small: these only use each other |
| **rivet-fs** | `fsops` (copy/move/trash with progress), `undo`, `archive`, `rename` (bulk-rename rules), `natural_cmp`, `fuzzy_score` and `folder_matches` | no | small |
| **rivet-look** | `palette` (live desktop colors), `fonts`, `icons` + `builtin_icons` + `nerd`, `highlight` (theme-colored code), plus the theme setup now in app.rs: visuals, corner radii (`r_xs`…`r_lg`, square mode), status colors, terminal-mode helpers, `paint_tex` | yes | medium: it reads `Settings` directly in a few places |
| **rivet-media** | `thumbs` (freedesktop thumbnail cache + thumbnailers), `preview`, `media` (ffmpeg check, video frames) | yes (textures) | small |
| **rivet-clip** | `wlclip` (Wayland clipboard and drag-out with uri-lists, cut/copy markers) | window handle only | small |
| **rivet-kit** | `headless` (in-memory screenshots for checks), the crash-report dialog, the key=value settings store (load/save, never write during checks), the background `watch` waker, the shortcut-window layout | yes | small–medium |
| **rivet-widgets** | stepped path bar + fuzzy suggestions, the three file views (icons/list/compact), sidebar rows and sections, toolbar buttons, status bar, tabs, split panes, Quick Open / Quick Actions palette, Peek panel, preview light box, dialog shell | yes | **large**: all of it is `impl RivetApp` today |

Stays in Rivet: the file picker and portal (`picker`, `portal`, `portal_setup`, `pickview`), gvfs
network browsing (`network`, `netview`), Zen mode, custom actions, `filters` and the menus.

New in the transfer app: the protocol clients, a site manager, password storage (Secret Service
over D-Bus; Rivet already uses `zbus`), the transfer queue, and the remote side of each view.

## The one design change that matters

The file views, Peek and the path bar all assume an entry is a local `PathBuf`. A
transfer client shows the same list for a remote folder. So before `rivet-widgets` can be
shared, two types need generalising:

```rust
/// Where an entry lives: a local path or a remote one.
enum Loc { Local(PathBuf), Remote { site: SiteId, path: String } }

/// Anything a file view can show and act on.
trait FileSource {
    fn list(&self, dir: &Loc) -> Result<Vec<Entry>, Error>;   // run off the UI thread
    fn stat(&self, loc: &Loc) -> Result<Entry, Error>;
    fn read_head(&self, loc: &Loc, bytes: usize) -> Result<Vec<u8>, Error>; // previews
    fn mkdir / rename / delete …
}
```

Rivet gets `LocalSource` (today's code). The transfer app adds `SftpSource` and `FtpSource`.
The views draw `Entry` values and send actions back, so they never need to know which
source they came from.

Copy/move with progress (`fsops::Progress`) becomes a small **job** type: queued,
cancellable, and reporting bytes and items. Local copies and uploads/downloads then share one
progress bar, and the transfer queue is just a list of jobs.

`model::Entry` currently depends on `settings` and `picker`. Passing in the two flags it
reads (show hidden, picker filter) removes both dependencies.

## Suggested order

1. ✅ **Workspace, no behavior change.** Rivet stays at the top of the repository, which is
   also the workspace root, so Flatpak, AUR, AppImage, CI and the release scripts work
   unchanged. Shared code goes in `crates/` and the other apps in `apps/`. CI builds, tests and
   lints the whole workspace.
2. Move the leaf crates:
   - ✅ **rivet-xdg** (`places`, `mime`, `launch`). Rivet uses it under the same names, so no
     other code changed.
   - ✅ **rivet-connections**, new: saved servers for Rivet FTP's window, its command line and
     `ssh`.
   - ✅ **rivet-kit**: the in-memory screenshot renderer, and a `Shot` helper that gives any
     app `--screenshot FILE.png`.
   - ✅ **rivet-media**: `preview` and `thumbs`, plus the light box (`lightbox`), used by both apps.
   - Next: rivet-fs, rivet-clip, and `crash` into rivet-xdg.
3. ✅ **rivet-look:** `palette`, `fonts`, `icons`, `builtin_icons`, `nerd` and `highlight`
   moved over unchanged. The theme moved too: `theme::apply` takes a small `Look` value (light,
   dark or system; accent; desktop palette; terminal mode) and also provides the corner radii and
   status colors. Rivet's screenshots are pixel-identical before and after the move.
4. **`Loc` + `FileSource` + jobs** in rivet-fs, with Rivet switched over to them.
5. **rivet-widgets:** lift the path bar, the file views and Peek out of `impl RivetApp`. Each
   widget takes the state it draws plus a `FileSource`, and returns actions.
6. ✅ (first version) **Rivet FTP's window:** the connection list, two panes (this computer |
   the server) and the transfer queue, built on rivet-look. Its panes are its own simple lists
   for now; once step 5 is done, they become Rivet's file views. See [Rivet FTP](rivet-ftp.md).

Steps 1–3 are low-risk and pay off even if the second app changes plans. Step 5 is the big
one, but it also makes Rivet's interface code easier to test.

## One repository: Rivet Apps

Decided November 2026: the apps share this repository, called Rivet Apps. Every app lives in
`apps/` (Rivet Files in `apps/rivet-files`), the top of the repository is only the Cargo
workspace, and its README is the front page for all of them. Plain `cargo build` still builds
Rivet Files (the workspace's default member), so its scripts and packaging only needed their
paths updated. Before that, Rivet Files sat at the top of the repository with a short section
for Rivet FTP in its README. Rivet FTP gets its own README in
`apps/rivet-ftp`, and the longer pages live in `docs/` as wiki pages. Releases are told apart by
tag (`v0.7.0` for Rivet as today, `rivet-ftp-v0.1.0` for Rivet FTP).

If Rivet FTP later needs its own GitHub page for marketing, the shared crates can move to a
third repository. Each app would then point at a tagged version of it, with nothing else
changing, because the crates already have their own manifests and no ties to either app.

The release tooling (`utilities/rivet-dev`: bump, AppImage, Flatpak, AUR, glibc check) and the
CI workflow can be reused with only the app name changed.

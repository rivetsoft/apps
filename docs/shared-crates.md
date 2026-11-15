# Sharing Rivet's core with a second app

A plan for building a dual-pane file-transfer client (FTP, FTPS, SFTP) on the same core as
Rivet. It is based on how Rivet's ~23,000 lines depend on each other today (November 2026).

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
| **rivet-look** | `palette` (live desktop colours), `fonts`, `icons` + `builtin_icons` + `nerd`, `highlight` (theme-coloured code), plus the theme setup now in app.rs: visuals, corner radii (`r_xs`…`r_lg`, square mode), status colours, terminal-mode helpers, `paint_tex` | yes | medium: it reads `Settings` directly in a few places |
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

1. **Workspace, no behaviour change.** Turn `app/` into a Cargo workspace (`crates/…` and
   `apps/rivet`), keeping one repository, one Cargo.lock and one CI. Rivet must look and work
   the same: `rivet-dev qa` (0 crashes, 8/8 workflows) and the screenshot checks prove it.
2. **Move the leaf crates:** rivet-xdg, rivet-fs, rivet-media and rivet-clip. These are mostly
   file moves.
3. **rivet-look:** replace the direct `Settings` reads with a small `Look` value (accent, the
   three palette colours, square corners, terminal mode, fonts) that each app fills in.
4. **`Loc` + `FileSource` + jobs** in rivet-fs, with Rivet switched over to them.
5. **rivet-widgets:** lift the path bar, the file views and Peek out of `impl RivetApp`. Each
   widget takes the state it draws plus a `FileSource`, and returns actions.
6. **Start the transfer app:** two panes (local | remote, based on Rivet's split view), a site
   manager, and the queue.

Steps 1–3 are low-risk and pay off even if the second app changes plans. Step 5 is the big
one, but it also makes Rivet's interface code easier to test.

## Keep one repository for now

Keep both apps in one workspace until the shared APIs stop changing. A change to a shared crate
then updates both apps in a single commit and one CI run. Separate repositories, or publishing
the crates on crates.io, can come later.

The release tooling (`utilities/rivet-dev`: bump, AppImage, Flatpak, AUR, glibc check) and the
CI workflow can be reused with only the app name changed.

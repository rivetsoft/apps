# Changelog

## Unreleased

- **Big folders sort faster**: sorting 100,000 files by name or type takes about a tenth of the time it did when they were made in no particular order, as in most real folders, so changing the sort or typing in the filter keeps up.
- **Smoother with many files selected**: right-clicking a large selection no longer slows the window down, and Peek reads an item's owner, group and dates once instead of on every frame, so it stays quick on network folders.

## 0.10.0

- `./install.sh --uninstall` in the downloaded archive takes Rivet Files back off: the program in `~/.local/bin`, its app launcher entry and its icons.
- **A new icon**: the pink folder with the rivet through it, drawn in the website's colors with a plum outline and a hard shadow.
- **Open in Rivet Vice**: right-click an archive to look inside it before taking anything out. Shown when Rivet Vice, the archive manager, is installed.
- On Arch Linux under X11, the message about a missing keyboard library names the right package to install, `libxkbcommon-x11`, instead of `libxkbcommon`, which doesn't include it on Arch.
- The Empty Trash question is roomier and easier to read: the Trash's icon beside it, how many items are in it, and the buttons on the right, as in the Delete question.
- `rivet trash:///` opens straight on the Trash, as other file managers do, so a dock or launcher can open it there.
- **Settings changed in one window stay changed**: with several windows open, closing one no
  longer puts back the toolbar, view and other settings as they were when it opened. Each
  window saves only what was changed in it.
- **Return in a question presses the highlighted button**: Empty Trash, a message's OK and
  Replace in the file picker answer to Return, as Delete already did. Tab to Cancel and Return
  cancels; before, Return in the Delete and Open With questions went ahead even with Cancel
  focused.
- **Long names read better in the icon view**: they wrap between words and at hyphens instead
  of mid-word, a word is broken only when it is wider than the label on its own, and a name
  longer than two lines ends in an ellipsis. Select it to see the whole name, or hover for it.
- **Peek fits its panel**: a text preview wraps to the panel's width instead of running off its
  right edge, and the list view's columns narrow to make room for Peek, ending in an ellipsis
  (Type, Location and Size are left out when there is no room) rather than running past the
  edge of the window.
- The AUR packages also install libxkbcommon-x11, which Rivet Files needs to start under X11.

## 0.9.0

- **Crash reports leave out personal details**: your user and computer names, file paths and
  server addresses are taken out before the report is saved. Report on GitHub now opens an
  issue with the report already filled in, for you to check and send.
- A new icon in Rivet's pink, to match the rest of the apps and the website.
- **Smoother in big folders**: selecting files no longer slows the window down. In a folder of
  100,000 files, choosing one file used to make every frame 15 to 30 times slower, and selecting
  everything (or having Peek open) more than that; now it stays as quick as with nothing selected.
- **Column view keeps up**: the other columns are read in the background, so moving through
  large folders no longer pauses the window.
- **Opens sooner**: about 30 ms quicker to the first window (more with a large matugen setup).
- The column view's button shows the column view with any icon theme, not a theme's row of bars.

## 0.8.0

- Tabs look and work like Rivet Edit's and Rivet Terminal's (one shared strip): drag a tab to
  move it, with a glow, and when there are too many to fit they scroll, with
  < and > at the end. Files dropped on a tab still go to its folder.
- The current place and folder in the sidebar are marked the way the other apps mark a chosen
  item: a soft tile with an accent edge, the icon in its own colors.
- The disk usage bar under a drive is the theme's tertiary color (red when nearly full).
- **Column view** (Ctrl+4, View → Columns, or the toolbar): Miller columns. The folders above this
  one sit side by side (from Home, or from / outside it), and the column on the right shows what
  the chosen folder holds, or Peek for a chosen file when the Peek panel is closed. Click in
  any column to go there; ← and → (or Vim's h / l) move between columns.
- **Shortcuts**: Ctrl+N makes a new folder, Ctrl+Shift+N opens a new window (File → New Window).

## 0.7.0

### New
- **Thumbnails beyond pictures**: videos, PDFs, fonts, office files, e-books and anything else
  the system has a thumbnailer for (`/usr/share/thumbnailers`), and the shared
  `~/.cache/thumbnails` cache that other file managers use — Rivet reads it first and adds what it
  makes, following the freedesktop thumbnail rules (failures are remembered too).
- **Custom actions**: your own commands in the right-click menu. Each is a small `.action` file
  in `~/.local/share/rivet/actions`: a name, an icon, the command (`%F` the selected files, `%d`
  the folder…), when it shows (one item, several, empty space) and for which files (folders,
  types like `image/*`, endings like `.png`), with optional submenus, a terminal window, and
  once-per-item runs. Preferences → Modules counts them, opens the folder and can add an example
  to start from.
- **Split view** (F3, View → Split View, or Quick Actions): two folders side by side, each with
  its own history and a header naming its folder. Click a pane (or F6) to work in it — the
  toolbar, path bar, keyboard and Peek follow — drag files between them, or use Copy / Move to
  Other Pane from the right-click menu.
- **Copy and paste with other apps**: Copy or Cut in Rivet and paste in Dolphin, GNOME Files,
  Nemo or a terminal; copy files there and paste them into Rivet (a cut there moves them). Rivet
  offers files the standard ways (`text/uri-list`, `x-special/gnome-copied-files`, KDE's cut flag
  and plain paths), and only pastes its own copy while it's still the newest one. On X11 files go
  across as paths.
- **Drag files out to other apps** (Wayland): drag files out of the window into a browser upload,
  a chat, an editor or another file manager.

### Fixed
- A background job that fails (a copy, a thumbnail, a search) can no longer take the window down
  with it.
- Dragging the zoom slider no longer keeps a copy of every icon at every size in memory, and
  thumbnails beyond the last 800 shown are let go (they come back quickly from the disk cache).
- When a file changes, only its own thumbnail is redone (the old one stays until then), instead of
  every thumbnail in the folder.
- In split view, the other pane now updates when files change on disk too.
- Copy Location (and every other text copy: crash reports, the terminal link command) reaches
  other apps again, and pasting text into Rivet's fields from other apps works.
- Bold text uses the font's real bold face when the desktop font can't be read (it fell back to
  the regular one).

### Changed
- Rivet Files now lives in the [Rivet Apps](https://github.com/rivetsoft/apps) repository,
  beside Rivet FTP and Rivet Notes; its downloads, Flatpak id and AUR packages are unchanged.

## 0.6.0

### New
- **File picker**: Rivet can be the Open and Save window every app shows, through the desktop
  portal (xdg-desktop-portal). Open one or several files, pick a folder, save (the name is
  selected up to its extension), or save several files into a folder; filter by the app's file
  types; confirm before replacing. The window is a picker, not the file manager: the title and
  the app asking at the top, a short toolbar, the list view, and one strip at the bottom with the
  name, the file types, Cancel and Open / Save — nothing in it moves, deletes or changes files.
  Turn it on in Preferences → System → File picker or with `rivet --file-picker on`: Rivet goes in
  front of the pickers you had (kept as fallbacks) in `<desktop>-portals.conf`, and turning it off
  restores that line exactly (and only ever removes Rivet from a list it didn't set up). Not
  available inside Flatpak. (Its D-Bus support adds about 2.5 MB, 1 MB to the AppImage.)
- **Crash reports**: if Rivet stops because of a bug, it saves a plain-text report on this computer
  (`~/.local/state/rivet/crashes`) — version, what went wrong and where, the stack, and the system.
  Nothing is sent anywhere. The next start shows it once, with Copy Report, Open Folder and Report
  on GitHub (copies the report and opens a new issue to paste it into). Preferences → System lists
  the saved reports.

### Improved
- **Starts about 0.2 s faster** (back under 0.4 s for 100,000 files): checking whether ffmpeg is
  installed no longer holds up the window, and the monospace font loads in the background
  (terminal mode still gets it from the first frame). Preferences → Modules no longer pauses the
  window every few seconds while it looks for ffmpeg.
- **New Folder / New Document** redone: the folder (or file type) icon, where it goes, a raised
  name field outlined in the accent, and a live check under it — taken names and “/” are caught
  as you type (Create waits until the name works), and a leading dot is flagged as hidden. In
  the file picker, a new folder opens right away.
- The code passes Rust's linter (clippy) cleanly, and CI now fails on any new clippy warning.
- The Flatpak's bundled sources include the new D-Bus libraries, and the app metadata lists every
  release (`rivet-dev bump` now adds each new one).

## 0.5.0

### New
- **Zen mode** (F12, or View → Zen Mode): the files fill the window, with an empty border round
  them. Move the pointer into the border on the toolbar's, sidebar's or Peek's side and that panel
  slides in over the files; move away and it slides back out. Peek takes the other side if it
  shares one with the sidebar, and the menus live behind the toolbar's menu button.
- **Quick Open** (Ctrl+P, or View → Quick Open): a jump-anywhere bar. Type a few letters of any
  folder — recent ones, places, bookmarks, drives, folders here and under Home — and press Enter.
  Start with `/` or `~` to walk a path; Tab fills in the highlighted folder.
- **Quick Actions** (Ctrl+Shift+P, or View → Quick Actions): every menu command and view option in
  one searchable list — views, panels, sorting, layout, theme, going places, file commands. On /
  off options show their state and keep the list open, so you can flip several in a row; pick-one
  choices list only what you can switch to.
- **Terminal mode** (F7, Preferences → Appearance → Mode, View → Terminal Mode, or Quick Actions):
  a GUI that feels like a TUI. Everything is monospace, icons are Nerd Font glyphs colored by kind
  (drawn from any installed Nerd Font, your monospace font first), corners are square, shadows are
  gone and the sidebar, files and Peek sit in plain boxes with their titles in the border. Picture
  thumbnails give way to glyphs in the file list (Peek and Preview still show pictures). Without a
  Nerd Font the icons stay as they are.
- **Monospace Font** (Preferences → Appearance → Typography): used for text and code previews, and
  for everything in terminal mode. Type to find a font, like the interface font.
- **Vim keys** (an option, off unless you turn it on in Preferences → Behavior): `h` `j` `k` `l`
  move (in the list view `h` goes to the parent folder and `l` opens), `-` parent folder and `o`
  open in any view, `gg` `G` first / last item, counts (`5j`, `10G`), `V` visual mode (movement
  extends the selection; `y` / `d` act on it), `yy` copy, `dd` or `x` cut, `p` paste, `cw` rename,
  `D` move to Trash, `.` or `zh` hidden files, `H` `L` back / forward, `/` search, `:` Quick
  Actions and `~` Home. The status bar shows `-- VISUAL --` and any half-typed command. Typing
  letters no longer filters the folder while it's on (`/` does).

### Improved
- **Keyboard shortcuts** (F1) redone: keys drawn as key caps, cards in two columns, a search field
  ("rename", "ctrl d") and the missing shortcuts added. With Vim keys on they get their own card
  at the top.
- **View menu** tidied: the on / off items sit together, then Filters, Quick Open and Quick Actions.
- Sidebar drive usage is read every few seconds instead of on every redraw, so a slow or sleeping
  drive can't hold up the window.
- Preferences no longer looks fonts up again on every redraw (it did when fontconfig's tools were
  missing).

## 0.4.0

### New
- **Filters**: show a folder with only certain items — by type (folders, pictures, videos, audio,
  documents, archives, code & text, other), by name (contains / excludes / includes patterns like
  `*.jpg, IMG_*`) and by date modified. A filter is Global, for This folder, or for This folder &
  below; only one applies, and a folder's own filter comes first, then one set on a folder above
  it, then Global. A strip above the files says when one is on and how many items it hides. The
  Filters panel drops down over the files (strip funnel, a Filters toolbar button, View → Filters…
  or Ctrl+Shift+F), so the sidebar stays free for getting around. Saved in `~/.config/rivet/filters`.
- **Bulk Rename**: select several items and press F2 (or right-click → Bulk Rename…). Insert /
  overwrite text, date / time, numbering, remove characters, search & replace, upper / lower /
  title / sentence case — on the name, the extension or both, with a live preview that flags
  clashes before anything changes. One Ctrl+Z undoes the whole batch.
- **Convert & Resize** (uses ffmpeg): right-click → Convert To… for pictures (JPEG, PNG, WebP, GIF,
  BMP, TIFF), videos (MP4, WebM, MKV, GIF, MP3) and audio (MP3, Ogg, FLAC, WAV, M4A), one file or
  many, with a quality setting; keep the originals or replace them (originals go to the Trash).
  Right-click → Resize… for pictures: by percentage or to fit a box, as new copies or overwriting.
  Runs in the background.
- **Modules** page in Preferences: Bulk Rename and Convert & Resize each have an on/off switch.
  Convert & Resize switches itself off while ffmpeg isn't installed, and notices when it is.

### Improved
- README: features grouped, a Tools gallery (Filters, Bulk Rename).

### Fixed
- Arrows in the Bulk Rename and Resize previews are drawn, so they show with every font.

### Developer
- All developer scripts live in `utilities/` behind one command, `utilities/rivet-dev <command>`:
  `bump`, `release [--publish]`, `push`, `appimage`, `portable`, `install`, `link`, `screenshots`,
  `bench`… (`help` lists them). `push` and `release --publish` take in GitHub's commits first.

## 0.3.0

### New
- **Toolbar and status bar positions** (Preferences → Appearance → Window): the toolbar at the top
  or bottom, the status bar off, top or bottom. On the same edge they stack.
- **Preview**: a full-window light box for pictures, videos (muted playback), music covers and
  text. Right-click → Preview or press Space; ← / → (or the side arrows) step through the folder, Return opens the file,
  Esc, Space or a click outside closes it. Its bottom split button opens with the default app
  (named, with its icon) or picks another one.
- **Preferences** has a sidebar of pages: Appearance (with Window), Views, Behavior, Menu (with the
  toolbar), System and About. Appearance is split into roomy groups: desktop colors, style and accent,
  icons, typography, window. Help → About opens the About page.
- **Theme-tinted action colors**: info, danger, warning and success are 70% their usual hue and
  30% your theme accent. Delete Permanently, close-button hovers and errors use danger; Rename uses success.
- **Folder suggestions** while typing a path: a fuzzy-matched list of sub-folders under the
  location bar; Tab completes the highlighted one, Enter completes it and goes there, ↑/↓ pick.
- **Location bar**: drag across it to select the path as text; right-click it (or any folder in it)
  for Copy Location, Open in New Tab and Edit Location.

### Improved
- **Keyboard focus**: Tab now shows a clear accent ring on whatever has focus (menus, toolbar,
  sidebar, status bar, file list). The file list is a single Tab stop — arrows, Enter and Space
  work on the files from there — instead of every file being one.
- **Keyboard**: Ctrl+, opens Preferences with the keyboard; ↑/↓ move between its pages, Home/End
  jump, Esc closes, and the file view no longer reacts to those keys while Preferences has focus.
- **Light mode contrast**: file icons and thumbnails get a soft shadow on light backgrounds, Rivet's
  own page icons are tinted with a thin edge, and secondary text, panels and controls are stronger.
- **Properties** redesigned: big icon and name field, a Basic / Permissions switch, and roomy
  General, Dates, Ownership and Access cards.
- **F8** shows or hides the status bar.
- Esc in a Preferences text field or list only leaves it.
- README: minimal vs bloated screenshots.
- **Font picker**: type in the font list to narrow it to the closest matches; Enter picks the best one.
- Clearer System page (name · status · action rows) and no filler text in Preferences.
- Roomier Rename window.
- The status bar no longer repeats the free disk space (the sidebar's drive bars show it).
- Peek's split *Open with* button: full-height divider and an apps icon for choosing another app.
- README: Rivet's goals and a short roadmap.
- Code tidy-up: no compiler warnings, unused code removed.

### Fixed
- The `rivet` terminal command (Preferences → System → Link) could point at another AppImage app
  when Rivet was started from one; Rivet now only treats itself as an AppImage when it runs from it.

### Removed
- The niri-only code that moved check / screenshot windows to another workspace.

## 0.2.0

### New
- **Peek panel** (F11, View menu, status bar): a big preview of the selection with all its details.
  Pictures, videos that play right in the panel (muted, with seek), music covers, and text files
  with syntax highlighting in your theme's colors. Drag the grip to resize the preview.
  A split "Open with …" button (the arrow picks another app), plus Copy location, Rename, Trash and Properties.
- **Network** that works: finds servers on your network, lists SMB shares, connects (with a login
  window or as guest), disconnects, and *Connect to Server…* for smb://, sftp://, ftp://, dav:// addresses.
- **Compress and Extract Here**: zip, tar.gz, tar.xz, tar.zst and 7z, in the background, with undo.
- **Undo and redo** (Ctrl+Z / Ctrl+Shift+Z) for copies, moves, renames, Trash and new items.
- **Sidebar and Peek on either side** (Preferences → Window); the status bar buttons follow them.

### Improved
- Open With window redesigned; it always lists the current default app.
- Roomier right-click menus.
- Peek layout: small action buttons first, a full-width split *Open with …* button below (files only).
- Tidier menus with consistent shortcut labels; roomier hamburger menu; wider menu bar hover.
- Title bar setting reads *System* / *App*.
- Check and screenshot windows stay out of the way on niri.
- Release builds run on older glibc; Flatpak and AUR packaging.

### Removed
- Alt no longer peeks at a hidden menu bar (Ctrl+M shows it).

## 0.1.2

- bump.sh: one command to set the next version
- Delete dialog: roomier padding
- Clearer delete confirmation; the Delete key asks
- Highlighted sidebar rows recolor the same icon
- Appearance: grouped spacing, US spelling, drop auto-update hint
- Toolbar settings: trash-can button instead of 'Remove'
- Title bar setting: only in Preferences, clearer wording
- Toolbar settings: up / down chevron buttons instead of text
- Sidebar icon size: Off, Small, Medium, Large
- Highlighted rows show the icon as a solid recolored silhouette
- Active view / sidebar-mode buttons use the selected-file look; status bar padding
- Grid view: one tile style for hover and selection; names stay visible
- Faster big folders, zero idle CPU, 100k-file benchmark, mouse back/forward
- README: cleaner layout with concise emoji feature list
- Pop-up windows: close button only (no roll-up / maximize)
- Describe Rivet as a classic Linux file manager like Nemo, Thunar and Caja
- CI workflow and README badges
- README: logo above the title, not floated right
- Logo assets in the codebase
- Separate Keyboard Shortcuts and About; new purple folder + rivet icon
- Polish: one radius scale, even location pills, flat status bar, readable icons on highlights

## 0.1.1

- **AppImage**: Rivet now also ships as a single-file AppImage — download, `chmod +x`, run.
  "Add Rivet to the app launcher" from inside it points your menus at the AppImage.
- New app icon (window, launcher entry and AppImage).
- The archive's installer also installs the icon.

## 0.1.0 — first release

The first public version of Rivet Files: a Nemo-style file manager for Linux, written in Rust
with egui. No GTK or Qt.

### Browsing
- Icon, list and compact views; tabs; back / forward / up; breadcrumb location bar
  (click it to type a path) with Ctrl+L
- Sidebar with My Computer, Bookmarks (shared with other file managers), Devices with
  usage bars and eject, Network, and a folder-tree mode
- Filter-as-you-type and recursive search, picture thumbnails, hidden files toggle,
  sorting by name / size / type / date

### Files
- Copy, cut, paste, drag and drop, rename, new folder / document (from Templates)
- Trash following the desktop standard, with restore and empty
- Open with the default app, Open With…, Open in Terminal, Properties with permissions

### Looks
- Uses your system icon theme, or Rivet's own SVG icons recolored from your palette
  (primary / secondary / tertiary)
- Follows matugen, Noctalia and GTK colors live — no restart
- Surface elevations instead of borders, roomy toolbar and sidebar
- UI font and text size options
- Optional window decorations (system title bar or a built-in one)
- Customizable toolbar; hideable menu bar (Ctrl+M, Alt to peek, hamburger button)
- Preferences and Properties as movable, resizable windows inside the app

### System
- Desktop entry, "make default file manager", and a global `rivet` command
- `rivet --bench` for timings; ~0.2 s to first frame, ~1 ms per frame

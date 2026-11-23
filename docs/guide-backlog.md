# User guide backlog

What the user docs will be written from. When a description in an app is cut down to one
sentence, the full explanation goes here, under the app and the page, with the place it came from.
Once the docs exist, each entry becomes a page or section, and the app gets a "Learn more" link to
it.

## Rivet Shell

### Plugins: hooks

From `settings_app/plugins.rs`, Hooks section.

A hook is a command that runs when something happens in the shell. The command receives
`RIVET_EVENT` (the event's name) and, for some events, more details as `RIVET_` variables:

| Event | Details |
| --- | --- |
| An idle step was reached | `RIVET_STAGE`: dim, lock, screen-off, suspend |
| Someone came back after idle | `RIVET_STAGE` |
| The wallpaper changed | `RIVET_PATH` |
| The style switched between dark and light | `RIVET_MODE`: dark, light |
| A notification arrived | `RIVET_APP`, `RIVET_SUMMARY`, `RIVET_BODY`, `RIVET_URGENCY` |
| Do Not Disturb turned on or off | `RIVET_ON`: true, false |
| The active workspace changed | `RIVET_KEY`, `RIVET_NAME` |
| What's playing changed | `RIVET_TITLE`, `RIVET_ARTIST`, `RIVET_PLAYER`, `RIVET_PLAYING` |

### Plugins: writing your own

From `settings_app/plugins.rs`, the removed "Write a plugin" section. `apps/rivet-shell/PLUGINS.md`
is the full reference.

A plugin is a folder with a `plugin.conf` (name, exec, mode, interval, events) and a program in
any language. The program prints what its panel item shows (plain text, or JSON with text, an
icon, a menu and launcher entries), and it hears about clicks and the hook events. It can also add
settings and a launcher command (`@name`). Disk Space, Notes and FTP come with the shell as
examples to copy.

### Plugins: getting more

From `settings_app/plugins.rs`, the Get Plugins window ("Other people make third-party plugins. A
plugin can do anything you can, so turn on only ones you trust.").

The list is `plugins/registry.json` in Rivet's repository. A plugin is accepted into it once; its
author then publishes updates themselves, in a release file the listing points to, which names the
current version, its archive and the archive's checksum. The shell refuses a download that doesn't
match that checksum, but nobody at Rivet reads an update before it reaches people, which is why
third-party plugins carry the warning. Installing copies the plugin into
`~/.local/share/rivet-shell/plugins/`; it runs only once it's turned on. Remove deletes that folder
and keeps the plugin's settings. View Source opens the plugin's own page. A plugin that isn't in
the list can be installed from a terminal (`rivet-shell plugin install FILE`), with no checksum
to check it against. `apps/rivet-shell/PLUGINS.md` has how to share one.

### Panels: zones

From `settings_app/panel_editor.rs`.

A panel has three zones. On a top or bottom panel, Left starts at the panel's left end, Center sits
in the middle and Right ends at the right end. On a side panel the zones are Top, Middle and Bottom.
On a side panel, numbers sit under their symbols, the clock wraps onto lines and the window title
runs downward. When items don't fit, the ones nearest the middle are hidden first.

### Panels: sub panels

From `settings_app/panel_editor.rs` (Add Sub Panel, and a sub panel's own card), `panels.rs` and
`rivet-shell msg subpanel`.

A sub panel is a panel that stays rolled up under another panel, its host, until it's asked for.
It holds items like any panel (zones, groups, its own look and size) and takes its edge and
screen from its host. Add one with Add Sub Panel on a panel's page; it gets a button on that
panel, which can be moved or removed like any item, and added to other panels from Add Item.

Three ways to open one: its button; a key bound in the compositor to
`rivet-shell msg subpanel NAME` (add `open`, `close` or `stay` to do only that; with no name, the
first sub panel); and pausing the pointer for a moment just past the host (below a top panel, above a bottom one), which can be
turned off. The place to pause runs the length of the host's bar: from the last few points of the bar, on the
side the sub panel rolls out from, to a little past the host, where a small handle shows while
the sub panel is rolled up. Moving straight through on the way to or from the panel doesn't
open it. The handle has a section of its own under Look. It can be hidden; drawn as a bar, a
line, an arrow (pointing the way the sub panel rolls out) or dots; made 12 to 400 points long
and 2 to 10 thick; placed at either end of the panel or its middle; set from 8 points over the
panel's edge to 40 points away from it; shown in the panel's own color (outlined) or the
primary, secondary or tertiary color; and made see-through, down to 15%. The place that takes
the pointer grows to take in the handle wherever it's put.

"Rolls Back Up": it stays while the pointer is on it or one of its popups is open, and for the
time set (4 seconds to start with, 1 to 30) after the pointer leaves or after it was opened
without being visited. It draws over windows. Opening one closes the host's other sub panels
that roll up. "Stays Open": it's opened and closed only by its button or key, stays across
restarts, and keeps its room from windows the way its host does. Right-clicking the button
switches between the two; one switched back to rolling up waits its usual time first.

"Join it to the panel, like a drawer pulled out of it" (on to start with): the sub panel's bar
begins where its panel's bar ends, with no room between them, its corners on that side are
square, and it fills in under the panel's rounded corners, so the two read as one shape. Only
the sub panel right under the panel joins it; one stacked after another that's left open sits
below as a bar of its own. "Match the panel's length and place" (always so for a drawer) makes
the sub panel as long as its panel and in line with it, whatever its own length says.

"Attached to" moves a sub panel to another panel. When its host is removed the sub panel is
kept, attached to nothing and not shown, until it's given another. A sub panel under a panel
that hides brings that panel out with it. With several screens, a key opens the sub panel on
each screen it's on; its button, only on that screen.

### Weather: where it comes from, and what leaves the computer

(`weather.rs`, `settings_app/weather_opts.rs`: "Open-Meteo is asked for the weather at this place…")

The weather is one reading for one place, shown by the desktop widget (Widgets › "The weather for
a place you choose") and by the Weather panel item, whose popup has the next hours and the week.
Both share the place and the units, set under Widgets › Weather or from the item's gear.

Choosing a place: type a town's name and press Find, then pick from the places found. The name
goes to Open-Meteo's place search; the shell keeps the place's name, latitude and longitude in
its settings file. Nothing finds the computer's location by itself.

While the widget or the item is on and a place is chosen, the shell asks Open-Meteo for that
place's forecast every 15 minutes, and again after a minute when a try fails. With both off,
nothing is asked. It needs curl, and no account or key. The Refresh button in the popup reads
again at once. `rivet-shell msg weather` opens the popup from a key.

Readings are kept in metric and shown in the units chosen, so changing units is instant. The
units start as °F and mph where the computer's region uses them (United States, Liberia,
Myanmar), else °C and km/h.

For anyone adding a widget: `weather_ui` draws the weather from parts (`Part::Place`, `Now`,
`Details`, `Hours(n)`, `Days(n)`). `view` stacks the parts asked for into a rectangle,
`height` says how tall they are, and `icon` draws a sky at any size. The desktop card and the
popup are two choices of parts over the same reading (`Extra::weather`).

### Displays and window look: how Rivet Shell changes the compositor's settings

From `settings_app/displays.rs` and `settings_app/window_look.rs`.

Rivet Shell keeps its screen and window settings in a file of its own next to the compositor's
config (`rivet-shell.kdl` for niri, `rivet-shell.conf` or `rivet-shell.lua` for Hyprland,
`rivet-shell.conf` for mango). The compositor's config needs one include line at the end so these
settings take effect; Settings adds it on request and first saves the original as
`NAME.rivet-backup`. Niri needs version 25.11 or later. "Leave the Screens to …" removes the
screens from Rivet Shell's file so the compositor's own config sets them again.

### Browser colors

Location: `apps/rivet-shell/src/settings_app/browser_colors.rs`, Appearance, "Browser colors";
the work is `apps/rivet-shell/src/browser.rs`.

- Chromium, Chrome, Brave (and Brave Origin) and Edge take one color from the theme and make
  their whole look from it, with light or dark to match. Tabs, toolbar and menus stay the
  browser's own; extensions, sign-ins and protected video are untouched.
- Color: Accent or Surface. The browser derives its tones from that one color, so single parts
  can't be set.
- It follows the theme: the shell writes the color again when the wallpaper's colors change
  and tells a running browser to read it, so open windows recolor with no restart.
- How: these browsers read "managed policies" from a folder under `/etc`
  (`/etc/chromium/policies/managed`, `/etc/opt/chrome/…`, `/etc/brave/…`, `/etc/opt/edge/…`).
  Rivet keeps one file there, `rivet-color.json`, with `BrowserThemeColor` and
  `BrowserColorScheme`. The folders are root's, so Allow asks for the password once to make
  that file and give it to the person; afterwards nothing asks again.
- Side effect: the browser's menu and `chrome://management` say it is managed by your
  organization. That is this file and nothing else.
- Turning it off empties the file (the folder is root's, so Rivet can't delete it). To remove
  it fully: `sudo rm /etc/*/policies/managed/rivet-color.json /etc/opt/*/policies/managed/rivet-color.json`.
- Not covered: Flatpak and Snap browsers (they don't read `/etc` policies the same way),
  Vivaldi, Opera and Firefox.

### Softcore: its settings in the Settings window

Location: `apps/rivet-shell/src/settings_app/softcore_page.rs`, the Softcore page.

The page is only in the sidebar and in search when Softcore is the compositor; on niri,
Hyprland, Sway and mango nothing of Softcore's is looked for or run. The guide should cover:

- The cards start closed, each saying how many settings it holds and how many you have
  changed. The names along the top open a card and go to it; Open All opens every one.
- The controls are Softcore's own list of settings, so a newer Softcore's settings appear
  without a newer Rivet Shell. Their labels and notes are Softcore's words.
- A change is one line of `~/.config/softcore/settings`, changed by `softcore settings set`.
  Comments, order and anything typed by hand stay. Editing the file by hand and using the
  window can be mixed freely: the window reads the file again when it changes, and Softcore
  takes a change up by itself.
- Reset takes the line out of the file, which puts the setting back to Softcore's own value.
  "Follows" means nothing is set and the value follows something else (the edges follow the
  gaps, colors follow Rivet); Set gives it a value of its own.
- While Window Look is on (Appearance), gaps, border width, corners, unfocused opacity and
  shadows are written by it and are left off the Softcore page.
- Window borders: the focused window's border and the other windows' border are each one
  row. One Color is a plain border; Two Colors is a gradient from the first color to the
  second, along Border gradient angle. Each color is Automatic (first color only: the accent
  for the focused window, the theme's outline for the others, following the theme as it
  changes), Accent, Secondary or Tertiary from the theme, or Custom. A color picked from the
  theme follows it: Rivet Shell's settings keep the name (`softcore_color.border_active=accent`)
  and the shell writes the color itself into `rivet-shell.settings`, the file Softcore's
  settings include, again each time the theme's colors change (a new wallpaper). Picking one
  takes Softcore's own line for that color out of its settings file, since that line would
  stand over the included one; Custom puts a line back. Without the include line in
  Softcore's settings a theme color can't follow: it is written once, as the color it is
  then, and the page says so. Under Softcore, Window Look (Appearance) leaves border colors
  alone and points here.
- Key bindings: the keys as Softcore names them (`Mod+Shift+T`) and an action (`spawn
  rivet-terminal`, `screenshot`, `title="Launcher" spawn …`). Softcore refuses keys or actions
  it doesn't know, and the page says why. A binding with no action takes away one of
  Softcore's own. Softcore's own bindings aren't listed.
- Window rules, layer rules, named workspaces, scratchpads, environment lines and startup
  programs are not on the page yet: edit them in the file.

### Login screen (greetd)

From `greeter.rs` and `rivet-shell greeter --setup`.

The login screen is the lock screen, run by greetd. greetd runs it as its own user, which can't
read anyone's home folder, so each person's Rivet Shell leaves a copy of their wallpaper, their
colors and clock, their name, their picture (`~/.face`) and their session in
`/var/lib/rivet-greeter/LOGIN`, whenever the wallpaper or its colors change. The login screen
shows whoever changed theirs last; Tab (or the arrow keys) goes to the next account. The folder
works like `/tmp`: anyone can add their own folder, nobody can change another's, and the login
screen ignores a folder its owner didn't make. The session started is the one the person was
using (by its desktop name), else the first Wayland session installed. An account that asks for
more than a password (a one-time code) can't sign in from this screen yet.

### Lock screen

From `settings_app/idle_lock.rs`.

Rivet Shell's lock screen checks the password through PAM, using the "rivet-shell" service if
`/etc/pam.d/rivet-shell` exists and the "login" service otherwise. With another
locker, the command must keep running until the screen unlocks, so the shell knows the screen is
locked. That is why swaylock's `-f` (which makes it return at once) has to be left out.

## Rivet Snip

### Getting a picture into it, and binding it to a key

Location: `apps/rivet-snip/src/gui.rs`, the empty window ("No picture yet") and More › Take a
Screenshot.

The window says only that a screenshot can be taken, a picture opened or one pasted. The guide
should cover:

- How a compositor hands over its own screenshot: `rivet-snip FILE --replace` on the file it
  saved, or the PNG piped to `rivet-snip -`. With Softcore this is what its screenshot keys
  will do once it is told to; until then, run it on the saved file.
- On other compositors, binding a key to `rivet-snip capture` (and `--delay SECONDS`). It
  uses `grim` where programs may read the screen, else the desktop asks whether to allow it.
- Take a Screenshot closes the window first so it isn't in the picture, then opens a new one
  with what was taken. It is unavailable while there are marks that were never copied or saved.

### Where Save writes

Location: `apps/rivet-snip/src/gui.rs`, the Save button ("Saved as …" at the bottom).

The three cases: over the file when opened with `--replace`; `NAME marked.png` beside a file
opened without it, so the original is kept; `Screenshot from DATE TIME.png` in
Pictures/Screenshots (or `save_dir`) when the picture didn't come from a file. A second Save
writes to the same file again.

### Blur

Location: `apps/rivet-snip/src/gui.rs`, the Blur tool's tooltip ("Drag over what shouldn't be
read.").

Blur replaces the area with squares that each hold the average color of what they covered, in
the saved picture itself. Nothing of the original is left under it in the file. The squares are
at least 8 pixels across; very large text may still be guessed at from its shape, so cover
generously.


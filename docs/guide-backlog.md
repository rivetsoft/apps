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

### Applications: terminals and editors in the theme's colors

Location: `apps/rivet-shell/src/settings_app/themed_apps.rs`, Applications, "Terminals and
editors"; the work is `apps/rivet-shell/src/themed.rs`.

- kitty, Ghostty, Alacritty, foot, Zed and btop. Each one installed has a switch; the rest are
  left out.
- Turned on, Rivet writes one file of its own in that program's settings folder, in the
  program's own format, and writes it again whenever the theme's colors change. The colors
  are Rivet's: the windows' background and text, the accent, secondary and tertiary, and
  sixteen terminal colors made the way Rivet Terminal makes its own, so red stays reddish
  while leaning toward the theme. This works with a palette chosen in Rivet too, with or
  without matugen.
- Rivet never rewrites a program's settings file. What points the program at Rivet's file:
  - kitty: the line `include rivet-colors.conf` in `kitty.conf`. Add the Line adds it at the end.
  - Ghostty: the line `theme = rivet` in its `config`. Add the Line adds it at the end.
  - foot: the line `include=~/.config/foot/rivet-colors.ini` at the top of `foot.ini`, before
    the first section, which is the only place foot reads it. Add the Line puts it there.
  - Alacritty: `import = ["rivet-colors.toml"]` under `[general]` in `alacritty.toml`, added by
    hand, because where it goes depends on what the file already has.
  - Zed: choose the theme named Rivet in Zed's theme selector.
  - btop: Esc, Options, Color theme, rivet.
  Add the Line keeps everything already in the file, follows a link to where the file really
  is, and puts a comment above the line saying where it came from.
- When an open program picks the change up: kitty and Ghostty are sent a signal and reread
  at once; Alacritty and Zed watch the file themselves; foot and btop show it in windows
  opened afterwards.
- Turned off, Rivet takes its file away. The line added to the program's settings stays and
  is harmless; remove it by hand to be tidy.
- Colors set later in a program's own settings file win over Rivet's in kitty (the include
  comes last when Rivet adds it, so Rivet's win there unless more is added below).
- Not covered yet: GTK and Qt apps, VS Code, Neovim, Helix, Firefox and Discord.

### Browser colors

Location: `apps/rivet-shell/src/settings_app/browser_colors.rs`, Applications (under Look),
"Browser colors"; the Applications page is where other apps that take the theme's colors will go;
the work is `apps/rivet-shell/src/browser.rs`.

- Chromium, Chrome, Brave (and Brave Origin) and Edge take one color from the theme and make
  their whole look from it. Tabs, toolbar and menus stay the browser's own; extensions,
  sign-ins and protected video are untouched.
- Color: Windows or Accent. The browser paints its window in the color it is given and picks
  light or dark text to suit, so the color itself decides light or dark; there is no separate
  policy for that. Windows (the default) gives it the desktop's window color with a little of
  the accent mixed in: dark under a dark theme, with the accent's hue on buttons and outlines.
  A plain gray would leave the browser to pick a hue of its own. Accent gives it the accent
  for the whole window, which is bright under most themes. Single parts can't be set.
- Darker (shown under Windows): takes that color down, 0% to 100% in steps of 5, for a browser
  darker than the desktop's windows. It scales the color, which keeps the accent's hue in it,
  and stops just short of black, so the buttons keep the accent. The limit is the browser's:
  it paints its frame in exactly the color given and always draws the toolbar and tabs a
  fixed step lighter, so at 100% the frame is almost black and the toolbar is dark gray
  (about #3b3632 in Brave). Nothing in the one color can make the toolbar darker.
- Shade (shown under Accent): the accent 100% darker to 100% lighter, in steps of 5. A darker
  accent gives a dark, strongly colored browser; a lighter one a pale browser.
- It follows the theme: the shell writes the color again when the wallpaper's colors change
  and tells a running browser to read it, so open windows recolor with no restart.
- How: these browsers read "managed policies" from a folder under `/etc`
  (`/etc/chromium/policies/managed`, `/etc/opt/chrome/…`, `/etc/brave/…`, `/etc/opt/edge/…`).
  Rivet keeps one file there, `rivet-color.json`, with `BrowserThemeColor`. The folders are root's, so Allow asks for the password once to make
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

### Settings: the overview, and the Network, Bluetooth and Sound pages

`settings_app/nav.rs` (the overview), `settings_app/connect.rs` (Network, Bluetooth), `settings_app/sound.rs`.

- Settings opens on an overview: every page as a tile, in the sidebar's groups, with search above. Opening a page brings the sidebar; All Settings at its top goes back. Typing in the overview's search shows the results in the sidebar, and clearing it returns to the overview.
- A page shows only when the computer can use it: Network needs NetworkManager (`nmcli`), Bluetooth needs BlueZ (`bluetoothctl`) and an adapter, Sound needs `pactl` (PipeWire or PulseAudio).
- Network, Wired: one row for each wired adapter, saying whether a cable is in and, when connected, the link's speed and this computer's address, with Connect or Disconnect. The card is left out on a computer with no wired adapter, and the Wi-Fi card is left out on one with no Wi-Fi.
- Network, Wi-Fi: the switch, the networks in range, Join (a new secured network asks for its password first), Disconnect, and Forget for a network the computer remembers. VPNs and setting an address by hand are not on the page yet.
- Bluetooth: the switch, My devices (paired: Connect, Disconnect, Forget) and Devices nearby (Pair). Pairing works for devices that need no code; a device that shows a code to confirm or type has to be paired another way for now.
- Sound: Output and Input each list the devices, with Use to choose one; choosing an output moves the apps playing to it. Volume and Mute under each list act on the one in use. Apps lists what is playing, with a volume each and, when there is more than one output, which output it plays on.
- The pages read only while they are showing, and use the same code as the panel's Network, Bluetooth and Sound popups.
- Getting around with the keyboard: Tab moves between controls and Enter or Space uses one; while the keyboard is in use, what it's on has a ring in the accent, and a page scrolls to it. In the sidebar, Up and Down go through the pages, opening each (up from the first is All Settings). In the overview, the arrow keys go from tile to tile, Down from the search field reaches the first one, and Up from the top row goes back to the field. Anywhere: Ctrl+Page Down and Ctrl+Page Up (or Alt+Down and Alt+Up) open the next and previous page; Alt+Left, or Esc when nothing is being typed and no menu is open, goes back to All Settings; Ctrl+F is search.

### VPN: the page in Settings and the block in quick settings

`vpn.rs`, `settings_app/vpn_page.rs`, `quick.rs` (the VPN toggle).

- Three kinds in one page, each shown only when its program is installed: NetworkManager's VPN connections (`nmcli`), Tailscale (`tailscale`) and ZeroTier (`zerotier-cli`).
- Connections: every WireGuard or other VPN connection NetworkManager keeps, with Connect or Disconnect. Rivet does not add or import connections yet: add them with NetworkManager's own tools. A tunnel another program brought up (wg-quick, for one) is listed as connected with no button, because NetworkManager only watches it.
- Tailscale: Connect and Disconnect, the tailnet's name and this computer's address, and Exit node when the tailnet offers any. If Tailscale refuses because this user isn't its operator, the card shows Allow, which asks for the password once and makes this user the operator. Signing in is still done with `tailscale login` in a terminal.
- ZeroTier: its networks can only be read with ZeroTier's key, which belongs to root. Allow asks for the password once and puts a copy of the key in the home folder, readable by this user only, which is the way ZeroTier documents. After that: each joined network with Leave, a field to join one by its 16-character ID, and the networks left earlier with Join and Forget. Rivet remembers those in its own settings, since ZeroTier forgets a network when it is left. A new network usually has to be approved by whoever runs it before it connects; the row says so.
- Quick settings: a VPN block appears when there is at least one VPN that can be switched. It turns one on and off: the first there is, or the one chosen under Quick settings on the VPN page when there are several. Its arrow opens the VPN page. For a ZeroTier network, off means leaving it and on means joining it again.
- The shell reads the VPNs every ten seconds with the rest of the slower status.

### Applications: Discord

Location: `apps/rivet-shell/src/settings_app/themed_apps.rs`, Applications; the theme is
written by `discord()` in `apps/rivet-shell/src/themed.rs`.

- Discord itself has no themes. A mod adds them: BetterDiscord or Vencord inside Discord, or a
  client built on one (Vesktop, Equibop). Each shows on the Applications page when its settings
  folder exists, under its own name.
- Turned on, Rivet writes a theme named Rivet into that mod's themes folder and writes it again
  when the theme changes. The person turns it on once, in Discord's settings under Themes. The
  mods watch that folder, so a new wallpaper recolours a running Discord.
- It only sets colours: the chat is the window's colour, the lists beside it and what floats
  over it step away from it, links, buttons and mentions take the accent, and online, idle and
  busy take the theme's green, yellow and red. Discord's layout is untouched, and the file
  loads nothing from the internet.
- Turned off, Rivet's theme file is taken away.

### Widgets: placing them, and the HUD

Location: `apps/rivet-shell/src/settings_app.rs`, Widgets, "On the desktop" and "HUD"; the boards
themselves are `apps/rivet-shell/src/board.rs`.

- There are two boards of widgets. The desktop's sit on the wallpaper, under the windows. The
  HUD's open over the windows, on a dark backdrop that is blurred where the compositor can blur.
- The widgets: Clock, Calendar, Events (what's coming up, from a calendar plugin such as Rivet
  Calendar's), Apps (a grid, the launcher's favorites first), Now Playing, Weather and System. A
  display shows one of each kind per board.
- Edit Widgets and Edit the HUD open the board over the windows. Drag a widget to move it. Drag
  the lines at its lower right corner to resize it; the Clock grows and shrinks with its room.
  Click a widget for its own buttons: "On Every Display" or "Only on NAME" (the display being
  edited), "Its Own Size" (after a resize) and "Remove". The bar at the top adds the kinds not
  on this display yet. Done or Esc ends editing. Changes save as they are made.
- "Lock to Grid" in the editor's bar (on for the HUD, off for the desktop, remembered for each):
  a widget dragged keeps its top left corner on the grid's lines, and one resized is a whole
  number of cells wide and tall, with the same gap between neighbours. The grid is 24 columns
  across the room with rows as near square as fit, and shows as dots while a widget is held.
  Widgets already placed stay where they are until they are next dragged.
- Places and sizes are kept as fractions of the screen's room (the screen less its panels), so a
  layout holds on a display of another size. A desktop widget that has never been moved stacks
  on the side chosen by "Side"; Reset Positions puts them all back there at their own sizes.
- Outside editing, desktop widgets don't move, so the buttons in Now Playing, the calendar's
  days and the apps can be clicked.
- The HUD opens with `rivet-shell msg hud` (bind it to a key in the compositor's settings) and
  closes with the same, Esc, or a click on the backdrop. "Open from a corner" opens it when the
  pointer reaches that corner of any display; the corner opens it and never closes it. Typing
  narrows the Apps widget, Enter opens the first app found, and Esc clears what was typed before
  it closes the HUD. `rivet-shell msg hud edit` and `rivet-shell msg widgets edit` open the two
  editors directly.
- Reset the HUD puts back the layout a new setup has: the clock, calendar and events on the left,
  apps in the middle, and what's playing, the weather and the system on the right.
- Settings files from before this keep their desktop clock, system and weather widgets where
  they were.

### Dock: moving the pinned apps

Press a pinned app's icon and drag it along the dock: the other pins move over to show where it
will land, and letting go leaves it there. A press let go without moving still opens the app.
The order is the same list the Dock page in Settings shows, so the page follows within a moment,
and Move Left and Move Right, there and in an icon's right-click menu, do the same thing a step
at a time. Only pinned apps move: open apps that aren't pinned, Downloads and the Trash keep
their places at the end.

### Launcher: rows and columns

Location: `apps/rivet-shell/src/settings_app/launcher.rs`, Launcher, Layout, "Rows" and
"Columns"; the size is worked out in `apps/rivet-shell/src/launcher.rs`.

- Rows: how many rows of apps the launcher shows before it scrolls, 1 to 20, in the grid and
  in the list. The launcher is exactly that tall. On a screen too short for them it shows as
  many as fit.
- Columns (grid only): how many apps across, 1 to 12. The launcher is exactly that wide, so
  bigger icons make a wider launcher. On a screen too narrow it shows as many as fit.
- "From the size" (the slider at its left end) leaves that direction to Size, as before.
  The two are separate: Rows can be set with Columns left to Size, and the other way round.

### User: your picture, and the weather's place

Location: `apps/rivet-shell/src/settings_app/user_page.rs`, User; the picture is handled in
`apps/rivet-shell/src/user.rs`, the panel item in `bar.rs` ("user").

- Your picture: Choose a Picture opens the file picker. Rivet keeps the middle square of the
  picture, 512 pixels a side, as the file `.face` in the home folder, which is where the lock
  screen, the login screen and other desktops look. Remove takes that file away.
- With no `.face`, the picture the system keeps for the account is shown (one set from
  another desktop's settings). Rivet can't remove that one, so Remove isn't offered for it;
  choosing a picture here takes its place in Rivet.
- With no picture at all, the first letter of the name shows on the accent.
- The name is the account's full name, and can't be changed here.
- Weather: the place and the units, for the weather widget and the panel's Weather item
  alike. They used to be on the Widgets page, which now points here. The panel item's own
  options still show them too.
- The panel's User item (Panels, Add Item, User): the picture in a circle. Clicking it opens
  the same Lock, Suspend, Log Out, Restart and Shut Down as the Power item.

### Clipboard: what is kept, and where

From `clip.rs` and the Clipboard page (`settings_app.rs`, "Keep a history of what you copy").

- Rivet Shell reads the clipboard through the compositor's data-control protocol, which
  Softcore, niri, Hyprland, Sway, river, labwc and Wayfire have. GNOME doesn't share the
  clipboard this way, so there the Clipboard item, `@clip` and `rivet-shell msg clipboard` are
  left out.
- It keeps text, and pictures copied as PNG, JPEG or WebP. A copy of other things (files from a
  file manager, rich text alone) isn't kept. A text copy larger than 1 MB or a picture larger
  than 16 MB is skipped, and pictures that aren't pinned share 64 MB: past that the oldest go.
- Copying the same thing again moves it to the top rather than adding it twice.
- Password managers such as KeePassXC mark a password they copy as secret; those copies are
  never read. A password manager that doesn't mark them can't be told apart from other text.
- "Keep the history after a restart" saves it in `~/.local/share/rivet-shell/clipboard`, a file
  only your account can read. Turning it off deletes that file; turning the history off forgets
  every copy, pinned ones too.
- `rivet-shell msg clipboard` opens the list; `rivet-shell msg clipboard 2` copies the second
  copy in the list again (pinned ones come first); `rivet-shell msg clipboard clear` clears the
  copies that aren't pinned. In the list, the arrow keys choose, Enter copies and Delete removes.

### Login screen (greetd)

From `apps/rivet-greeter` (`rivet-greeter --setup`, `rivet-greeter sync`) and Rivet Shell's
`greeter.rs`.

The login screen is Rivet Greeter, which draws Rivet Shell's lock screen, run by greetd. greetd
runs it as its own user, which can't read anyone's home folder, so each person's Rivet Shell
leaves a copy of their wallpaper, their colors and clock, their name, their picture (`~/.face`)
and their session in `/var/lib/rivet-greeter/LOGIN`, whenever the wallpaper or its colors
change. On a desktop without Rivet Shell, `rivet-greeter sync --wallpaper PICTURE` leaves the
same, with the desktop's colors. `rivet-shell greeter` (Rivet Shell 0.1.2's own greeter) runs
Rivet Greeter for now. The login screen
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

## Rivet Vice

### Opening, extracting and making archives

Location: `apps/rivet-vice/src/gui.rs`; the work is `crates/rivet-archive`.

- What it opens depends on the archive tools installed: tar reads every tar; 7-Zip reads
  zip, 7z, rar, iso, cab, deb and rpm; without 7-Zip, unzip reads zip and bsdtar the rest.
  With none that fits, opening says so and names 7-Zip as the one to install.
- The list shows the archive as folders. An archive whose top is one folder opens inside
  it. A folder's Size column counts the files under it. Changed is the time the archive
  tool prints, which is the time on the clock where the file was packed for zip, and this
  computer's local time for tar.
- Extract All asks for a folder. One thing at the archive's top comes out as itself;
  several come out in a folder named after the archive. A name that is taken gets a number
  ("Trip 2"), so nothing is written over. Extract Selected puts the selected rows in the
  folder chosen, inside the folders they have in the archive, and does write over files of
  the same name there, as the archive tools do.
- Compress a Folder, or files dropped on the window, asks for a name and a kind. The archive
  is made beside what it holds, and then opened. A kind whose tool isn't installed is
  greyed out.
- Stop, on the bottom line while work is under way, ends the tool and removes a half-made
  archive.
- Show in Files, after extracting or making, opens the folder in the file manager.
- Not there yet: adding to or deleting from an archive, opening one file from an archive,
  passwords, split archives.

### Rivet Files: Open in Rivet Vice

Location: `apps/rivet-files/src/app/views.rs`, the right-click menu, "Open in Rivet Vice".

- Shown for one selected archive, when the `rivet-vice` program is beside Rivet Files'
  own or on the path. Extract Here is still there for taking everything out at once.

### Panel calendar: events

`ui.rs` (`calendar`), `plugins.rs` (`DayEvent`), PLUGINS.md (Calendar events).

- With a plugin that gives events turned on (Rivet Calendar's is Calendar, in Settings, Plugins), the panel's calendar puts a dot under each day that has events, one for each calendar's colour, up to three.
- Under the month, the chosen day's events are listed: today's when it opens. Click another day to list its; the month's name goes back to this month.
- Each shows its time (or All day) and title, five at most and then how many more. Clicking one closes the calendar and opens the event in its app.
- With no such plugin, the calendar is the month alone, as before.

## Rivet Calendar

### Where the calendars are, and sharing them

`rivet-cal/src/store.rs`.

- Each calendar is a folder in `~/.local/share/rivet-calendar`: one `.ics` file per event, and the calendar's name and colour in the files `displayname` and `color`. A folder with a file called `.read-only` in it can't be changed from Rivet.
- That's the layout vdirsyncer and khal use. Point vdirsyncer's local storage at a calendar's folder to keep it in step with a CalDAV server (Nextcloud, Fastmail, iCloud); khal reads and writes the same folders in a terminal.
- Changes made to the folders by another program show within a few seconds, in the window and on the panel.

### Subscriptions and .ics files

`rivet-cal/src/feed.rs`, `app.rs`.

- + beside Calendars, then Subscribe to a Feed: paste a `webcal://` or `https://` address. Left empty, the name is the feed's own. Opening a `webcal://` link with Rivet Calendar (it's set to open them) does the same.
- A subscription is a calendar folder like the others, marked read-only, with its address in `.rivet-feed`. Its events can't be changed or deleted here.
- It's read again every 6 hours while Rivet Calendar or the panel's Calendar plugin is running. Right-click it for Refresh Now; hover it to see when it was last read. An address that doesn't answer is tried again at the next 6 hours, not sooner.
- Unsubscribe (right-click) moves the folder to the Trash, as Move to Trash does for one's own calendars: put it back from the Trash to have it again.
- Opening an `.ics` file asks which calendar to add its events to. An event with the same UID as one already there replaces it.
- From a terminal: `rivet-calendar subscribe URL [NAME]`, `rivet-calendar refresh`. Feeds are downloaded with curl.

### CalDAV accounts

`rivet-cal/src/caldav.rs`, `app.rs`.

- + beside Calendars, then Add a CalDAV Account: the server's address (the host alone is often enough: Rivet looks for `/.well-known/caldav`), the user name and password. iCloud and Fastmail need an app password made on their website; iCloud's server is `caldav.icloud.com`.
- Every calendar of events on the server becomes a calendar here. Task lists are left out.
- They sync both ways: every 5 minutes while Rivet Calendar is open, every 10 while the panel's Calendar plugin runs, and a few seconds after an event is saved or deleted here. Right-click one of its calendars for Sync Now.
- When the same event changed here and on the server since the last sync, the server's version is kept and the change here is lost. An event deleted on the server but changed here goes back up.
- The password is kept in the desktop's keyring (GNOME Keyring, KWallet, KeePassXC), which needs `secret-tool` (libsecret). Requests are made with curl, which is given the password on its input, never on a command line.
- Remove Account (right-click) moves its calendars here to the Trash and forgets the password; the calendars on the server stay as they are.
- From a terminal: `rivet-calendar account add URL USER [NAME]` (the password is read from input), `rivet-calendar account`, `rivet-calendar account remove ID`, `rivet-calendar sync`.

### Google and Microsoft accounts

`rivet-cal/src/oauth.rs`, `caldav.rs`, `graph.rs`, `app.rs` (`account_window`).

- + beside Calendars, then Add a Google Account or Add a Microsoft Account. Sign In opens the browser on Google's or Microsoft's own sign-in page; Rivet never sees the password. The browser comes back to Rivet Calendar on this computer, and the dialog closes once the calendars are added. Stop gives up waiting (after 5 minutes it gives up by itself).
- Google calendars sync over Google's CalDAV, Microsoft's (Outlook.com, Microsoft 365) over Microsoft Graph. Both follow the same rules as a CalDAV account: both ways, every 5 minutes, soon after a change, Microsoft's or Google's version kept when both changed.
- Calendars shared with the account to look at, not change, are read-only here.
- What stays signed in is a token in the keyring, never the password. Removing the account forgets it; to cut Rivet off on the provider's side too, remove its access in the Google or Microsoft account's security settings.
- Microsoft: Rivet keeps an event's title, times, place, notes and how it repeats. Attendees, reminders and categories stay as they are in Outlook and aren't shown here. Outlook can't repeat every hour, or on more than one day of the month; such a rule made here can't go up, and the sync says so. Changed and deleted occurrences of a repeating event are read for two months back and a little over a year ahead.
- "Sign in with my own client ID": for someone who'd rather sign in as an app they registered themselves (their own Google Cloud project or Microsoft Entra app). Google also needs the client secret it gives with the ID. The ID is kept with the account and the secret in the keyring. A copy of Rivet Calendar built without client IDs of its own only offers this. How to register one: `docs/calendar-accounts.md`.
- From a terminal: `rivet-calendar account add google [NAME]`, `rivet-calendar account add microsoft [NAME]`, each with `--client-id ID` for one's own (Google's client secret is then read from input).

### Events in more than one calendar

`rivet-cal/src/store.rs` (`roll_up`), `app.rs` (`also_stripes`, `in_calendars`).

- When the same event is in several calendars at the same time (an invitation in your calendar and colleagues' shared ones, which keep the same event ID), it shows once. Its other calendars' colours show as extra dots (month view), stripes down its right edge (day and week, whole-day events) or a split bar (agenda). Pointing at it names the calendars.
- Opening it opens the copy in a calendar you can change, if one of them can be changed; otherwise the first in the sidebar.
- The same event at a different time in one calendar (someone moved their copy) shows separately.
- Hiding a calendar hides only its copy: the event still shows from the others.

### Repeating events

`app.rs` (the editor), `rivet-cal/src/expand.rs`.

- The editor offers every day, every weekday, every week, every month and every year. A rule made in another program (every second Tuesday, the last Friday of the month) shows as Custom and is kept as it is unless another is chosen.
- Opening one occurrence of a repeating event offers Change: This One or Every One. This One changes only that date (the others stay as they were); Every One moves the whole series by as much as this occurrence was moved.
- Delete does the same: this occurrence only, or the event and every occurrence.

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

## Rivet Long Shot

### Stepping, zooming and turning

Location: `apps/rivet-longshot/src/gui.rs`; the picture stage is `crates/rivet-media/src/stage.rs`.

- The folder's order: Rivet Files passes the sort it shows (name, size, kind or changed, either
  way round) when it opens a picture in Rivet Long Shot. Opened any other way, it is by name, with
  numbers counted as numbers. Hidden pictures are left out unless the one opened is hidden.
- Zoom: the wheel and a pinch zoom around the pointer, from fitting the window up to 3200%.
  "Fit" never enlarges a picture past its own size. Double-click goes between fitting and 100%.
- Turned for viewing: the bar over the picture offers Save and Turn Back. Moving to another
  picture, or opening one, asks "Save the turned picture?" with Save Turned and Leave As It Was.
  A JPEG changes only its orientation note; PNG, WebP, TIFF, BMP, QOI, TGA and PNM are written
  again with the same pixels, without camera details; the rest can't be saved turned.
- Set as Wallpaper shows only while Rivet Shell runs in the session.
- Move to Trash puts the picture in the desktop's trash; Undo on the note puts it back where it
  was, unless another file has taken its name.
- HEIC, AVIF and JPEG XL open through a program that reads them: libheif's heif-dec (or
  heif-convert), djxl for JPEG XL, ImageMagick, or ffmpeg, tried in that order.

### Rivet Files: Open in Rivet Long Shot

Location: `apps/rivet-files/src/app/views.rs`, the right-click menu, and `open_entry` in `app.rs`.

- Shown for one selected picture when the `rivet-longshot` program is beside Rivet Files' own or on
  the path, and Rivet Long Shot isn't already the app Open uses. When it is, Open passes the folder's
  sort to it.

## Rivet Cinema

### Keep on top

From `gui.rs`, the Keep on Top button's tooltip.

Keep on Top shows the video in a small window of its own with no frame: drag it to move it,
click it to pause, double-click it or press Escape to go back. On X11 it stays above other
windows by itself. Wayland doesn't let a window ask for that, so the small window has its own
app id, `rivet-cinema-pip`, for a compositor rule (the README lists Softcore's, Hyprland's,
niri's and Sway's).

### Preferences: languages and hardware decoding

From `gui.rs`, Preferences.

Audio and subtitle languages are codes such as `en`, `eng` or `fr`, first choice first; a file
says which language each track is in, and the first that matches is chosen when a file opens.
Hardware decoding uses the graphics card (VA-API) for the formats it knows; a driver that gets
it wrong shows blocks, green frames or a black picture, which turning it off fixes.

### Resume

From `gui.rs`, "Started from …" and Preferences.

Rivet Cinema keeps the place in files over a minute long, once past the first 30 seconds and
until the last 5%. The places are kept for the newest 500 files, under a key made from the
file's path and size, so a file replaced by another starts from the top.

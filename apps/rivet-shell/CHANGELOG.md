# Changelog

## Unreleased

- An Edit plugin: Rivet Edit's five most recent folders on the panel and in the launcher
  (`@edit`). Turn it on in Settings, Plugins.
- A new icon in Rivet's pink, to match the rest of the apps and the website.
- Now Playing does more. A click opens a player with the cover, the album, how far in the
  track is (click the line to jump), and previous, play and next. The item itself shows the
  cover, and its options choose the text and how wide it gets, previous and next buttons,
  hiding while paused, what a click does, and scrolling to change track.
- A visualizer: bars that move with the sound, in the player and, when turned on in Now
  Playing's options, on the panel. It listens only while something plays and needs
  `pw-record`, which comes with PipeWire.
- `rivet-shell msg media` opens the player.
- With Style and Accent, you can choose the secondary and tertiary colors yourself (a blue
  secondary beside a pink accent, say), or leave them made from the accent as before.
- The desktop widgets can be moved: drag the clock or the system widget anywhere on the
  desktop, and it keeps its place on every screen. Double-click one, or use Reset Positions on
  the Widgets page, to put it back.
- The system widget has as much room below its last row as above its first.
- The shell asks for your password when an app needs administrator rights (installing
  software, changing system settings, `pkexec`), in its own dialog, so no separate polkit
  agent has to be started. Where one is already running, the shell leaves the asking to it.
  Settings, Idle & Lock has the switch.
- `rivet-shell --install-service` starts the shell with your session as a systemd service,
  which also brings it back if it stops by accident; `--remove-service` takes it out. It
  works where the compositor starts systemd's graphical session (niri-session, uwsm).
- Tray icons stay sharp when they're shown in one color (the panel's text or highlight
  color): their details no longer turn to a gray blur.
- Settings: Appearance's "Color use" is one choice with a legend of what the accent, secondary
  and tertiary colors are on at that setting. Choices read in title case (Tonal Spot, From the
  Wallpaper, Smart Hide), and hints and tips are reworded to be plainer.
- Settings speaks plainly: descriptions say what a setting does for you, without file names,
  services or variables, and settings whose name says it all have no description. The "Shell
  running" badge, the plugin-writing notes and plugins' folder paths are gone.
- A panel's or the dock's own look is easier to read: a row you've changed gets a Reset button
  that puts it back to Appearance's value, in place of the "Appearance" chip on every row.
- Smart Hide and the dock's open-app dots say they work on mango too. The dots and open apps only
  appear where the compositor supports them.
- Commands in the launcher: `@` and a plugin's word show only that plugin's entries, and the
  words after it search them (`@ftp work`, `@notes plan`). `@` alone lists the words.
- An FTP plugin, for Rivet FTP: your saved connections in a panel item's menu, under their
  groups, and in the launcher (by name, or all of them under `@ftp`). Choosing one opens a new
  Rivet FTP window connected to it.
- The Notes plugin finds notes by name under `@notes`, and `@notes some words` makes a note.

- A plugin's panel icon shows in its own colors, or as a solid shape in the panel's text color
  or the accent, like the tray's icons (the item's options, on the Panels page).

- A Notes plugin, for Rivet Notes: the vault's open tasks and latest notes in a panel item's
  menu, a dot and a notification when something is due today, and New Note in the launcher
  (type a title, choose New Note). Its vault is chosen in Settings, Plugins.
- Plugins can do more: settings of their own, shown under them in Settings (`setting.KEY=` in
  plugin.conf: a folder, some text or a toggle); entries in the launcher; menus with headings,
  icons and tasks' boxes; and a dot on their panel item. PLUGINS.md has the whole of it.

- Settings → Panels → Groups shows one group at a time: the groups are tabs in their own
  colors, and the chosen one's settings sit under them, with colors as swatches.
- The shell uses the theme's secondary and tertiary colours, as much as Appearance's "Colour
  use" says: Accent Only; Some (switches and quick settings that are on, notifications, the
  system widget's bars), the default; or A Lot (also sliders, the thing in use in a list,
  running apps' dots, what's playing).
- The style, accent and surface chosen in Settings → Appearance are followed by every Rivet
  app, like a palette or the wallpaper's colors are.
- A panel group's height follows the panel, or is its own: from 12 points up to the panel's
  whole height, where the group fills the panel edge to edge (its width, on a side panel).
- The Quick Settings panel item's symbol can be sliders (as before), switches, tiles or a gear
  (its options in Settings, Panels).
- Settings → Appearance → Colors → Style and accent has a Surface row: the shell's background
  color, on a scale of contrast (Black to Soft; White to Grey in the light style) or any
  color. Dark and light keep one each (`surface_dark=`, `surface_light=`). "Tint the surface
  with the accent" makes it follow the accent (`surface_tint=`).
- Settings → Appearance → Icons: the icon themes installed on the system, to choose the one
  the dock, launcher, panels and tray take their icons from (the desktop's own by default). In
  the settings file: `icon_theme=`.
- Settings: the Theme page is now Wallpaper (the picture and its transition), and Appearance
  starts with one Colors card that asks where the colors come from: the wallpaper (matugen and
  its options), a palette, or the shell's own style and accent. Only the chosen one's settings
  show. Font and text size have a Text card of their own. matugen now runs only when the colors
  come from the wallpaper.
- Palettes, for a desktop without matugen: Settings → Appearance → Colors → A palette lists the built-in
  color schemes (the ones Rivet Edit and Rivet Terminal color code with: Nord, Dracula, Gruvbox,
  Catppuccin, Tokyo Night and the rest). The chosen one colors the shell and every Rivet app,
  in place of colors made from the wallpaper. `rivet-shell palette NAME` (or `none`) does the
  same; `rivet-shell palette` lists them. In the settings file: `palette=`.
- The shell comes back by itself after a crash: `rivet-shell` now watches the shell proper and
  starts it again (giving up after six crashes in a row soon after starting). Panics, the lock
  screen's too, are written to `~/.local/state/rivet-shell/crash.log` with where they happened.
  `RIVET_SHELL_NO_WATCH=1` runs it unwatched, for a debugger.
- A new wallpaper shows much sooner: scaling a picture to the screen is several times faster,
  and the shell takes a change made in Settings at once rather than at its next check.
- A panel's Look sets the bar's height (a side panel's width), 24 to 72 points, and the room
  between its items, 0 to 24 points (36 and 2 as before by default).
- Appearance sets the font the shell and its settings are written in (any installed family,
  with a search; the desktop's by default). A panel can have its own (its Look), which its
  popups share, and so can the launcher (the Launcher page).
- A panel's Look sets how far a floating bar sits from the screen's edge, and how much room is
  kept between the bar and windows (0 to 64 points each; 6 and 6 as before by default).
- Workspaces can show as squares: rounded squares all one size, the active one in the accent
  colour (the Workspaces item's options).
- A panel's groups can fuse: two groups with backgrounds side by side join into one shape, each
  keeping its own colour (Settings, Panels, Groups: "Groups that touch fuse into one shape").
- A panel group's background and text can be Surface, the theme's own surface colour, beside
  Primary, Secondary and Tertiary.
- A panel group's background can be a gradient: it fades along the panel from its colour to a
  darker shade made from it (Settings, Panels, Groups: "Fade to a darker shade").
- A Spacer panel item: empty room between items, 2 to 600 points, set on its row in Settings,
  Panels (drag the number, or click it to type). A panel takes as many as wanted.
- Plugins and hooks. A plugin is a folder with a `plugin.conf` and a program in any language:
  the shell runs it (every so often, or kept running), shows what it prints as a panel item
  (`plugin:NAME`: text, an icon, a tooltip, a state's colour, a menu), and tells it of clicks
  and of the shell's events (lock, unlock, idle, wallpaper, colors, style, notification, Do Not
  Disturb, workspace, media). Official ones come with the shell, third-party ones go in
  `~/.local/share/rivet-shell/plugins`; each is turned on in Settings, Plugins. Hooks run your
  own command for any of those events (`hook.lock=playerctl pause`). Disk Space is the example
  plugin; PLUGINS.md has the whole of it.
- Settings has a Displays page: the screens drawn where they sit (drag one to move it; it snaps
  to the others' edges), and each one's resolution, refresh rate, scale, rotation, variable
  refresh rate and whether it's used. Apply writes them to the same overrides file as the
  window look (`rivet-shell.kdl` for niri, `rivet-shell.conf` for Hyprland's hyprland.conf),
  which the compositor's config includes, then asks whether to keep them and goes back after
  15 seconds with no answer. "Leave the Screens to niri" takes them out again.
- Settings has a Launcher page: the apps as a grid or a list, the launcher's size (small,
  medium, large), its icons' size, whether sums are answered, and whether the screen dims
  behind it.
  It opens in the middle of the screen or from a panel: the one it was opened on, or with
  a key the screen's first.
- Settings' sidebar: the pages sit in groups (Look, Desktop, System) with their own symbols,
  drawn in the accent colour, and in the text's colour on the chosen page. Each page has a line
  under its title saying what it's for, and "Shell running" is a small pill.
- Panel items can be grouped (Settings, Panels: point at an item and click its ring). A
  group's items that sit side by side share a background, and take the group's background
  and text colours: soft, the theme's primary, secondary or tertiary, or your own. A group
  can show only its first few items and fold the rest behind an arrow on the panel.
- Panels go on the left and right edges too. Down a side panel the items stack: a symbol sits
  over its number, the clock breaks into lines, the window's title runs downward, and popups
  open beside the panel. A panel can take part of its edge (20 to 100 percent of the screen's
  width or height, placed at the start, middle or end), and can hide like the dock: Auto-hide,
  or Smart hide while the workspace has windows (Settings, Panels: Length, Placed, Hide).
  A panel that hides keeps no room from windows.
- Notifications have their own panel, opened from the bell (`rivet-shell msg notifications`):
  grouped by app, newest first, a line or two of each, Do Not Disturb and Clear All at the top,
  and a Clear for one app's. Clicking one the app can open opens it. Quick settings no longer
  lists them, and has a little more room.
- Settings: the Wallpaper page is now Theme. The colors and the transition come first; the
  Wallpaper card shows the picture in use, and "Change…" opens the chooser (folder, recent
  folders, the pictures) in a popover.
- A Sound popup from the volume item (click; middle-click still mutes): the output's volume and
  every output to choose from (speakers, headphones, Bluetooth, screens, network speakers;
  ones with nothing plugged in are left out), the same for microphones, and each app playing
  with its own volume, mute and the output it plays on. Read with `pactl` (PipeWire or
  PulseAudio) only while it's open; `rivet-shell msg sound` opens it, and the mixer app
  (pavucontrol) when installed. Caps Lock and Num Lock show on screen as they turn on and off,
  beside volume and brightness (Settings, Notifications).
- Wallpaper: a picture per screen (Settings, Wallpaper: choose a screen, then a picture; the
  rest show the shared one), and a transition when the picture changes (fade, wipe, slide or
  grow, 0.2 to 3 seconds). The old picture stays up while the new one loads. Colors from the
  wallpaper gain a contrast setting and, with several screens, which screen's picture they're
  made from.
- The Power item has options (its gear in Settings, Panels): the menu as a list, a compact row
  of symbols, or large tiles in the middle of the screen with your name and the uptime;
  whether log out, restart and shut down ask for a second click; and which actions show. Each
  action has its own symbol.
- Idle and the lock screen. As the computer sits untouched the shell dims the screen, locks,
  turns the screens off and suspends, each after its own time (Settings, Idle & Lock), holding
  off while an app inhibits idle, Keep Awake is on, or music or a video plays. It locks before
  sleep and on `loginctl lock-session` or `rivet-shell msg lock`. The lock screen is its own
  program (`rivet-shell lock`, the session-lock protocol, the password checked with PAM): the
  blurred wallpaper, the time, your picture or initial, and suspend, restart and shut down
  (two clicks); if it stops while locked, the shell starts it again. Another locker
  (hyprlock, swaylock) can be used instead; `rivet-shell lock --preview` tries the look.
- Panel items share one spacing: the same room inside each item's ends and between a symbol
  and its text, so the battery no longer runs into its percent. The window title and the
  workspaces keep a little room around them, the System item has a symbol, items sit on whole
  pixels (sharper symbols), and muting no longer shifts the items beside the volume.
- The compositor is detected (niri, Hyprland, mango, Sway), and mango is supported like the
  others: its tags as workspaces, open apps, the window title, log out. Appearance has a Window
  Look card: gaps, border, rounded corners, unfocused opacity, shadows, and borders in the
  accent colour, written to an overrides file in the compositor's own format
  (`rivet-shell.kdl`, `.conf` or `.lua`) that its config includes; one button adds the include
  line, keeping a copy of the config first.
- An AI Usage panel item: the most used limit of your AI plans in the panel, and a popup with
  each plan's limits, bars and reset times. Claude's comes from what Claude Code already keeps
  (no requests while it's fresh), with whether Claude Code is running and working; Codex and
  Grok use their own sign-ins; OpenRouter and DeepSeek take API keys from the environment or
  `~/.config/rivet-shell/ai.env`. `rivet-shell ai` prints the same; `rivet-shell msg ai`
  opens the popup.
- Pin any AI limit to the panel from its popup (several, from several providers); the AI
  item then shows each, such as "5h 34%  wk 78%".
- Search the settings from the top of the Settings window's sidebar (Ctrl+F): results show
  as you type, with where each one is; the arrow keys and Enter pick one. A result opens its
  page and outlines its card, or opens a panel item's options. Escape clears the search.
- Volume, brightness and battery have settings: show the icon and percent, just the icon or
  just the percent; scroll to change volume or brightness (on by default); hide the battery
  while it's charged.
- The items' settings cards (the gear on an item's row in Settings, Panels) are redone: a
  header with the item's symbol, choices drawn as they look in the panel, switches instead of
  checkboxes. The clock takes a custom strftime pattern too, checked as it's typed.
- The workspaces item has settings: pills, dots or numbers; names or numbers; hide empty
  workspaces; scroll to switch.
- The launcher keeps up with typing: drawing is several times faster (plain shapes take the fast
  path, see-through ones blend two channels at a time), only the changed rows go to the
  compositor, the cursor doesn't blink, and the launcher is made ready at the start.
- Quick settings in the Rivet apps' own look: a title row with toolbar buttons, folding
  sections like Rivet Files' sidebar, thin sliders, toggles that light up like a selected item,
  what's playing as a row, the notifications as a list.
- `rivet-shell msg wallpaper random` (and `next`, `previous`) picks another picture from the
  wallpaper folder.
- A clearer Panels page: a tab for each panel, a live preview of it, Position and Style
  switches, and the three zones side by side, where items are dragged to reorder them or move
  them between zones; Add Item lists what's left with a picture and a line about each.
- The dock can auto-hide or smart-hide (shown while the workspace has no windows), sliding
  away and back when the pointer reaches the bottom edge.
- Several panels, each on every screen or one, with three zones (left, center, right) of items
  chosen and ordered in Settings; a processor and memory item. The old single panel's
  settings carry over.
- Quick settings (sliders, toggles, what's playing, notification history), notifications with
  cards in the corner, an on-screen display for volume and brightness, media and brightness and
  the window title in the panel, open apps and a right-click menu (pin, unpin, move) in the dock,
  a calculator in the launcher, and more `rivet-shell msg` commands for keys.
- The launcher opens at once (its icons are kept between openings).

The first version.

- A panel (launcher, workspaces for niri / Hyprland / Sway, clock with a calendar, volume,
  network, battery, settings, power menu), floating or edge to edge, top or bottom.
- A dock of pinned apps with running dots (niri, Hyprland), and an app launcher with search.
- Desktop widgets: a large clock and date; processor, memory and battery.
- The wallpaper on every screen, with matugen run on it so the desktop's colors follow it.
- A settings window like the other Rivet apps' Preferences; changes apply at once.
- `rivet-shell msg launcher|settings|wallpaper PATH|reload|quit` for key bindings and scripts.
- Drawn in software into Wayland shared memory with Rivet's renderer (now shared in rivet-kit).

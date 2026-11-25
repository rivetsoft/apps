# Changelog

## Unreleased

- An idle window follows desktop color changes right away.

## 0.1.0

- **A Color plugin** (Rivet Color's): the color you picked last on the panel. Click it to pick a new one or copy a recent one, or middle-click to pick straight away. In the launcher, `@color` lists your recent colors and palettes, and `@color #d53f8c` copies a color in the format you use.
- **Hermes Agent in the AI item**: when Hermes Agent is installed, the AI Usage popup shows its last 7 days: sessions, messages, tool calls, tokens, active time and cost. Turn it off on the AI item's page in Settings.
- **Discord can take the theme's colors**: with BetterDiscord or Vencord in Discord, or with Vesktop or Equibop, turn it on under Applications in Settings, then turn on the Rivet theme in Discord's own Themes settings. Discord's layout stays as it is.
- **The HUD**: a screen of widgets that opens over your windows, with a grid of apps, the calendar, the events coming up, what's playing, the weather and the system's readings. Open it with a key (`rivet-shell msg hud`) or from a corner of the screen, chosen on the Widgets page. Type to narrow the apps and press Enter to open the first.
- **Widgets you place yourself**: Edit Widgets on the Widgets page shows the desktop over your windows. Drag a widget to move it, drag its corner to resize it, add or remove one, and choose whether it is on every display or only the one you are editing. The HUD is arranged the same way, and its widgets lock to a grid as you drag them; Lock to Grid in the editor's bar turns that on or off for each. The desktop also gains Calendar, Events, Apps and Now Playing widgets.
- Widgets on the desktop no longer move when dragged outside Edit Widgets, so the ones with buttons can be clicked.
- **A new icon**: a desktop with a sun, hills, a panel and a dock, drawn in the website's colors with a plum outline and a hard shadow.
- **Drag the dock's icons into order**: press a pinned app and drag it along the dock, and the others move over to make room. The Dock page in Settings shows the new order.
- **Events on the panel's calendar**: with the Calendar plugin on (Rivet Calendar's), days with events get a dot in each calendar's color, and the chosen day's events are listed under the month. Click a day to see its events, and an event to open it in Rivet Calendar. Any plugin can give the calendar events the same way.
- **Quick settings opens with you**: your picture and name sit at its top in place of the Quick Settings title, with how long the computer has been up under them. The picture is the one the lock screen shows; without one, your initial shows on the accent.
- **Where an app comes from, in the launcher**: Flatpak, Snap and AppImage apps carry a small package on their icon in the grid, and after their name in the list; point at it to see which. Turn it off on the Launcher page.
- **A User page in Settings**: choose your picture, which shows on the lock screen, the login screen and a new User item for the panel. The weather's place and units moved here from Widgets, and the weather widget and the panel's Weather item both use them. The User item opens Lock, Log Out and the rest when clicked.
- **Settings with the keyboard**: what the keyboard is on has a ring in the accent, and the page scrolls to it. Up and Down go through the pages in the sidebar, the arrow keys move between tiles in All Settings, Ctrl+Page Down and Ctrl+Page Up open the next and previous page from anywhere, and Esc goes back to All Settings.
- **Rows and columns for the launcher**: on the Launcher page, Rows sets how many rows the launcher shows, in the grid or the list, and Columns sets how many the grid has across. Left at "From the size", Size decides as before.
- **Blur behind panels, the dock and pop-ups, in their own shapes**: when they're see-through, Rivet Shell asks the compositor to blur what's behind exactly the parts it draws, rounded corners included, rather than the whole strip a panel's surface spans. Under Softcore this works by itself; turn it off under Panels and dock on the Appearance page. Softcore's own list of surfaces to blur is now for other bars and docks.
- **Downloads and Trash on the dock**: they sit at the dock's end. Click Downloads for your newest files: click one to open it, or drag it into another app. Click Trash to open it, or right-click it to empty it. Turn them off on the Dock page.
- **Drop files on the dock**: drop files on an app to open them in it, or on the Trash to throw them away. While you drag, the apps that can't open those files fade, and a hidden dock comes out.
- **Terminals and editors in the theme's colors**: on the Applications page, turn on kitty, Ghostty, Alacritty, foot, Zed or btop and it takes the theme's colors and changes with the wallpaper. Each one installed gets a switch. Rivet keeps a color file of its own beside the program's settings and never rewrites yours: where a line has to point at it, Add the Line adds that one line.
- **Blur behind panels, picked from a list**: on the Softcore page, choose which shell surfaces to blur behind from Rivet's own (panels, dock, menus, notifications) and whatever else is on screen, such as another bar. Each one shows as a tag you can take off, and Add also takes any other name.
- **An Applications page in Settings**, under Look, for other apps that take the theme's colors. Browser colors moved there from Appearance. With Accent as the browser's color, a new Shade slider makes the accent darker or lighter; with Windows, Darker at 100% now reaches almost black and keeps the accent on the browser's buttons.
- **Browser colors can go darker**: under Browser colors on the Appearance page, Darker takes the browser's window from the color of the desktop's windows down to black.
- **Browser colors follow dark and light**: the browser now takes the color of the desktop's windows, with the accent on its buttons, so it is dark under a dark theme. Before, it took the accent for its whole window, which made it bright. Accent is still there as a choice.
- **VPNs**: a VPN page in Settings lists the VPN connections NetworkManager keeps (WireGuard, OpenVPN and others), Tailscale with its exit node, and ZeroTier's networks, which you can join by ID, leave and join again. Quick settings gets a VPN block that turns one of them on and off; the page chooses which when there are several.
- **Favorites in the launcher**: turn them on under Favorites on the Launcher page and your favorite apps sit in a row above the rest. Left and Right move along that row, Down goes to the other apps, and directions along the bottom show the keys. Add or remove an app with Ctrl+D, by right-clicking it, or by clicking its star.
- **Network, Bluetooth and Sound pages in Settings**: see each wired connection with its speed and address, and connect or disconnect it; join, leave and forget Wi-Fi networks; pair, connect and forget Bluetooth devices; choose where sound plays and records, set each one's volume, and set the volume and output of each app playing. Each page shows only when the computer has what it needs.
- **Settings opens on an overview**: every page as a symbol and its name, group by group, with search above them. Open a page and the sidebar is there as before, with All Settings at its top to go back.
- **Softcore**: under Softcore, Rivet's own compositor, the panel shows workspaces and the focused window's title, the dock shows what is open and can close it, and Log Out and turning the screens off work. Displays and the window look (gaps, borders, corners) can be set from Settings there too; Softcore applies them as you change them.
- **A Softcore page in Settings**, shown only under Softcore: its windows, columns, motion, keyboard and pointer settings, and your key bindings. Each change is one line of Softcore's settings file, so what you wrote there by hand stays, and a change made by hand shows in the window.
- **Border colors on the Softcore page**: the focused window's border and the other windows' border each take One Color or Two Colors (a gradient). Pick each color from the theme (Accent, Secondary, Tertiary), and it changes with the theme when the wallpaper does. Or choose Custom for one of your own, or leave the first on Automatic. Under Softcore, Window Look on Appearance no longer sets border colors.
- The Softcore page's cards are split into parts: the settings about one thing (borders, shadows, blur) sit together, with a line between the parts.
- **Browser colors**: Chromium, Chrome, Brave and Edge can take the theme's color and change with the wallpaper, without a restart. Turn it on under Browser colors on the Appearance page; Rivet asks for your password once. Extensions and everything else about the browser stay as they are.
- **Weather**: a desktop widget and a panel item show the sky and the temperature for a place you choose. Click the panel item for the next hours and the week. Turn the widget on under Widgets in Settings, where you also pick the place and °C or °F.
- Holding a key repeats it in the launcher and the other popups: hold Down to run through the launcher's entries, or Backspace to clear what you typed.
- Fixed a crash that could happen when a click closed a popup or a notification while other parts of the desktop were being drawn.
- A palette of Rivet's own colors: choose Rivet under A Palette on Appearance for the pink accent with its blue and yellow on a dark plum surface.
- Previews in Settings are marked: an eye and "Preview" above, and a dashed line around.
- Appearance opens with a preview of your colors: the four surfaces, from the base up to a pop-up, with text on each and a button in each color.
- Settings is easier to read: on and off are switches, choices are one kind of row, buttons carry a symbol for what they do, and long cards are split into named parts. The Panels page lines its settings up in rows, and each key command has a Copy button.
- **Sub panels**: a panel that stays rolled up under another until you ask for it, to keep the
  main panel clean. Add one with Add Sub Panel on Settings' Panels page and put any items on
  it. Open it with its button, a key (`rivet-shell msg subpanel NAME`) or by pausing the pointer
  just past the panel. It rolls back up a few seconds after you leave it, or stays
  open until you close it: right-click its button to switch. A sub panel can be attached to
  another panel, and is kept when its panel is removed. It joins its panel like a drawer pulled
  out of it, as long as the panel and filled in under its rounded corners; both can be turned off.
  While it's rolled up, a small handle past the panel shows where to pause the pointer; you
  choose its shape (a bar, a line, an arrow or dots), size, place along the panel, distance
  from it, color and opacity, or hide it.
- In the launcher, a command such as @1p or @ftp turns into a badge once its word is typed, and
  what you type next searches its entries. Backspace in the empty field takes it off.
- Right-click an open app in the dock and choose Close to close its windows, on niri and Hyprland.
- Plugins from other people: Get Plugins, on Settings' Plugins page, lists the ones that have
  been shared beside the shell's own, and installs and updates them. One you install stays off
  until you turn it on, and has Remove on the page. `rivet-shell plugin` does the same from a
  terminal.
- The lock screen can be your login screen too: a greeter for greetd that shows the wallpaper,
  colors and clock of whoever changed their wallpaper last, with Tab to pick another account.
  Run `sudo rivet-shell greeter --setup` once to set it up.
- Notifications can show in any corner, narrow, medium or wide, for as long as you choose and
  up to eight at once. They also have a look of their own (corners, opacity, background,
  highlight and outline), starting from Appearance's, under Settings → Notifications, with a
  preview that shows them as they'll appear.
- The volume and brightness pop-up can show at the top, middle or bottom of the screen, in three
  sizes, with its bar in the primary, secondary or tertiary color, for as long as you like.
  Changing one shows the pop-up so you can see it.
- Settings: text fields, slider tracks, buttons and switches now stand out from their card in
  every color scheme, and Remove Panel is an outlined button instead of bright red text.
- Lighter on the computer. The shell used to start six small programs every second to ask
  about workspaces, windows, the volume and what's playing; now it's told when one of them
  changes, so a quiet desktop costs next to nothing and a change shows sooner.
- A widget on the wallpaper changing its number no longer redraws the whole screen.
- Programs the shell started and that have since closed no longer linger in the process list.
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

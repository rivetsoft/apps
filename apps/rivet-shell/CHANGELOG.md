# Changelog

## Unreleased

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

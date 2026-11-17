# Changelog

## Unreleased

The first version.

- A panel (launcher, workspaces for niri / Hyprland / Sway, clock with a calendar, volume,
  network, battery, settings, power menu), floating or edge to edge, top or bottom.
- A dock of pinned apps with running dots (niri, Hyprland), and an app launcher with search.
- Desktop widgets: a large clock and date; processor, memory and battery.
- The wallpaper on every screen, with matugen run on it so the desktop's colors follow it.
- A settings window like the other Rivet apps' Preferences; changes apply at once.
- `rivet-shell msg launcher|settings|wallpaper PATH|reload|quit` for key bindings and scripts.
- Drawn in software into Wayland shared memory with Rivet's renderer (now shared in rivet-kit).

# <img src="data/rivet-shelltarget/release/rivet-shell msg quit              # stop itsvg" width="40" alt=""> Rivet Shell

A desktop shell for Wayland, in the Rivet apps' look: a panel, a dock, desktop widgets and the
wallpaper, with a settings window. **Just started**: not released yet; build it from source.

![Rivet Shell: the panel, widgets and dock over a wallpaper](../../docs/screenshots/shell-wide.png)

- **Panel**: a launcher button, workspaces (niri, Hyprland, Sway), the focused window's title,
  the clock (click for a calendar), what's playing (click to pause), volume and brightness
  (scroll to change), network, battery, notifications, quick settings and a power menu (lock,
  suspend, log out, restart, shut down; the last three ask first). Floating or edge to edge, at
  the top or the bottom.
- **Panels**: as many as you like, each along the top or bottom of every screen or of one,
  floating or edge to edge. Each has three zones (left, center, right) and you choose which
  items go in each and in what order (Settings, Panels). Items are named by id, ready for
  plugins (`plugin:NAME`) later.
- **Quick settings**: volume and brightness sliders; Wi-Fi, Bluetooth, Night Light, Do Not
  Disturb, Keep Awake, power mode, microphone and dark style; what's playing with its buttons;
  and the notification history.
- **Notifications**: the shell is the desktop's notification service (cards in the corner, with
  the apps' buttons; urgent ones stay until closed), unless another one (mako, dunst) runs.
- **On-screen display**: volume and brightness show as they change, from keys or anywhere else.
- **Dock**: pinned apps, then open apps that aren't pinned; a dot under the ones with windows
  open (niri, Hyprland); right-click an icon to pin, unpin or move it. It can auto-hide (back when the pointer
  reaches the bottom edge) or smart-hide (shown while the workspace has no windows).
- **Launcher**: every installed app, searched as you type; arrows and Enter, or click. Type a sum
  (`12*(3+4)`) for its answer; Enter copies it.
- **Widgets** on the desktop: a large clock and the date, processor, memory and battery.
- **Wallpaper and colors**: choose a picture in Settings; the shell shows it on every screen and
  runs [matugen](https://github.com/InioX/matugen) on it, so the desktop's colors (and every
  Rivet app's, which follow matugen live) come from the picture.
- **Settings** (`rivet-shell settings`): wallpaper, colors, panels, dock, widgets and notifications, like the
  other apps' Preferences. Changes apply to the running shell at once.

## Running it

```sh
cargo build --release -p rivet-shell
./target/release/rivet-shell                       # in your Wayland session
./target/release/rivet-shell settings              # the settings window
./target/release/rivet-shell msg launcher          # open (or close) the launcher
./target/release/rivet-shell msg control-center    # quick settings
./target/release/rivet-shell msg volume up         # also down, mute; brightness up|down; media next…
./target/release/rivet-shell msg wallpaper random  # also next, previous, or a picture's path
./target/release/rivet-shell msg quit              # stop it
./target/release/rivet-shell --install-desktop-entry   # Rivet Shell Settings in your launcher
```

Start it with your session and bind a key to the launcher:

| Compositor | Start | Launcher key |
| --- | --- | --- |
| niri | `spawn-at-startup "rivet-shell"` | `Mod+Space { spawn "rivet-shell" "msg" "launcher"; }` |
| Hyprland | `exec-once = rivet-shell` | `bind = SUPER, SPACE, exec, rivet-shell msg launcher` |
| Sway | `exec rivet-shell` | `bindsym $mod+space exec rivet-shell msg launcher` |

It needs a compositor with the wlr layer-shell protocol (niri, Hyprland, Sway, river, labwc,
Wayfire, COSMIC; not GNOME). Volume uses `wpctl` (PipeWire), brightness `brightnessctl`, Wi-Fi
`nmcli`, Bluetooth `bluetoothctl`, music `playerctl`, power modes `powerprofilesctl`, Night Light
`gammastep`, `hyprsunset` or `wlsunset`, power `loginctl` and `systemctl`; anything missing
simply doesn't show. Stop another shell or bar
(Noctalia, Waybar…) first, or both will be on screen.

## How it's made

Each part is a layer surface drawn with egui by Rivet's own software renderer
(`rivet_kit::soft`) into shared memory: no GPU, and only when something changed (the clock each
minute, the status when it changes, hover effects). Settings live in
`~/.config/rivet-shell/settings`.

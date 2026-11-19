# Before and after, from Rivet

These are real strings from Rivet Shell's Settings window, rewritten on 2026-11-19.

## Machinery in a description

| Before | After | Why |
| --- | --- | --- |
| Your password is checked with PAM (the "login" service). | Unlock with the password you log in with. | Names a service; says nothing the person can act on |
| Apply writes to rivet-shell.kdl (included by config.kdl) and asks you to confirm. With no answer in 15 seconds, the screens revert. | After you apply, you have 15 seconds to keep the changes. If you don't, the screens go back to how they were. | File names out; the consequence kept, from the person's side |
| Adds one line at the end (include "rivet-shell.kdl") so settings here override earlier ones. Backs up the file first. | Adds one line to the end of config.kdl so these settings take effect. A copy of the original is saved first. | The effect and the safety net, without the syntax |
| Rivet Shell writes them to ~/.config/niri/rivet-shell.kdl and keeps it up to date. Borders can follow the accent as the wallpaper changes. | Borders can follow the accent and change with the wallpaper. | The path is plumbing; the useful half stays |
| Adds Rivet's template to matugen's config, so matugen writes the color file Rivet apps read | Rivet apps then change color with the wallpaper, like the rest of the desktop | Mechanism replaced with the result |
| Shell commands that run when an event happens. Event details are in environment variables: RIVET_EVENT, plus the ones shown on hover (RIVET_PATH, RIVET_STAGE…). | Run a command of your own when one of these happens. | The variable names moved to each event's tooltip, where only someone writing a command looks |

## Too long for the app: belongs in the docs

| Before | After |
| --- | --- |
| A folder with a plugin.conf (name, exec, mode, interval, events) and a program in any language. The program prints what its panel item shows (plain text, or JSON: text, icon, a menu, launcher entries)… | Removed. Writing a plugin is in PLUGINS.md and the docs backlog. |
| Zones: Left starts at the panel's left end, Center sits in the middle, Right ends at its right end. On a narrow screen, items that don't fit are dropped from the inner end. | If items don't fit on a narrow screen, the ones nearest the middle are hidden first. |

## Program names that stay

These name something the person must install or type, so they stay:

- Needs gammastep, hyprsunset or wlsunset. Install one to use Night Light.
- With swaylock, leave out -f.
- Some older apps' icons only show if xembed-sni-proxy is installed.

## Decoration that went

- The "Shell running" badge in the corner of every page. It told the person nothing they needed
  on that page. Checking that things are set up belongs in Rivet Welcome.

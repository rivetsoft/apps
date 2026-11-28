# Changelog

## Unreleased

- **Screen readers and the keyboard**: Orca and other screen readers can read the window's
  buttons, menus, check boxes and text fields, and Tab shows a ring around the control that has
  the keyboard. Shift+F10 opens the right-click menu of the control that has the keyboard. With higher contrast or less motion asked for in the desktop's settings, edges
  and secondary text get stronger and animations stop.

## 0.3.3

- Memory freed by a closed window or pane goes back to the system once all of them have closed,
  and again whenever Rivet Terminal sits idle with memory to spare.

## 0.3.2

- Closing a tab or pane with a long scrollback now gives its memory back to the system, so
  Rivet Terminal shrinks again instead of staying at its largest size.
- Scrollback keeps at most 100,000 lines, the most Preferences offers. A settings file that
  asks for more gets 100,000, which keeps one pane under about half a gigabyte.
- **Block pictures without gaps**: block characters such as ▀ and █ fill their whole cell, so
  pictures drawn with them, like rivetfetch's mascot, show without stripes between the lines.
- An idle window follows desktop color changes right away.

## 0.3.1

- **A new icon**: a green window with a prompt and a yellow cursor, drawn in the website's colors with a plum outline and a hard shadow.
- When Rivet Terminal asks before closing something that is still running, Return closes it
  and Escape cancels.
- In that question, Tab to Cancel and Return now cancels. It used to close anyway.

## 0.3.0

- A Rivet color theme, in Rivet's own pink, blue and yellow.
- A new icon in Rivet's pink, to match the rest of the apps and the website.
- **Move panes and tabs between windows.** Drag a pane by the bar at its top onto another
  pane to swap them, or out of the window: it moves to the Rivet Terminal window you let go
  over, or opens in a window of its own. Tabs pulled down out of the strip move the same way.
  Detach Pane and Move Pane To are in the File menu and a pane's right-click menu, and a tab's
  right-click menu has Detach Tab and Move Tab To. What runs in them keeps running.
- Every Rivet Terminal window is now part of one running program, so they can trade panes and
  tabs. Opening Rivet Terminal again adds a window to it; `--separate` opens one that stays apart.
- File has Close Window, and Quit closes every window.

## 0.2.0

- Drag a tab to move it: it lifts and follows the pointer with a glow, the
  tabs it passes slide aside, and it glides into place when let go. Ctrl+Shift+, and
  Ctrl+Shift+. still move the open one.
- **Faster drawing**: a full screen draws in about a third of the time, a whole row of text at
  a time, with runs of one background as one rectangle. The screen is copied out quickly and
  drawn after, so a program's output keeps streaming in while a frame is drawn (`cat` of a big
  file finishes sooner).
- Columns line up with the font's real advance, so long lines of text stay on the grid.
- The scrollback marker and the bell flash show while a program hides the cursor.

## 0.1.0

The first version.

- Tabs, each with one or more panes in a layout: Tall, Fat, Grid, Side by Side, Stacked, or one
  at a time; new tabs and panes start in the current folder.
- The terminal (`crates/rivet-term`, on alacritty_terminal): 24-bit and 256 colors, bold,
  italic, underlines, strikethrough, wide characters, the alternate screen, mouse reporting,
  bracketed paste, focus reports, program titles, OSC 52 copying, the bell.
- Colors from the desktop and accent, the same as Rivet Edit's code colors (Rivet's default palette
  leaning toward the theme), or any of the shared
  color themes, previewed in the quick panel.
- Selecting (drag, double-click a word, triple-click a line, Alt for a block), copy and paste,
  Ctrl+click links, scrollback with search, clear scrollback.
- Ctrl+Shift keys like kitty's, a command list (Ctrl+Shift+P), a right-click menu, an optional
  menu bar, and Preferences like the other Rivet apps: font, size, cursor, padding, shell,
  scrollback, copy on select, the bell, asking before closing a running program.

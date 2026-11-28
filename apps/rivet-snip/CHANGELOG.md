# Changelog

## Unreleased

- **Screen readers and the keyboard**: Orca and other screen readers can read the window's
  buttons, menus, check boxes and text fields, and Tab shows a ring around the control that has
  the keyboard. Shift+F10 opens the right-click menu of the control that has the keyboard.
  With higher contrast or less motion asked for in the desktop's settings, edges and secondary
  text get stronger and animations stop.

## 0.1.1

- An idle window follows desktop color changes right away.

## 0.1.0

- **Rivet Snip, a new app**: clip and mark up a screenshot, then copy or save it. Crop, draw
  arrows, lines, boxes, ovals and freehand, highlight, type text, place numbered steps, and
  blur what shouldn't be read.
- A compositor can hand it the picture it took (`rivet-snip FILE --replace`, or a PNG on
  standard input). On any other desktop `rivet-snip capture` takes the picture itself.
- Marks can be moved, recolored and removed after they are drawn, with Undo and Redo.

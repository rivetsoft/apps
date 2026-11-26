# Changelog

## 0.1.1

- An idle window follows desktop color changes right away.

## 0.1.0

- **Rivet Snip, a new app**: clip and mark up a screenshot, then copy or save it. Crop, draw
  arrows, lines, boxes, ovals and freehand, highlight, type text, place numbered steps, and
  blur what shouldn't be read.
- A compositor can hand it the picture it took (`rivet-snip FILE --replace`, or a PNG on
  standard input). On any other desktop `rivet-snip capture` takes the picture itself.
- Marks can be moved, recolored and removed after they are drawn, with Undo and Redo.

# Changelog

## 0.1.0

The first version.

- **Rivet Color, a new app**: pick a color from anywhere on the screen and it's copied, in
  hex, RGB, HSL or OKLCH, whichever you chose last.
- **A loupe while you pick**: the screen holds still and a loupe beside the pointer shows the
  pixels around it enlarged, with the color's code under them. Scroll to zoom, and use the
  arrow keys to move one pixel at a time.
- **What goes with a color**: its tints and shades, and its complementary, analogous,
  triadic, split complementary and square harmonies. Click any of them to look at it instead.
- **Contrast**: how readable the color is as text on white and on black, with its WCAG grade.
- **Adjust** a color with hue, saturation and lightness sliders, or type or paste one in any
  format.
- **History**: the colors you pick and copy are kept, the latest first.
- **Palettes**: keep colors together and name them. They're saved as GIMP palettes, which
  GIMP, Inkscape and Krita open too. Your desktop's own colors are there as a palette.
- **In Rivet Shell**: the Color plugin puts the color picked last on the panel. Its menu picks
  a new one and copies recent ones, and a middle-click picks. In the launcher, `@color` lists
  your history and palettes, and `@color` with a color typed after it copies that color in
  your format.
- **`rivet-color pick`** picks and copies with no window, for a key binding (Super+P, say), and a
  notification shows what was copied.

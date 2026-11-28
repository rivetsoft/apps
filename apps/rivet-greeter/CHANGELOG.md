# Changelog

## Unreleased

- **Rivet Greeter, a login screen for greetd**: it is Rivet Shell's lock screen as the screen you
  sign in from, over the wallpaper and in the colors you chose last. It shows whoever changed
  their wallpaper last, and Tab or the arrow keys go to the next account. It runs in cage, Sway
  or Softcore and no longer needs Rivet Shell: it was `rivet-shell greeter` before.
- Run `sudo rivet-greeter --setup` once to make the folder it reads everyone's wallpaper and look
  from; it prints the lines for greetd's settings, and says so when they still start
  `rivet-shell greeter`.
- **Your wallpaper on the login screen from any desktop**: Rivet Shell leaves yours whenever it
  changes. On another desktop, `rivet-greeter sync --wallpaper PICTURE` leaves it with your
  desktop's colors; `--clock` sets the clock's format and `--name` the name shown.

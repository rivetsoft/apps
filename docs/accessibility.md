# Accessibility

Every Rivet app has to work with a screen reader and with the keyboard alone. egui hands its
widgets to AccessKit, which reaches AT-SPI on Linux, so Orca can read a Rivet window the way it
reads a GTK one. That only works when the apps say what their controls are, and most of Rivet's
controls are drawn by hand. This page is the rule each of them follows, and what is shared.

## Turned on everywhere

Each app builds eframe with the `accesskit` feature (the apps turn eframe's default features
off, so it has to be named). AccessKit costs nothing until a screen reader or another assistive
tool asks for the tree: egui only builds it while one is listening.

`rivet_look::a11y` holds what the apps share:

- **`Access`**, a trait on `egui::Response` that names a control drawn by hand and gives it a
  role and a state: `.role(Role::Tab, name)`, `.selected(on)`, `.toggled(on)`,
  `.expanded(open)`, `.described(text)`, and `.tip(text)` for a tooltip that is also the name
  of an icon button.
- **`area(role, name)`**, a `UiBuilder` that names a region (a tab strip, a sidebar, a list).
- **The focus ring.** `a11y::setup`, which `LiveLook::frame` calls, adds an egui plugin that
  draws a ring in the accent around whatever has the keyboard, from the first Tab until the next
  click. `a11y::ring_inside` puts it inside the edge of a large area (Rivet Files' file list).
- **The desktop's wishes.** `a11y::high_contrast()` and `a11y::reduced_motion()` follow the
  desktop portal's Settings (`org.freedesktop.appearance` `contrast` and `reduced-motion`, which
  Softcore and GNOME serve, and GNOME's `enable-animations` and `high-contrast`), else the GTK
  settings files (`gtk-enable-animations`, a high-contrast `gtk-theme-name`). A thread listens
  for changes and redraws. `theme::apply` takes them up: with high contrast every edge is drawn
  and secondary text is close to the main text; with reduced motion egui's animations and smooth
  scrolling stop, and Rivet's own slides (tabs moving into place) end at once. A program with
  its own animation checks `reduced_motion()` too.
  `RIVET_HIGH_CONTRAST=1` and `RIVET_REDUCED_MOTION=1` set them for a screenshot.

## The rule for a new control

egui's own widgets (`Button`, `Checkbox`, `TextEdit`, `Slider`, menus) name themselves from
their text. Anything else that takes a click or the keyboard follows these four steps:

1. **A role.** Say what it is with the AccessKit role a person would expect: `Button`, `Tab`,
   `CheckBox` or `Switch`, `ListBoxOption` (a row in a list you pick from), `TreeItem`,
   `MenuItem`, `Slider`. A control left without one reads as "unknown".
2. **A name.** What a person would call it, in the words on screen: the file's name, the tab's
   label, "Close". An icon button takes its tooltip as its name (`.tip("Close")`). A text field
   or slider without a visible label next to it gets `.accessible_name(…)`; with one, use
   `.labelled_by(label.id)`.
3. **Its state.** `.selected` for the chosen row, tab or place, `.toggled` for anything on or
   off, `.expanded` for something that opens and closes. Set it every frame from the same value
   the drawing uses, so the two can't disagree.
4. **The keyboard.** It's reachable with Tab, or it's part of something that is: a list is one
   Tab stop whose arrows move through its rows, and then the row under the keyboard is the
   list's active descendant (`set_active_descendant`), so a screen reader reads each row
   reached. Enter or Space activates what has focus; egui does this for any control with
   `Sense::click()`. Esc closes what's on top.

A control only for the pointer (a drag handle, the empty space behind a toolbar that opens a
menu) shouldn't be a Tab stop: use `Sense::CLICK` or `Sense::DRAG` without `FOCUSABLE`, and
still give it a role and a name, because a screen reader lists it all the same. Make sure what
it does can be done another way: the toolbar menu from the app's menu, a pane's size from the
keyboard.

## Checks

- `rivet_kit::access::tree` runs a few frames with AccessKit on and gives the tree;
  `access::unlabeled` lists every node a person can click or Tab to that has no name or reads
  as "unknown", with where it is on the window. `crates/rivet-kit/tests/access.rs` draws the
  shared controls and fails on any such node: add a new shared control to it.
- In a live session, make the apps build the tree (`gdbus call --session --dest org.a11y.Bus
  --object-path /org/a11y/bus --method org.freedesktop.DBus.Properties.Set org.a11y.Status
  IsEnabled '<true>'`, or start Orca), then read it with a pyatspi script or Accerciser.

## What AccessKit doesn't carry yet

AccessKit 0.25's AT-SPI side has no expanded or collapsed state, so Orca can't say whether a
folding section is open even though Rivet sets it; the state is in the tree for when it does.
Rivet Shell's panels, dock, launcher and popups are layer-shell surfaces drawn without winit,
so they have no AccessKit adapter yet; its Settings window is an ordinary window and does.

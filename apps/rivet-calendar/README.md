# Rivet Calendar

A calendar kept in plain `.ics` folders. See your days as a month, a week, a day or a list;
make, change and delete events, including ones that repeat; and see them on the panel's
calendar in Rivet Shell.

- **Plain files.** Each calendar is a folder in `~/.local/share/rivet-calendar` with one `.ics`
  file per event, and its name and colour in the files `displayname` and `color`. It's the
  layout vdirsyncer and khal use, so they share the same calendars: vdirsyncer can keep them in
  step with a CalDAV server (Nextcloud, Fastmail, iCloud) until Rivet does it itself.
- **Nothing lost.** An event made elsewhere keeps its alarms, attendees and anything else Rivet
  doesn't show when you change it here.
- **Repeating events.** Every day, weekday, week, month or year from the editor; a rule made in
  another program is kept as it is. Change or delete one occurrence, or all of them.
- **On the panel.** Turn on the Calendar plugin in Rivet Shell's Settings, Plugins: days with
  events get a dot in the panel's calendar, and the chosen day's events are listed under it.
  Click one to open it here.
- **Keys.** T today, Left and Right back and on, D W M A for the views, N for a new event, Esc to
  close the editor.

```sh
cargo run -p rivet-calendar                 # the window
rivet-calendar list 14                      # the next fortnight in the terminal
rivet-calendar --date 2026-12-24            # the window on a day
rivet-calendar --install-desktop-entry      # into the app launcher
```

Checks: `rivet-calendar --screenshot out.png` draws sample calendars around today, never your
own. `RIVET_CALENDAR_VIEW=day|week|month|agenda` picks the view, `RIVET_CALENDAR_EDIT=1` opens
the editor on today's first event.

Accounts (subscribed iCal feeds, CalDAV, Google and Microsoft) are next.

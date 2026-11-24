# Changelog

## Unreleased

- **Rivet Calendar, a new app**: your events by month, week, day or as a list, kept in plain
  `.ics` folders that vdirsyncer and khal share. Make, change and delete events, whole-day and
  repeating ones too, and change a single occurrence or all of them.
- Calendars in the sidebar: click one to show or hide it, right-click to rename or recolor it,
  and + to make another. A small month there marks the days with events.
- **On Rivet Shell's panel**: turn on the Calendar plugin and the panel's calendar marks the
  days with events and lists the chosen day's; click one to open it here.
- `rivet-calendar list` prints the next days' events in the terminal.
- **Subscriptions**: subscribe to a calendar feed (holidays, a team's shared calendar, a
  `webcal://` link) from + beside Calendars. It's read again every 6 hours, while the window
  or the panel's Calendar plugin runs; right-click it to refresh now or unsubscribe.
- Open a `webcal://` link or an `.ics` file with Rivet Calendar to subscribe to it, or to add
  its events to a calendar of yours.
- Right-click a calendar to move it to the Trash (it can be put back from there).
- **CalDAV accounts**: add Nextcloud, Fastmail, iCloud, Radicale or another CalDAV server from
  + beside Calendars. Its calendars sync both ways, every 5 minutes and a few seconds after you
  change something; when an event changed on both sides, the server's is kept. The password is
  kept in your keyring. Right-click one of its calendars to sync now or remove the account.
- **Google and Microsoft accounts**: + beside Calendars, then Add a Google Account or Add a
  Microsoft Account, and sign in in your browser. Their calendars sync both ways like a CalDAV
  account's; calendars shared with you to look at stay read-only. You can sign in with a client
  ID you registered yourself instead of Rivet's.
- An event that's in more than one calendar (a meeting in several colleagues' calendars) shows
  once, marked in each calendar's colour; point at it to see which calendars it's in. The
  panel's calendar lists it once too.
- Large calendars (tens of thousands of events) open without freezing the window: they're read
  in the background, and the views stay quick while a sync is bringing events down.

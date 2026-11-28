# App actions: letting AI work inside the Rivet apps

**Status: a design, nothing built.** `rivetctl mcp` (layer 1) already lets an AI assistant
install, update and remove the apps. This document is layer 2: an assistant doing things inside a
running app, such as opening a folder in Rivet Files, adding a task in Rivet Notes or making an
event in Rivet Calendar. It's written so the first app can adopt it without another design round.

## What we want

- An app chooses what it offers. Nothing is reachable by default, and every action is written on
  purpose, with a name, a description and typed arguments.
- The same actions serve everyone: the AI through MCP, Rivet Shell's launcher, scripts and key
  bindings. No channel is private to AI.
- The person stays in charge. Anything that deletes, overwrites, sends or spends asks them in the
  app's own window, and an assistant can't answer for them.
- Every app still works alone. With no rivetctl and no session bus, nothing breaks; actions are
  missing quietly.

## One D-Bus interface per app

Each app that opts in owns a well-known name on the **session bus** and exports one object with
one interface:

| | |
| --- | --- |
| Bus name | `io.github.rivetsoft.<App>`, such as `io.github.rivetsoft.RivetNotes` |
| Object | `/io/github/rivetsoft/<App>/Actions` |
| Interface | `io.github.rivetsoft.Actions1` (versioned in the name, as freedesktop interfaces are) |

The session bus is the standard way programs in one sign-in talk (AGENTS.md: never invent a
private channel where a standard one exists). It already gives us activation, a single instance
(Rivet Welcome's window uses the same pattern), and the caller's process id for the security
checks below. Every app speaks the same interface, so there's one bridge for every app rather
than one per app.

### Methods

```
List() -> s                        JSON: the actions, as described below
Describe(s name) -> s              JSON: one action
Invoke(s name, s args_json, s request_id) -> s
                                   JSON: { "status": "done" | "refused" | "needs-confirmation"
                                           | "failed", "result": …, "message": "…" }
Cancel(s request_id)
```

```
signal Progress(s request_id, d fraction, s label)
signal Finished(s request_id, s outcome_json)
signal ActionsChanged()           a vault was opened, a document closed…
```

Arguments and results travel as JSON strings rather than D-Bus variants. The schemas are JSON
Schema anyway (that's what MCP carries), so the app checks one schema with one library and the
bridge forwards it untouched. D-Bus stays the transport and the place where identity is checked.

### An action

```json
{
  "name": "notes.add_task",
  "title": "Add a task",
  "description": "Add a task to today's daily note in the open vault.",
  "input": { "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"] },
  "output": { "type": "object", "properties": { "note": { "type": "string" } } },
  "effect": "change",
  "needs_window": false
}
```

`effect` is one of:

- `read`: looks only, such as `files.selection` or `calendar.events_between`.
- `change`: makes something new or edits it in a way that's easy to undo, such as
  `notes.add_task` or `files.new_folder`.
- `destructive`: deletes, overwrites, sends, moves outside the app's own folders, runs a
  program, or can't be undone, such as `files.delete`, `ftp.upload` or `terminal.run`.

The bridge turns these into MCP's `readOnlyHint` and `destructiveHint`. The app is the one that
enforces them; the hints only help the assistant ask well.

## Opt-in, per app and per person

1. **The app opts in in code.** An app exports the interface only once it has written actions.
   A shared crate, `rivet-actions`, would hold the interface, schema checks, the confirmation
   dialog and the rate limits, so each app writes only its actions (as `rivet-install` holds
   what Welcome and rivetctl share).
2. **The person opts in per app.** A new key, `ai_actions=off|read|ask|on`, in each app's own
   settings file, off by default, shown in the app's Preferences:
   - `off`: the interface isn't exported at all.
   - `read`: `read` actions only.
   - `ask`: everything; `change` and `destructive` actions ask in the window each time.
   - `on`: `change` runs; `destructive` still asks every time. There's no setting that skips
     the question for a destructive action.
3. **rivetctl opts in to bridging** each app separately (`rivetctl mcp --apps notes,calendar`, or
   a list in its own settings), so an assistant set up to install apps doesn't also reach into
   them unless the person adds them.

## Confirmation

A `destructive` action, or a `change` under `ask`, never runs from the call alone:

1. `Invoke` answers `needs-confirmation` at once, with a sentence saying what will happen
   ("Delete 3 files from Downloads. They go to the trash.").
2. The app shows that sentence in its own window, with buttons naming the action
   ("Delete Files" / "Keep Them"), following `rivet-copy`'s rules for confirmations. A running
   app raises its window. An app that isn't running starts with only that dialog.
3. `Finished` reports what the person chose. Through MCP, the tool call stays open until then,
   with progress notifications. A cancelled or timed-out question (two minutes) counts as no.

The question is drawn by the app, not by the assistant or the bridge, so a caller can't fake the
person's yes. The `confirm` argument `rivetctl mcp` uses for installs is a lighter rule for a
lighter risk: it stops an assistant acting by accident, and doesn't stand in for this dialog.

## What `rivetctl mcp` bridges

`rivetctl mcp` stays the one MCP server for Rivet. Beside its install tools it would:

- On `tools/list`, call `List()` on every opted-in app that's running or can be started by bus
  activation, and list each action as an MCP tool named after it (`notes.add_task`), with its
  schemas and hints.
- Follow `ActionsChanged`, and send MCP's `notifications/tools/list_changed`.
- Forward `tools/call` to `Invoke`, turn `Progress` into MCP progress notifications, and wait
  on `Finished` for a confirmation.
- Not hold any of the apps' data and never call anything but the interface. Everything the
  bridge does, a person could do with `busctl --user call`.

Other MCP clients could talk to the bus themselves; the bridge is a convenience, not the gate.
The gate is in each app.

## Security model

**Who's trusted.** Programs running as this person in this sign-in can already read their files,
so the session bus isn't a wall between them. The risk to design for is an assistant (or a
prompt injected into what it reads) asking for more than the person meant, and that's what the
settings and the confirmation dialog govern. Other people on the computer, and anything over the
network, never reach the session bus.

**What every app enforces, in `rivet-actions`:**

- The schema, before the action runs. Unknown arguments are refused, not ignored.
- The setting: an action above the allowed `effect` is refused with a sentence saying where to
  change that.
- Paths are made canonical, and stay inside what the action declares (the open vault, the
  calendar folders, the folder Files is showing) unless the action is `destructive` and asks.
- A limit on how often each caller can invoke, so a looping assistant can't flood the person
  with questions; past it, `refused` with a sentence saying why.
- A log of every call made from outside the app, with the caller's process name, the
  action and the outcome, kept for a week. Preferences shows it ("Recent actions"), so the
  person can see what an assistant did.

**What's never an action.** Reading passwords, sign-in tokens or the keyring; changing an app's
AI setting; the confirmation dialog's answer; running arbitrary programs, except Terminal's own
`destructive` action, which shows the command in full before asking.

**Who's calling.** The app asks the bus daemon for the caller's process id
(`GetConnectionUnixProcessID`) and records its program for the log. It's informative, not a
permission: anything in the sign-in can pretend to be anything else, which is why the dialog
matters and why there's no allow-list by program name.

**Under Flatpak.** Rivet Files' Flatpak would ask to own its name on the session bus
(`--own-name=io.github.rivetsoft.RivetFiles`). A caller in another sandbox needs
`--talk-name` for it, which the person grants per app.

## Where to start

1. `crates/rivet-actions`: the interface, the settings key, schema checks, the dialog, the log.
2. Rivet Notes first: `notes.search` (read), `notes.open` (change, opens the note in the
   window), `notes.add_task` (change). It has the most to gain and nothing destructive.
3. The bridge in `rivetctl mcp`, tested with a scripted MCP session against Notes, like
   layer 1's.
4. Files and Calendar next, then the rest as each has actions worth offering.

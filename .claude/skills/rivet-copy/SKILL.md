---
name: rivet-copy
description: Write or review the words inside Rivet's apps, such as setting descriptions, hints, tooltips, empty states, errors, confirmations, and button and menu labels. Use it whenever you add or change a string a person will read in any Rivet app, and when asked to audit a screen's wording.
---

# Rivet copy

Every string in a Rivet app is there on purpose. A description exists only because the label leaves
a real question open, and it answers that question in one sentence. Anything longer goes to the
docs.

## Decide first: does it need words at all?

Ask these in order, for every label, hint and tooltip:

1. **Does the label already say it?** If so, write nothing. Most settings need no description.
   "Blur the wallpaper" is complete; "Blurs the wallpaper behind the lock screen" adds nothing.
2. **What would a person looking at this ask?** For example "What happens if I turn this on?",
   "Why can't I click this?" or "What do I do now?". Answer that question and nothing else.
3. **Does the answer need more than one sentence?** Then the one sentence stays in the app and the
   rest goes to the docs (see the last section).

## Rules

1. **One sentence.** Add a second only for a consequence the person must know before acting:
   something is deleted, overwritten, sent, restarted or can't be undone.
2. **Write from the person's side of the screen.** Say what changes for them, not how the code does
   it. "Unlock with the password you log in with", not "Your password is checked with PAM".
3. **Leave out the machinery.** That means file paths, config file names, environment variables,
   setting keys, service and protocol names (PAM, D-Bus, SNI, layer-shell, xdg-portal), JSON, and
   internal ids. The one exception is something the person must install, type or paste. Name that
   as the action: "Install gammastep, hyprsunset or wlsunset to use Night Light."
4. **Be concrete.** Use real numbers ("15 seconds"), real places in the app ("in quick settings",
   "on the Panels page") and real results ("the screens go back to how they were").
5. **For a toggle, describe what happens when it's on.** Mention off only when it isn't the obvious
   opposite.
6. **Errors say what happened, then what to do.** Never show a raw error on its own; if the detail
   helps someone report a bug, put it after a plain sentence.
7. **Empty states say what will appear here, plus the one action that fills it.** "No connections
   yet. Add one in Rivet FTP."
8. **Confirmations:** the title is the action, the body is the consequence, and the buttons name
   the action ("Delete Vault" / "Keep It"). Never "OK" or "Yes".
9. **Features that only work in some places:** where possible, hide or disable the control where
   it can't work. When it has to show, use one sentence saying where it works and what to do
   instead.

## Rivet's voice

These come from AGENTS.md, "Words in the interface".

- Plain, calm and direct. No exclamation marks, and no "simply", "just", "easily", "please" or
  "we".
- Labels and descriptions use sentence case; buttons and menu items use Title Case. Descriptions
  are full sentences ending with a period; labels and buttons have no period.
- One name per thing across every app: vault, connection, panel, item, group, plugin, command (a
  launcher `@word`), quick settings, launcher, dock, wallpaper, accent. Never introduce a synonym;
  if a new thing needs a name, choose one and use it everywhere.

## Tells to strip

These are adapted from the stop-slop skill (Hardik Pandya) and the humanizer skill (Siqi Chen),
both MIT-licensed. Text with these patterns reads as machine-written, which in interface text
means filler.

- **Not X but Y.** "Not just a dock, it's your workflow." State Y directly, or cut the line.
- **Triads for rhythm.** "Fast, simple and powerful." Keep three items only when there really are
  three.
- **Hype words:** powerful, beautiful, seamless, smart, modern, effortless, intuitive.
- **Adverbs and hedges:** really, actually, easily, seamlessly. Use "automatically" only when there
  is also a manual way.
- **Dashes joining clauses.** Use a period, comma or colon instead.
- **Passive voice that hides who acts.** "Colors are generated" becomes "matugen makes the colors
  from your wallpaper".
- **Lists stuffed into parentheses.** "(name, exec, mode, interval, events)" is reference material,
  so it belongs in the docs.
- **Restating the label, or announcing the setting.** "This option lets you…", "Use this to…".
- **Labels that are clever instead of plain.**

## Length budgets

| Element | Budget |
| --- | --- |
| Button, menu item | 1 to 3 words, starting with a verb |
| Label, checkbox, toggle | Up to about 8 words; the label is the description when it can be |
| Page subtitle | One line, up to about 70 characters |
| Description or hint under a control | One sentence, up to about 100 characters |
| Tooltip | One short sentence, up to about 80 characters, never repeating what's visible |
| Empty state | One sentence and one action |
| Error | What happened and what to do, at most two short sentences |

Going over the budget is the sign that something belongs in the docs.

## Where longer explanations go

Rivet's user docs aren't written yet; `docs/guide-backlog.md` is what they will be written from.

- Keep the one sentence in the app.
- Add the full explanation to `docs/guide-backlog.md`, under the app and the page, so nothing you
  cut from the app is lost. Mark it with the string's location (`file.rs` and the label).
- Until the docs are published, don't add "see the docs" text or a link. The "Learn more" link
  comes with them, and the backlog shows where each link goes.

## How to work

1. **Find the strings.** In egui code, look at `ui.label`, `hint(`, `note(`, `on_hover_text`,
   `checkbox`, `button`, `toggle(`, `section(`, `hint_text` and `format!` strings passed to them.
   The keywords in `search.rs` are never shown to anyone, so leave them alone.
2. **Confirm the behavior** by reading the code path. Never describe something you haven't
   confirmed; if you can't confirm it, say so.
3. **Draft a table** with the columns Where | Before | After | Why. Keep Why to a few words
   ("restated label", "machinery", "moved to docs"). Below it, list term decisions and anything you
   couldn't confirm.
4. **Apply the changes.** Keep `format!` placeholders and any conditional meaning (what a string
   says on one compositor and not another).
5. **Look at it.** Run `cargo build`, then take a screenshot of the page (`RIVET_SHELL_PAGE`,
   `RIVET_SHELL_SCROLL` and the other apps' hooks). Read the page as someone seeing it for the
   first time, and cut any line that doesn't answer a question they'd have.
6. **Add a changelog bullet** under `## Unreleased` if a person would notice the change.

## Before you finish

- Every description passes "does the label already say it?"
- No string is over its budget; the extra is in `docs/guide-backlog.md`.
- No machinery words, except things the person must install or type.
- Terms match the vocabulary list.
- No dashes joining clauses, no triads, no not-X-but-Y, no hype words.

`references/examples.md` has real before-and-after examples from Rivet.

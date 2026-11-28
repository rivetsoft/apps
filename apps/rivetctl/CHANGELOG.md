# Changelog

## Unreleased

- **rivetctl, a new program**: install, update and remove the Rivet apps from the terminal. Run
  it on its own for a full-screen list of every app by category, with what's installed and
  what the stable, alpha and edge channels offer, and each install's progress.
- Commands for scripts: `list`, `install`, `update`, `remove`, `channel` and `doctor`, with
  `--json` for their output.
- Give one app a channel of its own with `rivetctl channel notes edge`. Rivet Welcome follows it
  too, and doesn't move that app back.
- `rivetctl build fetch@main` builds an app from source at a branch, tag or commit and installs
  it.
- `rivetctl doctor` says what's missing for the apps to run here, such as libmpv for Rivet
  Cinema, and what to install.
- `rivetctl mcp` lets an AI assistant see the apps and install, update or remove them. It asks
  for a confirmation before anything changes.

# Releasing

How a Rivet app gets from its changelog to a download in Rivet Welcome and on the website. Each
app is released on its own, with its own version, changelog and tags, from this one repository.
Softcore is released from its own repository and has no AppImage; it isn't covered here.

## What a release is

| Thing | Rivet Files | Every other app (Notes here) |
| --- | --- | --- |
| Tag | `v0.10.0` | `rivet-notes-v0.4.1` |
| GitHub release | "Rivet Files 0.10.0", the repository's latest | "Rivet Notes 0.4.1", never the latest |
| Portable build | `rivet-files-0.10.0-x86_64-linux.tar.gz` | `rivet-notes-0.4.1-x86_64-linux.tar.gz` |
| AppImage | `Rivet_Files-0.10.0-x86_64.AppImage` | `Rivet_Notes-0.4.1-x86_64.AppImage` |
| Checksums | a `.sha256` beside each file | the same |
| Source | GitHub's "Source code" archives at the tag | the same |
| Notes | the README's first screenshot, the version's section of `CHANGELOG.md`, then how to install | the same |

Notes, Edit, Terminal and Files are full releases. The rest are pre-releases, which Rivet
Welcome badges "Alpha" and the website calls alpha: `STABLE` in
[`utilities/lib/apps.sh`](../utilities/lib/apps.sh) decides. [stability.md](stability.md) is
the bar an app meets to become stable, and how to promote it.

The portable build is the program linked for glibc 2.31, with its desktop file, icon, README,
changelog, licenses and an `install.sh`. **Rivet Welcome installs from it**, so its contract is:

- releases in `rivetsoft/apps`, read from GitHub's releases list, where drafts don't show;
- the tag is the app's prefix and a version (`rivet-notes-v` + `0.4.1`);
- an asset named `<dist>-<version>-<arch>-linux.tar.gz` and the same name with `.sha256`;
- an `install.sh` inside that puts the program in `~/.local/bin`;
- the program answers `--version` with the release's version.

Every name comes from the table in `utilities/lib/apps.sh`. Rivet Welcome keeps the same names
in [`catalog.rs`](../apps/rivet-welcome/src/catalog.rs), and its test
`every_released_app_is_known` fails when an app is in one and not the other. Rivet Welcome also
finds apps it doesn't know from their tags, but only when the program is named like the tag
(`rivet-clock`), so add every new app to both.

## Releasing an app

1. **As you work**, add notes under `## Unreleased` in the app's `CHANGELOG.md` (see
   [AGENTS.md](../AGENTS.md)).
2. **Choose the version.** `utilities/rivet-dev apps` shows each app's version, last tag and the
   changes since. Before 1.0: `minor` for something new a person can do, `patch` for fixes and
   small changes. A first release keeps the version in `Cargo.toml` (usually `0.1.0`).
3. **Bump in a pull request.** On a branch, `utilities/rivet-dev bump notes patch` (or `minor`,
   `major`, `X.Y.Z`) sets the version in `Cargo.toml` and `Cargo.lock`, turns `## Unreleased` into
   `## 0.4.1` and commits "Release Rivet Notes 0.4.1". Several apps can be bumped in one pull
   request. CI checks it; merge it.
4. **Ship.** On `main`, up to date with GitHub: `utilities/rivet-dev ship notes` (several apps, or
   `all` for every app whose version isn't tagged yet; `--dry-run` first to see the list). It
   checks the changelog has the version's section, tags the commit and pushes the tag.
5. **GitHub builds it.** The tag starts [the Release workflow](../.github/workflows/release.yml)
   on Ubuntu 22.04. It checks the tag matches `Cargo.toml` and is on `main`, runs
   `utilities/rivet-dev release notes` (the same build as on a computer), then does what Rivet
   Welcome will do: checks the sums, unpacks the portable build, runs its `install.sh` into a
   throwaway home and asks the program its version, and runs the AppImage. Then it makes a
   **draft** release with the files and the notes. `gh run list --workflow release.yml` follows it.
6. **Look at the draft** on the releases page: the notes, the four files. Nothing outside sees it
   yet.
7. **Go live.** `utilities/rivet-dev live notes` (or `all`) publishes each draft for the version
   on `main`. Rivet Files becomes the latest release, and the others are pre-releases or full
   releases as `apps.sh` says. Then it starts the website's rebuild
   (`gh workflow run deploy.yml -R rivetsoft/rivetsoft.github.io`). The website also rebuilds
   every hour. Publishing as a person, rather than by the workflow, is what starts Rivet Files'
   AUR and Flatpak workflows, which only run when a release is published.

Rivet Welcome offers the update at its next check (at sign-in and every few hours, or Check for
Updates).

### A rehearsal

Actions → Release → Run workflow, with an app's name, builds that app's downloads from the branch
chosen and keeps them as the run's artifacts. No tag, no release. On a computer,
`utilities/rivet-dev release notes` builds the same files in `dist/` (and tags locally; delete
the tag with `git tag -d` if you don't ship it).

### When something goes wrong

- **The build failed**: fix it on `main` in a pull request, then delete the tag
  (`git push origin :refs/tags/rivet-notes-v0.4.1 && git tag -d rivet-notes-v0.4.1`), and ship
  again. A tag nobody has published yet can move; a published one never does. Bump instead.
- **The draft is wrong** (notes, files): delete the draft on GitHub, fix, and ship again as above.
- **Already published and wrong**: release the next patch version.

### Without GitHub Actions

`utilities/rivet-dev release notes --publish` builds on your computer, tags, pushes and publishes
in one go, and `release-all` walks through every app the same way. They make the same files with
the same names; prefer the workflow, which builds every release on the same system.

## Edge builds

An edge build is an app as it is on `main`, for people who want the newest code every day
through Rivet Welcome's Edge channel (Settings → Versions → Edge). It isn't a release: no
version is bumped, nothing goes in the changelog, and the website never lists it.

| Thing | An edge build of Rivet Notes |
| --- | --- |
| Version | `0.4.3-edge.20261127.gabc1234`: the next patch version, the commit's day (UTC) and the commit. `--version` says it, so you can tell exactly which build is installed. |
| Where | One rolling GitHub pre-release per app, tagged `rivet-notes-edge` (`rivet-files-edge` for Files), never the latest. Each build replaces the last one's files. |
| Files | The portable archive and the AppImage with their `.sha256`, named like a release's: `rivet-notes-0.4.3-edge.20261127.gabc1234-x86_64-linux.tar.gz`. |
| Notes | The commit, the day, and the app's `## Unreleased` changelog notes. |

[The Edge builds workflow](../.github/workflows/edge.yml) runs every night at 07:23 UTC. It
builds each app whose folder, crates, `Cargo.toml` or `Cargo.lock` changed since its last edge
build (`utilities/rivet-dev edge-changed` lists them), with the same release scripts and checks
as the Release workflow, then publishes it. Actions → Edge builds → Run workflow builds the apps
named, or every changed app, at once.

On a computer, `utilities/rivet-dev edge notes` builds the same files in `dist/edge/` without
touching your branch (it works in a throwaway worktree), and `--publish` puts them in
`rivet-notes-edge`.

Edge tags have no `-v` in them, which keeps them out of everything that reads releases by
`PREFIX` + version: the Release workflow, the AUR and Flatpak workflows, the website, and
Rivet Welcome 0.4.0 and older (which never offer an edge build). Later versions look for
`DIST-edge` by name, take the version from the archive's name, and tell edge builds apart by
commit. When someone goes back from Edge to Stable or Alpha, the release replaces the
edge build, though its number is lower.

## Holding an app back

An app that is built and listed but not ready to go out has `HOLD=1` on its line in
`app_select` (`utilities/lib/apps.sh`). Rivet Long Shot is held this way.

- `utilities/rivet-dev release-all`, `ship all` and `live all` pass over it, and say so.
- Naming it still works: `ship longshot`, `live longshot`, `release longshot`, or `release-all --only longshot`.
- Rivet Welcome knows it (`held: true` in `catalog.rs`) but never offers it, even when a
  release of it turns up. Its test `every_released_app_is_known` checks that the apps held
  there are the ones with `HOLD=1`.
- CI still builds its portable release, so it keeps building the way a release would.

To let it go out, remove `HOLD=1` from `apps.sh` and `held: true` from `catalog.rs` in the
same pull request, with a line in Rivet Welcome's changelog.

## Adding a new app to the releases

1. Give it `data/<dist>.desktop` and `data/<dist>.svg`, a `CHANGELOG.md` with `## Unreleased`,
   and `--version` and `--install-desktop-entry` options (as the other apps have).
2. Add it to `ALL_APPS` and `app_select` in `utilities/lib/apps.sh`, with `HOLD=1` until it's
   ready to go out (see above), and to the CI's portable-release list.
3. Add it to `known()` in Rivet Welcome's `catalog.rs`, with a changelog line in Rivet Welcome.
4. Add it to the website's `src/data/apps.ts` in rivetsoft/rivetsoft.github.io.

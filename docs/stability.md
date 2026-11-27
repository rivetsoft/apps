# From alpha to stable

Every Rivet app starts as an alpha: its releases are GitHub pre-releases, which Rivet Welcome
badges "Alpha" and the website calls alpha. An app becomes stable when it meets the bar below,
and from then on its releases are full releases. Rivet Welcome offers only the full releases of
a stable app unless "Offer alpha versions of stable apps" is on in its Settings, so a stable
app's alphas can keep coming without reaching everyone.

`STABLE` in [`utilities/lib/apps.sh`](../utilities/lib/apps.sh) is the one switch: with it,
`rivet-dev release` and the Release workflow publish the app's releases as full releases;
without it, as pre-releases.

## The bar

An app is stable when all five hold. Each says how to check it.

1. **Real screenshots.** The app's README, its website page and its release notes show the app
   as it is now, made by its screenshot hook (`--screenshot`) with tidy sample data, in the
   pink scheme and at the size of the other apps' pictures. No placeholders, nothing personal.
   Check: build, rerun the hook, and compare with the pictures in `docs/screenshots`.
2. **A docs page.** The website has a page for the app under `/docs/` that covers what a person
   does with it and what to do when something goes wrong, and calls nothing "coming soon" that
   is already there. Check: read it against the app's README and changelog.
3. **No known data-loss bugs.** No open issue, and nothing in the app's README, where the app
   can delete, overwrite or damage a person's files or settings, or leave them with nothing
   working after a failed update. Check: search the issues for the app's name with "lost",
   "deleted", "overwrote" and "corrupt".
4. **An end-to-end or smoke test in CI.** A test that runs the built program the way a person
   would, not only unit tests, on every pull request that touches the app or the crates it
   uses. Check: the workflow exists, is path-filtered to the app, and passed on the release
   commit.
5. **A week with no regressions.** The release has been out as a pre-release for seven days,
   with no regression reported against it (an issue, or a fix in the next changelog for
   something the release broke). Check: the release date on GitHub, and the issues opened
   since.

The last one is why an app's first stable version goes out first as a pre-release: the week is
counted from the day that pre-release went live.

## Promoting an app

When the week is over and nothing came up, the same version becomes stable. Nothing is rebuilt.

1. **Flip the release.** `gh release edit rivet-NAME-vX.Y.Z -R rivetsoft/apps --prerelease=false
   --latest=false` (`--latest=false` keeps Rivet Files' release the repository's latest).
   Rivet Welcome drops the "Alpha" badge at its next check, and people who had the pre-release
   already have the stable version, so nothing downloads again.
2. **Make it stick, in a pull request.** Add `STABLE=1` to the app's line in `apps.sh`, move
   the app from the pre-releases to the full releases in `utilities/rivet-dev`'s help for
   `release --publish` and in [releasing.md](releasing.md), change the app's README where it
   says alpha, and move it in the table below. Without `STABLE=1`, the app's next release
   would go out as a pre-release again.
3. **Rebuild the website.** `gh workflow run deploy.yml -R rivetsoft/rivetsoft.github.io`, or
   wait for the hourly rebuild. The badge on the app's page comes from the release's
   pre-release flag, so it clears then. Change the page's own words where they call the app
   alpha.

If a regression turns up during the week, fix it, release a patch version as a pre-release, and
count the week again from that release.

## Where each app is

| App | Status |
| --- | --- |
| Rivet Files, Notes, Edit, Terminal | Stable |
| Rivet Welcome | Alpha. 0.4.0 is the stable candidate: screenshots, the end-to-end test (`rivet-dev welcome-e2e`, in CI) and the data-loss fixes are in. It's promoted after its week, once the website's Welcome docs page is up to date. |
| Rivet FTP, Color, Shell, Snip, Vice, Calendar, Fetch | Alpha |

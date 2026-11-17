#!/bin/sh
# Bump the version for a new release.
#
#   utilities/rivet-dev bump [APP]            patch: 0.1.1 -> 0.1.2
#   utilities/rivet-dev bump [APP] minor      0.1.1 -> 0.2.0
#   utilities/rivet-dev bump [APP] major      0.1.1 -> 1.0.0
#   utilities/rivet-dev bump [APP] 0.3.0      an exact version
#
# APP is files (the default), ftp, notes, edit or terminal (rivet-dev passes it as RIVET_APP).
# Updates the app's Cargo.toml and Cargo.lock, adds the release to the app metadata (Rivet
# Files), starts a CHANGELOG.md section from the commit messages since the app's last release
# tag, and commits it all as "Release <App> X.Y.Z".
# Edit the changelog afterwards if you like (then `git commit --amend -a --no-edit`),
# and publish with utilities/rivet-dev release --publish
set -e
cd "$(dirname "$0")/../.."
. ./utilities/lib/apps.sh
app_select "${RIVET_APP:-files}"

if [ -n "$(git status --porcelain)" ]; then
    echo "Commit your changes first." >&2
    exit 1
fi

OLD=$(app_version)
MAJOR=$(echo "$OLD" | cut -d. -f1)
MINOR=$(echo "$OLD" | cut -d. -f2)
PATCH=$(echo "$OLD" | cut -d. -f3)
case "${1:-patch}" in
    patch) NEW="$MAJOR.$MINOR.$((PATCH + 1))" ;;
    minor) NEW="$MAJOR.$((MINOR + 1)).0" ;;
    major) NEW="$((MAJOR + 1)).0.0" ;;
    [0-9]*.[0-9]*.[0-9]*) NEW="$1" ;;
    *) echo "Usage: utilities/rivet-dev bump [files|ftp|notes|edit|terminal] [patch|minor|major|X.Y.Z]" >&2; exit 1 ;;
esac
TAG="$TAG_PREFIX$NEW"
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    echo "$TAG already exists." >&2
    exit 1
fi

# Cargo.toml: only the package's own version line (the first one).
sed -i "0,/^version = \"$OLD\"/s//version = \"$NEW\"/" "$APP/Cargo.toml"
# Cargo.lock: record the new version without touching any dependency.
cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null

# CHANGELOG: a hand-written "## Unreleased" section becomes this version's section;
# without one, a new section is made from the commit subjects since the last tag.
if grep -q '^## Unreleased' "$APP/CHANGELOG.md"; then
    sed -i "0,/^## Unreleased.*/s//## $NEW/" "$APP/CHANGELOG.md"
else
    # The app's own last release (other apps' tags don't count).
    LAST=$(git describe --tags --abbrev=0 --match "$TAG_PREFIX[0-9]*" 2>/dev/null || true)
    RANGE=${LAST:+$LAST..}HEAD
    NOTES=$(git log --no-merges --format='- %s' $RANGE)
    [ -n "$NOTES" ] || NOTES="- Maintenance release"
    {
        sed -n '1,2p' "$APP/CHANGELOG.md"
        printf '## %s\n\n%s\n\n' "$NEW" "$NOTES"
        sed -n '3,$p' "$APP/CHANGELOG.md"
    } > "$APP/CHANGELOG.md.new"
    mv "$APP/CHANGELOG.md.new" "$APP/CHANGELOG.md"
fi

# App metadata (software centers show these): add this release at the top of <releases>.
META=$APP/packaging/flatpak/io.github.ulinaaron.rivet-files.metainfo.xml
if [ -f "$META" ] && ! grep -q "<release version=\"$NEW\"" "$META"; then
    sed -i "0,/<releases>/s//<releases>\n    <release version=\"$NEW\" date=\"$(date +%Y-%m-%d)\"\/>/" "$META"
fi

git add "$APP/Cargo.toml" Cargo.lock "$APP/CHANGELOG.md"
[ -f "$META" ] && git add "$META"
if [ "$APP_ID" = files ]; then
    git commit -q -m "Release $NEW"
else
    git commit -q -m "Release $TITLE $NEW"
fi
echo "Bumped $TITLE $OLD -> $NEW and committed. Review $APP/CHANGELOG.md, then run utilities/rivet-dev release $APP_ID --publish"

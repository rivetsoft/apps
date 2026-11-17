#!/bin/sh
# Make a release of Rivet FTP or Rivet Notes (Rivet Files' own release is release.sh).
#
#   utilities/rivet-dev release notes            build the archive in dist/ and tag the current
#                                                commit rivet-notes-vVERSION — on this computer only
#   utilities/rivet-dev release notes --publish  the same, then send the code and the tag to GitHub
#                                                and create a GitHub pre-release with the files
#
# The archive holds the program (linked for an old glibc, so it runs on older systems too), its
# desktop file and icon, README, CHANGELOG, licenses, and install.sh. These apps are released as
# GitHub pre-releases, so the repository's "latest release" stays Rivet Files'.
set -e
cd "$(dirname "$0")/../.."
. ./utilities/lib/apps.sh
app_select "${RIVET_APP:?which app}"
[ "$APP_ID" != files ] || { echo "Rivet Files is released by release.sh." >&2; exit 1; }

VERSION=$(app_version)
ARCH=$(uname -m)
NAME="$DIST_NAME-$VERSION-$ARCH-linux"
TAG="$TAG_PREFIX$VERSION"

if [ -n "$(git status --porcelain)" ]; then
    echo "Commit your changes first (the release is built from the current commit)." >&2
    exit 1
fi
. ./utilities/lib/publish.sh
# Publishing: take in GitHub's newer commits first (merged, never rebased: see publish.sh).
if [ "$1" = "--publish" ]; then
    sync_with_github
fi
grep -q "^## $VERSION" "$APP/CHANGELOG.md" || { echo "$APP/CHANGELOG.md has no section for $VERSION (run utilities/rivet-dev bump $APP_ID)." >&2; exit 1; }

# A published version never moves to a different commit; a release whose tag is on an earlier
# commit of this branch (publishing cut short) is finished instead of rebuilt.
HEAD_COMMIT=$(git rev-parse HEAD)
check_tag || { echo "Run utilities/rivet-dev bump $APP_ID, then this again." >&2; exit 1; }
NOTES="dist/notes-$DIST_NAME-$VERSION.md"
if [ -n "$RESUME" ]; then
    if [ ! -f "dist/$NAME.tar.gz" ] || [ ! -f "$NOTES" ]; then
        echo "$TAG is already made, but its files aren't in dist/ any more." >&2
        echo "Rebuild them from the tag (git checkout $TAG; utilities/rivet-dev release $APP_ID; git checkout main), then run this again." >&2
        exit 1
    fi
    echo "$TAG was made earlier; finishing it (nothing is rebuilt)."
else
RIVET_PKG=$PKG ./utilities/lib/portable-build.sh
BIN_PATH=$(RIVET_PKG=$PKG ./utilities/lib/portable-bin.sh)
ROOT=$(pwd)
# shellcheck disable=SC1091
. ./utilities/lib/ensure-zigbuild.sh
mkdir -p dist
STAGE="dist/$NAME"
rm -rf "$STAGE"
mkdir -p "$STAGE"
strip -o "$STAGE/$BIN" "$BIN_PATH"
./utilities/lib/check-glibc.sh --max "$GLIBC_MAX_VERSION" "$STAGE/$BIN"
cp "$APP/data/$DIST_NAME.desktop" "$APP/data/$DIST_NAME.svg" "$APP/README.md" "$APP/CHANGELOG.md" LICENSE-MIT LICENSE-APACHE "$STAGE/"
sed "s/@BIN@/$BIN/g; s/@TITLE@/$TITLE/g" utilities/lib/install-app.sh > "$STAGE/install.sh"
chmod +x "$STAGE/install.sh"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
rm -rf "$STAGE"
(cd dist && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")

awk -v v="## $VERSION" '$0 ~ "^## " { p = (index($0, v) == 1) } p' "$APP/CHANGELOG.md" | tail -n +2 > "$NOTES"
cat >> "$NOTES" <<EOF

### Install

$TITLE is an early alpha: keep backups of what you use it with.

\`\`\`sh
tar xzf $NAME.tar.gz
cd $NAME
./install.sh          # $BIN in ~/.local/bin, and $TITLE in your app launcher
\`\`\`

Portable $ARCH build, linked against glibc $GLIBC_LINK_VERSION. Or build from source: \`cargo build --release -p $PKG\`.
EOF

git rev-parse -q --verify "refs/tags/$TAG" >/dev/null || git tag -a "$TAG" -m "$TITLE $VERSION"
echo "Built dist/$NAME.tar.gz, tagged $TAG."
fi

if [ "$1" = "--publish" ]; then
    push_release
    if github_release_exists; then
        echo "GitHub already has the $TAG release."
    else
        gh release create "$TAG" "dist/$NAME.tar.gz" "dist/$NAME.tar.gz.sha256" \
            --title "$TITLE $VERSION" --notes-file "$NOTES" --prerelease --latest=false
    fi
    gh release view "$TAG" --json url -q .url
fi

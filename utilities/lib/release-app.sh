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
if [ "$1" = "--publish" ] && git remote get-url origin >/dev/null 2>&1; then
    git pull --rebase origin main
fi
grep -q "^## $VERSION" "$APP/CHANGELOG.md" || { echo "$APP/CHANGELOG.md has no section for $VERSION (run utilities/rivet-dev bump $APP_ID)." >&2; exit 1; }

# A published version never moves to a different commit: ask for a new version instead.
HEAD_COMMIT=$(git rev-parse HEAD)
LOCAL_TAG=$(git rev-parse -q --verify "refs/tags/$TAG^{}" || true)
REMOTE_TAG=$(git ls-remote origin "refs/tags/$TAG^{}" 2>/dev/null | cut -f1)
for T in "$LOCAL_TAG" "$REMOTE_TAG"; do
    if [ -n "$T" ] && [ "$T" != "$HEAD_COMMIT" ]; then
        echo "$TAG already exists for an earlier commit (it may already be published)." >&2
        echo "Run utilities/rivet-dev bump $APP_ID, then this again." >&2
        exit 1
    fi
done

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

NOTES="dist/notes-$DIST_NAME-$VERSION.md"
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

if [ "$1" = "--publish" ]; then
    git push origin main "$TAG"
    gh release create "$TAG" "dist/$NAME.tar.gz" "dist/$NAME.tar.gz.sha256" \
        --title "$TITLE $VERSION" --notes-file "$NOTES" --prerelease --latest=false
    gh release view "$TAG" --json url -q .url
fi

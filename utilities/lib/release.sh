#!/bin/sh
# Make a release of Rivet Files.
#
#   utilities/rivet-dev release            build the tarball and AppImage in dist/ and tag the
#                                          current commit vVERSION — on this computer only
#   utilities/rivet-dev release --publish  the same, then send the code and the tag to GitHub and
#                                          create the GitHub release with the files attached
#   utilities/rivet-dev release --draft    build, then make a draft GitHub release of the tag already
#                                          on GitHub (what the release workflow runs)
#
# The version comes from Cargo.toml; the release notes from its section in CHANGELOG.md.
# For a new release: run utilities/rivet-dev bump first (sets the version, changelog section, commits), then this.
set -e
cd "$(dirname "$0")/../.."
# Rivet FTP and Rivet Notes have their own (simpler) release.
case "${RIVET_APP:-files}" in
    files | rivet-files | rivet) ;;
    *) exec sh ./utilities/lib/release-app.sh "$@" ;;
esac
. ./utilities/lib/apps.sh
APP=apps/rivet-files   # Rivet Files: its version, changelog, desktop file and icons

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' $APP/Cargo.toml | head -1)
ARCH=$(uname -m)
NAME="rivet-files-$VERSION-$ARCH-linux"
TAG="v$VERSION"

if [ -n "$(git status --porcelain)" ]; then
    echo "Commit your changes first (the release is built from the current commit)." >&2
    exit 1
fi
# Publishing: take in GitHub's commits first (e.g. the automatic "aur: X.Y.Z" one), so the
# tag goes on a commit that GitHub will accept.
. ./utilities/lib/publish.sh
if [ "$1" = "--publish" ]; then
    sync_with_github
fi
grep -q "^## $VERSION" "$APP/CHANGELOG.md" || { echo "CHANGELOG.md has no section for $VERSION." >&2; exit 1; }

# A published version never moves to a different commit; a release whose tag is on an earlier
# commit of this branch (publishing cut short) is finished instead of rebuilt.
HEAD_COMMIT=$(git rev-parse HEAD)
check_tag || { echo "Run utilities/rivet-dev bump, then this again." >&2; exit 1; }
APPIMAGE="dist/Rivet_Files-$VERSION-$ARCH.AppImage"
if [ -n "$RESUME" ]; then
    if [ ! -f "$APPIMAGE" ] || [ ! -f "dist/$NAME.tar.gz" ] || [ ! -f "dist/notes-$VERSION.md" ]; then
        echo "$TAG is already made, but its files aren't in dist/ any more." >&2
        echo "Rebuild them from the tag (git checkout $TAG; utilities/rivet-dev release; git checkout main), then run this again." >&2
        exit 1
    fi
    echo "$TAG was made earlier; finishing it (nothing is rebuilt)."
else
./utilities/lib/portable-build.sh
RIVET_BIN=$(./utilities/lib/portable-bin.sh)
mkdir -p dist
STAGE="dist/$NAME"
rm -rf "$STAGE"
mkdir -p "$STAGE"
strip -o "$STAGE/rivet" "$RIVET_BIN"
# Check the stripped file that goes into the tarball, not only the unstripped build.
ROOT=$(pwd)
# shellcheck disable=SC1091
. ./utilities/lib/ensure-zigbuild.sh
./utilities/lib/check-glibc.sh --max "$GLIBC_MAX_VERSION" "$STAGE/rivet"
BIN_GLIBC=$(objdump -T "$STAGE/rivet" | sed -n 's/.*GLIBC_\([0-9][0-9.]*\).*/\1/p' | sort -Vu | tail -n 1)
cp "$APP/data/rivet-files.desktop" "$APP/data/rivet-files.svg" utilities/lib/install.sh "$APP/README.md" "$APP/CHANGELOG.md" LICENSE-MIT LICENSE-APACHE "$STAGE/"
cp -r "$APP/data/icons" "$STAGE/icons"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
rm -rf "$STAGE"
(cd dist && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")
./utilities/lib/appimage.sh
APPIMAGE="dist/Rivet_Files-$VERSION-$ARCH.AppImage"

# Release notes = the README's screenshot, then this version's CHANGELOG section.
release_shot "$APP/README.md" "$TAG" > "dist/notes-$VERSION.md"
awk -v v="## $VERSION" '$0 ~ "^## " { p = (index($0, v) == 1) } p' "$APP/CHANGELOG.md" | tail -n +2 >> "dist/notes-$VERSION.md"
cat >> "dist/notes-$VERSION.md" <<EOF

### Install

**AppImage (one file, nothing to install):** download \`Rivet_Files-$VERSION-$ARCH.AppImage\`, then

\`\`\`sh
chmod +x Rivet_Files-$VERSION-$ARCH.AppImage
./Rivet_Files-$VERSION-$ARCH.AppImage
\`\`\`

Inside Rivet, Preferences → System → *Add Rivet to the app launcher* puts it in your menus.

**Archive:** download \`$NAME.tar.gz\`, then:

\`\`\`sh
tar xzf $NAME.tar.gz
cd $NAME
./install.sh          # adds Rivet to your app launcher (add --default to make it the default file manager)
\`\`\`

Portable $ARCH build, linked against glibc $GLIBC_LINK_VERSION rather than the build machine's libc.
The program's highest GLIBC symbol is $BIN_GLIBC. Counting the keyboard libraries bundled in the AppImage, the artifacts need glibc $(cat dist/glibc-highest) or newer, which Ubuntu 22.04 and Debian 12 both have.
Older systems can build from source: \`cargo build --release\`.
EOF

git rev-parse -q --verify "refs/tags/$TAG" >/dev/null || git tag -a "$TAG" -m "Rivet Files $VERSION"
echo "Built $APPIMAGE and dist/$NAME.tar.gz, tagged $TAG."
fi

if [ "$1" = "--draft" ]; then
    # The release workflow: the tag is already on GitHub, and a person publishes the draft with
    # utilities/rivet-dev live (docs/releasing.md), which also makes it the latest release.
    if github_release_exists; then
        echo "GitHub already has the $TAG release."
    else
        gh release create "$TAG" "$APPIMAGE" "$APPIMAGE.sha256" "dist/$NAME.tar.gz" "dist/$NAME.tar.gz.sha256" \
            --verify-tag --draft --title "Rivet Files $VERSION" --notes-file "dist/notes-$VERSION.md"
    fi
    gh release view "$TAG" --json url -q .url
fi
if [ "$1" = "--publish" ]; then
    push_release
    if github_release_exists; then
        echo "GitHub already has the $TAG release."
    else
        gh release create "$TAG" "$APPIMAGE" "$APPIMAGE.sha256" "dist/$NAME.tar.gz" "dist/$NAME.tar.gz.sha256" \
            --title "Rivet Files $VERSION" --notes-file "dist/notes-$VERSION.md"
    fi
    gh release view "$TAG" --json url -q .url
fi

#!/bin/sh
# An app's edge build: the program as it is on this commit, built by the same release scripts
# as a release, and numbered after its next version with the day and the commit
# (0.4.1-edge.20261127.gabc1234), so --version says exactly which build is installed.
#
#   utilities/rivet-dev edge APP             build it in dist/edge/ (nothing is published)
#   utilities/rivet-dev edge APP --publish   put it in the app's rolling pre-release, tagged
#                                            DIST-edge (rivet-notes-edge), in place of the last
#                                            build there (building it first when it isn't in
#                                            dist/edge/ yet)
#   utilities/rivet-dev edge-changed         the apps changed since their last edge build
#
# The version, its changelog section and a commit are made in a throwaway worktree that shares
# target/ with this one, so the branch here is never touched; the local tag the release scripts
# make there is deleted afterwards. The edge tag has no "-v" in it, so the Release workflow,
# the website and Rivet Welcome 0.4.0 and older, which all go by PREFIX + version, never take it
# for a release; Rivet Welcome's Edge channel looks for it by name. The Edge builds workflow
# runs this every night (docs/releasing.md).
set -eu
cd "$(dirname "$0")/../.."
ROOT=$(pwd -P)
. ./utilities/lib/apps.sh

# The folders an app is built from: its own, every crate it uses all the way down, and the
# workspace's Cargo.toml and Cargo.lock.
app_paths() {
    todo=$APP seen=
    while [ -n "$todo" ]; do
        d=${todo%% *}
        if [ "$d" = "$todo" ]; then todo=; else todo=${todo#* }; fi
        case " $seen " in *" $d "*) continue ;; esac
        seen="$seen $d"
        for c in $(sed -n 's/.*path *= *"\(\.\.\/[^"]*\)".*/\1/p' "$d/Cargo.toml"); do
            todo="$todo $(cd "$d/$c" && pwd -P | sed "s|^$ROOT/||")"
        done
        todo=${todo# }
    done
    echo $seen Cargo.toml Cargo.lock
}

if [ "${1:-}" = --changed ]; then
    git fetch -q -f origin 'refs/tags/*-edge:refs/tags/*-edge' 2>/dev/null || true
    for a in $ALL_APPS; do
        (
            app_select "$a"
            last=$(git rev-parse -q --verify "refs/tags/$DIST_NAME-edge^{commit}" || true)
            # shellcheck disable=SC2046
            if [ -z "$last" ] || ! git diff --quiet "$last" HEAD -- $(app_paths); then
                echo "$APP_ID"
            fi
        )
    done
    exit 0
fi

app_select "${RIVET_APP:?which app: utilities/rivet-dev edge APP}"
ARCH=$(uname -m)
FULL=$(git rev-parse HEAD)
SHA=$(git rev-parse --short=7 HEAD)
DAY=$(TZ=UTC0 git show -s --format=%cd --date=format-local:%Y%m%d HEAD)
DAY_ISO=$(echo "$DAY" | sed 's/\(....\)\(..\)\(..\)/\1-\2-\3/')
OLD=$(app_version)
BASE=${OLD%%-*}
EDGE="$(echo "$BASE" | cut -d. -f1-2).$(($(echo "$BASE" | cut -d. -f3) + 1))-edge.$DAY.g$SHA"
NAME="$DIST_NAME-$EDGE-$ARCH-linux"
APPIMAGE="$(echo "$TITLE" | tr ' ' _)-$EDGE-$ARCH.AppImage"
OUT="$ROOT/dist/edge"
NOTES="$OUT/notes-$DIST_NAME-$EDGE.md"

if [ ! -f "$OUT/$NAME.tar.gz" ] || [ ! -f "$OUT/$APPIMAGE" ] || [ ! -f "$NOTES" ]; then
    WT=$(mktemp -d "${TMPDIR:-/tmp}/rivet-edge.XXXXXX")
    cleanup() {
        git -C "$ROOT" worktree remove --force "$WT" 2>/dev/null || rm -rf "$WT"
        git -C "$ROOT" worktree prune
        git -C "$ROOT" tag -d "$TAG_PREFIX$EDGE" >/dev/null 2>&1 || true
    }
    trap cleanup EXIT INT TERM
    git worktree add -q --detach "$WT" HEAD
    mkdir -p "$ROOT/target"
    ln -s "$ROOT/target" "$WT/target"
    (
        cd "$WT"
        sed -i "0,/^version = \"$OLD\"/s//version = \"$EDGE\"/" "$APP/Cargo.toml"
        cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null
        # The release scripts take their notes from the version's changelog section: the
        # Unreleased notes become this build's, or a section saying there are none.
        if grep -q '^## Unreleased' "$APP/CHANGELOG.md"; then
            sed -i "0,/^## Unreleased.*/s//## $EDGE/" "$APP/CHANGELOG.md"
        else
            awk -v v="## $EDGE" '!done && /^## / { print v; print ""; print "Nothing new since the last release."; print ""; done = 1 } { print }' \
                "$APP/CHANGELOG.md" > "$APP/CHANGELOG.md.new"
            mv "$APP/CHANGELOG.md.new" "$APP/CHANGELOG.md"
        fi
        git commit -q -a -m "Edge build of $TITLE $EDGE"
        RIVET_APP=$APP_ID sh ./utilities/lib/release.sh
    )
    mkdir -p "$OUT"
    rm -f "$OUT/$DIST_NAME"-*-edge.* "$OUT/$(echo "$TITLE" | tr ' ' _)"-*-edge.* "$OUT/notes-$DIST_NAME"-*
    for f in "$NAME.tar.gz" "$NAME.tar.gz.sha256" "$APPIMAGE" "$APPIMAGE.sha256"; do
        cp "$WT/dist/$f" "$OUT/$f"
    done
    {
        echo "The newest $TITLE from main: commit [$SHA](https://github.com/rivetsoft/apps/commit/$FULL), built $DAY_ISO."
        echo "Rivet Welcome installs it when Versions is set to Edge in its settings. This release is"
        echo "replaced each night $TITLE changes, and an edge build can break: keep backups of what you"
        echo "use it with."
        echo
        echo "### New since the last release"
        awk -v v="## $EDGE" '$0 ~ "^## " { p = (index($0, v) == 1); next } p' "$WT/$APP/CHANGELOG.md"
    } > "$NOTES"
    echo "Built $OUT/$NAME.tar.gz and $OUT/$APPIMAGE."
fi

if [ "${1:-}" = --publish ]; then
    TAG=$DIST_NAME-edge
    FILES="$NAME.tar.gz $NAME.tar.gz.sha256 $APPIMAGE $APPIMAGE.sha256"
    git push -q -f origin "$FULL:refs/tags/$TAG"
    # Always a pre-release and never the latest, so it's never taken for a release.
    if gh release view "$TAG" >/dev/null 2>&1; then
        gh release edit "$TAG" --title "$TITLE edge" --notes-file "$NOTES" --prerelease --latest=false >/dev/null
    else
        gh release create "$TAG" --verify-tag --title "$TITLE edge" --notes-file "$NOTES" --prerelease --latest=false >/dev/null
    fi
    # The new build goes up beside the last one before the last one goes, so Rivet Welcome
    # always finds a whole build (it takes the newest when there are two).
    before=$(gh release view "$TAG" --json assets -q '.assets[].name')
    # shellcheck disable=SC2086
    (cd "$OUT" && gh release upload "$TAG" $FILES --clobber)
    for a in $before; do
        case " $FILES " in *" $a "*) ;; *) gh release delete-asset "$TAG" "$a" -y ;; esac
    done
    gh release view "$TAG" --json url -q .url
fi

#!/bin/sh
# Publish the draft releases the Release workflow made (after utilities/rivet-dev ship), so
# Rivet Welcome offers them and the website lists them, then rebuild the website.
#
#   utilities/rivet-dev live notes             publish the draft of Rivet Notes' version on main
#   utilities/rivet-dev live notes edit        several apps
#   utilities/rivet-dev live all               every app with a draft for its version on main, except held ones
#
# Rivet Files' release becomes the repository's latest; the others never do. Apps that aren't
# STABLE in apps.sh are pre-releases, which Rivet Welcome badges "Alpha". Publishing as a person
# (not the workflow) is what starts the AUR and Flatpak workflows for Rivet Files.
set -e
cd "$(dirname "$0")/../.."
. ./utilities/lib/apps.sh

# The website's build, started again so it lists the new releases.
SITE_REPO=rivetsoft/rivetsoft.github.io
SITE_WORKFLOW=${RIVET_SITE_WORKFLOW:-deploy.yml}

ALL=
LIST=${RIVET_APP:-}
for arg in "$@"; do
    case "$arg" in
        all) LIST="$ALL_APPS" ALL=1 ;;
        *) is_app "$arg" || { echo "Unknown app: $arg (one of: $ALL_APPS, or all)" >&2; exit 1; }
           LIST="$LIST $arg" ;;
    esac
done
[ -n "$LIST" ] || { echo "Which app? utilities/rivet-dev live APP (or all)" >&2; exit 1; }
git fetch -q origin main
published=
for a in $LIST; do
    app_select "$a"
    if [ -n "$ALL" ] && [ -n "$HOLD" ]; then
        echo "$TITLE is held back (HOLD=1 in utilities/lib/apps.sh): skipped. Name it to go ahead."
        continue
    fi
    VERSION=$(git show "origin/main:$APP/Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p' | head -1)
    TAG=$TAG_PREFIX$VERSION
    state=$(gh release view "$TAG" --json isDraft,assets -q '"\(.isDraft) \(.assets | length)"' 2>/dev/null || true)
    case "$state" in
        "true 4") ;;
        "false "*)
            [ -n "$ALL" ] || echo "$TAG is already published."
            continue ;;
        "")
            [ -n "$ALL" ] && continue
            echo "No release for $TAG yet: utilities/rivet-dev ship $APP_ID, and wait for its build." >&2
            exit 1 ;;
        *)
            echo "The $TAG draft has $(echo "$state" | cut -d' ' -f2) files, not 4: look at its build (gh run list --workflow release.yml)." >&2
            exit 1 ;;
    esac
    if [ "$APP_ID" = files ]; then LATEST=--latest; else LATEST=--latest=false; fi
    gh release edit "$TAG" --draft=false "$LATEST" >/dev/null
    echo "Published $TITLE $VERSION: $(gh release view "$TAG" --json url -q .url)"
    published=1
done
if [ -n "$published" ]; then
    if gh workflow run "$SITE_WORKFLOW" -R "$SITE_REPO" >/dev/null; then
        echo "Rebuilding the website with the new releases."
    else
        echo "Couldn't start the website's rebuild: gh workflow run $SITE_WORKFLOW -R $SITE_REPO" >&2
    fi
fi

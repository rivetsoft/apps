#!/bin/sh
# Start a release on GitHub: tag the app's version on main and push the tag. The Release workflow
# (.github/workflows/release.yml) then builds the archive and the AppImage on GitHub, checks them,
# and makes a draft release with the changelog as its notes. Publish it with
# utilities/rivet-dev live once the draft looks right (docs/releasing.md).
#
#   utilities/rivet-dev ship notes             tag rivet-notes-vVERSION and push it
#   utilities/rivet-dev ship notes edit        several apps
#   utilities/rivet-dev ship all               every app whose version isn't tagged yet
#   utilities/rivet-dev ship all --dry-run     only say what it would tag
#
# The version is the one in the app's Cargo.toml on main, set by utilities/rivet-dev bump.
set -e
cd "$(dirname "$0")/../.."
. ./utilities/lib/apps.sh

DRY=
ALL=
LIST=${RIVET_APP:-}
for arg in "$@"; do
    case "$arg" in
        --dry-run | -n) DRY=1 ;;
        all) LIST="$ALL_APPS" ALL=1 ;;
        *) is_app "$arg" || { echo "Unknown app: $arg (one of: $ALL_APPS, or all)" >&2; exit 1; }
           LIST="$LIST $arg" ;;
    esac
done
[ -n "$LIST" ] || { echo "Which app? utilities/rivet-dev ship APP (or all)" >&2; exit 1; }

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    echo "Commit your changes first: a release is made from main as it is on GitHub." >&2
    exit 1
fi
git fetch -q origin main --tags
if ! git merge-base --is-ancestor HEAD origin/main; then
    echo "This commit isn't on GitHub's main. Merge the release pull request, then: git switch main && git pull" >&2
    exit 1
fi

tagged=
for a in $LIST; do
    app_select "$a"
    VERSION=$(app_version)
    TAG=$TAG_PREFIX$VERSION
    if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null || [ -n "$(git ls-remote origin "refs/tags/$TAG")" ]; then
        [ -n "$ALL" ] && continue
        echo "$TAG is already tagged: run utilities/rivet-dev bump $APP_ID first." >&2
        exit 1
    fi
    grep -q "^## $VERSION\$" "$APP/CHANGELOG.md" || {
        echo "$APP/CHANGELOG.md has no section for $VERSION: run utilities/rivet-dev bump $APP_ID first." >&2
        exit 1
    }
    if [ -n "$DRY" ]; then
        echo "Would tag $TAG ($TITLE $VERSION) on $(git rev-parse --short HEAD) and push it."
        continue
    fi
    git tag -a "$TAG" -m "$TITLE $VERSION"
    git push -q origin "refs/tags/$TAG"
    echo "Pushed $TAG: GitHub is building $TITLE $VERSION."
    tagged="$tagged $APP_ID"
done

if [ -n "$tagged" ]; then
    echo
    echo "Follow the builds with: gh run list --workflow release.yml"
    echo "Each makes a draft release. When they look right: utilities/rivet-dev live$tagged"
elif [ -z "$DRY" ]; then
    echo "Nothing to tag: every app's version is already tagged."
fi

#!/bin/sh
# Release the apps one at a time, asking about each.
#
#   utilities/rivet-dev release-all              walk through every app: release, skip or stop
#   utilities/rivet-dev release-all --publish    the same, and publish each release on GitHub
#   utilities/rivet-dev release-all --dry-run    show what it would do; change nothing
#
# Options (combine freely):
#   --publish     send each release to GitHub as it's made (else: built and tagged here only;
#                 publish later with `utilities/rivet-dev push` and the GitHub releases page)
#   --dry-run     only show each app's state and the version it would get
#   --no-checks   skip the build, test and lint run before the first release
#   --yes         release every app that has changes, with the suggested version, no questions
#   --only LIST   only these apps, e.g. --only edit,terminal
#
# For each app (Files, FTP, Notes, Edit, Terminal) it shows the version, the last release and
# the changes since. Apps with no changes since their last release are skipped. Then it asks:
# release (with the suggested version, or one you type), skip, or stop. Releasing runs the
# usual `bump` and `release` for that app, so each keeps its own version, changelog and tags.
# The first failure stops everything, so nothing broken goes out.
set -e
cd "$(dirname "$0")/../.."
. ./utilities/lib/apps.sh

PUBLISH=
DRY=
CHECKS=1
YES=
ONLY=
while [ $# -gt 0 ]; do
    case "$1" in
        --publish) PUBLISH=--publish ;;
        --dry-run | -n) DRY=1 ;;
        --no-checks) CHECKS= ;;
        --yes | -y) YES=1 ;;
        --only) shift; ONLY=$(printf '%s' "${1:-}" | tr ',' ' ') ;;
        -h | --help) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1 (see utilities/rivet-dev release-all --help)" >&2; exit 1 ;;
    esac
    shift
done

say() { printf '%s\n' "$*"; }
bold() { if [ -t 1 ]; then printf '\033[1m%s\033[0m\n' "$*"; else say "$*"; fi; }

# Questions come from the keyboard even when the output is piped.
tty_ok() { [ -t 0 ] || (: </dev/tty) 2>/dev/null; }
ask() {
    printf '%s ' "$1" >&2
    # The keyboard: standard input when it's a terminal, else the terminal itself when there is
    # one, else whatever is piped in (scripted answers). No answer to read (the end of the input,
    # no terminal) always means stop: silence must never release anything.
    if [ -t 0 ]; then
        read -r REPLY || REPLY=q
    elif (: </dev/tty) 2>/dev/null; then
        read -r REPLY </dev/tty || REPLY=q
    else
        read -r REPLY || REPLY=q
    fi
    [ -t 0 ] || printf '%s\n' "${REPLY:-(Enter)}" >&2
}

# The next version: a first release keeps its version (0.1.0); every later one goes up a minor
# step (0.8.0 to 0.9.0, 1.2.3 to 1.3.0). Type a version at the question for anything else.
suggest() {
    v=$1 last=$2
    if [ -z "$last" ]; then
        echo "$v"
        return
    fi
    major=$(echo "$v" | cut -d. -f1) minor=$(echo "$v" | cut -d. -f2)
    echo "$major.$((minor + 1)).0"
}

APPS="files ftp notes edit terminal welcome"
[ -n "$ONLY" ] && APPS=$ONLY
for a in $APPS; do
    is_app "$a" || { echo "Unknown app: $a (files, ftp, notes, edit, terminal or welcome)" >&2; exit 1; }
done

if [ -z "$DRY" ] && [ -n "$(git status --porcelain)" ]; then
    echo "Commit your changes first: releases are built from commits." >&2
    exit 1
fi
if [ -z "$DRY" ] && [ -z "$YES" ] && ! tty_ok && [ ! -p /dev/stdin ]; then
    echo "This asks about each app; run it in a terminal, or add --yes." >&2
    exit 1
fi
if [ -n "$PUBLISH" ] && [ -z "$DRY" ]; then
    command -v gh >/dev/null || { echo "Publishing needs the GitHub CLI (gh), signed in: gh auth login" >&2; exit 1; }
fi


bold "Rivet Apps: releasing one by one${DRY:+ (dry run: nothing changes)}${PUBLISH:+, publishing to GitHub}"
say

checked=
released=
skipped=
for a in $APPS; do
    app_select "$a"
    version=$(app_version)
    last=$(git describe --tags --abbrev=0 --match "$TAG_PREFIX[0-9]*" 2>/dev/null || true)
    if [ -n "$last" ]; then
        # The app's own code and the shared crates; not the AUR workflow's packaging updates or
        # Cargo.lock (another app's version bump touches it).
        changes=$(git rev-list --count --no-merges "$last..HEAD" -- "$APP" crates ":(exclude)$APP/packaging/aur")
    else
        changes=$(git rev-list --count HEAD -- "$APP")
    fi
    next=$(suggest "$version" "$last")

    bold "$TITLE"
    say "  version $version · last release ${last:-none yet} · $changes change(s) since"
    # Made here but publishing was cut short (e.g. GitHub turned main away): offer to finish it.
    if [ -n "$PUBLISH" ] && [ "$last" = "$TAG_PREFIX$version" ] && command -v gh >/dev/null && ! gh release view "$last" >/dev/null 2>&1; then
        say "  $last is made, but GitHub has no release for it yet."
        if [ -n "$DRY" ]; then
            say "  Would finish publishing $TITLE $version."
            say
            continue
        fi
        choice=f
        if [ -z "$YES" ]; then
            ask "  Finish publishing $TITLE $version? [Enter: yes · s: skip · q: stop]"
            choice=$REPLY
        fi
        case "$choice" in
            "" | f | y | yes)
                RIVET_APP=$APP_ID sh ./utilities/lib/release.sh --publish
                released="$released $APP_ID@$version"
                say
                continue ;;
            q | quit | stop)
                say "  Stopped."
                break ;;
            *)
                say "  Skipped."
                skipped="$skipped $APP_ID"
                say
                continue ;;
        esac
    fi
    if [ -n "$last" ] && [ "$changes" = 0 ]; then
        say "  Nothing new since $last: skipped."
        skipped="$skipped $APP_ID"
        say
        continue
    fi
    if git rev-parse -q --verify "refs/tags/$TAG_PREFIX$next" >/dev/null; then
        say "  $TAG_PREFIX$next is already tagged here (released earlier?): skipped."
        skipped="$skipped $APP_ID"
        say
        continue
    fi
    if [ -n "$DRY" ]; then
        say "  Would release $TITLE $next (tag $TAG_PREFIX$next)${PUBLISH:+ and publish it}."
        say
        continue
    fi

    choice=r
    if [ -z "$YES" ]; then
        ask "  Release $TITLE $next? [Enter: yes · a version like 0.3.0 · s: skip · q: stop]"
        choice=$REPLY
    fi
    case "$choice" in
        "" | r | y | yes) ;;
        s | skip | n | no)
            say "  Skipped."
            skipped="$skipped $APP_ID"
            say
            continue ;;
        q | quit | stop)
            say "  Stopped."
            break ;;
        [0-9]*.[0-9]*.[0-9]*) next=$choice ;;
        *)
            say "  Didn't understand \"$choice\": skipped."
            skipped="$skipped $APP_ID"
            say
            continue ;;
    esac

    # Build, test and lint once, before the first release (what CI runs).
    if [ -n "$CHECKS" ] && [ -z "$checked" ]; then
        say "  Checking everything first (build, tests, lint)…"
        cargo build --locked --workspace
        cargo test --locked --workspace -q
        cargo clippy --workspace --all-targets --locked -q -- -D warnings
        checked=1
    fi

    say "  Setting the version to $next…"
    RIVET_APP=$APP_ID sh ./utilities/lib/bump.sh "$next"
    # The release notes, from the changelog, before anything is built or sent.
    say "  Release notes ($APP/CHANGELOG.md):"
    awk -v v="## $next" '$0 ~ "^## " { p = (index($0, v) == 1); next } p' "$APP/CHANGELOG.md" | sed '/^$/d; s/^/    /'
    if [ -z "$YES" ]; then
        ask "  Go ahead with these notes? [Enter: yes · e: stop here to edit them]"
        case "$REPLY" in
            e | edit | q | n | no | stop)
                say "  Stopped. Edit $APP/CHANGELOG.md, then: git commit --amend -a --no-edit,"
                say "  utilities/rivet-dev release $APP_ID${PUBLISH:+ --publish}, and release-all again for the rest."
                break ;;
        esac
    fi
    say "  Building and tagging…"
    RIVET_APP=$APP_ID sh ./utilities/lib/release.sh $PUBLISH
    released="$released $APP_ID@$next"
    say
done

bold "Done."
say "  Released:${released:- nothing}"
say "  Skipped:${skipped:- nothing}"
if [ -n "$released" ] && [ -z "$PUBLISH" ] && [ -z "$DRY" ]; then
    say
    say "  The downloads are in dist/ and the tags are on this computer only."
    say "  To publish: utilities/rivet-dev release APP --publish for each (it reuses the tag),"
    say "  or run this again with --publish next time."
fi

#!/bin/sh
# Which app a command works on. Sourced by the other scripts:
#
#   . utilities/lib/apps.sh
#   app_select "$RIVET_APP"     # files (the default), or any name in ALL_APPS
#
# sets APP_ID, APP (its folder), PKG (the cargo package), BIN (the program), TITLE,
# TAG_PREFIX (release tags are "$TAG_PREFIX$VERSION"), DIST_NAME (archive names, and the name of
# its desktop file and icon in data/) and DESKTOP (the desktop file the archive and AppImage carry).
# Rivet Files keeps plain vX.Y.Z tags so its downloads, Flatpak and AUR packages carry on.
# Rivet Welcome reads the same names from GitHub (apps/rivet-welcome/src/catalog.rs), and a test
# there checks that it lists every app here: change both together.

# Every app that is released, in the order the release commands go through them. An app with
# HOLD=1 below is held back: `release-all`, `ship all` and `live all` pass over it and Rivet
# Welcome doesn't offer it, while naming it (`ship longshot`) still works. See docs/releasing.md.
ALL_APPS="files notes edit terminal ftp snip shell vice calendar color fetch longshot reader cinema greeter rivetctl welcome"

app_select() {
    # STABLE=1: published as a full release, not a pre-release (Rivet Welcome badges
    # pre-releases "Alpha"). Rivet Files is always "latest"; the others never are.
    # HOLD=1: not ready to go out; commands that go through every app pass over it.
    STABLE=
    HOLD=
    # Libraries the release carries beside the program (pdfium: utilities/lib/pdfium.sh).
    BUNDLE=
    case "${1:-files}" in
        files | rivet-files | rivet)
            APP_ID=files PKG=rivet BIN=rivet TITLE="Rivet Files" TAG_PREFIX=v DIST_NAME=rivet-files STABLE=1 ;;
        ftp | rivet-ftp)
            APP_ID=ftp TITLE="Rivet FTP" ;;
        notes | rivet-notes)
            APP_ID=notes TITLE="Rivet Notes" STABLE=1 ;;
        edit | rivet-edit)
            APP_ID=edit TITLE="Rivet Edit" STABLE=1 ;;
        terminal | term | rivet-terminal)
            APP_ID=terminal TITLE="Rivet Terminal" STABLE=1 ;;
        welcome | rivet-welcome)
            APP_ID=welcome TITLE="Rivet Welcome" ;;
        snip | rivet-snip)
            APP_ID=snip TITLE="Rivet Snip" ;;
        shell | rivet-shell)
            APP_ID=shell TITLE="Rivet Shell" ;;
        vice | rivet-vice)
            APP_ID=vice TITLE="Rivet Vice" ;;
        calendar | rivet-calendar)
            APP_ID=calendar TITLE="Rivet Calendar" ;;
        color | rivet-color)
            APP_ID=color TITLE="Rivet Color" ;;
        fetch | rivet-fetch | rivetfetch)
            APP_ID=fetch TITLE="Rivet Fetch" BIN=rivetfetch ;;
        longshot | rivet-longshot)
            APP_ID=longshot TITLE="Rivet Long Shot" HOLD=1 ;;
        reader | rivet-reader)
            APP_ID=reader TITLE="Rivet Reader" HOLD=1 BUNDLE=pdfium ;;
        cinema | rivet-cinema)
            APP_ID=cinema TITLE="Rivet Cinema" HOLD=1 ;;
        greeter | rivet-greeter)
            APP_ID=greeter TITLE="Rivet Greeter" HOLD=1 ;;
        rivetctl | ctl)
            APP_ID=rivetctl TITLE=rivetctl HOLD=1 ;;
        *)
            echo "Unknown app: $1 (one of: $ALL_APPS)" >&2
            return 1 ;;
    esac
    # The rest follow from the name: apps/rivet-notes, package and program rivet-notes,
    # tags rivet-notes-vX.Y.Z, archives rivet-notes-X.Y.Z-ARCH-linux.tar.gz.
    # rivetctl is its own name everywhere: apps/rivetctl, tags rivetctl-vX.Y.Z.
    if [ "$APP_ID" = rivetctl ]; then
        PKG=rivetctl BIN=rivetctl DIST_NAME=rivetctl TAG_PREFIX=rivetctl-v
    elif [ "$APP_ID" != files ]; then
        PKG=rivet-$APP_ID
        DIST_NAME=rivet-$APP_ID
        TAG_PREFIX=rivet-$APP_ID-v
        [ "$APP_ID" = fetch ] || BIN=rivet-$APP_ID
    fi
    APP=apps/rivet-$APP_ID
    [ "$APP_ID" = rivetctl ] && APP=apps/rivetctl
    # Rivet Shell is started by the compositor; what goes in the launcher is its Settings window.
    DESKTOP=$DIST_NAME.desktop
    [ "$APP_ID" = shell ] && DESKTOP=rivet-shell-settings.desktop
    # A program for the terminal (rivetfetch, rivetctl) gets no launcher entry when installed,
    # and neither does the login screen (greetd starts Rivet Greeter).
    LAUNCHER=1
    case "$APP_ID" in fetch | greeter | rivetctl) LAUNCHER= ;; esac
    # The glibc the release program is linked for. Rivet Shell and Rivet Greeter link
    # libxkbcommon themselves (their keyboard handling, rivet-lockscreen's for the greeter), and Ubuntu 22.04's libxkbcommon needs glibc 2.33; the others load it
    # at run time and keep 2.31 (ensure-zigbuild.sh). Both run on Ubuntu 22.04 and Debian 12.
    LINK_GLIBC=
    case "$APP_ID" in shell | greeter) LINK_GLIBC=2.33 ;; esac
    return 0
}

# The app a release tag belongs to: "rivet-notes-v0.4.1" -> notes, "v0.10.0" -> files.
app_of_tag() {
    for a in $ALL_APPS; do
        (
            app_select "$a"
            v=${1#"$TAG_PREFIX"}
            [ "$v" != "$1" ] && case "$v" in [0-9]*) echo "$APP_ID" ;; esac
        )
    done | head -n 1
}

# The first screenshot in a README (one from docs/screenshots), as a Markdown picture at a release
# tag, for the top of that release's notes: release_shot apps/rivet-notes/README.md rivet-notes-v0.4.2
release_shot() {
    line=$(grep -m1 -E '^!\[[^]]*\]\([^)]*docs/screenshots/[^)]+\)' "$1") || {
        echo "$1 has no screenshot from docs/screenshots to open the release notes with." >&2
        return 1
    }
    printf '%s\n\n' "$line" | sed -E "s#\]\([^)]*docs/screenshots/#](https://raw.githubusercontent.com/rivetsoft/apps/$2/docs/screenshots/#"
}

# The app's version, from its Cargo.toml.
app_version() {
    sed -n 's/^version = "\(.*\)"/\1/p' "$APP/Cargo.toml" | head -1
}

# Is this word an app name? (For commands that take one first.)
is_app() {
    case "$1" in
        "" | -*) return 1 ;;
    esac
    (app_select "$1") 2>/dev/null
}

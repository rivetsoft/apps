#!/bin/sh
# Which app a command works on. Sourced by the other scripts:
#
#   . utilities/lib/apps.sh
#   app_select "$RIVET_APP"     # files (the default), ftp, notes, edit, terminal, welcome, snip or color
#
# sets APP_ID, APP (its folder), PKG (the cargo package), BIN (the program), TITLE,
# TAG_PREFIX (release tags are "$TAG_PREFIX$VERSION") and DIST_NAME (archive names).
# Rivet Files keeps plain vX.Y.Z tags so its downloads, Flatpak and AUR packages carry on.

app_select() {
    # STABLE=1: published as a full release, not a pre-release (Rivet Welcome badges
    # pre-releases "Alpha"). Rivet Files is always "latest"; the others never are.
    STABLE=
    case "${1:-files}" in
        files | rivet-files | rivet)
            APP_ID=files APP=apps/rivet-files PKG=rivet BIN=rivet TITLE="Rivet Files" TAG_PREFIX=v DIST_NAME=rivet-files ;;
        ftp | rivet-ftp)
            APP_ID=ftp APP=apps/rivet-ftp PKG=rivet-ftp BIN=rivet-ftp TITLE="Rivet FTP" TAG_PREFIX=rivet-ftp-v DIST_NAME=rivet-ftp ;;
        notes | rivet-notes)
            APP_ID=notes APP=apps/rivet-notes PKG=rivet-notes BIN=rivet-notes TITLE="Rivet Notes" TAG_PREFIX=rivet-notes-v DIST_NAME=rivet-notes STABLE=1 ;;
        edit | rivet-edit)
            APP_ID=edit APP=apps/rivet-edit PKG=rivet-edit BIN=rivet-edit TITLE="Rivet Edit" TAG_PREFIX=rivet-edit-v DIST_NAME=rivet-edit STABLE=1 ;;
        terminal | term | rivet-terminal)
            APP_ID=terminal APP=apps/rivet-terminal PKG=rivet-terminal BIN=rivet-terminal TITLE="Rivet Terminal" TAG_PREFIX=rivet-terminal-v DIST_NAME=rivet-terminal STABLE=1 ;;
        welcome | rivet-welcome)
            APP_ID=welcome APP=apps/rivet-welcome PKG=rivet-welcome BIN=rivet-welcome TITLE="Rivet Welcome" TAG_PREFIX=rivet-welcome-v DIST_NAME=rivet-welcome ;;
        snip | rivet-snip)
            APP_ID=snip APP=apps/rivet-snip PKG=rivet-snip BIN=rivet-snip TITLE="Rivet Snip" TAG_PREFIX=rivet-snip-v DIST_NAME=rivet-snip ;;
        color | rivet-color)
            APP_ID=color APP=apps/rivet-color PKG=rivet-color BIN=rivet-color TITLE="Rivet Color" TAG_PREFIX=rivet-color-v DIST_NAME=rivet-color ;;
        *)
            echo "Unknown app: $1 (files, ftp, notes, edit, terminal, welcome, snip or color)" >&2
            return 1 ;;
    esac
}

# The app's version, from its Cargo.toml.
app_version() {
    sed -n 's/^version = "\(.*\)"/\1/p' "$APP/Cargo.toml" | head -1
}

# Is this word an app name? (For commands that take one first.)
is_app() {
    case "$1" in
        files | rivet-files | rivet | ftp | rivet-ftp | notes | rivet-notes | edit | rivet-edit | terminal | term | rivet-terminal | welcome | rivet-welcome | snip | rivet-snip | color | rivet-color) return 0 ;;
        *) return 1 ;;
    esac
}

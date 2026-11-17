#!/bin/sh
# Which app a command works on. Sourced by the other scripts:
#
#   . utilities/lib/apps.sh
#   app_select "$RIVET_APP"     # files (the default), ftp, notes or edit
#
# sets APP_ID, APP (its folder), PKG (the cargo package), BIN (the program), TITLE,
# TAG_PREFIX (release tags are "$TAG_PREFIX$VERSION") and DIST_NAME (archive names).
# Rivet Files keeps plain vX.Y.Z tags so its downloads, Flatpak and AUR packages carry on.

app_select() {
    case "${1:-files}" in
        files | rivet-files | rivet)
            APP_ID=files APP=apps/rivet-files PKG=rivet BIN=rivet TITLE="Rivet Files" TAG_PREFIX=v DIST_NAME=rivet-files ;;
        ftp | rivet-ftp)
            APP_ID=ftp APP=apps/rivet-ftp PKG=rivet-ftp BIN=rivet-ftp TITLE="Rivet FTP" TAG_PREFIX=rivet-ftp-v DIST_NAME=rivet-ftp ;;
        notes | rivet-notes)
            APP_ID=notes APP=apps/rivet-notes PKG=rivet-notes BIN=rivet-notes TITLE="Rivet Notes" TAG_PREFIX=rivet-notes-v DIST_NAME=rivet-notes ;;
        edit | rivet-edit)
            APP_ID=edit APP=apps/rivet-edit PKG=rivet-edit BIN=rivet-edit TITLE="Rivet Edit" TAG_PREFIX=rivet-edit-v DIST_NAME=rivet-edit ;;
        *)
            echo "Unknown app: $1 (files, ftp, notes or edit)" >&2
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
        files | rivet-files | rivet | ftp | rivet-ftp | notes | rivet-notes | edit | rivet-edit) return 0 ;;
        *) return 1 ;;
    esac
}

#!/bin/sh
# Each app's version, its last release tag, and how many commits touched it since.
set -e
cd "$(dirname "$0")/../.."
. ./utilities/lib/apps.sh
printf '%-12s %-9s %-24s %s\n' APP VERSION "LAST RELEASE" "CHANGES SINCE"
for a in files ftp notes edit terminal welcome; do
    app_select "$a"
    last=$(git describe --tags --abbrev=0 --match "$TAG_PREFIX[0-9]*" 2>/dev/null || true)
    if [ -n "$last" ]; then
        n=$(git rev-list --count "$last..HEAD" -- "$APP" crates)
        printf '%-12s %-9s %-24s %s commits (in %s or crates/)\n' "$APP_ID" "$(app_version)" "$last" "$n" "$APP"
    else
        printf '%-12s %-9s %-24s %s\n' "$APP_ID" "$(app_version)" "none yet" "-"
    fi
done

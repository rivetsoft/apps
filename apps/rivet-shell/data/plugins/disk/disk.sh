#!/usr/bin/env bash
# Disk Space, an example Rivet Shell plugin (stream mode).
#
# It prints one JSON object per line: what its panel item shows (text, icon, tooltip, state),
# the rows of its menu, and, once, a notification when the disk is nearly full. The shell
# writes what happens to its input, one JSON object per line: {"event":"click","button":"left"},
# {"event":"menu","id":"/home"}, {"event":"unlock"} (asked for in plugin.conf).

mount="${1:-/}"
warned=0

show() {
    local size free pct used state menu notify=""
    read -r size free pct < <(df -h --output=size,avail,pcent "$mount" | tail -n 1)
    used=${pct%\%}
    state=normal
    [ "$used" -ge 80 ] && state=warning
    [ "$used" -ge 92 ] && state=danger
    # Every real disk, as a row of the menu (its id is what a click on it sends back).
    menu=$(df -h --output=target,avail,size -x tmpfs -x devtmpfs -x efivarfs -x overlay -x squashfs 2>/dev/null |
        tail -n +2 | awk '$1 !~ /^\/(tmp|run|snap|var\/lib)\// { printf "%s{\"id\":\"%s\",\"label\":\"%s\",\"sub\":\"%s free of %s\"}", (n++ ? "," : ""), $1, $1, $2, $3 }')
    # Said once, not every minute.
    if [ "$used" -ge 95 ] && [ "$warned" -eq 0 ]; then
        warned=1
        notify=",\"notify\":{\"summary\":\"$mount is nearly full\",\"body\":\"$free left of $size\"}"
    fi
    printf '{"text":"%s","icon":"drive-harddisk","tooltip":"%s free of %s on %s","state":"%s","menu":[%s]%s}\n' \
        "$free" "$free" "$size" "$mount" "$state" "$menu" "$notify"
}

while true; do
    show
    # Wait a minute, or for the shell to say something (then show again at once).
    read -r -t 60 line
    rc=$?
    # The shell closed our input: it's gone, or the plugin was turned off.
    [ "$rc" -eq 1 ] && exit 0
    case "$line" in
    *'"event":"menu"'*)
        # A row was clicked: open that disk in the file manager.
        id=${line#*\"id\":\"}
        id=${id%%\"*}
        xdg-open "$id" >/dev/null 2>&1 &
        ;;
    *'"event":"click"'*'"button":"middle"'*)
        # Ask the shell to do something: any `rivet-shell msg` command.
        echo '{"run":"launcher"}'
        ;;
    esac
done

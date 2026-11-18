#!/bin/sh
# Send your latest commits to GitHub (or create the public repo the first time).
# Makes no release: for that, use `utilities/rivet-dev release --publish`.
#
#   push          the branch you're on (main, rivet-shell, …)
#   push --all    every local branch
#
# The branch you're on first takes in commits GitHub has that you don't (e.g. the automatic
# "aur: X.Y.Z" commit after a release), so the push isn't rejected. Other branches (with
# --all) are only pushed when GitHub's copy is behind; a branch that has moved on there is
# left alone and named, to update by switching to it and pushing.
set -e
cd "$(dirname "$0")/../.."

all=false
case "${1:-}" in
    --all) all=true ;;
    "") ;;
    *) echo "push: unknown option $1 (use --all for every branch)" >&2; exit 2 ;;
esac

if ! git remote get-url origin >/dev/null 2>&1; then
    gh repo create rivet-files --public --source=. --remote=origin --push \
        --description "Linux file manager like Nemo, Thunar and Caja, in Rust/egui — no GTK or Qt, system icons, live matugen colors"
    gh repo view --json url -q .url
    exit 0
fi

current=$(git symbolic-ref --quiet --short HEAD) || { echo "push: not on a branch (detached HEAD)" >&2; exit 1; }
git fetch --quiet origin

# The branch you're on: take in what GitHub has, then push (and track it the first time).
if git show-ref --verify --quiet "refs/remotes/origin/$current"; then
    git pull --rebase --autostash origin "$current"
fi
git push -u origin "$current"

if $all; then
    skipped=""
    for b in $(git for-each-ref --format='%(refname:short)' refs/heads/); do
        [ "$b" = "$current" ] && continue
        if git show-ref --verify --quiet "refs/remotes/origin/$b"; then
            [ "$(git rev-parse "$b")" = "$(git rev-parse "origin/$b")" ] && continue
            if ! git merge-base --is-ancestor "origin/$b" "$b"; then
                skipped="$skipped $b"
                continue
            fi
        fi
        git push -u origin "$b"
    done
    if [ -n "$skipped" ]; then
        echo "Not pushed (GitHub has commits these don't; switch to each and push):$skipped" >&2
    fi
fi
gh repo view --json url -q .url

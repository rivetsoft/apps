#!/bin/sh
# Shared by release.sh and release-app.sh: staying in step with GitHub while releasing.
# Sourced, not run. Expects TAG and HEAD_COMMIT to be set by the caller where noted.
#
# GitHub's main can move while a release builds: publishing Rivet Files starts the AUR workflow,
# which commits "aur: X.Y.Z" to main a minute later. So:
#   * GitHub's new commits are merged in, never rebased onto: a release commit that's already
#     tagged must keep its identity, or the tag would point at a commit missing from main;
#   * pushing main retries after merging what arrived meanwhile;
#   * a release whose tag exists on an earlier commit of this branch (a publish cut short) is
#     resumed: nothing is rebuilt or re-tagged, the rest is sent and the GitHub release made.

# Take in GitHub's main (fast-forward when possible, else a merge).
sync_with_github() {
    git remote get-url origin >/dev/null 2>&1 || return 0
    git fetch -q origin main || return 1
    if git merge-base --is-ancestor origin/main HEAD; then
        return 0
    fi
    if git merge -q --ff-only origin/main 2>/dev/null; then
        echo "Took in GitHub's newer commits."
        return 0
    fi
    echo "GitHub has commits this copy doesn't (e.g. the AUR workflow's); merging them in."
    git merge --no-edit origin/main || {
        echo "Merging GitHub's main didn't go cleanly: resolve it, commit, and run this again." >&2
        return 1
    }
}

# Decide whether TAG is new (RESUME empty), already here on an earlier commit of this branch
# (RESUME=1: a publish cut short), or somewhere else (an error: ask for a new version).
check_tag() {
    RESUME=
    local_tag=$(git rev-parse -q --verify "refs/tags/$TAG^{}" || true)
    remote_tag=$(git ls-remote origin "refs/tags/$TAG^{}" 2>/dev/null | cut -f1)
    for t in "$local_tag" "$remote_tag"; do
        [ -n "$t" ] || continue
        [ "$t" = "$HEAD_COMMIT" ] && continue
        if git merge-base --is-ancestor "$t" HEAD 2>/dev/null; then
            RESUME=1
            continue
        fi
        echo "$TAG already exists for a commit that isn't on this branch (it may already be published)." >&2
        return 1
    done
    # The tag on GitHub but not here: fetch it, so it can be pushed and compared.
    if [ -n "$remote_tag" ] && [ -z "$local_tag" ]; then
        git fetch -q origin "refs/tags/$TAG:refs/tags/$TAG" || true
    fi
}

# Push main (merging and retrying when GitHub moved meanwhile), then the tag.
push_release() {
    tries=0
    until git push origin main; do
        tries=$((tries + 1))
        if [ "$tries" -ge 3 ]; then
            echo "GitHub kept turning main away; run this again in a minute." >&2
            return 1
        fi
        echo "GitHub's main moved while releasing; merging and trying again."
        sync_with_github || return 1
    done
    git push origin "refs/tags/$TAG"
}

# Whether GitHub already has a release for TAG.
github_release_exists() {
    gh release view "$TAG" >/dev/null 2>&1
}

#!/bin/bash
# is86: build and run at a COMMIT, never the live tree (adapted by copy from
# s288's at-commit.sh): every commit gets its own worktree,
# $LANE/build-<sha7>, created before the lock is waited on and never moved;
# one cargo target directory per head.
# usage: at-commit.sh <rev> <cmd> [args...]
set -u
L=${LANE:?set LANE}
rev=$1; shift
SHA=$(git -C "$L/wolf-interp" rev-parse --short=7 "$rev^{commit}") || exit 1
B=$L/build-$SHA
if [ ! -d "$B" ]; then
  git -C "$L/wolf-interp" worktree add -q --detach "$B" "$SHA" || { echo "is86: worktree $B failed"; exit 1; }
fi
[ "$(git -C "$B" rev-parse --short=7 HEAD)" = "$SHA" ] || { echo "is86: $B is not at $SHA"; exit 1; }
[ -z "$(git -C "$B" status --porcelain --untracked-files=no)" ] || { echo "is86: $B is not clean"; exit 1; }
cd "$B" || exit 1
echo "is86: at wolf-interp $SHA in build-$SHA"
exec "$L/rig/locked.sh" "$@"

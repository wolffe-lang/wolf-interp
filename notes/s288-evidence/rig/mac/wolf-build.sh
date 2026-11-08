#!/bin/bash
# s288: the compiler at the reference commit, built in its own worktree (behind
# the lock, by at-commit.sh), then copied by value to $LANE/bin-<sha7>/ with
# its sha256. The records of the rows come from this binary.
set -u
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
step head bash -c 'git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l && rustc -V' || exit 1
step build cargo build --locked -p wolf_driver -p wolf_rt || exit 1
sha=$(git rev-parse --short=8 HEAD)
out=$LANE/bin-$sha
mkdir -p "$out"
step copy bash -c "cp -p target/debug/wolf '$out/wolf' && shasum -a 256 target/debug/wolf '$out/wolf'" || exit 1
echo "== done $(date -u +%FT%TZ)"

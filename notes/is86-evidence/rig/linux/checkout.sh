#!/usr/bin/env bash
# is86: a fresh clone of the pushed branch from GitHub, one per head
# ($lane/wi-<sha7>, its own target directory), checked against the head; then
# every crate the build reads fetched once, outside the heavy locks.
set -euo pipefail
lane=$LANE
sha=$1
dir=$lane/wi-${sha:0:7}
git=$lane/tool-bin/git
if test -e "$dir"; then
  test "$($git -C "$dir" rev-parse HEAD)" = "$sha"
  test -z "$($git -C "$dir" status --porcelain --untracked-files=no)"
else
  $git clone --quiet --branch "${IS86_BRANCH:-is86}" https://github.com/wolffe-lang/wolf-interp.git "$dir"
  test "$($git -C "$dir" rev-parse HEAD)" = "$sha"
fi
bash "$lane/gauntlet/prefetch.sh" "$dir"
echo "checkout: $dir at $sha, crates staged, $(date -u +%FT%TZ)"

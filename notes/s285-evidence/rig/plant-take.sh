#!/bin/bash
# s285: the lupin mirror's plant take on the Mac. A fresh GitHub clone of the
# pushed plant branch, checked against its sha and its parent (the green head);
# the build worktree's target copied in by clonefile (cargo rebuilds the crate,
# whose sources are newer); then the recorder, behind the Mac build lock, runs
# mac-interp-plant.sh. Detached by the caller; log and marker under records/.
#   plant-take.sh GREEN PLANT BRANCH ROW...
set -eu
export LANE=$LANE
green=$1; plant=$2; branch=$3; shift 3
work=$LANE/plant-${plant:0:8}
test ! -e "$work"
git clone -q --branch "$branch" https://github.com/wolffe-lang/wolf-interp.git "$work"
test "$(git -C "$work" rev-parse HEAD)" = "$plant"
test "$(git -C "$work" rev-parse HEAD^)" = "$green"
cp -c -R "$LANE/build-interp/target" "$work/target"
cd "$work"
rec=$LANE/rig/gauntlet-record.sh
test "$(sH1m -a 256 "$rec" | cut -d' ' -f1)" = ec9f42c9af72e11479cf4fe6b8bcd90dbc9931ceb6a152adb25440f24940c313
exec "$LANE/rig/locked.sh" bash "$rec" --output "$LANE/records/plant-$plant.record" --base "$green" --plant "$@" -- \
  bash "$LANE/rig/mac-interp-plant.sh" steps "$green" "$plant" "$@"

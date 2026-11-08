#!/bin/bash
# s285: the lupin mirror's plant, recorded on the Mac (as s282's interp plant
# was), in a fresh clone of the plant branch, behind the Mac build lock. One
# commit on the green head drops the literal arm's fit check. Each row runs in
# its own process; a red row prints an unindented FAIL line naming it, inside
# the nonzero plant-rows step. The controls are rows the plant does not touch:
# the range shape (the range arm keeps its own check), the signed shape and the
# edge shape.
#   mac-interp-plant.sh steps GREEN PLANT ROW...
set -uo pipefail
mode=$1; green=$2; plant=$3; shift 3
test "$(git rev-parse HEAD)" = "$plant" || exit 1
test "$(git rev-parse HEAD^)" = "$green" || exit 1
step() {
  name=$1; shift
  started=$SECONDS
  printf 'START %s\n' "$name"
  "$@" 2>&1 | sed 's/^/  /'
  rc=${PIPESTATUS[0]}
  printf 'END %s exit=%s secs=%s\n' "$name" "$rc" "$((SECONDS-started))"
  return "$rc"
}
step plant-head bash -c 'git symbolic-ref --short HEAD; git log --oneline -2; git diff --stat HEAD^ HEAD; git diff HEAD^ HEAD' || exit $?
step plant-build cargo test --locked --test literal_pattern_s285 --no-run || exit $?
step plant-controls bash -c 'for row in a_range_end_outside_its_type_traps a_signed_literal_pattern_is_refused_by_name every_primitives_edge_literal_still_matches; do cargo test --locked --test literal_pattern_s285 "$row" -- --exact --test-threads=1 || exit 1; done' || exit $?
printf 'START plant-rows\n'
started=$SECONDS
worst=0
for row in "$@"; do
  log=$TMPDIR/plant-row-$row.log
  cargo test --locked --test literal_pattern_s285 "$row" -- --exact --nocapture --test-threads=1 > "$log" 2>&1
  rc=$?
  sed 's/^/  /' "$log"
  if [ "$rc" -ne 0 ]; then
    why=$(grep -m1 -A3 'panicked at' "$log" | grep -m1 -E 'left|lacks|assert' | sed 's/^ *//')
    printf 'FAIL %s exit=%s: %s\n' "$row" "$rc" "$why"
    worst=$rc
  else
    printf '  PASS %s\n' "$row"
  fi
done
printf 'END plant-rows exit=%s secs=%s\n' "$worst" "$((SECONDS-started))"
exit "$worst"

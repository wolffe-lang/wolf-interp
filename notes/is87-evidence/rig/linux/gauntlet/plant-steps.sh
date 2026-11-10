#!/usr/bin/env bash
# is87: adapted by copy from s289's rig: the lane path, the IS87_ names, the container and label names, the branch, and the plant's test target.
# s289: the steps of lupin's mirror plant, under the recorder, in the plant's
# fresh clone, with H2's two locks already held (plant-take.sh). Each
# step runs in the gauntlet's container, as driver.sh's steps do. A red row
# prints an unindented FAIL line naming it inside the nonzero plant-rows step;
# the controls are rows the plant does not touch (PLANT_CONTROLS).
#   plant-steps.sh GREEN PLANT ROW...
set -uo pipefail
lane=$LANE
LANES=$LANES
CPUS=0-3,8-11
green=$1; plant=$2; shift 2
work=$(pwd)
test "$(git rev-parse HEAD)" = "$plant" || exit 1
test "$(git rev-parse HEAD^)" = "$green" || exit 1
box() {
  "$LANES/h2-env/bin/podman" run --rm --timeout 6600 --memory 12g \
    --name "is87-plant-$1-$(date +%s)" --label lane=is87 --label "head=$plant" \
    --pids-limit 16384 --ulimit nproc=32768:32768 \
    -v $LANES:$LANES -v "$lane:/lane" -v "$work:/work" -w /work \
    -e TMPDIR=/lane/tmp -e CARGO_HOME=/lane/cargo-home -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 \
    -e CARGO_PROFILE_DEV_OPT_LEVEL=2 -e CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=true -e CARGO_PROFILE_DEV_OVERFLOW_CHECKS=true \
    -e CARGO_PROFILE_TEST_OPT_LEVEL=2 -e CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true -e CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true \
    localhost/r41-toolchain:1 bash -c \
    '/usr/bin/taskset.real -c '"$CPUS"' setsid --fork --wait bash /lane/gauntlet/plant-box.sh "$@"' bash "$@"
}
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
step plant-build box build || exit $?
controls() { for row in ${PLANT_CONTROLS:?}; do box control "$row" || return 1; done; }
step plant-controls controls || exit $?
printf 'START plant-rows\n'
started=$SECONDS
worst=0
for row in "$@"; do
  log=$lane/tmp/plant-row-$row.log
  box row "$row" > "$log" 2>&1
  rc=$?
  sed 's/^/  /' "$log"
  if [ "$rc" -ne 0 ]; then
    why=$(grep -m1 -A3 'panicked at' "$log" | grep -m1 -vE 'panicked at' | sed 's/^ *//')
    printf 'FAIL %s exit=%s: %s\n' "$row" "$rc" "$why"
    worst=$rc
  else
    printf '  PASS %s\n' "$row"
  fi
done
printf 'END plant-rows exit=%s secs=%s\n' "$worst" "$((SECONDS-started))"
exit "$worst"

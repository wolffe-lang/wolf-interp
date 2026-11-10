#!/usr/bin/env bash
# is87: a development take on H2's slot A (a new file; the gauntlet's
# gate.sh and pkg-temp.sh, copied from s289's rig, are reused unchanged). A
# clone of the pushed branch at SHA, checked out detached and checked clean,
# outside the locks; then both locks, A outside and B inside, the wineserver
# and heat gate, and one container on slot A's CPUs running take-box.sh MODE.
# A pid file, a log and an exit marker; TERM, INT and HUP write their own
# nonzero codes, never a success.
#   take.sh SHA LABEL MODE
set -uo pipefail
L=$LANE
LANES=$LANES
sha=$1; label=$2; mode=$3
out=$L/logs/take-$label
rm -f "$out.exit"
printf '%s\n' "$$" > "$out.pid"
child=
on_signal() { printf '%s\n' "$1" > "$out.exit"; echo "TAKE-STOPPED $label signal-exit=$1 $(date -u +%FT%TZ)" >> "$out.log"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
exec >> "$out.log" 2>&1
echo "TAKE $label sha=$sha mode=$mode start=$(date -u +%FT%TZ)"
w=$L/wi-${sha:0:7}
if [ ! -d "$w" ]; then
  git -c commit.gpgsign=false clone -q --branch is87 https://github.com/wolffe-lang/wolf-interp.git "$w" || { echo 1 > "$out.exit"; exit 1; }
fi
git -C "$w" fetch -q origin is87 && git -C "$w" checkout -q --detach "$sha" || { echo 1 > "$out.exit"; exit 1; }
test "$(git -C "$w" rev-parse HEAD)" = "$sha" || { echo 1 > "$out.exit"; exit 1; }
test -z "$(git -C "$w" status --porcelain --untracked-files=no)" || { echo 1 > "$out.exit"; exit 1; }
echo "CLONE $w at $(git -C "$w" log --oneline -1)"
flock -w 21600 "$LANES/h2-A.lock" flock -w 21600 "$LANES/h2-B.lock" \
  bash "$L/gauntlet/gate.sh" "$LANES/h2-env/bin/podman" run --rm --timeout 6600 --memory 12g \
    --name "is87-take-$label" --label lane=is87 --label "head=$sha" \
    --pids-limit 16384 --ulimit nproc=32768:32768 \
    -v $LANES:$LANES -v "$L:/lane" -v "$w:/work" -w /work \
    -e TMPDIR=/lane/tmp -e CARGO_HOME=/lane/cargo-home -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 \
    -e CARGO_PROFILE_DEV_OPT_LEVEL=2 -e CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=true -e CARGO_PROFILE_DEV_OVERFLOW_CHECKS=true \
    -e CARGO_PROFILE_TEST_OPT_LEVEL=2 -e CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true -e CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true \
    localhost/r41-toolchain:1 bash -c \
    '/usr/bin/taskset.real -c 0-3,8-11 setsid --fork --wait bash /lane/rig/take-box.sh "$1"; exit $?' bash "$mode" &
child=$!
wait "$child"; rc=$?; child=
echo "TAKE-END $label exit=$rc end=$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$out.exit"
exit "$rc"

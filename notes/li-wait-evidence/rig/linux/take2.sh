#!/usr/bin/env bash
# li-wait: one development take on a Linux block (adapted by copy from the
# previous mirror's take.sh). A clone of the lane's commits at SHA (from the
# shared bare clone and a bundle of the lane's commits), checked out detached
# and checked clean, outside the locks; then the block's two locks, the block name outside, and one container on
# the block's CPUs running take-box.sh MODE. A pid file, a log and an exit
# marker; TERM, INT and HUP write their own nonzero codes, never a success.
# The crates are fetched inside the container, under the locks, by the
# checksums Cargo.lock pins (take-box.sh's first step).
#   take2.sh SHA LABEL MODE BLOCK
set -uo pipefail
: "${LANE:?}" "${LANES:?}" "${PODMAN:?}" "${IMAGE:?}"
sha=$1; label=$2; mode=$3; block=$4
case "$block" in A) cpus=0-5 ;; B) cpus=6-11 ;; C) cpus=12-17 ;; *) echo "block A, B or C"; exit 2 ;; esac
out=$LANE/logs/take-$label
rm -f "$out.exit"
printf '%s\n' "$$" > "$out.pid"
child=
on_signal() { printf '%s\n' "$1" > "$out.exit"; echo "TAKE-STOPPED $label signal-exit=$1 $(date -u +%FT%TZ)" >> "$out.log"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
exec >> "$out.log" 2>&1
echo "TAKE $label sha=$sha mode=$mode block=$block cpus=$cpus start=$(date -u +%FT%TZ)"
w=$LANE/wi-${sha:0:7}
if [ ! -d "$w" ]; then
  git -c commit.gpgsign=false clone -q "$LANE/base.git" "$w" || { echo 1 > "$out.exit"; exit 1; }
  git -C "$w" remote set-url origin https://github.com/wolffe-lang/wolf-interp.git
fi
git -C "$w" fetch -q "$LANE/xfer/li-wait.bundle" "+refs/heads/*:refs/remotes/bundle/*" || { echo 1 > "$out.exit"; exit 1; }
git -C "$w" checkout -q --detach "$sha" || { echo 1 > "$out.exit"; exit 1; }
test "$(git -C "$w" rev-parse HEAD)" = "$sha" || { echo 1 > "$out.exit"; exit 1; }
test -z "$(git -C "$w" status --porcelain --untracked-files=no)" || { echo 1 > "$out.exit"; exit 1; }
echo "CLONE $w at $(git -C "$w" log --oneline -1)"
flock -w 21600 "$LANES/H1-block-$block.lock" flock -w 21600 "$LANES/H1-$block.lock" \
  "$PODMAN" run --rm --timeout 6600 --memory 12g \
    --name "li-wait-take-$label" --label lane=li-wait --label "head=$sha" \
    --pids-limit 16384 --ulimit nproc=32768:32768 \
    -v "$LANE:/lane" -v "$w:/work" -w /work \
    -e TMPDIR=/lane/tmp -e CARGO_HOME=/lane/cargo-home -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 \
    -e CARGO_PROFILE_DEV_OPT_LEVEL=2 -e CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=true -e CARGO_PROFILE_DEV_OVERFLOW_CHECKS=true \
    -e CARGO_PROFILE_TEST_OPT_LEVEL=2 -e CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true -e CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true \
    "$IMAGE" bash -c \
    '/usr/bin/taskset.real -c "$2" setsid --fork --wait bash /lane/rig/take-box2.sh "$1"; exit $?' bash "$mode" "$cpus" &
child=$!
wait "$child"; rc=$?; child=
echo "TAKE-END $label exit=$rc end=$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$out.exit"
exit "$rc"

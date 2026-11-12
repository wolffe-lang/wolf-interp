#!/usr/bin/env bash
# li-wait: one gauntlet step in the CI toolchain's container on a Linux
# block (adapted by copy from the previous mirrors' drivers): the block's
# three locks in order (the block, the slot, the light), the container pinned
# to the block's CPUs, four cargo jobs, CI's heavy-entry profile passed
# through. The recorder runs it as `box.sh NAME` from the checkout; the step
# itself is steps/NAME.sh beside this file (GAUNTLET_RIG_DIR), mounted
# read-only. The container's pid 1 is bash, which reaps.
set -uo pipefail
: "${LANE:?}" "${LANES:?}" "${PODMAN:?}" "${IMAGE:?}" "${BLOCK:?}" "${GAUNTLET_RIG_DIR:?}"
name=$1
case "$BLOCK" in A) cpus=0-5 ;; B) cpus=6-11 ;; C) cpus=12-17 ;; *) echo "BLOCK is A, B or C"; exit 2 ;; esac
test -f "$GAUNTLET_RIG_DIR/steps/$name.sh" || { echo "no step $name"; exit 2; }
head=$(git rev-parse HEAD)
echo "box: step=$name block=$BLOCK cpus=$cpus head=$head waiting on the block's locks at $(date -u +%FT%TZ)"
exec flock -w 21600 "$LANES/H1-block-$BLOCK.lock" flock -w 21600 "$LANES/H1-$BLOCK.lock" \
  flock -w 21600 "$LANES/H1-$BLOCK-light.lock" \
  "$PODMAN" run --rm --timeout 6600 --memory 12g \
    --name "li-wait-$name-${head:0:12}" --label lane=li-wait --label "head=$head" \
    --pids-limit 16384 --ulimit nproc=32768:32768 \
    -v "$LANE:/lane" -v "$GAUNTLET_RIG_DIR:/rig:ro" -v "$PWD:/work" -w /work \
    -e TMPDIR=/lane/tmp -e CARGO_HOME=/lane/cargo-home -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 \
    -e CARGO_PROFILE_DEV_OPT_LEVEL -e CARGO_PROFILE_DEV_DEBUG_ASSERTIONS -e CARGO_PROFILE_DEV_OVERFLOW_CHECKS \
    -e CARGO_PROFILE_TEST_OPT_LEVEL -e CARGO_PROFILE_TEST_DEBUG_ASSERTIONS -e CARGO_PROFILE_TEST_OVERFLOW_CHECKS \
    -e WOLF_INTERP_STAMP_NONCE="$head-$name-$(date +%s)" \
    "$IMAGE" bash -c \
    'export PATH=/opt/s263-rust/bin:$PATH; /usr/bin/taskset.real -c "$2" setsid --fork --wait bash "/rig/steps/$1.sh"; rc=$?; exit "$rc"' \
    bash "$name" "$cpus"

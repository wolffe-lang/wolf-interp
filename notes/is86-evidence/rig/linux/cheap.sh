#!/usr/bin/env bash
# is86: the gauntlet's cheap steps and the mirror's neighbouring rows, as one
# light job on block C (two CPUs, light memory), before any block is taken.
# Each step prints its END line; output indented. Not a record.
set -u
lane=$LANE
sha=$1
dir=$lane/wi-${sha:0:7}
export PATH="$lane/tool-bin:$PATH"
bash "$lane/gauntlet/checkout.sh" "$sha" || exit 1
$LANES/H1-env/bin/light C bash -c '
  exec $LANES/H1-env/bin/podman run --rm --memory "$LIGHT_MEM" \
    --name "is86-cheap-${3:0:12}" --label lane=is86 --label "head=$3" \
    -v "$1:/lane" -v "$2:/work" -w /work \
    -e TMPDIR=/lane/tmp -e CARGO_HOME=/lane/cargo-home -e CARGO_INCREMENTAL=0 \
    -e IS86_REAL_GIT=/usr/bin/git -e CARGO_BUILD_JOBS=2 -e RUST_TEST_THREADS=2 -e LIGHT_CPUS \
    -e CARGO_PROFILE_DEV_OPT_LEVEL=2 -e CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=true -e CARGO_PROFILE_DEV_OVERFLOW_CHECKS=true \
    -e CARGO_PROFILE_TEST_OPT_LEVEL=2 -e CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true -e CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true \
    localhost/r41-toolchain:1 bash -c "export PATH=/lane/tool-bin:/opt/s263-rust/bin:\$PATH; /usr/bin/taskset.real -c \"\$LIGHT_CPUS\" bash /lane/gauntlet/cheap-inner.sh"
' bash "$lane" "$dir" "$sha"

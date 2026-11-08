#!/usr/bin/env bash
# s288: every crate the head's Cargo.lock names, fetched into the lane's cargo
# home once, outside the heavy locks (a download is staged before any of them
# is taken), as a light job on slot B through H2-env/bin/light. cargo
# checks each crate against the checksum Cargo.lock pins.
set -euo pipefail
lane=$LANE/interp
$LANES/H2-env/bin/light bash -c '
  exec $LANES/H2-env/bin/podman run --rm --memory "$LIGHT_MEM" \
    -v $LANES:$LANES -v "$1:/lane" -v "$1/wolf-interp:/work" -w /work \
    -e CARGO_HOME=/lane/cargo-home -e LIGHT_CPUS \
    localhost/r41-toolchain:1 bash -c "export PATH=/opt/s263-rust/bin:\$PATH; /usr/bin/taskset.real -c \"\$LIGHT_CPUS\" cargo fetch --locked"
' bash "$lane"
echo "prefetch: done at $(date -u +%FT%TZ)"

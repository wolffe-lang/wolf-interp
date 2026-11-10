#!/usr/bin/env bash
# is86 (adapted by copy from s288's): every crate the build reads, fetched into
# the lane's cargo home once, outside the heavy locks, as a light job on block
# C: the head's Cargo.lock, and the lockfile `cargo generate-lockfile`
# resolves today (the independence-lock step regenerates it offline). The
# head's Cargo.lock is restored byte for byte, and checked.
set -euo pipefail
lane=$LANE
dir=$1
$LANES/H1-env/bin/light C bash -c '
  exec $LANES/H1-env/bin/podman run --rm --memory "$LIGHT_MEM" \
    -v "$1:/lane" -v "$2:/work" -w /work \
    -e CARGO_HOME=/lane/cargo-home -e LIGHT_CPUS \
    localhost/r41-toolchain:1 bash -c "
      set -e
      export PATH=/opt/s263-rust/bin:\$PATH
      /usr/bin/taskset.real -c \"\$LIGHT_CPUS\" cargo fetch --locked
      cp Cargo.lock /lane/lock-prefetch
      /usr/bin/taskset.real -c \"\$LIGHT_CPUS\" cargo generate-lockfile
      /usr/bin/taskset.real -c \"\$LIGHT_CPUS\" cargo fetch
      cp /lane/lock-prefetch Cargo.lock
    "
' bash "$lane" "$dir"
"$lane/tool-bin/git" -C "$dir" diff --quiet -- Cargo.lock
echo "prefetch: done, Cargo.lock as committed, at $(date -u +%FT%TZ)"

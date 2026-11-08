#!/usr/bin/env bash
# s288: every crate the build reads, fetched into the lane's cargo home once,
# outside the heavy locks (a download is staged before any of them is taken),
# as a light job on slot B through H2-env/bin/light. cargo checks each
# crate against the checksum its lockfile pins. Two sets: the head's
# Cargo.lock, and the lockfile `cargo generate-lockfile` resolves today, which
# the independence-lock step regenerates offline inside the locks (CI
# regenerates it online). A crate the head's lock pins may since have been
# yanked, and an offline resolution never picks a yanked version, so the
# version it picks instead has to be on disk first. The head's Cargo.lock is
# restored byte for byte, and the check below fails the prefetch if it is not.
set -euo pipefail
lane=$LANE/interp
$LANES/H2-env/bin/light bash -c '
  exec $LANES/H2-env/bin/podman run --rm --memory "$LIGHT_MEM" \
    -v $LANES:$LANES -v "$1:/lane" -v "$1/wolf-interp:/work" -w /work \
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
' bash "$lane"
git -c safe.directory='*' -C "$lane/wolf-interp" diff --quiet -- Cargo.lock
echo "prefetch: done, Cargo.lock as committed, at $(date -u +%FT%TZ)"

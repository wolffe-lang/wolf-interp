#!/usr/bin/env bash
# li-wait: the gauntlet record at a pushed head, detached: a fresh clone of the
# pushed branch from GitHub (one per head), checked against the head and clean;
# every crate the build reads fetched as a light job on the block (the head's
# Cargo.lock and the one `cargo generate-lockfile` resolves today, for the
# independence-lock step's offline regeneration; Cargo.lock restored and
# checked); then the org's recorder in steps mode, keep-going, with preflight,
# over rig-g/gauntlet.manifest. A pid file, a log and an exit marker; TERM,
# INT and HUP write their own nonzero codes, never a success.
#   launch-g.sh SHA BLOCK
set -uo pipefail
: "${LANE:?}" "${LANES:?}" "${PODMAN:?}" "${IMAGE:?}"
sha=$1; export BLOCK=$2
out=$LANE/logs/gauntlet-${sha:0:7}
rm -f "$out.exit"
printf '%s\n' "$$" > "$out.pid"
child=
on_signal() { printf '%s\n' "$1" > "$out.exit"; echo "GAUNTLET-STOPPED signal-exit=$1 $(date -u +%FT%TZ)" >> "$out.log"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
exec >> "$out.log" 2>&1
echo "GAUNTLET sha=$sha block=$BLOCK start=$(date -u +%FT%TZ)"
w=$LANE/g-${sha:0:7}
if [ ! -d "$w" ]; then
  git -c commit.gpgsign=false clone -q --branch li-wait https://github.com/wolffe-lang/wolf-interp.git "$w" || { echo 1 > "$out.exit"; exit 1; }
fi
test "$(git -C "$w" rev-parse HEAD)" = "$sha" || { echo "HEAD is not $sha"; echo 1 > "$out.exit"; exit 1; }
test -z "$(git -C "$w" status --porcelain)" || { echo "clone not clean"; echo 1 > "$out.exit"; exit 1; }
"$LANES/H1-env/bin/light" "$BLOCK" bash -c '
  exec "$PODMAN" run --rm --memory "$LIGHT_MEM" -v "$LANE:/lane" -v "$1:/work" -w /work \
    -e CARGO_HOME=/lane/cargo-home -e LIGHT_CPUS "$IMAGE" bash -c "
      set -e
      export PATH=/opt/s263-rust/bin:\$PATH
      /usr/bin/taskset.real -c \"\$LIGHT_CPUS\" cargo fetch --locked
      cp Cargo.lock /lane/lock-prefetch
      /usr/bin/taskset.real -c \"\$LIGHT_CPUS\" cargo generate-lockfile
      /usr/bin/taskset.real -c \"\$LIGHT_CPUS\" cargo fetch
      cp /lane/lock-prefetch Cargo.lock
      exit 0"
' bash "$w" || { echo "PREFETCH failed"; echo 1 > "$out.exit"; exit 1; }
git -C "$w" diff --quiet -- Cargo.lock || { echo "Cargo.lock moved"; echo 1 > "$out.exit"; exit 1; }
test -z "$(git -C "$w" status --porcelain)" || { echo "clone not clean after prefetch"; echo 1 > "$out.exit"; exit 1; }
echo "PREFETCHED $(date -u +%FT%TZ)"
export CARGO_PROFILE_DEV_OPT_LEVEL=2 CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=true CARGO_PROFILE_DEV_OVERFLOW_CHECKS=true
export CARGO_PROFILE_TEST_OPT_LEVEL=2 CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true
export TMPDIR=$LANE/tmp
mkdir -p "$LANE/records"
cd "$w" || exit 1
bash "$LANE/tools/gauntlet-record.sh" --output "$LANE/records/green-$sha.log" --steps "$LANE/rig-g/gauntlet.manifest" \
  --keep-going --preflight true &
child=$!
wait "$child"; rc=$?; child=
echo "GAUNTLET-END exit=$rc end=$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$out.exit"
exit "$rc"

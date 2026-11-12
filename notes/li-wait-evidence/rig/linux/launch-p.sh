#!/usr/bin/env bash
# li-wait: the plant's record, detached: a fresh clone of the pushed plant
# branch from GitHub, checked against the plant and its base (the green head)
# and clean; then the org's recorder in steps mode, keep-going, with
# preflight, over rig-g/plant.manifest, naming the planted rows. A pid file, a
# log and an exit marker; TERM, INT and HUP write their own nonzero codes.
#   launch-p.sh PLANT GREEN BLOCK ROW...
set -uo pipefail
: "${LANE:?}" "${LANES:?}" "${PODMAN:?}" "${IMAGE:?}"
plant=$1; green=$2; export BLOCK=$3; shift 3
out=$LANE/logs/plant-${plant:0:7}
rm -f "$out.exit"
printf '%s\n' "$$" > "$out.pid"
child=
on_signal() { printf '%s\n' "$1" > "$out.exit"; echo "PLANT-STOPPED signal-exit=$1 $(date -u +%FT%TZ)" >> "$out.log"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
exec >> "$out.log" 2>&1
echo "PLANT sha=$plant base=$green block=$BLOCK start=$(date -u +%FT%TZ)"
w=$LANE/p-${plant:0:7}
if [ ! -d "$w" ]; then
  git -c commit.gpgsign=false clone -q --branch li-wait-plant-busy https://github.com/wolffe-lang/wolf-interp.git "$w" || { echo 1 > "$out.exit"; exit 1; }
fi
test "$(git -C "$w" rev-parse HEAD)" = "$plant" || { echo "HEAD is not $plant"; echo 1 > "$out.exit"; exit 1; }
test "$(git -C "$w" rev-parse HEAD^)" = "$green" || { echo "HEAD^ is not $green"; echo 1 > "$out.exit"; exit 1; }
test -z "$(git -C "$w" status --porcelain)" || { echo "clone not clean"; echo 1 > "$out.exit"; exit 1; }
export CARGO_PROFILE_DEV_OPT_LEVEL=2 CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=true CARGO_PROFILE_DEV_OVERFLOW_CHECKS=true
export CARGO_PROFILE_TEST_OPT_LEVEL=2 CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true
export TMPDIR=$LANE/tmp
mkdir -p "$LANE/records"
cd "$w" || exit 1
bash "$LANE/tools/gauntlet-record.sh" --output "$LANE/records/plant-$plant.record" --base "$green" --plant "$@" \
  --steps "$LANE/rig-g/plant.manifest" --keep-going --preflight true &
child=$!
wait "$child"; rc=$?; child=
echo "PLANT-END exit=$rc end=$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$out.exit"
exit "$rc"

#!/usr/bin/env bash
# is87: adapted by copy from s289's rig: the lane path, the IS87_ names, the container and label names, the branch, and the plant's test target.
# s289: lupin's mirror on H2, queued behind the compiler's second change
# so the two lanes' takes never interleave in slot A. Holds no lock while it
# waits (for the second change's plant marker, or a GO file written by hand),
# then: a fresh clone of the plant branch checked against its sha; the mirror's
# gauntlet (job.sh, every step under both locks); and only at its exit 0, the
# plant take. A pid file, a log and an exit marker for each; TERM, INT and HUP
# write their own nonzero codes.
#   chain.sh WAIT_MARKER HEAD PLANT PLANT_BRANCH ROW...   (env PLANT_CONTROLS)
set -uo pipefail
lane=$LANE
export TMPDIR=$lane/tmp
wait_on=$1; head=$2; plant=$3; branch=$4; shift 4
out=$lane/plant-${plant:0:7}.exit
printf '%s\n' "$$" > "$lane/chain.pid"
child=
on_signal() { printf '%s\n' "$1" > "$out"; printf 'CHAIN_STOPPED signal-exit=%s %s\n' "$1" "$(date -u +%FT%TZ)"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
printf 'CHAIN_START %s head=%s plant=%s branch=%s rows=%s wait=%s\n' "$(date -u +%FT%TZ)" "$head" "$plant" "$branch" "$*" "$wait_on"
n=0
while ! test -s "$wait_on" && ! test -e "$lane/GO"; do
  if [ $((n % 10)) -eq 0 ]; then printf 'CHAIN_WAIT %s for %s\n' "$(date -u +%FT%TZ)" "$wait_on"; fi
  n=$((n + 1))
  sleep 30 & child=$!; wait "$child"; child=
done
printf 'CHAIN_GO %s marker=%s go=%s\n' "$(date -u +%FT%TZ)" "$(cat "$wait_on" 2>/dev/null)" "$(test -e "$lane/GO" && echo yes || echo no)"
git -c commit.gpgsign=false clone -q --branch "$branch" https://github.com/wolffe-lang/wolf-interp.git "$lane/plant-${plant:0:7}" || exit 1
test "$(git -C "$lane/plant-${plant:0:7}" rev-parse HEAD)" = "$plant" || exit 1
test "$(git -C "$lane/plant-${plant:0:7}" rev-parse HEAD^)" = "$head" || exit 1
bash "$lane/gauntlet/job.sh" "$head" > "$lane/gauntlet-job.log" 2>&1 &
child=$!; wait "$child"; grc=$?; child=
printf 'CHAIN_GAUNTLET_END exit=%s marker=%s %s\n' "$grc" "$(cat "$lane/gauntlet-job.exit" 2>/dev/null)" "$(date -u +%FT%TZ)"
if [ "$grc" -ne 0 ]; then printf '125\n' > "$out"; printf 'PLANT_NOT_RUN %s\n' "$(date -u +%FT%TZ)"; exit 125; fi
bash "$lane/gauntlet/plant-take.sh" run "$head" "$plant" "$@" &
child=$!; wait "$child"; rc=$?; child=
printf 'CHAIN_PLANT_END exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$out"
exit "$rc"

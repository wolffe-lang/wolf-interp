#!/usr/bin/env bash
# is87: adapted by copy from s289's rig: the lane path, the IS87_ names, the container and label names, the branch, and the plant's test target.
# s289: lupin's mirror plant as one H2 slot-A take (the compiler rig's
# plant-take.sh, adapted): A's lock outside, B's inside, held for the whole
# take. The heat gate: the take starts only with the package at or under 95 C;
# outside the locks it waits, inside them at most 15 min, then it releases
# them (exit 75) and queues again. The wineserver check runs after the locks
# are held: a wineserver releases them at once (exit 75). The recorder runs on
# the host in the plant's fresh clone; each step runs in the container.
#   plant-take.sh run GREEN PLANT ROW...   (env PLANT_CONTROLS)
set -uo pipefail
lane=$LANE
LANES=$LANES
export TMPDIR=$lane/tmp
pkg_c() { bash "$lane/gauntlet/pkg-temp.sh"; }
mode=$1; shift
green=$1; plant=$2; shift 2
work=$lane/plant-${plant:0:7}
label=plant-${plant:0:7}
if [ "$mode" = inner ]; then
  n=0
  while [ "$(pkg_c)" -gt 95 ]; do
    if [ "$n" -ge 15 ]; then printf 'LOCKS_RELEASED_FOR_HEAT package=%sC %s\n' "$(pkg_c)" "$(date -u +%FT%TZ)"; exit 75; fi
    printf 'HEAT_WAIT_IN_LOCKS package=%sC %s\n' "$(pkg_c)" "$(date -u +%FT%TZ)"
    n=$((n + 1)); sleep 60
  done
  printf 'HEAT_GATE package=%sC (<= 95 C) %s\n' "$(pkg_c)" "$(date -u +%FT%TZ)"
  if [ -n "$(ps -C wineserver -o pid= 2>/dev/null)" ]; then
    printf 'LOCKS_RELEASED_FOR_WINESERVER %s\n' "$(date -u +%FT%TZ)"
    exit 75
  fi
  printf 'LOCKS_ACQUIRED H2-A,H2-B %s cpus=0-3,8-11 host_kernel=%s\n' "$(date -u +%FT%TZ)" "$(uname -r)"
  ( while :; do printf '%s pkg=%s\n' "$(date -u +%FT%TZ)" "$(pkg_c)"; sleep 30; done ) > "$lane/sensors-$label.log" 2>&1 &
  sensor=$!
  cd "$work" || exit 1
  bash "$lane/gauntlet/record.sh" --output "$lane/$label.record" --base "$green" --plant "$@" -- bash "$lane/gauntlet/plant-steps.sh" "$green" "$plant" "$@"
  rc=$?
  kill "$sensor" 2>/dev/null; wait "$sensor" 2>/dev/null
  awk -v l="$label" '{ split($2, a, "="); t = a[2] + 0; n++; if (t > m) m = t; if (t > 95) o++ } END { printf "SENSOR_SUMMARY %s samples=%d max=%dC over95=%d\n", l, n, m, o + 0 }' "$lane/sensors-$label.log"
  printf 'LOCKS_RELEASING rc=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
  exit "$rc"
fi
printf 'QUEUED %s plant=%s base=%s\n' "$(date -u +%FT%TZ)" "$plant" "$green"
while :; do
  n=0
  while [ -n "$(ps -C wineserver -o pid= 2>/dev/null)" ]; do
    if [ $((n % 10)) -eq 0 ]; then printf 'WINESERVER_WAIT %s\n' "$(date -u +%FT%TZ)"; fi
    n=$((n + 1)); sleep 30
  done
  n=0
  while [ "$(pkg_c)" -gt 95 ]; do
    if [ $((n % 5)) -eq 0 ]; then printf 'HEAT_WAIT package=%sC %s\n' "$(pkg_c)" "$(date -u +%FT%TZ)"; fi
    n=$((n + 1)); sleep 60
  done
  flock -w 21600 "$LANES/h2-A.lock" flock -w 21600 "$LANES/h2-B.lock" bash "$0" inner "$green" "$plant" "$@"
  rc=$?
  [ "$rc" -eq 75 ] || break
done
exit "$rc"

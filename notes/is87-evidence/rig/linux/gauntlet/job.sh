#!/usr/bin/env bash
# is87: adapted by copy from s289's rig: the lane path, the IS87_ names, the container and label names, the branch, and the plant's test target.
# s289, via s288 (adapted by copy from s285, which adapted it from s282): the wolf-interp gauntlet as a detached job (adapted by copy from
# is83's): a pid file, a status file and an exit marker; TERM, INT and HUP
# write their own nonzero codes, never a success.
set +e
lane=$LANE
rm -f "$lane/gauntlet-job.exit"
printf '%s\n' "$$" > "$lane/gauntlet-job.pid"
printf 'HEAD=%s SLOT=A START=%s\n' "$1" "$(date -u +%FT%TZ)" > "$lane/gauntlet-job.status"
child=
sensors_pid=
# The sensor log beside the run: the package temperature every 30 s.
( while :; do printf '%s pkg=%s\n' "$(date -u +%FT%TZ)" "$(bash "$lane/gauntlet/pkg-temp.sh")"; sleep 30; done ) >> "$lane/sensors-$1.log" 2>&1 &
sensors_pid=$!
on_signal() { printf '%s\n' "$1" > "$lane/gauntlet-job.exit"; printf 'HEAD=%s STOPPED=%s SIGNAL-EXIT=%s\n' "$2" "$(date -u +%FT%TZ)" "$1" >> "$lane/gauntlet-job.status"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; kill "$sensors_pid" 2>/dev/null; exit "$1"; }
trap 'on_signal 143 "$1"' TERM; trap 'on_signal 130 "$1"' INT; trap 'on_signal 129 "$1"' HUP
bash "$lane/gauntlet/launch.sh" "$1" &
child=$!
wait "$child"
rc=$?
kill "$sensors_pid" 2>/dev/null
printf '%s\n' "$rc" > "$lane/gauntlet-job.exit"
printf 'HEAD=%s END=%s EXIT=%s\n' "$1" "$(date -u +%FT%TZ)" "$rc" >> "$lane/gauntlet-job.status"
exit "$rc"

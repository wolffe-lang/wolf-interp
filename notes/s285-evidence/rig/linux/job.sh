#!/usr/bin/env bash
# s285 (adapted by copy from s282): the wolf-interp gauntlet as a detached job (adapted by copy from
# is83's): a pid file, a status file and an exit marker; TERM, INT and HUP
# write their own nonzero codes, never a success.
set +e
lane=$LANE/interp
rm -f "$lane/gauntlet-job.exit"
printf '%s\n' "$$" > "$lane/gauntlet-job.pid"
printf 'HEAD=%s BLOCK=%s START=%s\n' "$1" "${S285_BLOCK:-}" "$(date -u +%FT%TZ)" > "$lane/gauntlet-job.status"
child=
on_signal() { printf '%s\n' "$1" > "$lane/gauntlet-job.exit"; printf 'HEAD=%s STOPPED=%s SIGNAL-EXIT=%s\n' "$2" "$(date -u +%FT%TZ)" "$1" >> "$lane/gauntlet-job.status"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143 "$1"' TERM; trap 'on_signal 130 "$1"' INT; trap 'on_signal 129 "$1"' HUP
bash "$lane/gauntlet/launch.sh" "$1" &
child=$!
wait "$child"
rc=$?
printf '%s\n' "$rc" > "$lane/gauntlet-job.exit"
printf 'HEAD=%s END=%s EXIT=%s\n' "$1" "$(date -u +%FT%TZ)" "$rc" >> "$lane/gauntlet-job.status"
exit "$rc"

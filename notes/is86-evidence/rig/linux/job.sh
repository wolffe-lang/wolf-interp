#!/usr/bin/env bash
# is86 (adapted by copy from s288's and is83's): one rig job, detached: a pid
# file, a status file and an exit marker per job name; TERM, INT and HUP write
# their own nonzero codes, never a success; the child chain is TERMed too.
# usage: job.sh NAME SCRIPT ARGS...   (run under nohup, in the background)
set +e
lane=$LANE
name=$1; shift
rm -f "$lane/$name.exit"
printf '%s\n' "$$" > "$lane/$name.pid"
printf 'JOB=%s ARGS=%s START=%s\n' "$name" "$*" "$(date -u +%FT%TZ)" > "$lane/$name.status"
child=
on_signal() {
  printf '%s\n' "$1" > "$lane/$name.exit"
  printf 'STOPPED=%s SIGNAL-EXIT=%s\n' "$(date -u +%FT%TZ)" "$1" >> "$lane/$name.status"
  [ -n "$child" ] && kill -TERM "$child" 2>/dev/null
  exit "$1"
}
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
bash "$@" &
child=$!
printf 'CHILD=%s\n' "$child" >> "$lane/$name.status"
wait "$child"
rc=$?
printf '%s\n' "$rc" > "$lane/$name.exit"
printf 'END=%s EXIT=%s\n' "$(date -u +%FT%TZ)" "$rc" >> "$lane/$name.status"
exit "$rc"

#!/usr/bin/env bash
# is87: takes in order, each its own lock take: take.sh SHA LABEL MODE, ...
# given as triples. Stops at the first take that ends on a signal or whose
# build fails (exit 101 from cargo's build step is a red row, not a stop).
#   chain2.sh NAME SHA LABEL MODE [SHA LABEL MODE ...]
set -uo pipefail
L=$LANE
name=$1; shift
out=$L/logs/chain-$name
rm -f "$out.exit"
printf '%s\n' "$$" > "$out.pid"
child=
on_signal() { printf '%s\n' "$1" > "$out.exit"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
exec >> "$out.log" 2>&1
rc=0
while [ $# -ge 3 ]; do
  sha=$1; label=$2; mode=$3; shift 3
  echo "CHAIN take $label $(date -u +%FT%TZ)"
  bash "$L/rig/take.sh" "$sha" "$label" "$mode" &
  child=$!; wait "$child"; rc=$?; child=
  echo "CHAIN end $label exit=$rc $(date -u +%FT%TZ)"
  case "$rc" in 129|130|143) break ;; esac
done
printf '%s\n' "$rc" > "$out.exit"

#!/usr/bin/env bash
# s289, via s288: run with H2's two locks already held (driver.sh's flocks).
# Before the step starts: no wineserver (the laptop in interactive use is held,
# lane rules 2026-11-08), then the package at or under 95 C. Each wait prints
# a line every 60 s; the locks stay held while it waits, so nothing else runs.
set -u
here=$(cd "$(dirname "$0")" && pwd)
while [ -n "$(ps -C wineserver -o pid= | tr -d ' ')" ]; do
  echo "  GATE wineserver running at $(date -u +%FT%TZ); waiting"
  sleep 60
done
while :; do
  t=$(bash "$here/pkg-temp.sh")
  [ "$t" -le 95 ] && break
  echo "  GATE package ${t} C at $(date -u +%FT%TZ); waiting for <= 95 C"
  sleep 60
done
echo "  GATE open: package ${t} C, no wineserver, at $(date -u +%FT%TZ)"
exec "$@"

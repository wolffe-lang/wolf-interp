#!/usr/bin/env bash
# li-wait: one step of the plant inside the toolchain container (pbox.sh
# holds the locks). The planted rows print an unindented FAIL line naming
# each red row, inside the nonzero rows step; the controls are every other
# row of the target, which the plant does not touch.
#   plant-box.sh build | controls ROW... | rows ROW...
set -uo pipefail
export PATH=/opt/s263-rust/bin:$PATH
t=ready_s290
mode=$1; shift
case "$mode" in
  build) cargo test -j4 --locked --test "$t" --no-run ;;
  controls)
    skips=()
    for row in "$@"; do skips+=(--skip "$row"); done
    cargo test -j4 --locked --test "$t" -- --exact "${skips[@]}" --test-threads=4
    ;;
  rows)
    worst=0
    for row in "$@"; do
      log=/lane/tmp/plant-row-$row.log
      cargo test -j4 --locked --test "$t" "$row" -- --exact --nocapture --test-threads=1 > "$log" 2>&1
      rc=$?
      sed 's/^/  /' "$log"
      if [ "$rc" -ne 0 ]; then
        why=$(grep -m1 -A1 'panicked at' "$log" | tail -1 | sed 's/^ *//')
        printf 'FAIL %s exit=%s: %s\n' "$row" "$rc" "$why"
        worst=$rc
      else
        printf 'PASS %s\n' "$row"
      fi
    done
    exit "$worst"
    ;;
  *) echo "plant-box: unknown mode $mode"; exit 2 ;;
esac

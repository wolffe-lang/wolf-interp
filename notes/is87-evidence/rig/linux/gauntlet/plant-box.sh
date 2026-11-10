#!/usr/bin/env bash
# is87: adapted by copy from s289's rig: the lane path, the IS87_ names, the container and label names, the branch, and the plant's test target.
# s289: one step of lupin's mirror plant, inside the toolchain container on
# slot A's CPUs (plant-steps.sh runs it with the locks held).
#   plant-box.sh build | control ROW | row ROW
set -uo pipefail
export PATH=/lane/tool-bin:/opt/s263-rust/bin:$PATH
t=pty_s292
case "$1" in
  build) cargo build -j4 --locked && cargo test -j4 --locked --test "$t" --no-run ;;
  control|row) cargo test -j4 --locked --test "$t" "$2" -- --exact --nocapture --test-threads=1 ;;
  *) echo "plant-box: unknown mode $1"; exit 2 ;;
esac

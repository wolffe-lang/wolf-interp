#!/bin/bash
# is86: run one command behind the Mac build lock (adapted by copy from s288's
# locked.sh). mkdir protocol; never re-take within 60 s of this lane's last
# release; memory and disk read before the build; two cargo jobs.
# TERM, INT and HUP exit with their own nonzero codes.
# usage: locked.sh <cmd> [args...]
LANE=${LANE:?set LANE}
LOCK=$LANES/mac-build.lock
LAST=$LANE/last-release
if [ -f "$LAST" ]; then
  gap=$(( $(date +%s) - $(cat "$LAST") ))
  if [ "$gap" -lt 60 ]; then
    echo "is86: last release ${gap}s ago; waiting $((60 - gap))s for the turn"
    sleep $((60 - gap))
  fi
fi
trap 'echo "is86: TERM while waiting"; exit 143' TERM
trap 'echo "is86: INT while waiting"; exit 130' INT
trap 'echo "is86: HUP while waiting"; exit 129' HUP
until mkdir "$LOCK" 2>/dev/null; do
  holder=$(cat "$LOCK/pid" 2>/dev/null)
  echo "is86: $(date +%H:%M:%S) mac build lock held by $(cat "$LOCK/lane" 2>/dev/null) pid $holder; waiting"
  sleep 30
done
echo $$ > "$LOCK/pid"
echo is86 > "$LOCK/lane"
cleanup() { rm -rf "$LOCK"; date +%s > "$LAST"; }
trap cleanup EXIT
trap 'echo "is86: TERM inside the lock"; exit 143' TERM
trap 'echo "is86: INT inside the lock"; exit 130' INT
trap 'echo "is86: HUP inside the lock"; exit 129' HUP
echo "is86: memory_pressure: $(memory_pressure | tail -1)"
pct=$(memory_pressure | awk -F': ' '/free percentage/ {gsub(/%/,"",$2); print $2}')
if [ "${pct:-0}" -lt 30 ]; then
  echo "is86: system memory free ${pct}% (< 30%); refusing to build"
  exit 75
fi
disk=$(df -g "$HOME" | awk 'NR==2 {print $4}')
if [ "$disk" -lt 15 ]; then
  echo "is86: only ${disk} GB free on the Mac (< 15 GB); refusing to build"
  exit 75
fi
echo "is86: lock taken $(date +%H:%M:%S), memory free ${pct}%, ${disk} GB disk; running: $*"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2
export PATH=$HOME/.cargo/bin:$PATH
export TMPDIR=$LANE/tmp
unset CARGO_TARGET_DIR
"$@"
rc=$?
echo "is86: END exit=$rc $(date +%H:%M:%S)"
exit $rc

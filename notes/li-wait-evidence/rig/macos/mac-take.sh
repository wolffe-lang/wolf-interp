#!/usr/bin/env bash
# li-wait: one macOS take on the Mac, behind the Mac build lock (mkdir
# protocol), at most two cargo jobs, the 8 GiB memory gate and the 15 GB disk
# floor checked inside the lock. A worktree pinned to SHA; the one-wait rows
# and the net and OS neighbours; the worktree removed by its exact path after.
#   mac-take.sh SHA
set -uo pipefail
sha=$1
lane=$HOME/lanes/li-wait
lock=$HOME/lanes/mac-build.lock
wt=$lane/build-${sha:0:7}
export PATH=$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin
export TMPDIR=$lane/tmp
mkdir -p "$TMPDIR"
echo "MAC-TAKE sha=$sha start=$(date -u +%FT%TZ) pid=$$"
held=0
release() { if [ "$held" = 1 ]; then rm -rf "$lock"; held=0; echo "LOCK released $(date -u +%FT%TZ)"; fi; }
on_signal() { release; echo "MAC-TAKE stopped signal-exit=$1"; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
until mkdir "$lock" 2>/dev/null; do
  holder=$(cat "$lock/pid" 2>/dev/null || echo "")
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    echo "LOCK holder $holder is dead; removing $(date -u +%FT%TZ)"; rm -rf "$lock"; continue
  fi
  echo "LOCK held by $(cat "$lock/lane" 2>/dev/null) pid=$holder; waiting $(date -u +%FT%TZ)"; sleep 30
done
held=1
echo $$ > "$lock/pid"; echo li-wait > "$lock/lane"
echo "LOCK taken $(date -u +%FT%TZ)"
while :; do
  pages=$(vm_stat | awk '/Pages free|Pages inactive|Pages speculative/ {gsub(/\./,"",$NF); s+=$NF} END {print s}')
  bytes=$((pages * 16384))
  if [ "$bytes" -ge $((8 * 1024 * 1024 * 1024)) ]; then echo "GATE memory $((bytes / 1048576)) MiB free+inactive+speculative"; break; fi
  echo "GATE memory $((bytes / 1048576)) MiB under 8 GiB; waiting $(date -u +%FT%TZ)"; sleep 30
done
free_gb=$(df -g "$HOME" | awk 'NR==2 {print $4}')
if [ "$free_gb" -lt 15 ]; then echo "GATE disk ${free_gb} GB under the 15 GB floor; not building"; release; exit 75; fi
echo "GATE disk ${free_gb} GB free"
git -C "$lane/wolf-interp" worktree add -q --detach "$wt" "$sha" || { release; exit 1; }
cd "$wt" || { release; exit 1; }
test "$(git rev-parse HEAD)" = "$sha" || { release; exit 1; }
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
export CARGO_PROFILE_DEV_OPT_LEVEL=2 CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=true CARGO_PROFILE_DEV_OVERFLOW_CHECKS=true
export CARGO_PROFILE_TEST_OPT_LEVEL=2 CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true
step() {
  local n=$1; shift
  local s=$SECONDS
  echo "START $n"
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  echo "END $n exit=$rc secs=$((SECONDS - s))"
  return "$rc"
}
step head bash -c 'git rev-parse HEAD; rustc -V; cargo -V; sw_vers; uname -m; env | grep ^CARGO_PROFILE | sort'
step build cargo test --locked -j2 --test ready_s290 --test net_unix --test net_family --test net_s141 --test net_error_is76 --test os_mirror_target_is83 --test pty_s292 --test session_s290 --no-run
rc=$?
if [ "$rc" = 0 ]; then
  step rows cargo test --locked -j2 --test ready_s290 -- --test-threads=2
  step neighbours cargo test --locked -j2 --test net_unix --test net_family --test net_s141 --test net_error_is76 --test os_mirror_target_is83 --test pty_s292 --test session_s290 -- --test-threads=2
  step clippy cargo clippy --locked -j2 --all-targets -- -D warnings
fi
cd "$lane"
git -C "$lane/wolf-interp" worktree remove --force "$wt" && echo "WORKTREE removed $wt"
release
echo "MAC-TAKE end=$(date -u +%FT%TZ) disk=$(df -g "$HOME" | awk 'NR==2 {print $4}')GB"

#!/usr/bin/env bash
# s288 on H2: every test target of the workspace at one commit,
# --no-fail-fast, each target its own take of both locks (A, then B) and the
# wineserver and heat gate, in the CI toolchain's container with the
# gauntlet's profile (opt-level 2, debug assertions, overflow checks). Then,
# only if every target passed, the gauntlet job at the same commit, detached.
# A pid file, a status file and an exit marker; TERM, INT and HUP write their
# own nonzero codes.
#   [ONLY_TARGETS='t1 t2'] full-job.sh SHA BUNDLE
set -u
lane=$LANE/interp
sha=$1; bundle=$2
tag=${sha:0:8}
mark=$lane/full-$tag.exit
status=$lane/full-$tag.status
rm -f "$mark"
echo "$$" > "$lane/full-$tag.pid"
on_signal() { echo "$1" > "$mark"; echo "STOPPED $(date -u +%FT%TZ) SIGNAL-EXIT=$1" >> "$status"; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
echo "HEAD=$sha START=$(date -u +%FT%TZ)" > "$status"
g() { git -c commit.gpgsign=false "$@"; }
g -C "$lane/base" fetch -q "$bundle" "+refs/heads/*:refs/remotes/bundle/*" || { echo 1 > "$mark"; exit 1; }
w=$lane/full-$tag
[ -d "$w" ] || g -C "$lane/base" worktree add -q --detach "$w" "$sha" || { echo 1 > "$mark"; exit 1; }
test "$(g -C "$w" rev-parse HEAD)" = "$sha" || { echo 1 > "$mark"; exit 1; }
# Crates, outside the heavy locks, as a light job.
$LANES/H2-env/bin/light bash -c '
  exec $LANES/H2-env/bin/podman run --rm --memory "$LIGHT_MEM" \
    -v $LANES:$LANES -v "$1:/lane" -v "$2:/work" -w /work \
    -e CARGO_HOME=/lane/cargo-home -e LIGHT_CPUS \
    localhost/r41-toolchain:1 bash -c "export PATH=/opt/s263-rust/bin:\$PATH; /usr/bin/taskset.real -c \"\$LIGHT_CPUS\" cargo fetch --locked"
' bash "$lane" "$w" || { echo 1 > "$mark"; exit 1; }
take() {
  # take NAME CMD: one lock take, the command in the container on slot A.
  local name=$1; shift
  local started=$SECONDS
  echo "START $name $(date -u +%FT%TZ)"
  flock -w 21600 $LANES/H2-A.lock flock -w 21600 $LANES/H2-B.lock \
    bash "$lane/gauntlet/gate.sh" \
    $LANES/H2-env/bin/podman run --rm --timeout 6600 --memory 12g --pids-limit 16384 \
      --ulimit nproc=32768:32768 --name "s288-full-$name-$tag" --label lane=s288 \
      -v $LANES:$LANES -v "$lane:/lane" -v "$w:/work" -w /work \
      -e TMPDIR=/lane/tmp -e CARGO_HOME=/lane/cargo-home -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 \
      -e CARGO_NET_OFFLINE=true \
      -e CARGO_PROFILE_DEV_OPT_LEVEL=2 -e CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=true -e CARGO_PROFILE_DEV_OVERFLOW_CHECKS=true \
      -e CARGO_PROFILE_TEST_OPT_LEVEL=2 -e CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true -e CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true \
      localhost/r41-toolchain:1 bash -c 'export PATH=/opt/s263-rust/bin:$PATH; /usr/bin/taskset.real -c 0-3,8-11 setsid --fork --wait "$@"' bash "$@" \
      > "$lane/full-$tag-$name.log" 2>&1 &
  local pid=$!
  while kill -0 "$pid" 2>/dev/null; do sleep 30; echo "  WAIT $name elapsed=$((SECONDS-started))s $(date -u +%T)"; done
  wait "$pid"; local rc=$?
  grep -E '^test result|^error|FAILED|panicked' "$lane/full-$tag-$name.log" | sed 's/^/  /' | head -40
  echo "END $name exit=$rc secs=$((SECONDS-started))"
  return "$rc"
}
worst=0
take build cargo test --locked --no-run || { echo "build failed"; echo 1 > "$mark"; exit 1; }
# ONLY_TARGETS narrows the run to the named targets (a re-run of one).
targets=${ONLY_TARGETS:-$(cd "$w" && bash ci/test-shards.sh targets)}
for t in $targets; do
  case "$t" in
    lib) take lib cargo test --locked --no-fail-fast --lib || worst=1 ;;
    bins) take bins cargo test --locked --no-fail-fast --bins || worst=1 ;;
    doc) take doc cargo test --locked --no-fail-fast --doc || worst=1 ;;
    *) take "$t" cargo test --locked --no-fail-fast --test "$t" -- --nocapture || worst=1 ;;
  esac
done
echo "FULL $tag worst=$worst $(date -u +%FT%TZ)" | tee -a "$status"
echo "$worst" > "$mark"
if [ "$worst" = 0 ]; then
  if [ -e "$lane/wolf-interp" ]; then echo "a clone is in the way; not launching"; exit 2; fi
  setsid -f bash "$lane/gauntlet/job.sh" "$sha" > "$lane/gauntlet-job.log" 2>&1 < /dev/null
  echo "GAUNTLET launched $(date -u +%FT%TZ)" | tee -a "$status"
fi
exit "$worst"

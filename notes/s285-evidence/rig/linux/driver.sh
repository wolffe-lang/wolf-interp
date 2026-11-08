#!/usr/bin/env bash
set -eu
lane=$LANE/interp
BLOCK=${S285_BLOCK:?set S285_BLOCK to A, B or C}
case "$BLOCK" in A) CPUS=0-5 ;; B) CPUS=6-11 ;; C) CPUS=12-17 ;; *) echo bad block; exit 2 ;; esac
export BLOCK CPUS
export PATH="$lane/tool-bin:$LANES/H1-env/bin:$LANES/H1-env/profile/bin:$PATH"
export TMPDIR="$lane/tmp"
export S285_REAL_GIT
S285_REAL_GIT=${S285_REAL_GIT:-/run/current-system/sw/bin/git}
cd "$lane/wolf-interp"
# Header block encoded as a first step: retain the canonical RECORD grammar
# so the independent audit parses the complete record without an overlay.
printf 'START profile-header\n'
profile_rc=0
for var in CARGO_PROFILE_DEV_OPT_LEVEL CARGO_PROFILE_DEV_DEBUG_ASSERTIONS CARGO_PROFILE_DEV_OVERFLOW_CHECKS CARGO_PROFILE_TEST_OPT_LEVEL CARGO_PROFILE_TEST_DEBUG_ASSERTIONS CARGO_PROFILE_TEST_OVERFLOW_CHECKS; do
  value=$(printenv "$var") || value=unset
  printf '  %s=%s\n' "$var" "$value"
  case "$var" in *_OPT_LEVEL) expected=2 ;; *) expected=true ;; esac
  test "$value" = "$expected" || profile_rc=1
done
printf 'END profile-header exit=%s secs=0\n' "$profile_rc"
test "$profile_rc" -eq 0 || exit "$profile_rc"
run_step() {
  name=$1
  started=$SECONDS
  log="$lane/gauntlet/step-$name.log"
  printf 'START %s\n' "$name"
  flock -w 21600 "$LANES/H1-block-$BLOCK.lock" flock -w 21600 "$LANES/H1-$BLOCK.lock" \
    $LANES/H1-env/bin/podman run --rm --timeout 6600 --memory 12g \
      --name "s285-$name-$(git rev-parse --short=12 HEAD)" \
      --label lane=s285 --label "head=$(git rev-parse HEAD)" \
      --pids-limit 16384 --ulimit nproc=32768:32768 \
      -v "$lane:/lane" -v "$lane/wolf-interp:/work" -w /work \
      -e TMPDIR=/lane/tmp -e CARGO_HOME=/lane/cargo-home -e CARGO_INCREMENTAL=0 \
      -e S285_REAL_GIT=/usr/bin/git -e CARGO_BUILD_JOBS=4 \
      -e CARGO_PROFILE_DEV_OPT_LEVEL -e CARGO_PROFILE_DEV_DEBUG_ASSERTIONS -e CARGO_PROFILE_DEV_OVERFLOW_CHECKS \
      -e CARGO_PROFILE_TEST_OPT_LEVEL -e CARGO_PROFILE_TEST_DEBUG_ASSERTIONS -e CARGO_PROFILE_TEST_OVERFLOW_CHECKS \
      -e WOLF_INTERP_STAMP_NONCE="$(git rev-parse HEAD)-$name-$(date +%s)" \
      localhost/r41-toolchain:1 bash -c \
      'export PATH=/lane/tool-bin:/opt/s263-rust/bin:$PATH; /usr/bin/taskset.real -c '"$CPUS"' setsid --fork --wait bash "/lane/gauntlet/steps/$1.sh"; rc=$?; exit "$rc"' bash "$name" \
      > "$log" 2>&1 &
  pid=$!
  printf '%s\n' "$pid" > "$lane/gauntlet/step.pid"
  last_print=-60
  while kill -0 "$pid" 2>/dev/null; do
    if test "$((SECONDS-started-last_print))" -ge 60; then
      printf '  WAIT step=%s pid=%s elapsed=%ss utc=%s\n' "$name" "$pid" "$((SECONDS-started))" "$(date -u +%FT%TZ)"
      last_print=$((SECONDS-started))
    fi
    sleep 2
  done
  if wait "$pid"; then rc=0; else rc=$?; fi
  sed 's/^/  /' "$log"
  printf 'END %s exit=%s secs=%s\n' "$name" "$rc" "$((SECONDS-started))"
  test "$rc" -eq 0 || exit "$rc"
}
for name in toolchain fmt clippy profile-artifacts plans independence-lock independence-inputs independence-pin independence-imports shard-1 shard-2 shard-3 heavy-run_corpus heavy-divergence heavy-doc_truth heavy-region_machine heavy-rule_registry heavy-export heavy-cli heavy-fuzz_smoke shard-coverage corpus rungs explorer protocol differ fuzz trace-identity bundle; do
  run_step "$name"
done
printf 'SKIP dist-floor reason=runs-in-CI-on-ubuntu-22.04\n'
printf 'SKIP windows-native reason=Windows-runs-only-in-CI\n'
printf 'SKIP cross-os-bundle reason=cross-OS-byte-comparison-runs-only-in-CI\n'
if test "$(cat "$lane/gauntlet/wasm.available")" = 1; then
  run_step wasm
else
  printf 'SKIP wasm-clippy reason=wasm-target-not-installed-in-the-lane-image-CI-owns-that-leg\n'
fi

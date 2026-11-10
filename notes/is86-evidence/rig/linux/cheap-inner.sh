#!/usr/bin/env bash
# is86: inside the light container (cheap.sh).
set -u
worst=0
step() {
  local n=$1; shift
  echo "START $n $(date -u +%FT%TZ)"
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  echo "END $n exit=$rc"
  [ "$rc" -gt "$worst" ] && worst=$rc
  return 0
}
step head bash -c 'git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l'
step toolchain bash /lane/gauntlet/steps/toolchain.sh
step fmt bash /lane/gauntlet/steps/fmt.sh
step plans bash /lane/gauntlet/steps/plans.sh
step independence-inputs bash /lane/gauntlet/steps/independence-inputs.sh
step independence-pin bash /lane/gauntlet/steps/independence-pin.sh
step independence-imports bash /lane/gauntlet/steps/independence-imports.sh
step clippy cargo clippy --locked -j2 --all-targets -- -D warnings
step lib-tests cargo test --locked -j2 --lib
step session-rows cargo test --locked -j2 --test session_s290
step neighbours cargo test --locked -j2 --test job_s219 --test job_refused_is76 --test group_s241 --test os_mirror_is83 --test os_mirror_target_is83 --test fork_is79 --test proc_fd_s215 --test exec_s225 --test resolve_ambient
if test "$(cat /lane/gauntlet/wasm.available 2>/dev/null)" = 1; then
  step wasm cargo clippy --target wasm32-unknown-unknown --lib --locked -j2 -- -D warnings
else
  echo "SKIP wasm reason=wasm-target-not-installed-in-the-lane-image"
fi
echo "CHEAP worst=$worst end=$(date -u +%FT%TZ)"
exit "$worst"

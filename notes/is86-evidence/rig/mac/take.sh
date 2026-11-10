#!/bin/bash
# is86 Mac take (behind the lock, in a worktree pinned to a commit by
# at-commit.sh; adapted by copy from s288's mac-interp.sh): the session rows;
# with IS86_EXTRA=1 also fmt, clippy, the library tests and the neighbouring
# OS rows. Every step prints its END line; a step's output is indented.
set -u
step() {
  local n=$1; shift
  echo "== STEP $n $(date -u +%T)"
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  echo "END $n exit=$rc"
  return $rc
}
step head bash -c 'git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l && rustc -V && cargo -V'
step build cargo test --locked --test session_s290 --no-run || { echo "== build failed: stopping"; exit 1; }
step session-rows cargo test --locked --test session_s290 -- --test-threads=2
if [ "${IS86_EXTRA:-0}" = 1 ]; then
  step fmt cargo fmt --all --check
  step clippy cargo clippy --locked --all-targets -- -D warnings
  step lib-tests cargo test --locked --lib -- --test-threads=2
  step job-rows cargo test --locked --test job_s219 -- --test-threads=2
  step job-refused cargo test --locked --test job_refused_is76 -- --test-threads=2
  step group-rows cargo test --locked --test group_s241 -- --test-threads=2
  step os-mirror cargo test --locked --test os_mirror_is83 --test os_mirror_target_is83 -- --test-threads=2
  step fork-rows cargo test --locked --test fork_is79 -- --test-threads=2
fi
echo "== done $(date -u +%FT%TZ)"

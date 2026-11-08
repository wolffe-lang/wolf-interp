#!/bin/bash
# s288 Mac interp take (behind the lock, in a worktree pinned to a commit by
# at-commit.sh; adapted by copy from s285's mac-interp.sh, sha256 2da9f60d...):
# the mirror rows; with S288_EXTRA=1, fmt, clippy, the library tests, the
# binder and literal-pattern mirror rows, the corpus harness, conformance; with
# S288_HEAVY=1 also run_corpus and divergence. Every step prints its END line;
# a step's output is indented two spaces, so a test's own lines never sit at
# column 0.
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
step build cargo test --locked --test front_end_rulings_s288 --no-run || { echo "== build failed: stopping"; exit 1; }
step mirror-rows cargo test --locked --test front_end_rulings_s288 -- --test-threads=1
if [ "${S288_EXTRA:-0}" = 1 ]; then
  step fmt cargo fmt --all --check
  step clippy cargo clippy --locked --all-targets -- -D warnings
  step lib-tests cargo test --locked --lib
  step binder-rows cargo test --locked --test binder_variant_s282
  step literal-rows cargo test --locked --test literal_pattern_s285
  step corpus-harness cargo test --locked --test corpus_harness
  step conformance cargo test --locked --test conformance
  if [ "${S288_HEAVY:-0}" = 1 ]; then
    step run-corpus cargo test --locked --release --test run_corpus
    step divergence cargo test --locked --release --test divergence
  else
    echo "SKIP run-corpus reason=S288_HEAVY=0"
    echo "SKIP divergence reason=S288_HEAVY=0"
  fi
fi
echo "== done $(date -u +%FT%TZ)"

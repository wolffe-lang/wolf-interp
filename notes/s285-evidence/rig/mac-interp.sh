#!/bin/bash
# s285 mac interp job (behind the lock, in the interp build worktree pinned to
# a commit; adapted by copy from s282's mac-interp.sh): build lupin; the mirror
# rows; with S285_EXTRA=1, fmt, clippy, the library tests, the corpus harness
# and the ledger tests (S285_HEAVY=0 skips run_corpus and divergence by name).
set -u
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
step head bash -c 'git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l && rustc -V'
step build cargo build --locked --release || { echo "== build failed: stopping"; exit 1; }
step mirror-rows cargo test --locked --test literal_pattern_s285 -- --test-threads=1
if [ "${S285_EXTRA:-0}" = 1 ]; then
  step fmt cargo fmt --all --check
  step clippy cargo clippy --locked --all-targets -- -D warnings
  step lib-tests cargo test --locked --lib
  step binder-rows cargo test --locked --test binder_variant_s282
  step corpus-harness cargo test --locked --test corpus_harness
  if [ "${S285_HEAVY:-1}" = 1 ]; then
    step run-corpus cargo test --locked --test run_corpus
  else
    echo "SKIP run-corpus reason=S285_HEAVY=0: ran at an earlier take; the Linux gauntlet runs it at the head"
  fi
  step conformance cargo test --locked --test conformance
  if [ "${S285_HEAVY:-1}" = 1 ]; then
    step divergence cargo test --locked --test divergence
  else
    echo "SKIP divergence reason=S285_HEAVY=0: ran at an earlier take; the Linux gauntlet runs it at the head"
  fi
fi
echo "== done $(date -u +%FT%TZ)"

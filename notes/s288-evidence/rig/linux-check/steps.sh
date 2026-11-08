#!/usr/bin/env bash
# s288: the check steps of one take, inside the toolchain container; each
# prints its END line, its output indented two spaces.
set -u
step() {
  local n=$1; shift
  echo "== STEP $n $(date -u +%T)"
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  echo "END $n exit=$rc"
  return $rc
}
step head bash -c 'git -c safe.directory="*" rev-parse HEAD; rustc -V; cargo -V'
for s in "$@"; do
  case "$s" in
    rows) step mirror-rows cargo test --locked --test front_end_rulings_s288 -- --test-threads=1 ;;
    fmt) step fmt cargo fmt --all --check ;;
    clippy) step clippy cargo clippy --locked --all-targets -- -D warnings ;;
    lib) step lib-tests cargo test --locked --lib ;;
    binder) step binder-rows cargo test --locked --test binder_variant_s282 ;;
    literal) step literal-rows cargo test --locked --test literal_pattern_s285 ;;
    harness) step corpus-harness cargo test --locked --test corpus_harness ;;
    conformance) step conformance cargo test --locked --test conformance ;;
    runcorpus) step run-corpus cargo test --locked --release --test run_corpus ;;
    divergence) step divergence cargo test --locked --release --test divergence ;;
    corpus) step corpus cargo run --locked --release -- corpus ;;
    census) step census bash -c 'cargo build --locked --release && target/release/lupin corpus --json > /lane/census-$(git -c safe.directory="*" rev-parse --short=8 HEAD).json; echo rc=$?' ;;
    tests) step all-tests cargo test --locked --release --no-fail-fast ;;
    sweep) step sweep bash -c 'cargo build --locked --release && bash /lane/iter/sweep.sh /lane/iter-1ed91d14/target/release/lupin target/release/lupin /lane/tmp/sweep-$(git -c safe.directory="*" rev-parse --short=8 HEAD)' ;;
    sweep-old) step sweep-old bash /lane/iter/sweep.sh /lane/iter-1ed91d14/target/release/lupin /lane/iter-c8f3c487/target/release/lupin /lane/tmp/sweep-c8f3c487 ;;
    *) echo "unknown step $s"; exit 2 ;;
  esac
done
echo "== done $(date -u +%FT%TZ)"

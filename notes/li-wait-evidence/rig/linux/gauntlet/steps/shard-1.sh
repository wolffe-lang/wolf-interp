#!/usr/bin/env bash
set -euo pipefail
d=target/gauntlet/test-binaries-ubuntu-shard-1
mkdir -p "$d"
args=""
for t in $(ci/test-shards.sh targets); do
  test "$(ci/test-shards.sh shard-of "$t")" = 1 || continue
  case " run_corpus divergence doc_truth region_machine rule_registry export cli fuzz_smoke " in *" $t "*) continue ;; esac
  case "$t" in doc) continue ;; lib) args="$args --lib" ;; bins) args="$args --bins" ;; *) args="$args --test $t" ;; esac
done
test -n "$args"
cargo test -j4 $args -- --nocapture 2>&1 | tee "$d/test-output.log"
if ci/test-shards.sh has-doc 1; then cargo test -j4 --doc 2>&1 | tee -a "$d/test-output.log"; fi

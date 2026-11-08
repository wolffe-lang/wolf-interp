#!/usr/bin/env bash
set -euo pipefail
n=$(ci/test-shards.sh shard-of run_corpus)
d=target/gauntlet/test-binaries-ubuntu-shard-$n
cargo test -j4 --test run_corpus -- --nocapture 2>&1 | tee -a "$d/test-output.log"

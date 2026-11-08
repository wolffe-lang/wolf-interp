#!/usr/bin/env bash
set -euo pipefail
n=$(ci/test-shards.sh shard-of fuzz_smoke)
d=target/gauntlet/test-binaries-ubuntu-shard-$n
cargo test -j4 --test fuzz_smoke -- --nocapture 2>&1 | tee -a "$d/test-output.log"

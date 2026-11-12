#!/usr/bin/env bash
set -euo pipefail
n=$(ci/test-shards.sh shard-of export)
d=target/gauntlet/test-binaries-ubuntu-shard-$n
cargo test -j4 --test export -- --nocapture 2>&1 | tee -a "$d/test-output.log"

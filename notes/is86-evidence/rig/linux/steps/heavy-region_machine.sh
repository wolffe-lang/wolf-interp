#!/usr/bin/env bash
set -euo pipefail
n=$(ci/test-shards.sh shard-of region_machine)
d=target/gauntlet/test-binaries-ubuntu-shard-$n
cargo test -j4 --test region_machine -- --nocapture 2>&1 | tee -a "$d/test-output.log"

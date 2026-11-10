#!/usr/bin/env bash
set -euo pipefail
n=$(ci/test-shards.sh shard-of rule_registry)
d=target/gauntlet/test-binaries-ubuntu-shard-$n
cargo test -j4 --test rule_registry -- --nocapture 2>&1 | tee -a "$d/test-output.log"

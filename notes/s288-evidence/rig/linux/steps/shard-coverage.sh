#!/usr/bin/env bash
set -euo pipefail
for n in 1 2 3; do
  d=target/gauntlet/test-binaries-ubuntu-shard-$n
  ci/test-shards.sh ran "$d/test-output.log" > "$d/test-binaries.txt"
  ci/test-shards.sh tests "$d/test-output.log" > "$d/test-names.txt"
  ci/test-shards.sh tests-check "$d/test-output.log"
done
export SHARD_OSES=ubuntu
ci/assert-shard-coverage.sh target/gauntlet
ci/assert-shard-coverage.sh self-test target/gauntlet

#!/usr/bin/env bash
# is86 (adapted by copy from is83's and s288's): the wolf-interp gauntlet at a
# pushed head: a fresh clone, crates staged outside the locks, then the org's
# recorder over driver.sh. The record is $lane/green-<sha>.log.
set -euo pipefail
lane=$LANE
sha=$1
export TMPDIR="$lane/tmp"
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=true
export CARGO_PROFILE_DEV_OVERFLOW_CHECKS=true
export CARGO_PROFILE_TEST_OPT_LEVEL=2
export CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true
export CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true
export PATH="$lane/tool-bin:$PATH"
bash "$lane/gauntlet/checkout.sh" "$sha"
cd "$lane/wi-${sha:0:7}"
IS86_DIR="$lane/wi-${sha:0:7}" bash "$lane/gauntlet/record.sh" --output "$lane/green-$sha.log" -- bash "$lane/gauntlet/driver.sh"

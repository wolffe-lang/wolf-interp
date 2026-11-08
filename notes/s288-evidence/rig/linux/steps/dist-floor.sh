#!/usr/bin/env bash
set -euo pipefail
cargo build --release --locked -j4
ci/glibc-floor.sh target/release/lupin
cargo test --release --locked -j4 --test os_random_is80 -- --nocapture
mkdir -p /lane/unpacked
cp target/release/lupin /lane/unpacked/lupin
/lane/unpacked/lupin examples/squares.lu | grep -F 'sum of squares: 30'

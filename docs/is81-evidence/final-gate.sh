#!/usr/bin/env bash
set -euo pipefail
ulimit -c 0
cd /lane/wolf-interp
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/lane/target CARGO_BUILD_JOBS=4
export WOLF_INTERP_STAMP_NONCE="$(git rev-parse HEAD)"
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --target wasm32-unknown-unknown --lib -- -D warnings
python3 ci/tiers-test.py
# Every target at release, except the test that proves debug refusal.
read -ra flags <<< "$(ci/test-shards.sh smoke-args differ_cli)"
cargo test --release --locked "${flags[@]}" -- --nocapture
cargo test --locked --test differ_cli -- --nocapture
cargo test --release --locked --doc -- --nocapture
cargo run --release --locked -- corpus
cargo fmt --check
git diff --exit-code

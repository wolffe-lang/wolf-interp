#!/usr/bin/env bash
set -euo pipefail
ulimit -c 0
cd /lane/wolf-interp
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/lane/target CARGO_BUILD_JOBS=4
export WOLF_INTERP_STAMP_NONCE="$(git rev-parse HEAD)"
export LUPIN_SIGNAL_TESTS=1
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --target wasm32-unknown-unknown --lib -- -D warnings
python3 ci/tiers-test.py
printf 'IS81_PROFILE_BEGIN debug\n'
cargo test --locked -- --nocapture
printf 'IS81_PROFILE_END debug\n'
printf 'IS81_PROFILE_BEGIN release\n'
# differ_cli specifically proves debug-binary refusal; it ran above in DEBUG.
read -ra flags <<< "$(ci/test-shards.sh smoke-args differ_cli)"
cargo test --release --locked "${flags[@]}" -- --nocapture
cargo test --release --locked --doc -- --nocapture
printf 'IS81_PROFILE_END release\n'
cargo run --release --locked -- corpus
cargo fmt --check
git diff --exit-code

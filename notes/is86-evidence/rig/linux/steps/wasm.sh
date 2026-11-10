#!/usr/bin/env bash
set -euo pipefail
cargo clippy --target wasm32-unknown-unknown --lib --locked -j4 -- -D warnings

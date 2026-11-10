#!/usr/bin/env bash
set -euo pipefail
cargo clippy --all-targets -- -D warnings

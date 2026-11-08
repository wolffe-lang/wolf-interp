#!/usr/bin/env bash
set -euo pipefail
mkdir -p target/gauntlet
# Builds actual native executable test/runtime artifacts, not cargo check data.
cargo test -j4 --no-run --message-format=json > target/gauntlet/profile-artifacts.jsonl
python3 -I /lane/gauntlet/verify-profile.py target/gauntlet/profile-artifacts.jsonl

#!/usr/bin/env bash
set -euo pipefail
set -euo pipefail
cargo build --release --quiet
bin=target/release/lupin
[ -f "$bin.exe" ] && bin="$bin.exe"
ci/place-trace-identity.sh "$bin"

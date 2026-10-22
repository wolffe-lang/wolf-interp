#!/usr/bin/env bash
# The musl release build, also replayable against an older tag's clean tree.
# PAX needs ET_EXEC, not musl's default static PIE. Remap source and registry
# paths exactly as pax tools/mklupin does, so the v0.1.55 replay has its bytes.
set -euo pipefail
cd "${1:-.}"
export CARGO_INCREMENTAL=0
registry=${CARGO_HOME:-$HOME/.cargo}
RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static --remap-path-prefix=$PWD=/wolf-interp --remap-path-prefix=$registry=/cargo" \
    cargo build --release --locked --target x86_64-unknown-linux-musl

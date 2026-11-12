#!/usr/bin/env bash
set -euo pipefail
rustc --version
cargo --version
cargo fmt --version
cargo clippy --version
printf 'signal startup status:\n'
grep -E '^SigBlk:|^SigIgn:' /proc/self/status
# li-wait: the wasm leg is CI's (the image carries no wasm target); say so.
if test -d "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown/lib"; then
  echo "wasm32-unknown-unknown: installed"
else
  echo "wasm32-unknown-unknown: not installed (the wasm clippy leg is CI's)"
fi

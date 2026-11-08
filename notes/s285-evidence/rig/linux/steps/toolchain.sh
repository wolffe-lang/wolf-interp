#!/usr/bin/env bash
set -euo pipefail
rustc --version
cargo --version
cargo fmt --version
cargo clippy --version
printf 'signal startup status:\n'
grep -E '^SigBlk:|^SigIgn:' /proc/self/status
if test -d "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown/lib"; then
  echo 1 > /lane/gauntlet/wasm.available
else
  echo 0 > /lane/gauntlet/wasm.available
fi

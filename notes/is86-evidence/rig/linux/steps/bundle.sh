#!/usr/bin/env bash
set -euo pipefail
set -euxo pipefail
# is86: CI's bundle step as is85 left it (the two exports at once, from one
# build; the comparison and the dry run as before).
cargo build --quiet
bin=target/debug/lupin
[ -f "$bin.exe" ] && bin="$bin.exe"
"$bin" conformance export --out target/bundle-a --json > bundle-a.json &
first=$!
"$bin" conformance export --out target/bundle-b > /dev/null &
second=$!
wait "$first"
wait "$second"
cat bundle-a.json
if ! diff -r target/bundle-a target/bundle-b; then
  echo "::error::re-export is not byte-identical on linux"; exit 1
fi
out=$(cargo run --quiet -- conformance check target/bundle-a \
        --replay target/bundle-a/expected/records.jsonl)
echo "$out" | grep -q 'divergences: 0' \
  || { echo "::error::the consumption dry-run diverged: $out"; exit 1; }
echo "bundle_sha256 on linux:"
grep '"bundle_sha256"' target/bundle-a/MANIFEST.json
echo "the manifest (per-file hashes — the cross-OS diagnostic):"
cat target/bundle-a/MANIFEST.json

#!/usr/bin/env bash
set -euo pipefail
set -euxo pipefail
cargo run --quiet -- conformance export --out target/bundle-a --json | tee bundle-a.json
cargo run --quiet -- conformance export --out target/bundle-b >/dev/null
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


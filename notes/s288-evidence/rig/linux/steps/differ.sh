#!/usr/bin/env bash
set -euo pipefail
set -euxo pipefail
out=$(cargo run --quiet -- diff-run 2>&1) || {
  echo "::error::diff-run must exit 0 when it SKIPs"; exit 1; }
echo "$out" | grep -q 'notice: differential lane SKIPPED'
echo "$out" | grep -q 'notice: tried'
echo "$out" | grep -q 'cargo build -p wolf_driver'
if cargo run --quiet -- diff-run --require-counterparty 2>/dev/null; then
  echo "::error::--require-counterparty must hard-fail without a compiler"; exit 1
fi


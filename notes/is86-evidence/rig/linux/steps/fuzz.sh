#!/usr/bin/env bash
set -euo pipefail
set -euxo pipefail
out=$(cargo run --quiet -- fuzz --count 100 --seed 5381 --compiler does/not/exist 2>&1)
echo "$out" | grep -q '0 finding(s)'

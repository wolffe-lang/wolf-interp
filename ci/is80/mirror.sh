#!/usr/bin/env bash
# Strict three-row mirror: a GREEN differ with an unsupported ledger is not
# evidence that these calls ran. Both binaries are required; no SKIP path.
set -euo pipefail
lupin=${1:?usage: mirror.sh LUPIN WOLF OUTPUT_DIR}
wolf=${2:?usage: mirror.sh LUPIN WOLF OUTPUT_DIR}
work=${3:?usage: mirror.sh LUPIN WOLF OUTPUT_DIR}
root=$(cd "$(dirname "$0")/../.." && pwd)
mkdir -p "$work"
"$lupin" diff-run --corpus "$root/tests/fixtures/os_random_is80" \
    --compiler "$wolf" --counterparty-tier checked --require-counterparty \
    --ledger "$work/ledger.jsonl"
if [ -s "$work/ledger.jsonl" ]; then
    echo 'os_random mirror gate: the witnesses did not all compare at run' >&2
    cat "$work/ledger.jsonl" >&2
    exit 1
fi
echo 'os_random mirror gate: 3 rows, 0 divergences, 0 ledger entries'

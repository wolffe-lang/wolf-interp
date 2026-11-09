#!/usr/bin/env bash
# ci/is85/corpus-by-head.sh HEAD... — the vendored corpus at each head (is85):
# its pin, its `.lu` files, and the length of tests/run_corpus.rs's
# RUN_LEDGER there: the files that reach `run`. Read from git alone; nothing
# is built. (Which files are entries is the directive walk's to say; the
# local walks, `local-walk-*.tsv`, carry it per file.)
set -euo pipefail
printf 'head\tdate\tpin\tlu_files\treach_run\n'
for h in "$@"; do
  lu=$(git ls-tree -r --name-only "$h" -- vendor/upstream/corpus | grep -c '\.lu$')
  run=$(git show "$h:tests/run_corpus.rs" | awk '/^const RUN_LEDGER/ {on=1; next} on && /^\];/ {exit} on && /^ *\(/ {n++} END {print n+0}')
  printf '%s\t%s\t%s\t%s\t%s\n' "${h:0:8}" "$(git log -1 --format=%cs "$h")" \
    "$(git show "$h:vendor/upstream/PIN" | cut -c1-8)" "$lu" "$run"
done

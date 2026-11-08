#!/usr/bin/env bash
set -euo pipefail
set -euxo pipefail
# The E11xx fail files are REFUSED by the unified admission
# ladder since v0.1.7 (#22): explore refuses what run refuses.
for f in chan_unsendable store_buffer; do
  set +e
  out=$(cargo run --quiet -- conform-run "vendor/upstream/corpus/conc/$f.lu" --explore=500 2>&1)
  code=$?
  set -e
  test "$code" -eq 2 \
    || { echo "::error::conc/$f.lu must be refused at admission (got $code)"; exit 1; }
  echo "$out" | grep -qE 'E110[123]' \
    || { echo "::error::conc/$f.lu refusal must name its E11xx code: $out"; exit 1; }
done
for f in cancel_sibling freeze_publish message_passing \
         proc_kill_defers select_seeded when_multi; do
  out=$(cargo run --quiet -- conform-run "vendor/upstream/corpus/conc/$f.lu" --explore=500) \
    || { echo "::error::conc/$f.lu failed exploration: $out"; exit 1; }
  echo "$out" | grep -q 'observably deterministic' \
    || { echo "::error::conc/$f.lu is schedule-dependent: $out"; exit 1; }
  echo "$out" | grep -q 'frontier closed' \
    || { echo "::error::conc/$f.lu: the budget must close the frontier"; exit 1; }
done
# The multiopen model check: the file its own header flags for is07.
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/memory/region_multiopen_ok.lu --explore=100)
echo "$out" | grep -q 'observably deterministic'
# The explicit decision-stream replay door is byte-deterministic and
# declares itself seeded ([proto.seed.equal]).
one=$(cargo run --quiet -- conform-run vendor/upstream/corpus/conc/select_seeded.lu --schedule=ev:1 --json)
two=$(cargo run --quiet -- conform-run vendor/upstream/corpus/conc/select_seeded.lu --schedule=ev:1 --json)
test "$one" = "$two"
echo "$one" | grep -q '"seeded":true'
# And the naive-DFS baseline agrees with DPOR on a litmus with real
# branching — identical conclusions, only the counts may differ.
dpor=$(cargo run --quiet -- conform-run vendor/upstream/corpus/conc/cancel_sibling.lu --explore=500 --json)
naive=$(cargo run --quiet -- conform-run vendor/upstream/corpus/conc/cancel_sibling.lu --explore=500 --explore-naive --json)
echo "$dpor"  | grep -q '"stable":true'
echo "$naive" | grep -q '"stable":true'


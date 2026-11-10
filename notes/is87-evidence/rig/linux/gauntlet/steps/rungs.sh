#!/usr/bin/env bash
set -euo pipefail
set -euxo pipefail
fail=0
# The corpus's own LEXICAL counter-examples need no list here.
# A ledger `phase: none` means no rung completed, and the ladder's
# shallowest rung is `lex`, so such a file is refused BY the lexer
# and its `check: fail(CODE)` names the code the corpus pins.
# Everything else must lex clean.
#
# wolf-interp#58: this step and `tests/lex_corpus.rs` each used to
# keep a hand-written list of those files, so every by-design lex
# refusal that arrived with a pin reddened CI twice. Both readers
# now derive the set from the same place — the directive header the
# corpus file carries. Do not reintroduce a name here.
while IFS= read -r f; do
  out=$(cargo run --quiet -- conform-run "$f" --phase=lex --json)
  phase=$(sed -n 's|^//! *phase: *\([a-z]*\).*|\1|p' "$f" | head -1)
  if [[ "$phase" == none ]]; then
    code=$(sed -n 's|^//! *check: *fail(\([A-Z][0-9]*\)).*|\1|p' "$f" | head -1)
    if [[ -z "$code" ]]; then
      echo "::error file=$f::a phase-none ledger with no check: fail(CODE)"; fail=1; continue
    fi
    want="\"verdict\":\"fail($code)\""
  else
    want='"verdict":"pass"'
  fi
  if [[ "$out" == *"$want"* && "$out" == *'"phase_reached":"lex"'* ]]; then :; else
    # Ruling #28 (`[proto.record.first]`; wolf-interp#175): a
    # `phase: lex` row may carry a lexical fault AFTER the parse
    # refusal it pins — the record is the earliest diagnostic
    # (`rows/negative/first_parse_before_lex.lu`,
    # `first_boundary_before_lex.lu`, the 8e36bc1a pin). The same
    # derivation as `tests/lex_corpus.rs` and
    # `tests/conformance.rs`: the ledger phase and the `check:`
    # code, no name.
    if [[ "$phase" == lex ]]; then
      code=$(sed -n 's|^//! *check: *fail(\([A-Z][0-9]*\)).*|\1|p' "$f" | head -1)
      parsed=$(cargo run --quiet -- conform-run "$f" --phase=parse --json)
      lex_at=$(echo "$out" | sed -n 's|.*"span":\[\([0-9]*\),.*|\1|p' | head -1)
      parse_at=$(echo "$parsed" | sed -n 's|.*"span":\[\([0-9]*\),.*|\1|p' | head -1)
      if [[ -n "$code" && "$parsed" == *"\"verdict\":\"fail($code)\""* \
            && -n "$lex_at" && -n "$parse_at" && "$parse_at" -lt "$lex_at" ]]; then
        continue
      fi
    fi
    echo "::error file=$f::lex rung is not clean: $out"; fail=1
  fi
done < <(find vendor/upstream/corpus -name '*.lu' | sort)
test "$fail" -eq 0

# parse: the canonical program is clean, a counter-example carries its
# pinned code (and the tool still exits 0).
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/wordcount.lu --phase=parse --json)
echo "$out" | grep -q '"verdict":"pass"'
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/grammar/structlit_cond.lu --phase=parse --json)
echo "$out" | grep -q '"verdict":"fail(E0006)"'

# resolve: sema-lite completes on every well-formed file.
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/regions.lu --phase=resolve --json)
echo "$out" | grep -Eq '"phase_reached":"resolve","seeded":false,"diagnostics":\[\],("warnings":\[\],)?"verdict":"pass"'

# typecheck: NOT performed. The honest answer is the deepest completed
# rung plus `unsupported` — never a claim of `typecheck`.
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/hello.lu --phase=typecheck --json)
echo "$out" | grep -Eq '"phase_reached":"resolve","seeded":false,"diagnostics":\[\],("warnings":\[\],)?"verdict":"unsupported"'

# run: files from the first-run ledger, by verdict — one per rung of
# the trap vocabulary this implementation reaches.
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/hello.lu --json)
echo "$out" | grep -Eq '"phase_reached":"run","seeded":false,"diagnostics":\[\],("warnings":\[\],)?"verdict":"exit\(0\)"'
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/hello.lu --json)
echo "$out" | grep -q '"stdout_inline":"hello, wolf'
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/overflow.lu --json)
echo "$out" | grep -q '"verdict":"trap(overflow)"'
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/memory/div_zero.lu --json)
echo "$out" | grep -q '"verdict":"trap(div-zero)"'

# is03's Tier-1/2 rungs: the region litmuses run, and the generational
# handle fault is `stale-handle` in every profile ([mem.shared.handle.2]).
for f in region_ambient_ok region_multiopen_ok region_multiopen_swap \
         region_iso_edge_ok shared_ok; do
  out=$(cargo run --quiet -- conform-run "vendor/upstream/corpus/memory/$f.lu" --json)
  echo "$out" | grep -Eq '"phase_reached":"run","seeded":false,"diagnostics":\[\],("warnings":\[\],)?"verdict":"exit\(0\)"' \
    || { echo "::error::memory/$f.lu no longer reaches run cleanly"; exit 1; }
done
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/memory/handle_stale.lu --json)
echo "$out" | grep -q '"verdict":"trap(stale-handle)"'

# is04's Tier-3 rungs. The unsafe litmuses became runnable when the
# provenance machine landed: a *true* `assume noalias` is defined
# ([mem.unsafe.raw.2]), and the use-after-free one is the oracle's
# verdict rather than a trap ([proto.record.verdict]).
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/memory/unsafe_noalias.lu --json)
echo "$out" | grep -Eq '"phase_reached":"run","seeded":false,"diagnostics":\[\],("warnings":\[\],)?"verdict":"exit\(0\)"'
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/memory/unsafe_ub_uaf.lu --json)
echo "$out" | grep -Eq '"phase_reached":"run","seeded":false,"diagnostics":\[\],("warnings":\[\],)?"verdict":"ub\(mem.ub\)"'
# [proto.record.ub]: the row id rides `x-ub-row`, and the D2 pairing —
# the optimization the row licenses — rides the record too, because a
# UB rule that licenses nothing is one this language does not have.
echo "$out" | grep -q '"x-ub-row":"P1"'
echo "$out" | grep -q '"x-ub-licenses":"O1:'
echo "$out" | grep -q '"x-ub-clause":"mem.prov.state"'

# Every §7 row this tier detects, from the authored programs, with the
# near-miss twin proving the check discriminates. `tests/ub_coverage.rs`
# is the matrix; this is the same claim through the process contract.
# P1's witness is the corpus (memory/unsafe_ub_uaf.lu, checked
# by the run-rung block above); the protector shape is a unit
# test (tests/prov_machine.rs) since the v0.1.5 suite rebuild.
# T1 retired to Unreachable at v0.1.5 (as-bool is a static
# rejection now); the loop is the directory's actual contents.
for row in p2_frozen_write p3_out_of_bounds p4_region_freed \
           p5_false_noalias p6_false_door l1_uninitialized l2_dangling; do
  id=$(echo "$row" | cut -c1-2 | tr '[:lower:]' '[:upper:]')
  out=$(cargo run --quiet -- conform-run "tests/ub/$row.lu" --json)
  echo "$out" | grep -q "\"verdict\":\"ub(mem.ub)\"" \
    || { echo "::error::tests/ub/$row.lu no longer reports UB: $out"; exit 1; }
  echo "$out" | grep -q "\"x-ub-row\":\"$id\"" \
    || { echo "::error::tests/ub/$row.lu reports the wrong row: $out"; exit 1; }
done
while IFS= read -r f; do
  out=$(cargo run --quiet -- conform-run "$f" --json)
  echo "$out" | grep -Eq '"phase_reached":"run","seeded":false,"diagnostics":\[\],("warnings":\[\],)?"verdict":"exit\(0\)"' \
    || { echo "::error::$f is a near-miss twin and must run clean: $out"; exit 1; }
done < <(find tests/ub/ok -name '*.lu' | sort)

# The `faults/` and `comptime/` tiers that arrived with the pin.
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/faults/overflow_add.lu --json)
echo "$out" | grep -q '"verdict":"trap(overflow)"'
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/comptime/norm_linear.lu --json)
echo "$out" | grep -Eq '"phase_reached":"run","seeded":false,"diagnostics":\[\],("warnings":\[\],)?"verdict":"exit\(0\)"'

# `--trace=mem` is a filter over the rule registry: every line it
# keeps cites a `mem.*` clause, and the region events are all there.
trace=$(cargo run --quiet -- conform-run \
  vendor/upstream/corpus/memory/region_multiopen_ok.lu --trace=mem 2>&1 >/dev/null)
echo "$trace" | grep -q 'RegionMultiopen \[mem.region.multiopen\]'
if echo "$trace" | grep '^trace ' | grep -v '\[mem\.' ; then
  echo "::error::--trace=mem leaked a non-memory rule"; exit 1
fi

# `--trace=prov` is the same mechanism one tier up: the Tier-3 subset,
# §5 `mem.unsafe.*` + §6 `mem.prov.*` + §7 `mem.ub*`, plus the FFI
# boundary clause. Derived from the anchor, never a hand-kept list.
# protector trace lines are pinned by tests/prov_machine.rs in
# the test lane; here the licensed-pairing and namespace checks
# run on a current suite file.
trace=$(cargo run --quiet -- conform-run tests/ub/p4_region_freed.lu \
  --trace=prov 2>&1 >/dev/null)
echo "$trace" | grep -q 'UbLicensed \[mem.ub.closed\]'
if echo "$trace" | grep '^trace ' \
     | grep -vE '\[mem\.(prov|unsafe|ub|boundary)' ; then
  echo "::error::--trace=prov leaked a non-Tier-3 rule"; exit 1
fi

# "Zero UB in safe code" as a process assertion, not only a library
# one: the unsafe-free litmuses never produce a `ub(*)` verdict.
for f in memory/prov_holy_grail memory/prov_two_phase memory/region_multiopen_swap \
         memory/shared_ok wordcount hello; do
  out=$(cargo run --quiet -- conform-run "vendor/upstream/corpus/$f.lu" --json)
  if echo "$out" | grep -q '"verdict":"ub('; then
    echo "::error::$f.lu is safe-tier and reached a §7 row: $out"; exit 1
  fi
done

# A file outside coverage says so, with a reason on an `x-` key
# ([proto.record.ext]) rather than in the payload-free verdict.
# S-5 repaired upstream (s20): `procs.lu` is self-contained and RUNS.
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/procs.lu --json)
echo "$out" | grep -q '"verdict":"exit(0)"'

# is06's spec/03 rungs: the conc litmus tier runs under the sim
# scheduler. Unseeded records declare seeded:false and run FIFO.
for f in conc/cancel_sibling conc/message_passing conc/select_seeded; do
  out=$(cargo run --quiet -- conform-run "vendor/upstream/corpus/$f.lu" --json)
  echo "$out" | grep -Eq '"phase_reached":"run","seeded":false,"diagnostics":\[\],("warnings":\[\],)?"verdict":"exit\(0\)"'               || { echo "::error::$f.lu no longer reaches run cleanly: $out"; exit 1; }
done
# `--seed=N` requests the spec/03 §5 deterministic schedule, the
# record declares it ([proto.seed.flag]), and the same seed replays
# the identical record ([conc.det.seed]).
one=$(cargo run --quiet -- conform-run vendor/upstream/corpus/conc/select_seeded.lu --seed=7 --json)
echo "$one" | grep -q '"seeded":true'
two=$(cargo run --quiet -- conform-run vendor/upstream/corpus/conc/select_seeded.lu --seed=7 --json)
test "$one" = "$two"
# The E1005 dynamic-counterpart case finally RUNS (`channel` exists):
# the open region cannot transfer — region-fault per [conf.trap.map].
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/memory/region_move_while_open.lu --json)
echo "$out" | grep -q '"verdict":"trap(region-fault)"'
# `ffi.lu` now *executes* its unsafe block — the C intrinsics are
# modelled (docs/approximation-contract.md §8) — and stops at the
# inline `asm`, which no pinned document gives a meaning. Since
# is70 (wolf-interp#174, `[gram.item.attr.cfg]`) that asm is read
# through its `#[cfg(target = …)]`; at the 8e36bc1a pin (r28,
# wolf-lang e951afbb) the row carries an x86_64 and an aarch64
# variant, so every host keeps exactly one and declines it by name.
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/ffi.lu --json)
echo "$out" | grep -q '"verdict":"unsupported",.*asm' \
  || { echo "::error::ffi.lu keeps one asm variant on every host and declines it by name: $out"; exit 1; }
# `regions.lu` RUNS at pin 8b04edf: the tail now uses only specified
# semantics (frozen data readable forever, [mem.region.freeze.1]),
# closing the rest of is03's §5.5 finding. The seed corpus's flagship
# memory program reaching `run` was two pins in the making — keep it.
out=$(cargo run --quiet -- conform-run vendor/upstream/corpus/regions.lu --json)
echo "$out" | grep -Eq '"phase_reached":"run","seeded":false,"diagnostics":\[\],("warnings":\[\],)?"verdict":"exit\(0\)"'

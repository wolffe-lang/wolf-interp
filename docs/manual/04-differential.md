# 4 — The differential tools

Two implementations of the same spec are only comparable through the
observation records of chapter 3. The tools here automate that comparison:
`diff-run` against the pinned compiler, `conformance` against anything that
speaks `conform-run`.

## diff-run

`diff-run` walks the corpus, collects both implementations' records per
entry file, and compares them phase-aware per `[proto.cmp]`. The
counterparty is the compiler built inside the submodule (`cargo build -p
wolf_driver` in `upstream/` — building it is legitimate; reading its source
is not). Output from a checkout with the counterparty built:

```text
notice: counterparty compiler: upstream/target/debug/wolf
differential: 203 entries compared, 18 member(s) exercised through their entries
divergences: 11
  verdict: 11
conservatism ledger: 325 entries
  rejects-beyond(counterparty): 61
  run-unmatched: 103
  unsupported(counterparty): 120
  unsupported(interp): 41
verdict  upstream/corpus/memory/mode_missing_mut.lu  a=fail(E1007)@resolve  b=fail(E1007)@mem  resolve [filed: DIV-2026-011]
…
differential: GREEN — every divergence is filed in docs/divergence-log.md and none is a soundness candidate
```

(This block is not byte-checked in CI: whether a counterparty exists
depends on the environment. Without one, `diff-run` says so in `notice:`
lines and SKIPs; `--require-counterparty` hard-fails instead.)

A divergence is reported with its class, in descending severity:
`soundness-candidate` (one side reports UB where the other runs defined),
`verdict`, `span-or-code`, `stdout`, `protocol`, `timeout`. The
conservatism ledger beside it tracks the *expected* differences — programs
the compiler rejects statically that this implementation runs, and
`unsupported` on either side — so they are visible without being counted
as divergences. `--report` writes the JSONL report, `--filing` prints a
triage template per unfiled divergence, and every finding is routed through
[../divergence-log.md](../divergence-log.md).

## Conformance bundles

`conformance export` emits a self-contained, byte-deterministic bundle:
the corpus, the suite programs, this implementation's reference records,
the trap and UB vocabularies, the anchor registry, a coverage matrix, and
a hashed manifest ([../conformance-bundle.md](../conformance-bundle.md)).
`conformance check` runs any `conform-run`-speaking implementation against
a bundle — after verifying the bundle's integrity — and reports per
`[proto.cmp]`. Checking the bundle against its own recorded records is the
smoke test of the whole path:

```console
$ lupin conformance export --out target/bundle --json
{"anchors_covered":102,"anchors_total":315,"bundle_sha256":"…","files":317,"forward_tags":94,"out":"target/bundle","pin":"0b4e79c5deb48e93468d58971ae32b3ad8fed9b9","programs":300,"records":278}
$ lupin conformance check target/bundle --replay target/bundle/expected/records.jsonl
differential: 278 entries compared, 0 member(s) exercised through their entries
divergences: 0
conservatism ledger: 94 entries
  unsupported(counterparty): 47
  unsupported(interp): 47
differential: GREEN — every divergence is filed in docs/divergence-log.md and none is a soundness candidate
notice: bundle target/bundle at pin 0b4e79c5deb48e93468d58971ae32b3ad8fed9b9 verified (bundle_sha256 …)
```

The `bundle_sha256` covers every file in the bundle, so two exports at the
same interpreter commit and pin are byte-identical — CI exports on three
operating systems and diffs the manifests. A real counterparty replaces
`--replay` with `--impl <cmd>`; `conformance check --help` has the
invocation contract.

## fuzz

`fuzz` is the generator arm: seeded, deterministic, producing
defined-by-construction or boundary-poking programs, running both sides on
each, and reducing anything divergent to a minimal reproducer. Without a
counterparty it degrades to self-oracle checks (no unsafe-free program may
report UB; defined programs must not crash the machine) and says so. The
campaign record lives in [../divergence-log.md](../divergence-log.md).

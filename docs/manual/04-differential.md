# 4 — The differential tools

Two implementations of the same spec are only comparable through the
observation records of chapter 3. The tools here automate that comparison:
`diff-run` against the pinned compiler, `conformance` against anything that
speaks `conform-run`.

## diff-run

`diff-run` walks the corpus, collects both implementations' records per
entry file, and compares them phase-aware per `[proto.cmp]`. The
counterparty is the compiler built inside the submodule: `cargo build -p
wolf_driver` in `upstream/`, plus `cargo build -p wolf_rt` for the
`libwolf_rt.a` its native lanes link. Building them is legitimate; reading
the source is not. Output from a checkout with the counterparty built:

```text
notice: counterparty compiler: upstream/target/debug/wolf
notice: counterparty tier: default (conform-run)
harness profile: release
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

(That block is not byte-checked in CI, because whether a counterparty
exists depends on the environment. Without one, `diff-run` says so in
`notice:` lines and SKIPs. `--require-counterparty` hard-fails instead.)

### Build this binary at `--release` before comparing

`diff-run` hands each side a wall-clock budget and scores a timeout as a
verdict. That is right for a program the counterparty finishes and this
machine does not, and wrong for a program only the *build* cannot finish:
`corpus/memory/byte_list_ledger.lu` runs in under a second at `--release`
and was still going at 60 seconds under `target/debug/lupin`, against a
30-second budget. The report then carries a `timeout` line no reader can
tell from a divergence, which is how it was once written down as a load
artefact (wolf-interp#63).

So `diff-run` refuses to compare when this binary is an unoptimized build,
once a counterparty has been found. Build at `--release` and re-run. If you
have a reason to compare with a debug harness — debugging the harness
itself, most likely — `--allow-debug-self` proceeds, with a warning naming
the build and `x-self-profile` on every divergence in the JSONL report. The
`harness profile:` line is printed on every run either way.

A run with no counterparty still SKIPs green whatever the profile: there is
no comparison to make with the wrong instrument.

### Which counterparty engine answers

The compiler's `conform-run` is one process contract over several engines,
chosen by flag, and `--counterparty-tier` picks which one the comparison
drives:

| tier | invokes | counterparty reaches `run` on |
| --- | --- | --- |
| `default` | `conform-run` | 0 of 245 entries |
| `checked` | `conform-run --checked` | 120 |
| `native` | `conform-run --native` | 113 |
| `release` | `conform-run --release` | 104 |

Measured at pin `613c3dc`. At `default` the compiler walks its static
pipeline and stops (`unsupported` at `wir`), so no run-tier program has a
counterparty claim to compare against and the whole dynamic half of the
corpus lands in the conservatism ledger uncompared. Prefer `checked` or
`native` for coverage, and `release` when the question is whether
optimization preserved behavior: that lane runs the mid-end and the
whole-program layer, so comparing it against this machine is what tests
that claim.

`native` and `release` need `libwolf_rt.a` beside the `wolf` binary;
without it the compiler declines as a tool and the runner says so; it does
not fall back to a shallower lane. This machine has one engine, so its own
side is always invoked plainly, and the tier selects only the
counterparty's engine.

A divergence is reported with its class, in descending severity:
`soundness-candidate` (one side reports UB where the other runs defined),
`verdict`, `span-or-code`, `stdout`, `protocol`, `timeout`. The
conservatism ledger beside it tracks the *expected* differences: programs
the compiler rejects statically that this implementation runs, and
`unsupported` on either side. They stay visible without being counted as
divergences. `--report` writes the JSONL report, `--filing` prints a
triage template per unfiled divergence, and every finding is routed through
[../divergence-log.md](../divergence-log.md).

## Conformance bundles

`conformance export` emits a self-contained, byte-deterministic bundle:
the corpus, the suite programs, this implementation's reference records,
the trap and UB vocabularies, the anchor registry, a coverage matrix, and
a hashed manifest ([../conformance-bundle.md](../conformance-bundle.md)).
`conformance check` verifies the bundle's integrity, runs any
`conform-run`-speaking implementation against it, and reports per
`[proto.cmp]`. Checking the bundle against its own recorded records is the
smoke test of the whole path:

The anchor cross-check can report a FILED upstream finding as a `notice:`
line on every export without failing the export that already filed it
(`export::FILED_REGISTRY_FINDINGS`, the divergence-log waiver pattern).
The long-standing example retired at the 90c90df pin: `[conf.anchor.ns]`
was never amended for the `pkg` namespace `spec/08-package.md` introduced
(wolf-lang#120, filed upstream), until s115 amended the clause, and the
notice died with the amendment.

The second retirement is the same contract over a registry hole instead
of a namespace: c1f54f2's anchors regen dropped `[gram.lex.ident]`
while spec/01 §1.3 still defined it (wolf-lang#177, carried in
`export::FILED_REGISTRY_HOLES` and reported on every export from the
addcd7f pin), until upstream r03 fixed the spec-extract scanner and the
v0.2.0 registry re-gained the anchor. The waiver died at the c88ab64
pin as filed, and every export is notice-free again:

```console
$ lupin conformance export --out target/bundle --json
{"anchors_covered":205,"anchors_total":453,"bundle_sha256":"…","files":584,"forward_tags":109,"out":"target/bundle","pin":"e0ce01891577d64d6b9a3497f752a32acac3fefc","programs":567,"records":533}
$ lupin conformance check target/bundle --replay target/bundle/expected/records.jsonl
differential: 533 entries compared, 0 member(s) exercised through their entries
divergences: 0
conservatism ledger: 122 entries
  unsupported(counterparty): 61
  unsupported(interp): 61
differential: GREEN — every divergence is filed in docs/divergence-log.md and none is a soundness candidate
notice: bundle target/bundle at pin e0ce01891577d64d6b9a3497f752a32acac3fefc verified (bundle_sha256 …)
```

The `bundle_sha256` covers every file in the bundle, so two exports at the
same interpreter commit and pin are byte-identical. CI exports on three
operating systems and diffs the manifests. A real counterparty replaces
`--replay` with `--impl <cmd>`. The invocation contract is in
`conformance check --help`.

## fuzz

`fuzz` is the generator arm. It is seeded and deterministic. It produces
defined-by-construction or boundary-poking programs, runs both sides on
each, and reduces anything divergent to a minimal reproducer. Without a
counterparty it degrades to self-oracle checks (no unsafe-free program may
report UB; a defined program must not crash the machine) and says so. The
campaign record lives in [../divergence-log.md](../divergence-log.md).

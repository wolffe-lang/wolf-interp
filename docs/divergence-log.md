# Divergence log — the is05 ledger

Every divergence the differential runner finds lands here with its triage.
This document and `differ::FILED_DIVERGENCES` are one ledger in two forms:
the table below is the human record; the constant is what lets the runner
annotate a known finding (`x-filed`) and stop gating on it while it awaits
its fix. **An entry here never closes a finding** — it routes it. Closure is
the iron rule: the resolving commit lands a corpus file and a spec clause
citation, and then the entry moves to the *resolved* section.

## The triage workflow (`[proto.cmp.triage]`)

The runner emits, for each unfiled divergence, a filing template (visible
with `wolf-interp diff-run --filing`): the program, both verdicts, the class,
the rung the comparison fired at. A human then walks the decision tree —
**the spec document is the defendant first**:

1. **Spec silent or ambiguous** on the behavior → *spec bug*. Clause PR to
   the spec first; both implementations then conform to the new clause.
2. **Spec clear, interpreter matches it** → *compiler bug*. Filed in
   wolf-lang; the minimized program lands in `corpus/` as a regression file
   in the same fix.
3. **Spec clear, compiler matches it** → *interpreter bug*. Fixed here;
   corpus file likewise.

Divergence classes, descending severity (`[proto.cmp.severity]`, extended
by two runner-level classes): `soundness-candidate` (gates always, filed or
not), `verdict`, `span-or-code`, `stdout`, then `protocol` (a record the
schema rejects) and `timeout`. The conservatism ledger — `unsupported` on
either side, rejections at rungs the other side does not perform, run-tier
outcomes the pre-M1 compiler cannot check — is tracked and reported but
never gates (`[proto.record.unsupported]`).

## Counterparty acquisition (integrator ruling, is05)

Building and executing the pinned compiler is **legitimate binary
acquisition**: the binary is data consumed through the spec/06 protocol,
exactly like the corpus. Reading or studying its *source* remains forbidden
— independence is about shared code and shared blind spots. Locally:
`cargo build -p wolf_driver` inside `upstream/` (the submodule is
sparse-checked-out to `spec/` + `corpus/`; widen it for the build, restore
it after — the binary survives in `upstream/target/`). In CI the vendored
snapshot has no `crates/` and the private submodule cannot clone, so the
differential lane detects the absence, prints `notice:` lines, and SKIPs;
`--require-counterparty` turns the skip into a hard failure.

## Open findings

First corpus differential: 2026-08-09, pin `8b04edf`, 133 entries compared,
6 divergences, 228 conservatism-ledger entries (46 rejects-beyond by the
counterparty — the standing false-rejection metric, 51 run-unmatched
pre-M1, 78 counterparty-unsupported, 53 interp-unsupported).

### DIV-2026-001 — `typecheck/match_exhaustive.lu` — verdict @ parse — **compiler suspected**

- a (interp): `fail(E0201)@parse` at bytes 342..345 (`int`)
- b (wolfc): `unsupported@typecheck` (i.e. parsed, resolved, typechecked)
- The file spells `Rgb(int int int)`. `[gram.item.type]`'s production is
  `variant ::= IDENT ('(' type (',' type)* ')')?` — commas are required, and
  the clause is not ambiguous. The compiler accepts a form outside the
  published grammar (or the corpus file has a typo its parser tolerates —
  either way the defect is compiler-side). Owed: wolf-lang filing; either
  the file gains commas *upstream* or the grammar is amended to admit
  space-separated payload types — not this repo's call to make.

### DIV-2026-002 — `resolve/cycle/main.lu` — verdict @ resolve — **interp suspected**

- a (interp): `exit(1)@run` — the cycle is tolerated and the program runs
- b (wolfc): `fail(E0303)@resolve`
- D32 / `[mod.cycle]`: imports form a DAG; the corpus pins E0303. The spec
  is clear and the compiler matches it. This machine's sema-lite claims the
  `resolve` rung (frontend ladder mapping) and must therefore enforce the
  module-graph laws that rung owns. Owed here: cycle detection in `sema`,
  failing E0303 with the cycle's spans.

### DIV-2026-003 — `resolve/dupdef/main.lu` — verdict @ resolve — **interp suspected**

- a (interp): `unsupported@resolve`, reason "`helper` is defined more than
  once … (the compiler's E0302)"
- b (wolfc): `fail(E0302)@resolve`
- The detection **already exists** here; only the record shape is wrong:
  a detected D32 violation is a `fail(E0302)`, not an `unsupported` whose
  `phase_reached` claims resolve completed clean. Owed here: report the
  existing detection as the failure it is.

### DIV-2026-004 — `resolve/private/main.lu` — verdict @ resolve — **interp suspected**

- a (interp): `unsupported@resolve`, reason names `[mod.vis.private]` and
  E0304 itself
- b (wolfc): `fail(E0304)@resolve`
- Same shape as DIV-2026-003: the visibility violation is detected and then
  declared out of scope instead of failed. Owed here: `fail(E0304)`.

### DIV-2026-005 — `resolve/unused/main.lu` — verdict @ resolve — **interp suspected**

- a (interp): `exit(0)@run`
- b (wolfc): `fail(E0305)@resolve`
- `[mod.use.unused]` (D32): an unused import is a hard error, not a lint.
  The spec is clear, the compiler matches it, and this machine simply does
  not perform the check. Owed here: unused-import tracking in `sema`.

### DIV-2026-006 — `grammar/receiver_moded.lu` — span-or-code @ parse — **spec suspected**

- a (interp): `E0210` at bytes 233..240 — the whole `(mut y)` receiver
- b (wolfc): `E0210` at bytes 234..237 — the `mut` keyword
- Both implementations agree the program is illegal and agree on the code;
  the §3.3 receiver ruling pins E0210 but is silent on its span, and the
  two implementations chose defensibly and differently. Precedent:
  `[gram.amb.structlit]`'s E0006 span was pinned by amendment after exactly
  this kind of finding (is01). Owed: a spec amendment naming the span (this
  repo's candidate: the whole parenthesized receiver, since the *placement*
  is what is illegal); both implementations then conform.

## Resolved findings

(none yet — a resolution lands a corpus file + clause citation in its
resolving commit, then the entry moves here with the commit hash)

## Fuzz campaign record

| date | seed | count | mode | counterparty | findings | ledger |
|------|------|-------|------|--------------|----------|--------|
| 2026-08-09 | 5381 (0x1505) | 10000 | mixed (5000 defined / 5000 boundary) | wolfc @ 8b04edf (debug) | **0** | 20000 (exactly 2/case: counterparty-unsupported@typecheck + run-unmatched) |

The first campaign's ledger composition is itself a result: 2 entries per
case with **zero** rejects-beyond means every one of the 10,000 generated
programs — boundary mode's regions, moves and `mut` call sites included —
cleared the compiler's full frontend (lex, parse, resolve, s17's completed
sema) *and* this machine's run tier, with the two frontends in exact
agreement on all of them. The generator's semantic-plausibility layer is
doing its job; the next campaign should turn the boundary dial harder
(deeper nesting, adversarial-but-legal spellings) precisely because this one
found nothing. Replay: `wolf-interp fuzz --count 10000 --seed 5381 --compiler
upstream/target/debug/wolf`.

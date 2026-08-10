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
with `lupin diff-run --filing`): the program, both verdicts, the class,
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

Fifth corpus differential: is09, pin `cbde620` (s21's shared tier — nine
files advance to `mem`, `prov_holy_grail.lu` to `typecheck`; spec-extract
renders the §3.2 operator climb into `grammar.ebnf`), 148 entries
compared, **0 divergences** — the fourth consecutive zero round. 246
conservatism-ledger entries, composition unchanged from the fourth round
(59 rejects-beyond by the counterparty, 64 run-unmatched pre-M1, 80
counterparty-unsupported, 43 interp-unsupported): the pin moved only
`phase:` directives and spec text, and neither side's accept set moved
with it. The newly explicit operator-climb EBNF was diffed against this
repo's is01 §3.2 transcription (`parse::PRECEDENCE`,
`parse::PREFIX_OPERATORS`): tier-for-tier, operator-for-operator,
associativity-for-associativity identical — no finding; the check is now
mechanical
(`tests/spec_extract.rs::the_emitted_operator_climb_matches_our_transcription`).
`differ::FILED_DIVERGENCES` remains **empty**.

Fourth corpus differential: is08, pin `843174f`, 148 entries compared,
**0 divergences**, 246 conservatism-ledger entries (59 rejects-beyond by
the counterparty — the E1005/E1011/E1012 region-checker litmuses landed —
64 run-unmatched pre-M1, 80 counterparty-unsupported, 43 interp-unsupported
— down from 45: `procs.lu` and `conc/proc_kill_defers.lu` are
self-contained now and RUN, S-5 resolved). `differ::FILED_DIVERGENCES`
remains **empty**.

Third corpus differential: is07, pin `79ceec6`, 142 entries compared,
**0 divergences** (down from 1), 238 conservatism-ledger entries (55
rejects-beyond by the counterparty — up from 46: the E1004/E1007/E1010
litmuses landed — 60 run-unmatched pre-M1, 78 counterparty-unsupported,
45 interp-unsupported). `differ::FILED_DIVERGENCES` is **empty** for the
first time since the differential lane exists.

The is07 exploration record (the corpus half lives in
`tests/explore_corpus.rs::CONC_LEDGER`): every `conc/` litmus explored to a
**closed frontier** under DPOR — 8 files, 1–2 Mazurkiewicz classes each,
naive-DFS baseline agreeing on every conclusion — with one verdict per
file across its entire schedule space. The determinism-taxonomy claim
(spec/03 §5 `sched-ev/0`, `[proto.seed.equal]`) holds over the whole
pinned conc tier: **no corpus file is schedule-dependent**. The multiopen
model check (the question `memory/region_multiopen_ok.lu`'s own header
flags for is07) answers definitively within bounds: no explored schedule —
corpus files or the concurrent multiopen litmuses in
`tests/explore_machine.rs` — breaks the region forest invariant or leaks.

## Spec findings from is06/is07 (spec-is-defendant — filed, not absorbed)

spec/03 had never been executed before is06. The machine was the first
executable test of it, and the harvest was routed upstream, not patched
around. **The s20 S-batch (pin `843174f`) paid S-1 through S-8** — the
eight entries now live under *Resolved findings* below with what the spec
adopted and where this machine realigned. S-9 remains open.

- **S-9 (is07) — the seed↔schedule encoding has no normative home.**
  `[conc.det.seed]` defines `--replay=SEED` behaviorally and `[proto.seed]`
  makes equal seeds byte-comparable, but no pinned document says what a
  seed *is* beyond "a value that regenerates the stream" — the accepted
  s36 Phase A hook-design doc is the designated owner and **does not exist
  at pin `843174f` either** (re-checked at the is08 pin bump; the S-batch
  is spec/03+05 only). is07's provisional split of the `u64` namespace
  (bit 62 tags a packed schedule — `sched::PACKED_SEED_TAG`,
  approximation-contract §10.6) stands until the Phase A doc lands; the
  compiler runtime's format has priority and this side re-pins to it.

## Resolved findings

### S-1..S-8 — the is06 harvest — **resolved upstream, pin `843174f` (the s20 S-batch)**

All eight spec/03 findings from is06's first execution of the concurrency
spec were adjudicated in one amendment batch, 18 clauses. Entry by entry:

- **S-1 (`when` had no clauses)** → `[conc.when.order]`,
  `[conc.when.nodeadlock]`, `[conc.when.body]`, `[conc.when.nonest]`
  adopt the machine's canonical-order/whole-set/write-back semantics.
  The `sync.when.*` forward citations are retired; **realignment**: the
  nested-`when` stopgap `trap(assert)` is replaced per the spec's
  deviation — dynamically reaching an acquisition of an already-held
  sync object is `trap(deadlock)` (`[conc.deadlock.self]`), a lexical
  nest is the compiler's E1103, and a dynamically nested `when` over a
  disjoint set now *proceeds* (the compiler accepts it; the old blanket
  fault would have broken the one-way approximation direction).
- **S-2 (region-transfer clauses missing)** → `[conc.chan.move]`,
  `[conc.chan.staleuse]`, `[conc.chan.imm]` specify exactly the dynamic
  checks this machine runs at the send; the rules cite them now.
- **S-3 (no deadlock verdict or trap kind)** → `[conc.deadlock.def]` and
  `[conc.deadlock.trap]`, with `deadlock` added to `[conf.trap.set]` as
  the deliberate twelfth kind. **Realignment**: the machine's
  `unsupported`-with-roster report is retired for `trap(deadlock)` with
  the roster in the message; the is07 explorer's per-schedule verdict is
  the same spelling.
- **S-4 (child failure did not reach a blocked owner)** →
  `[conc.task.fail.owner]`: the scope is the cancellation unit, owner
  included (Trio posture). **Realignment**: the scheduler cancels a
  blocked, non-joining owner when a child fails; the owner's surfaced
  cancellation is its finished cancellation and the child's failure
  re-raises at the scope exit, after the join — replacing the is06
  deadlock-provoked-by-failure special case.
- **S-5 (`procs.lu`/`proc_kill_defers.lu` named undefined functions)** →
  both files are self-contained at the pin and **RUN** here: `procs.lu`
  exit(0), `proc_kill_defers.lu` exit(0) printing exactly `released`
  (kill skips defers), both schedule-independent under the explorer.
- **S-6 (`cancelled` unreachable)** → `[conc.proc.cancel]` specifies
  `w.cancel()` as the delivery mechanism; implemented (cooperative,
  defers run, monitors see `cancelled`; a proc completing its value
  anyway keeps `normal(value)`).
- **S-7 (`link` pair spelling + root death unspecified)** →
  `[conc.proc.link.pair]` (implemented: `a.link(b)`, idempotent per
  pair) and `[conc.proc.root]` (implemented: the root domain's abnormal
  death runs the killed-proc sequence for every live proc and exits
  nonzero — 1 here, class compared, never the number).
- **S-8 (closed-channel `select` readiness)** → `[conc.select.closed]`
  adopts the machine's Go-posture answer verbatim; the rule cites it.

### DIV-2026-007 — `grammar/receiver_moded.lu` — **resolved upstream, pin `79ceec6`**

Compiler suspected, confirmed and fixed: the compiler's E0210 primary span
now covers the whole parenthesized moded receiver, exactly as the pin
`67c977f` spec amendment demanded and as this interpreter has reported
since is05. Verified by the third corpus differential (0 divergences).
The DIV-2026-006 → 007 chain — spec amendment first, then the lagging
implementation — is closed end to end.

### DIV-2026-008 — `conc/freeze_publish.lu` — **resolved upstream, pin `79ceec6`**

Corpus/spec suspected, confirmed: the wolf-lang ruling kept
`[conc.task.spawn]`'s capture rule intact and repaired the *file* — it now
reports through a channel (`ch.send(table[3])` / `ch.recv()`) instead of
writing captured mutable locals, exactly the conforming spelling this
machine's `freeze_then_share_reads_from_any_task` litmus pinned. Runs
`exit(0)` here, matching the corpus; is07's explorer proves the exit
stable across its whole schedule space.

### DIV-2026-009 — `conc/when_multi.lu` — **resolved upstream, pin `79ceec6`**

Corpus suspected, confirmed: the expected total is 223 now — the
arithmetic this log recorded. Runs `exit(0)`, schedule-independent under
exploration. (`when` gained its spec/03 clauses at pin `843174f`; S-1 is
resolved above.)

### DIV-2026-001 — `typecheck/match_exhaustive.lu` — **resolved upstream, pin `67c977f`**

Compiler suspected, confirmed: the parser accepted comma-less variant
payloads outside the published grammar (and the formatter stripped the
commas the grammar requires — the printer bug the leniency masked). The
upstream fix landed both the parser rejection and the corrected corpus
file `Rgb(int, int, int)`. Both implementations now parse the file and it
**runs** here (`exit(0)`, in the run ledger). Closed by the pin bump.

### DIV-2026-002 — `resolve/cycle/main.lu` — **resolved here, is06**

Interp suspected, confirmed. `sema::resolve_check` now enforces
`[mod.cycle]` (D32): the module-use graph is walked depth-first and the
back-edge that closes a cycle fails `E0303` at the closing `use` decl —
span `[18,28]`, byte-identical to the counterparty's record. The corpus
file is the regression test (it fails at `resolve` exactly as pinned).

### DIV-2026-003 — `resolve/dupdef/main.lu` — **resolved here, is06**

Record shape fixed: a detected D32 duplicate is `fail(E0302)` at the
second definition site (`[21,27]`, matching the counterparty), not an
`unsupported` whose `phase_reached` claims resolve completed clean.

### DIV-2026-004 — `resolve/private/main.lu` — **resolved here, is06**

Same shape: the detected cross-module private access is `fail(E0304)` at
the referencing member ident (`[305,311]`, matching the counterparty).

### DIV-2026-005 — `resolve/unused/main.lu` — **resolved here, is06**

`[mod.use.unused]` implemented: an unused import of a loaded module is a
hard error, judged per file (D32 makes `use` file-scoped). `fail(E0305)`
at the bound name (`[250,255]`, matching the counterparty). Ambient
prelude names (`use std.fs`) are exempt — they resolve no directory and no
module law this rung owns speaks about them.

### DIV-2026-006 — `grammar/receiver_moded.lu` — **resolved in the spec, pin `67c977f`**

Spec suspected, confirmed: §3.3 now pins "primary span = the entire
parenthesized moded receiver" — the reading this repo proposed and already
implemented. The interpreter conforms as-is; the compiler does not yet,
and that residue is DIV-2026-007 above (compiler suspected).

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
found nothing. Replay: `lupin fuzz --count 10000 --seed 5381 --compiler
upstream/target/debug/wolf`.

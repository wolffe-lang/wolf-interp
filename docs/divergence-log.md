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

spec/03 had never been executed before this sprint. The machine is the
first executable test of it, and the harvest below is routed upstream, not
patched around. None of these gate: they are clause debts, and the rules
implementing them cite either a clause family root or the reserved `sync`
namespace and say so in their descriptions.

- **S-1 — `when` has no clauses.** 03 Q6 decided `when` is a language
  construct; the corpus exercises it (`procs.lu`, `when_multi.lu`); the
  sprint contract names `[conc.when.order]`/`[conc.when.nonest]` — and
  spec/03 contains no `conc.when.*` anchors at all. The machine's rules
  cite forward `sync.when.order`/`sync.when.nonest` until the section is
  written.
- **S-2 — region-transfer clauses missing.** The sprint names
  `[conc.chan.move]`, `[conc.chan.staleuse]`, `[conc.chan.imm]`;
  spec/03 §3 has only `[conc.chan.type]`'s parenthetical "(moved on
  send)" and `[conc.mm.hb.move]`. The dynamic disconnectedness check and
  the sender-stale-use fault have no clause ids to cite; the machine cites
  `conc.mm.hb.move` and the `conc.chan` family root.
- **S-3 — no verdict for deadlock, no trap kind either.**
  `[proto.record.verdict]` has no verdict for nontermination and the
  closed `[conf.trap.set]` has no `deadlock` kind. A program whose every
  task is blocked with no pending timer reports `unsupported` with the
  blocked-task roster — honest, but a spec gap for a language whose
  concurrency is supposed to be schedulable and explorable (is07 will
  need a stable spelling for "this schedule deadlocks").
- **S-4 — child failure does not reach a blocked scope owner.**
  `[conc.task.fail]` cancels *siblings* and re-raises *at the scope
  exit*; it says nothing about the owner's own pending blocking
  operations. An owner blocked on a channel its failed child would have
  served deadlocks (observed on `procs.lu`'s second scope). Trio/njs
  cancel the whole scope; spec/03 as written does not.
- **S-5 — `corpus/procs.lu` and `conc/proc_kill_defers.lu` name
  undefined functions.** `build_batch()`, `worker()`, `sleeper()` exist
  in no document and no corpus file; the acceptance criterion
  "`procs.lu` runs to completion" is unsatisfiable as the corpus stands.
  The machine reports `unsupported` (honest decline); the supervision
  semantics are pinned instead by self-contained litmuses in
  `tests/conc_machine.rs`.
- **S-6 — the `cancelled` exit reason is unreachable from the language.**
  `[conc.proc.exit]` lists `cancelled` ("structured cancellation reached
  the proc") but no construct in the pinned surface delivers structured
  cancellation *to a proc* (procs sit under the root supervisor, outside
  every user scope). Mechanism owed.
- **S-7 — `link` has no spelling for coupling two procs, and the root
  domain's death is unspecified.** `w.link()` couples `w` with the
  *caller's* proc; called from `main` that is the root supervisor's
  domain, whose abnormal exit spec/03 §2 never defines. The machine
  reports the root kill as `unsupported`
  (`tests/conc_machine.rs::a_linked_proc_takes_its_partner_with_it`).
- **S-8 — closed-channel readiness in `select` is unspecified.**
  `[conc.select.ready]` defines readiness for messages; `[conc.chan.close]`
  defines the drained-close error for `recv` — whether a drained-closed
  channel makes a `select` arm *ready* (Go: yes) is unwritten. The machine
  answers yes, delivering the closed error to the arm.
- **S-9 (is07) — the seed↔schedule encoding has no normative home.**
  `[conc.det.seed]` defines `--replay=SEED` behaviorally and `[proto.seed]`
  makes equal seeds byte-comparable, but no pinned document says what a
  seed *is* beyond "a value that regenerates the stream" — the accepted
  s36 Phase A hook-design doc is the designated owner and does not exist
  yet at pin `79ceec6`. is07 ships a provisional split of the `u64`
  namespace (bit 62 tags a packed schedule; low 62 bits are mixed-radix
  choice digits; everything else seeds the generator —
  `sched::PACKED_SEED_TAG`, approximation-contract §10.6) so that
  explorer counterexamples are `--seed=N` replays *today*, honoring
  `[proto.seed.equal]` one-sided. The compiler runtime's format has
  priority: when the Phase A doc lands, this side re-pins to it and the
  cross-validation harness diffs the two.

Status at is07, pin `79ceec6`: **none of S-1..S-8 landed in the pinned
spec** (the pin's spec tree is byte-identical to `67c977f`'s). The s20
S-batch amendments (`[conc.when.*]`, the deadlock verdict, `[conc.chan.
staleuse]`, link semantics) remain owed; the machine's rules keep their
documented forward citations and approximation-contract entries, and the
next pin bump re-checks this list first.

## Resolved findings

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
exploration. (`when` still has no spec/03 clauses; S-1 stays open.)

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
found nothing. Replay: `wolf-interp fuzz --count 10000 --seed 5381 --compiler
upstream/target/debug/wolf`.

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

Eleventh corpus differential: lupin 0.1.7, pin `e94b879` (wave six — s40
os/time/json, s70 match tier + X3 value paths, s69 idiom lints; 232
entries compared, 22 members through their entries, counterparty built
CLEAN at `e94b879`), **0 divergences**. The ruling the last four rounds
asked for landed in spec/06: **`[proto.cmp.rung]`** — when both records
reject with `fail(CODE)` and the first diagnostic's code and span agree,
the records AGREE even when `phase_reached` names different rungs of the
shared ladder; exactly one verdict wide. `compare_deep` implements the
clause, the eleven rung-placement divergences compare clean, and
**DIV-2026-011, -012, -014 and -015 ALL CLOSE** with the clause cited —
`FILED_DIVERGENCES` is empty for the first time since the fourth round.
One new shape surfaced and resolved comparator-side, no filing needed:
wolfc's `resolve/cycle/main.lu` record interleaves its new W0314 lint
ahead of the E0303 rejection in `diagnostics` (source order, licensed by
`[proto.record.warn]`'s "warning observations ride `diagnostics` at
warning severity"), so the fail comparison now reads the first
**error**-severity diagnostic — a lint's span is `[proto.cmp.warn]`'s
surface, never the rejection's. 382 conservatism-ledger entries (63
rejects-beyond by the counterparty, 122 run-unmatched, 147
counterparty-unsupported, 50 interp-unsupported — the fs/net/proc-spawn
tiers, comptime, and the compiler-only analyses).

---

Tenth corpus differential: lupin 0.1.6, pin `13b811f` (wave four — the
#41 capture law, s34 procs, s35 io reactor, s39/s40/#40 native
str/List/fs, the s68 lint corpus; 203 entries compared, 18 members
through their entries, counterparty built CLEAN at `13b811f`), **11
divergences, all filed, none a soundness candidate** — and every one is
the same finding: same code, same span, this machine at `resolve` where
wolfc's emission lives at `typecheck`/`mem`. DIV-2026-011 holds;
DIV-2026-012 holds; **DIV-2026-013 CLOSES** (wolfc's conform-run no
longer misrejects its own s38 fs/io files — `unsupported@wir` there now,
never a divergence); **DIV-2026-014's** wiring half closes the same way
(wolfc emits E0411/E0412/E0413 at `typecheck` now; this machine
realigned its E0412/E0413 spans to the counterparty's `:spec` shape) and
its residue rides DIV-2026-011; issue #19's realignment opens
**DIV-2026-015** (E1101/E1102/E1103/E0004 statically at this machine's
resolve rung, byte-identical codes and spans — the fourth family of the
one rung question). The `warnings` arrays agree wherever both sides
carry them (`store_buffer`'s W1101×4 + W1102 set is byte-identical).
325 conservatism-ledger entries (61 rejects-beyond by the counterparty,
103 run-unmatched, 120 counterparty-unsupported, 41 interp-unsupported —
the fs tier, sockets, procs-adjacent comptime, and the compiler-only
analyses).

### DIV-2026-015 — the E11xx capture law + E0004 — **RESOLVED by `[proto.cmp.rung]`, pin `e94b879` (0.1.7)**

Resolution: the `[proto.cmp.rung]` ruling (spec/06, s70) — fail parity
(code + span) at any shared-ladder rung is agreement, one verdict wide.
All four files compare clean at the eleventh round. The original filing:

Filed 2026-08-11 (lupin 0.1.6, CLEAN wolfc build at `13b811f`). Four
files, one class: verdict (rung placement only), codes and spans
byte-identical.

- `conc/store_buffer.lu` — both fail(E1101), span `[438,439]` (the
  first captured write, `x`); a at `resolve`, b at `typecheck`. The
  warning sets also agree: W1101 at `[438,439]`, `[445,447]`,
  `[511,512]`, `[518,520]`, W1102 at `[511,517]`.
- `conc/chan_unsendable.lu` — both fail(E1102), span `[275,284]` (the
  `List[int]` payload); a at `resolve`, b at `typecheck`.
- `conc/when_nested.lu` — both fail(E1103), span `[642,755]` (the whole
  inner `when`); a at `resolve`, b at `typecheck`.
- `grammar/intdot_exponent.lu` — both fail(E0004), span `[291,295]`
  (`1.e5`); a at `resolve`, b at `typecheck`. Issue #19's correction:
  the code stays an error (`int` has no member `e5`), and producing it
  here closed the last E000x unsupported(interp) conservatism row.

Triage: same as DIV-2026-011/-012/-014 — the spec is silent on
same-code-same-span rejections across implementations of unequal
pipeline depth; one `[proto.cmp]` ruling closes all four families.

---

Ninth corpus differential: lupin 0.1.5, pin `f0da6e6` (the five-lane
fan-out — s32 tasks, s33 channels, s37 str core, s38 fmt/io/fs, s67
warnings; 181 entries compared, 18 members through their entries,
counterparty built CLEAN at `f0da6e6`), **10 divergences, all filed,
none a soundness candidate**: DIV-2026-011 holds; issue #18's tier
statics open **DIV-2026-012** (four files, the DIV-2026-011 rung
question again — same code, same span, this machine at resolve where
wolfc's emissions live at mem/typecheck); and the pin exposes a
counterparty *surface* lag filed as **DIV-2026-013**/**DIV-2026-014**:
wolfc's conform-run at `f0da6e6` rejects or declines six of its own new
corpus files (the s38 fs/io builtins E0301-unresolved; the strings
statics reported `unsupported`) while its own corpus and checked-lane
tests pin them — the conform-run wiring landed upstream after this pin.
289 conservatism-ledger entries (60 rejects-beyond by the counterparty,
89 run-unmatched, 103 counterparty-unsupported, 37 interp-unsupported).

### DIV-2026-012 — the 0.1.5 tier statics — **RESOLVED by `[proto.cmp.rung]`, pin `e94b879` (0.1.7)**

Resolution: the `[proto.cmp.rung]` ruling (spec/06, s70) — same closure
as DIV-2026-011, which this filing rode. The original filing:

Filed 2026-08-11 (lupin 0.1.5, CLEAN wolfc build at `f0da6e6`). Four
files, one class: verdict (rung placement only), codes and spans
byte-identical where both sides emit.

- `memory/unsafe_raw_outside.lu` — both fail(E1301), span `[384,395]`
  (the `c.malloc(8)` call); a at `resolve`, b at `mem`.
- `memory/unsafe_sig.lu` — both fail(E1302), span `[329,330]` (the
  parameter `p`); a at `resolve`, b at `mem`.
- `typecheck/cast_bad.lu` — both fail(E0805); a at `resolve`, b at
  `typecheck`.
- (`memory/mode_missing_mut.lu` remains DIV-2026-011, the original
  filing of the question.)

Triage: the spec is the defendant first, and it is *silent* — spec/06
compares `phase_reached` without ruling on same-code-same-span
rejections across implementations of unequal pipeline depth. Routed
upstream with DIV-2026-011; whatever `[proto.cmp]` ruling closes that
filing closes this one. Sema-lite is this machine's only static tier
(issue #18: the unsafe ring, its signature boundary, and the cast
matrix's bool column now reject at resolve with the counterparty's
codes and spans, observed at this pin).

### DIV-2026-013 — the s38 fs/io files — **RESOLVED upstream, pin `13b811f` (0.1.6)**

Resolution: exactly the predicted closure. wolfc's conform-run at
`13b811f` no longer E0301-rejects its own s38 files — it reports
`unsupported@wir` on `fs/error_row.lu`, `fs/roundtrip.lu` and
`io/eprint.lu`, which `[proto.cmp.defined-divergence]` makes a ledger
row, never a divergence. The three FILED_DIVERGENCES entries retired
with this note. The original filing:

`fs/error_row.lu`, `fs/roundtrip.lu`, `io/eprint.lu`. wolfc's
conform-run at the pin answers `fail(E0301)` — `fs_read_text`,
`fs_write_text`, `eprint` "not in scope" — on files its own corpus pins
at `mem`/`run` and its own checked-lane tests execute. The spec is
clear (`[conf.directive.phase]`: the directive is the truthful ledger)
and the corpus is upstream's own contract, so the *conform-run surface*
is the defendant: the s38 builtins exist on the checked lane but the
conform-run wiring landed after `f0da6e6`. This machine runs
`io/eprint.lu` to the pinned stdout (stderr is the human channel,
never hashed) and declines the fs tier honestly (no filesystem by
design — `[proto.record.unsupported]`). Expected to resolve at the
next pin bump; if it does not, the filing escalates to a wolf-lang
issue.

### DIV-2026-014 — the strings statics — **RESOLVED by `[proto.cmp.rung]`, pin `e94b879` (0.1.7)**

Resolution: the `[proto.cmp.rung]` ruling (spec/06, s70) — the rung-
placement residue was all that remained after the `13b811f` wiring
closure, and the ruling absorbs it. The original filing:

`strings/char_index_fail.lu` (pins fail(E0411)),
`strings/format_spec_malformed.lu` (fail(E0412)),
`strings/format_spec_mismatch.lu` (fail(E0413)). As filed (0.1.5): this
machine rejects all three with the pinned codes at its resolve rung;
wolfc's conform-run at `f0da6e6` reported `unsupported` — the emissions
were not reachable through its conform-run surface. **Status update,
pin `13b811f` (0.1.6):** the wiring half closed as predicted — wolfc
emits all three pinned codes at its `typecheck` rung now. Spans:
E0411 agreed already (`[420,424]`); for E0412/E0413 this machine
realigned to the counterparty's `:spec` shape (`[530,534]` = `:>08`,
`[414,417]` = `:.2` — 0.1.5 spanned the whole hole). What remains is
rung placement only — the DIV-2026-011 question, resolved by the same
future `[proto.cmp]` ruling.

---

Eighth corpus differential: lupin 0.1.4, pin `ad6cef7` (s29+s30: real
glibc behind the unsafe tier, the erring-main pin, `fcmp.ne` as IEEE
unordered, module-path-qualified WIR names; 165 entries compared, 18
members through their entries, counterparty built CLEAN at `ad6cef7`),
**1 divergence, filed as DIV-2026-011** — and **DIV-2026-010 CLOSES**:
s29 moved wolfc's E0410 emission to the resolve rung (with the
`[conc.when.body]` exemption wolf-lang#21 carried from this machine) and
re-pinned the two corpus `phase:` directives resolve → parse, so
`typecheck/let_reassign.lu` and `typecheck/let_compound_assign.lu` now
reject on both sides at `resolve`, same code, same span — the eighth
round compares them clean, exactly the closure condition the seventh
round wrote down. The new filing is the same *shape* in the opposite
direction: `memory/mode_missing_mut.lu` rejects with **E1007 at the same
span on both sides** ([405,408], the argument), this machine at
`resolve` (issue #15's fix — sema-lite is its only static tier and the
signature is visible there), wolfc at `mem` (where mode checking lives
in its pipeline). 268 conservatism-ledger entries (63 rejects-beyond by
the counterparty, 79 run-unmatched — the wave's new run-rung witnesses
land ahead of the counterparty's run tier — 90 counterparty-unsupported,
36 interp-unsupported).

### DIV-2026-011 — `memory/mode_missing_mut.lu` — **RESOLVED by `[proto.cmp.rung]`, pin `e94b879` (0.1.7)**

Resolution: exactly the first branch the triage asked for — a
comparison-rule clause. `[proto.cmp.rung]` (spec/06, s70): same code +
same first-diagnostic span across `fail` records is agreement, the rung
recorded, never compared; one verdict wide, so `fail` against any other
verdict still diverges. The eleventh round compares this file clean at
`E1007`/`[405,408]`, resolve here, mem there. The original filing:

Filed 2026-08-10 (lupin 0.1.4, CLEAN wolfc build at `ad6cef7`).

- Class: verdict (rung placement only). Codes and spans byte-identical
  (`E1007` at `[405,408]` — the argument expression).
- a (lupin): `fail(E1007)@resolve` — issue #15's fix. The X1 call-site
  mode law's disagreement ran to a silently wrong answer here;
  `[conf.trap.map]` gives E1007 no dynamic meaning, so the honest stop
  is the rung where the callee's signature is visible, which for this
  machine is sema-lite at `resolve` (the E0410 precedent).
- b (wolfc ad6cef7): `fail(E1007)@mem` — mode checking is its memory
  tier's, after typecheck completes.
- Triage: **spec first defendant** — `[proto.cmp]` has no allowance for
  same-code-same-span rejections at different rungs across
  implementations of unequal pipeline depth, and DIV-2026-010 already
  spent one round on exactly this shape. Routed upstream for either a
  comparison-rule clause (same code + same span ⇒ agreement, rung
  recorded) or a rung ruling for E1007; whichever lands, one side's
  surface moves (or the comparison absorbs it) and this entry closes.

Seventh corpus differential: lupin 0.1.3, pin `d147a54` (s27+s28: the
spec's `[mem.iter.*]`/`[mem.str.*]`/`[conf.trap.assert]`/postfix-row
grammar land compiler-side and this machine realigns; 161 entries
compared, 16 members through their entries, counterparty built CLEAN at
the pin), **2 divergences, both still DIV-2026-010** — unchanged from
the sixth round: same E0410, same spans, wolfc's record says `typecheck`
where the corpus pins `phase: resolve`. **Re-verified at d147a54: the
fix has NOT landed at this pin.** It is in flight upstream — s29 work
moving the emission into a resolve-rung `letcheck` (and re-pinning the
corpus `phase:` directives resolve → parse) landed on trunk after this
pin (`626175b`/`6bfff9a`, CI still running at the close of this pass); this machine's heads-up that the new
walker must exempt `when`-body assignments per `[conc.when.body]`
(`when (a, b) { a += 10 }` on `let`-bound Mutex operands —
`conc/when_multi.lu` and `procs.lu` pin `run(exit=0)`) is filed as
wolf-lang#21. The entry closes when s29 lands and the eighth round
compares clean. 261 conservatism-ledger entries (64 rejects-beyond by
the counterparty, 75 run-unmatched, 86 counterparty-unsupported, 36
interp-unsupported — down from 46: impl-method dispatch, postfix rows,
numeric casts and the iterator protocol moved ten files onto this
machine's run rung).

Sixth corpus differential: lupin 0.1.2, pin `a0c4564` (the E0410
fail-files and the unsafe/checked memory tier land; 159 entries compared,
16 members through their entries), **2 divergences, both filed as
DIV-2026-010** — the first non-zero round since is07, and both are one
finding: `typecheck/let_reassign.lu` and `typecheck/let_compound_assign.lu`
reject on both sides with the **same E0410 at the same span**, but the
counterparty's record places the rejection at `typecheck` while the corpus
files themselves pin `phase: resolve`. The deep comparison reads wolfc's
record as claiming the resolve rung *completed*, which collides with this
machine's honest rejection at the rung it performs — a rung-placement
inconsistency between the compiler's record and its own corpus directive,
not a verdict disagreement. **Triage: spec/corpus first defendant** —
either the corpus directives should say `typecheck` or wolfc's driver
should report sema's E0410 at `resolve`; routed upstream with the filing;
this machine's placement follows the corpus. 261 conservatism-ledger
entries (64 rejects-beyond by the counterparty — the E1301/E1302 unsafe
tier landed — 67 run-unmatched, 84 counterparty-unsupported, 46
interp-unsupported).

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
adopted and where this machine realigned. S-9, S-10 and S-11 remain open.

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

- **S-10 (lupin 0.1.1, wolf-interp#4) — `[conc.task.spawn]`'s dynamic half
  is unstated: a task closure's write to a captured copy is silently
  task-local.** The clause makes capturing a `mut` borrow of enclosing
  state a *compile error* (E1101), and `[conf.trap.map]` states no dynamic
  meaning for E1101 (its table is E1001/E1002/E1004/E1005 — the E1004/E1005
  precedent is exactly how such a meaning gets added). This machine
  captures by value (`[gram.expr.closure]`; approximation-contract §10.2),
  so the E1101 shape *runs*, each task writes its own copy, and the
  cross-task write is lost without a fault — a wrong-looking answer, not a
  trap. The corpus and the book agree with the machine today:
  `conc/store_buffer.lu` is the pinned exemplar (exit 0, task-local
  effects, the standing conservatism class), and wolf-book ch13/appendix
  exercises document "exit 0 — captures by value" with the static E1101
  rejection pending on the compiler side. The s20 S-batch did not speak to
  it, and DIV-2026-008 (`freeze_publish`) was this family's first costume.
  **Not fixed here, deliberately**: a spawn-time capture-analysis trap
  would be this implementation legislating a dynamic meaning the spec
  never states — the same line `ledger::dynamic_meaning` refuses to cross.
  Routed upstream: spec/03 (or `[conf.trap.map]`) should either state
  E1101's runtime meaning (kind + clause, as the E1004/E1005 amendment
  did) or bless capture-by-copy as the defined interpreter-tier semantics.
  Until then §10.2 stands as the documented behavior.
  **Status update, pin `13b811f` (0.1.6, issue #19):** the #41 capture-law
  wave hardened the *static* half — `conc/store_buffer.lu` re-pinned to
  `fail(E1101)` and this machine now rejects it at resolve with the
  counterparty's code and span (DIV-2026-015), so the E1101 shape no
  longer runs here and the silent-write-loss face is unreachable through
  the pinned corpus. The clause still states no runtime meaning, so the
  dynamic question stays open exactly as filed; capture-by-value remains
  §10.2's documented semantics for the shapes the static walk cannot see.

- **S-11 (lupin 0.1.2, wolf-interp#9 / wolf-std F-0014 / wolf-lang#15) —
  container mutation during `for` iteration has no governing clause.**
  `loop_expr ::= 'for' pattern 'in' expr block` is the whole of the pinned
  text on `for` (`[gram.expr.flow]`): nothing states whether the loop
  moves its operand, holds a `mut`-grade access on it for the loop's
  extent, or reads it once. The three candidate readings produce three
  different verdicts on `for x in xs { xs.push(x) }`, and both
  implementations picked one: **wolfc** (a0c4564) lowers the operand as a
  *move* and rejects the body's use statically — `fail(E1001)`, "`xs`
  moved here", with a `for x in copy xs` fix-it — even though
  `[mem.tier0.move.1]`'s move list (assignment, initialization, `take`
  arguments, `return`) does not include loop operands; **lupin** evaluates
  the operand once at loop entry and iterates that snapshot — the MVS
  copy reading — so the program runs `exit(0)`, the pushes land, and the
  iteration never observes them (approximation-contract §6.8). A third
  reading — the loop holds the container `mut`-style for its extent —
  would make the body's push a `trap(exclusivity)` under
  `[conf.trap.map]`, and no clause states that hold either.
  **Not legislated here, deliberately**: the snapshot loop cannot produce
  a spurious fault (the one direction the approximation contract
  forbids), and inventing a move or a hold the spec never names is the
  compiler-alignment shortcut `ledger::dynamic_meaning` exists to refuse.
  Routed upstream: spec/01 (or spec/02 §2) should state the operand
  semantics of `for` — move (blessing E1001 and its dynamic
  `use-after-move` half), extent-hold (naming the `exclusivity` trap), or
  loop-entry copy (blessing this machine and making wolfc's E1001 a
  conservative extension). wolf-std keeps the divergence visible in CI:
  `tests/list/mutate_while_iterating.lu`, ledgered `lupin = run` /
  `wolfc = fail(E1001)`. Compiler half: wolf-lang#15.

## Resolved findings

### DIV-2026-010 — `typecheck/let_reassign.lu` + `typecheck/let_compound_assign.lu` — **resolved upstream, pin `ad6cef7` (s29)**

Closed 2026-08-10 at the 0.1.4 re-pin, by exactly the closure condition
the filing wrote down: s29's `letcheck` moves wolfc's E0410 emission to
the **resolve** rung (carrying the `[conc.when.body]` exemption this
machine flagged as wolf-lang#21 — `when (a, b) { a += 10 }` over
`let`-bound Mutex operands stays legal), and the corpus re-pins both
files' `phase:` directives resolve → parse with the rationale in the
files themselves. Eighth round: both sides `fail(E0410)@resolve`, same
span, 0 divergences on these files. The original filing (0.1.2, pin
`a0c4564`): class verdict, rung placement only — codes and spans
byte-identical (`E0410` at `[444,445]` / `[307,312]`), lupin at
`resolve`, wolfc at `typecheck`.

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

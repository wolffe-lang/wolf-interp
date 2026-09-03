# Divergence log — the is05 ledger

Every divergence the differential runner finds lands here with its triage.
This document and `differ::FILED_DIVERGENCES` are one ledger in two forms:
the table below is the human record; the constant is what lets the runner
annotate a known finding (`x-filed`) and stop gating on it while it awaits
its fix. **An entry here never closes a finding.** It routes it. Closure is
the iron rule: the resolving commit lands a corpus file and a spec clause
citation, and then the entry moves to the *resolved* section.

## The triage workflow (`[proto.cmp.triage]`)

The runner emits, for each unfiled divergence, a filing template (visible
with `lupin diff-run --filing`): the program, both verdicts, the class,
the rung the comparison fired at. A human then walks the decision tree, and
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
schema rejects) and `timeout`. The conservatism ledger is tracked and
reported but never gates (`[proto.record.unsupported]`). It covers
`unsupported` on either side, rejections at rungs the other side does not
perform, and run-tier outcomes the pre-M1 compiler cannot check.

## Counterparty acquisition (integrator ruling, is05)

Building and executing the pinned compiler is **legitimate binary
acquisition**: the binary is data consumed through the spec/06 protocol,
exactly like the corpus. Reading or studying its *source* remains
forbidden, because independence is about shared code and shared blind
spots. Locally: `cargo build -p wolf_driver` inside `upstream/` (the
submodule is sparse-checked-out to `spec/` + `corpus/`; widen it for the
build, then restore it, and the binary survives in `upstream/target/`). In
CI the vendored snapshot has no `crates/` and the private submodule cannot
clone, so the differential lane detects the absence, prints `notice:`
lines, and SKIPs. `--require-counterparty` turns the skip into a hard
failure.

`cargo build -p wolf_rt` as well, since 0.1.10: it produces the
`libwolf_rt.a` the compiler's `--native` and `--release` lanes link
against. Without it those lanes decline as a *tool* ("libwolf_rt.a not
found next to the `wolf` binary"), which the runner reports as
`ToolError` rather than degrading quietly to a shallower lane.

## The counterparty's tiers (`--counterparty-tier`, 0.1.10; re-measured 0.1.11)

wolfgang's `conform-run` is one process contract over several engines,
chosen by flag, and the flag decides how deep its record goes. The
harness drives the choice with `diff-run --counterparty-tier=`:

| tier | flag | counterparty reaches `run` on | both execute |
| --- | --- | --- | --- |
| `default` | *(none)* | **0** of 258 entries | **0** |
| `checked` | `--checked` | 127 | 114 |
| `native` | `--native` | 122 | 116 |
| `release` | `--release` | 112 | 106 |

Re-measured at pin `f8dca42` (0.1.11), and re-measured **independently
of the runner** — the table is the lane's own audit, so deriving it from
the lane it audits would be circular. Both columns come from invoking
each side once per entry per tier and reading `phase_reached` off the
records (`tests/`-external; the script and its four `tier-*.json`
outputs are scratch, the numbers are here). The `default` row is why
this table exists: through 0.1.9 the runner passed no flag, the compiler
answered `unsupported` at `wir` on every run-tier program, and the whole
dynamic half of the corpus compared **nowhere**. That row is still 0,
and it is still correct that it is 0 — the audit confirms the lane
reports honestly rather than that the gap reopened.

**The three run-reaching lanes are NOT nested, which the 0.1.10 table
did not show.** `checked` reaches `run` on more files than `native`
(127 vs 122) yet compares fewer (114 vs 116), because each lane declines
a different tier:

- `checked` declines the whole conc tier — 11× `conc/` + `procs.lu` +
  `test/conc_schedules_test.lu` + `rows/qmark_defer.lu` +
  `typecheck/match_exhaustive.lu` (15 files `native` compares).
- `native` declines most of the unsafe/region/shared tier — `regions.lu`,
  `memory/unsafe_*` ×4, `memory/shared_ok.lu`,
  `memory/handle_stale.lu`, `memory/region_multiopen_swap.lu`,
  `comptime/norm_linear.lu`, `rows/coarsen.lu`, `traits/dyn_ok.lu`,
  two `lints/` (13 files `checked` compares).
- `release` declines the conc tier by name (10 files vs `native`), the
  compiler's documented posture — conc lowering belongs to the debug
  tier — and conservatism, never divergence.

So the honest coverage figure is the **union: 129 files** compared at
`run` by at least one lane, with 101 compared by all three. Running one
lane and calling it "the run tier" would miss up to 15 files; this is
why the pass runs all four. Of this machine's 185 run-reaching entries,
**56 are met by no counterparty lane at all** (the `comptime/`, `fs/`,
`net/`, `os/`, `json/` and `projects/` tiers, plus the analyses only one
side performs) — the residue the differential still cannot see, and the
number to drive down.

`release` is the lane that matters most: it runs s42's mid-end and s43's
whole-program layer, so comparing it against this machine is the
falsifiable form of "optimization preserves observable behavior". Our
own side is always invoked plainly — this machine has one engine, and
the tier selects which of the *counterparty's* engines answers.

## Open findings

### The byte in the mirror — is35, lupin 0.1.24, pin `3befc3e` (wolf-lang v0.2.3)

The sprint that made a code mean one thing. Three findings closed, one
opened, one waiver retired by the ruling it was waiting for, and a lint bug
that only GitHub's checkout path could see.

**wolf-lang#225 — RESOLVED HERE, and the clause never moved.**
`[gram.lex.str.escape]` has read "`STR_ESC` … and nothing else; any other
`\` is **E0101** at the escape" since #198 landed in v0.2.2 — the pin
0.1.23 was released against — so the number was never this implementation's
to choose. It answered **E0103**, and a `\u` with no braces answered
**E0104**, which are the two numbers the catalog spends on the multiline's
LAYOUT. A program refused for a bad escape and one refused for a badly
shaped `"""` were the same record here. Triage case 3 all the way through:
the clause was unambiguous and this implementation was the defendant.

**Span parity was exact before the change and after it**, which is why 484
files never showed it — #198's two witnesses pin the `\u{…}` DIGIT BOUND,
where both machines already answered E0101, and the corpus walk compares the
`check:` code rather than the span. Measured against
`wolf 0.2.3 (wolfgang, pin 3befc3e)`, the whole escape family:

| program | lupin 0.1.23 | lupin 0.1.24 | wolfc 0.2.3 |
| --- | --- | --- | --- |
| `"a\qb"` | `fail(E0103)` `[30,32]` | `fail(E0101)` `[30,32]` | `fail(E0101)` `[30,32]` |
| `"a\xZb"` | `fail(E0103)` `[30,32]` | `fail(E0101)` `[30,32]` | `fail(E0101)` `[30,32]` |
| `"a\x4"` | `fail(E0103)` `[30,33]` | `fail(E0101)` `[30,33]` | `fail(E0101)` `[30,33]` |
| `"a\u41b"` | `fail(E0104)` `[30,32]` | `fail(E0101)` `[30,32]` | `fail(E0101)` `[30,32]` |
| `"a\u{}b"` | `fail(E0101)` `[30,34]` | unmoved | `fail(E0101)` `[30,34]` |
| `"a\u{0000041}b"` | `fail(E0101)` `[30,41]` | unmoved | `fail(E0101)` `[30,41]` |

Only the number moved, on every row. The witness `grammar/multiline_bad_escape.lu`
arrived with this pin already carrying the measured divergence and reads
`fail(E0101)@lex` — **match** — in the commit that takes the clause. Its
running twin `strings/multiline_escapes.lu` answered at first sight. The
`char` literal's own E0110 did not move with the string tier's number: `'\q'`
is still one report over the whole literal.

**What freeing E0103/E0104 revealed, filed as wolf-interp#59.** They were
free because this implementation does not implement the rules they name.
v0.2.3's `[gram.lex.str.multi]` — new productions, #215 — states three layout
side conditions with three codes; this machine has one rule for all of it,
`E_DEDENT_UNDERRUN` (E0109):

| program | lupin 0.1.24 | wolfc 0.2.3 |
| --- | --- | --- |
| text after the opening `"""` | `fail(E0109)` `[31,49]` | `fail(E0103)` `[31,35]` |
| a content line left of the margin | `fail(E0109)` `[32,43]` | `fail(E0104)` `[32,34]` |

Code **and** span, twice, and no corpus file measures either — the same shape
as #225 one clause over. `tests/str_escape_code.rs` asserts that this machine
answers neither number in the meantime, so the collision cannot come back
quietly. A wolf-lang question rides along: #225 quotes the catalog assigning
**E0104** to "a multiline string line sits left of the margin", while
`[gram.lex.str.multi]`'s own sentence assigns E0104 to the *closing
delimiter* condition and E0105 to the margin one. Two documents, one number,
two readings.

**wolf-interp#57 — closed.** What `main` may return is a declaration fact
(wolf-lang#106), and this machine discovered it from the value: `finish`
looked at what came back, so `typecheck/main_returns_str.lu` executed its
whole body and wrote `hi` to the process's stdout before declining. The
record said `unsupported@resolve`, which was true — and the invocation had a
side effect the record did not report, which for an observation tool is the
hazard is34 filed rather than absorbed. The decline is on the admission
ladder now. Verdict and rung unmoved; the claim is true.

**wolf-lang#216 — the comparator half landed, and it is MEASURED EMPTY.**
`differ::run_rung` compares a trap's output bytes when both sides hold them.
This is the clause's proposed reading applied to the instrument, not to
`src/compare.rs` (which still holds `[proto.cmp.phase]` as written), and it
is safe ahead of a ruling for two reasons: a widened comparison can only ADD
rows, never hide one, and it is gated on both sides HOLDING the field —
`None` on either is `[proto.record.fields]`'s honest-absent, the posture
`[proto.cmp.warn]` already takes to a missing `warnings` array.

**Every mover, classed: there are none, and here is why that is the finding
rather than a disappointment.** The bundle has 63 trap records; **61 write
nothing before the fault**, so the widened comparison has no field to look at
and is honest-absent on both sides. The two that do write are is34's:

| file | verdict | lupin | wolfc `--checked` | `--native` | `--release` | class |
| --- | --- | --- | --- | --- | --- | --- |
| `faults/trap_skips_root_defers.lu` | `trap(assert)` | `fe91a58b…` | `fe91a58b…` | `fe91a58b…` | `fe91a58b…` | **agreement** |
| `rows/handler_diverge_trap.lu` | `trap(assert)` | `c2eba7a1…` | *unsupported* | `c2eba7a1…` | `c2eba7a1…` | **agreement** (checked lane declines to run it — conservatism, unchanged) |

So: **zero new divergence rows on any tier**, and the class is not empty
because the question is uninteresting — it is empty because the one file it
was built for was fixed at 0.1.23. Under 0.1.22's behaviour the first row
reads `inner inner-defer before-trap root-defer` here against
`inner inner-defer before-trap` there: same verdict, same trap kind,
different bytes, and **invisible for the whole of D66..r05**. That
counterfactual is pinned as a unit test with the real digests
(`differ::tests::a_trap_s_output_compares_when_both_sides_hold_it`), because
"the comparison would have caught #209" is a claim and not a comment.

**This also answers #216's sub-question, as far as three lanes can.** The
flush concern was whether a trapping program's stdout is portable across
wolfc's tiers at all. On every trapping corpus program that writes before its
fault, `--checked`, `--native` and `--release` return the same digest as each
other and as this machine. That is two files, not a proof — but it is two
files more than the one r05 had, and no tier disagrees with any other
anywhere in the corpus.

**W0316 — a lint that read the checkout path.** Not a cross-implementation
divergence at all; recorded because of how it was found. The pin brought
`conc/proc_cross_module/main.lu`, which says `use work`. The W0316 walk asks
whether a module imports one of its own ancestor MODULES, and its stop
condition tested whether a candidate WAS the entry root — which never happens
for a scope file sitting directly in it, so the walk climbed the whole
filesystem path. GitHub checks this repository out under
`/home/runner/work/wolf-interp/…`; the file warned on both Linux and macOS
runners and on no developer's machine, and the corpus `warns:` ledger caught
it. Local green is not green.

**`byte` — DEFERRED to is36, by name.** D72 rules a byte-width scalar into
the language (`[type.byte]`, modelled on `[type.char]`: 8-bit, unsigned, no
arithmetic promotion, `List[byte]` charging 1x on every tier, literals via
`as byte` only) and assigns the landing to wolf-lang s135, with is35
mirroring it. s135 has not merged: at the pin step and again at the release
commit, `origin/trunk` was `5241ab7` (the r06 merge), no `s135` branch
existed, no PR was open, and wolf-lang#203 was still OPEN. So this pin is
v0.2.3 and nothing here parses the type name, sizes a 1-byte ledger slot, or
implements the two casts. The `byte` spelling is already in this
implementation's known-non-status list for `main`'s return type
(wolf-interp#57's whitelist), which costs nothing today and is correct the
moment the type exists. is36 takes it at whichever pin carries s135.

#### The seventeenth corpus differential

Counterparty built at v0.2.3 (`cargo build -p wolf_driver -p wolf_rt` inside
`upstream/`), all three run-reaching tiers, 452 entries.

| tier | divergences | of which filed | gating after filing | conservatism |
| --- | --- | --- | --- | --- |
| `checked` | 8 (was 15) | 2 | 6 | 251 |
| `native` | 5 (was 12) | 2 | 3 | 215 |
| `release` | 5 (was 12) | 2 | 3 | 215 |

**Minus seven on every tier, and every one of them is DIV-2026-020 closing.**
D71 ruled the strong form — the span IS the offending token — s134 aligned
wolfc's parser, and wolf-lang#220's closing comment assigned the waiver's
retirement to this lane's next pin bump. Seven of its eight files are
byte-identical now. Nothing this sprint wrote caused the drop; taking the pin
did. Conservatism rises by 2 on `checked`, which is the two new run-reaching
corpus files.

**Everything still gating is older than this sprint**, and after the two
retirements the filed list is two entries again rather than nine.

### DIV-2026-021 — `grammar/let_group_bare_tuple.lu` — **OPEN, filed upstream as wolf-lang#228**

The eighth row of DIV-2026-020's table, promoted when the other seven closed.
It was never the span-WIDTH question: both machines answer `fail(E0201)` at
parse and disagree about **where**, ten bytes apart, on all three tiers.

| | span | bytes |
| --- | --- | --- |
| lupin 0.1.24 | `[364,365)` | `,` — the comma in `let a, b` |
| wolf 0.2.3 (`--checked`/`--native`/`--release`) | `[374,375)` | `\n` — the end of the initializer list |

Triage: **spec bug**, case 1. `[gram.item.let]` says what a D63 let-group is
and what the bare-tuple shape is not; it does not say where refusing it
reports. Both readings are coherent — the comma is the first byte at which
the input stops being a legal `let`; the end of the initializer list is where
the count mismatch becomes knowable, and is what wolfc's teaching note is
about ("this value has no name", with both fixes). wolfc has the better
diagnostic and this machine the better locus, which is exactly a question a
clause should settle rather than two implementations settle by imitation. The
corpus directive cannot see it: `check: fail(E0201)` pins the code, and the
walk compares codes.

### The letters in the mirror — is34, lupin 0.1.23, pin `8cda3aa` (wolf-lang v0.2.2)

Not a corpus sweep but a **record honesty** finding and its consequences.
The ledger entry is short because two of the three letters closed at
agreement.

**#209 — RESOLVED HERE, at the ruling.** `faults/trap_skips_root_defers.lu`
arrived at this pin already carrying a measured divergence: r05 recorded
every wolfc lane (`--checked`, `--native`, `--release`) printing
`inner inner-defer before-trap` where lupin 0.1.22 printed
`inner inner-defer before-trap root-defer`, both at `trap(assert)`.
`[conf.trap.exit]` gained the sentence that settles it — *a trap runs no
`defer` or `errdefer`, anywhere* — and this machine's root path took it.
The witness now agrees byte for byte. Triage case 1 all the way through:
the spec was the defendant (it ruled the proc path in s132 and was silent
about the root), the clause was amended first, and only then was the
implementation moved. is33 flagging the gap rather than guessing at it is
what made that order possible.

**#55 — the blind spot that hid it.** Through 0.1.22 this implementation
reported `stdout_inline: null` and `stdout_sha256: null` on *every*
trapping program, so the two machines were verdict-identical whatever they
printed and no amount of corpus growth would have surfaced #209 through
the differ. The record now carries the output for any verdict that reports
a completed run (`exit`, `trap`, `ub`). Every record that moved, over the
487-record bundle, compared field for field with the `commit` stamp
excluded:

| file | verdict | before | after | class |
| --- | --- | --- | --- | --- |
| `faults/trap_skips_root_defers.lu` | `trap(assert)` | `null` | `"inner inner-defer before-trap"` | **agreement** |
| `rows/handler_diverge_trap.lu` | `trap(assert)` | `null` | `"FAILED: neg\n"` | **agreement** |

Two, and both agree with the `stdout=` their corpus directive pins for the
counterparty. 63 trap records in the bundle; the other 61 trap before
writing anything, and none of the 8 `ub` records writes first. So the
answer to "are there more #209-class divergences hiding behind the null?"
is, at this pin, **no** — `handler_diverge_trap` was the only other file
whose trap-path output had never been looked at, and it was right.

**A conservatism, declared.** Reading the clause with no verdict condition
at all would move a third record: `typecheck/main_returns_str.lu`,
`unsupported@resolve`, would gain `stdout_inline: "hi\n"` — because this
machine evaluates `main`'s body before declining that `main` returned
`str`. A record whose `phase_reached` says the run did not complete makes
no run observation, so it carries none; the side effect itself is a real
finding and is filed as **wolf-interp#57** rather than smuggled onto the
wire, with `a_record_that_completed_no_run_reports_no_stdout` standing as
its red test.

**The question this leaves open, filed as wolf-lang#216.**
`[proto.cmp.phase]` still rules the run rung "for `trap`, compare kind
only", and `compare`/`differ` still implement exactly that — widening the
comparison by private agreement is what the independence doctrine forbids,
and #209 is the proof that the letter comes first. So both machines now
*hold* the observable and the protocol rules it uncomparable, which is #55's
blind spot one layer up. The sub-question that makes it more than a
one-liner: whether a trapping program's stdout is flushed to the same byte
at all three wolfc tiers, or whether the current sentence is deliberate.

#### The sixteenth corpus differential — the first since 0.1.11

The table below this section is lupin **0.1.11**'s. Eleven releases went by
on the corpus walk alone (which compares the `check:` code, never the span)
and on record self-replay, so `diff-run` against a real counterparty had not
been recorded since pin `f8dca42`. is34 built it — `cargo build -p wolf_driver
-p wolf_rt` inside `upstream/` at v0.2.2, the legitimate binary acquisition
this document rules — and ran all three run-reaching tiers.

| tier | divergences | of which filed | gating after filing | conservatism |
| --- | --- | --- | --- | --- |
| `checked` | 15 | 9 | 6 | 249 |
| `native` | 12 | 9 | 3 | 215 |
| `release` | 12 | 9 | 3 | 215 |

**The three letters agree on every lane.** `faults/trap_skips_root_defers.lu`,
`rows/handler_diverge_trap.lu`, `grammar/str_uni_seven_digits.lu` and
`strings/str_uni_leading_zeros.lu` appear in **no** divergence report at any
tier — #209 is closed against the real counterparty and not merely against
r05's transcript, and #55's second mover is confirmed right.

**Everything gating is older than this sprint**, and one class dominates it.

### DIV-2026-020 — the E02xx span convention — **RESOLVED upstream at pin `3befc3e` (0.1.24): D71 ruled the span IS the offending token, s134 aligned wolfc, seven of eight files byte-identical; the eighth is DIV-2026-021**

Eight `grammar/` files, identical on all three tiers: same code (E0201),
same byte where the refusal starts, different span **width**. This machine
spans the offending token; the counterparty emits a zero-width span at its
start. Both renderings put the caret in the same column — wolfc's own output
for `struct_literal_no_separator.lu` carets byte 550, exactly where lupin
points — so s132/D69's "byte-for-byte where lupin points" is true of the
offset and not of the span, and nothing measured the difference because the
corpus walk compares the code.

| file | lupin | wolfc |
| --- | --- | --- |
| `grammar/struct_literal_no_separator.lu` | `[550,551)` = `y` | `[550,550)` |
| `grammar/struct_pattern_no_separator.lu` | `[534,535)` = `y` | `[534,534)` |
| `grammar/tuple_pattern_no_separator.lu` | `[415,416)` = `b` | `[415,415)` |
| `grammar/closure_params_no_separator.lu` | `[581,582)` = `b` | `[581,581)` |
| `grammar/struct_pattern_rest_bare.lu` | `[669,671)` = `..` | `[669,669)` |
| `grammar/let_group_one_init.lu` | `[332,333)` = `,` | `[332,332)` |
| `grammar/range_bare.lu` | `[896,897)` = `]` | `[896,896)` |
| `grammar/let_group_bare_tuple.lu` | `[364,365)` = `,` | `[374,374)` — **offsets differ** |

Triage (`[proto.cmp.triage]`): **spec bug**, case 1. `[proto.record.diag]`
rules spans byte-offset half-open and compared; nothing says what a
diagnostic about an *unexpected token* spans. Both conventions are
defensible — zero-width reads "something is missing HERE" and pairs with the
machine-applicable insertion suggestions the counterparty's parser grew in
s131/s132; a token span reads "THIS is what went wrong". Moving either side
to match the other without a ruling is imitation, which is the mistake #209
was resolved by not making. The upstream ask is either the convention in
`[proto.record.diag]` or a `[proto.cmp.rung]`-shaped tolerance in
`[proto.cmp.phase]`: same code, same start, agree. **The last row is not the
same finding** — its offsets genuinely differ — and it is named separately so
the weaker ruling cannot silently absorb it. lupin 0.1.23 changes nothing
here: its spans are byte-identical to 0.1.22's, and #56's teach-note is
additive (a second line and a longer message, never a relocation).
`differ::DIV_2026_020_FILES` carried the waiver and is **retired at the 3befc3e pin**, which is where wolf-lang#220's closing comment placed it: seven of the eight files are byte-identical to the counterparty now, and the eighth — `let_group_bare_tuple.lu`, whose offsets always genuinely differed — is carried on as DIV-2026-021 rather than absorbed by the ruling that does not cover it. is35 re-measured all eight on all three tiers before removing the list.

#### Triage owed — carried, not filed

The residue after DIV-2026-019 and DIV-2026-020, recorded so it is not
rediscovered as new. Each wants its own analysis; none is a soundness
candidate and none moved this sprint.

| file | tiers | shape |
| --- | --- | --- |
| `generics/explicit_apply_arity.lu` | all | `fail(E0812)` both, spans `[473,483)` vs `[469,483)` — same end, different start, so `[proto.cmp.rung]` cannot absorb the resolve/typecheck rung difference |
| `grammar/index_origin_bad.lu` | all | `fail(E0813)` both, spans `[303,311)` vs `[309,310)` — disjoint extents, same clause |
| `rows/negative/tag_undeclared_arg.lu` | all | `unsupported@resolve` here against `fail(E0301)@resolve` — a scope gap wearing a verdict, not a disagreement about the program |
| `faults/cast_float_nan_trap.lu`, `faults/cast_float_overflow_trap.lu` | `checked` only | `trap(overflow)` here, `exit(0)` on the checked lane; both compiled lanes trap, so this is the checked executor's own tier |
| `faults/cast_float_to_int_truncate.lu` | `checked` only | same exit, different stdout digest; likewise checked-only |

**Unchanged:** DIV-2026-019 is still the one standing corpus-walk mismatch, still
filed, still waived by `FILED_DIVERGENCES`. Corpus verdict-identity across
the whole pin bump: 332 match, 16 dynamic counterparts, 42 conservatism,
58 out of scope. #198's string half
(`grammar/str_uni_seven_digits.lu`, `strings/str_uni_leading_zeros.lu`)
answered at first sight — E0101 at the escape, column 14, the same column
its `char` twin reports — so it never became a finding at all. #56's
teach-note is wording, outside the protocol by D22, and moved no record.

Fifteenth corpus differential: lupin 0.1.11, pin `f8dca42` (**the
largest semantic movement the compiler has had in one wave**: s74 the
correctness cluster, s75 `List` element access as a load with
caller-side bounds checks, s76 containers allocating in the ambient
region per D12, s77 `s.bytes()` as a view over the receiver's own
storage, s78 the affine relational channel, plus s53 script mode and
D43/D44; 258 entries compared, 22 members through their entries;
counterparty built CLEAN at the pin from a **deleted** `target/` with
`libwolf_rt.a` provisioned). Run **four times**, once per counterparty
tier.

| tier | divergences | conservatism | both execute |
| --- | --- | --- | --- |
| `default` | 0 | 422 | 0 |
| `checked` | 1 | 181 | 114 |
| `native` | 1 | 184 | 116 |
| `release` | **1** | 204 | **106** |

**THE HEADLINE: the compiler moved its lowering out from under three
semantic areas and this machine's independent reading already agreed on
every one of them.** Ten of the wave's thirteen new corpus files reach
`run` here at FIRST SIGHT, with no new semantics written on this side —
including all four of the wave's own semantic witnesses. The one file in
the wave that needed a reading here was s53's `[gram.lex.shebang]`, the
wave's only `spec/` delta. The corpus-wide differential found **no new
divergence**; the single one it reports is DIV-2026-017, unchanged.

The three probes the re-pin was run to answer:

1. **Region-scoped container lifetime (s76) — AGREE on every defined
   shape, with one declared gap.** A container built in a region and
   freed with it, a callee allocating into its *caller's* region (D12 —
   the reason the ambient region is dynamic rather than lexical), growth
   across several region chunks, `freeze` letting a container outlive
   the block that built it, and nested regions: all identical on
   `lupin`, `--native` and `--release`. This machine has always modelled
   regions dynamically and has always placed a callee's allocation in
   the ambient region, so s76 is a move *toward* this machine's reading.
   The gap is the escape: `memory/region_escape_container.lu` is E1010
   on every compiler lane and `exit(0)` here, because the escape is a
   **static** region judgement this machine does not make — and, unlike
   the handle/pool escape (`tests/faults/region_uaf.lu`, which traps
   `region-fault`), this machine does not catch it dynamically either. A
   read through the escaped container after its region closes answers
   with the old values rather than trapping. That is conservatism in the
   ledger's sense — no conforming program can observe it, since the
   compiler rejects the shape statically — but it is a real modelling
   gap and it is now declared in the approximation contract (§6.13)
   rather than left implied.
2. **Byte views and the slice domain (s77) — AGREE, exhaustively.**
   `s.bytes()` is a view over the receiver's own storage on both sides:
   unsigned 0..=255 (the two continuation bytes of `é` are 195 and 169,
   never negative), length is the byte length, and the empty walk
   allocates nothing. The slice domain was swept rather than sampled:
   all 100 endpoint pairs of `s.get(a..b)` over the mixed-width `é€`
   from −2 to 7 — **including the whole negative half the corpus file
   does not reach** — are byte-identical on `lupin`, `--native` and
   `--release`, with exactly the six defined pairs the domain admits and
   a *miss* (never a wrap-around) for every negative endpoint, which is
   the `lo <=u hi <=u len` unsigned reading agreeing on both sides. The
   trapping form `s[a..b]` was swept over 29 ugly pairs — negative,
   inverted, mid-codepoint, past-end, degenerate-empty, open-ended,
   inclusive, and the `^n` from-end forms — and **28 of 29 agree
   exactly**. The 29th is a new finding, below.
3. **Line-atomic print (D43) — AGREE, and this machine was the prior
   art.** The interpreter renders a whole line, interpolation and all,
   and hands it to a single `out()` call; there has never been a yield
   point inside a `print`, so it was line-atomic by construction before
   D43 was ruled. Measured rather than asserted: eight tasks × 40 long
   multi-segment interpolated lines, 20 runs of the compiler's `--native`
   lane = **6400 lines, 0 torn, across 20 distinct interleavings** (the
   interleavings differ every run, which is what proves the threads
   really do race and the probe is not measuring a serialization). The
   same program on this machine: 3200 lines, 0 torn, one interleaving —
   the sim scheduler is deterministic by design. No tearing on either
   side, so nothing to file.

### DIV-2026-019 — `resolve/broken_sibling/entry.lu` — **OPEN: which parse error fires on an unparseable module sibling**

Found 2026-08-28 (is27) at the `e561c6f` pin bump: the s124 D59 wave's
broken-sibling witness pins `check: fail(E0202)` — the counterparty
reads `mangled.lu` (`fn mangled( {{{ not wolf at all`) to EOF inside
the mangled item — where this machine stops at the **first bad token**
and answers `fail(E0201)`@parse ("expected an identifier, found `{`",
`[gram.item.fn]`, mangled.lu 4:13). Same rung, same verdict class
(both reject at parse), **span-or-code** severity; not a soundness
candidate.

Triage (`[proto.cmp.triage]`): **spec bug**, decision-tree case 1 —
the spec is silent. E0201/E0202 are unpinned implementation choices
(`diag::UNPINNED_CODES`), no clause assigns either to junk recovery,
and which failure fires on unparseable text is a parser-recovery
choice the grammar does not rule. Mimicking the counterparty's
read-to-EOF recovery to hit its code number would be imitation, not
conformance (the independence doctrine). Proposal upstream: pin the
*class* (`fail` at parse) for this witness, or rule first-error
recovery in spec/01 — either resolves this entry. `FILED_DIVERGENCES`
carries the waiver; the corpus-walk gate resumes on this file the
moment the entry resolves.

### DIV-2026-018 — `s[..]` — **OPEN, filed upstream: the compiler admits a bare `..` range the grammar excludes**

Filed upstream as **wolf-lang#88**. Found 2026-08-13 (lupin 0.1.11,
CLEAN wolfgang build at `f8dca42`) by the s77 boundary sweep, on the
29th of 29 ugly endpoint pairs. Class **verdict** (accept-set: one side
rejects what the other accepts); not a soundness candidate. **No corpus
file witnesses it**, so it cannot take a `FILED_DIVERGENCES` entry —
that list is keyed by corpus file — and it is recorded here until a
witness exists, exactly as wolf-lang#71's stdout finding was.

```
program   let s = "é€" ; let t = s[..] ; print("ok {t.len}")
lupin     fail(E0201)@parse — "expected an end of statement"
wolfgang  exit(0), stdout "ok 5"   (identical on --checked/--native/--release)
```

Triage (`[proto.cmp.triage]`): **compiler bug**, decision-tree case 2 —
spec clear, interpreter matches it. `[gram.expr.primary]` gives

```ebnf
range_expr ::= r_end (('..' | '..=') r_end?)? | ('..' | '..=') r_end
```

Two alternatives, and **neither admits a bare `..`**: the first requires
a leading `r_end`, and the second requires a *trailing* one — the `?`
that makes an endpoint optional appears only in the first. So `a..`,
`..b` and `a..b` are expressions and `..` is not. That is very unlikely
to be an oversight, because `..` already has a different meaning one
production away (`error_row`'s rest marker, §1 line 167), which is
exactly the ambiguity an unrestricted bare `..` would create.

Extent, probed: the over-acceptance is in the **parser**, not the slice
path. `s[..=]` is also admitted and answers `trap(bounds)`; `s.get(..)`,
`let r = ..` and `xs[..]` on a `List` all parse on the compiler and
decline later as `unsupported`@`resolve` with **no diagnostic**, which
is the signature of a program that got past parse. Only `for i in ..`
is rejected by both, at E0201. So one parser rule over-accepts and two
spellings reach `run` with observable answers.

Recorded counter-argument, because the human ruling may go the other
way: `s[..]` meaning "the whole string" is what a reader would guess,
and a language may well want it. But the pinned grammar does not have
it, and the triage workflow makes the spec the defendant *first* — if
the intended answer is that `..` should be admitted, the fix is a
grammar clause landing before either implementation moves, which makes
this case 1 and a spec bug. wolf-lang#88 carries both readings;
this machine is not changing its parser ahead of that ruling, for the
same reason it did not land E1101's lend spelling ahead of wolf-lang#71:
conforming to an unwritten clause trades a clean rejection for a
divergence.

### The E1101 capture law: wolf-lang#71's interpreter half LANDED

The 0.1.10 round left this open deliberately: "the interpreter half is
to extend its capture analysis to treat a `(mut x)` receiver-lend of a
captured binding as a write and emit E1101 there too… it lands once
wolf-lang#71's fix fixes the span to match." s74 landed the compiler's
half, so this round landed ours. `lint::Walk::capture_lend` routes both
lend spellings — the X1 moded receiver `(mut xs).push(1)` and the
call-site argument mode `f(mut n)` — through the same door as
assignment, and the spans are the counterparty's byte for byte:
`[913,915]` and `[562,563]` on the wave's two new files, `[545,546]` on
the assignment twin. W1101 is deliberately NOT emitted for the lend
spellings: its text is about a write landing on the task's own copy,
which is an assignment's shape, and the counterparty emits E1101 alone
there — as do the corpus headers, which carry `warns:` on the
assignment file only.

This also retires the second finding recorded under wolf-lang#71 at
0.1.10 — the mut-lend program printing `0` here against wolfgang's `2`,
because closures capture by value. **Neither machine runs the program
now**; both reject it at their own rung with the same code and span, so
the stdout divergence is unreachable and needs no witness.

---

Fourteenth corpus differential: lupin 0.1.10, pin `613c3dc` (the
mid-end/whole-program wave: s42 the optimizer, s43 clusters + body dedup
+ the frozen summary index, s63 diagnostics polish; 245 entries
compared, 22 members through their entries; counterparty built CLEAN at
the pin with `libwolf_rt.a` provisioned). Run **four times**, once per
counterparty tier — the first round in which the run tier compared at
all.

| tier | divergences | conservatism | both execute |
| --- | --- | --- | --- |
| `default` | 0 | 403 | 0 |
| `checked` | 1 | 176 | 107 |
| `native` | 1 | 183 | 107 |
| `release` | **1** | 201 | **98** |

**THE HEADLINE: the mid-end changed nothing observable.** The release
lane — s42's optimizer and s43's whole-program layer both ON — produces
the same answer as this machine on all 98 files both execute, and the
single divergence it reports is the SAME one the `checked` and `native`
lanes report, byte for byte. A transformation that altered behavior
would have shown up here as a release-only finding; there is none. The
comparison is what makes "check elimination 10 of 10, 58.2% corpus-wide,
IR volume 84.6% of naive" a safety claim rather than a throughput one.

Honest scope: linux x86-64, one platform, unseeded; the corpus's
`kernels/` tier (the three files s42's own gates read) is three programs,
so "the optimizer is correct" is not what this shows — "the optimizer did
not change these 98 observable behaviors" is.

The one divergence is **DIV-2026-017**, filed below: a raw-literal decode
bug in the compiler's front end, identical on all three of its
run-reaching tiers, which is precisely how it is known NOT to be the
mid-end's doing.

### wolf-lang#71's premise corrected: lupin never said E0202

wolf-lang#71 (SOUNDNESS: E1101 misses `(mut x)` receiver-lends) records
that "lupin rejects the same program with **E0202** (a different code, so
the pair also disagree)". Re-verified at pin `613c3dc`, that is not what
this machine does, and E0202 could not have meant what the issue reads
into it: **`E0202` is this machine's `E_UNEXPECTED_EOF`**
(`src/diag.rs:186`), a *parse* code. It was never a judgement about the
capture law, so there is no code disagreement to reconcile — the
observation was a parse failure on some earlier spelling of the program
text, not lupin's verdict on the race.

What the two machines actually do with the issue's two spellings:

| program | lupin | wolfgang |
| --- | --- | --- |
| `n = 1` in two tasks (assignment) | `fail(E1101)` @resolve, span `[71,72]` | `fail(E1101)` @typecheck |
| `(mut xs).push(1)` in two tasks (mut-lend) | `exit(0)`, prints `0` | `--native`: `exit(0)`, prints `2` |

So on the assignment spelling the two **already agree on E1101**, at
their own rungs, which `[proto.cmp.rung]` makes agreement outright. On
the mut-lend spelling **neither** machine rejects: the hole is not
lupin-versus-wolfgang, it is a hole in both, and this machine's is
recorded here as its own gap rather than left implied by the issue.

The proposal, for the fix in flight: **E1101, on both sides.** This
machine does not want a distinct code and will not propose one — the
capture law has a code, both implementations emit it for the spelling
they do catch, and the corpus pins it (`conc/store_buffer.lu`,
`conc/chan_unsendable.lu`, the E11xx admission ladder). The interpreter
half is to extend its capture analysis to treat a `(mut x)`
receiver-lend of a captured binding as a write and emit E1101 there too.
Deliberately NOT done in this round: the code is corpus-pinned, and
landing our span before the compiler's would trade a missing diagnostic
for a span divergence. It lands once wolf-lang#71's fix fixes the span
to match.

A second, separate finding rides along, and it is this machine's own:
the mut-lend program prints `0` here against wolfgang's `2` because
closures capture free variables **by value**
(`[gram.expr.closure]`, `docs/approximation-contract.md`), so each task
mutates its own copy of `xs`. Whichever way the capture law lands, that
is a real stdout divergence on a program no corpus file witnesses —
the shared-gap class the issue itself names. It cannot be filed as a
`FILED_DIVERGENCES` entry (that list is keyed by corpus file) and is
recorded here until a witness exists.

### DIV-2026-017 — `lints/raw_interp_braces.lu` — **RESOLVED upstream (wolf-lang#76 closed); re-measured clean at pin `3befc3e` (0.1.24) on all three tiers**

**CLOSED at the 3befc3e pin (is35).** wolf-lang#76 is closed and the file
answers `{who}\n` on `--checked`, `--native` and `--release`, byte-identical
to this machine. It had already stopped diverging by v0.2.2 — the sixteenth
differential does not list it — so the waiver in `differ::FILED_DIVERGENCES`
outlived the divergence by a release, which is the wolf-lang#177 lesson in a
smaller shape: a waiver nobody re-measures is a green report that means
nothing. It is removed, and `differ`'s own test now asserts the retired
entries are gone rather than merely that the remaining ones are present.

The original filing, kept for the record:

Filed 2026-08-12 (lupin 0.1.10, CLEAN wolfgang build at `613c3dc`).
Class **stdout**; not a soundness candidate. The first finding ever
produced by a run-reaching counterparty lane, and it was waiting the
whole time.

**RE-CONFIRMED 2026-08-13 at pin `f8dca42`** (lupin 0.1.11, CLEAN
wolfgang build from a deleted `target/`), which was this round's
assignment for wolf-lang#76. Unchanged in every particular: still
`exit(0)` on both sides, still `"{who}` there and `{who}` here, still
**byte-identical shas to the original filing** (`7ff0fa2b…` /
`2e5a9158…`), still the same answer on all three of the compiler's
run-reaching tiers. The corpus file's header still reads
`check: run(exit=0, stdout="{who}")` at `phase: wir`, so the pin still
masks a bug that is now one-sided, and the corpus walk here still scores
the file `match` at the `run` rung. wolf-lang#76 is OPEN and correctly
so; the entry stays open with it. Nothing in the s74…s78 wave touched
the lexer's literal decode.

**RE-CONFIRMED AGAIN 2026-08-13 at pin `4e316ad`** (lupin 0.1.12).
Third confirmation, third pin, and nothing has moved: the same two
shas (`7ff0fa2b…` / `2e5a9158…`), the same answer on `--checked`,
`--native` and `--release`. What is new is the company it keeps —
it is now the **only** divergence any of the three tiers reports,
so the whole differential reduces to this one open upstream bug.
s79 (bench integrity), s80 (the `region.foreign` aliasing fix) and
s81 (str equality, `str_from_utf8`) touched neither the lexer nor
its literal decode. Open across three more compiler sprints.

```
lupin    stdout = "{who}\n"   sha 2e5a915893921b688dce9a7c81a122308247a2cf28ea9b8540f3abdba265ad8e
wolfgang stdout = "\"{who}\n" sha 7ff0fa2be6e89ffbd94f02c39786d85d6f2ebfa013bd8ce869a68d6471dc6693
```

The program is `let s = r"{who}"` followed by `print(s)`. Identical on
`--checked`, `--native` and `--release`, which places it in the shared
front end (the lexer's literal decode) and rules out the mid-end.

Triage: **compiler bug**, decision-tree case 2 (spec clear, interpreter
matches it). `[gram.lex.str.raw]` is explicit — `r"…"`, `r#"…"#` carry
the bytes between the quotes, no escapes, no interpolation — so
`r"{who}"` is the six bytes `{who}`. The compiler's answer keeps the
opening quote of the `r"` delimiter, which is the naive
first/last-byte quote strip applied to a two-character opening
delimiter. The corpus file's own header agrees with this machine:
`check: run(exit=0, stdout="{who}")`.

The file's `phase: wir` pin is the interesting part. Its header explains
the pin: at s40 **both** executors had this bug, agreed with each other
and disagreed with the header, so the file was pinned short of `run` to
keep the disagreement out of the comparison. This machine has since
fixed its decode; the compiler has not. The pin is therefore now
masking a one-sided bug, and the corpus walk here already scores the file
`match` at the `run` rung. With the fix, the header should advance to
`phase: run`.

---

Thirteenth corpus differential: lupin 0.1.9, pin `0b4e79c` (the c09
wave: s41 release tier, s51 package manager, s73 native concurrency;
241 entries compared, 22 members through their entries, counterparty
built CLEAN at `0b4e79c` with `libwolf_rt.a` provisioned), **0
divergences**. `FILED_DIVERGENCES` is empty again, and **DIV-2026-016
CLOSES**: the CLEAN build answers `fail(E0809)@typecheck`, span
`[518,523]`, which is `[proto.cmp.rung]` agreement with this machine's
resolve rung (see the entry). 395 conservatism-ledger entries (66
rejects-beyond by the counterparty, 130 run-unmatched, 152
counterparty-unsupported, 47 interp-unsupported).

THE CONC TIER RAN ON BOTH MACHINES this round, the first corpus tier
where both implementations *execute* concurrency (s73 advanced nine
files' headers to phase `run`: 8× `conc/` + `procs.lu`; the pin adds
`test/conc_schedules_test.lu`, also `run`). The harness's diff-run lane
still invokes plain `conform-run`, where wolfgang's deepest rung is
`wir` (the native rung is opt-in: `--native`/`WOLF_NATIVE=1`), so the
ten files ledger as counterparty-unsupported there. That is
conservatism, never divergence. The machine-to-machine comparison was
therefore run directly: `wolf conform-run --native --seed=N` vs
`lupin conform-run --seed=N`, seeds {0, 42}, all ten files. **10 of 10
agree on verdict, exit and output bytes, at both seeds, both records
`"seeded": true`**, a `[proto.seed.equal]` comparison (equal seeds ⇒
comparable observations including output bytes). Honest scope: two
seeds, one platform (linux x86-64), debug tier; the seeded-tie-break
files (`conc/select_seeded.lu`, `test/conc_schedules_test.lu`) agree at
these seeds and both their outcomes are conforming by design. Zero
semantic divergence between the two schedulers' observable behavior on
the pinned witnesses.

---

Twelfth corpus differential: lupin 0.1.8, pin `26fa98e` (wave seven:
r01 prep + s71 release polish, then the one lawful mid-pass re-pin when
s72's mode teeth merged; 240 entries compared, 22 members through their
entries, counterparty built CLEAN at `26fa98e`), **1 divergence, filed
as DIV-2026-016, not a soundness candidate**. 394 conservatism-ledger
entries (65 rejects-beyond by the counterparty, 130 run-unmatched, 152
counterparty-unsupported, 47 interp-unsupported: the fs/net/proc-spawn
tiers, comptime, and the compiler-only analyses). The D39/D40 dynamic
mirrors landed this round with their spec text in the pin
(`[mem.iter.excl]`; the trap map's E1013 row). s72's three fail-files
each produce this machine's `trap(exclusivity)`:
`memory/read_param_write.lu` (E1014), `memory/mut_read_overlap.lu`
(E1002), `memory/list_mutate_while_iter.lu` (E1013). They ledger as the
expected static/dynamic counterpart pairing (the corpus walk counts them
so via `ledger::dynamic_meaning`; the differ ledgers the rung difference
as conservatism, never a divergence). One nit routed with the round: the
pinned `[conf.trap.map]` prose names E1013 in the `exclusivity` row but
not E1014, so `dynamic_meaning`'s comment carries the citation argument
until the map gains the row.

### DIV-2026-016 — `rows/negative/handler_uncovered.lu` — **RESOLVED at pin `0b4e79c` (0.1.9): unreproducible against a CLEAN build**

Resolution, 2026-08-12 (lupin 0.1.9): the CLEAN wolfgang build at
`0b4e79c` answers `fail(E0809)@typecheck`, span `[518,523]`. That is
the same code and span as this machine's `fail(E0809)@resolve`, so
`[proto.cmp.rung]` makes it agreement, and the file compares clean in
the thirteenth differential. The E0806 answer does not reproduce,
matching wolf-lang#61's own reproduction attempt at the s72 trunk
(E0809 on both the plain and `--checked` invocations). Verdict on the
filing: the 0.1.8 observation came from a wolfgang built at the
intermediate first-pin state (`3d5cee6`), where it was real and
re-filed verbatim, and the "re-verified at `26fa98e`" claim is now
attributed to a stale scratch build. That is an evidence-hygiene
lesson (rebuild the counterparty from a clean target on every re-pin,
which this round's lane now does), not a semantic finding. No
reproducer exists at any sha since the release. wolf-lang#61 closed
with this evidence. The original filing follows verbatim.

Filed 2026-08-11 (lupin 0.1.8, CLEAN wolfgang build at `3d5cee6`;
re-verified unchanged against the CLEAN `26fa98e` build after the s72
re-pin). Class: `verdict`. One file, the s71 row-coverage fail-file.

- The corpus directive pins `check: fail(E0809)` at `phase: resolve`
  (s71, wolf-lang#43: an `else` handler pattern must cover the
  operand's whole row).
- **a (lupin)**: `fail(E0809)@resolve`, span `[518,523]` (the `Io(e)`
  handler pattern), agreeing with the pin.
- **b (wolfgang)**: `fail(E0806)@typecheck`, the *generic*
  refutable-binding diagnostic ("this pattern can fail to match, but a
  binding cannot"), same span `[518,523]`; its `--phase=resolve` record
  is `pass`. `[proto.cmp.rung]` cannot bridge different *codes*, so the
  records genuinely diverge.
- Triage (`[proto.cmp.triage]`): the clause is not ambiguous. The
  corpus file wolfgang itself ships pins E0809 and names the rule; the
  else-handler position takes `closed_pattern` by grammar
  (`[gram.pat]`), so a refutability complaint about the handler pattern
  is the wrong diagnostic there twice over. **The counterparty is the
  defendant**: its E0809 emission (s71's #43/#59 work) evidently does
  not reach the ladder its `conform-run` door runs. The generic E0806
  refutability check fires first at its typecheck rung. Filed
  upstream as **wolf-lang#61**, both records attached verbatim; the
  entry closes when wolfgang's conform-run answers its own pin.

Eleventh corpus differential: lupin 0.1.7, pin `e94b879` (wave six: s40
os/time/json, s70 match tier + X3 value paths, s69 idiom lints; 232
entries compared, 22 members through their entries, counterparty built
CLEAN at `e94b879`), **0 divergences**. The ruling the last four rounds
asked for landed in spec/06 as **`[proto.cmp.rung]`**: when both records
reject with `fail(CODE)` and the first diagnostic's code and span agree,
the records AGREE even when `phase_reached` names different rungs of the
shared ladder; exactly one verdict wide. `compare_deep` implements the
clause, the eleven rung-placement divergences compare clean, and
**DIV-2026-011, -012, -014 and -015 ALL CLOSE** with the clause cited.
`FILED_DIVERGENCES` is empty for the first time since the fourth round.
One new shape surfaced and resolved comparator-side, no filing needed:
wolfc's `resolve/cycle/main.lu` record interleaves its new W0314 lint
ahead of the E0303 rejection in `diagnostics` (source order, licensed by
`[proto.record.warn]`'s "warning observations ride `diagnostics` at
warning severity"), so the fail comparison now reads the first
**error**-severity diagnostic. A lint's span is `[proto.cmp.warn]`'s
surface, never the rejection's. 382 conservatism-ledger entries (63
rejects-beyond by the counterparty, 122 run-unmatched, 147
counterparty-unsupported, 50 interp-unsupported: the fs/net/proc-spawn
tiers, comptime, and the compiler-only analyses).

---

Tenth corpus differential: lupin 0.1.6, pin `13b811f` (wave four: the
#41 capture law, s34 procs, s35 io reactor, s39/s40/#40 native
str/List/fs, the s68 lint corpus; 203 entries compared, 18 members
through their entries, counterparty built CLEAN at `13b811f`), **11
divergences, all filed, none a soundness candidate**, and every one is
the same finding: same code, same span, this machine at `resolve` where
wolfc's emission lives at `typecheck`/`mem`. DIV-2026-011 holds;
DIV-2026-012 holds; **DIV-2026-013 CLOSES** (wolfc's conform-run no
longer misrejects its own s38 fs/io files: `unsupported@wir` there now,
never a divergence); **DIV-2026-014's** wiring half closes the same way
(wolfc emits E0411/E0412/E0413 at `typecheck` now; this machine
realigned its E0412/E0413 spans to the counterparty's `:spec` shape) and
its residue rides DIV-2026-011; issue #19's realignment opens
**DIV-2026-015** (E1101/E1102/E1103/E0004 statically at this machine's
resolve rung, byte-identical codes and spans, the fourth family of the
one rung question). The `warnings` arrays agree wherever both sides
carry them (`store_buffer`'s W1101×4 + W1102 set is byte-identical).
325 conservatism-ledger entries (61 rejects-beyond by the counterparty,
103 run-unmatched, 120 counterparty-unsupported, 41 interp-unsupported:
the fs tier, sockets, procs-adjacent comptime, and the compiler-only
analyses).

### DIV-2026-015 — the E11xx capture law + E0004 — **RESOLVED by `[proto.cmp.rung]`, pin `e94b879` (0.1.7)**

Resolution: the `[proto.cmp.rung]` ruling (spec/06, s70). Fail parity
(code + span) at any shared-ladder rung is agreement, one verdict wide.
All four files compare clean at the eleventh round. The original filing:

Filed 2026-08-11 (lupin 0.1.6, CLEAN wolfc build at `13b811f`). Four
files, one class: verdict (rung placement only), codes and spans
byte-identical.

- `conc/store_buffer.lu`: both fail(E1101), span `[438,439]` (the
  first captured write, `x`); a at `resolve`, b at `typecheck`. The
  warning sets also agree: W1101 at `[438,439]`, `[445,447]`,
  `[511,512]`, `[518,520]`, W1102 at `[511,517]`.
- `conc/chan_unsendable.lu`: both fail(E1102), span `[275,284]` (the
  `List[int]` payload); a at `resolve`, b at `typecheck`.
- `conc/when_nested.lu`: both fail(E1103), span `[642,755]` (the whole
  inner `when`); a at `resolve`, b at `typecheck`.
- `grammar/intdot_exponent.lu`: both fail(E0004), span `[291,295]`
  (`1.e5`); a at `resolve`, b at `typecheck`. Issue #19's correction:
  the code stays an error (`int` has no member `e5`), and producing it
  here closed the last E000x unsupported(interp) conservatism row.

Triage: same as DIV-2026-011/-012/-014. The spec is silent on
same-code-same-span rejections across implementations of unequal
pipeline depth; one `[proto.cmp]` ruling closes all four families.

---

Ninth corpus differential: lupin 0.1.5, pin `f0da6e6` (the five-lane
fan-out: s32 tasks, s33 channels, s37 str core, s38 fmt/io/fs, s67
warnings; 181 entries compared, 18 members through their entries,
counterparty built CLEAN at `f0da6e6`), **10 divergences, all filed,
none a soundness candidate**: DIV-2026-011 holds; issue #18's tier
statics open **DIV-2026-012** (four files, the DIV-2026-011 rung
question again, with the same code and span, this machine at resolve
where wolfc's emissions live at mem/typecheck); and the pin exposes a
counterparty *surface* lag filed as **DIV-2026-013**/**DIV-2026-014**:
wolfc's conform-run at `f0da6e6` rejects or declines six of its own new
corpus files (the s38 fs/io builtins E0301-unresolved; the strings
statics reported `unsupported`) while its own corpus and checked-lane
tests pin them. The conform-run wiring landed upstream after this pin.
289 conservatism-ledger entries (60 rejects-beyond by the counterparty,
89 run-unmatched, 103 counterparty-unsupported, 37 interp-unsupported).

### DIV-2026-012 — the 0.1.5 tier statics — **RESOLVED by `[proto.cmp.rung]`, pin `e94b879` (0.1.7)**

Resolution: the `[proto.cmp.rung]` ruling (spec/06, s70), the same
closure as DIV-2026-011, which this filing rode. The original filing:

Filed 2026-08-11 (lupin 0.1.5, CLEAN wolfc build at `f0da6e6`). Four
files, one class: verdict (rung placement only), codes and spans
byte-identical where both sides emit.

- `memory/unsafe_raw_outside.lu`: both fail(E1301), span `[384,395]`
  (the `c.malloc(8)` call); a at `resolve`, b at `mem`.
- `memory/unsafe_sig.lu`: both fail(E1302), span `[329,330]` (the
  parameter `p`); a at `resolve`, b at `mem`.
- `typecheck/cast_bad.lu`: both fail(E0805); a at `resolve`, b at
  `typecheck`.
- (`memory/mode_missing_mut.lu` remains DIV-2026-011, the original
  filing of the question.)

Triage: the spec is the defendant first, and it is *silent*, since
spec/06 compares `phase_reached` without ruling on same-code-same-span
rejections across implementations of unequal pipeline depth. Routed
upstream with DIV-2026-011; whatever `[proto.cmp]` ruling closes that
filing closes this one. Sema-lite is this machine's only static tier
(issue #18: the unsafe ring, its signature boundary, and the cast
matrix's bool column now reject at resolve with the counterparty's
codes and spans, observed at this pin).

### DIV-2026-013 — the s38 fs/io files — **RESOLVED upstream, pin `13b811f` (0.1.6)**

Resolution: exactly the predicted closure. wolfc's conform-run at
`13b811f` no longer E0301-rejects its own s38 files. It reports
`unsupported@wir` on `fs/error_row.lu`, `fs/roundtrip.lu` and
`io/eprint.lu`, which `[proto.cmp.defined-divergence]` makes a ledger
row, never a divergence. The three FILED_DIVERGENCES entries retired
with this note. The original filing:

`fs/error_row.lu`, `fs/roundtrip.lu`, `io/eprint.lu`. wolfc's
conform-run at the pin answers `fail(E0301)` (`fs_read_text`,
`fs_write_text`, `eprint` "not in scope") on files its own corpus pins
at `mem`/`run` and its own checked-lane tests execute. The spec is
clear (`[conf.directive.phase]`: the directive is the truthful ledger)
and the corpus is upstream's own contract, so the *conform-run surface*
is the defendant: the s38 builtins exist on the checked lane but the
conform-run wiring landed after `f0da6e6`. This machine runs
`io/eprint.lu` to the pinned stdout (stderr is the human channel,
never hashed) and declines the fs tier honestly (no filesystem by
design, `[proto.record.unsupported]`). Expected to resolve at the
next pin bump; if it does not, the filing escalates to a wolf-lang
issue.

### DIV-2026-014 — the strings statics — **RESOLVED by `[proto.cmp.rung]`, pin `e94b879` (0.1.7)**

Resolution: the `[proto.cmp.rung]` ruling (spec/06, s70). The
rung-placement residue was all that remained after the `13b811f` wiring
closure, and the ruling absorbs it. The original filing:

`strings/char_index_fail.lu` (pins fail(E0411)),
`strings/format_spec_malformed.lu` (fail(E0412)),
`strings/format_spec_mismatch.lu` (fail(E0413)). As filed (0.1.5): this
machine rejects all three with the pinned codes at its resolve rung;
wolfc's conform-run at `f0da6e6` reported `unsupported`, the emissions
not being reachable through its conform-run surface. **Status update,
pin `13b811f` (0.1.6):** the wiring half closed as predicted, and wolfc
emits all three pinned codes at its `typecheck` rung now. Spans:
E0411 agreed already (`[420,424]`); for E0412/E0413 this machine
realigned to the counterparty's `:spec` shape (`[530,534]` = `:>08`,
`[414,417]` = `:.2`; 0.1.5 spanned the whole hole). What remains is
rung placement only, the DIV-2026-011 question, resolved by the same
future `[proto.cmp]` ruling.

---

Eighth corpus differential: lupin 0.1.4, pin `ad6cef7` (s29+s30: real
glibc behind the unsafe tier, the erring-main pin, `fcmp.ne` as IEEE
unordered, module-path-qualified WIR names; 165 entries compared, 18
members through their entries, counterparty built CLEAN at `ad6cef7`),
**1 divergence, filed as DIV-2026-011**, and **DIV-2026-010 CLOSES**:
s29 moved wolfc's E0410 emission to the resolve rung (with the
`[conc.when.body]` exemption wolf-lang#21 carried from this machine) and
re-pinned the two corpus `phase:` directives resolve → parse, so
`typecheck/let_reassign.lu` and `typecheck/let_compound_assign.lu` now
reject on both sides at `resolve`, same code, same span. The eighth
round compares them clean, exactly the closure condition the seventh
round wrote down. The new filing is the same *shape* in the opposite
direction: `memory/mode_missing_mut.lu` rejects with **E1007 at the same
span on both sides** ([405,408], the argument), this machine at
`resolve` (issue #15's fix; sema-lite is its only static tier and the
signature is visible there), wolfc at `mem` (where mode checking lives
in its pipeline). 268 conservatism-ledger entries (63 rejects-beyond by
the counterparty, 79 run-unmatched, 90 counterparty-unsupported, 36
interp-unsupported). The run-unmatched rows are the wave's new run-rung
witnesses landing ahead of the counterparty's run tier.

### DIV-2026-011 — `memory/mode_missing_mut.lu` — **RESOLVED by `[proto.cmp.rung]`, pin `e94b879` (0.1.7)**

Resolution: exactly the first branch the triage asked for, a
comparison-rule clause. `[proto.cmp.rung]` (spec/06, s70): same code +
same first-diagnostic span across `fail` records is agreement, the rung
recorded, never compared; one verdict wide, so `fail` against any other
verdict still diverges. The eleventh round compares this file clean at
`E1007`/`[405,408]`, resolve here, mem there. The original filing:

Filed 2026-08-10 (lupin 0.1.4, CLEAN wolfc build at `ad6cef7`).

- Class: verdict (rung placement only). Codes and spans byte-identical
  (`E1007` at `[405,408]`, the argument expression).
- a (lupin): `fail(E1007)@resolve`, issue #15's fix. The X1 call-site
  mode law's disagreement ran to a silently wrong answer here;
  `[conf.trap.map]` gives E1007 no dynamic meaning, so the honest stop
  is the rung where the callee's signature is visible, which for this
  machine is sema-lite at `resolve` (the E0410 precedent).
- b (wolfc ad6cef7): `fail(E1007)@mem`. Mode checking is its memory
  tier's, after typecheck completes.
- Triage: **spec first defendant.** `[proto.cmp]` has no allowance for
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
the pin), **2 divergences, both still DIV-2026-010**, unchanged from
the sixth round: same E0410, same spans, wolfc's record says `typecheck`
where the corpus pins `phase: resolve`. **Re-verified at d147a54: the
fix has NOT landed at this pin.** It is in flight upstream. The s29 work
moving the emission into a resolve-rung `letcheck` (and re-pinning the
corpus `phase:` directives resolve → parse) landed on trunk after this
pin (`626175b`/`6bfff9a`, CI still running at the close of this pass).
This machine's heads-up that the new walker must exempt `when`-body
assignments per `[conc.when.body]` (`when (a, b) { a += 10 }` on
`let`-bound Mutex operands; `conc/when_multi.lu` and `procs.lu` pin
`run(exit=0)`) is filed as wolf-lang#21. The entry closes when s29 lands
and the eighth round compares clean. 261 conservatism-ledger entries (64
rejects-beyond by the counterparty, 75 run-unmatched, 86
counterparty-unsupported, 36 interp-unsupported, down from 46:
impl-method dispatch, postfix rows, numeric casts and the iterator
protocol moved ten files onto this machine's run rung).

Sixth corpus differential: lupin 0.1.2, pin `a0c4564` (the E0410
fail-files and the unsafe/checked memory tier land; 159 entries compared,
16 members through their entries), **2 divergences, both filed as
DIV-2026-010**, the first non-zero round since is07, and both are one
finding: `typecheck/let_reassign.lu` and `typecheck/let_compound_assign.lu`
reject on both sides with the **same E0410 at the same span**, but the
counterparty's record places the rejection at `typecheck` while the corpus
files themselves pin `phase: resolve`. The deep comparison reads wolfc's
record as claiming the resolve rung *completed*, which collides with this
machine's honest rejection at the rung it performs. That is a
rung-placement inconsistency between the compiler's record and its own
corpus directive, not a verdict disagreement. **Triage: spec/corpus
first defendant.** Either the corpus directives should say `typecheck`
or wolfc's driver should report sema's E0410 at `resolve`; routed
upstream with the filing; this machine's placement follows the corpus.
261 conservatism-ledger entries (64 rejects-beyond by the counterparty,
where the E1301/E1302 unsafe tier landed, 67 run-unmatched, 84
counterparty-unsupported, 46 interp-unsupported).

Fifth corpus differential: is09, pin `cbde620` (s21's shared tier: nine
files advance to `mem`, `prov_holy_grail.lu` to `typecheck`; spec-extract
renders the §3.2 operator climb into `grammar.ebnf`), 148 entries
compared, **0 divergences**, the fourth consecutive zero round. 246
conservatism-ledger entries, composition unchanged from the fourth round
(59 rejects-beyond by the counterparty, 64 run-unmatched pre-M1, 80
counterparty-unsupported, 43 interp-unsupported): the pin moved only
`phase:` directives and spec text, and neither side's accept set moved
with it. The newly explicit operator-climb EBNF was diffed against this
repo's is01 §3.2 transcription (`parse::PRECEDENCE`,
`parse::PREFIX_OPERATORS`): tier-for-tier, operator-for-operator,
associativity-for-associativity identical. No finding, and the check is
now mechanical
(`tests/spec_extract.rs::the_emitted_operator_climb_matches_our_transcription`).
`differ::FILED_DIVERGENCES` remains **empty**.

Fourth corpus differential: is08, pin `843174f`, 148 entries compared,
**0 divergences**, 246 conservatism-ledger entries (59 rejects-beyond by
the counterparty, where the E1005/E1011/E1012 region-checker litmuses
landed, 64 run-unmatched pre-M1, 80 counterparty-unsupported, 43
interp-unsupported, down from 45: `procs.lu` and
`conc/proc_kill_defers.lu` are self-contained now and RUN, S-5
resolved). `differ::FILED_DIVERGENCES`
remains **empty**.

Third corpus differential: is07, pin `79ceec6`, 142 entries compared,
**0 divergences** (down from 1), 238 conservatism-ledger entries (55
rejects-beyond by the counterparty, up from 46 as the E1004/E1007/E1010
litmuses landed, 60 run-unmatched pre-M1, 78 counterparty-unsupported,
45 interp-unsupported). `differ::FILED_DIVERGENCES` is **empty** for the
first time since the differential lane exists.

The is07 exploration record (the corpus half lives in
`tests/explore_corpus.rs::CONC_LEDGER`): every `conc/` litmus explored to a
**closed frontier** under DPOR (8 files, 1–2 Mazurkiewicz classes each,
naive-DFS baseline agreeing on every conclusion), with one verdict per
file across its entire schedule space. The determinism-taxonomy claim
(spec/03 §5 `sched-ev/0`, `[proto.seed.equal]`) holds over the whole
pinned conc tier: **no corpus file is schedule-dependent**. The multiopen
model check (the question `memory/region_multiopen_ok.lu`'s own header
flags for is07) answers definitively within bounds: no explored schedule
breaks the region forest invariant or leaks, whether it comes from the
corpus files or from the concurrent multiopen litmuses in
`tests/explore_machine.rs`.

## Spec findings from is06/is07 (spec-is-defendant — filed, not absorbed)

spec/03 had never been executed before is06. The machine was the first
executable test of it, and the harvest was routed upstream, not patched
around. **The s20 S-batch (pin `843174f`) paid S-1 through S-8.** The
eight entries now live under *Resolved findings* below with what the spec
adopted and where this machine realigned. S-9 and S-10 remain open;
S-11 was RESOLVED by ruling D40 (2026-08-12), and its entry carries the
status update below.

- **S-9 (is07): the seed↔schedule encoding has no normative home.**
  `[conc.det.seed]` defines `--replay=SEED` behaviorally and `[proto.seed]`
  makes equal seeds byte-comparable, but no pinned document says what a
  seed *is* beyond "a value that regenerates the stream". The accepted
  s36 Phase A hook-design doc is the designated owner and **does not exist
  at pin `843174f` either** (re-checked at the is08 pin bump; the S-batch
  is spec/03+05 only). is07's provisional split of the `u64` namespace
  (bit 62 tags a packed schedule, `sched::PACKED_SEED_TAG`,
  approximation-contract §10.6) stands until the Phase A doc lands; the
  compiler runtime's format has priority and this side re-pins to it.

- **S-10 (lupin 0.1.1, wolf-interp#4): `[conc.task.spawn]`'s dynamic half
  is unstated, and a task closure's write to a captured copy is silently
  task-local.** The clause makes capturing a `mut` borrow of enclosing
  state a *compile error* (E1101), and `[conf.trap.map]` states no dynamic
  meaning for E1101 (its table is E1001/E1002/E1004/E1005; the E1004/E1005
  precedent is exactly how such a meaning gets added). This machine
  captures by value (`[gram.expr.closure]`; approximation-contract §10.2),
  so the E1101 shape *runs*, each task writes its own copy, and the
  cross-task write is lost without a fault. The result is a wrong-looking
  answer, not a trap. The corpus and the book agree with the machine today:
  `conc/store_buffer.lu` is the pinned exemplar (exit 0, task-local
  effects, the standing conservatism class), and wolf-book ch13/appendix
  exercises document "exit 0 — captures by value" with the static E1101
  rejection pending on the compiler side. The s20 S-batch did not speak to
  it, and DIV-2026-008 (`freeze_publish`) was this family's first costume.
  **Not fixed here, deliberately**: a spawn-time capture-analysis trap
  would be this implementation legislating a dynamic meaning the spec
  never states. That is the line `ledger::dynamic_meaning` refuses to cross.
  Routed upstream: spec/03 (or `[conf.trap.map]`) should either state
  E1101's runtime meaning (kind + clause, as the E1004/E1005 amendment
  did) or bless capture-by-copy as the defined interpreter-tier semantics.
  Until then §10.2 stands as the documented behavior.
  **Status update, pin `13b811f` (0.1.6, issue #19):** the #41 capture-law
  wave hardened the *static* half: `conc/store_buffer.lu` re-pinned to
  `fail(E1101)` and this machine now rejects it at resolve with the
  counterparty's code and span (DIV-2026-015), so the E1101 shape no
  longer runs here and the silent-write-loss face is unreachable through
  the pinned corpus. The clause still states no runtime meaning, so the
  dynamic question stays open exactly as filed; capture-by-value remains
  §10.2's documented semantics for the shapes the static walk cannot see.

- **S-11 (lupin 0.1.2, wolf-interp#9 / wolf-std F-0014 / wolf-lang#15):
  container mutation during `for` iteration has no governing clause.**
  `loop_expr ::= 'for' pattern 'in' expr block` is the whole of the pinned
  text on `for` (`[gram.expr.flow]`): nothing states whether the loop
  moves its operand, holds a `mut`-grade access on it for the loop's
  extent, or reads it once. The three candidate readings produce three
  different verdicts on `for x in xs { xs.push(x) }`, and both
  implementations picked one: **wolfc** (a0c4564) lowers the operand as a
  *move* and rejects the body's use statically (`fail(E1001)`, "`xs`
  moved here", with a `for x in copy xs` fix-it), even though
  `[mem.tier0.move.1]`'s move list (assignment, initialization, `take`
  arguments, `return`) does not include loop operands; **lupin** evaluates
  the operand once at loop entry and iterates that snapshot (the MVS
  copy reading), so the program runs `exit(0)`, the pushes land, and the
  iteration never observes them (approximation-contract §6.8). A third
  reading, in which the loop holds the container `mut`-style for its
  extent, would make the body's push a `trap(exclusivity)` under
  `[conf.trap.map]`, and no clause states that hold either.
  **Not legislated here, deliberately**: the snapshot loop cannot produce
  a spurious fault (the one direction the approximation contract
  forbids), and inventing a move or a hold the spec never names is the
  compiler-alignment shortcut `ledger::dynamic_meaning` exists to refuse.
  Routed upstream: spec/01 (or spec/02 §2) should state the operand
  semantics of `for`, whether move (blessing E1001 and its dynamic
  `use-after-move` half), extent-hold (naming the `exclusivity` trap), or
  loop-entry copy (blessing this machine and making wolfc's E1001 a
  conservative extension). wolf-std keeps the divergence visible in CI:
  `tests/list/mutate_while_iterating.lu`, ledgered `lupin = run` /
  `wolfc = fail(E1001)`. Compiler half: wolf-lang#15.
  **RESOLVED by ruling D40 (2026-08-12; lupin 0.1.8).** The designers
  picked the third reading: `for x in xs` holds a **read claim** on the
  container for the loop's extent; a mut use inside is a static
  exclusivity-family error in wolfgang (new code E1013, fix-it teaching
  collect-then-apply or the index loop, deliberately never the
  accidental E1001-reads-as-moves story) and a dynamic
  `trap(exclusivity)` here, per `[conf.trap.map]` (which gains the E1013
  row). One rule, two enforcement modes; `[proto.cmp.rung]` makes the
  static/dynamic pair an agreement. This machine implements the claim at
  0.1.8 (`eval_for` holds `Access::Shared` on the iterated container's
  place; the conflict trap names the loop and the fix). The spec text
  itself lands with wolf-lang s72; this machine implemented it ahead of
  the pin on the ruling's authority, the noted drift of the 0.1.8
  release-pairing pass. approximation-contract §6.8 rewritten to the
  ruled semantics.
  wolf-interp#9 closes as fixed; compiler half remains wolf-lang#15/s72.

## Resolved findings

### DIV-2026-010 — `typecheck/let_reassign.lu` + `typecheck/let_compound_assign.lu` — **resolved upstream, pin `ad6cef7` (s29)**

Closed 2026-08-10 at the 0.1.4 re-pin, by exactly the closure condition
the filing wrote down: s29's `letcheck` moves wolfc's E0410 emission to
the **resolve** rung (carrying the `[conc.when.body]` exemption this
machine flagged as wolf-lang#21, where `when (a, b) { a += 10 }` over
`let`-bound Mutex operands stays legal), and the corpus re-pins both
files' `phase:` directives resolve → parse with the rationale in the
files themselves. Eighth round: both sides `fail(E0410)@resolve`, same
span, 0 divergences on these files. The original filing (0.1.2, pin
`a0c4564`): class verdict, rung placement only, with codes and spans
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
  deviation: dynamically reaching an acquisition of an already-held
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
  re-raises at the scope exit, after the join, replacing the is06
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
  nonzero; 1 here, class compared, never the number).
- **S-8 (closed-channel `select` readiness)** → `[conc.select.closed]`
  adopts the machine's Go-posture answer verbatim; the rule cites it.

### DIV-2026-007 — `grammar/receiver_moded.lu` — **resolved upstream, pin `79ceec6`**

Compiler suspected, confirmed and fixed: the compiler's E0210 primary span
now covers the whole parenthesized moded receiver, exactly as the pin
`67c977f` spec amendment demanded and as this interpreter has reported
since is05. Verified by the third corpus differential (0 divergences).
The DIV-2026-006 → 007 chain is closed end to end: spec amendment first,
then the lagging implementation.

### DIV-2026-008 — `conc/freeze_publish.lu` — **resolved upstream, pin `79ceec6`**

Corpus/spec suspected, confirmed: the wolf-lang ruling kept
`[conc.task.spawn]`'s capture rule intact and repaired the *file*. It now
reports through a channel (`ch.send(table[3])` / `ch.recv()`) instead of
writing captured mutable locals, exactly the conforming spelling this
machine's `freeze_then_share_reads_from_any_task` litmus pinned. Runs
`exit(0)` here, matching the corpus; is07's explorer proves the exit
stable across its whole schedule space.

### DIV-2026-009 — `conc/when_multi.lu` — **resolved upstream, pin `79ceec6`**

Corpus suspected, confirmed: the expected total is 223 now, the
arithmetic this log recorded. Runs `exit(0)`, schedule-independent under
exploration. (`when` gained its spec/03 clauses at pin `843174f`; S-1 is
resolved above.)

### DIV-2026-001 — `typecheck/match_exhaustive.lu` — **resolved upstream, pin `67c977f`**

Compiler suspected, confirmed: the parser accepted comma-less variant
payloads outside the published grammar (and the formatter stripped the
commas the grammar requires, which is the printer bug the leniency
masked). The
upstream fix landed both the parser rejection and the corrected corpus
file `Rgb(int, int, int)`. Both implementations now parse the file and it
**runs** here (`exit(0)`, in the run ledger). Closed by the pin bump.

### DIV-2026-002 — `resolve/cycle/main.lu` — **resolved here, is06**

Interp suspected, confirmed. `sema::resolve_check` now enforces
`[mod.cycle]` (D32): the module-use graph is walked depth-first and the
back-edge that closes a cycle fails `E0303` at the closing `use` decl,
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
prelude names (`use std.fs`) are exempt: they resolve no directory, and no
module law this rung owns speaks about them.

### DIV-2026-006 — `grammar/receiver_moded.lu` — **resolved in the spec, pin `67c977f`**

Spec suspected, confirmed: §3.3 now pins "primary span = the entire
parenthesized moded receiver", the reading this repo proposed and already
implemented. The interpreter conforms as-is; the compiler does not yet,
and that residue is DIV-2026-007 above (compiler suspected).

## Fuzz campaign record

| date | seed | count | mode | counterparty | findings | ledger |
|------|------|-------|------|--------------|----------|--------|
| 2026-08-09 | 5381 (0x1505) | 10000 | mixed (5000 defined / 5000 boundary) | wolfc @ 8b04edf (debug) | **0** | 20000 (exactly 2/case: counterparty-unsupported@typecheck + run-unmatched) |

The first campaign's ledger composition is itself a result: 2 entries per
case with **zero** rejects-beyond means every one of the 10,000 generated
programs (boundary mode's regions, moves and `mut` call sites included)
cleared the compiler's full frontend (lex, parse, resolve, s17's completed
sema) *and* this machine's run tier, with the two frontends in exact
agreement on all of them. The generator's semantic-plausibility layer is
doing its job; the next campaign should turn the boundary dial harder
(deeper nesting, adversarial-but-legal spellings) because this one found
nothing. Replay: `lupin fuzz --count 10000 --seed 5381 --compiler
upstream/target/debug/wolf`.

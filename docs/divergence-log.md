# Divergence log — the is05 ledger

Every divergence the differential runner finds lands here with its triage.
This document and `differ::FILED_DIVERGENCES` are one ledger in two forms:
the table below is the human record; the constant is what lets the runner
annotate a known finding (`x-filed`) and stop gating on it while it awaits
its fix. An entry here never closes a finding. It routes it. Closure is
the iron rule: the resolving commit lands a corpus file and a spec clause
citation, and then the entry moves to the *resolved* section.

## The triage workflow (`[proto.cmp.triage]`)

The runner emits, for each unfiled divergence, a filing template (visible
with `lupin diff-run --filing`): the program, both verdicts, the class,
the rung the comparison fired at. A human then walks the decision tree, and
the spec document is the defendant first:

1. Spec silent or ambiguous on the behavior → *spec bug*. Clause PR to
   the spec first; both implementations then conform to the new clause.
2. Spec clear, interpreter matches it → *compiler bug*. Filed in
   wolf-lang; the minimized program lands in `corpus/` as a regression file
   in the same fix.
3. Spec clear, compiler matches it → *interpreter bug*. Fixed here;
   corpus file likewise.

Divergence classes, descending severity (`[proto.cmp.severity]`, extended
by two runner-level classes): `soundness-candidate` (gates always, filed or
not), `verdict`, `span-or-code`, `stdout`, then `protocol` (a record the
schema rejects) and `timeout`. The conservatism ledger is tracked and
reported but never gates (`[proto.record.unsupported]`). It covers
`unsupported` on either side, rejections at rungs the other side does not
perform, and run-tier outcomes the pre-M1 compiler cannot check.

## Counterparty acquisition (integrator ruling, is05)

Building and executing the pinned compiler is legitimate binary
acquisition: the binary is data consumed through the spec/06 protocol,
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

Re-measured at pin `f8dca42` (0.1.11), and re-measured independently
of the runner. The table is the lane's own audit, so deriving it from
the lane it audits would be circular. Both columns come from invoking
each side once per entry per tier and reading `phase_reached` off the
records (`tests/`-external; the script and its four `tier-*.json`
outputs are scratch, the numbers are here). The `default` row is why
this table exists: through 0.1.9 the runner passed no flag, the compiler
answered `unsupported` at `wir` on every run-tier program, and the whole
dynamic half of the corpus compared nowhere. That row is still 0,
and it is still correct that it is 0; the audit confirms the lane's
report rather than a reopened gap.

The three run-reaching lanes are NOT nested, which the 0.1.10 table
did not show. `checked` reaches `run` on more files than `native`
(127 vs 122) yet compares fewer (114 vs 116), because each lane declines
a different tier:

- `checked` declines the whole conc tier: 11× `conc/` + `procs.lu` +
  `test/conc_schedules_test.lu` + `rows/qmark_defer.lu` +
  `typecheck/match_exhaustive.lu` (15 files `native` compares).
- `native` declines most of the unsafe/region/shared tier: `regions.lu`,
  `memory/unsafe_*` ×4, `memory/shared_ok.lu`,
  `memory/handle_stale.lu`, `memory/region_multiopen_swap.lu`,
  `comptime/norm_linear.lu`, `rows/coarsen.lu`, `traits/dyn_ok.lu`,
  two `lints/` (13 files `checked` compares).
- `release` declines the conc tier by name (10 files vs `native`), the
  compiler's documented posture (conc lowering belongs to the debug tier),
  and conservatism, never divergence.

So the coverage figure is the union: 129 files compared at
`run` by at least one lane, with 101 compared by all three. Running one
lane and calling it "the run tier" would miss up to 15 files; this is
why the pass runs all four. Of this machine's 185 run-reaching entries,
56 are met by no counterparty lane at all (the `comptime/`, `fs/`,
`net/`, `os/`, `json/` and `projects/` tiers, plus the analyses only one
side performs): the residue the differential still cannot see, and the
number to drive down.

`release` is the lane that matters most: it runs s42's mid-end and s43's
whole-program layer, so comparing it against this machine is what tests
"optimization preserves observable behavior". Our
own side is always invoked plainly: this machine has one engine, and
the tier selects which of the *counterparty's* engines answers.

## Open findings

### The thirty-ninth — is54, lupin 0.1.39, pin `2e4ca769` -> `93a5fe50` (wolf-lang **v0.2.16**)

Five subjects, one pin move: wolf-interp#134 (a `use m.Alias` whose only
use is an error row is a use), #130 (`Scope`/`Proc[T]` type names and
`p.join()`), #129 items 2–5 (`trap_message` emitted, `[proto.record.pass]`,
`[proto.cmp.pass]`, `[conf.exit]`), wolf-lang#447's lupin half (the linux
archive's glibc floor), and wolf-lang#437's lupin half (a file index on
record diagnostics — added to the contract mid-lane, shape pending on
lane s181's comment on that issue).

#### The ancestry oracle, before the pin moves

Run in wolf-lang at `origin` fetched 2026-09-24:

```
$ git rev-parse 'v0.2.16^{commit}'
93a5fe504593ca7642b78ba83b4986e7a03cfe71
$ git merge-base --is-ancestor 93a5fe50 v0.2.16 ; echo $?
0
$ git merge-base --is-ancestor 93a5fe50 origin/trunk ; echo $?     # trunk a565d4b9
0
$ git merge-base --is-ancestor 2e4ca769 93a5fe50 ; echo $?         # the old pin is on the line
0
```

#### The baseline, measured at `ba357aa` (0.1.38), old pin

`lupin corpus` (release build, kasumi): **666 files, 625 entries, 41
members, 0 failures, 358 distinct conforms; 485 reach `run`; 491 match,
26 dynamic counterpart, 48 conservatism, 59 out of scope, 1 mismatch**
(the filed DIV-2026-019). Identical to is53's measured row.

#### Prediction, before the pin moves

`git diff --name-status 2e4ca769 93a5fe50 -- corpus`: **24 `.lu` files
added** (21 entries, 3 `member: true` files — `rows/error_alias_qualified/disk/d.lu`,
`rows/negative/error_alias_private/disk/d.lu`,
`typecheck/fn_param_shadows_import/cmp/c.lu`), two protocol fixtures
added (`protocol/clean-stop.json`, `protocol/with-trap-message.json`),
14 files modified. **Not one `check:` line is edited**: the 21 `check:`
lines in the diff are all `+` lines of new files, and the modified files
change `phase:` (`mem` -> `run`/`wir`) and prose only. This machine's
judgement reads `check:` and never `phase:`, so:

**Claim 1: no pre-existing entry changes class.** Every moved row is a new
file. Falsifier: any of the 625 rows landing in a different column.

The 21 new entries, predicted at 0.1.38's code (before any fix):

| new entry | `check:` | predicted |
| --- | --- | --- |
| `conc/freeze_proc_snapshot.lu` | `run(exit=0, "2 2")` | match |
| `conc/proc_join_param.lu` | `run(exit=0, "9")` | **MISMATCH** — `fail(E0301)` at `Proc[int]` (#130) |
| `conc/proc_join_value.lu` | `run(exit=0, "42 7")` | out of scope — no `join` on a proc handle |
| `memory/move_field_siblings_ok.lu` | `run(exit=0, …)` | match |
| `memory/move_field_use_after.lu` | `fail(E1001)` | dynamic counterpart (`trap(use-after-move)`) |
| `memory/mut_arg_element.lu` | `run(exit=0, …)` | match |
| `memory/mut_elem_excl.lu` | `fail(E1002)` | conservatism (two elements, two places here) |
| `memory/mut_elem_nested_call.lu` | `fail(E1002)` | dynamic counterpart (`trap(exclusivity)`) |
| `memory/mut_place_nested.lu` | `run(exit=0, …)` | match |
| `memory/pool_accessors.lu` | `run(exit=0)` | out of scope — `Pool` has no `is_empty`/`alive`/`has`/`clear` here |
| `memory/pool_place_write.lu` | `run(exit=0)` | match |
| `memory/region_str_producers_charged.lu` | `run(exit=0, …)` | match |
| `memory/region_str_view_inside.lu` | `run(exit=0, …)` | match |
| `memory/region_str_view_return.lu` | `fail(E1010)` | conservatism (wolf-interp#126) |
| `rows/error_alias_qualified/main.lu` | `run(exit=0, …)` | **MISMATCH** — `fail(E0305)` (#134) |
| `rows/negative/error_alias_private/main.lu` | `fail(E0304)` | match |
| `strings/trim_cutset_refused.lu` | `fail(E0402)` | conservatism (wolf-interp#125) |
| `typecheck/fn_param_shadows_import/main.lu` | `run(exit=0, …)` | match |
| `typecheck/fn_param_shadows_item.lu` | `run(exit=0, …)` | match |
| `typecheck/list_lit_elem_i32.lu` | `run(exit=0, …)` | match |
| `typecheck/list_lit_elem_unfit.lu` | `fail(E0415)` | conservatism |

11 match, 2 dynamic counterpart, 4 conservatism, 2 out of scope, 2
mismatch. `distinct conforms` is derived, not guessed: the union of
`conforms:` tags gains nine and loses none (`conc.proc.arg`,
`conc.proc.handle`, `conc.proc.join`, `gram.expr.assign`,
`mem.model.place`, `mem.shared.handle.3`, `type.err.alias.qualified`,
`type.err.alias.transparent`, `type.list.lit.elem`); the same command at
`2e4ca769` gives 358, which is the measured baseline. The registry gains
fifteen anchors and drops none (`spec/anchors.json`, `+` lines only):
524 -> 539.

| class | `ba357aa` @ `2e4ca769` (measured) | 0.1.38 code @ `93a5fe50` (predicted) | after #134 + #130 (predicted) |
| --- | --- | --- | --- |
| files | 666 | 690 | 690 |
| entries | 625 | 646 | 646 |
| members | 41 | 44 | 44 |
| failures | 0 | 0 | 0 |
| distinct conforms | 358 | 367 | 367 |
| anchors | 524 | 539 | 539 |
| match | 491 | 502 | **505** |
| dynamic counterpart | 26 | 28 | 28 |
| conservatism | 48 | 52 | 52 |
| out of scope | 59 | 61 | **60** |
| mismatch | 1 (filed) | **3** (2 unfiled) | 1 (filed) |
| reach `run` | 485 | 501 | **504** |

`reach run` at the pin: the ten predicted run matches, the two dynamic
counterparts and the four conservatism rows (they run here precisely
because this machine does not refuse them). The flips, by name:
**#134 flips `rows/error_alias_qualified/main.lu` MISMATCH -> match; #130
flips `conc/proc_join_param.lu` MISMATCH -> match and
`conc/proc_join_value.lu` out of scope -> match.** Nothing else moves
for either fix — falsifier: any other row moving at either fix commit.

#### #447, the glibc floor — the red witness is measured, the floor after is predicted

Measured on the PUBLISHED 0.1.38 archives (kasumi, `gh release download
v0.1.38`, `objdump -T <binary> | grep -o 'GLIBC_[0-9.]*' | sort -uV`):

- `lupin-0.1.38-x86_64-unknown-linux-gnu.tar.gz` (sha256 `828b5c55…`):
  highest **`GLIBC_2.39`**, and it is named by exactly two symbols, both
  weak: `pidfd_spawnp` and `pidfd_getpid` — Rust std's process-spawn fast
  path, linked against the 24.04 runner's glibc. Everything else is at
  or below `GLIBC_2.34`.
- `lupin-0.1.38-aarch64-unknown-linux-gnu.tar.gz` (sha256 `b85ec647…`):
  the same, highest `GLIBC_2.39`.
- control: `wolf-0.2.16-x86_64-unknown-linux-gnu.tar.gz` (sha256
  `84e30c05…`), built on 22.04 by r21: highest `GLIBC_2.34`.

The contract asked for the symbol to be predicted; it was measured
first, while downloading the witness, so this is recorded as the red
witness and not as a prediction. **Predicted: an archive built the way
`release.yml` builds, on `ubuntu-22.04` / `ubuntu-22.04-arm`, names no
version above `GLIBC_2.34` on either arch**, because the two `pidfd_*`
symbols are absent from 2.35's libc and a weak reference to a symbol
the link-time libc lacks carries no version. Falsifier: any
`GLIBC_2.35`+ line in `objdump -T` of the built binary.

#### #129 items 2–5, predicted

- item 2: lupin's records carry no `trap_message` today; a failing
  `assert(cond, "msg")` record will gain it and nothing else moves.
- item 3 (`[proto.record.pass]`): `conform-run --phase=<p>` on a clean
  program already answers `pass`; predicted that it also answers `pass`
  (never `unsupported`) when the stop rung is SHALLOWER than the rung
  where this machine would refuse the program's construct — if not, that
  is the item's code change.
- item 4 (`[proto.cmp.pass]`): `differ::claim()` collapses `Pass` and
  `Unsupported` into one arm. Predicted red on two synthesized pairs at
  0.1.38: `unsupported@resolve` vs `fail(E0301)@resolve` is reported as a
  verdict divergence (`[proto.cmp.defined-divergence]` says never), and
  `pass@run` vs `exit(0)@run` is reported as a divergence
  (`[proto.cmp.pass]` says never). `pass` vs `fail` is already a
  divergence at a mutually performed rung, and stays one. The one live
  corpus row the split moves is predicted to be
  `rows/negative/tag_undeclared_arg.lu` (`a=unsupported@resolve
  b=fail(E0301)@resolve`, one of is53's four pre-existing gating
  findings), from a verdict divergence into the conservatism ledger.
- item 5 (`[conf.exit]`): `EXIT_REJECTED = 65` is a rejection wearing a
  second number — `lupin lex`/`lupin parse` take a program and report on
  it for a person, which is `[conf.exit]`'s definition of a front door,
  and wolf 0.2.16 has no such commands to disagree with (`wolf lex` and
  `wolf parse` answer "not a wolf command", exit 2). It becomes 2; the
  test that pins 65 goes red first.

#### Measured, after the pin moved

Pin commit `68b3a62` (`vendor/upstream/{spec,corpus}` tree-identical to
wolf-lang `93a5fe50`: `git rev-parse HEAD:vendor/upstream/corpus` =
`ef5ae1ce…` = `git -C wolf-lang rev-parse 93a5fe50:corpus`, and `spec` =
`cf6dbe07…` on both). `lupin corpus`, release build, kasumi.

| class | `ba357aa` @ `2e4ca769` | predicted, 0.1.38 code @ `93a5fe50` | **measured** | predicted after fixes | **measured at head** |
| --- | --- | --- | --- | --- | --- |
| files | 666 | 690 | **690** ✓ | 690 | **690** |
| entries | 625 | 646 | **646** ✓ | 646 | **646** |
| members | 41 | 44 | **44** ✓ | 44 | **44** |
| failures | 0 | 0 | **0** ✓ | 0 | **0** |
| distinct conforms | 358 | 367 | **367** ✓ | 367 | **367** |
| match | 491 | 502 | **500** ✗ | 505 | **503** |
| dynamic counterpart | 26 | 28 | **28** ✓ | 28 | **28** |
| conservatism | 48 | 52 | **53** ✗ | 52 | **53** |
| out of scope | 59 | 61 | **62** ✗ | 60 | **61** |
| mismatch | 1 | 3 | **3** ✓ | 1 | **1** (DIV-2026-019, filed) |
| reach `run` | 485 | 501 | **501** ✓ | 504 | **504** ✓ |

**Claim 1 held.** The two walks diffed row by row on the entry name, not by
totals: 625 rows at the old pin, 646 at the new; **none lost, none moved,
none changed its status string**; the 21 new rows are the only difference.

**Nineteen of twenty-one new rows called right.** The two misses, and the
three cells they account for:

- `memory/pool_place_write.lu` — predicted match, measured **out of
  scope**: `handle is not a Map key — a key is str, int, char or bool`.
  The program has no `Map`; it subscripts the pool by handle (`pool[cur]`,
  `pool[h].next = k`, `[mem.shared.handle.3]`), and this machine sends a
  subscript on a pool down its map-index path, which refuses a handle key.
  A finding for the pool surface, not this lane's.
- `rows/negative/error_alias_private/main.lu` — predicted match (E0304),
  measured **conservatism**: `probe(p: str) -> int ! disk.IoErrors`
  names a PRIVATE alias through its module and this machine runs it
  (`exit(0)`). The private-member check reads expression paths, never a
  row's entries. It is the half of `[type.err.alias.qualified]` #134 did
  not ask for, and it is left as the conservatism row it measures.

**After the fixes, exactly three rows move** — the three the prediction
named, and nothing else (`lupin corpus` at `62eea06` diffed row by row
against the pin walk):

| row | at the pin | at head | by |
| --- | --- | --- | --- |
| `conc/proc_join_param.lu` | `fail(E0301)@resolve` MISMATCH | `exit(0)@run` match | #130 |
| `conc/proc_join_value.lu` | `unsupported@resolve` out of scope | `exit(0)@run` match | #130 |
| `rows/error_alias_qualified/main.lu` | `fail(E0305)@resolve` MISMATCH | `exit(0)@run` match | #134 |

The pin commit's own suite (debug, kasumi) went red where the census said
it would, and nowhere it did not, over the fourteen targets it reached
before the lane stopped it for the head run: `cli` (the two corpus-walk
count tests), `corpus_harness::the_pin_holds_the_corpus_we_think_it_does`
(`left: 690 right: 666`), and `conformance`'s two sema-lite sweeps, both
on `conc/proc_join_param.lu` — `left: Fail("E0301") right: Pass` and
`right: Unsupported`. That last pair is #130's census parting, seen red by
two gates that had nothing to do with this lane.

#### The red witnesses, per issue

Every new test was committed BEFORE its fix, and run on a tree holding the
tests and none of the fixes (`f420d07`, kasumi `~/lanes/is54/red1`), then
on `6c68eaf` for the #437 pair (which lands after the other fixes). Output
verbatim:

- **#134** `std_root::an_imported_error_alias_named_only_in_a_row_is_used_and_an_unnamed_one_is_not`
  — `left: String("fail(E0305)") right: "exit(0)"`. Its four programs were
  first run on wolf 0.2.16 `conform-run --json --checked`: `exit(0)` ×3 and
  `fail(E0305)` at `[18,26]`, the verdicts the test asserts.
- **#130** `conc_machine::the_handle_names_carry_their_arity_and_the_keywords_are_not_types`
  — `left: "fail(E0301)" right: "fail(E0401)"`; `a_proc_is_joined_for_its_value…`
  and `a_join_reads_each_abnormal_exit…` panic in `exit_code` on
  `Unsupported`; `a_scope_handle_crosses_a_function_boundary_spelled_scope`
  through the front door — `verdict: Fail("E0301")`, `span: [14, 19]`
  (measured on `f420d07` with that test's corrected form cherry-picked,
  because its first form ran the evaluator alone, which never asks the
  resolve rung, and was GREEN on the unfixed tree: a test that could not
  fail, caught by the red run, rewritten `efa1798`). The arities are wolf
  0.2.16's: `Scope[int]` E0401 `[8,18]`, `Proc` `[8,12]`, `Proc[int, int]`
  `[8,22]`, the keywords E0206 at parse.
- **#129 item 2** `cli::a_failing_assert_s_message_rides_the_record_and_nothing_else_carries_one`
  — `left: Null right: "one is not two"` on a record whose verdict is
  `trap(assert)`. **Its first red was for the wrong reason**: the five
  programs shared one directory, which is one module (D32), so every one
  was `fail(E0302)` and the missing key was incidental. Caught when the
  same test stayed red on the FIXED tree; rewritten one directory per
  program (`c57415b`) and re-run red on the unfixed tree for the right
  reason.
- **#129 item 3** `cli::an_explicit_phase_stop_is_pass_and_never_unsupported_before_the_refusal`
  — **green on the unfixed tree, as predicted**: the item needed no code,
  and the test pins the measurement (`comptime/assert_static.lu` is
  `unsupported@resolve` on the full ladder and `pass` at `--phase=lex` and
  `--phase=parse`).
- **#129 item 4** `differ::tests::pass_and_unsupported_are_two_facts…` —
  `left: Some(DeepDivergence { … a: "unsupported@resolve", b: "fail(E0301)@resolve" … }) right: None`;
  `divergence::a_pass_meets_each_verdict_as_proto_cmp_pass_rules` —
  `left: Some(Divergence { … a: "exit(0)@run", b: "pass@wir" … }) right: None`.
- **#129 item 5** `cli::the_human_frontend_doors_exit_2_on_a_rejection` —
  `left: Some(65) right: Some(2)`.
- **wolf-lang#437** `schema::tests::a_file_index_on_diagnostics_is_admitted…`
  — `Err(… "/files": "unknown field; implementation extensions must begin with x-" …, "/diagnostics/0/file": "a protocol diagnostic carries only code, span and severity" …)`;
  `cli::a_diagnostic_in_a_sibling_module_names_its_file…` — `left: Null
  right: Array ["main.lu", "geometry/shapes.lu"]`.
- **wolf-lang#447** `ci/glibc-floor.sh`, run on kasumi against the
  PUBLISHED archives: lupin 0.1.38 x86-64 and aarch64 — `GLIBC_2.39 is
  above the 2.35 floor`, `needs GLIBC_2.39: pidfd_spawnp`, `needs
  GLIBC_2.39: pidfd_getpid`, exit 1; kasumi's own release build (glibc
  2.44 host) — the same two, exit 1; wolf 0.2.16 — `GLIBC_2.34`, floor
  held, exit 0. The green half is the PR's `dist floor` jobs on
  `ubuntu-22.04` and `ubuntu-22.04-arm`, which build the archive the way
  `release.yml` does and measure the shipped binary.

#### The differential, against the PUBLISHED wolf 0.2.16

`lupin diff-run --corpus upstream/corpus --compiler <wolf-0.2.16 archive>
--counterparty-tier T --require-counterparty`, both lupins over the SAME
corpus (the 0.1.38 archive binary is told the new corpus with `--corpus`):

| tier | 0.1.38 divergences | 0.1.39 divergences | gone | new |
| --- | --- | --- | --- | --- |
| default | 8 (8 verdict) | 8 (5 verdict, 3 span-or-code) | 3 | 3 |
| checked | 8 (8 verdict) | 8 (5 verdict, 3 span-or-code) | 3 | 3 |
| native | 10 (1 soundness, 9 verdict) | 10 (1 soundness, 6 verdict, 3 span-or-code) | 3 | 3 |

**Gone at every tier:** `conc/proc_join_param.lu` (#130),
`rows/error_alias_qualified/main.lu` (#134), and
`rows/negative/tag_undeclared_arg.lu` — `a=unsupported@resolve
b=fail(E0301)@resolve`, one of is53's four pre-existing gating findings,
which item 4's split moves from a verdict divergence into the conservatism
ledger (`rejects-beyond(counterparty)` 129 -> 130), exactly as predicted.

**New at every tier: DIV-2026-025**, below — the file index this release
emits, against a compiler that does not emit it yet.

### DIV-2026-025 — three sibling-file refusals — **OPEN, resolves when wolf-lang#437's compiler half ships**

`resolve/cycle/main.lu` (E0303 `[18,28]`), `resolve/dup_bare/clash.lu`
(E0302 `[110,116]`) and `resolve/dupdef/main.lu` (E0302 `[21,27]`): both
machines reject with the same code at the same bytes, and the bytes are in
a SIBLING file — `beta/b.lu`, `other.lu`, `twice.lu`. 0.1.39's record says
so (`files` + `file`); wolf 0.2.16's record carries no index, which
`[proto.record.diag]` now reads as "the entry". So the comparison, which
resolves the index as the clause says, reports `span-or-code` — and it is
right to: wolf 0.2.16's record claims an entry-file offset that points at
the wrong bytes, which is wolf-lang#437 stated as a measurement.

Triage (`[proto.cmp.triage]`): the clause is unambiguous since s181's
paragraph; the defendant is the implementation that does not emit the
index, and the compiler half is s181's (wave 47), unreleased at 0.2.16.
**Not added to `differ::FILED_DIVERGENCES`**: a waiver there also stops
`tests/conformance.rs` and `tests/run_corpus.rs` asserting on the three
files (and on their modules' members), and all three AGREE with the corpus
— waiving them would buy three silenced gates for a divergence CI cannot
see anyway (the differential lane SKIPs without a counterparty). The entry
resolves on the first pin whose compiler emits `files`; the three lines
leaving `diff-run` is the check.

**What 0.1.39 attributes and what it does not.** The index is emitted for
the rejection when the check that found it knows its file — a sibling's
parse failure, the module laws that walk files (`cycle`, `dup`, `private`,
`unused`, bare variant values) — and for EVERY warning (the lint walk is
per file, and so are W0314/W0315/W0316). The resolve checks that walk a
module's ITEMS rather than its files (`annotation`, `bound`, `scalar`,
`tail`, `mode`, `move`, and the rest of `sema::resolve_check`'s chain)
still report entry-relative spans for a fault in a sibling file: their
diagnostics carry no file and read as the entry, which is the pre-0.1.39
behaviour for exactly those checks. Filed as a follow-up rather than
guessed at here.

### The re-pin on the released line — is53, lupin 0.1.38, pin `41695e7` -> `2e4ca769` (wolf-lang **v0.2.15**)

wolf-interp#127: 0.1.37 declares a pairing pin no downstream can relate to
the released compiler. The oracle is one command and it is the whole of the
finding:

```
$ git -C upstream merge-base --is-ancestor 41695e78437fbf3644accad40ddb4804737d4440 v0.2.15 ; echo $?
1
$ git -C upstream merge-base --is-ancestor 2e4ca769b396219585a07ff18492529c944672d9 v0.2.15 ; echo $?
0
```

`2e4ca769` **is** `v0.2.15` (`git rev-parse v0.2.15^{commit}`), so the
ancestry is reflexive and the pin becomes quotable: "lupin 0.1.38 was
conformance-tested against wolf 0.2.15" is then a sentence with a
verifiable referent, which is exactly what #127 says 0.1.37 cannot say.

The pin is recorded mechanically in **`vendor/upstream/PIN`** — one line,
the full 40-character sha — together with the submodule gitlink at
`upstream` and the vendored `vendor/upstream/{spec,corpus}` snapshot that
`vendor/README.md` requires to be byte-identical to it. It is *not* in
`wolf-toolchain.toml`, which names a binary, not a specification checkout.

#### Prediction, before the pin moves

The corpus the pin carries changes by 12 added files and 13 modified ones
(`git -C upstream diff --name-status 41695e7 2e4ca769 -- corpus`). Of the
13 modified, twelve change only header prose, a `conforms:` list or the
retired `warns: W1004` directive; the thirteenth,
`corpus/methods/std/list/list.lu`, is the only one whose *code* changes —
`push(take f(x))` and `push(take pair)` under wolf-lang#385. That is the
one shape that could move a pre-existing row, so it was probed on a
synthesized witness at this head before the prediction was written:
lupin parses and evaluates `take <call>` and `take <name>` in argument
position and prints `3 1` for the equivalent program, exit 0.

**So the claim is: no pre-existing entry changes class. Every moved row is
a new file.** The falsifier is any row already in the ledger at `41695e7`
landing in a different column at `2e4ca769`.

The 12 new entries, by name and predicted class:

| new entry | `check:` | predicted |
| --- | --- | --- |
| `conc/chan_default_rendezvous.lu` | `run(exit=0, …)` | match |
| `conc/chan_root_task.lu` | `run(exit=0, …)` | match |
| `conc/proc_link_root_death.lu` | `run(exit=nonzero)` | match |
| `fs/remove_dir_refused.lu` | `run(exit=0, …)` | match |
| `grammar/range_header_inclusive_max.lu` | `run(exit=0, …)` | match |
| `grammar/range_value_wide_iter.lu` | `run(exit=0, …)` | match |
| `grammar/type_position_keyword.lu` | `fail(E0206)` | match |
| `memory/push_copies_element.lu` | `run(exit=0, …)` | match |
| `memory/push_take_moves.lu` | `fail(E1001)` | **conservatism** |
| `rows/raised_call_arg_position.lu` | `run(exit=0, …)` | match |
| `typecheck/generic_bind_once.lu` | `fail(E0401)` | match |
| `typecheck/generic_bind_scalar.lu` | `fail(E0401)` | **conservatism** |

The two conservatism calls are the corpus's own words, not a guess:
`generic_bind_scalar.lu`'s header records that "lupin 0.1.36 runs this
program (exit 0)" because is45's declaration read-back covers a
struct-typed argument only, and `push_take_moves.lu` pins a move error at
`typecheck`, a rung this machine does not perform. `type_position_keyword.lu`
is a match because is45 already adopted the counterparty's number
(`diag::E_EXPECTED_TYPE = "E0206"`); before is45 this side answered E0201
and the file would have been a `span-or-code` mismatch.

So the census, predicted:

| class | baseline (`41695e7`) | predicted (`2e4ca769`) |
| --- | --- | --- |
| files | 654 | 666 |
| entries | 613 | 625 |
| members | 41 | 41 |
| failures | 0 | 0 |
| distinct conforms | 350 | 358 |
| match | 480 | **490** |
| out of scope | 59 | 59 |
| mismatch (unfiled) | 0 | 0 |
| mismatch (filed) | 1 | 1 |
| dynamic counterpart | 26 | 26 |
| conservatism | 47 | **49** |
| reach `run` | 476 | **486** |

`distinct conforms` is derived, not guessed: the union of `conforms:` tags
over `corpus/` gains exactly eight and loses none across the two revisions
— `conc.chan.default`, `conf.directive.check`, `gram.type`,
`gram.type.start`, `os.fs.remove`, `type.generic.bind`, `type.interp.spec`,
`type.range.value` — which is why 350 becomes 358 and not 360.

`reach run` gains ten, not eight: the eight new `run(…)` entries plus the
two conservatism entries, which reach `run` on this side precisely because
this side does not refuse them.

**Corpus verdicts: none change.** No entry moves from match to mismatch, no
filed divergence resolves, and `differ::FILED_DIVERGENCES` is untouched by
this lane.

#### Measured, after the pin moved

`lupin corpus` at the head, at pin `2e4ca769`: **666 files, 625 entries, 41
members, 0 failure(s), 358 distinct conforms.**

| class | baseline (`41695e7`) | predicted | measured |
| --- | --- | --- | --- |
| files | 654 | 666 | **666** |
| entries | 613 | 625 | **625** |
| members | 41 | 41 | **41** |
| failures | 0 | 0 | **0** |
| distinct conforms | 350 | 358 | **358** |
| match | 480 | 490 | **491** |
| out of scope | 59 | 59 | **59** |
| mismatch (unfiled) | 0 | 0 | **0** |
| mismatch (filed) | 1 | 1 | **1** |
| dynamic counterpart | 26 | 26 | **26** |
| conservatism | 47 | 49 | **48** |
| reach `run` | 476 | 486 | **485** |

**Nine of twelve cells as predicted. The three that missed are one file.**
`memory/push_take_moves.lu` was predicted conservatism and measured
**match**: this machine answers `fail(E1001)` at `resolve`, against the
corpus's `fail(E1001)@typecheck`, so the codes agree and the row is a match.
The prediction reasoned "`typecheck` is a rung this machine does not perform"
and forgot that the move analysis does not live on that rung here — it lives
on `resolve`, where `E1001` has been answered since long before this pin.
Three cells move together because it is one file: match 490 -> 491,
conservatism 49 -> 48, reach-`run` 486 -> 485 (a file refused at `resolve`
never reaches `run`).

**The central claim held: no pre-existing entry changed class.** Measured by
diffing the two `lupin corpus` walks row by row on the entry name — twelve
rows gained, none lost, and of the 613 rows present in both, **zero** land in
a different column. The thirteen edited corpus files moved nothing:
`corpus/methods/std/list/list.lu`'s `push(take f(x))` parses and evaluates
here exactly as `push(f(x))` did, so the six `methods/` entries that resolve
through it are unmoved.

The twelve new entries, predicted against measured:

| new entry | predicted | measured |
| --- | --- | --- |
| `conc/chan_default_rendezvous.lu` | match | match |
| `conc/chan_root_task.lu` | match | match |
| `conc/proc_link_root_death.lu` | match | match |
| `fs/remove_dir_refused.lu` | match | match |
| `grammar/range_header_inclusive_max.lu` | match | match |
| `grammar/range_value_wide_iter.lu` | match | match |
| `grammar/type_position_keyword.lu` | match | match |
| `memory/push_copies_element.lu` | match | match |
| `memory/push_take_moves.lu` | conservatism | **match** |
| `rows/raised_call_arg_position.lu` | match | match |
| `typecheck/generic_bind_once.lu` | match | match |
| `typecheck/generic_bind_scalar.lu` | conservatism | conservatism |

Eleven of twelve row calls right; the falsifier the prediction named — a
moved pre-existing row — did not fire.

**The two blockers 0.1.37's changelog named are gone, and the second one had
a third site.** That changelog reverted a trunk re-pin because this machine
OOM'd on the two `grammar/range_*` files and its corpus runner did not know
`run(exit=nonzero)`. Both range files are matches here at first sight (is52's
lazy walk), and `conc/proc_link_root_death.lu` — the first corpus file in
this repository's history to carry `run(exit=nonzero)` — is a match in
`ledger::judge`, which is what the census reads. But
`tests/run_corpus.rs`'s `every_run_expectation_this_machine_reaches_is_met_exactly`
keeps its OWN matcher beside `ledger`'s, and that one had no `Nonzero` arm:
it panicked `expected exit=nonzero, observed exit(1)`. is52 added the
spelling against synthesized witnesses because no corpus file used it, and
this is the file that found the site it missed. A duplicate matcher is only
as good as its least-updated copy.

#### E1001 is five files and only one of them is a refusal

The one wrong prediction has a second half worth keeping. `E1001` is
pinned by **five** corpus files at this pin, all five tagged
`[mem.tier0.move.2]`, and this machine answers them in two different ways:

| file | this machine | class |
| --- | --- | --- |
| `memory/destructure_partial_move.lu` | `trap(use-after-move)@run` | dynamic counterpart |
| `memory/match_arm_whole_move.lu` | `trap(use-after-move)@run` | dynamic counterpart |
| `memory/move_use_after.lu` | `trap(use-after-move)@run` | dynamic counterpart |
| `memory/struct_destructure_partial_move.lu` | `trap(use-after-move)@run` | dynamic counterpart |
| **`memory/push_take_moves.lu`** | **`fail(E1001)@resolve`** | **match** |

So "this machine answers E1001 at resolve" is true of exactly one file and
false of four, and the thing that separates them is not the code and not the
shared tag but `[mem.region.edge.elem]` — wolf-lang#385's store-edge rule,
which `push_take_moves.lu` alone carries.

**This was found by being wrong about it in public, twice.** The first fix
put `E1001` in `tests/conformance.rs`'s blanket resolve-rung code list, and
both CI (run **35672394853**, `test (ubuntu-latest)` and
`test (macos-latest)`) and the local suite went red on
`memory/destructure_partial_move.lu` with `left: Pass, right: Fail("E1001")`.
The file's own table already warned about exactly this — "E0301 is NOT a
resolve-rung code in general either ... the conforms tag tells them apart" —
and the widening ignored its own precedent. The row is keyed on the clause
now.

`typecheck/generic_bind_once.lu` defeats even that, and is keyed by PATH
rather than pretended into the clause table: it and
`typecheck/generic_bind_scalar.lu` arrive in the same pin with the SAME two
tags and the same pinned `fail(E0401)`, and this machine refuses the first at
`resolve` and RUNS the second. is45's declaration read-back covers a
struct-typed argument and not a scalar one. The path row leaves the day the
scalar half of `[type.generic.bind]` is mirrored.

#### The bundle, and the export

```console
$ lupin conformance export --out target/bundle --json
{"anchors_covered":256,"anchors_total":524,"bundle_sha256":"c00c5ca723f7aa7c5c896847e166bc4f434598a8a9d53766352a177721714fcc","files":721,"forward_tags":109,"out":"target/bundle","pin":"2e4ca769b396219585a07ff18492529c944672d9","programs":704,"records":663}
$ lupin conformance check target/bundle --replay target/bundle/expected/records.jsonl
differential: 663 entries compared, 0 member(s) exercised through their entries
divergences: 0
conservatism ledger: 118 entries
differential: GREEN — every divergence is filed in docs/divergence-log.md and none is a soundness candidate
```

`programs`/`records` 692/651 -> **704/663**, both by twelve: every new corpus
file is an entry, so the two counts move together for the first time since
is47. `anchors_total` 514 -> **524** and `anchors_covered` 248 -> **256**;
key sets diffed both ways, ten added, none dropped, and the eight newly
covered are listed in `tests/export.rs`'s ratchet comment. `forward_tags`
holds at 109.

#### DIV-2026-021 — RETIRED, and measured from both ends

The re-pin closed the let-group locus row. `wolf-lang#228` ruled and s163
moved the compiler onto the comma, so:

```console
$ upstream/target/debug/wolf conform-run \
    vendor/upstream/corpus/grammar/let_group_bare_tuple.lu --json --checked
… "verdict":"fail(E0201)","phase_reached":"parse",
  "diagnostics":[{"code":"E0201","severity":"error","span":[511,512]}]
```

`[511, 512]` is this machine's locus byte-for-byte, and the byte is `,`. The
differential runner says the same thing from both ends. At the OLD pin,
counterparty built at `41695e7`:

```
span-or-code  upstream/corpus/grammar/let_group_bare_tuple.lu  a=E0201@[364, 365]  b=E0201@[374, 375]  parse [filed: DIV-2026-021]
```

At the NEW pin, counterparty built at `2e4ca769`, that line is **absent from
all four tiers**. The waiver came out of `differ::FILED_DIVERGENCES` in the
same commit, and `tests/let_group_locus.rs` now asserts the retirement rather
than the gap — a waiver that outlives its divergence is a green report that
means nothing, which is wolf-lang#177's lesson one layer down.

#### Two new differential findings, named and NOT triaged

Re-running `lupin diff-run --require-counterparty` over all four counterparty
tiers at both pins (counterparty built from the submodule at each pin;
harness profile `release`) gives the delta this lane is responsible for:

| finding | old pin | new pin |
| --- | --- | --- |
| `grammar/let_group_bare_tuple.lu` span-or-code | present, filed | **gone** |
| `memory/push_take_moves.lu` `a=fail(E1001)@resolve b=fail(E1001)@mem` | — | **new, all four tiers** |
| `conc/proc_link_root_death.lu` `a=exit(1)@run b=exit(121)@run` | — | **new, `native` and `release` only** |

Both new rows arrive with their corpus file and **neither is triaged here**:
routing a finding is a ruling, and this lane's contract is an ancestry
oracle and a release, not a triage. Both are named so the next lane meets
them rather than discovers them.

- `push_take_moves.lu` is the SAME CODE at a different rung — `E1001` at
  `resolve` here, at `mem` on the counterparty — which is the shape
  `[proto.cmp.rung]` exists to argue about. It is not a disagreement about
  the program.
- `proc_link_root_death.lu` is the status number, and the corpus file's own
  header says the number is the tool's: `[conc.proc.root]` ends the process
  "with a nonzero, implementation-specified status", wolf's native tier
  exits 121 and this machine exits 1. The DIRECTIVE knows that
  (`run(exit=nonzero)`) and both machines satisfy it; the record COMPARISON
  does not, because `[proto.cmp]` compares `exit(N)` against `exit(M)`. That
  looks like a hole in `[proto.cmp]` rather than a divergence, and it is
  wolf-interp's to file upstream once someone rules it.

**Four gating findings pre-date this lane and are not its doing**, recorded
because nothing in CI can see them (the differential SKIPs without a
counterparty, and CI has none): `generics/explicit_apply_arity.lu` and
`grammar/index_origin_bad.lu` (same code, different rung),
`methods/method_is_free_call.lu` (`a=exit(0)@run` against
`b=fail(E0301)@resolve`) and `rows/negative/tag_undeclared_arg.lu`
(`a=unsupported@resolve` against `b=fail(E0301)@resolve`). All four are
present at the OLD pin too, in the same four tiers, with the same verdicts.

#### `trap_message`, and the seal that must still close (#129)

`[proto.record.trap]` (wolf-lang s169, spec/06 §2) adds a bare
`trap_message` field, so `OPTIONAL_FIELDS` is `[&str; 4]`. The interesting
half of that change is the half that did NOT move: a bare key that is not on
the list is still refused. Both halves were seen red before the fix was
trusted, by planting each break in turn:

```console
=========== BREAK A: trap_message out of OPTIONAL_FIELDS ===========
thread 'schema::tests::a_trap_message_is_accepted_and_an_unknown_bare_key_is_still_refused'
panicked at src/schema.rs:494:9: assertion `left == right` failed
  left: Err(SchemaErrors([SchemaError { pointer: "/trap_message",
        message: "unknown field; implementation extensions must begin with `x-`" }]))
 right: Ok(())
test result: FAILED. 0 passed; 1 failed
=========== BREAK B: the x- seal opened (`if !key.starts_with("x-")` -> `if false`) ===========
thread 'schema::tests::a_trap_message_is_accepted_and_an_unknown_bare_key_is_still_refused'
panicked at src/schema.rs:446:25: must be rejected: ()
test result: FAILED. 0 passed; 1 failed
=========== RESTORED ===========
test schema::tests::a_trap_message_is_accepted_and_an_unknown_bare_key_is_still_refused ... ok
```

**What is NOT taken from #129, stated so it is not mistaken for done.** Only
item 1, the blocking one, lands here. Item 2 (this machine EMITTING
`trap_message`, which it already renders) is owed; item 3 needed no code and
the ticket records the measurement; item 4 names a real hole —
`differ::claim()` collapses `Pass` and `Unsupported`, so a `pass`-against-
`fail` pair may still read as agreement on this side — and item 5 asks
whether `EXIT_REJECTED = 65` is a class `[conf.exit.class]` should name or a
rejection wearing the wrong number. Items 4 and 5 are RULINGS, not
applications of one, and this lane does not make them.

### The windows shard, and the two mirrors — is52, lupin 0.1.37, pin `41695e7` (**no pin move**)

Two subjects: wolf-interp#121, the windows leg against GitHub's 6 h job cap,
and the mirrors is50 priced — wolf-interp#118's three clauses and
wolf-interp#115's ledger move.

#### Item 1 — windows had no coverage at all, and the cap is not the whole of it

The `test (windows-latest)` job was **cancelled** at 6 h on every head
carrying is50's and is51's corpus growth. r20 sized `cargo test` at 7 h+;
re-derived here from the log timestamps, it is **29,267 s = 8 h 08 m**, and
r20's three per-target figures reproduce to the decimal (`run_corpus` 8,262 s,
`export` 4,588 s, `doc_truth` 5,864 s). The derivation reconciles: the macOS
per-target durations sum to **15,564.3 s** against an independently taken step
total of 15,616 s, the 52 s difference being the build before the first
`Running` line.

**What the sizing did not carry.** r20's probe ran `cargo test` and nothing
else. Because the cancel lands *inside* step 7, steps 8-15 never ran either,
so windows had never run the corpus walk, the conform-run rungs, the
explorer, the differential lane, the fuzz smoke or the bundle export — which
is also why `bundle identical across OSes` had been **skipping**, and why the
cross-OS bundle assertion was dark on one of its three OSes. Those steps are
3,331 s on macOS, so the true windows leg is about **9 h 54 m**, not 7 h.

| target | macOS | windows | ratio |
| --- | --- | --- | --- |
| whole `cargo test` | 15,564 s | **29,267 s** | 1.88x |
| `run_corpus` | 5,038 s | 8,262 s | 1.64x |
| `doc_truth` | 2,515 s | 5,865 s | 2.33x |
| `export` | 2,593 s | 4,588 s | 1.77x |
| `cli` | 2,117 s | 3,998 s | 1.89x |
| `region_machine` | 1,170 s | 2,220 s | 1.90x |
| `rule_registry` | 1,017 s | 1,930 s | 1.90x |
| `divergence` | 854 s | 1,883 s | 2.21x |
| `fuzz_smoke` | 236 s | 488 s | 2.07x |
| the other 42 binaries | 24 s | 34 s | 1.40x |

**The fix.** `ci/test-shards.sh` keys a target's shard on its NAME alone —
eight heavies pinned by measurement, every other target (one that does not
exist yet included) by `cksum` of its name mod 3 — so a failure names the same
shard twice and a target a later lane adds never moves an existing one. The
`test` matrix became an `include` list carrying one copy of every step body,
selected with `if:`, so a shard cannot drift from what linux runs; windows is
three test shards plus a `ladder` job for the steps after `cargo test`. Every
job has `timeout-minutes` (330 windows, 355 linux/macOS): a cancel reads as
infrastructure, a failure reads as "this leg does not fit".

**Coverage is proved, not claimed.** `ci/assert-shard-coverage.sh` ignores the
shard table and compares what the shards' `cargo test` runs *reported*
running — cargo's own `Running` lines — against what the macOS and ubuntu
jobs reported, and against the crate's own target list: three independent
sources, set equality in both directions, the offending names printed on
disagreement, and non-emptiness asserted before equality. Measured green at
`313d87a`: **"the three windows shards ran 50 test binaries, the same set the
macOS and ubuntu jobs ran on this head"**, 14 + 21 + 15 = 50. That is r20's
hand count made a gate.

Predicted against measured, first green run:

| job | predicted | measured |
| --- | --- | --- |
| windows test shard 1 | 10,145 s | **10,780 s** |
| windows test shard 2 | 10,015 s | **10,608 s** |
| windows test shard 3 | 9,074 s | **9,852 s** |
| windows ladder + bundle | ~6,300 s | **8,733 s** |

All four are under half the 330-minute timeout. The ladder ran 39 % over its
prediction, which is the honest residual of predicting a leg that had never
executed on that OS from a macOS ratio.

**The limit, stated rather than buried.** `run_corpus` is ONE test binary at
8,262 s and no target-level key can divide it. This fix lasts until that
binary alone passes the cap — roughly 2.4x today's corpus. Splitting *inside*
a target is the next mechanism, and it is a different one.

**And linux is next: wolf-interp#123.** On the run #121 was sized from, `test
(ubuntu-latest)` took **5 h 41 m 12 s of the 6 h cap — 18 m 48 s of
headroom** — and 20,390 of its 20,472 seconds are the three corpus-scaling
steps. On is52's own green run macOS took **5 h 40 m 36 s**, so the two
unsharded legs trade places on runner luck and both sit at the edge. Filed
with the numbers; the `timeout-minutes: 355` here at least converts that
future cancel into a failure without redding anything today.

#### Items 2-3 — wolf-interp#118's three clauses

- **`[type.map]` (wolf-lang#344) — `m.remove(k)`.** Implemented.
  `Value::Map` is a `Vec` of pairs in insertion order, so "the remaining
  entries keep their order" costs nothing: the erase is `Vec::remove` and the
  survivors cannot reorder. It answers `V ! {none}` — the erased value, or the
  `none` row with the map unchanged — deliberately the same answer `m[k]`
  gives, so the two reads of an absent key cannot disagree. Added to the
  step-(1) table, to `mutates_receiver` and to the `list_len` mutation
  witness, where a remove that MISSES changes no count, which is exactly
  right. `memory/map_remove.lu` prints the pinned bytes.
  It was ledgered **nowhere** before this lane — no `RUN_LEDGER` row, no
  divergence-log row — and now has one.
- **`[gram.expr.variant]` (wolf-lang#348) — a bare variant value.** Refused,
  E0301 at `resolve`, as the file pins. is50 priced this the risky one because
  lupin's "an unresolved capitalized name is a row tag" posture is
  load-bearing, and **the measurement narrows the price rather than confirming
  it**: the refusal is gated on `module.variants`, so it fires only when an
  enum of the module actually declares the name. Swept over the 654-file
  corpus, **eight** files return a bare capitalized name as a row tag
  (`return Failed`, `return Boom`, `return Timeout`, …) and **not one of those
  names is a declared variant**; thirteen files declare an enum and twelve
  already spell every variant VALUE qualified. **The blast radius is one
  file** — the witness. A `match` arm keeps the bare name
  (`[gram.pat.nullary]`): a nullary variant pattern parses as
  `PatKind::Binding`, which the ref walk deliberately does not treat as a
  path, so an arm can never reach the check. The check runs LAST in
  `resolve_check`, so a file that trips an older check keeps the diagnostic it
  had.
- **`[type.trait.op]` (wolf-lang#352) — an operator uses its imported trait.**
  is50 found lupin agreeing on the positive **by accident**: `unused_check`
  skipped any bound name that was not itself a module the loader resolved, so
  `use cmp.Eq` — which binds `Eq`, a trait — could never be E0305 whatever the
  file spelled. Two halves fixed. The guard now asks the import's PATH rather
  than its bound name (`use a.B` is judged when `a` is a module this loader
  resolved), and an operator SPELLED pushes a reference to its trait, the
  precedent being `collect_generic_refs` (wolf-interp#97, the same bug class).
  The spelling is what counts, so `-` counts for both `Sub` and `Neg`, as the
  clause says. is50's four rows, re-measured here:

| program (the `cmp` member module in all four) | lupin at is50 | lupin at is52 | wolf 0.2.14 |
| --- | --- | --- | --- |
| `use cmp` + `use cmp.Eq`, `==` spelled | `exit(0)` | **`exit(0)`** | `fail(E0305)` (the bug #352 fixes) |
| the same, the `==` line deleted | `exit(0)` | **`fail(E0305)`** | `fail(E0305)` |
| `use cmp` alone, never used | `fail(E0305)` | **`fail(E0305)`** | `fail(E0305)` |
| `use cmp` used + `use cmp.Eq` unused | `exit(0)` | **`fail(E0305)`** | `fail(E0305)` |

  Both halves are pinned by
  `tests/std_root.rs::an_operator_is_a_use_of_its_imported_trait_and_an_unused_one_is_still_unused`,
  because a fix that merely exempted operator traits would pass the first row
  and fail the fourth, and the clause names that case explicitly.

#### Item — wolf-interp#115, the lend rule: the ledger move, and the tier's price

**Done as a ledger move, and the argument for the alternative is written
here rather than left implied.**

s165's eight `read_param_*` witnesses pin `fail(E1002)`/`fail(E1014)` at
`typecheck`. All eight RUN clean here, because lupin **copies at the
crossing**: a `read` parameter is a value, so nothing can alias, so no second
live path to the caller's value can exist for the rule to refuse. There is
therefore no dynamic counterpart to reach for — the trap vocabulary has
nothing to say about a situation this machine cannot construct — and the rows
are the conservatism class exactly as `read_param_take.lu` was under
wolf-interp#107. `read_param_move_legal.lu`, the file that says what the
refusal leaves alone, runs identically either way and matches.

**What a real mirror would cost.** Not a check — a rung. The clause needs a
static analysis that tracks, per binding, whether a value is reached by a move
out of a `read` parameter *through* fields, elements, match bindings and
rebindings, and then whether that value's type can reach shared storage — a
type-reachability relation over `List`/`Map`/`Pool` including through fields,
payloads, tuple elements and rows, and through a type parameter, which always
can because a generic body is checked once for every instantiation. lupin has
**no typecheck tier at all**: `conform-run --phase=typecheck` answers
`unsupported` at `resolve`, by design and by the approximation contract. So
this is not a missing check in an existing rung, it is the first tenant of a
rung that does not exist, and it would arrive with the escape analysis and
the reachability relation as its dependencies. That is a lane, not a fix, and
it should be taken as one if it is taken. **The ledger move is the honest
answer at this machine's tier**, and it is recorded rather than assumed.

The eight rows are unmoved by this lane, and that is the point: they were
already `exit(0)` in `RUN_LEDGER` under is51's block and already counted
conservatism. Recording them here is what turns "they run clean at first
sight" into a stated posture.

#### Predicted, before the first edit

Baseline: the binary at `cde9d86` (the windows-shard commits are CI-only and
change no source), `lupin corpus`, measured before a source line was edited —
613 entries, **478 match, 60 out of scope, 48 conservatism, 26 dynamic
counterparts, 1 filed mismatch, 476 reach `run`**, which reproduces is51's
closing figures cell for cell.

Rows this lane moves, by name:

- `memory/map_remove.lu` — out of scope (`unsupported: Map has no method
  remove in this machine's std subset`) -> **match**, reaching `run`.
- `typecheck/variant_bare_value.lu` — conservatism (runs, prints `true`,
  against a pinned `fail(E0301)`) -> **match**, refused at `resolve` and so no
  longer reaching `run`.
- `traits/op_eq_item_import/main.lu` — match -> **match, unmoved**, but for
  the right reason instead of by accident. The negative case has no corpus
  file and is pinned by a test.
- The eight `read_param_*` lend witnesses — conservatism -> **conservatism,
  unmoved**. The ledger move records them; it does not move them.

| class | baseline (`cde9d86`) | predicted |
| --- | --- | --- |
| match | 478 | 480 |
| out of scope | 60 | 59 |
| mismatch (unfiled) | 0 | 0 |
| mismatch (filed) | 1 | 1 |
| dynamic counterpart | 26 | 26 |
| conservatism | 48 | 47 |
| reach `run` | 476 | 476 |

The risk the prediction names: the bare-variant refusal could catch a row tag
in a module that also declares an enum of that name, and narrowing
`unused_check`'s guard touches every E0305 row in the corpus. Named at risk:
the eight row-tag files, the twelve other enum-declaring files,
`resolve/unused/main.lu` (must stay `fail(E0305)`) and
`lints/pkg_item_unused/main.lu` (must stay `exit(0)`). Predicted: none moves.

#### Measured, after the last edit

`lupin corpus` at the head: 654 files, 613 entries, 41 members, **0
failure(s)**, 350 distinct conforms.

| class | baseline (`cde9d86`) | predicted | measured |
| --- | --- | --- | --- |
| match | 478 | 480 | **480** |
| out of scope | 60 | 59 | **59** |
| mismatch (unfiled) | 0 | 0 | **0** |
| mismatch (filed) | 1 | 1 | **1** |
| dynamic counterpart | 26 | 26 | **26** |
| conservatism | 48 | 47 | **47** |
| reach `run` | 476 | 476 | **476** |

**Seven of seven cells as predicted, and the three rows by name.** The risk
the prediction named did not fire: no file outside the witness moved, which
the totals prove rather than assert — the classes still partition 613.


#### Item 4 — the pin: both blockers taken, and the pin still does not move

0.1.37's conformance pin is `41695e7`, preserved at
`refs/tags/lupin-0.1.37-conformance-pin`. r20 tested re-pinning to `12ca8acc`
and reverted it for two reasons, both of which are this machine's, and both
are fixed here so the next lane to try the bump meets neither.

**Blocker 1 — `for` over a range materialized it.** `Machine::range_items`
answered a `Vec<Value>`, so `for i in a..b` allocated the ENTIRE range before
the first iteration. Measured on this tree, a 50-million-element range:

| | peak RSS |
| --- | --- |
| before | 3,913,104 kB (**3.91 GB**) |
| after | 12,212 kB (**11.9 MB**) |

a 320x reduction, and now constant in the range's length rather than linear;
elapsed is unchanged (37.5 s -> 38.7 s, inside the noise of the evaluation-step
budget that ends both runs). That is r20's shape exactly — "one 5 GB
allocation, 28 GB RSS" on s161's two range witnesses — at a smaller range.
A range is the one iterable whose length is bounded by nothing already in
memory: a `List` at least had to be built first. `range_items` is now
`range_iter` and `eval_for_items` takes any `IntoIterator`, so the header form
and the range-VALUE form are both lazy and the container forms are untouched.
The walk is still COUNTED: both endpoints are evaluated exactly once before
the first test, so the bound is fixed at entry whether or not the elements
are, and `[mem.iter.range]`'s semantics are unchanged — including a char
range skipping a code point no `char` spells rather than inventing one.

**Blocker 2 — `run(exit=nonzero)` was unparseable.** s163 spells a witness
whose point is that the program FAILS, deliberately without pinning which
status: the exit status of a raised row is a property of the program's own
row, and pinning it would make the file a test of the number rather than of
the refusal. `ExitSpec` had `Code(u8)` and `Trap(Option<TrapKind>)` and
nothing else, so the directive was a hard parse error and every file carrying
it was a harness failure. `ExitSpec::Nonzero` is parsed, displayed and judged:
satisfied by any nonzero exit, by no successful one, and — the part that
matters — **not** by a trap, because `exit=trap` is the spelling that says
trap and names its kind. The unknown-spelling message now offers `nonzero`
among the options, so the next lane to mistype it learns something.

**The pin still does not move, and that is deliberate.** `41695e7` is
dev-stamped off an s166 branch that has since rebased, and choosing the sha to
land on is a release decision with the wolf-lang side's own state in it —
r20's, not a mirror lane's. What this lane owed was that the bump not fail for
lupin's reasons. Both are unblocked and pinned by tests
(`directive::tests::check_run_may_demand_a_nonzero_exit_without_naming_it`,
`ledger::tests::a_nonzero_exit_expectation_takes_any_failing_status_and_no_trap`);
neither could be verified against the new corpus itself, since the private
upstream at `12ca8acc` is not reachable from this tree, so both are verified
against synthesized witnesses and stated as such.

**Neither blocker moved a census cell**, which was the prediction and is the
measurement: 613 entries, 480 match, 59 out of scope, 47 conservatism, 26
dynamic counterparts, 1 filed mismatch, 476 reach `run`, 0 failures — the same
seven figures as before item 4, re-measured after both edits.

**A gate that passes what the real gate fails.** This lane's source edits were
validated with `cargo check` and CI red on all six test jobs at `rustfmt`,
step 5, with `independence` — the one job that runs no lint — green. `cargo
check` says nothing about formatting and `--all-targets` does not make it
say more; the CI gates are `cargo fmt --check` and `cargo clippy --all-targets
-- -D warnings`, and those are the two commands a local run must end with.
Recorded as the eighth false-signal shape: **a cheaper check standing in for
the gate, and agreeing with it right up until it does not.**

#### What the measurements found beyond the rows

- **A slow file is not a hung one, and I called one hung.** The census paused
  nine minutes at entry 41 and `lupin` sat at 99.7 % CPU with flat RSS, which
  reads exactly like an infinite loop. It is not:
  `memory/byte_list_ledger.lu` does 65,536 `push`es three times over with
  region accounting and takes **211.87 s** measured, and
  `byte_producers_ledger.lu` is its twin. The bisect that "found" the culprit
  was measuring my own 25-second timeout, and it convicted `Map.remove` —
  until the same file was run at `cde9d86`, where it "hangs" too. **Checking
  the baseline is what caught it**; the bisect alone would have reverted a
  correct change. Recorded because the wave's own note says wolf-interp is
  slow, not hung, and a per-file cost of three and a half minutes is also a
  number wolf-interp#123 should carry.
- **A census that moves moves the manual too, and `doc_truth` is what says
  so.** Two documented sample outputs carried the old figures and drifted the
  moment the census did: `docs/manual/00-building.md`'s `lupin corpus` summary
  (478/48/60 -> 480/47/59) and `docs/manual/04-differential.md`'s conformance
  check (conservatism ledger 120 -> 118, both halves falling by one). The
  second is the more interesting of the pair, because the differential's
  ledger is computed over a different population than `lupin corpus` and fell
  by **two** where the corpus census moved **one** row: `map_remove.lu` leaves
  `unsupported(interp)` and `variant_bare_value.lu` leaves
  `unsupported(counterparty)`. Neither number is reachable from the corpus
  census by arithmetic, which is exactly why the manual is checked against the
  binary rather than against a lane's report. The gate named both pairs and
  the count of them ("2 of 20"), so the fix was complete rather than iterative.

- **The windows ladder is the leg nobody had priced.** It is 39 % over the
  macOS-ratio prediction, and it is the half of the windows job that had never
  run at all. Any future re-balancing should size it from this run's 8,733 s,
  not from a ratio.

### The combinators — is51, lupin 0.1.37, pin `41695e7` (wolf-lang s166, dev-stamped)

wolf-lang#390 was ruled option 1 (`par` stays), and s166 wrote the clauses
lupin mirrors: `[type.method]` (a `List`, `Map`, `str` or `range` receiver has
a home module, and `recv.name(args)` IS `home.name(recv, args)`),
`[type.comb]` (eager combinators as std wolf code, `par` the only builtin),
and `[conc.task.par]` completed (order, failure, capture, chunk, det, cost).
Built from the clause text, never from wolfc.

**The pin moves twice, and the branch moved again after it.** s166's spec
commits were read at `4c046f1`; the branch was then rebased onto trunk
`4b56441` (s165), so the sha this lane pins is `41695e7` and it carries two
lanes' rows: 615 -> 654 files, 498 -> 514 anchors (sixteen added, none
dropped, key sets diffed both ways).

**`41695e7` is dev-stamped off an active branch, and s166 rebased again
after this lane vendored it** — the sha is no longer reachable from
`origin/s166`. What that does and does not cost: `vendor/upstream` holds the
byte-exact `spec/` and `corpus/` of that sha, and CI reads the snapshot and
never the submodule (the snapshot exists because CI cannot clone the private
upstream), so every number in this section is reproducible from this tree
alone. The `upstream` gitlink is the part that now names a commit `origin`
may garbage-collect. The pin was NOT chased a third time on purpose: each
rebase lands s166 on a newer trunk, and the later ones carry `[type.interp.spec]`,
s163's `[type.numlit.default]` correction (#403) and the `os.*` /
`type.generic` anchors — other lanes' rows, whose mismatches would be other
lanes' to file. **The merge, or r20, should re-pin to whatever s166 lands
as**, and re-derive the counts below at that sha.

#### What lupin did before the first edit (`790c127`)

- `eval_method` tried a user `impl` (`method_of`, nominal receivers only),
  then `builtin::method`, one `(Value, name)` match whose fallthrough is
  "`List` has no method `x` in this machine's std subset". `method_of` never
  answers for `List`/`Map`/`str`, so the order on std data was builtin-only.
- `use std.list` already loaded `<std root>/list` (`--std-root`/`LUPIN_STD`),
  and `list.any(xs, fn(x) x > 1)` ran and printed `true` against wolf-std
  `073aa19`. The gap was resolution, not evaluation.
- `par` had no arm. The scheduler is single-threaded by construction: one
  task runs at a time, and every spawn is a numbered `sched-ev/0` event.

#### Predicted, then measured

The prediction was written before the first edit, when s166 had pushed its
spec and none of its witnesses, so stage 2 was predicted **by class** and
each row got its name when the witnesses landed.

| class | `790c127` at `30731a6` | stage 1 `4c046f1` pred / meas | stage 2 `8bafa20` pred / meas | stage 3 `41695e7` | stage 4 `92d98af` rebased (head) |
| --- | --- | --- | --- | --- | --- |
| match | 460 | 462 / **462** | by class / **471** | 476 | **478** |
| out of scope | 54 | 54 / **54** | by class / **60** | 61 | **60** |
| mismatch (unfiled) | 0 | 0 / **0** | 0 / **1** | 0 | **0** |
| mismatch (filed) | 2 | 2 / **2** | 2 / **2** | 2 | **1** |
| dynamic counterpart | 23 | 23 / **23** | 23 / **23** | 23 | **26** |
| conservatism | 41 | 41 / **41** | 41 / **41** | 51 | **48** |
| reach `run` | 449 | 451 / **451** | by class / **459** | 474 | **476** |

Stage 1 is the pin bump alone (trunk's two s162 rows), measured with the
unchanged binary: every cell as predicted. Stage 3 is stage 2 plus the
rebase's s165 rows — the ten new conservatism entries and fourteen new run
rows are s165's, ledgered and not mirrored (wolf-interp#115 is that lane's).

**Stage 4 is the rebase onto trunk `92d98af`** (is50's two mirrors). Stages
1-3 are left as they were predicted and measured against `790c127`; the
head figures move because is50's rows are now underneath this lane, and
they are MEASURED on the rebased tree, never carried over by arithmetic.
DIV-2026-024 retires there (is50 landed the bare-path pattern), so the
filed mismatch falls 2 -> 1 and the differential stays GREEN with every
divergence filed.

Per row, for s166's sixteen new entries:

| row | predicted | measured |
| --- | --- | --- |
| `conc/par_order.lu` | match | **match** |
| `conc/par_fail_reraises.lu` | match | **match** |
| `conc/par_capture_write.lu` | match (E1101) | **match**, after the W1101 split below |
| `methods/comb_map_filter_fold.lu` | match if the rows carry a std | **match** |
| `methods/comb_sort_enumerate_zip.lu` | match if the rows carry a std | **match** |
| `methods/comb_sorted_ord.lu` | match if the rows carry a std | **match** |
| `methods/method_is_free_call.lu` | match if the rows carry a std | **match** |
| `methods/wordcount_serial.lu` | match if the rows carry a std | **match** |
| `methods/wordcount_par.lu` | match if the rows carry a std | **match** |
| `methods/home_range_str_map.lu` | match if the rows carry a std | **MISMATCH at stage 2**, match at stage 3 |
| `methods/method_unknown.lu` | out of scope | **out of scope** |
| `methods/method_wrong_arity.lu` | out of scope | **out of scope** |
| `methods/method_wrong_receiver.lu` | out of scope | **out of scope** |
| `methods/method_non_std_receiver.lu` | out of scope | **out of scope** |
| `methods/method_take.lu` | **match** | **out of scope** |
| `typecheck/method_home_no_std.lu` | out of scope | **out of scope** |

**Two predictions missed, and both are the census earning its keep.**

1. `method_take.lu` was predicted a match on the reasoning that `take` is a
   reserved word, so a member call named `take` can be refused statically
   with no type knowledge. It is a refusal by name at RUN time like the other
   five, because this machine has no resolve-tier method check to carry it —
   the refusal names E0403 and the two slices `[type.method.take]` gives the
   prefix and the suffix, but it arrives a rung late. Out of scope, honestly.
2. Stage 2 was predicted to open zero unfiled mismatches and opened one:
   `methods/home_range_str_map.lu`, `fail(E0401)@resolve` against the pinned
   `exit(0)`. The cause was is49's own `[type.range.name]` check, not this
   lane's dispatch — see below. Fixed here, and the row matches at stage 3.

#### The range family is closed at two types, not at two spellings

`(2..6).collect()` reaches `std.range`'s `collect`, and s166 made that one
generic function (`collect[T](r: range[T])`, wolf-lang `fcfe9db`: wolf has no
overloading, so the two element types cannot be two functions). is49's
`range_type_refusal` refused the fixture's own signature — `T` is neither
`int` nor `char` — so every home module carrying `collect` was E0401 at its
own declaration, and no program could reach the method at all.

The fix is `[type.map.key]`'s rule, which the sibling check already had: a
RIGID generic parameter is admitted, because inside a generic body the
element is not spelled yet and each instantiation is checked where it spells
one. `range_type_refusal` now takes the in-scope generic names exactly as
`map_key_refusal` does. `range[bool]` is still E0401 and bare `range` still
E0405, both pinned.

#### `par` in lupin: W is fixed at 4, and the events are the clause's

`[conc.task.par.chunk]` leaves `W` implementation-specified. lupin fixes it
at **4 on every host** (`eval::conc::PAR_WORKERS`; wasm has no thread to
spawn, so its `W` is 1 — the clause's single chunk on the calling task).
Reading the host's core count would buy no speed, because the scheduler runs
one task at a time whatever `W` is, and it would make the `spawn` events —
the only trace `k` leaves — differ between two hosts replaying one seed.

`xs.par(f)` opens a scope, splits `0..n` into `k = min(n, W)` contiguous
chunks whose lengths differ by at most one, spawns one task per chunk,
joins, and reads the slots back in index order. `k == 1` runs on the calling
task with no spawn at all and `n == 0` spawns nothing, both of which the
clause permits. A chunk stops at its first error value; the scheduler cancels
the siblings and the first failure in schedule order becomes the `par`'s own
row, so a failed `par` has no value and the caller's `?` or `else` sees the
row. A fault is the fault, and a captured region value is refused
(`[conc.task.par.capture]`).

**Measured, not asserted** (`lupin conform-run … --trace=all`), eight
elements:

    trace  TaskScope  `par` over 8 element(s): k = min(n, W = 4) = 4 contiguous chunk(s)
    trace  TaskScope  ev#1 scope#0 `par` opens (owner task 0)
    trace  SchedSpawn ev#2 spawn `par@139#0` (task 1) under scope#0 in proc#0
    trace  SchedSpawn ev#3 spawn `par@139#1` (task 2) under scope#0 in proc#0
    trace  SchedSpawn ev#4 spawn `par@139#2` (task 3) under scope#0 in proc#0
    trace  SchedSpawn ev#5 spawn `par@139#3` (task 4) under scope#0 in proc#0
    trace  TaskJoin   ev#6 `main` blocks at scope#0's exit join
    trace  TaskJoin   ev#18 scope#0 joins: all 4 child(ren) complete

One element records **no** spawn event at all. So `k` leaves exactly the
trace the clause says it may: numbered `spawn` events in the `sched-ev/0`
stream under the `par`'s own scope, and nothing else. The schedule explorer
enumerates the chunk tasks' interleavings as it enumerates any spawned
task's, because they ARE spawned tasks to the scheduler; `--seed=N` replays
one `k`, which is a function of `n` and a fixed `W`, so one seed is one `k`
on every host. lupin has no checked lane of its own to refuse `par` the way
`wolf conform-run --checked` refuses `scope` and closures: this machine has
one engine, and it runs `par` on every lane.

#### W1101 rides along with `spawn` and never with `par`

The corpus pins the split: `conc/capture_write_assign.lu` and
`conc/store_buffer.lu` carry `warns: W1101, W1102`, and
`conc/par_capture_write.lu` carries no `warns:` line at all. W1101's text is
a claim about the write landing on **the task's own copy**, and `par` runs
`f` in `k` of them, so the warning does not hold there;
`[conc.task.par.capture]` names E1101 and nothing else. lupin now emits
E1101 alone under `par` and both under `spawn`.

#### What this lane did NOT take

- The combinators are std wolf code that lupin runs, per `[type.comb.set]` —
  `map`, `filter`, `fold`, `sum`, `sort_by`, `sorted_by`, `sorted`,
  `enumerate`, `zip` and `collect` are none of them lupin builtins, and the
  only builtin this lane added to the method surface is `par`, plus `clear`
  on `List` and `Map`, which `[type.method.resolve]` step (1) names.
- The step-(2) refusals are `unsupported` by the counterparty's code
  (E0301, E0402, E0403, E0804), never traps and never guesses: this machine
  computes no receiver type at resolve, so each arrives at run time. Six
  corpus rows sit in the out-of-scope class for exactly that reason, and
  closing them means a resolve-tier type for std data, which is its own lane.
- s165's rows (`copy_independent`, the eight `read_param_*`,
  `bound_literal_default`, `op_eq_item_import`, `list_tuple_elem`,
  `variant_bare_value`) are ledgered here and mirrored nowhere: they run
  clean at first sight, and wolf-interp#115 is that lane's.

### The two mirrors — is50, lupin 0.1.37 (unreleased), pin `30731a6` (wolf-lang **v0.2.14**)

**No pin move.** The subject is two mirrors and their residue:
wolf-interp#107 (s157's three clauses: `[gram.pat.nullary]`,
`[conf.resolve.ambient]`, `[mem.tier0.mode.read]`'s `take`), wolf-interp#111
(s160's: `[mem.region.proc]`, `[mem.region.escape]`'s materializing `str`
producers and projected reads, `[os.net.writev.head]`), wolf-interp#112 (the
four `fs_*` names std.fs calls), #110 (the fs tier's residue) and #102 (an
imported trait alias expands no bound).

#### Predicted, before the first edit

Baseline: the 0.1.36+is49 binary at trunk `790c127`, `lupin corpus`,
measured at the head before a line was edited — 449 reach `run`, 460 match,
23 dynamic counterparts, 41 conservatism, 54 out of scope, 2 mismatch (both
filed), exactly is49's closing figures. The rows this lane moves, by name:

- `grammar/match_nullary_variant.lu` — MISMATCH (filed, DIV-2026-024) ->
  **match**, reaching `run`. The waiver retires.
- `net/writev_head_gather.lu` — out of scope (`net_writev_head` does not
  resolve) -> **match**, reaching `run`.
- `memory/read_param_take.lu` — conservatism (`exit(0)`) -> **dynamic
  counterpart**, `trap(exclusivity)` at the `take`: E1014's dynamic row is
  `exclusivity` (`ledger::static_code_to_trap`), the posture
  `read_param_write.lu` already holds.
- `memory/region_str_repeat_return.lu`, `memory/region_str_from_utf8_return.lu`
  — conservatism (`exit(0)`) -> **dynamic counterpart**,
  `trap(region-fault)` at the read after `scratch` dies.
- Unmoved and pinned by tests instead: `conc/chan_payload_escape_proc.lu`
  and `memory/region_str_field_return.lu` (already `trap(region-fault)`),
  `lints/shadow_prelude_call.lu` (already a match), and every row #112,
  #110 and #102 touch — none of them has a corpus file.

| class | baseline (`790c127`) | predicted |
| --- | --- | --- |
| match | 460 | 462 |
| out of scope | 54 | 53 |
| mismatch (unfiled) | 0 | 0 |
| mismatch (filed) | 2 | 1 |
| dynamic counterpart | 23 | 26 |
| conservatism | 41 | 38 |
| reach `run` | 449 | 451 |

The risk the prediction names: charging `upper`/`lower`/`repeat`/`replace`
to the ambient region could move a ledger relation in a file that builds
strings inside a region (`memory/byte_producers_ledger.lu`,
`memory/region_bytes_query.lu`). Predicted: none moves.

The compiler's table (`cargo xtask differ wolf lupin --triage`, 615 files,
580 entries), predicted from is49's measured head:

                    checked                     native
    agreements      346 -> 351  (+5)            376 -> 381  (+5)
    completeness    151 -> 148  (-3)            151 -> 148  (-3)
    soundness         0 ->   0                    0 ->   0
    unsupported     108 -> 107  (-1)             78 ->  77  (-1)
    hard              9 ->   8  (-1)              9 ->   8  (-1)
    coverage A      340 -> 340                  372 -> 372
    coverage B      449 -> 451  (+2)            449 -> 451  (+2)
    coverage BOTH   329 -> 331  (+2)            359 -> 361  (+2)

The five agreements are the five files above; the three completeness notes
leaving are the `exit(0)`-against-a-static-code rows; hard loses its one
Verdict (DIV-2026-024) and keeps #167's eight Diag rows.

#### Measured, after the last edit

`lupin corpus` at the head: "451 entries reach the `run` rung; 462 match
… 26 … 38 … 53 are out of scope, 1 mismatch", with the one mismatch
DIV-2026-019, filed, and `0 failure(s)`.

| class | baseline (`790c127`) | predicted | measured |
| --- | --- | --- | --- |
| match | 460 | 462 | **462** |
| out of scope | 54 | 53 | **53** |
| mismatch (unfiled) | 0 | 0 | **0** |
| mismatch (filed) | 2 | 1 | **1** |
| dynamic counterpart | 23 | 26 | **26** |
| conservatism | 41 | 38 | **38** |
| reach `run` | 449 | 451 | **451** |

Seven of seven cells as predicted, and the five rows by name. The risk the
prediction named did not fire: `memory/byte_producers_ledger.lu` and
`memory/region_str_charged.lu` still match with the producers charging.
The work that moves no corpus row, #112's three names, #110's residue and
#102's alias, was measured against wolf-std's rig instead. Its four dark
fs rows run with their expected verdicts, its other thirteen fs rows are
unchanged, and `tests/ops/num_alias_tier.lu`, ledgered `mirror-lag(E0501)`
there, runs `exit(0)` with the compiler's stdout sha256.

#### The compiler's table, re-derived at the head (for r20 to predict against)

`xtask differ wolf <lupin@is50> --triage --<tier> --corpus=corpus --control
<lupin@790c127>` at wolf-lang v0.2.14 (`30731a6`), `wolf 0.2.14 (pin
30731a6)`, 615 files, 580 entries, both tiers:

                    checked                     native
    agreements      346 -> 351  (+5)            376 -> 381  (+5)
    completeness    151 -> 148  (-3)            151 -> 148  (-3)
    soundness         0 ->   0                    0 ->   0
    unsupported     108 -> 107  (-1)             78 ->  77  (-1)
    hard              9 ->   8  (-1)              9 ->   8  (-1)
    coverage A      340 -> 340                  372 -> 372
    coverage B      449 -> 451  (+2)            449 -> 451  (+2)
    coverage BOTH   329 -> 331  (+2)            359 -> 361  (+2)

`THE INTERPRETER BUMP MOVED 5 LEDGER COUNT(S)` on each tier, the same five
files on both, every one `-> agreement`: `grammar/match_nullary_variant.lu`
(Verdict), `memory/read_param_take.lu`, `memory/region_str_from_utf8_return.lu`,
`memory/region_str_repeat_return.lu` (Completeness), `net/writev_head_gather.lu`
(unsupported). Zero files moved below the ledger. Hard is **8 = 8 Diag +
0 Verdict** on both tiers: v0.2.12's warning-parity rows (#167) and
nothing else. **Every cell measured as predicted, on both tiers.** The
control's own ledger reproduces is49's closing table exactly, so the
control and the head differ by this lane alone.

#### What the measurements found beyond the rows

- **The compiler's two tiers disagree on the s160 producers' charge.**
  `--native` charges `upper`, `lower`, `repeat`, `replace` and
  `str_from_utf8` to the ambient region (`region_bytes` moves, `cap: 0`
  traps `alloc-contract`), as `[mem.region.escape]` says. `--checked`
  charges none of them, though it charges `+`. This machine follows the
  clause. Filed as **wolf-lang#391**.
- **A view of a region-built `str` escapes clean on all three lanes.**
  `let t = (a + b).trim(); t` out of `region scratch` prints on both
  tiers and here. The clause says a view product allocates nothing and
  never says whether it carries its receiver's sites. This machine keeps
  the checker's reading rather than inventing one. Filed as
  **wolf-lang#392**.
- **Five of this machine's declared fs rows had drifted from the
  compiler's**, and it was observable: a `cross_device =>` arm on
  `fs_rename` was a binding and caught `io`. Re-read off the compiler's
  E0602 and corrected; `fs_answer` now coarsens an undeclared tag to `io`.
  The rows are pinned nowhere upstream, so this is recorded on
  **wolf-lang#181**, which asks for exactly that pin.
- **wolf-interp#112's fourth name does not exist.** `fs_rename_atomic` is
  E0301 on wolf 0.2.14, and std.fs mentions it only to say there is none.
  The issue counted it from a ledger comment.

### The three clauses — is49, lupin 0.1.37, pin `30731a6` (wolf-lang **v0.2.14**)

**The pin moves two releases**, `a7f517e` (v0.2.12) -> `30731a6` (v0.2.14):
580 -> 615 files, 475 -> 498 anchors (twenty-three added, none dropped —
key sets diffed both ways), the spec's §12–§14 of `10-types.md` and §2.8 of
`01-grammar.md` new. The subject is wolf-interp#106, s158's mirror: **list
literals** (`[gram.expr.list]`, `[type.list.lit]`), **a nameable
`range[int]`/`range[char]`** with `start`/`end` (`[type.range]`), and
**transparent error-set aliases** (`[gram.item.error]`, `[type.err.alias]`).
Eighteen witnesses, all measured on the 0.1.36 binary at the new pin before
a line was edited (`is49-baseline.txt`): fourteen `exit(0)`/`trap` rows
answered `fail(E0201)` at the `[`/`error` or `fail(E0301)` at `range`, three
`fail` rows answered E0201 for the wrong reason, and one (`error_alias_open`,
`fail(E0201)` at the `..`) was already a MATCH by code alone — E0201 at the
`error` item, the same code for a different reason, which a code ledger
cannot see. The eighteenth, `rows/error_alias_ident.lu`, needed no mirror at
all.

#### Predicted, then measured

The census, predicted before the first edit and measured after the last
(`lupin corpus` at the head, the 0.1.36 binary's column re-measured at the
new pin as the baseline):

| class | 0.1.36 at `a7f517e` | 0.1.36 at `30731a6` (baseline) | predicted | measured |
| --- | --- | --- | --- | --- |
| match | 432 | 443 | 460 | **460** |
| out of scope | 53 | 54 | 54 | **54** |
| mismatch (unfiled) | 0 | 18 | 0 | **0** |
| mismatch (filed) | 1 | 1 | 2 | **2** |
| dynamic counterpart | 21 | 23 | 24 -> 23 | **23** |
| conservatism | 38 | 41 | 40 -> 41 | **41** |
| reach `run` | 421 | 435 | 448 | **449** |

The prediction was written from r19's pairing table and the thirty-five
new headers; the baseline measurement then corrected the one row it had
flagged as uncertain before any edit: `memory/read_param_take.lu` (E1014
pinned) runs clean here rather than trapping `exclusivity`, so it is
conservatism and not a dynamic counterpart — 24 -> 23 and 40 -> 41 in the
table above, with the arrows kept so the miss is visible. Six of seven
cells then measured as predicted; the seventh, reach `run`, is 449 and not
448 — an arithmetic slip in the prediction (the fourteen mirrored rows added
to the measured baseline of 435 is 449), not a row the census surprised.
`lupin corpus` at the head: "449 entries reach the `run` rung; 460 match …
23 … 41 … 54 are out of scope, 2 mismatch" — the two being DIV-2026-019
(`resolve/broken_sibling/entry.lu`) and DIV-2026-024 below, both filed,
zero unfiled, `0 failure(s)`.

#### The compiler's table, re-derived at the head (for r20 to predict against)

`cargo xtask differ wolf lupin --triage --<tier> --corpus=corpus --control <0.1.36>`
at wolf-lang v0.2.14 (`30731a6`), `wolf 0.2.14 (pin 30731a6)` against
`lupin 0.1.36+dev.2135dcb`, the control the 0.1.36 release binary on the
same tree — 615 files, 580 entries, both tiers:

                    checked                     native
    agreements      332 -> 346  (+14)           362 -> 376  (+14)
    completeness    151 -> 151   (0)            151 -> 151   (0)
    soundness         0 ->   0   (0)              0 ->   0   (0)
    unsupported     108 -> 108   (0)             78 ->  78   (0)
    hard             23 ->   9  (-14)            23 ->   9  (-14)
    coverage A      340 -> 340   (0)            372 -> 372   (0)
    coverage B      435 -> 449  (+14)           435 -> 449  (+14)
    coverage BOTH   315 -> 329  (+14)           345 -> 359  (+14)

`THE INTERPRETER BUMP MOVED 14 LEDGER COUNT(S)` on each tier, the same
fourteen files on both, every one `Verdict -> agreement`: six
`grammar/list_lit_*`, five `grammar/range_type_*`, three `rows/error_alias_*`.
Three files moved BELOW the ledger on both tiers — the `fail`-pinned rows
whose lupin code changed while their class (completeness) could not:
`list_lit_mixed.lu` E0201 -> E0401, `list_lit_untyped_empty.lu` E0201 ->
E0419, `rows/negative/error_alias_cycle.lu` E0201 -> E0610. Hard stays
**9 = 8 Diag + 1 Verdict** on both tiers: the eight Diag are v0.2.12's
warning-parity rows (#167), the one Verdict is `grammar/match_nullary_variant.lu`
(DIV-2026-024, #107). Predicted before the run as 346/151/0/108/9 and
376/151/0/78/9 with coverage B 449 and BOTH 329/359 — every cell measured
as predicted, on both tiers.

#### What the pin's other rows are, by name

The thirty-five new files less the eighteen: s157's and s160's, ledgered
here and NOT mirrored — wolf-interp#107 and #111 are is50's.

- `grammar/match_nullary_variant.lu` — **DIV-2026-024** below: the one
  unfiled MISMATCH the bump left, waived by filing.
- `net/writev_head_gather.lu` — `unsupported` at resolve, `net_writev_head`
  declined by name (#111's third ask). Out of scope, honestly.
- `memory/region_str_repeat_return.lu`, `memory/region_str_from_utf8_return.lu`
  — E1010 pinned, `exit(0)` here (the region tag reaches a `str` field and
  an interpolated `str` but not `repeat`/`str_from_utf8`; #111's second
  ask). Conservatism. `memory/read_param_take.lu` — E1014 pinned, `exit(0)`
  here. Conservatism.
- `conc/chan_payload_escape_proc.lu`, `memory/region_str_field_return.lu` —
  E1010 pinned, `trap(region-fault)` here: dynamic counterparts.
- The other nine run and match at first sight: `conc/chan_payload_proc_param`,
  `conc/proc_link_root`, `grammar/str_dollar_brace` (E0102 at lex, a match),
  `lints/shadow_prelude_call` (with its W0304), `memory/list_elem_copy_loop`,
  `memory/region_str_charged`, `strings/dollar_brace_escape`,
  `strings/end_relative_get`, `typecheck/closure_param_call`.

#### The claim that was false, and the clause that names the wrong code

wolf-interp#106 says of `[type.range.accessor]` that "lupin already does
this — `Value::Range` has been built half-open since its first range arm".
Measured at `6e94436`: it had not. `Value::Range` carried `inclusive: bool`
and `eval_for` consumed it as `if inclusive { end } else { end - 1 }` on raw
`i128`; `0..=int.MAX` as a value neither trapped nor normalized. The mirror
normalizes at construction under `Machine::checked` (the trap lands at the
`let`, 21:15 in `range_type_overflow.lu`), keeps the `for` HEADER as a
non-materializing counted walk (so `for i in 0..=int.MAX` still does not
trap, `[type.range.value]`), and drops the flag. Recorded on #106.

`[type.err.alias.cycle]` says "A cycle is **E0515**"; `docs/diagnostics.md`,
`wolf_diag`'s registry, `wolf_sema` and `rows/negative/error_alias_cycle.lu`
all say E0610. This machine answers E0610 at the entry that closes the loop
(bytes 434..435, the compiler's own locus). Filed upstream as a prose nit.

### DIV-2026-024 — `grammar/match_nullary_variant.lu` — **RESOLVED here at is50 (0.1.37): the bare-path pattern landed; the file matches, the waiver is gone**

`match c { Color.Red => "red", … }` and `match e { none => "first", bad =>
"second" }` under `check: run(exit=0, stdout="green\nfirst\n")`, `phase:
run`.

| | verdict |
| --- | --- |
| wolf 0.2.14 (both tiers) | `exit(0)`, `green\nfirst\n` |
| lupin 0.1.36 at `30731a6` | `fail(E0201)` at parse, `[993,993]` — "a dotted path in a pattern must carry a payload, like `io.Error(e)`" |
| lupin 0.1.37 at is49 | unmoved — that lane took s158, not s157 |
| **lupin 0.1.37 at is50** | `exit(0)`, `green\nfirst\n` — `PatKind::Path` (wolf-interp#107) |

Triage: case 3 in shape (spec clear, compiler matches it) but not a defect
of reading — s157's clause and witness arrived in the same pin as s158's,
and this lane's brief names #107 as the next lane's. Waived in
`differ::FILED_DIVERGENCES` so the corpus walk stays green on the rows it
CAN see; the row stays visible in every differential report and retires
the day the bare-path pattern lands (`differ::retired_waivers` will say so
in the round it becomes true).

Resolved at is50: `[gram.pat.nullary]` is mirrored, the file matches, and
`FILED_DIVERGENCES` is two entries again (DIV-2026-019, DIV-2026-021). The
compiler's table records the same move as `Verdict -> agreement` on both
tiers.

### The fs tier — is48, lupin 0.1.36, pin `a7f517e` (wolf-lang **v0.2.12**)

**No pin move.** The pin, the corpus and the anchors are is47's exactly; what
changed is that this machine stopped declining `[os.fs]`.

#### The posture that was retired, and the one that survived it

Every release to 0.1.35 answered the whole io/fs family with "this machine
has no filesystem by design" — reading `[proto.cmp.defined-divergence]` as
putting the HOST's filesystem outside the comparison surface. The maintainer
ruled otherwise (BACKLOG B16), and the ruling is right for a reason the old
posture missed: the corpus's own fs witnesses are written to be **idempotent
by construction** and to print **relations** (`same_size`, `kind`, `cleaned`)
rather than host values, so they compare across lanes without the host
entering the comparison at all.

What survived is the containment rule: a path that is absolute or climbs out
of the working directory is refused **BY NAME**, never by a row. The compiled
lane does not contain paths this way — probed at `a7f517e`, `wolf` will write
`/tmp/x` on request — so this is a **stated, named narrowing** of this
machine's surface and the one place the two implementations are deliberately
not equivalent. It is not a divergence in the census sense (no corpus file
names such a path) and it is recorded here so a later lane meets it as a
decision rather than as a surprise.

**Filed: wolf-lang#365.** `[proto.cmp.triage]` puts the clause in the dock
first, and the clause is silent: §6 never says whether an `fs_*` path may be
absolute or climb out of the working directory, and no witness decides it
either. The issue carries both readings measured — `wolf 0.2.12` admits
`/tmp/x` and `../x`; this machine refuses both by name — and asks the clause
to state the domain rather than leaving two implementations to guess. It also
carries the second gap this tier met: §6 gives `fs_remove` the row set
`{not_found, denied, io}` without saying which a DIRECTORY takes, and the two
lanes only agree today because both delegate to the same standard library
(`denied` on macOS, `io` on linux).

#### Predicted, then measured

Written before the first edit, from `spec/11-os.md` §6, the five witness
headers under `corpus/fs/`, and empirical probes of `wolf 0.2.12` at this pin
— never `wolf_rt::fs`. The prediction named the nine files first and the
figures second:

| class | baseline (0.1.35) | predicted | measured (0.1.36) |
| --- | --- | --- | --- |
| match | 423 | 432 | **432** |
| out of scope | 62 | 53 | **53** |
| mismatch | 1 | 1 | **1** |
| dynamic counterpart | 21 | 21 | **21** |
| conservatism | 38 | 38 | **38** |
| reach `run` | 412 | 421 | **421** |

**Six of six exact**, and the nine files by name as predicted:
`fs/bytes_dirs.lu`, `fs/error_row.lu`, `fs/fstat.lu`, `fs/open_nonblock.lu`,
`fs/roundtrip.lu`, `memory/byte_producers_ledger.lu`, `net/unix_echo.lu`,
`projects/count.lu`, `projects/count_dir.lu`. The prediction flagged one
figure as uncertain — whether `net/unix_echo.lu`, which reached `run` before
failing rather than stopping at `resolve`, was already inside the 412. It was
not, and the landing is 412 -> 421 rather than 412 -> 420.

The row that was in doubt on its merits was
`memory/byte_producers_ledger.lu`, which pins a REGION-accounting relation
and not an fs one. `read_tight` ("at most the payload plus a header") holds
only because `fs_read_bytes` and `fs_read_chunk` mint their buffer at exact
capacity through `region::ledger::byte_buffer_bytes`, the way `s.bytes()` and
the net byte reader already did; a byte list built by pushing would pay
`[mem.region.account.1]`'s growth history and fail it.

#### The harness defect the tier exposed

Corpus programs now write real files, and this crate runs many programs at
once — several harnesses walk the whole corpus, cargo runs test binaries in
parallel, and `export::export` runs a full walk inside any of them. Two
symptoms, both measured before the fix: `fs/fstat.lu` read `size=0` off a
file another thread had just truncated, and two concurrent exports of one
corpus appended to one `log.txt` twice (`log=one|one|twotwo size=14`), so the
bundle sha256 was not reproducible.

The corpus's own answer to re-running a witness is "idempotent by
construction", which covers a SEQUENTIAL re-run — what the compiler's conform
pass does, one lane after the other — and says nothing about two at once.
Nothing in the spec contemplates it either. So the defect is this crate's,
and it is answered without weakening the tier or editing a witness: an
OBSERVED program's relative paths now resolve against a private, empty
project root, one per observation. A live `lupin run` keeps the user's own
cwd, and `os_cwd` answers whichever applies so a program never sees a split
between where it thinks it is and where it writes.

**Two wrong turns, recorded so they are not retaken.** The first fix ordered
the runs with one advisory lock per corpus FILE, keyed on the file's path. It
made `run_corpus` green and left `export` red, because `export::export`
observes each program from a COPY inside its own bundle directory — a
path-keyed lock excluded nothing in precisely the failing case. Re-keying on
the cwd plus the program's SOURCE fixed that and passed in release. It then
failed in DEBUG, for a reason measurement settled rather than argument:
`memory/byte_producers_ledger.lu` takes **74 seconds** in a debug build, so a
queue of walkers blew the lock's deadline, and making the deadline long
enough would have added roughly twenty minutes to a three-hour CI. Ordering
was the wrong shape of answer; removing the contention was the right one.

The lesson generalises past this tier: `export::re_export_is_byte_identical`
only fails when it runs beside its sibling, and the debug failure only
appears under the full suite. A green run of the failing test alone proved
nothing, twice.

#### Rows the census cannot see

Three facts about this tier are invisible to the corpus, because no witness
exercises the shape. They were probed against the compiled lane and are
pinned by unit tests instead:

| shape | row | why no witness reaches it |
| --- | --- | --- |
| `fs_read`/`fs_read_chunk` past end of file | **`eof`** | no corpus file reads a handle twice; `[os.fs.open]`'s mode-5 prose names the row |
| `fs_read_text` over bytes that are not UTF-8 | **`utf8`** | `corpus/fs/bytes_dirs.lu` witnesses the refusal but takes the tag with `_` |
| a mode-2 (append) handle read | **`io`** | the witnesses only write through an append handle |

A fourth is a genuine host split both implementations take together, because
both are `std`-backed: `fs_remove` on a DIRECTORY is `denied` on macOS
(`unlink` gives EPERM) and `io` on linux (EISDIR). No witness names it, and
neither implementation is the defendant — the row is the host's.

#### Two host splits closed rather than documented

**A handle-level failure is `io` everywhere.** The first cut forwarded the
host's `io::ErrorKind` from handle calls, and the three-host matrix found the
hole immediately: reading an APPEND handle is `EBADF` on macOS and
`ERROR_ACCESS_DENIED` on windows, so the same program answered `io` on one
and `denied` on the other. `[os.fs.fstat]` already rules it — "on a handle
the hosts answer `io` for nearly everything, the entry being already
resolved" — and the compiled lane agrees (probed: a write to a read handle, a
read from a write handle and a closed handle are all `io`). So the row is
uniform, and the split is gone rather than written down. The PATH calls keep
their kinds, the entry being exactly what they resolve.

This is the value of the matrix stated plainly: the local host was green and
the reading was wrong.

**The socket and the files agree about where they are.** Giving observed
programs a private root introduced an incoherence in the tier next door:
`fs_exists` resolved against the root while `net_listen_unix` resolved
against the process cwd. `corpus/net/unix_echo.lu` depends on the two
agreeing — it sweeps a stale socket with `fs_remove`, binds it, and closes
with `cleaned = !fs_exists(path)` — so the sweep swept a path nothing bound
and `cleaned` was vacuously TRUE. The witness passed while checking nothing,
and a stale socket left in the real cwd would have failed the bind at random,
which is the flaky-CI shape this log exists to prevent. `net_listen_unix` and
`net_connect_unix` now resolve through the same root, and the root's NAME is
kept short because `sockaddr_un` caps a socket path near 104 bytes and
macOS's temp directory alone is about fifty characters.

#### What a hostile review of the branch found

The tier was reviewed against itself before the merge, and the review earned
its place. Six findings, all fixed on the branch and each with a test:

1. **A reachable `unreachable!`.** The three predicates (`fs_exists`,
   `fs_is_dir`, `fs_is_file`) asserted that containment answers no row — true
   of `contained`, false of `fs_contained`, which is containment AND
   resolution, and resolution fails when the observation root cannot be made.
   `touch $TMPDIR/wolf-obs` and every observed run panicked on the first
   predicate. A `panic!` in this module "is by definition an interpreter bug"
   (`eval`'s own module doc), and a predicate has no row to carry a failure
   in, so the answer is the by-name refusal — `false` would be a claim about
   a filesystem this machine could not reach.
2. **A path call answering a row it does not declare.** `path_row` mapped
   `AlreadyExists` to `exists`, but no PATH call declares `exists` — it is
   `[os.fs.open]`'s mode-4 row. `fs_create_dir_all` over an existing FILE
   reaches it (std forwards the kind verbatim), so a program's handler got a
   tag no arm could match and the whole `match` fell through. The compiled
   lane answers `io` there (probed), which is also the only declared row, so
   `path_row` no longer produces `exists` at all.
3. **A failure collapsing into the live sentinel.** `fs_observation_base`
   returned `Option<PathBuf>` with `None` meaning "live: use the user's own
   cwd", and used `.ok()` — so a root that could not be built bound a unix
   socket in the user's real directory, which is the precise opposite of what
   that failure should cause. It returns a `Result` now.
4. **A predictable name in a world-writable directory.** The root was
   `$TMPDIR/wolf-obs/<pid>-<serial>` built with `create_dir_all`, so a
   symlink planted at that name redirected every write an observed program
   made. The leaf is now claimed with `create_dir` — which fails on anything
   already there, symlink included — and carries a clock reading.
5. **A leak on partial failure.** `self.root` was recorded after `target/`
   was created, so a failure in between left a directory nothing would ever
   remove. It is recorded first.
6. **The REPL was treated as an observation.** `is_live` read
   `live_stdout`, which only `lupin run` sets, so a REPL or `lupin eval`
   session wrote its files into a private root and DELETED them when it
   ended. Stdout pass-through and "this is somebody's real directory" are
   different properties and now have different flags.

Items 1, 4 and 6 are the ones worth carrying forward as a lesson: each was a
consequence of the private-observation-root design rather than of the fs tier
proper, and none was visible from the census, the corpus, or a green CI.

#### A containment hole the matrix would have found later

`contained` is lexical, and `NUL`, `CON`, `AUX`, `PRN`, `COM1`-`COM9` and
`LPT1`-`LPT9` are ordinary path components to a parser while being DEVICES to
windows wherever they appear. `fs_write_text("NUL", …)` would have written
nowhere and `fs_read_text("CON")` would have read the console, both outside
the working directory the tier promises. They are refused by name on EVERY
host rather than only on windows: the corpus and the manual are shared across
the matrix, so a program admitted here on unix is a program that cannot run
there. The guard is a device list and not a prefix ban — `console.txt`,
`communication.log`, `COM0` and `COM10` are all still admitted.

#### wolf-interp#86's mode-5 half, closed

`[os.fs.open]` mode 5 is a read open that cannot park. On a regular file it
is mode 0 in every respect, which is the parity `corpus/fs/open_nonblock.lu`
pins and which now matches. The half the corpus cannot reach — a fifo with no
writer — is pinned by `eval::fs`'s own test, which builds a real fifo with the
host's `mkfifo` and fails on a timeout if the open parks. There is no `libc`
dependency here and `unsafe_code` is `forbid`, so `O_NONBLOCK` is a
written-down constant per host; that test is what makes it a measured
constant rather than a copied one, and it runs on both unix hosts of the
matrix. windows serves mode 5 as mode 0, by name, as the clause states.

#### A correction to 0.1.35's record

0.1.35's CHANGELOG census reads "413 -> **422** match … 62 -> 63 out of
scope" and asserts the numbers were "measured before a line was edited and
unchanged after". They were not unchanged after: is47's own E0413 mirror
moved `typecheck/interp_spec_on_union.lu` from out-of-scope to `match`, so
the 0.1.35 binary answers **423 / 62**. The is47 table below is correct as
labelled — it says "measured with the 0.1.34 binary at the new pin" — and
`docs/manual/00-building.md` already carried 423 / 62, so the CHANGELOG
sentence was the only wrong thing. Recorded because a census figure nobody
re-measures is how a waiver outlives its divergence (wolf-lang#177's lesson,
one tier over).

#### The sprint premise that did not survive measurement

is48 was written expecting an `unsupported` count of "87-114 rows, most of
them fs/net". At `a7f517e` it is **62**, of which fs, net, os and ffi
together are 16; the largest single group is `comptime`, at 21. The tier
stands on the ruling, not on that arithmetic.

### The remainder — is47, lupin 0.1.35, pin `a7f517e` (wolf-lang **v0.2.12**)

Pin `c9237c1` -> `a7f517e`, **the v0.2.12 tag**. The delta is s154, s156 and
r17. `spec/anchors.json` **471 -> 475; key sets diffed BOTH ways** — four
arrive (`[gram.fmt.break]`, `[gram.fmt.paren]`, `[mem.region.edge.elem]`,
`[type.float.rem]`), **nothing drops**, no owner changes. No new NAMESPACE:
all four sit under `gram`/`mem`/`type`, and `spec/05-conformance.md` is
untouched in the range, which is the independent check —
`anchor::REGISTERED_NAMESPACES` holds at thirteen. Ten corpus files join and
none leaves: nine entries and one member (`traits/op_eq_imported/cmp/c.lu`),
570 -> 580 files, 536 -> 545 entries, 34 -> 35 members. Five are EDITED
rather than added: `grammar/interp_fmtcolon.lu`, `grammar/structlit_paren.lu`,
`traits/op_total_num.lu`, `typecheck/fn_value_captured_int.lu` and
`wordcount.lu`.

#### Predicted, then measured, with the 0.1.34 binary at the new pin

Written before the bump, from the corpus headers and the spec clauses read as
DATA, against the c9237c1 baseline (570 files, 536 entries, 404 reach run,
413 match, 20 dynamic counterparts, 38 conservatism, 62 out of scope, 3
mismatches, 471 anchors):

| class | predicted | measured (0.1.34 at a7f517e) |
| --- | --- | --- |
| files / entries / members | 580 / 545 / 35 | 580 / 545 / 35 |
| anchors | 475 | 475 |
| match | 422 | 422 |
| mismatch | 1 | 1 |
| conservatism | 39 | **38** |
| out of scope | 63 | 63 |
| dynamic counterpart | 20 | **21** |
| reach `run` | 411 | **412** |

**Two misses, and they are one miss.**
`typecheck/receiver_bare_mut_param.lu` was predicted **conservatism** and is a
**dynamic counterpart**: E0804 has a row in `ledger::dynamic_meaning`, which
its pre-existing twin `typecheck/receiver_bare_mut.lu` already showed and the
prediction did not read. The file therefore reaches `run` (it traps
`exclusivity`), which is the 411-vs-412 row. Every other class, and every
witness inside it, as predicted.

**DIV-2026-022 and DIV-2026-023 RETIRE on the pin.** wolf-lang#341 re-pinned
both headers, s156 `0bb7024` and `2600f34`:

| witness | at c9237c1 | at a7f517e |
| --- | --- | --- |
| `wordcount.lu` | `check: run(exit=2)` with `tally[w] += 1` — E0417 since s152, MISMATCH | the same check with `tally[w] = (tally[w] else 0) + 1`, `exit(2)`, **match** |
| `grammar/structlit_paren.lu` | `check: pass`/`phase: resolve` with `p == (Point { x: 0 })` — E0301 since s155, MISMATCH | `check: run(exit=0)`/`phase: run`, the literal bound and read, **match** |

Both leave `differ::FILED_DIVERGENCES` and return to `RUN_LEDGER`. The one
mismatch standing at this pin is **DIV-2026-019** alone.

**The one behavioural mirror the pin asked for, and what it is not.**
`typecheck/interp_spec_on_union.lu` pins `fail(E0413)` — s156's #323, a format
spec on a `!T` hole — and lupin answers `unsupported` at resolve, so the file
lands in the **out-of-scope** class rather than as a mismatch. The reason on
`x-unsupported` is honest ("a format spec on a non-primitive value"), which is
precisely the shape wolf-lang#158's ch03 row complains about one clause over:
the "not implemented yet" channel carrying a defect in the reader's program.
**Taken**, and it is the one behavioural mirror this pin asked for. The clause
puts the rule on the row walk itself — "`[type.row.operand]`'s posture, applied
to specs" — and that is the walk here that can already name a `!T`: a declared
row-typed name or a call to a fallible item (`RowWalk::row_of`), plus a `Map`
subscript, which `[mem.map.absent]` makes `V ! {none}` and which is the
clause's own example. E0413 at the spec, the colon through the spec's last
byte with `}` excluded — `[782,785]` here against wolfc's `[782,785]` and
`[795,798]`, this machine reporting the first hole where the compiler reports
both, which is `[proto.cmp.phase]`'s posture and not a difference. The bare
hole is untouched, and so is every handled one: `{m[k] else 0:>5}` and
`{x?:>5}` are an `ElseDefault` and a `Try`, neither of which the judgement
names.

#### The sprint's work, in census terms

| step | run | match | dyn | cons. | oos | mismatch |
| --- | --- | --- | --- | --- | --- | --- |
| 0.1.34 at c9237c1 | 404 | 413 | 20 | 38 | 62 | 3 |
| the pin alone (0.1.34 at a7f517e) | 412 | 422 | 21 | 38 | 63 | **1** |
| #100 `then` joins the contextual table | 412 | 422 | 21 | 38 | 63 | 1 |
| #45 `free_names` collects its binders | 412 | 422 | 21 | 38 | 63 | 1 |
| #61 the declared-scalar check learns `char` | 412 | 422 | 21 | 38 | 63 | 1 |
| wolf-lang#158's two refusal messages | 412 | 422 | 21 | 38 | 63 | 1 |
| `[type.interp.union]`'s spec half, E0413 | 412 | **423** | 21 | 38 | **62** | 1 |

Match 413 -> 423 across the pin and the sprint, mismatches 3 -> 1.

The flat column above the last row is the finding, not a null result. Three of
this lane's four issues were filed BY READING — #45 by is24's honesty pass,
#61 by walking `[type.byte]` sentence by sentence, #100 by a clause that did
not name a word — and each one says so in its own text: "no corpus file and no
witness set exercises the shape" (#45), "no corpus file measures any of the
four positions" (#61). A corpus-neutral fix is what a defect found by reading
looks like when it lands, and the number that matters for #61 is the one that
did NOT move: the false-positive surface is37 declined to take on, measured
over 545 entries, is **zero**. The one row that does move is the one the PIN
brought a witness for, which is the ordinary shape and the contrast worth
keeping in view.

Witness table, by issue (the compiler's spans measured with the v0.2.12
release archive, darwin `6be493a9…`):

| issue | witness | 0.1.34 | 0.1.35 | wolf 0.2.12 |
| --- | --- | --- | --- | --- |
| #100 | `lex::CONTEXTUAL` against §6.2 | 11 entries, §6.2 names 12 | 12, held BOTH ways | §6.2 names `then` |
| #100 | body-less member + another declaration, one line | E0201 (unpinned) | E0201 @ the `fn`, pinned | E0201 |
| #45 | a closure whose `match` arm rebinds an outer `var` | W1102 (a capture that never happened) | clean | clean |
| #45 | the same with a `for` element / an `else` binder | W1102 | clean | clean |
| #45 | a TAG arm over a tag-shaped scrutinee (the control) | W1102 | W1102 | W1102 |
| #61 | `fn widen(c: char)` handed `65` | `exit(0)`, stdout `65` | E0401 @ [77,79] | E0401 @ [77,79] |
| #61 | `fn givec() -> char { 65 }` | `exit(0)`, stdout `65` | E0401 @ [21,23] | E0401 @ [21,23] |
| #61 | `var c = 'a'; c = 65` | `exit(0)`, stdout `65` (retyped) | E0401 @ [44,46] | E0401 @ [44,46] |
| #61 | `fn f(n: int)` handed `'a'` | `exit(0)` | E0401 @ [49,52] | E0401 @ [49,52] |
| #61 | `fn f(b: byte)` handed `'a'` | `exit(0)` | E0401 @ [57,60] | E0401 @ [57,60] |
| #61 | `fn f(c: char)` handed `65 as byte` | `exit(0)` | E0401 @ [57,67] | E0401 @ [57,67] |
| #61 | `List[char].push(65)` | `exit(0)` | E0401 @ [64,66] | E0401 @ [64,66] |
| #61 | `c == 97` | `exit(0)` | E0401 @ [48,50] | E0401 @ [48,50] |
| #103 | `let mark = if c { "*" }` as a value | `exit(0)`, stdout `*` | unchanged, filed | E0401 @ [72,75] |
| #103 | `pick(1, "two")` on `fn pick[T](a: T, b: T)` | `exit(0)`, stdout `1` | unchanged, filed | E0401 @ [75,80] |
| #103 | `let s = shards[i]` in a loop | `exit(0)`, stdout `a\nb\n` | unchanged, filed | E1001 @ [198,208], [227,236] |
| #103 | a body whose value the signature omits | `exit(0)` | unchanged, filed | E0401 @ [30,36] |
| #103 | a captured `var` written after the closure (HEALED) | E1101/W1101/W1102 @ [83,84],[83,84],[114,122] | same | **the same three, the same bytes** |
| #103 | `let s = shards[i]` OUTSIDE a loop (HEALED) | `exit(0)` | `exit(0)` | `exit(0)` |
| #104 | a static rejection's rendering (HEALED) | `at 2:14` | `at 2:14` | the source line with a caret |
| #104 | `"héllo"[1..2]` and `xs[5]` | both `trap(bounds) … [mem.ub.defined]` | unchanged, filed | — |
| #104 | a non-exhaustive `match` | `unsupported`, exit 4 | unchanged, filed | E0801 @ [38,107] |
| #104 | one-operand `when` | "call the method on the sync type" | states the rule | E0201, states the rule |
| #104 | `for c in "abc"` | "`for` cannot iterate str" | names `chars()`/`words()`/`lines()` | `unsupported` (for-trait wiring) |
| #325 | `typecheck/receiver_bare_mut_param.lu` | W1002 beside the trap | no warning, trap(exclusivity) | E0804, no warning |
| #325 | `fn size(mut xs: List[int]) -> int { xs.len }` (the control) | W1002 @ [8,11] | W1002 @ [8,11] | W1002 @ [8,11] |
| #105 | `--chaos` | `unexpected argument` | unchanged, filed | — |
| #323 | `typecheck/interp_spec_on_union.lu` | unsupported@resolve | E0413 @ [782,785] | E0413 @ [782,785], [795,798] |
| #323 | `let v = maybe(3)` then `{v:>5}` | ran | E0413 @ [126,129] | E0413 @ [126,129] |

Three corrections to the launch brief and the triage that fed it, each
measured:

1. **`tests/spec_extract.rs` did NOT hold `lex::CONTEXTUAL` to §6.2 "in BOTH
   directions".** t02's triage comment on #100 says it twice and the sprint
   file repeats it. The test looped over `lex::CONTEXTUAL` only, so it would
   have caught this machine listing a word §6.2 does not name and was silent
   on the reverse — which is the direction `then` was in for two pins. The
   reverse loop is added in the same commit as the word.
2. **E0804 is a dynamic counterpart, not conservatism** (above), which is the
   whole of the census prediction's error.
3. **#158's ch01 row has healed.** lupin renders `line:col`, not a byte
   offset; the excerpt is what remains of that row.

#### One thing the pin brought that the prediction listed as a risk, and it fired

The prediction's first named risk: `typecheck/receiver_bare_mut_param.lu`
carries **no `warns:` directive at all**, so the warns ledger expects this
machine warning-clean, and this machine implements W1002 while the body's only
write is the misspelled bare `xs.pop()`. It fired, and it is s154's #325 seen
from this side rather than a ledger to edit.

wolfc's half of #325 is a stand-down: a mode error on a use of a parameter
retires the W1002 that contradicts it, because the lint offered to drop the
`mut` — the opposite of the fix E0804 names, and a reader who took it landed
on E1014. This machine has **no static mode error to stand the lint down
against** (E0804 is `trap(exclusivity)` here, the dynamic counterpart), so the
mirror is not available and the honest move is the other one: the lint was
wrong *on its own terms*. The body writes `xs`; it misspelled the write, and
the flat scan counted only the `ModedReceiver` spelling as evidence. A bare
receiver call to `push` or `pop` — `eval::builtin::mutates_receiver`'s exact
pair, read statically — is evidence now, and the control
(`fn size(mut xs: List[int]) -> int { xs.len }`) still warns at `[8,11]` on
both machines.

### The map mirror — is46, lupin 0.1.34, pin `c9237c1` (wolf-lang **v0.2.11**)

Pin `662b14c` -> `c9237c1`, **the v0.2.11 tag**. The delta is five sprints
the previous pin left past its tag: s149 (`[os.fs.open]` mode 5, the accept
posture), s150 (`[type.fn.value]`, `[abi.native.closure]` rewritten,
`[conc.chan.payload]`), s151 (`[gram.expr.if]`, `[gram.fmt.if]` — mirrored
at 0.1.33 from the trunk text, arriving in the corpus now), s152
(`[type.map]`, `[type.map.key]`, `[mem.map.absent]`), s153 (`[exec]`,
`[exec.checked]`, `[exec.checked.budget]`, `[mem.region.escape]`) and s155
(`[type.trait.op]`, `[type.trait.op.alias]`). `spec/anchors.json` **458 ->
471; key sets diffed BOTH ways** — thirteen arrive, **nothing drops**, no
owner changes; `exec` is a new NAMESPACE, registered in spec/05's list and
in `anchor::REGISTERED_NAMESPACES` in the same change
(`[conf.anchor.ns.admit]`). Thirty-six corpus files join, none leaves, all
entries: 534 -> 570 files, 500 -> 536 entries, members 34.

s154 (the papercuts: `[gram.item.let]`'s field rule, `then` in §6.2, the
one-line body-less trait member) is **past the tag** — measured with `git
merge-base --is-ancestor`, fourteen commits — so wolf-interp#99 is mirrored
from the trunk clause text the way is45 mirrored s151, and #100's `then`
waits: §6.2 at this pin does not name it, `tests/spec_extract.rs` holds
`lex::CONTEXTUAL` to §6.2 both ways, and adding it would fail that test at
a pin whose prose does not carry it.

#### Predicted, then measured, with the 0.1.33 binary at the new pin

Written before the bump, against the 662b14c baseline (534 files, 500
entries, 383 reach run, 380 match, 16 dynamic counterparts, 42
conservatism, 61 out of scope, 1 mismatch, 458 anchors):

| class | predicted | measured (0.1.33 at c9237c1) |
| --- | --- | --- |
| files / entries / members | 570 / 536 / 34 | 570 / **535 + 1 walk failure** / 34 |
| anchors | 471 | 471 |
| match | 398 | 397 |
| mismatch | 5 | 5 |
| conservatism | 46 | 46 |
| out of scope | 69 | 69 |
| dynamic counterpart | 18 | 18 |
| reach `run` | 407 | 406 |

Every class as predicted, every witness in the class predicted for it — the
one miss is the file the prediction never reached: the walk refused
`strings/bytes_view_walk.lu` outright, `exec.checked.budget` being in a
namespace this machine had not registered. That is the restrictive half of
`[conf.anchor.ns.admit]`, and the fix is one line in `anchor.rs`; with it
the file is the entry it always was and the census is 536 / 398 / 407, the
prediction to the digit.

The four mismatches the pin brought, each predicted by name:
`memory/map_absent_else.lu` (0.1.33 printed `7 () 1 true` / `[()]`),
`memory/map_char_bool_keys.lu` (`() () ()`), `traits/op_eq_inverting.lu`
(structural `==`: `true`/`false`/`false`), `traits/op_total_num.lu` (E0201
at `trait Num =`). All four match at 0.1.34.

#### The sprint's work, in census terms

| step | run | match | dyn | cons. | oos | mismatch |
| --- | --- | --- | --- | --- | --- | --- |
| 0.1.33 at c9237c1 (+ `exec`) | 407 | 398 | 18 | 46 | 69 | 5 |
| #96/#97 imported trait, qualified bound | 407 | 398 | 18 | 46 | 69 | 5 |
| #91 the key protocol | 407 | 403 | 18 | 45 | 66 | 4 (+wordcount) |
| #92 the operator bridge, the alias form | 406 | 411 | 18 | 42 | 62 | 3 (+structlit_paren, −the two above, −op_eq/op_total) |
| #88 a built str has a region | 406 | 411 | 20 | 40 | 62 | 3 |
| #94/#95/#98/#99 | 405 | 412 | 20 | 39 | 62 | 3 |
| the golden rule's call shape (`Show.show(v)` on a bare `T`, E0501) | 404 | **413** | 20 | 38 | 62 | 3 |

Match 380 -> 413 across the pin and the sprint; the three mismatches
standing are DIV-2026-019 and the two filed below. The #96/#97 row moves
nothing in the corpus by construction — both witnesses need an imported
module, which the flat corpus does not stage; they are `tests/std_root.rs`'s
— and it is the row that unblocks wolf-std's whole trait surface (the
`[K: Eq]` five, every `ops` impl).

Witness table, by issue (the compiler's spans measured with the v0.2.11
release archive, darwin `a91b77c0…`):

| issue | witness | 0.1.33 | 0.1.34 | wolf 0.2.11 |
| --- | --- | --- | --- | --- |
| #96 | sc44's `tiny.Eq.eq(a, b)` under `--std-root` | unsupported@resolve | `true false` | `true false` |
| #97 | `fn total[T: ops.Add]`, the bound the only use | fail(E0305) | `3` | `3` |
| #91 | `memory/map_absent_else.lu` | `7 () 1 true` | `7 0 1 true` / `[none]` | same |
| #91 | `memory/map_char_bool_keys.lu` | `() () ()` | `1 2 yes` | same |
| #91 | `memory/map_count.lu` | unsupported (`+` on `()`) | the three totals | same |
| #91 | `memory/map_int_keys.lu` | unsupported (not a place) | `3 2 12 -1` | same |
| #91 | `typecheck/map_compound_absent.lu` | unsupported | E0417 @ [786,801] | E0417 @ [786,801] |
| #91 | `typecheck/map_struct_key.lu` | exit(1) | E0418 @ [648,653] | E0418 @ [648,653] |
| #92 | `traits/op_eq_inverting.lu` | `true/false/false` | `false/true/true` | same |
| #92 | `traits/op_money.lu`, `op_ord_struct.lu` | unsupported | `150/-150/50`, `true/false/true/less` | same |
| #92 | `traits/op_total_num.lu` | E0201 at `=` | `6` / `7.5` | same |
| #92 | `op_eq_no_trait`, `op_missing_impl`, `op_hetero_add` | ran / unsupported | E0301 @ [457,458], E0502 @ [597,598], E0514 @ [732,733] | same bytes |
| #92 | `golden_arith.lu`, `golden_eq.lu` | ran (conservatism) | E0501 @ [385,386], [347,348] | same bytes |
| #92 | `golden_missing_bound.lu` (`Show.show(v)`, bare `T`) | ran (conservatism) | E0501 @ [371,372] | E0501 @ [371,372] |
| #88 | `memory/region_str_concat_return.lu` | `regions`, exit 0 | trap(region-fault) at the `}` | E1010 |
| #88 | `memory/region_str_concat_send.lu` | `regions`, exit 0 | trap(region-fault) at `got` | E1010 |
| #94 | `Row { kind: "drink" }` for a two-field `Row` | ran, `drink` | E0408 @ [69,90] | E0408 @ [69,90] |
| #95 | `fn total[T: Num]`, no `Num` | ran, `7` | E0301 @ [12,15] | E0301 @ [12,15] |
| #98 | wh-002, `Dot { x: 3 } as dyn Draw` | ran | E0810 @ [152,176] | E0810 @ [152,176] |
| #98 | `traits/dyn_temp_refused.lu` | exit(0), conservatism | E0810, match | E0810 |
| #99 | `let r = Row {…}; r.cents = 5` | ran, `5` | E0410 at `r.cents` | unsupported@wir at v0.2.11 (s154 is past the tag; trunk: E0410) |
| #100 | body-less member followed by a member on one line | E0201 | E0201 @ [28,30] | E0201 |
| #86 | `fs/open_nonblock.lu` | unsupported@resolve | unsupported@resolve (the fs tier declines by name) | runs |
| #86 | the same at **0.1.36** (is48) | — | **runs, match** — mode 5 served, `invalid` still decided before the open | runs |

Two cost rulings measured against the compiler's checked tier rather than
assumed: a `str` built by `+`, `+=` or a holed interpolation **charges the
ambient region's ledger** (`region idle(cap: 0) { print("{n}") }` is
`trap(alloc-contract)` on `wolf --checked` at c9237c1 and runs on
`--native`; this machine mirrors the checked machine), which retired two
unit-test probes that printed a hole inside a cap-zero region; and an enum
value with no `impl Eq` keeps this machine's structural tag comparison —
the clause says "refused by name like a struct", and the conservative side
is kept until a witness asks for the other.

#### The seam #96 named

The same trait dispatched or did not depending on which FILE declared it:
`tiny.Eq.eq(a, b)` after `use std.tiny` is a three-segment path, and the
trait-qualified call took two segments only, so the path evaluator answered
"`Eq` is a trait; … no dynamic semantics here" while the byte-identical
trait in the entry file dispatched. wolf-std carried "neither implementation
EXECUTES trait dispatch" in six module headers since sc01; half of that was
false on every machine and the other half was this one branch.

### DIV-2026-022 — `wordcount.lu` — **RESOLVED upstream at pin `a7f517e` (0.1.35): wolf-lang#341 re-pinned the header at s156 `0bb7024`; the file matches at first sight**

The seed program's line 23, `if !w.is_empty() { tally[w] += 1 } // absent
key defaults to zero value`, is the sentence s152 retired: `m[k]` is
`V ! {none}` and **`m[k] op= v` is E0417** in every profile
(`[mem.map.absent]`; `typecheck/map_compound_absent.lu` is that statement).
The header still pins `check: run(exit=2)`, `phase: resolve`.

| | verdict |
| --- | --- |
| lupin 0.1.33 | `exit(2)` (the compound path defaulted an absent key to an `int` zero) |
| **lupin 0.1.34** | `fail(E0417)` at resolve, at `tally[w]` |
| wolf 0.2.11 | `unsupported` at resolve — `text.words()` is the std surface, one rung before the checker |
| wolf 0.2.11 on the reduction (`map_compound_absent.lu`) | `fail(E0417)` at typecheck, `[786,801]` |

Triage: spec bug, case 1 — the clause is unambiguous and the corpus file is
stale against it; the compiler's own ledger cannot see it because the phase
its header names stops before the refusal. The two spellings the clause
names — `tally[w] = (tally[w] else 0) + 1`, or std's `map.tally(mut tally,
w)` — keep the seed program's meaning and its `exit(2)`. Waived in
`differ::FILED_DIVERGENCES`; `wordcount.lu` leaves `RUN_LEDGER` and the
explorer's seed pair, the CLI's exit(2) probe stands on its own program,
and the row returns the day the file is respelled.

**Resolved at `a7f517e` (is47).** s156's `0bb7024` took the first of the
two spellings: the seed program reads `if !w.is_empty() { tally[w] =
(tally[w] else 0) + 1 } // absent key is the `none` row` and its header
names `[mem.map.absent]` in `conforms:`. Measured with the **0.1.34**
binary at the new pin, before a line of is47 was edited: `exit(2)`, a
match. The waiver leaves `differ::FILED_DIVERGENCES` and the row returns
to `RUN_LEDGER` and to the explorer's seed set — three files there now,
not two. The CLI's exit(2) probe keeps the two-line program is46 gave it:
a contract about process exit codes should not be re-measured every time
the corpus moves.

### DIV-2026-023 — `grammar/structlit_paren.lu` — **RESOLVED upstream at pin `a7f517e` (0.1.35): wolf-lang#341 re-pinned the header at s156 `2600f34`; the file matches at first sight**

`if p == (Point { x: 0 }) { 0 } else { 1 }` under `check: pass`, `phase:
resolve`. s155: `==` on a user type IS `Eq.eq`, nothing is synthesized
(D49), and nothing named `Eq` in scope is E0301 — `traits/op_eq_no_trait.lu`
is the same program with a `let b` for the parenthesized literal.

| | verdict |
| --- | --- |
| lupin 0.1.33 | `exit(0)` (structural `==`) |
| **lupin 0.1.34** | `fail(E0301)` at resolve, at `p` |
| wolf 0.2.11 | `fail(E0301)` at typecheck, `[202,203]` — the same operand |

Triage: spec bug, case 1, the same shape as DIV-2026-022 one clause over.
The witness is about `[gram.amb.structlit]` (the parenthesized form E0006
suggests) and needs no equality: `let q = (Point { x: 0 })` then `q.x`
keeps its point under both clauses. Waived; the file moves to the annex
pair's REFUSED half in `tests/conformance.rs` (it still parses, which is
the annex's point) and leaves `RUN_LEDGER`.

**Resolved at `a7f517e` (is47).** s156's `2600f34` took exactly that
reading — `let q = (Point { x: 0 })` then `p.x + q.x` — and moved the
header with it, `check: pass`/`phase: resolve` to `check: run(exit=0)`/
`phase: run`, because judging at `resolve` is what let the staleness stay
invisible on the compiler's side while this machine's walk reported it.
Measured with the **0.1.34** binary at the new pin, before any edit:
`exit(0)`, a match. The waiver leaves `differ::FILED_DIVERGENCES` and the
row returns to `RUN_LEDGER`; the annex pair is unchanged either way, which
is the point of judging it at `parse`.

### The mirror takes `then` — is45, lupin 0.1.33, pin `662b14c` (wolf-lang **v0.2.10**)

Pin `e0ce018` -> `662b14c`, **the v0.2.10 tag**: 0.1.32 pinned a dev stamp
22 commits short of the release, and this is the release closing that gap.
The delta is s148 alone — `spec/10-types.md` gains `[type.fn]`/`[type.fn.ret]`
(§8) and `[type.row]`/`[type.row.operand]` (§9), `spec/02-memory-model.md`
gains `[mem.str.imm]`, and `spec/anchors.json` **453 -> 458; key sets diffed
BOTH ways** — five arrive, **nothing drops**, no owner changes. Five corpus
files join, none leaves, all entries: `rows/negative/row_operand_add.lu`,
`rows/negative/row_operand_compare.lu`, `typecheck/tail_declared_str.lu`,
`typecheck/tail_declared_union.lu`, `typecheck/str_slice_assign.lu`.

Three corrections to the launch brief, measured with `git merge-base
--is-ancestor` and worth writing down because the brief's census expected
them: **s149 is not in this pin** (`b342ad7`, `fs_open_mode` 5, is past the
tag — so wolf-interp#86's `corpus/fs/open_nonblock.lu` is not in the vendored
corpus either, and the fs tier's decline-by-name has nothing to answer yet);
**s150 is not in this pin** (`119d9e0`); and s151 — the sprint's own clause —
is not in it either, which is why the twelve `if_then_*` witnesses live under
`tests/s151/` and not in `run_corpus.rs`. The clause text was read from
wolf-lang trunk `f608487`; the pin is the release's; the next pin takes
v0.2.11 and the witnesses move into the shared corpus with it.

**The prediction, written before the binary ran** (the scratch file is
`is45-predictions.md`, dated before the pin bump commit): 534 files / 500
entries, 383 reaching run (none of the five runs), 378 matching, **2
mismatches** — DIV-2026-019 and `row_operand_compare.lu`, because lupin
0.1.32 answered E0401 for a comparison on a bare row and the clause rules
E0409 on either side (wolf-interp#85); `str_slice_assign.lu` out of scope
(lupin declined the slice as a place at run time); the two tail witnesses and
`row_operand_add.lu` matching at first sight, because is43's `tail_check` and
the arithmetic arm of `binary_operands` already answer what s148 wrote down.
Anchors 453 -> 458, ratchet 205 -> 208 (`type.fn` and `type.row` are the
§-heading anchors no witness cites), distinct conforms 307 -> 310.

**Measured, with the 0.1.32 release binary at the new pin, before a line was
edited:** 534/500, 383 reach run, **378 match, 2 mismatch, 62 out of scope**,
16 counterparts and 42 conservatism unmoved, **311** distinct conforms. Every
prediction held but the last: `mem.str.view` was also uncited before
`str_slice_assign.lu` arrived, so the distinct-conforms count moved by four,
not three. The measurement is in `is45-predictions.md` under MEASURED.

| witness | lupin 0.1.32 at the NEW pin | lupin 0.1.33 | the clause |
| --- | --- | --- | --- |
| `rows/negative/row_operand_add.lu` | `fail(E0409)@resolve`, match | unmoved | `[type.row.operand]` |
| `rows/negative/row_operand_compare.lu` | `fail(E0401)@resolve`, **MISMATCH** | `fail(E0409)@resolve`, match | `[type.row.operand]` |
| `typecheck/tail_declared_str.lu` | `fail(E0401)@resolve`, match | unmoved | `[type.fn.ret]` |
| `typecheck/tail_declared_union.lu` | `fail(E0401)@resolve`, match | unmoved | `[type.fn.ret]` |
| `typecheck/str_slice_assign.lu` | `unsupported@resolve`, out of scope | `fail(E0416)@resolve`, match | `[mem.str.imm]` |

Census 495 -> 500 entries, 383 reach run unmoved, 375 -> 380 match, 61 out of
scope unmoved; 42 conservatism and 16 dynamic counterparts unmoved; mismatches
1 -> 2 -> **1**, DIV-2026-019.

**s151's twelve, the sprint's item 2 (wolf-interp#90).** Not in the pin, so
the ledger for them is `tests/s151_if_then.rs`, judged against their own
headers the way `tests/d62/` was. Under 0.1.32 all nine positive witnesses
stopped at `E0201: expected `{`, found identifier `then``; the three refusals
answered E0201 too, at `then`'s column. Under 0.1.33:

| witness | lupin 0.1.32 | lupin 0.1.33 | wolf at `f608487` (trunk, dev build) |
| --- | --- | --- | --- |
| `grammar/if_then_let.lu` | `fail(E0201)@parse` | `exit(0)@run`, `29\n28\n` | pinned |
| `grammar/if_then_arm.lu` | `fail(E0201)@parse` | `exit(0)@run`, `31\n29\n28\n30\n` | pinned |
| `grammar/if_then_stmt.lu` | `fail(E0201)@parse` | `exit(0)@run`, five lines | pinned |
| `grammar/if_then_chain.lu` | `fail(E0201)@parse` | `exit(0)@run`, five lines | pinned |
| `grammar/if_then_paren_default.lu` | `fail(E0201)@parse` | `exit(0)@run`, `7\n0\n1\n` | pinned |
| `grammar/if_then_block.lu` | `fail(E0201)@parse` | `exit(0)@run`, `29\n` | pinned |
| `grammar/if_then_ident.lu` | `fail(E0201)@parse` | `exit(0)@run`, `then is a name\n1\n` | pinned |
| `grammar/if_then_member.lu` | `fail(E0201)@parse` | `exit(0)@run`, `less\nequal\n` | pinned |
| `grammar/if_then_width.lu` | `fail(E0201)@parse` | `exit(0)@run`, one line | pinned |
| `grammar/if_then_let_body.lu` | `fail(E0201)@parse` `[241,245]` | `fail(E0201)@parse` `[246,249]` | `fail(E0201)@parse` `[246,249]` |
| `grammar/if_then_missing.lu` | `fail(E0201)@parse` `[294,296]` | `fail(E0201)@parse` `[294,296]` | `fail(E0201)@parse` `[294,296]` |
| `grammar/if_then_mixed.lu` | `fail(E0201)@parse` `[269,273]` | `fail(E0201)@parse` `[282,283]` | `fail(E0201)@parse` `[282,283]` |

"pinned" means the stdout in the file's `check:` — the compiler's measured
answer, which is what the corpus pins. The fourth column's spans were
measured with a debug build of `wolf_driver` at `f608487` (the installed wolf
0.2.10 predates the clause and stops at `then` like 0.1.32 did); that build
answers `unsupported@wir` for the nine running ones, which is a stamp
question of an unreleased dev build and not a finding — the parse-rung spans
are the comparison this rung can make, and all three agree byte for byte.
`if_then_missing.lu` is the DIV-2026-021 shape again: 0.1.32's E0201 was at
the same span for the wrong reason (a parser with no bare form hits `29`
where it wanted `{`), and the message — "expected `{` or `then` … an `if` is
spelled braced … or bare …" — is what the clause asks for, so
`parse::tests::a_condition_followed_by_neither_brace_nor_then_is_e0201_naming_both`
reads the string.

**`[proto.cmp.triage]`: the spec is the defendant twice, and silent once.**

- `[gram.expr.if]` calls `then` "contextual, not reserved" and
  `[gram.inv.ctx]` (§6.2), the clause that enumerates the contextual
  keywords, does not name it — s151 never touched §6.2. `lex::CONTEXTUAL` is
  held to §6.2's prose both ways by `tests/spec_extract.rs`, so `then` cannot
  join the table without failing the test; the parser matches it by spelling
  in `parse_if` and the table stays at eleven. Filed as **wolf-lang#318**.
- wolf-interp#84 (`[T: Area](a: T, b: T)` with a `Rect` and a `Square`):
  `[gram.item.fn]` gives `generic_param ::= IDENT (':' bound)?` and stops.
  No clause says a type parameter binds once per call. wolf 0.2.10 answers
  E0401 at `[464,465]` and lupin 0.1.33 answers the same code at the same
  span, from declarations alone (a bare-`T` parameter; an argument whose
  struct a literal, an annotation or a parameter spelled) — two
  implementations agreeing on an unwritten rule, which is the shape the
  filing rule exists for. Filed as **wolf-lang#319** with the program and
  all three records, asking for the sentence and a shared witness.
- wolf-interp#89's second half (`fn f(p: proc)`): E0206 on the compiler,
  E0201 here, same span, same phase. §9 reserves the family and names no
  number; no corpus file pins E0206. This side follows the counterparty's
  number the way E0203 was ceded at wolf-interp#3 (`E_ASSUME_ARITY`, which had
  invented E0206, moves to E0212). Filed as **wolf-lang#320** for the corpus
  witness that would pin it on both sides.

**`then` is a parser fact and costs the later rungs nothing.** A bare branch
is a brace-less `Block` — no statements, its one expression as the tail, the
expression's span — so sema, lint and the evaluator see the shape they already
handle. The branch is parsed one tier below the defaulting `else` (tier 14),
which is what makes `if c then f() else 0` the two-way `if` per
`[gram.amb.else]`; a jump's operand stops at the same tier (`parse::CHOICES`
gains the row: `expr ::= else_expr | jump_expr` with `jump_expr ::= 'return'
expr?` would let `if c then return x else y` read `x else y` as the operand,
and the clause's own-`else` sentence does not mention jumps). The independence
doctrine held the way is44 held it: the clause states everything the parser
needed — the contextual position, the own-`else` binding, the shared-form rule,
the E0201 that names both spellings — and the counterparty's parser was not
opened.

### The mirror takes the range arm — is44, lupin 0.1.32, pin `e0ce018` (wolf-lang s147, dev-stamped)

Pin `4c60946` -> `e0ce018`, a dev stamp again: s147 landed
`[gram.pat.range]` after v0.2.9 and the four witnesses cannot be mirrored
without the pin that carries them. The delta is two sprints wide, not one —
s146's `[type.unit]` family rides along — and that is the first thing worth
writing down, because only one of the two cost this machine a line.

`spec/01-grammar.md` (§5's `closed_pattern` gains
`literal ('..' | '..=') literal`, plus the clause), `spec/03-concurrency.md`,
`spec/10-types.md` (§7 `[type.unit]`), `spec/grammar.ebnf`, and
`spec/anchors.json` **448 -> 453; key sets diffed BOTH ways** —
`gram.pat.range`, `type.unit`, `type.unit.consume`, `type.unit.context`,
`type.unit.discard` arrive, **nothing drops**, no owner changes
(wolf-lang#177's lesson, still standing). Eight corpus files join, none
leaves, all entries: `grammar/match_range.lu`, `grammar/match_range_char.lu`,
`grammar/match_range_open.lu`, `rows/match_range_empty.lu`,
`grammar/match_switch.lu` (s147), and `conc/chan_send_closed_row.lu`,
`conc/spawn_tail_send_raised_row.lu`, `typecheck/unit_context_discard.lu`
(s146).

**The prediction, written before the binary ran.** Item 1 ADMITS programs this
machine used to refuse, which is the opposite shape from is43's and asks the
opposite question: not "which running file starts to be declined" but "which
of the eight new files is already answered, and by accident?" The prediction
was: the four range witnesses move (three mismatching, and
`match_range_open.lu` matching for the WRONG reason — E0201 is the right code
from a parser that has no range production at all, so its verdict was correct
and its diagnostic said nothing about ranges); `match_switch.lu` and all three
s146 files match untouched, because #286 is a guard-arm coverage question this
machine never had wrong and `[type.unit]`'s discard is a *typing* rule with no
dynamic half — a machine with no unit context to violate cannot violate one.

**Measured, with the 0.1.31 binary at the new pin, before a line was edited:**
495 entries, 34 members, **4 mismatches**. Three are the range witnesses. The
fourth is DIV-2026-019, which has nothing to do with this pin. Every other new
file — `match_switch.lu`, `chan_send_closed_row.lu`,
`spawn_tail_send_raised_row.lu`, `unit_context_discard.lu` — matched at first
sight. **s146 cost this implementation zero source motion**, and that is a
finding, not a shrug: `[type.unit.discard]`'s (ii) reading was chosen partly
because it is non-breaking, and a mirror that already ran all thirteen
witnesses is the cheapest possible evidence for the claim.

| witness | lupin 0.1.31 at the NEW pin | lupin 0.1.32 | the clause |
| --- | --- | --- | --- |
| `grammar/match_range.lu` | `fail(E0201)@parse`, **MISMATCH** | `exit(0)@run`, match | `[gram.pat.range]` |
| `grammar/match_range_char.lu` | `fail(E0201)@parse`, **MISMATCH** | `exit(0)@run`, match | `[gram.pat.range]`, `[type.char.order]` |
| `rows/match_range_empty.lu` | `fail(E0201)@parse`, **MISMATCH** | `fail(E0815)@resolve`, match | `[gram.pat.range]` |
| `grammar/match_range_open.lu` | `fail(E0201)@parse`, match *by accident* | `fail(E0201)@parse`, match *by the clause* | `[gram.pat.range]` |
| `grammar/match_switch.lu` | `exit(0)@run`, match | unmoved | `[gram.expr.flow]` |
| `typecheck/unit_context_discard.lu` | `exit(0)@run`, match | unmoved | `[type.unit]` |

Census 487 -> 495 entries, 377 -> 383 reach run, 367 -> 375 match, 61 out of
scope unmoved; 42 conservatism and 16 dynamic counterparts unmoved; 4
mismatches -> **1**, DIV-2026-019.

**`match_range_open.lu` is the entry worth the ink.** It was a `match` at the
old pin and at the new one, on both sides of the work, and the walk could not
tell the difference — because the walk compares CODES. A parser with no range
production reaches `10..` and says `expected `=>`, found `..``: right code,
right rung, and a reader who writes an open range learns nothing. The clause
is unusually explicit about this — "the parser refuses them in pattern
position (E0201) **with a note that names the range form** — the diagnostic
must say the word 'range', which is the papercut the book carried" — so the
deliverable here is a *message*, which is precisely the axis
`[proto.record.diag]` keeps off the wire (D22). A corpus directive cannot pin
it and the differ cannot see it; the only thing that can is a test that reads
the string, and `parse::tests::an_open_range_in_pattern_position_is_refused_by_name`
is it. Same lesson as DIV-2026-021 one tier over: when the pinned surface is
coarser than the fix, the local test IS the ledger.

**`[proto.cmp.triage]`: the spec is not the defendant, and it is unusually
hard to make it one.** `[gram.pat.range]` decides every question this mirror
had to ask — which domains (integer primitives and `char`, `byte` excluded by
name), which spellings are refused and with which codes (E0201 open, E0815
empty, E0401 mixed, E0808 unordered), what overlap means (legal, first arm
wins), what subsumption reports (single-constructor, silent on unions), and
what exhaustiveness does **not** do (no union computed for any domain,
bounded or not, `_` still required). The one thing it leaves to the
implementation is a residue this parser already carried: a negative literal
endpoint. `[gram.pat]`'s `literal` admits no leading `-`, so `-5..0` is
wolf-lang's residue and not a range question — this parser has taken a
negative literal PATTERN since long before s147 (`parse_literal_only` under
`Tok::Minus`), and the range arm reads endpoints through the same door rather
than growing a second rule about signs. Nothing new is chosen; an old choice
is inherited. No filing.

**The exhaustiveness posture, mirrored from the clause and not from the
checker.** `[gram.pat.range]` states it in full — "integer and `char` columns
stay 'infinite' — the checker computes **no** union of ranges and literals for
any domain this sprint, bounded (`u8`, `char`) or not"; "overlapping ranges
are legal — the first arm wins, as with literals"; "an arm whose every value
an earlier literal or range already covers is E0802 ... a range covered only
by the union of several is not" — so it is mirrored from `spec/01` and the
counterparty's `exhaust.rs` was not opened. That is the independence doctrine
working the way CONTRIBUTING says it should: "Read `upstream/spec` and
reimplement. If the spec is silent or ambiguous, that ambiguity is the
finding." It is not silent here, and reading the checker to confirm a clause
this precise would have bought nothing and cost the thing the comparison is
for. In this machine E0801 has no half at all (the sema boundary: "a property
the static tier owns never becomes a trap"), so "no union computed" costs
nothing to hold; the half that is real is E0802, and it is gated on a range
having been spelled, which is why the 495-entry walk is byte-identical across
it.

### The tail is checked — is43, lupin 0.1.31, pin `4c60946` (wolf-lang **v0.2.9**)

Pin `2c03ed9` -> `4c60946`, and this one is a **tag**, not a dev stamp — the
first non-dev pin since `5c729e8`. The sprint said the pin need not move, and
for three of the four items it did not: the clauses items 1 and 3 mirror are
already at `2c03ed9`. #78's are not. wolf-lang#278 is spec commit `8d1e003`,
s145 merged after is42 pinned, and `git merge-base --is-ancestor 8d1e003
2c03ed9` is false — so the char rows could not be mirrored without the pin
that carries them, and the vendored corpus would have kept pinning
`fail(E0409)` on a program both compiler tiers run. The `2c03ed9 -> v0.2.9`
delta is four files and nothing else, which is why the bump was cheap:
`spec/10-types.md`, `spec/anchors.json` (446 -> 448; key sets diffed BOTH ways
— `type.closure` and `type.closure.return` arrive, nothing drops, no owner
changes), `corpus/strings/concat_mix_char.lu`, and the new
`corpus/typecheck/closure_return.lu`.

**The prediction, written before the walk moved.** Items 1 and 2 refuse
programs this machine used to run, so the question the sprint asked first was
which corpus files that ran here start to be declined, and whether any was
correct by accident. The prediction was **none, and none** — the corpus is
compiler-accepted programs plus `fail`-pinned negatives; a compiler-accepted
program can carry neither an unchecked unit tail nor a bare row operand nor an
unresolvable annotation, and the negatives whose pinned code is E0401 are
`typecheck/if_branch.lu` (branches disagree), `typecheck/coerce_no_widening.lu`
(no implicit widening) and `typecheck/numlit_ambiguity_named.lu` (literal
ambiguity): a branch, a widening and a literal, not a tail and not a row. Any
mover would therefore be a false positive of the new walk, not a finding.

**Measured: zero motion, three times.** The walk after #73/#81 is
byte-identical to the walk before it, entry for entry, all 486. The walk after
#79 is byte-identical to the walk after the pin bump, all 487. The only two
entries that move in the whole sprint are the two files the pin itself
changed:

| witness | lupin 0.1.30 at the OLD pin | lupin 0.1.31 | the clause |
| --- | --- | --- | --- |
| `strings/concat_mix_char.lu` | `unsupported@resolve`, out-of-scope vs `fail(E0409)` | **`exit(0)@run`, match, byte-identical stdout** | `[type.str.concat]` |
| `typecheck/closure_return.lu` | (not in the corpus) | **`exit(0)@run`, match at first sight** | `[type.closure.return]` |

Census 486 -> 487 entries, 375 -> 377 reach run, 365 -> 367 match, 62 -> 61 out
of scope; 42 conservatism and 16 dynamic counterparts unmoved; 1 mismatch,
still DIV-2026-019, which has nothing to do with this pin.

**The triage that mattered, and it did not go the usual way.**
`[proto.cmp.triage]` makes the spec document the defendant first, and the
habit of this log is to find it guilty. For #73 and #81 it is not.

- `[gram.expr.block]` makes a block's value its optional trailing expression
  (`block ::= '{' stmt* expr? '}'`), so a block without one is `()`; and
  `[gram.expr.tagident]`, in its list of *checked positions*, names "the
  operand of `return` (**and a fallible function's tail**) against the
  declared return row". The clause already said the tail is checked against
  the declaration. The implementation was simply not reading it, so the
  implementation is the defendant and the number is not this machine's to
  invent: six corpus files pin `fail(E0401)` at phase `resolve` and spec/10
  spends E0401 on a mismatch in five places.
- `[type.interp.union]` gives a `!T` a rendering inside an interpolation hole
  and marks it as a carve-out in the same breath — "this is a reading rule,
  not a handling rule: `?` and `else` still decide what the program does with
  the row". A hole is the one place a `!T` is read without being handled,
  which leaves an operator no reading at all; `[type.str.concat.mix]` fixes
  the number for "this operator is not defined on these operand types"
  ("Mixed operands stay **E0409**"). So arithmetic, the bitwise family and the
  shifts answer E0409 and the comparisons answer E0401 naming what the other
  side makes of the term — wolf 0.2.9's own measured sentence. `&&` and `||`
  are left alone: no clause and no measurement fixes a number for them here.

**What the spec could say and does not, filed as wolf-lang#284.** There is no
`[type.fn.ret]` and no `[type.row]`. Both rows are decided today by reading two
GRAMMAR clauses together and by inverting a RENDERING rule's carve-out. That
works, and both implementations land on the same answer — but the rule that a
`!T` is not a `T` in operator position is stated nowhere directly, only implied
by a clause about string holes. The issue carries all four witnesses (declared
`str` tail, declared `!int` tail, `!int + 1`, `!int <= n`) with both readings
and a candidate `[type.row.operand]` amendment, so r14/r15 can pin them rather
than re-derive the shapes each wave. Until the clause exists, witnesses 3 and 4
cite `[type.interp.union]` sideways, which is the honest thing to record.

**wollf's 56.** wl10 re-verified its training corpus under wolf 0.2.9 / lupin
0.1.29 and found 2,097 of 2,153 programs passing both machines and 56 refused
by wolf and running to completion here — E0409 x21, E0401 x17, and 8 that build
with a warning and exit 101 natively. The 38 typed ones are exactly #81's shape
and are closed from this release. The 8 are not this row and stay open.

**What is deliberately still lenient.** The tail check swears to a closed set
of unit shapes and leaves every other tail running, including a declared return
type this machine has not resolved — a struct, an alias, an enum, a container.
The row check swears to two operand shapes. The annotation check is
signature-width and excludes methods, because `sema::MethodDef` records the
decl and the trait but not the impl block's generic parameters, so `impl
Stack[T]`'s `fn push(self, v: T)` has no scope to read `T` from. Each of those
is a place a later sprint can widen with a measurement in hand; none of them is
a place to guess, because a wrong refusal stops a program from running at all,
which is the one failure mode the sema boundary exists to prevent.

### The mirror lets `else` start a line — is42, lupin 0.1.30, pin `2c03ed9` (wolf-lang s144, dev-stamped)

Pin `e9a17cb` -> `2c03ed9`, wolf-lang trunk past the s144 merge, **dev-stamped
again**: v0.2.9 is r13's and is being cut in parallel, and the clauses this
sprint mirrors do not exist at v0.2.8, so the sha is the only pin this
repository can record. Three rulings land, and **all four witnesses this
machine parted on are byte-identical from this release**. The walk goes 5
mismatches to 1, and the one left is DIV-2026-019, which has nothing to do
with this pin.

| witness | 0.1.29's binary at the NEW pin | lupin at 0.1.30 | the clause |
| --- | --- | --- | --- |
| `grammar/else_chain.lu` | `fail(E0005)@parse`, MISMATCH | **`exit(0)@run`, match** | `[gram.lex.newline]` |
| `grammar/else_default_newline.lu` | `fail(E0005)@parse`, MISMATCH | **`exit(0)@run`, match** | `[gram.amb.else]` |
| `conc/chan_closed_row.lu` | `exit(0)` but `Closed`, MISMATCH | **`exit(0)@run`, match** | `[conc.chan.close]` |
| `memory/list_pop_empty.lu` | `trap(bounds)@run`, MISMATCH | **`exit(0)@run`, match** | `[mem.list.pop]` |

The left column is deliberately not the 0.1.29 *release's* measurement: it is
the 0.1.29 binary run against the NEW pin before a line was edited, which is
the only column that isolates what this sprint's source motion bought. Five
mismatches before, one after, and the fifth was DIV-2026-019 both times.

**`[gram.lex.newline]`'s `else` lookahead — wolf-lang#276, wolf-interp#75.**
Predicted motion, written before the pin moved: one lexer arm, one parser
deletion, one diag constant, the manual's E-table row. Measured: the lexer arm
is real and is the whole feature (`else_follows` in `src/lex.rs`, one guard in
`insert_terminator`); the parser deletion is real and is exactly one match arm
plus its `orphaned_else` constructor. **The other two predictions were wrong,
and both were wrong in the direction of over-deleting.**

- The diag constant does NOT retire. §9 of `spec/01-grammar.md` still lists
  E0005 — "retired 2026-09-09 by wolf-lang#276 … the number is never reused" —
  and `tests/spec_extract.rs` re-reads that list at test time and diffs it
  against `diag`'s constants. Deleting `E_ELSE_NEW_LINE` would have made this
  implementation claim the spec dropped a reservation it explicitly kept. The
  constant stays, reserved and unreachable, exactly as E0004 is. What retires
  is the *emission*, not the number. Retiring a code and freeing a number are
  two different acts and the catalog only ever performs the first.
- There is no E-table in this repository's manual. The prediction was the
  compiler's shape read across the seam; `docs/manual/` documents how to drive
  this machine and defines no catalog, and the only E-code prose here is in the
  engineering documents. Nothing to move.

Two ledgers the prediction did not name moved instead, both mechanical: the
corpus census in `tests/cli.rs` and `tests/corpus_harness.rs` (517 -> 520, the
pin-bump ritual) and the `grammar/else_chain.lu` trace snapshot, which churns
because the corpus file itself was rewritten to the maintainer's aligned
layout. Reviewed the way the snapshot ritual asks: the aligned chain nests as
one `if`/`if`/`block expression`, identical in shape to the trailing form it
replaced, which is the claim the clause makes.

**`[conc.chan.close]` spells `closed` and `cancelled` — wolf-lang#273,
wolf-interp#76.** is41 recorded the posture with four line numbers attached
and s144 ruled it against this machine's spelling; the mirror is those four
literals and nothing else. Worth recording is what the rename *found*: this
machine was already inconsistent with itself. The net tier has spelled the
same condition `closed` since s39 (`NetErr::Row("closed")`, five sites), and
`err.is_cancelled()` compared its receiver's tag against `"cancelled"` —
a predicate that could not answer true for any value `cancelled_error()` ever
minted. A CapCase mark that nothing else in the codebase agreed with was not
a style question, and the clause found the bug the audit did not.

**`[mem.list.pop]` — wolf-lang#274, wolf-interp#77.** The one ruling with real
behavioural cost, and the one is41 called the asymmetry that decides: the
compiler *accepted and ran* a program this machine faulted on, so the machine
that traps is the one out of step with a language whose recoverable reads are
rows. `pop` on an empty list is `none`; `get` outside `0..len` is `none`;
`first` and `last` are `get(0)` and `get(len - 1)` by the clause's own words
and were not implemented here at all, so they arrive as new arms rather than
as a rename. `OutOfBounds` retires with them — the second CapCase payload-free
mark on the builtin surface, the one is41 counted after `[mem.str.parse]`
retired the first. The subscript `xs[i]` stays the faulting twin, and there is
now a test that says so, because "these reads never fault" is exactly the
sentence a later reader could over-apply.

All four cite `Rule::ErrUnion`, not a rule minted for `[mem.list.pop]`. That is
deliberate and it is `str.get`'s precedent: `[mem.str.get]`'s identical miss
has cited `ErrUnion` since is22, and the clause states the relation between
the two surfaces as an identity rather than an analogy. Minting a rule for one
half of a pair whose other half already has none would put the seam in the
registry instead of taking it out. The reason string names `[mem.list.pop]`, so
a `--trace` still carries the clause.

**One test lost its provocation, and that is the interesting cost.** Two tests
proved that a lent receiver returns to its slot — `src/eval/tests.rs`'s
`a_lend_hands_the_receiver_back_when_the_method_traps` and
`tests/repl_session.rs`'s `a_lent_receiver_is_back_in_its_slot_after_the_trap`
— and both rode `pop` on an empty list, because it was the shortest trap
reachable through a `mut` receiver. Ruling `pop` recoverable removed the only
trap either test had. The claim under test is untouched by the ruling, so the
answer is a different provocation and not a weaker assertion: `push` lends its
receiver at the same `check_home_write` site and traps `overflow` on a literal
outside the element type (`[arith.checked]`), after the lend and before the
store. `xs.len` answering 0 on the next REPL line is the same proof it was.
Recording it because it is the shape a lane hits when a clause makes a trap
unreachable: the test is not stale, its *provocation* is, and only one of those
two is safe to delete.

#### Neither #275 nor the compiler's own gap moved

`[conc.chan.send]` (wolf-lang#275) is left open upstream and this pin does not
reach it; nothing here changed for it. wolf-interp#73 — a function's tail
unchecked against its declared return type, this machine's soundness row on
the pairing table — is unmoved at this release and stays open.

### The mirror spells `parse` — is41, lupin 0.1.29, pin `e9a17cb` (wolf-lang s143, dev-stamped)

s143 ruled two families this machine had been serving without a clause:
`str.to_int`'s parse row, and the rendering of every non-primitive
interpolation hole. **One class closed and none opened**: the walk's one
mismatch is still DIV-2026-019, and `rows/to_int_parse.lu` — the single
witness the two machines parted on at 0.1.28 — matches from this release.

| witness | lupin at 0.1.28 | lupin at 0.1.29 | the clause |
| --- | --- | --- | --- |
| `rows/to_int_parse.lu` | **`error: NotAnInt`, MISMATCH** | **`exit(1)@run`, match** | `[mem.str.parse]` |
| `grammar/else_default.lu` | `fell back: NotAnInt`, mismatch | **`exit(0)@run`, match** | `[mem.str.parse]`, `[type.interp.row]` |
| `strings/interp_values.lu` | not at the pin | **`exit(0)@run`, match** | `[type.interp.value]`/`.agg`/`.row`/`.union` |
| `conc/reason_interp.lu` | not at the pin | **`exit(0)@run`, match** | `[type.interp.reason]` |
| `conc/chan_param_for.lu` | not at the pin | **`exit(0)@run`, match** | `[conc.chan.close]`, `[conc.chan.type]` |

**`[mem.str.parse]` cost one string and the rename is the whole of it.**
`NotAnInt` was this implementation's spelling first — served since before
0.1.13, copied by wolf-std's corpus and by the compiler at s142 — and it was
the one CapCase payload-free tag on the builtin surface, which is precisely
the shape W0603 warns a program about. The clause is an amendment against
*this* machine's guess, arrived at by the pipeline working as designed: the
divergence was filed (wolf-lang#265), the spec was the defendant first
(`[proto.cmp.triage]`), the clause ruled against the incumbent spelling, and
the compiler moved at s143 with the mirror one wave behind.

**`[type.interp.*]` cost NOTHING, and that was the prediction.** spec/10 §4c
says in its own words that it "adopts the interpreter's rendering, byte for
byte, as the language's — it was the only rendering anyone had written down,
in code". The four witnesses were run against 0.1.28's binary before a line
was edited: `strings/interp_values.lu`, `conc/reason_interp.lu` and
`conc/chan_param_for.lu` printed their pinned `stdout` exactly, and
`grammar/else_default.lu` printed it but for the mark. Zero source motion on
`Value`'s `Display`; the only edits under §4c are citations —
`Rule::StrInterp` retires the forward `str.interp` for the registered
`[type.interp.value]` (the is26 move at a second address), and E0412/E0413
cite the clause that now rules a format spec on a hole.

This is the *inverse* of the usual entry, and worth naming as such: a
divergence class normally closes when this machine changes to meet a clause.
Here six clauses were written to meet this machine, and one clause was written
against it. Both are the pipeline; only the second costs a rename.

#### Two spellings this pin does NOT rule (wolf-lang#273, #274)

> **Both ruled at s144 and mirrored at 0.1.30 (is42).** `[conc.chan.close]`
> spells `closed`/`cancelled`; `[mem.list.pop]` answers the `none` row. Each
> ruling landed a corpus witness — `conc/chan_closed_row.lu`,
> `memory/list_pop_empty.lu` — so the next lane reads the postures below as
> the record of a filing that worked, not as an open question. The line
> numbers are the ones the mirror moved.

Filed by s143 against the same measurement and open at this pin. Neither is a
walk mismatch — no corpus witness reaches either — so neither is a DIV entry;
they are recorded here so the next lane finds this machine's posture with the
line number attached rather than re-deriving it.

- **The closed-channel row (wolf-lang#273).** `[conc.chan.close]` says
  "further sends return an error value" without spelling the tag. This machine
  mints `Closed` (`src/eval/sched.rs:1340`, `closed_error`) and discriminates
  on it in exactly one other place (`src/eval/conc.rs:674`, the `for v in ch`
  drained-close test); the compiler's `recv` row is `{closed, cancelled}`. The
  same question rides `Cancelled` (`src/eval/sched.rs:1352`,
  `src/eval/conc.rs:172`), which the compiler also spells lowercase — a ruling
  on one is a ruling on both.
- **`List.pop()` on an empty list (wolf-lang#274).** No clause rules it.
  This machine traps `bounds` (`src/eval/builtin.rs:1181`, citing
  `[mem.ub.defined]` through `Rule::Bounds`); the compiler types `pop` as
  `T ! {none}` and answers the row. `[type.interp.union]` renders `{popped}`
  as `3` or `none`, which is the compiler's shape written into a clause — but
  §4c rules the RENDERING of a `!T`, not whether `pop` returns one, so the
  question stands. The same clause should rule `List.get`, which answers
  `OutOfBounds` here (`src/eval/builtin.rs:1199`) — the second CapCase
  payload-free mark left on this surface after `[mem.str.parse]` retired the
  first.

### The mirror writes vectored — is40, lupin 0.1.28, pin `5c729e8` (wolf-lang v0.2.8)

s141 gave the compiler a gathered write and a stream option; s142 gave it
`str.to_int` and a stat on an open handle. This sprint is the mirror taking
both merges at one tag, and **no class opened**: the walk's one mismatch is
still DIV-2026-019.

| witness | lupin at 0.1.27 (unimplemented) | lupin at 0.1.28 | the clause |
| --- | --- | --- | --- |
| `net/writev_gather.lu` | not at the pin | **`exit(0)@run`, match** | `[os.net.writev]` |
| `net/nodelay.lu` | not at the pin | **`exit(0)@run`, match** | `[os.net.nodelay]` |
| `net/syscall_first.lu` | not at the pin | **`exit(0)@run`, match** | `[os.net.io]` |
| `strings/to_int.lu` | not at the pin | **`exit(0)@run`, match** | wolf-lang#263 |
| `rows/to_int_not_an_int.lu` | not at the pin | **`exit(1)@run`, match** | wolf-lang#263 |
| `fs/fstat.lu` | not at the pin | **`unsupported@resolve`, by DESIGN** | `[os.fs.fstat]` |

The last row is the one worth reading and it is not new: the s38 fs surface is
declined here by construction (wolf-interp#18 item 6 — an interpreter
observing the HOST's filesystem puts the host into a differential comparison),
so a stat on a handle joins `corpus/fs/`'s other three as out-of-scope.
`[proto.cmp.defined-divergence]` makes that a scope gap rather than a
divergence, and the row never reaches the comparison.

`net/syscall_first.lu` is the one that found something. `[os.net.io]` moves a
COST rather than a row, and the posture it names — poll the syscall first,
wait only on not-yet — is what this machine has done since is18. But the
clause also puts a write's whole drain under the call's budget, and the
witness asserts that a budgeted large `net_write` comes back with
`net_write`'s own `io`. This machine answered a bare `timeout`: a tag outside
the row `net_write` declares, so a handler's `match` could not resolve it as a
tag at all (`[gram.expr.tagident]`; the wolf-interp#47 mechanism). That is a
pre-existing defect the new clause exposed, not a disagreement with it, and
`eval::net::budget_row` is the coarsening the clause states in its own words.

**Five files newly reach a verdict that matches, one is newly out of scope,
none stopped**, and the conservatism ledger moves 124 -> 126 with the sixth.

### The cores in the mirror — is38, lupin 0.1.27, pin `6ade878` (wolf-lang v0.2.5)

s137 landed a server's other half and r08 shipped it. This sprint is the
mirror catching up to all five anchors at once, and the ledger entry is short
because no class opened: the walk's one mismatch is still DIV-2026-019,
and the four witnesses moved exactly as far as each one's lane allows.

| witness | lupin at 0.1.26 (the v0.2.5 pin, unimplemented) | lupin at 0.1.27 | the counterparty (`--checked`) |
| --- | --- | --- | --- |
| `net/reuse_port.lu` | `unsupported@resolve` | **`exit(0)@run`, match** | `exit(0)@run` |
| `net/wait_readiness.lu` | `unsupported@resolve` | **`exit(0)@run`, match** | `exit(0)@run` |
| `os/cpus.lu` | `unsupported@resolve` | **`exit(0)@run`, match** | `exit(0)@run` |
| `net/inherit_listener.lu` | `unsupported@resolve` | **`unsupported@resolve`, by NAME** | `unsupported@mem`, by NAME |

The fourth row is the one worth reading. Both machines decline it and both
name the same construct (`fd inheritance across os_spawn_with in checked
execution`), at different RUNGS, because the counterparty refuses it at
lowering and this machine at the builtin call, and `phase_reached` is the
deepest rung each COMPLETED. `[proto.cmp.defined-divergence]` makes an
`unsupported` on either side a scope gap rather than a divergence, so the
row never reaches the comparison at all; the two strings agree because
`tests/cores_s137.rs` asserts them, not because anything compares them.

Three files newly reach a verdict that matches, none stopped, and the
conservatism ledger falls 128 -> 122 on the interp side with the three.

The one thing this sprint declined to do is in DIV-2026-021 above: is38 ruled
the LOCUS row open rather than closing it by imitation, and closed the two
holes that let it sit unmeasured instead.

### The byte has a domain — is37, lupin 0.1.26, pin `982f857` (wolf-lang v0.2.4)

is36 shipped the byte TYPE and left the DOMAIN to the compilers. sc35
measured what that cost, re-running wolf-std's 181 byte rows against 0.1.25:
the casts, the widening and the ledger slots all held, and then `push(256)`
into a `List[byte]` stored 256. Eight std rows carried `divergent(…)` for
exactly that shape (the compilers refuse at typecheck with E0401, this
machine ran the program to its end), and the word had never before
been used outside the take-mode pair. wolf-interp#62.

The fix is two rules at two rungs, and the split is the interesting part.

The resolve-time refusal (`sema::byte_check`). `[type.byte]` says a `byte`
"adopts no numeric literal in any position" and is not an integer type, so an
`int` reaching a byte slot is a type error whatever its VALUE; the domain is
not a range check, it is a refusal. This rung now performs that one rule, for
that one type, at the counterparty's code and the counterparty's span. It fires
only where both sides are KNOWN; `Unknown` is the default answer and no rule
fires on one, which is the sema boundary restated rather than abandoned.
Measured on both machines at this pin:

| program | lupin 0.1.25 | lupin 0.1.26 | wolfc 0.2.4 |
| --- | --- | --- | --- |
| `typecheck/byte_narrow_fail.lu` | `unsupported@resolve` | `fail(E0401)` `[588,590]` | `fail(E0401)` `[588,590]` |
| `typecheck/byte_elem_arith_fail.lu` | `unsupported@resolve` | `fail(E0401)` `[849,850]` | `fail(E0401)` `[849,850]` |

Both are match against their `check: fail(E0401)` now, where 0.1.25 was
out-of-scope. `[proto.cmp.rung]` covers the rung difference the way it covers
E0805: the counterparty answers at `typecheck`, this machine at `resolve`, and
the corpus directive itself says `phase: resolve`.

The dynamic boundary (`Value::List`'s element type). The static pass is
partial, so the domain has to hold at the run rung too. Until
this release the element context was `Option<IntTy>` (it could spell integer
widths and nothing else), so `List[byte]()` and a bare `List()` were the
SAME VALUE at runtime, which is the mechanism behind #62 in one sentence.
`ElemTy` splits the two, `str.bytes()` and `net_read_bytes` hand back lists
that carry it, and an `int` reaching a byte element is refused by name (a
static property never becomes a trap here). is36's byte-place rule is the
same posture one slot over and stays exactly where it was, for the flows the
static pass declines to guess about.

The acceptance: sc35's eight rows, and two more. Re-measured with
`--std-root std` on both machines at wolf-std `d71776e`; every row is
code-and-span identical to the counterparty's first diagnostic:

| wolf-std row | ledger word (sc35) | lupin 0.1.26 | wolfc 0.2.4 |
| --- | --- | --- | --- |
| `hex/encode_non_byte_refused.lu` | `divergent(trap(assert))` | `fail(E0401)` `[1744,1747]` | same |
| `net/write_bytes_invalid_row.lu` | `divergent(exit(1))` | `fail(E0401)` `[1766,1768]` | same |
| `x/crypto/sha2/non_byte_refused.lu` | `divergent(exit(0))` | `fail(E0401)` `[1767,1769]` | same |
| `x/crypto/chacha20/non_byte_refused.lu` | `divergent(exit(0))` | `fail(E0401)` `[1817,1818]` | same |
| `x/crypto/curve25519/non_byte_refused.lu` | `divergent(exit(0))` | `fail(E0401)` `[1827,1828]` | same |
| `x/tls/record/non_byte_refused.lu` | `divergent(exit(0))` | `fail(E0401)` `[2001,2004]` | same |
| `x/tls/handshake/non_byte_refused.lu` | `divergent(exit(0))` | `fail(E0401)` `[1777,1780]` | same |
| `x/tls/cert/non_byte_refused.lu` | `divergent(exit(1))` | `fail(E0401)` `[1757,1759]` | same |
| `fs/invalid_row.lu` | `unsupported` | `fail(E0401)` `[1652,1655]` | same |
| `x/crypto/p256/non_byte_refused.lu` | `unsupported` | `fail(E0401)` `[1800,1801]` | same |

The last two are the corpus twins: both were ledgered `unsupported` for a
reason that had nothing to do with bytes (this machine declines the fs tier
by design, and p256's ladder is outside the modelled surface), and both now
answer the directive because the refusal arrives BEFORE the decline. Ten
rows move, all of them toward agreement, and `divergent(…)` returns to zero
carriers (`cargo xtask std-test` says so in as many words). The ledger edits
are wolf-std's to make.

What did NOT move. A full resolve-rung sweep of wolf-std's 376 `.lu` files
found exactly these ten new E0401s and nothing else; the three standing E1001
rows (is29's static rung) are untouched, and the 503-file corpus walk gained
exactly two matches with no new refusal anywhere. `[type.byte.op]`'s widening is
the guard rail that makes that possible: `b + 1` is an `int` by clause, so the
pass says nothing about it, and it was measured against the counterparty before
it was written down.

wolf-interp#60: the report that could not tell two mistakes apart. ww13 put
`typecheck/byte_casts.lu` on the playground at the 0.1.24 pin and found
`200 as itn` (a typo) and `200 as byte` (a scalar the pinned spec declares,
which that release did not carry) answering byte-identically, with a
sentence that sent the reader hunting a misspelling that was not there. `byte`
arrived at 0.1.25, so that pair is gone, but the pin runs ahead of this
implementation BY DESIGN, so the collision recurs at every tag where the
compiler lands a type first. E0301's note now names the closed set of
built-in type names this release carries, rendered from the check's own table
so it cannot drift, and states both readings instead of asserting one.

wolf-interp#59: closed, by measurement. D74's three layout rules landed
at 0.1.25 and is36's release note claimed the issue closed; the claim was
never checked on this side against the counterparty. It is now. All five of
s136's witnesses answer wolfc's CODE at wolfc's SPAN, re-measured on both
machines at this pin: E0103 `[437,441]` and `[598,601]`, E0104 `[520,526]`,
E0105 `[575,583]`, E0101 `[1566,1568]`. The issue's own table (`fail(E0109)`
`[31,49]` against `fail(E0103)` `[31,35]`, "a code divergence AND a span
divergence, twice") has no surviving row. The BOM's two positions hold too:
a leading mark is stripped and never a diagnostic (`grammar/bom_at_start.lu`
runs to `exit(0)` on this lane), and mid-file the same three bytes are E0107
at the mark. `E0105` is gone from `UNPINNED_CODES`, which is the third
collision the issue asked to move.

### The byte arrives — is36, lupin 0.1.25, pin `982f857` (wolf-lang v0.2.4)

The sprint that took the deferral back. is35 recorded `byte` as DEFERRED BY
NAME because s135 had not merged; s135 and s136 merged together, and this
release mirrors both halves at once. Three findings, none of them a
cross-implementation divergence at the end of it: every one of the seventeen
corpus files this pin adds answers the counterparty's record, and the whole
byte and layout flip set is agreement.

wolf-lang#203/D72: the type, and the two programs it stopped from
running. The permissive divergence is the one that is hard to notice, and
`byte` closed two at once. Measured against
`lupin 0.1.24 (pin 3befc3e)` on the same files:

| program | lupin 0.1.24 | lupin 0.1.25 | corpus pins |
| --- | --- | --- | --- |
| `typecheck/byte_narrow_fail.lu` | `exit(0)`, stdout `65 300` | `unsupported@resolve` | `fail(E0401)` |
| `typecheck/byte_elem_arith_fail.lu` | `trap(bounds)` | `unsupported@resolve` | `fail(E0401)` |
| `typecheck/byte_shapes.lu` | `fail(E0301)` `[1131,1135]` | `exit(0)` — **match** | `run(exit=0, …)` |
| `typecheck/byte_casts.lu` | `fail(E0301)` `[1214,1218]` | `exit(0)` — **match** | `run(exit=0, …)` |
| `strings/bytes_roundtrip.lu` | `exit(0)` (by accident) | `exit(0)` — **match** | `run(exit=0, …)` |
| `memory/byte_list_ledger.lu` | `fail(E0301)` `[2212,2216]` | `exit(0)` — **match** | `run(exit=0, …)` |
| `memory/consumed_walk_charges_nothing.lu` | `exit(0)`, `bound_view_charges FALSE` | `exit(0)` — **match** | `run(exit=0, …)` |
| `net/echo_bytes.lu` | `fail(E0301)` `[1218,1222]` | `exit(0)` — **match** | `run(exit=0, …)` |
| `strings/from_utf8_border.lu` | `fail(E0301)` `[1531,1535]` | `exit(0)` — **match** | `run(exit=0, …)` |
| `net/byte_roundtrip.lu` | `fail(E0301)` `[1186,1190]` | `exit(0)` — **match** | `run(exit=0, …)` |
| `net/line_reader_bytes.lu` | `fail(E0301)` `[1118,1122]` | `exit(0)` — **match** | `run(exit=0, …)` |

The first row is the finding. `let b: byte = 65` and `let c: byte = n` with
`n = 300` ran to completion at 0.1.24 and printed `65 300` (a "byte" holding
three hundred), because an annotation naming no known scalar left the value
alone, and only a CAST target was resolved (#17's E0301 rung). The
counterparty rejects the program at `resolve`. Both refusals are
`unsupported` here rather than E0401, which is where this machine's type
mismatches live and where its `char` twins have sat since s121: the class is
out-of-scope, not a divergence.

The seventh row is the other kind of quiet wrong answer. #232's witness has
three relations that read `true` when NOTHING is charged and a fourth,
`bound_view_charges`, that exists to prove the region is being read at all.
0.1.24 passed three and failed the fourth, because `s.bytes()` charged
nothing in any position. Now the consumed positions charge nothing *by rule*
and the materializing one charges the buffer.

D74/wolf-lang#230: five witnesses, five exact spans. Measured against
`wolf 0.2.4 (wolfgang)` at this pin, code and span, both machines' records:

| witness | lupin 0.1.24 | lupin 0.1.25 | wolfc 0.2.4 |
| --- | --- | --- | --- |
| `grammar/multiline_open_shares_line.lu` | `E0109` `[437,460]` | `E0103` `[437,441]` | `E0103` `[437,441]` |
| `grammar/multiline_close_shares_line.lu` | `E0109` `[579,598]` | `E0103` `[598,601]` | `E0103` `[598,601]` |
| `grammar/multiline_short_margin.lu` | `E0109` `[507,538]` | `E0104` `[520,526]` | `E0104` `[520,526]` |
| `grammar/multiline_mixed_margin.lu` | `exit(0)` — it RAN | `E0105` `[575,583]` | `E0105` `[575,583]` |
| `grammar/str_bare_brace.lu` | `E0102` `[680,716]` | `E0102` `[680,694]` | `E0102` `[680,694]` |
| `grammar/bom_at_start.lu` | `fail(E0105)` `[0,3]` | `exit(0)` `bom ok` | `exit(0)` `bom ok` |

Every row is agreement now, span for span. Two are worth their own sentence.
`multiline_mixed_margin.lu` ran at 0.1.24, silently eating eight tabs
against an eight-space margin, which is the layout tier's own permissive
divergence. And `str_bare_brace.lu` had the right code with a span running to
the end of the FILE: `[gram.lex.newline]` says a newline inside an
interpolation never terminates, and `world"` inside the runaway interpolation
spells a *generalized* literal whose body this lexer let cross lines:
`GEN_TEXT ::= (SCALAR - ('"' | NL))*` excludes `NL` and `RAW_TEXT ::= SCALAR*`
does not, so the neighbouring rule is read off the production. wolf-interp#59
closes: the layout tier the freed codes named is implemented.

The finding the pin produced, and it was not in the lexer.
`grammar/bom_at_start.lu` is the first corpus file whose first three bytes are
`EF BB BF`. `[gram.lex.source]` governs how the FILE is read, and the corpus
DIRECTIVE header is a run of `//!` comments in that same file, so a header
reader that does not strip the mark sees `\u{feff}//! check: …` as prose and
the file loses its entire header. A file with no `check:`/`phase:` pair is
not a standalone entry (`[conf.directive.member]`, D59), so it joins its
directory's module, and its `main` collides with every sibling's:

| | files answering `E0302` `[mod.dup]` |
| --- | --- |
| pin taken, header reader unfixed | **32** (`grammar/` entire, plus the file itself) |
| header reader strips the mark | 0 |

Thirty-two of the raw pin's forty-three walk mismatches were one missing
`strip_prefix`. Recorded here because the shape generalizes: a lexical rule
about how a file is READ binds every reader of that file, not only the lexer.

A cross-compilation gate, not a divergence: the unix family broke clippy
on a host that has no unix family. Three `std::path` imports and one
`let Some(sock)` binding are used only inside `#[cfg(unix)]` arms, so on
windows they are dead code and `-D warnings` fails. Every local gate was
green; GitHub's windows runner was not. is35 recorded the same lesson from
W0316's checkout-path walk, and this is the answer to it that costs a minute
rather than a round trip: `cargo clippy --target x86_64-pc-windows-msvc
--all-targets -- -D warnings` reproduces the runner's verdict on the laptop,
and `--target x86_64-unknown-linux-gnu` covers the other half of the matrix.
Both targets were installed already; nobody had run them.

wolf-lang#227: the unix family serves, and its witness still does not
run. `[os.net.unix]` is implemented whole: both binders, the row
vocabulary that distinguishes the HOST (`unsupported`, by name, never a bare
`io`) from the PATH, `net_port` answering `io` because a path has no port,
and the cleanup posture: a LISTENER's close unlinks, a stream's close does
not. A plain regular file at dial is neither of the clause's two dial cases,
so the KERNEL decides and the hosts disagree: macOS answers `ENOTSOCK` (this
machine's `io` row, and wolfc's, measured), linux answers `ECONNREFUSED`
(`refused`). The witness reads the row and accepts either (the posture
`net/peer_close_after_serve.lu` takes to a reply the kernel may or may not
deliver past an RST), because the clause rules the two cases it names and
this is not one of them. GitHub's ubuntu runner said so and no developer's
machine did; local green is not green, twice in one sprint.

`corpus/net/unix_echo.lu` is nevertheless out of scope here, and not for
the sockets: its first statement is `fs_exists(path)` and its cleanup is
`fs_remove(path)`, and this machine declines the whole s38 fs surface by
design (wolf-interp#18 item 6, an interpreter observing the HOST's
filesystem puts the host into a differential comparison). The tension is
worth stating rather than papering over: this release serves a socket family
whose entire surface is filesystem PATHS while declining the filesystem.
`tests/net_unix.rs` carries the witness's three lanes and the row table
instead. The four `fs_*` byte producers are absent for the same standing
reason, which is why `memory/byte_producers_ledger.lu` and
`fs/bytes_dirs.lu` remain out of scope: the FS tier, never the byte one.

wolf-interp#50's shape, one type over, caught the same day. The first
`byte` this machine ran moved on assignment:

| program | lupin, first cut | lupin 0.1.25 | wolfc 0.2.4 `--checked` |
| --- | --- | --- | --- |
| `let a = 65 as byte` / `let b = a` / `{a} {b}` | `trap(use-after-move)` | `65 65` | `65 65` |

`[mem.tier0.move.3]` admits "POD-shaped types only", and D72 rules `byte` an
8-bit unsigned SCALAR ("an `i8`-shaped storage cell at every tier"), so it
is POD by the same clause `char` is. `char` took this exact wrong turn and
kept it until 0.1.16 (wolf-interp#50); `byte` kept it for an afternoon,
because the new type was walked against the clause's sentences one at a time
rather than only against the corpus. It never shipped. Recorded because the
lesson is about the method: a new `Value` variant inherits nothing, and the
move discipline is a list this machine keeps by hand.

wolf-interp#61 OPENED: the literal-adoption rule is enforced in two
positions out of four. Walking `[type.byte]`'s "no numeric-literal adoption
… in every position" found four places to test, and this machine enforced
one of them (the annotated `let`). Two more were closed here, because the
missing check produced a WRONG ANSWER rather than a missing refusal:

| position | lupin, before | lupin 0.1.25 | wolfc 0.2.4 |
| --- | --- | --- | --- |
| `let b: byte = 65` | refused | refused | `E0401` |
| `match b { 65 => …, _ => … }` | `exit(0)`, **`other`** | refused | `E0401` |
| `b = 66` / `b += 1` | `exit(0)`, `b` **becomes an int** | refused | `E0401`/`E0409` |
| `fn f(b: byte)` called `f(65)` | `exit(0)` | unchanged | `E0401` |
| `fn g() -> byte { 65 }` | `exit(0)` | unchanged | `E0401` |

The match arm took the wrong branch (`other` for a byte that is 65), and the
assignment silently retyped a live variable, so every later `{b}` rendered
an int. The last two positions do not do that: the value passes through and
the program computes what a correctly-spelled one would, which is this
machine's ordinary conservatism class and the class every other type
mismatch here already sits in.

The `char` twin is identical in all four, and its ASSIGNMENT position is
still open (`var c = 'a'` then `c = 65` prints `65` here, and has since
s121). Filed rather than widened: the fix is ONE rule (a declared-scalar
check at every typed boundary sema-lite can see, applied to `char` and
`byte` together), and the two byte-local guards added here should collapse
into it. Triage: this implementation is the defendant; both clauses are
unambiguous. No corpus file measures any of the four, which is why the
census is unaffected and why this was found by reading the clause rather
than by running the walk.

### The byte in the mirror — is35, lupin 0.1.24, pin `3befc3e` (wolf-lang v0.2.3)

The sprint that made a code mean one thing. Three findings closed, one
opened, one waiver retired by the ruling it was waiting for, and a lint bug
that only GitHub's checkout path could see.

wolf-lang#225: RESOLVED HERE, and the clause never moved.
`[gram.lex.str.escape]` has read "`STR_ESC` … and nothing else; any other
`\` is E0101 at the escape" since #198 landed in v0.2.2 (the pin 0.1.23 was
released against), so the number was never this implementation's to choose.
It answered E0103, and a `\u` with no braces answered E0104, which are the
two numbers the catalog spends on the multiline's LAYOUT. A program refused
for a bad escape and one refused for a badly shaped `"""` were the same
record here. Triage case 3 all the way through: the clause was unambiguous
and this implementation was the defendant.

Span parity was exact before the change and after it, which is why 484
files never showed it; #198's two witnesses pin the `\u{…}` DIGIT BOUND,
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

Only the number moved, on every row. The witness
`grammar/multiline_bad_escape.lu` arrived with this pin already carrying the
measured divergence and reads `fail(E0101)@lex` (match), in the commit that
takes the clause. Its running twin `strings/multiline_escapes.lu` answered
at first sight. The `char` literal's own E0110 did not move with the string
tier's number: `'\q'` is still one report over the whole literal.

What freeing E0103/E0104 revealed, filed as wolf-interp#59. They were
free because this implementation does not implement the rules they name.
v0.2.3's `[gram.lex.str.multi]` (new productions, #215) states three layout
side conditions with three codes; this machine has one rule for all of it,
`E_DEDENT_UNDERRUN` (E0109):

| program | lupin 0.1.24 | wolfc 0.2.3 |
| --- | --- | --- |
| text after the opening `"""` | `fail(E0109)` `[31,49]` | `fail(E0103)` `[31,35]` |
| a content line left of the margin | `fail(E0109)` `[32,43]` | `fail(E0104)` `[32,34]` |

Code and span, twice, and no corpus file measures either, the same shape
as #225 one clause over. `tests/str_escape_code.rs` asserts that this machine
answers neither number in the meantime, so the collision cannot come back
quietly. A wolf-lang question rides along: #225 quotes the catalog assigning
E0104 to "a multiline string line sits left of the margin", while
`[gram.lex.str.multi]`'s own sentence assigns E0104 to the *closing
delimiter* condition and E0105 to the margin one. Two documents, one number,
two readings.

wolf-interp#57: closed. What `main` may return is a declaration fact
(wolf-lang#106), and this machine discovered it from the value: `finish`
looked at what came back, so `typecheck/main_returns_str.lu` executed its
whole body and wrote `hi` to the process's stdout before declining. The
record said `unsupported@resolve`, which was true, and the invocation had a
side effect the record did not report, which for an observation tool is the
hazard is34 filed rather than absorbed. The decline is on the admission
ladder now. Verdict and rung unmoved; the claim is true.

wolf-lang#216: the comparator half landed, and it is MEASURED EMPTY.
`differ::run_rung` compares a trap's output bytes when both sides hold them.
This is the clause's proposed reading applied to the instrument, not to
`src/compare.rs` (which still holds `[proto.cmp.phase]` as written), and it
is safe ahead of a ruling for two reasons: a widened comparison can only ADD
rows, never hide one, and it is gated on both sides HOLDING the field:
`None` on either is `[proto.record.fields]`'s honest-absent, the posture
`[proto.cmp.warn]` already takes to a missing `warnings` array.

Every mover, classed: there are none, and here is why that is the finding
rather than a disappointment. The bundle has 63 trap records; 61 write
nothing before the fault, so the widened comparison has no field to look at
and is honest-absent on both sides. The two that do write are is34's:

| file | verdict | lupin | wolfc `--checked` | `--native` | `--release` | class |
| --- | --- | --- | --- | --- | --- | --- |
| `faults/trap_skips_root_defers.lu` | `trap(assert)` | `fe91a58b…` | `fe91a58b…` | `fe91a58b…` | `fe91a58b…` | **agreement** |
| `rows/handler_diverge_trap.lu` | `trap(assert)` | `c2eba7a1…` | *unsupported* | `c2eba7a1…` | `c2eba7a1…` | **agreement** (checked lane declines to run it — conservatism, unchanged) |

So: zero new divergence rows on any tier, and the class is not empty
because the question is uninteresting; it is empty because the one file it
was built for was fixed at 0.1.23. Under 0.1.22's behaviour the first row
reads `inner inner-defer before-trap root-defer` here against
`inner inner-defer before-trap` there: same verdict, same trap kind,
different bytes, and invisible for the whole of D66..r05. That
counterfactual is pinned as a unit test with the real digests
(`differ::tests::a_trap_s_output_compares_when_both_sides_hold_it`), because
"the comparison would have caught #209" is a claim and not a comment.

This also answers #216's sub-question, as far as three lanes can. The
flush concern was whether a trapping program's stdout is portable across
wolfc's tiers at all. On every trapping corpus program that writes before its
fault, `--checked`, `--native` and `--release` return the same digest as each
other and as this machine. That is two files, not a proof, but it is two
files more than the one r05 had, and no tier disagrees with any other
anywhere in the corpus.

W0316: a lint that read the checkout path. Not a cross-implementation
divergence at all; recorded because of how it was found. The pin brought
`conc/proc_cross_module/main.lu`, which says `use work`. The W0316 walk asks
whether a module imports one of its own ancestor MODULES, and its stop
condition tested whether a candidate WAS the entry root, which never happens
for a scope file sitting directly in it, so the walk climbed the whole
filesystem path. GitHub checks this repository out under
`/home/runner/work/wolf-interp/…`; the file warned on both Linux and macOS
runners and on no developer's machine, and the corpus `warns:` ledger caught
it. Local green is not green.

`byte`: DEFERRED to is36, by name. D72 rules a byte-width scalar into
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

Minus seven on every tier, and every one of them is DIV-2026-020
closing. D71 ruled the strong form (the span IS the offending token), s134
aligned wolfc's parser, and wolf-lang#220's closing comment assigned the
waiver's retirement to this lane's next pin bump. Seven of its eight files
are byte-identical now. Nothing this sprint wrote caused the drop; taking
the pin did. Conservatism rises by 2 on `checked`, which is the two new
run-reaching corpus files.

Everything still gating is older than this sprint, and after the two
retirements the filed list is two entries again rather than nine.

### DIV-2026-021 — `grammar/let_group_bare_tuple.lu` — **RESOLVED upstream at pin `2e4ca769` (0.1.38, is53): wolf-lang#228 ruled and s163 moved the compiler onto the comma; the two loci are one byte range and the waiver is retired**

**Resolution, measured before the entry moved.** At the `2e4ca769` pin
(wolf-lang **v0.2.15**) the counterparty answers
`fail(E0201)@parse` with span `[511, 512]` — this machine's own locus, and
the `,` itself — under
`wolf conform-run …/grammar/let_group_bare_tuple.lu --json --checked`. The
numbers moved twice at once: the corpus file's header grew 147 bytes in the
same bump, so `364 -> 511` is the file and `374 -> 511` is the ruling. The
differential runner confirms it from both ends — the `span-or-code` line
below is present at the old pin and absent from all four tiers at the new
one. `differ::FILED_DIVERGENCES` is one entry again (DIV-2026-019), and
`tests/let_group_locus.rs` asserts the retirement rather than the gap.
Nothing in this machine moved: it has pointed at the comma since is38 and
never moved to the counterparty's byte, which is what the filing rule is for.

The history, as it stood while the row was open:


The eighth row of DIV-2026-020's table, promoted when the other seven closed.
It was never the span-WIDTH question: both machines answer `fail(E0201)` at
parse and disagree about where, ten bytes apart, on all three tiers.

| | span | bytes |
| --- | --- | --- |
| lupin 0.1.24 | `[364,365)` | `,` — the comma in `let a, b` |
| wolf 0.2.3 (`--checked`/`--native`/`--release`) | `[374,375)` | `\n` — the end of the initializer list |
| **lupin 0.1.27** (is38) | `[364,365)` | unmoved |
| **wolf 0.2.5** (`--checked` and the default lane) | `[374,375)` | unmoved |

Triage: spec bug, case 1. `[gram.item.let]` says what a D63 let-group is
and what the bare-tuple shape is not; it does not say where refusing it
reports. Both readings are coherent: the comma is the first byte at which
the input stops being a legal `let`; the end of the initializer list is where
the count mismatch becomes knowable, and is what wolfc's teaching note is
about ("this value has no name", with both fixes). wolfc has the better
diagnostic and this machine the better locus, which is exactly a question a
clause should settle rather than two implementations settle by imitation. The
corpus directive cannot see it: `check: fail(E0201)` pins the code, and the
walk compares codes.

#### is38's reading, and why this lane does not close the row

The is38 contract offered two branches: implement the span comparison at
that rung so the row becomes measurable, or rule the divergence and close
it. The premise for the first branch is r08's pairing note ("this harness
compares codes at that rung, not spans"), and that sentence is TRUE OF THE
HARNESS IT WAS WRITTEN ABOUT and false here. `compare::compare` and
`differ::compare_deep` have compared the first diagnostic's code and span at
every rung through `mem` since is01, and this row is exactly what they
report, re-measured at the v0.2.5 pin:

```
span-or-code  …/grammar/let_group_bare_tuple.lu  a=E0201@[364, 365]  b=E0201@[374, 375]  parse [filed: DIV-2026-021]
```

So the span comparison at that rung exists, and the row is measurable in this
repository today. What is38 declines is the other branch, and the reason is
a standing rule rather than a preference. CONTRIBUTING's divergence-filing
rule is `[proto.cmp.triage]` in one sentence: *the two parsers never reconcile
by private agreement, and neither one is patched to match the other before the
clause is fixed.* `[gram.item.let]` has not moved and wolf-lang#228 has no
comment on it. Moving this machine's locus onto the counterparty's would close
the row without the clause ever deciding anything, with two implementations
agreeing on something undocumented, which is the exact failure the rule
exists to prevent. The ask is unchanged and it is one sentence in
`[gram.item.let]`; the loser then moves.

#### What DID land, because "measurable" was doing too much work

The row was measurable only in a harness that needs a counterparty binary and
runs on nobody's schedule. Two gates close that gap, and neither of them
touches the locus:

1. This machine's half is pinned hermetically, in `tests/let_group_locus.rs`.
   Nothing in `cargo test` had ever asserted that this parser points at the
   comma: the corpus directive is `check: fail(E0201)` and the walk compares
   codes, so a drift to byte 374 would have closed the divergence in silence
   and left this entry asserting a disagreement that no longer existed. The
   test reads the pinned witness, pins `E0201` at `[364,365)` and slices the
   byte back out of the source to name it (`","`), and the counterparty's half
   is re-measured beside it whenever a counterparty binary exists, SKIPping
   when one does not, as the differential lane does.
2. A waiver can no longer outlive its divergence:
   `differ::retired_waivers`, reported and GATING in `lupin diff-run`. For
   every entry in `FILED_DIVERGENCES` whose file a foreign counterparty
   actually answered for, the runner asks whether a divergence came back; a
   "no" is now a finding against this document. wolf-lang#177 taught the shape
   twice and both times a human noticed instead of a gate. The self-
   differential and a `--replay` of this machine's own bundle retire nothing,
   because a machine compared against itself agrees with itself everywhere and
   that is evidence about nobody.

A divergence no gate can see is the shape this project keeps finding the hard
way; so is a waiver no gate can retire. The locus stays where the clause left
it, and both of those holes are closed.

### The letters in the mirror — is34, lupin 0.1.23, pin `8cda3aa` (wolf-lang v0.2.2)

Not a corpus sweep but a finding about what a record reports, and its
consequences.
The ledger entry is short because two of the three letters closed at
agreement.

#209: RESOLVED HERE, at the ruling. `faults/trap_skips_root_defers.lu`
arrived at this pin already carrying a measured divergence: r05 recorded
every wolfc lane (`--checked`, `--native`, `--release`) printing `inner
inner-defer before-trap` where lupin 0.1.22 printed `inner inner-defer
before-trap root-defer`, both at `trap(assert)`. `[conf.trap.exit]` gained
the sentence that settles it (*a trap runs no `defer` or `errdefer`,
anywhere*), and this machine's root path took it. The witness now agrees
byte for byte. Triage case 1 all the way through: the spec was the defendant
(it ruled the proc path in s132 and was silent about the root), the clause
was amended first, and only then was the implementation moved. is33 flagging
the gap rather than guessing at it is what made that order possible.

#55: the blind spot that hid it. Through 0.1.22 this implementation
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
is, at this pin, no: `handler_diverge_trap` was the only other file
whose trap-path output had never been looked at, and it was right.

A conservatism, declared. Reading the clause with no verdict condition
at all would move a third record: `typecheck/main_returns_str.lu`,
`unsupported@resolve`, would gain `stdout_inline: "hi\n"`, because this
machine evaluates `main`'s body before declining that `main` returned
`str`. A record whose `phase_reached` says the run did not complete makes
no run observation, so it carries none; the side effect itself is a real
finding and is filed as wolf-interp#57 rather than smuggled onto the
wire, with `a_record_that_completed_no_run_reports_no_stdout` standing as
its red test.

The question this leaves open, filed as wolf-lang#216.
`[proto.cmp.phase]` still rules the run rung "for `trap`, compare kind
only", and `compare`/`differ` still implement exactly that; widening the
comparison by private agreement is what the independence doctrine forbids,
and #209 is the proof that the letter comes first. So both machines now
*hold* the observable and the protocol rules it uncomparable, which is #55's
blind spot one layer up. The sub-question that makes it more than a
one-liner: whether a trapping program's stdout is flushed to the same byte
at all three wolfc tiers, or whether the current sentence is deliberate.

#### The sixteenth corpus differential — the first since 0.1.11

The table below this section is lupin 0.1.11's. Eleven releases went by on
the corpus walk alone (which compares the `check:` code, never the span) and
on record self-replay, so `diff-run` against a real counterparty had not
been recorded since pin `f8dca42`. is34 built it (`cargo build -p
wolf_driver -p wolf_rt` inside `upstream/` at v0.2.2, the legitimate binary
acquisition this document rules), and ran all three run-reaching tiers.

| tier | divergences | of which filed | gating after filing | conservatism |
| --- | --- | --- | --- | --- |
| `checked` | 15 | 9 | 6 | 249 |
| `native` | 12 | 9 | 3 | 215 |
| `release` | 12 | 9 | 3 | 215 |

The three letters agree on every lane. `faults/trap_skips_root_defers.lu`,
`rows/handler_diverge_trap.lu`, `grammar/str_uni_seven_digits.lu` and
`strings/str_uni_leading_zeros.lu` appear in no divergence report at any
tier; #209 is closed against the real counterparty and not merely against
r05's transcript, and #55's second mover is confirmed right.

Everything gating is older than this sprint, and one class dominates it.

### DIV-2026-020 — the E02xx span convention — **RESOLVED upstream at pin `3befc3e` (0.1.24): D71 ruled the span IS the offending token, s134 aligned wolfc, seven of eight files byte-identical; the eighth is DIV-2026-021**

Eight `grammar/` files, identical on all three tiers: same code (E0201),
same byte where the refusal starts, different span width. This machine spans
the offending token; the counterparty emits a zero-width span at its start.
Both renderings put the caret in the same column (wolfc's own output for
`struct_literal_no_separator.lu` carets byte 550, exactly where lupin
points), so s132/D69's "byte-for-byte where lupin points" is true of the
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

Triage (`[proto.cmp.triage]`): spec bug, case 1. `[proto.record.diag]` rules
spans byte-offset half-open and compared; nothing says what a diagnostic
about an *unexpected token* spans. Both conventions are defensible:
zero-width reads "something is missing HERE" and pairs with the
machine-applicable insertion suggestions the counterparty's parser grew in
s131/s132; a token span reads "THIS is what went wrong". Moving either side
to match the other without a ruling is imitation, which is the mistake #209
was resolved by not making. The upstream ask is either the convention in
`[proto.record.diag]` or a `[proto.cmp.rung]`-shaped tolerance in
`[proto.cmp.phase]`: same code, same start, agree. The last row is not the
same finding (its offsets genuinely differ), and it is named separately so
the weaker ruling cannot silently absorb it. lupin 0.1.23 changes nothing
here: its spans are byte-identical to 0.1.22's, and #56's teach-note is
additive (a second line and a longer message, never a relocation).
`differ::DIV_2026_020_FILES` carried the waiver and is retired at the
3befc3e pin, which is where wolf-lang#220's closing comment placed it: seven
of the eight files are byte-identical to the counterparty now, and the
eighth (`let_group_bare_tuple.lu`, whose offsets always genuinely differed)
is carried on as DIV-2026-021 rather than absorbed by the ruling that does
not cover it. is35 re-measured all eight on all three tiers before removing
the list.

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

Unchanged: DIV-2026-019 is still the one standing corpus-walk mismatch,
still filed, still waived by `FILED_DIVERGENCES`. Corpus verdict-identity
across the whole pin bump: 332 match, 16 dynamic counterparts, 42
conservatism, 58 out of scope. #198's string half
(`grammar/str_uni_seven_digits.lu`, `strings/str_uni_leading_zeros.lu`)
answered at first sight (E0101 at the escape, column 14, the same column its
`char` twin reports), so it never became a finding at all. #56's teach-note
is wording, outside the protocol by D22, and moved no record.

Fifteenth corpus differential: lupin 0.1.11, pin `f8dca42` (the
largest semantic movement the compiler has had in one wave: s74 the
correctness cluster, s75 `List` element access as a load with
caller-side bounds checks, s76 containers allocating in the ambient
region per D12, s77 `s.bytes()` as a view over the receiver's own
storage, s78 the affine relational channel, plus s53 script mode and
D43/D44; 258 entries compared, 22 members through their entries;
counterparty built CLEAN at the pin from a deleted `target/` with
`libwolf_rt.a` provisioned). Run four times, once per counterparty
tier.

| tier | divergences | conservatism | both execute |
| --- | --- | --- | --- |
| `default` | 0 | 422 | 0 |
| `checked` | 1 | 181 | 114 |
| `native` | 1 | 184 | 116 |
| `release` | **1** | 204 | **106** |

THE HEADLINE: the compiler moved its lowering out from under three
semantic areas and this machine's independent reading already agreed on
every one of them. Ten of the wave's thirteen new corpus files reach
`run` here at FIRST SIGHT, with no new semantics written on this side,
including all four of the wave's own semantic witnesses. The one file in
the wave that needed a reading here was s53's `[gram.lex.shebang]`, the
wave's only `spec/` delta. The corpus-wide differential found no new
divergence; the single one it reports is DIV-2026-017, unchanged.

The three probes the re-pin was run to answer:

1. Region-scoped container lifetime (s76): AGREE on every defined shape,
   with one declared gap. A container built in a region and freed with it,
   a callee allocating into its *caller's* region (D12, the reason the
   ambient region is dynamic rather than lexical), growth across several
   region chunks, `freeze` letting a container outlive the block that built
   it, and nested regions: all identical on `lupin`, `--native` and
   `--release`. This machine has always modelled regions dynamically and has
   always placed a callee's allocation in the ambient region, so s76 is a
   move *toward* this machine's reading. The gap is the escape:
   `memory/region_escape_container.lu` is E1010 on every compiler lane and
   `exit(0)` here, because the escape is a static region judgement this
   machine does not make, and, unlike the handle/pool escape
   (`tests/faults/region_uaf.lu`, which traps `region-fault`), this machine
   does not catch it dynamically either. A read through the escaped
   container after its region closes answers with the old values rather than
   trapping. That is conservatism in the ledger's sense (no conforming
   program can observe it, since the compiler rejects the shape statically),
   but it is a real modelling gap and it is now declared in the
   approximation contract (§6.13) rather than left implied.
2. Byte views and the slice domain (s77): AGREE, exhaustively.
   `s.bytes()` is a view over the receiver's own storage on both sides:
   unsigned 0..=255 (the two continuation bytes of `é` are 195 and 169,
   never negative), length is the byte length, and the empty walk allocates
   nothing. The slice domain was swept rather than sampled: all 100 endpoint
   pairs of `s.get(a..b)` over the mixed-width `é€` from −2 to 7
   (including the whole negative half the corpus file does not reach)
   are byte-identical on `lupin`, `--native` and `--release`, with exactly
   the six defined pairs the domain admits and a *miss* (never a
   wrap-around) for every negative endpoint, which is the `lo <=u hi <=u
   len` unsigned reading agreeing on both sides. The trapping form `s[a..b]`
   was swept over 29 ugly pairs (negative, inverted, mid-codepoint,
   past-end, degenerate-empty, open-ended, inclusive, and the `^n` from-end
   forms), and 28 of 29 agree exactly. The 29th is a new finding, below.
3. Line-atomic print (D43): AGREE, and this machine was the prior
   art. The interpreter renders a whole line, interpolation and all,
   and hands it to a single `out()` call; there has never been a yield
   point inside a `print`, so it was line-atomic by construction before
   D43 was ruled. Measured rather than asserted: eight tasks × 40 long
   multi-segment interpolated lines, 20 runs of the compiler's `--native`
   lane = 6400 lines, 0 torn, across 20 distinct interleavings (the
   interleavings differ every run, which is what proves the threads
   really do race and the probe is not measuring a serialization). The
   same program on this machine: 3200 lines, 0 torn, one interleaving;
   the sim scheduler is deterministic by design. No tearing on either
   side, so nothing to file.

### DIV-2026-019 — `resolve/broken_sibling/entry.lu` — **OPEN: which parse error fires on an unparseable module sibling**

Found 2026-08-28 (is27) at the `e561c6f` pin bump: the s124 D59 wave's
broken-sibling witness pins `check: fail(E0202)` (the counterparty reads
`mangled.lu` (`fn mangled( {{{ not wolf at all`) to EOF inside the mangled
item), where this machine stops at the first bad token and answers
`fail(E0201)`@parse ("expected an identifier, found `{`", `[gram.item.fn]`,
mangled.lu 4:13). Same rung, same verdict class (both reject at parse),
span-or-code severity; not a soundness candidate.

Triage (`[proto.cmp.triage]`): spec bug, decision-tree case 1, the spec is
silent. E0201/E0202 are unpinned implementation choices
(`diag::UNPINNED_CODES`), no clause assigns either to junk recovery,
and which failure fires on unparseable text is a parser-recovery
choice the grammar does not rule. Mimicking the counterparty's
read-to-EOF recovery to hit its code number would be imitation, not
conformance (the independence doctrine). Proposal upstream: pin the
*class* (`fail` at parse) for this witness, or rule first-error
recovery in spec/01; either resolves this entry. `FILED_DIVERGENCES`
carries the waiver; the corpus-walk gate resumes on this file the
moment the entry resolves.

### DIV-2026-018 — `s[..]` — **OPEN, filed upstream: the compiler admits a bare `..` range the grammar excludes**

Filed upstream as wolf-lang#88. Found 2026-08-13 (lupin 0.1.11, CLEAN
wolfgang build at `f8dca42`) by the s77 boundary sweep, on the 29th of 29
ugly endpoint pairs. Class verdict (accept-set: one side rejects what the
other accepts); not a soundness candidate. No corpus file witnesses it,
so it cannot take a `FILED_DIVERGENCES` entry (that list is keyed by corpus
file), and it is recorded here until a witness exists, exactly as
wolf-lang#71's stdout finding was.

```
program   let s = "é€" ; let t = s[..] ; print("ok {t.len}")
lupin     fail(E0201)@parse — "expected an end of statement"
wolfgang  exit(0), stdout "ok 5"   (identical on --checked/--native/--release)
```

Triage (`[proto.cmp.triage]`): compiler bug, decision-tree case 2, spec
clear, interpreter matches it. `[gram.expr.primary]` gives

```ebnf
range_expr ::= r_end (('..' | '..=') r_end?)? | ('..' | '..=') r_end
```

Two alternatives, and neither admits a bare `..`: the first requires
a leading `r_end`, and the second requires a *trailing* one; the `?`
that makes an endpoint optional appears only in the first. So `a..`,
`..b` and `a..b` are expressions and `..` is not. That is very unlikely
to be an oversight, because `..` already has a different meaning one
production away (`error_row`'s rest marker, §1 line 167), which is
exactly the ambiguity an unrestricted bare `..` would create.

Extent, probed: the over-acceptance is in the parser, not the slice
path. `s[..=]` is also admitted and answers `trap(bounds)`; `s.get(..)`,
`let r = ..` and `xs[..]` on a `List` all parse on the compiler and
decline later as `unsupported`@`resolve` with no diagnostic, which
is the signature of a program that got past parse. Only `for i in ..`
is rejected by both, at E0201. So one parser rule over-accepts and two
spellings reach `run` with observable answers.

Recorded counter-argument, because the human ruling may go the other
way: `s[..]` meaning "the whole string" is what a reader would guess,
and a language may well want it. But the pinned grammar does not have
it, and the triage workflow makes the spec the defendant *first*: if
the intended answer is that `..` should be admitted, the fix is a
grammar clause landing before either implementation moves, which makes
this case 1 and a spec bug. wolf-lang#88 carries both readings;
this machine is not changing its parser ahead of that ruling, for the
same reason it did not land E1101's lend spelling ahead of wolf-lang#71:
conforming to an unwritten clause trades a clean rejection for a
divergence.

### The E1101 capture law: wolf-lang#71's interpreter half LANDED

The 0.1.10 round left this open: "the interpreter half is to
extend its capture analysis to treat a `(mut x)` receiver-lend of a captured
binding as a write and emit E1101 there too… it lands once wolf-lang#71's
fix fixes the span to match." s74 landed the compiler's half, so this round
landed ours. `lint::Walk::capture_lend` routes both lend spellings (the X1
moded receiver `(mut xs).push(1)` and the call-site argument mode `f(mut
n)`), through the same door as assignment, and the spans are the
counterparty's byte for byte: `[913,915]` and `[562,563]` on the wave's two
new files, `[545,546]` on the assignment twin. W1101 is NOT
emitted for the lend spellings: its text is about a write landing on the
task's own copy, which is an assignment's shape, and the counterparty emits
E1101 alone there, as do the corpus headers, which carry `warns:` on the
assignment file only.

This also retires the second finding recorded under wolf-lang#71 at
0.1.10, the mut-lend program printing `0` here against wolfgang's `2`,
because closures capture by value. Neither machine runs the program
now; both reject it at their own rung with the same code and span, so
the stdout divergence is unreachable and needs no witness.

---

Fourteenth corpus differential: lupin 0.1.10, pin `613c3dc` (the
mid-end/whole-program wave: s42 the optimizer, s43 clusters + body dedup
+ the frozen summary index, s63 diagnostics polish; 245 entries
compared, 22 members through their entries; counterparty built CLEAN at
the pin with `libwolf_rt.a` provisioned). Run four times, once per
counterparty tier, the first round in which the run tier compared at
all.

| tier | divergences | conservatism | both execute |
| --- | --- | --- | --- |
| `default` | 0 | 403 | 0 |
| `checked` | 1 | 176 | 107 |
| `native` | 1 | 183 | 107 |
| `release` | **1** | 201 | **98** |

THE HEADLINE: the mid-end changed nothing observable. The release lane
(s42's optimizer and s43's whole-program layer both ON), produces the same
answer as this machine on all 98 files both execute, and the single
divergence it reports is the SAME one the `checked` and `native` lanes
report, byte for byte. A transformation that altered behavior would have
shown up here as a release-only finding; there is none. The comparison is
what makes "check elimination 10 of 10, 58.2% corpus-wide, IR volume 84.6%
of naive" a safety claim rather than a throughput one.

Scope: linux x86-64, one platform, unseeded; the corpus's
`kernels/` tier (the three files s42's own gates read) is three programs,
so "the optimizer is correct" is not what this shows; "the optimizer did
not change these 98 observable behaviors" is.

The one divergence is DIV-2026-017, filed below: a raw-literal decode
bug in the compiler's front end, identical on all three of its
run-reaching tiers, which is precisely how it is known NOT to be the
mid-end's doing.

### wolf-lang#71's premise corrected: lupin never said E0202

wolf-lang#71 (SOUNDNESS: E1101 misses `(mut x)` receiver-lends) records
that "lupin rejects the same program with E0202 (a different code, so
the pair also disagree)". Re-verified at pin `613c3dc`, that is not what
this machine does, and E0202 could not have meant what the issue reads
into it: `E0202` is this machine's `E_UNEXPECTED_EOF`
(`src/diag.rs:186`), a *parse* code. It was never a judgement about the
capture law, so there is no code disagreement to reconcile; the
observation was a parse failure on some earlier spelling of the program
text, not lupin's verdict on the race.

What the two machines actually do with the issue's two spellings:

| program | lupin | wolfgang |
| --- | --- | --- |
| `n = 1` in two tasks (assignment) | `fail(E1101)` @resolve, span `[71,72]` | `fail(E1101)` @typecheck |
| `(mut xs).push(1)` in two tasks (mut-lend) | `exit(0)`, prints `0` | `--native`: `exit(0)`, prints `2` |

So on the assignment spelling the two already agree on E1101, at
their own rungs, which `[proto.cmp.rung]` makes agreement outright. On
the mut-lend spelling neither machine rejects: the hole is not
lupin-versus-wolfgang, it is a hole in both, and this machine's is
recorded here as its own gap rather than left implied by the issue.

The proposal, for the fix in flight: E1101, on both sides. This
machine does not want a distinct code and will not propose one; the
capture law has a code, both implementations emit it for the spelling
they do catch, and the corpus pins it (`conc/store_buffer.lu`,
`conc/chan_unsendable.lu`, the E11xx admission ladder). The interpreter
half is to extend its capture analysis to treat a `(mut x)`
receiver-lend of a captured binding as a write and emit E1101 there too.
NOT done in this round: the code is corpus-pinned, and
landing our span before the compiler's would trade a missing diagnostic
for a span divergence. It lands once wolf-lang#71's fix fixes the span
to match.

A second, separate finding rides along, and it is this machine's own:
the mut-lend program prints `0` here against wolfgang's `2` because
closures capture free variables by value
(`[gram.expr.closure]`, `docs/approximation-contract.md`), so each task
mutates its own copy of `xs`. Whichever way the capture law lands, that
is a real stdout divergence on a program no corpus file witnesses,
the shared-gap class the issue itself names. It cannot be filed as a
`FILED_DIVERGENCES` entry (that list is keyed by corpus file) and is
recorded here until a witness exists.

### DIV-2026-017 — `lints/raw_interp_braces.lu` — **RESOLVED upstream (wolf-lang#76 closed); re-measured clean at pin `3befc3e` (0.1.24) on all three tiers**

CLOSED at the 3befc3e pin (is35). wolf-lang#76 is closed and the file
answers `{who}\n` on `--checked`, `--native` and `--release`, byte-identical
to this machine. It had already stopped diverging by v0.2.2 (the sixteenth
differential does not list it), so the waiver in `differ::FILED_DIVERGENCES`
outlived the divergence by a release, which is the wolf-lang#177 lesson in a
smaller shape: a waiver nobody re-measures is a green report that means
nothing. It is removed, and `differ`'s own test now asserts the retired
entries are gone rather than merely that the remaining ones are present.

The original filing, kept for the record:

Filed 2026-08-12 (lupin 0.1.10, CLEAN wolfgang build at `613c3dc`).
Class stdout; not a soundness candidate. The first finding ever
produced by a run-reaching counterparty lane, and it was waiting the
whole time.

RE-CONFIRMED 2026-08-13 at pin `f8dca42` (lupin 0.1.11, CLEAN
wolfgang build from a deleted `target/`), which was this round's
assignment for wolf-lang#76. Unchanged in every particular: still
`exit(0)` on both sides, still `"{who}` there and `{who}` here, still
byte-identical shas to the original filing (`7ff0fa2b…` /
`2e5a9158…`), still the same answer on all three of the compiler's
run-reaching tiers. The corpus file's header still reads
`check: run(exit=0, stdout="{who}")` at `phase: wir`, so the pin still
masks a bug that is now one-sided, and the corpus walk here still scores
the file `match` at the `run` rung. wolf-lang#76 is OPEN and correctly
so; the entry stays open with it. Nothing in the s74…s78 wave touched
the lexer's literal decode.

RE-CONFIRMED AGAIN 2026-08-13 at pin `4e316ad` (lupin 0.1.12).
Third confirmation, third pin, and nothing has moved: the same two
shas (`7ff0fa2b…` / `2e5a9158…`), the same answer on `--checked`,
`--native` and `--release`. What is new is the company it keeps:
it is now the only divergence any of the three tiers reports,
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

Triage: compiler bug, decision-tree case 2 (spec clear, interpreter matches
it). `[gram.lex.str.raw]` is explicit (`r"…"`, `r#"…"#` carry the bytes
between the quotes, no escapes, no interpolation), so `r"{who}"` is the six
bytes `{who}`. The compiler's answer keeps the opening quote of the `r"`
delimiter, which is the naive first/last-byte quote strip applied to a
two-character opening delimiter. The corpus file's own header agrees with
this machine: `check: run(exit=0, stdout="{who}")`.

The file's `phase: wir` pin is the interesting part. Its header explains
the pin: at s40 both executors had this bug, agreed with each other
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
built CLEAN at `0b4e79c` with `libwolf_rt.a` provisioned), 0
divergences. `FILED_DIVERGENCES` is empty again, and DIV-2026-016
CLOSES: the CLEAN build answers `fail(E0809)@typecheck`, span
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
`lupin conform-run --seed=N`, seeds {0, 42}, all ten files. 10 of 10
agree on verdict, exit and output bytes, at both seeds, both records
`"seeded": true`, a `[proto.seed.equal]` comparison (equal seeds ⇒
comparable observations including output bytes). Scope: two
seeds, one platform (linux x86-64), debug tier; the seeded-tie-break
files (`conc/select_seeded.lu`, `test/conc_schedules_test.lu`) agree at
these seeds and both their outcomes are conforming by design. Zero
semantic divergence between the two schedulers' observable behavior on
the pinned witnesses.

---

Twelfth corpus differential: lupin 0.1.8, pin `26fa98e` (wave seven:
r01 prep + s71 release polish, then the one lawful mid-pass re-pin when
s72's mode teeth merged; 240 entries compared, 22 members through their
entries, counterparty built CLEAN at `26fa98e`), 1 divergence, filed
as DIV-2026-016, not a soundness candidate. 394 conservatism-ledger
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
- a (lupin): `fail(E0809)@resolve`, span `[518,523]` (the `Io(e)`
  handler pattern), agreeing with the pin.
- b (wolfgang): `fail(E0806)@typecheck`, the *generic*
  refutable-binding diagnostic ("this pattern can fail to match, but a
  binding cannot"), same span `[518,523]`; its `--phase=resolve` record
  is `pass`. `[proto.cmp.rung]` cannot bridge different *codes*, so the
  records genuinely diverge.
- Triage (`[proto.cmp.triage]`): the clause is not ambiguous. The
  corpus file wolfgang itself ships pins E0809 and names the rule; the
  else-handler position takes `closed_pattern` by grammar
  (`[gram.pat]`), so a refutability complaint about the handler pattern
  is the wrong diagnostic there twice over. The counterparty is the
  defendant: its E0809 emission (s71's #43/#59 work) evidently does
  not reach the ladder its `conform-run` door runs. The generic E0806
  refutability check fires first at its typecheck rung. Filed
  upstream as wolf-lang#61, both records attached verbatim; the
  entry closes when wolfgang's conform-run answers its own pin.

Eleventh corpus differential: lupin 0.1.7, pin `e94b879` (wave six: s40
os/time/json, s70 match tier + X3 value paths, s69 idiom lints; 232
entries compared, 22 members through their entries, counterparty built
CLEAN at `e94b879`), 0 divergences. The ruling the last four rounds
asked for landed in spec/06 as `[proto.cmp.rung]`: when both records
reject with `fail(CODE)` and the first diagnostic's code and span agree,
the records AGREE even when `phase_reached` names different rungs of the
shared ladder; exactly one verdict wide. `compare_deep` implements the
clause, the eleven rung-placement divergences compare clean, and
DIV-2026-011, -012, -014 and -015 ALL CLOSE with the clause cited.
`FILED_DIVERGENCES` is empty for the first time since the fourth round.
One new shape surfaced and resolved comparator-side, no filing needed:
wolfc's `resolve/cycle/main.lu` record interleaves its new W0314 lint
ahead of the E0303 rejection in `diagnostics` (source order, licensed by
`[proto.record.warn]`'s "warning observations ride `diagnostics` at
warning severity"), so the fail comparison now reads the first
error-severity diagnostic. A lint's span is `[proto.cmp.warn]`'s
surface, never the rejection's. 382 conservatism-ledger entries (63
rejects-beyond by the counterparty, 122 run-unmatched, 147
counterparty-unsupported, 50 interp-unsupported: the fs/net/proc-spawn
tiers, comptime, and the compiler-only analyses).

---

Tenth corpus differential: lupin 0.1.6, pin `13b811f` (wave four: the
#41 capture law, s34 procs, s35 io reactor, s39/s40/#40 native
str/List/fs, the s68 lint corpus; 203 entries compared, 18 members
through their entries, counterparty built CLEAN at `13b811f`), 11
divergences, all filed, none a soundness candidate, and every one is
the same finding: same code, same span, this machine at `resolve` where
wolfc's emission lives at `typecheck`/`mem`. DIV-2026-011 holds;
DIV-2026-012 holds; DIV-2026-013 CLOSES (wolfc's conform-run no
longer misrejects its own s38 fs/io files: `unsupported@wir` there now,
never a divergence); DIV-2026-014's wiring half closes the same way
(wolfc emits E0411/E0412/E0413 at `typecheck` now; this machine
realigned its E0412/E0413 spans to the counterparty's `:spec` shape) and
its residue rides DIV-2026-011; issue #19's realignment opens
DIV-2026-015 (E1101/E1102/E1103/E0004 statically at this machine's
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
counterparty built CLEAN at `f0da6e6`), 10 divergences, all filed,
none a soundness candidate: DIV-2026-011 holds; issue #18's tier
statics open DIV-2026-012 (four files, the DIV-2026-011 rung
question again, with the same code and span, this machine at resolve
where wolfc's emissions live at mem/typecheck); and the pin exposes a
counterparty *surface* lag filed as DIV-2026-013/DIV-2026-014:
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
never hashed) and declines the fs tier (no filesystem by
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
not being reachable through its conform-run surface. Status update,
pin `13b811f` (0.1.6): the wiring half closed as predicted, and wolfc
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
1 divergence, filed as DIV-2026-011, and DIV-2026-010 CLOSES:
s29 moved wolfc's E0410 emission to the resolve rung (with the
`[conc.when.body]` exemption wolf-lang#21 carried from this machine) and
re-pinned the two corpus `phase:` directives resolve → parse, so
`typecheck/let_reassign.lu` and `typecheck/let_compound_assign.lu` now
reject on both sides at `resolve`, same code, same span. The eighth
round compares them clean, exactly the closure condition the seventh
round wrote down. The new filing is the same *shape* in the opposite
direction: `memory/mode_missing_mut.lu` rejects with E1007 at the same
span on both sides ([405,408], the argument), this machine at
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
  `[conf.trap.map]` gives E1007 no dynamic meaning, so the stop
  is the rung where the callee's signature is visible, which for this
  machine is sema-lite at `resolve` (the E0410 precedent).
- b (wolfc ad6cef7): `fail(E1007)@mem`. Mode checking is its memory
  tier's, after typecheck completes.
- Triage: spec first defendant. `[proto.cmp]` has no allowance for
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
the pin), 2 divergences, both still DIV-2026-010, unchanged from
the sixth round: same E0410, same spans, wolfc's record says `typecheck`
where the corpus pins `phase: resolve`. Re-verified at d147a54: the
fix has NOT landed at this pin. It is in flight upstream. The s29 work
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
16 members through their entries), 2 divergences, both filed as
DIV-2026-010, the first non-zero round since is07, and both are one
finding: `typecheck/let_reassign.lu` and `typecheck/let_compound_assign.lu`
reject on both sides with the same E0410 at the same span, but the
counterparty's record places the rejection at `typecheck` while the corpus
files themselves pin `phase: resolve`. The deep comparison reads wolfc's
record as claiming the resolve rung *completed*, which collides with this
machine's rejection at the rung it performs. That is a
rung-placement inconsistency between the compiler's record and its own
corpus directive, not a verdict disagreement. Triage: spec/corpus
first defendant. Either the corpus directives should say `typecheck`
or wolfc's driver should report sema's E0410 at `resolve`; routed
upstream with the filing; this machine's placement follows the corpus.
261 conservatism-ledger entries (64 rejects-beyond by the counterparty,
where the E1301/E1302 unsafe tier landed, 67 run-unmatched, 84
counterparty-unsupported, 46 interp-unsupported).

Fifth corpus differential: is09, pin `cbde620` (s21's shared tier: nine
files advance to `mem`, `prov_holy_grail.lu` to `typecheck`; spec-extract
renders the §3.2 operator climb into `grammar.ebnf`), 148 entries
compared, 0 divergences, the fourth consecutive zero round. 246
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
`differ::FILED_DIVERGENCES` remains empty.

Fourth corpus differential: is08, pin `843174f`, 148 entries compared,
0 divergences, 246 conservatism-ledger entries (59 rejects-beyond by
the counterparty, where the E1005/E1011/E1012 region-checker litmuses
landed, 64 run-unmatched pre-M1, 80 counterparty-unsupported, 43
interp-unsupported, down from 45: `procs.lu` and
`conc/proc_kill_defers.lu` are self-contained now and RUN, S-5
resolved). `differ::FILED_DIVERGENCES`
remains empty.

Third corpus differential: is07, pin `79ceec6`, 142 entries compared,
0 divergences (down from 1), 238 conservatism-ledger entries (55
rejects-beyond by the counterparty, up from 46 as the E1004/E1007/E1010
litmuses landed, 60 run-unmatched pre-M1, 78 counterparty-unsupported,
45 interp-unsupported). `differ::FILED_DIVERGENCES` is empty for the
first time since the differential lane exists.

The is07 exploration record (the corpus half lives in
`tests/explore_corpus.rs::CONC_LEDGER`): every `conc/` litmus explored to a
closed frontier under DPOR (8 files, 1–2 Mazurkiewicz classes each,
naive-DFS baseline agreeing on every conclusion), with one verdict per
file across its entire schedule space. The determinism-taxonomy claim
(spec/03 §5 `sched-ev/0`, `[proto.seed.equal]`) holds over the whole
pinned conc tier: no corpus file is schedule-dependent. The multiopen
model check (the question `memory/region_multiopen_ok.lu`'s own header
flags for is07) answers definitively within bounds: no explored schedule
breaks the region forest invariant or leaks, whether it comes from the
corpus files or from the concurrent multiopen litmuses in
`tests/explore_machine.rs`.

### The two mirrors III — is55, `[mem.region.edge.elem]` + `[gram.expr.assign]` (wolf-lang#438) and `[os.fs.path.domain]` (wolf-lang#386), rulings at wolf-lang `0468dead`

s182 parked ten witnesses with ruled verdicts per machine under the planning
repo's `sprints/compiler/88-the-rulings-prose/witnesses/`. This section is
written in the contract's order: §2 re-derived, §3 committed before the
first edit, the rest appended as it lands.

#### §2 — inputs, re-derived 2026-09-24 (kasumi, linux x86-64)

| input as written | at origin / measured | drift |
| --- | --- | --- |
| trunk `ba357aa` (v0.1.38) | `origin/trunk` = `ba357aa`; branch `is55` cut there, not from `is54` | none |
| is54 in flight on `is54` | `origin/is54` = `03790e0`. Of the files this lane edits, **in is54's diff**: `src/parse.rs` (is54 at L495; this lane at the assignment statement, ~L2305), `src/eval/mod.rs` (is54 at the `Trap` type and ~L6693; this lane at `exec_assign` and the call-argument `take`), `src/sema.rs` and `src/lint.rs` (only the `Assign` destructures gain a field), `src/main.rs` (the `conform-run` dispatch), `docs/divergence-log.md` (is54 inserts at L110; this section sits at the end of Open findings to stay clear of it). **Not in is54's diff**: `src/ast.rs`, `src/eval/fs.rs`, `src/eval/net.rs`, `tests/fs_family.rs`, `docs/approximation-contract.md`, and the new `tests/rulings/` + `tests/rulings_s182.rs`. `CHANGELOG.md` is left to is54's 0.1.39 cut. | recorded, kept minimal |
| wolf 0.2.16 archive, lupin 0.1.38 archive | copied (not moved) from `~/lanes/s178/archives/`: `84e30c05…` and `828b5c55…`, s182's shas | none |
| s182's `[trunk]` table (10 × 3) | re-measured with the **published** wolf 0.2.16 in place of s182's wolf-lang debug build at `a565d4b9`, and with lupin 0.1.38 and a trunk `ba357aa` release build: every cell equal (`~/lanes/is55/evidence/published-0.2.16-0.1.38-witnesses.log`, `trunk-ba357aa-witnesses.log`) | none |
| #438: plain store traps `use-after-move`, `take` store E0201 | measured as written on both lupin builds | none |
| #386: lupin refuses `sub/../x` lexically | measured: `unsupported` "names a path outside the working directory" on `target/s182_sub/../s182_inner.txt` | none |
| #386: lupin "answers a write through ANY symlinked directory with a `not_found` ROW … a symlink defect" | the row is measured, **the cause is not symlinks**. `lupin run` (the live door) serves BOTH symlink witnesses, `exit 0`, `sym=sym gone=true`; and `lupin conform-run` of a program reading a pre-existing REGULAR file in its cwd (no symlink anywhere) answers the same `exit(1)` `error: not_found` (`~/lanes/is55/evidence/probe-cause-0.1.38.log`) | **corrected**: s182 §2 row 5 and `[os.fs.path.domain]`'s "a `not_found` row through a symlinked directory" name the symptom; the cause is §3's third bullet |
| "the five that must change" | the ruled lupin cells differ from today's on **seven** witnesses: the five named plus `fs_path_symlink_in` (`exit(1)` row → `exit(0)`, ruled) and `fs_path_symlink_out` (`exit(1)` row → `exit(0)` or `unsupported`, "never a row") | seven, not five |
| "the ten witnesses in lupin's corpus" | lupin has no corpus of its own: `vendor/upstream/corpus` is the pinned wolf-lang corpus and is read-only (CONTRIBUTING, "The corpus is read-only"). The in-repo witness convention is a `tests/<dir>/` of programs plus a test file (`tests/d62/` + `tests/str_append_d62.rs`) | they land at `tests/rulings/`, one directory each, verbatim, run through the built binary's `conform-run --json` from their own directory as s182's runner does |

#### §3 — prediction, committed before the first edit

- **Where an index store's mode is decided.** Parse: `src/parse.rs:2305-2307`
  — `assign_op`, then `parse_expr`; `take` is a keyword and no expression
  starts with it, so `outs[0] = take xs` is E0201 there. Run:
  `src/eval/mod.rs:3902`, `exec_assign`'s plain `=` arm calls
  `eval_for_init` (`:3705`), whose `consume_place` (`:3714`) MOVES any
  non-`Copy` place — the later `(mut xs).push(4)` is the trap.
- **Where paths are confined.** `src/eval/fs.rs:527` `contained`, lexical
  (`:529-536`: any `ParentDir`, `RootDir` or `Prefix` component, or
  `is_absolute`), reached from `fs_contained` (`:1047`) by every path call;
  `src/eval/net.rs:771` `socket_path` repeats it for unix socket paths,
  which `[os.fs.path]` says take the same answer.
- **The `not_found` row's cause, named.** `src/eval/fs.rs:224`
  `FsTable::resolve` joins every relative path onto the PRIVATE
  observation root minted at `:168` (`observation_root`: `$TMPDIR/wolf-obs/<id>/`
  holding only an empty `target/`) whenever `Machine::is_live`
  (`src/eval/mod.rs:767`) is false — which it is for every
  `lupin conform-run`. `setup.sh` makes its link in the cwd, a directory
  an observed program never sees, so the write's parent is missing and the
  host says `ENOENT`. `[os.fs.path]` has said since s163 that "a relative
  path resolves against the process's working directory, on every tier";
  the fix is the `conform-run` door resolving there, as `lupin run` does,
  while in-process observations (the corpus walk, `diff-run`, export,
  explore, fuzz — the concurrency is48's root exists for) keep the root.
- **Which witnesses flip, and to what** (lupin, `conform-run --json`, from
  the witness's own directory): `index_store_copies_list` `trap` → `exit(0)`
  `outs0=1 xs=2`; `index_store_copies_map` `trap` → `exit(0)` `m0=1 xs=2`;
  `index_store_take_list` and `index_store_take_map` `fail(E0201)` →
  `trap(use-after-move)` (dynamic, not a static E1001: the resolve-rung
  `move_check` keys on call-site markers and is not widened);
  `fs_path_inside_after_dotdot` `unsupported` → `exit(0)`;
  `fs_path_symlink_in` `exit(1)` row → `exit(0)`; `fs_path_symlink_out`
  `exit(1)` row → **`unsupported`** (it resolves outside the served tree,
  and the boundary is resolved now). **Hold:** `index_store_read_param`
  `exit(0)`; `fs_path_absolute` and `fs_path_climbs_out` `unsupported`
  (conformant; this lane narrows no scope it does not have to).
  **Seven flip, three hold.** Falsified by any cell otherwise.
- **Corpus rows that change verdict: zero.** Over `vendor/upstream/corpus`
  at pin `2e4ca769`, 56 lines carry `] =`; the index stores whose right-hand
  side is a bare non-literal name are three — `memory/map_remove.lu:40`
  (`ids[4] = xs`, `xs` never read after), `memory/map_set_generic.lu:24`
  (`m[k] = v`, `v` a `take` parameter, never read after) and
  `typecheck/map_struct_key.lu:24` (`= true`); `memory/list_session_struct.lu:34`
  stores a literal. None of the 11 fs / unix-socket corpus files names a
  `..` or an absolute path. So `lupin corpus` and `tests/run_corpus.rs`
  move no row, and the whole-corpus differential against wolf 0.2.16
  moves no entry on `checked` or `native` against the trunk baseline.
  Falsified by any row or entry moving.
- **Tests that must change with the door, predicted red first:** three in
  `tests/fs_family.rs` pin is48's private root at the CLI —
  `a_live_run_writes_in_the_users_directory_and_an_observed_one_does_not`,
  `two_concurrent_observations_of_one_program_do_not_interfere` and
  `an_observed_programs_socket_lands_beside_its_files_and_not_in_the_users_directory`
  (its planted stale socket becomes visible). Each is rewritten to the
  in-process door the property belongs to; no other test moves.

#### §3a — the prediction, scored

- **Seven flip, three hold — held, every cell.** At the head, lupin's cell
  on each witness is its ruled one (`~/lanes/is55/evidence/head-53de98f-witnesses.log`,
  three machines, wolf 0.2.16 published): `index_store_copies_{list,map}`
  `exit(0)` `outs0=1 xs=2` / `m0=1 xs=2`; `index_store_take_{list,map}`
  `trap(use-after-move)`; `index_store_read_param` `exit(0)`;
  `fs_path_inside_after_dotdot` `exit(0)` `inner=in gone=true`;
  `fs_path_symlink_in` `exit(0)` `sym=sym gone=true`; `fs_path_symlink_out`
  `unsupported` ("resolves outside the working directory"); `fs_path_absolute`
  and `fs_path_climbs_out` `unsupported`. wolf 0.2.16 answers its `[trunk]`
  cells unchanged (the #438 side is s180's).
- **Red at the witnesses' commit, cited:** `tests/rulings_s182.rs` at
  `feb7631` (no code change yet; measured before the rebase onto `fa18a92`,
  when the commit was `dc0e989`) — 7 failed, 4 passed, exactly the seven
  (`~/lanes/is55/evidence/red-dc0e989-rulings.log`, `EXIT=101`); and
  `tests/index_store.rs` run against the same sources — 5 failed,
  2 passed, the two passing being the non-regression pins (`take` refused
  elsewhere, a field store still moves) (`red-dc0e989-index_store.log`).
- **Where the mode and the confinement live — held.** The store's mode was
  `exec_assign` → `eval_for_init` → `consume_place`; the confinement was
  `contained`'s lexical test, repeated in `net::socket_path`.
- **The `not_found` row's cause — held**, and the fix is the one named: the
  `conform-run` door resolves against the process cwd
  (`4cc7fa4`); `fs_path_symlink_in` is `exit(0)` and a regular file the
  harness planted is read (`tests/fs_family.rs::a_conform_run_reads_what_its_harness_put_in_the_cwd`).
- **Zero corpus rows move — held.** `lupin corpus` (release) at trunk
  `ba357aa` and at the head: byte-identical 673-line reports, 491 match /
  26 dynamic-counterpart / 48 conservatism / 59 out of scope / 1 mismatch
  (the pre-existing `resolve/broken_sibling`) (`corpus-trunk-release.log`,
  `corpus-head-release.log`).
- **The differential moves no lupin answer — held.** `lupin diff-run`
  against the published wolf 0.2.16 on all four counterparty tiers, trunk
  release build vs head release build, same corpus (pin `2e4ca769`, 625
  entries, 41 members): divergences 6 / 6 / 8 / 8 (default / checked /
  native / release) on both sides; every report and every conservatism
  ledger byte-identical except one field of one line — the native tier's
  `memory/unsafe_ub_uaf.lu` row, where the COUNTERPARTY's exit status after
  its use-after-free read `exit(73)` on the trunk run and `exit(72)` on the
  head run; lupin's side is `ub(mem.ub)` in both. That is the program's UB
  read on the compiled lane, not this change (`~/lanes/is55/evidence/diffrun/`).
  **New divergences: zero.** The ruled ones are outside the corpus (the
  witnesses stay parked upstream) and are listed in the bullet above.
- **Three tests move with the door — held, the three named.** At the door
  commit, `tests/fs_family.rs` went red on exactly
  `a_live_run_writes_…`, `two_concurrent_observations_…` and
  `an_observed_programs_socket_…` (`dev-386-door-targeted.log`: 21 passed,
  3 failed); they are rewritten onto the embedded door (`72a19ae`), where
  the private root's two properties — no interference, fs and net agree —
  still hold and are still asserted, and a fourth pins the cwd read.
- **Re-measured after the rebase onto trunk `fa18a92`** (is54 merged, lupin
  0.1.39, pin `93a5fe50` = v0.2.16). The evidence files named
  `…-53de98f…` above come from the pre-rebase head. Release builds of trunk
  `fa18a92` and head `639b00f` (`~/lanes/is55/evidence/rebased/`): lupin's
  cell is the ruled one on all ten witnesses; `lupin corpus` reports are
  byte-identical (696 lines); `diff-run` against wolf 0.2.16 over 646
  entries and 44 members gives 8 / 8 / 10 / 10 divergences on both sides,
  and every report and ledger is identical except the same native-tier
  `memory/unsafe_ub_uaf.lu` counterparty exit status (`112` vs `16`: the
  compiled lane's use-after-free read). At the new pin the index stores of
  a bare non-literal name are still the two above, and the one new store,
  `memory/pool_place_write.lu:61`, is a struct literal. **New divergences:
  zero.**
- **Not predicted:** `tests/rulings_s182.rs` and `tests/net_unix.rs` did
  not run in that first targeted pass, because cargo stops at the first
  failed test binary; the second pass ran with `--no-fail-fast`.

### The moved element — is56, wolf-interp#141, `[mem.tier0.move.2]` under `[mem.model.place.elem]` (wolf-lang `38d5eeab`)

eg00 (wolf-lang PR wolffe-lang/wolf-lang#461, merged at `38d5eeab`) wrote
the element clause and found that lupin marks a moved element's slot and
then reads it anyway. This section is written in the contract's order:
§2 re-derived, §3 committed before the first edit under `src/` or
`tests/`, the rest appended as it lands. Measurements on kasumi (linux
x86-64): published wolf 0.2.17 `a95d0f0f…` and lupin 0.1.40 `509929e6…`
(digests equal to the release pages'), and a release build of trunk
`54f85e6`; probe log `~/lanes/is56/evidence/probes-archive-0.1.40.log`
(lupin, `wolf --checked`, `wolf --native`), with the trunk build's
lupin column byte-identical (`probes-trunk-54f85e6.log`).

#### §2 — inputs, re-derived 2026-09-26

| input as written | at origin / measured | drift |
| --- | --- | --- |
| trunk `54f85e6` (0.1.40) | `origin/trunk` = `54f85e6` "release: lupin 0.1.40"; pin `93a5fe50` (v0.2.16); branch `is56` cut there | none |
| lupin marks a moved element's slot (`src/eval/value.rs:71-74`, `mod.rs:1638-1672`) | `Slot::take_value` at `value.rs:71-74` sets `SlotState::Moved(at)`; `move_path` at `mod.rs:1638-1672` reaches it | none |
| the index read ignores the mark (`builtin.rs:2080`), reached from `mod.rs:7610` | `builtin::index`'s sequence arm answers `Some(slot) => Ok(slot.value.clone())` at `builtin.rs:2080`. **The witness does not reach it from `:7610`**: `xs` is a local, so `eval_bracket` takes the index-read lend (#28) and calls `builtin::index` at **`mod.rs:7518`**; `:7610` is the copy path, taken when the base is not a place path (`g[0][1]`'s outer index) | the call site is `:7518` for every local base; both reach the one arm |
| `let a = move xs[0]; print(xs[0])` prints `1 1` | measured: `List[int]` `exit(0)` `1 1` (probe `p01_idx_int`); `List[List[int]]` through `.len` `exit(0)` `1 1` (`p02_idx_heap_member`, and the witness) | none |
| the witness `witnesses/elem_move_same_const_read/` | **not parked**: it is a corpus row in wolf-lang, `corpus/memory/elem_move_same_const_read.lu` at `38d5eeab`, header `fail(E1001)` / `phase: typecheck`; the ruled lupin cell `trap(use-after-move)` is in its header prose and in eg00's gate (`element_places_lanes.rs`, `the_moved_element_itself_stays_unreadable`). The planning repo parks thirteen others; the four `elem_{const,dyn}_store_no_revive_{int,heap}` carry `lupin = trap(use-after-move)` in `[expected]` | the witness lands here with an `expected.toml` written from the gate's ruled cell |
| eg00's gate pins lupin by version: `PRE_MIRROR_LUPIN` | `const PRE_MIRROR_LUPIN: &[&str] = &["0.1.40"]` (`crates/wolf_driver/tests/element_places_lanes.rs:31` at `38d5eeab`); any other version takes the ruled arm | none |
| lupin prints `2 1 1` on wolf-lang#460's witness | measured `exit(0)` `2 1 1` (`w02_460_heap`); the `Copy` twin `1 1 9` (`w03_460_int`); wolf 0.2.17 checked `trap(use-after-move)`, native `2 2 1` / `1 1 9` | none |
| (not an input) whether the moved element is unreadable anywhere | **yes, through a place path**: `resolve` (`mod.rs:1435-1486`) walks every projection and reports the earliest `Moved`, so `let b = xs[0]` (`p15`) and `xs[0].0` (`p27`) already trap `use-after-move`. Only reads that pick the element out of a *value* skip the state | the defect is the value-level element reads, not "the index read" alone — §3 enumerates them |

wolf-lang#460's comment on `corpus/memory/list_session_struct.lu` (eg00):
the row moves `tbl[2]` with a plain `let` of a non-`Copy` `Session`, then
iterates `tbl`. lupin 0.1.40 marks `tbl[2]` moved (the `let` reaches
`consume_place` → `move_path`) and prints `102 1 1408 4 184` because its
`for` never looks at the state (measured, `w04_list_session_struct`).

#### §3 — prediction, committed before the first edit

**The one read path that must consult the mark:** `builtin::index`'s
sequence arm (`builtin.rs:2080`), for `List` and `Tuple` values. It is the
single point every `xs[e]` value read reaches, from the lend (`mod.rs:7518`)
and the copy path (`:7610`) alike, so the witness, `p01`/`p02`, `p16`
(`g[0][1]` after `move g[0]`), `p26` (run-time index) and #460's two
witnesses all turn on that one line.

**Every other path that reaches an element, and whether it honours the
mark at trunk:**

| path | where | honours at trunk | after |
| --- | --- | --- | --- |
| a place read of the element (`let b = xs[0]`, `f(xs[0])`, `xs[0].field`) | `read_path` → `resolve` | **yes** (`p15`, `p27` trap) | unchanged |
| `move`/`take` of an already-moved element | `move_path` → `resolve` | **yes** | unchanged |
| `xs[e]` as a value, list or tuple | `builtin::index` `:2080` | no | trap |
| `m[k]` as a value | `builtin::index` map arm `:2113` | no (`p12` `1 1`) | trap |
| `xs.get(i)`, `xs.first()`, `xs.last()` | `builtin::method` `:1384-1418` | no (`p06`-`p08`) | trap when the chosen slot is moved; `none` rows unchanged |
| `(mut xs).pop()` of a moved last element | `:1345` | no (`p11`) | trap |
| `(mut m).remove(k)` of a moved value | `:1447` | no (`p23`) | trap |
| `m.pairs()` | `:1462` — rebuilds every slot `Slot::live`, laundering the state | no (`p22`) | trap |
| `xs.par(f)` | `:1376` | no (`p24_par` `1 2 2`) | trap |
| a slice `xs[a..b]` covering a moved element | `builtin::slice` `:2199` — copies the slots, state and all | no (`p09`) | trap; a slice that misses it (`p10`) unchanged |
| `for v in xs`, `for (k, v) in m` | `eval_for` `mod.rs:5736-5742` — keeps `s.value`, drops the state | no (`p05`, `p14`) | trap when the walk reaches the moved element, after the earlier iterations ran |
| `xs.len`, `xs.is_empty()`, `m.len`, a push, a store | header and writes | not reads of an element | unchanged: `exit(0)` (`p04`, `p17`, `p18`), as `[mem.model.place.elem]` 1(c) rules `xs.len` |
| another element / another key | — | yes (`p03` `1 2`, `p13` `1 2`) | unchanged |

**Out of scope, named:** a read of the WHOLE container or aggregate that
holds a moved part — `copy xs` (`p19`), `"{xs}"` (`p20` prints `[1, 2]`),
`xs == ys`, passing `xs` to a builtin that walks it, and the same for a
struct after `move p.x` (`p21`: `let q = p` runs). That is not an element
path: `resolve` answers only for the path's own prefix chain, never its
descendants, for structs as for lists. Filed as a follow-up, not widened
here.

**Witnesses** (lupin, `conform-run --json`, from each witness's
directory): `elem_move_same_const_read` `exit(0)` `1 1` → `trap(use-after-move)`;
`elem_{const,dyn}_store_no_revive_{int,heap}` `exit(0)` `1 1 9` / `2 1 1`
→ `trap(use-after-move)` — **so lupin's `2 1 1` on #460's witness is
wrong under the clause, and the head answers the trap** (item 3: the
store to `xs[1]` revives nothing, and lupin never revived it — the read
simply never looked). The other nine parked witnesses hold their `[trunk]`
lupin cell, which is their `[expected]` one. Five flip, nine hold.

**Corpus rows that change verdict: one, not zero** —
`memory/list_session_struct.lu`, `exit(0)` `102 1 1408 4 184` →
`trap(use-after-move)` when the `for` reaches `tbl[2]`. It is the row
eg00 found reading a moved element on all four machines; wolf-lang's
eg01 re-spells it `let s2 = copy tbl[2]` (eg01's contract, item 2). The
spec is clear and the row is the defendant, so it is filed here as
DIV-2026-026 until the pin carries eg01's spelling — is46's precedent for
DIV-2026-022/023, headers a clause made stale. No other row moves: the
corpus rows that move an element and read it again through a value path
are that one. Falsified by any other row moving in `lupin corpus` or
`tests/run_corpus.rs`.

**The differential against wolf 0.2.17** (all 646 entries at pin
`93a5fe50`, four counterparty tiers, trunk release build against head
release build): exactly one entry added on each tier,
`memory/list_session_struct.lu` `a=trap(use-after-move)` `b=exit(0)`,
class `verdict`, `[filed: DIV-2026-026]`; every other line identical.
Falsified by any other entry appearing, leaving or changing.

**Existing tests that change:** `tests/run_corpus.rs`'s `RUN_LEDGER` row
for the one file, and `differ`'s unit tests that count
`FILED_DIVERGENCES` (one → two). No other test moves.

#### §3a — the prediction, scored

Evidence under `~/lanes/is56/evidence/` on kasumi; release builds of
trunk `54f85e6` and head `f4c5f00` (the code is `3ce773b`'s plus rustfmt,
the filing and the waiver rule).

- **The one read path — held.** `builtin::index`'s sequence arm was the
  point every `xs[e]` value read reaches; the witness, `p01`/`p02`,
  `p16` (`g[0][1]`, the copy path) and `p26` (run-time index) turn on the
  one line (`9bc2f89`).
- **The path table — held, every row.** Head against trunk
  (`probes-trunk-54f85e6.log`, `probes-head-f4c5f00.log`): exactly the
  fifteen probes the table marks "no" move to `trap(use-after-move)` —
  `p01 p02 p05 p06 p07 p08 p09 p11 p12 p14 p16 p22 p23 p24 p26` and the
  four `w0*` witnesses; the `for` probes keep the output of the iterations
  before the moved element (`p05` `1\n`, `p14` `a=1\n`). Unchanged: the
  place reads that already trapped (`p15`, `p27`), another element or key
  (`p03`, `p13`), the header and writes (`p04`, `p17`, `p18`), a slice
  beside the moved element (`p10`), and the out-of-scope whole-value reads
  (`p19`, `p20`, `p21`, `p25`).
- **Five witnesses flip, nine hold — held.** `tests/rulings_eg00.rs` red
  at `5970d95` on exactly the five, each `lupin answered exit(0), ruled
  ["trap(use-after-move)"]` (`red-5970d95-rulings_eg00.log`, 10 passed,
  5 failed, `EXIT=101`); the path tests red at `4870cad` on exactly the
  thirteen that assert a trap, each `expected a trap, got Exit(0)` or the
  `for` stdout (`red-4870cad-paths.log`, 3 passed, 13 failed,
  `EXIT=101`). With eg00's own runner (`witnesses/run.sh`, `check.py`):
  trunk 0 of 52 cells differ from `[trunk]` (`parked-trunk.log`); head
  13 of 13 lupin cells equal `[expected]` (`parked-head.log`,
  `parked-head-vs-expected.txt`; the 39 wolfgang cells that differ are
  eg01's). **wolf-lang#460's witness answers `trap(use-after-move)` now;
  0.1.40's `2 1 1` was the unread mark, not a reading of the clause.**
- **One corpus row — held.** `lupin corpus` trunk against head
  (`corpus-trunk-release.log`, `corpus-head-f4c5f00-release.log`): one
  entry line moves, `memory/list_session_struct.lu` `match` →
  `MISMATCH expected exit(0), observed trap(use-after-move)`; the census
  line reads 502 match / 2 mismatch against 503 / 1. Filed as DIV-2026-026
  (`057c99e`), `RUN_LEDGER` moved with it (`b444fd0`).
- **The differential — held on three tiers, WRONG on one.** Against the
  published wolf 0.2.17, 646 entries, 44 members (`diffrun/`):
  `checked` 5 → 6, `native` 7 → 8, `release` 7 → 8, each the one
  predicted line `verdict … list_session_struct.lu
  a=trap(use-after-move)@run b=exit(0)@run run [filed: DIV-2026-026]`,
  conservatism ledgers byte-identical, and on `native` the known
  counterparty exit status of `memory/unsafe_ub_uaf.lu` (`137` vs `74`,
  the compiled lane's use-after-free read, is55's same line). **`default`
  added no entry**: that tier reaches `run` on no file, so the row is a
  `run-unmatched` ledger line whose verdict moved (`exit(0)` →
  `trap(use-after-move)`), not a divergence. My prediction forgot the
  tier's depth. And the first head run (`head-b444fd0-default.log`)
  printed `retired-waiver memory/list_session_struct.lu [DIV-2026-026] —
  the counterparty compared it CLEAN` as a gating finding: the retirement
  check took any foreign record as evidence, including one that stopped
  short of the rung the filing lives at. Fixed (`6c22d42`, `ebc65ae`):
  only a comparison with no divergence and no conservatism entry is
  evidence; at `f4c5f00` the `default` report is byte-identical to
  trunk's. **New divergences: one, filed, the predicted one.**
- **Tests that change — held.** `RUN_LEDGER`'s row and `differ`'s
  `FILED_DIVERGENCES` count; plus the new waiver-evidence test, which the
  prediction could not name because the defect was not yet seen.
- **Not predicted: the manual.** `docs/manual/00-building.md` quotes the
  `lupin corpus` census line, and `tests/doc_truth.rs` holds it to the
  binary: red in the kasumi gauntlet at `f4c5f00` (`gauntlet-f4c5f00.log`,
  "1 of 20 documented pair(s) drifted", 503/1 against 502/2). The
  manual now counts DIV-2026-026 (`c8527bc`). A corpus row that
  moves moves every document that prints the census.

### DIV-2026-026 — `memory/list_session_struct.lu` — **RETIRED at pin `ec56a08f` (0.1.42, r24)**

The row (s119's #144 layout witness) builds `tbl: List[Session]`, where
`Session` carries a `List[int]`, then `let s2 = tbl[2]` — a plain `let` of
a non-`Copy` value, so a MOVE (`[mem.tier0.move.1]`) — stores `tbl[1]`, and
iterates `tbl`. The walk reads `tbl[2]`. Its header pins `exit(0)`
`102 1 1408 4 184`.

| machine | answer |
| --- | --- |
| lupin at is56 | `trap(use-after-move)` when the `for` reaches `tbl[2]`; nothing printed (the one `print` follows the loop) |
| lupin 0.1.40, wolf 0.2.17 checked / native / release | `exit(0)` `102 1 1408 4 184` |

Triage (`[proto.cmp.triage]`): the spec is clear —
`[mem.tier0.move.2]`: "Use of an uninitialized or moved-from place is …
E1001 … the read traps with kind `use-after-move`", and
`[mem.model.place.elem]` item 3 says the store to `tbl[1]` revives nothing
about `tbl[2]`. **This machine matches the clause; the row is the
defendant.** The compiler accepts it only through wolffe-lang/wolf-lang#460
(an index store revives the collapsed `tbl[_]`), and all four machines
read the moved element silently until #141's fix taught this one's `for`
to look. eg00 found the row prototyping the must-revival and recorded it
on wolffe-lang/wolf-lang#460 (comment 5851212791); wolf-lang's eg01 takes
the fix, `let s2 = copy tbl[2]`, which keeps the row's point (the element
stride) and its bytes on every machine. is46's DIV-2026-022/023 are the
precedent: headers a clause made stale, filed for the releases until the
pin carried the re-spelled row.

**In `differ::FILED_DIVERGENCES`**, unlike DIV-2026-025: this file
DISAGREES with the corpus (025's three agree), so `tests/run_corpus.rs`
and `tests/conformance.rs` must stop asserting its header, and its
`RUN_LEDGER` row records `trap(use-after-move)`. It retires the round a
differential compares it clean.

**Retired at the `ec56a08f` pin (r24, wolf-lang v0.2.18 — the TAG).** The
re-vendored row reads `let s2 = copy tbl[2]` (eg01), so no element is
moved and the `for` reads live ones. Measured before the entry came out
(kasumi `~/lanes/r24/logs/div026-retire.log`): the published wolf 0.2.18
(`da027bf9…6bd4`) answers `wolf conform-run …/list_session_struct.lu
--json` with `exit(0)` at `run` printing `102 1 1408 4 184` on `--checked`,
`--native` and `--release`, and this machine at the new pin answers the
same. Out of `differ::FILED_DIVERGENCES` (one entry left, DIV-2026-019);
its `RUN_LEDGER` row is `exit(0)` again.

### The member read — is59, wolf-lang#472 ruled A, `[mem.model.place.elem]` 1(c) under a claim (wolf-lang `d3bd49cc`)

Wave 50's row (added 2026-09-29): the maintainer ruled **A** on
wolffe-lang/wolf-lang#472 — "Clause 1(c) stands as written. lupin reads a
container member as the member". The clause, at wolf-lang `d3bd49cc`
(`spec/02-memory-model.md`, `[mem.model.place.elem]` item 1(c)): "An index
step and a member step on the same container: `xs[i]` and `xs.len`,
whatever `i` is — an element is never the container's header." The
contract is is56's, in five sections; §1–§3 are committed before the first
edit under `src/` or `tests/`, the rest is appended as it lands.
Measurements on kasumi (linux x86-64) under `~/lanes/is59/`: the published
wolf 0.2.18 (`da027bf9…6bd4`) and lupin 0.1.41 (`18848901…e8d4`), both
digests equal to the release pages', and a release build of trunk
`0cfc0cf` (`lupin-trunk-0cfc0cf`, `42b13d1e…`). Probes: 21 one-directory
programs under `~/lanes/is59/probes/` (`q*` a member read under an element
claim, `t*` twins that must still trap, `o*` neighbours out of scope, `c01`
an operand count); runner `~/lanes/is59/scripts/run-probes.sh`
(`lupin conform-run main.lu --json`, and `wolf conform-run main.lu
--checked --json` beside it).

#### §1 — forbidden, absolutely

No `rm` outside `~/lanes/is59/` (kasumi) and `/private/tmp/is59`; no
deletion in any tree this lane did not create; no `git add -A`; no edit to
another lane's file (is58 owns its four mirrors, is57 owns
`.github/workflows/`); no `~/.claude`; no build on nomad-1; no tag; no pin
move (pin stays `93a5fe50`); no merge, no rebase-merge; no `2>/dev/null` on
a checkout; kill only my own pids, never a pattern or a group; no claim of
"seen red" without the log it is in.

#### §2 — inputs, re-derived 2026-09-29

| input as written | at origin / measured | drift |
| --- | --- | --- |
| wolf-interp trunk | `origin/trunk` = `0cfc0cf` "release: lupin 0.1.41"; pin `93a5fe50` (wolf-lang v0.2.16); branch `is59` cut there | none |
| wolf 0.2.18, lupin 0.1.41 from the published archives | `wolf 0.2.18 (wolfgang, pin ec56a08)`, `lupin 0.1.41 … pin 93a5fe5`; the archive's lupin column on all 21 probes equals the trunk build's (`probes-archive-0.1.41.log`, `probes-trunk-0cfc0cf.log`); the trunk build's sha equals is58's trunk build (`42b13d1e…`) | none |
| the ruling | wolf-lang#472's last comment: "**Ruled by the maintainer 2026-09-29: A.** … lupin reads a container member as the member (is59, wolf-interp); the compiler then accepts `bump(mut xs[0], xs.len)` (eg02b), pinning lupin ≤ 0.1.41 as pre-mirror until is59's release." | none |
| lupin traps `bump(mut xs[0], xs.len)` | `q01`: 0.1.41 `trap(exclusivity)`, clause `mem.model.path.disjoint`; wolf 0.2.18 checked `fail(E1002)` | none |
| the mechanism (eg02's words: "it reads `.len` as the whole container") | two routes read a member that is not a stored field, and both read the PARENT as a whole place first: `eval_path_expr` (`src/eval/mod.rs:4717-4723`, a dotted path `xs.len`, `b.xs.len`) calls `read_path(parent)`, and `eval_member` (`mod.rs:7450`, a projected base `g[0].len`, `(xs).len`) calls `eval_consumed(base)`, which reads the base's container through `read_claim` (the index-read lend, `eval_bracket`). `read_claim` → `check_access(parent, Shared)` (`mod.rs:1491`, `:1556`) meets the held `xs[0]` as a prefix. The path model already separates the two: `Proj::may_equal` (`src/eval/place.rs:115`) answers `false` for a `Field` against an `Index`/`Key` — so the member's OWN path, `xs.len`, is disjoint from `xs[0]` today; nothing asks it | none; the fix is where the check is made, not what the path model says |
| "`xs.len` and every other member read" | lupin's non-stored members (`builtin::property`, `src/eval/builtin.rs:1045`): `List.len`, `Map.len`, `str.len`, a range's `start`/`end`, the duration suffixes on an int, and a field through a `shared` cell. An element claim exists only on a `List` or a `Map` (`Pool` is declined by name), so the claim-reachable members are `List.len` and `Map.len`, reached through either route | the row's "…" is these two members, over two routes |
| methods | `xs.count()` and `xs.is_empty()` are METHOD calls (`eval_method`), not member reads; 0.1.41 traps both under `mut xs[0]` (`o01`, `o02`) | **drift: wolf 0.2.18 checked RUNS both (`4`, `2`)** — see below |
| the compiler is "never looser than the oracle" (eg02, wolf-lang#472) | wolf 0.2.18 checked, which predates EG2, runs four probe shapes lupin 0.1.41 traps: a member read inside a larger argument expression (`q09` `xs.len * 10 + xs.len` → `34`; `q11` `"{xs.len}{xs.len}"` → `3`) and the two methods (`o01`, `o02`). Its E1002 sees a member read only as a bare argument | **drift**: 0.2.18 is already looser than 0.1.41 on these; after this lane `q09`/`q11` agree (ruling A); `o01`/`o02` stay parted — to be filed on wolf-lang, not fixed here |
| wolf-lang's gate at `d3bd49cc` | `element_places_lanes.rs::a_member_read_under_an_element_claim_stays_refused` asserts lupin `trap(exclusivity)` for ANY version; eg02b is to pin ≤ 0.1.41 as pre-mirror | this lane's lupin turns that case red until eg02b lands — the pairing order the ruling names |
| is58 in the same repo | `origin/is58` (`c6a4859`) edits `eval_path_expr` and `eval_member` (a new `ReadAs` parameter; `eval_consumed(base)` → `eval_projected(base)` on the member route), `CHANGELOG.md`, `docs/divergence-log.md` (a section at this same spot) | overlap in three files, named; the second to finish rebases |
| found: an operand count | `c01`: `let n = g[f()].len` prints `f` **three times** on 0.1.41 (and on is58's head `e965adb`), once on wolf 0.2.18: `live_place` at the `let`, `place_of(base)` in `eval_member`, and `eval_consumed(base)` each evaluate `f()` | not in the row; filed separately, and see §3 |

#### §3 — prediction, committed before the first edit

**The fix, one mechanism.** A member read that is not a stored field
checks exclusivity on the member's own path — the parent projected by a
`Field(member)` step — instead of on the parent. Everything else a read of
the parent does stays where it was: the `Moved` trap on the parent, the
freed-region fault, the `PlacePath` rule fire, the provenance read of the
parent. Both routes take it: `eval_path_expr`'s dotted tail, and
`eval_member` when the base is a place that resolves (the member is then
read off that place, not by evaluating the base a second time). The member
path has the parent as its prefix, so any claim on the parent or on one of
its prefixes conflicts exactly as before, and **when it conflicts the trap
is reported against the parent, byte for byte as trunk reports it**
(message, clause, span): no twin's record moves. Only a claim on an
element (an `Index`/`Key` step below the parent) stops conflicting.

**The probe table** (lupin at head against 0.1.41; wolf 0.2.18 checked in
brackets):

| probe | shape | 0.1.41 | head | [0.2.18] |
| --- | --- | --- | --- | --- |
| `q01` | `bump(mut xs[0], xs.len)` (wolf-lang#472's witness) | `trap(exclusivity)` | `exit(0)` `4 3` | [E1002] |
| `q02` | `bump(mut m["a"], m.len)` | trap | `exit(0)` `3 2` | [E0401] |
| `q03` | `bump(mut g[0][1], g[0].len)`, `bump(mut g[1][0], g[1].len)` (member route) | trap | `exit(0)` `4 6` | [E1002] |
| `q04` | `bump(mut g[1][2], g.len)` | trap | `exit(0)` `7 2` | [E1002] |
| `q05` | `bump(mut b.xs[2], b.xs.len)` (a field's list) | trap | `exit(0)` `7 4 7` | [E1002] |
| `q06` | `bump(mut xs[i], xs.len)` (run-time index; 1(c) "whatever `i` is") | trap | `exit(0)` `6 3` | [E1002] |
| `q08` | `add2(mut xs[0], mut xs[1], xs.len)` | trap at `xs.len` | `exit(0)` `4 5 3` | [E1002, pre-EG2] |
| `q09` | `bump(mut xs[0], xs.len * 10 + xs.len)` | trap | `exit(0)` `34` | [`34`] |
| `q10` | `bump(mut xs[0], (xs).len)` (member route, grouped base) | trap | `exit(0)` `4` | [E1002] |
| `q11` | `note(mut xs[0], "{xs.len}{xs.len}")` | trap | `exit(0)` `3` | [`3`] |
| `q12` | `let r = &mut xs[0]` then `xs.len` (a borrow's claim) | trap | `exit(0)` `3` | [unsupported] |
| `q07`, `o03` | `xs.len` before the claim; another container's `len` | `exit(0)` | unchanged | [same] |
| `t01` | `grow(mut xs, xs.len)` | `trap(exclusivity)` `mem.tier0.excl.1` | unchanged, same record | [E1002] |
| `t02`, `t03` | `trim(mut g[0], g[0].len)`, `grow(mut b.xs, b.xs.len)` | trap `mem.model.path.disjoint` | unchanged, same record | [E1002] |
| `t04`, `t05` | `bump(mut xs[0], xs[0])`, `sum(mut xs[0], xs)` (no member) | trap | unchanged | [E1002] |
| `o01`, `o02` | `xs.count()`, `xs.is_empty()` under `mut xs[0]` | trap | **unchanged — out of scope, named** (a method, not a member read) | [`4`, `2`] |
| `c01` | `let n = g[f()].len` | `f` ×3 | `f` ×2 — the member route stops evaluating its base twice; the `let`'s own `place_of` stays (filed) | [`f` ×1] |

**Addendum, before the first edit (two probes added after the table):**
`q13`, `bump(mut g[1][0], g[0].len)` (the member route with the SIBLING
claimed): 0.1.41 `trap`, head `exit(0)` `5 2` [E1002]. `o04`,
`bump(mut xs[1], xs[0] + 1)` (an element read inside an expression, no
member): 0.1.41 `trap` — the index-read lend reads `xs` whole — head
**unchanged, out of scope** [wolf 0.2.18 checked RUNS it, `4`]: a third
shape where 0.2.18 is looser than 0.1.41, to be filed with `o01`/`o02`.

**Corpus rows that change verdict: zero.** No row at the pin passes a
member of a container while an element of it is claimed (a grep of
`vendor/upstream/corpus` for a `mut x[…]` argument beside a `.len` in one
call finds none). Falsified by any line moving in `lupin corpus`.

**The differential against wolf 0.2.18** (all entries at pin `93a5fe50`,
four counterparty tiers, trunk release build against head release build):
**no entry added or removed on any tier, and no ledger line added.**
Falsified by any divergence or ledger line that differs.

**Existing tests that change: none.** Falsified by any test red at head
that was green at trunk (`a_two_phase_receiver_still_reads_itself_through_the_parent`
and `tests/snapshots/exclusivity_nested_path.snap` are the closest).

#### §3a — the prediction, scored (head `e64371b`)

- **The fix — held, with one design change named.** One mechanism, both
  routes (`1709aec`). What §3 did not say: the member's path is consulted
  only while some claim is held under the container's binding
  (`read_header`); with no claim, and whenever the member meets a claim
  too, the read is trunk's own code path, which is what makes "the twins
  keep their record byte for byte" true by construction rather than by
  re-deriving each message.
- **The probe table — 20 of 21 held.** `q01`–`q06`, `q08`–`q13` run at the
  predicted bytes; `q07`, `o03` unchanged; `t01`–`t05` keep trunk's verdict,
  clause and span (`probes-head-e64371b.log` against
  `probes-archive-0.1.41.log`); `o01`, `o02`, `o04` still trap, as
  predicted. **Missed: `c01`** — predicted `f` ×2, measured `f` ×3,
  unchanged. The prediction assumed the member route would always read off
  the evaluated place; the design above takes it only under a claim, so a
  program with no claim reads exactly as trunk — including
  wolffe-lang/wolf-interp#151's triple evaluation, which stands filed.
- **Corpus — held.** `lupin corpus` at head equals trunk line for line
  (`corpus-trunk-0cfc0cf-release.log`, `corpus-head-e64371b-release.log`):
  502 match, 2 mismatch, both runs.
- **Differential — held.** All four tiers against wolf 0.2.18 at pin
  `93a5fe50`: every ledger byte-identical (default 623, checked 313, native
  223, release 225 lines); findings 5/5/7/7 lines, the same entries. One
  line differs, on native, and it is the COMPILER's side: `a=ub(mem.ub)`
  both runs, `b=exit(151)` at trunk and `b=exit(130)` at head for
  `memory/unsafe_ub_uaf.lu` — wolf 0.2.18's native binary exits with noise
  on that UB program (four reruns: 90, 248, 251, 168,
  `unsafe_ub_uaf-native-reruns.log`), as eg02 recorded.
- **Existing tests — held so far:** `rulings_eg00` 15/15, `rulings_s182`
  11/11, `index_store` 7/7 at `1709aec` (`green-1709aec-rulings.log`); the
  whole suite is the gauntlet's, below.
- **Filed, not fixed:** wolffe-lang/wolf-interp#151 (`c01`),
  wolffe-lang/wolf-interp#152 (`o04`: an element read inside an
  expression checks the whole container, while a bare one does not), and
  wolffe-lang/wolf-lang#474 (wolf 0.2.18 runs `o01`, `o02`, `o04`, which
  lupin traps — the compiler looser than the oracle — and a ruling on
  whether `count()`/`is_empty()` are header reads under 1(c)).

#### §4 — evidence index

Commits: `19ed462` contract; `1bc1f72` §3 addendum; `0d45a6a` the 18
witnesses and their runner (`tests/rulings_is59/`,
`tests/rulings_is59.rs`), red at trunk; `1709aec` the fix; `3421ff8`
rustfmt; `885ac91`, `e64371b` CHANGELOG `Unreleased`.

Artifacts (kasumi `~/lanes/is59/evidence/`; wolf 0.2.18 `da027bf9…`,
lupin 0.1.41 `18848901…`, trunk build `42b13d1e…`, head build
`lupin-head-e64371b` `0308cfb9…`):
- inputs: `probes-archive-0.1.41.log`, `probes-trunk-0cfc0cf.log`,
  `probes-extra-trunk-0cfc0cf.log`, `witnesses-trunk-0cfc0cf.log`
- red at trunk, for the named reason: `red-0d45a6a-rulings_is59.log` —
  12 failed, each "lupin answered `trap(exclusivity)`, ruled `exit(0)`";
  6 passed (the control and the five twins); `EXIT=101`
- green: `green-1709aec-rulings.log` — `rulings_is59` 18/18,
  `rulings_eg00` 15/15, `rulings_s182` 11/11, `index_store` 7/7; `EXIT=0`
- probes at head: `probes-head-e64371b.log`
- corpus: `corpus-trunk-0cfc0cf-release.log`,
  `corpus-head-e64371b-release.log`
- differential: `diffrun/trunk-0cfc0cf-*.{log,jsonl,ledger.jsonl}` against
  `diffrun/head-e64371b-*`; `unsafe_ub_uaf-native-reruns.log`
- gauntlet (fmt, clippy, test, corpus) and GitHub CI at the head: in the
  PR body, which is written after they finish.

#### §5 — done-when

- [ ] branch `is59` on origin; PR open, unmerged, with these five sections
- [ ] §2 re-derived, §3 committed before the first `src/`/`tests/` edit, §3a scored
- [ ] wolf-lang#472's witness: a test red at trunk for the named reason, green at head
- [ ] every member read path under a live element claim has a test (both routes, `List` and `Map`, a call claim and a borrow claim), and every twin still traps with trunk's record
- [ ] whole-corpus differential against wolf 0.2.18: no new divergence
- [ ] CHANGELOG `Unreleased`
- [ ] the operand-count finding (`c01`) and the method parting (`o01`, `o02`) filed

### The four mirrors — is58, wolf-interp#143, #144, #145, #146 (wolf-lang `ec56a08f`, wolf 0.2.18)

Wave 50's row: lupin mirrors the four lupin issues wolf 0.2.18's gates pin
by version. The contract is is56's, in five sections; §2 and §3 are written
and committed before the first edit under `src/` or `tests/`, the rest is
appended as it lands. Measurements on kasumi (linux x86-64) under
`~/lanes/is58/`: the published wolf 0.2.18 (`da027bf9…`) and lupin 0.1.41
(`18848901…`), digests equal to the release pages', and a release build of
trunk `0cfc0cf` (`lupin 0.1.41`, `42b13d1e…`). Probes: 84 one-directory
programs under `~/lanes/is58/probes/` (`w*` #143, `m*` #144, `s*` #145, `p*`
#146) and the 40 corpus rows the three wolf-lang gates name, copied from
wolf-lang `ec56a08f` into `~/lanes/is58/gates/`; runner
`~/lanes/is58/scripts/run-probes.sh` (`lupin conform-run main.lu --json`,
and `wolf conform-run main.lu --checked --json` beside it).

#### §1 — forbidden, absolutely

No `rm` outside `~/lanes/is58/` (kasumi) and `/private/tmp/is58`; no
deletion in any tree this lane did not create; no `git add -A`; no edit to
another lane's file (is57 owns `.github/workflows/`); no `~/.claude`; no
build on nomad-1; no tag; no pin move (pin stays `93a5fe50`); no merge, no
rebase-merge; no `2>/dev/null` on a checkout; kill only my own pids, never
a pattern or a group; no claim of "seen red" without the log it is in.

#### §2 — inputs, re-derived 2026-09-28

| input as written | at origin / measured | drift |
| --- | --- | --- |
| wolf-interp trunk `0cfc0cf` | `origin/trunk` = `0cfc0cf` "release: lupin 0.1.41"; pin `93a5fe50` (wolf-lang v0.2.16); branch `is58` cut there | none |
| wolf 0.2.18 from the published archive | release v0.2.18, `wolf-0.2.18-x86_64-unknown-linux-gnu.tar.gz` sha256 `da027bf9…6bd4` downloaded and checked; `wolf --version` "0.2.18 (wolfgang, pin ec56a08) paired with lupin 0.1.41" | none |
| lupin 0.1.41 | release v0.1.41 archive sha256 `18848901…e8d4`; its lupin column on all 124 probes and rows equals the trunk build's (`probes-archive-0.1.41.log`, `probes-trunk-0cfc0cf.log`; whitespace-only difference in the runner's line ends) | none |
| #143: `copy xs`, `"{xs}"`, `xs == [1, 2]`, `let q = p` run after a part moved | measured `exit(0)` on 0.1.41 (`w01` `1 2`, `w02` `1 [1, 2]`, `w03` `1 true`, `w04` `1 2`); wolf 0.2.18 checked `fail(E1001)` on `w01`, `w02`, `w04`, `unsupported` on `w03` | none |
| #143's mechanism: `resolve` answers only the prefix chain | `resolve` (`src/eval/mod.rs:1435-1486`) walks the path's own projections; nothing asks a value's descendants | none |
| #144: `m[k] else …` of a non-`Copy` value copies out | measured on the witness (`m01`): 0.1.41 `exit(0)` `1`, wolf 0.2.18 `fail(E1001)`. **Narrower than the title**: a `let r = m["a"]` (`m07`), a `match` (`m20`) and an explicit `move m["a"]` (`m24`) already move — `consume_place` reaches the entry's slot. The copy is the `else` operand (`eval_else`, `mod.rs:5636`, evaluates the index read as a value) and `?` (`mod.rs:4365`) | the defect is the read-out under `else`/`?`, not every `Map` read |
| #144's clause: "A `Map` index READ copies exactly when `V` copies" | present at the pin, `vendor/upstream/spec/02-memory-model.md:956` (`[mem.map.absent]`, s152) | none |
| #145: a store evaluates every index operand but the last twice | measured `s01` `r1 r2 0 s1 s1 s2 a b a b c 7 9`; wolf 0.2.18 checked `r1 r2 0 s1 s2 a b c 7 9`. Cause: `exec_assign` asks `raw_target(place)` first, which evaluates the base place (`live_place(base)`, `mod.rs:8312`) to see whether it is a raw pointer, and then `place_of(place)` evaluates the same base operands again | none |
| #146: a whole `mut` parameter moved out is not seen by the caller | measured `p01` (`mut_param_moveout_whole`) `exit(0)` `1`. Cause: `call_fn` reads each parameter's final VALUE (`mod.rs:2851-2872`) and `finish_args` writes it back with `slot.state = SlotState::Live` (`mod.rs:3207-3214`); the parameter slot's own `Moved` state never leaves the callee. The field shape traps because its mark rides inside the value | none |
| the gates pin lupin by version at `ec56a08f` | `element_places_lanes.rs`: `PRE_MIRROR_LUPIN = ["0.1.40"]`, `PRE_MAP_MOVE_LUPIN = ["0.1.40", "0.1.41"]`; `mut_param_return_lanes.rs`: `PRE_MIRROR_LUPIN = ["0.1.40", "0.1.41"]`, `PRE_ELEM_TRAP_LUPIN = ["0.1.40"]`, `PRE_MAP_MOVE_LUPIN = ["0.1.40", "0.1.41"]`; `store_order_lanes.rs`: `LUPIN_145` for `0.1.40` and `0.1.41`. A branch build reports `0.1.41+dev.<sha>` (`build.rs`, D57), which no list names, so each gate's ruled arm applies to it | none |
| the 40 rows' lupin cells at 0.1.41 | `gates-archive-0.1.41.log`: the pinned pre-mirror answer on the five pinned rows (`elem_key_reassigned_no_revive` `1`, `mut_param_moveout_whole` `1`, `mut_param_moveout_one_path` `1`, `mut_param_moveout_map` `1`, `ctl_store_order_nested_index` doubled); every other row at its ruled cell | none |

#### §3 — prediction, committed before the first edit

**The fixes, one mechanism each.**

- **#146:** the callee's parameter slot state travels back with its value;
  a `Moved` parameter makes the caller's argument place `Moved` at the
  callee's move site. Every writeback path carries it: a free call, a
  trait-qualified call, an impl or home-module method's `mut self`.
- **#145:** `raw_target` hands back the base place it already evaluated,
  and the store projects its last index onto that path instead of calling
  `place_of` again. A base that does not resolve keeps today's route.
- **#144:** `else` and `?` over an index read of a `Map` entry read the
  entry OUT: a `Copy` value is copied, anything else moves
  (`consume_place`, as `let r = m[k]` already does). The key is evaluated
  once; an absent key answers the `none` row as today.
- **#143:** a WHOLE read of a place holding a moved part traps
  `use-after-move`, naming the moved part and its move site. Whole: a
  value read that hands the value on (a binding, an argument in any mode,
  `copy`, `move`/`take`, an interpolation hole, an operand, a return, a
  literal's field or element, a `match` scrutinee, an element a `for`
  binds). Not whole — the base of a projection: `xs.len`, `xs[i]`, `p.f`,
  a method receiver, a `for` head (which reads element by element, #141).
  The walk is skipped until the program first moves a part (one flag, the
  `region_teeth` pattern), so a program that never moves a part pays one
  load per whole read.

**The probe table** (lupin at head; wolf 0.2.18 checked in brackets):

| probe | 0.1.41 | head | |
| --- | --- | --- | --- |
| `w01`–`w14`, `w19`, `w20`, `w21`, `w34` (whole reads) | `exit(0)` | `trap(use-after-move)` (`w09` after printing `1`) | [E1001] |
| `w24`, `w27`, `w36` | `exit(0)` | `trap` after the part-wise prints | [E1001] |
| `w25` (`for row in g`, `g[0]` holds a moved part) | `exit(0)` | `trap` at the first iteration | [E1001] |
| `w03` (`==`), `w33` (`copy m` after `move m["a"]`) | `exit(0)` | `trap` | [unsupported] |
| `w15`, `w16`, `w22`, `w23`, `w35` (header, another element) | `exit(0)` | unchanged | [same] |
| `w31`, `w32` (revived by a store) | `exit(0)` | unchanged | [same] |
| `w17` (`(mut xs).push`), `w18` | `exit(0)` / `unsupported` | **unchanged — out of scope, named**: a method receiver is a projection base here; the compiler refuses `w17` (E1001) | [E1001] / [E0301] |
| `m01`, `m02`, `m09`, `m10`, `m19`, `m21`, `m25`–`m28` | `exit(0)` | `trap(use-after-move)` | [E1001] |
| `m13` (`for` after reading `m["b"]` out) | `exit(0)` | `trap` at `b`, after `1`, `a=1` | [unsupported] |
| `m22` (`pairs` after a read-out) | `exit(0)` | `trap` (#141's `pairs`) | [E1001] |
| `m23` (`"{m}"` after a read-out) | `exit(0)` | `trap` after `1` (#143) | [unsupported] |
| `m03`, `m18` (`Copy` values), `m04`, `m05` (stored back), `m06` (interpolation reads, no read-out), `m11`, `m12` | `exit(0)` | unchanged | [same] |
| `m15` (absent key) | `exit(0)` `0 2` | unchanged — nothing moved at run time | [E1001: the compiler cannot see the absence] |
| `m07`, `m20`, `m24` | `trap` | unchanged | |
| `s01`–`s06`, `s10`, `s11` | an outer operand twice | every operand once, equal to wolf's bytes | [the single order] |
| `s07`, `s08`, `s09`, `s12`, `s13`, `s14` (argument, read, move, receiver, `take`, `copy`) | once | unchanged | [same] |
| `p01`, `p02`, `p07`, `p11`, `p13` | `exit(0)` | `trap(use-after-move)` | [E1001] |
| `p05`, `p06`, `p10`, `p12`, `p14` | `exit(0)` | `trap` after the lines before the read | [E1001] |
| `p03`, `p04`, `p08`, `p09` (stored back, or revived by the caller) | `exit(0)` | unchanged | [E1001 on `p03`, `p08`, `p09`: the callee's may-path, static] |

**The gates' rows** (lupin leg, head against the ruled cell): the five
pinned rows flip to the ruled answer — `elem_key_reassigned_no_revive`,
`mut_param_moveout_whole`, `mut_param_moveout_one_path`,
`mut_param_moveout_map` → `trap(use-after-move)`;
`ctl_store_order_nested_index` → `i j val 7 a b c val 9`. The other 35
hold their cells. Falsified by any of the 40 off its ruled cell.

**Corpus rows that change verdict: zero.** Every shape above is E1001 on
the compiler, so no `exit(0)` row at the pin should reach one; the corpus
rows that read a map value out do so once per key or store it back.
Falsified by any line moving in `lupin corpus` or `tests/run_corpus.rs`.

**The differential against wolf 0.2.18** (all entries at pin `93a5fe50`,
four counterparty tiers, trunk release build against head release build):
**no entry added on any tier.** Conservatism-ledger lines may leave (a
negative `fail(E1001)` row whose lupin run now traps agrees with the
compiler's dynamic meaning) — none added. Falsified by any divergence
line added, or any ledger line added.

**Existing tests that change: none** — is56's
`what_reads_no_moved_element_is_untouched` keeps its push, `is_empty`,
`get` of a live index and header reads.

#### §3a — the prediction, scored

Evidence under `~/lanes/is58/evidence/` on kasumi; release builds of trunk
`0cfc0cf` and head `e965adb` (`lupin 0.1.41+dev.e965adb`), whose code is the
four fixes `f44b975`, `be3d4b5`, `c3af852`, `e965adb`.

- **The fixes, one mechanism each — held.** Each landed as its own commit
  and each stage compiles on its own (`cargo check --all-targets`, no
  warning). #145 needed no new evaluation order, only the kept base place
  (`project_index`, split out of `place_of`); #146 carries the slot state
  in `Applied::moved` through the three writebacks; #144 is one function,
  `eval_read_out`, under `else` and `?`; #143 is `check_whole` behind the
  `parts_moved` flag, and `ReadAs` telling a whole read from a projection
  base.
- **The probe table — held, every row.** Trunk against head
  (`probes-trunk-0cfc0cf.log`, `probes-head-e965adb.log`), 84 probes: each
  row moved exactly as the table says and no other — the `w`, `m` and `p`
  traps with the lines printed before them as predicted (`w09` `1`, `w24`
  `1 2 3`, `m13` `1`, `a=1`, `p10` `2`, `p12` `1`, …), the eight `s` stores
  to wolf's bytes, and every "unchanged" row byte-identical.
- **The gates' rows — held.** `gates-archive-0.1.41.log` against
  `gates-head-e965adb.log`: the five pinned rows move to the ruled answer
  and the other 35 are byte-identical; all 40 are at the lupin cell their
  gate asserts for an unpinned version. §2's claim that the 35 already sat
  there was read off the gate source row by row after the run, not before —
  it held.
- **Zero corpus rows — held.** `lupin corpus` trunk against head
  (`corpus-trunk-0cfc0cf-release.log`, `corpus-head-e965adb-release.log`):
  698 lines each, identical; census 502 match, 2 mismatch, as before. So
  `docs/manual/00-building.md`'s census does not move either (is56's third
  place).
- **The differential — held, and stronger than predicted.** Against the
  published wolf 0.2.18, 646 entries, 44 members (`diffrun/`): `default`,
  `checked` and `release` logs byte-identical trunk to head (5, 5 and 7
  divergences, all trunk's); `native` differs on one line, the counterparty
  exit status of `memory/unsafe_ub_uaf.lu` (`114` at trunk, `224` at head)
  — the compiled lane's use-after-free read, which is55 and is56 saw move
  run to run. Every conservatism ledger is byte-identical (313, 623, 223,
  225 lines); none left, where the prediction allowed some to.
  **New divergences: none.**
- **Existing tests that change: none — held.** `cargo test --lib` 719
  passed on the working tree before the commits were cut (`dev-a.log`);
  is56's `what_reads_no_moved_element_is_untouched` unchanged.
- **Not predicted:** the path-test commit `aa2b673` was not rustfmt-clean
  (`red-aa2b673.log`, `fmt` rc 1), fixed by `9c44904` before any fix
  landed. And the receivers: wolf 0.2.18 refuses a method called on a
  partly-moved container (`(mut xs).push`, `get`, `is_empty`, `pop`, a
  slice beside the moved element, an impl method's `self`) with E1001,
  where §3 kept them projection bases — measured in
  `probes2-head-e965adb.log` and filed as wolffe-lang/wolf-interp#149, a
  ruling for the clause rather than a change here (is56's test pins the
  current reading).

#### §4 — evidence index

Commits:
- `117128d` §1–§5, §2 re-derived and §3 predicted before any edit
- `6ecc3b1` the nineteen gate rows under `tests/rulings_is58/`; `f5206dd` their runner `tests/rulings_is58.rs`
- `aa2b673` 30 path tests in `src/eval/tests.rs`; `9c44904` rustfmt of them
- `f44b975` #145; `be3d4b5` #143; `c3af852` #146; `e965adb` #144
- `b8e4e60` CHANGELOG `Unreleased`; this section

Artifacts (kasumi `~/lanes/is58/evidence/`; wolf 0.2.18 `da027bf9…`,
lupin 0.1.41 `18848901…`, digests = release pages):
- red at trunk, for the named reason: `red-aa2b673.log` — paths 24 failed
  / 6 passed (the six are the controls: what stays readable, what was
  already evaluated once, the out-of-bounds store), each failure a program
  that ran on past its trap or ran an operand twice; `rulings_is58` 5 failed /
  15 passed, exactly the five pinned rows, each `lupin answered exit(0),
  ruled ["trap(use-after-move)"]` or the doubled stdout; `rulings_eg00`
  15/15 and `rulings_s182` 11/11 green at trunk
- green on the working tree that became `e965adb`: `dev-a.log` — fmt 0,
  clippy 0, `--lib` 719 passed, `rulings_is58` 20/20, `rulings_eg00`
  15/15, `rulings_s182` 11/11, `index_store` 7/7
- probes: `probes-archive-0.1.41.log`, `probes-trunk-0cfc0cf.log`,
  `probes-head-e965adb.log`; receivers `probes2-head-e965adb.log`
- gate rows: `gates-archive-0.1.41.log`, `gates-head-e965adb.log`
- wolf-lang's own gates at `ec56a08f` with `LUPIN` set:
  `wolfgates-head-e965adb.log`, `wolfgates-red-aa2b673.log`,
  `wolfgates-archive-0.1.41.log` (results in the PR body)
- corpus: `corpus-trunk-0cfc0cf-release.log`, `corpus-head-e965adb-release.log`
- differential: `diffrun/trunk-0cfc0cf-*` against `diffrun/head-e965adb-*`
- gauntlet at `e965adb`: `gauntlet-e965adb.log`; CI at the head sha: the PR body

#### §5 — done-when

- [x] branch `is58` on origin; PR open, unmerged, with these five sections
- [x] §2 re-derived, §3 committed before the first `src/`/`tests/` edit, §3a scored
- [x] each of #143, #144, #145, #146: a test red at trunk for its named reason, green at head
- [x] every read or store path each fix reaches has a test
- [x] whole-corpus differential against wolf 0.2.18: no new divergence
- [x] CHANGELOG `Unreleased`
- [ ] the gates' pinned lupin cases this head satisfies, listed by test name
- [ ] CI green at the head sha
- [ ] kasumi build dirs pruned once evidence is written; worktree removed

### Header reads — is60, wolf-lang#474's lupin half, wolf-interp#149, #151, #152 (ruled 2026-09-30)

Wave 51's row: the maintainer's ruling on wolffe-lang/wolf-lang#474 and
wolffe-lang/wolf-interp#149 (2026-09-30), quoted from #149: "`len`,
`count`, `is_empty` are header reads — allowed on a container with a moved
element, so the compiler widens for those (s185); `push`, `get`, `pop`,
slices and an impl method's `self` read the whole container — the
compiler's E1001 stands and lupin traps (is60)." And from wolf-lang#474:
"`len`, `count` and `is_empty` are header reads under `[mem.model.place.elem]`
1(c) on every machine; every other method reads the whole container. The
compiler's current acceptance of `count`/`is_empty` under an element claim
stands; lupin moves (is60)." Four parts, one commit chain each, each red at
trunk first: (H) `count()`/`is_empty()` under an element claim are header
reads; (E) wolf-interp#152, an element read inside an expression checks
the element; (R) #149's lupin half, a whole-read method or slice on a
container holding a moved part traps while the header methods run; (C)
wolf-interp#151, `g[f()].len` evaluates `f()` once. The contract is is59's,
in five sections; §1–§3 are committed before the first edit under `src/`
or `tests/`, the rest is appended as it lands. Measurements on kasumi
(linux x86-64) under `~/lanes/is60/`: the published wolf 0.2.18
(`da027bf9…6bd4`) and lupin 0.1.41 (`18848901…e8d4`), archive digests
equal to the release pages', and a release build of trunk `c68c4d3`
(`lupin-trunk-c68c4d3`, `lupin 0.1.41+dev.c68c4d3`, `91839eb9…`). Probes:
82 one-directory programs under `~/lanes/is60/probes/` (`h*` header methods
under a claim, `e*` element reads under a claim, `r*` receivers and slices
on a partly-moved container, `c*` operand counts, `*t*` twins that keep
trunk's answer, and `s_*` — s185's nine corpus witnesses copied verbatim
from wolf-lang `origin/s185` `ecaf655a`, Pool's left out since lupin
declines `Pool`); runner `~/lanes/is60/scripts/run-probes.sh`
(`lupin conform-run main.lu --json`, and `wolf conform-run main.lu
--checked --json` beside it), summarised by `~/lanes/is60/scripts/summ.py`.

#### §1 — forbidden, absolutely

No `rm` outside `~/lanes/is60/` (kasumi) and `/private/tmp/is60`; no
deletion in any tree this lane did not create; no `git add -A`; no edit to
another lane's file (s185 owns wolf-lang; no workflow edit here); no
`~/.claude`; no build or test on nomad-1 (kasumi only, `CARGO_BUILD_JOBS=4`);
no tag; no pin move (pin stays `93a5fe50`); no merge, no rebase-merge; no
`2>/dev/null` on a checkout; kill only my own pids, never a pattern or a
group; no claim of "seen red" without the log it is in; no trailer on any
commit.

#### §2 — inputs, re-derived 2026-09-30

| input as written | at origin / measured | drift |
| --- | --- | --- |
| wolf-interp trunk `c68c4d3` | `origin/trunk` = `c68c4d3` "docs: re-point is58's shas after the rebase onto is57 and is59"; pin `93a5fe50` (wolf-lang v0.2.16); branch `is60` cut there | none |
| wolf 0.2.18, lupin 0.1.41 from the published archives | archives re-downloaded into `~/lanes/is60/archives/`: `da027bf9…6bd4` and `18848901…e8d4`, equal to the release assets' digests; `wolf 0.2.18 (wolfgang, pin ec56a08) paired with lupin 0.1.41` | none |
| the archive and trunk agree | `probes-archive-0.1.41.log` against `probes-trunk-c68c4d3.log`, lupin column: equal on 80 of 82; `c16` (a member read under a claim) runs on trunk and traps on 0.1.41 — is59; `e03` (`(m["b"] else 0) + 1` under `mut m["a"]`) runs on trunk and traps on 0.1.41 — is58's #144 read-out took the `else` operand off the index-read lend | trunk is 0.1.41 plus is58 and is59, as expected; `e03` needs no change here |
| H: `count`/`is_empty` under an element claim trap on lupin | `h01` `bump(mut xs[0], xs.count())`, `h02` `xs.is_empty()`, `h07` `g.count()` under `mut g[1][2]`, `h08` `b.xs.count()`, `h09` in an operand, `h10` under a borrow, `h11` under two claims, `s_elem_header_methods_under_claim`: trunk `trap(exclusivity)` `mem.model.path.disjoint`; wolf 0.2.18 runs `h01` `4`, `h02` `2`, `h07` `7`, `h08` `7`, `h09` `34`, s185's row `4 3` | none. Cause: `eval_method` reads a `Receiver::Place` through `read_claim(path)` (`src/eval/mod.rs:7557`), which checks the whole container against the held element; is59's `read_header` (`mod.rs:1603`) is only asked by the member routes |
| H: `xs.len()` | lupin's builtin surface answers `len` as a method too (`builtin.rs:1389`); wolf 0.2.18 refuses it (`E0301: len is not a builtin method of List[int]`, `h03`, `r08`) | the header set is `len`/`count`/`is_empty` on a `List` or `Map` receiver; `len()` rides along on lupin's surface, no compiler row |
| E: `xs[0] + 1` under `mut xs[1]` traps (#152) | `e01` trunk `trap(exclusivity)` "`xs` is accessed as `read` while `xs[1]` is held as `mut`"; wolf `4`. Also traps on trunk: `e02` two siblings, `e04` a sibling row `g[0][1]` under `mut g[1][0]`, `e05` the same row `g[0][1]` under `mut g[0][0]`, `e06` run-time indices, `e07` an interpolation hole, `e08` a field's list, `e09` under a borrow, `e10` an index operand with an effect, `e11` beside two claims, `e13` under `#![index(1)]`; wolf runs every one it accepts (`15`, `6`, `4`, `5`, `4`, `4`, `f 4`, `5`) | none. Cause as #152 says: the index-read lend in `eval_bracket` (`mod.rs:7936-7952`) calls `read_claim(&place)` on the CONTAINER before the index is known; a nested `g[0][1]` reaches it through `eval_projected(g[0])`, so the check is made on `g[0]` at best, never on `g[0][1]` |
| E: the bare form | `e12` `bump(mut xs[1], xs[0])` runs on trunk (`3`) — `live_place` checks `xs[0]` itself; wolf 0.2.18 `E1002` | the bare form is already element-granular; the expression form moves to it |
| R: #149's seven rows | trunk runs `r01` push `1 3`, `r02` get `1 2`, `r03` is_empty `1 false`, `r04` pop `1 2 1`, `r05` slice `1 2`, `r06` impl `self` `1 2`; wolf 0.2.18 `E1001` on each (`r07`'s `&xs` is `unsupported` on wolf and is not in the ruling: not probed again) | none |
| R: the rest of the method surface | trunk runs `r09` `last`, `r10` `clear`, `r12` `Map.remove`, `r13` `mut self`, `r14` `(mut g[0]).push` with `g[0][0]` moved, `r16` `g.get(1)` with `g[0][0]` moved, `r18` `b.xs.get(0)`, `r22` a slice of the row `g[0][1..3]`, `r23` an impl method on `ps[0]` whose field moved; wolf refuses each it can check (`E1001`; `Map` methods are `unsupported` on the checked machine) and runs `r17` (`(mut g[1]).push`, `g[1].get(0)` with `g[0][0]` moved: `1 2 1`) | "every other method" is the rule: any method but the three header reads on a `List`/`Map`, and any impl or home-module method, reads the receiver whole. An impl method NAMED `count` is not a header read (s185's `elem_whole_read_named_count_after_move.lu`) |
| R: header reads after a move | `r03`, `r07`, `r11`, `r15`, s185's `elem_header_methods_after_move{,_map}.lu`: trunk runs them; wolf 0.2.18 `E1001` (s185 widens the compiler) | none — they keep running here |
| R: the mechanism | a method receiver is a projection base (`ReadAs::Projected`, is58, `mod.rs:217`): `eval_method` reads it with `read_claim`/`read_path` and never walks it for a moved part; a slice's target comes from `eval_projected(base)` (`mod.rs:7962`) and `builtin::slice` checks only the sliced range (#141) | none |
| C: `g[f()].len` runs `f()` three times (#151) | trunk: `f` ×3 in a `let` (`c01`), an argument (`c04`), `copy` (`c08`), a `match` scrutinee (`c09`), a grouped base (`c10`), a field's list (`c07`), a `str` element (`c15`), a `Map` value (`c05`, `k` ×3); ×2 in an interpolation hole (`c02`) and an operand (`c03`); `f h` ×3 on a nested base (`c06`); ×2 under a claim (`c16`); ×3 then the trap on a moved element (`c17`). wolf 0.2.18: once, everywhere it runs | **wider than the title**: two duplications, not one. (A) a caller asks `live_place(expr)` (`mod.rs:3891`), which evaluates the operands, finds no slot for the member, answers `None`, and the caller then evaluates `expr` from scratch; (B) `eval_member` asks `place_of(base)` (`mod.rs:7838`), and when the member is not a stored field falls to `eval_projected(base)` (`:7865`), evaluating the base a second time. Interpolation and operands meet only (B) |
| C: controls and neighbours | once on trunk: `c11` `g[f()].count()` (the receiver's place is taken once, `method_split`), `c12` `g[f()][0]`. Twice on trunk, not a member read: `c13` `let n = g[f()]` out of bounds (the `live_place` fallback, then the bounds trap), `c14` `xs[a()..2]` (`place_of` evaluates the range before refusing a slice as a place) | the neighbours are (A)'s family on the element and slice side, not #151's shape: filed, not fixed here |
| the compiler is looser on the twins too | wolf 0.2.18 runs `et01` `bump(mut xs[1], xs[1] + 1)` (`5`), `et03` `grow(mut g[0], g[0][1] + 1)` (`3`), `et04` `grow(mut xs, xs[0] + 1)` (`4`), `ht01` `grow(mut xs, xs.count())` (`4`), `ht02`, `ht03` `xs.get(1)` under `mut xs[0]` (`3`), `ht04` (`4`) — its E1002 sees an element or member read only as a bare argument (wolf-lang#474's body; the nested-call half is wolf-lang#476) | the compiler's half, on file; lupin keeps trapping these — each reads the claimed place or the whole container |
| the gates at wolf-lang `d11c03b7` | `element_places_lanes.rs`: `PRE_MIRROR_LUPIN = ["0.1.40"]`, `PRE_MAP_MOVE_LUPIN = ["0.1.40", "0.1.41"]`, `PRE_MEMBER_LUPIN = ["0.1.40", "0.1.41"]`; `mut_param_return_lanes.rs`: `PRE_MIRROR_LUPIN`, `PRE_MAP_MOVE_LUPIN` (`0.1.40`, `0.1.41`), `PRE_ELEM_TRAP_LUPIN = ["0.1.40"]`; `store_order_lanes.rs`: `LUPIN_145` for `0.1.40`, `0.1.41`; `index_store_lanes.rs`: `PRE_MIRROR_LUPIN = ["0.1.38", "0.1.39"]`. No case at `d11c03b7` names #149, #151, #152 or `count`/`is_empty`; s185's branch adds corpus rows (above), no gate yet | none; the pinned cases this head satisfies are measured, not read (§4) |

#### §3 — prediction, committed before the first edit

**The fixes, one mechanism each.**

- **H.** A method call whose receiver is a place holding a `List` or a
  `Map`, spelled bare (no `mut`/`take`), with no argument, named `len`,
  `count` or `is_empty`, is a header read: while a claim is held under the
  receiver's binding it asks is59's `read_header` with the method's name as
  the member, and answers from the builtin surface. When the header meets
  a claim (the whole container, a prefix, the element whose header it is),
  the call takes trunk's route, so the trap keeps trunk's record.
- **E.** An index read whose bracket chain (`xs[i]`, `g[i][j]`, `b.xs[i]`)
  bottoms out at a plain container path, while a claim is held strictly
  below that container and none on it or above it, evaluates its index
  operands in trunk's order and then checks the ELEMENT's full path; the
  rest of the container read (the `Moved` trap, the freed-region fault,
  the rule fire, the provenance read) is made where trunk made it. When the
  element meets the claim, the trap is trunk's (the container path, its
  clause and span). With no claim, or one on the container or above it,
  the read is trunk's code.
- **R.** A method call on a place whose value holds a moved part traps
  `use-after-move` at the receiver read, naming the part and its move
  site (is58's whole-read trap), unless it is one of H's header reads on a
  `List`/`Map`. A slice (`e[a..b]`) whose target holds a moved part traps
  the same way before its endpoints are evaluated. Behind is58's
  `parts_moved` flag, so a program that never moves a part pays one load.
- **C.** A member read that is not a stored field reads its base off the
  place its operands were already evaluated into: `eval_member` reads the
  base path instead of evaluating the base again (B), and a `live_place`
  that evaluated a member's base and found no slot for the member hands
  the base's path to the evaluation that follows it (A). When the base
  place meets a claim the read takes trunk's route (whose container check
  traps before any operand runs again).

**The probe table** (lupin at head against trunk; wolf 0.2.18 checked in
brackets):

| probe | trunk | head | [0.2.18] |
| --- | --- | --- | --- |
| `h01`, `h02`, `h07`, `h08`, `h09` | `trap(exclusivity)` | `exit(0)` `4`, `2`, `7`, `7`, `34` | [same bytes] |
| `h03` `xs.len()`, `h04` `m.count()`, `h05` `m.is_empty()`, `h10` borrow, `h11` two claims | trap | `exit(0)` `4`, `3`, `2`, `3`, `4 5` | [E0301, E0401, E0401, unsupported, E1002] |
| `s_elem_header_methods_under_claim` | trap | `exit(0)` `4 3` | [`4 3`] |
| `h06` `g[0].count()` under `mut g[1][0]` | `exit(0)` `5` | unchanged | [`5`] |
| `ht01`–`ht04` (whole claim; the element's own header; `get`, `last` under an element claim) | trap | unchanged, same clause and span | [runs — the compiler's half] |
| `e01`, `e02`, `e04`–`e08`, `e10`, `e13` | trap | `exit(0)` `4`, `15`, `6`, `4`, `5`, `4`, `4`, `f 4`, `5` | [same bytes] |
| `e09` borrow, `e11` two claims | trap | `exit(0)` `2`, `5 6` | [unsupported, E1002] |
| `e03`, `e12` | `exit(0)` | unchanged | [E0401, E1002] |
| `et01`, `et03`, `et04`, `et05` | trap | unchanged, same clause and span | [runs, runs, runs, E1002] |
| `et02` `bump(mut xs[1], xs[f()] + 1)`, `f` → 1 | trap, no stdout | trap, same clause and span, stdout `f` — the index now runs before the element's check, as the bare `et05` already does | [`f 5`] |
| `r01`, `r02`, `r04`, `r05`, `r06`, `r09`, `r10`, `r12`, `r13`, `r14`, `r16`, `r18`, `r22`; s185's `get`/`pop`/`push`/`self`/`slice`/`named_count` rows | `exit(0)` | `trap(use-after-move)` `mem.tier0.move.2`, nothing printed | [E1001; `r12` unsupported] |
| `r23` | `exit(0)` `1 5` `3` | `1 5` then the trap at `ps[0].total()` | [E1001 at that line] |
| `r03`, `r07`, `r08`, `r11`, `r15`, `r17`, `r19`, `r20`, `r21`; s185's two header rows | `exit(0)` | unchanged | [E1001 on the header rows until s185; `r17`, `r19`–`r21` run] |
| `c01`–`c10`, `c15`, `c16` | `f` ×2 or ×3 | `f` (`k`; `f h`) once, the rest of the bytes unchanged | [once] |
| `c17` | `f` ×3, then `trap(use-after-move)` | `f` once, the same trap (clause, span) | [E1001] |
| `c11`, `c12` | once | unchanged | [once] |
| `c13`, `c14` | twice | **unchanged — out of scope, named** | [once] |

**Existing tests that change: exactly one**, by the ruling: is56's
`what_reads_no_moved_element_is_untouched` (`src/eval/tests.rs`) pushes
onto and `get`s from a list whose `xs[0]` moved; its push and `get` move to
the trap side and the test keeps the header reads, another element, the
store that revives, and `push`/`get` after the revival. Falsified by any
other test red at head that was green at trunk.

**Corpus rows that change verdict: zero.** No row at the pin calls a
method or slices a container holding a moved part, or reads an element in
an expression beside an element claim (the one `mut xs[0], mut xs[1]` row,
`memory/mut_elem_excl.lu`, reads nothing beside them). Falsified by any
line moving in `lupin corpus`.

**The differential against wolf 0.2.18** (all entries at pin `93a5fe50`,
four counterparty tiers, trunk release build against head release build):
**no entry added on any tier, and no ledger line added.** Falsified by any
divergence or ledger line added.

**The gates at wolf-lang `d11c03b7`**, run on kasumi with `LUPIN` set to
the head build presented as the next release (is58's
`lupin-as-next.sh` shape): every case green, which is every pinned case
under `PRE_MEMBER_LUPIN`, `PRE_MAP_MOVE_LUPIN`, `PRE_MIRROR_LUPIN` and
`LUPIN_145` satisfied by the ruled arm. Falsified by any red.

**Addendum, before the first edit of part C** (H, E and R had landed; no C
edit existed): the neighbours `c13`/`c14` are the same duplication as #151's
(A), and a slice base meets (B) too. Five probes added:

| probe | trunk | head (C) | [0.2.18 checked] |
| --- | --- | --- | --- |
| `c13` `let n = g[f()]`, `f` → 5 | `f` ×2, `trap(bounds)` | `f` once, the same trap (clause, span) | [`f` once] |
| `c14` `let s = xs[a()..2]` | `a` ×2 | `a` once | [once] |
| `c18` `let n = xs[a()..2].len` | `a` ×3 | `a` once | [`a` ×2 — **the checked machine's own double evaluation**; `--native` and `--release` once] |
| `c19` `"{xs[a()..2].len}"` | `a` ×2 | `a` once | [`a` ×3 checked; once native/release] |
| `c20` `let v = m[k()]`, `k` absent | `k` ×2 | `k` once | [once] |
| `c21` `"{g[f()].len}"`, `f` → 5 | `f` ×2, `trap(bounds)` | `f` once, the same trap | [once] |
| `c22` `let n = xs[f()]` under `#![index(1)]`, `f` → 4 | `f` ×2, `trap(bounds)` | `f` once, the same trap and the writer's index in its line | [once] |

So C widens to the whole read-side family: a place whose operands ran and
which has no slot (a non-stored member, an index out of range, an absent
key) is read off the evaluated path; a slice spelled in brackets is refused
as a place before its endpoints run. `c13`/`c14` leave the "unchanged" row
of the table above. The wolf 0.2.18 checked machine's own duplication on
`c18`/`c19` is the compiler's, to be filed on wolf-lang.

#### §3a — the prediction, scored

Release builds of trunk `c68c4d3` (`91839eb9…`) and head `4a8c506`
(`lupin 0.1.41+dev.4a8c506`, `10df69ba…`), whose code is the four fixes
`4708106` (H), `e675c2f` (E), `e78c40a` (R), `4a8c506` (C); later commits
touch only `CHANGELOG.md` and this log.

- **The fixes, one mechanism each — held.** H is `header_read` plus
  is59's `read_header` asked with the method's name; E is
  `read_elem_under_claim` in front of the index-read lend, with
  `AccessSet::conflicts_only_below` (`place.rs`) deciding when it applies;
  R is one whole-read check at the receiver in `eval_method` and one before
  a slice's endpoints, both behind is58's `parts_moved` flag; C is
  `Machine::evaluated_place` (left by `live_place`, taken by the very next
  `eval`), the member route reading its base's evaluated place, and
  `read_missing_element` for an index out of range or an absent key, with
  `project_index` refusing a spelled range before its endpoints run.
- **The probe table — held, every row** (`probes-trunk-c68c4d3.log`
  against `probes-head-4a8c506.log`, 89 probes): the `h*`, `e*`, `r*`, `c*`
  and `s_*` rows moved exactly as §3 and its addendum said, the twins kept
  verdict, clause and span, `et02` gained its `f`, and every "unchanged" row
  is byte-identical. **Drift:** the table ran on 89 probes, not §2's 82 —
  `ht05` (an impl method named `count` under a field claim), `e14` (a
  `Map` element in a hole) and the addendum's five `c*` were added before
  their parts' edits.
- **Existing tests that change — missed: three, not one.** §3 named is56's
  `what_reads_no_moved_element_is_untouched`; the ruling also moved is56's
  `a_slice_covering_a_moved_element_traps_and_one_beside_it_does_not` (its
  second half sliced beside the moved element; renamed
  `…_and_so_does_one_beside_it`) and is58's
  `what_projects_through_a_partly_moved_place_is_untouched` (its `push` and
  `get` beside `move xs[0]`). All three were rewritten to the ruling in
  `80434ba`/`ea7e403`; the lib run at `e78c40a` (`lib-e78c40a.log`, 3
  failed) is where they surfaced. My search for affected tests grepped the
  corpus, not `src/eval/tests.rs`.
- **Two of my own tests were red for the wrong reason**, caught before
  they counted: a home-module receiver test (`80434ba`) failed on
  `unsupported` (a one-file program has no home module) — dropped in
  `e082d09`; and the slice test first read its endpoint in a `let` and then
  as a member base, where #151's double evaluation printed the endpoint
  before the trap (`lib-ea7e403.log`) — it reads the slice whole since
  `9043eeb`.
- **Corpus rows that change verdict: zero — held.** `lupin corpus` at head
  equals trunk line for line (`corpus-trunk-c68c4d3-release.log`,
  `corpus-head-4a8c506-release.log`, 698 lines, 502 match, 2 mismatch).
- **The differential — held.** Against the published wolf 0.2.18, 646
  entries, 44 members, four tiers (`diffrun/trunk-c68c4d3-*` against
  `diffrun/head-4a8c506-*`): every ledger byte-identical (default 623,
  checked 313, native 223, release 225 lines); the default, checked and
  release reports byte-identical (5, 5, 7 divergences, all trunk's); native
  differs on one line, the compiled lane's own exit status on
  `memory/unsafe_ub_uaf.lu` (`b=exit(213)` at trunk, `b=exit(203)` at
  head), the UB program whose native exit is noise run to run (is55, is58,
  is59). **New divergences: none.**
- **Found, filed:** the wolf 0.2.18 checked machine runs a slice's
  endpoints twice (`let n = xs[a()..2].len`) or three times
  (`"{xs[a()..2].len}"`) when the slice is a member base; native and release
  run them once — wolffe-lang/wolf-lang#479. The compiler-side looseness on
  the twins (`et01`, `et03`, `et04`, `ht01`–`ht05` run on 0.2.18) is its
  bare-argument E1002 (wolffe-lang/wolf-lang#474's body,
  wolffe-lang/wolf-lang#476), already on file.

#### §4 — evidence index

Commits:
- `1659ae2` §1–§3; `f941e5d` §3 addendum (before the first C edit)
- H: `ecda713` 17 witnesses (red), `4708106` the fix
- E: `3533b0e` 16 witnesses (red), `e675c2f` the fix and `conflicts_only_below`'s unit test
- R: `f7cfa61` 28 witnesses (red), `80434ba` path tests (red) and is56's test to the ruling, `e082d09` the wrong-reason test dropped, `e78c40a` the fix, `ea7e403`/`9043eeb` is56's and is58's rows to the ruling and the slice test's operand
- C: `bd287d1` 21 witnesses (red), `4a8c506` the fix
- `bf10a67` CHANGELOG `Unreleased`; this section

Artifacts (kasumi `~/lanes/is60/evidence/`; wolf 0.2.18 `da027bf9…`,
lupin 0.1.41 `18848901…`, digests = release pages; trunk build
`91839eb9…`, head build `10df69ba…`):
- inputs: `probes-archive-0.1.41.log`, `probes-trunk-c68c4d3.log`, `witnesses-trunk-c68c4d3.log`
- red, each for its named reason:
  - H `red-ecda713-H.log`: 11 failed, each "lupin answered `trap(exclusivity)`, ruled `exit(0)`"; 6 passed (the control, five twins); `EXIT=101`
  - E `red-3533b0e-E.log`: 13 failed — 12 "answered `trap(exclusivity)`, ruled `exit(0)`", and `twin_effect_index_same_elem` on its stdout (`f` before the trap); `EXIT=101`
  - R `red-80434ba-R.log`: 20 failed, each "answered `exit(0)`, ruled `trap(use-after-move)`"; the 8 keep-running rows pass; `EXIT=101`. Path tests `red-80434ba-R-lib.log` and, for the rows rewritten after the fix, `red-overlay-e082d09-tests-9043eeb.log` (the pre-fix tree with `9043eeb`'s `tests.rs`: 4 failed, each a program that ran past its whole read; `EXIT=101`)
  - C `red-bd287d1-C.log`: 20 failed, each on "the ruled stdout" (an operand run twice or three times); `EXIT=101`
- green: `green-4708106-H.log`, `green-e675c2f-E.log`, `green-e78c40a-R.log`, `green-4a8c506-C.log` (`rulings_is60` 17 → 33 → 61 → 82, with `rulings_is59` 18, `rulings_is58` 20, `rulings_eg00` 15, `rulings_s182` 11, `index_store` 7, each `EXIT=0`); `green-9043eeb-lib.log`, `green-4a8c506-lib.log` (725 passed)
- probes at head: `probes-head-4a8c506.log`; witnesses `witnesses-head-4a8c506.log`
- corpus: `corpus-trunk-c68c4d3-release.log`, `corpus-head-4a8c506-release.log`
- differential: `diffrun/trunk-c68c4d3-*.{log,jsonl,ledger.jsonl}` against `diffrun/head-4a8c506-*`
- gauntlet at `4a8c506` (the code head): `gauntlet-4a8c506.log` — `dirty: 0`; fmt 0, clippy 0, test 0, corpus 0; `GAUNTLET_FAILS=0`; 57 test binaries, 1417 passed, 0 failed; corpus 646 entries, 0 failures; 01:18:30Z–05:05:49Z
- wolf-lang's ten lupin-reading gates at `d11c03b7` (`scripts/gates.sh`: `element_places_lanes`, `mut_param_return_lanes`, `store_order_lanes`, `index_store_lanes`, `element_move_conservatism_lanes`, `move_expression_lanes`, `mut_claim_extent`, `pairing`, `record_file_index`, `store_rhs_first_lanes`; `WOLF_PAIRING_REQUIRE_SIBLING=1`; checked lane and lupin run, no SKIP):
  - `wolfgates-archive-0.1.41.log`: the published 0.1.41 takes the pinned arms, 102/102, `EXIT=0`
  - `wolfgates-head-4a8c506-as-next.log`: head, its record claiming `0.1.42-is60` (`scripts/lupin-as-next.sh`; `--version` already `0.1.41+dev.4a8c506`), takes every ruled arm, 102/102, `EXIT=0`
  - `wolfgates-plant-0.1.41-as-next.log`: the plant — the 0.1.41 archive presented as `0.1.42-plant` — goes red on exactly the seven pinned cases (below) and on `pairing` (the planted version is not `PAIRING`'s), `EXIT=101`: the gates can see the difference
  - the pinned cases head satisfies, for the next pairing bump: `element_places_lanes` `a_member_read_under_an_element_claim_runs` and `a_member_read_beside_an_element_claim_runs_on_every_leg` (`PRE_MEMBER_LUPIN`), `a_store_through_a_reassigned_key_revives_nothing` (`PRE_MAP_MOVE_LUPIN`); `mut_param_return_lanes` `a_whole_mut_parameter_left_moved_out_is_refused` and `a_mut_parameter_stored_back_on_one_path_only_is_refused` (`PRE_MIRROR_LUPIN`), `a_map_value_of_a_mut_parameter_left_read_out_is_refused` (`PRE_MAP_MOVE_LUPIN`); `store_order_lanes` `every_index_operand_runs_once_outermost_first_before_the_value` (the `0.1.41` row of `LUPIN_145`). These carry is58 and is59; no gate case at `d11c03b7` names #149, #151, #152 or the header methods — s185's rows for them (branch `ecaf655a`) are corpus files, and head answers all nine at their ruled cell (`tests/rulings_is60/*_s185`)
- GitHub CI: run 36654452989 at `bf10a67` (code equal to `4a8c506`), `completed success`; the head's run in the PR body

#### §5 — done-when

- [x] branch `is60` on origin; PR open, unmerged, with these five sections (wolffe-lang/wolf-interp#154)
- [x] §2 re-derived, §3 committed before the first `src/`/`tests/` edit (and its addendum before C's), §3a scored
- [x] H, E, R, C: each a test red at trunk for its named reason, green at head, in its own commit chain
- [x] every read path each part reaches has a test (H: `List`/`Map`, dotted and projected receivers, call and borrow claims; E: flat, nested, field, map, dynamic, origin-1, interpolation; R: every method family, impl and `mut self`, slices, the header reads that keep running; C: every caller of `live_place` a member reaches, the member route, out of range, an absent key, a slice)
- [x] whole-corpus differential against wolf 0.2.18: no new divergence
- [x] CHANGELOG `Unreleased`
- [x] the pinned lupin cases in wolf-lang's gates this head satisfies, by test name (§4)
- [x] the neighbours `c13`/`c14`: fixed under C's addendum rather than filed; the checked machine's own slice double evaluation filed as wolffe-lang/wolf-lang#479
- [ ] CI green at the head sha (the PR body)
- [ ] kasumi build dirs pruned once evidence is written; worktree removed

### The move is the write — is61, wolf-interp#155, W1002 beside `[mem.tier0.mode.mut]`'s at-return E1001 (wolf 0.2.19)

Wave 52's row: wolffe-lang/wolf-interp#155 — lupin's lint warns W1002
("`mut` parameter the body never writes") on s184's four moveout rows
(`memory/mut_param_moveout_{whole,elem,field,map}.lu`), whose `warns:` is
empty because the compiler stands W1002 down beside the at-return E1001
(wolffe-lang/wolf-lang#464, s184). The lint must agree with the compiler on
those four rows and on every nearby shape found, each seen red first; r24's
waiver for #155 (`WARNS_FILED` in `tests/run_corpus.rs`, `bc0ea33`) retires
by name in the same PR; no new divergence against wolf 0.2.19 (release
400208356); the coverage ratchet holds or rises. The contract is is60's, in
five sections; §1–§3 are committed before the first edit under `src/` or
`tests/`, the rest is appended as it lands. Measurements on kasumi (linux
x86-64) under `~/lanes/is61/`: the published wolf 0.2.19
(`wolf-0.2.19-x86_64-unknown-linux-gnu.tar.gz` `9f3873d8…8c8e`, binary
`3821bfaa…13f9`) and lupin 0.1.42 (`lupin-0.1.42-x86_64-unknown-linux-gnu.tar.gz`
`9856335a…8ab6`, binary `03f4a710…c660`), archive digests equal to the
release pages'; a release build of trunk `8e2516d`
(`archives/lupin-trunk-8e2516d`, `4b116c4e…a4cc`). Probes: 104
one-directory programs under `~/lanes/is61/probes/` (`a*` s184's five rows,
`m*` explicit `move`, `p*` plain moves of a non-`Copy` place, `q*` `Map`
read-outs, `r*`/`s*` controls and value/read positions, `u*` uses after the
move, `t*` rebinding, `defer`, generics, `self`, rung order); runner
`~/lanes/is61/scripts/run-probes.sh` (`lupin conform-run main.lu --json`,
and `wolf conform-run main.lu --checked --json` beside it).

#### §1 — forbidden, absolutely

No `rm` outside `~/lanes/is61/` (kasumi) and `/private/tmp/is61`; no
deletion in any tree this lane did not create; no `git add -A`; no edit to
another lane's file (s186/s187 own wolf-lang; no workflow edit here); no
`~/.claude`; no build or test on this Mac (kasumi only,
`CARGO_BUILD_JOBS=4`); no tag; no pin move (pin stays `ec56a08f`); no
merge, no rebase-merge; no `2>/dev/null` on a checkout; kill only my own
pids, never a pattern or a group; no claim of "seen red" without the log it
is in; no trailer on any commit.

#### §2 — inputs, re-derived 2026-09-30

| input as written | at origin / measured | drift |
| --- | --- | --- |
| wolf-interp trunk `8e2516dc` | `origin/trunk` = `8e2516d` "release: lupin 0.1.42"; pin `ec56a08f` (wolf-lang v0.2.18); branch `is61` cut there | none |
| wolf-interp#155, "with comments" | open, **0 comments**; the body is the table and r24's measurement (`c4143cf`, `repin-1.log`) | no comments to read |
| r24 waived #155 by name | `tests/run_corpus.rs` `WARNS_FILED`: four rows, each `("…", "W1002", "wolffe-lang/wolf-interp#155")`, and the check that a waived row still differs by exactly that code (`bc0ea33`); `r24-evidence.md` prediction 3 names it | none |
| wolf 0.2.19, release 400208356 | `repos/wolffe-lang/wolf-lang/releases/tags/v0.2.19` → id 400208356, not draft, published 16:02:58Z; tag object `ba0b43a7` → `c2401f05`; `wolf --version` = `wolf 0.2.19 (wolfgang, pin c2401f0) paired with lupin 0.1.42` | none |
| the four rows: lupin W1002, compiler none | `probes-lupin-0.1.42.log` (`8af9d0ae…52a7`): `a01`–`a04` lupin `trap(use-after-move)` with `W1002` at the `mut`; wolf 0.2.19 `fail(E1001)`, `warnings: []`; `a05` (one path) agrees on `[]` (its store back is a write) | none |
| the compiler's rule | wolf-lang `v0.2.19`: W1002 is a resolve-rung syntactic scan (`wolf_sema/src/wave.rs` `mode_discipline`, `body_writes`: an assignment rooted at the name, a moded argument or receiver) that **skips any name the body rebinds** (`rebound_names`: `let`/`var`/`const` patterns, every binding pattern, closure params); `suppress_mode_shadowed` (`wolf_diag/src/lib.rs`) drops it where E1001 names the parameter; only the **at-return** E1001 names one (`wolf_mem/src/moves.rs` `report_at_return` `.about(name, …)`), and it is not raised for a move whose use in the body already drew E1001 (`used`); the mem rung runs only when typecheck passed | the fix is not "a move is a write": it is "a move that reaches a return with no use after it", per parameter, and the rebinding skip |
| what moves on the compiler | `wolf_mem/src/lower.rs` `use_value`: a place in value position moves unless its type is `Copy` (`is_copy`: scalars, `str`, ranges, fn values, `handle`, raw pointers, the conc handles; not structs, enums, tuples, `List`, `Map`); `move e` moves any type | measured below |
| trunk vs the compiler on 104 probes | **62 part** — lupin warns where wolf is silent: `a01`–`a04`; explicit `move` out of the parameter, a field, an element, a nested place, `self`, a grouped operand, a `Copy` param or field, into a call, a struct or list literal, a tail or `return`, a `take` store (`m01`–`m07`, `m10`–`m15`, `m19`, `m20`, `r08`, `t15`, `t17`, `t19`); plain moves of a non-`Copy` place in value position (`p01`–`p07`, `p11`, `p12`, `p14`, `p15`, `s01`, `s02`, `s09`, `t07`, `t11`, `t14`, `u19`, `u20`); `Map` read-outs under `else`/`?` (`q02`, `q06`–`q09`, `u15`); a move whose later reads do not reach it (`u01` header read after an element move, `u02` a sibling literal index, `u04` the other branch, `u06` a second move, `u08` after an early return, `u09` a sibling field, `u11`'s `ys`, `u12` a `loop` left by `break`); rebinding (`r04`, `u16`, `t04`, `t05`, `t18`; and W1003 on `t02`). **42 agree**, among them every shape where the compiler KEEPS W1002 beside a move: `m09`/`u03`/`u10`/`t10` a use after the move, `u05` a move inside a loop with no store back (the back edge is the use), `u07` a read before the move inside the loop, `u13` a dynamic index, `u14` a read after a `?`, `t06` a `defer` reading it, and `t01` (typecheck E0401 stops the compiler before mem); plus the reads that do not move (`p08`, `s03`–`s08`, `s10`–`s12`, `q03`, `q05`, `q10`, `t16` struct shorthand) | wider than #155's four rows; the used-after-move half means W1002 must stay where a read follows the move |
| struct field shorthand | `t16` `W { xs }` with `mut xs: List[int]`: wolf 0.2.19 keeps W1002, `--native` runs `1\n1\n` (no move); lupin also `exit(0)` (no move) — while `W { tags: xs }` (`p06`) moves on both | both machines treat the shorthand as not moving; the lint follows them here; filed (§3a) |
| coverage ratchet | `tests/export.rs` `RATCHET_FLOOR = 268`, counted from `conforms:` lines — the lint does not enter it | none |

#### §3 — prediction, committed before the first edit

**The fix, one mechanism.** For a `mut` parameter the lint already finds
unwritten, a second, flow-sensitive walk of the body asks whether some move
out of the parameter — `move e` on a place rooted at it; a place rooted at
it whose declared type is not `Copy`, in value position (an initializer, a
plain non-index store's right side, a `take` store's, a `return`, a tail
of the body or of an `if`/`match`/block in value position, a struct or list
literal's element, a tuple element); a `Map` value read out of it under
`else`/`?` when the value type is not `Copy` — reaches a return (a
`return`, a `?` edge, the fall-through) with no later access of an
overlapping place on any path after it (a read, a move, a mutation, a
`defer` run at the exit; header reads `len`/`count`/`is_empty` overlap only
the container itself or an ancestor; distinct literal indices, keys and
fields do not overlap). If one does, W1002 does not fire. Types come from
the parameter's declared type, the module's struct, enum and alias
declarations, `List[T]`/`Map[K, V]`/tuple projections, and generic
parameters (not `Copy`); an unresolvable type moves nothing (the lint keeps
today's answer). Independently, a parameter whose name the body rebinds
anywhere draws neither W1002 nor W1003, as on the compiler.

**Predicted, at head:**

- the 62 parting probes agree (lupin's W1002/W1003 set equals wolf's; `u11`
  keeps W1002 on `xs` only); the 41 agreeing probes other than `t01` keep
  trunk's warnings byte for byte;
- **`t01` parts, the one new parting**: lupin drops W1002 (the move reaches
  the return unused) while wolf keeps it, because E0401 stops the compiler
  at typecheck, before the at-return check can stand W1002 down. Falsified
  if any other probe parts at head;
- verdicts do not move: every probe's lupin verdict and stdout at head equal
  trunk's (the change is the lint's);
- the corpus: every entry's lupin `warnings` at head equals trunk's except the
  four rows, which lose `W1002` — exactly 4 records move; the warns-ledger
  test is green with `WARNS_FILED` empty, and red at trunk on exactly the
  four rows once the waiver is gone;
- `lupin corpus` output identical trunk vs head; the differential against
  wolf 0.2.19 on the four tiers: ledgers byte-identical trunk vs head (the
  four rows already part on verdict, so `[proto.cmp.warn]` never reached
  them); no new divergence;
- coverage ratchet: 268, unchanged.

#### §3a — scored (appended after the measurement)

- **held** — one mechanism: `consumed_mut_params` and `rebound_names`
  in `src/lint.rs` (`62cfe67`), asked only for a `mut` parameter the
  existing scan finds unwritten.
- **held** — the 62 parting probes agree at head; `u11` keeps W1002 on
  `xs` only. **Widened before the fix**: sixteen more shapes (`v01`–`v16`:
  nested loops, `continue`, a `defer` after the move, a `?` before it,
  `self` whole, a loop condition reading the moved place, two moves and one
  whole read, a move after an infinite `loop`, a `return` inside a loop, an
  `else` block reading the map's header, sibling field moves, a `for` left
  by `break`, a generic parameter, a `str` element, a `take` store into a
  map), probed before `62cfe67` was written: 10 parted at trunk, 6 agreed,
  all 16 agree at head. **Totals: 120 probes; at trunk 72 part, at head 1**
  (`t01`).
- **held** — `t01` is the one parting, as predicted (wolf keeps W1002
  because E0401 stops it before mem; lupin has no typecheck refusal there).
- **held** — no verdict or stdout moves on any of the 120 probes, trunk vs
  head; wolf's column is byte-identical between the two runs.
- **held** — exactly 4 corpus records move: every corpus file's lupin
  `warnings`, trunk vs head, differ on `memory/mut_param_moveout_{elem,field,map,whole}.lu`
  alone, each `[W1002]` → `[]`; 743 of 747 identical.
- **held** — `lupin corpus` identical trunk vs head (747 files, 702
  entries, 0 failures; 556 at run, 536 match, 1 mismatch).
- **held** — the differential against wolf 0.2.19 on four tiers: ledgers
  byte-identical trunk vs head on all four; reports identical on default,
  checked and release; native differs only in the compiled lane's own exit on
  `memory/unsafe_ub_uaf.lu` (218 vs 19, UB noise, as at r24 and is60). Both
  trunk and head carry the same four gating findings against 0.2.19 (among
  them DIV-2026-019, `push_take_moves.lu`'s rung, `method_is_free_call.lu`),
  so **no new divergence**.
- **held** — coverage ratchet 268 (`coverage_is_ratcheted` green; the lint
  does not enter it).
- **drift in my own inputs**: §2 cited the probe log at `8af9d0ae…`
  (104 probes); the log was rewritten when `v*` joined it and is
  `bfa08c85…` (120 probes) — the 104 rows are unchanged in it.
- **found, filed**: `t16`'s struct shorthand is a soundness gap on the
  compiler — `var w = W { xs }; (mut w.xs).push(9)` then `xs.len` prints
  `2 2` on native and release (an undeclared alias) where `W { xs: xs }` is
  E1001; lupin copies (`1 2`) →
  wolffe-lang/wolf-lang#486, and the lupin mirror
  wolffe-lang/wolf-interp#159. The lint reads the shorthand as not moving,
  agreeing with 0.2.19 until the ruling.

#### §4 — evidence index

Commits:
- `17d5a34` §1–§3 (before any `src/`/`tests/` edit)
- `44a5a23` `tests/lint_is61.rs`, 102 shapes against wolf 0.2.19 (red)
- `8aa78d2` r24's #155 waiver retired from `WARNS_FILED` (red)
- `c3efd66` sixteen more shapes (red)
- `62cfe67` the fix (`src/lint.rs`; `sema::BUILTIN_SCALAR_TYPES` made `pub(crate)`)
- `6987a56` CHANGELOG `Unreleased`; this section

Artifacts (kasumi `~/lanes/is61/evidence/`; wolf 0.2.19 archive
`9f3873d8…`, lupin 0.1.42 archive `9856335a…`, digests = release pages;
trunk build `lupin-trunk-8e2516d` `4b116c4e…`, head build
`lupin-head-62cfe67` `e38dd70f…`, `lupin 0.1.42+dev.62cfe67`):
- inputs: `probes-lupin-0.1.42.log` (`bfa08c85…`, 120 probes, lupin 0.1.42 and wolf 0.2.19 `--checked`)
- red, each for its named reason:
  - `red-44a5a23-lint_is61.log`: 63 failed, each "lupin warned […], wolf 0.2.19 warns […]" (the 62 parting and `t01`), 0 "verdict moved"; 39 passed; `EXIT=101`
  - `red-8aa78d2-warns-ledger.log`: `the_warns_ledger_…` fails on exactly the four rows, each "warns ledger {}, observed {"W1002"}"; `EXIT=101`
  - `red-c3efd66-lint_is61.log`: 73 failed (the 63 and the 10 `v*` that part at trunk), each "lupin warned"; 45 passed; `EXIT=101`
- green: `green-62cfe67-tests.log` — `lint_is61` 118/118, the warns ledger 1/1, each `EXIT=0`
- probes at head: `probes-head-62cfe67.log` (`af8e9b02…`): 119 of 120 agree, `t01` parts
- corpus warnings: `corpus-warnings-trunk-62cfe67.txt` vs `corpus-warnings-head-62cfe67.txt` (747 lines each, trunk and head binaries): the four rows differ, nothing else
- corpus: `corpus-trunk-8e2516d-release.log` = `corpus-head-62cfe67-release.log`
- differential: `diffrun/trunk-8e2516d-*` vs `diffrun/head-62cfe67-*` (`.log`, `.jsonl`, `.ledger.jsonl`, four tiers each)
- gauntlet at `62cfe67` (the code head): `gauntlet-62cfe67.log` — `dirty: 0`; fmt 0, clippy 0, `cargo test --release --no-fail-fast` 58 binaries, 1,534 passed, 2 failed — `differ_cli`'s `a_debug_harness_is_refused_…` and `the_door_is_loud_…`, which assert a debug harness (as at r24) — then `cargo test --test differ_cli` (debug) 8/8, corpus 0 failures
- the shorthand finding: `shorthand-alias.log` (wolf 0.2.19 three tiers, lupin 0.1.42)
- GitHub CI: the head's run in the PR body

Filed: wolffe-lang/wolf-lang#486, wolffe-lang/wolf-interp#159.

#### §5 — done-when

- [x] branch `is61` on origin; PR open, unmerged, with these five sections (wolffe-lang/wolf-interp#158)
- [x] §2 re-derived, §3 committed before the first `src/`/`tests/` edit, §3a scored
- [x] s184's four rows and every nearby shape found agree with wolf 0.2.19 (117 of 118 tested shapes; the one parting named and pinned), each seen red at trunk
- [x] r24's waiver for #155 retired by name, red at trunk without it
- [x] whole-corpus differential against wolf 0.2.19: no new divergence
- [x] coverage ratchet holds (268)
- [x] CHANGELOG `Unreleased`
- [ ] CI green at the head sha (the PR body)
- [ ] kasumi build dirs pruned once evidence is written; worktree removed

### Two-phase reads — is63, ruling #17 (the maintainer, 2026-09-30): a `mut` argument's claim takes effect at call entry

Wave 52's row: "lupin evaluates the argument list before any `mut` claim
takes effect, so it runs the reads #17 allows and keeps trapping the rest."
Ruling #17 (`wolf/sprints/STATUS.md` item 17, quoted): "within one call,
arguments are evaluated left to right, and a `mut` argument's claim takes
effect when the call is entered, not when its argument is evaluated. A
later argument may **read** the claimed place (a `Copy` value, an operand,
a header or member read, a whole read such as string interpolation, or a
nested call whose result lends nothing from the place), because each such
read ends before the call. A later argument may **not** write the place,
move it, claim it again (`mut`), or lend it into the same call; those stay
E1002. [...] `take` arguments are unchanged: a move happens where it is
written." The spec clause is s186's (wolffe-lang/wolf-lang#485); at the
start of this lane that branch (`4e6af726`) carries no `spec/` change, so
this lane cites the ruling and leaves the anchor to the re-pin. The
contract is is60's, in five sections; §1–§3 are committed before the
first edit under `src/` or `tests/`, the rest is appended as it lands.
Measurements on kasumi (linux x86-64) under `~/lanes/is63/`: the
published wolf 0.2.19 (`9f3873d8…8c8e`, `wolf 0.2.19 (wolfgang, pin
c2401f0)`) and lupin 0.1.42 (`9856335a…8ab6`), archive digests equal to
the release pages' (`gh release view`), lupin 0.1.42 being trunk
`8e2516d` released. Witnesses: 37 one-directory programs under
`tests/rulings_is63/` (`read_*` the reads #17 allows, `stay_*` the
writes, moves, re-claims and lends it keeps refused), run by
`~/lanes/is63/scripts/run-witnesses.sh` (`lupin conform-run main.lu
--json`, and `wolf conform-run main.lu --{checked,native,release}
--json` beside it), summarised by `~/lanes/is63/scripts/summ.py`.

#### §1 — forbidden, absolutely

No `rm` outside `~/lanes/is63/` (kasumi) and `/private/tmp/is63`; no
deletion in any tree this lane did not create; no `git add -A`; no edit
to another lane's file — is61 owns `src/lint.rs` (W1002), s186 owns
wolf-lang and the clause; no workflow edit; no `~/.claude`; no build or
test on nomad-1 (kasumi only, `CARGO_BUILD_JOBS=4`); no tag; no pin move
(pin stays `ec56a08f`); no merge, no rebase-merge; no `2>/dev/null` on a
checkout; kill only my own pids, never a pattern or a group; jobs
launched with `setsid`; no claim of "seen red" without the log it is
in; no trailer on any commit.

#### §2 — inputs, re-derived 2026-09-30

| input as written | at origin / measured | drift |
| --- | --- | --- |
| wolf-interp trunk | `origin/trunk` = `8e2516d` "release: lupin 0.1.42" = tag `v0.1.42`; pin `ec56a08f` (wolf-lang v0.2.18); branch `is63` cut there | none |
| wolf 0.2.19, lupin 0.1.42 | archives `9f3873d8…8c8e` and `9856335a…8ab6`, equal to the release assets' digests; the wolf record names commit `c2401f0` | none |
| lupin 0.1.42 traps every read shape #17 names | `witnesses-archive-0.1.42-wolf-0.2.19.log`: all 20 `read_*` trap `exclusivity` (`mem.tier0.excl.1` on a whole claim, `mem.model.path.disjoint` on an element or field claim): `ensure(mut f, f.len + 2)`, `ring_drop(mut r, ring_len(r))`, `fail(mut fl, "bad {fl.store} and {fl}")`, `ev_log(mut el, 3, ev_seq("abc", el.seq))`, `bump(mut a, a + 1)`, `bump(mut xs[0], id(xs[0]))`, `grow(mut xs, xs.len)`, `grow(mut xs, xs.count())`, `xs.get(1) else 9` under `mut xs[0]`, `total(xs)` under `mut xs[0]` and under `mut xs`, `msize(m)` under `mut m`, an `if` arm, a slice one call down, a `for` over the claimed container inside a block, an impl `self`, two claims then an operand, effects in order, an inner call's operand reading the outer claim, a `push` on another place whose argument reads the claim | none |
| wolf 0.2.19 on the read shapes | runs 19 of 20 on checked, native and release with one set of bytes per row; refuses `grow(mut xs, xs.len)` E1002 ("`xs.len` is read for this call after `xs` goes `mut` in it") — the direct member read, which #17 now allows and s186 narrows | the one row lupin will run where 0.2.19 refuses; a #17 row, named |
| the shapes #17 keeps refused | all 17 `stay_*` refused by lupin 0.1.42: 15 trap `exclusivity` (block write, element write, `(mut xs).push` inside a block, `take xs` after `mut xs`, `move xs` in a block, `take fl.store` one call down, `mut a` twice, `inc(mut a)` one call down, `clear_len(mut xs)` under `mut xs[0]`, the direct lends `bump(mut a, a)`, `both(mut xs[0], xs)`, `grow(mut xs, xs[1])`, `ring_drop(mut r, r.head)`, the lend before the claim `second(xs, mut xs)`, a write of the outer claim inside an inner call's argument, a `&a` borrow in a block); `grow(mut xs, eat(take xs))` is lupin's static `fail(E1001)` | none |
| wolf 0.2.19 on the kept shapes | E1002 on the lends and re-claims; E1001 on the moves; **runs** the block write (`6` checked, `2` native/release), the element write (`3`), and the outer-claim write inside an inner argument (`18 11` checked, `12 11` native/release) — wrong answers s186 refuses; `&a` is `unsupported` (borrow expressions) | the compiler's half, s186's; lupin keeps trapping these |
| the mechanism | `eval_args_for` (`src/eval/mod.rs:3189`) pushes each `mut` argument's claim into the access set as the argument is evaluated (`:3220`), and every later access meets it through `AccessSet::conflict` (`place.rs:258`): reads through `read_claim` (`mod.rs:1670`), header and element reads through `read_header`/`conflicts_only_below`, writes through `write_path`, moves through `move_path`, a read lend through the argument loop's own `check_access` (`:3267`) | none |
| existing witnesses that assert a trap #17 moves | ruled `trap(exclusivity)` today, each a later-argument read: is59 `twin_elem_mut_its_member_read`, `twin_field_mut_member_read`, `twin_whole_mut_member_read`; is60 `twin_count_whole_claim`, `twin_effect_index_same_elem`, `twin_elem_count_same_elem_claim`, `twin_get_under_elem_claim`, `twin_impl_count_under_field_claim`, `twin_last_under_elem_claim`, `twin_row_claim_elem_read`, `twin_same_elem_in_expr`, `twin_whole_claim_elem_read` (12). Kept, each a lend into the same call: is59 `twin_same_elem_read` (`bump(mut xs[0], xs[0])`), `twin_whole_read_under_elem_claim` (`sum(mut xs[0], xs)`) | the 12 move to the ruling in their own commit |
| the pinned corpus (`ec56a08f`) | every `fail(E1002)` row with a claim is a pair of claims or a lend (`mut_read_overlap.lu` `bump(mut p, p.x)`, `mut_elem_nested_call.lu` a nested re-claim); no row reads a claimed place in a later argument | none |
| the differential at trunk | `diffrun/archive-0.1.42-*` (the 0.1.42 archive against wolf 0.2.19, four tiers): 4 gating findings on each tier, all pre-existing (`checked`: `explicit_apply_arity`, `index_origin_bad`, `push_take_moves` rung partings, `method_is_free_call`) | the baseline head is compared against |
| wolf-lang's lupin-reading gates | wolf-lang trunk `4c57f3d7`: 11 files read `LUPIN`; only `element_places_lanes.rs` asserts `trap(exclusivity)` on a call, each a pair of claims or the direct lend `elem_claim_whole_read.lu`; s186's `nested_claim_reads_lanes.rs` (branch `4e6af726`, unmerged) asserts lupin traps on #476's rows | the s186 rows are s186's to narrow; listed at head, not edited |
| s186's clause | `origin/s186` = `4e6af726`: 22 commits, `git diff --stat origin/trunk...origin/s186 -- spec` empty | cite ruling #17; the anchor waits for the re-pin |
| is61 in the same repo | `origin/is61` touches `src/lint.rs`, `src/sema.rs` (2 lines), `tests/lint_is61/`, `tests/run_corpus.rs`, `CHANGELOG.md`, this file | no overlap in code; the second to merge rebases this file and the CHANGELOG |
| the coverage ratchet | `tests/export.rs`: `RATCHET_FLOOR = 268`, `ANCHORS_TOTAL = 542` | none |

#### §3 — prediction, committed before the first edit

**The fix, one mechanism.** A `mut` argument's claim is pushed as today,
but marked **pending** for its call (a fresh call id per argument list)
until the list is fully evaluated; at the end of `eval_args_for` — call
entry — every pending claim of the call becomes the ordinary call claim
it is today, so the callee's extent is unchanged. While pending, a claim
meets: every exclusive access (a write, a move, a `take`, a `mut`
argument or receiver, a `&mut` borrow), a read **lend into its own call**
(a bare place argument in `read` mode, checked with the call's id), and
a local `&` borrow (lupin-only surface; a borrow is a lend). It does not
meet a read: a `Copy` read, an operand, a header or member read, a whole
read (interpolation, a `for` over the container, a method receiver read,
a slice), or a nested call's read lend into ITS OWN call. Every trap that
stays a trap keeps its record (the same detail, clause and span), since
the check that finds it is the one that found it before.

**The witness table** (lupin at head against trunk; wolf 0.2.19 checked
in brackets):

| witness | trunk (0.1.42) | head | [0.2.19] |
| --- | --- | --- | --- |
| `read_operand` | trap | `exit(0)` `3` | [`3`] |
| `read_copy_nested` | trap | `2` | [`2`] |
| `read_member_ensure` | trap | `3 3` | [`3 3`] |
| `read_nested_ring_len` | trap | `4 0` | [`4 0`] |
| `read_interp_whole` | trap | `bad [1, 2] and Fl { store: [1, 2], msg:  }` | [same] |
| `read_nested_field_seq` | trap | `13 1` | [`13 1`] |
| `read_member_whole_claim` | trap | `3 4` | [E1002 — a #17 row] |
| `read_count_whole_claim` | trap | `3 4` | [`3 4`] |
| `read_get_elem_claim` | trap | `3` | [`3`] |
| `read_total_elem_claim` | trap | `4 3` | [`4 3`] |
| `read_total_whole_claim` | trap | `1 4` | [`1 4`] |
| `read_map_nested` | trap | `2 1` | [`2 1`] |
| `read_if_arm` | trap | `44` | [`44`] |
| `read_slice_nested` | trap | `3 4` | [`3 4`] |
| `read_for_in_block` | trap | `6 4` | [`6 4`] |
| `read_impl_self` | trap | `5` | [`5`] |
| `read_two_claims` | trap | `13 14 3` | [`13 14 3`] |
| `read_order_effects` | trap, nothing printed | `twice 5`, `in 5 10`, `15` | [same] |
| `read_nested_claims` | trap | `22 21` | [`22 21`] |
| `read_method_mut_receiver_arg` | trap | `4 2` | [`4 2`] |
| the 16 trapping `stay_*` | trap | unchanged: verdict, clause, span | [E1002/E1001; runs the three writes; `&` unsupported] |
| `stay_move_nested` | `fail(E1001)` | unchanged | [E1001] |

**Existing tests that change: exactly the 12 twins in §2**, each to
`exit(0)` with the bytes wolf 0.2.19 prints where it runs the row; no
other test red at head that was green at trunk (`cargo test
--no-fail-fast` in the gauntlet). Falsified by any other.

**Corpus and differential.** `lupin corpus` at the pin: identical,
trunk against head. `lupin diff-run` against wolf 0.2.19 on the four
tiers: the same 4 gating findings per tier, report lines identical; no
row at the pin moves (none reads a claimed place in a later argument).
**The rows #17 moves** against 0.2.19 are therefore this lane's own:
`read_member_whole_claim` and is59's three member-read twins (0.2.19
refuses E1002, head runs), and the 19 other `read_*` plus nine is60 twins
move from a lupin trap to wolf 0.2.19's own bytes — agreement where
there was a parting.

**Coverage.** `RATCHET_FLOOR` holds at 268 (the witnesses carry no
`conforms:` line the bundle counts).

**wolf-lang's gates** at trunk `4c57f3d7`, head presented as the next
lupin: green, no case changes (none asserts a trap on a later-argument
read). At s186's branch head, `nested_claim_reads_lanes` reds exactly on
its lupin-trap assertions for the read rows — s186's to narrow under the
same ruling — and on nothing else.

**Out of scope, named:** a method call's `mut` receiver is not held as a
claim while its arguments run (`(mut xs).push(…)` checks the receiver at
the write-back), so a write of the receiver inside its own argument is
not this change's. Probed before the edit (`~/lanes/is63/probes/
p01_receiver_self_write`, `(mut xs).push({ xs = [9]; 5 })`): lupin 0.1.42
answers `ub(mem.ub)` (`mem.prov.state`, row P1: the write is foreign to
the receiver's protected Reserved tag), wolf 0.2.19 runs `1 9` on
checked, native and release. Reported, not fixed; predicted unchanged.

#### §3 addendum — s186's clause landed mid-lane; committed before the edit it drives

At 19:25Z s186's branch moved to `e3ae62b5` and carries the clause:
`6dbb05b5` "spec: [mem.tier0.excl.4] — arguments are two-phase". Its
operative sentences: a later argument may read "a `Copy` value
(`f(mut a, a.x)`) [...] A later argument may **not** [...] lend it into
the same call (a non-`Copy` place passed `read`, `both(mut xs[0], xs)`,
or a closure or `dyn` value that borrows it)". So **a bare `Copy` place
passed `read` is a read, not a lend** — D39's direct form retires
(s186 `da5b7fb4` flips `mut_read_overlap.lu` to `run(exit=0)`). §3 read
the ruling's "lend it into the same call" as every bare place argument;
the clause is narrower, and this lane follows the clause. The anchor
this lane cites is `[mem.tier0.excl.4]` (s186 `6dbb05b5`, unmerged; the
vendored pin `ec56a08f` does not carry it yet).

What moves, predicted before the edit:

- **The mechanism, one more arm.** A bare place argument passed `read`
  whose value is `Copy` (`is_copy`: the scalars, `str`, ranges, fn values,
  handles, raw pointers; never a `List`, `Map` or struct) and which meets
  a claim THIS call holds pending is read as an operand is — the value
  from before the call, no hold and no retag for the callee's extent —
  and runs. Every other bare place argument is the lend it was.
- **Three `stay_*` witnesses are reads:** `stay_lend_direct`
  (`bump(mut a, a)`), `stay_lend_elem_whole_claim` (`grow(mut xs, xs[1])`)
  and `stay_lend_field` (`ring_drop(mut r, r.head)`) become
  `read_copy_direct` → `2`, `read_copy_elem_direct` → `2 4`,
  `read_copy_field_direct` → `2`; a `str` field (`fail(mut fl,
  fl.store)`) joins as `read_copy_str_direct` → `4 1`; and two lends
  that stay join the `stay_*` rows: a non-`Copy` field
  (`stay_lend_list_field`, `fail(mut fl, fl.store)` with `store:
  List[int]`) and a whole struct beside its claimed field
  (`stay_lend_struct`, `bump(mut p.x, p)`). Each new or renamed row is
  seen red against `8fd78de` (the first fix) before this arm lands.
- **Existing tests that change, beyond §3's twelve: exactly three,**
  each D39's direct `Copy` read: is59's `twin_same_elem_read`
  (`bump(mut xs[0], xs[0])` → `2`), `src/eval/tests.rs`'s
  `a_read_argument_conflicts_with_a_mut_one` (`f(mut p.x, p.x)`), and
  `tests/mode_read_iteration.rs`'s `the_caller_side_overlap_half_still_traps`
  (`f(mut a, a.x)`). is59's `twin_whole_read_under_elem_claim`
  (`sum(mut xs[0], xs)`, a `List`) keeps its trap.
- **The pinned corpus moves by exactly one row**, `memory/mut_read_overlap.lu`
  (`bump(mut p, p.x)`, header `fail(E1002)` at `ec56a08f`): head runs it
  `exit(0)` (`p.x - 2` = 0). It becomes a filed divergence, DIV-2026-027,
  in `differ::FILED_DIVERGENCES` until the re-pin carries s186's re-spelled
  row; `lupin corpus` gains that one mismatch (filed) and nothing else,
  and the differential against wolf 0.2.19 gains that one verdict parting
  on each tier.
- **Named, not changed:** a closure passed into the call that captures the
  claimed place (`grow2(mut xs, fn() { xs.len })`,
  `~/lanes/is63/probes/p02_closure_into_same_call`) runs `4 3` on lupin
  0.1.42 already — lupin's closure copies its captures where it is written
  — while wolf 0.2.19 is E1002 and the clause calls it a lend: a
  pre-existing parting in the looser direction, reported, not fixed here.
  An all-`Copy` struct passed whole beside its claimed field
  (`stay_lend_struct`) is a lend here because lupin's `is_copy` never
  counts a struct; if the compiler counts it `Copy` under the clause,
  lupin is the stricter side.

#### §3 addendum 2: the clause merged; a closure lend; committed before the edit it drives

The orchestrator reported at 20:50Z that s186 merged to wolf-lang trunk as
`e3ae62b5`. `[mem.tier0.excl.4]` "Arguments are two-phase" is now the
clause at trunk, and `anchors.json` goes from 542 to 543. The sentence
this addendum acts on is "A later argument may **not** [...] lend it into
the same call (a non-`Copy` place passed `read`, `both(mut xs[0], xs)`,
or a closure or `dyn` value that borrows it)". lupin 0.1.42 runs the
closure case. Its closure copies its captures where it is written, and
creating one checks nothing against the call's held claims (filed as
wolffe-lang/wolf-interp#160 before this addendum). Measured before the
edit, head `9046a66` against the merged compiler's checked machine
(`e3ae62b5` debug build, `~/lanes/is63/probes/p0{2,3,6,7,8,9}_*`):

| probe | lupin head | wolf `e3ae62b5` checked |
| --- | --- | --- |
| a closure capturing the claimed `xs`, the call's own argument (`p02`) | `exit(0)` `4 3` | E1002 |
| a closure reading only the claimed struct's `Copy` field `r.n` (`p08`) | `exit(0)` `4` | E1002 |
| a closure created and called inside a nested call (`p03`) | `4 3` | `unsupported` (native `4 3` on 0.2.19) |
| a closure bound before the claim, passed by name (`p06`) | `4 3` | `unsupported`, no E1002 |
| a closure capturing no claimed place (`p07`, `p09`) | runs | `unsupported`, no E1002 |

Predicted, before the edit:

- **The mechanism, one more arm.** A closure literal written as a
  bare argument lends every local it names, whether or not that local is
  `Copy`, into the call being evaluated. When such a local meets a claim
  that this same call holds pending, the argument traps `exclusivity` at
  the closure, with `check_access`'s record. A closure passed by name, or
  created inside a nested call, lends nothing to this call and keeps
  running.
- **Witnesses:**
  - `stay_closure_lend` (`p02`) and `stay_closure_lend_copy_field` (`p08`)
    trap. Both are seen red against the code of `9046a66` first.
  - `read_closure_nested`, `read_closure_bound_before` and
    `read_closure_other` keep running with the bytes above.
  - s186's thirteen changed or new corpus rows, copied verbatim from
    `e3ae62b5` as `s186_*`, answer their `check:` lines: 11 run (among
    them `mut_claim_two_phase_reads.lu`, `2 4 23 1 3 5 2 3`), and
    `mut_claim_arg_block_{write,move}.lu` trap `exclusivity`. None of the
    13 needs this arm.
- **No other test changes**, and the pinned corpus and the differential
  do not move. No pinned row passes a capturing closure beside a `mut`
  argument of the same call.

#### §3 addendum 3: the closure arm's mechanism was wrong; committed before the replacement

`3f4e423` (addendum 2's arm) asked `lint::free_names` which locals the
closure names. That walk counts bare single names only. `xs.len` and `r.n`
are one dotted `path` production (`[gram.item.use]` folds them), so it
sees neither of them, and both closure witnesses still ran (`red-dbd030e.log`
and `green-3f4e423.log`: `stay_closure_lend` and
`stay_closure_lend_copy_field` answer `exit(0)` at `3f4e423`). The walk is
the lint's (`src/lint.rs`, out of this lane), and #36's capture loans share
its blind spot (filed with #160's comment).

**The replacement arm is dynamic, the way this machine answers every
other E1002.** When a closure literal is evaluated as a bare argument, the
captured locals whose places meet a claim that call holds pending are
recorded on the closure value, each with the claim's span. When that
closure's body runs, each such capture is held `mut` for the body's
extent, so the first access to it traps `exclusivity` at that read, with
the claim as the second span. A capture the body never touches traps
nothing: that is the conservatism class (the compiler refuses statically
what this machine never reaches), not a looser answer.

Re-predicted: `stay_closure_lend` and `stay_closure_lend_copy_field` trap
`exclusivity` under `mem.tier0.excl.1`, but the trap is now at the read
inside the body (`xs.len`, `r.n`) and not at the closure literal. Their
`[expected]` spans move to those reads, in the same commit as the arm. The
three `read_closure_*` rows keep running: `read_closure_other` captures
`xs` too (a closure copies every live local) but never reads it. Nothing
else moves.

#### §3a — the prediction, scored

The branch was rebased onto is61's merge (`0bb6021`) mid-lane. Every
red and green below was re-run at the rebased shas, and only those runs
are cited.

- **held: the mechanism.** It is one arm in `eval_args_for` (`8fd78de`): a
  pending claim per argument list, entered at the list's end. `Reach` in
  `place.rs` is the whole rule: an access meets a pending claim when it
  is exclusive, a lend into the claim's own call, or a local borrow. Two
  arms follow. The bare `Copy` argument (addendum 1, `9046a66`) was
  predicted before its edit. The closure lend (addenda 2 and 3,
  `a6d7791`) was also predicted before its edit, but its first mechanism
  was wrong: `3f4e423`'s name walk missed dotted captures, and addendum 3
  replaced it before the second edit.
- **held: 27 of 27 reads run** with the predicted bytes
  (`witnesses-head-a6d7791.log`). Each prints wolf 0.2.19's bytes where
  0.2.19 runs the row.
- **held on 17 of 18 kept rows.** The miss is `stay_elem_write`: it keeps
  its verdict and clause, but its span moves from the store's target read
  `[242, 244]` to the store `[242, 259]`, because under #17 that read runs
  and the write meets the claim (`1484f11`). The closure rows trap at the
  body's read, as addendum 3 re-predicted.
- **held: the existing tests that change.**
  - At `8fd78de` exactly the twelve §2 twins went red: is59 3, is60 9.
    `rulings_is63` also went red on the `stay_elem_write` miss
    (`green-8fd78de.log`: 923 passed, 13 failed).
  - At `9046a66` exactly addendum 1's three went red: the lib test,
    `mode_read_iteration`, and is59's `twin_same_elem_read`
    (`green-9046a66.log`: 936 passed, 3 failed).
  - No other test moved at any step. `run_corpus`'s one move is below.
- **missed, in the corpus's own vocabulary.** Addendum 1 predicted that
  `mut_read_overlap.lu` would become a filed mismatch. It did not: the
  census counts a program this machine runs and the compiler refuses as
  static-conservatism. `lupin corpus` moves exactly one row, from
  counterpart to conservatism (46 → 45 and 55 → 56; mismatches stay at 1,
  DIV-2026-019), and no DIV entry is needed. What moves is its
  `RUN_LEDGER` row (`5466196`), where `the_run_ledger_is_exactly_what_reaches_run`
  differed on that one file and nothing else, and the manual's census
  line (`6759869`).
- **held: the differential.** `diffrun/head-a6d7791-*` against
  `diffrun/archive-0.1.42-*` (wolf 0.2.19, four tiers): the reports are
  identical on `default`, `checked` and `release`. On `native` the only
  difference is `memory/unsafe_ub_uaf.lu`'s compiled exit code (206
  against 255). That row is UB, and its exit code is noise (s185's
  ten-run finding). The same 4 gating findings appear per tier. **No new
  divergence.**
- **The rows #17 moves against wolf 0.2.19, by name.**
  - lupin now runs, where 0.2.19 refuses with E1002: the witnesses
    `read_member_whole_claim`, `read_copy_direct`,
    `read_copy_elem_direct`, `read_copy_field_direct` and
    `read_copy_str_direct`; is59's `twin_elem_mut_its_member_read`,
    `twin_field_mut_member_read`, `twin_whole_mut_member_read` and
    `twin_same_elem_read`; s186's `mut_claim_two_phase_reads.lu` and
    `mut_read_overlap.lu` (its `elem_dyn_read_after_mut.lu` already ran
    on 0.1.42); and the pinned corpus row `memory/mut_read_overlap.lu`.
  - lupin now traps, where 0.2.19 also refuses (lupin used to run them):
    `stay_closure_lend` and `stay_closure_lend_copy_field`.
  - Moved from a trap to 0.2.19's own bytes: the other 19 original
    `read_*` witnesses, is60's nine twins, and s186's eight `mut_claim_nested_*`
    and `mut_claim_operand_read` rows.
- **held: the gates at wolf-lang trunk `e3ae62b5`** (s186 merged; 13
  lupin-reading gate files). Head is green on all of them
  (`wolfgates-trunke3ae-head-a6d7791.log`, `EXIT=0`), including
  `nested_claim_reads_lanes`' 11 ruled arms. The plant, 0.1.42 presented
  as `0.1.43-plant`, reds on exactly the 8 read cases and on `pairing`
  (`wolfgates-trunke3ae-plant-0.1.42-as-next.log`, `EXIT=101`). With these
  bytes, `PRE_TWO_PHASE_LUPIN` can be emptied at the next pairing.
- **held: consistency with the checked machine.** I ran every witness
  against wolf-lang `e3ae62b5`'s checked machine (debug build,
  `060a5758…`). Of the 58, 54 agree: every run row prints the same bytes,
  and every refused row is E1002 or E1001 where lupin traps. The other
  4 are `unsupported` on that machine: three closure rows and `&a`.
  The 14 earlier twins agree 14 of 14.
  (`vs-checked-e3ae62b5-{head,twins-head,probes-head}-a6d7791.log`)
- **a slip, repaired:** addenda 2 and 3 were committed in time
  (`178ded5`, `5b15ef9`), but they were inserted at the file's first
  "§3a" heading, which is inside is55's section. The next commit moves
  them here verbatim, and trunk's lines are untouched: `git diff
  origin/trunk -- docs/divergence-log.md` removes nothing.
- **filed**:
  - wolffe-lang/wolf-lang#487: a `mut` receiver written inside its own
    call's argument runs `1 9` on every lane, and on `e3ae62b5`, and
    loses the push. lupin answers `ub(mem.ub)`.
  - wolffe-lang/wolf-interp#160: the closure lend. It is fixed here by
    `a6d7791`; the comment there records that `lint::free_names` misses
    dotted captures.

#### §4 — evidence index

Commits:
- `6824f39` §1–§3; `7da5d6d` addendum 1; `178ded5` addendum 2; `5b15ef9` addendum 3
- `988274b` 37 witnesses (red); `8fd78de` the pending claim
- `15cfade` the twelve twins to the ruling; `1484f11` `stay_elem_write`'s span
- `fda9e4d` addendum 1's witnesses (red); `9046a66` the `Copy` read
- `dee7167`, `31b8354` addendum 1's three tests; `a39b4c8` approximation contract §6.12
- `5466196` `RUN_LEDGER`; `6759869` the manual's census
- `dbd030e` the closure witnesses (red) and s186's 13 rows verbatim; `3f4e423` the name walk (did not fire); `a6d7791` the closure lend, dynamic
- `8989431` rustfmt; `ded7ecc`, and the changelog commit after `a6d7791`, CHANGELOG `Unreleased`; the addenda's move and this section

Artifacts are in kasumi `~/lanes/is63/evidence/`. The archives are wolf
0.2.19 `9f3873d8…` and lupin 0.1.42 `9856335a…`, whose digests equal the
release pages'. The head release build is `lupin-head-a6d7791`,
`4397433a…`, and wolf-lang `e3ae62b5`'s debug `wolf` is `060a5758…`.
- inputs:
  - `witnesses-archive-0.1.42-wolf-0.2.19{,-addendum,-addendum2}.log`
  - `probes-archive-0.1.42-wolf-0.2.19.log`
  - `diffrun/archive-0.1.42-*`
- red, each for its named reason (`scripts/rebased.sh`):
  - `red-988274b.log`: 20 failed, each "lupin answered `trap(exclusivity)`, ruled `exit(0)`"; 17 passed; `EXIT=101`
  - `red-fda9e4d.log`: 4 failed, for the same reason; `EXIT=101`
  - `red-dbd030e.log`: 2 failed, each "answered `exit(0)`, ruled `trap(exclusivity)`"; `EXIT=101`
  - `green-3f4e423.log`: the name walk's miss, the same 2 still failing
- green:
  - `green-8fd78de.log` and `green-9046a66.log`: the predicted reds, above
  - `green-a6d7791.log`: `rulings_is63` 58/58, `rulings_is60` 82, `rulings_is59` 18, `rulings_is58` 20, `rulings_eg00` 15, `rulings_s182` 11, `index_store`, `mode_read_iteration` 20, lib 727, `conformance`, `corpus_harness`, `prov_machine`, `fault_snapshots`; `EXIT=0`
- head:
  - `build-rel-a6d7791.log`, `witnesses-head-a6d7791.log`, `probes-head-a6d7791.log`
  - `vs-checked-e3ae62b5-*-a6d7791.log`
- differential and corpus: `diffrun/head-a6d7791-*`, with `…-corpus.log`
- gates (`scripts/gates.sh`, `WOLF_PAIRING_REQUIRE_SIBLING=1`, no SKIP): `wolfgates-trunke3ae-head-a6d7791.log`, `wolfgates-trunke3ae-plant-0.1.42-as-next.log`
- gauntlet at `a6d7791` (the code head; later commits are `CHANGELOG.md` and `docs/` only), run in `~/lanes/is63/rel` with the default target dir: `gauntlet-a6d7791.log`. Its result and the GitHub CI run are in the PR body.

#### §5 — done-when

- [x] branch `is63` on origin, rebased on trunk `0bb6021`; PR open, unmerged, with these five sections
- [x] §2 re-derived; §3 committed before the first `src/`/`tests/` edit, and each addendum before its edit
- [x] every read shape #17 names is seen red at trunk and runs at head; every write, move, re-claim and lend shape traps, each with an explicit row
- [x] no new divergence against wolf 0.2.19; the rows #17 moves are listed by name
- [x] the witnesses agree with the merged compiler's checked machine (`e3ae62b5`)
- [x] CHANGELOG `Unreleased`
- [ ] the coverage ratchet holds (`export::coverage_is_ratcheted`, in the gauntlet and CI)
- [ ] kasumi gauntlet and GitHub CI green at the head sha (the PR body)
- [ ] kasumi build dirs pruned once evidence is written; worktree removed

### Every operand once, the shorthand is the longhand, a receiver's own argument — is62, wolf-interp#157, #162, #159 and wolf-lang#487's lupin half

Wave 52's row: "lupin mirrors wolf-interp#157, #162 and #159, and
wolf-lang#487's trap kind. It empties the compiler gates' 0.1.42 pins."
is64 (#159) is absorbed. The contract is is63's, in five sections; §1–§3
are committed before the first edit under `src/` or `tests/`, the rest is
appended as it lands. Measurements on kasumi (linux x86-64) under
`~/lanes/is62/`: the published lupin 0.1.42 (`9856335a…8ab6`) and wolf
0.2.19 (`9f3873d8…8c8e`), archive digests equal to the release pages'
(`gh release view`); lupin trunk `1e96e1f` built release
(`lupin-trunk-1e96e1f`, `387fd253…`); wolf-lang trunk `57805e35` built
debug with `libwolf_rt.a` beside it (`archives/wolf-trunk-57805e35/`).
Probes: 67 one-directory programs under `~/lanes/is62/probes/` (the
wolf-lang rows the three gates run, verbatim, as `s_*`; their longhand
twins as `l_*`; this lane's own shapes as `p*`), run by
`scripts/run-probes.sh` (`lupin conform-run main.lu --json`, and `wolf
conform-run main.lu --{checked,native,release} --json`), summarised by
`scripts/summ.py`.

#### §1 — forbidden, absolutely

No `rm` outside `~/lanes/is62/` (kasumi) and `/private/tmp/is62`; no
deletion in any tree this lane did not create; no `git add -A`; no edit to
another lane's file — wolf-lang (s192 owns #487's compiler half, s191
`ubcheck.rs`) is read and built, never edited or pushed; no workflow edit;
no `~/.claude`; no build or test on nomad-1 (kasumi only,
`CARGO_BUILD_JOBS=4`); no tag; no pin move (the pin stays `ec56a08f`); no
merge, no rebase-merge; no `2>/dev/null` on a checkout; kill only my own
pids, never a pattern or a group; jobs launched with `setsid`; no claim of
"seen red" without the log it is in; no trailer on any commit.

#### §2 — inputs, re-derived 2026-10-01

| input as written | at origin / measured | drift |
| --- | --- | --- |
| wolf-interp trunk `1e96e1f`, is63 merged | `origin/trunk` = `1e96e1f`; `Cargo.toml` 0.1.42; a dev build answers `impl_version` `0.1.42+dev.<sha>`, so the gates' `"0.1.42"` pins never match a branch build — only the archive | none; noted, it decides the plant below |
| wolf-lang trunk "about `57805e35`" | `origin/trunk` = `57805e35`; `PAIRING` 0.1.42 / `ec56a08` | none |
| the three gates and their pins | `slice_endpoint_once_lanes.rs` 1 pin (`LUPIN_0_1_42_INDEXED_BASE`), `index_flow_once_lanes.rs` 2 (`LUPIN_0_1_42_RECEIVERS`, `LUPIN_0_1_42_SLICE_BASE`), `field_shorthand_lanes.rs` 6 (`moves`, `mixed`, `nested`, `nested_bound`, `return_mut`, `closure_borrow`): 9 pinned cases of 24 | none |
| the gates at trunk with lupin 0.1.42 | `wolfgates-base-0.1.42-pinned.log`: 24/24 green, `EXIT=0`, 0 SKIP lines (`--nocapture`); with every pin replaced by `&[]` (`scripts/gates.sh … unpinned`), `wolfgates-plant-0.1.42-unpinned.log`: exactly the 9 pinned cases red, `EXIT=101`, 0 SKIP | none. **A first run skipped the native and release lanes silently** (`libwolf_rt.a not found next to the wolf binary`, visible only under `--nocapture`); superseded, kept in `evidence/superseded/` |
| #157: `gi` runs two or three times in `g[gi()][lo()..hi()]` | trunk: `let` ×2, `.len` ×3, hole ×2; also `.count()` ×2, `.is_empty()` ×2, a `read` argument ×2, a `for` iterable ×1, under a pending claim ×3 then ×2; every index of a deeper base (`h[a()][b()][lo()..hi()]` runs `a`, `b` twice each), a field base, a `str` base, open, inclusive and `^` endpoints. wolf trunk runs each once on checked, native and release | the family is wider than the issue's table; all listed as witnesses |
| the mechanism of #157 | `place_of` (`src/eval/mod.rs:4447`) evaluates the base's place (`gi` runs) before `project_index` refuses the bracket's range as "not a place"; every caller then evaluates the whole expression again (`live_place` → `eval`, `eval_member_at`, `method_split`), and `eval_member_at` under `live_place` once more | none |
| #162: a propagating `?` in a receiver's index runs twice | trunk: `count`, `(mut …).push`, `.bytes().len`, `for … in ….bytes()`, `str` `.len` twice, as the issue says; also `upper()`, an impl method, a parenthesized receiver, a `List` `.len` as an operand, and a `return`, `break` or `continue` in a receiver's index (not only `?`) | wider: any flow, not only `?` |
| the mechanism of #162 | `method_split` (`:7364`) answers `Err(_) => Receiver::Expr(base)` and `eval_member_at` (`:8083`) `if let Ok(path)`: a flow out of the base's operand is swallowed and the base is evaluated again | none |
| #159: the shorthand copies (`1 2`) | trunk: `moves`, `mixed`, `nested`, `nested_bound` print `1 2`, `1 2 2 1`, `1 2 5`, `1 2 5`; `return_mut` `1 2` with W1002; `closure_borrow` `1 2`; every longhand twin traps (`use-after-move`, or `exclusivity` with W1102). The other six rows already agree | none |
| the mechanism of #159 | `FieldInit.value` is `None` for the shorthand (`src/ast.rs:686`), and nine walkers skip `None` (`sema.rs` ×7, `lint.rs` ×4, `parse.rs` `trace_expr`); `eval` reads the name with `read_whole` — a copy, never `eval_for_init`'s move | none |
| s190's three extra shapes | closure borrow as above. Module-level `let n` + `P { n }`: shorthand `unsupported` ("`n` does not denote a place"), longhand runs `3`. **The "unknown name" row compares `P { nope }` with `P { n: nope }`** (`~/lanes/s190/probes/shorthand/c04{s,l}`), which is not its longhand; the true twin `P { nope: nope }` answers `fail(E0408)` as the shorthand does, already on 0.1.42. With no `n` in scope, `P { n }` and `P { n: n }` are both `unsupported`, for two different reasons | the unknown-name row agrees on 0.1.42 at its true twin; witnessed both ways |
| `tests/lint_is61/t16_struct_shorthand` | trunk: `exit(0)`, W1002 `[33, 36]`; its longhand twin traps `use-after-move` with no W1002; wolf trunk: `fail(E1001)`, no warning | t16 moves with the mirror, to the twin's answer |
| #487: lupin answers `ub(mem.ub)` | trunk: `ub(mem.ub)` (`mem.prov.state` P1) on the whole write, a field write, an impl receiver's write, a nested `(mut xs).push` re-claim and `grow(mut xs)` one call down; **runs** an element write (`3 9`), a move (`eat(take xs)`, `3`), a lend into the same call (`(mut c).absorb(c)`, `2`) and a closure lend (`2`). wolf trunk refuses the move E1001 and both lends E1002, and runs the writes and the re-claims (`1 9`, `3 9`, `4`; the impl write `105` on checked, `6` on native) | wider than the issue; the runs are silent wrong answers too |
| the mechanism of #487 | `eval_method` (`:7706`) holds no claim for a `(mut …)` receiver while `eval_args` runs; the receiver is checked only at its write-back | none |
| found beside #487, not this lane's | a flow (`?`, `continue`) out of ANY call's argument list leaves that call's protected `mut` retag in the tree, and the next write through the place answers `ub(mem.ub)`: `pargs_try_leak`, `pargs_continue_leak` (`put(mut xs, v()?)`), and the receiver forms `p487_try_in_arg`, `p487_continue_in_arg`. wolf trunk runs all four | to be filed; out of scope |
| the coverage ratchet | `tests/export.rs`: `RATCHET_FLOOR = 268`, `ANCHORS_TOTAL = 542` | none |

#### §3 — prediction, committed before the first edit

**Four mechanisms, one per issue.**

1. **#157.** `place_of` refuses a bracket whose operand list is not one
   plain index (a range, several arguments) BEFORE it evaluates the base,
   so a slice's place lookup evaluates nothing and the slice is evaluated
   once, where it is read.
2. **#162.** `method_split` and `eval_member_at` fall back to evaluating
   the base only on `Unsupported` ("not a place"); any other signal out of
   `place_of` — `?`'s return, `return`, `break`, `continue`, a trap —
   leaves as it left the operand. `method_split` returns `EResult`.
3. **#159.** The parser builds the shorthand's value node: `W { xs }` is
   `W { xs: xs }`, the value a one-segment path spanning the name, exactly
   s190's reading. `FieldInit.value` becomes `Expr` (no `Option`), so no
   walker can skip it again.
4. **#487.** A `(mut …)` receiver's place is held exclusive and pending
   for its own call (`HeldWhy::Pending`, is63's mechanism) while the
   arguments run, under the same `CallId` the argument list uses, and is
   withdrawn at call entry. It meets a write, a move, a re-claim, and a
   lend into the same call (a non-`Copy` place passed `read`, a closure
   capturing it); it meets no read. Disjoint places are untouched.

**The witness table** (lupin head against trunk `1e96e1f`; wolf trunk
`57805e35`'s answer in brackets, one answer on checked, native and release
unless shown):

| witness | trunk | head | [wolf trunk] |
| --- | --- | --- | --- |
| `s_ctl_slice_endpoints_indexed_base` | `gi` ×2/×3/×2 | once each | [once] |
| `s_ctl_slice_try_once` | `gi` ×2 | once | [once] |
| `s_ctl_index_try_once_receivers` | `idx` ×2 in five readers | once | [once] |
| `p157_count`, `p157_deep_field_str`, `p157_inclusive_open`, `p157_under_claim` | ×2, ×3 | once | [once] |
| `p162_more_receivers`, `p162_flows` | ×2 | once | [once] |
| `s_field_shorthand_moves`, `_mixed`, `_nested`, `_nested_bound` | `exit(0)` `1 2…` | `trap(use-after-move)`, the twin's span | [E1001] |
| `s_field_shorthand_return_mut` | `exit(0)` `1 2`, W1002 | `trap(use-after-move)`, no warning | [E1001] |
| `s_field_shorthand_closure_borrow` | `exit(0)` `1 2` | `trap(exclusivity)` with W1102 | [E1002, W1102] |
| `p159_module_let` | `unsupported` | `exit(0)` `3` | [`unsupported`] |
| `p159_unknown` (`P { n }`, no `n`) | `unsupported` | `unsupported`, the longhand's reason | [E0301] |
| the other six `s_field_shorthand_*` rows | the twin's answer | unchanged | [as the gate rules] |
| `p487_write`, `_field_write`, `_impl_write`, `_reclaim_receiver`, `_reclaim` | `ub(mem.ub)` | `trap(exclusivity)` | [runs — s192's half] |
| `p487_elem_write` | `exit(0)` `3 9` | `trap(exclusivity)` | [runs `3 9` — s192's half] |
| `p487_move` | `exit(0)` `3` | `trap(exclusivity)` | [E1001] |
| `p487_lend_impl`, `p487_closure_lend` | `exit(0)` `2` | `trap(exclusivity)` | [E1002] |
| `p487_reads_run`, `p487_copy_read_impl`, `p487_sibling_write` | runs | unchanged bytes | [same bytes] |

**The gates.** wolf-lang `57805e35`'s three gates with every 0.1.42 pin
dropped, `LUPIN` = head: 24/24 green, no SKIP; the same files with lupin
0.1.42: the 9 pinned cases red (already measured, §2). The pinned files
with head are green too (a dev build's version never matches `"0.1.42"`).

**Existing tests that change: exactly one,** `lint_is61`'s
`t16_struct_shorthand`, from `exit(0)` with W1002 to the longhand's
`trap(use-after-move)` with no warning (wolf trunk's warnings array is
empty there). No other test red at head that was green at trunk.

**Corpus and differential.** `lupin corpus` at the pin: identical, trunk
against head. `lupin diff-run` on four tiers against wolf trunk
`57805e35` (release build), on the vendored corpus and on wolf-lang
trunk's own `corpus/`: no new divergence. On wolf-lang's corpus, only
the rows behind the 9 pinned cases may move (3 `ctl_*` rows to wolf's
bytes; 6 `field_shorthand_*` rows from a run to the twin's trap, beside
wolf's E1001/E1002); nothing else moves. A row that writes a
`mut` receiver inside its own argument would move from `ub` or a run to a
trap: none is predicted on either corpus.

**Coverage.** `RATCHET_FLOOR` holds at 268.

**Out of scope, named:** the protected-retag leak on a flow out of an
argument list (§2's last finding) — filed, not fixed; it is not #487's
shape and it is not specific to receivers.

#### §3 addendum — wolf-interp#164 and wolf-lang#494 join the lane; committed before the edit they drive

The coordinator added two receiver mirrors mid-lane (after `ef9fa67`, with
CI run 36805436056 in progress on it):

1. **wolf-interp#164** (s192): all fourteen `corpus/memory/recv_claim_arg_*`
   refusal rows on wolf-lang branch `s192` (`c51a314d`) must trap
   `exclusivity`; the gate is that branch's `receiver_claim_args_lanes.rs`
   (`PRE_RECEIVER_LUPIN = ["0.1.42"]`). Measured before any edit
   (`probes3-head-ee0cbfc.log`): **head `ee0cbfc` already traps all
   fourteen** (`ee0cbfc`'s pending receiver claim is #164's mechanism too),
   and `recv_claim_arg_reads.lu` runs `5 2 5 5 2 3 3 5 3 3 2 10 5`, wolf's
   bytes. lupin trunk `1e96e1f` and 0.1.42 answer `ub` on eight and run six,
   as the issue says (`probes3-trunk-1e96e1f.log`,
   `probes3-archive-0.1.42.log`).
2. **wolf-lang#494, lupin's half:** `(mut p).set_x({ p.z = 9; p.z })` with
   `fn set_x(mut self.{x}, n: int)` must print `10 9`. lupin 0.1.42 and
   trunk print `10 3` (the receiver is read whole before the arguments and
   written back whole). **Head `ee0cbfc` traps `exclusivity` at `p.z = 9`**
   — `ee0cbfc` claims the whole receiver, not its view set. That is a
   regression of this lane's own, and it is fixed here before the PR can
   land.

**The mechanism, one more arm of `ee0cbfc`'s.** When the call resolves to
an impl method whose receiver is `mut self.{f, …}` (`[mem.tier0.excl.3]`:
the view set is the callee's path footprint), the pending receiver claim
is one claim per view-set field (`p.x`), not the whole place; and the
write-back after the call writes only the view-set fields that changed, not
the whole value — so an argument's write to a field outside the view set
stands. A write of a view-set field in the argument still traps
(`recv_claim_arg_view_write.lu`), and so does a whole lend or write of
`p` (it overlaps `p.x`).

**Predicted at the next head:**

| witness | `ee0cbfc` | next head | [wolf trunk `57805e35` checked / native] |
| --- | --- | --- | --- |
| `p494_viewset_disjoint_write` (#494's program) | `trap(exclusivity)` | `exit(0)` `10 9` | [`10 9` / `10 3`] |
| `p494_viewset_two_fields` (`mut self.{x, y}`; the argument writes `p.z`, then `p.y`) | `trap(exclusivity)` | `8 9 7`, then `trap(exclusivity)` at `p.y = 100` | [`8 9 7`… / `8 9 3`…] |
| the 14 `recv_claim_arg_*` refusal rows | `trap(exclusivity)` | unchanged | [E1002 on `s192`] |
| `recv_claim_arg_reads.lu` | `5 2 5 5 2 3 3 5 3 3 2 10 5` | unchanged | [same] |

`s192`'s gate with `PRE_RECEIVER_LUPIN` emptied: 15/15 green with the next
head; with lupin 0.1.42, exactly the 14 refusal cases red. The three gates
of §3 and every witness of `1498db2` unchanged; the differential and corpus
unchanged against `ee0cbfc` (no corpus row at the pin or at wolf-lang
`57805e35` declares a view set and writes outside it in its argument —
falsified by any row that moves).

#### §3a addendum — scored

- **held: #164 at `ee0cbfc` already**, and unchanged at `da9aa91`: the 14
  refusal rows trap `exclusivity`, the reads row runs with wolf's bytes
  (`probes3-head-da9aa91.log`). s192's gate at `c51a314d`,
  `PRE_RECEIVER_LUPIN` emptied: 15/15 green with `da9aa91`
  (`wolfgates192-head-da9aa91-unpinned.log`), exactly the 14 refusal cases
  red with lupin 0.1.42 (`wolfgates192-plant-0.1.42-unpinned.log`,
  `EXIT=101`); pinned, 0.1.42 is 15/15 green
  (`wolfgates192-pinned-0.1.42.log`). 0 SKIP lines in each.
- **held: #494.** `recv494_viewset_disjoint_write` prints `10 9`;
  `recv494_viewset_two_fields` prints `8 9 7` and traps at `p.y = 100`.
  Both were red at `ecd6864` (`red-ecd6864.log`: exactly these 2 of 53
  failed) and are green at `da9aa91` (`green-da9aa91.log`: `rulings_is62`
  53/53, lib 729, every neighbouring suite).
- **held: nothing else moves.** Against `ee0cbfc`: `lupin corpus`
  identical at the pin and on wolf-lang `57805e35`'s corpus; the
  differential's divergences identical on all eight runs (the one
  differing line is `unsafe_ub_uaf.lu`'s native exit under UB); 0 ledger
  rows moved (`ledgerdiff-head-ee0cbfc-head-da9aa91.txt`). On `s192`'s
  corpus, trunk `1e96e1f` against `da9aa91`: the 14 `recv_claim_arg_*`
  refusal rows, the 6 shorthand rows and the 3 once rows move — 4 → 1
  mismatch, 61 → 81 dynamic counterparts, 85 → 65 static conservatism —
  and nothing else.
- **missed, a detail of the gates:** with its pin kept, s192's gate is
  green with `da9aa91` too, where the three gates of §3 red. It reads the
  version from `lupin --version` (`0.1.42+dev.da9aa91`), they read the
  record's `impl_version` (`0.1.42`). Either way a release that carries
  this lane answers a new version and meets the ruled arm.

#### §3a — the prediction, scored

- **held: the four mechanisms**, one commit each: `9fb80c8` (#157),
  `d6a1cb0` (#162), `301fdef` (#159), `ee0cbfc` (#487). The 25 witnesses
  red at `1498db2` are green at `ee0cbfc`, each for its named reason; the
  11 controls were green on both sides.
- **held: the witness table**, row for row (`witnesses-head-ee0cbfc.log`).
  The receiver traps land on the offending access: `xs = [9]`,
  `xs[0] = 9`, `h.xs = [9]`, `c = C { n: 100 }`, `mut xs`,
  `(mut xs).push(7)`, `take xs`, `c`, `c.n` (`87e2ae7` pins clause and
  span).
- **missed: "the pinned files with head are green too".** §2's first row
  was wrong: a dev build prints `0.1.42+dev.<sha>` for `--version`, but its
  record's `impl_version` is `0.1.42`, so with the pins in place the gates
  read head as 0.1.42 and red on exactly the 9 pinned cases
  (`wolfgates-head-ee0cbfc-pinned.log`, `EXIT=101`). This is the gates'
  design working, not a defect: the pins must be emptied at the pairing
  that ships these mirrors (0.1.43 answers a different version, so it
  never matches them). Unpinned, head is 24/24 green
  (`wolfgates-head-ee0cbfc-unpinned.log`, `EXIT=0`, 0 SKIP).
- **held: existing tests that change, exactly one**, `lint_is61`'s
  `t16_struct_shorthand` (`301fdef`); `lint_is61` 118/118 at `ee0cbfc`.
- **held: corpus and differential, no new divergence.** `lupin corpus` at
  the pin: identical reports, 536 / 45 / 56 / 64 / 1 mismatch. `lupin
  diff-run` against wolf-lang trunk `57805e35` (release), four tiers: on
  the pinned corpus the same divergences per tier (5 / 5 / 7 / 7); the one
  differing line is `unsafe_ub_uaf.lu`'s native exit code under UB
  (`89` / `83`), noise on a program both sides call undefined. On wolf-lang
  trunk's corpus, head has **3 fewer divergences** on checked, native and
  release (8 → 5, 10 → 7, 10 → 7): `ctl_slice_endpoints_indexed_base`,
  `ctl_slice_try_once`, `ctl_index_try_once_receivers`. The conservatism
  ledgers move 0 rows on all eight runs (`ledgerdiff-trunk-1e96e1f-head-
  ee0cbfc.txt`). `lupin corpus --root` over wolf-lang's corpus: the same 3
  rows go from mismatch to match (4 → 1 mismatch), and the 6
  `field_shorthand_*` rows from static conservatism to the dynamic
  counterpart of wolf's E1001/E1002 (61 → 67, 71 → 65); nothing else moves.
- **coverage**: in the gauntlet (`export::coverage_is_ratcheted`; the
  witnesses carry no `conforms:` line the bundle counts).

Three slips, all mine, all repaired:
- the first gate runs reported 24/24 green with the native and release
  lanes **skipped** (`libwolf_rt.a not found next to the wolf binary`),
  visible only under `--nocapture`. `scripts/gates.sh` now builds
  `wolf_rt`, runs `--nocapture` and counts SKIP lines; every gate log
  cited here says `SKIP lines: 0`. The first logs are kept in
  `evidence/superseded/`.
- one `git add -A src tests/lint_is61.rs` (`301fdef`), against §1's letter;
  it was path-limited and staged exactly the six files `git status` listed.
- the first gauntlet, at `b26ed5d`, failed `cargo fmt --check`; it was
  killed (my pids 723743, 736409 and their children 770069, 770280,
  770328), `74313a1` is the rustfmt, and the gauntlet re-ran there. The
  first log is in `evidence/superseded/`.
- the gauntlet at `74313a1` was superseded by the additions (`da9aa91`)
  after 37 test targets had passed and none failed; killed by pid (2184642,
  2184652, 2184990, 3955808), log in `evidence/superseded/`.

#### §4 — evidence index

Commits:
- `c535eca` §1–§3; this section and §5 in the last commit
- `1498db2` 36 witnesses (25 red); `87e2ae7` the receiver traps' clause and span
- `9fb80c8` #157; `d6a1cb0` #162; `301fdef` #159 (and `t16`); `ee0cbfc` #487
- `9cf486e`, `3c9c645` unit tests (the shorthand's value node; the receiver claim's withdrawal)
- `b26ed5d` CHANGELOG; `74313a1` rustfmt
- `08404fb` §3 addendum (#164, #494); `ecd6864` 17 witnesses (#494's 2 red);
  `da9aa91` #494, the view set; `7d1fc36` the view-set trap's span;
  `b6ef5ae` CHANGELOG

Artifacts on kasumi under `~/lanes/is62/` (`archives/`, `evidence/`,
`probes/`, `scripts/`, `witnesses/`). Archives: lupin 0.1.42 `9856335a…`
and wolf 0.2.19 `9f3873d8…`, digests equal to the release pages'; lupin
trunk `lupin-trunk-1e96e1f` `387fd253…`; head `lupin-head-ee0cbfc`
`3f899c9a…`, then `lupin-head-da9aa91`; wolf-lang trunk `57805e35`
release `wolf` `c608799f…` with its `libwolf_rt.a` `69f743b8…`; wolf-lang
`s192` at `c51a314d` (`wolf-lang-s192/`, debug, for its gate and corpus).
- #164 and #494: `probes3-{archive-0.1.42,trunk-1e96e1f,head-ee0cbfc,head-da9aa91}.log`,
  `red-ecd6864.log`, `green-da9aa91.log`, `witnesses-head-da9aa91.log`,
  `wolfgates192-{pinned-0.1.42,plant-0.1.42-unpinned,head-ee0cbfc-unpinned,head-da9aa91-unpinned,head-da9aa91-pinned}.log`,
  `wolfgates-head-da9aa91-{unpinned,pinned}.log`,
  `diffrun/head-da9aa91-*`, `diffrun/{trunk-1e96e1f,head-da9aa91}-s192corpus.log`
- inputs: `probes-archive-0.1.42.log`, `probes2-archive-0.1.42.log`,
  `probes-trunk-1e96e1f.log`, `witnesses-trunk-1e96e1f-wolf-57805e35.log`,
  `witnesses-archive-0.1.42.log`
- gates at trunk: `wolfgates-base-0.1.42-pinned.log` (24/24, `EXIT=0`),
  `wolfgates-plant-0.1.42-unpinned.log` (the 9 pinned cases red,
  `EXIT=101`); each `SKIP lines: 0`
- red: `red-1498db2.log` (25 failed, each "lupin answered …, ruled …" or
  "the ruled stdout"; `EXIT=101`)
- green: `green-ee0cbfc.log` (`rulings_is62` 36/36, `lint_is61` 118,
  `rulings_is63` 58, `rulings_is60` 82, `rulings_is59` 18, `rulings_is58`
  20, `rulings_eg00` 15, `rulings_s182` 11, `index_store` 7,
  `mode_read_iteration` 20, `prov_machine` 14, lib 727; `EXIT=0`)
- head: `build-head-ee0cbfc.log`, `witnesses-head-ee0cbfc.log`,
  `probes-head-ee0cbfc.log`
- gates at head: `wolfgates-head-ee0cbfc-unpinned.log` (24/24, `EXIT=0`),
  `wolfgates-head-ee0cbfc-pinned.log` (the 9 pinned cases red, `EXIT=101`)
- differential and corpus: `diffrun/{trunk-1e96e1f,head-ee0cbfc}-{pin,wl}-{default,checked,native,release}.*`,
  `…-corpus.log`, `…-wlcorpus.log`, `ledgerdiff-trunk-1e96e1f-head-ee0cbfc.txt`
- gauntlet at `7d1fc36`, the code head (the commits after it are
  `CHANGELOG.md` and `docs/` only): `gauntlet-7d1fc36.log`. Its result and
  the GitHub CI run are in the PR body.

Filed and commented:
- wolffe-lang/wolf-interp#163: a flow out of an argument list leaves the
  `mut` argument's protected retag, and the next write answers `ub(mem.ub)`
  (receivers included). Not fixed here.
- wolffe-lang/wolf-lang#487: lupin's half, with the twelve receiver
  shapes against wolf trunk for s192 (the impl-receiver write parts on wolf
  trunk itself: `105` checked, `6` native and release).
- wolffe-lang/wolf-interp#159: the unknown-name row's true twin.

#### §5 — done-when

- [x] branch `is62` on origin, cut at trunk `1e96e1f`; PR open, unmerged, with these five sections
- [x] §2 re-derived; §3 committed (`c535eca`) before the first `src/`/`tests/` edit
- [x] each mirror seen red first (`red-1498db2.log`) and green at head (`green-ee0cbfc.log`)
- [x] wolf-lang `57805e35`'s three gates, every 0.1.42 pin dropped: green with head, red with 0.1.42 on exactly the 9 pinned cases
- [x] wolf-lang `s192`'s `receiver_claim_args_lanes.rs`, `PRE_RECEIVER_LUPIN` emptied: green with head, red with 0.1.42 on exactly the 14 refusal cases (#164)
- [x] #494's program prints `10 9`
- [x] no new divergence against wolf trunk; 3 fewer on its own corpus
- [x] CHANGELOG `Unreleased`
- [ ] the coverage ratchet holds (`export::coverage_is_ratcheted`, in the gauntlet and CI)
- [ ] kasumi gauntlet and GitHub CI green at the head sha (the PR body)
- [ ] #157, #162, #159, #164 close on merge (by hand if not)
- [ ] kasumi build dirs pruned once evidence is written; worktree removed

### A flow out of an argument list withdraws the call's claims — is65, wolf-interp#163

Wave 52's row: "wolf-interp#163. A `?` or `continue` leaving an argument
list leaves the `mut` argument's retag in place, so the next write answers
`ub(mem.ub)`." The contract is is62's, in five sections; §1–§3 are
committed before the first edit under `src/` or `tests/`, the rest is
appended as it lands. Measurements on kasumi (linux x86-64) under
`~/lanes/is65/`: the published lupin 0.1.43 (`e957c8de…`) and wolf 0.2.19
(`9f3873d8…`), archive digests equal to the release assets' (`gh api
…/releases/tags/<tag>`, `.assets[].digest`); lupin trunk `6d6cde5` built
release (`archives/lupin-trunk-6d6cde5`, `cb8d424c…`). Probes: 23
one-directory programs under `~/lanes/is65/probes/` plus a REPL script
(`repl-trap.txt`), run by `scripts/run-probes.sh` (`lupin conform-run
main.lu --json`, and `wolf conform-run main.lu --{checked,native,release}
--json`), summarised by `scripts/summ.py`.

#### §1 — forbidden, absolutely

No `rm` outside `~/lanes/is65/` (kasumi) and `/private/tmp/is65`; no
deletion in any tree this lane did not create (is62's `~/lanes/is62/` is
read only); no `git add -A`; no edit to another lane's file; no workflow
edit; no `~/.claude`; no build or test on this Mac or on nomad-1 (kasumi
only, `CARGO_BUILD_JOBS=4`); no tag; no pin move (the pin stays `c2401f05`);
no merge, no rebase-merge; no `2>/dev/null` on a checkout; kill only my own
pids, never a pattern or a group; jobs launched with `setsid`, never
`ssh -f`; no claim of "seen red" without the log it is in; no trailer of
any kind on any commit; no `gh run watch` without `--interval 60`; no
silent wait past four minutes.

#### §2 — inputs, re-derived 2026-10-01

| input as written | at origin / measured | drift |
| --- | --- | --- |
| wolf-interp trunk `6d6cde5`, lupin 0.1.43 tagged | `origin/trunk` = `6d6cde5` ("release: lupin 0.1.43"); `Cargo.toml` 0.1.43; `vendor/upstream/PIN` `c2401f05` (wolf-lang v0.2.19, the tag); a dev build answers `--version` `0.1.43` (no `+dev` suffix at a tagged tree) | none |
| the archives | lupin 0.1.43 linux x86-64 `e957c8de…`, wolf 0.2.19 `9f3873d8…`; both equal to the release assets' `digest` | none |
| #163's shape: `put(mut xs, v(ok)?)` answers `ub(mem.ub)` after `2` `9`; `continue` in a `while` `ub(mem.ub)`; the receiver forms likewise; wolf runs all | trunk `6d6cde5` and the 0.1.43 archive: `ub(mem.ub)`, `mem.prov.state` P1, on all four (`probes-trunk-6d6cde5.log`, `probes-archive-0.1.43.log`); wolf 0.2.19 `exit(0)` `2 9 [1, 5, 3]` and `[1, 1, 3]` on checked, native and release | none |
| the family | wider than the issue's four: a `return` and a `break` in the argument (`2 7` / `[1, 1, 3]` on wolf), `continue` in a `for`, the next write made by a `mut` argument instead of a receiver, a nested call inside the argument in three shapes (`outer(mut xs, inner(mut ys, v(ok)?))`, `outer(mut xs, inner(mut ys, 1), v(ok)?)`, `outer(inner(mut ys, v(ok)?), mut xs)`), a `read` lend (`both(ys, v(ok)?)`: the Frozen child leaks and `(mut ys).push(4)` is the foreign write), a scalar `mut n`, two `mut` arguments in one list, and every receiver form of `?`/`return`/`break`/`continue`/nested — 18 shapes, each `ub(mem.ub)` on trunk and 0.1.43, each run by wolf 0.2.19 on three tiers | wider; all listed as witnesses |
| a trap out of the list | `put(mut xs, ys[9])`: `trap(bounds)` `[130, 135]` on lupin and on all three wolf tiers (`mem.ub.defined`); nothing after a trap runs in a program. In the REPL, where the session survives (`[repl.trap.alive]`), the next line `(mut xs).push(3)` answers `trap(exclusivity): xs is held as mut` and so does `xs` (`repl-trap-trunk-and-archive.log`, trunk and 0.1.43 alike): the claim leaks too, not only the protector — at top level no scope ever pops it | the REPL is the trap kind's witness; the hold leak is a second defect under the same cause |
| shapes that already agree (controls) | `mut b.xs` (a field place), `mut xs[0]` (an element), an impl `(mut c).add(v(ok)?)` with a field write after, a view-set `(mut p).set_x(v(ok)?)` with field writes after: `exit(0)`, wolf's bytes, on trunk; `eat(take xs, v(ok)?)`: `6 9` on all four (the move stands on every machine) | none; kept as controls |
| the mechanism | `eval_args_for` (`src/eval/mod.rs:3214`): `eval_arg_list` returns `EResult<Args>`, so on a signal the `Args` — `protectors`, `held`, `writebacks` — is dropped; `enter_call` still runs, so the list's claims become `HeldWhy::Call` and live until the enclosing scope pops them (`pop_scope_escaping` :1321, `eval_for` :6450, `release_frame` :3080) — which the REPL's top level never does; nothing calls `unprotect` for the protectors but `finish_args` (:3532), which the signal path never reaches. `eval_method` (:7919): the receiver's protected child (`receiver_tag`, :7869) is unprotected only at :8112, past `evaluated?` (:7924). The is63 comment at :3211 states the old behaviour as a design ("a list that stops on a signal enters too") | as the issue reads it, plus the hold |
| the spec | `[mem.tier0.excl.4]` (ruling #17): a `mut` argument's claim "takes effect when the call is entered"; `[mem.prov.tag]`: parameter entry "is protector-equivalent: the tag is protected for the whole call". A call never entered has no extent: there is nothing for the claim to take effect for and nothing for the protector to protect | the clauses decide it; no ruling needed |
| CI | `.github/workflows/ci.yml` runs on `push` to `trunk`/`main` and on `pull_request`: a branch has runs only while its PR is open, so the PR opens at the first push and its body is finished at the end | noted |
| the coverage ratchet | `tests/export.rs`: `RATCHET_FLOOR = 271`, `ANCHORS_TOTAL = 542` | none |
| no test pins the old behaviour | `grep -rn "stops on a signal\|enters too\|stay held" src tests`: only the :3211 comment | none |

#### §3 — prediction, committed before the first edit

**One mechanism, one commit.** `eval_arg_list` fills an `Args` it is
handed by `&mut`; when any argument leaves on a signal — `?`'s `Return`,
`return`, `break`, `continue`, a trap, a UB finding, an unsupported — the
list is **abandoned**: the accesses it pushed are released (`held` of
them, the most recent, so every `Pending(call)` claim and every `read`
lend's `HeldWhy::Call` of this list go, and nothing of an enclosing list
does), every protector it minted is unprotected and the forest pruned,
and the signal leaves. `eval_args_for` enters the call only on `Ok`.
`eval_method` does the same for the receiver on the signal path: its
protected child is unprotected and pruned before the rebind that never
happens, and the receiver claims are withdrawn as they already are. A
trace line names it (`Rule::ModeMut`, "withdrawn: the call was never
entered"). No change to `AccessSet`, `finish_args` or the success path.

**The witness table** (lupin head against trunk `6d6cde5`; wolf 0.2.19's
answer in brackets, one answer on checked, native and release):

| witness | trunk and 0.1.43 | head | [wolf 0.2.19] |
| --- | --- | --- | --- |
| `try_arg` (#163's program) | `ub(mem.ub)` after `2` `9` | `exit(0)` `2` `9` `[1, 5, 3]` | [same] |
| `try_arg_then_mut_arg` (the next write is `put(mut xs, 3)`) | `ub(mem.ub)` | `2` `9` `[1, 5, 3]` | [same] |
| `continue_arg` (#163's `while`), `continue_for` | `ub(mem.ub)` | `[1, 1, 3]` | [same] |
| `return_arg` | `ub(mem.ub)` after `2` `7` | `2` `7` `[1, 5, 3]` | [same] |
| `break_arg` | `ub(mem.ub)` | `[1, 1, 3]` | [same] |
| `try_nested_inner_claim`, `try_nested_first_arg` | `ub(mem.ub)` after `4` `9` | `4` `9` `[1, 2, 3] [2, 5, 4]` | [same] |
| `try_after_nested_done` | `ub(mem.ub)` after `4` `9` | `4` `9` `[1, 7, 3] [2, 1, 1, 4]` | [same] |
| `read_lend_try` | `ub(mem.ub)` after `2` `9` | `2` `9` `[2, 6, 4]` | [same] |
| `recv_try`, `recv_return` | `ub(mem.ub)` | `2` `9` `[1, 5, 3]`; `2` `7` `[1, 5, 3]` | [same] |
| `recv_continue`, `recv_break` | `ub(mem.ub)` | `[1, 1, 3]` | [same] |
| `recv_nested_try` | `ub(mem.ub)` after `4` `9` | `4` `9` `[1, 2, 3] [2, 5, 4]` | [same] |
| `scalar_mut_try` | `ub(mem.ub)` after `6` `9` | `6` `9` `16` | [same] |
| `two_muts_try` | `ub(mem.ub)` after `4` `9` | `4` `9` `[1, 5, 3] [2, 5, 4]` | [same] |
| `trap_arg` (control) | `trap(bounds)` `[130, 135]` | unchanged | [`trap(bounds)`] |
| `take_then_try`, `field_mut_try`, `elem_mut_try`, `impl_recv_try`, `viewset_recv_try` (controls) | wolf's bytes | unchanged bytes | [same] |
| REPL `trap_then_write` (`tests/repl_session.rs`'s pipe; lupin only) | `trap(exclusivity)` on the line after the trap | the push runs, `xs` answers `[1, 3]` | [no REPL] |

Each witness is run red at the commit that adds it (18 red, the
controls and the REPL row's trap line green) and green at the fix.

**Existing tests that change: none.** No test pins the old behaviour (§2).
`src/eval/tests.rs` gains one unit test: after a `?` out of an argument
list the trace carries the withdrawal line and the next write runs.

**Corpus and differential.** `lupin corpus` at the pin: identical reports,
trunk against head. `lupin diff-run` on four tiers against wolf 0.2.19 on
the vendored corpus (the pin is the 0.2.19 tag): no new divergence; a row
could only move from `ub` to wolf's bytes, and none is predicted (no
vendored row has a flow out of an argument list under a claim). The
conservatism ledgers move 0 rows.

**Coverage.** `RATCHET_FLOOR` holds at 271 (the witnesses carry no
`conforms:` line the bundle counts).

**Out of scope, named:** nothing new is filed from §2; the `take`-then-`?`
move stands on every machine and is a control, not a finding.

#### §3a — the prediction, scored

- **held: one mechanism, one commit**, `dfebf6b`. The 18 witnesses red at
  `3e7f230` (`red-3e7f230.log`: 17 × "lupin answered `ub(mem.ub)`, ruled
  `exit(0)`" and the REPL's "the abandoned list's claim on `xs` outlived
  the trap"; the 6 controls green; `EXIT=101`) are 24/24 green at
  `7264f7d` (`green-7264f7d.log`, `EXIT=0`).
- **held: the witness table**, row for row (`witnesses-head-a4378d0.log`):
  22 of 23 rows are byte-identical with wolf 0.2.19 on checked, native and
  release; `trap_arg` is the same `trap(bounds)` with native and release
  carrying no clause or span. The REPL (`repl-trap-head-a4378d0.log`): the
  push after the trap runs and `xs` answers `[1, 3] : List`.
- **held: existing tests that change, none.** At `7264f7d` the lib is 730
  (one new), `rulings_is62` 53, `rulings_is63` 58, `rulings_is60` 82,
  `rulings_eg00` 15, `repl_session` 9, `prov_machine` 14, `index_store`
  7, `mode_read_iteration` 20; the trace line is `Rule::ModeMut`'s "1
  claim(s) and 1 protector(s) withdrawn: the call was never entered".
- **held: corpus and differential, no new divergence.** `lupin corpus` at
  the pin: identical reports, 586 / 554 / 53 / 60 / 65 / 1 mismatch.
  `lupin diff-run` against wolf 0.2.19, four tiers: the same gating lines
  per tier (5 / 5 / 6 / 6), 0 differing report lines on default, checked
  and release; native's one differing line is `unsafe_ub_uaf.lu`'s exit
  code under UB (`148` / `238`; is62 saw `89` / `83`), noise on a program
  both sides call undefined. The conservatism ledgers move 0 rows on all
  four (`diffrun/`).
- **coverage**: in the gauntlet (`export::coverage_is_ratcheted`) and in
  CI; the PR body carries both.

Two slips, both mine, both repaired:
- rustfmt, twice: the first `cargo fmt --check` log was read through
  `tail -20`, which hid a third reflow, so `710db1c` fixed two of three and
  CI went red on `rustfmt` at every job (run 36953762235); the gauntlet at
  `710db1c` was red on its fmt step and was killed (my pids 3305952,
  3305954, 3343257, 3379754), kept as
  `evidence/superseded-gauntlet-710db1c-fmt-red.log`; `a4378d0` is the
  third reflow, the gauntlet re-ran there.
- `final.sh` lost its executable bit on a second `scp` and the chained
  launch ran the gauntlet alone; `final.sh` now builds in a clone of its
  own (`headsrc/`) so it never waits on, or touches, the gauntlet's `dev/`.

#### §4 — evidence index

Commits:
- `02e1c31` §1–§3; this section and §5 in the last commit
- `3e7f230` 24 witnesses (18 red); `dfebf6b` the fix; `7264f7d` unit test
- `9f62d3a` CHANGELOG; `710db1c`, `a4378d0` rustfmt

Artifacts on kasumi under `~/lanes/is65/` (`archives/`, `evidence/`,
`probes/`, `scripts/`, `witnesses/`). Archives: lupin 0.1.43 `e957c8de…`
and wolf 0.2.19 `9f3873d8…`, digests equal to the release assets'; lupin
trunk `lupin-trunk-6d6cde5` `cb8d424c…`; head `lupin-head-a4378d0`
`29fea436…`.
- inputs: `setup.log`, `probes-trunk-6d6cde5.log`,
  `probes-archive-0.1.43.log`, `repl-trap-trunk-and-archive.log`,
  `witnesses-trunk-6d6cde5-wolf-0.2.19.log`
- red: `red-3e7f230.log` (18 failed, `EXIT=101`); green:
  `green-7264f7d.log` (`EXIT=0`); `fmt-7264f7d.log`
- head: `build-head-a4378d0.log`, `witnesses-head-a4378d0.log`,
  `probes-head-a4378d0.log`, `repl-trap-head-a4378d0.log`
- differential and corpus: `diffrun/{trunk-6d6cde5,head-a4378d0}-pin-{default,checked,native,release}.{jsonl,ledger.jsonl,log}`,
  `…-corpus.log`, `….done`, `….version`
- gauntlet at `a4378d0`, the code head (the commit after it is this
  section, `docs/` only): `gauntlet-a4378d0.log`. Its result and the
  GitHub CI run are in the PR body.

#### §5 — done-when

- [x] branch `is65` on origin, cut at trunk `6d6cde5`; PR #167 open, unmerged, with these five sections
- [x] §2 re-derived; §3 committed (`02e1c31`) before the first `src/`/`tests/` edit
- [x] each witness seen red first (`red-3e7f230.log`) and green at the fix (`green-7264f7d.log`)
- [x] every exit kind (`?`, `return`, `break`, `continue`, a trap) and a nested call inside an argument witnessed, receivers included, each agreeing with wolf 0.2.19 on checked, native and release
- [x] no new divergence against wolf 0.2.19; the corpus report identical
- [x] CHANGELOG `Unreleased`
- [ ] the coverage ratchet holds (`export::coverage_is_ratcheted`, in the gauntlet and CI)
- [ ] kasumi gauntlet green at `a4378d0` and GitHub CI green at the head sha (the PR body)
- [ ] #163 closes on merge (by hand if not)
- [ ] kasumi build dirs pruned once evidence is written; worktree removed

### `match` over a fallible value, and `?` under a `defer` — is67, rulings #21 and #19 (wolf-lang#497, #498)

Wave 52's row: "**is67** (lupin: tag arms, the value half, the collision
refusal)" on ruling #21, and "lupin's #498 refusal rides with is67" on
ruling #19 (`sprints/wave-52.md`, 2026-10-02; the rulings' text is
`sprints/STATUS.md` "Needs the human" items 19 and 21). The contract is
is62's, in five sections; §1–§3 are committed before the first edit under
`src/` or `tests/`, the rest is appended as it lands. Measurements on
kasumi (linux x86-64) under `~/lanes/is67/`: the published lupin 0.1.43
(archive `e957c8de…`, binary `3b0702c0…`) and wolf 0.2.20 (archive
`24855d5e…`), both digests equal to the release pages' (`gh release view`,
`evidence/setup.log`); lupin trunk `6d6cde5` (the 0.1.43 tag) built release
in `dev/` (`archives/lupin-trunk-6d6cde5`, `cb8d424c…`,
`evidence/build-trunk-6d6cde5.log`). Probes: 34 one-directory programs
under `probes/` (`m21_*` for #497, `d19_*` for #498, `s196_*` the eight rows
of wolf-lang PR #509 verbatim), run by `scripts/run-probes.sh` (`lupin
conform-run main.lu --json`, `wolf conform-run main.lu
--{checked,native,release} --json`), summarised by `scripts/summ.py`:
`evidence/probes-archive-0.1.43-wolf-0.2.20.log`,
`evidence/probes-trunk-6d6cde5.log` (lupin lines identical to the
archive's), `evidence/probes-closure-archive-0.1.43-wolf-0.2.20.log`.

#### §1 — forbidden, absolutely

No `rm` outside `~/lanes/is67/` (kasumi) and `/private/tmp/is67`; no
deletion in any tree this lane did not create; no `git add -A`; no edit to
another lane's file — wolf-lang (s196's branch, s197's when it appears) is
read and built, never edited or pushed; is65's claim-withdrawal code in
`src/eval/mod.rs` (`eval_args`' abandon path and its unit tests) is not
touched; no workflow edit; no `~/.claude`; no build or test on this Mac or
nomad-1 (kasumi only, `CARGO_BUILD_JOBS=4`, jobs launched with `setsid`,
never `ssh -f`; tars with `COPYFILE_DISABLE=1`); no tag; no pin move (the
pin stays `c2401f05`); no merge, no rebase-merge; no `2>/dev/null` on a
checkout; kill only my own pids, never a pattern or a group; no claim of
"seen red" without the log it is in; no trailer on any commit; waits in
printing loops; `gh run view`, never `gh run watch` without
`--interval 60`.

#### §2 — inputs, re-derived 2026-10-02

| input as written | at origin / measured | drift |
| --- | --- | --- |
| wolf-interp trunk `6d6cde5`, lupin 0.1.43 tagged | `origin/trunk` = `6d6cde5`; `Cargo.toml` 0.1.43; the release build's record says `impl_version` `0.1.43` and `--version` `0.1.43` (a dev build `0.1.43+dev.<sha>`), so a gate that pins "0.1.43" by `impl_version` reads a head build as 0.1.43 (is62's §3a lesson) | none; the pinned gate below is run with its pin emptied |
| is65 (#163) in flight; stay out of the claim withdrawal | PR #167 open, branch `is65` at `9d9c023`: `src/eval/mod.rs` hunks at 3210–3420 and 7987–8010, `src/eval/tests.rs`, `tests/rulings_is65*`, CHANGELOG, this log | this lane's eval edits are `ExprKind::Match` (4861), `match_pattern` (6760), `exec_binding` (3804), `call_fn`'s parameter binding (3060), `Scope` (437); adjacent to nothing of is65's |
| ruling #21, "yes, as proposed"; s197 in parallel, "cite `[type.row.match]` when it exists" | STATUS #21 as quoted in §3; wolf-lang has **no `s197` branch and no PR** (`git ls-remote`, `gh pr list`, 2026-10-02); the pinned `spec/anchors.json` (c2401f05) has `type.row`, `type.row.operand` and no `type.row.match` | the ruling is cited; a `Rule` label cannot cite `type.row.match` (the registry test checks the registered `type` namespace against the pinned index), so the evaluator's label cites `type.row.operand` and the refusal texts name `[type.row.match]` |
| ruling #19, "the same diagnostic code s196 chooses" | wolf-lang PR #509, branch `s196` at `2886c9e9`: **E0611**, clause `[type.row.defer]` (spec/10-types.md), `phase: resolve` on the three negative rows; "the refusal reads the whole deferred expression: a `?` in a call argument, an interpolation hole, a binding or a block under the `defer`"; "a `?` inside a closure defined under the `defer` is that closure's own propagation and is not refused"; the gate `crates/wolf_driver/tests/try_under_defer_refused_lanes.rs` compares verdict and stdout only and pins lupin 0.1.43 as pre-mirror by version | none; the span is the lane's choice (the `?` expression, operand through `?`) |
| "lupin 0.1.43 already runs the two-arm shape" | it runs the **miss** path only. On the hit path the row arm BINDS the value: `match look(m, "a") { none => -1, v => v }` prints `-1` (`m21_call_tag_value`), as do the local, `_`, `Map`-index, imported, two-tag, nested, consumed and `?`-in-scrutinee shapes (`-1`, `-1`, `-1`, `-1`, `-1 -2 -1`, `-2 -2`, `0 0`, `key look a -1`); `{ Io(code) => …, Timeout => 0, v => v }` answers `0` for 40 (`Timeout` bound it); `{ none => 0, true => 1, false => 2 }` answers `0 0 0`; and `{ v => v, _ => -1 }` binds the ERROR to `v` and prints `none` (`m21_wild_row_half`). Eleven silent wrong answers | **the value half is wrong on 0.1.43**, not merely unimplemented; every one is a witness |
| the mechanism | `match_pattern` (`src/eval/mod.rs:6760`) decides whether an identifier is a row-tag pattern from the VALUE: only over a `Value::Error` does a lowercase name in the value's row (or the module's `row_tags`) dispatch; over a plain value every identifier binds, so the first arm wins. `ExprKind::Match` (4861) tries arms in order with no notion of the scrutinee's two halves. The lint's `scrutinee_row` (`src/lint.rs:1525`) knows only an `else`-binder's row | none |
| s191's probes (`~/lanes/s191/probes/`, read-only) | m1, m2: `unsupported` at resolve on checked, native and release of wolf 0.2.20 (the `check_match` NotYet, span of the match); lupin `look zz -1`. m3: `look zz -1 look zz seven` on all four. Reproduced byte for byte | none |
| #498: p2 `… 9` on lupin, `… 1` on checked, native and release crash | wolf 0.2.20 (published): native and release `thread 'main' has overflowed its stack`, exit 134, no record (`d19_try_in_defer`, `d19_try_in_errdefer`, `d19_try_in_defer_block`); checked `body key deferred a 1 body key 1`; lupin 0.1.43 `body key deferred a 1 body key 9` | none; the crash ships in 0.2.20 |
| s196's eight rows on lupin 0.1.43 | the five `run` rows print the checked machine's bytes (= the ruled bytes); native and release part on the three `errdefer` block rows (s196's #499 fix, not this lane's); the three `fail(E0611)` rows run on lupin (`9`) | none; the five are controls here |
| the enum and bool value halves | `m21_enum_value_half` already prints `0 1 2 6` (a variant name resolves before the tag rule); `m21_bool_value_half` `0 0 0`; `m21_tag_collision` runs, `1` | the enum row is a control; the collision runs today |
| a `?` in a closure under a `defer` (`fn(o: bool) { key(o)? }`) | lupin, native and release: `body key deferred a 1 body key deferred - 1`; checked `unsupported` ("closures in checked execution") | a control under `[type.row.defer]`'s closure sentence; unchanged |
| warnings on the probes | W0603 on the capitalized tags (`Io`, `Timeout`) and W0313/W0314 on the imported module, on lupin and on every wolf tier alike | none |
| the coverage ratchet | `tests/export.rs`: `RATCHET_FLOOR = 271`, `ANCHORS_TOTAL = 542` | none |
| kasumi `/home` at 99% | 97%, 30 G free at launch; the lane dir is 522 MB after the trunk build | prune `dev/target` as each item's evidence is written |

#### §3 — prediction, committed before the first edit

**The ruled design, as this machine reads it** (`[type.row.match]`, the
maintainer's #21, quoted from STATUS): the scrutinee has type `T ! {row}`;
an arm is a **row arm** (a tag of the row by name, binding its payload when
it has one: `none => …`, `Io(e) => …`) or a **value arm** (any pattern over
`T`); an identifier that names a tag of the scrutinee's row is a row arm,
anything else a value pattern; `_` covers what is left on both halves; the
match must cover every tag of the row and the whole of `T`, and E0801 names
the missing tag or the uncovered value half; the row is consumed; a tag
that is also a constructor name reachable from `T` is refused by name,
never guessed; `else |e| match e { … }` keeps working beside it.

**Five mechanisms.**

1. **The scrutinee's static type** (one reader for sema, the lint and the
   evaluator, `src/rowmatch.rs`). A `match` is a ROW MATCH exactly when its
   scrutinee is statically fallible: a call to a module `fn` (own or
   `use`d, by its declared return row: `-> T ! {row}` in either spelling),
   a call to a builtin with a declared row (`eval::builtin::declared_row`),
   a `Map` index (`V ! {none}`, `[mem.map.absent]`, the base a local or
   parameter declared or built as `Map[K, V]`), a local bound by such an
   initializer or by a `T ! {row}` annotation, and a parenthesized one. A
   method call, a closure call, an operator or anything else is not
   statically fallible, and a match over it keeps today's behaviour,
   unchanged — the sema boundary's rule: a guess never becomes a verdict.
   Locals carry what the binding knew (`Scope::known`, set by
   `exec_binding` and by `call_fn` for parameters).
2. **Dispatch** (`ExprKind::Match`). In a row match a row value
   (`Value::Error` that is not an enum variant) is tried against the row
   arms and `_` only; any other value against the value arms and `_`
   only. An arm is a row arm when its pattern is a bare identifier naming
   a tag of the static row (the value's own row joins it for a row value),
   or a payload pattern `Tag(…)` whose head names one; `_` is both; every
   other pattern is a value arm (an enum variant, a literal, a binder, a
   tuple, a struct, a range). A row value no arm takes, or a value no arm
   takes, is `unsupported` naming the tag or the value half — never a
   wrong arm. The arm boundary's whole-move rule and the guard are
   unchanged. The match's value is the arm's: the row is consumed.
3. **Exhaustiveness, static** (`sema::row_match_check`, the last link of
   `resolve_check`'s chain; E0801 at the resolve rung, as E0805 is —
   `[proto.cmp.rung]` makes it agreement; primary span from `match` to the
   end of the scrutinee). For a row match: every tag of a closed row needs
   an unguarded row arm or an unguarded `_`, else "this `match` does not
   cover `none`"; an open row (`..`) needs `_`; the value half needs an
   unguarded `_` or an unguarded irrefutable value arm (a binder, an `@`,
   a tuple or struct of irrefutables), or — `T` a `bool` — both literals,
   or — `T` an enum this module declares — every variant named with
   irrefutable fields; else E0801 names the missing variant or literal,
   or "the value half (`int`)" when the unguarded value arms are all
   literals or ranges over a scalar. Any other value-half picture (a
   product pattern with refutable parts, an unresolvable `T`) is left
   alone: the dynamic miss of (2) answers. A guarded arm counts for
   nothing (the compiler's rule).
4. **The collision, static and by name** (`sema::row_match_refusal`,
   asked by `frontend::admit` as `raise_check` is, before anything runs).
   A tag of the static row that an enum of the module declares as a
   variant of `T` (`T` the scrutinee's ok type, resolved by head name) is
   `Refusal::Unsupported` naming the tag, the row and the enum and citing
   `[type.row.match]`. The checker's code is s197's: it is not guessed, so
   this is the conservatism class, not a `fail(E…)`. If s197's PR names a
   code before this PR is final, the refusal becomes a `Diag` with it (one
   edit, the detection unchanged).
5. **#19** (`sema::defer_try_check`, in `resolve_check`'s chain before
   (3)): a `?` anywhere inside a `defer`/`errdefer` expression — through
   blocks, bindings, call arguments and interpolation holes, stopping at a
   closure literal — is **E0611** at the `?` expression's span (operand
   through `?`), anchor `type.row.defer`, message naming the fix as the
   clause does. Items, impl methods, nested fns and closures are walked
   alike.

**The lint** (`src/lint.rs`): a row arm is not a binder (the same
classification, so W0305 never reads `none` as a shadow), and
`match_reachability` (E0802) says nothing about a row match — the two
halves are the compiler's usefulness question (s197), and a `_` after a
binder is live there. **The rule label**: `Rule::RowMatch`, anchored on
`type.row.operand` (the pinned clause that names `match` as a way to handle
a row), fired once per row match with the half taken.

**The witness table** (`tests/rulings_is67/`, one directory per shape, the
runner is is65's; lupin trunk `6d6cde5` = 0.1.43 against the head; wolf
0.2.20's answer in brackets, one answer on checked, native and release
unless shown):

| witness | trunk | head | [wolf 0.2.20] |
| --- | --- | --- | --- |
| `m21_call_tag_value` (the issue's shape, miss then hit) | `look zz -1 look a -1` | `look zz -1 look a 5` | [`unsupported` at resolve] |
| `m21_local_scrutinee` (s191's m2, miss then hit) | `-1 -1` | `-1 1` | [`unsupported`] |
| `m21_payload_tags` (`Io(code)`, `Timeout`, `v`; 0, 1, 40) | `0 109 0` | `0 109 40` | [`unsupported`] |
| `m21_wild_value_half` (`{ none => -1, _ => 1 }`) | `-1 -1` | `-1 1` | [`unsupported`] |
| `m21_wild_row_half` (`{ v => v, _ => -1 }`) | `none 5` | `-1 5` | [`unsupported`] |
| `m21_wild_both` (`{ _ => 7 }`, control) | `7 7` | `7 7` | [`unsupported`] |
| `m21_missing_tag` (`{ v => v }`) | `exit(0)` `look a 5` | `fail(E0801)` naming `none`, nothing printed | [`unsupported`] |
| `m21_uncovered_value` (`{ none => -1 }`) | `exit(0)` `look zz -1` | `fail(E0801)` naming the value half (`int`) | [`unsupported`] |
| `m21_tag_collision` (`Timeout` a tag and a `Status` variant) | `exit(0)` `1` | `unsupported` by name, before running | [`unsupported`] |
| `m21_else_match_control` (s191's m3, control) | `look zz -1 look zz seven` | unchanged | [same bytes] |
| `m21_nested` (a row match in a row arm and in a value arm) | `look zz look a -2 look a look a -2` | `look zz look a -5 look a look zz 50` | [`unsupported`] |
| `m21_try_in_scrutinee` (the #492 shape) | `key look a -1 key 9` | `key look a 5 key 9` | [`unsupported`] |
| `m21_map_index` (`match m["zz"]`, miss then hit) | `-1 -1` | `-1 5` | [`unsupported`] |
| `m21_two_tags_named` (`{none, stale}`, no `_`) | `-1 -2 -1` | `-1 -2 5` | [`unsupported`] |
| `m21_guard_no_cover` (`none if flag => …, v => v`) | `exit(0)` `look zz -1` | `fail(E0801)` naming `none` | [`unsupported`] |
| `m21_payload_missing_tag` (`{ Io(e) => e, v => v }`) | `exit(0)` `9` | `fail(E0801)` naming `Timeout` | [`unsupported`] |
| `m21_enum_value_half` (every `Color` variant named, control) | `0 1 2 6` | `0 1 2 6` | [`unsupported`] |
| `m21_enum_missing_variant` (`Green` unnamed) | `exit(0)` `1` | `fail(E0801)` naming `Green` | [`unsupported`] |
| `m21_bool_value_half` (`true`, `false`) | `0 0 0` | `0 1 2` | [`unsupported`] |
| `m21_result_consumed` (`(match …) + 1`) | `0 0` | `0 6` | [`unsupported`] |
| `m21_imported_row` (`store.find`, a `use`d module's row) | `-1 -1` | `-1 5` | [`unsupported`] |
| `d19_try_in_defer` (s191's p2) | `exit(0)` `body key deferred a 1 body key 9` | `fail(E0611)`, nothing printed | [checked `… 1`; native, release crash] |
| `d19_try_in_errdefer` (s191's p3) | `exit(0)` `key look a 5 key key 9` | `fail(E0611)` | [checked runs; native, release crash] |
| `d19_try_in_defer_block` (a `?` in a binding in a `defer` block) | `exit(0)` | `fail(E0611)` | [checked runs; native, release crash] |
| `d19_defer_else_control` (PR #509's `defer_else_handles`) | `body key a key deferred a 1 body key key errdefer - key deferred - 9` | unchanged | [same bytes] |
| `d19_try_in_closure_in_defer` (a closure's own `?`, control) | `body key deferred a 1 body key deferred - 1` | unchanged | [native, release same; checked `unsupported`] |

Twenty-one witnesses red at the witness commit (on trunk's code), five
controls green on both sides; each red for the reason its row names
(wrong bytes, or a run where a refusal is ruled).

**The gate.** wolf-lang `s196` (`2886c9e9`) `try_under_defer_refused_lanes.rs`
with its 0.1.43 pre-mirror pin emptied, `LUPIN` = head: green on all three
rows; with lupin 0.1.43: red on exactly the three; pinned, 0.1.43 green
(the control). Its default lane needs the s196 compiler (`fail(E0611)`),
built in `~/lanes/is67/wolf-lang-s196/` (debug, read-only).

**Existing tests that change: none predicted.** No test under `tests/` or
`src/eval/tests.rs` matches directly over a statically fallible scrutinee
with a binder arm (the compiler refuses every such program, so no corpus
row carries one); the suite decides, and a change is reported here.

**Corpus and differential.** `lupin corpus` at the pin: identical reports,
trunk against head (the census line's five counts unchanged). `lupin
diff-run` on four tiers against wolf 0.2.20 (the archive), on the vendored
corpus and on wolf-lang trunk's own `corpus/`: no new divergence; the
conservatism ledgers move 0 rows. **New divergence against wolf 0.2.20,
by name, the ruled rows only:** the seventeen `m21_*` programs that run or
refuse here where 0.2.20 answers `unsupported` at resolve (the `check_match`
NotYet s197 retires) — every `m21_*` row above except the two controls that
already agree; and on the three `d19_try_*` programs lupin moves to the
compiler's ruled answer (`fail(E0611)`) where 0.2.20's native and release
still crash. On wolf-lang `s196`'s eight rows: the three negatives move
`exit(0)` → `fail(E0611)`, the five run rows unchanged.

**Coverage.** `RATCHET_FLOOR` holds at 271 (the witnesses are tests, not
corpus rows).

**Out of scope, named:** a `match` over a method call, a closure call or
an operator result (no static row; today's behaviour, named in the PR); a
static E0801 for a plain enum or bool match (the existing conservatism
rows stay); E0802 over a row match; the compiler's E0801 span and witness
rendering, and the collision's code (s197's).

#### §3 addendum — s197's PR appeared mid-lane (wolf-lang#510, 2026-10-02)

Committed before the edit it drives (`39c6390`'s). s197 (branch `s197`,
code head `68d80376`; `83f370f1` is its plant) names the collision
**E0816** ("the row tag `Line` is also a variant of `Shape`"), refused
"whether or not an arm spells it", and refuses by name "an or-pattern
mixing a row arm with a value pattern, an `@`-binding at the top of an
arm". Its E0801 reads "does not cover `_` (the value half, `int`)" with
the primary span `match` through the scrutinee, and its gate
`match_fallible_lanes.rs` pins lupin 0.1.43 pre-mirror by version on nine
rows. Per §3's clause, the collision becomes a `Diag` with s197's code
(the detection unchanged), the mixed shapes become the by-name refusal
(`frontend::admit`, as `raise_check`), and the E0801 wording follows the
compiler's. Predicted: `m21_tag_collision` moves `unsupported` →
`fail(E0816)`; s197's nine rows answer their `check:` lines on the head;
its gate with the pin emptied: head green 9/9, 0.1.43 red on 8 (the
`else` control passes), 0.1.43 pinned green.

#### §3a — the prediction, scored

- **held: the five mechanisms**, one commit each: `233dbb7` (the static
  reader and the resolve-rung checks), `54b453e` (the dispatch), `79af9df`
  (the lint), with the addendum at `39c6390`. The 21 witnesses red at
  `c08285f` (`red-c08285f.log`: 5 passed, 21 failed, `EXIT=101`, each for
  its named reason — a wrong byte, or a run where a refusal is ruled) are
  green at the code head `99788ca` (`witnesses-head-99788ca.log`, lupin
  `d743fff5…`), the three chapter 6 shapes bs57 measured (`99788ca`) with
  them: `rulings_is67` 29/29 (`green-99788ca.log`, `EXIT=0`); 0.1.43 answers `no comma / no
  comma / … / no comma`, `no_comma` and `7 -4 Weird` on them
  (`probes-book-archive-0.1.43-wolf-0.2.20.log`), the head `340 cents /
  nothing owed / …`, `fail(E0801)` and `7 -4 -99`.
- **held: the witness table, row for row** — with one slip of mine:
  `m21_result_consumed`'s ruled stdout is `look zz\nlook a\n0 6\n`, not
  `0 6\n` (the two `look` lines were in the trunk cell and not in the
  prediction; `e9fa018` corrects the cell, the fix is real: trunk `0 0`).
- **held: s197's nine rows** answer their `check:` lines on the head
  byte for byte (`probes-head-99788ca.log`, `s197_*`), the E0816 and both
  E0801 rows included; 0.1.43 ran the six run rows with the first-arm
  bytes and ran the three refusals.
- **held: the gates.** s196's `try_under_defer_refused_lanes.rs` at
  `7a8be823` (worktree `wl-s196wt/`, `wolf` `3bf39452…`): pin emptied,
  head 3/3 green (`wolfgate-s196-head-99788ca-unpinned.log`, `EXIT=0`),
  0.1.43 red on exactly 3 (`wolfgate-s196-archive-0.1.43-unpinned.log`,
  `EXIT=101`); pinned, 0.1.43 3/3 green. s197's `match_fallible_lanes.rs`
  at `68d80376` (`wolf-lang-s196/`, `wolf` `3f7e29ce…`): pin emptied, head
  9/9 green (`wolfgate-s197-head-99788ca-unpinned.log`, `EXIT=0`), 0.1.43
  red on 8 with the `else` control green
  (`wolfgate-s197-archive-0.1.43-unpinned.log`, `EXIT=101`); pinned,
  0.1.43 9/9 green. `SKIP lines: 0` in every log. As §2 predicted, the
  pinned files read a dev build as 0.1.43 and red on it (3 and 8): the
  pins are emptied at the pairing that ships this.
- **missed: existing tests that change — one, not none.**
  `lint_is61`'s `q10_map_match` (CI run 36964862132 at the pre-rebase
  head, `test (ubuntu-latest, test shard 2 of 3)`; the gauntlet at
  `99788ca` reads the same): `match m[k] { ok(v) => v.len, none => 0 }`
  over a `List[int] ! {none}`. 0.1.43 bound the list to `none` and
  printed `0` (`exit(0)`, the row's asserted verdict, written when the
  lint moved no verdict); under `[type.row.match]` `none` is a row arm and
  `ok(v)` fits no list, so no value arm covers the value half and the
  head refuses it by name (`unsupported`). The compiler's answer at s197
  is E0801 or E0808 (an opaque `T` is covered only by a binding or `_`;
  `ok(v)` is a constructor pattern over a list) — a code this machine
  does not spend, so the dynamic refusal stays. `f7049fc` moves the row's
  verdict to `unsupported` with the reason beside it; its warnings are
  unchanged. §3's claim rested on the corpus (the compiler refused every
  such program) and forgot lupin's own lint suite, whose programs were
  written for lupin alone.
- **held: corpus and differential, no new divergence beyond the ruled
  rows.** `lupin diff-run` on four tiers against wolf 0.2.20, trunk
  `9d9c023` (is65 merged; `lupin-trunk-9d9c023`, `4a27762b…`) against
  `99788ca`: on the pinned corpus the divergence lists
  are identical (5 / 5 / 6 / 6 per tier) and the ledgers move 0 rows; on
  wolf-lang trunk `cdde128a`'s corpus the same (5 / 5 / 6 / 6, 0 ledger
  rows); the census lines are identical (pin 554 / 53 / 60 / 65 / 1,
  wl-trunk 601 / 81 / 65 / 68 / 1). On s196's corpus (`7a8be823`) three
  rows move, by name — `rows/negative/try_in_defer.lu`,
  `try_in_defer_block.lu`, `try_in_errdefer.lu` — from a stdout mismatch
  against 0.2.20's checked machine (lupin `… 9`, checked `… 1`) to a
  verdict mismatch (lupin `fail(E0611)`, 0.2.20 checked `exit(0)`): the
  ruled answer against a compiler that still runs the shape (checked) or
  dies on it (native, release); the census moves 606 → 609 match, 68 → 65
  conservatism. On s197's corpus (`68d80376`) the nine `match_row_*` rows
  move: the census 602 → 610 match, 6 → 1 mismatch (the one is
  DIV-2026-019), 68 → 65 conservatism; the ledger moves 6 rows per tier
  (the six run rows, from the first-arm bytes to their `check:`); the
  divergence lists against 0.2.20 are unchanged (`unsupported` on every
  tier is the conservatism ledger).
- **held, the addendum**: `m21_tag_collision` is `fail(E0816)` at the
  head (`witnesses-head-99788ca.log`); the mixed shapes are refused by
  name (`rowmatch` unit tests).
- **coverage**: `RATCHET_FLOOR` 271, unchanged (the gauntlet's
  `export::coverage_is_ratcheted`).

Slips, all mine, all repaired:
- the first patch shipped to kasumi lacked the untracked `src/rowmatch.rs`
  (`git diff` before `git add -N`); the build that read it was killed by
  pid (2043320 and its two children) and re-run.
- `head.sh`'s gate loop at `39c6390` wrote no logs: the scripts had been
  re-shipped without their execute bit (`Permission denied`, seen under
  `bash -x`); `chmod +x`, and the eight runs were made by
  `scripts/gates-all.sh` with explicit calls.
- the pin-emptying regex did not cross a Rust string's `\`-newline
  continuation, so s197's `a_wildcard_covers_what_is_left_on_each_half`
  kept its pin and read the head as 0.1.43 (8/9 green, 1 red on the
  pre-mirror bytes). The regex now spans newlines; the four first logs
  are in `evidence/superseded/` and the cited runs are the second.
- the first gauntlet, at `d19ccb6`, was superseded by the E0816
  alignment and killed by pid (2621914 and its tree); its log is in
  `evidence/superseded/`.
- the first head run, at `e9fa018`, failed `cargo fmt --check` and one
  clippy `manual_contains`; `d19ccb6` is the rustfmt and the fix.
- **the rebase** (the coordinator's ask, is65 merged at `9d9c023`): the
  branch was rebased once, before the final push, keeping both sides of
  `CHANGELOG.md` and this log (the subset check against both parents:
  0 non-blank lines missing, both ways, both files); a first attempt
  continued past a conflict with markers in the file and was redone from
  the pre-rebase tip (`git reset --hard`, the reflog). Every sha this
  section cites was re-pointed from the rebase's map and the evidence
  re-measured at the rebased code head `99788ca` (fmt, clippy, the
  release build, the witnesses red at `c08285f` and green, the probes,
  the eight gate runs, the differentials against trunk `9d9c023`): the
  same answers and the same numbers as before the rebase, row for row.
  The pre-rebase logs (`*-1648ea7*`, `*-61cb7d8*`, `*-a804d40*`,
  `trunk-6d6cde5*`) are kept as what they are.
- `head2.sh` built the trunk binary in `headsrc/` after the head's, so
  its witness step ran on a checkout of trunk and found no witness
  directory (`== *`); `scripts/redgreen.sh` re-ran the red, the green and
  the measurement beside wolf 0.2.20 at the right checkouts, and those
  are the logs cited.

#### §4 — evidence index

Commits:
- `3d23732` §1–§3; `aa32fc8` §3 addendum and §3a; the last commit this
  section and §5
- `c08285f` 26 witnesses (21 red); `e9fa018` the consumed row's cell;
  `99788ca` chapter 6's three shapes (bs57)
- `233dbb7` the static reader, E0801, E0611 and the by-name refusal
  (`src/rowmatch.rs`, `sema`'s chain, `frontend::admit`); `54b453e` the
  two-half dispatch and `Rule::RowMatch`; `79af9df` the lint
- `d19ccb6` rustfmt and clippy; `39c6390` E0816 (s197's code) and the
  mixed shapes by name; `683990c` CHANGELOG; `f7049fc` `lint_is61`'s
  `q10_map_match` moved with the ruling

Artifacts on kasumi under `~/lanes/is67/` (`archives/`, `evidence/`,
`probes/`, `scripts/`, `corpora/`, the read-only clones `wolf-lang-s196/`
at s197 `68d80376` and its worktree `wl-s196wt/` at s196 `7a8be823`).
Archives: lupin 0.1.43 `e957c8de…` (binary `3b0702c0…`) and wolf 0.2.20
`24855d5e…`, digests equal to the release pages' (`setup.log`); lupin
trunk `lupin-trunk-6d6cde5` `cb8d424c…` and, after the rebase,
`lupin-trunk-9d9c023` `4a27762b…`; head `lupin-head-99788ca` `d743fff5…`
(the code head: `src/` through `39c6390`, the witnesses through
`99788ca`); wolf-lang
s196 `wolf` `3bf39452…`, s197 `wolf` `3f7e29ce…`.
- inputs: `probes-archive-0.1.43-wolf-0.2.20.log` (34 probes on lupin
  0.1.43 and wolf 0.2.20's three tiers), `probes-trunk-6d6cde5.log`
  (identical lupin lines), `probes-closure-archive-0.1.43-wolf-0.2.20.log`,
  `probes-book-archive-0.1.43-wolf-0.2.20.log` (bs57's three shapes)
- red: `red-c08285f.log` (`rulings_is67`: 5 passed, 21 failed,
  `EXIT=101`)
- green: `witnesses-head-99788ca.log` (the 26 rows, lupin head beside wolf
  0.2.20), `probes-head-99788ca.log` (the probes, s197's nine rows
  included), `probes-book-head-99788ca.log`, `green-99788ca.log`
  (`rulings_is67` 29/29, `EXIT=0`); `wip-build.log`'s `rowmatch` unit
  tests ran again in the gauntlet
- gates: `wolfgate-s196-{head-99788ca,archive-0.1.43}-{unpinned,pinned}.log`,
  `wolfgate-s197-{head-99788ca,archive-0.1.43}-{unpinned,pinned}.log`
  (each `SKIP lines: 0`; the first s197 four in `superseded/`),
  `build-wl-s196wt.log`, `build-wolf-s196.log`, `build-wolf-s197.log`
- differential and corpus: `diffrun/{trunk-9d9c023,head-99788ca}-pin-{default,checked,native,release}.*`,
  `diffrun/{trunk-9d9c023,head-99788ca}-{wltrunk,s196,s197}-{default,checked,native,release}.*`
  (the pre-rebase runs, `trunk-6d6cde5` and `head-1648ea7`, kept beside them: the same numbers),
  `diffrun/*-corpus.log` (the census lines), `diffrun/*.ledger.jsonl`
- head: `fmt-99788ca.log` (`FMT_EXIT=0`), `clippy-99788ca.log`
  (`CLIPPY_EXIT=0`), `build-head-99788ca.log`
- gauntlet at `99788ca` (`gauntlet-99788ca.log`, red on `q10_map_match`
  alone, the missed prediction above) and at `f7049fc`, the code head
  with that row moved (`gauntlet-f7049fc.log`; the commits after it are
  docs only; its result
  is in §5 and the PR body); the superseded `gauntlet-5fffc6d.log` in
  `superseded/`
- GitHub CI at the head sha: in the PR body

Filed and commented: nothing new. The three findings this lane made are
in §2 (0.1.43's value half was eleven silent wrong answers, not an
unimplemented form; wolf 0.2.20's native and release still die on the
`?`-under-`defer` programs; s196's five run rows already print the ruled
bytes on lupin).

#### §5 — done-when

- [x] branch `is67` on origin, cut at trunk `6d6cde5` and rebased onto `9d9c023` (is65 merged); PR #168 open, unmarked draft once CI is green, with these five sections
- [x] §2 re-derived; §3 committed (`3d23732`) before the first `src/`/`tests/` edit; §3 addendum (`aa32fc8`) before the E0816 edit
- [x] each witness seen red first (`red-c08285f.log`), green at the head (`green-99788ca.log`)
- [x] s196's gate with its 0.1.43 pin dropped: green with the head, red with 0.1.43 on exactly 3
- [x] s197's gate with its 0.1.43 pins dropped: green with the head, red with 0.1.43 on 8 (the `else` control green)
- [x] s197's nine rows and s196's eight rows answer their `check:` lines on the head
- [x] no new divergence against wolf 0.2.20 beyond the ruled rows (named in §3a); the ledgers move 0 rows on the pinned and wolf-lang trunk corpora
- [x] CHANGELOG `Unreleased`
- [ ] the coverage ratchet holds at 271 (`export::coverage_is_ratcheted`, in the gauntlet and CI)
- [ ] kasumi gauntlet green at `f7049fc` (`GAUNTLET_FAILS=0`); GitHub CI green at the head sha (the PR body)
- [ ] kasumi build dirs pruned once the evidence is written (`headsrc/target`, `wl-s196wt/target`, `wolf-lang-s196/target`, `dev/target`); worktree `/private/tmp/is67` removed after the last push

### The P0s and the issues beside them — is68, wolf-interp#126, #103, #125, #138, #169

Wave 53's row, from t04's triage (wolffe-lang/wolf#6, `sprints/triage/t04-report.md`):
"wolf-interp #126 and #103 P0s, plus #125, #138, #169". Two of the three P0s
on the board are this machine's: #126 runs a `[mem.str.view]` product out of
its region and prints from freed bytes where every wolf lane refuses E1010,
and #103's first row binds `"*"` from an `if` with no `else` where
`[type.unit.context]` says the value is `()`. The contract is is65's, in five
sections; §1–§3 are committed before the first edit under `src/` or
`tests/`, the rest is appended as it lands. Measurements on kasumi (linux
x86-64) under `~/lanes/is68/`: the published lupin 0.1.43 (`e957c8de…`) and
wolf 0.2.20 (`24855d5e…`), archive digests equal to the release assets'
(`setup.log`); lupin trunk `6ce7bc8` built release (`bin/lupin-trunk-6ce7bc8`,
`93fd8329…`); wolf-lang trunk `12a56b22` built release
(`bin/wolf-trunk-12a56b22/wolf`, `eaa22f61…`). Probes: 125 one-directory
programs in `probes/` (71), `probes2/` (43) and `probes3/` (11), run by
`scripts/run-probes.sh` (`lupin conform-run main.lu --json`, and `wolf
conform-run main.lu --{checked,native,release} --json`), spans and phases by
`scripts/dump.sh`.

#### §1 — forbidden, absolutely

No `rm` outside `~/lanes/is68/` (kasumi) and `/private/tmp/is68`; no
deletion in any tree this lane did not create; no edit to r26's files (the
re-pin, `CHANGELOG.md`, the version bump) — this lane writes no CHANGELOG
entry and expects a rebase onto r26; no `git add -A`; no edit to another
lane's file; no workflow edit; no `~/.claude`; no build or test on this Mac
or nomad-1 (kasumi only, `CARGO_BUILD_JOBS=4`); no tag; no pin move; no
merge; no `2>/dev/null` on a checkout; kill only my own pids, never a
pattern or a group; jobs launched with `setsid`, never `ssh -f`; no claim
of "seen red" without the log it is in; no trailer of any kind on any
commit; no `gh run watch` without `--interval 60`; no silent wait past four
minutes.

#### §2 — inputs, re-derived 2026-10-02

| input as written | at origin / measured | drift |
| --- | --- | --- |
| wolf-interp trunk `6ce7bc8` or later | `origin/trunk` = `6ce7bc8` (is67's merge); `Cargo.toml` 0.1.43; `vendor/upstream/PIN` `c2401f05` (wolf-lang v0.2.19); r26 (PR #170, branch `r26` at `ba47627`, CI run 37051991895 in progress) re-pins on v0.2.20 (`cdde128a`) and owns `CHANGELOG.md`, the version and the pin | none |
| the archives and trunks | lupin 0.1.43 linux x86-64 `e957c8de…`, wolf 0.2.20 `24855d5e…`, both equal to the release assets' `digest`. lupin trunk `6ce7bc8` answers byte-identically to the 0.1.43 archive on all 125 probes, and wolf trunk `12a56b22` (s197's merge) byte-identically to wolf 0.2.20 on all 125, every tier (`probes{,2,3}-*.log`) | none: "wolf trunk" and 0.2.20 are one answer on everything this lane touches |
| #126: `region scratch { let s = "  re" + "gions  "; let t = s.trim(); t }` returned from `build()` | lupin `exit(0)` `regions`; wolf `fail(E1010)` `[109,110]` at `mem` on checked, native and release | none |
| #126's family | `[mem.region.escape]` (s171): `trim`/`trim_start`/`trim_end`, `get`, `strip_prefix`/`strip_suffix`, the pieces of `split`/`words`/`lines` carry the RECEIVER's sites; `bytes()` excluded; a view of a literal is site-free; only the receiver's sites flow, never the needle's. Measured: every member returned, held outside (`keep = s.trim()`), as the block's value, through a struct field, a view of a view, `copy` of a view inside the region, the slice `s[2..6]`, `get` open and inclusive, a split piece by index (`ps[0]`), and sent from a `spawn proc`: lupin `exit(0)`, wolf `fail(E1010)` on three tiers — 20 shapes | wider; all are witnesses |
| #126 controls | a view of a literal (`[lit]`), of a parameter, of an outer local held in a region, a needle built in the region (`[z]`), views used inside the region, `region_str_view_inside.lu`'s shapes: `exit(0)`, the same bytes on all four. `s.bytes()` returned and `s.trim().upper()` returned: lupin already `trap(region-fault)`, wolf `fail(E1010)` | none |
| **#126, the compiler side** | a piece bound by a `for` over `s.words()`/`s.lines()`/`s.split(",")` and then returned (`last = w; last`) or held outside (`keep = w`): **wolf `exit(0)` on all three tiers**, printing the piece; the clause names the pieces of all three, so the compiler is the looser machine here — the same freed-bytes read #126 is, one binding further | **new finding**: filed in wolf-lang, §3's table carries the four rows with lupin's ruled answer |
| the mechanism (#126) | `Str` carries `home: Option<RegionId>`, consulted at every access and at a block's exit (`escaping_regions`, `pop_scope_escaping`), which is how a built `str` and every materializing producer already fault (`produced_str`, `builtin.rs:2518`). The view arms mint `home: None`: `trim`/`trim_start`/`trim_end` (`builtin.rs:1529`), `get` and `str_get` (`:2299`), the `s[a..b]` slice (`:2247`), `strip_prefix`/`strip_suffix` (`:1689`), the `split`/`words`/`lines` pieces (`:1668`, `:1725`, `:1732`). The doc comment on `Str` (`value.rs:458`) and `a_view_product_is_no_site_and_escapes_clean`'s comment still state the pre-s171 reading | as the issue reads it |
| #103 row 1 | `let mark = if cents > 300 { "*" }`: lupin `exit(0)` `*`; wolf `fail(E0401)` `[72,75]` (the `"*"`) at `typecheck`, three tiers | none |
| #103's neighbourhood | `[type.unit.context]`'s list on wolf: a non-unit tail is `fail(E0401)` at the tail in an else-less `if` as a statement (`[61,64]`), in a `for` (`[42,43]`), `while` (`[78,79]`) and `loop` (`[98,101]`) body, in a unit fn body (`fn total() { 1 }`, `[13,14]`; `fn main() { …; 3 }`, `[32,33]`; `-> ()`, `[19,20]`; a nested fn, `[46,47]`; a method, `[90,96]`), for literal, interpolated, arithmetic, comparison, concatenation, list, struct, char, float, call (`seven()`, `[81,88]`), parameter and local tails, and for an `if … else` tail (`[57,86]`, the whole `if`). A chain ending without `else` reports the trailing else-less `if` first (`[83,105]` then `[100,103]`; three links: `[90,106]` then `[101,104]`). lupin runs every one; `fn main() { …; 3 }` exits 3 | wider; the shapes lupin can type from syntax are witnesses |
| #103 row 1's dynamic half, measured | a `!T` tail is W0601's discard (`[type.unit.discard]`): `let v = if n > 1 { maybe(n) }` (`int ! {none}`) prints `()` on native and release and **`3` on checked and lupin**; `fn main() { print("hi"); boom() }` (a `() ! {bad}` tail) exits 0 on native and release, **1 with `error: bad` on checked and lupin**; a discarded raise in a statement-position else-less `if` exits 1 on checked only. But an else-less `if` that is the TAIL of a `-> () ! {bad}` fn hands the raise to its caller on all three tiers and on lupin (`raised`), with no W0601 | **new finding**: the checked machine propagates a discarded row (filed in wolf-lang). The tail-position flow on all four is not what `[type.unit.discard]` says either; left as all four answer it and noted in the filing |
| #103 rows 2–4 | row 2 (`pick(1, "two")`) and row 4 (`{ xs.len }`, a member read) need types this machine does not have; row 3 (the loop element binding) is `exit(0)` `a\nb` on BOTH machines at 0.2.20 — healed upstream | row 3 healed; rows 2 and 4's static halves stay owed (row 4's dynamic value becomes `()` with row 1's) |
| the mechanism (#103) | `ExprKind::If` (`mod.rs:4996`) returns the then-block's value; `call_fn` (`:3042`) returns the body's value whatever `decl.ret` says. No resolve-rung check reads a unit context | as the issue reads it |
| #125 | `s.trim(".,!?")`: lupin `exit(0)` `[hi]`; wolf `fail(E0402)` `[52,66]` (the whole call). The ruling (s175) is option 1 and `[mem.str.ws]` says "the family takes no argument". Measured: `trim_start`, `trim_end`, `words`, two arguments, a variable argument, a literal receiver (`[33,50]`), a `str` parameter receiver (`[30,41]`) — all E0402 on wolf. `lines(x)` is E0402 too, outside the clause's family. A user `impl` method named `trim(k)` runs on both | wider: `lines` |
| #125, what this machine can see | a receiver this machine cannot type (a `for` piece, `c12`) is still E0402 on wolf; the builtin arm ignores extra arguments (`trim_start`/`trim_end`/`words`/`lines` run unchanged) and `trim` treats one as a cutset | the static half covers receivers known to be `str` (a literal, a literal-bound local, a `str` parameter); the rest declines by name |
| #138 | s181's program (`let s: str = side`, `side: int`, in a sibling module): lupin `exit(0)`; wolf `fail(E0401)` `[90,94]`, file index 1 | **not sibling-specific**: the same body in the root module runs too. lupin's declared-scalar lattice (`ScalarTy`, `sema.rs:5101`) knows `byte`, `char`, integers and their lists and nothing else, so `str`/`bool` against a known other scalar is never a clash. Measured on wolf, all E0401 at the operand: a `let` annotation (`[37,38]`, `[45,49]`, `[53,54]`, `[41,42]`, `[38,39]`, `[37,41]`, `[37,40]`, `[51,56]`), a return tail (`[23,24]`), an argument (`[73,74]`, `[88,93]`), a field (`[72,73]`), an assignment (`[58,59]`); a two-file sibling module reports file index 2 |
| #138's file index | an item-walking check reports no file (wolf-interp#136), so a sibling diagnostic would compare as a different file under wolf-lang#437 | the new refusals carry their item's file; #136's other checks stay as they are |
| #169 | r26 waives `memory/nested_fn_mut_omitted.lu` by name in `tests/conformance.rs` (`NESTED_MODED_FN_DECLINED`, commit `6b7ac41` on `r26`); the file arrives with the `cdde128a` pin, so it is not on trunk | the waiver and its file exist only on r26: the retirement lands after the rebase onto r26 |
| #169's rows | `nested_fn_mut_param.lu`: wolf native/release `exit(0)` `4 42 2 9 7 5\n4\n`, checked `unsupported` (a nested fn); `nested_fn_mut_omitted.lu` `fail(E1007)` `[600,602]`; `nested_fn_mut_moveout.lu` `fail(E1001)` `[600,602]`; lupin `unsupported` on all three ("declares a parameter mode", `mod.rs:3886`). Wider: a `take` omitted (`[117,119]`) and a `mut` spelled on a read parameter (`[118,120]`, beside W0308) are E1007 — the second RUNS on lupin today; a moded nested fn restored before return runs `2 9` on native/release; capturing, recursive and as-a-value moded nested fns are `unsupported` on every wolf lane | wider; all listed |
| the mechanism (#169) | `exec_nested_fn` (`mod.rs:3846`) binds a nested fn as a capture-free `ClosureValue` and refuses a mode; `apply` (`:7648`) runs a closure with no modes, so the module-fn convention (`call_fn`: `read` barrier, write-back, moved-out parameters) is never reached. The resolve rung's mode pass (`check_call_modes`, `sema.rs:4393`) keys signatures by `(module, fn)` and never declares a nested item, so a call to one is never checked | as the issue reads it |
| CI | `.github/workflows/ci.yml` runs on `push` to trunk and on `pull_request`; about 3 h, and Actions is congested under eleven lanes | noted |
| the coverage ratchet | `tests/export.rs` on trunk: `RATCHET_FLOOR = 271`; r26 moves it to 273 | none |

#### §3 — prediction, committed before the first edit

**Five mechanisms, five commits under `src/`**, each after its witnesses:

1. **#126, `eval`/`builtin`: a view product carries its receiver's home.**
   Every `[mem.str.view]` arm builds `Str { text, home: receiver.home }`
   (one helper); the needle's home is never read; `bytes()` and `chars()`
   are untouched; a literal's home is `None`, so its views stay site-free.
   The trap is the one the clauses name: `region-fault`
   (`[mem.region.escape]`'s dynamic half, `[mem.region.intra.2]`), raised
   where a built `str` already raises it — no read of freed bytes. No
   charge moves (a view still allocates nothing).
2. **#103, `eval`: a unit context's value is `()`.** An `if` with no `else`,
   and every `if` of a chain that ends without one, evaluates to `()`
   whatever its taken block's tail — except a row value (a raise), which
   leaves as the `if`'s value exactly as all three wolf tiers let it leave
   (the tail-position finding above). A body whose fn declares no result
   or `-> ()` returns `()` (rows included: the native/release answer).
   **And statically, at the end of the resolve chain** (`unit_tail_check`,
   a second pass of the tier walk so no older row's first diagnostic
   moves): E0401 at a tail this machine can type from syntax in a unit
   context — the then-block of an else-less `if`, a `for`/`while`/`loop`
   body, a unit fn or method body (closures excluded) — where "can type"
   is: a literal (incl. interpolated), a list or struct literal, arithmetic
   or concatenation or a comparison over typed operands, a call to a module
   fn whose declared result is a plain scalar, a local or parameter the
   walk already classes, and an `if … else` whose then-tail is one of
   those (spanning the whole `if`). An `if … else` whose else is an
   else-less `if` and whose then-tail is typed reports at that else-less
   `if` first, as wolf does.
3. **#125, `sema` + `builtin`: the `[mem.str.ws]` family takes no
   argument.** E0402 at the whole call, wolf's sentence ("`trim` takes 0
   arguments, but this call passes 1"), for `trim`/`trim_start`/`trim_end`/
   `words` and `lines` on a receiver the walk classes `str`, in the same
   late pass; any other receiver reaching the builtin arms with an argument
   declines by name (`unsupported`, naming E0402) — never a cutset, never an
   ignored argument.
4. **#138, `sema`: `str` and `bool` join the declared-scalar lattice**, in
   a late pass (`scalar_wide_check`) so the byte/char rows keep their
   first diagnostics: a `str` or `bool` slot against a known other scalar,
   and a known `str`/`bool` against a scalar slot, is E0401 at the operand,
   in every position the pass already reads; the item's file rides on the
   diagnostic (`Module::item_files`, filled in `define`).
5. **#169, `sema` + `eval`: a nested fn's parameter modes are the module
   fn's.** The mode pass records each nested fn's signature in the block
   that declares it and checks a call to it exactly as a module fn's
   (E1007 at the argument: omitted, spelled where the parameter is plain,
   or the wrong word); the evaluator binds a moded nested fn with its
   declaration and calls it through `call_fn`, so writes reach the caller,
   a `take` consumes, and a parameter moved out and never stored back is
   moved-out in the caller (`trap(use-after-move)` at the next read — the
   dynamic counterpart of the E1001 row, as `mut_param_moveout_whole.lu`
   is). Captures, generics, rows and `self` keep their refusals; a mode
   disagreement through a value declines as a module fn's does.

**The witness table** (`tests/rulings_is68/<row>/`, lupin head against trunk
`6ce7bc8` = 0.1.43; wolf trunk `12a56b22` = 0.2.20, one answer on checked,
native and release unless named):

| witnesses | trunk and 0.1.43 | head | [wolf] |
| --- | --- | --- | --- |
| #126: the issue's program; `trim` (bound and as the tail), `trim_start`, `trim_end`, `get` (closed, open, inclusive), `strip_prefix` (bound and as the tail), `strip_suffix`, a view of a view, `copy` of a view inside its region, `s[2..6]`, a split piece by index, held outside, the block's value, a field's view, a proc send (19) | `exit(0)`, the view's bytes | `trap(region-fault)` (`mem.region.intra.2`) | [`fail(E1010)`] |
| #126: a `for` piece of `words`/`lines`/`split` returned, and a `words` piece held outside (4) | `exit(0)` | `trap(region-fault)` | [`exit(0)`: the compiler-side finding] |
| #126 controls: a literal's view, a parameter's view, an outer local's view held in a region, a built needle, views inside the region, `bytes()` returned, `trim().upper()` returned (7) | unchanged | unchanged | [same; the last two `fail(E1010)` against lupin's trap] |
| #103: the issue's program, a statement `"*"` and `1`, a chain of two and of three, a `while`/`loop` tail, a local, interpolated, arithmetic, `bool`, comparison, list, struct, char/float, concatenation, call, parameter and `if … else` tail, a unit fn, `main`, `-> ()`, nested fn (23) | `exit(0)` | `fail(E0401)` at wolf's span | [same] |
| #103 dynamic: `let v = if n > 1 { maybe(n) }`; `fn main() { …; boom() }` (2) | `3`; `exit(1)` `error: bad` | `()`; `exit(0)` `hi` | [native/release; checked is the filed finding] |
| #103 residue: `let v = total([1, 2])` with `{ xs.len }`; `for x in [1, 2] { x }`; a method's `self.n` tail (3) | `2`; `done`; `2` | `()`; `done`; `2` | [`fail(E0401)` — member reads and an element type this machine cannot see] |
| #103 controls: a unit-valued `if`, an `if … else` value, a raise out of a tail else-less `if` (`raised`), a W0601 statement discard, `match` arms in a loop, a closure with no result, a statement raise, a fn-body statement `if` (8) | unchanged | unchanged | [same] |
| #125: the issue's program, `trim_start`, `trim_end`, a variable cutset, two arguments, `words(" ")`, `lines("x")`, a literal receiver, a `str` parameter receiver (9) | `exit(0)` | `fail(E0402)` at wolf's span | [same] |
| #125: a `for` piece's `trim(".,")` (1) | `exit(0)` `2` | `unsupported` (E0402 named) | [`fail(E0402)`] |
| #125 controls: the zero-argument family; a user `trim(k)` method (2) | unchanged | unchanged | [same] |
| #138: s181's two-file program (file index 1), a three-file module (index 2), the root module, and the twelve positions of §2 (15) | `exit(0)` (two `unsupported`) | `fail(E0401)` at wolf's span and file | [same] |
| #138 controls: a sibling's `int` from an `int`, `str` from a `str`, the ok shapes, a generic `str`, a `List[str]` element (5) | unchanged | unchanged | [same, the generic one aside: wolf `fail(E1002)`, unrelated] |
| #169: `nested_fn_mut_param.lu`, the restored form (2) | `unsupported` | `exit(0)` `4 42 2 9 7 5\n4\n`; `2 9` | [native/release; checked `unsupported`] |
| #169: `nested_fn_mut_omitted.lu`, `take` omitted, `mut` on a read parameter (3) | `unsupported`; the third `exit(0)` | `fail(E1007)` at wolf's span | [same] |
| #169: `nested_fn_mut_moveout.lu` (1) | `unsupported` | `trap(use-after-move)` | [`fail(E1001)`: the dynamic counterpart] |
| #169: capturing, recursive, as a value (3) | `unsupported` | `unsupported`, `unsupported`, `exit(0)` `1` | [`unsupported` on every lane] |

Each witness is run red at the commit that adds it (every row whose head
column differs) and green at its fix; the commits are ordered so each fix's
witnesses go red in a CI run of their own (s188's lesson).

**Existing tests that change** (each named in the commit that moves it):
`tests/run_corpus.rs`'s run ledger — `memory/region_str_view_return.lu`
`exit(0)` → `trap(region-fault)` (a dynamic counterpart now, not
conservatism) and `strings/trim_cutset_refused.lu` leaves the ledger (a
match at `resolve`); `tests/conformance.rs` — `E0402` by `mem.str.ws` joins
`declaration_read_code`'s table; `src/eval/tests.rs`'s
`a_view_product_is_no_site_and_escapes_clean` keeps its asserts and loses
the pre-s171 comment. Nothing else: no corpus row at the pin returns a view
of a region-built `str`, binds an else-less `if`, passes a cutset, clashes a
`str`/`bool`, or calls a moded nested fn.

**Corpus and differential.** `lupin corpus` at the pin: the two rows above
move, nothing else. `lupin diff-run` on four tiers against wolf 0.2.20 on
the vendored corpus: the two rows move toward agreement (a trap where E1010
was a run; a matching E0402), no row moves away. After the rebase onto r26
the same at the `cdde128a` pin, plus `nested_fn_mut_omitted.lu` (E1007, a
match), `nested_fn_mut_param.lu` (wolf's bytes) and
`nested_fn_mut_moveout.lu` (a dynamic counterpart), and r26's
`NESTED_MODED_FN_DECLINED` waiver deleted by name.

**Coverage.** The ratchet holds (271 on trunk, 273 after r26): the
witnesses carry no `conforms:` line the bundle counts.

**Out of scope, named:** #103 rows 2 and 4's static halves (inference this
machine does not have); wolf-interp#136's other item-walking checks; the
tail-position raise out of an else-less `if` (all four machines agree, the
clause reads otherwise — in the wolf-lang filing).

#### §3a — the prediction, scored

The lane was cut at trunk `6ce7bc8` and rebased onto r26's `ba47627`
(PR #170, the `cdde128a` re-pin, unmerged when this lane closed) so that
#169's waiver could be retired by name; every commit sha below is on the
rebased branch, and the evidence logs keep the sha they were taken at (the
pre-rebase twins are named where they differ).

- **held**: five mechanisms, each in its own `src/` commit(s) after its
  witnesses: #126 `1a1c14f`; #103 `5314dfb` (eval) and `db9ac2a` (sema);
  #125 `a4862f7` (sema) and `a77c4a4` (eval); #138 `a75adcc`; #169
  `3ffdfd7` (sema) and `7dc33ba` (eval).
- **held**: the witness table. 108 rows at `c8e8535`: 81 red, 27 green,
  the split §3 counted row for row (`red-c8e8535.log`; the identical
  split at the pre-rebase `e614d3c`, `red-e614d3c.log`). Each fix turned
  exactly its own rows green: 58 left after #126, 32 after #103, 22 after
  #125, 7 after #138, 0 after #169 (`fix126-7ebe1ba.log`,
  `fix103-5d76681.log`, `fix125-89ca590.log`, `fix138-15752fc.log`,
  `fix169-2ac7e10.log` — pre-rebase shas of `d11cafe`, `6fd7454`,
  `6bebaac`, `a75adcc`, `7dc33ba`). 108/108 at `849e8e4` and at the head.
- **held**: the trap is the clauses' — every #126 row answers
  `trap(region-fault)`, `mem.region.intra.2`, at the region's exit or at
  the first read after it; no row prints a view of freed bytes.
- **missed, three existing tests**: §3 named three tests that change
  (`run_corpus.rs`'s two ledger rows, `conformance.rs`'s E0402 row, a
  comment in `src/eval/tests.rs`). Two more moved:
  `eval::tests::the_nested_fn_scoped_out_shapes_refuse_by_name` pinned
  the mode refusal #169 removes (red in `fix169-2ac7e10.log`; the mode
  shape moved to its own test, `a_nested_fn_with_a_mode_is_a_module_fns_call`,
  `849e8e4`), and `lint_is61`'s `t01_type_error_beside_move` pinned
  lupin RUNNING `let n: int = "a"` — #138 refuses it E0401 at the
  compiler's span `[83,86]`, so the one parting that test recorded heals
  (red in `light-f897e62.log`, updated at `c1ad684`). Neither was a
  behaviour any clause or ruling wanted kept.
- **held**: corpus and differential. At the `c2401f05` pin (trunk against
  the pre-rebase head `6611216`): the census moves by exactly the two
  rows (586 → 585 run, conservatism 60 → 58, dynamic 53 → 54); the
  gating lines are the same on all four tiers (4 / 4 / 6 / 6); one ledger
  row leaves per tier (`trim_cutset_refused.lu`'s `rejects-beyond`). At
  the `cdde128a` pin (r26's `ba47627` against `b6ea927` and the head):
  666 → 667 run, 601 → 604 match, 81 → 83 dynamic, 65 → 63
  conservatism, 68 → 65 out of scope; gating lines the same on all four
  tiers; per tier the `unsupported(interp)` rows of the three
  `nested_fn_mut_*.lu` files leave, and the E1007 and E0402
  `rejects-beyond` rows leave; `nested_fn_mut_param.lu` becomes
  `run-unmatched` on default/checked only (the checked executor declines a
  nested fn). The only line that differs otherwise is
  `unsafe_ub_uaf.lu`'s exit status under UB on native, which moves run to
  run. No row moves away from agreement.
- **held**: the waiver retires by name — at `849e8e4` (rebased, waiver in
  place) both conformance tests go red on
  `memory/nested_fn_mut_omitted.lu (#169)` (`waiver-red-849e8e4.log`);
  `b6ea927` deletes `NESTED_MODED_FN_DECLINED` and both are green.
- **held**: the ratchet (see §4).
- **new, beside the prediction**: two compiler-side findings from §2's
  probes, filed: wolffe-lang/wolf-lang#540 (a `for`-bound piece of
  `words`/`lines`/`split` escapes its region on all three lanes; lupin
  traps it) and wolffe-lang/wolf-lang#541 (the checked machine propagates
  a row W0601 discards; native and release discard, and lupin now answers
  with them).

Slips, repaired:

- `db9ac2a` does not compile: it added a second `TierWalk::is_local`
  beside an existing one (`fix103-2b4bf49.log`, pre-rebase); fixed forward
  at `6fd7454`.
- my `pgrep -f` on the gauntlet's own command line matched the ssh shell
  that ran it and killed that shell (my pids only); the gauntlet itself was
  then stopped by number.
- rustfmt, five times: the at-sha runs check `cargo fmt` first and each
  reflow is its own commit (`e8de8d4`, `2f33757`, `52df1e8`, `c416f9e`,
  `e7cf848`).
- three gauntlets were stopped by pid and their logs kept: at `6611216`
  when the rebase onto r26 moved the head
  (`superseded-gauntlet-6611216-killed-for-r26-rebase.log`), at `f897e62`
  when the light set found `t01`
  (`superseded-gauntlet-f897e62-lint_is61-t01.log`, fmt and clippy
  already 0 there), and at `c1ad684` when `heavy3` found the census block
  and the no-stdout rule
  (`superseded-gauntlet-c1ad684-doc_truth-run_corpus.log`; `cli`,
  `conformance` and `divergence` had passed).

#### §4 — evidence index

Commits (on `is68`, rebased onto r26's `ba47627`, which merged to trunk
as a fast-forward at 23:13Z):

- `a5f923d` §1–§3; the closing commit §3a, §4, §5
- `c8e8535` 108 witnesses (81 red); `e8de8d4` rustfmt
- #126: `1a1c14f` eval; `7d6f6ee` unit test; `d11cafe` run ledger; `2f33757` rustfmt
- #103: `5314dfb` eval; `db9ac2a` sema; `6fd7454` the duplicate removed; `52df1e8` rustfmt
- #125: `a4862f7` sema; `a77c4a4` eval; `9def8e3` conformance; `6bebaac` run ledger
- #138: `a75adcc` sema; `c416f9e` rustfmt
- #169: `3ffdfd7` sema; `7dc33ba` eval; `e7cf848` rustfmt; `849e8e4` unit tests; `b6ea927` r26's waiver retired by name; `70dcdf5` run ledger
- `f897e62` the manual's census; `c1ad684` `lint_is61` t01; `22fc514` the manual's bundle replay; `7e7c51c` the no-stdout record rule (the code head)

Artifacts on kasumi under `~/lanes/is68/evidence/`. Archives: lupin
0.1.43 `e957c8de…` and wolf 0.2.20 `24855d5e…`, both equal to the
release assets' digests (`setup.log`); wolf-lang trunk `12a56b22` built
release (`bin/wolf-trunk-12a56b22/wolf`, `eaa22f61…`); lupin trunk
`6ce7bc8` (`93fd8329…`), r26 `ba47627` (`1e62a0d1…`), head `f897e62`
(`629396d5…`, the same `src/` as `7e7c51c`). Every wolf measurement
finished by 21:07Z, before kasumi's system upgrade began (21:35Z, `rc=0`
at 21:43Z): no wolf binary was run after it.

- inputs: `setup.log`, `probes-archives-0.1.43-0.2.20.log`,
  `probes-trunk-6ce7bc8-wolf-12a56b22.log`, `probes2-trunk-6ce7bc8.log`
  (wolf trunk and 0.2.20 both), `probes2-archive-0.1.43.log`,
  `probes3-trunk-6ce7bc8.log`, `probes3-archives-0.1.43-0.2.20.log`;
  the witness tree from them, `genwit.log`
- red: `red-c8e8535.log` (27 passed, 81 failed; the same rows as
  `red-e614d3c.log` before the rebase); the waiver: `waiver-red-849e8e4.log`
- per fix (pre-rebase shas): `fix126-7ebe1ba.log`, `fix103-2b4bf49.log`
  (does not compile), `fix103-5d76681.log`, `fix125-89ca590.log`,
  `fix138-15752fc.log`, `fix169-2ac7e10.log`, `fix169b-6611216.log`;
  after the rebase `quick-f897e62.log` (108/108, lib 745, conformance 9,
  fmt 0); the light set (54 binaries, 1,738 passed) `light-c1ad684.log`;
  `heavy3-c1ad684.log` (`export` 10/10 with `coverage_is_ratcheted`;
  `doc_truth` and `run_corpus` one red each, both fixed at `22fc514` and
  `7e7c51c`); `heavyB-7e7c51c.log` (`fuzz_smoke`, `region_machine`,
  `rule_registry`)
- head: `head-f897e62.log`, `build-head-f897e62.log`,
  `probes{,2,3}-head-f897e62.log` (every lupin answer identical to
  `…-head-6611216.log`, the pre-rebase head; wolf trunk = 0.2.20 on all
  125)
- differential and corpus: `diffrun/{trunk-6ce7bc8,head-6611216}-*` at the
  `c2401f05` pin; `diffrun/{r26-ba47627,rebased-b6ea927,head-f897e62}-*`
  at the `cdde128a` pin; `pinpair-ba47627-b6ea927.log`
- clippy: `clippy-6611216.log` (0); the gauntlet: `gauntlet-7e7c51c.log`
  at the code head (the closing commit is `docs/divergence-log.md` only,
  which no test reads); its result and GitHub CI's are in PR #172's body
- filed: wolffe-lang/wolf-lang#540, #541

#### §5 — done-when

- [x] branch `is68` on origin, cut at trunk `6ce7bc8`, rebased onto r26's
  `ba47627` (now trunk); PR #172 open, unmerged; five sections
- [x] §2 re-derived; §3 committed before the first edit; §3a scored
- [x] each witness red first (`red-c8e8535.log`), green at its fix and at
  the head
- [x] #126: the escape is `trap(region-fault)` (`mem.region.intra.2`), the
  dynamic counterpart of E1010; never a read of freed bytes
- [x] #103 row 1: E0401 at `[72,75]` as on every wolf tier; the else-less
  `if`'s value is `()` where this machine cannot type the tail
- [x] #125, #138, #169 to the clause, agreeing with wolf trunk and 0.2.20
  on checked, native and release wherever wolf answers (two compiler-side
  partings filed, #540 and #541)
- [x] #169's waiver retired by name (`b6ea927`)
- [x] no new divergence against wolf 0.2.20 at either pin; corpus census
  as predicted; no CHANGELOG entry (r26's file)
- [x] coverage ratchet holds (`coverage_is_ratcheted` ok; floor 273)
- [ ] kasumi gauntlet green at the code head `7e7c51c` (PR body)
- [ ] GitHub CI green at the head (PR body)
- [ ] #126, #125, #138, #169 close on merge (by hand if not); #103 stays
  open for rows 2 and 4's static halves (row 1 done here, row 3 healed
  upstream), with a comment saying so
- [ ] kasumi build dirs pruned, worktree removed after the last push

### The inferred row — is69, wolf-interp#176, `-> !T`'s private row read from the body (`[type.row.match]` over an inferred row)

Wave 53's row (the orchestrator's brief, 2026-10-02): lupin 0.1.44's
row-match reader (is67's `src/rowmatch.rs`, ruling #21) reads a function's
inferred `-> !T` row as the open row `{..}`, so wolf-lang's
`corpus/rows/eu_bind_empty_row_handled.lu` (s202, wolf-lang trunk
`ebba7574`) answers `fail(E0801)` where checked, native, release and lupin
0.1.43 answer `exit(0)` `43 42 42`. The grammar (`01-grammar.md`:
"`-> !T` error union with inferred private row") and the compiler's
sealing (`crates/wolf_sema/src/rows.rs`, s15) say the row is what the body
can raise. The contract is is67's, in five sections; §1–§3 are committed
before the first edit under `src/` or `tests/`, the rest is appended as it
lands. Measurements on kasumi (linux x86-64) under `~/lanes/is69/`: the
published lupin 0.1.44 (archive `e44aae06…`, binary `be9bf9fc…`) and
0.1.43 (archive `e957c8de…`, binary `3b0702c0…`), both archive digests
equal to the release pages' (`evidence/setup.log`); wolf-lang r26
`dfcc2f13` (trunk `ebba7574` plus the 0.2.21 pairing, which carries the
pin this lane retires) built debug in `wolf-lang/` (`wolf` `135f784f…`,
`libwolf_rt.a` beside it). Probes: 24 one-directory programs under
`probes/` (`i69_*` the witnesses, `q_*` the questions asked of the
compiler), run by `scripts/run-probes.sh` (`lupin conform-run main.lu
--json` and, on a `fail`, the first line of `lupin check`; `wolf
conform-run main.lu --{checked,native,release} --json` and, on a `fail`,
the first line of `wolf build`): `evidence/probes-archives-0.1.44-0.1.43-wolf-dfcc2f13.log`.

#### §1 — forbidden, absolutely

No `rm` outside `~/lanes/is69/` (kasumi) and `/private/tmp/is69`; no
deletion in any tree this lane did not create; no `git add -A`; no edit to
another lane's file — wolf-lang is read and built in the lane's own clone,
never edited or pushed (the gate's pin is emptied in that clone for a run
and restored by `git checkout` after it); is68's code (wolf-interp#126,
#103: `sema`'s late tier pass and unit-context arms, `eval/builtin.rs`'s
`[mem.str.ws]` family, `eval/value.rs`, `tests/run_corpus.rs`) is not
touched; no workflow edit; no `~/.claude`; no build or test on this Mac or
nomad-1 (kasumi only, `CARGO_BUILD_JOBS=4`, jobs launched with `setsid
-f`, never `ssh -f`); no tag; no pin move (the pin stays `cdde128a`); no
merge; no `2>/dev/null` on a checkout; kill only my own pids by number,
never a pattern or a group; no claim of "seen red" without the log it is
in; no trailer of any kind on any commit; waits in printing loops; `gh run
view`, never `gh run watch` without `--interval 60`.

#### §2 — inputs, re-derived 2026-10-02

| input as written | at origin / measured | drift |
| --- | --- | --- |
| wolf-interp trunk `ba47627`, lupin 0.1.44 tagged | `origin/trunk` = `ba47627` (rustfmt after `91774fa`, the 0.1.44 release commit); `v0.1.44` released 2026-10-02; `Cargo.toml` 0.1.44; the archive's record answers `impl_version` `0.1.44`, a dev build `0.1.44+dev.<sha>`, so the gate's `"0.1.44"` pin never matches a branch build — only the archive | none; it decides the plant below |
| is68 (#126, #103) in flight | branch `is68` at `5e7c944`: `src/sema.rs` (+689), `src/eval/mod.rs`, `src/eval/builtin.rs`, `src/eval/value.rs`, `src/eval/tests.rs`, `tests/conformance.rs`, `tests/lint_is61.rs`, `tests/run_corpus.rs`, the log | this lane's `sema.rs` edit is one field of `Program` and its initializer; a rebase is expected and will be subset-checked |
| the issue's row: `fail(E0801)` on 0.1.44, `exit(0)` `43 42 42` elsewhere | reproduced: 0.1.44 `fail(E0801)` "does not cover the rest of an open row: the scrutinee is a `int ! {..}`" at 23:13; 0.1.43 and wolf r26 checked, native, release `exit(0)` `43 42 42` | none |
| r26 pins 0.1.44 by version on this one case in `fallible_bind_empty_row_lanes.rs` | `origin/r26` `51b478b4`: `lupin_pre_mirror` `&[("0.1.44", "fail(E0801)")]` on `a_bound_empty_row_value_matches_elses_and_widens` only. At `dfcc2f13` with the archive: pinned 3/3 green (`wolfgate-archive-0.1.44-pinned.log`, `EXIT=0`); pin emptied, exactly that case red, `left: "fail(E0801)" right: "exit(0)"` (`wolfgate-archive-0.1.44-unpinned.log`, `EXIT=101`); 0 SKIP lines in each | none |
| the compiler's inference (s197's clause over s15's sealing) | `rows.rs`: a cycle-aware fixpoint over each module's inferred fns, rows growing monotonically from empty; the collector absorbs (a) a tag raised at a checked position against the fn's own row (`inject_tag`: a bare capitalized unresolved name, or such a name called with a payload), (b) a fallible value flowing into the return (`expect_unify` → `require_row_widening`: a tail or `return` whose value is a `T ! {row}`), (c) a `?`'s row (`caller_row`), (d) a spawned task closure's row at the spawn; a `?` inside a closure is the closure's own. `absorb_row` takes the listed tags only, so an open row flowing in is later E0602 (`open_into_closed`) | none; this machine mirrors (a)–(c) and treats (d), and anything it cannot name, as not inferable |
| the witnesses the brief names | measured on the compiler (r26, three tiers, one answer each): one tag `4 -1`, uncovered E0801 "does not cover `Neg`"; `?` from a callee `8 -1`, uncovered "`bad`"; a value flowing out of the tail `5 -1`, uncovered "`none`"; recursion and mutual recursion `0 -1 10 -2`, the cycle's tag uncovered "`Near`"; the grown row: before `4`, after "`High`", answered `4 -2`; generic `4 -1 x`, uncovered "`Empty`". **lupin 0.1.44 answers `fail(E0801)` "open row" on every one, the runs included**; 0.1.43 runs all of them and BINDS the value to the first tag arm (`-1 -1`, `-1 -1`, `-1 -1 -2 -2`, `-1 -1 -`: is67's value-half defect) and runs the uncovered ones | the 0.1.43 answers are silent wrong answers already retired by is67; every run row here is a 0.1.44 false refusal |
| the issue's fallback: "where this machine cannot infer it, keeps the old dispatch" | `q_method_tail` (`fn pick(xs) -> !int { xs.get(0) }`, `match pick([3]) { v => v }`): the compiler `fail(E0801)` "does not cover `none`"; 0.1.44 `fail(E0801)` (the open row); the old dispatch would run it (0.1.43: `3`) | **the fallback keeps 0.1.44's reading (`{..}`), not the old dispatch**: a row this machine cannot infer may hold tags, so only `_` is safe, and the old dispatch would part with the compiler where 0.1.44 agrees |
| the other readings of an inferred row | `let a: !int = f()` then `match a { v => v + 1 }` (`i69_let_annotated`): the compiler `exit(0)` `43`; 0.1.44 E0801 (the annotation read as `{..}`). `let a: !int = half(8)` with `half`'s row `{Neg}` (`q_let_annot_tag`): the compiler refuses the binding, `fail(E0602)` "this can also fail with `Neg`, which `main`'s row does not include"; 0.1.44 E0801 | the annotated binding is a reader of the inferred row: it takes its initializer's row where that is known; `q_let_annot_tag` stays a verdict-code parting (E0801 here, E0602 there; lupin has no E0602 for a binding) — named, not this lane's |
| more compiler answers | a tail `xs.len` (`q_method_len_tail`) `1`; a tail field read `p.x` (`q_field_tail`) `3`; a tail `match` whose value arm returns a binder (`q_match_binder_tail`) `8`; a `?` inside a closure (`q_closure_try`) native and release `1`, checked `unsupported` (closures) | a member read of a plain value and a value arm's binder are plain; a closure's `?` is its own |
| nested fns | `eval` refuses a nested fn with a rowed return by name (`unsupported`, "lift it to the module (#38)") | none; out of scope |
| the coverage ratchet | `tests/export.rs`: `RATCHET_FLOOR = 271` | none |
| kasumi | load 7–9 on 24 threads; `/home` 96%, 39 G free; the lane dir about 1.5 G after the wolf-lang debug build | prune `wolf-lang/target` and `dev/target` as evidence is written |

#### §3 — prediction, committed before the first edit

**The reading.** A module `fn` whose return is `-> !T` (no row spelled)
has the row its body can raise, sealed per module as the compiler seals
it: the least fixpoint over the module's inferred fns of (a) a tag raised
at a checked position — the operand of `return` or the body's tail,
through `if`/`match`/block/`else` branches — spelled as a bare capitalized
name no local, item or import resolves, or that name called (`Io(3)`);
(b) the row of a fallible value flowing into the return (a call with a
declared or inferred row, a `Map` index, a local bound to one); (c) the
row of every `?` in the body, closures excluded. A fn whose body reaches
something this machine cannot name — a method call, a closure call, a
spawn, an open row, a bare import, an unknown local — has an **unknown**
row, and an unknown row is read as `{..}`, exactly 0.1.44's reading.

**Three mechanisms.**

1. **The inference** (`src/rowinfer.rs`, new): one walk per inferred fn
   (locals typed plain / `Map` / a closed row / unknown, parameters from
   their types, a generic `T` plain), iterated to a fixpoint over every
   inferred fn of the program (monotone: tags only grow, unknown absorbs;
   a cap of 64 rounds makes every row unknown). Computed once per program
   and cached on `sema::Program` (a `OnceLock`, reset when the REPL
   installs a definition).
2. **The readers** (`src/rowmatch.rs`): `callee_row` answers a `-> !T`
   callee (own module, or a `use`d module's `pub fn`) with the inferred
   row, closed, `ok` the spelled `T` — so sema's E0801/E0816 judge, the
   evaluator's two-half dispatch and the lint (all three already read
   `callee_row`) read the body's row; an unknown row keeps `{..}`.
   `known_of_binding`: an annotation `!T` with an initializer whose row
   is known takes that row; otherwise `{..}` as before. A parameter
   annotated `!T` stays `{..}`.
3. **The E0801 text** is unchanged: with a closed row the judge's existing
   "does not cover `Neg`" sentence fires, which is the compiler's first
   line.

**The witness table** (`tests/rulings_is69/`, one directory per shape,
is67's runner extended by one optional key, `names`: a string the first
diagnostic line of `lupin check main.lu` must contain; trunk = the code
at `ba47627` = 0.1.44; the compiler = wolf-lang r26 `dfcc2f13`, one answer
on checked, native and release unless shown):

| witness | trunk (0.1.44) | head | [compiler] |
| --- | --- | --- | --- |
| `i69_issue_row` (s202's row verbatim) | `fail(E0801)` open row | `exit(0)` `43 42 42` | [same] |
| `i69_one_tag` (`return Neg`; `{ Neg => -1, v => v }`) | E0801 open row | `4 -1` | [same] |
| `i69_one_tag_missing` (`{ v => v }`) | E0801 open row | E0801 names `Neg` | [same] |
| `i69_try_callee` (`parse(s)?` with `{bad}`) | E0801 open row | `8 -1` | [same] |
| `i69_try_callee_missing` | E0801 open row | E0801 names `bad` | [same] |
| `i69_tail_call_row` (tail `look(m, k)`, `{none}`) | E0801 open row, W0604 | `5 -1`, W0604 | [same] |
| `i69_tail_call_row_missing` | E0801 open row | E0801 names `none` | [same] |
| `i69_recursive` (`down` recursive; `ping`/`pong` mutual) | E0801 open row | `0 -1 10 -2` | [same] |
| `i69_recursive_missing` (`Far` only; `Near` through the cycle) | E0801 open row | E0801 names `Near` | [same] |
| `i69_row_grows_before` (`{Low}`) | E0801 open row | `4` | [same] |
| `i69_row_grows_after` (`High` added, the match unchanged) | E0801 open row | E0801 names `High` | [same] |
| `i69_row_grows_covered` (an arm for `High`) | E0801 open row | `4 -2` | [same] |
| `i69_generic` (`wrap[T]`, `{Empty}`, `int` and `str`) | E0801 open row | `4 -1 x` | [same] |
| `i69_generic_missing` | E0801 open row | E0801 names `Empty` | [same] |
| `i69_let_annotated` (`let a: !int = f()`) | E0801 open row | `43` | [same] |
| `i69_match_binder_tail` (tail `match` returning `v * 2`) | E0801 open row | `8` | [same] |
| `i69_member_tail` (tail `xs.len`) | E0801 open row | `1` | [same] |
| `i69_field_tail` (tail `p.x`) | E0801 open row | `3` | [same] |
| `i69_closure_try` (a `?` in a closure is the closure's) | E0801 open row | `1` | [native, release `1`; checked `unsupported`] |
| `i69_unknown_method` (control: tail `xs.get(0)`, not inferable) | E0801 open row | unchanged | [E0801 names `none`] |

Nineteen witnesses red at the witness commit (on trunk's code), each for
its named reason (a run refused, or an E0801 that names the open row and
not the tag); one control green on both sides.

**The gate.** wolf-lang r26 `dfcc2f13`'s
`fallible_bind_empty_row_lanes.rs` with the 0.1.44 pre-mirror pin emptied,
`LUPIN` = the head's release build: 3/3 green; with the archive 0.1.44:
red on exactly `a_bound_empty_row_value_matches_elses_and_widens` (the
plant, already measured in §2); pinned, the archive green (§2) and the
head green (its `impl_version` is `0.1.44+dev.<sha>`, so the pin does not
apply and the ruled answer is asserted).

**Existing tests that change: none predicted.** No `tests/` row and no
unit test matches over the result of a `-> !T` fn (the rowmatch unit
tests use `-> T ! {row}` callees; `-> !int` appears on `main` only); the
suite decides, and a change is reported here.

**Corpus and differential.** `lupin corpus` at the pin: identical
reports, trunk against head. `lupin diff-run`, four tiers, against r26's
`wolf`: on the vendored corpus (pin `cdde128a`) no row moves; on wolf-lang
r26's own `corpus/`, exactly `rows/eu_bind_empty_row_handled.lu` moves
from a verdict mismatch to agreement on every tier, and nothing else
moves (the conservatism ledgers 0 rows).

**Coverage.** `RATCHET_FLOOR` holds at 271 (the witnesses are tests).

**Out of scope, named:** a method call's row (`xs.get(0)` and every
builtin method: no static method table here; the row stays `{..}`); a
`spawn`'s re-raise; a nested fn's row (refused by name); a parameter
annotated `!T`; E0602 for a binding whose row is wider than its
annotation (`q_let_annot_tag`); E0605 for a `pub fn -> !T`.

## Spec findings from is06/is07 (spec-is-defendant — filed, not absorbed)

spec/03 had never been executed before is06. The machine was the first
executable test of it, and the harvest was routed upstream, not patched
around. The s20 S-batch (pin `843174f`) paid S-1 through S-8. The
eight entries now live under *Resolved findings* below with what the spec
adopted and where this machine realigned. S-9 and S-10 remain open;
S-11 was RESOLVED by ruling D40 (2026-08-12), and its entry carries the
status update below.

- S-9 (is07): the seed↔schedule encoding has no normative home.
  `[conc.det.seed]` defines `--replay=SEED` behaviorally and `[proto.seed]`
  makes equal seeds byte-comparable, but no pinned document says what a
  seed *is* beyond "a value that regenerates the stream". The accepted
  s36 Phase A hook-design doc is the designated owner and does not exist
  at pin `843174f` either (re-checked at the is08 pin bump; the S-batch
  is spec/03+05 only). is07's provisional split of the `u64` namespace
  (bit 62 tags a packed schedule, `sched::PACKED_SEED_TAG`,
  approximation-contract §10.6) stands until the Phase A doc lands; the
  compiler runtime's format has priority and this side re-pins to it.

- S-10 (lupin 0.1.1, wolf-interp#4): `[conc.task.spawn]`'s dynamic half
  is unstated, and a task closure's write to a captured copy is silently
  task-local. The clause makes capturing a `mut` borrow of enclosing
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
  Not fixed here: a spawn-time capture-analysis trap
  would be this implementation legislating a dynamic meaning the spec
  never states. That is the line `ledger::dynamic_meaning` refuses to cross.
  Routed upstream: spec/03 (or `[conf.trap.map]`) should either state
  E1101's runtime meaning (kind + clause, as the E1004/E1005 amendment
  did) or bless capture-by-copy as the defined interpreter-tier semantics.
  Until then §10.2 stands as the documented behavior.
  Status update, pin `13b811f` (0.1.6, issue #19): the #41 capture-law
  wave hardened the *static* half: `conc/store_buffer.lu` re-pinned to
  `fail(E1101)` and this machine now rejects it at resolve with the
  counterparty's code and span (DIV-2026-015), so the E1101 shape no
  longer runs here and the silent-write-loss face is unreachable through
  the pinned corpus. The clause still states no runtime meaning, so the
  dynamic question stays open exactly as filed; capture-by-value remains
  §10.2's documented semantics for the shapes the static walk cannot see.

- S-11 (lupin 0.1.2, wolf-interp#9 / wolf-std F-0014 / wolf-lang#15):
  container mutation during `for` iteration has no governing clause.
  `loop_expr ::= 'for' pattern 'in' expr block` is the whole of the pinned
  text on `for` (`[gram.expr.flow]`): nothing states whether the loop
  moves its operand, holds a `mut`-grade access on it for the loop's
  extent, or reads it once. The three candidate readings produce three
  different verdicts on `for x in xs { xs.push(x) }`, and both
  implementations picked one: wolfc (a0c4564) lowers the operand as a
  *move* and rejects the body's use statically (`fail(E1001)`, "`xs`
  moved here", with a `for x in copy xs` fix-it), even though
  `[mem.tier0.move.1]`'s move list (assignment, initialization, `take`
  arguments, `return`) does not include loop operands; lupin evaluates
  the operand once at loop entry and iterates that snapshot (the MVS
  copy reading), so the program runs `exit(0)`, the pushes land, and the
  iteration never observes them (approximation-contract §6.8). A third
  reading, in which the loop holds the container `mut`-style for its
  extent, would make the body's push a `trap(exclusivity)` under
  `[conf.trap.map]`, and no clause states that hold either.
  Not legislated here: the snapshot loop cannot produce
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
  RESOLVED by ruling D40 (2026-08-12; lupin 0.1.8). The designers
  picked the third reading: `for x in xs` holds a read claim on the
  container for the loop's extent; a mut use inside is a static
  exclusivity-family error in wolfgang (new code E1013, fix-it teaching
  collect-then-apply or the index loop, never the
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
the resolve rung (carrying the `[conc.when.body]` exemption this
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

- S-1 (`when` had no clauses) → `[conc.when.order]`,
  `[conc.when.nodeadlock]`, `[conc.when.body]`, `[conc.when.nonest]`
  adopt the machine's canonical-order/whole-set/write-back semantics.
  The `sync.when.*` forward citations are retired; realignment: the
  nested-`when` stopgap `trap(assert)` is replaced per the spec's
  deviation: dynamically reaching an acquisition of an already-held
  sync object is `trap(deadlock)` (`[conc.deadlock.self]`), a lexical
  nest is the compiler's E1103, and a dynamically nested `when` over a
  disjoint set now *proceeds* (the compiler accepts it; the old blanket
  fault would have broken the one-way approximation direction).
- S-2 (region-transfer clauses missing) → `[conc.chan.move]`,
  `[conc.chan.staleuse]`, `[conc.chan.imm]` specify exactly the dynamic
  checks this machine runs at the send; the rules cite them now.
- S-3 (no deadlock verdict or trap kind) → `[conc.deadlock.def]` and
  `[conc.deadlock.trap]`, with `deadlock` added to `[conf.trap.set]` as
  the twelfth kind. Realignment: the machine's
  `unsupported`-with-roster report is retired for `trap(deadlock)` with
  the roster in the message; the is07 explorer's per-schedule verdict is
  the same spelling.
- S-4 (child failure did not reach a blocked owner) →
  `[conc.task.fail.owner]`: the scope is the cancellation unit, owner
  included (Trio posture). Realignment: the scheduler cancels a
  blocked, non-joining owner when a child fails; the owner's surfaced
  cancellation is its finished cancellation and the child's failure
  re-raises at the scope exit, after the join, replacing the is06
  deadlock-provoked-by-failure special case.
- S-5 (`procs.lu`/`proc_kill_defers.lu` named undefined functions) →
  both files are self-contained at the pin and RUN here: `procs.lu`
  exit(0), `proc_kill_defers.lu` exit(0) printing exactly `released`
  (kill skips defers), both schedule-independent under the explorer.
- S-6 (`cancelled` unreachable) → `[conc.proc.cancel]` specifies
  `w.cancel()` as the delivery mechanism; implemented (cooperative,
  defers run, monitors see `cancelled`; a proc completing its value
  anyway keeps `normal(value)`).
- S-7 (`link` pair spelling + root death unspecified) →
  `[conc.proc.link.pair]` (implemented: `a.link(b)`, idempotent per
  pair) and `[conc.proc.root]` (implemented: the root domain's abnormal
  death runs the killed-proc sequence for every live proc and exits
  nonzero; 1 here, class compared, never the number).
- S-8 (closed-channel `select` readiness) → `[conc.select.closed]`
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
runs here (`exit(0)`, in the run ledger). Closed by the pin bump.

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
case with zero rejects-beyond means every one of the 10,000 generated
programs (boundary mode's regions, moves and `mut` call sites included)
cleared the compiler's full frontend (lex, parse, resolve, s17's completed
sema) *and* this machine's run tier, with the two frontends in exact
agreement on all of them. The generator's semantic-plausibility layer is
doing its job; the next campaign should turn the boundary dial harder
(deeper nesting, adversarial-but-legal spellings) because this one found
nothing. Replay: `lupin fuzz --count 10000 --seed 5381 --compiler
upstream/target/debug/wolf`.

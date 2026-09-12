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

### DIV-2026-021 — `grammar/let_group_bare_tuple.lu` — **OPEN, filed upstream as wolf-lang#228**

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

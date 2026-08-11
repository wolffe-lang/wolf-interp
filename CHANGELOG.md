# Changelog

## 0.1.5 — 2026-08-11

The unsafe-tier batch (issue #18, the book's ch09 differential) and the
five-lane re-pin. Pin bumped `ad6cef7` → `f0da6e6` (s32 tasks, s33
channels, s37 str core, s38 fmt/io/fs, s67 warnings: the corpus grows
`strings/` ×9, `lints/` ×4, `fs/` ×2, `io/` ×1 — 183 → 199 files; the
registry grows the `diag.*` block, `[mem.str.get]` and
`[proto.record.warn]`/`[proto.cmp.warn]` — 290 → 303 anchors). One
issue closed (#18, six items), three divergences filed.

- **#18 (1) — the unsafe ring is enforced.** Raw-tier operations
  outside `unsafe` blocks reject at this machine's resolve rung with
  the counterparty's code and span: C calls (E1301 at the call —
  `[384,395]` on `unsafe_raw_outside.lu`, byte-identical), raw reads
  and writes through pointer-holding locals (the place, `[452,456]`
  shape), int→pointer casts, `borrow … from`, `assume noalias`, and
  the provenance operations. Sema-lite tracks what it can *see* —
  literal-bound locals, allocator calls, the book's laundered
  `unsafe { … }` initializer — and never guesses. Rung placement vs
  wolfc's mem emission is **DIV-2026-012** (the DIV-2026-011 question).
- **#18 (2) — nothing casts to `bool`.** The cast matrix's bool column
  closed: `n as bool` is E0805 at the whole cast expression, inside
  `unsafe` too (observed parity at the pin). §7/T1's one modelled
  production door closes with it — the T1 trigger/twin retired,
  `UbRow::T1` is `Coverage::Unreachable` with its reason, and the
  detection logic stays for frontend-bypassing callers. `bool as _`
  and `_ as str` reject statically where the class is visible
  (`typecheck/cast_bad.lu` now matches its pin). P1's protector-form
  suite pair retired with its `*u8` signatures; the protector
  acceptance evidence moved inline (machine-direct), and P2's
  trigger/twin rebuilt on `freeze r` — in-language, same row.
- **#18 (3) — `*T` never crosses a signature.** E1302 at the parameter
  name (`[329,330]` on `unsafe_sig.lu`, byte-identical), return types
  at the type span. Also DIV-2026-012.
- **#18 (4) — the C intrinsics check their arguments.** Exact arity
  for the modelled five; size/count arguments must be non-negative
  integers; `c.memset`'s byte argument no longer defaults silently —
  every refusal names the construct.
- **#18 (5) — the §7.4 format specs, to parity.** New `fmtspec`
  module: `[[fill]align][+][0][width][.precision][type]` — zero-pad
  AFTER the sign (`{n:08}` is the flag plus width; the absorb-into-
  width reading was the filed bug), `+` with zero taking it,
  sign-magnitude bases, `e`/`E` signed two-digit exponents, str
  precision on code-point boundaries, shortest-round-trip f64 default
  (the `std.fmt.decimal.to_str` layout; floats render `3`, not `3.0`).
  Malformed specs are **E0412** and type-mismatched specs **E0413**,
  statically at the literal where sema-lite sees the hole's class;
  E0411 statically refuses `s[i]` char indexing. The three corpus
  fail-files match their pins; wolfc's conform-run at this pin cannot
  reach its own emissions there — **DIV-2026-014**, counterparty
  suspected.
- **#18 (6) — the fs/io posture.** No filesystem by design: the s38
  `fs_*` family and `read_line` resolve and decline with the construct
  named. `eprint`/`eprint_raw` are real — one fmt machinery, two fds,
  stderr live-gated like stdout's pass-through and never hashed;
  `io/eprint.lu` runs to its pinned stdout. wolfc's conform-run at the
  pin E0301-rejects its own s38 files — **DIV-2026-013**.
- **Realignment:** the s37 str surface lands in full (`get` — the
  `[mem.str.get]` boundary primitive, oob = reversed = split-code-point
  = `none`, hits bit-identical to the checked slice — `find`/`rfind`,
  `bytes`, `split`/`count`/`replace`, `strip_prefix`/`strip_suffix`,
  `trim_start`/`trim_end`, `ends_with`, negative `repeat` trapping
  `bounds`), `^n` end-relative endpoints resolve before the domain
  question, and `[proto.record.warn]` is wire-complete: the additive
  `warnings` array (schema, comparison per `[proto.cmp.warn]`,
  honest-absent — this machine runs no warning analyses yet and says
  so by omission), the `warns:` corpus directive, and the `diag`
  anchor namespace. The lints tier runs warning-clean; spec/01 §9's
  bare `[diag]` heading token is exempted from the registry
  cross-check as a namespace, not a clause (routed upstream).
- Ninth differential: 181 entries, 18 members, **10 divergences, all
  filed** (011 open; 012 rung placement ×4; 013/014 the counterparty's
  conform-run surface lagging its own corpus at the pin), 289
  conservatism entries. Bundle: 235 programs, 217 records; coverage
  ratchet raised 86 → 90.

## 0.1.4 — 2026-08-10

The held maintenance wave, released once s30 shipped upstream. Pin
bumped `d147a54` → `ad6cef7` (s29+s30: the corpus grows
`memory/unsafe_c_alloc_native.lu`, `rows/eu_main_err_exit.lu`,
`typecheck/float_nan_cmp.lu` and `resolve/same_name/` — 177 → 183 files;
anchors hold at 290; the two E0410 fail-files re-pin `phase:` resolve →
parse). Four issues closed, one divergence resolved, one filed.

- **#15 (silent-wrong, ba:blocker) — the X1 call-site mode law has its
  missing half.** `f(x)` where the signature demands `f(mut x)` ran to a
  wrong answer silently (the writeback never happened) — the book caught
  it teaching chapter 7. E1007's static rule is now sema-lite's at the
  resolve rung, all four disagreement shapes (missing `mut`/`take`,
  extra mode, wrong word), matching wolfc's code, span, and message
  shapes, exactly as E0410 in 0.1.2; `[conf.trap.map]` gives E1007 no
  dynamic meaning, and a wrong answer is not a semantics candidate, so
  rejection is the honest stop. The dynamic residue — calls through
  function values — is refused at the call, never run wrong.
  `memory/mode_missing_mut.lu` leaves the run ledger; the book's ch07
  repro is a regression test. Rung placement vs wolfc's `mem` emission
  is **DIV-2026-011** (same code, same span; routed upstream).
- **#13 — `c.calloc(n, size)` allocates `n * size` bytes.** The modelled
  C heap gave it `n`; s29's native differential (real glibc) caught the
  disagreement — the first soundness candidate it produced, and a lupin
  bug. Overflow in the size computation is `unsupported` (real calloc
  says NULL; no null surface is pinned). `malloc`/`memset`/`memcpy`
  audited correct. `unsafe_c_alloc_native.lu` runs `exit(0)`.
- **#14 — integer literals consult their context.** `-9223372036854775808`
  is writable in every annotated spelling: literals stay unconstrained
  through negation and literal-only arithmetic (i128-checked), a
  declared return type types the value a call returns, and
  `[arith.literal.default]`'s i32 rule (with a range check) applies
  where the literal meets its binding. `var k = 0` remains i32 — the
  rule wolfc implements, now documented (approximation-contract §6.11).
- **#11 — closed after the sc04 reopen.** The 0.1.3 cast matrix holds at
  this pin: the reopening program prints `3.0` / `converts` and exits 0;
  `(3 as f64) == 3` is false. What the reopen's evidence showed the
  matrix still misses — cast target types are not *resolved* (`s as
  nonsense` no-ops) — is filed separately as #17.
- **DIV-2026-010 closed.** s29 moved wolfc's E0410 to the resolve rung
  (with the `[conc.when.body]` exemption this machine flagged as
  wolf-lang#21) and re-pinned the corpus directives; the eighth
  differential compares both files clean. Eighth round: 165 entries, 18
  members, **1 divergence** (DIV-2026-011, filed), 268 conservatism
  entries.
- Realignment: `float_nan_cmp.lu` (IEEE `!=` is unordered — this
  machine's f64 model already agreed; wolf-lang#22 was the compiler's
  half), `eu_main_err_exit.lu` (`error: Boom` + exit 1, agreeing with
  the native shim bit-for-bit), `resolve/same_name/` (two `len`s stay
  distinct). Bundle: 222 programs, 204 records; coverage ratchet raised
  84 → 86.

## 0.1.3 — 2026-08-10

The rows half, and the s27/s28 catch-up. Pin bumped `a0c4564` →
`d147a54` (the corpus grows `rows/qmark_defer.lu` and
`faults/assert_msg_holds.lu`: 175 → 177 files; the spec grows
`[mem.iter.*]`, `[mem.str.*]`, `[conf.trap.assert]` and the postfix-row
type grammar: 281 → 290 anchors). Three issues closed, one divergence
resolved, one filed.

- **#12 — postfix rows, all three halves.** `type ::= type '!' error_row`
  parses in **every** type position (param, `let`/`var` annotation,
  nested); a bare lowercase name at a raise site resolves against the
  enclosing function's declared return row (`return none` under
  `-> int ! {none}` raises the tag); and resolution is **eager** — sema's
  `raise_check` refuses an unresolvable tag at the resolve rung whatever
  path the input takes, so the sc02 false-certification trap
  (`unsupported` only when the raise was *hit*) is structurally closed.
  Lowercase identifiers over tag-shaped scrutinees dispatch as row-tag
  patterns when they name a module-declared row tag; `else |err|` keeps
  its binder. The acceptance: wolf-std `std.option`'s six helpers — `or`,
  `expect`, `flatten`, `to_list`, `exists`, `is_none`, the F-0002 family,
  unwritable since sc01 — execute under lupin, lowercase `none` included
  (`tests/rows_option.rs`).
- **#11 (silent-wrong) — numeric casts convert.** `n as f64` produced an
  int that compared equal to ints and unequal to the float it claimed to
  be. `as` between numeric types now converts in every direction:
  int→float exact, float→int truncating toward zero with an X3 range
  check (NaN/∞/out-of-range trap `overflow`), int→int narrowing
  range-checks, `wrapping[T]`/`saturating[T]` targets reduce by their
  mode, `as f32` rounds through f32 precision (the one-f64 float model,
  approximation-contract §6.9). The non-bridges refuse like wolfc's
  E0805 (no truthiness, no `int as str`). `tests/cast_matrix.rs` pins
  the matrix, both directions of every pair.
- **#10 — slice-of-binding receivers.** `d[0..1].upper()` refused at
  resolve (`d["0..1"]` does not denote a place) because the range key was
  stringified into a map-key projection. A slice expression is a value,
  not a place: `place_of` refuses it, the method call falls into the
  by-value receiver path, and `binding[range].method()` runs exactly like
  `literal[range].method()` always did.
- **The s27 spec realignments.** `[mem.iter.for]`: `for` over an
  `impl Iter for T` value desugars to the clause's drive loop
  (`next(mut self) -> T ! {done}` through call-by-value-result; range-for
  unchanged) — impl-block **method dispatch** lands with it, s17
  resolution order included (inherent wins; `Speak.speak(d)` reaches the
  shadowed trait method; trait default bodies stay `unsupported`).
  `[conf.trap.assert]`: `assert` is an intrinsic — never shadowed by a
  module fn, two-arg form's message evaluated **only** on the failing
  path, rendered to stdout before the trap (the counterparty's #19
  shape, from this side). `[mem.str.order]`: the executed byte-
  lexicographic ordering is now clause-backed and witness-tested. Twelve
  more corpus entries reach the run rung than at 0.1.2 (114 of 161;
  matches 76 → 84, out-of-scope 46 → 36, 0 mismatch).
- **DIV-2026-010 re-verified: still open at this pin.** A CLEAN wolfc
  build at `d147a54` reports `fail(E0410)@typecheck` where the corpus
  pins `phase: resolve` — the sixth round's two divergences stand
  unchanged, filed and non-gating. The fix is in flight upstream (s29's
  resolve-rung `letcheck`, landed past this pin with CI still running at
  the close of this pass);
  wolf-lang#21 carries this machine's heads-up that the new walker must
  exempt `when`-body assignments per `[conc.when.body]` (its draft
  E0410-rejects `when (a, b) { a += 10 }` on `let`-bound Mutex operands
  that `conc/when_multi.lu` and `procs.lu` pin `run(exit=0)`). The
  seventh corpus differential is GREEN-with-filing: 161 entries, 2
  divergences, both DIV-2026-010.
- Bundle: 216 programs, 200 records, 290 anchors, ratchet floor 84
  (coverage +1: `[mem.model.order]` through `qmark_defer`'s own
  `conforms:` line; the nine new s27 anchors enter the debt list —
  their behaviors are exercised in `tests/`, and lifting them into
  `conforms:`-tagged suite programs is the next bundle's work).

## 0.1.2 — 2026-08-10

The lupin maintenance pass: five filed issues, four fixed, one routed
upstream. Pin bumped `cbde620` → `a0c4564` (the corpus grows the E0410
fail-files and the unsafe/checked memory tier: 164 → 175 files).

- **#5 (silent-wrong) — bare-ident match patterns dispatch.** An
  identifier that names an in-scope enum variant is a *variant pattern*
  (matching the tag spelled bare or enum-qualified, payload half
  included); a capitalized identifier over a tag-shaped scrutinee is a
  structural row-tag pattern (D30); everything else binds, including a
  capitalized name over a non-error scrutinee, which is the
  counterparty's reading too. First-arm-always is dead:
  `match Ordering.Greater { Less => 1, Equal => 2, Greater => 3 }`
  yields 3, `corpus/typecheck/match_missing.lu` moves to its honest
  `exit(1)` in the run ledger, and a match no arm of which applies is
  `unsupported`, never a wrong answer. The out-of-grammar mirror image —
  bare *dotted* path patterns (`Ordering.Less =>`) — now rejects at parse
  with the counterparty's exact E0201 shape (zero-width span at the token
  after the path). Approximation-contract §6.7 records the dynamic
  approximation. Same-scope `let` shadowing also reads the *latest*
  binding now (the `rposition` repair) — `let_shadow_var_ok.lu`'s
  pinned `exit(0)`.
- **#7 (false UB) — both `ub(mem.ub)` shapes of wolf-std F-0013 were one
  defect in `Provenance::drop_frame`:** it still parsed the pre-task
  `<frame>:<path>` place-key shape (keys are `t<task>:<frame>:<path>`
  since is06) and so dropped *nothing* — a callee's parameter binding
  survived its call, and the next call reusing that frame index and
  parameter name resolved its accesses through the stale tag: a Disabled
  read (shape a, the interpolated `mut` argument) or a protected-sibling
  foreign write (shape b, the allocating read-mode call). `drop_frame`
  now takes `(task, frame)` and forgets that task's frame and deeper,
  exactly. Both filed shapes are staged as regression tests
  (`tests/std_root.rs`), the transition table is untouched, and the ub
  matrix + ok-twins stay green — no true detection weakened.
- **#8 — `let` reassignment rejects at the resolve rung.** Sema-lite
  tracks binding mutability (params and pattern bindings are the mode
  system's business; `when` bodies assign through the acquired cell;
  shadowing rebinds): plain and compound assignment to a `let`-bound
  name is **E0410** with a `var` fix-it, span on the assigned place —
  byte-identical code+span to wolfc on the pin's fail-files. The
  interpreter half of wolf-lang#2, closing the last half of the
  divergence.
- **#6 — lupin has a std root.** `--std-root DIR` on `run`/`check`/
  `conform-run`, `LUPIN_STD` as the flagless spelling: `use std.X[.Y]`
  resolves `<DIR>/X[/Y]/` through the normal loader (nested paths
  included — `std/x/deque_int` ships at that depth), the path's last
  segment is the bound name, and without a root the flat
  `<package root>/<last segment>` fallback keeps mirrors and sibling
  modules working. Mirrors wolfc's s26 `--std-root`/`WOLF_STD`; the
  wolf-std rig's flat-mirror interim can retire.
- **#9 — mutation during `for` iteration: routed upstream, not
  legislated.** The pinned spec says nothing about `for`'s operand —
  no move, no extent-hold, no copy — and the implementations picked
  different readings (wolfc: E1001 static move; lupin: loop-entry
  snapshot, runs). Filed as **S-11** in the divergence log with both
  behaviors and the three candidate rulings; approximation-contract §6.8
  documents the machine's snapshot semantics. Compiler half wolf-lang#15;
  wolf-std keeps the divergence visible in CI.
- Conformance bundle: 214 programs / 198 records at pin `a0c4564`;
  anchor ratchet 79 → 83; manual, export ledger and corpus-harness
  ledger updated in the same breath.

## 0.1.1 — 2026-08-10

The bs00x maintenance pass: four filed issues, three fixed, one routed
upstream.

- **#1 (top severity) — the DPOR closed-frontier miss is fixed.** Two
  scheduler defects starved the explorer's backtrack sets: a send that
  committed a `select` arm consumed the selecter's registration on every
  channel of the select but recorded only the sent-on channel (the
  conflict alphabet missed the coupling — `op_select_consume` is the fix),
  and channel sends published the sender's vector clock without ticking it
  past the snapshot (spawn and mutex release already ticked), so the
  sender's later ops carried false happens-before edges. The filed
  reproducer (wolf-book ex17-1, the lost-update server) now shows both
  outcomes — `balance=50` and `balance=100` — inside a *closed* frontier,
  identical to naive DFS, and is pinned as a regression litmus
  (`tests/explore_machine.rs::the_select_coupled_lost_update_is_inside_the_closed_frontier`).
  The pinned `conc/` exploration ledger re-ran with **no count movement**
  and every verdict-stability oracle still green.
- **#2 — write-after-freeze now traps on value paths.** Struct values
  carry the region charged at their allocation site
  (`Value::Struct::home`), and `write_path` refuses a write through a
  container homed in a `Frozen` region: `region-fault
  [mem.region.freeze.1]`, before anything is mutated — E1012's shape
  executed, agreeing with wolfc's static rejection. Reads and rebinding
  stay legal (`tests/faults/region_freeze_value_write.lu` + its
  `region_freeze_rebind_ok.lu` twin; approximation-contract §6.1 records
  the remaining list/map gap). `memory/region_freeze_write.lu` moves from
  `exit(7)` to `trap(region-fault)` in the run ledger.
- **#3 — `when`-arity code aligned to wolfc.** The malformed-`when`
  sentence now carries **E0201** (the established generic expected-token
  assignment) instead of the invented E0203, which collides with wolfc's
  toplevel-decl family. Message and `[gram.expr.conc]` anchor unchanged;
  `diag::UNPINNED_CODES` (the published choices table) updated.
- **#4 — cross-task capture-by-copy: routed upstream, not patched.**
  spec/03 makes the mut-capture shape a compile error (E1101) and states
  no runtime meaning; inventing a spawn-time trap would be legislating.
  Capture-by-copy stays the documented interpreter semantics
  (approximation-contract §10.2, corpus exemplar `conc/store_buffer.lu`),
  and the gap is filed as **S-10** in the divergence log for a spec ruling
  (the E1004/E1005 precedent).
- Conformance bundle: 203 programs / 187 records (the two new freeze
  litmuses); manual and export ledger updated in the same breath.

## 0.1.0 — 2026-08-10

The binary is named `lupin`; the package and repository stay `wolf-interp`.

- `lupin FILE.lu` runs a file; `lupin -` reads a program from stdin; bare
  `lupin` opens the REPL. The exit code reports the program: its own
  `exit(N)`, 2 on a static-phase rejection, 3 on a trap or UB finding, 4 on
  `unsupported`.
- `run` is the explicit subcommand spelling and carries `--seed`,
  `--schedule` and `--json` (the spec/06 observation record at the front
  door). A subcommand name wins over a file of the same name.
- `lupin eval 'CODE'` (short spelling `-e`) evaluates a snippet in a fresh
  session and prints what the REPL would print.
- `lupin check FILE...` runs the frontend only (lex, parse, resolve).
- Observation records and bundle manifests report `"impl": "lupin"` with
  `impl_version` 0.1.0; `--version` prints the version, the package, and
  the upstream pin.

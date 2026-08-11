# Changelog

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

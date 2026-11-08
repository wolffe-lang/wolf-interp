# s285 — lupin mirrors the literal-pattern rule: the overflow trap at the arm, E0207 for a signed literal

The compiler now refuses a literal pattern its scrutinee's type cannot hold: `match v { 300 => … }`
with `v: u8` is E0415 at the `300` on the checked machine, native and release (wolf-lang#1071,
`[gram.pat.lit]`, for wolf-lang#1058). Before, native wrapped the arm into eight bits and matched
44. This machine compared the number and never matched, so it ran the program down the fallback
arm. It has no checker, so it answers E0415 as it answers every other E0415, with the dynamic
counterpart: when `match` tests the arm, the run traps `overflow` at the literal, naming the
scrutinee's type, the clause and the code. A range's ends are literal patterns too. A program stops
at its first trap, which is the compiler's first E0415. A signed literal pattern, which the grammar
does not admit and which this machine read as the negated literal and ran, is now the compiler's
static E0207, "a literal pattern has no sign", at the signed literal.

## Forbidden

Footprint: module

The diff is `src/` (the parser's two pattern positions in `parse.rs`, and the literal and range
arms of `match_pattern` in `eval/mod.rs`), the mirror rows and their fixtures, the changelog and
notes: one repository's program, with no CI, pin, spec or vendored-tree change. Clean room: the
spec at this pin, the compiler's corpus rows and records as black-box answers (`wolf conform-run
--json` on the three compiled tiers), and lupin's own source. No label, merge, tag, release, issue
filed or closed, and no attribution trailer. The plant is pushed without a PR. Nothing ran under
`/tmp` or `/var/folders`. No commit was made on the Linux host.

## Inputs, verified

- Base: trunk `17bd522`, at the upstream pin `5f1f1c94` (wolf 0.2.38's conformance pin).
- The compiler's change: wolf-lang#1071, merged at trunk `d848fbed`, its seven rows
  (`corpus/typecheck/numlit_pattern_*.lu`, `corpus/grammar/match_pattern_signed_literal.lu`,
  `corpus/rows/match_literal_pattern_edges.lu`) and their records on the checked machine, native
  and release, with its Linux gauntlet record (`worst=0 steps=35`) and its plant. The seven
  fixtures here are those rows byte for byte at `d848fbed` (sha256 read on both sides).
- lupin 0.1.60, the release archives (`aarch64-apple-darwin` sha256 `f3cae16f…6bb8`,
  `x86_64-unknown-linux-gnu` `e7d4c4f1…637e`, each read twice and equal to the release page),
  answers the five E0415 rows with the never-matching exit, runs the signed row (`exit(2)`; it
  matches `-1` against an `i8` holding -1), and answers the edge row correctly.
- The machine: `match_pattern`'s literal arm compared `Value::Int(a, _)` with `Value::Int(b, _)`,
  ignoring the scrutinee's `IntTy`; the range arm likewise. The parser read `-` and a literal in
  pattern position as the negated literal (`parse_literal_only`), at a pattern's start and at a
  range's high end. The record carries the first diagnostic only (`[proto.record.first]`), and a
  trap's words live in the observation, never on the wire (`[proto.record.trap]`).
- README read, current: it states nothing about patterns or these refusals.

## Prediction

Committed before the first change (`5548c76`, `notes/s285-prediction.md`). Scored:

- **The base: hit.** At the rows' commit (`4ba74a6`, trunk's code) the five E0415 shapes run and
  never match (`exit(0)` or `exit(2)`, by the row's fallback), the signed shape runs (`exit(2)`:
  `-1` over an `i8` holding -1 matched), and the edge shape is right.
- **The rows red first: hit.** `mac-interp-red-4ba74a6.log`: 1 passed, 6 failed. The one that
  passed is the edge row, `every_primitives_edge_literal_still_matches`. The six that failed
  are the five trap rows (`an_arm_literal_outside_a_u8_traps_at_the_literal`,
  `the_first_primitive_past_its_maximum_traps`, `a_nested_literal_outside_its_type_traps`,
  `a_range_end_outside_its_type_traps`, `a_literal_scrutinee_holds_its_pattern_to_i32`, each
  `exit(…)` where `trap(overflow)` is due) and `a_signed_literal_pattern_is_refused_by_name`
  (`exit(2)` where `fail(E0207)` is due).
- **1, the arm traps: hit.** `trap(overflow)` at the literal on every trap shape at the first
  fix (`mac-interp-fix-6ff573e.log`). The trap's words name the scrutinee's type,
  `[gram.pat.lit]` and E0415; the rows check them from `4a6a9ab` on, in the takes at the head.
  The value's `IntTy` was enough: no type had to be carried that the machine did not already
  carry.
- **2, the first trap is the compiler's first E0415: hit,** at `537..540`, `702..705`, `714..717`,
  `749..752` and `627..637`, the spans the compiler reports first.
- **3, E0207 at the compiler's span: hit,** `fail(E0207)` at `607..609`, the first diagnostic,
  "a literal pattern has no sign". The diagnostic cites `[gram.pat]` as its anchor, because this
  pin's spec has no `[gram.pat.lit]` yet; its help names `[gram.pat.lit]` in prose.
- **4, the edge row: hit,** unchanged, `exit(0)` and the compiler's stdout.
- **The first fix take: two misses, in the rows and the format, none in the machine.** At
  `6ff573e` the rows went 2 passed (the signed and edge rows), 5 failed: each trap row's verdict
  and span held, and each failed at its next check, "the trap message lacks" the type. The rows
  read the trap's words from the record's `trap_message`, which carries only the program's own
  message (`[proto.record.trap]`); `4a6a9ab` reads them from the observation. `cargo fmt` wanted
  one call split (`e0b8369`).
- **What must not move: hit.** At `6ff573e` the library tests (796), the s282 binder rows (9),
  the corpus harness (10), conformance (10) and divergence (6) pass. `run_corpus` passed 15 of
  its 16 tests, `no_corpus_file_mismatches_its_expectation` and
  `every_run_expectation_this_machine_reaches_is_met_exactly` among them, so no vendored program
  traps where it did not, the `i32` reading of a literal scrutinee included. I stopped the
  sixteenth, `evaluation_is_deterministic_over_the_corpus` (two serial walks of the corpus, in a
  debug build), after 84 minutes; the Linux gauntlet's `heavy-run_corpus` runs all sixteen at the
  head.

## Evidence index

`notes/s285-evidence/index.txt` lists every file:
- `mac-interp-red-4ba74a6.log`: the rows at trunk's code, red by name (1 passed, 6 failed);
- `mac-interp-fix-6ff573e.log`: the first fix take: the rows (the message check red), fmt
  (red), clippy, the library tests, the s282 binder rows, the corpus harness, the corpus run,
  conformance and divergence;
- `rig/`, `rig/linux/`: the scripts as run, the Linux gauntlet's driver and steps among them.

Three takes live outside the tree and the PR body quotes every `END` line of each: the Linux
gauntlet record at the final head and the plant record on it (a record cannot predate the head
it names), and a Mac take at `e04b5fe`, the code head (the rows, fmt, clippy, the library tests,
the binder rows, the corpus harness, conformance), which ran after this note was written.

## Done-when

Commits:
- `5548c76`: the prediction, before the first change.
- `2b3bc9b`: the seven fixtures, the compiler's rows byte for byte. `4ba74a6`: the rows, red first.
- `efa144e`: the parser refuses a signed literal pattern by name (E0207); `d0928ff` keeps
  `expected_pattern`'s doc comment on it.
- `ce076dc`: `match_pattern` traps a literal pattern, and a range's end, its scrutinee's type
  cannot hold.
- `6ff573e`: the changelog.
- `4a6a9ab`: the rows read the trap's words from the observation. `e0b8369`: formatting.
  `e04b5fe`: the rows name the trunk commit that carries them.
- This note and its evidence.

Test targets:
- [x] the prediction committed before the first change (`5548c76`)
- [x] the rows red at trunk's code, by name; the one passing row is the edge row
      (`mac-interp-red-4ba74a6.log`)
- [x] at the first fix: clippy (all targets, `-D warnings`), the library tests, the binder rows,
      the corpus harness (`mac-interp-fix-6ff573e.log`)
- [x] the corpus run (15 of 16, the sixteenth stopped and left to the Linux record),
      conformance and divergence (`mac-interp-fix-6ff573e.log`)
- [ ] the rows green at the code head, the spans exact, with fmt and clippy, on the Mac (in the
      PR body)
- [ ] the Linux gauntlet record at the final head, every `END … exit=0` (in the PR body)
- [ ] the plant red by name (in the PR body)
- [ ] the audit, once

Follow-on, decided: wolf-lang#1071 stays as it merged. Its lupin half
(`crates/wolf_driver/tests/literal_pattern_lanes.rs`) holds lupin to the answers of the release it
pairs with: the never-matching exit on the five E0415 rows and the run on the signed row. When
wolf-lang next moves its lupin pairing to a release that carries this change, that move should
teach those rows this machine's new answers: the `overflow` trap at the compiler's first E0415
span, and E0207 at the first signed literal only, since a record carries one diagnostic. This is
the shape of wolf-interp#343, which names what the next pin advance owes the binder and `select`
rows.

In the other direction, when this repository's pin next advances past `d848fbed`, the vendored rows
are these fixtures byte for byte, so the advance should move no answer here, only ledger them. Six
reach run on this machine and join the run ledger of `tests/run_corpus.rs`: the edge row, matching
its `check:`, and the five E0415 rows, trapping `overflow`, the conservatism class that ledger
already names for `typecheck/numlit_binding_literal*.lu`. The signed row is one more E0207 among
the pinned grammar-tier codes of `tests/conformance.rs`.

Issues: wolf-interp#349 (a signed literal pattern accepted and matched) is the parser half of this
change; it stays open for the maintainer. wolf-interp#350 (an integer literal adopting a
`distinct` integer type) is untouched here.

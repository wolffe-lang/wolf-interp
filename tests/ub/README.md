# UB triggers, and their near-miss defined twins

One program per row of `[mem.ub]`'s **closed** enumeration that this tier can
reach, written in the pinned corpus's own directive dialect
(`[conf.directive]`) so they are **upstream-ready as-is**: dropping a file into
`wolf-lang/corpus/ub/` should need no edits.

They live here rather than in `upstream/corpus` for the reason
`tests/faults/README.md` gives — the independence rule forbids writing into the
pinned tree, and "authored here and upstreamed" has a second step that is a
wolf-lang commit, not a wolf-interp one.

## Triggers

Each pins `run(exit=trap(ub))`. This implementation answers with the *verdict*
`ub(mem.ub)` and the row on `x-ub-row`; the two are one event seen through two
lenses, and `ledger::ub_is_the_oracle_verdict` is where that is argued.

| row | file | what it does | licenses |
|---|---|---|---|
| P1 | `p1_protector.lu` | a foreign write during a protected call extent — the canonical retag-then-opaque-call program | O1 `noalias` on `mut` params |
| P1 | *(corpus)* `memory/unsafe_ub_uaf.lu` | use-after-free through a freed C allocation | O1 |
| P2 | `p2_frozen_write.lu` | a write through a `read` parameter's Frozen tag | O2 load hoisting (the SB "holy grail") |
| P3 | `p3_out_of_bounds.lu` | byte 16 of an 8-byte allocation | O3a `dereferenceable(n)` |
| P4 | `p4_region_freed.lu` | a pointer that outlives the region owning its memory | O3b per-region alias domains, O4 `invariant.load` |
| P5 | `p5_false_noalias.lu` | `assume noalias p, q` where `q` is a copy of `p` | O5 asserted-disjoint vectorization |
| P6 | `p6_false_door.lu` | `borrow r from ptr` where `ptr` is not in `r`'s footprint | O6 the door concentrates trust |
| L1 | `l1_uninitialized.lu` | a read of `malloc`'d storage nothing wrote | O7 no zero-init of locals |
| L2 | `l2_dangling.lu` | an int→ptr round trip across a `free` | O8 escape analysis without pinning addresses |
| T1 | `t1_invalid_bool.lu` | `7 as bool` in unsafe code | O9 niche packing, default-free jump tables |

Two rows are **not** here, and `UbRow::coverage` says why rather than leaving
them absent: **T2** (a torn write) is unreachable in a single-threaded machine
whose every store is whole-value, and **C1** (a data race) is
`deferred(concurrency)` to ic03.

## Near-miss twins — `ok/`

Each trigger has a twin in `ok/` pinned at `run(exit=0)`: the *same* machinery,
one property changed. A machine that reported UB on everything would satisfy
"the triggers trigger"; only the twin shows the check is discriminating.
`tests/ub_coverage.rs` fails the build for a row with a trigger and no twin.

## Tests

- `tests/ub_coverage.rs` — the D2 coverage matrix. Every row is detected and
  paired, or listed as deferred with a reason; a **planted** uncovered row is
  shown to fail the gate.
- `tests/prov_machine.rs` — the report each trigger renders, snapshot-tested:
  the row, both spans, the borrow-tree slice, and the optimization the row
  licenses.

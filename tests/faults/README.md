# Fault-class programs, and their near-miss twins

One program per trap identity this evaluator can raise, written in the pinned
corpus's own directive dialect (`[conf.directive]`) so they are **upstream-ready
as-is**: dropping a file into `wolf-lang/corpus/memory/` should need no edits.

They live here rather than in `upstream/corpus` because the independence rule
this repo runs under forbids writing into the pinned tree — `spec/` + `corpus/`
are read-only inputs, and the sprint's "authored here and upstreamed" is a
two-step process whose second step is a wolf-lang commit, not a wolf-interp one.

> **Dedupe status at pin `ecea37c`:** s16's `corpus/faults/` tier landed, and it
> is six of these programs upstreamed. The rule this file wrote in advance now
> applies: **the vendored copies are the source of truth**, so
> `assert_fails.lu`, `bounds_slice.lu`, `div_zero_rem.lu`,
> `exclusivity_nested_path.lu`, `overflow_add.lu` and `use_after_move_field.lu`
> have been deleted from this directory. They are still snapshot-tested —
> `tests/fault_snapshots.rs` walks *both* directories, and asserts that no
> program exists in both, so a future re-add is a build failure rather than a
> silently duplicated snapshot. The upstream copies differ from what was sent
> only in their `phase:` ledger, which is the compiler's rung to state.
>
> The programs with no upstream twin stay: every `region-fault` and
> `stale-handle` case below, which the corpus still has no counterpart for.

## Faults

The closed trap vocabulary has eleven kinds (`[conf.trap.set]`). **Eight** are
reachable at is03 — the dynamic region machine added the last two families.

| kind | file | clause |
|---|---|---|
| `overflow` | `corpus/faults/overflow_add.lu` † | `arith.checked`, `[mem.ub.defined]` |
| `div-zero` | `corpus/faults/div_zero_rem.lu` † | `[mem.ub.defined]` |
| `bounds` | `corpus/faults/bounds_slice.lu` † | `[mem.ub.defined]` |
| `use-after-move` | `corpus/faults/use_after_move_field.lu` † | `[mem.tier0.move.2]` |
| `use-after-move` | `handle_uninit.lu` | `[mem.shared.handle.1]` (a reserved, never-`init`ed slot *is* uninitialized storage) |
| `exclusivity` | `corpus/faults/exclusivity_nested_path.lu` † | `[mem.tier0.excl.1]`, `[mem.model.path.disjoint]` |
| `region-fault` | `region_uaf.lu` | `[mem.region.intra.2]` |
| `region-fault` | `region_edge_cross.lu` | `[mem.region.edge]` (E1004's dynamic half) |
| `region-fault` | `region_freeze_write.lu` | `[mem.region.freeze.1]` |
| `region-fault` | `region_freeze_value_write.lu` | `[mem.region.freeze.1]` (the value-path half — E1012's shape, executed; wolf-interp#2) |
| `region-fault` | `region_suspended_write.lu` | `[mem.region.open.3]` |
| `region-fault` | `region_move_open.lu` | `[mem.region.freeze.3]` (E1005's dynamic half) |
| `region-fault` | `region_multiopen_nested.lu` | `[mem.region.multiopen]` |
| `stale-handle` | `handle_stale_reuse.lu` | `[mem.shared.handle.2]` |
| `assert` | `corpus/faults/assert_fails.lu` † | `[conf.trap.map]` |

† Upstreamed at pin `ecea37c` and retired from this directory; the vendored copy
is the one the tests read.

The remaining three belong to tiers is03 did not implement: `alloc-contract`
(I15's `#[noalloc]` family) and `race` (ic03) are still absent, and `ub` left
this list **sideways** at is04. The oracle's finding is the protocol *verdict*
`ub(anchor)`, not a `Trap` — `[proto.record.verdict]` gives it its own shape —
so no program here raises `TrapKind::Ub` and none should. The kind stays in the
closed vocabulary for the checked build (`[conf.trap.map]`); its programs live
in `tests/ub/`, and `tests/ub_coverage.rs` is their gate.

## Near-miss twins — `ok/`

Each fault class has a **legal twin** in `ok/`: the same machinery exercised
without faulting, pinned at `run(exit=0)`. A machine that faults on everything
satisfies "the fault programs fault"; only the twin shows the check is
*discriminating*.

| twin | proves |
|---|---|
| `region_edge_intra_ok.lu` | intra-region back-edges are safe (`[mem.region.intra.1]`) — the cross-region twin of `region_edge_cross.lu` |
| `region_freeze_read_ok.lu` | frozen data reads from anywhere, forever (`[mem.region.edge.imm]`) |
| `region_freeze_method_read_ok.lu` | a read-only method through a frozen container is a read, not a write-back (`[mem.region.freeze.4]` — the issue #20 regression, the book's ch10 shape) |
| `region_freeze_rebind_ok.lu` | rebinding the binding that held a frozen value is legal; only writes *through* the value fault |
| `region_reopen_ok.lu` | re-entering an already-open region is a no-op |
| `region_transfer_closed_ok.lu` | a **closed** subtree transfers, establishing one owning edge |
| `handle_reserve_init_ok.lu` | reserve → init → read, and a reused slot's fresh handle is live |
| `shared_weak_dead_ok.lu` | a dead `weak` upgrades to the option-shaped result, not a fault |

## Tests

- `tests/fault_snapshots.rs` snapshot-tests each fault program's rendering:
  trap kind, both spans, and the clause anchor.
- `tests/region_machine.rs` runs the twins, checks every fault class has one,
  and asserts the leak and forest invariants over the whole corpus.

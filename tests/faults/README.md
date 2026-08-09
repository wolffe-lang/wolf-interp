# Fault-class programs, and their near-miss twins

One program per trap identity this evaluator can raise, written in the pinned
corpus's own directive dialect (`[conf.directive]`) so they are **upstream-ready
as-is**: dropping a file into `wolf-lang/corpus/memory/` should need no edits.

They live here rather than in `upstream/corpus` because the independence rule
this repo runs under forbids writing into the pinned tree — `spec/` + `corpus/`
are read-only inputs, and the sprint's "authored here and upstreamed" is a
two-step process whose second step is a wolf-lang commit, not a wolf-interp one.

> **Dedupe status at pin `bd41920`:** s16's `corpus/faults/` tier had not landed
> upstream, so none of these programs is duplicated there and no retirement was
> owed. When it lands, the vendored copies become the source of truth and the
> overlapping files here retire; the ones with no upstream twin stay.

## Faults

The closed trap vocabulary has eleven kinds (`[conf.trap.set]`). **Eight** are
reachable at is03 — the dynamic region machine added the last two families.

| kind | file | clause |
|---|---|---|
| `overflow` | `overflow_add.lu` | `arith.checked`, `[mem.ub.defined]` |
| `div-zero` | `div_zero_rem.lu` | `[mem.ub.defined]` |
| `bounds` | `bounds_slice.lu` | `[mem.ub.defined]` |
| `use-after-move` | `use_after_move_field.lu` | `[mem.tier0.move.2]` |
| `use-after-move` | `handle_uninit.lu` | `[mem.shared.handle.1]` (a reserved, never-`init`ed slot *is* uninitialized storage) |
| `exclusivity` | `exclusivity_nested_path.lu` | `[mem.tier0.excl.1]`, `[mem.model.path.disjoint]` |
| `region-fault` | `region_uaf.lu` | `[mem.region.intra.2]` |
| `region-fault` | `region_edge_cross.lu` | `[mem.region.edge]` (E1004's dynamic half) |
| `region-fault` | `region_freeze_write.lu` | `[mem.region.freeze.1]` |
| `region-fault` | `region_suspended_write.lu` | `[mem.region.open.3]` |
| `region-fault` | `region_move_open.lu` | `[mem.region.freeze.3]` (E1005's dynamic half) |
| `region-fault` | `region_multiopen_nested.lu` | `[mem.region.multiopen]` |
| `stale-handle` | `handle_stale_reuse.lu` | `[mem.shared.handle.2]` |
| `assert` | `assert_fails.lu` | `[conf.trap.map]` |

The remaining three belong to tiers this sprint does not implement:
`alloc-contract` (I15's `#[noalloc]` family), `race` (ic03), `ub` (is04's
oracle). Their programs are those sprints' to author — writing them here would
mean authoring expectations for machinery that does not exist.

## Near-miss twins — `ok/`

Each fault class has a **legal twin** in `ok/`: the same machinery exercised
without faulting, pinned at `run(exit=0)`. A machine that faults on everything
satisfies "the fault programs fault"; only the twin shows the check is
*discriminating*.

| twin | proves |
|---|---|
| `region_edge_intra_ok.lu` | intra-region back-edges are safe (`[mem.region.intra.1]`) — the cross-region twin of `region_edge_cross.lu` |
| `region_freeze_read_ok.lu` | frozen data reads from anywhere, forever (`[mem.region.edge.imm]`) |
| `region_reopen_ok.lu` | re-entering an already-open region is a no-op |
| `region_transfer_closed_ok.lu` | a **closed** subtree transfers, establishing one owning edge |
| `handle_reserve_init_ok.lu` | reserve → init → read, and a reused slot's fresh handle is live |
| `shared_weak_dead_ok.lu` | a dead `weak` upgrades to the option-shaped result, not a fault |

## Tests

- `tests/fault_snapshots.rs` snapshot-tests each fault program's rendering:
  trap kind, both spans, and the clause anchor.
- `tests/region_machine.rs` runs the twins, checks every fault class has one,
  and asserts the leak and forest invariants over the whole corpus.

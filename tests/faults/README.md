# Tier-0 fault-class programs

One program per trap identity this evaluator can raise, written in the pinned
corpus's own directive dialect (`[conf.directive]`) so they are **upstream-ready
as-is**: dropping a file into `wolf-lang/corpus/memory/` should need no edits.

They live here rather than in `upstream/corpus` because the independence rule
this repo runs under forbids writing into the pinned tree — `spec/` + `corpus/`
are read-only inputs, and the sprint's "authored here and upstreamed" is a
two-step process whose second step is a wolf-lang commit, not a wolf-interp one.

The closed trap vocabulary has eleven kinds (`[conf.trap.set]`). Six are
reachable at is02:

| kind | file | clause |
|---|---|---|
| `overflow` | `overflow_add.lu` | `arith.checked`, `[mem.ub.defined]` |
| `div-zero` | `div_zero_rem.lu` | `[mem.ub.defined]` |
| `bounds` | `bounds_slice.lu` | `[mem.ub.defined]` |
| `use-after-move` | `use_after_move_field.lu` | `[mem.tier0.move.2]` |
| `exclusivity` | `exclusivity_nested_path.lu` | `[mem.tier0.excl.1]`, `[mem.model.path.disjoint]` |
| `assert` | `assert_fails.lu` | `[conf.trap.map]` |

The other five belong to tiers this sprint does not implement: `region-fault`
(is03), `stale-handle` (is03), `alloc-contract` (I15's `#[noalloc]` family),
`race` (ic03), `ub` (is04's oracle). Their corpus programs are those sprints'
to author — writing them here would mean authoring expectations for machinery
that does not exist.

`tests/fault_snapshots.rs` snapshot-tests each program's fault rendering: trap
kind, both spans, and the clause anchor.

# Dynamic region semantics — the approximation contract

**Status:** drafted by wolf-interp is03, proposed as an amendment to
`spec/02-memory-model.md`. Anchors are stable once published
(`[conf.anchor.stable]`): this section **appends**, and renumbers nothing.

**Audience:** the compiler's static region and `shared` checkers (s19–s21) and
the differential protocol (spec/06). This document says what the *dynamic*
machine does, so the static checker's obligation can be stated in terms of it
rather than in terms of prose.

---

## 1. The direction, stated once

> **compiler accepts ⇒ interpreter never faults; interpreter faults ⇒ compiler
> must reject.**

Both halves are testable and only one is a bug when it breaks:

- A program the compiler accepts that faults here is a **compiler soundness
  bug** (or a spec bug — `[proto.cmp.triage]` makes the document the defendant
  first).
- A program the compiler rejects that runs clean here is **static
  conservatism**, an expected verdict class. `tests/run_corpus.rs` ledgers it
  and never green-washes it.

The static checker is correct exactly when it conservatively approximates the
machine below. Nothing about *how* it approximates is normative.

## 2. What the machine is

`[mem.model.machine]`'s components 3 and 4, made concrete
(`src/eval/region.rs`):

| component | representation |
|---|---|
| region table | `{id, name, state, strategy, parent, generation, allocations, depth}` |
| region state | `Open` \| `Suspended` \| `Frozen` \| `Freed` |
| strategy | `arena` (default) \| `rc` \| `pool(T)` |
| scope stack | a vector of open region ids; **the last is the current region**, the whole vector is the open set |
| pool | `{region, elem, slots: [{generation, life, value}]}` with `life ∈ {Reserved, Live, Free}` |
| `shared` cell | `{strong, weak, value, strong-edges-out, dead}` |

There is always a current region: a **program region** (`#0`) is created open
before `main` and freed by the machine after `main` returns.
`[mem.region.create.3]`'s "every function executes with a current region" has
no exception, and a machine with an empty ambient stack could not honour it.

Function calls do **not** push. That is the whole implementation of "the default
current region of a function body is its **caller's** current region" (D12).

## 3. The observable rules, and the trap each one raises

Every §3 fault surfaces as the single closed-vocabulary kind `region-fault`
(`[conf.trap.set]`); the **rule**, and through it the clause anchor, is what
distinguishes them. Each row is a fault program in `tests/faults/` with a
near-miss twin in `tests/faults/ok/`.

| clause | dynamic rule | trap kind | static counterpart |
|---|---|---|---|
| `[mem.region.intra.2]` | access through a reference into a `Freed` region | `region-fault` | — (lifetime error) |
| `[mem.region.edge]` | store of a reference off §3's edge table | `region-fault` | **E1004** |
| `[mem.region.freeze.1]` | write through a `Frozen` path | `region-fault` | — |
| `[mem.region.open.3]` | write through a `Suspended` path | `region-fault` | — |
| `[mem.region.freeze.3]` | freeze/transfer of an open subtree | `region-fault` | **E1005** |
| `[mem.region.multiopen]` | opening a non-antichain set (see §5) | `region-fault` | — |
| `[mem.shared.handle.2]` | deref of a stale generational handle | `stale-handle` | — (defined behavior) |
| `[mem.shared.handle.1]` | read of a `Reserved`, never-`init`ed slot | `use-after-move` | — |
| `[mem.tier0.move.2]` | use of a moved-from region value | `use-after-move` | **E1001** |

The pairing discipline is is02's, extended: `[conf.trap.map]` already states the
dynamic meaning of **E1001** (`use-after-move`) and **E1002** (`exclusivity`).
This section proposes the same treatment for **E1004** and **E1005**, whose
dynamic meanings the machine now produces and the ledger cannot yet classify as
counterparts (`src/ledger.rs::dynamic_meaning` is deliberately limited to the
two the spec states).

Detection is **exact**, never probabilistic: a dangling reference is a live
index whose region generation or slot generation no longer matches, which is a
comparison.

## 4. The leak assertion

At a clean program exit, **every region is `Freed` or `Frozen`**. `Frozen` is
not a leak: `[mem.region.freeze.1]` makes frozen data immutable *forever*, so
"never freed" is its specified end state.

A program that traps left its scopes without running them; the regions it still
holds are is06's crash-cleanup subject, not a leak. `tests/region_machine.rs`
asserts the invariant over every corpus file that exits cleanly, and asserts
separately that a sugar block's `}` frees on the trap path too.

## 5. Findings against `spec/02-memory-model.md`

These are the reasons this document is a *proposal* and not just a description.

### 5.1 `[mem.region.multiopen]` is incoherent as written — **top severity**

The clause discharges its own disjointness obligation like this:

> the open set is a set of *distinct region values*; since region values are
> affine (`[mem.region.create.2]`), two open handles are two regions by
> construction.

**Distinctness of affine values is not disjointness of footprints.**
`[mem.region.edge.iso]` explicitly lets one region own another; an owner's open
window reaches its child's data, which is precisely the aliasing Verona's
single-window rule exists to forbid. Two distinct affine region values in an
ancestor/descendant relation satisfy the clause and violate the property it is
trying to state.

*Proposed repair, one sentence:* the open set must be an **antichain in the
region forest** — no member may be an ancestor of another. This machine
implements the repair (`Store::enter`), it costs one walk of the parent chain,
and it leaves every pinned multiopen litmus green, because those open sibling
roots rather than an ancestor/descendant pair.

### 5.2 `[mem.region.open.1]` contradicts the corpus — **top severity**

> A region is **Open** (mutable) in at most one scope at a time.

`corpus/memory/region_multiopen_swap.lu` — pinned at `run(exit=0)`, and one of
the two files the clause itself flags for model checking — writes:

```
region a: pool(Node) {
    …
    in a { pa.init(ha, Node { value: 10 }) }
    …
}
```

The sugar opens `a` for the whole block; `in a` opens it again inside. Under a
literal reading of the clause the corpus's own litmus is a violation. Since the
corpus is not the defendant, the reading that survives is: **re-entering a
region that is already open is idempotent** (a depth count, not a second
window). This machine implements that; the clause needs the sentence.

### 5.3 The region "forest" has no region-to-region parent edge for the
common case — **medium**

`[mem.model.machine]` promises a forest of "regions with parent edges (≤1
each)", but `[mem.region.edge.iso]` locates the owning handle in "the region
value **or** the iso field holding it". A region value bound to a local is owned
by a **stack** slot, which is not a region — so lexically nested sugar blocks
create *no* parent edges and are siblings in the forest. Two consequences the
document does not draw:

1. `[mem.region.freeze.3]`'s "a region containing an open child region" is
   defined only for the iso-field case.
2. The disjointness hole in §5.1 is *reachable only* through an iso field,
   which is what makes the antichain repair cheap.

The machine records a parent edge exactly when a region value is stored into
region data, and re-walks the forest invariant after every such store.

### 5.4 E1006 has no dynamic counterpart in the closed trap vocabulary
— **medium**

The sprint asks for a "`shared` acyclicity **assertion** at strong-edge
creation, the dynamic counterpart of E1006". `[mem.shared.rc.2]` makes a strong
cycle a compile error and `[mem.ub.defined]` lists it as an error rather than a
trap; `[conf.trap.set]` is closed at eleven kinds and has none for it.

The machine therefore implements the check as a **trace assertion**, not a trap:
inventing a kind would extend a closed vocabulary, and reusing `region-fault`
would put this implementation's guess into a differential comparison. If the
acyclicity rule is to be dynamically enforceable, spec/02 must do for E1006 what
it already does for E1001/E1002 — state the kind — or spec/05 must open the set.

### 5.5 `corpus/regions.lu` cannot reach its pinned `run(exit=0)` — **high**

The file's `main` calls `build_config()`, which is declared nowhere in the file,
nowhere else in `corpus/`, and is not in the ambient std stub (s13's list). It
also calls `frozen.get(config)`, `…is_valid()` and `h.child.is_closed()`, none
of which any pinned document specifies. `check: run(exit=0)` is therefore
unsatisfiable for *any* implementation, this one included; wolf-interp reports
`unsupported` with `x-unsupported: "build_config does not resolve"`.

The appendix in spec/02 §A reproduces the same undefined call, so the fix is one
edit in two places (add the function, or change the file's `check:`). Not a
wolf-interp bug and not adjusted here: the corpus is not the defendant, but it
is also not self-consistent.

### 5.6 `[mem.shared.rc.3]` fixes the *shape* of a failed upgrade but not the
tag — **low**

"upgrading yields an option-shaped result the caller must handle" names the
shape only. `corpus/memory/shared_ok.lu` handles it with a wildcard
(`else |_|`), so nothing observable turns on the choice; this machine yields the
tag `None`. A later std/option lock should pin it.

## 6. Deliberate approximations in *this* machine

Each one is a place where the interpreter is less precise than the spec, always
in the direction that cannot produce a spurious fault.

### 6.1 Only granules with identity can dangle

`[mem.model.value]` says values "have no identity beyond their current place",
and is02 implemented that literally: a wolf assignment is a Rust move and a wolf
`copy` is a Rust clone. Consequently an **aggregate cannot dangle** — copying it
out of a region copies the data. The things that can dangle are the ones the
language gives identity to: region values, pools, `handle`s and `shared` cells.
Use-after-free is exact over exactly that set.

This is why the use-after-free litmus is written with a `handle`: at Tier 1 with
affine region values, there is no way to *write* a dangling plain reference. A
Tier-3 raw pointer is the other way, and it is is04's.

### 6.2 The edge check runs at the stores this machine has

§3 says "on **every** store of a reference". The destinations that are region
data in this machine are (a) struct-literal fields, checked against the current
region at construction, and (b) pool slots, checked against the pool's region at
`init`. Stack locals are not region data (`[mem.model.machine]`: "or *stack* for
locals"), so storing a region value in a local is unrestricted — that is what
makes a region value first-class. Growing a collection *in place* inside a
region is not yet a checked store; no pinned corpus program performs one.

### 6.3 Reclamation is at scope exit, not at last use

`[mem.region.intra.2]` frees at "the last use of the region value" and
`[mem.shared.drop.2]` reclaims destructor-free values "any time after their last
use". Both are unobservable **except** through destructor timing
(`[mem.shared.rc.4]`), and the language has no user destructors yet — so
`[mem.shared.drop.1]`'s scope-exit point is the only observable one, and it is
the one implemented, LIFO, after the scope's `defer`/`errdefer`.

**When user destructors land**, the interleaving of a destructor-carrying value
with `defer` becomes observable and must be by registration order, not "defers
then drops". That is the one thing in this section that will need re-doing.

### 6.4 Function parameters are not swept

A `read`-mode argument copies its value under MVS, so sweeping a parameter at
frame exit could free a region the caller still owns. Declining to sweep leaks,
which is defined and safe (`[mem.ub.defined]`); sweeping would fault wrongly,
and §1 forbids that direction.

### 6.5 What is still `unsupported`, and why

`channel`/`Mutex`/`worker`/`acquire`/`release` have no pinned semantics, so
`corpus/memory/region_move_while_open.lu` (whose first statement is
`channel[region](1)`) never reaches its region check. The dynamic counterpart of
E1005 is implemented and exercised by `tests/faults/region_move_open.lu`
instead. Region transfer across procs is ic03's; the unsafe tier is is04's.

## 7. Observability

`--trace=mem` logs every region event — create, open, close/suspend, freeze,
free, edge checks, ambient allocations, RC operations, handle faults — each line
naming the rule and its clause anchor. The filter is the anchor's namespace, not
a hand-kept list, so it cannot drift from the rule registry.
`tests/region_machine.rs` asserts that `--trace=mem` is exactly the `mem.*`
subset of `--trace`, and that each of the five D3 optimizer-fact witness
programs (`tests/witness/`) cites the rules that license its fact.

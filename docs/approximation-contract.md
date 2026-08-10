# Dynamic memory semantics — the approximation contract

**Status:** drafted by wolf-interp is03 (Tier 1 and 2, §§1–6), extended by is04
(Tier 3, §§7–9). Proposed as an amendment to `spec/02-memory-model.md`. Anchors
are stable once published (`[conf.anchor.stable]`): these sections **append**,
and renumber nothing.

**is03's findings, at pin `ecea37c`:** §5.1 and §5.2 were **repaired upstream**
on 2026-08-09 and are now normative text; the machine already conformed and
`src/eval/region.rs` cites the repaired anchors. §5.4's E1004/E1005 half was
repaired in `[conf.trap.map]` and `src/ledger.rs::dynamic_meaning` now
classifies both as dynamic counterparts. §5.3 and §5.6 stand.

**At pin `8b04edf` (is05):** §5.5 is **fully repaired** — `regions.lu`'s tail
now uses only specified semantics and RUNS (`exit(0)`, the run ledger's newest
entry). The §6 provenance table's Reserved row was repaired upstream (child
reads no longer activate; the two-phase window is real) and
`src/eval/prov.rs` was realigned to the published-TB table it always claimed
to implement. P1's row text now names the protected foreign-write explicitly;
this machine always landed protectors on P1, and the reviewed snapshot moved
with the wording. Each subsection below carries its own status line.

**Audience:** the compiler's static region and `shared` checkers (s19–s21), the
unsafe-tier implementation tests (s22), the compiler's shipped miri-lite (s23 —
this document says what it is diffed against), and the differential protocol
(spec/06). This document says what the *dynamic* machine does, so the static
checker's obligation can be stated in terms of it rather than in terms of
prose.

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
(`src/eval/region.rs`) — and, since is04, component **2**, the provenance
forest (`src/eval/prov.rs`; §7 below is its contract):

| component | representation |
|---|---|
| region table | `{id, name, state, strategy, parent, generation, allocations, depth}` |
| region state | `Open` \| `Suspended` \| `Frozen` \| `Freed` |
| strategy | `arena` (default) \| `rc` \| `pool(T)` |
| scope stack | a vector of open region ids; **the last is the current region**, the whole vector is the open set |
| pool | `{region, elem, slots: [{generation, life, value}]}` with `life ∈ {Reserved, Live, Free}` |
| `shared` cell | `{strong, weak, value, strong-edges-out, dead}` |
| provenance forest | per allocation `{size, bytes, init, live, owner, root, tags, exposed}`, per tag `{parent, perms-per-byte, protected}` |
| pointer value | `(allocation, offset, tag \| wildcard, pointee size)` |

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
| `[mem.region.edge]` | store of a reference off §3's edge table | `region-fault` | **E1004** ✅ |
| `[mem.region.freeze.1]` | write through a `Frozen` path | `region-fault` | — |
| `[mem.region.open.3]` | write through a `Suspended` path | `region-fault` | — |
| `[mem.region.freeze.3]` | freeze/transfer of an open subtree | `region-fault` | **E1005** ✅ |
| `[mem.region.multiopen]` | opening a non-antichain set (see §5) | `region-fault` | — |
| `[mem.shared.handle.2]` | deref of a stale generational handle | `stale-handle` | — (defined behavior) |
| `[mem.shared.handle.1]` | read of a `Reserved`, never-`init`ed slot | `use-after-move` | — |
| `[mem.tier0.move.2]` | use of a moved-from region value | `use-after-move` | **E1001** |

The pairing discipline is is02's, extended: `[conf.trap.map]` states the dynamic
meaning of **E1001** (`use-after-move`) and **E1002** (`exclusivity`), and — as
of pin `ecea37c`, in response to this section — of **E1004** ("illegal
cross-region edge") and **E1005** ("transfer of an open region"), both
`region-fault`. `src/ledger.rs::dynamic_meaning` now carries all four, so the
two files the corpus pins at `fail(E1004)`/`fail(E1005)` are classified as
**dynamic counterparts** rather than as conservatism. The ✅ marks above are that
change. The table is still only what the document states: no other E1xxx has a
stated dynamic meaning, and inventing one would be this implementation
legislating.

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

> **Status: REPAIRED upstream at `ecea37c` (2026-08-09).** The clause now reads
> "the open set must be an **antichain in the region forest** — no open region
> may be an ancestor (owner, transitively via iso edges) of another open
> region", with "Distinctness of affine region values is *not* sufficient"
> spelled out. `Store::enter` already implemented exactly this, so the machine
> needed no change; what changed is that it is now conformance rather than a
> proposal. Verified against the normative text by
> `region::tests::an_ancestor_and_its_descendant_do_not_open_together`.

The clause discharged its own disjointness obligation like this:

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

> **Status: REPAIRED upstream at `ecea37c` (2026-08-09).** The clause now adds:
> "Re-entering a region that is already open in the current scope chain
> (`in a { … }` inside `region a { … }`) is **idempotent** — openness is
> depth-counted, not a violation." Which is what `Region::depth` already was.
> Verified by `region::tests::reopening_an_open_region_is_idempotent`.

The clause read:

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

> **Status: FULLY repaired at `8b04edf` (2026-08-09).** The tail is now
> `if total == 4950 && config.limit == 42 { 0 } else { 1 }` — frozen data
> readable forever per `[mem.region.freeze.1]`, nothing unspecified left in
> the file — and this machine runs it to `exit(0)`. The finding below is kept
> as the record of what the corpus said before the repair.

The file's `main` called `build_config()`, which was declared nowhere in the
file, nowhere else in `corpus/`, and was not in the ambient std stub (s13's
list). That half is fixed. What survives is the *other* half of the same
sentence:

```
if total == 4950 && frozen.get(config).is_valid() { 0 } else { 1 }
```

`frozen` is a frozen region value; `region.get(x)` and `.is_valid()` appear in
no pinned document. `&&` does not short-circuit past them — `total == 4950` is
true — so every conforming implementation must evaluate a call whose meaning
nothing specifies. `check: run(exit=0)` is therefore still unsatisfiable, and
wolf-interp reports `unsupported` with `x-unsupported: "`region` has no method
`get` …"`. The remaining fix is the same shape as the one already made: declare
what `get`/`is_valid` mean, or change the file's tail to something the documents
define.

Not a wolf-interp bug and not adjusted here: the corpus is not the defendant,
but it is also not yet self-consistent.

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

One consequence of the copy model used to leak past `[mem.region.freeze.1]`:
a write *through* a struct built inside a `freeze region { … }` landed on the
value-tree copy and executed, where wolfc rejects the program E1012
(wolf-interp#2 — dynamic conservatism in the wrong direction). Struct values
now carry the region charged at their allocation site (`Value::Struct::home`;
`[mem.model.alloc]` lands in the current region), and `write_path` refuses a
write whose path passes through a container homed in a `Frozen` region — the
value-path half of the check the granule paths always ran, faulting
`region-fault [mem.region.freeze.1]` before anything is mutated. Reads stay
legal forever, and rebinding the *binding* stays legal (it replaces what the
binding holds; no frozen storage is touched) —
`tests/faults/region_freeze_value_write.lu` and its twin
`ok/region_freeze_rebind_ok.lu` pin both directions. The remaining
approximation: only **struct** composites carry a home, so a bare list or map
frozen the same way still takes the write on its copy; no pinned corpus or
book program performs one, and the E1012 ⇄ `region-fault` pairing stays out
of `ledger::dynamic_meaning` until `[conf.trap.map]` states it (the
E1004/E1005 precedent).

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

*(Update, is06: spec/03 pinned `channel`, and the file now runs and traps
`region-fault` — CI asserts it. The rest of the list stands.)*

### 6.6 Declared types do not coerce at struct construction (found is11, filed)

Passing a literal to an `int` parameter adopts the parameter's type
(`f(42)` where `fn f(x: int) -> int` yields an `i64`), but constructing a
struct does not: `C { limit: 42 }` with `limit: int` stores the literal's
own `i32`, observable at the REPL as `:type c.limit` → `i32`. Coercion is
the type checker's property and sema-lite takes signatures at face value,
so the inconsistency cannot produce a spurious fault — but the two
positions disagree with each other, which is a wart, not a choice. Filed
during the is11 sweep rather than changed: aligning either direction is a
behavior change, and the code that owns literal typing is the compiler's
half of the split.

### 6.7 Bare-ident patterns resolve against the value, not the type (0.1.2)

The checker resolves a bare identifier in a pattern against the scrutinee's
*type*: an in-scope enum variant is a variant pattern, a row tag of the
scrutinee's `!T ! {row}` is a tag pattern, anything else binds (issue #5,
wolf-std F-0007 — first-arm-always was the bug). This machine has no types
to consult, so it approximates with what it has:

- an identifier that names a variant of an enum **declared in the current
  module** is a variant pattern — it matches the payload-free tag spelled
  either bare (`Greater`) or enum-qualified (`Ordering.Greater`), and the
  same table lets a payload pattern `Rgb(r, g, b)` match a value built as
  `Color.Rgb(1, 2, 3)`;
- otherwise, a **capitalized** identifier whose scrutinee is a tag-shaped
  value (an error value) is a structural row-tag pattern (D30: rows need
  no declaration; the machine already reads unresolved capitalized names
  as tags in expression position) — it matches on tag equality and never
  binds;
- everything else binds, including a capitalized name over a non-error
  scrutinee, which the counterparty also treats as a binding (observed at
  pin a0c4564: `match 3 { Zed => Zed, _ => 9 }` warns E0802 unreachable on
  the `_` arm and runs).

The residual imprecision: a capitalized *binding* over an error scrutinee —
a name the checker would resolve as a binding because it is neither a
variant nor in the row — reads as a tag pattern here and fails to match.
No pinned corpus, book, or wolf-std program spells one (the convention is
lowercase bindings), and the failure mode is an honest `unsupported` ("no
`match` arm applied"), never a wrong answer. Exhaustiveness stays the type
checker's (E0801 has no dynamic half): a match no arm of which applies is
`unsupported`, not a trap. Bare *dotted* path patterns (`Ordering.Less =>`)
are outside `[gram.pat]` and rejected at parse with the counterparty's
E0201 shape.

### 6.8 `for` iterates a loop-entry snapshot; the spec is silent (S-11)

`loop_expr ::= 'for' pattern 'in' expr block` is all the pinned spec says
about `for` (`[gram.expr.flow]`): no clause in spec/01 or spec/02 states
whether the loop holds an access on the iterated container for its extent,
moves it, or copies it. This machine evaluates the operand **once, at loop
entry**, and iterates that value — under MVS the natural reading — so a
body that mutates the container (`for x in xs { xs.push(x) }`) executes,
the mutation lands, and the iteration never observes it. No trap fires.

wolfc rejects the same program statically with **E1001** (use-after-move:
its lowering moves the operand into the loop), and `[conf.trap.map]`'s
comparison alphabet would predict an `exclusivity` trap if the spec gave
the loop a `mut`-grade hold it never states. Three mutually consistent
readings, zero clauses: filed as **S-11** in `docs/divergence-log.md`
(issue #9, wolf-std F-0014; compiler half wolf-lang#15) rather than
legislated here — a snapshot loop cannot produce a spurious fault, which
is the one direction §1 forbids, and inventing a trap the spec never
names would be this machine legislating. wolf-std keeps the divergence
visible: `tests/list/mutate_while_iterating.lu`, ledgered
`lupin = run` / `wolfc = fail(E1001)`.

## 7. Deliberate approximations in the **provenance** machine (is04)

`src/eval/prov.rs` is `spec/02` §6 made executable: per-allocation tag trees,
the `[mem.prov.state]` transition table per location, protectors for a call's
extent, angelic wildcard resolution, and `[mem.prov.region]`'s composition with
the region store. Everything below is a place where it is *less* precise than
the document, always in the direction that cannot produce a spurious `ub(…)`.
The direction matters more here than anywhere else in this file: a spurious UB
verdict is a **soundness-candidate divergence** (`[proto.cmp.severity]`), the
highest-severity class the protocol has, so a wrong one wastes the most
expensive kind of attention.

### 7.1 The order of the checks decides the row, and the order is a choice

One access can be true of several rows at once — a read past the end of a freed
allocation in a dead region is P3, P4, P1 and L1 simultaneously. `[mem.ub]`
enumerates the rows but does not order them, so this machine picks, and prints
the pick rather than hiding it:

| order | row | why it is first |
|---|---|---|
| 1 | **P3** bounds | an out-of-bounds access has no location to have a permission *at*; asking the tag tree about byte 16 of an 8-byte allocation is meaningless |
| 2 | **P4** region freed | the *region* is the more specific fact than the tag, and it licenses a different optimization (O3b/O4's per-region alias domains, not O1's per-tag noalias) |
| 3 | **P1/P2** the tag tree | `[mem.prov.state]`'s table |
| 4 | **L2** dangling | taken instead of 3 when the pointer carries a wildcard into a dead allocation: there is no tag to consult |
| 5 | **L1** uninitialized | last, because "what was written here" is only a question once the access is otherwise legal |

`corpus/memory/unsafe_ub_uaf.lu` pins the P1-versus-P4 half of this: the file's
own comment says §7/P1, its `c.free` kills the allocation while the *region*
that owns it is still alive, and the machine reports P1. A machine that checked
the region first would have to disagree with the corpus.

**What a second implementation may do differently:** any of it. The row rides
`x-ub-row`, an `x-` key, and `[proto.cmp.defined-divergence]` says a key absent
on one side is never a divergence — but two oracles that both emit it and
disagree *is* one, and this table is what triage would consult first.

### 7.2 §7 has no row for a protector violation, so P1 carries it

`[mem.prov.state]` creates a UB condition §7 does not enumerate: "Protected tags
escalate the foreign-write transition to immediate UB for the protection's
duration." The write itself is not an access through a Disabled tag (P1's
wording), a write through a Frozen tag (P2's), or anything else in the closed
table. It is the *invalidation* of a live protected borrow.

This machine reports **P1** and says so in the message, on the reading that
P1's parenthetical — "use of an invalidated borrow" — is the row the protector
exists to make immediate: without it, the protected tag would simply become
Disabled and the UB would arrive at the borrow's next use, which is P1 by any
reading. The licensed optimization agrees: P1's O1 is "`mut` params lower to
`noalias` + `dereferenceable`", and the protector is exactly what licenses the
call-extent half of that.

**Finding, low severity:** the enumeration is closed (`[mem.ub.closed]`), so a
UB condition stated in §6 that no §7 row names is a gap in the closure argument.
Either P1's wording should say "…or the invalidation of a protected borrow", or
a row should be added with its own pairing. `tests/ub/p1_protector.lu` is the
program; `tests/ub/ok/p1_protector_ok.lu` is the same aliasing with the write
moved past the call, which is defined.

### 7.3 A `read` parameter's Frozen tag is a witness, not the callee's path

`[mem.prov.tag]` makes parameter entry a retag point for both modes. Under MVS
(`[mem.model.value]`) a `read` parameter is the **callee's own copy** of the
value, not a second name for the caller's place — so binding the callee's
parameter place to the caller's Frozen child would model an aliasing that the
language does not have, and any write to the parameter inside the callee would
report §7/P2 on a *safe-tier* program.

So: `mut` binds (the callee genuinely writes through the borrow, and
call-by-value-result writes the result back), `read` does not. The Frozen child
still exists, still protected for the call's extent, as the **witness** of the
caller-side promise `[mem.tier0.mode.read]` makes — which is what a foreign
write during the call violates, and what O2's load-hoisting is licensed by.
`corpus/memory/prov_holy_grail.lu`'s trace shows it.

Raw pointers are different and simpler: their tag travels *in* the value, so
both modes retag the pointer and the callee's accesses genuinely go through the
child. That is why `tests/ub/p2_frozen_write.lu` is a raw-pointer program.

### 7.4 Tier-0 places are tagged lazily, and their tags form a forest of stumps

A place gets an allocation the first time something retags it, and reads and
writes of an untagged place cost nothing. Two consequences:

1. **`a` and `a.x` are unrelated in the provenance forest.** Their place keys
   differ, so they get different allocations, and a write to `a` does not
   invalidate a borrow of `a.x`. This *under*-approximates — it can only miss a
   violation, never invent one — and Tier-0 exclusivity (`[mem.tier0.excl.1]`,
   is02's `AccessSet`) already decides that case exactly, by the same
   prefix rule `[mem.model.path.disjoint]` states.
2. **A place's storage is stack, never region data.** `[mem.model.machine]`
   locates an allocation's owner at "a region (or *stack* for locals)", and a
   local is the second; giving place allocations a region owner would fire
   §7/P4 on ordinary safe-tier reads after any region free. Likewise their bytes
   are born initialized, because a Tier-0 place's initialization is `Slot`'s
   business and reading a moved-from one is `trap(use-after-move)` — a *defined*
   execution (`[mem.tier0.move.2]`), not §7/L1, whose wording is "via raw
   pointers".

Together these are why `[mem.ub]`'s "safe-tier programs cannot reach any row" is
true of this machine **by construction** rather than by luck, and
`run_corpus::unsafe_free_corpus_programs_never_produce_a_ub_verdict` asserts it
over every unsafe-free program in the pinned corpus.

### 7.5 Dead tags are pruned

The sprint permits "dead-tag pruning only if tests need it", and they do: the
tree walk is per-access, so a loop that passes one place a thousand times would
otherwise leave a thousand dead siblings for every later access to walk. A tag
is pruned when it is not a root, not protected, has no children, is bound to no
place and is not exposed — at which point nothing can ever observe it again, so
removing it changes no verdict. Pruning happens at scope exit and at the end of
a call, which are the points where the extents that hold unbound tags end.

### 7.6 Addresses are implementation-specified, and that is a protocol fact

`ptr as int` has to produce *something*. This machine lays allocations out at
`(id + 1) * 0x1_0000` and reports offsets into that. It is deterministic,
platform-independent, and deliberately not a real address —
`[proto.cmp.defined-divergence]` lists "unspecified layout observations (Tier-3
address inspection)" among the things that are **never** divergences, so the
number is not a comparison surface. What *is* comparable is the round trip: an
integer from a live allocation casts back to a pointer that resolves angelically
to an exposed tag, and one past the end of every allocation resolves to none,
which is what keeps §7/L2 reachable.

### 7.7 `[mem.prov.tag]`'s "Tier-2 cell access" retag point is not implemented

The clause lists four retag points; three are implemented (borrow creation via
the door, `mut`/`read` parameter entry, and `borrow r from ptr`). Tier-2 cell
access is not: `shared`/`handle` are Store-side granules with no byte-level
storage in the provenance machine, so there is no allocation for a cell-access
tag to be a child *of*. The Tier-2 rules that matter dynamically —
`[mem.shared.handle.2]`'s generation check, `[mem.shared.handle.3]`'s
exclusivity — are enforced exactly by is03's machine and produce traps, not UB.
Recorded as unimplemented rather than left as an absence.

### 7.8 `assume noalias` is checked where it is written, not carried as a contract

`[mem.unsafe.raw.2]` says the assertion holds "for the assertion's scope". This
machine evaluates the operands, compares the ranges, and reports §7/P5 there and
then — which is **exact** for every shape the corpus and `tests/ub/` contain,
because a raw pointer is a value and the two operands are fully known at the
statement. What it does not do is re-check the assertion after a later
assignment: `var p = …; assume noalias p, q; p = q` would evade it. Recording
the ranges for the scope and re-checking each access is the precise reading, and
it is the one to implement when a program needs it; nothing pinned does yet, and
the assumption list is already kept (`Provenance::assumptions`) so the check has
somewhere to live.

The sprint file names an anchor `[ub.assume.noalias]` for this row. No such
anchor exists in the pinned `spec/02`, which puts it at §7/P5 and states the
rule at `[mem.unsafe.raw.2]`; those are what the machine cites. Recorded so the
mismatch is a known one rather than a citation this repo invented.

## 8. The C library, modelled — the host-intrinsic approximation

`corpus/memory/unsafe_noalias.lu`, `corpus/memory/unsafe_ub_uaf.lu` and
`corpus/ffi.lu` open with `import c "stdlib.h"` and then call C. There is no FFI
in this interpreter and there will not be one: `unsafe_code = "forbid"` is in
`Cargo.toml`, and an interpreter that dlopen'd libc would be comparing the
*host's* allocator against the compiler's — an observation the protocol already
says is not a comparison surface.

**The modelled set is closed and small:** `c.malloc`, `c.calloc`, `c.free`,
`c.memset`, `c.memcpy`. A C name outside it resolves (so the failure is
"unsupported feature", never "unknown name") and then declines with a reason,
by the same two rules `src/eval/builtin.rs` states for the std stub.

What the model claims, clause by clause:

| behaviour | clause | modelled as |
|---|---|---|
| `malloc(n)` yields a live allocation of `n` uninitialized bytes | — | a provenance allocation with `init` all false, so an unwritten read is §7/L1 |
| the allocation belongs to a region | `[mem.boundary.ffi]` "a C call executes against an implicit region borrowed for the call's extent" | owned by the region current at the call, so `[mem.prov.region]` decides what a region free does to it (§7/P4) |
| the pointer C hands back is wildcard-shaped | `[mem.prov.expose]` "Wildcard pointers from FFI behave as exposed" | the root tag is exposed at creation, so a later int→ptr resolves to it |
| passing a pointer *to* C exposes it | `[mem.prov.expose]` | `expose_to_c`, and the call is a foreign havoc that angelic resolution already models |
| a C call is not a wolf call | `[mem.prov.tag]` (retag is at *parameter* entry) | **no retag** on C arguments. Retagging them would invent a `read` borrow and then report `memset`'s own write through it as §7/P2 — a spurious verdict on `corpus/ffi.lu` |
| `free(p)` ends the allocation | `[mem.prov.region]`, by analogy | the whole tag tree is Disabled; a later tagged access is §7/P1, a later wildcard access is §7/L2 |
| a double free, or a free of an interior pointer | — | §7/L2: `free` dereferences the block it releases |
| never calling `free` | `[mem.ub.defined]` "Memory leak … defined, safe" | reported on `Run::host_leaks`, never faulted |

**What this is not.** It is not a claim about any real libc, and it is not a
claim that these are the semantics the compiler must reproduce — s22 links
against a real C library and will find differences this model cannot have (real
`malloc` can fail; real `memcpy` on overlapping ranges is UB and this one is
not). The claim is narrower and is the only one the oracle needs: *given* the
allocation events, the provenance consequences are the ones §6 states.

`corpus/ffi.lu` executes its whole unsafe block under this model — allocation,
`memset`, the raw store, the read, the free, with exposure traced — and then
reports `unsupported` at the inline `asm` block, whose meaning no pinned
document gives. Declining there is the same rule as declining `c.strlen`.

## 9. Findings against `spec/02-memory-model.md` §5–§7 (is04)

### 9.1 §6's state set and the s04 sketch disagree on a name — **low**

`[mem.prov.state]`'s table names the states `Reserved | Active | Frozen |
Disabled`. The is04 sprint file's own sketch draws `Reserved → Unique → Frozen →
Disabled`. The sprint says s04 is normative and that "the machine and the spec
must agree on the state set exactly", so this machine implements `Active` and
the sketch's `Unique` is the stale spelling. Recorded because a reader coming
from the sprint file will look for `Unique` and not find it, and because
`ReservedIM` — which the sketch offers conditionally — is correctly **absent**:
`spec/02` admits no interior-mutability type, so the state is omitted and this
sentence is the spec saying so.

### 9.2 §6 does not say whether a *child read* activates a Reserved tag — it says it does, and that is unusual — **low**

`[mem.prov.state]`'s Reserved row reads `→ Active` under **both** child read and
child write. Tree Borrows as published leaves Reserved alone on a child read and
activates only on a write; the difference is observable in principle (a child
read followed by a foreign read would Freeze an Active tag but not a Reserved
one). This machine implements the table **as written**, because the table is the
normative artifact and the two-phase property the corpus pins
(`prov_two_phase.lu`) turns on the *foreign*-read row, which is unaffected.
Flagged so that a later alignment with the paper is a deliberate amendment
rather than a silent divergence.

### 9.3 §7's `Detected` column promises `Q` for rows this tier cannot reach — **informational**

Five rows are marked `O, Q` — the oracle *and* the D21 debug quarantine
allocator. This machine is the `O`. `Q` is s2x's, and nothing here claims it.
Listed so the coverage matrix in `tests/ub_coverage.rs` is not read as a claim
about the quarantine allocator's coverage.

### 9.4 §7/T2 is unreachable at this tier, and says so — **informational**

"Torn write producing a partially-updated wide value observed through another
tag" needs a second observer of a store in flight. This machine is
single-threaded and every store is whole-value (`[mem.model.value]`), and the
language surface has no split or volatile wide-store form to spell a tear with.
The row is therefore listed as `Coverage::Unreachable` with that reason rather
than silently absent, and ic03's interleavings are what make it reachable. §7/C1
is the sprint's `deferred(concurrency)` mark for the same reason, one campaign
further out.

## 10. Deliberate approximations in the **concurrency** machine (is06)

**Status:** drafted by is06 at pin `67c977f` — the first executable test of
`spec/03-concurrency.md`; **revised by is08 at pin `843174f`**, whose s20
S-batch turned findings S-1..S-8 into clauses (see §10.5 for the
confirmations and the two realignments). This section records what *this
machine* chose where the spec left room, and why every choice keeps the
one-way approximation direction.

### 10.1 One task at a time, by construction

Tasks live on OS threads only because a suspended tree-walk needs a call
stack; a per-task gate serializes them so **at most one task ever runs**.
Every schedule decision flows through one seeded generator and is a
numbered `sched-ev/0` event (`[conc.det.events]`); the same seed replays
the identical stream (`[conc.det.seed]`). Seed 0 (and the unseeded
default) is strict FIFO. There is no `unsafe` anywhere in the machine —
the crate forbids it — so the determinism claim rests on the baton, not on
memory-order reasoning.

### 10.2 Captures copy; cross-task shared mutability is (almost) inexpressible

Closures capture **by value** (`[gram.expr.closure]`), and that decision
does the heavy lifting: a spawned task writes its own copies, globals are
snapshotted per task, region transfer is checked at the send, frozen data
is immutable, and `Mutex` payloads move through the scheduler. The shapes
the compiler rejects statically (E1101/E1102) therefore mostly *cannot
misbehave* here — they run, with task-local effects (the standing
conservatism class; `conc/store_buffer.lu` is the exemplar, and
`conc/freeze_publish.lu`'s reliance on the forbidden shape is
DIV-2026-008). The two memories two tasks CAN share mutably are **pool
slots in an unmoved region** and **raw allocations** — exactly
`[conc.mm.race.1]`'s reachability — and both are watched by a
vector-clock race detector that traps `race` (`[conc.mm.race.3]`) exactly
at the conflicting interleaving the schedule realized. Detection is per
byte range (raw) and per slot (pools): no over-approximation, so the
forbidden direction (faulting a compiler-accepted program wrongly) stays
closed.

The silent-write-loss face of this choice — a task closure writes its
captured copy and the enclosing state never sees it — is now formally
routed upstream as **S-10** (divergence log; wolf-interp#4): spec/03
states E1101 as a compile error only, `[conf.trap.map]` gives it no
runtime meaning, and this machine will not invent one. If an amendment
lands (the E1004/E1005 precedent), the realignment happens then.

### 10.3 The killed-proc sequence, and what "no user code" means

`[conc.proc.kill]` runs in order: tasks are marked killed and unwound with
a dedicated signal that **skips every `defer`/`errdefer`**; the proc's own
region bulk-frees; reasons deliver. The machine's scope-exit *sweep*
(refcount decrements, idempotent region frees) still runs on the unwind —
it is bookkeeping, not user code, and skipping it would leak cells the
proc never owned. A task's ambient region at spawn is the spawner's
current region; a proc's is its own fresh region (`[conc.proc.1]`).

### 10.4 Blocking points and the wakes they surface

Cancellation and kill are delivered **only** at the closed set of
runtime-owned blocking points (`[conc.cancel.points]`): channel send/recv,
`select`, `when` acquisition, scope join, timer wait. Cancellation
surfaces as an error value (`Cancelled`) that ordinary returns carry —
defers run (`[conc.cancel.defer]`). A `--checked` build's function-entry/
back-edge polls are not implemented (no `--checked` profile exists here).
`checkpoint()` is not implemented (no pinned std surface). C intrinsics
complete synchronously, so `[conc.cancel.c]`'s "next safe point after
return" degenerates to the next blocking point — noted in the trace when a
concurrent program crosses the membrane.

### 10.5 Choices the spec left open — now adjudicated by the s20 S-batch

**Status update (is08, pin `843174f`):** spec/03 wrote the clauses this
section anticipated. Where the S-batch **confirmed** a choice, the rule now
cites the registered clause instead of a forward namespace or a family
root; where it **deviated**, the machine realigned. The ledger:

- **Scope join under cancellation keeps joining** — *confirmed and
  extended* by `[conc.task.fail.owner]`: the scope is the cancellation
  unit, owner included (the Trio posture, adopted from finding S-4). A
  failing child now cancels a scope owner blocked at a non-join blocking
  point (`Rule::TaskFailOwner`); an owner blocked at the *join* keeps
  joining, exactly as before. The owner's surfaced cancellation error is
  its finished cancellation — the child's failure still re-raises at the
  scope exit, after the join.
- **Drained-closed channels make `select` arms ready** — *confirmed*:
  `[conc.select.closed]` adopts Go's posture from finding S-8 verbatim.
  `Rule::SelectClosed` cites it at both readiness sites.
- **Deadlock** — *deviation paid as specified*: `unsupported`-with-roster
  is retired. `[conc.deadlock.def]` makes all-blocked/no-timer a defined
  outcome, `[conc.deadlock.trap]` adds `deadlock` as the deliberate
  twelfth `[conf.trap.set]` kind, and the machine traps it with the
  blocked-task roster in the message. Detection is *required* in
  deterministic test modes — this machine and the is07 explorer are both
  such modes, and the explorer's per-schedule verdict is now
  `trap(deadlock)`.
- **`when` payload write-back** — *confirmed*: `[conc.when.body]` states
  the rebind/write-back/reverse-release semantics the machine chose.
  **Nested `when`** — *deviation paid as specified*: the spec split the
  stopgap `assert` trap into E1103 (lexical, the compiler's) and
  `trap(deadlock)` for the dynamic already-held case
  (`[conc.when.nonest]`, `[conc.deadlock.self]`). The machine now keeps
  the per-task held-set and traps `deadlock` only on a genuine
  self-acquisition; a `when` reached through a call over a *disjoint* set
  proceeds — the compiler accepts that program, so the old blanket
  nesting fault would have broken the one-way approximation direction.
- **Region transfer over channels** — *confirmed*: `[conc.chan.move]`
  (the send is the affine move; closed and disconnected, dynamically
  re-checked at the send), `[conc.chan.staleuse]` (the sender's fault, at
  the use site) and `[conc.chan.imm]` (by reference, no move, no copy)
  are what `Rule::ChanMove`/`ChanStale`/`ChanImm` cite now.
- **Proc surface debts paid** (S-6/S-7): `w.cancel()` implements
  `[conc.proc.cancel]` (cooperative, defers run, `cancelled` reachable at
  last), `a.link(b)` implements `[conc.proc.link.pair]`, and the root
  domain's death is `[conc.proc.root]` — killed-proc sequence for every
  live proc, then a nonzero, implementation-specified exit (1 here;
  compare the class, never the number).
- **Monitor of an already-exited proc** delivers immediately; the reason
  value keeps its label but a `normal` payload observed this way collapses
  to unit (the label is what the corpus and tests compare). Unchanged —
  the S-batch does not speak to it.
- **Exclusivity and borrows stay per task.** The access set that enforces
  `[mem.tier0.excl]` is task-local; cross-task exclusivity has no dynamic
  meaning here because cross-task mutable paths do not exist (10.2).
  Unchanged.

### 10.6 The schedule explorer (is07): what it proves, what it approximates

**Status:** is07, pin `79ceec6`. The explorer (`src/explore.rs`,
`conform-run --explore=N`) is stateless model checking over the reified
`sched-ev/0` stream: replay from the root with a forced decision prefix,
classic Flanagan–Godefroid DPOR with sleep sets, full branching over
`select`-arm commits, budgets for schedules/steps/preemptions/wall clock.
The choices it rests on, named:

- **The branch alphabet is two decision kinds.** Every schedule point of
  is06's enumeration funnels into `State::decide` at exactly two places:
  the ready-task pick and the `select`-arm pick. Channel pairings, `when`
  grants, timer fires and deliveries are *deterministic consequences* of
  those picks in this machine (FIFO queues, sorted timers), so permuting
  the picks covers the whole space — the completeness assertion checks
  this on every replay, and the red test proves the check bites
  (`tests/explore_machine.rs`).
- **Conflicts are syntactic over runtime objects.** Two ops conflict when
  they touch the same channel/mutex/proc/task-control/scope/memory key and
  one mutates. This over-approximates (two buffered sends to a non-full
  channel "conflict" even when the program never observes order), which
  costs exploration and never soundness. One refinement is load-bearing:
  a task's *successful* scope-exit is a read of its scope (completion
  order of non-failing siblings is unobservable — `[conc.task.fail]`
  orders only failures), while a failing exit writes it; without this the
  independent-writers reduction would collapse.
- **The canonical state hash cannot see suspended frames.** It covers the
  scheduler-visible state (tasks + queues + wakes + clocks + open stacks,
  channels with contents, mutexes, procs, scopes, timers, race-detector
  memory, virtual clock) plus a rolling stdout digest — not a task's Rust
  call stack and not the region store's interior. Convergence pruning on
  that hash could in principle merge schedules whose difference lives only
  in un-communicated locals; `--explore-no-prune` removes the risk,
  `--paranoid` re-verifies every hit against the stored preimage (the
  collision guard), and on the pinned corpus pruning changes no
  conclusion (asserted in tests).
- **"Preemption" means a non-FIFO pick.** The machine is cooperative —
  tasks run to their next blocking point — so the CHESS bound maps to
  "decisions that depart from the front of the ready queue";
  `--explore-preemptions=P` bounds that count and reports what it skipped
  as an open frontier.
- **The packed-seed namespace is provisional (finding S-9).** Bit 62 of a
  `--seed` value tags a packed schedule (low 62 bits = mixed-radix choice
  digits, trailing FIFO choices free); everything else seeds the xorshift
  generator exactly as before, so all pre-is07 seeds and snapshots are
  untouched. Streams too wide for 62 bits use `--schedule=ev:c0,c1,…`.
  The accepted s36 Phase A hook-design doc owns the real encoding; it has
  not landed at this pin, the compiler runtime's format has priority, and
  this side re-pins when it exists.
- **Deferred, named:** source-DPOR (Abdulla et al. 2014), sleep-set
  refinements beyond the classic algorithm, stateful checkpointing, and
  any relaxed-memory exploration (SC only — `[conc.mm]`'s unsafe-tier
  relaxed orderings wait for stable clauses).

## 11. Observability

`--trace=mem` logs every region event — create, open, close/suspend, freeze,
free, edge checks, ambient allocations, RC operations, handle faults — each line
naming the rule and its clause anchor. The filter is the anchor's namespace, not
a hand-kept list, so it cannot drift from the rule registry.
`tests/region_machine.rs` asserts that `--trace=mem` is exactly the `mem.*`
subset of `--trace`, and that each of the five D3 optimizer-fact witness
programs (`tests/witness/`) cites the rules that license its fact.

`--trace=prov` is the same mechanism one tier up: the Tier-3 subset — §5
`mem.unsafe.*`, §6 `mem.prov.*`, §7 `mem.ub*`, and `[mem.boundary.ffi]` — so
every retag, permission transition, exposure, protector, angelic resolution and
UB row is on it, and nothing else is. `tests/prov_machine.rs` asserts it is
exactly that subset of `--trace`, computed from the rule registry rather than
listed.

A UB report carries, on the record and in the human trace alike: the §7 row
(`x-ub-row`), the clause (`x-ub-clause`), the access span (`x-ub-span`), the
tag-creation span (`x-ub-tag-span`), the rendered borrow-tree slice
(`x-ub-tree`), and — the D2 pairing, executable — the optimization the row
licenses (`x-ub-licenses`). `[mem.ub.closed]` makes the last one an invariant
rather than a nicety: a row that licenses nothing is a rule this language does
not have, so a reader who is stopped by one can always find out what it bought.

# Dynamic memory semantics: the approximation contract

**Status:** drafted by wolf-interp is03 (Tier 1 and 2, §§1–6), extended by is04
(Tier 3, §§7–9). Proposed as an amendment to `spec/02-memory-model.md`. Anchors
are stable once published (`[conf.anchor.stable]`): these sections **append**,
and renumber nothing.

**is03's findings, at pin `ecea37c`:** §5.1 and §5.2 were **repaired upstream**
on 2026-08-09 and are now normative text. The machine already conformed, and
`src/eval/region.rs` cites the repaired anchors. §5.4's E1004/E1005 half was
repaired in `[conf.trap.map]`, and `src/ledger.rs::dynamic_meaning` now
classifies both as dynamic counterparts. §5.3 and §5.6 stand.

**At pin `8b04edf` (is05):** §5.5 is **fully repaired**. `regions.lu`'s tail
now uses only specified semantics and RUNS (`exit(0)`, the run ledger's newest
entry). The §6 provenance table's Reserved row was repaired upstream (child
reads no longer activate; the two-phase window is real), and
`src/eval/prov.rs` was realigned to the published-TB table it always claimed
to implement. P1's row text now names the protected foreign-write explicitly;
this machine always landed protectors on P1, and the reviewed snapshot moved
with the wording. Each subsection below carries its own status line.

**Audience:** the compiler's static region and `shared` checkers (s19–s21), the
unsafe-tier implementation tests (s22), the compiler's shipped miri-lite (s23),
and the differential protocol (spec/06). The miri-lite is diffed against this
document. This document says what the *dynamic* machine does, so the static
checker's obligation can be stated in terms of it instead of in terms of prose.

---

## 1. The direction, stated once

> **compiler accepts ⇒ interpreter never faults; interpreter faults ⇒ compiler
> must reject.**

Both halves are testable and only one is a bug when it breaks:

- A program the compiler accepts that faults here is a **compiler soundness
  bug** (or a spec bug: `[proto.cmp.triage]` makes the document the defendant
  first).
- A program the compiler rejects that runs clean here is **static
  conservatism**, an expected verdict class. `tests/run_corpus.rs` ledgers it
  and never green-washes it.

The static checker is correct exactly when it conservatively approximates the
machine below. Nothing about *how* it approximates is normative.

## 2. What the machine is

`[mem.model.machine]`'s components 3 and 4, made concrete
(`src/eval/region.rs`). Since is04, component **2** as well, the provenance
forest (`src/eval/prov.rs`; §7 below is its contract):

| component | representation |
|---|---|
| region table | `{id, name, state, strategy, parent, generation, allocations, bytes, depth}` |
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
(`[conf.trap.set]`). The **rule**, and through it the clause anchor, is what
distinguishes them. Each row is a fault program in `tests/faults/` with a
near-miss twin in `tests/faults/ok/`.

| clause | dynamic rule | trap kind | static counterpart |
|---|---|---|---|
| `[mem.region.intra.2]` | access through a reference into a `Freed` region | `region-fault` | none (lifetime error) |
| `[mem.region.edge]` | store of a reference off §3's edge table | `region-fault` | **E1004** ✅ |
| `[mem.region.freeze.1]` | write through a `Frozen` path | `region-fault` | none |
| `[mem.region.open.3]` | write through a `Suspended` path | `region-fault` | none |
| `[mem.region.freeze.3]` | freeze/transfer of an open subtree | `region-fault` | **E1005** ✅ |
| `[mem.region.multiopen]` | opening a non-antichain set (see §5) | `region-fault` | none |
| `[mem.shared.handle.2]` | deref of a stale generational handle | `stale-handle` | none (defined behavior) |
| `[mem.shared.handle.1]` | read of a `Reserved`, never-`init`ed slot | `use-after-move` | none |
| `[mem.tier0.move.2]` | use of a moved-from region value | `use-after-move` | **E1001** |

The pairing discipline is is02's, extended. `[conf.trap.map]` states the dynamic
meaning of **E1001** (`use-after-move`) and **E1002** (`exclusivity`). As of pin
`ecea37c`, in response to this section, it states **E1004** ("illegal
cross-region edge") and **E1005** ("transfer of an open region") too, both
`region-fault`. `src/ledger.rs::dynamic_meaning` carries all four, so the two
files the corpus pins at `fail(E1004)`/`fail(E1005)` are classified as **dynamic
counterparts** instead of as conservatism. The ✅ marks above are that change.
The table states only what the document states: no other E1xxx has a stated
dynamic meaning, and inventing one would be this implementation legislating.

Detection is **exact**, never probabilistic: a dangling reference is a live
index whose region generation or slot generation no longer matches, which is a
comparison.

## 4. The leak assertion

At a clean program exit, **every region is `Freed` or `Frozen`**. `Frozen` is
not a leak: `[mem.region.freeze.1]` makes frozen data immutable *forever*, so
"never freed" is its specified end state.

A program that traps left its scopes without running them. The regions it still
holds are is06's crash-cleanup subject, not a leak. `tests/region_machine.rs`
asserts the invariant over every corpus file that exits cleanly, and asserts
separately that a sugar block's `}` frees on the trap path too.

## 5. Findings against `spec/02-memory-model.md`

These are the reasons this document is a *proposal* and not a description.

### 5.1 `[mem.region.multiopen]` is incoherent as written (top severity)

> **Status: REPAIRED upstream at `ecea37c` (2026-08-09).** The clause now
> reads, verbatim: "the open set must be an **antichain in the region
> forest** — no open region may be an ancestor (owner, transitively via iso
> edges) of another open region. Distinctness of affine region values is
> *not* sufficient". `Store::enter` already implemented exactly this, so the
> machine needed no change; what changed is that it is now conformance and no
> longer a proposal. Verified against the normative text by
> `region::tests::an_ancestor_and_its_descendant_do_not_open_together`.

The clause discharged its own disjointness obligation like this:

> the open set is a set of *distinct region values*; since region values are
> affine (`[mem.region.create.2]`), two open handles are two regions by
> construction.

**Distinctness of affine values is not disjointness of footprints.**
`[mem.region.edge.iso]` explicitly lets one region own another. An owner's open
window reaches its child's data, which is precisely the aliasing Verona's
single-window rule exists to forbid. Two distinct affine region values in an
ancestor/descendant relation satisfy the clause and violate the property it is
trying to state.

*Proposed repair, one sentence:* the open set must be an **antichain in the
region forest**, so that no member is an ancestor of another. This machine
implements the repair (`Store::enter`), it costs one walk of the parent chain,
and it leaves every pinned multiopen litmus green, because those litmuses open
sibling roots, never an ancestor/descendant pair.

### 5.2 `[mem.region.open.1]` contradicts the corpus (top severity)

> **Status: REPAIRED upstream at `ecea37c` (2026-08-09).** The clause now adds
> that re-entering a region already open in the current scope chain
> (`in a { … }` inside `region a { … }`) is "**idempotent**": openness is
> depth-counted and is not a violation. That is what `Region::depth` already
> was. Verified by `region::tests::reopening_an_open_region_is_idempotent`.

The clause read:

> A region is **Open** (mutable) in at most one scope at a time.

`corpus/memory/region_multiopen_swap.lu` is pinned at `run(exit=0)` and is one
of the two files the clause itself flags for model checking. It writes:

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

### 5.3 The region "forest" has no region-to-region parent edge for the common case (medium)

`[mem.model.machine]` promises a forest of "regions with parent edges (≤1
each)", but `[mem.region.edge.iso]` locates the owning handle in "the region
value **or** the iso field holding it". A region value bound to a local is owned
by a **stack** slot, which is not a region. So lexically nested sugar blocks
create *no* parent edges and are siblings in the forest. Two consequences the
document does not draw:

1. `[mem.region.freeze.3]`'s "a region containing an open child region" is
   defined only for the iso-field case.
2. The disjointness hole in §5.1 is *reachable only* through an iso field,
   which is what makes the antichain repair cheap.

The machine records a parent edge exactly when a region value is stored into
region data, and re-walks the forest invariant after every such store.

### 5.4 E1006 has no dynamic counterpart in the closed trap vocabulary (medium)

The sprint asks for a "`shared` acyclicity **assertion** at strong-edge
creation, the dynamic counterpart of E1006". `[mem.shared.rc.2]` makes a strong
cycle a compile error, and `[mem.ub.defined]` lists it as an error and not a
trap. `[conf.trap.set]` is a closed vocabulary (twelve kinds since `deadlock`
landed, §10.5) and has no kind for it.

The machine therefore implements the check as a **trace assertion**, not a trap:
inventing a kind would extend a closed vocabulary, and reusing `region-fault`
would put this implementation's guess into a differential comparison. If the
acyclicity rule is to be dynamically enforceable, spec/02 must do for E1006 what
it already does for E1001/E1002 and state the kind, or spec/05 must open the
set.

### 5.5 `corpus/regions.lu` cannot reach its pinned `run(exit=0)` (high)

> **Status: FULLY repaired at `8b04edf` (2026-08-09).** The tail is now
> `if total == 4950 && config.limit == 42 { 0 } else { 1 }`. Frozen data is
> readable forever per `[mem.region.freeze.1]`, nothing unspecified is left in
> the file, and this machine runs it to `exit(0)`. The finding below is kept
> as the record of what the corpus said before the repair.

The file's `main` called `build_config()`, which was declared nowhere in the
file, nowhere else in `corpus/`, and was not in the ambient std stub (s13's
list). The other half of the same sentence was this tail:

```
if total == 4950 && frozen.get(config).is_valid() { 0 } else { 1 }
```

`frozen` was a frozen region value, and `region.get(x)` and `.is_valid()`
appeared in no pinned document. `&&` does not short-circuit past them, and
`total == 4950` was true, so every conforming implementation had to evaluate a
call whose meaning nothing specified. `check: run(exit=0)` was unsatisfiable,
and wolf-interp reported `unsupported` with `x-unsupported: "`region` has no
method `get` …"`. The repair took the second of the two options this finding
named: the tail now reads `config.limit`, which the documents define.

Not a wolf-interp bug and never adjusted here: the corpus is not the defendant.

### 5.6 `[mem.shared.rc.3]` fixes the *shape* of a failed upgrade but not the tag (low)

"upgrading yields an option-shaped result the caller must handle" names the
shape only. `corpus/memory/shared_ok.lu` handles it with a wildcard
(`else |_|`), so nothing observable turns on the choice. This machine yields the
tag `None`. A later std/option lock should pin it.

## 6. Deliberate approximations in *this* machine

Each one is a place where the interpreter is less precise than the spec, always
in the direction that cannot produce a spurious fault.

### 6.1 Only granules with identity can dangle

`[mem.model.value]` says values "have no identity beyond their current place",
and is02 implemented that literally: a wolf assignment is a Rust move and a wolf
`copy` is a Rust clone. Consequently an **aggregate cannot dangle**, because
copying it out of a region copies the data. The things that can dangle are the
ones the language gives identity to: region values, pools, `handle`s and
`shared` cells. Use-after-free is exact over exactly that set.

This is why the use-after-free litmus is written with a `handle`: at Tier 1 with
affine region values, there is no way to *write* a dangling plain reference. A
Tier-3 raw pointer is the other way, and it is is04's.

One consequence of the copy model used to leak past `[mem.region.freeze.1]`: a
write *through* a struct built inside a `freeze region { … }` landed on the
value-tree copy and executed, where wolfc rejects the program E1012
(wolf-interp#2, dynamic conservatism in the wrong direction). Struct values now
carry the region charged at their allocation site (`Value::Struct::home`;
`[mem.model.alloc]` lands in the current region), and `write_path` refuses a
write whose path passes through a container homed in a `Frozen` region. That is
the value-path half of the check the granule paths always ran, and it faults
`region-fault [mem.region.freeze.1]` before anything is mutated. Reads stay
legal forever. Rebinding the *binding* stays legal too, since it replaces what
the binding holds and touches no frozen storage.
`tests/faults/region_freeze_value_write.lu` and its twin
`ok/region_freeze_rebind_ok.lu` pin both directions. The remaining
approximation: only **struct** composites carry a home, so a bare list or map
frozen the same way still takes the write on its copy. No pinned corpus or book
program performs one, and the E1012 ⇄ `region-fault` pairing stays out of
`ledger::dynamic_meaning` until `[conf.trap.map]` states it (the E1004/E1005
precedent).

### 6.2 The edge check runs at the stores this machine has

§3 says "on **every** store of a reference". The destinations that are region
data in this machine are (a) struct-literal fields, checked against the current
region at construction, and (b) pool slots, checked against the pool's region at
`init`. Stack locals are not region data (`[mem.model.machine]`: "or *stack* for
locals"), so storing a region value in a local is unrestricted. That is what
makes a region value first-class. Growing a collection *in place* inside a
region is not yet a checked store, and no pinned corpus program performs one.

### 6.3 Reclamation is at scope exit, not at last use

`[mem.region.intra.2]` frees at "the last use of the region value" and
`[mem.shared.drop.2]` reclaims destructor-free values "any time after their last
use". Both are unobservable **except** through destructor timing
(`[mem.shared.rc.4]`), and the language has no user destructors yet. So
`[mem.shared.drop.1]`'s scope-exit point is the only observable one, and it is
the one implemented, LIFO, after the scope's `defer`/`errdefer`.

**When user destructors land**, the interleaving of a destructor-carrying value
with `defer` becomes observable and must be by registration order, not "defers
then drops". That is the open item in this section, and it belongs to this
machine.

*(Update, is17 — wolf-interp#35.)* A region whose VALUE rides out of the
dying scope — a block tail, a `return`, a `break` value — is not swept:
a return is a move and a region is affine (X4, `[mem.region.create.2]`),
so the adopting binding owns the free now (wolf_mem's s20 ret-region
shape; `corpus/memory/region_value_return.lu` runs to the compiled
lanes' exit). Only the region value transfers this way: a container
merely allocated in the dying region does not carry its home out, and
the freed-home fault (#25) still fires on any later access —
`region_escape_local.lu`'s family traps exactly as before.

### 6.4 Function parameters are not swept

A `read`-mode argument copies its value under MVS, so sweeping a parameter at
frame exit could free a region the caller still owns. Declining to sweep leaks,
which is defined and safe (`[mem.ub.defined]`). Sweeping would fault wrongly,
and §1 forbids that direction.

### 6.5 What is still `unsupported`, and why

`channel`/`Mutex`/`worker`/`acquire`/`release` have no pinned semantics, so
`corpus/memory/region_move_while_open.lu` (whose first statement is
`channel[region](1)`) never reaches its region check. The dynamic counterpart of
E1005 is implemented and exercised by `tests/faults/region_move_open.lu`
instead. Region transfer across procs is ic03's; the unsafe tier is is04's.

*(Update, is06: spec/03 pinned `channel`, and the file now runs and traps
`region-fault`. CI asserts it. The rest of the list stands.)*

### 6.6 Declared types do not coerce at struct construction (found is11, filed)

Passing a literal to an `int` parameter adopts the parameter's type
(`f(42)` where `fn f(x: int) -> int` yields an `i64`), but constructing a
struct does not: `C { limit: 42 }` with `limit: int` stores the literal's
own `i32`, observable at the REPL as `:type c.limit` → `i32`. Coercion is
the type checker's property and sema-lite takes signatures at face value,
so the inconsistency cannot produce a spurious fault. The two positions
still disagree with each other, which is a wart, not a choice. Filed
during the is11 sweep instead of changed: aligning either direction is a
behavior change, and the code that owns literal typing is the compiler's
half of the split.

### 6.7 Bare-ident patterns resolve against the value, not the type (0.1.2)

The checker resolves a bare identifier in a pattern against the scrutinee's
*type*: an in-scope enum variant is a variant pattern, a row tag of the
scrutinee's `!T ! {row}` is a tag pattern, anything else binds (issue #5,
wolf-std F-0007, where first-arm-always was the bug). This machine has no
types to consult, so it approximates with what it has:

- an identifier that names a variant of an enum **declared in the current
  module** is a variant pattern. It matches the payload-free tag spelled
  either bare (`Greater`) or enum-qualified (`Ordering.Greater`), and the
  same table lets a payload pattern `Rgb(r, g, b)` match a value built as
  `Color.Rgb(1, 2, 3)`;
- otherwise, a **capitalized** identifier whose scrutinee is a tag-shaped
  value (an error value) is a structural row-tag pattern (D30: rows need
  no declaration; the machine already reads unresolved capitalized names
  as tags in expression position). It matches on tag equality and never
  binds;
- everything else binds, including a capitalized name over a non-error
  scrutinee, which the counterparty also treats as a binding (observed at
  pin a0c4564: `match 3 { Zed => Zed, _ => 9 }` warns E0802 unreachable on
  the `_` arm and runs).

The residual imprecision: a capitalized *binding* over an error scrutinee
reads as a tag pattern here and fails to match. That is a name the checker
would resolve as a binding, because it is neither a variant nor in the row.
No pinned corpus, book, or wolf-std program spells one (the convention is
lowercase bindings), and the failure mode is an honest `unsupported` ("no
`match` arm applied"), never a wrong answer. Exhaustiveness stays the type
checker's (E0801 has no dynamic half): a match no arm of which applies is
`unsupported`, not a trap. Bare *dotted* path patterns (`Ordering.Less =>`)
are outside `[gram.pat]` and rejected at parse with the counterparty's
E0201 shape.

### 6.8 `for` holds a read claim on the container it iterates (D40, 0.1.8)

Ruled, after two pins filed as S-11: `for x in xs` holds a **read claim**
on the container for the loop's whole extent (ruling D40, 2026-08-12, the
resolution of wolf-interp#9 / wolf-std F-0014 / wolf-lang#15). The operand
is still evaluated once, at loop entry, and the loop iterates that value.
The new half is the claim. A mut use of the container inside the body
conflicts with the claim in the exclusivity machinery and is
`trap(exclusivity)` **at the mutation**. Such a use is `xs.push(x)`, an
element write, a whole-container assignment, or a `mut` pass. The trap
message names the loop and teaches the ruled fix-its (collect-then-apply,
or the index loop). wolfgang enforces the same rule statically as its new
E1013 (deliberately not the old accidental E1001-reads-as-moves story), and
`[conf.trap.map]` gains the E1013 row, so `[proto.cmp.rung]` reads the
static-fail/dynamic-trap pair as agreement.

The claim attaches when the operand is a side-effect-free place
expression naming a live `List`/`Map` (`xs`, `state.items`). A call
result, a literal, or an indexed element iterates a value with no
caller-visible place, and no claim exists to violate. Reads of the
container inside the body stay legal (read beside read). The D40 spec
text itself lands with wolf-lang s72. This machine implements the ruling
at 0.1.8 ahead of that pin, which is the 0.1.8 pass's noted drift.

### 6.9 Numeric casts convert; the float model is one f64 (0.1.3)

Issue #11 (wolf-std F-0022) found `n as f64` retagging instead of
converting: the value stayed an integer, silently. Since 0.1.3, `as`
between numeric types is a **conversion** in every direction. The
approximations are these:

- **Every float value is an `f64`.** There is no separate f32
  representation. `x as f32` converts through f32 precision and widens
  back, so the *value* is what an f32 would hold, carried in the one
  float shape this machine has. `16777217 as f32 == 16777216.0` reads
  true, and nothing distinguishes an "f32-typed" value afterward. Width
  tracking on floats is the checker's, like every other type.
- **int → float** converts exactly where f64 can represent the integer;
  beyond ±2^53 it rounds to the nearest representable, as the target
  demands. No trap: the conversion is total.
- **float → int** truncates toward zero and range-checks: NaN, the
  infinities and values outside the target's range trap `overflow` (X3:
  checked semantics in every profile, no silent saturation). The
  compiler currently refuses these dynamically ("int↔float casts, no
  conversion op yet" at its wir rung), so this is lupin executing ahead
  of the counterparty, not against it.
- **int → int** narrowing range-checks and traps `overflow`.
  `wrapping[T]` / `saturating[T]` *targets* reduce by their own mode
  instead, and the cast into a wrapping type is how intended overflow is
  spelled. The compiler defers narrowing ("range-check semantics, s27"),
  which is the same reading, unexecuted.
- **The non-bridges stay refused** (`unsupported`, mirroring wolfc's
  E0805): no truthiness (`bool as int`), no stringly casts (`int as
  str`). Adapter (`distinct`) casts stay free and bidirectional.

`tests/cast_matrix.rs` pins the whole matrix. Mixed-type *comparison*
(`1.0 == 1`) keeps its 6.7-era reading: distinct values, `false`. wolfc
rejects the comparison statically (E0401), which is the standing
conservatism class, visible in every differential and not a divergence.

### 6.10 `Iter` dispatch, row-tag patterns, and the `assert` intrinsic (0.1.3)

The s27 spec realignments, and the sema-lite depth each one gets:

- **`[mem.iter.for]`.** `for` over a non-builtin operand looks up `next`
  among impl-block methods of the operand's *struct type name*, and only
  an `impl Iter for T` block qualifies (`[mem.iter.impl]`: by name, no
  structural conformance). The drive loop is the clause's desugar
  verbatim, `var it = e; loop { let pat = (mut it).next() else { break };
  body }`, so *any* raise from `next` ends the loop, not only `done`,
  exactly as the bare `else` reads. Method dispatch is dynamic (by the
  receiver's runtime type). The s17 resolution *order* is honored
  (inherent wins, `Trait.method(x)` reaches the shadowed one), and trait
  default bodies remain `unsupported`.
- **Lowercase row tags** (issue #12). At a raise site, a bare lowercase
  name resolves against the enclosing function's *declared return row*
  (checked eagerly at resolve: an unresolvable tag refuses whatever path
  the input takes). In a pattern over a tag-shaped scrutinee, a lowercase
  identifier is a row-tag pattern iff it names a tag some signature of
  the module declares in a row. That is the sema-lite stand-in for the
  checker's row-typed resolution. An undeclared name still binds, which
  keeps `else |err|` a binder. A tag declared only in a *body-level*
  annotation is outside the vocabulary, so a false bind is possible there
  and accepted. The checker's half is exact.
- **`[conf.trap.assert]`.** `assert` is intercepted before argument
  evaluation. The intrinsic wins over any module-level `assert` (the
  clause's no-shadowing rule), and the two-arg form's message is evaluated
  **only** on the failing path, rendered as one line to stdout before
  the trap. A *local binding* named `assert` still shadows: binding
  names are the program's own scope, and the clause speaks only of
  library functions.

### 6.11 The X1 call-site mode law, integer-literal contexts, and `calloc`'s arithmetic (0.1.4)

The lupin v0.1.4 maintenance wave, three semantic repairs:

- **E1007 at resolve (issue #15).** A call whose argument spelling
  disagrees with the callee's declared parameter mode (`mut`/`take`
  missing, extra, or wrong) is rejected at the resolve rung, where
  sema-lite can see the signature: a bare name naming a function item of
  the current module, or `module.fn` naming a sibling module's. Code,
  span (the argument expression), and message shapes match the
  counterparty's. The reasoning is E0410's: running the disagreement
  computed a **silently wrong answer** (an unspelled `mut` argument
  passed by value and the writeback never happened), and
  `[conf.trap.map]` gives E1007 no dynamic meaning to trap with. The
  dynamic residue is a call through a function *value* whose declared
  mode the static tier could not see; it is refused (`unsupported`) at
  the call, never run wrong. Method receivers stay E0804's business
  (ledgered conservatism), and closures declare no modes this machine
  reads. Rung placement vs wolfc's `mem` emission is DIV-2026-011.
- **Integer literals meet their context (issue #14).** An unconstrained
  literal stays unconstrained through negation and literal-only
  arithmetic (checked at i128, the machine's computing width), adopts a
  concrete operand's type as before, and is typed by the declared return
  type of any call it comes back from. It meets `[arith.literal.default]`
  where it finally lands. A binding annotation types it and range-checks
  it (out of range traps `overflow`, the dynamic reading of the checker's
  E0401). An unannotated binding defaults it to **i32** and range-checks
  the same way (`var k = 0` is i32, and wolfc agrees, to the WIR
  constant). Assignment into an existing place adopts the place's type,
  range-checked. Net: `-9223372036854775808` is writable in every
  annotated spelling, and `int_max() - 1` is `int` arithmetic wherever
  `int_max` lives. **Containers joined at 0.1.7 (issue #21, the #53
  mechanism):** a `List` carries its element checking context, which is
  either `List[i32]()`'s bracket argument read off the constructor's
  syntax or a `List[T]` annotation through `coerce`. A pushed literal
  therefore adopts the element type (range-checked at the push; out of
  range traps `overflow`), element loads feed checked arithmetic at the
  element's width, and the compound `l[0] *= 2` is checked at that width
  BEFORE the write lands. A container with no context gives a pushed
  literal `int` (64-bit, locked), like every other literal meeting a
  concrete home. It never gives the i32 default, which is the
  *unannotated binding's* rule and not the container's.
- **`c.calloc(n, size)` is `n * size` bytes (issue #13).** The modelled
  C heap allocated `n`. Real glibc through s29's native rung disagreed,
  which made this the first soundness candidate the native differential
  produced, and a lupin bug. The multiplication is overflow-checked.
  Real calloc reports that overflow by returning NULL, and the model has
  no null-returning surface pinned, so the overflow case is
  `unsupported` instead of an invented block. `malloc`/`memset`/`memcpy`
  take one size or an explicit length and were audited correct.

### 6.12 A write through a read-mode parameter traps (D39, 0.1.8)

`[mem.tier0.mode.read]` always said the callee "reads a value that is
immutable for the whole call". The gap was enforcement (ruling D39,
2026-08-12, wolf-lang#27's dynamic half). This machine used to run such a
write against the callee's own copy, and MVS made it invisible to the
caller and therefore silent. Now every call frame carries its read-mode
parameter list, and `write_path` traps a write whose base resolves to one,
with kind **`exclusivity`**. Whole-parameter stores, projections
(`p.x = 9`), compound assigns, and a mutating method's receiver write-back
all trap alike. `exclusivity` is `[conf.trap.map]`'s family for mode
violations: `[mem.tier0.excl.1]` already gives every read/write conflict
that kind, and D39 names no new one. The trap's second span is the
parameter's declaration, and the message teaches `mut` plus the X1
call-site spelling. A body-scope local shadowing the parameter's name is an
ordinary local and writes freely. wolfgang's static half (a new
memory-family code, s72) is the same rule at the other rung, and the
caller-side overlap half (`f(mut a, a.x)`) was already trapped here and
stays. The D39 spec text lands with s72; this machine implements it ahead
of the pin on the ruling's authority, which is the 0.1.8 pass's noted
drift (with §6.8's D40).

### 6.13 Containers are outside the region story on this machine (s76, 0.1.11)

Found by the 0.1.11 re-pin's first probe, and the one place where the
s74…s78 wave left this machine behind rather than confirming it.

s76 moved the compiler's containers *into* the region story: a `List`
allocates in the **ambient region at its allocation site**, dynamically
scoped per D12, so a callee allocates into its caller's region and a
container built inside `region r { }` is freed with `r`. Before s76 a
container "opted out of the region story entirely", which is still this
machine's posture.

On every **defined** shape the two machines agree exactly, which is the
useful half of the finding: a container built in a region and freed with
it, a callee allocating into its caller's region, growth across several
region chunks, `freeze` letting a container outlive its building block
(`[mem.region.freeze.1]`), and nested regions all produce identical
answers here and on `--native`/`--release`. This machine models regions
dynamically and always placed a callee's allocation in the ambient
region, so s76 moved *toward* this reading.

The gap is the **escape**. Once a container is region-allocated, a
container that outlives its region is a dangling pointer, and keeping it
out is the region checker's job — E1010, which the compiler emits on
every lane for `memory/region_escape_container.lu`. This machine does
not make that static judgement, which alone would be ordinary
conservatism (`ledger::dynamic_meaning`'s territory, and the corpus walk
scores the file that way). What makes it worth declaring is that this
machine does not catch it **dynamically** either: reading through the
escaped container after its region closes answers with the old values
rather than trapping, where the handle/pool escape one tier over
(`tests/faults/region_uaf.lu`) correctly traps `region-fault`. The
region machine's granules-with-identity rule (§6.1) is why — a `List`
here is an ordinary value, not a region-homed granule, so it never
acquires the identity the dangle check needs.

Consequence, stated honestly: **no conforming program can observe this**,
because the compiler rejects the shape statically and the corpus pins
that rejection. It is not a divergence and it does not gate. It is a
place where this machine would fail to be an oracle if the compiler's
static check were ever wrong, which is exactly the situation the
differential exists to catch — so it is recorded rather than left
implied.

### 6.14.1 The escape, re-measured at `4e316ad`, and the design that closes it

wolf-interp#25 named this as "containers lack identity in the value
model". Measured again at this pin, that diagnosis is **half right**, and
the other half is the more useful one:

```console
$ lupin run upstream/corpus/memory/region_escape_container.lu   # a List
exit=0
$ lupin run upstream/corpus/memory/region_escape_local.lu        # a struct
exit=0
```

The struct sibling escapes too — and `Value::Struct` **already carries
`home: Option<RegionId>`**, stamped at its allocation site. So the
container's missing `home` is not the whole reason the escape goes
uncaught; the deeper reason is that **nothing ever checks a tier-0
value's `home` against its region's liveness**. `home` today has exactly
one reader, `Machine::frozen_container`, and it asks one question —
`[mem.region.freeze.1]`, is this write into frozen data — on the *write*
path only. `RegionState::Freed` is consulted for pools and handles
(`check_pool_region`) and never for a value. `region_escape_local.lu`'s
own header says "dynamically this is a region-fault after the free"; this
machine does not raise it.

That makes the fix a bounded, three-part change rather than an open
question, and it is written down here so a later sprint can execute it
without re-deriving it:

1. **Give `List` and `Map` a home.** `Value::List(Vec<Slot>,
   Option<IntTy>)` and `Value::Map(Vec<(Value, Slot)>)` gain
   `home: Option<RegionId>`, stamped by `builtin::call`'s `List`/`Map`
   arms from `Machine::current_region` exactly as the struct literal's
   is. This is the mechanical bulk of the work — `Value::List(..)` is
   matched in scores of places — and is the reason this is a sprint, not
   a patch.

2. **Give `home` a second reader, on both paths.** Generalize
   `frozen_container` into a `home_regions(path)` that returns every
   region reachable along the path *including the leaf value's own*
   (today it walks proper prefixes only, and returns `None` outright for
   a projection-free path, which is why bare `keep` could never be
   caught). Then consult it from `read_claim` as well as `write_path`:
   a home in `RegionState::Freed` is a `region-fault` under `Rule::RegionFree`,
   carrying the region's creation span, worded like `check_pool_region`'s
   existing "freed wholesale; the slot died with the region". The frozen
   and suspended answers stay write-only and unchanged.

3. **Keep identity out of equality — both equalities.** The language's
   `==` runs through `value_eq`, which already discards `home` via `..`
   and must keep doing so. The *derived* `PartialEq` on `Value` does
   **not**: it compares `Struct`'s `home`, and adding the field to
   `List`/`Map` would extend that to containers. That derived impl is
   what `eval_method`'s copy path compares a receiver against to decide
   whether a method wrote (§`Lend`), so a home-only difference would
   register as a write and trip `[mem.region.freeze.4]`. Hand-write
   `PartialEq` for the three variants, as `ErrorValue` already does for
   `enum_variant`, so the home is never a value's identity to anyone but
   the region checker.

The cost to be budgeted is not the code, it is the **re-differential**:
this adds new dynamic faults, so every corpus entry that builds a
container inside a region has to be re-compared on all three counterparty
tiers, and any program that legitimately reads a container after its
region closes — there should be none — becomes a finding to triage.

**Decision, this pass: NOT DONE — a named design task, deliberately.**
It is a change to §6.1's granules-with-identity rule, it changes the trap
surface, and this pass already moved the pin and rewrote the method-call
path for wolf-interp#24. Landing all three together would make any
resulting divergence un-bisectable, which is the one thing an oracle
cannot afford.

### 6.15 The byte ledger's units are a model, and the model is written down (is32, 0.1.21)

`[mem.region.account.1]`/`[mem.region.account.2]` (s131, wolf-lang#187) give
wolf `region_bytes(r)` and `live_region_bytes()`. The clause pins four
*relations* — zero at creation, monotone within the lifetime, stable between
allocations, and (for the live total) wholesale disappearance at a free — and
leaves the **units** per tier on purpose: "what charges, and by how much, are
implementation-measured facts per tier, **not comparison surface**".

That carve-out is what makes the surface implementable here at all. There is
no arena in this machine: §6.1's value model makes a `Value` a plain owned Rust
tree, so the bytes a wolf program occupies are the Rust allocator's business,
neither stable across builds nor meaningful to the program. So the ledger
counts a **model** of the storage the same program would take in a compiled
arena (`src/eval/region.rs::ledger`):

| charge | model |
|---|---|
| grain | every charge rounds up to **16 bytes** |
| allocation header | **32 bytes**, on every allocation site |
| value slot | **16 bytes** per struct field or container element |
| `str` payload | its UTF-8 length in bytes |
| container capacity | powers of two from **4** slots up, this machine's own growth policy — not Rust's `Vec` |
| growth | a capacity step charges the **whole new buffer**; the abandoned one is never discharged |

Two consequences worth stating out loud:

1. **It is a high-water accounting, not an occupancy one.** It answers "how
   much storage has this region been asked for", which is exactly the monotone
   quantity the clause guarantees and the quantity wolf-lang#187's customer (a
   per-region memory budget) wants. It is never an address, never a placement
   (`[mem.region.promote.1]` is untouched), and never an RSS proxy.
2. **`str` charges here, and does not charge on the counterparty's native
   tier.** wolf-lang#191 (the c09 seam) is recorded *in the clause*: native
   realizes a `str` materialization's ambient region as the process root, so
   string bytes appear in no named region there today. This machine charges a
   `str` allocation to the ambient region like any other. That is a units
   divergence, which the clause rules out of comparison, and the clause's own
   words anticipate it — "programs must not read this clause as `str` never
   charges". The two witnesses print relation *booleans* precisely so the
   three lanes compare where they agree.

`live_region_bytes()`'s granularity here is the exact charge: the sum of every
unfreed **named** region's ledger. The process-root arena is never counted
(the clause's letter), and a `Frozen` region keeps contributing because
`[mem.region.freeze.1]` means it is never freed — §4's leak reading, applied to
the counter.

The **cap** half of wolf-lang#187 (D68: a cap breach is `trap(alloc-contract)`
at the allocating site, contained at the proc boundary) is *not* implemented
here. It is deferred by name to the is33-era twin: no `Region.cap`, no cap
syntax, and no clause on wolf-lang trunk to implement against at this pin.

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

One access can be true of several rows at once. A read past the end of a freed
allocation in a dead region is P3, P4, P1 and L1 simultaneously. `[mem.ub]`
enumerates the rows but does not order them, so this machine picks, and prints
the pick instead of hiding it:

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
on one side is never a divergence. Two oracles that both emit it and disagree
*is* one, and this table is what triage would consult first.

### 7.2 §7 has no row for a protector violation, so P1 carries it

`[mem.prov.state]` creates a UB condition §7 does not enumerate: "Protected tags
escalate the foreign-write transition to immediate UB for the protection's
duration." The write itself is not an access through a Disabled tag (P1's
wording), a write through a Frozen tag (P2's), or anything else in the closed
table. It is the *invalidation* of a live protected borrow.

This machine reports **P1** and says so in the message. The reading is that P1's
parenthetical, "use of an invalidated borrow", is the row the protector exists
to make immediate: without the protector the tag would become Disabled and the
UB would arrive at the borrow's next use, which is P1 by any reading. The
licensed optimization agrees: P1's O1 is "`mut` params lower to `noalias` +
`dereferenceable`", and the protector is what licenses the call-extent half of
that.

**Finding, low severity:** the enumeration is closed (`[mem.ub.closed]`), so a
UB condition stated in §6 that no §7 row names is a gap in the closure argument.
Either P1's wording should say "…or the invalidation of a protected borrow", or
a row should be added with its own pairing. The protector-form program retired
at pin f0da6e6, because its `*u8` parameters are outside the language (E1302,
`[mem.unsafe.scope]`). It survives inline and frontend-free in
`tests/prov_machine.rs::the_retag_then_opaque_call_program_demonstrates_the_protector`,
which asserts the P1 verdict and the traced protector.
`corpus/memory/unsafe_ub_uaf.lu` is P1's pinned witness, and
`tests/ub/ok/p1_read_before_free_ok.lu` is the near-miss twin with the read
moved before the free, which is defined.

### 7.3 A `read` parameter's Frozen tag is a witness, not the callee's path

`[mem.prov.tag]` makes parameter entry a retag point for both modes. Under MVS
(`[mem.model.value]`) a `read` parameter is the **callee's own copy** of the
value, not a second name for the caller's place. Binding the callee's parameter
place to the caller's Frozen child would model an aliasing that the language
does not have, and any write to the parameter inside the callee would report
§7/P2 on a *safe-tier* program.

So: `mut` binds (the callee genuinely writes through the borrow, and
call-by-value-result writes the result back), `read` does not. The Frozen child
still exists, still protected for the call's extent, as the **witness** of the
caller-side promise `[mem.tier0.mode.read]` makes. A foreign write during the
call violates that promise, and O2's load-hoisting is licensed by it.
`corpus/memory/prov_holy_grail.lu`'s trace shows it. (Since 0.1.8 the
callee-side write itself is unreachable anyway: D39's write barrier traps it
`exclusivity` before any slot is touched. See §6.12.)

Raw pointers are different and simpler: their tag travels *in* the value, so
both modes retag the pointer and the callee's accesses genuinely go through the
child. That is why `tests/ub/p2_frozen_write.lu` is a raw-pointer program.

### 7.4 Tier-0 places are tagged lazily, and their tags form a forest of stumps

A place gets an allocation the first time something retags it, and reads and
writes of an untagged place cost nothing. Two consequences:

1. **`a` and `a.x` are unrelated in the provenance forest.** Their place keys
   differ, so they get different allocations, and a write to `a` does not
   invalidate a borrow of `a.x`. This *under*-approximates: it can only miss a
   violation, never invent one. Tier-0 exclusivity (`[mem.tier0.excl.1]`,
   is02's `AccessSet`) already decides that case exactly, by the same
   prefix rule `[mem.model.path.disjoint]` states.
2. **A place's storage is stack, never region data.** `[mem.model.machine]`
   locates an allocation's owner at "a region (or *stack* for locals)", and a
   local is the second; giving place allocations a region owner would fire
   §7/P4 on ordinary safe-tier reads after any region free. Likewise their bytes
   are born initialized, because a Tier-0 place's initialization is `Slot`'s
   business and reading a moved-from one is `trap(use-after-move)`, a *defined*
   execution (`[mem.tier0.move.2]`), and not §7/L1, whose wording is "via raw
   pointers".

Together these are why `[mem.ub]`'s "safe-tier programs cannot reach any row" is
true of this machine **by construction** and not by luck, and
`run_corpus::unsafe_free_corpus_programs_never_produce_a_ub_verdict` asserts it
over every unsafe-free program in the pinned corpus.

### 7.5 Dead tags are pruned

The sprint permits "dead-tag pruning only if tests need it", and they do: the
tree walk is per-access, so a loop that passes one place a thousand times would
otherwise leave a thousand dead siblings for every later access to walk. A tag
is pruned when it is not a root, not protected, has no children, is bound to no
place and is not exposed. At that point nothing can observe it again, so
removing it changes no verdict. Pruning happens at scope exit and at the end of
a call, which are the points where the extents that hold unbound tags end.

### 7.6 Addresses are implementation-specified, and that is a protocol fact

`ptr as int` has to produce *something*. This machine lays allocations out at
`(id + 1) * 0x1_0000` and reports offsets into that. It is deterministic,
platform-independent, and deliberately not a real address.
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
tag to be a child *of*. The Tier-2 rules that matter dynamically are
`[mem.shared.handle.2]`'s generation check and `[mem.shared.handle.3]`'s
exclusivity. is03's machine enforces both exactly and produces traps, not UB.
Recorded as unimplemented instead of left as an absence.

### 7.8 `assume noalias` is checked where it is written, not carried as a contract

`[mem.unsafe.raw.2]` says the assertion holds "for the assertion's scope". This
machine evaluates the operands, compares the ranges, and reports §7/P5 there and
then. That is **exact** for every shape the corpus and `tests/ub/` contain,
because a raw pointer is a value and the two operands are fully known at the
statement. What it does not do is re-check the assertion after a later
assignment: `var p = …; assume noalias p, q; p = q` would evade it. Recording
the ranges for the scope and re-checking each access is the precise reading, and
it is the one to implement when a program needs it. Nothing pinned does yet, and
the assumption list is already kept (`Provenance::assumptions`), so the check has
somewhere to live.

The sprint file names an anchor `[ub.assume.noalias]` for this row. No such
anchor exists in the pinned `spec/02`, which puts it at §7/P5 and states the
rule at `[mem.unsafe.raw.2]`; those are what the machine cites. Recorded so the
mismatch is a known one and not a citation this repo invented. The s68 mining
pass confirmed it (issue #19): the clause is `[mem.unsafe.raw.2]`, full stop.
**W1302** is the compile-time face of the same hole. Since 0.1.6 the lint pass
warns at a whole-name reassignment of a standing `assume noalias` operand
(`corpus/lints/assume_reassigned.lu`, counterparty span parity), which is the
`p = q` evasion this paragraph describes. The warning lands at the assignment;
the dynamic re-check remains unimplemented as stated.

## 8. The C library, modelled: the host-intrinsic approximation

`corpus/memory/unsafe_noalias.lu`, `corpus/memory/unsafe_ub_uaf.lu` and
`corpus/ffi.lu` open with `import c "stdlib.h"` and then call C. This
interpreter has no FFI, and adding one is ruled out: `unsafe_code = "forbid"`
is in `Cargo.toml`, and an interpreter that dlopen'd libc would be comparing the
*host's* allocator against the compiler's, an observation the protocol already
says is not a comparison surface.

**The modelled set is closed:** `c.malloc`, `c.calloc`, `c.free`,
`c.memset`, `c.memcpy`. A C name outside it resolves (so the failure is
"unsupported feature", never "unknown name") and then declines with a reason,
by the same two rules `src/eval/builtin.rs` states for the std stub.

What the model claims, clause by clause:

| behaviour | clause | modelled as |
|---|---|---|
| `malloc(n)` yields a live allocation of `n` uninitialized bytes | none | a provenance allocation with `init` all false, so an unwritten read is §7/L1 |
| the allocation belongs to a region | `[mem.boundary.ffi]` "a C call executes against an implicit region borrowed for the call's extent" | owned by the region current at the call, so `[mem.prov.region]` decides what a region free does to it (§7/P4) |
| the pointer C hands back is wildcard-shaped | `[mem.prov.expose]` "Wildcard pointers from FFI behave as exposed" | the root tag is exposed at creation, so a later int→ptr resolves to it |
| passing a pointer *to* C exposes it | `[mem.prov.expose]` | `expose_to_c`, and the call is a foreign havoc that angelic resolution already models |
| a C call is not a wolf call | `[mem.prov.tag]` (retag is at *parameter* entry) | **no retag** on C arguments. Retagging them would invent a `read` borrow and then report `memset`'s own write through it as §7/P2, a spurious verdict on `corpus/ffi.lu` |
| `free(p)` ends the allocation | `[mem.prov.region]`, by analogy | the whole tag tree is Disabled; a later tagged access is §7/P1, a later wildcard access is §7/L2 |
| a double free, or a free of an interior pointer | none | §7/L2: `free` dereferences the block it releases |
| never calling `free` | `[mem.ub.defined]` "Memory leak … defined, safe" | reported on `Run::host_leaks`, never faulted |

**What this is not.** It is not a claim about any real libc, and it is not a
claim that these are the semantics the compiler must reproduce. s22 links
against a real C library, which has differences this model cannot have: real
`malloc` can fail, and real `memcpy` on overlapping ranges is UB where this one
is not. The claim is narrower, and it is the only one the oracle needs: *given*
the allocation events, the provenance consequences are the ones §6 states.

`corpus/ffi.lu` executes its whole unsafe block under this model (allocation,
`memset`, the raw store, the read, the free, with exposure traced) and then
reports `unsupported` at the inline `asm` block, whose meaning no pinned
document gives. Declining there is the same rule as declining `c.strlen`.

## 9. Findings against `spec/02-memory-model.md` §5–§7 (is04)

### 9.1 §6's state set and the s04 sketch disagree on a name (low)

`[mem.prov.state]`'s table names the states `Reserved | Active | Frozen |
Disabled`. The is04 sprint file's own sketch draws `Reserved → Unique → Frozen →
Disabled`. The sprint says s04 is normative and that "the machine and the spec
must agree on the state set exactly", so this machine implements `Active` and
the sketch's `Unique` is the stale spelling. Recorded because a reader coming
from the sprint file looks for `Unique` and does not find it, and because
`ReservedIM`, which the sketch offers conditionally, is correctly **absent**:
`spec/02` admits no interior-mutability type, so the state is omitted and this
sentence is the spec saying so.

### 9.2 A *child read* activates a Reserved tag, which is unusual (low)

`[mem.prov.state]`'s Reserved row reads `→ Active` under **both** child read and
child write. Tree Borrows as published leaves Reserved alone on a child read and
activates only on a write. The difference is observable in principle (a child
read followed by a foreign read would Freeze an Active tag but not a Reserved
one). This machine implements the table **as written**, because the table is the
normative artifact and the two-phase property the corpus pins
(`prov_two_phase.lu`) turns on the *foreign*-read row, which is unaffected.
Flagged so that a later alignment with the paper is a deliberate amendment and
not a silent divergence.

### 9.3 §7's `Detected` column promises `Q` for rows this tier cannot reach (informational)

Five rows are marked `O, Q`: the oracle *and* the D21 debug quarantine
allocator. This machine is the `O`. `Q` is s2x's, and nothing here claims it.
Listed so the coverage matrix in `tests/ub_coverage.rs` is not read as a claim
about the quarantine allocator's coverage.

### 9.4 §7/T2 is unreachable at this tier, and says so (informational)

"Torn write producing a partially-updated wide value observed through another
tag" needs a second observer of a store in flight. This machine runs at most one
task at a time (§10.1) and every store is whole-value (`[mem.model.value]`), and
the language surface has no split or volatile wide-store form to spell a tear
with. The row is therefore listed as `Coverage::Unreachable` with that reason
instead of silently absent, and ic03's interleavings are what make it reachable.
§7/C1 carries the sprint's `deferred(concurrency)` mark for the same reason, one
campaign further out.

## 10. Deliberate approximations in the **concurrency** machine (is06)

**Status:** drafted by is06 at pin `67c977f`, the first executable test of
`spec/03-concurrency.md`. **Revised by is08 at pin `843174f`**, whose s20
S-batch turned findings S-1..S-8 into clauses (see §10.5 for the
confirmations and the two realignments). This section records what *this
machine* chose where the spec left room, and why every choice keeps the
one-way approximation direction.

### 10.1 One task at a time, by construction

Tasks live on OS threads only because a suspended tree-walk needs a call
stack. A per-task gate serializes them so **at most one task ever runs**.
Every schedule decision flows through one seeded generator and is a
numbered `sched-ev/0` event (`[conc.det.events]`); the same seed replays
the identical stream (`[conc.det.seed]`). Seed 0 (and the unseeded
default) is strict FIFO. There is no `unsafe` anywhere in the machine,
because the crate forbids it, so the determinism claim rests on the baton
and not on memory-order reasoning.

### 10.2 Captures copy; cross-task shared mutability is (almost) inexpressible

Closures capture **by value** (`[gram.expr.closure]`), and that decision
does the heavy lifting: a spawned task writes its own copies, globals are
snapshotted per task, region transfer is checked at the send, frozen data
is immutable, and `Mutex` payloads move through the scheduler. The shapes
the compiler rejects statically (E1101/E1102) therefore mostly *cannot
misbehave* here. Through 0.1.5 they ran, with task-local effects (the
then-standing conservatism class; `conc/store_buffer.lu` was the exemplar,
and `conc/freeze_publish.lu`'s reliance on the forbidden shape was
DIV-2026-008). Since 0.1.6 (the 13b811f re-pin) the *visible* shapes are
rejected at this machine's resolve rung with the counterparty's codes and
spans: E1101 (a task writes a captured bare name; `when` bodies exempt),
E1102 (a spelled `List`/`Map` channel payload), E1103 (lexically nested
`when`). Capture-by-value remains the runtime semantics for everything the
static walk cannot see, which is the sema-lite posture: track what is
visible, never guess. The two memories two tasks CAN share mutably are
**pool slots in an unmoved region** and **raw allocations**, which is
exactly `[conc.mm.race.1]`'s reachability. A vector-clock race detector
watches both and traps `race` (`[conc.mm.race.3]`) at the conflicting
interleaving the schedule realized. Detection is per byte range (raw) and
per slot (pools): no over-approximation, so the forbidden direction
(faulting a compiler-accepted program wrongly) stays closed.

The silent-write-loss face of this choice is a task closure writing its
captured copy while the enclosing state never sees it. It was routed
upstream as **S-10** (divergence log; wolf-interp#4): spec/03 states E1101
as a compile error only, `[conf.trap.map]` gives it no runtime meaning, and
this machine does not invent one. The realignment happened at pin `13b811f`
(0.1.6, the #41 capture-law wave). The pinned fail-files carry the
rejection, and this machine produces E1101 statically at its resolve rung,
so a program that would silently lose writes is rejected before it can run.
That retires the observable half of S-10 for the pinned corpus. The clause
still states no *runtime* meaning, so the dynamic question stays open
exactly as filed.

The SAME-task face closed at is17 (wolf-interp#36). The compiler's closure
env BORROWS its captured places (the s98 loans, `[abi.native.closure]`),
so a write to a captured binding while the closure is still needed is
fail(E1002) upstream — and through 0.1.13 this machine ran that program to
its stale-read answer, the one observation that can tell copy-captures
from loans apart. Since 0.1.14 every closure records a loan per place its body
actually uses (frame-serial + name + write generation), and a call on the
capturing task whose loan generation has moved traps `exclusivity` at the
use, naming the write. Checking at the use rather than the write is what
keeps the forbidden direction closed: "still needed" is a *fact* at a
call and a guess at a write, so W1102's advisory shape — a write after
the closure's last use — still runs, and no compiler-accepted program
faults. Capture-by-value remains the mechanism; the loan check is what
makes the copy unobservable, which is the property `[abi.native.closure]`
actually pins. Task closures stay under D14's copy law and E1101 above —
their loans are exempt by task identity.

### 10.3 The killed-proc sequence, and what "no user code" means

`[conc.proc.kill]` runs in order: tasks are marked killed and unwound with
a dedicated signal that **skips every `defer`/`errdefer`**; the proc's own
region bulk-frees; reasons deliver. The machine's scope-exit *sweep*
(refcount decrements, idempotent region frees) still runs on the unwind.
It is bookkeeping, not user code, and skipping it would leak cells the
proc never owned. A task's ambient region at spawn is the spawner's
current region; a proc's is its own fresh region (`[conc.proc.1]`).

### 10.4 Blocking points and the wakes they surface

Cancellation and kill are delivered **only** at the closed set of
runtime-owned blocking points (`[conc.cancel.points]`): channel send/recv,
`select`, `when` acquisition, scope join, timer wait. Cancellation
surfaces as an error value (`Cancelled`) that ordinary returns carry, and
defers run (`[conc.cancel.defer]`). A `--checked` build's function-entry/
back-edge polls are not implemented (no `--checked` profile exists here).
`checkpoint()` is not implemented (no pinned std surface). C intrinsics
complete synchronously, so `[conc.cancel.c]`'s "next safe point after
return" degenerates to the next blocking point, which the trace notes when
a concurrent program crosses the membrane.

### 10.5 Choices the spec left open, now adjudicated by the s20 S-batch

**Status update (is08, pin `843174f`):** spec/03 wrote the clauses this
section anticipated. Where the S-batch **confirmed** a choice, the rule now
cites the registered clause instead of a forward namespace or a family
root; where it **deviated**, the machine realigned. The ledger:

- **Scope join under cancellation keeps joining.** *Confirmed and
  extended* by `[conc.task.fail.owner]`: the scope is the cancellation
  unit, owner included (the Trio posture, adopted from finding S-4). A
  failing child now cancels a scope owner blocked at a non-join blocking
  point (`Rule::TaskFailOwner`); an owner blocked at the *join* keeps
  joining, exactly as before. The owner's surfaced cancellation error is
  its finished cancellation, and the child's failure still re-raises at
  the scope exit, after the join.
- **Drained-closed channels make `select` arms ready.** *Confirmed*:
  `[conc.select.closed]` adopts Go's posture from finding S-8 verbatim.
  `Rule::SelectClosed` cites it at both readiness sites.
- **Deadlock.** *Deviation paid as specified*: `unsupported`-with-roster
  is retired. `[conc.deadlock.def]` makes all-blocked/no-timer a defined
  outcome, `[conc.deadlock.trap]` adds `deadlock` as the deliberate
  twelfth `[conf.trap.set]` kind, and the machine traps it with the
  blocked-task roster in the message. Detection is *required* in
  deterministic test modes. This machine and the is07 explorer are both
  such modes, and the explorer's per-schedule verdict is
  `trap(deadlock)`.
- **`when` payload write-back.** *Confirmed*: `[conc.when.body]` states
  the rebind/write-back/reverse-release semantics the machine chose.
  **Nested `when`** is a *deviation paid as specified*: the spec split the
  stopgap `assert` trap into E1103 (lexical, the compiler's) and
  `trap(deadlock)` for the dynamic already-held case
  (`[conc.when.nonest]`, `[conc.deadlock.self]`). The machine keeps the
  per-task held-set and traps `deadlock` only on a genuine
  self-acquisition. A `when` reached through a call over a *disjoint* set
  proceeds, because the compiler accepts that program and the old blanket
  nesting fault would have broken the one-way approximation direction.
- **Region transfer over channels.** *Confirmed*: `[conc.chan.move]`
  (the send is the affine move; closed and disconnected, dynamically
  re-checked at the send), `[conc.chan.staleuse]` (the sender's fault, at
  the use site) and `[conc.chan.imm]` (by reference, no move, no copy)
  are what `Rule::ChanMove`/`ChanStale`/`ChanImm` cite now.
- **Proc surface debts paid** (S-6/S-7): `w.cancel()` implements
  `[conc.proc.cancel]` (cooperative, defers run, `cancelled` reachable at
  last), `a.link(b)` implements `[conc.proc.link.pair]`, and the root
  domain's death is `[conc.proc.root]`: the killed-proc sequence for every
  live proc, then a nonzero, implementation-specified exit (1 here;
  compare the class, never the number).
- **Monitor of an already-exited proc** delivers immediately; the reason
  value keeps its label but a `normal` payload observed this way collapses
  to unit (the label is what the corpus and tests compare). Unchanged,
  since the S-batch does not speak to it.
- **Exclusivity and borrows stay per task.** The access set that enforces
  `[mem.tier0.excl]` is task-local; cross-task exclusivity has no dynamic
  meaning here because cross-task mutable paths do not exist (10.2).
  Unchanged.

### 10.6 The schedule explorer (is07): what it proves, what it approximates

**Status:** is07, pin `79ceec6`; admission unified at 0.1.7 (issue #22).
The explorer (`src/explore.rs`, `conform-run --explore=N`) is
stateless model checking over the reified `sched-ev/0` stream: replay
from the root with a forced decision prefix, classic Flanagan–Godefroid
DPOR with sleep sets, full branching over `select`-arm commits, budgets
for schedules/steps/preemptions/wall clock. **A program must clear the
same admission ladder `run` clears** (`frontend::admit`: module laws,
the E11xx/E0004 statics, the raise check). A statically rejected program
has no schedule space, and before 0.1.7 the `--explore` door bypassed
those checks and certified programs "observably deterministic" that the
same binary refuses to run. The choices it rests on, named:

- **The branch alphabet is two decision kinds.** Every schedule point of
  is06's enumeration funnels into `State::decide` at exactly two places:
  the ready-task pick and the `select`-arm pick. Channel pairings, `when`
  grants, timer fires and deliveries are *deterministic consequences* of
  those picks in this machine (FIFO queues, sorted timers), so permuting
  the picks covers the whole space. The completeness assertion checks
  this on every replay, and the red test proves the check bites
  (`tests/explore_machine.rs`).
- **Conflicts are syntactic over runtime objects.** Two ops conflict when
  they touch the same channel/mutex/proc/task-control/scope/memory key and
  one mutates. This over-approximates (two buffered sends to a non-full
  channel "conflict" even when the program never observes order), which
  costs exploration and never soundness. One refinement is load-bearing:
  a task's *successful* scope-exit is a read of its scope (completion
  order of non-failing siblings is unobservable, and `[conc.task.fail]`
  orders only failures), while a failing exit writes it. Without this the
  independent-writers reduction would collapse.
- **The canonical state hash cannot see suspended frames.** It covers the
  scheduler-visible state (tasks + queues + wakes + clocks + open stacks,
  channels with contents, mutexes, procs, scopes, timers, race-detector
  memory, virtual clock) plus a rolling stdout digest. It does not cover a
  task's Rust call stack or the region store's interior. Convergence
  pruning on that hash could in principle merge schedules whose difference
  lives only in un-communicated locals. `--explore-no-prune` removes the
  risk, `--paranoid` re-verifies every hit against the stored preimage
  (the collision guard), and on the pinned corpus pruning changes no
  conclusion (asserted in tests).
- **"Preemption" means a non-FIFO pick.** The machine is cooperative, and
  tasks run to their next blocking point, so the CHESS bound maps to
  "decisions that depart from the front of the ready queue".
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
  any relaxed-memory exploration (SC only, because `[conc.mm]`'s
  unsafe-tier relaxed orderings wait for stable clauses).

## 11. Observability

`--trace=mem` logs every region event: create, open, close/suspend, freeze,
free, edge checks, ambient allocations, RC operations, handle faults. Each line
names the rule and its clause anchor. The filter is the anchor's namespace, not
a hand-kept list, so it cannot drift from the rule registry.
`tests/region_machine.rs` asserts that `--trace=mem` is exactly the `mem.*`
subset of `--trace`, and that each of the five D3 optimizer-fact witness
programs (`tests/witness/`) cites the rules that license its fact.

`--trace=prov` is the same mechanism one tier up. The Tier-3 subset is §5
`mem.unsafe.*`, §6 `mem.prov.*`, §7 `mem.ub*`, and `[mem.boundary.ffi]`, so
every retag, permission transition, exposure, protector, angelic resolution and
UB row is on it, and nothing else is. `tests/prov_machine.rs` asserts it is
exactly that subset of `--trace`, computed from the rule registry instead of
listed.

A UB report carries, on the record and in the human trace alike: the §7 row
(`x-ub-row`), the clause (`x-ub-clause`), the access span (`x-ub-span`), the
tag-creation span (`x-ub-tag-span`), the rendered borrow-tree slice
(`x-ub-tree`), and the optimization the row licenses (`x-ub-licenses`). That
last one is the D2 pairing, executable. `[mem.ub.closed]` makes it an invariant:
a row that licenses nothing is a rule this language does not have, so a reader
who is stopped by one can always find out what it bought.

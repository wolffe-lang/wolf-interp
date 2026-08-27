//! Unit tests for the tree-walk core.
//!
//! Every test here names the clause it exercises. The corpus-driven acceptance
//! lives in `tests/run_corpus.rs`; these are the litmuses the corpus does not
//! contain — the ones a *dynamic* machine has to get right and a static checker
//! would have rejected before they ran.

use super::*;

fn run(source: &str) -> Run {
    run_source("t.lu", source).expect("the fixture parses")
}

fn outcome(source: &str) -> Outcome {
    run(source).outcome
}

fn stdout(source: &str) -> String {
    String::from_utf8(run(source).stdout).expect("utf-8")
}

fn trap_kind(source: &str) -> TrapKind {
    match outcome(source) {
        Outcome::Trap(trap) => trap.kind,
        other => panic!("expected a trap, got {other:?}"),
    }
}

fn trap_of(source: &str) -> Trap {
    match outcome(source) {
        Outcome::Trap(trap) => *trap,
        other => panic!("expected a trap, got {other:?}"),
    }
}

// -- §2 moves --------------------------------------------------------------

#[test]
fn a_read_of_a_moved_from_place_traps_use_after_move() {
    // `[mem.tier0.move.2]`: "Use of an uninitialized or moved-from place is a
    // compile error (E1001) in the safe tiers. Dynamic meaning … the read traps
    // with kind `use-after-move`."
    let trap = trap_of(
        "struct Big { n: int }\n\
         fn consume(take b: Big) -> int { b.n }\n\
         fn main() -> int {\n\
         \x20   var b = Big { n: 1 }\n\
         \x20   let x = consume(take b)\n\
         \x20   let y = b.n\n\
         \x20   x + y\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::UseAfterMove);
    assert_eq!(trap.rule, Rule::UseAfterMove);
    assert_eq!(trap.rule.anchor(), "mem.tier0.move.2");
    // "the move site and the use site both spanned".
    let (move_site, _) = trap.secondary.expect("the move site is reported");
    assert!(
        move_site.start < trap.span.start,
        "{move_site} vs {}",
        trap.span
    );
}

#[test]
fn a_moved_from_place_may_be_re_initialised() {
    // `[mem.tier0.move.4]`: "A moved-from place may be re-initialized by
    // assignment; it is then live again."
    assert_eq!(
        outcome(
            "struct Big { n: int }\n\
             fn consume(take b: Big) -> int { b.n }\n\
             fn main() -> int {\n\
             \x20   var b = Big { n: 1 }\n\
             \x20   let x = consume(take b)\n\
             \x20   b = Big { n: 2 }\n\
             \x20   x + b.n\n\
             }\n"
        ),
        Outcome::Exit(3)
    );
}

#[test]
fn copy_duplicates_and_leaves_the_source_live() {
    // `[mem.tier0.move.3]`: "`copy x` produces an independent value from any
    // type."
    assert_eq!(
        outcome(
            "struct Big { n: int }\n\
             fn main() -> int {\n\
             \x20   var b = Big { n: 4 }\n\
             \x20   let c = copy b\n\
             \x20   b.n + c.n\n\
             }\n"
        ),
        Outcome::Exit(8)
    );
}

#[test]
fn move_states_are_field_granular() {
    // "Whole-value moves, field-granular states: `a.x` can be `Moved` while
    // `a.y` is `Live`."
    let source = "struct Inner { n: int }\n\
                  struct P { x: Inner, y: Inner }\n\
                  fn eat(take i: Inner) -> int { i.n }\n\
                  fn main() -> int {\n\
                  \x20   var p = P { x: Inner { n: 1 }, y: Inner { n: 2 } }\n\
                  \x20   let a = eat(take p.x)\n\
                  \x20   a + p.y.n\n\
                  }\n";
    assert_eq!(outcome(source), Outcome::Exit(3));

    // …and reading the moved field is still a trap.
    let source = source.replace("a + p.y.n", "a + p.x.n");
    assert_eq!(trap_kind(&source), TrapKind::UseAfterMove);
}

// -- §2 exclusivity --------------------------------------------------------

#[test]
fn disjoint_paths_may_be_mut_simultaneously() {
    // `[mem.tier0.excl.2]`: "`f(mut a.x, mut a.y)` is legal by
    // `[mem.model.path.disjoint]`".
    assert_eq!(
        outcome(
            "struct P { x: int, y: int }\n\
             fn bump(mut a: int, mut b: int) { a += 1\n b += 1 }\n\
             fn main() -> int {\n\
             \x20   var p = P { x: 1, y: 2 }\n\
             \x20   bump(mut p.x, mut p.y)\n\
             \x20   p.x + p.y\n\
             }\n"
        ),
        Outcome::Exit(5)
    );
}

#[test]
fn overlapping_mut_paths_trap_exclusivity() {
    // "`f(mut a.x, mut a.x)` faults — per-path state, not per-variable."
    let trap = trap_of(
        "struct P { x: int, y: int }\n\
         fn bump(mut a: int, mut b: int) { a += 1\n b += 1 }\n\
         fn main() -> int {\n\
         \x20   var p = P { x: 1, y: 2 }\n\
         \x20   bump(mut p.x, mut p.x)\n\
         \x20   p.x\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::Exclusivity);
    assert!(trap.secondary.is_some(), "both spans are printed");
}

#[test]
fn a_whole_value_and_one_of_its_fields_conflict() {
    // "`f(mut a, mut a.x)` is E1002" — dynamically, `exclusivity`.
    assert_eq!(
        trap_kind(
            "struct P { x: int }\n\
             fn both(mut p: P, mut n: int) { n += 1 }\n\
             fn main() -> int {\n\
             \x20   var p = P { x: 1 }\n\
             \x20   both(mut p, mut p.x)\n\
             \x20   p.x\n\
             }\n"
        ),
        TrapKind::Exclusivity
    );
}

#[test]
fn a_read_argument_conflicts_with_a_mut_one() {
    // `[mem.tier0.excl.1]`: a `mut`-held place has "no other live access path"
    // — a `read` of the same path during the same call is one.
    assert_eq!(
        trap_kind(
            "struct P { x: int }\n\
             fn f(mut a: int, b: int) -> int { a + b }\n\
             fn main() -> int {\n\
             \x20   var p = P { x: 1 }\n\
             \x20   f(mut p.x, p.x)\n\
             }\n"
        ),
        TrapKind::Exclusivity
    );
}

#[test]
fn two_read_arguments_over_the_same_path_are_fine() {
    // `[mem.tier0.mode.read]`: several readers coexist; only `mut` excludes.
    assert_eq!(
        outcome(
            "struct P { x: int }\n\
             fn f(a: int, b: int) -> int { a + b }\n\
             fn main() -> int {\n\
             \x20   var p = P { x: 3 }\n\
             \x20   f(p.x, p.x)\n\
             }\n"
        ),
        Outcome::Exit(6)
    );
}

#[test]
fn a_mut_argument_is_written_back_at_the_call_boundary() {
    // `[mem.tier0.mode.mut]`: exclusive *inout*.
    assert_eq!(
        outcome(
            "fn bump(mut n: int) { n += 41 }\n\
             fn main() -> int {\n\
             \x20   var n = 1\n\
             \x20   bump(mut n)\n\
             \x20   n\n\
             }\n"
        ),
        Outcome::Exit(42)
    );
}

#[test]
fn a_read_argument_does_not_leak_the_callees_writes() {
    // `[mem.tier0.mode.read]`: "the callee reads a value that is immutable for
    // the whole call; the caller retains it."
    assert_eq!(
        outcome(
            "fn peek(n: int) -> int { 99 }\n\
             fn main() -> int {\n\
             \x20   var n = 7\n\
             \x20   let _seen = peek(n)\n\
             \x20   n\n\
             }\n"
        ),
        Outcome::Exit(7)
    );
}

#[test]
fn a_mut_borrow_holds_its_path_for_the_bindings_extent() {
    // `[mem.tier0.borrow.2]`: "While an `&mut` borrow of a path is live, that
    // path is exclusively held."
    assert_eq!(
        trap_kind(
            "fn f(mut a: int) { a += 1 }\n\
             fn main() -> int {\n\
             \x20   var n = 1\n\
             \x20   let r = &mut n\n\
             \x20   f(mut n)\n\
             \x20   n\n\
             }\n"
        ),
        TrapKind::Exclusivity
    );
}

#[test]
fn a_borrows_extent_ends_with_its_scope() {
    // The same program with the borrow confined to an inner block runs clean:
    // "a borrow's dynamic extent ends at its binding's death".
    assert_eq!(
        outcome(
            "fn f(mut a: int) { a += 1 }\n\
             fn main() -> int {\n\
             \x20   var n = 1\n\
             \x20   { let r = &mut n }\n\
             \x20   f(mut n)\n\
             \x20   n\n\
             }\n"
        ),
        Outcome::Exit(2)
    );
}

// -- §3 Tier 1: the dynamic region machine ---------------------------------

/// A program body wrapped in `fn main`, for the many small region litmuses.
fn main_of(body: &str) -> String {
    format!("fn main() -> int {{\n{body}\n}}\n")
}

#[test]
fn allocations_land_in_the_innermost_open_region() {
    // `[mem.region.create.3]`: "every function executes with a *current
    // region*; heap allocations land there … `in r { … }` sets the current
    // region to `r` for the block." The callee allocating into the *caller's*
    // region is D12's default, and it falls out of not pushing on a call.
    let run = run("fn fill() -> List[int] {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(7)\n\
         \x20   xs\n\
         }\n\
         fn main() -> int {\n\
         \x20   region tmp {\n\
         \x20       let xs = fill()\n\
         \x20       xs[0] - 7\n\
         \x20   }\n\
         }\n");
    assert_eq!(run.outcome, Outcome::Exit(0));
    assert!(run.leaks.is_empty());
}

#[test]
fn a_sugar_block_frees_wholesale_at_its_closing_brace() {
    // `[mem.region.intra.2]`: "A region dies as a unit … every allocation in it
    // is freed **wholesale**. Per-allocation frees do not exist in safe code."
    let run = run(&main_of(
        "    region a { let xs = List[int]() }\n\
         \x20   region b { let ys = List[int]() }\n\
         \x20   0",
    ));
    assert_eq!(run.outcome, Outcome::Exit(0));
    assert!(run.leaks.is_empty(), "{:?}", run.leaks);
}

#[test]
fn a_first_class_region_is_freed_when_its_owner_dies() {
    // Scope exit, not last use — see `Machine::reclaim`'s note. The observable
    // is the same for a program with no destructors, which is every program
    // this machine can currently express.
    let run = run(&main_of("    let r = region(rc)\n\x20   in r { 0 }"));
    assert_eq!(run.outcome, Outcome::Exit(0));
    assert!(run.leaks.is_empty(), "{:?}", run.leaks);
}

#[test]
fn two_disjoint_regions_open_simultaneously() {
    // `[mem.region.multiopen]` — the Verona relaxation, executed. This is the
    // clause flagged for model checking, and this machine is its first
    // executable test.
    let run = run(&main_of(
        "    let a = region()\n\
         \x20   let b = region()\n\
         \x20   var total = 0\n\
         \x20   in a {\n\
         \x20       var xs = List[int]()\n\
         \x20       (mut xs).push(1)\n\
         \x20       in b {\n\
         \x20           var ys = List[int]()\n\
         \x20           (mut ys).push(2)\n\
         \x20           total += xs[0] + ys[0]\n\
         \x20       }\n\
         \x20   }\n\
         \x20   total - 3",
    ));
    assert_eq!(run.outcome, Outcome::Exit(0));
}

#[test]
fn reopening_the_region_you_are_already_inside_is_a_no_op() {
    // `corpus/memory/region_multiopen_swap.lu` writes exactly this shape and
    // pins it at `run(exit=0)`. `[mem.region.open.1]`'s "Open in at most one
    // scope at a time" does not say it, and the corpus requires it — the
    // finding is filed in `docs/approximation-contract.md`.
    let run = run(&main_of(
        "    region a {\n\
         \x20       var xs = List[int]()\n\
         \x20       in a { (mut xs).push(1) }\n\
         \x20       xs[0] - 1\n\
         \x20   }",
    ));
    assert_eq!(run.outcome, Outcome::Exit(0));
}

#[test]
fn an_ancestor_and_its_descendant_do_not_open_together() {
    // The other half of the multiopen finding: "distinct region values" is not
    // "disjoint regions" once `[mem.region.edge.iso]` lets one own the other.
    let trap = trap_of(
        "struct Holder { child: region }\n\
         fn main() -> int {\n\
         \x20   let c = region()\n\
         \x20   let outer = region()\n\
         \x20   let h = in outer { Holder { child: move c } }\n\
         \x20   in outer { in h.child { 1 } }\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.multiopen");
}

#[test]
fn a_dangling_handle_into_a_freed_region_faults_exactly() {
    // `[mem.region.intra.2]`: "Any later access through a surviving reference
    // faults … detection is exact (region id + generation), never
    // probabilistic."
    let trap = trap_of(
        "struct Node { value: int }\n\
         fn main() -> int {\n\
         \x20   var pool = Pool[Node]()\n\
         \x20   var h = pool.reserve()\n\
         \x20   region r: pool(Node) {\n\
         \x20       var inner = Pool[Node]()\n\
         \x20       h = inner.reserve()\n\
         \x20       inner.init(h, Node { value: 1 })\n\
         \x20       pool = move inner\n\
         \x20   }\n\
         \x20   pool[h].value\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.intra.2");
    assert!(trap.secondary.is_some(), "the dead region's creation site");
}

#[test]
fn an_illegal_cross_region_edge_faults_at_the_store() {
    // §3's edge table, "other region ❌ E1004", as the dynamic half. The check
    // happens at the *store*, which is why the fault's span is the struct
    // literal and not the later read.
    let trap = trap_of(
        "struct Node { value: int, link: handle Node }\n\
         fn main() -> int {\n\
         \x20   region a: pool(Node) {\n\
         \x20       var pa = Pool[Node]()\n\
         \x20       let ha = pa.reserve()\n\
         \x20       pa.init(ha, Node { value: 1, link: ha })\n\
         \x20       region b: pool(Node) {\n\
         \x20           var pb = Pool[Node]()\n\
         \x20           let hb = pb.reserve()\n\
         \x20           pb.init(hb, Node { value: 2, link: ha })\n\
         \x20       }\n\
         \x20   }\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.edge");
    assert!(trap.message.contains("E1004"), "{}", trap.message);
}

#[test]
fn intra_region_cycles_are_safe_by_construction() {
    // `[mem.region.intra.1]`: "cycles, back-edges, intrusive structures are
    // safe. Nothing dangles while the region lives." The doubly-linked ring is
    // the program Rust makes miserable.
    let run = run(
        "struct Node { value: int, next: handle Node, prev: handle Node }\n\
         fn main() -> int {\n\
         \x20   region r: pool(Node) {\n\
         \x20       var p = Pool[Node]()\n\
         \x20       let a = p.reserve()\n\
         \x20       let b = p.reserve()\n\
         \x20       p.init(a, Node { value: 1, next: b, prev: b })\n\
         \x20       p.init(b, Node { value: 2, next: a, prev: a })\n\
         \x20       p[p[a].next].value - 2\n\
         \x20   }\n\
         }\n",
    );
    assert_eq!(run.outcome, Outcome::Exit(0));
    assert!(run.leaks.is_empty());
}

#[test]
fn a_region_transfers_only_as_a_closed_subtree() {
    // `[mem.region.freeze.3]` (E1005's dynamic half): the open form faults and
    // the closed form is the legal twin.
    let trap = trap_of(
        "struct Holder { child: region }\n\
         fn main() -> int {\n\
         \x20   let r = region()\n\
         \x20   let h = in r { Holder { child: move r } }\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.freeze.3");

    assert_eq!(
        outcome(
            "struct Holder { child: region }\n\
             fn main() -> int {\n\
             \x20   let r = region()\n\
             \x20   let n = in r { 21 }\n\
             \x20   let h = Holder { child: move r }\n\
             \x20   n - 21\n\
             }\n",
        ),
        Outcome::Exit(0)
    );
}

#[test]
fn a_region_has_at_most_one_owning_edge() {
    // `[mem.region.edge.iso]`, the forest invariant. A second owner is
    // impossible to *write* — the region value is affine, so the second store
    // reads a moved-from place — which is exactly the static rule made
    // dynamic.
    let trap = trap_of(
        "struct Holder { child: region }\n\
         fn main() -> int {\n\
         \x20   let r = region()\n\
         \x20   let a = Holder { child: move r }\n\
         \x20   let b = Holder { child: move r }\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::UseAfterMove);
    assert_eq!(trap.rule.anchor(), "mem.tier0.move.2");
}

#[test]
fn a_returned_region_transfers_to_the_caller() {
    // wolf-interp#35, `corpus/memory/region_value_return.lu`'s shape: a
    // region is an affine first-class value (X4) and a return is a move, so
    // the callee's scope teardown must NOT free a region its return value
    // carries out — the caller's binding adopts identity + handle (the s20
    // ret-region rig) and opens it. Through is16 this trapped
    // `[mem.region.intra.2]` at the caller's `in`.
    let run = run("fn make() -> region {\n\
         \x20   let r = region()\n\
         \x20   r\n\
         }\n\
         fn main() -> int {\n\
         \x20   let r = make()\n\
         \x20   var t = 0\n\
         \x20   in r {\n\
         \x20       var xs = List[int]()\n\
         \x20       (mut xs).push(41)\n\
         \x20       t = xs[0] + 1\n\
         \x20   }\n\
         \x20   if t == 42 { 0 } else { 1 }\n\
         }\n");
    assert_eq!(run.outcome, Outcome::Exit(0));
    // The transfer is a move, not a leak: the adopting binding freed it.
    assert!(run.leaks.is_empty());
}

#[test]
fn a_region_returned_through_a_return_statement_transfers_too() {
    // The `Signal::Return` road (an early return unwinding through nested
    // scopes) transfers exactly as the block-tail road does.
    let run = run("fn make(flag: bool) -> region {\n\
         \x20   let r = region()\n\
         \x20   if flag {\n\
         \x20       return r\n\
         \x20   }\n\
         \x20   r\n\
         }\n\
         fn main() -> int {\n\
         \x20   let r = make(true)\n\
         \x20   in r {\n\
         \x20       var xs = List[int]()\n\
         \x20       (mut xs).push(1)\n\
         \x20       xs[0] - 1\n\
         \x20   }\n\
         }\n");
    assert_eq!(run.outcome, Outcome::Exit(0));
    assert!(run.leaks.is_empty());
}

#[test]
fn a_region_not_returned_still_frees_at_callee_scope_end() {
    // The other direction, pinned: when the region value does NOT ride the
    // return, teardown frees it exactly as before — were the skip too eager,
    // the region would outlive its binding and show up as a leak.
    let run = run("fn busywork() -> int {\n\
         \x20   let r = region()\n\
         \x20   in r {\n\
         \x20       var xs = List[int]()\n\
         \x20       (mut xs).push(7)\n\
         \x20       xs[0]\n\
         \x20   }\n\
         }\n\
         fn main() -> int {\n\
         \x20   busywork() - 7\n\
         }\n");
    assert_eq!(run.outcome, Outcome::Exit(0));
    assert!(run.leaks.is_empty());
}

#[test]
fn a_value_escaping_its_region_still_faults_after_the_free() {
    // The transfer is for the region VALUE only. A container merely
    // allocated in the dying region does not carry its home out: the callee's
    // teardown frees the region, and the caller's access faults through the
    // freed home (#25) — `region_escape_local.lu`'s class, unchanged by #35.
    let trap = trap_of(
        "fn make() -> List[int] {\n\
         \x20   let r = region()\n\
         \x20   in r {\n\
         \x20       var xs = List[int]()\n\
         \x20       (mut xs).push(1)\n\
         \x20       xs\n\
         \x20   }\n\
         }\n\
         fn main() -> int {\n\
         \x20   let xs = make()\n\
         \x20   xs[0]\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
}

#[test]
fn freezing_is_deep_and_forever_and_writes_through_it_fault() {
    // `[mem.region.freeze.1]`: "promotes the entire graph to `imm` — deep, in
    // place, no copy. Frozen data is immutable **forever**." The read is legal
    // from anywhere (`[mem.region.edge.imm]`); the write faults.
    let trap = trap_of(
        "struct Node { value: int }\n\
         fn main() -> int {\n\
         \x20   let r = region(pool(Node))\n\
         \x20   let p = in r { Pool[Node]() }\n\
         \x20   let frozen = freeze r\n\
         \x20   let h = p.reserve()\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.freeze.1");

    // `freeze region { … }` builds anonymously and promotes: the block's value
    // survives, and the region is never freed.
    let run = run(&main_of(
        "    let cfg = freeze region { 2 }\n\x20   cfg - 2",
    ));
    assert_eq!(run.outcome, Outcome::Exit(0));
    assert!(run.leaks.is_empty(), "a frozen region is not a leak");
}

#[test]
fn a_suspended_region_is_read_only() {
    // `[mem.region.open.3]`: "A Suspended region's contents are unreachable for
    // writing." The read of the same slot is fine, which is what makes the
    // O4 `invariant.load` entitlement sound.
    let source = "struct Node { value: int }\n\
                  fn main() -> int {\n\
                  \x20   let r = region(pool(Node))\n\
                  \x20   let p = in r { Pool[Node]() }\n\
                  \x20   let h = in r { p.reserve() }\n\
                  \x20   in r { p.init(h, Node { value: 42 }) }\n\
                  \x20   let seen = p[h].value\n\
                  \x20   seen - 42\n\
                  }\n";
    assert_eq!(outcome(source), Outcome::Exit(0));

    let trap = trap_of(
        "struct Node { value: int }\n\
         fn main() -> int {\n\
         \x20   let r = region(pool(Node))\n\
         \x20   let p = in r { Pool[Node]() }\n\
         \x20   let h = in r { p.reserve() }\n\
         \x20   p.init(h, Node { value: 42 })\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.open.3");
}

#[test]
fn a_write_through_a_frozen_value_path_traps_region_fault() {
    // `[mem.region.freeze.1]` on tier-0 value paths (wolf-interp#2): the
    // struct is homed in the region the sugar froze, so the write through
    // `cfg.limit` is E1012's shape executed — a trap, never a landed write.
    let trap = trap_of(
        "struct Config { limit: int }\n\
         fn main() -> int {\n\
         \x20   var cfg = freeze region { Config { limit: 42 } }\n\
         \x20   cfg.limit = 7\n\
         \x20   cfg.limit\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule, Rule::RegionFreeze);
    assert_eq!(trap.rule.anchor(), "mem.region.freeze.1");

    // The named-region shape: built under `in r`, frozen after — the home
    // travels with the value, not with the sugar.
    let trap = trap_of(
        "struct Cfg { limit: int }\n\
         fn main() -> int {\n\
         \x20   let r = region(rc)\n\
         \x20   var cfg = in r { Cfg { limit: 7 } }\n\
         \x20   let frozen = freeze r\n\
         \x20   cfg.limit = 9\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.freeze.1");

    // Reads stay legal forever, and rebinding the binding replaces what it
    // holds without touching frozen storage — both halves of the twin.
    let source = "struct Config { limit: int }\n\
                  fn main() -> int {\n\
                  \x20   var cfg = freeze region { Config { limit: 42 } }\n\
                  \x20   let seen = cfg.limit\n\
                  \x20   cfg = Config { limit: 7 }\n\
                  \x20   if seen == 42 && cfg.limit == 7 { 0 } else { 1 }\n\
                  }\n";
    assert_eq!(outcome(source), Outcome::Exit(0));
}

#[test]
fn region_values_are_affine() {
    // `[mem.region.create.2]`: "Region values are **affine**: they move, are
    // never copied". Binding one to a second name moves it, and the first is
    // then a moved-from place — the Tier-0 rule doing Tier-1 work.
    let trap = trap_of(&main_of(
        "    let a = region()\n\
         \x20   let b = a\n\
         \x20   in a { 0 }",
    ));
    assert_eq!(trap.kind, TrapKind::UseAfterMove);
}

// -- §4 Tier 2: `shared` and `handle` --------------------------------------

#[test]
fn a_pool_is_two_phase_and_reading_a_reserved_slot_is_uninitialized() {
    // `[mem.shared.handle.1]`: "`reserve()` yields a handle; `init(h, v)` fills
    // it (grammar/corpus lock — **no null handles exist**)." Reading between
    // the two is a read of uninitialized storage, which `[mem.tier0.move.2]`
    // already names.
    let trap = trap_of(
        "struct Node { value: int }\n\
         fn main() -> int {\n\
         \x20   region r: pool(Node) {\n\
         \x20       var p = Pool[Node]()\n\
         \x20       let h = p.reserve()\n\
         \x20       p[h].value\n\
         \x20   }\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::UseAfterMove);
    assert_eq!(trap.rule.anchor(), "mem.shared.handle.1");
}

#[test]
fn a_stale_handle_faults_deterministically_and_slots_are_reused() {
    // `[mem.shared.handle.2]`: "Accessing a handle whose slot was freed or
    // re-generationed is a **deterministic fault** … in every profile. This is
    // defined behavior, not UB." And: "Freed slots may be reused; generation
    // bumps on reuse."
    let trap = trap_of(
        "struct Node { value: int }\n\
         fn main() -> int {\n\
         \x20   region r: pool(Node) {\n\
         \x20       var p = Pool[Node]()\n\
         \x20       let first = p.reserve()\n\
         \x20       p.init(first, Node { value: 1 })\n\
         \x20       p.remove(first)\n\
         \x20       let second = p.reserve()\n\
         \x20       p.init(second, Node { value: 2 })\n\
         \x20       p[first].value\n\
         \x20   }\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::StaleHandle);
    assert_eq!(trap.rule.anchor(), "mem.shared.handle.2");
    assert!(trap.message.contains("generation"), "{}", trap.message);
}

#[test]
fn refcounts_are_naive_and_the_payload_drops_at_the_last_release() {
    // `[mem.shared.rc.1]` and `[mem.shared.drop.3]`. The interpreter runs
    // honest non-atomic refcounts on purpose: Perceus elision is a compiler
    // optimization that must be *unobservable* against this baseline.
    let run = run("struct Cfg { limit: int }\n\
         fn main() -> int {\n\
         \x20   let a = shared (Cfg { limit: 7 })\n\
         \x20   let b = a.clone()\n\
         \x20   b.limit - 7\n\
         }\n");
    assert_eq!(run.outcome, Outcome::Exit(0));
    assert!(
        run.live_cells.is_empty(),
        "the cell outlived its owners: {:?}",
        run.live_cells
    );
}

#[test]
fn a_weak_edge_keeps_nothing_alive_and_upgrading_is_option_shaped() {
    // `[mem.shared.rc.3]`: "`weak T` does not keep the value alive; upgrading
    // yields an option-shaped result the caller must handle."
    let run = run("struct Cfg { limit: int }\n\
         fn weak_of() -> weak Cfg {\n\
         \x20   let a = shared (Cfg { limit: 7 })\n\
         \x20   a.downgrade()\n\
         }\n\
         fn main() -> int {\n\
         \x20   var w = weak_of()\n\
         \x20   let live = w.upgrade() else |_| { return 0 }\n\
         \x20   1\n\
         }\n");
    assert_eq!(run.outcome, Outcome::Exit(0));

    // While a strong owner is alive, the upgrade succeeds.
    assert_eq!(
        outcome(
            "struct Cfg { limit: int }\n\
             fn main() -> int {\n\
             \x20   let a = shared (Cfg { limit: 7 })\n\
             \x20   let w = a.downgrade()\n\
             \x20   let live = w.upgrade() else |_| { return 1 }\n\
             \x20   live.limit - 7\n\
             }\n",
        ),
        Outcome::Exit(0)
    );
}

#[test]
fn the_acyclicity_assertion_reports_a_strong_cycle_without_inventing_a_trap() {
    // `[mem.shared.rc.2]` is a *compile* error (E1006) and `[conf.trap.set]` is
    // closed with no kind for it, so the dynamic half is an assertion in the
    // trace — never a trap this implementation legislated into existence. The
    // missing counterpart is a spec finding.
    let program = crate::sema::load_source(
        "t.lu",
        "struct S { value: int }\n\
         fn main() -> int {\n\
         \x20   let inner = shared (S { value: 1 })\n\
         \x20   let outer = shared (inner)\n\
         \x20   0\n\
         }\n",
    )
    .expect("parses");
    let run = Machine::new(&program).tracing(super::Trace::Memory).run();
    assert_eq!(run.outcome, Outcome::Exit(0));
    assert!(
        run.trace
            .iter()
            .any(|line| line.contains("mem.shared.rc.2") && line.contains("acyclic")),
        "the acyclicity assertion never ran: {:?}",
        run.trace
    );
    assert!(!matches!(run.outcome, Outcome::Trap(_)));
}

// -- checked arithmetic (X3) ----------------------------------------------

#[test]
fn overflow_traps_in_every_profile() {
    let trap = trap_of("fn main() -> int {\n    var x: i32 = 2147483647\n    x += 1\n    0\n}\n");
    assert_eq!(trap.kind, TrapKind::Overflow);
    assert_eq!(trap.rule.anchor(), "arith.checked");
}

#[test]
fn wrapping_types_are_the_spelling_for_intended_overflow() {
    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   var h: wrapping[u32] = 4294967295\n\
             \x20   h = h + 2\n\
             \x20   if h == 1 { 0 } else { 1 }\n\
             }\n"
        ),
        Outcome::Exit(0)
    );
}

#[test]
fn wrapping_shift_counts_mask_at_sixty_four_on_u64() {
    // #42: the shift COUNT masks to the TYPE's bit width on wrapping[T] —
    // `x << 64 == x` on wrapping[u64], the WIR shl contract all three
    // compiler lanes implement. Before this landed, lupin answered 0.
    // The `x << 32` line is the per-type half of the pin: on u64 a count
    // of 32 is a REAL shift (no one-constant-serves-all mask at 32).
    assert_eq!(
        stdout(
            "fn main() -> int {\n\
             \x20   let x: wrapping[u64] = 5\n\
             \x20   let a = (x << 64) as int\n\
             \x20   let b = (x >> 64) as int\n\
             \x20   let c = (x << 65) as int\n\
             \x20   let d = (x << 32) as int\n\
             \x20   print(\"{a} {b} {c} {d}\")\n\
             \x20   0\n\
             }\n"
        ),
        "5 5 10 21474836480\n"
    );
}

#[test]
fn wrapping_shift_counts_mask_at_thirty_two_on_u32() {
    // #42's other pinned width: wrapping[u32] masks at 32 — `y << 32 == y`,
    // and a count of 64 masks to 0 (64 % 32), NOT to a 64-bit behavior.
    // `y >> 33` masks to `y >> 1`.
    assert_eq!(
        stdout(
            "fn main() -> int {\n\
             \x20   let y: wrapping[u32] = 7\n\
             \x20   let a = (y << 32) as int\n\
             \x20   let b = (y << 64) as int\n\
             \x20   let c = (y >> 33) as int\n\
             \x20   print(\"{a} {b} {c}\")\n\
             \x20   0\n\
             }\n"
        ),
        "7 7 3\n"
    );
}

#[test]
fn checked_shift_at_the_width_still_traps() {
    // #42 is a WRAPPING ruling only: on a checked type a shift whose result
    // leaves the range keeps X3's overflow trap — no count mask sneaks into
    // the checked family.
    let trap = trap_of(
        "fn main() -> int {\n\
         \x20   let x: u64 = 1\n\
         \x20   let y = x << 64\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::Overflow);
    assert_eq!(trap.rule.anchor(), "arith.checked");
}

#[test]
fn division_by_zero_traps_rather_than_being_ub() {
    // `[mem.ub.defined]`: "Division by zero | trap `div-zero`".
    let trap = trap_of("fn main() -> int {\n    var d = 0\n    let x = 10 / d\n    x\n}\n");
    assert_eq!(trap.kind, TrapKind::DivZero);
    assert_eq!(trap.rule.anchor(), "mem.ub.defined");
    assert_eq!(
        trap_kind("fn main() -> int {\n    var d = 0\n    let x = 10 % d\n    x\n}\n"),
        TrapKind::DivZero
    );
}

#[test]
fn out_of_bounds_indexing_traps() {
    // `[mem.ub.defined]`: "OOB index / split-code-point slice (D25) | trap
    // `bounds`".
    let trap = trap_of(
        "fn main() -> int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(1)\n\
         \x20   xs[3]\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::Bounds);
    assert_eq!(trap.rule.anchor(), "mem.ub.defined");
}

#[test]
fn a_slice_that_splits_a_code_point_traps() {
    assert_eq!(
        trap_kind("fn main() -> int {\n    let s = \"é\"\n    let head = s[..1]\n    0\n}\n"),
        TrapKind::Bounds
    );
}

#[test]
fn the_s37_str_surface_answers_the_corpus_shapes() {
    // `corpus/strings/builtin_methods.lu`'s gauntlet, as a unit litmus:
    // `[mem.str.get]`'s domain law (oob and split-code-point are the same
    // `none` miss, a hit is bit-identical to the checked slice), `^n`
    // end-relative slicing, byte-offset `find`, and the probe/view set.
    assert_eq!(
        stdout(
            "fn main() -> int {\n\
             \x20   let s = \"the wolf runs\"\n\
             \x20   let head = s[..8]\n\
             \x20   let tail = s[^4..]\n\
             \x20   let mid = s.get(4..8) else \"?\"\n\
             \x20   let miss = s.get(0..99) else \"?\"\n\
             \x20   let split = \"é\".get(0..1) else \"?\"\n\
             \x20   let off = s.find(\"wolf\") else 0 - 1\n\
             \x20   let gone = s.find(\"fox\") else 0 - 1\n\
             \x20   let words = s.words()\n\
             \x20   print(\"{head}|{tail}|{mid}|{miss}|{split}|{off}|{gone}|{words.len}\")\n\
             \x20   0\n\
             }\n"
        ),
        "the wolf|runs|wolf|?|?|4|-1|3\n"
    );
    // The rest of the 18-method set, each through its row where it has one.
    assert_eq!(
        stdout(
            "fn main() -> int {\n\
             \x20   let s = \"  three wolves  \"\n\
             \x20   let a = s.trim_start().trim_end()\n\
             \x20   let b = a.strip_prefix(\"three \") else \"?\"\n\
             \x20   let c = a.strip_suffix(\"three\") else \"missed\"\n\
             \x20   let parts = a.split(\" \")\n\
             \x20   let n = a.count(\"e\")\n\
             \x20   let r = a.replace(\"wolves\", \"wolf\")\n\
             \x20   let e = a.ends_with(\"wolves\")\n\
             \x20   let bytes = a.bytes()\n\
             \x20   print(\"{b}|{c}|{parts.len}|{n}|{r}|{e}|{bytes.len}|{bytes[0]}\")\n\
             \x20   0\n\
             }\n"
        ),
        "wolves|missed|2|3|three wolf|true|12|116\n"
    );
    // A negative repeat is a caller contract violation: the deterministic
    // `assert` trap on every lane, never a modular wrap ([mem.str.repeat],
    // s71 — the sc03-era `bounds` spelling retired with the clause).
    assert_eq!(
        trap_kind(
            "fn main() -> int {\n    let n = 0 - 2\n    let s = \"ab\".repeat(n)\n    0\n}\n"
        ),
        TrapKind::Assert
    );
}

#[test]
fn debug_and_release_agree_because_there_is_only_one_semantics() {
    // X3/D2: one semantics everywhere. The machine has no profile switch at
    // all, which is the strongest form of the guarantee — this test exists to
    // fail loudly if one is ever added.
    for _ in 0..2 {
        assert_eq!(
            trap_kind("fn main() -> int {\n    var x: i32 = 2147483647\n    x + 1\n}\n"),
            TrapKind::Overflow
        );
    }
}

// -- errors as values (D30) -----------------------------------------------

#[test]
fn question_mark_propagates_an_error_without_unwinding() {
    assert_eq!(
        outcome(
            "fn inner() -> !int { Boom }\n\
             fn outer() -> !int {\n\
             \x20   let v = inner()?\n\
             \x20   v + 1\n\
             }\n\
             fn main() -> !int {\n\
             \x20   outer() else 7\n\
             }\n"
        ),
        Outcome::Exit(7)
    );
}

#[test]
fn else_defaults_in_all_three_forms() {
    assert_eq!(
        outcome(
            "fn bad() -> !int { Boom }\n\
             fn main() -> !int {\n\
             \x20   let a = bad() else 1\n\
             \x20   let b = bad() else { 2 }\n\
             \x20   let c = bad() else |err| { 3 }\n\
             \x20   a + b + c\n\
             }\n"
        ),
        Outcome::Exit(6)
    );
}

#[test]
fn an_error_payload_is_readable_through_the_handler_pattern() {
    assert_eq!(
        stdout(
            "struct ParseError { at: int }\n\
             fn bad() -> !int { BadDigit(ParseError { at: 3 }) }\n\
             fn main() -> !int {\n\
             \x20   let v = bad() else |err| {\n\
             \x20       match err {\n\
             \x20           BadDigit(e) => print(\"at {e.at}\"),\n\
             \x20           _ => print(\"other\"),\n\
             \x20       }\n\
             \x20       0\n\
             \x20   }\n\
             \x20   v\n\
             }\n"
        ),
        "at 3\n"
    );
}

#[test]
fn defers_run_lifo_at_scope_exit() {
    // `[mem.shared.drop.1]`: "Scope-exit effects run LIFO".
    assert_eq!(
        stdout(
            "fn main() -> int {\n\
             \x20   defer print_raw(\" second\")\n\
             \x20   defer print_raw(\" first\")\n\
             \x20   print_raw(\"body\")\n\
             \x20   0\n\
             }\n"
        ),
        "body first second"
    );
}

#[test]
fn errdefer_runs_on_the_error_path_only() {
    // `[err.errdefer]`, and the "only then" half is the one worth a test.
    assert_eq!(
        stdout(
            "fn ok() -> !int {\n\
             \x20   errdefer print_raw(\"cleanup\")\n\
             \x20   1\n\
             }\n\
             fn main() -> !int { ok() }\n"
        ),
        ""
    );
    assert_eq!(
        stdout(
            "fn bad() -> !int {\n\
             \x20   errdefer print_raw(\"cleanup\")\n\
             \x20   return Boom\n\
             }\n\
             fn main() -> !int { bad() else 0 }\n"
        ),
        "cleanup"
    );
}

#[test]
fn defer_runs_on_both_paths() {
    assert_eq!(
        stdout(
            "fn bad() -> !int {\n\
             \x20   defer print_raw(\"always\")\n\
             \x20   return Boom\n\
             }\n\
             fn main() -> !int { bad() else 0 }\n"
        ),
        "always"
    );
}

// -- control flow is expression-oriented ----------------------------------

#[test]
fn every_control_form_is_an_expression() {
    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   let a = if true { 1 } else { 2 }\n\
             \x20   let b = match a { 1 => 10, _ => 20 }\n\
             \x20   var c = 0\n\
             \x20   for i in 1..4 { c += i }\n\
             \x20   var d = 0\n\
             \x20   while d < 3 { d += 1 }\n\
             \x20   let e = loop { break 100 }\n\
             \x20   let f = { 5 }\n\
             \x20   a + b + c + d + e + f\n\
             }\n"
        ),
        // 1 + 10 + 6 + 3 + 100 + 5
        Outcome::Exit(125)
    );
}

#[test]
fn an_inclusive_range_includes_its_end() {
    assert_eq!(
        outcome("fn main() -> int {\n    var s = 0\n    for i in 1..=4 { s += i }\n    s\n}\n"),
        Outcome::Exit(10)
    );
}

#[test]
fn break_and_continue_target_the_innermost_loop() {
    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   var s = 0\n\
             \x20   for i in 0..10 {\n\
             \x20       if i == 3 { continue }\n\
             \x20       if i == 5 { break }\n\
             \x20       s += i\n\
             \x20   }\n\
             \x20   s\n\
             }\n"
        ),
        // 0 + 1 + 2 + 4
        Outcome::Exit(7)
    );
}

#[test]
fn boolean_operators_short_circuit() {
    // `[gram.expr.prec]`. The right operand would trap if it were evaluated,
    // which is how the test proves it was not.
    assert_eq!(
        outcome(
            "fn main() -> int {\n    var d = 0\n    if false && (10 / d) == 0 { 1 } else { 0 }\n}\n"
        ),
        Outcome::Exit(0)
    );
    assert_eq!(
        outcome(
            "fn main() -> int {\n    var d = 0\n    if true || (10 / d) == 0 { 0 } else { 1 }\n}\n"
        ),
        Outcome::Exit(0)
    );
}

#[test]
fn a_closure_used_after_a_write_to_its_captured_place_refuses_the_stale_read() {
    // wolf-interp#36, `corpus/memory/closure_borrow_write.lu`'s shape. This
    // test used to pin the OPPOSITE — "a later write to the captured local
    // is invisible inside it", exit on the stale copy — which is exactly the
    // divergence the issue filed: the compiler's closure env BORROWS its
    // captures (s98, `[abi.native.closure]`) and refuses the write with
    // E1002 whenever the closure is still needed. A dynamic machine learns
    // "still needed" at the use, so the use is where it faults, naming the
    // write.
    let trap = trap_of(
        "fn main() -> int {\n\
         \x20   var n = 1\n\
         \x20   let f = fn(x) x + n\n\
         \x20   n = 100\n\
         \x20   f(1)\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::Exclusivity);
    assert_eq!(trap.rule, Rule::BorrowExtent);
    assert_eq!(trap.rule.anchor(), "mem.tier0.borrow.2");
    assert!(trap.message.contains("E1002"), "{}", trap.message);
    let (write_site, _) = trap.secondary.expect("the write site is spanned");
    assert!(write_site.start < trap.span.start);
}

#[test]
fn a_write_after_the_closures_last_use_stays_legal() {
    // The NLL complement, dynamically exact: once nothing needs the closure
    // any more, the write is W1102's advisory shape, not E1002 — and this
    // machine runs it. Zero false positives is the point of checking at the
    // use instead of at the write.
    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   var n = 1\n\
             \x20   let f = fn(x) x + n\n\
             \x20   let a = f(1)\n\
             \x20   n = 100\n\
             \x20   a - 2\n\
             }\n"
        ),
        Outcome::Exit(0)
    );
}

#[test]
fn a_closure_writing_its_own_captured_copy_keeps_running() {
    // `corpus/memory/closure_capture_write.lu`'s shape: the write INSIDE the
    // body lands on the closure's own frame-local copy — no loan on the
    // creating frame moves, so the program runs (the compiler refuses this
    // by name at `wir`; here it is the honest copy semantics executing).
    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   var n = 0\n\
             \x20   let f = fn() { n = n + 1 }\n\
             \x20   f()\n\
             \x20   0\n\
             }\n"
        ),
        Outcome::Exit(0)
    );
}

#[test]
fn a_task_closure_is_a_capture_not_a_loan() {
    // D14's law: task captures are copies (`move`/copy/`imm`), and E1101 is
    // the code that polices writes to them — not E1002. A closure crossing
    // to a spawned task is exempt from the loan check even when the parent
    // keeps writing; the corpus's conc tier leans on this.
    assert_eq!(
        outcome(
            "fn main() -> !int {\n\
             \x20   let ch = channel[int](1)\n\
             \x20   var base = 10\n\
             \x20   scope s {\n\
             \x20       s.spawn(fn() { ch.send(base) })\n\
             \x20   }\n\
             \x20   base = 20\n\
             \x20   let got = ch.recv() else |_| { return 2 }\n\
             \x20   got - 10\n\
             }\n"
        ),
        Outcome::Exit(0)
    );
}

// -- strings ---------------------------------------------------------------

#[test]
fn f_strings_interpolate_and_print_appends_a_newline() {
    assert_eq!(
        stdout(
            "fn main() -> int {\n    let who = \"wolf\"\n    print(\"hello, {who}\")\n    0\n}\n"
        ),
        "hello, wolf\n"
    );
}

#[test]
fn format_specs_pad_to_a_width() {
    assert_eq!(
        stdout("fn main() -> int {\n    let n = 6\n    print(\"{n:>3}|\")\n    0\n}\n"),
        "  6|\n"
    );
}

// -- the sema-lite boundary ------------------------------------------------

#[test]
fn an_unresolved_name_is_unsupported_not_a_crash() {
    // "a sema-lite failure (unresolvable name, ambiguous dispatch) is verdict
    // `unsupported` … never a crash."
    let Outcome::Unsupported(reason) = outcome("fn main() -> int { nope() }\n") else {
        panic!("expected unsupported")
    };
    assert!(reason.contains("nope"), "{reason}");
}

#[test]
fn a_type_error_the_checker_owns_is_unsupported_not_a_trap() {
    // Arity is E0402's; a wrong-typed operand is E0401's. Neither is a fault of
    // a *defined* execution, so neither may borrow the trap vocabulary
    // (`[conf.trap.map]`).
    for source in [
        "fn area(w: int, h: int) -> int { w * h }\nfn main() -> int { area(3) }\n",
        "fn get() -> int { 41 }\nfn add(a: int, b: int) -> int { a + b }\nfn main() -> int { add(get, 1) }\n",
        "struct C { radius: int }\nfn main() -> int {\n    let c = C { radius: 2 }\n    let r = c.radbus\n    0\n}\n",
    ] {
        assert!(
            matches!(outcome(source), Outcome::Unsupported(_)),
            "{source}"
        );
    }
}

#[test]
fn an_ambiguous_definition_declines_to_dispatch() {
    let Outcome::Unsupported(reason) = outcome(
        "fn helper() -> int { 1 }\nfn helper() -> int { 2 }\nfn main() -> int { helper() }\n",
    ) else {
        panic!("expected unsupported")
    };
    assert!(reason.contains("more than once"), "{reason}");
}

#[test]
fn a_construct_outside_this_tier_names_the_tier_that_owns_it() {
    for (source, owner) in [
        // Tier 1 and Tier 2 moved *into* coverage at is03; what is left
        // outside is the unsafe tier and concurrency.
        (
            "fn main() -> int {\n    var a = 1\n    let p = &a\n    let q = *p\n    0\n}\n",
            "Tier 3",
        ),
        // `unsafe { }` itself moved *into* coverage at is04; what is left
        // outside it is the opaque C body, whose meaning is c10's.
        // (Concurrency left this list at is06: `scope s { … }` runs now.)
        ("fn main() -> int {\n    unsafe c [] { }\n    0\n}\n", "c10"),
    ] {
        let Outcome::Unsupported(reason) = outcome(source) else {
            panic!("expected unsupported for {source}")
        };
        assert!(reason.contains(owner), "{reason}");
    }
}

#[test]
fn a_non_terminating_program_declines_rather_than_hangs() {
    // Not a trap: `[conf.trap.set]` has no kind for "did not finish", and
    // inventing one would extend a closed vocabulary.
    let program =
        crate::sema::load_source("t.lu", "fn main() -> int {\n    loop { }\n}\n").expect("parses");
    let machine = Machine::new(&program);
    machine
        .shared
        .steps
        .store(Machine::FUEL - 10, std::sync::atomic::Ordering::Relaxed);
    assert!(matches!(machine.run().outcome, Outcome::Unsupported(_)));
}

// -- the trace -------------------------------------------------------------

#[test]
fn the_trace_records_each_rule_as_it_fires() {
    let program = crate::sema::load_source(
        "t.lu",
        "fn main() -> int {\n    var x: i32 = 2147483647\n    x + 1\n}\n",
    )
    .expect("parses");
    let run = Machine::new(&program).tracing(super::Trace::All).run();
    assert!(matches!(run.outcome, Outcome::Trap(_)));
    let trace = run.trace.join("\n");
    // Rules render with their anchor, so the trace is self-citing too.
    assert!(
        trace.contains("[mem.tier0.move.1]") || trace.contains("[mem.model.place]"),
        "{trace}"
    );
    assert!(trace.contains("[conf.trap.set]"), "{trace}");
}

#[test]
fn tracing_is_off_by_default() {
    assert!(run("fn main() -> int { 0 }\n").trace.is_empty());
}

// -- 0.1.2: bare patterns resolve variants and row tags (issue #5) ----------

#[test]
fn a_bare_identifier_naming_a_variant_dispatches_rather_than_binding() {
    // wolf-std F-0007: `match Ordering.Greater { Less => 1, … }` yielded 1 —
    // the first arm always matched. An in-scope variant name is a variant
    // pattern (`[gram.pat]`; the checker resolves it against the scrutinee's
    // type, this machine against the module's variant table).
    assert_eq!(
        outcome(
            "enum Ordering { Less, Equal, Greater }\n\
             fn main() -> int {\n\
             \x20   let o = Ordering.Greater\n\
             \x20   match o { Less => 1, Equal => 2, Greater => 3 }\n\
             }\n"
        ),
        Outcome::Exit(3)
    );
}

#[test]
fn a_bare_variant_value_matches_its_qualified_and_bare_spellings() {
    // The value spelled bare (`Greater` resolves as a structural tag) still
    // matches the variant pattern through the table.
    assert_eq!(
        outcome(
            "enum Ordering { Less, Equal, Greater }\n\
             fn main() -> int {\n\
             \x20   let o = Greater\n\
             \x20   match o { Less => 1, Equal => 2, Greater => 3 }\n\
             }\n"
        ),
        Outcome::Exit(3)
    );
}

#[test]
fn a_payload_pattern_matches_the_enum_qualified_value() {
    // `corpus/typecheck/match_exhaustive.lu`'s shape: `Rgb(r, g, b)` against
    // a value built as `Color.Rgb(1, 2, 3)` — the same variant-table
    // resolution, payload half.
    assert_eq!(
        outcome(
            "enum Color { Red, Rgb(int, int, int) }\n\
             fn main() -> int {\n\
             \x20   match Color.Rgb(1, 2, 3) { Red => 0, Rgb(r, g, b) => r + g + b }\n\
             }\n"
        ),
        Outcome::Exit(6)
    );
}

#[test]
fn a_variant_value_dispatches_its_enums_impl_methods() {
    // wolf-interp#34's second shape (wolf-lang#23's surviving leg,
    // `corpus/typecheck/variant_value/` at unit size): a payload-free
    // variant as a bare VALUE owns its enum's nominal identity in method
    // dispatch — `favorite()`'s `Hue.Red` answers `.name()` through
    // `impl Hue`, whose own `match self` resolves the bare variant
    // patterns against the same variant table.
    assert_eq!(
        stdout(
            "enum Hue { Red, Blue }\n\
             impl Hue {\n\
             \x20   fn name(self) -> str {\n\
             \x20       match self { Red => \"red\", Blue => \"blue\" }\n\
             \x20   }\n\
             }\n\
             fn favorite() -> Hue { Hue.Red }\n\
             fn main() -> int {\n\
             \x20   print(favorite().name())\n\
             \x20   0\n\
             }\n"
        ),
        "red\n"
    );
}

#[test]
fn row_tags_dispatch_and_lowercase_still_binds() {
    // D30 rows need no declaration: over a tag-shaped scrutinee a
    // capitalized bare identifier is a tag pattern; a lowercase one binds.
    assert_eq!(
        outcome(
            "fn risky(n: int) -> int ! {TooShort, BadDigit(int)} {\n\
             \x20   if n == 0 { return TooShort }\n\
             \x20   if n == 1 { return BadDigit(9) }\n\
             \x20   n\n\
             }\n\
             fn main() -> int {\n\
             \x20   let a = risky(0) else |err| {\n\
             \x20       match err { TooShort => 40, BadDigit(code) => code, other => 0 }\n\
             \x20   }\n\
             \x20   let b = risky(1) else |err| {\n\
             \x20       match err { TooShort => 0, BadDigit(code) => code, other => 1 }\n\
             \x20   }\n\
             \x20   a + b - 7\n\
             }\n"
        ),
        Outcome::Exit(42)
    );
}

// -- the index-read lend (issue #28) ---------------------------------------

#[test]
fn an_index_read_lends_the_container_and_leaves_it_where_it_was() {
    // Issue #28: `xs[i]` used to deep-copy `xs` to pick one element out of
    // it. The value now steps out of its slot for the length of one
    // `builtin::index` call and steps back, so every later read of the
    // container — and every nesting of one index inside another — sees it
    // exactly as before. `xs[xs[0]]` is the load-bearing shape: the inner
    // read runs while the outer lend has not been taken yet.
    assert_eq!(
        stdout(
            "fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   (mut xs).push(1)\n\
             \x20   (mut xs).push(2)\n\
             \x20   (mut xs).push(3)\n\
             \x20   print(\"{xs[0]} {xs[xs[0]]} {xs[2]} {xs.len}\")\n\
             \x20   var m = Map[str, int]()\n\
             \x20   m[\"a\"] = 7\n\
             \x20   print(\"{m[\"a\"]} {m[\"absent\"]} {m.len}\")\n\
             \x20   0\n\
             }\n"
        ),
        // An absent key is the zero value (`Unit` here), which renders `()`.
        "1 2 3 3\n7 () 1\n"
    );
}

#[test]
fn an_out_of_bounds_index_still_traps_through_the_lend() {
    // The lend ends before the trap is reported, so a faulting index read
    // leaves the container in its slot rather than in the machine's hand.
    assert_eq!(
        trap_kind(
            "fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   (mut xs).push(1)\n\
             \x20   print(\"{xs[3]}\")\n\
             \x20   0\n\
             }\n"
        ),
        TrapKind::Bounds
    );
}

#[test]
fn an_index_expression_that_mutates_the_container_reads_the_mutated_one() {
    // The lend is taken *after* the index expression is evaluated — the
    // same ordering the receiver lend uses (issue #24) — so a write hidden
    // in the index is visible to the read it precedes. Pinned because it is
    // the one shape the lend moved: the copy path snapshotted the container
    // before evaluating the index, which made `xs[bump(mut xs) - 1]` trap
    // `bounds` against the stale length in interpolation position while the
    // identical `let v = xs[bump(mut xs) - 1]` answered 9. wolfgang answers
    // 9 (`--checked`, trunk 13b811f); the two spellings now agree with it
    // and with each other.
    assert_eq!(
        stdout(
            "fn bump(mut ys: List[int]) -> int {\n\
             \x20   (mut ys).push(9)\n\
             \x20   ys.len\n\
             }\n\
             fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   (mut xs).push(1)\n\
             \x20   (mut xs).push(2)\n\
             \x20   let v = xs[bump(mut xs) - 1]\n\
             \x20   print(\"{v} {xs[bump(mut xs) - 1]}\")\n\
             \x20   0\n\
             }\n"
        ),
        "9 9\n"
    );
}

#[test]
fn an_index_read_under_a_for_loops_read_claim_is_allowed() {
    // D40's read claim over the iterated container is shared, so reading an
    // element of it inside the body is a read against a read — the lend
    // charges the same claim the copy did, at the same moment.
    assert_eq!(
        stdout(
            "fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   (mut xs).push(4)\n\
             \x20   (mut xs).push(5)\n\
             \x20   for v in xs {\n\
             \x20       print(\"{v} {xs[0]}\")\n\
             \x20   }\n\
             \x20   0\n\
             }\n"
        ),
        "4 4\n5 4\n"
    );
}

#[test]
fn an_uppercase_name_over_a_non_error_scrutinee_binds_like_the_counterparty() {
    // Observed at pin a0c4564: `match 3 { Zed => Zed, _ => 9 }` — the
    // compiler treats `Zed` as a binding (E0802 unreachable on `_`) and the
    // program yields the scrutinee.
    assert_eq!(
        outcome("fn main() -> int {\n    match 3 { Zed => Zed, _ => 9 }\n}\n"),
        Outcome::Exit(3)
    );
}

#[test]
fn a_match_no_arm_of_which_applies_is_unsupported_not_a_wrong_answer() {
    // Exhaustiveness is the type checker's (E0801 has no dynamic half);
    // a dynamic miss is the honest `unsupported`, never first-arm-wins.
    assert!(matches!(
        outcome(
            "enum Signal { Go, Slow, Stop }\n\
             fn main() -> int {\n\
             \x20   match Signal.Stop { Go => 0, Slow => 1 }\n\
             }\n"
        ),
        Outcome::Unsupported(_)
    ));
}

#[test]
fn same_scope_let_shadowing_reads_the_latest_binding() {
    // `corpus/typecheck/let_shadow_var_ok.lu`'s core, unit-sized: the
    // rposition repair — a second `let b` shadows the first in the same
    // scope, and reads and writes mean the latest one.
    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   let b = 6\n\
             \x20   let b = b + 4\n\
             \x20   var b = b\n\
             \x20   b += 32\n\
             \x20   b - 42\n\
             }\n"
        ),
        Outcome::Exit(0)
    );
}

// -- integer literals meet their context (issue #14) ------------------------

#[test]
fn int_min_is_writable_in_every_annotated_spelling() {
    // wolf-std F-0025 shape 1: every spelling of -2^63 with an `int`/`i64`
    // context is the value, not an i32 overflow. The literal stays
    // unconstrained through negation and literal-only arithmetic
    // (`[arith.literal.default]` applies at the binding, not the operator).
    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   const A: int = -9223372036854775808\n\
             \x20   let b: int = -9223372036854775807 - 1\n\
             \x20   let c: int = 0 - 9223372036854775807 - 1\n\
             \x20   let d: i64 = -9223372036854775808\n\
             \x20   if A == b { if b == c { if c == d { return 0 } } }\n\
             \x20   1\n\
             }\n"
        ),
        Outcome::Exit(0)
    );
}

#[test]
fn an_annotated_binding_widens_where_the_default_would_not() {
    // Shape 2 is a RULE, not a bug (wolfc agrees at pin ad6cef7: `var k = 0`
    // lowers to an i32 constant): the binding is the defaulting context, so
    // a literal that does not fit i32 traps THERE, and an annotation is the
    // spelling that widens it.
    assert_eq!(
        outcome("fn main() -> int {\n\x20   let x: int = 4503599627370496\n\x20   x - x\n}\n"),
        Outcome::Exit(0),
    );
}

#[test]
fn a_literal_outside_i32_traps_at_the_unannotated_binding() {
    let trap = trap_of("fn main() -> int {\n\x20   var k = 4503599627370496\n\x20   k - k\n}\n");
    assert_eq!(trap.kind, TrapKind::Overflow);
}

#[test]
fn the_declared_return_type_types_the_returned_literal() {
    // Shape 3: `int_max() - 1` is `int` arithmetic because the signature
    // says `-> int` — the same-file and cross-module cases go through the
    // same coercion, so the module boundary cannot lose the type again.
    assert_eq!(
        outcome(
            "fn int_max() -> int {\n\
             \x20   9223372036854775807\n\
             }\n\
             fn main() -> int {\n\
             \x20   let m = int_max() - 1\n\
             \x20   if m == 9223372036854775806 { 0 } else { 1 }\n\
             }\n"
        ),
        Outcome::Exit(0)
    );
}

// -- the X1 mode law's dynamic residue (issue #15) --------------------------

#[test]
fn a_fn_value_call_missing_the_declared_mut_is_refused_not_run_wrong() {
    // The static E1007 half lives in sema (resolve rejects direct calls);
    // a call through a function *value* is the residue the static tier
    // cannot see. Running it would copy the argument and lose the
    // writeback — a silently wrong answer — and `[conf.trap.map]` gives
    // E1007 no trap kind, so the machine refuses.
    let outcome = outcome(
        "fn bump(mut n: int) { n += 1 }\n\
         fn main() -> int {\n\
         \x20   let f = bump\n\
         \x20   var x = 1\n\
         \x20   f(x)\n\
         \x20   x - 1\n\
         }\n",
    );
    match outcome {
        Outcome::Unsupported(reason) => {
            assert!(reason.contains("E1007"), "{reason}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_mut_receiver_method_on_a_bare_call_site_traps_with_the_mode_named() {
    // wolf-interp#37, `corpus/typecheck/receiver_bare_mut.lu`'s shape: X1
    // binds receiver modes at the call site, and the compiler refuses the
    // bare spelling with E0804. This machine ran it to the mutated answer
    // through 0.1.13 (exit 42 on the corpus witness) — the silently wrong
    // class. Now the call-site marker is demanded at call evaluation:
    // `trap(exclusivity)`, the mode family's kind, with both spans (the
    // call and the `mut self` declaration).
    let trap = trap_of(
        "struct Counter { n: int }\n\
         impl Counter {\n\
         \x20   fn bump(mut self) -> int {\n\
         \x20       self.n = self.n + 1\n\
         \x20       self.n\n\
         \x20   }\n\
         }\n\
         fn main() -> int {\n\
         \x20   var c = Counter { n: 41 }\n\
         \x20   c.bump()\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::Exclusivity);
    assert_eq!(trap.rule, Rule::ModeMut);
    assert_eq!(trap.rule.anchor(), "mem.tier0.mode.mut");
    assert!(trap.message.contains("E0804"), "{}", trap.message);
    assert!(trap.message.contains("(mut c)"), "{}", trap.message);
    assert!(trap.secondary.is_some(), "the declaration site is spanned");
}

#[test]
fn the_marked_spellings_of_a_mut_receiver_still_run() {
    // The legal spellings are untouched: the user impl's `(mut c).bump()`
    // and the builtin pair `(mut xs).push`/`(mut xs).pop`.
    assert_eq!(
        outcome(
            "struct Counter { n: int }\n\
             impl Counter {\n\
             \x20   fn bump(mut self) -> int {\n\
             \x20       self.n = self.n + 1\n\
             \x20       self.n\n\
             \x20   }\n\
             }\n\
             fn main() -> int {\n\
             \x20   var c = Counter { n: 41 }\n\
             \x20   (mut c).bump() - 42\n\
             }\n",
        ),
        Outcome::Exit(0)
    );
    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   var xs = List[int]()\n\
             \x20   (mut xs).push(7)\n\
             \x20   let last = (mut xs).pop() else 0\n\
             \x20   last - 7\n\
             }\n",
        ),
        Outcome::Exit(0)
    );
}

#[test]
fn a_bare_builtin_mutating_call_traps_and_a_reading_one_does_not() {
    // `List.push` is the builtin surface's mut-receiver arm; a bare `len`
    // or `xs[i]` read keeps running exactly as before — the demand is only
    // where the receiver mode is `mut`.
    let trap = trap_of(
        "fn main() -> int {\n\
         \x20   var xs = List[int]()\n\
         \x20   xs.push(7)\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::Exclusivity);
    assert!(trap.message.contains("E0804"), "{}", trap.message);

    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   var xs = List[int]()\n\
             \x20   (mut xs).push(3)\n\
             \x20   xs.len - xs[0] + 2\n\
             }\n",
        ),
        Outcome::Exit(0)
    );
}

#[test]
fn a_prim_impl_dispatches_through_the_trait_qualified_call() {
    // wolf-interp#34's third shape (upstream #119, D49's substrate):
    // `impl Text for int` registers under the spelling `int` exactly as a
    // nominal does, and the trait-qualified call reaches it for an
    // int-typed receiver. The literal leg stays undispatched on BOTH
    // machines (`Text.text(7)` types the literal i32 upstream —
    // prim_impl.lu's own header leaves it with D49's implementing
    // campaign), so the second program declines rather than guessing.
    let run = run("trait Text {\n\
         \x20   fn text(x: Self) -> str\n\
         }\n\
         impl Text for int {\n\
         \x20   fn text(x: Self) -> str { \"n\" }\n\
         }\n\
         fn main() -> !int {\n\
         \x20   let n: int = 7\n\
         \x20   print(Text.text(n))\n\
         \x20   0\n\
         }\n");
    assert_eq!(run.outcome, Outcome::Exit(0));
    assert_eq!(String::from_utf8(run.stdout).expect("utf-8"), "n\n");

    let literal = outcome(
        "trait Text {\n\
         \x20   fn text(x: Self) -> str\n\
         }\n\
         impl Text for int {\n\
         \x20   fn text(x: Self) -> str { \"n\" }\n\
         }\n\
         fn main() -> !int {\n\
         \x20   print(Text.text(7))\n\
         \x20   0\n\
         }\n",
    );
    assert!(
        matches!(literal, Outcome::Unsupported(_)),
        "the literal leg is D49's campaign, not this machine's guess: {literal:?}"
    );
}

// -- the lent receiver (issue #24) ------------------------------------------

#[test]
fn a_lent_receiver_is_the_same_machine_only_cheaper() {
    // `List.push` used to cost the whole list: `read_path` copied it out,
    // `eval_method` copied it again to compare against, the comparison walked
    // it, and the write-back copied it back — four traversals per element
    // appended, so a loop of pushes was quadratic. Lending the receiver
    // (`Lend`) removes all four without moving a single observable: what this
    // asserts is the *semantics*, and the corpus report being byte-identical
    // across the change is what asserts the rest.
    assert_eq!(
        stdout(
            "fn main() -> int {\n\
             \x20   var xs = List[int]()\n\
             \x20   var i = 0\n\
             \x20   while i < 200 {\n\
             \x20       (mut xs).push(i * 2)\n\
             \x20       i = i + 1\n\
             \x20   }\n\
             \x20   print(\"{xs.len} {xs[0]} {xs[199]}\")\n\
             \x20   0\n\
             }\n",
        ),
        "200 0 398\n"
    );
}

#[test]
fn a_lend_hands_the_receiver_back_when_the_method_traps() {
    // `pop` on an empty `List` faults `bounds`. The receiver was lent, so the
    // slot held `Value::Unit` while the builtin ran — and a `defer` runs on
    // the trap path (`[err.errdefer]`'s sibling), so it can SEE the slot. The
    // list has to be back in it: were the placeholder left behind, `xs.len`
    // below would refuse ("`()` has no member `len`") instead of printing 0.
    let run = run("fn main() -> int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(1)\n\
         \x20   defer print(\"after={xs.len}\")\n\
         \x20   let a = (mut xs).pop()\n\
         \x20   let b = (mut xs).pop()\n\
         \x20   a + b\n\
         }\n");
    match &run.outcome {
        Outcome::Trap(trap) => assert_eq!(trap.kind, TrapKind::Bounds),
        other => panic!("expected the bounds trap, got {other:?}"),
    }
    assert_eq!(String::from_utf8(run.stdout).expect("utf-8"), "after=0\n");
}

#[test]
fn a_two_phase_receiver_still_reads_itself_through_the_parent() {
    // The lend is taken AFTER the arguments are evaluated, precisely so
    // `(mut xs).push(xs.len)` still reads `xs` through its parent while the
    // receiver's fresh tag is Reserved (`corpus/memory/prov_two_phase.lu`).
    // Taking it earlier would hand the argument a placeholder.
    assert_eq!(
        stdout(
            "fn main() -> int {\n\
             \x20   var xs = List[int]()\n\
             \x20   (mut xs).push(7)\n\
             \x20   (mut xs).push(xs.len)\n\
             \x20   print(\"{xs[0]} {xs[1]} {xs.len}\")\n\
             \x20   0\n\
             }\n",
        ),
        "7 1 2\n"
    );
}

#[test]
fn a_read_only_method_on_a_lent_receiver_is_not_a_write() {
    // `[mem.region.freeze.4]` (issue #20): a method that only read its
    // receiver performed a read, legal through frozen data to any depth. The
    // lend must not turn putting the value back into a write, or every
    // `frozen.words()` would fault.
    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   let held = freeze region r {\n\
             \x20       var xs = List[int]()\n\
             \x20       (mut xs).push(3)\n\
             \x20       xs\n\
             \x20   }\n\
             \x20   held.len - 1\n\
             }\n",
        ),
        Outcome::Exit(0)
    );
}

// -- #28: the CoW list spine ------------------------------------------------
//
// `Value::List` shares its element vector behind an `Arc`; every write path
// diverges its copy first (`Arc::make_mut`). These litmuses pin the property
// that makes the sharing invisible: no program can distinguish the CoW spine
// from the plain `Vec` it replaced — only the clock could, and the clock is
// not a value.

#[test]
fn a_read_mode_list_argument_shares_and_the_caller_keeps_its_list() {
    // `[mem.tier0.mode.read]`: the callee sees the caller's list, the caller
    // observes no change — under CoW the argument is a refcount bump, and
    // this pins that the caller's value survives the call intact.
    let out = stdout(
        "fn peek(xs: List[int]) -> int { xs.len }\n\
         fn main() -> !int {\n\
             var xs = List[int]()\n\
             (mut xs).push(7)\n\
             (mut xs).push(9)\n\
             let a = peek(xs)\n\
             let b = peek(xs)\n\
             print(\"{a} {b} {xs.len}\")\n\
             0\n\
         }\n",
    );
    assert_eq!(out, "2 2 2\n");
}

#[test]
fn a_donor_push_after_a_container_insert_diverges_the_spines() {
    // `(mut outer).push(inner)` retains the donor and shares the spine;
    // the donor's next push must diverge it — `outer[0]` is a snapshot,
    // exactly as the plain `Vec` copy left it.
    let out = stdout(
        "fn main() -> !int {\n\
             var inner = List[int]()\n\
             (mut inner).push(5)\n\
             var outer = List()\n\
             (mut outer).push(inner)\n\
             (mut inner).push(6)\n\
             print(\"{outer[0].len} {inner.len}\")\n\
             0\n\
         }\n",
    );
    assert_eq!(out, "1 2\n");
}

#[test]
fn a_donor_element_write_leaves_the_snapshot_element() {
    // The place-projection write (`inner[0] = …`) goes through the CoW
    // divergence too: the inserted snapshot's element is untouched.
    let out = stdout(
        "fn main() -> !int {\n\
             var inner = List[int]()\n\
             (mut inner).push(5)\n\
             var outer = List()\n\
             (mut outer).push(inner)\n\
             inner[0] = 9\n\
             print(\"{outer[0][0]} {inner[0]}\")\n\
             0\n\
         }\n",
    );
    assert_eq!(out, "5 9\n");
}

#[test]
fn a_moved_list_still_traps_use_after_move() {
    // The spine is shared machinery, not shared SEMANTICS: `let ys = xs`
    // is still a move (`[mem.tier0.move.1]`), and the CoW change must not
    // quietly turn moves into copies.
    let trap = trap_kind(
        "fn main() -> !int {\n\
             var xs = List[int]()\n\
             (mut xs).push(1)\n\
             let ys = xs\n\
             (mut xs).push(2)\n\
             print(\"{ys.len}\")\n\
             0\n\
         }\n",
    );
    assert_eq!(trap, TrapKind::UseAfterMove);
}

#[test]
fn the_same_list_twice_as_read_arguments_is_two_shares() {
    // `f(xs, xs)` — Shared + Shared is legal, and both parameters see the
    // same length; neither copy is ever materialized.
    let out = stdout(
        "fn both(a: List[int], b: List[int]) -> int { a.len + b.len }\n\
         fn main() -> !int {\n\
             var xs = List[int]()\n\
             (mut xs).push(3)\n\
             print(\"{both(xs, xs)}\")\n\
             0\n\
         }\n",
    );
    assert_eq!(out, "2\n");
}

#[test]
fn a_donor_pop_leaves_the_snapshot_whole() {
    // `pop` is the other write path; the container-insert snapshot keeps
    // both elements while the donor shrinks.
    let out = stdout(
        "fn main() -> !int {\n\
             var inner = List[int]()\n\
             (mut inner).push(4)\n\
             (mut inner).push(8)\n\
             var outer = List()\n\
             (mut outer).push(inner)\n\
             let last = (mut inner).pop()\n\
             print(\"{last} {inner.len} {outer[0].len}\")\n\
             0\n\
         }\n",
    );
    assert_eq!(out, "8 1 2\n");
}

// -- #25: the container knows its home ---------------------------------------
//
// `Value::List` carries the region charged at its allocation site, exactly as
// `Value::Struct` has since is08, and every access consults it: a `Freed` home
// faults `[mem.region.intra.2]` at the access, a `Frozen` one faults writes
// `[mem.region.freeze.1]`. The compiler's half is E1010, a static refusal at
// `mem`; these litmuses pin the dynamic complement — and the negatives pin
// that the catch never fires early.

#[test]
fn a_list_escaping_its_freed_region_faults_on_read() {
    // The #25 reproducer, read arm (`corpus/memory/region_escape_container.lu`'s
    // shape): the escaped list's home is freed wholesale at the sugar-block
    // exit, so `keep.len` — an access through the container — faults with the
    // region named, instead of running to a clean exit.
    let trap = trap_of(
        "fn main() -> !int {\n\
         \x20   var keep = List[int]()\n\
         \x20   region tmp {\n\
         \x20       keep = List[int]()\n\
         \x20       (mut keep).push(7)\n\
         \x20   }\n\
         \x20   if keep.len == 1 { 0 } else { 1 }\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule, Rule::RegionFree);
    assert_eq!(trap.rule.anchor(), "mem.region.intra.2");
    // The region is named and its creation site is the secondary span.
    let (created, _) = trap.secondary.expect("the creation site is reported");
    assert!(created.start < trap.span.start);
}

#[test]
fn a_list_escaping_its_freed_region_faults_on_write() {
    // The write arm: `push` into the freed region's storage. The fault fires
    // at the receiver access, before anything diverges — a trapping write
    // mutates nothing.
    let trap = trap_of(
        "fn main() -> !int {\n\
         \x20   var keep = List[int]()\n\
         \x20   region tmp {\n\
         \x20       keep = List[int]()\n\
         \x20       (mut keep).push(7)\n\
         \x20   }\n\
         \x20   (mut keep).push(9)\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.intra.2");

    // The element-write spelling reaches the same fault through the path
    // walk (`write_refusal`), not the method surface.
    let trap = trap_of(
        "fn main() -> !int {\n\
         \x20   var keep = List[int]()\n\
         \x20   region tmp {\n\
         \x20       keep = List[int]()\n\
         \x20       (mut keep).push(7)\n\
         \x20   }\n\
         \x20   keep[0] = 5\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.intra.2");
}

#[test]
fn a_struct_escaping_its_freed_region_faults_the_same_way() {
    // `corpus/memory/region_escape_local.lu`'s shape — the struct sibling.
    // The home machinery existed since is08; is16 makes ACCESS consult it,
    // so the read of `keep.value` faults instead of answering 7.
    let trap = trap_of(
        "struct Node { value: int }\n\
         fn main() -> !int {\n\
         \x20   var keep = Node { value: 0 }\n\
         \x20   region tmp {\n\
         \x20       keep = Node { value: 7 }\n\
         \x20   }\n\
         \x20   if keep.value == 7 { 0 } else { 1 }\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.intra.2");
}

#[test]
fn a_write_through_a_frozen_list_traps_region_fault() {
    // Freeze parity — the struct freeze litmus duplicated for a list
    // (`[mem.region.freeze.1]` on list value paths). The named-region shape:
    // built under `in r`, frozen after — the home travels with the value.
    let trap = trap_of(
        "fn main() -> !int {\n\
         \x20   let r = region(rc)\n\
         \x20   var xs = in r {\n\
         \x20       var l = List[int]()\n\
         \x20       (mut l).push(1)\n\
         \x20       l\n\
         \x20   }\n\
         \x20   let frozen = freeze r\n\
         \x20   (mut xs).push(2)\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule, Rule::RegionFreeze);
    assert_eq!(trap.rule.anchor(), "mem.region.freeze.1");

    // The element write is the other divergence point; same fault.
    let trap = trap_of(
        "fn main() -> !int {\n\
         \x20   let r = region(rc)\n\
         \x20   var xs = in r {\n\
         \x20       var l = List[int]()\n\
         \x20       (mut l).push(1)\n\
         \x20       l\n\
         \x20   }\n\
         \x20   let frozen = freeze r\n\
         \x20   xs[0] = 9\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.freeze.1");

    // Reads stay legal forever (`[mem.region.edge.imm]`), and rebinding the
    // binding replaces what it holds without touching frozen storage — the
    // same twin the struct litmus pins.
    let source = "fn main() -> !int {\n\
                  \x20   let r = region(rc)\n\
                  \x20   var xs = in r {\n\
                  \x20       var l = List[int]()\n\
                  \x20       (mut l).push(4)\n\
                  \x20       l\n\
                  \x20   }\n\
                  \x20   let frozen = freeze r\n\
                  \x20   let seen = xs[0]\n\
                  \x20   xs = List[int]()\n\
                  \x20   (mut xs).push(6)\n\
                  \x20   if seen == 4 && xs.len == 1 && xs[0] == 6 { 0 } else { 1 }\n\
                  }\n";
    assert_eq!(outcome(source), Outcome::Exit(0));
}

#[test]
fn a_list_outliving_a_scope_with_a_live_region_reads_clean() {
    // The catch must NOT fire early: a container outliving the block that
    // built it is legal while its region lives — only `Freed` faults. The
    // list here is built in `r` inside an `in r` block, read long after the
    // block closed, and `r` is still alive.
    let source = "fn build(n: int) -> List[int] {\n\
                  \x20   var xs = List[int]()\n\
                  \x20   var i = 0\n\
                  \x20   while i < n {\n\
                  \x20       (mut xs).push(i)\n\
                  \x20       i = i + 1\n\
                  \x20   }\n\
                  \x20   xs\n\
                  }\n\
                  fn main() -> !int {\n\
                  \x20   let r = region(rc)\n\
                  \x20   var xs = in r { build(4) }\n\
                  \x20   (mut xs).push(9)\n\
                  \x20   if xs.len == 5 && xs[2] == 2 && xs[4] == 9 { 0 } else { 1 }\n\
                  }\n";
    assert_eq!(outcome(source), Outcome::Exit(0));
}

// -- #38: nested named fns (the compiler's #116b twin) ----------------------

#[test]
fn a_capture_free_nested_fn_binds_like_a_let_and_runs_every_provenance() {
    // `typecheck/nested_fn_value.lu`'s three shapes in one fixture: direct
    // call, pass to a higher-order fn, bind-and-call. The nested fn is the
    // closure recipe with a declared signature and NO captures.
    let source = "fn apply(f: fn(int) -> bool, v: int) -> bool { f(v) }\n\
                  fn main() -> !int {\n\
                  \x20   fn odd(v: int) -> bool {\n\
                  \x20       let m = v % 2\n\
                  \x20       m == 1\n\
                  \x20   }\n\
                  \x20   if odd(3) { print(\"odd\") } else { return 1 }\n\
                  \x20   if apply(odd, 5) {} else { return 2 }\n\
                  \x20   let g = odd\n\
                  \x20   if g(7) { print(\"yes\") } else { return 3 }\n\
                  \x20   0\n\
                  }\n";
    assert_eq!(outcome(source), Outcome::Exit(0));
    assert_eq!(stdout(source), "odd\nyes\n");
}

#[test]
fn a_nested_fn_may_call_module_items_and_prelude_names() {
    // Free names that resolve at module or prelude level are not captures:
    // the refused set is enclosing LOCALS only.
    let source = "fn double(v: int) -> int { v * 2 }\n\
                  fn main() -> !int {\n\
                  \x20   fn shout(v: int) -> int { print(\"{v}\"); double(v) }\n\
                  \x20   if shout(4) == 8 { 0 } else { 1 }\n\
                  }\n";
    assert_eq!(outcome(source), Outcome::Exit(0));
    assert_eq!(stdout(source), "4\n");
}

#[test]
fn a_nested_fn_capturing_an_enclosing_local_refuses_by_name() {
    // `typecheck/nested_fn_capture.lu`: a capture means an environment, and
    // the env machinery belongs to closure VALUES a binding claims. The
    // refusal names the local and points at the closure spelling — parity
    // with the compiler's scoped v1, never a silent miscompile of `base`.
    let Outcome::Unsupported(reason) = outcome(
        "fn main() -> !int {\n\
         \x20   let base = 4\n\
         \x20   fn plus(v: int) -> int { v + base }\n\
         \x20   if plus(1) == 5 { 0 } else { 1 }\n\
         }\n",
    ) else {
        panic!("a capturing nested fn must refuse");
    };
    assert!(reason.contains("`base`"), "{reason}");
    assert!(reason.contains("bind a closure instead"), "{reason}");
}

#[test]
fn the_nested_fn_scoped_out_shapes_refuse_by_name() {
    // Generics, an error row on the nested return, and parameter modes are
    // the compiler's refused set too — refusing THERE and here is parity.
    for (source, needle) in [
        (
            "fn main() -> !int {\n\
             \x20   fn id[T](v: T) -> T { v }\n\
             \x20   if id(1) == 1 { 0 } else { 1 }\n\
             }\n",
            "generics",
        ),
        (
            "fn main() -> !int {\n\
             \x20   fn pick(v: int) -> int ! {none} { v }\n\
             \x20   if pick(1) == 1 { 0 } else { 1 }\n\
             }\n",
            "error row",
        ),
        (
            "fn main() -> !int {\n\
             \x20   fn bump(mut v: int) { v = v + 1 }\n\
             \x20   bump(1)\n\
             \x20   0\n\
             }\n",
            "parameter mode",
        ),
    ] {
        let Outcome::Unsupported(reason) = outcome(source) else {
            panic!("expected a by-name refusal for: {source}");
        };
        assert!(reason.contains(needle), "{reason} vs {needle}");
    }
}

#[test]
fn a_nested_fn_shadowing_is_scoped_to_its_block() {
    // The binding is a `let`: it lives in the block that declared it and is
    // gone after — a later same-name call resolves the module fn, exactly as
    // a shadowed `let` would.
    let source = "fn tag() -> int { 1 }\n\
                  fn main() -> !int {\n\
                  \x20   var first = 0\n\
                  \x20   {\n\
                  \x20       fn tag() -> int { 2 }\n\
                  \x20       first = tag()\n\
                  \x20   }\n\
                  \x20   if first == 2 && tag() == 1 { 0 } else { 1 }\n\
                  }\n";
    assert_eq!(outcome(source), Outcome::Exit(0));
}

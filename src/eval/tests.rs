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
         \x20   xs.push(1)\n\
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
fn a_closure_captures_by_value() {
    // `[gram.expr.closure]`: captured when the closure is built, so a later
    // write to the captured local is invisible inside it.
    assert_eq!(
        outcome(
            "fn main() -> int {\n\
             \x20   var n = 1\n\
             \x20   let f = fn(x) x + n\n\
             \x20   n = 100\n\
             \x20   f(1)\n\
             }\n"
        ),
        Outcome::Exit(2)
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
        (
            "fn main() -> int {\n    region r { let a = 1 }\n    0\n}\n",
            "Tier 1",
        ),
        (
            "fn main() -> int {\n    let s = shared 1\n    0\n}\n",
            "Tier 2",
        ),
        (
            "fn main() -> int {\n    unsafe { let a = 1 }\n    0\n}\n",
            "is04",
        ),
        (
            "fn main() -> int {\n    scope s { let a = 1 }\n    0\n}\n",
            "ic03",
        ),
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
    let mut machine = Machine::new(&program);
    machine.steps = Machine::FUEL - 10;
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
    let run = Machine::new(&program).tracing(true).run();
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

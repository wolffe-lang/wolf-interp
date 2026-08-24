//! The 0.1.8 rulings, executing.
//!
//! - **D39** (`[mem.tier0.mode.read]`, wolf-lang#27's dynamic half): a write
//!   through a read-mode binding traps `exclusivity` — the callee-side write
//!   barrier. The caller-side overlap half (`f(mut a, a.x)`) was already
//!   trapped and stays (approximation-contract §6.12).
//! - **D40** (resolves S-11, wolf-interp#9): `for x in xs` holds a read claim
//!   on the container for the loop's extent; a mut use inside the body traps
//!   `exclusivity` at the mutation (`[conf.trap.map]`'s E1013 row;
//!   approximation-contract §6.8).
//! - **`[mem.str.empty]`** (s71, #56): the searching family is defined on an
//!   empty needle — count 0, split one whole piece, replace identity.
//! - **`[mem.str.repeat]`** (s71, #57): a negative repeat count traps
//!   `assert`, not `bounds`.
//! - **E0809** (s71, #43): an `else` handler pattern must cover the
//!   operand's whole error row, judged at this machine's resolve rung.

use wolf_interp::frontend;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn observe(source: &str) -> frontend::Observation {
    frontend::observe(source.as_bytes(), None)
}

fn exits_zero(source: &str) {
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
}

fn traps_exclusivity(source: &str) {
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Trap(TrapKind::Exclusivity),
        "reason: {:?}",
        observation.reason
    );
}

// -- D39: the callee-side write barrier ----------------------------------

#[test]
fn a_write_to_a_read_parameter_traps_exclusivity() {
    traps_exclusivity(
        "\
fn poke(n: int) -> int {
    n = 9
    n
}

fn main() -> !int {
    let k = 3
    print(\"{poke(k)}\")
    0
}
",
    );
}

#[test]
fn a_projection_write_through_a_read_parameter_traps() {
    traps_exclusivity(
        "\
struct P { x: int, y: int }

fn poke(p: P) -> int {
    p.x = 9
    p.x
}

fn main() -> !int {
    var a = P { x: 1, y: 2 }
    print(\"{poke(a)}\")
    0
}
",
    );
}

#[test]
fn a_compound_assign_through_a_read_parameter_traps() {
    traps_exclusivity(
        "\
fn bump(n: int) -> int {
    n += 1
    n
}

fn main() -> !int {
    print(\"{bump(1)}\")
    0
}
",
    );
}

#[test]
fn a_mutating_method_on_a_read_parameter_traps() {
    // The receiver write-back is a write through the read binding.
    traps_exclusivity(
        "\
fn grow(xs: List[int]) -> int {
    (mut xs).push(9)
    xs.len
}

fn main() -> !int {
    var l = List[int]()
    (mut l).push(1)
    print(\"{grow(l)}\")
    0
}
",
    );
}

#[test]
fn a_mut_parameter_still_writes_and_writes_back() {
    // The barrier watches READ bindings only; `mut` is exclusive inout and
    // the write-back is observable (call-by-value-result).
    exits_zero(
        "\
fn bump(mut n: int) {
    n += 1
}

fn main() -> !int {
    var k = 3
    bump(mut k)
    if k != 4 { return 1 }
    0
}
",
    );
}

#[test]
fn a_body_local_shadowing_the_parameter_writes_freely() {
    // Scope 0 of a call frame holds exactly the parameters; a body-scope
    // shadow of the name is an ordinary local.
    exits_zero(
        "\
fn poke(n: int) -> int {
    var n = n
    n = 9
    n
}

fn main() -> !int {
    if poke(1) != 9 { return 1 }
    0
}
",
    );
}

#[test]
fn the_caller_side_overlap_half_still_traps() {
    // D39's other half, held since before this pass: `f(mut a, a.x)`.
    traps_exclusivity(
        "\
struct P { x: int, y: int }

fn f(mut p: P, v: int) -> int {
    p.x + v
}

fn main() -> !int {
    var a = P { x: 1, y: 2 }
    print(\"{f(mut a, a.x)}\")
    0
}
",
    );
}

// -- D40: the loop's read claim -------------------------------------------

#[test]
fn pushing_into_the_iterated_container_traps() {
    // wolf-interp#9's exact program (wolf-std F-0014): it used to run
    // exit(0) over the loop-entry snapshot. D40 rules it a trap.
    traps_exclusivity(
        "\
fn main() -> !int {
    var xs = List[int]()
    (mut xs).push(1)
    (mut xs).push(2)
    for x in xs {
        (mut xs).push(x)
    }
    0
}
",
    );
}

#[test]
fn writing_an_element_of_the_iterated_container_traps() {
    traps_exclusivity(
        "\
fn main() -> !int {
    var xs = List[int]()
    (mut xs).push(1)
    (mut xs).push(2)
    for x in xs {
        xs[0] = x
    }
    0
}
",
    );
}

#[test]
fn assigning_the_whole_iterated_container_traps() {
    traps_exclusivity(
        "\
fn main() -> !int {
    var xs = List[int]()
    (mut xs).push(1)
    for x in xs {
        xs = List[int]()
    }
    0
}
",
    );
}

#[test]
fn reading_the_container_inside_the_loop_stays_legal() {
    // Read beside read: the claim is Shared.
    exits_zero(
        "\
fn main() -> !int {
    var xs = List[int]()
    (mut xs).push(1)
    (mut xs).push(2)
    var sum = 0
    for x in xs {
        sum += x + xs.len + xs[0]
    }
    if sum != 9 { return 1 }
    0
}
",
    );
}

#[test]
fn the_claim_ends_with_the_loop_on_every_exit() {
    // Mutation after the loop — including after a `break` — is ordinary.
    exits_zero(
        "\
fn main() -> !int {
    var xs = List[int]()
    (mut xs).push(1)
    (mut xs).push(2)
    for x in xs {
        if x == 1 { break }
    }
    (mut xs).push(3)
    if xs.len != 3 { return 1 }
    0
}
",
    );
}

#[test]
fn iterating_a_range_claims_nothing() {
    // The ruled fix-it: the index loop mutates freely.
    exits_zero(
        "\
fn main() -> !int {
    var xs = List[int]()
    (mut xs).push(1)
    (mut xs).push(2)
    for i in 0..2 {
        (mut xs).push(xs[i])
    }
    if xs.len != 4 { return 1 }
    0
}
",
    );
}

// -- [mem.str.empty] / [mem.str.repeat] ------------------------------------

#[test]
fn the_empty_needle_is_defined_count_split_replace() {
    // An empty needle matches nothing: count 0, split one whole piece,
    // replace identity — the only conforming answers, on every lane.
    exits_zero(
        "\
fn main() -> !int {
    if \"abc\".count(\"\") != 0 { return 1 }
    let p = \"abc\".split(\"\")
    if p.len != 1 { return 2 }
    if p[0] != \"abc\" { return 3 }
    if \"abc\".replace(\"\", \"-\") != \"abc\" { return 4 }
    0
}
",
    );
}

#[test]
fn a_negative_repeat_is_the_assert_trap_not_bounds() {
    let observation = observe(
        "\
fn main() -> !int {
    let s = \"ab\"
    let n = 0 - 1
    let r = s.repeat(n)
    print(\"{r}\")
    0
}
",
    );
    assert_eq!(observation.verdict, Verdict::Trap(TrapKind::Assert));
}

#[test]
fn a_zero_repeat_answers_the_empty_string() {
    exits_zero(
        "\
fn main() -> !int {
    if \"ab\".repeat(0) != \"\" { return 1 }
    0
}
",
    );
}

// -- E0809: else-handler row coverage (resolve rung) ------------------------

fn resolve(source: &str) -> frontend::Observation {
    frontend::observe(source.as_bytes(), Some(Phase::Resolve))
}

#[test]
fn an_uncovering_handler_pattern_is_e0809_at_resolve() {
    let observation = resolve(
        "\
fn poke(n: int) -> int ! {Io(int), timeout} {
    if n == 0 { return Io(9) }
    if n == 1 { return timeout }
    n
}

fn main() -> !int {
    let v = poke(2) else |Io(e)| { e }
    print(\"{v}\")
    0
}
",
    );
    assert_eq!(observation.verdict, Verdict::Fail("E0809".to_owned()));
    assert_eq!(observation.phase_reached, Phase::Resolve);
}

#[test]
fn a_single_tag_pattern_over_a_two_tag_row_is_e0809_too() {
    // The lowercase declared-tag reading: `timeout` is a row-tag pattern,
    // so it covers one tag and leaves `Io` unhandled.
    let observation = resolve(
        "\
fn poke(n: int) -> int ! {Io(int), timeout} {
    if n == 0 { return Io(9) }
    if n == 1 { return timeout }
    n
}

fn main() -> !int {
    let v = poke(2) else |timeout| { 0 - 1 }
    print(\"{v}\")
    0
}
",
    );
    assert_eq!(observation.verdict, Verdict::Fail("E0809".to_owned()));
}

#[test]
fn a_binder_covers_the_row_entire() {
    let observation = resolve(
        "\
fn poke(n: int) -> int ! {Io(int), timeout} {
    if n == 0 { return Io(9) }
    if n == 1 { return timeout }
    n
}

fn main() -> !int {
    let v = poke(2) else |err| { 0 - 1 }
    print(\"{v}\")
    0
}
",
    );
    assert_eq!(observation.verdict, Verdict::Pass);
}

#[test]
fn a_total_single_tag_pattern_passes_and_binds_the_payload() {
    // rows/else_tag_payload.lu's shape: one tag, `Parse(p)` is total.
    exits_zero(
        "\
struct E { offset: int }

fn get(s: str) -> int ! {Parse(E)} {
    if s.starts_with(\"2\") { return 2 }
    return Parse(E { offset: 7 })
}

fn main() -> !int {
    var seen = 0
    let miss = get(\"x\") else |Parse(p)| {
        seen = p.offset
        0 - 1
    }
    if miss != 0 - 1 { return 1 }
    if seen != 7 { return 2 }
    0
}
",
    );
}

//! The numeric cast matrix (issue #11, wolf-std F-0022).
//!
//! `n as f64` used to *retag* rather than convert: the value stayed an
//! integer, compared equal to ints and unequal to the float it claimed to
//! be — a silently wrong answer, the worst class. The fix makes `as`
//! between numeric types a **conversion** in every direction, so this file
//! pins the whole matrix (`[ty.cast.closed-set]`; the float model is
//! approximation-contract §6.9):
//!
//! - int → float converts exactly (`3 as f64` *is* `3.0`);
//! - float → int truncates toward zero, range-checked — NaN, the
//!   infinities and out-of-range values trap (X3: checked in every
//!   profile), never saturate silently;
//! - int → int narrowing range-checks and traps; `wrapping[T]` /
//!   `saturating[T]` targets reduce by their own mode instead;
//! - `as f32` rounds through f32 precision (every float value is an f64);
//! - the non-bridges stay refused: no truthiness (`bool as int`), no
//!   stringly casts (`3 as str`) — the compiler's E0805 row;
//! - adapter casts (`distinct`) stay free and bidirectional.

use wolf_interp::frontend;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn observe(body: &str) -> frontend::Observation {
    let source = format!("fn main() -> !int {{\n{body}\n}}\n");
    frontend::observe(source.as_bytes(), None)
}

fn exits_zero(body: &str) {
    let observation = observe(body);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "body: {body}\nreason: {:?}",
        observation.reason
    );
}

fn traps_overflow(body: &str) {
    let observation = observe(body);
    assert_eq!(
        observation.verdict,
        Verdict::Trap(TrapKind::Overflow),
        "body: {body}\nreason: {:?}",
        observation.reason
    );
    assert_eq!(observation.phase_reached, Phase::Run);
}

// -- int → float --------------------------------------------------------

/// The F-0022 discriminating pair, healed: the cast converts, the float it
/// produces equals the float literal, and the mixed comparison with the
/// *int* is now the one that reads false.
#[test]
fn int_to_f64_converts() {
    exits_zero("if (3 as f64) == 3.0 { 0 } else { 1 }");
    exits_zero("if (3 as f64) == 3 { 1 } else { 0 }");
}

/// The filed program, byte for byte: `to_f(3)` converts, computes as a
/// float, and compares equal to `3.0`. Its *rendering* moved at the
/// f0da6e6 re-pin: the s38 float surface pins the shortest round-trip
/// decimal (`corpus/strings/float_format.lu`), so `3.0` prints `3` —
/// the value is still every bit a float, which the comparison proves.
#[test]
fn the_filed_program_runs() {
    let source = "\
fn to_f(n: int) -> f64 {
    n as f64
}

fn main() -> !int {
    let v = to_f(3)
    print(\"{v}\")
    if v == 3.0 { 0 } else { 1 }
}
";
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(observation.verdict, Verdict::Exit(0));
    assert_eq!(String::from_utf8_lossy(&observation.stdout).trim(), "3");
}

/// The mixed arithmetic the filed issue found `unsupported` — a converted
/// float multiplies with a float literal like any other.
#[test]
fn converted_floats_compute() {
    exits_zero("if (3 as f64) * 1.0 == 3.0 { 0 } else { 1 }");
    exits_zero("let n = 10\nif (n as f64) / 4.0 == 2.5 { 0 } else { 1 }");
}

/// `as f32` rounds through f32 precision: 2^24 + 1 is the first integer
/// f32 cannot hold, and it lands on 2^24.
#[test]
fn int_to_f32_rounds() {
    exits_zero("if (16777217 as f32) == 16777216.0 { 0 } else { 1 }");
}

// -- float → int --------------------------------------------------------

#[test]
fn float_to_int_truncates_toward_zero() {
    exits_zero("if (3.9 as int) == 3 { 0 } else { 1 }");
    exits_zero("let x = 0.0 - 3.9\nif (x as int) == 0 - 3 { 0 } else { 1 }");
}

#[test]
fn float_to_int_out_of_range_traps() {
    traps_overflow("let x = 3000.0\nlet y = x as i8\ny as int");
    traps_overflow("let x = 1.0 / 0.0\nlet y = x as int\ny");
}

#[test]
fn float_nan_to_int_traps() {
    traps_overflow("let x = 0.0 / 0.0\nlet y = x as int\ny");
}

// -- float → float ------------------------------------------------------

/// `1.1` is not representable in f32, so rounding through f32 moves it.
#[test]
fn f64_to_f32_loses_precision() {
    exits_zero("if (1.1 as f32) == 1.1 { 1 } else { 0 }");
    exits_zero("if (1.5 as f32) == 1.5 { 0 } else { 1 }");
}

// -- int → int ----------------------------------------------------------

#[test]
fn widening_keeps_the_value() {
    exits_zero("if (40 as i64) as int == 40 { 0 } else { 1 }");
    exits_zero("if (255 as u8) as int == 255 { 0 } else { 1 }");
}

#[test]
fn narrowing_range_checks() {
    exits_zero("if (127 as i8) as int == 127 { 0 } else { 1 }");
    traps_overflow("let x = 300 as i8\nx as int");
    traps_overflow("let x = 0 - 1\nlet y = x as u8\ny as int");
}

/// A `wrapping[T]` target is how intended overflow is spelled: the cast
/// reduces by the mode instead of trapping.
#[test]
fn wrapping_and_saturating_targets_reduce() {
    exits_zero("if (300 as wrapping[u8]) == 44 { 0 } else { 1 }");
    exits_zero("if (300 as saturating[i8]) == 127 { 0 } else { 1 }");
}

// -- the non-bridges ----------------------------------------------------

/// The bool COLUMN of the matrix (issue #18 item 2, pin `f0da6e6`): nothing
/// casts to `bool` — not an int, not inside `unsafe` (the counterparty
/// rejects there too, observed on the retired T1 trigger). E0805 at the
/// whole cast expression, statically, at this machine's resolve rung.
#[test]
fn nothing_casts_to_bool() {
    for body in [
        "let n = 3\nlet b = n as bool\n0",
        "let b = 1 as bool\n0",
        "unsafe { let b = 7 as bool\n0 }",
    ] {
        let observation = observe(body);
        assert_eq!(
            observation.verdict,
            Verdict::Fail("E0805".to_owned()),
            "body: {body}\nreason: {:?}",
            observation.reason
        );
        assert_eq!(observation.phase_reached, Phase::Resolve, "body: {body}");
    }
}

#[test]
fn bool_does_not_cast_to_numbers() {
    // Statically since 0.1.5 (pin f0da6e6): `corpus/typecheck/cast_bad.lu`
    // pins fail(E0805) at resolve, and sema-lite sees the literal-bound
    // `bool`, so the refusal moved from a dynamic decline to the pinned
    // rejection.
    fails_e0805("let b = true\nlet n = b as int\nn");
    fails_e0805("let b = false\nlet f = b as f64\n0");
}

#[test]
fn numbers_do_not_cast_to_str() {
    // Same move: nothing casts to `str` (interpolation is the rendering
    // surface), statically at the literal-visible shapes.
    fails_e0805("let s = 3 as str\n0");
    fails_e0805("let s = 3.5 as str\n0");
}

fn fails_e0805(body: &str) {
    let observation = observe(body);
    assert_eq!(
        observation.verdict,
        Verdict::Fail("E0805".to_owned()),
        "body: {body}\nreason: {:?}",
        observation.reason
    );
    assert_eq!(observation.phase_reached, Phase::Resolve, "body: {body}");
}

// -- adapters and round trips -------------------------------------------

/// `corpus/typecheck/cast_set.lu`'s adapter shape stays free and
/// bidirectional, range-checked like everything numeric.
#[test]
fn adapter_casts_stay_free() {
    let source = "\
type Meters = distinct int

fn main() -> !int {
    let m = 2 as Meters
    if m as int == 2 { 0 } else { 1 }
}
";
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(observation.verdict, Verdict::Exit(0));
}

#[test]
fn round_trips_hold() {
    exits_zero("if ((3 as f64) as int) == 3 { 0 } else { 1 }");
    exits_zero("if ((7 as i64) as i8) as int == 7 { 0 } else { 1 }");
}

/// Distinct-type comparisons keep their pre-fix reading: `1.0 == 1` is
/// `false` (distinct types), which wolfc refuses statically (E0401) — the
/// standing conservatism class, not a divergence.
#[test]
fn mixed_equality_stays_false() {
    exits_zero("if 1.0 == 1 { 1 } else { 0 }");
    exits_zero("if 3.0 == 3 { 1 } else { 0 }");
}

// -- issue #17: the target type is resolved, and `str` is not a source ---

/// A cast target that names nothing is refused with the counterparty's code
/// and span (issue #17 ask 1).
///
/// Through 0.1.9 `s as nonsense` ran to `exit(0)` with the string passed
/// through unchanged: the type expression was never resolved, so a typo in a
/// cast target was invisible. The span is the TYPE NAME, not the cast
/// expression — verified against wolfgang at pin `613c3dc`, which answers
/// `E0301` `[55,63]` for the `nonsense` case and `[55,60]` for `bytes`
/// (`bytes` names no type in this language either; `[mem.str.cmp]` puts the
/// string library in-library with no bytes accessor).
#[test]
fn a_cast_to_a_name_that_resolves_nowhere_is_refused() {
    for (target, name) in [("nonsense", "nonsense"), ("bytes", "bytes")] {
        let source = format!(
            "fn main() -> int {{\n    let s = \"wolf\"\n    let x = s as {target}\n    0\n}}\n"
        );
        let observation = frontend::observe(source.as_bytes(), None);
        assert_eq!(
            observation.verdict,
            Verdict::Fail("E0301".to_owned()),
            "target `{target}`: {:?}",
            observation.reason
        );
        assert_eq!(observation.phase_reached, Phase::Resolve, "{target}");
        let diagnostic = observation
            .diagnostics
            .first()
            .expect("the refusal carries its diagnostic");
        let span = diagnostic.span;
        let spanned = &source[span[0] as usize..span[1] as usize];
        assert_eq!(spanned, name, "the span covers the type name alone");
    }
}

/// The names that DO resolve keep working: every built-in scalar, and a type
/// the module declares. This is the other half of ask 1 — a resolution check
/// that refused real types would be worse than the bug.
#[test]
fn casts_to_resolvable_targets_are_untouched() {
    for target in [
        "int", "uint", "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128",
    ] {
        exits_zero(&format!(
            "if (3 as {target}) as int == 3 {{ 0 }} else {{ 1 }}"
        ));
    }
    exits_zero("if (3 as f64) == 3.0 { 0 } else { 1 }");
    exits_zero("if (3 as f32) == 3.0 { 0 } else { 1 }");

    // A module-declared lower-case type name resolves like any other item —
    // the check is on resolution, never on spelling.
    let source = "\
type meters = distinct int

fn main() -> !int {
    let m = 2 as meters
    if m as int == 2 { 0 } else { 1 }
}
";
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "{:?}",
        observation.reason
    );

    // An upper-case name is a nominal type this rung has no registry for, so
    // it declines to judge rather than guessing (`x as T` in a generic body,
    // a prelude type, an imported struct).
    let source = "\
fn id[T](x: T) -> T {
    x as T
}

fn main() -> !int {
    if id(7) == 7 { 0 } else { 1 }
}
";
    let observation = frontend::observe(source.as_bytes(), None);
    assert_ne!(
        observation.verdict,
        Verdict::Fail("E0301".to_owned()),
        "a generic parameter is not an unresolved name: {:?}",
        observation.reason
    );
}

/// `str` is not a cast SOURCE (issue #17 ask 2). Where sema-lite can see the
/// operand's class it refuses with the counterparty's E0805 and span; where
/// it cannot — a call return is the typecheck rung, which this machine does
/// not perform — the run rung declines by name instead of passing the string
/// through as if it were a number.
#[test]
fn str_does_not_cast_to_a_number() {
    for target in ["int", "i64", "u8", "f64", "bool"] {
        let source = format!(
            "fn main() -> int {{\n    let s = \"wolf\"\n    let x = s as {target}\n    0\n}}\n"
        );
        let observation = frontend::observe(source.as_bytes(), None);
        assert_eq!(
            observation.verdict,
            Verdict::Fail("E0805".to_owned()),
            "target `{target}`: {:?}",
            observation.reason
        );
        let span = observation.diagnostics[0].span;
        assert_eq!(
            &source[span[0] as usize..span[1] as usize],
            format!("s as {target}"),
            "the span covers the whole cast expression"
        );
    }

    // The shape sema-lite cannot classify: an honest decline, never exit(0)
    // with a `str` standing in for a number.
    let source = "\
fn name() -> str {
    \"wolf\"
}

fn main() -> int {
    let x = name() as int
    print(\"{x}\")
    0
}
";
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(
        observation.verdict,
        Verdict::Unsupported,
        "{:?}",
        observation.reason
    );
    let reason = observation.reason.unwrap_or_default();
    assert!(reason.contains("not in the cast set"), "{reason}");
    assert!(reason.contains("E0805"), "{reason}");
}

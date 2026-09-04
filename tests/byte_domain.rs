//! is37 — the byte's DOMAIN (`[type.byte]`, D72; wolf-interp#62).
//!
//! is36 shipped the byte TYPE. sc35 re-measured wolf-std against it and found
//! what was missing: `var b = List[byte]()` followed by `(mut b).push(256)`
//! **stored 256** here, an out-of-domain `int` flowed into a byte slot, and
//! the compilers refuse every one of those programs at typecheck with E0401.
//! Eight wolf-std rows carried `divergent(…)` for exactly that shape.
//!
//! The domain is not a range check bolted onto a value — `Value::Byte` is a
//! `u8`, so it cannot hold 256 in the first place. What was missing is the
//! REFUSAL at the boundary: `[type.byte]` says a `byte` "adopts no numeric
//! literal in any position" and is not an integer type, so an `int` reaching
//! a byte slot is a type error, not a conversion, whatever its value. The
//! refusal lands at `resolve`, at the offending operand's span, and both are
//! the counterparty's — every span below was observed with
//! `wolf conform-run --json` at pin `982f857` (wolf-lang v0.2.4) before it
//! was written down.
//!
//! `[type.byte.op]`'s widening is the other half and is deliberately NOT a
//! finding: `b + 1` is an `int` by clause, so the pass says nothing about it.
//! `b += 1` is, because the compound assignment stores that `int` back.

use wolf_interp::frontend;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;

/// Observes `source` at the `resolve` rung and returns the first diagnostic's
/// code with the source text under its span — "the right code" is half a
/// claim, so every assertion below carries the bytes too.
fn refusal(source: &str) -> (String, String) {
    let observed = frontend::observe(source.as_bytes(), Some(Phase::Resolve));
    let diag = observed
        .detail
        .unwrap_or_else(|| panic!("{source:?} was not refused: {:?}", observed.verdict));
    (
        diag.code.to_owned(),
        source[diag.span.start..diag.span.end].to_owned(),
    )
}

/// Asserts the program reaches its requested rung with nothing to say.
fn clean(source: &str) {
    let observed = frontend::observe(source.as_bytes(), Some(Phase::Resolve));
    assert!(
        matches!(observed.verdict, Verdict::Pass),
        "{source:?} wanted a clean resolve, got {:?} ({:?})",
        observed.verdict,
        observed.detail
    );
}

fn program(body: &str) -> String {
    format!("fn main() -> !int {{\n{body}\n    0\n}}\n")
}

// ---------------------------------------------------------------------------
// A `byte` adopts no numeric literal, in any position.
// ---------------------------------------------------------------------------

#[test]
fn a_byte_annotation_adopts_no_literal() {
    // The clause's own negative, and `char`'s word for word: `let b: byte =
    // 65` is not a narrowing — it is a literal that has no byte to become.
    // wolfc spans the `65`, not the annotation and not the statement.
    assert_eq!(
        refusal(&program("    let b: byte = 65")),
        ("E0401".to_owned(), "65".to_owned())
    );
}

#[test]
fn a_concrete_int_never_narrows_into_a_byte() {
    assert_eq!(
        refusal(&program("    let n = 300\n    let c: byte = n")),
        ("E0401".to_owned(), "n".to_owned())
    );
}

#[test]
fn the_cast_is_the_spelling_and_it_is_clean() {
    // `[type.byte.cast]`: `as byte` truncates by clause, so a spelled
    // conversion is never a finding — that is why only the UN-CAST flow
    // needs checking at all.
    clean(&program("    let b: byte = 65 as byte\n    print(\"{b}\")"));
}

#[test]
fn a_byte_satisfies_no_integer_expectation_either() {
    // The reverse direction, refused on the same grounds: the total bridge
    // is spelled `b as int`.
    assert_eq!(
        refusal(&program("    let b = 65 as byte\n    let n: int = b")),
        ("E0401".to_owned(), "b".to_owned())
    );
    clean(&program(
        "    let b = 65 as byte\n    let n: int = b as int\n    print(\"{n}\")",
    ));
}

// ---------------------------------------------------------------------------
// The four slots an `int` can reach a `byte` through.
// ---------------------------------------------------------------------------

#[test]
fn a_list_of_bytes_holds_no_int_however_small() {
    // sc35's shape, and the whole of wolf-interp#62: `push(256)` stored 256.
    // `push(65)` is refused too — the domain is not the question the type
    // asks, and wolfc answers E0401 at the `65` (measured, pin `982f857`).
    assert_eq!(
        refusal(&program("    var b = List[byte]()\n    (mut b).push(256)")),
        ("E0401".to_owned(), "256".to_owned())
    );
    assert_eq!(
        refusal(&program("    var b = List[byte]()\n    (mut b).push(65)")),
        ("E0401".to_owned(), "65".to_owned())
    );
    clean(&program(
        "    var b = List[byte]()\n    (mut b).push(65 as byte)\n    print(\"{b.len}\")",
    ));
}

#[test]
fn an_assignment_to_a_byte_place_takes_no_int() {
    assert_eq!(
        refusal(&program("    var b = 65 as byte\n    b = 66")),
        ("E0401".to_owned(), "66".to_owned())
    );
}

#[test]
fn a_struct_field_declared_byte_takes_no_int() {
    let source = format!(
        "struct S {{ b: byte }}\n{}",
        program("    let s = S { b: 65 }")
    );
    assert_eq!(refusal(&source), ("E0401".to_owned(), "65".to_owned()));
}

#[test]
fn a_parameter_declared_byte_takes_no_int() {
    // wolf-interp#61 left the parameter boundary conservative dynamically.
    // Statically it is the same one rule, and it is where wolfc answers.
    let source = format!(
        "fn take_one(b: byte) -> int {{ b as int }}\n{}",
        program("    print(\"{take_one(65)}\")")
    );
    assert_eq!(refusal(&source), ("E0401".to_owned(), "65".to_owned()));
}

#[test]
fn a_return_type_declared_byte_takes_no_int() {
    let source = format!(
        "fn eight() -> byte {{ 8 }}\n{}",
        program("    print(\"{eight()}\")")
    );
    assert_eq!(refusal(&source), ("E0401".to_owned(), "8".to_owned()));
}

// ---------------------------------------------------------------------------
// `[type.byte.op]` — what widens, and what therefore does not.
// ---------------------------------------------------------------------------

#[test]
fn arithmetic_on_a_byte_is_never_a_finding() {
    // "every arithmetic and bitwise operator widens its byte operand to `int`
    // first, and yields `int`". So `b + 1` is an `int` expression and the
    // pass has nothing to say — measured against wolfc, which accepts it.
    clean(&program(
        "    let b = 65 as byte\n    let s = b + 1\n    print(\"{s}\")",
    ));
    clean(&program(
        "    let b = 65 as byte\n    let m = b & 15\n    print(\"{m}\")",
    ));
}

#[test]
fn compound_assignment_is_the_named_consequence() {
    // `b += 1` assigns `b + 1` — an `int` — to a `byte` place. The clause
    // draws this consequence by name.
    assert_eq!(
        refusal(&program("    var b = 65 as byte\n    b += 1")),
        ("E0401".to_owned(), "1".to_owned())
    );
}

#[test]
fn a_comparison_takes_two_operands_of_one_type() {
    // Comparisons do NOT widen — two bytes compare in octet order — so a
    // `byte` against an `int` literal is the mismatch it looks like, and
    // wolfc spans the INT side, the operand that has to change.
    assert_eq!(
        refusal(&program(
            "    let b = 65 as byte\n    if b == 65 { return 1 }"
        )),
        ("E0401".to_owned(), "65".to_owned())
    );
    clean(&program(
        "    let b = 65 as byte\n    if b == (66 as byte) { return 1 }",
    ));
}

#[test]
fn a_byte_is_not_a_subscript() {
    // `[type.byte]`: "no indexing with one". The counterparty spans the
    // subscript.
    let body = "    let table = List[int]()\n    let s = \"wolf\"\n    let b = s.bytes()[0]\n    let hit = table[b]";
    assert_eq!(
        refusal(&program(body)),
        ("E0401".to_owned(), "b".to_owned())
    );
}

// ---------------------------------------------------------------------------
// The pass never guesses.
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_side_says_nothing() {
    // Both sides must be KNOWN. A loop element, a match binding and a
    // handler binding are all names this pass does not model; declaring them
    // `Unknown` is what keeps an outer `b: byte` from being read through a
    // shadow that is not one.
    clean(&program(
        "    let b = 65 as byte\n    for b in 0..3 { print(\"{b}\") }",
    ));
    clean(&program(
        "    let xs = List[int]()\n    let i = 0\n    print(\"{xs.len} {i}\")",
    ));
}

#[test]
fn a_list_this_pass_cannot_type_is_never_judged() {
    // `List()` carries no element annotation, so nothing about its elements
    // is knowable here and nothing is claimed.
    clean(&program("    var xs = List()\n    (mut xs).push(256)"));
}

// ---------------------------------------------------------------------------
// Span parity, against the pinned witnesses.
// ---------------------------------------------------------------------------

/// The two corpus files s135 and s136 landed for this rule, with the spans
/// `wolf conform-run --json` reported for them at pin `982f857`. A pin bump
/// that moves either file moves this test, which is the point: the claim is
/// "byte-identical to the counterparty", and it is checked against the bytes.
#[test]
fn the_pinned_witnesses_answer_wolfc_span_for_span() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus/typecheck");
    for (file, code, span) in [
        ("byte_narrow_fail.lu", "E0401", [588usize, 590]),
        ("byte_elem_arith_fail.lu", "E0401", [849, 850]),
    ] {
        let source = std::fs::read_to_string(root.join(file)).expect("witness readable");
        let observed = frontend::observe(source.as_bytes(), Some(Phase::Resolve));
        let diag = observed
            .detail
            .unwrap_or_else(|| panic!("{file} was not refused: {:?}", observed.verdict));
        assert_eq!(
            (diag.code, [diag.span.start, diag.span.end]),
            (code, span),
            "{file}: the counterparty's code and span are the contract"
        );
    }
}

// ---------------------------------------------------------------------------
// The dynamic half: a `List[byte]` knows its element type.
// ---------------------------------------------------------------------------

/// Runs a program through the `run` rung and returns the observation.
fn observe_run(source: &str) -> wolf_interp::frontend::Observation {
    frontend::observe(source.as_bytes(), None)
}

#[test]
fn a_list_of_bytes_carries_its_element_type_at_runtime() {
    // Until is37 the element context could spell integer widths and nothing
    // else, so `List[byte]()` and `List()` were the same value at runtime and
    // wolf-interp#62's `push(256)` stored 256. The static pass catches the
    // literal; this is the flow it declines to guess about — the pushed value
    // comes out of a container this pass cannot type — and the list itself is
    // what refuses.
    let source = "\
fn main() -> !int {
    var src = List()
    (mut src).push(256)
    var out = List[byte]()
    (mut out).push(src[0])
    print(\"{out[0]}\")
    0
}
";
    let observed = observe_run(source);
    assert_eq!(
        observed.verdict,
        Verdict::Unsupported,
        "an int must never land in a byte list: {observed:?}"
    );
    let reason = observed.reason.unwrap_or_default();
    assert!(reason.contains("List[byte]"), "{reason}");
    assert!(
        reason.contains("as byte"),
        "the note names the fix: {reason}"
    );
    assert!(
        reason.contains("E0401"),
        "and the code that owns it: {reason}"
    );
}

#[test]
fn a_byte_producer_hands_back_a_list_that_knows_its_element() {
    // `s.bytes()` is a `List[byte]` (s136, wolf-lang#231), so the list it
    // returns carries the element type too — pushing an `int` into it is the
    // same refusal as pushing one into `List[byte]()`.
    let source = "\
fn main() -> !int {
    var src = List()
    (mut src).push(66)
    var bs = \"wolf\".bytes()
    (mut bs).push(src[0])
    print(\"{bs.len}\")
    0
}
";
    let observed = observe_run(source);
    assert_eq!(observed.verdict, Verdict::Unsupported, "{observed:?}");
    assert!(
        observed.reason.unwrap_or_default().contains("List[byte]"),
        "the byte view's element type survives the hand-back"
    );
}

#[test]
fn a_byte_list_still_takes_bytes_and_still_strides_by_one() {
    // The refusal must not cost the type its ordinary use: a `byte` pushed
    // into a `List[byte]` is exactly what the container is for.
    let source = "\
fn main() -> !int {
    var out = List[byte]()
    (mut out).push(65 as byte)
    (mut out).push(300 as byte)
    print(\"{out[0]} {out[1]} {out.len}\")
    0
}
";
    let observed = observe_run(source);
    assert_eq!(observed.verdict, Verdict::Exit(0), "{observed:?}");
    assert_eq!(String::from_utf8_lossy(&observed.stdout), "65 44 2\n");
}

#[test]
fn a_list_of_ints_is_untouched_by_any_of_this() {
    // The element context still does its original job (issue #21): a pushed
    // literal adopts the container's integer width, and an out-of-width
    // literal still traps rather than being refused by name.
    let source = "\
fn main() -> !int {
    var xs = List[i32]()
    (mut xs).push(7)
    print(\"{xs[0]}\")
    0
}
";
    let observed = observe_run(source);
    assert_eq!(observed.verdict, Verdict::Exit(0), "{observed:?}");
    assert_eq!(String::from_utf8_lossy(&observed.stdout), "7\n");
}

//! s285 — a literal pattern outside its scrutinee's type, mirrored from the
//! compiler (wolf-lang#1058, `[gram.pat.lit]`).
//!
//! A literal pattern takes its scrutinee's type. One that type cannot hold is
//! the compiler's E0415 at the literal, on the checked machine, native and
//! release; before, native and release wrapped it into the scrutinee's width
//! and tested a different number. This machine has no checker, so it answers
//! E0415 as it answers every other E0415: with the dynamic counterpart, the
//! overflow trap, at the arm's literal, when `match` tests it. A program stops
//! at its first trap, which is the compiler's first E0415. A signed literal
//! pattern, which the grammar does not admit, is the compiler's static E0207 at
//! the signed literal, and this machine's too.
//!
//! One directory per shape under `tests/literal_pattern_s285/`, each the
//! compiler's corpus row byte for byte (wolf-lang's `s285b` branch:
//! `corpus/typecheck/numlit_pattern_*.lu`,
//! `corpus/grammar/match_pattern_signed_literal.lu`,
//! `corpus/rows/match_literal_pattern_edges.lu`). The spans are the compiler's
//! (`wolf conform-run --json`, every tier alike).

use std::path::Path;

use wolf_interp::Observed;
use wolf_interp::protocol::ObservationRecord;

/// The record, and the human-facing half (the trap's and the diagnostic's
/// words, never on the wire).
fn observe(name: &str) -> (ObservationRecord, Observed) {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("literal_pattern_s285")
        .join(name)
        .join("main.lu");
    let source = std::fs::read(&file).expect("the shape carries its main.lu");
    wolf_interp::observe_record(&file, &source, None)
}

/// The run traps `overflow` at `span`, the compiler's first E0415 in the row,
/// and says why.
fn traps_at(name: &str, span: [u64; 2], scrutinee: &str) {
    let (r, seen) = observe(name);
    assert_eq!(r.verdict.to_string(), "trap(overflow)", "{name}: {r:?}");
    assert_eq!(
        r.extensions.get("x-trap-span"),
        Some(&serde_json::json!(span)),
        "{name}: the trap sits at the compiler's first E0415: {r:?}"
    );
    let message = seen.trap.map(|t| t.message).unwrap_or_default();
    for text in [scrutinee, "[gram.pat.lit]", "E0415"] {
        assert!(
            message.contains(text),
            "{name}: the trap message lacks {text:?}: {message}"
        );
    }
}

/// wolf-lang#1058's program: `300` over a `u8` holding 44. The arm never
/// matched here; native and release took it.
#[test]
fn an_arm_literal_outside_a_u8_traps_at_the_literal() {
    traps_at("wraps", [537, 540], "`u8`");
}

/// Every primitive's max+1; the first called is the `i8` one.
#[test]
fn the_first_primitive_past_its_maximum_traps() {
    traps_at("edges", [702, 705], "`i8`");
}

/// A tuple element is the first position the row reaches.
#[test]
fn a_nested_literal_outside_its_type_traps() {
    traps_at("positions", [714, 717], "`u8`");
}

/// A range's high end outside a `u8`.
#[test]
fn a_range_end_outside_its_type_traps() {
    traps_at("range", [749, 752], "`u8`");
}

/// A literal scrutinee reads as its `i32` default.
#[test]
fn a_literal_scrutinee_holds_its_pattern_to_i32() {
    traps_at("default", [627, 637], "`i32`");
}

/// A signed literal pattern is E0207, static, at the signed literal: the
/// first of the compiler's four, `[proto.record.first]`.
#[test]
fn a_signed_literal_pattern_is_refused_by_name() {
    let (r, seen) = observe("signed");
    assert_eq!(r.verdict.to_string(), "fail(E0207)", "signed: {r:?}");
    let words = seen.diagnostic.map(|d| d.message).unwrap_or_default();
    assert!(
        words.contains("a literal pattern has no sign"),
        "signed: the compiler's words: {words}"
    );
    let first = r
        .diagnostics
        .iter()
        .find(|d| d.severity != "warning")
        .map(|d| (d.code.clone(), d.span));
    assert_eq!(
        first,
        Some(("E0207".to_owned(), [607, 609])),
        "signed: the compiler's first E0207: {r:?}"
    );
}

/// The in-range edge of every primitive matches, as before.
#[test]
fn every_primitives_edge_literal_still_matches() {
    let (r, _) = observe("edge_values");
    assert_eq!(r.verdict.to_string(), "exit(0)", "edge_values: {r:?}");
    assert_eq!(
        r.stdout_inline.as_deref(),
        Some("i8 i16 i32 i64 int u8 u16 u32 u64 uint\n0 x 2p63 wrap nested\n"),
        "edge_values: {r:?}"
    );
}

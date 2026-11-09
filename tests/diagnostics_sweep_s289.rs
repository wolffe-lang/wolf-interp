//! s289 — the compiler's diagnostics sweep, first change (wolf-lang#1085),
//! mirrored: lupin gives the compiler's code at the compiler's span.
//!
//! A capitalized name that nothing declares is read as an error tag, and a
//! tag is a value only where an error row takes it. Where none does, the
//! compiler says E0301 at the name ("nothing named `OPEN` is in scope"), and
//! where the function spells the tag in a row but cannot fail, E0604. This
//! machine read every such name as a tag value and ran the program (`error:
//! OPEN`, exit 1).
//!
//! One directory per row under `tests/diagnostics_sweep_s289/`: `main.lu` (and
//! any module beside it) is the compiler's corpus row byte for byte
//! (wolf-lang's s289 branch at `67bd054f`), and `compiler.json` is the
//! compiler's record of it (`wolf conform-run --json --checked`; native and
//! release agree on every row). The tests carry the names of the compiler's
//! four-machine test (`crates/wolf_driver/tests/diagnostics_sweep_lanes.rs`).
//!
//! A refusal row holds this machine's record to the compiler's: the verdict,
//! the first error, and every `{code, span, severity}` entry. The phase differs
//! by design: this machine's one static rung is `resolve`, and
//! `[proto.cmp.rung]` makes a resolve-rung emission of a checker code
//! agreement.

use std::path::{Path, PathBuf};

use wolf_interp::Observed;
use wolf_interp::protocol::ObservationRecord;

fn row(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("diagnostics_sweep_s289")
        .join(name)
}

/// This machine's record of the row, and the human half (never on the wire).
fn observe(name: &str) -> (ObservationRecord, Observed) {
    let file = row(name).join("main.lu");
    let source = std::fs::read(&file).expect("the row carries its main.lu");
    wolf_interp::observe_record(&file, &source, None)
}

/// The compiler's record of the row.
fn compiler(name: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(row(name).join("compiler.json"))
        .expect("the row carries the compiler's record");
    serde_json::from_str(&text).expect("the compiler's record is JSON")
}

/// `{code, span, severity}` entries, sorted by span, then severity, then
/// code, as one JSON line: the form both records are compared in.
fn entries(diagnostics: &serde_json::Value) -> String {
    let mut out: Vec<(u64, u64, String, String)> = diagnostics
        .as_array()
        .expect("diagnostics is an array")
        .iter()
        .map(|d| {
            (
                d["span"][0].as_u64().expect("a span start"),
                d["span"][1].as_u64().expect("a span end"),
                d["severity"].as_str().expect("a severity").to_owned(),
                d["code"].as_str().expect("a code").to_owned(),
            )
        })
        .collect();
    out.sort();
    let json: Vec<serde_json::Value> = out
        .into_iter()
        .map(|(start, end, severity, code)| {
            serde_json::json!({"code": code, "severity": severity, "span": [start, end]})
        })
        .collect();
    serde_json::to_string(&json).expect("serializes")
}

/// The first error of a record, by `[proto.record.first]`'s order.
fn first_error(diagnostics: &serde_json::Value) -> Option<(String, [u64; 2])> {
    diagnostics
        .as_array()?
        .iter()
        .filter(|d| d["severity"] == "error")
        .map(|d| {
            (
                d["code"].as_str().unwrap_or("?").to_owned(),
                [
                    d["span"][0].as_u64().unwrap_or(u64::MAX),
                    d["span"][1].as_u64().unwrap_or(u64::MAX),
                ],
            )
        })
        .min_by_key(|(_, span)| *span)
}

/// This machine refuses `name` by `code`, with the compiler's first error
/// and the compiler's diagnostics, and its words say each of `texts`.
fn refused(name: &str, code: &str, texts: &[&str]) {
    let want = compiler(name);
    let verdict = format!("fail({code})");
    assert_eq!(
        want["verdict"],
        verdict.as_str(),
        "{name}: the stored record"
    );
    let (record, seen) = observe(name);
    let got = serde_json::to_value(&record).expect("the record serializes");
    assert_eq!(got["verdict"], verdict.as_str(), "{name}: {got}");
    assert_eq!(
        first_error(&got["diagnostics"]),
        first_error(&want["diagnostics"]),
        "{name}: the first error"
    );
    assert_eq!(
        entries(&got["diagnostics"]),
        entries(&want["diagnostics"]),
        "{name}: the diagnostics"
    );
    let words = seen.diagnostic.map(|d| d.message).unwrap_or_default();
    for text in texts {
        assert!(
            words.contains(text),
            "{name}: the words lack {text:?}: {words}"
        );
    }
}

// ------------------------- wolf-lang#1085, an undeclared capitalized name --

/// The tail of a function that returns `int`.
#[test]
fn an_undeclared_name_as_a_tail_is_e0301() {
    refused(
        "tag_undeclared_tail",
        "E0301",
        &["nothing named `OPEN` is in scope", "make `answer` fallible"],
    );
}

/// Assigned to a field: no raise position, so no `!T` advice.
#[test]
fn an_undeclared_name_assigned_is_e0301_without_the_fallible_advice() {
    refused(
        "tag_undeclared_assign",
        "E0301",
        &["nothing named `INPUT` is in scope"],
    );
    let (_, seen) = observe("tag_undeclared_assign");
    let words = seen.diagnostic.map(|d| d.message).unwrap_or_default();
    assert!(!words.contains("fallible"), "{words}");
}

#[test]
fn an_undeclared_name_returned_is_e0301() {
    refused(
        "tag_undeclared_return",
        "E0301",
        &[
            "nothing named `CANCEL` is in scope",
            "make `answer` fallible",
        ],
    );
}

#[test]
fn an_undeclared_name_in_a_let_is_e0301() {
    refused(
        "tag_undeclared_let",
        "E0301",
        &["nothing named `OPEN` is in scope"],
    );
}

/// The call form: E0301 at the name, not the call.
#[test]
fn an_undeclared_name_called_with_a_payload_is_e0301() {
    refused(
        "tag_undeclared_payload",
        "E0301",
        &["nothing named `Bad` is in scope"],
    );
}

/// Inside a function that can fail, a `let` is still no raise position.
#[test]
fn an_undeclared_name_in_a_fallible_functions_let_is_e0301() {
    refused(
        "tag_undeclared_fallible_let",
        "E0301",
        &["nothing named `Bad` is in scope", "`return Bad`"],
    );
}

/// The file imports a module that defines `OPEN`: the bare name still names
/// nothing here.
#[test]
fn a_bare_name_an_imported_module_defines_is_e0301() {
    refused(
        "tag_undeclared_module_item",
        "E0301",
        &["nothing named `OPEN` is in scope"],
    );
}

/// A tag the function spells, returned where it cannot fail.
#[test]
fn the_functions_own_tag_where_it_cannot_fail_is_e0604() {
    refused(
        "tag_known_cannot_fail",
        "E0604",
        &["`bad` is an error tag, and `answer` cannot fail"],
    );
}

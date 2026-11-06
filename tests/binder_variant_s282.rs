//! s282 — variant names in binder positions: W0801's binder half and the
//! `select` arm's E0806, mirrored from the compiler.
//!
//! A binder pattern (a statement `let`/`var` binder, a `for` header, the
//! `else |…|` handler's binding form) binds every value it meets. A
//! capitalized bare name in it that spells a variant of an enum in scope (the
//! module's own, or one the file imports by item) still binds, and warns
//! W0801 at the name. A `select` arm's receive takes every value its channel
//! delivers, so the same name there is refused (E0806).
//!
//! One directory per shape under `tests/binder_variant_s282/`, each the
//! compiler's corpus row of the same name byte for byte (`corpus/lints/
//! binder_variant_*.lu`, `corpus/conc/select_arm_*.lu` on wolf-lang's `s282`
//! branch). Each row pins the verdict and stdout every compiled tier answers,
//! and the warnings array (`wolf conform-run --json`, every tier alike): this
//! machine must answer that array exactly, codes and spans.

use std::path::Path;

use wolf_interp::protocol::Warning;

struct Answer {
    verdict: String,
    stdout: Option<String>,
    warnings: Vec<(String, [u64; 2])>,
    first_diagnostic: Option<(String, [u64; 2])>,
}

fn observe(name: &str) -> Answer {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("binder_variant_s282")
        .join(name)
        .join("main.lu");
    let source = std::fs::read(&file).expect("the shape carries its main.lu");
    let (record, _) = wolf_interp::observe_record(&file, &source, None);
    let warnings: &[Warning] = record
        .warnings
        .as_deref()
        .unwrap_or_else(|| panic!("{name}: the analyses did not run"));
    let mut warnings: Vec<(String, [u64; 2])> =
        warnings.iter().map(|w| (w.code.clone(), w.span)).collect();
    warnings.sort();
    Answer {
        verdict: record.verdict.to_string(),
        stdout: record.stdout_inline.clone(),
        warnings,
        first_diagnostic: record
            .diagnostics
            .iter()
            .find(|d| d.severity != "warning")
            .map(|d| (d.code.clone(), d.span)),
    }
}

fn w0801(spans: &[[u64; 2]]) -> Vec<(String, [u64; 2])> {
    spans.iter().map(|s| ("W0801".to_owned(), *s)).collect()
}

fn runs(name: &str, stdout: &str, compiler: &[[u64; 2]]) {
    let a = observe(name);
    assert_eq!(a.verdict, "exit(0)", "{name}: the program runs and binds");
    assert_eq!(a.stdout.as_deref(), Some(stdout), "{name}: what the names bound");
    assert_eq!(
        a.warnings,
        w0801(compiler),
        "{name}: lupin's warnings against the compiler's"
    );
}

#[test]
fn let_and_var_binders_that_spell_a_variant_warn() {
    runs(
        "let",
        "food 3\ntip 4\ndrink\ntip 2\ndrink 1 2\nfood 5 6\n7 8\n",
        &[
            [978, 983],
            [1074, 1078],
            [1176, 1184],
            [1253, 1258],
            [1361, 1365],
            [1449, 1457],
            [1553, 1557],
        ],
    );
}

#[test]
fn a_for_header_that_spells_a_variant_warns() {
    runs("for", "food1tip2\ndrinktip\n", &[[710, 715], [794, 798]]);
}

/// The handler's name binds the error, as the compiler binds it. Before this
/// row it read as a variant test and the run refused ("the `else |pat|`
/// pattern did not match the error value").
#[test]
fn an_else_handler_name_that_spells_a_variant_binds_and_warns() {
    runs("else", "7 5\n", &[[568, 573], [603, 607]]);
}

#[test]
fn an_item_imported_enums_variant_warns_in_a_binder() {
    runs("imported", "tip 3\n", &[[412, 417]]);
}

/// A lowercase binder, a capitalized name that spells no variant, and a
/// function and closure parameter named like a variant: silent.
#[test]
fn binders_and_parameters_that_spell_no_test_stay_silent() {
    runs("controls", "food 3\n5 3\n8\n5\n", &[]);
}

/// Before this row the arm read as a variant test and the run refused ("the
/// committed `select` arm's pattern did not match the received value").
#[test]
fn a_select_arm_that_spells_a_variant_is_refused() {
    let a = observe("select_variant");
    assert_eq!(a.verdict, "fail(E0806)", "a static refusal");
    assert_eq!(
        a.first_diagnostic,
        Some(("E0806".to_owned(), [659, 664])),
        "at the name, where the compiler refuses it"
    );
}

#[test]
fn a_select_arm_that_binds_a_lowercase_name_runs() {
    runs("select_lowercase", "food\n", &[]);
}

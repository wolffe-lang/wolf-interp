//! s288 — the compiler's front-end rulings of wolf-lang#1079 and #1080,
//! mirrored: lupin gives the compiler's code at the compiler's span.
//!
//! One directory per row under `tests/front_end_rulings_s288/`: `main.lu` is the
//! compiler's corpus row byte for byte (wolf-lang trunk `a57276f2`), and
//! `compiler.json` is the compiler's record of it (`wolf conform-run --json
//! --checked`; native and release agree on every row). The tests carry the
//! names of the compiler's four-machine test
//! (`crates/wolf_driver/tests/front_end_rulings_lanes.rs`).
//!
//! A refusal row holds this machine's record to the compiler's: the verdict,
//! the first error, and every `{code, span, severity}` entry of `diagnostics`,
//! warnings included, compared once both lists are sorted by span. This
//! machine writes the error first and its warnings after it; the compiler
//! writes span order. `[proto.cmp.phase]` compares the first error, and the
//! rows hold the whole set. The phase differs by design: this machine's one
//! static rung is `resolve`, and `[proto.cmp.rung]` makes a resolve-rung
//! emission of a checker code agreement.

use std::path::{Path, PathBuf};

use wolf_interp::Observed;
use wolf_interp::protocol::ObservationRecord;

fn row(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("front_end_rulings_s288")
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

/// This machine runs `name` to `exit(want)`, as the compiler does, with the
/// compiler's warnings.
fn runs(name: &str, want: u8) {
    let compiled = compiler(name);
    let verdict = format!("exit({want})");
    assert_eq!(
        compiled["verdict"],
        verdict.as_str(),
        "{name}: the stored record"
    );
    let (record, _) = observe(name);
    let got = serde_json::to_value(&record).expect("the record serializes");
    assert_eq!(got["verdict"], verdict.as_str(), "{name}: {got}");
    assert_eq!(
        entries(&got["diagnostics"]),
        entries(&compiled["diagnostics"]),
        "{name}: the diagnostics"
    );
}

// ------------------------------------------- wolf-lang#1064, alias cycles --

#[test]
fn an_unused_alias_that_names_itself_is_e0610() {
    refused("error_alias_cycle_unused", "E0610", &["names itself"]);
}

#[test]
fn an_unused_two_alias_loop_is_one_e0610() {
    refused("error_alias_cycle_pair_unused", "E0610", &["names itself"]);
}

/// An unused alias's own row: a tag spelled twice is E0601 at the second.
#[test]
fn an_unused_alias_that_spells_a_tag_twice_is_e0601() {
    refused(
        "error_alias_own_dup_unused",
        "E0601",
        &[
            "the tag `x` appears twice in this error row",
            "[gram.type.row.dup]",
        ],
    );
}

#[test]
fn a_used_alias_loop_keeps_one_e0610() {
    refused("error_alias_cycle", "E0610", &["names itself"]);
}

// ----------------------------- wolf-lang#1059, a nullary arm binds nothing --

/// The arm's `Done` tests `E.Done`, so `Done = 9` assigns the `let`.
#[test]
fn assigning_a_let_through_a_nullary_arm_is_e0410() {
    refused(
        "nullary_arm_let_assign",
        "E0410",
        &[
            "`Done` is bound with `let`, so it cannot be assigned again",
            "this pattern tests `Done` and binds nothing",
            "[gram.pat.nullary.scope]",
        ],
    );
}

#[test]
fn a_nullary_arm_writes_an_outer_var() {
    runs("nullary_arm_var_assign", 9);
}

#[test]
fn assigning_a_let_through_a_row_tag_arm_is_e0410() {
    refused(
        "nullary_arm_tag_assign",
        "E0410",
        &[
            "`gone` is bound with `let`, so it cannot be assigned again",
            "this pattern tests `gone` and binds nothing",
        ],
    );
}

#[test]
fn assigning_a_lets_field_through_a_nullary_arm_is_e0410() {
    refused(
        "nullary_arm_field_assign",
        "E0410",
        &[
            "`Done` is bound with `let`, so its fields cannot be assigned either",
            "this pattern tests `Done` and binds nothing",
        ],
    );
}

// ----------------- wolf-lang#1060, a variant or a field declared twice --

#[test]
fn a_variant_declared_twice_is_e0302() {
    refused(
        "enum_variant_twice",
        "E0302",
        &[
            "the variant `A` is declared twice in this enum",
            "[gram.item.type.names]",
        ],
    );
}

#[test]
fn a_variant_declared_twice_with_two_payloads_is_e0302() {
    refused(
        "enum_variant_twice_payload",
        "E0302",
        &["the variant `A` is declared twice in this enum"],
    );
}

#[test]
fn a_struct_field_declared_twice_is_e0302() {
    refused(
        "struct_field_twice",
        "E0302",
        &["the field `a` is declared twice in this struct"],
    );
}

/// The general spellings, `type … = enum {…}` and `= struct {…}`: two E0302s,
/// both in the record.
#[test]
fn the_general_type_spellings_refuse_a_member_declared_twice() {
    refused(
        "type_decl_member_twice",
        "E0302",
        &["the variant `A` is declared twice in this enum"],
    );
}

#[test]
fn variant_names_differing_in_case_are_two_variants() {
    runs("enum_variant_case_distinct", 2);
}

// ------------ wolf-lang#1062 and #1073, a name bound twice in one pattern --

#[test]
fn a_struct_pattern_that_binds_a_name_twice_is_e0827() {
    refused(
        "pattern_name_twice_struct",
        "E0827",
        &["`x` is bound twice in this pattern", "[gram.pat.names]"],
    );
}

#[test]
fn a_name_bound_twice_at_two_types_is_e0827() {
    refused(
        "pattern_name_twice_types",
        "E0827",
        &["`x` is bound twice in this pattern"],
    );
}

#[test]
fn a_let_pattern_that_binds_a_name_twice_is_e0827() {
    refused(
        "pattern_name_twice_let",
        "E0827",
        &["`x` is bound twice in this pattern"],
    );
}

#[test]
fn a_name_on_both_sides_of_at_is_e0827() {
    refused(
        "pattern_name_twice_at",
        "E0827",
        &["`x` is bound twice in this pattern"],
    );
}

#[test]
fn a_nested_name_bound_twice_is_e0827() {
    refused(
        "pattern_name_twice_nested",
        "E0827",
        &["`x` is bound twice in this pattern"],
    );
}

#[test]
fn a_for_header_that_binds_a_name_twice_is_e0827() {
    refused(
        "pattern_name_twice_for",
        "E0827",
        &["`x` is bound twice in this pattern"],
    );
}

#[test]
fn a_parameter_declared_twice_is_e0827() {
    refused(
        "param_name_twice",
        "E0827",
        &["`a` is bound twice in this parameter list"],
    );
}

#[test]
fn a_closure_parameter_declared_twice_is_e0827() {
    refused(
        "param_name_twice_closure",
        "E0827",
        &["`a` is bound twice in this parameter list"],
    );
}

#[test]
fn a_comma_group_rebinds_by_shadowing() {
    runs("let_group_shadow", 2);
}

#[test]
fn two_tests_of_one_variant_are_not_a_name_bound_twice() {
    runs("pattern_variant_tests_twice", 4);
}

// --------------- wolf-lang#1065, a tag spelled after an alias that has it --

#[test]
fn a_tag_spelled_after_its_alias_merges() {
    runs("error_alias_union_own_after", 1);
}

#[test]
fn a_payload_conflict_after_an_alias_is_e0609_at_the_spelling() {
    refused(
        "error_alias_union_own_after_payload",
        "E0609",
        &[
            "the tag `Bad` reaches this error row with two different payloads",
            "[type.err.alias.union]",
        ],
    );
}

#[test]
fn two_aliases_that_disagree_are_e0609_at_the_later() {
    refused(
        "error_alias_union_two_aliases_payload",
        "E0609",
        &["the tag `Bad` reaches this error row with two different payloads"],
    );
}

#[test]
fn a_tag_the_row_spells_twice_after_its_alias_is_e0601_at_the_second() {
    refused(
        "error_alias_union_own_twice",
        "E0601",
        &["the tag `x` appears twice in this error row"],
    );
}

#[test]
fn a_tag_the_row_spells_twice_is_e0601() {
    refused(
        "error_row_own_dup_bare",
        "E0601",
        &["the tag `x` appears twice in this error row"],
    );
    refused(
        "dup_tags",
        "E0601",
        &["the tag `Io` appears twice in this error row"],
    );
}

// ------------------ wolf-lang#1061, what an or-pattern's alternatives bind --

/// The compiled machines refuse these at `mem` (their join of an
/// or-pattern's bindings is not built yet, the conservatism ledger); this
/// machine runs them, and its checks find nothing.
fn or_pattern_runs(name: &str, want: u8) {
    let compiled = compiler(name);
    assert_eq!(
        compiled["verdict"], "unsupported",
        "{name}: the stored record"
    );
    assert_eq!(
        compiled["phase_reached"], "mem",
        "{name}: the stored record"
    );
    let (record, _) = observe(name);
    let got = serde_json::to_value(&record).expect("the record serializes");
    assert_eq!(
        got["verdict"],
        format!("exit({want})").as_str(),
        "{name}: {got}"
    );
    assert_eq!(first_error(&got["diagnostics"]), None, "{name}: {got}");
}

#[test]
fn an_or_pattern_whose_alternatives_agree_checks() {
    or_pattern_runs("or_pattern_binds", 3);
    or_pattern_runs("or_pattern_binds_nested", 3);
    or_pattern_runs("or_pattern_binds_at", 21);
}

#[test]
fn an_or_pattern_alternative_that_lacks_a_name_is_e0828() {
    refused(
        "or_pattern_binds_missing",
        "E0828",
        &[
            "`x` is bound by another alternative of this or-pattern, but not by this one",
            "[gram.pat.or]",
        ],
    );
}

#[test]
fn the_left_alternative_that_lacks_a_name_is_e0828() {
    refused(
        "or_pattern_binds_missing_left",
        "E0828",
        &["`x` is bound by another alternative of this or-pattern, but not by this one"],
    );
}

#[test]
fn an_or_pattern_name_at_two_types_is_e0401() {
    refused(
        "or_pattern_binds_types",
        "E0401",
        &["`x` is `int` in one alternative of this or-pattern and `bool` in this one"],
    );
}

// ---------------------- wolf-lang#1062, a type that contains itself by value --

#[test]
fn an_enum_that_contains_itself_by_value_is_e0404() {
    refused(
        "rec_by_value_enum",
        "E0404",
        &[
            "`L` contains itself by value, so it would have no finite size",
            "L -> L",
            "List[L]",
            "[type.rec]",
        ],
    );
}

#[test]
fn a_cycle_through_a_tuple_payload_is_e0404() {
    refused(
        "rec_by_value_tuple",
        "E0404",
        &["`L` contains itself by value"],
    );
}

#[test]
fn a_struct_and_enum_cycle_is_one_e0404() {
    refused(
        "rec_by_value_mutual",
        "E0404",
        &["`N` contains itself by value", "N -> L -> N"],
    );
}

#[test]
fn an_unused_struct_that_holds_itself_is_e0404() {
    refused(
        "rec_by_value_unused",
        "E0404",
        &["`N` contains itself by value"],
    );
}

#[test]
fn a_cycle_through_a_generic_struct_is_e0404() {
    refused(
        "rec_by_value_generic",
        "E0404",
        &["`L` contains itself by value", "L -> Box -> L"],
    );
}

#[test]
fn a_cycle_through_an_error_union_is_e0404() {
    refused(
        "rec_by_value_errunion",
        "E0404",
        &["`N` contains itself by value"],
    );
}

#[test]
fn recursion_through_containers_is_legal() {
    runs("rec_through_containers", 6);
}

// ---------------- wolf-lang#1062, a pattern's bindings bind as `let` does --

#[test]
fn assigning_a_match_binding_is_e0410() {
    refused(
        "match_binding_assign",
        "E0410",
        &[
            "`x` is bound by a pattern, so it cannot be assigned",
            "var x = x",
            "[gram.pat.let]",
        ],
    );
}

#[test]
fn assigning_a_for_binding_is_e0410() {
    refused(
        "for_binding_assign",
        "E0410",
        &["`x` is bound by a pattern, so it cannot be assigned"],
    );
}

#[test]
fn assigning_an_else_binding_is_e0410() {
    refused(
        "else_binding_assign",
        "E0410",
        &["`n` is bound by a pattern, so it cannot be assigned"],
    );
}

#[test]
fn assigning_a_select_binding_is_e0410() {
    refused(
        "select_binding_assign",
        "E0410",
        &["`n` is bound by a pattern, so it cannot be assigned"],
    );
}

#[test]
fn assigning_a_match_bindings_field_is_e0410() {
    refused(
        "match_binding_field_assign",
        "E0410",
        &["`p` is bound by a pattern, so its fields cannot be assigned either"],
    );
}

#[test]
fn assigning_an_at_name_is_e0410() {
    refused(
        "match_binding_at_assign",
        "E0410",
        &["`n` is bound by a pattern, so it cannot be assigned"],
    );
}

#[test]
fn the_mut_parameter_trap_is_e0410() {
    refused(
        "match_binding_mut_param",
        "E0410",
        &["`x` is bound by a pattern, so it cannot be assigned"],
    );
}

#[test]
fn a_var_shadow_of_a_match_binding_is_assignable() {
    runs("match_binding_var_shadow", 4);
}

// ------------------------------------------------- the compiler's edges --
//
// Programs of this lane's own, `probe_<shape>/main.lu`, each beside the
// compiler's record of it (wolf-lang `a57276f2`, `conform-run --json
// --checked`; native and release agree), measured before the mirror was
// written. They pin what the rows leave open: how many diagnostics a repeat
// gives, where a span starts and ends, and what stays legal.

/// Each repeat is its own diagnostic: `{x, x, x}` is two E0601s.
#[test]
fn every_repeat_of_a_tag_is_its_own_e0601() {
    refused(
        "probe_row_dup_three",
        "E0601",
        &["the tag `x` appears twice"],
    );
}

/// `{Io, Io(str)}`: E0601 at the second tag's name, not its payload, beside
/// the first's W0603.
#[test]
fn a_repeated_tag_is_refused_at_its_name() {
    refused(
        "probe_row_dup_payload_second",
        "E0601",
        &["the tag `Io` appears twice"],
    );
}

/// An alias entry is not a spelling of the row: `{A, A}` runs.
#[test]
fn an_alias_named_twice_in_a_row_runs() {
    runs("probe_alias_twice", 1);
}

/// `{Bad(str), A}`: the row's own spelling first, the alias after; E0609 at
/// the spelling all the same.
#[test]
fn a_payload_conflict_before_an_alias_is_e0609_at_the_spelling() {
    refused(
        "probe_own_before_alias_payload",
        "E0609",
        &["`Bad(str)` here"],
    );
}

/// Inside an alias's own declaration, used or not: at the later alias
/// entry, and at the declaration's own spelling.
#[test]
fn an_alias_declaration_is_judged_where_it_stands() {
    refused(
        "probe_alias_decl_payload_conflict",
        "E0609",
        &["from the alias `B`"],
    );
    refused(
        "probe_alias_decl_own_conflict",
        "E0609",
        &["`Bad(str)` here"],
    );
}

/// Three `A`s are two E0302s; a struct's two repeated fields are two.
#[test]
fn every_repeated_member_is_its_own_e0302() {
    refused(
        "probe_enum_three",
        "E0302",
        &["the variant `A` is declared twice"],
    );
    refused(
        "probe_struct_two_dups",
        "E0302",
        &["the field `a` is declared twice"],
    );
}

/// `(x, x, x)` is two E0827s, and a shorthand field binds its name.
#[test]
fn every_repeated_binding_is_its_own_e0827() {
    refused("probe_tuple_three", "E0827", &["`x` is bound twice"]);
    refused(
        "probe_struct_shorthand_twice",
        "E0827",
        &["`a` is bound twice"],
    );
}

/// `x @ A(x)`: the payload's `x` is the second binding.
#[test]
fn a_payload_under_an_at_binds_after_it() {
    refused("probe_at_payload", "E0827", &["`x` is bound twice"]);
}

/// `(A(x) | B(x), x)`: the or-pattern binds `x` once, and the tuple's second
/// `x` is a second binding.
#[test]
fn an_or_pattern_binds_its_names_once_in_the_pattern_around_it() {
    refused("probe_or_then_tuple", "E0827", &["`x` is bound twice"]);
}

/// Two alternatives that lack `x` are two E0828s.
#[test]
fn every_alternative_that_lacks_a_name_is_its_own_e0828() {
    refused(
        "probe_or_three",
        "E0828",
        &["this alternative binds no `x`"],
    );
}

/// A struct pattern's shorthand field takes the field's declared type.
#[test]
fn a_shorthand_field_at_two_types_is_e0401() {
    refused("probe_or_types_struct", "E0401", &["`n` is `int`"]);
}

/// A compound assignment, a field of a `for` binding, and a qualified arm
/// that binds nothing.
#[test]
fn every_write_to_a_pattern_binding_is_e0410() {
    refused(
        "probe_compound_arm",
        "E0410",
        &["`x` is bound by a pattern"],
    );
    refused(
        "probe_for_field_assign",
        "E0410",
        &["its fields cannot be assigned"],
    );
    refused(
        "probe_nullary_qualified",
        "E0410",
        &["`Done` is bound with `let`"],
    );
}

/// A closure's parameter stays assignable (wolf-lang#1076 is open).
#[test]
fn a_closure_parameter_is_still_assignable() {
    runs("probe_closure_param_assign", 2);
}

/// One cycle through two members is one E0404, at the first.
#[test]
fn a_cycle_through_two_members_is_one_e0404() {
    refused("probe_rec_two_members", "E0404", &["L -> L"]);
}

/// An alias is transparent and names no step; `distinct` is nominal and
/// does; `!T` holds its `T` inline; a generic that holds itself at its own
/// parameters is a cycle.
#[test]
fn what_holds_a_type_inline() {
    refused("probe_rec_alias_type", "E0404", &["N -> N"]);
    refused("probe_rec_distinct", "E0404", &["N -> M -> N"]);
    refused("probe_rec_errunion_ok", "E0404", &["N -> N"]);
    refused("probe_rec_generic_self", "E0404", &["L -> L"]);
}

/// A recursion that only grows its arguments never repeats one type, so the
/// walk stops and reports nothing (`[type.rec]`'s last sentence). This
/// machine stops at 32 inline levels or a 4096-character spelling, its own
/// bound; the compiler at `a57276f2` does not return on this program
/// (wolf-lang#1081), so the row holds this machine alone.
#[test]
fn a_recursion_that_grows_its_arguments_is_not_reported() {
    let (record, _) = observe("probe_rec_grows");
    let got = serde_json::to_value(&record).expect("the record serializes");
    assert_eq!(got["verdict"], "exit(0)", "{got}");
    assert_eq!(first_error(&got["diagnostics"]), None, "{got}");
}

/// `else |e| match e { exists => { exists = "exists"; 0 }, … }`: `e` holds
/// the handled call's error, so `exists` tests that row's tag and binds
/// nothing, and the arm writes the outer `var` (`[gram.pat.nullary.scope]`).
/// The shape of `tests/net_unix.rs`'s bind rows, which a reader that did not
/// see the handler's row refused as E0410.
#[test]
fn an_else_handlers_tag_arm_writes_the_outer_var() {
    runs("probe_else_tag_arm_writes_outer_var", 0);
}

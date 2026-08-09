//! Snapshots of the frontend's two dumps.
//!
//! Both formats are **ours** (`lex::dump`, `parse::trace`) and neither is
//! compared with anything: the protocol carries `{code, span, severity}` and
//! nothing else (`[proto.record.diag]`). These snapshots exist so that a change
//! in how the mode stack decomposes a literal, or in how a production nests, is
//! visible in review rather than discovered by is02.
//!
//! # Two families, on purpose
//!
//! `snapshot_exemplar.rs` warns against embedding corpus text: corpus bytes are
//! canonical `wolf fmt` output (STYLE_VERSION 1) and get rewritten wholesale on
//! a style bump, so a snapshot holding them churns for reasons unrelated to
//! this repo. That warning is honoured where it can be:
//!
//! - **fixtures** below are owned by this file. They cover every lexical mode
//!   and every terminator rule, and they only move when the *lexer* moves.
//! - **corpus snapshots** are the handful the sprint asks for. They do embed
//!   corpus-derived text and byte offsets, and they *will* churn on a
//!   STYLE_VERSION bump — accepted deliberately, because "the lexer agrees with
//!   the canonical program" is a claim worth a regression test. When they churn
//!   for that reason, review the diff for span shifts only and accept.

use std::path::{Path, PathBuf};

use wolf_interp::{lex, parse};

fn corpus(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn tokens_of(source: &str) -> String {
    lex::dump(&lex::lex(source))
}

fn trace_of(source: &str) -> String {
    let parsed = parse::parse_source(source).unwrap_or_else(|e| panic!("must parse: {e}"));
    parse::trace(&parsed.unit)
}

// ---------------------------------------------------------------------------
// Fixtures we own: churn-free, and each one is a rule from spec/01 §1.
// ---------------------------------------------------------------------------

/// Every string mode in one file: plain, interpolated, format-spec'd,
/// escape-braced, nested, multiline-dedented, raw-fenced, generalized.
const EVERY_STRING_MODE: &str = r##"fn modes() -> int {
    let plain = "abc"
    let interp = "a{b}c"
    let spec = "{n:>8}"
    let width = "{n:>{w}}"
    let braces = "{{literal}}"
    let nested = "{"inner-{deep}"}"
    let block = """
        one
        two
        """
    let raw = r#"no \escapes "here""#
    let generalized = re"[a-z]+"
    0
}
"##;

/// `[gram.lex.newline]` end to end: the token class, the innermost-delimiter
/// rule, the attribute exception, and `;`.
const TERMINATOR_RULES: &str = r#"#[repr(c)]
struct Point { x: int }

fn rules(
    a: int,
    b: int,
) -> int {
    let trailing = a +
        b
    let call = f(
        a,
        b,
    )
    let closure = g(fn() {
        let inside = a
        inside
    })
    if a == 1 { print("x"); return 0 }
    trailing
}
"#;

#[test]
fn every_string_mode_decomposes() {
    insta::assert_snapshot!(tokens_of(EVERY_STRING_MODE));
}

#[test]
fn the_terminator_rules_insert_exactly_where_the_spec_says() {
    insta::assert_snapshot!(tokens_of(TERMINATOR_RULES));
}

#[test]
fn the_terminator_fixture_also_parses() {
    insta::assert_snapshot!(trace_of(TERMINATOR_RULES));
}

// ---------------------------------------------------------------------------
// Representative corpus files. These embed corpus-derived text; see the header.
// ---------------------------------------------------------------------------

#[test]
fn corpus_tokens_hello() {
    insta::assert_snapshot!(tokens_of(&corpus("hello.lu")));
}

#[test]
fn corpus_tokens_interp_fmtcolon() {
    // `[gram.amb.fmtcolon]`'s accepted reading, as a token stream: the `:`
    // after `]` is top-level and opens the spec; the one inside `{…}` is not.
    insta::assert_snapshot!(tokens_of(&corpus("grammar/interp_fmtcolon.lu")));
}

#[test]
fn corpus_tokens_interp_nested() {
    insta::assert_snapshot!(tokens_of(&corpus("grammar/interp_nested.lu")));
}

#[test]
fn corpus_tokens_wordcount_multiline() {
    // The canonical program's `"""` literal, dedented by the closing column.
    let source = corpus("wordcount.lu");
    let dump = tokens_of(&source);
    let excerpt: String = dump
        .lines()
        .skip_while(|line| !line.contains("str-open multiline"))
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(excerpt);
}

#[test]
fn corpus_trace_closure_extent() {
    // `[gram.amb.closure]`: the expression body extends maximally and `,`
    // terminates it — visible in the trace as the closure's single child.
    insta::assert_snapshot!(trace_of(&corpus("grammar/closure_extent.lu")));
}

#[test]
fn corpus_trace_else_chain() {
    insta::assert_snapshot!(trace_of(&corpus("grammar/else_chain.lu")));
}

#[test]
fn corpus_trace_brackets_are_one_shape() {
    // `[gram.amb.brackets]`: `f[int](x)` and `m[k]` produce the same production.
    insta::assert_snapshot!(trace_of(&corpus("grammar/brackets_generic_call.lu")));
}

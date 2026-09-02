//! is35 — the escape's code: `[gram.lex.str.escape]` says **E0101**, and this
//! implementation said E0103 (wolf-lang#225).
//!
//! The clause has read "`STR_ESC` — `\n \t \r \\ \" \0 \x7f \u{1F43A}` — and
//! nothing else; any other `\` is **E0101** at the escape" since #198 landed
//! in v0.2.2, so the number was never this implementation's to pick. Two of
//! its picks — E0103 for an unknown escape, E0104 for a `\u` with no braces —
//! are numbers wolfc's catalog spends on the multiline's LAYOUT, so a bad
//! escape and a badly-shaped `"""` were the same record here. r06 found it
//! adding #215's witnesses; the escape's SPAN was already identical on both
//! machines, so only the number moved.
//!
//! The witness is `grammar/multiline_bad_escape.lu`, and its point is that
//! `MULTI_PART` reaches the same `STR_ESC` a plain `STR_PART` does: the rule
//! is the escape's, not the literal kind's. Every shape is covered here —
//! plain, multiline, raw's non-participation — with the span asserted by
//! slicing the source, because "at the escape" is half the claim.

use wolf_interp::diag;
use wolf_interp::lex;

/// The `(code, text-under-the-span)` of the sole lex error, or a panic
/// naming what came back instead.
fn sole_error(src: &str) -> (String, String) {
    let lexed = lex::lex(src);
    assert_eq!(
        lexed.errors.len(),
        1,
        "{src:?} wanted one lex error, got {:?}",
        lexed.errors
    );
    let e = &lexed.errors[0];
    (e.code.to_string(), src[e.span.start..e.span.end].to_owned())
}

#[test]
fn an_unknown_escape_is_e0101_at_the_escape() {
    // The `\q` of #225's report, in the two literal kinds that derive
    // `STR_ESC`. `E_UNEXPECTED_BYTE` and `E_BAD_ESCAPE` are the same number
    // by the clause; naming the escape's constant is what this file pins.
    assert_eq!(diag::E_BAD_ESCAPE, "E0101");

    let (code, at) = sole_error(r#"let s = "a\qb""#);
    assert_eq!(code, diag::E_BAD_ESCAPE);
    assert_eq!(at, r"\q", "the span is the escape, not the literal");

    let (code, at) = sole_error("let s = \"\"\"\n    a\\qb\n    \"\"\"");
    assert_eq!(
        code,
        diag::E_BAD_ESCAPE,
        "a multiline reaches the same rule"
    );
    assert_eq!(at, r"\q");
}

#[test]
fn a_malformed_x_or_u_escape_rides_the_same_code() {
    // `\x` short of its two hex digits, and `\u` with no braces — both are
    // "any other `\`" as far as `STR_ESC` derives, so both are E0101. The
    // `\u` half is the one that used to answer E0104.
    for (src, want) in [
        (r#"let s = "a\xZb""#, r"\x"),
        (r#"let s = "a\x4""#, r"\x4"),
        (r#"let s = "a\u41b""#, r"\u"),
    ] {
        let (code, at) = sole_error(src);
        assert_eq!(code, diag::E_BAD_ESCAPE, "{src}");
        assert_eq!(at, want, "{src}");
    }
}

#[test]
fn the_digit_bound_twins_are_unmoved() {
    // #198's two witnesses already agreed with wolfc at E0101 — that is why
    // 484 files never showed #225. They must still agree, and their spans
    // must still cover the whole escape.
    for (src, want) in [
        (r#"let s = "x\u{0000041}""#, r"\u{0000041}"),
        (r#"let s = "x\u{}""#, r"\u{}"),
        (r"let c = '\u{0000041}'", r"\u{0000041}"),
    ] {
        let (code, at) = sole_error(src);
        assert_eq!(code, diag::E_UNEXPECTED_BYTE, "{src}");
        assert_eq!(at, want, "{src}");
    }
}

#[test]
fn a_char_literal_keeps_its_own_malformed_code() {
    // E0110 is spec-pinned over the whole literal for a malformed `char`
    // shape, and the move of the string tier's number must not have reached
    // it: `'\q'` is E0110 at `'\q'`, not E0101 at `\q`.
    let (code, at) = sole_error(r"let c = '\q'");
    assert_eq!(code, diag::E_BAD_CHAR_LITERAL);
    assert_eq!(at, r"'\q'");
}

#[test]
fn a_raw_string_has_no_escapes_to_get_wrong() {
    // `RAW_TEXT` derives scalars and nothing else, so `\q` inside `r"…"` is
    // two characters of text and no diagnostic at all.
    let lexed = lex::lex(r#"let s = r"a\qb""#);
    assert!(lexed.errors.is_empty(), "{:?}", lexed.errors);
}

#[test]
fn e0103_and_e0104_are_unspoken_here() {
    // The whole point of #225: those two numbers mean the multiline's layout
    // rules in the catalog, and this implementation must not answer with
    // either until it implements them (wolf-interp#59). A regression that
    // reintroduces one is a silent collision, so it is asserted rather than
    // left to a reader of `diag.rs`.
    for (code, _, _) in diag::UNPINNED_CODES {
        assert_ne!(*code, "E0103");
        assert_ne!(*code, "E0104");
    }
    for src in [
        r#"let s = "a\qb""#,
        r#"let s = "a\u41b""#,
        r#"let s = "a\xZb""#,
    ] {
        let (code, _) = sole_error(src);
        assert!(code != "E0103" && code != "E0104", "{src} answered {code}");
    }
}

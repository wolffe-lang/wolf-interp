//! is36 — D74's string-layout codes, one rule per code, in this machine.
//!
//! wolf-lang#230 found the tangle: spec and catalog agreed on E0103 (the
//! opening `"""` is not the last thing on its line) and E0105 (a content
//! line's margin mixes tabs and spaces differently from the closing
//! delimiter's), and disagreed on E0104 — the spec spent it on "the closing
//! `"""` does not stand alone on its line", the catalog on "a content line
//! sits left of the margin" — with no code left over for the other's rule.
//! The ruling gives each rule one code:
//!
//! - **E0103** — a `"""` delimiter shares its line with text, BOTH sides.
//! - **E0104** — a content line sits left of the margin.
//! - **E0105** — margin tabs and spaces do not match, byte for byte.
//! - **E0102** — the bare `{` in a plain string, the unterminated family.
//! - **E0107** — a byte order mark anywhere but offset 0; at offset 0 it is
//!   stripped and is never a diagnostic.
//!
//! This implementation had none of the three layout rules: every multiline
//! layout fault came out as its own invented E0109 (wolf-interp#59), the BOM
//! was an invented E0105 (colliding with the margin code), and the bare `{`
//! was right by accident but reported at the wrong span, because `world"`
//! inside the runaway interpolation spells a *generalized* literal whose body
//! ate newlines it had no production for (`[gram.lex.str.gen]`).
//!
//! Every assertion below carries the SPAN, sliced back out of the source,
//! because "the right code" is half a claim. The five corpus witnesses
//! s136 landed measure the same rules end to end; these are the unit half,
//! which runs without a pin.

use wolf_interp::diag;
use wolf_interp::lex;

/// The `(code, text-under-the-span)` of the FIRST lex error — the one the
/// observation record carries — or a panic naming what came back instead.
fn first_error(src: &str) -> (String, String) {
    let lexed = lex::lex(src);
    let diag = lexed
        .first_error()
        .unwrap_or_else(|| panic!("{src:?} lexed clean, wanted a diagnostic"));
    (
        diag.code.to_owned(),
        src[diag.span.start..diag.span.end].to_owned(),
    )
}

fn lexes_clean(src: &str) {
    let lexed = lex::lex(src);
    assert!(
        lexed.first_error().is_none(),
        "{src:?} wanted no diagnostic, got {:?}",
        lexed.errors
    );
}

// ---------------------------------------------------------------------------
// E0103 — a `"""` delimiter shares its line with text. One rule, one code,
// both delimiters: the spec's closing sentence folds into it (D74).
// ---------------------------------------------------------------------------

#[test]
fn e0103_the_opening_delimiter_is_the_last_thing_on_its_line() {
    // The span is the text after the opener, to the end of that line: a line
    // that opens `"""oops` gives `oops` no column to be measured against.
    let (code, text) = first_error("let p = \"\"\"oops\n    a\n    \"\"\"\n");
    assert_eq!(code, diag::E_DELIMITER_SHARES_LINE);
    assert_eq!(code, "E0103");
    assert_eq!(text, "oops");
}

#[test]
fn e0103_the_closing_delimiter_is_the_first_thing_on_its_line() {
    // The closing side of the SAME rule, which is the fold D74 performed:
    // the closing delimiter's column IS the margin, so text before it leaves
    // nothing to strip. The span is the delimiter itself.
    let (code, text) = first_error("let p = \"\"\"\n    a\n    b\"\"\"\n");
    assert_eq!(code, diag::E_DELIMITER_SHARES_LINE);
    assert_eq!(text, "\"\"\"");
}

#[test]
fn e0103_a_one_line_multiline_shares_both_delimiters_and_reports_the_opening() {
    // `"""abc"""` breaks both halves at once. The record carries the first
    // diagnostic in source order, which is the opening one, and its span runs
    // to the end of the line — the closing delimiter included, because that
    // is what sits after the opener.
    let (code, text) = first_error("let p = \"\"\"abc\"\"\"\n");
    assert_eq!(code, diag::E_DELIMITER_SHARES_LINE);
    assert_eq!(text, "abc\"\"\"");
}

#[test]
fn trailing_whitespace_after_the_opener_is_hygiene_and_stays_content() {
    // "the opening `\"\"\"` must be the LAST THING on its line" is about
    // TEXT: whitespace after the opener is trailing-whitespace hygiene, not a
    // layout fault, and the bytes stay in the literal. The opening line is
    // exempt from the margin rule for the same reason it is exempt from
    // E0103 — it has no column — so it is exempt from the STRIP as well.
    lexes_clean("let p = \"\"\"   \n        a\n        \"\"\"\n");
}

// ---------------------------------------------------------------------------
// E0104 — a content line sits left of the margin (the catalog's meaning; the
// spec sentence amended to it at the ruling).
// ---------------------------------------------------------------------------

#[test]
fn e0104_a_short_content_line_spans_its_leading_whitespace() {
    let (code, text) = first_error("let p = \"\"\"\n        good\n      bad\n        \"\"\"\n");
    assert_eq!(code, diag::E_SHORT_MARGIN);
    assert_eq!(code, "E0104");
    assert_eq!(text, "      ", "the short line's leading whitespace");
}

#[test]
fn e0104_a_content_line_with_no_margin_at_all_spans_its_first_byte() {
    // "its first byte when there is none" — a zero-width span points at
    // nothing, so the report takes the byte the line begins with.
    let (code, text) = first_error("let p = \"\"\"\nbad\n        \"\"\"\n");
    assert_eq!(code, diag::E_SHORT_MARGIN);
    assert_eq!(text, "b");
}

#[test]
fn a_blank_line_has_nothing_to_strip_and_nothing_to_complain_about() {
    // Requiring a blank line to carry the indentation would make
    // trailing-whitespace hygiene a compile error.
    lexes_clean("let p = \"\"\"\n        a\n\n        b\n        \"\"\"\n");
}

#[test]
fn a_closing_delimiter_at_column_zero_sets_an_empty_margin() {
    // No margin, so no line can sit left of it and every line keeps its own
    // indentation.
    lexes_clean("let p = \"\"\"\n  a\n\"\"\"\n");
}

// ---------------------------------------------------------------------------
// E0105 — margin tabs and spaces do not match. Unchanged by D74; never
// implemented here, because this machine's E0105 was an invented BOM code.
// ---------------------------------------------------------------------------

#[test]
fn e0105_eight_tabs_against_eight_spaces_is_a_mismatch_not_a_short_margin() {
    // The widths agree on screen and the bytes do not: "the comparison is
    // byte-for-byte and never by visual width".
    let (code, text) = first_error("let p = \"\"\"\n\t\t\t\t\t\t\t\tbad\n        \"\"\"\n");
    assert_eq!(code, diag::E_MIXED_MARGIN);
    assert_eq!(code, "E0105");
    assert_eq!(text, "\t\t\t\t\t\t\t\t");
}

#[test]
fn e0105_catches_a_margin_that_is_long_enough_and_still_wrong() {
    // One tab and seven spaces is eight whitespace bytes — the length rule
    // passes and the byte rule does not.
    let (code, text) = first_error("let p = \"\"\"\n\t       ab\n        \"\"\"\n");
    assert_eq!(code, diag::E_MIXED_MARGIN);
    assert_eq!(text, "\t       ");
}

#[test]
fn a_margin_that_is_short_reads_e0104_even_when_its_bytes_also_differ() {
    // Order matters: "a margin SHORTER than the delimiter's is E0104's rule",
    // so the length test comes first and two tabs against eight spaces is the
    // short-margin code, not the mixed one.
    let (code, _) = first_error("let p = \"\"\"\n\t\tbad\n        \"\"\"\n");
    assert_eq!(code, diag::E_SHORT_MARGIN);
}

// ---------------------------------------------------------------------------
// E0102 — the bare `{` in a plain string. lupin was right about the family;
// the span was not, and the reason was a generalized literal.
// ---------------------------------------------------------------------------

#[test]
fn e0102_a_bare_brace_spans_the_string_to_the_end_of_its_line() {
    let (code, text) = first_error("let s = \"hello {world\"\nprint(s)\n");
    assert_eq!(code, diag::E_UNTERMINATED_STRING);
    assert_eq!(code, "E0102");
    assert_eq!(text, "\"hello {world\"");
}

#[test]
fn a_generalized_literals_body_does_not_cross_a_line() {
    // `[gram.lex.str.gen]`: `GEN_TEXT ::= (SCALAR - ('"' | NL))*`, and the
    // excluded `NL` is how a production says its literal may not span lines
    // (#215). This machine let one span lines, which is precisely how the
    // bare brace above ran away to the end of the file: `world"` spells a
    // generalized literal. The refusal is E0109, the meaning D74 leaves that
    // code.
    let (code, _) = first_error("let s = re\"[a-z]\nlet t = 1\n");
    assert_eq!(code, diag::E_UNTERMINATED_RAW);
    assert_eq!(code, "E0109");
}

#[test]
fn a_raw_literal_still_spans_lines() {
    // `RAW_TEXT ::= SCALAR*` does NOT exclude `NL`, so the neighbouring rule
    // is read off the production rather than assumed from the generalized
    // one.
    lexes_clean("let s = r\"a\nb\"\n");
}

// ---------------------------------------------------------------------------
// E0107 — the byte order mark, both halves of `[gram.lex.source]`.
// ---------------------------------------------------------------------------

#[test]
fn a_leading_byte_order_mark_is_stripped_and_never_a_diagnostic() {
    lexes_clean("\u{feff}fn main() -> !int { 0 }\n");
}

#[test]
fn a_byte_order_mark_anywhere_else_is_a_stray_character() {
    let (code, text) = first_error("let x = \u{feff}1\n");
    assert_eq!(code, diag::E_STRAY_CHARACTER);
    assert_eq!(code, "E0107");
    assert_eq!(text, "\u{feff}");
}

// ---------------------------------------------------------------------------
// The code table itself.
// ---------------------------------------------------------------------------

#[test]
fn the_three_layout_codes_are_no_longer_this_implementations_to_choose() {
    // D74 and s136's five witnesses pin E0102/E0103/E0104/E0105, so none of
    // them may be listed as an implementation invention — and the two numbers
    // this machine invented in their neighbourhood (E0105 for a BOM, E0109
    // for the dedent underrun) must be gone from the table.
    for (code, _, _) in diag::UNPINNED_CODES {
        assert!(
            !matches!(*code, "E0102" | "E0103" | "E0104" | "E0105"),
            "{code} is D74's, not this implementation's"
        );
    }
}

// ---------------------------------------------------------------------------
// is37 — wolf-interp#59's closing measurement, against the pinned witnesses.
// ---------------------------------------------------------------------------

/// The five corpus files s136 landed for D74, each with the code AND the span
/// `wolf conform-run --json` reported for it at pin `982f857` (wolf-lang
/// v0.2.4).
///
/// wolf-interp#59 asked for two things: the three layout conditions as three
/// rules carrying the catalog's codes, and **wolfc's spans**. The unit half
/// above proves the rules; this is the differential half, and it is the one
/// the issue's own table indicts — 0.1.23 answered its invented E0109 at
/// `[31,49]` where wolfc answered E0103 at `[31,35]`, "a code divergence AND
/// a span divergence, twice". Every row below was re-measured on BOTH sides
/// before it was written down; a pin bump that moves a witness moves this
/// table, which is the point.
#[test]
fn the_five_witnesses_answer_wolfc_code_and_span() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus/grammar");
    let witnesses: &[(&str, &str, [usize; 2], &str)] = &[
        // The opening `"""` shares its line with text.
        ("multiline_open_shares_line.lu", "E0103", [437, 441], "oops"),
        // The closing one does — D74 folds it into E0103, one rule per code.
        (
            "multiline_close_shares_line.lu",
            "E0103",
            [598, 601],
            "\"\"\"",
        ),
        // A content line sits left of the margin (the catalog's E0104).
        ("multiline_short_margin.lu", "E0104", [520, 526], "      "),
        // The margin mixes tabs and spaces: eight tabs against eight spaces,
        // compared byte for byte and never by visual width.
        (
            "multiline_mixed_margin.lu",
            "E0105",
            [575, 583],
            "\t\t\t\t\t\t\t\t",
        ),
        // And the escape that freed E0103/E0104 in the first place (#225).
        ("multiline_bad_escape.lu", "E0101", [1566, 1568], "\\q"),
    ];
    for (file, code, span, text) in witnesses {
        let source = std::fs::read_to_string(root.join(file)).expect("witness readable");
        let lexed = lex::lex(&source);
        let diag = lexed
            .first_error()
            .unwrap_or_else(|| panic!("{file} lexed clean, wanted {code}"));
        assert_eq!(
            (diag.code, [diag.span.start, diag.span.end]),
            (*code, *span),
            "{file}: the counterparty's code and span are the contract"
        );
        assert_eq!(&&source[diag.span.start..diag.span.end], text, "{file}");
    }
}

/// The BOM's two positions, as the corpus and the catalog split them.
///
/// `grammar/bom_at_start.lu` is the only corpus file whose first three bytes
/// are `EF BB BF`; it is a `run(exit=0)` file on every lane, because a leading
/// mark is stripped and never a diagnostic. Mid-file the same bytes are a
/// stray character, and no corpus row can pin that alone — the stray token the
/// lexer leaves behind is also the parser's E0201, which the file's own
/// directive block says. So the mid-file case is asserted here, and the
/// leading case is asserted against the pinned file.
#[test]
fn the_pinned_bom_file_lexes_clean_and_the_mid_file_mark_does_not() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus/grammar/bom_at_start.lu");
    let source = std::fs::read_to_string(&file).expect("witness readable");
    assert!(
        source.starts_with('\u{feff}'),
        "the witness must actually begin with the mark"
    );
    lexes_clean(&source);

    let (code, text) = first_error(&source.replacen("fn main", "\u{feff}fn main", 1));
    assert_eq!(code, "E0107");
    assert_eq!(text, "\u{feff}");
}

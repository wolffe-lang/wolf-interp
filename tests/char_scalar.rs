//! is26 — the scalar arrives: `char` per `[gram.lex.char]` and the
//! `[type.char]` family (s121, D58), reimplemented from the spec.
//!
//! The lexical tier first: `'a'` with the string escape set plus `\'`, and
//! the named refusals — E0110, spec-pinned, one report per malformed
//! literal. The surrogate gap is refused *at the literal*: the value a
//! `char` cannot hold is the value its literal cannot spell, which is the
//! lex-time twin of the trapping cast this file's eval tier pins further
//! down.

use wolf_interp::diag;
use wolf_interp::lex::{self, Tok};

/// The `Tok::Char` values a source lexes to, in order.
fn chars_of(src: &str) -> Vec<char> {
    let lexed = lex::lex(src);
    assert!(
        lexed.errors.is_empty(),
        "expected a clean lex of {src:?}, got {:?}",
        lexed.errors
    );
    lexed
        .tokens
        .iter()
        .filter_map(|t| match t.tok {
            Tok::Char(c) => Some(c),
            _ => None,
        })
        .collect()
}

/// A malformed literal: every diagnostic is E0110 at `[gram.lex.char]`, and
/// the token stream carries an `Error` token where the literal stood.
fn refused(src: &str) -> usize {
    let lexed = lex::lex(src);
    assert!(
        !lexed.errors.is_empty(),
        "expected {src:?} to be refused, and it lexed clean"
    );
    for e in &lexed.errors {
        assert_eq!(e.code, diag::E_BAD_CHAR_LITERAL, "{src:?}: {e}");
        assert_eq!(e.anchor, "gram.lex.char", "{src:?}: {e}");
    }
    assert!(
        lexed.tokens.iter().any(|t| matches!(t.tok, Tok::Error)),
        "{src:?}: a refused char literal still leaves an Error token"
    );
    lexed.errors.len()
}

// -- the literal, at every UTF-8 width ----------------------------------

#[test]
fn a_char_literal_lexes_at_every_width() {
    assert_eq!(chars_of("'a'"), vec!['a']); // 1 byte
    assert_eq!(chars_of("'é'"), vec!['é']); // 2 bytes
    assert_eq!(chars_of("'中'"), vec!['中']); // 3 bytes
    assert_eq!(chars_of("'🐺'"), vec!['🐺']); // 4 bytes
}

#[test]
fn the_escape_set_is_the_string_set_plus_the_quote() {
    // `\n \t \r \\ \' \" \0 \xNN \u{1–6 hex}` ([gram.lex.char]).
    assert_eq!(chars_of(r"'\n'"), vec!['\n']);
    assert_eq!(chars_of(r"'\t'"), vec!['\t']);
    assert_eq!(chars_of(r"'\r'"), vec!['\r']);
    assert_eq!(chars_of(r"'\\'"), vec!['\\']);
    assert_eq!(chars_of(r"'\''"), vec!['\'']);
    assert_eq!(chars_of(r#"'\"'"#), vec!['"']);
    assert_eq!(chars_of(r"'\0'"), vec!['\0']);
    assert_eq!(chars_of(r"'\x61'"), vec!['a']);
    assert_eq!(chars_of(r"'\u{E9}'"), vec!['é']);
    assert_eq!(chars_of(r"'\u{1F43A}'"), vec!['🐺']);
}

#[test]
fn distinct_spellings_of_one_scalar_are_one_token_value() {
    // `[type.char.lit]`: `'\n'` equals `'\u{A}'` — decoded at the lexer, so
    // the parser and evaluator never see the spelling.
    assert_eq!(chars_of(r"'\n'"), chars_of(r"'\u{A}'"));
    assert_eq!(chars_of("'a'"), chars_of(r"'\x61'"));
    assert_eq!(chars_of("'é'"), chars_of(r"'\u{E9}'"));
}

#[test]
fn the_domain_edges_are_spellable() {
    // The gap's neighbours and the last scalar are LEGAL ([type.char.lit] —
    // the same edges the cast witnesses pin): the proof the gap has the
    // right shape.
    assert_eq!(chars_of(r"'\u{D7FF}'"), vec!['\u{D7FF}']);
    assert_eq!(chars_of(r"'\u{E000}'"), vec!['\u{E000}']);
    assert_eq!(chars_of(r"'\u{10FFFF}'"), vec!['\u{10FFFF}']);
    assert_eq!(chars_of(r"'\u{0}'"), vec!['\0']);
}

#[test]
fn a_char_literal_ends_its_lines_statement() {
    // `[gram.lex.newline]`: CHAR_LIT joined the literal bullet, so a
    // terminator is inserted after it.
    let lexed = lex::lex("let c = 'a'\nlet d = 'b'\n");
    let mut saw_term_after_char = false;
    for pair in lexed.tokens.windows(2) {
        if matches!(pair[0].tok, Tok::Char('a'))
            && matches!(pair[1].tok, Tok::Term { explicit: false })
        {
            saw_term_after_char = true;
        }
    }
    assert!(saw_term_after_char, "{:?}", lexed.tokens);
}

// -- the named refusals: E0110, one report each -------------------------

#[test]
fn the_empty_literal_is_refused() {
    assert_eq!(refused("let c = ''"), 1);
}

#[test]
fn a_multi_scalar_literal_is_refused() {
    assert_eq!(refused("let c = 'ab'"), 1);
    // A base plus a combining accent renders as one glyph and is still two
    // scalars: a `char` is a scalar, not a grapheme ([gram.lex.char]).
    assert_eq!(refused("let c = 'e\u{301}'"), 1);
    assert_eq!(refused(r"let c = 'e\u{301}'"), 1);
}

#[test]
fn an_unterminated_literal_is_refused_at_its_line() {
    assert_eq!(refused("let c = 'a"), 1);
    assert_eq!(refused("let c = 'a\nlet d = 1\n"), 1);
    assert_eq!(refused("let c = '"), 1);
}

#[test]
fn a_non_scalar_escape_is_refused_at_the_literal() {
    // The surrogate gap, both ends, and past the last scalar: the lex-time
    // twin of the trapping cast ([gram.lex.char] / [type.char.cast]).
    assert_eq!(refused(r"let c = '\u{D800}'"), 1);
    assert_eq!(refused(r"let c = '\u{DFFF}'"), 1);
    assert_eq!(refused(r"let c = '\u{110000}'"), 1);
}

#[test]
fn malformed_escapes_are_refused() {
    assert_eq!(refused(r"let c = '\q'"), 1);
    assert_eq!(refused(r"let c = '\u{}'"), 1);
    assert_eq!(refused(r"let c = '\u{0000041}'"), 1); // seven digits
    assert_eq!(refused(r"let c = '\u41'"), 1); // unbraced
    assert_eq!(refused(r"let c = '\x6'"), 1); // one hex digit
}

#[test]
fn one_malformed_literal_files_one_report() {
    // Several things are wrong inside one literal; the first report wins
    // ([gram.lex.char]: "one report each").
    assert_eq!(refused(r"let c = '\q\p'"), 1);
    assert_eq!(refused(r"let c = 'ab\q'"), 1);
}

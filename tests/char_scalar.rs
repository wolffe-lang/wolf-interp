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
use wolf_interp::frontend;
use wolf_interp::lex::{self, Tok};
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

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

/// A malformed literal refused under `code` at `[gram.lex.char]`, with an
/// `Error` token left where the literal stood.
fn refused_as(code: &str, src: &str) -> usize {
    let lexed = lex::lex(src);
    assert!(
        !lexed.errors.is_empty(),
        "expected {src:?} to be refused, and it lexed clean"
    );
    for e in &lexed.errors {
        assert_eq!(e.code, code, "{src:?}: {e}");
        assert_eq!(e.anchor, "gram.lex.char", "{src:?}: {e}");
    }
    assert!(
        lexed.tokens.iter().any(|t| matches!(t.tok, Tok::Error)),
        "{src:?}: a refused char literal still leaves an Error token"
    );
    lexed.errors.len()
}

/// A malformed SHAPE: E0110, the char literal's own code.
fn refused(src: &str) -> usize {
    refused_as(diag::E_BAD_CHAR_LITERAL, src)
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
    assert_eq!(refused(r"let c = '\u41'"), 1); // unbraced
    assert_eq!(refused(r"let c = '\x6'"), 1); // one hex digit
}

#[test]
fn the_unicode_escape_s_digit_count_is_e0101_at_the_escape() {
    // `[gram.lex.char]`, amended at wolf-lang#189 (r04): the one-to-six
    // bound is the ESCAPE's shape, not the `char`'s value, so it carries the
    // escape set's code and the escape's span rather than the char literal's
    // E0110 over the whole literal. `'\u{0000041}'` is refused before
    // anything asks that `0x0000041` names `'A'` — leading zeros count.
    assert_eq!(refused_as(diag::E_UNEXPECTED_BYTE, r"let c = '\u{}'"), 1);
    assert_eq!(
        refused_as(diag::E_UNEXPECTED_BYTE, r"let c = '\u{0000041}'"),
        1
    );

    // "at the escape": the span covers `\u{…}`, not the quotes around it.
    let src = r"let c = '\u{0000041}'";
    let lexed = lex::lex(src);
    let span = lexed.errors[0].span;
    assert_eq!(&src[span.start..span.end], r"\u{0000041}");

    // The bound "binds in string literals too" — same rule, same code, and
    // the escape is judged before the value question, so the seven-digit
    // spelling of `A` is refused rather than quietly decoded.
    for src in [r#"let s = "x\u{0000041}""#, r#"let s = "x\u{}""#] {
        let lexed = lex::lex(src);
        assert_eq!(lexed.errors.len(), 1, "{src}");
        assert_eq!(lexed.errors[0].code, diag::E_UNEXPECTED_BYTE, "{src}");
        assert_eq!(lexed.errors[0].anchor, "gram.lex.str", "{src}");
    }

    // Six digits is in bounds, in both literal forms.
    assert_eq!(chars_of(r"'\u{000041}'"), vec!['A']);
    assert!(lex::lex(r#"let s = "x\u{000041}""#).errors.is_empty());
}

#[test]
fn one_malformed_literal_files_one_report() {
    // Several things are wrong inside one literal; the first report wins
    // ([gram.lex.char]: "one report each").
    assert_eq!(refused(r"let c = '\q\p'"), 1);
    assert_eq!(refused(r"let c = 'ab\q'"), 1);
}

// -- the parse tier ------------------------------------------------------

#[test]
fn char_literals_parse_in_expression_and_pattern_position() {
    // `literal ::= INT | FLOAT | CHAR_LIT | …` ([gram.expr.primary]), and a
    // pattern literal ([gram.pat]).
    let src = "fn main() -> int {\n    let c = '🐺'\n    match c {\n        'a' => 1,\n        _ => 0,\n    }\n}\n";
    let parsed = wolf_interp::parse::parse_source(src).expect("parses");
    let trace = wolf_interp::parse::trace(&parsed.unit);
    // Expression position shows in the parse trace; pattern position is
    // proven by the parse succeeding at all (a pattern that failed to admit
    // CHAR_LIT would be a parse error on the arm).
    assert!(trace.contains("char '🐺'"), "{trace}");
    assert!(trace.contains("match (2 arm(s))"), "{trace}");
}

// -- the eval tier -------------------------------------------------------

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

fn refuses(body: &str) {
    let observation = observe(body);
    assert_eq!(
        observation.verdict,
        Verdict::Unsupported,
        "body: {body}\nreason: {:?}",
        observation.reason
    );
}

/// A program the DECLARED-SCALAR pass refuses before it runs — the answer
/// wolfc gives, at the rung wolfc gives it (`sema::scalar_check`, is47 /
/// wolf-interp#61). Stronger than [`refuses`]: an `unsupported` says "this
/// machine did not implement that", a `fail(E0401)` says "this program is
/// wrong", and only the second is a verdict the corpus can pin.
fn refuses_statically(body: &str) {
    let observation = observe(body);
    assert_eq!(
        observation.verdict,
        Verdict::Fail("E0401".to_owned()),
        "body: {body}\nreason: {:?}",
        observation.reason
    );
    assert_eq!(observation.phase_reached, Phase::Resolve, "body: {body}");
}

#[test]
fn char_orders_by_scalar_value() {
    // [type.char.order]: total, locale-free, and honestly NOT collation —
    // 'z' < 'é' because 0x7A < 0xE9, however a dictionary would sort them.
    exits_zero(
        "if 'a' < 'b' && 'A' < 'a' && '0' < 'A' && 'z' < 'é' && 'é' < '中' && '中' < '🐺' \
         { 0 } else { 1 }",
    );
    exits_zero("if 'a' <= 'a' && 'a' >= 'a' && !('a' < 'a') { 0 } else { 1 }");
}

#[test]
fn char_equality_is_scalar_equality() {
    // Distinct spellings collapse; distinct scalars stay distinct — a
    // precomposed é is NOT the letter e ([type.char.order]).
    exits_zero(
        "if 'é' == 'é' && '\\n' == '\\u{A}' && '\\'' == '\\x27' && 'é' != 'e' { 0 } else { 1 }",
    );
}

#[test]
fn char_is_not_an_integer() {
    // [type.char]: no arithmetic, no numeric-literal adoption, no mixed
    // comparison. Every one of these is a program the compiler rejects; a
    // refusal beats running it (the permissive divergence).
    //
    // is47 (wolf-interp#61) moved four of the six from the dynamic channel to
    // the static one. `unsupported` was the honest answer while nothing read
    // the declared type — but it is the "not implemented yet" channel, and
    // these are defects in the reader's program, which is a different
    // sentence. `sema::scalar_check` reads `char` beside `byte` now and the
    // four answer E0401 at resolve, where wolfc answers.
    refuses_statically("if 'a' == 97 { 0 } else { 1 }");
    refuses_statically("if 'a' < 97 { 0 } else { 1 }");
    refuses_statically("let c: char = 65\n0");
    refuses_statically("let n: int = 'a'\n0");

    // The two that stay dynamic, and deliberately: `[type.byte.op]`'s
    // widening rule is about `byte`, and this pass says nothing about what a
    // `+` over two `char`s produces rather than guessing a rule `[type.char]`
    // does not state. The evaluator still refuses them by name.
    refuses("let x = 'a' + 'b'\nx as int");
    refuses("let x = 'a' + 1\nx");
}

/// wolf-interp#61's three programs and the five rows the one rule brings with
/// them, each measured against `wolf 0.2.12 (wolfgang) conform-run --json
/// --checked` at pin `a7f517e` — the code AND the span, because "the same
/// answer" is a claim about bytes.
///
/// All eight RAN on lupin 0.1.34. The first three are the issue's own, filed
/// at is36 and unpaid through seven releases because is37's lattice was
/// `byte`-only by construction; the third is the one that mattered most,
/// because it did not merely accept a program the compiler rejects — the
/// assignment silently RETYPED a live variable, so every later `{c}` printed
/// an int's rendering of whatever the arithmetic produced. The last five are
/// what "one rule, not a third sibling guard" buys: a `char` in an `int`
/// slot, the two width-bearing scalars against each other in BOTH directions,
/// a `List[char]` element, and a mixed comparison — none of them measured
/// before this lane, all of them E0401 on the counterparty.
const WOLFC_ROWS: &[(&str, &str, [usize; 2])] = &[
    (
        "a parameter declared `char`",
        "fn widen(c: char) -> int { c as int }\n\nfn main() -> !int {\n    print(\"{widen(65)}\")\n    0\n}\n",
        [77, 79],
    ),
    (
        "a return type declared `char`",
        "fn givec() -> char { 65 }\n\nfn main() -> !int {\n    print(\"{givec()}\")\n    0\n}\n",
        [21, 23],
    ),
    (
        "an assignment place declared `char`",
        "fn main() -> !int {\n    var c = 'a'\n    c = 65\n    print(\"{c}\")\n    0\n}\n",
        [44, 46],
    ),
    (
        "a `char` in an `int` slot",
        "fn f(n: int) -> int { n }\n\nfn main() -> !int { f('a') }\n",
        [49, 52],
    ),
    (
        "a `char` in a `byte` slot",
        "fn f(b: byte) -> int { b as int }\n\nfn main() -> !int { f('a') }\n",
        [57, 60],
    ),
    (
        "a `byte` in a `char` slot",
        "fn f(c: char) -> int { c as int }\n\nfn main() -> !int { f(65 as byte) }\n",
        [57, 67],
    ),
    (
        "a `List[char]` element",
        "fn main() -> !int {\n    var xs = List[char]()\n    (mut xs).push(65)\n    xs.len\n}\n",
        [64, 66],
    ),
    (
        "a mixed comparison, spanning the `int` side",
        "fn main() -> !int {\n    let c = 'a'\n    if c == 97 { 0 } else { 1 }\n}\n",
        [48, 50],
    ),
];

#[test]
fn the_declared_char_boundaries_answer_wolfc_span_for_span() {
    for (what, source, span) in WOLFC_ROWS {
        let observed = frontend::observe(source.as_bytes(), Some(Phase::Resolve));
        let diag = observed
            .detail
            .clone()
            .unwrap_or_else(|| panic!("{what} was not refused: {:?}", observed.verdict));
        assert_eq!(
            (diag.code, [diag.span.start, diag.span.end]),
            ("E0401", *span),
            "{what}: the counterparty's code and span are the contract"
        );
        assert_eq!(observed.verdict, Verdict::Fail("E0401".to_owned()), "{what}");
        assert_eq!(observed.phase_reached, Phase::Resolve, "{what}");
    }
}

#[test]
fn the_char_half_of_the_rule_is_the_byte_half() {
    // is37's two guards did not gain a third sibling: one lattice answers
    // every boundary it can see, for both width-bearing scalars and in both
    // directions. `byte`'s own rows are unmoved — `tests/byte_domain.rs` and
    // `tests/byte_scalar.rs` are the proof, and they run unchanged.
    //
    // The cast is the spelling, in both directions, on the same programs:
    // nothing here refuses a program that says what it means.
    exits_zero("let c: char = 65 as char\nlet n: int = 'a' as int\nif n == 97 { 0 } else { 1 }");
    exits_zero("var c = 'a'\nc = 65 as char\nif c == 'A' { 0 } else { 1 }");
    exits_zero("var xs = List[char]()\n(mut xs).push(65 as char)\nif xs[0] == 'A' { 0 } else { 1 }");

    // And the sema boundary holds: a side this pass cannot see says nothing.
    // `List()` carries no element annotation and a generic parameter resolves
    // somewhere this walk does not go, so neither is judged.
    exits_zero("var xs = List()\n(mut xs).push(65)\nif xs[0] == 65 { 0 } else { 1 }");
}

#[test]
fn char_as_int_is_total() {
    // [type.char.cast], the total direction: every scalar names its code
    // point.
    exits_zero(
        "if 'a' as int == 97 && 'é' as int == 233 && '中' as int == 20013 \
         && '🐺' as int == 128058 { 0 } else { 1 }",
    );
}

#[test]
fn int_as_char_converts_every_scalar_and_round_trips() {
    exits_zero(
        "if 97 as char == 'a' && 233 as char == 'é' && 20013 as char == '中' \
         && 128058 as char == '🐺' { 0 } else { 1 }",
    );
    // The domain's legal edges: zero, the gap's NEIGHBOURS, the last scalar
    // — the proof the gap has the right shape ([type.char.cast]).
    exits_zero("if 0 as char == '\\0' { 0 } else { 1 }");
    exits_zero("if 55295 as char == '\\u{D7FF}' { 0 } else { 1 }");
    exits_zero("if 57344 as char == '\\u{E000}' { 0 } else { 1 }");
    exits_zero("if 1114111 as char == '\\u{10FFFF}' { 0 } else { 1 }");
}

#[test]
fn int_as_char_traps_on_a_non_scalar() {
    // The boundary quartet's trapping half, plus the far gap edge: the
    // surrogate gap at both ends, past the last scalar, and negative — all
    // trap(overflow), D56's family ([type.char.cast]).
    traps_overflow("let n = 0xD800\nlet c = n as char\nif c == 'x' { 1 } else { 2 }");
    traps_overflow("let n = 0xDFFF\nlet c = n as char\nif c == 'x' { 1 } else { 2 }");
    traps_overflow("let n = 0x110000\nlet c = n as char\nif c == 'x' { 1 } else { 2 }");
    traps_overflow("let n = 0 - 1\nlet c = n as char\nif c == 'x' { 1 } else { 2 }");
}

#[test]
fn the_char_cast_bridges_nothing_else() {
    // Other widths cast through `int` ([type.char.cast]); floats and
    // strings never bridge. Refusals, by name.
    refuses("let x = 'a' as f64\n0");
    refuses("let x = 'a' as u32\n0");
    refuses("let x = \"a\" as char\n0");
    refuses("let x = 3.0 as char\n0");
    // `bool` casts to nothing, and sema-lite can SEE this one statically —
    // the E0805 the counterparty answers, code for code.
    let observation = observe("let x = true as char\n0");
    assert_eq!(observation.verdict, Verdict::Fail("E0805".to_owned()));
}

#[test]
fn chars_yields_the_scalars_in_string_order() {
    // [mem.str.chars]: List[char], one element per code point, at every
    // UTF-8 width; the empty string yields nothing; ASCII is one char per
    // byte while `s.len` stays the BYTE count.
    exits_zero(
        "let cs = \"aé中🐺\".chars()\n\
         if cs.len == 4 && cs[0] == 'a' && cs[1] == 'é' && cs[2] == '中' && cs[3] == '🐺' \
         { 0 } else { 1 }",
    );
    exits_zero("if \"\".chars().len == 0 { 0 } else { 1 }");
    exits_zero(
        "let ascii = \"wolf\".chars()\n\
         if ascii.len == 4 && ascii[0] == 'w' && ascii[3] == 'f' && \"wolf\".len == 4 \
         { 0 } else { 1 }",
    );
    exits_zero("if \"aé中🐺\".chars().len == 4 && \"aé中🐺\".len == 10 { 0 } else { 1 }");
}

#[test]
fn the_width_identity_holds() {
    // THE reason the primitive exists ([mem.str.chars]): a scalar's byte
    // extent is a function of its value — 1 below 0x80, 2 below 0x800, 3
    // below 0x10000, else 4 — and a cursor advanced that way lands on
    // exactly the boundaries `get` accepts, ending at the byte length.
    // Witnessed, not assumed: every `get` at a computed offset must succeed.
    let src = r#"fn width(c: char) -> int {
    let n = c as int
    if n < 128 { 1 } else if n < 2048 { 2 } else if n < 65536 { 3 } else { 4 }
}

fn main() -> !int {
    let s = "aé中🐺"
    let cs = s.chars()
    var off = 0
    var hits = 0
    for c in cs {
        let w = width(c)
        let piece = s.get(off..off + w) else "?"
        if piece != "?" {
            hits = hits + 1
        }
        off = off + w
    }
    if off == s.len && hits == 4 && s.len == 10 { 0 } else { 1 }
}
"#;
    let observation = frontend::observe(src.as_bytes(), None);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
}

#[test]
fn a_char_prints_the_character() {
    // [type.char.interp]: `{c}` renders the scalar's UTF-8 encoding, in
    // interpolation and in `print` alike; the number is spelled, not
    // ambient. A spec on a char hole takes the str surface — width in
    // BYTES (D25), so `é` at `<4` pads with two spaces.
    let src = "fn main() -> !int {\n    let w = '🐺'\n    let e = 'é'\n    print(\"wolf: {w}{e}\")\n    print(\"{w as int}/{w}\")\n    print(\"[{e:<4}]\")\n    0\n}\n";
    let observation = frontend::observe(src.as_bytes(), None);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
    assert_eq!(
        String::from_utf8_lossy(&observation.stdout),
        "wolf: 🐺é\n128058/🐺\n[é  ]\n"
    );
}

#[test]
fn a_numeric_spec_on_a_char_hole_is_refused() {
    // [type.char.interp]: `{c:x}` is the E0413 mismatch it looks like —
    // refused by name, never silently ignored.
    refuses("let c = 'a'\nlet s = \"{c:x}\"\n0");
}

#[test]
fn char_dispatches_through_match_by_scalar_identity() {
    exits_zero(
        "let c = '\\u{A}'\n\
         let k = match c {\n    'a' | 'e' => 1,\n    '\\n' => 2,\n    '🐺' => 3,\n    _ => 0,\n}\n\
         if k == 2 { 0 } else { 1 }",
    );
}

#[test]
fn char_assignment_copies_exactly_as_int_does() {
    // wolf-interp#50 (is28): `char` is a copy value in the tier-0 move
    // discipline exactly as `int` is (D58: a scalar, i32-shaped at every
    // tier). Measured at 0.1.16, `d = c` MOVED `c` and the re-read trapped
    // use-after-move while the compiler printed `xx` — the human's live
    // reproducer, pinned here read-after-assign on both spellings.
    let observation = observe(
        "let c = \"x\".chars()[0]\n    var d = 0 as char\n    d = c\n    print(\"{c}{d}\")\n    0",
    );
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}\ntrap: {:?}",
        observation.reason,
        observation.trap
    );
    assert_eq!(observation.stdout, b"xx\n");

    // The int control the issue names: ints copy; char now matches.
    let observation = observe("let c = 7\n    var d = 0\n    d = c\n    print(\"{c}{d}\")\n    0");
    assert_eq!(observation.verdict, Verdict::Exit(0));
    assert_eq!(observation.stdout, b"77\n");

    // A `let`-initializer read is the same discipline: no move, no trap.
    let observation = observe("let c = 'w'\n    let d = c\n    if c == d { 0 } else { 1 }");
    assert_eq!(observation.verdict, Verdict::Exit(0));
}

#[test]
fn for_over_a_str_stays_a_named_refusal() {
    // Named-not-built on both sides (the s17 iteration-protocol question);
    // a refusal beats an approximation.
    refuses("for c in \"str\" {\n    print(\"{c}\")\n}\n0");
}

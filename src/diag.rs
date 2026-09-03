//! Spans and diagnostics.
//!
//! The protocol only ever sees `{code, span, severity}` (`[proto.record.diag]`,
//! D22) — messages are a per-implementation quality concern and are never
//! compared. Internally we still carry a message, because a human running
//! `wolf-interp parse` deserves one; it simply never reaches the wire.
//!
//! # Code families
//!
//! Per the sprint's revised families: `E000x` are the reservations spelled out
//! in `spec/01-grammar.md` §9, `E01xx` is the lexer, `E02xx` the parser. Only
//! three of those are fixed by a document we do not own:
//!
//! - the eight `E000x` reservations (spec/01 §9),
//! - `E0108`, named in `[gram.lex.str]` as the depth-32 lexer rail, and
//! - `E0110`, named in `[gram.lex.char]` (s121, D58) for malformed char
//!   literals.
//!
//! Everything else in `E01xx`/`E02xx` is **this implementation's choice**. The
//! choices are listed in [`UNPINNED_CODES`] so a divergence report can say
//! "wolf-interp picked E0102 here" without archaeology, and so the eventual
//! s10 catalog has a concrete counter-proposal to argue with.

use std::fmt;

use crate::protocol::Diagnostic;

/// A byte-offset half-open span `[start, end)` (D25, `[gram.lex.source]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    /// A zero-width span at `at` — used when the offending thing is an
    /// *absence* (end of input where a token was required).
    #[must_use]
    pub const fn empty(at: usize) -> Span {
        Span { start: at, end: at }
    }

    #[must_use]
    pub const fn join(self, other: Span) -> Span {
        Span {
            start: if self.start < other.start {
                self.start
            } else {
                other.start
            },
            end: if self.end > other.end {
                self.end
            } else {
                other.end
            },
        }
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.end <= self.start
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// 1-based `(line, column)` of a byte offset, columns counted in
/// **characters**.
///
/// This is the repo's one line:col spelling — the fault snapshots pinned it
/// first (`tests/fault_snapshots.rs`), and `[conf.trap.render]` makes it the
/// human rendering of runtime faults: one span spelling per tool, with raw
/// byte offsets staying available through `--json`
/// (`[proto.record.diag]` / `[proto.record.ext]`). Offsets past the end of
/// the source clamp to it, so a zero-width EOF span renders as the position
/// just past the last character.
#[must_use]
pub fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut at = offset.min(source.len());
    // A clamped or mid-code-point offset backs up to the nearest boundary
    // rather than panicking: rendering never gets to crash the report.
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let before = &source[..at];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before, |(_, tail)| tail)
        .chars()
        .count()
        + 1;
    (line, column)
}

impl Span {
    /// The span's start spelled `line:col` (see [`line_col`]) — the location
    /// grammar of every human diagnostic line rendered with a source in hand.
    #[must_use]
    pub fn position(self, source: &str) -> String {
        let (line, column) = line_col(source, self.start);
        format!("{line}:{column}")
    }
}

/// A teach-note: what to WRITE, and where it goes.
///
/// The counterparty's diagnostics grew these in s131/s132 as machine-applicable
/// suggestions (`wolf fix --apply` round-trips them). This implementation does
/// not rewrite source, so the note is not machine-applicable here — it is the
/// same fact rendered for a human: a zero-width insertion point and the text
/// that belongs at it. Like `message`, it is a quality concern and never
/// reaches the wire (D22, `[proto.record.diag]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Help {
    /// Where the insertion goes — zero-width by construction for an
    /// insertion suggestion.
    pub span: Span,
    pub note: String,
}

impl Help {
    /// An insertion at `at`: the span is zero-width there, per
    /// [`Span::empty`].
    pub fn insert(at: usize, note: impl Into<String>) -> Help {
        Help {
            span: Span::empty(at),
            note: note.into(),
        }
    }
}

/// One diagnostic. `code` + `span` are protocol; `message`, `anchor` and
/// `help` are ours and stay local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag {
    pub code: &'static str,
    pub span: Span,
    /// The `[gram.…]` clause whose production failed. Every parser rule cites
    /// one, which is what makes is02's "every rule cites a clause" cheap.
    pub anchor: &'static str,
    pub message: String,
    /// The fix, when the refusal knows one (wolf-interp#56). Rendered as a
    /// second line; absent from `to_protocol`, like the message.
    pub help: Option<Help>,
}

impl Diag {
    pub fn new(
        code: &'static str,
        span: Span,
        anchor: &'static str,
        message: impl Into<String>,
    ) -> Diag {
        Diag {
            code,
            span,
            anchor,
            message: message.into(),
            help: None,
        }
    }

    /// Attaches the fix suggestion (wolf-interp#56).
    #[must_use]
    pub fn with_help(mut self, help: Help) -> Diag {
        self.help = Some(help);
        self
    }

    /// Projects onto the protocol shape: code, span, severity — nothing else.
    #[must_use]
    pub fn to_protocol(&self) -> Diagnostic {
        Diagnostic {
            code: self.code.to_owned(),
            span: [self.span.start as u64, self.span.end as u64],
            severity: "error".to_owned(),
        }
    }

    /// The human line for a caller holding the source: the [`fmt::Display`]
    /// grammar with the location spelled `line:col` (`[conf.trap.render]`'s
    /// one-spelling rule; the REPL, whose offsets are entry-relative, keeps
    /// the offset spelling deliberately — see `docs/repl.md`). The protocol
    /// projection keeps byte offsets untouched.
    #[must_use]
    pub fn render(&self, source: &str) -> String {
        let head = format!(
            "{}: {} [{}] at {}",
            self.code,
            self.message,
            self.anchor,
            self.span.position(source)
        );
        match &self.help {
            None => head,
            Some(help) => format!("{head}\n  {} at {}", help.note, help.span.position(source)),
        }
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} [{}] at {}",
            self.code, self.message, self.anchor, self.span
        )?;
        match &self.help {
            None => Ok(()),
            Some(help) => write!(f, "\n  {} at {}", help.note, help.span),
        }
    }
}

// ---------------------------------------------------------------------------
// The codes spec/01 §9 reserves. These are not ours to choose.
// ---------------------------------------------------------------------------

/// Leading-operator continuation (`[gram.amb.newline]`).
pub const E_LEADING_OPERATOR: &str = "E0001";
/// Empty statement — a `;` with no statement before it (`[gram.lex.newline]`).
pub const E_EMPTY_STATEMENT: &str = "E0002";
/// Comparison chaining; tier 11 is non-associative (`[gram.expr.prec]`).
pub const E_COMPARISON_CHAIN: &str = "E0003";
/// Float written `1.e5` (`[gram.lex.number]`). Reserved but unreachable here —
/// see [`UNPINNED_CODES`]' notes.
pub const E_FLOAT_DOT_EXPONENT: &str = "E0004";
/// `else` on a new line (`[gram.amb.else]`).
pub const E_ELSE_NEW_LINE: &str = "E0005";
/// Struct literal in condition/scrutinee position (`[gram.amb.structlit]`).
pub const E_STRUCT_LIT_IN_COND: &str = "E0006";
/// Interpolation nesting past depth 8 (`[gram.lex.str]`), parse tier.
pub const E_INTERP_DEPTH: &str = "E0007";
/// A reserved keyword used as an identifier (`[gram.inv.kw]`).
pub const E_KEYWORD_AS_IDENT: &str = "E0008";

// ---------------------------------------------------------------------------
// Lexer tier, E01xx. Only E0108 is spec-pinned.
// ---------------------------------------------------------------------------

/// A byte that begins no token (`[gram.lex]`), and — **spec-pinned since
/// #189/r04** — a `\u{…}` escape whose digit count is outside the
/// production's one-to-six, reported at the escape in char and string
/// literals alike (`[gram.lex.char]`: "Seven or more digits, or none, is
/// **E0101** at the escape").
pub const E_UNEXPECTED_BYTE: &str = "E0101";
/// A string literal with no closing delimiter (`[gram.lex.str]`). **Unpinned.**
pub const E_UNTERMINATED_STRING: &str = "E0102";
/// An escape outside the closed set, in any shape: an unknown letter after
/// the `\`, a `\x` without its two hex digits, a `\u` without its braces.
/// **Spec-pinned since #198/r05** — `[gram.lex.str.escape]`: "`STR_ESC` …
/// and nothing else; any other `\` is **E0101** at the escape", which the
/// multiline reaches through the same production (#215).
///
/// This was **E0103** through 0.1.23, and `\u`-without-braces was
/// **E0104** — two numbers the catalog spends on the multiline's LAYOUT
/// (the opening `"""` must end its line; the closing one must stand
/// alone), so a bad escape and a badly-shaped multiline were
/// indistinguishable in this machine's record. wolf-lang#225 measured it;
/// the span was already identical on both sides, so only the number moved.
pub const E_BAD_ESCAPE: &str = "E0101";
/// A `"""` delimiter that shares its line with text — the OPENING one with
/// text after it, the CLOSING one with text before it on the line.
/// **Spec-pinned by D74** (`[gram.lex.str.multi]`): "a `"""` delimiter
/// stands alone on its line … one rule and one code for both (E0103)".
///
/// One rule, one code, both delimiters: before D74 the spec spent E0104 on
/// the closing side while the catalog spent it on a short margin, and this
/// implementation spent neither — every multiline layout fault came out as
/// this machine's own E0109 (wolf-interp#59). The fold is the ruling's.
pub const E_DELIMITER_SHARES_LINE: &str = "E0103";
/// A `"""` content line that sits LEFT of the margin — fewer leading
/// whitespace bytes than the closing delimiter's own indentation, which is
/// the width stripped from every content line. **Spec-pinned by D74**
/// (`[gram.lex.str.multi]`): the catalog's meaning, which the spec sentence
/// amended to at the ruling. Blank lines are exempt (they have nothing to
/// strip), and so is the opening delimiter's own line, whose remainder has
/// no column to measure.
pub const E_SHORT_MARGIN: &str = "E0104";
/// A `"""` content line whose margin holds the right NUMBER of whitespace
/// bytes but not the right ones — tabs where the closing delimiter has
/// spaces, or the reverse. **Spec-pinned by D74** (`[gram.lex.str.multi]`):
/// "the comparison is byte-for-byte and never by visual width", so eight
/// tabs against eight spaces is this code even though the widths agree.
/// A margin that is SHORT is [`E_SHORT_MARGIN`]'s, tested first.
pub const E_MIXED_MARGIN: &str = "E0105";
/// A numeric literal with no digits after its base prefix, or a malformed
/// exponent (`[gram.lex.number]`). **Unpinned.**
pub const E_BAD_NUMBER: &str = "E0106";
/// Source that is not UTF-8 (`[gram.lex.source]`). **Unpinned.**
pub const E_NOT_UTF8: &str = "E0107";
/// A stray character that begins no token but is not merely an unexpected
/// byte: a byte order mark anywhere but at offset 0. **Spec-pinned by D74**
/// (`[gram.lex.source]`): a BOM "at the very start of a file is stripped and
/// is never a diagnostic … anywhere else it is a stray character (E0107)".
///
/// This shares E0107 with [`E_NOT_UTF8`] the way [`E_BAD_ESCAPE`] shares
/// E0101 with [`E_UNEXPECTED_BYTE`]: one number, two sites, and the clause
/// names the number for this one. This implementation's own
/// `E_BYTE_ORDER_MARK` (an E0105 it invented, colliding with the margin
/// code the catalog assigns) retires here.
pub const E_STRAY_CHARACTER: &str = "E0107";
/// The depth-32 interpolation rail. **Spec-pinned by `[gram.lex.str]`.**
pub const E_INTERP_RAIL: &str = "E0108";
/// A raw or generalized literal that never closes — in practice a
/// generalized one whose body reaches the end of its line, since
/// `[gram.lex.str.gen]`'s `GEN_TEXT` excludes `NL` and `[gram.lex.str.raw]`'s
/// `RAW_TEXT` does not. **Named by D74**: "E0109 stays 'unterminated raw or
/// generalized string'", the family the ruling took the bare `{` OUT of.
///
/// This implementation spent E0109 on the multiline dedent underrun until
/// D74 gave that rule E0104's number; the code lands on the meaning the
/// ruling leaves it, rather than going quiet.
pub const E_UNTERMINATED_RAW: &str = "E0109";
/// A malformed `char` literal — empty, multi-scalar, unterminated before the
/// end of its line, or a `\x`/`\u` escape naming a non-scalar (the surrogate
/// gap `0xD800..=0xDFFF`, or above `0x10FFFF`). **Spec-pinned by
/// `[gram.lex.char]`** (s121, D58): "Malformed shapes are E0110 with an
/// `Error` token, one report each." This number was this implementation's
/// unterminated-interpolation pick until the spec claimed it; that code moved
/// to E0112 — see [`E_UNTERMINATED_INTERP`].
pub const E_BAD_CHAR_LITERAL: &str = "E0110";
/// A bare `{` or `}` in string text; the escapes are `{{` and `}}`
/// (`[gram.lex.str.escape]`). **Unpinned.**
pub const E_BARE_BRACE_IN_STRING: &str = "E0111";
/// An interpolation with no closing `}` (`[gram.lex.str]`). **Unpinned.**
/// Sat at E0110 until `[gram.lex.char]` (s121, D58) pinned that number for
/// malformed char literals; the unpinned code yielded to the spec-pinned one.
pub const E_UNTERMINATED_INTERP: &str = "E0112";

// ---------------------------------------------------------------------------
// Parser tier, E02xx. All unpinned.
// ---------------------------------------------------------------------------

/// The token here begins no legal continuation of the production. **Unpinned.**
pub const E_UNEXPECTED_TOKEN: &str = "E0201";
/// Input ended inside a production. **Corpus-pinned since e561c6f**:
/// `resolve/broken_sibling/entry.lu` pins `fail(E0202)` for the D59
/// broken-sibling witness, so the code is the corpus's to define and no
/// longer this implementation's to choose. (This machine currently answers
/// that witness E0201 — it stops at the first bad token where the
/// counterparty reads to EOF; filed as DIV-2026-019, span-or-code.)
pub const E_UNEXPECTED_EOF: &str = "E0202";
/// `when` needs ≥2 operands (`[gram.expr.conc]`). **Unpinned.**
///
/// The code is E0201, not a dedicated number: wolfc emits the generic
/// expected-token family for this sentence, and the unpinned-code process
/// follows the established assignment rather than inventing a competing one
/// (wolf-interp#3 — this implementation's former E0203 collides with wolfc's
/// toplevel-decl family). The message and the `[gram.expr.conc]` anchor stay
/// specific; only the code family is shared.
pub const E_WHEN_ARITY: &str = E_UNEXPECTED_TOKEN;
/// A bodyless `fn` outside `extern` (`[gram.item.fn]`). **Unpinned.**
pub const E_FN_NEEDS_BODY: &str = "E0204";
/// Range chaining; tier 14 is non-associative (`[gram.expr.prec]`).
/// **Unpinned.**
pub const E_RANGE_CHAIN: &str = "E0205";
/// `assume noalias` needs ≥2 operands (`[gram.expr.unsafe]`). **Unpinned.**
pub const E_ASSUME_ARITY: &str = "E0206";
/// Syntactic nesting past the recursion rail. The **code** is unpinned; the
/// **depth** is not: `[gram.lex.rails]` makes expression/statement recursion
/// depth 256 normative and differential-tested, an amendment this
/// implementation's is01 fuzz smoke provoked. See [`crate::parse::MAX_NESTING`].
pub const E_NESTING_RAIL: &str = "E0207";
/// A moded receiver `(mut e)` / `(take e)` detached from a `.` member.
/// **Spec-pinned**: §3.3's X1 receiver ruling names E0210 — "the moded form is
/// admitted only where a `.` member immediately follows the closing `)`;
/// anywhere else it is a parse error (E0210)."
pub const E_MODED_RECEIVER_DETACHED: &str = "E0210";
/// A file-wide `#![…]` attribute anywhere but the file's first non-trivia
/// construct. **Spec-pinned**: `[gram.attr.index]` (D61, s126) — "Anywhere
/// else it is refused (E0211), parsed but never in force."
pub const E_INNER_ATTR_MISPLACED: &str = "E0211";
/// An origin marker with bad arguments, a duplicated `index` item on one
/// node, or an inner attribute naming anything this implementation does not
/// know. **Spec-pinned**: `[gram.attr.index]` (D61, s126) — refused by name,
/// never ignored; the faulty marker takes no effect. Reported at this
/// machine's resolve rung (wolfc's sema owns the same validation).
pub const E_BAD_INNER_ATTR: &str = "E0813";

/// Every code this implementation invented, with the clause it serves.
///
/// This table exists to be *read by a human writing the s10 catalog*. Codes
/// outside the corpus's pinned set are a known cross-implementation divergence
/// axis (`[proto.cmp.triage]`: the spec is the defendant), and the honest way
/// to run that process is to publish our picks rather than discover them in a
/// differ report.
pub const UNPINNED_CODES: &[(&str, &str, &str)] = &[
    // E0101 sat here until wolf-lang's r04 landed
    // `grammar/char_uni_seven_digits.lu` at pin e6cf24e (wolf-lang#189):
    // `[gram.lex.char]` now names **E0101** for a `\u{…}` escape whose digit
    // count leaves the production's one to six, so the code is the corpus's
    // to define and no longer this implementation's to choose. Its other
    // use — a byte that begins no token — rides the same code, which the
    // clause neither forbids nor mentions.
    // E_UNTERMINATED_STRING (E0102) sat here until D74 and s136's
    // `grammar/str_bare_brace.lu`: the bare `{` in a plain string belongs to
    // the unterminated family — "an interpolation still open when the line
    // ends is E0102 — the string never closed" — so the corpus defines the
    // number and this implementation no longer chooses it.
    //
    // E_BAD_ESCAPE sat here as E0103, and a malformed `\u` beside it as
    // E0104, until wolf-lang#225: `[gram.lex.str.escape]` had already said
    // "any other `\` is **E0101** at the escape", so neither number was
    // ever this implementation's to choose — and both collided with the
    // multiline-layout codes the catalog assigns. Both sites are E0101 now.
    //
    // E_BYTE_ORDER_MARK — an E0105 this implementation invented for a
    // rejected BOM — retires at D74: a BOM at offset 0 is stripped and is
    // never a diagnostic, elsewhere it is E0107, and E0105 belongs to the
    // mixed-margin rule. E_DEDENT_UNDERRUN — an E0109 that stood in for the
    // whole multiline layout tier (wolf-interp#59) — retires beside it now
    // that E0103/E0104/E0105 are implemented and corpus-pinned; E0109 is
    // unspoken by this implementation.
    (E_BAD_NUMBER, "gram.lex.number", "malformed numeric literal"),
    (E_NOT_UTF8, "gram.lex.source", "source is not UTF-8"),
    (
        E_UNTERMINATED_INTERP,
        "gram.lex.str",
        "interpolation with no `}`",
    ),
    (
        E_BARE_BRACE_IN_STRING,
        "gram.lex.str.escape",
        "bare `{`/`}` in string text",
    ),
    // E0201 sat here until wolf-lang's s88 landed `grammar/range_bare.lu`,
    // which pins it: a range with no endpoint is `fail(E0201)` at parse in
    // the corpus, so the code is the corpus's to define and no longer this
    // implementation's to choose. Same sentence, same `when` arity duty
    // ([gram.expr.conc]; wolf-interp#3 retired the competing E0203 in
    // favour of wolfc's established assignment) — only the authority moved.
    // E0202 followed at e561c6f (is27): the s124 broken-sibling witness
    // `resolve/broken_sibling/entry.lu` pins `fail(E0202)`, so the
    // unexpected-EOF code is the corpus's now too — see E_UNEXPECTED_EOF's
    // note and DIV-2026-019 for the recovery-behavior divergence it exposed.
    (
        E_FN_NEEDS_BODY,
        "gram.item.fn",
        "bodyless `fn` outside `extern`",
    ),
    (
        E_RANGE_CHAIN,
        "gram.expr.prec",
        "tier 14 is non-associative",
    ),
    (
        E_ASSUME_ARITY,
        "gram.expr.unsafe",
        "`assume noalias` needs ≥2 operands",
    ),
    (
        E_NESTING_RAIL,
        "gram.lex.rails",
        "syntactic nesting past the normative depth-256 rail",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_are_half_open_byte_offsets() {
        let s = Span::new(4, 9);
        assert_eq!(s.len(), 5);
        assert!(!s.is_empty());
        assert!(Span::empty(7).is_empty());
        assert_eq!(Span::new(1, 3).join(Span::new(8, 9)), Span::new(1, 9));
    }

    #[test]
    fn line_col_is_one_based_and_counts_characters() {
        let source = "let a = 1\nlet bé = 2\nassert(bé == 2)\n";
        // Offset 0: the very first character.
        assert_eq!(line_col(source, 0), (1, 1));
        // `let` on line 2 starts right after the first newline.
        assert_eq!(line_col(source, 10), (2, 1));
        // `é` is two bytes, one character: the column after it counts one.
        let third = source.find("assert").expect("present");
        assert_eq!(line_col(source, third), (3, 1));
        let after_be = source.rfind("é == 2").expect("present") + "é".len();
        assert_eq!(line_col(source, after_be), (3, 10));
        // Past the end clamps to just after the last character.
        assert_eq!(line_col(source, source.len()), (4, 1));
        assert_eq!(line_col(source, source.len() + 40), (4, 1));
        // A mid-code-point offset backs up to the boundary, never panics.
        let mid_e = source.find('é').expect("present") + 1;
        assert_eq!(line_col(source, mid_e), line_col(source, mid_e - 1));
        // The empty source has exactly one position.
        assert_eq!(line_col("", 0), (1, 1));
    }

    #[test]
    fn the_rendered_line_spells_the_location_line_colon_col() {
        let source = "fn main() -> int {\n    0;\n}\n";
        let at = source.find(';').expect("present");
        let d = Diag::new(
            E_EMPTY_STATEMENT,
            Span::new(at, at + 1),
            "gram.lex.newline",
            "stray `;`",
        );
        assert_eq!(
            d.render(source),
            "E0002: stray `;` [gram.lex.newline] at 2:6"
        );
        // The Display grammar (offsets) is unchanged — the REPL's spelling.
        assert_eq!(
            d.to_string(),
            format!("E0002: stray `;` [gram.lex.newline] at {at}..{}", at + 1)
        );
    }

    #[test]
    fn the_protocol_projection_drops_the_message() {
        let d = Diag::new(
            E_EMPTY_STATEMENT,
            Span::new(10, 11),
            "gram.lex.newline",
            "stray `;`",
        );
        let wire = d.to_protocol();
        assert_eq!(wire.code, "E0002");
        assert_eq!(wire.span, [10, 11]);
        assert_eq!(wire.severity, "error");
    }

    #[test]
    fn every_unpinned_code_is_unique_and_out_of_the_spec_reserved_range() {
        let mut seen = std::collections::BTreeSet::new();
        for (code, _, _) in UNPINNED_CODES {
            assert!(seen.insert(*code), "duplicate code {code}");
            // E000x is spec/01 §9's; E0108 is [gram.lex.str]'s; E0110 is
            // [gram.lex.char]'s. Ours live strictly outside all three.
            assert!(code.starts_with("E01") || code.starts_with("E02"), "{code}");
            assert_ne!(*code, E_INTERP_RAIL);
            assert_ne!(*code, E_BAD_CHAR_LITERAL);
        }
    }

    #[test]
    fn the_spec_reserved_codes_are_the_eight_of_section_nine() {
        let reserved = [
            E_LEADING_OPERATOR,
            E_EMPTY_STATEMENT,
            E_COMPARISON_CHAIN,
            E_FLOAT_DOT_EXPONENT,
            E_ELSE_NEW_LINE,
            E_STRUCT_LIT_IN_COND,
            E_INTERP_DEPTH,
            E_KEYWORD_AS_IDENT,
        ];
        assert_eq!(reserved.len(), 8);
        for (i, code) in reserved.iter().enumerate() {
            assert_eq!(*code, format!("E{:04}", i + 1));
        }
    }
}

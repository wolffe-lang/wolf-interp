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

/// One diagnostic. `code` + `span` are protocol; `message` and `anchor` are
/// ours and stay local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag {
    pub code: &'static str,
    pub span: Span,
    /// The `[gram.…]` clause whose production failed. Every parser rule cites
    /// one, which is what makes is02's "every rule cites a clause" cheap.
    pub anchor: &'static str,
    pub message: String,
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
        }
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
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} [{}] at {}",
            self.code, self.message, self.anchor, self.span
        )
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

/// A byte that begins no token (`[gram.lex]`). **Unpinned.**
pub const E_UNEXPECTED_BYTE: &str = "E0101";
/// A string literal with no closing delimiter (`[gram.lex.str]`). **Unpinned.**
pub const E_UNTERMINATED_STRING: &str = "E0102";
/// An escape outside the closed set (`[gram.lex.str]`). **Unpinned.**
pub const E_BAD_ESCAPE: &str = "E0103";
/// A malformed `\u{…}` escape (`[gram.lex.str]`). **Unpinned.**
pub const E_BAD_UNICODE_ESCAPE: &str = "E0104";
/// A byte order mark (`[gram.lex.source]` rejects them). **Unpinned.**
pub const E_BYTE_ORDER_MARK: &str = "E0105";
/// A numeric literal with no digits after its base prefix, or a malformed
/// exponent (`[gram.lex.number]`). **Unpinned.**
pub const E_BAD_NUMBER: &str = "E0106";
/// Source that is not UTF-8 (`[gram.lex.source]`). **Unpinned.**
pub const E_NOT_UTF8: &str = "E0107";
/// The depth-32 interpolation rail. **Spec-pinned by `[gram.lex.str]`.**
pub const E_INTERP_RAIL: &str = "E0108";
/// A `"""` content line indented less than the closing delimiter's column
/// (`[gram.lex.str.multi]`). **Unpinned.**
pub const E_DEDENT_UNDERRUN: &str = "E0109";
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
/// Input ended inside a production. **Unpinned.**
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

/// Every code this implementation invented, with the clause it serves.
///
/// This table exists to be *read by a human writing the s10 catalog*. Codes
/// outside the corpus's pinned set are a known cross-implementation divergence
/// axis (`[proto.cmp.triage]`: the spec is the defendant), and the honest way
/// to run that process is to publish our picks rather than discover them in a
/// differ report.
pub const UNPINNED_CODES: &[(&str, &str, &str)] = &[
    (E_UNEXPECTED_BYTE, "gram.lex", "byte begins no token"),
    (
        E_UNTERMINATED_STRING,
        "gram.lex.str",
        "unterminated string literal",
    ),
    (
        E_BAD_ESCAPE,
        "gram.lex.str",
        "escape outside the closed set",
    ),
    (E_BAD_UNICODE_ESCAPE, "gram.lex.str", "malformed `\\u{…}`"),
    (E_BYTE_ORDER_MARK, "gram.lex.source", "byte order mark"),
    (E_BAD_NUMBER, "gram.lex.number", "malformed numeric literal"),
    (E_NOT_UTF8, "gram.lex.source", "source is not UTF-8"),
    (
        E_DEDENT_UNDERRUN,
        "gram.lex.str.multi",
        "`\"\"\"` line under the dedent column",
    ),
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
    (E_UNEXPECTED_EOF, "gram", "input ended inside a production"),
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

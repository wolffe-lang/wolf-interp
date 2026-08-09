//! Hand-written recursive descent over `spec/01-grammar.md` §2–§6.
//!
//! # No recovery, on purpose
//!
//! The first error wins. There are no error nodes, no resynchronisation points,
//! no "expected `)`, inserting one" heuristics. A parse either yields a tree or
//! yields exactly one [`Diag`] — span, code, and the `[gram.…]` clause whose
//! production failed. This is a deliberate is01 non-target: divergence from the
//! compiler's *recovered* parse of ill-formed input is acceptable and
//! interesting, and the differential protocol only compares the first
//! diagnostic anyway (`[proto.cmp.phase]`).
//!
//! # The precedence climb
//!
//! [`PRECEDENCE`] is a transcription of §3.2's table — tier number, operator
//! spellings, associativity — kept as *data* so `tests/spec_extract.rs` can
//! re-read the table out of the pinned markdown and diff it against this. The
//! spec asks for "one authoritative table, two independent transcriptions"; a
//! transcription diff is a finding, so it is made mechanical rather than
//! trusted.
//!
//! # Where the spec underdetermines, we choose and say so
//!
//! Every such choice is listed in [`CHOICES`], with the clause it interprets.

use crate::ast::*;
use crate::diag::{self, Diag, Span};
use crate::lex::{self, Lexed, Tok, Token};

/// Associativity, per §3.2's third column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assoc {
    Left,
    Right,
    /// "does not chain" — `a < b < c` is a parse error (E0003).
    None,
}

/// §3.2's operator table, tightest first — every row the document gives an
/// associativity. Tiers 1 and 3 are marked `—` there (primaries and prefix
/// operators bind by shape, not by a climb) and are the only rows absent.
///
/// Tier 2 is here as data even though [`Parser::parse_postfix`] handles it
/// structurally: leaving it out would let the postfix row change under us
/// without `tests/spec_extract.rs` noticing, and noticing is this table's job.
pub const PRECEDENCE: &[(u8, &[&str], Assoc)] = &[
    (2, &["(", "[", ".", "?"], Assoc::Left),
    (4, &["as"], Assoc::Left),
    (5, &["*", "/", "%"], Assoc::Left),
    (6, &["+", "-"], Assoc::Left),
    (7, &["<<", ">>"], Assoc::Left),
    (8, &["&"], Assoc::Left),
    (9, &["^"], Assoc::Left),
    (10, &["|"], Assoc::Left),
    (11, &["==", "!=", "<", ">", "<=", ">=", "<=>"], Assoc::None),
    (12, &["&&"], Assoc::Left),
    (13, &["||"], Assoc::Left),
    (14, &["..", "..="], Assoc::None),
    (15, &["else"], Assoc::Right),
];

/// Tier 3's prefix operators, per §3.2 row 3.
pub const PREFIX_OPERATORS: &[&str] = &["!", "-", "&", "&mut", "*", "move", "copy", "shared"];

/// Places where `spec/01-grammar.md` does not determine the parse, and what
/// this implementation does instead. Each is a candidate spec amendment; the
/// process (`[proto.cmp.triage]`) says the document is the defendant, so these
/// are published rather than absorbed.
pub const CHOICES: &[(&str, &str)] = &[
    (
        "gram.amb.structlit",
        "E0006 fires only when the brace body cannot be a block — `IDENT :` or \
         `IDENT ,`. The shorthand `{ x }` is genuinely ambiguous with a block, and \
         §3.4 resolves it in the block's favour, so it is a block, silently. \
         (The amendment settled E0006's *span*, not its trigger; this stays open.)",
    ),
    (
        "gram.item.fn",
        "a bodyless `fn` is accepted inside a `trait` body as well as under \
         `extern`. §2.6 makes `trait_member ::= fn_item` while §2.3's prose says \
         \"Bodyless form (TERM) only under `extern`\"; the pinned corpus's \
         `traits/*.lu` are full of bodyless trait methods, so the prose is the \
         narrower of the two and the corpus decides. E0204 still rejects a \
         bodyless `fn` at item or `impl` level.",
    ),
    (
        "gram.lex.number",
        "E0004 is unreachable: `[gram.amb.intdot]` requires `1.e5` to *parse* as \
         member access on `1`, so nothing at the parse tier can reject it. The code \
         belongs to a lint, not to this rung.",
    ),
    (
        "gram.expr.unsafe",
        "`unsafe c { … }`'s body is skipped brace-balanced over already-lexed \
         tokens. C text that is not also wolf-lexable (character literals, say) \
         therefore fails at the lex rung rather than being ignored.",
    ),
];

/// Choices is01 published that the pin bump to `[gram.lex.rails]` + the seven
/// sibling amendments **closed**. Kept as a table, not deleted, because the
/// point of the divergence pipeline is that a filed gap has a visible fate:
/// each row names the clause that now decides it, and a test asserts the
/// implementation still behaves the way the amendment says.
///
/// This is the is01→is02 CHOICES ledger update in machine-readable form.
pub const CHOICES_RESOLVED: &[(&str, &str)] = &[
    (
        "gram.lex.rails",
        "syntactic recursion depth. is01 railed at 32 (borrowed from the string \
         rail) and filed the gap; the amendment makes 256 normative and requires \
         both implementations to enforce the same value. MAX_NESTING is 256.",
    ),
    (
        "gram.lex.ident",
        "`_foo` is an identifier. is01 accepted it against a literal reading of \
         `IDENT ::= XID_Start XID_Continue*`; the amended production is \
         `('_' XID_Continue+) | (XID_Start XID_Continue*)`, and a bare `_` stays \
         the wildcard.",
    ),
    (
        "gram.expr.region",
        "`freeze` takes a tier-3 operand. is01 parsed `freeze r == x` as \
         `(freeze r) == x`; the amended EBNF reads `'freeze' prefix_operand` and \
         the prose spells the same example out.",
    ),
    (
        "gram.amb.structlit",
        "the `in`-block header is a no-struct-literal position. is01 inferred it \
         (otherwise `in r { … }` eats its own block); `[gram.amb.structlit]` now \
         names condition/`in`-header/scrutinee explicitly.",
    ),
    (
        "gram.expr.flow",
        "match-arm separators follow the §3.4 prose, not `arm_sep ::= ',' | TERM` \
         read alone. The amendment brings the EBNF into line with the prose.",
    ),
    (
        "gram.amb.structlit",
        "E0006's primary span is the opening `{`. is01 spanned the whole \
         `Point { x: 0 }`; §9's reservation now pins the brace, and the parser \
         was moved onto it.",
    ),
];

/// How deep syntactic nesting may go before the parser refuses.
///
/// **Normative since the `[gram.lex.rails]` amendment**, which is01's fuzz
/// smoke provoked: "expression/statement recursion depth **256** — deeper input
/// is rejected with a diagnostic at the point the rail is hit. Both
/// implementations enforce identical rail values (differential-tested)."
///
/// is01 railed at 32 (the string rail's number, reused so there was one figure
/// to argue about) and filed the gap; the amendment picked 256 and made it a
/// differential-comparison surface, so the number here is the spec's, not a
/// tolerance. The rail is still enforced with E0207 — the *code* remains this
/// implementation's invention (`diag::UNPINNED_CODES`), only the depth is
/// pinned.
///
/// 256 frames of `parse_expr` fit inside a debug build's 2 MiB test-thread
/// stack with room to spare; `tests/fuzz_smoke.rs` proves it at the boundary.
/// The deepest nesting anywhere in the pinned corpus is 6.
pub const MAX_NESTING: usize = 256;

type PResult<T> = Result<T, Diag>;

/// A successfully parsed unit, plus anything the lexer deferred to this tier.
#[derive(Debug, Clone)]
pub struct Parsed {
    pub unit: Unit,
    /// Parse-tier diagnostics the *lexer* detected — today only E0007
    /// (`[gram.lex.str]` says depth-8 nesting must still tokenize so the parser
    /// can produce the friendly error).
    pub deferred: Vec<Diag>,
}

/// Stack reserved for the recursive descent, in bytes.
///
/// `[gram.lex.rails]` makes depth **256** normative and differential-tested, so
/// the parser has to *reach* 256 in order to be the thing that stops the
/// program — a stack overflow at depth 190 is not "enforcing the rail", it is
/// crashing, and `[proto.record]` has no verdict for a crash.
///
/// A debug build of this parser needs between 8 and 16 MiB to climb 260 nested
/// `(`; a release build needs far less. Rather than depend on the ambient
/// thread's stack (2 MiB by default, 8 MiB for the main thread on Linux, and
/// 1 MiB on Windows), [`parse`] runs the descent on a thread whose stack it
/// chose. The reservation is address space, not memory: only the pages the
/// descent actually touches are ever committed.
const PARSE_STACK: usize = 64 * 1024 * 1024;

/// Parses an already-lexed token stream.
///
/// Runs the descent on a [`PARSE_STACK`]-sized thread so the nesting rail, not
/// the stack, is what stops hostile input (`[gram.lex.rails]`).
///
/// # Errors
///
/// The first parse failure, as a `{code, span, anchor, message}` diagnostic.
/// There is never a second: this parser does not recover.
pub fn parse(lexed: &Lexed) -> Result<Parsed, Diag> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(PARSE_STACK)
            .name("wolf-interp-parse".to_owned())
            .spawn_scoped(scope, || parse_on_this_stack(lexed))
            .expect("the parser's stack thread must spawn")
            .join()
            // The descent itself never panics; a panic here is an interpreter
            // bug and is propagated rather than converted into a verdict.
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

fn parse_on_this_stack(lexed: &Lexed) -> Result<Parsed, Diag> {
    let mut parser = Parser {
        tokens: &lexed.tokens,
        pos: 0,
        eof: lexed
            .tokens
            .last()
            .map_or(Span::empty(0), |t| Span::empty(t.span.end)),
        no_struct_lit: false,
        in_trait_body: false,
        depth: 0,
    };
    let unit = parser.parse_unit()?;

    // A deferred diagnostic that sits before the (nonexistent) parse error is
    // still the file's first diagnostic.
    Ok(Parsed {
        unit,
        deferred: lexed.deferred.clone(),
    })
}

/// Lexes and parses one source string, returning the first diagnostic of the
/// whole frontend in source order.
///
/// # Errors
///
/// The first lex-tier or parse-tier diagnostic.
pub fn parse_source(source: &str) -> Result<Parsed, Diag> {
    let lexed = lex::lex(source);
    if let Some(first) = lexed.first_error() {
        return Err(first.clone());
    }
    let parsed = parse(&lexed);
    match parsed {
        Ok(parsed) => match parsed.deferred.first() {
            Some(first) => Err(first.clone()),
            None => Ok(parsed),
        },
        Err(error) => {
            // Deferred lex diagnostics compete with the parse error on span
            // order — whichever is earlier is the file's first diagnostic.
            match lexed.deferred.iter().min_by_key(|d| d.span.start) {
                Some(deferred) if deferred.span.start <= error.span.start => Err(deferred.clone()),
                _ => Err(error),
            }
        }
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    eof: Span,
    /// `[gram.amb.structlit]`: in condition, `in`-header and scrutinee position
    /// a `{` begins the block, so struct literals need parens. The flag is
    /// cleared by every `(`, `[` and `{` — a nested expression is no longer in
    /// that position.
    no_struct_lit: bool,
    /// Inside a `trait` body, where `fn_item`'s bodyless form is legal without
    /// `extern` — see [`CHOICES`].
    in_trait_body: bool,
    /// Current syntactic nesting, against [`MAX_NESTING`].
    depth: usize,
}

impl<'a> Parser<'a> {
    // -- token plumbing ----------------------------------------------------

    fn tok(&self) -> Option<&'a Tok> {
        self.tokens.get(self.pos).map(|t| &t.tok)
    }

    fn tok_at(&self, offset: usize) -> Option<&'a Tok> {
        self.tokens.get(self.pos + offset).map(|t| &t.tok)
    }

    fn span(&self) -> Span {
        self.tokens.get(self.pos).map_or(self.eof, |t| t.span)
    }

    fn prev_span(&self) -> Span {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map_or(self.eof, |t| t.span)
    }

    fn advance(&mut self) -> Span {
        let span = self.span();
        self.pos += 1;
        span
    }

    fn at(&self, tok: &Tok) -> bool {
        self.tok() == Some(tok)
    }

    fn at_kw(&self, kw: &str) -> bool {
        matches!(self.tok(), Some(Tok::Kw(k)) if *k == kw)
    }

    /// A contextual keyword (`[gram.inv.ctx]`) — an identifier everywhere else,
    /// so it is matched by spelling and never by token kind.
    fn at_ctx(&self, word: &str) -> bool {
        matches!(self.tok(), Some(Tok::Ident(name)) if name == word)
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.at(tok) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.at_kw(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_ctx(&mut self, word: &str) -> bool {
        if self.at_ctx(word) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn error(&self, code: &'static str, anchor: &'static str, message: impl Into<String>) -> Diag {
        Diag::new(code, self.span(), anchor, message)
    }

    fn unexpected(&self, anchor: &'static str, wanted: &str) -> Diag {
        match self.tok() {
            None => Diag::new(
                diag::E_UNEXPECTED_EOF,
                self.eof,
                anchor,
                format!("the file ends where {wanted} was required"),
            ),
            Some(tok) => Diag::new(
                diag::E_UNEXPECTED_TOKEN,
                self.span(),
                anchor,
                format!("expected {wanted}, found {}", tok.describe()),
            ),
        }
    }

    fn expect(&mut self, tok: &Tok, anchor: &'static str) -> PResult<Span> {
        if self.at(tok) {
            Ok(self.advance())
        } else {
            Err(self.unexpected(anchor, &tok.describe()))
        }
    }

    fn expect_kw(&mut self, kw: &'static str, anchor: &'static str) -> PResult<Span> {
        if self.at_kw(kw) {
            Ok(self.advance())
        } else {
            Err(self.unexpected(anchor, &format!("keyword `{kw}`")))
        }
    }

    /// Every position that wants a *name*. A reserved keyword here is E0008 —
    /// "keyword as identifier", the code `corpus/grammar/when_reserved.lu`
    /// pins.
    fn expect_ident(&mut self, anchor: &'static str) -> PResult<Ident> {
        match self.tok() {
            Some(Tok::Ident(name)) => {
                let span = self.advance();
                Ok(Ident {
                    name: name.clone(),
                    span,
                })
            }
            Some(Tok::Kw(kw)) => Err(Diag::new(
                diag::E_KEYWORD_AS_IDENT,
                self.span(),
                anchor,
                format!(
                    "`{kw}` is a reserved keyword and cannot be a name; wolf has no raw \
                     identifiers, so pick another name"
                ),
            )),
            _ => Err(self.unexpected(anchor, "an identifier")),
        }
    }

    /// Skips inserted terminators. An **explicit** `;` is never skipped: at
    /// statement position it is an empty statement (E0002), and everywhere else
    /// it is the statement's own terminator.
    fn skip_inserted_terms(&mut self) {
        while matches!(self.tok(), Some(Tok::Term { explicit: false })) {
            self.pos += 1;
        }
    }

    fn skip_terms(&mut self) {
        while matches!(self.tok(), Some(Tok::Term { .. })) {
            self.pos += 1;
        }
    }

    /// A statement terminator: the inserted one, an explicit `;`, or — Go's
    /// rule 2 — nothing at all before a closing `}` or the end of input.
    fn expect_term(&mut self, anchor: &'static str) -> PResult<()> {
        match self.tok() {
            Some(Tok::Term { .. }) => {
                self.pos += 1;
                Ok(())
            }
            None | Some(Tok::RBrace) => Ok(()),
            _ => Err(self.unexpected(anchor, "an end of statement")),
        }
    }

    /// Runs `f` with the struct-literal restriction in a known state, restoring
    /// it afterwards. Every bracketed context clears it: inside `(…)` a struct
    /// literal is legal again, which is exactly the fix E0006 suggests.
    fn with_struct_lit<T>(&mut self, allowed: bool, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.no_struct_lit;
        self.no_struct_lit = !allowed;
        let out = f(self);
        self.no_struct_lit = saved;
        out
    }

    fn with_trait_body<T>(&mut self, inside: bool, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.in_trait_body;
        self.in_trait_body = inside;
        let out = f(self);
        self.in_trait_body = saved;
        out
    }

    /// Runs one recursive production against the nesting rail.
    ///
    /// Every production that can recur through itself without consuming a
    /// bounded amount of input goes through here: expressions, prefix chains,
    /// types, patterns and blocks. The rail is the parser's, not the spec's —
    /// see [`MAX_NESTING`].
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        if self.depth >= MAX_NESTING {
            return Err(self.error(
                diag::E_NESTING_RAIL,
                "gram.expr",
                format!("syntax nested more than {MAX_NESTING} deep"),
            ));
        }
        self.depth += 1;
        let out = f(self);
        self.depth -= 1;
        out
    }

    // -- unit & items ------------------------------------------------------

    fn parse_unit(&mut self) -> PResult<Unit> {
        let start = self.span().start;
        let mut items = Vec::new();
        loop {
            self.skip_inserted_terms();
            match self.tok() {
                None => break,
                Some(Tok::Term { explicit: true }) => return Err(self.empty_statement()),
                Some(tok) if tok.is_binary_only() => return Err(self.leading_operator()),
                _ => {}
            }
            items.push(self.parse_item()?);
        }
        let end = self.prev_span().end;
        Ok(Unit {
            items,
            span: Span::new(start, end.max(start)),
        })
    }

    /// E0002 — `[gram.lex.newline]`: "An empty statement (a `;` with no
    /// statement before it on the same line) is an error."
    fn empty_statement(&self) -> Diag {
        Diag::new(
            diag::E_EMPTY_STATEMENT,
            self.span(),
            "gram.lex.newline",
            "an empty statement; `;` only separates statements inside a single-line block",
        )
    }

    /// E0001 — `[gram.amb.newline]`: continuation is trailing-operator style,
    /// so a statement that *begins* with a binary operator is the broken half
    /// of a line the writer meant to continue.
    fn leading_operator(&self) -> Diag {
        let found = self.tok().map_or_else(String::new, Tok::describe);
        Diag::new(
            diag::E_LEADING_OPERATOR,
            self.span(),
            "gram.amb.newline",
            format!(
                "a statement cannot begin with {found}; a terminator was inserted at the end \
                 of the previous line, so continuations put the operator there instead"
            ),
        )
    }

    /// E0005 — `[gram.amb.else]`: `else` must share the line with the preceding
    /// `}`, or the inserted terminator orphans it.
    fn orphaned_else(&self) -> Diag {
        Diag::new(
            diag::E_ELSE_NEW_LINE,
            self.span(),
            "gram.amb.else",
            "`else` must be on the same line as the preceding `}`",
        )
    }

    fn parse_attributes(&mut self) -> PResult<Vec<Attribute>> {
        let mut out = Vec::new();
        loop {
            self.skip_inserted_terms();
            if !self.at(&Tok::HashBracket) {
                break;
            }
            let start = self.advance().start;
            let mut attrs = vec![self.parse_attr()?];
            while self.eat(&Tok::Comma) {
                if self.at(&Tok::RBracket) {
                    break;
                }
                attrs.push(self.parse_attr()?);
            }
            let end = self.expect(&Tok::RBracket, "gram.item.attr")?.end;
            out.push(Attribute {
                attrs,
                span: Span::new(start, end),
            });
        }
        Ok(out)
    }

    fn parse_attr(&mut self) -> PResult<Attr> {
        let path = self.parse_path("gram.item.attr")?;
        let start = path.span.start;
        let input = if self.at(&Tok::LParen) {
            self.advance();
            let mut args = Vec::new();
            if !self.at(&Tok::RParen) {
                loop {
                    args.push(self.parse_attr_arg()?);
                    if !self.eat(&Tok::Comma) || self.at(&Tok::RParen) {
                        break;
                    }
                }
            }
            self.expect(&Tok::RParen, "gram.item.attr")?;
            Some(AttrInput::Args(args))
        } else if self.eat(&Tok::Assign) {
            Some(AttrInput::Literal(Box::new(self.parse_literal_only()?)))
        } else {
            None
        };
        Ok(Attr {
            path,
            input,
            span: Span::new(start, self.prev_span().end),
        })
    }

    fn parse_attr_arg(&mut self) -> PResult<AttrArg> {
        // `attr_arg ::= attr | literal`; `key = "v"` is an attr with `=` input.
        if matches!(self.tok(), Some(Tok::Ident(_))) {
            Ok(AttrArg::Nested(self.parse_attr()?))
        } else {
            Ok(AttrArg::Literal(Box::new(self.parse_literal_only()?)))
        }
    }

    /// Attributes are closed and structured — not token soup
    /// (`[gram.item.attr]`), so only literals appear in their leaves.
    fn parse_literal_only(&mut self) -> PResult<Expr> {
        let anchor = "gram.item.attr";
        match self.tok() {
            Some(Tok::Int(_) | Tok::Float(_) | Tok::StrStart(_)) => self.parse_primary(),
            Some(Tok::Kw("true" | "false")) => self.parse_primary(),
            Some(Tok::Minus) => {
                let start = self.advance().start;
                let operand = self.parse_literal_only()?;
                let span = Span::new(start, operand.span.end);
                Ok(Expr {
                    kind: Box::new(ExprKind::Unary {
                        op: UnOp::Neg,
                        operand,
                    }),
                    span,
                    anchor,
                })
            }
            _ => Err(self.unexpected(anchor, "a literal")),
        }
    }

    fn parse_visibility(&mut self) -> PResult<Option<Visibility>> {
        if !self.at_kw("pub") {
            return Ok(None);
        }
        let start = self.advance().start;
        let mut package_only = false;
        if self.at(&Tok::LParen) {
            self.advance();
            if !self.eat_ctx("pkg") {
                return Err(self.unexpected("gram.item.unit", "`pkg`"));
            }
            self.expect(&Tok::RParen, "gram.item.unit")?;
            package_only = true;
        }
        Ok(Some(Visibility {
            package_only,
            span: Span::new(start, self.prev_span().end),
        }))
    }

    fn at_item_start(&self) -> bool {
        match self.tok() {
            Some(Tok::HashBracket) => true,
            Some(Tok::Kw(kw)) => matches!(
                *kw,
                "pub"
                    | "fn"
                    | "let"
                    | "var"
                    | "const"
                    | "type"
                    | "struct"
                    | "enum"
                    | "trait"
                    | "impl"
                    | "use"
                    | "import"
                    | "comptime"
                    | "extern"
                    | "export"
            ),
            _ => false,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn parse_item(&mut self) -> PResult<Item> {
        let attrs = self.parse_attributes()?;
        self.skip_inserted_terms();
        let start = self.span().start;
        let visibility = self.parse_visibility()?;

        let (kind, anchor) = match self.tok() {
            Some(Tok::Kw("comptime" | "extern" | "export" | "fn")) => (
                ItemKind::Fn(Box::new(self.parse_fn_decl()?)),
                "gram.item.fn",
            ),
            Some(Tok::Kw("let" | "var" | "const")) => (
                ItemKind::Binding(Box::new(self.parse_binding()?)),
                "gram.item.let",
            ),
            Some(Tok::Kw("type")) => (
                ItemKind::TypeAlias(Box::new(self.parse_type_alias()?)),
                "gram.item.type",
            ),
            Some(Tok::Kw("struct")) => (
                ItemKind::Struct(Box::new(self.parse_struct_def(true)?)),
                "gram.item.type",
            ),
            Some(Tok::Kw("enum")) => (
                ItemKind::Enum(Box::new(self.parse_enum_def(true)?)),
                "gram.item.type",
            ),
            Some(Tok::Kw("trait")) => (
                ItemKind::Trait(Box::new(self.parse_trait_def()?)),
                "gram.item.trait",
            ),
            Some(Tok::Kw("impl")) => (
                ItemKind::Impl(Box::new(self.parse_impl_def()?)),
                "gram.item.trait",
            ),
            Some(Tok::Kw("use")) => (
                ItemKind::Use(Box::new(self.parse_use_decl()?)),
                "gram.item.use",
            ),
            Some(Tok::Kw("import")) => {
                self.advance();
                // `import c "header"` — `c` is contextual (`[gram.inv.ctx]`).
                if !self.eat_ctx("c") {
                    return Err(self.unexpected("gram.item.use", "`c`"));
                }
                let literal = self.parse_string_literal("gram.item.use")?;
                self.expect_term("gram.item.use")?;
                (ItemKind::ImportC(Box::new(literal)), "gram.item.use")
            }
            _ => return Err(self.unexpected("gram.item.unit", "an item")),
        };

        Ok(Item {
            attrs,
            visibility,
            kind,
            span: Span::new(start, self.prev_span().end),
            anchor,
        })
    }

    fn parse_fn_decl(&mut self) -> PResult<FnDecl> {
        let anchor = "gram.item.fn";
        let start = self.span().start;
        let mut quals = Vec::new();
        loop {
            if self.at_kw("comptime") {
                quals.push(FnQual::Comptime(self.advance()));
            } else if self.at_kw("export") {
                quals.push(FnQual::Export(self.advance()));
            } else if self.at_kw("extern") {
                let kw = self.advance();
                let abi = self.parse_string_literal(anchor)?;
                let span = Span::new(kw.start, abi.span.end);
                quals.push(FnQual::Extern { abi, span });
            } else {
                break;
            }
        }
        let is_extern = quals.iter().any(|q| matches!(q, FnQual::Extern { .. }));
        self.expect_kw("fn", anchor)?;
        let name = self.expect_ident(anchor)?;
        let generics = self.parse_generics()?;

        self.expect(&Tok::LParen, anchor)?;
        let params = self.with_struct_lit(true, Parser::parse_params)?;
        self.expect(&Tok::RParen, anchor)?;

        let ret = if self.eat(&Tok::Arrow) {
            Some(self.parse_ret_type()?)
        } else {
            None
        };

        let body = if self.at(&Tok::LBrace) {
            // A default method's body is an ordinary body; items nested in it
            // are not trait members and still need bodies of their own.
            Some(self.with_trait_body(false, |p| p.with_struct_lit(true, Parser::parse_block))?)
        } else {
            // `[gram.item.fn]`: "Bodyless form (TERM) only under `extern`" —
            // plus trait members, which §2.6 builds out of `fn_item` and the
            // pinned corpus writes bodyless throughout (see [`CHOICES`]).
            if !is_extern && !self.in_trait_body {
                return Err(self.error(
                    diag::E_FN_NEEDS_BODY,
                    anchor,
                    "a function needs a body unless it is `extern` or a trait member",
                ));
            }
            self.expect_term(anchor)?;
            None
        };

        Ok(FnDecl {
            quals,
            name,
            generics,
            params,
            ret,
            body,
            span: Span::new(start, self.prev_span().end),
        })
    }

    fn parse_params(&mut self) -> PResult<Vec<Param>> {
        let anchor = "gram.item.fn";
        let mut params = Vec::new();
        if self.at(&Tok::RParen) {
            return Ok(params);
        }
        loop {
            self.skip_inserted_terms();
            let start = self.span().start;
            let mode = self.parse_param_mode();
            // `param ::= param_mode? IDENT ':' type | param_mode? 'self' view_set?`
            let kind = if self.at_ctx("self") {
                self.advance();
                let mut view_set = Vec::new();
                if self.at(&Tok::Dot) && self.tok_at(1) == Some(&Tok::LBrace) {
                    self.advance();
                    self.advance();
                    loop {
                        view_set.push(self.expect_ident(anchor)?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(&Tok::RBrace, anchor)?;
                }
                ParamKind::SelfParam { view_set }
            } else {
                let name = self.expect_ident(anchor)?;
                // The counter-example `fn f(x: mut int)` dies here: modes
                // precede the *name*, so `mut` after `:` is not a type.
                self.expect(&Tok::Colon, anchor)?;
                let ty = self.parse_type()?;
                ParamKind::Named { name, ty }
            };
            params.push(Param {
                mode,
                kind,
                span: Span::new(start, self.prev_span().end),
            });
            self.skip_inserted_terms();
            if !self.eat(&Tok::Comma) {
                break;
            }
            self.skip_inserted_terms();
            if self.at(&Tok::RParen) {
                break;
            }
        }
        Ok(params)
    }

    fn parse_param_mode(&mut self) -> Option<ParamMode> {
        if self.eat_kw("mut") {
            Some(ParamMode::Mut)
        } else if self.eat_kw("take") {
            Some(ParamMode::Take)
        } else {
            None
        }
    }

    fn parse_generics(&mut self) -> PResult<Vec<GenericParam>> {
        let anchor = "gram.item.fn";
        if !self.at(&Tok::LBracket) {
            return Ok(Vec::new());
        }
        self.advance();
        let mut out = Vec::new();
        loop {
            self.skip_inserted_terms();
            if self.at(&Tok::RBracket) {
                break;
            }
            let name = self.expect_ident(anchor)?;
            let start = name.span.start;
            let bound = if self.eat(&Tok::Colon) {
                if self.at_kw("type") {
                    Some(Bound::TypeOfTypes(self.advance()))
                } else {
                    let mut paths = vec![self.parse_path(anchor)?];
                    while self.eat(&Tok::Plus) {
                        paths.push(self.parse_path(anchor)?);
                    }
                    Some(Bound::Paths(paths))
                }
            } else {
                None
            };
            out.push(GenericParam {
                name,
                bound,
                span: Span::new(start, self.prev_span().end),
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RBracket, anchor)?;
        Ok(out)
    }

    fn parse_binding(&mut self) -> PResult<Binding> {
        let anchor = "gram.item.let";
        let start = self.span().start;
        let kind = if self.eat_kw("let") {
            BindingKind::Let
        } else if self.eat_kw("var") {
            BindingKind::Var
        } else {
            self.expect_kw("const", anchor)?;
            BindingKind::Const
        };
        // `const_item` binds an IDENT, not a pattern.
        let pattern = if kind == BindingKind::Const {
            let name = self.expect_ident(anchor)?;
            let span = name.span;
            Pattern {
                kind: Box::new(PatKind::Binding(name)),
                span,
                anchor: "gram.pat",
            }
        } else {
            self.parse_pattern()?
        };
        let ty = if self.eat(&Tok::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&Tok::Assign, anchor)?;
        let value = self.with_struct_lit(true, Parser::parse_expr)?;
        self.expect_term(anchor)?;
        Ok(Binding {
            kind,
            pattern,
            ty,
            value,
            span: Span::new(start, self.prev_span().end),
        })
    }

    fn parse_type_alias(&mut self) -> PResult<TypeAlias> {
        let anchor = "gram.item.type";
        let start = self.expect_kw("type", anchor)?.start;
        let name = self.expect_ident(anchor)?;
        let generics = self.parse_generics()?;
        self.expect(&Tok::Assign, anchor)?;
        let def = if self.at_kw("struct") {
            TypeDef::Struct(Box::new(self.parse_struct_def(false)?))
        } else if self.at_kw("enum") {
            TypeDef::Enum(Box::new(self.parse_enum_def(false)?))
        } else {
            TypeDef::Alias(self.parse_type()?)
        };
        // `type_item ::= … TERM?` — the terminator is optional here because the
        // struct/enum forms end in `}`.
        if matches!(self.tok(), Some(Tok::Term { .. })) {
            self.pos += 1;
        }
        Ok(TypeAlias {
            name,
            generics,
            def,
            span: Span::new(start, self.prev_span().end),
        })
    }

    fn parse_struct_def(&mut self, named: bool) -> PResult<StructDef> {
        let anchor = "gram.item.type";
        let start = self.expect_kw("struct", anchor)?.start;
        let name = if named {
            Some(self.expect_ident(anchor)?)
        } else {
            None
        };
        let generics = if named {
            self.parse_generics()?
        } else {
            Vec::new()
        };
        self.expect(&Tok::LBrace, anchor)?;
        let mut fields = Vec::new();
        loop {
            self.skip_terms();
            if self.at(&Tok::RBrace) || self.tok().is_none() {
                break;
            }
            let attrs = self.parse_attributes()?;
            self.skip_terms();
            let field_start = self.span().start;
            let visibility = self.parse_visibility()?;
            let fname = self.expect_ident(anchor)?;
            self.expect(&Tok::Colon, anchor)?;
            let ty = self.with_struct_lit(true, Parser::parse_type)?;
            fields.push(Field {
                attrs,
                visibility,
                name: fname,
                ty,
                span: Span::new(field_start, self.prev_span().end),
            });
            // `field ::= … ','?` — fields are newline-separated declarations,
            // so the comma is optional and a terminator does the same work.
            self.eat(&Tok::Comma);
        }
        let end = self.expect(&Tok::RBrace, anchor)?.end;
        Ok(StructDef {
            name,
            generics,
            fields,
            span: Span::new(start, end),
        })
    }

    fn parse_enum_def(&mut self, named: bool) -> PResult<EnumDef> {
        let anchor = "gram.item.type";
        let start = self.expect_kw("enum", anchor)?.start;
        let name = if named {
            Some(self.expect_ident(anchor)?)
        } else {
            None
        };
        let generics = if named {
            self.parse_generics()?
        } else {
            Vec::new()
        };
        self.expect(&Tok::LBrace, anchor)?;
        let mut variants = Vec::new();
        loop {
            self.skip_terms();
            if self.at(&Tok::RBrace) || self.tok().is_none() {
                break;
            }
            let vname = self.expect_ident(anchor)?;
            let vstart = vname.span.start;
            let mut payload = Vec::new();
            if self.eat(&Tok::LParen) {
                loop {
                    payload.push(self.with_struct_lit(true, Parser::parse_type)?);
                    if !self.eat(&Tok::Comma) || self.at(&Tok::RParen) {
                        break;
                    }
                }
                self.expect(&Tok::RParen, anchor)?;
            }
            variants.push(Variant {
                name: vname,
                payload,
                span: Span::new(vstart, self.prev_span().end),
            });
            self.skip_terms();
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.skip_terms();
        let end = self.expect(&Tok::RBrace, anchor)?.end;
        Ok(EnumDef {
            name,
            generics,
            variants,
            span: Span::new(start, end),
        })
    }

    fn parse_trait_def(&mut self) -> PResult<TraitDef> {
        let anchor = "gram.item.trait";
        let start = self.expect_kw("trait", anchor)?.start;
        let name = self.expect_ident(anchor)?;
        let generics = self.parse_generics()?;
        self.expect(&Tok::LBrace, anchor)?;
        // Trait members may be bodyless — see [`CHOICES`] under `gram.item.fn`.
        let members = self.with_trait_body(true, Parser::parse_members)?;
        let end = self.expect(&Tok::RBrace, anchor)?.end;
        Ok(TraitDef {
            name,
            generics,
            members,
            span: Span::new(start, end),
        })
    }

    fn parse_impl_def(&mut self) -> PResult<ImplDef> {
        let anchor = "gram.item.trait";
        let start = self.expect_kw("impl", anchor)?.start;
        let generics = self.parse_generics()?;
        // The impl subject is a *type* (`impl[T] List[T] { … }`); with `for`,
        // the first type is the trait applied to its arguments.
        let first = self.with_struct_lit(false, Parser::parse_type)?;
        let subject = if self.eat_kw("for") {
            Some(self.with_struct_lit(false, Parser::parse_type)?)
        } else {
            None
        };
        self.expect(&Tok::LBrace, anchor)?;
        // An `impl` member is a definition: bodies are required there.
        let members = self.with_trait_body(false, Parser::parse_members)?;
        let end = self.expect(&Tok::RBrace, anchor)?.end;
        Ok(ImplDef {
            generics,
            trait_or_subject: first,
            subject,
            members,
            span: Span::new(start, end),
        })
    }

    /// `trait_member`/`impl_member ::= fn_item | type_item | const_item`.
    fn parse_members(&mut self) -> PResult<Vec<Item>> {
        let mut members = Vec::new();
        loop {
            self.skip_terms();
            if self.at(&Tok::RBrace) || self.tok().is_none() {
                break;
            }
            members.push(self.parse_item()?);
        }
        Ok(members)
    }

    fn parse_use_decl(&mut self) -> PResult<UseDecl> {
        let anchor = "gram.item.use";
        let start = self.expect_kw("use", anchor)?.start;
        // `use path ('.' '{' IDENT,* '}')? ('as' IDENT)? TERM`
        let mut segments = vec![self.expect_ident(anchor)?];
        let mut list = Vec::new();
        loop {
            if !self.at(&Tok::Dot) {
                break;
            }
            if self.tok_at(1) == Some(&Tok::LBrace) {
                self.advance();
                self.advance();
                loop {
                    self.skip_inserted_terms();
                    list.push(self.expect_ident(anchor)?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                    self.skip_inserted_terms();
                    if self.at(&Tok::RBrace) {
                        break;
                    }
                }
                self.expect(&Tok::RBrace, anchor)?;
                break;
            }
            self.advance();
            segments.push(self.expect_ident(anchor)?);
        }
        let alias = if self.eat_kw("as") {
            Some(self.expect_ident(anchor)?)
        } else {
            None
        };
        self.expect_term(anchor)?;
        let path_span = Span::new(
            segments[0].span.start,
            segments.last().map_or(start, |s| s.span.end),
        );
        Ok(UseDecl {
            path: Path {
                segments,
                span: path_span,
            },
            list,
            alias,
            span: Span::new(start, self.prev_span().end),
        })
    }

    fn parse_path(&mut self, anchor: &'static str) -> PResult<Path> {
        let first = self.expect_ident(anchor)?;
        let start = first.span.start;
        let mut segments = vec![first];
        while self.at(&Tok::Dot) && matches!(self.tok_at(1), Some(Tok::Ident(_))) {
            self.advance();
            segments.push(self.expect_ident(anchor)?);
        }
        let end = segments.last().map_or(start, |s| s.span.end);
        Ok(Path {
            segments,
            span: Span::new(start, end),
        })
    }

    // -- types -------------------------------------------------------------

    fn parse_ret_type(&mut self) -> PResult<RetType> {
        let ty = self.parse_type()?;
        let start = ty.span.start;
        // `ret_type ::= type ('!' error_row)?` — `-> int ! {Failed, Slow}`.
        // `-> !int` is the other shape entirely: `type ::= '!' type`.
        let row = if self.at(&Tok::Bang) {
            self.advance();
            Some(self.parse_error_row()?)
        } else {
            None
        };
        Ok(RetType {
            ty,
            row,
            span: Span::new(start, self.prev_span().end),
        })
    }

    fn parse_error_row(&mut self) -> PResult<ErrorRow> {
        let anchor = "gram.type.row";
        let start = self.expect(&Tok::LBrace, anchor)?.start;
        let mut entries = Vec::new();
        let mut open = false;
        loop {
            self.skip_inserted_terms();
            if self.at(&Tok::RBrace) {
                break;
            }
            if self.eat(&Tok::DotDot) {
                open = true;
                self.eat(&Tok::Comma);
                break;
            }
            let path = self.parse_path(anchor)?;
            let estart = path.span.start;
            let mut payload = Vec::new();
            if self.eat(&Tok::LParen) {
                loop {
                    payload.push(self.parse_type()?);
                    if !self.eat(&Tok::Comma) || self.at(&Tok::RParen) {
                        break;
                    }
                }
                self.expect(&Tok::RParen, anchor)?;
            }
            entries.push(RowEntry {
                path,
                payload,
                span: Span::new(estart, self.prev_span().end),
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.skip_inserted_terms();
        let end = self.expect(&Tok::RBrace, anchor)?.end;
        Ok(ErrorRow {
            entries,
            open,
            span: Span::new(start, end),
        })
    }

    fn prefix_type_kw(&self) -> Option<PrefixTypeKw> {
        match self.tok() {
            Some(Tok::Kw("shared")) => Some(PrefixTypeKw::Shared),
            Some(Tok::Kw("handle")) => Some(PrefixTypeKw::Handle),
            Some(Tok::Kw("weak")) => Some(PrefixTypeKw::Weak),
            Some(Tok::Kw("distinct")) => Some(PrefixTypeKw::Distinct),
            _ => None,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn parse_type(&mut self) -> PResult<Type> {
        self.nested(Self::parse_type_inner)
    }

    fn parse_type_inner(&mut self) -> PResult<Type> {
        let anchor = "gram.type";
        let start = self.span().start;

        if let Some(kw) = self.prefix_type_kw() {
            self.advance();
            let ty = self.parse_type()?;
            let span = Span::new(start, ty.span.end);
            return Ok(Type {
                kind: Box::new(TypeKind::Prefixed { kw, ty }),
                span,
                anchor: "gram.type",
            });
        }

        let kind = match self.tok() {
            // `'!' type` — the error-union constructor. `[gram.amb.bang]`: in
            // *type* position `!` never means "not".
            Some(Tok::Bang) => {
                self.advance();
                TypeKind::ErrorUnion(self.parse_type()?)
            }
            Some(Tok::Star) => {
                self.advance();
                TypeKind::RawPointer(self.parse_type()?)
            }
            Some(Tok::Kw("dyn")) => {
                self.advance();
                TypeKind::Dyn(self.parse_path(anchor)?)
            }
            Some(Tok::Kw("type")) => {
                self.advance();
                TypeKind::TypeOfTypes
            }
            Some(Tok::Kw("region")) => {
                self.advance();
                TypeKind::Region
            }
            Some(Tok::Kw("fn")) => {
                self.advance();
                self.expect(&Tok::LParen, anchor)?;
                let mut params = Vec::new();
                if !self.at(&Tok::RParen) {
                    loop {
                        params.push(self.parse_type()?);
                        if !self.eat(&Tok::Comma) || self.at(&Tok::RParen) {
                            break;
                        }
                    }
                }
                self.expect(&Tok::RParen, anchor)?;
                let ret = if self.eat(&Tok::Arrow) {
                    Some(self.parse_ret_type()?)
                } else {
                    None
                };
                TypeKind::Fn { params, ret }
            }
            Some(Tok::LParen) => {
                self.advance();
                let mut types = Vec::new();
                if !self.at(&Tok::RParen) {
                    loop {
                        types.push(self.parse_type()?);
                        if !self.eat(&Tok::Comma) || self.at(&Tok::RParen) {
                            break;
                        }
                    }
                }
                self.expect(&Tok::RParen, anchor)?;
                TypeKind::Tuple(types)
            }
            Some(Tok::Ident(_)) => {
                let path = self.parse_path(anchor)?;
                let args = if self.at(&Tok::LBracket) {
                    self.parse_type_args()?
                } else {
                    Vec::new()
                };
                TypeKind::Path { path, args }
            }
            _ => return Err(self.unexpected(anchor, "a type")),
        };

        Ok(Type {
            kind: Box::new(kind),
            span: Span::new(start, self.prev_span().end),
            anchor: "gram.type",
        })
    }

    /// `type_args ::= '[' type_arg (',' type_arg)* ','? ']'` where
    /// `type_arg ::= type | expr` — const generics are disambiguated in sema,
    /// so the parse only has to decide which production *reads*.
    fn parse_type_args(&mut self) -> PResult<Vec<TypeArg>> {
        let anchor = "gram.type";
        self.expect(&Tok::LBracket, anchor)?;
        let mut args = Vec::new();
        self.with_struct_lit(true, |p| -> PResult<()> {
            loop {
                p.skip_inserted_terms();
                if p.at(&Tok::RBracket) {
                    break;
                }
                // A literal, or *any* argument carrying a binary arithmetic
                // operator, is a const generic; anything else reads as a type.
                if matches!(
                    p.tok(),
                    Some(
                        Tok::Int(_) | Tok::Float(_) | Tok::StrStart(_) | Tok::Kw("true" | "false")
                    )
                ) || p.type_arg_is_const_expr()
                {
                    args.push(TypeArg::Expr(Box::new(p.parse_expr()?)));
                } else {
                    args.push(TypeArg::Type(p.parse_type()?));
                }
                if !p.eat(&Tok::Comma) {
                    break;
                }
            }
            Ok(())
        })?;
        self.expect(&Tok::RBracket, anchor)?;
        Ok(args)
    }

    /// Does the type argument starting here read as a const-generic
    /// **expression** rather than as a type?
    ///
    /// `[gram.type]` writes `type_arg ::= type | expr` and adds "const
    /// generics; disambiguated in sema" — so the parser's whole job is deciding
    /// which production *reads*, and a leading literal is not enough:
    /// `corpus/comptime/norm_linear.lu` writes `Buf[N + 1]`, whose first token
    /// is an identifier and whose second is an operator no type production has.
    ///
    /// The decision is a bounded scan to this argument's end — the `,` or `]`
    /// at bracket depth zero — looking for a **binary** arithmetic operator. It
    /// is binary exactly when the token before it could end an operand, which
    /// is what keeps `Foo[*u8]`'s prefix `*` a raw-pointer type and makes
    /// `Buf[N *2]`'s a multiplication. No backtracking: is01's parser has none
    /// and this does not introduce any.
    fn type_arg_is_const_expr(&self) -> bool {
        let mut depth = 0i32;
        let mut offset = 0usize;
        // Whether the previous token can end an operand, so the next operator
        // is binary rather than prefix.
        let mut operand = false;
        loop {
            let Some(tok) = self.tok_at(offset) else {
                return false;
            };
            match tok {
                Tok::LBracket | Tok::LParen | Tok::LBrace => {
                    depth += 1;
                    operand = false;
                }
                Tok::RParen | Tok::RBrace => {
                    depth -= 1;
                    operand = true;
                }
                Tok::RBracket => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                    operand = true;
                }
                Tok::Comma if depth == 0 => return false,
                Tok::Ident(_) | Tok::Int(_) | Tok::Float(_) => operand = true,
                Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash | Tok::Percent => {
                    if depth == 0 && operand {
                        return true;
                    }
                    operand = false;
                }
                _ => operand = false,
            }
            offset += 1;
            // A type argument is short; a runaway scan means the token stream
            // ended without a closing bracket, which the caller reports.
            if offset > self.tokens.len() {
                return false;
            }
        }
    }

    // -- patterns ----------------------------------------------------------

    /// `pattern ::= closed_pattern ('|' closed_pattern)*` (`[gram.pat]`).
    fn parse_pattern(&mut self) -> PResult<Pattern> {
        let first = self.parse_closed_pattern()?;
        if !self.at(&Tok::Pipe) {
            return Ok(first);
        }
        let start = first.span.start;
        let mut alts = vec![first];
        while self.eat(&Tok::Pipe) {
            alts.push(self.parse_closed_pattern()?);
        }
        let end = self.prev_span().end;
        Ok(Pattern {
            kind: Box::new(PatKind::Or(alts)),
            span: Span::new(start, end),
            anchor: "gram.pat",
        })
    }

    /// A pattern with no top-level `|` — what every position followed by a `|`
    /// delimiter takes, so the or-bar and the closing delimiter stay
    /// unambiguous (`[gram.pat]`).
    fn parse_closed_pattern(&mut self) -> PResult<Pattern> {
        self.nested(Self::parse_closed_pattern_inner)
    }

    fn parse_closed_pattern_inner(&mut self) -> PResult<Pattern> {
        let anchor = "gram.pat";
        let start = self.span().start;
        let kind = match self.tok() {
            Some(Tok::Underscore) => {
                self.advance();
                PatKind::Wildcard
            }
            Some(Tok::Int(_) | Tok::Float(_) | Tok::StrStart(_) | Tok::Kw("true" | "false")) => {
                PatKind::Literal(Box::new(self.parse_primary()?))
            }
            Some(Tok::Minus) => PatKind::Literal(Box::new(self.parse_literal_only()?)),
            Some(Tok::LParen) => {
                self.advance();
                let mut items = Vec::new();
                if !self.at(&Tok::RParen) {
                    loop {
                        items.push(self.parse_pattern()?);
                        if !self.eat(&Tok::Comma) || self.at(&Tok::RParen) {
                            break;
                        }
                    }
                }
                self.expect(&Tok::RParen, anchor)?;
                PatKind::Tuple(items)
            }
            Some(Tok::Ident(_)) => {
                let path = self.parse_path(anchor)?;
                if self.at(&Tok::LParen) {
                    self.advance();
                    let mut fields = Vec::new();
                    if !self.at(&Tok::RParen) {
                        loop {
                            fields.push(self.parse_pattern()?);
                            if !self.eat(&Tok::Comma) || self.at(&Tok::RParen) {
                                break;
                            }
                        }
                    }
                    self.expect(&Tok::RParen, anchor)?;
                    PatKind::Variant { path, fields }
                } else if path.is_single() && self.at(&Tok::At) {
                    self.advance();
                    let inner = self.parse_closed_pattern()?;
                    PatKind::At {
                        name: path.segments.into_iter().next().expect("single"),
                        pattern: inner,
                    }
                } else if path.is_single() {
                    PatKind::Binding(path.segments.into_iter().next().expect("single"))
                } else {
                    PatKind::Path(path)
                }
            }
            _ => return Err(self.unexpected(anchor, "a pattern")),
        };
        Ok(Pattern {
            kind: Box::new(kind),
            span: Span::new(start, self.prev_span().end),
            anchor,
        })
    }

    // -- blocks & statements ----------------------------------------------

    fn parse_block(&mut self) -> PResult<Block> {
        self.nested(Self::parse_block_inner)
    }

    fn parse_block_inner(&mut self) -> PResult<Block> {
        let anchor = "gram.expr.block";
        let start = self.expect(&Tok::LBrace, anchor)?.start;
        let mut stmts: Vec<Stmt> = Vec::new();
        let mut tail: Option<Box<Expr>> = None;

        loop {
            self.skip_inserted_terms();
            match self.tok() {
                None => break,
                Some(Tok::RBrace) => break,
                Some(Tok::Term { explicit: true }) => return Err(self.empty_statement()),
                Some(Tok::Kw("else")) => return Err(self.orphaned_else()),
                Some(tok) if tok.is_binary_only() => return Err(self.leading_operator()),
                _ => {}
            }
            let stmt = self.parse_stmt()?;
            stmts.push(stmt);
        }

        // Go's rule 2 makes the terminator before `}` optional, so a trailing
        // expression statement *is* the block's value — the shape every corpus
        // function ends with (`tally`, `sum`, `0`).
        if stmts
            .last()
            .is_some_and(|last| matches!(last.kind, StmtKind::Expr(_)) && last.attrs.is_empty())
        {
            let Some(Stmt {
                kind: StmtKind::Expr(expr),
                ..
            }) = stmts.pop()
            else {
                unreachable!("just matched")
            };
            tail = Some(Box::new(expr));
        }

        let end = self.expect(&Tok::RBrace, anchor)?.end;
        Ok(Block {
            stmts,
            tail,
            span: Span::new(start, end),
        })
    }

    #[allow(clippy::too_many_lines)]
    fn parse_stmt(&mut self) -> PResult<Stmt> {
        let attrs = self.parse_attributes()?;
        self.skip_inserted_terms();
        let start = self.span().start;

        // `assume noalias e, e` is a statement, not an expression
        // (`[gram.expr.unsafe]`).
        if self.at_kw("assume") {
            self.advance();
            if !self.eat_ctx("noalias") {
                return Err(self.unexpected("gram.expr.unsafe", "`noalias`"));
            }
            let mut operands = vec![self.parse_expr()?];
            while self.eat(&Tok::Comma) {
                operands.push(self.parse_expr()?);
            }
            if operands.len() < 2 {
                return Err(Diag::new(
                    diag::E_ASSUME_ARITY,
                    Span::new(start, self.prev_span().end),
                    "gram.expr.unsafe",
                    "`assume noalias` compares pointers, so it needs at least two",
                ));
            }
            self.expect_term("gram.expr.unsafe")?;
            return Ok(Stmt {
                attrs,
                kind: StmtKind::AssumeNoalias(operands),
                span: Span::new(start, self.prev_span().end),
                anchor: "gram.expr.unsafe",
            });
        }

        if self.at_kw("defer") || self.at_kw("errdefer") {
            let on_error = self.at_kw("errdefer");
            self.advance();
            let expr = self.parse_expr()?;
            self.expect_term("gram.expr.block")?;
            return Ok(Stmt {
                attrs,
                kind: StmtKind::Defer { on_error, expr },
                span: Span::new(start, self.prev_span().end),
                anchor: "gram.expr.block",
            });
        }

        if matches!(self.tok(), Some(Tok::Kw("let" | "var" | "const"))) {
            let binding = self.parse_binding()?;
            let span = binding.span;
            return Ok(Stmt {
                attrs,
                kind: StmtKind::Binding(Box::new(binding)),
                span,
                anchor: "gram.item.let",
            });
        }

        // `fn` is a closure only when a `(` follows it; with a name it is an
        // item, and items are statements (`stmt_base ::= … | item`).
        let is_item = match self.tok() {
            Some(Tok::Kw("fn")) => matches!(self.tok_at(1), Some(Tok::Ident(_))),
            _ => self.at_item_start(),
        };
        if is_item {
            let item = self.parse_item()?;
            let (span, anchor) = (item.span, item.anchor);
            return Ok(Stmt {
                attrs,
                kind: StmtKind::Item(Box::new(item)),
                span,
                anchor,
            });
        }

        let expr = self.parse_expr()?;

        if let Some(op) = self.assign_op() {
            self.advance();
            let value = self.parse_expr()?;
            self.expect_term("gram.expr.assign")?;
            return Ok(Stmt {
                attrs,
                kind: StmtKind::Assign {
                    place: expr,
                    op,
                    value,
                },
                span: Span::new(start, self.prev_span().end),
                anchor: "gram.expr.assign",
            });
        }

        self.expect_term("gram.expr.block")?;
        Ok(Stmt {
            attrs,
            kind: StmtKind::Expr(expr),
            span: Span::new(start, self.prev_span().end),
            anchor: "gram.expr.block",
        })
    }

    fn assign_op(&self) -> Option<AssignOp> {
        Some(match self.tok()? {
            Tok::Assign => AssignOp::Assign,
            Tok::PlusEq => AssignOp::Add,
            Tok::MinusEq => AssignOp::Sub,
            Tok::StarEq => AssignOp::Mul,
            Tok::SlashEq => AssignOp::Div,
            Tok::PercentEq => AssignOp::Rem,
            Tok::AmpEq => AssignOp::BitAnd,
            Tok::PipeEq => AssignOp::BitOr,
            Tok::CaretEq => AssignOp::BitXor,
            Tok::ShlEq => AssignOp::Shl,
            Tok::ShrEq => AssignOp::Shr,
            _ => return None,
        })
    }

    // -- expressions: the §3.2 climb ---------------------------------------

    /// `expr ::= else_expr | jump_expr` (`[gram.expr.primary]`).
    fn parse_expr(&mut self) -> PResult<Expr> {
        self.nested(Self::parse_expr_inner)
    }

    fn parse_expr_inner(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.flow";
        let start = self.span().start;
        match self.tok() {
            Some(Tok::Kw("return")) => {
                self.advance();
                let value = if self.at_expr_end() {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                Ok(self.expr(ExprKind::Return(value), start, anchor))
            }
            Some(Tok::Kw("break")) => {
                self.advance();
                let value = if self.at_expr_end() {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                Ok(self.expr(ExprKind::Break(value), start, anchor))
            }
            Some(Tok::Kw("continue")) => {
                self.advance();
                Ok(self.expr(ExprKind::Continue, start, anchor))
            }
            _ => self.parse_else_expr(),
        }
    }

    /// Tokens that cannot begin an expression, so `return` and `break` know
    /// their operand is absent.
    fn at_expr_end(&self) -> bool {
        matches!(
            self.tok(),
            None | Some(
                Tok::Term { .. }
                    | Tok::RBrace
                    | Tok::RParen
                    | Tok::RBracket
                    | Tok::Comma
                    | Tok::InterpEnd
                    | Tok::FmtStart
                    | Tok::FatArrow
            )
        )
    }

    fn expr(&self, kind: ExprKind, start: usize, anchor: &'static str) -> Expr {
        Expr {
            kind: Box::new(kind),
            span: Span::new(start, self.prev_span().end),
            anchor,
        }
    }

    /// Tier 15: `else` defaulting, right-associative
    /// (`else_expr ::= range_expr ('else' …)?`).
    fn parse_else_expr(&mut self) -> PResult<Expr> {
        let lhs = self.parse_range_expr()?;
        if !self.at_kw("else") {
            return Ok(lhs);
        }
        let start = lhs.span.start;
        self.advance();
        let handler = if self.at(&Tok::LBrace) {
            ElseHandler::Block(self.with_struct_lit(true, Parser::parse_block)?)
        } else if self.at(&Tok::Pipe) {
            self.advance();
            let pattern = self.parse_closed_pattern()?;
            self.expect(&Tok::Pipe, "gram.expr.primary")?;
            let body = if self.at(&Tok::LBrace) {
                let block = self.with_struct_lit(true, Parser::parse_block)?;
                let span = block.span;
                Expr {
                    kind: Box::new(ExprKind::Block(block)),
                    span,
                    anchor: "gram.expr.block",
                }
            } else {
                self.with_struct_lit(true, Parser::parse_expr)?
            };
            ElseHandler::Handler { pattern, body }
        } else {
            ElseHandler::Expr(self.with_struct_lit(true, Parser::parse_expr)?)
        };
        Ok(self.expr(
            ExprKind::ElseDefault {
                expr: lhs,
                handler: Box::new(handler),
            },
            start,
            "gram.expr.primary",
        ))
    }

    /// Tier 14: ranges, non-associative.
    ///
    /// `range_expr ::= r_end (('..' | '..=') r_end?)? | ('..' | '..=') r_end`
    /// and `r_end ::= or_expr | '^' or_expr` — `^n` is a from-end endpoint
    /// (D25), never an operator, which is why `..^1` needs no `..^` token.
    fn parse_range_expr(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.primary";
        let start = self.span().start;

        if self.at(&Tok::DotDot) || self.at(&Tok::DotDotEq) {
            let inclusive = self.at(&Tok::DotDotEq);
            self.advance();
            let end = self.parse_range_end()?;
            return Ok(self.expr(
                ExprKind::Range {
                    start: None,
                    end: Some(end),
                    inclusive,
                },
                start,
                anchor,
            ));
        }

        let lhs = self.parse_range_end()?;
        if !(self.at(&Tok::DotDot) || self.at(&Tok::DotDotEq)) {
            return Ok(lhs);
        }
        let inclusive = self.at(&Tok::DotDotEq);
        self.advance();
        let end = if self.range_end_follows() {
            Some(self.parse_range_end()?)
        } else {
            None
        };
        if self.at(&Tok::DotDot) || self.at(&Tok::DotDotEq) {
            return Err(self.error(
                diag::E_RANGE_CHAIN,
                "gram.expr.prec",
                "ranges do not chain; tier 14 is non-associative",
            ));
        }
        Ok(self.expr(
            ExprKind::Range {
                start: Some(lhs),
                end,
                inclusive,
            },
            start,
            anchor,
        ))
    }

    fn range_end_follows(&self) -> bool {
        !matches!(
            self.tok(),
            None | Some(
                Tok::Term { .. }
                    | Tok::RBrace
                    | Tok::RParen
                    | Tok::RBracket
                    | Tok::Comma
                    | Tok::InterpEnd
                    | Tok::FmtStart
                    | Tok::FatArrow
                    | Tok::Kw("else")
            )
        )
    }

    fn parse_range_end(&mut self) -> PResult<Expr> {
        if self.at(&Tok::Caret) {
            let start = self.advance().start;
            let inner = self.parse_binary(13)?;
            return Ok(self.expr(ExprKind::FromEnd(inner), start, "gram.expr.primary"));
        }
        self.parse_binary(13)
    }

    /// Tiers 13 down to 5, driven by [`PRECEDENCE`]. Tighter binds first, so
    /// the recursion descends by tier number.
    fn parse_binary(&mut self, tier: u8) -> PResult<Expr> {
        if tier < 5 {
            return self.parse_cast();
        }
        let mut lhs = self.parse_binary(tier - 1)?;
        while let Some(op) = self.binary_op_at_tier(tier) {
            let assoc = tier_assoc(tier);
            self.advance();
            let rhs = self.parse_binary(tier - 1)?;
            let start = lhs.span.start;
            lhs = self.expr(ExprKind::Binary { op, lhs, rhs }, start, "gram.expr.prec");

            if assoc == Assoc::None {
                if self.binary_op_at_tier(tier).is_some() {
                    // `[gram.expr.prec]`: "Comparison operators do not chain
                    // (`a < b < c` is a parse error with a 'did you mean &&'
                    // diagnostic)."
                    return Err(self.error(
                        diag::E_COMPARISON_CHAIN,
                        "gram.expr.prec",
                        "comparisons do not chain; did you mean `&&`?",
                    ));
                }
                break;
            }
        }
        Ok(lhs)
    }

    fn binary_op_at_tier(&self, tier: u8) -> Option<BinOp> {
        let op = match (tier, self.tok()?) {
            (5, Tok::Star) => BinOp::Mul,
            (5, Tok::Slash) => BinOp::Div,
            (5, Tok::Percent) => BinOp::Rem,
            (6, Tok::Plus) => BinOp::Add,
            (6, Tok::Minus) => BinOp::Sub,
            (7, Tok::Shl) => BinOp::Shl,
            (7, Tok::Shr) => BinOp::Shr,
            (8, Tok::Amp) => BinOp::BitAnd,
            (9, Tok::Caret) => BinOp::BitXor,
            (10, Tok::Pipe) => BinOp::BitOr,
            (11, Tok::EqEq) => BinOp::Eq,
            (11, Tok::Ne) => BinOp::Ne,
            (11, Tok::Lt) => BinOp::Lt,
            (11, Tok::Gt) => BinOp::Gt,
            (11, Tok::Le) => BinOp::Le,
            (11, Tok::Ge) => BinOp::Ge,
            (11, Tok::Spaceship) => BinOp::Cmp,
            (12, Tok::AndAnd) => BinOp::And,
            (13, Tok::OrOr) => BinOp::Or,
            _ => return None,
        };
        Some(op)
    }

    /// Tier 4: `as` type cast, left-associative.
    fn parse_cast(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_prefix()?;
        while self.at_kw("as") {
            self.advance();
            let ty = self.parse_type()?;
            let start = expr.span.start;
            expr = self.expr(ExprKind::Cast { expr, ty }, start, "gram.expr.prec");
        }
        Ok(expr)
    }

    /// Tier 3: `!` `-` `&` `&mut` `*` `move` `copy` `shared`.
    ///
    /// Only the *recursive* call is railed: a prefix-free expression must not
    /// spend a nesting level merely for passing through this tier.
    fn parse_prefix(&mut self) -> PResult<Expr> {
        let start = self.span().start;
        let op = match self.tok() {
            Some(Tok::Bang) => UnOp::Not,
            Some(Tok::Minus) => UnOp::Neg,
            Some(Tok::Star) => UnOp::Deref,
            Some(Tok::Amp) => {
                if matches!(self.tok_at(1), Some(Tok::Kw("mut"))) {
                    self.advance();
                    self.advance();
                    let operand = self.nested(Self::parse_prefix)?;
                    return Ok(self.expr(
                        ExprKind::Unary {
                            op: UnOp::BorrowMut,
                            operand,
                        },
                        start,
                        "gram.expr.prec",
                    ));
                }
                UnOp::Borrow
            }
            Some(Tok::Kw("move")) => UnOp::Move,
            Some(Tok::Kw("copy")) => UnOp::Copy,
            // Prefix `shared` creates a Tier-2 RC cell from a value (s13
            // finding): `let a = shared (Cfg { limit: 7 })`.
            Some(Tok::Kw("shared")) => UnOp::Shared,
            _ => return self.parse_postfix(),
        };
        self.advance();
        let operand = self.nested(Self::parse_prefix)?;
        Ok(self.expr(ExprKind::Unary { op, operand }, start, "gram.expr.prec"))
    }

    /// Tier 2, left-associative:
    /// `postfix_expr ::= primary (call_args | index_args | '.' member | '?')*`.
    fn parse_postfix(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_primary()?;
        let start = expr.span.start;
        loop {
            match self.tok() {
                Some(Tok::LParen) => {
                    self.advance();
                    let args = self.parse_call_args()?;
                    expr = self.expr(
                        ExprKind::Call { callee: expr, args },
                        start,
                        "gram.expr.primary",
                    );
                }
                Some(Tok::LBracket) => {
                    self.advance();
                    let args = self.parse_index_args()?;
                    expr = self.expr(
                        ExprKind::BracketApply { base: expr, args },
                        start,
                        "gram.amb.brackets",
                    );
                }
                Some(Tok::Dot) => {
                    self.advance();
                    let member = self.parse_member()?;
                    expr = self.expr(
                        ExprKind::Member { base: expr, member },
                        start,
                        "gram.expr.primary",
                    );
                }
                Some(Tok::Question) => {
                    self.advance();
                    expr = self.expr(ExprKind::Try(expr), start, "gram.expr.primary");
                }
                _ => break,
            }
        }

        // `struct_lit ::= path '{' …`. The postfix chain has already stopped at
        // the `{`, so this is where the literal is recognised — and where
        // `[gram.amb.structlit]`'s restriction bites.
        if self.at(&Tok::LBrace)
            && let Some(path) = path_of(&expr)
        {
            if self.no_struct_lit {
                let _ = &path;
                if let Some(diag) = self.struct_lit_in_condition() {
                    return Err(diag);
                }
            } else {
                return self.parse_struct_lit_body(path, start);
            }
        }
        Ok(expr)
    }

    /// `member ::= IDENT | INT | reserved_kw` — member position is
    /// keyword-transparent, so `.take(n)` and `s.spawn(…)` parse.
    fn parse_member(&mut self) -> PResult<Member> {
        match self.tok() {
            Some(Tok::Ident(name)) => {
                let span = self.advance();
                Ok(Member::Named(Ident {
                    name: name.clone(),
                    span,
                }))
            }
            Some(Tok::Kw(kw)) => {
                let name = (*kw).to_owned();
                let span = self.advance();
                Ok(Member::Named(Ident { name, span }))
            }
            Some(Tok::Int(text)) => {
                let value = text.replace('_', "").parse::<u64>().unwrap_or(0);
                let span = self.advance();
                Ok(Member::Index(value, span))
            }
            _ => Err(self.unexpected("gram.expr.primary", "a member name")),
        }
    }

    /// `call_args ::= '(' (call_arg (',' call_arg)* ','?)? ')'` with
    /// `call_arg ::= ('mut' | 'take')? expr` — call-site modes are grammar (X1).
    fn parse_call_args(&mut self) -> PResult<Vec<Arg>> {
        let anchor = "gram.expr.primary";
        let mut args = Vec::new();
        self.with_struct_lit(true, |p| -> PResult<()> {
            loop {
                p.skip_inserted_terms();
                if p.at(&Tok::RParen) {
                    break;
                }
                let start = p.span().start;
                let mode = p.parse_param_mode();
                let expr = p.parse_expr()?;
                args.push(Arg {
                    mode,
                    expr,
                    span: Span::new(start, p.prev_span().end),
                });
                p.skip_inserted_terms();
                if !p.eat(&Tok::Comma) {
                    break;
                }
            }
            Ok(())
        })?;
        self.skip_inserted_terms();
        self.expect(&Tok::RParen, anchor)?;
        Ok(args)
    }

    /// `index_arg ::= call_arg | prefix_type_kw type | 'region'`.
    ///
    /// The two type forms exist because no *expression* can spell them —
    /// `List[handle Node]()`, `channel[region]` — and keeping them inside the
    /// one `index_args` production is what makes `e[…]` a single postfix shape
    /// (`[gram.amb.brackets]`).
    fn parse_index_args(&mut self) -> PResult<Vec<IndexArg>> {
        let anchor = "gram.amb.brackets";
        let mut args = Vec::new();
        self.with_struct_lit(true, |p| -> PResult<()> {
            loop {
                p.skip_inserted_terms();
                if p.at(&Tok::RBracket) {
                    break;
                }
                if p.prefix_type_kw().is_some() || p.at_kw("region") {
                    args.push(IndexArg::Type(p.parse_type()?));
                } else {
                    let start = p.span().start;
                    let mode = p.parse_param_mode();
                    let expr = p.parse_expr()?;
                    args.push(IndexArg::Value(Arg {
                        mode,
                        expr,
                        span: Span::new(start, p.prev_span().end),
                    }));
                }
                p.skip_inserted_terms();
                if !p.eat(&Tok::Comma) {
                    break;
                }
            }
            Ok(())
        })?;
        self.skip_inserted_terms();
        self.expect(&Tok::RBracket, anchor)?;
        Ok(args)
    }

    /// E0006 — `[gram.amb.structlit]`: "No struct-literal expressions in
    /// condition/scrutinee position; `if x == (Point { x: 0 }) { … }` requires
    /// parens."
    ///
    /// Only fires when the brace body *cannot* be a block: `IDENT :` or
    /// `IDENT ,`. The shorthand `{ x }` is ambiguous with a block and §3.4
    /// resolves it in the block's favour, so it is left alone (see [`CHOICES`]).
    ///
    /// The **primary span is the opening `{`**, per the amended §9 reservation
    /// ("E0006 (struct literal in condition; primary span = the opening `{`)").
    /// is01 spanned the whole `Point { x: 0 }` and reported the ambiguity
    /// upward; the amendment picked the brace, because the brace is the token
    /// whose reading is contested — the path before it is a perfectly good
    /// expression under either reading.
    fn struct_lit_in_condition(&self) -> Option<Diag> {
        let looks_like_a_literal = matches!(self.tok_at(1), Some(Tok::Ident(_)))
            && matches!(self.tok_at(2), Some(Tok::Colon | Tok::Comma));
        if !looks_like_a_literal {
            return None;
        }
        Some(Diag::new(
            diag::E_STRUCT_LIT_IN_COND,
            self.span(),
            "gram.amb.structlit",
            "a struct literal here is read as the block that follows the condition; \
             wrap it in parentheses",
        ))
    }

    fn parse_struct_lit_body(&mut self, path: Path, start: usize) -> PResult<Expr> {
        let anchor = "gram.expr.primary";
        self.expect(&Tok::LBrace, anchor)?;
        let mut fields = Vec::new();
        self.with_struct_lit(true, |p| -> PResult<()> {
            loop {
                p.skip_inserted_terms();
                if p.at(&Tok::RBrace) {
                    break;
                }
                let name = p.expect_ident(anchor)?;
                let fstart = name.span.start;
                // `field_init ::= IDENT ':' expr | IDENT` — the shorthand binds
                // the field from the identifier.
                let value = if p.eat(&Tok::Colon) {
                    Some(p.parse_expr()?)
                } else {
                    None
                };
                fields.push(FieldInit {
                    name,
                    value,
                    span: Span::new(fstart, p.prev_span().end),
                });
                p.skip_inserted_terms();
                if !p.eat(&Tok::Comma) {
                    break;
                }
            }
            Ok(())
        })?;
        self.skip_inserted_terms();
        self.expect(&Tok::RBrace, anchor)?;
        Ok(self.expr(ExprKind::StructLit { path, fields }, start, anchor))
    }

    // -- primaries ---------------------------------------------------------

    #[allow(clippy::too_many_lines)]
    fn parse_primary(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.primary";
        let start = self.span().start;
        match self.tok() {
            Some(Tok::Int(text)) => {
                let text = text.clone();
                self.advance();
                Ok(self.expr(ExprKind::Int(text), start, anchor))
            }
            Some(Tok::Float(text)) => {
                let text = text.clone();
                self.advance();
                Ok(self.expr(ExprKind::Float(text), start, anchor))
            }
            Some(Tok::Kw("true")) => {
                self.advance();
                Ok(self.expr(ExprKind::Bool(true), start, anchor))
            }
            Some(Tok::Kw("false")) => {
                self.advance();
                Ok(self.expr(ExprKind::Bool(false), start, anchor))
            }
            Some(Tok::StrStart(_)) => {
                let literal = self.parse_string_literal(anchor)?;
                Ok(self.expr(ExprKind::Str(Box::new(literal)), start, "gram.lex.str"))
            }
            Some(Tok::Underscore) => {
                self.advance();
                Ok(self.expr(ExprKind::Wildcard, start, "gram.lex.ident"))
            }
            Some(Tok::Ident(_)) => {
                let path = self.parse_path(anchor)?;
                Ok(self.expr(ExprKind::Path(path), start, anchor))
            }
            Some(Tok::LParen) => self.parse_paren_or_tuple(),
            Some(Tok::LBrace) => {
                let block = self.with_struct_lit(true, Parser::parse_block)?;
                Ok(self.expr(ExprKind::Block(block), start, "gram.expr.block"))
            }
            Some(Tok::Kw("if")) => self.parse_if(),
            Some(Tok::Kw("match")) => self.parse_match(),
            Some(Tok::Kw("for")) => self.parse_for(),
            Some(Tok::Kw("while")) => self.parse_while(),
            Some(Tok::Kw("loop")) => {
                self.advance();
                let body = self.with_struct_lit(true, Parser::parse_block)?;
                Ok(self.expr(ExprKind::Loop { body }, start, "gram.expr.flow"))
            }
            Some(Tok::Kw("fn")) => self.parse_closure(),
            Some(Tok::Kw("region")) => self.parse_region(),
            Some(Tok::Kw("in")) => {
                self.advance();
                let region = self.with_struct_lit(false, Parser::parse_expr)?;
                let body = self.with_struct_lit(true, Parser::parse_block)?;
                Ok(self.expr(ExprKind::In { region, body }, start, "gram.expr.region"))
            }
            Some(Tok::Kw("freeze")) => {
                self.advance();
                let operand = self.parse_prefix()?;
                Ok(self.expr(ExprKind::Freeze(operand), start, "gram.expr.region"))
            }
            Some(Tok::Kw("scope")) => {
                self.advance();
                let name = if matches!(self.tok(), Some(Tok::Ident(_))) {
                    Some(self.expect_ident("gram.expr.conc")?)
                } else {
                    None
                };
                let body = self.with_struct_lit(true, Parser::parse_block)?;
                Ok(self.expr(ExprKind::Scope { name, body }, start, "gram.expr.conc"))
            }
            Some(Tok::Kw("spawn")) => {
                self.advance();
                self.expect_kw("proc", "gram.expr.conc")?;
                let path = self.parse_path("gram.expr.conc")?;
                self.expect(&Tok::LParen, "gram.expr.conc")?;
                let args = self.parse_call_args()?;
                Ok(self.expr(ExprKind::SpawnProc { path, args }, start, "gram.expr.conc"))
            }
            Some(Tok::Kw("select")) => self.parse_select(),
            Some(Tok::Kw("when")) => self.parse_when(),
            Some(Tok::Kw("unsafe")) => self.parse_unsafe(),
            Some(Tok::Kw("asm")) => self.parse_asm(),
            Some(Tok::Kw("borrow")) => {
                self.advance();
                let place = self.parse_prefix()?;
                if !self.eat_ctx("from") {
                    return Err(self.unexpected("gram.expr.unsafe", "`from`"));
                }
                let from = self.parse_prefix()?;
                Ok(self.expr(ExprKind::Borrow { place, from }, start, "gram.expr.unsafe"))
            }
            _ => Err(self.unexpected(anchor, "an expression")),
        }
    }

    /// `(e)` grouping; `(a, b)` tuple; `(a,)` one-tuple — comma decides.
    fn parse_paren_or_tuple(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.primary";
        let start = self.expect(&Tok::LParen, anchor)?.start;
        let mut items = Vec::new();
        let mut saw_comma = false;
        self.with_struct_lit(true, |p| -> PResult<()> {
            loop {
                p.skip_inserted_terms();
                if p.at(&Tok::RParen) {
                    break;
                }
                items.push(p.parse_expr()?);
                p.skip_inserted_terms();
                if p.eat(&Tok::Comma) {
                    saw_comma = true;
                } else {
                    break;
                }
            }
            Ok(())
        })?;
        self.skip_inserted_terms();
        self.expect(&Tok::RParen, anchor)?;
        let kind = if items.len() == 1 && !saw_comma {
            ExprKind::Group(items.into_iter().next().expect("one"))
        } else {
            ExprKind::Tuple(items)
        };
        Ok(self.expr(kind, start, anchor))
    }

    fn parse_if(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.flow";
        let start = self.expect_kw("if", anchor)?.start;
        // "The condition of `if`/`while` and the scrutinee of `match`/`for` use
        // no-struct-literal expression mode."
        let cond = self.with_struct_lit(false, Parser::parse_expr)?;
        let then = self.with_struct_lit(true, Parser::parse_block)?;
        let otherwise = if self.at_kw("else") {
            self.advance();
            if self.at_kw("if") {
                Some(self.parse_if()?)
            } else {
                let block_start = self.span().start;
                let block = self.with_struct_lit(true, Parser::parse_block)?;
                Some(self.expr(ExprKind::Block(block), block_start, "gram.expr.block"))
            }
        } else {
            None
        };
        Ok(self.expr(
            ExprKind::If {
                cond,
                then,
                otherwise,
            },
            start,
            anchor,
        ))
    }

    fn parse_while(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.flow";
        let start = self.expect_kw("while", anchor)?.start;
        let cond = self.with_struct_lit(false, Parser::parse_expr)?;
        let body = self.with_struct_lit(true, Parser::parse_block)?;
        Ok(self.expr(ExprKind::While { cond, body }, start, anchor))
    }

    fn parse_for(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.flow";
        let start = self.expect_kw("for", anchor)?.start;
        let pattern = self.parse_pattern()?;
        self.expect_kw("in", anchor)?;
        let iter = self.with_struct_lit(false, Parser::parse_expr)?;
        let body = self.with_struct_lit(true, Parser::parse_block)?;
        Ok(self.expr(
            ExprKind::For {
                pattern,
                iter,
                body,
            },
            start,
            anchor,
        ))
    }

    fn parse_match(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.flow";
        let start = self.expect_kw("match", anchor)?.start;
        let scrutinee = self.with_struct_lit(false, Parser::parse_expr)?;
        self.expect(&Tok::LBrace, anchor)?;
        let mut arms: Vec<MatchArm> = Vec::new();
        self.with_struct_lit(true, |p| -> PResult<()> {
            loop {
                p.skip_inserted_terms();
                if p.at(&Tok::RBrace) || p.tok().is_none() {
                    break;
                }
                let astart = p.span().start;
                let pattern = p.parse_pattern()?;
                let guard = if p.eat_kw("if") {
                    Some(p.with_struct_lit(false, Parser::parse_expr)?)
                } else {
                    None
                };
                p.expect(&Tok::FatArrow, anchor)?;
                let (body, block_bodied) = p.parse_arm_body()?;
                arms.push(MatchArm {
                    pattern,
                    guard,
                    body,
                    block_bodied,
                    span: Span::new(astart, p.prev_span().end),
                });
                if !p.consume_arm_separator(block_bodied, anchor)? {
                    break;
                }
            }
            Ok(())
        })?;
        self.skip_inserted_terms();
        self.expect(&Tok::RBrace, anchor)?;
        Ok(self.expr(ExprKind::Match { scrutinee, arms }, start, anchor))
    }

    fn parse_arm_body(&mut self) -> PResult<(Expr, bool)> {
        if self.at(&Tok::LBrace) {
            let start = self.span().start;
            let block = self.with_struct_lit(true, Parser::parse_block)?;
            Ok((
                self.expr(ExprKind::Block(block), start, "gram.expr.block"),
                true,
            ))
        } else {
            Ok((self.parse_expr()?, false))
        }
    }

    /// `arm_sep ::= ',' | TERM`, narrowed by §3.4's prose: the comma is
    /// **required** after an expression-bodied arm that is followed by another
    /// arm, and **optional** after a block-bodied arm (the terminator inserted
    /// after the arm's `}` is the separator).
    ///
    /// Returns whether another arm may follow.
    fn consume_arm_separator(&mut self, block_bodied: bool, anchor: &'static str) -> PResult<bool> {
        if self.eat(&Tok::Comma) {
            return Ok(true);
        }
        if block_bodied {
            if matches!(self.tok(), Some(Tok::Term { .. })) {
                self.skip_terms();
                return Ok(true);
            }
            return Ok(false);
        }
        // Expression-bodied: a bare terminator is not a legal separator when
        // another arm follows.
        if matches!(self.tok(), Some(Tok::Term { .. })) {
            let save = self.pos;
            self.skip_terms();
            if self.at(&Tok::RBrace) || self.tok().is_none() {
                return Ok(false);
            }
            self.pos = save;
            return Err(self.unexpected(anchor, "`,` after an expression-bodied arm"));
        }
        Ok(false)
    }

    fn parse_closure(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.closure";
        let start = self.expect_kw("fn", anchor)?.start;
        self.expect(&Tok::LParen, anchor)?;
        let mut params = Vec::new();
        self.with_struct_lit(true, |p| -> PResult<()> {
            loop {
                p.skip_inserted_terms();
                if p.at(&Tok::RParen) {
                    break;
                }
                let pstart = p.span().start;
                let mode = p.parse_param_mode();
                let name = p.expect_ident(anchor)?;
                let ty = if p.eat(&Tok::Colon) {
                    Some(p.parse_type()?)
                } else {
                    None
                };
                params.push(ClosureParam {
                    mode,
                    name,
                    ty,
                    span: Span::new(pstart, p.prev_span().end),
                });
                if !p.eat(&Tok::Comma) {
                    break;
                }
            }
            Ok(())
        })?;
        self.expect(&Tok::RParen, anchor)?;

        // `[gram.amb.closure]`: an expression body is one `else_expr`. It
        // extends maximally rightward and is terminated only by a token the
        // expression grammar cannot consume (`,` `)` `]` `}` TERM) — which is
        // why `sorted_by(fn(a, b) b.1 <=> a.1)` needs no braces and why a
        // closure passed as a non-final argument cannot swallow the comma.
        let (body, block_bodied) = if self.at(&Tok::LBrace) {
            let bstart = self.span().start;
            let block = self.with_struct_lit(true, Parser::parse_block)?;
            (
                self.expr(ExprKind::Block(block), bstart, "gram.expr.block"),
                true,
            )
        } else {
            (self.with_struct_lit(true, Parser::parse_else_expr)?, false)
        };
        Ok(self.expr(
            ExprKind::Closure {
                params,
                body,
                block_bodied,
            },
            start,
            anchor,
        ))
    }

    /// `[gram.expr.region]`, both locked forms (X4).
    fn parse_region(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.region";
        let start = self.expect_kw("region", anchor)?.start;

        // Value form: `region()`, `region(rc)`, `region(pool(Node))`.
        if self.at(&Tok::LParen) {
            self.advance();
            let strategy = if self.at(&Tok::RParen) {
                None
            } else {
                Some(self.parse_region_strategy()?)
            };
            self.expect(&Tok::RParen, anchor)?;
            return Ok(self.expr(ExprKind::RegionValue { strategy }, start, anchor));
        }

        // Sugar: `region tmp { … }`, `region r: pool(Node) { … }`, and the
        // anonymous `region { … }` that `freeze region { … }` builds on.
        let name = if matches!(self.tok(), Some(Tok::Ident(_))) {
            Some(self.expect_ident(anchor)?)
        } else {
            None
        };
        let strategy = if self.eat(&Tok::Colon) {
            Some(self.parse_region_strategy()?)
        } else {
            None
        };
        let body = self.with_struct_lit(true, Parser::parse_block)?;
        Ok(self.expr(
            ExprKind::RegionSugar {
                name,
                strategy,
                body,
            },
            start,
            anchor,
        ))
    }

    /// `region_strategy ::= 'rc' | 'pool' '(' type ')'` — both contextual.
    fn parse_region_strategy(&mut self) -> PResult<RegionStrategy> {
        let anchor = "gram.expr.region";
        if self.at_ctx("rc") {
            return Ok(RegionStrategy::Rc(self.advance()));
        }
        if self.at_ctx("pool") {
            let start = self.advance().start;
            self.expect(&Tok::LParen, anchor)?;
            let ty = self.parse_type()?;
            let end = self.expect(&Tok::RParen, anchor)?.end;
            return Ok(RegionStrategy::Pool {
                ty,
                span: Span::new(start, end),
            });
        }
        Err(self.unexpected(anchor, "`rc` or `pool(T)`"))
    }

    fn parse_select(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.conc";
        let start = self.expect_kw("select", anchor)?.start;
        self.expect(&Tok::LBrace, anchor)?;
        let mut arms: Vec<SelectArm> = Vec::new();
        self.with_struct_lit(true, |p| -> PResult<()> {
            loop {
                p.skip_inserted_terms();
                if p.at(&Tok::RBrace) || p.tok().is_none() {
                    break;
                }
                let astart = p.span().start;
                // `timeout` is contextual and only meaningful here; a pattern
                // could never be `timeout(1.s)` because `1.s` is not a pattern.
                let kind = if p.at_ctx("timeout") && p.tok_at(1) == Some(&Tok::LParen) {
                    p.advance();
                    p.advance();
                    let deadline = p.parse_expr()?;
                    p.expect(&Tok::RParen, anchor)?;
                    SelectArmKind::Timeout(deadline)
                } else {
                    let pattern = p.parse_pattern()?;
                    if !p.eat_ctx("from") {
                        return Err(p.unexpected(anchor, "`from`"));
                    }
                    let channel = p.parse_expr()?;
                    SelectArmKind::Recv { pattern, channel }
                };
                p.expect(&Tok::FatArrow, anchor)?;
                let (body, block_bodied) = p.parse_arm_body()?;
                arms.push(SelectArm {
                    kind,
                    body,
                    block_bodied,
                    span: Span::new(astart, p.prev_span().end),
                });
                if !p.consume_arm_separator(block_bodied, anchor)? {
                    break;
                }
            }
            Ok(())
        })?;
        self.skip_inserted_terms();
        self.expect(&Tok::RBrace, anchor)?;
        Ok(self.expr(ExprKind::Select { arms }, start, anchor))
    }

    /// `when_expr ::= 'when' '(' expr (',' expr)+ ','? ')' block` — the `+` is
    /// the point: one operand is just a method on the sync type.
    fn parse_when(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.conc";
        let start = self.expect_kw("when", anchor)?.start;
        self.expect(&Tok::LParen, anchor)?;
        let mut operands = Vec::new();
        self.with_struct_lit(true, |p| -> PResult<()> {
            loop {
                p.skip_inserted_terms();
                if p.at(&Tok::RParen) {
                    break;
                }
                operands.push(p.parse_expr()?);
                if !p.eat(&Tok::Comma) {
                    break;
                }
            }
            Ok(())
        })?;
        let paren_end = self.expect(&Tok::RParen, anchor)?.end;
        if operands.len() < 2 {
            return Err(Diag::new(
                diag::E_WHEN_ARITY,
                Span::new(start, paren_end),
                anchor,
                "`when` acquires a set, so it needs at least two operands; for one, call the \
                 method on the sync type",
            ));
        }
        let body = self.with_struct_lit(true, Parser::parse_block)?;
        Ok(self.expr(ExprKind::When { operands, body }, start, anchor))
    }

    fn parse_unsafe(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.unsafe";
        let start = self.expect_kw("unsafe", anchor)?.start;

        // `unsafe c [captures] { … }` — inline C. The body is opaque token text
        // to wolf (brace-balanced scan); c10 owns its meaning.
        if self.at_ctx("c") {
            self.advance();
            let mut captures = Vec::new();
            if self.at(&Tok::LBracket) {
                self.advance();
                loop {
                    if self.at(&Tok::RBracket) {
                        break;
                    }
                    captures.push(self.expect_ident(anchor)?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RBracket, anchor)?;
            }
            let open = self.expect(&Tok::LBrace, anchor)?;
            let mut depth = 1usize;
            while depth > 0 {
                match self.tok() {
                    None => return Err(self.unexpected(anchor, "the `}` closing the C block")),
                    Some(Tok::LBrace) => depth += 1,
                    Some(Tok::RBrace) => depth -= 1,
                    _ => {}
                }
                self.advance();
            }
            let body_span = Span::new(open.end, self.prev_span().start);
            return Ok(self.expr(
                ExprKind::UnsafeC {
                    captures,
                    body_span,
                },
                start,
                anchor,
            ));
        }

        let body = self.with_struct_lit(true, Parser::parse_block)?;
        Ok(self.expr(ExprKind::Unsafe { body }, start, anchor))
    }

    /// `asm_expr ::= 'asm' '{' STRING (',' asm_operand)* ','? '}'`.
    fn parse_asm(&mut self) -> PResult<Expr> {
        let anchor = "gram.expr.unsafe";
        let start = self.expect_kw("asm", anchor)?.start;
        self.expect(&Tok::LBrace, anchor)?;
        self.skip_inserted_terms();
        let template = self.parse_string_literal(anchor)?;
        let mut operands = Vec::new();
        loop {
            self.skip_inserted_terms();
            if !self.eat(&Tok::Comma) {
                break;
            }
            self.skip_inserted_terms();
            if self.at(&Tok::RBrace) {
                break;
            }
            operands.push(self.parse_asm_operand()?);
        }
        self.skip_inserted_terms();
        self.expect(&Tok::RBrace, anchor)?;
        Ok(self.expr(
            ExprKind::Asm {
                template: Box::new(template),
                operands,
            },
            start,
            anchor,
        ))
    }

    fn parse_asm_operand(&mut self) -> PResult<AsmOperand> {
        let anchor = "gram.expr.unsafe";
        let start = self.span().start;
        // `IDENT '=' asm_dir '(' asm_constraint ')' expr` or the unnamed form.
        let name =
            if matches!(self.tok(), Some(Tok::Ident(_))) && self.tok_at(1) == Some(&Tok::Assign) {
                let ident = self.expect_ident(anchor)?;
                self.advance();
                Some(ident)
            } else {
                None
            };
        let direction = if self.eat_kw("in") {
            AsmDir::In
        } else if self.eat_ctx("out") {
            AsmDir::Out
        } else if self.eat_ctx("inout") {
            AsmDir::InOut
        } else if self.eat_ctx("lateout") {
            AsmDir::LateOut
        } else {
            return Err(self.unexpected(anchor, "`in`, `out`, `inout` or `lateout`"));
        };
        self.expect(&Tok::LParen, anchor)?;
        let constraint = self.expect_ident(anchor)?;
        self.expect(&Tok::RParen, anchor)?;
        let value = self.parse_expr()?;
        Ok(AsmOperand {
            name,
            direction,
            constraint,
            value,
            span: Span::new(start, self.prev_span().end),
        })
    }

    // -- string literals ---------------------------------------------------

    /// Reassembles the lexer's mode-stack token run into one literal, parsing
    /// each interpolation as a full expression.
    fn parse_string_literal(&mut self, anchor: &'static str) -> PResult<StrLit> {
        let Some(Tok::StrStart(kind)) = self.tok() else {
            return Err(self.unexpected(anchor, "a string literal"));
        };
        let kind = kind.clone();
        let start = self.advance().start;
        let mut parts = Vec::new();

        loop {
            match self.tok() {
                Some(Tok::StrText(text)) => {
                    parts.push(StrPart::Text(text.clone()));
                    self.advance();
                }
                Some(Tok::InterpStart) => parts.push(StrPart::Interp(self.parse_interpolation()?)),
                Some(Tok::StrEnd) => {
                    let end = self.advance().end;
                    return Ok(StrLit {
                        kind,
                        parts,
                        span: Span::new(start, end),
                    });
                }
                _ => return Err(self.unexpected("gram.lex.str", "the rest of the string literal")),
            }
        }
    }

    fn parse_interpolation(&mut self) -> PResult<Interpolation> {
        let start = self.expect(&Tok::InterpStart, "gram.lex.str")?.start;
        let expr = self.with_struct_lit(true, Parser::parse_expr)?;
        // `[gram.amb.fmtcolon]`: the lexer already decided which `:` was
        // top-level, so the parser only has to read the decision.
        let format = if self.at(&Tok::FmtStart) {
            self.advance();
            let mut fmt = Vec::new();
            loop {
                match self.tok() {
                    Some(Tok::FmtText(text)) => {
                        fmt.push(FmtPart::Text(text.clone()));
                        self.advance();
                    }
                    Some(Tok::InterpStart) => {
                        self.advance();
                        let inner = self.with_struct_lit(true, Parser::parse_expr)?;
                        self.expect(&Tok::InterpEnd, "gram.lex.str")?;
                        fmt.push(FmtPart::Interp(Box::new(inner)));
                    }
                    _ => break,
                }
            }
            Some(fmt)
        } else {
            None
        };
        let end = self.expect(&Tok::InterpEnd, "gram.lex.str")?.end;
        Ok(Interpolation {
            expr: Box::new(expr),
            format,
            span: Span::new(start, end),
        })
    }
}

/// The associativity of a tier, read out of [`PRECEDENCE`].
fn tier_assoc(tier: u8) -> Assoc {
    PRECEDENCE
        .iter()
        .find(|(t, _, _)| *t == tier)
        .map_or(Assoc::Left, |(_, _, assoc)| *assoc)
}

/// The path an expression spells, if it spells one — the test
/// `struct_lit ::= path '{'` needs.
fn path_of(expr: &Expr) -> Option<Path> {
    match expr.kind.as_ref() {
        ExprKind::Path(path) => Some(path.clone()),
        _ => None,
    }
}

/// The number of associativity-bearing tiers §3.2's table defines. Exposed so
/// a spec-extraction test can check the transcription is complete rather than
/// merely consistent.
pub const PRECEDENCE_TIERS: usize = 13;

// ---------------------------------------------------------------------------
// The production trace
// ---------------------------------------------------------------------------

/// A production trace of a parsed unit — **our** dump format.
///
/// One node per line: `span  clause-anchor  node`, indented by tree depth. The
/// clause anchor is the point: every line names the `[gram.…]` production that
/// built it, so a divergence report can be read against the spec document
/// without a decoder ring, and is02 inherits the citation for free.
#[must_use]
pub fn trace(unit: &Unit) -> String {
    let mut out = String::new();
    line(&mut out, 0, unit.span, "gram.item.unit", "unit");
    for item in &unit.items {
        trace_item(&mut out, 1, item);
    }
    out
}

fn line(out: &mut String, depth: usize, span: Span, anchor: &str, label: &str) {
    out.push_str(&format!(
        "{:>5}..{:<5} [{anchor}] {}{label}\n",
        span.start,
        span.end,
        "  ".repeat(depth)
    ));
}

fn trace_item(out: &mut String, depth: usize, item: &Item) {
    for attr in &item.attrs {
        line(out, depth, attr.span, "gram.item.attr", "attribute");
    }
    let label = match &item.kind {
        ItemKind::Fn(decl) => format!("fn {}", decl.name.name),
        ItemKind::Binding(binding) => format!(
            "{} binding",
            match binding.kind {
                BindingKind::Let => "let",
                BindingKind::Var => "var",
                BindingKind::Const => "const",
            }
        ),
        ItemKind::TypeAlias(alias) => format!("type {}", alias.name.name),
        ItemKind::Struct(def) => format!(
            "struct {} ({} field(s))",
            def.name.as_ref().map_or("<anon>", |n| n.name.as_str()),
            def.fields.len()
        ),
        ItemKind::Enum(def) => format!(
            "enum {} ({} variant(s))",
            def.name.as_ref().map_or("<anon>", |n| n.name.as_str()),
            def.variants.len()
        ),
        ItemKind::Trait(def) => format!("trait {}", def.name.name),
        ItemKind::Impl(_) => "impl".to_owned(),
        ItemKind::Use(decl) => format!("use {}", join_path(&decl.path)),
        ItemKind::ImportC(_) => "import c".to_owned(),
    };
    line(out, depth, item.span, item.anchor, &label);

    match &item.kind {
        ItemKind::Fn(decl) => {
            if let Some(body) = &decl.body {
                trace_block(out, depth + 1, body);
            }
        }
        ItemKind::Binding(binding) => trace_expr(out, depth + 1, &binding.value),
        ItemKind::Trait(def) => {
            for member in &def.members {
                trace_item(out, depth + 1, member);
            }
        }
        ItemKind::Impl(def) => {
            for member in &def.members {
                trace_item(out, depth + 1, member);
            }
        }
        _ => {}
    }
}

fn trace_block(out: &mut String, depth: usize, block: &Block) {
    line(out, depth, block.span, "gram.expr.block", "block");
    for stmt in &block.stmts {
        trace_stmt(out, depth + 1, stmt);
    }
    if let Some(tail) = &block.tail {
        line(out, depth + 1, tail.span, "gram.expr.block", "tail value");
        trace_expr(out, depth + 2, tail);
    }
}

fn trace_stmt(out: &mut String, depth: usize, stmt: &Stmt) {
    let label = match &stmt.kind {
        StmtKind::Binding(_) => "binding",
        StmtKind::Assign { .. } => "assign",
        StmtKind::Defer { on_error: true, .. } => "errdefer",
        StmtKind::Defer { .. } => "defer",
        StmtKind::AssumeNoalias(_) => "assume noalias",
        StmtKind::Expr(_) => "expr statement",
        StmtKind::Item(_) => "nested item",
    };
    line(out, depth, stmt.span, stmt.anchor, label);
    match &stmt.kind {
        StmtKind::Binding(binding) => trace_expr(out, depth + 1, &binding.value),
        StmtKind::Assign { place, value, .. } => {
            trace_expr(out, depth + 1, place);
            trace_expr(out, depth + 1, value);
        }
        StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => trace_expr(out, depth + 1, expr),
        StmtKind::AssumeNoalias(operands) => {
            for operand in operands {
                trace_expr(out, depth + 1, operand);
            }
        }
        StmtKind::Item(item) => trace_item(out, depth + 1, item),
    }
}

#[allow(clippy::too_many_lines)]
fn trace_expr(out: &mut String, depth: usize, expr: &Expr) {
    let label = match expr.kind.as_ref() {
        ExprKind::Int(text) => format!("int {text}"),
        ExprKind::Float(text) => format!("float {text}"),
        ExprKind::Bool(value) => format!("bool {value}"),
        ExprKind::Str(literal) => format!("string ({} part(s))", literal.parts.len()),
        ExprKind::Path(path) => format!("path {}", join_path(path)),
        ExprKind::Wildcard => "wildcard".to_owned(),
        ExprKind::StructLit { path, fields } => {
            format!(
                "struct literal {} ({} field(s))",
                join_path(path),
                fields.len()
            )
        }
        ExprKind::Tuple(items) => format!("tuple ({})", items.len()),
        ExprKind::Group(_) => "group".to_owned(),
        ExprKind::Block(_) => "block expression".to_owned(),
        ExprKind::Unary { op, .. } => format!("unary {op:?}"),
        ExprKind::Binary { op, .. } => format!("binary {op:?}"),
        ExprKind::Cast { .. } => "cast".to_owned(),
        ExprKind::Call { args, .. } => format!("call ({} arg(s))", args.len()),
        ExprKind::BracketApply { args, .. } => format!("bracket-apply ({} arg(s))", args.len()),
        ExprKind::Member { member, .. } => match member {
            Member::Named(ident) => format!("member .{}", ident.name),
            Member::Index(index, _) => format!("member .{index}"),
        },
        ExprKind::Try(_) => "try `?`".to_owned(),
        ExprKind::Range { inclusive, .. } => {
            format!("range {}", if *inclusive { "..=" } else { ".." })
        }
        ExprKind::FromEnd(_) => "from-end `^`".to_owned(),
        ExprKind::ElseDefault { .. } => "else defaulting".to_owned(),
        ExprKind::If { .. } => "if".to_owned(),
        ExprKind::Match { arms, .. } => format!("match ({} arm(s))", arms.len()),
        ExprKind::For { .. } => "for".to_owned(),
        ExprKind::While { .. } => "while".to_owned(),
        ExprKind::Loop { .. } => "loop".to_owned(),
        ExprKind::Return(_) => "return".to_owned(),
        ExprKind::Break(_) => "break".to_owned(),
        ExprKind::Continue => "continue".to_owned(),
        ExprKind::Closure { params, .. } => format!("closure ({} param(s))", params.len()),
        ExprKind::RegionSugar { name, .. } => format!(
            "region sugar {}",
            name.as_ref().map_or("<anon>", |n| n.name.as_str())
        ),
        ExprKind::RegionValue { .. } => "region value".to_owned(),
        ExprKind::In { .. } => "in region".to_owned(),
        ExprKind::Freeze(_) => "freeze".to_owned(),
        ExprKind::Scope { name, .. } => format!(
            "scope {}",
            name.as_ref().map_or("<anon>", |n| n.name.as_str())
        ),
        ExprKind::SpawnProc { path, .. } => format!("spawn proc {}", join_path(path)),
        ExprKind::Select { arms } => format!("select ({} arm(s))", arms.len()),
        ExprKind::When { operands, .. } => format!("when ({} operand(s))", operands.len()),
        ExprKind::Unsafe { .. } => "unsafe".to_owned(),
        ExprKind::UnsafeC { .. } => "unsafe c".to_owned(),
        ExprKind::Asm { operands, .. } => format!("asm ({} operand(s))", operands.len()),
        ExprKind::Borrow { .. } => "borrow".to_owned(),
    };
    line(out, depth, expr.span, expr.anchor, &label);

    let child = depth + 1;
    match expr.kind.as_ref() {
        ExprKind::Str(literal) => {
            for part in &literal.parts {
                if let StrPart::Interp(interp) = part {
                    line(out, child, interp.span, "gram.lex.str", "interpolation");
                    trace_expr(out, child + 1, &interp.expr);
                    if let Some(fmt) = &interp.format {
                        for piece in fmt {
                            if let FmtPart::Interp(inner) = piece {
                                trace_expr(out, child + 1, inner);
                            }
                        }
                    }
                }
            }
        }
        ExprKind::Group(inner)
        | ExprKind::Unary { operand: inner, .. }
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Try(inner)
        | ExprKind::FromEnd(inner)
        | ExprKind::Freeze(inner) => trace_expr(out, child, inner),
        ExprKind::Tuple(items) => {
            for item in items {
                trace_expr(out, child, item);
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for field in fields {
                if let Some(value) = &field.value {
                    trace_expr(out, child, value);
                }
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            trace_expr(out, child, lhs);
            trace_expr(out, child, rhs);
        }
        ExprKind::Block(block)
        | ExprKind::Loop { body: block }
        | ExprKind::Unsafe { body: block }
        | ExprKind::Scope { body: block, .. }
        | ExprKind::RegionSugar { body: block, .. } => trace_block(out, child, block),
        ExprKind::Call { callee, args } => {
            trace_expr(out, child, callee);
            for arg in args {
                trace_expr(out, child, &arg.expr);
            }
        }
        ExprKind::BracketApply { base, args } => {
            trace_expr(out, child, base);
            for arg in args {
                if let IndexArg::Value(value) = arg {
                    trace_expr(out, child, &value.expr);
                }
            }
        }
        ExprKind::Member { base, .. } => trace_expr(out, child, base),
        ExprKind::Range { start, end, .. } => {
            if let Some(start) = start {
                trace_expr(out, child, start);
            }
            if let Some(end) = end {
                trace_expr(out, child, end);
            }
        }
        ExprKind::ElseDefault { expr, handler } => {
            trace_expr(out, child, expr);
            match handler.as_ref() {
                ElseHandler::Block(block) => trace_block(out, child, block),
                ElseHandler::Expr(inner) | ElseHandler::Handler { body: inner, .. } => {
                    trace_expr(out, child, inner);
                }
            }
        }
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            trace_expr(out, child, cond);
            trace_block(out, child, then);
            if let Some(otherwise) = otherwise {
                trace_expr(out, child, otherwise);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            trace_expr(out, child, scrutinee);
            for arm in arms {
                line(out, child, arm.span, "gram.expr.flow", "match arm");
                trace_expr(out, child + 1, &arm.body);
            }
        }
        ExprKind::For { iter, body, .. } => {
            trace_expr(out, child, iter);
            trace_block(out, child, body);
        }
        ExprKind::While { cond, body } => {
            trace_expr(out, child, cond);
            trace_block(out, child, body);
        }
        ExprKind::Return(Some(inner)) | ExprKind::Break(Some(inner)) => {
            trace_expr(out, child, inner);
        }
        ExprKind::Closure { body, .. } => trace_expr(out, child, body),
        ExprKind::In { region, body } => {
            trace_expr(out, child, region);
            trace_block(out, child, body);
        }
        ExprKind::SpawnProc { args, .. } => {
            for arg in args {
                trace_expr(out, child, &arg.expr);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                line(out, child, arm.span, "gram.expr.conc", "select arm");
                match &arm.kind {
                    SelectArmKind::Recv { channel, .. } => trace_expr(out, child + 1, channel),
                    SelectArmKind::Timeout(deadline) => trace_expr(out, child + 1, deadline),
                }
                trace_expr(out, child + 1, &arm.body);
            }
        }
        ExprKind::When { operands, body } => {
            for operand in operands {
                trace_expr(out, child, operand);
            }
            trace_block(out, child, body);
        }
        ExprKind::Asm { operands, .. } => {
            for operand in operands {
                trace_expr(out, child, &operand.value);
            }
        }
        ExprKind::Borrow { place, from } => {
            trace_expr(out, child, place);
            trace_expr(out, child, from);
        }
        _ => {}
    }
}

fn join_path(path: &Path) -> String {
    path.segments
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parses(source: &str) -> Unit {
        match parse_source(source) {
            Ok(parsed) => parsed.unit,
            Err(e) => panic!("expected a parse, got {e}\n--- source ---\n{source}"),
        }
    }

    fn rejects(source: &str) -> Diag {
        match parse_source(source) {
            Ok(_) => panic!("expected a rejection\n--- source ---\n{source}"),
            Err(e) => e,
        }
    }

    #[test]
    fn the_precedence_table_covers_every_tier_once() {
        assert_eq!(PRECEDENCE.len(), PRECEDENCE_TIERS);
        let tiers: Vec<u8> = PRECEDENCE.iter().map(|(t, _, _)| *t).collect();
        let mut sorted = tiers.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(tiers, sorted, "tiers must be listed once, tightest first");
        assert_eq!(tiers.first(), Some(&2));
        assert_eq!(tiers.last(), Some(&15));
    }

    #[test]
    fn hello_parses() {
        let unit = parses(
            "fn main() -> !int {\n    let who = \"wolf\"\n    print(\"hello, {who}\")\n    0\n}\n",
        );
        assert_eq!(unit.items.len(), 1);
    }

    #[test]
    fn comparisons_do_not_chain() {
        let d = rejects("fn main() -> int { a < b < c }\n");
        assert_eq!(d.code, diag::E_COMPARISON_CHAIN);
    }

    #[test]
    fn a_leading_operator_is_e0001() {
        let d = rejects("fn main() -> int {\n    let a = 1\n        + 2\n    0\n}\n");
        assert_eq!(d.code, diag::E_LEADING_OPERATOR);
    }

    #[test]
    fn an_empty_statement_is_e0002() {
        let d = rejects("fn main() -> int {\n    ;\n    0\n}\n");
        assert_eq!(d.code, diag::E_EMPTY_STATEMENT);
    }

    #[test]
    fn a_keyword_name_is_e0008() {
        let d = rejects("fn when(a: int) -> int { a }\n");
        assert_eq!(d.code, diag::E_KEYWORD_AS_IDENT);
    }

    #[test]
    fn an_orphaned_else_is_e0005() {
        let d = rejects("fn main() -> int {\n    if a { 1 }\n    else { 2 }\n}\n");
        assert_eq!(d.code, diag::E_ELSE_NEW_LINE);
    }

    #[test]
    fn a_struct_literal_in_a_condition_is_e0006() {
        let d = rejects("fn main() -> int {\n    if p == Point { x: 0 } { return 0 }\n    1\n}\n");
        assert_eq!(d.code, diag::E_STRUCT_LIT_IN_COND);
    }

    #[test]
    fn parenthesising_the_struct_literal_is_the_fix() {
        parses("fn main() -> int {\n    if p == (Point { x: 0 }) { 0 } else { 1 }\n}\n");
    }

    #[test]
    fn when_needs_two_operands() {
        let d = rejects("fn main() -> int {\n    when (a) { b }\n    0\n}\n");
        assert_eq!(d.code, diag::E_WHEN_ARITY);
    }

    #[test]
    fn a_bodyless_fn_needs_extern() {
        let d = rejects("fn detached() -> int\n");
        assert_eq!(d.code, diag::E_FN_NEEDS_BODY);
        parses("extern \"c\" fn detached() -> int\n");
    }

    #[test]
    fn the_bang_positions_are_disjoint() {
        // `!` as the error-union constructor in type position, as unary not in
        // expression position — one line, both readings (`[gram.amb.bang]`).
        parses("fn f() -> !int { if !t { 1 } else { 0 } }\n");
        parses("fn rowy() -> int ! {Failed, Slow} { 7 }\n");
    }

    #[test]
    fn brackets_are_one_postfix_form() {
        parses("fn main() -> int {\n    let a = first[int](xs)\n    let b = xs[0]\n    0\n}\n");
    }

    #[test]
    fn from_end_endpoints_parse() {
        parses(
            "fn main() -> int {\n    let a = s[^1]\n    let b = s[..^1]\n    let c = s[^13..]\n    0\n}\n",
        );
    }

    #[test]
    fn the_nesting_rail_answers_instead_of_meeting_the_stack() {
        // The rail is ours, not the spec's; what matters is that the deep case
        // produces a diagnostic and the shallow case does not.
        let inside = format!(
            "fn f() -> int {{ let v = {}1{}\n0 }}\n",
            "(".repeat(MAX_NESTING - 8),
            ")".repeat(MAX_NESTING - 8)
        );
        parses(&inside);
        let outside = format!(
            "fn f() -> int {{ let v = {}1{}\n0 }}\n",
            "(".repeat(MAX_NESTING + 4),
            ")".repeat(MAX_NESTING + 4)
        );
        assert_eq!(rejects(&outside).code, diag::E_NESTING_RAIL);
    }

    #[test]
    fn an_expression_bodied_closure_extends_maximally() {
        let unit = parses(
            "fn main() -> int {\n    let t = xs.sorted_by(fn(a, b) b.1 <=> a.1).take(1)\n    0\n}\n",
        );
        assert_eq!(unit.items.len(), 1);
    }
}

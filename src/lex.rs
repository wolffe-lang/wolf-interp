//! The lexer, implemented from `spec/01-grammar.md` §1 `[gram.lex]`.
//!
//! Three things here are hard, and the spec says so:
//!
//! 1. **The mode stack** (`[gram.lex.str]`). Every plain string is an f-string;
//!    inside `{…}` the lexer re-enters normal token mode, where another string
//!    may open, whose interpolation may open another… The stack is
//!    [`Ctx`]: delimiters, interpolations, string bodies and format specs all
//!    live on one stack, because they nest into each other, and the spec's two
//!    hardest rules — "the first *top-level* `:` starts the format spec"
//!    (`[gram.amb.fmtcolon]`) and "the *innermost* enclosing delimiter decides"
//!    (`[gram.lex.newline]`) — are both questions about that stack's top.
//! 2. **Terminator insertion** (`[gram.lex.newline]`), which the spec calls
//!    normative and byte-exact and which the interpreter "must match". It is
//!    transcribed here as a token class predicate plus three exceptions, in the
//!    same order the document states them.
//! 3. **`"""` dedent** (`[gram.lex.str.multi]`): the *closing* delimiter's
//!    column sets the strip width, so the width is unknown until the literal
//!    ends. We buffer the literal's text chunks and rewrite them on close.
//!
//! Trivia is dropped: comments and whitespace produce no tokens. The
//! interpreter never reprints source (the formatter is compiler-side), so
//! there is nothing for losslessness to buy (is01 non-target).
//!
//! Totality: the lexer runs to the end of the input over arbitrary bytes,
//! producing error tokens and diagnostics rather than stopping. It performs
//! **no recovery cleverness** — the first diagnostic is the one that matters
//! and the rest are a courtesy.

use std::fmt;

use crate::diag::{self, Diag, Span};

/// The 50 reserved keywords of `[gram.inv.kw]`. The list is normative; the
/// count is the spec's own checksum.
pub const KEYWORDS: [&str; 50] = [
    "as", "asm", "assume", "borrow", "break", "comptime", "const", "continue", "copy", "defer",
    "distinct", "dyn", "else", "enum", "errdefer", "export", "extern", "false", "fn", "for",
    "freeze", "handle", "if", "impl", "import", "in", "let", "loop", "match", "move", "mut",
    "proc", "pub", "region", "return", "scope", "select", "shared", "spawn", "struct", "take",
    "trait", "true", "type", "unsafe", "use", "var", "weak", "when", "while",
];

/// Contextual keywords (`[gram.inv.ctx]`): identifiers everywhere except one
/// position. They lex as [`Tok::Ident`] and are recognised by the parser, never
/// by the lexer — that is what "contextual" means.
/// §6.2 also covers "register classes (asm operands)" — `reg` and friends —
/// but never names one, and nothing in the parser matches them by spelling:
/// an asm constraint is read with the ordinary identifier rule. They are
/// therefore absent from this list, which holds only the words a production
/// tests for.
pub const CONTEXTUAL: [&str; 11] = [
    "c", "rc", "pool", "from", "timeout", "noalias", "pkg", "self", "out", "inout", "lateout",
];

/// Is this identifier a reserved keyword (`[gram.inv.kw]`)?
#[must_use]
pub fn is_keyword(word: &str) -> bool {
    KEYWORDS.binary_search(&word).is_ok()
}

/// Which flavour of string literal a [`Tok::StrStart`] opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrKind {
    /// `"…"` — an f-string with escapes and interpolation (X9/D26).
    Plain,
    /// `"""…"""` — multiline, dedented by the closing column
    /// (`[gram.lex.str.multi]`).
    Multiline,
    /// `r"…"` / `r#"…"#` — no escapes, no interpolation
    /// (`[gram.lex.str.raw]`).
    Raw { hashes: usize },
    /// `re"…"`, `path"/etc/hosts"` — raw body, comptime-desugared
    /// (`[gram.lex.str.gen]`).
    Generalized { prefix: String },
}

impl StrKind {
    /// Interpolation only runs in the two non-raw modes.
    #[must_use]
    pub fn interpolates(&self) -> bool {
        matches!(self, StrKind::Plain | StrKind::Multiline)
    }
}

/// A lexical token. String literals are *several* tokens: the mode stack is
/// visible in the stream, which is what lets the parser build an f-string node
/// without re-lexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    /// `_`, the wildcard identifier — never a binding you can read
    /// (`[gram.lex.ident]`).
    Underscore,
    Kw(&'static str),
    Int(String),
    Float(String),

    /// Opens a literal; every `StrStart` is matched by a `StrEnd`.
    StrStart(StrKind),
    /// A decoded run of literal text (escapes already applied, `{{`/`}}`
    /// already collapsed, `"""` dedent already stripped).
    StrText(String),
    /// `{` of an interpolation — normal token mode resumes after it.
    InterpStart,
    /// `}` closing an interpolation (or its format spec).
    InterpEnd,
    /// The `:` that begins a format spec (`[gram.amb.fmtcolon]`).
    FmtStart,
    /// Literal format-spec text between `:` and `}`.
    FmtText(String),
    StrEnd,

    /// A statement terminator: inserted at a newline per `[gram.lex.newline]`,
    /// or written explicitly as `;`. The parser needs to tell them apart to
    /// diagnose E0002, so the provenance rides along.
    Term {
        explicit: bool,
    },

    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    /// `#[` — attributes are one token because the `]` that closes them is the
    /// `[gram.lex.newline]` exception, and pairing them here keeps that
    /// bookkeeping in the lexer where it belongs.
    HashBracket,

    /// `!` — unary not in expression position, the error-union constructor in
    /// type position. `[gram.amb.bang]` calls the two positions disjoint, so
    /// one token serves both.
    Bang,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Shl,
    Shr,
    Amp,
    Caret,
    Pipe,
    AndAnd,
    OrOr,

    EqEq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    /// `<=>`, three-way comparison.
    Spaceship,

    Assign,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,

    Dot,
    DotDot,
    DotDotEq,
    Arrow,
    FatArrow,
    Question,
    Colon,
    Comma,
    At,

    /// A byte that begins no token. Totality, not recovery: the diagnostic is
    /// already filed; this keeps offsets honest for everything after it.
    Error,
}

impl Tok {
    /// The `[gram.lex.newline]` statement-ending token class, transcribed from
    /// the spec's bullet list in its own order:
    ///
    /// - an identifier or `_`
    /// - any literal (INT, FLOAT, any string-mode end, `true`, `false`)
    /// - one of the keywords `return`, `break`, `continue`
    /// - a closing delimiter `)`, `]`, `}`
    /// - postfix `?`
    #[must_use]
    pub fn ends_a_statement(&self) -> bool {
        match self {
            Tok::Ident(_) | Tok::Underscore => true,
            Tok::Int(_) | Tok::Float(_) | Tok::StrEnd => true,
            Tok::Kw(k) => matches!(*k, "true" | "false" | "return" | "break" | "continue"),
            Tok::RParen | Tok::RBracket | Tok::RBrace => true,
            Tok::Question => true,
            _ => false,
        }
    }

    /// Binary-only operators: tokens that continue an expression but can never
    /// begin one. A statement that starts with one of these is the
    /// leading-operator continuation `[gram.amb.newline]` rejects (E0001).
    ///
    /// `-`, `*`, `&`, `!` are absent on purpose — all four are prefix operators
    /// (tier 3), so a statement may legitimately start with them.
    #[must_use]
    pub fn is_binary_only(&self) -> bool {
        matches!(
            self,
            Tok::Plus
                | Tok::Slash
                | Tok::Percent
                | Tok::Shl
                | Tok::Shr
                | Tok::Caret
                | Tok::Pipe
                | Tok::AndAnd
                | Tok::OrOr
                | Tok::EqEq
                | Tok::Ne
                | Tok::Lt
                | Tok::Gt
                | Tok::Le
                | Tok::Ge
                | Tok::Spaceship
                | Tok::Dot
                | Tok::DotDot
                | Tok::DotDotEq
                | Tok::Kw("as")
        )
    }

    /// A short, stable spelling for dumps and diagnostics. **Ours** — the
    /// compiler's token names are not something we are allowed to have seen.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("identifier `{s}`"),
            Tok::Underscore => "`_`".to_owned(),
            Tok::Kw(k) => format!("keyword `{k}`"),
            Tok::Int(s) => format!("integer `{s}`"),
            Tok::Float(s) => format!("float `{s}`"),
            Tok::StrStart(_) => "the start of a string literal".to_owned(),
            Tok::StrText(_) => "string text".to_owned(),
            Tok::InterpStart => "`{` of an interpolation".to_owned(),
            Tok::InterpEnd => "`}` of an interpolation".to_owned(),
            Tok::FmtStart => "`:` of a format spec".to_owned(),
            Tok::FmtText(_) => "format-spec text".to_owned(),
            Tok::StrEnd => "the end of a string literal".to_owned(),
            Tok::Term { explicit: true } => "`;`".to_owned(),
            Tok::Term { explicit: false } => "an end of line".to_owned(),
            Tok::Error => "an unrecognised byte".to_owned(),
            other => format!("`{}`", punctuation_spelling(other)),
        }
    }
}

fn punctuation_spelling(tok: &Tok) -> &'static str {
    match tok {
        Tok::LParen => "(",
        Tok::RParen => ")",
        Tok::LBracket => "[",
        Tok::RBracket => "]",
        Tok::LBrace => "{",
        Tok::RBrace => "}",
        Tok::HashBracket => "#[",
        Tok::Bang => "!",
        Tok::Plus => "+",
        Tok::Minus => "-",
        Tok::Star => "*",
        Tok::Slash => "/",
        Tok::Percent => "%",
        Tok::Shl => "<<",
        Tok::Shr => ">>",
        Tok::Amp => "&",
        Tok::Caret => "^",
        Tok::Pipe => "|",
        Tok::AndAnd => "&&",
        Tok::OrOr => "||",
        Tok::EqEq => "==",
        Tok::Ne => "!=",
        Tok::Lt => "<",
        Tok::Gt => ">",
        Tok::Le => "<=",
        Tok::Ge => ">=",
        Tok::Spaceship => "<=>",
        Tok::Assign => "=",
        Tok::PlusEq => "+=",
        Tok::MinusEq => "-=",
        Tok::StarEq => "*=",
        Tok::SlashEq => "/=",
        Tok::PercentEq => "%=",
        Tok::AmpEq => "&=",
        Tok::PipeEq => "|=",
        Tok::CaretEq => "^=",
        Tok::ShlEq => "<<=",
        Tok::ShrEq => ">>=",
        Tok::Dot => ".",
        Tok::DotDot => "..",
        Tok::DotDotEq => "..=",
        Tok::Arrow => "->",
        Tok::FatArrow => "=>",
        Tok::Question => "?",
        Tok::Colon => ":",
        Tok::Comma => ",",
        Tok::At => "@",
        _ => "?",
    }
}

/// A token and its byte-exact span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Everything one lexer run produced.
#[derive(Debug, Clone)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    /// Lex-tier failures. Non-empty ⇒ the `lex` rung failed.
    pub errors: Vec<Diag>,
    /// Diagnostics the lexer *detected* but the spec assigns to the parse tier
    /// — today exactly E0007, the depth-8 interpolation limit, which
    /// `[gram.lex.str]` says must still tokenize so the parser can produce the
    /// friendly error.
    pub deferred: Vec<Diag>,
    /// The deepest string-nesting depth seen, for the dump format.
    pub max_str_depth: usize,
    /// Was the mode/delimiter stack still open when the bytes ran out — an
    /// unclosed `(`/`[`/`{`/string/interpolation? The REPL's continuation
    /// predicate ([`repl_input_complete`]) reads the stack the lexer itself
    /// kept, captured before `close_unterminated` drains it.
    pub open_at_eof: bool,
}

impl Lexed {
    /// The first lex-tier diagnostic, in source order.
    #[must_use]
    pub fn first_error(&self) -> Option<&Diag> {
        self.errors.iter().min_by_key(|d| d.span.start)
    }
}

/// The mode stack (`[gram.lex.str]`) and the delimiter stack
/// (`[gram.lex.newline]`) are the same stack: interpolations nest inside
/// brackets which nest inside strings, and both rules ask about its top.
#[derive(Debug, Clone)]
enum Ctx {
    Paren,
    Bracket,
    /// The `[` of an attribute — its `]` gets no inserted terminator.
    AttrBracket,
    Brace,
    /// Inside `{…}` of an interpolation: normal token mode, but `}` closes and
    /// a top-level `:` opens the format spec.
    Interp,
    Str(Box<StrFrame>),
    /// Inside a format spec: literal text until `}`, with `{expr}` allowed.
    Fmt,
}

/// A piece of a multiline literal, remembered so the dedent can be applied
/// once the closing column is known.
#[derive(Debug, Clone)]
enum Piece {
    /// Index into `tokens` of a `StrText` produced by this literal.
    Text(usize),
    /// An interpolation sat here; it ends any line-start state.
    Interp,
}

#[derive(Debug, Clone)]
struct StrFrame {
    kind: StrKind,
    /// Byte offset just past the opening delimiter.
    body_start: usize,
    /// Text accumulated since the last emitted `StrText`.
    pending: String,
    /// Byte offset where `pending` began.
    pending_start: usize,
    /// Multiline bookkeeping (empty for every other kind).
    pieces: Vec<Piece>,
}

/// The depth-8 friendly limit and the depth-32 hard rail (`[gram.lex.str]`).
const NESTING_FRIENDLY_LIMIT: usize = 8;
const NESTING_HARD_RAIL: usize = 32;

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    tokens: Vec<Token>,
    ctx: Vec<Ctx>,
    errors: Vec<Diag>,
    deferred: Vec<Diag>,
    max_str_depth: usize,
    /// E0007 is reported once per file, not once per nested literal.
    depth_reported: bool,
    /// Index of the most recent `]` that closed an attribute — the
    /// `[gram.lex.newline]` exception is a property of *that* token, so it is
    /// recorded rather than re-derived from a stack we have already popped.
    attr_close: Option<usize>,
    /// Captured by `close_unterminated`: the stack was open at end of input.
    open_at_eof: bool,
}

/// Tokenizes a source file.
///
/// Total over arbitrary input: every byte is consumed, unrecognised ones
/// becoming [`Tok::Error`] with a diagnostic. Callers decide whether a
/// non-empty `errors` means the `lex` rung failed (it does).
#[must_use]
pub fn lex(src: &str) -> Lexed {
    let mut lexer = Lexer {
        src,
        pos: 0,
        tokens: Vec::new(),
        ctx: Vec::new(),
        errors: Vec::new(),
        deferred: Vec::new(),
        max_str_depth: 0,
        depth_reported: false,
        attr_close: None,
        open_at_eof: false,
    };
    lexer.run();
    Lexed {
        tokens: lexer.tokens,
        errors: lexer.errors,
        deferred: lexer.deferred,
        max_str_depth: lexer.max_str_depth,
        open_at_eof: lexer.open_at_eof,
    }
}

/// The REPL's continuation predicate (is08): is `src`, as typed so far, a
/// complete input?
///
/// This runs the real lexer and asks the real `[gram.lex.newline]` machinery
/// — not a paraphrase of it. An input **continues** (returns `false`) when:
///
/// - the mode/delimiter stack is still open at end of input — an unclosed
///   `(`, `[`, `{`, string, or interpolation ([`Lexer`]'s `ctx`), or
/// - the final token cannot end a statement, so the lexer inserted no
///   terminator at EOF ([`Tok::ends_a_statement`] via
///   `insert_terminator_at_eof` — a trailing `+` continues, byte-exactly as
///   `grammar/newline_trailing.lu` pins for whole files).
///
/// Anything else — including input the parser will reject — is *complete*:
/// the REPL evaluates it and reports, rather than trapping the user in a
/// continuation they cannot escape.
#[must_use]
pub fn repl_input_complete(src: &str) -> bool {
    let lexed = lex(src);
    if lexed.open_at_eof {
        return false;
    }
    match lexed.tokens.last() {
        None => true,
        Some(token) => matches!(token.tok, Tok::Term { .. }),
    }
}

/// Tokenizes raw bytes, rejecting non-UTF-8 source (`[gram.lex.source]`).
///
/// # Errors
///
/// Never — invalid UTF-8 comes back as a `Lexed` whose `errors` holds E0107,
/// so the caller's single "did lexing fail?" branch covers it.
#[must_use]
pub fn lex_bytes(bytes: &[u8]) -> Lexed {
    match std::str::from_utf8(bytes) {
        Ok(text) => lex(text),
        Err(e) => {
            let at = e.valid_up_to();
            Lexed {
                tokens: Vec::new(),
                errors: vec![Diag::new(
                    diag::E_NOT_UTF8,
                    Span::new(at, (at + 1).min(bytes.len())),
                    "gram.lex.source",
                    "source files are UTF-8",
                )],
                deferred: Vec::new(),
                max_str_depth: 0,
                open_at_eof: false,
            }
        }
    }
}

impl<'a> Lexer<'a> {
    fn run(&mut self) {
        // `[gram.lex.source]`: byte order marks are rejected.
        if self.src.starts_with('\u{feff}') {
            self.error(
                diag::E_BYTE_ORDER_MARK,
                Span::new(0, 3),
                "gram.lex.source",
                "byte order marks are rejected; save the file as plain UTF-8",
            );
            self.pos = 3;
        }

        loop {
            match self.ctx.last() {
                Some(Ctx::Str(_)) => {
                    if !self.scan_string_body() {
                        break;
                    }
                }
                Some(Ctx::Fmt) => {
                    if !self.scan_format_spec() {
                        break;
                    }
                }
                _ => {
                    if !self.scan_normal() {
                        break;
                    }
                }
            }
        }
        self.close_unterminated();

        // Go's rule 2 in reverse: a file that ends without a newline still ends
        // its last statement.
        self.insert_terminator_at_eof();
    }

    // -- plumbing ---------------------------------------------------------

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.src.get(self.pos + offset..)?.chars().next()
    }

    fn starts_with(&self, s: &str) -> bool {
        self.src[self.pos..].starts_with(s)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn push(&mut self, tok: Tok, span: Span) {
        self.tokens.push(Token { tok, span });
    }

    fn error(&mut self, code: &'static str, span: Span, anchor: &'static str, msg: &str) {
        self.errors.push(Diag::new(code, span, anchor, msg));
    }

    /// The innermost *delimiter*, skipping string and format frames — the
    /// subject of `[gram.lex.newline]`'s "the innermost enclosing delimiter
    /// decides".
    fn innermost_delimiter(&self) -> Option<&Ctx> {
        self.ctx
            .iter()
            .rev()
            .find(|c| !matches!(c, Ctx::Str(_) | Ctx::Fmt))
    }

    fn string_depth(&self) -> usize {
        self.ctx.iter().filter(|c| matches!(c, Ctx::Str(_))).count()
    }

    /// `[gram.lex.newline]`, in the document's own order: insert iff the last
    /// token on the line ends a statement, *and* the innermost enclosing
    /// delimiter permits it.
    ///
    /// - `(`, `[`, an interpolation: newlines never terminate.
    /// - `{…}` re-enables insertion inside itself whatever it is nested in.
    /// - top level behaves like a block.
    /// - the `]` closing an attribute is exempt — handled where it is emitted.
    fn insert_terminator(&mut self, at: usize) {
        match self.innermost_delimiter() {
            None | Some(Ctx::Brace) => {}
            Some(_) => return,
        }
        let Some(last) = self.tokens.last() else {
            return;
        };
        if !last.tok.ends_a_statement() {
            return;
        }
        // The attribute exception: no terminator after the `]` that closes a
        // `#[…]` — the attribute prefixes the construct on the next line.
        if self.attr_close == Some(self.tokens.len() - 1) {
            return;
        }
        self.push(Tok::Term { explicit: false }, Span::empty(at));
    }

    fn insert_terminator_at_eof(&mut self) {
        match self.innermost_delimiter() {
            None | Some(Ctx::Brace) => {}
            Some(_) => return,
        }
        if self.attr_close == Some(self.tokens.len().saturating_sub(1)) {
            return;
        }
        if self.tokens.last().is_some_and(|t| t.tok.ends_a_statement()) {
            let at = self.src.len();
            self.push(Tok::Term { explicit: false }, Span::empty(at));
        }
    }

    // -- normal token mode -------------------------------------------------

    /// Scans one token (or a run of trivia). Returns false at end of input.
    fn scan_normal(&mut self) -> bool {
        loop {
            let Some(c) = self.peek() else { return false };
            match c {
                '\n' => {
                    let at = self.pos;
                    self.pos += 1;
                    self.insert_terminator(at);
                }
                ' ' | '\t' | '\r' | '\u{0b}' | '\u{0c}' => {
                    self.pos += 1;
                }
                '/' if self.starts_with("//") => {
                    // `//`, `///` and `//!` all run to end of line
                    // (`[gram.lex.comment]`); there are no block comments.
                    while let Some(ch) = self.peek() {
                        if ch == '\n' {
                            break;
                        }
                        self.pos += ch.len_utf8();
                    }
                }
                _ => break,
            }
        }
        self.scan_token();
        true
    }

    #[allow(clippy::too_many_lines)]
    fn scan_token(&mut self) {
        let start = self.pos;
        let Some(c) = self.peek() else { return };

        // identifiers, keywords, and the three literal prefixes that ride on
        // an identifier: `r"…"`, `r#"…"#`, `re"…"`.
        if is_xid_start(c) || c == '_' {
            return self.scan_word();
        }
        if c.is_ascii_digit() {
            return self.scan_number();
        }
        if c == '"' {
            return self.open_string(None);
        }

        macro_rules! op {
            ($text:literal, $tok:expr) => {
                if self.starts_with($text) {
                    self.pos += $text.len();
                    self.push($tok, Span::new(start, self.pos));
                    return;
                }
            };
        }

        // `#[` opens an attribute; its `]` is `[gram.lex.newline]`'s exception,
        // so the bracket is tagged on the way in.
        if self.starts_with("#[") {
            self.pos += 2;
            self.ctx.push(Ctx::AttrBracket);
            self.push(Tok::HashBracket, Span::new(start, self.pos));
            return;
        }

        // Longest match first, and the three-character operators before the
        // two-character prefixes they share.
        op!("<<=", Tok::ShlEq);
        op!(">>=", Tok::ShrEq);
        op!("<=>", Tok::Spaceship);
        op!("..=", Tok::DotDotEq);
        op!("<<", Tok::Shl);
        op!(">>", Tok::Shr);
        op!("<=", Tok::Le);
        op!(">=", Tok::Ge);
        op!("==", Tok::EqEq);
        op!("!=", Tok::Ne);
        op!("&&", Tok::AndAnd);
        op!("||", Tok::OrOr);
        op!("+=", Tok::PlusEq);
        op!("-=", Tok::MinusEq);
        op!("*=", Tok::StarEq);
        op!("/=", Tok::SlashEq);
        op!("%=", Tok::PercentEq);
        op!("&=", Tok::AmpEq);
        op!("|=", Tok::PipeEq);
        op!("^=", Tok::CaretEq);
        op!("->", Tok::Arrow);
        op!("=>", Tok::FatArrow);
        op!("..", Tok::DotDot);

        self.pos += c.len_utf8();
        let span = Span::new(start, self.pos);
        let tok = match c {
            '(' => {
                self.ctx.push(Ctx::Paren);
                Tok::LParen
            }
            '[' => {
                self.ctx.push(Ctx::Bracket);
                Tok::LBracket
            }
            '{' => {
                self.ctx.push(Ctx::Brace);
                Tok::LBrace
            }
            ')' => {
                self.pop_delimiter(Ctx::Paren);
                Tok::RParen
            }
            ']' => return self.close_bracket(span),
            '}' => return self.close_brace(span),
            ':' => {
                // `[gram.amb.fmtcolon]`: the first top-level `:` inside an
                // interpolation — top level meaning the interpolation itself is
                // the innermost context — begins the format spec.
                if matches!(self.ctx.last(), Some(Ctx::Interp)) {
                    self.ctx.pop();
                    self.ctx.push(Ctx::Fmt);
                    Tok::FmtStart
                } else {
                    Tok::Colon
                }
            }
            '!' => Tok::Bang,
            '+' => Tok::Plus,
            '-' => Tok::Minus,
            '*' => Tok::Star,
            '/' => Tok::Slash,
            '%' => Tok::Percent,
            '&' => Tok::Amp,
            '^' => Tok::Caret,
            '|' => Tok::Pipe,
            '<' => Tok::Lt,
            '>' => Tok::Gt,
            '=' => Tok::Assign,
            '.' => Tok::Dot,
            '?' => Tok::Question,
            ',' => Tok::Comma,
            '@' => Tok::At,
            ';' => Tok::Term { explicit: true },
            _ => {
                self.error(
                    diag::E_UNEXPECTED_BYTE,
                    span,
                    "gram.lex",
                    &format!("`{c}` begins no token"),
                );
                Tok::Error
            }
        };
        self.push(tok, span);
    }

    fn pop_delimiter(&mut self, expected: Ctx) {
        let balanced = matches!(
            (&expected, self.ctx.last()),
            (Ctx::Paren, Some(Ctx::Paren))
                | (Ctx::Bracket, Some(Ctx::Bracket | Ctx::AttrBracket))
                | (Ctx::Brace, Some(Ctx::Brace))
        );
        if balanced {
            self.ctx.pop();
        }
        // An unmatched closer keeps the stack as-is: the parser will file the
        // real complaint, and dropping frames here would corrupt every span
        // after it.
    }

    /// `]` — with `[gram.lex.newline]`'s attribute exception. The `]` that
    /// closes a `#[…]` never ends a statement, so the attribute can prefix the
    /// construct on the next line.
    fn close_bracket(&mut self, span: Span) {
        let was_attribute = matches!(self.ctx.last(), Some(Ctx::AttrBracket));
        self.pop_delimiter(Ctx::Bracket);
        self.push(Tok::RBracket, span);
        if was_attribute {
            self.attr_close = Some(self.tokens.len() - 1);
        }
    }

    /// `}` — closes an interpolation when that is the innermost context
    /// (`[gram.lex.str]`'s mode stack), otherwise an ordinary block.
    fn close_brace(&mut self, span: Span) {
        match self.ctx.last() {
            Some(Ctx::Interp | Ctx::Fmt) => {
                self.ctx.pop();
                self.push(Tok::InterpEnd, span);
            }
            _ => {
                self.pop_delimiter(Ctx::Brace);
                self.push(Tok::RBrace, span);
            }
        }
    }

    fn scan_word(&mut self) {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_xid_continue(c) {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        let word = &self.src[start..self.pos];

        // `[gram.lex.str.raw]` / `[gram.lex.str.gen]`: a literal prefix must
        // touch its quote — no whitespace between.
        if word == "r" && matches!(self.peek(), Some('"' | '#')) {
            return self.open_raw_string(start);
        }
        if self.peek() == Some('"') && !is_keyword(word) && word != "_" {
            let prefix = word.to_owned();
            return self.open_string(Some((start, prefix)));
        }

        let span = Span::new(start, self.pos);
        let tok = if word == "_" {
            Tok::Underscore
        } else if let Ok(idx) = KEYWORDS.binary_search(&word) {
            Tok::Kw(KEYWORDS[idx])
        } else {
            Tok::Ident(word.to_owned())
        };
        self.push(tok, span);
    }

    /// `[gram.lex.number]`. A float needs digits on **both** sides of the dot:
    /// `1.0` is a float, `1.` is an integer followed by member-access `.`
    /// (`[gram.amb.intdot]`), and `1..10` is a range. `1.e5` is member access
    /// on `1` with member `e5` — deliberately not a float, and deliberately
    /// not an error either (see `diag::E_FLOAT_DOT_EXPONENT`).
    fn scan_number(&mut self) {
        let start = self.pos;

        // `0x…`, `0o…`, `0b…` — a based literal is a different production
        // from `DEC_LIT`, and never a float.
        if self.peek() == Some('0')
            && let Some(radix) = self.peek_at(1).and_then(base_radix)
        {
            self.pos += 2;
            let digits_start = self.pos;
            while let Some(c) = self.peek() {
                // Alphanumerics outside the base are consumed too, so the
                // diagnostic's span covers the whole of what was written.
                if c == '_' || c.is_ascii_alphanumeric() {
                    self.pos += c.len_utf8();
                } else {
                    break;
                }
            }
            let text = &self.src[digits_start..self.pos];
            let span = Span::new(start, self.pos);
            if text.is_empty() || !text.chars().all(|c| c == '_' || c.is_digit(radix)) {
                self.error(
                    diag::E_BAD_NUMBER,
                    span,
                    "gram.lex.number",
                    "this base prefix needs digits of its own base",
                );
            }
            let literal = self.src[start..self.pos].to_owned();
            self.push(Tok::Int(literal), span);
            return;
        }

        self.eat_decimal_run();
        let mut is_float = false;

        // `.` joins the literal only when a digit follows it — and never when
        // the `.` is the first of a `..` range.
        if self.peek() == Some('.')
            && self.peek_at(1) != Some('.')
            && self.peek_at(1).is_some_and(|c| c.is_ascii_digit())
        {
            self.pos += 1;
            self.eat_decimal_run();
            is_float = true;
        }

        if matches!(self.peek(), Some('e' | 'E')) {
            let after = self.peek_at(1);
            let exponent_digits = if matches!(after, Some('+' | '-')) {
                self.peek_at(2)
            } else {
                after
            };
            if exponent_digits.is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
                if matches!(self.peek(), Some('+' | '-')) {
                    self.pos += 1;
                }
                self.eat_decimal_run();
                is_float = true;
            }
        }

        let span = Span::new(start, self.pos);
        let text = self.src[start..self.pos].to_owned();
        self.push(
            if is_float {
                Tok::Float(text)
            } else {
                Tok::Int(text)
            },
            span,
        );
    }

    fn eat_decimal_run(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    // -- string modes ------------------------------------------------------

    /// Opens `"` / `"""` / `prefix"…"`.
    fn open_string(&mut self, generalized: Option<(usize, String)>) {
        let (start, kind) = match generalized {
            Some((start, prefix)) => (start, StrKind::Generalized { prefix }),
            None => {
                let start = self.pos;
                // `"""` opens multiline; `""` is the empty string. The lexer
                // asks for three quotes and gets them or does not.
                if self.starts_with("\"\"\"") {
                    (start, StrKind::Multiline)
                } else {
                    (start, StrKind::Plain)
                }
            }
        };

        let quote_len = if matches!(kind, StrKind::Multiline) {
            3
        } else {
            1
        };
        // Step over the prefix (already consumed for generalized) and quotes.
        self.pos = if matches!(kind, StrKind::Generalized { .. }) {
            self.pos + 1
        } else {
            self.pos + quote_len
        };

        // `[gram.lex.str.multi]`: the first newline after the opening `"""` is
        // dropped.
        if matches!(kind, StrKind::Multiline) {
            let save = self.pos;
            if self.peek() == Some('\r') {
                self.pos += 1;
            }
            if self.peek() == Some('\n') {
                self.pos += 1;
            } else {
                self.pos = save;
            }
        }

        self.push(Tok::StrStart(kind.clone()), Span::new(start, self.pos));
        self.enter_string(kind);
    }

    /// `r"…"`, `r#"…"#`, `r##"…"##` — `#`-fences balance
    /// (`[gram.lex.str.raw]`).
    fn open_raw_string(&mut self, start: usize) {
        let mut hashes = 0usize;
        while self.peek() == Some('#') {
            hashes += 1;
            self.pos += 1;
        }
        if self.peek() != Some('"') {
            // `r#` with no quote is not a raw string at all; back out and let
            // `r` be an identifier so the offsets stay honest.
            self.pos = start + 1;
            self.push(Tok::Ident("r".to_owned()), Span::new(start, self.pos));
            return;
        }
        self.pos += 1;
        let kind = StrKind::Raw { hashes };
        self.push(Tok::StrStart(kind.clone()), Span::new(start, self.pos));
        self.enter_string(kind);
    }

    fn enter_string(&mut self, kind: StrKind) {
        let frame = StrFrame {
            kind,
            body_start: self.pos,
            pending: String::new(),
            pending_start: self.pos,
            pieces: Vec::new(),
        };
        self.ctx.push(Ctx::Str(Box::new(frame)));
        let depth = self.string_depth();
        self.max_str_depth = self.max_str_depth.max(depth);

        // `[gram.lex.str]`: legal to depth 8; deeper is E0007 at the *parse*
        // tier so the user gets the friendly error; depth 32 is the lexer's
        // hard safety rail, E0108, and there the run stops being useful.
        if depth > NESTING_FRIENDLY_LIMIT && depth <= NESTING_HARD_RAIL && !self.depth_reported {
            self.depth_reported = true;
            let span = self.tokens.last().map_or(Span::empty(self.pos), |t| t.span);
            self.deferred.push(Diag::new(
                diag::E_INTERP_DEPTH,
                span,
                "gram.lex.str",
                format!(
                    "strings nested {depth} deep inside interpolations; the limit is \
                     {NESTING_FRIENDLY_LIMIT}, and you do not want this"
                ),
            ));
        }
        if depth > NESTING_HARD_RAIL {
            let span = self.tokens.last().map_or(Span::empty(self.pos), |t| t.span);
            self.error(
                diag::E_INTERP_RAIL,
                span,
                "gram.lex.str",
                &format!(
                    "interpolation nesting hit the lexer's hard rail at depth {NESTING_HARD_RAIL}"
                ),
            );
        }
    }

    fn frame(&mut self) -> &mut StrFrame {
        match self.ctx.last_mut() {
            Some(Ctx::Str(frame)) => frame,
            _ => unreachable!("scan_string_body is only entered with a string frame on top"),
        }
    }

    fn flush_text(&mut self) {
        let end = self.pos;
        let frame = self.frame();
        if frame.pending.is_empty() {
            return;
        }
        let text = std::mem::take(&mut frame.pending);
        let span = Span::new(frame.pending_start, end);
        let multiline = matches!(frame.kind, StrKind::Multiline);
        self.push(Tok::StrText(text), span);
        if multiline {
            let index = self.tokens.len() - 1;
            self.frame().pieces.push(Piece::Text(index));
        }
    }

    fn text_push(&mut self, c: char) {
        let at = self.pos;
        let frame = self.frame();
        if frame.pending.is_empty() {
            frame.pending_start = at;
        }
        frame.pending.push(c);
    }

    /// Scans inside a string body. Returns false at end of input.
    #[allow(clippy::too_many_lines)]
    fn scan_string_body(&mut self) -> bool {
        let (kind, hashes) = {
            let frame = self.frame();
            let hashes = match &frame.kind {
                StrKind::Raw { hashes } => *hashes,
                _ => 0,
            };
            (frame.kind.clone(), hashes)
        };

        loop {
            let Some(c) = self.peek() else { return false };

            // -- closing delimiter --------------------------------------
            let closes = match &kind {
                StrKind::Multiline => self.starts_with("\"\"\""),
                StrKind::Raw { .. } => {
                    c == '"' && self.src[self.pos + 1..].starts_with(&"#".repeat(hashes))
                }
                StrKind::Plain | StrKind::Generalized { .. } => c == '"',
            };
            if closes {
                return self.close_string(&kind, hashes);
            }

            match &kind {
                StrKind::Raw { .. } | StrKind::Generalized { .. } => {
                    // Raw-mode body: no escapes, no interpolation, nothing but
                    // bytes until the fence.
                    self.text_push(c);
                    self.pos += c.len_utf8();
                }
                StrKind::Plain | StrKind::Multiline => {
                    match c {
                        '{' if self.starts_with("{{") => {
                            // `[gram.lex.str.escape]`
                            self.text_push('{');
                            self.pos += 2;
                        }
                        '}' if self.starts_with("}}") => {
                            self.text_push('}');
                            self.pos += 2;
                        }
                        '{' => {
                            let span = Span::new(self.pos, self.pos + 1);
                            self.flush_text();
                            self.pos += 1;
                            self.push(Tok::InterpStart, span);
                            if matches!(kind, StrKind::Multiline) {
                                self.frame().pieces.push(Piece::Interp);
                            }
                            self.ctx.push(Ctx::Interp);
                            return true;
                        }
                        '}' => {
                            let span = Span::new(self.pos, self.pos + 1);
                            self.error(
                                diag::E_BARE_BRACE_IN_STRING,
                                span,
                                "gram.lex.str.escape",
                                "a literal `}` in a string is written `}}`",
                            );
                            self.pos += 1;
                        }
                        '\\' => self.scan_escape(),
                        '\n' if matches!(kind, StrKind::Plain) => {
                            // A plain `"…"` does not span lines; `"""` does.
                            let span = Span::new(self.pos, self.pos + 1);
                            self.error(
                                diag::E_UNTERMINATED_STRING,
                                span,
                                "gram.lex.str",
                                "this string reaches the end of its line; use `\"\"\"` to span lines",
                            );
                            return self.abandon_string();
                        }
                        _ => {
                            self.text_push(c);
                            self.pos += c.len_utf8();
                        }
                    }
                }
            }
        }
    }

    fn close_string(&mut self, kind: &StrKind, hashes: usize) -> bool {
        let close_start = self.pos;
        let close_len = match kind {
            StrKind::Multiline => 3,
            StrKind::Raw { .. } => 1 + hashes,
            _ => 1,
        };

        if matches!(kind, StrKind::Multiline) {
            self.apply_dedent(close_start);
        }
        self.flush_text();

        self.pos = close_start + close_len;
        self.push(Tok::StrEnd, Span::new(close_start, self.pos));
        self.ctx.pop();
        true
    }

    /// `[gram.lex.str.multi]`: the closing delimiter's column is the dedent.
    ///
    /// Applied on close, because that is the first moment the column is known.
    /// Every content line must start with at least that much whitespace, which
    /// is stripped; blank lines are exempt (they have nothing to strip and
    /// requiring them to carry the indentation would make trailing-whitespace
    /// hygiene a compile error).
    fn apply_dedent(&mut self, close_start: usize) {
        let line_start = self.src[..close_start].rfind('\n').map_or(0, |nl| nl + 1);
        let prefix = &self.src[line_start..close_start];
        let column = prefix.chars().count();
        let prefix_is_blank = prefix.chars().all(|c| c == ' ' || c == '\t');

        // The closing line's indentation is delimiter, not content.
        if prefix_is_blank && column > 0 {
            let frame = self.frame();
            for _ in 0..column {
                if frame.pending.ends_with([' ', '\t']) {
                    frame.pending.pop();
                } else {
                    break;
                }
            }
        }
        if column == 0 {
            return;
        }

        // Walk the literal's pieces in order, stripping `column` leading
        // whitespace characters at every line start.
        let mut pieces = std::mem::take(&mut self.frame().pieces);
        // The pending tail is one more (not yet emitted) text piece.
        let pending = std::mem::take(&mut self.frame().pending);
        let pending_span = Span::new(self.frame().pending_start, close_start);

        let mut at_line_start = true;
        let mut underrun: Option<Span> = None;

        for piece in &mut pieces {
            match piece {
                Piece::Interp => at_line_start = false,
                Piece::Text(index) => {
                    let span = self.tokens[*index].span;
                    let Tok::StrText(text) = &self.tokens[*index].tok else {
                        continue;
                    };
                    let (stripped, ends_at_line_start, bad) =
                        dedent_chunk(text, column, at_line_start, span);
                    at_line_start = ends_at_line_start;
                    if underrun.is_none() {
                        underrun = bad;
                    }
                    self.tokens[*index].tok = Tok::StrText(stripped);
                }
            }
        }
        let (stripped, _, bad) = dedent_chunk(&pending, column, at_line_start, pending_span);
        if underrun.is_none() {
            underrun = bad;
        }
        let frame = self.frame();
        frame.pieces = pieces;
        frame.pending = stripped;

        if let Some(span) = underrun {
            self.error(
                diag::E_DEDENT_UNDERRUN,
                span,
                "gram.lex.str.multi",
                &format!(
                    "every line of a `\"\"\"` literal must be indented at least to the closing \
                     delimiter's column ({column})"
                ),
            );
        }
    }

    /// Gives up on a string whose body is unterminated, without losing the
    /// stack: the frame is popped and a `StrEnd` closes the token structure so
    /// downstream code never sees an unbalanced literal.
    fn abandon_string(&mut self) -> bool {
        self.flush_text();
        self.push(Tok::StrEnd, Span::empty(self.pos));
        self.ctx.pop();
        true
    }

    fn scan_escape(&mut self) {
        let start = self.pos;
        self.pos += 1; // the backslash
        let Some(c) = self.bump() else {
            self.error(
                diag::E_UNTERMINATED_STRING,
                Span::new(start, self.pos),
                "gram.lex.str",
                "the file ends inside an escape",
            );
            return;
        };
        // `[gram.lex.str]`: \n \t \r \\ \" \0 \x7f \u{1F43A} — a closed set.
        let decoded = match c {
            'n' => Some('\n'),
            't' => Some('\t'),
            'r' => Some('\r'),
            '\\' => Some('\\'),
            '"' => Some('"'),
            '0' => Some('\0'),
            'x' => return self.scan_hex_escape(start),
            'u' => return self.scan_unicode_escape(start),
            _ => None,
        };
        match decoded {
            Some(ch) => {
                let at = start;
                let frame = self.frame();
                if frame.pending.is_empty() {
                    frame.pending_start = at;
                }
                frame.pending.push(ch);
            }
            None => self.error(
                diag::E_BAD_ESCAPE,
                Span::new(start, self.pos),
                "gram.lex.str",
                &format!("`\\{c}` is not an escape; the set is \\n \\t \\r \\\\ \\\" \\0 \\x \\u"),
            ),
        }
    }

    fn scan_hex_escape(&mut self, start: usize) {
        let mut value = 0u32;
        let mut digits = 0;
        while digits < 2 {
            match self.peek().and_then(|c| c.to_digit(16)) {
                Some(d) => {
                    value = value * 16 + d;
                    self.pos += 1;
                    digits += 1;
                }
                None => break,
            }
        }
        if digits != 2 {
            self.error(
                diag::E_BAD_ESCAPE,
                Span::new(start, self.pos),
                "gram.lex.str",
                "`\\x` takes exactly two hex digits",
            );
            return;
        }
        self.push_escaped(start, char::from_u32(value).unwrap_or('\u{fffd}'));
    }

    fn scan_unicode_escape(&mut self, start: usize) {
        if self.peek() != Some('{') {
            self.error(
                diag::E_BAD_UNICODE_ESCAPE,
                Span::new(start, self.pos),
                "gram.lex.str",
                "`\\u` takes a braced scalar value, as in `\\u{1F43A}`",
            );
            return;
        }
        self.pos += 1;
        let mut value: u32 = 0;
        let mut digits = 0;
        let mut overflow = false;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(16)) {
            value = value.saturating_mul(16).saturating_add(d);
            if value > 0x0010_FFFF {
                overflow = true;
            }
            self.pos += 1;
            digits += 1;
        }
        if self.peek() == Some('}') {
            self.pos += 1;
        } else {
            self.error(
                diag::E_BAD_UNICODE_ESCAPE,
                Span::new(start, self.pos),
                "gram.lex.str",
                "this `\\u{…}` escape is missing its closing brace",
            );
            return;
        }
        let scalar = (!overflow && digits > 0)
            .then(|| char::from_u32(value))
            .flatten();
        match scalar {
            Some(ch) => self.push_escaped(start, ch),
            None => self.error(
                diag::E_BAD_UNICODE_ESCAPE,
                Span::new(start, self.pos),
                "gram.lex.str",
                "not a Unicode scalar value",
            ),
        }
    }

    fn push_escaped(&mut self, start: usize, ch: char) {
        let frame = self.frame();
        if frame.pending.is_empty() {
            frame.pending_start = start;
        }
        frame.pending.push(ch);
    }

    // -- format spec mode --------------------------------------------------

    /// Inside a format spec: literal text until the interpolation's `}`, with
    /// nested `{expr}` allowed (`"{m[k]:>{w}}"` — the width is itself
    /// interpolated).
    fn scan_format_spec(&mut self) -> bool {
        let mut text = String::new();
        let start = self.pos;
        loop {
            let Some(c) = self.peek() else {
                if !text.is_empty() {
                    self.push(Tok::FmtText(text), Span::new(start, self.pos));
                }
                return false;
            };
            match c {
                '}' => {
                    if !text.is_empty() {
                        self.push(Tok::FmtText(text), Span::new(start, self.pos));
                    }
                    let span = Span::new(self.pos, self.pos + 1);
                    self.pos += 1;
                    self.ctx.pop();
                    self.push(Tok::InterpEnd, span);
                    return true;
                }
                '{' => {
                    if !text.is_empty() {
                        self.push(Tok::FmtText(text), Span::new(start, self.pos));
                    }
                    let span = Span::new(self.pos, self.pos + 1);
                    self.pos += 1;
                    self.push(Tok::InterpStart, span);
                    self.ctx.push(Ctx::Interp);
                    return true;
                }
                _ => {
                    text.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }
    }

    // -- end of input ------------------------------------------------------

    /// Everything still open when the bytes run out is unterminated. One
    /// diagnostic per open frame, innermost first, so the first one reported is
    /// the one nearest the mistake.
    fn close_unterminated(&mut self) {
        self.open_at_eof = !self.ctx.is_empty();
        while let Some(ctx) = self.ctx.pop() {
            match ctx {
                Ctx::Str(frame) => {
                    let span = Span::new(frame.body_start.saturating_sub(1), self.src.len());
                    self.error(
                        diag::E_UNTERMINATED_STRING,
                        span,
                        "gram.lex.str",
                        "this string literal is never closed",
                    );
                    self.push(Tok::StrEnd, Span::empty(self.src.len()));
                }
                Ctx::Interp | Ctx::Fmt => {
                    self.error(
                        diag::E_UNTERMINATED_INTERP,
                        Span::empty(self.src.len()),
                        "gram.lex.str",
                        "this interpolation is never closed",
                    );
                    self.push(Tok::InterpEnd, Span::empty(self.src.len()));
                }
                Ctx::Paren | Ctx::Bracket | Ctx::AttrBracket | Ctx::Brace => {
                    // Unbalanced brackets are the parser's complaint, not the
                    // lexer's: it will say which production wanted the closer.
                }
            }
        }
    }
}

/// Strips `column` leading whitespace characters at every line start of one
/// text chunk. Returns the stripped text, whether the chunk ends at a line
/// start, and the span of the first line that had too little indentation.
fn dedent_chunk(
    text: &str,
    column: usize,
    mut at_line_start: bool,
    span: Span,
) -> (String, bool, Option<Span>) {
    let mut out = String::with_capacity(text.len());
    let mut underrun = None;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if at_line_start {
            // `c` is the first character of a line: consume up to `column`
            // whitespace characters starting with it.
            if c == '\n' {
                // A blank line: nothing to strip, and nothing to complain about.
                out.push('\n');
                continue;
            }
            let mut taken = 0;
            let mut current = Some(c);
            while taken < column {
                match current {
                    Some(' ' | '\t') => {
                        taken += 1;
                        current = chars.next();
                    }
                    _ => break,
                }
            }
            if taken < column && underrun.is_none() && current.is_some() {
                underrun = Some(span);
            }
            at_line_start = false;
            if let Some(ch) = current {
                if ch == '\n' {
                    at_line_start = true;
                }
                out.push(ch);
            }
            continue;
        }
        if c == '\n' {
            at_line_start = true;
        }
        out.push(c);
    }
    (out, at_line_start, underrun)
}

/// The radix a `0x`/`0o`/`0b` prefix introduces (`[gram.lex.number]`).
fn base_radix(marker: char) -> Option<u32> {
    match marker {
        'x' | 'X' => Some(16),
        'o' | 'O' => Some(8),
        'b' | 'B' => Some(2),
        _ => None,
    }
}

/// XID_Start, approximated by Unicode Alphabetic.
///
/// `[gram.lex.ident]` specifies `XID_Start XID_Continue*`. Pulling a Unicode
/// table in for the difference would be the only dependency in the crate that
/// exists to serve one predicate; `char::is_alphabetic` agrees with XID_Start
/// on every character any wolf program in the corpus uses, and the residue
/// (a handful of Other_ID_Start/NFKC-unstable codepoints) is recorded as a
/// known approximation rather than hidden.
fn is_xid_start(c: char) -> bool {
    c.is_alphabetic()
}

fn is_xid_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.span, self.tok.describe())
    }
}

/// A token-stream dump — **this implementation's own format**, deliberately not
/// modelled on anything the compiler prints.
///
/// One token per line, `start..end  TAG  payload`, with the string mode stack
/// shown as indentation so a nested f-string reads as the tree it is. It exists
/// for snapshot tests and for `wolf-interp lex --dump`; nothing in the protocol
/// consumes it.
#[must_use]
pub fn dump(lexed: &Lexed) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for token in &lexed.tokens {
        if matches!(token.tok, Tok::StrEnd | Tok::InterpEnd) {
            depth = depth.saturating_sub(1);
        }
        let (tag, payload) = dump_parts(&token.tok);
        out.push_str(&format!(
            "{:>5}..{:<5} {}{tag}{}{payload}\n",
            token.span.start,
            token.span.end,
            "  ".repeat(depth),
            if payload.is_empty() { "" } else { " " },
        ));
        if matches!(token.tok, Tok::StrStart(_) | Tok::InterpStart) {
            depth += 1;
        }
    }
    for diag in &lexed.errors {
        out.push_str(&format!("!! {} {} {}\n", diag.code, diag.span, diag.anchor));
    }
    for diag in &lexed.deferred {
        out.push_str(&format!(
            "?? {} {} {} (deferred to parse)\n",
            diag.code, diag.span, diag.anchor
        ));
    }
    out
}

fn dump_parts(tok: &Tok) -> (&'static str, String) {
    match tok {
        Tok::Ident(s) => ("ident", s.clone()),
        Tok::Underscore => ("wildcard", String::new()),
        Tok::Kw(k) => ("kw", (*k).to_owned()),
        Tok::Int(s) => ("int", s.clone()),
        Tok::Float(s) => ("float", s.clone()),
        Tok::StrStart(kind) => (
            "str-open",
            match kind {
                StrKind::Plain => "plain".to_owned(),
                StrKind::Multiline => "multiline".to_owned(),
                StrKind::Raw { hashes } => format!("raw#{hashes}"),
                StrKind::Generalized { prefix } => format!("generalized:{prefix}"),
            },
        ),
        Tok::StrText(s) => ("text", format!("{s:?}")),
        Tok::InterpStart => ("interp-open", String::new()),
        Tok::InterpEnd => ("interp-close", String::new()),
        Tok::FmtStart => ("fmt-open", String::new()),
        Tok::FmtText(s) => ("fmt", format!("{s:?}")),
        Tok::StrEnd => ("str-close", String::new()),
        Tok::Term { explicit: true } => ("term", ";".to_owned()),
        Tok::Term { explicit: false } => ("term", "nl".to_owned()),
        Tok::Error => ("error", String::new()),
        other => ("punct", punctuation_spelling(other).to_owned()),
    }
}

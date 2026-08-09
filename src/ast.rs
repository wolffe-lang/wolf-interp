//! The syntax tree.
//!
//! **Our design, for is02's tree-walker** — deliberately not a mirror of the
//! compiler's `wolf_ast`: no lossless trivia, no error nodes, no red/green
//! machinery. Nothing here needs to reprint source or survive an edit; it needs
//! to be walked once, fast, by an evaluator that cites spec clauses in its
//! faults.
//!
//! Two invariants make that cheap:
//!
//! - **every node carries its byte-exact span**, so a fault can point at source
//!   without a side table (D25, `[proto.record.diag]`);
//! - **every node carries the clause anchor of the production that built it**,
//!   so the tree is self-citing and is02's "every rule cites a clause"
//!   discipline costs a field read rather than a lookup table.

use crate::diag::Span;
use crate::lex::StrKind;

/// A name, with the span of the token it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// `path ::= IDENT ('.' IDENT)*` (`[gram.item.use]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: Span,
}

impl Path {
    #[must_use]
    pub fn is_single(&self) -> bool {
        self.segments.len() == 1
    }
}

/// `visibility ::= 'pub' ('(' 'pkg' ')')?` (`[gram.item.unit]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visibility {
    pub package_only: bool,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Literals
// ---------------------------------------------------------------------------

/// A string literal, already through the mode stack: text is decoded, `{{`/`}}`
/// are collapsed, `"""` is dedented, and interpolations are parsed expressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrLit {
    pub kind: StrKind,
    pub parts: Vec<StrPart>,
    pub span: Span,
}

impl StrLit {
    /// The literal text, when the literal has no interpolations — the shape
    /// `import c "…"` and `asm { "…" }` require.
    #[must_use]
    pub fn as_plain_text(&self) -> Option<String> {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                StrPart::Text(text) => out.push_str(text),
                StrPart::Interp(_) => return None,
            }
        }
        Some(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrPart {
    Text(String),
    Interp(Interpolation),
}

/// `INTERP ::= '{' expr FORMAT_SPEC? '}'` (`[gram.lex.str]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpolation {
    pub expr: Box<Expr>,
    /// Present iff a top-level `:` opened a format spec
    /// (`[gram.amb.fmtcolon]`). Its own parts may interpolate: `"{x:>{w}}"`.
    pub format: Option<Vec<FmtPart>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FmtPart {
    Text(String),
    Interp(Box<Expr>),
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

/// A compilation unit: `unit ::= inner_doc* item*` (`[gram.item.unit]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub items: Vec<Item>,
    pub span: Span,
}

/// `attribute ::= '#[' attr (',' attr)* ']'` (`[gram.item.attr]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub attrs: Vec<Attr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    pub path: Path,
    pub input: Option<AttrInput>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrInput {
    /// `(a, b = "c")`
    Args(Vec<AttrArg>),
    /// `= "value"`
    Literal(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrArg {
    Nested(Attr),
    Literal(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub attrs: Vec<Attribute>,
    pub visibility: Option<Visibility>,
    pub kind: ItemKind,
    pub span: Span,
    pub anchor: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    Fn(Box<FnDecl>),
    /// `let` / `var` / `const` — one production shape, three binding forms
    /// (`[gram.item.let]`).
    Binding(Box<Binding>),
    TypeAlias(Box<TypeAlias>),
    Struct(Box<StructDef>),
    Enum(Box<EnumDef>),
    Trait(Box<TraitDef>),
    Impl(Box<ImplDef>),
    Use(Box<UseDecl>),
    /// `import c "stdlib.h"` — the string is a header name, opaque to wolf
    /// (`[gram.item.use]`).
    ImportC(Box<StrLit>),
}

/// `fn_qual ::= 'comptime' | 'extern' STRING | 'export'` (`[gram.item.fn]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FnQual {
    Comptime(Span),
    Extern { abi: StrLit, span: Span },
    Export(Span),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnDecl {
    pub quals: Vec<FnQual>,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub ret: Option<RetType>,
    /// Absent only under `extern` (`[gram.item.fn]`: "Bodyless form (TERM) only
    /// under `extern`").
    pub body: Option<Block>,
    pub span: Span,
}

/// `generic_param ::= IDENT (':' bound)? | IDENT ':' 'type'` (`[gram.item.fn]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericParam {
    pub name: Ident,
    pub bound: Option<Bound>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    /// `bound ::= path ('+' path)*`
    Paths(Vec<Path>),
    /// `IDENT ':' 'type'` — a comptime type parameter (D29).
    TypeOfTypes(Span),
}

/// `param_mode ::= 'mut' | 'take'` (D10 Tier 0; the *absence* is `read`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamMode {
    Mut,
    Take,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub mode: Option<ParamMode>,
    pub kind: ParamKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamKind {
    Named {
        name: Ident,
        ty: Type,
    },
    /// `self`, with an optional field-granular view set:
    /// `fn norm(mut self.{x, y})` (`[gram.item.fn]`).
    SelfParam {
        view_set: Vec<Ident>,
    },
}

/// `ret_type ::= type ('!' error_row)?` (`[gram.item.fn]`, `[gram.type.row]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetType {
    pub ty: Type,
    pub row: Option<ErrorRow>,
    pub span: Span,
}

/// `error_row ::= '{' row_entry (',' row_entry)* (',' '..')? ','? '}'`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorRow {
    pub entries: Vec<RowEntry>,
    /// A trailing `..` — the row is open.
    pub open: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowEntry {
    pub path: Path,
    pub payload: Vec<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Let,
    Var,
    Const,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub kind: BindingKind,
    pub pattern: Pattern,
    pub ty: Option<Type>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAlias {
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub def: TypeDef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeDef {
    Struct(Box<StructDef>),
    Enum(Box<EnumDef>),
    Alias(Type),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDef {
    /// Absent for the anonymous `type N = struct { … }` form.
    pub name: Option<Ident>,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<Field>,
    pub span: Span,
}

/// `field ::= attribute* visibility? IDENT ':' type ','?` — fields are
/// newline-separated declarations, unlike comma-punctuated variants and rows
/// (`[gram.item.trait]`'s "punctuation asymmetry, intentional").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub attrs: Vec<Attribute>,
    pub visibility: Option<Visibility>,
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: Option<Ident>,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub name: Ident,
    pub payload: Vec<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitDef {
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub members: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplDef {
    pub generics: Vec<GenericParam>,
    /// The impl subject is a *type* — `impl[T] List[T] { … }`
    /// (`[gram.item.trait]`). With `for`, this is the trait applied to its
    /// arguments and `subject` below is the implementing type.
    pub trait_or_subject: Type,
    pub subject: Option<Type>,
    pub members: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDecl {
    pub path: Path,
    /// `use std.{fs, net}`
    pub list: Vec<Ident>,
    pub alias: Option<Ident>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type {
    pub kind: Box<TypeKind>,
    pub span: Span,
    pub anchor: &'static str,
}

/// `prefix_type_kw ::= 'shared' | 'handle' | 'weak' | 'distinct'`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixTypeKw {
    Shared,
    Handle,
    Weak,
    Distinct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeKind {
    /// `path type_args?` — `Map[str, int]`, `wrapping[u32]`.
    Path {
        path: Path,
        args: Vec<TypeArg>,
    },
    /// `'!' type` — the error-union constructor (D30, `[gram.amb.bang]`).
    ErrorUnion(Type),
    Prefixed {
        kw: PrefixTypeKw,
        ty: Type,
    },
    /// `'*' type`, raw pointer, unsafe tier.
    RawPointer(Type),
    Dyn(Path),
    Tuple(Vec<Type>),
    Fn {
        params: Vec<Type>,
        ret: Option<RetType>,
    },
    /// `type`, the type of types (comptime, D29).
    TypeOfTypes,
    /// `region`, the type of first-class regions (X4).
    Region,
}

/// `type_arg ::= type | expr` — const generics, disambiguated in sema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeArg {
    Type(Type),
    Expr(Box<Expr>),
}

// ---------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub kind: Box<PatKind>,
    pub span: Span,
    pub anchor: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatKind {
    Wildcard,
    Literal(Box<Expr>),
    Binding(Ident),
    /// `path '(' pattern,* ')'` — payload binding, `BadDigit(e)`.
    Variant {
        path: Path,
        fields: Vec<Pattern>,
    },
    /// A dotted path with no payload, `io.Error`.
    Path(Path),
    Tuple(Vec<Pattern>),
    /// `IDENT '@' closed_pattern`
    At {
        name: Ident,
        pattern: Pattern,
    },
    /// `closed_pattern ('|' closed_pattern)*` — never nested inside an
    /// or-pattern, which is what "closed" buys (`[gram.pat]`).
    Or(Vec<Pattern>),
}

// ---------------------------------------------------------------------------
// Statements & blocks
// ---------------------------------------------------------------------------

/// `block ::= '{' stmt* expr? '}'` (`[gram.expr.block]`).
///
/// The tail expression is the block's value. A trailing statement that is an
/// expression *is* the tail: `[gram.lex.newline]` inserts a terminator after
/// the last line of a block, and Go's rule 2 ("a terminator may be omitted
/// before a closing `}`") makes that terminator meaningless — so the corpus's
/// `tally\n}` and `sum\n}` are values, not discarded statements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stmt {
    pub attrs: Vec<Attribute>,
    pub kind: StmtKind,
    pub span: Span,
    pub anchor: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StmtKind {
    Binding(Box<Binding>),
    /// `place assign_op expr TERM`. Assignment is a **statement**, not an
    /// expression (`[gram.expr.assign]`); "place" is checked in sema, not here.
    Assign {
        place: Expr,
        op: AssignOp,
        value: Expr,
    },
    Defer {
        on_error: bool,
        expr: Expr,
    },
    /// `assume noalias e, e (, e)*` (`[gram.expr.unsafe]`).
    AssumeNoalias(Vec<Expr>),
    Expr(Expr),
    Item(Box<Item>),
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
    pub kind: Box<ExprKind>,
    pub span: Span,
    pub anchor: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// `!` — unary not (`[gram.amb.bang]`, expression position).
    Not,
    Neg,
    /// `&` — a Tier-0 local borrow, second-class at function boundaries.
    Borrow,
    /// `&mut`
    BorrowMut,
    /// `*` — raw-pointer deref, unsafe tier.
    Deref,
    Move,
    Copy,
    /// `shared` — creates a Tier-2 RC cell from a value.
    Shared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Mul,
    Div,
    Rem,
    Add,
    Sub,
    Shl,
    Shr,
    BitAnd,
    BitXor,
    BitOr,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    /// `<=>`
    Cmp,
    And,
    Or,
}

/// `call_arg ::= ('mut' | 'take')? expr` — call-site modes are grammar (X1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    pub mode: Option<ParamMode>,
    pub expr: Expr,
    pub span: Span,
}

/// `index_arg ::= call_arg | prefix_type_kw type | 'region'`.
///
/// `e[…]` is **one** postfix form (`[gram.amb.brackets]`): index or generic
/// application is sema's call (D29), so the tree records the shape and nothing
/// more. The `Type` arm exists only for the two forms no expression can spell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexArg {
    Value(Arg),
    Type(Type),
}

/// `member ::= IDENT | INT | reserved_kw` — member position is
/// keyword-transparent (`[gram.expr.primary]`): `.take(n)` and `s.spawn(…)`
/// parse because member names live in their own namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    Named(Ident),
    /// Tuple access: `pair.0`.
    Index(u64, Span),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInit {
    pub name: Ident,
    /// `None` for the `Point { x }` shorthand, which binds the field from the
    /// identifier.
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureParam {
    pub mode: Option<ParamMode>,
    pub name: Ident,
    pub ty: Option<Type>,
    pub span: Span,
}

/// `region_strategy ::= 'rc' | 'pool' '(' type ')'` — contextual keywords
/// (`[gram.inv.ctx]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionStrategy {
    Rc(Span),
    Pool { ty: Type, span: Span },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    /// Whether the body was written as a block — the arm-separator rule turns
    /// on it (`[gram.expr.flow]`).
    pub block_bodied: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectArm {
    pub kind: SelectArmKind,
    pub body: Expr,
    pub block_bodied: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectArmKind {
    /// `pattern 'from' expr`
    Recv { pattern: Pattern, channel: Expr },
    /// `'timeout' '(' expr ')'`
    Timeout(Expr),
}

/// `asm_operand ::= IDENT '=' asm_dir '(' asm_constraint ')' expr
///                | asm_dir '(' asm_constraint ')' expr`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmOperand {
    pub name: Option<Ident>,
    pub direction: AsmDir,
    pub constraint: Ident,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsmDir {
    In,
    Out,
    InOut,
    LateOut,
}

/// The `else` defaulting operator's right-hand side (`[gram.expr.primary]`,
/// tier 15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElseHandler {
    /// `expr else block`
    Block(Block),
    /// `expr else expr`
    Expr(Expr),
    /// `expr else |pat| (expr | block)` — `closed_pattern`, so the or-bar and
    /// the closing delimiter stay unambiguous (`[gram.pat]`).
    Handler { pattern: Pattern, body: Expr },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprKind {
    Int(String),
    Float(String),
    Bool(bool),
    Str(Box<StrLit>),
    Path(Path),
    /// `_` — the wildcard, legal in patterns and as an asm/discard position.
    Wildcard,

    StructLit {
        path: Path,
        fields: Vec<FieldInit>,
    },
    Tuple(Vec<Expr>),
    /// `(e)` grouping. Kept distinct from a 1-tuple: `(a,)` is the one-tuple,
    /// `(a)` is `a` in parentheses — and the difference is what makes
    /// `if x == (Point { x: 0 })` legal where the bare literal is not
    /// (`[gram.amb.structlit]`).
    Group(Expr),
    Block(Block),

    Unary {
        op: UnOp,
        operand: Expr,
    },
    Binary {
        op: BinOp,
        lhs: Expr,
        rhs: Expr,
    },
    Cast {
        expr: Expr,
        ty: Type,
    },

    Call {
        callee: Expr,
        args: Vec<Arg>,
    },
    /// `e[…]` — index or generic application, one shape
    /// (`[gram.amb.brackets]`).
    BracketApply {
        base: Expr,
        args: Vec<IndexArg>,
    },
    Member {
        base: Expr,
        member: Member,
    },
    /// Postfix `?` — error propagation (D30).
    Try(Expr),

    Range {
        start: Option<Expr>,
        end: Option<Expr>,
        inclusive: bool,
    },
    /// `^n` — a from-end endpoint (D25): `s[^1]`, `s[..^1]`. There is no `..^`
    /// operator.
    FromEnd(Expr),

    ElseDefault {
        expr: Expr,
        handler: Box<ElseHandler>,
    },

    If {
        cond: Expr,
        then: Block,
        otherwise: Option<Expr>,
    },
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
    },
    For {
        pattern: Pattern,
        iter: Expr,
        body: Block,
    },
    While {
        cond: Expr,
        body: Block,
    },
    Loop {
        body: Block,
    },
    /// `break`/`continue` target the innermost loop — there are no labels at v1
    /// (`[gram.expr.flow]`).
    Return(Option<Expr>),
    Break(Option<Expr>),
    Continue,

    Closure {
        params: Vec<ClosureParam>,
        body: Expr,
        block_bodied: bool,
    },

    /// `region tmp { … }` — create, scope, free at `}`; the name is optional so
    /// `freeze region { … }` builds anonymously and promotes (X4).
    RegionSugar {
        name: Option<Ident>,
        strategy: Option<RegionStrategy>,
        body: Block,
    },
    /// `region()`, `region(rc)` — the first-class value form (X4).
    RegionValue {
        strategy: Option<RegionStrategy>,
    },
    /// `in r { … }` — allocations in the block land in `r`.
    In {
        region: Expr,
        body: Block,
    },
    /// `freeze r` — promote to deep-immutable.
    Freeze(Expr),

    Scope {
        name: Option<Ident>,
        body: Block,
    },
    /// `spawn proc worker(args)`. Task spawning is a *method* on a scope
    /// handle; only procs have a keyword (`[gram.expr.conc]`).
    SpawnProc {
        path: Path,
        args: Vec<Arg>,
    },
    Select {
        arms: Vec<SelectArm>,
    },
    /// `when (a, b) { … }` — ≥2 operands by grammar.
    When {
        operands: Vec<Expr>,
        body: Block,
    },

    Unsafe {
        body: Block,
    },
    /// `unsafe c [captures] { … }` — the body is opaque token text to wolf
    /// (`[gram.expr.unsafe]`); its meaning is c10's.
    UnsafeC {
        captures: Vec<Ident>,
        body_span: Span,
    },
    Asm {
        template: Box<StrLit>,
        operands: Vec<AsmOperand>,
    },
    /// `borrow r from ptr`
    Borrow {
        place: Expr,
        from: Expr,
    },
}

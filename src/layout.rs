//! The C layout and its two modifiers — lupin's half of KWC K4's rest (kw08;
//! wolf-interp#188, is73): `[abi.layout.c]`, `[abi.layout.packed]`,
//! `[abi.layout.align]` and `[abi.layout.query]`.
//!
//! This machine has no byte-level aggregate: a whole-aggregate raw load or
//! store stays refused by name (the clause's own allowance for the checked
//! machine and the reference interpreter). What it does model is the layout
//! as *numbers*: `size_of(T)`, `align_of(T)` and `offset_of(T, field)` are
//! comptime facts every machine computes from the clause, never from
//! codegen, so a packed struct's fields can be written byte by byte at
//! their `offset_of` through a `*u8` exactly as on the compiling tiers.
//!
//! Four readers share [`Repr`] and [`layout_of_name`]:
//!
//! - [`crate::attrs`] refuses a representation that cannot be laid out
//!   (E0820), beside the closed set's E0817s;
//! - [`query_check`] refuses a query on a type with the native layout
//!   (E0708) and an `offset_of` field the struct lacks (E0403);
//! - [`lend_check`] refuses a lend of a packed field (E0819);
//! - the evaluator answers the queries ([`query`]).
//!
//! The codes and spans are the compiler's on all three of its lanes,
//! measured with wolf 0.2.23 (`tests/rulings_is73/`).

use std::collections::BTreeMap;

use crate::ast::{
    Arg, AttrArg, AttrInput, Attribute, Block, Expr, ExprKind, FnDecl, ItemKind, Member, ParamKind,
    ParamMode, PatKind, Stmt, StmtKind, StructDef, Type, TypeKind,
};
use crate::diag::{Diag, Span};
use crate::rowmatch::{Child, each_child_with_blocks};
use crate::sema::{Def, Module, Program};

/// The largest `align(N)` the clause admits: 2^28, gcc's ceiling.
pub const MAX_ALIGN: u64 = 1 << 28;

/// A struct's representation as its well-formed `repr` items spell it.
/// Malformed items are the attribute check's to refuse; this reads only
/// what it can.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Repr {
    pub c: bool,
    pub packed: bool,
    pub align: Option<u64>,
}

/// An integer literal's value, `_` separators and the `0x`/`0o`/`0b`
/// prefixes read.
#[must_use]
pub fn int_literal(expr: &Expr) -> Option<u64> {
    let ExprKind::Int(text) = &*expr.kind else {
        return None;
    };
    let digits: String = text.chars().filter(|c| *c != '_').collect();
    let (radix, body) = if let Some(rest) = digits.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = digits.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = digits.strip_prefix("0b") {
        (2, rest)
    } else {
        (10, digits.as_str())
    };
    u64::from_str_radix(body, radix).ok()
}

/// `align(N)`'s `N` when the item is exactly one integer literal.
#[must_use]
pub fn align_value(input: Option<&AttrInput>) -> Option<u64> {
    let Some(AttrInput::Args(args)) = input else {
        return None;
    };
    let [AttrArg::Literal(lit)] = args.as_slice() else {
        return None;
    };
    int_literal(lit)
}

/// Whether `n` is an alignment the clause admits: a power of two from 1 to
/// 2^28.
#[must_use]
pub fn admissible_align(n: u64) -> bool {
    n.is_power_of_two() && n <= MAX_ALIGN
}

/// Every `repr` item on a node, read across all its `repr` attributes (the
/// compiler merges `#[repr(c)]` and `#[repr(packed)]` written apart).
#[must_use]
pub fn repr_of(attrs: &[Attribute]) -> Repr {
    let mut repr = Repr::default();
    for attr in attrs.iter().flat_map(|a| &a.attrs) {
        if !(attr.path.is_single() && attr.path.segments[0].name == "repr") {
            continue;
        }
        let Some(AttrInput::Args(args)) = &attr.input else {
            continue;
        };
        for arg in args {
            let AttrArg::Nested(inner) = arg else {
                continue;
            };
            if !inner.path.is_single() {
                continue;
            }
            match (inner.path.segments[0].name.as_str(), &inner.input) {
                ("c", None) => repr.c = true,
                ("packed", None) => repr.packed = true,
                ("align", input) => {
                    if let Some(n) = align_value(input.as_ref()).filter(|n| admissible_align(*n)) {
                        repr.align = Some(n);
                    }
                }
                _ => {}
            }
        }
    }
    repr
}

/// A struct declared in a module, with the attributes it was written with.
#[derive(Clone, Copy)]
pub struct StructDecl<'a> {
    pub def: &'a StructDef,
    pub repr: Repr,
}

/// The module's named `struct` items, by name, with their representation.
/// The `type N = struct { … }` form carries no attributes: native layout.
#[must_use]
pub fn structs(module: &Module) -> BTreeMap<String, StructDecl<'_>> {
    let mut out = BTreeMap::new();
    for unit in &module.units {
        for item in &unit.unit.items {
            if let ItemKind::Struct(def) = &item.kind
                && let Some(name) = &def.name
            {
                out.entry(name.name.clone()).or_insert(StructDecl {
                    def,
                    repr: repr_of(&item.attrs),
                });
            }
        }
    }
    out
}

/// A scalar's natural size and alignment (`[abi.layout.query]`): the sized
/// integers, `int`/`uint` as 64-bit, `byte`, `bool`, the floats, and `char`
/// (four bytes, as the compiler answers).
#[must_use]
pub fn scalar(name: &str) -> Option<u64> {
    Some(match name {
        "i8" | "u8" | "byte" | "bool" => 1,
        "i16" | "u16" => 2,
        "i32" | "u32" | "f32" | "char" => 4,
        "i64" | "u64" | "int" | "uint" | "f64" => 8,
        _ => return None,
    })
}

/// The layout the clause gives a type, in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub size: u64,
    pub align: u64,
    /// Each field's name and offset, in declaration order (empty for a
    /// scalar).
    pub fields: Vec<(String, u64)>,
}

/// Why a type has no comptime layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoLayout {
    /// The native layout `[abi.native.layout]` leaves free: E0708.
    Native,
    /// A name this machine cannot read as a type at all: declined by name.
    Unknown,
}

fn round_up(n: u64, align: u64) -> u64 {
    n.div_ceil(align) * align
}

/// The layout of the type a query's first argument names: a scalar, or a
/// struct of `module`.
pub fn layout_of_name(module: &Module, name: &str) -> Result<Layout, NoLayout> {
    let decls = structs(module);
    layout_named(module, &decls, name, 0)
}

fn layout_named(
    module: &Module,
    decls: &BTreeMap<String, StructDecl<'_>>,
    name: &str,
    depth: usize,
) -> Result<Layout, NoLayout> {
    if let Some(n) = scalar(name) {
        return Ok(Layout {
            size: n,
            align: n,
            fields: Vec::new(),
        });
    }
    if name == "str" {
        return Err(NoLayout::Native);
    }
    if let Some(decl) = decls.get(name) {
        if !decl.repr.c || !decl.def.generics.is_empty() || depth > 64 {
            return Err(NoLayout::Native);
        }
        return struct_layout(module, decls, decl, depth);
    }
    match module.items.get(name) {
        Some((Def::Struct(_) | Def::Opaque("enum"), _)) => Err(NoLayout::Native),
        _ => Err(NoLayout::Unknown),
    }
}

/// A field's layout: a raw pointer is 8 aligned 8; a named type is a scalar
/// or a struct of the module; anything else is native.
fn field_layout(
    module: &Module,
    decls: &BTreeMap<String, StructDecl<'_>>,
    ty: &Type,
    depth: usize,
) -> Result<Layout, NoLayout> {
    match &*ty.kind {
        TypeKind::RawPointer(_) => Ok(Layout {
            size: 8,
            align: 8,
            fields: Vec::new(),
        }),
        TypeKind::Path { path, args } if path.is_single() && args.is_empty() => {
            match layout_named(module, decls, &path.segments[0].name, depth + 1) {
                Ok(layout) => Ok(layout),
                // A field whose type this machine cannot read is a field
                // with a native layout as far as the clause is concerned.
                Err(_) => Err(NoLayout::Native),
            }
        }
        _ => Err(NoLayout::Native),
    }
}

fn struct_layout(
    module: &Module,
    decls: &BTreeMap<String, StructDecl<'_>>,
    decl: &StructDecl<'_>,
    depth: usize,
) -> Result<Layout, NoLayout> {
    let mut offset = 0u64;
    let mut strictest = 1u64;
    let mut fields = Vec::new();
    for field in &decl.def.fields {
        let inner = field_layout(module, decls, &field.ty, depth)?;
        let align = if decl.repr.packed { 1 } else { inner.align };
        offset = round_up(offset, align);
        fields.push((field.name.name.clone(), offset));
        offset += inner.size;
        strictest = strictest.max(align);
    }
    let align = decl.repr.align.map_or(strictest, |n| strictest.max(n));
    Ok(Layout {
        size: round_up(offset, align),
        align,
        fields,
    })
}

/// Whether a value of type `ty` holds an `align(N)` struct by value, at any
/// depth — what a packed struct may not hold (`[abi.layout.packed]`, E0820).
#[must_use]
pub fn holds_aligned(decls: &BTreeMap<String, StructDecl<'_>>, ty: &Type, depth: usize) -> bool {
    let TypeKind::Path { path, args } = &*ty.kind else {
        return false;
    };
    if !path.is_single() || !args.is_empty() || depth > 64 {
        return false;
    }
    let Some(decl) = decls.get(&path.segments[0].name) else {
        return false;
    };
    decl.repr.align.is_some()
        || decl
            .def
            .fields
            .iter()
            .any(|field| holds_aligned(decls, &field.ty, depth + 1))
}

/// The three queries (`[abi.layout.query]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query {
    Size,
    Align,
    Offset,
}

impl Query {
    #[must_use]
    pub fn named(name: &str) -> Option<Query> {
        match name {
            "size_of" => Some(Query::Size),
            "align_of" => Some(Query::Align),
            "offset_of" => Some(Query::Offset),
            _ => None,
        }
    }

    fn arity(self) -> usize {
        match self {
            Query::Size | Query::Align => 1,
            Query::Offset => 2,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Query::Size => "size_of",
            Query::Align => "align_of",
            Query::Offset => "offset_of",
        }
    }
}

/// A call this module reads as a layout query: the callee is the bare name
/// of one of the three, the module defines no item of that name, and the
/// argument count is the query's. Anything else is an ordinary call.
#[must_use]
pub fn as_query<'e>(module: &Module, callee: &Expr, args: &'e [Arg]) -> Option<(Query, &'e [Arg])> {
    let ExprKind::Path(path) = &*callee.kind else {
        return None;
    };
    if !path.is_single() {
        return None;
    }
    let name = path.segments[0].name.as_str();
    let query = Query::named(name)?;
    if module.items.contains_key(name) || args.len() != query.arity() {
        return None;
    }
    Some((query, args))
}

fn single_name(expr: &Expr) -> Option<&str> {
    match &*expr.kind {
        ExprKind::Path(path) if path.is_single() => Some(path.segments[0].name.as_str()),
        _ => None,
    }
}

/// What a query answers, or why it cannot: `Err(Some(diag))` is the static
/// refusal (E0708, E0403) at the compiler's span, `Err(None)` a shape this
/// machine declines by name.
fn answer(
    module: &Module,
    query: Query,
    args: &[Arg],
    call_span: Span,
) -> Result<u64, Option<Diag>> {
    let ty = single_name(&args[0].expr).ok_or(None)?;
    let layout = match layout_of_name(module, ty) {
        Ok(layout) => layout,
        Err(NoLayout::Native) => {
            return Err(Some(Diag::new(
                "E0708",
                call_span,
                "abi.layout.c",
                format!(
                    "`{}({ty})` has no answer at compile time: `{ty}` has the native layout, \
                     which [abi.native.layout] leaves free. The layout queries answer for \
                     scalars and `#[repr(c)]` structs whose fields all have a C layout \
                     ([abi.layout.query]); write `#[repr(c)]` on the struct to fix its layout",
                    query.name()
                ),
            )));
        }
        Err(NoLayout::Unknown) => return Err(None),
    };
    match query {
        Query::Size => Ok(layout.size),
        Query::Align => Ok(layout.align),
        Query::Offset => {
            let field_arg = &args[1].expr;
            let Some(field) = single_name(field_arg) else {
                return match &*field_arg.kind {
                    ExprKind::Path(_) => Err(None),
                    _ => Err(Some(e0403(
                        field_arg.span,
                        ty,
                        "`offset_of`'s second argument is a field NAME, not an expression",
                    ))),
                };
            };
            layout
                .fields
                .iter()
                .find(|(name, _)| name == field)
                .map(|(_, offset)| *offset)
                .ok_or_else(|| {
                    Some(e0403(
                        call_span,
                        ty,
                        &format!("`{ty}` has no field `{field}`"),
                    ))
                })
        }
    }
}

fn e0403(span: Span, ty: &str, why: &str) -> Diag {
    Diag::new(
        "E0403",
        span,
        "abi.layout.c",
        format!(
            "{why}: `offset_of({ty}, field)` names one of `{ty}`'s fields ([abi.layout.query])"
        ),
    )
}

/// The evaluator's half: the query's value, or the reason it is declined
/// (a shape the static check let through because it is not this machine's
/// to judge — a type alias, an unknown name, a path for a field).
pub fn query(module: &Module, query: Query, args: &[Arg], call_span: Span) -> Result<u64, String> {
    answer(module, query, args, call_span).map_err(|refusal| match refusal {
        Some(diag) => format!("{} ({})", diag.message, diag.code),
        None => format!(
            "`{}` over this argument: a layout query names a scalar or a struct of this \
             module by its bare name ([abi.layout.query]), and this machine reads no other \
             type expression here",
            query.name()
        ),
    })
}

// ---------------------------------------------------------------------------
// The static walks: E0708/E0403 at every query, E0819 at every packed lend
// ---------------------------------------------------------------------------

/// A body the static walks read: a fn's block, or a module initializer.
enum Body<'a> {
    Fn(&'a FnDecl, &'a Block),
    Init(&'a Expr),
}

/// Every fn body and module initializer of a module, in a fixed order.
fn each_body<'a>(module: &'a Module, mut visit: impl FnMut(Body<'a>)) {
    for (def, _) in module.items.values() {
        match def {
            Def::Fn(decl) => {
                if let Some(body) = &decl.body {
                    visit(Body::Fn(decl, body));
                }
            }
            Def::Binding(binding) => visit(Body::Init(&binding.value)),
            _ => {}
        }
    }
    for methods in module.methods.values() {
        for method in methods.values().flatten() {
            if let Some(body) = &method.decl.body {
                visit(Body::Fn(&method.decl, body));
            }
        }
    }
}

/// E0708 and E0403 at every layout query the program writes, every one of
/// them in source order, as the compiler lists them. The first module that
/// earns any is reported (the record names one file's spans).
#[must_use]
pub fn query_check(program: &Program) -> Vec<Diag> {
    for module in program.modules.values() {
        let mut out = Vec::new();
        each_body(module, |body| {
            let mut visit = |e: &Expr| query_walk(module, e, &mut out);
            match body {
                Body::Fn(_, block) => walk_block_exprs(block, &mut visit),
                Body::Init(expr) => visit(expr),
            }
        });
        if !out.is_empty() {
            out.sort_by_key(|d| (d.span.start, d.span.end));
            out.dedup_by_key(|d| (d.span.start, d.span.end, d.code));
            return out;
        }
    }
    Vec::new()
}

fn walk_block_exprs(block: &Block, visit: &mut dyn FnMut(&Expr)) {
    for stmt in &block.stmts {
        walk_stmt_exprs(stmt, visit);
    }
    if let Some(tail) = &block.tail {
        visit(tail);
    }
}

fn walk_stmt_exprs(stmt: &Stmt, visit: &mut dyn FnMut(&Expr)) {
    match &stmt.kind {
        StmtKind::Binding(binding) => visit(&binding.value),
        StmtKind::Assign { place, value, .. } => {
            visit(place);
            visit(value);
        }
        StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => visit(expr),
        StmtKind::AssumeNoalias(operands) => operands.iter().for_each(&mut *visit),
        StmtKind::Item(item) => match &item.kind {
            ItemKind::Binding(binding) => visit(&binding.value),
            ItemKind::Fn(decl) => {
                if let Some(body) = &decl.body {
                    walk_block_exprs(body, visit);
                }
            }
            _ => {}
        },
    }
}

fn query_walk(module: &Module, expr: &Expr, out: &mut Vec<Diag>) {
    if let ExprKind::Call { callee, args } = &*expr.kind
        && let Some((query, args)) = as_query(module, callee, args)
        && let Err(Some(diag)) = answer(module, query, args, expr.span)
    {
        out.push(diag);
    }
    each_child_with_blocks(expr, &mut |child| match child {
        Child::Expr(child) => query_walk(module, child, out),
        Child::Block(block) => walk_block_exprs(block, &mut |e| query_walk(module, e, out)),
    });
}

/// E0819 (`[abi.layout.packed]`): a packed field is never lent. A `mut`
/// argument whose place runs through a field of a packed struct, or an
/// aggregate passed `read` (the unmarked mode) that does, hands the callee
/// an address that may be misaligned for the field's type. Every site is
/// reported, as the compiler lists them.
///
/// Sema-lite has no types, so the walk knows a local's struct only where
/// the syntax says it — a typed parameter, an annotated `let`/`var`, or a
/// struct literal initializer — and judges nothing else: a lend it cannot
/// type is the dynamic machine's, which has no address to misalign.
#[must_use]
pub fn lend_check(program: &Program) -> Vec<Diag> {
    for module in program.modules.values() {
        let decls = structs(module);
        if !decls.values().any(|d| d.repr.packed) {
            continue;
        }
        let mut walk = LendWalk {
            decls: &decls,
            scopes: Vec::new(),
            out: Vec::new(),
        };
        each_body(module, |body| {
            walk.scopes = vec![Vec::new()];
            match body {
                Body::Fn(decl, block) => {
                    for param in &decl.params {
                        if let ParamKind::Named { name, ty } = &param.kind {
                            walk.declare(&name.name, type_name(ty));
                        }
                    }
                    walk.block(block);
                }
                Body::Init(expr) => walk.expr(expr),
            }
        });
        if !walk.out.is_empty() {
            let mut out = walk.out;
            out.sort_by_key(|d| (d.span.start, d.span.end));
            out.dedup_by_key(|d| (d.span.start, d.span.end));
            return out;
        }
    }
    Vec::new()
}

fn type_name(ty: &Type) -> Option<String> {
    match &*ty.kind {
        TypeKind::Path { path, args } if path.is_single() && args.is_empty() => {
            Some(path.segments[0].name.clone())
        }
        _ => None,
    }
}

struct LendWalk<'a> {
    decls: &'a BTreeMap<String, StructDecl<'a>>,
    /// Local → the struct its syntax names, when it names one.
    scopes: Vec<Vec<(String, Option<String>)>>,
    out: Vec<Diag>,
}

impl LendWalk<'_> {
    fn declare(&mut self, name: &str, ty: Option<String>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name.to_owned(), ty));
        }
    }

    fn local(&self, name: &str) -> Option<Option<String>> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|(n, _)| n == name)
            .map(|(_, ty)| ty.clone())
    }

    /// A field place's facts: the type its last field names, and whether
    /// any step reads a field of a packed struct. `None` when the head is
    /// not a local this walk can type.
    fn place(&self, expr: &Expr) -> Option<(Option<String>, bool)> {
        match &*expr.kind {
            ExprKind::Group(inner) => self.place(inner),
            // `[gram.item.use]`'s path production swallows the dots:
            // `d.base` arrives as a two-segment path.
            ExprKind::Path(path) => {
                let head = self.local(&path.segments[0].name)??;
                let mut steps = (Some(head), false);
                for segment in &path.segments[1..] {
                    steps = self.step(steps, &segment.name)?;
                }
                Some(steps)
            }
            ExprKind::Member {
                base,
                member: Member::Named(name),
            } => {
                let base = self.place(base)?;
                self.step(base, &name.name)
            }
            _ => None,
        }
    }

    fn step(
        &self,
        (ty, packed): (Option<String>, bool),
        field: &str,
    ) -> Option<(Option<String>, bool)> {
        let decl = self.decls.get(ty.as_deref()?)?;
        let field = decl.def.fields.iter().find(|f| f.name.name == field)?;
        Some((type_name(&field.ty), packed || decl.repr.packed))
    }

    fn args(&mut self, args: &[Arg]) {
        for arg in args {
            let Some((ty, packed)) = self.place(&arg.expr) else {
                continue;
            };
            if !packed {
                continue;
            }
            let aggregate = ty.as_deref().is_some_and(|t| self.decls.contains_key(t));
            let lent = match arg.mode {
                Some(ParamMode::Mut) => true,
                None => aggregate,
                Some(ParamMode::Take) => false,
            };
            if lent {
                self.out.push(Diag::new(
                    "E0819",
                    arg.expr.span,
                    "abi.layout.c",
                    "a field of a packed struct is never lent: a `mut` argument, or an \
                     aggregate passed `read`, hands the callee the field's address, which may \
                     be misaligned for its type ([abi.layout.packed]). Copy the field out, lend \
                     the copy, and write it back",
                ));
            }
        }
    }

    fn block(&mut self, block: &Block) {
        self.scopes.push(Vec::new());
        for stmt in &block.stmts {
            match &stmt.kind {
                StmtKind::Binding(binding) => {
                    self.expr(&binding.value);
                    if let PatKind::Binding(ident) = &*binding.pattern.kind {
                        let ty =
                            binding.ty.as_ref().and_then(type_name).or_else(|| {
                                match &*binding.value.kind {
                                    ExprKind::StructLit { path, .. } if path.is_single() => {
                                        Some(path.segments[0].name.clone())
                                    }
                                    _ => None,
                                }
                            });
                        self.declare(&ident.name, ty);
                    }
                }
                _ => walk_stmt_exprs(stmt, &mut |e| self.expr(e)),
            }
        }
        if let Some(tail) = &block.tail {
            self.expr(tail);
        }
        self.scopes.pop();
    }

    fn expr(&mut self, expr: &Expr) {
        match &*expr.kind {
            ExprKind::Call { callee, args } => {
                self.args(args);
                self.expr(callee);
                for arg in args {
                    self.expr(&arg.expr);
                }
            }
            ExprKind::Closure { params, body, .. } => {
                self.scopes.push(Vec::new());
                for param in params {
                    let ty = param.ty.as_ref().and_then(type_name);
                    self.declare(&param.name.name, ty);
                }
                self.expr(body);
                self.scopes.pop();
            }
            _ => each_child_with_blocks(expr, &mut |child| match child {
                Child::Expr(child) => self.expr(child),
                Child::Block(block) => self.block(block),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(source: &str, name: &str) -> Result<Layout, NoLayout> {
        let program = crate::sema::load_source("main.lu", source).expect("loads");
        layout_of_name(program.root(), name)
    }

    const C: &str = "#[repr(c)]\nstruct C3 {\n    a: u8,\n    b: u32,\n    c: u8,\n}\n\n\
                     #[repr(c, packed)]\nstruct P3 {\n    a: u8,\n    b: u32,\n    c: u8,\n}\n\n\
                     #[repr(c, align(16))]\nstruct A16 {\n    x: u32,\n}\n\n\
                     #[repr(c)]\nstruct Outer {\n    tag: u8,\n    inner: A16,\n    z: u16,\n}\n\n\
                     #[repr(c)]\nstruct PNest {\n    a: u8,\n    p: P3,\n    z: u16,\n}\n\n\
                     struct Native {\n    x: f64,\n}\n\n\
                     #[repr(c)]\nstruct Holds {\n    n: Native,\n}\n\n\
                     fn main() -> int {\n    0\n}\n";

    fn numbers(layout: &Layout) -> (u64, u64, Vec<u64>) {
        (
            layout.size,
            layout.align,
            layout.fields.iter().map(|(_, o)| *o).collect(),
        )
    }

    #[test]
    fn the_clause_layouts_are_cs() {
        // gcc 16.2.1 and clang 23.1.1 print these for the same declarations
        // (kw08's `layout_query_repr_c.lu`).
        assert_eq!(numbers(&layout(C, "C3").unwrap()), (12, 4, vec![0, 4, 8]));
        assert_eq!(numbers(&layout(C, "P3").unwrap()), (6, 1, vec![0, 1, 5]));
        assert_eq!(numbers(&layout(C, "A16").unwrap()), (16, 16, vec![0]));
        assert_eq!(
            numbers(&layout(C, "Outer").unwrap()),
            (48, 16, vec![0, 16, 32])
        );
        assert_eq!(
            numbers(&layout(C, "PNest").unwrap()),
            (10, 2, vec![0, 1, 8])
        );
    }

    #[test]
    fn a_native_layout_has_no_comptime_answer() {
        assert_eq!(layout(C, "Native"), Err(NoLayout::Native));
        assert_eq!(layout(C, "Holds"), Err(NoLayout::Native));
        assert_eq!(layout(C, "str"), Err(NoLayout::Native));
        assert_eq!(layout(C, "Nope"), Err(NoLayout::Unknown));
    }

    #[test]
    fn scalars_are_their_natural_size() {
        for (name, n) in [("u8", 1), ("i16", 2), ("f32", 4), ("char", 4), ("int", 8)] {
            assert_eq!(numbers(&layout(C, name).unwrap()), (n, n, vec![]), "{name}");
        }
    }

    #[test]
    fn alignments_the_clause_admits() {
        assert!(admissible_align(1));
        assert!(admissible_align(MAX_ALIGN));
        assert!(!admissible_align(0));
        assert!(!admissible_align(3));
        assert!(!admissible_align(MAX_ALIGN * 2));
    }
}

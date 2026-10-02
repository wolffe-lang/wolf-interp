//! A `match` over a fallible value, and a `?` under a `defer` — the static
//! readings of the maintainer's rulings #21 and #19 of 2026-10-02 (is67;
//! wolffe-lang/wolf-lang#497 and #498).
//!
//! **`[type.row.match]`** (ruling #21, the clause s197 writes): the scrutinee
//! has type `T ! {row}`; an arm is a *row arm* (a tag of the row by name,
//! binding its payload when it has one: `none => …`, `Io(e) => …`) or a
//! *value arm* (any pattern over `T`); an identifier that names a tag of the
//! scrutinee's row is a row arm, anything else a value pattern; `_` covers
//! what is left on both halves; the match must cover every tag of the row
//! and the whole of `T`, and E0801 names the missing tag or the uncovered
//! value half; the row is consumed; a tag that is also a constructor name
//! reachable from `T` is refused by name, never guessed.
//!
//! **`[type.row.defer]`** (ruling #19, s196 — wolf-lang PR #509): a `?`
//! inside a `defer` or `errdefer` expression is **E0611** at compile time on
//! every machine; the refusal reads the whole deferred expression (a call
//! argument, an interpolation hole, a binding, a block), and a `?` inside a
//! closure defined under the `defer` is that closure's own propagation.
//!
//! # What this machine can know
//!
//! Sema-lite (`sema`) checks no types, so the one question both rulings turn
//! on — *is this scrutinee a `T ! {row}`?* — is answered from what a
//! signature or a binding spells, and from nothing else: a call to a module
//! `fn` (own or `use`d) by its declared return row, a call to a builtin with
//! a declared row, a `Map` index (`V ! {none}`, `[mem.map.absent]`), a local
//! bound by such an initializer or by a `T ! {row}` annotation, and a
//! parenthesized one. A method call, a closure call, an operator — anything
//! the reader cannot name — is **not** statically fallible, and a `match`
//! over it keeps the behaviour it had before this module existed. The sema
//! boundary's rule: a guess never becomes a verdict.
//!
//! One reader serves three callers, so they cannot drift: the evaluator
//! (`eval::Machine` dispatches a row value to the row arms and a value to the
//! value arms), the lint (a row arm is not a binder; E0802 says nothing about
//! a row match) and the resolve-rung checks here ([`row_match_check`] for
//! E0801, [`collision_refusal`] for the by-name refusal, [`defer_try_check`]
//! for E0611).

use std::collections::BTreeMap;

use crate::ast::{
    Binding, Block, ElseHandler, Expr, ExprKind, FnDecl, IndexArg, Item, ItemKind, MatchArm,
    ParamKind, PatKind, Path, Pattern, RetType, Stmt, StmtKind, StrPart, Type, TypeArg, TypeKind,
};
use crate::diag::{Diag, Span};
use crate::sema::{Def, Module, Program};

/// What a binding or an expression is statically known to hold, as far as
/// this reader can tell from a signature or an annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Known {
    /// A `T ! {row}`.
    Fallible(RowTy),
    /// A `Map[K, V]`: an index of it is `V ! {none}`.
    Map { value: Type },
}

/// A fallible type, read off a signature: its row and, when spelled, its ok
/// type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowTy {
    /// The row's single-segment tags, in declaration order. A qualified
    /// entry (`io.Eof`) is not a tag this reader names, so a row spelling
    /// one is never judged exhaustive — see [`RowTy::open`].
    pub tags: Vec<String>,
    /// The row needs a `_`: it is open (`..`), or spells an entry this reader
    /// cannot name.
    pub open: bool,
    /// The ok type `T`, when a signature spells it (a builtin's row carries
    /// none).
    pub ok: Option<Type>,
}

impl RowTy {
    /// `{none, stale}` — the row as a diagnostic spells it.
    #[must_use]
    pub fn render(&self) -> String {
        let mut parts = self.tags.clone();
        if self.open {
            parts.push("..".to_owned());
        }
        format!("{{{}}}", parts.join(", "))
    }
}

/// The row a `type ! error_row` spells, with the ok type beside it.
fn row_of(ty: &Type, row: &crate::ast::ErrorRow) -> RowTy {
    let mut tags = Vec::new();
    let mut unnamed = false;
    for entry in &row.entries {
        match entry.path.segments.as_slice() {
            [segment] => tags.push(segment.name.clone()),
            _ => unnamed = true,
        }
    }
    RowTy {
        tags,
        open: row.open || unnamed,
        ok: Some(ty.clone()),
    }
}

/// What an annotation or a parameter type says the binding holds.
#[must_use]
pub fn known_of_type(ty: &Type) -> Option<Known> {
    match &*ty.kind {
        TypeKind::Fallible { ty: inner, row } => Some(Known::Fallible(row_of(inner, row))),
        // `!T`: the error-union constructor with no row spelled — open.
        TypeKind::ErrorUnion(inner) => Some(Known::Fallible(RowTy {
            tags: Vec::new(),
            open: true,
            ok: Some(inner.clone()),
        })),
        TypeKind::Path { path, args } => {
            let [head] = path.segments.as_slice() else {
                return None;
            };
            if head.name != "Map" {
                return None;
            }
            let [_, TypeArg::Type(value)] = args.as_slice() else {
                return None;
            };
            Some(Known::Map {
                value: value.clone(),
            })
        }
        _ => None,
    }
}

/// The row a declared return type spells, in either spelling: `-> T ! {row}`
/// folded into the type, or `ret_type`'s own `'!' error_row`.
#[must_use]
pub fn fallible_of_ret(ret: &RetType) -> Option<RowTy> {
    match (&ret.row, &*ret.ty.kind) {
        (Some(row), _) => Some(row_of(&ret.ty, row)),
        (None, _) => match known_of_type(&ret.ty) {
            Some(Known::Fallible(row)) => Some(row),
            _ => None,
        },
    }
}

/// The row a declared `fn` returns.
#[must_use]
pub fn fallible_of_decl(decl: &FnDecl) -> Option<RowTy> {
    decl.ret.as_ref().and_then(fallible_of_ret)
}

/// The callee a call's path names, at the resolve rung: the module's own
/// `fn`, a `use`d module's `pub fn`, or an ambient builtin with a declared
/// row. A single-segment name a LOCAL shadows is the caller's question
/// (`locals` is asked first by [`known_of_expr`]'s callers).
#[must_use]
pub fn callee_row(program: &Program, module: &str, path: &Path) -> Option<RowTy> {
    match path.segments.as_slice() {
        [name] => {
            if let Some(Def::Fn(decl)) = program.lookup(module, &name.name, false) {
                return fallible_of_decl(decl);
            }
            let row = crate::eval::builtin::declared_row(&name.name);
            (!row.is_empty()).then(|| RowTy {
                tags: row.iter().map(|tag| (*tag).to_owned()).collect(),
                open: false,
                ok: None,
            })
        }
        [head, name] => {
            let home = program.modules.get(module)?;
            let target = home
                .use_paths
                .iter()
                .find(|(bound, _)| bound == &head.name)
                .map(|(_, segments)| segments.join("."))
                .or_else(|| {
                    program
                        .modules
                        .contains_key(&head.name)
                        .then(|| head.name.clone())
                })?;
            match program.lookup(&target, &name.name, true) {
                Some(Def::Fn(decl)) => fallible_of_decl(decl),
                _ => None,
            }
        }
        _ => None,
    }
}

/// A bracket argument read as a type: `Map[str, int]()` parses its arguments
/// as EXPRESSIONS where a bare name is ambiguous (`[gram.amb.brackets]`), so
/// a single-segment path there is the type it spells.
fn type_of_index_arg(arg: &IndexArg) -> Option<Type> {
    match arg {
        IndexArg::Type(ty) => Some(ty.clone()),
        IndexArg::Value(arg) => match &*arg.expr.kind {
            ExprKind::Path(path) => Some(Type {
                kind: Box::new(TypeKind::Path {
                    path: path.clone(),
                    args: Vec::new(),
                }),
                span: arg.expr.span,
                anchor: "gram.type",
            }),
            _ => None,
        },
    }
}

/// What an expression is statically known to hold: the reader both machines
/// share. `locals` answers for a single-segment name (what its binding
/// recorded), `callee` for a call's path.
#[must_use]
pub fn known_of_expr(
    expr: &Expr,
    locals: &dyn Fn(&str) -> Option<Known>,
    callee: &dyn Fn(&Path) -> Option<RowTy>,
) -> Option<Known> {
    match &*expr.kind {
        ExprKind::Group(inner) => known_of_expr(inner, locals, callee),
        ExprKind::Path(path) if path.is_single() => locals(&path.segments[0].name),
        ExprKind::Call { callee: head, .. } => match &*head.kind {
            ExprKind::Path(path) => {
                // A local shadows an item of the same name.
                if path.is_single() && locals(&path.segments[0].name).is_some() {
                    return None;
                }
                callee(path).map(Known::Fallible)
            }
            // `Map[K, V]()` — the constructor names the map's type.
            ExprKind::BracketApply { base, args, .. } => {
                let ExprKind::Path(path) = &*base.kind else {
                    return None;
                };
                let [head] = path.segments.as_slice() else {
                    return None;
                };
                if head.name != "Map" {
                    return None;
                }
                let [_, value] = args.as_slice() else {
                    return None;
                };
                Some(Known::Map {
                    value: type_of_index_arg(value)?,
                })
            }
            _ => None,
        },
        // `m[k]` on a `Map`: `V ! {none}` (`[mem.map.absent]`).
        ExprKind::BracketApply { base, args, .. } => {
            let [IndexArg::Value(arg)] = args.as_slice() else {
                return None;
            };
            if matches!(&*arg.expr.kind, ExprKind::Range { .. }) {
                return None;
            }
            match known_of_expr(base, locals, callee)? {
                Known::Map { value } => Some(Known::Fallible(RowTy {
                    tags: vec!["none".to_owned()],
                    open: false,
                    ok: Some(value),
                })),
                Known::Fallible(_) => None,
            }
        }
        _ => None,
    }
}

/// The row a `match` scrutinee is statically known to carry — `Some` exactly
/// when the match is a ROW MATCH under `[type.row.match]`.
#[must_use]
pub fn fallible_of_expr(
    expr: &Expr,
    locals: &dyn Fn(&str) -> Option<Known>,
    callee: &dyn Fn(&Path) -> Option<RowTy>,
) -> Option<RowTy> {
    match known_of_expr(expr, locals, callee)? {
        Known::Fallible(row) => Some(row),
        Known::Map { .. } => None,
    }
}

/// What a binding statically records for its name: the annotation first,
/// the initializer's reading otherwise.
#[must_use]
pub fn known_of_binding(
    binding: &Binding,
    locals: &dyn Fn(&str) -> Option<Known>,
    callee: &dyn Fn(&Path) -> Option<RowTy>,
) -> Option<Known> {
    if let Some(ty) = &binding.ty {
        return known_of_type(ty);
    }
    known_of_expr(&binding.value, locals, callee)
}

/// Which half of a row match an arm's pattern belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Half {
    /// A row arm: the tags it names (one, or an or-pattern's several).
    Row(Vec<String>),
    /// `_` — both halves.
    Both,
    /// A value arm: any pattern over `T`.
    Value,
    /// An or-pattern mixing a row arm with a value arm. Tried on both halves
    /// by the evaluator; never judged statically.
    Mixed,
}

/// An identifier that names a tag of the row is a row arm; a payload pattern
/// whose head names one too; `_` is both; anything else is a value pattern.
#[must_use]
pub fn arm_half(pattern: &Pattern, tags: &[String]) -> Half {
    match &*pattern.kind {
        PatKind::Wildcard => Half::Both,
        PatKind::Binding(ident) if tags.contains(&ident.name) => {
            Half::Row(vec![ident.name.clone()])
        }
        PatKind::Variant { path, .. }
            if path.is_single() && tags.iter().any(|tag| *tag == path.segments[0].name) =>
        {
            Half::Row(vec![path.segments[0].name.clone()])
        }
        PatKind::At { pattern, .. } => arm_half(pattern, tags),
        PatKind::Or(alternatives) => {
            let mut rows = Vec::new();
            let mut values = false;
            for alternative in alternatives {
                match arm_half(alternative, tags) {
                    Half::Both => return Half::Both,
                    Half::Row(names) => rows.extend(names),
                    Half::Value => values = true,
                    Half::Mixed => return Half::Mixed,
                }
            }
            match (rows.is_empty(), values) {
                (false, false) => Half::Row(rows),
                (true, _) => Half::Value,
                (false, true) => Half::Mixed,
            }
        }
        _ => Half::Value,
    }
}

/// Whether a value pattern matches every value of its type: a binder, `_`,
/// an `@`, or a product of such. A bare identifier that names an enum
/// variant in scope (`is_variant`) is a variant pattern, not a binder
/// (`[gram.pat.nullary]`'s resolution rule).
#[must_use]
pub fn irrefutable(pattern: &Pattern, is_variant: &dyn Fn(&str) -> bool) -> bool {
    match &*pattern.kind {
        PatKind::Wildcard => true,
        PatKind::Binding(ident) => !is_variant(&ident.name),
        PatKind::At { pattern, .. } => irrefutable(pattern, is_variant),
        PatKind::Tuple(items) => items.iter().all(|item| irrefutable(item, is_variant)),
        PatKind::Struct { fields, .. } => fields.iter().all(|field| {
            field
                .pattern
                .as_ref()
                .is_none_or(|sub| irrefutable(sub, is_variant))
        }),
        PatKind::Or(alternatives) => alternatives
            .iter()
            .any(|alternative| irrefutable(alternative, is_variant)),
        PatKind::Literal(_)
        | PatKind::Range { .. }
        | PatKind::Path(_)
        | PatKind::Variant { .. } => false,
    }
}

/// The last segment of a type's head path, for naming `T`.
fn head_name(ty: &Type) -> Option<String> {
    crate::sema::head_name(ty)
}

// ---------------------------------------------------------------------------
// The resolve-rung checks
// ---------------------------------------------------------------------------

/// One thing the body walk found, in source order.
#[derive(Debug, Clone)]
enum Finding {
    /// A static rejection — `fail(CODE)` at the resolve rung.
    Diag(Diag),
    /// A refusal by name — `[proto.record.unsupported]`'s conservatism
    /// class, before anything runs.
    Unsupported(String),
}

/// `[type.row.match]`'s exhaustiveness, at the resolve rung: E0801 naming
/// the missing tag or the uncovered value half, as E0805 is answered here
/// (`[proto.cmp.rung]` makes a resolve-rung emission of the checker's code
/// agreement). The first finding in source order wins.
#[must_use]
pub fn row_match_check(program: &Program) -> Option<Diag> {
    findings(program)
        .into_iter()
        .find_map(|finding| match finding {
            Finding::Diag(diag) => Some(diag),
            Finding::Unsupported(_) => None,
        })
}

/// `[type.row.match]`'s collision: a tag of the row that is also a
/// constructor name reachable from `T` is refused by name, never guessed —
/// before anything runs, as `sema::raise_check` refuses. The checker's code
/// for it is s197's; this machine spends none.
#[must_use]
pub fn collision_refusal(program: &Program) -> Option<String> {
    findings(program)
        .into_iter()
        .find_map(|finding| match finding {
            Finding::Unsupported(reason) => Some(reason),
            Finding::Diag(_) => None,
        })
}

/// `[type.row.defer]` (s196, E0611): a `?` anywhere inside a `defer` or
/// `errdefer` expression, outside a closure literal, at the `?` expression's
/// span. Every body is walked — items, impl methods, nested fns, closures.
#[must_use]
pub fn defer_try_check(program: &Program) -> Option<Diag> {
    for module in program.modules.values() {
        for unit in &module.units {
            let mut found: Option<Span> = None;
            for item in &unit.unit.items {
                each_defer_in_item(item, &mut |expr| {
                    if found.is_none() {
                        found = first_try(expr);
                    }
                });
                if let Some(span) = found {
                    return Some(
                        Diag::new(
                            "E0611",
                            span,
                            "type.row.defer",
                            "a `?` inside a `defer` or `errdefer` expression is refused \
                             ([type.row.defer]): the deferred expression runs while the \
                             function is already leaving, so an error propagated from inside \
                             it has nowhere to go. Handle the row inside the deferred \
                             expression (`defer close(f) else |e| note(e)`), or move the \
                             fallible call out of the `defer` into the body, where its `?` \
                             has a function to leave",
                        )
                        .in_file(unit.file.clone()),
                    );
                }
            }
        }
    }
    None
}

/// The first `?` lexically inside `expr`, not counting a closure's body.
fn first_try(expr: &Expr) -> Option<Span> {
    if let ExprKind::Try(_) = &*expr.kind {
        return Some(expr.span);
    }
    if let ExprKind::Closure { .. } = &*expr.kind {
        return None;
    }
    let mut found = None;
    each_child(expr, &mut |child| {
        if found.is_none() {
            found = first_try(child);
        }
    });
    found
}

/// Every `defer`/`errdefer` expression under an item, however deep.
fn each_defer_in_item(item: &Item, on_defer: &mut dyn FnMut(&Expr)) {
    match &item.kind {
        ItemKind::Fn(decl) => {
            if let Some(body) = &decl.body {
                each_defer_in_block(body, on_defer);
            }
        }
        ItemKind::Impl(def) => {
            for member in &def.members {
                each_defer_in_item(member, on_defer);
            }
        }
        ItemKind::Binding(binding) => each_defer_in_expr(&binding.value, on_defer),
        _ => {}
    }
}

fn each_defer_in_block(block: &Block, on_defer: &mut dyn FnMut(&Expr)) {
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Defer { expr, .. } => {
                on_defer(expr);
                each_defer_in_expr(expr, on_defer);
            }
            StmtKind::Binding(binding) => each_defer_in_expr(&binding.value, on_defer),
            StmtKind::Assign { place, value, .. } => {
                each_defer_in_expr(place, on_defer);
                each_defer_in_expr(value, on_defer);
            }
            StmtKind::AssumeNoalias(operands) => {
                for operand in operands {
                    each_defer_in_expr(operand, on_defer);
                }
            }
            StmtKind::Expr(expr) => each_defer_in_expr(expr, on_defer),
            StmtKind::Item(item) => each_defer_in_item(item, on_defer),
        }
    }
    if let Some(tail) = &block.tail {
        each_defer_in_expr(tail, on_defer);
    }
}

fn each_defer_in_expr(expr: &Expr, on_defer: &mut dyn FnMut(&Expr)) {
    each_child_with_blocks(expr, &mut |child| match child {
        Child::Expr(child) => each_defer_in_expr(child, on_defer),
        Child::Block(block) => each_defer_in_block(block, on_defer),
    });
}

/// One direct child of an expression: a sub-expression, or a block whose
/// statements the visitor owns.
enum Child<'a> {
    Expr(&'a Expr),
    Block(&'a Block),
}

/// Every direct child expression of `expr`, a block's statements and tail
/// included (through `each_child_with_blocks` with the block arm folded in).
fn each_child(expr: &Expr, visit: &mut dyn FnMut(&Expr)) {
    each_child_with_blocks(expr, &mut |child| match child {
        Child::Expr(child) => visit(child),
        Child::Block(block) => {
            for stmt in &block.stmts {
                match &stmt.kind {
                    StmtKind::Binding(binding) => visit(&binding.value),
                    StmtKind::Assign { place, value, .. } => {
                        visit(place);
                        visit(value);
                    }
                    StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => visit(expr),
                    StmtKind::AssumeNoalias(operands) => {
                        for operand in operands {
                            visit(operand);
                        }
                    }
                    StmtKind::Item(_) => {}
                }
            }
            if let Some(tail) = &block.tail {
                visit(tail);
            }
        }
    });
}

/// The generic walk: `v` on every direct child expression and on every
/// directly nested block (whose statements are the visitor's).
fn each_child_with_blocks<'a>(expr: &'a Expr, v: &mut dyn FnMut(Child<'a>)) {
    match &*expr.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Char(_)
        | ExprKind::Wildcard
        | ExprKind::Path(_)
        | ExprKind::Continue
        | ExprKind::RegionValue { .. }
        | ExprKind::UnsafeC { .. } => {}
        ExprKind::Str(lit) => {
            for part in &lit.parts {
                if let StrPart::Interp(interp) = part {
                    v(Child::Expr(&interp.expr));
                    if let Some(parts) = &interp.format {
                        for fmt_part in parts {
                            if let crate::ast::FmtPart::Interp(inner) = fmt_part {
                                v(Child::Expr(inner));
                            }
                        }
                    }
                }
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for field in fields {
                v(Child::Expr(&field.value));
            }
        }
        ExprKind::Tuple(items) | ExprKind::List(items) => {
            for item in items {
                v(Child::Expr(item));
            }
        }
        ExprKind::Group(inner)
        | ExprKind::Try(inner)
        | ExprKind::FromEnd(inner)
        | ExprKind::Freeze(inner)
        | ExprKind::Unary { operand: inner, .. }
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Member { base: inner, .. }
        | ExprKind::ModedReceiver { place: inner, .. } => v(Child::Expr(inner)),
        ExprKind::Block(block) => v(Child::Block(block)),
        ExprKind::Binary { lhs, rhs, .. } => {
            v(Child::Expr(lhs));
            v(Child::Expr(rhs));
        }
        ExprKind::Call { callee, args } => {
            v(Child::Expr(callee));
            for arg in args {
                v(Child::Expr(&arg.expr));
            }
        }
        ExprKind::BracketApply { base, args, .. } => {
            v(Child::Expr(base));
            for arg in args {
                if let IndexArg::Value(arg) = arg {
                    v(Child::Expr(&arg.expr));
                }
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(start) = start {
                v(Child::Expr(start));
            }
            if let Some(end) = end {
                v(Child::Expr(end));
            }
        }
        ExprKind::ElseDefault {
            expr: inner,
            handler,
        } => {
            v(Child::Expr(inner));
            match &**handler {
                ElseHandler::Block(block) => v(Child::Block(block)),
                ElseHandler::Expr(fallback) => v(Child::Expr(fallback)),
                ElseHandler::Handler { body, .. } => v(Child::Expr(body)),
            }
        }
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            v(Child::Expr(cond));
            v(Child::Block(then));
            if let Some(otherwise) = otherwise {
                v(Child::Expr(otherwise));
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            v(Child::Expr(scrutinee));
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    v(Child::Expr(guard));
                }
                v(Child::Expr(&arm.body));
            }
        }
        ExprKind::For { iter, body, .. } => {
            v(Child::Expr(iter));
            v(Child::Block(body));
        }
        ExprKind::While { cond, body } => {
            v(Child::Expr(cond));
            v(Child::Block(body));
        }
        ExprKind::Loop { body } | ExprKind::Scope { body, .. } | ExprKind::Unsafe { body } => {
            v(Child::Block(body))
        }
        ExprKind::RegionSugar { cap, body, .. } => {
            if let Some(cap) = cap {
                v(Child::Expr(cap));
            }
            v(Child::Block(body));
        }
        ExprKind::In { region, body } => {
            v(Child::Expr(region));
            v(Child::Block(body));
        }
        ExprKind::When { operands, body } => {
            for operand in operands {
                v(Child::Expr(operand));
            }
            v(Child::Block(body));
        }
        ExprKind::Return(value) | ExprKind::Break(value) => {
            if let Some(value) = value {
                v(Child::Expr(value));
            }
        }
        ExprKind::Closure { body, .. } => v(Child::Expr(body)),
        ExprKind::SpawnProc { args, .. } => {
            for arg in args {
                v(Child::Expr(&arg.expr));
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.kind {
                    crate::ast::SelectArmKind::Recv { channel, .. } => v(Child::Expr(channel)),
                    crate::ast::SelectArmKind::Timeout(deadline) => v(Child::Expr(deadline)),
                }
                v(Child::Expr(&arm.body));
            }
        }
        ExprKind::Asm { operands, .. } => {
            for operand in operands {
                v(Child::Expr(&operand.value));
            }
        }
        ExprKind::Borrow { place, from } => {
            v(Child::Expr(place));
            v(Child::Expr(from));
        }
    }
}

// ---------------------------------------------------------------------------
// The row-match body walk
// ---------------------------------------------------------------------------

/// The walk's view of one module: what resolves a callee and names an enum.
struct Scope<'a> {
    program: &'a Program,
    module: &'a Module,
    /// The enums of this module, each with its variants in declaration
    /// order — `Module::variants` inverted.
    enums: BTreeMap<String, Vec<String>>,
    file: &'a str,
    /// Locals, innermost last; a name maps to what its binding recorded.
    locals: Vec<Vec<(String, Option<Known>)>>,
    findings: Vec<Finding>,
}

impl Scope<'_> {
    fn local(&self, name: &str) -> Option<Known> {
        self.locals
            .iter()
            .rev()
            .find_map(|scope| scope.iter().rev().find(|(n, _)| n == name))
            .and_then(|(_, known)| known.clone())
    }

    fn shadowed(&self, name: &str) -> bool {
        self.locals
            .iter()
            .any(|scope| scope.iter().any(|(n, _)| n == name))
    }

    fn known_of_expr(&self, expr: &Expr) -> Option<Known> {
        let locals = |name: &str| self.local(name);
        let callee = |path: &Path| {
            if path.is_single() && self.shadowed(&path.segments[0].name) {
                return None;
            }
            callee_row(self.program, &self.module.name, path)
        };
        known_of_expr(expr, &locals, &callee)
    }

    fn fallible_of_expr(&self, expr: &Expr) -> Option<RowTy> {
        match self.known_of_expr(expr)? {
            Known::Fallible(row) => Some(row),
            Known::Map { .. } => None,
        }
    }

    fn declare(&mut self, name: &str, known: Option<Known>) {
        if let Some(scope) = self.locals.last_mut() {
            scope.push((name.to_owned(), known));
        }
    }

    fn declare_pattern(&mut self, pattern: &Pattern) {
        match &*pattern.kind {
            PatKind::Binding(ident) => self.declare(&ident.name, None),
            PatKind::Variant { fields, .. } | PatKind::Tuple(fields) | PatKind::Or(fields) => {
                for field in fields {
                    self.declare_pattern(field);
                }
            }
            PatKind::Struct { fields, .. } => {
                for field in fields {
                    match &field.pattern {
                        Some(sub) => self.declare_pattern(sub),
                        None => self.declare(&field.name.name, None),
                    }
                }
            }
            PatKind::At { name, pattern } => {
                self.declare(&name.name, None);
                self.declare_pattern(pattern);
            }
            PatKind::Wildcard | PatKind::Literal(_) | PatKind::Range { .. } | PatKind::Path(_) => {}
        }
    }

    fn item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Fn(decl) => self.fn_decl(decl),
            ItemKind::Impl(def) => {
                for member in &def.members {
                    self.item(member);
                }
            }
            ItemKind::Binding(binding) => self.expr(&binding.value),
            _ => {}
        }
    }

    fn fn_decl(&mut self, decl: &FnDecl) {
        let Some(body) = &decl.body else {
            return;
        };
        self.locals.push(Vec::new());
        for param in &decl.params {
            match &param.kind {
                ParamKind::Named { name, ty } => self.declare(&name.name, known_of_type(ty)),
                ParamKind::SelfParam { .. } => self.declare("self", None),
            }
        }
        self.block(body);
        self.locals.pop();
    }

    fn block(&mut self, block: &Block) {
        self.locals.push(Vec::new());
        for stmt in &block.stmts {
            self.stmt(stmt);
        }
        if let Some(tail) = &block.tail {
            self.expr(tail);
        }
        self.locals.pop();
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Binding(binding) => {
                self.expr(&binding.value);
                let known = match &*binding.pattern.kind {
                    PatKind::Binding(_) => {
                        let locals = |name: &str| self.local(name);
                        let callee = |path: &Path| {
                            if path.is_single() && self.shadowed(&path.segments[0].name) {
                                return None;
                            }
                            callee_row(self.program, &self.module.name, path)
                        };
                        known_of_binding(binding, &locals, &callee)
                    }
                    _ => None,
                };
                match &*binding.pattern.kind {
                    PatKind::Binding(ident) => {
                        let name = ident.name.clone();
                        self.declare(&name, known);
                    }
                    _ => self.declare_pattern(&binding.pattern),
                }
            }
            StmtKind::Assign { place, value, .. } => {
                self.expr(place);
                self.expr(value);
            }
            StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => self.expr(expr),
            StmtKind::AssumeNoalias(operands) => {
                for operand in operands {
                    self.expr(operand);
                }
            }
            StmtKind::Item(item) => self.item(item),
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match &*expr.kind {
            ExprKind::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                let row = self.fallible_of_expr(scrutinee);
                if let Some(row) = &row {
                    self.judge(expr, scrutinee, arms, row);
                }
                for arm in arms {
                    self.locals.push(Vec::new());
                    let is_row_arm = row.as_ref().is_some_and(|row| {
                        matches!(arm_half(&arm.pattern, &row.tags), Half::Row(_))
                    });
                    if !is_row_arm {
                        self.declare_pattern(&arm.pattern);
                    }
                    if let Some(guard) = &arm.guard {
                        self.expr(guard);
                    }
                    self.expr(&arm.body);
                    self.locals.pop();
                }
            }
            ExprKind::Closure { params, body, .. } => {
                self.locals.push(Vec::new());
                for param in params {
                    let known = param.ty.as_ref().and_then(known_of_type);
                    self.declare(&param.name.name, known);
                }
                self.expr(body);
                self.locals.pop();
            }
            ExprKind::For {
                pattern,
                iter,
                body,
            } => {
                self.expr(iter);
                self.locals.push(Vec::new());
                self.declare_pattern(pattern);
                self.block(body);
                self.locals.pop();
            }
            ExprKind::ElseDefault {
                expr: inner,
                handler,
            } => {
                self.expr(inner);
                match &**handler {
                    ElseHandler::Block(block) => self.block(block),
                    ElseHandler::Expr(fallback) => self.expr(fallback),
                    ElseHandler::Handler { pattern, body } => {
                        self.locals.push(Vec::new());
                        self.declare_pattern(pattern);
                        self.expr(body);
                        self.locals.pop();
                    }
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms {
                    self.locals.push(Vec::new());
                    match &arm.kind {
                        crate::ast::SelectArmKind::Recv { pattern, channel } => {
                            self.expr(channel);
                            self.declare_pattern(pattern);
                        }
                        crate::ast::SelectArmKind::Timeout(deadline) => self.expr(deadline),
                    }
                    self.expr(&arm.body);
                    self.locals.pop();
                }
            }
            _ => {
                let mut children: Vec<Child<'_>> = Vec::new();
                each_child_with_blocks(expr, &mut |child| children.push(child));
                for child in children {
                    match child {
                        Child::Expr(child) => self.expr(child),
                        Child::Block(block) => self.block(block),
                    }
                }
            }
        }
    }

    /// `[type.row.match]`'s two judgements over one row match: the
    /// collision (by name) and exhaustiveness (E0801).
    fn judge(&mut self, expr: &Expr, scrutinee: &Expr, arms: &[MatchArm], row: &RowTy) {
        let ok_head = row.ok.as_ref().and_then(head_name);
        // The collision first: an arm the checker cannot read is refused
        // before its coverage is judged.
        if let Some(head) = &ok_head {
            for tag in &row.tags {
                let Some(enums) = self.module.variants.get(tag) else {
                    continue;
                };
                if enums.iter().any(|owner| owner == head) {
                    self.findings.push(Finding::Unsupported(format!(
                        "`{tag}` names both a tag of the row {} and a constructor of `{head}`, the \
                         scrutinee's value type: a `match` over `{head} ! {}` cannot tell a `{tag}` \
                         arm's half, and [type.row.match] refuses it by name rather than guess; \
                         the checker's code for this refusal is the compiler's (s197)",
                        row.render(),
                        row.render()
                    )));
                    return;
                }
            }
        }
        let halves: Vec<(Half, bool)> = arms
            .iter()
            .map(|arm| (arm_half(&arm.pattern, &row.tags), arm.guard.is_some()))
            .collect();
        if halves.iter().any(|(half, _)| *half == Half::Mixed) {
            return;
        }
        let span = Span {
            start: expr.span.start,
            end: scrutinee.span.end,
        };
        let covered_both = halves
            .iter()
            .any(|(half, guarded)| *half == Half::Both && !guarded);
        if covered_both {
            return;
        }
        let guarded_any = halves.iter().any(|(_, guarded)| *guarded);
        let note = if guarded_any {
            " (arms with `if` guards do not count toward coverage — a guard can be false)"
        } else {
            ""
        };
        // The row half.
        for tag in &row.tags {
            let named = halves.iter().any(|(half, guarded)| {
                !guarded && matches!(half, Half::Row(names) if names.iter().any(|n| n == tag))
            });
            if !named {
                self.findings.push(Finding::Diag(self.e0801(
                    span,
                    format!(
                        "this `match` does not cover `{tag}`: the scrutinee is a `{} ! {}` and \
                         every tag of the row needs an arm, or a `_` to catch the rest \
                         ([type.row.match]){note}",
                        ok_head.as_deref().unwrap_or("T"),
                        row.render()
                    ),
                )));
                return;
            }
        }
        if row.open {
            self.findings.push(Finding::Diag(self.e0801(
                span,
                format!(
                    "this `match` does not cover the rest of an open row: the scrutinee is a \
                     `{} ! {}` and only a `_` arm covers `..` ([type.row.match]){note}",
                    ok_head.as_deref().unwrap_or("T"),
                    row.render()
                ),
            )));
            return;
        }
        // The value half.
        let value_arms: Vec<&MatchArm> = arms
            .iter()
            .zip(&halves)
            .filter(|(_, (half, guarded))| *half == Half::Value && !guarded)
            .map(|(arm, _)| arm)
            .collect();
        let is_variant = |name: &str| self.module.variants.contains_key(name);
        if value_arms
            .iter()
            .any(|arm| irrefutable(&arm.pattern, &is_variant))
        {
            return;
        }
        let Some(head) = ok_head.as_deref() else {
            // A builtin's row: `T` is not spelled, so the value half is judged
            // only when nothing covers it at all.
            if value_arms.is_empty() {
                self.findings.push(Finding::Diag(self.e0801(
                    span,
                    format!(
                        "this `match` does not cover the value half: the scrutinee is fallible \
                         (`{}`) and only its tags have arms; add a value arm (`v => …`) or a \
                         `_` ([type.row.match]){note}",
                        row.render()
                    ),
                )));
            }
            return;
        };
        let missing = match head {
            "bool" => {
                let has = |want: bool| {
                    value_arms.iter().any(|arm| {
                        matches!(&*arm.pattern.kind, PatKind::Literal(lit)
                            if matches!(&*lit.kind, ExprKind::Bool(b) if *b == want))
                    })
                };
                match (has(true), has(false)) {
                    (true, true) => return,
                    (false, _) => Some("`true`".to_owned()),
                    (true, false) => Some("`false`".to_owned()),
                }
            }
            _ if self.enums.contains_key(head) => {
                let variants = &self.enums[head];
                variants
                    .iter()
                    .find(|variant| {
                        !value_arms
                            .iter()
                            .any(|arm| covers_variant(&arm.pattern, head, variant, &is_variant))
                    })
                    .map(|variant| format!("`{head}.{variant}`"))
            }
            _ if crate::sema::BUILTIN_SCALAR_TYPES.contains(&head) => {
                // Literals and ranges never cover a scalar.
                let only_literals = value_arms.iter().all(|arm| {
                    matches!(
                        &*arm.pattern.kind,
                        PatKind::Literal(_) | PatKind::Range { .. }
                    )
                });
                if only_literals {
                    Some(format!("the value half (`{head}`)"))
                } else {
                    None
                }
            }
            _ => value_arms
                .is_empty()
                .then(|| format!("the value half (`{head}`)")),
        };
        if let Some(missing) = missing {
            self.findings.push(Finding::Diag(self.e0801(
                span,
                format!(
                    "this `match` does not cover {missing}: the scrutinee is a `{head} ! {}` and \
                     the value half needs an arm for every value of `{head}`, or a `_` to catch \
                     the rest ([type.row.match]){note}",
                    row.render()
                ),
            )));
        }
    }

    fn e0801(&self, span: Span, message: String) -> Diag {
        Diag::new("E0801", span, "type.row.match", message).in_file(self.file.to_owned())
    }
}

/// Whether a value pattern covers one variant of an enum: the bare name, the
/// qualified path, or the payload form with irrefutable fields.
fn covers_variant(
    pattern: &Pattern,
    owner: &str,
    variant: &str,
    is_variant: &dyn Fn(&str) -> bool,
) -> bool {
    match &*pattern.kind {
        PatKind::Binding(ident) => ident.name == variant,
        PatKind::Path(path) => match path.segments.as_slice() {
            [head, last] => head.name == owner && last.name == variant,
            _ => false,
        },
        PatKind::Variant { path, fields } => {
            let named = match path.segments.as_slice() {
                [last] => last.name == variant,
                [head, last] => head.name == owner && last.name == variant,
                _ => false,
            };
            named && fields.iter().all(|field| irrefutable(field, is_variant))
        }
        PatKind::At { pattern, .. } => covers_variant(pattern, owner, variant, is_variant),
        PatKind::Or(alternatives) => alternatives
            .iter()
            .any(|alternative| covers_variant(alternative, owner, variant, is_variant)),
        _ => false,
    }
}

/// Every finding of the row-match walk over the whole program, in source
/// order within each file.
fn findings(program: &Program) -> Vec<Finding> {
    let mut all = Vec::new();
    for module in program.modules.values() {
        let mut enums: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (variant, owners) in &module.variants {
            for owner in owners {
                enums
                    .entry(owner.clone())
                    .or_default()
                    .push(variant.clone());
            }
        }
        for unit in &module.units {
            let mut scope = Scope {
                program,
                module,
                enums: enums.clone(),
                file: &unit.file,
                locals: Vec::new(),
                findings: Vec::new(),
            };
            for item in &unit.unit.items {
                scope.item(item);
            }
            all.extend(scope.findings);
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(source: &str) -> Program {
        crate::sema::load_source("main.lu", source).expect("the program loads")
    }

    const LOOK: &str = "fn look(m: Map[str, int], k: str) -> int ! {none} {\n    m[k]\n}\n";

    #[test]
    fn a_call_to_a_declared_fn_is_statically_fallible() {
        let prog = program(&format!(
            "{LOOK}fn main() -> !int {{\n    let r = match look(Map[str, int](), \"a\") {{ none => -1, v => v }}\n    r\n}}\n"
        ));
        assert!(row_match_check(&prog).is_none());
        assert!(collision_refusal(&prog).is_none());
    }

    #[test]
    fn a_missing_tag_is_e0801_naming_it() {
        let prog = program(&format!(
            "{LOOK}fn main() -> !int {{\n    match look(Map[str, int](), \"a\") {{ v => v }}\n}}\n"
        ));
        let diag = row_match_check(&prog).expect("E0801");
        assert_eq!(diag.code, "E0801");
        assert!(diag.message.contains("`none`"), "{}", diag.message);
    }

    #[test]
    fn an_uncovered_value_half_is_e0801_naming_the_type() {
        let prog = program(&format!(
            "{LOOK}fn main() -> !int {{\n    match look(Map[str, int](), \"a\") {{ none => -1 }}\n}}\n"
        ));
        let diag = row_match_check(&prog).expect("E0801");
        assert!(
            diag.message.contains("the value half (`int`)"),
            "{}",
            diag.message
        );
    }

    #[test]
    fn a_guarded_arm_counts_for_nothing() {
        let prog = program(&format!(
            "{LOOK}fn main() -> !int {{\n    let f = true\n    match look(Map[str, int](), \"a\") {{ none if f => -1, v => v }}\n}}\n"
        ));
        let diag = row_match_check(&prog).expect("E0801");
        assert!(
            diag.message.contains("`none`") && diag.message.contains("guard"),
            "{}",
            diag.message
        );
    }

    #[test]
    fn a_wildcard_covers_both_halves_and_a_local_carries_its_row() {
        let prog = program(&format!(
            "{LOOK}fn main() -> !int {{\n    let x = look(Map[str, int](), \"a\")\n    match x {{ _ => 1 }}\n}}\n"
        ));
        assert!(row_match_check(&prog).is_none());
        let prog = program(&format!(
            "{LOOK}fn main() -> !int {{\n    let x = look(Map[str, int](), \"a\")\n    match x {{ none => 1 }}\n}}\n"
        ));
        assert!(row_match_check(&prog).is_some());
    }

    #[test]
    fn a_map_index_is_v_bang_none() {
        let prog = program(
            "fn main() -> !int {\n    var m = Map[str, int]()\n    match m[\"a\"] { none => -1 }\n}\n",
        );
        let diag = row_match_check(&prog).expect("E0801");
        assert!(
            diag.message.contains("the value half (`int`)"),
            "{}",
            diag.message
        );
    }

    #[test]
    fn an_enum_value_half_is_covered_by_every_variant_and_misses_one_by_name() {
        let pick = "enum Color { Red, Green, Rgb(int, int, int) }\nfn pick(n: int) -> Color ! {none} { if n == 0 { return none }\n Color.Red }\n";
        let prog = program(&format!(
            "{pick}fn main() -> !int {{\n    match pick(1) {{ none => 0, Red => 1, Green => 2, Rgb(r, g, b) => r }}\n}}\n"
        ));
        assert!(row_match_check(&prog).is_none());
        let prog = program(&format!(
            "{pick}fn main() -> !int {{\n    match pick(1) {{ none => 0, Red => 1, Rgb(r, g, b) => r }}\n}}\n"
        ));
        let diag = row_match_check(&prog).expect("E0801");
        assert!(diag.message.contains("`Color.Green`"), "{}", diag.message);
    }

    #[test]
    fn a_tag_that_is_also_a_variant_is_refused_by_name() {
        let prog = program(
            "enum Status { Timeout, Fine }\nfn probe(n: int) -> Status ! {Timeout} { Status.Fine }\nfn main() -> !int {\n    match probe(1) { Timeout => 0, s => 1 }\n}\n",
        );
        let reason = collision_refusal(&prog).expect("refused by name");
        assert!(
            reason.contains("`Timeout`") && reason.contains("`Status`"),
            "{reason}"
        );
        assert!(row_match_check(&prog).is_none());
    }

    #[test]
    fn a_method_call_scrutinee_is_not_judged() {
        let prog = program(&format!(
            "{LOOK}fn main() -> !int {{\n    let xs = [1]\n    match xs.get(0) {{ v => 1 }}\n}}\n"
        ));
        assert!(row_match_check(&prog).is_none());
    }

    #[test]
    fn a_try_under_a_defer_is_e0611_at_the_try() {
        let prog = program(
            "fn key(ok: bool) -> str ! {parse} { if ok { \"a\" } else { return parse } }\nfn under(ok: bool) -> int ! {parse} {\n    defer print(\"d {key(ok)?}\")\n    1\n}\nfn main() -> !int { under(true) else 9 }\n",
        );
        let diag = defer_try_check(&prog).expect("E0611");
        assert_eq!(diag.code, "E0611");
        let source = "fn key(ok: bool) -> str ! {parse} { if ok { \"a\" } else { return parse } }\nfn under(ok: bool) -> int ! {parse} {\n    defer print(\"d {key(ok)?}\")\n    1\n}\nfn main() -> !int { under(true) else 9 }\n";
        assert_eq!(&source[diag.span.start..diag.span.end], "key(ok)?");
    }

    #[test]
    fn a_try_inside_a_closure_under_a_defer_is_the_closures_own() {
        let prog = program(
            "fn key(ok: bool) -> str ! {parse} { if ok { \"a\" } else { return parse } }\nfn under(ok: bool) -> int ! {parse} {\n    defer {\n        let f = fn(o: bool) { key(o)? }\n        print(\"d {f(ok) else \"-\"}\")\n    }\n    1\n}\nfn main() -> !int { under(true) else 9 }\n",
        );
        assert!(defer_try_check(&prog).is_none());
    }

    #[test]
    fn an_errdefer_block_binding_is_read_whole() {
        let prog = program(
            "fn key(ok: bool) -> str ! {parse} { if ok { \"a\" } else { return parse } }\nfn under(ok: bool) -> int ! {parse} {\n    errdefer {\n        let k = key(ok)?\n        print(\"{k}\")\n    }\n    1\n}\nfn main() -> !int { under(true) else 9 }\n",
        );
        assert!(defer_try_check(&prog).is_some());
    }
}

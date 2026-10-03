//! The private row of a `-> !T` function, inferred from its body (is69,
//! wolffe-lang/wolf-interp#176).
//!
//! `01-grammar.md`: "`-> !T` error union with inferred private row". The
//! compiler seals that row per module at signature elaboration (s15,
//! `wolf_sema::rows`): a cycle-aware fixpoint over the module's inferred
//! functions, each row growing from empty by what its body can raise — a tag
//! raised at a checked position against the function's own row, the row of a
//! fallible value that flows into the return, and the row of every `?` (a
//! `?` inside a closure is the closure's own). `[type.row.match]` then judges
//! a `match` over the function's result against that row, so a body that
//! raises nothing has the EMPTY row and a lone value arm covers it
//! (wolf-lang's `rows/eu_bind_empty_row_handled.lu`).
//!
//! lupin 0.1.44 read every such row as the open row `{..}` (is67's reader
//! had no inference), so a `match` with no `_` was E0801 whatever the body
//! raised. This module is the inference; [`crate::rowmatch::callee_row`] is
//! its one reader, which serves sema's E0801 judge, the evaluator's
//! two-half dispatch and the lint alike.
//!
//! # What this machine can know
//!
//! Sema-lite checks no types, so a body is read by what it spells: literals,
//! operators and constructors are plain; a call to a module `fn` has its
//! declared row (or its own inferred row — recursion and mutual recursion
//! converge in the fixpoint); a builtin its declared row; a `Map` index
//! `{none}`; a local what its binding recorded; a member read of a plain
//! value is plain unless some aggregate of the program carries a row. A
//! body that reaches anything else in a position whose row would flow out —
//! a method call, a closure call, a spawn, an open row, an unknown local —
//! makes the row **unknown**, and an unknown row is read as `{..}`, exactly
//! 0.1.44's reading: a row this machine cannot name may hold tags, so only
//! `_` is safe. A guess never becomes a verdict.

use std::collections::BTreeMap;

use crate::ast::{
    Block, ElseHandler, Expr, ExprKind, FnDecl, IndexArg, Item, ItemKind, ParamKind, PatKind, Path,
    Pattern, RetType, Stmt, StmtKind, StrPart, Type, TypeArg, TypeDef, TypeKind,
};
use crate::sema::{Def, Program};

/// Every inferred row of a program, by `(module, fn name)`: `Some(tags)` in
/// first-seen order, or `None` when the body reaches something this machine
/// cannot name.
pub type Rows = BTreeMap<(String, String), Option<Vec<String>>>;

/// The fixpoint's cap. Rows only grow inside a finite tag universe, so it
/// is never reached by a real program; if it were, every row would be
/// unknown rather than a guess.
const MAX_ROUNDS: usize = 64;

/// The ok type `T` of a `-> !T` return — `Some` exactly when the row is
/// inferred (no row spelled in either position).
#[must_use]
pub fn inferred_ok(ret: &RetType) -> Option<&Type> {
    match (&ret.row, &*ret.ty.kind) {
        (None, TypeKind::ErrorUnion(inner)) => Some(inner),
        _ => None,
    }
}

/// The inferred row of `module`'s `fn name`, read from the program's cache
/// (computed once, on first use). `None` when the fn's row is not inferred,
/// or when the inference could not name it.
#[must_use]
pub fn row_of(program: &Program, module: &str, name: &str) -> Option<Vec<String>> {
    program
        .inferred_rows
        .get_or_init(|| infer(program))
        .get(&(module.to_owned(), name.to_owned()))
        .cloned()
        .flatten()
}

/// Every module `fn` with a `-> !T` return and a body, sealed to its row.
#[must_use]
pub fn infer(program: &Program) -> Rows {
    let mut fns: Vec<(String, String, &FnDecl)> = Vec::new();
    for (module_name, module) in &program.modules {
        for (name, (def, _)) in &module.items {
            if let Def::Fn(decl) = def
                && decl.body.is_some()
                && decl
                    .ret
                    .as_ref()
                    .is_some_and(|ret| inferred_ok(ret).is_some())
            {
                fns.push((module_name.clone(), name.clone(), decl));
            }
        }
    }
    let mut rows: Rows = fns
        .iter()
        .map(|(module, name, _)| ((module.clone(), name.clone()), Some(Vec::new())))
        .collect();
    if fns.is_empty() {
        return rows;
    }
    let rows_in_data = rows_in_data(program);
    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        for (module, name, decl) in &fns {
            let key = (module.clone(), name.clone());
            if rows[&key].is_none() {
                continue;
            }
            let found = Walk::new(program, module, &rows, rows_in_data).fn_row(decl);
            let entry = rows.get_mut(&key).expect("every inferred fn has an entry");
            match (found, entry.as_mut()) {
                (None, _) => {
                    *entry = None;
                    changed = true;
                }
                (Some(tags), Some(row)) => {
                    for tag in tags {
                        if !row.contains(&tag) {
                            row.push(tag);
                            changed = true;
                        }
                    }
                }
                (Some(_), None) => {}
            }
        }
        if !changed {
            return rows;
        }
    }
    rows.values_mut().for_each(|row| *row = None);
    rows
}

/// Whether some aggregate the program declares (a struct field, an enum
/// payload, an alias) mentions a row: then a member read, an element, or a
/// binder over a plain value may itself be fallible, and this machine
/// cannot tell which.
fn rows_in_data(program: &Program) -> bool {
    program.modules.values().any(|module| {
        module.units.iter().any(|unit| {
            unit.unit.items.iter().any(|item| match &item.kind {
                ItemKind::Struct(def) => def.fields.iter().any(|f| mentions_row(&f.ty)),
                ItemKind::Enum(def) => def
                    .variants
                    .iter()
                    .any(|v| v.payload.iter().any(mentions_row)),
                ItemKind::TypeAlias(alias) => match &alias.def {
                    TypeDef::Struct(def) => def.fields.iter().any(|f| mentions_row(&f.ty)),
                    TypeDef::Enum(def) => def
                        .variants
                        .iter()
                        .any(|v| v.payload.iter().any(mentions_row)),
                    TypeDef::Alias(ty) => mentions_row(ty),
                },
                _ => false,
            })
        })
    })
}

/// A type that is, or holds, a `T ! {row}` or a `!T`. A function type is a
/// plain value whatever it returns.
fn mentions_row(ty: &Type) -> bool {
    match &*ty.kind {
        TypeKind::ErrorUnion(_) | TypeKind::Fallible { .. } => true,
        TypeKind::Path { args, .. } => args.iter().any(|arg| match arg {
            TypeArg::Type(ty) => mentions_row(ty),
            _ => false,
        }),
        TypeKind::Tuple(items) => items.iter().any(mentions_row),
        TypeKind::Prefixed { ty, .. } | TypeKind::RawPointer(ty) => mentions_row(ty),
        TypeKind::Fn { .. } | TypeKind::Dyn(_) | TypeKind::TypeOfTypes | TypeKind::Region => false,
    }
}

/// What a value is statically known to be, for the inference.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ty {
    /// No row: a literal, an operator's result, a constructor, an ok value.
    Plain,
    /// A `Map[K, V]`: an index of it is `V ! {none}`.
    Map,
    /// A `T ! {row}` with a closed row this machine names.
    Row(Vec<String>),
    /// Diverges (`return`, `break`, `continue`): contributes nothing.
    Never,
    /// Anything this machine cannot name.
    Unknown,
}

impl Ty {
    /// Two branches meeting at one value (`if`/`match` arms, an `else`).
    fn join(self, other: Ty) -> Ty {
        match (self, other) {
            (Ty::Never, ty) | (ty, Ty::Never) => ty,
            (Ty::Unknown, _) | (_, Ty::Unknown) => Ty::Unknown,
            (Ty::Row(mut a), Ty::Row(b)) => {
                for tag in b {
                    if !a.contains(&tag) {
                        a.push(tag);
                    }
                }
                Ty::Row(a)
            }
            // A plain branch is the ok half of the other's union.
            (Ty::Row(a), Ty::Plain) | (Ty::Plain, Ty::Row(a)) => Ty::Row(a),
            (Ty::Plain, Ty::Plain) => Ty::Plain,
            (Ty::Map, Ty::Map) => Ty::Map,
            _ => Ty::Unknown,
        }
    }
}

/// One body's walk: the tags it raises into its own row, or unknown.
struct Walk<'a> {
    program: &'a Program,
    module: &'a str,
    rows: &'a Rows,
    rows_in_data: bool,
    /// Locals, innermost scope last.
    locals: Vec<Vec<(String, Ty)>>,
    /// Inside a closure: its `?` and `return` are its own.
    closure_depth: usize,
    tags: Vec<String>,
    unknown: bool,
}

impl<'a> Walk<'a> {
    fn new(program: &'a Program, module: &'a str, rows: &'a Rows, rows_in_data: bool) -> Self {
        Walk {
            program,
            module,
            rows,
            rows_in_data,
            locals: Vec::new(),
            closure_depth: 0,
            tags: Vec::new(),
            unknown: false,
        }
    }

    /// The row a fn's body raises, `None` when it cannot be named.
    fn fn_row(mut self, decl: &FnDecl) -> Option<Vec<String>> {
        let body = decl.body.as_ref()?;
        self.locals.push(Vec::new());
        for param in &decl.params {
            match &param.kind {
                ParamKind::Named { name, ty } => {
                    let ty = self.ty_of_type(ty);
                    self.declare(&name.name, ty);
                }
                ParamKind::SelfParam { .. } => self.declare("self", Ty::Plain),
            }
        }
        let tail = self.block(body, true);
        self.raise(tail);
        (!self.unknown).then_some(self.tags)
    }

    // -- scopes -------------------------------------------------------------

    fn declare(&mut self, name: &str, ty: Ty) {
        if let Some(scope) = self.locals.last_mut() {
            scope.push((name.to_owned(), ty));
        }
    }

    fn local(&self, name: &str) -> Option<Ty> {
        self.locals
            .iter()
            .rev()
            .find_map(|scope| scope.iter().rev().find(|(n, _)| n == name))
            .map(|(_, ty)| ty.clone())
    }

    /// Every name a pattern binds, each with `ty`.
    fn declare_pattern(&mut self, pattern: &Pattern, ty: &Ty) {
        match &*pattern.kind {
            PatKind::Binding(ident) => self.declare(&ident.name, ty.clone()),
            PatKind::Variant { fields, .. } | PatKind::Tuple(fields) | PatKind::Or(fields) => {
                for field in fields {
                    self.declare_pattern(field, ty);
                }
            }
            PatKind::Struct { fields, .. } => {
                for field in fields {
                    match &field.pattern {
                        Some(sub) => self.declare_pattern(sub, ty),
                        None => self.declare(&field.name.name, ty.clone()),
                    }
                }
            }
            PatKind::At { name, pattern } => {
                self.declare(&name.name, ty.clone());
                self.declare_pattern(pattern, ty);
            }
            PatKind::Wildcard | PatKind::Literal(_) | PatKind::Range { .. } | PatKind::Path(_) => {}
        }
    }

    /// What a part of a plain value holds: plain, unless some aggregate of
    /// the program carries a row.
    fn part_of(&self, whole: &Ty) -> Ty {
        match whole {
            Ty::Plain | Ty::Map if !self.rows_in_data => Ty::Plain,
            _ => Ty::Unknown,
        }
    }

    // -- the row ------------------------------------------------------------

    /// A value of type `ty` flows out of the function (its tail, a `return`
    /// operand) or is propagated by `?`: its row joins the function's.
    fn raise(&mut self, ty: Ty) {
        match ty {
            Ty::Row(tags) => {
                for tag in tags {
                    if !self.tags.contains(&tag) {
                        self.tags.push(tag);
                    }
                }
            }
            Ty::Unknown => self.unknown = true,
            Ty::Plain | Ty::Map | Ty::Never => {}
        }
    }

    /// A closed row spelled in a signature or an annotation, by its tags; an
    /// open row, a qualified entry or an `error` alias is not one this
    /// machine names.
    fn spelled_row(&self, row: &crate::ast::ErrorRow) -> Ty {
        if row.open {
            return Ty::Unknown;
        }
        let aliases = self
            .program
            .modules
            .get(self.module)
            .map(|module| &module.error_aliases);
        let mut tags = Vec::new();
        for entry in &row.entries {
            match entry.path.segments.as_slice() {
                [segment] if !aliases.is_some_and(|a| a.contains_key(&segment.name)) => {
                    tags.push(segment.name.clone());
                }
                _ => return Ty::Unknown,
            }
        }
        Ty::Row(tags)
    }

    /// What an annotation or a parameter type says the value is. A `!T`
    /// annotation's row is not named here (a binding takes its initializer's
    /// — [`Walk::binding`]).
    fn ty_of_type(&self, ty: &Type) -> Ty {
        match &*ty.kind {
            TypeKind::Fallible { row, .. } => self.spelled_row(row),
            TypeKind::ErrorUnion(_) => Ty::Unknown,
            TypeKind::Path { path, args } => match path.segments.as_slice() {
                [head] if head.name == "Map" && args.len() == 2 => Ty::Map,
                [head] => match self.program.lookup(self.module, &head.name, false) {
                    // An alias may name a fallible type.
                    Some(Def::Opaque("type" | "error")) => Ty::Unknown,
                    _ if mentions_row(ty) => Ty::Unknown,
                    _ => Ty::Plain,
                },
                _ => Ty::Unknown,
            },
            _ if mentions_row(ty) => Ty::Unknown,
            _ => Ty::Plain,
        }
    }

    /// What calling a module fn gives: its declared row, its inferred row
    /// (the fixpoint's current one), or plain.
    fn fn_result(&self, module: &str, decl: &FnDecl) -> Ty {
        let Some(ret) = &decl.ret else {
            return Ty::Plain;
        };
        if let Some(row) = &ret.row {
            return self.spelled_row(row);
        }
        match &*ret.ty.kind {
            TypeKind::ErrorUnion(_) => self
                .rows
                .get(&(module.to_owned(), decl.name.name.clone()))
                .and_then(|row| row.clone())
                .map_or(Ty::Unknown, Ty::Row),
            _ => self.ty_of_type(&ret.ty),
        }
    }

    /// The module a qualified call's head names: a `use`d module by its
    /// bound name, or a loaded module by its own.
    fn target_module(&self, head: &str) -> Option<String> {
        let home = self.program.modules.get(self.module)?;
        home.use_paths
            .iter()
            .find(|(bound, _)| bound == head)
            .map(|(_, segments)| segments.join("."))
            .or_else(|| {
                self.program
                    .modules
                    .contains_key(head)
                    .then(|| head.to_owned())
            })
    }

    /// A bare name nothing resolves — a local, an item, an import, a
    /// variant — spelled capitalized: at a checked position against the
    /// function's own row it is a tag (`[gram.expr.tagident]`, D30).
    fn is_tag(&self, name: &str) -> bool {
        if !name.starts_with(char::is_uppercase) || self.local(name).is_some() {
            return false;
        }
        let Some(module) = self.program.modules.get(self.module) else {
            return false;
        };
        !module.items.contains_key(name)
            && !module.use_paths.iter().any(|(bound, _)| bound == name)
            && !module.variants.contains_key(name)
    }

    // -- the walk -----------------------------------------------------------

    fn block(&mut self, block: &Block, out: bool) -> Ty {
        self.locals.push(Vec::new());
        let mut ty = Ty::Plain;
        for stmt in &block.stmts {
            if self.stmt(stmt) {
                ty = Ty::Never;
            }
        }
        if let Some(tail) = &block.tail {
            let tail = self.expr(tail, out);
            ty = if ty == Ty::Never { Ty::Never } else { tail };
        }
        self.locals.pop();
        ty
    }

    /// Walks one statement; `true` when it always diverges (`return`).
    fn stmt(&mut self, stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Binding(binding) => {
                let value = self.expr(&binding.value, false);
                let ty = match &binding.ty {
                    // `let a: !int = f()` takes `f`'s row (i69_let_annotated).
                    Some(ty) if matches!(&*ty.kind, TypeKind::ErrorUnion(_)) => match value {
                        Ty::Row(_) => value,
                        _ => Ty::Unknown,
                    },
                    Some(ty) => self.ty_of_type(ty),
                    None => value,
                };
                match &*binding.pattern.kind {
                    PatKind::Binding(ident) => self.declare(&ident.name, ty),
                    _ => {
                        let part = self.part_of(&ty);
                        self.declare_pattern(&binding.pattern, &part);
                    }
                }
                false
            }
            StmtKind::Assign { place, value, .. } => {
                self.expr(place, false);
                self.expr(value, false);
                false
            }
            StmtKind::Defer { expr, .. } => {
                self.expr(expr, false);
                false
            }
            StmtKind::Expr(expr) => self.expr(expr, false) == Ty::Never,
            StmtKind::AssumeNoalias(operands) => {
                for operand in operands {
                    self.expr(operand, false);
                }
                false
            }
            StmtKind::Item(item) => {
                self.item(item);
                false
            }
        }
    }

    /// A nested item: a nested fn's body is its own frame; its name shadows.
    fn item(&mut self, item: &Item) {
        if let ItemKind::Fn(decl) = &item.kind {
            self.declare(&decl.name.name, Ty::Unknown);
        }
    }

    /// Walks `expr` and answers what its value is. `out`: the value flows
    /// into the function's return (the tail, a `return` operand, a branch of
    /// either), so a bare tag there is a raise.
    fn expr(&mut self, expr: &Expr, out: bool) -> Ty {
        match &*expr.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Char(_)
            | ExprKind::Wildcard => Ty::Plain,
            ExprKind::Str(lit) => {
                for part in &lit.parts {
                    if let StrPart::Interp(hole) = part {
                        self.expr(&hole.expr, false);
                    }
                }
                Ty::Plain
            }
            ExprKind::Path(path) => self.path(path, out),
            ExprKind::StructLit { fields, .. } => {
                for field in fields {
                    self.expr(&field.value, false);
                }
                Ty::Plain
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for item in items {
                    self.expr(item, false);
                }
                Ty::Plain
            }
            ExprKind::Group(inner) => self.expr(inner, out),
            ExprKind::Block(block) => self.block(block, out),
            ExprKind::Unary { operand, .. } => {
                self.expr(operand, false);
                Ty::Plain
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs, false);
                self.expr(rhs, false);
                Ty::Plain
            }
            ExprKind::Cast { expr: inner, .. } => {
                self.expr(inner, false);
                Ty::Plain
            }
            ExprKind::Call { callee, args } => {
                for arg in args {
                    self.expr(&arg.expr, false);
                }
                self.call(callee, out)
            }
            ExprKind::BracketApply { base, args, .. } => {
                let base = self.expr(base, false);
                let mut range = false;
                for arg in args {
                    if let IndexArg::Value(arg) = arg {
                        range |= matches!(&*arg.expr.kind, ExprKind::Range { .. });
                        self.expr(&arg.expr, false);
                    }
                }
                match base {
                    Ty::Map if !range => Ty::Row(vec!["none".to_owned()]),
                    Ty::Plain | Ty::Map if range => Ty::Plain,
                    Ty::Plain => self.part_of(&Ty::Plain),
                    _ => Ty::Unknown,
                }
            }
            ExprKind::Member { base, .. } => {
                let base = self.expr(base, false);
                self.part_of(&base)
            }
            ExprKind::ModedReceiver { place, .. } => self.expr(place, false),
            ExprKind::Try(inner) => {
                let ty = self.expr(inner, false);
                if self.closure_depth == 0 {
                    self.raise(ty);
                }
                Ty::Plain
            }
            ExprKind::Range { start, end, .. } => {
                for bound in [start, end].into_iter().flatten() {
                    self.expr(bound, false);
                }
                Ty::Plain
            }
            ExprKind::FromEnd(inner) | ExprKind::Freeze(inner) => {
                let ty = self.expr(inner, false);
                if matches!(&*expr.kind, ExprKind::FromEnd(_)) {
                    Ty::Plain
                } else {
                    ty
                }
            }
            ExprKind::ElseDefault {
                expr: inner,
                handler,
            } => {
                // The row of `inner` is handled here (`[type.row.else]`); a
                // `?` inside it was raised by the walk.
                self.expr(inner, false);
                let handled = match &**handler {
                    ElseHandler::Block(block) => self.block(block, out),
                    ElseHandler::Expr(fallback) => self.expr(fallback, out),
                    ElseHandler::Handler { pattern, body } => {
                        self.locals.push(Vec::new());
                        self.declare_pattern(pattern, &Ty::Unknown);
                        let ty = self.expr(body, out);
                        self.locals.pop();
                        ty
                    }
                };
                Ty::Plain.join(handled)
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.expr(cond, false);
                let then = self.block(then, out);
                let otherwise = match otherwise {
                    Some(otherwise) => self.expr(otherwise, out),
                    None => Ty::Plain,
                };
                then.join(otherwise)
            }
            ExprKind::Match { scrutinee, arms } => {
                let scrutinee = self.expr(scrutinee, false);
                let mut ty = Ty::Never;
                for arm in arms {
                    self.locals.push(Vec::new());
                    let binders = match &scrutinee {
                        // A row match: a row arm binds a payload, a value
                        // arm binds parts of the ok value.
                        Ty::Row(tags) => match crate::rowmatch::arm_half(&arm.pattern, tags) {
                            crate::rowmatch::Half::Value | crate::rowmatch::Half::Both => {
                                self.part_of(&Ty::Plain)
                            }
                            _ => Ty::Unknown,
                        },
                        other => self.part_of(other),
                    };
                    self.declare_pattern(&arm.pattern, &binders);
                    if let Some(guard) = &arm.guard {
                        self.expr(guard, false);
                    }
                    let body = self.expr(&arm.body, out);
                    ty = ty.join(body);
                    self.locals.pop();
                }
                ty
            }
            ExprKind::For {
                pattern,
                iter,
                body,
            } => {
                let iter = self.expr(iter, false);
                self.locals.push(Vec::new());
                let part = self.part_of(&iter);
                self.declare_pattern(pattern, &part);
                self.block(body, false);
                self.locals.pop();
                Ty::Plain
            }
            ExprKind::While { cond, body } => {
                self.expr(cond, false);
                self.block(body, false);
                Ty::Plain
            }
            ExprKind::Loop { body } => {
                self.block(body, false);
                if breaks_with_value(body) {
                    Ty::Unknown
                } else {
                    Ty::Plain
                }
            }
            ExprKind::Return(value) => {
                let ty = match value {
                    Some(value) => self.expr(value, self.closure_depth == 0),
                    None => Ty::Plain,
                };
                if self.closure_depth == 0 {
                    self.raise(ty);
                }
                Ty::Never
            }
            ExprKind::Break(value) => {
                if let Some(value) = value {
                    self.expr(value, false);
                }
                Ty::Never
            }
            ExprKind::Continue => Ty::Never,
            ExprKind::Closure { params, body, .. } => {
                self.closure_depth += 1;
                self.locals.push(Vec::new());
                for param in params {
                    let ty = param
                        .ty
                        .as_ref()
                        .map_or(Ty::Unknown, |ty| self.ty_of_type(ty));
                    self.declare(&param.name.name, ty);
                }
                self.expr(body, false);
                self.locals.pop();
                self.closure_depth -= 1;
                Ty::Plain
            }
            ExprKind::RegionSugar { cap, body, .. } => {
                if let Some(cap) = cap {
                    self.expr(cap, false);
                }
                self.block(body, out)
            }
            ExprKind::RegionValue { cap, .. } => {
                if let Some(cap) = cap {
                    self.expr(cap, false);
                }
                Ty::Plain
            }
            ExprKind::In { region, body } => {
                self.expr(region, false);
                self.block(body, out)
            }
            ExprKind::When { operands, body } => {
                for operand in operands {
                    self.expr(operand, false);
                }
                self.block(body, out)
            }
            ExprKind::Unsafe { body } | ExprKind::Scope { body, .. } => self.block(body, out),
            ExprKind::SpawnProc { args, .. } => {
                // A spawned task's row re-raises at the scope (the compiler's
                // `[conc.task.fail]`): not followed here.
                for arg in args {
                    self.expr(&arg.expr, false);
                }
                self.unknown = true;
                Ty::Unknown
            }
            ExprKind::Select { arms } => {
                let mut ty = Ty::Never;
                for arm in arms {
                    self.locals.push(Vec::new());
                    match &arm.kind {
                        crate::ast::SelectArmKind::Recv { pattern, channel } => {
                            self.expr(channel, false);
                            self.declare_pattern(pattern, &Ty::Unknown);
                        }
                        crate::ast::SelectArmKind::Timeout(deadline) => {
                            self.expr(deadline, false);
                        }
                    }
                    let body = self.expr(&arm.body, out);
                    ty = ty.join(body);
                    self.locals.pop();
                }
                ty
            }
            ExprKind::UnsafeC { .. } => Ty::Unknown,
            ExprKind::Asm { operands, .. } => {
                for operand in operands {
                    self.expr(&operand.value, false);
                }
                Ty::Unknown
            }
            ExprKind::Borrow { place, from } => {
                self.expr(place, false);
                self.expr(from, false);
                Ty::Unknown
            }
        }
    }

    /// A name in value position.
    fn path(&mut self, path: &Path, out: bool) -> Ty {
        match path.segments.as_slice() {
            [segment] => {
                let name = &segment.name;
                if let Some(ty) = self.local(name) {
                    return ty;
                }
                match self.program.lookup(self.module, name, false) {
                    Some(Def::Fn(_)) => Ty::Plain,
                    Some(Def::Binding(binding)) => binding
                        .ty
                        .as_ref()
                        .map_or(Ty::Unknown, |ty| self.ty_of_type(ty)),
                    Some(_) => Ty::Unknown,
                    None if out && self.is_tag(name) => Ty::Row(vec![name.clone()]),
                    None => Ty::Unknown,
                }
            }
            // `Color.Red`: a variant value.
            [owner, variant] if self.is_variant_of(&owner.name, &variant.name) => Ty::Plain,
            // `p.x`, `xs.len`: a dotted path from a local is a member read.
            [base, ..] => match self.local(&base.name) {
                Some(ty) => self.part_of(&ty),
                None => Ty::Unknown,
            },
            [] => Ty::Unknown,
        }
    }

    fn is_variant_of(&self, owner: &str, variant: &str) -> bool {
        self.program
            .modules
            .get(self.module)
            .and_then(|module| module.variants.get(variant))
            .is_some_and(|owners| owners.iter().any(|o| o == owner))
    }

    /// What a call gives, by its callee.
    fn call(&mut self, callee: &Expr, out: bool) -> Ty {
        match &*callee.kind {
            ExprKind::Path(path) => match path.segments.as_slice() {
                [segment] => {
                    let name = &segment.name;
                    if self.local(name).is_some() {
                        // A closure or a fn value: its row is not named here.
                        return Ty::Unknown;
                    }
                    match self.program.lookup(self.module, name, false) {
                        Some(Def::Fn(decl)) => self.fn_result(self.module, decl),
                        Some(Def::Struct(_)) => Ty::Plain,
                        Some(_) => Ty::Unknown,
                        None if self.is_tag(name) => {
                            if out {
                                Ty::Row(vec![name.clone()])
                            } else {
                                Ty::Unknown
                            }
                        }
                        None if self
                            .program
                            .modules
                            .get(self.module)
                            .is_some_and(|m| m.use_paths.iter().any(|(b, _)| b == name)) =>
                        {
                            Ty::Unknown
                        }
                        None => {
                            let row = crate::eval::builtin::declared_row(name);
                            if row.is_empty() {
                                Ty::Plain
                            } else {
                                Ty::Row(row.iter().map(|tag| (*tag).to_owned()).collect())
                            }
                        }
                    }
                }
                [owner, name] => {
                    if self.is_variant_of(&owner.name, &name.name) {
                        return Ty::Plain;
                    }
                    if self.local(&owner.name).is_some() {
                        return Ty::Unknown;
                    }
                    let Some(target) = self.target_module(&owner.name) else {
                        return Ty::Unknown;
                    };
                    match self.program.lookup(&target, &name.name, true) {
                        Some(Def::Fn(decl)) => self.fn_result(&target, decl),
                        _ => Ty::Unknown,
                    }
                }
                _ => Ty::Unknown,
            },
            // `wrap[int](…)`, `Map[str, int]()`, `List[int]()`.
            ExprKind::BracketApply { base, .. } => {
                let ExprKind::Path(path) = &*base.kind else {
                    return Ty::Unknown;
                };
                let [segment] = path.segments.as_slice() else {
                    return Ty::Unknown;
                };
                if self.local(&segment.name).is_some() {
                    return Ty::Unknown;
                }
                match self.program.lookup(self.module, &segment.name, false) {
                    Some(Def::Fn(decl)) => self.fn_result(self.module, decl),
                    None if segment.name == "Map" => Ty::Map,
                    None if matches!(segment.name.as_str(), "List" | "Set") => Ty::Plain,
                    _ => Ty::Unknown,
                }
            }
            // A method call: no static method table here.
            ExprKind::Member { base, .. } => {
                self.expr(base, false);
                Ty::Unknown
            }
            _ => {
                self.expr(callee, false);
                Ty::Unknown
            }
        }
    }
}

/// Whether a `loop` body has a `break` with a value aimed at it (not at a
/// nested loop, not inside a closure).
fn breaks_with_value(body: &Block) -> bool {
    fn in_expr(expr: &Expr) -> bool {
        match &*expr.kind {
            ExprKind::Break(Some(_)) => true,
            ExprKind::Loop { .. }
            | ExprKind::While { .. }
            | ExprKind::For { .. }
            | ExprKind::Closure { .. } => false,
            _ => {
                let mut found = false;
                crate::rowmatch::each_child_with_blocks(expr, &mut |child| match child {
                    crate::rowmatch::Child::Expr(e) => found |= in_expr(e),
                    crate::rowmatch::Child::Block(b) => found |= in_block(b),
                });
                found
            }
        }
    }
    fn in_block(block: &Block) -> bool {
        block.stmts.iter().any(|stmt| match &stmt.kind {
            StmtKind::Binding(binding) => in_expr(&binding.value),
            StmtKind::Assign { place, value, .. } => in_expr(place) || in_expr(value),
            StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => in_expr(expr),
            StmtKind::AssumeNoalias(operands) => operands.iter().any(in_expr),
            StmtKind::Item(_) => false,
        }) || block.tail.as_deref().is_some_and(in_expr)
    }
    in_block(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(source: &str) -> Rows {
        let program = crate::sema::load_source("main.lu", source).expect("the program loads");
        infer(&program)
    }

    fn row(rows: &Rows, name: &str) -> Option<Vec<String>> {
        rows.get(&(String::new(), name.to_owned()))
            .cloned()
            .flatten()
    }

    #[test]
    fn a_body_that_raises_nothing_has_the_empty_row() {
        let rows = rows("fn f() -> !int {\n    42\n}\nfn main() {\n}\n");
        assert_eq!(row(&rows, "f"), Some(Vec::new()));
    }

    #[test]
    fn a_raised_tag_a_question_mark_and_a_tail_value_join_the_row() {
        let rows = rows(
            "fn p(s: str) -> int ! {bad} {\n    if s == \"x\" {\n        return bad\n    }\n    1\n}\n\
             fn look(m: Map[str, int], k: str) -> int ! {none} {\n    m[k]\n}\n\
             fn f(s: str) -> !int {\n    if s == \"\" {\n        return Empty\n    }\n    let n = p(s)?\n    var m = Map[str, int]()\n    look(m, s)\n}\n\
             fn main() {\n}\n",
        );
        assert_eq!(
            row(&rows, "f"),
            Some(vec![
                "Empty".to_owned(),
                "bad".to_owned(),
                "none".to_owned()
            ])
        );
    }

    #[test]
    fn mutual_recursion_converges_on_the_union() {
        let rows = rows(
            "fn ping(n: int) -> !int {\n    if n > 5 {\n        return Far\n    }\n    if n == 0 {\n        return 0\n    }\n    pong(n - 1)?\n}\n\
             fn pong(n: int) -> !int {\n    if n < 0 {\n        return Near\n    }\n    ping(n)?\n}\n\
             fn main() {\n}\n",
        );
        let mut ping = row(&rows, "ping").expect("ping is inferred");
        let mut pong = row(&rows, "pong").expect("pong is inferred");
        ping.sort();
        pong.sort();
        assert_eq!(ping, vec!["Far".to_owned(), "Near".to_owned()]);
        assert_eq!(pong, ping);
    }

    #[test]
    fn a_closures_question_mark_is_its_own() {
        let rows = rows(
            "fn p(s: str) -> int ! {bad} {\n    1\n}\n\
             fn f(s: str) -> !int {\n    let g = fn(t: str) { p(t)? }\n    1\n}\nfn main() {\n}\n",
        );
        assert_eq!(row(&rows, "f"), Some(Vec::new()));
    }

    #[test]
    fn a_method_call_flowing_out_is_not_inferred() {
        let opaque = rows("fn f(xs: List[int]) -> !int {\n    xs.get(0)\n}\nfn main() {\n}\n");
        assert_eq!(opaque.get(&(String::new(), "f".to_owned())), Some(&None));
        // A method call whose value is used, not returned, does not matter.
        let used = rows(
            "fn f(xs: List[int]) -> !int {\n    let n = xs.count()\n    if n > 3 {\n        return Big\n    }\n    1\n}\nfn main() {\n}\n",
        );
        assert_eq!(row(&used, "f"), Some(vec!["Big".to_owned()]));
    }

    #[test]
    fn unknown_spreads_through_a_question_mark() {
        let rows = rows(
            "fn f(xs: List[int]) -> !int {\n    xs.get(0)\n}\nfn g(xs: List[int]) -> !int {\n    f(xs)?\n}\nfn main() {\n}\n",
        );
        assert_eq!(rows.get(&(String::new(), "g".to_owned())), Some(&None));
    }

    #[test]
    fn a_member_read_of_a_plain_value_is_plain() {
        let rows = rows(
            "struct P {\n    x: int,\n}\nfn f(p: P, xs: List[int]) -> !int {\n    if p.x < 0 {\n        return Neg\n    }\n    p.x + xs.len\n}\nfn g(p: P) -> !int {\n    p.x\n}\nfn main() {\n}\n",
        );
        assert_eq!(row(&rows, "f"), Some(vec!["Neg".to_owned()]));
        assert_eq!(row(&rows, "g"), Some(Vec::new()));
    }

    #[test]
    fn a_row_in_an_aggregate_makes_member_reads_unknown() {
        let rows = rows(
            "struct P {\n    x: int ! {bad},\n}\nfn f(p: P) -> !int {\n    p.x\n}\nfn main() {\n}\n",
        );
        assert_eq!(rows.get(&(String::new(), "f".to_owned())), Some(&None));
    }
}

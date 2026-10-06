//! is74 — the static half of kw07's volatile access and kw11's atomics
//! (wolf-interp#185, #194): the three codes the compiler reports at resolve.
//!
//! - **E1307** (`[mem.unsafe.volatile.1]`): `read_volatile`/`write_volatile`
//!   on a `*T` whose `T` is not a fixed-width integer or `byte` — at the
//!   method's name.
//! - **E1308** (`[conc.mm.atomic.raw.1]`): an atomic method on a `*T` whose
//!   `T` is not a fixed-width integer — at the method's name.
//! - **E1309** (`[conc.mm.atomic.order]`, `[conc.mm.atomic.raw.2]`,
//!   `[conc.mm.fence]`): an order operand that is not a mark (at the
//!   operand), a mark that is not one of the five (at the mark's name), a
//!   mark the operation does not admit (at the operand), and `Order` used
//!   as a value anywhere else (at the path).
//!
//! The spans are the compiler's on all three lanes (wolf 0.2.24, measured on
//! `tests/rulings_is74/`). Like the rest of sema-lite this walk never
//! guesses: a pointee it cannot read off the syntax — a cast, a `*T`
//! parameter or annotation, a local bound to one, `with_addr` of one — says
//! nothing here, and the evaluator declines such a call by name
//! (`eval::rawop`). The first finding in source order is the verdict
//! (`[proto.record.first]`); E1301, the ring, is the tier walk's.

use std::collections::BTreeSet;

use crate::ast::{
    Arg, Block, ElseHandler, Expr, ExprKind, FnDecl, IndexArg, Member, ParamKind, PatKind, Pattern,
    StmtKind, StrPart, Type, TypeKind,
};
use crate::diag::{Diag, Span};
use crate::eval::rawop::{ATOMIC, MARKS, VOLATILE, order_slots};
use crate::sema::{Def, Program};

/// What this walk knows of a raw pointer's pointee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pointee {
    /// `u8`…`u64`, `i8`…`i64`.
    Width,
    /// `byte`.
    Byte,
    /// A pointee the clauses name as refused: `int`, `uint`, `bool`, a
    /// float, `char`, `str`, a 128-bit integer, a pointer.
    Refused,
}

impl Pointee {
    fn of(ty: &Type) -> Option<Pointee> {
        match &*ty.kind {
            TypeKind::RawPointer(_) => Some(Pointee::Refused),
            TypeKind::Path { path, args } if args.is_empty() && path.is_single() => {
                match path.segments[0].name.as_str() {
                    "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" => {
                        Some(Pointee::Width)
                    }
                    "byte" => Some(Pointee::Byte),
                    "int" | "uint" | "bool" | "f32" | "f64" | "char" | "str" | "i128" | "u128" => {
                        Some(Pointee::Refused)
                    }
                    // An alias, a struct, an enum: not read here.
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// The pointee of a raw pointer TYPE, when the type is one and its pointee
/// is readable; `Some(None)` is a raw pointer whose pointee is not.
fn raw_pointee(ty: &Type) -> Option<Option<Pointee>> {
    match &*ty.kind {
        TypeKind::RawPointer(inner) => Some(Pointee::of(inner)),
        _ => None,
    }
}

/// The admitted orders of one operation (`[conc.mm.atomic.raw.2]`).
fn admits(method: &str, slot: usize, mark: &str, success: Option<&str>) -> bool {
    match method {
        "atomic_load" => matches!(mark, "relaxed" | "acquire" | "seq_cst"),
        "atomic_store" => matches!(mark, "relaxed" | "release" | "seq_cst"),
        "atomic_cas" if slot == 3 => match mark {
            "relaxed" => true,
            "acquire" => matches!(success, Some("acquire" | "acq_rel" | "seq_cst")),
            "seq_cst" => success == Some("seq_cst"),
            _ => false,
        },
        "fence" => mark != "relaxed",
        _ => true,
    }
}

/// Runs the walk over every fn and module binding; the first finding.
#[must_use]
pub fn raw_op_check(program: &Program) -> Option<Diag> {
    for module in program.modules.values() {
        let items: BTreeSet<String> = module.items.keys().cloned().collect();
        let mut fns: Vec<(&str, &FnDecl)> = module
            .items
            .iter()
            .filter_map(|(name, (def, _))| match def {
                Def::Fn(decl) => Some((
                    module.item_files.get(name).map_or("", String::as_str),
                    &**decl,
                )),
                _ => None,
            })
            .collect();
        fns.sort_by_key(|(file, decl)| (*file, decl.span.start));
        let methods = module
            .methods
            .values()
            .flat_map(|methods| methods.values().flatten().map(|m| &*m.decl));
        for decl in fns.into_iter().map(|(_, decl)| decl).chain(methods) {
            let mut walk = Walk::new(&items);
            for param in &decl.params {
                if let ParamKind::Named { name, ty } = &param.kind {
                    walk.declare(&name.name, raw_pointee(ty));
                }
            }
            if let Some(body) = &decl.body {
                walk.block(body);
            }
            if walk.found.is_some() {
                return walk.found;
            }
        }
        for (def, _) in module.items.values() {
            if let Def::Binding(binding) = def {
                let mut walk = Walk::new(&items);
                walk.expr(&binding.value);
                if walk.found.is_some() {
                    return walk.found;
                }
            }
        }
    }
    None
}

struct Walk<'a> {
    /// The module's top-level names: an item named `Order` or `fence`
    /// shadows the builtin (D32: the directory is the module).
    items: &'a BTreeSet<String>,
    /// Locals in scope: `Some(p)` a raw pointer (with its pointee, when
    /// readable), `None` any other binding of the name.
    scopes: Vec<Vec<(String, Option<Option<Pointee>>)>>,
    found: Option<Diag>,
}

impl<'a> Walk<'a> {
    fn new(items: &'a BTreeSet<String>) -> Walk<'a> {
        Walk {
            items,
            scopes: vec![Vec::new()],
            found: None,
        }
    }

    fn declare(&mut self, name: &str, raw: Option<Option<Pointee>>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name.to_owned(), raw));
        }
    }

    fn lookup(&self, name: &str) -> Option<Option<Option<Pointee>>> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|(n, _)| n == name)
            .map(|(_, raw)| *raw)
    }

    /// Whether `Order` here is the builtin: no local and no item takes it.
    fn order_is_builtin(&self) -> bool {
        self.lookup("Order").is_none() && !self.items.contains("Order")
    }

    fn report(&mut self, code: &'static str, span: Span, anchor: &'static str, message: String) {
        if self.found.is_none() {
            self.found = Some(Diag::new(code, span, anchor, message));
        }
    }

    /// The raw pointer an expression is, when the syntax shows one: a cast
    /// to `*T`, a raw local or parameter, `with_addr`/`with_exposed` of one,
    /// a block or group ending in one. `Some(None)`: raw, pointee unread.
    fn raw_of(&self, expr: &Expr) -> Option<Option<Pointee>> {
        match &*expr.kind {
            ExprKind::Group(inner) => self.raw_of(inner),
            ExprKind::Cast { ty, .. } => raw_pointee(ty),
            ExprKind::Path(path) if path.is_single() => {
                self.lookup(&path.segments[0].name).flatten()
            }
            ExprKind::Block(block) | ExprKind::Unsafe { body: block } => {
                block.tail.as_ref().and_then(|tail| self.raw_of(tail))
            }
            ExprKind::Call { callee, .. } => {
                let (receiver, method) = self.split(callee)?;
                matches!(method.name.as_str(), "with_addr" | "with_exposed")
                    .then(|| self.receiver_raw(receiver))
                    .flatten()
            }
            _ => None,
        }
    }

    fn receiver_raw(&self, receiver: Receiver<'_>) -> Option<Option<Pointee>> {
        match receiver {
            Receiver::Local(name) => self.lookup(name).flatten(),
            Receiver::Expr(expr) => self.raw_of(expr),
        }
    }

    /// A method call's receiver and method name, when the callee is one:
    /// `p.m` (a two-segment path whose head is a local) or `(e).m`.
    fn split<'e>(&self, callee: &'e Expr) -> Option<(Receiver<'e>, &'e crate::ast::Ident)> {
        match &*callee.kind {
            ExprKind::Path(path) if path.segments.len() == 2 => {
                let head = &path.segments[0].name;
                self.lookup(head)?;
                Some((Receiver::Local(head), &path.segments[1]))
            }
            ExprKind::Member {
                base,
                member: Member::Named(name),
            } => Some((Receiver::Expr(base), name)),
            _ => None,
        }
    }

    fn block(&mut self, block: &Block) {
        self.scopes.push(Vec::new());
        for stmt in &block.stmts {
            if self.found.is_some() {
                break;
            }
            match &stmt.kind {
                StmtKind::Binding(binding) => {
                    self.expr(&binding.value);
                    let raw = match &binding.ty {
                        Some(ty) => raw_pointee(ty),
                        None => self.raw_of(&binding.value),
                    };
                    match &*binding.pattern.kind {
                        PatKind::Binding(ident) => self.declare(&ident.name, raw),
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
                StmtKind::Item(_) => {}
            }
        }
        if let Some(tail) = &block.tail {
            self.expr(tail);
        }
        self.scopes.pop();
    }

    /// Every name a pattern binds, as a binding that is not a known pointer.
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

    /// An order operand (`[conc.mm.atomic.order]`): a mark, admitted by its
    /// operation. Answers the mark when it is one of the five.
    fn order_operand(
        &mut self,
        operand: &Expr,
        method: &str,
        slot: usize,
        success: Option<&str>,
    ) -> Option<&'static str> {
        // In an order operand the mark means the builtin whatever else
        // `Order` names in scope (`[conc.mm.atomic.order]`).
        if let ExprKind::Path(path) = &*operand.kind
            && let [head, mark] = path.segments.as_slice()
            && head.name == "Order"
        {
            let Some(known) = MARKS.iter().copied().find(|m| *m == mark.name) else {
                self.report(
                    "E1309",
                    mark.span,
                    "conc.mm.atomic.order",
                    format!(
                        "`{}` is not an order: `Order`'s five marks are `relaxed`, `acquire`, \
                         `release`, `acq_rel` and `seq_cst`",
                        mark.name
                    ),
                );
                return None;
            };
            if !admits(method, slot, known, success) {
                let why = match (method, slot) {
                    ("atomic_load", _) => "a load has no release half: it admits `relaxed`, \
                                           `acquire` and `seq_cst`"
                        .to_owned(),
                    ("atomic_store", _) => "a store has no acquire half: it admits `relaxed`, \
                                            `release` and `seq_cst`"
                        .to_owned(),
                    ("fence", _) => "a `relaxed` fence orders nothing".to_owned(),
                    _ => format!(
                        "a compare-and-swap's failure order is the order of a load, never a \
                         release order and never stronger than its success order `{}`",
                        success.unwrap_or("?")
                    ),
                };
                self.report(
                    "E1309",
                    operand.span,
                    "conc.mm.atomic.raw.2",
                    format!("`Order.{known}` is not admitted here: {why}"),
                );
            }
            return Some(known);
        }
        self.report(
            "E1309",
            operand.span,
            "conc.mm.atomic.order",
            format!(
                "the order operand of `{method}` is a mark written `Order.<mark>` at the call, \
                 not an expression: the order chooses the instruction, so it is known where \
                 the operation is compiled"
            ),
        );
        None
    }

    fn expr(&mut self, expr: &Expr) {
        if self.found.is_some() {
            return;
        }
        match &*expr.kind {
            ExprKind::Path(path) => {
                // `Order` used as a value, anywhere but an order operand.
                if let [head, _] = path.segments.as_slice()
                    && head.name == "Order"
                    && self.order_is_builtin()
                {
                    self.report(
                        "E1309",
                        expr.span,
                        "conc.mm.atomic.order",
                        "`Order` is not a value: a mark is written only as the order operand of \
                         an atomic operation or a fence"
                            .to_owned(),
                    );
                }
            }
            ExprKind::Call { callee, args } => self.call(callee, args),
            ExprKind::Closure { params, body, .. } => {
                self.scopes.push(Vec::new());
                for param in params {
                    let raw = param.ty.as_ref().and_then(raw_pointee);
                    self.declare(&param.name.name, raw);
                }
                self.expr(body);
                self.scopes.pop();
            }
            ExprKind::Block(block)
            | ExprKind::Loop { body: block }
            | ExprKind::RegionSugar { body: block, .. }
            | ExprKind::Scope { body: block, .. }
            | ExprKind::Unsafe { body: block }
            | ExprKind::When { body: block, .. } => {
                if let ExprKind::When { operands, .. } = &*expr.kind {
                    for operand in operands {
                        self.expr(operand);
                    }
                }
                if let ExprKind::RegionSugar { cap: Some(cap), .. } = &*expr.kind {
                    self.expr(cap);
                }
                if let ExprKind::Scope {
                    name: Some(name), ..
                } = &*expr.kind
                {
                    self.scopes.push(vec![(name.name.clone(), None)]);
                    self.block(block);
                    self.scopes.pop();
                } else {
                    self.block(block);
                }
            }
            ExprKind::In { region, body } => {
                self.expr(region);
                self.block(body);
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.expr(cond);
                self.block(then);
                if let Some(otherwise) = otherwise {
                    self.expr(otherwise);
                }
            }
            ExprKind::While { cond, body } => {
                self.expr(cond);
                self.block(body);
            }
            ExprKind::For {
                pattern,
                iter,
                body,
            } => {
                self.expr(iter);
                self.scopes.push(Vec::new());
                self.declare_pattern(pattern);
                self.block(body);
                self.scopes.pop();
            }
            ExprKind::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for arm in arms {
                    self.scopes.push(Vec::new());
                    self.declare_pattern(&arm.pattern);
                    if let Some(guard) = &arm.guard {
                        self.expr(guard);
                    }
                    self.expr(&arm.body);
                    self.scopes.pop();
                }
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
                        self.scopes.push(Vec::new());
                        self.declare_pattern(pattern);
                        self.expr(body);
                        self.scopes.pop();
                    }
                }
            }
            ExprKind::Str(lit) => {
                for part in &lit.parts {
                    if let StrPart::Interp(interp) = part {
                        self.expr(&interp.expr);
                        if let Some(parts) = &interp.format {
                            for fmt_part in parts {
                                if let crate::ast::FmtPart::Interp(inner) = fmt_part {
                                    self.expr(inner);
                                }
                            }
                        }
                    }
                }
            }
            ExprKind::StructLit { fields, .. } => {
                for field in fields {
                    self.expr(&field.value);
                }
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for item in items {
                    self.expr(item);
                }
            }
            ExprKind::Group(inner)
            | ExprKind::Try(inner)
            | ExprKind::FromEnd(inner)
            | ExprKind::Freeze(inner)
            | ExprKind::Cast { expr: inner, .. }
            | ExprKind::Unary { operand: inner, .. }
            | ExprKind::Member { base: inner, .. }
            | ExprKind::ModedReceiver { place: inner, .. } => self.expr(inner),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::BracketApply { base, args, .. } => {
                self.expr(base);
                for arg in args {
                    if let IndexArg::Value(arg) = arg {
                        self.expr(&arg.expr);
                    }
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(start) = start {
                    self.expr(start);
                }
                if let Some(end) = end {
                    self.expr(end);
                }
            }
            ExprKind::Return(value) | ExprKind::Break(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            ExprKind::SpawnProc { args, .. } => {
                for arg in args {
                    self.expr(&arg.expr);
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.kind {
                        crate::ast::SelectArmKind::Recv { channel, .. } => self.expr(channel),
                        crate::ast::SelectArmKind::Timeout(deadline) => self.expr(deadline),
                    }
                    self.expr(&arm.body);
                }
            }
            ExprKind::Asm { operands, .. } => {
                for operand in operands {
                    self.expr(&operand.value);
                }
            }
            ExprKind::Borrow { place, from } => {
                self.expr(place);
                self.expr(from);
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Char(_)
            | ExprKind::Wildcard
            | ExprKind::Continue
            | ExprKind::RegionValue { .. }
            | ExprKind::UnsafeC { .. } => {}
        }
    }

    fn call(&mut self, callee: &Expr, args: &[Arg]) {
        // `fence(o)`: the builtin unless a local or an item takes the name.
        if let ExprKind::Path(path) = &*callee.kind
            && path.is_single()
            && path.segments[0].name == "fence"
            && self.lookup("fence").is_none()
            && !self.items.contains("fence")
            && let [arg] = args
        {
            self.order_operand(&arg.expr, "fence", 0, None);
            return;
        }
        let Some((receiver, method)) = self.split(callee) else {
            self.expr(callee);
            for arg in args {
                self.expr(&arg.expr);
            }
            return;
        };
        if let Receiver::Expr(base) = receiver {
            self.expr(base);
        }
        let name = method.name.as_str();
        let raw = self.receiver_raw(receiver);
        let volatile = VOLATILE.contains(&name);
        let atomic = ATOMIC.contains(&name);
        if let Some(Some(pointee)) = raw {
            if volatile && pointee == Pointee::Refused {
                self.report(
                    "E1307",
                    method.span,
                    "mem.unsafe.volatile.1",
                    format!(
                        "`{name}` needs a pointee that is one machine access: a fixed-width \
                         integer (`u8`…`u64`, `i8`…`i64`) or `byte` — `int` and `uint` are the \
                         platform's integer, not a width, and a `bool`'s value set is restricted"
                    ),
                );
            }
            if atomic && pointee != Pointee::Width {
                self.report(
                    "E1308",
                    method.span,
                    "conc.mm.atomic.raw.1",
                    format!(
                        "`{name}` needs a fixed-width integer pointee (`u8`…`u64`, `i8`…`i64`): \
                         `int` and `uint` are not widths, `byte` has no arithmetic, a `bool`'s \
                         value set is restricted"
                    ),
                );
            }
        }
        // The order operands: judged where the receiver is a known raw
        // pointer, or where the operand is written `Order.…` — a user
        // method of the same name on another type takes no mark.
        let slots = if atomic { order_slots(name) } else { None };
        let mut success = None;
        for (i, arg) in args.iter().enumerate() {
            let is_slot = slots.is_some_and(|slots| slots.contains(&i));
            let spelled = matches!(&*arg.expr.kind, ExprKind::Path(path)
                if path.segments.len() == 2 && path.segments[0].name == "Order");
            if is_slot && (raw.is_some() || spelled) {
                let mark = self.order_operand(&arg.expr, name, i, success);
                if i == 2 {
                    success = mark;
                }
            } else {
                self.expr(&arg.expr);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Receiver<'e> {
    Local(&'e str),
    Expr(&'e Expr),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_admitted_orders_are_the_clause_s_table() {
        for mark in MARKS {
            assert_eq!(
                admits("atomic_load", 0, mark, None),
                !matches!(mark, "release" | "acq_rel")
            );
            assert_eq!(
                admits("atomic_store", 1, mark, None),
                !matches!(mark, "acquire" | "acq_rel")
            );
            assert!(admits("atomic_add", 1, mark, None));
            assert!(admits("atomic_cas", 2, mark, None));
            assert_eq!(admits("fence", 0, mark, None), mark != "relaxed");
        }
        // The nine admitted (success, failure) pairs, and no other.
        let mut pairs = Vec::new();
        for success in MARKS {
            for failure in MARKS {
                if admits("atomic_cas", 3, failure, Some(success)) {
                    pairs.push((success, failure));
                }
            }
        }
        assert_eq!(
            pairs,
            [
                ("relaxed", "relaxed"),
                ("acquire", "relaxed"),
                ("acquire", "acquire"),
                ("release", "relaxed"),
                ("acq_rel", "relaxed"),
                ("acq_rel", "acquire"),
                ("seq_cst", "relaxed"),
                ("seq_cst", "acquire"),
                ("seq_cst", "seq_cst"),
            ]
        );
    }
}

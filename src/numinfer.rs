//! is79 (wolf-interp#273; wolf-lang s252, #822's fifth finding, yew's
//! `wrapping-infer`) — the one case of `[type.numlit.value]`'s "a later use
//! in the same body types the binding" this machine reads statically: an
//! unannotated counter whose every deciding use is a `wrapping[T]`.
//!
//! `var i = 15` takes its type from its first deciding use — "defaulting is
//! applied once at the end of the enclosing body and a reachable expectation
//! gets there first" — and a shift's count has the operand's type
//! (`[type.int.shift]` (5)). So in s252's countdown row `h >> i * 4` with
//! `h: wrapping[u64]` makes `i` a `wrapping[u64]`, `i >= 0` never fails and
//! `i -= 1` wraps: forty turns on every compiling machine, where this
//! machine — which types an unannotated literal binding `i32` at the binding
//! (`[arith.literal.default]`) and reads mixed integer operands loosely —
//! ran sixteen.
//!
//! This machine has no checker and does not grow one here. The pass names a
//! binding's type only where the answer is CERTAIN, and only where the
//! answer changes what the program does — a `wrapping` type, whose
//! arithmetic wraps where `i32`'s traps and whose unsigned comparisons never
//! go negative. Everything else keeps the machine's `i32` reading, as it
//! always had. Certain means:
//!
//! - the binding is a single name, unannotated, initialized by a bare
//!   integer literal from 0 to `i32::MAX` (so the literal fits `i32` as the
//!   clause requires and the wrapping type alike), and it is the body's only
//!   binder of that name;
//! - every use of it is one this pass can read: an operand of an arithmetic,
//!   bitwise, shift or comparison operator, either side of an assignment, an
//!   interpolation hole, or a cast's operand (a cast decides nothing about
//!   its source); a use anywhere else — an argument, an index, a field, a
//!   return, a range — could be a deciding one the pass cannot type, and
//!   leaves the binding alone;
//! - the operands it meets (through literals and other such counters) are
//!   all of ONE declared type — a parameter the body never rebinds, a local
//!   annotated once, a module item's annotation, or a cast — and that type is
//!   `wrapping[T]`.
//!
//! The answer is written as the binding's annotation on the machine's own
//! copy of the program (`Machine::with_sched`), after every static check and
//! lint has read the program as written.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{
    BinOp, Block, Expr, ExprKind, FnDecl, ItemKind, ParamKind, PatKind, StmtKind, StrPart, Type,
    UnOp,
};
use crate::sema::{Def, Program};

/// Every function body's counters typed where the rule above is certain.
pub(crate) fn type_counters(program: &mut Program) {
    for module in program.modules.values_mut() {
        let items: BTreeMap<String, (String, Type)> = module
            .items
            .iter()
            .filter_map(|(name, (def, _))| match def {
                Def::Binding(binding) => {
                    let ty = binding.ty.as_ref()?;
                    let spelling = crate::lint::int_spelling(ty)?;
                    Some((name.clone(), (spelling, ty.clone())))
                }
                _ => None,
            })
            .collect();
        for (def, _) in module.items.values_mut() {
            if let Def::Fn(decl) = def {
                type_fn(decl, &items);
            }
        }
        for by_name in module.methods.values_mut() {
            for methods in by_name.values_mut() {
                for method in methods {
                    type_fn(&mut method.decl, &items);
                }
            }
        }
        for by_name in module.trait_defaults.values_mut() {
            for decl in by_name.values_mut() {
                type_fn(decl, &items);
            }
        }
    }
}

/// One body: decide, then write each decided counter's annotation (and the
/// nested `fn` items' bodies, each its own).
fn type_fn(decl: &mut FnDecl, items: &BTreeMap<String, (String, Type)>) {
    let Some(body) = decl.body.as_ref() else {
        return;
    };
    let decided = decide(decl, body, items);
    if let Some(body) = decl.body.as_mut() {
        write_block(body, &decided, items);
    }
}

fn write_block(
    block: &mut Block,
    decided: &BTreeMap<(usize, usize), Type>,
    items: &BTreeMap<String, (String, Type)>,
) {
    for stmt in &mut block.stmts {
        match &mut stmt.kind {
            StmtKind::Binding(binding) => {
                if binding.ty.is_none()
                    && let Some(ty) = decided.get(&(binding.span.start, binding.span.end))
                {
                    binding.ty = Some(ty.clone());
                }
                write_expr(&mut binding.value, decided, items);
            }
            StmtKind::Assign { place, value, .. } => {
                write_expr(place, decided, items);
                write_expr(value, decided, items);
            }
            StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => {
                write_expr(expr, decided, items);
            }
            StmtKind::AssumeNoalias(es) => {
                for e in es {
                    write_expr(e, decided, items);
                }
            }
            StmtKind::Item(item) => {
                if let ItemKind::Fn(decl) = &mut item.kind {
                    type_fn(decl, items);
                }
            }
        }
    }
    if let Some(tail) = &mut block.tail {
        write_expr(tail, decided, items);
    }
}

/// The blocks inside an expression, where a counter's binding may sit.
fn write_expr(
    expr: &mut Expr,
    decided: &BTreeMap<(usize, usize), Type>,
    items: &BTreeMap<String, (String, Type)>,
) {
    match &mut *expr.kind {
        ExprKind::Block(block)
        | ExprKind::Loop { body: block }
        | ExprKind::Unsafe { body: block } => write_block(block, decided, items),
        ExprKind::While { cond, body } => {
            write_expr(cond, decided, items);
            write_block(body, decided, items);
        }
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            write_expr(cond, decided, items);
            write_block(then, decided, items);
            if let Some(other) = otherwise {
                write_expr(other, decided, items);
            }
        }
        ExprKind::For { iter, body, .. } => {
            write_expr(iter, decided, items);
            write_block(body, decided, items);
        }
        _ => {}
    }
}

/// What an expression is, to this pass.
#[derive(Clone)]
enum Info {
    /// Literals and counters only: their type is still open, and they share
    /// it (the counters listed).
    Open(Vec<usize>),
    /// A value of this declared integer type (its spelling, the type).
    Known(String, Type),
    /// Anything else: not an integer this pass can type.
    Other,
}

/// A name in scope.
#[derive(Clone)]
enum Name {
    Counter(usize),
    Known(String, Type),
    Opaque,
}

struct Counter {
    key: (usize, usize),
    parent: usize,
    decided: Option<(String, Type)>,
    conflict: bool,
    left_alone: bool,
}

struct Decide<'a> {
    counters: Vec<Counter>,
    scopes: Vec<Vec<(String, Name)>>,
    /// How many binders each name has in the body (any pattern, any depth).
    binders: BTreeMap<String, usize>,
    params: BTreeSet<String>,
    items: &'a BTreeMap<String, (String, Type)>,
}

fn decide(
    decl: &FnDecl,
    body: &Block,
    items: &BTreeMap<String, (String, Type)>,
) -> BTreeMap<(usize, usize), Type> {
    let binders = crate::lint::bound_name_counts(body);
    let mut walk = Decide {
        counters: Vec::new(),
        scopes: vec![Vec::new()],
        binders,
        params: BTreeSet::new(),
        items,
    };
    for param in &decl.params {
        if let ParamKind::Named { name, ty } = &param.kind {
            walk.params.insert(name.name.clone());
            let named = match crate::lint::int_spelling(ty) {
                Some(spelling) if !walk.binders.contains_key(&name.name) => {
                    Name::Known(spelling, ty.clone())
                }
                _ => Name::Opaque,
            };
            walk.declare(&name.name, named);
        }
    }
    walk.block(body);
    let mut out = BTreeMap::new();
    for id in 0..walk.counters.len() {
        let root = walk.root(id);
        let group = &walk.counters[root];
        if let Some((spelling, ty)) = &group.decided
            && !group.conflict
            && !group.left_alone
            && spelling.starts_with("wrapping[")
        {
            out.insert(walk.counters[id].key, ty.clone());
        }
    }
    out
}

impl Decide<'_> {
    fn root(&mut self, mut id: usize) -> usize {
        while self.counters[id].parent != id {
            let up = self.counters[id].parent;
            self.counters[id].parent = self.counters[up].parent;
            id = up;
        }
        id
    }

    fn declare(&mut self, name: &str, named: Name) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name.to_owned(), named));
        }
    }

    fn lookup(&self, name: &str) -> Name {
        for scope in self.scopes.iter().rev() {
            if let Some((_, named)) = scope.iter().rev().find(|(n, _)| n == name) {
                return named.clone();
            }
        }
        match self.items.get(name) {
            Some((spelling, ty))
                if !self.binders.contains_key(name) && !self.params.contains(name) =>
            {
                Name::Known(spelling.clone(), ty.clone())
            }
            _ => Name::Opaque,
        }
    }

    /// The counters share one type from here on.
    fn link(&mut self, ids: &[usize]) {
        let Some((&first, rest)) = ids.split_first() else {
            return;
        };
        let a = self.root(first);
        for &id in rest {
            let b = self.root(id);
            if a == b {
                continue;
            }
            self.counters[b].parent = a;
            let moved = self.counters[b].decided.take();
            let (conflict, left_alone) = (self.counters[b].conflict, self.counters[b].left_alone);
            let group = &mut self.counters[a];
            group.conflict |= conflict;
            group.left_alone |= left_alone;
            match (&group.decided, moved) {
                (Some((x, _)), Some((y, _))) if *x != y => group.conflict = true,
                (None, Some(d)) => group.decided = Some(d),
                _ => {}
            }
        }
    }

    /// The counters meet a value of `spelling`'s type.
    fn settle(&mut self, ids: &[usize], spelling: &str, ty: &Type) {
        for &id in ids {
            let root = self.root(id);
            let group = &mut self.counters[root];
            match &group.decided {
                Some((have, _)) if have != spelling => group.conflict = true,
                Some(_) => {}
                None => group.decided = Some((spelling.to_owned(), ty.clone())),
            }
        }
    }

    /// The counters were used where this pass cannot tell what decides them.
    fn leave_alone(&mut self, info: &Info) {
        if let Info::Open(ids) = info {
            for &id in ids {
                let root = self.root(id);
                self.counters[root].left_alone = true;
            }
        }
    }

    /// Two operands of one type (an arithmetic, bitwise, shift or
    /// comparison operator, or an assignment's two sides).
    fn meet(&mut self, left: Info, right: Info) -> Info {
        match (left, right) {
            (Info::Open(mut a), Info::Open(b)) => {
                a.extend(b);
                self.link(&a);
                Info::Open(a)
            }
            (Info::Open(a), Info::Known(spelling, ty))
            | (Info::Known(spelling, ty), Info::Open(a)) => {
                self.settle(&a, &spelling, &ty);
                Info::Known(spelling, ty)
            }
            (Info::Known(spelling, ty), Info::Known(..)) => Info::Known(spelling, ty),
            (open @ Info::Open(_), Info::Other) | (Info::Other, open @ Info::Open(_)) => {
                self.leave_alone(&open);
                Info::Other
            }
            _ => Info::Other,
        }
    }

    fn block(&mut self, block: &Block) -> Info {
        self.scopes.push(Vec::new());
        for stmt in &block.stmts {
            match &stmt.kind {
                StmtKind::Binding(binding) => {
                    let value = self.expr(&binding.value);
                    let PatKind::Binding(ident) = &*binding.pattern.kind else {
                        self.leave_alone(&value);
                        // The names a destructuring binds are opaque here.
                        let mut names = BTreeSet::new();
                        pattern_names(&binding.pattern, &mut names);
                        for name in names {
                            self.declare(&name, Name::Opaque);
                        }
                        continue;
                    };
                    let once = self.binders.get(&ident.name).copied() == Some(1)
                        && !self.params.contains(&ident.name);
                    let named = match &binding.ty {
                        Some(ty) => match crate::lint::int_spelling(ty) {
                            Some(spelling) => {
                                if let Info::Open(ids) = &value {
                                    let ids = ids.clone();
                                    self.settle(&ids, &spelling, ty);
                                }
                                if once {
                                    Name::Known(spelling, ty.clone())
                                } else {
                                    Name::Opaque
                                }
                            }
                            None => {
                                self.leave_alone(&value);
                                Name::Opaque
                            }
                        },
                        None => match (&*binding.value.kind, &value) {
                            (ExprKind::Int(text), Info::Open(ids))
                                if ids.is_empty()
                                    && once
                                    && crate::lint::parse_int_literal(text).is_some_and(|v| {
                                        (0..=i128::from(i32::MAX)).contains(&v)
                                    }) =>
                            {
                                let id = self.counters.len();
                                self.counters.push(Counter {
                                    key: (binding.span.start, binding.span.end),
                                    parent: id,
                                    decided: None,
                                    conflict: false,
                                    left_alone: false,
                                });
                                Name::Counter(id)
                            }
                            (_, Info::Known(spelling, ty)) if once => {
                                Name::Known(spelling.clone(), ty.clone())
                            }
                            _ => {
                                self.leave_alone(&value);
                                Name::Opaque
                            }
                        },
                    };
                    self.declare(&ident.name, named);
                }
                StmtKind::Assign { place, value, .. } => {
                    let place = self.expr(place);
                    let value = self.expr(value);
                    self.meet(place, value);
                }
                StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => {
                    self.expr(expr);
                }
                StmtKind::AssumeNoalias(es) => {
                    for e in es {
                        let info = self.expr(e);
                        self.leave_alone(&info);
                    }
                }
                // A nested `fn` is its own body, typed by its own pass.
                StmtKind::Item(_) => {}
            }
        }
        let tail = match &block.tail {
            Some(tail) => self.expr(tail),
            None => Info::Other,
        };
        self.scopes.pop();
        tail
    }

    fn expr(&mut self, expr: &Expr) -> Info {
        match &*expr.kind {
            ExprKind::Int(_) => Info::Open(Vec::new()),
            ExprKind::Group(inner) => self.expr(inner),
            ExprKind::Path(path) if path.is_single() => match self.lookup(&path.segments[0].name) {
                Name::Counter(id) => Info::Open(vec![id]),
                Name::Known(spelling, ty) => Info::Known(spelling, ty),
                Name::Opaque => Info::Other,
            },
            ExprKind::Unary {
                op: UnOp::Neg | UnOp::Not,
                operand,
            } => self.expr(operand),
            ExprKind::Binary { op, lhs, rhs } => {
                let left = self.expr(lhs);
                let right = self.expr(rhs);
                match op {
                    BinOp::And | BinOp::Or => {
                        self.leave_alone(&left);
                        self.leave_alone(&right);
                        Info::Other
                    }
                    BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::Le
                    | BinOp::Ge
                    | BinOp::Cmp => {
                        self.meet(left, right);
                        Info::Other
                    }
                    _ => self.meet(left, right),
                }
            }
            // A cast converts from any integer type: it decides nothing about
            // its operand, and its own type is the one it names.
            ExprKind::Cast { expr: inner, ty } => {
                self.expr(inner);
                match crate::lint::int_spelling(ty) {
                    Some(spelling) => Info::Known(spelling, ty.clone()),
                    None => Info::Other,
                }
            }
            // An interpolation hole renders any integer: it decides nothing.
            ExprKind::Str(lit) => {
                for part in &lit.parts {
                    if let StrPart::Interp(interp) = part {
                        self.expr(&interp.expr);
                    }
                }
                Info::Other
            }
            ExprKind::Block(block) => self.block(block),
            // Every other shape: its children are read, and an open value
            // reaching one of them is a use this pass cannot type.
            _ => {
                let mut children: Vec<Info> = Vec::new();
                let mut blocks: Vec<&Block> = Vec::new();
                let mut exprs: Vec<&Expr> = Vec::new();
                crate::rowmatch::each_child_with_blocks(expr, &mut |child| match child {
                    crate::rowmatch::Child::Expr(child) => exprs.push(child),
                    crate::rowmatch::Child::Block(block) => blocks.push(block),
                });
                for child in exprs {
                    children.push(self.expr(child));
                }
                for block in blocks {
                    children.push(self.block(block));
                }
                for info in &children {
                    self.leave_alone(info);
                }
                Info::Other
            }
        }
    }
}

/// Every name a pattern binds.
fn pattern_names(pattern: &crate::ast::Pattern, out: &mut BTreeSet<String>) {
    match &*pattern.kind {
        PatKind::Binding(ident) => {
            out.insert(ident.name.clone());
        }
        PatKind::At { name, pattern } => {
            out.insert(name.name.clone());
            pattern_names(pattern, out);
        }
        PatKind::Variant { fields, .. } => fields.iter().for_each(|p| pattern_names(p, out)),
        PatKind::Tuple(parts) | PatKind::Or(parts) => {
            parts.iter().for_each(|p| pattern_names(p, out));
        }
        PatKind::Struct { fields, .. } => {
            for field in fields {
                match &field.pattern {
                    Some(p) => pattern_names(p, out),
                    None => {
                        out.insert(field.name.name.clone());
                    }
                }
            }
        }
        PatKind::Wildcard | PatKind::Literal(_) | PatKind::Range { .. } | PatKind::Path(_) => {}
    }
}

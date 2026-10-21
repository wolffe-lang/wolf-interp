//! Module state and the link — lupin's half of KWC K11 = A and K6 (kw09;
//! wolf-interp#190, is73): `[mem.static]`, `[abi.link.extern]`,
//! `[abi.link.section]`.
//!
//! On this machine a module item is ordinary memory, evaluated once
//! (`eval::Machine::initialize`); what the clauses add is static:
//!
//! - [`extern_let_check`]: `extern "c" let NAME: *T` is a module item with
//!   no initializer whose type is a raw pointer; anything else is E0821.
//! - [`init_check`]: an initializer is comptime, so one that reads a module
//!   `var`, or needs its own value, is E0705 at the reference (`[mem.static.3]`).
//!   0.1.46 recursed on a cycle until the stack overflowed, with no record.
//! - [`not_static_data`]: module state holds the integers, `byte`, `bool`,
//!   the floats, and `str` in a `let`/`const`; any other written type is
//!   refused by name.
//! - [`section_placement`]: a program that places a section has no meaning
//!   without an image, so it is refused by name.
//!
//! E1301 for a module `var` touched outside `unsafe` lives in the tier walk
//! (`sema::tier_check`), beside the ring's other operations.
//!
//! Every code and span is the compiler's on all three of its lanes,
//! measured with wolf 0.2.23 (`tests/rulings_is73/`).

use std::collections::BTreeSet;

use crate::ast::{BindingKind, Block, Expr, ExprKind, Item, ItemKind, StmtKind, TypeKind};
use crate::diag::{Diag, Span};
use crate::rowmatch::{Child, each_child_with_blocks};
use crate::sema::{Def, Module, Program};

/// The root module first, then the rest in key order — the order the
/// record's one file is chosen in.
fn modules(program: &Program) -> impl Iterator<Item = &Module> {
    let root = program.root();
    std::iter::once(root).chain(
        program
            .modules
            .values()
            .filter(move |module| !std::ptr::eq(*module, root)),
    )
}

fn e0821(span: Span, message: String) -> Diag {
    Diag::new("E0821", span, "abi.c.seams", message)
}

/// E0821 for one `extern "c" let` written at a module's top level.
fn judge_extern(def: &crate::ast::ExternLet) -> Option<Diag> {
    let name = &def.name.name;
    if def.kind != BindingKind::Let {
        let word = if def.kind == BindingKind::Var {
            "var"
        } else {
            "const"
        };
        return Some(e0821(
            def.kind_span,
            format!(
                "`extern \"c\" {word} {name}`: a link-time symbol is named with `let` — its \
                 value is the symbol's address, which nothing writes ([abi.link.extern])"
            ),
        ));
    }
    match &def.ty {
        None => {
            return Some(e0821(
                def.name.span,
                format!(
                    "`extern \"c\" let {name}` names no type: the symbol's address is a raw \
                     pointer, `extern \"c\" let {name}: *u8` ([abi.link.extern])"
                ),
            ));
        }
        Some(ty) if !matches!(&*ty.kind, TypeKind::RawPointer(_)) => {
            return Some(e0821(
                ty.span,
                format!(
                    "`extern \"c\" let {name}` names a symbol's ADDRESS, so its type is a raw \
                     pointer (`*T`), not this; read the value through the pointer, inside \
                     `unsafe` ([abi.link.extern])"
                ),
            ));
        }
        Some(_) => {}
    }
    def.value.as_ref().map(|value| {
        e0821(
            value.span,
            format!(
                "`extern \"c\" let {name}` has no initializer: the link defines the symbol \
                 ([abi.link.extern])"
            ),
        )
    })
}

fn nested_externs(block: &Block, out: &mut Vec<Diag>) {
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Item(item) => {
                if let ItemKind::ExternLet(def) = &item.kind {
                    out.push(e0821(
                        def.span,
                        format!(
                            "`extern \"c\" let {}` is a module item: a link-time symbol is \
                             named at a module's top level, not inside a body \
                             ([abi.link.extern])",
                            def.name.name
                        ),
                    ));
                }
                if let ItemKind::Fn(decl) = &item.kind
                    && let Some(body) = &decl.body
                {
                    nested_externs(body, out);
                }
            }
            StmtKind::Binding(binding) => nested_externs_expr(&binding.value, out),
            StmtKind::Assign { place, value, .. } => {
                nested_externs_expr(place, out);
                nested_externs_expr(value, out);
            }
            StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => nested_externs_expr(expr, out),
            StmtKind::AssumeNoalias(_) => {}
        }
    }
    if let Some(tail) = &block.tail {
        nested_externs_expr(tail, out);
    }
}

fn nested_externs_expr(expr: &Expr, out: &mut Vec<Diag>) {
    each_child_with_blocks(expr, &mut |child| match child {
        Child::Expr(child) => nested_externs_expr(child, out),
        Child::Block(block) => nested_externs(block, out),
    });
}

fn item_bodies(item: &Item, out: &mut Vec<Diag>) {
    match &item.kind {
        ItemKind::Fn(decl) => {
            if let Some(body) = &decl.body {
                nested_externs(body, out);
            }
        }
        ItemKind::Impl(def) => {
            for member in &def.members {
                item_bodies(member, out);
            }
        }
        ItemKind::Trait(def) => {
            for member in &def.members {
                item_bodies(member, out);
            }
        }
        ItemKind::Binding(binding) => nested_externs_expr(&binding.value, out),
        _ => {}
    }
}

/// Every E0821 the program earns (`[abi.link.extern]`), in source order
/// within the first file that earns any.
#[must_use]
pub fn extern_let_check(program: &Program) -> Vec<Diag> {
    for module in modules(program) {
        for unit in &module.units {
            let mut out = Vec::new();
            for item in &unit.unit.items {
                if let ItemKind::ExternLet(def) = &item.kind {
                    out.extend(judge_extern(def));
                }
                item_bodies(item, &mut out);
            }
            if !out.is_empty() {
                out.sort_by_key(|d| (d.span.start, d.span.end));
                return out
                    .into_iter()
                    .map(|d| d.in_file(unit.file.clone()))
                    .collect();
            }
        }
    }
    Vec::new()
}

/// The module items an initializer names, in evaluation order (left to
/// right, a callee before its arguments), stopping where the comptime
/// engine would stop reading: a closure's body runs later, and an `unsafe`
/// block is outside the engine's subset (the compiler declines it by name).
/// A name a block inside the initializer binds shadows the item.
fn references(expr: &Expr, shadow: &mut Vec<String>, out: &mut Vec<(String, Span)>) {
    match &*expr.kind {
        ExprKind::Path(path) => {
            let name = &path.segments[0].name;
            if !shadow.iter().any(|s| s == name) {
                out.push((name.clone(), path.segments[0].span));
            }
        }
        ExprKind::Closure { .. } | ExprKind::Unsafe { .. } | ExprKind::UnsafeC { .. } => {}
        _ => each_child_with_blocks(expr, &mut |child| match child {
            Child::Expr(child) => references(child, shadow, out),
            Child::Block(block) => {
                let depth = shadow.len();
                for stmt in &block.stmts {
                    match &stmt.kind {
                        StmtKind::Binding(binding) => {
                            references(&binding.value, shadow, out);
                            if let crate::ast::PatKind::Binding(ident) = &*binding.pattern.kind {
                                shadow.push(ident.name.clone());
                            }
                        }
                        StmtKind::Expr(expr) | StmtKind::Defer { expr, .. } => {
                            references(expr, shadow, out);
                        }
                        StmtKind::Assign { place, value, .. } => {
                            references(place, shadow, out);
                            references(value, shadow, out);
                        }
                        StmtKind::AssumeNoalias(_) | StmtKind::Item(_) => {}
                    }
                }
                if let Some(tail) = &block.tail {
                    references(tail, shadow, out);
                }
                shadow.truncate(depth);
            }
        }),
    }
}

/// The comptime engine's walk over one module's initializers: each item in
/// declaration order, its references in evaluation order. A reference to a
/// `var`, or to an item whose evaluation is still open, is E0705 there and
/// ends that item's evaluation; a failed item is not remembered, so a later
/// item that needs it fails again (the compiler reports the inner reference
/// once per evaluation that reaches it). Depth is bounded: past 256 open
/// items the walk stops judging, and the evaluator's own rail answers.
struct InitWalk<'a> {
    module: &'a Module,
    good: BTreeSet<String>,
    open: Vec<String>,
    out: Vec<Diag>,
}

impl InitWalk<'_> {
    fn visit(&mut self, name: &str) -> bool {
        if self.good.contains(name) {
            return true;
        }
        let Some((Def::Binding(binding), _)) = self.module.items.get(name) else {
            return true;
        };
        if self.open.len() > 256 {
            return true;
        }
        self.open.push(name.to_owned());
        let mut refs = Vec::new();
        references(&binding.value, &mut Vec::new(), &mut refs);
        let mut ok = true;
        for (target, span) in refs {
            let Some((Def::Binding(other), _)) = self.module.items.get(&target) else {
                continue;
            };
            if other.kind == BindingKind::Var {
                self.out.push(Diag::new(
                    "E0705",
                    span,
                    "gram.item.let",
                    format!(
                        "`{target}` is a module `var`, whose value changes at run time: an \
                         initializer is evaluated at compile time, so it may name a `const` or \
                         a `let`, never a `var` ([mem.static.3])"
                    ),
                ));
                ok = false;
                break;
            }
            if self.open.contains(&target) {
                self.out.push(Diag::new(
                    "E0705",
                    span,
                    "gram.item.let",
                    format!(
                        "`{target}`'s initializer needs its own value: `{}` -> `{target}` \
                         ([mem.static.3]). An initializer may name any other `const` or `let` \
                         of its module, before or after it, but not one that depends on it",
                        self.open.join("` -> `")
                    ),
                ));
                ok = false;
                break;
            }
            if !self.visit(&target) {
                ok = false;
                break;
            }
        }
        self.open.pop();
        if ok {
            self.good.insert(name.to_owned());
        }
        ok
    }
}

/// Every E0705 the module initializers earn (`[mem.static.3]`), in source
/// order within the first module that earns any.
#[must_use]
pub fn init_check(program: &Program) -> Vec<Diag> {
    for module in modules(program) {
        let mut walk = InitWalk {
            module,
            good: BTreeSet::new(),
            open: Vec::new(),
            out: Vec::new(),
        };
        for name in &module.bindings {
            walk.visit(name);
        }
        if !walk.out.is_empty() {
            let mut out = walk.out;
            out.sort_by_key(|d| (d.span.start, d.span.end));
            return out;
        }
    }
    Vec::new()
}

/// The written type of a module binding is static data (`[mem.static.3]`):
/// the integers, `byte`, `bool`, the floats, and `str` for a `let` or
/// `const`. An unwritten type is not judged here.
fn static_data(kind: BindingKind, ty: &crate::ast::Type) -> bool {
    let TypeKind::Path { path, args } = &*ty.kind else {
        return false;
    };
    if !path.is_single() || !args.is_empty() {
        return false;
    }
    match path.segments[0].name.as_str() {
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "int" | "uint" | "byte"
        | "bool" | "f32" | "f64" => true,
        "str" => kind != BindingKind::Var,
        _ => false,
    }
}

/// `[mem.static.3]`: module state of a type that is not static data is
/// refused by name, never initialized at some run-time moment the program
/// cannot see. Judged before the module-`var` E1301 so a program the
/// compiler refuses for a reason this machine has no static half of
/// (`memory/read_param_escape_static.lu`'s E1002) is declined by name, not
/// answered with a different code.
#[must_use]
pub fn not_static_data(program: &Program) -> Option<String> {
    for module in modules(program) {
        for name in &module.bindings {
            if let Some((Def::Binding(binding), _)) = module.items.get(name)
                && let Some(ty) = &binding.ty
                && !static_data(binding.kind, ty)
            {
                return Some(format!(
                    "module state of a type that is not static data ([mem.static.3]): `{name}` — \
                     module state holds the integers, `byte`, `bool` and the floats (and `str` \
                     in a `const` or `let`) at this cut"
                ));
            }
        }
    }
    None
}

/// `[abi.link.extern]`: the checked machine and lupin model no link, so a
/// program that declares a link-time symbol is refused by name — the checked
/// machine's answer even where nothing names it (measured, wolf 0.2.23).
#[must_use]
pub fn link_symbol(program: &Program) -> Option<String> {
    for module in modules(program) {
        for unit in &module.units {
            for item in &unit.unit.items {
                if let ItemKind::ExternLet(def) = &item.kind {
                    let name = &def.name.name;
                    return Some(format!(
                        "a link-time symbol (`extern \"c\" let {name}`): the link-time symbol \
                         `{name}` is defined by the image's link, which this machine does not \
                         model ([abi.link.extern])"
                    ));
                }
            }
        }
    }
    None
}

fn section_of(item: &Item) -> bool {
    item.attrs
        .iter()
        .flat_map(|a| &a.attrs)
        .any(|attr| attr.path.is_single() && attr.path.segments[0].name == "section")
}

/// `[abi.link.section]`: this machine has no image, so a program that
/// places a section is refused by name, never run as if the attribute were
/// absent. The attribute check has already refused every misplaced or
/// malformed `#[section]`.
#[must_use]
pub fn section_placement(program: &Program) -> Option<String> {
    for module in modules(program) {
        for unit in &module.units {
            for item in &unit.unit.items {
                if section_of(item) {
                    return Some(
                        "section placement: `#[section(\"…\")]` places code or data in an \
                         object section, and this machine has no image ([abi.link.section])"
                            .to_owned(),
                    );
                }
            }
        }
    }
    None
}

// -- is76 (wolf-interp#251): a module initialiser folded by the rule ----------

/// A plain integer type a module item may be written with
/// ([`static_data`]'s integers): its width and signedness.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Plain {
    name: &'static str,
    bits: u32,
    signed: bool,
}

impl Plain {
    fn of(ty: &crate::ast::Type) -> Option<Plain> {
        let TypeKind::Path { path, args } = &*ty.kind else {
            return None;
        };
        if !path.is_single() || !args.is_empty() {
            return None;
        }
        let (name, bits, signed) = match path.segments[0].name.as_str() {
            "i8" => ("i8", 8, true),
            "i16" => ("i16", 16, true),
            "i32" => ("i32", 32, true),
            "i64" => ("i64", 64, true),
            "int" => ("int", 64, true),
            "u8" => ("u8", 8, false),
            "u16" => ("u16", 16, false),
            "u32" => ("u32", 32, false),
            "u64" => ("u64", 64, false),
            "uint" => ("uint", 64, false),
            _ => return None,
        };
        Some(Plain { name, bits, signed })
    }

    fn holds(self, v: i128) -> bool {
        if self.signed {
            let half = 1i128 << (self.bits - 1);
            (-half..half).contains(&v)
        } else {
            (0..(1i128 << self.bits)).contains(&v)
        }
    }
}

/// What folding one expression found.
enum Folded {
    /// The value, in range for the item's type.
    Value(i128),
    /// The comptime fault: E0706 at the operator that faulted.
    Fault(Diag),
    /// A shape this fold does not read — it says nothing, and the run (or a
    /// refusal by name) answers instead.
    Unread,
}

/// An integer literal's value, as the lexer spelled it.
fn literal(text: &str) -> Option<i128> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    let (radix, digits) = if let Some(rest) = cleaned.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = cleaned.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = cleaned.strip_prefix("0b") {
        (2, rest)
    } else {
        (10, cleaned.as_str())
    };
    i128::from_str_radix(digits, radix).ok()
}

/// The fold over one module's items: each `const`/`let` with a plain integer
/// type, its initialiser read as integer literals, `-`, `(…)` groups, the
/// arithmetic and bitwise operators and names of other such items of the
/// SAME type — every operator computed at the item's type by the rule the
/// run uses (`[type.int.shift]` (7): "a constant shift is evaluated by the
/// same rule, so `const K: u8 = 200 << 1` is E0706").
struct FoldWalk<'a> {
    module: &'a Module,
    /// Each item read so far: its value (`None`: not read here), or its fault.
    done: std::collections::BTreeMap<String, Result<Option<i128>, Diag>>,
    open: Vec<String>,
}

impl FoldWalk<'_> {
    /// The value of item `name` at its type, `None` when it is not read
    /// here; `Err` carries the first fault its evaluation met.
    fn item(&mut self, name: &str) -> Result<Option<(Plain, i128)>, Diag> {
        let Some((Def::Binding(binding), _)) = self.module.items.get(name) else {
            return Ok(None);
        };
        // Past 64 open items the fold stops judging (`InitWalk` stops at
        // 256; a fold's frames are wider): a chain that deep is the
        // evaluator's rail to answer, and a fold that followed it overflowed
        // a small stack — windows' 1 MiB main thread on `rulings_is73`'s
        // 2000-item chain (the windows smoke of run 38975226904); 256 still
        // overflowed 1 MiB in a debug build on kasumi, 64 does not.
        if binding.kind == BindingKind::Var
            || self.open.len() > 64
            || self.open.iter().any(|o| o == name)
        {
            return Ok(None);
        }
        let Some(ty) = binding.ty.as_ref().and_then(Plain::of) else {
            return Ok(None);
        };
        if let Some(done) = self.done.get(name) {
            return done.clone().map(|v| v.map(|v| (ty, v)));
        }
        self.open.push(name.to_owned());
        let folded = self.fold(&binding.value, ty);
        self.open.pop();
        match folded {
            Folded::Value(v) => {
                self.done.insert(name.to_owned(), Ok(Some(v)));
                Ok(Some((ty, v)))
            }
            Folded::Unread => {
                self.done.insert(name.to_owned(), Ok(None));
                Ok(None)
            }
            Folded::Fault(diag) => {
                self.done.insert(name.to_owned(), Err(diag.clone()));
                Err(diag)
            }
        }
    }

    fn fold(&mut self, expr: &Expr, ty: Plain) -> Folded {
        match &*expr.kind {
            ExprKind::Group(inner) => self.fold(inner, ty),
            ExprKind::Int(text) => match literal(text) {
                // A literal that does not fit is the checker's E0415, not
                // this fold's: say nothing.
                Some(v) if ty.holds(v) => Folded::Value(v),
                _ => Folded::Unread,
            },
            ExprKind::Unary {
                op: crate::ast::UnOp::Neg,
                operand,
            } => match &*operand.kind {
                // `-128` at `i8` is the literal -128.
                ExprKind::Int(text) => match literal(text).map(|v| -v) {
                    Some(v) if ty.holds(v) => Folded::Value(v),
                    _ => Folded::Unread,
                },
                _ => match self.fold(operand, ty) {
                    Folded::Value(v) => {
                        if ty.holds(-v) {
                            Folded::Value(-v)
                        } else {
                            Folded::Fault(fault(
                                expr.span,
                                "-",
                                ty,
                                &format!("-({v}) leaves `{}`'s range", ty.name),
                            ))
                        }
                    }
                    other => other,
                },
            },
            ExprKind::Path(path) if path.is_single() => {
                match self.item(&path.segments[0].name) {
                    Ok(Some((other, v))) if other == ty => Folded::Value(v),
                    Ok(_) => Folded::Unread,
                    // The fault is that item's own, reported at its own
                    // operator when the walk reaches it.
                    Err(_) => Folded::Unread,
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                use crate::ast::BinOp;
                let spelling = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Rem => "%",
                    BinOp::Shl => "<<",
                    BinOp::Shr => ">>",
                    BinOp::BitAnd => "&",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    _ => return Folded::Unread,
                };
                let a = match self.fold(lhs, ty) {
                    Folded::Value(v) => v,
                    other => return other,
                };
                let b = match self.fold(rhs, ty) {
                    Folded::Value(v) => v,
                    other => return other,
                };
                let why = |what: &str| fault(expr.span, spelling, ty, what);
                let computed = match op {
                    BinOp::Add => a.checked_add(b),
                    BinOp::Sub => a.checked_sub(b),
                    BinOp::Mul => a.checked_mul(b),
                    BinOp::Div | BinOp::Rem if b == 0 => {
                        return Folded::Fault(why("division by zero"));
                    }
                    BinOp::Div => a.checked_div(b),
                    BinOp::Rem => a.checked_rem(b),
                    BinOp::Shl | BinOp::Shr if b < 0 => {
                        return Folded::Fault(why(&format!(
                            "a shift by {b}: the count is negative"
                        )));
                    }
                    BinOp::Shl => crate::eval::shift_value_left(a, b),
                    BinOp::Shr => crate::eval::shift_value_right(a, b),
                    BinOp::BitAnd => Some(a & b),
                    BinOp::BitOr => Some(a | b),
                    _ => Some(a ^ b),
                };
                match computed {
                    Some(v) if ty.holds(v) => Folded::Value(v),
                    _ => Folded::Fault(why(&format!(
                        "{a} {spelling} {b} leaves `{}`'s range",
                        ty.name
                    ))),
                }
            }
            _ => Folded::Unread,
        }
    }
}

/// E0706 at `span`: the operator `spelling` on `ty` faults at compile time.
fn fault(span: Span, spelling: &str, ty: Plain, what: &str) -> Diag {
    Diag::new(
        "E0706",
        span,
        "mem.static.3",
        format!(
            "this `{spelling}` on `{}` faults at compile time: {what}. A module item's \
             initialiser is evaluated at compile time ([mem.static.3]) by the rule the run \
             uses ([type.int.shift] (7)), and what would trap at run time is an error there; \
             intended wraparound is spelled `wrapping[T]`",
            ty.name
        ),
    )
}

/// is76 (wolf-interp#251): the first module initialiser whose fold faults —
/// E0706 at the operator, in the item's own file — in declaration order
/// within the first module that has one. An initialiser this fold does not
/// read is left to the run.
#[must_use]
pub fn fold_check(program: &Program) -> Option<Diag> {
    for module in modules(program) {
        let mut walk = FoldWalk {
            module,
            done: std::collections::BTreeMap::new(),
            open: Vec::new(),
        };
        for name in &module.bindings {
            if let Err(diag) = walk.item(name) {
                return Some(match module.item_files.get(name) {
                    Some(file) => diag.in_file(file.clone()),
                    None => diag,
                });
            }
        }
    }
    None
}

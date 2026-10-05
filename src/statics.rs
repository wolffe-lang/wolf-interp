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

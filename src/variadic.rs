//! [abi.c.variadic]: declaration facts, not a C argument-storage machine.
//! Diagnostics are measured through the compiler's observation protocol;
//! no compiler implementation or ABI lowering is shared.
use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::rowmatch::{Child, each_child_with_blocks};
use crate::sema::{Def, Module, Program};
use std::collections::BTreeMap;

pub(crate) fn builtin_name(callee: &Expr) -> Option<&str> {
    let base = match &*callee.kind {
        ExprKind::BracketApply { base, .. } => base,
        _ => callee,
    };
    let ExprKind::Path(path) = &*base.kind else {
        return None;
    };
    let [name] = path.segments.as_slice() else {
        return None;
    };
    matches!(
        name.name.as_str(),
        "va_start" | "va_arg" | "va_copy" | "va_end"
    )
    .then_some(name.name.as_str())
}

pub(crate) fn builtin_refusal(name: &str) -> String {
    format!("the {name} C variadic cursor builtin on the checked machine")
}

pub(crate) fn call_refusal(decl: &FnDecl) -> String {
    let direction = if decl.body.is_some() {
        "to a wolf definition"
    } else {
        "into C"
    };
    format!(
        "a C variadic call {direction} `{}` (the checked machine has no variadic membrane)",
        decl.name.name
    )
}

pub(crate) fn supported_target() -> bool {
    matches!(
        crate::attrs::build_target(),
        "x86_64-unknown-linux-gnu" | "x86_64-unknown-none"
    )
}

pub(crate) fn target_diag(span: Span) -> Diag {
    Diag::new(
        "E0826",
        span,
        "abi.c.variadic.targets",
        format!(
            "target `{}` does not support C's unnamed arguments",
            crate::attrs::build_target()
        ),
    )
}

fn type_name(ty: &Type) -> Option<String> {
    match &*ty.kind {
        TypeKind::Path { path, args } if args.is_empty() && path.is_single() => {
            Some(path.segments[0].name.clone())
        }
        TypeKind::RawPointer(_) => Some("*".into()),
        _ => None,
    }
}

fn list_type(ty: &Type) -> Option<Diag> {
    match &*ty.kind {
        TypeKind::Path { path, args } => {
            if path.is_single() && path.segments[0].name == "VaList" && !args.is_empty() {
                return Some(Diag::new(
                    "E0812",
                    ty.span,
                    "abi.c.variadic.list",
                    "`VaList` is one C-layout record and takes no type arguments",
                ));
            }
            args.iter().find_map(|arg| match arg {
                TypeArg::Type(ty) => list_type(ty),
                _ => None,
            })
        }
        TypeKind::RawPointer(ty)
        | TypeKind::ErrorUnion(ty)
        | TypeKind::Prefixed { ty, .. }
        | TypeKind::Fallible { ty, .. } => list_type(ty),
        TypeKind::Tuple(types) => types.iter().find_map(list_type),
        TypeKind::Fn { params, ret } => params
            .iter()
            .find_map(list_type)
            .or_else(|| ret.as_ref().and_then(|r| list_type(&r.ty))),
        _ => None,
    }
}

struct Walk<'a> {
    program: &'a Program,
    module: &'a Module,
    scopes: Vec<BTreeMap<String, Option<String>>>,
    frame: bool,
    started: bool,
    unsafe_depth: usize,
    diagnostic: Option<Diag>,
}

impl Walk<'_> {
    fn list_type(&self, ty: &Type) -> Option<Diag> {
        if self.module.items.contains_key("VaList") {
            None
        } else {
            list_type(ty)
        }
    }
    fn local(&self, name: &str) -> Option<&Option<String>> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }
    fn declare(&mut self, pattern: &Pattern, ty: Option<String>) {
        match &*pattern.kind {
            PatKind::Binding(name) | PatKind::At { name, .. } => {
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(name.name.clone(), ty);
            }
            PatKind::Tuple(parts) | PatKind::Or(parts) => {
                for part in parts {
                    self.declare(part, None);
                }
            }
            PatKind::Struct { fields, .. } => {
                for field in fields {
                    if let Some(pattern) = &field.pattern {
                        self.declare(pattern, None);
                    } else {
                        self.scopes
                            .last_mut()
                            .unwrap()
                            .insert(field.name.name.clone(), None);
                    }
                }
            }
            _ => {}
        }
    }
    fn function(&self, expr: &Expr) -> Option<&FnDecl> {
        let ExprKind::Path(path) = &*expr.kind else {
            return None;
        };
        let head = path.segments.first()?;
        if self.local(&head.name).is_some() {
            return None;
        }
        let def = match path.segments.as_slice() {
            [one] => self.program.lookup(&self.module.name, &one.name, false),
            [module, name] => self.program.lookup(&module.name, &name.name, true),
            _ => None,
        };
        match def {
            Some(Def::Fn(decl)) => Some(decl),
            _ => None,
        }
    }
    fn class(&self, expr: &Expr) -> Option<String> {
        match &*expr.kind {
            ExprKind::Path(path) if path.is_single() => {
                self.local(&path.segments[0].name).cloned().flatten()
            }
            ExprKind::Str(_) => Some("str".into()),
            ExprKind::Bool(_) => Some("bool".into()),
            ExprKind::Int(_) => Some("int".into()),
            ExprKind::Float(_) => Some("f64".into()),
            ExprKind::Char(_) => Some("char".into()),
            ExprKind::List(_) => Some("List".into()),
            ExprKind::Tuple(_) => Some("tuple".into()),
            ExprKind::Closure { .. } => Some("fn".into()),
            ExprKind::Cast { ty, .. } => type_name(ty),
            ExprKind::Group(inner) => self.class(inner),
            ExprKind::StructLit { path, .. } => path.segments.last().map(|i| i.name.clone()),
            _ => None,
        }
    }
    fn c_type(&self, name: &str) -> bool {
        matches!(
            name,
            "*" | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "int"
                | "uint"
                | "byte"
                | "bool"
                | "f32"
                | "f64"
                | "VaList"
        ) || crate::layout::layout_of_name(self.module, name).is_ok()
    }
    fn decl(&mut self, decl: &FnDecl) {
        for param in &decl.params {
            if let ParamKind::Named { ty, .. } = &param.kind {
                self.diagnostic = self.diagnostic.take().or_else(|| self.list_type(ty));
            }
        }
        if let Some(ret) = &decl.ret {
            self.diagnostic = self.diagnostic.take().or_else(|| self.list_type(&ret.ty));
        }
        let saved = (self.frame, self.started, self.unsafe_depth);
        self.frame = decl.variadic.is_some();
        self.started = false;
        self.unsafe_depth = 0;
        self.scopes.push(BTreeMap::new());
        for param in &decl.params {
            if let ParamKind::Named { name, ty } = &param.kind {
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(name.name.clone(), type_name(ty));
            }
        }
        if let Some(body) = &decl.body {
            self.block(body);
        }
        self.scopes.pop();
        (self.frame, self.started, self.unsafe_depth) = saved;
    }
    fn item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Fn(decl) => self.decl(decl),
            ItemKind::Binding(binding) => self.binding(binding),
            ItemKind::Impl(def) => {
                for member in &def.members {
                    self.item(member);
                }
            }
            ItemKind::Trait(def) => {
                for member in &def.members {
                    self.item(member);
                }
            }
            ItemKind::Struct(def) => {
                for field in &def.fields {
                    self.diagnostic = self.diagnostic.take().or_else(|| self.list_type(&field.ty));
                }
            }
            _ => {}
        }
    }
    fn binding(&mut self, binding: &Binding) {
        if let Some(ty) = &binding.ty {
            self.diagnostic = self.diagnostic.take().or_else(|| self.list_type(ty));
        }
        self.expr(&binding.value, false);
        self.declare(
            &binding.pattern,
            binding
                .ty
                .as_ref()
                .and_then(type_name)
                .or_else(|| self.class(&binding.value)),
        );
    }
    fn block(&mut self, block: &Block) {
        self.scopes.push(BTreeMap::new());
        for stmt in &block.stmts {
            match &stmt.kind {
                StmtKind::Binding(binding) => self.binding(binding),
                StmtKind::Item(item) => {
                    if let ItemKind::Fn(decl) = &item.kind {
                        self.scopes
                            .last_mut()
                            .unwrap()
                            .insert(decl.name.name.clone(), None);
                    }
                    self.item(item);
                }
                StmtKind::Assign { place, value, .. } => {
                    self.expr(place, false);
                    self.expr(value, false);
                }
                StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => self.expr(expr, false),
                StmtKind::AssumeNoalias(operands) => {
                    for expr in operands {
                        self.expr(expr, false);
                    }
                }
            }
        }
        if let Some(tail) = &block.tail {
            self.expr(tail, false);
        }
        self.scopes.pop();
    }
    fn call(&mut self, expr: &Expr, callee: &Expr, args: &[Arg]) -> Option<Diag> {
        if let Some(name) = builtin_name(callee)
            && self.local(name).is_none()
            && !self.module.items.contains_key(name)
        {
            if !supported_target() {
                return Some(target_diag(expr.span));
            }
            if name == "va_start" {
                if !self.frame {
                    return Some(Diag::new(
                        "E0824",
                        expr.span,
                        "abi.c.variadic.builtins",
                        "`va_start` needs the current function's C variadic frame",
                    ));
                }
                if self.started {
                    return Some(Diag::new(
                        "E0825",
                        expr.span,
                        "abi.c.variadic.builtins",
                        "`va_start` is already spelled in this function — use `va_copy` for another cursor",
                    ));
                }
                self.started = true;
            }
            if self.unsafe_depth == 0 {
                return Some(Diag::new(
                    "E1301",
                    expr.span,
                    "mem.unsafe.scope",
                    format!("the C call `{name}` needs an `unsafe` block"),
                ));
            }
            if name == "va_arg"
                && let ExprKind::BracketApply { args, .. } = &*callee.kind
            {
                let ty = args.first().and_then(|arg| match arg {
                    IndexArg::Type(ty) => type_name(ty),
                    IndexArg::Value(arg) => match &*arg.expr.kind {
                        ExprKind::Path(path) if path.is_single() => {
                            Some(path.segments[0].name.clone())
                        }
                        _ => None,
                    },
                });
                if ty.as_deref().is_some_and(|ty| {
                    matches!(ty, "i8" | "i16" | "u8" | "u16" | "byte" | "bool" | "f32")
                }) {
                    return Some(Diag::new(
                        "E0401",
                        expr.span,
                        "abi.c.variadic.arg",
                        "`va_arg` reads the promoted type — use `i32` for narrow integers or `f64` for `f32`",
                    ));
                }
            }
        } else if let Some(decl) = self.function(callee)
            && decl.variadic.is_some()
        {
            if args.len() < decl.params.len() {
                let n = decl.params.len();
                let passed = args.len();
                return Some(Diag::new(
                    "E0402",
                    expr.span,
                    "abi.c.variadic.call",
                    format!(
                        "`{}` takes {n} argument{}, but this call passes {passed}",
                        decl.name.name,
                        if n == 1 { "" } else { "s" }
                    ),
                ));
            }
            for arg in args.iter().skip(decl.params.len()) {
                if let Some(ty) = self.class(&arg.expr)
                    && !self.c_type(&ty)
                {
                    return Some(Diag::new(
                        "E0401",
                        arg.expr.span,
                        "abi.c.variadic.call",
                        "an unnamed C argument must have a C-compatible value type",
                    ));
                }
            }
        }
        None
    }
    fn expr(&mut self, expr: &Expr, callee: bool) {
        if self.diagnostic.is_some() {
            return;
        }
        match &*expr.kind {
            ExprKind::Call { callee, args } => {
                if let Some(diag) = self.call(expr, callee, args) {
                    self.diagnostic = Some(diag);
                    return;
                }
                self.expr(callee, true);
                for arg in args {
                    self.expr(&arg.expr, false);
                }
                return;
            }
            ExprKind::Path(_) if !callee => {
                if let Some(decl) = self.function(expr)
                    && decl.variadic.is_some()
                {
                    self.diagnostic = Some(Diag::new(
                        "E0823",
                        expr.span,
                        "abi.c.variadic.syntax",
                        format!(
                            "`{}` is variadic — call it by name, not through a function value",
                            decl.name.name
                        ),
                    ));
                }
            }
            ExprKind::Closure { params, body, .. } => {
                let saved = (self.frame, self.started, self.unsafe_depth);
                self.frame = false;
                self.started = false;
                self.unsafe_depth = 0;
                self.scopes.push(BTreeMap::new());
                for param in params {
                    self.scopes.last_mut().unwrap().insert(
                        param.name.name.clone(),
                        param.ty.as_ref().and_then(type_name),
                    );
                }
                self.expr(body, false);
                self.scopes.pop();
                (self.frame, self.started, self.unsafe_depth) = saved;
                return;
            }
            ExprKind::Unsafe { .. } => {
                self.unsafe_depth += 1;
                each_child_with_blocks(expr, &mut |child| self.child(child));
                self.unsafe_depth -= 1;
                return;
            }
            _ => {}
        }
        each_child_with_blocks(expr, &mut |child| self.child(child));
    }
    fn child(&mut self, child: Child<'_>) {
        match child {
            Child::Expr(expr) => self.expr(expr, false),
            Child::Block(block) => self.block(block),
        }
    }
}

pub(crate) fn check(program: &Program) -> Option<Diag> {
    for module in program.modules.values() {
        for unit in &module.units {
            let mut walk = Walk {
                program,
                module,
                scopes: vec![BTreeMap::new()],
                frame: false,
                started: false,
                unsafe_depth: 0,
                diagnostic: None,
            };
            for item in &unit.unit.items {
                walk.item(item);
            }
            if let Some(diag) = walk.diagnostic {
                return Some(diag.in_file(&unit.file));
            }
        }
    }
    None
}

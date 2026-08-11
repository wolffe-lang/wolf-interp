//! The warning analyses (s68, issue #19) and the pin-`13b811f` statics.
//!
//! wolfc's s68 first wave registered fifteen W-codes (spec/01 §9.2). Eleven
//! need nothing beyond syntax and name resolution — analyses this machine's
//! resolve rung (sema-lite) already performs — so this module implements
//! them, and warning parity (`[proto.cmp.warn]`) grows lint-by-lint. The
//! remaining four are **honest-absent** (`[proto.record.warn]`): W0402 needs
//! float typing, W0601 row typing, W0801 scrutinee case tables, W1001 region
//! inference — none of which sema-lite has, so this machine simply never
//! observes those codes rather than guessing. W0301 (partial format, s11)
//! and W1301 (`# Safety:` comment, s22) are grandfathered wolfc lints
//! outside the shared-analysis set.
//!
//! `#[allow(…)]` is part of the program (spec/01 §9.3) and suppresses
//! identically on both sides: the attribute's region is the whole declaration
//! it sits on, body included. The two attribute self-lints ride along —
//! W0302 (`#[allow]` of an unregistered code) and W0303 (`#[allow]` of
//! nothing) — because the suppression machinery *is* their analysis.
//!
//! The same walk carries the statics the 13b811f re-pin aligned (issue #19's
//! E0004 correction and the E11xx capture law):
//!
//! - **E0004** — `1.e5` is member access on an integer (`[gram.amb.intdot]`):
//!   `int` has no member `e5`, so the expression has no meaning at any rung
//!   (`[diag.sev.error]`); rejecting it here closes the last E000x
//!   `unsupported(interp)` conservatism row.
//! - **E1101** — a task closure writing to a name it captures from the
//!   enclosing function (`[conc.task.spawn]`); `when` bodies are exempt
//!   (`[conc.when.body]` — the sync objects mediate the write).
//! - **E1102** — a channel payload type that is visibly not sendable
//!   (`[conc.chan.type]`): a bare region-interior container (`List`, `Map`)
//!   can never cross; the walk names only what it can see and never guesses.
//! - **E1103** — a `when` lexically inside a `when` body
//!   (`[conc.when.nonest]`); the through-a-call case is the dynamic half
//!   (`trap(deadlock)`, `[conc.deadlock.self]`).
//!
//! Every span was observed through the spec/06 protocol at pin `13b811f`
//! (the E0410/E1007 precedent: the counterparty's codes and spans, this
//! machine's rung). Statement-shaped warnings (W0306, W1102, W1302) span the
//! statement *through its terminator* — `;` or the newline — which is the
//! counterparty's convention, observed on the fixtures.

use std::collections::BTreeSet;

use crate::ast::{
    Arg, AttrArg, AttrInput, Attribute, BinOp, Binding, BindingKind, Block, ElseHandler, Expr,
    ExprKind, FnDecl, IndexArg, Item, ItemKind, Member, ParamMode, PatKind, Pattern, RetType, Stmt,
    StmtKind, StrLit, StrPart, Type, TypeKind, UnOp,
};
use crate::diag::{Diag, Span};
use crate::lex::StrKind;
use crate::protocol::Warning;
use crate::sema::{Def, Program};

/// The warning analyses this implementation runs. The corpus `warns:` ledger
/// and the record's `warnings` array are enforced/populated for exactly these
/// codes (`[proto.record.warn]`'s honest-absent rule covers the rest).
pub const IMPLEMENTED: &[&str] = &[
    "W0302", "W0303", "W0304", "W0305", "W0306", "W0307", "W0308", "W0309", "W0401", "W0602",
    "W1101", "W1102", "W1302",
];

/// Compiler-only for now — codes this machine's rungs cannot observe, kept
/// here so the honest-absent posture is written down beside the implemented
/// set: W0402 (float typing), W0601 (row typing), W0801 (case tables),
/// W1001 (region inference), plus the grandfathered W0301/W1301.
pub const HONEST_ABSENT: &[&str] = &["W0301", "W0402", "W0601", "W0801", "W1001", "W1301"];

/// Codes `#[allow(…)]` recognizes — spec/01 §9.2's registered families as of
/// the 13b811f pin. An argument outside this set suppresses nothing and is
/// itself W0302.
const REGISTERED: &[&str] = &[
    "w0301", "w0302", "w0303", "w0304", "w0305", "w0306", "w0307", "w0308", "w0309", "w0401",
    "w0402", "w0601", "w0602", "w0801", "w1001", "w1101", "w1102", "w1301", "w1302",
    // Warning-severity diagnostics under E-numbers (spec/01 §9.2: "a
    // warning-severity diagnostic under an E-number"): E0802 unreachable arm.
    "e0802",
];

/// Prelude names a user declaration shadows (W0304): the ambient std stub,
/// minus the provisional corpus stand-ins the s68 triage exempts.
const STAND_INS: &[&str] = &["worker", "acquire", "release", "zip"];

/// What the combined walk produced for one program.
#[derive(Debug, Default)]
pub struct Analysis {
    /// The E-code rejections, in source order per file — the first is the
    /// verdict when the resolve rung otherwise passes.
    pub statics: Vec<Diag>,
    /// The warning observations after `#[allow]` suppression, sorted by
    /// `(span, code)` and deduplicated — `[proto.record.warn]`'s array.
    pub warnings: Vec<Warning>,
}

/// Runs every analysis over the loaded program.
#[must_use]
pub fn analyze(program: &Program) -> Analysis {
    let mut findings: Vec<(String, Span)> = Vec::new();
    let mut statics: Vec<Diag> = Vec::new();
    let mut allows: Vec<(String, Span)> = Vec::new();

    for module in program.modules.values() {
        let item_names: BTreeSet<String> = module
            .items
            .keys()
            .cloned()
            .chain(module.uses.iter().cloned())
            .collect();
        let fn_names: BTreeSet<String> = module
            .items
            .iter()
            .filter(|(_, (def, _))| matches!(def, Def::Fn(_)))
            .map(|(name, _)| name.clone())
            .collect();
        for unit in &module.units {
            let mut walk = Walk {
                source: &unit.source,
                from_std: unit.from_std,
                item_names: &item_names,
                fn_names: &fn_names,
                findings: &mut findings,
                statics: &mut statics,
                allows: &mut allows,
                scopes: Vec::new(),
                closures: Vec::new(),
                assigns: Vec::new(),
                closure_stack: Vec::new(),
                task_depth: 0,
                task_scope_base: Vec::new(),
                when_depth: 0,
                in_interp: false,
                assumed: Vec::new(),
            };
            walk.unit(&unit.unit);
        }
    }

    // `#[allow]` suppression: a warning is dropped when an allow of its code
    // covers its span (the attribute's region is the whole declaration).
    let suppressed = |code: &str, span: Span| {
        allows.iter().any(|(allowed, region)| {
            allowed.eq_ignore_ascii_case(code)
                && region.start <= span.start
                && span.end <= region.end
        })
    };
    let mut warnings: Vec<Warning> = findings
        .iter()
        .filter(|(code, span)| !suppressed(code, *span))
        .map(|(code, span)| Warning {
            code: code.clone(),
            span: [span.start as u64, span.end as u64],
        })
        .collect();
    warnings.sort_by(|a, b| (a.span, a.code.as_str()).cmp(&(b.span, b.code.as_str())));
    warnings.dedup();

    Analysis { statics, warnings }
}

/// One closure seen in the current function, for W1102.
struct ClosureRec {
    span: Span,
    captured: BTreeSet<String>,
}

/// One whole-name assignment in the current function, for W1102.
struct AssignRec {
    name: String,
    /// The statement through its terminator.
    stmt_span: Span,
    /// Inside a `when` body (exempt — the sync objects mediate).
    in_when: bool,
}

struct Walk<'a> {
    source: &'a str,
    from_std: bool,
    /// Module-level names (items + imports) — what a bare name resolves to
    /// when no local scope declares it.
    item_names: &'a BTreeSet<String>,
    /// Module-level `fn` names, for the W0305 collision check.
    fn_names: &'a BTreeSet<String>,
    findings: &'a mut Vec<(String, Span)>,
    statics: &'a mut Vec<Diag>,
    allows: &'a mut Vec<(String, Span)>,
    /// Locals of the enclosing function, innermost last: `(name, is_var)`.
    scopes: Vec<Vec<(String, bool)>>,
    /// Closures created so far in the current function (W1102).
    closures: Vec<ClosureRec>,
    /// Whole-name assignments in the current function (W1102).
    assigns: Vec<AssignRec>,
    /// Spans of the closures the walk is currently inside.
    closure_stack: Vec<Span>,
    /// > 0 while inside a closure passed to `.spawn(…)` (E1101/W1101).
    task_depth: usize,
    /// The scope-stack depth where the innermost task closure begins: a name
    /// resolved at or above this index is the task's own; below it, captured.
    task_scope_base: Vec<usize>,
    /// Lexical `when` nesting (E1103; the W1101/W1102 exemption).
    when_depth: usize,
    /// Inside a string interpolation hole (W0308).
    in_interp: bool,
    /// `assume noalias` operands still standing, one frame per block (W1302).
    assumed: Vec<Vec<String>>,
}

impl Walk<'_> {
    fn warn(&mut self, code: &str, span: Span) {
        if !self.from_std {
            self.findings.push((code.to_owned(), span));
        }
    }

    /// Extends a statement span through its terminator byte (`;` or the
    /// newline) — the counterparty's statement-lint convention, observed at
    /// the pin (`[746,752]` is `q = p\n` on `assume_reassigned.lu`).
    fn through_terminator(&self, span: Span) -> Span {
        match self.source.as_bytes().get(span.end) {
            Some(b';' | b'\n') => Span::new(span.start, span.end + 1),
            _ => span,
        }
    }

    fn declared(&self, name: &str) -> Option<bool> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|(n, _)| n == name)
            .map(|(_, is_var)| *is_var)
    }

    fn declare_pattern(&mut self, pattern: &Pattern, is_var: bool) {
        match &*pattern.kind {
            PatKind::Binding(ident) => {
                if let Some(scope) = self.scopes.last_mut() {
                    scope.push((ident.name.clone(), is_var));
                }
            }
            PatKind::Variant { fields, .. } => {
                for field in fields {
                    self.declare_pattern(field, is_var);
                }
            }
            PatKind::Tuple(fields) | PatKind::Or(fields) => {
                for field in fields {
                    self.declare_pattern(field, is_var);
                }
            }
            PatKind::At { name, pattern } => {
                if let Some(scope) = self.scopes.last_mut() {
                    scope.push((name.name.clone(), is_var));
                }
                self.declare_pattern(pattern, is_var);
            }
            PatKind::Wildcard | PatKind::Literal(_) => {}
        }
    }

    // -- attributes ---------------------------------------------------------

    /// Registers `#[allow(…)]` regions and fires the two self-lints.
    fn attributes(&mut self, attrs: &[Attribute], region: Span) {
        for attribute in attrs {
            for attr in &attribute.attrs {
                if !attr.path.is_single() || attr.path.segments[0].name != "allow" {
                    continue;
                }
                match &attr.input {
                    None => {
                        // W0303: `#[allow]` allows nothing — the dead
                        // attribute, reported rather than implying a
                        // suppression that never happens.
                        self.warn("W0303", attr.path.segments[0].span);
                    }
                    Some(AttrInput::Args(args)) => {
                        for arg in args {
                            let AttrArg::Nested(code_attr) = arg else {
                                continue;
                            };
                            if !code_attr.path.is_single() {
                                continue;
                            }
                            let code = &code_attr.path.segments[0].name;
                            if REGISTERED.iter().any(|r| r.eq_ignore_ascii_case(code)) {
                                self.allows.push((code.clone(), region));
                            } else {
                                // W0302: a typo'd suppression that silently
                                // did nothing would be worse than the
                                // warning it meant to hide.
                                self.warn("W0302", code_attr.path.segments[0].span);
                            }
                        }
                    }
                    Some(AttrInput::Literal(_)) => {}
                }
            }
        }
    }

    // -- items --------------------------------------------------------------

    fn unit(&mut self, unit: &crate::ast::Unit) {
        for item in &unit.items {
            self.item(item);
        }
    }

    fn item(&mut self, item: &Item) {
        self.attributes(&item.attrs, item.span);
        match &item.kind {
            ItemKind::Fn(decl) => {
                self.shadow_check(&decl.name.name, decl.name.span);
                self.signature(decl, item.visibility.is_some());
                self.fn_body(decl);
            }
            ItemKind::Binding(binding) => {
                if let PatKind::Binding(ident) = &*binding.pattern.kind {
                    self.shadow_check(&ident.name, ident.span);
                }
                self.expr(&binding.value);
            }
            ItemKind::Struct(def) => {
                if let Some(name) = &def.name {
                    self.shadow_check(&name.name, name.span);
                }
            }
            ItemKind::Enum(def) => {
                if let Some(name) = &def.name {
                    self.shadow_check(&name.name, name.span);
                }
            }
            ItemKind::Impl(def) => {
                for member in &def.members {
                    self.attributes(&member.attrs, member.span);
                    if let ItemKind::Fn(decl) = &member.kind {
                        self.signature(decl, member.visibility.is_some());
                        self.fn_body(decl);
                    }
                }
            }
            ItemKind::TypeAlias(alias) => {
                self.shadow_check(&alias.name.name, alias.name.span);
            }
            ItemKind::Trait(_) | ItemKind::Use(_) | ItemKind::ImportC(_) => {}
        }
    }

    /// W0304 — a declaration shadowing a prelude/built-in name. The
    /// provisional corpus stand-ins and the std tree are exempt (s68 triage).
    fn shadow_check(&mut self, name: &str, span: Span) {
        let prelude =
            crate::eval::builtin::AMBIENT_NAMES.contains(&name) && !STAND_INS.contains(&name);
        if prelude {
            self.warn("W0304", span);
        }
    }

    /// The signature lints: W0305 (row tag colliding with a name in scope)
    /// and W0602 (a `pub` signature spelling a ≥2-tag row inline).
    fn signature(&mut self, decl: &FnDecl, is_pub: bool) {
        let mut rows: Vec<(&crate::ast::ErrorRow, bool)> = Vec::new();
        if let Some(RetType { row: Some(row), .. }) = &decl.ret {
            rows.push((row, true));
        }
        for param in &decl.params {
            if let crate::ast::ParamKind::Named { ty, .. } = &param.kind {
                collect_type_rows(ty, &mut rows);
            }
        }
        if let Some(RetType { ty, .. }) = &decl.ret {
            // A row anywhere in the return type is return-position for
            // W0602: `-> int ! {A, B}` parses as `TypeKind::Fallible`, not
            // as the `RetType::row` field.
            let mut ret_rows = Vec::new();
            collect_type_rows(ty, &mut ret_rows);
            rows.extend(ret_rows.into_iter().map(|(row, _)| (row, true)));
        }
        for (row, in_ret) in rows {
            if is_pub && in_ret && row.entries.len() >= 2 {
                // W0602 — the s15 lean: allowed, linted. The span is the
                // whole inline row, braces included.
                self.warn("W0602", row.span);
            }
            for entry in &row.entries {
                if entry.path.is_single() {
                    let tag = &entry.path.segments[0].name;
                    let collides = self.fn_names.contains(tag)
                        || self.item_names.contains(tag)
                        || crate::eval::builtin::AMBIENT_NAMES.contains(&tag.as_str());
                    if collides {
                        // W0305 — the double-reading hazard (F-0036): raise
                        // position resolves the word as the tag, every other
                        // position as the item.
                        self.warn("W0305", entry.path.segments[0].span);
                    }
                }
            }
        }
    }

    fn fn_body(&mut self, decl: &FnDecl) {
        let Some(body) = &decl.body else { return };
        self.scopes.push(Vec::new());
        for param in &decl.params {
            if let crate::ast::ParamKind::Named { name, .. } = &param.kind {
                self.scopes
                    .last_mut()
                    .expect("pushed above")
                    .push((name.name.clone(), param.mode.is_some()));
            }
        }
        // Nested `fn` items re-enter here; the W1102 state is per-function.
        let outer_closures = std::mem::take(&mut self.closures);
        let outer_assigns = std::mem::take(&mut self.assigns);
        self.block(body);
        self.scopes.pop();

        // W1102 — a closure captured an enclosing `var` that is reassigned
        // after the closure's creation. The reassignment site is the
        // observation (`[511,517]` = `y = 1;` on `store_buffer.lu`); `when`
        // bodies are exempt on both ends (observed: `when_multi.lu` is
        // warning-clean at the pin).
        let mut fired: Vec<(String, Span)> = Vec::new();
        for closure in &self.closures {
            for assign in &self.assigns {
                if assign.in_when
                    || assign.stmt_span.start < closure.span.end
                    || !closure.captured.contains(&assign.name)
                {
                    continue;
                }
                fired.push(("W1102".to_owned(), assign.stmt_span));
            }
        }
        for (code, span) in fired {
            self.warn(&code, span);
        }
        self.closures = outer_closures;
        self.assigns = outer_assigns;
    }

    // -- statements and expressions -----------------------------------------

    fn block(&mut self, block: &Block) {
        self.scopes.push(Vec::new());
        self.assumed.push(Vec::new());
        for (index, stmt) in block.stmts.iter().enumerate() {
            let tail_position = block.tail.is_none() && index + 1 == block.stmts.len();
            self.stmt(stmt, tail_position);
        }
        if let Some(tail) = &block.tail {
            self.expr(tail);
        }
        self.assumed.pop();
        self.scopes.pop();
    }

    fn stmt(&mut self, stmt: &Stmt, tail_position: bool) {
        self.attributes(&stmt.attrs, stmt.span);
        match &stmt.kind {
            StmtKind::Binding(binding) => self.binding(binding),
            StmtKind::Assign {
                place,
                op: _,
                value,
            } => {
                self.assign(place, stmt.span);
                self.expr(place);
                self.expr(value);
            }
            StmtKind::Defer { expr, .. } => self.expr(expr),
            StmtKind::AssumeNoalias(operands) => {
                for operand in operands {
                    if let ExprKind::Path(path) = &*operand.kind
                        && path.is_single()
                    {
                        let name = path.segments[0].name.clone();
                        self.assumed
                            .last_mut()
                            .expect("block pushed a frame")
                            .push(name);
                    }
                    self.expr(operand);
                }
            }
            StmtKind::Expr(expr) => {
                // W0306 — a statement that is only a prefix operator applied
                // to a value: the broken-continuation shape. The block's tail
                // is the block's value, never inert, so it is exempt.
                if !tail_position
                    && let ExprKind::Unary {
                        op: UnOp::Neg | UnOp::Not,
                        ..
                    } = &*expr.kind
                {
                    let span = self.through_terminator(stmt.span);
                    self.warn("W0306", span);
                }
                self.expr(expr);
            }
            StmtKind::Item(item) => self.item(item),
        }
    }

    fn binding(&mut self, binding: &Binding) {
        self.expr(&binding.value);
        self.declare_pattern(&binding.pattern, binding.kind == BindingKind::Var);
    }

    /// One assignment statement: the E1101/W1101 capture check, the W1302
    /// assume check, and the W1102 record.
    fn assign(&mut self, place: &Expr, stmt_span: Span) {
        let ExprKind::Path(path) = &*place.kind else {
            return;
        };
        if !path.is_single() {
            return;
        }
        let name = &path.segments[0].name;

        // W1302 — a standing `assume noalias` operand reassigned, whole-name
        // only (writes through projections are exempt).
        if self
            .assumed
            .iter()
            .any(|frame| frame.iter().any(|n| n == name))
        {
            let span = self.through_terminator(stmt_span);
            self.warn("W1302", span);
        }

        // The W1102 record: whole-name writes, with their `when` context.
        self.assigns.push(AssignRec {
            name: name.clone(),
            stmt_span: self.through_terminator(stmt_span),
            in_when: self.when_depth > 0,
        });

        // E1101/W1101 — inside a task closure, a write to a name the closure
        // did not declare is a write to captured state. `when` bodies are
        // exempt (`[conc.when.body]` — the sync objects mediate). A name
        // that resolves nowhere in the enclosing function (a module item, a
        // prelude name) is not a capture; the walk never guesses about it.
        let capture = self.task_depth > 0 && self.when_depth == 0 && {
            let base = *self.task_scope_base.last().expect("task_depth > 0");
            let holds = |scopes: &[Vec<(String, bool)>]| {
                scopes
                    .iter()
                    .any(|scope| scope.iter().any(|(n, _)| n == name))
            };
            !holds(&self.scopes[base..]) && holds(&self.scopes[..base])
        };
        if capture {
            let span = path.segments[0].span;
            self.statics.push(Diag::new(
                "E1101",
                span,
                "conc.task.spawn",
                format!(
                    "this task writes to `{name}`, which it captures from the enclosing \
                     function: unsynchronized mutable capture across tasks (D14 — copy, share \
                     `imm`, or `move`; a `sync` type mediates shared writes)"
                ),
            ));
            self.warn("W1101", span);
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match &*expr.kind {
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Wildcard => {}
            ExprKind::Path(_) => {}
            ExprKind::Str(lit) => self.str_lit(lit),
            ExprKind::StructLit { fields, .. } => {
                for field in fields {
                    if let Some(value) = &field.value {
                        self.expr(value);
                    }
                }
            }
            ExprKind::Tuple(items) => {
                for item in items {
                    self.expr(item);
                }
            }
            ExprKind::Group(inner) | ExprKind::Try(inner) | ExprKind::FromEnd(inner) => {
                self.expr(inner);
            }
            ExprKind::Block(block) => self.block(block),
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Cast { expr: inner, ty } => {
                self.cast(expr, inner, ty);
                self.expr(inner);
            }
            ExprKind::Call { callee, args } => {
                self.call(expr, callee, args);
            }
            ExprKind::BracketApply { base, args } => {
                self.expr(base);
                for arg in args {
                    if let IndexArg::Value(arg) = arg {
                        self.arg(arg);
                    }
                }
            }
            ExprKind::Member { base, member } => {
                self.intdot(expr, base, member);
                self.expr(base);
            }
            ExprKind::ModedReceiver { place, .. } => self.expr(place),
            ExprKind::Range { start, end, .. } => {
                if let Some(start) = start {
                    self.expr(start);
                }
                if let Some(end) = end {
                    self.expr(end);
                }
            }
            ExprKind::ElseDefault {
                expr: inner,
                handler,
            } => {
                self.expr(inner);
                match &**handler {
                    ElseHandler::Block(block) => self.block(block),
                    ElseHandler::Expr(fallback) => {
                        self.else_comparison(fallback);
                        self.expr(fallback);
                    }
                    ElseHandler::Handler { pattern, body } => {
                        self.scopes.push(Vec::new());
                        self.declare_pattern(pattern, false);
                        self.expr(body);
                        self.scopes.pop();
                    }
                }
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
            ExprKind::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for arm in arms {
                    self.scopes.push(Vec::new());
                    self.declare_pattern(&arm.pattern, false);
                    if let Some(guard) = &arm.guard {
                        self.expr(guard);
                    }
                    self.expr(&arm.body);
                    self.scopes.pop();
                }
            }
            ExprKind::For {
                pattern,
                iter,
                body,
            } => {
                self.expr(iter);
                self.scopes.push(Vec::new());
                self.declare_pattern(pattern, false);
                self.block(body);
                self.scopes.pop();
            }
            ExprKind::While { cond, body } => {
                self.expr(cond);
                self.block(body);
            }
            ExprKind::Loop { body } => self.block(body),
            ExprKind::Return(value) | ExprKind::Break(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            ExprKind::Continue => {}
            ExprKind::Closure { params, body, .. } => {
                self.closure(expr, params, body, false);
            }
            ExprKind::RegionSugar { body, .. } | ExprKind::In { body, .. } => self.block(body),
            ExprKind::RegionValue { .. } => {}
            ExprKind::Freeze(inner) => self.expr(inner),
            ExprKind::Scope { body, .. } => self.block(body),
            ExprKind::SpawnProc { args, .. } => {
                for arg in args {
                    self.arg(arg);
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.kind {
                        crate::ast::SelectArmKind::Recv { pattern, channel } => {
                            self.expr(channel);
                            self.scopes.push(Vec::new());
                            self.declare_pattern(pattern, false);
                            self.expr(&arm.body);
                            self.scopes.pop();
                        }
                        crate::ast::SelectArmKind::Timeout(deadline) => {
                            self.expr(deadline);
                            self.expr(&arm.body);
                        }
                    }
                }
            }
            ExprKind::When { operands, body } => {
                for operand in operands {
                    self.expr(operand);
                }
                if self.when_depth > 0 {
                    // E1103 — `when` acquires its whole set at once; a `when`
                    // lexically inside a `when` body is incremental
                    // acquisition by another spelling.
                    self.statics.push(Diag::new(
                        "E1103",
                        expr.span,
                        "conc.when.nonest",
                        "`when` blocks do not nest: the whole sync set is acquired at once \
                         ([conc.when.nodeadlock]), and a nested `when` is incremental \
                         acquisition by another spelling. Merge the sets into one `when`"
                            .to_owned(),
                    ));
                }
                self.when_depth += 1;
                self.block(body);
                self.when_depth -= 1;
            }
            ExprKind::Unsafe { body } => self.block(body),
            ExprKind::UnsafeC { .. } => {}
            ExprKind::Asm { template, operands } => {
                self.str_lit(template);
                for operand in operands {
                    self.expr(&operand.value);
                }
            }
            ExprKind::Borrow { place, from } => {
                self.expr(place);
                self.expr(from);
            }
        }
    }

    fn arg(&mut self, arg: &Arg) {
        // W0308 — a `mut` argument inside a string interpolation: a write
        // buried where readers expect pure formatting. The span is the mode
        // through the argument (`[458,463]` = `mut a` on `mut_in_interp.lu`).
        if self.in_interp && arg.mode == Some(ParamMode::Mut) {
            self.warn("W0308", arg.span);
        }
        self.expr(&arg.expr);
    }

    fn call(&mut self, _call: &Expr, callee: &Expr, args: &[Arg]) {
        // E1102 — `channel[T](…)` with a visibly unsendable payload: a bare
        // region-interior container can never cross a channel.
        if let ExprKind::BracketApply { base, args: targs } = &*callee.kind
            && let ExprKind::Path(path) = &*base.kind
            && path.is_single()
            && path.segments[0].name == "channel"
            && let Some(IndexArg::Value(payload)) = targs.first()
            && let Some(head) = expr_type_head(&payload.expr)
            && matches!(head, "List" | "Map")
        {
            let rendered = &self.source[payload.expr.span.start..payload.expr.span.end];
            self.statics.push(Diag::new(
                "E1102",
                payload.expr.span,
                "conc.chan.type",
                format!(
                    "`{rendered}` cannot be sent through a channel: a payload must be `Copy`, \
                     `imm`, a moved region, or a `sync` type — a bare region-interior \
                     `{head}` is none of those. Send the region instead"
                ),
            ));
        }

        // A task closure: `s.spawn(fn() { … })` — the E1101/W1101 context.
        let spawn = matches!(&*callee.kind, ExprKind::Member { member: Member::Named(name), .. }
            if name.name == "spawn");
        self.expr(callee);
        for arg in args {
            if spawn && matches!(&*arg.expr.kind, ExprKind::Closure { .. }) {
                if let ExprKind::Closure { params, body, .. } = &*arg.expr.kind {
                    self.closure(&arg.expr, params, body, true);
                }
                continue;
            }
            self.arg(arg);
        }
    }

    fn closure(
        &mut self,
        expr: &Expr,
        params: &[crate::ast::ClosureParam],
        body: &Expr,
        task: bool,
    ) {
        // Record the closure's free names for W1102 before descending.
        let mut locals: BTreeSet<String> =
            params.iter().map(|param| param.name.name.clone()).collect();
        let mut captured = BTreeSet::new();
        free_names(body, &mut locals, &mut captured);
        captured.retain(|name| self.declared(name) == Some(true));
        self.closures.push(ClosureRec {
            span: expr.span,
            captured,
        });

        self.closure_stack.push(expr.span);
        if task {
            self.task_depth += 1;
            self.task_scope_base.push(self.scopes.len());
        }
        self.scopes.push(
            params
                .iter()
                .map(|param| (param.name.name.clone(), param.mode.is_some()))
                .collect(),
        );
        self.expr(body);
        self.scopes.pop();
        if task {
            self.task_depth -= 1;
            self.task_scope_base.pop();
        }
        self.closure_stack.pop();
    }

    fn else_comparison(&mut self, fallback: &Expr) {
        // W0307 — a comparison operator in `else`-fallback position:
        // `flag() else a == b` compares first because `else` binds loosest.
        // The span is the operator itself (`[503,505]` = `==`).
        if let ExprKind::Binary { op, lhs, rhs } = &*fallback.kind {
            let symbol = match op {
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Le => "<=",
                BinOp::Ge => ">=",
                BinOp::Cmp => "<=>",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                _ => return,
            };
            let between = &self.source[lhs.span.end..rhs.span.start];
            if let Some(at) = between.find(symbol) {
                let start = lhs.span.end + at;
                self.warn("W0307", Span::new(start, start + symbol.len()));
            }
        }
    }

    fn cast(&mut self, whole: &Expr, inner: &Expr, ty: &Type) {
        // W0401 — an integer literal outside the spelled cast target's range:
        // compile-time-known, can never be preserved. The span is the whole
        // cast (`[436,445]` = `300 as u8`).
        let ExprKind::Int(text) = &*inner.kind else {
            return;
        };
        let Some(value) = parse_int_literal(text) else {
            return;
        };
        let TypeKind::Path { path, args } = &*ty.kind else {
            return;
        };
        if !args.is_empty() || !path.is_single() {
            return;
        }
        let range: Option<(i128, i128)> = match path.segments[0].name.as_str() {
            "u8" => Some((0, u8::MAX as i128)),
            "u16" => Some((0, u16::MAX as i128)),
            "u32" => Some((0, u32::MAX as i128)),
            "u64" | "uint" => Some((0, u64::MAX as i128)),
            "i8" => Some((i8::MIN as i128, i8::MAX as i128)),
            "i16" => Some((i16::MIN as i128, i16::MAX as i128)),
            "i32" => Some((i32::MIN as i128, i32::MAX as i128)),
            "i64" | "int" => Some((i64::MIN as i128, i64::MAX as i128)),
            _ => None,
        };
        if let Some((min, max)) = range
            && (value < min || value > max)
        {
            self.warn("W0401", whole.span);
        }
    }

    fn intdot(&mut self, whole: &Expr, base: &Expr, member: &Member) {
        // E0004 — `1.e5` is member access on an integer, and `int` has no
        // member `e5` (`[gram.amb.intdot]`): floats need digits on both
        // sides of the dot. The span is the whole expression (`[291,295]`).
        if let (ExprKind::Int(_), Member::Named(name)) = (&*base.kind, member) {
            let mut chars = name.name.chars();
            let exponent_shaped = matches!(chars.next(), Some('e' | 'E'))
                && chars.clone().next().is_some()
                && chars.all(|c| c.is_ascii_digit());
            if exponent_shaped {
                self.statics.push(Diag::new(
                    "E0004",
                    whole.span,
                    "gram.amb.intdot",
                    format!(
                        "`int` has no member `{}`: floats need digits on both sides of the \
                         dot — write `{}.0{}`",
                        name.name,
                        render_int(base, self.source),
                        name.name,
                    ),
                ));
            }
        }
    }

    fn str_lit(&mut self, lit: &StrLit) {
        // W0309 — interpolation-shaped braces in a raw literal: one
        // keystroke from an interpolation, no diagnostic otherwise. The span
        // is the brace group (`[809,814]` = `{who}`).
        if matches!(lit.kind, StrKind::Raw { .. }) {
            let text = &self.source[lit.span.start..lit.span.end];
            for (start, len) in interp_shaped_braces(text) {
                self.warn(
                    "W0309",
                    Span::new(lit.span.start + start, lit.span.start + start + len),
                );
            }
            return;
        }
        for part in &lit.parts {
            let StrPart::Interp(interp) = part else {
                continue;
            };
            let was = self.in_interp;
            self.in_interp = true;
            self.expr(&interp.expr);
            if let Some(parts) = &interp.format {
                for fmt_part in parts {
                    if let crate::ast::FmtPart::Interp(inner) = fmt_part {
                        self.expr(inner);
                    }
                }
            }
            self.in_interp = was;
        }
    }
}

/// The `{ident}` groups inside a raw literal's text, as `(offset, len)` pairs
/// relative to the text's start.
fn interp_shaped_braces(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let mut j = i + 1;
            let mut ident = false;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                if j == i + 1 && bytes[j].is_ascii_digit() {
                    break;
                }
                ident = true;
                j += 1;
            }
            if ident && j < bytes.len() && bytes[j] == b'}' {
                out.push((i, j + 1 - i));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The head type name an expression in type position spells (`List` in
/// `channel[List[int]](1)`), when it visibly spells one.
fn expr_type_head(expr: &Expr) -> Option<&str> {
    match &*expr.kind {
        ExprKind::Path(path) if path.is_single() => Some(&path.segments[0].name),
        ExprKind::BracketApply { base, .. } => expr_type_head(base),
        _ => None,
    }
}

/// The free bare names of a closure body: referenced names minus the ones the
/// body itself declares. Approximate downward (module items and prelude names
/// are filtered by the caller against the enclosing scopes).
fn free_names(expr: &Expr, locals: &mut BTreeSet<String>, out: &mut BTreeSet<String>) {
    match &*expr.kind {
        ExprKind::Path(path) if path.is_single() => {
            let name = &path.segments[0].name;
            if !locals.contains(name) {
                out.insert(name.clone());
            }
        }
        ExprKind::Block(block) => {
            let mut inner = locals.clone();
            for stmt in &block.stmts {
                free_names_stmt(stmt, &mut inner, out);
            }
            if let Some(tail) = &block.tail {
                free_names(tail, &mut inner, out);
            }
        }
        ExprKind::Closure { params, body, .. } => {
            let mut inner = locals.clone();
            for param in params {
                inner.insert(param.name.name.clone());
            }
            free_names(body, &mut inner, out);
        }
        _ => {
            walk_child_exprs(expr, &mut |child| free_names(child, locals, out));
        }
    }
}

fn free_names_stmt(stmt: &Stmt, locals: &mut BTreeSet<String>, out: &mut BTreeSet<String>) {
    match &stmt.kind {
        StmtKind::Binding(binding) => {
            free_names(&binding.value, locals, out);
            collect_pattern_names(&binding.pattern, locals);
        }
        StmtKind::Assign { place, value, .. } => {
            free_names(place, locals, out);
            free_names(value, locals, out);
        }
        StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => free_names(expr, locals, out),
        StmtKind::AssumeNoalias(operands) => {
            for operand in operands {
                free_names(operand, locals, out);
            }
        }
        StmtKind::Item(_) => {}
    }
}

fn collect_pattern_names(pattern: &Pattern, into: &mut BTreeSet<String>) {
    match &*pattern.kind {
        PatKind::Binding(ident) => {
            into.insert(ident.name.clone());
        }
        PatKind::Variant { fields, .. } | PatKind::Tuple(fields) | PatKind::Or(fields) => {
            for field in fields {
                collect_pattern_names(field, into);
            }
        }
        PatKind::At { name, pattern } => {
            into.insert(name.name.clone());
            collect_pattern_names(pattern, into);
        }
        PatKind::Wildcard | PatKind::Literal(_) => {}
    }
}

/// Calls `visit` on every direct child expression of `expr` — the generic
/// arm of [`free_names`], covering the kinds with no binding structure.
fn walk_child_exprs(expr: &Expr, visit: &mut impl FnMut(&Expr)) {
    match &*expr.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Wildcard
        | ExprKind::Path(_)
        | ExprKind::Continue
        | ExprKind::RegionValue { .. }
        | ExprKind::UnsafeC { .. } => {}
        ExprKind::Str(lit) => {
            for part in &lit.parts {
                if let StrPart::Interp(interp) = part {
                    visit(&interp.expr);
                    if let Some(parts) = &interp.format {
                        for fmt_part in parts {
                            if let crate::ast::FmtPart::Interp(inner) = fmt_part {
                                visit(inner);
                            }
                        }
                    }
                }
            }
        }
        ExprKind::StructLit { fields, .. } => {
            for field in fields {
                if let Some(value) = &field.value {
                    visit(value);
                }
            }
        }
        ExprKind::Tuple(items) => {
            for item in items {
                visit(item);
            }
        }
        ExprKind::Group(inner)
        | ExprKind::Try(inner)
        | ExprKind::FromEnd(inner)
        | ExprKind::Freeze(inner) => visit(inner),
        ExprKind::Unary { operand, .. } => visit(operand),
        ExprKind::Binary { lhs, rhs, .. } => {
            visit(lhs);
            visit(rhs);
        }
        ExprKind::Cast { expr: inner, .. } => visit(inner),
        ExprKind::Call { callee, args } => {
            visit(callee);
            for arg in args {
                visit(&arg.expr);
            }
        }
        ExprKind::BracketApply { base, args } => {
            visit(base);
            for arg in args {
                if let IndexArg::Value(arg) = arg {
                    visit(&arg.expr);
                }
            }
        }
        ExprKind::Member { base, .. } => visit(base),
        ExprKind::ModedReceiver { place, .. } => visit(place),
        ExprKind::Range { start, end, .. } => {
            if let Some(start) = start {
                visit(start);
            }
            if let Some(end) = end {
                visit(end);
            }
        }
        ExprKind::ElseDefault {
            expr: inner,
            handler,
        } => {
            visit(inner);
            match &**handler {
                ElseHandler::Block(block) => visit_block(block, visit),
                ElseHandler::Expr(fallback) => visit(fallback),
                ElseHandler::Handler { body, .. } => visit(body),
            }
        }
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            visit(cond);
            visit_block(then, visit);
            if let Some(otherwise) = otherwise {
                visit(otherwise);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            visit(scrutinee);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    visit(guard);
                }
                visit(&arm.body);
            }
        }
        ExprKind::For { iter, body, .. } => {
            visit(iter);
            visit_block(body, visit);
        }
        ExprKind::While { cond, body } => {
            visit(cond);
            visit_block(body, visit);
        }
        ExprKind::Loop { body }
        | ExprKind::RegionSugar { body, .. }
        | ExprKind::In { body, .. }
        | ExprKind::Scope { body, .. }
        | ExprKind::Unsafe { body }
        | ExprKind::When { body, .. } => visit_block(body, visit),
        ExprKind::Return(value) | ExprKind::Break(value) => {
            if let Some(value) = value {
                visit(value);
            }
        }
        ExprKind::SpawnProc { args, .. } => {
            for arg in args {
                visit(&arg.expr);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.kind {
                    crate::ast::SelectArmKind::Recv { channel, .. } => visit(channel),
                    crate::ast::SelectArmKind::Timeout(deadline) => visit(deadline),
                }
                visit(&arm.body);
            }
        }
        ExprKind::Asm { operands, .. } => {
            for operand in operands {
                visit(&operand.value);
            }
        }
        ExprKind::Borrow { place, from } => {
            visit(place);
            visit(from);
        }
        ExprKind::Block(_) | ExprKind::Closure { .. } => {
            unreachable!("handled by free_names before dispatch")
        }
    }
}

fn visit_block(block: &Block, visit: &mut impl FnMut(&Expr)) {
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

/// Every inline `ErrorRow` a type spells, for the W0305/W0602 signature walk.
/// The `bool` marks return position — always `false` here; the return row
/// proper is collected separately by the caller.
fn collect_type_rows<'a>(ty: &'a Type, out: &mut Vec<(&'a crate::ast::ErrorRow, bool)>) {
    match &*ty.kind {
        TypeKind::Fallible { ty, row } => {
            out.push((row, false));
            collect_type_rows(ty, out);
        }
        TypeKind::ErrorUnion(inner)
        | TypeKind::Prefixed { ty: inner, .. }
        | TypeKind::RawPointer(inner) => collect_type_rows(inner, out),
        TypeKind::Tuple(items) => {
            for item in items {
                collect_type_rows(item, out);
            }
        }
        TypeKind::Fn { params, ret } => {
            for param in params {
                collect_type_rows(param, out);
            }
            if let Some(RetType { ty, row, .. }) = ret.as_ref() {
                if let Some(row) = row {
                    out.push((row, false));
                }
                collect_type_rows(ty, out);
            }
        }
        TypeKind::Path { .. } | TypeKind::Dyn(_) | TypeKind::TypeOfTypes | TypeKind::Region => {}
    }
}

/// Renders the integer literal an E0004 message quotes.
fn render_int<'a>(base: &Expr, source: &'a str) -> &'a str {
    source.get(base.span.start..base.span.end).unwrap_or("1")
}

/// Parses an integer literal's spelling: underscores, `0x`/`0o`/`0b` bases.
fn parse_int_literal(text: &str) -> Option<i128> {
    let clean: String = text.chars().filter(|c| *c != '_').collect();
    let (digits, radix) = if let Some(rest) = clean.strip_prefix("0x").or(clean.strip_prefix("0X"))
    {
        (rest, 16)
    } else if let Some(rest) = clean.strip_prefix("0o").or(clean.strip_prefix("0O")) {
        (rest, 8)
    } else if let Some(rest) = clean.strip_prefix("0b").or(clean.strip_prefix("0B")) {
        (rest, 2)
    } else {
        (clean.as_str(), 10)
    };
    i128::from_str_radix(digits, radix).ok()
}

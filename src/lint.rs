//! The warning analyses (s68, issue #19; s69 at 0.1.7) and the pin statics.
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
//! The s69 idiom wave (0.1.7, the `e94b879` re-pin) joins the same walk:
//! W0310–W0312 (naming that lies), W0313 (docless `pub`), W0314/W0315
//! (module shape and `pub(pkg)` hygiene, answered per module after the
//! unit walks), W0603/W0604 (row-tag case and rowless `get`), W1002/W1003
//! (mode hygiene over the per-function write evidence), and E0802's
//! literal-precise dead-arm analysis (#54: a duplicated literal arm, `_`
//! after bool's two literals, and the `pattern_shape` scalar degradation
//! — never a bare identifier, which a handler match resolves against the
//! operand row's tags first, #48). W0316 stays honest-absent: its witness
//! needs dotted nested-module loading this loader does not perform. Every
//! span was observed through the spec/06 protocol at pin `e94b879`.
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

use std::collections::{BTreeMap, BTreeSet};

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
    "W0302", "W0303", "W0304", "W0305", "W0306", "W0307", "W0308", "W0309", "W0310", "W0311",
    "W0312", "W0313", "W0314", "W0315", "W0316", "W0317", "W0401", "W0602", "W0603", "W0604",
    "W1002", "W1003", "W1101", "W1102", "W1302", "E0802",
];

/// Compiler-only for now — codes this machine's rungs cannot observe, kept
/// here so the honest-absent posture is written down beside the implemented
/// set: W0402 (float typing), W0601 (row typing), W0801 (case tables),
/// W1001 (region inference), plus the grandfathered W0301/W1301. W0316
/// (ancestor import, s69) sat here from 0.1.6 to is18: its only witness
/// shape needs the dotted `use outer.inner` nested-module loading, the
/// detection code stood ready, and is18's module-identity work (#39) is
/// what finally fed it — `lints/ancestor_import/` runs and warns now, so
/// W0316 moved to `IMPLEMENTED` at is19 and its ledger is enforced.
pub const HONEST_ABSENT: &[&str] = &["W0301", "W0402", "W0601", "W0801", "W1001", "W1301"];

/// Codes `#[allow(…)]` recognizes — spec/01 §9.2's registered families as of
/// the 13b811f pin. An argument outside this set suppresses nothing and is
/// itself W0302.
const REGISTERED: &[&str] = &[
    "w0301", "w0302", "w0303", "w0304", "w0305", "w0306", "w0307", "w0308", "w0309", "w0310",
    "w0311", "w0312", "w0313", "w0314", "w0315", "w0316", "w0401", "w0402", "w0601", "w0602",
    "w0603", "w0604", "w0801", "w1001", "w1002", "w1003", "w1101", "w1102", "w1301", "w1302",
    // Warning-severity diagnostics under E-numbers (spec/01 §9.2: "a
    // warning-severity diagnostic under an E-number"): E0802 unreachable arm.
    "e0802",
];

/// Prelude names a user declaration shadows (W0304): the ambient std stub,
/// minus the provisional corpus stand-ins the s68 triage exempts.
const STAND_INS: &[&str] = &["worker", "acquire", "release", "zip"];

/// Why an `index` marker's shape is wrong, if it is (`[gram.attr.index]`,
/// D61): the argument is exactly one, the integer literal `0` or `1` —
/// another number, no argument, several, or the `=` input form are all
/// refused, because 0 and 1 are the origins.
fn bad_index_marker(attr: &crate::ast::Attr) -> Option<String> {
    match &attr.input {
        Some(AttrInput::Args(args)) => {
            if let [AttrArg::Literal(lit)] = args.as_slice()
                && let ExprKind::Int(text) = &*lit.kind
                && matches!(text.as_str(), "0" | "1")
            {
                return None;
            }
            Some(
                "`index` takes exactly one argument, the integer literal `0` or `1` — \
                 0 and 1 are the origins (D61); the faulty marker takes no effect"
                    .to_owned(),
            )
        }
        Some(AttrInput::Literal(_)) => Some(
            "`index = …` is not the marker's shape; spell it `index(0)` or `index(1)` \
             (`[gram.attr.index]`)"
                .to_owned(),
        ),
        None => Some(
            "`index` takes exactly one argument, the integer literal `0` or `1` — \
             a bare `index` names no origin (D61)"
                .to_owned(),
        ),
    }
}

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
        // The module's fn signatures by name, for the checks that read a
        // callee's declaration at a call site (W0305's fire-at-use).
        let fn_decls: BTreeMap<&str, &FnDecl> = module
            .items
            .iter()
            .filter_map(|(name, (def, _))| match def {
                Def::Fn(decl) => Some((name.as_str(), &**decl)),
                _ => None,
            })
            .collect();
        for unit in &module.units {
            let mut walk = Walk {
                source: &unit.source,
                from_std: unit.from_std,
                item_names: &item_names,
                fn_names: &fn_names,
                fn_decls: &fn_decls,
                row_tags: &module.row_tags,
                findings: &mut findings,
                statics: &mut statics,
                allows: &mut allows,
                scopes: Vec::new(),
                ret_row: Vec::new(),
                closures: Vec::new(),
                assigns: Vec::new(),
                closure_stack: Vec::new(),
                task_depth: 0,
                task_scope_base: Vec::new(),
                when_depth: 0,
                in_interp: false,
                assumed: Vec::new(),
                writes: Vec::new(),
                literal_lets: Vec::new(),
                origin: 0,
                list_locals: Vec::new(),
            };
            walk.unit(&unit.unit);
        }
    }

    // The module-shape lints (s69) — answered per module, after the walks:
    //
    // W0314 — a module holding exactly one item: a directory of ceremony
    // around a single function. Root is exempt (it has no ceremony to
    // shed); the std tree is exempt like every lint. Span: the item's name
    // in the member file (`[114,118]` = `only`, observed at the pin).
    //
    // W0315 — a `pub(pkg)` item no other module in the package references.
    //
    // W0316 — a child module importing its own ancestor: the
    // cyclic-adjacent shape one edit from the E0303 hard error. Span: the
    // `use` target's ident (`[92,97]` = `outer`).
    let entry_root = std::path::Path::new(&program.entry).parent();
    for (key, module) in &program.modules {
        let std_files: BTreeSet<&str> = module
            .units
            .iter()
            .filter(|unit| unit.from_std)
            .map(|unit| unit.file.as_str())
            .collect();
        if !key.is_empty() && module.units.iter().any(|unit| !unit.from_std) {
            let name_span = |item: &Item| -> Option<Span> {
                match &item.kind {
                    ItemKind::Fn(decl) => Some(decl.name.span),
                    ItemKind::Struct(def) => def.name.as_ref().map(|name| name.span),
                    ItemKind::Enum(def) => def.name.as_ref().map(|name| name.span),
                    ItemKind::Trait(def) => Some(def.name.span),
                    ItemKind::TypeAlias(alias) => Some(alias.name.span),
                    ItemKind::Binding(binding) => match &*binding.pattern.kind {
                        PatKind::Binding(ident) => Some(ident.span),
                        _ => None,
                    },
                    ItemKind::Impl(_) | ItemKind::Use(_) | ItemKind::ImportC(_) => None,
                }
            };
            // Count what the module DECLARES (uses and C imports are
            // wiring; an impl rides its type): exactly one is ceremony.
            let declared: Vec<&Item> = module
                .units
                .iter()
                .filter(|unit| !unit.from_std)
                .flat_map(|unit| &unit.unit.items)
                .filter(|item| {
                    !matches!(
                        item.kind,
                        ItemKind::Use(_) | ItemKind::ImportC(_) | ItemKind::Impl(_)
                    )
                })
                .collect();
            if let [only] = declared[..]
                && let Some(span) = name_span(only)
            {
                findings.push(("W0314".to_owned(), span));
            }
        }
        for unit in &module.units {
            if unit.from_std {
                continue;
            }
            for item in &unit.unit.items {
                let Some(vis) = &item.visibility else {
                    continue;
                };
                if !vis.package_only {
                    continue;
                }
                let name = match &item.kind {
                    ItemKind::Fn(decl) => Some(&decl.name),
                    ItemKind::TypeAlias(alias) => Some(&alias.name),
                    ItemKind::Struct(def) => def.name.as_ref(),
                    ItemKind::Enum(def) => def.name.as_ref(),
                    ItemKind::Trait(def) => Some(&def.name),
                    _ => None,
                };
                let Some(name) = name else { continue };
                let used = program.modules.iter().any(|(other_key, other)| {
                    other_key != key
                        && other.scopes.iter().any(|scope| {
                            scope.refs.iter().any(|r| {
                                r.head == *key
                                    && r.tail.as_ref().is_some_and(|(t, _)| t == &name.name)
                            })
                        })
                });
                if !used {
                    findings.push(("W0315".to_owned(), name.span));
                }
            }
        }
        for scope in &module.scopes {
            if std_files.contains(scope.file.as_str()) {
                continue;
            }
            let mut dir = std::path::Path::new(&scope.file).parent();
            for use_ref in &scope.uses {
                let mut ancestor = dir.and_then(std::path::Path::parent);
                while let Some(candidate) = ancestor {
                    // The walk climbs MODULE directories, and modules stop at
                    // the entry's own directory. It used to stop only when a
                    // candidate WAS the entry root, which never happens for a
                    // scope file sitting directly in it — the first candidate
                    // is already the entry root's parent, and the walk then
                    // climbed the whole filesystem path looking for a
                    // directory named like the `use` target.
                    //
                    // That is not hypothetical: `conc/proc_cross_module/main.lu`
                    // says `use work`, and GitHub's runner checks this
                    // repository out under `/home/runner/WORK/wolf-interp/…`,
                    // so the corpus file warned W0316 on CI and nowhere else.
                    // A lint whose answer depends on where the checkout lives
                    // is not an observation of the program.
                    let inside = entry_root
                        .is_some_and(|root| candidate != root && candidate.starts_with(root));
                    if !inside || candidate.as_os_str().is_empty() {
                        break;
                    }
                    if candidate
                        .file_name()
                        .is_some_and(|base| base.to_string_lossy() == use_ref.name)
                    {
                        findings.push(("W0316".to_owned(), use_ref.name_span));
                        break;
                    }
                    ancestor = candidate.parent();
                }
            }
            let _ = &mut dir;
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

/// One declared local of the enclosing function.
struct Local {
    name: String,
    /// Declared `var` (reassignable) — W1102's question.
    is_var: bool,
    /// `Some(row)` when this local is an `else`-handler's error binder: a
    /// tag-shaped value (`[gram.expr.tagident]`'s handler side, wolf-lang#48).
    /// The vec holds the operand's declared row when the callee's signature
    /// is in sight, and is empty when it is not — tag-shaped either way; a
    /// match over the binder resolves bare lowercase arms against this row
    /// and the module's row vocabulary before they may bind, exactly as
    /// `eval::match_pattern` does dynamically.
    err_row: Option<Vec<String>>,
}

impl Local {
    fn plain(name: String, is_var: bool) -> Self {
        Self {
            name,
            is_var,
            err_row: None,
        }
    }
}

struct Walk<'a> {
    source: &'a str,
    from_std: bool,
    /// Module-level names (items + imports) — what a bare name resolves to
    /// when no local scope declares it.
    item_names: &'a BTreeSet<String>,
    /// Module-level `fn` names, for the W0305 collision check.
    fn_names: &'a BTreeSet<String>,
    /// Module-level `fn` signatures by name — a callee's declaration read at
    /// its call site (W0305's fire-at-use over the declared parameter rows).
    fn_decls: &'a BTreeMap<&'a str, &'a FnDecl>,
    findings: &'a mut Vec<(String, Span)>,
    statics: &'a mut Vec<Diag>,
    allows: &'a mut Vec<(String, Span)>,
    /// Every lowercase tag any signature of the module declares in an error
    /// row — the pattern-resolution vocabulary (`sema::Module::row_tags`),
    /// the same fallback the evaluator asks when a scrutinee's own row is
    /// out of sight.
    row_tags: &'a BTreeSet<String>,
    /// Locals of the enclosing function, innermost last.
    scopes: Vec<Vec<Local>>,
    /// The enclosing function's declared return-row tags — the expected row
    /// of `return` position for W0305's fire-at-use.
    ret_row: Vec<String>,
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
    /// Head names the current function writes — assignment targets (through
    /// any projection), call-site `mut`/`take` arguments, moded receivers.
    /// W1002/W1003's evidence; per-function like `assigns`.
    writes: Vec<String>,
    /// `let` names bound directly to a scalar literal in the current
    /// function — the one scrutinee shape whose type sema-lite knows for
    /// certain (E0802's shape-degradation evidence).
    literal_lets: Vec<String>,
    /// The subscript origin in lexical force (`[gram.attr.index]`, D61):
    /// the file-wide `#![index(n)]` and the node markers, innermost wins —
    /// the same replay the parser performs to stamp `BracketApply`, here
    /// for W0317 (`.get` is origin-free; a literal fed to it inside a
    /// 1-origin scope warns at the literal).
    origin: u8,
    /// Locals visibly bound to a `List[…]()` constructor in the current
    /// function — the one receiver shape whose `.get` this walk KNOWS is
    /// the ordinal accessor (W0317 fires only there; a user type's `get`
    /// is its own business, and the compiler does not warn on it either —
    /// observed at pin addcd7f).
    list_locals: Vec<String>,
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
        self.local(name).map(|local| local.is_var)
    }

    /// The innermost local of this name, if any — the resolution the
    /// tightest-scope-wins rule gives a bare identifier.
    fn local(&self, name: &str) -> Option<&Local> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|local| local.name == name)
    }

    fn declare_pattern(&mut self, pattern: &Pattern, is_var: bool) {
        match &*pattern.kind {
            PatKind::Binding(ident) => {
                if let Some(scope) = self.scopes.last_mut() {
                    scope.push(Local::plain(ident.name.clone(), is_var));
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
            PatKind::Struct { fields, .. } => {
                // `[gram.pat.struct]`: shorthand declares the field's own
                // name; an explicit sub-pattern declares what it binds.
                for field in fields {
                    match &field.pattern {
                        Some(sub) => self.declare_pattern(sub, is_var),
                        None => {
                            if let Some(scope) = self.scopes.last_mut() {
                                scope.push(Local::plain(field.name.name.clone(), is_var));
                            }
                        }
                    }
                }
            }
            PatKind::At { name, pattern } => {
                if let Some(scope) = self.scopes.last_mut() {
                    scope.push(Local::plain(name.name.clone(), is_var));
                }
                self.declare_pattern(pattern, is_var);
            }
            PatKind::Wildcard | PatKind::Literal(_) => {}
        }
    }

    /// Declares an `else`-handler's binder. The binder always BINDS — a
    /// handler pattern is irrefutable (E0806's law), so even a binder named
    /// like a declared tag is a real local (and a real shadow, W0305-wise).
    /// What travels with it is its tag-shapedness: the operand's declared
    /// row, for the arm-resolution rule when a `match` scrutinizes it.
    fn declare_err_binder(&mut self, pattern: &Pattern, row: Vec<String>) {
        if let PatKind::Binding(ident) = &*pattern.kind {
            if let Some(scope) = self.scopes.last_mut() {
                scope.push(Local {
                    name: ident.name.clone(),
                    is_var: false,
                    err_row: Some(row),
                });
            }
        } else {
            self.declare_pattern(pattern, false);
        }
    }

    /// The declared error row of an `else` operand, when a signature is in
    /// sight: a call to a module-level `fn` reads the callee's declared
    /// return row; a call to an ambient BUILTIN reads the pinned prelude row
    /// (`eval::builtin::declared_row` — the same vocabulary the evaluator
    /// rides since wolf-interp#47, kept in mirror so the static arm rule and
    /// the dynamic one answer alike). Anything else is tag-shaped with an
    /// unknown row (empty — the module vocabulary still applies).
    fn operand_row(&self, operand: &Expr) -> Vec<String> {
        if let ExprKind::Call { callee, .. } = &*operand.kind
            && let ExprKind::Path(path) = &*callee.kind
            && path.is_single()
            && self.declared(&path.segments[0].name).is_none()
        {
            if let Some(decl) = self.fn_decls.get(path.segments[0].name.as_str()) {
                return crate::sema::declared_raise_tags(decl);
            }
            let row = crate::eval::builtin::declared_row(&path.segments[0].name);
            if !row.is_empty() {
                return row.iter().map(|tag| (*tag).to_owned()).collect();
            }
        }
        Vec::new()
    }

    /// Whether a match arm's pattern is a ROW-TAG PATTERN rather than a
    /// binding — `[gram.expr.tagident]`'s handler side (wolf-lang#48,
    /// wolf-interp#44), the static mirror of `eval::match_pattern`: over a
    /// tag-shaped scrutinee (`scrutinee_row` is `Some`), a bare lowercase
    /// identifier that names a declared row tag — the scrutinee's own row
    /// first, the module's row vocabulary behind it — dispatches on the tag
    /// and binds nothing. No binding, no shadow: W0305 has no subject in
    /// such an arm, which is exactly where it used to false-fire.
    fn arm_is_tag_pattern(&self, pattern: &Pattern, scrutinee_row: Option<&[String]>) -> bool {
        let Some(row) = scrutinee_row else {
            return false;
        };
        if let PatKind::Binding(ident) = &*pattern.kind {
            ident.name.starts_with(char::is_lowercase)
                && (row.contains(&ident.name) || self.row_tags.contains(&ident.name))
        } else {
            false
        }
    }

    // -- attributes ---------------------------------------------------------

    /// Registers `#[allow(…)]` regions and fires the two self-lints; the
    /// origin marker's E0813 validation rides the same walk (every item,
    /// impl-member and statement attribute position passes through here).
    fn attributes(&mut self, attrs: &[Attribute], region: Span) {
        self.index_markers(attrs);
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
        self.inner_attributes(&unit.inner_attrs);
        // The file-wide origin (`#![index(n)]`, D61) — exactly the parser's
        // reading: only an exactly well-formed marker decides anything.
        if let Some(origin) = crate::parse::index_origin_of(&unit.inner_attrs) {
            self.origin = origin;
        }
        for item in &unit.items {
            self.item(item);
        }
    }

    /// The file-wide `#![…]` group (`[gram.attr.index]`, D61): strict from
    /// birth. At v1 `index` is the only file-wide attribute — an inner
    /// attribute naming anything else is E0813, never ignored (a file-wide
    /// marker an implementation silently skipped would silently change how
    /// every subscript in the file reads) — and the `index` marker's own
    /// argument shape is validated exactly as the statement form's is.
    fn inner_attributes(&mut self, attrs: &[Attribute]) {
        for attribute in attrs {
            for attr in &attribute.attrs {
                if !(attr.path.is_single() && attr.path.segments[0].name == "index") {
                    let name = attr
                        .path
                        .segments
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    self.statics.push(Diag::new(
                        crate::diag::E_BAD_INNER_ATTR,
                        attr.span,
                        "gram.attr.index",
                        format!(
                            "`{name}` is not a file-wide attribute this implementation knows — \
                             the inner form is strict from birth (at v1 `index` is the only \
                             `#![…]` attribute), and an unknown one is refused by name, never \
                             ignored"
                        ),
                    ));
                }
            }
        }
        self.index_markers(attrs);
    }

    /// The origin marker's shape (`[gram.attr.index]`, D61): exactly one
    /// argument, the integer literal `0` or `1`, at most one `index` item
    /// per node. E0813 at the offending attr; the faulty marker took no
    /// effect (the parser stamps origins only from exactly well-formed
    /// markers), so one mistake is one diagnostic, never a scope of shifted
    /// subscripts.
    fn index_markers(&mut self, attrs: &[Attribute]) {
        let mut seen = false;
        for attribute in attrs {
            for attr in &attribute.attrs {
                if !(attr.path.is_single() && attr.path.segments[0].name == "index") {
                    continue;
                }
                if seen {
                    self.statics.push(Diag::new(
                        crate::diag::E_BAD_INNER_ATTR,
                        attr.span,
                        "gram.attr.index",
                        "`index` appears twice on one node — one origin marker per node; \
                         nesting is the spelling for narrower scopes, and the innermost wins",
                    ));
                    continue;
                }
                seen = true;
                if let Some(message) = bad_index_marker(attr) {
                    self.statics.push(Diag::new(
                        crate::diag::E_BAD_INNER_ATTR,
                        attr.span,
                        "gram.attr.index",
                        message,
                    ));
                }
            }
        }
    }

    fn item(&mut self, item: &Item) {
        self.attributes(&item.attrs, item.span);
        // `#[index(n)]` scopes the annotated item's full extent, innermost
        // wins; the outer origin is restored when the item's walk ends.
        let outer_origin = self.origin;
        if let Some(origin) = crate::parse::index_origin_of(&item.attrs) {
            self.origin = origin;
        }
        self.item_inner(item);
        self.origin = outer_origin;
    }

    fn item_inner(&mut self, item: &Item) {
        // W0313 — a plain-`pub` item with no `///` (s69): the export is a
        // promise, the doc comment is where it is written down. `pub(pkg)`
        // is package-internal and exempt (observed: `pkg_item_unused`'s
        // undocumented `spare` draws W0315 only). Span: the `pub` keyword
        // (`[298,301]` on `pub_undocumented.lu`).
        if let Some(vis) = &item.visibility
            && !vis.package_only
            && matches!(
                item.kind,
                ItemKind::Fn(_)
                    | ItemKind::Binding(_)
                    | ItemKind::Struct(_)
                    | ItemKind::Enum(_)
                    | ItemKind::Trait(_)
                    | ItemKind::TypeAlias(_)
            )
            && !self.documented(item.span.start)
        {
            self.warn("W0313", vis.span);
        }
        match &item.kind {
            ItemKind::Fn(decl) => {
                self.shadow_check(&decl.name.name, decl.name.span);
                self.signature(decl, item.visibility.is_some());
                self.idiom_signature(decl);
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
                    let outer_origin = self.origin;
                    if let Some(origin) = crate::parse::index_origin_of(&member.attrs) {
                        self.origin = origin;
                    }
                    if let ItemKind::Fn(decl) = &member.kind {
                        self.signature(decl, member.visibility.is_some());
                        self.idiom_signature(decl);
                        self.fn_body(decl);
                    }
                    self.origin = outer_origin;
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

    /// W0305's fire-at-use (D52, `[gram.expr.tagident]`): a bare lowercase
    /// identifier in a *checked position* whose expected declared row spells
    /// the name resolves LOCAL-first — the tightest scopes shadow, as
    /// everywhere — and the shadow warns where it happens, at the use. The
    /// checked positions are the `return` operand, a call argument against
    /// the callee's declared parameter row, and an annotated `let`/`var`
    /// initializer against the annotation's row.
    fn tag_shadow_use(&mut self, expr: &Expr, row: &[String]) {
        if let ExprKind::Path(path) = &*expr.kind
            && path.is_single()
            && path.segments[0].name.starts_with(char::is_lowercase)
            && row.contains(&path.segments[0].name)
            && self.declared(&path.segments[0].name).is_some()
        {
            self.warn("W0305", path.segments[0].span);
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

    /// Is the source line directly above `item_start` a `///` doc comment?
    /// Attribute lines between the doc and the item are transparent; a blank
    /// line breaks adjacency (the doc no longer documents the item).
    fn documented(&self, item_start: usize) -> bool {
        let before = &self.source[..item_start.min(self.source.len())];
        let mut lines = before.lines().rev();
        // When the item does not start its own line, the final (unterminated)
        // line is the fragment before it — text there means no doc above.
        if !before.is_empty()
            && !before.ends_with('\n')
            && let Some(fragment) = lines.next()
            && !fragment.trim().is_empty()
        {
            return false;
        }
        for line in lines {
            let t = line.trim();
            if t.starts_with("///") {
                return true;
            }
            if t.starts_with("#[") || t.starts_with("#!") {
                continue;
            }
            return false;
        }
        false
    }

    /// The s69 idiom lints a signature answers by itself: W0310 (`get_`
    /// prefix), W0311 (predicate names answer bool), W0312 (`as_` must
    /// borrow), W0604 (bare `get` promises a row), W0603 (tag case vs
    /// payload, return-position rows). Spans observed through the spec/06
    /// protocol at pin `e94b879`.
    fn idiom_signature(&mut self, decl: &FnDecl) {
        let name = decl.name.name.as_str();
        // W0310 — `get_` names nothing; the noun is the whole name.
        if name.starts_with("get_") {
            self.warn("W0310", decl.name.span);
        }
        // The return rows (RetType::row plus rows spelled inside the type).
        let mut rows: Vec<(&crate::ast::ErrorRow, bool)> = Vec::new();
        if let Some(RetType { row: Some(row), .. }) = &decl.ret {
            rows.push((row, true));
        }
        if let Some(RetType { ty, .. }) = &decl.ret {
            collect_type_rows(ty, &mut rows);
        }
        // W0604 — bare `get` is the checked-access spelling: it answers
        // `T ! {none}`; rowless `get` promises a lookup, delivers a total fn.
        if name == "get" && rows.is_empty() {
            self.warn("W0604", decl.name.span);
        }
        // W0311 — an `is_`/`has_` name promises a yes-or-no answer.
        if name.starts_with("is_") || name.starts_with("has_") {
            let answers_bool = decl.ret.as_ref().is_some_and(|ret| {
                fn base(ty: &Type) -> &Type {
                    match &*ty.kind {
                        TypeKind::Fallible { ty, .. } | TypeKind::ErrorUnion(ty) => base(ty),
                        _ => ty,
                    }
                }
                matches!(
                    &*base(&ret.ty).kind,
                    TypeKind::Path { path, .. }
                        if path.segments.last().is_some_and(|s| s.name == "bool")
                )
            });
            if !answers_bool {
                self.warn("W0311", decl.name.span);
            }
        }
        // W0312 — an `as_` conversion must borrow; a consuming one charges a
        // cost the name denies. Span: the whole offending parameter
        // (`[294,305]` = `take s: str`).
        if name.starts_with("as_") {
            for param in &decl.params {
                if param.mode == Some(ParamMode::Take) {
                    self.warn("W0312", param.span);
                }
            }
        }
        // W0603 — a tag's case contradicts its payload: payload-free marks
        // are lowercase, payload carriers CapCase. Return-position rows. A
        // row VARIABLE — an entry naming one of the fn's generic params,
        // `-> T ! {E}` — is not a tag and carries no case convention
        // (`rows/hof_tail.lu` is warning-clean at the pin).
        for (row, _) in rows {
            for entry in &row.entries {
                if !entry.path.is_single() {
                    continue;
                }
                let seg = &entry.path.segments[0];
                if decl.generics.iter().any(|g| g.name.name == seg.name) {
                    continue;
                }
                let cap = seg.name.chars().next().is_some_and(char::is_uppercase);
                if (entry.payload.is_empty() && cap) || (!entry.payload.is_empty() && !cap) {
                    self.warn("W0603", seg.span);
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
                    .push(Local::plain(name.name.clone(), param.mode.is_some()));
            }
        }
        // Nested `fn` items re-enter here; the W1102 state is per-function.
        let outer_closures = std::mem::take(&mut self.closures);
        let outer_assigns = std::mem::take(&mut self.assigns);
        let outer_writes = std::mem::take(&mut self.writes);
        let outer_literal_lets = std::mem::take(&mut self.literal_lets);
        let outer_list_locals = std::mem::take(&mut self.list_locals);
        let outer_ret_row =
            std::mem::replace(&mut self.ret_row, crate::sema::declared_raise_tags(decl));
        self.block(body);
        self.ret_row = outer_ret_row;
        self.scopes.pop();

        // W1002 — a `mut` parameter the body never writes: every call site
        // surrenders exclusive access for nothing. W1003 — a `take`
        // parameter returned unchanged: a round trip that consumes nothing.
        // Spans: the mode keyword at the declaration (`[393,396]` = `mut`,
        // `[307,311]` = `take`, observed at pin `e94b879`).
        for param in &decl.params {
            let name = match &param.kind {
                crate::ast::ParamKind::Named { name, .. } => name.name.as_str(),
                crate::ast::ParamKind::SelfParam { .. } => "self",
            };
            let written = self.writes.iter().any(|w| w == name);
            let keyword = |len: usize, word: &str| {
                let start = param.span.start;
                (self.source.get(start..start + len) == Some(word))
                    .then(|| Span::new(start, start + len))
            };
            match param.mode {
                Some(ParamMode::Mut) if !written => {
                    if let Some(span) = keyword(3, "mut") {
                        self.warn("W1002", span);
                    }
                }
                Some(ParamMode::Take) if !written => {
                    let returned_unchanged = body.tail.as_ref().is_some_and(|tail| {
                        matches!(
                            &*tail.kind,
                            ExprKind::Path(path)
                                if path.is_single() && path.segments[0].name == name
                        )
                    });
                    if returned_unchanged && let Some(span) = keyword(4, "take") {
                        self.warn("W1003", span);
                    }
                }
                _ => {}
            }
        }
        self.writes = outer_writes;
        self.literal_lets = outer_literal_lets;
        self.list_locals = outer_list_locals;

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
        let outer_origin = self.origin;
        if let Some(origin) = crate::parse::index_origin_of(&stmt.attrs) {
            self.origin = origin;
        }
        self.stmt_inner(stmt, tail_position);
        self.origin = outer_origin;
    }

    fn stmt_inner(&mut self, stmt: &Stmt, tail_position: bool) {
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
        // W0305's annotated-`let`/`var` position (D52): the annotation's row
        // is the expected row of the initializer.
        if let Some(ty) = &binding.ty {
            let row = crate::sema::type_tags(ty);
            if !row.is_empty() {
                self.tag_shadow_use(&binding.value, &row);
            }
        }
        self.expr(&binding.value);
        // E0802's shape evidence: a `let` bound directly to a scalar
        // literal has a type no variant pattern can ever match.
        if binding.kind == BindingKind::Let
            && let PatKind::Binding(ident) = &*binding.pattern.kind
            && matches!(
                &*binding.value.kind,
                ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_)
            )
        {
            self.literal_lets.push(ident.name.clone());
        }
        // W0317's receiver evidence: a name bound directly to a `List[…]()`
        // constructor is the one receiver whose `.get` the walk KNOWS counts
        // from 0. A rebinding to anything else ends the story.
        if let PatKind::Binding(ident) = &*binding.pattern.kind {
            self.list_locals.retain(|name| name != &ident.name);
            if is_list_constructor(&binding.value) {
                self.list_locals.push(ident.name.clone());
            }
        }
        self.declare_pattern(&binding.pattern, binding.kind == BindingKind::Var);
    }

    /// One assignment statement: the E1101/W1101 capture check, the W1302
    /// assume check, and the W1102 record.
    fn assign(&mut self, place: &Expr, stmt_span: Span) {
        // The W1002/W1003 evidence: a write through any projection writes
        // the head (`l[0] = v` writes `l`, `self.x = v` writes `self`).
        if let Some(head) = head_of(place) {
            self.writes.push(head.to_owned());
        }
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

        // E1101/W1101 — assignment is the capture law's ASSIGNMENT spelling.
        // W1101 rides along here and only here: it says the write "lands on
        // the task's own copy", which is a claim about assignment to a
        // by-value capture. The lend spellings (`(mut x).m()`, `f(mut x)`)
        // report through the same door with `stays_inside = false` — see
        // `capture_write`, and `conc/capture_mut_lend.lu`'s header, which
        // pins E1101 with no `warns:` line.
        self.capture_write(name, path.segments[0].span, true);
    }

    /// The capture law (`[conc.task.spawn]`, D14), one door for all three
    /// spellings of a write to captured state.
    ///
    /// Inside a task closure, a write to a name the closure did not declare
    /// is a write to captured state. `when` bodies are exempt
    /// (`[conc.when.body]` — the sync objects mediate). A name that resolves
    /// nowhere in the enclosing function (a module item, a prelude name) is
    /// not a capture; the walk never guesses about it.
    ///
    /// `stays_inside` adds W1101, whose text is about an assignment landing
    /// on the task's own copy. A `mut` lend is not that shape — it hands the
    /// callee an exclusive window — so the lend spellings pass `false`, which
    /// is also what the counterparty emits (E1101 alone, no W1101).
    fn capture_write(&mut self, name: &str, span: Span, stays_inside: bool) {
        let capture = self.task_depth > 0 && self.when_depth == 0 && {
            let base = *self.task_scope_base.last().expect("task_depth > 0");
            let holds = |scopes: &[Vec<Local>]| {
                scopes
                    .iter()
                    .any(|scope| scope.iter().any(|local| local.name == name))
            };
            !holds(&self.scopes[base..]) && holds(&self.scopes[..base])
        };
        if !capture {
            return;
        }
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
        if stays_inside {
            self.warn("W1101", span);
        }
    }

    /// A `mut` lend of a captured binding is a write (s74, wolf-lang#71).
    ///
    /// Both lend positions route here: the X1 moded receiver
    /// `(mut xs).push(1)` and the call-site argument mode `f(mut n)`. The
    /// judgement is deliberately narrow — a single-segment path, exactly the
    /// shape `assign` judges — so `(mut xs[0]).m()` and every qualified path
    /// still decline. `take` is not this law: it is a move, and no pinned
    /// clause makes it a captured *write*.
    fn capture_lend(&mut self, mode: ParamMode, place: &Expr) {
        if mode != ParamMode::Mut {
            return;
        }
        let ExprKind::Path(path) = &*place.kind else {
            return;
        };
        if !path.is_single() {
            return;
        }
        let name = path.segments[0].name.clone();
        self.capture_write(&name, path.segments[0].span, false);
    }

    fn expr(&mut self, expr: &Expr) {
        match &*expr.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Char(_)
            | ExprKind::Wildcard => {}
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
            ExprKind::BracketApply { base, args, .. } => {
                self.explicit_apply(expr, base, args);
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
            ExprKind::ModedReceiver { place, mode } => {
                // `(mut l).push(…)` / `(take conn).close()` — the receiver
                // surrenders exclusive or consuming access: W1002/W1003
                // evidence for the head.
                if let Some(head) = head_of(place) {
                    self.writes.push(head.to_owned());
                }
                // …and, inside a task closure, a `mut` receiver-lend of a
                // captured binding is a write to captured state (E1101).
                self.capture_lend(*mode, place);
                self.expr(place);
            }
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
                        // The binder is tag-shaped: what `else` caught is the
                        // operand's error, and its declared row (when the
                        // callee's signature is in sight) travels with the
                        // name for the arm-resolution rule (#44/#48).
                        let row = self.operand_row(inner);
                        self.scopes.push(Vec::new());
                        self.declare_err_binder(pattern, row);
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
                self.match_reachability(scrutinee, arms);
                // Tag-shaped scrutinee? A single-segment path resolving to
                // an `else`-handler's error binder carries its row here.
                let scrutinee_row: Option<Vec<String>> = match &*scrutinee.kind {
                    ExprKind::Path(path) if path.is_single() => self
                        .local(&path.segments[0].name)
                        .and_then(|local| local.err_row.clone()),
                    _ => None,
                };
                for arm in arms {
                    self.scopes.push(Vec::new());
                    if !self.arm_is_tag_pattern(&arm.pattern, scrutinee_row.as_deref()) {
                        self.declare_pattern(&arm.pattern, false);
                    }
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
            ExprKind::Return(value) => {
                if let Some(value) = value {
                    // W0305's return position (D52): the enclosing declared
                    // return row is the expected row of the operand.
                    let row = std::mem::take(&mut self.ret_row);
                    self.tag_shadow_use(value, &row);
                    self.ret_row = row;
                    self.expr(value);
                }
            }
            ExprKind::Break(value) => {
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

    /// E0802 — a dead `match` arm, literal-precise (#54: arms past the
    /// first are NOT blanket-dead): a literal arm duplicating an earlier
    /// unguarded literal, any arm after an unguarded `_`, and a `_` after
    /// bool's two literals. Bare identifiers are never treated as
    /// catch-alls here — a handler-match ident resolves against the
    /// operand row's tags first (#48), which this static walk cannot see.
    /// Spans: the dead arm's pattern (`[464,467]` = the duplicated `"x"`;
    /// `[337,338]` = the `_` after `true`/`false`), observed at the pin.
    ///
    /// is31 (#179) widens the same three rules to PRODUCT arms, column-wise:
    /// a product pattern is a vector of per-column tests, an earlier
    /// unguarded arm kills a later one when it covers it column by column,
    /// and the `_`-after-`true`/`false` rule becomes "a bool COLUMN split by
    /// two otherwise-unconstrained arms closes the shape"
    /// (`typecheck/match_arm_product_unreachable.lu`: the third arm is dead
    /// because arms one and two split `on`). Every column this walk cannot
    /// judge is opaque, and an opaque column neither covers nor is covered —
    /// `match_arm_deep_tree.lu`'s enum test at product depth stays live.
    fn match_reachability(&mut self, scrutinee: &Expr, arms: &[crate::ast::MatchArm]) {
        fn literal_key(expr: &Expr) -> Option<String> {
            match &*expr.kind {
                ExprKind::Bool(b) => Some(format!("b:{b}")),
                ExprKind::Int(text) => Some(format!("i:{text}")),
                ExprKind::Str(lit) => {
                    let mut cooked = String::new();
                    for part in &lit.parts {
                        match part {
                            StrPart::Text(text) => cooked.push_str(text),
                            StrPart::Interp { .. } => return None,
                        }
                    }
                    Some(format!("s:{cooked}"))
                }
                _ => None,
            }
        }
        // The shape-degradation half (`typecheck/pattern_shape.lu`): when
        // the scrutinee is certainly a scalar — a literal, or a `let` bound
        // directly to one — a variant pattern can never be checked against
        // it (E0808's shape); the checker degrades it to its bindings,
        // which cover everything, so the arms after it are dead. Only the
        // certain case fires: an error-row scrutinee keeps its variant
        // arms meaningful and this walk cannot type those.
        let scalar_scrutinee = match &*scrutinee.kind {
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => true,
            ExprKind::Path(path) if path.is_single() => self
                .literal_lets
                .iter()
                .any(|name| *name == path.segments[0].name),
            _ => false,
        };
        let mut seen: Vec<String> = Vec::new();
        let mut products: Vec<ProductKey> = Vec::new();
        let mut wildcard_seen = false;
        for arm in arms {
            let key = match &*arm.pattern.kind {
                PatKind::Literal(expr) => literal_key(expr),
                _ => None,
            };
            let product = product_key(&arm.pattern, &literal_key);
            let bool_complete =
                seen.iter().any(|k| k == "b:true") && seen.iter().any(|k| k == "b:false");
            let dead = wildcard_seen
                || key.as_ref().is_some_and(|k| seen.contains(k))
                || (matches!(&*arm.pattern.kind, PatKind::Wildcard) && bool_complete)
                || product.as_ref().is_some_and(|p| {
                    products.iter().any(|earlier| earlier.covers(p))
                        || bool_column_closes(&products, &p.shape)
                });
            if dead {
                self.warn("E0802", arm.pattern.span);
                continue;
            }
            if arm.guard.is_none() {
                if let Some(key) = key {
                    seen.push(key);
                } else if let Some(product) = product {
                    // A product every column of which is a binder is
                    // irrefutable: it covers its whole type, so it is the
                    // catch-all the later arms die behind.
                    if product.columns.values().all(|c| *c == Column::Any) {
                        wildcard_seen = true;
                    }
                    products.push(product);
                } else if matches!(&*arm.pattern.kind, PatKind::Wildcard)
                    || (scalar_scrutinee && matches!(&*arm.pattern.kind, PatKind::Variant { .. }))
                {
                    wildcard_seen = true;
                }
            }
        }
    }

    fn arg(&mut self, arg: &Arg) {
        // A call-site `mut`/`take` argument surrenders exclusive or
        // consuming access — W1002/W1003 evidence for the head.
        if arg.mode.is_some()
            && let Some(head) = head_of(&arg.expr)
        {
            self.writes.push(head.to_owned());
        }
        // W0308 — a `mut` argument inside a string interpolation: a write
        // buried where readers expect pure formatting. The span is the mode
        // through the argument (`[458,463]` = `mut a` on `mut_in_interp.lu`).
        if self.in_interp && arg.mode == Some(ParamMode::Mut) {
            self.warn("W0308", arg.span);
        }
        // …and, inside a task closure, a `mut` argument-lend of a captured
        // binding is a write to captured state (E1101). Same law as the
        // receiver spelling, so the same door — `f(mut n)` and `(mut n).m()`
        // must never drift apart again (wolf-lang#71).
        if let Some(mode) = arg.mode {
            self.capture_lend(mode, &arg.expr);
        }
        self.expr(&arg.expr);
    }

    /// E0812 — explicit generic application arity (wolf-lang#111, the
    /// wolf-interp#34 straggler): the explicit form is one type per generic
    /// parameter, in declaration order. Judged only where BOTH counts are in
    /// sight without a type checker: the base names a module-level `fn` no
    /// local shadows — `e[…]` is one production (`[gram.amb.brackets]`), and
    /// a fn base makes it application, never indexing — which is exactly the
    /// corpus's shape (`generics/explicit_apply_arity.lu`, `pick[int, str]`
    /// against one declared parameter). The non-type-argument half of E0812
    /// and dotted callees stay the compiler's; declining those is the honest
    /// narrowness, never a wrong refusal.
    fn explicit_apply(&mut self, expr: &Expr, base: &Expr, args: &[IndexArg]) {
        let ExprKind::Path(path) = &*base.kind else {
            return;
        };
        if !path.is_single() || self.declared(&path.segments[0].name).is_some() {
            return;
        }
        let Some(decl) = self.fn_decls.get(path.segments[0].name.as_str()) else {
            return;
        };
        if args.is_empty() || args.len() == decl.generics.len() {
            return;
        }
        // The brackets, types included — the span of the wrong count.
        let span = Span::new(base.span.end, expr.span.end);
        self.statics.push(Diag::new(
            "E0812",
            span,
            "generics.apply.explicit",
            format!(
                "`{name}` declares {declared} generic parameter(s) and this explicit \
                 application spells {spelled} type(s) — one type per generic parameter, in \
                 declaration order; the generics are declared at `{name}`'s signature",
                name = path.segments[0].name,
                declared = decl.generics.len(),
                spelled = args.len(),
            ),
        ));
    }

    fn call(&mut self, _call: &Expr, callee: &Expr, args: &[Arg]) {
        // W0317 — the D61 kindness lint (`[gram.expr.index.origin]`): `.get`
        // is origin-free, so an int literal fed to it inside a 1-origin
        // scope is usually a subscript habit carried over, off by one. Span:
        // the literal (`[468,469]` on `lints/index_origin_get.lu`, observed
        // at pin addcd7f); a non-literal index warns nothing, and only a
        // receiver visibly bound to `List[…]()` fires — a user type's `get`
        // does not warn on the compiled lanes either (probed).
        if self.origin == 1
            && let ExprKind::Path(path) = &*callee.kind
            && let [head, member] = path.segments.as_slice()
            && member.name == "get"
            && self.list_locals.iter().any(|local| local == &head.name)
            && let [arg] = args
            && arg.mode.is_none()
            && matches!(&*arg.expr.kind, ExprKind::Int(_))
        {
            self.warn("W0317", arg.expr.span);
        }
        // E1102 — `channel[T](…)` with a visibly unsendable payload: a bare
        // region-interior container can never cross a channel.
        if let ExprKind::BracketApply {
            base, args: targs, ..
        } = &*callee.kind
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

        // W0305's argument position (D52): a call to a module-level fn whose
        // parameter's declared row spells a bare-identifier argument that a
        // local also binds — the local wins, and the shadow warns at the use.
        if let ExprKind::Path(path) = &*callee.kind
            && path.is_single()
            && self.declared(&path.segments[0].name).is_none()
            && let Some(decl) = self.fn_decls.get(path.segments[0].name.as_str())
        {
            for (param, arg) in decl.params.iter().zip(args) {
                if let crate::ast::ParamKind::Named { ty, .. } = &param.kind {
                    let row = crate::sema::type_tags(ty);
                    self.tag_shadow_use(&arg.expr, &row);
                }
            }
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
                .map(|param| Local::plain(param.name.name.clone(), param.mode.is_some()))
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

/// Whether an initializer is visibly the `List` constructor — `List[int]()`
/// (generic application) or bare `List()` — W0317's receiver evidence.
fn is_list_constructor(expr: &Expr) -> bool {
    let ExprKind::Call { callee, .. } = &*expr.kind else {
        return false;
    };
    let mut base = callee;
    if let ExprKind::BracketApply { base: b, .. } = &*base.kind {
        base = b;
    }
    matches!(&*base.kind, ExprKind::Path(path)
        if path.is_single() && path.segments[0].name == "List")
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
pub(crate) fn free_names(expr: &Expr, locals: &mut BTreeSet<String>, out: &mut BTreeSet<String>) {
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

/// One column of a product pattern's coverage (is31, E0802 over products).
///
/// `Opaque` is the honest third answer: a shape this static walk declines to
/// judge — a nested tag test, an or-pattern, a capitalized name that may be a
/// variant. It never covers and is never covered, so an arm carrying one is
/// neither declared dead nor allowed to kill a later arm.
#[derive(PartialEq, Eq, Clone, Debug)]
enum Column {
    Lit(String),
    Any,
    Opaque,
}

impl Column {
    /// Does a column of an earlier arm cover the same column of a later one?
    fn covers(&self, other: &Column) -> bool {
        match (self, other) {
            (Column::Any, _) => true,
            (Column::Lit(a), Column::Lit(b)) => a == b,
            _ => false,
        }
    }
}

/// A product arm's coverage: its shape (which product it tests) and one
/// column per named position. A column the pattern leaves unnamed — omitted
/// under `..`, or absent from another arm's field set — is [`Column::Any`].
#[derive(Debug)]
struct ProductKey {
    shape: String,
    columns: BTreeMap<String, Column>,
}

impl ProductKey {
    fn column(&self, name: &str) -> &Column {
        self.columns.get(name).unwrap_or(&Column::Any)
    }

    /// Does this (earlier, unguarded) arm cover a later one entirely?
    fn covers(&self, other: &ProductKey) -> bool {
        self.shape == other.shape
            && self
                .columns
                .keys()
                .chain(other.columns.keys())
                .all(|name| self.column(name).covers(other.column(name)))
    }
}

/// Do two earlier arms split a bool COLUMN and constrain nothing else?
///
/// The generalization of the scalar `_`-after-`true`/`false` rule: two arms
/// that agree on `Any` everywhere except one column, where one tests `true`
/// and the other `false`, together cover the whole shape — so every later arm
/// of that shape is dead (`typecheck/match_arm_product_unreachable.lu`).
fn bool_column_closes(products: &[ProductKey], shape: &str) -> bool {
    let single = |key: &ProductKey, column: &str, want: &str| {
        key.shape == shape
            && key.column(column) == &Column::Lit(want.to_owned())
            && key
                .columns
                .iter()
                .all(|(name, value)| name == column || *value == Column::Any)
    };
    products
        .iter()
        .filter(|key| key.shape == shape)
        .flat_map(|key| key.columns.keys())
        .any(|column| {
            products.iter().any(|key| single(key, column, "b:true"))
                && products.iter().any(|key| single(key, column, "b:false"))
        })
}

/// The coverage of a top-level product arm, or `None` for every other shape.
///
/// `@` is transparent here: `w @ (0, b)` tests exactly what `(0, b)` tests.
/// Nesting is not walked — a sub-product or a tag test at depth is one opaque
/// column, which is what keeps `match_arm_deep_tree.lu`'s arms live.
fn product_key(
    pattern: &Pattern,
    literal_key: &dyn Fn(&Expr) -> Option<String>,
) -> Option<ProductKey> {
    fn column(pattern: &Pattern, literal_key: &dyn Fn(&Expr) -> Option<String>) -> Column {
        match &*pattern.kind {
            PatKind::Wildcard => Column::Any,
            // A lowercase name binds; a capitalized one may be a variant or a
            // row tag, which this walk cannot resolve (#48).
            PatKind::Binding(ident) if ident.name.starts_with(char::is_lowercase) => Column::Any,
            PatKind::Literal(expr) => literal_key(expr).map_or(Column::Opaque, Column::Lit),
            PatKind::At { pattern, .. } => column(pattern, literal_key),
            _ => Column::Opaque,
        }
    }
    match &*pattern.kind {
        PatKind::At { pattern, .. } => product_key(pattern, literal_key),
        PatKind::Tuple(items) => Some(ProductKey {
            shape: format!("tuple:{}", items.len()),
            columns: items
                .iter()
                .enumerate()
                .map(|(index, item)| (index.to_string(), column(item, literal_key)))
                .collect(),
        }),
        PatKind::Struct { path, fields, .. } => Some(ProductKey {
            shape: format!(
                "struct:{}",
                path.segments.last().map(|s| s.name.as_str()).unwrap_or("")
            ),
            columns: fields
                .iter()
                .map(|field| {
                    let value = match &field.pattern {
                        Some(sub) => column(sub, literal_key),
                        None => Column::Any,
                    };
                    (field.name.name.clone(), value)
                })
                .collect(),
        }),
        _ => None,
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
        PatKind::Struct { fields, .. } => {
            for field in fields {
                match &field.pattern {
                    Some(sub) => collect_pattern_names(sub, into),
                    None => {
                        into.insert(field.name.name.clone());
                    }
                }
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
        | ExprKind::Char(_)
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
        ExprKind::BracketApply { base, args, .. } => {
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
/// The head name a place expression writes through: `l[0]`, `b.items[0]`,
/// `(mut p).x` all answer their leftmost identifier.
fn head_of(place: &Expr) -> Option<&str> {
    match &*place.kind {
        ExprKind::Path(path) => Some(path.segments.first()?.name.as_str()),
        ExprKind::Member { base, .. } | ExprKind::BracketApply { base, .. } => head_of(base),
        ExprKind::ModedReceiver { place, .. } | ExprKind::Group(place) => head_of(place),
        _ => None,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every E-code the capture law raised over one program, with its span.
    fn capture_codes(source: &str) -> Vec<(String, Span)> {
        let program = crate::sema::load_source("t.lu", source).expect("loads");
        analyze(&program)
            .statics
            .into_iter()
            .filter(|d| d.code == "E1101")
            .map(|d| (d.code.to_owned(), d.span))
            .collect()
    }

    fn warn_codes(source: &str) -> Vec<String> {
        let program = crate::sema::load_source("t.lu", source).expect("loads");
        analyze(&program)
            .warnings
            .into_iter()
            .map(|w| w.code)
            .collect()
    }

    // ---- E0812: explicit generic application arity (wolf-lang#111) --------

    /// Every E0812 the walk raised over one program, with its span.
    fn arity_codes(source: &str) -> Vec<(String, Span)> {
        let program = crate::sema::load_source("t.lu", source).expect("loads");
        analyze(&program)
            .statics
            .into_iter()
            .filter(|d| d.code == "E0812")
            .map(|d| (d.code.to_owned(), d.span))
            .collect()
    }

    #[test]
    fn a_wrong_count_explicit_application_fails_e0812() {
        // The corpus witness's shape (generics/explicit_apply_arity.lu):
        // one declared parameter, two bracket types.
        let found = arity_codes(
            "fn pick[T](xs: List[T], i: int) -> T ! {none} {\n\
             \x20   xs.get(i)\n\
             }\n\
             fn main() -> !int {\n\
             \x20   var a = List[int]()\n\
             \x20   (mut a).push(7)\n\
             \x20   let v = pick[int, str](a, 0) else 0\n\
             \x20   if v == 7 { 0 } else { 1 }\n\
             }\n",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        // The span is the bracket list, types included: `[int, str]`.
        let source_span = &found[0].1;
        assert_eq!(source_span.end - source_span.start, "[int, str]".len());
    }

    #[test]
    fn the_arity_judgement_is_exactly_as_narrow_as_the_syntax() {
        // Correct arity: no diagnostic (generics/explicit_apply.lu's shape).
        let ok = arity_codes(
            "fn pick[T](xs: List[T], i: int) -> T ! {none} {\n\
             \x20   xs.get(i)\n\
             }\n\
             fn main() -> !int {\n\
             \x20   var a = List[int]()\n\
             \x20   (mut a).push(7)\n\
             \x20   let v = pick[int](a, 0) else 0\n\
             \x20   if v == 7 { 0 } else { 1 }\n\
             }\n",
        );
        assert!(ok.is_empty(), "{ok:?}");

        // A local shadowing the fn name makes the brackets INDEXING, and the
        // judgement declines — never a wrong refusal.
        let shadowed = arity_codes(
            "fn pick[T](xs: List[T], i: int) -> T ! {none} {\n\
             \x20   xs.get(i)\n\
             }\n\
             fn main() -> !int {\n\
             \x20   var pick = List[int]()\n\
             \x20   (mut pick).push(7)\n\
             \x20   if pick[0] == 7 { 0 } else { 1 }\n\
             }\n",
        );
        assert!(shadowed.is_empty(), "{shadowed:?}");

        // A builtin constructor's bracket type is not a module fn's.
        let builtin = arity_codes(
            "fn main() -> !int {\n\
             \x20   var a = List[int]()\n\
             \x20   (mut a).push(7)\n\
             \x20   if a[0] == 7 { 0 } else { 1 }\n\
             }\n",
        );
        assert!(builtin.is_empty(), "{builtin:?}");
    }

    // ---- W0305's fire-at-use (D52, [gram.expr.tagident]) ------------------
    //
    // A local named like a tag the expected declared row spells SHADOWS the
    // tag — locals win, matching resolution everywhere else — and the hazard
    // warns at the use, in every checked position: call argument, annotated
    // `let`/`var` initializer, `return` operand.

    #[test]
    fn a_local_shadowing_a_declared_tag_warns_at_the_argument_use() {
        // The corpus witness's own shape (rows/tag_shadow_local.lu).
        let found = warn_codes(
            "fn or(v: int ! {none}, d: int) -> int { v else d }\n\
             fn main() -> !int {\n\
             \x20   let none = 3\n\
             \x20   if or(none, 9) == 3 { 0 } else { 1 }\n\
             }\n",
        );
        assert_eq!(
            found.iter().filter(|c| *c == "W0305").count(),
            1,
            "{found:?}"
        );
    }

    #[test]
    fn a_local_shadowing_the_annotation_row_warns_at_the_initializer() {
        let found = warn_codes(
            "fn main() -> !int {\n\
             \x20   let none = 3\n\
             \x20   let v: int ! {none} = none\n\
             \x20   let w = v else 5\n\
             \x20   if w == 3 { 0 } else { 1 }\n\
             }\n",
        );
        assert_eq!(
            found.iter().filter(|c| *c == "W0305").count(),
            1,
            "{found:?}"
        );
    }

    #[test]
    fn a_local_shadowing_the_return_row_warns_at_the_return() {
        let found = warn_codes(
            "fn f() -> int ! {none} {\n\
             \x20   let none = 3\n\
             \x20   return none\n\
             }\n\
             fn main() -> !int {\n\
             \x20   f() else 0\n\
             }\n",
        );
        assert_eq!(
            found.iter().filter(|c| *c == "W0305").count(),
            1,
            "{found:?}"
        );
    }

    #[test]
    fn the_fire_at_use_is_exactly_as_wide_as_the_shadow() {
        // No local: the bare tag resolves as the tag, no warning
        // (rows/tag_arg_position.lu and tag_let_position.lu are
        // warning-clean); a local whose name no expected row spells is an
        // ordinary argument.
        let clean = warn_codes(
            "fn or(v: int ! {none}, d: int) -> int { v else d }\n\
             fn main() -> !int {\n\
             \x20   if or(none, 9) == 9 { 0 } else { 1 }\n\
             }\n",
        );
        assert!(!clean.iter().any(|c| c == "W0305"), "{clean:?}");

        let ordinary = warn_codes(
            "fn or(v: int ! {none}, d: int) -> int { v else d }\n\
             fn main() -> !int {\n\
             \x20   let k = 3\n\
             \x20   if or(k, 9) == 3 { 0 } else { 1 }\n\
             }\n",
        );
        assert!(!ordinary.iter().any(|c| c == "W0305"), "{ordinary:?}");
    }

    // ---- the capture law's three spellings (s74, wolf-lang#71) ------------
    //
    // The corpus pins the positive half at the pin's three `conc/capture_*`
    // files. What it does NOT pin is the judgement's *narrowness*, and a
    // capture check that fires too widely is how a false rejection ships.

    #[test]
    fn a_mut_receiver_lend_of_a_captured_binding_is_a_write() {
        let found = capture_codes(
            "fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   scope s {\n\
             \x20       s.spawn(fn() { (mut xs).push(1) })\n\
             \x20   }\n\
             \x20   0\n\
             }\n",
        );
        assert_eq!(found.len(), 1, "the lend spelling raises E1101: {found:?}");
    }

    #[test]
    fn a_mut_argument_lend_of_a_captured_binding_is_a_write() {
        let found = capture_codes(
            "fn bump(mut k: int) { k = k + 1 }\n\
             fn main() -> !int {\n\
             \x20   var n = 0\n\
             \x20   scope s {\n\
             \x20       s.spawn(fn() { bump(mut n) })\n\
             \x20   }\n\
             \x20   0\n\
             }\n",
        );
        assert_eq!(found.len(), 1, "the arg spelling raises E1101: {found:?}");
    }

    #[test]
    fn the_lend_spellings_raise_no_w1101() {
        // W1101 says the write "lands on the task's own copy", which is a
        // claim about ASSIGNMENT to a by-value capture. A `mut` lend is a
        // different shape, the counterparty emits E1101 alone there, and the
        // corpus headers carry `warns:` on the assignment file only.
        let lend = warn_codes(
            "fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   scope s {\n\
             \x20       s.spawn(fn() { (mut xs).push(1) })\n\
             \x20   }\n\
             \x20   0\n\
             }\n",
        );
        assert!(
            !lend.iter().any(|c| c == "W1101"),
            "a lend is not a write-to-my-own-copy: {lend:?}"
        );

        let assign = warn_codes(
            "fn main() -> !int {\n\
             \x20   var n = 0\n\
             \x20   scope s {\n\
             \x20       s.spawn(fn() { n = 1 })\n\
             \x20   }\n\
             \x20   0\n\
             }\n",
        );
        assert!(
            assign.iter().any(|c| c == "W1101"),
            "the assignment spelling still warns: {assign:?}"
        );
    }

    #[test]
    fn a_take_lend_is_not_this_law() {
        // `take` is a move, not a captured *write*, and no pinned clause
        // makes it one. Judging it here would be inventing a rule.
        let found = capture_codes(
            "fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   scope s {\n\
             \x20       s.spawn(fn() { (take xs).len })\n\
             \x20   }\n\
             \x20   0\n\
             }\n",
        );
        assert!(found.is_empty(), "`take` is not the capture law: {found:?}");
    }

    #[test]
    fn a_lend_of_the_closures_own_local_is_not_a_capture() {
        let found = capture_codes(
            "fn main() -> !int {\n\
             \x20   scope s {\n\
             \x20       s.spawn(fn() { var own = List[int]()\n\
             \x20           (mut own).push(1) })\n\
             \x20   }\n\
             \x20   0\n\
             }\n",
        );
        assert!(
            found.is_empty(),
            "a binding the closure declared is not captured: {found:?}"
        );
    }

    #[test]
    fn a_lend_outside_any_task_is_not_a_capture() {
        let found = capture_codes(
            "fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   (mut xs).push(1)\n\
             \x20   0\n\
             }\n",
        );
        assert!(found.is_empty(), "no task, no capture law: {found:?}");
    }

    // ---- W0305 v. the arm-resolution rule (is24, wolf-interp#44) ----------
    //
    // THE BOUNDARY. The rule (`[gram.expr.tagident]` D52; wolf-lang#30 at
    // raise position; wolf-lang#48 on the handler side; this machine's own
    // `eval::match_pattern` since is19): over a tag-shaped scrutinee, a bare
    // lowercase identifier pattern that names a *declared* row tag — the
    // scrutinee's own row, or the module's row vocabulary — is a ROW-TAG
    // PATTERN. It dispatches on the tag and binds NOTHING. W0305's subject
    // is a double reading: a local BINDING standing where an expected row
    // spells the same word. No binding, no shadow, no warning.
    //
    // Must NOT warn (the arm's name is the resolved tag, not a shadow):
    //   - the same-tag re-raise in its own arm (#44's shape, wsm01 verbatim);
    //   - ANY checked-position use of the arm's tag in its body (argument
    //     position too — the rule, not the reported symptom);
    //   - a cross-tag body naming a different declared tag (never warned).
    // Must STILL warn (a real binding stands between the row and the use):
    //   - a `let` of the tag's name, anywhere — including INSIDE the arm
    //     whose pattern is that very tag;
    //   - an `else |binder|` named like a tag (a handler binder is
    //     irrefutable and always binds — E0806's law — so it truly shadows);
    //   - a nested match over a NON-tag-shaped scrutinee whose bare-ident
    //     arm rebinds the tag's name (a catch-all binding, per the same
    //     rule read from its other side).

    /// Every W0305 the walk raised over one program.
    fn shadow_codes(source: &str) -> Vec<String> {
        warn_codes(source)
            .into_iter()
            .filter(|code| code == "W0305")
            .collect()
    }

    #[test]
    fn a_local_let_shadowing_a_row_tag_still_warns_at_the_use() {
        // The corpus witness's class (rows/tag_shadow_local.lu): the local
        // IS a binding, locals win, and the double reading is priced. This
        // negative is load-bearing: the #44 fix must not weaken it.
        let found = shadow_codes(
            "fn or(v: int ! {none}, d: int) -> int {\n\
             \x20   v else d\n\
             }\n\
             fn main() -> !int {\n\
             \x20   let none = 3\n\
             \x20   if or(none, 9) == 3 { 0 } else { 1 }\n\
             }\n",
        );
        assert_eq!(found.len(), 1, "a real local still shadows: {found:?}");
    }

    #[test]
    fn a_let_inside_the_tag_arm_itself_still_warns() {
        // The arm is the tag (binds nothing) — but a `let` of the same name
        // INSIDE its body is a real binding, and the use after it is the
        // double reading again. Proves the fix is arm-resolution-scoped,
        // never name-scoped.
        let found = shadow_codes(
            "fn f() -> int ! {refused} {\n\
             \x20   return refused\n\
             }\n\
             fn g() -> int ! {refused} {\n\
             \x20   let v = f() else |e| match e {\n\
             \x20       refused => {\n\
             \x20           let refused = 1\n\
             \x20           if refused == 1 {\n\
             \x20               return refused\n\
             \x20           }\n\
             \x20           0\n\
             \x20       },\n\
             \x20       _ => 0,\n\
             \x20   }\n\
             \x20   v\n\
             }\n\
             fn main() -> !int {\n\
             \x20   let a = g() else 9\n\
             \x20   a\n\
             }\n",
        );
        assert_eq!(
            found.len(),
            1,
            "the inner `let` is a genuine shadow: {found:?}"
        );
    }

    #[test]
    fn an_else_binder_named_like_a_tag_still_warns() {
        // `else |refused|` BINDS — a handler binder is irrefutable (E0806's
        // law), so the name truly stands between the row and the return.
        let found = shadow_codes(
            "fn f() -> int ! {refused} {\n\
             \x20   return refused\n\
             }\n\
             fn g() -> int ! {refused} {\n\
             \x20   let v = f() else |refused| {\n\
             \x20       return refused\n\
             \x20   }\n\
             \x20   v\n\
             }\n\
             fn main() -> !int {\n\
             \x20   let a = g() else 9\n\
             \x20   a\n\
             }\n",
        );
        assert_eq!(
            found.len(),
            1,
            "a binder named like a tag shadows: {found:?}"
        );
    }

    #[test]
    fn a_nested_match_rebinding_the_tag_still_warns() {
        // The inner match's scrutinee is a scalar, not the tag-shaped
        // binder — its bare-ident arm is a catch-all BINDING (the same rule
        // read from the other side), and the use under it is shadowed.
        let found = shadow_codes(
            "fn f() -> int ! {refused} {\n\
             \x20   return refused\n\
             }\n\
             fn g(x: int) -> int ! {refused} {\n\
             \x20   let v = f() else |e| match e {\n\
             \x20       refused => match x {\n\
             \x20           refused => {\n\
             \x20               return refused\n\
             \x20           },\n\
             \x20       },\n\
             \x20       _ => 0,\n\
             \x20   }\n\
             \x20   v\n\
             }\n\
             fn main() -> !int {\n\
             \x20   let a = g(3) else 9\n\
             \x20   a\n\
             }\n",
        );
        assert_eq!(
            found.len(),
            1,
            "rebinding under a non-tag scrutinee shadows: {found:?}"
        );
    }

    #[test]
    fn the_same_tag_re_raise_in_its_own_arm_is_clean() {
        // #44's minimal shape: the arm IS the tag ([gram.expr.tagident]'s
        // handler side, wolf-lang#48), the return operand resolves as the
        // tag (D52's declared-row-first), and nothing is bound anywhere —
        // so there is no shadow for W0305 to price.
        let found = shadow_codes(
            "fn f() -> int ! {refused, io} {\n\
             \x20   return refused\n\
             }\n\
             fn g() -> int ! {refused, io} {\n\
             \x20   let v = f() else |e| match e {\n\
             \x20       refused => {\n\
             \x20           return refused\n\
             \x20       },\n\
             \x20       io => {\n\
             \x20           return io\n\
             \x20       },\n\
             \x20   }\n\
             \x20   v\n\
             }\n\
             fn main() -> !int {\n\
             \x20   let a = g() else 9\n\
             \x20   a\n\
             }\n",
        );
        assert!(
            found.is_empty(),
            "the arm is the tag, not a shadow: {found:?}"
        );
    }

    #[test]
    fn a_checked_argument_use_of_the_arm_tag_is_clean() {
        // The rule, not the symptom: the arm's tag used in ARGUMENT position
        // (against a callee parameter row that spells it) is the same
        // resolved name. A fix that only silenced `return <tag>` fails here.
        let found = shadow_codes(
            "fn f() -> int ! {refused} {\n\
             \x20   return refused\n\
             }\n\
             fn or(v: int ! {refused}, d: int) -> int {\n\
             \x20   v else d\n\
             }\n\
             fn g() -> int {\n\
             \x20   let v = f() else |e| match e {\n\
             \x20       refused => or(refused, 7),\n\
             \x20       _ => 0,\n\
             \x20   }\n\
             \x20   v\n\
             }\n\
             fn main() -> !int {\n\
             \x20   if g() == 7 { 0 } else { 1 }\n\
             }\n",
        );
        assert!(
            found.is_empty(),
            "argument position is the same rule: {found:?}"
        );
    }

    #[test]
    fn a_cross_tag_body_naming_a_different_declared_tag_stays_clean() {
        // `io => { return refused }` never warned (the arm's name and the
        // body's differ); pinned so the boundary holds from both sides.
        let found = shadow_codes(
            "fn f() -> int ! {refused, io} {\n\
             \x20   return io\n\
             }\n\
             fn g() -> int ! {refused, io} {\n\
             \x20   let v = f() else |e| match e {\n\
             \x20       io => {\n\
             \x20           return refused\n\
             \x20       },\n\
             \x20       _ => 0,\n\
             \x20   }\n\
             \x20   v\n\
             }\n\
             fn main() -> !int {\n\
             \x20   let a = g() else 9\n\
             \x20   a\n\
             }\n",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_tag_arm_no_longer_masks_the_capture_law() {
        // The same bug from the capture law's side (the is24 honesty pass):
        // the arm-"local" the old walk registered satisfied `holds(scopes)`
        // inside the task closure, so a write to a CAPTURED outer var that
        // shares the tag's name slipped past E1101. The arm is the tag; the
        // write lands on the capture; the law fires.
        let found = capture_codes(
            "fn f() -> int ! {refused} {\n\
             \x20   return refused\n\
             }\n\
             fn main() -> !int {\n\
             \x20   var refused = 0\n\
             \x20   scope s {\n\
             \x20       s.spawn(fn() {\n\
             \x20           let v = f() else |e| match e {\n\
             \x20               refused => {\n\
             \x20                   refused = 1\n\
             \x20                   0\n\
             \x20               },\n\
             \x20               _ => 0,\n\
             \x20           }\n\
             \x20           v\n\
             \x20       })\n\
             \x20   }\n\
             \x20   refused\n\
             }\n",
        );
        assert_eq!(
            found.len(),
            1,
            "the tag arm binds nothing, so the write is captured: {found:?}"
        );
    }

    #[test]
    fn the_wsm01_witness_is_verbatim_clean() {
        // wolf-interp#44's reproducer, byte-shaped as filed (the wolf-wws
        // wsm01 finding): TWO same-tag re-raises, each inside its own arm.
        // wolfc reads this clean at 64a38f3; as of is24 this machine agrees.
        let found = shadow_codes(
            "fn f(x: int) -> int ! {refused, io} {\n\
             \x20   if x == 1 {\n\
             \x20       return refused\n\
             \x20   }\n\
             \x20   if x == 2 {\n\
             \x20       return io\n\
             \x20   }\n\
             \x20   x\n\
             }\n\
             \n\
             fn g(x: int) -> int ! {refused, io} {\n\
             \x20   let v = f(x) else |e| match e {\n\
             \x20       refused => {\n\
             \x20           return refused\n\
             \x20       },\n\
             \x20       io => {\n\
             \x20           return io\n\
             \x20       },\n\
             \x20   }\n\
             \x20   v\n\
             }\n\
             \n\
             fn main() -> int {\n\
             \x20   let a = g(0) else 9\n\
             \x20   a\n\
             }\n",
        );
        assert!(found.is_empty(), "the wsm01 shape is clean: {found:?}");
    }

    // ---- W0317: `.get(literal)` inside a 1-origin scope (D61, #167) -------

    /// Every W0317 over one program, with its span's byte range.
    fn w0317_spans(source: &str) -> Vec<(usize, usize)> {
        let program = crate::sema::load_source("t.lu", source).expect("loads");
        analyze(&program)
            .warnings
            .into_iter()
            .filter(|w| w.code == "W0317")
            .map(|w| (w.span[0] as usize, w.span[1] as usize))
            .collect()
    }

    #[test]
    fn a_literal_fed_to_get_inside_a_1_origin_scope_warns_at_the_literal() {
        // `lints/index_origin_get.lu`'s shape: the literal warns ([468,469]
        // there — the span is the literal), the variable index does not.
        let source = "#![index(1)]\n\
             fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   (mut xs).push(5)\n\
             \x20   let a = xs.get(1) else { 0 - 1 }\n\
             \x20   var j = 0\n\
             \x20   let b = xs.get(j) else { 0 - 1 }\n\
             \x20   if a == 0 - 1 && b == 5 { 0 } else { 1 }\n\
             }\n";
        let found = w0317_spans(source);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(&source[found[0].0..found[0].1], "1");
        assert!(source[..found[0].0].ends_with("xs.get("));
    }

    #[test]
    fn the_statement_marker_narrows_and_the_innermost_wins() {
        // `#[index(0)]` on the statement suppresses exactly that statement's
        // `.get` (observed at addcd7f: one warning, the unannotated line's).
        let source = "#![index(1)]\n\
             fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   #[index(0)]\n\
             \x20   let a = xs.get(1) else { 0 - 1 }\n\
             \x20   let c = xs.get(1) else { 0 - 1 }\n\
             \x20   if a == c { 0 } else { 1 }\n\
             }\n";
        let found = w0317_spans(source);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(source[..found[0].0].ends_with("let c = xs.get("));
    }

    #[test]
    fn the_lint_is_exactly_as_narrow_as_the_observed_surface() {
        // No marker, no warning — the scope IS the trigger.
        assert!(
            w0317_spans(
                "fn main() -> !int {\n\
             \x20   var xs = List[int]()\n\
             \x20   let a = xs.get(1) else { 0 - 1 }\n\
             \x20   a + 1\n\
             }\n"
            )
            .is_empty()
        );
        // A user type's `get` is its own business — the compiled lanes do
        // not warn on it either (probed at addcd7f).
        assert!(
            w0317_spans(
                "#![index(1)]\n\
             struct Wolf { n: int }\n\
             impl Wolf {\n\
             \x20   fn get(self, i: int) -> int {\n\
             \x20       self.n + i\n\
             \x20   }\n\
             }\n\
             fn main() -> !int {\n\
             \x20   let b = Wolf { n: 4 }\n\
             \x20   let v = b.get(1)\n\
             \x20   v - 5\n\
             }\n"
            )
            .is_empty()
        );
    }

    // ---- E0802 over PRODUCT arms (is31, #179) -----------------------------

    /// Every E0802 the reachability walk raised over one program.
    fn dead_arms(source: &str) -> Vec<String> {
        warn_codes(source)
            .into_iter()
            .filter(|code| code == "E0802")
            .collect()
    }

    #[test]
    fn a_split_bool_column_closes_a_struct_shape() {
        // `typecheck/match_arm_product_unreachable.lu`: the first two arms
        // split `on` and constrain nothing else, so the third can never
        // match. Exactly one warning, on the third arm.
        let found = dead_arms(
            "struct Flag { on: bool, n: int }\n\
             fn main() -> !int {\n\
             \x20   let f = Flag { on: false, n: 9 }\n\
             \x20   let r = match f {\n\
             \x20       Flag { on: true, n } => n,\n\
             \x20       Flag { on: false, n } => n * 2,\n\
             \x20       Flag { n, .. } => n * 3,\n\
             \x20   }\n\
             \x20   if r != 18 { return 1 }\n\
             \x20   0\n\
             }\n",
        );
        assert_eq!(found.len(), 1, "the third arm is dead: {found:?}");
    }

    #[test]
    fn a_bool_column_under_another_test_closes_nothing() {
        // The literal-precision half (#54's lesson, column-wise): the two
        // `true`/`false` arms also pin element 0, so they cover only `0` —
        // `tuple_pattern_match_arm.lu`'s four arms are all live.
        assert!(
            dead_arms(
                "fn classify(p: (int, bool)) -> int {\n\
                 \x20   match p {\n\
                 \x20       (0, true) => 1,\n\
                 \x20       (0, false) => 2,\n\
                 \x20       (n, true) => n + 10,\n\
                 \x20       (n, false) => n + 20,\n\
                 \x20   }\n\
                 }\n\
                 fn main() -> !int { classify((0, true)) - 1 }\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_column_this_walk_cannot_judge_kills_nothing() {
        // `match_arm_deep_tree.lu`: the first arm's `p` column is a TAG test
        // at product depth — opaque here — so it neither dies nor kills the
        // arm behind it.
        assert!(
            dead_arms(
                "enum Pairs { Empty, Pair(int, int) }\n\
                 struct Box { p: Pairs, k: int }\n\
                 fn probe(b: Box) -> int {\n\
                 \x20   match b {\n\
                 \x20       Box { p: Pair(a, _), .. } => a,\n\
                 \x20       Box { k, .. } => k,\n\
                 \x20   }\n\
                 }\n\
                 fn main() -> !int { probe(Box { p: Pairs.Pair(7, 1), k: 3 }) - 7 }\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn an_all_binder_product_is_the_catch_all_later_arms_die_behind() {
        // Every column a binder: the arm is irrefutable and covers its whole
        // type, so what follows is dead exactly as after `_`.
        let found = dead_arms(
            "struct Flag { on: bool, n: int }\n\
             fn main() -> !int {\n\
             \x20   let f = Flag { on: true, n: 3 }\n\
             \x20   match f {\n\
             \x20       Flag { on, n } => n,\n\
             \x20       Flag { n, .. } => n * 2,\n\
             \x20   }\n\
             }\n",
        );
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn a_guarded_product_arm_covers_nothing() {
        // A guard can fail, so the arm behind it stays reachable —
        // `match_arm_at_binding.lu`'s middle arm, read from the other side.
        assert!(
            dead_arms(
                "struct Point { x: int, y: int }\n\
                 fn at_struct(p: Point) -> int {\n\
                 \x20   match p {\n\
                 \x20       Point { x, y } if x > y => x - y,\n\
                 \x20       Point { y, .. } => y,\n\
                 \x20   }\n\
                 }\n\
                 fn main() -> !int { at_struct(Point { x: 5, y: 2 }) - 3 }\n"
            )
            .is_empty()
        );
    }
}

//! Sema-lite: the static analysis needed to *run* programs, and no more.
//!
//! This is the load-bearing design decision of the interpreter track, decided
//! at is00 and codified here (README §"The sema boundary"):
//!
//! > The interpreter implements **full dynamic semantics but only the static
//! > analysis needed to run programs**. It does *not* implement the type
//! > checker proper, the borrow checker, or the region checker. Every safety
//! > property those enforce statically is enforced dynamically instead.
//!
//! What lives here, therefore, is exactly the D32 module machinery a call needs
//! in order to find its callee:
//!
//! - **directory = module**, every `.lu` file in a directory is one module, and
//!   forward references across sibling files are unrestricted;
//! - `use <name>` binds a sibling module of the *package root* (the entry
//!   file's directory), file-scoped;
//! - `pub` / `pub(pkg)` gate cross-module access — cheap, and refusing to
//!   honour it would make a private call silently succeed, which is worse than
//!   declining to run it;
//! - item signatures are taken at **face value** (D27): nothing here checks a
//!   type.
//!
//! The E03xx module-law family lives here too, since is06. is02 deliberately
//! deferred it; the first corpus differential (is05) filed DIV-2026-002..005
//! against that posture — this machine claims the `resolve` rung on the phase
//! ladder, and `[proto.record.phase]` makes claiming a rung mean *performing*
//! it. So [`resolve_check`] now enforces the D32 module-graph laws the rung
//! owns: import cycles (E0303, `[mod.cycle]`), duplicate definitions (E0302,
//! `[mod.dup]`), cross-module private access (E0304, `[mod.vis.private]`) and
//! unused imports (E0305, `[mod.use.unused]`). Everything else the checker
//! owns (types, arity, exhaustiveness) still is not here, and using it is
//! still `unsupported`, never a guess.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::ast::{
    Block, Expr, ExprKind, FnDecl, Item, ItemKind, PatKind, Pattern, Stmt, StmtKind, StrLit,
    StrPart, StructDef, Type, TypeArg, TypeKind, Unit,
};
use crate::diag::{Diag, Span};

/// What a name in a module denotes.
#[derive(Debug, Clone)]
pub enum Def {
    Fn(Box<FnDecl>),
    Struct(Box<StructDef>),
    /// An item-level `let`/`var`/`const`, evaluated once at program start.
    Binding(Box<crate::ast::Binding>),
    /// A type alias, an enum, a trait, an impl — recorded so the name resolves,
    /// with no semantics attached at is02.
    Opaque(&'static str),
    /// Defined more than once in the same module (D32: file boundaries create
    /// no scopes). Dispatch is ambiguous, so using it is `unsupported`.
    Ambiguous,
}

/// A `use` decl as one file wrote it, with the spans E0303/E0305 report.
#[derive(Debug, Clone)]
pub struct UseRef {
    /// The name the decl binds (alias, list item, or last path segment).
    pub name: String,
    /// The bound name's own ident span — E0305's primary span.
    pub name_span: Span,
    /// The whole decl through its line terminator — E0303's primary span
    /// (the counterparty spans the statement including its newline, observed
    /// through the spec/06 protocol at pin 67c977f).
    pub decl_span: Span,
}

/// A dotted-path reference as one file wrote it: `alpha.ping`, `vault.secret`.
#[derive(Debug, Clone)]
pub struct PathRef {
    pub head: String,
    /// The second segment, when there is one — the cross-module member E0304
    /// reports, at its own ident span.
    pub tail: Option<(String, Span)>,
}

/// What one source file contributes to the module-law checks. D32 makes `use`
/// **file-scoped**, so usage is judged per file, not per module.
#[derive(Debug, Clone, Default)]
pub struct FileScope {
    pub file: String,
    pub uses: Vec<UseRef>,
    pub refs: Vec<PathRef>,
}

/// A second definition of a name in one module (D32: file boundaries create no
/// scopes). E0302's primary span is the *second* definition site.
#[derive(Debug, Clone)]
pub struct DupDef {
    pub name: String,
    pub again: Span,
    pub file: String,
}

/// One module: a directory's worth of `.lu` files.
#[derive(Debug, Clone, Default)]
pub struct Module {
    pub name: String,
    /// Name → definition, plus whether the definition is visible outside the
    /// module (`pub` or `pub(pkg)`).
    pub items: BTreeMap<String, (Def, bool)>,
    /// Item-level bindings in declaration order, so program start can evaluate
    /// them in the order they were written.
    pub bindings: Vec<String>,
    /// Modules this one `use`s, in declaration order.
    pub uses: Vec<String>,
    /// Per-file `use` decls and references, for [`resolve_check`].
    pub scopes: Vec<FileScope>,
    /// Duplicate definitions, in collection order.
    pub dups: Vec<DupDef>,
    /// Headers this module pulled in with `import c "…"` (D17). The importer
    /// is c10's; what `resolve` needs is that the name `c` is bound, so
    /// `c.malloc` resolves to a namespace rather than to nothing.
    pub c_headers: Vec<String>,
}

/// A whole program: the root module plus every module reachable through `use`.
#[derive(Debug, Clone)]
pub struct Program {
    /// `""` is the root module — the entry file's directory.
    pub modules: BTreeMap<String, Module>,
    /// Corpus-relative path of the entry file, for diagnostics.
    pub entry: String,
    /// Files that were loaded, in load order.
    pub files: Vec<String>,
}

impl Program {
    #[must_use]
    pub fn root(&self) -> &Module {
        self.modules.get("").expect("the root module always exists")
    }

    /// Did `module` pull a C header in with `import c "…"`?
    ///
    /// The name `c` is bound by the import (D17: "`import c` pulls a real
    /// header through the importer"), and the interpreter needs exactly that
    /// much: whether `c.malloc` names the C namespace or nothing at all.
    #[must_use]
    pub fn imports_c(&self, module: &str) -> bool {
        self.modules
            .get(module)
            .is_some_and(|module| !module.c_headers.is_empty())
    }

    /// Looks a name up in `module`, honouring visibility when the lookup comes
    /// from outside.
    #[must_use]
    pub fn lookup(&self, module: &str, name: &str, from_outside: bool) -> Option<&Def> {
        let module = self.modules.get(module)?;
        let (def, visible) = module.items.get(name)?;
        if from_outside && !visible {
            return None;
        }
        Some(def)
    }
}

/// Why a program could not be loaded at all.
#[derive(Debug, Clone)]
pub enum LoadError {
    /// A file in the program failed to lex or parse. Carries the file and the
    /// diagnostic — the record reports `fail(CODE)` at the phase that failed.
    Syntax { file: String, diag: Box<Diag> },
    /// The entry file, or a directory the program needs, could not be read.
    Io(String),
}

/// Loads the program rooted at `entry`.
///
/// # Errors
///
/// [`LoadError::Syntax`] when any file of the program fails the frontend, and
/// [`LoadError::Io`] when a file the program names cannot be read.
pub fn load(entry: &Path) -> Result<Program, LoadError> {
    // A bare filename (`lupin hello.lu` from the program's own directory)
    // has `parent() == Some("")`; the package root is the current directory.
    let package_root = match entry.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let mut program = Program {
        modules: BTreeMap::new(),
        entry: crate::slash_path(entry),
        files: Vec::new(),
    };

    let mut queue = vec![(
        String::new(),
        package_root.clone(),
        Some(entry.to_path_buf()),
    )];
    let mut loaded: Vec<String> = Vec::new();

    while let Some((name, dir, entry)) = queue.pop() {
        if loaded.contains(&name) {
            // An import cycle is a compile error the compiler owns (E0303).
            // Here it is simply a module already loaded — the graph is walked
            // once, so a cycle terminates instead of recursing forever.
            continue;
        }
        loaded.push(name.clone());

        let module = load_module(&name, &dir, entry.as_deref(), &mut program.files)?;
        for used in &module.uses {
            let candidate = package_root.join(used);
            if candidate.is_dir() && !loaded.contains(used) {
                queue.push((used.clone(), candidate, None));
            }
        }
        program.modules.insert(name, module);
    }

    Ok(program)
}

/// Loads one program from a single source buffer, with no filesystem module
/// graph. The shape unit tests and `--eval` want.
///
/// # Errors
///
/// [`LoadError::Syntax`] if the source does not lex or parse.
pub fn load_source(name: &str, source: &str) -> Result<Program, LoadError> {
    let parsed = crate::parse::parse_source(source).map_err(|diag| LoadError::Syntax {
        file: name.to_owned(),
        diag: Box::new(diag),
    })?;
    let mut module = Module {
        name: String::new(),
        ..Module::default()
    };
    collect(&parsed.unit, &mut module, name, source);
    let mut modules = BTreeMap::new();
    modules.insert(String::new(), module);
    Ok(Program {
        modules,
        entry: name.to_owned(),
        files: vec![name.to_owned()],
    })
}

fn load_module(
    name: &str,
    dir: &Path,
    entry: Option<&Path>,
    files: &mut Vec<String>,
) -> Result<Module, LoadError> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| LoadError::Io(format!("{}: {e}", dir.display())))?
        .flatten()
        .map(|path| path.path())
        .filter(|path| {
            // The entry belongs to its own program whatever it is named —
            // `lupin run repl` runs a file literally named `repl` (is12's
            // collision rule); everything else needs the `.lu` extension.
            path.is_file()
                && (path.extension().is_some_and(|e| e == "lu")
                    || entry.is_some_and(|entry| same_file(entry, path)))
        })
        .collect();
    // `read_dir` order is platform noise; D32 says the files are one module, so
    // the order must not change what the module contains.
    paths.sort();

    let mut module = Module {
        name: name.to_owned(),
        ..Module::default()
    };
    for path in paths {
        let source = std::fs::read_to_string(&path)
            .map_err(|e| LoadError::Io(format!("{}: {e}", path.display())))?;
        if !belongs(&path, entry, &source) {
            continue;
        }
        let parsed = crate::parse::parse_source(&source).map_err(|diag| LoadError::Syntax {
            file: crate::slash_path(&path),
            diag: Box::new(diag),
        })?;
        let display = crate::slash_path(&path);
        files.push(display.clone());
        collect(&parsed.unit, &mut module, &display, &source);
    }
    Ok(module)
}

/// Whether a `.lu` file in the module's directory is part of *this* program.
///
/// D32 says a directory is one module, and for an ordinary package that is the
/// end of it. The corpus is not an ordinary package: `corpus/` holds dozens of
/// independent single-file programs side by side, and reading them as one
/// module would make every one of them define `main` twice.
/// `[conf.directive.member]` is the discriminator the corpus itself supplies:
///
/// > `member: true` marks a file compiled through its directory's entry file
/// > (directory = module; the package root is the entry file's directory) —
/// > never conform-run directly. […] an entry file must carry both
/// > [`check:` and `phase:`].
///
/// So a sibling belongs iff it is the entry itself, or it declares itself a
/// member, or it carries no directive header at all (an ordinary package's
/// file, which is the non-corpus case).
fn belongs(path: &Path, entry: Option<&Path>, source: &str) -> bool {
    if entry.is_some_and(|entry| same_file(entry, path)) {
        return true;
    }
    match crate::directive::parse_header(source) {
        // A well-formed entry header next door: a different program.
        Ok(directives) => {
            directives.member || (directives.check.is_none() && directives.phase.is_none())
        }
        // No legible header: an ordinary source file of this module.
        Err(_) => true,
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn collect(unit: &Unit, module: &mut Module, file: &str, source: &str) {
    let mut scope = FileScope {
        file: file.to_owned(),
        ..FileScope::default()
    };
    for item in &unit.items {
        let visible = item.visibility.is_some();
        collect_item_refs(item, &mut scope);
        match &item.kind {
            ItemKind::Fn(decl) => {
                define(
                    module,
                    decl.name.name.clone(),
                    Def::Fn(decl.clone()),
                    visible,
                    Some(decl.name.span),
                    file,
                );
            }
            ItemKind::Struct(def) => {
                if let Some(name) = &def.name {
                    define(
                        module,
                        name.name.clone(),
                        Def::Struct(def.clone()),
                        // A type is usable wherever its module is: struct
                        // literals in the corpus cross no module boundary, and
                        // hiding a type behind `pub` would only produce a
                        // spurious `unsupported`.
                        true,
                        Some(name.span),
                        file,
                    );
                }
            }
            ItemKind::Binding(binding) => {
                if let crate::ast::PatKind::Binding(ident) = &*binding.pattern.kind {
                    define(
                        module,
                        ident.name.clone(),
                        Def::Binding(binding.clone()),
                        visible,
                        Some(ident.span),
                        file,
                    );
                    module.bindings.push(ident.name.clone());
                }
            }
            ItemKind::TypeAlias(alias) => {
                // `type Name = struct { … }` defines a constructible type.
                match &alias.def {
                    crate::ast::TypeDef::Struct(def) => define(
                        module,
                        alias.name.name.clone(),
                        Def::Struct(def.clone()),
                        true,
                        Some(alias.name.span),
                        file,
                    ),
                    _ => define(
                        module,
                        alias.name.name.clone(),
                        Def::Opaque("type"),
                        true,
                        Some(alias.name.span),
                        file,
                    ),
                }
            }
            ItemKind::Enum(def) => {
                if let Some(name) = &def.name {
                    define(
                        module,
                        name.name.clone(),
                        Def::Opaque("enum"),
                        true,
                        Some(name.span),
                        file,
                    );
                }
            }
            ItemKind::Trait(def) => {
                define(
                    module,
                    def.name.name.clone(),
                    Def::Opaque("trait"),
                    true,
                    Some(def.name.span),
                    file,
                );
            }
            ItemKind::Impl(_) => {}
            ItemKind::Use(decl) => collect_use(decl, module, &mut scope, source),
            ItemKind::ImportC(header) => {
                let name = header
                    .parts
                    .iter()
                    .filter_map(|part| match part {
                        crate::ast::StrPart::Text(text) => Some(text.as_str()),
                        crate::ast::StrPart::Interp(_) => None,
                    })
                    .collect::<String>();
                if !module.c_headers.contains(&name) {
                    module.c_headers.push(name);
                }
            }
        }
    }
    module.scopes.push(scope);
}

fn collect_use(
    decl: &crate::ast::UseDecl,
    module: &mut Module,
    scope: &mut FileScope,
    source: &str,
) {
    // The decl through its line terminator — the counterparty's E0303 span
    // shape, observed at pin 67c977f: `[18,28]` on `use alpha\n`.
    let end = source
        .get(decl.span.end..)
        .and_then(|rest| rest.find('\n'))
        .map_or(source.len(), |at| decl.span.end + at + 1);
    let decl_span = Span::new(decl.span.start, end);
    let mut bind = |name: String, name_span: Span, module: &mut Module| {
        scope.uses.push(UseRef {
            name: name.clone(),
            name_span,
            decl_span,
        });
        module.uses.push(name);
    };
    // `use std.fs` and friends name the ambient prelude, not a directory; the
    // loader tries the directory and simply finds nothing.
    let head = decl.path.segments.first().cloned();
    if decl.list.is_empty() {
        let bound = decl.alias.as_ref().map_or_else(
            || decl.path.segments.last().cloned(),
            |alias| Some(alias.clone()),
        );
        if let Some(bound) = bound {
            bind(bound.name.clone(), bound.span, module);
        }
    } else {
        for item in &decl.list {
            bind(item.name.clone(), item.span, module);
        }
    }
    if let Some(head) = head
        && !module.uses.contains(&head.name)
    {
        module.uses.push(head.name);
    }
}

fn define(
    module: &mut Module,
    name: String,
    def: Def,
    visible: bool,
    span: Option<Span>,
    file: &str,
) {
    match module.items.entry(name.clone()) {
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert((def, visible));
        }
        std::collections::btree_map::Entry::Occupied(mut slot) => {
            // D32: every `.lu` file in a directory is ONE module, so a second
            // definition is a duplicate, not a shadow. The compiler rejects it
            // (E0302); [`resolve_check`] reports it at the resolve rung, and
            // the ambiguity marker keeps dispatch honest for callers that
            // bypass the check (`load` consumers running a single buffer).
            slot.insert((Def::Ambiguous, visible));
            if let Some(again) = span {
                module.dups.push(DupDef {
                    name,
                    again,
                    file: file.to_owned(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The resolve rung: D32's module laws (E0302–E0305)
// ---------------------------------------------------------------------------

/// Runs the module-law checks the `resolve` rung owns and returns the first
/// failure as a protocol-shaped diagnostic.
///
/// Check order is fixed and deterministic: cycle (E0303), duplicate (E0302),
/// private access (E0304), unused import (E0305). Each corpus law file
/// exercises exactly one; a program violating several reports the first in
/// this order, which is a defensible choice the spec does not pin.
#[must_use]
pub fn resolve_check(program: &Program) -> Option<Diag> {
    cycle_check(program)
        .or_else(|| dup_check(program))
        .or_else(|| private_check(program))
        .or_else(|| unused_check(program))
}

/// `[mod.cycle]` (D32): imports form a DAG. E0303 at the `use` that closes
/// the cycle, found by depth-first walk from the root in declaration order.
fn cycle_check(program: &Program) -> Option<Diag> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        White,
        Gray,
        Black,
    }
    fn walk(
        program: &Program,
        name: &str,
        marks: &mut BTreeMap<String, Mark>,
        path: &mut Vec<String>,
    ) -> Option<Diag> {
        marks.insert(name.to_owned(), Mark::Gray);
        path.push(name.to_owned());
        let module = program.modules.get(name)?;
        for scope in &module.scopes {
            for used in &scope.uses {
                if !program.modules.contains_key(&used.name) {
                    // The ambient prelude (`use std.fs`), not a directory.
                    continue;
                }
                match marks.get(&used.name).copied().unwrap_or(Mark::White) {
                    Mark::Gray => {
                        // The back-edge: this `use` closes the cycle.
                        let start = path
                            .iter()
                            .position(|at| at == &used.name)
                            .unwrap_or_default();
                        let cycle = path[start..]
                            .iter()
                            .map(|at| format!("`{at}`"))
                            .collect::<Vec<_>>()
                            .join(" → ");
                        return Some(Diag::new(
                            "E0303",
                            used.decl_span,
                            "mod.cycle",
                            format!(
                                "this import completes a cycle: {cycle} → `{}` (in `{}`); \
                                 imports between modules must form a DAG (D32)",
                                used.name, scope.file
                            ),
                        ));
                    }
                    Mark::Black => {}
                    Mark::White => {
                        if let Some(diag) = walk(program, &used.name, marks, path) {
                            return Some(diag);
                        }
                    }
                }
            }
        }
        path.pop();
        marks.insert(name.to_owned(), Mark::Black);
        None
    }
    let mut marks = BTreeMap::new();
    let mut path = Vec::new();
    walk(program, "", &mut marks, &mut path)
}

/// `[mod.dup]` (D32): every `.lu` file in a directory is one module, so a
/// second definition is a duplicate, not a shadow. E0302 at the second site.
fn dup_check(program: &Program) -> Option<Diag> {
    for module in program.modules.values() {
        if let Some(dup) = module.dups.first() {
            return Some(Diag::new(
                "E0302",
                dup.again,
                "mod.dup",
                format!(
                    "the name `{}` is defined twice in this module (defined again in `{}`); \
                     file boundaries create no scopes (D32)",
                    dup.name, dup.file
                ),
            ));
        }
    }
    None
}

/// `[mod.vis.private]` (D32): private is the default; only `pub`/`pub(pkg)`
/// items are visible across modules. E0304 at the referencing member ident.
fn private_check(program: &Program) -> Option<Diag> {
    for module in program.modules.values() {
        for scope in &module.scopes {
            for reference in &scope.refs {
                let Some((member, member_span)) = &reference.tail else {
                    continue;
                };
                if reference.head == module.name {
                    continue;
                }
                let Some(target) = program.modules.get(&reference.head) else {
                    continue;
                };
                if let Some((_, visible)) = target.items.get(member)
                    && !visible
                {
                    return Some(Diag::new(
                        "E0304",
                        *member_span,
                        "mod.vis.private",
                        format!(
                            "`{member}` exists in `{}`, but it is private; only `pub`/`pub(pkg)` \
                             items are visible across modules (D32)",
                            reference.head
                        ),
                    ));
                }
            }
        }
    }
    None
}

/// `[mod.use.unused]` (D32): an unused import is a hard error, not a lint.
/// `use` is file-scoped, so usage is judged per file. E0305 at the bound name.
fn unused_check(program: &Program) -> Option<Diag> {
    for module in program.modules.values() {
        for scope in &module.scopes {
            for used in &scope.uses {
                if !program.modules.contains_key(&used.name) {
                    // The ambient prelude: not a module this loader resolved,
                    // so no law this rung owns speaks about it.
                    continue;
                }
                let referenced = scope
                    .refs
                    .iter()
                    .any(|reference| reference.head == used.name);
                if !referenced {
                    return Some(Diag::new(
                        "E0305",
                        used.name_span,
                        "mod.use.unused",
                        format!(
                            "the import `{}` is never used in `{}`; an unused import is a hard \
                             error (D32), and deleting the line is machine-applicable",
                            used.name, scope.file
                        ),
                    ));
                }
            }
        }
    }
    None
}

// -- the reference walker ---------------------------------------------------

fn collect_item_refs(item: &Item, scope: &mut FileScope) {
    match &item.kind {
        ItemKind::Fn(decl) => collect_fn_refs(decl, scope),
        ItemKind::Binding(binding) => {
            if let Some(ty) = &binding.ty {
                collect_type_refs(ty, scope);
            }
            collect_expr_refs(&binding.value, scope);
        }
        ItemKind::TypeAlias(alias) => {
            if let crate::ast::TypeDef::Alias(ty) = &alias.def {
                collect_type_refs(ty, scope);
            }
        }
        ItemKind::Struct(def) => {
            for field in &def.fields {
                collect_type_refs(&field.ty, scope);
            }
        }
        ItemKind::Enum(def) => {
            for variant in &def.variants {
                for ty in &variant.payload {
                    collect_type_refs(ty, scope);
                }
            }
        }
        ItemKind::Trait(def) => {
            for member in &def.members {
                collect_item_refs(member, scope);
            }
        }
        ItemKind::Impl(def) => {
            collect_type_refs(&def.trait_or_subject, scope);
            if let Some(subject) = &def.subject {
                collect_type_refs(subject, scope);
            }
            for member in &def.members {
                collect_item_refs(member, scope);
            }
        }
        ItemKind::Use(_) | ItemKind::ImportC(_) => {}
    }
}

fn collect_fn_refs(decl: &FnDecl, scope: &mut FileScope) {
    for param in &decl.params {
        if let crate::ast::ParamKind::Named { ty, .. } = &param.kind {
            collect_type_refs(ty, scope);
        }
    }
    if let Some(ret) = &decl.ret {
        collect_type_refs(&ret.ty, scope);
    }
    if let Some(body) = &decl.body {
        collect_block_refs(body, scope);
    }
}

fn collect_block_refs(block: &Block, scope: &mut FileScope) {
    for stmt in &block.stmts {
        collect_stmt_refs(stmt, scope);
    }
    if let Some(tail) = &block.tail {
        collect_expr_refs(tail, scope);
    }
}

fn collect_stmt_refs(stmt: &Stmt, scope: &mut FileScope) {
    match &stmt.kind {
        StmtKind::Binding(binding) => {
            collect_pattern_refs(&binding.pattern, scope);
            if let Some(ty) = &binding.ty {
                collect_type_refs(ty, scope);
            }
            collect_expr_refs(&binding.value, scope);
        }
        StmtKind::Assign { place, value, .. } => {
            collect_expr_refs(place, scope);
            collect_expr_refs(value, scope);
        }
        StmtKind::Defer { expr, .. } => collect_expr_refs(expr, scope),
        StmtKind::AssumeNoalias(operands) => {
            for operand in operands {
                collect_expr_refs(operand, scope);
            }
        }
        StmtKind::Expr(expr) => collect_expr_refs(expr, scope),
        StmtKind::Item(item) => collect_item_refs(item, scope),
    }
}

fn collect_path_ref(path: &crate::ast::Path, scope: &mut FileScope) {
    let Some(head) = path.segments.first() else {
        return;
    };
    scope.refs.push(PathRef {
        head: head.name.clone(),
        tail: path
            .segments
            .get(1)
            .map(|segment| (segment.name.clone(), segment.span)),
    });
}

fn collect_strlit_refs(literal: &StrLit, scope: &mut FileScope) {
    for part in &literal.parts {
        if let StrPart::Interp(interp) = part {
            collect_expr_refs(&interp.expr, scope);
            if let Some(parts) = &interp.format {
                for fmt in parts {
                    if let crate::ast::FmtPart::Interp(expr) = fmt {
                        collect_expr_refs(expr, scope);
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn collect_expr_refs(expr: &Expr, scope: &mut FileScope) {
    match &*expr.kind {
        ExprKind::Path(path) => collect_path_ref(path, scope),
        ExprKind::StructLit { path, fields } => {
            collect_path_ref(path, scope);
            for field in fields {
                if let Some(value) = &field.value {
                    collect_expr_refs(value, scope);
                }
            }
        }
        ExprKind::Str(literal) => collect_strlit_refs(literal, scope),
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Wildcard => {}
        ExprKind::Tuple(items) => {
            for item in items {
                collect_expr_refs(item, scope);
            }
        }
        ExprKind::Group(inner)
        | ExprKind::Try(inner)
        | ExprKind::FromEnd(inner)
        | ExprKind::Freeze(inner) => collect_expr_refs(inner, scope),
        ExprKind::Block(block) | ExprKind::Loop { body: block } => {
            collect_block_refs(block, scope);
        }
        ExprKind::Unary { operand, .. } => collect_expr_refs(operand, scope),
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_refs(lhs, scope);
            collect_expr_refs(rhs, scope);
        }
        ExprKind::Cast { expr: inner, ty } => {
            collect_expr_refs(inner, scope);
            collect_type_refs(ty, scope);
        }
        ExprKind::Call { callee, args } => {
            collect_expr_refs(callee, scope);
            for arg in args {
                collect_expr_refs(&arg.expr, scope);
            }
        }
        ExprKind::BracketApply { base, args } => {
            collect_expr_refs(base, scope);
            for arg in args {
                match arg {
                    crate::ast::IndexArg::Value(arg) => collect_expr_refs(&arg.expr, scope),
                    crate::ast::IndexArg::Type(ty) => collect_type_refs(ty, scope),
                }
            }
        }
        ExprKind::Member { base, .. } => collect_expr_refs(base, scope),
        ExprKind::ModedReceiver { place, .. } => collect_expr_refs(place, scope),
        ExprKind::Range { start, end, .. } => {
            if let Some(start) = start {
                collect_expr_refs(start, scope);
            }
            if let Some(end) = end {
                collect_expr_refs(end, scope);
            }
        }
        ExprKind::ElseDefault {
            expr: inner,
            handler,
        } => {
            collect_expr_refs(inner, scope);
            match &**handler {
                crate::ast::ElseHandler::Block(block) => collect_block_refs(block, scope),
                crate::ast::ElseHandler::Expr(expr) => collect_expr_refs(expr, scope),
                crate::ast::ElseHandler::Handler { pattern, body } => {
                    collect_pattern_refs(pattern, scope);
                    collect_expr_refs(body, scope);
                }
            }
        }
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            collect_expr_refs(cond, scope);
            collect_block_refs(then, scope);
            if let Some(otherwise) = otherwise {
                collect_expr_refs(otherwise, scope);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_expr_refs(scrutinee, scope);
            for arm in arms {
                collect_pattern_refs(&arm.pattern, scope);
                if let Some(guard) = &arm.guard {
                    collect_expr_refs(guard, scope);
                }
                collect_expr_refs(&arm.body, scope);
            }
        }
        ExprKind::For {
            pattern,
            iter,
            body,
        } => {
            collect_pattern_refs(pattern, scope);
            collect_expr_refs(iter, scope);
            collect_block_refs(body, scope);
        }
        ExprKind::While { cond, body } => {
            collect_expr_refs(cond, scope);
            collect_block_refs(body, scope);
        }
        ExprKind::Return(value) | ExprKind::Break(value) => {
            if let Some(value) = value {
                collect_expr_refs(value, scope);
            }
        }
        ExprKind::Continue => {}
        ExprKind::Closure { body, .. } => collect_expr_refs(body, scope),
        ExprKind::RegionSugar { body, .. } => collect_block_refs(body, scope),
        ExprKind::RegionValue { .. } => {}
        ExprKind::In { region, body } => {
            collect_expr_refs(region, scope);
            collect_block_refs(body, scope);
        }
        ExprKind::Scope { body, .. } => collect_block_refs(body, scope),
        ExprKind::SpawnProc { path, args } => {
            collect_path_ref(path, scope);
            for arg in args {
                collect_expr_refs(&arg.expr, scope);
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.kind {
                    crate::ast::SelectArmKind::Recv { pattern, channel } => {
                        collect_pattern_refs(pattern, scope);
                        collect_expr_refs(channel, scope);
                    }
                    crate::ast::SelectArmKind::Timeout(expr) => collect_expr_refs(expr, scope),
                }
                collect_expr_refs(&arm.body, scope);
            }
        }
        ExprKind::When { operands, body } => {
            for operand in operands {
                collect_expr_refs(operand, scope);
            }
            collect_block_refs(body, scope);
        }
        ExprKind::Unsafe { body } => collect_block_refs(body, scope),
        ExprKind::UnsafeC { .. } => {}
        ExprKind::Asm { template, operands } => {
            collect_strlit_refs(template, scope);
            for operand in operands {
                collect_expr_refs(&operand.value, scope);
            }
        }
        ExprKind::Borrow { place, from } => {
            collect_expr_refs(place, scope);
            collect_expr_refs(from, scope);
        }
    }
}

fn collect_pattern_refs(pattern: &Pattern, scope: &mut FileScope) {
    match &*pattern.kind {
        PatKind::Wildcard | PatKind::Binding(_) => {}
        PatKind::Literal(expr) => collect_expr_refs(expr, scope),
        // Pattern paths mark their head *used* (E0305) but are not E0304
        // candidates: a dotted error tag (`io.Error`) is structural (D30) and
        // never a module member access.
        PatKind::Variant { path, fields } => {
            if let Some(head) = path.segments.first() {
                scope.refs.push(PathRef {
                    head: head.name.clone(),
                    tail: None,
                });
            }
            for field in fields {
                collect_pattern_refs(field, scope);
            }
        }
        PatKind::Path(path) => {
            if let Some(head) = path.segments.first() {
                scope.refs.push(PathRef {
                    head: head.name.clone(),
                    tail: None,
                });
            }
        }
        PatKind::Tuple(items) => {
            for item in items {
                collect_pattern_refs(item, scope);
            }
        }
        PatKind::At { pattern, .. } => collect_pattern_refs(pattern, scope),
        PatKind::Or(alternatives) => {
            for alternative in alternatives {
                collect_pattern_refs(alternative, scope);
            }
        }
    }
}

fn collect_type_refs(ty: &Type, scope: &mut FileScope) {
    match &*ty.kind {
        TypeKind::Path { path, args } => {
            // A type reference marks its head used but is not an E0304
            // candidate here: type visibility is the checker's half.
            if let Some(head) = path.segments.first() {
                scope.refs.push(PathRef {
                    head: head.name.clone(),
                    tail: None,
                });
            }
            for arg in args {
                match arg {
                    TypeArg::Type(inner) => collect_type_refs(inner, scope),
                    TypeArg::Expr(expr) => collect_expr_refs(expr, scope),
                }
            }
        }
        TypeKind::ErrorUnion(inner)
        | TypeKind::Prefixed { ty: inner, .. }
        | TypeKind::RawPointer(inner) => collect_type_refs(inner, scope),
        TypeKind::Dyn(path) => {
            if let Some(head) = path.segments.first() {
                scope.refs.push(PathRef {
                    head: head.name.clone(),
                    tail: None,
                });
            }
        }
        TypeKind::Tuple(items) => {
            for item in items {
                collect_type_refs(item, scope);
            }
        }
        TypeKind::Fn { params, ret } => {
            for param in params {
                collect_type_refs(param, scope);
            }
            if let Some(ret) = ret {
                collect_type_refs(&ret.ty, scope);
            }
        }
        TypeKind::TypeOfTypes | TypeKind::Region => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_file_program_has_a_root_module() {
        let program = load_source("t.lu", "fn main() -> int { 0 }\n").expect("loads");
        assert!(matches!(
            program.root().items.get("main"),
            Some((Def::Fn(_), _))
        ));
    }

    #[test]
    fn a_duplicate_definition_becomes_ambiguous_rather_than_an_error() {
        let program = load_source(
            "t.lu",
            "fn helper() -> int { 1 }\nfn helper() -> int { 2 }\nfn main() -> int { 0 }\n",
        )
        .expect("loads");
        assert!(matches!(
            program.root().items.get("helper"),
            Some((Def::Ambiguous, _))
        ));
    }

    #[test]
    fn visibility_gates_cross_module_lookup_only() {
        let program = load_source(
            "t.lu",
            "fn secret() -> int { 1 }\npub fn open() -> int { 2 }\n",
        )
        .expect("loads");
        assert!(program.lookup("", "secret", false).is_some());
        assert!(program.lookup("", "secret", true).is_none());
        assert!(program.lookup("", "open", true).is_some());
    }

    #[test]
    fn item_level_bindings_are_recorded_in_declaration_order() {
        let program =
            load_source("t.lu", "let A = 1\nlet B = 2\nfn main() -> int { 0 }\n").expect("loads");
        assert_eq!(
            program.root().bindings,
            vec!["A".to_owned(), "B".to_owned()]
        );
    }

    #[test]
    fn a_syntax_error_is_a_load_error_that_names_the_file() {
        let err = load_source(
            "t.lu",
            "fn main() -> int {\n    let a = 1\n        + 2\n    0\n}\n",
        )
        .expect_err("rejects");
        let LoadError::Syntax { file, diag } = err else {
            panic!("expected a syntax error")
        };
        assert_eq!(file, "t.lu");
        assert_eq!(diag.code, crate::diag::E_LEADING_OPERATOR);
    }
}

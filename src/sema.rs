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
    /// Variant name → the enums of this module that declare it, in declaration
    /// order. What makes a bare identifier in a pattern a *variant pattern*
    /// rather than a binding (`[gram.pat]`; the checker resolves an in-scope
    /// variant name to its case — issue #5, wolf-std F-0007).
    pub variants: BTreeMap<String, Vec<String>>,
    /// Each `use` decl's bound name with its full dotted path, in declaration
    /// order — what lets the loader resolve `use std.x.deque_int` against a
    /// std root (issue #6, wolf-std F-0010) instead of only trying the flat
    /// `<package root>/<last segment>` directory.
    pub use_paths: Vec<(String, Vec<String>)>,
    /// Impl-block methods: subject type name → method name → every decl of
    /// that name, with the trait each impl names (`None` for an inherent
    /// impl). All of them are kept because the s17 resolution order is
    /// positional — the type's own impl wins over a trait's, and the
    /// trait-qualified form (`Speak.speak(d)`) reaches the shadowed one
    /// explicitly. `[mem.iter.for]` dispatch needs the trait too: user types
    /// implement `Iter` **by name** — no structural conformance.
    pub methods: BTreeMap<String, BTreeMap<String, Vec<MethodDef>>>,
    /// Every lowercase tag a function signature of this module declares in an
    /// error row (return rows and postfix rows alike). The pattern-resolution
    /// rule (issue #12, the interpreter half of wolf-lang#4): a lowercase
    /// identifier pattern over a tag-shaped scrutinee that names a *declared*
    /// tag is a row-tag pattern; everything else still binds.
    pub row_tags: std::collections::BTreeSet<String>,
}

/// One impl-block method: `impl Iter for RangeIter { fn next(…) }` records
/// `next → MethodDef { decl, trait_name: Some("Iter") }` under `RangeIter`.
#[derive(Debug, Clone)]
pub struct MethodDef {
    pub decl: Box<FnDecl>,
    pub trait_name: Option<String>,
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

/// Loads the program rooted at `entry`, resolving `use std.*` against the
/// `LUPIN_STD` environment variable when it is set.
///
/// # Errors
///
/// [`LoadError::Syntax`] when any file of the program fails the frontend, and
/// [`LoadError::Io`] when a file the program names cannot be read.
pub fn load(entry: &Path) -> Result<Program, LoadError> {
    load_with(entry, None)
}

/// As [`load`], with an explicit std root — `--std-root DIR`, falling back to
/// the `LUPIN_STD` environment variable (issue #6, wolf-std F-0010; the
/// mechanism mirrors the compiler's s26 `--std-root`/`WOLF_STD` loader).
///
/// `use std.X[.Y]` resolves the module directory `<root>/X[/Y]/` and binds it
/// under the path's last segment, exactly as the flat loader binds a sibling
/// directory. Without a root — or when the named directory does not exist —
/// the loader falls back to `<package root>/<bound name>`, which keeps flat
/// mirrors and ordinary sibling modules working unchanged.
///
/// # Errors
///
/// As [`load`].
pub fn load_with(entry: &Path, std_root: Option<&Path>) -> Result<Program, LoadError> {
    let env_root = std::env::var_os("LUPIN_STD").map(PathBuf::from);
    let std_root: Option<&Path> = std_root.or(env_root.as_deref());
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
        let mut queued: Vec<String> = Vec::new();
        for (bound, segments) in &module.use_paths {
            if loaded.contains(bound) || queued.contains(bound) {
                continue;
            }
            // `use std.X[.Y]` against a configured root: `<root>/X[/Y]/`.
            if let Some(root) = std_root
                && segments.len() >= 2
                && segments[0] == "std"
            {
                let mut candidate = root.to_path_buf();
                for segment in &segments[1..] {
                    candidate.push(segment);
                }
                if candidate.is_dir() {
                    queue.push((bound.clone(), candidate, None));
                    queued.push(bound.clone());
                    continue;
                }
            }
            let candidate = package_root.join(bound);
            if candidate.is_dir() {
                queue.push((bound.clone(), candidate, None));
                queued.push(bound.clone());
            }
        }
        // The heads of dotted `use` paths (`std` in `use std.prelude`) sit in
        // `module.uses` without a `use_paths` entry of their own; a sibling
        // directory by that name is still a module of this program.
        for used in &module.uses {
            let candidate = package_root.join(used);
            if candidate.is_dir() && !loaded.contains(used) && !queued.contains(used) {
                queue.push((used.clone(), candidate, None));
                queued.push(used.clone());
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

/// The head name of a type, for impl-subject and trait naming: the path's
/// last segment, through prefix keywords. `None` for shapes an impl subject
/// does not take at this machine's depth (tuples, fn types, …).
fn type_head_name(ty: &Type) -> Option<String> {
    match &*ty.kind {
        TypeKind::Path { path, .. } => path.segments.last().map(|s| s.name.clone()),
        TypeKind::Prefixed { ty, .. }
        | TypeKind::ErrorUnion(ty)
        | TypeKind::Fallible { ty, .. } => type_head_name(ty),
        _ => None,
    }
}

/// Collects every single-segment tag name a type's postfix rows declare —
/// `int ! {none}` yields `none`; rows nest ( `(int ! {none}) ! {stale}` ).
fn type_row_tags(ty: &Type, out: &mut Vec<String>) {
    match &*ty.kind {
        TypeKind::Fallible { ty, row } => {
            for entry in &row.entries {
                if let [segment] = entry.path.segments.as_slice() {
                    out.push(segment.name.clone());
                }
            }
            type_row_tags(ty, out);
        }
        TypeKind::ErrorUnion(inner)
        | TypeKind::Prefixed { ty: inner, .. }
        | TypeKind::RawPointer(inner) => type_row_tags(inner, out),
        TypeKind::Tuple(items) => {
            for item in items {
                type_row_tags(item, out);
            }
        }
        _ => {}
    }
}

/// The tags a `raise` inside `decl` may name: the declared *return* row —
/// `-> int ! {none}` in either spelling (`ret_type`'s own `'!' error_row` or
/// the postfix-row type). Parameter rows describe arguments, not raises.
#[must_use]
pub fn declared_raise_tags(decl: &FnDecl) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(ret) = &decl.ret {
        if let Some(row) = &ret.row {
            for entry in &row.entries {
                if let [segment] = entry.path.segments.as_slice() {
                    tags.push(segment.name.clone());
                }
            }
        }
        type_row_tags(&ret.ty, &mut tags);
    }
    tags
}

/// Every row tag a signature declares — return row and parameter rows alike —
/// into the module's pattern-resolution vocabulary.
fn collect_signature_tags(decl: &FnDecl, out: &mut std::collections::BTreeSet<String>) {
    out.extend(declared_raise_tags(decl));
    for param in &decl.params {
        if let crate::ast::ParamKind::Named { ty, .. } = &param.kind {
            let mut tags = Vec::new();
            type_row_tags(ty, &mut tags);
            out.extend(tags);
        }
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
                collect_signature_tags(decl, &mut module.row_tags);
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
                    for variant in &def.variants {
                        module
                            .variants
                            .entry(variant.name.name.clone())
                            .or_default()
                            .push(name.name.clone());
                    }
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
            ItemKind::Impl(def) => {
                // `impl X { … }` is an inherent impl; `impl Iter for X { … }`
                // implements the trait by name (`[gram.item.trait]`,
                // `[mem.iter.impl]`). Methods land under the subject type's
                // name; nothing here checks a type (D27 — face value).
                let subject = def.subject.as_ref().unwrap_or(&def.trait_or_subject);
                let trait_name = def
                    .subject
                    .is_some()
                    .then(|| type_head_name(&def.trait_or_subject))
                    .flatten();
                if let Some(subject) = type_head_name(subject) {
                    for member in &def.members {
                        if let ItemKind::Fn(decl) = &member.kind {
                            collect_signature_tags(decl, &mut module.row_tags);
                            module
                                .methods
                                .entry(subject.clone())
                                .or_default()
                                .entry(decl.name.name.clone())
                                .or_default()
                                .push(MethodDef {
                                    decl: decl.clone(),
                                    trait_name: trait_name.clone(),
                                });
                        }
                    }
                }
            }
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
    let path_segments: Vec<String> = decl.path.segments.iter().map(|s| s.name.clone()).collect();
    let mut bind = |name: String, name_span: Span, segments: Vec<String>, module: &mut Module| {
        scope.uses.push(UseRef {
            name: name.clone(),
            name_span,
            decl_span,
        });
        module.use_paths.push((name.clone(), segments));
        module.uses.push(name);
    };
    // `use std.fs` and friends name the ambient prelude when no std root is
    // configured — the loader tries the directories and simply finds nothing.
    // With a std root (issue #6), the recorded path resolves against it.
    let head = decl.path.segments.first().cloned();
    if decl.list.is_empty() {
        let bound = decl.alias.as_ref().map_or_else(
            || decl.path.segments.last().cloned(),
            |alias| Some(alias.clone()),
        );
        if let Some(bound) = bound {
            bind(bound.name.clone(), bound.span, path_segments, module);
        }
    } else {
        for item in &decl.list {
            let mut segments = path_segments.clone();
            segments.push(item.name.clone());
            bind(item.name.clone(), item.span, segments, module);
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
/// private access (E0304), unused import (E0305), `let` reassignment (E0410).
/// Each corpus law file exercises exactly one; a program violating several
/// reports the first in this order, which is a defensible choice the spec
/// does not pin.
#[must_use]
pub fn resolve_check(program: &Program) -> Option<Diag> {
    cycle_check(program)
        .or_else(|| dup_check(program))
        .or_else(|| private_check(program))
        .or_else(|| unused_check(program))
        .or_else(|| reassign_check(program))
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

// -- E0410: `let` reassignment (`[gram.item.let]`) --------------------------

/// `let` is immutable (spec/01 §2.4: "`let` immutable, `var` mutable"), and
/// assignment — plain or compound, `[gram.expr.assign]` routes both through
/// the same place rules — to a `let`-bound name is rejected here, at the
/// resolve rung this machine claims. E0410 with a `var` fix-it, the primary
/// span on the assigned place, matching the counterparty's shape (observed at
/// pin a0c4564: span is the place ident alone, for `+=` too). The interpreter
/// half of wolf-lang#2 (issue #8, wolf-std F-0017).
///
/// Scope discipline, per the corpus's own non-cases
/// (`typecheck/let_shadow_var_ok.lu`): a second `let x` *shadows* rather than
/// assigns; a `var` shadowing a `let` is assignable again; a parameter is not
/// a `let` binding (its mutability is the mode system's business); pattern
/// bindings in `match`/`for`/`else |pat|`/`select` arms are not `let`
/// bindings either. Only the latest binding of a name in scope speaks.
fn reassign_check(program: &Program) -> Option<Diag> {
    body_walk(program, false).0
}

/// The eager raise check (issue #12(c), the correction to wolf-std's sc02
/// claim): row-tag resolution used to be lazy — a `return none` on a branch
/// the input never took produced no diagnostic at all, so a verification that
/// only exercised the hit path certified a raise site that did not work. A
/// bare lowercase `return` name now resolves at the resolve rung: as a
/// binding in scope, a module-level name, or a tag of the enclosing
/// function's declared row — and an unresolvable one is a diagnostic about
/// the *program*, not a property of the input. The refusal is `unsupported`
/// (name resolution beyond the module laws is the checker's), never a guess.
pub fn raise_check(program: &Program) -> Option<String> {
    body_walk(program, true).1
}

fn body_walk(program: &Program, raises: bool) -> (Option<Diag>, Option<String>) {
    for module in program.modules.values() {
        // Module-level bindings are in scope in every function of the module.
        let mut globals: Vec<(String, bool)> = Vec::new();
        for name in &module.bindings {
            if let Some((Def::Binding(binding), _)) = module.items.get(name) {
                globals.push((name.clone(), binding.kind == crate::ast::BindingKind::Var));
            }
        }
        let known = raises.then(|| {
            let mut known: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            known.extend(module.items.keys().cloned());
            known.extend(program.modules.keys().cloned());
            known.extend(module.uses.iter().cloned());
            known.extend(module.variants.keys().cloned());
            for ambient in crate::eval::builtin::AMBIENT_NAMES {
                known.insert((*ambient).to_owned());
            }
            if !module.c_headers.is_empty() {
                known.insert("c".to_owned());
            }
            known
        });
        let decls = module
            .items
            .values()
            .filter_map(|(def, _)| match def {
                Def::Fn(decl) => Some(&**decl),
                _ => None,
            })
            .chain(
                module
                    .methods
                    .values()
                    .flat_map(|methods| methods.values().flatten().map(|m| &*m.decl)),
            );
        for decl in decls {
            let mut env = Env {
                scopes: vec![globals.clone()],
                raise: known.clone().map(|known| RaiseCtx {
                    row: Vec::new(),
                    known,
                    refusal: None,
                }),
            };
            let diag = walk_fn_assigns(decl, &mut env);
            if let Some(ctx) = env.raise
                && let Some(refusal) = ctx.refusal
            {
                return (None, Some(refusal));
            }
            if diag.is_some() {
                return (diag, None);
            }
        }
        for (def, _) in module.items.values() {
            if let Def::Binding(binding) = def {
                let mut env = Env {
                    scopes: vec![globals.clone()],
                    raise: None,
                };
                let diag = walk_expr_assigns(&binding.value, &mut env);
                if diag.is_some() {
                    return (diag, None);
                }
            }
        }
    }
    (None, None)
}

/// The lexical environment of the reassignment walk: name → is it assignable.
struct Env {
    scopes: Vec<Vec<(String, bool)>>,
    /// Present when the walk is also the eager raise check ([`raise_check`]):
    /// a `return <bare lowercase name>` must resolve *somewhere* — binding,
    /// item, or the enclosing function's declared row — at resolve time.
    raise: Option<RaiseCtx>,
}

/// What the eager raise check resolves a bare lowercase `return` name
/// against, beyond the lexical scopes the walk already tracks.
struct RaiseCtx {
    /// The enclosing function's declared return-row tags.
    row: Vec<String>,
    /// Module-level names: items, sibling modules, `use` bindings, enum
    /// variants, the ambient builtins.
    known: std::collections::BTreeSet<String>,
    /// The first unresolvable raise site, as the `unsupported` reason.
    refusal: Option<String>,
}

impl Env {
    fn declare(&mut self, name: &str, assignable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name.to_owned(), assignable));
        }
    }

    /// The latest binding of `name`, innermost scope first — shadowing means
    /// the most recent entry wins even within one scope.
    fn assignable(&self, name: &str) -> Option<bool> {
        self.scopes.iter().rev().find_map(|scope| {
            scope
                .iter()
                .rev()
                .find(|(n, _)| n == name)
                .map(|(_, assignable)| *assignable)
        })
    }
}

/// Declares every name a pattern binds. Pattern bindings are never `let`
/// bindings, so they are all assignable as far as this rule cares.
fn declare_pattern(pattern: &Pattern, assignable: bool, env: &mut Env) {
    match &*pattern.kind {
        PatKind::Wildcard | PatKind::Literal(_) => {}
        PatKind::Binding(ident) => env.declare(&ident.name, assignable),
        PatKind::Variant { fields, .. } => {
            for field in fields {
                declare_pattern(field, assignable, env);
            }
        }
        PatKind::Tuple(items) => {
            for item in items {
                declare_pattern(item, assignable, env);
            }
        }
        PatKind::At { name, pattern } => {
            env.declare(&name.name, assignable);
            declare_pattern(pattern, assignable, env);
        }
        PatKind::Or(alternatives) => {
            for alternative in alternatives {
                declare_pattern(alternative, assignable, env);
            }
        }
    }
}

fn walk_fn_assigns(decl: &FnDecl, env: &mut Env) -> Option<Diag> {
    let body = decl.body.as_ref()?;
    env.scopes.push(Vec::new());
    let outer_row = match &mut env.raise {
        Some(ctx) => Some(std::mem::replace(&mut ctx.row, declared_raise_tags(decl))),
        None => None,
    };
    for param in &decl.params {
        match &param.kind {
            crate::ast::ParamKind::Named { name, .. } => env.declare(&name.name, true),
            crate::ast::ParamKind::SelfParam { .. } => env.declare("self", true),
        }
    }
    let diag = walk_block_assigns(body, env);
    if let (Some(ctx), Some(row)) = (&mut env.raise, outer_row) {
        ctx.row = row;
    }
    env.scopes.pop();
    diag
}

fn walk_block_assigns(block: &Block, env: &mut Env) -> Option<Diag> {
    env.scopes.push(Vec::new());
    let mut diag = None;
    for stmt in &block.stmts {
        diag = walk_stmt_assigns(stmt, env);
        if diag.is_some() {
            break;
        }
    }
    if diag.is_none()
        && let Some(tail) = &block.tail
    {
        diag = walk_expr_assigns(tail, env);
    }
    env.scopes.pop();
    diag
}

fn walk_stmt_assigns(stmt: &Stmt, env: &mut Env) -> Option<Diag> {
    match &stmt.kind {
        StmtKind::Binding(binding) => {
            let diag = walk_expr_assigns(&binding.value, env);
            if diag.is_some() {
                return diag;
            }
            declare_pattern(
                &binding.pattern,
                binding.kind == crate::ast::BindingKind::Var,
                env,
            );
            None
        }
        StmtKind::Assign { place, value, .. } => {
            let diag = walk_expr_assigns(value, env).or_else(|| walk_expr_assigns(place, env));
            if diag.is_some() {
                return diag;
            }
            if let ExprKind::Path(path) = &*place.kind
                && path.is_single()
                && let Some(head) = path.segments.first()
                && env.assignable(&head.name) == Some(false)
            {
                return Some(Diag::new(
                    "E0410",
                    place.span,
                    "gram.item.let",
                    format!(
                        "`{}` is bound with `let`, so it cannot be assigned again; declare the \
                         binding with `var` to update it in place (machine-applicable), or shadow \
                         it with a second `let` if the next value is really a new thing",
                        head.name
                    ),
                ));
            }
            None
        }
        StmtKind::Defer { expr, .. } => walk_expr_assigns(expr, env),
        StmtKind::AssumeNoalias(operands) => {
            operands.iter().find_map(|op| walk_expr_assigns(op, env))
        }
        StmtKind::Expr(expr) => walk_expr_assigns(expr, env),
        StmtKind::Item(item) => match &item.kind {
            ItemKind::Fn(decl) => {
                let mut nested = Env {
                    scopes: vec![env.scopes.first().cloned().unwrap_or_default()],
                    raise: env.raise.take(),
                };
                let diag = walk_fn_assigns(decl, &mut nested);
                env.raise = nested.raise;
                diag
            }
            _ => None,
        },
    }
}

#[allow(clippy::too_many_lines)]
fn walk_expr_assigns(expr: &Expr, env: &mut Env) -> Option<Diag> {
    match &*expr.kind {
        ExprKind::Path(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Wildcard
        | ExprKind::Continue
        | ExprKind::RegionValue { .. }
        | ExprKind::UnsafeC { .. } => None,
        ExprKind::Str(literal) => literal.parts.iter().find_map(|part| match part {
            StrPart::Interp(interp) => walk_expr_assigns(&interp.expr, env),
            StrPart::Text(_) => None,
        }),
        ExprKind::StructLit { fields, .. } => fields
            .iter()
            .filter_map(|field| field.value.as_ref())
            .find_map(|value| walk_expr_assigns(value, env)),
        ExprKind::Tuple(items) => items.iter().find_map(|item| walk_expr_assigns(item, env)),
        ExprKind::Group(inner)
        | ExprKind::Try(inner)
        | ExprKind::FromEnd(inner)
        | ExprKind::Freeze(inner)
        | ExprKind::Unary { operand: inner, .. }
        | ExprKind::Cast { expr: inner, .. } => walk_expr_assigns(inner, env),
        ExprKind::Block(block)
        | ExprKind::Loop { body: block }
        | ExprKind::RegionSugar { body: block, .. }
        | ExprKind::Scope { body: block, .. }
        | ExprKind::Unsafe { body: block } => walk_block_assigns(block, env),
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr_assigns(lhs, env).or_else(|| walk_expr_assigns(rhs, env))
        }
        ExprKind::Call { callee, args } => walk_expr_assigns(callee, env).or_else(|| {
            args.iter()
                .find_map(|arg| walk_expr_assigns(&arg.expr, env))
        }),
        ExprKind::BracketApply { base, args } => walk_expr_assigns(base, env).or_else(|| {
            args.iter().find_map(|arg| match arg {
                crate::ast::IndexArg::Value(arg) => walk_expr_assigns(&arg.expr, env),
                crate::ast::IndexArg::Type(_) => None,
            })
        }),
        ExprKind::Member { base, .. } => walk_expr_assigns(base, env),
        ExprKind::ModedReceiver { place, .. } => walk_expr_assigns(place, env),
        ExprKind::Range { start, end, .. } => start
            .as_ref()
            .and_then(|s| walk_expr_assigns(s, env))
            .or_else(|| end.as_ref().and_then(|e| walk_expr_assigns(e, env))),
        ExprKind::ElseDefault {
            expr: inner,
            handler,
        } => walk_expr_assigns(inner, env).or_else(|| match &**handler {
            crate::ast::ElseHandler::Block(block) => walk_block_assigns(block, env),
            crate::ast::ElseHandler::Expr(expr) => walk_expr_assigns(expr, env),
            crate::ast::ElseHandler::Handler { pattern, body } => {
                env.scopes.push(Vec::new());
                declare_pattern(pattern, true, env);
                let diag = walk_expr_assigns(body, env);
                env.scopes.pop();
                diag
            }
        }),
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => walk_expr_assigns(cond, env)
            .or_else(|| walk_block_assigns(then, env))
            .or_else(|| otherwise.as_ref().and_then(|e| walk_expr_assigns(e, env))),
        ExprKind::Match { scrutinee, arms } => walk_expr_assigns(scrutinee, env).or_else(|| {
            arms.iter().find_map(|arm| {
                env.scopes.push(Vec::new());
                declare_pattern(&arm.pattern, true, env);
                let diag = arm
                    .guard
                    .as_ref()
                    .and_then(|guard| walk_expr_assigns(guard, env))
                    .or_else(|| walk_expr_assigns(&arm.body, env));
                env.scopes.pop();
                diag
            })
        }),
        ExprKind::For {
            pattern,
            iter,
            body,
        } => walk_expr_assigns(iter, env).or_else(|| {
            env.scopes.push(Vec::new());
            declare_pattern(pattern, true, env);
            let diag = walk_block_assigns(body, env);
            env.scopes.pop();
            diag
        }),
        ExprKind::While { cond, body } => {
            walk_expr_assigns(cond, env).or_else(|| walk_block_assigns(body, env))
        }
        ExprKind::Return(value) => {
            // The eager raise check (issue #12(c)): a bare lowercase `return`
            // name must resolve now — binding, module-level name, or a tag of
            // the enclosing function's declared row. Uppercase names are D30
            // structural tags and need no declaration (`[err.rows]`).
            if let Some(value) = value
                && let ExprKind::Path(path) = &*value.kind
                && let [segment] = path.segments.as_slice()
                && segment.name.starts_with(char::is_lowercase)
                && env.assignable(&segment.name).is_none()
                && let Some(ctx) = &mut env.raise
                && ctx.refusal.is_none()
                && !ctx.known.contains(&segment.name)
                && !ctx.row.contains(&segment.name)
            {
                ctx.refusal = Some(format!(
                    "`{}` does not resolve at this raise site: it is not a binding, not a \
                     module-level name, and not a tag of the enclosing function's declared row",
                    segment.name
                ));
            }
            value.as_ref().and_then(|v| walk_expr_assigns(v, env))
        }
        ExprKind::Break(value) => value.as_ref().and_then(|v| walk_expr_assigns(v, env)),
        ExprKind::Closure { params, body, .. } => {
            env.scopes.push(Vec::new());
            for param in params {
                env.declare(&param.name.name, true);
            }
            let diag = walk_expr_assigns(body, env);
            env.scopes.pop();
            diag
        }
        ExprKind::In { region, body } => {
            walk_expr_assigns(region, env).or_else(|| walk_block_assigns(body, env))
        }
        ExprKind::SpawnProc { args, .. } => args
            .iter()
            .find_map(|arg| walk_expr_assigns(&arg.expr, env)),
        ExprKind::Select { arms } => arms.iter().find_map(|arm| {
            env.scopes.push(Vec::new());
            let diag = match &arm.kind {
                crate::ast::SelectArmKind::Recv { pattern, channel } => {
                    let diag = walk_expr_assigns(channel, env);
                    declare_pattern(pattern, true, env);
                    diag
                }
                crate::ast::SelectArmKind::Timeout(expr) => walk_expr_assigns(expr, env),
            }
            .or_else(|| walk_expr_assigns(&arm.body, env));
            env.scopes.pop();
            diag
        }),
        ExprKind::When { operands, body } => {
            operands
                .iter()
                .find_map(|op| walk_expr_assigns(op, env))
                .or_else(|| {
                    // A `when` body addresses its operands *unlocked*
                    // (`[conc.when.body]`): `when (a, b) { a += 10 }` assigns
                    // through the acquired cell, not to the binding, so the
                    // operand names are assignable inside the body whatever
                    // introduced them (`corpus/conc/when_multi.lu` pins this).
                    env.scopes.push(Vec::new());
                    for op in operands {
                        // A single operand arrives grouped (`when (a) { … }`
                        // parses the parens as a group); unwrap to the place.
                        let mut place = op;
                        while let ExprKind::Group(inner) = &*place.kind {
                            place = inner;
                        }
                        if let ExprKind::Path(path) = &*place.kind
                            && path.is_single()
                            && let Some(head) = path.segments.first()
                        {
                            env.declare(&head.name, true);
                        }
                    }
                    let diag = walk_block_assigns(body, env);
                    env.scopes.pop();
                    diag
                })
        }
        ExprKind::Asm { template, operands } => template
            .parts
            .iter()
            .find_map(|part| match part {
                StrPart::Interp(interp) => walk_expr_assigns(&interp.expr, env),
                StrPart::Text(_) => None,
            })
            .or_else(|| {
                operands
                    .iter()
                    .find_map(|op| walk_expr_assigns(&op.value, env))
            }),
        ExprKind::Borrow { place, from } => {
            walk_expr_assigns(place, env).or_else(|| walk_expr_assigns(from, env))
        }
    }
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
        | TypeKind::RawPointer(inner)
        | TypeKind::Fallible { ty: inner, .. } => collect_type_refs(inner, scope),
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

    // -- E0410: `let` reassignment at the resolve rung (issue #8) -----------

    fn resolve(source: &str) -> Option<Diag> {
        let program = load_source("t.lu", source).expect("loads");
        resolve_check(&program)
    }

    #[test]
    fn let_reassignment_is_e0410_at_the_place() {
        let source = "fn main() -> !int {\n    let x = 1\n    x = 2\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E0410");
        // The counterparty's span discipline (pin a0c4564): the assigned
        // place alone, not the whole statement.
        let covered = &source[diag.span.start..diag.span.end];
        assert_eq!(covered, "x");
        assert!(diag.span.start > source.find("let x").expect("present"));
    }

    #[test]
    fn compound_assignment_is_assignment() {
        // `[gram.expr.assign]` routes `+=` through the same place rules.
        let source = "fn main() -> !int {\n    let total = 40\n    total += 2\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E0410");
        assert_eq!(&source[diag.span.start..diag.span.end], "total");
    }

    #[test]
    fn the_non_cases_stay_clean() {
        // `var` reassigns; a second `let` shadows; a `var` shadowing a `let`
        // is assignable again; a parameter is the mode system's business.
        assert!(
            resolve(
                "fn bump(n: int) -> int {\n    var m = n\n    m = m + 1\n    m\n}\n\
                 fn main() -> !int {\n    var a = 1\n    a += 2\n    let b = bump(a)\n    \
                 let b = b + 1\n    var b = b\n    b = 0\n    b\n}\n"
            )
            .is_none()
        );
    }

    #[test]
    fn a_when_body_assigns_through_the_acquired_cell_not_the_binding() {
        // `[conc.when.body]` — `corpus/conc/when_multi.lu`'s shape: the
        // operand names are assignable inside the body whatever introduced
        // them.
        assert!(
            resolve(
                "fn main() -> !int {\n    let a = Mutex(1)\n    let b = Mutex(2)\n    \
                 when (a, b) { a += 10; b += 10 }\n    0\n}\n"
            )
            .is_none()
        );
    }

    #[test]
    fn a_let_captured_by_a_closure_still_rejects_assignment() {
        let source = "fn main() -> !int {\n    let x = 1\n    \
                      let f = fn() { x = 2 }\n    f()\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E0410");
    }

    // -- the variant table (issue #5) ---------------------------------------

    #[test]
    fn enum_variants_are_collected_per_module() {
        let program = load_source(
            "t.lu",
            "enum Ordering { Less, Equal, Greater }\nfn main() -> int { 0 }\n",
        )
        .expect("loads");
        assert_eq!(
            program.root().variants.get("Greater"),
            Some(&vec!["Ordering".to_owned()])
        );
        assert!(!program.root().variants.contains_key("Ordering"));
    }
}

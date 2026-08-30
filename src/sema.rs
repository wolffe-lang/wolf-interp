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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::ast::{
    Arg, Block, Expr, ExprKind, FnDecl, Item, ItemKind, ParamMode, PatKind, Pattern, Stmt,
    StmtKind, StrLit, StrPart, StructDef, Type, TypeArg, TypeKind, Unit,
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

/// One parsed source file, retained beside the collected definitions.
///
/// The lint pass (s68, issue #19) needs what [`collect`] drops: item and
/// statement attributes (`#[allow]` — spec/01 §9.3, part of the program),
/// statement shapes, and the file's own bytes for the spans that extend
/// through a statement terminator or point inside a literal.
#[derive(Debug, Clone)]
pub struct SourceUnit {
    pub file: String,
    pub source: String,
    pub unit: Unit,
    /// Loaded through the configured std root. The std tree is exempt from
    /// the shadow lint (W0304) per the s68 triage.
    pub from_std: bool,
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
    /// A trait's **default method bodies**: trait name → method name → the
    /// decl as written in the `trait` item. Consulted only when a subject's
    /// own table misses (the s17 order: the type's impl wins; the default is
    /// the floor under it) — wolf-interp#32's fix. Inside a default body,
    /// `self` is the concrete receiver, so `self.name()` re-enters ordinary
    /// dispatch and the impl's override wins: dispatch-back-through-Self
    /// without a Self machinery this face-value tier never had.
    pub trait_defaults: BTreeMap<String, BTreeMap<String, Box<FnDecl>>>,
    /// `type Cover = distinct media.Song`: alias name → the target's head
    /// name (`Song`). What lets an adapter cast MOVE the nominal identity
    /// (`s as Cover` renames the value, both directions), which is the D28
    /// layout-identity fact plus the one thing a by-name dispatcher needs:
    /// dispatch follows the name, so the name must follow the cast
    /// (wolf-interp#32's adapter case).
    pub distincts: BTreeMap<String, String>,
    /// Which traits each subject implements, from its `impl T for S` items —
    /// including an EMPTY impl block, which is exactly the case where every
    /// method comes from the defaults and the `methods` table alone cannot
    /// even name the subject.
    pub trait_impls: BTreeMap<String, Vec<String>>,
    /// Every lowercase tag a function signature of this module declares in an
    /// error row (return rows and postfix rows alike). The pattern-resolution
    /// rule (issue #12, the interpreter half of wolf-lang#4): a lowercase
    /// identifier pattern over a tag-shaped scrutinee that names a *declared*
    /// tag is a row-tag pattern; everything else still binds.
    pub row_tags: std::collections::BTreeSet<String>,
    /// The module's parsed files, in load order — the lint pass's input.
    pub units: Vec<SourceUnit>,
    /// The directory's standalone entries (`[conf.directive.standalone]`,
    /// D59) — files that opted OUT of this module, kept so the resolve-time
    /// teach-notes can name the file, the marker, and the fix when a lookup
    /// misses something a standalone sibling defines.
    pub standalone: Vec<StandaloneSibling>,
}

/// One file that opted out of its directory's module (D59): the spelling it
/// used, and the top-level names it defines (empty when the file does not
/// parse — a standalone sibling's syntax is its own program's problem).
#[derive(Debug, Clone)]
pub struct StandaloneSibling {
    /// Display path, `/`-separated.
    pub file: String,
    /// The standalone spelling, human-named — e.g. "`//! member: false`".
    pub marker: &'static str,
    /// Top-level item names the file defines.
    pub names: Vec<String>,
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
        false,
    )];
    let mut loaded: Vec<String> = Vec::new();
    // #39 (module identity is the FULL path): every binding's resolved
    // directory, program-wide. Two `use` decls binding the same name to
    // DIFFERENT directories used to single-bind silently — the first won and
    // the second's calls answered through the wrong module. That is an
    // honest error now (the compiler's E0306 shape; `use … as` is the fix
    // the message names). Same name to the SAME directory stays legal: `use`
    // is file-scoped upstream, and two files importing one module is the
    // ordinary case.
    let mut resolved: BTreeMap<String, PathBuf> = BTreeMap::new();

    while let Some((name, dir, entry, from_std)) = queue.pop() {
        if loaded.contains(&name) {
            // An import cycle is a compile error the compiler owns (E0303).
            // Here it is simply a module already loaded — the graph is walked
            // once, so a cycle terminates instead of recursing forever.
            continue;
        }
        loaded.push(name.clone());

        let module = load_module(&name, &dir, entry.as_deref(), &mut program.files, from_std)?;
        let mut queued: Vec<String> = Vec::new();
        let mut claim = |bound: &str, candidate: &Path, module: &Module| -> Result<(), LoadError> {
            match resolved.get(bound) {
                None => {
                    resolved.insert(bound.to_owned(), candidate.to_path_buf());
                    Ok(())
                }
                Some(first) if same_dir(first, candidate) => Ok(()),
                Some(first) => {
                    // The later decl's own span, from this module's scopes.
                    let (span, file) = module
                        .scopes
                        .iter()
                        .flat_map(|scope| {
                            scope
                                .uses
                                .iter()
                                .filter(|used| used.name == bound)
                                .map(|used| (used.name_span, scope.file.clone()))
                        })
                        .next_back()
                        .unwrap_or((Span::new(0, 0), module.name.clone()));
                    Err(LoadError::Syntax {
                        file,
                        diag: Box::new(Diag::new(
                            "E0306",
                            span,
                            "gram.item.use",
                            format!(
                                "`{bound}` is already bound to the module at `{}`; this \
                                 import names `{}` — module identity is the full path \
                                 (#39), so give one side its own name with `use … as`",
                                first.display(),
                                candidate.display()
                            ),
                        )),
                    })
                }
            }
        };
        for (bound, segments) in &module.use_paths {
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
                    claim(bound, &candidate, &module)?;
                    if !loaded.contains(bound) && !queued.contains(bound) {
                        queue.push((bound.clone(), candidate, None, true));
                        queued.push(bound.clone());
                    }
                    continue;
                }
            }
            // #39: a dotted `use` names the directory at its FULL path —
            // `use fmt.float` is `<package root>/fmt/float`, and two leaves
            // spelled `float` coexist under distinct bound names. The flat
            // `<package root>/<bound>` spelling stays as the fallback, which
            // keeps single-segment imports and flat mirrors working
            // unchanged.
            let mut full = package_root.clone();
            for segment in segments {
                full.push(segment);
            }
            if segments.len() >= 2 && full.is_dir() {
                claim(bound, &full, &module)?;
                if !loaded.contains(bound) && !queued.contains(bound) {
                    queue.push((bound.clone(), full, None, false));
                    queued.push(bound.clone());
                }
                continue;
            }
            let candidate = package_root.join(bound);
            if candidate.is_dir() {
                claim(bound, &candidate, &module)?;
                if !loaded.contains(bound) && !queued.contains(bound) {
                    queue.push((bound.clone(), candidate, None, false));
                    queued.push(bound.clone());
                }
            }
        }
        // The heads of dotted `use` paths (`std` in `use std.prelude`) sit in
        // `module.uses` without a `use_paths` entry of their own; a sibling
        // directory by that name is still a module of this program.
        for used in &module.uses {
            let candidate = package_root.join(used);
            if candidate.is_dir() && !loaded.contains(used) && !queued.contains(used) {
                queue.push((used.clone(), candidate, None, false));
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
    module.units.push(SourceUnit {
        file: name.to_owned(),
        source: source.to_owned(),
        unit: parsed.unit,
        from_std: false,
    });
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
    from_std: bool,
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
        if let Some(marker) = standalone_mark(&path, entry, &source, from_std) {
            // The display name is the bare file name — the loader reads a
            // directory's direct children only, and a note that travels on a
            // record must not carry a machine-local path prefix.
            let file = path.file_name().map_or_else(
                || crate::slash_path(&path),
                |name| name.to_string_lossy().into_owned(),
            );
            module.standalone.push(StandaloneSibling {
                file,
                marker,
                names: top_level_names(&source),
            });
            continue;
        }
        let parsed = crate::parse::parse_source(&source).map_err(|diag| LoadError::Syntax {
            file: crate::slash_path(&path),
            diag: Box::new(diag),
        })?;
        let display = crate::slash_path(&path);
        files.push(display.clone());
        collect(&parsed.unit, &mut module, &display, &source);
        module.units.push(SourceUnit {
            file: display,
            source,
            unit: parsed.unit,
            from_std,
        });
    }
    Ok(module)
}

/// The standalone mark a `.lu` file carries, or `None` when the file is a
/// member of its directory's module — `[conf.directive.standalone]` (D59).
///
/// Membership is the default: a plain `.lu` file joins its directory's module
/// with no marker. A file opts OUT by being a standalone entry, and the
/// standalone set is exactly the four spellings D59 names:
///
/// 1. `//! member: false` — the user-facing opt-out (several programs in one
///    directory is one header line per program);
/// 2. both `//! check:` and `//! phase:` — the corpus entry pair
///    (`[conf.directive.member]`);
/// 3. a script announcement — a `#!` first line (`#![` is the file-wide
///    attribute opener, not a script — `[gram.lex.shebang]`) or a
///    `pkg { … }` frontmatter block (s53: a script is a single-file package);
/// 4. a `_test.lu` name (s39: `wolf test` runs each test file alone).
///
/// An explicit `member:` key always decides. Two boundaries the clause
/// draws: the NAMED ENTRY of a compilation always belongs to its own root
/// module, whatever its markers say; and std/dep trees stay whole-package —
/// every file participates, so a std file is never excluded.
fn standalone_mark(
    path: &Path,
    entry: Option<&Path>,
    source: &str,
    from_std: bool,
) -> Option<&'static str> {
    if from_std || entry.is_some_and(|entry| same_file(entry, path)) {
        return None;
    }
    if let Some(member) = crate::directive::member_key(source) {
        return (!member).then_some("`//! member: false`");
    }
    if crate::directive::entry_pair(source) {
        return Some("the `//! check:` + `//! phase:` entry pair");
    }
    if source.starts_with("#!") && !source.starts_with("#![") {
        return Some("a `#!` script line");
    }
    if has_pkg_frontmatter(source) {
        return Some("a `pkg { … }` frontmatter block");
    }
    if path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with("_test.lu"))
    {
        return Some("a `_test.lu` file name");
    }
    None
}

/// Whether the file's first non-trivia line opens a `pkg { … }` frontmatter
/// block (s53's script mode: a script is a single-file package carrying its
/// manifest in-file). Trivia here is the shebang line, blank lines, and `//`
/// comments of every stripe — exactly what may precede the frontmatter.
fn has_pkg_frontmatter(source: &str) -> bool {
    for (index, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if (index == 0 && raw.starts_with("#!") && !raw.starts_with("#!["))
            || line.is_empty()
            || line.starts_with("//")
        {
            continue;
        }
        let Some(rest) = line.strip_prefix("pkg") else {
            return false;
        };
        return rest.trim_start().starts_with('{');
    }
    false
}

/// The top-level item names a source defines, for the D59 teach-notes — best
/// effort: a file that does not parse defines nothing nameable.
fn top_level_names(source: &str) -> Vec<String> {
    let Ok(parsed) = crate::parse::parse_source(source) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for item in &parsed.unit.items {
        let name = match &item.kind {
            crate::ast::ItemKind::Fn(decl) => Some(decl.name.name.clone()),
            crate::ast::ItemKind::Struct(def) => def.name.as_ref().map(|n| n.name.clone()),
            crate::ast::ItemKind::Enum(def) => def.name.as_ref().map(|n| n.name.clone()),
            crate::ast::ItemKind::TypeAlias(alias) => Some(alias.name.name.clone()),
            crate::ast::ItemKind::Trait(def) => Some(def.name.name.clone()),
            crate::ast::ItemKind::Binding(binding) => {
                if let crate::ast::PatKind::Binding(ident) = &*binding.pattern.kind {
                    Some(ident.name.clone())
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(name) = name {
            names.push(name);
        }
    }
    names
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Two directory paths naming the same directory (#39's collision check
/// compares module *identities*, which are full paths).
fn same_dir(a: &Path, b: &Path) -> bool {
    same_file(a, b)
}

/// The head name of a type, for impl-subject and trait naming: the path's
/// last segment, through prefix keywords. `None` for shapes an impl subject
/// does not take at this machine's depth (tuples, fn types, …).
/// The head name of a type as written — `distinct media.Song` → `Song`.
/// The eval tier's adapter-cast rebranding wants the same reading the
/// method tables were built with, so the one function serves both.
#[must_use]
pub fn head_name(ty: &Type) -> Option<String> {
    type_head_name(ty)
}

fn type_head_name(ty: &Type) -> Option<String> {
    match &*ty.kind {
        TypeKind::Path { path, .. } => path.segments.last().map(|s| s.name.clone()),
        TypeKind::Prefixed { ty, .. }
        | TypeKind::ErrorUnion(ty)
        | TypeKind::Fallible { ty, .. } => type_head_name(ty),
        _ => None,
    }
}

/// The row tags a type spells — the *expected declared row* of a checked
/// position (`[gram.expr.tagident]`, D52): a callee parameter's type at
/// argument position, a `let`/`var` annotation at initializer position.
#[must_use]
pub fn type_tags(ty: &Type) -> Vec<String> {
    let mut tags = Vec::new();
    type_row_tags(ty, &mut tags);
    tags
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
                    crate::ast::TypeDef::Alias(ty)
                        if matches!(
                            &*ty.kind,
                            crate::ast::TypeKind::Prefixed {
                                kw: crate::ast::PrefixTypeKw::Distinct,
                                ..
                            }
                        ) =>
                    {
                        if let Some(target) = type_head_name(ty) {
                            module.distincts.insert(alias.name.name.clone(), target);
                        }
                        define(
                            module,
                            alias.name.name.clone(),
                            Def::Opaque("type"),
                            true,
                            Some(alias.name.span),
                            file,
                        );
                    }
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
                for member in &def.members {
                    if let ItemKind::Fn(decl) = &member.kind
                        && decl.body.is_some()
                    {
                        collect_signature_tags(decl, &mut module.row_tags);
                        module
                            .trait_defaults
                            .entry(def.name.name.clone())
                            .or_default()
                            .insert(decl.name.name.clone(), decl.clone());
                    }
                }
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
                    if let Some(trait_name) = &trait_name {
                        let traits = module.trait_impls.entry(subject.clone()).or_default();
                        if !traits.contains(trait_name) {
                            traits.push(trait_name.clone());
                        }
                    }
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
/// private access (E0304), unused import (E0305), `let` reassignment (E0410),
/// call-site mode (E1007), take-mode reuse (E1001, issue #48), then the
/// pin-`f0da6e6` tier statics — raw signature boundary (E1302), the unsafe
/// ring plus the cast matrix's bool column, char indexing, format specs, and
/// the s71 `else` handler row-coverage rule
/// (E1301/E0805/E0411/E0412/E0413/E0809, one source-order body walk). Each
/// corpus law file exercises exactly one; a program violating several
/// reports the first in this order, which is a defensible choice the spec
/// does not pin.
#[must_use]
pub fn resolve_check(program: &Program) -> Option<Diag> {
    cycle_check(program)
        .or_else(|| dup_check(program))
        .or_else(|| private_check(program))
        .or_else(|| unused_check(program))
        .or_else(|| reassign_check(program))
        .or_else(|| mode_check(program))
        .or_else(|| move_check(program))
        .or_else(|| unsafe_sig_check(program))
        .or_else(|| tier_check(program))
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
                     file boundaries create no scopes (D32) — two separate programs sharing a \
                     directory each mark themselves `//! member: false` (D59)",
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
    body_walk(program, false, false, false).0
}

/// The X1 call-site mode law, statically (issue #15, the book's ch07 catch —
/// bs03 ba:blocker): a call missing (or misspelling) the mode the signature
/// demands must not run to a wrong answer. E1007 is the static rule and
/// `[conf.trap.map]` gives it **no** dynamic meaning — there is no
/// mode-mismatch trap kind, and unlike E1001/E1002 the s04 tables state no
/// runtime semantics for the disagreement — so the honest place to stop is
/// the rung where the signature is visible: this machine's resolve tier,
/// exactly as E0410 (issue #8). Code, span (the argument expression), and
/// message shape match the counterparty's, observed at pin `ad6cef7`
/// (`corpus/memory/mode_missing_mut.lu` and the three probe shapes: missing
/// mode, extra mode, wrong mode word).
///
/// Scope discipline: only callees whose signature the resolve rung can
/// actually see — a bare name naming a function item of the current module,
/// or `module.fn` naming one of a sibling module — and never a name a local
/// binding shadows (the callee is then a *value*; its signature is dynamic).
/// Method calls are the receiver-mode rule's business (E0804, ledgered
/// conservatism), not this check's. The dynamic residue — calls through
/// function values — is refused at run time (`eval_call`), never executed to
/// a wrong answer.
fn mode_check(program: &Program) -> Option<Diag> {
    body_walk(program, false, true, false).0
}

/// The take-mode reuse law, statically (issue #48, wolf-std F-0098): a
/// call-site `mut`/`take` marker over a local whose WHOLE BINDING an earlier
/// `take` marker already consumed is E1001 at the reuse argument — the code,
/// span and message shape the counterparty emits (observed at pin `addcd7f`:
/// primary span the argument identifier, "`s` is used here after its value
/// moved away"), at this machine's only static rung.
///
/// Scope discipline, the E1007 pattern: the walk diagnoses only what the
/// resolve rung can see with certainty — a bare single-segment path taken
/// whole by an explicit call-site marker (argument or moded receiver), then
/// re-marked in straight-line source order with no re-initialization between
/// (`[mem.tier0.move.4]` clears; so does shadowing). Everything narrower
/// stays the DYNAMIC discipline `[mem.tier0.move.2]` states for the
/// interpreter and the corpus pins: a bare READ of a moved-from place still
/// traps `use-after-move` (`memory/move_use_after.lu`'s check is satisfied
/// either way; `faults/use_after_move_field.lu` runs to its pinned trap —
/// field-granular takes are not tracked here), moves recorded inside a
/// branch, loop, closure or `defer` never leak past it, and a callee's
/// signature is never consulted — the MARKER is the spelling, exactly as it
/// is for the dynamic move.
fn move_check(program: &Program) -> Option<Diag> {
    body_walk(program, false, false, true).0
}

// ---------------------------------------------------------------------------
// The unsafe-tier statics (issue #18, pin `f0da6e6`): E1302, E1301, E0805,
// E0411 — wolfc-parity code+span at this machine's only static rung
// ---------------------------------------------------------------------------
//
// The book's ch09 differential (bs05) found the unsafe ring unenforced: raw
// operations ran outside `unsafe` blocks the counterparty rejects. Like
// E0410 and E1007 before them, these checks live at the resolve rung —
// sema-lite is this machine's only static tier — while wolfc's emissions
// live deeper (E1301/E1302 at its mem rung, E0805 at typecheck). The rung
// placement is the DIV-2026-011 question again, filed as DIV-2026-012;
// codes and spans match the counterparty, observed at pin `f0da6e6`.
//
// Sema-lite has no types, so the walk tracks the little state the rules
// need: which locals *syntactically* hold raw pointers (bound from an
// `as *T` cast, a `c.malloc`/`c.calloc` call, or an `unsafe { … }` block
// whose tail is one of those — the book's "laundered pointer" shape), and
// which locals have a literal-known class (E0411's "this receiver is a
// `str`"). Unknown stays unknown and is never diagnosed — the checks fire
// only on what the resolve rung can actually see, the E1007 discipline.

/// What a local is syntactically known to hold, for the tier statics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LitClass {
    Str,
    Int,
    Float,
    Bool,
    Raw,
    Unknown,
}

/// A function's declared **closed** error row, for the E0809 handler-coverage
/// rule: the tag names, and a compact rendering for the diagnostic.
struct RowInfo {
    tags: Vec<String>,
    render: String,
}

/// The tier walk's lexical state: one pass per function body, source order,
/// first finding wins (`[proto.cmp.phase]` compares the first diagnostic).
struct TierWalk<'a> {
    /// Whether this module `import c`s — bound once from the module's own
    /// header list, so the answer cannot drift from the map key.
    imports_c: bool,
    unsafe_depth: usize,
    scopes: Vec<Vec<(String, LitClass)>>,
    /// The module's function items' declared closed rows, for E0809.
    rows: &'a BTreeMap<String, RowInfo>,
    /// Every top-level name this module declares, for [`unknown_cast_target`].
    /// A module is a directory (D32), so this is the whole namespace a bare
    /// lower-case type name could resolve into.
    declared: &'a BTreeSet<String>,
}

impl TierWalk<'_> {
    fn lookup(&self, name: &str) -> LitClass {
        for scope in self.scopes.iter().rev() {
            for (n, class) in scope.iter().rev() {
                if n == name {
                    return *class;
                }
            }
        }
        LitClass::Unknown
    }

    fn declare(&mut self, name: &str, class: LitClass) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name.to_owned(), class));
        }
    }

    fn in_unsafe(&self) -> bool {
        self.unsafe_depth > 0
    }

    /// The syntactic class of an expression, conservatively. `Unknown` means
    /// "say nothing" — the walk never guesses.
    fn classify(&self, expr: &Expr) -> LitClass {
        match &*expr.kind {
            ExprKind::Int(_) => LitClass::Int,
            ExprKind::Float(_) => LitClass::Float,
            ExprKind::Bool(_) => LitClass::Bool,
            ExprKind::Str(_) => LitClass::Str,
            ExprKind::Group(inner) => self.classify(inner),
            ExprKind::Path(path) if path.is_single() => self.lookup(&path.segments[0].name),
            ExprKind::Unary { operand, .. } => match self.classify(operand) {
                c @ (LitClass::Int | LitClass::Float) => c,
                _ => LitClass::Unknown,
            },
            ExprKind::Binary { op, lhs, rhs } => {
                use crate::ast::BinOp;
                if !matches!(
                    op,
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem
                ) {
                    return LitClass::Unknown;
                }
                match (self.classify(lhs), self.classify(rhs)) {
                    (LitClass::Int, LitClass::Int) => LitClass::Int,
                    (LitClass::Float, LitClass::Float) => LitClass::Float,
                    _ => LitClass::Unknown,
                }
            }
            ExprKind::Cast { ty, .. } if type_contains_raw(ty) => LitClass::Raw,
            ExprKind::Call { callee, .. } if self.c_alloc_name(callee).is_some() => LitClass::Raw,
            // The book's laundering shape: `let p = unsafe { … as *u8 }` —
            // the VALUE that leaves the block is raw; holding it is free,
            // using it outside the ring is not.
            ExprKind::Unsafe { body } => {
                body.tail
                    .as_deref()
                    .map_or(LitClass::Unknown, |tail| match self.classify(tail) {
                        LitClass::Raw => LitClass::Raw,
                        _ => LitClass::Unknown,
                    })
            }
            _ => LitClass::Unknown,
        }
    }

    /// `c.NAME` when the module imports C and `c` is not locally shadowed —
    /// the resolution rule the machine applies dynamically, statically. A
    /// dotted name parses as a two-segment *path* (`[gram.expr.primary]`),
    /// the same shape the machine's own `c.*` dispatch reads.
    fn c_member_name(&self, callee: &Expr) -> Option<String> {
        let ExprKind::Path(path) = &*callee.kind else {
            return None;
        };
        if path.segments.len() != 2
            || path.segments[0].name != "c"
            || self.lookup("c") != LitClass::Unknown
            || !self.imports_c
        {
            return None;
        }
        Some(path.segments[1].name.clone())
    }

    fn c_alloc_name(&self, callee: &Expr) -> Option<String> {
        self.c_member_name(callee)
            .filter(|name| matches!(name.as_str(), "malloc" | "calloc"))
    }
}

/// Does this type mention `*T` anywhere the signature's reader can see?
fn type_contains_raw(ty: &Type) -> bool {
    match &*ty.kind {
        TypeKind::RawPointer(_) => true,
        TypeKind::ErrorUnion(inner) | TypeKind::Prefixed { ty: inner, .. } => {
            type_contains_raw(inner)
        }
        TypeKind::Fallible { ty: inner, .. } => type_contains_raw(inner),
        TypeKind::Tuple(items) => items.iter().any(type_contains_raw),
        TypeKind::Fn { params, ret } => {
            params.iter().any(type_contains_raw)
                || ret.as_ref().is_some_and(|r| type_contains_raw(&r.ty))
        }
        _ => false,
    }
}

/// Every function of a module, items first then impl methods — the same
/// deterministic order as [`body_walk`].
fn each_fn(module: &Module) -> impl Iterator<Item = &FnDecl> {
    module
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
        )
}

/// `[mem.unsafe.scope]` (s22's boundary law): unsafety never crosses a
/// signature — there are no `unsafe fn`s, so a `*T` in a parameter or return
/// type would smuggle the raw tier into every caller's audit surface. E1302
/// at the parameter *name* (the counterparty's span, observed at pin
/// `f0da6e6`: `corpus/memory/unsafe_sig.lu`, span `[329,330]` — the `p` of
/// `fn peek(p: *u8)`); a raw return type reports at the type's span (no
/// pinned witness — this machine's documented choice).
fn unsafe_sig_check(program: &Program) -> Option<Diag> {
    for module in program.modules.values() {
        for decl in each_fn(module) {
            for param in &decl.params {
                if let crate::ast::ParamKind::Named { name, ty } = &param.kind
                    && type_contains_raw(ty)
                {
                    return Some(Diag::new(
                        "E1302",
                        name.span,
                        "mem.unsafe.scope",
                        format!(
                            "`{}`'s parameter `{}` carries a raw pointer, but this boundary \
                             stays fully safe: there are no `unsafe fn`s — the proof lives \
                             at the `unsafe` block and the module is the audit granule. Pass \
                             a `handle` or a region value, or keep the `*T` in a \
                             module-private field",
                            decl.name.name, name.name
                        ),
                    ));
                }
            }
            if let Some(ret) = &decl.ret
                && type_contains_raw(&ret.ty)
            {
                return Some(Diag::new(
                    "E1302",
                    ret.ty.span,
                    "mem.unsafe.scope",
                    format!(
                        "`{}`'s return type carries a raw pointer, but this boundary stays \
                         fully safe: there are no `unsafe fn`s — keep the `*T` inside the \
                         module and hand out a `handle` or a region value",
                        decl.name.name
                    ),
                ));
            }
        }
    }
    None
}

/// The tier body walk: E1301 (the unsafe ring), E0805 (the cast matrix's
/// `bool` column), E0411 (`s[i]` char indexing), E0412/E0413 (format specs,
/// s38). One deterministic pass per function body; the first finding wins.
fn tier_check(program: &Program) -> Option<Diag> {
    for module in program.modules.values() {
        // The E0809 signature map: every function item's declared closed row.
        // An open row (`..`) is never judged — a handler cannot enumerate it.
        let mut rows: BTreeMap<String, RowInfo> = BTreeMap::new();
        for (name, (def, _)) in &module.items {
            // `-> int ! {…}`: `parse_type` folds the postfix row into the
            // type itself (`Fallible`), so the row lives there; `RetType::row`
            // carries one only for the shapes `parse_type` cannot swallow.
            if let Def::Fn(decl) = def
                && let Some(ret) = &decl.ret
                && let Some(row) = (match &ret.row {
                    Some(row) => Some(row),
                    None => match &*ret.ty.kind {
                        TypeKind::Fallible { row, .. } => Some(row),
                        _ => None,
                    },
                })
                && !row.open
            {
                let mut tags = Vec::new();
                let mut parts = Vec::new();
                for entry in &row.entries {
                    let [segment] = entry.path.segments.as_slice() else {
                        continue;
                    };
                    tags.push(segment.name.clone());
                    parts.push(if entry.payload.is_empty() {
                        segment.name.clone()
                    } else {
                        format!("{}(…)", segment.name)
                    });
                }
                if tags.len() == row.entries.len() && !tags.is_empty() {
                    let render = format!("{{{}}}", parts.join(", "));
                    rows.insert(name.clone(), RowInfo { tags, render });
                }
            }
        }
        // Every top-level name of the module, whatever it defines. A type
        // name resolves into this namespace (D32: the directory is the
        // module), and `Def` does not distinguish an alias from an enum —
        // both arrive as `Opaque` — so the set is names, not kinds. Used
        // only to decline judging a name that *does* resolve.
        let declared: BTreeSet<String> = module.items.keys().cloned().collect();
        for decl in each_fn(module) {
            let mut walk = TierWalk {
                imports_c: !module.c_headers.is_empty(),
                unsafe_depth: 0,
                scopes: vec![Vec::new()],
                rows: &rows,
                declared: &declared,
            };
            for param in &decl.params {
                if let crate::ast::ParamKind::Named { name, ty } = &param.kind {
                    walk.declare(&name.name, class_of_type(ty));
                }
            }
            if let Some(body) = &decl.body
                && let Some(diag) = walk.block(body)
            {
                return Some(diag);
            }
        }
        for (def, _) in module.items.values() {
            if let Def::Binding(binding) = def {
                let mut walk = TierWalk {
                    imports_c: !module.c_headers.is_empty(),
                    unsafe_depth: 0,
                    scopes: vec![Vec::new()],
                    rows: &rows,
                    declared: &declared,
                };
                if let Some(diag) = walk.expr(&binding.value) {
                    return Some(diag);
                }
            }
        }
    }
    None
}

/// Every built-in scalar type name the language spells in lower case.
///
/// One list, two readers: [`class_of_type`] classifies annotations with it and
/// [`unknown_cast_target`] decides what "names nothing" means. Keeping them on
/// one constant is the point — a name known to one and unknown to the other
/// would make a cast both classified and unresolvable.
///
/// The language has no `char`, and no `bytes`: `[mem.str.cmp]`'s note that the
/// string library is "in-library with **no bytes accessor**" is the closest the
/// spec comes, and the counterparty answers E0301 for `as bytes` — the two
/// agree that the name is not a type.
const BUILTIN_SCALAR_TYPES: &[&str] = &[
    "bool", "char", "f32", "f64", "i128", "i16", "i32", "i64", "i8", "int", "str", "u128", "u16",
    "u32", "u64", "u8", "uint",
];

/// The class an *annotated* type names, for parameters (`greet(name: str)`).
fn class_of_type(ty: &Type) -> LitClass {
    match &*ty.kind {
        TypeKind::RawPointer(_) => LitClass::Raw,
        TypeKind::Path { path, args } if path.is_single() && args.is_empty() => {
            match path.segments[0].name.as_str() {
                "str" => LitClass::Str,
                "bool" => LitClass::Bool,
                "f32" | "f64" => LitClass::Float,
                "int" | "uint" | "i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16" | "u32"
                | "u64" | "u128" => LitClass::Int,
                _ => LitClass::Unknown,
            }
        }
        _ => LitClass::Unknown,
    }
}

/// The span of a cast target that names nothing at all, or `None` when this
/// rung cannot say (issue #17 ask 1).
///
/// The judgement is deliberately narrow, because "never guess" is this pass's
/// whole discipline. It fires only on a **single, unqualified, lower-case**
/// path with no generic arguments that is neither a built-in scalar nor a name
/// this module declares. Everything else declines:
///
/// - An upper-case initial is a nominal type — a struct, an enum, a `distinct`
///   alias, a generic parameter (`x as T`), or a prelude name this rung has no
///   registry for. `typecheck/cast_set.lu`'s `2 as Meters` lives here, and so
///   does every adapter cast.
/// - A qualified path (`media.Song`) resolves through another module's items,
///   which sema-lite does not walk for types.
/// - Generic arguments (`wrapping[u32]`, `List[int]`) name a constructor, not a
///   scalar.
///
/// What is left is exactly the issue's `s as nonsense` and `s as bytes` shape:
/// a lower-case name that resolves nowhere. The counterparty answers E0301 at
/// `resolve` spanning the **type name** (observed at pin `613c3dc`:
/// `s as nonsense` → `E0301` `[55,63]`, the eight bytes of `nonsense`), so
/// that is the code and the span this returns.
fn unknown_cast_target(ty: &Type, declared: &BTreeSet<String>) -> Option<Span> {
    let TypeKind::Path { path, args } = &*ty.kind else {
        return None;
    };
    if !args.is_empty() || !path.is_single() {
        return None;
    }
    let segment = &path.segments[0];
    let name = segment.name.as_str();
    if !name.starts_with(|c: char| c.is_lowercase()) {
        return None;
    }
    if BUILTIN_SCALAR_TYPES.contains(&name) || declared.contains(name) {
        return None;
    }
    Some(segment.span)
}

impl TierWalk<'_> {
    fn block(&mut self, block: &Block) -> Option<Diag> {
        self.scopes.push(Vec::new());
        for stmt in &block.stmts {
            if let Some(diag) = self.stmt(stmt) {
                self.scopes.pop();
                return Some(diag);
            }
        }
        let out = block.tail.as_deref().and_then(|tail| self.expr(tail));
        self.scopes.pop();
        out
    }

    fn stmt(&mut self, stmt: &Stmt) -> Option<Diag> {
        match &stmt.kind {
            StmtKind::Binding(binding) => {
                if let Some(diag) = self.expr(&binding.value) {
                    return Some(diag);
                }
                if let PatKind::Binding(ident) = &*binding.pattern.kind {
                    let class = binding
                        .ty
                        .as_ref()
                        .map(class_of_type)
                        .filter(|c| *c != LitClass::Unknown)
                        .unwrap_or_else(|| self.classify(&binding.value));
                    self.declare(&ident.name, class);
                }
                None
            }
            StmtKind::Assign { place, value, .. } => {
                // A write through a raw local — `p[0] = 1` — is the ring's
                // op; the span is the place, the counterparty's shape
                // (observed `[452,456]` on `unsafe_raw_outside.lu`).
                if let ExprKind::BracketApply { base, .. } = &*place.kind
                    && let ExprKind::Path(path) = &*base.kind
                    && path.is_single()
                    && self.lookup(&path.segments[0].name) == LitClass::Raw
                    && !self.in_unsafe()
                {
                    return Some(ring_diag("a raw pointer write", place.span));
                }
                self.expr(place).or_else(|| self.expr(value))
            }
            StmtKind::AssumeNoalias(operands) => {
                if !self.in_unsafe() {
                    return Some(ring_diag("`assume noalias`", stmt.span));
                }
                operands.iter().find_map(|operand| self.expr(operand))
            }
            StmtKind::Defer { expr, .. } => self.expr(expr),
            StmtKind::Expr(expr) => self.expr(expr),
            StmtKind::Item(item) => match &item.kind {
                ItemKind::Binding(binding) => self.expr(&binding.value),
                ItemKind::Fn(decl) => decl.body.as_ref().and_then(|body| self.block(body)),
                _ => None,
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    fn expr(&mut self, expr: &Expr) -> Option<Diag> {
        match &*expr.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Char(_)
            | ExprKind::Wildcard => None,
            ExprKind::Path(_) => None,
            ExprKind::Str(lit) => self.str_lit(lit),
            ExprKind::Group(inner)
            | ExprKind::Try(inner)
            | ExprKind::FromEnd(inner)
            | ExprKind::Freeze(inner) => self.expr(inner),
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { lhs, rhs, .. } => self.expr(lhs).or_else(|| self.expr(rhs)),
            ExprKind::Cast { expr: operand, ty } => {
                if let Some(diag) = self.expr(operand) {
                    return Some(diag);
                }
                // Issue #17 ask 1: the target type is RESOLVED. A lower-case
                // name that is neither a built-in scalar nor a name this
                // module declares names nothing, and a cast to nothing was
                // silently a no-op through 0.1.9 — the reported bug. E0301
                // at the type name is the counterparty's answer, span for
                // span.
                if let Some(span) = unknown_cast_target(ty, self.declared) {
                    return Some(Diag::new(
                        "E0301",
                        span,
                        "mod.scope",
                        "nothing with this name is in scope, so this cast names no target \
                         type — a typo in a cast target used to pass the value through \
                         unchanged, which is how a wrong type reaches the rest of the program",
                    ));
                }
                // Issue #17 ask 2: `str` is not a cast SOURCE either. The
                // matrix bridges numbers to numbers; `s as int` has no rule,
                // and passing the string through unchanged (0.1.9's
                // behavior) means the caller computes with a `str` where a
                // number was meant and nothing ever fails. The counterparty
                // answers E0805 at typecheck spanning the whole cast
                // expression (observed at pin `613c3dc`: `[50,58]` for
                // `s as int`); `[proto.cmp.rung]` makes our resolve-rung
                // emission of the same code agreement.
                if self.classify(operand) == LitClass::Str
                    && matches!(
                        class_of_type(ty),
                        LitClass::Int | LitClass::Float | LitClass::Bool
                    )
                {
                    return Some(Diag::new(
                        "E0805",
                        expr.span,
                        "ty.cast.closed-set",
                        "`str` does not cast to a numeric or `bool` type — `as` is outside \
                         the cast set here, and it is not a parser: parse the text instead \
                         of retyping it",
                    ));
                }
                // The cast matrix's `bool` column (issue #18 item 2): `as`
                // is not a truthiness bridge, and the counterparty rejects
                // in unsafe code too (observed at pin `f0da6e6`: E0805 at
                // the whole cast expression, `ty.cast.closed-set`).
                if is_bool_type(ty) {
                    return Some(Diag::new(
                        "E0805",
                        expr.span,
                        "ty.cast.closed-set",
                        "this does not cast to `bool` — `as` is outside the cast set here; \
                         there is no truthiness bridge. Compare instead: `x != 0` is the \
                         `bool` you meant",
                    ));
                }
                // The other side of the bridge (`corpus/typecheck/cast_bad.lu`,
                // the row this rung can see): a `bool` casts to nothing.
                if self.classify(operand) == LitClass::Bool {
                    return Some(Diag::new(
                        "E0805",
                        expr.span,
                        "ty.cast.closed-set",
                        "`bool` does not cast — `as` is outside the cast set here; write the \
                         value out instead, e.g. `if b { 1 } else { 0 }`",
                    ));
                }
                // And `as` is not a stringifier: nothing casts to `str`.
                if is_str_type(ty) {
                    return Some(Diag::new(
                        "E0805",
                        expr.span,
                        "ty.cast.closed-set",
                        "nothing casts to `str` — `as` is outside the cast set here; build \
                         strings with interpolation: \"{value}\" formats any primitive",
                    ));
                }
                // The int→pointer door outside the ring: `42 as *u8` forges
                // provenance. Retyping a value that is ALREADY raw (a c
                // allocator call, a raw local, a laundering unsafe block) is
                // inert — the counterparty flags the call, never the cast
                // (observed: `c.malloc(8) as *u8` carries one E1301, on the
                // call), so this fires only on non-raw operands.
                if type_contains_raw(ty)
                    && !self.in_unsafe()
                    && self.classify(operand) != LitClass::Raw
                {
                    return Some(ring_diag("an integer-to-pointer cast", expr.span));
                }
                None
            }
            ExprKind::Call { callee, args } => {
                // The C call itself is the ring's op; its span is the whole
                // call (observed `[384,395]`, `c.malloc(8)`).
                if let Some(name) = self.c_member_name(callee) {
                    if !self.in_unsafe() {
                        return Some(ring_diag(&format!("the C call `c.{name}`"), expr.span));
                    }
                } else if let ExprKind::Path(path) = &*callee.kind
                    && path.segments.len() == 2
                    && self.lookup(&path.segments[0].name) == LitClass::Raw
                    && matches!(
                        path.segments[1].name.as_str(),
                        "addr" | "with_addr" | "expose" | "with_exposed"
                    )
                    && !self.in_unsafe()
                {
                    // The provenance surface of `*T` (spec/02 §6) is
                    // ring-gated with the operation named.
                    return Some(ring_diag(
                        &format!("the provenance operation `{}`", path.segments[1].name),
                        expr.span,
                    ));
                }
                if let Some(diag) = self.expr(callee) {
                    return Some(diag);
                }
                args.iter().find_map(|arg| self.expr(&arg.expr))
            }
            ExprKind::BracketApply { base, args, .. } => {
                if let ExprKind::Path(path) = &*base.kind
                    && path.is_single()
                {
                    let class = self.lookup(&path.segments[0].name);
                    // A read through a raw local outside the ring.
                    if class == LitClass::Raw && !self.in_unsafe() {
                        return Some(ring_diag("a raw pointer read", expr.span));
                    }
                    // E0411 — no `s[i]` character indexing exists (D25): a
                    // single index cannot honestly name "a character".
                    if class == LitClass::Str
                        && let [crate::ast::IndexArg::Value(arg)] = args.as_slice()
                        && !matches!(&*arg.expr.kind, ExprKind::Range { .. })
                    {
                        return Some(Diag::new(
                            "E0411",
                            expr.span,
                            "str.slice",
                            "there is no `s[i]` character indexing in wolf (D25): a single \
                             index cannot honestly name \"a character\". Slice bytes with \
                             `s[a..b]`, take the byte view with `s.bytes()`, or find offsets \
                             with `s.find(…)`",
                        ));
                    }
                }
                if let Some(diag) = self.expr(base) {
                    return Some(diag);
                }
                args.iter().find_map(|arg| match arg {
                    crate::ast::IndexArg::Value(arg) => self.expr(&arg.expr),
                    crate::ast::IndexArg::Type(_) => None,
                })
            }
            ExprKind::Member { base, .. } | ExprKind::ModedReceiver { place: base, .. } => {
                self.expr(base)
            }
            ExprKind::StructLit { fields, .. } => fields
                .iter()
                .find_map(|field| field.value.as_ref().and_then(|value| self.expr(value))),
            ExprKind::Tuple(items) => items.iter().find_map(|item| self.expr(item)),
            ExprKind::Block(block) | ExprKind::Loop { body: block } => self.block(block),
            ExprKind::Range { start, end, .. } => start
                .as_ref()
                .and_then(|s| self.expr(s))
                .or_else(|| end.as_ref().and_then(|e| self.expr(e))),
            ExprKind::ElseDefault {
                expr: inner,
                handler,
            } => {
                if let Some(diag) = self.expr(inner) {
                    return Some(diag);
                }
                if let Some(diag) = self.else_cover(inner, handler) {
                    return Some(diag);
                }
                self.else_handler(handler)
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => self
                .expr(cond)
                .or_else(|| self.block(then))
                .or_else(|| otherwise.as_ref().and_then(|e| self.expr(e))),
            ExprKind::Match { scrutinee, arms } => {
                if let Some(diag) = self.expr(scrutinee) {
                    return Some(diag);
                }
                arms.iter().find_map(|arm| {
                    self.scopes.push(Vec::new());
                    declare_pattern_classes(&arm.pattern, self);
                    let out = arm
                        .guard
                        .as_ref()
                        .and_then(|guard| self.expr(guard))
                        .or_else(|| self.expr(&arm.body));
                    self.scopes.pop();
                    out
                })
            }
            ExprKind::For {
                pattern,
                iter,
                body,
            } => {
                if let Some(diag) = self.expr(iter) {
                    return Some(diag);
                }
                self.scopes.push(Vec::new());
                declare_pattern_classes(pattern, self);
                let out = self.block(body);
                self.scopes.pop();
                out
            }
            ExprKind::While { cond, body } => self.expr(cond).or_else(|| self.block(body)),
            ExprKind::Return(value) | ExprKind::Break(value) => {
                value.as_ref().and_then(|v| self.expr(v))
            }
            ExprKind::Continue => None,
            ExprKind::Closure { params, body, .. } => {
                self.scopes.push(Vec::new());
                for param in params {
                    let class = param
                        .ty
                        .as_ref()
                        .map(class_of_type)
                        .unwrap_or(LitClass::Unknown);
                    self.declare(&param.name.name, class);
                }
                let out = self.expr(body);
                self.scopes.pop();
                out
            }
            ExprKind::RegionSugar { body, .. } | ExprKind::Scope { body, .. } => self.block(body),
            ExprKind::In { region, body } => self.expr(region).or_else(|| self.block(body)),
            ExprKind::RegionValue { .. } => None,
            ExprKind::SpawnProc { args, .. } => args.iter().find_map(|arg| self.expr(&arg.expr)),
            ExprKind::Select { arms } => arms.iter().find_map(|arm| {
                let opening = match &arm.kind {
                    crate::ast::SelectArmKind::Recv { channel, .. } => self.expr(channel),
                    crate::ast::SelectArmKind::Timeout(expr) => self.expr(expr),
                };
                opening.or_else(|| self.expr(&arm.body))
            }),
            ExprKind::When { operands, body } => operands
                .iter()
                .find_map(|operand| self.expr(operand))
                .or_else(|| self.block(body)),
            ExprKind::Unsafe { body } => {
                self.unsafe_depth += 1;
                let out = self.block(body);
                self.unsafe_depth -= 1;
                out
            }
            ExprKind::UnsafeC { .. } => None,
            ExprKind::Asm { operands, .. } => operands
                .iter()
                .find_map(|operand| self.expr(&operand.value)),
            ExprKind::Borrow { place, from } => {
                if !self.in_unsafe() {
                    return Some(ring_diag("`borrow … from`", expr.span));
                }
                self.expr(place).or_else(|| self.expr(from))
            }
        }
    }

    /// E0809 (s71, wolf-lang#43): an `else` handler runs for **every** error
    /// its operand can carry, so a pattern in handler position must cover the
    /// operand's whole row. Judged only where this rung can see the row — a
    /// direct, unshadowed call to a same-module function item with a
    /// declared closed row — the E1007 discipline: the walk never guesses.
    fn else_cover(&self, inner: &Expr, handler: &crate::ast::ElseHandler) -> Option<Diag> {
        let crate::ast::ElseHandler::Handler { pattern, .. } = handler else {
            return None;
        };
        let ExprKind::Call { callee, .. } = &*inner.kind else {
            return None;
        };
        let ExprKind::Path(path) = &*callee.kind else {
            return None;
        };
        if !path.is_single() || self.is_local(&path.segments[0].name) {
            return None;
        }
        let row = self.rows.get(&path.segments[0].name)?;
        let Cover::Tags(covered) = handler_cover(pattern, &row.tags) else {
            return None;
        };
        let missing: Vec<&str> = row
            .tags
            .iter()
            .map(String::as_str)
            .filter(|tag| !covered.contains(*tag))
            .collect();
        if missing.is_empty() {
            return None;
        }
        let missing = missing
            .iter()
            .map(|tag| format!("`{tag}`"))
            .collect::<Vec<_>>()
            .join(", ");
        Some(Diag::new(
            "E0809",
            pattern.span,
            "err.else",
            format!(
                "this `else` handler pattern leaves {missing} unhandled: the operand's error \
                 row is `{}`, and an `else` handler runs for every error its operand can carry \
                 — cover the whole row here, or `match` over the row to handle its cases \
                 separately",
                row.render
            ),
        ))
    }

    /// Whether `name` is bound in any lexical scope of this walk — every
    /// `let`/`var`/parameter/pattern binding is declared here, so a hit means
    /// a call through `name` does not reach the module item of that name.
    fn is_local(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.iter().any(|(n, _)| n == name))
    }

    fn else_handler(&mut self, handler: &crate::ast::ElseHandler) -> Option<Diag> {
        use crate::ast::ElseHandler;
        match handler {
            ElseHandler::Block(block) => self.block(block),
            ElseHandler::Expr(expr) => self.expr(expr),
            ElseHandler::Handler { pattern, body } => {
                self.scopes.push(Vec::new());
                declare_pattern_classes(pattern, self);
                let out = self.expr(body);
                self.scopes.pop();
                out
            }
        }
    }

    /// E0412/E0413 — every *literal* format spec is comptime-known (s38), so
    /// a malformed one, or a well-formed one that cannot fit the hole's
    /// class, rejects at the literal. A spec with interpolated parts
    /// (`{x:>{w}}`) or a hole whose class this rung cannot see is left to
    /// run time — the walk never guesses.
    fn str_lit(&mut self, lit: &StrLit) -> Option<Diag> {
        for part in &lit.parts {
            let StrPart::Interp(interp) = part else {
                continue;
            };
            if let Some(diag) = self.expr(&interp.expr) {
                return Some(diag);
            }
            let Some(parts) = &interp.format else {
                continue;
            };
            let mut text = String::new();
            let mut literal_only = true;
            for fmt_part in parts {
                match fmt_part {
                    crate::ast::FmtPart::Text(t) => text.push_str(t),
                    crate::ast::FmtPart::Interp(inner) => {
                        literal_only = false;
                        if let Some(diag) = self.expr(inner) {
                            return Some(diag);
                        }
                    }
                }
            }
            if !literal_only {
                continue;
            }
            // The diagnostic spans the `:spec` — from the colon after the
            // hole's expression through the spec's last byte, `}` excluded
            // (observed at pin 13b811f: `[530,534]` = `:>08` on
            // `format_spec_malformed.lu`, `[414,417]` = `:.2` on
            // `format_spec_mismatch.lu`). 0.1.5 spanned the whole hole;
            // realigned for span parity at the re-pin.
            let spec_span = Span::new(
                interp.expr.span.end,
                interp.span.end.saturating_sub(1).max(interp.expr.span.end),
            );
            let spec = match crate::fmtspec::parse(&text) {
                Ok(spec) => spec,
                Err(error) => {
                    return Some(Diag::new(
                        "E0412",
                        spec_span,
                        "str.interp",
                        format!("malformed format spec `{text}`: {}", error.message()),
                    ));
                }
            };
            let class = match self.classify(&interp.expr) {
                LitClass::Str => Some(crate::fmtspec::HoleClass::Str),
                LitClass::Bool => Some(crate::fmtspec::HoleClass::Bool),
                LitClass::Int => Some(crate::fmtspec::HoleClass::Int),
                LitClass::Float => Some(crate::fmtspec::HoleClass::Float),
                LitClass::Raw | LitClass::Unknown => None,
            };
            if let Some(class) = class
                && let Err(mismatch) = crate::fmtspec::validate(&spec, class)
            {
                return Some(Diag::new(
                    "E0413",
                    spec_span,
                    "str.interp",
                    format!(
                        "format spec `{text}` does not fit this hole: {}",
                        mismatch.message()
                    ),
                ));
            }
        }
        None
    }
}

/// E1301's one shape: the operation named, the ring stated, no moralizing —
/// the raw tier is simpler, not scarier (D11).
fn ring_diag(what: &str, span: Span) -> Diag {
    Diag::new(
        "E1301",
        span,
        "mem.unsafe.scope",
        format!(
            "{what} needs an `unsafe` block: raw pointers are inert data anywhere — only the \
             tier's operations need the ring. Wrap this in `unsafe {{ }}` and state the \
             invariant in a `# Safety:` comment"
        ),
    )
}

/// Is this type literally `bool`?
fn is_bool_type(ty: &Type) -> bool {
    matches!(&*ty.kind, TypeKind::Path { path, args }
        if args.is_empty() && path.is_single() && path.segments[0].name == "bool")
}

/// Is this type literally `str`?
fn is_str_type(ty: &Type) -> bool {
    matches!(&*ty.kind, TypeKind::Path { path, args }
        if args.is_empty() && path.is_single() && path.segments[0].name == "str")
}

/// Pattern bindings enter the scope as `Unknown` — honest ignorance beats a
/// wrong class.
/// What an `else` handler pattern covers, for E0809.
enum Cover {
    /// A binder or wildcard: the whole row, whatever it is.
    All,
    /// Exactly these tags.
    Tags(std::collections::BTreeSet<String>),
    /// A shape this rung declines to judge (qualified paths, literals,
    /// tuples, capitalized names outside the row). Never diagnosed.
    Opaque,
}

/// The tag set an `else` handler pattern covers, against a known row.
fn handler_cover(pattern: &Pattern, row: &[String]) -> Cover {
    match &*pattern.kind {
        PatKind::Wildcard => Cover::All,
        PatKind::Binding(ident) => {
            let name = &ident.name;
            if row.iter().any(|tag| tag == name) {
                // A bare name that IS a row tag is a row-tag pattern — the
                // machine's own pattern resolution rule, statically.
                Cover::Tags(std::iter::once(name.clone()).collect())
            } else if name.starts_with(char::is_lowercase) {
                // `else |err| …`: a binder covers the row entire.
                Cover::All
            } else {
                Cover::Opaque
            }
        }
        PatKind::Variant { path, .. } => match path.segments.as_slice() {
            [segment] => Cover::Tags(std::iter::once(segment.name.clone()).collect()),
            _ => Cover::Opaque,
        },
        PatKind::At { pattern, .. } => handler_cover(pattern, row),
        PatKind::Or(alternatives) => {
            let mut tags = std::collections::BTreeSet::new();
            for alternative in alternatives {
                match handler_cover(alternative, row) {
                    Cover::All => return Cover::All,
                    Cover::Opaque => return Cover::Opaque,
                    Cover::Tags(sub) => tags.extend(sub),
                }
            }
            Cover::Tags(tags)
        }
        PatKind::Literal(_) | PatKind::Tuple(_) => Cover::Opaque,
    }
}

fn declare_pattern_classes(pattern: &Pattern, walk: &mut TierWalk<'_>) {
    match &*pattern.kind {
        PatKind::Binding(ident) => walk.declare(&ident.name, LitClass::Unknown),
        PatKind::Variant { fields, .. } => {
            for field in fields {
                declare_pattern_classes(field, walk);
            }
        }
        PatKind::Tuple(items) => {
            for item in items {
                declare_pattern_classes(item, walk);
            }
        }
        PatKind::At { name, pattern } => {
            walk.declare(&name.name, LitClass::Unknown);
            declare_pattern_classes(pattern, walk);
        }
        _ => {}
    }
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
    body_walk(program, true, false, false).1
}

fn body_walk(
    program: &Program,
    raises: bool,
    modes: bool,
    moves: bool,
) -> (Option<Diag>, Option<String>) {
    // The signature map of the mode pass: every function item of every
    // module, keyed `(module, name)`, valued by its parameters' declared
    // modes. Visibility is E0304's business and ran earlier in the chain.
    let sigs = modes.then(|| {
        let mut sigs = SigMap::new();
        for (module_name, module) in &program.modules {
            for (item_name, (def, _)) in &module.items {
                if let Def::Fn(decl) = def {
                    let params = decl
                        .params
                        .iter()
                        .map(|param| {
                            let name = match &param.kind {
                                crate::ast::ParamKind::Named { name, .. } => name.name.clone(),
                                crate::ast::ParamKind::SelfParam { .. } => "self".to_owned(),
                            };
                            (param.mode, name)
                        })
                        .collect();
                    sigs.insert((module_name.clone(), item_name.clone()), params);
                }
            }
        }
        sigs
    });
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
                modes: sigs.clone().map(|sigs| ModeCtx {
                    module: module.name.clone(),
                    sigs,
                }),
                moves: moves.then(MoveCtx::default),
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
                    modes: sigs.clone().map(|sigs| ModeCtx {
                        module: module.name.clone(),
                        sigs,
                    }),
                    moves: moves.then(MoveCtx::default),
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
    /// Present when the walk is the call-site mode check ([`mode_check`]):
    /// the module the walked body lives in, and every function item's
    /// declared parameter modes.
    modes: Option<ModeCtx>,
    /// Present when the walk is the take-mode reuse check ([`move_check`]):
    /// the locals whose whole binding a call-site `take` marker consumed,
    /// by name, valued by the move site (the `take` argument's span).
    moves: Option<MoveCtx>,
}

/// The take-mode reuse check's state: which names are moved-from, and where
/// each moved. Straight-line only — the walk snapshots and restores this map
/// around every construct whose body is not certainly on the path
/// (branches, loop bodies, handlers, `select`/`when` arms), and hands
/// latent bodies (closures, `defer`, nested `fn` items) a fresh map: their
/// straight line is their own.
#[derive(Default)]
struct MoveCtx {
    moved: BTreeMap<String, Span>,
}

/// One declared parameter of a visible signature: its mode and its name.
type SigParam = (Option<ParamMode>, String);

/// The signature map of the mode pass: `(module, fn)` → the declared
/// parameter row.
type SigMap = BTreeMap<(String, String), Vec<SigParam>>;

/// What the mode check resolves a callee against: `(module, fn)` → the
/// declared `(mode, parameter name)` row of the signature.
#[derive(Clone)]
struct ModeCtx {
    module: String,
    sigs: SigMap,
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
        // A fresh binding is a fresh thing: shadowing (or any pattern
        // rebinding the name) ends the moved-from story the old binding
        // carried ([`move_check`]).
        if let Some(ctx) = &mut self.moves {
            ctx.moved.remove(name);
        }
    }

    /// The moved-map as it stands, for restore after a body the straight
    /// line does not certainly reach ([`move_check`]); `None` outside the
    /// move pass.
    fn moved_snapshot(&self) -> Option<BTreeMap<String, Span>> {
        self.moves.as_ref().map(|ctx| ctx.moved.clone())
    }

    fn moved_restore(&mut self, snapshot: Option<BTreeMap<String, Span>>) {
        if let (Some(ctx), Some(moved)) = (&mut self.moves, snapshot) {
            ctx.moved = moved;
        }
    }

    /// Re-initialization by assignment makes the place live again
    /// (`[mem.tier0.move.4]`); any projected write clears the head — the
    /// conservative direction for a check that only ever refuses.
    fn moved_clear(&mut self, name: &str) {
        if let Some(ctx) = &mut self.moves {
            ctx.moved.remove(name);
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
            if let ExprKind::Path(path) = &*place.kind
                && let Some(head) = path.segments.first()
            {
                env.moved_clear(&head.name);
            }
            None
        }
        StmtKind::Defer { expr, .. } => {
            // A defer body is latent (it runs at scope exit): its straight
            // line is its own — fresh moved-map in, outer map untouched.
            let outer = env.moves.take();
            if outer.is_some() {
                env.moves = Some(MoveCtx::default());
            }
            let diag = walk_expr_assigns(expr, env);
            env.moves = outer;
            diag
        }
        StmtKind::AssumeNoalias(operands) => {
            operands.iter().find_map(|op| walk_expr_assigns(op, env))
        }
        StmtKind::Expr(expr) => walk_expr_assigns(expr, env),
        StmtKind::Item(item) => match &item.kind {
            ItemKind::Fn(decl) => {
                let mut nested = Env {
                    scopes: vec![env.scopes.first().cloned().unwrap_or_default()],
                    raise: env.raise.take(),
                    modes: env.modes.take(),
                    // A nested fn's body is latent: the enclosing straight
                    // line's moves do not apply inside it, and its own moves
                    // never leak out — a fresh map either way.
                    moves: env.moves.is_some().then(MoveCtx::default),
                };
                let diag = walk_fn_assigns(decl, &mut nested);
                env.raise = nested.raise;
                env.modes = nested.modes;
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
        | ExprKind::Char(_)
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
        | ExprKind::RegionSugar { body: block, .. }
        | ExprKind::Scope { body: block, .. }
        | ExprKind::Unsafe { body: block } => walk_block_assigns(block, env),
        ExprKind::Loop { body } => {
            // A loop body re-runs: its moves are not straight-line facts for
            // the code after (or before, next iteration) — see `MoveCtx`.
            let snapshot = env.moved_snapshot();
            let diag = walk_block_assigns(body, env);
            env.moved_restore(snapshot);
            diag
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr_assigns(lhs, env).or_else(|| walk_expr_assigns(rhs, env))
        }
        ExprKind::Call { callee, args } => walk_expr_assigns(callee, env)
            .or_else(|| {
                args.iter()
                    .find_map(|arg| walk_expr_assigns(&arg.expr, env))
            })
            .or_else(|| check_call_modes(callee, args, env))
            .or_else(|| check_call_moves(args, env)),
        ExprKind::BracketApply { base, args, .. } => walk_expr_assigns(base, env).or_else(|| {
            args.iter().find_map(|arg| match arg {
                crate::ast::IndexArg::Value(arg) => walk_expr_assigns(&arg.expr, env),
                crate::ast::IndexArg::Type(_) => None,
            })
        }),
        ExprKind::Member { base, .. } => walk_expr_assigns(base, env),
        ExprKind::ModedReceiver { place, mode } => {
            walk_expr_assigns(place, env).or_else(|| check_marked_place(place, *mode, env))
        }
        ExprKind::Range { start, end, .. } => start
            .as_ref()
            .and_then(|s| walk_expr_assigns(s, env))
            .or_else(|| end.as_ref().and_then(|e| walk_expr_assigns(e, env))),
        ExprKind::ElseDefault {
            expr: inner,
            handler,
        } => walk_expr_assigns(inner, env).or_else(|| {
            // The handler runs only on the error path: not straight-line.
            let snapshot = env.moved_snapshot();
            let diag = match &**handler {
                crate::ast::ElseHandler::Block(block) => walk_block_assigns(block, env),
                crate::ast::ElseHandler::Expr(expr) => walk_expr_assigns(expr, env),
                crate::ast::ElseHandler::Handler { pattern, body } => {
                    env.scopes.push(Vec::new());
                    declare_pattern(pattern, true, env);
                    let diag = walk_expr_assigns(body, env);
                    env.scopes.pop();
                    diag
                }
            };
            env.moved_restore(snapshot);
            diag
        }),
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => walk_expr_assigns(cond, env)
            .or_else(|| {
                let snapshot = env.moved_snapshot();
                let diag = walk_block_assigns(then, env);
                env.moved_restore(snapshot);
                diag
            })
            .or_else(|| {
                let snapshot = env.moved_snapshot();
                let diag = otherwise.as_ref().and_then(|e| walk_expr_assigns(e, env));
                env.moved_restore(snapshot);
                diag
            }),
        ExprKind::Match { scrutinee, arms } => walk_expr_assigns(scrutinee, env).or_else(|| {
            arms.iter().find_map(|arm| {
                env.scopes.push(Vec::new());
                let snapshot = env.moved_snapshot();
                declare_pattern(&arm.pattern, true, env);
                let diag = arm
                    .guard
                    .as_ref()
                    .and_then(|guard| walk_expr_assigns(guard, env))
                    .or_else(|| walk_expr_assigns(&arm.body, env));
                env.moved_restore(snapshot);
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
            let snapshot = env.moved_snapshot();
            declare_pattern(pattern, true, env);
            let diag = walk_block_assigns(body, env);
            env.moved_restore(snapshot);
            env.scopes.pop();
            diag
        }),
        ExprKind::While { cond, body } => {
            // Condition and body both re-run; neither leaves straight-line
            // facts behind — see `MoveCtx`.
            let snapshot = env.moved_snapshot();
            let diag = walk_expr_assigns(cond, env).or_else(|| walk_block_assigns(body, env));
            env.moved_restore(snapshot);
            diag
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
            // A closure body is latent — fresh moved-map in, outer map
            // untouched (see `MoveCtx`).
            let outer = env.moves.take();
            if outer.is_some() {
                env.moves = Some(MoveCtx::default());
            }
            for param in params {
                env.declare(&param.name.name, true);
            }
            let diag = walk_expr_assigns(body, env);
            env.moves = outer;
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
            let snapshot = env.moved_snapshot();
            let diag = match &arm.kind {
                crate::ast::SelectArmKind::Recv { pattern, channel } => {
                    let diag = walk_expr_assigns(channel, env);
                    declare_pattern(pattern, true, env);
                    diag
                }
                crate::ast::SelectArmKind::Timeout(expr) => walk_expr_assigns(expr, env),
            }
            .or_else(|| walk_expr_assigns(&arm.body, env));
            env.moved_restore(snapshot);
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
                    // The body runs when the locks land, not on this line:
                    // its moves are not straight-line facts (see `MoveCtx`).
                    let snapshot = env.moved_snapshot();
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
                    env.moved_restore(snapshot);
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

/// One call site against one visible signature — [`mode_check`]'s working
/// half. `None` is "nothing to say": an invisible signature is *skipped*,
/// never guessed at, so every emission here is a disagreement between an
/// argument's spelling and a declaration the resolve rung actually resolved.
fn check_call_modes(callee: &Expr, args: &[Arg], env: &Env) -> Option<Diag> {
    let ctx = env.modes.as_ref()?;
    // `f[T](…)` — generic application is one postfix form; the callee under
    // the brackets is still the named function.
    let mut base = callee;
    if let ExprKind::BracketApply { base: b, .. } = &*base.kind {
        base = b;
    }
    let ExprKind::Path(path) = &*base.kind else {
        return None;
    };
    // A local binding shadowing the head makes the callee a value; its
    // signature is not this rung's to see.
    if let Some(head) = path.segments.first()
        && env.assignable(&head.name).is_some()
    {
        return None;
    }
    let (fn_name, key) = match path.segments.as_slice() {
        [single] => (
            single.name.as_str(),
            (ctx.module.clone(), single.name.clone()),
        ),
        // `module.fn` — the head must actually be a module for the lookup to
        // land; a trait-qualified call (`Speak.speak(d)`) or an enum path
        // simply finds no signature and is skipped.
        [head, member] => (
            member.name.as_str(),
            (head.name.clone(), member.name.clone()),
        ),
        _ => return None,
    };
    let sig = ctx.sigs.get(&key)?;
    for (arg, (declared, param)) in args.iter().zip(sig) {
        if arg.mode == *declared {
            continue;
        }
        // Message and span shapes match the counterparty's E1007, observed
        // at pin ad6cef7: the primary span is the argument's expression.
        let (anchor, message) = match (declared, arg.mode) {
            (Some(mode), None) => (
                mode_anchor(*mode),
                format!(
                    "`{fn_name}` declares `{param}` as `{}`, but the call site does not say so",
                    mode_word(*mode)
                ),
            ),
            (None, Some(_)) => (
                "mem.tier0.mode.read",
                format!("`{fn_name}` takes `{param}` as plain `read` — no mode is written for it"),
            ),
            (Some(mode), Some(given)) => (
                mode_anchor(*mode),
                format!(
                    "`{fn_name}` declares `{param}` as `{}`, but the call site says `{}`",
                    mode_word(*mode),
                    mode_word(given)
                ),
            ),
            (None, None) => unreachable!("equal modes were skipped above"),
        };
        return Some(Diag::new("E1007", arg.expr.span, anchor, message));
    }
    None
}

/// [`move_check`]'s call-argument half: every explicitly moded argument is a
/// USE of its place, and a `take`-moded one is also a MOVE of it. A bare
/// (read-moded) argument is neither — reads stay the dynamic trap's business
/// (`[mem.tier0.move.2]`'s interpreter meaning), which is what keeps
/// `memory/move_use_after.lu` and the faults tier on their pinned verdicts.
fn check_call_moves(args: &[Arg], env: &mut Env) -> Option<Diag> {
    for arg in args {
        let Some(mode) = arg.mode else { continue };
        if let Some(diag) = check_marked_place(&arg.expr, mode, env) {
            return Some(diag);
        }
    }
    None
}

/// One explicitly moded place — a marked call argument or a moded receiver —
/// against the moved-map: a marker over a moved-from whole binding is E1001
/// at the place expression (the counterparty's span, observed at `addcd7f`);
/// a `take` marker over a live one records the move. Only a bare
/// single-segment path naming a binding the walk has seen participates —
/// field paths, projections and unseen names are skipped, never guessed at.
fn check_marked_place(place: &Expr, mode: ParamMode, env: &mut Env) -> Option<Diag> {
    env.moves.as_ref()?;
    let ExprKind::Path(path) = &*place.kind else {
        return None;
    };
    if !path.is_single() {
        return None;
    }
    let head = &path.segments[0].name;
    env.assignable(head)?;
    let ctx = env.moves.as_mut()?;
    if ctx.moved.contains_key(head) {
        return Some(Diag::new(
            "E1001",
            place.span,
            "mem.tier0.move.2",
            format!(
                "`{head}` is used here after its value moved away — a `take` argument consumed \
                 it; re-initializing the place (assigning to it) makes it usable again, and \
                 `take copy {head}` at the move keeps the original"
            ),
        ));
    }
    if mode == ParamMode::Take {
        ctx.moved.insert(head.clone(), place.span);
    }
    None
}

const fn mode_word(mode: ParamMode) -> &'static str {
    match mode {
        ParamMode::Mut => "mut",
        ParamMode::Take => "take",
    }
}

const fn mode_anchor(mode: ParamMode) -> &'static str {
    match mode {
        ParamMode::Mut => "mem.tier0.mode.mut",
        ParamMode::Take => "mem.tier0.mode.take",
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
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Char(_)
        | ExprKind::Wildcard => {}
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
        ExprKind::BracketApply { base, args, .. } => {
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

    // -- `[conf.directive.standalone]` (D59): the standalone set ------------

    #[test]
    fn the_standalone_set_is_the_four_d59_spellings() {
        let p = Path::new("dir/prog.lu");
        assert_eq!(
            standalone_mark(
                p,
                None,
                "//! member: false\nfn main() -> int { 0 }\n",
                false
            ),
            Some("`//! member: false`")
        );
        assert_eq!(
            standalone_mark(p, None, "//! check: pass\n//! phase: parse\n", false),
            Some("the `//! check:` + `//! phase:` entry pair")
        );
        assert_eq!(
            standalone_mark(
                p,
                None,
                "#!/usr/bin/env lupin\nfn main() -> int { 0 }\n",
                false
            ),
            Some("a `#!` script line")
        );
        assert_eq!(
            standalone_mark(
                p,
                None,
                "pkg { name: \"o/p\" }\nfn main() -> int { 0 }\n",
                false
            ),
            Some("a `pkg { … }` frontmatter block")
        );
        assert_eq!(
            standalone_mark(
                p,
                None,
                "// a comment above\n\npkg {\n}\nfn main() -> int { 0 }\n",
                false
            ),
            Some("a `pkg { … }` frontmatter block")
        );
        assert_eq!(
            standalone_mark(
                Path::new("dir/foo_test.lu"),
                None,
                "fn main() -> int { 0 }\n",
                false
            ),
            Some("a `_test.lu` file name")
        );
        // Membership is the default: a plain file carries no mark.
        assert_eq!(
            standalone_mark(p, None, "fn helper() -> int { 1 }\n", false),
            None
        );
        // `pkg` as an ordinary identifier deeper in the file announces nothing.
        assert_eq!(
            standalone_mark(p, None, "fn pkg_count() -> int { 1 }\n", false),
            None
        );
    }

    #[test]
    fn an_explicit_member_key_always_decides() {
        let p = Path::new("dir/prog.lu");
        // `member: true` joins even an entry-shaped header.
        assert_eq!(
            standalone_mark(
                p,
                None,
                "//! member: true\n//! prose, not directives\nfn f() -> int { 1 }\n",
                false
            ),
            None
        );
        // …and even a `_test.lu` name.
        assert_eq!(
            standalone_mark(
                Path::new("dir/foo_test.lu"),
                None,
                "//! member: true\nfn f() -> int { 1 }\n",
                false
            ),
            None
        );
    }

    #[test]
    fn a_file_wide_attribute_is_not_a_script_announcement() {
        // `[gram.lex.shebang]` narrowed: `#![` opens the file-wide attribute,
        // so it must not read as a `#!` script line.
        assert_eq!(
            standalone_mark(
                Path::new("dir/prog.lu"),
                None,
                "#![index(1)]\nfn main() -> int { 0 }\n",
                false
            ),
            None
        );
    }

    #[test]
    fn the_named_entry_and_the_std_tree_are_never_excluded() {
        let p = Path::new("dir/prog.lu");
        // The named entry always belongs to its own root module.
        assert_eq!(
            standalone_mark(
                p,
                Some(p),
                "//! member: false\nfn main() -> int { 0 }\n",
                false
            ),
            None
        );
        // std/dep trees stay whole-package: every file participates.
        assert_eq!(
            standalone_mark(p, None, "//! member: false\nfn main() -> int { 0 }\n", true),
            None
        );
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

    // -- E1007: the call-site mode law at the resolve rung (issue #15) ------

    #[test]
    fn a_missing_call_site_mut_is_e1007_at_the_argument() {
        // `corpus/memory/mode_missing_mut.lu`'s shape. The span is the
        // argument's expression, matching the counterparty at pin ad6cef7.
        let source = "struct P { x: int, y: int }\n\
                      fn bump(mut n: int) { n += 1 }\n\
                      fn main() -> !int {\n    var p = P { x: 1, y: 2 }\n    \
                      bump(p.x)\n    if p.x == 2 { 0 } else { 1 }\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E1007");
        assert_eq!(&source[diag.span.start..diag.span.end], "p.x");
    }

    #[test]
    fn the_books_ch07_repro_is_rejected_not_run_to_a_wrong_answer() {
        // wolf-book ch07 §7.4's failing program (bs03 ba:blocker): `add`
        // declares `mut s` and the call site does not say so. Running this
        // used to return a wrong answer silently.
        let source = "struct Doc { title: str, words: int }\n\
                      struct Shelf { docs: List[Doc] }\n\
                      fn add(mut s: Shelf, d: Doc) {\n    (mut s.docs).push(d)\n}\n\
                      fn main() -> !int {\n    \
                      var shelf = Shelf { docs: List[Doc]() }\n    \
                      add(shelf, Doc { title: \"regions\", words: 900 })\n    \
                      print(\"{shelf.docs.len}\")\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E1007");
        assert_eq!(&source[diag.span.start..diag.span.end], "shelf");
    }

    #[test]
    fn an_extra_mode_and_a_wrong_mode_word_are_e1007_too() {
        // The counterparty's other three fixture shapes (e1007_extra_mut,
        // e1007_missing_take, e1007_take_where_mut), probed at ad6cef7.
        let extra = "fn look(n: int) -> int { n }\n\
                     fn main() -> !int {\n    var x = 1\n    look(mut x)\n    0\n}\n";
        let diag = resolve(extra).expect("rejected");
        assert_eq!(diag.code, "E1007");
        let take = "fn eat(take n: int) -> int { n }\n\
                    fn main() -> !int {\n    var x = 1\n    eat(x)\n    0\n}\n";
        let diag = resolve(take).expect("rejected");
        assert_eq!(diag.code, "E1007");
        let wrong = "fn bump(mut n: int) { n += 1 }\n\
                     fn main() -> !int {\n    var x = 1\n    bump(take x)\n    0\n}\n";
        let diag = resolve(wrong).expect("rejected");
        assert_eq!(diag.code, "E1007");
    }

    #[test]
    fn spelled_modes_and_invisible_signatures_stay_clean() {
        // Agreement passes; a locally shadowed name is a *value* whose
        // signature this rung cannot see, so it is skipped, not guessed at.
        assert!(
            resolve(
                "fn bump(mut n: int) { n += 1 }\n\
                 fn eat(take n: int) -> int { n }\n\
                 fn main() -> !int {\n    var x = 1\n    bump(mut x)\n    \
                 let y = eat(take x)\n    let bump = fn(n: int) { n }\n    \
                 let z = bump(y)\n    z - z\n}\n"
            )
            .is_none()
        );
    }

    // -- E1001: take-mode reuse at the resolve rung (issue #48) -------------

    #[test]
    fn a_take_marked_reuse_is_e1001_at_the_second_argument() {
        // wolf-std `process/use_after_wait.lu`'s shape: the same binding
        // handed to a `take` marker twice. Span: the reuse argument's
        // identifier, matching the counterparty at pin addcd7f.
        let source = "struct S { n: int }\n\
                      fn eat(take s: S) -> int { s.n }\n\
                      fn main() -> !int {\n    var s = S { n: 1 }\n    \
                      let a = eat(take s)\n    let b = eat(take s)\n    a + b\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E1001");
        assert_eq!(diag.anchor, "mem.tier0.move.2");
        // The SECOND `s`-argument, not the first.
        let reuse = source.rfind("take s").expect("spelled") + "take ".len();
        assert_eq!((diag.span.start, diag.span.end), (reuse, reuse + 1));
    }

    #[test]
    fn a_mut_marked_reuse_after_a_take_is_e1001_too() {
        // wolf-std `net/use_after_close.lu`'s shape: `close(take cli)` then
        // `write(mut cli, …)` — any explicit marker is a USE of the place.
        let source = "struct S { n: int }\n\
                      fn close(take s: S) -> int { s.n }\n\
                      fn poke(mut s: S) -> int {\n    s.n += 1\n    s.n\n}\n\
                      fn main() -> !int {\n    var s = S { n: 1 }\n    \
                      let a = close(take s)\n    let b = poke(mut s)\n    a + b\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E1001");
        assert_eq!(&source[diag.span.start..diag.span.end], "s");
        assert!(source[..diag.span.start].ends_with("poke(mut "));
    }

    #[test]
    fn reinitialization_and_shadowing_make_the_place_live_again() {
        // `[mem.tier0.move.4]`: assignment re-initializes; a fresh binding
        // is a fresh thing.
        assert!(
            resolve(
                "struct S { n: int }\n\
                 fn eat(take s: S) -> int { s.n }\n\
                 fn main() -> !int {\n    var s = S { n: 1 }\n    \
                 let a = eat(take s)\n    s = S { n: 2 }\n    \
                 let b = eat(take s)\n    a + b\n}\n"
            )
            .is_none()
        );
        assert!(
            resolve(
                "struct S { n: int }\n\
                 fn eat(take s: S) -> int { s.n }\n\
                 fn main() -> !int {\n    let s = S { n: 1 }\n    \
                 let a = eat(take s)\n    let s = S { n: 2 }\n    \
                 let b = eat(take s)\n    a + b\n}\n"
            )
            .is_none()
        );
    }

    #[test]
    fn a_bare_read_after_a_take_stays_the_dynamic_traps_business() {
        // `memory/move_use_after.lu`'s shape: the reuse is an unmarked READ.
        // The static rung says nothing — `[mem.tier0.move.2]`'s dynamic
        // meaning (trap `use-after-move`) is the interpreter's answer, and
        // the corpus faults tier pins it.
        assert!(
            resolve(
                "struct Big { data: int }\n\
                 fn consume(take b: Big) -> int { b.data }\n\
                 fn main() -> !int {\n    let b = Big { data: 2 }\n    \
                 let n = consume(take b)\n    let m = b.data\n    n + m\n}\n"
            )
            .is_none()
        );
    }

    #[test]
    fn field_takes_and_branch_moves_are_not_tracked() {
        // `faults/use_after_move_field.lu`'s granularity stays dynamic: a
        // field-path take is skipped, never guessed at.
        assert!(
            resolve(
                "struct Inner { n: int }\n\
                 struct P { x: Inner, y: Inner }\n\
                 fn eat(take i: Inner) -> int { i.n }\n\
                 fn poke(mut i: Inner) -> int {\n    i.n += 1\n    i.n\n}\n\
                 fn main() -> !int {\n    \
                 var p = P { x: Inner { n: 1 }, y: Inner { n: 2 } }\n    \
                 let a = eat(take p.x)\n    let b = poke(mut p.x)\n    a + b\n}\n"
            )
            .is_none()
        );
        // A move inside a branch (or a loop body) never leaks past it: the
        // walk refuses only what is certain in straight-line source order.
        assert!(
            resolve(
                "struct S { n: int }\n\
                 fn eat(take s: S) -> int { s.n }\n\
                 fn poke(mut s: S) -> int {\n    s.n += 1\n    s.n\n}\n\
                 fn main() -> !int {\n    var s = S { n: 1 }\n    \
                 var a = 0\n    if s.n == 2 { a = eat(take s) }\n    \
                 let b = poke(mut s)\n    a + b\n}\n"
            )
            .is_none()
        );
        assert!(
            resolve(
                "struct S { n: int }\n\
                 fn eat(take s: S) -> int { s.n }\n\
                 fn main() -> !int {\n    var total = 0\n    \
                 for i in 0..3 {\n        var s = S { n: i }\n        \
                 total += eat(take s)\n    }\n    total - total\n}\n"
            )
            .is_none()
        );
    }

    #[test]
    fn a_taken_receiver_is_a_move_and_a_marked_receiver_after_it_is_e1001() {
        // `(take c).finish()` consumes the whole binding exactly as a
        // `take` argument does; a later moded receiver is the reuse.
        let source = "struct Counter { n: int }\n\
                      impl Counter {\n    \
                      fn bump(mut self) -> int {\n        self.n += 1\n        self.n\n    }\n    \
                      fn finish(take self) -> int {\n        self.n\n    }\n}\n\
                      fn main() -> !int {\n    var c = Counter { n: 1 }\n    \
                      let z = (take c).finish()\n    let a = (mut c).bump()\n    z + a\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E1001");
        assert!(source[..diag.span.start].ends_with("(mut "));
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

    // ---- the pin-f0da6e6 tier statics (issue #18) -------------------------

    #[test]
    fn a_c_call_outside_the_ring_is_e1301_at_the_call() {
        // `corpus/memory/unsafe_raw_outside.lu`'s first offender; the span is
        // the whole call, matching the counterparty at pin f0da6e6
        // ([384,395] there — `c.malloc(8)`).
        let source = "import c \"stdlib.h\"\n\nfn main() -> !int {\n    \
                      let p = c.malloc(8) as *u8\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E1301");
        assert_eq!(&source[diag.span.start..diag.span.end], "c.malloc(8)");
    }

    #[test]
    fn a_raw_write_and_a_raw_read_outside_the_ring_are_e1301_at_the_place() {
        // The write: span is the place (`p[0]`), the counterparty's shape.
        let source = "import c \"stdlib.h\"\n\nfn main() -> !int {\n    \
                      let p = unsafe { c.malloc(8) as *u8 }\n    p[0] = 1\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E1301");
        assert_eq!(&source[diag.span.start..diag.span.end], "p[0]");

        // The read, through the book's laundering shape (ch09 exercise 9-4):
        // binding a pointer OUT of an unsafe block is free; reading through
        // it outside the ring is the op.
        let source = "import c \"stdlib.h\"\n\nfn main() -> !int {\n    \
                      let p = unsafe { c.malloc(8) as *u8 }\n    let x = p[0]\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E1301");
        assert_eq!(&source[diag.span.start..diag.span.end], "p[0]");
    }

    #[test]
    fn the_ring_ops_inside_unsafe_are_clean() {
        let source = "import c \"stdlib.h\"\n\nfn main() -> !int {\n    unsafe {\n        \
                      let p = c.malloc(8) as *u8\n        p[0] = 1\n        \
                      let x = p[0]\n        c.free(p)\n    }\n    0\n}\n";
        assert_eq!(resolve(source), None);
    }

    #[test]
    fn an_int_to_pointer_cast_outside_the_ring_is_e1301() {
        // Forging provenance from an integer is the exposed-door cast;
        // retyping an already-raw value is inert (the counterparty flags the
        // allocator call, never `… as *u8` over it — observed at f0da6e6).
        let source = "fn main() -> !int {\n    let p = 42 as *u8\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E1301");
    }

    #[test]
    fn a_raw_pointer_signature_is_e1302_at_the_parameter_name() {
        // `corpus/memory/unsafe_sig.lu`: span [329,330] there is the `p` of
        // `fn peek(p: *u8)` — the parameter NAME, the counterparty's choice.
        let source = "fn peek(p: *u8) -> int {\n    0\n}\n\nfn main() -> !int {\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E1302");
        assert_eq!(&source[diag.span.start..diag.span.end], "p");
    }

    #[test]
    fn a_cast_to_bool_is_e0805_at_the_cast_expression() {
        // The cast matrix's bool column (issue #18 item 2). Observed at pin
        // f0da6e6: the whole cast expression, in unsafe code too.
        let source = "fn main() -> !int {\n    let n = 3\n    let b = n as bool\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E0805");
        assert_eq!(&source[diag.span.start..diag.span.end], "n as bool");
    }

    #[test]
    fn str_char_indexing_is_e0411_and_slicing_is_not() {
        // `corpus/strings/char_index_fail.lu`'s shape (D25).
        let source = "fn main() -> !int {\n    let s = \"wolf\"\n    let i = 2\n    \
                      let c = s[i]\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E0411");
        assert_eq!(&source[diag.span.start..diag.span.end], "s[i]");

        let sliced = "fn main() -> !int {\n    let s = \"wolf\"\n    \
                      let h = s[0..2]\n    0\n}\n";
        assert_eq!(resolve(sliced), None);
    }

    #[test]
    fn a_malformed_literal_spec_is_e0412_and_a_mismatched_one_is_e0413() {
        // `corpus/strings/format_spec_malformed.lu`: the zero FLAG after an
        // explicit alignment; comptime-known, so it rejects at the literal.
        let source = "fn main() -> !int {\n    let n = 42\n    \
                      print(\"[{n:>08}]\")\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E0412");

        // `corpus/strings/format_spec_mismatch.lu`: `.2` on an integer hole.
        let source = "fn main() -> !int {\n    let n = 42\n    \
                      print(\"[{n:.2}]\")\n    0\n}\n";
        let diag = resolve(source).expect("rejected");
        assert_eq!(diag.code, "E0413");

        // `{n:0>8}` is fill `0` + align — legal; an unknown hole class is
        // never guessed at; a dynamic-width spec is left to run time.
        for clean in [
            "fn main() -> !int {\n    let n = 42\n    print(\"[{n:0>8}]\")\n    0\n}\n",
            "fn f(x: List[int]) -> str { \"{x:.2}\" }\nfn main() -> !int {\n    0\n}\n",
            "fn main() -> !int {\n    let n = 42\n    let w = 8\n    \
             print(\"[{n:>{w}}]\")\n    0\n}\n",
        ] {
            assert_eq!(resolve(clean), None, "{clean}");
        }
    }
}

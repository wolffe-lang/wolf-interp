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
//! What deliberately does *not* live here: the E03xx family. A duplicate
//! definition is recorded as [`Def::Ambiguous`] and only bites if the program
//! actually calls it — "a sema-lite failure (unresolvable name, ambiguous
//! dispatch) is verdict `unsupported` … never a crash". A program the compiler
//! rejects (unused import, import cycle) but this machine runs clean is the
//! *expected* static-conservatism class, ledgered rather than papered over.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::ast::{FnDecl, ItemKind, StructDef, Unit};
use crate::diag::Diag;

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
    let package_root = entry.parent().unwrap_or(Path::new(".")).to_path_buf();
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
    collect(&parsed.unit, &mut module);
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
        .filter(|path| path.is_file() && path.extension().is_some_and(|e| e == "lu"))
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
        files.push(crate::slash_path(&path));
        collect(&parsed.unit, &mut module);
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

fn collect(unit: &Unit, module: &mut Module) {
    for item in &unit.items {
        let visible = item.visibility.is_some();
        match &item.kind {
            ItemKind::Fn(decl) => {
                define(
                    module,
                    decl.name.name.clone(),
                    Def::Fn(decl.clone()),
                    visible,
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
                    ),
                    _ => define(module, alias.name.name.clone(), Def::Opaque("type"), true),
                }
            }
            ItemKind::Enum(def) => {
                if let Some(name) = &def.name {
                    define(module, name.name.clone(), Def::Opaque("enum"), true);
                }
            }
            ItemKind::Trait(def) => {
                define(module, def.name.name.clone(), Def::Opaque("trait"), true);
            }
            ItemKind::Impl(_) => {}
            ItemKind::Use(decl) => collect_use(decl, module),
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
}

fn collect_use(decl: &crate::ast::UseDecl, module: &mut Module) {
    // `use std.fs` and friends name the ambient prelude, not a directory; the
    // loader tries the directory and simply finds nothing.
    let head = decl
        .path
        .segments
        .first()
        .map(|s| s.name.clone())
        .unwrap_or_default();
    if decl.list.is_empty() {
        let bound = decl.alias.as_ref().map_or_else(
            || decl.path.segments.last().map(|s| s.name.clone()),
            |a| Some(a.name.clone()),
        );
        if let Some(bound) = bound {
            module.uses.push(bound);
        }
    } else {
        for item in &decl.list {
            module.uses.push(item.name.clone());
        }
    }
    if !module.uses.contains(&head) {
        module.uses.push(head);
    }
}

fn define(module: &mut Module, name: String, def: Def, visible: bool) {
    match module.items.entry(name) {
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert((def, visible));
        }
        std::collections::btree_map::Entry::Occupied(mut slot) => {
            // D32: every `.lu` file in a directory is ONE module, so a second
            // definition is a duplicate, not a shadow. The compiler rejects it
            // (E0302); this machine records the ambiguity and declines to
            // dispatch through it.
            slot.insert((Def::Ambiguous, visible));
        }
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

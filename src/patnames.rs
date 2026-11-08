//! What one pattern binds (s288; wolf-lang#1061, #1062 and #1073): the
//! compiler's E0827, E0828 and E0401, read by `rowmatch`'s body walk at every
//! position a pattern or a parameter list stands.
//!
//! - **`[gram.pat.names]`**: a pattern binds each name once. A name bound
//!   again anywhere in one pattern — two fields, two tuple elements, a payload
//!   and an enclosing `@`, both sides of `@`, at any depth — is **E0827** at
//!   the later binding, at one type or at two. A parameter list binds each
//!   name once too, a function's and a closure's. Only bindings count: a bare
//!   name that tests a case binds nothing, so `(Red, Red)` is two tests. Each
//!   alternative of an or-pattern is one pattern for this rule, and the
//!   or-pattern binds each of its names once in the pattern around it. A
//!   comma group (`let a = 1, a = a + 1`) is several patterns.
//! - **`[gram.pat.or]`**: every alternative of an or-pattern binds the same
//!   names, each at one type. A name one alternative binds and another does
//!   not is **E0828** at the alternative that lacks it, naming the name; a
//!   name bound at two types is **E0401** at the later binding.
//!
//! Which bare names test is this machine's evaluator's rule, so the checks and
//! the run cannot disagree: in a `match` arm, a name that spells a variant of
//! an enum the module declares, or, at the top of the arm, a tag of the row
//! the scrutinee is statically known to carry; in a binder position (`let`,
//! `var`, a `for` header, an `else |…|` handler, a `select` receive), none.
//!
//! A binding's type is read only from a declaration: a variant's payload at
//! its position, a struct's field, and a tuple of those. A type that mentions
//! a generic parameter or a type alias, or a position no declaration types,
//! is unknown, and an unknown type is never judged — the sema boundary's rule
//! that a guess never becomes a verdict.
//!
//! The pinned spec (`5f1f1c94`) has neither clause yet, so the diagnostics
//! cite `[gram.pat]` and name the clause in their words.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{
    ClosureParam, EnumDef, FieldPat, ItemKind, Param, ParamKind, PatKind, Pattern, StructDef, Type,
    TypeDef, TypeKind,
};
use crate::diag::{Diag, Span};
use crate::sema::Module;

/// The type declarations of one module, by name, with the names a type
/// written in them may mention that this reader cannot see through: the
/// declaration's generic parameters, and every alias the module declares.
pub struct TypeIndex<'a> {
    enums: BTreeMap<&'a str, (&'a EnumDef, BTreeSet<String>)>,
    structs: BTreeMap<&'a str, (&'a StructDef, BTreeSet<String>)>,
    aliases: BTreeSet<String>,
    variants: &'a BTreeMap<String, Vec<String>>,
}

impl<'a> TypeIndex<'a> {
    #[must_use]
    pub fn of(module: &'a Module) -> TypeIndex<'a> {
        let mut index = TypeIndex {
            enums: BTreeMap::new(),
            structs: BTreeMap::new(),
            aliases: BTreeSet::new(),
            variants: &module.variants,
        };
        for unit in &module.units {
            for item in &unit.unit.items {
                let generics = |list: &[crate::ast::GenericParam]| -> BTreeSet<String> {
                    list.iter().map(|g| g.name.name.clone()).collect()
                };
                match &item.kind {
                    ItemKind::Struct(def) => {
                        if let Some(name) = &def.name {
                            index
                                .structs
                                .entry(name.name.as_str())
                                .or_insert((def, generics(&def.generics)));
                        }
                    }
                    ItemKind::Enum(def) => {
                        if let Some(name) = &def.name {
                            index
                                .enums
                                .entry(name.name.as_str())
                                .or_insert((def, generics(&def.generics)));
                        }
                    }
                    ItemKind::TypeAlias(alias) => {
                        let mut opaque = generics(&alias.generics);
                        match &alias.def {
                            TypeDef::Struct(def) => {
                                opaque.extend(generics(&def.generics));
                                index
                                    .structs
                                    .entry(alias.name.name.as_str())
                                    .or_insert((def, opaque));
                            }
                            TypeDef::Enum(def) => {
                                opaque.extend(generics(&def.generics));
                                index
                                    .enums
                                    .entry(alias.name.name.as_str())
                                    .or_insert((def, opaque));
                            }
                            TypeDef::Alias(_) => {
                                index.aliases.insert(alias.name.name.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        index
    }

    /// A type as this reader knows it, or `None` when it mentions a name it
    /// cannot see through.
    fn known(&self, ty: &Type, opaque: &BTreeSet<String>) -> Option<String> {
        (!mentions(ty, opaque) && !mentions(ty, &self.aliases)).then(|| ty.spelling())
    }

    /// The payload types of the variant an arm's path names, by position.
    fn payload(&self, path: &crate::ast::Path) -> Vec<Option<String>> {
        let found = match path.segments.as_slice() {
            [variant] => match self.variants.get(&variant.name).map(Vec::as_slice) {
                Some([owner]) => self.variant(owner, &variant.name),
                _ => None,
            },
            [owner, variant] => self.variant(&owner.name, &variant.name),
            _ => None,
        };
        found.unwrap_or_default()
    }

    fn variant(&self, owner: &str, variant: &str) -> Option<Vec<Option<String>>> {
        let (def, opaque) = self.enums.get(owner)?;
        let variant = def.variants.iter().find(|v| v.name.name == variant)?;
        Some(
            variant
                .payload
                .iter()
                .map(|ty| self.known(ty, opaque))
                .collect(),
        )
    }

    /// The type of the field `field` of the struct a pattern's path names.
    fn field(&self, path: &crate::ast::Path, field: &str) -> Option<String> {
        let [name] = path.segments.as_slice() else {
            return None;
        };
        let (def, opaque) = self.structs.get(name.name.as_str())?;
        let field = def.fields.iter().find(|f| f.name.name == field)?;
        self.known(&field.ty, opaque)
    }
}

/// Whether `ty` mentions one of `names` anywhere, as a path's first segment.
fn mentions(ty: &Type, names: &BTreeSet<String>) -> bool {
    if names.is_empty() {
        return false;
    }
    match &*ty.kind {
        TypeKind::Path { path, args } => {
            path.segments
                .first()
                .is_some_and(|head| names.contains(&head.name))
                || args.iter().any(|arg| match arg {
                    crate::ast::TypeArg::Type(inner) => mentions(inner, names),
                    crate::ast::TypeArg::Expr(_) => false,
                })
        }
        TypeKind::ErrorUnion(inner)
        | TypeKind::Prefixed { ty: inner, .. }
        | TypeKind::RawPointer(inner) => mentions(inner, names),
        TypeKind::Fallible { ty: inner, row } => {
            mentions(inner, names)
                || row
                    .entries
                    .iter()
                    .flat_map(|entry| &entry.payload)
                    .any(|payload| mentions(payload, names))
        }
        TypeKind::Tuple(parts) => parts.iter().any(|part| mentions(part, names)),
        TypeKind::Fn { params, ret } => {
            params.iter().any(|param| mentions(param, names))
                || ret.as_ref().is_some_and(|ret| mentions(&ret.ty, names))
        }
        TypeKind::Dyn(_) | TypeKind::TypeOfTypes | TypeKind::Region => false,
    }
}

/// One binding a pattern makes: its name, where, and its type when known.
#[derive(Debug, Clone)]
struct Binding {
    name: String,
    span: Span,
    ty: Option<String>,
}

/// The walk over one pattern.
struct Walk<'i, 'a> {
    index: &'i TypeIndex<'a>,
    /// In an arm, the tags of the scrutinee's row (empty when it carries
    /// none); `None` in a binder position, where every name binds.
    arm: Option<&'i [String]>,
    file: &'i str,
    out: &'i mut Vec<Diag>,
}

impl Walk<'_, '_> {
    fn tests(&self, name: &str, top: bool) -> bool {
        self.arm.is_some_and(|tags| {
            self.index.variants.contains_key(name) || (top && tags.iter().any(|tag| tag == name))
        })
    }

    /// Adds `more` to `bound`, refusing each name `bound` already holds.
    fn join(&mut self, bound: &mut Vec<Binding>, more: Vec<Binding>) {
        for binding in more {
            if let Some(first) = bound.iter().find(|b| b.name == binding.name) {
                self.out.push(twice(
                    &binding.name,
                    binding.span,
                    first.span,
                    "pattern",
                    self.file,
                ));
            } else {
                bound.push(binding);
            }
        }
    }

    fn names(&mut self, pattern: &Pattern, ty: Option<String>, top: bool) -> Vec<Binding> {
        match &*pattern.kind {
            PatKind::Wildcard | PatKind::Literal(_) | PatKind::Range { .. } | PatKind::Path(_) => {
                Vec::new()
            }
            PatKind::Binding(ident) => {
                if self.tests(&ident.name, top) {
                    Vec::new()
                } else {
                    vec![Binding {
                        name: ident.name.clone(),
                        span: ident.span,
                        ty,
                    }]
                }
            }
            PatKind::Variant { path, fields } => {
                let payload = self.index.payload(path);
                let mut bound = Vec::new();
                for (at, field) in fields.iter().enumerate() {
                    let ty = payload.get(at).cloned().flatten();
                    let more = self.names(field, ty, false);
                    self.join(&mut bound, more);
                }
                bound
            }
            // A tuple's elements are typed by no declaration this reader
            // holds: unknown, so never judged at E0401.
            PatKind::Tuple(items) => {
                let mut bound = Vec::new();
                for item in items {
                    let more = self.names(item, None, false);
                    self.join(&mut bound, more);
                }
                bound
            }
            PatKind::Struct { path, fields, .. } => {
                let mut bound = Vec::new();
                for FieldPat {
                    name, pattern: sub, ..
                } in fields
                {
                    let ty = self.index.field(path, &name.name);
                    let more = match sub {
                        Some(sub) => self.names(sub, ty, false),
                        None => vec![Binding {
                            name: name.name.clone(),
                            span: name.span,
                            ty,
                        }],
                    };
                    self.join(&mut bound, more);
                }
                bound
            }
            PatKind::At { name, pattern } => {
                let mut bound = vec![Binding {
                    name: name.name.clone(),
                    span: name.span,
                    ty: ty.clone(),
                }];
                let more = self.names(pattern, ty, top);
                self.join(&mut bound, more);
                bound
            }
            PatKind::Or(alternatives) => self.or(alternatives, ty, top),
        }
    }

    /// An or-pattern: each alternative is one pattern, and together they bind
    /// each of their names once, at the first alternative's binding.
    fn or(&mut self, alternatives: &[Pattern], ty: Option<String>, top: bool) -> Vec<Binding> {
        let each: Vec<Vec<Binding>> = alternatives
            .iter()
            .map(|alternative| self.names(alternative, ty.clone(), top))
            .collect();
        let mut union: Vec<Binding> = Vec::new();
        for bound in &each {
            for binding in bound {
                if !union.iter().any(|b| b.name == binding.name) {
                    union.push(binding.clone());
                }
            }
        }
        for (alternative, bound) in alternatives.iter().zip(&each) {
            for name in &union {
                if !bound.iter().any(|b| b.name == name.name) {
                    self.out
                        .push(lacks(&name.name, alternative.span, self.file));
                }
            }
        }
        // E0401: the first type each name is bound at, against every later one.
        for name in &union {
            let mut first: Option<&Binding> = None;
            for binding in each.iter().flatten().filter(|b| b.name == name.name) {
                match (first, &binding.ty) {
                    (None, Some(_)) => first = Some(binding),
                    (Some(earlier), Some(ty)) if earlier.ty.as_ref() != Some(ty) => {
                        let earlier_ty = earlier.ty.clone().unwrap_or_default();
                        self.out.push(
                            Diag::new(
                                "E0401",
                                binding.span,
                                "gram.pat",
                                format!(
                                    "`{}` is `{earlier_ty}` in one alternative of this \
                                     or-pattern and `{ty}` in this one: each name an or-pattern \
                                     binds has one type, since the arm's body sees it whichever \
                                     alternative matched ([gram.pat.or]). Give the two values \
                                     different names in two arms, or match them so they bind one \
                                     type",
                                    name.name
                                ),
                            )
                            .in_file(self.file.to_owned()),
                        );
                    }
                    _ => {}
                }
            }
        }
        union
    }
}

fn twice(name: &str, at: Span, first: Span, list: &str, file: &str) -> Diag {
    let why = if list == "pattern" {
        "a pattern binds each name once ([gram.pat.names]): a second binding would hide the \
         first one's value. Give one of them another name, or match the value you do not need \
         with `_`"
    } else {
        "a parameter list binds each name once ([gram.pat.names]): the later parameter would \
         hide the earlier one for the whole body. Give one of them another name"
    };
    Diag::new(
        "E0827",
        at,
        "gram.pat",
        format!("`{name}` is bound twice in this {list} (first bound at {first}): {why}"),
    )
    .in_file(file.to_owned())
}

fn lacks(name: &str, at: Span, file: &str) -> Diag {
    Diag::new(
        "E0828",
        at,
        "gram.pat",
        format!(
            "`{name}` is bound by another alternative of this or-pattern, but not by this one: \
             this alternative binds no `{name}`. Every alternative of an or-pattern binds the \
             same names, because the arm's body sees `{name}` whichever alternative matched \
             ([gram.pat.or]); bind `{name}` in this alternative too, or write the alternatives \
             as separate arms"
        ),
    )
    .in_file(file.to_owned())
}

/// A pattern in a binder position: `let`, `var`, a `for` header, an `else
/// |…|` handler, a `select` receive. Every bare name binds.
pub fn binder(pattern: &Pattern, index: &TypeIndex<'_>, file: &str, out: &mut Vec<Diag>) {
    let mut walk = Walk {
        index,
        arm: None,
        file,
        out,
    };
    walk.names(pattern, None, true);
}

/// A `match` arm's pattern over a scrutinee whose row carries `tags` (empty
/// when it carries none).
pub fn arm(
    pattern: &Pattern,
    tags: &[String],
    index: &TypeIndex<'_>,
    file: &str,
    out: &mut Vec<Diag>,
) {
    let mut walk = Walk {
        index,
        arm: Some(tags),
        file,
        out,
    };
    walk.names(pattern, None, true);
}

/// A function's parameter list.
pub fn params(params: &[Param], file: &str, out: &mut Vec<Diag>) {
    let names = params.iter().filter_map(|param| match &param.kind {
        ParamKind::Named { name, .. } => Some(name),
        ParamKind::SelfParam { .. } => None,
    });
    list(names, file, out);
}

/// A closure's parameter list.
pub fn closure_params(params: &[ClosureParam], file: &str, out: &mut Vec<Diag>) {
    list(params.iter().map(|param| &param.name), file, out);
}

fn list<'a>(names: impl Iterator<Item = &'a crate::ast::Ident>, file: &str, out: &mut Vec<Diag>) {
    let mut seen: Vec<&crate::ast::Ident> = Vec::new();
    for name in names {
        if let Some(first) = seen.iter().find(|ident| ident.name == name.name) {
            out.push(twice(
                &name.name,
                name.span,
                first.span,
                "parameter list",
                file,
            ));
        } else {
            seen.push(name);
        }
    }
}

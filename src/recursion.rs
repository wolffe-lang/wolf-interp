//! `[type.rec]` (s288; wolf-lang#1062, the second case): a type may contain
//! itself only through storage that is not inline.
//!
//! A struct's fields, an enum's payloads and a tuple's elements are laid out
//! inline (`[abi.native.enum]`: an enum value is its tag, then its payload
//! slots), and so are an error union's ok type and tag payloads, a `distinct`
//! type's base, and what a type alias names. A type that reaches itself
//! through those alone would be a value with no finite size, so a cycle among
//! them is **E0404**, once per cycle, the cycle named in full.
//!
//! - Types are checked where they are declared, used or not, in declaration
//!   order, and a cycle is reported at the member (a field's or a variant's
//!   name) through which its first-declared type reaches itself:
//!   `enum L { Nil, Cons(int, L) }` at `Cons`, and `struct N { v: int, next: L }`
//!   with `enum L { Nil, More(N) }` once, at `next`.
//! - A generic declaration is walked at its arguments: with `struct Box[T] {
//!   v: T }`, `Box[L]` holds an `L` inline, so `L -> Box -> L` is reported at
//!   `L`'s member, never inside `Box`'s own definition.
//! - The walk follows only what this module declares. `List`, `Map`, `Pool`,
//!   `handle`, `weak`, `shared` (whose strong cycles are E1006, a rule of its
//!   own), raw pointers, channels, mutexes and function types hold their
//!   element elsewhere, and so does any path the walk cannot resolve here: a
//!   cycle cannot cross a module boundary without an import cycle.
//! - A recursion that only grows its arguments (`L[T]` holding `L[(T, T)]`)
//!   never repeats one type: the walk stops at 32 inline levels and does not
//!   report it, as the clause says.
//!
//! The checked machine and this one ran a by-value recursive enum before the
//! clause, and every implementation accepted an unused one. The pinned spec
//! (`5f1f1c94`) has no `[type.rec]` yet, so the diagnostic cites
//! `[gram.item.type]` and names the clause in its words.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Ident, ItemKind, PrefixTypeKw, Type, TypeArg, TypeDef, TypeKind};
use crate::diag::Diag;
use crate::sema::{Module, Program};

/// How deep the walk follows inline members, as the clause bounds it.
const DEPTH: usize = 32;
/// How long a type's spelling may grow before the walk stops following it: a
/// recursion that grows its arguments doubles it at every level.
const SPELLING: usize = 4096;
/// How many types one declaration's walk visits at most.
const VISITS: usize = 100_000;

/// One type declaration the walk can follow.
enum Decl<'a> {
    /// A struct's fields or an enum's variants: each member's name and the
    /// types it holds inline.
    Members(Vec<(&'a Ident, Vec<&'a Type>)>),
    /// `type A = T`: transparent, so it names no step of a cycle; `distinct`
    /// is nominal and does.
    Alias(&'a Type),
}

struct Declared<'a> {
    generics: Vec<String>,
    decl: Decl<'a>,
}

/// Every E0404 the program's declarations hold, in source order.
#[must_use]
pub fn check(program: &Program) -> Vec<Diag> {
    let mut out = Vec::new();
    for module in program.modules.values() {
        out.extend(module_check(module));
    }
    out
}

fn module_check(module: &Module) -> Vec<Diag> {
    let mut table: BTreeMap<&str, Declared<'_>> = BTreeMap::new();
    let mut order: Vec<(&str, &str)> = Vec::new();
    for unit in &module.units {
        for item in &unit.unit.items {
            let Some((name, declared)) = declared(&item.kind) else {
                continue;
            };
            if table.contains_key(name) {
                continue; // E0302's business; the first wins here.
            }
            if matches!(declared.decl, Decl::Members(_)) {
                order.push((name, unit.file.as_str()));
            }
            table.insert(name, declared);
        }
    }
    let mut reported: Vec<BTreeSet<String>> = Vec::new();
    let mut out = Vec::new();
    for (name, file) in order {
        let Declared { generics, decl } = &table[name];
        let Decl::Members(members) = decl else {
            continue;
        };
        let start = if generics.is_empty() {
            name.to_owned()
        } else {
            format!("{name}[{}]", generics.join(", "))
        };
        for (member, types) in members {
            let mut walk = Walk {
                table: &table,
                target: &start,
                params: generics,
                visits: 0,
            };
            let mut trail = vec![name.to_owned()];
            let mut seen = vec![start.clone()];
            let cycle = types
                .iter()
                .find_map(|ty| walk.reaches(ty, &BTreeMap::new(), 1, &mut trail, &mut seen));
            let Some(cycle) = cycle else {
                continue;
            };
            let set: BTreeSet<String> = cycle.iter().cloned().collect();
            if reported.contains(&set) {
                continue;
            }
            reported.push(set);
            out.push(
                Diag::new(
                    "E0404",
                    member.span,
                    "gram.item.type",
                    format!(
                        "`{name}` contains itself by value, so it would have no finite size: \
                         `{name}` reaches itself through here: {}. A struct's fields, an enum's \
                         payloads and a tuple's elements are laid out inline, so a type that \
                         reaches itself through them alone would be a value with no finite size \
                         ([type.rec]); recurse through a container that holds its element \
                         elsewhere: `List[{name}]`, `Map[K, {name}]`, or a `handle {name}` into \
                         a pool",
                        cycle.join(" -> ")
                    ),
                )
                .in_file(file.to_owned()),
            );
        }
    }
    out
}

fn declared(kind: &ItemKind) -> Option<(&str, Declared<'_>)> {
    fn names(list: &[crate::ast::GenericParam]) -> Vec<String> {
        list.iter().map(|g| g.name.name.clone()).collect()
    }
    fn struct_members(def: &crate::ast::StructDef) -> Decl<'_> {
        Decl::Members(def.fields.iter().map(|f| (&f.name, vec![&f.ty])).collect())
    }
    fn enum_members(def: &crate::ast::EnumDef) -> Decl<'_> {
        Decl::Members(
            def.variants
                .iter()
                .map(|v| (&v.name, v.payload.iter().collect()))
                .collect(),
        )
    }
    match kind {
        ItemKind::Struct(def) => Some((
            def.name.as_ref()?.name.as_str(),
            Declared {
                generics: names(&def.generics),
                decl: struct_members(def),
            },
        )),
        ItemKind::Enum(def) => Some((
            def.name.as_ref()?.name.as_str(),
            Declared {
                generics: names(&def.generics),
                decl: enum_members(def),
            },
        )),
        ItemKind::TypeAlias(alias) => {
            let mut generics = names(&alias.generics);
            let decl = match &alias.def {
                TypeDef::Struct(def) => {
                    generics.extend(names(&def.generics));
                    struct_members(def)
                }
                TypeDef::Enum(def) => {
                    generics.extend(names(&def.generics));
                    enum_members(def)
                }
                TypeDef::Alias(ty) => Decl::Alias(ty),
            };
            Some((alias.name.name.as_str(), Declared { generics, decl }))
        }
        _ => None,
    }
}

struct Walk<'t, 'a> {
    table: &'t BTreeMap<&'a str, Declared<'a>>,
    /// The starting declaration, spelled at its own parameters (`L[T]`).
    target: &'t str,
    /// Its generic parameters: a name the walk cannot see through.
    params: &'t [String],
    visits: usize,
}

impl Walk<'_, '_> {
    /// Whether `ty`, read under `subst`, holds the target inline: the cycle's
    /// nominal steps from the target back to it, or `None`.
    fn reaches(
        &mut self,
        ty: &Type,
        subst: &BTreeMap<String, Type>,
        depth: usize,
        trail: &mut Vec<String>,
        seen: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        self.visits += 1;
        if depth > DEPTH || self.visits > VISITS {
            return None;
        }
        match &*ty.kind {
            TypeKind::Path { path, args } => {
                let [head] = path.segments.as_slice() else {
                    return None; // another module's type: no cycle crosses back
                };
                if args.is_empty()
                    && let Some(bound) = subst.get(&head.name)
                {
                    let bound = bound.clone();
                    return self.reaches(&bound, &BTreeMap::new(), depth, trail, seen);
                }
                if args.is_empty() && self.params.contains(&head.name) {
                    return None;
                }
                let table = self.table;
                let declared = table.get(head.name.as_str())?;
                let args: Vec<Type> = args
                    .iter()
                    .filter_map(|arg| match arg {
                        TypeArg::Type(inner) => Some(substitute(inner, subst)),
                        TypeArg::Expr(_) => None,
                    })
                    .collect();
                let spelled = substitute(ty, subst).spelling();
                if spelled == self.target {
                    let mut cycle = trail.clone();
                    cycle.push(head.name.clone());
                    return Some(cycle);
                }
                if spelled.len() > SPELLING || seen.contains(&spelled) {
                    return None; // a cycle that does not pass the target
                }
                let inner: BTreeMap<String, Type> =
                    declared.generics.iter().cloned().zip(args).collect();
                let nominal = match &declared.decl {
                    Decl::Members(_) => true,
                    Decl::Alias(aliased) => matches!(
                        &*aliased.kind,
                        TypeKind::Prefixed {
                            kw: PrefixTypeKw::Distinct,
                            ..
                        }
                    ),
                };
                seen.push(spelled);
                if nominal {
                    trail.push(head.name.clone());
                }
                let found = match &declared.decl {
                    Decl::Members(members) => members.iter().find_map(|(_, types)| {
                        types
                            .iter()
                            .find_map(|ty| self.reaches(ty, &inner, depth + 1, trail, seen))
                    }),
                    Decl::Alias(aliased) => self.reaches(aliased, &inner, depth + 1, trail, seen),
                };
                if nominal {
                    trail.pop();
                }
                seen.pop();
                found
            }
            TypeKind::Tuple(parts) => parts
                .iter()
                .find_map(|part| self.reaches(part, subst, depth, trail, seen)),
            TypeKind::ErrorUnion(inner) => self.reaches(inner, subst, depth, trail, seen),
            TypeKind::Fallible { ty: inner, row } => {
                self.reaches(inner, subst, depth, trail, seen).or_else(|| {
                    row.entries
                        .iter()
                        .flat_map(|entry| &entry.payload)
                        .find_map(|payload| self.reaches(payload, subst, depth, trail, seen))
                })
            }
            TypeKind::Prefixed {
                kw: PrefixTypeKw::Distinct,
                ty: inner,
            } => self.reaches(inner, subst, depth, trail, seen),
            TypeKind::Prefixed { .. }
            | TypeKind::RawPointer(_)
            | TypeKind::Dyn(_)
            | TypeKind::Fn { .. }
            | TypeKind::TypeOfTypes
            | TypeKind::Region => None,
        }
    }
}

/// `ty` with every bare name `subst` binds replaced by its type.
fn substitute(ty: &Type, subst: &BTreeMap<String, Type>) -> Type {
    if subst.is_empty() {
        return ty.clone();
    }
    let kind = match &*ty.kind {
        TypeKind::Path { path, args } => {
            if args.is_empty()
                && path.is_single()
                && let Some(bound) = subst.get(&path.segments[0].name)
            {
                return bound.clone();
            }
            TypeKind::Path {
                path: path.clone(),
                args: args
                    .iter()
                    .map(|arg| match arg {
                        TypeArg::Type(inner) => TypeArg::Type(substitute(inner, subst)),
                        TypeArg::Expr(expr) => TypeArg::Expr(expr.clone()),
                    })
                    .collect(),
            }
        }
        TypeKind::ErrorUnion(inner) => TypeKind::ErrorUnion(substitute(inner, subst)),
        TypeKind::Fallible { ty: inner, row } => {
            let mut row = row.clone();
            for entry in &mut row.entries {
                for payload in &mut entry.payload {
                    *payload = substitute(payload, subst);
                }
            }
            TypeKind::Fallible {
                ty: substitute(inner, subst),
                row,
            }
        }
        TypeKind::Prefixed { kw, ty: inner } => TypeKind::Prefixed {
            kw: *kw,
            ty: substitute(inner, subst),
        },
        TypeKind::RawPointer(inner) => TypeKind::RawPointer(substitute(inner, subst)),
        TypeKind::Tuple(parts) => {
            TypeKind::Tuple(parts.iter().map(|part| substitute(part, subst)).collect())
        }
        other => other.clone(),
    };
    Type {
        kind: Box::new(kind),
        span: ty.span,
        anchor: ty.anchor,
    }
}

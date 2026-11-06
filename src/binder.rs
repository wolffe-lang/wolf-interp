//! Variant names in binder positions (`[gram.pat.binder]` upstream): the
//! binder half of W0801 and the `select` arm's E0806, mirrored from the
//! compiler's resolve-rung analysis.
//!
//! A binder pattern — a statement `let`/`var` binder, a `for` header, the
//! `else |…|` handler's binding form — binds every value it meets, so every
//! bare name in it is a new binding, at any depth. A name there that begins
//! with an uppercase letter and spells a variant of an enum *in scope* still
//! binds, and warns W0801 at the name: it reads as the test it is not. A
//! `select` arm's receive takes whatever its channel delivers, so the same
//! name there is refused (E0806).
//!
//! "In scope" is the compiler's set, which needs resolution alone: the enums
//! the module declares (`Module::variants`), then the enums the FILE imports
//! by item (`use m.Kind`, spelled by its alias when aliased; `use` is
//! file-scoped, D32). Wolf has no builtin enum, so the prelude adds none.
//! Scope decides, not type, which is what lets this machine — which has no
//! checker — answer it exactly as the compiler does. The std tree is not
//! exempt: the compiler reports a W0801 wherever it arises.

use std::collections::BTreeMap;

use crate::ast::{Ident, ItemKind, PatKind, Pattern};
use crate::sema::{Module, Program, SourceUnit};

/// Variant name → the enums this file names directly that declare it, in the
/// compiler's order: the module's own enums by name, then the file's item
/// imports in import order. The first is the one a message names.
#[must_use]
pub fn homes(program: &Program, module: &Module, unit: &SourceUnit) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (variant, enums) in &module.variants {
        let mut sorted = enums.clone();
        sorted.sort();
        sorted.dedup();
        out.insert(variant.clone(), sorted);
    }
    for item in &unit.unit.items {
        let ItemKind::Use(decl) = &item.kind else {
            continue;
        };
        let path: Vec<String> = decl.path.segments.iter().map(|s| s.name.clone()).collect();
        // (the bound name, the full path it imports): the same reading
        // `sema::collect_use` makes of the decl.
        let bound: Vec<(String, Vec<String>)> = if decl.list.is_empty() {
            let name = decl
                .alias
                .as_ref()
                .or_else(|| decl.path.segments.last())
                .map(|ident| ident.name.clone());
            name.map(|name| (name, path.clone())).into_iter().collect()
        } else {
            decl.list
                .iter()
                .map(|item| {
                    let mut segments = path.clone();
                    segments.push(item.name.clone());
                    (item.name.clone(), segments)
                })
                .collect()
        };
        for (name, segments) in bound {
            let [.., home, last] = segments.as_slice() else {
                continue;
            };
            // The module a dotted path names, as the error-alias reader in
            // `lint::analyze` finds it: by the path's last module segment,
            // then by its head.
            let Some(target) = [home, &segments[0]]
                .into_iter()
                .find_map(|key| program.modules.get(key.as_str()))
            else {
                continue;
            };
            for (variant, enums) in &target.variants {
                if !enums.iter().any(|declared| declared == last) {
                    continue;
                }
                let entry = out.entry(variant.clone()).or_default();
                if !entry.contains(&name) {
                    entry.push(name.clone());
                }
            }
        }
    }
    out
}

/// Does the name read as a variant test: it begins with an uppercase letter
/// (W0801's "capitalized name"; a lowercase name is already the binding
/// spelling the warning recommends)?
#[must_use]
pub fn capitalized(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// Every name a binder pattern binds, through the shapes a binder admits:
/// tuples, struct fields (explicit or shorthand) and both sides of `@`. A
/// refutable shape (a path, a variant, an or-pattern, a literal, a range) is
/// E0806's and is not walked.
pub fn names<'p>(pattern: &'p Pattern, out: &mut Vec<&'p Ident>) {
    match &*pattern.kind {
        PatKind::Binding(ident) => out.push(ident),
        PatKind::At { name, pattern } => {
            out.push(name);
            names(pattern, out);
        }
        PatKind::Tuple(fields) => {
            for field in fields {
                names(field, out);
            }
        }
        PatKind::Struct { fields, .. } => {
            for field in fields {
                match &field.pattern {
                    Some(sub) => names(sub, out),
                    None => out.push(&field.name),
                }
            }
        }
        PatKind::Wildcard
        | PatKind::Literal(_)
        | PatKind::Path(_)
        | PatKind::Variant { .. }
        | PatKind::Or(_)
        | PatKind::Range { .. } => {}
    }
}

/// The lowercase spelling the refusal's help names: `Drink` → `drink`,
/// `HotDrink` → `hot_drink`. Local prose only: the compiler's fix also
/// steps around taken names, and this machine applies no edits.
#[must_use]
pub fn lowercase(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1).copied();
            let boundary = prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit())
                || (prev.is_some_and(char::is_uppercase) && next.is_some_and(char::is_lowercase));
            if boundary && !out.ends_with('_') {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::lowercase;

    #[test]
    fn lowercase_splits_words_and_acronyms() {
        assert_eq!(lowercase("Drink"), "drink");
        assert_eq!(lowercase("HotDrink"), "hot_drink");
        assert_eq!(lowercase("HTTPCode"), "http_code");
    }
}

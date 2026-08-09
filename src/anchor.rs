//! Clause anchors.
//!
//! Reimplemented from `spec/05-conformance.md` `[conf.anchor]`. An anchor is a
//! dotted lowercase token whose leading segment is its **namespace**; a tag
//! outside every registered and reserved namespace is an error
//! (`[conf.anchor.ns]`).
//!
//! ## Recorded divergence between spec/05 and the corpus
//!
//! `[conf.anchor.grammar]` says an anchor is built from "letters, digits, `-`,
//! `.`" — but the pinned corpus ships anchors containing underscores
//! (`comptime.types_as_values`, `str.regex_literal`, `mod.use.unused` is fine,
//! but the first two are not). Rejecting `_` would redden the pinned corpus, so
//! this implementation accepts it and the discrepancy is reported upward as a
//! finding for the queued spec/05 amendment rather than silently normalized.

use std::fmt;

/// Namespaces with an owning spec document today (`[conf.anchor.ns]`).
pub const REGISTERED_NAMESPACES: [&str; 6] = ["gram", "mem", "conc", "abi", "conf", "proto"];

/// Namespaces reserved for spec documents not yet written; tags in them are
/// legal and counted as *forward* (`[conf.anchor.ns]`).
pub const RESERVED_NAMESPACES: [&str; 13] = [
    "str", "err", "task", "proc", "sync", "generics", "arith", "ffi", "unsafe", "comptime", "perf",
    "mod", "std",
];

/// How an anchor's namespace is classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    /// Owned by a spec document that exists; the anchor is checkable against
    /// `spec/anchors.json`.
    Registered,
    /// Owned by a document not yet written; legal, counted as forward.
    Reserved,
}

/// Why an anchor is not a well-formed clause tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnchorError {
    Empty,
    EmptySegment(String),
    BadCharacter { anchor: String, ch: char },
    UnknownNamespace { anchor: String, namespace: String },
}

impl fmt::Display for AnchorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnchorError::Empty => f.write_str("empty anchor"),
            AnchorError::EmptySegment(a) => write!(f, "anchor `{a}` has an empty dotted segment"),
            AnchorError::BadCharacter { anchor, ch } => write!(
                f,
                "anchor `{anchor}` contains `{ch}`; anchors are lowercase letters, digits, `-`, `_` and `.`"
            ),
            AnchorError::UnknownNamespace { anchor, namespace } => write!(
                f,
                "anchor `{anchor}` is in unknown namespace `{namespace}` (neither registered nor reserved)"
            ),
        }
    }
}

impl std::error::Error for AnchorError {}

/// Validates an anchor's shape and namespace, classifying the namespace.
///
/// # Errors
///
/// Returns [`AnchorError`] when the token is empty, has an empty dotted
/// segment, uses a character outside the anchor alphabet, or names a namespace
/// that is neither registered nor reserved.
pub fn classify(anchor: &str) -> Result<Namespace, AnchorError> {
    if anchor.is_empty() {
        return Err(AnchorError::Empty);
    }
    for ch in anchor.chars() {
        let ok =
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_' || ch == '.';
        if !ok {
            return Err(AnchorError::BadCharacter {
                anchor: anchor.to_owned(),
                ch,
            });
        }
    }
    if anchor.split('.').any(str::is_empty) {
        return Err(AnchorError::EmptySegment(anchor.to_owned()));
    }

    let namespace = anchor.split('.').next().unwrap_or_default();
    if REGISTERED_NAMESPACES.contains(&namespace) {
        Ok(Namespace::Registered)
    } else if RESERVED_NAMESPACES.contains(&namespace) {
        Ok(Namespace::Reserved)
    } else {
        Err(AnchorError::UnknownNamespace {
            anchor: anchor.to_owned(),
            namespace: namespace.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_and_reserved_namespaces_are_distinguished() {
        assert_eq!(classify("mem.tier0.move.1"), Ok(Namespace::Registered));
        assert_eq!(classify("gram.amb.bang"), Ok(Namespace::Registered));
        assert_eq!(classify("str.interp"), Ok(Namespace::Reserved));
        assert_eq!(
            classify("comptime.types_as_values"),
            Ok(Namespace::Reserved)
        );
    }

    #[test]
    fn unknown_namespaces_are_rejected() {
        assert_eq!(
            classify("wolf.made.up"),
            Err(AnchorError::UnknownNamespace {
                anchor: "wolf.made.up".to_owned(),
                namespace: "wolf".to_owned(),
            })
        );
    }

    #[test]
    fn shape_is_enforced() {
        assert_eq!(classify(""), Err(AnchorError::Empty));
        assert!(matches!(
            classify("Mem.Tier0"),
            Err(AnchorError::BadCharacter { .. })
        ));
        assert!(matches!(
            classify("mem..tier0"),
            Err(AnchorError::EmptySegment(_))
        ));
        assert!(matches!(
            classify("mem."),
            Err(AnchorError::EmptySegment(_))
        ));
        assert!(matches!(
            classify("mem tier0"),
            Err(AnchorError::BadCharacter { .. })
        ));
    }

    #[test]
    fn a_bare_namespace_is_a_legal_anchor() {
        assert_eq!(classify("mem"), Ok(Namespace::Registered));
    }
}

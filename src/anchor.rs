//! Clause anchors.
//!
//! Reimplemented from `spec/05-conformance.md` `[conf.anchor]`. An anchor is a
//! dotted lowercase token whose leading segment is its **namespace**; a tag
//! outside every registered and reserved namespace is an error
//! (`[conf.anchor.ns]`).
//!
//! ## Resolved: the underscore in the anchor charset
//!
//! At the previous pin `[conf.anchor.grammar]` said an anchor is built from
//! "letters, digits, `-`, `.`" while the corpus shipped anchors containing
//! underscores (`comptime.types_as_values`, `str.regex_literal`). We accepted
//! `_` and reported the discrepancy upward; the amendment landed — the pinned
//! clause now reads "letters, digits, `-`, `_`, `.`" and the charset below is
//! the spec's, not a tolerance.

use std::fmt;

/// Namespaces with an owning spec document today (`[conf.anchor.ns]`).
/// `diag` joined at the f0da6e6 pin (s67 — `[diag.sev]`/`[diag.level]`
/// land in spec/01 §9 and the anchor registry).
/// `ct` joined at the da8582d pin (s112 — `spec/09-constant-time.md` lands
/// with `[ct.attr]`/`[ct.taint]`, and every `ct.*` anchor is in
/// `anchors.json`, so the namespace is checkable, not forward).
/// `type` joined at the 77466a3 pin (s113 — `spec/10-types.md` lands with
/// D54's `[type.numlit]` family, and every `type.*` anchor is in
/// `anchors.json`, so the namespace is checkable, not forward). The chapter
/// spells its namespace `type`, not the older reserved `ty`.
/// `pkg` joined at the 90c90df pin (s115 amends `[conf.anchor.ns]` for #120
/// — the clause letter now admits 08-package.md's namespace, so the
/// standing waiver in `export::FILED_REGISTRY_FINDINGS` dies with the
/// filing and the namespace is registered like any other).
/// `os` joined at the 90c90df pin (s114 — `spec/11-os.md` lands with the
/// `[os.signal]` family, and every `os.*` anchor is in `anchors.json`, so
/// the namespace is checkable, not forward).
///
/// `sched` joined at the v0.2.8 pin (`5c729e8`; s139 ruled
/// `spec/07-schedule-points.md` normative and admitted the namespace at
/// `ed8f526`, #246, anchors 424 -> 431 — **one merge after the `v0.2.6`
/// tag this list previously pinned**, which is why is39 deferred it and
/// said so here). is40 takes the pin that carries it and appends in the
/// SAME change, which is what `[conf.anchor.ns.admit]` asks: the clause's
/// registered list and this list move together or one of them is silent.
/// Deferring it past this pin would have been the *restrictive* half —
/// this machine rejecting `conforms: … sched.stable`, which
/// `corpus/test/conc_schedules_test.lu` now carries as a real tag instead
/// of the prose comment it wore at `398e5f5`.
///
/// `exec` appended 2026-09-11 by is46 at the pin that carries s153
/// (c9237c1, wolf-lang v0.2.11): `[exec.checked.budget]` went normative in
/// 05-conformance.md §5 and the clause's registered list took `exec` in
/// the same change (`[conf.anchor.ns.admit]`); without it this walker
/// refused `corpus/strings/bytes_view_walk.lu` as an unknown namespace,
/// which is the restrictive half again.
pub const REGISTERED_NAMESPACES: [&str; 13] = [
    "gram", "mem", "conc", "abi", "conf", "proto", "diag", "ct", "type", "pkg", "os", "sched",
    "exec",
];

/// Namespaces reserved for spec documents not yet written; tags in them are
/// legal and counted as *forward* (`[conf.anchor.ns]`).
pub const RESERVED_NAMESPACES: [&str; 16] = [
    "str", "err", "task", "proc", "sync", "generics", "arith", "ffi", "unsafe", "comptime", "perf",
    "mod", "std", "ty",
    // `test` appended 2026-08-11 by s39 (pin 13b811f) for the built-in test
    // framework's litmus tier — `[conf.anchor.ns]`'s own additive contract,
    // D34/D36 own the future spec document.
    "test",
    // is08: the REPL-spec notes' own namespace (`docs/repl.md`). The REPL
    // extends the spec — its incremental-definition rules are deliberately
    // NOT spec/01..05 clauses — and is09 exports the notes.
    "repl",
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

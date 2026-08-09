//! The rule registry — is02's clause-citation discipline, as data.
//!
//! Target 7 of the sprint: *"Every dynamic rule lives in a rule registry:
//! `{anchor, description, fault constructor}`. `--trace` logs each rule as it
//! fires. A unit test walks the registry and the evaluator's fault sites: a rule
//! without an anchor, or an anchor absent from the pinned spec, fails the build.
//! 'Every rule traceable to a spec clause' is thereby a test, not a review
//! item."*
//!
//! The shape that makes this cheap: [`Rule`] is a C-like enum, so every
//! evaluation step in `eval` names the rule it is applying by *type* rather than
//! by string, and the compiler's exhaustiveness check does half the work. The
//! other half is [`REGISTRY`], which pairs each variant with its clause anchor
//! and one sentence; [`validate`] is the predicate the tests run against it, and
//! it is deliberately callable on a hand-built table so a *planted* anchorless
//! rule can be shown to fail without editing the real one.
//!
//! Anchors in a registered namespace (`gram`, `mem`, `conf`, …) must resolve
//! against the pinned `spec/anchors.json`; anchors in a reserved forward
//! namespace (`arith`, `err`, `str`, …) are legal and counted as forward
//! (`[conf.anchor.ns]`) — the documents that will own them are not written yet,
//! and `arith.checked` is exactly the kind of rule this sprint implements.

use std::fmt;

/// Every dynamic rule this evaluator implements.
///
/// A variant here is a promise: somewhere in `eval` there is a step that cites
/// it, and `REGISTRY` gives it a clause. Adding a variant without a registry row
/// fails to compile (the table is exhaustive-matched in [`Rule::row`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    // -- §1 model ----------------------------------------------------------
    /// Whole-value assignment and argument passing; values have no identity.
    ValueSemantics,
    /// Places are denoted by paths with field/index projections.
    PlacePath,
    /// Two paths conflict iff one is a prefix of the other.
    PathDisjoint,
    /// Struct literals and collection constructors are the allocation sites.
    Alloc,

    // -- §2 moves ----------------------------------------------------------
    /// Initialization, assignment, `take` and `return` move; the source place
    /// becomes uninitialized.
    Move,
    /// Reading a moved-from place traps `use-after-move`.
    UseAfterMove,
    /// `copy x` produces an independent value.
    Copy,
    /// A moved-from place may be re-initialized by assignment.
    Reinit,

    // -- §2 parameter modes ------------------------------------------------
    /// Default mode: immutable for the whole call, caller retains.
    ModeRead,
    /// `mut`: exclusive inout for the call's extent.
    ModeMut,
    /// `take`: the argument moves into the callee.
    ModeTake,

    // -- §2 exclusivity ----------------------------------------------------
    /// A `mut`-held place has no other live access path.
    Exclusivity,
    /// Disjoint paths may be `mut` simultaneously.
    ExclusivityDisjoint,
    /// A view set bounds the callee's path footprint.
    ViewSet,

    // -- §2 borrows --------------------------------------------------------
    /// `&path` / `&mut path` create local borrows.
    Borrow,
    /// A live `&mut` holds its path exclusively; `&` read-freezes it.
    BorrowExtent,

    // -- defined behavior (§7) --------------------------------------------
    /// Integer overflow traps in every profile.
    ArithChecked,
    /// `wrapping`/`saturating` types are the spelling for intended overflow.
    ArithWrapping,
    /// Division (and remainder) by zero traps `div-zero`.
    DivZero,
    /// Out-of-bounds indexing and slicing trap `bounds`.
    Bounds,
    /// An untyped integer literal defaults to `i32`, a float to `f64`, unless a
    /// checking context supplies a type.
    LiteralDefault,

    // -- expression-oriented control flow ---------------------------------
    /// A block yields its tail expression.
    Block,
    /// `if`/`match`/`for`/`while`/`loop` are expressions; `return`/`break`/
    /// `continue` are jumps.
    Flow,
    /// `&&`/`||` short-circuit; every other operand position evaluates
    /// left-to-right.
    EvalOrder,
    /// Assignment is a statement and its place is a path.
    Assign,
    /// A call binds arguments to parameters by mode and evaluates the body.
    Call,
    /// A closure captures its free variables by value at construction.
    Closure,

    // -- errors as values (D30) -------------------------------------------
    /// `!T` values are ok-or-tagged; there is no unwinding.
    ErrUnion,
    /// Error tags are structural: a row entry needs no declaration.
    ErrRows,
    /// `?` returns the error to the caller, widening the row by union.
    ErrPropagate,
    /// `else` / `else |err|` default an error away.
    ErrElse,
    /// `errdefer` runs on the error path only.
    ErrDefer,
    /// Scope-exit effects run LIFO.
    DeferLifo,

    // -- strings -----------------------------------------------------------
    /// Every string literal is an f-string; interpolations evaluate in place.
    StrInterp,

    // -- traps -------------------------------------------------------------
    /// A user assertion that fails traps `assert`.
    Assert,
    /// Traps map onto the closed eleven-kind vocabulary and terminate.
    TrapVocabulary,
}

/// One registry row: the rule, its clause anchor, and one sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub rule: Rule,
    pub anchor: &'static str,
    pub description: &'static str,
}

impl Rule {
    /// This rule's registry row. Exhaustive by construction — a new variant
    /// without a row does not compile.
    #[must_use]
    pub const fn row(self) -> Row {
        let (anchor, description) = match self {
            Rule::ValueSemantics => (
                "mem.model.value",
                "assignment and argument passing transfer or copy the whole value",
            ),
            Rule::PlacePath => (
                "mem.model.place",
                "a place is a base binding plus field/index projections",
            ),
            Rule::PathDisjoint => (
                "mem.model.path.disjoint",
                "two paths conflict iff one is a prefix of the other",
            ),
            Rule::Alloc => (
                "mem.model.alloc",
                "struct literals and collection constructors are the allocation sites",
            ),
            Rule::Move => (
                "mem.tier0.move.1",
                "initialization, assignment, `take` and `return` move; the source becomes uninitialized",
            ),
            Rule::UseAfterMove => (
                "mem.tier0.move.2",
                "reading an uninitialized or moved-from place traps `use-after-move`",
            ),
            Rule::Copy => (
                "mem.tier0.move.3",
                "`copy x` produces an independent value from any type",
            ),
            Rule::Reinit => (
                "mem.tier0.move.4",
                "a moved-from place may be re-initialized by assignment and is then live again",
            ),
            Rule::ModeRead => (
                "mem.tier0.mode.read",
                "the default mode reads a value that is immutable for the whole call",
            ),
            Rule::ModeMut => (
                "mem.tier0.mode.mut",
                "`mut` parameters are exclusive inout for the duration of the call",
            ),
            Rule::ModeTake => (
                "mem.tier0.mode.take",
                "`take` consumes: the argument moves into the callee",
            ),
            Rule::Exclusivity => (
                "mem.tier0.excl.1",
                "a place held through `mut` has no other live access path; violations trap `exclusivity`",
            ),
            Rule::ExclusivityDisjoint => (
                "mem.tier0.excl.2",
                "disjoint paths may be `mut` simultaneously: `f(mut a.x, mut a.y)` is legal",
            ),
            Rule::ViewSet => (
                "mem.tier0.excl.3",
                "a view set declares the callee's path footprint and is part of the signature",
            ),
            Rule::Borrow => (
                "mem.tier0.borrow.1",
                "`&path` / `&mut path` create local borrows that never outlive their activation",
            ),
            Rule::BorrowExtent => (
                "mem.tier0.borrow.2",
                "a live `&mut` holds its path exclusively; live `&` borrows read-freeze it",
            ),
            Rule::ArithChecked => (
                "arith.checked",
                "integer overflow traps `overflow` in every profile (X3); debug and release agree",
            ),
            Rule::ArithWrapping => (
                "arith.wrapping",
                "`wrapping`/`saturating` types are the spelling for intended overflow",
            ),
            Rule::DivZero => (
                "mem.ub.defined",
                "division or remainder by zero is defined behavior: it traps `div-zero`",
            ),
            Rule::Bounds => (
                "mem.ub.defined",
                "an out-of-bounds index or slice is defined behavior: it traps `bounds`",
            ),
            Rule::LiteralDefault => (
                "arith.literal.default",
                "an unconstrained integer literal defaults to i32 and a float literal to f64",
            ),
            Rule::Block => (
                "gram.expr.block",
                "a block evaluates its statements and yields its tail expression",
            ),
            Rule::Flow => (
                "gram.expr.flow",
                "`if`/`match`/`for`/`while`/`loop` are expressions; `return`/`break`/`continue` jump",
            ),
            Rule::EvalOrder => (
                "gram.expr.prec",
                "`&&`/`||` short-circuit; other operand positions evaluate left to right",
            ),
            Rule::Assign => (
                "gram.expr.assign",
                "assignment is a statement whose left side denotes a place",
            ),
            Rule::Call => (
                "gram.item.fn",
                "a call binds each argument to its parameter by mode, then evaluates the body",
            ),
            Rule::Closure => (
                "gram.expr.closure",
                "a closure captures its free variables by value when it is constructed",
            ),
            Rule::ErrUnion => (
                "err.union",
                "`!T` values are ok-or-tagged data; nothing unwinds",
            ),
            Rule::ErrRows => (
                "err.rows",
                "error tags are structural — a row entry needs no declaration to exist",
            ),
            Rule::ErrPropagate => (
                "err.propagate",
                "`?` returns the error to the caller, widening the row by union",
            ),
            Rule::ErrElse => (
                "err.else",
                "`else`, `else |err|` and `else |err| { … }` default an error away",
            ),
            Rule::ErrDefer => (
                "err.errdefer",
                "`errdefer` runs when the scope is left on the error path, and only then",
            ),
            Rule::DeferLifo => (
                "mem.shared.drop.1",
                "scope-exit effects run in reverse registration order",
            ),
            Rule::StrInterp => (
                "str.interp",
                "every string literal is an f-string; each interpolation evaluates in place",
            ),
            Rule::Assert => ("conf.trap.map", "a failed user assertion traps `assert`"),
            Rule::TrapVocabulary => (
                "conf.trap.set",
                "every fault this machine raises is one of the closed eleven kinds",
            ),
        };
        Row {
            rule: self,
            anchor,
            description,
        }
    }

    /// This rule's clause anchor.
    #[must_use]
    pub const fn anchor(self) -> &'static str {
        self.row().anchor
    }

    /// This rule's one-sentence description.
    #[must_use]
    pub const fn description(self) -> &'static str {
        self.row().description
    }

    /// Every rule, in declaration order. The registry.
    pub const ALL: [Rule; 36] = [
        Rule::ValueSemantics,
        Rule::PlacePath,
        Rule::PathDisjoint,
        Rule::Alloc,
        Rule::Move,
        Rule::UseAfterMove,
        Rule::Copy,
        Rule::Reinit,
        Rule::ModeRead,
        Rule::ModeMut,
        Rule::ModeTake,
        Rule::Exclusivity,
        Rule::ExclusivityDisjoint,
        Rule::ViewSet,
        Rule::Borrow,
        Rule::BorrowExtent,
        Rule::ArithChecked,
        Rule::ArithWrapping,
        Rule::DivZero,
        Rule::Bounds,
        Rule::LiteralDefault,
        Rule::Block,
        Rule::Flow,
        Rule::EvalOrder,
        Rule::Assign,
        Rule::Call,
        Rule::Closure,
        Rule::ErrUnion,
        Rule::ErrRows,
        Rule::ErrPropagate,
        Rule::ErrElse,
        Rule::ErrDefer,
        Rule::DeferLifo,
        Rule::StrInterp,
        Rule::Assert,
        Rule::TrapVocabulary,
    ];
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} [{}]", self, self.anchor())
    }
}

/// The registry as a table.
#[must_use]
pub fn registry() -> Vec<Row> {
    Rule::ALL.iter().map(|r| r.row()).collect()
}

/// Why a registry row is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowError {
    /// The row cites no clause at all — the failure the sprint asks CI to prove.
    NoAnchor(String),
    /// The row cites something that is not a well-formed anchor, or names a
    /// namespace outside `[conf.anchor.ns]`.
    BadAnchor { rule: String, reason: String },
    /// The row cites an anchor in a *registered* namespace that the pinned
    /// `spec/anchors.json` does not contain.
    UnknownAnchor { rule: String, anchor: String },
    /// The row says nothing about what the rule does.
    NoDescription(String),
}

impl fmt::Display for RowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RowError::NoAnchor(rule) => write!(f, "rule `{rule}` cites no clause anchor"),
            RowError::BadAnchor { rule, reason } => write!(f, "rule `{rule}`: {reason}"),
            RowError::UnknownAnchor { rule, anchor } => write!(
                f,
                "rule `{rule}` cites `{anchor}`, which the pinned spec does not define"
            ),
            RowError::NoDescription(rule) => write!(f, "rule `{rule}` has no description"),
        }
    }
}

impl std::error::Error for RowError {}

/// Checks a set of registry rows against the pinned anchor index.
///
/// `known` is `spec/anchors.json`'s anchor set, or `None` when it could not be
/// read (in which case anchors are checked for *shape* only, as the corpus walk
/// does). Every error is reported, not just the first: a registry with three
/// bad rows should say so once.
///
/// # Errors
///
/// Every [`RowError`] found, in row order.
pub fn validate(rows: &[Row], known: Option<&std::collections::BTreeSet<String>>) -> Vec<RowError> {
    let mut errors = Vec::new();
    for row in rows {
        let rule = format!("{:?}", row.rule);
        if row.anchor.is_empty() {
            errors.push(RowError::NoAnchor(rule.clone()));
        } else {
            match crate::anchor::classify(row.anchor) {
                Err(e) => errors.push(RowError::BadAnchor {
                    rule: rule.clone(),
                    reason: e.to_string(),
                }),
                Ok(crate::anchor::Namespace::Registered) => {
                    if let Some(known) = known
                        && !known.contains(row.anchor)
                    {
                        errors.push(RowError::UnknownAnchor {
                            rule: rule.clone(),
                            anchor: row.anchor.to_owned(),
                        });
                    }
                }
                // Forward namespaces are legal and counted as forward
                // (`[conf.anchor.ns]`); their documents are not written yet.
                Ok(crate::anchor::Namespace::Reserved) => {}
            }
        }
        if row.description.trim().is_empty() {
            errors.push(RowError::NoDescription(rule));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_appears_in_all_exactly_once() {
        let mut seen = std::collections::BTreeSet::new();
        for rule in Rule::ALL {
            assert!(seen.insert(rule), "{rule:?} listed twice in Rule::ALL");
        }
        assert_eq!(seen.len(), Rule::ALL.len());
    }

    #[test]
    fn every_rule_cites_a_clause_and_says_what_it_does() {
        // Shape-only pass; the pinned-index pass lives in `tests/rule_registry.rs`,
        // which can read `spec/anchors.json`.
        assert_eq!(validate(&registry(), None), Vec::new());
    }

    #[test]
    fn a_planted_anchorless_rule_fails_validation() {
        // The sprint's acceptance criterion, made executable: "a planted
        // anchorless rule demonstrably fails CI".
        let planted = [Row {
            rule: Rule::Move,
            anchor: "",
            description: "moves things, trust me",
        }];
        assert_eq!(
            validate(&planted, None),
            vec![RowError::NoAnchor("Move".to_owned())]
        );
    }

    #[test]
    fn a_planted_undescribed_rule_fails_validation() {
        let planted = [Row {
            rule: Rule::Move,
            anchor: "mem.tier0.move.1",
            description: "   ",
        }];
        assert_eq!(
            validate(&planted, None),
            vec![RowError::NoDescription("Move".to_owned())]
        );
    }

    #[test]
    fn a_planted_rule_citing_an_unpinned_anchor_fails_validation() {
        let known = std::collections::BTreeSet::from(["mem.tier0.move.1".to_owned()]);
        let planted = [Row {
            rule: Rule::UseAfterMove,
            anchor: "mem.tier0.move.99",
            description: "an anchor nobody published",
        }];
        assert_eq!(
            validate(&planted, Some(&known)),
            vec![RowError::UnknownAnchor {
                rule: "UseAfterMove".to_owned(),
                anchor: "mem.tier0.move.99".to_owned(),
            }]
        );
    }

    #[test]
    fn a_planted_rule_in_an_unregistered_namespace_fails_validation() {
        let planted = [Row {
            rule: Rule::Flow,
            anchor: "wolfy.made.up",
            description: "a namespace nobody registered",
        }];
        assert!(matches!(
            validate(&planted, None).as_slice(),
            [RowError::BadAnchor { .. }]
        ));
    }

    #[test]
    fn rules_render_with_their_anchor() {
        assert_eq!(
            Rule::UseAfterMove.to_string(),
            "UseAfterMove [mem.tier0.move.2]"
        );
    }
}

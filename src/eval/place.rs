//! Places, paths, and dynamic exclusivity.
//!
//! `[mem.model.place]`:
//!
//! > A **place** is a storage location denoted by a **path**: a base binding
//! > followed by field/index projections (`a.x.y`, `xs[i]`). Paths are
//! > field-granular: `a.x` and `a.y` are disjoint places.
//! > `[mem.model.path.disjoint]` Two paths conflict iff one is a prefix of the
//! > other (after identical projections); otherwise they are disjoint.
//!
//! That clause is implemented literally in [`Path::conflicts_with`], and it is
//! the *only* place disjointness is decided — the exclusivity check, the
//! borrow check and the argument check all call it. `f(mut a.x, mut a.y)` is
//! legal and `f(mut a, mut a.x)` is not, for the one reason the clause gives.
//!
//! Exclusivity itself (`[mem.tier0.excl.1]`) is a *set of held accesses* rather
//! than a per-slot counter, because the unit of the rule is a path and paths
//! are not slots: `a.x` and `a` are different paths over the same storage. The
//! set is a stack — accesses are pushed for a call's extent or a borrow's
//! extent and popped when it ends — so the check is "does this new access
//! conflict with anything still held", which is the clause read aloud.

use std::fmt;

use crate::diag::Span;

/// One projection step of a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Proj {
    Field(String),
    /// A constant index. A *dynamic* index whose value the machine knows is
    /// recorded here too — at run time every index is constant.
    Index(i128),
    /// A map key, rendered. Wolf map keys are `str`/int-shaped in the pinned
    /// corpus, and rendering them is faithful for exactly those: two keys are
    /// the same place iff they render the same. A structured key would need a
    /// real value comparison here, and that is where this representation would
    /// have to grow.
    Key(String),
    /// An index the machine could not reduce to a constant (a key expression
    /// with no evaluated value). Conservatively conflicts with every sibling
    /// index, which keeps the check sound in the only direction that matters.
    UnknownIndex,
}

impl fmt::Display for Proj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Proj::Field(name) => write!(f, ".{name}"),
            Proj::Index(i) => write!(f, "[{i}]"),
            Proj::Key(key) => write!(f, "[{key:?}]"),
            Proj::UnknownIndex => f.write_str("[?]"),
        }
    }
}

impl Proj {
    /// Whether two projection steps *may* denote the same storage.
    fn may_equal(&self, other: &Proj) -> bool {
        match (self, other) {
            (Proj::Field(a), Proj::Field(b)) => a == b,
            (Proj::Index(a), Proj::Index(b)) => a == b,
            (Proj::Key(a), Proj::Key(b)) => a == b,
            (Proj::UnknownIndex, Proj::Index(_) | Proj::Key(_) | Proj::UnknownIndex)
            | (Proj::Index(_) | Proj::Key(_), Proj::UnknownIndex) => true,
            _ => false,
        }
    }
}

/// A path: a base binding plus projections (`[mem.model.place]`).
///
/// The base is `(frame, name)` rather than a name alone, because two
/// activations of the same function have distinct locals and a recursive call
/// must not look like an exclusivity violation against its own caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub frame: usize,
    pub base: String,
    pub projections: Vec<Proj>,
}

impl Path {
    #[must_use]
    pub fn local(frame: usize, base: impl Into<String>) -> Path {
        Path {
            frame,
            base: base.into(),
            projections: Vec::new(),
        }
    }

    #[must_use]
    pub fn project(mut self, step: Proj) -> Path {
        self.projections.push(step);
        self
    }

    /// `[mem.model.path.disjoint]`, verbatim: two paths conflict iff one is a
    /// prefix of the other after identical projections.
    #[must_use]
    pub fn conflicts_with(&self, other: &Path) -> bool {
        if self.frame != other.frame || self.base != other.base {
            return false;
        }
        let shared = self.projections.len().min(other.projections.len());
        for i in 0..shared {
            if !self.projections[i].may_equal(&other.projections[i]) {
                // The paths diverge at step `i`: disjoint places.
                return false;
            }
        }
        // No divergence: the shorter is a prefix of the longer (or they are
        // equal). Either way, one contains the other.
        true
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.base)?;
        for step in &self.projections {
            write!(f, "{step}")?;
        }
        Ok(())
    }
}

/// How a path is being touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// `read` passing and ordinary reads: several may coexist.
    Shared,
    /// `mut` passing, `&mut`, and writes: exclusive.
    Exclusive,
}

impl fmt::Display for Access {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Access::Shared => "read",
            Access::Exclusive => "mut",
        })
    }
}

/// An access held open for some extent: a call's, or a borrow binding's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub path: Path,
    pub access: Access,
    /// Where the access was taken — the second span a fault prints.
    pub span: Span,
}

/// The set of accesses currently held (`[mem.tier0.excl.1]`).
#[derive(Debug, Clone, Default)]
pub struct AccessSet {
    held: Vec<Held>,
}

impl AccessSet {
    #[must_use]
    pub fn new() -> AccessSet {
        AccessSet::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.held.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// The first held access that conflicts with `path` under `access`, if any.
    ///
    /// Two accesses conflict when their paths conflict *and* at least one of
    /// them is exclusive — `[mem.tier0.excl.1]` bans "any other live access
    /// path" only for `mut`; several `read`s coexist by
    /// `[mem.tier0.mode.read]`.
    #[must_use]
    pub fn conflict(&self, path: &Path, access: Access) -> Option<&Held> {
        self.held.iter().find(|held| {
            (held.access == Access::Exclusive || access == Access::Exclusive)
                && held.path.conflicts_with(path)
        })
    }

    /// A held access over the *same base* that this path does not conflict
    /// with — `[mem.tier0.excl.2]`'s "disjoint paths may be `mut`
    /// simultaneously", observed rather than merely permitted.
    #[must_use]
    pub fn disjoint_sibling(&self, path: &Path) -> Option<&Held> {
        self.held.iter().find(|held| {
            held.path.frame == path.frame
                && held.path.base == path.base
                && !held.path.conflicts_with(path)
        })
    }

    /// Holds an access open. The caller has already checked for conflicts.
    pub fn push(&mut self, held: Held) {
        self.held.push(held);
    }

    /// Releases the most recently held `count` accesses — the end of a call's
    /// or a borrow's extent.
    pub fn release(&mut self, count: usize) {
        let keep = self.held.len().saturating_sub(count);
        self.held.truncate(keep);
    }

    /// Releases every access whose base lives in `frame` or deeper — a frame's
    /// locals dying takes their borrows with them (`[mem.tier0.borrow.1]`: no
    /// borrow outlives its function activation).
    pub fn release_frame(&mut self, frame: usize) {
        self.held.retain(|held| held.path.frame < frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(base: &str, steps: &[&str]) -> Path {
        let mut path = Path::local(0, base);
        for step in steps {
            path = path.project(match step.parse::<i128>() {
                Ok(i) => Proj::Index(i),
                Err(_) => Proj::Field((*step).to_owned()),
            });
        }
        path
    }

    #[test]
    fn the_disjointness_clause_reads_exactly_as_written() {
        // "`f(mut a.x, mut a.y)` is legal by `[mem.model.path.disjoint]`;
        //  `f(mut a, mut a.x)` is E1002."
        assert!(!p("a", &["x"]).conflicts_with(&p("a", &["y"])));
        assert!(p("a", &[]).conflicts_with(&p("a", &["x"])));
        assert!(p("a", &["x"]).conflicts_with(&p("a", &[])));
        assert!(p("a", &["x"]).conflicts_with(&p("a", &["x"])));
        assert!(!p("a", &["x"]).conflicts_with(&p("b", &["x"])));
        // Deeper: divergence at any step makes them disjoint.
        assert!(!p("a", &["x", "u"]).conflicts_with(&p("a", &["x", "v"])));
        assert!(p("a", &["x", "u"]).conflicts_with(&p("a", &["x"])));
    }

    #[test]
    fn indices_are_paths_too() {
        assert!(!p("xs", &["0"]).conflicts_with(&p("xs", &["1"])));
        assert!(p("xs", &["0"]).conflicts_with(&p("xs", &["0"])));
        assert!(p("xs", &[]).conflicts_with(&p("xs", &["7"])));
        // An index the machine could not reduce conflicts with every sibling —
        // conservative in the direction that keeps the check sound.
        let unknown = Path::local(0, "xs").project(Proj::UnknownIndex);
        assert!(unknown.conflicts_with(&p("xs", &["3"])));
    }

    #[test]
    fn a_disjoint_sibling_is_found_and_a_conflicting_one_is_not() {
        let mut set = AccessSet::new();
        set.push(Held {
            path: p("a", &["x"]),
            access: Access::Exclusive,
            span: Span::new(0, 1),
        });
        assert!(set.disjoint_sibling(&p("a", &["y"])).is_some());
        assert!(set.disjoint_sibling(&p("a", &["x"])).is_none());
        assert!(set.disjoint_sibling(&p("b", &["y"])).is_none());
    }

    #[test]
    fn different_activations_of_the_same_local_never_conflict() {
        let outer = Path::local(0, "n");
        let inner = Path::local(1, "n");
        assert!(!outer.conflicts_with(&inner));
    }

    #[test]
    fn reads_coexist_and_mut_excludes() {
        let mut set = AccessSet::new();
        set.push(Held {
            path: p("a", &[]),
            access: Access::Shared,
            span: Span::new(0, 1),
        });
        assert!(set.conflict(&p("a", &[]), Access::Shared).is_none());
        assert!(set.conflict(&p("a", &[]), Access::Exclusive).is_some());

        let mut set = AccessSet::new();
        set.push(Held {
            path: p("a", &["x"]),
            access: Access::Exclusive,
            span: Span::new(0, 1),
        });
        assert!(set.conflict(&p("a", &["x"]), Access::Shared).is_some());
        assert!(set.conflict(&p("a", &["y"]), Access::Exclusive).is_none());
        assert!(set.conflict(&p("a", &[]), Access::Exclusive).is_some());
    }

    #[test]
    fn releasing_a_frame_drops_its_borrows() {
        let mut set = AccessSet::new();
        set.push(Held {
            path: Path::local(0, "outer"),
            access: Access::Exclusive,
            span: Span::new(0, 1),
        });
        set.push(Held {
            path: Path::local(1, "inner"),
            access: Access::Exclusive,
            span: Span::new(2, 3),
        });
        set.release_frame(1);
        assert_eq!(set.len(), 1);
        assert!(
            set.conflict(&Path::local(1, "inner"), Access::Exclusive)
                .is_none()
        );
    }
}

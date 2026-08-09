//! The trap vocabulary.
//!
//! Reimplemented from `spec/05-conformance.md` `[conf.trap.set]`: a **closed**
//! set of eleven kinds. It is the comparison alphabet of spec/06, so an
//! unknown kind is an error everywhere it can appear — corpus directives and
//! protocol verdicts alike.

use std::fmt;
use std::str::FromStr;

/// One of the eleven kinds in the closed `[conf.trap.set]` vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrapKind {
    Overflow,
    DivZero,
    Bounds,
    UseAfterMove,
    Exclusivity,
    RegionFault,
    StaleHandle,
    AllocContract,
    Assert,
    Race,
    Ub,
}

impl TrapKind {
    /// The whole closed set, in spec order. Extension requires a spec revision.
    pub const ALL: [TrapKind; 11] = [
        TrapKind::Overflow,
        TrapKind::DivZero,
        TrapKind::Bounds,
        TrapKind::UseAfterMove,
        TrapKind::Exclusivity,
        TrapKind::RegionFault,
        TrapKind::StaleHandle,
        TrapKind::AllocContract,
        TrapKind::Assert,
        TrapKind::Race,
        TrapKind::Ub,
    ];

    /// The spelling used in directives and on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            TrapKind::Overflow => "overflow",
            TrapKind::DivZero => "div-zero",
            TrapKind::Bounds => "bounds",
            TrapKind::UseAfterMove => "use-after-move",
            TrapKind::Exclusivity => "exclusivity",
            TrapKind::RegionFault => "region-fault",
            TrapKind::StaleHandle => "stale-handle",
            TrapKind::AllocContract => "alloc-contract",
            TrapKind::Assert => "assert",
            TrapKind::Race => "race",
            TrapKind::Ub => "ub",
        }
    }

    /// Parses a kind, or `None` for anything outside the closed set.
    #[must_use]
    pub fn parse(s: &str) -> Option<TrapKind> {
        TrapKind::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// The closed set rendered for diagnostics.
    #[must_use]
    pub fn vocabulary() -> String {
        TrapKind::ALL
            .iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for TrapKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returned when a string is not a member of the closed trap vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrapKindParseError(pub String);

impl fmt::Display for TrapKindParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown trap kind `{}` (closed set: {})",
            self.0,
            TrapKind::vocabulary()
        )
    }
}

impl std::error::Error for TrapKindParseError {}

impl FromStr for TrapKind {
    type Err = TrapKindParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TrapKind::parse(s).ok_or_else(|| TrapKindParseError(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_set_has_exactly_eleven_kinds() {
        assert_eq!(TrapKind::ALL.len(), 11);
    }

    #[test]
    fn every_kind_round_trips() {
        for kind in TrapKind::ALL {
            assert_eq!(TrapKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn spellings_are_hyphenated_exactly_as_the_spec_writes_them() {
        assert_eq!(TrapKind::DivZero.as_str(), "div-zero");
        assert_eq!(TrapKind::UseAfterMove.as_str(), "use-after-move");
        assert_eq!(TrapKind::StaleHandle.as_str(), "stale-handle");
        assert_eq!(TrapKind::AllocContract.as_str(), "alloc-contract");
        assert_eq!(TrapKind::RegionFault.as_str(), "region-fault");
    }

    #[test]
    fn the_set_is_closed() {
        // Plausible-looking near-misses that are NOT in the vocabulary.
        for bad in [
            "div_zero",
            "divzero",
            "oob",
            "panic",
            "unwind",
            "leak",
            "double-free",
            "use-after-free",
            "Overflow",
            "",
        ] {
            assert!(TrapKind::parse(bad).is_none(), "{bad} must be rejected");
        }
    }
}

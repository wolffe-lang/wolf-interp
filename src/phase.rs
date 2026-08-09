//! The canonical phase ladder.
//!
//! Reimplemented from `spec/06-differential-protocol.md` `[proto.invoke.cli]`
//! (and echoed by `corpus/README.md`): `none, lex, parse, resolve, typecheck,
//! mem, wir, run`. The ladder is *ordered* — `--phase=<p>` means "stop after
//! `<p>`" — so `Phase` is comparable.

use std::fmt;
use std::str::FromStr;

/// A rung of the canonical phase ladder, ordered shallowest-first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    None,
    Lex,
    Parse,
    Resolve,
    Typecheck,
    Mem,
    Wir,
    Run,
}

impl Phase {
    /// Every rung, shallowest first. The ladder is closed.
    pub const LADDER: [Phase; 8] = [
        Phase::None,
        Phase::Lex,
        Phase::Parse,
        Phase::Resolve,
        Phase::Typecheck,
        Phase::Mem,
        Phase::Wir,
        Phase::Run,
    ];

    /// The spelling used on the wire and in corpus directives.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::None => "none",
            Phase::Lex => "lex",
            Phase::Parse => "parse",
            Phase::Resolve => "resolve",
            Phase::Typecheck => "typecheck",
            Phase::Mem => "mem",
            Phase::Wir => "wir",
            Phase::Run => "run",
        }
    }

    /// Parses a ladder rung, or `None` for anything outside the closed set.
    #[must_use]
    pub fn parse(s: &str) -> Option<Phase> {
        Phase::LADDER.into_iter().find(|p| p.as_str() == s)
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returned when a string is not a rung of the canonical ladder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseParseError(pub String);

impl fmt::Display for PhaseParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ladder = Phase::LADDER
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "unknown phase `{}` (canonical ladder: {ladder})", self.0)
    }
}

impl std::error::Error for PhaseParseError {}

impl FromStr for Phase {
    type Err = PhaseParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Phase::parse(s).ok_or_else(|| PhaseParseError(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_round_trips() {
        for phase in Phase::LADDER {
            assert_eq!(Phase::parse(phase.as_str()), Some(phase));
        }
    }

    #[test]
    fn ladder_is_ordered_shallow_to_deep() {
        assert!(Phase::None < Phase::Lex);
        assert!(Phase::Lex < Phase::Parse);
        assert!(Phase::Mem < Phase::Run);
    }

    #[test]
    fn unknown_rungs_are_rejected() {
        for bad in ["", "RUN", "codegen", "borrowck", "none "] {
            assert!(Phase::parse(bad).is_none(), "{bad} should not parse");
        }
        assert!("codegen".parse::<Phase>().is_err());
    }
}

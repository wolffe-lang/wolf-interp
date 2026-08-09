//! The frontend as a rung of the canonical ladder.
//!
//! This is where `spec/01`'s lexer and parser meet `spec/06`'s observation
//! record. The whole file is one contract: `[proto.record.phase]`.
//!
//! > `phase_reached` names the deepest phase that **completed**. An
//! > implementation that cannot complete a phase because a construct is outside
//! > its current coverage reports the last *completed* phase with verdict
//! > `unsupported` — never the incomplete phase. A `fail(CODE)` verdict reports
//! > the phase that failed as `phase_reached`.
//!
//! Read literally, that gives four shapes and no others:
//!
//! | what happened | `phase_reached` | `verdict` |
//! |---|---|---|
//! | lexing failed | `lex` | `fail(CODE)` |
//! | parsing failed | `parse` | `fail(CODE)` |
//! | clean, and `--phase` asked for `lex`/`parse` | that rung | `pass` |
//! | clean, and `--phase` asked for more (or nothing) | `parse` | `unsupported` |
//!
//! The last row is the one worth being pedantic about: at is01 there is no
//! resolver, so `--phase=resolve` must not claim `resolve`. It reports the
//! deepest rung that *completed* — `parse` — and declares itself unsupported,
//! which keeps the scope gap in the conservatism ledger instead of silently
//! passing (`[proto.record.unsupported]`).

use crate::diag::Diag;
use crate::lex;
use crate::parse;
use crate::phase::Phase;
use crate::protocol::{Diagnostic, Verdict};

/// What the frontend made of one program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub phase_reached: Phase,
    pub verdict: Verdict,
    /// Empty unless the verdict is `fail`, and then exactly one: this parser
    /// performs no recovery, so there is no second diagnostic to report and
    /// `[proto.cmp.phase]` compares only the first anyway.
    pub diagnostics: Vec<Diagnostic>,
    /// The diagnostic behind a `fail`, with its message and clause anchor —
    /// for humans, never for the wire.
    pub detail: Option<Diag>,
}

impl Observation {
    fn failed(phase: Phase, diag: Diag) -> Observation {
        Observation {
            phase_reached: phase,
            verdict: Verdict::Fail(diag.code.to_owned()),
            diagnostics: vec![diag.to_protocol()],
            detail: Some(diag),
        }
    }

    fn clean(phase: Phase, verdict: Verdict) -> Observation {
        Observation {
            phase_reached: phase,
            verdict,
            diagnostics: Vec::new(),
            detail: None,
        }
    }
}

/// The deepest rung this implementation can reach on a well-formed program.
pub const DEEPEST_SUPPORTED: Phase = Phase::Parse;

/// Observes one source file at the requested rung.
///
/// `requested` is `--phase=<p>`; `None` means "as deep as the implementation
/// can" (`[proto.invoke.cli]`).
#[must_use]
pub fn observe(source: &[u8], requested: Option<Phase>) -> Observation {
    if requested == Some(Phase::None) {
        return Observation::clean(Phase::None, Verdict::Pass);
    }

    let lexed = lex::lex_bytes(source);
    if let Some(error) = lexed.first_error() {
        return Observation::failed(Phase::Lex, error.clone());
    }
    if requested == Some(Phase::Lex) {
        return Observation::clean(Phase::Lex, Verdict::Pass);
    }

    match parse::parse(&lexed) {
        Err(error) => {
            // A deferred lex diagnostic (E0007) is a parse-tier finding; when
            // it sits earlier in the file than the parse error, it is the first
            // diagnostic and the one the protocol compares.
            let first = lexed
                .deferred
                .iter()
                .min_by_key(|d| d.span.start)
                .filter(|d| d.span.start <= error.span.start)
                .cloned()
                .unwrap_or(error);
            Observation::failed(Phase::Parse, first)
        }
        Ok(parsed) => match parsed.deferred.into_iter().min_by_key(|d| d.span.start) {
            Some(deferred) => Observation::failed(Phase::Parse, deferred),
            None if requested == Some(Phase::Parse) => {
                Observation::clean(Phase::Parse, Verdict::Pass)
            }
            // Anything deeper is outside is01's scope. Report the last
            // *completed* rung, never the incomplete one.
            None => Observation::clean(DEEPEST_SUPPORTED, Verdict::Unsupported),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLEAN: &str =
        "fn main() -> !int {\n    let who = \"wolf\"\n    print(\"hi, {who}\")\n    0\n}\n";
    const BAD_PARSE: &str = "fn main() -> !int {\n    let a = 1\n        + 2\n    0\n}\n";
    const BAD_LEX: &str = "fn main() -> !int {\n    let s = \"unclosed\n    0\n}\n";

    #[test]
    fn a_clean_program_passes_at_the_rung_it_was_asked_for() {
        for rung in [Phase::Lex, Phase::Parse] {
            let obs = observe(CLEAN.as_bytes(), Some(rung));
            assert_eq!(obs.verdict, Verdict::Pass);
            assert_eq!(obs.phase_reached, rung);
            assert!(obs.diagnostics.is_empty());
        }
    }

    #[test]
    fn deeper_rungs_are_unsupported_at_the_deepest_completed_phase() {
        for rung in [
            None,
            Some(Phase::Resolve),
            Some(Phase::Typecheck),
            Some(Phase::Mem),
            Some(Phase::Wir),
            Some(Phase::Run),
        ] {
            let obs = observe(CLEAN.as_bytes(), rung);
            assert_eq!(obs.verdict, Verdict::Unsupported, "{rung:?}");
            assert_eq!(obs.phase_reached, Phase::Parse, "{rung:?}");
        }
    }

    #[test]
    fn a_parse_failure_reports_the_phase_that_failed() {
        let obs = observe(BAD_PARSE.as_bytes(), None);
        assert_eq!(obs.phase_reached, Phase::Parse);
        assert_eq!(obs.verdict, Verdict::Fail("E0001".to_owned()));
        assert_eq!(obs.diagnostics.len(), 1);
        assert_eq!(obs.diagnostics[0].severity, "error");
    }

    #[test]
    fn a_parse_failure_still_passes_the_lex_rung() {
        let obs = observe(BAD_PARSE.as_bytes(), Some(Phase::Lex));
        assert_eq!(obs.verdict, Verdict::Pass);
        assert_eq!(obs.phase_reached, Phase::Lex);
    }

    #[test]
    fn a_lex_failure_reports_the_lex_rung_at_every_depth() {
        for rung in [None, Some(Phase::Lex), Some(Phase::Parse), Some(Phase::Run)] {
            let obs = observe(BAD_LEX.as_bytes(), rung);
            assert_eq!(obs.phase_reached, Phase::Lex, "{rung:?}");
            assert!(matches!(obs.verdict, Verdict::Fail(_)), "{rung:?}");
        }
    }

    #[test]
    fn phase_none_completes_trivially() {
        let obs = observe(BAD_LEX.as_bytes(), Some(Phase::None));
        assert_eq!(obs.phase_reached, Phase::None);
        assert_eq!(obs.verdict, Verdict::Pass);
    }

    #[test]
    fn spans_on_the_wire_are_byte_offsets_into_the_source() {
        let obs = observe(BAD_PARSE.as_bytes(), None);
        let [start, end] = obs.diagnostics[0].span;
        assert!(start < end);
        assert_eq!(&BAD_PARSE.as_bytes()[start as usize..end as usize], b"+");
    }
}

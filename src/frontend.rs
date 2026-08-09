//! The phase ladder, and what this implementation actually completes on it.
//!
//! This is where `spec/01`'s lexer and parser, `sema`'s module machinery and
//! `eval`'s dynamic semantics meet `spec/06`'s observation record. The whole
//! file is one contract: `[proto.record.phase]`.
//!
//! > `phase_reached` names the deepest phase that **completed**. An
//! > implementation that cannot complete a phase because a construct is outside
//! > its current coverage reports the last *completed* phase with verdict
//! > `unsupported` — never the incomplete phase. A `fail(CODE)` verdict reports
//! > the phase that failed as `phase_reached`.
//!
//! # The ladder mapping (is02–is03)
//!
//! The canonical ladder is `none, lex, parse, resolve, typecheck, mem, wir,
//! run`. It is the *compiler's* pipeline; an interpreter does not have all of
//! it, and pretending otherwise is what `[proto.record.phase]` forbids. What
//! this implementation completes, rung by rung:
//!
//! | rung | wolf-interp | note |
//! |---|---|---|
//! | `lex` | the mode-stack lexer | is01 |
//! | `parse` | the full-surface parser | is01 |
//! | `resolve` | **sema-lite** — the D32 module graph, item collection, visibility | is02 |
//! | `typecheck` | *not implemented* | the type checker is the compiler's half (`sema` docs) |
//! | `mem` | *not implemented* | the borrow/region checkers are the compiler's half |
//! | `wir` | *not implemented* | there is no IR here; a tree-walk has no lowering |
//! | `run` | the tree-walk evaluator, with is03's dynamic region machine | is02–is03 |
//!
//! Two consequences, both deliberate:
//!
//! - `--phase=typecheck|mem|wir` reports `resolve` + `unsupported`. It asks the
//!   implementation to *stop after* a phase this one never performs, and the
//!   deepest thing it did complete is `resolve`.
//! - `--phase=run` (and the default) can report `run` even though `typecheck`
//!   and `mem` were skipped, because the run **completed**. The static rungs
//!   this machine skips are exactly the properties it enforces dynamically
//!   instead — that is the sema boundary, not a gap in the evidence.
//!
//! A program the machine cannot evaluate — regions, `shared`, concurrency,
//! comptime, method dispatch, an unimplemented std name — reports `resolve`
//! (the last completed rung) with verdict `unsupported` and the reason on the
//! `x-unsupported` extension key (`[proto.record.ext]`; the verdict itself
//! carries no payload, per `[proto.record.verdict]`).

use std::path::Path;

use crate::diag::Diag;
use crate::eval::prov::UbFinding;
use crate::eval::{Machine, Outcome, Trap};
use crate::lex;
use crate::parse;
use crate::phase::Phase;
use crate::protocol::{Diagnostic, Verdict};
use crate::sema::{self, LoadError};

/// What the implementation made of one program.
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
    /// The program's output, when it ran.
    pub stdout: Vec<u8>,
    /// Why the verdict is `unsupported`. Rides `x-unsupported`; never the
    /// verdict itself (`[proto.record.verdict]` gives verdicts no payload).
    pub reason: Option<String>,
    /// The fault behind a `trap`, for humans: spans, clause anchor, one
    /// sentence.
    pub trap: Option<Trap>,
    /// The finding behind a `ub` verdict: the §7 row, the two spans, the
    /// licensed optimization and the borrow-tree slice.
    pub ub: Option<UbFinding>,
    /// `--trace`'s rule log, when it was asked for.
    pub trace: Vec<String>,
    /// Regions still holding memory when the program ended (is03's leak
    /// assertion). Empty for every phase that does not run.
    pub leaks: Vec<crate::eval::region::RegionId>,
    /// C allocations never freed. Leaks are defined and safe
    /// (`[mem.ub.defined]`); this is a report.
    pub host_leaks: Vec<crate::eval::prov::AllocId>,
    /// `Err` when the region forest invariant broke during the run — an
    /// interpreter bug, surfaced rather than swallowed.
    pub forest: Result<(), String>,
}

impl Observation {
    fn failed(phase: Phase, diag: Diag) -> Observation {
        Observation {
            diagnostics: vec![diag.to_protocol()],
            verdict: Verdict::Fail(diag.code.to_owned()),
            detail: Some(diag),
            ..Observation::clean(phase, Verdict::Pass)
        }
    }

    fn clean(phase: Phase, verdict: Verdict) -> Observation {
        Observation {
            phase_reached: phase,
            verdict,
            diagnostics: Vec::new(),
            detail: None,
            stdout: Vec::new(),
            reason: None,
            trap: None,
            ub: None,
            trace: Vec::new(),
            leaks: Vec::new(),
            host_leaks: Vec::new(),
            forest: Ok(()),
        }
    }

    fn unsupported(phase: Phase, reason: impl Into<String>) -> Observation {
        Observation {
            reason: Some(reason.into()),
            ..Observation::clean(phase, Verdict::Unsupported)
        }
    }
}

/// The deepest rung this implementation can reach on a well-formed program it
/// can evaluate.
pub const DEEPEST_SUPPORTED: Phase = Phase::Run;

/// The deepest *static* rung this implementation completes. Everything between
/// here and [`DEEPEST_SUPPORTED`] belongs to the compiler.
pub const DEEPEST_STATIC: Phase = Phase::Resolve;

/// Observes one source buffer at the requested rung, with no module graph.
///
/// The frontend-only door: `lex`, `parse`, and — because a single buffer is a
/// one-file root module — `resolve` and `run` as well.
#[must_use]
pub fn observe(source: &[u8], requested: Option<Phase>) -> Observation {
    observe_with(None, source, requested, crate::eval::Trace::Off)
}

/// Observes the program rooted at `file`, loading its module graph (D32:
/// directory = module).
#[must_use]
pub fn observe_file(
    file: &Path,
    source: &[u8],
    requested: Option<Phase>,
    trace: crate::eval::Trace,
) -> Observation {
    observe_with(Some(file), source, requested, trace)
}

fn observe_with(
    file: Option<&Path>,
    source: &[u8],
    requested: Option<Phase>,
    trace: crate::eval::Trace,
) -> Observation {
    if requested == Some(Phase::None) {
        return Observation::clean(Phase::None, Verdict::Pass);
    }

    // -- lex ---------------------------------------------------------------
    let lexed = lex::lex_bytes(source);
    if let Some(error) = lexed.first_error() {
        return Observation::failed(Phase::Lex, error.clone());
    }
    if requested == Some(Phase::Lex) {
        return Observation::clean(Phase::Lex, Verdict::Pass);
    }

    // -- parse -------------------------------------------------------------
    let parsed = match parse::parse(&lexed) {
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
            return Observation::failed(Phase::Parse, first);
        }
        Ok(parsed) => parsed,
    };
    if let Some(deferred) = parsed.deferred.iter().min_by_key(|d| d.span.start) {
        return Observation::failed(Phase::Parse, deferred.clone());
    }
    if requested == Some(Phase::Parse) {
        return Observation::clean(Phase::Parse, Verdict::Pass);
    }

    // -- resolve (sema-lite) ----------------------------------------------
    let program = match file {
        Some(file) => sema::load(file),
        None => sema::load_source("<buffer>", std::str::from_utf8(source).unwrap_or_default()),
    };
    let program = match program {
        Ok(program) => program,
        Err(LoadError::Syntax { file, diag }) => {
            // A sibling file of the same module failed the frontend. The phase
            // that failed is that file's parse rung, and its code is the one
            // the protocol compares.
            let mut observation = Observation::failed(Phase::Parse, *diag);
            observation.reason = Some(format!("in module file `{file}`"));
            return observation;
        }
        Err(LoadError::Io(message)) => {
            return Observation::unsupported(
                Phase::Parse,
                format!("the module graph could not be read: {message}"),
            );
        }
    };
    if requested == Some(Phase::Resolve) {
        return Observation::clean(Phase::Resolve, Verdict::Pass);
    }

    // -- typecheck / mem / wir: not this implementation's -------------------
    if matches!(requested, Some(Phase::Typecheck | Phase::Mem | Phase::Wir)) {
        let asked = requested.expect("checked");
        return Observation::unsupported(
            DEEPEST_STATIC,
            format!(
                "`--phase={asked}` asks this implementation to stop after a phase it does not \
                 perform: the type checker, the borrow/region checkers and the IR are the \
                 compiler's half of the split. Every property they prove statically is enforced \
                 dynamically at `run` instead"
            ),
        );
    }

    // -- run ---------------------------------------------------------------
    let run = Machine::new(&program).tracing(trace).run();
    let mut observation = match run.outcome {
        Outcome::Exit(status) => Observation::clean(Phase::Run, Verdict::Exit(status)),
        Outcome::Trap(trap) => Observation {
            trap: Some((*trap).clone()),
            ..Observation::clean(Phase::Run, Verdict::Trap(trap.kind))
        },
        // `[proto.record.phase]`: the run *completed* — it reached a defined
        // stopping point, and "this execution has no defined behavior" is that
        // point. `run` is therefore the deepest completed phase, exactly as it
        // is for a trap; the verdict is what carries the difference.
        Outcome::Ub(finding) => Observation {
            ub: Some((*finding).clone()),
            ..Observation::clean(Phase::Run, Verdict::Ub(finding.anchor().to_owned()))
        },
        // The run did not complete, so `run` is not the deepest *completed*
        // phase — `resolve` is. Inflating it here is exactly the lie
        // `[proto.record.phase]` names.
        Outcome::Unsupported(reason) => Observation::unsupported(DEEPEST_STATIC, reason),
    };
    observation.stdout = run.stdout;
    observation.trace = run.trace;
    observation.leaks = run.leaks;
    observation.host_leaks = run.host_leaks;
    observation.forest = run.forest;
    observation
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLEAN: &str =
        "fn main() -> !int {\n    let who = \"wolf\"\n    print(\"hi, {who}\")\n    0\n}\n";
    const BAD_PARSE: &str = "fn main() -> !int {\n    let a = 1\n        + 2\n    0\n}\n";
    const BAD_LEX: &str = "fn main() -> !int {\n    let s = \"unclosed\n    0\n}\n";
    const REGIONS: &str = "fn main() -> !int {\n    region r { let a = 1 }\n    0\n}\n";
    const CONCURRENCY: &str = "fn main() -> !int {\n    scope s { let a = 1 }\n    0\n}\n";

    #[test]
    fn a_clean_program_passes_at_the_rung_it_was_asked_for() {
        for rung in [Phase::Lex, Phase::Parse, Phase::Resolve] {
            let obs = observe(CLEAN.as_bytes(), Some(rung));
            assert_eq!(obs.verdict, Verdict::Pass, "{rung:?}");
            assert_eq!(obs.phase_reached, rung);
            assert!(obs.diagnostics.is_empty());
        }
    }

    #[test]
    fn a_clean_program_runs_at_the_run_rung_and_by_default() {
        for rung in [None, Some(Phase::Run)] {
            let obs = observe(CLEAN.as_bytes(), rung);
            assert_eq!(obs.verdict, Verdict::Exit(0), "{rung:?}");
            assert_eq!(obs.phase_reached, Phase::Run);
            assert_eq!(obs.stdout, b"hi, wolf\n");
        }
    }

    #[test]
    fn the_static_rungs_this_implementation_skips_are_declared_not_claimed() {
        // `--phase=typecheck` asks it to stop after a phase it never performs.
        for rung in [Phase::Typecheck, Phase::Mem, Phase::Wir] {
            let obs = observe(CLEAN.as_bytes(), Some(rung));
            assert_eq!(obs.verdict, Verdict::Unsupported, "{rung:?}");
            assert_eq!(obs.phase_reached, Phase::Resolve, "{rung:?}");
            assert!(obs.reason.is_some());
        }
    }

    #[test]
    fn a_construct_outside_coverage_reports_the_last_completed_phase() {
        let obs = observe(CONCURRENCY.as_bytes(), None);
        assert_eq!(obs.verdict, Verdict::Unsupported);
        assert_eq!(obs.phase_reached, Phase::Resolve);
        assert!(obs.reason.expect("a reason").contains("ic03"));
    }

    #[test]
    fn a_region_block_runs_at_is03_and_leaks_nothing() {
        // The rung moved: `region r { … }` was `unsupported` at is02 and is a
        // run outcome now, with the sprint's leak assertion clean.
        let obs = observe(REGIONS.as_bytes(), None);
        assert_eq!(obs.verdict, Verdict::Exit(0));
        assert_eq!(obs.phase_reached, Phase::Run);
        assert!(obs.leaks.is_empty(), "{:?}", obs.leaks);
        assert_eq!(obs.forest, Ok(()));
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

    #[test]
    fn a_trap_reports_the_run_rung_and_the_kind() {
        let obs = observe(b"fn main() -> int {\n    var d = 0\n    10 / d\n}\n", None);
        assert_eq!(obs.phase_reached, Phase::Run);
        assert_eq!(obs.verdict, Verdict::Trap(crate::trap::TrapKind::DivZero));
        assert_eq!(obs.trap.expect("a trap").rule.anchor(), "mem.ub.defined");
    }
}

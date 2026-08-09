//! Judging one observation against one corpus expectation.
//!
//! `[conf.directive.check]` is the corpus's claim about a file; a record is this
//! implementation's observation of it. Comparing them has exactly five honest
//! outcomes, and keeping them apart is the whole point — collapsing the middle
//! three into "failure" would make the sema boundary look like a bug, and
//! collapsing them into "success" would hide a real one.
//!
//! | verdict class | meaning |
//! |---|---|
//! | [`Judgement::Match`] | the expectation is a run outcome and this machine produced it |
//! | [`Judgement::DynamicCounterpart`] | the corpus pins a static code whose *dynamic meaning* spec/02 states, and this machine trapped with exactly that kind |
//! | [`Judgement::Conservatism`] | the corpus expects a **static** rejection this implementation does not perform, and the program ran |
//! | [`Judgement::OutOfScope`] | the file uses a construct outside is02; verdict `unsupported` |
//! | [`Judgement::Mismatch`] | everything else — a finding |
//!
//! The middle three are `[proto.record.unsupported]`'s conservatism ledger and
//! the sprint's "expected verdict class (static conservatism — ledgered in
//! is05, not a bug by construction)". A `Mismatch` is never explained away
//! here: `[proto.cmp.triage]` says the spec document is the defendant first,
//! and that triage is a human act, not a table lookup.

use std::fmt;

use crate::directive::{Check, ExitSpec};
use crate::phase::Phase;
use crate::protocol::{ObservationRecord, Verdict};

/// What one file's observation says about one file's expectation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Judgement {
    /// The corpus states a run outcome and this implementation produced it.
    Match(String),
    /// The corpus expects a static rejection whose **dynamic meaning** spec/02
    /// itself states, and this machine produced exactly that trap. Not
    /// conservatism: it is the two halves of one clause agreeing.
    DynamicCounterpart(String),
    /// The corpus expects a static rejection this implementation does not
    /// perform. Expected by construction; ledgered, never green-washed.
    Conservatism(String),
    /// The file is outside is02's coverage.
    OutOfScope(String),
    /// A finding.
    Mismatch(String),
}

impl Judgement {
    #[must_use]
    pub fn is_mismatch(&self) -> bool {
        matches!(self, Judgement::Mismatch(_))
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Judgement::Match(_) => "match",
            Judgement::DynamicCounterpart(_) => "dynamic-counterpart",
            Judgement::Conservatism(_) => "conservatism",
            Judgement::OutOfScope(_) => "out-of-scope",
            Judgement::Mismatch(_) => "MISMATCH",
        }
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        match self {
            Judgement::Match(d)
            | Judgement::DynamicCounterpart(d)
            | Judgement::Conservatism(d)
            | Judgement::OutOfScope(d)
            | Judgement::Mismatch(d) => d,
        }
    }
}

impl fmt::Display for Judgement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.label(), self.detail())
    }
}

/// Whether an expectation's stdout matches an observation's.
///
/// `corpus/hello.lu`: "`print` appends a newline; **stdout matching ignores the
/// trailing one**." That sentence is the corpus's own statement of the rule, so
/// it is implemented as written: compare with one trailing newline removed from
/// each side.
#[must_use]
pub fn stdout_matches(expected: &str, actual: &str) -> bool {
    fn trim(text: &str) -> &str {
        text.strip_suffix('\n').unwrap_or(text)
    }
    trim(expected) == trim(actual)
}

/// Judges one record against one `check:` directive.
///
/// `stdout` is the program's output, which the record only carries in
/// (truncated) inline form.
#[must_use]
pub fn judge(check: &Check, record: &ObservationRecord, stdout: &str) -> Judgement {
    let reason = record
        .extensions
        .get("x-unsupported")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("no reason given")
        .to_owned();

    match (check, &record.verdict) {
        // -- the file is outside coverage ---------------------------------
        (_, Verdict::Unsupported) => Judgement::OutOfScope(reason),

        // -- run expectations ---------------------------------------------
        (Check::Run { exit, stdout: want }, verdict) => {
            let termination = match (exit, verdict) {
                (ExitSpec::Code(want), Verdict::Exit(got)) if want == got => None,
                (ExitSpec::Code(want), got) => {
                    Some(format!("expected exit({want}), observed {got}"))
                }
                (ExitSpec::Trap(None), Verdict::Trap(_)) => None,
                (ExitSpec::Trap(Some(want)), Verdict::Trap(got)) if want == got => None,
                (ExitSpec::Trap(want), got) => Some(format!(
                    "expected {}, observed {got}",
                    ExitSpec::Trap(*want)
                )),
            };
            if let Some(problem) = termination {
                return Judgement::Mismatch(problem);
            }
            // `[proto.cmp.phase]`: at `run`, compare the verdict; for `exit`,
            // the status and the stdout digest.
            match want {
                Some(want) if !stdout_matches(want, stdout) => {
                    Judgement::Mismatch(format!("expected stdout {want:?}, observed {stdout:?}"))
                }
                _ => Judgement::Match(format!("{verdict}")),
            }
        }

        // -- rejection expectations ---------------------------------------
        (Check::Fail(want), Verdict::Fail(got)) if want == got => {
            Judgement::Match(format!("fail({got})"))
        }
        (Check::Fail(want), Verdict::Fail(got)) => {
            Judgement::Mismatch(format!("expected fail({want}), observed fail({got})"))
        }
        (Check::Fail(want), verdict) => {
            // A code this implementation does not produce, from a family it
            // does not implement, is the conservatism class — the sprint's
            // "a program the compiler rejects but the interpreter runs clean
            // is an *expected* verdict class". A *syntax*-tier code is not: the
            // frontend does implement those, so failing to produce one is a
            // finding.
            if is_syntax_code(want) {
                Judgement::Mismatch(format!("expected fail({want}), observed {verdict}"))
            } else if let (Some(expected), Verdict::Trap(got)) = (dynamic_meaning(want), verdict)
                && expected == *got
            {
                // `[conf.trap.map]`: "`use-after-move`/`exclusivity` (s04
                // dynamic meanings of E1001/E1002)". The compiler rejects the
                // program; this machine executes it and faults at the same
                // clause. The two halves agree.
                Judgement::DynamicCounterpart(format!(
                    "{want} statically ⇄ trap({got}) dynamically"
                ))
            } else {
                Judgement::Conservatism(format!(
                    "the corpus expects the static rejection {want}; this implementation does not \
                     perform that phase and observed {verdict}"
                ))
            }
        }

        // -- `pass` expectations -------------------------------------------
        (Check::Pass, Verdict::Pass) => Judgement::Match("pass".to_owned()),
        (Check::Pass, Verdict::Exit(status)) => {
            // A `pass` file is a well-formed program; the corpus states nothing
            // about its exit status, so running it is extra evidence, not a
            // contradiction.
            Judgement::Match(format!("exit({status}) beyond the corpus's `pass`"))
        }
        (Check::Pass, verdict) => {
            Judgement::Mismatch(format!("expected a clean program, observed {verdict}"))
        }
    }
}

/// The trap a static memory-tier code means dynamically, where spec/02 says so.
///
/// `[mem.tier0.move.2]`: "Use of an uninitialized or moved-from place is a
/// **compile error** (E1001) … Dynamic meaning (for the interpreter …): the
/// read traps with kind `use-after-move`." `[mem.tier0.excl.1]`: "Violations
/// are compile errors (E1002); the dynamic machine traps with kind
/// `exclusivity`." `[conf.trap.map]` collects both.
///
/// The table is only these two. Every other E1xxx is a static property with no
/// stated dynamic counterpart, and inventing one would be this implementation
/// legislating.
#[must_use]
pub fn dynamic_meaning(code: &str) -> Option<crate::trap::TrapKind> {
    match code {
        "E1001" => Some(crate::trap::TrapKind::UseAfterMove),
        "E1002" => Some(crate::trap::TrapKind::Exclusivity),
        _ => None,
    }
}

/// Is this a syntax-tier code — one the frontend is responsible for?
///
/// `[mem.codes]`: "`E000x` (spec-01 §9) + `E01xx` (lexer) + `E02xx` (parser)
/// are syntax-tier — the file fails to lex or parse. `E03xx` (resolution),
/// `E04xx` (types), `E1xxx` (memory), `E11xx` (concurrency), and `E12xx` (ABI)
/// are post-parse static rejections."
#[must_use]
pub fn is_syntax_code(code: &str) -> bool {
    let Some(digits) = code.strip_prefix('E') else {
        return false;
    };
    matches!(
        digits.parse::<u32>(),
        Ok(0..=9) | Ok(100..=199) | Ok(200..=299)
    )
}

/// The rung a file's own ledger claims (`[conf.directive.phase]`), for reports
/// that want to show what the *compiler* reaches beside what this does.
#[must_use]
pub fn ledger_phase(phase: Option<Phase>) -> String {
    phase.map_or_else(|| "-".to_owned(), |p| p.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trap::TrapKind;
    use std::collections::BTreeMap;

    fn record(verdict: Verdict) -> ObservationRecord {
        ObservationRecord {
            protocol: crate::protocol::PROTOCOL_VERSION,
            impl_name: crate::IMPL_NAME.to_owned(),
            impl_version: "0".to_owned(),
            commit: "c".to_owned(),
            file: "t.lu".to_owned(),
            phase_reached: Phase::Run,
            seeded: false,
            diagnostics: Vec::new(),
            verdict,
            stdout_sha256: None,
            stdout_inline: None,
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn the_trailing_newline_is_ignored_exactly_as_the_corpus_says() {
        assert!(stdout_matches("hello, wolf", "hello, wolf\n"));
        assert!(stdout_matches("hello, wolf\n", "hello, wolf"));
        assert!(stdout_matches("body first second", "body first second"));
        // Only ONE trailing newline, and nothing else, is forgiven.
        assert!(!stdout_matches("hello", "hello\n\n"));
        assert!(!stdout_matches("hello", " hello\n"));
        assert!(!stdout_matches("hello", "goodbye\n"));
    }

    #[test]
    fn a_matching_run_expectation_is_a_match() {
        let check = Check::Run {
            exit: ExitSpec::Code(0),
            stdout: Some("hello, wolf".to_owned()),
        };
        assert!(matches!(
            judge(&check, &record(Verdict::Exit(0)), "hello, wolf\n"),
            Judgement::Match(_)
        ));
    }

    #[test]
    fn the_wrong_status_or_the_wrong_output_is_a_finding() {
        let check = Check::Run {
            exit: ExitSpec::Code(0),
            stdout: Some("hello, wolf".to_owned()),
        };
        assert!(judge(&check, &record(Verdict::Exit(1)), "hello, wolf\n").is_mismatch());
        assert!(judge(&check, &record(Verdict::Exit(0)), "goodbye\n").is_mismatch());
    }

    #[test]
    fn trap_expectations_compare_the_kind_and_only_the_kind() {
        // `[conf.trap.exit]`: "conforming tools compare the *kind*, never the
        // status number."
        let exact = Check::Run {
            exit: ExitSpec::Trap(Some(TrapKind::Overflow)),
            stdout: None,
        };
        assert!(matches!(
            judge(&exact, &record(Verdict::Trap(TrapKind::Overflow)), ""),
            Judgement::Match(_)
        ));
        assert!(judge(&exact, &record(Verdict::Trap(TrapKind::DivZero)), "").is_mismatch());

        let any = Check::Run {
            exit: ExitSpec::Trap(None),
            stdout: None,
        };
        assert!(matches!(
            judge(&any, &record(Verdict::Trap(TrapKind::Bounds)), ""),
            Judgement::Match(_)
        ));
    }

    #[test]
    fn the_two_codes_with_a_stated_dynamic_meaning_are_counterparts_not_conservatism() {
        assert!(matches!(
            judge(
                &Check::Fail("E1001".to_owned()),
                &record(Verdict::Trap(TrapKind::UseAfterMove)),
                ""
            ),
            Judgement::DynamicCounterpart(_)
        ));
        assert!(matches!(
            judge(
                &Check::Fail("E1002".to_owned()),
                &record(Verdict::Trap(TrapKind::Exclusivity)),
                ""
            ),
            Judgement::DynamicCounterpart(_)
        ));
        // The wrong trap for the code is still a finding-shaped conservatism
        // entry, not a counterpart.
        assert!(matches!(
            judge(
                &Check::Fail("E1001".to_owned()),
                &record(Verdict::Trap(TrapKind::Bounds)),
                ""
            ),
            Judgement::Conservatism(_)
        ));
        // No other memory code has a stated dynamic meaning.
        assert_eq!(dynamic_meaning("E1003"), None);
        assert_eq!(dynamic_meaning("E1006"), None);
    }

    #[test]
    fn a_static_rejection_this_implementation_skips_is_conservatism() {
        for code in ["E0303", "E0402", "E1002", "E1102", "E1201"] {
            let check = Check::Fail(code.to_owned());
            assert!(
                matches!(
                    judge(&check, &record(Verdict::Exit(0)), ""),
                    Judgement::Conservatism(_)
                ),
                "{code}"
            );
        }
    }

    #[test]
    fn a_missed_syntax_tier_rejection_is_a_finding_not_conservatism() {
        // The frontend *does* implement E000x/E01xx/E02xx, so not producing one
        // is a real disagreement.
        for code in ["E0001", "E0006", "E0108", "E0207"] {
            let check = Check::Fail(code.to_owned());
            assert!(
                judge(&check, &record(Verdict::Exit(0)), "").is_mismatch(),
                "{code}"
            );
            assert!(is_syntax_code(code), "{code}");
        }
        for code in ["E0301", "E0405", "E1001", "E1101"] {
            assert!(!is_syntax_code(code), "{code}");
        }
    }

    #[test]
    fn unsupported_is_out_of_scope_whatever_the_expectation_was() {
        let mut r = record(Verdict::Unsupported);
        r.extensions.insert(
            "x-unsupported".to_owned(),
            serde_json::Value::from("concurrency is spec/03 and campaign ic03"),
        );
        for check in [
            Check::Pass,
            Check::Fail("E1002".to_owned()),
            Check::Run {
                exit: ExitSpec::Code(0),
                stdout: None,
            },
        ] {
            let judgement = judge(&check, &r, "");
            assert!(matches!(judgement, Judgement::OutOfScope(_)));
            assert_eq!(
                judgement.detail(),
                "concurrency is spec/03 and campaign ic03"
            );
        }
    }

    #[test]
    fn running_a_pass_file_further_than_the_corpus_claims_is_not_a_finding() {
        assert!(matches!(
            judge(&Check::Pass, &record(Verdict::Exit(3)), ""),
            Judgement::Match(_)
        ));
        assert!(judge(&Check::Pass, &record(Verdict::Fail("E0001".to_owned())), "").is_mismatch());
    }
}

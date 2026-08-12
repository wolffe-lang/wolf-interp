//! Record comparison, from `spec/06-differential-protocol.md` §3 `[proto.cmp]`.
//!
//! The two tracks are comparable **only** through the protocol, and the
//! protocol is a spec, not code — so this is the second implementation of the
//! comparison, written from the document, exactly like everything else here.
//!
//! Why it lives in is01 rather than waiting for a compiler to talk to: the
//! sprint's acceptance asks for the divergence-filing rule to be exercised end
//! to end, *seeded if no genuine divergence surfaced* — "prove the pipeline,
//! not the luck". A comparison that only runs when a real disagreement appears
//! is a comparison nobody has ever run. `tests/divergence.rs` seeds one.
//!
//! The triage rule that comes with it is normative and worth quoting, because
//! it is the reason this repo exists (`[proto.cmp.triage]`):
//!
//! > **the spec document is the defendant first** — an ambiguous clause is
//! > presumed the root cause until the clause is shown unambiguous; then the
//! > implementation that disagrees with it is the defendant.

use std::fmt;

use crate::phase::Phase;
use crate::protocol::{ObservationRecord, Verdict};

/// Divergence classes, descending in severity (`[proto.cmp.severity]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// One side reports `ub(…)` where the other runs defined
    /// (`[proto.record.ub]`) — the highest-severity class.
    SoundnessCandidate,
    VerdictMismatch,
    SpanOrCodeMismatch,
    StdoutMismatch,
}

impl Class {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Class::SoundnessCandidate => "soundness-candidate",
            Class::VerdictMismatch => "verdict",
            Class::SpanOrCodeMismatch => "span-or-code",
            Class::StdoutMismatch => "stdout",
        }
    }
}

impl fmt::Display for Class {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One divergence, in the shape `[proto.harness.differ]` puts on a JSONL line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    pub file: String,
    pub class: Class,
    /// What each side said, rendered for the report.
    pub a: String,
    pub b: String,
    /// Why the comparison fired — ours, never compared.
    pub detail: String,
}

impl Divergence {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "file": self.file,
            "class": self.class.as_str(),
            "a": self.a,
            "b": self.b,
            "x-detail": self.detail,
        })
    }
}

/// Compares two observation records of the same file at the same rung.
///
/// Returns `None` when the two agree, or when they differ only in ways
/// `[proto.cmp.defined-divergence]` says are never divergences.
#[must_use]
pub fn compare(a: &ObservationRecord, b: &ObservationRecord) -> Option<Divergence> {
    // `[proto.cmp.defined-divergence]`: `unsupported` on either side is never a
    // divergence. It is a scope gap, and scope gaps live in the conservatism
    // ledger, not the divergence report.
    if matches!(a.verdict, Verdict::Unsupported) || matches!(b.verdict, Verdict::Unsupported) {
        return None;
    }

    let file = a.file.clone();
    let render = |r: &ObservationRecord| format!("{}@{}", r.verdict, r.phase_reached);

    // `[proto.record.ub]`: one side reporting `ub(…)` where the other runs
    // defined is the highest-severity class, and it outranks the plain verdict
    // mismatch it would otherwise be reported as.
    let ub_a = matches!(a.verdict, Verdict::Ub(_));
    let ub_b = matches!(b.verdict, Verdict::Ub(_));
    if ub_a != ub_b {
        return Some(Divergence {
            file,
            class: Class::SoundnessCandidate,
            a: render(a),
            b: render(b),
            detail: "one side reports UB where the other runs defined".to_owned(),
        });
    }

    if a.phase_reached != b.phase_reached || !verdicts_agree(a, b) {
        return Some(Divergence {
            file,
            class: Class::VerdictMismatch,
            a: render(a),
            b: render(b),
            detail: "phase_reached or verdict differ".to_owned(),
        });
    }

    // At `lex|parse` (and every rung through `mem`), a `fail` compares the
    // **first** diagnostic's code *and* span. Later diagnostics are a
    // compiler-quality concern and are never compared — which this
    // implementation gets for free, having only ever produced one.
    if matches!(a.verdict, Verdict::Fail(_)) && a.phase_reached <= Phase::Mem {
        // The diagnostic that carries the `fail` is the first **error**:
        // `[proto.record.warn]` lets warning observations ride `diagnostics`
        // at warning severity, and `[proto.cmp.warn]` below owns those.
        fn first_error(r: &ObservationRecord) -> Option<&crate::protocol::Diagnostic> {
            r.diagnostics.iter().find(|d| d.severity != "warning")
        }
        let first_a = first_error(a);
        let first_b = first_error(b);
        match (first_a, first_b) {
            (Some(x), Some(y)) if x.code == y.code && x.span == y.span => {}
            _ => {
                return Some(Divergence {
                    file,
                    class: Class::SpanOrCodeMismatch,
                    a: format!("{first_a:?}"),
                    b: format!("{first_b:?}"),
                    detail: "the first diagnostic's code or span differs".to_owned(),
                });
            }
        }
    }

    // At `run`, `exit` compares status and stdout hash; `trap` compares kind
    // only. The verdict check above already covered status and kind.
    if matches!(a.verdict, Verdict::Exit(_)) && a.stdout_sha256 != b.stdout_sha256 {
        return Some(Divergence {
            file,
            class: Class::StdoutMismatch,
            a: format!("{:?}", a.stdout_sha256),
            b: format!("{:?}", b.stdout_sha256),
            detail: "stdout hashes differ".to_owned(),
        });
    }

    // `[proto.cmp.warn]` (s67): when BOTH records carry the `warnings` array,
    // the sorted `{code, span}` sets must agree. Absent on either side is
    // never a divergence — `[proto.record.warn]`'s honest-absent, so parity
    // grows lint-by-lint as each implementation builds its analyses.
    if let (Some(wa), Some(wb)) = (&a.warnings, &b.warnings) {
        let sorted = |w: &[crate::protocol::Warning]| {
            let mut v: Vec<(String, [u64; 2])> =
                w.iter().map(|x| (x.code.clone(), x.span)).collect();
            v.sort();
            v
        };
        let (sa, sb) = (sorted(wa), sorted(wb));
        if sa != sb {
            return Some(Divergence {
                file,
                class: Class::SpanOrCodeMismatch,
                a: format!("{sa:?}"),
                b: format!("{sb:?}"),
                detail: "both sides carry the warnings array and the sorted {code, span} \
                         sets differ ([proto.cmp.warn])"
                    .to_owned(),
            });
        }
    }

    None
}

/// `[proto.cmp.phase]` for the verdict itself, with `trap` compared by kind
/// only — which `Verdict::Trap` already is, since the vocabulary is closed and
/// the exit status is explicitly not compared (`[conf.trap.exit]`).
fn verdicts_agree(a: &ObservationRecord, b: &ObservationRecord) -> bool {
    a.verdict == b.verdict
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::protocol::{Diagnostic, PROTOCOL_VERSION};
    use crate::trap::TrapKind;

    fn record(impl_name: &str, phase: Phase, verdict: Verdict) -> ObservationRecord {
        ObservationRecord {
            protocol: PROTOCOL_VERSION,
            impl_name: impl_name.to_owned(),
            impl_version: "0.0.1".to_owned(),
            commit: "abc1234".to_owned(),
            file: "corpus/grammar/semicolon.lu".to_owned(),
            phase_reached: phase,
            seeded: false,
            diagnostics: Vec::new(),
            warnings: None,
            verdict,
            stdout_sha256: None,
            stdout_inline: None,
            extensions: BTreeMap::new(),
        }
    }

    fn with_diag(mut r: ObservationRecord, code: &str, span: [u64; 2]) -> ObservationRecord {
        r.diagnostics = vec![Diagnostic {
            code: code.to_owned(),
            span,
            severity: "error".to_owned(),
        }];
        r
    }

    #[test]
    fn agreement_is_silence() {
        let a = record("wolf-interp", Phase::Parse, Verdict::Pass);
        let b = record("wolfc", Phase::Parse, Verdict::Pass);
        assert_eq!(compare(&a, &b), None);
    }

    #[test]
    fn unsupported_on_either_side_is_never_a_divergence() {
        let ours = record("wolf-interp", Phase::Parse, Verdict::Unsupported);
        let theirs = record("wolfc", Phase::Run, Verdict::Exit(0));
        assert_eq!(compare(&ours, &theirs), None);
        assert_eq!(compare(&theirs, &ours), None);
    }

    #[test]
    fn a_differing_verdict_is_a_divergence() {
        let a = record("wolf-interp", Phase::Parse, Verdict::Pass);
        let b = record("wolfc", Phase::Parse, Verdict::Fail("E0002".to_owned()));
        assert_eq!(
            compare(&a, &b).map(|d| d.class),
            Some(Class::VerdictMismatch)
        );
    }

    #[test]
    fn the_same_code_at_a_different_span_is_still_a_divergence() {
        let a = with_diag(
            record(
                "wolf-interp",
                Phase::Parse,
                Verdict::Fail("E0002".to_owned()),
            ),
            "E0002",
            [222, 223],
        );
        let b = with_diag(
            record("wolfc", Phase::Parse, Verdict::Fail("E0002".to_owned())),
            "E0002",
            [222, 236],
        );
        assert_eq!(
            compare(&a, &b).map(|d| d.class),
            Some(Class::SpanOrCodeMismatch)
        );
    }

    #[test]
    fn ub_against_defined_outranks_the_verdict_class() {
        let a = record("wolf-interp", Phase::Run, Verdict::Ub("mem.ub".to_owned()));
        let b = record("wolfc", Phase::Run, Verdict::Exit(0));
        assert_eq!(
            compare(&a, &b).map(|d| d.class),
            Some(Class::SoundnessCandidate)
        );
        // And it is the most severe class there is.
        assert!(Class::SoundnessCandidate < Class::VerdictMismatch);
    }

    #[test]
    fn a_trap_compares_by_kind_and_not_by_status() {
        let a = record("wolf-interp", Phase::Run, Verdict::Trap(TrapKind::Bounds));
        let b = record("wolfc", Phase::Run, Verdict::Trap(TrapKind::Bounds));
        assert_eq!(compare(&a, &b), None);
    }

    #[test]
    fn warnings_absent_on_either_side_is_never_a_divergence() {
        use crate::protocol::Warning;
        let mut a = record("wolf-interp", Phase::Run, Verdict::Exit(0));
        let b = record("wolfc", Phase::Run, Verdict::Exit(0));
        a.warnings = Some(vec![Warning {
            code: "W1301".to_owned(),
            span: [356, 362],
        }]);
        // `[proto.record.warn]` honest-absent: b runs no warning analyses.
        assert_eq!(compare(&a, &b), None);
        assert_eq!(compare(&b, &a), None);
    }

    #[test]
    fn warnings_compare_as_sorted_sets_when_both_carry_the_array() {
        use crate::protocol::Warning;
        let w = |code: &str, span: [u64; 2]| Warning {
            code: code.to_owned(),
            span,
        };
        let mut a = record("wolf-interp", Phase::Run, Verdict::Exit(0));
        let mut b = record("wolfc", Phase::Run, Verdict::Exit(0));
        // Same set, different order: agreement.
        a.warnings = Some(vec![w("W1301", [10, 20]), w("W0302", [1, 5])]);
        b.warnings = Some(vec![w("W0302", [1, 5]), w("W1301", [10, 20])]);
        assert_eq!(compare(&a, &b), None);
        // A differing span in the set: span-or-code divergence.
        b.warnings = Some(vec![w("W0302", [1, 6]), w("W1301", [10, 20])]);
        assert_eq!(
            compare(&a, &b).map(|d| d.class),
            Some(Class::SpanOrCodeMismatch)
        );
        // An empty array against a non-empty one: both stand behind their
        // analyses, so the disagreement is real.
        b.warnings = Some(Vec::new());
        assert_eq!(
            compare(&a, &b).map(|d| d.class),
            Some(Class::SpanOrCodeMismatch)
        );
    }

    #[test]
    fn stdout_is_compared_only_for_exit_verdicts() {
        let mut a = record("wolf-interp", Phase::Run, Verdict::Exit(0));
        let mut b = record("wolfc", Phase::Run, Verdict::Exit(0));
        a.stdout_sha256 = Some("aaa".to_owned());
        b.stdout_sha256 = Some("bbb".to_owned());
        assert_eq!(
            compare(&a, &b).map(|d| d.class),
            Some(Class::StdoutMismatch)
        );
    }
}

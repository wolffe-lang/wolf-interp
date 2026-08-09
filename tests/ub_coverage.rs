//! The D2 coverage matrix — `[mem.ub]`'s closed enumeration, as a checked table.
//!
//! > s04's enumeration becomes a checked matrix: every item gets (a) detection
//! > logic in this machine … and (b) a corpus pair — one triggering program, one
//! > near-miss defined twin. […] Deferred items must be *listed* as deferred,
//! > never silently absent.
//!
//! The sprint spells the gate `cargo xtask ub-coverage`; this repo has no xtask
//! (its CI is plain `cargo`), so the gate is this file, and it fails the build
//! for the same three reasons: a row with no detection, a row with no pair, and
//! a row whose "deferred" claim carries no reason. [`audit`] is deliberately
//! callable on a hand-built table so a **planted** uncovered row can be shown to
//! fail without breaking the real one.
//!
//! # Where the programs live
//!
//! `tests/ub/<row>.lu` triggers and `tests/ub/ok/<row>_ok.lu` twins, in the
//! pinned corpus's own directive dialect so they are upstream-ready as-is — the
//! same arrangement, and the same reason, as `tests/faults/`. Two rows are
//! carried by the corpus itself: `memory/unsafe_ub_uaf.lu` is P1's own witness
//! and `memory/unsafe_noalias.lu` is P5's twin.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wolf_interp::directive::{Check, ExitSpec};
use wolf_interp::eval::prov::{Coverage, UbRow};
use wolf_interp::frontend;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn ub_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("ub")
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
}

fn programs_in(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "lu"))
        .map(|path| {
            (
                path.file_name()
                    .expect("a file name")
                    .to_string_lossy()
                    .into_owned(),
                std::fs::read_to_string(&path).expect("readable"),
            )
        })
        .collect();
    out.sort();
    out
}

fn triggers() -> Vec<(String, String)> {
    programs_in(&ub_dir())
}

fn twins() -> Vec<(String, String)> {
    programs_in(&ub_dir().join("ok"))
}

/// The row each trigger program actually reaches, observed rather than declared.
fn observed_rows() -> BTreeMap<UbRow, Vec<String>> {
    let mut out: BTreeMap<UbRow, Vec<String>> = BTreeMap::new();
    let mut record = |name: String, source: &str| {
        let observation = frontend::observe(source.as_bytes(), None);
        if let Some(finding) = observation.ub {
            out.entry(finding.row).or_default().push(name);
        }
    };
    for (name, source) in triggers() {
        record(name, &source);
    }
    // The corpus carries P1's own witness; nothing here re-authors it.
    for name in ["unsafe_ub_uaf.lu", "unsafe_noalias.lu"] {
        let path = corpus_dir().join("memory").join(name);
        let source = std::fs::read_to_string(&path).expect("the pinned corpus is readable");
        record(format!("corpus/memory/{name}"), &source);
    }
    out
}

/// Why a coverage matrix is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Gap {
    /// The row claims detection and no program reaches it.
    Undetected(&'static str),
    /// The row is detected and has no near-miss defined twin.
    Unpaired(&'static str),
    /// The row is not detected and says nothing about why.
    Unexplained(&'static str),
    /// `[mem.ub.closed]`'s invariant: zero rows without a named optimization.
    Unlicensed(&'static str),
}

/// Audits a coverage matrix against the programs that exist.
///
/// `witnesses` maps a row to the programs observed reaching it, and `paired`
/// holds the rows that have a near-miss twin. Both are inputs rather than
/// globals precisely so a planted table can be audited.
fn audit(rows: &[UbRow], witnesses: &BTreeMap<UbRow, Vec<String>>, paired: &[UbRow]) -> Vec<Gap> {
    let mut gaps = Vec::new();
    for row in rows {
        if row.optimization().is_empty() {
            gaps.push(Gap::Unlicensed(row.id()));
        }
        match row.coverage() {
            Coverage::Detected => {
                if !witnesses.get(row).is_some_and(|found| !found.is_empty()) {
                    gaps.push(Gap::Undetected(row.id()));
                }
                if !paired.contains(row) {
                    gaps.push(Gap::Unpaired(row.id()));
                }
            }
            Coverage::DeferredConcurrency => {
                if witnesses.contains_key(row) {
                    gaps.push(Gap::Undetected(row.id()));
                }
            }
            Coverage::Unreachable(reason) => {
                if reason.trim().len() < 20 {
                    gaps.push(Gap::Unexplained(row.id()));
                }
                if witnesses.contains_key(row) {
                    // Claiming a row is unreachable while reaching it is the
                    // same lie as an inflated `phase_reached`.
                    gaps.push(Gap::Unexplained(row.id()));
                }
            }
        }
    }
    gaps
}

/// The rows that have a near-miss twin, read off the twin file names.
fn paired_rows() -> Vec<UbRow> {
    let names: Vec<String> = twins().into_iter().map(|(name, _)| name).collect();
    UbRow::ALL
        .into_iter()
        .filter(|row| {
            let prefix = row.id().to_lowercase();
            names.iter().any(|name| name.starts_with(&prefix))
        })
        .collect()
}

#[test]
fn every_enumeration_row_is_detected_and_paired_or_explicitly_deferred() {
    let witnesses = observed_rows();
    let paired = paired_rows();
    let gaps = audit(&UbRow::ALL, &witnesses, &paired);
    assert!(
        gaps.is_empty(),
        "the `[mem.ub]` coverage matrix has holes. Every row is either \
         detectable-and-tested or explicitly deferred with a reason — an absence is \
         never allowed to look like a pass:\n{gaps:#?}\nwitnesses: {witnesses:#?}\npaired: {paired:?}"
    );
}

#[test]
fn a_planted_uncovered_row_fails_the_gate() {
    // The sprint's acceptance criterion, made executable: "a planted uncovered
    // item fails CI". P1 is detected and paired for real; audit it against an
    // empty world and the gate must refuse.
    let gaps = audit(&[UbRow::P1], &BTreeMap::new(), &[]);
    assert_eq!(gaps, vec![Gap::Undetected("P1"), Gap::Unpaired("P1")]);

    // A row with a witness but no twin is *also* a hole: a machine that reports
    // UB on everything satisfies "the trigger triggers".
    let witnesses = BTreeMap::from([(UbRow::P1, vec!["planted.lu".to_owned()])]);
    assert_eq!(
        audit(&[UbRow::P1], &witnesses, &[]),
        vec![Gap::Unpaired("P1")]
    );
}

#[test]
fn the_deferred_and_unreachable_rows_are_listed_with_their_reasons() {
    // "Deferred items must be *listed* as deferred, never silently absent."
    let deferred: Vec<&str> = UbRow::ALL
        .into_iter()
        .filter(|row| row.coverage() != Coverage::Detected)
        .map(UbRow::id)
        .collect();
    assert_eq!(
        deferred,
        vec!["T2", "C1"],
        "the set of rows this tier cannot reach moved; say why in `UbRow::coverage`"
    );

    // C1 is the sprint's `deferred(concurrency)` mark: spec/03's model, ic03's
    // machine, and the row's own wording says the pairing lives in spec/03.
    assert_eq!(UbRow::C1.coverage(), Coverage::DeferredConcurrency);
    assert!(UbRow::C1.optimization().contains("spec/03"));

    // T2 is unreachable at *this tier* rather than deferred to another sprint,
    // and the difference is worth keeping: a torn write needs a second observer.
    let Coverage::Unreachable(reason) = UbRow::T2.coverage() else {
        panic!("T2 is unreachable at this tier, not deferred to a campaign")
    };
    assert!(reason.contains("single-threaded"), "{reason}");
    assert!(reason.contains("ic03"), "{reason}");
}

#[test]
fn every_row_names_the_optimization_it_licenses() {
    // `[mem.ub.closed]`, verbatim: "zero rows without a named optimization is an
    // invariant of this table". D2 says the pairing is the *reason* the rule
    // exists, so a row that licenses nothing is a rule this language does not
    // have — and the report prints it, so a reader of a UB verdict learns what
    // the language bought with the rule that stopped them.
    for row in UbRow::ALL {
        assert!(!row.what().is_empty(), "{row}");
        assert!(row.optimization().len() > 20, "{row}");
        assert!(
            wolf_interp::anchor::classify(row.clause()).is_ok(),
            "{row} cites `{}`, which is not a well-formed anchor",
            row.clause()
        );
        assert!(!row.rule().description().is_empty(), "{row}");
    }
}

#[test]
fn every_trigger_program_yields_its_row_and_every_twin_runs_clean() {
    // The acceptance criterion: "Every trigger program yields `ub(anchor)` with
    // the right row id; every near-miss twin runs clean."
    for (name, source) in triggers() {
        let observation = frontend::observe(source.as_bytes(), None);
        assert_eq!(
            observation.verdict,
            Verdict::Ub("mem.ub".to_owned()),
            "{name}: {:?}",
            observation.reason
        );
        // The row id is in the file name, so the matrix cannot drift from the
        // programs without one of them being renamed.
        let finding = observation.ub.expect("a ub verdict carries its finding");
        let expected = name.split('_').next().expect("a name").to_ascii_uppercase();
        assert_eq!(finding.row.id(), expected, "{name}");
        // Two spans and a tree: the report the sprint specifies. §7/T1 is the
        // one row with no allocation to name — it is about the representation
        // of a *value*, not about a tag — so it carries the access span alone,
        // and says so rather than inventing a tree.
        assert!(finding.span.start < source.len(), "{name}");
        if finding.row == UbRow::T1 {
            assert!(finding.tag_span.is_none(), "{name}");
            assert!(finding.tree.is_empty(), "{name}");
        } else {
            assert!(finding.tag_span.is_some(), "{name}: no tag-creation span");
            assert!(finding.tree.len() >= 2, "{name}: no borrow-tree slice");
        }
        assert_eq!(observation.phase_reached, Phase::Run, "{name}");
    }

    for (name, source) in twins() {
        let observation = frontend::observe(source.as_bytes(), None);
        assert_eq!(
            observation.verdict,
            Verdict::Exit(0),
            "{name}: {:?}",
            observation.reason
        );
    }
}

#[test]
fn every_program_here_pins_its_own_expectation() {
    // The programs are written in the corpus's directive dialect so they are
    // upstream-ready; that dialect is also what makes them self-checking.
    for (name, source) in triggers() {
        let directives =
            wolf_interp::directive::parse_header(&source).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            directives.check,
            Some(Check::Run {
                exit: ExitSpec::Trap(Some(TrapKind::Ub)),
                stdout: None,
            }),
            "{name}: a UB trigger pins `run(exit=trap(ub))`"
        );
        assert!(
            !directives.conforms.is_empty(),
            "{name}: a litmus cites the clauses it is about"
        );
        for tag in &directives.conforms {
            assert!(
                wolf_interp::anchor::classify(tag).is_ok(),
                "{name}: `{tag}`"
            );
        }
    }
    for (name, source) in twins() {
        let directives =
            wolf_interp::directive::parse_header(&source).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            directives.check,
            Some(Check::Run {
                exit: ExitSpec::Code(0),
                stdout: None,
            }),
            "{name}: a near-miss twin pins `run(exit=0)`"
        );
    }
}

#[test]
fn the_trap_kind_and_the_oracle_verdict_stay_distinct() {
    // The sprint's own finding, held: "`ub` is also the eleventh trap *kind* in
    // spec/05 `[conf.trap.set]`; keep the verdict (`ub(anchor)`, provenance
    // oracle) and the trap kind (`trap(ub)`, defined runtime detection) distinct
    // in the protocol layer."
    //
    // So: this machine emits the **verdict** and never the kind, while the
    // directive dialect accepts the kind — because a checked build, which this
    // is not, terminates with it (`[conf.trap.map]`).
    for (name, source) in triggers() {
        let observation = frontend::observe(source.as_bytes(), None);
        assert!(
            observation.trap.is_none(),
            "{name}: UB is not a trap; a trap is a fault of a *defined* execution"
        );
        assert!(matches!(observation.verdict, Verdict::Ub(_)), "{name}");
    }
    assert!(wolf_interp::ledger::ub_is_the_oracle_verdict(Some(
        TrapKind::Ub
    )));
}

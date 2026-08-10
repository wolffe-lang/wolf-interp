//! The explorer over the pinned corpus — is07's determinism-taxonomy proof.
//!
//! spec/03 §5 (`sched-ev/0`) and spec/06 `[proto.seed]` promise that a
//! conforming program's observable behavior is a function of its schedule
//! *seed alone* — and a safe-tier program with no deliberate order dependence
//! must be observably identical on **every** schedule (the DRF-SC claim, D14).
//! This suite runs the explorer over the whole `conc/` litmus tier as proof:
//! any file that is not schedule-independent would fail here as a
//! top-severity finding, named rather than absorbed.
//!
//! Everything below is bounded exhaustive: the budgets close the frontier on
//! every file (asserted), so "no divergent schedule was found" means "none
//! exists", not "we stopped looking".

use std::path::{Path, PathBuf};

use wolf_interp::explore::{self, Options};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
}

fn explore_file(relative: &str, options: &Options) -> explore::Report {
    let path = corpus_root().join(relative);
    wolf_interp::explore_file(&path, options)
        .unwrap_or_else(|e| panic!("{relative}: cannot explore: {e}"))
}

fn dpor() -> Options {
    Options {
        max_schedules: 20_000,
        ..Options::default()
    }
}

/// Every file of the `conc/` tier, with the schedule-space size the explorer
/// establishes for it (DPOR classes / naive DFS schedules) and the one
/// verdict all of them produce. This ledger is the sprint's exploration
/// record: a count moving is a machine or corpus change worth a commit
/// message; a file leaving green is a finding.
const CONC_LEDGER: &[(&str, u64, u64, &str)] = &[
    ("conc/cancel_sibling.lu", 2, 2, "exit(0)"),
    ("conc/chan_unsendable.lu", 1, 1, "exit(0)"),
    ("conc/freeze_publish.lu", 2, 2, "exit(0)"),
    ("conc/message_passing.lu", 1, 1, "exit(0)"),
    // Self-contained since the s20 S-batch (S-5 resolved): it RUNS, and the
    // kill-skips-defers outcome is schedule-independent — only the owner's
    // `released` prints, on every schedule.
    ("conc/proc_kill_defers.lu", 1, 1, "exit(0)"),
    ("conc/select_seeded.lu", 2, 2, "exit(0)"),
    // Two tasks writing their own captured copies: the orders commute, so
    // DPOR proves one class where naive DFS walks both orders.
    ("conc/store_buffer.lu", 1, 2, "exit(0)"),
    ("conc/when_multi.lu", 2, 2, "exit(0)"),
];

#[test]
fn every_conc_litmus_is_schedule_independent_within_a_closed_frontier() {
    let mut findings = Vec::new();
    for (relative, dpor_schedules, naive_schedules, verdict) in CONC_LEDGER {
        let report = explore_file(relative, &dpor());
        assert!(
            !report.frontier_open,
            "{relative}: the budget must close the frontier for the proof to be one"
        );
        assert_eq!(
            report.schedules, *dpor_schedules,
            "{relative}: the explored class count moved"
        );
        if !report.green() {
            findings.push(format!(
                "  {relative}: {} distinct outcome(s), divergence={:?}, oracle failures={:?}",
                report.outcomes.len(),
                report.divergence,
                report.oracle_failures
            ));
            continue;
        }
        assert_eq!(&report.outcomes[0].verdict, verdict, "{relative}");
        assert_eq!(
            report.outcomes[0].leaks, 0,
            "{relative}: leaked on some schedule"
        );
        assert!(report.outcomes[0].forest_ok, "{relative}");

        // The naive DFS baseline agrees on every conclusion — identical
        // bug-finding, only the schedule count may differ.
        let baseline = explore_file(
            relative,
            &Options {
                naive: true,
                ..dpor()
            },
        );
        assert!(!baseline.frontier_open, "{relative} (naive)");
        assert_eq!(baseline.schedules, *naive_schedules, "{relative} (naive)");
        assert!(baseline.schedules >= report.schedules, "{relative}");
        let a: Vec<u128> = report.outcomes.iter().map(|o| o.digest).collect();
        let b: Vec<u128> = baseline.outcomes.iter().map(|o| o.digest).collect();
        assert_eq!(
            a, b,
            "{relative}: DPOR and naive DFS must find the same outcomes"
        );
    }
    assert!(
        findings.is_empty(),
        "SCHEDULE-DEPENDENT corpus files — a top-severity finding, triage per \
         [proto.cmp.triage] (the spec document is the defendant first):\n{}",
        findings.join("\n")
    );
}

#[test]
fn the_flagged_multiopen_litmuses_hold_their_invariant_over_their_whole_space() {
    // `memory/region_multiopen_ok.lu` is FLAGGED FOR is07 MODEL CHECKING in
    // its own header. Single-task files have exactly one schedule — the
    // model check is trivial and the answer definitive: the forest invariant
    // holds over the entire (unit) space. The concurrent multiopen spaces
    // live in `tests/explore_machine.rs::no_schedule_breaks_the_multiopen_forest_invariant`.
    for relative in [
        "memory/region_multiopen_ok.lu",
        "memory/region_multiopen_swap.lu",
    ] {
        let report = explore_file(relative, &dpor());
        assert!(report.green(), "{relative}: {report:?}");
        assert_eq!(report.schedules, 1, "{relative}: single-task, unit space");
        assert!(!report.frontier_open, "{relative}");
        assert_eq!(report.outcomes[0].verdict, "exit(0)", "{relative}");
        assert!(report.outcomes[0].forest_ok, "{relative}");
        assert_eq!(report.outcomes[0].leaks, 0, "{relative}");
    }
}

#[test]
fn the_wide_seed_corpus_stays_deterministic_under_exploration() {
    // The two flagship programs outside the conc tier that exercise the
    // scheduler (wordcount's task tier is sequentialized; hello is the
    // control): exploring them is cheap and pins "sequential programs have
    // exactly one schedule".
    for (relative, verdict) in [("hello.lu", "exit(0)"), ("wordcount.lu", "exit(2)")] {
        let report = explore_file(relative, &dpor());
        assert!(report.green(), "{relative}: {report:?}");
        assert_eq!(report.schedules, 1, "{relative}");
        assert_eq!(&report.outcomes[0].verdict, verdict, "{relative}");
    }
}

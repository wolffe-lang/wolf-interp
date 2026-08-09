//! The `run` rung against the pinned corpus — is02's acceptance test.
//!
//! Nothing here hardcodes an outcome except the **first-run ledger**, which is
//! deliberately a written list: the sprint asks for "the exact set of files that
//! reach `run` with their outcomes", and a set that is only ever computed cannot
//! regress visibly. Everything else is driven by the corpus's own directives.
//!
//! The load-bearing assertion is `no_corpus_file_mismatches_its_expectation`:
//! `[proto.cmp.triage]` makes a disagreement a *finding*, triaged by a human
//! with the spec as defendant — never silently absorbed by moving a number here
//! or, worse, by editing the corpus.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wolf_interp::corpus::{self, Outcome};
use wolf_interp::directive::{Check, Directives, ExitSpec};
use wolf_interp::ledger::{self, Judgement};
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
}

struct Entry {
    path: String,
    directives: Directives,
    verdict: Verdict,
    phase: Phase,
    stdout: String,
    judgement: Judgement,
}

fn entries() -> Vec<Entry> {
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("the pinned corpus is walkable");
    report
        .files
        .into_iter()
        .filter_map(|file| {
            let Outcome::Entry(directives) = file.outcome else {
                return None;
            };
            let full = root.join(&file.path);
            let source = std::fs::read(&full).expect("readable");
            let (record, _) = wolf_interp::observe_record(&full, &source, None);
            let stdout = record.stdout_inline.clone().unwrap_or_default();
            let judgement = ledger::judge(
                directives.check.as_ref().expect("an entry pins a check"),
                &record,
                &stdout,
            );
            Some(Entry {
                path: file.path,
                directives: *directives,
                verdict: record.verdict,
                phase: record.phase_reached,
                stdout,
                judgement,
            })
        })
        .collect()
}

/// The files this implementation evaluates end to end, with what they produce.
///
/// **This list is the sprint's first-run ledger.** Adding to it is progress and
/// belongs in a commit message; losing an entry is a regression. A file leaves
/// this list only when the corpus drops it.
const FIRST_RUN_LEDGER: &[(&str, &str)] = &[
    // The seed corpus.
    ("hello.lu", "exit(0)"),
    ("overflow.lu", "trap(overflow)"),
    ("wordcount.lu", "exit(2)"),
    // Tier-0 memory litmuses.
    ("memory/defer_order.lu", "exit(0)"),
    ("memory/div_zero.lu", "trap(div-zero)"),
    ("memory/excl_disjoint_ok.lu", "exit(0)"),
    ("memory/excl_overlap.lu", "trap(exclusivity)"),
    ("memory/move_ok.lu", "exit(0)"),
    ("memory/move_use_after.lu", "trap(use-after-move)"),
    ("memory/oob_bounds.lu", "trap(bounds)"),
    ("memory/prov_holy_grail.lu", "exit(0)"),
    ("memory/prov_two_phase.lu", "exit(0)"),
    ("memory/shared_cycle.lu", "exit(0)"),
    // Tier-1/2 litmuses, newly reaching `run` at is03 (the dynamic region
    // machine). Every one matches its `check:` exactly.
    ("memory/handle_stale.lu", "trap(stale-handle)"),
    ("memory/region_ambient_ok.lu", "exit(0)"),
    ("memory/region_iso_edge_ok.lu", "exit(0)"),
    ("memory/region_multiopen_ok.lu", "exit(0)"),
    ("memory/region_multiopen_swap.lu", "exit(0)"),
    ("memory/shared_ok.lu", "exit(0)"),
    // The `rows/` tier, newly present at pin bd41920. Nothing in the evaluator
    // moved for these: D30's error rows are values, and is02's machine already
    // ran them — they arrived with the pin, not with the sprint.
    ("rows/coarsen.lu", "exit(0)"),
    ("rows/hof_tail.lu", "exit(0)"),
    ("rows/inferred_private.lu", "exit(0)"),
    ("rows/negative/dup_tags.lu", "exit(7)"),
    ("rows/negative/errdefer_infallible.lu", "exit(1)"),
    ("rows/negative/missing_tag.lu", "exit(0)"),
    ("rows/negative/payload_mismatch.lu", "exit(0)"),
    ("rows/negative/pub_inferred.lu", "exit(1)"),
    ("rows/propagate/main.lu", "exit(0)"),
    // The grammar annex's accepted readings, now executed rather than parsed.
    ("grammar/bang_errunion.lu", "exit(0)"),
    ("grammar/bang_not.lu", "exit(0)"),
    ("grammar/brackets_generic_call.lu", "exit(0)"),
    ("grammar/brackets_index.lu", "exit(0)"),
    ("grammar/else_chain.lu", "exit(0)"),
    ("grammar/else_default.lu", "exit(0)"),
    ("grammar/intdot_range.lu", "exit(0)"),
    ("grammar/interp_fmtcolon.lu", "exit(0)"),
    ("grammar/interp_nested.lu", "exit(0)"),
    ("grammar/newline_trailing.lu", "exit(0)"),
    ("grammar/structlit_paren.lu", "exit(0)"),
    // D32's module graph.
    ("resolve/cycle/main.lu", "exit(1)"),
    ("resolve/forward/main.lu", "exit(0)"),
    ("resolve/pkgvis/main.lu", "exit(0)"),
    ("resolve/two_mod/main.lu", "exit(0)"),
    ("resolve/unused/main.lu", "exit(0)"),
    // Programs the type checker rejects and this machine runs anyway.
    ("typecheck/ambiguous.lu", "exit(0)"),
    ("typecheck/if_branch.lu", "exit(0)"),
    // Trait-tier programs whose `main` is Tier-0.
    ("traits/coherence_orphan/main.lu", "exit(0)"),
    ("traits/coherence_overlap.lu", "exit(0)"),
    ("traits/coherence_uncovered.lu", "exit(0)"),
    ("traits/dyn_assoc_escape.lu", "exit(0)"),
    ("traits/dyn_generic_method.lu", "exit(0)"),
    ("traits/dyn_ok.lu", "exit(0)"),
    ("traits/dyn_self_position.lu", "exit(0)"),
    ("traits/golden_arith.lu", "exit(0)"),
    ("traits/golden_eq.lu", "exit(0)"),
    ("traits/golden_missing_bound.lu", "exit(0)"),
];

#[test]
fn no_corpus_file_mismatches_its_expectation() {
    let mismatches: Vec<String> = entries()
        .iter()
        .filter(|entry| entry.judgement.is_mismatch())
        .map(|entry| format!("  {}: {}", entry.path, entry.judgement))
        .collect();
    assert!(
        mismatches.is_empty(),
        "the corpus and this implementation disagree. `[proto.cmp.triage]`: the spec document \
         is the defendant first — triage each of these, do NOT edit the corpus:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn the_first_run_ledger_is_exactly_what_reaches_run() {
    let observed: BTreeMap<String, String> = entries()
        .into_iter()
        .filter(|entry| entry.phase == Phase::Run)
        .map(|entry| (entry.path, entry.verdict.to_string()))
        .collect();
    let expected: BTreeMap<String, String> = FIRST_RUN_LEDGER
        .iter()
        .map(|(path, verdict)| ((*path).to_owned(), (*verdict).to_owned()))
        .collect();

    let gained: Vec<&String> = observed
        .keys()
        .filter(|k| !expected.contains_key(*k))
        .collect();
    let lost: Vec<&String> = expected
        .keys()
        .filter(|k| !observed.contains_key(*k))
        .collect();
    assert!(
        gained.is_empty() && lost.is_empty(),
        "the set of files reaching `run` moved.\n  newly running (progress — add them): {gained:?}\
         \n  no longer running (regression): {lost:?}"
    );
    assert_eq!(observed, expected, "a file's run outcome changed");
}

#[test]
fn the_four_seed_files_are_accounted_for() {
    // The sprint's corpus-coverage target names `hello.lu`, `errors.lu`,
    // `overflow.lu`, `strings.lu`. Two run; two do not, and the reason is
    // recorded here rather than left as an absence.
    let by_path: BTreeMap<String, Entry> = entries()
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();

    let hello = &by_path["hello.lu"];
    assert_eq!(hello.verdict, Verdict::Exit(0));
    assert_eq!(hello.stdout, "hello, wolf\n");
    assert!(matches!(hello.judgement, Judgement::Match(_)));

    let overflow = &by_path["overflow.lu"];
    assert_eq!(overflow.verdict, Verdict::Trap(TrapKind::Overflow));

    // `errors.lu` and `strings.lu` need std surface that no pinned document
    // specifies (`acquire`/`release`/`Resource.text`; `re"…"` literals and
    // `^n` from-end indexing). Declining is the honest answer — see the
    // `builtin` module's two rules — and the reason travels on `x-unsupported`.
    for name in ["errors.lu", "strings.lu"] {
        let entry = &by_path[name];
        assert_eq!(entry.verdict, Verdict::Unsupported, "{name}");
        assert_eq!(entry.phase, Phase::Resolve, "{name}");
        assert!(
            matches!(entry.judgement, Judgement::OutOfScope(_)),
            "{name}"
        );
    }
}

#[test]
fn every_run_expectation_this_machine_reaches_is_met_exactly() {
    // The narrow claim, stated separately from the broad one: for every corpus
    // file whose `check:` is a *run* expectation and which this machine
    // evaluated, the termination and the output agree.
    let mut checked = 0usize;
    for entry in entries() {
        let Some(Check::Run { exit, stdout }) = &entry.directives.check else {
            continue;
        };
        if entry.phase != Phase::Run {
            continue;
        }
        checked += 1;
        match (exit, &entry.verdict) {
            (ExitSpec::Code(want), Verdict::Exit(got)) => {
                assert_eq!(want, got, "{}", entry.path);
            }
            (ExitSpec::Trap(Some(want)), Verdict::Trap(got)) => {
                // `[conf.trap.exit]`: compare the kind, never a status number.
                assert_eq!(want, got, "{}", entry.path);
            }
            (ExitSpec::Trap(None), Verdict::Trap(_)) => {}
            (want, got) => panic!("{}: expected {want}, observed {got}", entry.path),
        }
        if let Some(want) = stdout {
            assert!(
                ledger::stdout_matches(want, &entry.stdout),
                "{}: expected stdout {want:?}, observed {:?}",
                entry.path,
                entry.stdout
            );
        }
    }
    assert!(
        checked >= 11,
        "only {checked} run expectations were exercised"
    );
}

#[test]
fn phase_reached_is_never_inflated() {
    // `[proto.record.phase]`, the s13 convention: `phase_reached` names the
    // deepest phase that COMPLETED. An `unsupported` verdict must therefore
    // never claim `run`, and a `fail` must name the rung that failed.
    for entry in entries() {
        match &entry.verdict {
            Verdict::Unsupported => assert!(
                entry.phase < Phase::Run,
                "{}: unsupported at {}",
                entry.path,
                entry.phase
            ),
            Verdict::Fail(_) => assert!(
                entry.phase <= Phase::Parse,
                "{}: this implementation only fails at lex/parse, not {}",
                entry.path,
                entry.phase
            ),
            Verdict::Exit(_) | Verdict::Trap(_) => {
                assert_eq!(entry.phase, Phase::Run, "{}", entry.path);
            }
            Verdict::Pass | Verdict::Ub(_) => {
                panic!("{}: unexpected verdict {}", entry.path, entry.verdict)
            }
        }
    }
}

#[test]
fn every_unsupported_file_says_why() {
    // "the protocol verdict carries no payload — `[proto.record.verdict]`; the
    // reason rides an `x-` extension key". A reasonless `unsupported` is a hole
    // in the conservatism ledger.
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("walkable");
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let (record, _) = wolf_interp::observe_record(&full, &source, None);
        if record.verdict != Verdict::Unsupported {
            continue;
        }
        let reason = record
            .extensions
            .get("x-unsupported")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        assert!(
            reason.len() > 10,
            "{}: `unsupported` with no useful reason ({reason:?})",
            file.path
        );
    }
}

#[test]
fn every_record_is_schema_valid_including_the_running_ones() {
    // A record with stdout has to carry both the digest and the inline text
    // (`[proto.record.fields]`), and this implementation must never emit one
    // its own validator would reject.
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("walkable");
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let (record, _) = wolf_interp::observe_record(&full, &source, None);
        let value = serde_json::to_value(&record).expect("serializes");
        assert_eq!(
            wolf_interp::schema::validate(&value),
            Ok(()),
            "{}",
            file.path
        );
        if let Verdict::Exit(_) = record.verdict
            && record.stdout_inline.as_ref().is_some_and(|s| !s.is_empty())
        {
            let digest = record.stdout_sha256.as_ref().expect("a digest");
            assert_eq!(digest.len(), 64, "{}", file.path);
        }
    }
}

#[test]
fn the_stdout_digest_is_the_digest_of_the_stdout() {
    // The digest is a comparison surface: two implementations agree on output
    // by agreeing on this string, so it had better be computed from the bytes.
    let root = corpus_root();
    let path = root.join("hello.lu");
    let source = std::fs::read(&path).expect("readable");
    let (record, _) = wolf_interp::observe_record(&path, &source, None);
    assert_eq!(record.stdout_inline.as_deref(), Some("hello, wolf\n"));
    assert_eq!(
        record.stdout_sha256.as_deref(),
        Some(wolf_interp::sha256::hex(b"hello, wolf\n").as_str())
    );
}

#[test]
fn evaluation_is_deterministic_over_the_corpus() {
    // Two runs, same verdict, same bytes. Cheap, and it catches the whole class
    // of bugs where a result depends on hash iteration or allocation order —
    // which would make every differential comparison meaningless.
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("walkable");
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let (first, _) = wolf_interp::observe_record(&full, &source, None);
        let (second, _) = wolf_interp::observe_record(&full, &source, None);
        assert_eq!(first, second, "{} observed differently twice", file.path);
    }
}

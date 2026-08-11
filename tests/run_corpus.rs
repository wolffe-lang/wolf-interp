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
/// **This list is the sprint's run ledger.** Adding to it is progress and
/// belongs in a commit message; losing an entry is a regression. A file leaves
/// this list only when the corpus drops it.
const RUN_LEDGER: &[(&str, &str)] = &[
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
    // D32's module graph. `cycle` and `unused` left this ledger at is06: the
    // resolve rung enforces the module laws now (E0303/E0305, closing
    // DIV-2026-002 and DIV-2026-005), so both fail at `resolve` exactly as
    // the corpus pins instead of running.
    ("resolve/forward/main.lu", "exit(0)"),
    ("resolve/pkgvis/main.lu", "exit(0)"),
    ("resolve/two_mod/main.lu", "exit(0)"),
    // Programs the type checker rejects and this machine runs anyway.
    ("typecheck/ambiguous.lu", "exit(0)"),
    ("typecheck/if_branch.lu", "exit(0)"),
    // Runs since pin 67c977f put the grammar's required commas into the file
    // (the upstream DIV-2026-001 repair).
    ("typecheck/match_exhaustive.lu", "exit(0)"),
    // -- is06: the sim scheduler (spec/03) ----------------------------------
    // The conc litmus tier runs. `freeze_publish` and `when_multi` produced
    // exit(1) against the corpus's exit=0 through pin `67c977f`; pin `79ceec6`
    // paid both debts (DIV-2026-008: the file reports through a channel now;
    // DIV-2026-009: the expected total is 223) and both run exit(0).
    // `chan_unsendable` and `store_buffer` run clean where the compiler
    // rejects statically (E1102/E1101 conservatism).
    ("conc/cancel_sibling.lu", "exit(0)"),
    ("conc/chan_unsendable.lu", "exit(0)"),
    ("conc/freeze_publish.lu", "exit(0)"),
    ("conc/message_passing.lu", "exit(0)"),
    ("conc/select_seeded.lu", "exit(0)"),
    ("conc/store_buffer.lu", "exit(0)"),
    ("conc/when_multi.lu", "exit(0)"),
    // `channel` exists now, so the E1005 dynamic-counterpart case finally
    // RUNS: the open region cannot be transferred, trap(region-fault) citing
    // the clause — [conf.trap.map]'s E1005 → region-fault row, exercised.
    ("memory/region_move_while_open.lu", "trap(region-fault)"),
    // A local borrow sent into a channel: E1003 is the *static* borrow
    // checker's; under MVS the borrow is a value and the send copies it, so
    // the program runs clean — the standing conservatism class.
    ("memory/borrow_escape.lu", "exit(0)"),
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
    // -- is04 -------------------------------------------------------------
    // The unsafe litmuses. `unsafe_noalias.lu` runs *defined*: the assertion is
    // true, so `[mem.unsafe.raw.2]` is discharged and O5's `noalias` treatment
    // is licensed. `unsafe_ub_uaf.lu` is the oracle's first verdict — §7/P1
    // through a freed C allocation, satisfying the file's `run(exit=trap(ub))`
    // per `[conf.trap.map]`'s `ub` kind (see `ledger::ub_is_the_oracle_verdict`).
    ("memory/unsafe_noalias.lu", "exit(0)"),
    ("memory/unsafe_ub_uaf.lu", "ub(mem.ub)"),
    // The six fault-class programs this repo authored at is03 and upstreamed;
    // they arrived in `corpus/faults/` with pin `ecea37c` and the machine
    // already reproduced every one (`tests/faults/README.md`).
    ("faults/assert_fails.lu", "trap(assert)"),
    ("faults/bounds_slice.lu", "trap(bounds)"),
    ("faults/div_zero_rem.lu", "trap(div-zero)"),
    ("faults/exclusivity_nested_path.lu", "trap(exclusivity)"),
    ("faults/overflow_add.lu", "trap(overflow)"),
    ("faults/use_after_move_field.lu", "trap(use-after-move)"),
    // Const-generic normalization: `Buf[N + 1]` needs `type_arg ::= expr` to
    // read as an expression, which is01's parser only did for a *leading*
    // literal. Both files now parse, resolve and run.
    ("comptime/norm_linear.lu", "exit(0)"),
    ("comptime/norm_witness.lu", "exit(0)"),
    // -- is05 (pin 8b04edf) -------------------------------------------------
    // `regions.lu` RUNS AT LAST: the tail now reads `config.limit` under
    // `[mem.region.freeze.1]`'s "frozen data readable forever" instead of the
    // unspecified `frozen.get(config).is_valid()` — the is03 §5.5 finding,
    // closed by the pin rather than by this machine guessing.
    ("regions.lu", "exit(0)"),
    // s17's typecheck tier: the Tier-0 bodies run; the static rejections the
    // files pin (E0401/E0801/E0808…) are the compiler's half and ledger as
    // conservatism.
    ("rows/negative/open_into_closed.lu", "exit(1)"),
    ("rows/open_row_growth.lu", "exit(0)"),
    ("typecheck/cast_set.lu", "exit(0)"),
    ("typecheck/coerce_no_widening.lu", "exit(0)"),
    // exit(1) since 0.1.2: `Signal.Slow` dispatches to the `Slow` arm now
    // that bare identifiers resolve as variant patterns (issue #5) — the old
    // exit(0) was the first-arm-always bug's coincidence. The pinned
    // fail(E0801) stays the compiler's half (conservatism).
    ("typecheck/match_missing.lu", "exit(1)"),
    ("typecheck/match_unreachable.lu", "exit(0)"),
    ("typecheck/pattern_shape.lu", "exit(0)"),
    // -- is07 (pin 79ceec6) --------------------------------------------------
    // The three region-inference demonstrations run with zero annotations —
    // the machine's dynamic region semantics needed nothing new. The E1004/
    // E1010 litmuses and `mode_missing_mut` pin *static* rejections
    // (`fail(E1004)`/`fail(E1010)`/`fail(E1007)`); their Tier-0 bodies run
    // here and ledger as conservatism.
    ("memory/mode_missing_mut.lu", "exit(1)"),
    ("memory/region_conflict_params.lu", "exit(0)"),
    ("memory/region_escape_local.lu", "exit(0)"),
    ("memory/region_infer_list_builder.lu", "exit(0)"),
    ("memory/region_infer_request_handler.lu", "exit(0)"),
    ("memory/region_infer_tree_transform.lu", "exit(0)"),
    // -- is08 (pin 843174f) --------------------------------------------------
    // The s20 S-batch made `procs.lu` and `conc/proc_kill_defers.lu`
    // self-contained (S-5 resolved): the supervision showcase finally RUNS,
    // and kill-skips-defers prints exactly `released`. The region-checker
    // tier's pass files run; the fail litmuses' dynamic counterparts land on
    // `[conf.trap.map]`'s region-fault row (E1005/E1011).
    // `region_freeze_write`'s write used to land on a value-semantics copy
    // (the standing conservatism class, wolf-interp#2); struct values now
    // carry their allocation-site region, so the write through the frozen
    // value traps `[mem.region.freeze.1]` like every other frozen path.
    ("procs.lu", "exit(0)"),
    ("conc/proc_kill_defers.lu", "exit(0)"),
    ("memory/region_freeze_ok.lu", "exit(0)"),
    ("memory/region_freeze_open.lu", "trap(region-fault)"),
    ("memory/region_freeze_write.lu", "trap(region-fault)"),
    ("memory/region_multiopen_values_ok.lu", "exit(0)"),
    ("memory/region_open_ancestor.lu", "trap(region-fault)"),
    ("memory/region_transfer_open.lu", "trap(region-fault)"),
    // -- 0.1.2 (pin a0c4564) -------------------------------------------------
    // The pin's new unsafe/checked litmuses run clean (`unsafe_raw_outside`
    // and `unsafe_sig` pin static rejections E1301/E1302 — conservatism;
    // their Tier-0 bodies run). `let_shadow_var_ok` is E0410's ok-twin: it
    // runs `exit(0)` now that same-scope `let` shadowing reads the *latest*
    // binding (the `rposition` repair) — its fail-twins `let_reassign` and
    // `let_compound_assign` stop at `resolve` with E0410 and so never enter
    // this ledger.
    ("memory/checked_unsigned.lu", "exit(0)"),
    ("memory/unsafe_creation_not_use.lu", "exit(0)"),
    ("memory/unsafe_raw_outside.lu", "exit(0)"),
    ("memory/unsafe_sig.lu", "exit(0)"),
    ("memory/unsafe_trusted.lu", "exit(0)"),
    ("typecheck/let_shadow_var_ok.lu", "exit(0)"),
    // -- 0.1.3 (pin d147a54) -------------------------------------------------
    // The s27 realignment's run-rung gains, two families:
    //
    // The pin's own new witnesses run to their pinned exit —
    // `rows/qmark_defer.lu` (postfix rows, `?` across defer chains, lowercase
    // declared tags dispatching in `match`) and `faults/assert_msg_holds.lu`
    // (the two-arg assert whose holding message stays cold).
    //
    // Impl-block methods dispatch now (`[mem.iter.for]` needed `next`; the
    // rest came with it): the method-shaped files that were `unsupported`
    // reach `run`. `method_inherent` prints both resolution orders
    // (inherent wins; `Speak.speak(d)` reaches the shadowed trait method);
    // `receiver_modes`, `exclusivity`, `view_set_norm`, `assoc_rewrite`
    // (`exit(7)` is its own pinned arithmetic) and `show_bound` run clean.
    // `method_ambiguous`, `method_scope/main`, `view_set_violation` and
    // `receiver_bare_mut` run where the compiler statically rejects — the
    // standing conservatism class, not divergences.
    ("faults/assert_msg_holds.lu", "exit(0)"),
    ("memory/exclusivity.lu", "exit(0)"),
    ("memory/view_set_norm.lu", "exit(0)"),
    ("memory/view_set_violation.lu", "exit(0)"),
    ("rows/qmark_defer.lu", "exit(0)"),
    ("traits/assoc_rewrite.lu", "exit(7)"),
    ("traits/show_bound.lu", "exit(0)"),
    ("typecheck/method_ambiguous.lu", "exit(0)"),
    ("typecheck/method_inherent.lu", "exit(0)"),
    ("typecheck/method_scope/main.lu", "exit(0)"),
    ("typecheck/receiver_bare_mut.lu", "exit(42)"),
    ("typecheck/receiver_modes.lu", "exit(0)"),
];

#[test]
fn no_corpus_file_mismatches_its_expectation() {
    // A mismatch that has been triaged and filed in `docs/divergence-log.md`
    // (mirrored by `differ::FILED_DIVERGENCES`) is a *known* disagreement:
    // still visible in every differential report, no longer re-asserted here.
    // The waiver dies with the filing — resolve the entry and this test
    // resumes gating the file.
    let mismatches: Vec<String> = entries()
        .iter()
        .filter(|entry| entry.judgement.is_mismatch())
        .filter(|entry| wolf_interp::differ::filed(&entry.path).is_none())
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
fn the_run_ledger_is_exactly_what_reaches_run() {
    let observed: BTreeMap<String, String> = entries()
        .into_iter()
        .filter(|entry| entry.phase == Phase::Run)
        .map(|entry| (entry.path, entry.verdict.to_string()))
        .collect();
    let expected: BTreeMap<String, String> = RUN_LEDGER
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
        // A filed divergence (docs/divergence-log.md) is a known disagreement:
        // visible in every differential report, waived here until resolved.
        if wolf_interp::differ::filed(&entry.path).is_some() {
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
            // `run(exit=trap(ub))` against `ub(anchor)`: one event, two lenses.
            // `[conf.trap.map]` routes the `ub` kind's comparison semantics to
            // `[proto.record.ub]` precisely so the two must agree.
            (ExitSpec::Trap(want), Verdict::Ub(anchor)) => {
                assert!(
                    ledger::ub_is_the_oracle_verdict(*want),
                    "{}: expected {}, observed ub({anchor})",
                    entry.path,
                    ExitSpec::Trap(*want)
                );
            }
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
        checked >= 20,
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
                entry.phase <= Phase::Resolve,
                "{}: this implementation fails at lex/parse/resolve only, not {}",
                entry.path,
                entry.phase
            ),
            // A `ub` verdict reaches `run` exactly as a trap does: the
            // execution *completed*, at the defined stopping point "this has no
            // defined behavior". Inflating or deflating it would both be lies.
            Verdict::Exit(_) | Verdict::Trap(_) | Verdict::Ub(_) => {
                assert_eq!(entry.phase, Phase::Run, "{}", entry.path);
            }
            Verdict::Pass => {
                panic!("{}: unexpected verdict {}", entry.path, entry.verdict)
            }
        }
    }
}

#[test]
fn unsafe_free_corpus_programs_never_produce_a_ub_verdict() {
    // The sprint's headline claim as a CI assertion: **zero UB in safe code**.
    //
    // `[mem.ub]` states it — "Safe-tier programs cannot reach any row — every
    // row requires Tier 3 (or an FFI boundary) in the execution" — and this is
    // that sentence checked against every program in the pinned corpus rather
    // than argued. The filter is the source text: a file with no `unsafe`
    // block, no `asm`, and no `import c` is safe-tier by construction, and its
    // verdict must never be `ub(…)`.
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("walkable");
    let mut safe = 0usize;
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let text = String::from_utf8_lossy(&source);
        if text.contains("unsafe") || text.contains("import c ") || text.contains("asm ") {
            continue;
        }
        safe += 1;
        let (record, observed) = wolf_interp::observe_record(&full, &source, None);
        assert!(
            !matches!(record.verdict, Verdict::Ub(_)),
            "{}: a safe-tier program reached §7/{} — either the machine is wrong or `[mem.ub]`'s \
             \"safe-tier programs cannot reach any row\" is\n{}",
            file.path,
            observed
                .ub
                .as_ref()
                .map_or_else(|| "?".to_owned(), |finding| finding.row.to_string()),
            observed
                .ub
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
        );
    }
    assert!(safe > 90, "only {safe} safe-tier programs were checked");
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

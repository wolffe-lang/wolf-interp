//! The dynamic region model, end to end — is03's acceptance test.
//!
//! Four claims the sprint makes, each an executable one:
//!
//! 1. **Every fault class has a near-miss twin.** A machine that faults on
//!    everything satisfies "the fault programs fault"; only the legal twin
//!    shows the check is *discriminating*. `tests/faults/*.lu` are the faults
//!    (snapshot-tested in `fault_snapshots.rs`); `tests/faults/ok/*.lu` are the
//!    twins, and each one must run clean.
//! 2. **The leak assertion.** "At program exit, every region freed" — the
//!    invariant is06's crash-cleanup oracle depends on. Asserted over every
//!    corpus file that runs to a clean exit, plus the local programs.
//! 3. **The forest invariant.** Re-walked after every mutation of the region
//!    graph, and asserted here over the whole corpus. A planted
//!    invariant-breaking mutation trips it — that negative control lives in
//!    `src/eval/region.rs`'s unit tests, where a `Store` can be corrupted
//!    directly.
//! 4. **The D3 witnesses.** One program per optimizer fact
//!    (`spec/02` §7's O1/O2/O3b/O4 and "imm-forever"), whose *traced* behavior
//!    exhibits the rule that licenses it. These become s26's lowering tests and
//!    s23's differential seeds, so the trace is snapshot-tested rather than
//!    merely inspected.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use wolf_interp::corpus::{self, Outcome};
use wolf_interp::directive::{Check, ExitSpec};
use wolf_interp::eval::Trace;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
}

/// Every `.lu` directly under `dir`, sorted.
fn programs(dir: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} is readable: {e}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "lu"))
        .map(|path| {
            let name = path
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned();
            (name, std::fs::read_to_string(&path).expect("readable"))
        })
        .collect();
    out.sort();
    out
}

// -- 1. fault classes and their near-miss twins ----------------------------

#[test]
fn every_near_miss_twin_runs_clean() {
    let twins = programs(&tests_dir().join("faults").join("ok"));
    assert!(twins.len() >= 6, "the twin set shrank: {}", twins.len());
    for (name, source) in twins {
        let directives =
            wolf_interp::directive::parse_header(&source).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            matches!(
                directives.check,
                Some(Check::Run {
                    exit: ExitSpec::Code(0),
                    ..
                })
            ),
            "{name}: a near-miss twin pins `run(exit=0)` — that is what makes it a twin"
        );
        let observation = wolf_interp::frontend::observe(source.as_bytes(), None);
        assert_eq!(observation.verdict, Verdict::Exit(0), "{name}");
        assert_eq!(observation.phase_reached, Phase::Run, "{name}");
        assert!(observation.leaks.is_empty(), "{name} leaked a region");
        assert_eq!(observation.forest, Ok(()), "{name}");
    }
}

#[test]
fn every_region_fault_class_has_a_program_and_a_twin() {
    // The six §3/§4 fault classes the sprint enumerates, each pinned to the
    // clause it enforces, and each paired with a legal program that exercises
    // the same machinery without faulting.
    let faults = tests_dir().join("faults");
    let expected: &[(&str, &str, &str)] = &[
        // fault program, clause anchor, its near-miss twin
        (
            "region_uaf.lu",
            "mem.region.intra.2",
            "region_edge_intra_ok.lu",
        ),
        (
            "region_edge_cross.lu",
            "mem.region.edge",
            "region_edge_intra_ok.lu",
        ),
        (
            "region_freeze_write.lu",
            "mem.region.freeze.1",
            "region_freeze_read_ok.lu",
        ),
        (
            "region_suspended_write.lu",
            "mem.region.open.3",
            "region_reopen_ok.lu",
        ),
        (
            "region_move_open.lu",
            "mem.region.freeze.3",
            "region_transfer_closed_ok.lu",
        ),
        (
            "region_multiopen_nested.lu",
            "mem.region.multiopen",
            "region_reopen_ok.lu",
        ),
        (
            "handle_stale_reuse.lu",
            "mem.shared.handle.2",
            "handle_reserve_init_ok.lu",
        ),
        (
            "handle_uninit.lu",
            "mem.shared.handle.1",
            "handle_reserve_init_ok.lu",
        ),
    ];
    for (program, anchor, twin) in expected {
        let source = std::fs::read_to_string(faults.join(program))
            .unwrap_or_else(|e| panic!("{program}: {e}"));
        let trap = wolf_interp::frontend::observe(source.as_bytes(), None)
            .trap
            .unwrap_or_else(|| panic!("{program} did not fault"));
        assert_eq!(trap.rule.anchor(), *anchor, "{program}");
        assert!(
            faults.join("ok").join(twin).exists(),
            "{program}'s twin {twin} is missing"
        );
    }
}

#[test]
fn the_region_family_maps_onto_one_trap_kind() {
    // `[conf.trap.set]` is closed and `[conf.trap.map]` maps the whole §3
    // family onto `region-fault`: use-after-free, an illegal edge, a frozen
    // write, a suspended write and an open-discipline violation are one kind
    // with five clauses. The *rule* is what tells them apart.
    let faults = tests_dir().join("faults");
    let mut clauses = BTreeSet::new();
    for (name, source) in programs(&faults) {
        let Some(trap) = wolf_interp::frontend::observe(source.as_bytes(), None).trap else {
            panic!("{name} did not fault");
        };
        if trap.kind == TrapKind::RegionFault {
            clauses.insert(trap.rule.anchor());
        }
    }
    assert_eq!(
        clauses,
        BTreeSet::from([
            "mem.region.edge",
            "mem.region.freeze.1",
            "mem.region.freeze.3",
            "mem.region.intra.2",
            "mem.region.multiopen",
            "mem.region.open.3",
        ]),
        "the region-fault clause set moved"
    );
}

// -- 2 & 3. the leak assertion and the forest invariant ---------------------

#[test]
fn no_program_that_exits_cleanly_leaks_a_region() {
    // > Leak assertion: at program exit, every region freed (the is06
    // > crash-cleanup oracle depends on it).
    //
    // Scoped to *clean exits* deliberately: a program that traps left its
    // scopes without running them, and the regions it still holds are the
    // crash-cleanup oracle's subject rather than a leak.
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("walkable");
    let mut ran = 0usize;
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let (record, observed) = wolf_interp::observe_record(&full, &source, None);
        if !matches!(record.verdict, Verdict::Exit(_)) {
            continue;
        }
        ran += 1;
        assert!(
            observed.leaks.is_empty(),
            "{} exited cleanly holding region(s) {:?}",
            file.path,
            observed.leaks
        );
    }
    assert!(ran >= 45, "only {ran} corpus files exited cleanly");
}

#[test]
fn the_forest_invariant_holds_over_the_whole_corpus() {
    // "After every mutation, a debug assertion re-walks the region graph and
    // asserts the forest invariant." The assertion records rather than panics,
    // so this is where a recorded break becomes a failure.
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("walkable");
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let (_, observed) = wolf_interp::observe_record(&full, &source, None);
        assert_eq!(observed.forest, Ok(()), "{}", file.path);
    }
}

#[test]
fn a_region_that_faults_mid_block_is_still_freed_at_the_brace() {
    // The sugar's `}` frees on the trap path too, which is what makes is06's
    // crash-cleanup oracle checkable: a faulting program leaks nothing the
    // block itself owned.
    let source = "\
struct Node { value: int }

fn main() -> !int {
    region r: pool(Node) {
        var p = Pool[Node]()
        let h = p.reserve()
        p.init(h, Node { value: 1 })
        p.remove(h)
        p[h].value
    }
}
";
    let observation = wolf_interp::frontend::observe(source.as_bytes(), None);
    assert_eq!(observation.verdict, Verdict::Trap(TrapKind::StaleHandle));
    assert!(
        observation.leaks.is_empty(),
        "the faulting block leaked {:?}",
        observation.leaks
    );
}

#[test]
fn a_shared_cell_is_released_when_its_last_strong_owner_dies() {
    // `[mem.shared.drop.3]`: "runs it when the last strong count drops, at that
    // release point". No user destructors exist yet, so the observable is the
    // cell's own liveness at exit.
    let source = "\
struct Cfg { limit: int }

fn main() -> !int {
    let a = shared (Cfg { limit: 7 })
    let b = a.clone()
    if b.limit == 7 { 0 } else { 1 }
}
";
    let observation = wolf_interp::frontend::observe(source.as_bytes(), None);
    assert_eq!(observation.verdict, Verdict::Exit(0));
    assert!(observation.leaks.is_empty());
}

// -- 4. the D3 optimizer-fact witnesses ------------------------------------

#[test]
fn every_d3_witness_runs_and_its_trace_names_the_licensing_rule() {
    // Each witness program's traced behavior must exhibit the rule that
    // licenses the optimizer fact it stands for. Asserting on the *anchors* in
    // the trace, rather than on the program's exit status alone, is what makes
    // it a witness rather than another passing test.
    let dir = tests_dir().join("witness");
    let expected: &[(&str, &[&str])] = &[
        // D3/O1 — `mut` params lower to `noalias` + `dereferenceable`.
        (
            "mut_noalias.lu",
            &["mem.tier0.mode.mut", "mem.tier0.excl.2"],
        ),
        // D3/O2 — `read` params are immutable for the call; `imm` needs no sync.
        (
            "read_frozen.lu",
            &["mem.tier0.mode.read", "mem.region.freeze.1"],
        ),
        // D3/O3b — one alias-scope domain per region.
        (
            "region_disjointness.lu",
            &["mem.region.multiopen", "mem.region.create.3"],
        ),
        // D3 — `imm` data const-propagates, forever.
        (
            "imm_forever.lu",
            &["mem.region.freeze.1", "mem.region.edge.imm"],
        ),
        // D3/O4 — regions not open in the current scope yield `invariant.load`.
        (
            "suspended_invariant.lu",
            &["mem.region.open.3", "mem.shared.handle.3"],
        ),
    ];
    assert_eq!(
        programs(&dir).len(),
        expected.len(),
        "the witness set moved; every D3 fact owes exactly one program"
    );

    for (name, anchors) in expected {
        let path = dir.join(name);
        let source = std::fs::read(&path).expect("readable");
        let (record, observed) =
            wolf_interp::observe_record_traced(&path, &source, None, Trace::Memory);
        assert_eq!(record.verdict, Verdict::Exit(0), "{name}");
        assert!(observed.leaks.is_empty(), "{name} leaked");
        let cited: BTreeSet<&str> = observed
            .trace
            .iter()
            .filter_map(|line| {
                line.split('[')
                    .nth(1)
                    .and_then(|rest| rest.split(']').next())
            })
            .collect();
        for anchor in *anchors {
            assert!(
                cited.contains(anchor),
                "{name}: the trace never cites [{anchor}]; it cites {cited:?}"
            );
        }
        // `--trace=mem` is a filter over the registry, so nothing else may
        // appear: a non-`mem` rule in a memory trace is a filter bug.
        for anchor in &cited {
            assert!(
                anchor.starts_with("mem."),
                "{name}: `--trace=mem` leaked a non-memory rule [{anchor}]"
            );
        }
    }
}

#[test]
fn the_memory_trace_is_a_filter_over_the_same_registry() {
    // `--trace` and `--trace=mem` must agree on the memory rules: the filter
    // selects, it does not re-derive.
    let path = corpus_root().join("memory").join("region_multiopen_ok.lu");
    let source = std::fs::read(&path).expect("readable");
    let (_, all) = wolf_interp::observe_record_traced(&path, &source, None, Trace::All);
    let (_, mem) = wolf_interp::observe_record_traced(&path, &source, None, Trace::Memory);
    let memory_of_all: Vec<&String> = all
        .trace
        .iter()
        .filter(|line| {
            line.split('[')
                .nth(1)
                .and_then(|rest| rest.split(']').next())
                .is_some_and(|anchor| anchor.starts_with("mem."))
        })
        .collect();
    let filtered: Vec<&String> = mem.trace.iter().collect();
    assert_eq!(memory_of_all, filtered);
    assert!(!filtered.is_empty());
    assert!(
        all.trace.len() > filtered.len(),
        "the filter filtered nothing"
    );
}

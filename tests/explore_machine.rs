//! The is07 schedule explorer, litmus by litmus.
//!
//! Everything the sprint's acceptance names is here as an executable claim:
//! the hand-computable ping-pong enumeration, the measurable DPOR win over
//! naive DFS with identical bug-finding, the planted schedule-dependent bug
//! found and replayed from its emitted seed, the DRF stability oracle, the
//! kill/cancel defer law held across every schedule, the multiopen forest
//! invariant model-checked under concurrency, the deadlock verdict counted
//! per schedule, and the red test: deliberately-leaked nondeterminism caught
//! by the completeness assertion (`[conc.det.flow]`), then reverted — the
//! leak exists only behind the test-only option this file sets.

use wolf_interp::eval::sched::{self, Decision, DecisionKind};
use wolf_interp::eval::{self, Machine, Outcome, SchedRequest};
use wolf_interp::explore::{self, Options, Report};

fn run_explore(source: &str, options: &Options) -> Report {
    explore::explore_source("t.lu", source, options).expect("parses")
}

fn dpor() -> Options {
    Options {
        max_schedules: 100_000,
        ..Options::default()
    }
}

fn naive() -> Options {
    Options {
        naive: true,
        ..dpor()
    }
}

// ---------------------------------------------------------------------------
// 1. the hand-computable case
// ---------------------------------------------------------------------------

/// Two tasks over two rendezvous channels. The paper enumeration:
///
/// - The only decision with more than one candidate is the first (`main`
///   blocks at the scope join with both children fresh: ready = {P, Q}).
/// - **P first**: P blocks sending `ping` (no receiver) → only Q is ready →
///   Q pairs the recv, runs to its `pong` send and blocks (P not receiving
///   yet) → only P → P's send completed, pairs `pong`, finishes → only Q →
///   Q finishes. Every decision after the first is a singleton.
/// - **Q first**: Q blocks receiving `ping` → only P → P's send pairs
///   immediately (rendezvous completes synchronously) and P blocks on
///   `pong` → only Q → Q sends (pairs), finishes → only P → P finishes.
///
/// Two maximal schedules, both `exit(0)`. DPOR keeps both — the initiation
/// orders of the conflicting send/recv on `ping` are not happens-before
/// ordered, so the two interleavings are inequivalent — and the count equals
/// the Mazurkiewicz class count, also 2.
const PING_PONG: &str = r#"fn main() -> !int {
    let ping = channel[int](0)
    let pong = channel[int](0)
    scope s {
        s.spawn(fn() {
            ping.send(1)
            let r = pong.recv() else |_| { return 1 }
            if r == 2 { 0 } else { 3 }
        })
        s.spawn(fn() {
            let v = ping.recv() else |_| { return 2 }
            pong.send(v + 1)
            0
        })
    }
    0
}
"#;

#[test]
fn ping_pong_matches_the_paper_enumeration() {
    let with_naive = run_explore(PING_PONG, &naive());
    assert_eq!(with_naive.schedules, 2, "the paper says two schedules");
    assert!(with_naive.stable() && with_naive.green());

    let with_dpor = run_explore(PING_PONG, &dpor());
    assert_eq!(
        with_dpor.schedules, 2,
        "two Mazurkiewicz classes, also by hand"
    );
    assert!(with_dpor.stable() && with_dpor.green());
    assert_eq!(with_dpor.outcomes[0].verdict, "exit(0)");
    assert!(
        !with_dpor.frontier_open,
        "the space is tiny and fully closed"
    );
}

// ---------------------------------------------------------------------------
// 2. DPOR wins measurably, with identical results
// ---------------------------------------------------------------------------

/// Five writers, each touching only its own channel: every pair of slices
/// commutes. Naive DFS explores all 5! = 120 orders (each writer's whole
/// body is one slice — buffered sends never block — so the tree is exactly
/// the permutations of five independent slices); DPOR proves one schedule
/// suffices. 120x ≥ the sprint's 10x bar.
const INDEPENDENT_WRITERS: &str = r#"fn main() -> !int {
    let a = channel[int](2)
    let b = channel[int](2)
    let c = channel[int](2)
    let d = channel[int](2)
    let e = channel[int](2)
    scope s {
        s.spawn(fn() { a.send(1) })
        s.spawn(fn() { b.send(2) })
        s.spawn(fn() { c.send(3) })
        s.spawn(fn() { d.send(4) })
        s.spawn(fn() { e.send(5) })
    }
    let va = a.recv() else |_| { return 1 }
    let vb = b.recv() else |_| { return 1 }
    let vc = c.recv() else |_| { return 1 }
    let vd = d.recv() else |_| { return 1 }
    let ve = e.recv() else |_| { return 1 }
    if va + vb + vc + vd + ve == 15 { 0 } else { 1 }
}
"#;

#[test]
fn dpor_beats_naive_dfs_tenfold_with_identical_findings() {
    let with_naive = run_explore(INDEPENDENT_WRITERS, &naive());
    assert_eq!(
        with_naive.schedules, 120,
        "5! permutations of independent slices"
    );

    let with_dpor = run_explore(INDEPENDENT_WRITERS, &dpor());
    assert_eq!(with_dpor.schedules, 1, "every order commutes: one class");
    assert!(
        with_naive.schedules >= 10 * with_dpor.schedules,
        "the sprint's reduction bar: {} vs {}",
        with_naive.schedules,
        with_dpor.schedules
    );

    // Identical bug-finding: the same outcome digests, here exactly one.
    let naive_digests: Vec<u128> = with_naive.outcomes.iter().map(|o| o.digest).collect();
    let dpor_digests: Vec<u128> = with_dpor.outcomes.iter().map(|o| o.digest).collect();
    assert_eq!(naive_digests, dpor_digests);
    assert!(with_naive.green() && with_dpor.green());
}

// ---------------------------------------------------------------------------
// 3. the planted schedule-dependent bug, found and replayed
// ---------------------------------------------------------------------------

/// The program's exit code is whichever send committed first — a bug that
/// only fires under one interleaving, planted on purpose.
const ORDER_DEPENDENT: &str = r#"fn main() -> !int {
    let ch = channel[int](2)
    scope s {
        s.spawn(fn() { ch.send(1) })
        s.spawn(fn() { ch.send(2) })
    }
    let first = ch.recv() else |_| { return 9 }
    first
}
"#;

#[test]
fn a_planted_schedule_dependent_bug_is_found_and_its_seed_replays_it() {
    let report = run_explore(ORDER_DEPENDENT, &dpor());
    assert!(!report.stable(), "the dependence must be exposed");
    assert_eq!(report.outcomes.len(), 2);
    let verdicts: Vec<&str> = report.outcomes.iter().map(|o| o.verdict.as_str()).collect();
    assert!(verdicts.contains(&"exit(1)") && verdicts.contains(&"exit(2)"));

    // Every distinct outcome carries a replayable handle; feed each seed
    // back and the machine reproduces that outcome deterministically —
    // [proto.seed.equal], demonstrated rather than claimed.
    for outcome in &report.outcomes {
        let seed = outcome.seed.expect("these tiny streams pack into 62 bits");
        for _ in 0..2 {
            let run =
                eval::run_source_seeded("t.lu", ORDER_DEPENDENT, Some(seed), eval::Trace::Off)
                    .expect("parses");
            let verdict = match run.outcome {
                Outcome::Exit(status) => format!("exit({status})"),
                other => panic!("expected an exit, got {other:?}"),
            };
            assert_eq!(
                verdict, outcome.verdict,
                "seed {seed} must replay its schedule"
            );
        }
    }

    // The counterexample's non-FIFO schedule replays through the explicit
    // `ev:` stream spelling too (the fallback for streams too large to pack).
    let divergent = report
        .outcomes
        .iter()
        .find(|o| o.preemptions > 0)
        .expect("one witness departs from FIFO");
    let program = wolf_interp::sema::load_source("t.lu", ORDER_DEPENDENT).expect("loads");
    let run =
        Machine::with_request(&program, &SchedRequest::Stream(divergent.choices.clone())).run();
    match run.outcome {
        Outcome::Exit(status) => assert_eq!(format!("exit({status})"), divergent.verdict),
        other => panic!("expected an exit, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 4. the DRF stability oracle and the defer law, across every schedule
// ---------------------------------------------------------------------------

#[test]
fn the_kill_and_cancel_defer_law_holds_on_every_schedule() {
    // The is06 litmus, now quantified over the whole schedule space: on every
    // interleaving the killed proc's defer must NOT print, the cancelled
    // task's defer MUST, and the kill reason must deliver ([conc.proc.kill],
    // [conc.cancel.defer]). Stability makes the law a single assertion: one
    // outcome, and it is the lawful one.
    let source = r#"fn sleeper_body() -> !int {
    defer print_raw("KILLED-DEFER ")
    let never = channel[int](0)
    let v = never.recv()?
    v
}

fn boom() -> !int { Boom }

fn cancelled_half() -> !int {
    let ch = channel[int](0)
    scope s {
        s.spawn(fn() {
            defer print_raw("cancel-defer ")
            let v = ch.recv()?
            v
        })
        s.spawn(fn() { boom() })
    }
    0
}

fn main() -> !int {
    let w = spawn proc sleeper_body()
    let m = w.monitor()
    w.kill()
    select {
        exit(reason) from m => { if reason.is_killed() { print_raw("released ") } },
        timeout(1.s) => { print_raw("timeout ") },
    }
    let r = cancelled_half() else |_| { 0 }
    0
}
"#;
    let report = run_explore(source, &dpor());
    assert!(report.green(), "{report:?}");
    let stdout = String::from_utf8_lossy(&report.outcomes[0].stdout).into_owned();
    assert!(
        !stdout.contains("KILLED-DEFER"),
        "a killed proc ran defers on some schedule: {stdout}"
    );
    assert!(stdout.contains("released"), "{stdout}");
    assert!(stdout.contains("cancel-defer"), "{stdout}");
    assert!(
        report.schedules >= 2,
        "the law was checked across schedules, not on one"
    );
}

#[test]
fn a_deliberately_racy_program_traps_race_on_the_schedule_that_races() {
    // Two unordered pool-slot writes — the Go posture ([conc.mm.race.3]):
    // detect, report, halt. Every explored schedule agrees on the verdict.
    let source = r#"fn main() -> !int {
    region p: pool(int) {
        let pool = Pool[int]()
        let h = pool.reserve()
        pool.init(h, 0)
        scope s {
            s.spawn(fn() { pool.init(h, 1) })
            s.spawn(fn() { pool.init(h, 2) })
        }
    }
    0
}
"#;
    let report = run_explore(source, &dpor());
    assert!(report.schedules >= 2);
    assert_eq!(
        report.races, report.schedules,
        "every interleaving realizes the race"
    );
    for outcome in &report.outcomes {
        assert_eq!(outcome.verdict, "trap(race)");
    }
}

// ---------------------------------------------------------------------------
// 5. the multiopen model check ([mem.region.multiopen] under concurrency)
// ---------------------------------------------------------------------------

/// Two regions open in `main` (the corpus's flagged multiopen shape) while
/// two spawned tasks run and report through one contended channel: the
/// explorer answers the s20 fallback question over this space — no schedule
/// breaks the region forest invariant, and no schedule leaks.
const MULTIOPEN_UNDER_TASKS: &str = r#"fn main() -> !int {
    let a = region()
    let b = region()
    let ch = channel[int](2)
    var total = 0
    in a {
        var xs = List[int]()
        xs.push(1)
        in b {
            var ys = List[int]()
            ys.push(2)
            scope s {
                s.spawn(fn() { ch.send(10) })
                s.spawn(fn() { ch.send(20) })
            }
            total += xs[0] + ys[0]
        }
        total += xs[0]
    }
    let u = ch.recv() else |_| { return 3 }
    let v = ch.recv() else |_| { return 3 }
    if total == 4 { if u + v == 30 { 0 } else { 4 } } else { 5 }
}
"#;

/// A disconnected region transfers to one task — which OPENS it — while a
/// sibling task opens its own region and `main` holds a third open: three
/// open windows across three tasks, interleaved every legal way.
const TRANSFER_WHILE_MULTIOPEN: &str = r#"fn main() -> !int {
    let ch = channel[region](1)
    let done = channel[int](2)
    let p = region(pool(int))
    let pl = in p { Pool[int]() }
    let h = in p {
        let slot = pl.reserve()
        pl.init(slot, 41)
        slot
    }
    let a = region()
    var total = 0
    scope s {
        s.spawn(fn() {
            let r = ch.recv() else |_| { return 1 }
            let got = in r { pl[h] }
            done.send(got)
            0
        })
        s.spawn(fn() {
            let q = region()
            let n = in q {
                var zs = List[int]()
                zs.push(9)
                zs[0]
            }
            done.send(n)
            0
        })
        ch.send(move p)
        in a {
            var xs = List[int]()
            xs.push(1)
            total += xs[0]
        }
    }
    let u = done.recv() else |_| { return 2 }
    let v = done.recv() else |_| { return 2 }
    if total == 1 { if u + v == 50 { 0 } else { 3 } } else { 4 }
}
"#;

#[test]
fn no_schedule_breaks_the_multiopen_forest_invariant() {
    for (name, source) in [
        ("multiopen-under-tasks", MULTIOPEN_UNDER_TASKS),
        ("transfer-while-multiopen", TRANSFER_WHILE_MULTIOPEN),
    ] {
        let report = run_explore(source, &dpor());
        assert!(report.green(), "{name}: {report:?}");
        assert_eq!(report.outcomes[0].verdict, "exit(0)", "{name}");
        for outcome in &report.outcomes {
            assert!(
                outcome.forest_ok,
                "{name}: a schedule broke the forest invariant"
            );
            assert_eq!(outcome.leaks, 0, "{name}: a schedule leaked a region");
        }
        // The claim is over a real space, not a single trace.
        assert!(
            report.schedules >= 2,
            "{name}: explored {}",
            report.schedules
        );
        assert!(
            !report.frontier_open,
            "{name}: the answer must be definitive"
        );
    }
}

// ---------------------------------------------------------------------------
// 6. deadlock, budgets, pruning
// ---------------------------------------------------------------------------

#[test]
fn a_deadlocking_program_is_counted_deadlocked_on_every_schedule() {
    let source = r#"fn main() -> !int {
    let ch = channel[int](0)
    let v = ch.recv()?
    v
}
"#;
    let report = run_explore(source, &dpor());
    assert_eq!(report.schedules, 1);
    assert_eq!(
        report.deadlocks, 1,
        "the deadlock oracle counts per schedule"
    );
    assert!(report.outcomes[0].deadlocked);
    assert_eq!(
        report.outcomes[0].verdict, "trap(deadlock)",
        "[conc.deadlock.trap]: the verdict spelling is07 reports per schedule \
         (S-3 resolved by the s20 S-batch)"
    );
}

#[test]
fn an_exhausted_schedule_budget_reports_an_open_frontier_never_silence() {
    let options = Options {
        max_schedules: 3,
        naive: true,
        ..Options::default()
    };
    let report = run_explore(INDEPENDENT_WRITERS, &options);
    assert_eq!(report.executions, 3);
    assert!(report.frontier_open);
    assert!(report.notes.iter().any(|n| n.contains("budget exhausted")));
}

#[test]
fn the_preemption_bound_is_first_class_and_reports_what_it_skipped() {
    // Bound 0 pins strict FIFO: exactly one schedule runs, and the skipped
    // alternatives are declared an open frontier (CHESS's useful bound).
    let options = Options {
        max_preemptions: Some(0),
        ..dpor()
    };
    let report = run_explore(ORDER_DEPENDENT, &options);
    assert_eq!(report.schedules, 1);
    assert!(report.frontier_open);
    assert!(report.notes.iter().any(|n| n.contains("preemption bound")));
}

#[test]
fn paranoid_mode_verifies_every_hash_hit_and_finds_no_collision() {
    let options = Options {
        paranoid: true,
        ..dpor()
    };
    let report = run_explore(PING_PONG, &options);
    assert!(report.divergence.is_none(), "{:?}", report.divergence);
    assert!(report.green());
    // And pruning changes no conclusion: the unpruned space agrees.
    let unpruned = Options {
        prune: false,
        ..dpor()
    };
    let full = run_explore(PING_PONG, &unpruned);
    let a: Vec<u128> = report.outcomes.iter().map(|o| o.digest).collect();
    let b: Vec<u128> = full.outcomes.iter().map(|o| o.digest).collect();
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------
// 7. the red test: a leaked decision is caught, then reverted
// ---------------------------------------------------------------------------

#[test]
fn deliberately_unregistered_nondeterminism_is_caught_as_divergence() {
    // The completeness assertion under fire: replays perturb the ready queue
    // WITHOUT recording a decision (the test-only leak), and the explorer
    // must refuse to continue — this is the exact bug class that killed
    // madsim-style tools, surfaced as a hard error rather than absorbed.
    // "Reverted" is structural: the leak exists only behind this option.
    let options = Options {
        leak_nondeterminism: true,
        ..dpor()
    };
    let report = run_explore(PING_PONG, &options);
    let divergence = report.divergence.expect("the leak must be caught");
    assert!(divergence.contains("conc.det.flow"), "{divergence}");
    // And with the leak reverted, the same exploration is clean.
    assert!(run_explore(PING_PONG, &dpor()).divergence.is_none());
}

// ---------------------------------------------------------------------------
// 8. the seed encoding itself
// ---------------------------------------------------------------------------

#[test]
fn the_packed_seed_namespace_round_trips_and_declines_honestly() {
    // All-FIFO packs to seed 0 — the strict-FIFO seed replays it by
    // definition.
    let fifo = vec![decision(DecisionKind::Task, vec![7, 8], 0)];
    assert_eq!(sched::pack_schedule(&fifo), Some(0));

    // A non-FIFO stream packs into the tagged namespace…
    let stream = vec![
        decision(DecisionKind::Task, vec![1, 2, 3], 2),
        decision(DecisionKind::Task, vec![1, 2], 1),
        decision(DecisionKind::Arm, vec![0, 1], 1),
    ];
    let seed = sched::pack_schedule(&stream).expect("fits");
    assert!(sched::seed_is_packed(seed));
    // …as mixed-radix digits, least significant first: 2 + 3·(1 + 2·1) = 11.
    assert_eq!(seed, sched::PACKED_SEED_TAG | 11);

    // …and a stream too wide for 62 bits declines rather than truncates:
    // forty base-3 digits of 2 end past 2^62.
    let wide: Vec<Decision> = (0..40)
        .map(|_| decision(DecisionKind::Task, vec![0, 1, 2], 2))
        .collect();
    assert_eq!(sched::pack_schedule(&wide), None);

    // Generator seeds are untouched by the split: 0 and small values are not
    // packed, and [proto.seed.flag] semantics stay theirs.
    assert!(!sched::seed_is_packed(0));
    assert!(!sched::seed_is_packed(7));
}

fn decision(kind: DecisionKind, candidates: Vec<usize>, chosen: usize) -> Decision {
    Decision {
        kind,
        candidates,
        chosen,
        state: 0,
    }
}

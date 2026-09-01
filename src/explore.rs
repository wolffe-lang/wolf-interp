//! The schedule explorer — is07: bounded exhaustive interleaving search over
//! the is06 machine's reified `sched-ev/0` decision stream.
//!
//! # Architecture: stateless, replay-based (VeriSoft lineage)
//!
//! No interpreter state is ever checkpointed. To explore a sibling branch the
//! explorer re-executes from the root with the decision prefix *forced*
//! ([`crate::eval::sched::Plan`]); is06's determinism makes the replay exact,
//! and the completeness assertion makes it *checked*: every forced decision
//! carries the alternatives the recording run saw, and any mismatch — a
//! decision that escaped the registered schedule points — is a hard error,
//! never a silent skew (`[conc.det.flow]`; the bug class that killed
//! madsim-style tools elsewhere).
//!
//! # The branch alphabet
//!
//! Exactly is06's enumeration ([`crate::eval::sched`]): everything that blocks
//! or chooses flows through `State::decide`, which the machine reaches at two
//! kinds of point — **which ready task runs next** (spawn commits, unparks,
//! channel pairings, `when` grants, timer fires, cancellation and exit
//! deliveries all feed the ready queue this decision drains) and **which
//! simultaneously-ready `select` arm commits** (`[conc.select.fair]`). A
//! schedule is the sequence of (decision, choice) pairs and nothing else;
//! a prefix plus the deterministic machine fully determines the state.
//!
//! # Dynamic partial-order reduction (Flanagan–Godefroid 2005 shape)
//!
//! Naive DFS over the decision tree is factorial. The explorer maintains a
//! happens-before relation over executed operations — per-task vector clocks,
//! edges exactly at spec/03's synchronizes-before operations (send→recv,
//! release→acquire, spawn→first step, last step→join, exit→monitor delivery),
//! all recorded by the scheduler as [`OpRecord`]s — and, on each executed
//! conflicting-unordered pair, grows a **backtrack set** at the decision point
//! that scheduled the earlier operation's slice: the task whose step must also
//! be tried there (or, when it was not yet enabled there, conservatively every
//! enabled task — the persistent-set soundness fallback). DFS then explores
//! only backtrack sets instead of all enabled tasks; every Mazurkiewicz trace
//! (equivalence class under commuting independent steps) is still visited
//! (`[conc.det.dpor]`). Classic **sleep sets** ride along: a choice fully
//! explored at a node sleeps for its siblings until a conflicting slice wakes
//! it, killing the classic redundancy. Source-DPOR (Abdulla et al. 2014) is
//! the named upgrade path if benchmarks demand it — documented, not
//! implemented, at v1. `select`-arm decisions branch fully: arm commits are
//! payload-carrying and never provably commutative from the op log alone.
//!
//! # State hashing (convergence pruning)
//!
//! At every task decision the scheduler hashes its canonical visible state —
//! task states + queues, channel contents, mutexes, procs, scopes, timers,
//! the virtual clock, the race detector's memory and a rolling stdout digest
//! ([`crate::eval::sched::Decision::state`]). A state already fully explored
//! at equal or greater remaining depth budget is pruned. The hash cannot see
//! suspended interpreter frames (a task's Rust stack is not serializable), so
//! two schedules differing only in un-communicated locals could in principle
//! merge; `docs/approximation-contract.md` §10.6 records the approximation,
//! `prune: false` turns the pruning off, and `paranoid` re-verifies every
//! hash hit against the stored canonical preimage (the 128-bit-collision
//! guard). Pruning never suppresses an outcome: every *executed* schedule is
//! still run to completion and judged by every oracle.
//!
//! # Seeds — the s36 correspondence contract, one-sided today
//!
//! A failing schedule is emitted as a `--seed=N` value whenever its choices
//! pack into the 62-bit payload ([`crate::eval::sched::pack_schedule`]):
//! `[proto.seed.equal]` then holds — equal seeds replay byte-identical
//! records, output included. Streams that do not fit print the explicit
//! `--schedule=ev:c0,c1,…` spelling. The encoding is provisional until the
//! accepted s36 Phase A hook-design doc lands upstream; the compiler
//! runtime's format has priority and this side re-pins to it.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::eval::sched::{Decision, DecisionKind, ObjKey, OpRecord, Plan, fnv128, pack_schedule};
use crate::eval::{Machine, Outcome};
use crate::sema::Program;
use crate::trap::TrapKind;

/// Exploration budgets and modes — all CLI-settable (`conform-run
/// --explore=N` and friends). Exhausted budgets report "frontier open",
/// never silent success.
#[derive(Debug, Clone)]
pub struct Options {
    /// Maximum schedules to execute (each is one full replay).
    pub max_schedules: u64,
    /// Maximum decisions per schedule; deeper structure is depth-capped and
    /// reported as an open frontier.
    pub max_steps: usize,
    /// CHESS-style preemption bound: maximum non-FIFO choices per schedule.
    /// In this cooperative machine a "preemption" is exactly a decision that
    /// departs from the front of the ready queue. `None` = unbounded.
    pub max_preemptions: Option<u32>,
    /// Wall-clock budget for the whole exploration.
    pub wall: Option<Duration>,
    /// Full-branching DFS instead of DPOR — the baseline the ≥10x reduction
    /// acceptance is measured against. Disables sleep sets and pruning.
    pub naive: bool,
    /// State-hash convergence pruning (defaults on; off under `naive`).
    pub prune: bool,
    /// Keep canonical-state preimages and re-verify every hash hit by
    /// comparison — the 128-bit-collision guard.
    pub paranoid: bool,
    /// Test-only: plant unregistered nondeterminism in replays so the
    /// completeness assertion can be shown to catch it (the red test).
    #[doc(hidden)]
    pub leak_nondeterminism: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            max_schedules: 10_000,
            max_steps: 4_096,
            max_preemptions: None,
            wall: None,
            naive: false,
            prune: true,
            paranoid: false,
            leak_nondeterminism: false,
        }
    }
}

/// One distinct observable outcome, with a replay handle for its first
/// witnessing schedule.
#[derive(Debug, Clone)]
pub struct ScheduleOutcome {
    /// `exit(N)` / `trap(kind)` / `ub(anchor)` / `unsupported`.
    pub verdict: String,
    pub stdout: Vec<u8>,
    /// The per-schedule leak assertion (is03's oracle, per explored schedule).
    pub leaks: usize,
    pub live_cells: usize,
    pub host_leaks: usize,
    pub forest_ok: bool,
    /// The schedule hit the all-blocked/no-timer deadlock verdict.
    pub deadlocked: bool,
    /// FNV-128 over everything above — the stability comparison key.
    pub digest: u128,
    /// The full decision stream (chosen index per decision) of the first
    /// schedule that produced this outcome.
    pub choices: Vec<usize>,
    /// The packed `--seed=N` replay value, when the stream fits 62 bits.
    pub seed: Option<u64>,
    /// Non-FIFO choices in the witnessing schedule.
    pub preemptions: u32,
    /// How many explored schedules produced this outcome.
    pub count: u64,
}

impl ScheduleOutcome {
    /// The witnessing schedule as its readable, diffable `ev:…` spelling —
    /// the decision stream, which is what `--schedule=` replays.
    ///
    /// It is always available: the packed `--seed=N` handle is an encoding of
    /// this and exists only when the stream fits 62 bits, so a report that
    /// carried only the seed would lose the schedule for exactly the deep
    /// findings that most need it (wolf-interp#53).
    #[must_use]
    pub fn stream(&self) -> String {
        format!(
            "ev:{}",
            self.choices
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    /// The copy-pasteable replay spelling for this outcome's witness.
    #[must_use]
    pub fn replay(&self) -> String {
        match self.seed {
            Some(seed) => format!("--seed={seed}"),
            None => format!("--schedule={}", self.stream()),
        }
    }
}

/// What one exploration established, within its bounds.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Replays executed (every schedule is one execution from the root).
    pub executions: u64,
    /// Complete schedules explored and judged.
    pub schedules: u64,
    /// Redundant alternatives skipped by sleep sets.
    pub slept: u64,
    /// Subtrees skipped by state-hash convergence.
    pub pruned: u64,
    /// Schedules that ended in the deadlock verdict.
    pub deadlocks: u64,
    /// Schedules that trapped `race` (the VC detector's per-schedule oracle).
    pub races: u64,
    /// Unexplored alternatives remain (budget exhausted or bound-skipped).
    pub frontier_open: bool,
    /// Human-readable budget/frontier notes.
    pub notes: Vec<String>,
    /// Distinct observable outcomes, in first-seen order. One entry =
    /// observably deterministic; more = schedule-dependent behavior.
    pub outcomes: Vec<ScheduleOutcome>,
    /// The completeness violation, when a replay diverged — is06 leaked a
    /// decision; a top-severity interpreter bug, never a program finding.
    pub divergence: Option<String>,
    /// Per-schedule oracle failures (leaks, forest breaks), with replays.
    pub oracle_failures: Vec<String>,
    /// The deepest decision stream seen.
    pub max_depth: usize,
}

impl Report {
    /// The verdict-stability oracle: every explored schedule produced the
    /// same verdict, stdout and cleanup profile, and no decision escaped the
    /// stream. On a `sched-ev/0` machine this is the DRF-SC promise, checked
    /// rather than claimed (`[proto.seed.equal]`, D14).
    #[must_use]
    pub fn stable(&self) -> bool {
        self.outcomes.len() <= 1 && self.divergence.is_none()
    }

    /// Everything the sprint's oracles assert, in one predicate: stability,
    /// stream completeness, and clean per-schedule teardown.
    #[must_use]
    pub fn green(&self) -> bool {
        self.stable() && self.oracle_failures.is_empty()
    }
}

/// One node of the current DFS path: a decision point, its alternatives, and
/// the exploration bookkeeping DPOR grows on it.
#[derive(Debug)]
struct Node {
    kind: DecisionKind,
    candidates: Vec<usize>,
    /// Canonical-state hash (Task nodes; 0 for Arm nodes).
    state: u128,
    /// The candidate *value* currently being explored.
    current: usize,
    /// Shared-object footprint of the slice `current` runs (Task nodes).
    footprint: Vec<(ObjKey, bool)>,
    /// Candidate values already explored (or being explored).
    done: BTreeSet<usize>,
    /// Candidate values that must be explored (grown by DPOR; everything,
    /// for Arm nodes and in naive mode).
    backtrack: BTreeSet<usize>,
    /// Sleeping tasks at this node's state, with the footprint their pending
    /// slice had when they went to sleep.
    sleep: Vec<(usize, Vec<(ObjKey, bool)>)>,
    /// This node's state was already fully explored at equal or greater
    /// remaining budget — no alternatives are taken from it.
    pruned: bool,
}

impl Node {
    fn new(decision: &Decision, naive: bool) -> Node {
        let current = decision.candidates[decision.chosen];
        let mut done = BTreeSet::new();
        done.insert(current);
        let mut backtrack = BTreeSet::new();
        if naive || decision.kind == DecisionKind::Arm {
            backtrack.extend(decision.candidates.iter().copied());
        } else {
            backtrack.insert(current);
        }
        Node {
            kind: decision.kind,
            candidates: decision.candidates.clone(),
            state: decision.state,
            current,
            footprint: Vec::new(),
            done,
            backtrack,
            sleep: Vec::new(),
            pruned: false,
        }
    }

    /// Index of `current` in `candidates` — the plan spelling of the choice.
    fn current_index(&self) -> usize {
        self.candidates
            .iter()
            .position(|c| *c == self.current)
            .expect("current is always drawn from candidates")
    }
}

/// Two operations conflict when they touch the same object and at least one
/// mutates (`[conc.mm.race.3]`'s shape, lifted to runtime objects).
fn conflicts(a: &OpRecord, b: &OpRecord) -> bool {
    a.obj == b.obj && (a.write || b.write)
}

fn footprints_conflict(a: &[(ObjKey, bool)], b: &[(ObjKey, bool)]) -> bool {
    a.iter()
        .any(|(obj, w)| b.iter().any(|(obj2, w2)| obj == obj2 && (*w || *w2)))
}

/// `a` happens-before `b`: `b`'s clock has seen `a`'s own component.
fn happens_before(a: &OpRecord, b: &OpRecord) -> bool {
    let own = a.vc.get(a.task).copied().unwrap_or(0);
    b.vc.get(a.task).copied().unwrap_or(0) >= own
}

fn verdict_of(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Exit(status) => format!("exit({status})"),
        Outcome::Trap(trap) => format!("trap({})", trap.kind),
        Outcome::Ub(finding) => format!("ub({})", finding.anchor()),
        Outcome::Unsupported(_) => "unsupported".to_owned(),
    }
}

/// Explores every inequivalent schedule of `program` within the budget.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn explore(program: &Program, options: &Options) -> Report {
    let started = Instant::now();
    let mut report = Report::default();
    let mut path: Vec<Node> = Vec::new();
    // Forced prefix length for the coming run (0 = free run from the root).
    let mut forced = 0usize;
    // Task-decision states fully explored, keyed to the remaining depth
    // budget they were explored with.
    let mut visited: BTreeMap<u128, usize> = BTreeMap::new();
    // `--paranoid`: hash → canonical preimage, verified on every hit.
    let mut preimages: BTreeMap<u128, String> = BTreeMap::new();
    // Which outcome digests were seen, mapping into `report.outcomes`.
    let mut seen: BTreeMap<u128, usize> = BTreeMap::new();
    let mut depth_capped = false;
    let prune_on = options.prune && !options.naive;

    loop {
        if report.executions >= options.max_schedules {
            report.frontier_open = true;
            report.notes.push(format!(
                "schedule budget exhausted at {} execution(s); frontier open",
                report.executions
            ));
            break;
        }
        if let Some(wall) = options.wall
            && started.elapsed() >= wall
        {
            report.frontier_open = true;
            report.notes.push(format!(
                "wall-clock budget exhausted after {} execution(s); frontier open",
                report.executions
            ));
            break;
        }

        // -- one replay with the forced prefix -------------------------------
        let plan = Plan {
            choices: path[..forced].iter().map(Node::current_index).collect(),
            expected: path[..forced]
                .iter()
                .map(|n| (n.kind, n.candidates.clone()))
                .collect(),
            keep_preimages: options.paranoid,
            leak_nondeterminism: options.leak_nondeterminism && forced > 0,
        };
        let run = Machine::with_plan(program, plan).run();
        report.executions += 1;
        let trace = run.schedule;

        if let Some(divergence) = trace.divergence {
            report.divergence = Some(divergence);
            break;
        }
        if options.paranoid {
            let mut collision = None;
            for (hash, preimage) in &trace.preimages {
                if let Some(stored) = preimages.get(hash) {
                    if stored != preimage {
                        collision = Some(format!(
                            "state-hash collision at {hash:032x}: two distinct canonical \
                             states share a 128-bit hash — pruning is unsound for this \
                             program; rerun with pruning off"
                        ));
                        break;
                    }
                } else {
                    preimages.insert(*hash, preimage.clone());
                }
            }
            if let Some(collision) = collision {
                report.divergence = Some(collision);
                break;
            }
        }

        let decisions = &trace.decisions;
        report.max_depth = report.max_depth.max(decisions.len());
        if decisions.len() > options.max_steps && !depth_capped {
            depth_capped = true;
            report.frontier_open = true;
            report.notes.push(format!(
                "a schedule reached {} decision(s); alternatives beyond the {}-step \
                 cap stay unexplored",
                decisions.len(),
                options.max_steps
            ));
        }

        // -- rebuild the path beyond the forced prefix -----------------------
        path.truncate(forced);
        for decision in decisions
            .iter()
            .skip(forced)
            .take(options.max_steps.saturating_sub(forced))
        {
            path.push(Node::new(decision, options.naive));
        }

        // Footprints: the ops each slice performed, recomputed from this run
        // (deterministic per prefix, so prefix nodes always recompute the
        // same sets).
        for node in &mut path {
            node.footprint.clear();
        }
        for op in &trace.ops {
            if let Some(slice) = op.slice
                && slice < path.len()
            {
                path[slice].footprint.push((op.obj, op.write));
            }
        }

        // Sleep inheritance for the fresh part of the path: a sleeping task
        // stays asleep below until a conflicting slice wakes it.
        if !options.naive {
            for i in forced..path.len() {
                if i == 0 {
                    continue;
                }
                let inherited: Vec<(usize, Vec<(ObjKey, bool)>)> = path[i - 1]
                    .sleep
                    .iter()
                    .filter(|(_, fp)| !footprints_conflict(fp, &path[i - 1].footprint))
                    .cloned()
                    .collect();
                path[i].sleep = inherited;
            }
        }

        // -- DPOR: grow backtrack sets from conflicting-unordered pairs ------
        if !options.naive {
            for (bi, b) in trace.ops.iter().enumerate() {
                for a in trace.ops[..bi].iter().rev() {
                    if a.task == b.task || !conflicts(a, b) || happens_before(a, b) {
                        continue;
                    }
                    if let Some(slice) = a.slice
                        && slice < path.len()
                    {
                        let node = &mut path[slice];
                        if node.candidates.contains(&b.task) {
                            node.backtrack.insert(b.task);
                        } else {
                            // Not yet enabled at that state: conservatively
                            // try everything that was (persistent-set
                            // soundness fallback).
                            let all: Vec<usize> = node.candidates.clone();
                            node.backtrack.extend(all);
                        }
                    }
                }
            }
        }

        // -- state pruning bookkeeping ---------------------------------------
        if prune_on {
            for (i, node) in path.iter_mut().enumerate().skip(forced) {
                if node.kind != DecisionKind::Task {
                    continue;
                }
                let remaining = options.max_steps - i;
                if visited.get(&node.state).is_some_and(|r| *r >= remaining) {
                    node.pruned = true;
                    report.pruned += 1;
                }
            }
        }

        // -- judge this schedule ---------------------------------------------
        report.schedules += 1;
        if trace.deadlocked {
            report.deadlocks += 1;
        }
        let verdict = verdict_of(&run.outcome);
        if matches!(&run.outcome, Outcome::Trap(t) if t.kind == TrapKind::Race) {
            report.races += 1;
        }
        let forest_ok = run.forest.is_ok();
        let choices: Vec<usize> = decisions.iter().map(|d| d.chosen).collect();
        let preemptions = decisions
            .iter()
            .filter(|d| d.kind == DecisionKind::Task && d.chosen != 0)
            .count() as u32;
        let mut digest_input = String::new();
        {
            use std::fmt::Write as _;
            let _ = write!(
                digest_input,
                "v={verdict};leaks={};cells={};host={};forest={forest_ok};dead={};out=",
                run.leaks.len(),
                run.live_cells.len(),
                run.host_leaks.len(),
                trace.deadlocked
            );
        }
        let mut digest_bytes = digest_input.into_bytes();
        digest_bytes.extend_from_slice(&run.stdout);
        let digest = fnv128(&digest_bytes);

        let outcome_index = *seen.entry(digest).or_insert_with(|| {
            report.outcomes.push(ScheduleOutcome {
                verdict: verdict.clone(),
                stdout: run.stdout.clone(),
                leaks: run.leaks.len(),
                live_cells: run.live_cells.len(),
                host_leaks: run.host_leaks.len(),
                forest_ok,
                deadlocked: trace.deadlocked,
                digest,
                choices: choices.clone(),
                seed: pack_schedule(decisions),
                preemptions,
                count: 0,
            });
            report.outcomes.len() - 1
        });
        report.outcomes[outcome_index].count += 1;

        if (!run.leaks.is_empty() || !forest_ok) && report.outcomes[outcome_index].count == 1 {
            report.oracle_failures.push(format!(
                "{}: {} region leak(s), forest {} (replay: {})",
                verdict,
                run.leaks.len(),
                if forest_ok { "ok" } else { "BROKEN" },
                report.outcomes[outcome_index].replay()
            ));
        }

        // -- backtrack: deepest node with an unexplored alternative ----------
        let scan_top = path.len().min(options.max_steps);
        let first_pruned = path.iter().position(|n| n.pruned).unwrap_or(usize::MAX);
        let mut advanced = false;
        for d in (0..scan_top).rev() {
            if d >= first_pruned {
                // The subtree below a converged state is already explored.
                continue;
            }
            let prefix_preemptions: u32 = path[..d]
                .iter()
                .filter(|n| n.kind == DecisionKind::Task && n.current != n.candidates[0])
                .count() as u32;
            let node = &mut path[d];
            let waiting: Vec<usize> = node.backtrack.difference(&node.done).copied().collect();
            let mut pick = None;
            let mut bound_skipped = false;
            for candidate in waiting {
                if node.kind == DecisionKind::Task
                    && !options.naive
                    && node.sleep.iter().any(|(t, _)| *t == candidate)
                {
                    // Provably redundant by the sleep-set theorem: mark it
                    // explored without running it.
                    node.done.insert(candidate);
                    report.slept += 1;
                    continue;
                }
                let extra = u32::from(candidate != node.candidates[0]);
                if let Some(bound) = options.max_preemptions
                    && node.kind == DecisionKind::Task
                    && prefix_preemptions + extra > bound
                {
                    bound_skipped = true;
                    continue;
                }
                pick = Some(candidate);
                break;
            }
            if bound_skipped {
                if !report.frontier_open {
                    report.notes.push(format!(
                        "preemption bound {} skipped alternatives; frontier open",
                        options.max_preemptions.unwrap_or(0)
                    ));
                }
                report.frontier_open = true;
            }
            if let Some(pick) = pick {
                // The explored choice goes to sleep for its siblings.
                if node.kind == DecisionKind::Task && !options.naive {
                    let footprint = node.footprint.clone();
                    node.sleep.push((node.current, footprint));
                }
                node.done.insert(pick);
                node.current = pick;
                path.truncate(d + 1);
                forced = d + 1;
                advanced = true;
                break;
            }
            // Fully exhausted (nothing waiting, nothing bound-skipped): this
            // state's whole subtree is explored at this remaining budget.
            if prune_on
                && node.kind == DecisionKind::Task
                && !bound_skipped
                && node.backtrack.difference(&node.done).next().is_none()
            {
                let remaining = options.max_steps - d;
                let entry = visited.entry(node.state).or_insert(0);
                *entry = (*entry).max(remaining);
            }
        }
        if !advanced {
            break;
        }
    }

    report
}

/// As [`explore`], over a single in-memory source (the tests' door). Runs the
/// same admission ladder as every other evaluating door (issue #22).
///
/// # Errors
///
/// The rendered load error or admission refusal.
pub fn explore_source(name: &str, source: &str, options: &Options) -> Result<Report, String> {
    let program = match crate::sema::load_source(name, source) {
        Ok(program) => program,
        Err(crate::sema::LoadError::Syntax { file, diag }) => {
            // Offsets, deliberately: the failing file may be a `use`d module
            // loaded from disk, and this door holds only the entry source —
            // rendering `line:col` against the wrong text would lie.
            return Err(format!("{file}: {diag}"));
        }
        Err(crate::sema::LoadError::Io(message)) => return Err(message),
    };
    match crate::frontend::admit(&program) {
        Some(crate::frontend::Refusal::Reject(diag)) => Err(format!(
            "{diag} — statically rejected; the explorer runs the same admission ladder as `run`"
        )),
        Some(crate::frontend::Refusal::Unsupported(reason)) => {
            Err(format!("unsupported — {reason}"))
        }
        None => Ok(explore(&program, options)),
    }
}

/// Renders the human report for `conform-run --explore`.
#[must_use]
pub fn render(file: &str, report: &Report, options: &Options) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{file}: explored {} schedule(s) in {} execution(s) ({}; {} slept, {} pruned), \
         frontier {}",
        report.schedules,
        report.executions,
        if options.naive { "naive DFS" } else { "DPOR" },
        report.slept,
        report.pruned,
        if report.frontier_open {
            "OPEN"
        } else {
            "closed"
        },
    );
    for note in &report.notes {
        let _ = writeln!(out, "  note: {note}");
    }
    if let Some(divergence) = &report.divergence {
        let _ = writeln!(out, "  COMPLETENESS VIOLATION: {divergence}");
    }
    let _ = writeln!(
        out,
        "  outcomes: {} distinct — {}",
        report.outcomes.len(),
        if report.stable() {
            "observably deterministic (every schedule agrees)"
        } else {
            "SCHEDULE-DEPENDENT"
        }
    );
    for outcome in &report.outcomes {
        let _ = writeln!(
            out,
            "    {} ×{} stdout={} leaks={} forest={}{} — replay: {}",
            outcome.verdict,
            outcome.count,
            String::from_utf8_lossy(&outcome.stdout).escape_debug(),
            outcome.leaks,
            if outcome.forest_ok { "ok" } else { "BROKEN" },
            if outcome.deadlocked { " DEADLOCK" } else { "" },
            outcome.replay(),
        );
        if !report.stable() {
            // A finding prints its full decision stream, not only the packed
            // handle: `--schedule=ev:…` replays it exactly.
            let _ = writeln!(out, "      decision stream: {}", outcome.stream());
        }
    }
    let _ = writeln!(
        out,
        "  deadlocks: {} · races: {} · max depth: {} decision(s)",
        report.deadlocks, report.races, report.max_depth
    );
    for failure in &report.oracle_failures {
        let _ = writeln!(out, "  ORACLE FAILURE: {failure}");
    }
    out
}

/// The machine-readable report (`--explore` with `--json`).
///
/// Every outcome carries its **whole replay handle**, not a pointer at the
/// human report: `replay` is the copy-pasteable flag, `schedule` is the
/// decision stream in the readable `ev:…` form `--schedule=` takes, and
/// `seed` is the packed 62-bit value when the stream fits one (`null`
/// otherwise). wolf-interp#53: a rig building a replay artifact used to run
/// the exploration twice and pair the human report's "decision stream" line
/// with a seed by adjacency — the stream is the schedule in the form a bug
/// report wants, and a `--json` door that omits it forces text scraping.
#[must_use]
pub fn to_json(file: &str, report: &Report, options: &Options) -> serde_json::Value {
    serde_json::json!({
        "file": file,
        "mode": if options.naive { "naive" } else { "dpor" },
        "executions": report.executions,
        "schedules": report.schedules,
        "slept": report.slept,
        "pruned": report.pruned,
        "deadlocks": report.deadlocks,
        "races": report.races,
        "frontier_open": report.frontier_open,
        "stable": report.stable(),
        "green": report.green(),
        "max_depth": report.max_depth,
        "divergence": report.divergence,
        "oracle_failures": report.oracle_failures,
        "notes": report.notes,
        "outcomes": report.outcomes.iter().map(|o| serde_json::json!({
            "verdict": o.verdict,
            "stdout": String::from_utf8_lossy(&o.stdout),
            "count": o.count,
            "leaks": o.leaks,
            "forest_ok": o.forest_ok,
            "deadlocked": o.deadlocked,
            "preemptions": o.preemptions,
            "replay": o.replay(),
            "schedule": o.stream(),
            "seed": o.seed,
        })).collect::<Vec<_>>(),
    })
}

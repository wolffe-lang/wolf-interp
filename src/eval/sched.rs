//! The sim scheduler — spec/03's dynamic concurrency semantics as a machine.
//!
//! # The design bet, made literal (X12, `[conc.det.flow]`)
//!
//! Every nondeterministic choice this machine ever makes is a **decision at a
//! runtime-owned primitive**, drawn from one seeded generator and logged as a
//! numbered event from the closed `sched-ev/0` taxonomy (`[conc.det.events]`).
//! The same program with the same seed produces a byte-identical trace, twice,
//! forever; `--seed=N` selects the schedule and replays it exactly
//! (`[conc.det.seed]`). This is the foundation is07's explorer permutes: a
//! schedule is exactly a sequence of these decisions, nothing more.
//!
//! # Single-threaded by construction
//!
//! Tasks live on OS threads only because a suspended tree-walk needs a call
//! stack to sleep on — **at most one task ever runs at a time**. The baton is
//! a per-task gate (mutex + condvar): the yielding task picks its successor
//! under the scheduler lock, opens the successor's gate, and parks on its own.
//! There is no parallelism to observe, no data race to have (the crate forbids
//! `unsafe`), and determinism holds because every shared-state mutation happens
//! while holding the baton.
//!
//! # Schedule points, enumerated exhaustively
//!
//! Anything that blocks or chooses goes through this module — the sprint's
//! target 1, which is07 asserts completeness against:
//!
//! 1. **spawn commit** — [`Sched::spawn_task`] (`[conc.task.spawn]`)
//! 2. **scope join** — [`Sched::join_scope`] (`[conc.task.join]`)
//! 3. **channel send/recv pairing** — [`Sched::send`] / [`Sched::recv`]
//!    (`[conc.mm.hb.chan]`, per channel, per k)
//! 4. **`select` arm commit** — [`Sched::select`] (`[conc.select.fair]`)
//! 5. **`when` acquisition** — [`Sched::acquire`] (`[conc.mm.hb.mutex]`)
//! 6. **timer fire** — virtual-clock advance in [`State::advance_clock`]
//!    (`[conc.select.timeout]`)
//! 7. **cancellation delivery** — [`State::cancel_task`]
//!    (`[conc.cancel.points]`: only at the blocking points above)
//! 8. **monitor/exit-signal delivery** — [`State::deliver_exit`]
//!    (`[conc.mm.hb.proc]`)
//!
//! # Virtual time
//!
//! The clock advances **only** when the ready queue is empty and every runnable
//! task is waiting on a timer (the madsim/FDB model): hours of timeout logic
//! run in wall-clock milliseconds, and a timer fire is a schedule decision like
//! any other (`[conc.select.timeout]`).

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

use super::prov::UbFinding;
use super::region::RegionId;
use super::rules::Rule;
use super::value::{ErrorValue, Value};
use crate::eval::Trap;

pub type TaskId = usize;
pub type ScopeId = usize;
pub type ChanId = usize;
pub type MutexId = usize;
pub type ProcId = usize;

// ---------------------------------------------------------------------------
// The decision stream, reified — is07's branch alphabet
// ---------------------------------------------------------------------------

/// Which alphabet a recorded decision draws from (`[conc.det.events]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionKind {
    /// Which ready task runs next; candidates are task ids in queue order.
    Task,
    /// Which simultaneously-ready `select` arm commits (`[conc.select.fair]`);
    /// candidates are arm indices in declaration order.
    Arm,
}

/// One numbered decision from the `sched-ev/0` stream, with the alternatives
/// that were live when it was taken. A schedule is exactly this sequence
/// (`[conc.det.seed]`); the explorer permutes it and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub kind: DecisionKind,
    /// The eligible alternatives, in deterministic queue order.
    pub candidates: Vec<usize>,
    /// Index into `candidates`.
    pub chosen: usize,
    /// FNV-1a-128 of the canonical scheduler-visible state at the decision
    /// point (`Task` decisions; 0 for `Arm`). See [`State::canonical_state`]
    /// for exactly what the hash covers — and what it deliberately cannot.
    pub state: u128,
}

/// A shared object two tasks can contend on — the conflict alphabet the
/// explorer's happens-before analysis reads. Two operations conflict when
/// they name the same key and at least one mutates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjKey {
    /// A channel (`[conc.mm.hb.chan]`): sends, receives, closes, `select`
    /// registrations and monitor deliveries all mutate its queues.
    Chan(ChanId),
    /// A `Mutex`/`when` target (`[conc.mm.hb.mutex]`).
    Mutex(MutexId),
    /// A proc's control state: kill, link, monitor, exit delivery
    /// (`[conc.proc.2]`, `[conc.proc.kill]`).
    Proc(ProcId),
    /// A task's control flags: cancellation and kill requests race the
    /// target's own blocking-point checks (`[conc.cancel.points]`).
    TaskCtl(TaskId),
    /// A scope's join/failure ledger (`[conc.task.join]`, `[conc.task.fail]`).
    Scope(ScopeId),
    /// Shared memory the race detector watches (`[conc.mm.race.1]`).
    Mem(RaceKey),
}

/// One operation on a shared object, recorded at its initiation site with the
/// acting task's vector clock — the event the DPOR happens-before relation is
/// computed over (`[conc.det.dpor]`).
#[derive(Debug, Clone)]
pub struct OpRecord {
    pub task: TaskId,
    /// Index into the decision log of the `Task` decision that scheduled the
    /// slice this op ran in; `None` for `main`'s initial slice (before any
    /// decision existed, so there is no alternative to backtrack to).
    pub slice: Option<usize>,
    pub obj: ObjKey,
    pub write: bool,
    /// The acting task's vector clock at the op.
    pub vc: Vec<u64>,
}

/// Everything the explorer needs from one finished run.
#[derive(Debug, Clone, Default)]
pub struct SchedTrace {
    pub decisions: Vec<Decision>,
    pub ops: Vec<OpRecord>,
    /// The run hit the all-blocked/no-timer verdict (`declare_deadlock`).
    pub deadlocked: bool,
    /// Set when a plan replay observed different alternatives than the run
    /// that produced the plan: a decision escaped the `sched-ev/0` stream —
    /// the completeness violation `[conc.det.flow]` names, a hard error.
    pub divergence: Option<String>,
    /// `state hash → canonical preimage`, kept only under the explorer's
    /// `--paranoid` flag so a hash hit can be re-verified by comparison.
    pub preimages: Vec<(u128, String)>,
}

/// The explorer's replay plan: forced choice indices per decision position,
/// with the alternatives each forced position is expected to present (the
/// completeness assertion), replayed over the deterministic machine.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// Choice index per decision position; positions beyond the end take the
    /// deterministic default (index 0 — strict FIFO).
    pub choices: Vec<usize>,
    /// What each forced position must offer, from the run that produced the
    /// plan. Any mismatch is recorded as `SchedTrace::divergence`.
    pub expected: Vec<(DecisionKind, Vec<usize>)>,
    /// Keep canonical-state preimages beside their hashes (`--paranoid`).
    pub keep_preimages: bool,
    /// Test-only: perturb the ready queue *without* recording a decision —
    /// the deliberately-leaked nondeterminism the red test plants, which the
    /// completeness assertion must catch. Never set outside tests.
    #[doc(hidden)]
    pub leak_nondeterminism: bool,
}

/// The `--seed` namespace split (provisional until the s36 Phase A hook-design
/// doc lands upstream; the compiler runtime's format has priority and this
/// implementation will re-pin to it — see `docs/approximation-contract.md`
/// §10.6). A seed with bit 62 set and bit 63 clear is a **packed schedule**:
/// its low 62 bits are the schedule's choices at multi-candidate decision
/// points, as mixed-radix digits, least significant first, trailing FIFO
/// choices omitted. Every other value seeds the xorshift generator (0 = strict
/// FIFO). Both kinds honor `[proto.seed.equal]`: equal seeds replay the
/// identical decision stream, output bytes included.
pub const PACKED_SEED_TAG: u64 = 1 << 62;
/// The low 62 bits of a packed seed: the mixed-radix digit payload.
pub const PACKED_SEED_MASK: u64 = PACKED_SEED_TAG - 1;

/// Is this seed value a packed schedule rather than a generator seed?
#[must_use]
pub fn seed_is_packed(seed: u64) -> bool {
    seed & PACKED_SEED_TAG != 0 && seed.leading_zeros() >= 1
}

/// Packs a decision stream into a `--seed=N` value, when it fits the 62-bit
/// payload. `Some(0)` is the all-FIFO schedule (seed 0 replays it); `None`
/// means the stream needs the explicit `--schedule=ev:…` spelling.
#[must_use]
pub fn pack_schedule(decisions: &[Decision]) -> Option<u64> {
    let Some(last) = decisions.iter().rposition(|d| d.chosen != 0) else {
        return Some(0);
    };
    let mut value: u128 = 0;
    let mut weight: u128 = 1;
    for decision in &decisions[..=last] {
        let n = decision.candidates.len() as u128;
        if n <= 1 {
            continue;
        }
        value += (decision.chosen as u128) * weight;
        weight = weight.checked_mul(n)?;
        if weight > u128::from(PACKED_SEED_MASK) + 1 {
            return None;
        }
    }
    if value > u128::from(PACKED_SEED_MASK) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)] // checked against the 62-bit mask above
    Some(PACKED_SEED_TAG | (value as u64))
}

/// How the scheduler answers a decision point.
#[derive(Debug)]
enum Chooser {
    /// `--seed=N` generator seeds: 0 = strict FIFO, else xorshift
    /// (`[conc.det.seed]`, `[conc.select.fair]`).
    Seeded { seed: u64, rng: u64 },
    /// A packed schedule seed: mixed-radix digits consumed least significant
    /// first; exhausted digits mean index 0 (FIFO).
    Packed { digits: u64 },
    /// The explorer's forced prefix, with the completeness assertion.
    Plan(Box<Plan>),
}

/// The gate one task parks on. `true` means "you are scheduled: run".
#[derive(Debug, Default)]
struct Gate {
    open: Mutex<bool>,
    cv: Condvar,
}

impl Gate {
    fn unlock(&self) {
        let mut open = self.open.lock().expect("gate lock");
        *open = true;
        self.cv.notify_one();
    }

    fn wait(&self) {
        let mut open = self.open.lock().expect("gate lock");
        while !*open {
            open = self.cv.wait(open).expect("gate wait");
        }
        *open = false;
    }
}

/// Why the scheduler woke a parked task.
#[derive(Debug, Clone, PartialEq)]
pub enum Wake {
    /// The blocked operation completed with this value (a received message,
    /// a granted acquisition — `Unit` for a completed send).
    Ok(Value),
    /// The channel closed under the blocked operation (`[conc.chan.close]`).
    Closed,
    /// Structured cancellation reached this blocking point
    /// (`[conc.cancel.points]`).
    Cancelled,
    /// The task's proc was killed: unwind without running user code
    /// (`[conc.proc.kill]`).
    Killed,
    /// A `select` arm committed (`[conc.select.fair]`): the arm index and the
    /// received value, `None` for a timeout arm.
    Arm(usize, Option<Value>),
    /// Every task is blocked and no timer is pending: a **deadlock**, the
    /// defined outcome `[conc.deadlock.def]` names. Surfaces as
    /// `trap(deadlock)` with the blocked-task roster attached
    /// (`[conc.deadlock.trap]`).
    Deadlock(String),
}

/// How a task's execution ended, as the scheduler records it.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskEnd {
    /// Completed with a non-error value.
    Value(Value),
    /// Completed with an error value — a *failure* for `[conc.task.fail]`
    /// unless the task had been cancelled first.
    Error(Box<ErrorValue>),
    /// Faulted. Inside a proc this is a crash to contain (`[conc.proc.kill]`);
    /// in a plain scope it propagates to the program.
    Trapped(Box<Trap>),
    /// The oracle found UB on this task's execution — always program-fatal.
    Ub(Box<UbFinding>),
    /// Outside coverage — always program-fatal (the record must say so).
    Unsupported(String),
    /// Finished its cancellation (`[conc.task.fail]`: not a failure).
    Cancelled,
    /// Killed with its proc; ran no further user code.
    Killed,
}

/// What a task is doing, from the scheduler's chair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskState {
    /// Created, thread not yet through its first schedule.
    Fresh,
    Ready,
    Running,
    Blocked,
    Done,
}

/// Exit reasons, closed set (`[conc.proc.exit]`).
#[derive(Debug, Clone)]
pub enum ExitReason {
    Normal(Value),
    Error(Box<ErrorValue>),
    Killed,
    Cancelled,
}

impl ExitReason {
    /// The reason as a value (D30: reasons are values, never unwinding):
    /// a structural tag `normal(v)` / `error(e)` / `killed` / `cancelled`,
    /// which `reason.is_killed()`-style predicates and `match` both read.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let (tag, payload) = match self {
            ExitReason::Normal(value) => ("normal", vec![value.clone()]),
            ExitReason::Error(err) => ("error", vec![Value::Error(err.clone())]),
            ExitReason::Killed => ("killed", Vec::new()),
            ExitReason::Cancelled => ("cancelled", Vec::new()),
        };
        Value::Error(Box::new(ErrorValue {
            tag: tag.to_owned(),
            payload,
            enum_variant: false,
            row: Vec::new(),
        }))
    }

    fn label(&self) -> &'static str {
        match self {
            ExitReason::Normal(_) => "normal",
            ExitReason::Error(_) => "error",
            ExitReason::Killed => "killed",
            ExitReason::Cancelled => "cancelled",
        }
    }
}

/// One task control block.
#[derive(Debug)]
struct Tcb {
    name: String,
    state: TaskState,
    /// The failure domain this task belongs to (`[conc.proc.1]`).
    proc: ProcId,
    /// The scope that owns this task, when it was `s.spawn(…)`ed.
    scope: Option<ScopeId>,
    gate: Arc<Gate>,
    /// Structured cancellation requested; delivered at the next blocking
    /// point (`[conc.cancel.points]`).
    cancelled: bool,
    /// Proc kill requested; the task unwinds without user code.
    killed: bool,
    wake: Option<Wake>,
    /// The task's region scope stack, swapped into the store while it runs.
    open_stack: Vec<RegionId>,
    /// This task's vector clock — the happens-before witness `[conc.mm.hb]`
    /// defines and the race detector (`[conc.mm.race.3]`) reads.
    vc: Vec<u64>,
    outcome: Option<TaskEnd>,
}

/// One structured-concurrency scope (`[conc.task.scope]`).
#[derive(Debug, Default)]
struct ScopeState {
    owner: TaskId,
    children: Vec<TaskId>,
    /// The owner is parked at the scope-exit join.
    joining: bool,
    /// Failures in schedule order (`[conc.task.fail]`: the first re-raises,
    /// the rest attach as context).
    failures: Vec<TaskEnd>,
}

/// One typed channel (`[conc.chan]`). Buffered messages carry the sender's
/// vector clock: the k-th send happens-before the k-th receive completes
/// (`[conc.mm.hb.chan]`), and for a region transfer the whole prior graph
/// publishes (`[conc.mm.hb.move]`).
#[derive(Debug)]
struct Chan {
    cap: usize,
    buf: VecDeque<(Value, Vec<u64>)>,
    closed: bool,
    /// Blocked senders in arrival order, with the value they are sending.
    senders: VecDeque<(TaskId, Value)>,
    /// Blocked receivers in arrival order.
    receivers: VecDeque<TaskId>,
    /// Tasks blocked in a `select` with an arm on this channel: `(task, arm)`.
    selecters: Vec<(TaskId, usize)>,
    /// Sends and receives paired so far — the `k` of `[conc.mm.hb.chan]`.
    sends: u64,
    recvs: u64,
}

/// One `Mutex` (`[conc.mm.hb.mutex]`): the n-th release happens-before the
/// n+1-th acquisition, carried by the mutex's vector clock.
#[derive(Debug)]
struct Mx {
    value: Value,
    holder: Option<TaskId>,
    waiters: VecDeque<TaskId>,
    vc: Vec<u64>,
    acquisitions: u64,
}

/// One proc: a failure domain owning its regions (`[conc.proc.1]`).
#[derive(Debug)]
struct Proc {
    name: String,
    root: TaskId,
    /// The proc's own region — allocation inside the proc lands here, and
    /// `[conc.proc.kill]` step 3 bulk-frees it. `None` for proc 0, whose
    /// region is the program region with its own lifecycle.
    region: Option<RegionId>,
    monitors: Vec<ChanId>,
    links: Vec<ProcId>,
    exited: Option<&'static str>,
}

/// A pending virtual timer (`[conc.select.timeout]`).
#[derive(Debug)]
struct Timer {
    deadline: u128,
    seq: u64,
    task: TaskId,
    /// The `select` arm this timer belongs to.
    arm: usize,
}

/// A memory access the race detector remembers (`[conc.mm.race.3]`).
#[derive(Debug)]
struct Access {
    task: TaskId,
    /// The accessor's own clock component at the access.
    tick: u64,
    lo: usize,
    hi: usize,
    write: bool,
}

/// What raced with what, for the trap message.
#[derive(Debug, Clone)]
pub struct RaceReport {
    pub other_task: String,
    pub other_write: bool,
}

/// The kinds of memory two tasks can actually share in this machine.
///
/// Closures capture by value, globals are per-task, region transfer is
/// checked at the send, frozen data is immutable and `Mutex` serializes — so
/// the only mutable memory reachable from two tasks at once is **pool slots
/// in an unmoved region** and **raw allocations** (Tier 3/FFI, exactly the
/// reachability `[conc.mm.race.1]` names). Detection over these two surfaces
/// is therefore *exact* over the interleavings the schedule realizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RaceKey {
    Pool(usize, usize),
    Alloc(usize),
}

/// The shared scheduler. One per program run.
#[derive(Debug)]
pub struct Sched {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    tasks: Vec<Tcb>,
    handles: Vec<Option<std::thread::JoinHandle<()>>>,
    ready: VecDeque<TaskId>,
    scopes: Vec<ScopeState>,
    chans: Vec<Chan>,
    mutexes: Vec<Mx>,
    procs: Vec<Proc>,
    timers: Vec<Timer>,
    timer_seq: u64,
    /// Virtual nanoseconds (`[conc.select.timeout]`).
    clock: u128,
    /// How this run answers decision points (`[conc.det.seed]`).
    chooser: Chooser,
    /// Every decision taken, with its alternatives — the reified
    /// `sched-ev/0` stream the explorer branches over.
    decisions: Vec<Decision>,
    /// Shared-object operations in execution order, for the explorer's
    /// happens-before analysis (`[conc.det.dpor]`).
    ops: Vec<OpRecord>,
    /// The decision index that scheduled the currently-running slice.
    cur_slice: Option<usize>,
    /// The completeness violation, when a plan replay diverged.
    divergence: Option<String>,
    /// `declare_deadlock` fired during this run.
    deadlocked: bool,
    /// Canonical-state preimages beside their hashes (`--paranoid` only).
    preimages: Vec<(u128, String)>,
    /// Rolling FNV of everything printed, folded into the canonical state
    /// hash so schedules that already diverged observably never merge.
    stdout_len: u64,
    stdout_fnv: u64,
    /// Numbered decisions — the `sched-ev/0` stream (`[conc.det.events]`).
    events: u64,
    /// Trace lines pending pickup by the running task's machine.
    notes: Vec<(Rule, String)>,
    /// Race-detector memory (`[conc.mm.race.3]`).
    accesses: std::collections::BTreeMap<RaceKey, Vec<Access>>,
    /// More than one task has ever existed — the race detector's fast path.
    concurrent: bool,
    /// The root supervisor is reaping; scheduling is shutdown's alone.
    shutdown: bool,
}

impl State {
    fn note(&mut self, rule: Rule, text: String) {
        let event = self.events;
        self.events += 1;
        self.notes.push((rule, format!("ev#{event} {text}")));
    }

    /// One scheduling decision: which of the candidates goes next.
    ///
    /// Seed 0 is strict FIFO (`index 0`); any other generator seed draws from
    /// a xorshift generator — pseudo-random, **seeded by the scheduler**,
    /// never wall-clock incidental (`[conc.select.fair]`'s load-bearing
    /// wording). A packed seed consumes its digits; a plan replays its forced
    /// prefix and asserts the alternatives match what the recording run saw
    /// (`[conc.det.flow]` — a mismatch means a decision escaped the stream).
    /// Every decision is recorded with its candidate set, whatever the mode.
    fn decide(&mut self, kind: DecisionKind, candidates: Vec<usize>) -> usize {
        let n = candidates.len();
        let position = self.decisions.len();
        let chosen = match &mut self.chooser {
            Chooser::Seeded { seed, rng } => {
                if *seed == 0 || n <= 1 {
                    0
                } else {
                    let mut x = *rng;
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    *rng = x;
                    (x % n as u64) as usize
                }
            }
            Chooser::Packed { digits } => {
                if n <= 1 {
                    0
                } else {
                    let digit = (*digits % n as u64) as usize;
                    *digits /= n as u64;
                    digit
                }
            }
            Chooser::Plan(plan) => {
                let mut mismatch = None;
                if let Some((want_kind, want)) = plan.expected.get(position)
                    && (*want_kind != kind || *want != candidates)
                {
                    mismatch = Some(format!(
                        "decision #{position}: the recording run saw {want_kind:?} over \
                         {want:?}, this replay sees {kind:?} over {candidates:?} — \
                         nondeterminism outside a registered schedule point \
                         ([conc.det.flow]): a decision escaped the sched-ev/0 stream"
                    ));
                }
                let choice = plan.choices.get(position).copied().unwrap_or(0);
                let chosen = if choice < n {
                    choice
                } else {
                    mismatch.get_or_insert_with(|| {
                        format!(
                            "decision #{position}: the plan forces choice {choice} but only \
                             {n} candidate(s) exist — the replayed stream diverged \
                             ([conc.det.flow])"
                        )
                    });
                    0
                };
                if let Some(mismatch) = mismatch
                    && self.divergence.is_none()
                {
                    self.divergence = Some(mismatch);
                }
                chosen
            }
        };
        let state = if kind == DecisionKind::Task {
            let preimage = self.canonical_state();
            let hash = fnv128(preimage.as_bytes());
            if let Chooser::Plan(plan) = &self.chooser
                && plan.keep_preimages
            {
                self.preimages.push((hash, preimage));
            }
            hash
        } else {
            0
        };
        self.decisions.push(Decision {
            kind,
            candidates,
            chosen,
            state,
        });
        if kind == DecisionKind::Task {
            self.cur_slice = Some(position);
        }
        chosen
    }

    /// Records one shared-object operation for the explorer, attributed to
    /// the decision that scheduled the slice it runs in.
    fn op(&mut self, task: TaskId, obj: ObjKey, write: bool) {
        let vc = self.tasks[task].vc.clone();
        self.ops.push(OpRecord {
            task,
            slice: self.cur_slice,
            obj,
            write,
            vc,
        });
    }

    /// Records the cross-channel footprint of consuming a `select`
    /// registration: committing one arm deregisters `selecter` from *every*
    /// channel of its select, so the acting slice mutates all of those
    /// channels' waiter lists — and the explorer's conflict alphabet must see
    /// the coupling, or a send on a sibling arm's channel looks independent
    /// of this commit and DPOR never tries the reversal (the is07
    /// closed-frontier miss).
    fn op_select_consume(&mut self, selecter: TaskId) {
        let actor = self.actor();
        let registered: Vec<ChanId> = self
            .chans
            .iter()
            .enumerate()
            .filter(|(_, c)| c.selecters.iter().any(|(t, _)| *t == selecter))
            .map(|(id, _)| id)
            .collect();
        for chan in registered {
            self.op(actor, ObjKey::Chan(chan), true);
        }
    }

    /// The task currently holding the baton — for op attribution at doors
    /// that do not carry a `me` parameter (close, kill, link, monitor). The
    /// baton guarantees at most one `Running` task; during a handoff window
    /// the initiator is still the acting task, and task 0 opens the world.
    fn actor(&self) -> TaskId {
        self.tasks
            .iter()
            .position(|t| t.state == TaskState::Running)
            .unwrap_or(0)
    }

    /// The canonical scheduler-visible state, serialized deterministically:
    /// task states + queues (with wakes, clocks and region stacks), channel
    /// contents, mutexes, procs, scopes, timers, the virtual clock, the race
    /// detector's memory and a rolling digest of everything printed.
    ///
    /// What it deliberately cannot cover: suspended interpreter frames (a
    /// task's Rust call stack is not serializable) and the region store's
    /// interior. Two schedules whose difference lives *only* in local
    /// variables could therefore collide; `docs/approximation-contract.md`
    /// §10.6 records the approximation and the explorer's `--paranoid` flag
    /// re-verifies every hash hit against this exact preimage.
    fn canonical_state(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::with_capacity(512);
        let _ = write!(
            s,
            "clock={};ready={:?};out={}:{:016x};",
            self.clock, self.ready, self.stdout_len, self.stdout_fnv
        );
        for (id, t) in self.tasks.iter().enumerate() {
            let _ = write!(
                s,
                "t{id}={:?},c{},k{},w{:?},vc{:?},os{:?};",
                t.state, t.cancelled, t.killed, t.wake, t.vc, t.open_stack
            );
        }
        for (id, c) in self.chans.iter().enumerate() {
            let _ = write!(
                s,
                "ch{id}=cap{},cl{},b{:?},s{:?},r{:?},sel{:?},k{}/{};",
                c.cap, c.closed, c.buf, c.senders, c.receivers, c.selecters, c.sends, c.recvs
            );
        }
        for (id, m) in self.mutexes.iter().enumerate() {
            let _ = write!(
                s,
                "mx{id}={:?},h{:?},w{:?},vc{:?},n{};",
                m.value, m.holder, m.waiters, m.vc, m.acquisitions
            );
        }
        for (id, p) in self.procs.iter().enumerate() {
            let _ = write!(
                s,
                "p{id}={:?},mon{:?},lnk{:?};",
                p.exited, p.monitors, p.links
            );
        }
        for (id, scope) in self.scopes.iter().enumerate() {
            let _ = write!(
                s,
                "sc{id}=o{},ch{:?},j{},f{};",
                scope.owner,
                scope.children,
                scope.joining,
                scope.failures.len()
            );
        }
        for t in &self.timers {
            let _ = write!(s, "tm={},{},{},{};", t.deadline, t.seq, t.task, t.arm);
        }
        for (key, history) in &self.accesses {
            let _ = write!(s, "acc{key:?}=");
            for a in history {
                let _ = write!(s, "({},{},{},{},{})", a.task, a.tick, a.lo, a.hi, a.write);
            }
            s.push(';');
        }
        s
    }

    fn tick(&mut self, task: TaskId) -> u64 {
        let len = self.tasks.len();
        let vc = &mut self.tasks[task].vc;
        vc.resize(len, 0);
        vc[task] += 1;
        vc[task]
    }

    fn merge_vc(into: &mut Vec<u64>, from: &[u64]) {
        if into.len() < from.len() {
            into.resize(from.len(), 0);
        }
        for (slot, value) in into.iter_mut().zip(from) {
            *slot = (*slot).max(*value);
        }
    }

    /// Joins `from`'s clock into `task`'s — a happens-before edge.
    fn hb_edge(&mut self, from: &[u64], task: TaskId) {
        let from = from.to_vec();
        State::merge_vc(&mut self.tasks[task].vc, &from);
        self.tick(task);
    }

    fn task_vc(&self, task: TaskId) -> Vec<u64> {
        self.tasks[task].vc.clone()
    }

    // -- the ready queue and the clock --------------------------------------

    fn make_ready(&mut self, task: TaskId, wake: Wake) {
        let tcb = &mut self.tasks[task];
        debug_assert!(tcb.state != TaskState::Done, "waking done task {task}");
        tcb.wake = Some(wake);
        match tcb.state {
            // The running task's own operation completed synchronously (it
            // registered, then settling paired it immediately): it consumes
            // the wake inline, no scheduling decision happens.
            TaskState::Running => {}
            TaskState::Ready => {}
            TaskState::Blocked | TaskState::Fresh => {
                tcb.state = TaskState::Ready;
                self.ready.push_back(task);
                let name = self.tasks[task].name.clone();
                self.note(Rule::SchedUnpark, format!("unpark `{name}` (task {task})"));
            }
            TaskState::Done => {}
        }
    }

    /// Removes a task from every waiter list — it is being woken by one of
    /// its registrations and must not be woken twice.
    fn deregister(&mut self, task: TaskId) {
        for chan in &mut self.chans {
            chan.receivers.retain(|t| *t != task);
            chan.senders.retain(|(t, _)| *t != task);
            chan.selecters.retain(|(t, _)| *t != task);
        }
        for mx in &mut self.mutexes {
            mx.waiters.retain(|t| *t != task);
        }
        self.timers.retain(|timer| timer.task != task);
    }

    /// Picks the next task to run, advancing virtual time if it must.
    /// `None` means nothing can ever run again.
    fn choose_next(&mut self) -> Option<TaskId> {
        loop {
            if !self.ready.is_empty() {
                if let Chooser::Plan(plan) = &self.chooser
                    && plan.leak_nondeterminism
                    && self.ready.len() > 1
                {
                    // The red test's planted bug: perturb the ready queue
                    // WITHOUT recording a decision. The candidates now differ
                    // from what the recording run saw, and the completeness
                    // assertion in `decide` must catch it.
                    self.ready.rotate_left(1);
                }
                let candidates: Vec<usize> = self.ready.iter().copied().collect();
                let index = self.decide(DecisionKind::Task, candidates);
                let task = self.ready.remove(index).expect("decide() stays in bounds");
                let name = self.tasks[task].name.clone();
                self.note(
                    Rule::SchedDecision,
                    format!(
                        "schedule `{name}` (task {task}), picked {index} of {} ready",
                        self.ready.len() + 1
                    ),
                );
                return Some(task);
            }
            if !self.advance_clock() {
                return None;
            }
        }
    }

    /// Advances the virtual clock to the earliest pending timer and fires
    /// everything due. Returns whether anything fired.
    fn advance_clock(&mut self) -> bool {
        let Some(earliest) = self.timers.iter().map(|t| t.deadline).min() else {
            return false;
        };
        let jump = earliest.saturating_sub(self.clock);
        self.clock = self.clock.max(earliest);
        let clock = self.clock;
        let mut due: Vec<Timer> = Vec::new();
        let mut keep = Vec::new();
        for timer in self.timers.drain(..) {
            if timer.deadline <= clock {
                due.push(timer);
            } else {
                keep.push(timer);
            }
        }
        self.timers = keep;
        // Deterministic fire order: deadline, then registration sequence.
        due.sort_by_key(|t| (t.deadline, t.seq));
        let fired = !due.is_empty();
        self.note(
            Rule::SchedTimer,
            format!(
                "virtual clock advances {jump}ns to {clock}ns; {} timer(s) fire",
                due.len()
            ),
        );
        for timer in due {
            if self.tasks[timer.task].state == TaskState::Blocked {
                let name = self.tasks[timer.task].name.clone();
                self.note(
                    Rule::SchedTimer,
                    format!(
                        "timer for `{name}` (task {}) fires at {clock}ns → select arm {}",
                        timer.task, timer.arm
                    ),
                );
                self.deregister(timer.task);
                self.make_ready(timer.task, Wake::Arm(timer.arm, None));
            }
        }
        fired
    }

    /// Every blocked task's name, for the deadlock report.
    fn blocked_roster(&self) -> String {
        self.tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.state == TaskState::Blocked)
            .map(|(id, t)| format!("`{}` (task {id})", t.name))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// No task can run again: `[conc.deadlock.def]`'s defined outcome, which
    /// a deterministic scheduler detects exactly. Wake everything blocked
    /// with the deadlock verdict so each unwinds into `trap(deadlock)`
    /// (`[conc.deadlock.trap]` — detection is *required* in deterministic
    /// test modes, and this machine is one).
    fn declare_deadlock(&mut self) {
        self.deadlocked = true;
        let roster = self.blocked_roster();
        self.note(
            Rule::DeadlockDef,
            format!("DEADLOCK: every task is blocked and no timer is pending ({roster})"),
        );
        let blocked: Vec<TaskId> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.state == TaskState::Blocked)
            .map(|(id, _)| id)
            .collect();
        for task in blocked {
            self.deregister(task);
            self.make_ready(task, Wake::Deadlock(roster.clone()));
        }
    }

    // -- channels -----------------------------------------------------------

    /// After a state change on `chan`, complete whatever can complete now.
    /// This is where send↔receive pairing decisions happen — each one a
    /// recorded event (`[conc.mm.hb.chan]`).
    fn settle_chan(&mut self, chan: ChanId) {
        loop {
            // Move blocked senders into buffer space.
            while self.chans[chan].buf.len() < self.chans[chan].cap
                && !self.chans[chan].senders.is_empty()
            {
                let (sender, value) = self.chans[chan].senders.pop_front().expect("non-empty");
                let vc = self.task_vc(sender);
                // A send is a release: tick the sender past the published
                // snapshot, or its *later* ops share the released clock and
                // acquirers appear ordered after them too — false
                // happens-before edges that starve DPOR of backtracks
                // (spawn and mutex release already tick; [conc.mm.hb.chan]).
                self.tick(sender);
                self.chans[chan].buf.push_back((value, vc));
                self.chans[chan].sends += 1;
                let k = self.chans[chan].sends;
                let name = self.tasks[sender].name.clone();
                self.note(
                    Rule::SchedChan,
                    format!("channel#{chan} send k={k} completes; `{name}` unblocks"),
                );
                self.make_ready(sender, Wake::Ok(Value::Unit));
            }

            // A buffered (or rendezvous) message meets a receiver.
            let receiver = self.chans[chan].receivers.front().copied();
            let selecter = self.chans[chan].selecters.first().copied();
            let has_buf = !self.chans[chan].buf.is_empty();
            let has_sender = !self.chans[chan].senders.is_empty();
            if !has_buf && !has_sender {
                // Nothing to deliver. A closed, drained channel releases every
                // blocked receiver with the closed error ([conc.chan.close]).
                if self.chans[chan].closed {
                    while let Some(receiver) = self.chans[chan].receivers.pop_front() {
                        self.note(
                            Rule::ChanClose,
                            format!("channel#{chan} drained-closed; receiver unblocks with the closed error"),
                        );
                        self.make_ready(receiver, Wake::Closed);
                    }
                    let selecters = std::mem::take(&mut self.chans[chan].selecters);
                    for (task, arm) in selecters {
                        self.op_select_consume(task);
                        self.deregister(task);
                        self.note(
                            Rule::SelectClosed,
                            format!("channel#{chan} drained-closed; select arm {arm} becomes ready and commits with the closed error"),
                        );
                        self.make_ready(task, Wake::Arm(arm, Some(closed_error())));
                    }
                }
                return;
            }
            let Some(target) = receiver
                .map(|t| (t, None))
                .or(selecter.map(|(t, a)| (t, Some(a))))
            else {
                return;
            };
            let (task, arm) = target;
            if arm.is_some() {
                // The commit consumes the select's registration on every arm
                // channel — record that footprint before deregistering.
                self.op_select_consume(task);
            }
            let (value, sender_vc) = self.take_message(chan);
            self.chans[chan].recvs += 1;
            let k = self.chans[chan].recvs;
            self.hb_edge(&sender_vc, task);
            self.deregister(task);
            let name = self.tasks[task].name.clone();
            match arm {
                None => {
                    self.note(
                        Rule::SchedChan,
                        format!("channel#{chan} recv k={k} pairs; `{name}` receives"),
                    );
                    self.make_ready(task, Wake::Ok(value));
                }
                Some(arm) => {
                    self.note(
                        Rule::SchedSelect,
                        format!("channel#{chan} recv k={k} commits select arm {arm} of `{name}`"),
                    );
                    self.make_ready(task, Wake::Arm(arm, Some(value)));
                }
            }
        }
    }

    /// Takes the next deliverable message: buffer first, else a blocked
    /// sender's rendezvous value (which also completes that send).
    fn take_message(&mut self, chan: ChanId) -> (Value, Vec<u64>) {
        if let Some(message) = self.chans[chan].buf.pop_front() {
            return message;
        }
        let (sender, value) = self.chans[chan]
            .senders
            .pop_front()
            .expect("caller checked a sender exists");
        let vc = self.task_vc(sender);
        // Release discipline, as at the buffered path: the published
        // snapshot must not cover the sender's later ops.
        self.tick(sender);
        self.chans[chan].sends += 1;
        let k = self.chans[chan].sends;
        let name = self.tasks[sender].name.clone();
        self.note(
            Rule::SchedChan,
            format!("channel#{chan} rendezvous k={k}: `{name}`'s send completes with the receive"),
        );
        self.make_ready(sender, Wake::Ok(Value::Unit));
        (value, vc)
    }

    // -- cancellation, kill, exit ------------------------------------------

    /// Requests cooperative cancellation (`[conc.cancel.points]`): delivered
    /// now if the task is parked at a blocking point, at its next one
    /// otherwise. Structured: cancellation flows down into every scope the
    /// cancelled task owns, so its own children finish their cancellation
    /// before its join completes.
    fn cancel_task(&mut self, task: TaskId) {
        if self.tasks[task].state != TaskState::Done {
            // The request races the target's own blocking-point checks: a
            // conflict the explorer must see ([conc.cancel.points]).
            let actor = self.actor();
            self.op(actor, ObjKey::TaskCtl(task), true);
        }
        let tcb = &mut self.tasks[task];
        if tcb.state == TaskState::Done || tcb.cancelled || tcb.killed {
            return;
        }
        tcb.cancelled = true;
        let name = tcb.name.clone();
        let state = tcb.state;
        self.note(
            Rule::CancelPoint,
            format!("cancel `{name}` (task {task}); delivery at a runtime-owned blocking point"),
        );
        if state == TaskState::Blocked {
            self.deregister(task);
            self.make_ready(task, Wake::Cancelled);
        }
        let owned_children: Vec<TaskId> = self
            .scopes
            .iter()
            .filter(|scope| scope.owner == task)
            .flat_map(|scope| scope.children.iter().copied())
            .collect();
        for child in owned_children {
            self.cancel_task(child);
        }
    }

    /// Delivers a proc's exit reason to every monitor (`[conc.mm.hb.proc]`:
    /// the exit happens-before each delivery) and takes linked procs with it
    /// on abnormal exit (`[conc.proc.2]`). Returns the regions to bulk-free —
    /// the *caller* frees them, because this module never touches the store.
    fn deliver_exit(&mut self, proc: ProcId, reason: &ExitReason) -> Vec<RegionId> {
        if self.procs[proc].exited.is_some() {
            return Vec::new();
        }
        let actor = self.actor();
        self.op(actor, ObjKey::Proc(proc), true);
        self.procs[proc].exited = Some(reason.label());
        let name = self.procs[proc].name.clone();
        self.note(
            Rule::ProcExit,
            format!("proc#{proc} `{name}` exits {}", reason.label()),
        );
        let root = self.procs[proc].root;
        let vc = self.task_vc(root);
        let message = Value::Error(Box::new(ErrorValue {
            tag: "exit".to_owned(),
            payload: vec![reason.to_value()],
            enum_variant: false,
            row: Vec::new(),
        }));
        for chan in self.procs[proc].monitors.clone() {
            self.op(actor, ObjKey::Chan(chan), true);
            self.chans[chan]
                .buf
                .push_back((message.clone(), vc.clone()));
            self.chans[chan].sends += 1;
            self.note(
                Rule::ProcMonitor,
                format!(
                    "exit reason `{}` delivered to monitor channel#{chan}",
                    reason.label()
                ),
            );
            self.settle_chan(chan);
        }
        let mut regions = match self.procs[proc].region {
            Some(region) => vec![region],
            None => Vec::new(),
        };
        // Symmetric fate ([conc.proc.2]): an abnormal exit kills every link.
        if !matches!(reason, ExitReason::Normal(_)) {
            for linked in self.procs[proc].links.clone() {
                self.note(
                    Rule::ProcLink,
                    format!("link: proc#{proc}'s abnormal exit kills proc#{linked}"),
                );
                regions.extend(self.kill_proc(linked));
            }
        }
        regions
    }

    /// The killed-proc sequence (`[conc.proc.kill]`), steps 1 and 4; the
    /// returned regions are step 3, freed by the caller. Step 2 (external
    /// frames) is vacuous: this machine has no running-external tasks — a
    /// C call is a host intrinsic that completes synchronously
    /// (`[conc.ffi.external]` is noted, not exercised).
    fn kill_proc(&mut self, proc: ProcId) -> Vec<RegionId> {
        if self.procs[proc].exited.is_some() {
            return Vec::new();
        }
        let actor = self.actor();
        self.op(actor, ObjKey::Proc(proc), true);
        let name = self.procs[proc].name.clone();
        self.note(
            Rule::ProcKill,
            format!(
                "kill proc#{proc} `{name}`: task tree cancelled WITHOUT running user code — \
                 pending defer/errdefer do NOT run; regions bulk-free; reasons deliver"
            ),
        );
        let members: Vec<TaskId> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.proc == proc && t.state != TaskState::Done)
            .map(|(id, _)| id)
            .collect();
        for task in members {
            self.op(actor, ObjKey::TaskCtl(task), true);
            self.tasks[task].killed = true;
            match self.tasks[task].state {
                TaskState::Blocked => {
                    self.deregister(task);
                    self.make_ready(task, Wake::Killed);
                }
                // A pending wake (the task was unblocked but not yet
                // scheduled) is superseded: the kill reaches the task at the
                // schedule point it was already parked at, and NO further
                // user code runs ([conc.proc.kill] step 1).
                TaskState::Ready | TaskState::Fresh => {
                    self.deregister(task);
                    self.tasks[task].wake = Some(Wake::Killed);
                }
                TaskState::Running | TaskState::Done => {}
            }
        }
        self.deliver_exit(proc, &ExitReason::Killed)
    }
}

/// FNV-1a, 128-bit — the canonical-state and outcome digest. Not
/// cryptographic; collisions are what `--paranoid` re-verifies against the
/// stored preimage.
#[must_use]
pub fn fnv128(bytes: &[u8]) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

fn closed_error() -> Value {
    Value::Error(Box::new(ErrorValue {
        tag: "Closed".to_owned(),
        payload: Vec::new(),
        enum_variant: false,
        row: Vec::new(),
    }))
}

fn cancelled_error() -> Value {
    Value::Error(Box::new(ErrorValue {
        tag: "Cancelled".to_owned(),
        payload: Vec::new(),
        enum_variant: false,
        row: Vec::new(),
    }))
}

/// What a blocked channel/mutex operation resolved to, machine-side.
pub enum Resolved {
    Value(Value),
    /// The op returned an error value (closed channel, cancellation).
    Err(Value),
    /// Killed: unwind without user code.
    Killed,
    /// Deadlock: unwind into `trap(deadlock)` carrying the roster
    /// (`[conc.deadlock.trap]`).
    Deadlock(String),
}

impl Sched {
    #[must_use]
    pub fn new(seed: u64) -> Sched {
        Sched::with_chooser(
            Chooser::Seeded {
                seed,
                rng: seed | 1,
            },
            format!(
                "sched-ev/0 stream opens; seed={seed} ({}); root supervisor scope up",
                if seed == 0 {
                    "strict FIFO"
                } else {
                    "seeded pseudo-random"
                }
            ),
        )
    }

    /// A packed-schedule seed (`[proto.seed.equal]` holds: the digits replay
    /// one exact decision stream). `seed` is the full tagged value, for the
    /// trace; the digits are its low 62 bits.
    #[must_use]
    pub fn packed(seed: u64) -> Sched {
        Sched::with_chooser(
            Chooser::Packed {
                digits: seed & PACKED_SEED_MASK,
            },
            format!(
                "sched-ev/0 stream opens; seed={seed} (packed schedule); root supervisor scope up"
            ),
        )
    }

    /// The explorer's door: replay `plan`'s forced prefix, FIFO beyond it,
    /// asserting the alternatives match what the recording run saw.
    #[must_use]
    pub fn planned(plan: Plan) -> Sched {
        let forced = plan.choices.len();
        Sched::with_chooser(
            Chooser::Plan(Box::new(plan)),
            format!(
                "sched-ev/0 stream opens; explorer-forced prefix of {forced} decision(s), \
                 FIFO beyond; root supervisor scope up"
            ),
        )
    }

    fn with_chooser(chooser: Chooser, opening: String) -> Sched {
        let mut state = State {
            tasks: Vec::new(),
            handles: Vec::new(),
            ready: VecDeque::new(),
            scopes: Vec::new(),
            chans: Vec::new(),
            mutexes: Vec::new(),
            procs: Vec::new(),
            timers: Vec::new(),
            timer_seq: 0,
            clock: 0,
            chooser,
            decisions: Vec::new(),
            ops: Vec::new(),
            cur_slice: None,
            divergence: None,
            deadlocked: false,
            preimages: Vec::new(),
            stdout_len: 0,
            stdout_fnv: 0xcbf2_9ce4_8422_2325,
            events: 0,
            notes: Vec::new(),
            accesses: std::collections::BTreeMap::new(),
            concurrent: false,
            shutdown: false,
        };
        // Task 0: `main`, running, under the root supervisor
        // (`[conc.task.root]`): the process runs under a root scope of
        // process lifetime, so daemon-shaped procs are named and enumerable,
        // never detached.
        state.tasks.push(Tcb {
            name: "main".to_owned(),
            state: TaskState::Running,
            proc: 0,
            scope: None,
            gate: Arc::new(Gate::default()),
            cancelled: false,
            killed: false,
            wake: None,
            open_stack: Vec::new(),
            vc: vec![1],
            outcome: None,
        });
        state.handles.push(None);
        state.procs.push(Proc {
            name: "program".to_owned(),
            root: 0,
            region: None,
            monitors: Vec::new(),
            links: Vec::new(),
            exited: None,
        });
        state.note(Rule::SchedSeed, opening);
        Sched {
            state: Mutex::new(state),
        }
    }

    /// Everything the explorer needs from this run: the decision stream with
    /// its alternatives, the op log, and the flags. Call after the run ends.
    #[must_use]
    pub fn take_trace(&self) -> SchedTrace {
        let mut state = self.lock();
        SchedTrace {
            decisions: std::mem::take(&mut state.decisions),
            ops: std::mem::take(&mut state.ops),
            deadlocked: state.deadlocked,
            divergence: state.divergence.take(),
            preimages: std::mem::take(&mut state.preimages),
        }
    }

    /// Folds printed bytes into the canonical-state digest, so schedules that
    /// already diverged observably never merge under state hashing.
    pub fn stdout_mark(&self, bytes: &[u8]) {
        let mut state = self.lock();
        state.stdout_len += bytes.len() as u64;
        let mut hash = state.stdout_fnv;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        state.stdout_fnv = hash;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("scheduler lock")
    }

    /// Trace lines produced since the last drain, in decision order.
    pub fn take_notes(&self) -> Vec<(Rule, String)> {
        std::mem::take(&mut self.lock().notes)
    }

    /// The current virtual time, for messages.
    #[must_use]
    pub fn now(&self) -> u128 {
        self.lock().clock
    }

    /// Has more than one task ever existed? (The race detector's fast path.)
    #[must_use]
    pub fn ever_concurrent(&self) -> bool {
        self.lock().concurrent
    }

    // -- the baton ----------------------------------------------------------

    /// Parks the current — **running** — task until a scheduling decision
    /// picks it again: mark it blocked (or ready, if its wake arrived while
    /// registering), pick a successor, open the successor's gate, and wait on
    /// its own. Only the task holding the baton may call this; a fresh
    /// thread's first wait is [`Sched::park_fresh`], which must not hand off.
    fn park(&self, me: TaskId) -> Wake {
        let gate = {
            let mut state = self.lock();
            debug_assert_eq!(
                state.tasks[me].state,
                TaskState::Running,
                "only the baton holder parks"
            );
            let gate = state.tasks[me].gate.clone();
            if state.tasks[me].wake.is_none() {
                state.tasks[me].state = TaskState::Blocked;
                let name = state.tasks[me].name.clone();
                state.note(Rule::SchedPark, format!("park `{name}` (task {me})"));
            } else {
                // The wake raced in while registering: this park is a
                // plain yield through the ready queue.
                state.tasks[me].state = TaskState::Ready;
                state.ready.push_back(me);
            }
            let next = match state.choose_next() {
                Some(next) => Some(next),
                None => {
                    state.declare_deadlock();
                    state.choose_next()
                }
            };
            if let Some(next) = next {
                state.tasks[next].state = TaskState::Running;
                let next_gate = state.tasks[next].gate.clone();
                drop(state);
                next_gate.unlock();
            }
            gate
        };
        gate.wait();
        let mut state = self.lock();
        state.tasks[me].state = TaskState::Running;
        state.tasks[me]
            .wake
            .take()
            .expect("a scheduled task carries its wake reason")
    }

    /// A fresh task thread's very first act: wait to be scheduled. **No
    /// handoff** — the spawner still holds the baton when the thread starts,
    /// and a scheduling decision may already have picked this task before the
    /// thread gets here; the gate carries that decision either way, and
    /// running the generic [`Sched::park`] from this thread would let two
    /// tasks run at once.
    fn park_fresh(&self, me: TaskId) -> Wake {
        let gate = self.lock().tasks[me].gate.clone();
        gate.wait();
        let mut state = self.lock();
        state.tasks[me].state = TaskState::Running;
        state.tasks[me]
            .wake
            .take()
            .expect("a scheduled task carries its wake reason")
    }

    /// A task's park immediately resolved (its wake was already set when it
    /// tried to block): consume it without a context switch.
    fn immediate(&self, me: TaskId) -> Option<Wake> {
        self.lock().tasks[me].wake.take()
    }

    /// Hands the baton on when a task finishes: schedule the next runnable
    /// thing, or fall through if nothing remains (the program is over or
    /// everyone still alive is parked forever and was woken as deadlocked).
    pub fn dispatch_next(&self) {
        let mut state = self.lock();
        if state.shutdown {
            // Shutdown drives tasks itself, one join at a time; a finishing
            // task must not schedule a second runner beside it.
            return;
        }
        match state.choose_next() {
            Some(next) => {
                state.tasks[next].state = TaskState::Running;
                let gate = state.tasks[next].gate.clone();
                drop(state);
                gate.unlock();
            }
            None => {
                let any_blocked = state.tasks.iter().any(|t| t.state == TaskState::Blocked);
                if any_blocked {
                    state.declare_deadlock();
                    if let Some(next) = state.choose_next() {
                        state.tasks[next].state = TaskState::Running;
                        let gate = state.tasks[next].gate.clone();
                        drop(state);
                        gate.unlock();
                    }
                }
            }
        }
    }

    /// Saves the running task's open stack (called with the store's stack).
    pub fn save_stack(&self, me: TaskId, stack: Vec<RegionId>) {
        self.lock().tasks[me].open_stack = stack;
    }

    /// Restores the task's open stack after it is scheduled again.
    #[must_use]
    pub fn restore_stack(&self, me: TaskId) -> Vec<RegionId> {
        std::mem::take(&mut self.lock().tasks[me].open_stack)
    }

    // -- spawn / scopes -----------------------------------------------------

    /// Opens a scope owned by `owner` (`[conc.task.scope]`).
    pub fn open_scope(&self, owner: TaskId, name: Option<&str>) -> ScopeId {
        let mut state = self.lock();
        let id = state.scopes.len();
        state.scopes.push(ScopeState {
            owner,
            ..ScopeState::default()
        });
        let label = name.unwrap_or("_");
        state.note(
            Rule::TaskScope,
            format!("scope#{id} `{label}` opens (owner task {owner})"),
        );
        id
    }

    /// Commits a spawn (`[conc.task.spawn]`): registers the TCB and returns
    /// the new task's id. The caller starts the OS thread and hands the
    /// join handle back via [`Sched::register_thread`].
    pub fn spawn_task(
        &self,
        parent: TaskId,
        scope: Option<ScopeId>,
        proc: ProcId,
        name: String,
    ) -> TaskId {
        let mut state = self.lock();
        let id = state.tasks.len();
        let parent_vc = state.task_vc(parent);
        state.tick(parent);
        state.tasks.push(Tcb {
            name: name.clone(),
            state: TaskState::Fresh,
            proc,
            scope,
            gate: Arc::new(Gate::default()),
            cancelled: false,
            killed: false,
            wake: None,
            open_stack: Vec::new(),
            vc: parent_vc,
            outcome: None,
        });
        state.handles.push(None);
        state.tick(id);
        state.concurrent = true;
        if let Some(scope) = scope {
            state.scopes[scope].children.push(id);
        }
        state.ready.push_back(id);
        state.tasks[id].state = TaskState::Ready;
        state.tasks[id].wake = Some(Wake::Ok(Value::Unit));
        state.note(
            Rule::SchedSpawn,
            format!(
                "spawn `{name}` (task {id}) under {} in proc#{proc}",
                match scope {
                    Some(scope) => format!("scope#{scope}"),
                    None => "the root supervisor".to_owned(),
                }
            ),
        );
        id
    }

    pub fn register_thread(&self, task: TaskId, handle: std::thread::JoinHandle<()>) {
        self.lock().handles[task] = Some(handle);
    }

    /// Sets the initial region stack a fresh task will swap in.
    pub fn seed_stack(&self, task: TaskId, stack: Vec<RegionId>) {
        self.lock().tasks[task].open_stack = stack;
    }

    /// First park of a fresh task's thread: wait to be scheduled.
    /// Returns `false` if the task was killed before ever running.
    #[must_use]
    pub fn first_schedule(&self, me: TaskId) -> bool {
        let wake = self.park_fresh(me);
        !matches!(wake, Wake::Killed) && !self.lock().tasks[me].killed
    }

    /// The scope-exit join (`[conc.task.join]`): blocks until every child has
    /// completed (or finished its cancellation), then reports the failures in
    /// schedule order (`[conc.task.fail]`).
    pub fn join_scope(&self, me: TaskId, scope: ScopeId) -> Result<Vec<TaskEnd>, Resolved> {
        loop {
            {
                let mut state = self.lock();
                state.op(me, ObjKey::Scope(scope), false);
                let done = state.scopes[scope]
                    .children
                    .iter()
                    .all(|child| state.tasks[*child].state == TaskState::Done);
                if done {
                    state.scopes[scope].joining = false;
                    let children = state.scopes[scope].children.clone();
                    for child in children {
                        let vc = state.task_vc(child);
                        state.hb_edge(&vc, me);
                    }
                    let failures = state.scopes[scope].failures.clone();
                    let count = state.scopes[scope].children.len();
                    state.note(
                        Rule::TaskJoin,
                        format!("scope#{scope} joins: all {count} child(ren) complete"),
                    );
                    return Ok(failures);
                }
                state.scopes[scope].joining = true;
                let name = state.tasks[me].name.clone();
                state.note(
                    Rule::TaskJoin,
                    format!("`{name}` blocks at scope#{scope}'s exit join"),
                );
            }
            match self.park(me) {
                Wake::Ok(_) => {}
                Wake::Killed => return Err(Resolved::Killed),
                Wake::Deadlock(roster) => return Err(Resolved::Deadlock(roster)),
                // A join is a blocking point, but cancellation cannot skip it:
                // structured concurrency's contract is that the scope does not
                // exit before its children. Remember it and keep joining —
                // the children were cancelled with us, so this terminates.
                Wake::Cancelled => {}
                Wake::Closed | Wake::Arm(..) => unreachable!("not a join wake"),
            }
        }
    }

    /// Cancels every live child of a scope — the owner's own teardown path
    /// (its body errored or returned early, before the join).
    pub fn cancel_scope(&self, scope: ScopeId) {
        let mut state = self.lock();
        let children = state.scopes[scope].children.clone();
        for child in children {
            state.cancel_task(child);
        }
    }

    /// A task's execution finished; records the outcome and settles its scope
    /// and proc. Returns the regions to bulk-free when the ending task was a
    /// proc root — the caller frees them and *then* calls
    /// [`Sched::dispatch_next`], so the frees land in the trace before the
    /// next task's steps.
    pub fn end_task(&self, me: TaskId, end: TaskEnd) -> Vec<RegionId> {
        let mut regions = Vec::new();
        {
            let mut state = self.lock();
            state.op(me, ObjKey::TaskCtl(me), false);
            let cancelled = state.tasks[me].cancelled;
            let killed = state.tasks[me].killed;
            // A cancelled task completing with an error finished its
            // cancellation ([conc.task.fail]: not a new failure); a killed
            // one ran no user code at all.
            let end = match end {
                TaskEnd::Error(_) | TaskEnd::Value(_) if killed => TaskEnd::Killed,
                TaskEnd::Error(_) if cancelled => TaskEnd::Cancelled,
                other => other,
            };
            let name = state.tasks[me].name.clone();
            state.note(
                Rule::TaskName,
                format!(
                    "task `{name}` (task {me}) completes: {}",
                    match &end {
                        TaskEnd::Value(_) => "ok".to_owned(),
                        TaskEnd::Error(err) => format!("error `{}`", err.tag),
                        TaskEnd::Trapped(trap) => format!("trap({})", trap.kind),
                        TaskEnd::Ub(finding) => format!("ub({})", finding.anchor()),
                        TaskEnd::Unsupported(_) => "unsupported".to_owned(),
                        TaskEnd::Cancelled => "cancelled".to_owned(),
                        TaskEnd::Killed => "killed".to_owned(),
                    }
                ),
            );
            state.tasks[me].state = TaskState::Done;
            state.tasks[me].outcome = Some(end.clone());
            if let Some(scope) = state.tasks[me].scope {
                // A successful completion commutes with its siblings' (the
                // ledger only orders *failures*, [conc.task.fail]); a failing
                // one writes the ledger and cancels, so it conflicts.
                let failing = matches!(
                    end,
                    TaskEnd::Error(_)
                        | TaskEnd::Trapped(_)
                        | TaskEnd::Ub(_)
                        | TaskEnd::Unsupported(_)
                );
                state.op(me, ObjKey::Scope(scope), failing);
            }

            // Scope accounting ([conc.task.fail]): a failing child cancels
            // its siblings now and re-raises at the scope exit.
            if let Some(scope) = state.tasks[me].scope {
                let failed = matches!(
                    end,
                    TaskEnd::Error(_)
                        | TaskEnd::Trapped(_)
                        | TaskEnd::Ub(_)
                        | TaskEnd::Unsupported(_)
                );
                if failed {
                    state.scopes[scope].failures.push(end.clone());
                    let siblings: Vec<TaskId> = state.scopes[scope]
                        .children
                        .iter()
                        .copied()
                        .filter(|sibling| *sibling != me)
                        .collect();
                    let name = state.tasks[me].name.clone();
                    state.note(
                        Rule::TaskFail,
                        format!(
                            "`{name}` failed: cancelling {} sibling(s); the failure re-raises at \
                             the scope exit",
                            siblings.len()
                        ),
                    );
                    for sibling in siblings {
                        state.cancel_task(sibling);
                    }
                    // `[conc.task.fail.owner]`: the scope is the cancellation
                    // unit, owner included. An owner blocked at a blocking
                    // point it entered inside the scope's extent (it runs the
                    // scope body, so any non-join block while the scope is
                    // live qualifies) is cancelled exactly like a sibling. An
                    // owner blocked at the *join* keeps joining — structured
                    // concurrency's invariant outranks prompt cancellation,
                    // and the children were cancelled with it.
                    let owner = state.scopes[scope].owner;
                    if !state.scopes[scope].joining
                        && state.tasks[owner].state == TaskState::Blocked
                    {
                        let owner_name = state.tasks[owner].name.clone();
                        state.note(
                            Rule::TaskFailOwner,
                            format!(
                                "the failure's cancellation reaches the blocked scope owner \
                                 `{owner_name}` (task {owner}) too; the failure still re-raises \
                                 at the scope exit, after the join"
                            ),
                        );
                        state.cancel_task(owner);
                    }
                }
                // Wake the joining owner if everything is done now.
                let owner = state.scopes[scope].owner;
                let all_done = state.scopes[scope]
                    .children
                    .iter()
                    .all(|child| state.tasks[*child].state == TaskState::Done);
                if state.scopes[scope].joining
                    && all_done
                    && state.tasks[owner].state == TaskState::Blocked
                {
                    state.make_ready(owner, Wake::Ok(Value::Unit));
                }
            }

            // Proc accounting: a proc's root task ending is the proc's exit
            // ([conc.proc.exit]); a fault is a *crash*, contained by the
            // failure domain ([conc.proc.1]) — its regions bulk-free and the
            // reason delivers, but the program does not fall over.
            let proc = state.tasks[me].proc;
            if state.procs[proc].root == me && proc != 0 {
                let reason = match &end {
                    TaskEnd::Value(value) => ExitReason::Normal(value.clone()),
                    TaskEnd::Error(err) => ExitReason::Error(err.clone()),
                    TaskEnd::Trapped(trap) => ExitReason::Error(Box::new(ErrorValue {
                        tag: "trap".to_owned(),
                        payload: vec![Value::Str(trap.kind.to_string())],
                        enum_variant: false,
                        row: Vec::new(),
                    })),
                    TaskEnd::Ub(finding) => ExitReason::Error(Box::new(ErrorValue {
                        tag: "ub".to_owned(),
                        payload: vec![Value::Str(finding.anchor().to_owned())],
                        enum_variant: false,
                        row: Vec::new(),
                    })),
                    TaskEnd::Unsupported(reason) => ExitReason::Error(Box::new(ErrorValue {
                        tag: "unsupported".to_owned(),
                        payload: vec![Value::Str(reason.clone())],
                        enum_variant: false,
                        row: Vec::new(),
                    })),
                    TaskEnd::Cancelled => ExitReason::Cancelled,
                    TaskEnd::Killed => ExitReason::Killed,
                };
                regions = state.deliver_exit(proc, &reason);
            }
        }
        regions
    }

    // -- channels -----------------------------------------------------------

    pub fn new_chan(&self, cap: usize) -> ChanId {
        let mut state = self.lock();
        let id = state.chans.len();
        state.chans.push(Chan {
            cap,
            buf: VecDeque::new(),
            closed: false,
            senders: VecDeque::new(),
            receivers: VecDeque::new(),
            selecters: Vec::new(),
            sends: 0,
            recvs: 0,
        });
        state.note(
            Rule::ChanBuf,
            format!(
                "channel#{id} created, {}",
                if cap == 0 {
                    "rendezvous (cap 0)".to_owned()
                } else if cap == usize::MAX {
                    "unbounded (monitor mailbox)".to_owned()
                } else {
                    format!("bounded cap {cap}")
                }
            ),
        );
        id
    }

    /// Sends on a channel. Blocks on a full buffer or an unmet rendezvous —
    /// a cancellation point and a recorded event (`[conc.chan.buf]`).
    pub fn send(&self, me: TaskId, chan: ChanId, value: Value) -> Resolved {
        {
            let mut state = self.lock();
            if state.tasks[me].killed {
                return Resolved::Killed;
            }
            if state.tasks[me].cancelled {
                state.note(
                    Rule::CancelPoint,
                    format!("cancellation delivered to task {me} at channel#{chan} send"),
                );
                return Resolved::Err(cancelled_error());
            }
            state.op(me, ObjKey::TaskCtl(me), false);
            state.op(me, ObjKey::Chan(chan), true);
            if state.chans[chan].closed {
                // Never UB, never a fault: an error value ([conc.chan.close]).
                state.note(
                    Rule::ChanClose,
                    format!("send on closed channel#{chan} returns the closed error"),
                );
                return Resolved::Err(closed_error());
            }
            state.chans[chan].senders.push_back((me, value));
            state.settle_chan(chan);
            if state.tasks[me].wake.is_some() {
                // The send completed synchronously (buffer space or a waiting
                // receiver); consume the wake without a context switch.
                drop(state);
                return match self.immediate(me).expect("wake set") {
                    Wake::Ok(value) => Resolved::Value(value),
                    Wake::Closed => Resolved::Err(closed_error()),
                    Wake::Cancelled => Resolved::Err(cancelled_error()),
                    Wake::Killed => Resolved::Killed,
                    Wake::Deadlock(roster) => Resolved::Deadlock(roster),
                    Wake::Arm(..) => unreachable!("not a select"),
                };
            }
            let name = state.tasks[me].name.clone();
            state.note(
                Rule::ChanBuf,
                format!("`{name}` blocks sending on channel#{chan} (full or rendezvous)"),
            );
        }
        match self.park(me) {
            Wake::Ok(value) => Resolved::Value(value),
            Wake::Closed => Resolved::Err(closed_error()),
            Wake::Cancelled => Resolved::Err(cancelled_error()),
            Wake::Killed => Resolved::Killed,
            Wake::Deadlock(roster) => Resolved::Deadlock(roster),
            Wake::Arm(..) => unreachable!("not a select"),
        }
    }

    /// Receives from a channel; blocks on empty (`[conc.chan.buf]`).
    pub fn recv(&self, me: TaskId, chan: ChanId) -> Resolved {
        {
            let mut state = self.lock();
            if state.tasks[me].killed {
                return Resolved::Killed;
            }
            if state.tasks[me].cancelled {
                state.note(
                    Rule::CancelPoint,
                    format!("cancellation delivered to task {me} at channel#{chan} recv"),
                );
                return Resolved::Err(cancelled_error());
            }
            state.op(me, ObjKey::TaskCtl(me), false);
            state.op(me, ObjKey::Chan(chan), true);
            state.chans[chan].receivers.push_back(me);
            state.settle_chan(chan);
            if state.tasks[me].wake.is_some() {
                drop(state);
                return match self.immediate(me).expect("wake set") {
                    Wake::Ok(value) => Resolved::Value(value),
                    Wake::Closed => Resolved::Err(closed_error()),
                    Wake::Cancelled => Resolved::Err(cancelled_error()),
                    Wake::Killed => Resolved::Killed,
                    Wake::Deadlock(roster) => Resolved::Deadlock(roster),
                    Wake::Arm(..) => unreachable!("not a select"),
                };
            }
            let name = state.tasks[me].name.clone();
            state.note(
                Rule::ChanBuf,
                format!("`{name}` blocks receiving on channel#{chan} (empty)"),
            );
        }
        match self.park(me) {
            Wake::Ok(value) => Resolved::Value(value),
            Wake::Closed => Resolved::Err(closed_error()),
            Wake::Cancelled => Resolved::Err(cancelled_error()),
            Wake::Killed => Resolved::Killed,
            Wake::Deadlock(roster) => Resolved::Deadlock(roster),
            Wake::Arm(..) => unreachable!("not a select"),
        }
    }

    /// Closes a channel (`[conc.chan.close]`): buffered items drain, blocked
    /// receivers get the closed error, further sends return it.
    pub fn close(&self, chan: ChanId) {
        let mut state = self.lock();
        if state.chans[chan].closed {
            return;
        }
        let actor = state.actor();
        state.op(actor, ObjKey::Chan(chan), true);
        state.chans[chan].closed = true;
        state.note(Rule::ChanClose, format!("channel#{chan} closes"));
        // Blocked senders can no longer complete: closed error for each.
        let senders = std::mem::take(&mut state.chans[chan].senders);
        for (sender, _) in senders {
            state.note(
                Rule::ChanClose,
                format!("blocked send on channel#{chan} returns the closed error"),
            );
            state.make_ready(sender, Wake::Closed);
        }
        state.settle_chan(chan);
    }

    /// One `select` (`[conc.select.ready]`): readiness first; among the
    /// simultaneously ready the seeded choice commits (`[conc.select.fair]`,
    /// a recorded decision with the ready set in the record); with none
    /// ready, block on every arm — a cancellation point.
    ///
    /// `arms` maps arm index → channel; `timeout` is `(arm, deadline_ns)`.
    pub fn select(
        &self,
        me: TaskId,
        arms: &[(usize, ChanId)],
        timeout: Option<(usize, u128)>,
    ) -> Result<(usize, Option<Value>), Resolved> {
        {
            let mut state = self.lock();
            if state.tasks[me].killed {
                return Err(Resolved::Killed);
            }
            if state.tasks[me].cancelled {
                state.note(
                    Rule::CancelPoint,
                    format!("cancellation delivered to task {me} at `select`"),
                );
                return Err(Resolved::Err(cancelled_error()));
            }
            state.op(me, ObjKey::TaskCtl(me), false);
            for (_, chan) in arms {
                state.op(me, ObjKey::Chan(*chan), true);
            }
            // Readiness: a recv arm is ready when a message is deliverable
            // now, or the channel is drained-closed (the closed error is the
            // delivery). A timeout arm is ready when its deadline has passed.
            let mut ready: Vec<usize> = Vec::new();
            for (arm, chan) in arms {
                let c = &state.chans[*chan];
                if !c.buf.is_empty() || !c.senders.is_empty() || c.closed {
                    ready.push(*arm);
                }
            }
            if let Some((arm, deadline)) = timeout
                && state.clock >= deadline
            {
                ready.push(arm);
            }
            if !ready.is_empty() {
                let pick = state.decide(DecisionKind::Arm, ready.clone());
                let arm = ready[pick];
                state.note(
                    Rule::SchedSelect,
                    format!(
                        "select in task {me}: ready set {ready:?}, seeded choice commits arm {arm}"
                    ),
                );
                if let Some((t_arm, _)) = timeout
                    && t_arm == arm
                {
                    return Ok((arm, None));
                }
                let chan = arms
                    .iter()
                    .find(|(a, _)| *a == arm)
                    .map(|(_, c)| *c)
                    .expect("a ready recv arm names its channel");
                if state.chans[chan].buf.is_empty() && state.chans[chan].senders.is_empty() {
                    // Ready because drained-closed ([conc.select.closed]):
                    // the closed error value is the delivery.
                    state.note(
                        Rule::SelectClosed,
                        format!(
                            "select arm {arm} ready because channel#{chan} is drained-closed; \
                             the arm receives the closed error value"
                        ),
                    );
                    return Ok((arm, Some(closed_error())));
                }
                let (value, vc) = state.take_message(chan);
                state.chans[chan].recvs += 1;
                let k = state.chans[chan].recvs;
                state.hb_edge(&vc, me);
                state.note(
                    Rule::SchedChan,
                    format!("channel#{chan} recv k={k} pairs inside the committed arm"),
                );
                return Ok((arm, Some(value)));
            }
            // Nothing ready: register on every arm and the timer, then park.
            for (arm, chan) in arms {
                state.chans[*chan].selecters.push((me, *arm));
            }
            if let Some((arm, deadline)) = timeout {
                let seq = state.timer_seq;
                state.timer_seq += 1;
                state.timers.push(Timer {
                    deadline,
                    seq,
                    task: me,
                    arm,
                });
                state.note(
                    Rule::SchedTimer,
                    format!("timer set for select arm {arm} at {deadline}ns (virtual)"),
                );
            }
            let name = state.tasks[me].name.clone();
            state.note(
                Rule::SchedSelect,
                format!("`{name}` blocks in `select`: no arm ready"),
            );
        }
        match self.park(me) {
            Wake::Arm(arm, value) => Ok((arm, value)),
            Wake::Cancelled => Err(Resolved::Err(cancelled_error())),
            Wake::Killed => Err(Resolved::Killed),
            Wake::Deadlock(roster) => Err(Resolved::Deadlock(roster)),
            Wake::Ok(_) | Wake::Closed => unreachable!("select wakes carry their arm"),
        }
    }

    // -- mutexes / `when` ---------------------------------------------------

    pub fn new_mutex(&self, value: Value) -> MutexId {
        let mut state = self.lock();
        let id = state.mutexes.len();
        state.mutexes.push(Mx {
            value,
            holder: None,
            waiters: VecDeque::new(),
            vc: Vec::new(),
            acquisitions: 0,
        });
        id
    }

    /// Acquires one mutex of a `when` set — always in canonical order, so the
    /// set acquisition is deadlock-free by construction. Blocking here is a
    /// cancellation point (`[conc.cancel.points]`). Returns the payload.
    pub fn acquire(&self, me: TaskId, mx: MutexId) -> Resolved {
        {
            let mut state = self.lock();
            if state.tasks[me].killed {
                return Resolved::Killed;
            }
            if state.tasks[me].cancelled {
                state.note(
                    Rule::CancelPoint,
                    format!("cancellation delivered to task {me} at mutex#{mx} acquisition"),
                );
                return Resolved::Err(cancelled_error());
            }
            state.op(me, ObjKey::TaskCtl(me), false);
            state.op(me, ObjKey::Mutex(mx), true);
            if state.mutexes[mx].holder.is_none() {
                state.mutexes[mx].holder = Some(me);
                state.mutexes[mx].acquisitions += 1;
                let n = state.mutexes[mx].acquisitions;
                let vc = state.mutexes[mx].vc.clone();
                state.hb_edge(&vc, me);
                state.note(
                    Rule::SchedAcquire,
                    format!("mutex#{mx} acquired by task {me} (n={n})"),
                );
                let value = state.mutexes[mx].value.clone();
                return Resolved::Value(value);
            }
            state.mutexes[mx].waiters.push_back(me);
            let name = state.tasks[me].name.clone();
            state.note(
                Rule::SchedAcquire,
                format!("`{name}` blocks acquiring mutex#{mx}"),
            );
        }
        match self.park(me) {
            Wake::Ok(value) => Resolved::Value(value),
            Wake::Cancelled => Resolved::Err(cancelled_error()),
            Wake::Killed => Resolved::Killed,
            Wake::Deadlock(roster) => Resolved::Deadlock(roster),
            Wake::Closed | Wake::Arm(..) => unreachable!("not a mutex wake"),
        }
    }

    /// Releases a mutex, writing the (possibly updated) payload back. The
    /// release happens-before the next acquisition (`[conc.mm.hb.mutex]`).
    pub fn release(&self, me: TaskId, mx: MutexId, value: Value) {
        let mut state = self.lock();
        debug_assert_eq!(state.mutexes[mx].holder, Some(me));
        state.op(me, ObjKey::Mutex(mx), true);
        state.mutexes[mx].value = value;
        state.mutexes[mx].holder = None;
        let vc = state.task_vc(me);
        State::merge_vc(&mut state.mutexes[mx].vc, &vc);
        state.tick(me);
        state.note(
            Rule::SchedAcquire,
            format!("mutex#{mx} released by task {me}"),
        );
        if let Some(next) = state.mutexes[mx].waiters.pop_front() {
            state.mutexes[mx].holder = Some(next);
            state.mutexes[mx].acquisitions += 1;
            let n = state.mutexes[mx].acquisitions;
            let vc = state.mutexes[mx].vc.clone();
            state.hb_edge(&vc, next);
            state.note(
                Rule::SchedAcquire,
                format!("mutex#{mx} granted to waiting task {next} (n={n})"),
            );
            let value = state.mutexes[mx].value.clone();
            state.make_ready(next, Wake::Ok(value));
        }
    }

    // -- procs --------------------------------------------------------------

    /// Registers a proc under the root supervisor (`[conc.task.root]`,
    /// `[conc.proc.1]`). Returns `(proc id, root task id)`.
    pub fn spawn_proc(&self, parent: TaskId, name: String, region: RegionId) -> (ProcId, TaskId) {
        let proc = {
            let mut state = self.lock();
            let id = state.procs.len();
            state.procs.push(Proc {
                name: name.clone(),
                root: 0, // patched below
                region: Some(region),
                monitors: Vec::new(),
                links: Vec::new(),
                exited: None,
            });
            state.op(parent, ObjKey::Proc(id), true);
            state.note(
                Rule::ProcModel,
                format!(
                    "proc#{id} `{name}` spawns under the root supervisor, owning region #{region}; \
                     its only cross-proc surface is typed channels"
                ),
            );
            id
        };
        let root = self.spawn_task(parent, None, proc, format!("proc:{name}"));
        self.lock().procs[proc].root = root;
        (proc, root)
    }

    /// `w.monitor()` — an unbounded mailbox the exit reason will arrive on
    /// (`[conc.proc.2]`; asynchronous, reason as a typed value). Monitoring
    /// an already-exited proc delivers immediately.
    pub fn monitor(&self, proc: ProcId) -> ChanId {
        let chan = self.new_chan(usize::MAX);
        let mut state = self.lock();
        let actor = state.actor();
        state.op(actor, ObjKey::Proc(proc), true);
        state.procs[proc].monitors.push(chan);
        let name = state.procs[proc].name.clone();
        state.note(
            Rule::ProcMonitor,
            format!("monitor on proc#{proc} `{name}` → channel#{chan}"),
        );
        if let Some(label) = state.procs[proc].exited {
            let reason = match label {
                "killed" => ExitReason::Killed,
                "cancelled" => ExitReason::Cancelled,
                _ => ExitReason::Normal(Value::Unit),
            };
            let message = Value::Error(Box::new(ErrorValue {
                tag: "exit".to_owned(),
                payload: vec![reason.to_value()],
                enum_variant: false,
                row: Vec::new(),
            }));
            let root = state.procs[proc].root;
            let vc = state.task_vc(root);
            state.chans[chan].buf.push_back((message, vc));
            state.chans[chan].sends += 1;
            state.note(
                Rule::ProcMonitor,
                format!("proc#{proc} already exited {label}; the reason delivers immediately"),
            );
        }
        chan
    }

    /// `a.link(b)`-style symmetric fate coupling (`[conc.proc.2]`).
    pub fn link(&self, a: ProcId, b: ProcId) {
        let mut state = self.lock();
        let actor = state.actor();
        state.op(actor, ObjKey::Proc(a), true);
        state.op(actor, ObjKey::Proc(b), true);
        if !state.procs[a].links.contains(&b) {
            state.procs[a].links.push(b);
        }
        if !state.procs[b].links.contains(&a) {
            state.procs[b].links.push(a);
        }
        state.note(
            Rule::ProcLink,
            format!("link proc#{a} ⇄ proc#{b}: either side's abnormal exit kills the other"),
        );
    }

    /// `w.kill()` (`[conc.proc.kill]`). Returns the regions to bulk-free.
    pub fn kill(&self, proc: ProcId) -> Vec<RegionId> {
        self.lock().kill_proc(proc)
    }

    /// `w.cancel()` (`[conc.proc.cancel]`): structured cancellation delivered
    /// to the proc's task tree, cooperatively, at `[conc.cancel.points]`
    /// blocking points — defers run (`[conc.cancel.defer]`; contrast
    /// `[conc.proc.kill]`, which skips them). A proc that exits *because*
    /// this reached it exits `cancelled`; one that completes its value
    /// despite it keeps `normal(value)` — both fall out of the existing
    /// end-of-task accounting.
    pub fn cancel_proc(&self, proc: ProcId) {
        let mut state = self.lock();
        let actor = state.actor();
        state.op(actor, ObjKey::Proc(proc), true);
        let name = state.procs[proc].name.clone();
        state.note(
            Rule::ProcCancel,
            format!(
                "cancel proc#{proc} `{name}`: structured cancellation, cooperative at blocking \
                 points; defers run"
            ),
        );
        let members: Vec<TaskId> = state
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.proc == proc && t.state != TaskState::Done)
            .map(|(id, _)| id)
            .collect();
        for task in members {
            state.cancel_task(task);
        }
    }

    /// The proc a task belongs to.
    #[must_use]
    pub fn proc_of(&self, task: TaskId) -> ProcId {
        self.lock().tasks[task].proc
    }

    /// The REPL's `:schedule seed` (is08): re-seed the decision generator for
    /// every decision from here on. The world (tasks, channels, clocks) is
    /// untouched — only future `sched-ev/0` choices change — and the reseed
    /// is itself a recorded note so transcripts stay honest about it.
    pub fn reseed(&self, seed: u64) {
        let mut state = self.lock();
        state.chooser = Chooser::Seeded {
            seed,
            rng: seed | 1,
        };
        state.note(
            Rule::SchedSeed,
            format!(
                "sched-ev/0 generator re-seeded; seed={seed} ({}) from here on",
                if seed == 0 {
                    "strict FIFO"
                } else {
                    "seeded pseudo-random"
                }
            ),
        );
    }

    // -- the race detector (`[conc.mm.race.3]`) -----------------------------

    /// Checks one access against the happens-before order and remembers it.
    /// Returns the conflicting prior access when the two are unordered — a
    /// data race, exactly at the interleaving the schedule realized.
    pub fn race_check(
        &self,
        me: TaskId,
        key: RaceKey,
        lo: usize,
        hi: usize,
        write: bool,
    ) -> Option<RaceReport> {
        let mut state = self.lock();
        if !state.concurrent {
            return None;
        }
        state.op(me, ObjKey::Mem(key), write);
        let my_vc = state.task_vc(me);
        let found = state.accesses.get(&key).and_then(|history| {
            history
                .iter()
                .find(|prior| {
                    prior.task != me
                        && (prior.write || write)
                        && prior.lo < hi
                        && prior.hi > lo
                        && my_vc.get(prior.task).copied().unwrap_or(0) < prior.tick
                })
                .map(|prior| (prior.task, prior.write))
        });
        if let Some((other, other_write)) = found {
            let report = RaceReport {
                other_task: format!("`{}` (task {other})", state.tasks[other].name),
                other_write,
            };
            state.note(
                Rule::RaceDetect,
                format!("RACE on {key:?}: tasks {me} and {other} conflict unordered"),
            );
            return Some(report);
        }
        let tick = state.tasks[me].vc.get(me).copied().unwrap_or(0);
        state.accesses.entry(key).or_default().push(Access {
            task: me,
            tick,
            lo,
            hi,
            write,
        });
        None
    }

    // -- shutdown -----------------------------------------------------------

    /// Ends the world: kills every live proc and task (daemons under the root
    /// supervisor die here — `[conc.task.root]` makes them enumerable, and
    /// this is the enumeration), wakes each in a deterministic order, joins
    /// their threads one at a time, and returns the proc regions to free.
    pub fn shutdown(&self) -> Vec<RegionId> {
        let mut regions = Vec::new();
        self.lock().shutdown = true;
        loop {
            let handle = {
                let mut state = self.lock();
                let found = state
                    .tasks
                    .iter()
                    .enumerate()
                    .find(|(id, t)| *id != 0 && t.state != TaskState::Done)
                    .map(|(id, t)| (id, t.name.clone(), t.proc));
                let Some((id, name, proc)) = found else {
                    break;
                };
                state.note(
                    Rule::TaskRoot,
                    format!("shutdown: root supervisor reaps `{name}` (task {id}, proc#{proc})"),
                );
                if proc != 0 && state.procs[proc].exited.is_none() {
                    regions.extend(state.kill_proc(proc));
                }
                let tcb = &mut state.tasks[id];
                tcb.killed = true;
                if tcb.wake.is_none() {
                    tcb.wake = Some(Wake::Killed);
                }
                // Do not go through the ready queue: shutdown runs tasks one
                // at a time itself, joining each thread before the next, so
                // the unwind order (and the trace it produces) stays
                // deterministic.
                tcb.state = TaskState::Running;
                state.ready.retain(|t| *t != id);
                let gate = state.tasks[id].gate.clone();
                let handle = state.handles[id].take();
                drop(state);
                gate.unlock();
                handle
            };
            if let Some(handle) = handle {
                let _ = handle.join();
            }
        }
        regions
    }

    /// Joins every remaining task thread (all must be Done). Called once at
    /// the very end of the run.
    pub fn join_all(&self) {
        loop {
            let handle = self.lock().handles.iter_mut().find_map(Option::take);
            match handle {
                Some(handle) => {
                    let _ = handle.join();
                }
                None => break,
            }
        }
    }

    /// The structured dump's contents (`[conc.task.name]`): every task and
    /// proc, named. The dump's *existence* is contract; its contents are
    /// implementation-specified.
    #[must_use]
    pub fn dump(&self) -> Vec<String> {
        let state = self.lock();
        let mut out = Vec::new();
        for (id, proc) in state.procs.iter().enumerate() {
            out.push(format!(
                "proc#{id} `{}` root=task{} state={}",
                proc.name,
                proc.root,
                proc.exited.unwrap_or("live")
            ));
        }
        for (id, task) in state.tasks.iter().enumerate() {
            out.push(format!("task#{id} `{}` {:?}", task.name, task.state));
        }
        out
    }

    /// How many scheduling events have been recorded.
    #[must_use]
    pub fn events(&self) -> u64 {
        self.lock().events
    }
}

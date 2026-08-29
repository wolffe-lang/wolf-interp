//! The tree-walking evaluator — `spec/02-memory-model.md` §2 as a machine.
//!
//! # What this is
//!
//! Every ownership rule in `[mem.tier0]` is enforced here as a **dynamic
//! check**, and every fault cites the clause it enforces ([`rules::Rule`]).
//! The compiler proves these properties statically and rejects; this machine
//! observes them at run time and traps. The approximation direction is
//! one-way and stated in the sprint: *the compiler accepts ⇒ the interpreter
//! must not fault; never the converse.* A program the compiler rejects that
//! runs clean here is static conservatism, not a bug.
//!
//! # There is no unwinding
//!
//! D30 is load-bearing: `!T` is a tagged value, `?` is a return, `else` is a
//! branch, `errdefer` is a scope-exit list consulted on one of two paths. The
//! Rust-level [`Signal`] type carries `return`/`break`/`continue`/trap out of a
//! walk, and *that* is not unwinding either — it is a `Result` on the way back
//! up. A Rust `panic!` anywhere in this module is by definition an interpreter
//! bug; `tests/fuzz_smoke.rs` treats it as one.
//!
//! # Deliberately not clever
//!
//! No caching, no bytecode, no interning, per the sprint's non-targets: "a
//! tree-walk with per-access state checks is the design, not a compromise".

pub mod builtin;
mod conc;
/// The s39 net family over std::net (no sockets on wasm — the builtin
/// arms decline there, like the time tier).
#[cfg(not(target_family = "wasm"))]
mod net;
/// The s40 process trio's child table (no `std::process` on wasm — the
/// builtin arms decline there, like the time tier).
#[cfg(not(target_family = "wasm"))]
mod os;
pub mod place;
pub mod prov;
pub mod region;
pub mod repl;
pub mod rules;
pub mod sched;
pub mod value;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::ast::{
    Arg, AssignOp, BinOp, Binding, Block, ClosureParam, ElseHandler, Expr, ExprKind, FnDecl,
    IndexArg, Interpolation, Member, ParamKind, ParamMode, PatKind, Pattern, RegionStrategy, Stmt,
    StmtKind, StrPart, Type, TypeArg, TypeKind, UnOp,
};
use crate::diag::Span;
use crate::sema::{Def, Program};
use crate::trap::TrapKind;

use place::{Access, AccessSet, Held, HeldWhy, Path, Proj};
use prov::{AccessKind, Prov, Provenance, RawPtr, RetagKind, UbFinding, UbRow};
use region::{Edge, Ref, RegionId, RegionState, Store, Strategy};
use rules::Rule;
use value::{
    ArithMode, CaptureLoan, ClosureValue, ErrorValue, HandleValue, IntTy, RegionValue, Slot,
    SlotState, Value,
};

/// A trap: a fault of a *defined* execution, named by the closed vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trap {
    pub kind: TrapKind,
    /// The rule that fired, and through it the clause anchor.
    pub rule: Rule,
    /// Where the fault happened.
    pub span: Span,
    /// The second span the sprint asks every ownership fault to print — the
    /// move site for `use-after-move`, the conflicting access for
    /// `exclusivity`.
    pub secondary: Option<(Span, String)>,
    pub message: String,
}

impl std::fmt::Display for Trap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "trap({}): {} [{}] at {}",
            self.kind,
            self.message,
            self.rule.anchor(),
            self.span
        )?;
        if let Some((span, note)) = &self.secondary {
            write!(f, "; {note} at {span}")?;
        }
        Ok(())
    }
}

impl Trap {
    /// The human trap line for a caller holding the source: the
    /// [`std::fmt::Display`] grammar with every location spelled `line:col`
    /// (1-based, character columns — the repo's one line:col spelling,
    /// `tests/fault_snapshots.rs` first). `[conf.trap.render]`: the reference
    /// interpreter renders kind, message, clause, and the location as
    /// `line:col` in the same span grammar its diagnostics use; raw byte
    /// offsets remain available through `--json`. The REPL keeps the offset
    /// spelling deliberately — its offsets are entry-relative and a
    /// secondary span may point into an earlier entry (`docs/repl.md`).
    #[must_use]
    pub fn render(&self, source: &str) -> String {
        let mut line = format!(
            "trap({}): {} [{}] at {}",
            self.kind,
            self.message,
            self.rule.anchor(),
            self.span.position(source)
        );
        if let Some((span, note)) = &self.secondary {
            line.push_str(&format!("; {note} at {}", span.position(source)));
        }
        line
    }
}

/// Non-local control, carried up a `Result`'s error arm. Not unwinding.
#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Return(Value),
    Break(Value),
    Continue,
    Trap(Box<Trap>),
    /// The task's proc was killed (`[conc.proc.kill]`): the frames return
    /// without running **any** further user code — `defer`/`errdefer`
    /// included, which is the decided D14 distinction from cancellation
    /// (`[conc.cancel.defer]`: cancellation is polite; kill is structural).
    ProcKilled,
    /// `os_exit(code)` (s40): immediate termination with the code —
    /// everything printed so far stands, nothing after runs, defers do NOT
    /// run (the documented contract; supervised teardown is the proc
    /// tier's, `[conc.proc.kill]`). The code is already masked to the
    /// process range (`rem_euclid(256)`, identical on both lanes).
    Exit(u8),
    /// The provenance oracle reached a row of `[mem.ub]`'s closed enumeration.
    ///
    /// Distinct from [`Signal::Trap`] on purpose: a trap is a fault of a
    /// *defined* execution and the compiler must reproduce it, while this says
    /// the execution has no defined behavior at all. The protocol keeps them
    /// apart too — `trap(kind)` versus `ub(anchor)` (`[proto.record.verdict]`).
    Ub(Box<UbFinding>),
    /// Outside this evaluator's coverage, or a sema-lite failure (unresolvable name,
    /// ambiguous dispatch, a type error the checker owns). Becomes verdict
    /// `unsupported` with the reason on an `x-` key — never a crash, and never
    /// a trap, because the trap vocabulary is for faults of *defined*
    /// executions (`[conf.trap.map]`).
    Unsupported(String),
}

type EResult<T> = Result<T, Signal>;

/// What applying a callable produced.
#[derive(Debug, Clone)]
struct Applied {
    value: Value,
    /// The callee's parameters as they stood when the call ended — the
    /// "result" half of call-by-value-result, which is how `mut` is inout
    /// under value semantics (`[mem.tier0.mode.mut]`).
    params: Vec<Value>,
}

/// The evaluated arguments of one call.
#[derive(Debug, Clone)]
struct Args {
    /// One value per argument, in source order.
    values: Vec<Value>,
    /// `(argument index, path)` for each `mut` argument — where the callee's
    /// final parameter value is stored back when the call ends.
    writebacks: Vec<(usize, Path)>,
    /// How many accesses this call pushed onto the access set, to release.
    held: usize,
    /// `(argument index, retag)` for each argument that got a fresh tag at
    /// parameter entry (`[mem.prov.tag]`).
    retags: Vec<(usize, PendingRetag)>,
    /// Tags protected for this call's extent, released when it ends.
    protectors: Vec<prov::TagId>,
}

/// Which side of the C membrane a call lands on (`[mem.boundary.ffi]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Callee {
    /// A wolf function, closure or std method: parameter modes apply, so
    /// parameter entry is a retag point.
    Wolf,
    /// A modelled C intrinsic: no wolf parameters, no modes, no retag —
    /// exposure and a foreign havoc instead.
    C,
}

impl Callee {
    fn of(target: &Value) -> Callee {
        match target {
            Value::Builtin(name) if name.starts_with("c.") => Callee::C,
            _ => Callee::Wolf,
        }
    }
}

/// One argument's retag, waiting to be bound to the callee's parameter place.
#[derive(Debug, Clone, Copy)]
struct PendingRetag {
    alloc: prov::AllocId,
    tag: prov::TagId,
    /// `mut` arguments bind: the callee writes *through* the borrow, so its
    /// parameter place is the child tag. `read` arguments do not — under MVS
    /// the callee's parameter is its own copy, and the Frozen child exists to
    /// witness "the caller's place is immutable for the call", which is what
    /// the protector enforces (`docs/approximation-contract.md` §7.3).
    bind: bool,
}

/// A method call's receiver: a place when the receiver denotes one (so a
/// mutating method's effect lands back in it), a plain expression otherwise.
#[derive(Debug, Clone)]
enum Receiver {
    Place(Path),
    Expr(Expr),
}

/// A method receiver that is **lent** to the call instead of copied into it
/// (issue #24).
///
/// Lending is not a semantic change; it is the same value semantics arranged
/// so the machine stops paying for them. `[mem.model.value]` says a receiver's
/// value passes whole and comes back whole, and it still does — the value
/// leaves its slot for the duration of the call and returns to it, and the
/// only thing that is *not* done is copying it twice and comparing the copies.
/// What that costs is the difference between appending to a `List` and
/// rebuilding it: `xs.push(v)` was four traversals of `xs`, so a loop of
/// pushes was quadratic and every `List`-returning std function with it.
///
/// The receivers that can be lent are the builtin containers, which is the
/// conjunction of three facts: [`Machine::method_of`] answers only for
/// `Value::Struct`, so they always dispatch to [`builtin::method`]; no
/// container arm of `builtin::method` re-enters the machine, so nothing can
/// observe the slot while the value is out of it; and their copies are the
/// expensive ones. An impl-block method keeps the copy — it runs arbitrary
/// user code, which can reach the receiver's place by other names.
#[derive(Debug, Clone, Copy)]
struct Lend {
    /// The method is one of the two arms of [`builtin::method`] that take the
    /// receiver's elements mutably, per [`builtin::mutates_receiver`]. Only a
    /// mutating lend can end in a write, and only a mutating lend has to ask
    /// whether that write could fault.
    mutating: bool,
}

/// What the `}` of a `region … { … }` block does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SugarExit {
    /// The sugar's own contract: create, scope, free (X4).
    Free,
    /// `freeze region { … }`: promote instead of free, and the block's value
    /// is `imm` forever (`[mem.region.freeze.1]`).
    Freeze,
}

fn unsupported<T>(reason: impl Into<String>) -> EResult<T> {
    Err(Signal::Unsupported(reason.into()))
}

/// How a program ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Exit(u8),
    Trap(Box<Trap>),
    /// Oracle-detected UB (`[proto.record.ub]`).
    Ub(Box<UbFinding>),
    Unsupported(String),
}

/// What `--trace` was asked for.
///
/// `mem` is the sprint's own request — "`--trace=mem` logs every region event
/// (create/open/suspend/freeze/free, edge checks, RC ops, handle faults)" —
/// and is a *filter* over the same rule registry rather than a second trace
/// mechanism, so a memory rule cannot be traced by one and missed by the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Trace {
    #[default]
    Off,
    /// Every rule as it fires.
    All,
    /// Only rules citing a `mem.*` clause.
    Memory,
    /// Only the Tier-3 rules — §5 `mem.unsafe.*`, §6 `mem.prov.*`, §7
    /// `mem.ub.*`, and the FFI boundary clause. Derived from the anchor, not
    /// from a hand-kept list ([`Rule::is_provenance`]).
    Provenance,
}

impl Trace {
    #[must_use]
    pub fn is_on(self) -> bool {
        self != Trace::Off
    }

    #[must_use]
    pub fn keeps(self, rule: Rule) -> bool {
        match self {
            Trace::Off => false,
            Trace::All => true,
            Trace::Memory => rule.is_memory(),
            Trace::Provenance => rule.is_provenance(),
        }
    }
}

impl std::str::FromStr for Trace {
    type Err = String;

    fn from_str(s: &str) -> Result<Trace, String> {
        match s {
            "all" | "" => Ok(Trace::All),
            "mem" | "memory" => Ok(Trace::Memory),
            "prov" | "provenance" => Ok(Trace::Provenance),
            "off" | "none" => Ok(Trace::Off),
            other => Err(format!(
                "unknown trace filter `{other}` (all, mem, prov, off)"
            )),
        }
    }
}

/// How a caller asks for a schedule (`[proto.seed.flag]` and the is07
/// explorer's replay spelling).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SchedRequest {
    /// Unseeded: the strict-FIFO default; the record declares `seeded: false`.
    #[default]
    Default,
    /// `--seed=N`: a generator seed, or a packed schedule when bit 62 is set
    /// ([`sched::PACKED_SEED_TAG`]).
    Seed(u64),
    /// `--schedule=ev:c0,c1,…`: an explicit decision stream, replayed exactly
    /// (choices beyond the stream take the FIFO default). The spelling for
    /// counterexamples too large for the 62-bit packed-seed payload.
    Stream(Vec<usize>),
}

impl SchedRequest {
    /// Does the resulting record declare `seeded: true`? Any explicit
    /// schedule request is deterministic replay (`[proto.seed.equal]`).
    #[must_use]
    pub fn is_seeded(&self) -> bool {
        !matches!(self, SchedRequest::Default)
    }
}

/// Everything one run produced.
#[derive(Debug, Clone)]
pub struct Run {
    pub outcome: Outcome,
    pub stdout: Vec<u8>,
    /// One line per rule firing, when `--trace` asked for it.
    pub trace: Vec<String>,
    /// Regions still holding memory when the program ended: the sprint's leak
    /// assertion, exposed as a hook rather than only as an `assert!` so is06's
    /// crash-cleanup oracle can read it.
    pub leaks: Vec<RegionId>,
    /// `shared` cells whose payload outlived the program.
    pub live_cells: Vec<region::CellId>,
    /// C allocations never `free`d. A leak is **defined and safe**
    /// (`[mem.ub.defined]`), so this is a report and never a fault.
    pub host_leaks: Vec<prov::AllocId>,
    /// `Err` when the region forest invariant was broken at exit.
    pub forest: Result<(), String>,
    /// The run's reified `sched-ev/0` stream and op log — what the is07
    /// explorer branches over ([`crate::explore`]).
    pub schedule: sched::SchedTrace,
}

/// A lexical scope: its locals and its scope-exit effects.
#[derive(Debug, Default)]
struct Scope {
    locals: Vec<(String, Slot)>,
    /// `(on_error, expr)` in registration order; run in reverse
    /// (`[mem.shared.drop.1]`).
    defers: Vec<(bool, Expr)>,
    /// Accesses held for this scope's extent (local borrows).
    held: usize,
}

#[derive(Debug)]
struct Frame {
    module: String,
    /// This activation's never-reused id (#36): frame *indices* are reused
    /// as frames push and pop, so a capture loan keyed by index could alias
    /// a stranger's local. Serials are minted once per activation per task.
    serial: u64,
    scopes: Vec<Scope>,
    /// The enclosing function's declared return-row tags — what a bare
    /// lowercase name at a raise site resolves against (issue #12, the
    /// interpreter half of wolf-lang#4): `return none` under
    /// `-> int ! {none}` produces the tag value `none`.
    row: Vec<String>,
    /// The parameters this activation bound in the default (unwritten) mode
    /// — read bindings, immutable for the whole call
    /// (`[mem.tier0.mode.read]`). D39's callee-side write barrier watches
    /// this list: a write through one traps `exclusivity`. Each entry
    /// carries the parameter's declaration span, the trap's second span.
    read_params: Vec<(String, Span)>,
}

/// The state every task of one program run shares, behind locks.
///
/// The locks are for the borrow checker, not for parallelism: the scheduler's
/// baton guarantees at most one task runs at a time (`sched`), so every guard
/// here is uncontended by construction and the ordering of mutations is the
/// schedule's, deterministically. Each component gets its own mutex so a
/// transient store access can never deadlock against a trace line; no code
/// holds two of these at once except [`Machine::save_task_stack`]'s documented
/// store→sched ordering.
#[derive(Clone)]
struct Shared {
    program: Arc<Program>,
    /// `[mem.model.machine]` components 3 and 4: the region forest and the
    /// Tier-2 pools and cells. The open-stack half is per task, swapped in
    /// and out at context switches ([`Store::swap_open`]).
    store: Arc<Mutex<Store>>,
    /// The store's [`Store::teeth`] flag, readable without the store lock:
    /// set the first time any region is freed or frozen. While it is unset
    /// no home consult can fault, so the per-access walk (#25) is skipped
    /// entirely — a program that never frees or freezes a region pays one
    /// atomic load per access, not a path walk.
    region_teeth: Arc<std::sync::atomic::AtomicBool>,
    /// `[mem.model.machine]` component 2: the provenance forest (is04).
    prov: Arc<Mutex<Provenance>>,
    stdout: Arc<Mutex<Vec<u8>>>,
    trace: Arc<Mutex<Vec<String>>>,
    /// Set when the forest invariant assertion ever failed. A broken invariant
    /// is an interpreter bug, so it is recorded rather than swallowed.
    forest: Arc<Mutex<Result<(), String>>>,
    /// Steps taken by every task together, against [`Machine::FUEL`].
    steps: Arc<AtomicU64>,
    /// The sim scheduler (is06): tasks, channels, procs, virtual time.
    sched: Arc<sched::Sched>,
    tracing: Trace,
    /// is12: the front door's live pass-through. When set, every byte the
    /// program prints reaches the process stdout the moment it is produced,
    /// in addition to the buffered copy the observation keeps — `lupin
    /// FILE.lu` streams a long-running program's output instead of holding
    /// it to the end. Never set on the record-emitting surfaces.
    live_stdout: bool,
    /// is08: the REPL session's type-generation map (`[repl.type.gen]`,
    /// `docs/repl.md`). Empty outside a session, in which case struct
    /// literals keep their written names and nothing here changes behavior.
    /// Inside a session, a literal `Point { … }` mints a value of the
    /// *current* generation (`Point#2`), so values created before a
    /// redefinition keep their old nominal identity exactly.
    repl_types: Arc<Mutex<BTreeMap<String, u32>>>,
    /// s40 env v0: the machine-local environment OVERLAY — `env_set` writes
    /// here and `env_get` reads here, never the host's real environment
    /// (the checked-lane posture: the same program observes the same
    /// answers on any machine).
    env: Arc<Mutex<BTreeMap<String, String>>>,
    /// is18: the s40 process trio's children, by handle — shared like the
    /// store because a handle is an `int` any task may hold. Wait reaps;
    /// kill never tombstones (`eval::os`).
    #[cfg(not(target_family = "wasm"))]
    children: Arc<Mutex<os::ChildTable>>,
    /// is18: the s39 net family's sockets, by fd — shared for the same
    /// reason (`net/spawn_accept.lu`'s task accepts on the fd main bound).
    #[cfg(not(target_family = "wasm"))]
    net: Arc<Mutex<net::NetTable>>,
    /// s40 time v0 (X12): `time_now_ms`'s process-local monotonic anchor —
    /// values compare and subtract; they are never wall timestamps.
    ///
    /// Absent on wasm, where `Instant::now` has no implementation to call and
    /// aborts the module. The anchor cannot be faked without inventing a clock,
    /// so `time_now_ms` reports `unsupported` there instead (see
    /// [`super::eval::builtin`]) and the conservatism ledger stays truthful.
    #[cfg(not(target_family = "wasm"))]
    epoch: std::time::Instant,
}

/// The abstract machine — one **task's** view of the program run.
///
/// Frames, exclusivity, pending retags and the unsafe depth are per task;
/// everything with identity lives in [`Shared`]. `main` is task 0; spawned
/// tasks get their own `Machine` on their own thread, scheduled one at a
/// time (`sched`).
pub struct Machine {
    shared: Shared,
    /// This machine's task id in the scheduler.
    task: sched::TaskId,
    frames: Vec<Frame>,
    access: AccessSet,
    /// Item-level bindings. Evaluated at program start on task 0 and
    /// snapshotted into each task at spawn: cross-task global mutation is not
    /// a channel this machine provides (the compiler's E1101 rejects the
    /// shape statically; `docs/approximation-contract.md` records the choice).
    globals: BTreeMap<String, Slot>,
    /// How many `unsafe { }` blocks are open. `[mem.ub]`'s enumeration is
    /// Tier-3-reachable only, so the rows that need "in unsafe code" in their
    /// wording (T1) ask this.
    unsafe_depth: u32,
    /// The sync objects held by `when` bodies currently open in THIS task.
    /// Re-acquiring one can never complete and is detected immediately —
    /// `trap(deadlock)` (`[conc.deadlock.self]`; the lexical nest is the
    /// compiler's E1103, `[conc.when.nonest]`).
    when_held: Vec<sched::MutexId>,
    /// Retags produced by the current call's arguments, waiting for
    /// [`Machine::call_fn`] to bind them to the callee's parameter places.
    pending_retags: Vec<Option<PendingRetag>>,
    /// The next [`Frame::serial`] this task will mint (#36).
    next_frame_serial: u64,
    /// The places some live closure has captured, `(frame serial, name)` —
    /// the filter that keeps [`Machine::note_captured_write`] O(1) for the
    /// programs that never create a capturing closure (#36).
    captured_places: BTreeSet<(u64, String)>,
    /// Write generations (and the last write's span) for captured places
    /// (#36). Entries are never removed: serials are never reused, so a
    /// stale entry can never alias, and the map is bounded by the distinct
    /// captured bindings a run actually writes.
    capture_gens: BTreeMap<(u64, String), (u64, Span)>,
    tracing: Trace,
}

/// Set by [`Machine::set_fuel_limit`]; zero means [`Machine::FUEL`].
static FUEL_LIMIT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Machine {
    /// Evaluation steps a program may take before the machine gives up.
    ///
    /// A non-terminating program has no verdict — `[proto.record.verdict]`
    /// offers none — so the machine declines rather than hangs a CI job. It is
    /// generous enough that nothing in the pinned corpus comes close.
    pub const FUEL: u64 = 50_000_000;

    /// A tighter rail, for harnesses that run thousands of programs.
    ///
    /// [`Machine::FUEL`] is sized so nothing a person writes comes near it,
    /// which is right for one program and wrong for three thousand: a single
    /// mutant that reaches the rail costs about six seconds in release and
    /// thirty-seven in debug, so a few dozen runaways eat a CI job whole. They
    /// did — the fuzz smoke test took the interpreter's Windows lane past four
    /// hours and had the other two cancelled at the six-hour limit.
    ///
    /// A harness sets this once, and gets the same guarantee the rail already
    /// promises (decline rather than hang) at a bound it can afford. Zero means
    /// the default.
    pub fn set_fuel_limit(steps: u64) {
        FUEL_LIMIT.store(steps, Ordering::Relaxed);
    }

    /// The rail in force: whatever a harness set, else [`Machine::FUEL`].
    #[must_use]
    pub fn fuel_limit() -> u64 {
        match FUEL_LIMIT.load(Ordering::Relaxed) {
            0 => Machine::FUEL,
            n => n,
        }
    }

    #[must_use]
    pub fn new(program: &Program) -> Machine {
        Machine::with_seed(program, None)
    }

    /// As [`Machine::new`], with `--seed=N`'s deterministic schedule request
    /// (`[conc.det.seed]`; `[proto.seed.flag]`). `None` and `Some(0)` are both
    /// the strict-FIFO schedule; the record's `seeded` flag is the caller's.
    #[must_use]
    pub fn with_seed(program: &Program, seed: Option<u64>) -> Machine {
        let sched = match seed {
            // The packed-schedule half of the seed namespace ([`sched::PACKED_SEED_TAG`]):
            // the seed *is* the decision stream, replayed digit by digit.
            Some(seed) if sched::seed_is_packed(seed) => sched::Sched::packed(seed),
            other => sched::Sched::new(other.unwrap_or(0)),
        };
        Machine::with_sched(program, sched)
    }

    /// The explorer's door: force `plan`'s decision prefix, FIFO beyond it,
    /// with the completeness assertion armed ([`sched::Plan`]).
    #[must_use]
    pub fn with_plan(program: &Program, plan: sched::Plan) -> Machine {
        Machine::with_sched(program, sched::Sched::planned(plan))
    }

    /// The CLI's door: `--seed=N` or `--schedule=ev:…` ([`SchedRequest`]).
    #[must_use]
    pub fn with_request(program: &Program, request: &SchedRequest) -> Machine {
        match request {
            SchedRequest::Default => Machine::with_seed(program, None),
            SchedRequest::Seed(seed) => Machine::with_seed(program, Some(*seed)),
            SchedRequest::Stream(choices) => Machine::with_plan(
                program,
                sched::Plan {
                    choices: choices.clone(),
                    ..sched::Plan::default()
                },
            ),
        }
    }

    fn with_sched(program: &Program, sched: sched::Sched) -> Machine {
        let store = Store::new();
        let region_teeth = store.teeth();
        let shared = Shared {
            program: Arc::new(program.clone()),
            store: Arc::new(Mutex::new(store)),
            region_teeth,
            prov: Arc::new(Mutex::new(Provenance::new())),
            stdout: Arc::new(Mutex::new(Vec::new())),
            trace: Arc::new(Mutex::new(Vec::new())),
            forest: Arc::new(Mutex::new(Ok(()))),
            steps: Arc::new(AtomicU64::new(0)),
            sched: Arc::new(sched),
            tracing: Trace::Off,
            live_stdout: false,
            repl_types: Arc::new(Mutex::new(BTreeMap::new())),
            env: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(not(target_family = "wasm"))]
            children: Arc::new(Mutex::new(os::ChildTable::default())),
            #[cfg(not(target_family = "wasm"))]
            net: Arc::new(Mutex::new(net::NetTable::default())),
            #[cfg(not(target_family = "wasm"))]
            epoch: std::time::Instant::now(),
        };
        Machine::for_task(shared, 0, BTreeMap::new())
    }

    /// A task's machine: its own frames over the shared world.
    fn for_task(shared: Shared, task: sched::TaskId, globals: BTreeMap<String, Slot>) -> Machine {
        let tracing = shared.tracing;
        Machine {
            shared,
            task,
            frames: Vec::new(),
            access: AccessSet::new(),
            globals,
            unsafe_depth: 0,
            when_held: Vec::new(),
            pending_retags: Vec::new(),
            next_frame_serial: 0,
            captured_places: BTreeSet::new(),
            capture_gens: BTreeMap::new(),
            tracing,
        }
    }

    /// Mints a [`Frame::serial`] — every frame construction calls this.
    fn mint_frame_serial(&mut self) -> u64 {
        self.next_frame_serial += 1;
        self.next_frame_serial
    }

    #[must_use]
    pub fn tracing(mut self, trace: Trace) -> Machine {
        self.tracing = trace;
        self.shared.tracing = trace;
        self
    }

    /// Streams program output to the process stdout as it is produced (the
    /// is12 front door). Set before the run starts; spawned tasks inherit it.
    #[must_use]
    pub fn live_stdout(mut self) -> Machine {
        self.shared.live_stdout = true;
        self
    }

    // -- the shared world's doors ------------------------------------------

    /// The region store, for this module and `builtin`'s Tier-2 surface.
    /// A transient guard: never held across another `self` call.
    pub(crate) fn store(&self) -> MutexGuard<'_, Store> {
        self.shared.store.lock().expect("store lock")
    }

    /// The provenance machine, for this module and `builtin`'s C intrinsics.
    pub(crate) fn prov(&self) -> MutexGuard<'_, Provenance> {
        self.shared.prov.lock().expect("prov lock")
    }

    /// The process trio's child table (is18) — uncontended like the store:
    /// the scheduler's baton runs one task at a time.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn children(&self) -> MutexGuard<'_, os::ChildTable> {
        self.shared.children.lock().expect("children lock")
    }

    /// The net family's socket table (is18) — same posture as the store.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn net(&self) -> MutexGuard<'_, net::NetTable> {
        self.shared.net.lock().expect("net lock")
    }

    /// Stack the tree-walk runs on.
    ///
    /// The same argument the parser makes for [`crate::parse::PARSE_STACK`],
    /// one tier up: the machine's own limit — 512 activations, and
    /// [`Machine::FUEL`] steps — has to be the thing that stops a runaway
    /// program, because a stack overflow is a *crash* and `[proto.record]` has
    /// no verdict for one. `corpus/comptime/depth_spiral.lu` is the program
    /// that makes the point: an unbounded `comptime fn` recursion this machine
    /// does not fold must come back as `unsupported`, not as a SIGSEGV. The
    /// reservation is address space; only the pages the descent touches are
    /// committed.
    #[cfg_attr(
        target_family = "wasm",
        expect(dead_code, reason = "no thread to size")
    )]
    const RUN_STACK: usize = 64 * 1024 * 1024;

    /// Runs the program's `main`, on the ambient stack.
    ///
    /// The wasm half of the [`Machine::RUN_STACK`] bargain: there is no thread
    /// to size, so the embedder reserves the stack at link time
    /// (`-C link-arg=-zstack-size=…`) and a runaway program still meets the
    /// machine's own 512-activation and fuel rails first.
    #[cfg(target_family = "wasm")]
    pub fn run(self) -> Run {
        self.run_on_this_stack()
    }

    /// Runs the program's `main`, on a stack this machine chose.
    #[cfg(not(target_family = "wasm"))]
    pub fn run(self) -> Run {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(Machine::RUN_STACK)
                .name("wolf-interp-run".to_owned())
                .spawn_scoped(scope, || self.run_on_this_stack())
                .expect("the machine's stack thread must spawn")
                .join()
                // The walk never panics; a panic here is an interpreter bug
                // (`tests/fuzz_smoke.rs` treats it as one) and is propagated
                // rather than turned into a verdict.
                .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
        })
    }

    fn run_on_this_stack(mut self) -> Run {
        let outcome = self.run_main();
        // The root supervisor's teardown (`[conc.task.root]`): every daemon
        // proc still alive is killed per `[conc.proc.kill]`, every remaining
        // task thread is reaped one at a time, and the regions the killed
        // procs owned bulk-free here.
        let leftover = self.shared.sched.shutdown();
        for region in leftover {
            let freed = self.store().free(region);
            self.prov().region_freed(&freed, Span::new(0, 0));
        }
        self.shared.sched.join_all();
        self.drain_sched();
        // The program region is freed last, by the machine: `main`'s caller is
        // the runtime, and `[mem.region.intra.2]` frees at the owner's death.
        let freed = self.store().free(Store::root());
        // `[mem.prov.region]`: freeing a region Disables every tag tree it owns.
        self.prov().region_freed(&freed, Span::new(0, 0));
        let leaks = self.store().leaks();
        let live_cells = self.store().live_cells();
        let host_leaks = self.prov().live_host_allocs();
        let forest = match (
            &*self.shared.forest.lock().expect("forest lock"),
            self.store().assert_forest(),
        ) {
            (Err(broken), _) => Err(broken.clone()),
            (Ok(()), verdict) => verdict,
        };
        // The program region's own wholesale free is the last region event,
        // and the leak assertion is what it leaves behind.
        self.fire(
            Rule::RegionFree,
            Span::new(0, 0),
            &format!(
                "program exit: {} region(s) created, {} leaked, {} live `shared` cell(s)",
                self.store().regions().len(),
                leaks.len(),
                live_cells.len()
            ),
        );
        self.drain_prov();
        let stdout = std::mem::take(&mut *self.shared.stdout.lock().expect("stdout lock"));
        let trace = std::mem::take(&mut *self.shared.trace.lock().expect("trace lock"));
        let schedule = self.shared.sched.take_trace();
        Run {
            outcome,
            stdout,
            trace,
            leaks,
            live_cells,
            host_leaks,
            forest,
            schedule,
        }
    }

    fn run_main(&mut self) -> Outcome {
        let serial = self.mint_frame_serial();
        self.frames.push(Frame {
            module: String::new(),
            serial,
            scopes: vec![Scope::default()],
            row: Vec::new(),
            read_params: Vec::new(),
        });

        // Item-level `let`/`var`/`const` evaluate once, in declaration order.
        let bindings = self.shared.program.root().bindings.clone();
        for name in bindings {
            let Some(Def::Binding(binding)) = self.shared.program.lookup("", &name, false).cloned()
            else {
                continue;
            };
            match self.eval(&binding.value) {
                Ok(value) => {
                    self.globals.insert(name, Slot::live(value));
                }
                Err(signal) => return self.finish(Err(signal)),
            }
        }

        let Some(Def::Fn(main)) = self.shared.program.lookup("", "main", false).cloned() else {
            return Outcome::Unsupported(
                "the program has no `main` in its root module (D32: directory = module)".to_owned(),
            );
        };

        // `fn main(args: List[str])` receives the process arguments; conform-run
        // passes none, which is what `wordcount.lu`'s `check: run(exit=2)`
        // expects to observe.
        let args = if main.params.is_empty() {
            Vec::new()
        } else {
            vec![Value::list(Vec::new(), None, None)]
        };
        let result = self
            .call_fn(&main, "", args, main.span)
            .map(|applied| applied.value);
        self.finish(result)
    }

    fn finish(&mut self, result: EResult<Value>) -> Outcome {
        match result {
            Ok(Value::Int(v, _)) => Outcome::Exit(u8::try_from(v.rem_euclid(256)).unwrap_or(0)),
            Ok(Value::Unit) => Outcome::Exit(0),
            Ok(Value::Error(err)) => {
                // `main` returned an error rather than an int. The protocol has
                // no verdict for "returned an error", and the spec does not fix
                // a status; 1 is the conventional one and the divergence, if
                // any, is a stdout/exit comparison the differ will surface.
                self.write_out(&format!("error: {}\n", err.tag));
                Outcome::Exit(1)
            }
            Ok(other) => Outcome::Unsupported(format!(
                "`main` returned {}; the exit status comes from an `int`",
                other.kind()
            )),
            Err(Signal::Trap(trap)) => Outcome::Trap(trap),
            Err(Signal::Ub(finding)) => Outcome::Ub(finding),
            Err(Signal::Unsupported(reason)) => Outcome::Unsupported(reason),
            Err(Signal::Return(value)) => {
                let result = Ok(value);
                self.finish(result)
            }
            Err(Signal::Break(_) | Signal::Continue) => {
                Outcome::Unsupported("`break`/`continue` outside a loop".to_owned())
            }
            Err(Signal::Exit(code)) => Outcome::Exit(code),
            Err(Signal::ProcKilled) => {
                // `[conc.proc.root]`: the root supervisor's domain is the
                // process. A linked partner's abnormal exit reached it, so
                // the killed-proc sequence runs for every live proc (the
                // scheduler shutdown below is exactly that enumeration) and
                // the process terminates with a nonzero, implementation-
                // specified status — 1 here; conforming tools compare the
                // outcome class, never the number ([conf.trap.exit]).
                self.fire(
                    Rule::ProcRoot,
                    Span::new(0, 0),
                    "the root domain dies: killed-proc sequence for every live proc; nonzero exit",
                );
                Outcome::Exit(1)
            }
        }
    }

    // -- bookkeeping -------------------------------------------------------

    /// s40 env v0: one overlay read (never the host environment).
    pub(crate) fn env_read(&self, name: &str) -> Option<String> {
        self.shared.env.lock().expect("env lock").get(name).cloned()
    }

    /// s40 env v0: one overlay write (never the host environment).
    pub(crate) fn env_write(&self, name: &str, value: &str) {
        self.shared
            .env
            .lock()
            .expect("env lock")
            .insert(name.to_owned(), value.to_owned());
    }

    /// s40 time v0: milliseconds since the process-local monotonic anchor.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn monotonic_ms(&self) -> i128 {
        i128::try_from(self.shared.epoch.elapsed().as_millis()).unwrap_or(i128::MAX)
    }

    fn fire(&mut self, rule: Rule, span: Span, detail: &str) {
        if self.tracing.keeps(rule) {
            self.shared.trace.lock().expect("trace lock").push(format!(
                "{:>6}..{:<6} {} {}",
                span.start, span.end, rule, detail
            ));
        }
    }

    /// Moves the scheduler's decision log into `--trace`, in decision order.
    /// Called after every operation that can schedule, exactly as
    /// [`Machine::drain_prov`] does for the provenance machine.
    fn drain_sched(&mut self) {
        let notes = self.shared.sched.take_notes();
        for (rule, detail) in notes {
            self.fire(rule, Span::new(0, 0), &detail);
        }
    }

    /// The forest invariant, re-walked after a mutation of the region graph.
    ///
    /// > After every mutation, a debug assertion re-walks the region graph and
    /// > asserts the forest invariant — O(heap) per store is fine, this is a
    /// > reference interpreter.
    ///
    /// It records rather than panics: a `panic!` in this module is by
    /// definition an interpreter bug (`tests/fuzz_smoke.rs` treats it as one),
    /// and the recorded failure surfaces on [`Run::forest`], which CI asserts
    /// over the whole corpus.
    fn assert_forest(&mut self, span: Span) {
        if self.shared.forest.lock().expect("forest lock").is_err() {
            return;
        }
        let verdict = self.store().assert_forest();
        if let Err(broken) = verdict {
            self.fire(
                Rule::RegionEdgeIso,
                span,
                &format!("FOREST BROKEN: {broken}"),
            );
            *self.shared.forest.lock().expect("forest lock") = Err(broken);
        }
    }

    /// Moves the provenance machine's rule firings into `--trace`.
    ///
    /// Called after every operation that can produce them, so a `prov` trace
    /// interleaves with the `mem` one in execution order rather than arriving
    /// as a block at the end.
    fn drain_prov(&mut self) {
        let notes = self.prov().take_notes();
        for (rule, span, detail) in notes {
            self.fire(rule, span, &detail);
        }
    }

    /// Reports one row of `[mem.ub]` and stops the execution.
    ///
    /// Not a trap: `[proto.record.verdict]` gives UB its own verdict, and
    /// `[proto.record.ub]` makes it the highest-severity divergence class. The
    /// D2 pairing rides along — every report names the optimization the row
    /// licenses, because a UB rule that licenses nothing is one this language
    /// does not have (`[mem.ub.closed]`).
    fn ub<T>(&mut self, finding: UbFinding) -> EResult<T> {
        self.drain_prov();
        self.fire(
            finding.row.rule(),
            finding.span,
            &format!("§7/{}: {}", finding.row, finding.message),
        );
        self.fire(
            Rule::UbLicensed,
            finding.span,
            &format!("§7/{} licenses {}", finding.row, finding.row.optimization()),
        );
        for line in &finding.tree {
            self.fire(Rule::ProvTag, finding.span, line);
        }
        self.fire(
            Rule::UbVerdict,
            finding.span,
            &format!(
                "verdict ub({}) with x-ub-row={}",
                finding.anchor(),
                finding.row
            ),
        );
        Err(Signal::Ub(Box::new(finding)))
    }

    /// A UB row this machine raises without going through the tag tree.
    ///
    /// `alloc` is the allocation the row is *about*, when there is one: the
    /// report then carries the same two-spans-and-tree shape a tag-tree
    /// violation does. §7/T1 is the row with no allocation to name — it is
    /// about the representation of a value, not about a tag — and it says so by
    /// passing `None`.
    fn ub_row<T>(
        &mut self,
        row: UbRow,
        span: Span,
        alloc: Option<prov::AllocId>,
        message: impl Into<String>,
    ) -> EResult<T> {
        // One guard, one statement: two `self.prov()` temporaries in a single
        // expression would deadlock the non-reentrant lock against itself
        // (guard temporaries live to the end of the statement).
        let (tag_span, tree) = match alloc {
            Some(alloc) => {
                let prov = self.prov();
                (prov.alloc(alloc).map(|entry| entry.span), prov.tree(alloc))
            }
            None => (None, Vec::new()),
        };
        let finding = UbFinding {
            row,
            span,
            tag_span,
            message: message.into(),
            tree,
        };
        self.ub(finding)
    }

    /// Runs one provenance-checked access, turning a violation into the verdict.
    fn prov_access(
        &mut self,
        ptr: RawPtr,
        len: usize,
        kind: AccessKind,
        span: Span,
    ) -> EResult<()> {
        // The race detector first (`[conc.mm.race.3]`): raw memory is Tier
        // 3/FFI-reachable, exactly `[conc.mm.race.1]`'s surface, and two tasks
        // with copies of one pointer are the shape that can actually race in
        // this machine.
        if let Some(alloc) = ptr.alloc
            && self.shared.sched.ever_concurrent()
        {
            let lo = usize::try_from(ptr.offset.max(0)).unwrap_or(0);
            let report = self.shared.sched.race_check(
                self.task,
                sched::RaceKey::Alloc(alloc),
                lo,
                lo.saturating_add(len),
                kind == AccessKind::Write,
            );
            self.drain_sched();
            if let Some(report) = report {
                return self.trap(
                    TrapKind::Race,
                    Rule::RaceDetect,
                    span,
                    format!(
                        "data race: this {} of alloc#{alloc} conflicts with an unordered {} by {} \
                         — no happens-before edge orders them ([conc.mm.hb]); detection is exact \
                         at the interleaving this schedule realized",
                        if kind == AccessKind::Write {
                            "write"
                        } else {
                            "read"
                        },
                        if report.other_write { "write" } else { "read" },
                        report.other_task
                    ),
                    None,
                );
            }
        }
        let access = self.prov().access(ptr, len, kind, span);
        match access {
            Ok(()) => {
                self.drain_prov();
                Ok(())
            }
            Err(finding) => self.ub(finding),
        }
    }

    /// The provenance key of a place: the frame keeps two activations of one
    /// local apart, exactly as [`Path`] does — and the task keeps two tasks'
    /// frame 0 apart, because frames are per task (is06). Without the task
    /// component, `ch` in two spawned closures would collide into one tag
    /// tree and mint spurious §7/P1 findings.
    fn place_key(&self, path: &Path) -> String {
        format!("t{}:{}:{path}", self.task, path.frame)
    }

    fn step(&mut self) -> EResult<()> {
        let taken = self.shared.steps.fetch_add(1, Ordering::Relaxed);
        let limit = Machine::fuel_limit();
        if taken >= limit {
            return unsupported(format!(
                "the program did not terminate within {limit} evaluation steps"
            ));
        }
        Ok(())
    }

    fn trap<T>(
        &mut self,
        kind: TrapKind,
        rule: Rule,
        span: Span,
        message: impl Into<String>,
        secondary: Option<(Span, String)>,
    ) -> EResult<T> {
        let message = message.into();
        self.fire(Rule::TrapVocabulary, span, &format!("{kind}: {message}"));
        Err(Signal::Trap(Box::new(Trap {
            kind,
            rule,
            span,
            secondary,
            message,
        })))
    }

    fn frame(&self) -> usize {
        self.frames.len() - 1
    }

    fn push_scope(&mut self) {
        let held = self.access.len();
        if let Some(frame) = self.frames.last_mut() {
            frame.scopes.push(Scope {
                held,
                ..Scope::default()
            });
        }
    }

    fn pop_scope(&mut self) {
        self.pop_scope_escaping(&[]);
    }

    /// As [`Machine::pop_scope`], naming the regions whose values are LEAVING
    /// the scope — on a block's tail, a `return`, or a `break` value.
    ///
    /// A region is an affine first-class value (X4, `[mem.region.create.2]`)
    /// and a return is a move: the escaping value carries the handle out, so
    /// teardown must not free the region under it — the adopting binding owns
    /// it now (wolf_mem's s20 ret-region shape; wolf-interp#35). Only the
    /// region *value* transfers this way: a container merely allocated in a
    /// dying region does not carry its home out, and the freed-home fault
    /// (#25) still fires on any later access — `region_escape_local.lu` and
    /// its family trap exactly as before.
    fn pop_scope_escaping(&mut self, escapes: &[RegionId]) {
        let Some(frame) = self.frames.last_mut() else {
            return;
        };
        let Some(scope) = frame.scopes.pop() else {
            return;
        };
        // A borrow's dynamic extent ends at its binding's death
        // (`[mem.tier0.borrow.1]`).
        let extra = self.access.len().saturating_sub(scope.held);
        self.access.release(extra);
        self.reclaim(&scope, escapes);
        self.prov().prune();
    }

    /// The region ids riding out of a scope on its result value — the input
    /// to [`Machine::pop_scope_escaping`]. `Ok` is a block tail; `Return` and
    /// `Break` carry values across scopes the same way. Every other signal
    /// carries no user value, so nothing escapes on it.
    fn escaping_regions(result: &EResult<Value>) -> Vec<RegionId> {
        let value = match result {
            Ok(value) | Err(Signal::Return(value) | Signal::Break(value)) => value,
            Err(_) => return Vec::new(),
        };
        let mut refs = Vec::new();
        region::references(value, &mut refs);
        refs.into_iter()
            .filter_map(|granule| match granule {
                Ref::Region(id) => Some(id),
                _ => None,
            })
            .collect()
    }

    /// Scope-exit reclamation: the Tier-1 and Tier-2 half of `[mem.shared.drop]`.
    ///
    /// The bindings that die here are swept in reverse declaration order (LIFO,
    /// `[mem.shared.drop.1]`), *after* the scope's `defer`/`errdefer` have run.
    /// Two deliberate approximations, both recorded in
    /// `docs/approximation-contract.md`:
    ///
    /// - **Scope exit, not last use.** `[mem.region.intra.2]` frees at "the last
    ///   use of the region value" and `[mem.shared.drop.2]` reclaims
    ///   destructor-free values "any time after their last use". Both are
    ///   explicitly unobservable *except* through destructor timing, and this
    ///   machine has no user destructors to time — `[mem.shared.drop.1]`'s
    ///   scope-exit point is therefore the only observable one, and it is the
    ///   one implemented.
    /// - **Function parameters are not swept.** A `read`-mode argument copies
    ///   its value under MVS, so sweeping a parameter could free a region the
    ///   caller still owns. Declining to sweep leaks (defined and safe,
    ///   `[mem.ub.defined]`); sweeping would fault wrongly, and the
    ///   approximation direction forbids that.
    fn reclaim(&mut self, scope: &Scope, escapes: &[RegionId]) {
        for (name, slot) in scope.locals.iter().rev() {
            if !slot.is_live() {
                // Moved out: its new owner is responsible for it.
                continue;
            }
            let mut refs = Vec::new();
            region::references(&slot.value, &mut refs);
            for granule in refs {
                match granule {
                    Ref::Region(id) if escapes.contains(&id) => {
                        // The scope's result value carries this region out:
                        // the affine handle moves with it (X4, a return is a
                        // move — `[mem.region.create.2]`; wolf-interp#35).
                        // The adopting binding frees it at ITS death instead.
                        self.fire(
                            Rule::RegionAffine,
                            Span::new(0, 0),
                            &format!(
                                "`{name}` moved out with the scope's value: the affine region \
                                 transfers instead of being freed"
                            ),
                        );
                    }
                    Ref::Region(id) => {
                        let freed = self.store().free(id);
                        self.prov().region_freed(&freed, Span::new(0, 0));
                        if !freed.is_empty() {
                            let detail = format!(
                                "`{name}` dies: {} freed wholesale",
                                freed
                                    .iter()
                                    .map(|id| self.store().label(*id))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                            self.fire(Rule::RegionFree, Span::new(0, 0), &detail);
                        }
                    }
                    Ref::Shared(cell) => {
                        if self.store().release(cell) {
                            self.fire(
                                Rule::SharedDrop,
                                Span::new(0, 0),
                                &format!("`{name}` dies: shared#{cell} released its payload"),
                            );
                        } else {
                            self.fire(
                                Rule::SharedRc,
                                Span::new(0, 0),
                                &format!("`{name}` dies: shared#{cell} strong count decremented"),
                            );
                        }
                    }
                    Ref::Weak(cell) => {
                        self.store().drop_weak(cell);
                        self.fire(
                            Rule::SharedWeak,
                            Span::new(0, 0),
                            &format!("`{name}` dies: weak#{cell} dropped, nothing released"),
                        );
                    }
                    // Pools and handles are storage *in* a region; the region's
                    // wholesale free is what reclaims them.
                    Ref::Pool(_) | Ref::Handle(_) => {}
                }
            }
        }
    }

    fn declare(&mut self, name: &str, slot: Slot) {
        if let Some(frame) = self.frames.last_mut()
            && let Some(scope) = frame.scopes.last_mut()
        {
            scope.locals.push((name.to_owned(), slot));
        }
    }

    fn local_exists(&self, name: &str) -> bool {
        self.frames.last().is_some_and(|frame| {
            frame
                .scopes
                .iter()
                .any(|scope| scope.locals.iter().any(|(n, _)| n == name))
        })
    }

    /// The slot a path denotes, if the path is well-formed against the current
    /// state. `None` means "no such place", which the caller turns into a
    /// sema-lite gap rather than a trap.
    fn slot_mut(&mut self, path: &Path) -> Option<&mut Slot> {
        self.resolve(path).map(|(slot, _)| slot)
    }

    /// The slot a path denotes, plus the *earliest* move along the way.
    ///
    /// The second half is the whole of "reads of `Moved` trap": `a` moving out
    /// makes `a.x` unreadable too, because `a.x`'s storage went with it, and
    /// the fault has to name the move site that actually happened rather than
    /// the leaf's own state.
    fn resolve(&mut self, path: &Path) -> Option<(&mut Slot, Option<(Span, usize)>)> {
        let mut slot: &mut Slot = if path.frame == usize::MAX {
            self.globals.get_mut(&path.base)?
        } else {
            let frame = self.frames.get_mut(path.frame)?;
            let mut found = None;
            for scope in frame.scopes.iter_mut().rev() {
                // `rposition`: a second `let x` in the same scope *shadows* the
                // first, so the latest entry is the one every later read and
                // write means (`upstream/corpus/typecheck/let_shadow_var_ok.lu`
                // pins this).
                if let Some(index) = scope.locals.iter().rposition(|(n, _)| *n == path.base) {
                    found = Some(&mut scope.locals[index].1);
                    break;
                }
            }
            found?
        };

        let mut moved = match slot.state {
            SlotState::Moved(at) => Some((at, 0usize)),
            SlotState::Live => None,
        };
        for (depth, step) in path.projections.iter().enumerate() {
            slot = match (&mut slot.value, step) {
                (Value::Struct { fields, .. }, Proj::Field(name)) => {
                    let index = fields.iter().position(|(f, _)| f == name)?;
                    &mut fields[index].1
                }
                (Value::Tuple(items), Proj::Index(i)) => {
                    let index = usize::try_from(*i).ok()?;
                    items.get_mut(index)?
                }
                (Value::List(items, _, _), Proj::Index(i)) => {
                    // A write through a shared CoW list diverges its copy
                    // first (#28): value semantics exactly as the plain Vec.
                    let index = usize::try_from(*i).ok()?;
                    std::sync::Arc::make_mut(items).get_mut(index)?
                }
                (Value::Map(pairs), Proj::Key(key)) => {
                    let index = pairs.iter().position(|(k, _)| k.to_string() == *key)?;
                    &mut pairs[index].1
                }
                _ => return None,
            };
            if moved.is_none()
                && let SlotState::Moved(at) = slot.state
            {
                moved = Some((at, depth + 1));
            }
        }
        Some((slot, moved))
    }

    /// Checks a new access against everything currently held
    /// (`[mem.tier0.excl.1]`).
    fn check_access(&mut self, path: &Path, access: Access, span: Span) -> EResult<()> {
        if let Some(held) = self.access.conflict(path, access) {
            let (held_path, held_access, held_span, held_why) =
                (held.path.to_string(), held.access, held.span, held.why);
            // D40's ruling: the `for` loop's read claim makes mutation during
            // iteration THIS trap — one rule, two enforcement modes (wolfgang's
            // static E1013, `[conf.trap.map]`'s E1013 row here). The message
            // teaches what the fix-it teaches: collect-then-apply, or the
            // index loop.
            if held_why == HeldWhy::Iteration {
                return self.trap(
                    TrapKind::Exclusivity,
                    Rule::Exclusivity,
                    span,
                    format!(
                        "`{path}` is mutated while a `for` loop iterates `{held_path}`: the loop \
                         holds a read claim on the container for its whole extent (D40; \
                         wolfgang's E1013) — collect the changes and apply them after the loop, \
                         or use an index loop"
                    ),
                    Some((
                        held_span,
                        format!("the `for` loop's read claim on `{held_path}`"),
                    )),
                );
            }
            let rule = if path.projections.is_empty() && held_path == path.to_string() {
                Rule::Exclusivity
            } else {
                Rule::PathDisjoint
            };
            return self.trap(
                TrapKind::Exclusivity,
                rule,
                span,
                format!(
                    "`{path}` is accessed as `{access}` while `{held_path}` is held as `{held_access}`; \
                     the paths conflict"
                ),
                Some((held_span, format!("`{held_path}` held here"))),
            );
        }
        Ok(())
    }

    /// Reads through a path: exclusivity first, then the slot's own state.
    fn read_path(&mut self, path: &Path, span: Span) -> EResult<Value> {
        self.read_claim(path, span)?;
        match self.resolve(path) {
            Some((slot, _)) => Ok(slot.value.clone()),
            // `read_claim` already answered for the place; it cannot vanish
            // between the claim and the copy.
            None => unsupported(format!("`{path}` does not denote a place at run time")),
        }
    }

    /// Everything a read of `path` *does* — the exclusivity check, the
    /// `Moved` trap, the rule fire, the provenance access — with the copy of
    /// the value left out.
    ///
    /// [`read_path`] is this plus the copy. Splitting them is what lets a
    /// method receiver be **lent** rather than copied (issue #24): the read
    /// is charged at exactly the moment and in exactly the order it always
    /// was, but a `List` receiver need not be deep-copied to be handed to
    /// `List.push`. See [`Machine::lend_path`].
    fn read_claim(&mut self, path: &Path, span: Span) -> EResult<()> {
        self.check_access(path, Access::Shared, span)?;
        let display = path.to_string();
        match self.resolve(path) {
            None => unsupported(format!("`{display}` does not denote a place at run time")),
            Some((_, None)) => {
                // #25: any access through a container whose home region was
                // freed wholesale faults, with the region named. Only
                // `Freed` — a live (or frozen) home reads clean, and the
                // `Moved` arm below still owns moved slots: home is not
                // identity, moves stay moves.
                if let Some(freed) = self.freed_region_on_read(path) {
                    return self.region_freed_fault(&format!("`{display}`"), freed, span);
                }
                self.fire(Rule::PlacePath, span, &format!("read `{display}`"));
                self.access_place(path, AccessKind::Read, span)
            }
            Some((_, Some((moved_at, depth)))) => {
                // Name the path that actually moved, which may be a *prefix* of
                // the one being read: moving `p.x` is what makes `p.x.n`
                // unreadable, and blaming `p.x.n` would send the reader to the
                // wrong line.
                let moved_path = prefix(path, depth);
                self.trap(
                    TrapKind::UseAfterMove,
                    Rule::UseAfterMove,
                    span,
                    format!("`{display}` was moved out and is uninitialized here"),
                    Some((moved_at, format!("`{moved_path}` moved here"))),
                )
            }
        }
    }

    /// Takes a lent receiver's value out of its slot, leaving [`Value::Unit`]
    /// behind for the duration of the call.
    ///
    /// The placeholder is unobservable: a receiver is only lent when it is a
    /// builtin container (`List`, `Map`, `str`), those dispatch to
    /// [`builtin::method`] and never to an impl block ([`Machine::method_of`]
    /// answers only for `Value::Struct`), and no container arm of
    /// `builtin::method` re-enters the machine. The arguments have already
    /// been evaluated by this point, so `xs.push(xs.len)` read `xs` through
    /// its parent while the value was still home.
    ///
    /// The slot's [`SlotState`] is deliberately untouched: lending is not
    /// moving. A receiver whose slot is `Moved` still lends its bytes, exactly
    /// as the copy path would have copied them.
    fn lend_path(&mut self, path: &Path) -> Value {
        match self.resolve(path) {
            Some((slot, _)) => std::mem::replace(&mut slot.value, Value::Unit),
            None => Value::Unit,
        }
    }

    /// Ends a lend that did not change the value: the bytes go back where they
    /// came from, and *nothing else happens*. `[mem.region.freeze.4]` (issue
    /// #20) says a method that only read its receiver performed a read, so
    /// there is no write to check, to guard against a freeze, or to fire.
    fn restore_lent(&mut self, path: &Path, value: Value) {
        if let Some((slot, _)) = self.resolve(path) {
            slot.value = value;
        }
    }

    /// Whether [`write_path`](Machine::write_path) would fault *before* it
    /// stored — the three guards it consults ahead of touching the slot.
    ///
    /// Pure: it reports, it never fires or traps. A lend is only taken when
    /// this is false, which is what keeps the lend honest. `write_path`
    /// faults before it writes, so on a faulting write-back the receiver has
    /// to survive the call intact — and a lend, having handed the value to
    /// the method, cannot produce the old one again. Asking first costs three
    /// cheap lookups; being wrong would cost a wrong program.
    fn writeback_would_trap(&self, path: &Path) -> bool {
        self.read_param_write(path).is_some()
            || self.access.conflict(path, Access::Exclusive).is_some()
            || self.write_refusal(path).is_some()
    }

    /// Moves out of a path: the source place becomes uninitialized
    /// (`[mem.tier0.move.1]`).
    fn move_path(&mut self, path: &Path, span: Span) -> EResult<Value> {
        self.check_access(path, Access::Exclusive, span)?;
        let display = path.to_string();
        let moved = match self.resolve(path) {
            None => {
                return unsupported(format!("`{display}` does not denote a place at run time"));
            }
            Some((_, moved)) => moved,
        };
        if let Some((moved_at, depth)) = moved {
            let moved_path = prefix(path, depth);
            return self.trap(
                TrapKind::UseAfterMove,
                Rule::UseAfterMove,
                span,
                format!("`{display}` was already moved out"),
                Some((moved_at, format!("`{moved_path}` moved here"))),
            );
        }
        // #25: a move reads the value out, so a `Freed` home faults exactly
        // as a read does — after the `Moved` check above, which keeps moves
        // moves (home is not identity).
        if let Some(freed) = self.freed_region_on_read(path) {
            return self.region_freed_fault(&format!("`{display}`"), freed, span);
        }
        let Some((slot, _)) = self.resolve(path) else {
            return unsupported(format!("`{display}` does not denote a place at run time"));
        };
        let value = slot.take_value(span);
        self.fire(Rule::Move, span, &format!("move out of `{display}`"));
        // A move ends the captured place's life as surely as a write ends
        // its loan (#36): the generation advances either way.
        self.note_captured_write(path, span);
        Ok(value)
    }

    /// Has any region ever been freed or frozen? While false, no home
    /// consult can fault, and the per-access walks skip themselves — the
    /// bounded perf tax the is16 acceptance pins.
    fn region_teeth(&self) -> bool {
        self.shared
            .region_teeth
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The slot `path`'s base binding currently holds, if it denotes one.
    fn base_slot(&self, path: &Path) -> Option<&Slot> {
        if path.frame == usize::MAX {
            self.globals.get(&path.base)
        } else {
            let frame = self.frames.get(path.frame)?;
            frame.scopes.iter().rev().find_map(|scope| {
                scope
                    .locals
                    .iter()
                    .rev()
                    .find(|(n, _)| *n == path.base)
                    .map(|(_, slot)| slot)
            })
        }
    }

    /// Walks the containers a `path` access touches — the base binding's
    /// value, then every projection target in turn — calling `visit` with
    /// the home of each one that carries a home (`Value::home`: structs
    /// since is08, lists since is16/#25). The walk stops early when `visit`
    /// answers, so the cost is one Vec-indexed region lookup per container
    /// the ACCESS actually reaches, never per element.
    ///
    /// `include_target` says whether the *resolved* value's own home counts.
    /// A read copies the resolved value itself, so it does; a write replaces
    /// the resolved slot's value and touches only the containers ABOVE it,
    /// so it does not — which is also what keeps rebinding the base (no
    /// projections) legal: such a write visits nothing.
    fn walk_homes<T>(
        &self,
        path: &Path,
        include_target: bool,
        mut visit: impl FnMut(RegionId) -> Option<T>,
    ) -> Option<T> {
        let mut current: Option<&Slot> = self.base_slot(path);
        for step in &path.projections {
            let slot = current?;
            if let Some(id) = slot.value.home()
                && let Some(answer) = visit(id)
            {
                return Some(answer);
            }
            current = match (&slot.value, step) {
                (Value::Struct { fields, .. }, Proj::Field(name)) => {
                    fields.iter().find(|(f, _)| f == name).map(|(_, slot)| slot)
                }
                (Value::Tuple(items), Proj::Index(i)) => {
                    usize::try_from(*i).ok().and_then(|index| items.get(index))
                }
                (Value::List(items, _, _), Proj::Index(i)) => {
                    usize::try_from(*i).ok().and_then(|index| items.get(index))
                }
                (Value::Map(pairs), Proj::Key(key)) => pairs
                    .iter()
                    .find(|(k, _)| k.to_string() == *key)
                    .map(|(_, slot)| slot),
                _ => None,
            };
        }
        if include_target
            && let Some(slot) = current
            && let Some(id) = slot.value.home()
        {
            return visit(id);
        }
        None
    }

    /// The first region a write through `path` would reach into whose state
    /// refuses the write: `Freed` first (#25's write arm — the storage died
    /// with the region, `[mem.region.intra.2]`), then `Frozen`
    /// (`[mem.region.freeze.1]` — the value-path half of the check the
    /// granule paths already run). Rebinding the base (no projections)
    /// replaces what the *binding* holds and touches no region storage, so
    /// it passes. The walk stops where the value tree does: a path that
    /// grows a map key still writes through every container above the
    /// growth point.
    fn write_refusal(&self, path: &Path) -> Option<(RegionId, RegionState)> {
        if !self.region_teeth() {
            return None;
        }
        let mut frozen = None;
        let store = &self.shared.store;
        let freed = self.walk_homes(path, false, |id| {
            match store.lock().expect("store lock").state(id) {
                Some(RegionState::Freed) => Some(id),
                Some(RegionState::Frozen) => {
                    frozen.get_or_insert(id);
                    None
                }
                _ => None,
            }
        });
        freed
            .map(|id| (id, RegionState::Freed))
            .or_else(|| frozen.map(|id| (id, RegionState::Frozen)))
    }

    /// #25's read half: the first `Freed` region a read of `path` reaches
    /// into. Only `Freed` faults here — reads through `Frozen` data are
    /// legal forever (`[mem.region.edge.imm]`), and a container merely
    /// OUTLIVING a scope while its region lives is legal; the fault is the
    /// dynamic complement of the compiler's E1010, at the ACCESS
    /// (`[mem.region.intra.2]`).
    fn freed_region_on_read(&self, path: &Path) -> Option<RegionId> {
        if !self.region_teeth() {
            return None;
        }
        let store = &self.shared.store;
        self.walk_homes(path, true, |id| {
            (store.lock().expect("store lock").state(id) == Some(RegionState::Freed)).then_some(id)
        })
    }

    /// The `[mem.region.intra.2]` access fault: `what` reaches into a region
    /// that was freed wholesale, with the region named and its creation site
    /// as the secondary span — the same shape the granule paths give
    /// use-after-free (`check_pool_region`).
    fn region_freed_fault<T>(&mut self, what: &str, id: RegionId, span: Span) -> EResult<T> {
        let label = self.store().label(id);
        let created = self
            .store()
            .region(id)
            .map(|region| (region.span, "the region was created here".to_owned()));
        self.region_fault(
            Rule::RegionFree,
            span,
            format!("{what} reaches into {label}, which was freed wholesale; the value died with the region"),
            created,
        )
    }

    /// The home consult at a container's own mutating methods — `List.push`
    /// and `List.pop`, the two CoW divergence points (is15). A method
    /// receiver's path has no projection for [`Machine::write_refusal`] to
    /// walk (the writeback is a whole-value store into the base), so the
    /// receiver value's own home is consulted here, before `Arc::make_mut`
    /// diverges anything: `Freed` faults `[mem.region.intra.2]` (#25's
    /// write arm), `Frozen` faults `[mem.region.freeze.1]` — exactly the
    /// consult a struct field write runs through its path walk.
    pub(crate) fn check_home_write(
        &mut self,
        home: Option<RegionId>,
        what: &str,
        span: Span,
    ) -> EResult<()> {
        let Some(id) = home else { return Ok(()) };
        if !self.region_teeth() {
            return Ok(());
        }
        let state = self.store().state(id);
        match state {
            Some(RegionState::Freed) => self.region_freed_fault(what, id, span),
            Some(RegionState::Frozen) => {
                let label = self.store().label(id);
                self.region_fault(
                    Rule::RegionFreeze,
                    span,
                    format!("{label} is frozen: `imm` data is immutable forever"),
                    None,
                )
            }
            _ => Ok(()),
        }
    }

    /// D39's callee-side write barrier (`[mem.tier0.mode.read]`): the span of
    /// the read-mode parameter `path` writes through, when it writes through
    /// one. The innermost binding of the base decides — scope 0 of a call
    /// frame holds exactly the parameters, so a body-scope shadow of the name
    /// is an ordinary local and passes.
    fn read_param_write(&self, path: &Path) -> Option<Span> {
        let frame = self.frames.get(path.frame)?;
        let scope_index = frame
            .scopes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, scope)| scope.locals.iter().any(|(n, _)| *n == path.base))
            .map(|(index, _)| index)?;
        if scope_index != 0 {
            return None;
        }
        frame
            .read_params
            .iter()
            .find(|(name, _)| *name == path.base)
            .map(|(_, span)| *span)
    }

    /// Writes through a path, making the place live again if it was moved from
    /// (`[mem.tier0.move.4]`).
    fn write_path(&mut self, path: &Path, value: Value, span: Span) -> EResult<()> {
        // D39's dynamic mirror: a write through a read-mode binding traps in
        // the exclusivity family (`[conf.trap.map]`), the kind
        // `[mem.tier0.excl.1]` already gives every read/write conflict —
        // wolfgang rejects the same write statically (s72's memory-family
        // code); one rule, two enforcement modes.
        if let Some(param_span) = self.read_param_write(path) {
            return self.trap(
                TrapKind::Exclusivity,
                Rule::ModeRead,
                span,
                format!(
                    "write to `{path}` through the read-mode parameter `{}`: the default \
                     (unwritten) mode reads a value that is immutable for the whole call \
                     (`[mem.tier0.mode.read]`, D39) — declare the parameter `mut` and pass \
                     `f(mut …)` at the call site to write through it",
                    path.base
                ),
                Some((
                    param_span,
                    format!("`{}` bound in read mode here", path.base),
                )),
            );
        }
        self.check_access(path, Access::Exclusive, span)?;
        // `[mem.region.intra.2]` / `[mem.region.freeze.1]`: a write into a
        // freed region's storage is #25's write arm, and frozen data is
        // immutable forever — through value paths as much as through
        // granules. Both checked before the slot is touched, so a trapping
        // write mutates nothing.
        match self.write_refusal(path) {
            Some((freed, RegionState::Freed)) => {
                let display = path.to_string();
                return self.region_freed_fault(&format!("the write to `{display}`"), freed, span);
            }
            Some((frozen, _)) => {
                let label = self.store().label(frozen);
                return self.region_fault(
                    Rule::RegionFreeze,
                    span,
                    format!("{label} is frozen: `imm` data is immutable forever"),
                    None,
                );
            }
            None => {}
        }
        let display = path.to_string();
        // A map grows on assignment to an absent key; every other path must
        // already denote a place.
        if self.slot_mut(path).is_none() && !self.insert_map_key(path, span)? {
            return unsupported(format!("`{display}` does not denote a place at run time"));
        }
        let Some(slot) = self.slot_mut(path) else {
            return unsupported(format!("`{display}` does not denote a place at run time"));
        };
        let was_moved = !slot.is_live();
        slot.state = SlotState::Live;
        slot.value = value;
        let rule = if was_moved {
            Rule::Reinit
        } else {
            Rule::Assign
        };
        self.fire(rule, span, &format!("write `{display}`"));
        self.note_captured_write(path, span);
        self.access_place(path, AccessKind::Write, span)
    }

    /// Advances a captured place's write generation (#36). Free until a
    /// program creates a capturing closure: the `captured_places` filter is
    /// empty and the walk never starts. Keyed by the owning frame's serial
    /// plus the base name — a write anywhere under the binding (an element,
    /// a field) conflicts with the whole-binding capture exactly as
    /// `[mem.model.path.disjoint]` reads prefixes.
    fn note_captured_write(&mut self, path: &Path, span: Span) {
        if self.captured_places.is_empty() || path.frame == usize::MAX {
            return;
        }
        let Some(frame) = self.frames.get(path.frame) else {
            return;
        };
        let key = (frame.serial, path.base.clone());
        if !self.captured_places.contains(&key) {
            return;
        }
        let entry = self.capture_gens.entry(key).or_insert((0, span));
        entry.0 += 1;
        entry.1 = span;
    }

    /// One Tier-0 place access, against the provenance forest.
    ///
    /// **Lazy by design.** A place that no borrow ever named has no tag tree,
    /// and this returns immediately — so the cost of the machine is bounded by
    /// the borrows a program actually creates rather than by the reads it
    /// performs. The first retag of a place is what mints its root.
    fn access_place(&mut self, path: &Path, kind: AccessKind, span: Span) -> EResult<()> {
        let key = self.place_key(path);
        let access = self.prov().access_place(&key, kind, span);
        match access {
            Ok(()) => {
                self.drain_prov();
                Ok(())
            }
            Err(finding) => self.ub(finding),
        }
    }

    /// Grows a map when a path's last step is an absent key. Returns whether it
    /// did.
    fn insert_map_key(&mut self, path: &Path, _span: Span) -> EResult<bool> {
        let Some(Proj::Key(key)) = path.projections.last() else {
            return Ok(false);
        };
        let key = key.clone();
        let mut parent = path.clone();
        parent.projections.pop();
        let Some(slot) = self.slot_mut(&parent) else {
            return Ok(false);
        };
        let Value::Map(pairs) = &mut slot.value else {
            return Ok(false);
        };
        pairs.push((Value::Str(key), Slot::live(Value::Unit)));
        Ok(true)
    }

    fn write_out(&mut self, text: &str) {
        self.shared
            .stdout
            .lock()
            .expect("stdout lock")
            .extend_from_slice(text.as_bytes());
        if self.shared.live_stdout {
            // The is12 pass-through: the bytes reach the terminal now, not
            // at program exit. A failed write is the pipe's condition, never
            // the program's fault — the buffered copy remains authoritative.
            use std::io::Write;
            let mut out = std::io::stdout();
            let _ = out.write_all(text.as_bytes());
            let _ = out.flush();
        }
        // Printed bytes fold into the scheduler's canonical-state digest, so
        // observably-diverged schedules never merge under state hashing.
        self.shared.sched.stdout_mark(text.as_bytes());
    }

    // -- Tier 1: the region machine (§3) -----------------------------------

    /// Every §3 fault surfaces through one trap kind.
    ///
    /// `[conf.trap.set]` is closed at twelve kinds and `[conf.trap.map]` maps
    /// the region family onto `region-fault`: use-after-free, an illegal
    /// cross-region edge, a write through a frozen or suspended path, and an
    /// open-discipline violation are one *kind* with different rules and
    /// messages. The rule (and through it the clause anchor) is what tells them
    /// apart, which is why every call names one.
    fn region_fault<T>(
        &mut self,
        rule: Rule,
        span: Span,
        message: impl Into<String>,
        secondary: Option<(Span, String)>,
    ) -> EResult<T> {
        self.trap(TrapKind::RegionFault, rule, span, message, secondary)
    }

    /// `[mem.region.create.1]`'s strategy: arena by default, `rc`, or `pool(T)`.
    fn strategy_of(&mut self, strategy: Option<&RegionStrategy>) -> Strategy {
        match strategy {
            None => Strategy::Arena,
            Some(RegionStrategy::Rc(_)) => Strategy::Rc,
            Some(RegionStrategy::Pool { ty, .. }) => Strategy::Pool(type_name(ty)),
        }
    }

    /// Charges one allocation to the current region (`[mem.region.create.3]`).
    ///
    /// "There is no `new` keyword. Allocation sites are struct literals,
    /// collection constructors, and closures" (`[mem.model.alloc]`) — so this
    /// is called from exactly those, and nowhere else.
    pub(crate) fn allocate(&mut self, span: Span, what: &str) -> RegionId {
        let id = self.store().charge();
        let label = self.store().label(id);
        self.fire(
            Rule::RegionAmbient,
            span,
            &format!("{what} allocated in {label}"),
        );
        id
    }

    /// The current (ambient) region.
    pub(crate) fn current_region(&self) -> RegionId {
        self.store().current()
    }

    /// Opens a region for a block (`[mem.region.open.1]`, `[mem.region.multiopen]`).
    fn enter_region(&mut self, id: RegionId, span: Span) -> EResult<()> {
        let entered = self.store().enter(id);
        match entered {
            Ok(()) => {
                let label = self.store().label(id);
                let open = self.store().open_set().len();
                self.fire(Rule::RegionOpen, span, &format!("open {label}"));
                if open > 2 {
                    // More than the program region and this one: the relaxation
                    // past Verona's single window is in force here.
                    self.fire(
                        Rule::RegionMultiopen,
                        span,
                        &format!("{open} regions open simultaneously"),
                    );
                }
                Ok(())
            }
            Err(refused) => {
                // The clause that refused is the clause the fault cites: an
                // open-discipline violation is `[mem.region.open.1]`, a
                // non-antichain open set is `[mem.region.multiopen]`.
                let rule = match refused {
                    region::EnterError::State(_) => Rule::RegionOpen,
                    region::EnterError::NotDisjoint(_) => Rule::RegionMultiopen,
                };
                let reason = refused.reason().to_owned();
                self.region_fault(rule, span, reason, None)
            }
        }
    }

    fn leave_region(&mut self, id: RegionId, span: Span) {
        self.store().leave(id);
        let label = self.store().label(id);
        let state = self
            .store()
            .state(id)
            .map_or_else(|| "gone".to_owned(), |state| state.to_string());
        self.fire(
            if state == "suspended" {
                Rule::RegionSuspended
            } else {
                Rule::RegionOpen
            },
            span,
            &format!("close {label} → {state}"),
        );
    }

    /// The §3 edge table, applied to every reference a stored value carries.
    ///
    /// > On **every** store of a reference, check the edge rule: intra-region
    /// > references unrestricted …; cross-region references only to a region's
    /// > bridge (`iso` edge) or into `Frozen` (`imm`) data.
    ///
    /// `into` is `None` when the destination is a **stack** local, which §3's
    /// table does not cover: a local may name any region, and that is what
    /// makes a region value first-class.
    fn check_edges(
        &mut self,
        into: Option<RegionId>,
        value: &Value,
        span: Span,
        what: &str,
    ) -> EResult<()> {
        for granule in region::refs_of(value) {
            let edge = self.store().classify_edge(into, granule);
            match edge {
                Edge::Intra => self.fire(
                    Rule::RegionIntra,
                    span,
                    &format!("{what}: intra-region reference — cycles and back-edges are safe"),
                ),
                Edge::Imm => self.fire(
                    Rule::RegionEdgeImm,
                    span,
                    &format!("{what}: reference into frozen data, legal from anywhere"),
                ),
                Edge::Tier2 => self.fire(
                    Rule::SharedRc,
                    span,
                    &format!("{what}: Tier-2 cell reference — §4 governs, not §3's table"),
                ),
                Edge::Iso(child) => {
                    if let Some(parent) = into {
                        self.store().adopt(parent, child);
                        self.assert_forest(span);
                    }
                    let label = self.store().label(child);
                    self.fire(
                        Rule::RegionEdgeIso,
                        span,
                        &format!("{what}: owning `iso` edge to {label} — at most one"),
                    );
                }
                Edge::Illegal(reason) => {
                    self.fire(Rule::RegionEdge, span, &format!("{what}: refused"));
                    return self.region_fault(
                        Rule::RegionEdge,
                        span,
                        format!(
                            "{what}: {reason} — the compiler rejects this edge statically (E1004)"
                        ),
                        None,
                    );
                }
            }
        }
        Ok(())
    }

    /// One-statement `freeze`: the guard must not live into the match arms.
    fn store_freeze(&mut self, id: RegionId) -> Result<Vec<RegionId>, String> {
        self.store().freeze(id)
    }

    /// `region name { … }` / `freeze region { … }` — the sugar (X4).
    ///
    /// Create, open, run, close, and then either free wholesale
    /// (`[mem.region.intra.2]`) or promote to `imm` (`[mem.region.freeze.1]`).
    /// The binder is declared *inside* the block's scope, which is what makes
    /// `region a { … in a { … } … }` — the corpus's own multiopen litmus —
    /// name something.
    fn eval_region_sugar(
        &mut self,
        name: Option<&crate::ast::Ident>,
        strategy: Option<&RegionStrategy>,
        body: &Block,
        span: Span,
        finish: SugarExit,
    ) -> EResult<Value> {
        let strategy = self.strategy_of(strategy);
        let binder = name.map(|ident| ident.name.clone());
        let id = self.store().create(binder.clone(), strategy.clone(), span);
        let label = self.store().label(id);
        self.fire(
            Rule::RegionCreate,
            span,
            &format!("create {label} with strategy {strategy} (sugar)"),
        );
        self.fire(
            Rule::RegionIdentity,
            span,
            &format!("{label} has identity in this machine and none in compiled code"),
        );

        self.push_scope();
        if let Some(binder) = &binder {
            let generation = self.store().generation(id);
            self.fire(
                Rule::RegionAffine,
                span,
                &format!("`{binder}` is an affine region value"),
            );
            self.declare(
                binder,
                Slot::live(Value::Region(RegionValue { id, generation })),
            );
        }
        let entered = self.enter_region(id, span);
        let result = match entered {
            Ok(()) => {
                let result = self.eval_block(body);
                self.leave_region(id, span);
                result
            }
            Err(signal) => Err(signal),
        };

        // The exit action runs on the trap path too: a region whose block
        // faulted is still freed, which is the invariant is06's crash-cleanup
        // oracle checks.
        match finish {
            SugarExit::Free => {
                let freed = self.store().free(id);
                // `[mem.prov.region]`: the wholesale free Disables every tag
                // tree of every allocation the region owned.
                self.prov().region_freed(&freed, span);
                if !freed.is_empty() {
                    let labels = freed
                        .iter()
                        .map(|id| self.store().label(*id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.fire(
                        Rule::RegionFree,
                        span,
                        &format!("`}}` frees {labels} wholesale"),
                    );
                }
            }
            SugarExit::Freeze => match self.store_freeze(id) {
                Ok(frozen) => {
                    // `[mem.prov.region]`: `freeze` transitions all its tags to
                    // Frozen, so a later write through any of them is §7/P2.
                    self.prov().region_frozen(&frozen, span);
                    let count = frozen.len();
                    self.fire(
                        Rule::RegionFreeze,
                        span,
                        &format!(
                            "{label} and {} owned region(s) are `imm` forever",
                            count - 1
                        ),
                    );
                }
                Err(reason) => {
                    self.pop_scope();
                    return self.region_fault(Rule::RegionClosedSubtree, span, reason, None);
                }
            },
        }
        self.pop_scope();
        self.assert_forest(span);
        result
    }

    /// `in r { … }` — set the current region for the block
    /// (`[mem.region.create.3]`).
    fn eval_in(&mut self, region: &Expr, body: &Block, span: Span) -> EResult<Value> {
        let value = self.eval(region)?;
        let id = self.region_id_of(&value, span, "`in`")?;
        self.enter_region(id, span)?;
        let result = self.eval_block(body);
        self.leave_region(id, span);
        result
    }

    /// `freeze r` (`[mem.region.freeze.1]`), including the anonymous
    /// `freeze region { … }` form the grammar exists for.
    fn eval_freeze(&mut self, operand: &Expr, span: Span) -> EResult<Value> {
        if let ExprKind::RegionSugar {
            name,
            strategy,
            body,
        } = &*operand.kind
        {
            // `freeze region { … }` "builds anonymously and promotes": the
            // block's value is the frozen data, and the region is never freed.
            return self.eval_region_sugar(
                name.as_ref(),
                strategy.as_ref(),
                body,
                span,
                SugarExit::Freeze,
            );
        }
        // `freeze r` *consumes* the region value (`[mem.region.create.2]`), so
        // the operand moves rather than being read.
        let value = match self.live_place(operand)? {
            Some(path) => self.move_path(&path, span)?,
            None => self.eval(operand)?,
        };
        let id = self.region_id_of(&value, span, "`freeze`")?;
        let outcome = self.store().freeze(id);
        match outcome {
            Ok(frozen) => {
                self.prov().region_frozen(&frozen, span);
                let label = self.store().label(id);
                self.fire(
                    Rule::RegionFreeze,
                    span,
                    &format!(
                        "{label} and {} owned region(s) are `imm` forever",
                        frozen.len().saturating_sub(1)
                    ),
                );
                let generation = self.store().generation(id);
                Ok(Value::Region(RegionValue { id, generation }))
            }
            Err(reason) => self.region_fault(Rule::RegionClosedSubtree, span, reason, None),
        }
    }

    /// The region a value names, checked live.
    ///
    /// A region value that outlived its region's wholesale free is *exactly*
    /// detectable — the generation moved — and that is the use-after-free this
    /// machine promises never to miss (`[mem.region.intra.2]`).
    fn region_id_of(&mut self, value: &Value, span: Span, what: &str) -> EResult<RegionId> {
        let Value::Region(handle) = value else {
            return unsupported(format!(
                "{what} needs a region value, got {}; region typing is the checker's",
                value.kind()
            ));
        };
        let live = self.store().generation(handle.id) == handle.generation
            && self.store().state(handle.id) != Some(RegionState::Freed);
        if live {
            // Cross-task claims (spec/03 §3, is06): a region `move`d through
            // a channel is the receiver's wholesale; any later touch by the
            // sender — through any surviving path — faults. The clause id the
            // sprint names ([conc.chan.staleuse]) is one spec/03 still owes;
            // the machine cites the channel family and says so.
            let claim = self.store().claim_of(handle.id);
            match claim {
                Some(region::TaskClaim::Task(owner)) if owner != self.task => {
                    let label = self.store().label(handle.id);
                    return self.region_fault(
                        Rule::ChanStale,
                        span,
                        format!(
                            "{what} touches {label}, which was `move`d through a channel and is \
                             owned by task {owner} now; the sender's access is stale \
                             (the E1005 family's dynamic cross-task half)"
                        ),
                        None,
                    );
                }
                Some(region::TaskClaim::InChannel) => {
                    let label = self.store().label(handle.id);
                    return self.region_fault(
                        Rule::ChanStale,
                        span,
                        format!(
                            "{what} touches {label}, which is in flight inside a channel; \
                             nothing may touch a region between its `move` send and its receive"
                        ),
                        None,
                    );
                }
                _ => {}
            }
            return Ok(handle.id);
        }
        let label = self.store().label(handle.id);
        let created = self
            .store()
            .region(handle.id)
            .map(|region| (region.span, "the region was created here".to_owned()));
        self.region_fault(
            Rule::RegionFree,
            span,
            format!(
                "{what} names {label}, which was freed wholesale; every allocation in it died \
                 with it, and this reference is dangling"
            ),
            created,
        )
    }

    // -- Tier 2: `shared`, `weak`, pools and handles (§4) ------------------

    /// Reads a pool slot through a handle, with the generation check every
    /// deref performs (`[mem.shared.handle.2]`).
    pub(crate) fn read_slot(&mut self, handle: HandleValue, span: Span) -> EResult<Value> {
        self.check_pool_region(handle.pool, span, false)?;
        self.race_check_pool(handle.pool, handle.index, false, span)?;
        let slot = self
            .store()
            .slot(handle.pool, handle.index, handle.generation)
            .cloned();
        match slot {
            Some(slot) if slot.life == region::SlotLife::Live => {
                let value = slot.value.clone();
                self.fire(
                    Rule::HandleAccess,
                    span,
                    &format!("pool#{}[{}] read", handle.pool, handle.index),
                );
                Ok(value)
            }
            Some(_) => {
                // Reserved but not yet `init`ed: the handle is valid and the
                // storage is uninitialized, which `[mem.tier0.move.2]` already
                // names — "use of an **uninitialized** … place".
                self.trap(
                    TrapKind::UseAfterMove,
                    Rule::HandleTwoPhase,
                    span,
                    format!(
                        "pool#{}[{}] was reserved and never initialized; `reserve` yields the \
                         handle, `init` fills the slot",
                        handle.pool, handle.index
                    ),
                    None,
                )
            }
            None => self.stale_handle(handle, span),
        }
    }

    pub(crate) fn stale_handle<T>(&mut self, handle: HandleValue, span: Span) -> EResult<T> {
        let current = self
            .store()
            .pool(handle.pool)
            .and_then(|pool| pool.slots.get(handle.index))
            .map_or_else(
                || "no such slot".to_owned(),
                |slot| format!("generation {}", slot.generation),
            );
        self.trap(
            TrapKind::StaleHandle,
            Rule::HandleStale,
            span,
            format!(
                "handle into pool#{} slot {} carries generation {}, the slot is at {current}; a \
                 stale handle is a deterministic fault in every profile, never UB",
                handle.pool, handle.index, handle.generation
            ),
            None,
        )
    }

    /// A pool's region must be live to read and open to write.
    pub(crate) fn check_pool_region(
        &mut self,
        pool: region::PoolId,
        span: Span,
        writing: bool,
    ) -> EResult<()> {
        let Some(id) = self.store().pool_region(pool) else {
            return unsupported(format!("pool#{pool} does not exist"));
        };
        // A pool anchored in a transferred region is subject to the same
        // cross-task claim its region is (spec/03 §3, is06).
        let claim = self.store().claim_of(id);
        match claim {
            Some(region::TaskClaim::Task(owner)) if owner != self.task => {
                let label = self.store().label(id);
                return self.region_fault(
                    Rule::ChanStale,
                    span,
                    format!(
                        "this handle reaches into {label}, which was `move`d through a channel \
                         and belongs to task {owner} now; the sender's access is stale"
                    ),
                    None,
                );
            }
            Some(region::TaskClaim::InChannel) => {
                let label = self.store().label(id);
                return self.region_fault(
                    Rule::ChanStale,
                    span,
                    format!(
                        "this handle reaches into {label}, which is in flight inside a channel"
                    ),
                    None,
                );
            }
            _ => {}
        }
        let label = self.store().label(id);
        let state = self.store().state(id);
        match state {
            Some(RegionState::Freed) => {
                // Detection is exact and it says so: the region that died, and
                // where it was created (`[mem.region.intra.2]`).
                let created = self
                    .store()
                    .region(id)
                    .map(|region| (region.span, "the region was created here".to_owned()));
                self.region_fault(
                    Rule::RegionFree,
                    span,
                    format!(
                        "this handle reaches into {label}, which was freed wholesale; the slot \
                         died with the region"
                    ),
                    created,
                )
            }
            Some(RegionState::Frozen) if writing => self.region_fault(
                Rule::RegionFreeze,
                span,
                format!("{label} is frozen: `imm` data is immutable forever"),
                None,
            ),
            Some(RegionState::Suspended) if writing => self.region_fault(
                Rule::RegionSuspended,
                span,
                format!(
                    "{label} is suspended here, so no live path may write into it; open it with \
                     `in`"
                ),
                None,
            ),
            _ => Ok(()),
        }
    }

    /// One pool-slot access against the race detector (`[conc.mm.race.3]`).
    ///
    /// Pool slots in an **unmoved** region are one of exactly two memories two
    /// tasks can share mutably in this machine (the other is raw allocations;
    /// see `sched::RaceKey`) — the shape the compiler rejects statically
    /// (E1101/E1102) and the dynamic machine detects exactly.
    pub(crate) fn race_check_pool(
        &mut self,
        pool: region::PoolId,
        index: usize,
        write: bool,
        span: Span,
    ) -> EResult<()> {
        if !self.shared.sched.ever_concurrent() {
            return Ok(());
        }
        let report =
            self.shared
                .sched
                .race_check(self.task, sched::RaceKey::Pool(pool, index), 0, 1, write);
        self.drain_sched();
        if let Some(report) = report {
            return self.trap(
                TrapKind::Race,
                Rule::RaceDetect,
                span,
                format!(
                    "data race: this {} of pool#{pool}[{index}] conflicts with an unordered {} \
                     by {} — share the region by `move`, `freeze` or a `sync` wrapper (D14)",
                    if write { "write" } else { "read" },
                    if report.other_write { "write" } else { "read" },
                    report.other_task
                ),
                None,
            );
        }
        Ok(())
    }

    /// The acyclicity assertion of `[mem.shared.rc.2]`, run at strong-edge
    /// creation.
    ///
    /// It is an **assertion**, not a trap: the rule is a compile error (E1006)
    /// and `[mem.ub.defined]` lists it as such, while `[conf.trap.set]` is
    /// closed at eleven kinds and has none for it. Inventing one would put this
    /// implementation's guess into a differential comparison. The assertion
    /// fires into the trace and is asserted directly in tests; the missing
    /// dynamic counterpart is a spec finding, recorded in
    /// `docs/approximation-contract.md`.
    pub(crate) fn assert_shared_acyclic(
        &mut self,
        from: region::CellId,
        value: &Value,
        span: Span,
    ) {
        for granule in region::refs_of(value) {
            let Ref::Shared(to) = granule else { continue };
            if self.store().would_cycle(from, to) {
                self.fire(
                    Rule::SharedAcyclic,
                    span,
                    &format!(
                        "ASSERTION: a strong edge shared#{from} → shared#{to} would close a cycle \
                         (the compiler's E1006)"
                    ),
                );
            } else {
                self.fire(
                    Rule::SharedAcyclic,
                    span,
                    &format!("strong edge shared#{from} → shared#{to} keeps the graph acyclic"),
                );
            }
            self.store().add_strong_edge(from, to);
        }
    }

    // -- calls -------------------------------------------------------------

    /// What a call produced: its value, and the *final* values of its
    /// parameters.
    ///
    /// `mut` is exclusive **inout** (`[mem.tier0.mode.mut]`), and with value
    /// semantics the only way inout is observable is call-by-value-result: the
    /// callee's parameter is copied back into the caller's place when the call
    /// ends. Exclusivity for the call's extent is exactly what makes that
    /// equivalent to sharing the storage — which is why the check and the
    /// write-back are the same mechanism.
    fn call_fn(
        &mut self,
        decl: &FnDecl,
        module: &str,
        args: Vec<Value>,
        span: Span,
    ) -> EResult<Applied> {
        // `comptime fn` is evaluated by the *compiler's* comptime engine (s16):
        // an absolute sandbox, fuel/heap/depth budgets, memoization on argument
        // hashes. `comptime` is a **reserved forward namespace**
        // (`[conf.anchor.ns]`) — no document this implementation reads pins any
        // of it — so running the body as an ordinary function would not be a
        // conservative approximation of comptime evaluation, it would be a
        // *different execution* reported under comptime's name. It also cannot
        // reproduce the outcome the corpus pins: a budget violation is a
        // diagnostic, and this machine's only bound is `Machine::FUEL`, which
        // arrives after fifty million steps rather than after the budget.
        if decl
            .quals
            .iter()
            .any(|qual| matches!(qual, crate::ast::FnQual::Comptime(_)))
        {
            return unsupported(format!(
                "`{}` is a `comptime fn`; compile-time evaluation with its sandbox and budgets is \
                 the compiler's engine (s16), and nothing in `spec/` pins it — the `comptime` \
                 namespace is still a reserved forward one",
                decl.name.name
            ));
        }
        let Some(body) = &decl.body else {
            return unsupported(format!(
                "`{}` has no body (extern or trait signature); the interpreter has no C ABI \
                 and no trait-method dispatch",
                decl.name.name
            ));
        };
        if args.len() != decl.params.len() {
            // Arity is the static type checker's (E0402). Declining is honest;
            // inventing a trap would put a static rejection in the run rung.
            return unsupported(format!(
                "`{}` takes {} argument(s) and was called with {}; argument arity is the type \
                 checker's (E0402), not this machine's",
                decl.name.name,
                decl.params.len(),
                args.len()
            ));
        }
        if self.frames.len() > 512 {
            return unsupported("call depth exceeded 512 frames".to_owned());
        }

        self.fire(Rule::Call, span, &format!("call `{}`", decl.name.name));
        // D39 (`[mem.tier0.mode.read]`): parameters whose mode is unwritten
        // are read bindings — immutable for the whole call. The frame carries
        // the watch list for the callee-side write barrier in `write_path`.
        let read_params: Vec<(String, Span)> = decl
            .params
            .iter()
            .filter(|param| param.mode.is_none())
            .map(|param| {
                let name = match &param.kind {
                    ParamKind::Named { name, .. } => name.name.clone(),
                    ParamKind::SelfParam { .. } => "self".to_owned(),
                };
                (name, param.span)
            })
            .collect();
        let serial = self.mint_frame_serial();
        self.frames.push(Frame {
            module: module.to_owned(),
            serial,
            scopes: vec![Scope::default()],
            row: crate::sema::declared_raise_tags(decl),
            read_params,
        });
        let retags = std::mem::take(&mut self.pending_retags);
        let frame = self.frames.len() - 1;
        for (index, (param, value)) in decl.params.iter().zip(args).enumerate() {
            let name = match &param.kind {
                ParamKind::Named { name, .. } => name.name.clone(),
                ParamKind::SelfParam { .. } => "self".to_owned(),
            };
            let value = match &param.kind {
                ParamKind::Named { ty, .. } => coerce(value, Some(ty)),
                ParamKind::SelfParam { .. } => value,
            };
            // A `mut` parameter *is* the borrow: the callee's reads and writes
            // of it go through the child tag its caller minted, so a write is a
            // child write (Reserved → Active) and the caller's own access
            // during the call is foreign.
            if let Some(Some(retag)) = retags.get(index) {
                let key = format!("t{}:{frame}:{name}", self.task);
                self.prov().bind_place(&key, retag.alloc, retag.tag);
            }
            self.declare(&name, Slot::live(value));
        }
        self.drain_prov();

        let result = self.eval_block(body);
        let frame = self.frame();
        self.access.release_frame(frame);
        // The parameters' final values, read out before the frame dies — this
        // is the "result" half of call-by-value-result.
        let params = decl
            .params
            .iter()
            .map(|param| {
                let name = match &param.kind {
                    ParamKind::Named { name, .. } => name.name.as_str(),
                    ParamKind::SelfParam { .. } => "self",
                };
                self.frames
                    .last()
                    .and_then(|frame| {
                        frame.scopes.iter().rev().find_map(|scope| {
                            scope
                                .locals
                                .iter()
                                .rev()
                                .find(|(n, _)| n == name)
                                .map(|(_, slot)| slot.value.clone())
                        })
                    })
                    .unwrap_or(Value::Unit)
            })
            .collect();
        self.frames.pop();
        let task = self.task;
        self.prov().drop_frame(task, frame);

        let value = match result {
            Ok(value) | Err(Signal::Return(value)) => value,
            Err(other) => return Err(other),
        };
        // The declared return type types the value it returns (issue #14's
        // third shape): `math.int_max() - 1` is `int` arithmetic because
        // `int_max` says `-> int`, wherever the callee lives. Without this a
        // returned literal stayed a literal and the caller's operator
        // defaulted it.
        let value = match &decl.ret {
            Some(ret) => coerce(value, Some(&ret.ty)),
            None => value,
        };
        Ok(Applied { value, params })
    }

    /// Evaluates the arguments of a call, applying call-site modes (X1).
    ///
    /// Exclusivity is checked as the arguments accumulate, which is what makes
    /// `f(mut a.x, mut a.y)` legal and `f(mut a, mut a.x)` a trap.
    fn eval_args(&mut self, args: &[Arg]) -> EResult<Args> {
        self.eval_args_for(args, Callee::Wolf, None)
    }

    /// D52's declared-row-first resolution at one checked position
    /// (`[gram.expr.tagident]`): a bare lowercase identifier the position's
    /// expected DECLARED row spells resolves as the tag — the raise value,
    /// not a name lookup. A *local binding* shadows it (the tightest scopes
    /// win, as everywhere; W0305's fire-at-use in `lint` warns there), while
    /// module items, imports and prelude names LOSE to the declared tag —
    /// which is why this is asked before any of those lookups would run.
    /// A name the row does not spell is `None`: the caller's ordinary
    /// resolution proceeds, and an unresolvable name keeps its refusal
    /// (`rows/negative/tag_undeclared_arg.lu` — E0301's fact, stated here
    /// as the honest unsupported).
    fn declared_row_tag(&mut self, expr: &Expr, row: &[String]) -> Option<Value> {
        let ExprKind::Path(path) = &*expr.kind else {
            return None;
        };
        if !path.is_single() {
            return None;
        }
        let name = &path.segments[0].name;
        if !name.starts_with(char::is_lowercase) || !row.contains(name) || self.local_exists(name) {
            return None;
        }
        let name = name.clone();
        self.fire(
            Rule::ErrRows,
            expr.span,
            &format!("declared-row tag `{name}` at a checked position (D52)"),
        );
        Some(Value::Error(Box::new(ErrorValue {
            tag: name,
            payload: Vec::new(),
            // A name reached through a checked position's declared row is a
            // raise by construction, whatever else it spells.
            enum_variant: false,
            // The row rides with the value (wolf-interp#29): handler arm
            // resolution asks the VALUE which row it came through.
            row: row.to_vec(),
        })))
    }

    /// As [`Machine::eval_args`], knowing what is being called.
    ///
    /// The distinction exists for exactly one reason and it is a spec reason:
    /// `[mem.prov.tag]`'s retag point is "`mut`/`read` **parameter** entry",
    /// and a C function has no wolf parameters and no wolf modes. Retagging its
    /// arguments would invent a `read` borrow the language never promised and
    /// then report the callee's own write through it as §7/P2 — a *spurious*
    /// UB verdict on `corpus/ffi.lu`, in the one direction the approximation
    /// contract forbids. What `[mem.boundary.ffi]` and `[mem.prov.expose]` say
    /// happens instead is exposure: "wildcard pointers from FFI behave as
    /// exposed", so a pointer handed to C joins the exposed set and the call is
    /// a foreign havoc that angelic resolution already models.
    ///
    /// `param_rows` carries the callee's declared parameter-row tags by index
    /// where a declaration is in sight (a direct call to a named `fn`) — the
    /// expected rows of D52's argument position. `None` means no declaration
    /// is visible from the call site (a closure value, the builtin surface, a
    /// trait-qualified call whose dispatch needs the arguments first): those
    /// positions keep ordinary resolution, honestly narrower than the clause,
    /// exactly as wide as what this machine can know without a type checker.
    fn eval_args_for(
        &mut self,
        args: &[Arg],
        callee: Callee,
        param_rows: Option<&[Vec<String>]>,
    ) -> EResult<Args> {
        let mut values = Vec::with_capacity(args.len());
        let mut writebacks = Vec::new();
        let mut held = 0usize;
        let mut retags: Vec<(usize, PendingRetag)> = Vec::new();
        let mut protectors: Vec<prov::TagId> = Vec::new();

        for (index, arg) in args.iter().enumerate() {
            match arg.mode {
                Some(ParamMode::Mut) => {
                    let path = self.place_of(&arg.expr)?;
                    self.check_access(&path, Access::Exclusive, arg.span)?;
                    let value = self.read_path(&path, arg.span)?;
                    self.fire(
                        Rule::ModeMut,
                        arg.span,
                        &format!("`mut {path}` held for the call"),
                    );
                    // `[mem.tier0.excl.2]`: `f(mut a.x, mut a.y)` is legal by
                    // `[mem.model.path.disjoint]`. The rule fires when it is
                    // actually being *used* — two exclusive holds over one
                    // base — which is the O1 alias fact made observable.
                    if let Some(sibling) = self.access.disjoint_sibling(&path) {
                        let detail = format!("`{path}` and `{}` are disjoint places", sibling.path);
                        self.fire(Rule::ExclusivityDisjoint, arg.span, &detail);
                    }
                    self.access.push(Held {
                        path: path.clone(),
                        access: Access::Exclusive,
                        span: arg.span,
                        why: HeldWhy::Call,
                    });
                    held += 1;
                    // `[mem.prov.tag]`: `mut` parameter entry is a retag point,
                    // and "parameter entry is protector-equivalent: the tag is
                    // protected for the whole call". The child is *Reserved*,
                    // not Active — creation is not a use, which is the
                    // two-phase window `corpus/memory/prov_two_phase.lu` needs.
                    let value = self.pass_argument(
                        callee,
                        &path,
                        value,
                        RetagKind::Mutable,
                        index,
                        &mut retags,
                        &mut protectors,
                        arg.span,
                    );
                    writebacks.push((index, path));
                    values.push(value);
                }
                Some(ParamMode::Take) => {
                    // `take` consumes: the argument moves at the call site
                    // (`[mem.tier0.mode.take]` → `[mem.tier0.move.1]`).
                    let value = match self.live_place(&arg.expr)? {
                        Some(path) => {
                            self.fire(Rule::ModeTake, arg.span, &format!("`take {path}`"));
                            self.move_path(&path, arg.span)?
                        }
                        None => self.eval(&arg.expr)?,
                    };
                    values.push(value);
                }
                None => {
                    // D52's argument position (`[gram.expr.tagident]`): the
                    // callee's declared parameter row is the expected row,
                    // asked FIRST — a local place would have shadowed inside
                    // `declared_row_tag`, and everything else (module items,
                    // prelude names, the unresolvable) loses to the tag.
                    if let Some(row) = param_rows.and_then(|rows| rows.get(index))
                        && let Some(value) = self.declared_row_tag(&arg.expr, row)
                    {
                        values.push(value);
                        continue;
                    }
                    // Default mode: immutable for the whole call, caller retains
                    // (`[mem.tier0.mode.read]`).
                    let value: Value = match self.live_place(&arg.expr)? {
                        Some(path) => {
                            self.check_access(&path, Access::Shared, arg.span)?;
                            let value = self.read_path(&path, arg.span)?;
                            self.fire(Rule::ModeRead, arg.span, &format!("`read {path}`"));
                            held += 1;
                            // `read` parameter entry retags too, with a Frozen
                            // child: the caller's place is immutable for the
                            // whole call, which is O2 — the SB "holy grail"
                            // load-hoisting fact — as a protector.
                            let value = self.pass_argument(
                                callee,
                                &path,
                                value,
                                RetagKind::Shared,
                                index,
                                &mut retags,
                                &mut protectors,
                                arg.span,
                            );
                            self.access.push(Held {
                                path,
                                access: Access::Shared,
                                span: arg.span,
                                why: HeldWhy::Call,
                            });
                            value
                        }
                        None => self.eval(&arg.expr)?,
                    };
                    values.push(value);
                }
            }
        }
        // `[mem.model.order]`: "arguments left-to-right before the call". The
        // loop above is that order, and this records it so the trace can be
        // read as evidence rather than taken on trust.
        if !args.is_empty() {
            self.fire(
                Rule::EvalStrictOrder,
                args[0].span,
                &format!("{} argument(s) evaluated left to right", args.len()),
            );
        }
        Ok(Args {
            values,
            writebacks,
            held,
            retags,
            protectors,
        })
    }

    /// One argument crossing a call boundary.
    #[allow(clippy::too_many_arguments)]
    fn pass_argument(
        &mut self,
        callee: Callee,
        path: &Path,
        value: Value,
        kind: RetagKind,
        index: usize,
        retags: &mut Vec<(usize, PendingRetag)>,
        protectors: &mut Vec<prov::TagId>,
        span: Span,
    ) -> Value {
        match callee {
            Callee::Wolf => self.retag_argument(path, value, kind, index, retags, protectors, span),
            Callee::C => self.expose_to_c(value, span),
        }
    }

    /// A value handed across the C membrane.
    ///
    /// `[mem.prov.expose]`: "Wildcard pointers from FFI behave as exposed."
    /// The pointer keeps its tag — copies of raw pointers are unrestricted
    /// (`[mem.unsafe.raw.1]`) — and the *allocation* gains it as a resolution
    /// candidate, so anything C hands back later resolves angelically to it
    /// rather than to nothing.
    fn expose_to_c(&mut self, value: Value, span: Span) -> Value {
        if let Value::Raw(ptr) = value {
            self.prov().expose(ptr, span);
            self.fire(
                Rule::BoundaryFfi,
                span,
                &format!(
                    "{ptr} crosses the C membrane: exposed, and the call is a foreign \
                     havoc over what it reaches"
                ),
            );
            self.drain_prov();
        }
        value
    }

    /// The retag at parameter entry (`[mem.prov.tag]`).
    ///
    /// Two shapes, because two things can carry provenance across a call:
    ///
    /// - a **raw pointer value**, whose tag travels *in* the value — the callee
    ///   receives a pointer carrying the fresh child, so a write through a
    ///   `read`-mode pointer is §7/P2 and a foreign write during the extent is
    ///   §7/P1 at the write;
    /// - a **Tier-0 place**, whose tag lives beside it in the provenance
    ///   forest. Only `mut` binds the callee's parameter to the child: under
    ///   MVS a `read` parameter is the callee's own copy, so its Frozen child
    ///   is a witness of the caller-side promise rather than an access path
    ///   (`docs/approximation-contract.md` §7.3).
    #[allow(clippy::too_many_arguments)]
    fn retag_argument(
        &mut self,
        path: &Path,
        value: Value,
        kind: RetagKind,
        index: usize,
        retags: &mut Vec<(usize, PendingRetag)>,
        protectors: &mut Vec<prov::TagId>,
        span: Span,
    ) -> Value {
        if let Value::Raw(ptr) = value {
            let (Some(alloc), Prov::Tag(parent)) = (ptr.alloc, ptr.prov) else {
                // A wildcard pointer takes on no obligations at a call
                // boundary: `[mem.unsafe.raw.1]`, and D11's "simpler than safe".
                return value;
            };
            let child = self
                .prov()
                .retag(alloc, parent, kind, true, "parameter", span);
            protectors.push(child);
            self.drain_prov();
            return Value::Raw(RawPtr {
                prov: Prov::Tag(child),
                ..ptr
            });
        }
        let key = self.place_key(path);
        let (alloc, child) = self.prov().retag_place(&key, kind, true, span);
        protectors.push(child);
        retags.push((
            index,
            PendingRetag {
                alloc,
                tag: child,
                bind: kind == RetagKind::Mutable,
            },
        ));
        self.drain_prov();
        value
    }

    fn finish_args(
        &mut self,
        writebacks: &[(usize, Path)],
        final_values: &[Value],
        held: usize,
        protectors: &[prov::TagId],
        span: Span,
    ) {
        self.access.release(held);
        // The call's extent ended: every protector minted for it comes off,
        // and the tags nothing can reach any more go away
        // (`[mem.prov.tag]`'s "protected for the whole call").
        for tag in protectors {
            self.prov().unprotect(*tag, span);
        }
        self.prov().prune();
        self.drain_prov();
        for (index, path) in writebacks {
            let Some(value) = final_values.get(*index) else {
                continue;
            };
            if let Some(slot) = self.slot_mut(path) {
                slot.state = SlotState::Live;
                slot.value = value.clone();
            }
        }
        if !writebacks.is_empty() {
            self.fire(Rule::ModeMut, span, "`mut` arguments written back");
        }
    }

    // -- statements & blocks -----------------------------------------------

    fn eval_block(&mut self, block: &Block) -> EResult<Value> {
        self.push_scope();
        let result = self.eval_block_body(block);
        // The killed-proc rule (`[conc.proc.kill]`, D14's decided distinction):
        // a KILLED proc's frames unwind without running any further user code —
        // `defer`/`errdefer` included. Contrast cancellation, which flows
        // through the ordinary error-value paths below and runs them
        // (`[conc.cancel.defer]`).
        if matches!(result, Err(Signal::ProcKilled | Signal::Exit(_))) {
            self.pop_scope();
            return result;
        }
        // `errdefer` runs when the scope is left on the error path, and only
        // then (`[err.errdefer]`); `defer` runs on both.
        let errored = match &result {
            Ok(value) => value.is_error(),
            Err(Signal::Return(value)) => value.is_error(),
            Err(Signal::ProcKilled | Signal::Exit(_)) => unreachable!("returned above"),
            Err(Signal::Trap(_) | Signal::Ub(_) | Signal::Unsupported(_)) => true,
            Err(Signal::Break(_) | Signal::Continue) => false,
        };
        let defers = self.run_defers(errored);
        // A region value in the block's result moves OUT of the dying scope
        // (wolf-interp#35): name it so teardown skips its wholesale free.
        let escapes = Machine::escaping_regions(&result);
        self.pop_scope_escaping(&escapes);
        match (result, defers) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(signal), _) => Err(signal),
            (Ok(_), Err(signal)) => Err(signal),
        }
    }

    fn eval_block_body(&mut self, block: &Block) -> EResult<Value> {
        for stmt in &block.stmts {
            self.exec(stmt)?;
        }
        match &block.tail {
            // `[gram.expr.block]`: the block's value is its tail expression.
            Some(tail) => {
                let value = self.eval(tail)?;
                self.fire(Rule::Block, block.span, "block yields its tail");
                Ok(value)
            }
            None => Ok(Value::Unit),
        }
    }

    fn run_defers(&mut self, errored: bool) -> EResult<()> {
        let Some(frame) = self.frames.last_mut() else {
            return Ok(());
        };
        let Some(scope) = frame.scopes.last_mut() else {
            return Ok(());
        };
        let defers = std::mem::take(&mut scope.defers);
        // LIFO (`[mem.shared.drop.1]`): scope-exit effects run in reverse
        // registration order.
        for (on_error, expr) in defers.into_iter().rev() {
            if on_error && !errored {
                continue;
            }
            let rule = if on_error {
                Rule::ErrDefer
            } else {
                Rule::DeferLifo
            };
            self.fire(rule, expr.span, "scope-exit effect");
            self.eval(&expr)?;
        }
        Ok(())
    }

    fn exec(&mut self, stmt: &Stmt) -> EResult<()> {
        self.step()?;
        match &stmt.kind {
            StmtKind::Binding(binding) => self.exec_binding(binding),
            StmtKind::Assign { place, op, value } => self.exec_assign(place, *op, value, stmt.span),
            StmtKind::Defer { on_error, expr } => {
                if let Some(frame) = self.frames.last_mut()
                    && let Some(scope) = frame.scopes.last_mut()
                {
                    scope.defers.push((*on_error, expr.clone()));
                }
                Ok(())
            }
            StmtKind::AssumeNoalias(operands) => self.exec_assume_noalias(operands, stmt.span),
            StmtKind::Expr(expr) => {
                self.eval(expr)?;
                Ok(())
            }
            StmtKind::Item(item) => {
                // #38 (wolf-lang#116b): a nested named `fn` RESOLVES — it
                // binds like a `let` whose value is a capture-free fn value
                // (the closure recipe with a declared signature). Every other
                // item kind in statement position stays inert, as before.
                if let crate::ast::ItemKind::Fn(decl) = &item.kind {
                    return self.exec_nested_fn(decl, stmt.span);
                }
                Ok(())
            }
        }
    }

    /// A nested named `fn` in statement position (#38, the compiler's #116b
    /// twin): the capture-free shape checks as a fn value with a name and
    /// binds like a `let`, so a direct call, a pass to a higher-order fn,
    /// and a call through a later binding all resolve.
    ///
    /// The scoped-out shapes refuse BY NAME — the counterparty's scoped v1
    /// refuses the same set, so an honest `unsupported` here is parity, not
    /// a gap: captures of enclosing locals (`typecheck/nested_fn_capture.lu`
    /// pins the refusal; "bind a closure instead"), generics, an error row
    /// on the nested return, parameter modes and `self`.
    fn exec_nested_fn(&mut self, decl: &FnDecl, span: Span) -> EResult<()> {
        let name = &decl.name.name;
        if !decl.generics.is_empty() {
            return unsupported(format!(
                "nested fn `{name}` declares generics; a nested fn is the capture-free \
                 closure recipe and its scoped v1 holds no type parameters — lift it to \
                 the module (#38)"
            ));
        }
        // Both row spellings on the return refuse: the postfix row
        // (`-> int ! {none}` — RetType's own or the Fallible type's) and the
        // bang union (`-> !int`).
        let rowed_return = decl.ret.as_ref().is_some_and(|ret| {
            ret.row.is_some()
                || matches!(
                    &*ret.ty.kind,
                    TypeKind::ErrorUnion(_) | TypeKind::Fallible { .. }
                )
        });
        if rowed_return {
            return unsupported(format!(
                "nested fn `{name}` declares an error row on its return; rows on a nested \
                 return are outside the scoped v1 — lift it to the module (#38)"
            ));
        }
        let Some(body) = &decl.body else {
            return unsupported(format!(
                "nested fn `{name}` has no body; only `extern` items are bodyless and the \
                 extern surface is not a statement's"
            ));
        };
        let mut params = Vec::new();
        for param in &decl.params {
            let ParamKind::Named { name: pname, .. } = &param.kind else {
                return unsupported(format!(
                    "nested fn `{name}` takes `self`; methods belong to impl blocks"
                ));
            };
            if param.mode.is_some() {
                return unsupported(format!(
                    "nested fn `{name}` declares a parameter mode; closure-recipe \
                     parameters carry none in the scoped v1 (#38)"
                ));
            }
            params.push(pname.name.clone());
        }
        let body_expr = Expr {
            kind: Box::new(ExprKind::Block(body.clone())),
            span: decl.span,
            anchor: "gram.item.fn",
        };
        // The capture question, asked the way `eval_closure` asks it: the
        // body's free single names, minus the parameters. A free name that is
        // a live LOCAL of the enclosing frame is a capture, and captures are
        // the refused shape — an environment belongs to closure VALUES a
        // binding claims. Module items, globals and prelude names are not
        // captures; they resolve at call time as they do everywhere.
        let mut bound: BTreeSet<String> = params.iter().cloned().collect();
        let mut used = BTreeSet::new();
        crate::lint::free_names(&body_expr, &mut bound, &mut used);
        for free in &used {
            if self.local_exists(free) {
                return unsupported(format!(
                    "nested fn `{name}` captures the enclosing local `{free}` (bind a \
                     closure instead) — the scoped v1 is capture-free on every lane (#38)"
                ));
            }
        }
        self.fire(
            Rule::Call,
            span,
            &format!("nested fn `{name}` binds as a capture-free fn value, like a `let`"),
        );
        self.declare(
            name,
            Slot::live(Value::Closure(Box::new(ClosureValue {
                params,
                body: body_expr,
                captures: Vec::new(),
                loans: Vec::new(),
            }))),
        );
        Ok(())
    }

    fn exec_binding(&mut self, binding: &Binding) -> EResult<()> {
        // D52's annotated-`let`/`var` position (`[gram.expr.tagident]`): the
        // annotation's declared row is the initializer's expected row, asked
        // before ordinary resolution — locals shadow, module items lose.
        // `rows/tag_let_position.lu` pins the spec reading (bind, run the
        // handler); the compiler's CHECKED lane mishandles this shape
        // (wolf-lang#122) and this machine matches spec/native, not checked.
        if let Some(ty) = &binding.ty {
            let row = crate::sema::type_tags(ty);
            if !row.is_empty()
                && let Some(value) = self.declared_row_tag(&binding.value, &row)
            {
                return self.bind_pattern(&binding.pattern, value);
            }
        }
        // D54.1 `[type.numlit.adopt]`: a float-typed binding is a float
        // expectation. Propagate it into the initializer so an integer-literal
        // term adopts float BEFORE its operators run — `let x: f64 = 1 / 2` is
        // `0.5`, float division, not the C integer-division footgun
        // (`[type.numlit.propagate]`). Its int twin (`let n: int = 1 / 2` → `0`)
        // takes the ordinary path and defaults to int.
        let expect_float = binding.ty.as_ref().and_then(float_ty_name).is_some();
        // Initialization moves (`[mem.tier0.move.1]`) unless the source is a
        // `Copy`-shaped value or is not a place at all.
        let value = if expect_float {
            self.eval_float_expected(&binding.value)?
        } else {
            self.eval_for_init(&binding.value)?
        };
        // The negatives D54 pins hard. Adoption is a LITERAL's privilege
        // (`[type.numlit.value]`): a concrete int VALUE never becomes a float —
        // `let n = 0; let x: f64 = n` is refused, the conversion is spelled
        // `n as f64`. And adoption is one-directional (`[type.numlit.adopt]`): a
        // `{float}` literal never satisfies an integer expectation —
        // `let n: int = 0.0` is refused. A tree-walk has no static E0401, so the
        // honest refusal is `unsupported` (the census conservatism class);
        // never a silent adopt.
        if expect_float && matches!(&value, Value::Int(_, ty) if !ty.literal) {
            return unsupported(
                "a concrete `int` value does not adopt a float type; adoption is a literal's \
                 privilege (D54.3, `[type.numlit.value]`) — spell the conversion `as f64`",
            )
            .map(|_: Value| ());
        }
        if !expect_float
            && binding
                .ty
                .as_ref()
                .is_some_and(|ty| int_of_type(ty).is_some())
            && matches!(&value, Value::Float(_))
        {
            return unsupported(
                "a `{float}` literal never satisfies an integer expectation; adoption is \
                 one-directional (D54.1, `[type.numlit.adopt]`) — `0.0` is not an integer",
            )
            .map(|_: Value| ());
        }
        // D58's negative ([type.char]): `char` is not an integer type and
        // adopts no numeric literal — `let c: char = 65` is the checker's
        // type error. A tree-walk has no static code for it, so the honest
        // refusal is `unsupported` (the conservatism class), never a silent
        // adoption; the conversion is spelled `65 as char`, and it traps on
        // a non-scalar. The reverse direction is refused on the same
        // grounds: a `char` value satisfies no numeric expectation.
        let char_expected = binding
            .ty
            .as_ref()
            .and_then(crate::sema::head_name)
            .is_some_and(|name| name == "char");
        if char_expected && matches!(&value, Value::Int(..) | Value::Float(_)) {
            return unsupported(
                "`char` adopts no numeric literal (D58, `[type.char]`) — spell the \
                 conversion `n as char`, which traps on a non-scalar",
            )
            .map(|_: Value| ());
        }
        if !char_expected
            && binding
                .ty
                .as_ref()
                .is_some_and(|ty| int_of_type(ty).is_some() || float_ty_name(ty).is_some())
            && matches!(&value, Value::Char(_))
        {
            return unsupported(
                "a `char` value satisfies no numeric expectation (D58, `[type.char]`) — \
                 the total direction is spelled `c as int`",
            )
            .map(|_: Value| ());
        }
        let was_literal = matches!(&value, Value::Int(_, ty) if ty.literal);
        let value = coerce(value, binding.ty.as_ref());
        // A literal meets its context HERE (issue #14): the annotation types
        // it (coerce above), or `[arith.literal.default]`'s i32 rule applies.
        // Either way the value is range-checked against the type it just
        // took, which is where `let x: i32 = 4_503_599_627_370_496` stops —
        // the checker's E0401 statically; the overflow trap is the closest
        // dynamic reading, never a silent out-of-range retag.
        let value = match value {
            Value::Int(v, ty) if was_literal => {
                let ty = if ty.literal {
                    self.fire(
                        Rule::LiteralDefault,
                        binding.value.span,
                        "literals default to i32",
                    );
                    IntTy::I32
                } else {
                    ty
                };
                if ty.holds(v) {
                    Value::Int(v, ty)
                } else {
                    return self
                        .trap(
                            TrapKind::Overflow,
                            Rule::ArithChecked,
                            binding.value.span,
                            format!(
                                "the literal {v} is outside `{}`, the binding's type — checked \
                                 arithmetic traps in every profile (X3)",
                                ty.name()
                            ),
                            None,
                        )
                        .map(|_: Value| ());
                }
            }
            value => value,
        };
        self.bind_pattern(&binding.pattern, value)
    }

    /// Evaluates an initializer under a float expectation (D54.1/D54.2). The
    /// expectation reaches down through the arithmetic/comparison operators that
    /// form one term (`[type.numlit.propagate]`), so an integer LITERAL at any
    /// leaf adopts float and the operator runs as float arithmetic. The reach is
    /// exactly those operators and a transparent `(…)` group: it does not cross
    /// into a call or any other expression shape, matching the clause's "the
    /// term connected by these operators". A concrete int VALUE leaf is left as
    /// an int, so `[type.numlit.value]`'s refusal still catches it.
    fn eval_float_expected(&mut self, expr: &Expr) -> EResult<Value> {
        match &*expr.kind {
            ExprKind::Group(inner) => self.eval_float_expected(inner),
            ExprKind::Binary { op, lhs, rhs }
                if matches!(
                    op,
                    BinOp::Add
                        | BinOp::Sub
                        | BinOp::Mul
                        | BinOp::Div
                        | BinOp::Rem
                        | BinOp::Lt
                        | BinOp::Le
                        | BinOp::Gt
                        | BinOp::Ge
                        | BinOp::Cmp
                ) =>
            {
                let left = self.eval_float_expected(lhs)?;
                let right = self.eval_float_expected(rhs)?;
                self.binary(*op, left, right, expr.span)
            }
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => match self.eval_float_expected(operand)? {
                Value::Float(v) => Ok(Value::Float(-v)),
                Value::Int(v, ty) if ty.literal => {
                    self.checked(IntTy::LITERAL_WIDE, v.checked_neg(), expr.span, "negation")
                }
                Value::Int(v, ty) => self.checked(ty, v.checked_neg(), expr.span, "negation"),
                other => unsupported(format!("`-` needs a number, got {}", other.kind())),
            },
            // A leaf: evaluate normally, then adopt an integer LITERAL to float.
            // A value or any non-numeric leaf is returned untouched.
            _ => {
                let value = self.eval_for_init(expr)?;
                Ok(match value {
                    Value::Int(v, ty) if ty.literal => Value::Float(v as f64),
                    other => other,
                })
            }
        }
    }

    /// The path an expression denotes, but only when that path denotes storage
    /// that exists right now.
    ///
    /// `xs[3]` on a one-element list *is* a well-formed place expression and is
    /// *not* a place: the difference is the `bounds` trap, and it belongs to the
    /// value path (`builtin::index`), not to a "this is not a place" gap.
    fn live_place(&mut self, expr: &Expr) -> EResult<Option<Path>> {
        match self.place_of(expr) {
            Ok(path) if self.slot_mut(&path).is_some() => Ok(Some(path)),
            Ok(_) | Err(Signal::Unsupported(_)) => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// Evaluates an initializer: a bare place expression *moves*.
    fn eval_for_init(&mut self, expr: &Expr) -> EResult<Value> {
        match self.live_place(expr)? {
            Some(path) => {
                let value = self.read_path(&path, expr.span)?;
                if is_copy(&value) {
                    self.fire(Rule::ValueSemantics, expr.span, "copy (Copy-shaped value)");
                    Ok(value)
                } else {
                    self.move_path(&path, expr.span)
                }
            }
            None => self.eval(expr),
        }
    }

    fn bind_pattern(&mut self, pattern: &Pattern, value: Value) -> EResult<()> {
        match &*pattern.kind {
            PatKind::Wildcard => Ok(()),
            PatKind::Binding(ident) => {
                self.declare(&ident.name, Slot::live(value));
                Ok(())
            }
            PatKind::Tuple(items) => {
                let Value::Tuple(slots) = value else {
                    return unsupported(format!(
                        "a tuple pattern was given {}; shape mismatches are the type checker's",
                        value.kind()
                    ));
                };
                if slots.len() != items.len() {
                    return unsupported(
                        "tuple pattern arity does not match the value; the type checker owns this"
                            .to_owned(),
                    );
                }
                for (sub, slot) in items.iter().zip(slots) {
                    self.bind_pattern(sub, slot.value)?;
                }
                Ok(())
            }
            PatKind::At { name, pattern } => {
                self.declare(&name.name, Slot::live(value.clone()));
                self.bind_pattern(pattern, value)
            }
            _ => unsupported(
                "only irrefutable patterns bind here; a refutable one needs `match`".to_owned(),
            ),
        }
    }

    fn exec_assign(&mut self, place: &Expr, op: AssignOp, value: &Expr, span: Span) -> EResult<()> {
        // `p[0] = 1` / `*p = 1`: the destination is bytes in an allocation, not
        // a slot in the value tree, so the provenance machine takes it.
        if let Some(ptr) = self.raw_target(place)? {
            let rhs = self.eval(value)?;
            let rhs = if op == AssignOp::Assign {
                rhs
            } else {
                let current = self.raw_load(ptr, place.span)?;
                let binop = assign_binop(op);
                self.binary(binop, current, rhs, span)?
            };
            return self.raw_store(ptr, &rhs, span);
        }
        let path = self.place_of(place)?;
        if op == AssignOp::Assign {
            let value = self.eval_for_init(value)?;
            // Keep the place's integer type: `x += 1` and `x = x + 1` agree.
            let value = match (self.slot_mut(&path).map(|s| s.value.clone()), value) {
                (Some(Value::Int(_, ty)), Value::Int(v, lit)) if lit.literal => {
                    if !ty.literal && !ty.holds(v) {
                        // The place's type is this literal's context (issue
                        // #14): adopting it range-checks, like any checked op.
                        return self
                            .trap(
                                TrapKind::Overflow,
                                Rule::ArithChecked,
                                span,
                                format!(
                                    "the literal {v} is outside `{}`, the place's type — checked \
                                     arithmetic traps in every profile (X3)",
                                    ty.name()
                                ),
                                None,
                            )
                            .map(|_: Value| ());
                    }
                    Value::Int(v, ty)
                }
                (_, value) => value,
            };
            return self.write_path(&path, value, span);
        }

        let current = self.read_path(&path, place.span)?;
        let rhs = self.eval(value)?;
        let binop = assign_binop(op);
        // A map's absent key defaults to its value type's zero, which is what
        // makes `tally[w] += 1` the idiom the corpus writes.
        let current = if current == Value::Unit {
            Value::Int(0, IntTy::INT)
        } else {
            current
        };
        let result = self.binary(binop, current, rhs, span)?;
        self.write_path(&path, result, span)
    }

    // -- places ------------------------------------------------------------

    /// The path an expression denotes, or `Unsupported` if it denotes no place.
    ///
    /// `Unsupported` here is a *control* answer, not a verdict: callers that
    /// have a value-expression fallback catch it. Only an unresolvable name
    /// escapes to the record.
    fn place_of(&mut self, expr: &Expr) -> EResult<Path> {
        match &*expr.kind {
            // `[gram.item.use]` folds `a.b.c` into one `path` production, so a
            // dotted expression is a *place* whenever its head names a local:
            // field projection and module qualification share a syntax and are
            // told apart here, by what the head turned out to be.
            ExprKind::Path(path) => {
                let name = &path.segments[0].name;
                let mut base = if self.local_exists(name) {
                    Path::local(self.frame(), name.clone())
                } else if self.globals.contains_key(name) {
                    Path {
                        frame: usize::MAX,
                        base: name.clone(),
                        projections: Vec::new(),
                    }
                } else {
                    return unsupported(format!("`{name}` is not a local place"));
                };
                for segment in &path.segments[1..] {
                    base = base.project(Proj::Field(segment.name.clone()));
                }
                Ok(base)
            }
            ExprKind::Group(inner) => self.place_of(inner),
            // `(mut p).norm()` — the place is `p`; the mode is the *call's*
            // concern (`[gram.expr.primary]`, the X1 receiver ruling) and is
            // read by `method_split`, not here.
            ExprKind::ModedReceiver { place, .. } => self.place_of(place),
            ExprKind::Member { base, member } => {
                let path = self.place_of(base)?;
                Ok(match member {
                    Member::Named(ident) => path.project(Proj::Field(ident.name.clone())),
                    Member::Index(index, _) => path.project(Proj::Index(i128::from(*index))),
                })
            }
            ExprKind::BracketApply { base, args } => {
                let path = self.place_of(base)?;
                let [IndexArg::Value(arg)] = args.as_slice() else {
                    return unsupported("only a single-argument index denotes a place".to_owned());
                };
                let key = self.eval(&arg.expr)?;
                Ok(match key {
                    Value::Int(i, _) => path.project(Proj::Index(i)),
                    Value::Str(s) => path.project(Proj::Key(s)),
                    // A slice expression is a *value*, not a place (issue
                    // #10, wolf-std F-0021): refusing here sends
                    // `d[0..1].upper()` down the by-value receiver path,
                    // exactly where `"abc"[0..1].upper()` already runs.
                    Value::Range { .. } => {
                        return unsupported(
                            "a slice expression denotes a value, not a place".to_owned(),
                        );
                    }
                    other => path.project(Proj::Key(other.to_string())),
                })
            }
            _ => unsupported("this expression denotes no place".to_owned()),
        }
    }

    // -- expressions -------------------------------------------------------

    #[allow(clippy::too_many_lines)]
    fn eval(&mut self, expr: &Expr) -> EResult<Value> {
        self.step()?;
        match &*expr.kind {
            ExprKind::Int(text) => Ok(Value::Int(parse_int(text)?, IntTy::LITERAL)),
            ExprKind::Float(text) => Ok(Value::Float(parse_float(text)?)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            // A char literal is its decoded scalar ([type.char.lit]); the
            // lexer already collapsed every spelling to the value.
            ExprKind::Char(c) => Ok(Value::Char(*c)),
            ExprKind::Str(literal) => {
                let mut out = String::new();
                for part in &literal.parts {
                    match part {
                        StrPart::Text(text) => out.push_str(text),
                        StrPart::Interp(interp) => out.push_str(&self.eval_interp(interp)?),
                    }
                }
                self.fire(Rule::StrInterp, expr.span, "f-string");
                Ok(Value::Str(out))
            }
            ExprKind::Wildcard => {
                unsupported("`_` is never a value you can read (`[gram.lex.ident]`)".to_owned())
            }
            ExprKind::Path(_) => self.eval_path_expr(expr),
            ExprKind::Group(inner) => self.eval(inner),
            ExprKind::Block(block) => self.eval_block(block),
            ExprKind::Tuple(items) => {
                let mut slots = Vec::with_capacity(items.len());
                for item in items {
                    slots.push(Slot::live(self.eval_for_init(item)?));
                }
                Ok(Value::Tuple(slots))
            }
            ExprKind::StructLit { path, fields } => {
                let mut name = path
                    .segments
                    .last()
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                // A REPL session's types are generational ([repl.type.gen]):
                // the literal creates a value of the generation current NOW,
                // and the value keeps that identity across redefinitions.
                {
                    let types = self.shared.repl_types.lock().expect("repl types lock");
                    if let Some(generation) = types.get(&name) {
                        name = format!("{name}#{generation}");
                    }
                }
                let mut built = Vec::with_capacity(fields.len());
                for field in fields {
                    let value = match &field.value {
                        Some(value) => self.eval_for_init(value)?,
                        // `Point { x }` binds the field from the identifier.
                        None => {
                            let path = Path::local(self.frame(), field.name.name.clone());
                            self.read_path(&path, field.span)?
                        }
                    };
                    built.push((field.name.name.clone(), Slot::live(value)));
                }
                // `[mem.model.order]`: struct-literal fields evaluate in
                // written order, which the loop above is.
                self.fire(
                    Rule::EvalStrictOrder,
                    expr.span,
                    &format!(
                        "`{name}`'s {} field(s) evaluated in written order",
                        built.len()
                    ),
                );
                self.fire(Rule::Alloc, expr.span, &format!("struct literal `{name}`"));
                // An allocation site (`[mem.model.alloc]`) lands in the current
                // region (`[mem.region.create.3]`), and every reference it
                // carries is a store into that region's data — §3's table
                // applies to each one.
                let owner = self.allocate(expr.span, &format!("struct literal `{name}`"));
                let value = Value::Struct {
                    name,
                    fields: built,
                    home: Some(owner),
                };
                self.check_edges(Some(owner), &value, expr.span, "struct field")?;
                Ok(value)
            }
            ExprKind::Unary { op, operand } => self.eval_unary(*op, operand, expr.span),
            ExprKind::Binary { op, lhs, rhs } => {
                // `&&`/`||` short-circuit; everything else evaluates both
                // operands left to right (`[gram.expr.prec]`).
                if matches!(op, BinOp::And | BinOp::Or) {
                    let left = self.eval(lhs)?;
                    let Some(left) = left.as_bool() else {
                        return unsupported(format!(
                            "`{}` needs a bool, got {}",
                            if *op == BinOp::And { "&&" } else { "||" },
                            left.kind()
                        ));
                    };
                    self.fire(Rule::EvalOrder, expr.span, "short-circuit");
                    if (*op == BinOp::And && !left) || (*op == BinOp::Or && left) {
                        return Ok(Value::Bool(left));
                    }
                    let right = self.eval(rhs)?;
                    return match right.as_bool() {
                        Some(right) => Ok(Value::Bool(right)),
                        None => unsupported(format!(
                            "a bool operand was expected, got {}",
                            right.kind()
                        )),
                    };
                }
                let left = self.eval(lhs)?;
                let right = self.eval(rhs)?;
                self.binary(*op, left, right, expr.span)
            }
            ExprKind::Cast { expr: inner, ty } => {
                let value = self.eval(inner)?;
                self.eval_cast(value, ty, expr.span)
            }
            ExprKind::Call { callee, args } => self.eval_call(callee, args, expr.span),
            ExprKind::BracketApply { base, args } => self.eval_bracket(base, args, expr.span),
            ExprKind::Member { base, member } => self.eval_member(base, member, expr.span),
            // A moded receiver evaluates as its place; the mode is consumed by
            // `method_split` when the member access is a call, and marks
            // nothing on a bare member read (`(mut p).x` — the grammar admits
            // it, and no pinned clause gives the mode a meaning there).
            ExprKind::ModedReceiver { place, .. } => self.eval(place),
            ExprKind::Try(inner) => {
                let mut value = self.eval(inner)?;
                if value.is_error() {
                    // `?` returns the error to the caller, widening the row by
                    // union (`[err.propagate]`). A return, not an unwind.
                    //
                    // The widening is LITERAL (wolf-interp#33, F-0079's
                    // sequel): the value carries the row it was raised
                    // through, and a tag raised in a sub-row (`{overflow}`)
                    // that `?` lifts into a wider row (`{syntax, deep,
                    // overflow}`) must arrive carrying the WIDE vocabulary,
                    // or a far-side handler reads the missing tags as
                    // binders and its first arm swallows every widened tag —
                    // the exact first-arm disease #29 fixed, one layer up.
                    if let Value::Error(e) = &mut value
                        && !e.enum_variant
                        && !e.row.is_empty()
                        && let Some(frame) = self.frames.last()
                    {
                        for tag in &frame.row {
                            if !e.row.contains(tag) {
                                e.row.push(tag.clone());
                            }
                        }
                    }
                    self.fire(Rule::ErrPropagate, expr.span, "`?` propagates");
                    return Err(Signal::Return(value));
                }
                self.fire(Rule::ErrUnion, expr.span, "`?` unwraps ok");
                Ok(value)
            }
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                let (Some(start), Some(end)) = (start, end) else {
                    return unsupported(
                        "an open-ended range is only meaningful as a slice endpoint".to_owned(),
                    );
                };
                let start = self.eval(start)?;
                let end = self.eval(end)?;
                match (start, end) {
                    (Value::Int(a, ty), Value::Int(b, _)) => Ok(Value::Range {
                        start: a,
                        end: b,
                        inclusive: *inclusive,
                        ty: if ty.literal { IntTy::INT } else { ty },
                    }),
                    (a, b) => unsupported(format!(
                        "a range needs integer endpoints, got {} and {}",
                        a.kind(),
                        b.kind()
                    )),
                }
            }
            ExprKind::FromEnd(_) => unsupported(
                "`^n` from-end indexing needs the string/collection surface s-tier owns".to_owned(),
            ),
            ExprKind::ElseDefault {
                expr: inner,
                handler,
            } => self.eval_else(inner, handler, expr.span),
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                let value = self.eval(cond)?;
                let Some(taken) = value.as_bool() else {
                    return unsupported(format!(
                        "an `if` condition must be a bool, got {}",
                        value.kind()
                    ));
                };
                self.fire(Rule::Flow, expr.span, "if");
                if taken {
                    self.eval_block(then)
                } else {
                    match otherwise {
                        Some(other) => self.eval(other),
                        None => Ok(Value::Unit),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                let value = self.eval(scrutinee)?;
                for arm in arms {
                    self.push_scope();
                    let matched = self.match_pattern(&arm.pattern, &value)?;
                    let guard = if matched {
                        match &arm.guard {
                            Some(guard) => self.eval(guard)?.as_bool().unwrap_or(false),
                            None => true,
                        }
                    } else {
                        false
                    };
                    if guard {
                        self.fire(Rule::Flow, arm.span, "match arm taken");
                        let result = self.eval(&arm.body);
                        self.pop_scope();
                        return result;
                    }
                    self.pop_scope();
                }
                unsupported(
                    "no `match` arm applied; exhaustiveness is the type checker's".to_owned(),
                )
            }
            ExprKind::For {
                pattern,
                iter,
                body,
            } => self.eval_for(pattern, iter, body, expr.span),
            ExprKind::While { cond, body } => {
                loop {
                    self.step()?;
                    let value = self.eval(cond)?;
                    let Some(taken) = value.as_bool() else {
                        return unsupported(format!(
                            "a `while` condition must be a bool, got {}",
                            value.kind()
                        ));
                    };
                    if !taken {
                        break;
                    }
                    match self.eval_block(body) {
                        Ok(_) | Err(Signal::Continue) => {}
                        Err(Signal::Break(value)) => return Ok(value),
                        Err(other) => return Err(other),
                    }
                }
                Ok(Value::Unit)
            }
            ExprKind::Loop { body } => loop {
                self.step()?;
                match self.eval_block(body) {
                    Ok(_) | Err(Signal::Continue) => {}
                    Err(Signal::Break(value)) => return Ok(value),
                    Err(other) => return Err(other),
                }
            },
            ExprKind::Return(value) => {
                let value = match value {
                    Some(value) => {
                        // D52's return position, re-derived from the clause
                        // (`[gram.expr.tagident]`): the enclosing declared
                        // return row is the expected row, asked BEFORE the
                        // ordinary lookups so module items and prelude names
                        // lose to the declared tag (the s37 silent-wrong
                        // fix); a local still shadows. The wider frame-row
                        // fallback in `eval_path_expr` continues to serve
                        // the fallible tail — see the note there.
                        let row = self
                            .frames
                            .last()
                            .map(|frame| frame.row.clone())
                            .unwrap_or_default();
                        match self.declared_row_tag(value, &row) {
                            Some(tag) => tag,
                            None => self.eval_for_init(value)?,
                        }
                    }
                    None => Value::Unit,
                };
                self.fire(Rule::Flow, expr.span, "return");
                Err(Signal::Return(value))
            }
            ExprKind::Break(value) => {
                let value = match value {
                    Some(value) => self.eval(value)?,
                    None => Value::Unit,
                };
                Err(Signal::Break(value))
            }
            ExprKind::Continue => Err(Signal::Continue),
            ExprKind::Closure {
                params,
                body,
                block_bodied: _,
            } => self.eval_closure(params, body, expr.span),

            // -- Tier 1: regions (is03) -------------------------------------
            ExprKind::RegionSugar {
                name,
                strategy,
                body,
            } => self.eval_region_sugar(
                name.as_ref(),
                strategy.as_ref(),
                body,
                expr.span,
                SugarExit::Free,
            ),
            ExprKind::RegionValue { strategy } => {
                let strategy = self.strategy_of(strategy.as_ref());
                let id = self.store().create(None, strategy.clone(), expr.span);
                let label = self.store().label(id);
                self.fire(
                    Rule::RegionCreate,
                    expr.span,
                    &format!("create {label} with strategy {strategy} (first-class value)"),
                );
                self.fire(
                    Rule::RegionAffine,
                    expr.span,
                    "the region value is affine: it moves and is never copied",
                );
                let generation = self.store().generation(id);
                Ok(Value::Region(RegionValue { id, generation }))
            }
            ExprKind::In { region, body } => self.eval_in(region, body, expr.span),
            ExprKind::Freeze(operand) => self.eval_freeze(operand, expr.span),

            // -- spec/03: the concurrency surface (is06) --------------------
            ExprKind::Scope { name, body } => self.eval_scope(name.as_ref(), body, expr.span),
            ExprKind::SpawnProc { path, args } => self.eval_spawn_proc(path, args, expr.span),
            ExprKind::Select { arms } => self.eval_select(arms, expr.span),
            ExprKind::When { operands, body } => self.eval_when(operands, body, expr.span),
            // -- Tier 3: unsafe (is04) --------------------------------------
            ExprKind::Unsafe { body } => {
                // `[mem.unsafe.scope]`: an `unsafe { }` block is a *marker*, not
                // a different evaluator — the same rules run, and the §7 rows
                // become reachable because raw pointers do.
                self.fire(
                    Rule::UnsafeScope,
                    expr.span,
                    "enter `unsafe { }`; the module is the audit granule",
                );
                self.unsafe_depth += 1;
                let result = self.eval_block(body);
                self.unsafe_depth -= 1;
                self.fire(Rule::UnsafeScope, expr.span, "leave `unsafe { }`");
                result
            }
            ExprKind::Borrow { place, from } => self.eval_door(place, from, expr.span),
            ExprKind::UnsafeC { .. } => unsupported(
                "`unsafe c { … }` is opaque token text whose meaning is c10's (`[gram.expr.unsafe]`);                  nothing here compiles C",
            ),
            ExprKind::Asm { .. } => unsupported(
                "inline `asm` has no pinned semantics in any document this implementation \
                 reads; executing a guessed instruction would put invented behavior into a \
                 differential comparison",
            ),
        }
    }

    fn eval_interp(&mut self, interp: &Interpolation) -> EResult<String> {
        let value = self.eval(&interp.expr)?;
        let Some(parts) = &interp.format else {
            return Ok(value.to_string());
        };
        // A nested `{w}` inside the spec evaluates to text first
        // (`{m[k]:>{w}}`); the assembled text then parses as one spec.
        let mut text = String::new();
        for part in parts {
            match part {
                crate::ast::FmtPart::Text(part) => text.push_str(part),
                crate::ast::FmtPart::Interp(expr) => {
                    let value = self.eval(expr)?;
                    text.push_str(&value.to_string());
                }
            }
        }
        if text.is_empty() {
            return Ok(value.to_string());
        }
        // §7.4 (wolf-lang#28): parse, fit, render — never silently ignore
        // (#10). A malformed spec is statically E0412 and a fit failure
        // E0413; a program that reaches here anyway (sema-lite could not
        // see the literal or classify the hole) is refused, not guessed.
        let spec = match crate::fmtspec::parse(&text) {
            Ok(spec) => spec,
            Err(error) => {
                return unsupported(format!(
                    "format spec `{text}` is malformed (statically E0412): {}",
                    error.message()
                ));
            }
        };
        let char_text;
        let fmt_value = match &value {
            Value::Str(s) => Some(crate::fmtspec::FmtValue::Str(s)),
            // `[type.char.interp]`: a spec on a char hole takes the `str`
            // spec surface — fill/align/width, width in BYTES (D25) — and a
            // numeric spec (`{c:x}`) is the E0413 mismatch it looks like,
            // which `apply`'s class check reports below.
            Value::Char(c) => {
                char_text = c.to_string();
                Some(crate::fmtspec::FmtValue::Str(&char_text))
            }
            Value::Bool(b) => Some(crate::fmtspec::FmtValue::Bool(*b)),
            Value::Int(v, _) => Some(crate::fmtspec::FmtValue::Int(*v)),
            Value::Float(v) => Some(crate::fmtspec::FmtValue::F64(*v)),
            _ => None,
        };
        let Some(fmt_value) = fmt_value else {
            return unsupported(format!(
                "format spec `{text}` on {} — the spec grammar speaks about strings, bools \
                 and numbers, and this machine will not guess what it means elsewhere",
                value.kind()
            ));
        };
        match crate::fmtspec::apply(&spec, fmt_value) {
            Ok(rendered) => Ok(rendered),
            Err(mismatch) => unsupported(format!(
                "format spec `{text}` does not fit its hole (statically E0413): {}",
                mismatch.message()
            )),
        }
    }

    fn eval_path_expr(&mut self, expr: &Expr) -> EResult<Value> {
        let ExprKind::Path(path) = &*expr.kind else {
            unreachable!("caller checked")
        };
        let head = &path.segments[0].name;
        if self.local_exists(head) || self.globals.contains_key(head) {
            let place = self.place_of(expr)?;
            if self.slot_mut(&place).is_some() {
                return self.read_path(&place, expr.span);
            }
            // A dotted tail that is not a stored field: `xs.len`, `s.len`.
            let mut parent = place.clone();
            let Some(Proj::Field(member)) = parent.projections.pop() else {
                return self.read_path(&place, expr.span);
            };
            let value = self.read_path(&parent, expr.span)?;
            return builtin::property(self, &value, &member, expr.span);
        }

        if path.is_single() {
            let name = head;
            let module = self
                .frames
                .last()
                .map(|f| f.module.clone())
                .unwrap_or_default();
            if let Some(def) = self.shared.program.lookup(&module, name, false).cloned() {
                return self.value_of_def(&def, &module, name, expr.span);
            }
            if self.shared.program.modules.contains_key(name) {
                return Ok(Value::Module(name.clone()));
            }
            if let Some(builtin) = builtin::ambient(name) {
                return Ok(builtin);
            }
            // An unresolved capitalized name is a structural error tag: D30's
            // rows need no declaration (`[err.rows]`). Unless an `enum` in
            // scope declares it as a variant — then it is a VALUE of that
            // enum, and calling it an error is wolf-interp#16.
            if name.starts_with(char::is_uppercase) {
                let variant = self.declares_variant(None, name);
                self.fire(
                    Rule::ErrRows,
                    expr.span,
                    &if variant {
                        format!("enum variant `{name}`")
                    } else {
                        format!("error tag `{name}`")
                    },
                );
                return Ok(Value::Error(Box::new(ErrorValue {
                    tag: name.clone(),
                    payload: Vec::new(),
                    enum_variant: variant,
                    row: Vec::new(),
                })));
            }
            // A lowercase bare tag resolves against the enclosing function's
            // declared row (issue #12, the interpreter half of wolf-lang#4):
            // `return none` under `-> int ! {none}` raises the tag. The
            // eager half lives in sema's `raise_check` — by the time this
            // runs, an unresolvable raise has already been refused.
            //
            // Re-derived against `[gram.expr.tagident]` (D52, is19): the
            // clause's return-position rule — the `return` operand and a
            // fallible function's TAIL check against the declared return
            // row, locals shadowing, module items losing — is implemented
            // by `declared_row_tag` at the `Return` arm (exact, items lose)
            // plus this fallback, which is what resolves the tail and any
            // expression feeding it. Two honest deltas from the clause
            // remain here, named rather than hidden: (1) this fallback runs
            // AFTER the module/prelude lookups, so a module item spelling a
            // declared tag would win in tail position (no witness
            // exercises the collision; the `return` spelling is exact);
            // (2) it answers in positions the clause does not check —
            // wider than the rule, but a program relying on that width is
            // one the compiler refuses E0301, so the differential lands it
            // in the refusal classes, never as a silent disagreement.
            if let Some(row) = self
                .frames
                .last()
                .filter(|frame| frame.row.iter().any(|tag| tag == name))
                .map(|frame| frame.row.clone())
            {
                self.fire(
                    Rule::ErrRows,
                    expr.span,
                    &format!("declared-row tag `{name}`"),
                );
                return Ok(Value::Error(Box::new(ErrorValue {
                    tag: name.clone(),
                    payload: Vec::new(),
                    // A name reached through the enclosing function's declared
                    // ROW is a raise by construction, whatever else it spells.
                    enum_variant: false,
                    // The row rides with the value (wolf-interp#29): the
                    // handler's arm resolution asks the VALUE what row it came
                    // through, not the matching module's own signatures, so a
                    // module boundary between raise and handler changes
                    // nothing about which arm wins.
                    row,
                })));
            }
            // The D59 teach-note (the compiler's E0301 situation (a), lupin's
            // voice): the name IS defined next door, in a file that opted out
            // of this module — say which file, which marker, and the fix.
            let module = self
                .frames
                .last()
                .map(|f| f.module.clone())
                .unwrap_or_default();
            if let Some(note) = self.standalone_note(&module, name) {
                return unsupported(format!("`{name}` does not resolve{note}"));
            }
            return unsupported(format!("`{name}` does not resolve"));
        }

        // A dotted path: module member, or a dotted error tag (`io.Error`).
        let head = path.segments[0].name.clone();
        let tail = path.segments[1].name.clone();
        if self.shared.program.modules.contains_key(&head) {
            return match self.shared.program.lookup(&head, &tail, true) {
                Some(def) => {
                    let def = def.clone();
                    self.value_of_def(&def, &head, &tail, expr.span)
                }
                None => {
                    // The D59 teach-notes, cross-module: the member lives in
                    // a standalone sibling of the imported directory (a), or
                    // the directory formed an empty module because every
                    // file opted out (b) / it holds no `.lu` files at all
                    // (c) — the compiler's three E0301 situations.
                    if let Some(note) = self.standalone_note(&head, &tail) {
                        return unsupported(format!("`{head}.{tail}` does not resolve{note}"));
                    }
                    if let Some(note) = self.empty_module_note(&head) {
                        return unsupported(format!("`{head}.{tail}` does not resolve{note}"));
                    }
                    unsupported(format!(
                        "`{head}.{tail}` does not resolve; it is either absent or not `pub` \
                         (`[mod.vis.private]` is the compiler's E0304)"
                    ))
                }
            };
        }
        if head == "c" && !self.local_exists("c") {
            let module = self
                .frames
                .last()
                .map(|f| f.module.clone())
                .unwrap_or_default();
            if self.shared.program.imports_c(&module) {
                return match builtin::c_intrinsic(&tail) {
                    Some(name) => {
                        self.fire(
                            Rule::BoundaryFfi,
                            expr.span,
                            &format!("`c.{tail}` names an imported C function"),
                        );
                        Ok(Value::Builtin(name))
                    }
                    None => unsupported(format!(
                        "`c.{tail}` is an imported C function this machine does not model; the \
                         host-intrinsic set is documented in `docs/approximation-contract.md` §8, \
                         and inventing a body for a real libc call would put guessed behavior into \
                         a differential comparison"
                    )),
                };
            }
        }
        if let Some(builtin) = builtin::ambient_dotted(&head, &tail) {
            return Ok(builtin);
        }
        if tail.starts_with(char::is_uppercase) {
            // `W.Num` is an enum variant when `W` is an enum declaring `Num`,
            // and a dotted error tag (`io.Error`) otherwise. wolf-interp#16:
            // this is the construction site where an enum VALUE was being
            // minted as a raise, so that every `-> W ! {none}` function's
            // ordinary return read as an error at the call site.
            let variant = self.declares_variant(Some(&head), &tail);
            let tag = format!("{head}.{tail}");
            self.fire(
                Rule::ErrRows,
                expr.span,
                &if variant {
                    format!("enum variant `{tag}`")
                } else {
                    format!("error tag `{tag}`")
                },
            );
            return Ok(Value::Error(Box::new(ErrorValue {
                tag,
                payload: Vec::new(),
                enum_variant: variant,
                row: Vec::new(),
            })));
        }
        unsupported(format!("`{head}.{tail}` does not resolve"))
    }

    /// Does an `enum` in the current module declare `variant`, optionally
    /// under the specific enum name `owner`?
    ///
    /// The map is sema's `Module::variants` (variant name → the enums that
    /// declare it), the same table the variant-pattern rule reads, so a name
    /// is a variant here exactly when it is a variant there. A module is a
    /// directory (D32), which is why one module's map is the whole question.
    fn declares_variant(&self, owner: Option<&str>, variant: &str) -> bool {
        let module = self
            .frames
            .last()
            .map(|frame| frame.module.clone())
            .unwrap_or_default();
        self.shared
            .program
            .modules
            .get(&module)
            .and_then(|m| m.variants.get(variant))
            .is_some_and(|enums| match owner {
                Some(owner) => enums.iter().any(|declared| declared == owner),
                None => !enums.is_empty(),
            })
    }

    /// The D59 teach-note when `name` misses in `module` but one of the
    /// directory's standalone entries defines it — the compiler's E0301
    /// situation (a), in this machine's voice: file, marker, and fix named
    /// (`[conf.directive.standalone]`).
    fn standalone_note(&self, module: &str, name: &str) -> Option<String> {
        let sibling = self
            .shared
            .program
            .modules
            .get(module)?
            .standalone
            .iter()
            .find(|s| s.names.iter().any(|n| n == name))?;
        Some(format!(
            "; it IS defined in `{}`, a standalone entry ({}) — a standalone entry is its own \
             program and never a member of its directory's module \
             (`[conf.directive.standalone]`, D59; the compiler's E0301). Drop the marker to \
             make that file a member",
            sibling.file, sibling.marker
        ))
    }

    /// The D59 teach-notes for an import that formed an empty module: every
    /// file opted out (situation (b) — listed), or the directory holds no
    /// `.lu` files at all (situation (c) — a formation note, not a layout
    /// assertion).
    fn empty_module_note(&self, module: &str) -> Option<String> {
        let m = self.shared.program.modules.get(module)?;
        if !m.units.is_empty() {
            return None;
        }
        if m.standalone.is_empty() {
            return Some(
                "; the imported directory holds no `.lu` files, so no module formed \
                 (D32: a module is a directory of `.lu` files)"
                    .to_owned(),
            );
        }
        let opted_out: Vec<String> = m
            .standalone
            .iter()
            .map(|s| format!("`{}` ({})", s.file, s.marker))
            .collect();
        Some(format!(
            "; every `.lu` file in the imported directory is a standalone entry — {} — so the \
             import formed an empty module (`[conf.directive.standalone]`, D59; the compiler's \
             E0301). Drop a marker to give the module members",
            opted_out.join(", ")
        ))
    }

    fn value_of_def(&mut self, def: &Def, module: &str, name: &str, span: Span) -> EResult<Value> {
        match def {
            Def::Fn(_) => Ok(Value::Fn(qualify(module, name))),
            Def::Struct(_) => Ok(Value::Fn(qualify(module, name))),
            Def::Binding(binding) => {
                if let Some(slot) = self.globals.get(name) {
                    return Ok(slot.value.clone());
                }
                let binding = binding.clone();
                let value = self.eval(&binding.value)?;
                self.globals
                    .insert(name.to_owned(), Slot::live(value.clone()));
                Ok(value)
            }
            Def::Opaque(what) => unsupported(format!(
                "`{name}` is a {what}; traits, enums and type-level items have no dynamic semantics here"
            )),
            Def::Ambiguous => {
                let _ = span;
                unsupported(format!(
                    "`{name}` is defined more than once in module `{module}`; dispatch is \
                     ambiguous (the compiler's E0302)"
                ))
            }
        }
    }

    fn eval_unary(&mut self, op: UnOp, operand: &Expr, span: Span) -> EResult<Value> {
        match op {
            UnOp::Copy => {
                // `copy x` produces an independent value from any type
                // (`[mem.tier0.move.3]`) — and does NOT move the source.
                let value = match self.live_place(operand)? {
                    Some(path) => self.read_path(&path, span)?,
                    None => self.eval(operand)?,
                };
                self.fire(Rule::Copy, span, "explicit copy");
                Ok(value)
            }
            UnOp::Move => {
                let path = self.place_of(operand)?;
                let value = self.move_path(&path, span)?;
                // `[mem.region.freeze.2]` transfers the region value, and
                // `[mem.region.freeze.3]` refuses to transfer an open one —
                // "the forest transfers as closed subtrees only". E1005's
                // dynamic half.
                if let Value::Region(handle) = &value {
                    let id = handle.id;
                    let depth = self.store().region(id).map_or(0, |region| region.depth);
                    if depth > 0 {
                        let label = self.store().label(id);
                        return self.region_fault(
                            Rule::RegionClosedSubtree,
                            span,
                            format!(
                                "{label} is open here and cannot be transferred; a region moves \
                                 as a closed subtree (the compiler's E1005)"
                            ),
                            None,
                        );
                    }
                    let label = self.store().label(id);
                    self.fire(Rule::RegionTransfer, span, &format!("transfer {label}"));
                }
                Ok(value)
            }
            UnOp::Not => match self.eval(operand)? {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                other => unsupported(format!("`!` needs a bool, got {}", other.kind())),
            },
            UnOp::Neg => match self.eval(operand)? {
                // Negating a still-unconstrained literal keeps it one
                // (issue #14): `-9223372036854775808` is a value of `int`,
                // and checking the negation at the i32 default before the
                // binding's declared type can arrive made it unwritable.
                Value::Int(v, ty) if ty.literal => {
                    self.checked(IntTy::LITERAL_WIDE, v.checked_neg(), span, "negation")
                }
                Value::Int(v, ty) => self.checked(ty, v.checked_neg(), span, "negation"),
                Value::Float(v) => Ok(Value::Float(-v)),
                other => unsupported(format!("`-` needs a number, got {}", other.kind())),
            },
            UnOp::Borrow | UnOp::BorrowMut => {
                // A local borrow gets the same slot states as a `mut` argument:
                // it holds its path for the extent of its binding
                // (`[mem.tier0.borrow.1]`, `[mem.tier0.borrow.2]`).
                let path = self.place_of(operand)?;
                let access = if op == UnOp::BorrowMut {
                    Access::Exclusive
                } else {
                    Access::Shared
                };
                self.check_access(&path, access, span)?;
                let value = self.read_path(&path, span)?;
                self.fire(
                    if op == UnOp::BorrowMut {
                        Rule::BorrowExtent
                    } else {
                        Rule::Borrow
                    },
                    span,
                    &format!("borrow `{path}` as `{access}`"),
                );
                self.access.push(Held {
                    path,
                    access,
                    span,
                    why: HeldWhy::Borrow,
                });
                Ok(value)
            }
            UnOp::Deref => {
                // `*p` is `p[0]`: one pointee, at the pointer's own offset.
                let value = self.eval(operand)?;
                let Value::Raw(ptr) = value else {
                    return unsupported(format!(
                        "`*` is the Tier 3 raw-pointer dereference (`[mem.unsafe.raw.1]`), and {} \
                         is not a raw pointer; `&`/`&mut` are Tier-0 borrows here and yield values, \
                         not pointers",
                        value.kind()
                    ));
                };
                self.raw_load(ptr, span)
            }
            UnOp::Shared => {
                // `shared X` builds the RC cell (`[mem.shared.rc.1]`). The
                // payload moves into it: the cell is now its place.
                let value = self.eval_for_init(operand)?;
                let cell = self.store().new_cell(value.clone(), span);
                self.fire(
                    Rule::SharedRc,
                    span,
                    &format!("shared#{cell} created with one strong owner"),
                );
                self.assert_shared_acyclic(cell, &value, span);
                Ok(Value::Shared(cell))
            }
        }
    }

    fn checked(
        &mut self,
        ty: IntTy,
        computed: Option<i128>,
        span: Span,
        what: &str,
    ) -> EResult<Value> {
        let Some(raw) = computed else {
            return self.trap(
                TrapKind::Overflow,
                Rule::ArithChecked,
                span,
                format!("{what} overflowed the machine's widest integer"),
                None,
            );
        };
        match ty.reduce(raw) {
            Some(value) => {
                if ty.mode != ArithMode::Checked {
                    self.fire(
                        Rule::ArithWrapping,
                        span,
                        &format!("{what} in {}", ty.name()),
                    );
                }
                Ok(Value::Int(value, ty))
            }
            None => self.trap(
                TrapKind::Overflow,
                Rule::ArithChecked,
                span,
                format!(
                    "{what} produced {raw}, outside `{}` — checked arithmetic traps in every \
                     profile (X3); spell intended overflow `wrapping[{}]`",
                    ty.name(),
                    ty.name()
                ),
                None,
            ),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn binary(&mut self, op: BinOp, left: Value, right: Value, span: Span) -> EResult<Value> {
        use BinOp::{
            Add, BitAnd, BitOr, BitXor, Cmp, Div, Eq, Ge, Gt, Le, Lt, Mul, Ne, Rem, Shl, Shr, Sub,
        };

        // Equality is over *values*, never over the type a value carries: an
        // `i32` 3 and an untyped literal 3 are the same value
        // (`[mem.model.value]` — "values have no identity beyond their current
        // place", and certainly none in their width).
        match op {
            Eq | Ne => {
                // [repl.type.mix]: two generations of one REPL-redefined type
                // are distinct nominal types; comparing them is a type error
                // with a hint, not a quiet `false`. Generational names exist
                // only inside a REPL session (`#` cannot appear in a source
                // identifier), so this arm is unreachable from the corpus.
                if let (Value::Struct { name: a, .. }, Value::Struct { name: b, .. }) =
                    (&left, &right)
                    && a != b
                    && let (Some((base_a, _)), Some((base_b, _))) =
                        (a.split_once('#'), b.split_once('#'))
                    && base_a == base_b
                {
                    return unsupported(format!(
                        "`{a}` and `{b}` are different generations of `{base_a}`: redefining a \
                         type mints a new nominal type ([repl.type.gen]); rebuild the older \
                         value to compare them"
                    ));
                }
                // `[type.char]`: char is not an integer — `'a' == 97` is the
                // checker's type error, and a quiet `false` here would run a
                // program the compiler rejects (the permissive divergence,
                // the harder kind to notice). One char operand demands the
                // other; the bridge is spelled `c as int`.
                if matches!(&left, Value::Char(_)) != matches!(&right, Value::Char(_)) {
                    return unsupported(format!(
                        "`{}` between {} and {} is the checker's refusal — `char` compares                          only with `char` ([type.char.order]); spell the bridge `c as int`",
                        spelling(op),
                        left.kind(),
                        right.kind()
                    ));
                }
                return match op {
                    Eq => Ok(Value::Bool(value_eq(&left, &right))),
                    _ => Ok(Value::Bool(!value_eq(&left, &right))),
                };
            }
            _ => {}
        }

        // `[type.char.order]`: char orders by scalar value — total,
        // locale-free, deterministic, and honestly NOT collation
        // ('z' < 'é' because 0x7A < 0xE9). Only against another char: the
        // comparisons never bridge into the numeric tower, and arithmetic
        // on chars falls through to the generic refusal below — `char` is
        // not an integer type ([type.char]).
        if let (Value::Char(a), Value::Char(b)) = (&left, &right) {
            let (a, b) = (*a, *b);
            return match op {
                Lt => Ok(Value::Bool(a < b)),
                Le => Ok(Value::Bool(a <= b)),
                Gt => Ok(Value::Bool(a > b)),
                Ge => Ok(Value::Bool(a >= b)),
                Cmp => Ok(Value::int(i128::from(a.cmp(&b) as i8))),
                _ => unsupported(format!(
                    "`{}` is not defined on two chars — `char` is not an integer type                      ([type.char]); spell arithmetic through `c as int`",
                    spelling(op)
                )),
            };
        }

        // Strings compare and concatenate.
        if let (Value::Str(a), Value::Str(b)) = (&left, &right) {
            return match op {
                Add => Ok(Value::Str(format!("{a}{b}"))),
                Lt => Ok(Value::Bool(a < b)),
                Le => Ok(Value::Bool(a <= b)),
                Gt => Ok(Value::Bool(a > b)),
                Ge => Ok(Value::Bool(a >= b)),
                Cmp => Ok(Value::int(i128::from(a.cmp(b) as i8))),
                _ => unsupported(format!("`{}` is not defined on two strings", spelling(op))),
            };
        }

        // D54.2 `[type.numlit.propagate]`: the arithmetic/comparison bridge.
        // A float operand carries its float type onto an integer LITERAL in the
        // same term — `c * 1.8 + 32` and `c <= 200` with `c: f64` adopt the bare
        // `32`/`200` as f64 before the operator runs. Adoption is literal-only
        // (D54.3): a concrete int VALUE never adopts, so the mixed-operand
        // `unsupported` refusal below still fires for it. A bare `{float}`
        // literal meeting a bare `{integer}` literal with no concrete float
        // context (`1 + 2.0`) is the static ambiguity `[type.numlit.ambig]`
        // names; a tree-walk carries no float-literal kind, so it computes the
        // f64 result here rather than issuing E0401 — the honest conservatism
        // class (documented for the merger; census-neutral, no stdout change).
        let (left, right) = adopt_numeric(left, right);

        if let (Value::Float(a), Value::Float(b)) = (&left, &right) {
            let (a, b) = (*a, *b);
            return match op {
                Add => Ok(Value::Float(a + b)),
                Sub => Ok(Value::Float(a - b)),
                Mul => Ok(Value::Float(a * b)),
                Div => Ok(Value::Float(a / b)),
                Rem => Ok(Value::Float(a % b)),
                Lt => Ok(Value::Bool(a < b)),
                Le => Ok(Value::Bool(a <= b)),
                Gt => Ok(Value::Bool(a > b)),
                Ge => Ok(Value::Bool(a >= b)),
                _ => unsupported(format!("`{}` is not defined on floats", spelling(op))),
            };
        }

        let (Value::Int(a, ta), Value::Int(b, tb)) = (&left, &right) else {
            return unsupported(format!(
                "`{}` is not defined on {} and {}; operand types are the checker's",
                spelling(op),
                left.kind(),
                right.kind()
            ));
        };
        let (a, b) = (*a, *b);
        // Literal defaulting and checking-context propagation
        // (`[arith.literal.default]`): an unconstrained literal adopts the
        // other operand's type. Two literals stay a literal (issue #14):
        // the i32 default is applied where the value meets its context — an
        // unannotated binding — because a declared type may still arrive to
        // type the whole expression (`let d: int = -9223372036854775807 - 1`
        // is INT_MIN, not an i32 overflow).
        let ty = match (ta.literal, tb.literal) {
            (true, false) => *tb,
            (false, true) | (false, false) => *ta,
            (true, true) => IntTy::LITERAL_WIDE,
        };

        match op {
            Lt => return Ok(Value::Bool(a < b)),
            Le => return Ok(Value::Bool(a <= b)),
            Gt => return Ok(Value::Bool(a > b)),
            Ge => return Ok(Value::Bool(a >= b)),
            Cmp => return Ok(Value::int(i128::from(a.cmp(&b) as i8))),
            _ => {}
        }

        if matches!(op, Div | Rem) && b == 0 {
            // Defined behavior, not UB (`[mem.ub.defined]`).
            return self.trap(
                TrapKind::DivZero,
                Rule::DivZero,
                span,
                "division by zero is defined behavior in wolf: it traps",
                None,
            );
        }

        // #42: on a wrapping type the shift COUNT masks to the TYPE's bit
        // width — `x << 64 == x` on wrapping[u64], `y << 32 == y` on
        // wrapping[u32] — the in-repo ruling all three compiler lanes
        // implement (the WIR shl contract; s111's #130 mirrored it into the
        // checked tier). The width comes from the operand's type, never a
        // constant: u32 masks at 32, u64 at 64. Powers of two throughout, so
        // `rem_euclid` IS the bit mask, and it also reads a negative count
        // the way hardware reads its two's-complement pattern. Checked and
        // saturating types keep the overflow trap `checked` below imposes —
        // the spec's own wrapping-shift-count clause is still owed upstream
        // ([gram] has no arith rows), recorded on the issue.
        let shift_count = || {
            let masked = if ty.mode == ArithMode::Wrapping {
                b.rem_euclid(i128::from(ty.bits))
            } else {
                b
            };
            u32::try_from(masked).ok()
        };
        let computed = match op {
            Add => a.checked_add(b),
            Sub => a.checked_sub(b),
            Mul => a.checked_mul(b),
            Div => a.checked_div(b),
            Rem => a.checked_rem(b),
            BitAnd => Some(a & b),
            BitOr => Some(a | b),
            BitXor => Some(a ^ b),
            Shl => shift_count().and_then(|s| a.checked_shl(s)),
            Shr => shift_count().and_then(|s| a.checked_shr(s)),
            _ => unreachable!("handled above"),
        };
        self.checked(ty, computed, span, &format!("`{}`", spelling(op)))
    }

    fn eval_else(&mut self, inner: &Expr, handler: &ElseHandler, span: Span) -> EResult<Value> {
        let value = self.eval(inner)?;
        if !value.is_error() {
            return Ok(value);
        }
        self.fire(Rule::ErrElse, span, "`else` defaults the error away");
        match handler {
            ElseHandler::Expr(expr) => self.eval(expr),
            ElseHandler::Block(block) => self.eval_block(block),
            ElseHandler::Handler { pattern, body } => {
                self.push_scope();
                let matched = self.match_pattern(pattern, &value);
                let result = match matched {
                    Ok(true) => self.eval(body),
                    Ok(false) => unsupported(
                        "the `else |pat|` pattern did not match the error value".to_owned(),
                    ),
                    Err(signal) => Err(signal),
                };
                self.pop_scope();
                result
            }
        }
    }

    fn eval_for(
        &mut self,
        pattern: &Pattern,
        iter: &Expr,
        body: &Block,
        span: Span,
    ) -> EResult<Value> {
        let iterable = self.eval(iter)?;
        // `for v in ch` iterates a channel lazily until drained-close
        // ([conc.chan.close]) — each iteration is a blocking point.
        if let Value::Chan(chan) = iterable {
            self.fire(Rule::Flow, span, "for over a channel");
            return self.eval_for_chan(chan, pattern, body, iter.span);
        }
        let is_container = matches!(&iterable, Value::List(..) | Value::Map(..));
        let items: Vec<Value> = match iterable {
            Value::Range {
                start,
                end,
                inclusive,
                ty,
            } => {
                let last = if inclusive { end } else { end - 1 };
                let mut out = Vec::new();
                let mut at = start;
                while at <= last {
                    out.push(Value::Int(at, ty));
                    at += 1;
                }
                out
            }
            Value::List(slots, _, _) => std::sync::Arc::unwrap_or_clone(slots)
                .into_iter()
                .map(|s| s.value)
                .collect(),
            Value::Map(pairs) => pairs
                .into_iter()
                .map(|(key, slot)| Value::Tuple(vec![Slot::live(key), slot]))
                .collect(),
            other => {
                return self.eval_for_iter(other, pattern, body, span);
            }
        };

        self.fire(Rule::Flow, span, "for");
        // D40 (resolves S-11): `for x in xs` holds a READ claim on the
        // container for the loop's whole extent, so a mut use of the
        // container inside the body — push/pop/clear, an element write, a
        // `mut` pass — conflicts in `check_access` and traps `exclusivity`
        // at the mutation. One rule, two enforcement modes: wolfgang's
        // static E1013 and this claim ([conf.trap.map]'s E1013 row); the
        // held s68 for-over-mutated lint died by promotion.
        let depth = self.access.len();
        if is_container && let Some(path) = self.iterated_container(iter) {
            self.check_access(&path, Access::Shared, iter.span)?;
            self.fire(
                Rule::Exclusivity,
                iter.span,
                &format!("`{path}`: the `for` loop's read claim, held for its extent (D40)"),
            );
            self.access.push(Held {
                path,
                access: Access::Shared,
                span: iter.span,
                why: HeldWhy::Iteration,
            });
        }
        let mut outcome = Ok(Value::Unit);
        for item in items {
            if let Err(signal) = self.step() {
                outcome = Err(signal);
                break;
            }
            self.push_scope();
            let bound = self.bind_pattern(pattern, item);
            let result = match bound {
                Ok(()) => self.eval_block(body),
                Err(signal) => Err(signal),
            };
            self.pop_scope();
            match result {
                Ok(_) | Err(Signal::Continue) => {}
                Err(Signal::Break(value)) => {
                    outcome = Ok(value);
                    break;
                }
                Err(other) => {
                    outcome = Err(other);
                    break;
                }
            }
        }
        // The claim's extent ends with the loop, on every exit path.
        self.access.release(self.access.len().saturating_sub(depth));
        outcome
    }

    /// The container place a `for` loop iterates, when its operand is a
    /// side-effect-free place expression (`xs`, `state.items`) — the shape
    /// D40's read claim attaches to. Anything else (a call, a literal, an
    /// indexed element) iterates a value with no caller-visible place.
    fn iterated_container(&mut self, expr: &Expr) -> Option<Path> {
        fn placelike(expr: &Expr) -> bool {
            match &*expr.kind {
                ExprKind::Path(path) => path.is_single(),
                ExprKind::Member { base, .. } => placelike(base),
                ExprKind::Group(inner) => placelike(inner),
                _ => false,
            }
        }
        if !placelike(expr) {
            return None;
        }
        self.live_place(expr).unwrap_or_default()
    }

    /// `[mem.iter.for]` — `for pat in e { body }` over an `Iter[T]`
    /// implementor desugars to the explicit drive loop:
    ///
    /// ```text
    /// var it = e
    /// loop {
    ///     let pat = (mut it).next() else { break }
    ///     body
    /// }
    /// ```
    ///
    /// `next(mut self) -> T ! {done}` is called through the same
    /// call-by-value-result machinery as every method: the iterator value is
    /// the receiver, its post-call `self` is the next iteration's iterator.
    /// The desugar's bare `else { break }` catches *any* raise — `done` and
    /// anything else the row carries — exactly as spelled.
    fn eval_for_iter(
        &mut self,
        iterable: Value,
        pattern: &Pattern,
        body: &Block,
        span: Span,
    ) -> EResult<Value> {
        let Some((module, decl)) = self.iter_next_of(&iterable) else {
            if let Some((module, _)) = self.method_of(&iterable, "next") {
                let _ = module;
                return unsupported(format!(
                    "`for` iterates `Iter` implementors by name ([mem.iter.impl]): {} has a \
                     `next` method, but no `impl Iter for …` block declares it",
                    iterable.kind()
                ));
            }
            return unsupported(format!("`for` cannot iterate {}", iterable.kind()));
        };
        self.fire(
            Rule::Flow,
            span,
            "for over an `Iter` implementor: the [mem.iter.for] drive loop",
        );
        let mut it = iterable;
        loop {
            self.step()?;
            // `let pat = (mut it).next() else { break }`
            self.pending_retags = Vec::new();
            let applied = self.call_fn(&decl, &module, vec![it], span)?;
            it = applied.params.into_iter().next().unwrap_or(Value::Unit);
            if applied.value.is_error() {
                self.fire(Rule::ErrElse, span, "the drive loop's `else { break }`");
                break;
            }
            self.push_scope();
            let bound = self.bind_pattern(pattern, applied.value);
            let result = match bound {
                Ok(()) => self.eval_block(body),
                Err(signal) => Err(signal),
            };
            self.pop_scope();
            match result {
                Ok(_) | Err(Signal::Continue) => {}
                Err(Signal::Break(value)) => return Ok(value),
                Err(other) => return Err(other),
            }
        }
        Ok(Value::Unit)
    }

    /// The `next` an `impl Iter for <type>` block declares for this value's
    /// type, if any — trait conformance is by name (`[mem.iter.impl]`), so an
    /// inherent `next` does not qualify.
    fn iter_next_of(&self, value: &Value) -> Option<(String, Box<crate::ast::FnDecl>)> {
        let (module, defs) = self.method_defs_of(value, "next")?;
        defs.iter()
            .find(|def| def.trait_name.as_deref() == Some("Iter"))
            .map(|def| (module, def.decl.clone()))
    }

    /// An impl-block method for this value's type, by name, in the s17
    /// resolution order: the type's own impl wins over a trait's — no
    /// fallback chains (`[ty.method.order]`).
    fn method_of(&self, value: &Value, method: &str) -> Option<(String, Box<crate::ast::FnDecl>)> {
        let (module, defs) = self.method_defs_of(value, method)?;
        defs.iter()
            .find(|def| def.trait_name.is_none())
            .or_else(|| defs.first())
            .map(|def| (module, def.decl.clone()))
    }

    /// The trait-qualified form: `Speak.speak(d)` reaches trait `Speak`'s
    /// method for `d`'s type even where an inherent method shadows it
    /// (`[ty.trait.qualified-call]`).
    ///
    /// Primitives dispatch here too (wolf-interp#34's third shape, upstream
    /// #119/D49): `impl Text for int` registers under the spelling `int`
    /// exactly as a nominal does, so a prim receiver falls back to its
    /// TYPE-name lookup. Only the qualified call gets this road — the
    /// method-call syntax on prim receivers stays with the builtin surface,
    /// whose inherent tier wins the s17 resolution order.
    fn trait_method_of(
        &self,
        trait_name: &str,
        method: &str,
        value: &Value,
    ) -> Option<(String, Box<crate::ast::FnDecl>)> {
        let (module, defs) = self.method_defs_of(value, method).or_else(|| {
            prim_type_names(value)
                .into_iter()
                .find_map(|name| self.method_defs_named(&name, method))
        })?;
        defs.iter()
            .find(|def| def.trait_name.as_deref() == Some(trait_name))
            .map(|def| (module, def.decl.clone()))
    }

    fn method_defs_of(
        &self,
        value: &Value,
        method: &str,
    ) -> Option<(String, Vec<crate::sema::MethodDef>)> {
        let owned;
        let name: &str = match value {
            Value::Struct { name, .. } => name,
            // A declared enum's variant VALUE owns its enum's nominal
            // identity (wolf-interp#34's second shape, wolf-lang#23's
            // surviving leg): `Hue.Red` outside call position is the tag —
            // the same encoding call-form construction emits — and a method
            // on it dispatches through `impl Hue` exactly as a struct value
            // does through its type's impls. The tag never stops being the
            // tag (pattern matching, equality and rendering are untouched);
            // only dispatch learns the type's name.
            Value::Error(e) if e.enum_variant => {
                owned = self.enum_of_variant(e)?;
                &owned
            }
            _ => return None,
        };
        self.method_defs_named(name, method)
    }

    /// The enum a variant VALUE belongs to: the qualifier of a dotted tag
    /// (`Hue.Red` → `Hue`), or — for a bare tag minted where the variant
    /// name alone was in scope — the declaring enum, current module first
    /// (the same order every nominal lookup walks).
    fn enum_of_variant(&self, e: &ErrorValue) -> Option<String> {
        if let Some((owner, _)) = e.tag.split_once('.') {
            return Some(owner.to_owned());
        }
        let current = self
            .frames
            .last()
            .map(|f| f.module.clone())
            .unwrap_or_default();
        let modules = &self.shared.program.modules;
        std::iter::once(&current)
            .chain(modules.keys().filter(|k| **k != current))
            .find_map(|m| modules.get(m)?.variants.get(&e.tag)?.first().cloned())
    }

    /// [`Machine::method_defs_of`] by the type's NAME alone — the nominal
    /// half of dispatch, split out so a caller holding only the name (the
    /// receiver-mode demand below peeks at the slot without reading it) can
    /// ask the same question.
    fn method_defs_named(
        &self,
        name: &str,
        method: &str,
    ) -> Option<(String, Vec<crate::sema::MethodDef>)> {
        // REPL generations (`Point#2`) impl under their base name.
        let name = name.split('#').next().unwrap_or(name);
        let current = self
            .frames
            .last()
            .map(|f| f.module.clone())
            .unwrap_or_default();
        let modules = &self.shared.program.modules;
        let order = || std::iter::once(&current).chain(modules.keys().filter(|k| **k != current));
        if let Some(hit) = order().find_map(|m| {
            let defs = modules.get(m)?.methods.get(name)?.get(method)?;
            Some((m.clone(), defs.clone()))
        }) {
            return Some(hit);
        }
        // The subject's own table missed: the floor under it is a default
        // body from a trait the subject implements (wolf-interp#32). The
        // impl link and the trait may live in different modules (an entry
        // file implementing an imported trait), so both lookups walk the
        // module order independently. The synthesized MethodDef carries the
        // trait's name — the same shape an explicit `impl` method records.
        let implemented: Vec<String> = order()
            .filter_map(|m| modules.get(m)?.trait_impls.get(name))
            .flatten()
            .cloned()
            .collect();
        for trait_name in implemented {
            if let Some((m, decl)) = order().find_map(|m| {
                let decl = modules
                    .get(m)?
                    .trait_defaults
                    .get(&trait_name)?
                    .get(method)?;
                Some((m.clone(), decl.clone()))
            }) {
                return Some((
                    m,
                    vec![crate::sema::MethodDef {
                        decl,
                        trait_name: Some(trait_name),
                    }],
                ));
            }
        }
        None
    }

    fn eval_closure(&mut self, params: &[ClosureParam], body: &Expr, span: Span) -> EResult<Value> {
        // Captured by value at construction (`[gram.expr.closure]`, D10 Tier 0:
        // there is nothing to capture by reference).
        let mut captures = Vec::new();
        if let Some(frame) = self.frames.last() {
            for scope in &frame.scopes {
                for (name, slot) in &scope.locals {
                    if slot.is_live() {
                        captures.push((name.clone(), slot.value.clone()));
                    }
                }
            }
        }
        // wolf-interp#36: the compiler's closure env BORROWS its captures
        // (the s98 loan design, `[abi.native.closure]`); this machine copies
        // the bits, and the shared loan is what keeps the copy unobservable.
        // Record a loan for every place the body actually USES (the copy
        // above over-captures the whole frame; the loans must not), so a
        // later call can notice a write that landed after the capture — the
        // one program that could tell copy from reference apart — and refuse
        // it instead of running the stale read.
        let mut bound: BTreeSet<String> = params.iter().map(|p| p.name.name.clone()).collect();
        let mut used = BTreeSet::new();
        crate::lint::free_names(body, &mut bound, &mut used);
        let mut loans = Vec::new();
        if let Some(frame) = self.frames.last() {
            let serial = frame.serial;
            for name in &used {
                if !captures.iter().any(|(captured, _)| captured == name) {
                    continue;
                }
                let generation = self
                    .capture_gens
                    .get(&(serial, name.clone()))
                    .map_or(0, |(generation, _)| *generation);
                loans.push(CaptureLoan {
                    name: name.clone(),
                    task: self.task,
                    serial,
                    generation,
                    span,
                });
            }
        }
        for loan in &loans {
            self.captured_places
                .insert((loan.serial, loan.name.clone()));
        }
        self.fire(Rule::Closure, span, "closure captures by value");
        Ok(Value::Closure(Box::new(ClosureValue {
            params: params.iter().map(|p| p.name.name.clone()).collect(),
            body: body.clone(),
            captures,
            loans,
        })))
    }

    fn match_pattern(&mut self, pattern: &Pattern, value: &Value) -> EResult<bool> {
        match &*pattern.kind {
            PatKind::Wildcard => Ok(true),
            PatKind::Binding(ident) => {
                let name = &ident.name;
                // The checker's resolution rule, dynamically (issue #5,
                // wolf-std F-0007): a bare identifier that names an in-scope
                // enum variant is a *variant pattern* — it matches that case
                // and binds nothing. Otherwise, over a tag-shaped scrutinee, a
                // capitalized identifier is a structural row-tag pattern (D30
                // rows need no declaration, and this machine already reads
                // unresolved capitalized names as tags in expression
                // position). Everything else binds, as before — including a
                // capitalized name over a non-error scrutinee, which the
                // counterparty also treats as a binding (observed at pin
                // a0c4564: `match 3 { Zed => Zed, _ => 9 }` warns E0802 on
                // the `_` arm).
                let module = self
                    .frames
                    .last()
                    .map(|f| f.module.clone())
                    .unwrap_or_default();
                if let Some(enums) = self
                    .shared
                    .program
                    .modules
                    .get(&module)
                    .and_then(|m| m.variants.get(name))
                {
                    let matched = matches!(value, Value::Error(e) if e.payload.is_empty()
                    && (e.tag == *name
                        || enums.iter().any(|en| {
                            e.tag.len() == en.len() + 1 + name.len()
                                && e.tag.starts_with(en.as_str())
                                && e.tag.as_bytes()[en.len()] == b'.'
                                && e.tag.ends_with(name.as_str())
                        })));
                    self.fire(
                        Rule::Flow,
                        pattern.span,
                        &format!("`{name}` resolves to an enum variant: a variant pattern"),
                    );
                    return Ok(matched);
                }
                if name.starts_with(char::is_uppercase)
                    && let Value::Error(e) = value
                {
                    self.fire(
                        Rule::ErrRows,
                        pattern.span,
                        &format!("`{name}` over a tag value: a row-tag pattern"),
                    );
                    return Ok(e.payload.is_empty() && e.tag == *name);
                }
                // The lowercase mirror (issue #12, wolf-lang#4's other
                // half): over a tag-shaped scrutinee, a lowercase identifier
                // that names a declared row tag is a row-tag pattern —
                // `match err { empty => …, negative => … }` under
                // `-> int ! {empty, negative}` dispatches on the tag. An
                // undeclared lowercase name still binds (`else |err| …` keeps
                // its binder), which is the sema-lite reading of the checker's
                // row-typed resolution.
                //
                // "Declared" is asked of the SCRUTINEE's own row first
                // (wolf-interp#29): the row the value was raised through
                // travels with it, so the answer is the same on either side of
                // a module boundary. Asking only the matching module's own
                // signatures — as this did — made every arm of a handler over
                // an imported callee's row a fresh binding, so the first arm
                // matched every tag and answered, silently, wrong. The
                // module's own vocabulary stays in the union behind it: a tag
                // the machine minted with no row still resolves where the
                // matching module declares it.
                if name.starts_with(char::is_lowercase)
                    && let Value::Error(e) = value
                    && (e.row.iter().any(|tag| tag == name)
                        || self
                            .shared
                            .program
                            .modules
                            .get(&module)
                            .is_some_and(|m| m.row_tags.contains(name)))
                {
                    self.fire(
                        Rule::ErrRows,
                        pattern.span,
                        &format!("`{name}` names a declared row tag: a row-tag pattern"),
                    );
                    return Ok(e.payload.is_empty() && e.tag == *name);
                }
                self.declare(name, Slot::live(value.clone()));
                Ok(true)
            }
            PatKind::Literal(expr) => {
                let literal = self.eval(expr)?;
                Ok(match (&literal, value) {
                    (Value::Int(a, _), Value::Int(b, _)) => a == b,
                    _ => literal == *value,
                })
            }
            PatKind::Variant { path, fields } => {
                let tag = path
                    .segments
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let Value::Error(err) = value else {
                    return Ok(false);
                };
                // The same resolution rule as bare identifiers (issue #5): a
                // payload pattern spelled `Rgb(…)` matches a value built as
                // `Color.Rgb(…)` when `Rgb` is an in-scope variant, and the
                // enum-qualified spelling matches the bare-built value — the
                // checker equates the two through the scrutinee's type; this
                // machine equates them through the variant table.
                let matched = err.tag == tag || {
                    let module = self
                        .frames
                        .last()
                        .map(|f| f.module.clone())
                        .unwrap_or_default();
                    let variants = self
                        .shared
                        .program
                        .modules
                        .get(&module)
                        .map(|m| &m.variants);
                    match path.segments.as_slice() {
                        [single] => {
                            variants
                                .and_then(|v| v.get(&single.name))
                                .is_some_and(|enums| {
                                    enums
                                        .iter()
                                        .any(|en| err.tag == format!("{en}.{}", single.name))
                                })
                        }
                        [qualifier, name] => {
                            err.tag == name.name
                                && variants
                                    .and_then(|v| v.get(&name.name))
                                    .is_some_and(|enums| enums.contains(&qualifier.name))
                        }
                        _ => false,
                    }
                };
                if !matched || err.payload.len() != fields.len() {
                    return Ok(false);
                }
                let payload = err.payload.clone();
                for (sub, item) in fields.iter().zip(payload) {
                    if !self.match_pattern(sub, &item)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            PatKind::Tuple(items) => {
                let Value::Tuple(slots) = value else {
                    return Ok(false);
                };
                if slots.len() != items.len() {
                    return Ok(false);
                }
                let values: Vec<Value> = slots.iter().map(|s| s.value.clone()).collect();
                for (sub, item) in items.iter().zip(values) {
                    if !self.match_pattern(sub, &item)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            PatKind::At { name, pattern } => {
                if self.match_pattern(pattern, value)? {
                    self.declare(&name.name, Slot::live(value.clone()));
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            PatKind::Or(alternatives) => {
                for alternative in alternatives {
                    if self.match_pattern(alternative, value)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    // -- calls, members, indexing -----------------------------------------

    fn eval_call(&mut self, callee: &Expr, args: &[Arg], span: Span) -> EResult<Value> {
        // A method call is a call whose callee projects a member out of a
        // *value*. `[gram.item.use]`'s `path` production swallows the dots, so
        // `xs.push` and `geometry.area` arrive in the same shape and are told
        // apart by whether the head names a local or a module.
        if let Some((receiver, method, mode)) = self.method_split(callee) {
            return self.eval_method(&receiver, &method, mode, args, span);
        }

        // `assert` is an **intrinsic**, one name in both tiers, and is never
        // shadowed by a library function (`[conf.trap.assert]`; wolf-std
        // F-0009 observed a module-level `assert` severing callers from the
        // trap). The two-argument form's `msg` is evaluated **only** on the
        // failing path — which is why the intrinsic is handled here, before
        // the argument row is evaluated.
        if let ExprKind::Path(path) = &*callee.kind
            && path.is_single()
            && path.segments[0].name == "assert"
            && !self.local_exists("assert")
        {
            return self.eval_assert(args, span);
        }

        // `Speak.speak(d)` — the trait-qualified call reaches the trait's
        // method for the first argument's type, even where an inherent
        // method shadows it (`[ty.trait.qualified-call]`, the s17 resolution
        // order's explicit escape).
        if let ExprKind::Path(path) = &*callee.kind
            && path.segments.len() == 2
            && !self.local_exists(&path.segments[0].name)
            && !self.globals.contains_key(&path.segments[0].name)
        {
            let module = self
                .frames
                .last()
                .map(|f| f.module.clone())
                .unwrap_or_default();
            let head = path.segments[0].name.clone();
            // The head may be the trait's own definition, or a `use`-bound
            // name for a trait in another module (`use media.Show` then
            // `Show.show(v)` — wolf-interp#32's adapter case). A use decl
            // binds a path, not a Def, so the import is followed here: the
            // recorded segments' tail looked up in their module, visibility
            // honoured.
            let is_trait = matches!(
                self.shared.program.lookup(&module, &head, false),
                Some(Def::Opaque("trait"))
            ) || self
                .shared
                .program
                .modules
                .get(&module)
                .into_iter()
                .flat_map(|m| m.use_paths.iter())
                .filter(|(bound, _)| *bound == head)
                .any(|(_, segments)| {
                    let (Some(name), init) = (segments.last(), &segments[..segments.len() - 1])
                    else {
                        return false;
                    };
                    let target = init.join(".");
                    matches!(
                        self.shared.program.lookup(&target, name, true),
                        Some(Def::Opaque("trait"))
                    )
                });
            if is_trait {
                let method = path.segments[1].name.clone();
                let evaluated = self.eval_args(args)?;
                let Some(first) = evaluated.values.first() else {
                    return unsupported(format!(
                        "`{head}.{method}` is a trait-qualified call; the receiver is the \
                         first argument, and there is none"
                    ));
                };
                let dispatched = match self.trait_method_of(&head, &method, first) {
                    Some((impl_module, decl)) => {
                        self.pending_retags = Vec::new();
                        self.call_fn(&decl, &impl_module, evaluated.values.clone(), span)
                    }
                    None => unsupported(format!(
                        "`{head}.{method}` does not resolve to an `impl {head} for …` \
                         method of {}",
                        first.kind()
                    )),
                };
                let finals = match &dispatched {
                    Ok(applied) => applied.params.clone(),
                    Err(_) => evaluated.values,
                };
                self.finish_args(
                    &evaluated.writebacks,
                    &finals,
                    evaluated.held,
                    &evaluated.protectors,
                    span,
                );
                return dispatched.map(|applied| applied.value);
            }
        }

        let target = self.eval(callee)?;
        // The X1 mode law's dynamic residue (issue #15): a direct call whose
        // spelling disagrees with the signature was already rejected at
        // resolve (sema's E1007 half), so a disagreement surviving to here
        // came through a function *value* the static tier could not see.
        // `[conf.trap.map]` gives E1007 no runtime meaning — no trap kind
        // exists for it — and running would compute a wrong answer (an
        // unspelled `mut` argument passes by value and the writeback never
        // happens), so the honest verdict is a refusal.
        //
        // The same declaration, where it is in sight, carries the parameter
        // rows D52's argument position resolves against (`param_rows` below).
        let mut param_rows: Option<Vec<Vec<String>>> = None;
        if let Value::Fn(qualified) = &target
            && let (module, name) = split_qualified(qualified)
            && let Some(Def::Fn(decl)) = self.shared.program.lookup(&module, &name, false)
        {
            for (param, arg) in decl.params.iter().zip(args) {
                if param.mode != arg.mode {
                    let param_name = match &param.kind {
                        ParamKind::Named { name, .. } => name.name.as_str(),
                        ParamKind::SelfParam { .. } => "self",
                    };
                    return unsupported(format!(
                        "`{name}` declares `{param_name}` with a call-site mode this call does \
                         not spell (X1); the disagreement is E1007's static rule, \
                         `[conf.trap.map]` gives it no dynamic meaning, and running it would \
                         produce a wrong answer — a call through a function value is refused, \
                         not guessed"
                    ));
                }
            }
            param_rows = Some(
                decl.params
                    .iter()
                    .map(|param| match &param.kind {
                        ParamKind::Named { ty, .. } => crate::sema::type_tags(ty),
                        ParamKind::SelfParam { .. } => Vec::new(),
                    })
                    .collect(),
            );
        }
        let evaluated = self.eval_args_for(args, Callee::of(&target), param_rows.as_deref())?;
        // Handed to `call_fn` across `apply`, which performs no evaluation of
        // its own between here and the callee's parameter binding.
        self.pending_retags = vec![None; evaluated.values.len()];
        for (index, retag) in &evaluated.retags {
            if retag.bind && *index < self.pending_retags.len() {
                self.pending_retags[*index] = Some(*retag);
            }
        }
        let result = self.apply(target, evaluated.values.clone(), span);
        let finals = match &result {
            Ok(applied) => applied.params.clone(),
            Err(_) => evaluated.values,
        };
        self.finish_args(
            &evaluated.writebacks,
            &finals,
            evaluated.held,
            &evaluated.protectors,
            span,
        );
        result.map(|applied| match applied.value {
            // Issue #21: `List[i32]()` — the constructor's bracket type
            // argument is the element's checking context. `eval_bracket`
            // erases type arguments from values, so the annotation is read
            // off the callee's own syntax here and stamped onto the fresh
            // container.
            Value::List(items, None, home) => Value::List(items, list_elem_of(callee), home),
            value => value,
        })
    }

    /// `assert(cond)` / `assert(cond, msg)` — the intrinsic's own arities
    /// (`[conf.trap.assert]`). Silent and effect-free when the condition
    /// holds; on the failing path `msg` is evaluated, rendered as one line to
    /// stdout, and the `assert` trap fires at the call's own span.
    fn eval_assert(&mut self, args: &[Arg], span: Span) -> EResult<Value> {
        let (cond, msg) = match args {
            [cond] => (cond, None),
            [cond, msg] => (cond, Some(msg)),
            _ => {
                return unsupported(format!(
                    "`assert` is the intrinsic's own arity: a condition and an optional \
                     message, not {} argument(s)",
                    args.len()
                ));
            }
        };
        let value = self.eval(&cond.expr)?;
        let Some(holds) = value.as_bool() else {
            return unsupported(format!(
                "`assert` takes a bool condition, got {}",
                value.kind()
            ));
        };
        if holds {
            // Silent and effect-free — the message is *not* a second
            // condition and is never evaluated on the passing path
            // (the counterparty's #19 shape).
            self.fire(Rule::Assert, span, "assert holds; the message stays cold");
            return Ok(Value::Unit);
        }
        if let Some(msg) = msg {
            let rendered = self.eval(&msg.expr)?;
            self.out(&format!("{rendered}\n"));
        }
        self.fault(
            TrapKind::Assert,
            Rule::Assert,
            span,
            "assertion failed".to_owned(),
        )
    }

    /// Splits a callee into `(receiver, method, receiver mode)` when it is a
    /// method call. The mode is the X1 receiver spelling — `(mut c).bump()`,
    /// `(take conn).close()` — and `None` is the bare (`read self`) form.
    fn method_split(&mut self, callee: &Expr) -> Option<(Receiver, String, Option<ParamMode>)> {
        match &*callee.kind {
            ExprKind::Member {
                base,
                member: Member::Named(name),
            } if !self.is_module_expr(base) => {
                let (base, mode) = match &*base.kind {
                    ExprKind::ModedReceiver { mode, place } => (place, Some(*mode)),
                    _ => (base, None),
                };
                Some((
                    match self.place_of(base) {
                        Ok(path) => Receiver::Place(path),
                        Err(_) => Receiver::Expr(base.clone()),
                    },
                    name.name.clone(),
                    mode,
                ))
            }
            ExprKind::Path(path) if path.segments.len() >= 2 => {
                let head = &path.segments[0].name;
                if !self.local_exists(head) && !self.globals.contains_key(head) {
                    return None;
                }
                let mut place = self.place_of(callee).ok()?;
                let Some(Proj::Field(method)) = place.projections.pop() else {
                    return None;
                };
                Some((Receiver::Place(place), method, None))
            }
            _ => None,
        }
    }

    fn is_module_expr(&self, expr: &Expr) -> bool {
        match &*expr.kind {
            ExprKind::Path(path) if path.is_single() => {
                let name = &path.segments[0].name;
                !self.local_exists(name) && self.shared.program.modules.contains_key(name)
            }
            _ => false,
        }
    }

    fn apply(&mut self, target: Value, args: Vec<Value>, span: Span) -> EResult<Applied> {
        let unchanged = args.clone();
        let plain = move |value: Value| Applied {
            value,
            params: unchanged,
        };
        match target {
            Value::Fn(qualified) => {
                let (module, name) = split_qualified(&qualified);
                match self.shared.program.lookup(&module, &name, false).cloned() {
                    Some(Def::Fn(decl)) => self.call_fn(&decl, &module, args, span),
                    Some(Def::Struct(def)) => {
                        // A tuple-shaped constructor call. The corpus builds
                        // structs with literals, so this is only reached by a
                        // program the type checker would have caught.
                        let _ = def;
                        unsupported(format!(
                            "`{name}` is a type; build it with a struct literal"
                        ))
                    }
                    _ => unsupported(format!("`{qualified}` is not callable")),
                }
            }
            Value::Closure(closure) => {
                if closure.params.len() != args.len() {
                    return unsupported(format!(
                        "a closure of {} parameter(s) was called with {}",
                        closure.params.len(),
                        args.len()
                    ));
                }
                // wolf-interp#36: the env borrows its captured places (s98,
                // `[abi.native.closure]`). This call is the moment "the
                // closure is still needed" becomes a fact rather than a
                // liveness guess, so the loan is checked here: a captured
                // place written since the capture means the compiler's NLL
                // engine refuses the WRITE as E1002, and running on would
                // read this machine's stale copy — the one observable
                // difference between copy and borrow. Refuse it, naming both
                // sites. A closure on a foreign task is exempt: task
                // captures are copies by D14's law, not loans.
                for loan in &closure.loans {
                    if loan.task != self.task {
                        continue;
                    }
                    let key = (loan.serial, loan.name.clone());
                    let Some((current, written_at)) = self.capture_gens.get(&key).copied() else {
                        continue;
                    };
                    if current != loan.generation {
                        let name = &loan.name;
                        return self.trap(
                            TrapKind::Exclusivity,
                            Rule::BorrowExtent,
                            span,
                            format!(
                                "this closure captured `{name}` and is still needed, but \
                                 `{name}` was written after the capture: the closure env \
                                 borrows its places, so the write ends the loan — the \
                                 compiler refuses it as E1002, and running on would read a \
                                 stale copy"
                            ),
                            Some((
                                written_at,
                                format!("`{name}` written here, after the closure captured it"),
                            )),
                        );
                    }
                }
                let serial = self.mint_frame_serial();
                self.frames.push(Frame {
                    module: self
                        .frames
                        .last()
                        .map(|f| f.module.clone())
                        .unwrap_or_default(),
                    serial,
                    scopes: vec![Scope::default()],
                    // A closure body raising a bare tag reads the enclosing
                    // function's declared row — the closure has no row of
                    // its own to declare.
                    row: self
                        .frames
                        .last()
                        .map(|f| f.row.clone())
                        .unwrap_or_default(),
                    // Closure parameters carry no declared modes; D39's
                    // barrier watches `fn` parameters only.
                    read_params: Vec::new(),
                });
                for (name, value) in &closure.captures {
                    self.declare(name, Slot::live(value.clone()));
                }
                for (name, value) in closure.params.iter().zip(args) {
                    self.declare(name, Slot::live(value));
                }
                let result = self.eval(&closure.body);
                let frame = self.frame();
                self.access.release_frame(frame);
                self.frames.pop();
                let task = self.task;
                self.prov().drop_frame(task, frame);
                match result {
                    Ok(value) | Err(Signal::Return(value)) => Ok(plain(value)),
                    Err(other) => Err(other),
                }
            }
            Value::Error(tag) if tag.payload.is_empty() => {
                // `BadDigit(payload)` — a structural row entry applied to its
                // payload (`[err.rows]`) — or `W.Num(3)`, an enum variant
                // applied to its. Applying a payload never changes WHICH of
                // the two the callee was, so the flag rides through:
                // wolf-interp#16's reproducer is the payload-carrying shape,
                // and dropping it here would re-open the bug one call deeper.
                self.fire(
                    Rule::ErrRows,
                    span,
                    &if tag.enum_variant {
                        format!("enum variant `{}` with payload", tag.tag)
                    } else {
                        format!("error `{}` with payload", tag.tag)
                    },
                );
                Ok(plain(Value::Error(Box::new(ErrorValue {
                    tag: tag.tag,
                    payload: args,
                    enum_variant: tag.enum_variant,
                    // The declared row survives the payload for the same
                    // reason the flag does: applying a payload says nothing
                    // about which row the tag was reached through.
                    row: tag.row,
                }))))
            }
            Value::Builtin(name) => builtin::call(self, name, args, span).map(plain),
            other => unsupported(format!("{} is not callable", other.kind())),
        }
    }

    /// Whether this receiver can be lent to this call rather than copied into
    /// it — see [`Lend`].
    ///
    /// `List`, `Map` and `str` qualify; everything else keeps the copy.
    /// `str.get(a..b)` is excluded because it reads its range off the *syntax*
    /// before ordinary argument evaluation, and so wants the value in hand
    /// before the point a lend hands it over.
    fn lendable(&mut self, path: &Path, method: &str, args: &[Arg]) -> Option<Lend> {
        if method == "get"
            && let [arg] = args
            && matches!(&*arg.expr.kind, ExprKind::Range { .. })
        {
            return None;
        }
        let (slot, _) = self.resolve(path)?;
        match &slot.value {
            value @ (Value::List(..) | Value::Map(_) | Value::Str(_)) => Some(Lend {
                mutating: builtin::mutates_receiver(value, method),
            }),
            _ => None,
        }
    }

    /// Does `method` on the value at `path` declare a `mut` receiver — the
    /// demand a bare call site fails (#37)?
    ///
    /// `Some(declaration_span)` when it does: the span is the `mut self`
    /// parameter's for a user impl method, `None`-inside for the builtin
    /// arms, whose declaration is the counterparty's prelude, not a span in
    /// this program. Outer `None` when the receiver's method wants no `mut`
    /// (or the place does not resolve/is not live — the ordinary read path
    /// reports those its own way).
    fn mut_receiver_demand(&mut self, path: &Path, method: &str) -> Option<Option<Span>> {
        let type_name = {
            let (slot, _) = self.resolve(path)?;
            if !slot.is_live() {
                return None;
            }
            match &slot.value {
                value @ Value::List(..) => {
                    return builtin::mutates_receiver(value, method).then_some(None);
                }
                Value::Struct { name, .. } => name.clone(),
                _ => return None,
            }
        };
        let (_, defs) = self.method_defs_named(&type_name, method)?;
        let decl = defs
            .iter()
            .find(|def| def.trait_name.is_none())
            .or_else(|| defs.first())
            .map(|def| &def.decl)?;
        let receiver_param = decl
            .params
            .first()
            .filter(|param| matches!(param.kind, ParamKind::SelfParam { .. }))?;
        (receiver_param.mode == Some(ParamMode::Mut)).then_some(Some(receiver_param.span))
    }

    fn eval_method(
        &mut self,
        receiver: &Receiver,
        method: &str,
        mode: Option<ParamMode>,
        args: &[Arg],
        span: Span,
    ) -> EResult<Value> {
        // wolf-interp#37 — receiver modes get teeth. X1 is locked surface:
        // mutation is visible where it happens, and the call site spells it.
        // A callee whose receiver mode is `mut` (a user impl's `fn m(mut
        // self)`, or the builtin surface's two receiver-mutating arms,
        // `List.push`/`List.pop`) demands the call-site `(mut …)` marker; the
        // compiler refuses the bare spelling with E0804
        // (`corpus/typecheck/receiver_bare_mut.lu`). This machine performs no
        // static receiver-mode check, so the demand is made here, at call
        // evaluation, with the mode named — E0804's dynamic meaning, kind
        // `exclusivity` (the mode family's row, beside E1013/E1014; see
        // `ledger::dynamic_meaning`).
        if mode != Some(ParamMode::Mut)
            && let Receiver::Place(path) = receiver
            && let Some(declared) = self.mut_receiver_demand(path, method)
        {
            let display = path.to_string();
            return self.trap(
                TrapKind::Exclusivity,
                Rule::ModeMut,
                span,
                format!(
                    "`{method}` takes its receiver `mut`, and X1 binds the mode at the call \
                     site: write `(mut {display}).{method}(…)` — the bare spelling is the \
                     compiler's E0804"
                ),
                declared.map(|at| (at, "the receiver mode is declared here".to_owned())),
            );
        }
        // A receiver whose value is a builtin container is **lent** rather than
        // copied: the read is charged here exactly as it always was, but the
        // value stays in its slot until the arguments have been evaluated and
        // then moves — it is never deep-copied (issue #24). Copying it was
        // what made `xs.push(v)` cost the whole list: a copy out, a second
        // copy to compare against, a whole-value comparison and a copy back,
        // four traversals of `xs` for one element appended, so a loop of
        // pushes was quadratic and every `List`-returning std function with
        // it.
        let lend = match receiver {
            Receiver::Place(path) if mode != Some(ParamMode::Take) => {
                self.lendable(path, method, args)
            }
            _ => None,
        };
        let (path, value) = match receiver {
            Receiver::Place(path) if self.slot_mut(path).is_some() => {
                if lend.is_some() {
                    self.read_claim(path, span)?;
                    (Some(path.clone()), None)
                } else {
                    let value = self.read_path(path, span)?;
                    (Some(path.clone()), Some(value))
                }
            }
            Receiver::Place(path) => {
                let display = path.to_string();
                return unsupported(format!("`{display}` does not denote a place at run time"));
            }
            Receiver::Expr(expr) => (None, Some(self.eval(expr)?)),
        };
        // `[mem.str.get]`: the boundary primitive's argument is a RANGE
        // whose endpoints may be open (`s.get(..2)`) or `^n` end-relative —
        // shapes no `Value` carries — so, exactly as `eval_bracket` does for
        // slices, the range is read off the syntax before ordinary argument
        // evaluation would refuse it. `get` never mutates its receiver, so
        // the two-phase retag machinery below has nothing to observe here.
        if method == "get"
            && let Some(Value::Str(s)) = &value
            && let [arg] = args
            && let ExprKind::Range {
                start,
                end,
                inclusive,
            } = &*arg.expr.kind
        {
            let len = Some(s.len() as i128);
            let s = s.clone();
            let from = match start {
                Some(expr) => Some(self.eval_index_endpoint(expr, len)?),
                None => None,
            };
            let to = match end {
                Some(expr) => Some(self.eval_index_endpoint(expr, len)?),
                None => None,
            };
            return builtin::str_get(self, &s, from, to, *inclusive, span);
        }
        // The receiver of a method call is a `mut` argument in all but
        // spelling, so it retags first — and the retag happens *before* the
        // arguments are evaluated, which is the whole of the two-phase shape:
        // `xs.push(xs.len)` reads `xs` through the parent while the receiver's
        // fresh tag is still Reserved. Stacked Borrows invalidates the receiver
        // there; a tree does not (`[mem.prov.state]`, "foreign read: Reserved
        // ok"), which is `corpus/memory/prov_two_phase.lu`'s entire point.
        let receiver_tag = match &path {
            Some(path) => {
                let key = self.place_key(path);
                let (alloc, child) = self
                    .prov()
                    .retag_place(&key, RetagKind::Mutable, true, span);
                self.drain_prov();
                Some((key, alloc, child))
            }
            None => None,
        };
        let evaluated = self.eval_args(args)?;
        // Now the receiver's own accesses go through the child.
        let previous = receiver_tag
            .as_ref()
            .map(|(key, alloc, child)| self.prov().rebind_place(key, *alloc, *child));
        // The lend's second half. It happens *here*, after the arguments were
        // evaluated, so `xs.push(xs.len)` still reads the receiver through its
        // parent while the receiver's fresh tag is Reserved — the two-phase
        // window `corpus/memory/prov_two_phase.lu` pins is untouched by any of
        // this.
        //
        // A mutating lend asks first whether the write-back can fault: it
        // faults before it stores, so on that path the receiver must survive
        // the call unchanged, which only the copy can promise. The question is
        // asked now rather than at the read because `eval_args` is what
        // establishes the `mut` arguments' held claims.
        let mut lend = lend;
        if let (Some(kind), Some(path)) = (lend, &path)
            && kind.mutating
            && self.writeback_would_trap(path)
        {
            lend = None;
        }
        let mut receiver_value = match (value, &path) {
            (Some(value), _) => value,
            (None, Some(path)) if lend.is_some() => self.lend_path(path),
            // The lend was cancelled above; take the copy the general path
            // wants. One deep copy, on a path that is about to fault anyway.
            (None, Some(path)) => match self.resolve(path) {
                Some((slot, _)) => slot.value.clone(),
                None => Value::Unit,
            },
            (None, None) => unreachable!("a lend implies a place"),
        };
        // The lend's O(1) mutation witness, captured before the call.
        let lent_len = lend.map(|_| builtin::list_len(&receiver_value));
        // Kept for the `[mem.region.freeze.4]` comparison below: a method
        // that returns its receiver unmodified performed only reads, and an
        // unmodified write-back must not count as a write. A lend answers the
        // same question from the witness instead — except in a debug build,
        // where it keeps the copy too and checks the two agree.
        #[cfg(debug_assertions)]
        let original_receiver = receiver_value.clone();
        #[cfg(not(debug_assertions))]
        let original_receiver = if lend.is_some() {
            Value::Unit
        } else {
            receiver_value.clone()
        };
        let mut final_args = evaluated.values.clone();
        // An impl-block method wins over the builtin surface for the types
        // that have one (user structs); the receiver is `self`, and its
        // post-call value is the writeback — call-by-value-result, the same
        // contract as every `mut` parameter.
        let result = match self.method_of(&receiver_value, method) {
            Some((module, decl)) => {
                let mut call_args = Vec::with_capacity(evaluated.values.len() + 1);
                call_args.push(receiver_value.clone());
                call_args.extend(evaluated.values.iter().cloned());
                self.pending_retags = Vec::new();
                self.call_fn(&decl, &module, call_args, span)
                    .map(|applied| {
                        let mut params = applied.params.into_iter();
                        if let Some(next_self) = params.next() {
                            receiver_value = next_self;
                        }
                        final_args = params.collect();
                        applied.value
                    })
            }
            None => builtin::method(
                self,
                &mut receiver_value,
                method,
                evaluated.values.clone(),
                span,
            ),
        };
        self.finish_args(
            &evaluated.writebacks,
            &final_args,
            evaluated.held,
            &evaluated.protectors,
            span,
        );
        // A mutating method writes its receiver back — value semantics, so the
        // observable effect is a whole-value store into the receiver's place.
        // It is a *child* write through the retagged tag: Reserved → Active,
        // the activation the two-phase window was waiting for.
        //
        // A `(take c)` receiver is consumed instead (`[mem.tier0.mode.take]` →
        // `[mem.tier0.move.1]`): no writeback, and the place is moved-from
        // afterwards, so a later use traps `use-after-move`.
        //
        // `[mem.region.freeze.4]` (issue #20): a write-back of an UNMODIFIED
        // receiver is not a write. A method that only read its receiver is an
        // ordinary read — legal through frozen data to any depth, per
        // `[mem.region.edge.imm]` — so the store (and its exclusive-access
        // check and freeze guard) happens only when the method actually
        // changed the value. `frozen[0].body.words()` reads; it must not trap.
        //
        // A lent receiver ends its lend here, whatever the call did — a trap
        // on `pop` of an empty `List` returns the list to its slot just as a
        // successful `len` does. What is decided is only whether that return
        // is *also* a write.
        let written = match (&path, &result) {
            (Some(path), Ok(_)) if mode == Some(ParamMode::Take) => {
                self.fire(Rule::ModeTake, span, &format!("`take {path}` (receiver)"));
                self.move_path(path, span).map(|_| ())
            }
            (Some(path), result) if lend.is_some() => {
                let mutated = result.is_ok()
                    && lend.is_some_and(|kind| kind.mutating)
                    && lent_len.flatten() != builtin::list_len(&receiver_value);
                debug_assert_eq!(
                    mutated,
                    result.is_ok() && receiver_value != original_receiver,
                    "`builtin::mutates_receiver`/`list_len` disagreed with the whole-value \
                     comparison for `{method}`"
                );
                let value = std::mem::replace(&mut receiver_value, Value::Unit);
                if mutated {
                    // `writeback_would_trap` said this store cannot fault
                    // before it lands, so the slot never keeps the
                    // placeholder.
                    self.write_path(path, value, span)
                } else {
                    self.restore_lent(path, value);
                    Ok(())
                }
            }
            (Some(path), Ok(_)) if receiver_value != original_receiver => {
                self.write_path(path, receiver_value, span)
            }
            _ => Ok(()),
        };
        if let Some((key, _, child)) = receiver_tag {
            self.prov().unprotect(child, span);
            self.prov()
                .restore_place(&key, previous.expect("set beside receiver_tag"));
            self.prov().prune();
            self.drain_prov();
        }
        written?;
        result
    }

    fn eval_member(&mut self, base: &Expr, member: &Member, span: Span) -> EResult<Value> {
        if self.is_module_expr(base)
            && let (ExprKind::Path(path), Member::Named(name)) = (&*base.kind, member)
        {
            let module = path.segments[0].name.clone();
            return match self
                .shared
                .program
                .lookup(&module, &name.name, true)
                .cloned()
            {
                Some(def) => self.value_of_def(&def, &module, &name.name, span),
                None => unsupported(format!(
                    "`{module}.{}` does not resolve; it is either absent or not `pub`",
                    name.name
                )),
            };
        }

        if let Ok(path) = self.place_of(base) {
            let projected = match member {
                Member::Named(ident) => path.clone().project(Proj::Field(ident.name.clone())),
                Member::Index(index, _) => path.clone().project(Proj::Index(i128::from(*index))),
            };
            if self.slot_mut(&projected).is_some() {
                return self.read_path(&projected, span);
            }
        }

        let value = self.eval(base)?;
        match member {
            Member::Named(ident) => builtin::property(self, &value, &ident.name, span),
            Member::Index(index, _) => match &value {
                Value::Tuple(slots) => {
                    match slots.get(usize::try_from(*index).unwrap_or(usize::MAX)) {
                        Some(slot) => Ok(slot.value.clone()),
                        None => self.trap(
                            TrapKind::Bounds,
                            Rule::Bounds,
                            span,
                            format!("tuple has no element {index}"),
                            None,
                        ),
                    }
                }
                other => unsupported(format!("{} has no numbered members", other.kind())),
            },
        }
    }

    /// The place an index read can be **lent** from, when there is one — see
    /// [`Lend`] and [`Machine::lendable`], of which this is the index-read
    /// half (issue #28, wolf-std F-0078).
    ///
    /// The same three containers, on the same three facts: the base denotes a
    /// stored place, so there is a slot to put the value back into;
    /// [`builtin::index`] never re-enters the machine, so nothing can observe
    /// the slot while the value is out of it; and their copies are the
    /// expensive ones.
    fn index_lend_place(&mut self, base: &Expr) -> Option<Path> {
        let ExprKind::Path(path) = &*base.kind else {
            return None;
        };
        let head = &path.segments[0].name;
        if !(self.local_exists(head) || self.globals.contains_key(head)) {
            return None;
        }
        let place = self.place_of(base).ok()?;
        let (slot, _) = self.resolve(&place)?;
        matches!(slot.value, Value::List(..) | Value::Map(_) | Value::Str(_)).then_some(place)
    }

    fn eval_bracket(&mut self, base: &Expr, args: &[IndexArg], span: Span) -> EResult<Value> {
        // The index-read lend (issue #28, wolf-std F-0078) — the other half of
        // #24's shape. `xs[i]` evaluated `xs` in order to pick one element out
        // of it, and evaluating a place-valued `xs` deep-copies the whole
        // container: an indexed read cost O(n), an indexed walk O(n²) — four
        // times the work per doubling, where the identical `for v in xs` cost
        // twice. A read cannot observe a copy, so the value stays in its slot.
        // The read is charged here at exactly the moment and in exactly the
        // order it always was — `read_claim` before the index expression is
        // evaluated, which is where `eval(base)` charged it — and the
        // container steps out of its slot only for the length of one
        // `builtin::index` call, which cannot re-enter the machine.
        //
        // Slices keep the copy: `s[a..b]` reads its endpoints off the *syntax*
        // (`^n`, open ends), which is the same exclusion `lendable` makes for
        // `str.get(a..b)`. So does a base that is not a plain place path —
        // there would be nowhere to put the value back.
        if let [IndexArg::Value(arg)] = args
            && !matches!(&*arg.expr.kind, ExprKind::Range { .. })
            && let Some(place) = self.index_lend_place(base)
        {
            // The base's own evaluation step, which the copy path charged on
            // the way into `eval`. `index_lend_place` evaluates nothing — a
            // path expression's place is read off the syntax — so charging it
            // here keeps the fuel account identical step for step. A lend must
            // not let a program run one step further than the copy would.
            self.step()?;
            self.read_claim(&place, base.span)?;
            let index = self.eval(&arg.expr)?;
            let value = self.lend_path(&place);
            let element = builtin::index(self, &value, &index, span);
            self.restore_lent(&place, value);
            return element;
        }
        let target = self.eval(base)?;

        // `e[…]` is one production (`[gram.amb.brackets]`): generic application
        // or indexing, told apart here by what the base turned out to be.
        if matches!(target, Value::Builtin(_) | Value::Fn(_))
            || args.iter().any(|a| matches!(a, IndexArg::Type(_)))
        {
            return Ok(target);
        }

        let [IndexArg::Value(arg)] = args else {
            return unsupported("indexing takes exactly one argument".to_owned());
        };
        // A range in index position is a *slice*, and its endpoints are
        // optional (`s[..8]`, `s[4..]`) or end-relative (`s[^4..]`) — shapes
        // no `Value` carries, so the range is read off the syntax here
        // rather than evaluated to one.
        if let ExprKind::Range {
            start,
            end,
            inclusive,
        } = &*arg.expr.kind
        {
            let len = slice_len_of(&target);
            let start = match start {
                Some(expr) => Some(self.eval_index_endpoint(expr, len)?),
                None => None,
            };
            let end = match end {
                Some(expr) => Some(self.eval_index_endpoint(expr, len)?),
                None => None,
            };
            return builtin::slice(self, &target, start, end, *inclusive, span);
        }
        let index = self.eval(&arg.expr)?;
        if let Value::Raw(ptr) = target {
            let Value::Int(i, _) = index else {
                return unsupported(format!(
                    "a raw pointer is indexed by an integer, got {}",
                    index.kind()
                ));
            };
            return self.raw_load(ptr.offset_by(i), span);
        }
        builtin::index(self, &target, &index, span)
    }

    fn eval_index_endpoint(&mut self, expr: &Expr, len: Option<i128>) -> EResult<i128> {
        // `^n` counts from the end (D25): it resolves to `len - n` BEFORE
        // the bounds/boundary question is asked, exactly as `[mem.str.get]`
        // words it for the recoverable twin.
        if let ExprKind::FromEnd(inner) = &*expr.kind {
            let Some(len) = len else {
                return Err(Signal::Unsupported(
                    "`^n` counts from the end of a string or List; this target has no length \
                     to count from"
                        .to_owned(),
                ));
            };
            return match self.eval(inner)? {
                Value::Int(n, _) => Ok(len - n),
                other => Err(Signal::Unsupported(format!(
                    "`^n` takes an integer, got {}",
                    other.kind()
                ))),
            };
        }
        match self.eval(expr)? {
            Value::Int(v, _) => Ok(v),
            other => Err(Signal::Unsupported(format!(
                "a slice endpoint must be an integer, got {}",
                other.kind()
            ))),
        }
    }

    // -- Tier 3: raw pointers, casts and the door (§5–§7) ------------------

    /// Are we inside an `unsafe { }` block? `[mem.ub]`'s rows are Tier-3
    /// reachable only, and T1's wording says so out loud.
    pub(crate) fn in_unsafe(&self) -> bool {
        self.unsafe_depth > 0
    }

    /// Reports a UB row from `builtin` (the C intrinsics raise P1/L2).
    pub(crate) fn ub_from_builtin<T>(
        &mut self,
        row: UbRow,
        span: Span,
        alloc: Option<prov::AllocId>,
        message: impl Into<String>,
    ) -> EResult<T> {
        self.ub_row(row, span, alloc, message)
    }

    /// `assume noalias p, q (, r)*` (`[mem.unsafe.raw.2]`).
    ///
    /// The *only* assertion-created UB in the language, and the only way raw
    /// code takes on an aliasing obligation at all — everything else about
    /// `*T` is unrestricted by `[mem.unsafe.raw.1]`.
    fn exec_assume_noalias(&mut self, operands: &[Expr], span: Span) -> EResult<()> {
        let mut ptrs = Vec::with_capacity(operands.len());
        for operand in operands {
            let value = match self.live_place(operand)? {
                Some(path) => self.read_path(&path, operand.span)?,
                None => self.eval(operand)?,
            };
            match value {
                Value::Raw(ptr) => ptrs.push(ptr),
                other => {
                    return unsupported(format!(
                        "`assume noalias` asserts about pointed-to *ranges*, and {} names none",
                        other.kind()
                    ));
                }
            }
        }
        let checked = self.prov().assume_noalias(&ptrs, span);
        match checked {
            Ok(()) => {
                self.drain_prov();
                Ok(())
            }
            Err(finding) => self.ub(finding),
        }
    }

    /// `borrow r from ptr` — D11's singular unsafe→safe door
    /// (`[mem.unsafe.door]`).
    ///
    /// > Obligation: `ptr` addresses a live allocation wholly inside region
    /// > `r`'s footprint, correctly typed, for the borrow's extent.
    /// > Discharging a door's obligation falsely is UB at the *door* (§7/P6),
    /// > not later — the safe tier stays safe by construction.
    ///
    /// Which is why every part of the obligation is checked *here*, and the
    /// result carries a fresh child tag under the region's node rather than a
    /// wildcard: safe code above the door never sees one.
    fn eval_door(&mut self, place: &Expr, from: &Expr, span: Span) -> EResult<Value> {
        let region = self.eval(place)?;
        let id = self.region_id_of(&region, span, "`borrow … from`")?;
        let pointer = match self.live_place(from)? {
            Some(path) => self.read_path(&path, from.span)?,
            None => self.eval(from)?,
        };
        let Value::Raw(ptr) = pointer else {
            return unsupported(format!(
                "`borrow r from ptr` launders a *raw pointer*, got {}",
                pointer.kind()
            ));
        };
        let label = self.store().label(id);
        let (alloc, tag) = match (ptr.alloc, ptr.prov) {
            (Some(alloc), Prov::Tag(tag)) => (alloc, tag),
            (Some(alloc), Prov::Wildcard) => {
                // A wildcard pointer discharges nothing: the door's obligation
                // is about a *specific* allocation, and an angelically-resolved
                // pointer names no tag to reborrow from.
                return self.ub_row(
                    UbRow::P6,
                    span,
                    Some(alloc),
                    format!(
                        "`borrow` from a wildcard pointer into alloc#{alloc}: the door needs a \
                         pointer with provenance, and an exposed one carries none"
                    ),
                );
            }
            (None, _) => {
                return self.ub_row(
                    UbRow::P6,
                    span,
                    None,
                    "`borrow` from a null pointer: the door's obligation is that `ptr` addresses \
                     a live allocation",
                );
            }
        };
        let (live, owner) = match self.prov().alloc(alloc) {
            Some(entry) => (entry.live, entry.owner),
            None => (false, None),
        };
        if !live {
            return self.ub_row(
                UbRow::P6,
                span,
                Some(alloc),
                format!(
                    "`borrow {label} from` a pointer into alloc#{alloc}, which is not live: the \
                     door's obligation is a *live* allocation, and discharging it falsely is UB \
                     here rather than at the first use"
                ),
            );
        }
        if owner != Some(id) {
            return self.ub_row(
                UbRow::P6,
                span,
                Some(alloc),
                format!(
                    "`borrow {label} from` a pointer into alloc#{alloc}, which is owned by {} — \
                     the obligation is that the allocation lies wholly inside the named region's \
                     footprint",
                    owner.map_or_else(|| "no region".to_owned(), |owner| self.store().label(owner))
                ),
            );
        }
        // `[mem.prov.tag]`: `borrow r from ptr` is a retag point.
        let child = self.prov().retag(
            alloc,
            tag,
            RetagKind::Mutable,
            false,
            "`borrow … from`",
            span,
        );
        self.drain_prov();
        self.fire(
            Rule::UnsafeDoor,
            span,
            &format!(
                "door discharged: alloc#{alloc} is live and inside {label}; tag#{child} is the \
                 region-scoped reborrow, so nothing above this line sees a wildcard"
            ),
        );
        Ok(Value::Raw(RawPtr {
            prov: Prov::Tag(child),
            ..ptr
        }))
    }

    /// `e as T`.
    ///
    /// Three of the four arms are Tier 3: ptr→int exposes, int→ptr resolves
    /// angelically, and a cast that *produces* a restricted type's value in
    /// unsafe code is §7/T1's whole subject.
    fn eval_cast(&mut self, value: Value, ty: &Type, span: Span) -> EResult<Value> {
        // An adapter cast MOVES the nominal identity (wolf-interp#32's
        // adapter case): `s as Cover` where `type Cover = distinct
        // media.Song` renames the struct value, and the reverse cast
        // renames it back — free and bidirectional (D28 layout identity),
        // and dispatch-by-name follows the new name. Only a RECORDED
        // distinct pair rebrands; unrelated struct casts keep their
        // existing behavior.
        if let Value::Struct { name, fields, home } = &value
            && let Some(target) = crate::sema::head_name(ty)
        {
            let base = name.split('#').next().unwrap_or(name);
            let modules = &self.shared.program.modules;
            let relates = modules.values().any(|m| {
                m.distincts.get(&target).is_some_and(|t| t == base)
                    || m.distincts.get(base).is_some_and(|t| *t == target)
            });
            if relates && target != base {
                self.fire(
                    Rule::ValueSemantics,
                    span,
                    &format!("adapter cast renames `{base}` to `{target}` — layout identity"),
                );
                return Ok(Value::Struct {
                    name: target,
                    fields: fields.clone(),
                    home: *home,
                });
            }
        }
        if let TypeKind::RawPointer(inner) = &*ty.kind {
            let (elem, signed) = pointee(inner);
            return match value {
                // A cast between raw pointer types keeps the tag:
                // `[mem.unsafe.raw.1]` makes casts of raw pointers unrestricted.
                Value::Raw(ptr) => {
                    self.fire(
                        Rule::UnsafeRaw,
                        span,
                        &format!("raw→raw cast keeps {} — casts are unrestricted", ptr.prov),
                    );
                    Ok(Value::Raw(RawPtr {
                        elem,
                        signed,
                        ..ptr
                    }))
                }
                Value::Int(address, _) => {
                    // `[mem.prov.expose]`: "Int→ptr casts produce a pointer with
                    // **exposed** provenance resolved angelically among exposed
                    // tags." The resolution happens at the *access*, so what the
                    // cast produces is a wildcard.
                    let resolved = self.prov().resolve_address(address);
                    let ptr = match resolved {
                        Some((alloc, offset)) => RawPtr {
                            alloc: Some(alloc),
                            offset,
                            prov: Prov::Wildcard,
                            elem,
                            signed,
                        },
                        None => RawPtr {
                            elem,
                            signed,
                            ..RawPtr::null()
                        },
                    };
                    self.fire(
                        Rule::ProvExpose,
                        span,
                        &format!("int→ptr: {address} becomes {ptr} with wildcard provenance"),
                    );
                    Ok(Value::Raw(ptr))
                }
                other => unsupported(format!(
                    "a `*T` cast takes a raw pointer or an integer address, got {}",
                    other.kind()
                )),
            };
        }

        let target = type_name(ty);
        if let Value::Raw(ptr) = value {
            if IntTy::named(&target).is_some() {
                // ptr→int **exposes** the tag (`[mem.prov.expose]`), which is
                // what later lets a wildcard resolve back to it.
                self.prov().expose(ptr, span);
                self.drain_prov();
                let address = self.prov().address_of(ptr);
                return Ok(coerce(Value::Int(address, IntTy::INT), Some(ty)));
            }
            return unsupported(format!("a raw pointer does not cast to `{target}`"));
        }

        // §7/T1 — "producing an invalid value of a restricted type in unsafe
        // code (bool ∉ {0,1} …)". The row exists to license O9's niche packing
        // and default-free jump tables, so the check is at the *production*.
        if target == "bool"
            && let Value::Int(v, _) = value
        {
            if v == 0 || v == 1 {
                return Ok(Value::Bool(v == 1));
            }
            if self.in_unsafe() {
                return self.ub_row(
                    UbRow::T1,
                    span,
                    None,
                    format!(
                        "`{v} as bool` produces a `bool` outside {{0, 1}}; the representation is \
                         restricted, which is what licenses niche packing and default-free jump \
                         tables"
                    ),
                );
            }
            return unsupported(
                "a safe-tier `int as bool` cast has no pinned meaning; the restricted-value rule \
                 that gives one is §7/T1, and it is Tier-3 reachable only"
                    .to_owned(),
            );
        }

        // `[type.char.cast]` (s121, D58) — the char type's two numeric
        // bridges, and the only ones. `char as int` is total: every scalar
        // names its code point. `int as char` traps on a non-scalar —
        // negative, above 0x10FFFF, or the surrogate gap 0xD800..=0xDFFF —
        // as D56's trapping family, `trap(overflow)`: an admitted surrogate
        // would mint a `char` no `str` could ever encode (D24). The gap
        // EDGES 0xD7FF and 0xE000 are legal and convert. Other widths cast
        // through `int`; everything else is refused by name.
        if target == "char" {
            return match value {
                // `c as char` is the identity — no conversion, no retag.
                Value::Char(_) => Ok(value),
                Value::Int(v, _) => match u32::try_from(v).ok().and_then(char::from_u32) {
                    Some(c) => {
                        self.fire(
                            Rule::CharCast,
                            span,
                            &format!("`{v} as char` converts — {v:#X} is a scalar"),
                        );
                        Ok(Value::Char(c))
                    }
                    None => {
                        let why = if v < 0 {
                            "negative, and no scalar is".to_owned()
                        } else if v > 0x0010_FFFF {
                            format!("{v:#X}, above the last scalar 0x10FFFF")
                        } else {
                            format!("{v:#X}, inside the surrogate gap 0xD800..=0xDFFF")
                        };
                        self.trap(
                            TrapKind::Overflow,
                            Rule::CharCast,
                            span,
                            format!(
                                "`{v} as char`: the value is {why} — it names no \
                                     character, and admitting it would mint a `char` that \
                                     cannot be UTF-8-encoded (D24), so the cast traps \
                                     ([type.char.cast], D56's family)"
                            ),
                            None,
                        )
                    }
                },
                other => unsupported(format!(
                    "`{} as char` is outside the cast set — only `int as char` bridges \
                     into `char` ([type.char.cast]); other widths cast through `int`",
                    other.kind()
                )),
            };
        }
        if let Value::Char(c) = value {
            if target == "int" {
                self.fire(
                    Rule::CharCast,
                    span,
                    &format!("`{c:?} as int` is total — the code point {}", u32::from(c)),
                );
                return Ok(Value::Int(i128::from(u32::from(c)), IntTy::INT));
            }
            return unsupported(format!(
                "`char as {target}` is outside the cast set — `char as int` is the total \
                 direction ([type.char.cast]); other widths cast through `int`"
            ));
        }

        // The closed cast set's numeric family (`[ty.cast.closed-set]`,
        // issue #11 — wolf-std F-0022): `as` between numeric types
        // **converts**. It never merely retags: `3 as f64` is `3.0`, and a
        // value outside the target's range is an X3 trap (checked semantics
        // in every profile), not a silent re-label. `wrapping[T]` /
        // `saturating[T]` targets reduce by their own mode instead of
        // trapping — the cast into a wrapping type is how intended overflow
        // is spelled. Approximation-contract §6.9 records the float model
        // (every float is an f64; `as f32` rounds through f32 precision).
        let target_float = matches!(target.as_str(), "f32" | "f64");
        match value {
            Value::Bool(_) if target_float || IntTy::named(&target).is_some() => {
                return unsupported(
                    "`bool` does not cast to a numeric type: there is no truthiness bridge \
                     (the compiler's E0805); write the value out, e.g. `if b { 1 } else { 0 }`"
                        .to_owned(),
                );
            }
            Value::Int(v, _) if target_float => {
                #[allow(clippy::cast_precision_loss)]
                let wide = v as f64;
                let converted = if target == "f32" {
                    #[allow(clippy::cast_possible_truncation)]
                    f64::from(wide as f32)
                } else {
                    wide
                };
                self.fire(
                    Rule::EvalOrder,
                    span,
                    &format!("`{v} as {target}` converts to {converted}"),
                );
                return Ok(Value::Float(converted));
            }
            Value::Float(f) => {
                if target_float {
                    let converted = if target == "f32" {
                        #[allow(clippy::cast_possible_truncation)]
                        f64::from(f as f32)
                    } else {
                        f
                    };
                    return Ok(Value::Float(converted));
                }
                if let Some(int_ty) = IntTy::named(&target) {
                    // Truncation toward zero, range-checked: a float that
                    // does not fit the target — NaN and the infinities
                    // included — traps rather than saturating silently.
                    let truncated = f.trunc();
                    #[allow(clippy::cast_precision_loss)]
                    let fits = truncated.is_finite()
                        && truncated >= int_ty.range().0 as f64
                        && truncated <= int_ty.range().1 as f64;
                    if !fits {
                        return self.trap(
                            TrapKind::Overflow,
                            Rule::ArithChecked,
                            span,
                            format!(
                                "`{f} as {target}` does not fit `{target}` — numeric casts \
                                 are checked conversions in every profile (X3)"
                            ),
                            None,
                        );
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    return Ok(Value::Int(truncated as i128, int_ty));
                }
                if target == "str" {
                    return unsupported(
                        "`as` does not bridge numbers and `str` (the compiler's E0805); \
                         interpolation `\"{x}\"` is the rendering surface"
                            .to_owned(),
                    );
                }
            }
            Value::Int(v, from) => {
                if target == "str" {
                    return unsupported(
                        "`as` does not bridge numbers and `str` (the compiler's E0805); \
                         interpolation `\"{x}\"` is the rendering surface"
                            .to_owned(),
                    );
                }
                let coerced = coerce(Value::Int(v, from), Some(ty));
                if let Value::Int(v2, to) = coerced {
                    // A named integer target converts with a range check;
                    // wrapping/saturating targets reduce by their mode.
                    return match to.reduce(v2) {
                        Some(reduced) => Ok(Value::Int(reduced, to)),
                        None => self.trap(
                            TrapKind::Overflow,
                            Rule::ArithChecked,
                            span,
                            format!(
                                "`{v2} as {}` is outside `{}` — numeric casts are checked \
                                 conversions in every profile (X3); spell intended overflow \
                                 `wrapping[{}]`",
                                to.name(),
                                to.name(),
                                to.name()
                            ),
                            None,
                        ),
                    };
                }
                return Ok(coerced);
            }
            _ => {}
        }
        // Issue #17 ask 2, the runtime half. The fallthrough below RETYPES
        // without converting, which is right for an adapter cast (`m as
        // Meters` is free and bidirectional, the D28 layout-identity fact)
        // and wrong for a conversion the matrix does not define — there it
        // handed the caller a value of the wrong kind and no diagnostic, the
        // silent-wrong-answer this issue was filed for.
        //
        // sema-lite refuses the shapes it can see (`s as int` on a
        // `str`-classified local, E0805, code and span identical to the
        // counterparty's). It cannot see through a call return — that is the
        // typecheck rung, which this machine does not perform — so the same
        // program written `name() as int` arrives here. An honest decline is
        // the answer the ladder allows: `unsupported`, naming the conversion,
        // which the conservatism ledger records against the counterparty's
        // E0805 instead of a wrong exit(0).
        if matches!(value, Value::Str(_))
            && (target == "bool" || target_float || IntTy::named(&target).is_some())
        {
            return unsupported(format!(
                "`str as {target}` is not in the cast set — `as` converts between numeric \
                 types and adapts `distinct` aliases; it does not parse text (the compiler's \
                 E0805). Parse the string instead of retyping it"
            ));
        }
        Ok(coerce(value, Some(ty)))
    }

    /// `p[i]` / `*p` — a provenance-checked load.
    fn raw_load(&mut self, ptr: RawPtr, span: Span) -> EResult<Value> {
        self.prov_access(ptr, ptr.elem, AccessKind::Read, span)?;
        let raw = self.prov().load(ptr, ptr.elem);
        let bits = u32::try_from(ptr.elem * 8).unwrap_or(8);
        let ty = IntTy {
            bits,
            signed: ptr.signed,
            mode: ArithMode::Checked,
            literal: false,
        };
        Ok(Value::Int(raw, ty))
    }

    /// `p[i] = v` / `*p = v` — a provenance-checked store.
    fn raw_store(&mut self, ptr: RawPtr, value: &Value, span: Span) -> EResult<()> {
        let Value::Int(v, _) = value else {
            return unsupported(format!(
                "a raw store writes an integer-shaped pointee, got {}",
                value.kind()
            ));
        };
        self.prov_access(ptr, ptr.elem, AccessKind::Write, span)?;
        self.prov().store(ptr, ptr.elem, *v);
        Ok(())
    }

    /// `c.memset` — one checked write over the whole range.
    ///
    /// A range write, not a loop of byte writes: `[mem.prov.state]` is
    /// per-location, so writing `[lo, hi)` in one access is the faithful
    /// reading, and it is also what makes the §7/P3 report name the *whole*
    /// out-of-bounds range rather than the first byte past the end.
    pub(crate) fn raw_fill(
        &mut self,
        ptr: RawPtr,
        byte: i128,
        len: usize,
        span: Span,
    ) -> EResult<()> {
        self.prov_access(ptr, len, AccessKind::Write, span)?;
        for i in 0..len {
            let at = RawPtr {
                offset: ptr.offset + i as i128,
                elem: 1,
                signed: false,
                ..ptr
            };
            self.prov().store(at, 1, byte & 0xff);
        }
        Ok(())
    }

    /// `c.memcpy` — a checked read of the source and a checked write of the
    /// destination, in that order.
    pub(crate) fn raw_copy(
        &mut self,
        dst: RawPtr,
        src: RawPtr,
        len: usize,
        span: Span,
    ) -> EResult<()> {
        self.prov_access(src, len, AccessKind::Read, span)?;
        self.prov_access(dst, len, AccessKind::Write, span)?;
        for i in 0..len {
            let from = RawPtr {
                offset: src.offset + i as i128,
                elem: 1,
                signed: false,
                ..src
            };
            let to = RawPtr {
                offset: dst.offset + i as i128,
                elem: 1,
                signed: false,
                ..dst
            };
            let byte = self.prov().load(from, 1);
            self.prov().store(to, 1, byte);
        }
        Ok(())
    }

    /// The pointer an assignment target denotes, when it is a raw one.
    ///
    /// `p[0] = 1` and `*p = 1` are not paths into this machine's slot tree —
    /// they name bytes in the provenance machine's allocations — so the
    /// assignment path asks here first and only falls back to [`Path`].
    fn raw_target(&mut self, expr: &Expr) -> EResult<Option<RawPtr>> {
        let (base, index) = match &*expr.kind {
            ExprKind::BracketApply { base, args } => {
                let [IndexArg::Value(arg)] = args.as_slice() else {
                    return Ok(None);
                };
                (base, Some(arg.expr.clone()))
            }
            ExprKind::Unary {
                op: UnOp::Deref,
                operand,
            } => (operand, None),
            _ => return Ok(None),
        };
        let value = match self.live_place(base)? {
            Some(path) => self.read_path(&path, base.span)?,
            None => match self.eval(base) {
                Ok(value) => value,
                Err(Signal::Unsupported(_)) => return Ok(None),
                Err(other) => return Err(other),
            },
        };
        let Value::Raw(ptr) = value else {
            return Ok(None);
        };
        let Some(index) = index else {
            return Ok(Some(ptr));
        };
        match self.eval(&index)? {
            Value::Int(i, _) => Ok(Some(ptr.offset_by(i))),
            other => unsupported(format!(
                "a raw pointer is indexed by an integer, got {}",
                other.kind()
            )),
        }
    }

    // -- the machine's own doors, used by `builtin` ------------------------

    pub(crate) fn out(&mut self, text: &str) {
        self.write_out(text);
    }

    /// The stderr half of the s38 writers: live pass-through only. stderr
    /// is never part of the observation record (`[proto.record]` hashes
    /// stdout alone), so there is nothing to capture — but an embedded run
    /// (corpus walk, differ, explorer) must not scribble on the HARNESS's
    /// stderr, so the write follows the same live gate as stdout's
    /// pass-through.
    pub(crate) fn err_out(&mut self, text: &str) {
        if self.shared.live_stdout {
            use std::io::Write;
            let mut err = std::io::stderr();
            let _ = err.write_all(text.as_bytes());
            let _ = err.flush();
        }
    }

    pub(crate) fn fault<T>(
        &mut self,
        kind: TrapKind,
        rule: Rule,
        span: Span,
        message: impl Into<String>,
    ) -> EResult<T> {
        self.trap(kind, rule, span, message, None)
    }

    pub(crate) fn note(&mut self, rule: Rule, span: Span, detail: &str) {
        self.fire(rule, span, detail);
    }

    pub(crate) fn invoke(&mut self, target: Value, args: Vec<Value>, span: Span) -> EResult<Value> {
        self.apply(target, args, span).map(|applied| applied.value)
    }

    /// §3's edge table, for `builtin`'s stores into region data (pool slots).
    pub(crate) fn check_edge_into(
        &mut self,
        into: Option<RegionId>,
        value: &Value,
        span: Span,
        what: &str,
    ) -> EResult<()> {
        self.check_edges(into, value, span, what)
    }
}

// -- helpers ---------------------------------------------------------------

/// The first `depth` projections of a path, rendered — the place that actually
/// moved when a read of a deeper place traps.
/// The byte/element length a `^n` endpoint counts back from, when the
/// slicing target has one (D25: `str` counts bytes, collections count
/// elements).
fn slice_len_of(target: &Value) -> Option<i128> {
    match target {
        Value::Str(s) => Some(s.len() as i128),
        _ => target.seq_slots().map(|items| items.len() as i128),
    }
}

fn prefix(path: &Path, depth: usize) -> Path {
    let mut prefix = path.clone();
    prefix.projections.truncate(depth);
    prefix
}

/// A binary operator as the programmer wrote it. Fault messages quote source,
/// never a Rust variant name.
fn spelling(op: BinOp) -> &'static str {
    match op {
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::BitAnd => "&",
        BinOp::BitXor => "^",
        BinOp::BitOr => "|",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::Cmp => "<=>",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

/// Structural value equality, blind to the width an integer is carrying.
pub(crate) fn value_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Int(a, _), Value::Int(b, _)) => a == b,
        (Value::Tuple(_) | Value::List(..), Value::Tuple(_) | Value::List(..)) => {
            let (Some(a), Some(b)) = (left.seq_slots(), right.seq_slots()) else {
                return false;
            };
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| value_eq(&x.value, &y.value))
        }
        (
            Value::Struct {
                name: an,
                fields: af,
                ..
            },
            Value::Struct {
                name: bn,
                fields: bf,
                ..
            },
        ) => {
            an == bn
                && af.len() == bf.len()
                && af
                    .iter()
                    .zip(bf)
                    .all(|((n, x), (m, y))| n == m && value_eq(&x.value, &y.value))
        }
        (Value::Error(a), Value::Error(b)) => {
            a.tag == b.tag
                && a.payload.len() == b.payload.len()
                && a.payload
                    .iter()
                    .zip(&b.payload)
                    .all(|(x, y)| value_eq(x, y))
        }
        (a, b) => a == b,
    }
}

fn qualify(module: &str, name: &str) -> String {
    if module.is_empty() {
        name.to_owned()
    } else {
        format!("{module}::{name}")
    }
}

fn split_qualified(qualified: &str) -> (String, String) {
    match qualified.split_once("::") {
        Some((module, name)) => (module.to_owned(), name.to_owned()),
        None => (String::new(), qualified.to_owned()),
    }
}

/// The spellings a primitive value's type may register an `impl` under
/// (wolf-interp#34, upstream #119/D49): `impl Text for int` mangles the
/// prim's spelling exactly as a nominal's, so the lookup tries the
/// language-default alias first where the value carries it (`int` IS i64,
/// `uint` IS u64), then the width name. A *literal* stays its
/// `[arith.literal.default]` i32 and binds no alias — `Text.text(7)` is
/// the leg prim_impl.lu's own header leaves with D49's implementing
/// campaign, on both machines.
fn prim_type_names(value: &Value) -> Vec<String> {
    match value {
        Value::Int(_, ty) => {
            let width = ty.name();
            if ty.literal {
                return vec![width];
            }
            match width.as_str() {
                "i64" => vec!["int".to_owned(), width],
                "u64" => vec!["uint".to_owned(), width],
                _ => vec![width],
            }
        }
        Value::Float(_) => vec!["float".to_owned(), "f64".to_owned()],
        Value::Str(_) => vec!["str".to_owned()],
        Value::Bool(_) => vec!["bool".to_owned()],
        _ => Vec::new(),
    }
}

/// Are values of this shape `Copy` (`[mem.tier0.move.3]`: "POD-shaped types
/// only")?
///
/// Scalars and `str` are; aggregates are not. `str` is the judgement call: it
/// owns bytes, but wolf strings are immutable views (D25) and treating them as
/// non-`Copy` would make the dynamic machine *stricter* than the compiler,
/// which is the one direction the sprint forbids.
///
/// The Tier-1/2 granules split: `handle` is `Copy`-shaped — spec/02 Appendix A
/// annotates `var cur = hs[0]` with `[mem.tier0.move.3]` — while region values
/// are affine by `[mem.region.create.2]`, and `shared`/`weak`/pool references
/// carry ownership that a silent duplicate would forge.
fn is_copy(value: &Value) -> bool {
    matches!(
        value,
        Value::Unit
            | Value::Bool(_)
            | Value::Int(..)
            | Value::Float(_)
            | Value::Str(_)
            | Value::Range { .. }
            | Value::Fn(_)
            | Value::Module(_)
            | Value::Builtin(_)
            | Value::Handle(_)
            // `[mem.unsafe.raw.1]`: "copies of raw pointers are unrestricted".
            // A copy shares the tag — no retag, no obligation, which is the
            // whole of D11's "simpler than safe".
            | Value::Raw(_)
            // The concurrency granules are reference-shaped ids into the
            // scheduler: scope handles are passable capabilities (D16),
            // channels and Mutexes are `sync`-shaped and share, proc handles
            // name a failure domain, durations are plain data (spec/03).
            | Value::Scope(_)
            | Value::Chan(_)
            | Value::MutexRef(_)
            | Value::Proc(_)
            | Value::Duration(_)
    )
}

/// A compound-assignment operator's binary operator.
fn assign_binop(op: AssignOp) -> BinOp {
    match op {
        AssignOp::Assign | AssignOp::Add => BinOp::Add,
        AssignOp::Sub => BinOp::Sub,
        AssignOp::Mul => BinOp::Mul,
        AssignOp::Div => BinOp::Div,
        AssignOp::Rem => BinOp::Rem,
        AssignOp::BitAnd => BinOp::BitAnd,
        AssignOp::BitOr => BinOp::BitOr,
        AssignOp::BitXor => BinOp::BitXor,
        AssignOp::Shl => BinOp::Shl,
        AssignOp::Shr => BinOp::Shr,
    }
}

/// A raw pointer's pointee size in bytes and signedness — `*u8` is `(1, false)`,
/// `*int` is `(8, true)`. What `p[i]` addresses, and what `assume noalias`
/// compares (`[mem.unsafe.raw.2]` asserts about *ranges*).
fn pointee(ty: &Type) -> (usize, bool) {
    let name = type_name(ty);
    match IntTy::named(&name) {
        Some(int) => ((int.bits / 8).max(1) as usize, int.signed),
        // An unknown pointee is one byte: the conservative choice, since a
        // narrower range can only *miss* an overlap, never invent one.
        None => (1, false),
    }
}

/// The last segment of a type's path — `pool(Node)`'s `Node`, `Pool[Node]`'s.
fn type_name(ty: &Type) -> String {
    match &*ty.kind {
        TypeKind::Path { path, .. } => path
            .segments
            .last()
            .map(|segment| segment.name.clone())
            .unwrap_or_default(),
        TypeKind::Prefixed { ty, .. }
        | TypeKind::ErrorUnion(ty)
        | TypeKind::Fallible { ty, .. } => type_name(ty),
        _ => "_".to_owned(),
    }
}

/// The `wrapping`/`saturating` mode a wrapper name spells, if either.
fn arith_mode_named(name: &str) -> Option<ArithMode> {
    match name {
        "wrapping" => Some(ArithMode::Wrapping),
        "saturating" => Some(ArithMode::Saturating),
        _ => None,
    }
}

/// The integer type a type annotation names, when it names one: a bare
/// width name (`i32`, `u64`, `int`…) or a `wrapping[uN]`/`saturating[uN]`
/// wrapper — the wrapper resolves in every position a bare width does
/// (#43, mirroring wolf-lang#132's container-element ruling; a bare
/// `wrapping` with no argument wraps the `int` default, as `coerce`
/// always read it).
fn int_of_type(ty: &Type) -> Option<IntTy> {
    match &*ty.kind {
        TypeKind::Path { path, args } => {
            let name = path.segments.last().map_or("", |s| s.name.as_str());
            if let Some(mode) = arith_mode_named(name) {
                let inner = args.iter().find_map(|arg| match arg {
                    TypeArg::Type(inner) => int_of_type(inner),
                    TypeArg::Expr(_) => None,
                });
                return Some(inner.unwrap_or(IntTy::INT).with_mode(mode));
            }
            IntTy::named(name)
        }
        _ => None,
    }
}

/// [`int_of_type`]'s value-position twin: the integer type a value-position
/// type SPELLING names. `e[…]` is one production and type-vs-value is
/// sema's call (D29), so `List[i32]`'s element annotation arrives as a
/// value-position path and `List[wrapping[u64]]`'s as a value-position
/// bracket apply (#43); the prefix-keyword forms arrive as real types.
fn int_of_type_spelling(expr: &Expr) -> Option<IntTy> {
    match &*expr.kind {
        ExprKind::Path(path) if path.is_single() => IntTy::named(path.segments[0].name.as_str()),
        ExprKind::BracketApply { base, args } => {
            let ExprKind::Path(path) = &*base.kind else {
                return None;
            };
            let mode = arith_mode_named(path.segments.last().map_or("", |s| s.name.as_str()))?;
            let [arg] = &args[..] else { return None };
            let inner = match arg {
                IndexArg::Type(ty) => int_of_type(ty),
                IndexArg::Value(arg) => int_of_type_spelling(&arg.expr),
            };
            Some(inner.unwrap_or(IntTy::INT).with_mode(mode))
        }
        _ => None,
    }
}

/// `List[i32]()` — the element checking context on a `List` constructor's
/// callee, read off the syntax (issue #21): `eval_bracket` erases bracket
/// type arguments from values, so this is where the annotation survives.
fn list_elem_of(callee: &Expr) -> Option<IntTy> {
    if let ExprKind::BracketApply { base, args } = &*callee.kind
        && let ExprKind::Path(path) = &*base.kind
        && path.segments.last().is_some_and(|s| s.name == "List")
        && let [arg] = &args[..]
    {
        return match arg {
            IndexArg::Type(ty) => int_of_type(ty),
            IndexArg::Value(arg) => int_of_type_spelling(&arg.expr),
        };
    }
    None
}

/// The runtime half of D54.2's operator bridge (`[type.numlit.propagate]`): an
/// integer LITERAL meeting a float operand adopts that float's type, so the
/// operator runs as float arithmetic. Adoption is literal-only — a concrete int
/// VALUE (`ty.literal == false`) is left untouched so the mixed-operand refusal
/// still fires (D54.3, `[type.numlit.value]`).
fn adopt_numeric(left: Value, right: Value) -> (Value, Value) {
    match (left, right) {
        (Value::Float(f), Value::Int(v, t)) if t.literal => {
            (Value::Float(f), Value::Float(v as f64))
        }
        (Value::Int(v, t), Value::Float(f)) if t.literal => {
            (Value::Float(v as f64), Value::Float(f))
        }
        (left, right) => (left, right),
    }
}

/// The float type a declared type names (`f32`/`f64`), or `None`. The value-
/// position twin of [`int_of_type`] for D54.1's float expectation.
fn float_ty_name(ty: &Type) -> Option<&str> {
    match &*ty.kind {
        TypeKind::Path { path, .. } => match path.segments.last().map(|s| s.name.as_str()) {
            Some(name @ ("f32" | "f64")) => Some(name),
            _ => None,
        },
        _ => None,
    }
}

/// Applies a declared type to a value: the checking context sema-lite provides.
fn coerce(value: Value, ty: Option<&Type>) -> Value {
    let Some(ty) = ty else { return value };
    match (&*ty.kind, value) {
        // A `List[T]` annotation stamps the element checking context onto an
        // untagged container (issue #21) — `var l: List[i32] = …`, and the
        // declared type of a `mut`/`take` parameter.
        (TypeKind::Path { path, args }, Value::List(items, None, home))
            if path.segments.last().is_some_and(|s| s.name == "List") =>
        {
            let elem = args.iter().find_map(|arg| match arg {
                TypeArg::Type(inner) => int_of_type(inner),
                TypeArg::Expr(_) => None,
            });
            Value::List(items, elem, home)
        }
        (TypeKind::Path { .. }, Value::Int(v, current)) => {
            // D54.1 `[type.numlit.adopt]`: an integer LITERAL satisfies a float
            // expectation — `let x: f64 = 0` binds `0.0`, lossless because the
            // literal denotes an exact value. Literal-only: a concrete int VALUE
            // is left alone here (`[type.numlit.value]` refuses it at the
            // binding). One-directional — this never runs for a `{float}`
            // literal, which stays a float below.
            if current.literal && float_ty_name(ty).is_some() {
                return Value::Float(v as f64);
            }
            // `int_of_type` reads bare width names AND the wrapping/
            // saturating wrappers (its doc says how a bare wrapper reads);
            // an annotation naming neither leaves the value's type alone.
            match int_of_type(ty) {
                Some(named) => Value::Int(v, named),
                None => Value::Int(v, current),
            }
        }
        (TypeKind::Path { path, .. }, Value::Float(v)) => {
            let _ = path;
            Value::Float(v)
        }
        (TypeKind::ErrorUnion(inner), value) => coerce(value, Some(inner)),
        // `T ! {row}` checks the ok payload against `T`; an error value is
        // already row-shaped and passes through untouched.
        (TypeKind::Fallible { ty: inner, .. }, value) => match value {
            Value::Error(_) => value,
            value => coerce(value, Some(inner)),
        },
        (_, value) => value,
    }
}

fn parse_int(text: &str) -> EResult<i128> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    let (radix, digits) = if let Some(rest) = cleaned.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = cleaned.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = cleaned.strip_prefix("0b") {
        (2, rest)
    } else {
        (10, cleaned.as_str())
    };
    i128::from_str_radix(digits, radix)
        .map_err(|_| Signal::Unsupported(format!("integer literal `{text}` does not fit in i128")))
}

fn parse_float(text: &str) -> EResult<f64> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned
        .parse()
        .map_err(|_| Signal::Unsupported(format!("float literal `{text}` does not parse")))
}

/// Runs one program from source, with no filesystem module graph.
///
/// # Errors
///
/// The load error, when the source does not lex or parse.
pub fn run_source(name: &str, source: &str) -> Result<Run, crate::sema::LoadError> {
    let program = crate::sema::load_source(name, source)?;
    Ok(Machine::new(&program).run())
}

/// As [`run_source`], with a schedule seed and a trace filter — what the is06
/// determinism tests replay (`[conc.det.seed]`).
///
/// # Errors
///
/// The load error, when the source does not lex or parse.
pub fn run_source_seeded(
    name: &str,
    source: &str,
    seed: Option<u64>,
    trace: Trace,
) -> Result<Run, crate::sema::LoadError> {
    let program = crate::sema::load_source(name, source)?;
    Ok(Machine::with_seed(&program, seed).tracing(trace).run())
}

#[cfg(test)]
mod tests;

//! The rule registry — is02's clause-citation discipline, as data.
//!
//! Target 7 of the sprint: *"Every dynamic rule lives in a rule registry:
//! `{anchor, description, fault constructor}`. `--trace` logs each rule as it
//! fires. A unit test walks the registry and the evaluator's fault sites: a rule
//! without an anchor, or an anchor absent from the pinned spec, fails the build.
//! 'Every rule traceable to a spec clause' is thereby a test, not a review
//! item."*
//!
//! The shape that makes this cheap: [`Rule`] is a C-like enum, so every
//! evaluation step in `eval` names the rule it is applying by *type* rather than
//! by string, and the compiler's exhaustiveness check does half the work. The
//! other half is [`REGISTRY`], which pairs each variant with its clause anchor
//! and one sentence; [`validate`] is the predicate the tests run against it, and
//! it is deliberately callable on a hand-built table so a *planted* anchorless
//! rule can be shown to fail without editing the real one.
//!
//! Anchors in a registered namespace (`gram`, `mem`, `conf`, …) must resolve
//! against the pinned `spec/anchors.json`; anchors in a reserved forward
//! namespace (`arith`, `err`, `str`, …) are legal and counted as forward
//! (`[conf.anchor.ns]`) — the documents that will own them are not written yet,
//! and `arith.checked` is exactly the kind of rule this sprint implements.

use std::fmt;

/// Every dynamic rule this evaluator implements.
///
/// A variant here is a promise: somewhere in `eval` there is a step that cites
/// it, and `REGISTRY` gives it a clause. Adding a variant without a registry row
/// fails to compile (the table is exhaustive-matched in [`Rule::row`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    // -- §1 model ----------------------------------------------------------
    /// Whole-value assignment and argument passing; values have no identity.
    ValueSemantics,
    /// Places are denoted by paths with field/index projections.
    PlacePath,
    /// Two paths conflict iff one is a prefix of the other.
    PathDisjoint,
    /// Struct literals and collection constructors are the allocation sites.
    Alloc,

    // -- §2 moves ----------------------------------------------------------
    /// Initialization, assignment, `take` and `return` move; the source place
    /// becomes uninitialized.
    Move,
    /// Reading a moved-from place traps `use-after-move`.
    UseAfterMove,
    /// `copy x` produces an independent value.
    Copy,
    /// A moved-from place may be re-initialized by assignment.
    Reinit,

    // -- §2 parameter modes ------------------------------------------------
    /// Default mode: immutable for the whole call, caller retains.
    ModeRead,
    /// `mut`: exclusive inout for the call's extent.
    ModeMut,
    /// `take`: the argument moves into the callee.
    ModeTake,

    // -- §2 exclusivity ----------------------------------------------------
    /// A `mut`-held place has no other live access path.
    Exclusivity,
    /// Disjoint paths may be `mut` simultaneously.
    ExclusivityDisjoint,
    /// A view set bounds the callee's path footprint.
    ViewSet,

    // -- §2 borrows --------------------------------------------------------
    /// `&path` / `&mut path` create local borrows.
    Borrow,
    /// A live `&mut` holds its path exclusively; `&` read-freezes it.
    BorrowExtent,

    // -- defined behavior (§7) --------------------------------------------
    /// Integer overflow traps in every profile.
    ArithChecked,
    /// `wrapping`/`saturating` types are the spelling for intended overflow.
    ArithWrapping,
    /// Division (and remainder) by zero traps `div-zero`.
    DivZero,
    /// Out-of-bounds indexing and slicing trap `bounds`.
    Bounds,
    /// An untyped integer literal defaults to `i32`, a float to `f64`, unless a
    /// checking context supplies a type.
    LiteralDefault,
    /// `char as int` is total; `int as char` traps `overflow` on a non-scalar
    /// — negative, above `0x10FFFF`, or the surrogate gap (s121, D58).
    CharCast,

    // -- §3 Tier 1: regions (is03) ----------------------------------------
    /// `region name { }` sugar and `region(…)` values both create a region.
    RegionCreate,
    /// Region values are affine: they move, are never copied, and denote
    /// distinct regions.
    RegionAffine,
    /// Ambient allocation: heap values land in the current region.
    RegionAmbient,
    /// Region identity is a static fact with zero runtime representation.
    RegionIdentity,
    /// Within one region, references are unrestricted — cycles included.
    RegionIntra,
    /// A region dies as a unit: wholesale free, never per-allocation.
    RegionFree,
    /// The §3 edge table, checked at every store of a reference.
    RegionEdge,
    /// A region has at most one owning edge: the forest invariant.
    RegionEdgeIso,
    /// Frozen data may be referenced from anywhere, forever.
    RegionEdgeImm,
    /// Entering opens a region; leaving closes it (Suspended).
    RegionOpen,
    /// Multiple disjoint regions may be open simultaneously.
    RegionMultiopen,
    /// A suspended region's contents are unreachable for writing.
    RegionSuspended,
    /// `freeze r` promotes the whole graph to `imm`, deep and in place.
    RegionFreeze,
    /// `move r` transfers the region value; the old binding is moved-from.
    RegionTransfer,
    /// Freezing or transferring a region with an open child is refused.
    RegionClosedSubtree,

    // -- §4 Tier 2: `shared` and `handle` (is03) --------------------------
    /// `shared T` is a refcounted cell; the value drops at the last strong
    /// release.
    SharedRc,
    /// Strong `shared` edges are acyclic; there is no cycle collector.
    SharedAcyclic,
    /// `weak` keeps nothing alive; upgrading is option-shaped.
    SharedWeak,
    /// A `shared` payload's destructor runs at the last strong release point.
    SharedDrop,
    /// `handle T` is a generational index into a `pool(T)`; pools are
    /// two-phase.
    HandleTwoPhase,
    /// A stale handle is a deterministic fault in every profile.
    HandleStale,
    /// `pool[h]` accesses the slot under Tier-0 exclusivity; the pool is the
    /// place base.
    HandleAccess,

    // -- §5 Tier 3: unsafe (is04) -----------------------------------------
    /// Raw pointers carry no aliasing assumptions; arithmetic, casts and copies
    /// of them are unrestricted.
    UnsafeRaw,
    /// `assume noalias p, q` asserts the pointed-to ranges are disjoint; a
    /// false assertion is UB.
    AssumeNoalias,
    /// `borrow r from ptr` and the checked `handle` are the only two doors back
    /// into the safe world.
    UnsafeDoor,
    /// `unsafe { }` appears only inside fully-safe signatures; the module is the
    /// audit granule.
    UnsafeScope,
    /// A C call executes against an implicit region borrowed for the call's
    /// extent.
    BoundaryFfi,

    // -- §6 provenance (is04) ---------------------------------------------
    /// Every pointer carries a tag; every allocation carries a tree of them,
    /// and new tags are children of the tag they derive from.
    ProvTag,
    /// The per-tag, per-location state machine and its transition table.
    ProvState,
    /// Int→ptr casts resolve angelically among exposed tags; ptr→int exposes.
    ProvExpose,
    /// Freeing a region Disables every tag tree it owns; freezing Frozens them.
    ProvRegion,

    // -- §7 the UB enumeration (is04) --------------------------------------
    /// A row of the closed UB enumeration was reached.
    Ub,
    /// Every row of the enumeration names the optimization it licenses (D2).
    UbLicensed,
    /// `ub(anchor)` is a protocol verdict that participates in comparison as
    /// the highest-severity divergence class.
    UbVerdict,

    // -- expression-oriented control flow ---------------------------------
    /// A block yields its tail expression.
    Block,
    /// `if`/`match`/`for`/`while`/`loop` are expressions; `return`/`break`/
    /// `continue` are jumps.
    Flow,
    /// `&&`/`||` short-circuit; every other operand position evaluates
    /// left-to-right.
    EvalOrder,
    /// Evaluation is strict and left-to-right everywhere; nothing is
    /// unsequenced.
    EvalStrictOrder,
    /// Assignment is a statement and its place is a path.
    Assign,
    /// A call binds arguments to parameters by mode and evaluates the body.
    Call,
    /// A closure captures its free variables by value at construction.
    Closure,

    // -- errors as values (D30) -------------------------------------------
    /// `!T` values are ok-or-tagged; there is no unwinding.
    ErrUnion,
    /// Error tags are structural: a row entry needs no declaration.
    ErrRows,
    /// `?` returns the error to the caller, widening the row by union.
    ErrPropagate,
    /// `else` / `else |err|` default an error away.
    ErrElse,
    /// `errdefer` runs on the error path only.
    ErrDefer,
    /// Scope-exit effects run LIFO.
    DeferLifo,

    // -- strings -----------------------------------------------------------
    /// Every string literal is an f-string; interpolations evaluate in place.
    StrInterp,

    // -- traps -------------------------------------------------------------
    /// A user assertion that fails traps `assert`.
    Assert,
    /// Traps map onto the closed twelve-kind vocabulary and terminate.
    TrapVocabulary,

    // -- spec/03: the sim scheduler (is06) ---------------------------------
    /// The sched-ev/0 stream opens with its seed; `--seed=N` replays exactly.
    SchedSeed,
    /// Spawn commit: a task is created, named, under a scope and a proc.
    SchedSpawn,
    /// A task parks at a runtime-owned blocking point.
    SchedPark,
    /// A blocked task becomes runnable again.
    SchedUnpark,
    /// One scheduling decision: which ready task runs next.
    SchedDecision,
    /// A send↔receive pairing, per channel, per k.
    SchedChan,
    /// A `select` arm commits among the ready set — seeded, recorded.
    SchedSelect,
    /// A Mutex/`when` acquisition or release, ordered per sync object.
    SchedAcquire,
    /// A virtual-clock advance or timer fire.
    SchedTimer,

    // -- spec/03 §2: tasks and scopes --------------------------------------
    /// `scope name? { … }` opens a structured-concurrency scope.
    TaskScope,
    /// Scope exit joins all children before the block completes.
    TaskJoin,
    /// A failing child cancels its siblings and re-raises at the scope exit.
    TaskFail,
    /// Tasks and procs carry names surfaced in the structured dump.
    TaskName,
    /// The process runs under a root supervisor scope of process lifetime.
    TaskRoot,

    // -- spec/03 §3: channels, select, cancellation ------------------------
    /// `channel[T](n)` requires a sendable payload.
    ChanType,
    /// Capacity n ≥ 1 buffers; n = 0 is rendezvous; full/empty block.
    ChanBuf,
    /// `close` drains buffers; further sends and drained receives get the
    /// closed error value — never UB, never a fault.
    ChanClose,
    /// A region `move`d through a channel publishes the whole graph.
    ChanMove,
    /// A sender's later touch of a sent region faults.
    ChanStale,
    /// Frozen data shares by reference across tasks — no transfer.
    ChanImm,
    /// Cancellation is cooperative, delivered at runtime-owned blocking
    /// points only.
    CancelPoint,
    /// A cancelled task's own defer/errdefer run as its frames return.
    CancelDefer,
    /// C frames are never unwound; cancellation waits for the next safe point.
    CancelFfi,

    // -- spec/03 §3: procs -------------------------------------------------
    /// A proc is a failure domain owning its regions.
    ProcModel,
    /// `link` couples fates symmetrically.
    ProcLink,
    /// `monitor` delivers the exit reason asynchronously as a typed value.
    ProcMonitor,
    /// Exit reasons are a closed set: normal, error, killed, cancelled.
    ProcExit,
    /// The killed-proc sequence: cancel without user code, free, deliver.
    ProcKill,
    /// Procs communicate exclusively via typed channels + `select`.
    ProcMailbox,

    // -- `when` (03 Q6; clauses landed in the s20 S-batch) ------------------
    /// `when (a, b)` acquires the whole set in canonical order.
    WhenOrder,
    /// No lock-order deadlock can form: every `when` uses the one order.
    WhenNoDeadlock,
    /// The body runs with exclusive payload access; write-back at release.
    WhenBody,
    /// Lexically nested `when` is the compiler's E1103; the dynamic
    /// already-held case is `[conc.deadlock.self]`.
    WhenNoNest,

    // -- deadlock (the defined outcome, `[conc.deadlock]`) ------------------
    /// Every live task blocked, no timer, no I/O: a defined outcome.
    DeadlockDef,
    /// A detected deadlock traps `deadlock` with the blocked-task roster.
    DeadlockTrap,
    /// Acquiring a sync object the task already holds can never complete.
    DeadlockSelf,

    // -- the S-batch's remaining machine-confirmed choices ------------------
    /// A drained-closed channel makes its `select` receive arm ready.
    SelectClosed,
    /// A failing child's cancellation reaches a blocked scope owner too.
    TaskFailOwner,
    /// `w.cancel()` delivers structured cancellation to a proc.
    ProcCancel,
    /// `a.link(b)` couples two procs symmetrically; `w.link()` sugars it.
    ProcLinkPair,
    /// The root supervisor's domain is the process; its abnormal death runs
    /// the killed-proc sequence for every live proc and exits nonzero.
    ProcRoot,

    // -- races -------------------------------------------------------------
    /// A detected data race halts with trap kind `race`.
    RaceDetect,
    /// Record/replay/free — the conforming runtime's three modes.
    DetMode,
}

/// One registry row: the rule, its clause anchor, and one sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub rule: Rule,
    pub anchor: &'static str,
    pub description: &'static str,
}

impl Rule {
    /// This rule's registry row. Exhaustive by construction — a new variant
    /// without a row does not compile.
    #[must_use]
    pub const fn row(self) -> Row {
        let (anchor, description) = match self {
            Rule::ValueSemantics => (
                "mem.model.value",
                "assignment and argument passing transfer or copy the whole value",
            ),
            Rule::PlacePath => (
                "mem.model.place",
                "a place is a base binding plus field/index projections",
            ),
            Rule::PathDisjoint => (
                "mem.model.path.disjoint",
                "two paths conflict iff one is a prefix of the other",
            ),
            Rule::Alloc => (
                "mem.model.alloc",
                "struct literals and collection constructors are the allocation sites",
            ),
            Rule::Move => (
                "mem.tier0.move.1",
                "initialization, assignment, `take` and `return` move; the source becomes uninitialized",
            ),
            Rule::UseAfterMove => (
                "mem.tier0.move.2",
                "reading an uninitialized or moved-from place traps `use-after-move`",
            ),
            Rule::Copy => (
                "mem.tier0.move.3",
                "`copy x` produces an independent value from any type",
            ),
            Rule::Reinit => (
                "mem.tier0.move.4",
                "a moved-from place may be re-initialized by assignment and is then live again",
            ),
            Rule::ModeRead => (
                "mem.tier0.mode.read",
                "the default mode reads a value that is immutable for the whole call",
            ),
            Rule::ModeMut => (
                "mem.tier0.mode.mut",
                "`mut` parameters are exclusive inout for the duration of the call",
            ),
            Rule::ModeTake => (
                "mem.tier0.mode.take",
                "`take` consumes: the argument moves into the callee",
            ),
            Rule::Exclusivity => (
                "mem.tier0.excl.1",
                "a place held through `mut` has no other live access path; violations trap `exclusivity`",
            ),
            Rule::ExclusivityDisjoint => (
                "mem.tier0.excl.2",
                "disjoint paths may be `mut` simultaneously: `f(mut a.x, mut a.y)` is legal",
            ),
            Rule::ViewSet => (
                "mem.tier0.excl.3",
                "a view set declares the callee's path footprint and is part of the signature",
            ),
            Rule::Borrow => (
                "mem.tier0.borrow.1",
                "`&path` / `&mut path` create local borrows that never outlive their activation",
            ),
            Rule::BorrowExtent => (
                "mem.tier0.borrow.2",
                "a live `&mut` holds its path exclusively; live `&` borrows read-freeze it",
            ),
            Rule::ArithChecked => (
                "arith.checked",
                "integer overflow traps `overflow` in every profile (X3); debug and release agree",
            ),
            Rule::ArithWrapping => (
                "arith.wrapping",
                "`wrapping`/`saturating` types are the spelling for intended overflow",
            ),
            Rule::DivZero => (
                "mem.ub.defined",
                "division or remainder by zero is defined behavior: it traps `div-zero`",
            ),
            Rule::Bounds => (
                "mem.ub.defined",
                "an out-of-bounds index or slice is defined behavior: it traps `bounds`",
            ),
            Rule::LiteralDefault => (
                "arith.literal.default",
                "an unconstrained integer literal defaults to i32 and a float literal to f64",
            ),
            Rule::CharCast => (
                "type.char.cast",
                "`char as int` is total; `int as char` traps `overflow` on a non-scalar (negative, above 0x10FFFF, or the surrogate gap)",
            ),
            Rule::RegionCreate => (
                "mem.region.create.1",
                "`region name { … }` sugar and `region(…)` values both create a region with a strategy",
            ),
            Rule::RegionAffine => (
                "mem.region.create.2",
                "region values are affine: they move, are never copied, and denote distinct regions",
            ),
            Rule::RegionAmbient => (
                "mem.region.create.3",
                "every function executes with a current region and heap allocations land there",
            ),
            Rule::RegionIdentity => (
                "mem.region.create.4",
                "region identity is a static fact the dynamic machine tracks and compiled code need not",
            ),
            Rule::RegionIntra => (
                "mem.region.intra.1",
                "within one region references are unrestricted: cycles and back-edges are safe",
            ),
            Rule::RegionFree => (
                "mem.region.intra.2",
                "a region dies as a unit — every allocation in it is freed wholesale",
            ),
            Rule::RegionEdge => (
                "mem.region.edge",
                "every store of a reference is checked against the cross-region edge table",
            ),
            Rule::RegionEdgeIso => (
                "mem.region.edge.iso",
                "a region has at most one owning edge; the owning handle is affine like the region",
            ),
            Rule::RegionEdgeImm => (
                "mem.region.edge.imm",
                "frozen (`imm`) data may be referenced from anywhere, forever",
            ),
            // Both of these anchors were **repaired upstream** on 2026-08-09 in
            // response to is03's verdict, and the machine's behaviour is now the
            // normative reading rather than a proposal: openness is depth-counted
            // (`in a { … }` inside `region a { … }` is idempotent) and the open
            // set must be an antichain in the region forest.
            Rule::RegionOpen => (
                "mem.region.open.1",
                "entering `in r { … }` or the sugar block opens a region; exit closes it (Suspended), and re-entry is depth-counted idempotent",
            ),
            Rule::RegionMultiopen => (
                "mem.region.multiopen",
                "the open set must be an antichain in the region forest: disjoint siblings may be open at once, an owner and its child may not",
            ),
            Rule::RegionSuspended => (
                "mem.region.open.3",
                "a suspended region's contents are unreachable for writing",
            ),
            Rule::RegionFreeze => (
                "mem.region.freeze.1",
                "`freeze r` consumes the region value and promotes the whole graph to `imm`, deep and in place",
            ),
            Rule::RegionTransfer => (
                "mem.region.freeze.2",
                "`move r` transfers the region value; any use of the old binding is a moved-from use",
            ),
            Rule::RegionClosedSubtree => (
                "mem.region.freeze.3",
                "freezing or transferring a region containing an open child is refused: closed subtrees only",
            ),
            Rule::SharedRc => (
                "mem.shared.rc.1",
                "`shared T` is a refcounted cell; clones share ownership and it drops at the last strong release",
            ),
            Rule::SharedAcyclic => (
                "mem.shared.rc.2",
                "strong `shared` edges are acyclic — wolf has no cycle collector and refuses to leak instead",
            ),
            Rule::SharedWeak => (
                "mem.shared.rc.3",
                "`weak T` keeps nothing alive; upgrading yields an option-shaped result the caller handles",
            ),
            Rule::SharedDrop => (
                "mem.shared.drop.3",
                "a `shared` payload's destructor runs when the last strong count drops, at that release point",
            ),
            Rule::HandleTwoPhase => (
                "mem.shared.handle.1",
                "`handle T` is a generational index into a `pool(T)`; `reserve` then `init`, so no null handles exist",
            ),
            Rule::HandleStale => (
                "mem.shared.handle.2",
                "accessing a freed or re-generationed slot is a deterministic fault in every profile",
            ),
            Rule::HandleAccess => (
                "mem.shared.handle.3",
                "`pool[h]` accesses the slot under Tier-0 exclusivity rules; the pool is the place base",
            ),
            Rule::UnsafeRaw => (
                "mem.unsafe.raw.1",
                "raw pointers carry no aliasing assumptions; their arithmetic, casts and copies are unrestricted",
            ),
            Rule::AssumeNoalias => (
                "mem.unsafe.raw.2",
                "`assume noalias p, q` asserts the pointed-to ranges do not overlap; a false assertion is UB",
            ),
            Rule::UnsafeDoor => (
                "mem.unsafe.door",
                "`borrow r from ptr` and the checked `handle` are the only doors back into the safe world",
            ),
            Rule::UnsafeScope => (
                "mem.unsafe.scope",
                "`unsafe { }` blocks appear only inside fully-safe signatures; the module is the audit granule",
            ),
            Rule::BoundaryFfi => (
                "mem.boundary.ffi",
                "a C call executes against an implicit region borrowed for the call's extent",
            ),
            Rule::ProvTag => (
                "mem.prov.tag",
                "every pointer carries a tag and every allocation a tree of them; new tags are children of their provenance parent",
            ),
            Rule::ProvState => (
                "mem.prov.state",
                "each tag is Reserved, Active, Frozen or Disabled per location, and every access applies the transition table",
            ),
            Rule::ProvExpose => (
                "mem.prov.expose",
                "ptr→int exposes a tag; int→ptr resolves angelically among the exposed ones",
            ),
            Rule::ProvRegion => (
                "mem.prov.region",
                "freeing a region Disables every tag tree it owns; `freeze` transitions all of them to Frozen",
            ),
            Rule::Ub => (
                "mem.ub",
                "the UB enumeration is closed: an execution reaching a row has no defined behavior",
            ),
            Rule::UbLicensed => (
                "mem.ub.closed",
                "zero rows without a named licensed optimization — the D2 ratchet, carried on every report",
            ),
            Rule::UbVerdict => (
                "proto.record.ub",
                "`ub(anchor)` cites the §7 row and participates in comparison as a soundness-candidate divergence",
            ),
            Rule::Block => (
                "gram.expr.block",
                "a block evaluates its statements and yields its tail expression",
            ),
            Rule::Flow => (
                "gram.expr.flow",
                "`if`/`match`/`for`/`while`/`loop` are expressions; `return`/`break`/`continue` jump",
            ),
            Rule::EvalOrder => (
                "gram.expr.prec",
                "`&&`/`||` short-circuit; other operand positions evaluate left to right",
            ),
            Rule::EvalStrictOrder => (
                "mem.model.order",
                "evaluation is strict and left-to-right everywhere: nothing is unsequenced",
            ),
            Rule::Assign => (
                "gram.expr.assign",
                "assignment is a statement whose left side denotes a place",
            ),
            Rule::Call => (
                "gram.item.fn",
                "a call binds each argument to its parameter by mode, then evaluates the body",
            ),
            Rule::Closure => (
                "gram.expr.closure",
                "a closure captures its free variables by value when it is constructed",
            ),
            Rule::ErrUnion => (
                "err.union",
                "`!T` values are ok-or-tagged data; nothing unwinds",
            ),
            Rule::ErrRows => (
                "err.rows",
                "error tags are structural — a row entry needs no declaration to exist",
            ),
            Rule::ErrPropagate => (
                "err.propagate",
                "`?` returns the error to the caller, widening the row by union",
            ),
            Rule::ErrElse => (
                "err.else",
                "`else`, `else |err|` and `else |err| { … }` default an error away",
            ),
            Rule::ErrDefer => (
                "err.errdefer",
                "`errdefer` runs when the scope is left on the error path, and only then",
            ),
            Rule::DeferLifo => (
                "mem.shared.drop.1",
                "scope-exit effects run in reverse registration order",
            ),
            Rule::StrInterp => (
                "str.interp",
                "every string literal is an f-string; each interpolation evaluates in place",
            ),
            Rule::Assert => (
                "conf.trap.map",
                "a failed user assertion, or a ruled caller-contract violation of a builtin \
                 surface (`[mem.str.repeat]`), traps `assert`",
            ),
            Rule::TrapVocabulary => (
                "conf.trap.set",
                "every fault this machine raises is one of the closed twelve kinds",
            ),
            Rule::SchedSeed => (
                "conc.det.seed",
                "one seed selects the whole schedule; `--seed=N` regenerates the identical decision stream",
            ),
            Rule::SchedSpawn => (
                "conc.task.spawn",
                "spawn commit: the closure's captures obey D14 and the task enters the ready set",
            ),
            Rule::SchedPark => (
                "conc.det.events",
                "park: a task blocks at a runtime-owned primitive — a recorded event",
            ),
            Rule::SchedUnpark => (
                "conc.det.events",
                "unpark: a blocked task becomes runnable — a recorded event",
            ),
            Rule::SchedDecision => (
                "conc.det.events",
                "a scheduling decision picks the next runnable task from the ready set",
            ),
            Rule::SchedChan => (
                "conc.mm.hb.chan",
                "the k-th send on a channel happens-before the k-th receive completes; rendezvous also orders the return",
            ),
            Rule::SchedSelect => (
                "conc.select.fair",
                "among simultaneously-ready arms the choice is pseudo-random from the scheduler seed — recorded, never wall-clock incidental",
            ),
            Rule::SchedAcquire => (
                "conc.mm.hb.mutex",
                "the n-th release of a Mutex (or `when` exit) happens-before the n+1-th acquisition",
            ),
            Rule::SchedTimer => (
                "conc.select.timeout",
                "timer arms fire on the scheduler's clock — virtual under test — and each fire is a recorded event",
            ),
            Rule::TaskScope => (
                "conc.task.scope",
                "`scope name? { … }` opens a structured scope; handles are ordinary passable values; no detached spawn exists",
            ),
            Rule::TaskJoin => (
                "conc.task.join",
                "scope exit joins all children: the block completes only when every spawned task has completed or finished its cancellation",
            ),
            Rule::TaskFail => (
                "conc.task.fail",
                "a child completing with an error or fault cancels its siblings and re-raises at the scope exit, first failure in schedule order",
            ),
            Rule::TaskName => (
                "conc.task.name",
                "tasks and procs carry names surfaced in the structured dump; the dump's existence is contract",
            ),
            Rule::TaskRoot => (
                "conc.task.root",
                "the process runs under a root supervisor scope; daemon-shaped work is named, supervised and enumerable",
            ),
            Rule::ChanType => (
                "conc.chan.type",
                "`channel[T](n)` requires T sendable: Copy, imm, a region value moved on send, or a sync type",
            ),
            Rule::ChanBuf => (
                "conc.chan.buf",
                "capacity n ≥ 1 buffers, n = 0 is rendezvous; full sends and empty receives block as recorded cancellation points",
            ),
            Rule::ChanClose => (
                "conc.chan.close",
                "`close` makes further sends return an error value; buffered items drain; drained receives get the closed error",
            ),
            Rule::ChanMove => (
                "conc.chan.move",
                "sending a region value is its affine move: the closed, disconnected subtree transfers wholesale and every prior write publishes ([conc.mm.hb.move])",
            ),
            Rule::ChanStale => (
                "conc.chan.staleuse",
                "after a moving send the donor's binding is moved-from; any later use is the sender's fault at the use site — E1001 statically, `trap(use-after-move)` here",
            ),
            Rule::ChanImm => (
                "conc.chan.imm",
                "`imm` data sends by reference — no move, no copy, sender access survives ([conc.mm.hb.freeze] orders the reads)",
            ),
            Rule::CancelPoint => (
                "conc.cancel.points",
                "cancellation is cooperative, delivered at runtime-owned blocking points from the closed set",
            ),
            Rule::CancelDefer => (
                "conc.cancel.defer",
                "a cancelled task runs its own defer/errdefer as its frames return — cancellation is polite; kill is structural",
            ),
            Rule::CancelFfi => (
                "conc.cancel.c",
                "C frames are never unwound or interrupted; a task in a C call cancels at its next safe point after return",
            ),
            Rule::ProcModel => (
                "conc.proc.1",
                "a proc is a failure domain owning its regions; nothing may assume shared address-space visibility beyond its channels",
            ),
            Rule::ProcLink => (
                "conc.proc.2",
                "`link` couples fates symmetrically: either side's abnormal exit kills the other",
            ),
            Rule::ProcMonitor => (
                "conc.proc.2",
                "`monitor` delivers the exit reason asynchronously to the monitor's channel as a typed value",
            ),
            Rule::ProcExit => (
                "conc.proc.exit",
                "exit reasons are a closed set — normal(value), error(value), killed, cancelled — and are values, never unwinding",
            ),
            Rule::ProcKill => (
                "conc.proc.kill",
                "killed-proc sequence: task tree cancelled without running user code (defers do NOT run), regions bulk-free, reasons deliver",
            ),
            Rule::ProcMailbox => (
                "conc.chan.mailbox",
                "procs communicate exclusively via typed channels + select; no selective receive; handlers are atomic and non-blocking",
            ),
            Rule::WhenOrder => (
                "conc.when.order",
                "`when (a, b, …)` acquires the entire operand set one object at a time in the canonical order, regardless of the order written at the site",
            ),
            Rule::WhenNoDeadlock => (
                "conc.when.nodeadlock",
                "no lock-order deadlock, by construction: every `when` acquires its whole set in the one canonical order, so no cycle of `when` acquisitions can form",
            ),
            Rule::WhenBody => (
                "conc.when.body",
                "the body runs with exclusive access to every operand's payload; simple paths rebind to payloads and write back at release, in reverse canonical order",
            ),
            Rule::WhenNoNest => (
                "conc.when.nonest",
                "a lexically nested `when` is the compiler's E1103; dynamically reaching an acquisition of a sync object the task already holds is `trap(deadlock)` ([conc.deadlock.self])",
            ),
            Rule::DeadlockDef => (
                "conc.deadlock.def",
                "every live task blocked at a blocking point with no pending timer and no in-flight I/O is a deadlock — a defined outcome, detected exactly by a deterministic scheduler",
            ),
            Rule::DeadlockTrap => (
                "conc.deadlock.trap",
                "a detected deadlock terminates with trap kind `deadlock`, reporting the blocked-task roster; detection is required in deterministic test modes",
            ),
            Rule::DeadlockSelf => (
                "conc.deadlock.self",
                "acquiring a sync object the acquiring task already holds can never complete: detected immediately, `trap(deadlock)`",
            ),
            Rule::SelectClosed => (
                "conc.select.closed",
                "a drained-closed channel makes its receive arm ready: the arm runs and receives the closed error value — never a block-forever, never a fault",
            ),
            Rule::TaskFailOwner => (
                "conc.task.fail.owner",
                "a failing child's cancellation reaches the scope owner too: an owner blocked at a blocking point entered inside the scope's extent is cancelled exactly like a sibling",
            ),
            Rule::ProcCancel => (
                "conc.proc.cancel",
                "`w.cancel()` delivers structured cancellation to a proc: cooperative at blocking points, defers run; exit reason `cancelled` unless the value completes anyway",
            ),
            Rule::ProcLinkPair => (
                "conc.proc.link.pair",
                "`a.link(b)` couples two procs symmetrically, idempotent per pair; `w.link()` is `w.link(<the calling task's proc>)`",
            ),
            Rule::ProcRoot => (
                "conc.proc.root",
                "the root supervisor's domain is the process: its abnormal death runs the killed-proc sequence for every live proc and terminates nonzero — compare the outcome class, never the number ([conf.trap.exit])",
            ),
            Rule::RaceDetect => (
                "conc.mm.race.3",
                "an implementation may detect a data race and halt with trap kind `race`; the sim scheduler detects exactly at realized interleavings",
            ),
            Rule::DetMode => (
                "conc.det.modes",
                "record, replay, free: the deterministic runtime's three modes; test builds keep every event point",
            ),
        };
        Row {
            rule: self,
            anchor,
            description,
        }
    }

    /// This rule's clause anchor.
    #[must_use]
    pub const fn anchor(self) -> &'static str {
        self.row().anchor
    }

    /// This rule's one-sentence description.
    #[must_use]
    pub const fn description(self) -> &'static str {
        self.row().description
    }

    /// Is this a memory-model rule — one `--trace=mem` keeps?
    ///
    /// The filter is the anchor's namespace, not a hand-kept list: a rule
    /// citing `mem.*` is a memory rule by definition, so the filter cannot
    /// drift away from the registry.
    #[must_use]
    pub fn is_memory(self) -> bool {
        self.anchor().starts_with("mem.")
    }

    /// Is this a Tier-3 rule — one `--trace=prov` keeps?
    ///
    /// The filter is derived from the anchor exactly as [`Rule::is_memory`] is,
    /// but over the three clause families that *are* the unsafe tier: §5
    /// (`mem.unsafe`), §6 (`mem.prov`) and §7 (`mem.ub`). A rule joining one of
    /// those namespaces is a provenance rule by definition, so the filter cannot
    /// drift away from the registry.
    #[must_use]
    pub fn is_provenance(self) -> bool {
        let anchor = self.anchor();
        anchor.starts_with("mem.prov")
            || anchor.starts_with("mem.unsafe")
            || anchor.starts_with("mem.ub")
            || anchor.starts_with("mem.boundary")
    }

    /// Every rule, in declaration order. The registry.
    pub const ALL: [Rule; 115] = [
        Rule::ValueSemantics,
        Rule::PlacePath,
        Rule::PathDisjoint,
        Rule::Alloc,
        Rule::Move,
        Rule::UseAfterMove,
        Rule::Copy,
        Rule::Reinit,
        Rule::ModeRead,
        Rule::ModeMut,
        Rule::ModeTake,
        Rule::Exclusivity,
        Rule::ExclusivityDisjoint,
        Rule::ViewSet,
        Rule::Borrow,
        Rule::BorrowExtent,
        Rule::ArithChecked,
        Rule::ArithWrapping,
        Rule::DivZero,
        Rule::Bounds,
        Rule::LiteralDefault,
        Rule::CharCast,
        Rule::RegionCreate,
        Rule::RegionAffine,
        Rule::RegionAmbient,
        Rule::RegionIdentity,
        Rule::RegionIntra,
        Rule::RegionFree,
        Rule::RegionEdge,
        Rule::RegionEdgeIso,
        Rule::RegionEdgeImm,
        Rule::RegionOpen,
        Rule::RegionMultiopen,
        Rule::RegionSuspended,
        Rule::RegionFreeze,
        Rule::RegionTransfer,
        Rule::RegionClosedSubtree,
        Rule::SharedRc,
        Rule::SharedAcyclic,
        Rule::SharedWeak,
        Rule::SharedDrop,
        Rule::HandleTwoPhase,
        Rule::HandleStale,
        Rule::HandleAccess,
        Rule::UnsafeRaw,
        Rule::AssumeNoalias,
        Rule::UnsafeDoor,
        Rule::UnsafeScope,
        Rule::BoundaryFfi,
        Rule::ProvTag,
        Rule::ProvState,
        Rule::ProvExpose,
        Rule::ProvRegion,
        Rule::Ub,
        Rule::UbLicensed,
        Rule::UbVerdict,
        Rule::Block,
        Rule::Flow,
        Rule::EvalOrder,
        Rule::EvalStrictOrder,
        Rule::Assign,
        Rule::Call,
        Rule::Closure,
        Rule::ErrUnion,
        Rule::ErrRows,
        Rule::ErrPropagate,
        Rule::ErrElse,
        Rule::ErrDefer,
        Rule::DeferLifo,
        Rule::StrInterp,
        Rule::Assert,
        Rule::TrapVocabulary,
        Rule::SchedSeed,
        Rule::SchedSpawn,
        Rule::SchedPark,
        Rule::SchedUnpark,
        Rule::SchedDecision,
        Rule::SchedChan,
        Rule::SchedSelect,
        Rule::SchedAcquire,
        Rule::SchedTimer,
        Rule::TaskScope,
        Rule::TaskJoin,
        Rule::TaskFail,
        Rule::TaskName,
        Rule::TaskRoot,
        Rule::ChanType,
        Rule::ChanBuf,
        Rule::ChanClose,
        Rule::ChanMove,
        Rule::ChanStale,
        Rule::ChanImm,
        Rule::CancelPoint,
        Rule::CancelDefer,
        Rule::CancelFfi,
        Rule::ProcModel,
        Rule::ProcLink,
        Rule::ProcMonitor,
        Rule::ProcExit,
        Rule::ProcKill,
        Rule::ProcMailbox,
        Rule::WhenOrder,
        Rule::WhenNoDeadlock,
        Rule::WhenBody,
        Rule::WhenNoNest,
        Rule::DeadlockDef,
        Rule::DeadlockTrap,
        Rule::DeadlockSelf,
        Rule::SelectClosed,
        Rule::TaskFailOwner,
        Rule::ProcCancel,
        Rule::ProcLinkPair,
        Rule::ProcRoot,
        Rule::RaceDetect,
        Rule::DetMode,
    ];
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} [{}]", self, self.anchor())
    }
}

/// The registry as a table.
#[must_use]
pub fn registry() -> Vec<Row> {
    Rule::ALL.iter().map(|r| r.row()).collect()
}

/// Why a registry row is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowError {
    /// The row cites no clause at all — the failure the sprint asks CI to prove.
    NoAnchor(String),
    /// The row cites something that is not a well-formed anchor, or names a
    /// namespace outside `[conf.anchor.ns]`.
    BadAnchor { rule: String, reason: String },
    /// The row cites an anchor in a *registered* namespace that the pinned
    /// `spec/anchors.json` does not contain.
    UnknownAnchor { rule: String, anchor: String },
    /// The row says nothing about what the rule does.
    NoDescription(String),
}

impl fmt::Display for RowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RowError::NoAnchor(rule) => write!(f, "rule `{rule}` cites no clause anchor"),
            RowError::BadAnchor { rule, reason } => write!(f, "rule `{rule}`: {reason}"),
            RowError::UnknownAnchor { rule, anchor } => write!(
                f,
                "rule `{rule}` cites `{anchor}`, which the pinned spec does not define"
            ),
            RowError::NoDescription(rule) => write!(f, "rule `{rule}` has no description"),
        }
    }
}

impl std::error::Error for RowError {}

/// Checks a set of registry rows against the pinned anchor index.
///
/// `known` is `spec/anchors.json`'s anchor set, or `None` when it could not be
/// read (in which case anchors are checked for *shape* only, as the corpus walk
/// does). Every error is reported, not just the first: a registry with three
/// bad rows should say so once.
///
/// # Errors
///
/// Every [`RowError`] found, in row order.
pub fn validate(rows: &[Row], known: Option<&std::collections::BTreeSet<String>>) -> Vec<RowError> {
    let mut errors = Vec::new();
    for row in rows {
        let rule = format!("{:?}", row.rule);
        if row.anchor.is_empty() {
            errors.push(RowError::NoAnchor(rule.clone()));
        } else {
            match crate::anchor::classify(row.anchor) {
                Err(e) => errors.push(RowError::BadAnchor {
                    rule: rule.clone(),
                    reason: e.to_string(),
                }),
                Ok(crate::anchor::Namespace::Registered) => {
                    if let Some(known) = known
                        && !known.contains(row.anchor)
                    {
                        errors.push(RowError::UnknownAnchor {
                            rule: rule.clone(),
                            anchor: row.anchor.to_owned(),
                        });
                    }
                }
                // Forward namespaces are legal and counted as forward
                // (`[conf.anchor.ns]`); their documents are not written yet.
                Ok(crate::anchor::Namespace::Reserved) => {}
            }
        }
        if row.description.trim().is_empty() {
            errors.push(RowError::NoDescription(rule));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_appears_in_all_exactly_once() {
        let mut seen = std::collections::BTreeSet::new();
        for rule in Rule::ALL {
            assert!(seen.insert(rule), "{rule:?} listed twice in Rule::ALL");
        }
        assert_eq!(seen.len(), Rule::ALL.len());
    }

    #[test]
    fn every_rule_cites_a_clause_and_says_what_it_does() {
        // Shape-only pass; the pinned-index pass lives in `tests/rule_registry.rs`,
        // which can read `spec/anchors.json`.
        assert_eq!(validate(&registry(), None), Vec::new());
    }

    #[test]
    fn a_planted_anchorless_rule_fails_validation() {
        // The sprint's acceptance criterion, made executable: "a planted
        // anchorless rule demonstrably fails CI".
        let planted = [Row {
            rule: Rule::Move,
            anchor: "",
            description: "moves things, trust me",
        }];
        assert_eq!(
            validate(&planted, None),
            vec![RowError::NoAnchor("Move".to_owned())]
        );
    }

    #[test]
    fn a_planted_undescribed_rule_fails_validation() {
        let planted = [Row {
            rule: Rule::Move,
            anchor: "mem.tier0.move.1",
            description: "   ",
        }];
        assert_eq!(
            validate(&planted, None),
            vec![RowError::NoDescription("Move".to_owned())]
        );
    }

    #[test]
    fn a_planted_rule_citing_an_unpinned_anchor_fails_validation() {
        let known = std::collections::BTreeSet::from(["mem.tier0.move.1".to_owned()]);
        let planted = [Row {
            rule: Rule::UseAfterMove,
            anchor: "mem.tier0.move.99",
            description: "an anchor nobody published",
        }];
        assert_eq!(
            validate(&planted, Some(&known)),
            vec![RowError::UnknownAnchor {
                rule: "UseAfterMove".to_owned(),
                anchor: "mem.tier0.move.99".to_owned(),
            }]
        );
    }

    #[test]
    fn a_planted_rule_in_an_unregistered_namespace_fails_validation() {
        let planted = [Row {
            rule: Rule::Flow,
            anchor: "wolfy.made.up",
            description: "a namespace nobody registered",
        }];
        assert!(matches!(
            validate(&planted, None).as_slice(),
            [RowError::BadAnchor { .. }]
        ));
    }

    #[test]
    fn rules_render_with_their_anchor() {
        assert_eq!(
            Rule::UseAfterMove.to_string(),
            "UseAfterMove [mem.tier0.move.2]"
        );
    }
}

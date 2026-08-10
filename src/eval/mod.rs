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
pub mod place;
pub mod prov;
pub mod region;
pub mod rules;
pub mod value;

use std::collections::BTreeMap;

use crate::ast::{
    Arg, AssignOp, BinOp, Binding, Block, ClosureParam, ElseHandler, Expr, ExprKind, FnDecl,
    IndexArg, Interpolation, Member, ParamKind, ParamMode, PatKind, Pattern, RegionStrategy, Stmt,
    StmtKind, StrPart, Type, TypeArg, TypeKind, UnOp,
};
use crate::diag::Span;
use crate::sema::{Def, Program};
use crate::trap::TrapKind;

use place::{Access, AccessSet, Held, Path, Proj};
use prov::{AccessKind, Prov, Provenance, RawPtr, RetagKind, UbFinding, UbRow};
use region::{Edge, Ref, RegionId, RegionState, Store, Strategy};
use rules::Rule;
use value::{
    ArithMode, ClosureValue, ErrorValue, HandleValue, IntTy, RegionValue, Slot, SlotState, Value,
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

/// Non-local control, carried up a `Result`'s error arm. Not unwinding.
#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Return(Value),
    Break(Value),
    Continue,
    Trap(Box<Trap>),
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
    scopes: Vec<Scope>,
}

/// The abstract machine.
pub struct Machine<'p> {
    program: &'p Program,
    frames: Vec<Frame>,
    access: AccessSet,
    globals: BTreeMap<String, Slot>,
    /// `[mem.model.machine]` components 3 and 4: the region forest and the
    /// scope stack, plus the Tier-2 pools and cells.
    store: Store,
    /// `[mem.model.machine]` component 2: the provenance forest (is04).
    prov: Provenance,
    /// How many `unsafe { }` blocks are open. `[mem.ub]`'s enumeration is
    /// Tier-3-reachable only, so the rows that need "in unsafe code" in their
    /// wording (T1) ask this.
    unsafe_depth: u32,
    /// Retags produced by the current call's arguments, waiting for
    /// [`Machine::call_fn`] to bind them to the callee's parameter places.
    pending_retags: Vec<Option<PendingRetag>>,
    stdout: Vec<u8>,
    trace: Vec<String>,
    tracing: Trace,
    /// Set when the forest invariant assertion ever failed. A broken invariant
    /// is an interpreter bug, so it is recorded rather than swallowed.
    forest: Result<(), String>,
    /// Steps taken, against [`Machine::FUEL`].
    steps: u64,
}

impl<'p> Machine<'p> {
    /// Evaluation steps a program may take before the machine gives up.
    ///
    /// A non-terminating program has no verdict — `[proto.record.verdict]`
    /// offers none — so the machine declines rather than hangs a CI job. It is
    /// generous enough that nothing in the pinned corpus comes close.
    pub const FUEL: u64 = 50_000_000;

    #[must_use]
    pub fn new(program: &'p Program) -> Machine<'p> {
        Machine {
            program,
            frames: Vec::new(),
            access: AccessSet::new(),
            globals: BTreeMap::new(),
            store: Store::new(),
            prov: Provenance::new(),
            unsafe_depth: 0,
            pending_retags: Vec::new(),
            stdout: Vec::new(),
            trace: Vec::new(),
            tracing: Trace::Off,
            forest: Ok(()),
            steps: 0,
        }
    }

    #[must_use]
    pub fn tracing(mut self, trace: Trace) -> Machine<'p> {
        self.tracing = trace;
        self
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
    const RUN_STACK: usize = 64 * 1024 * 1024;

    /// Runs the program's `main`, on a stack this machine chose.
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
        // The program region is freed last, by the machine: `main`'s caller is
        // the runtime, and `[mem.region.intra.2]` frees at the owner's death.
        let freed = self.store.free(Store::root());
        // `[mem.prov.region]`: freeing a region Disables every tag tree it owns.
        self.prov.region_freed(&freed, Span::new(0, 0));
        let leaks = self.store.leaks();
        let live_cells = self.store.live_cells();
        let host_leaks = self.prov.live_host_allocs();
        let forest = match (&self.forest, self.store.assert_forest()) {
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
                self.store.regions().len(),
                leaks.len(),
                live_cells.len()
            ),
        );
        self.drain_prov();
        Run {
            outcome,
            stdout: self.stdout,
            trace: self.trace,
            leaks,
            live_cells,
            host_leaks,
            forest,
        }
    }

    fn run_main(&mut self) -> Outcome {
        self.frames.push(Frame {
            module: String::new(),
            scopes: vec![Scope::default()],
        });

        // Item-level `let`/`var`/`const` evaluate once, in declaration order.
        let bindings = self.program.root().bindings.clone();
        for name in bindings {
            let Some(Def::Binding(binding)) = self.program.lookup("", &name, false).cloned() else {
                continue;
            };
            match self.eval(&binding.value) {
                Ok(value) => {
                    self.globals.insert(name, Slot::live(value));
                }
                Err(signal) => return self.finish(Err(signal)),
            }
        }

        let Some(Def::Fn(main)) = self.program.lookup("", "main", false).cloned() else {
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
            vec![Value::List(Vec::new())]
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
        }
    }

    // -- bookkeeping -------------------------------------------------------

    fn fire(&mut self, rule: Rule, span: Span, detail: &str) {
        if self.tracing.keeps(rule) {
            self.trace.push(format!(
                "{:>6}..{:<6} {} {}",
                span.start, span.end, rule, detail
            ));
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
        if self.forest.is_err() {
            return;
        }
        if let Err(broken) = self.store.assert_forest() {
            self.fire(
                Rule::RegionEdgeIso,
                span,
                &format!("FOREST BROKEN: {broken}"),
            );
            self.forest = Err(broken);
        }
    }

    /// Moves the provenance machine's rule firings into `--trace`.
    ///
    /// Called after every operation that can produce them, so a `prov` trace
    /// interleaves with the `mem` one in execution order rather than arriving
    /// as a block at the end.
    fn drain_prov(&mut self) {
        for (rule, span, detail) in self.prov.take_notes() {
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
        let (tag_span, tree) = match alloc {
            Some(alloc) => (
                self.prov.alloc(alloc).map(|entry| entry.span),
                self.prov.tree(alloc),
            ),
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
        match self.prov.access(ptr, len, kind, span) {
            Ok(()) => {
                self.drain_prov();
                Ok(())
            }
            Err(finding) => self.ub(finding),
        }
    }

    /// The provenance key of a place: the frame keeps two activations of one
    /// local apart, exactly as [`Path`] does.
    fn place_key(path: &Path) -> String {
        format!("{}:{path}", path.frame)
    }

    fn step(&mut self) -> EResult<()> {
        self.steps += 1;
        if self.steps > Machine::FUEL {
            return unsupported(format!(
                "the program did not terminate within {} evaluation steps",
                Machine::FUEL
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
        self.reclaim(&scope);
        self.prov.prune();
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
    fn reclaim(&mut self, scope: &Scope) {
        for (name, slot) in scope.locals.iter().rev() {
            if !slot.is_live() {
                // Moved out: its new owner is responsible for it.
                continue;
            }
            let mut refs = Vec::new();
            region::references(&slot.value, &mut refs);
            for granule in refs {
                match granule {
                    Ref::Region(id) => {
                        let freed = self.store.free(id);
                        self.prov.region_freed(&freed, Span::new(0, 0));
                        if !freed.is_empty() {
                            let detail = format!(
                                "`{name}` dies: {} freed wholesale",
                                freed
                                    .iter()
                                    .map(|id| self.store.label(*id))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                            self.fire(Rule::RegionFree, Span::new(0, 0), &detail);
                        }
                    }
                    Ref::Shared(cell) => {
                        if self.store.release(cell) {
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
                        self.store.drop_weak(cell);
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
                if let Some(index) = scope.locals.iter().position(|(n, _)| *n == path.base) {
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
                (Value::Tuple(items) | Value::List(items), Proj::Index(i)) => {
                    let index = usize::try_from(*i).ok()?;
                    items.get_mut(index)?
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
            let (held_path, held_access, held_span) =
                (held.path.to_string(), held.access, held.span);
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
        self.check_access(path, Access::Shared, span)?;
        let display = path.to_string();
        match self.resolve(path) {
            None => unsupported(format!("`{display}` does not denote a place at run time")),
            Some((slot, None)) => {
                let value = slot.value.clone();
                self.fire(Rule::PlacePath, span, &format!("read `{display}`"));
                self.access_place(path, AccessKind::Read, span)?;
                Ok(value)
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

    /// Moves out of a path: the source place becomes uninitialized
    /// (`[mem.tier0.move.1]`).
    fn move_path(&mut self, path: &Path, span: Span) -> EResult<Value> {
        self.check_access(path, Access::Exclusive, span)?;
        let display = path.to_string();
        let outcome = match self.resolve(path) {
            None => None,
            Some((slot, None)) => Some(Ok(slot.take_value(span))),
            Some((_, Some((at, depth)))) => Some(Err((at, depth))),
        };
        match outcome {
            None => unsupported(format!("`{display}` does not denote a place at run time")),
            Some(Ok(value)) => {
                self.fire(Rule::Move, span, &format!("move out of `{display}`"));
                Ok(value)
            }
            Some(Err((moved_at, depth))) => {
                let moved_path = prefix(path, depth);
                self.trap(
                    TrapKind::UseAfterMove,
                    Rule::UseAfterMove,
                    span,
                    format!("`{display}` was already moved out"),
                    Some((moved_at, format!("`{moved_path}` moved here"))),
                )
            }
        }
    }

    /// Writes through a path, making the place live again if it was moved from
    /// (`[mem.tier0.move.4]`).
    fn write_path(&mut self, path: &Path, value: Value, span: Span) -> EResult<()> {
        self.check_access(path, Access::Exclusive, span)?;
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
        self.access_place(path, AccessKind::Write, span)
    }

    /// One Tier-0 place access, against the provenance forest.
    ///
    /// **Lazy by design.** A place that no borrow ever named has no tag tree,
    /// and this returns immediately — so the cost of the machine is bounded by
    /// the borrows a program actually creates rather than by the reads it
    /// performs. The first retag of a place is what mints its root.
    fn access_place(&mut self, path: &Path, kind: AccessKind, span: Span) -> EResult<()> {
        let key = Machine::place_key(path);
        match self.prov.access_place(&key, kind, span) {
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
        self.stdout.extend_from_slice(text.as_bytes());
    }

    // -- Tier 1: the region machine (§3) -----------------------------------

    /// Every §3 fault surfaces through one trap kind.
    ///
    /// `[conf.trap.set]` is closed at eleven kinds and `[conf.trap.map]` maps
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
        let id = self.store.charge();
        let label = self.store.label(id);
        self.fire(
            Rule::RegionAmbient,
            span,
            &format!("{what} allocated in {label}"),
        );
        id
    }

    /// The current (ambient) region.
    pub(crate) fn current_region(&self) -> RegionId {
        self.store.current()
    }

    /// Opens a region for a block (`[mem.region.open.1]`, `[mem.region.multiopen]`).
    fn enter_region(&mut self, id: RegionId, span: Span) -> EResult<()> {
        match self.store.enter(id) {
            Ok(()) => {
                let label = self.store.label(id);
                let open = self.store.open_set().len();
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
        self.store.leave(id);
        let label = self.store.label(id);
        let state = self
            .store
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
            match self.store.classify_edge(into, granule) {
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
                        self.store.adopt(parent, child);
                        self.assert_forest(span);
                    }
                    let label = self.store.label(child);
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
        let id = self.store.create(binder.clone(), strategy.clone(), span);
        let label = self.store.label(id);
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
            let generation = self.store.generation(id);
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
                let freed = self.store.free(id);
                // `[mem.prov.region]`: the wholesale free Disables every tag
                // tree of every allocation the region owned.
                self.prov.region_freed(&freed, span);
                if !freed.is_empty() {
                    let labels = freed
                        .iter()
                        .map(|id| self.store.label(*id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.fire(
                        Rule::RegionFree,
                        span,
                        &format!("`}}` frees {labels} wholesale"),
                    );
                }
            }
            SugarExit::Freeze => match self.store.freeze(id) {
                Ok(frozen) => {
                    // `[mem.prov.region]`: `freeze` transitions all its tags to
                    // Frozen, so a later write through any of them is §7/P2.
                    self.prov.region_frozen(&frozen, span);
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
        match self.store.freeze(id) {
            Ok(frozen) => {
                self.prov.region_frozen(&frozen, span);
                let label = self.store.label(id);
                self.fire(
                    Rule::RegionFreeze,
                    span,
                    &format!(
                        "{label} and {} owned region(s) are `imm` forever",
                        frozen.len().saturating_sub(1)
                    ),
                );
                let generation = self.store.generation(id);
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
        let live = self.store.generation(handle.id) == handle.generation
            && self.store.state(handle.id) != Some(RegionState::Freed);
        if live {
            return Ok(handle.id);
        }
        let label = self.store.label(handle.id);
        self.region_fault(
            Rule::RegionFree,
            span,
            format!(
                "{what} names {label}, which was freed wholesale; every allocation in it died \
                 with it, and this reference is dangling"
            ),
            self.store
                .region(handle.id)
                .map(|region| (region.span, "the region was created here".to_owned())),
        )
    }

    // -- Tier 2: `shared`, `weak`, pools and handles (§4) ------------------

    /// The store, for `builtin`'s Tier-2 surface.
    pub(crate) fn store(&mut self) -> &mut Store {
        &mut self.store
    }

    /// Reads a pool slot through a handle, with the generation check every
    /// deref performs (`[mem.shared.handle.2]`).
    pub(crate) fn read_slot(&mut self, handle: HandleValue, span: Span) -> EResult<Value> {
        self.check_pool_region(handle.pool, span, false)?;
        match self
            .store
            .slot(handle.pool, handle.index, handle.generation)
        {
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
            .store
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
        let Some(id) = self.store.pool_region(pool) else {
            return unsupported(format!("pool#{pool} does not exist"));
        };
        let label = self.store.label(id);
        match self.store.state(id) {
            Some(RegionState::Freed) => {
                // Detection is exact and it says so: the region that died, and
                // where it was created (`[mem.region.intra.2]`).
                let created = self
                    .store
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
            if self.store.would_cycle(from, to) {
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
            self.store.add_strong_edge(from, to);
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
        self.frames.push(Frame {
            module: module.to_owned(),
            scopes: vec![Scope::default()],
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
                let key = format!("{frame}:{name}");
                self.prov.bind_place(&key, retag.alloc, retag.tag);
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
        self.prov.drop_frame(frame);

        let value = match result {
            Ok(value) | Err(Signal::Return(value)) => value,
            Err(other) => return Err(other),
        };
        Ok(Applied { value, params })
    }

    /// Evaluates the arguments of a call, applying call-site modes (X1).
    ///
    /// Exclusivity is checked as the arguments accumulate, which is what makes
    /// `f(mut a.x, mut a.y)` legal and `f(mut a, mut a.x)` a trap.
    fn eval_args(&mut self, args: &[Arg]) -> EResult<Args> {
        self.eval_args_for(args, Callee::Wolf)
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
    fn eval_args_for(&mut self, args: &[Arg], callee: Callee) -> EResult<Args> {
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
            self.prov.expose(ptr, span);
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
                .prov
                .retag(alloc, parent, kind, true, "parameter", span);
            protectors.push(child);
            self.drain_prov();
            return Value::Raw(RawPtr {
                prov: Prov::Tag(child),
                ..ptr
            });
        }
        let key = Machine::place_key(path);
        let (alloc, child) = self.prov.retag_place(&key, kind, true, span);
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
            self.prov.unprotect(*tag, span);
        }
        self.prov.prune();
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
        // `errdefer` runs when the scope is left on the error path, and only
        // then (`[err.errdefer]`); `defer` runs on both.
        let errored = match &result {
            Ok(value) => value.is_error(),
            Err(Signal::Return(value)) => value.is_error(),
            Err(Signal::Trap(_) | Signal::Ub(_) | Signal::Unsupported(_)) => true,
            Err(Signal::Break(_) | Signal::Continue) => false,
        };
        let defers = self.run_defers(errored);
        self.pop_scope();
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
            StmtKind::Item(_) => Ok(()),
        }
    }

    fn exec_binding(&mut self, binding: &Binding) -> EResult<()> {
        // Initialization moves (`[mem.tier0.move.1]`) unless the source is a
        // `Copy`-shaped value or is not a place at all.
        let value = self.eval_for_init(&binding.value)?;
        let value = coerce(value, binding.ty.as_ref());
        self.bind_pattern(&binding.pattern, value)
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
                (Some(Value::Int(_, ty)), Value::Int(v, lit)) if lit.literal => Value::Int(v, ty),
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
                let name = path
                    .segments
                    .last()
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
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
                let value = self.eval(inner)?;
                if value.is_error() {
                    // `?` returns the error to the caller, widening the row by
                    // union (`[err.propagate]`). A return, not an unwind.
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
                    Some(value) => self.eval_for_init(value)?,
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
                let id = self.store.create(None, strategy.clone(), expr.span);
                let label = self.store.label(id);
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
                let generation = self.store.generation(id);
                Ok(Value::Region(RegionValue { id, generation }))
            }
            ExprKind::In { region, body } => self.eval_in(region, body, expr.span),
            ExprKind::Freeze(operand) => self.eval_freeze(operand, expr.span),

            // -- outside is03's coverage, by name ---------------------------
            ExprKind::Scope { .. }
            | ExprKind::SpawnProc { .. }
            | ExprKind::Select { .. }
            | ExprKind::When { .. } => {
                unsupported("concurrency is spec/03 and campaign ic03; nothing here schedules")
            }
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
        let rendered = value.to_string();
        let Some(parts) = &interp.format else {
            return Ok(rendered);
        };
        let mut spec = String::new();
        for part in parts {
            match part {
                crate::ast::FmtPart::Text(text) => spec.push_str(text),
                crate::ast::FmtPart::Interp(expr) => {
                    let value = self.eval(expr)?;
                    spec.push_str(&value.to_string());
                }
            }
        }
        match value::apply_format(&rendered, &spec) {
            Ok(text) => Ok(text),
            Err(reason) => unsupported(reason),
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
            if let Some(def) = self.program.lookup(&module, name, false) {
                return self.value_of_def(def, &module, name, expr.span);
            }
            if self.program.modules.contains_key(name) {
                return Ok(Value::Module(name.clone()));
            }
            if let Some(builtin) = builtin::ambient(name) {
                return Ok(builtin);
            }
            // An unresolved capitalized name is a structural error tag: D30's
            // rows need no declaration (`[err.rows]`).
            if name.starts_with(char::is_uppercase) {
                self.fire(Rule::ErrRows, expr.span, &format!("error tag `{name}`"));
                return Ok(Value::Error(Box::new(ErrorValue {
                    tag: name.clone(),
                    payload: Vec::new(),
                })));
            }
            return unsupported(format!("`{name}` does not resolve"));
        }

        // A dotted path: module member, or a dotted error tag (`io.Error`).
        let head = path.segments[0].name.clone();
        let tail = path.segments[1].name.clone();
        if self.program.modules.contains_key(&head) {
            return match self.program.lookup(&head, &tail, true) {
                Some(def) => {
                    let def = def.clone();
                    self.value_of_def(&def, &head, &tail, expr.span)
                }
                None => unsupported(format!(
                    "`{head}.{tail}` does not resolve; it is either absent or not `pub` \
                     (`[mod.vis.private]` is the compiler's E0304)"
                )),
            };
        }
        if head == "c" && !self.local_exists("c") {
            let module = self
                .frames
                .last()
                .map(|f| f.module.clone())
                .unwrap_or_default();
            if self.program.imports_c(&module) {
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
            let tag = format!("{head}.{tail}");
            self.fire(Rule::ErrRows, expr.span, &format!("error tag `{tag}`"));
            return Ok(Value::Error(Box::new(ErrorValue {
                tag,
                payload: Vec::new(),
            })));
        }
        unsupported(format!("`{head}.{tail}` does not resolve"))
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
                    if let Some(region) = self.store.region(id)
                        && region.depth > 0
                    {
                        let label = self.store.label(id);
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
                    let label = self.store.label(id);
                    self.fire(Rule::RegionTransfer, span, &format!("transfer {label}"));
                }
                Ok(value)
            }
            UnOp::Not => match self.eval(operand)? {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                other => unsupported(format!("`!` needs a bool, got {}", other.kind())),
            },
            UnOp::Neg => match self.eval(operand)? {
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
                self.access.push(Held { path, access, span });
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
                let cell = self.store.new_cell(value.clone(), span);
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
            Eq => return Ok(Value::Bool(value_eq(&left, &right))),
            Ne => return Ok(Value::Bool(!value_eq(&left, &right))),
            _ => {}
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
        // (`[arith.literal.default]`): an unconstrained literal adopts the other
        // operand's type; two literals default to i32.
        let ty = match (ta.literal, tb.literal) {
            (true, false) => *tb,
            (false, true) | (false, false) => *ta,
            (true, true) => IntTy::I32,
        };
        if ta.literal && tb.literal {
            self.fire(Rule::LiteralDefault, span, "literals default to i32");
        }

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

        let computed = match op {
            Add => a.checked_add(b),
            Sub => a.checked_sub(b),
            Mul => a.checked_mul(b),
            Div => a.checked_div(b),
            Rem => a.checked_rem(b),
            BitAnd => Some(a & b),
            BitOr => Some(a | b),
            BitXor => Some(a ^ b),
            Shl => u32::try_from(b).ok().and_then(|s| a.checked_shl(s)),
            Shr => u32::try_from(b).ok().and_then(|s| a.checked_shr(s)),
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
            Value::List(slots) => slots.into_iter().map(|s| s.value).collect(),
            Value::Map(pairs) => pairs
                .into_iter()
                .map(|(key, slot)| Value::Tuple(vec![Slot::live(key), slot]))
                .collect(),
            other => {
                return unsupported(format!("`for` cannot iterate {}", other.kind()));
            }
        };

        self.fire(Rule::Flow, span, "for");
        for item in items {
            self.step()?;
            self.push_scope();
            let bound = self.bind_pattern(pattern, item);
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
        self.fire(Rule::Closure, span, "closure captures by value");
        Ok(Value::Closure(Box::new(ClosureValue {
            params: params.iter().map(|p| p.name.name.clone()).collect(),
            body: body.clone(),
            captures,
        })))
    }

    fn match_pattern(&mut self, pattern: &Pattern, value: &Value) -> EResult<bool> {
        match &*pattern.kind {
            PatKind::Wildcard => Ok(true),
            PatKind::Binding(ident) => {
                self.declare(&ident.name, Slot::live(value.clone()));
                Ok(true)
            }
            PatKind::Literal(expr) => {
                let literal = self.eval(expr)?;
                Ok(match (&literal, value) {
                    (Value::Int(a, _), Value::Int(b, _)) => a == b,
                    _ => literal == *value,
                })
            }
            PatKind::Path(path) => {
                let tag = path
                    .segments
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                Ok(matches!(value, Value::Error(e) if e.tag == tag && e.payload.is_empty()))
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
                if err.tag != tag || err.payload.len() != fields.len() {
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

        let target = self.eval(callee)?;
        let evaluated = self.eval_args_for(args, Callee::of(&target))?;
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
        result.map(|applied| applied.value)
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
                !self.local_exists(name) && self.program.modules.contains_key(name)
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
                match self.program.lookup(&module, &name, false).cloned() {
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
                self.frames.push(Frame {
                    module: self
                        .frames
                        .last()
                        .map(|f| f.module.clone())
                        .unwrap_or_default(),
                    scopes: vec![Scope::default()],
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
                self.prov.drop_frame(frame);
                match result {
                    Ok(value) | Err(Signal::Return(value)) => Ok(plain(value)),
                    Err(other) => Err(other),
                }
            }
            Value::Error(tag) if tag.payload.is_empty() => {
                // `BadDigit(payload)` — a structural row entry applied to its
                // payload (`[err.rows]`).
                self.fire(
                    Rule::ErrRows,
                    span,
                    &format!("error `{}` with payload", tag.tag),
                );
                Ok(plain(Value::Error(Box::new(ErrorValue {
                    tag: tag.tag,
                    payload: args,
                }))))
            }
            Value::Builtin(name) => builtin::call(self, name, args, span).map(plain),
            other => unsupported(format!("{} is not callable", other.kind())),
        }
    }

    fn eval_method(
        &mut self,
        receiver: &Receiver,
        method: &str,
        mode: Option<ParamMode>,
        args: &[Arg],
        span: Span,
    ) -> EResult<Value> {
        let (path, value) = match receiver {
            Receiver::Place(path) if self.slot_mut(path).is_some() => {
                let value = self.read_path(path, span)?;
                (Some(path.clone()), value)
            }
            Receiver::Place(path) => {
                let display = path.to_string();
                return unsupported(format!("`{display}` does not denote a place at run time"));
            }
            Receiver::Expr(expr) => (None, self.eval(expr)?),
        };
        // The receiver of a method call is a `mut` argument in all but
        // spelling, so it retags first — and the retag happens *before* the
        // arguments are evaluated, which is the whole of the two-phase shape:
        // `xs.push(xs.len)` reads `xs` through the parent while the receiver's
        // fresh tag is still Reserved. Stacked Borrows invalidates the receiver
        // there; a tree does not (`[mem.prov.state]`, "foreign read: Reserved
        // ok"), which is `corpus/memory/prov_two_phase.lu`'s entire point.
        let receiver_tag = match &path {
            Some(path) => {
                let key = Machine::place_key(path);
                let (alloc, child) = self.prov.retag_place(&key, RetagKind::Mutable, true, span);
                self.drain_prov();
                Some((key, alloc, child))
            }
            None => None,
        };
        let evaluated = self.eval_args(args)?;
        // Now the receiver's own accesses go through the child.
        let previous = receiver_tag
            .as_ref()
            .map(|(key, alloc, child)| self.prov.rebind_place(key, *alloc, *child));
        let mut receiver_value = value;
        let result = builtin::method(
            self,
            &mut receiver_value,
            method,
            evaluated.values.clone(),
            span,
        );
        self.finish_args(
            &evaluated.writebacks,
            &evaluated.values,
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
        let written = match (&path, &result) {
            (Some(path), Ok(_)) if mode == Some(ParamMode::Take) => {
                self.fire(Rule::ModeTake, span, &format!("`take {path}` (receiver)"));
                self.move_path(path, span).map(|_| ())
            }
            (Some(path), Ok(_)) => self.write_path(path, receiver_value, span),
            _ => Ok(()),
        };
        if let Some((key, _, child)) = receiver_tag {
            self.prov.unprotect(child, span);
            self.prov
                .restore_place(&key, previous.expect("set beside receiver_tag"));
            self.prov.prune();
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
            return match self.program.lookup(&module, &name.name, true).cloned() {
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

    fn eval_bracket(&mut self, base: &Expr, args: &[IndexArg], span: Span) -> EResult<Value> {
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
        // optional (`s[..8]`, `s[4..]`) — which no `Value` shape carries, so
        // the range is read off the syntax here rather than evaluated to one.
        if let ExprKind::Range {
            start,
            end,
            inclusive,
        } = &*arg.expr.kind
        {
            let start = match start {
                Some(expr) => Some(self.eval_index_endpoint(expr)?),
                None => None,
            };
            let end = match end {
                Some(expr) => Some(self.eval_index_endpoint(expr)?),
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

    fn eval_index_endpoint(&mut self, expr: &Expr) -> EResult<i128> {
        match self.eval(expr)? {
            Value::Int(v, _) => Ok(v),
            other => Err(Signal::Unsupported(format!(
                "a slice endpoint must be an integer, got {}",
                other.kind()
            ))),
        }
    }

    // -- Tier 3: raw pointers, casts and the door (§5–§7) ------------------

    /// The provenance machine, for `builtin`'s C-intrinsic surface.
    pub(crate) fn prov(&mut self) -> &mut Provenance {
        &mut self.prov
    }

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
        match self.prov.assume_noalias(&ptrs, span) {
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
        let label = self.store.label(id);
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
        let (live, owner) = match self.prov.alloc(alloc) {
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
                    owner.map_or_else(|| "no region".to_owned(), |owner| self.store.label(owner))
                ),
            );
        }
        // `[mem.prov.tag]`: `borrow r from ptr` is a retag point.
        let child = self.prov.retag(
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
                    let resolved = self.prov.resolve_address(address);
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
                self.prov.expose(ptr, span);
                self.drain_prov();
                let address = self.prov.address_of(ptr);
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
        Ok(coerce(value, Some(ty)))
    }

    /// `p[i]` / `*p` — a provenance-checked load.
    fn raw_load(&mut self, ptr: RawPtr, span: Span) -> EResult<Value> {
        self.prov_access(ptr, ptr.elem, AccessKind::Read, span)?;
        let raw = self.prov.load(ptr, ptr.elem);
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
        self.prov.store(ptr, ptr.elem, *v);
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
            self.prov.store(at, 1, byte & 0xff);
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
            let byte = self.prov.load(from, 1);
            self.prov.store(to, 1, byte);
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
        (Value::Tuple(a) | Value::List(a), Value::Tuple(b) | Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| value_eq(&x.value, &y.value))
        }
        (
            Value::Struct {
                name: an,
                fields: af,
            },
            Value::Struct {
                name: bn,
                fields: bf,
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
        TypeKind::Prefixed { ty, .. } | TypeKind::ErrorUnion(ty) => type_name(ty),
        _ => "_".to_owned(),
    }
}

/// Applies a declared type to a value: the checking context sema-lite provides.
fn coerce(value: Value, ty: Option<&Type>) -> Value {
    let Some(ty) = ty else { return value };
    match (&*ty.kind, value) {
        (TypeKind::Path { path, args }, Value::Int(v, current)) => {
            let name = path.segments.last().map(|s| s.name.as_str()).unwrap_or("");
            let mode = match name {
                "wrapping" => Some(ArithMode::Wrapping),
                "saturating" => Some(ArithMode::Saturating),
                _ => None,
            };
            if let Some(mode) = mode {
                let inner = args.iter().find_map(|arg| match arg {
                    TypeArg::Type(inner) => match &*inner.kind {
                        TypeKind::Path { path, .. } => {
                            IntTy::named(path.segments.last().map_or("", |s| s.name.as_str()))
                        }
                        _ => None,
                    },
                    TypeArg::Expr(_) => None,
                });
                let base = inner.unwrap_or(IntTy::INT);
                return Value::Int(v, base.with_mode(mode));
            }
            match IntTy::named(name) {
                Some(ty) => Value::Int(v, ty),
                None => Value::Int(v, current),
            }
        }
        (TypeKind::Path { path, .. }, Value::Float(v)) => {
            let _ = path;
            Value::Float(v)
        }
        (TypeKind::ErrorUnion(inner), value) => coerce(value, Some(inner)),
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

#[cfg(test)]
mod tests;

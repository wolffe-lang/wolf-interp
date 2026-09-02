//! spec/03's dynamic semantics, evaluated — the is06 half of the machine.
//!
//! [`super::sched`] owns *when* things happen (every blocking or choosing
//! operation is a scheduler decision); this module owns *what* they mean:
//! scopes and their joins, spawns, channels with region transfer, `select`,
//! `when`, procs and their supervision. Every rule cites its spec/03 clause.
//! The s20 S-batch (pin `843174f`) paid the clause debts is06 filed: `when`
//! has its section (`[conc.when.*]`), region transfer over channels has
//! `[conc.chan.move]`/`[conc.chan.staleuse]`/`[conc.chan.imm]`, deadlock is
//! a defined outcome with its own trap kind (`[conc.deadlock.*]`), and a
//! failing child's cancellation reaches a blocked scope owner
//! (`[conc.task.fail.owner]`) — see `docs/divergence-log.md` for the
//! resolved findings S-1..S-8.

use crate::ast::{Arg, Block, Expr, Ident, SelectArm, SelectArmKind};
use crate::diag::Span;
use crate::trap::TrapKind;

use super::place::Path;
use super::region::{RegionId, Strategy, TaskClaim};
use super::rules::Rule;
use super::sched::{ChanId, MutexId, ProcId, Resolved, ScopeId, TaskEnd};
use super::value::{ClosureValue, Slot, Value};
use super::{EResult, Frame, Machine, Scope, Signal, unsupported};

/// Stack for a spawned task's tree walk. Smaller than `main`'s 64MiB
/// reservation — a task deeper than this hits the 512-frame rail first.
const TASK_STACK: usize = 16 * 1024 * 1024;

/// The region values a message carries, with their frozen-ness — what
/// `[conc.chan.type]`'s "a region value (moved on send)" transfers and
/// `[conc.mm.hb.freeze]`'s `imm` data does not.
fn region_granules(value: &Value) -> Vec<RegionId> {
    let mut out = Vec::new();
    let mut refs = Vec::new();
    super::region::references(value, &mut refs);
    for granule in refs {
        if let super::region::Ref::Region(id) = granule
            && !out.contains(&id)
        {
            out.push(id);
        }
    }
    out
}

impl Machine {
    // -- the blocking bridge ------------------------------------------------

    /// Runs one scheduler operation that may park this task: the task's open
    /// stack is swapped out of the store first and back in afterwards, so the
    /// store always holds the *running* task's scope stack. The lock order
    /// (store, then scheduler) is this function's alone.
    pub(super) fn sched_block<T>(
        &mut self,
        f: impl FnOnce(&super::sched::Sched, super::sched::TaskId) -> T,
    ) -> T {
        {
            let mut stack = Vec::new();
            self.store().swap_open(&mut stack);
            self.shared.sched.save_stack(self.task, stack);
        }
        let sched = self.shared.sched.clone();
        let out = f(&sched, self.task);
        {
            let mut stack = self.shared.sched.restore_stack(self.task);
            self.store().swap_open(&mut stack);
        }
        self.drain_sched();
        out
    }

    /// Frees a batch of regions wholesale — the killed/exited proc's step 3
    /// (`[conc.proc.kill]`), performed by whichever task holds the baton.
    /// Step 3 of the killed-proc sequence, and then step 4
    /// (`[conc.proc.kill]`, `[mem.region.cap.3]`): the proc's regions
    /// bulk-free here, and only then does the parked exit reason publish.
    ///
    /// The order is the clause's, stated once at the one seam that can keep
    /// it: the scheduler hands back regions it cannot free itself, so the
    /// publish must wait for whoever can. A join therefore reads
    /// `live_region_bytes()` at its pre-spawn value — no postmortem query can
    /// observe a dead proc's charge.
    pub(super) fn free_regions(&mut self, regions: Vec<RegionId>, span: Span) {
        for region in regions {
            let freed = self.store().free(region);
            self.prov().region_freed(&freed, span);
            if !freed.is_empty() {
                let labels = freed
                    .iter()
                    .map(|id| self.store().label(*id))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.fire(
                    Rule::ProcKill,
                    span,
                    &format!("proc regions bulk-free: {labels}"),
                );
            }
        }
        self.assert_forest(span);
        self.shared.sched.deliver_pending();
        self.drain_sched();
    }

    /// Has this run ever had a second task? (Gates the notes that only make
    /// sense in a concurrent program, so single-task traces stay stable.)
    pub(crate) fn concurrent(&self) -> bool {
        self.shared.sched.ever_concurrent()
    }

    /// `Mutex(v)` — a scheduler-owned sync object (`[conc.mm.hb.mutex]`).
    pub(crate) fn shared_mutex(&mut self, value: Value) -> MutexId {
        let id = self.shared.sched.new_mutex(value);
        self.drain_sched();
        id
    }

    // -- scopes and spawns (§2) ---------------------------------------------

    /// `scope name? { … }` (`[conc.task.scope]`): open, bind the handle, run
    /// the body, then **join** (`[conc.task.join]`) — the block does not
    /// complete until every child has completed or finished its cancellation.
    pub(super) fn eval_scope(
        &mut self,
        name: Option<&Ident>,
        body: &Block,
        span: Span,
    ) -> EResult<Value> {
        let scope_id = self
            .shared
            .sched
            .open_scope(self.task, name.map(|ident| ident.name.as_str()));
        self.drain_sched();
        self.push_scope();
        if let Some(ident) = name {
            self.fire(
                Rule::TaskScope,
                span,
                &format!(
                    "`{}` binds scope#{scope_id}: a first-class, passable capability",
                    ident.name
                ),
            );
            self.declare(&ident.name, Slot::live(Value::Scope(scope_id)));
        }
        let body_result = self.eval_block(body);
        if body_result.is_err() {
            // The owner itself failed (or is returning early): structured
            // teardown cancels the children before the join, so the join
            // terminates and the owner's own signal propagates.
            self.shared.sched.cancel_scope(scope_id);
            self.drain_sched();
        }
        let joined = self.sched_block(|sched, task| sched.join_scope(task, scope_id));
        let failures = match joined {
            Ok(failures) => failures,
            Err(Resolved::Killed) => return Err(Signal::ProcKilled),
            Err(Resolved::Deadlock(roster)) => {
                return self.deadlock_trap(&roster, span);
            }
            Err(_) => unreachable!("join resolves to killed or deadlock only"),
        };
        self.pop_scope();
        // `[conc.task.fail.owner]`: when a child failed, the owner blocked
        // inside the scope's extent was cancelled exactly like a sibling —
        // the cancellation error the owner's body then carried out is its
        // *finished cancellation*, not a new failure. The child's failure is
        // what re-raises at the scope exit, below, after the join.
        let body_result = match body_result {
            Err(Signal::Return(Value::Error(err)))
                if err.tag == "Cancelled" && !failures.is_empty() =>
            {
                self.fire(
                    Rule::TaskFailOwner,
                    span,
                    "the owner's cancellation completed; the child's failure re-raises at the                      scope exit",
                );
                Ok(Value::Unit)
            }
            other => other,
        };
        let body_value = body_result?;
        match failures.into_iter().next() {
            None => Ok(body_value),
            // `[conc.task.fail]`: the first failure in schedule order
            // re-raises at the scope exit; the siblings were already
            // cancelled by the scheduler when it happened.
            Some(TaskEnd::Error(err)) => {
                self.fire(
                    Rule::TaskFail,
                    span,
                    &format!(
                        "scope exit re-raises the first child failure: `{}`",
                        err.tag
                    ),
                );
                Err(Signal::Return(Value::Error(err)))
            }
            Some(TaskEnd::Trapped(trap)) => Err(Signal::Trap(trap)),
            Some(TaskEnd::Ub(finding)) => Err(Signal::Ub(finding)),
            Some(TaskEnd::Unsupported(reason)) => Err(Signal::Unsupported(reason)),
            Some(TaskEnd::Value(_) | TaskEnd::Cancelled | TaskEnd::Killed) => {
                unreachable!("not failures")
            }
        }
    }

    /// `s.spawn(closure)` (`[conc.task.spawn]`): commit the spawn, start the
    /// task's thread, return. The closure's captures were taken **by value**
    /// at construction (`[gram.expr.closure]`); D14's capture rules are the
    /// type checker's half (E1101), and the dynamic consequence — a task
    /// writes its own copies — is recorded in the approximation contract.
    #[cfg_attr(target_family = "wasm", allow(unreachable_code))]
    pub(super) fn spawn_closure_task(
        &mut self,
        scope: ScopeId,
        closure: &ClosureValue,
        span: Span,
    ) -> EResult<Value> {
        if !closure.params.is_empty() {
            return unsupported(
                "a spawned closure takes no parameters; passing values happens through captures \
                 or channels",
            );
        }
        // A task is an OS thread here (`[conc.task.model]`'s dynamic reading),
        // and wasm has none to spawn. Declining is the honest answer: the
        // alternative is a second, guessed scheduler for one platform, and
        // `[proto.record.unsupported]` exists for exactly this.
        #[cfg(target_family = "wasm")]
        {
            let _ = (scope, span);
            return unsupported(
                "the task tier needs one OS thread per task, and this wasm build has none to \
                 spawn; run the program under `lupin` for the concurrency rungs",
            );
        }
        let proc = self.shared.sched.proc_of(self.task);
        let name = format!("task@{}", span.start);
        let ambient = self.store().current();
        let task = self
            .shared
            .sched
            .spawn_task(self.task, Some(scope), proc, name.clone());
        self.shared.sched.seed_stack(task, vec![ambient]);
        let shared = self.shared.clone();
        let globals = self.globals.clone();
        let body = closure.clone();
        let module = self
            .frames
            .last()
            .map(|frame| frame.module.clone())
            .unwrap_or_default();
        let handle = std::thread::Builder::new()
            .name(name)
            .stack_size(TASK_STACK)
            .spawn(move || {
                Machine::for_task(shared, task, globals).run_spawned(&body, module);
            })
            .expect("a task thread must spawn");
        self.shared.sched.register_thread(task, handle);
        self.drain_sched();
        Ok(Value::Unit)
    }

    /// A spawned task's whole life, on its own thread.
    fn run_spawned(mut self, closure: &ClosureValue, module: String) {
        if !self.shared.sched.first_schedule(self.task) {
            let regions = self.shared.sched.end_task(self.task, TaskEnd::Killed);
            self.free_regions(regions, Span::new(0, 0));
            self.shared.sched.dispatch_next();
            return;
        }
        {
            let mut stack = self.shared.sched.restore_stack(self.task);
            self.store().swap_open(&mut stack);
        }
        let serial = self.mint_frame_serial();
        self.frames.push(Frame {
            module,
            serial,
            scopes: vec![Scope::default()],
            row: Vec::new(),
            read_params: Vec::new(),
        });
        for (name, value) in &closure.captures {
            self.declare(name, Slot::live(value.clone()));
        }
        let result = self.eval(&closure.body);
        self.finish_task_thread(result);
    }

    /// Frame teardown + outcome recording + baton handoff, shared by task and
    /// proc threads. Runs *before* the handoff so the trace stays in schedule
    /// order.
    fn finish_task_thread(mut self, result: EResult<Value>) {
        let frame = self.frames.len().saturating_sub(1);
        self.access.release_frame(frame);
        // The task frame's outermost scope holds the closure's **captures** —
        // clones of the spawner's locals, which the spawner still owns. They
        // are not swept, for exactly the reason function parameters are not
        // (`Machine::reclaim`'s documented approximation): sweeping a
        // captured region value would free a region the spawner still holds.
        // Everything the task itself created died with its block scopes.
        self.frames.pop();
        let task = self.task;
        self.prov().drop_frame(task, frame);
        {
            let mut stack = Vec::new();
            self.store().swap_open(&mut stack);
            self.shared.sched.save_stack(self.task, stack);
        }
        let end = match result {
            Ok(Value::Error(err)) | Err(Signal::Return(Value::Error(err))) => TaskEnd::Error(err),
            Ok(value) | Err(Signal::Return(value)) => TaskEnd::Value(value),
            Err(Signal::Trap(trap)) => TaskEnd::Trapped(trap),
            Err(Signal::Ub(finding)) => TaskEnd::Ub(finding),
            Err(Signal::Unsupported(reason)) => TaskEnd::Unsupported(reason),
            Err(Signal::ProcKilled) => TaskEnd::Killed,
            // `os_exit` inside a spawned task: the builtin refuses before
            // this point (`machine.concurrent()`); reaching here anyway is
            // conservatism, not a guess.
            Err(Signal::Exit(code)) => {
                TaskEnd::Unsupported(format!("`os_exit({code})` escaped a task body"))
            }
            Err(Signal::Break(_) | Signal::Continue) => {
                TaskEnd::Unsupported("`break`/`continue` escaped a task body".to_owned())
            }
        };
        let regions = self.shared.sched.end_task(self.task, end);
        self.free_regions(regions, Span::new(0, 0));
        self.drain_sched();
        self.shared.sched.dispatch_next();
    }

    // -- procs (§3) ---------------------------------------------------------

    /// `spawn proc worker(args)` (`[conc.proc.1]`, `[conc.task.root]`): a
    /// failure domain under the root supervisor, owning a fresh region its
    /// allocations land in.
    #[cfg_attr(target_family = "wasm", allow(unreachable_code))]
    pub(super) fn eval_spawn_proc(
        &mut self,
        path: &crate::ast::Path,
        args: &[Arg],
        span: Span,
    ) -> EResult<Value> {
        // A proc's root task is an OS thread, as a task's is; see
        // `spawn_closure_task` for why wasm declines rather than mocks.
        #[cfg(target_family = "wasm")]
        {
            let _ = (path, args, span);
            return unsupported(
                "the proc tier needs an OS thread per proc, and this wasm build has none to \
                 spawn; run the program under `lupin` for the supervision rungs",
            );
        }
        let current_module = self
            .frames
            .last()
            .map(|frame| frame.module.clone())
            .unwrap_or_default();
        let (module, name) = if path.segments.len() >= 2 {
            (path.segments[0].name.clone(), path.segments[1].name.clone())
        } else {
            (
                current_module.clone(),
                path.segments
                    .first()
                    .map(|segment| segment.name.clone())
                    .unwrap_or_default(),
            )
        };
        let Some(super::Def::Fn(decl)) = self
            .shared
            .program
            .lookup(&module, &name, module != current_module)
            .cloned()
        else {
            return unsupported(format!("`{name}` does not name a proc body function"));
        };
        let evaluated = self.eval_args(args)?;
        let values = evaluated.values.clone();
        self.finish_args(&[], &values, evaluated.held, &evaluated.protectors, span);

        // A proc's own region carries no cap: `[mem.region.cap.1]` puts the
        // budget where a program writes one, and the proc region is the
        // failure domain's ambient, not a region the source created.
        let region = self
            .store()
            .create(Some(format!("proc:{name}")), Strategy::Arena, None, span);
        self.store().force_open(region);
        self.fire(
            Rule::ProcModel,
            span,
            &format!(
                "proc `{name}` owns region #{region}: allocation inside the proc lands there \
                 ([conc.proc.1]); its only cross-proc surface is typed channels"
            ),
        );
        let (proc, task) = self.shared.sched.spawn_proc(self.task, name, region);
        self.shared.sched.seed_stack(task, vec![region]);
        let shared = self.shared.clone();
        let globals = self.globals.clone();
        let handle = std::thread::Builder::new()
            .name(format!("proc#{proc}"))
            .stack_size(TASK_STACK)
            .spawn(move || {
                Machine::for_task(shared, task, globals).run_proc(&decl, &module, values, span);
            })
            .expect("a proc thread must spawn");
        self.shared.sched.register_thread(task, handle);
        self.drain_sched();
        Ok(Value::Proc(proc))
    }

    /// A proc's root task: run the body function, exit with a reason
    /// (`[conc.proc.exit]`).
    fn run_proc(mut self, decl: &crate::ast::FnDecl, module: &str, args: Vec<Value>, span: Span) {
        if !self.shared.sched.first_schedule(self.task) {
            let regions = self.shared.sched.end_task(self.task, TaskEnd::Killed);
            self.free_regions(regions, span);
            self.shared.sched.dispatch_next();
            return;
        }
        {
            let mut stack = self.shared.sched.restore_stack(self.task);
            self.store().swap_open(&mut stack);
        }
        // `call_fn` pushes its own frame; give the thread a root frame so
        // frame teardown in `finish_task_thread` has something to pop.
        let serial = self.mint_frame_serial();
        self.frames.push(Frame {
            module: module.to_owned(),
            serial,
            scopes: vec![Scope::default()],
            row: Vec::new(),
            read_params: Vec::new(),
        });
        let result = self
            .call_fn(decl, module, args, span)
            .map(|applied| applied.value);
        self.finish_task_thread(result);
    }

    /// `w.monitor()` (`[conc.proc.2]`): the exit reason will arrive on this
    /// channel as `exit(reason)` — a typed value, never unwinding (D30).
    pub(crate) fn proc_monitor(&mut self, proc: ProcId, span: Span) -> EResult<Value> {
        let chan = self.shared.sched.monitor(proc);
        self.drain_sched();
        self.fire(
            Rule::ProcMailbox,
            span,
            &format!(
                "monitor mailbox channel#{chan}: procs communicate exclusively via typed \
                 channels + `select` (03 Q3); handlers are atomic and non-blocking"
            ),
        );
        Ok(Value::Chan(chan))
    }

    /// `w.kill()` (`[conc.proc.kill]`): the decided sequence, in order.
    pub(crate) fn proc_kill(&mut self, proc: ProcId, span: Span) -> EResult<Value> {
        let regions = self.shared.sched.kill(proc);
        self.drain_sched();
        self.free_regions(regions, span);
        Ok(Value::Unit)
    }

    /// `a.link(b)` / `w.link()` (`[conc.proc.link.pair]`): symmetric fate
    /// coupling, idempotent per pair; the no-argument spelling is
    /// `w.link(<the calling task's proc>)`. Delivery is `[conc.proc.2]`'s.
    pub(crate) fn proc_link(
        &mut self,
        proc: ProcId,
        other: Option<ProcId>,
        span: Span,
    ) -> EResult<Value> {
        let partner = match other {
            Some(partner) => partner,
            None => self.shared.sched.proc_of(self.task),
        };
        self.fire(
            Rule::ProcLinkPair,
            span,
            &format!(
                "link proc#{proc} ⇄ proc#{partner}{}",
                if other.is_some() {
                    ""
                } else {
                    " (`w.link()` couples `w` with the calling task's proc)"
                }
            ),
        );
        self.shared.sched.link(partner, proc);
        self.drain_sched();
        Ok(Value::Unit)
    }

    /// `w.cancel()` (`[conc.proc.cancel]`): structured cancellation to the
    /// proc's task tree — the delivery mechanism that makes
    /// `[conc.proc.exit]`'s `cancelled` reason reachable (finding S-6,
    /// resolved by the pinned amendment).
    pub(crate) fn proc_cancel(&mut self, proc: ProcId, span: Span) -> EResult<Value> {
        self.fire(
            Rule::ProcCancel,
            span,
            &format!(
                "proc#{proc} receives structured cancellation: cooperative at blocking points, \
                 defers run; exits `cancelled` unless its value completes anyway"
            ),
        );
        self.shared.sched.cancel_proc(proc);
        self.drain_sched();
        Ok(Value::Unit)
    }

    // -- channels (§3) ------------------------------------------------------

    /// `channel[T](n)` (`[conc.chan.type]`, `[conc.chan.buf]`). Sendability
    /// of T is the type checker's E1102; this machine checks the *dynamic*
    /// transfer rules at each send instead.
    pub(crate) fn chan_new(&mut self, cap: usize, span: Span) -> EResult<Value> {
        let chan = self.shared.sched.new_chan(cap);
        self.drain_sched();
        self.fire(
            Rule::ChanType,
            span,
            &format!(
                "channel#{chan}: payload sendability (Copy, imm, moved region, sync) is E1102's \
                 static half; the dynamic transfer rules run at each send"
            ),
        );
        Ok(Value::Chan(chan))
    }

    /// `ch.send(v)` / `ch.send(move r)`. A region payload's send is its affine
    /// move (`[conc.chan.move]`): the dynamic disconnectedness check runs at
    /// the moment of send, and the sender's later touches fault at the use
    /// site (`[conc.chan.staleuse]`, [`Rule::ChanStale`]).
    pub(crate) fn chan_send(&mut self, chan: ChanId, value: Value, span: Span) -> EResult<Value> {
        let mut transferred = Vec::new();
        for id in region_granules(&value) {
            let (state, parent, depth) = {
                let store = self.store();
                let region = store.region(id);
                (
                    store.state(id),
                    region.and_then(|r| r.parent),
                    region.map_or(0, |r| r.depth),
                )
            };
            if state == Some(super::region::RegionState::Frozen) {
                // `[conc.chan.imm]`'s meaning via `[conc.mm.hb.freeze]`:
                // frozen data shares by reference; nothing transfers.
                self.fire(
                    Rule::ChanImm,
                    span,
                    &format!(
                        "frozen region #{id} shares by reference through channel#{chan}; \
                         no transfer, readable from any task forever"
                    ),
                );
                continue;
            }
            if depth > 0 {
                let label = self.store().label(id);
                return self.region_fault(
                    Rule::RegionClosedSubtree,
                    span,
                    format!(
                        "{label} is open here and cannot be sent; a region transfers as a \
                         closed subtree (the compiler's E1005)"
                    ),
                    None,
                );
            }
            if let Some(parent) = parent {
                // The dynamic disconnectedness check: what SE-0414-style
                // inference proves statically, checked at the send.
                let label = self.store().label(id);
                let owner = self.store().label(parent);
                return self.region_fault(
                    Rule::ChanMove,
                    span,
                    format!(
                        "{label} still has a live external edge from {owner}; a region sent \
                         through a channel must be disconnected ([conc.chan.move])"
                    ),
                    None,
                );
            }
            self.store().claim(id, Some(TaskClaim::InChannel));
            self.fire(
                Rule::ChanMove,
                span,
                &format!(
                    "region #{id} moves wholesale into channel#{chan}: the send is the affine \
                     move, and every prior write publishes to the receiver ([conc.mm.hb.move])"
                ),
            );
            transferred.push(id);
        }
        let resolved = self.sched_block(|sched, task| sched.send(task, chan, value));
        match resolved {
            Resolved::Value(_) => Ok(Value::Unit),
            Resolved::Err(err) => {
                // The send never happened (closed, or cancellation delivered
                // at the blocking point): the in-flight claim comes back.
                for id in transferred {
                    self.store().claim(id, Some(TaskClaim::Task(self.task)));
                }
                self.fire(
                    Rule::CancelPoint,
                    span,
                    "the send returns an error value; nothing transferred",
                );
                Ok(err)
            }
            Resolved::Killed => Err(Signal::ProcKilled),
            Resolved::Deadlock(roster) => self.deadlock_trap(&roster, span),
        }
    }

    /// `ch.recv()`: the received value, or the closed/cancelled error value.
    /// Receiving a moved region claims it for this task ([conc.mm.hb.move]).
    pub(crate) fn chan_recv(&mut self, chan: ChanId, span: Span) -> EResult<Value> {
        let resolved = self.sched_block(|sched, task| sched.recv(task, chan));
        match resolved {
            Resolved::Value(value) => {
                for id in region_granules(&value) {
                    let frozen = self.store().state(id) == Some(super::region::RegionState::Frozen);
                    if !frozen {
                        self.store().claim(id, Some(TaskClaim::Task(self.task)));
                        self.fire(
                            Rule::ChanMove,
                            span,
                            &format!(
                                "region #{id} received: this task owns the graph now; the \
                                 sender's accesses are stale"
                            ),
                        );
                    }
                }
                Ok(value)
            }
            Resolved::Err(err) => Ok(err),
            Resolved::Killed => Err(Signal::ProcKilled),
            Resolved::Deadlock(roster) => self.deadlock_trap(&roster, span),
        }
    }

    /// `ch.close()` (`[conc.chan.close]`).
    pub(crate) fn chan_close(&mut self, chan: ChanId, span: Span) -> EResult<Value> {
        self.shared.sched.close(chan);
        self.drain_sched();
        let _ = span;
        Ok(Value::Unit)
    }

    /// `for v in ch` iterates until drained-close (`[conc.chan.close]`).
    pub(super) fn eval_for_chan(
        &mut self,
        chan: ChanId,
        pattern: &crate::ast::Pattern,
        body: &Block,
        span: Span,
    ) -> EResult<Value> {
        loop {
            self.step()?;
            let value = self.chan_recv(chan, span)?;
            if let Value::Error(err) = &value {
                if err.tag == "Closed" {
                    self.fire(Rule::ChanClose, span, "`for v in ch` ends at drained-close");
                    return Ok(Value::Unit);
                }
                // Cancellation surfaced at the loop's blocking point: the
                // error value propagates by ordinary return
                // ([conc.cancel.defer] — the frames' defers run on the way).
                return Err(Signal::Return(value));
            }
            self.push_scope();
            let bound = self.bind_pattern(pattern, value);
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
    }

    // -- select (§3) --------------------------------------------------------

    /// `select { … }` (`[conc.select.ready]`, `[conc.select.fair]`,
    /// `[conc.select.timeout]`).
    pub(super) fn eval_select(&mut self, arms: &[SelectArm], span: Span) -> EResult<Value> {
        let mut chan_arms: Vec<(usize, ChanId)> = Vec::new();
        let mut timeout: Option<(usize, u128)> = None;
        for (index, arm) in arms.iter().enumerate() {
            match &arm.kind {
                SelectArmKind::Recv { channel, .. } => {
                    let value = self.eval(channel)?;
                    let Value::Chan(chan) = value else {
                        return unsupported(format!(
                            "a `select` arm receives from a channel, got {}",
                            value.kind()
                        ));
                    };
                    chan_arms.push((index, chan));
                }
                SelectArmKind::Timeout(duration) => {
                    let value = self.eval(duration)?;
                    let nanos = match value {
                        Value::Duration(nanos) => nanos,
                        other => {
                            return unsupported(format!(
                                "`timeout(d)` takes a duration (`1.s`, `20.ms`), got {}",
                                other.kind()
                            ));
                        }
                    };
                    let deadline = self.shared.sched.now().saturating_add(nanos);
                    // Several timeout arms: the earliest is the one that can
                    // fire; later ones are dead arms.
                    if timeout.is_none_or(|(_, existing)| deadline < existing) {
                        timeout = Some((index, deadline));
                    }
                }
            }
        }
        let committed = self.sched_block(|sched, task| sched.select(task, &chan_arms, timeout));
        match committed {
            Ok((index, value)) => {
                let arm = &arms[index];
                self.push_scope();
                if let (SelectArmKind::Recv { pattern, .. }, Some(value)) = (&arm.kind, &value) {
                    for id in region_granules(value) {
                        let frozen =
                            self.store().state(id) == Some(super::region::RegionState::Frozen);
                        if !frozen {
                            self.store().claim(id, Some(TaskClaim::Task(self.task)));
                        }
                    }
                    let matched = self.match_pattern(pattern, value);
                    match matched {
                        Ok(true) => {}
                        Ok(false) => {
                            self.pop_scope();
                            return unsupported(
                                "the committed `select` arm's pattern did not match the \
                                 received value; pattern typing is the checker's",
                            );
                        }
                        Err(signal) => {
                            self.pop_scope();
                            return Err(signal);
                        }
                    }
                }
                let result = self.eval(&arm.body);
                self.pop_scope();
                result
            }
            Err(Resolved::Err(err)) => {
                // Cancellation at the `select` blocking point: the error
                // value surfaces and ordinary returns do the rest.
                self.fire(
                    Rule::CancelPoint,
                    span,
                    "cancellation delivered at `select`; the error value returns",
                );
                Err(Signal::Return(err))
            }
            Err(Resolved::Killed) => Err(Signal::ProcKilled),
            Err(Resolved::Deadlock(roster)) => self.deadlock_trap(&roster, span),
            Err(Resolved::Value(_)) => unreachable!("select resolves through its arms"),
        }
    }

    // -- when (`[conc.when]`, the s20 S-batch) ------------------------------

    /// `when (a, b) { … }` (`[conc.when.order]`): canonical-order acquisition
    /// of the whole operand set — deadlock-free by construction
    /// (`[conc.when.nodeadlock]`) — then the body with each operand rebound
    /// to its payload, then write-back and release in reverse canonical
    /// order (`[conc.when.body]`).
    pub(super) fn eval_when(
        &mut self,
        operands: &[Expr],
        body: &Block,
        span: Span,
    ) -> EResult<Value> {
        let mut set: Vec<(MutexId, Option<String>)> = Vec::new();
        for operand in operands {
            let name = match &*operand.kind {
                crate::ast::ExprKind::Path(path) if path.is_single() => {
                    Some(path.segments[0].name.clone())
                }
                _ => None,
            };
            let value = self.eval(operand)?;
            let Value::MutexRef(id) = value else {
                return unsupported(format!(
                    "`when` acquires `Mutex` values, got {}; `sync` typing is the checker's",
                    value.kind()
                ));
            };
            set.push((id, name));
        }
        // Canonical order: by scheduler mutex id — creation order, the same
        // total order at every `when` site (`[conc.when.order]`), which is
        // the whole deadlock-freedom argument (`[conc.when.nodeadlock]`).
        let mut order = set.clone();
        order.sort_by_key(|(id, _)| *id);
        order.dedup_by_key(|(id, _)| *id);
        // `[conc.deadlock.self]`: an acquisition of a sync object this task
        // already holds — a `when` reached *through a call* from inside a
        // `when` body whose sets intersect — can never complete. Detected
        // immediately. (The lexical nest is the compiler's E1103,
        // `[conc.when.nonest]`; a dynamically nested `when` over a DISJOINT
        // set is not self-acquisition and proceeds.)
        if let Some((held, _)) = order.iter().find(|(id, _)| self.when_held.contains(id)) {
            let held = *held;
            self.fire(
                Rule::WhenNoNest,
                span,
                "a nested acquisition reached dynamically is incremental acquisition by \
                 another spelling; lexical nesting is E1103, this one is self-acquisition",
            );
            return self.trap(
                TrapKind::Deadlock,
                Rule::DeadlockSelf,
                span,
                format!(
                    "this task already holds mutex#{held}: acquiring a sync object the \
                     acquiring task already holds can never complete ([conc.deadlock.self])"
                ),
                None,
            );
        }
        self.fire(
            Rule::WhenOrder,
            span,
            &format!(
                "`when` acquires {{{}}} in canonical order",
                order
                    .iter()
                    .map(|(id, _)| format!("mutex#{id}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        self.fire(
            Rule::WhenNoDeadlock,
            span,
            "one canonical order at every `when` site: no cycle of `when` acquisitions can form",
        );
        let mut acquired: Vec<(MutexId, Option<String>, Value)> = Vec::new();
        for (id, name) in order {
            let resolved = self.sched_block(|sched, task| sched.acquire(task, id));
            match resolved {
                Resolved::Value(payload) => acquired.push((id, name, payload)),
                Resolved::Err(err) => {
                    // Cancellation at an acquisition point: hand back what was
                    // taken (bookkeeping, not user code) and surface the error.
                    for (id, _, payload) in acquired.into_iter().rev() {
                        self.shared.sched.release(self.task, id, payload);
                    }
                    self.drain_sched();
                    return Err(Signal::Return(err));
                }
                Resolved::Killed => {
                    for (id, _, payload) in acquired.into_iter().rev() {
                        self.shared.sched.release(self.task, id, payload);
                    }
                    self.drain_sched();
                    return Err(Signal::ProcKilled);
                }
                Resolved::Deadlock(roster) => return self.deadlock_trap(&roster, span),
            }
        }
        for (id, _, _) in &acquired {
            self.when_held.push(*id);
        }
        self.push_scope();
        for (_, name, payload) in &acquired {
            if let Some(name) = name {
                self.declare(name, Slot::live(payload.clone()));
            }
        }
        self.fire(
            Rule::WhenBody,
            span,
            "the body holds exclusive access to every operand's payload; simple-path operands \
             rebind to their payloads and write back at release",
        );
        let result = self.eval_block(body);
        // Write back and release in reverse acquisition order; the release
        // publishes the body's writes to the next acquirer
        // (`[conc.mm.hb.mutex]`).
        let releases: Vec<(MutexId, Option<String>, Value)> = acquired
            .iter()
            .rev()
            .map(|(id, name, payload)| {
                let value = name
                    .as_ref()
                    .and_then(|name| {
                        let path = Path::local(self.frames.len() - 1, name.clone());
                        self.slot_mut(&path).map(|slot| slot.value.clone())
                    })
                    .unwrap_or_else(|| payload.clone());
                (*id, name.clone(), value)
            })
            .collect();
        self.pop_scope();
        for (id, _, _) in &acquired {
            if let Some(at) = self.when_held.iter().rposition(|held| held == id) {
                self.when_held.remove(at);
            }
        }
        for (id, _, value) in releases {
            self.shared.sched.release(self.task, id, value);
        }
        self.drain_sched();
        result
    }

    /// `[conc.deadlock.trap]`: a detected deadlock terminates with trap kind
    /// `deadlock`, reporting the blocked-task roster. The scheduler detected
    /// it (`[conc.deadlock.def]`: every live task blocked, no pending timer,
    /// no in-flight I/O); this is the surfacing, on whichever task's blocking
    /// operation the wake reached.
    fn deadlock_trap<T>(&mut self, roster: &str, span: Span) -> EResult<T> {
        self.trap(
            TrapKind::Deadlock,
            Rule::DeadlockTrap,
            span,
            format!(
                "every live task is blocked at a runtime-owned blocking point and no timer is \
                 pending; blocked-task roster: {roster}"
            ),
            None,
        )
    }
}

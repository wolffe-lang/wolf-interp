//! The dynamic region store — `spec/02-memory-model.md` §3 and §4 as data.
//!
//! `[mem.model.machine]` names four components of the abstract machine state.
//! This module owns two of them:
//!
//! > 3. **Region forest**: regions with parent edges (≤1 each), state
//! >    (Open/Suspended/Frozen/Freed), and strategy (arena/rc/pool).
//! > 4. **Scope stack**: the currently-open regions and active borrows.
//!
//! (Component 1's liveness bit lives here too, as [`RegionState::Freed`] plus
//! the per-slot generation; component 2, the provenance forest, is is04's.)
//!
//! # What has identity and what does not
//!
//! `[mem.model.value]` says values "have no identity beyond their current
//! place", and is02 took that literally: [`super::value::Value`] is a plain
//! owned Rust tree, so a wolf assignment *is* a Rust move and a wolf `copy` *is*
//! a Rust clone. That decision has a consequence this sprint has to state out
//! loud: **an aggregate cannot dangle**, because copying it out of a region
//! copies the data. The granules that *can* dangle are the ones the language
//! gives identity to — region values, pools, handles, and `shared` cells — and
//! those are exactly what this store holds. Use-after-free is therefore exact
//! (id + generation, never probabilistic) over precisely the set of things that
//! can be dangling references, which is the honest reading of
//! `[mem.region.intra.2]` under MVS.
//!
//! # The scope stack is a stack of *open* regions with a current one
//!
//! `[mem.region.create.3]`'s ambient rule and `[mem.region.multiopen]`'s open
//! set are one structure: [`Store::open`] is the scope stack, its last element
//! is the **current** (ambient) region, and its whole extent is the **open
//! set**. `in r { … }` pushes; leaving pops. A function body does not push, so
//! "the default current region of a function body is its caller's current
//! region" is not a rule this machine implements — it is a rule it cannot help
//! obeying.

use std::fmt;

use super::value::Value;
use crate::diag::Span;

pub type RegionId = usize;
pub type PoolId = usize;
pub type CellId = usize;

/// The region-state lattice of `[mem.model.machine]`.
///
/// The transitions are one-way except Open⇄Suspended: `[mem.region.open.1]`
/// opens and closes, `[mem.region.freeze.1]` freezes "forever", and
/// `[mem.region.intra.2]` frees wholesale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionState {
    /// Mutable in the current scope.
    Open,
    /// Not open here: read-only (`[mem.region.open.3]`).
    Suspended,
    /// `imm` forever (`[mem.region.freeze.1]`).
    Frozen,
    /// Wholesale-freed (`[mem.region.intra.2]`).
    Freed,
}

impl fmt::Display for RegionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RegionState::Open => "open",
            RegionState::Suspended => "suspended",
            RegionState::Frozen => "frozen",
            RegionState::Freed => "freed",
        })
    }
}

/// Per-region strategy (D12, `[mem.region.create.1]`): "strategy changes cost
/// and §4 availability, never safety".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Strategy {
    Arena,
    Rc,
    Pool(String),
}

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Strategy::Arena => f.write_str("arena"),
            Strategy::Rc => f.write_str("rc"),
            Strategy::Pool(ty) => write!(f, "pool({ty})"),
        }
    }
}

/// One region in the forest.
#[derive(Debug, Clone)]
pub struct Region {
    pub id: RegionId,
    /// The sugar's binder, when it had one — messages quote source.
    pub name: Option<String>,
    pub state: RegionState,
    pub strategy: Strategy,
    /// The owning edge, when one exists (`[mem.region.edge.iso]`: at most one).
    pub parent: Option<RegionId>,
    /// Bumped on free, so a surviving region value is stale *exactly*.
    pub generation: u32,
    /// How many allocations were charged here (`[mem.region.create.3]`).
    pub allocations: u64,
    /// Re-entrant open depth. See [`Store::enter`] for why this is a count and
    /// not a flag.
    pub depth: u32,
    /// Which task may touch this region, once it has crossed a task boundary
    /// (is06, spec/03 §3: a region `move`d through a channel transfers
    /// wholesale, and any later touch by the sender faults). `None` means the
    /// region has never been transferred and the single-task rules apply.
    pub claim: Option<TaskClaim>,
    pub span: Span,
}

/// Where a transferred region currently lives (spec/03 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskClaim {
    /// Sent, not yet received: owned by the channel, touchable by nobody.
    InChannel,
    /// Owned by one task; every other task's touch is a `region-fault`.
    Task(usize),
}

impl Region {
    /// The spelling used in fault messages.
    #[must_use]
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => format!("`{name}` (region #{})", self.id),
            None => format!("region #{}", self.id),
        }
    }
}

/// A pool slot's life stage — the two-phase shape of `[mem.shared.handle.1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotLife {
    /// `reserve()` happened, `init` has not: the handle exists and is not null,
    /// but the storage is uninitialized.
    Reserved,
    Live,
    /// Removed. The generation has already been bumped, so every handle that
    /// named this slot is stale (`[mem.shared.handle.2]`).
    Free,
}

/// One slot of a `pool(T)` region.
#[derive(Debug, Clone)]
pub struct PoolSlot {
    pub generation: u32,
    pub life: SlotLife,
    pub value: Value,
}

/// A `Pool[T]`, anchored to the region that was current when it was built.
#[derive(Debug, Clone)]
pub struct Pool {
    pub id: PoolId,
    pub region: RegionId,
    pub elem: String,
    pub slots: Vec<PoolSlot>,
}

/// A `shared T` cell: honest, naive, non-atomic refcounts.
///
/// `[mem.shared.rc.4]` makes RC operations unobservable *except* through
/// destructor timing, so the interpreter runs the naive version and lets the
/// compiler's Perceus elision be judged against it (is05).
#[derive(Debug, Clone)]
pub struct Cell {
    pub strong: u32,
    pub weak: u32,
    pub value: Value,
    /// Strong edges out of this cell, for the acyclicity assertion
    /// (`[mem.shared.rc.2]`).
    pub edges: Vec<CellId>,
    /// The payload has been released (strong hit zero).
    pub dead: bool,
    pub span: Span,
}

/// A reference granule found inside a value — the things §3's edge table is
/// about ("source stores a **reference** to target").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ref {
    Region(RegionId),
    Pool(PoolId),
    Handle(PoolId),
    Shared(CellId),
    Weak(CellId),
}

/// Why a region could not be opened — which clause refused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnterError {
    /// `[mem.region.open.1]`: the state forbids it (freed, or frozen forever).
    State(String),
    /// `[mem.region.multiopen]`: the open set would stop being an antichain.
    NotDisjoint(String),
}

impl EnterError {
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            EnterError::State(reason) | EnterError::NotDisjoint(reason) => reason,
        }
    }
}

/// Why a store of a reference is or is not legal (§3's edge table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edge {
    /// Same region: unrestricted (`[mem.region.intra.1]`).
    Intra,
    /// The owning edge into a child (`[mem.region.edge.iso]`).
    Iso(RegionId),
    /// Into `Frozen` data, from anywhere, forever (`[mem.region.edge.imm]`).
    Imm,
    /// A Tier-2 cell: the §4 rules govern, not §3's table.
    Tier2,
    /// Off the table: the static half is E1004, the dynamic half is a
    /// `region-fault`. The string is the reason, rendered.
    Illegal(String),
}

/// The region table, the pools, and the `shared` heap.
#[derive(Debug, Default)]
pub struct Store {
    regions: Vec<Region>,
    pools: Vec<Pool>,
    cells: Vec<Cell>,
    /// The scope stack: every open region, innermost last. The last element is
    /// the **current** region; the whole vector is the **open set**.
    open: Vec<RegionId>,
}

impl Store {
    /// A store with the program region already created and open.
    ///
    /// There is always a current region, because `[mem.region.create.3]` says
    /// "every function executes with a *current region*" without exception, and
    /// a machine with an empty ambient stack could not honour that for `main`.
    #[must_use]
    pub fn new() -> Store {
        let mut store = Store::default();
        let root = store.create(Some("program".to_owned()), Strategy::Arena, Span::new(0, 0));
        store.open.push(root);
        store.regions[root].state = RegionState::Open;
        store.regions[root].depth = 1;
        store
    }

    /// The program region — the ambient region `main` starts in.
    #[must_use]
    pub const fn root() -> RegionId {
        0
    }

    // -- regions -----------------------------------------------------------

    /// `[mem.region.create.1]`: both surfaces create a region here.
    ///
    /// A fresh region has no parent: `[mem.region.edge.iso]` says the owning
    /// handle is "the region value or the iso field holding it", and a region
    /// value freshly bound to a local is owned by a *stack* slot, which is not
    /// a region. The parent edge appears only when the value is stored into
    /// region data — see [`Store::classify_edge`].
    pub fn create(&mut self, name: Option<String>, strategy: Strategy, span: Span) -> RegionId {
        let id = self.regions.len();
        self.regions.push(Region {
            id,
            name,
            state: RegionState::Suspended,
            strategy,
            parent: None,
            generation: 0,
            allocations: 0,
            depth: 0,
            claim: None,
            span,
        });
        id
    }

    /// Claims a region subtree for a task or a channel (spec/03 §3, is06).
    /// A `move` send claims for the channel; the receive claims for the
    /// receiving task; `freeze` clears the claim (`imm` shares by reference).
    pub fn claim(&mut self, id: RegionId, claim: Option<TaskClaim>) {
        for target in self.descendants(id) {
            if let Some(region) = self.regions.get_mut(target) {
                region.claim = claim;
            }
        }
    }

    /// The claim on a region, when a transfer has ever happened to it.
    #[must_use]
    pub fn claim_of(&self, id: RegionId) -> Option<TaskClaim> {
        self.regions.get(id).and_then(|region| region.claim)
    }

    /// Opens a region for a task that is *not* the running one — the fresh
    /// proc's own region, entered before its root task ever runs. No stack
    /// push here: the task's seeded stack carries it (is06).
    pub fn force_open(&mut self, id: RegionId) {
        if let Some(region) = self.regions.get_mut(id) {
            region.state = RegionState::Open;
            region.depth += 1;
        }
    }

    /// Swaps the whole open stack — the scheduler's context switch. Each task
    /// carries its own scope stack (`[mem.region.create.3]`'s ambient rule is
    /// per task); the store holds the *running* task's.
    pub fn swap_open(&mut self, stack: &mut Vec<RegionId>) {
        std::mem::swap(&mut self.open, stack);
    }

    #[must_use]
    pub fn region(&self, id: RegionId) -> Option<&Region> {
        self.regions.get(id)
    }

    #[must_use]
    pub fn state(&self, id: RegionId) -> Option<RegionState> {
        self.regions.get(id).map(|r| r.state)
    }

    #[must_use]
    pub fn generation(&self, id: RegionId) -> u32 {
        self.regions.get(id).map_or(0, |r| r.generation)
    }

    #[must_use]
    pub fn label(&self, id: RegionId) -> String {
        self.regions
            .get(id)
            .map_or_else(|| format!("region #{id}"), Region::label)
    }

    /// The current (ambient) region: where allocations land
    /// (`[mem.region.create.3]`).
    #[must_use]
    pub fn current(&self) -> RegionId {
        self.open.last().copied().unwrap_or_else(Store::root)
    }

    /// The open set, innermost last (`[mem.region.multiopen]`).
    #[must_use]
    pub fn open_set(&self) -> &[RegionId] {
        &self.open
    }

    /// Charges one allocation to the current region (`[mem.region.create.3]`).
    pub fn charge(&mut self) -> RegionId {
        let id = self.current();
        if let Some(region) = self.regions.get_mut(id) {
            region.allocations += 1;
        }
        id
    }

    /// Is `ancestor` an ancestor of (or equal to) `id` in the region forest?
    #[must_use]
    pub fn is_ancestor(&self, ancestor: RegionId, id: RegionId) -> bool {
        let mut at = Some(id);
        let mut guard = 0usize;
        while let Some(current) = at {
            if current == ancestor {
                return true;
            }
            guard += 1;
            if guard > self.regions.len() {
                // A cycle: the forest invariant is broken, and the assertion
                // below is the one that reports it. Stop rather than spin.
                return false;
            }
            at = self.regions.get(current).and_then(|r| r.parent);
        }
        false
    }

    /// Opens a region (`[mem.region.open.1]`), or says why it cannot be opened.
    ///
    /// Two rules beyond the obvious, both forced by the pinned corpus:
    ///
    /// 1. **Re-entrant opens are idempotent.** `corpus/memory/region_multiopen_swap.lu`
    ///    writes `region a: pool(Node) { … in a { … } … }` — the sugar opens
    ///    `a` for the whole block and `in a` opens it again inside. Read
    ///    literally, `[mem.region.open.1]`'s "Open in at most one scope at a
    ///    time" makes that file (which the corpus pins at `run(exit=0)`) a
    ///    violation. The reading that keeps the corpus green is a depth count,
    ///    implemented here; the clause needs the amendment.
    /// 2. **The open set must be an antichain.** `[mem.region.multiopen]`
    ///    discharges disjointness with "the open set is a set of *distinct
    ///    region values*", which affinity guarantees. Distinctness is not
    ///    disjointness: `[mem.region.edge.iso]` lets a region own another, and
    ///    an ancestor's open window reaches its descendant's data. Opening an
    ///    ancestor or descendant of an already-open region is therefore
    ///    rejected here even though the clause as written permits it.
    ///
    /// # Errors
    ///
    /// The reason, for the caller's `region-fault`.
    pub fn enter(&mut self, id: RegionId) -> Result<(), EnterError> {
        let Some(region) = self.regions.get(id) else {
            return Err(EnterError::State(format!("region #{id} does not exist")));
        };
        match region.state {
            RegionState::Freed => {
                return Err(EnterError::State(format!(
                    "{} was freed wholesale and cannot be opened again",
                    self.label(id)
                )));
            }
            RegionState::Frozen => {
                return Err(EnterError::State(format!(
                    "{} is frozen: `imm` data is readable from anywhere and writable from \
                     nowhere, so there is no window to open",
                    self.label(id)
                )));
            }
            RegionState::Open | RegionState::Suspended => {}
        }
        // A re-entrant open (depth > 0) skips the antichain check: the region
        // is already in the open set, so it cannot make the set worse.
        if region.depth == 0
            && let Some(other) = self
                .open
                .iter()
                .copied()
                .find(|other| self.is_ancestor(*other, id) || self.is_ancestor(id, *other))
        {
            return Err(EnterError::NotDisjoint(format!(
                "{} is not disjoint from the already-open {}: one owns the other, and an \
                     owner's open window reaches its child's data. `[mem.region.multiopen]` \
                     discharges disjointness with distinctness of affine values, which does not \
                     imply it across an `iso` edge",
                self.label(id),
                self.label(other)
            )));
        }
        let region = &mut self.regions[id];
        region.state = RegionState::Open;
        region.depth += 1;
        self.open.push(id);
        Ok(())
    }

    /// Closes the innermost open region: state Suspended when the last scope
    /// holding it leaves (`[mem.region.open.1]`).
    pub fn leave(&mut self, id: RegionId) {
        if let Some(at) = self.open.iter().rposition(|open| *open == id) {
            self.open.remove(at);
        }
        if let Some(region) = self.regions.get_mut(id) {
            region.depth = region.depth.saturating_sub(1);
            if region.depth == 0 && region.state == RegionState::Open {
                region.state = RegionState::Suspended;
            }
        }
    }

    /// Frees a region and every region it owns, wholesale
    /// (`[mem.region.intra.2]`). Idempotent: freeing a freed region is a no-op,
    /// which is what lets the sugar free at `}` and the scope-exit sweep run
    /// over the same binding without double-freeing.
    ///
    /// Returns the ids actually freed, innermost first, for the trace.
    pub fn free(&mut self, id: RegionId) -> Vec<RegionId> {
        let mut freed = Vec::new();
        let descendants = self.descendants(id);
        for target in descendants {
            let Some(region) = self.regions.get_mut(target) else {
                continue;
            };
            if matches!(region.state, RegionState::Freed | RegionState::Frozen) {
                continue;
            }
            region.state = RegionState::Freed;
            region.generation += 1;
            region.depth = 0;
            freed.push(target);
        }
        self.open.retain(|open| !freed.contains(open));
        // Every pool the freed regions held dies with them: one operation, not
        // a per-allocation free.
        for pool in &mut self.pools {
            if freed.contains(&pool.region) {
                for slot in &mut pool.slots {
                    slot.life = SlotLife::Free;
                    slot.generation += 1;
                    slot.value = Value::Unit;
                }
            }
        }
        freed
    }

    /// Promotes a region and everything it owns to `imm`, deep and in place
    /// (`[mem.region.freeze.1]`).
    ///
    /// # Errors
    ///
    /// The reason, when the region (or a region it owns) is open —
    /// `[mem.region.freeze.3]`: "the forest transfers as closed subtrees only".
    pub fn freeze(&mut self, id: RegionId) -> Result<Vec<RegionId>, String> {
        if self.state(id) == Some(RegionState::Freed) {
            return Err(format!("{} was freed wholesale", self.label(id)));
        }
        let family = self.descendants(id);
        if let Some(open) = family
            .iter()
            .copied()
            .find(|target| self.regions.get(*target).is_some_and(|r| r.depth > 0))
        {
            return Err(if open == id {
                format!(
                    "{} is open here; a region is frozen or transferred as a closed subtree",
                    self.label(id)
                )
            } else {
                format!(
                    "{} owns the open {}; the forest transfers as closed subtrees only",
                    self.label(id),
                    self.label(open)
                )
            });
        }
        let mut frozen = Vec::new();
        for target in family {
            if let Some(region) = self.regions.get_mut(target)
                && region.state != RegionState::Frozen
            {
                region.state = RegionState::Frozen;
                // Frozen data is readable from any task forever
                // (`[conc.mm.hb.freeze]`), so a transfer claim ends here.
                region.claim = None;
                frozen.push(target);
            }
        }
        Ok(frozen)
    }

    /// `id` and every region it owns, transitively.
    #[must_use]
    pub fn descendants(&self, id: RegionId) -> Vec<RegionId> {
        let mut out = vec![id];
        let mut at = 0usize;
        while at < out.len() {
            let parent = out[at];
            for region in &self.regions {
                if region.parent == Some(parent) && !out.contains(&region.id) {
                    out.push(region.id);
                }
            }
            at += 1;
        }
        out
    }

    // -- the edge table (§3) -----------------------------------------------

    /// Classifies a store of `reference` into data owned by `into`.
    ///
    /// `into` is `None` for a **stack** destination — `[mem.model.machine]`
    /// says an allocation's owner is "a region (or *stack* for locals)", and
    /// §3's table is about stores into *region data*. A local may name any
    /// region; that is what makes a region value first-class.
    #[must_use]
    pub fn classify_edge(&self, into: Option<RegionId>, reference: Ref) -> Edge {
        let target = match reference {
            Ref::Shared(_) | Ref::Weak(_) => return Edge::Tier2,
            Ref::Region(id) => id,
            Ref::Pool(id) | Ref::Handle(id) => match self.pools.get(id) {
                Some(pool) => pool.region,
                None => return Edge::Illegal(format!("pool #{id} does not exist")),
            },
        };
        if self.state(target) == Some(RegionState::Frozen) {
            // `[mem.region.edge.imm]`: frozen data may be referenced from
            // anywhere, forever.
            return Edge::Imm;
        }
        let Some(source) = into else {
            return Edge::Intra;
        };
        if source == target {
            return Edge::Intra;
        }
        if let Ref::Region(child) = reference {
            return match self.regions.get(child).and_then(|r| r.parent) {
                None => Edge::Iso(child),
                Some(owner) if owner == source => Edge::Iso(child),
                Some(owner) => Edge::Illegal(format!(
                    "{} already has an owning edge from {}; a region has at most one owner",
                    self.label(child),
                    self.label(owner)
                )),
            };
        }
        Edge::Illegal(format!(
            "a reference from {} into {} is neither intra-region, nor an owning `iso` edge, \
             nor into frozen data",
            self.label(source),
            self.label(target)
        ))
    }

    /// Records an owning edge (`[mem.region.edge.iso]`), the ≤1-owner
    /// invariant made a write.
    pub fn adopt(&mut self, parent: RegionId, child: RegionId) {
        if let Some(region) = self.regions.get_mut(child) {
            region.parent = Some(parent);
        }
    }

    /// The forest invariant, re-walked: ≤1 parent each (by construction), no
    /// region its own ancestor, no dangling parent.
    ///
    /// > After every mutation, a debug assertion re-walks the region graph and
    /// > asserts the forest invariant — O(heap) per store is fine, this is a
    /// > reference interpreter.
    ///
    /// # Errors
    ///
    /// The broken invariant, rendered.
    pub fn assert_forest(&self) -> Result<(), String> {
        for region in &self.regions {
            let Some(parent) = region.parent else {
                continue;
            };
            if parent >= self.regions.len() {
                return Err(format!(
                    "{} names a parent #{parent} that does not exist",
                    region.label()
                ));
            }
            let mut at = Some(parent);
            let mut steps = 0usize;
            while let Some(current) = at {
                if current == region.id {
                    return Err(format!(
                        "{} is its own ancestor: the region graph is not a forest",
                        region.label()
                    ));
                }
                steps += 1;
                if steps > self.regions.len() {
                    return Err(format!(
                        "the parent chain above {} does not terminate",
                        region.label()
                    ));
                }
                at = self.regions[current].parent;
            }
        }
        Ok(())
    }

    /// Every region still holding memory at program exit.
    ///
    /// The sprint's leak assertion, and is06's crash-cleanup oracle reads it:
    /// a clean exit must free every region. `Frozen` regions are not leaks —
    /// `[mem.region.freeze.1]` makes them immutable *forever* by construction,
    /// so "never freed" is their specified end state.
    #[must_use]
    pub fn leaks(&self) -> Vec<RegionId> {
        self.regions
            .iter()
            .filter(|region| matches!(region.state, RegionState::Open | RegionState::Suspended))
            .map(|region| region.id)
            .collect()
    }

    /// Every region, for reports and the `--trace=mem` summary.
    #[must_use]
    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    // -- pools & handles (§4) ----------------------------------------------

    /// Builds a pool in the current region (`[mem.shared.handle.1]`).
    pub fn new_pool(&mut self, elem: String) -> PoolId {
        let region = self.charge();
        let id = self.pools.len();
        self.pools.push(Pool {
            id,
            region,
            elem,
            slots: Vec::new(),
        });
        id
    }

    /// Every pool, in creation order — the REPL's `:mem` dump reads them
    /// (`[mem.shared.handle.1]`: slots and generations are visible state).
    #[must_use]
    pub fn pools(&self) -> &[Pool] {
        &self.pools
    }

    #[must_use]
    pub fn pool(&self, id: PoolId) -> Option<&Pool> {
        self.pools.get(id)
    }

    /// `reserve()`: a handle to uninitialized storage. Freed slots are reused
    /// and their generation bumps on reuse (`[mem.shared.handle.2]`).
    ///
    /// Returns `(index, generation)`.
    pub fn reserve(&mut self, id: PoolId) -> Option<(usize, u32)> {
        let pool = self.pools.get_mut(id)?;
        if let Some(index) = pool
            .slots
            .iter()
            .position(|slot| slot.life == SlotLife::Free)
        {
            let slot = &mut pool.slots[index];
            slot.life = SlotLife::Reserved;
            slot.value = Value::Unit;
            return Some((index, slot.generation));
        }
        let index = pool.slots.len();
        pool.slots.push(PoolSlot {
            generation: 0,
            life: SlotLife::Reserved,
            value: Value::Unit,
        });
        Some((index, 0))
    }

    /// The slot a handle names, when the handle is not stale.
    #[must_use]
    pub fn slot(&self, id: PoolId, index: usize, generation: u32) -> Option<&PoolSlot> {
        let slot = self.pools.get(id)?.slots.get(index)?;
        (slot.generation == generation && slot.life != SlotLife::Free).then_some(slot)
    }

    /// Fills a reserved slot (`init`, phase two).
    pub fn init_slot(&mut self, id: PoolId, index: usize, generation: u32, value: Value) -> bool {
        let Some(pool) = self.pools.get_mut(id) else {
            return false;
        };
        let Some(slot) = pool.slots.get_mut(index) else {
            return false;
        };
        if slot.generation != generation || slot.life == SlotLife::Free {
            return false;
        }
        slot.life = SlotLife::Live;
        slot.value = value;
        true
    }

    /// Frees a slot and bumps its generation, which is what makes every
    /// outstanding handle to it stale (`[mem.shared.handle.2]`).
    pub fn remove_slot(&mut self, id: PoolId, index: usize, generation: u32) -> bool {
        let Some(pool) = self.pools.get_mut(id) else {
            return false;
        };
        let Some(slot) = pool.slots.get_mut(index) else {
            return false;
        };
        if slot.generation != generation || slot.life == SlotLife::Free {
            return false;
        }
        slot.life = SlotLife::Free;
        slot.generation += 1;
        slot.value = Value::Unit;
        true
    }

    /// The region a pool lives in — the one whose state gates every access.
    #[must_use]
    pub fn pool_region(&self, id: PoolId) -> Option<RegionId> {
        self.pools.get(id).map(|pool| pool.region)
    }

    // -- `shared` cells (§4) -----------------------------------------------

    pub fn new_cell(&mut self, value: Value, span: Span) -> CellId {
        let id = self.cells.len();
        self.cells.push(Cell {
            strong: 1,
            weak: 0,
            value,
            edges: Vec::new(),
            dead: false,
            span,
        });
        id
    }

    #[must_use]
    pub fn cell(&self, id: CellId) -> Option<&Cell> {
        self.cells.get(id)
    }

    /// `clone()`: one more strong owner (`[mem.shared.rc.1]`).
    pub fn retain(&mut self, id: CellId) -> bool {
        match self.cells.get_mut(id) {
            Some(cell) if !cell.dead => {
                cell.strong += 1;
                true
            }
            _ => false,
        }
    }

    /// A strong owner goes away; the payload drops at zero
    /// (`[mem.shared.drop.3]`: "runs it when the last strong count drops, at
    /// that release point"). Returns whether this release was the last.
    pub fn release(&mut self, id: CellId) -> bool {
        let Some(cell) = self.cells.get_mut(id) else {
            return false;
        };
        cell.strong = cell.strong.saturating_sub(1);
        if cell.strong == 0 && !cell.dead {
            cell.dead = true;
            cell.value = Value::Unit;
            cell.edges.clear();
            return true;
        }
        false
    }

    /// `downgrade()` — a weak edge does not keep the value alive
    /// (`[mem.shared.rc.3]`).
    pub fn downgrade(&mut self, id: CellId) -> bool {
        match self.cells.get_mut(id) {
            Some(cell) => {
                cell.weak += 1;
                true
            }
            None => false,
        }
    }

    pub fn drop_weak(&mut self, id: CellId) {
        if let Some(cell) = self.cells.get_mut(id) {
            cell.weak = cell.weak.saturating_sub(1);
        }
    }

    /// `upgrade()` — `Some` while a strong owner remains, `None` once the
    /// payload is gone.
    pub fn upgrade(&mut self, id: CellId) -> bool {
        match self.cells.get_mut(id) {
            Some(cell) if !cell.dead && cell.strong > 0 => {
                cell.strong += 1;
                true
            }
            _ => false,
        }
    }

    /// Would adding the strong edge `from → to` close a strong cycle?
    ///
    /// `[mem.shared.rc.2]` makes this a **compile** error (E1006); the dynamic
    /// machine asserts it instead, because the closed trap vocabulary
    /// (`[conf.trap.set]`) has no kind for it and inventing one would be this
    /// implementation legislating. See `docs/approximation-contract.md`.
    #[must_use]
    pub fn would_cycle(&self, from: CellId, to: CellId) -> bool {
        if from == to {
            return true;
        }
        let mut seen = vec![to];
        let mut at = 0usize;
        while at < seen.len() {
            let Some(cell) = self.cells.get(seen[at]) else {
                at += 1;
                continue;
            };
            for next in &cell.edges {
                if *next == from {
                    return true;
                }
                if !seen.contains(next) {
                    seen.push(*next);
                }
            }
            at += 1;
        }
        false
    }

    /// Records a strong edge between cells, for [`Store::would_cycle`].
    pub fn add_strong_edge(&mut self, from: CellId, to: CellId) {
        if let Some(cell) = self.cells.get_mut(from)
            && !cell.edges.contains(&to)
        {
            cell.edges.push(to);
        }
    }

    /// Every `shared` cell still holding its payload at program exit.
    #[must_use]
    pub fn live_cells(&self) -> Vec<CellId> {
        self.cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| !cell.dead && cell.strong > 0)
            .map(|(id, _)| id)
            .collect()
    }
}

/// Every reference granule inside a value, in traversal order.
///
/// §3's table is about "a store of a reference": under MVS the aggregate is
/// the store and the references are what it *carries*, so the edge check walks
/// the stored value for granules rather than asking about the value as a whole.
pub fn references(value: &Value, out: &mut Vec<Ref>) {
    match value {
        Value::Region(handle) => out.push(Ref::Region(handle.id)),
        Value::PoolRef(id) => out.push(Ref::Pool(*id)),
        Value::Handle(handle) => out.push(Ref::Handle(handle.pool)),
        Value::Shared(id) => out.push(Ref::Shared(*id)),
        Value::Weak(id) => out.push(Ref::Weak(*id)),
        Value::Tuple(items) | Value::List(items, _) => {
            for item in items {
                references(&item.value, out);
            }
        }
        Value::Struct { fields, .. } => {
            for (_, slot) in fields {
                references(&slot.value, out);
            }
        }
        Value::Map(pairs) => {
            for (key, slot) in pairs {
                references(key, out);
                references(&slot.value, out);
            }
        }
        Value::Error(err) => {
            for item in &err.payload {
                references(item, out);
            }
        }
        Value::Unit
        | Value::Bool(_)
        | Value::Int(..)
        | Value::Float(_)
        | Value::Str(_)
        | Value::Range { .. }
        | Value::Fn(_)
        | Value::Closure(_)
        | Value::Module(_)
        | Value::Builtin(_)
        // A raw pointer is not a §3 *reference*: `[mem.unsafe.raw.1]` gives it
        // no aliasing obligations, and the edge table is about the granules the
        // safe tier can name. Its obligations are §6's, and they are checked
        // there (`super::prov`).
        | Value::Raw(_)
        // The concurrency granules are scheduler-owned identities, not region
        // storage: a channel endpoint or scope handle stores nothing in a
        // region, and cross-task transfer of *regions* is checked at the
        // `send` itself (spec/03 §3, is06), not by §3's edge table.
        | Value::Scope(_)
        | Value::Chan(_)
        | Value::MutexRef(_)
        | Value::Proc(_)
        | Value::Duration(_) => {}
    }
}

/// The reference granules a value carries.
#[must_use]
pub fn refs_of(value: &Value) -> Vec<Ref> {
    let mut out = Vec::new();
    references(value, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::new()
    }

    #[test]
    fn the_program_region_is_open_and_ambient() {
        let store = store();
        assert_eq!(store.current(), Store::root());
        assert_eq!(store.state(Store::root()), Some(RegionState::Open));
    }

    #[test]
    fn allocations_land_in_the_innermost_open_region() {
        // `[mem.region.create.3]`, operationally.
        let mut store = store();
        let a = store.create(Some("a".to_owned()), Strategy::Arena, Span::new(0, 1));
        assert_eq!(store.charge(), Store::root());
        store.enter(a).expect("a opens");
        assert_eq!(store.charge(), a);
        assert_eq!(store.region(a).expect("a").allocations, 1);
        store.leave(a);
        assert_eq!(store.current(), Store::root());
        assert_eq!(store.state(a), Some(RegionState::Suspended));
    }

    #[test]
    fn disjoint_regions_open_simultaneously() {
        // `[mem.region.multiopen]`, the relaxation this interpreter exists to
        // test: two sibling roots, both open.
        let mut store = store();
        let a = store.create(None, Strategy::Arena, Span::new(0, 1));
        let b = store.create(None, Strategy::Arena, Span::new(2, 3));
        store.enter(a).expect("a opens");
        store.enter(b).expect("b opens beside a");
        assert_eq!(store.open_set(), &[Store::root(), a, b]);
        assert_eq!(store.current(), b);
    }

    #[test]
    fn an_ancestor_and_its_descendant_do_not_open_together() {
        // The finding: "distinct region values" is not "disjoint regions".
        let mut store = store();
        let parent = store.create(None, Strategy::Arena, Span::new(0, 1));
        let child = store.create(None, Strategy::Arena, Span::new(2, 3));
        store.adopt(parent, child);
        store.enter(parent).expect("the parent opens");
        let refused = store.enter(child).expect_err("the child must not join it");
        assert!(matches!(refused, EnterError::NotDisjoint(_)), "{refused:?}");
        assert!(refused.reason().contains("not disjoint"), "{refused:?}");
    }

    #[test]
    fn reopening_an_open_region_is_idempotent() {
        // What `corpus/memory/region_multiopen_swap.lu` requires and
        // `[mem.region.open.1]` does not say.
        let mut store = store();
        let a = store.create(Some("a".to_owned()), Strategy::Arena, Span::new(0, 1));
        store.enter(a).expect("a opens");
        store
            .enter(a)
            .expect("`in a` inside `region a { … }` re-opens it");
        assert_eq!(store.region(a).expect("a").depth, 2);
        store.leave(a);
        assert_eq!(store.state(a), Some(RegionState::Open));
        store.leave(a);
        assert_eq!(store.state(a), Some(RegionState::Suspended));
    }

    #[test]
    fn freeing_is_wholesale_and_recursive_and_idempotent() {
        let mut store = store();
        let parent = store.create(None, Strategy::Arena, Span::new(0, 1));
        let child = store.create(None, Strategy::Arena, Span::new(2, 3));
        store.adopt(parent, child);
        let freed = store.free(parent);
        assert_eq!(freed, vec![parent, child]);
        assert_eq!(store.state(child), Some(RegionState::Freed));
        assert_eq!(store.generation(child), 1);
        assert!(store.free(parent).is_empty(), "freeing twice frees nothing");
    }

    #[test]
    fn a_frozen_region_is_immutable_forever_and_never_reopens() {
        let mut store = store();
        let a = store.create(None, Strategy::Arena, Span::new(0, 1));
        store.freeze(a).expect("a closed region freezes");
        assert_eq!(store.state(a), Some(RegionState::Frozen));
        assert!(store.enter(a).is_err());
        // A freeze does not free: `[mem.region.freeze.1]`'s "forever".
        assert!(store.free(a).is_empty());
        assert_eq!(store.state(a), Some(RegionState::Frozen));
    }

    #[test]
    fn a_subtree_with_an_open_child_neither_freezes_nor_transfers() {
        // `[mem.region.freeze.3]`, E1005's dynamic half.
        let mut store = store();
        let parent = store.create(None, Strategy::Arena, Span::new(0, 1));
        let child = store.create(None, Strategy::Arena, Span::new(2, 3));
        store.adopt(parent, child);
        store.enter(child).expect("the child opens");
        let refused = store.freeze(parent).expect_err("an open child blocks it");
        assert!(refused.contains("closed subtrees only"), "{refused}");
    }

    #[test]
    fn the_edge_table_reads_as_the_document_writes_it() {
        let mut store = store();
        let a = store.create(None, Strategy::Arena, Span::new(0, 1));
        let b = store.create(None, Strategy::Arena, Span::new(2, 3));
        let frozen = store.create(None, Strategy::Arena, Span::new(4, 5));
        store.freeze(frozen).expect("freezes");
        let pool_in_a = {
            store.enter(a).expect("a opens");
            let id = store.new_pool("Node".to_owned());
            store.leave(a);
            id
        };

        // same region ✅, into `imm` ✅, Tier-2 cell ✅, stack destination ✅
        assert_eq!(
            store.classify_edge(Some(a), Ref::Handle(pool_in_a)),
            Edge::Intra
        );
        assert_eq!(store.classify_edge(Some(b), Ref::Region(frozen)), Edge::Imm);
        assert_eq!(store.classify_edge(Some(b), Ref::Shared(0)), Edge::Tier2);
        assert_eq!(
            store.classify_edge(None, Ref::Handle(pool_in_a)),
            Edge::Intra
        );

        // other region ❌ — E1004's dynamic half.
        assert!(matches!(
            store.classify_edge(Some(b), Ref::Handle(pool_in_a)),
            Edge::Illegal(_)
        ));

        // child region via the owning handle ✅, and only once.
        assert_eq!(store.classify_edge(Some(b), Ref::Region(a)), Edge::Iso(a));
        store.adopt(b, a);
        assert_eq!(store.classify_edge(Some(b), Ref::Region(a)), Edge::Iso(a));
        assert!(matches!(
            store.classify_edge(Some(frozen), Ref::Region(a)),
            Edge::Imm | Edge::Illegal(_)
        ));
    }

    #[test]
    fn a_planted_cycle_trips_the_forest_assertion() {
        // The sprint's "a planted invariant-breaking mutation demonstrably
        // trips it".
        let mut store = store();
        let a = store.create(None, Strategy::Arena, Span::new(0, 1));
        let b = store.create(None, Strategy::Arena, Span::new(2, 3));
        store.adopt(a, b);
        assert_eq!(store.assert_forest(), Ok(()));
        store.adopt(b, a);
        let broken = store.assert_forest().expect_err("a cycle is not a forest");
        assert!(broken.contains("own ancestor"), "{broken}");
    }

    #[test]
    fn handles_are_generational_and_slots_are_reused() {
        let mut store = store();
        let pool = store.new_pool("Node".to_owned());
        let (index, generation) = store.reserve(pool).expect("reserve");
        assert!(store.init_slot(pool, index, generation, Value::int(1)));
        assert!(store.slot(pool, index, generation).is_some());
        assert!(store.remove_slot(pool, index, generation));
        // The outstanding handle is now stale, exactly.
        assert!(store.slot(pool, index, generation).is_none());
        // Reuse bumps, it does not reset.
        let (again, next) = store.reserve(pool).expect("reuse");
        assert_eq!(again, index);
        assert_eq!(next, generation + 1);
    }

    #[test]
    fn freeing_a_pool_region_kills_every_slot_in_it() {
        let mut store = store();
        let r = store.create(None, Strategy::Pool("Node".to_owned()), Span::new(0, 1));
        store.enter(r).expect("opens");
        let pool = store.new_pool("Node".to_owned());
        let (index, generation) = store.reserve(pool).expect("reserve");
        store.init_slot(pool, index, generation, Value::int(1));
        store.leave(r);
        store.free(r);
        assert!(store.slot(pool, index, generation).is_none());
    }

    #[test]
    fn refcounts_are_naive_and_weak_edges_do_not_keep_anything_alive() {
        let mut store = store();
        let cell = store.new_cell(Value::int(7), Span::new(0, 1));
        assert!(store.retain(cell));
        assert!(store.downgrade(cell));
        assert!(!store.release(cell), "one owner remains");
        assert!(store.upgrade(cell), "upgrade while alive");
        assert!(!store.release(cell));
        assert!(store.release(cell), "the last strong release drops it");
        assert!(!store.upgrade(cell), "a dead weak upgrades to nothing");
    }

    #[test]
    fn the_acyclicity_assertion_sees_a_strong_cycle_forming() {
        let mut store = store();
        let a = store.new_cell(Value::int(1), Span::new(0, 1));
        let b = store.new_cell(Value::int(2), Span::new(2, 3));
        assert!(!store.would_cycle(a, b));
        store.add_strong_edge(a, b);
        assert!(store.would_cycle(b, a), "b → a closes the loop");
        assert!(store.would_cycle(a, a), "a self edge is a cycle");
    }

    #[test]
    fn leaks_are_regions_neither_freed_nor_frozen() {
        let mut store = store();
        let leaked = store.create(None, Strategy::Arena, Span::new(0, 1));
        let frozen = store.create(None, Strategy::Arena, Span::new(2, 3));
        let freed = store.create(None, Strategy::Arena, Span::new(4, 5));
        store.freeze(frozen).expect("freezes");
        store.free(freed);
        store.free(Store::root());
        assert_eq!(store.leaks(), vec![leaked]);
    }
}

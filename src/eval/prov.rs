//! The provenance machine — `spec/02-memory-model.md` §6 and §7, executable.
//!
//! # What this is
//!
//! `[mem.model.machine]` names four components of the abstract machine state;
//! [`super::region::Store`] owns components 3 and 4, and this module owns
//! component 2: the **provenance forest**. Every pointer value carries a tag
//! ([`Prov`]); every allocation carries a *tree* of tags rooted at the tag its
//! creator got ([`Alloc::root`]); a new tag is a child of the tag it derives
//! from (`[mem.prov.tag]`). Each tag holds a permission **per location**
//! (`[mem.prov.state]` says "per allocation range"; a per-byte vector is the
//! finest reading of that and the one a reference interpreter should take).
//!
//! The lineage is Stacked Borrows, the shape is Tree Borrows, and the shape is
//! the point: a *tree* tolerates the two-phase and pointer-arithmetic patterns
//! a stack rejects, which is where reports/01's −54% false-rejection number
//! comes from. `corpus/memory/prov_two_phase.lu` is that number as a program.
//!
//! # The transition table, verbatim
//!
//! `[mem.prov.state]`:
//!
//! | state | child read | child write | foreign read | foreign write |
//! |---|---|---|---|---|
//! | Reserved | ok (stays Reserved) | → Active | ok | → Disabled |
//! | Active | ok | ok | → Frozen | → Disabled |
//! | Frozen | ok | **UB §7/P2** | ok | → Disabled |
//! | Disabled | **UB §7/P1** | **UB §7/P1** | ok | ok |
//!
//! Upstream `8b04edf` repaired the Reserved row: the original table *activated
//! on child reads* by transcription error, which closed the two-phase window
//! one access early (a read between the retag and the first write flipped the
//! tag to Active, so a subsequent foreign read froze it — published Tree
//! Borrows keeps Reserved until the first child **write**). This machine
//! implemented the bugged table verbatim, as it must; it implements the
//! repaired one now, for the same reason.
//!
//! "child" is the clause's own definition — *access through this tag or a
//! descendant* — so for a node `n` and an access through `t`, the access is a
//! child access iff `n` is `t` or an ancestor of `t`. Everything else is
//! foreign, **including** an access through `n`'s own parent, which is what
//! makes a reborrow invalidate its children.
//!
//! **Protectors.** `[mem.prov.state]`: "Protected tags escalate the
//! foreign-write transition to immediate UB for the protection's duration."
//! That single sentence is the whole of D3's `mut`-noalias call-extent fact:
//! a `mut`/`read` parameter's tag is protected for the call, so a foreign write
//! during the call is UB *at the write* rather than a later use of a Disabled
//! tag. See `docs/approximation-contract.md` §7.2 for the row this lands on and
//! why the enumeration is thin there.
//!
//! # Wildcards are the raw tier
//!
//! `[mem.prov.expose]`: an int→ptr cast produces a pointer whose provenance is
//! "resolved angelically among exposed tags (a defined execution is chosen if
//! one exists)". [`Provenance::access`] implements *angelically* literally: it
//! snapshots the allocation, tries each exposed tag in turn, and commits the
//! first candidate that keeps the execution defined. Raw code therefore commits
//! provenance UB only by touching an allocation with no exposed tags or a dead
//! one — which is `[mem.unsafe.raw.1]`'s "no aliasing assumptions" made
//! operational, and D11's "simpler than safe" made checkable.

use std::collections::BTreeMap;
use std::fmt;

use super::region::RegionId;
use super::rules::Rule;
use crate::diag::Span;

pub type AllocId = usize;
pub type TagId = usize;

// ---------------------------------------------------------------------------
// §7 — the closed UB enumeration
// ---------------------------------------------------------------------------

/// One row of `[mem.ub]`'s closed enumeration.
///
/// The table is **closed** ("behavior not listed here is defined … Adding UB
/// requires a spec amendment naming its licensed optimization"), so this enum
/// is closed too, and `tests/ub_coverage.rs` walks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UbRow {
    P1,
    P2,
    P3,
    P4,
    P5,
    P6,
    L1,
    L2,
    T1,
    T2,
    C1,
}

/// Whether this machine detects a row, and if not, why not.
///
/// `[proto.record.unsupported]`'s honesty contract applied to the coverage
/// matrix: a row this machine cannot reach is *listed* as unreachable with the
/// reason, never silently absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Implemented here and exercised by a program.
    Detected,
    /// spec/03's model, ic03's machine. The sprint's `deferred(concurrency)`.
    DeferredConcurrency,
    /// Reachable in principle, not with the surface this tier has.
    Unreachable(&'static str),
}

impl UbRow {
    /// Every row, in `[mem.ub]`'s order.
    pub const ALL: [UbRow; 11] = [
        UbRow::P1,
        UbRow::P2,
        UbRow::P3,
        UbRow::P4,
        UbRow::P5,
        UbRow::P6,
        UbRow::L1,
        UbRow::L2,
        UbRow::T1,
        UbRow::T2,
        UbRow::C1,
    ];

    /// The row id, as it appears on `x-ub-row` (`[proto.record.ub]`).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            UbRow::P1 => "P1",
            UbRow::P2 => "P2",
            UbRow::P3 => "P3",
            UbRow::P4 => "P4",
            UbRow::P5 => "P5",
            UbRow::P6 => "P6",
            UbRow::L1 => "L1",
            UbRow::L2 => "L2",
            UbRow::T1 => "T1",
            UbRow::T2 => "T2",
            UbRow::C1 => "C1",
        }
    }

    /// The row's own words.
    #[must_use]
    pub const fn what(self) -> &'static str {
        match self {
            // 8b04edf named the protector escalation in the row itself; this
            // machine always landed protected foreign writes on P1, and the
            // row's words now say so rather than leaving it to the state table.
            UbRow::P1 => {
                "access through a Disabled tag (use-after-free, use of an invalidated borrow), \
                 or a foreign write to a protected tag"
            }
            UbRow::P2 => "write through a Frozen tag",
            UbRow::P3 => "access outside an allocation's bounds",
            UbRow::P4 => "access to an allocation whose region was freed",
            UbRow::P5 => "violated `assume noalias p, q`",
            UbRow::P6 => "false discharge of a re-entry door",
            UbRow::L1 => "read of uninitialized or moved-from memory via raw pointers",
            UbRow::L2 => "deref of a dangling raw pointer",
            UbRow::T1 => "producing an invalid value of a restricted type in unsafe code",
            UbRow::T2 => {
                "torn write producing a partially-updated wide value observed through another tag"
            }
            UbRow::C1 => "data race on non-atomic memory reachable only from unsafe/FFI code",
        }
    }

    /// The optimization the row licenses — **the D2 pairing, executable**.
    ///
    /// `[mem.ub.closed]`: "zero rows without a named optimization is an
    /// invariant of this table". Carrying the pairing here means a UB report
    /// can print *why the language pays for this rule*, which is the whole
    /// point of D2.
    #[must_use]
    pub const fn optimization(self) -> &'static str {
        match self {
            UbRow::P1 => {
                "O1: `mut` params lower to `noalias` + `dereferenceable`; unique-tag stores forward without memory checks"
            }
            UbRow::P2 => {
                "O2: `read` params are immutable-for-the-call — loads hoist/CSE across opaque calls (the SB \"holy grail\"); `imm` data const-propagates"
            }
            UbRow::P3 => {
                "O3a: `dereferenceable(n)` on known-size accesses; bounds-based alias disproof between distinct allocations"
            }
            UbRow::P4 => {
                "O3b: one alias-scope domain per region — pointers into distinct regions never alias; O4: regions not open in the current scope yield `invariant.load`"
            }
            UbRow::P5 => {
                "O5: the asserted ranges get `noalias` treatment in Tier-3 code — vectorization/reordering as if proven"
            }
            UbRow::P6 => {
                "O6: safe-tier code after the door keeps all safe-tier entitlements (O1–O4) — the door is where trust concentrates"
            }
            UbRow::L1 => {
                "O7: moves lower to memcpy-and-forget; dead-store elimination on moved-from places; no zero-init of locals"
            }
            UbRow::L2 => {
                "O8: escape analysis / stack promotion without conservatively pinning addresses"
            }
            UbRow::T1 => {
                "O9: niche packing; match jump tables without default arms; UTF-8 fast paths without re-validation"
            }
            UbRow::T2 => {
                "O10: layout freedom — field reorder, no address identity for value fields outside `#[repr(c)]`; wide stores split freely"
            }
            UbRow::C1 => "spec/03 §(DRF-SC): sync-free stretches permit store motion/combining",
        }
    }

    /// The most specific clause this machine can cite for the row.
    ///
    /// The *verdict* always carries `mem.ub` (`[proto.record.ub]`'s own
    /// example); this rides `x-ub-clause`, where a second implementation is
    /// free to disagree without producing a spurious divergence
    /// (`[proto.cmp.defined-divergence]`: `x-` keys absent on one side).
    #[must_use]
    pub const fn clause(self) -> &'static str {
        match self {
            UbRow::P1 | UbRow::P2 => "mem.prov.state",
            UbRow::P4 => "mem.prov.region",
            UbRow::P5 => "mem.unsafe.raw.2",
            UbRow::P6 => "mem.unsafe.door",
            UbRow::L2 => "mem.unsafe.raw.1",
            UbRow::C1 => "conc.mm.race.3",
            UbRow::P3 | UbRow::L1 | UbRow::T1 | UbRow::T2 => "mem.ub",
        }
    }

    /// The rule this machine fires when the row is reported.
    #[must_use]
    pub const fn rule(self) -> Rule {
        match self {
            UbRow::P1 | UbRow::P2 => Rule::ProvState,
            UbRow::P4 => Rule::ProvRegion,
            UbRow::P5 => Rule::AssumeNoalias,
            UbRow::P6 => Rule::UnsafeDoor,
            UbRow::L2 => Rule::UnsafeRaw,
            UbRow::P3 | UbRow::L1 | UbRow::T1 | UbRow::T2 | UbRow::C1 => Rule::Ub,
        }
    }

    /// Whether this machine detects the row, and the reason when it does not.
    #[must_use]
    pub const fn coverage(self) -> Coverage {
        match self {
            UbRow::P1
            | UbRow::P2
            | UbRow::P3
            | UbRow::P4
            | UbRow::P5
            | UbRow::P6
            | UbRow::L1
            | UbRow::L2 => Coverage::Detected,
            // T1's only modelled production door was `int as bool`, and the
            // cast matrix's bool column closed statically at pin f0da6e6
            // (E0805 at resolve, issue #18 item 2 — the counterparty rejects
            // the cast in unsafe code too, observed on the retired trigger).
            // The detection logic stays (`eval_cast`'s §7/T1 path still
            // reports the row for a frontend-bypassing caller), but no source
            // program reaches it: an invalid bool now needs a Tier-3
            // typed-raw door this machine does not model.
            UbRow::T1 => Coverage::Unreachable(
                "the one production door this machine modelled, `int as bool` in unsafe code, is \
                 statically outside the language since the cast matrix's bool column closed \
                 (E0805 at resolve, pin f0da6e6); an invalid bool now needs a Tier-3 typed-raw \
                 door that is not modelled here — the detection logic remains for callers that \
                 bypass the frontend",
            ),
            // A tear needs a *second observer* of a wide store mid-flight.
            // This machine executes one thread with atomic whole-value stores
            // (`[mem.model.value]`: a store transfers the whole value), and the
            // language surface has no split/volatile wide-store form to spell
            // one with. ic03's schedules make T2 reachable; until then it is
            // listed, not silently absent.
            UbRow::T2 => Coverage::Unreachable(
                "a torn write needs a second observer of a wide store in flight; this machine is \
                 single-threaded and every store is whole-value, so no execution reaches the row \
                 (ic03's interleavings make it reachable)",
            ),
            UbRow::C1 => Coverage::DeferredConcurrency,
        }
    }
}

impl fmt::Display for UbRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// One UB finding: the two spans and the tree dump the sprint asks for.
///
/// > The report carries the item id, the access span, the tag-creation span,
/// > and a rendered slice of the borrow tree at the violation — the
/// > two-spans-plus-tree dump that makes provenance bugs debuggable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UbFinding {
    pub row: UbRow,
    /// Where the offending access happened.
    pub span: Span,
    /// Where the tag it went through (or the tag it violated) was created.
    pub tag_span: Option<Span>,
    pub message: String,
    /// The borrow tree at the violation, one line per tag.
    pub tree: Vec<String>,
}

impl UbFinding {
    /// The verdict anchor. `[proto.record.ub]`'s own example is `ub(mem.ub)`,
    /// with the row id on `x-ub-row`; the specific clause rides `x-ub-clause`
    /// so the two implementations compare the *row*, not each other's taste in
    /// anchors.
    #[must_use]
    pub const fn anchor(&self) -> &'static str {
        "mem.ub"
    }
}

impl fmt::Display for UbFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ub({}) §7/{}: {} [{}] at {}",
            self.anchor(),
            self.row,
            self.message,
            self.row.clause(),
            self.span
        )?;
        if let Some(span) = self.tag_span {
            write!(f, "; tag created at {span}")?;
        }
        write!(f, "\n  licenses {}", self.row.optimization())?;
        for line in &self.tree {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The machine state
// ---------------------------------------------------------------------------

/// A tag's permission at one location (`[mem.prov.state]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perm {
    /// Created, unused — the two-phase window.
    Reserved,
    /// Live unique/mutable.
    Active,
    /// Shared/read-only.
    Frozen,
    /// Dead.
    Disabled,
}

impl fmt::Display for Perm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Perm::Reserved => "Reserved",
            Perm::Active => "Active",
            Perm::Frozen => "Frozen",
            Perm::Disabled => "Disabled",
        })
    }
}

/// What a pointer value carries (`[mem.prov.tag]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prov {
    Tag(TagId),
    /// `[mem.prov.expose]`: from an int→ptr cast or the FFI boundary. Resolved
    /// angelically among the allocation's exposed tags.
    Wildcard,
}

impl fmt::Display for Prov {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Prov::Tag(id) => write!(f, "tag#{id}"),
            Prov::Wildcard => f.write_str("wildcard"),
        }
    }
}

/// A raw pointer value: `(address, tag)` with the address split into the
/// allocation it names and a byte offset, because a reference interpreter that
/// invented numeric addresses would be inventing the one thing
/// `[proto.cmp.defined-divergence]` says is never compared (layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawPtr {
    /// `None` is the null pointer.
    pub alloc: Option<AllocId>,
    pub offset: i128,
    pub prov: Prov,
    /// The pointee's size in bytes — `*u8` is 1, `*int` is 8. Fixes what
    /// `p[i]` addresses and what `assume noalias` compares.
    pub elem: usize,
    /// Whether the pointee is a signed integer, so a load sign-extends.
    pub signed: bool,
}

impl RawPtr {
    #[must_use]
    pub const fn null() -> RawPtr {
        RawPtr {
            alloc: None,
            offset: 0,
            prov: Prov::Wildcard,
            elem: 1,
            signed: false,
        }
    }

    /// The same pointer, `n` pointees along — `p[n]`, and the unrestricted
    /// pointer arithmetic of `[mem.unsafe.raw.1]`.
    #[must_use]
    pub const fn offset_by(self, n: i128) -> RawPtr {
        RawPtr {
            offset: self.offset + n * self.elem as i128,
            ..self
        }
    }

    #[must_use]
    pub const fn is_null(&self) -> bool {
        self.alloc.is_none()
    }
}

impl fmt::Display for RawPtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.alloc {
            None => f.write_str("*null"),
            Some(alloc) => write!(f, "*alloc#{alloc}+{}@{}", self.offset, self.prov),
        }
    }
}

/// Where an allocation came from — which decides who kills it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A `c.malloc` allocation. `[mem.boundary.ffi]` says "a C call executes
    /// against an implicit region borrowed for the call's extent", so it is
    /// still *owned* by the region that was current — see `owner`.
    HostC,
    /// A Tier-0 place: the storage behind a local, a field, an element. Size 1,
    /// because the unit of a Tier-0 place is the place.
    Place,
}

/// One allocation and its tag tree.
#[derive(Debug, Clone)]
pub struct Alloc {
    pub id: AllocId,
    pub size: usize,
    pub bytes: Vec<u8>,
    /// Per-byte initialization, for §7/L1.
    pub init: Vec<bool>,
    pub live: bool,
    pub origin: Origin,
    /// The region that owns it (`[mem.prov.region]`: "freeing a region Disables
    /// every tag tree of every allocation it owns").
    pub owner: Option<RegionId>,
    pub root: TagId,
    pub tags: Vec<TagId>,
    /// `[mem.prov.expose]`'s exposed set — the candidates a wildcard resolves
    /// among.
    pub exposed: Vec<TagId>,
    pub label: String,
    pub span: Span,
}

/// One node of an allocation's borrow tree.
#[derive(Debug, Clone)]
pub struct Tag {
    pub id: TagId,
    pub alloc: AllocId,
    pub parent: Option<TagId>,
    /// One permission per byte of the allocation (`[mem.prov.state]`: "per
    /// allocation range").
    pub perms: Vec<Perm>,
    /// Protected for a call's extent. `[mem.prov.state]`: a protected tag
    /// escalates the foreign-write transition to immediate UB.
    pub protected: bool,
    pub label: String,
    pub span: Span,
}

/// An `assume noalias p, q` assertion in force (`[mem.unsafe.raw.2]`).
#[derive(Debug, Clone)]
pub struct Assumption {
    pub ranges: Vec<(Option<AllocId>, i128, i128)>,
    pub span: Span,
}

/// How memory is being touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    Read,
    Write,
}

impl fmt::Display for AccessKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AccessKind::Read => "read",
            AccessKind::Write => "write",
        })
    }
}

/// How a retag creates its child (`[mem.prov.tag]`'s retag points).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetagKind {
    /// `mut` passing / `&mut` — a Reserved child. Reserved, not Active:
    /// "creation is NOT a use", which is the two-phase window.
    Mutable,
    /// `read` passing / `&` — a Frozen child.
    Shared,
}

impl RetagKind {
    const fn perm(self) -> Perm {
        match self {
            RetagKind::Mutable => Perm::Reserved,
            RetagKind::Shared => Perm::Frozen,
        }
    }
}

/// The provenance forest: component 2 of `[mem.model.machine]`.
#[derive(Debug, Default)]
pub struct Provenance {
    allocs: Vec<Alloc>,
    tags: Vec<Tag>,
    /// Place key (`"<frame>:<path>"`) → the tag reads and writes of that place
    /// go through. A parameter's key maps to the *retagged child*, which is how
    /// a callee's access is a child access and the caller's is foreign.
    places: BTreeMap<String, (AllocId, TagId)>,
    assumptions: Vec<Assumption>,
    /// Regions whose wholesale free this machine has been told about.
    ///
    /// Kept here rather than read back out of the region store, because the
    /// provenance machine must stay usable on its own in tests — and because a
    /// region id is never reused (`Store::create` only appends), so a set of
    /// ids is exact.
    region_dead: Vec<RegionId>,
    /// Allocations whose tag set or bindings changed since the last
    /// [`Provenance::prune`] — the only ones a prune must revisit. An
    /// allocation not in this set has exactly the prunable-set it had after
    /// the last prune (nothing that feeds prunability changed for it), so
    /// walking it again would keep everything it kept. Keyed on: a retag
    /// (new tag), a protector release, and every binding removal or
    /// replacement (a tag that *was* bound becoming unbound). This is what
    /// keeps a prune O(what changed) instead of O(every allocation the
    /// program ever made) — wolf-interp#41 measured the latter as
    /// per-iteration cost growing linearly with program age. A `Vec` with
    /// consecutive-duplicate suppression at the insert and sort+dedup at
    /// the prune, because the insert sits on the per-retag hot path.
    dirty: Vec<AllocId>,
    /// Rule firings waiting for the machine to drain into `--trace`.
    notes: Vec<(Rule, Span, String)>,
}

impl Provenance {
    #[must_use]
    pub fn new() -> Provenance {
        Provenance::default()
    }

    /// Trace lines produced since the last drain.
    pub fn take_notes(&mut self) -> Vec<(Rule, Span, String)> {
        std::mem::take(&mut self.notes)
    }

    fn note(&mut self, rule: Rule, span: Span, detail: String) {
        self.notes.push((rule, span, detail));
    }

    /// Queues an allocation for the next [`Provenance::prune`]. Consecutive
    /// duplicates (the common shape: one place retagged and released per
    /// call) collapse at the push; the prune sorts and dedups the rest.
    fn mark_dirty(&mut self, alloc: AllocId) {
        if self.dirty.last() != Some(&alloc) {
            self.dirty.push(alloc);
        }
    }

    #[must_use]
    pub fn alloc(&self, id: AllocId) -> Option<&Alloc> {
        self.allocs.get(id)
    }

    #[must_use]
    pub fn tag(&self, id: TagId) -> Option<&Tag> {
        self.tags.get(id)
    }

    /// Every allocation, for the leak/liveness reports.
    #[must_use]
    pub fn allocs(&self) -> &[Alloc] {
        &self.allocs
    }

    /// Host allocations that were never freed at program exit — the C-side
    /// half of is03's leak assertion. Leaks are *defined and safe*
    /// (`[mem.ub.defined]`), so this is a report, never a fault.
    #[must_use]
    pub fn live_host_allocs(&self) -> Vec<AllocId> {
        self.allocs
            .iter()
            .filter(|a| a.live && a.origin == Origin::HostC)
            .map(|a| a.id)
            .collect()
    }

    // -- creation ----------------------------------------------------------

    fn new_alloc(
        &mut self,
        size: usize,
        origin: Origin,
        owner: Option<RegionId>,
        label: String,
        span: Span,
        root_perm: Perm,
    ) -> AllocId {
        let id = self.allocs.len();
        let root = self.tags.len();
        self.tags.push(Tag {
            id: root,
            alloc: id,
            parent: None,
            perms: vec![root_perm; size],
            protected: false,
            label: format!("{label}#root"),
            span,
        });
        self.allocs.push(Alloc {
            id,
            size,
            bytes: vec![0; size],
            init: vec![false; size],
            live: true,
            origin,
            owner,
            root,
            tags: vec![root],
            exposed: Vec::new(),
            label,
            span,
        });
        id
    }

    /// A host C allocation (`c.malloc`). Modelled, not linked: see
    /// `docs/approximation-contract.md` §8.
    ///
    /// The root tag is **exposed** on creation, because `[mem.prov.expose]`
    /// says "wildcard pointers from FFI behave as exposed" and a malloc result
    /// is exactly that — the wolf side receives a pointer the C library also
    /// holds.
    pub fn host_alloc(&mut self, size: usize, owner: RegionId, span: Span) -> RawPtr {
        let id = self.new_alloc(
            size,
            Origin::HostC,
            Some(owner),
            format!("c.malloc({size})"),
            span,
            Perm::Active,
        );
        let root = self.allocs[id].root;
        self.allocs[id].exposed.push(root);
        self.note(
            Rule::BoundaryFfi,
            span,
            format!(
                "alloc#{id}: {size} byte(s) from the C heap, owned by region #{owner} for the \
                 call's extent; root tag#{root} exposed"
            ),
        );
        RawPtr {
            alloc: Some(id),
            offset: 0,
            prov: Prov::Tag(root),
            elem: 1,
            signed: false,
        }
    }

    /// `c.free`. Disables the whole tag tree, exactly as a region free does
    /// (`[mem.prov.region]`) — a later access through any tag of it is §7/P1,
    /// and a later access through a *wildcard* is §7/L2.
    ///
    /// Returns `false` when the pointer does not name a live host allocation
    /// (a double free / a free of a non-heap pointer), which the caller turns
    /// into UB.
    pub fn host_free(&mut self, ptr: RawPtr, span: Span) -> bool {
        let Some(id) = ptr.alloc else { return false };
        let Some(alloc) = self.allocs.get(id) else {
            return false;
        };
        if !alloc.live || alloc.origin != Origin::HostC || ptr.offset != 0 {
            return false;
        }
        let tags = alloc.tags.clone();
        self.allocs[id].live = false;
        for tag in tags {
            for perm in &mut self.tags[tag].perms {
                *perm = Perm::Disabled;
            }
        }
        self.note(
            Rule::ProvRegion,
            span,
            format!("alloc#{id} freed: every tag in its tree is Disabled"),
        );
        true
    }

    /// The storage behind a Tier-0 place, created on first use.
    ///
    /// Size 1: `[mem.model.place]` makes the place the unit (paths are already
    /// field-granular), so one location per place is the faithful granularity —
    /// and it keeps the tree over safe-tier places cheap enough to run on every
    /// access of every corpus program.
    fn place_alloc(&mut self, key: &str, owner: Option<RegionId>, span: Span) -> (AllocId, TagId) {
        let _ = &owner;
        if let Some(found) = self.places.get(key) {
            return *found;
        }
        let id = self.new_alloc(
            1,
            Origin::Place,
            owner,
            format!("place `{key}`"),
            span,
            Perm::Active,
        );
        // §7/L1 is "read of uninitialized or moved-from memory **via raw
        // pointers**". A Tier-0 place's initialization is [`super::value::Slot`]'s
        // business, and reading a moved-from one is `trap(use-after-move)`
        // (`[mem.tier0.move.2]`) — a *defined* execution, not a UB row. So a
        // place allocation is born initialized and L1 never speaks about it.
        self.allocs[id].init.fill(true);
        let root = self.allocs[id].root;
        let entry = (id, root);
        self.places.insert(key.to_owned(), entry);
        entry
    }

    /// Binds a place key to an existing `(allocation, tag)` — how a callee's
    /// parameter comes to name the child tag its caller minted.
    pub fn bind_place(&mut self, key: &str, alloc: AllocId, tag: TagId) {
        if let Some((was, _)) = self.places.insert(key.to_owned(), (alloc, tag)) {
            // The displaced binding's tag may be unreachable now.
            self.mark_dirty(was);
        }
    }

    /// Binds a place key and returns what it was bound to, for a scoped swap.
    pub fn rebind_place(
        &mut self,
        key: &str,
        alloc: AllocId,
        tag: TagId,
    ) -> Option<(AllocId, TagId)> {
        let was = self.places.insert(key.to_owned(), (alloc, tag));
        if let Some((displaced, _)) = was {
            self.mark_dirty(displaced);
        }
        was
    }

    /// Restores a binding taken by [`Provenance::rebind_place`].
    pub fn restore_place(&mut self, key: &str, previous: Option<(AllocId, TagId)>) {
        let was = match previous {
            Some(entry) => self.places.insert(key.to_owned(), entry),
            None => self.places.remove(key),
        };
        if let Some((displaced, _)) = was {
            // The displaced binding's tag may be unreachable now.
            self.mark_dirty(displaced);
        }
    }

    /// The tag a place's reads and writes currently go through, if it has one.
    #[must_use]
    pub fn place(&self, key: &str) -> Option<(AllocId, TagId)> {
        self.places.get(key).copied()
    }

    /// One Tier-0 place access.
    ///
    /// A place no borrow ever named has no tag tree and no obligations: this
    /// returns `Ok` without allocating, which is what keeps the machine's cost
    /// proportional to the borrows a program creates rather than to its reads.
    ///
    /// # Errors
    ///
    /// The `[mem.ub]` row the access violates.
    pub fn access_place(
        &mut self,
        key: &str,
        kind: AccessKind,
        span: Span,
    ) -> Result<(), UbFinding> {
        let Some((alloc, tag)) = self.places.get(key).copied() else {
            return Ok(());
        };
        let ptr = RawPtr {
            alloc: Some(alloc),
            offset: 0,
            prov: Prov::Tag(tag),
            elem: 1,
            signed: false,
        };
        self.access(ptr, 1, kind, span)
    }

    /// Drops tags nothing can reach any more.
    ///
    /// The sprint permits exactly this and no more: "no tag GC beyond
    /// correctness (dead-tag pruning only if tests need it)". The tests need
    /// it — without it a loop that passes one place a thousand times leaves a
    /// thousand dead siblings for every later access to walk, and the tree walk
    /// is per-access. A tag is prunable when it is not a root, not protected,
    /// has no children, is not bound to any place, and is not exposed: nothing
    /// can ever observe it again, so removing it changes no verdict.
    pub fn prune(&mut self) {
        if self.dirty.is_empty() {
            return;
        }
        let bound: Vec<TagId> = self.places.values().map(|(_, tag)| *tag).collect();
        let mut dirty = std::mem::take(&mut self.dirty);
        dirty.sort_unstable();
        dirty.dedup();
        for index in dirty.drain(..) {
            // Repeat per allocation until nothing more falls off: pruning a
            // leaf can make its parent a leaf. Depth is the bound, and borrow
            // trees are shallow.
            loop {
                let root = self.allocs[index].root;
                let has_child: Vec<TagId> = self.allocs[index]
                    .tags
                    .iter()
                    .filter_map(|tag| self.tags[*tag].parent)
                    .collect();
                let keep: Vec<TagId> = self.allocs[index]
                    .tags
                    .iter()
                    .copied()
                    .filter(|tag| {
                        *tag == root
                            || self.tags[*tag].protected
                            || has_child.contains(tag)
                            || bound.contains(tag)
                            || self.allocs[index].exposed.contains(tag)
                    })
                    .collect();
                if keep.len() == self.allocs[index].tags.len() {
                    break;
                }
                self.allocs[index].tags = keep;
            }
        }
        // Hand the emptied buffer back so the hot path keeps its capacity.
        self.dirty = dirty;
    }

    /// Forgets every place of `task`'s frame `frame` and deeper: a frame's
    /// locals dying takes their tags with them.
    ///
    /// Keys are `t<task>:<frame>:<path>` (`Machine::place_key`). Until 0.1.2
    /// this parsed the pre-task `<frame>:<path>` shape and so retained
    /// *everything*: a callee's parameter binding survived its call, and the
    /// next call reusing that frame index and parameter name resolved its
    /// accesses through the stale (possibly Disabled) tag — the false
    /// `ub(mem.ub)` pair of issue #7 (wolf-std F-0013).
    pub fn drop_frame(&mut self, task: usize, frame: usize) {
        let prefix = format!("t{task}:");
        let dirty = &mut self.dirty;
        self.places.retain(|key, entry| {
            let keep = key
                .strip_prefix(&prefix)
                .and_then(|rest| rest.split_once(':'))
                .and_then(|(f, _)| f.parse::<usize>().ok())
                .is_none_or(|f| f < frame);
            if !keep && dirty.last() != Some(&entry.0) {
                // The dying binding's tag may be unreachable now.
                dirty.push(entry.0);
            }
            keep
        });
    }

    // -- retagging (`[mem.prov.tag]`) --------------------------------------

    /// Mints a child tag under `parent` (`[mem.prov.tag]`: "New tags are
    /// children of the tag they derive from").
    pub fn retag(
        &mut self,
        alloc: AllocId,
        parent: TagId,
        kind: RetagKind,
        protected: bool,
        label: &str,
        span: Span,
    ) -> TagId {
        let size = self.allocs[alloc].size;
        let id = self.tags.len();
        self.tags.push(Tag {
            id,
            alloc,
            parent: Some(parent),
            perms: vec![kind.perm(); size],
            protected,
            label: label.to_owned(),
            span,
        });
        self.allocs[alloc].tags.push(id);
        self.mark_dirty(alloc);
        self.note(
            Rule::ProvTag,
            span,
            format!(
                "retag {label}: tag#{id} is a child of tag#{parent} in alloc#{alloc}, {}{}",
                kind.perm(),
                if protected {
                    " and PROTECTED for the call's extent"
                } else {
                    ""
                }
            ),
        );
        id
    }

    /// Retags a Tier-0 place for a call, and returns the child.
    ///
    /// `[mem.prov.tag]`'s retag points include "`mut`/`read` parameter entry
    /// (parameter entry is protector-equivalent: the tag is protected for the
    /// whole call)".
    pub fn retag_place(
        &mut self,
        key: &str,
        kind: RetagKind,
        protected: bool,
        span: Span,
    ) -> (AllocId, TagId) {
        // A place's storage is **stack**, never region data:
        // `[mem.model.machine]` puts an allocation's owner at "a region (or
        // *stack* for locals)", and a local is the second. Giving it a region
        // owner would make §7/P4 fire on ordinary safe-tier reads after a
        // region free, which is a spurious fault in the one direction the
        // approximation contract forbids.
        let (alloc, parent) = self.place_alloc(key, None, span);
        let child = self.retag(alloc, parent, kind, protected, &format!("`{key}`"), span);
        (alloc, child)
    }

    /// Ends a protection extent.
    pub fn unprotect(&mut self, tag: TagId, span: Span) {
        if let Some(entry) = self.tags.get_mut(tag) {
            if !entry.protected {
                return;
            }
            entry.protected = false;
            let label = entry.label.clone();
            // No longer protected: the next prune must revisit its tree.
            self.mark_dirty(self.tags[tag].alloc);
            self.note(
                Rule::ProvTag,
                span,
                format!("protector on tag#{tag} ({label}) released: the call's extent ended"),
            );
        }
    }

    // -- exposure (`[mem.prov.expose]`) ------------------------------------

    /// ptr→int: the tag becomes exposed.
    pub fn expose(&mut self, ptr: RawPtr, span: Span) {
        let (Some(alloc), Prov::Tag(tag)) = (ptr.alloc, ptr.prov) else {
            return;
        };
        if let Some(entry) = self.allocs.get_mut(alloc)
            && !entry.exposed.contains(&tag)
        {
            entry.exposed.push(tag);
            self.note(
                Rule::ProvExpose,
                span,
                format!("ptr→int exposes tag#{tag} of alloc#{alloc}"),
            );
        }
    }

    /// Is this tag exposed?
    #[must_use]
    pub fn is_exposed(&self, alloc: AllocId, tag: TagId) -> bool {
        self.allocs
            .get(alloc)
            .is_some_and(|a| a.exposed.contains(&tag))
    }

    // -- region composition (`[mem.prov.region]`) --------------------------

    /// "Freeing a region **Disables every tag tree** of every allocation it
    /// owns."
    pub fn region_freed(&mut self, regions: &[RegionId], span: Span) {
        for region in regions {
            if !self.region_dead.contains(region) {
                self.region_dead.push(*region);
            }
        }
        // `live` is about the *allocation*: whether `c.free` released it. A
        // region's death Disables the tags that reach it and makes further
        // access §7/P4, but the block itself is still allocated — which is why
        // the host-leak report is a report about `free`, not about scopes.
        let mut hit = Vec::new();
        for alloc in &self.allocs {
            if alloc.owner.is_some_and(|owner| regions.contains(&owner)) {
                hit.push((alloc.id, alloc.tags.clone()));
            }
        }
        for (id, tags) in hit {
            for tag in tags {
                for perm in &mut self.tags[tag].perms {
                    *perm = Perm::Disabled;
                }
            }
            self.note(
                Rule::ProvRegion,
                span,
                format!("alloc#{id}'s owning region was freed: its whole tag tree is Disabled"),
            );
        }
    }

    /// "`freeze` transitions all its tags to Frozen."
    pub fn region_frozen(&mut self, regions: &[RegionId], span: Span) {
        let mut hit = Vec::new();
        for alloc in &self.allocs {
            if alloc.owner.is_some_and(|owner| regions.contains(&owner)) {
                hit.push((alloc.id, alloc.tags.clone()));
            }
        }
        for (id, tags) in hit {
            for tag in tags {
                for perm in &mut self.tags[tag].perms {
                    if *perm != Perm::Disabled {
                        *perm = Perm::Frozen;
                    }
                }
            }
            self.note(
                Rule::ProvRegion,
                span,
                format!("alloc#{id}'s owning region froze: every tag in its tree is Frozen"),
            );
        }
    }

    // -- `assume noalias` (`[mem.unsafe.raw.2]`) ---------------------------

    /// Asserts that the pointed-to ranges do not overlap.
    ///
    /// # Errors
    ///
    /// §7/P5 when two of them do: "A false assertion is UB — the only
    /// *assertion-created* UB in the language."
    pub fn assume_noalias(&mut self, ptrs: &[RawPtr], span: Span) -> Result<(), UbFinding> {
        let ranges: Vec<(Option<AllocId>, i128, i128)> = ptrs
            .iter()
            .map(|p| (p.alloc, p.offset, p.offset + p.elem as i128))
            .collect();
        for (i, a) in ranges.iter().enumerate() {
            for b in ranges.iter().skip(i + 1) {
                let Some(alloc) = a.0 else { continue };
                if a.0 == b.0 && a.1 < b.2 && b.1 < a.2 {
                    return Err(UbFinding {
                        row: UbRow::P5,
                        span,
                        tag_span: self.allocs.get(alloc).map(|a| a.span),
                        message: format!(
                            "`assume noalias` asserts these ranges are disjoint, and \
                             alloc#{alloc}[{}..{}) overlaps alloc#{alloc}[{}..{}) — the assertion \
                             is false",
                            a.1, a.2, b.1, b.2
                        ),
                        tree: self.tree(alloc),
                    });
                }
            }
        }
        self.assumptions.push(Assumption { ranges, span });
        self.note(
            Rule::AssumeNoalias,
            span,
            format!(
                "{} range(s) asserted disjoint; Tier-3 code may treat them as `noalias`",
                ptrs.len()
            ),
        );
        Ok(())
    }

    /// Assertions currently in force, for the trace and for tests.
    #[must_use]
    pub fn assumptions(&self) -> &[Assumption] {
        &self.assumptions
    }

    // -- the access check --------------------------------------------------

    /// Is `node` `access`'s tag or one of its ancestors?
    ///
    /// `[mem.prov.state]`: "child" = access through this tag **or a
    /// descendant". So the access is a child access for `node` exactly when the
    /// accessing tag lies in `node`'s subtree.
    fn is_child_access(&self, node: TagId, through: TagId) -> bool {
        let mut at = Some(through);
        let mut guard = 0usize;
        while let Some(current) = at {
            if current == node {
                return true;
            }
            guard += 1;
            if guard > self.tags.len() {
                return false;
            }
            at = self.tags[current].parent;
        }
        false
    }

    /// One access through one tag, against the whole tree. Returns the
    /// transitions to apply, or the row it violates.
    // The Ok arm is a list of (tag, offset, permission) transitions consumed
    // immediately by the two access paths; naming the triple would add a type
    // for one line of destructuring.
    #[allow(clippy::type_complexity)]
    fn plan(
        &self,
        alloc: AllocId,
        through: TagId,
        lo: usize,
        hi: usize,
        kind: AccessKind,
        span: Span,
    ) -> Result<Vec<(TagId, usize, Perm)>, UbFinding> {
        let mut plan = Vec::new();
        for node in self.allocs[alloc].tags.iter().copied() {
            let child = self.is_child_access(node, through);
            let tag = &self.tags[node];
            for off in lo..hi {
                let current = tag.perms[off];
                let next = match (current, child, kind) {
                    // -- child accesses -------------------------------------
                    // A child *read* leaves Reserved alone; activation happens
                    // at the first child *write* — the repaired `[mem.prov.state]`
                    // row (8b04edf; the original activated on reads by
                    // transcription error, closing the two-phase window early).
                    (Perm::Reserved, true, AccessKind::Read) => None,
                    (Perm::Reserved, true, AccessKind::Write) => Some(Perm::Active),
                    (Perm::Active, true, _) => None,
                    (Perm::Frozen, true, AccessKind::Read) => None,
                    (Perm::Frozen, true, AccessKind::Write) => {
                        return Err(self.finding(
                            UbRow::P2,
                            span,
                            node,
                            format!(
                                "write through tag#{node} ({}), which is Frozen at alloc#{alloc}[{off}] — \
                                 a `read`-mode borrow is immutable for its whole extent",
                                tag.label
                            ),
                        ));
                    }
                    (Perm::Disabled, true, _) => {
                        return Err(self.finding(
                            UbRow::P1,
                            span,
                            node,
                            format!(
                                "{kind} through tag#{node} ({}), which is Disabled at \
                                 alloc#{alloc}[{off}]",
                                tag.label
                            ),
                        ));
                    }
                    // -- foreign accesses -----------------------------------
                    // The TB fix for SB's "reference created but never used"
                    // spurious UB: a foreign read leaves Reserved alone.
                    (Perm::Reserved, false, AccessKind::Read) => None,
                    (Perm::Active, false, AccessKind::Read) => Some(Perm::Frozen),
                    (Perm::Frozen | Perm::Disabled, false, AccessKind::Read) => None,
                    (Perm::Disabled, false, AccessKind::Write) => None,
                    (Perm::Reserved | Perm::Active | Perm::Frozen, false, AccessKind::Write) => {
                        if tag.protected {
                            return Err(self.finding(
                                UbRow::P1,
                                span,
                                node,
                                format!(
                                    "foreign write at alloc#{alloc}[{off}] while tag#{node} ({}) is \
                                     PROTECTED for a call's extent — the protector makes the \
                                     invalidation UB at the write rather than at a later use",
                                    tag.label
                                ),
                            ));
                        }
                        Some(Perm::Disabled)
                    }
                };
                if let Some(next) = next {
                    plan.push((node, off, next));
                }
            }
        }
        Ok(plan)
    }

    fn finding(&self, row: UbRow, span: Span, tag: TagId, message: String) -> UbFinding {
        UbFinding {
            row,
            span,
            tag_span: self.tags.get(tag).map(|t| t.span),
            message,
            tree: self.tree(self.tags[tag].alloc),
        }
    }

    /// One memory access, provenance-checked end to end.
    ///
    /// The order of the checks is itself a decision, recorded in
    /// `docs/approximation-contract.md` §7.1: bounds (P3) before ownership
    /// (P4) before the tag tree (P1/P2) before initialization (L1), with the
    /// wildcard branch (L2) taken when the pointer carries no tag. Each row is
    /// the *most specific* thing true about the access, and each licenses a
    /// different optimization, so collapsing them would lose the D2 pairing.
    ///
    /// # Errors
    ///
    /// The `[mem.ub]` row the access violates.
    pub fn access(
        &mut self,
        ptr: RawPtr,
        len: usize,
        kind: AccessKind,
        span: Span,
    ) -> Result<(), UbFinding> {
        let Some(alloc) = ptr.alloc else {
            return Err(UbFinding {
                row: UbRow::L2,
                span,
                tag_span: None,
                message: format!("{kind} through a null raw pointer"),
                tree: Vec::new(),
            });
        };
        let Some(entry) = self.allocs.get(alloc) else {
            return Err(UbFinding {
                row: UbRow::L2,
                span,
                tag_span: None,
                message: format!("{kind} through a raw pointer into no allocation"),
                tree: Vec::new(),
            });
        };
        let size = entry.size;
        let owner = entry.owner;
        let live = entry.live;
        let exposed = entry.exposed.clone();

        // §7/P3 — outside the allocation.
        if ptr.offset < 0 || ptr.offset + (len as i128) > size as i128 {
            return Err(UbFinding {
                row: UbRow::P3,
                span,
                tag_span: Some(self.allocs[alloc].span),
                message: format!(
                    "{kind} of {len} byte(s) at alloc#{alloc}[{}], which holds {size}",
                    ptr.offset
                ),
                tree: self.tree(alloc),
            });
        }
        let lo = ptr.offset as usize;
        let hi = lo + len;

        // §7/P4 — the owning region died. Checked before the tag tree because
        // `[mem.prov.region]` Disables the tree *as a consequence* of the free,
        // and P4's licensed optimization (O3b/O4, per-region alias domains) is
        // the more specific fact.
        if let Some(owner) = owner
            && self.region_dead.contains(&owner)
        {
            return Err(UbFinding {
                row: UbRow::P4,
                span,
                tag_span: Some(self.allocs[alloc].span),
                message: format!(
                    "{kind} at alloc#{alloc}[{lo}], whose owning region #{owner} was freed \
                     wholesale"
                ),
                tree: self.tree(alloc),
            });
        }

        match ptr.prov {
            Prov::Tag(tag) => {
                let plan = self.plan(alloc, tag, lo, hi, kind, span)?;
                self.commit(plan);
            }
            Prov::Wildcard => {
                // §7/L2 — a wildcard into a dead allocation has no exposed tag
                // that could keep it defined, and the row that names it is the
                // dangling-raw-pointer one, not the tag one.
                if !live {
                    return Err(UbFinding {
                        row: UbRow::L2,
                        span,
                        tag_span: Some(self.allocs[alloc].span),
                        message: format!(
                            "{kind} through an exposed pointer into alloc#{alloc}, which was freed"
                        ),
                        tree: self.tree(alloc),
                    });
                }
                // `[mem.prov.expose]`: "resolved angelically among exposed tags
                // (a defined execution is chosen if one exists)". Try each.
                let mut first: Option<UbFinding> = None;
                let mut chosen = None;
                for candidate in exposed {
                    match self.plan(alloc, candidate, lo, hi, kind, span) {
                        Ok(plan) => {
                            chosen = Some((candidate, plan));
                            break;
                        }
                        Err(finding) => {
                            if first.is_none() {
                                first = Some(finding);
                            }
                        }
                    }
                }
                match chosen {
                    Some((candidate, plan)) => {
                        self.commit(plan);
                        self.note(
                            Rule::ProvExpose,
                            span,
                            format!(
                                "wildcard {kind} at alloc#{alloc}[{lo}] resolved angelically to \
                                 exposed tag#{candidate}"
                            ),
                        );
                    }
                    None => {
                        return Err(first.unwrap_or_else(|| UbFinding {
                            row: UbRow::P1,
                            span,
                            tag_span: Some(self.allocs[alloc].span),
                            message: format!(
                                "{kind} at alloc#{alloc}[{lo}] through a wildcard, and the \
                                 allocation has no exposed tag for it to resolve to"
                            ),
                            tree: self.tree(alloc),
                        }));
                    }
                }
            }
        }

        // §7/L1 — a read of memory nothing wrote.
        if kind == AccessKind::Read
            && let Some(off) = (lo..hi).find(|off| !self.allocs[alloc].init[*off])
        {
            return Err(UbFinding {
                row: UbRow::L1,
                span,
                tag_span: Some(self.allocs[alloc].span),
                message: format!("read of alloc#{alloc}[{off}], which nothing has written"),
                tree: self.tree(alloc),
            });
        }
        if kind == AccessKind::Write {
            for off in lo..hi {
                self.allocs[alloc].init[off] = true;
            }
        }
        self.note(
            Rule::ProvState,
            span,
            format!(
                "{kind} alloc#{alloc}[{lo}..{hi}) through {} — tree consistent",
                ptr.prov
            ),
        );
        Ok(())
    }

    fn commit(&mut self, plan: Vec<(TagId, usize, Perm)>) {
        for (tag, off, perm) in plan {
            self.tags[tag].perms[off] = perm;
        }
    }

    /// Renders the borrow tree of one allocation — the third element of the
    /// sprint's "two spans plus tree" report.
    #[must_use]
    pub fn tree(&self, alloc: AllocId) -> Vec<String> {
        let Some(entry) = self.allocs.get(alloc) else {
            return Vec::new();
        };
        let mut out = vec![format!(
            "alloc#{alloc} `{}` {} byte(s), {}{}",
            entry.label,
            entry.size,
            if entry.live { "live" } else { "FREED" },
            match entry.owner {
                Some(owner) => format!(", owned by region #{owner}"),
                None => String::new(),
            }
        )];
        self.render(entry.root, 1, &mut out);
        out
    }

    fn render(&self, tag: TagId, depth: usize, out: &mut Vec<String>) {
        let Some(entry) = self.tags.get(tag) else {
            return;
        };
        let perms: Vec<String> = entry.perms.iter().map(ToString::to_string).collect();
        let squashed = if perms.iter().all(|p| *p == perms[0]) {
            perms[0].clone()
        } else {
            perms.join("|")
        };
        out.push(format!(
            "{}tag#{} {} {}{}{}",
            "  ".repeat(depth),
            entry.id,
            entry.label,
            squashed,
            if entry.protected { " PROTECTED" } else { "" },
            if self.is_exposed(entry.alloc, entry.id) {
                " exposed"
            } else {
                ""
            }
        ));
        let children: Vec<TagId> = self.allocs[entry.alloc]
            .tags
            .iter()
            .copied()
            .filter(|child| self.tags[*child].parent == Some(tag))
            .collect();
        for child in children {
            self.render(child, depth + 1, out);
        }
    }

    // -- the address space (`[mem.prov.expose]`) ---------------------------

    /// Bytes between one modelled allocation's base and the next.
    ///
    /// A reference interpreter has no addresses to report, and the protocol
    /// says so: `[proto.cmp.defined-divergence]` lists "unspecified layout
    /// observations (Tier-3 address inspection)" among the things that are
    /// **never** divergences. So this is a deterministic implementation-
    /// specified layout, wide enough that no modelled allocation reaches the
    /// next one, and its only job is to make `ptr as int as *T` round-trip.
    pub const STRIDE: i128 = 0x1_0000;

    /// The integer a ptr→int cast yields.
    #[must_use]
    pub fn address_of(&self, ptr: RawPtr) -> i128 {
        match ptr.alloc {
            None => 0,
            Some(alloc) => (alloc as i128 + 1) * Provenance::STRIDE + ptr.offset,
        }
    }

    /// The allocation an int→ptr cast lands in, if any.
    #[must_use]
    pub fn resolve_address(&self, address: i128) -> Option<(AllocId, i128)> {
        if address <= 0 {
            return None;
        }
        let alloc = usize::try_from(address / Provenance::STRIDE - 1).ok()?;
        let offset = address % Provenance::STRIDE;
        // Off the end of every allocation is still "no allocation", which is
        // what makes §7/L2 reachable from a stale integer.
        let size = self.allocs.get(alloc)?.size as i128;
        if offset > size {
            return None;
        }
        Some((alloc, offset))
    }

    // -- byte-level storage -------------------------------------------------

    /// Reads `len` little-endian bytes, sign-extending a signed pointee. The
    /// caller has already run [`Provenance::access`].
    #[must_use]
    pub fn load(&self, ptr: RawPtr, len: usize) -> i128 {
        let Some(alloc) = ptr.alloc.and_then(|id| self.allocs.get(id)) else {
            return 0;
        };
        let lo = ptr.offset.max(0) as usize;
        let mut value: i128 = 0;
        for (i, off) in (lo..lo + len).enumerate() {
            let byte = alloc.bytes.get(off).copied().unwrap_or(0);
            value |= i128::from(byte) << (8 * i);
        }
        let bits = 8 * len;
        if ptr.signed && bits < 128 && (value >> (bits - 1)) & 1 == 1 {
            value -= 1i128 << bits;
        }
        value
    }

    /// Marks a range initialized without writing it — `calloc`'s zeroing, which
    /// §7/L1 has to see or it would report a read of memory `calloc` did write.
    pub fn init_range(&mut self, ptr: RawPtr, len: usize) {
        let Some(alloc) = ptr.alloc.and_then(|id| self.allocs.get_mut(id)) else {
            return;
        };
        let lo = ptr.offset.max(0) as usize;
        for off in lo..(lo + len).min(alloc.init.len()) {
            alloc.init[off] = true;
        }
    }

    /// Writes `len` little-endian bytes. The caller has already run
    /// [`Provenance::access`].
    pub fn store(&mut self, ptr: RawPtr, len: usize, value: i128) {
        let Some(alloc) = ptr.alloc.and_then(|id| self.allocs.get_mut(id)) else {
            return;
        };
        let lo = ptr.offset.max(0) as usize;
        for (i, off) in (lo..lo + len).enumerate() {
            if let Some(byte) = alloc.bytes.get_mut(off) {
                // Masked to one byte the line above; the truncation is the point.
                #[allow(clippy::cast_possible_truncation)]
                {
                    *byte = ((value >> (8 * i)) & 0xff) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::new(0, 1)
    }

    /// A four-byte host allocation whose bytes are already written, so a test
    /// about the *tag tree* is not also a test about §7/L1.
    fn machine() -> (Provenance, RawPtr) {
        let mut prov = Provenance::new();
        let ptr = prov.host_alloc(4, 0, span());
        prov.init_range(ptr, 4);
        prov.take_notes();
        (prov, ptr)
    }

    fn perm(prov: &Provenance, tag: TagId) -> Perm {
        prov.tag(tag).expect("a tag").perms[0]
    }

    fn child(prov: &mut Provenance, parent: TagId, kind: RetagKind, protected: bool) -> RawPtr {
        let alloc = prov.tag(parent).expect("a tag").alloc;
        let tag = prov.retag(alloc, parent, kind, protected, "test", span());
        RawPtr {
            alloc: Some(alloc),
            offset: 0,
            prov: Prov::Tag(tag),
            elem: 1,
            signed: false,
        }
    }

    #[test]
    fn drop_frame_forgets_the_tasks_frame_and_deeper_and_nothing_else() {
        // Issue #7 (wolf-std F-0013): a callee's parameter binding must die
        // with its frame. Keys are `t<task>:<frame>:<path>`; the 0.1.1
        // parser read the pre-task `<frame>:<path>` shape and retained
        // *everything*, so the next call reusing the frame index and
        // parameter name resolved its accesses through the stale — possibly
        // Disabled — tag, which is both filed false-`ub(mem.ub)` shapes.
        let mut prov = Provenance::new();
        let (alloc, tag) = prov.retag_place("t0:1:xs", RetagKind::Mutable, false, span());
        prov.bind_place("t0:2:xs", alloc, tag);
        prov.bind_place("t0:3:tmp", alloc, tag);
        prov.bind_place("t1:2:xs", alloc, tag);
        prov.drop_frame(0, 2);
        assert!(
            prov.place("t0:2:xs").is_none(),
            "the callee binding dies with its frame"
        );
        assert!(
            prov.place("t0:3:tmp").is_none(),
            "deeper frames of the same task die too"
        );
        assert!(
            prov.place("t0:1:xs").is_some(),
            "the caller's own place survives"
        );
        assert!(
            prov.place("t1:2:xs").is_some(),
            "another task's frames are not this task's to drop"
        );
    }

    #[test]
    fn the_transition_table_is_the_one_the_clause_prints() {
        // `[mem.prov.state]`, row by row. This is the spec-extracted table made
        // executable; a change here is a spec change.
        for (state, kind, child_access, expected) in [
            // The repaired Reserved row (8b04edf): a child read leaves the
            // two-phase window open; only the first child write activates.
            (Perm::Reserved, AccessKind::Read, true, Ok(Perm::Reserved)),
            (Perm::Reserved, AccessKind::Write, true, Ok(Perm::Active)),
            (Perm::Reserved, AccessKind::Read, false, Ok(Perm::Reserved)),
            (Perm::Reserved, AccessKind::Write, false, Ok(Perm::Disabled)),
            (Perm::Active, AccessKind::Read, true, Ok(Perm::Active)),
            (Perm::Active, AccessKind::Write, true, Ok(Perm::Active)),
            (Perm::Active, AccessKind::Read, false, Ok(Perm::Frozen)),
            (Perm::Active, AccessKind::Write, false, Ok(Perm::Disabled)),
            (Perm::Frozen, AccessKind::Read, true, Ok(Perm::Frozen)),
            (Perm::Frozen, AccessKind::Write, true, Err(UbRow::P2)),
            (Perm::Frozen, AccessKind::Read, false, Ok(Perm::Frozen)),
            (Perm::Frozen, AccessKind::Write, false, Ok(Perm::Disabled)),
            (Perm::Disabled, AccessKind::Read, true, Err(UbRow::P1)),
            (Perm::Disabled, AccessKind::Write, true, Err(UbRow::P1)),
            (Perm::Disabled, AccessKind::Read, false, Ok(Perm::Disabled)),
            (Perm::Disabled, AccessKind::Write, false, Ok(Perm::Disabled)),
        ] {
            // The node under test is a child of the root; the access goes
            // through it (child) or through a *sibling* (foreign).
            let (mut prov, root) = machine();
            let Prov::Tag(root_tag) = root.prov else {
                unreachable!()
            };
            let under = child(&mut prov, root_tag, RetagKind::Mutable, false);
            let Prov::Tag(under_tag) = under.prov else {
                unreachable!()
            };
            prov.tags[under_tag].perms.fill(state);
            let through = if child_access {
                under
            } else {
                child(&mut prov, root_tag, RetagKind::Mutable, false)
            };
            // Keep the *other* tags out of the way: only the node under test
            // decides the outcome.
            prov.allocs[0].tags.retain(|tag| {
                *tag == under_tag || matches!(through.prov, Prov::Tag(t) if *tag == t)
            });
            if !child_access && let Prov::Tag(t) = through.prov {
                prov.tags[t].perms.fill(Perm::Active);
            }
            let result = prov.access(through, 1, kind, span());
            match (expected, result) {
                (Ok(want), Ok(())) => assert_eq!(
                    perm(&prov, under_tag),
                    want,
                    "{state:?} {kind} child={child_access}"
                ),
                (Err(row), Err(finding)) => {
                    assert_eq!(finding.row, row, "{state:?} {kind} child={child_access}")
                }
                (want, got) => {
                    panic!("{state:?} {kind} child={child_access}: wanted {want:?}, got {got:?}")
                }
            }
        }
    }

    #[test]
    fn a_foreign_read_leaves_reserved_alone_which_is_the_tree_borrows_fix() {
        // reports/01's −54% false-rejection number, encoded: Stacked Borrows
        // pops a reference on a foreign read even when it was "created but never
        // used"; `[mem.prov.state]` says Reserved is unaffected, so the later
        // child write still activates rather than faulting.
        let (mut prov, root) = machine();
        let Prov::Tag(root_tag) = root.prov else {
            unreachable!()
        };
        let borrow = child(&mut prov, root_tag, RetagKind::Mutable, false);
        let Prov::Tag(borrow_tag) = borrow.prov else {
            unreachable!()
        };
        assert_eq!(perm(&prov, borrow_tag), Perm::Reserved);

        prov.access(root, 1, AccessKind::Read, span())
            .expect("a foreign read of a Reserved tag is defined");
        assert_eq!(
            perm(&prov, borrow_tag),
            Perm::Reserved,
            "the reborrow survived the parent's read"
        );
        prov.access(borrow, 1, AccessKind::Write, span())
            .expect("and the reborrow is still usable");
        assert_eq!(perm(&prov, borrow_tag), Perm::Active);
    }

    #[test]
    fn a_protector_turns_the_foreign_write_into_ub_at_the_write() {
        // The D3 `mut`-noalias call-extent fact. Without the protector the same
        // write merely Disables the tag, and the UB arrives later (or never).
        let (mut prov, root) = machine();
        let Prov::Tag(root_tag) = root.prov else {
            unreachable!()
        };
        let unprotected = child(&mut prov, root_tag, RetagKind::Shared, false);
        let Prov::Tag(unprotected_tag) = unprotected.prov else {
            unreachable!()
        };
        prov.access(root, 1, AccessKind::Write, span())
            .expect("a foreign write past an unprotected tag is defined");
        assert_eq!(perm(&prov, unprotected_tag), Perm::Disabled);

        let (mut prov, root) = machine();
        let Prov::Tag(root_tag) = root.prov else {
            unreachable!()
        };
        let protected = child(&mut prov, root_tag, RetagKind::Shared, true);
        let Prov::Tag(protected_tag) = protected.prov else {
            unreachable!()
        };
        let finding = prov
            .access(root, 1, AccessKind::Write, span())
            .expect_err("a protected tag escalates it");
        assert_eq!(finding.row, UbRow::P1);
        assert!(finding.message.contains("PROTECTED"), "{}", finding.message);
        // And the escalation is *only* for the extent.
        prov.unprotect(protected_tag, span());
        prov.access(root, 1, AccessKind::Write, span())
            .expect("the extent ended");
        assert_eq!(perm(&prov, protected_tag), Perm::Disabled);
    }

    #[test]
    fn a_wildcard_resolves_angelically_among_the_exposed_tags() {
        // `[mem.prov.expose]`: "a defined execution is chosen if one exists".
        // The exposed set holds a Disabled tag *and* a usable one; a machine
        // that picked the first candidate would report UB, and the clause says
        // it must not.
        let (mut prov, root) = machine();
        let Prov::Tag(root_tag) = root.prov else {
            unreachable!()
        };
        let dead = child(&mut prov, root_tag, RetagKind::Mutable, false);
        let Prov::Tag(dead_tag) = dead.prov else {
            unreachable!()
        };
        prov.tags[dead_tag].perms.fill(Perm::Disabled);
        // Expose the dead one *first*, so order cannot be what saves it.
        prov.allocs[0].exposed.insert(0, dead_tag);
        prov.allocs[0].init.fill(true);

        let wildcard = RawPtr {
            prov: Prov::Wildcard,
            ..root
        };
        prov.access(wildcard, 1, AccessKind::Read, span())
            .expect("the root is exposed and keeps the execution defined");

        // With no candidate left, the same access has no defined execution.
        let (mut prov, root) = machine();
        prov.allocs[0].exposed.clear();
        prov.allocs[0].init.fill(true);
        let wildcard = RawPtr {
            prov: Prov::Wildcard,
            ..root
        };
        assert!(prov.access(wildcard, 1, AccessKind::Read, span()).is_err());
    }

    #[test]
    fn region_composition_disables_and_freezes_whole_trees() {
        // `[mem.prov.region]`, both halves.
        let mut prov = Provenance::new();
        let ptr = prov.host_alloc(2, 7, span());
        let Prov::Tag(root) = ptr.prov else {
            unreachable!()
        };
        prov.region_frozen(&[7], span());
        assert_eq!(perm(&prov, root), Perm::Frozen);
        let write = prov
            .access(ptr, 1, AccessKind::Write, span())
            .expect_err("frozen data is writable from nowhere");
        assert_eq!(write.row, UbRow::P2);

        let mut prov = Provenance::new();
        let ptr = prov.host_alloc(2, 7, span());
        let Prov::Tag(root) = ptr.prov else {
            unreachable!()
        };
        prov.region_freed(&[7], span());
        assert_eq!(perm(&prov, root), Perm::Disabled);
        let read = prov
            .access(ptr, 1, AccessKind::Read, span())
            .expect_err("the region died and took the allocation with it");
        // P4, not P1: the *region* is the fact, and it licenses a different
        // optimization (O3b/O4 rather than O1).
        assert_eq!(read.row, UbRow::P4);
    }

    #[test]
    fn out_of_bounds_and_uninitialized_are_their_own_rows() {
        let (mut prov, ptr) = machine();
        let past = RawPtr { offset: 3, ..ptr };
        assert_eq!(
            prov.access(past, 4, AccessKind::Write, span())
                .expect_err("4 bytes at offset 3 of a 4-byte allocation")
                .row,
            UbRow::P3
        );
        // A *fresh* allocation is uninitialized; `machine()` writes its bytes,
        // so L1 needs one that has not been through it.
        let mut fresh = Provenance::new();
        let untouched = fresh.host_alloc(4, 0, span());
        assert_eq!(
            fresh
                .access(untouched, 1, AccessKind::Read, span())
                .expect_err("malloc'd storage is uninitialized")
                .row,
            UbRow::L1
        );
        prov.access(ptr, 1, AccessKind::Write, span())
            .expect("writing it is fine");
        prov.access(ptr, 1, AccessKind::Read, span())
            .expect("and then reading it is too");
    }

    #[test]
    fn a_false_noalias_assertion_is_ub_where_it_is_written() {
        let (mut prov, ptr) = machine();
        let same = ptr;
        let finding = prov
            .assume_noalias(&[ptr, same], span())
            .expect_err("the same range is not disjoint from itself");
        assert_eq!(finding.row, UbRow::P5);

        let apart = RawPtr { offset: 2, ..ptr };
        prov.assume_noalias(&[ptr, apart], span())
            .expect("`[0,1)` and `[2,3)` are disjoint");
        assert_eq!(prov.assumptions().len(), 1);
    }

    #[test]
    fn the_address_space_round_trips_and_stops_at_the_allocation() {
        let mut prov = Provenance::new();
        let a = prov.host_alloc(8, 0, span());
        let b = prov.host_alloc(8, 0, span());
        assert_ne!(prov.address_of(a), prov.address_of(b));
        assert_eq!(prov.resolve_address(prov.address_of(a)), Some((0, 0)));
        let inside = RawPtr { offset: 5, ..a };
        assert_eq!(prov.resolve_address(prov.address_of(inside)), Some((0, 5)));
        // Past the end of the allocation is no allocation at all, which is what
        // keeps §7/L2 reachable from a stale integer.
        let past = RawPtr { offset: 99, ..a };
        assert_eq!(prov.resolve_address(prov.address_of(past)), None);
        assert_eq!(prov.resolve_address(0), None);
        assert_eq!(prov.resolve_address(-1), None);
    }

    #[test]
    fn the_tree_dump_shows_the_shape_and_the_states() {
        let (mut prov, root) = machine();
        let Prov::Tag(root_tag) = root.prov else {
            unreachable!()
        };
        let borrow = child(&mut prov, root_tag, RetagKind::Shared, true);
        let Prov::Tag(borrow_tag) = borrow.prov else {
            unreachable!()
        };
        let _ = borrow_tag;
        let tree = prov.tree(0);
        assert!(tree[0].contains("4 byte(s), live"), "{tree:?}");
        assert!(tree[1].contains("Active"), "{tree:?}");
        assert!(tree[1].contains("exposed"), "{tree:?}");
        assert!(tree[2].contains("Frozen"), "{tree:?}");
        assert!(tree[2].contains("PROTECTED"), "{tree:?}");
        // Depth is indentation, so the parent/child relation is readable.
        assert!(tree[2].starts_with("    "), "{tree:?}");
    }

    #[test]
    fn pruning_keeps_everything_that_can_still_be_observed() {
        let (mut prov, root) = machine();
        let Prov::Tag(root_tag) = root.prov else {
            unreachable!()
        };
        let protected = child(&mut prov, root_tag, RetagKind::Shared, true);
        let bound = child(&mut prov, root_tag, RetagKind::Mutable, false);
        let dead = child(&mut prov, root_tag, RetagKind::Mutable, false);
        let (Prov::Tag(protected_tag), Prov::Tag(bound_tag), Prov::Tag(dead_tag)) =
            (protected.prov, bound.prov, dead.prov)
        else {
            unreachable!()
        };
        prov.bind_place("0:x", 0, bound_tag);
        assert_eq!(prov.allocs[0].tags.len(), 4);

        prov.prune();
        let kept = prov.allocs[0].tags.clone();
        assert!(kept.contains(&root_tag), "the root is never pruned");
        assert!(kept.contains(&protected_tag), "a protector is live");
        assert!(kept.contains(&bound_tag), "a bound place is live");
        assert!(
            !kept.contains(&dead_tag),
            "nothing can reach this one again"
        );

        // Releasing the protector makes it prunable in turn — the release
        // alone re-dirties the allocation, no new tag needed (is20, #41).
        prov.unprotect(protected_tag, span());
        prov.prune();
        assert!(!prov.allocs[0].tags.contains(&protected_tag));

        // A binding's death re-dirties too: unbind the placed tag and the
        // next prune drops it.
        prov.restore_place("0:x", None);
        prov.prune();
        assert!(!prov.allocs[0].tags.contains(&bound_tag));
        assert_eq!(prov.allocs[0].tags, vec![root_tag]);
    }

    #[test]
    fn prune_revisits_only_what_changed() {
        // wolf-interp#41: prune ran over EVERY allocation the program had
        // ever made, on every call return — per-iteration cost grew linearly
        // with program age (129 CAVP vectors: 27s as four programs, 132s as
        // one). The fix keys prune on a dirty set; an allocation nothing
        // touched since the last prune is not revisited.
        let mut prov = Provenance::new();
        for i in 0..100 {
            let key = format!("0:aged{i}");
            let _ = prov.retag_place(&key, RetagKind::Mutable, false, span());
            prov.restore_place(&key, None);
        }
        prov.prune();
        assert!(prov.dirty.is_empty(), "a prune consumes the dirty set");

        // New work dirties exactly its own allocation.
        let (alloc, _) = prov.retag_place("0:fresh", RetagKind::Mutable, false, span());
        assert_eq!(
            prov.dirty,
            vec![alloc],
            "one retag dirties one allocation, not the program's whole history"
        );
        prov.prune();
        assert!(prov.dirty.is_empty());
    }

    #[test]
    fn a_place_never_reports_uninitialized_or_region_freed() {
        // The two approximations that keep safe-tier code out of §7 by
        // construction: a place's storage is *stack* (so no region owns it) and
        // its initialization is `Slot`'s business (so L1 never speaks about it).
        let mut prov = Provenance::new();
        let (alloc, _) = prov.retag_place("0:x", RetagKind::Mutable, false, span());
        assert_eq!(prov.alloc(alloc).expect("an alloc").owner, None);
        assert!(prov.alloc(alloc).expect("an alloc").init.iter().all(|i| *i));
        prov.region_freed(&[0], span());
        prov.access_place("0:x", AccessKind::Read, span())
            .expect("freeing every region does not touch a stack place");
    }

    #[test]
    fn a_place_with_no_tag_costs_nothing() {
        // The laziness that keeps the machine affordable: a place no borrow ever
        // named has no allocation at all.
        let mut prov = Provenance::new();
        prov.access_place("0:never-borrowed", AccessKind::Write, span())
            .expect("defined");
        assert!(prov.allocs().is_empty());
    }

    #[test]
    fn every_row_of_the_enumeration_names_its_licensed_optimization() {
        // `[mem.ub.closed]`, at the unit level; `tests/ub_coverage.rs` is the
        // matrix that also demands a program per row.
        for row in UbRow::ALL {
            assert!(!row.optimization().is_empty(), "{row}");
            assert!(!row.what().is_empty(), "{row}");
        }
        assert_eq!(UbRow::ALL.len(), 11);
    }
}

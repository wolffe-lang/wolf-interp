//! Runtime values and slots — MVS made literal (`[mem.model.value]`).
//!
//! > A **value** is data with MVS semantics: assignment and argument passing
//! > transfer or copy the *whole* value; values have no identity beyond their
//! > current place.
//!
//! Which is why [`Value`] is an ordinary owned Rust tree with no `Rc` in it
//! anywhere: there is no sharing to model at Tier 0, so a Rust `clone` *is* a
//! wolf `copy` and a Rust move *is* a wolf move.
//!
//! is03 adds the values that *do* have identity, and every one of them is an
//! **index into [`super::region::Store`]** rather than a Rust pointer:
//! [`Value::Region`], [`Value::PoolRef`], [`Value::Handle`], [`Value::Shared`]
//! and [`Value::Weak`]. Keeping identity in a side table rather than in the
//! value tree is what makes use-after-free *exact* — a stale reference is a
//! live index whose generation no longer matches, which is a comparison, not a
//! guess — and it keeps `Value` cloneable, which the whole Tier-0 machine
//! depends on.
//!
//! Storage is **slots**, and every slot carries its own dynamic state
//! ([`SlotState`]). Struct fields and tuple elements are slots in their own
//! right, which is the whole of "field-granular states: `a.x` can be `Moved`
//! while `a.y` is `Live`" — it costs a struct field rather than a side table.

use std::fmt;

use super::region::{CellId, PoolId, RegionId};
use crate::diag::Span;

/// Whether a slot currently holds a value (`[mem.tier0.move.1]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Live,
    /// Moved out at this span; reading traps `use-after-move`
    /// (`[mem.tier0.move.2]`). The span is the *move site*, which the fault
    /// prints alongside the use site.
    Moved(Span),
}

/// One storage location: a local, a field, a tuple element, a list element.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub state: SlotState,
    /// The value. Kept even when `state` is `Moved` — the move took the value
    /// away logically, and reading is a trap, so what remains is unobservable.
    /// Retaining it costs nothing and keeps `Slot` total.
    pub value: Value,
}

impl Slot {
    #[must_use]
    pub fn live(value: Value) -> Slot {
        Slot {
            state: SlotState::Live,
            value,
        }
    }

    #[must_use]
    pub fn is_live(&self) -> bool {
        self.state == SlotState::Live
    }

    /// Moves the value out, leaving the slot uninitialized
    /// (`[mem.tier0.move.1]`).
    ///
    /// The bytes stay behind. Logically the place is uninitialized and reading
    /// it traps, so their content is unobservable — but their *shape* is not:
    /// `a.x` has to keep denoting a place after `a.y` moves out, or
    /// field-granular states could not be checked at all.
    pub fn take_value(&mut self, at: Span) -> Value {
        self.state = SlotState::Moved(at);
        self.value.clone()
    }
}

/// How an integer type behaves at the edge of its range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithMode {
    /// X3: overflow traps, in every profile.
    Checked,
    Wrapping,
    Saturating,
}

/// An integer type: width, signedness, and overflow behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntTy {
    pub bits: u32,
    pub signed: bool,
    pub mode: ArithMode,
    /// True while the value is still an unconstrained literal: it has not met a
    /// checking context, so it adopts the other operand's type instead of
    /// imposing its own (`[arith.literal.default]` — D27's "literals default by
    /// rule (i32/f64)" plus the propagation sema-lite needs).
    pub literal: bool,
}

impl IntTy {
    pub const I32: IntTy = IntTy {
        bits: 32,
        signed: true,
        mode: ArithMode::Checked,
        literal: false,
    };
    /// `int`, wolf's pointer-free default integer. Distinct from `i32` so a
    /// program that writes both keeps them apart.
    pub const INT: IntTy = IntTy {
        bits: 64,
        signed: true,
        mode: ArithMode::Checked,
        literal: false,
    };
    /// An integer literal that has not yet met a type. Defaults to `i32`.
    pub const LITERAL: IntTy = IntTy {
        literal: true,
        ..IntTy::I32
    };
    /// A literal that has passed through arithmetic and is *still*
    /// unconstrained (issue #14): `-9223372036854775807 - 1` must stay
    /// writable, so literal-only arithmetic is checked at the width the
    /// machine computes in (i128) and the `[arith.literal.default]` i32 rule
    /// is applied where the value finally meets its context — an
    /// unannotated binding — not at the operator, where the declared type
    /// of a binding, const, or concrete operand may still arrive to type it.
    pub const LITERAL_WIDE: IntTy = IntTy {
        bits: 128,
        signed: true,
        mode: ArithMode::Checked,
        literal: true,
    };

    /// Names the spec spells: `i8…i128`, `u8…u128`, `int`, `uint`, plus the
    /// `wrapping[T]`/`saturating[T]` wrappers applied separately.
    #[must_use]
    pub fn named(name: &str) -> Option<IntTy> {
        let base = |bits, signed| {
            Some(IntTy {
                bits,
                signed,
                mode: ArithMode::Checked,
                literal: false,
            })
        };
        match name {
            "i8" => base(8, true),
            "i16" => base(16, true),
            "i32" => base(32, true),
            "i64" => base(64, true),
            "i128" => base(128, true),
            "u8" => base(8, false),
            "u16" => base(16, false),
            "u32" => base(32, false),
            "u64" => base(64, false),
            "u128" => base(128, false),
            "int" => Some(IntTy::INT),
            "uint" => base(64, false),
            _ => None,
        }
    }

    #[must_use]
    pub fn with_mode(self, mode: ArithMode) -> IntTy {
        IntTy { mode, ..self }
    }

    /// The inclusive range this type can hold.
    #[must_use]
    pub fn range(self) -> (i128, i128) {
        if self.bits >= 128 {
            return if self.signed {
                (i128::MIN, i128::MAX)
            } else {
                (0, i128::MAX)
            };
        }
        if self.signed {
            (-(1i128 << (self.bits - 1)), (1i128 << (self.bits - 1)) - 1)
        } else {
            (0, (1i128 << self.bits) - 1)
        }
    }

    #[must_use]
    pub fn holds(self, value: i128) -> bool {
        let (lo, hi) = self.range();
        value >= lo && value <= hi
    }

    /// Reduces a value into range for `wrapping`, saturates for `saturating`.
    /// Returns `None` for `Checked` — the caller traps.
    #[must_use]
    pub fn reduce(self, value: i128) -> Option<i128> {
        if self.holds(value) {
            return Some(value);
        }
        let (lo, hi) = self.range();
        match self.mode {
            ArithMode::Checked => None,
            ArithMode::Saturating => Some(if value < lo { lo } else { hi }),
            ArithMode::Wrapping => {
                if self.bits >= 128 {
                    return Some(value);
                }
                let span = 1i128 << self.bits;
                let mut wrapped = value.rem_euclid(span);
                if self.signed && wrapped > hi {
                    wrapped -= span;
                }
                Some(wrapped)
            }
        }
    }

    /// The spelling used in messages.
    #[must_use]
    pub fn name(self) -> String {
        if self.literal {
            // An unconstrained literal *reads* as its `[arith.literal.default]`
            // default in every message and REPL context — its carried width is
            // the machine's computing width, not a name the language has.
            return "i32".to_owned();
        }
        let base = format!("{}{}", if self.signed { 'i' } else { 'u' }, self.bits);
        match self.mode {
            ArithMode::Checked => base,
            ArithMode::Wrapping => format!("wrapping[{base}]"),
            ArithMode::Saturating => format!("saturating[{base}]"),
        }
    }
}

/// A structural error tag: `BadDigit(ParseError { … })`, `TooShort`,
/// `io.Error(_)`.
///
/// D30's rows are structural (`[err.rows]`), so a tag needs no declaration to
/// exist — which is exactly what a dynamic machine wants: the tag *is* its name.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorValue {
    /// The dotted tag path, e.g. `BadDigit` or `io.Error`.
    pub tag: String,
    pub payload: Vec<Value>,
}

/// A closure: parameters, body, and the environment it captured **by value**.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosureValue {
    pub params: Vec<String>,
    pub body: crate::ast::Expr,
    /// Free variables, captured when the closure was built (`[gram.expr.closure]`).
    pub captures: Vec<(String, Value)>,
}

/// A first-class region value (X4, `[mem.region.create.2]`).
///
/// Affine: it moves and is never copied, which is what
/// `[mem.region.multiopen]` leans on for distinctness. The generation is the
/// region's at binding time, so a value that outlives a wholesale free is
/// detectably stale rather than merely wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionValue {
    pub id: RegionId,
    pub generation: u32,
}

/// A `handle T`: `(pool, index, generation)` (`[mem.shared.handle.1]`).
///
/// Copy-shaped — `spec/02` Appendix A annotates `var cur = hs[0]` with
/// `[mem.tier0.move.3]` "handle is Copy-shaped" — so handles behave like the
/// generational indices they are, and every deref re-checks the generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandleValue {
    pub pool: PoolId,
    pub index: usize,
    pub generation: u32,
}

/// A wolf runtime value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// The value of a statement, an empty block, a `for` loop.
    Unit,
    Bool(bool),
    Int(i128, IntTy),
    Float(f64),
    Str(String),
    Tuple(Vec<Slot>),
    Struct {
        name: String,
        fields: Vec<(String, Slot)>,
        /// The region charged at the literal's allocation site
        /// (`[mem.model.alloc]` lands in the current region,
        /// `[mem.region.create.3]`). Writes through the value consult this
        /// region's state — what lets `[mem.region.freeze.1]` fault on
        /// tier-0 value paths, not only on granule paths. `None` only for
        /// values minted by machinery with no allocation site (tests,
        /// scaffolding); region ids are never reused, so a bare id stays
        /// meaningful forever.
        home: Option<RegionId>,
    },
    /// A `List`. The second field is the element's integer checking context,
    /// when one is known: `List[i32]()` carries `Some(i32)`, `List[int]()`
    /// `Some(int)`, `List[str]()`/`List()` `None`. A pushed literal adopts
    /// this type — or `int` when none is known, exactly as every other
    /// unconstrained literal meeting a container does (issue #21, the #53
    /// mechanism) — so element loads feed checked arithmetic at the
    /// element's width (X3).
    List(Vec<Slot>, Option<IntTy>),
    /// Insertion-ordered; wolf's `Map` has no specified iteration order, and
    /// insertion order is the one that makes output reproducible.
    Map(Vec<(Value, Slot)>),
    Range {
        start: i128,
        end: i128,
        inclusive: bool,
        ty: IntTy,
    },
    /// A named function, by module-qualified name.
    Fn(String),
    Closure(Box<ClosureValue>),
    /// A tagged error, first-class (`[err.union]`). There is no unwinding
    /// anywhere in this machine: an error travels as data.
    Error(Box<ErrorValue>),
    /// A module namespace, so `geometry.area(3)` has something to project from.
    Module(String),
    /// An ambient builtin (`print`, `List`, `min`, …). The compiler track
    /// resolves the corpus against a names-only std stub; this is the same set
    /// with behavior attached, and it moves when the real std surface lands.
    Builtin(&'static str),

    // -- Tier 1 & 2: the granules with identity (is03) ---------------------
    /// `region()`, `region(rc)`, and the sugar's binder (`[mem.region.create.1]`).
    Region(RegionValue),
    /// A `Pool[T]`, anchored to the region that was current when it was built.
    PoolRef(PoolId),
    /// `handle T` (`[mem.shared.handle.1]`).
    Handle(HandleValue),
    /// A strong `shared T` reference (`[mem.shared.rc.1]`).
    Shared(CellId),
    /// A `weak T` reference, which keeps nothing alive (`[mem.shared.rc.3]`).
    Weak(CellId),

    // -- Tier 3: raw pointers (is04) ---------------------------------------
    /// A `*T` raw pointer: `(address, tag)` (`[mem.prov.tag]`). Copies of it
    /// are unrestricted and share its tag — `[mem.unsafe.raw.1]` says so, and
    /// that is exactly what makes the raw tier *simpler* than the safe one.
    Raw(super::prov::RawPtr),

    // -- is06: the concurrency granules (spec/03) --------------------------
    /// A structured-concurrency scope handle (`[conc.task.scope]`): an
    /// ordinary, passable value — the nursery-as-capability shape D16 names.
    Scope(usize),
    /// A typed channel (`[conc.chan.type]`). The value is a reference to a
    /// scheduler-owned endpoint pair; copies name the same channel.
    Chan(usize),
    /// A `Mutex(v)` — a `sync` wrapper (`[conc.mm.hb.mutex]`); copies share.
    MutexRef(usize),
    /// A proc handle (`[conc.proc.1]`): the failure domain, by id.
    Proc(usize),
    /// A duration, in nanoseconds — what `1.s` builds and `timeout(d)` reads
    /// (`[conc.select.timeout]`; virtual time under test).
    Duration(u128),
}

impl Value {
    /// Is this an error value? The only question `?` and `else` need to ask.
    #[must_use]
    pub fn is_error(&self) -> bool {
        matches!(self, Value::Error(_))
    }

    #[must_use]
    pub fn int(value: i128) -> Value {
        Value::Int(value, IntTy::LITERAL)
    }

    /// The name used in messages: a type-ish word, never a type-checker verdict.
    #[must_use]
    pub fn kind(&self) -> String {
        match self {
            Value::Unit => "()".to_owned(),
            Value::Bool(_) => "bool".to_owned(),
            Value::Int(_, ty) => ty.name(),
            Value::Float(_) => "f64".to_owned(),
            Value::Str(_) => "str".to_owned(),
            Value::Tuple(items) => format!("a {}-tuple", items.len()),
            Value::Struct { name, .. } => name.clone(),
            Value::List(..) => "List".to_owned(),
            Value::Map(_) => "Map".to_owned(),
            Value::Range { .. } => "a range".to_owned(),
            Value::Fn(name) => format!("fn {name}"),
            Value::Closure(_) => "a closure".to_owned(),
            Value::Error(e) => format!("error {}", e.tag),
            Value::Module(name) => format!("module {name}"),
            Value::Builtin(name) => format!("builtin {name}"),
            Value::Region(_) => "region".to_owned(),
            Value::PoolRef(_) => "Pool".to_owned(),
            Value::Handle(_) => "handle".to_owned(),
            Value::Shared(_) => "shared".to_owned(),
            Value::Weak(_) => "weak".to_owned(),
            Value::Raw(_) => "*T".to_owned(),
            Value::Scope(_) => "a scope handle".to_owned(),
            Value::Chan(_) => "channel".to_owned(),
            Value::MutexRef(_) => "Mutex".to_owned(),
            Value::Proc(_) => "a proc handle".to_owned(),
            Value::Duration(_) => "a duration".to_owned(),
        }
    }

    /// Truthiness is *not* a wolf concept — conditions are `bool` — so this is
    /// `None` for everything else and the caller reports a sema-lite gap.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_int(&self) -> Option<i128> {
        match self {
            Value::Int(v, _) => Some(*v),
            _ => None,
        }
    }
}

/// Rendering, which is what f-string interpolation needs (`[str.interp]`).
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Unit => f.write_str("()"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(v, _) => write!(f, "{v}"),
            Value::Float(v) => {
                // The SHORTEST decimal that reads back as the same bits, in
                // the layout `corpus/strings/float_format.lu` pins (s38) —
                // `3.0` renders `3`, `0.5` renders `0.5`, division by zero
                // is `inf` (a value, never a trap: X3 is integer law).
                f.write_str(&crate::fmtspec::f64_shortest(*v))
            }
            Value::Str(s) => f.write_str(s),
            Value::Tuple(items) => {
                f.write_str("(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", item.value)?;
                }
                f.write_str(")")
            }
            Value::Struct { name, fields, .. } => {
                write!(f, "{name} {{")?;
                for (i, (field, slot)) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, " {field}: {}", slot.value)?;
                }
                f.write_str(" }")
            }
            Value::List(items, _) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", item.value)?;
                }
                f.write_str("]")
            }
            Value::Map(pairs) => {
                f.write_str("{")?;
                for (i, (key, slot)) in pairs.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{key}: {}", slot.value)?;
                }
                f.write_str("}")
            }
            Value::Range {
                start,
                end,
                inclusive,
                ..
            } => write!(f, "{start}..{}{end}", if *inclusive { "=" } else { "" }),
            Value::Fn(name) | Value::Module(name) => f.write_str(name),
            Value::Builtin(name) => f.write_str(name),
            Value::Closure(_) => f.write_str("<closure>"),
            // Identity-carrying granules render as what they are: an id, and
            // the generation that makes staleness decidable. `[mem.region.create.4]`
            // says region identity has zero runtime representation in compiled
            // code — this is the dynamic machine's bookkeeping, not a value the
            // language promises to print.
            Value::Region(handle) => write!(f, "region#{}@{}", handle.id, handle.generation),
            Value::PoolRef(id) => write!(f, "pool#{id}"),
            Value::Handle(handle) => write!(
                f,
                "handle#{}[{}]@{}",
                handle.pool, handle.index, handle.generation
            ),
            Value::Shared(id) => write!(f, "shared#{id}"),
            Value::Weak(id) => write!(f, "weak#{id}"),
            Value::Raw(ptr) => write!(f, "{ptr}"),
            Value::Scope(id) => write!(f, "scope#{id}"),
            Value::Chan(id) => write!(f, "channel#{id}"),
            Value::MutexRef(id) => write!(f, "mutex#{id}"),
            Value::Proc(id) => write!(f, "proc#{id}"),
            Value::Duration(nanos) => write!(f, "{nanos}ns"),
            Value::Error(e) => {
                f.write_str(&e.tag)?;
                if !e.payload.is_empty() {
                    f.write_str("(")?;
                    for (i, item) in e.payload.iter().enumerate() {
                        if i > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{item}")?;
                    }
                    f.write_str(")")?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_ranges_are_the_two_complement_ones() {
        assert_eq!(IntTy::I32.range(), (-2_147_483_648, 2_147_483_647));
        assert_eq!(
            IntTy::named("u32").expect("u32").range(),
            (0, 4_294_967_295)
        );
        assert_eq!(IntTy::named("u8").expect("u8").range(), (0, 255));
        assert!(IntTy::I32.holds(2_147_483_647));
        assert!(!IntTy::I32.holds(2_147_483_648));
    }

    #[test]
    fn checked_types_refuse_to_reduce_and_wrapping_types_wrap() {
        assert_eq!(IntTy::I32.reduce(2_147_483_648), None);

        let wrapping = IntTy::named("u32")
            .expect("u32")
            .with_mode(ArithMode::Wrapping);
        assert_eq!(wrapping.reduce(4_294_967_296), Some(0));
        assert_eq!(wrapping.reduce(-1), Some(4_294_967_295));

        let signed = IntTy::I32.with_mode(ArithMode::Wrapping);
        assert_eq!(signed.reduce(2_147_483_648), Some(-2_147_483_648));

        let saturating = IntTy::I32.with_mode(ArithMode::Saturating);
        assert_eq!(saturating.reduce(9_999_999_999), Some(2_147_483_647));
        assert_eq!(saturating.reduce(-9_999_999_999), Some(-2_147_483_648));
    }

    #[test]
    fn a_move_leaves_the_slot_uninitialized() {
        let mut slot = Slot::live(Value::int(7));
        assert!(slot.is_live());
        let value = slot.take_value(Span::new(3, 4));
        assert_eq!(value, Value::int(7));
        assert_eq!(slot.state, SlotState::Moved(Span::new(3, 4)));
        assert!(!slot.is_live());
    }

    #[test]
    fn field_states_are_independent() {
        // "Whole-value moves, field-granular states: `a.x` can be `Moved` while
        // `a.y` is `Live`."
        let mut value = Value::Struct {
            name: "P".to_owned(),
            fields: vec![
                ("x".to_owned(), Slot::live(Value::int(1))),
                ("y".to_owned(), Slot::live(Value::int(2))),
            ],
            home: None,
        };
        let Value::Struct { fields, .. } = &mut value else {
            unreachable!()
        };
        fields[0].1.take_value(Span::new(0, 1));
        assert!(!fields[0].1.is_live());
        assert!(fields[1].1.is_live());
    }

    #[test]
    fn floats_render_shortest_round_trip() {
        // The s38 realignment: the 0.1.4 "integral floats keep a `.0`"
        // reading retired when `corpus/strings/float_format.lu` pinned the
        // shortest-decimal layout. The full grammar and its pins live in
        // `crate::fmtspec`; this asserts only that `Display` routes there.
        assert_eq!(Value::Float(1.0).to_string(), "1");
        assert_eq!(Value::Float(1.5).to_string(), "1.5");
        assert_eq!(Value::Float(0.5).to_string(), "0.5");
        assert_eq!(Value::Float(f64::INFINITY).to_string(), "inf");
        assert_eq!(Value::Float(-0.0).to_string(), "-0");
    }
}

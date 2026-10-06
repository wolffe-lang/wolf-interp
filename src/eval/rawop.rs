//! is74 — kw07's volatile access and kw11's atomics on `*T`
//! (wolf-interp#185, #194).
//!
//! `[mem.unsafe.volatile]`: `p.read_volatile()` / `p.write_volatile(v)` are
//! one access of the pointee's width each; on an allocation that access is
//! an ordinary one, under the same rows as `p[0]` (P1–P4, L1, L2), and an
//! address that is not a multiple of the width is row L3.
//!
//! `[conc.mm.atomic.raw]`: nine methods, each one indivisible operation on
//! the `T` at `p`, ordered by order operands that are SYNTAX — a mark
//! `Order.<mark>` read at the call, never an expression
//! (`[conc.mm.atomic.order]`). This machine is an interleaving machine: it
//! runs every order as `seq_cst`, each operation whole
//! (`[conc.mm.atomic.raw.5]`), so the order operand chooses nothing here and
//! is never evaluated. A misaligned operation is row L4 (`[mem.unsafe.raw.4]`).
//! `fence(o)` is in force already (`[conc.mm.fence]`: this machine runs one
//! operation at a time).
//!
//! The static half — E1307, E1308, E1309 and the ring's E1301 — is sema's
//! (`crate::rawops`). What reaches here unchecked (a receiver whose pointee
//! the static walk could not see) is declined by name with the code the
//! compiler gives, never run on a guess.

use crate::ast::{Arg, Expr, ExprKind};
use crate::diag::Span;
use crate::trap::TrapKind;

use super::prov::{AccessKind, Pointee, RawPtr, UbRow};
use super::rules::Rule;
use super::sched::RaceKey;
use super::value::{ArithMode, IntTy, Slot, Value};
use super::{EResult, Machine, unsupported};

/// The two volatile methods (`[mem.unsafe.volatile]`).
pub const VOLATILE: [&str; 2] = ["read_volatile", "write_volatile"];

/// The nine atomic methods (`[conc.mm.atomic.raw]`).
pub const ATOMIC: [&str; 9] = [
    "atomic_load",
    "atomic_store",
    "atomic_swap",
    "atomic_add",
    "atomic_sub",
    "atomic_and",
    "atomic_or",
    "atomic_xor",
    "atomic_cas",
];

/// The five marks of `Order` (`[conc.mm.atomic.order]`), C++20's set
/// without `consume`, strongest last.
pub const MARKS: [&str; 5] = ["relaxed", "acquire", "release", "acq_rel", "seq_cst"];

/// Whether `method` is one of the eleven raw-pointer methods this module runs.
#[must_use]
pub fn is_raw_method(method: &str) -> bool {
    VOLATILE.contains(&method) || ATOMIC.contains(&method)
}

/// The argument positions of an atomic method that are order operands: a
/// load's only argument, a compare-and-swap's last two, every other
/// method's second. `None` for a name that is not an atomic method.
#[must_use]
pub fn order_slots(method: &str) -> Option<&'static [usize]> {
    match method {
        "atomic_load" => Some(&[0]),
        "atomic_cas" => Some(&[2, 3]),
        m if ATOMIC.contains(&m) => Some(&[1]),
        _ => None,
    }
}

/// How many arguments an atomic method takes.
#[must_use]
pub fn atomic_arity(method: &str) -> usize {
    match method {
        "atomic_load" => 1,
        "atomic_cas" => 4,
        _ => 2,
    }
}

/// The mark an order operand spells, when it is written `Order.<mark>` with
/// one of the five marks — the only thing an order operand may be.
#[must_use]
pub fn mark_of(expr: &Expr) -> Option<&'static str> {
    let ExprKind::Path(path) = &*expr.kind else {
        return None;
    };
    match path.segments.as_slice() {
        [head, mark] if head.name == "Order" => {
            MARKS.iter().copied().find(|m| *m == mark.name.as_str())
        }
        _ => None,
    }
}

impl Machine {
    /// `p.<method>(args)` for a raw receiver and one of [`is_raw_method`]'s
    /// names. The receiver has been read; the operands run here, left to
    /// right after it (`[conc.mm.atomic.raw]`, `[mem.model.order]`), and an
    /// order operand is never run — it is a mark.
    pub(super) fn raw_method(
        &mut self,
        ptr: RawPtr,
        method: &str,
        args: &[Arg],
        span: Span,
    ) -> EResult<Value> {
        if VOLATILE.contains(&method) {
            return self.volatile(ptr, method, args, span);
        }
        self.atomic(ptr, method, args, span)
    }

    /// `[mem.unsafe.volatile]`: one access of the pointee's width at `p`.
    fn volatile(&mut self, ptr: RawPtr, method: &str, args: &[Arg], span: Span) -> EResult<Value> {
        let write = method == "write_volatile";
        if args.len() != usize::from(write) {
            return unsupported(format!(
                "`{method}` takes {} argument(s), got {}",
                usize::from(write),
                args.len()
            ));
        }
        if !matches!(ptr.kind, Pointee::Width | Pointee::Byte) {
            return unsupported(format!(
                "`{method}` on a pointer whose pointee is not a fixed-width integer or `byte` — \
                 the compiler's E1307 (`[mem.unsafe.volatile.1]`); the static walk could not see \
                 this receiver's pointee, so the call is declined, never run"
            ));
        }
        let stored = if write {
            match self.eval(&args[0].expr)? {
                Value::Int(v, _) => Some(v),
                Value::Byte(b) => Some(i128::from(b)),
                other => {
                    return unsupported(format!(
                        "`write_volatile` stores an integer or a `byte`, got {}",
                        other.kind()
                    ));
                }
            }
        } else {
            None
        };
        // `[mem.unsafe.volatile.3]`: L3 before any other row, as the
        // compiler's checked machine asks it — an allocation's address
        // alone decides it (allocations sit at `Provenance::STRIDE`
        // multiples); a pointer no allocation owns keeps its own row (L2).
        self.misaligned(ptr, UbRow::L3, method, span)?;
        let kind = if write {
            AccessKind::Write
        } else {
            AccessKind::Read
        };
        self.prov_access(ptr, ptr.elem, kind, span)?;
        if self.tracing.keeps(Rule::Volatile) {
            self.fire(
                Rule::Volatile,
                span,
                &format!("{method}: one {}-byte access at {ptr}", ptr.elem),
            );
        }
        if let Some(v) = stored {
            self.prov().store(ptr, ptr.elem, v);
            return Ok(Value::Unit);
        }
        let raw = self.prov().load(ptr, ptr.elem);
        if ptr.kind == Pointee::Byte {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            return Ok(Value::Byte((raw & 0xff) as u8));
        }
        Ok(Value::Int(raw, width_ty(ptr)))
    }

    /// `[conc.mm.atomic.raw]`: one indivisible operation, run as `seq_cst`.
    fn atomic(&mut self, ptr: RawPtr, method: &str, args: &[Arg], span: Span) -> EResult<Value> {
        let arity = atomic_arity(method);
        if args.len() != arity {
            return unsupported(format!(
                "`{method}` takes {arity} argument(s), got {}",
                args.len()
            ));
        }
        if ptr.kind != Pointee::Width {
            return unsupported(format!(
                "`{method}` on a pointer whose pointee is not a fixed-width integer — the \
                 compiler's E1308 (`[conc.mm.atomic.raw.1]`); the static walk could not see this \
                 receiver's pointee, so the call is declined, never run"
            ));
        }
        let slots = order_slots(method).unwrap_or(&[]);
        let mut operands = Vec::with_capacity(arity);
        for (i, arg) in args.iter().enumerate() {
            if slots.contains(&i) {
                if mark_of(&arg.expr).is_none() {
                    return unsupported(format!(
                        "an order operand of `{method}` that is not a mark `Order.<mark>` — the \
                         compiler's E1309 (`[conc.mm.atomic.order]`)"
                    ));
                }
                continue;
            }
            match self.eval(&arg.expr)? {
                Value::Int(v, _) => operands.push(v),
                other => {
                    return unsupported(format!(
                        "`{method}` takes integer operands, got {}",
                        other.kind()
                    ));
                }
            }
        }
        self.misaligned(ptr, UbRow::L4, method, span)?;
        let bits = ptr.elem * 8;
        if method == "atomic_store" {
            // A store reads nothing: an uninitialized word is no row for it.
            self.atomic_touch(ptr, AccessKind::Write, span)?;
            self.prov().store(ptr, ptr.elem, operands[0]);
            self.fire_atomic(method, ptr, span);
            return Ok(Value::Unit);
        }
        let old = self.atomic_touch(ptr, AccessKind::Read, span)?;
        let result = match method {
            "atomic_load" => Value::Int(old, width_ty(ptr)),
            "atomic_cas" => {
                let expected = wrap(operands[0], bits, ptr.signed);
                let wrote = old == expected;
                if wrote {
                    self.atomic_touch(ptr, AccessKind::Write, span)?;
                    self.prov().store(ptr, ptr.elem, operands[1]);
                }
                Value::Tuple(vec![
                    Slot::live(Value::Int(old, width_ty(ptr))),
                    Slot::live(Value::Bool(wrote)),
                ])
            }
            _ => {
                let v = operands[0];
                let new = match method {
                    "atomic_swap" => v,
                    "atomic_add" => old.wrapping_add(v),
                    "atomic_sub" => old.wrapping_sub(v),
                    "atomic_and" => old & v,
                    "atomic_or" => old | v,
                    _ => old ^ v,
                };
                self.atomic_touch(ptr, AccessKind::Write, span)?;
                // The store truncates to the width: the read-modify-writes
                // wrap there (two's complement), never trap `overflow`.
                self.prov().store(ptr, ptr.elem, new);
                Value::Int(old, width_ty(ptr))
            }
        };
        self.fire_atomic(method, ptr, span);
        Ok(result)
    }

    /// The trace line of one atomic operation, formatted only when traced
    /// (a counter runs hundreds of thousands of them).
    fn fire_atomic(&mut self, method: &str, ptr: RawPtr, span: Span) {
        if self.tracing.keeps(Rule::Atomic) {
            self.fire(
                Rule::Atomic,
                span,
                &format!("{method} at {ptr}: one indivisible operation, run as seq_cst"),
            );
        }
    }

    /// One half of an atomic operation's access: the race detector's atomic
    /// path (a sync on the location, a race only against a plain access),
    /// then the provenance rows. A read answers the value at `p`.
    fn atomic_touch(&mut self, ptr: RawPtr, kind: AccessKind, span: Span) -> EResult<i128> {
        if let Some(alloc) = ptr.alloc
            && self.shared.sched.ever_concurrent()
        {
            let lo = usize::try_from(ptr.offset.max(0)).unwrap_or(0);
            let report = self.shared.sched.atomic_access(
                self.task,
                RaceKey::Alloc(alloc),
                lo,
                lo.saturating_add(ptr.elem),
                kind == AccessKind::Write,
            );
            self.drain_sched();
            if let Some(report) = report {
                return self.trap(
                    TrapKind::Race,
                    Rule::RaceDetect,
                    span,
                    format!(
                        "data race: this atomic {} of alloc#{alloc} conflicts with an unordered \
                         plain {} by {} — two atomic accesses never race, an atomic and a plain \
                         one do ([conc.mm.race.1]); detection is exact at the interleaving this \
                         schedule realized",
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
        // One guard, one statement (`prov_access`'s rule): the access's
        // result leaves the lock before the match re-enters the machine.
        let access = self.prov().access(ptr, ptr.elem, kind, span);
        match access {
            Ok(()) => self.drain_prov(),
            Err(finding) => return self.ub(finding),
        }
        let value = self.prov().load(ptr, ptr.elem);
        Ok(value)
    }

    /// The alignment row a volatile (L3) or atomic (L4) access asks first:
    /// the address of a pointer with an allocation is a multiple of the
    /// pointee's width, or the row.
    fn misaligned(&mut self, ptr: RawPtr, row: UbRow, method: &str, span: Span) -> EResult<()> {
        let width = ptr.elem;
        if width <= 1 || ptr.alloc.is_none() {
            return Ok(());
        }
        let address = self.prov().address_of(ptr);
        if address.rem_euclid(width as i128) == 0 {
            return Ok(());
        }
        let licensed = match row {
            UbRow::L3 => {
                "the compiled tiers emit one aligned access of the width and no check (O11)"
            }
            _ => "the compiled tiers emit the aligned atomic instruction and no check (O12)",
        };
        self.ub_row(
            row,
            span,
            ptr.alloc,
            format!(
                "`{method}` of a {width}-byte pointee at address {address:#x}, which is not a \
                 multiple of {width}: {licensed}"
            ),
        )
    }

    /// `fence(o)` (`[conc.mm.fence]`): this machine runs one operation at a
    /// time, so every fence is in force already. The operand is a mark,
    /// never run.
    pub(super) fn fence(&mut self, args: &[Arg], span: Span) -> EResult<Value> {
        let [arg] = args else {
            return unsupported(format!("`fence` takes one order, got {}", args.len()));
        };
        match mark_of(&arg.expr) {
            Some("relaxed") | None => unsupported(
                "a `fence` whose order is `relaxed` or not a mark `Order.<mark>` — the compiler's \
                 E1309 (`[conc.mm.fence]`)",
            ),
            Some(mark) => {
                self.fire(
                    Rule::Atomic,
                    span,
                    &format!("fence(Order.{mark}): in force — one operation runs at a time here"),
                );
                Ok(Value::Unit)
            }
        }
    }
}

/// The integer type a raw pointer's pointee reads as.
fn width_ty(ptr: RawPtr) -> IntTy {
    IntTy {
        bits: u32::try_from(ptr.elem * 8).unwrap_or(8),
        signed: ptr.signed,
        mode: ArithMode::Checked,
        literal: false,
    }
}

/// `v` read at `bits` wide, as a load of that width would read it: the low
/// bits, sign-extended when the pointee is signed.
fn wrap(v: i128, bits: usize, signed: bool) -> i128 {
    if bits >= 128 {
        return v;
    }
    let low = v & ((1i128 << bits) - 1);
    if signed && (low >> (bits - 1)) & 1 == 1 {
        low - (1i128 << bits)
    } else {
        low
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compare_reads_the_expected_value_at_the_width() {
        assert_eq!(wrap(255, 8, false), 255);
        assert_eq!(wrap(-1, 8, false), 255);
        assert_eq!(wrap(255, 8, true), -1);
        assert_eq!(wrap(-128, 8, true), -128);
        assert_eq!(wrap(1 << 40, 32, false), 0);
        assert_eq!(wrap(i128::from(u64::MAX), 64, true), -1);
    }

    #[test]
    fn every_atomic_method_names_its_order_operands() {
        for method in ATOMIC {
            let slots = order_slots(method).expect("an atomic method");
            assert!(slots.iter().all(|&i| i < atomic_arity(method)), "{method}");
        }
        assert_eq!(order_slots("read_volatile"), None);
        assert_eq!(order_slots("push"), None);
    }
}

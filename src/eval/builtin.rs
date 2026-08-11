//! The ambient prelude, with behavior attached.
//!
//! The compiler track resolves the corpus against a **names-only std/prelude
//! stub** (s13's finding: "print, print_raw, List, Map, Pool, Mutex, channel,
//! min, the region APIs, std.fs.read_text, plus provisional stand-ins like
//! worker/acquire"). The interpreter needs the same ambient name set in order
//! to run the same corpus — so this module mirrors that stub and gives the
//! subset a Tier-0 machine can honour real semantics.
//!
//! **The real std surface is pending spec/lock, so this moves.** Two rules keep
//! that from turning into invented language:
//!
//! 1. A name in the stub that this module cannot implement *faithfully* is left
//!    unimplemented, and using it is `unsupported` with the reason attached.
//!    Inventing semantics for `acquire()`/`release()` would put wolf-interp's
//!    guesses into a differential comparison against wolfc's guesses, and the
//!    resulting "divergence" would be about nothing.
//! 2. Anything defined here that the spec *does* pin — bounds-checked slicing
//!    and indexing (D25, `[mem.ub.defined]`), `print`'s trailing newline
//!    (`corpus/hello.lu`'s directive) — cites the clause.

use crate::diag::Span;
use crate::trap::TrapKind;

use super::prov::UbRow;
use super::region::SlotLife;
use super::rules::Rule;
use super::value::{HandleValue, IntTy, Slot, Value};
use super::{Machine, Signal};

type BResult = Result<Value, Signal>;

fn unsupported(reason: impl Into<String>) -> BResult {
    Err(Signal::Unsupported(reason.into()))
}

/// Ambient single-segment names — the std stub this machine resolves against.
/// Public because sema's eager raise check must know what resolves.
pub const AMBIENT_NAMES: &[&str] = &[
    // Implemented here.
    "print",
    "print_raw",
    "List",
    "Map",
    "min",
    "max",
    "assert",
    // `Pool[T]()` is pinned: `[mem.shared.handle.1]` fixes the two-phase
    // shape ("`reserve()` yields a handle; `init(h, v)` fills it") and the
    // corpus locks the spelling, so is03 implements it.
    "Pool",
    // In the stub, and deliberately not implemented: their semantics are
    // not in any pinned document, so this machine declines rather than
    // guesses. Naming them still resolves the *name*, which keeps the
    // failure "unsupported feature" instead of "unknown name".
    "Mutex",
    "channel",
    "worker",
    "acquire",
    "release",
    "zip",
    "region",
];

/// Ambient single-segment names. `None` means "not in the stub", which the
/// caller reports as an unresolved name.
#[must_use]
pub fn ambient(name: &str) -> Option<Value> {
    AMBIENT_NAMES
        .iter()
        .find(|candidate| **candidate == name)
        .map(|candidate| Value::Builtin(candidate))
}

/// The C functions this machine models as **host intrinsics**.
///
/// # The approximation, stated where it lives
///
/// `corpus/memory/unsafe_noalias.lu`, `corpus/memory/unsafe_ub_uaf.lu` and
/// `corpus/ffi.lu` open with `import c "stdlib.h"` and then call `c.malloc`,
/// `c.memset` and `c.free`. There is no FFI here and there will not be one: an
/// interpreter that dlopen'd libc would be comparing the *host's* allocator
/// against the compiler's, and `[proto.cmp.defined-divergence]` already says
/// layout observations are not a comparison surface. So these are modelled —
/// `malloc` mints an allocation inside the provenance machine, `free` kills
/// one, `memset` writes bytes — and the model is the *only* thing the oracle
/// claims. `docs/approximation-contract.md` §8 is the contract; the set is
/// deliberately small, and a C name outside it is `unsupported`, never guessed.
///
/// `[mem.boundary.ffi]` is what makes the ownership honest: "a C call executes
/// against an implicit region borrowed for the call's extent", so a host
/// allocation is owned by the region that was current when it was made, and
/// `[mem.prov.region]` then decides what a region free does to it (§7/P4).
/// `[mem.prov.expose]`'s "wildcard pointers from FFI behave as exposed" is why
/// the root tag is exposed the moment it exists.
#[must_use]
pub fn c_intrinsic(name: &str) -> Option<&'static str> {
    const MODELLED: &[&str] = &["c.malloc", "c.calloc", "c.free", "c.memset", "c.memcpy"];
    MODELLED
        .iter()
        .copied()
        .find(|candidate| candidate.strip_prefix("c.") == Some(name))
}

/// Ambient dotted names: `std.fs.read_text`, and the `fs` binding `use std.fs`
/// leaves behind.
#[must_use]
pub fn ambient_dotted(head: &str, tail: &str) -> Option<Value> {
    match (head, tail) {
        ("std", "fs") | ("fs", "read_text") => Some(Value::Builtin("fs.read_text")),
        _ => None,
    }
}

/// Calls an ambient function.
///
/// # Errors
///
/// A trap, or `unsupported` for a stub name with no pinned semantics.
pub fn call(machine: &mut Machine, name: &str, args: Vec<Value>, span: Span) -> BResult {
    // `[conc.cancel.c]` / `[conc.ffi.external]`: a C frame is never unwound
    // or interrupted. This machine's C surface is host intrinsics that
    // complete synchronously, so "the next safe point after return" is simply
    // the task's next blocking point — noted where a concurrent program can
    // observe it.
    if name.starts_with("c.") && machine.concurrent() {
        machine.note(
            Rule::CancelFfi,
            span,
            "entering a C intrinsic: running-external, never interrupted; cancellation waits \
             for the next runtime-owned blocking point",
        );
    }
    match name {
        // `corpus/hello.lu`: "`print` appends a newline; stdout matching ignores
        // the trailing one."
        "print" => {
            let text = args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            machine.out(&format!("{text}\n"));
            Ok(Value::Unit)
        }
        "print_raw" => {
            let text = args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("");
            machine.out(&text);
            Ok(Value::Unit)
        }
        // Collection constructors are allocation sites (`[mem.model.alloc]`),
        // so they land in the current region (`[mem.region.create.3]`).
        "List" => {
            machine.allocate(span, "List");
            Ok(Value::List(Vec::new()))
        }
        "Map" => {
            machine.allocate(span, "Map");
            Ok(Value::Map(Vec::new()))
        }
        "Pool" => {
            // The pool is anchored to the region that is current when it is
            // built, and `Store::new_pool` charges the allocation itself.
            let current = machine.current_region();
            let elem = match machine
                .store()
                .region(current)
                .map(|region| region.strategy.clone())
            {
                Some(super::region::Strategy::Pool(ty)) => ty,
                _ => "_".to_owned(),
            };
            let pool = machine.store().new_pool(elem);
            machine.note(
                Rule::HandleTwoPhase,
                span,
                &format!("pool#{pool} built; slots are reserved then initialized"),
            );
            Ok(Value::PoolRef(pool))
        }
        "min" | "max" => {
            let mut best: Option<i128> = None;
            let mut ty = IntTy::LITERAL;
            for arg in &args {
                let Value::Int(v, t) = arg else {
                    return unsupported(format!("`{name}` takes integers, got {}", arg.kind()));
                };
                if !t.literal {
                    ty = *t;
                }
                best = Some(match best {
                    None => *v,
                    Some(current) => {
                        if (name == "min") == (*v < current) {
                            *v
                        } else {
                            current
                        }
                    }
                });
            }
            match best {
                Some(v) => Ok(Value::Int(v, ty)),
                None => unsupported(format!("`{name}` needs at least one argument")),
            }
        }
        "assert" => {
            // The indirect spelling (`assert` as a value, applied late). The
            // intrinsic's lazy-message form lives in `eval_call`
            // (`[conf.trap.assert]`); by the time a value application lands
            // here the arguments are already evaluated, so all that is left
            // of the contract is the failing-path rendering.
            let ok = args.first().and_then(Value::as_bool).unwrap_or(false);
            if ok {
                return Ok(Value::Unit);
            }
            if let Some(msg) = args.get(1) {
                machine.out(&format!("{msg}\n"));
            }
            machine.fault(
                TrapKind::Assert,
                Rule::Assert,
                span,
                "assertion failed".to_owned(),
            )
        }
        // -- spec/03: the concurrency constructors (is06) ------------------
        "channel" => {
            // `channel[T](n)`: capacity n ≥ 1 buffers, n = 0 (or absent) is
            // rendezvous (`[conc.chan.buf]`). The type argument was consumed
            // by the bracket application; sendability is E1102's static half.
            let cap = match args.first() {
                None => 0,
                Some(Value::Int(n, _)) if *n >= 0 => usize::try_from(*n).unwrap_or(0),
                Some(other) => {
                    return unsupported(format!(
                        "`channel[T](n)` takes a non-negative capacity, got {}",
                        other.kind()
                    ));
                }
            };
            machine.chan_new(cap, span)
        }
        "Mutex" => {
            // `Mutex(v)` — the `sync` wrapper (`[conc.mm.hb.mutex]`); `when`
            // is its only access surface in the pinned corpus.
            let value = args.into_iter().next().unwrap_or(Value::Unit);
            let id = machine.shared_mutex(value);
            machine.note(
                Rule::SchedAcquire,
                span,
                &format!("mutex#{id} created; acquisitions are totally ordered per sync object"),
            );
            Ok(Value::MutexRef(id))
        }
        // -- Tier 3: the modelled C intrinsics (see `c_intrinsic`) ---------
        "c.malloc" | "c.calloc" => {
            // `malloc(size)` is one byte count; `calloc(n, size)` is a COUNT
            // and an ELEMENT SIZE — the allocation is `n * size` bytes
            // (issue #13: modeling it as `n` bytes made `calloc(8, 8)` an
            // 8-byte block, and s29's native differential caught the 64-byte
            // memcpy that real glibc accepts).
            let size = if name == "c.calloc" {
                let count = c_size(&args, name)?;
                let elem = c_len(&args, 1, name)?;
                count.checked_mul(elem).ok_or_else(|| {
                    // Real calloc reports this overflow by returning NULL;
                    // the model has no null-returning surface pinned, so the
                    // honest verdict is unsupported, not an invented block.
                    Signal::Unsupported(format!(
                        "`c.calloc({count}, {elem})` overflows the size computation"
                    ))
                })?
            } else {
                c_size(&args, name)?
            };
            let region = machine.current_region();
            let ptr = machine.prov().host_alloc(size, region, span);
            if name == "c.calloc" {
                // `calloc` zeroes, which is *observable* here: §7/L1 is a read
                // of memory nothing wrote, and calloc wrote.
                machine.prov().init_range(ptr, size);
            }
            machine.note(
                Rule::BoundaryFfi,
                span,
                &format!("`{name}` allocates {size} byte(s) → {ptr}, exposed per the FFI posture"),
            );
            Ok(Value::Raw(ptr))
        }
        "c.free" => {
            let Some(Value::Raw(ptr)) = args.first().copied_raw() else {
                return unsupported("`c.free` takes a raw pointer".to_owned());
            };
            if machine.prov().host_free(ptr, span) {
                machine.note(
                    Rule::ProvRegion,
                    span,
                    &format!("`c.free({ptr})`: the allocation's whole tag tree is Disabled"),
                );
                return Ok(Value::Unit);
            }
            // A double free, a free of an interior pointer, or a free of
            // something that never came from the modelled heap. The row is the
            // dangling-raw-pointer one: `free` derefs the block it releases.
            machine.ub_from_builtin(
                UbRow::L2,
                span,
                ptr.alloc,
                format!(
                    "`c.free({ptr})` releases a block that is not a live allocation of the \
                     modelled C heap at offset 0"
                ),
            )
        }
        "c.memset" => {
            let Some(Value::Raw(ptr)) = args.first().copied_raw() else {
                return unsupported("`c.memset` takes a raw pointer".to_owned());
            };
            let byte = args.get(1).and_then(Value::as_int).unwrap_or(0);
            let len = c_len(&args, 2, name)?;
            machine.raw_fill(ptr, byte, len, span)?;
            machine.note(
                Rule::BoundaryFfi,
                span,
                &format!("`c.memset({ptr}, {byte}, {len})` — a checked write of the whole range"),
            );
            Ok(Value::Unit)
        }
        "c.memcpy" => {
            let (Some(Value::Raw(dst)), Some(Value::Raw(src))) =
                (args.first().copied_raw(), args.get(1).copied_raw())
            else {
                return unsupported("`c.memcpy` takes two raw pointers".to_owned());
            };
            let len = c_len(&args, 2, name)?;
            machine.raw_copy(dst, src, len, span)?;
            Ok(Value::Unit)
        }
        other => unsupported(format!(
            "`{other}` is in the ambient std stub but has no pinned semantics; the real std \
             surface is not specified yet, and guessing it would put invented behavior into a \
             differential comparison"
        )),
    }
}

/// Reads a member that is not a stored field: `xs.len`, `s.len`.
///
/// # Errors
///
/// `unsupported` when the receiver has no such member.
pub fn property(machine: &mut Machine, receiver: &Value, name: &str, span: Span) -> BResult {
    let _ = span;
    match (receiver, name) {
        (Value::List(items), "len") => Ok(Value::Int(items.len() as i128, IntTy::INT)),
        (Value::Map(pairs), "len") => Ok(Value::Int(pairs.len() as i128, IntTy::INT)),
        // D25: `str` is bytes, and `len` is a byte count — the same unit
        // slicing uses, so `s[..s.len]` is the whole string.
        // Duration constructors (`[conc.select.timeout]`): `1.s`, `20.ms`.
        // Member access on an int literal parses by `[gram.amb.intdot]`.
        (Value::Int(v, _), "s" | "ms" | "us" | "ns") if *v >= 0 => {
            let scale: u128 = match name {
                "s" => 1_000_000_000,
                "ms" => 1_000_000,
                "us" => 1_000,
                _ => 1,
            };
            let v = u128::try_from(*v).unwrap_or_default();
            Ok(Value::Duration(v.saturating_mul(scale)))
        }
        (Value::Str(s), "len") => Ok(Value::Int(s.len() as i128, IntTy::INT)),
        // Projection *through* an RC cell: `[mem.shared.rc.1]`'s cell holds the
        // payload, and reading a field of a `shared T` reads the payload's.
        // Writes through a `shared` are not implemented — there is no pinned
        // interior-mutability surface, and guessing one would put invented
        // behavior into a differential comparison.
        (Value::Shared(cell), field) => {
            let payload = match machine.store().cell(*cell) {
                Some(cell) if !cell.dead => cell.value.clone(),
                _ => {
                    return unsupported(format!(
                        "shared#{cell}'s payload was released; `[mem.shared.rc.1]` says a strong \
                         reference keeps it alive, so reaching a dead one is an interpreter bug"
                    ));
                }
            };
            machine.note(Rule::SharedRc, span, &format!("shared#{cell}.{field}"));
            property(machine, &payload, field, span)
        }
        (
            Value::Struct {
                name: ty, fields, ..
            },
            field,
        ) => match fields.iter().find(|(f, _)| f == field) {
            Some((_, slot)) => Ok(slot.value.clone()),
            None => {
                let _ = machine;
                unsupported(format!("`{ty}` has no field `{field}`"))
            }
        },
        (other, field) => unsupported(format!("{} has no member `{field}`", other.kind())),
    }
}

/// Calls a method on a receiver. `receiver` is mutable: a mutating method
/// changes it, and the caller stores the whole value back into the receiver's
/// place — value semantics all the way down (`[mem.model.value]`).
///
/// # Errors
///
/// A trap, or `unsupported` for a method outside this subset.
#[allow(clippy::too_many_lines)]
pub fn method(
    machine: &mut Machine,
    receiver: &mut Value,
    name: &str,
    args: Vec<Value>,
    span: Span,
) -> BResult {
    // A closure or function stored in a field is called, not dispatched.
    match (&mut *receiver, name) {
        (Value::List(items), "push") => {
            for arg in args {
                items.push(Slot::live(arg));
            }
            machine.note(Rule::Alloc, span, "List.push");
            Ok(Value::Unit)
        }
        (Value::List(items), "pop") => match items.pop() {
            Some(slot) => Ok(slot.value),
            None => machine.fault(
                TrapKind::Bounds,
                Rule::Bounds,
                span,
                "`pop` on an empty List".to_owned(),
            ),
        },
        (Value::List(items), "len" | "count") => Ok(Value::Int(items.len() as i128, IntTy::INT)),
        (Value::List(items), "is_empty") => Ok(Value::Bool(items.is_empty())),
        (Value::List(items), "get") => {
            let Some(Value::Int(index, _)) = args.first() else {
                return unsupported("`get` takes an integer index".to_owned());
            };
            match usize::try_from(*index).ok().and_then(|i| items.get(i)) {
                Some(slot) => Ok(slot.value.clone()),
                None => Ok(error("OutOfBounds")),
            }
        }

        (Value::Map(pairs), "len" | "count") => Ok(Value::Int(pairs.len() as i128, IntTy::INT)),
        (Value::Map(pairs), "is_empty") => Ok(Value::Bool(pairs.is_empty())),
        (Value::Map(pairs), "pairs") => Ok(Value::List(
            pairs
                .iter()
                .map(|(key, slot)| {
                    Slot::live(Value::Tuple(vec![
                        Slot::live(key.clone()),
                        Slot::live(slot.value.clone()),
                    ]))
                })
                .collect(),
        )),

        (Value::Str(s), "len") => Ok(Value::Int(s.len() as i128, IntTy::INT)),
        (Value::Str(s), "is_empty") => Ok(Value::Bool(s.is_empty())),
        (Value::Str(s), "upper") => Ok(Value::Str(s.to_uppercase())),
        (Value::Str(s), "lower") => Ok(Value::Str(s.to_lowercase())),
        (Value::Str(s), "trim") => Ok(Value::Str(match args.first() {
            Some(Value::Str(cut)) => s.trim_matches(|c| cut.contains(c)).to_owned(),
            _ => s.trim().to_owned(),
        })),
        (Value::Str(s), "repeat") => {
            let Some(Value::Int(n, _)) = args.first() else {
                return unsupported("`repeat` takes a count".to_owned());
            };
            Ok(Value::Str(
                s.repeat(usize::try_from(*n).unwrap_or_default()),
            ))
        }
        (Value::Str(s), "contains") => Ok(Value::Bool(match args.first() {
            Some(Value::Str(needle)) => s.contains(needle.as_str()),
            _ => false,
        })),
        (Value::Str(s), "starts_with") => Ok(Value::Bool(match args.first() {
            Some(Value::Str(prefix)) => s.starts_with(prefix.as_str()),
            _ => false,
        })),
        (Value::Str(s), "words") => Ok(Value::List(
            s.split_whitespace()
                .map(|word| Slot::live(Value::Str(word.to_owned())))
                .collect(),
        )),
        (Value::Str(s), "lines") => Ok(Value::List(
            s.lines()
                .map(|line| Slot::live(Value::Str(line.to_owned())))
                .collect(),
        )),
        (Value::Str(s), "to_int") => {
            // `-> !int`: a value, tagged. There is no unwinding (`[err.union]`).
            match s.trim().parse::<i128>() {
                Ok(v) => Ok(Value::Int(v, IntTy::INT)),
                Err(_) => {
                    machine.note(Rule::ErrUnion, span, "`to_int` yields an error value");
                    Ok(error("NotAnInt"))
                }
            }
        }

        // -- Tier 3: raw pointers (`[mem.unsafe.raw.1]`) -------------------
        (Value::Raw(ptr), "is_null") => Ok(Value::Bool(ptr.is_null())),
        (Value::Raw(ptr), "addr") => {
            let ptr = *ptr;
            machine.prov().expose(ptr, span);
            let address = machine.prov().address_of(ptr);
            machine.note(
                Rule::ProvExpose,
                span,
                &format!("{ptr}.addr exposes its tag"),
            );
            Ok(Value::Int(address, IntTy::INT))
        }

        (Value::Closure(_) | Value::Fn(_), "call") => {
            let target = receiver.clone();
            machine.invoke(target, args, span)
        }

        // -- spec/03: scopes, channels, procs (is06) -----------------------
        (Value::Scope(scope), "spawn") => {
            // `s.spawn(closure)` — task spawning is a *method* on a scope
            // handle; only procs have a keyword (`[gram.expr.conc]`, D16).
            let scope = *scope;
            match args.into_iter().next() {
                Some(Value::Closure(closure)) => machine.spawn_closure_task(scope, &closure, span),
                Some(Value::Fn(qualified)) => {
                    // `s.spawn(worker)` — a named function with no captures.
                    let closure = super::value::ClosureValue {
                        params: Vec::new(),
                        body: crate::ast::Expr {
                            kind: Box::new(crate::ast::ExprKind::Call {
                                callee: crate::ast::Expr {
                                    kind: Box::new(crate::ast::ExprKind::Path(crate::ast::Path {
                                        segments: qualified
                                            .split("::")
                                            .map(|part| crate::ast::Ident {
                                                name: part.to_owned(),
                                                span,
                                            })
                                            .collect(),
                                        span,
                                    })),
                                    span,
                                    anchor: "gram.expr.primary",
                                },
                                args: Vec::new(),
                            }),
                            span,
                            anchor: "gram.expr.primary",
                        },
                        captures: Vec::new(),
                    };
                    machine.spawn_closure_task(scope, &closure, span)
                }
                other => unsupported(format!(
                    "`spawn` takes a closure, got {}",
                    other.map_or_else(|| "nothing".to_owned(), |v| v.kind())
                )),
            }
        }
        (Value::Chan(chan), "send") => {
            let chan = *chan;
            let value = args.into_iter().next().unwrap_or(Value::Unit);
            machine.chan_send(chan, value, span)
        }
        (Value::Chan(chan), "recv") => {
            let chan = *chan;
            machine.chan_recv(chan, span)
        }
        (Value::Chan(chan), "close") => {
            let chan = *chan;
            machine.chan_close(chan, span)
        }
        (Value::Proc(proc), "monitor") => {
            let proc = *proc;
            machine.proc_monitor(proc, span)
        }
        (Value::Proc(proc), "kill") => {
            let proc = *proc;
            machine.proc_kill(proc, span)
        }
        (Value::Proc(proc), "link") => {
            let proc = *proc;
            // `a.link(b)` names the partner; `w.link()` couples with the
            // calling task's proc ([conc.proc.link.pair]).
            let other = match args.first() {
                Some(Value::Proc(other)) => Some(*other),
                None => None,
                Some(other) => {
                    return unsupported(format!(
                        "`link` couples procs: expected a proc argument, got {}",
                        other.kind()
                    ));
                }
            };
            machine.proc_link(proc, other, span)
        }
        (Value::Proc(proc), "cancel") => {
            let proc = *proc;
            machine.proc_cancel(proc, span)
        }
        // Exit-reason predicates (`[conc.proc.exit]`): the closed set as
        // structural tags, queried without a `match`.
        (Value::Error(err), "is_normal") => Ok(Value::Bool(err.tag == "normal")),
        (Value::Error(err), "is_error") => Ok(Value::Bool(err.tag == "error")),
        (Value::Error(err), "is_killed") => Ok(Value::Bool(err.tag == "killed")),
        (Value::Error(err), "is_cancelled") => Ok(Value::Bool(err.tag == "cancelled")),

        // -- Tier 2: pools and handles (`[mem.shared.handle]`) -------------
        (Value::PoolRef(pool), "reserve") => {
            let pool = *pool;
            // Reserving is a write into the pool's region.
            machine.check_pool_region(pool, span, true)?;
            let Some((index, generation)) = machine.store().reserve(pool) else {
                return unsupported(format!("pool#{pool} does not exist"));
            };
            machine.note(
                Rule::HandleTwoPhase,
                span,
                &format!("reserve pool#{pool}[{index}] at generation {generation}"),
            );
            Ok(Value::Handle(HandleValue {
                pool,
                index,
                generation,
            }))
        }
        (Value::PoolRef(pool), "init") => {
            let pool = *pool;
            machine.check_pool_region(pool, span, true)?;
            let (handle, value) = two_phase_args(&args, "init")?;
            machine.race_check_pool(pool, handle.index, true, span)?;
            // The slot is region data, so §4's store goes through §3's edge
            // table exactly like a struct field does.
            let owner = machine.store().pool_region(pool);
            machine.check_edge_into(owner, &value, span, "pool slot")?;
            if machine
                .store()
                .init_slot(pool, handle.index, handle.generation, value)
            {
                machine.note(
                    Rule::HandleTwoPhase,
                    span,
                    &format!("init pool#{pool}[{}]", handle.index),
                );
                Ok(Value::Unit)
            } else {
                machine.stale_handle(handle, span)
            }
        }
        (Value::PoolRef(pool), "remove") => {
            let pool = *pool;
            machine.check_pool_region(pool, span, true)?;
            let Some(handle) = handle_arg(&args) else {
                return unsupported("`remove` takes a handle".to_owned());
            };
            machine.race_check_pool(pool, handle.index, true, span)?;
            if machine
                .store()
                .remove_slot(pool, handle.index, handle.generation)
            {
                machine.note(
                    Rule::HandleStale,
                    span,
                    &format!(
                        "pool#{pool}[{}] freed; its generation bumped and every outstanding \
                         handle to it is now stale",
                        handle.index
                    ),
                );
                Ok(Value::Unit)
            } else {
                machine.stale_handle(handle, span)
            }
        }
        (Value::PoolRef(pool), "get") => {
            let pool = *pool;
            let Some(handle) = handle_arg(&args) else {
                return unsupported("`get` takes a handle".to_owned());
            };
            let _ = pool;
            machine.read_slot(handle, span)
        }
        (Value::PoolRef(pool), "len" | "count") => {
            let count = machine
                .store()
                .pool(*pool)
                .map(|p| p.slots.iter().filter(|s| s.life == SlotLife::Live).count())
                .unwrap_or_default();
            Ok(Value::Int(count as i128, IntTy::INT))
        }

        // -- Tier 2: `shared` and `weak` (`[mem.shared.rc]`) ---------------
        (Value::Shared(cell), "clone") => {
            let cell = *cell;
            if machine.store().retain(cell) {
                machine.note(
                    Rule::SharedRc,
                    span,
                    &format!("shared#{cell} cloned: one more strong owner"),
                );
                Ok(Value::Shared(cell))
            } else {
                unsupported(format!("shared#{cell} has no payload to clone"))
            }
        }
        (Value::Shared(cell), "downgrade") => {
            let cell = *cell;
            machine.store().downgrade(cell);
            machine.note(
                Rule::SharedWeak,
                span,
                &format!("shared#{cell} downgraded: a weak edge keeps nothing alive"),
            );
            Ok(Value::Weak(cell))
        }
        (Value::Shared(cell), "strong_count") => {
            let count = machine.store().cell(*cell).map_or(0, |c| c.strong);
            Ok(Value::Int(i128::from(count), IntTy::INT))
        }
        (Value::Weak(cell), "upgrade") => {
            let cell = *cell;
            if machine.store().upgrade(cell) {
                machine.note(
                    Rule::SharedWeak,
                    span,
                    &format!("weak#{cell} upgraded: the payload is still alive"),
                );
                Ok(Value::Shared(cell))
            } else {
                // `[mem.shared.rc.3]`: "upgrading yields an **option-shaped**
                // result the caller must handle". The clause names the shape
                // and not the tag; `None` is the shape's own word, and the
                // corpus handles it with a wildcard `else |_|`, so nothing
                // observable turns on the choice. Recorded as an open question
                // in `docs/approximation-contract.md`.
                machine.note(
                    Rule::SharedWeak,
                    span,
                    &format!("weak#{cell} upgraded to nothing: the payload is gone"),
                );
                Ok(error("None"))
            }
        }

        // -- Tier 1: region values (`[mem.region]`) -------------------------
        (Value::Region(handle), "is_closed") => {
            let state = machine.store().state(handle.id);
            let label = machine.store().label(handle.id);
            let detail = format!(
                "{label} is {}",
                state.map_or_else(|| "gone".to_owned(), |state| state.to_string())
            );
            machine.note(Rule::RegionOpen, span, &detail);
            Ok(Value::Bool(!matches!(
                state,
                Some(super::region::RegionState::Open)
            )))
        }
        (Value::Region(handle), "is_frozen") => {
            let frozen =
                machine.store().state(handle.id) == Some(super::region::RegionState::Frozen);
            Ok(Value::Bool(frozen))
        }

        (receiver, name) => unsupported(format!(
            "`{}` has no method `{name}` in this machine's std subset",
            receiver.kind()
        )),
    }
}

/// `e[i]` where `e` turned out to be a collection.
///
/// # Errors
///
/// `trap(bounds)` for an out-of-range index (`[mem.ub.defined]`: "OOB index /
/// split-code-point slice (D25) → trap `bounds`"), `unsupported` otherwise.
pub fn index(machine: &mut Machine, target: &Value, index: &Value, span: Span) -> BResult {
    match (target, index) {
        (Value::List(items) | Value::Tuple(items), Value::Int(i, _)) => {
            match usize::try_from(*i).ok().and_then(|i| items.get(i)) {
                Some(slot) => Ok(slot.value.clone()),
                None => machine.fault(
                    TrapKind::Bounds,
                    Rule::Bounds,
                    span,
                    format!(
                        "index {i} is outside a collection of {} element(s)",
                        items.len()
                    ),
                ),
            }
        }
        // `pool[h]` — the checked slot access of `[mem.shared.handle.3]`. Every
        // deref compares generations; a stale one is `stale-handle`, in every
        // profile (`[mem.shared.handle.2]`).
        (Value::PoolRef(pool), Value::Handle(handle)) => {
            if handle.pool != *pool {
                return unsupported(format!(
                    "a handle into pool#{} was used against pool#{pool}; handles are indices into \
                     one pool",
                    handle.pool
                ));
            }
            machine.read_slot(*handle, span)
        }
        (Value::Map(pairs), key) => match pairs.iter().find(|(k, _)| k == key) {
            Some((_, slot)) => Ok(slot.value.clone()),
            // "absent key defaults to zero value" — the idiom `tally[w] += 1`
            // relies on it, and `Unit` is what the compound-assignment path
            // reads as "zero of whatever type this is".
            None => Ok(Value::Unit),
        },
        (Value::Str(_), Value::Int(_, _)) => unsupported(
            "there is no `s[i]` character indexing in wolf (D25); slice with a range".to_owned(),
        ),
        (Value::Raw(_), _) => unsupported(
            "a raw index is provenance-checked in `eval` before it reaches the std subset"
                .to_owned(),
        ),
        (target, index) => unsupported(format!(
            "{} cannot be indexed by {}",
            target.kind(),
            index.kind()
        )),
    }
}

/// `s[a..b]` — byte-offset checked slicing (D25).
///
/// # Errors
///
/// `trap(bounds)` when an endpoint is outside the string or lands inside a
/// UTF-8 code point ("split-code-point slice", `[mem.ub.defined]`).
pub fn slice(
    machine: &mut Machine,
    target: &Value,
    start: Option<i128>,
    end: Option<i128>,
    inclusive: bool,
    span: Span,
) -> BResult {
    match target {
        Value::Str(s) => {
            let len = s.len() as i128;
            let from = start.unwrap_or(0);
            let to = end.map_or(len, |e| if inclusive { e + 1 } else { e });
            if from < 0 || to > len || from > to {
                return machine.fault(
                    TrapKind::Bounds,
                    Rule::Bounds,
                    span,
                    format!("byte range {from}..{to} is outside a {len}-byte string"),
                );
            }
            let (from, to) = (from as usize, to as usize);
            if !s.is_char_boundary(from) || !s.is_char_boundary(to) {
                return machine.fault(
                    TrapKind::Bounds,
                    Rule::Bounds,
                    span,
                    format!("byte range {from}..{to} splits a UTF-8 code point"),
                );
            }
            Ok(Value::Str(s[from..to].to_owned()))
        }
        Value::List(items) => {
            let len = items.len() as i128;
            let from = start.unwrap_or(0);
            let to = end.map_or(len, |e| if inclusive { e + 1 } else { e });
            if from < 0 || to > len || from > to {
                return machine.fault(
                    TrapKind::Bounds,
                    Rule::Bounds,
                    span,
                    format!("range {from}..{to} is outside a {len}-element List"),
                );
            }
            Ok(Value::List(items[from as usize..to as usize].to_vec()))
        }
        other => unsupported(format!("{} cannot be sliced", other.kind())),
    }
}

/// A `size_t`-shaped argument.
fn c_size(args: &[Value], name: &str) -> Result<usize, Signal> {
    match args.first().and_then(Value::as_int) {
        Some(n) if n >= 0 => usize::try_from(n).map_err(|_| {
            Signal::Unsupported(format!(
                "`{name}` was asked for {n} bytes, which does not fit"
            ))
        }),
        _ => Err(Signal::Unsupported(format!(
            "`{name}` takes a non-negative byte count"
        ))),
    }
}

fn c_len(args: &[Value], at: usize, name: &str) -> Result<usize, Signal> {
    match args.get(at).and_then(Value::as_int) {
        Some(n) if n >= 0 => usize::try_from(n)
            .map_err(|_| Signal::Unsupported(format!("`{name}`'s length {n} does not fit"))),
        _ => Err(Signal::Unsupported(format!(
            "`{name}` takes a non-negative length"
        ))),
    }
}

/// `Option<&Value>` → `Option<Value>` for the `Copy`-shaped raw pointer, so the
/// call sites above read as one `let … else`.
trait CopiedRaw {
    fn copied_raw(self) -> Option<Value>;
}

impl CopiedRaw for Option<&Value> {
    fn copied_raw(self) -> Option<Value> {
        match self {
            Some(Value::Raw(ptr)) => Some(Value::Raw(*ptr)),
            _ => None,
        }
    }
}

fn error(tag: &str) -> Value {
    Value::Error(Box::new(super::value::ErrorValue {
        tag: tag.to_owned(),
        payload: Vec::new(),
    }))
}

/// The handle argument of `remove(h)` / `get(h)`.
fn handle_arg(args: &[Value]) -> Option<HandleValue> {
    match args.first() {
        Some(Value::Handle(handle)) => Some(*handle),
        _ => None,
    }
}

/// `init(h, v)`'s two arguments — phase two of `[mem.shared.handle.1]`.
fn two_phase_args(args: &[Value], name: &str) -> Result<(HandleValue, Value), Signal> {
    match (args.first(), args.get(1)) {
        (Some(Value::Handle(handle)), Some(value)) => Ok((*handle, value.clone())),
        _ => Err(Signal::Unsupported(format!(
            "`{name}` takes a handle and a value; pools are two-phase \
             (`[mem.shared.handle.1]`)"
        ))),
    }
}

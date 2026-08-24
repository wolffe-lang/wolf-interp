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

/// s40 env v0: a name the platform can hold — nonempty, no `=`, no NUL.
fn env_name_valid(name: &str) -> bool {
    !name.is_empty() && !name.contains('=') && !name.contains('\0')
}

/// Ambient single-segment names — the std stub this machine resolves against.
/// Public because sema's eager raise check must know what resolves.
pub const AMBIENT_NAMES: &[&str] = &[
    // Implemented here.
    "print",
    "print_raw",
    // The s38 stderr writers: same fmt machinery, the OTHER stream —
    // stdout stays clean (`corpus/io/eprint.lu` is the pin; the record
    // hashes stdout only, stderr is the rich human channel, spec/06).
    "eprint",
    "eprint_raw",
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
    // The s38 io/fs surface (wolf-interp#18 item 6): the names resolve so
    // the refusal is "unsupported feature", never "unknown name" — this
    // machine has no filesystem by design (see `call`'s fs arm).
    "fs_read_text",
    "fs_write_text",
    "fs_open",
    "fs_remove",
    "fs_exists",
    "read_line",
    // The s40 os/env/time tier (0.1.7): env and time are implemented (the
    // checked-lane posture — overlay env, empty argv, X12 monotonic
    // anchor); the process trio is exec surface this machine declines by
    // design; the json kernels are wolf_mem's reference and are declined
    // rather than reimplemented-and-guessed.
    "env_args",
    "env_get",
    "env_set",
    "os_cwd",
    "os_exit",
    "os_spawn",
    "os_wait",
    "os_kill",
    "time_now_ms",
    "time_sleep_ms",
    "time_unix_ms",
    "json_valid",
    "json_get",
    "json_type",
    "json_len",
    // s81 (wolf-lang#58): the byte SOURCE. s77 gave the language a byte
    // VIEW (`s.bytes()`) and deliberately no way back, because an unchecked
    // bytes-to-str path is the forging hole — "every `str` is valid UTF-8"
    // has to survive CONSTRUCTION, not only narrowing. This is the only
    // entry in the language that builds a `str` out of arbitrary numbers,
    // and it validates; its refusal is the `utf8` row, never a trap and
    // never a cast. Pure — no capability, no sandbox category.
    "str_from_utf8",
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
        // The s38 stderr writers (`corpus/io/eprint.lu`): `eprint` mirrors
        // `print` (trailing newline; `eprint_raw` appends nothing) onto the
        // OTHER stream. One fmt machinery, two fds — the argument was
        // rendered by the same interpolation path `print`'s was. stderr is
        // the rich human channel and is never hashed or compared (spec/06),
        // so it follows stdout's pass-through discipline: the host stream is
        // written under `lupin run`'s live mode and stays quiet inside
        // embedded observers (the corpus walk, the differ, the explorer) —
        // a harness's own stderr is not the program's channel.
        "eprint" => {
            let text = args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            machine.err_out(&format!("{text}\n"));
            Ok(Value::Unit)
        }
        "eprint_raw" => {
            let text = args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("");
            machine.err_out(&text);
            Ok(Value::Unit)
        }
        // The s38 fs family and `read_line` (wolf-interp#18 item 6): this
        // machine has no filesystem and no stdin by design — an interpreter
        // observing the HOST's filesystem would put the host into a
        // differential comparison, and `[proto.cmp.defined-divergence]`
        // already rules that surface out. The honest verdict is
        // `unsupported` with the construct named, never a mock.
        "fs_read_text" | "fs_write_text" | "fs_open" | "fs_remove" | "fs_exists" | "read_line" => {
            unsupported(format!(
                "`{name}` is the s38 io/fs surface; this machine has no filesystem (or \
                 injectable stdin) by design, so the fs tier is declined rather than mocked"
            ))
        }
        // -- the s40 os/env/time tier (0.1.7) ------------------------------
        //
        // env v0: the machine-local OVERLAY — `env_set` writes here and
        // `env_get` reads here, never the host's real environment, so the
        // same program observes the same answers on any machine (the
        // checked-lane posture; wolfgang's checked machine does the same).
        // argv defaults empty (the stdin posture, mirrored — real argv is
        // `wolf run file.lu a b c`'s, a surface this embedding lacks).
        // Machinery-minted (the corpus posture: argv defaults empty), no
        // user allocation site — no home.
        "env_args" => Ok(Value::list(Vec::new(), None, None)),
        "env_get" => {
            let Some(Value::Str(name)) = args.first() else {
                return unsupported("`env_get` takes a variable name".to_owned());
            };
            if !env_name_valid(name) {
                machine.note(Rule::ErrUnion, span, "`env_get` yields the `invalid` row");
                return Ok(error("invalid"));
            }
            match machine.env_read(name) {
                Some(value) => Ok(Value::Str(value)),
                None => {
                    machine.note(Rule::ErrUnion, span, "`env_get` yields the `missing` row");
                    Ok(error("missing"))
                }
            }
        }
        "env_set" => {
            let (Some(Value::Str(name)), Some(Value::Str(value))) = (args.first(), args.get(1))
            else {
                return unsupported("`env_set` takes a name and a value".to_owned());
            };
            if !env_name_valid(name) {
                machine.note(Rule::ErrUnion, span, "`env_set` yields the `invalid` row");
                return Ok(error("invalid"));
            }
            machine.env_write(name, value);
            Ok(Value::Unit)
        }
        // The current directory is process state like env, not the fs tier:
        // no file is opened or observed. The corpus asserts predicates over
        // it, never paths (host independence).
        "os_cwd" => match std::env::current_dir() {
            Ok(dir) => Ok(Value::Str(dir.to_string_lossy().into_owned())),
            Err(_) => {
                machine.note(Rule::ErrUnion, span, "`os_cwd` yields the `io` row");
                Ok(error("io"))
            }
        },
        // `os_exit` (s40): immediate termination with the code — defers do
        // NOT run (the documented contract), and the code masks to the
        // process range identically on both lanes. Inside a task tier the
        // builtin refuses: supervised teardown is the proc tier's job.
        "os_exit" => {
            let Some(Value::Int(code, _)) = args.first() else {
                return unsupported("`os_exit` takes an integer status".to_owned());
            };
            if machine.concurrent() {
                return unsupported(
                    "`os_exit` inside the task tier: supervised teardown is the proc                      tier's job ([conc.proc.kill])"
                        .to_owned(),
                );
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Err(Signal::Exit(code.rem_euclid(256) as u8))
        }
        // The process trio is exec surface: this machine spawns, reaps and
        // signals no processes by design — the checked-lane twins live in
        // the counterparty's test tier. Declined, never mocked.
        "os_spawn" | "os_wait" | "os_kill" => unsupported(format!(
            "`{name}` is the s40 process trio (exec surface); this machine runs no child \
             processes by design, so the tier is declined rather than mocked"
        )),
        // The time trio needs a clock and a way to block, and wasm has
        // neither to call: `Instant`, `SystemTime` and `thread::sleep` all
        // abort the module. A guessed clock would make the tier *look*
        // present and report numbers no wolf program should trust, so the
        // surface is declined here exactly as the process trio is declined
        // above (`[proto.record.unsupported]`).
        #[cfg(target_family = "wasm")]
        "time_now_ms" | "time_sleep_ms" | "time_unix_ms" => unsupported(format!(
            "`{name}` is s40's time v0; this wasm build has no clock to read and no way to \
             block, so the tier is declined rather than mocked"
        )),
        // time v0 (s40, the X12 posture): monotonic ms from a process-local
        // anchor — values compare and subtract, never wall timestamps.
        #[cfg(not(target_family = "wasm"))]
        "time_now_ms" => Ok(Value::Int(machine.monotonic_ms(), IntTy::INT)),
        #[cfg(not(target_family = "wasm"))]
        "time_sleep_ms" => {
            let Some(Value::Int(ms, _)) = args.first() else {
                return unsupported("`time_sleep_ms` takes a duration in ms".to_owned());
            };
            if *ms > 0 {
                let capped = u64::try_from(*ms).unwrap_or(u64::MAX);
                std::thread::sleep(std::time::Duration::from_millis(capped));
            }
            Ok(Value::Unit)
        }
        #[cfg(not(target_family = "wasm"))]
        "time_unix_ms" => {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i128)
                .unwrap_or(0);
            Ok(Value::Int(ms, IntTy::INT))
        }
        // std.x.json's kernels are wolf_mem's reference parser, pinned by
        // its module doc — a surface this machine declines rather than
        // reimplements-and-guesses ([proto.record.unsupported]).
        "json_valid" | "json_get" | "json_type" | "json_len" => unsupported(format!(
            "`{name}` is std.x.json's s40 query tier; its reference kernel is the \
             counterparty's `wolf_mem::json`, and this machine declines the surface rather \
             than risk a second, guessed RFC 8259 reading"
        )),
        // `str_from_utf8(b: List[int]) -> str ! {utf8}` (s81, wolf-lang#58) —
        // the str-construction border, and the ONLY way a wolf program builds
        // a `str` out of numbers.
        //
        // It VALIDATES, which is the whole reason it exists: s77 declined an
        // unchecked bytes-to-str path because that is the forging hole, and
        // every other str-producing entry only ever narrows an ALREADY-valid
        // `str` on code-point boundaries. A cast would have made "every `str`
        // is valid UTF-8" a hope instead of an invariant.
        //
        // Two refusals, in this order, and the order is observable because
        // the first one is not UTF-8's:
        //
        // 1. An element outside `0..=255` is not a byte at all — `List[int]`
        //    holds `int`s, so `300` and `-1` are reachable — and is refused
        //    before the decoder ever sees the sequence.
        // 2. The byte sequence then goes through Rust's `String::from_utf8`,
        //    which is the same reference the counterparty uses on both of its
        //    lanes (`wolf_mem::ubcheck::str_from_utf8` calls it; the native
        //    lane's `__wolf_rt_str_from_utf8` calls `core::str::from_utf8`).
        //    So the refused set here is exactly UTF-8's: a lone continuation
        //    byte, a truncated multi-byte sequence, an overlong encoding, a
        //    surrogate (U+D800..U+DFFF), and a scalar past U+10FFFF.
        //
        // An interior NUL is VALID text and is accepted — a wolf `str` carries
        // its length, so nothing terminates — and the empty list decodes to
        // the empty string. Both are pinned by
        // `corpus/strings/from_utf8_border.lu`.
        //
        // The failure is the `utf8` ROW with an EMPTY payload, matching
        // `Value::ErrTag { tag: "utf8", payload: Vec::new() }` on the checked
        // lane and rc≠0 on the native one. Bytes off a socket are data, and
        // mis-encoded data is an outcome a caller handles with `else`.
        "str_from_utf8" => {
            let Some(Value::List(items, _, _)) = args.first() else {
                return unsupported("`str_from_utf8` takes a `List[int]` of bytes".to_owned());
            };
            let mut bytes: Vec<u8> = Vec::with_capacity(items.len());
            for slot in items.iter() {
                let Value::Int(n, _) = slot.value else {
                    // Unreachable from typed source — sema types the argument
                    // `List[int]` — so this is the counterparty's `refuse`
                    // ("unmodelled"), NOT the `utf8` row. Conflating the two
                    // would answer a row where the compiler answers nothing.
                    return unsupported(format!(
                        "`str_from_utf8` was given a list holding {}, not bytes",
                        slot.value.kind()
                    ));
                };
                match u8::try_from(n) {
                    Ok(byte) => bytes.push(byte),
                    Err(_) => {
                        machine.note(
                            Rule::ErrUnion,
                            span,
                            "`str_from_utf8` yields the `utf8` row",
                        );
                        return Ok(error("utf8"));
                    }
                }
            }
            match String::from_utf8(bytes) {
                Ok(text) => {
                    machine.allocate(span, "str_from_utf8");
                    Ok(Value::Str(text))
                }
                Err(_) => {
                    machine.note(
                        Rule::ErrUnion,
                        span,
                        "`str_from_utf8` yields the `utf8` row",
                    );
                    Ok(error("utf8"))
                }
            }
        }
        // Collection constructors are allocation sites (`[mem.model.alloc]`),
        // so they land in the current region (`[mem.region.create.3]`).
        "List" => {
            let home = machine.allocate(span, "List");
            // The element checking context (issue #21) is stamped by the
            // caller (`eval_call`), which alone sees the constructor's
            // bracket type argument. The charged region is the value's home
            // (#25): accesses consult its state from here on.
            Ok(Value::list(Vec::new(), None, Some(home)))
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
            // `channel[T](n)`: capacity n ≥ 1 buffers, n = 0 is rendezvous
            // (`[conc.chan.buf]`); an ABSENT capacity is rendezvous by
            // `[conc.chan.default]` — the clause the 13b811f pin added to
            // adopt this machine's behavior as normative (the bs06 ledger's
            // spec-gap row: this default predates the clause).
            // The type argument was consumed by the bracket application;
            // sendability is E1102's static half.
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
        //
        // The modelled set's real signatures (wolf-interp#18 item 4):
        // `malloc(size)`, `calloc(count, size)`, `free(ptr)`,
        // `memset(ptr, byte, len)`, `memcpy(dst, src, len)` — exact arity,
        // `size_t` arguments are non-negative integers, pointer arguments
        // are raw pointers. A call shaped otherwise is refused with the
        // shape named; C would have coerced silently, and silently is how
        // the ch09 differential caught this machine accepting it.
        "c.malloc" | "c.calloc" => {
            c_arity(name, &args, if name == "c.calloc" { 2 } else { 1 })?;
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
            c_arity(name, &args, 1)?;
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
            c_arity(name, &args, 3)?;
            let Some(Value::Raw(ptr)) = args.first().copied_raw() else {
                return unsupported("`c.memset` takes a raw pointer".to_owned());
            };
            // The byte is an `int` in the modelled signature; defaulting a
            // non-int to 0 was the silent coercion #18 item 4 retired.
            let Some(byte) = args.get(1).and_then(Value::as_int) else {
                return unsupported(format!(
                    "`c.memset`'s byte argument must be an integer, got {}",
                    args.get(1)
                        .map_or_else(|| "nothing".to_owned(), Value::kind)
                ));
            };
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
            c_arity(name, &args, 3)?;
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
        (Value::List(items, _, _), "len") => Ok(Value::Int(items.len() as i128, IntTy::INT)),
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

/// Whether [`method`] can change `receiver` in place — that is, whether the
/// arm it dispatches to takes the receiver's elements mutably.
///
/// Exactly two arms of [`method`] do: `List.push` and `List.pop`. Every other
/// arm either reads its receiver and returns a fresh value, or changes
/// *machine* state (the store, the provenance forest, the scheduler) behind a
/// receiver that is an immutable id.
///
/// The caller uses this to know whether the end of a lend is a write. Both
/// answers are safe in the direction that matters: the lend restores the
/// value either way, so a method wrongly called read-only still lands its
/// mutation — it would only skip the write's checks. The `debug_assertions`
/// arm of [`Machine::eval_method`](super::Machine::eval_method) compares this
/// against the whole-value comparison on every method call the test suite and
/// the corpus make, so drift here is caught rather than deduced.
#[must_use]
pub fn mutates_receiver(receiver: &Value, name: &str) -> bool {
    matches!((receiver, name), (Value::List(..), "push" | "pop"))
}

/// The O(1) witness for [`mutates_receiver`]'s two methods: `List.push` and
/// `List.pop` change the element count exactly when they change the value, so
/// the count is a complete stand-in for comparing the whole list.
#[must_use]
pub fn list_len(value: &Value) -> Option<usize> {
    match value {
        Value::List(items, _, _) => Some(items.len()),
        _ => None,
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
        (Value::List(items, elem, home), "push") => {
            // The receiver's home is consulted before anything diverges
            // (#25): `push` writes into the region's storage, so a `Freed`
            // or `Frozen` home faults here — the same consult a struct
            // field write runs — and a trapping push mutates nothing.
            machine.check_home_write(*home, "this `push`", span)?;
            // The write path diverges a shared CoW list before mutating
            // (#28): a caller holding the same spine observes nothing.
            let items = std::sync::Arc::make_mut(items);
            // Issue #21 (the #53 mechanism): a container-element literal
            // adopts the container's element type — or `int` (64-bit,
            // locked: `[arith.literal.default]`'s container half) when the
            // container carries none — like every other int literal meeting
            // its context. A literal outside the adopted width traps here,
            // before the write lands (X3).
            let elem = *elem;
            for arg in args {
                let arg = match arg {
                    Value::Int(v, ty) if ty.literal => {
                        let target = elem.unwrap_or(IntTy::INT);
                        if !target.holds(v) {
                            return machine.fault(
                                TrapKind::Overflow,
                                Rule::ArithChecked,
                                span,
                                format!(
                                    "`push` stored {v}, outside `{}` — checked arithmetic \
                                     traps in every profile (X3)",
                                    target.name()
                                ),
                            );
                        }
                        Value::Int(v, target)
                    }
                    arg => arg,
                };
                items.push(Slot::live(arg));
            }
            machine.note(Rule::Alloc, span, "List.push");
            Ok(Value::Unit)
        }
        (Value::List(items, _, home), "pop") => {
            // The other CoW divergence point — the same #25 consult as
            // `push`, before `Arc::make_mut` diverges anything.
            machine.check_home_write(*home, "this `pop`", span)?;
            match std::sync::Arc::make_mut(items).pop() {
                Some(slot) => Ok(slot.value),
                None => machine.fault(
                    TrapKind::Bounds,
                    Rule::Bounds,
                    span,
                    "`pop` on an empty List".to_owned(),
                ),
            }
        }
        (Value::List(items, _, _), "len" | "count") => {
            Ok(Value::Int(items.len() as i128, IntTy::INT))
        }
        (Value::List(items, _, _), "is_empty") => Ok(Value::Bool(items.is_empty())),
        (Value::List(items, _, _), "get") => {
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
        (Value::Map(pairs), "pairs") => Ok(Value::list(
            pairs
                .iter()
                .map(|(key, slot)| {
                    Slot::live(Value::Tuple(vec![
                        Slot::live(key.clone()),
                        Slot::live(slot.value.clone()),
                    ]))
                })
                .collect(),
            None,
            // A fresh container minted at the call lands in the ambient
            // region (D12), which is its home (#25).
            Some(machine.current_region()),
        )),

        // -- the s37 builtin `str` surface (D24/D25) -----------------------
        //
        // `corpus/strings/builtin_methods.lu` is the witness: `len` is bytes
        // and says so, probes take `str` needles, offsets out are BYTE
        // offsets, absence is a row (`none`), never a sentinel, and views
        // materialize `List`s at v0.
        (Value::Str(s), "len") => Ok(Value::Int(s.len() as i128, IntTy::INT)),
        (Value::Str(s), "is_empty") => Ok(Value::Bool(s.is_empty())),
        (Value::Str(s), "upper") => Ok(Value::Str(s.to_uppercase())),
        (Value::Str(s), "lower") => Ok(Value::Str(s.to_lowercase())),
        (Value::Str(s), "trim") => Ok(Value::Str(match args.first() {
            Some(Value::Str(cut)) => s.trim_matches(|c| cut.contains(c)).to_owned(),
            _ => s.trim().to_owned(),
        })),
        (Value::Str(s), "trim_start") => Ok(Value::Str(s.trim_start().to_owned())),
        (Value::Str(s), "trim_end") => Ok(Value::Str(s.trim_end().to_owned())),
        (Value::Str(s), "get") => {
            // `[mem.str.get]` — the boundary primitive, when the range
            // arrived as an evaluated value (`s.get(a..b)`, `s.get(r)`).
            // Open-ended and `^n` endpoints have no value shape; those
            // spellings are read off the syntax in `eval_method` and land
            // in `str_get` directly.
            let Some(Value::Range {
                start,
                end,
                inclusive,
                ..
            }) = args.first()
            else {
                return unsupported("`str.get` takes a byte range, like `s.get(4..8)`".to_owned());
            };
            str_get(machine, s, Some(*start), Some(*end), *inclusive, span)
        }
        (Value::Str(s), "bytes") => {
            // The byte view, materialized at v0 (D25 licenses byte indexing
            // on `bytes`; `b[i]` rides List indexing).
            Ok(Value::list(
                s.bytes()
                    .map(|b| Slot::live(Value::Int(i128::from(b), IntTy::INT)))
                    .collect(),
                Some(IntTy::INT),
                Some(machine.current_region()),
            ))
        }
        (Value::Str(s), "repeat") => {
            let Some(Value::Int(n, _)) = args.first() else {
                return unsupported("`repeat` takes a count".to_owned());
            };
            if *n < 0 {
                // `[mem.str.repeat]` (s71, #57): a negative count is a caller
                // contract violation — the deterministic `assert` trap on
                // every lane. Not `bounds` (no access is out of range), and
                // not the empty string (the sc03-era answer, retired by the
                // clause); `[conf.trap.map]`'s `assert` row names ruled
                // caller-contract violations of builtin surfaces now.
                return machine.fault(
                    TrapKind::Assert,
                    Rule::Assert,
                    span,
                    format!(
                        "`repeat({n})`: a negative repeat count is a caller contract violation \
                         (`[mem.str.repeat]`)"
                    ),
                );
            }
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
        (Value::Str(s), "ends_with") => {
            let Some(Value::Str(suffix)) = args.first() else {
                return unsupported("`ends_with` takes a `str` needle".to_owned());
            };
            Ok(Value::Bool(s.ends_with(suffix.as_str())))
        }
        (Value::Str(s), "find" | "rfind") => {
            // Byte offsets out; absence is a row, not a sentinel
            // (`s.find("wolf") else 0 - 1` is the caller's choice, not ours).
            let Some(Value::Str(needle)) = args.first() else {
                return unsupported(format!("`{name}` takes a `str` needle"));
            };
            let hit = if name == "find" {
                s.find(needle.as_str())
            } else {
                s.rfind(needle.as_str())
            };
            match hit {
                Some(offset) => Ok(Value::Int(offset as i128, IntTy::INT)),
                None => {
                    machine.note(Rule::ErrUnion, span, "`find` yields the `none` row");
                    Ok(error("none"))
                }
            }
        }
        (Value::Str(s), "count") => {
            let Some(Value::Str(needle)) = args.first() else {
                return unsupported("`count` takes a `str` needle".to_owned());
            };
            if needle.is_empty() {
                // `[mem.str.empty]` (s71, #56): the searching family is
                // DEFINED on an empty needle — it matches nothing, so the
                // count is 0 on every lane.
                return Ok(Value::Int(0, IntTy::INT));
            }
            Ok(Value::Int(
                s.matches(needle.as_str()).count() as i128,
                IntTy::INT,
            ))
        }
        (Value::Str(s), "split") => {
            let Some(Value::Str(sep)) = args.first() else {
                return unsupported("`split` takes a `str` separator".to_owned());
            };
            if sep.is_empty() {
                // `[mem.str.empty]`: an empty separator matches nothing, so
                // the split yields the whole string as its one piece.
                return Ok(Value::list(
                    vec![Slot::live(Value::Str(s.clone()))],
                    None,
                    Some(machine.current_region()),
                ));
            }
            Ok(Value::list(
                s.split(sep.as_str())
                    .map(|part| Slot::live(Value::Str(part.to_owned())))
                    .collect(),
                None,
                Some(machine.current_region()),
            ))
        }
        (Value::Str(s), "strip_prefix" | "strip_suffix") => {
            let Some(Value::Str(needle)) = args.first() else {
                return unsupported(format!("`{name}` takes a `str` needle"));
            };
            let stripped = if name == "strip_prefix" {
                s.strip_prefix(needle.as_str())
            } else {
                s.strip_suffix(needle.as_str())
            };
            match stripped {
                Some(rest) => Ok(Value::Str(rest.to_owned())),
                None => {
                    machine.note(Rule::ErrUnion, span, "`strip` yields the `none` row");
                    Ok(error("none"))
                }
            }
        }
        (Value::Str(s), "replace") => {
            let (Some(Value::Str(from)), Some(Value::Str(to))) = (args.first(), args.get(1)) else {
                return unsupported("`replace` takes two `str` arguments".to_owned());
            };
            if from.is_empty() {
                // `[mem.str.empty]`: an empty needle matches nothing, so the
                // replacement is the identity.
                return Ok(Value::Str(s.clone()));
            }
            Ok(Value::Str(s.replace(from.as_str(), to.as_str())))
        }
        (Value::Str(s), "words") => Ok(Value::list(
            s.split_whitespace()
                .map(|word| Slot::live(Value::Str(word.to_owned())))
                .collect(),
            None,
            Some(machine.current_region()),
        )),
        (Value::Str(s), "lines") => Ok(Value::list(
            s.lines()
                .map(|line| Slot::live(Value::Str(line.to_owned())))
                .collect(),
            None,
            Some(machine.current_region()),
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
        (Value::List(..) | Value::Tuple(_), Value::Int(i, _)) => {
            let items = target.seq_slots().expect("sequence arm");
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
        Value::List(items, elem, _) => {
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
            // The slice is a NEW list minted at this expression, so its home
            // is the ambient region here — not the receiver's (#25).
            Ok(Value::list(
                items[from as usize..to as usize].to_vec(),
                *elem,
                Some(machine.current_region()),
            ))
        }
        other => unsupported(format!("{} cannot be sliced", other.kind())),
    }
}

/// `s.get(a..b) -> str ! {none}` — `[mem.str.get]`, the boundary
/// primitive (s37, the core of wolf-lang#17).
///
/// Answers the same question as the checked slice `s[a..b]` with the same
/// domain: **exactly** the inputs on which `s[a..b]` faults `bounds` — an
/// offset outside `0..=s.len`, `b < a`, or an offset that splits a UTF-8
/// code point — answer the tag `none`, and every other input answers the
/// slice value `s[a..b]` would produce, bit-identical. No third outcome
/// exists: `get` never faults on any input. End-relative endpoints (`^n`)
/// and open ends were resolved by the caller before the domain question
/// is asked, exactly as in `s[a..b]`.
///
/// # Errors
///
/// None from the range itself — that is the whole point. `Err` here is
/// only the machine's own signal plumbing.
pub fn str_get(
    machine: &mut Machine,
    s: &str,
    start: Option<i128>,
    end: Option<i128>,
    inclusive: bool,
    span: Span,
) -> BResult {
    let len = s.len() as i128;
    let from = start.unwrap_or(0);
    let to = end.map_or(len, |e| if inclusive { e + 1 } else { e });
    let miss = |machine: &mut Machine| {
        machine.note(
            Rule::ErrUnion,
            span,
            "`str.get` answers the `none` row — oob and split-code-point are the same miss",
        );
        Ok(error("none"))
    };
    if from < 0 || to > len || from > to {
        return miss(machine);
    }
    let (from, to) = (from as usize, to as usize);
    if !s.is_char_boundary(from) || !s.is_char_boundary(to) {
        return miss(machine);
    }
    Ok(Value::Str(s[from..to].to_owned()))
}

/// Exact arity for a modelled C intrinsic (wolf-interp#18 item 4): C
/// would coerce or ignore extras silently; the model refuses with the
/// shape named instead.
fn c_arity(name: &str, args: &[Value], want: usize) -> Result<(), Signal> {
    if args.len() == want {
        return Ok(());
    }
    Err(Signal::Unsupported(format!(
        "`{name}` takes exactly {want} argument(s), got {} — the modelled signature is the \
         real one (approximation-contract §8)",
        args.len()
    )))
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
        enum_variant: false,
        row: Vec::new(),
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

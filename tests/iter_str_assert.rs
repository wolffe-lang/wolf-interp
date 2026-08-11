//! The other three s27 spec realignments, executing.
//!
//! - `[mem.iter.*]` — the iterator protocol: `for` over an `Iter[T]`
//!   implementor desugars to the drive loop
//!   (`var it = e; loop { let pat = (mut it).next() else { break }; body }`),
//!   conformance is **by name** (`impl Iter for …`), ranges stay the closed
//!   builtin family, and `next(mut self) -> T ! {done}` raises the
//!   payload-free lowercase `done` at exhaustion.
//! - `[mem.str.order]` — `< <= > >= <=>` on `str` × `str` is
//!   byte-lexicographic over the UTF-8 bytes, unsigned byte compare,
//!   shorter string first on a shared prefix.
//! - `[conf.trap.assert]` — `assert` is an intrinsic with its own two-arg
//!   arity: silent and effect-free when the condition holds (the message is
//!   **not** a second condition and stays unevaluated — wolfc's #19 shape),
//!   rendered as one stdout line before the trap when it fails, and never
//!   shadowed by a library function (wolf-std F-0009).

use wolf_interp::frontend;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn observe(source: &str) -> frontend::Observation {
    frontend::observe(source.as_bytes(), None)
}

fn exits_zero(source: &str) {
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
}

// -- [mem.iter.for]: the drive loop over a user implementor -------------

#[test]
fn for_drives_a_user_iter_implementor() {
    exits_zero(
        "\
struct Counter {
    at: int,
    stop: int,
}

impl Iter for Counter {
    fn next(mut self) -> int ! {done} {
        if self.at >= self.stop { return done }
        let v = self.at
        self.at += 1
        v
    }
}

fn main() -> !int {
    var sum = 0
    let c = Counter { at: 0, stop: 5 }
    for v in c { sum += v }
    if sum == 10 { 0 } else { 1 }
}
",
    );
}

/// `break`/`continue` in the body target the desugared loop
/// (`[mem.iter.for]`), and a `break` with a value is the `for`'s value.
#[test]
fn break_and_continue_target_the_drive_loop() {
    exits_zero(
        "\
struct Counter {
    at: int,
    stop: int,
}

impl Iter for Counter {
    fn next(mut self) -> int ! {done} {
        if self.at >= self.stop { return done }
        let v = self.at
        self.at += 1
        v
    }
}

fn main() -> !int {
    var sum = 0
    let c = Counter { at: 0, stop: 10 }
    for v in c {
        if v == 2 { continue }
        if v == 5 { break }
        sum += v
    }
    if sum == 0 + 1 + 3 + 4 { 0 } else { 1 }
}
",
    );
}

/// Conformance is by name (`[mem.iter.impl]`): an *inherent* `next` does
/// not make a type iterable, and the refusal says why.
#[test]
fn an_inherent_next_is_not_iter() {
    let source = "\
struct Fake {
    at: int,
}

impl Fake {
    fn next(mut self) -> int ! {done} {
        return done
    }
}

fn main() -> !int {
    let f = Fake { at: 0 }
    for v in f { return 1 }
    0
}
";
    let observation = observe(source);
    assert_eq!(observation.verdict, Verdict::Unsupported);
    let reason = observation.reason.unwrap_or_default();
    assert!(reason.contains("Iter"), "reason: {reason}");
}

/// The iterator value is copied into the loop (the desugar's
/// `var it = e`): the binding the program iterates is untouched after.
#[test]
fn the_loop_drives_a_copy_of_the_iterator() {
    exits_zero(
        "\
struct Counter {
    at: int,
    stop: int,
}

impl Iter for Counter {
    fn next(mut self) -> int ! {done} {
        if self.at >= self.stop { return done }
        let v = self.at
        self.at += 1
        v
    }
}

fn main() -> !int {
    let c = Counter { at: 0, stop: 3 }
    var n = 0
    for v in c { n += 1 }
    for v in c { n += 1 }
    if n == 6 && c.at == 0 { 0 } else { 1 }
}
",
    );
}

/// Explicit `next` calls see the same protocol the loop does, exhaustion
/// included: the mut receiver advances in place and `done` raises after
/// the last element — stable, because the guard keeps answering `done`.
#[test]
fn explicit_next_calls_advance_and_exhaust() {
    exits_zero(
        "\
struct Counter {
    at: int,
    stop: int,
}

impl Iter for Counter {
    fn next(mut self) -> int ! {done} {
        if self.at >= self.stop { return done }
        let v = self.at
        self.at += 1
        v
    }
}

fn main() -> !int {
    var c = Counter { at: 0, stop: 2 }
    if ((mut c).next() else 99) != 0 { return 1 }
    if ((mut c).next() else 99) != 1 { return 2 }
    if ((mut c).next() else 99) != 99 { return 3 }
    if ((mut c).next() else 99) != 99 { return 4 }
    0
}
",
    );
}

// -- [mem.str.order]: byte-lexicographic relational family --------------

#[test]
fn str_ordering_is_byte_lexicographic() {
    exits_zero(
        "\
fn main() -> !int {
    if !(\"a\" < \"b\") { return 1 }
    if !(\"a\" < \"ab\") { return 2 }
    if !(\"Z\" < \"a\") { return 3 }
    if !(\"abc\" <= \"abc\") { return 4 }
    if !(\"b\" > \"ab\") { return 5 }
    if !(\"wolf\" >= \"wolf\") { return 6 }
    0
}
",
    );
}

/// Shorter string first on a shared prefix; `<=>` yields the integer
/// ordering value (the v0 `int` read); UTF-8 *bytes*, not code points —
/// `é`'s lead byte 0xC3 sorts after every ASCII byte.
#[test]
fn str_spaceship_and_utf8_bytes() {
    exits_zero(
        "\
fn main() -> !int {
    if (\"a\" <=> \"b\") != 0 - 1 { return 1 }
    if (\"b\" <=> \"a\") != 1 { return 2 }
    if (\"ab\" <=> \"ab\") != 0 { return 3 }
    if !(\"é\" > \"z\") { return 4 }
    0
}
",
    );
}

// -- [conf.trap.assert]: the two-argument intrinsic ---------------------

/// The counterparty's #19 shape, from this side: a message on a HOLDING
/// assert is never evaluated — no side effect, no output, no trap.
#[test]
fn a_holding_asserts_message_stays_cold() {
    let source = "\
fn loud() -> str {
    print(\"evaluated!\")
    \"boom\"
}

fn main() -> !int {
    assert(1 + 1 == 2, loud())
    0
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
    assert!(
        observation.stdout.is_empty(),
        "stdout: {}",
        String::from_utf8_lossy(&observation.stdout)
    );
}

/// The failing path evaluates the message, renders it as one line to
/// stdout, and traps at the assert's own span.
#[test]
fn a_failing_assert_renders_its_message_then_traps() {
    let source = "\
fn main() -> !int {
    let k = 3
    assert(k == 2, \"k must be 2, got {k}\")
    0
}
";
    let observation = observe(source);
    assert_eq!(observation.verdict, Verdict::Trap(TrapKind::Assert));
    assert_eq!(
        String::from_utf8_lossy(&observation.stdout).trim(),
        "k must be 2, got 3"
    );
}

/// One-argument `assert` keeps its shape: no message, same trap.
#[test]
fn one_argument_assert_still_traps_bare() {
    let source = "\
fn main() -> !int {
    assert(false)
    0
}
";
    let observation = observe(source);
    assert_eq!(observation.verdict, Verdict::Trap(TrapKind::Assert));
    assert!(observation.stdout.is_empty());
}

/// `assert` is never shadowed by a library function (wolf-std F-0009: a
/// module-level `assert` severed callers from the trap). The intrinsic
/// wins; the module function is unreachable under that name.
#[test]
fn a_module_level_assert_does_not_shadow_the_intrinsic() {
    let source = "\
fn assert(cond: bool) -> int {
    42
}

fn main() -> !int {
    assert(false)
    0
}
";
    let observation = observe(source);
    assert_eq!(observation.verdict, Verdict::Trap(TrapKind::Assert));
}

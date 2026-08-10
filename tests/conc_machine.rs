//! The is06 acceptance suite: spec/03's dynamic semantics under the sim
//! scheduler.
//!
//! Five claims from the sprint contract, each a section here:
//!
//! 1. **Determinism** — the same program with the same seed produces a
//!    byte-identical trace, twice; two different seeds produce different
//!    (both legal) traces (`[conc.det.seed]`).
//! 2. **The supervision matrix** — a failing child cancels siblings and
//!    re-raises; a linked proc dies with its partner; a monitor receives each
//!    enumerated exit reason; a proc crash bulk-frees its regions with the
//!    leak assertion on (`[conc.task.fail]`, `[conc.proc.2]`,
//!    `[conc.proc.exit]`, `[conc.proc.kill]`).
//! 3. **The D14 litmus** — a KILLED proc's defers do NOT run; a CANCELLED
//!    task's defers DO (`[conc.proc.kill]` vs `[conc.cancel.defer]`).
//! 4. **Region transfer** — use-after-move-across-channel faults with the
//!    clause cited; a live-external-edge send faults; `when` nested
//!    acquisition faults.
//! 5. **Virtual time** — a timeout-heavy program completes in wall-clock
//!    milliseconds (`[conc.select.timeout]`).
//!
//! Plus the race detector (`[conc.mm.race.3]`) over the only two memories two
//! tasks can share mutably here, and the deadlock report's honesty.

use std::time::Instant;

use wolf_interp::eval::{self, Outcome, Trace};
use wolf_interp::trap::TrapKind;

fn run(source: &str) -> eval::Run {
    eval::run_source("t.lu", source).expect("parses")
}

fn run_seeded(source: &str, seed: u64) -> eval::Run {
    eval::run_source_seeded("t.lu", source, Some(seed), Trace::All).expect("parses")
}

fn exit_code(run: &eval::Run) -> u8 {
    match &run.outcome {
        Outcome::Exit(code) => *code,
        other => panic!("expected an exit, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 1. determinism: seed → schedule → byte-identical trace
// ---------------------------------------------------------------------------

/// A program with real scheduling freedom: two producers, one consumer.
const CONTENDED: &str = r#"fn main() -> !int {
    var sum = 0
    scope s {
        let ch = channel[int](1)
        s.spawn(fn() {
            for i in 1..=5 { ch.send(i) }
        })
        s.spawn(fn() {
            for i in 1..=5 { ch.send(i * 10) }
        })
        for _ in 1..=10 {
            let v = ch.recv()?
            sum += v
        }
    }
    if sum == 165 { 0 } else { 1 }
}
"#;

#[test]
fn the_same_seed_produces_a_byte_identical_trace_twice() {
    for seed in [0, 1, 42] {
        let first = run_seeded(CONTENDED, seed);
        let second = run_seeded(CONTENDED, seed);
        assert_eq!(first.stdout, second.stdout, "seed {seed}");
        assert_eq!(first.trace, second.trace, "seed {seed}");
        assert_eq!(exit_code(&first), 0, "seed {seed}");
        assert_eq!(exit_code(&second), 0, "seed {seed}");
    }
}

#[test]
fn two_different_seeds_produce_different_and_both_legal_traces() {
    // Seed 0 is strict FIFO; seed 1 draws from the generator. Both runs are
    // legal executions (exit 0); the *decision streams* differ.
    let fifo = run_seeded(CONTENDED, 0);
    let seeded = run_seeded(CONTENDED, 1);
    assert_eq!(exit_code(&fifo), 0);
    assert_eq!(exit_code(&seeded), 0);
    assert_ne!(
        fifo.trace, seeded.trace,
        "two seeds picked identical schedules on a contended program"
    );
}

#[test]
fn the_seeded_select_choice_is_replayed_from_the_seed() {
    // `[conc.select.fair]`: among simultaneously-ready arms the choice is
    // pseudo-random **from the scheduler seed** — never wall-clock incidental.
    let source = r#"fn main() -> !int {
    let a = channel[int](1)
    let b = channel[int](1)
    a.send(1)
    b.send(2)
    var got = 0
    select {
        v from a => { got = v },
        v from b => { got = v },
    }
    got
}
"#;
    let mut outcomes = std::collections::BTreeSet::new();
    for seed in 0..8 {
        let first = run_seeded(source, seed);
        let again = run_seeded(source, seed);
        assert_eq!(first.trace, again.trace, "seed {seed} must replay exactly");
        outcomes.insert(exit_code(&first));
    }
    // Both arms are conforming outcomes, and the seed space realizes both.
    assert_eq!(
        outcomes,
        std::collections::BTreeSet::from([1, 2]),
        "eight seeds never exercised both ready arms"
    );
}

// ---------------------------------------------------------------------------
// 2. the supervision matrix
// ---------------------------------------------------------------------------

#[test]
fn a_failing_child_cancels_its_siblings_and_reraises_at_the_scope_exit() {
    // [conc.task.fail]: the sibling is blocked at a recv (a runtime-owned
    // blocking point); the failure cancels it there, its defers run
    // ([conc.cancel.defer]), and the error re-raises into the caller.
    let source = r#"fn boom() -> !int { Boom }

fn race_them() -> !int {
    let ch = channel[int](0)
    scope s {
        s.spawn(fn() {
            defer print_raw("sibling-defer-ran ")
            let v = ch.recv()?
            v
        })
        s.spawn(fn() { boom() })
    }
    0
}

fn main() -> !int {
    let r = race_them() else |_| { 42 }
    if r == 42 { 0 } else { 1 }
}
"#;
    let run = run(source);
    assert_eq!(exit_code(&run), 0);
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "sibling-defer-ran ",
        "the cancelled sibling's defer must run — cancellation is polite"
    );
}

#[test]
fn a_monitor_receives_the_normal_exit_reason_with_its_value_shape() {
    let source = r#"fn worker_body() -> int { 7 }

fn main() -> !int {
    let w = spawn proc worker_body()
    let m = w.monitor()
    select {
        exit(reason) from m => { if reason.is_normal() { 0 } else { 1 } },
        timeout(1.s) => { 2 },
    }
}
"#;
    assert_eq!(exit_code(&run(source)), 0);
}

#[test]
fn a_monitor_receives_the_error_reason_when_the_proc_body_errors() {
    let source = r#"fn failing() -> !int { Kaboom }

fn main() -> !int {
    let w = spawn proc failing()
    let m = w.monitor()
    select {
        exit(reason) from m => { if reason.is_error() { 0 } else { 1 } },
        timeout(1.s) => { 2 },
    }
}
"#;
    assert_eq!(exit_code(&run(source)), 0);
}

#[test]
fn a_monitor_receives_the_killed_reason_and_kill_is_idempotent() {
    let source = r#"fn sleeper_body() -> !int {
    let never = channel[int](0)
    let v = never.recv()?
    v
}

fn main() -> !int {
    let w = spawn proc sleeper_body()
    let m = w.monitor()
    w.kill()
    w.kill()
    select {
        exit(reason) from m => { if reason.is_killed() { 0 } else { 1 } },
        timeout(1.s) => { 2 },
    }
}
"#;
    let run = run(source);
    assert_eq!(exit_code(&run), 0);
    assert!(run.leaks.is_empty(), "killed proc leaked: {:?}", run.leaks);
}

#[test]
fn monitoring_an_already_exited_proc_delivers_immediately() {
    let source = r#"fn quick() -> int { 1 }

fn main() -> !int {
    let w = spawn proc quick()
    let sync = channel[int](0)
    scope s {
        s.spawn(fn() { sync.send(0) })
        let _ = sync.recv()?
    }
    let m = w.monitor()
    select {
        exit(reason) from m => { 0 },
        timeout(1.s) => { 1 },
    }
}
"#;
    // The proc may or may not have run yet when the monitor attaches; either
    // way the reason arrives (immediately if already exited).
    assert_eq!(exit_code(&run(source)), 0);
}

#[test]
fn a_linked_proc_takes_its_partner_with_it() {
    // [conc.proc.2]: symmetric fate. `w.link()` couples the *caller's* proc
    // with `w` — here that is the root domain, and the failing partner's
    // abnormal exit kills it. spec/03 gives the root domain no exit
    // semantics, so the machine reports the kill honestly as `unsupported`
    // (a filed spec gap: `link` has no in-language spelling for coupling two
    // child procs, and the root's death is unspecified).
    let source = r#"fn failing() -> !int { Kaboom }

fn main() -> !int {
    let b = spawn proc failing()
    b.link()
    let m = b.monitor()
    select {
        exit(reason) from m => { 0 },
        timeout(1.s) => { 1 },
    }
}
"#;
    let run = run(source);
    match &run.outcome {
        Outcome::Unsupported(reason) => {
            assert!(reason.contains("killed"), "{reason}");
        }
        other => panic!("expected the root-kill report, got {other:?}"),
    }
}

#[test]
fn a_proc_crash_is_contained_bulk_frees_and_reports_error() {
    // [conc.proc.kill]/[conc.proc.1]: the Armstrong claim, executable — a
    // trap inside a proc crashes the proc, not the program; its regions
    // bulk-free (the leak assertion is the proof) and the reason is an error.
    let source = r#"fn crasher() -> int {
    let xs = List[int]()
    xs.push(1)
    let d = 0
    10 / d
}

fn main() -> !int {
    let w = spawn proc crasher()
    let m = w.monitor()
    select {
        exit(reason) from m => { if reason.is_error() { 0 } else { 1 } },
        timeout(1.s) => { 2 },
    }
}
"#;
    let run = run(source);
    assert_eq!(exit_code(&run), 0, "the crash escaped its failure domain");
    assert!(run.leaks.is_empty(), "crash leaked: {:?}", run.leaks);
    assert_eq!(
        run.forest,
        Ok(()),
        "the region forest broke at proc teardown"
    );
}

// ---------------------------------------------------------------------------
// 3. the D14 litmus: kill is structural, cancellation is polite
// ---------------------------------------------------------------------------

#[test]
fn killed_proc_defers_do_not_run_and_cancelled_task_defers_do() {
    // The decided rule, side by side in one program. The killed proc's defer
    // print must NOT appear; the cancelled task's defer print MUST.
    let source = r#"fn sleeper_body() -> !int {
    defer print_raw("KILLED-DEFER ")
    let never = channel[int](0)
    let v = never.recv()?
    v
}

fn boom() -> !int { Boom }

fn cancelled_half() -> !int {
    let ch = channel[int](0)
    scope s {
        s.spawn(fn() {
            defer print_raw("cancel-defer ")
            let v = ch.recv()?
            v
        })
        s.spawn(fn() { boom() })
    }
    0
}

fn main() -> !int {
    let w = spawn proc sleeper_body()
    let m = w.monitor()
    w.kill()
    select {
        exit(reason) from m => { if reason.is_killed() { print_raw("released ") } },
        timeout(1.s) => { print_raw("timeout ") },
    }
    let r = cancelled_half() else |_| { 0 }
    0
}
"#;
    let run = run(source);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        !stdout.contains("KILLED-DEFER"),
        "a KILLED proc ran its defers: {stdout}"
    );
    assert!(
        stdout.contains("released"),
        "the kill reason never delivered: {stdout}"
    );
    assert!(
        stdout.contains("cancel-defer"),
        "a CANCELLED task's defers did not run: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// 4. region transfer across channels
// ---------------------------------------------------------------------------

#[test]
fn touching_a_sent_region_from_the_sender_faults_with_the_clause() {
    // The sender keeps a pool handle into the region it moves; touching it
    // after the send is the cross-task stale use the sprint names
    // [conc.chan.staleuse] — a clause spec/03 does not define yet (filed), so
    // the fault cites the `conc.chan` family and says so.
    let source = r#"fn main() -> !int {
    let ch = channel[region](1)
    var stolen = 0
    let p = region(pool(int))
    let pl = in p { Pool[int]() }
    let h = in p {
        let slot = pl.reserve()
        pl.init(slot, 41)
        slot
    }
    scope s {
        s.spawn(fn() {
            let r = ch.recv() else |_| { return 1 }
            0
        })
        ch.send(move p)
        stolen = pl[h]
    }
    stolen
}
"#;
    let run = run(source);
    match &run.outcome {
        Outcome::Trap(trap) => {
            assert_eq!(trap.kind, TrapKind::RegionFault);
            assert_eq!(trap.rule.anchor(), "conc.chan");
            assert!(
                trap.message.contains("stale") || trap.message.contains("in flight"),
                "{}",
                trap.message
            );
        }
        other => panic!("expected the stale-use region-fault, got {other:?}"),
    }
}

#[test]
fn sending_a_region_with_a_live_external_edge_faults() {
    // The dynamic disconnectedness check at the moment of send: the region is
    // owned by another region (an `iso` edge), so it is not disconnected.
    let source = r#"type Holder = struct {
    inner: region
}

fn main() -> !int {
    let ch = channel[region](1)
    region outer {
        let child = region()
        let hold = Holder { inner: child }
        ch.send(move hold.inner)
    }
    0
}
"#;
    let run = run(source);
    match &run.outcome {
        Outcome::Trap(trap) => {
            assert_eq!(trap.kind, TrapKind::RegionFault);
            assert!(
                trap.message.contains("live external edge")
                    || trap.message.contains("disconnected"),
                "{}",
                trap.message
            );
        }
        other => panic!("expected the disconnectedness fault, got {other:?}"),
    }
}

#[test]
fn freeze_then_share_reads_from_any_task() {
    // [conc.mm.hb.freeze]: imm data is readable from every task, by
    // reference, no transfer. The tasks *report* through a channel, so the
    // reads are observable without the E1101 capture shape.
    let source = r#"fn main() -> !int {
    let table = freeze region {
        var xs = List[int]()
        for i in 0..10 { xs.push(i * i) }
        xs
    }
    let out = channel[int](2)
    scope s {
        s.spawn(fn() { out.send(table[3]) })
        s.spawn(fn() { out.send(table[4]) })
        let a = out.recv()?
        let b = out.recv()?
        if a + b == 25 { return 0 }
        return 1
    }
    1
}
"#;
    assert_eq!(exit_code(&run(source)), 0);
}

#[test]
fn nested_when_acquisition_faults() {
    let source = r#"fn main() -> !int {
    let a = Mutex(1)
    let b = Mutex(2)
    when (a, b) {
        when (a, b) {
            a += 1
        }
    }
    0
}
"#;
    let run = run(source);
    match &run.outcome {
        Outcome::Trap(trap) => {
            assert_eq!(trap.kind, TrapKind::Assert);
            assert_eq!(trap.rule.anchor(), "sync.when.nonest");
        }
        other => panic!("expected the nested-when fault, got {other:?}"),
    }
}

#[test]
fn when_acquires_in_canonical_order_from_either_spelling() {
    // Two tasks acquire {a, b} in opposite written orders; ordered set
    // acquisition means no deadlock exists to find, under any seed.
    let source = r#"fn main() -> !int {
    let a = Mutex(1)
    let b = Mutex(2)
    scope s {
        s.spawn(fn() {
            when (a, b) { a += 10; b += 10 }
        })
        s.spawn(fn() {
            when (b, a) { b += 100; a += 100 }
        })
    }
    var total = 0
    when (a, b) { total = a + b }
    total
}
"#;
    for seed in 0..6 {
        let run = run_seeded(source, seed);
        // 1+10+100 + 2+10+100 = 223 under every schedule.
        assert_eq!(exit_code(&run), 223, "seed {seed}");
    }
}

// ---------------------------------------------------------------------------
// 5. virtual time
// ---------------------------------------------------------------------------

#[test]
fn an_hour_of_timeouts_runs_in_wall_clock_milliseconds() {
    // [conc.select.timeout] + the madsim model: the clock is virtual; time
    // advances when the ready queue is empty. 3600 seconds of waiting, twice,
    // in a test that must come back immediately.
    let source = r#"fn main() -> !int {
    let never = channel[int](1)
    var fired = 0
    select {
        v from never => { fired = v },
        timeout(3600.s) => { fired = 1 },
    }
    select {
        v from never => { fired = v },
        timeout(3600.s) => { fired += 1 },
    }
    if fired == 2 { 0 } else { 1 }
}
"#;
    let started = Instant::now();
    let run = run(source);
    assert_eq!(exit_code(&run), 0);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "virtual time leaked into wall time: {:?}",
        started.elapsed()
    );
}

// ---------------------------------------------------------------------------
// the race detector, and its negative control
// ---------------------------------------------------------------------------

#[test]
fn two_unordered_writes_to_one_allocation_trap_race() {
    // [conc.mm.race.1] names the reachability (Tier 3/FFI); [conc.mm.race.3]
    // licenses detection. Copies of a raw pointer share the allocation, the
    // two tasks have no happens-before edge, and the schedule realizes the
    // conflict: exact detection, at that interleaving.
    let source = r#"import c "stdlib.h"

fn main() -> !int {
    unsafe {
        let p = c.malloc(8)
        scope s {
            s.spawn(fn() { unsafe { p[0] = 1 } })
            s.spawn(fn() { unsafe { p[0] = 2 } })
        }
        c.free(p)
    }
    0
}
"#;
    let run = run(source);
    match &run.outcome {
        Outcome::Trap(trap) => {
            assert_eq!(trap.kind, TrapKind::Race);
            assert_eq!(trap.rule.anchor(), "conc.mm.race.3");
        }
        other => panic!("expected trap(race), got {other:?}"),
    }
}

#[test]
fn channel_ordered_writes_to_one_allocation_do_not_race() {
    // The negative control: the same two writes, ordered by a channel
    // rendezvous ([conc.mm.hb.chan]) — no race exists and none is reported.
    let source = r#"import c "stdlib.h"

fn main() -> !int {
    let order = channel[int](0)
    unsafe {
        let p = c.malloc(8)
        scope s {
            s.spawn(fn() {
                unsafe { p[0] = 1 }
                order.send(1)
            })
            s.spawn(fn() {
                let go = order.recv() else |_| { return 1 }
                unsafe { p[0] = 2 }
                0
            })
        }
        c.free(p)
    }
    0
}
"#;
    let run = run(source);
    assert_eq!(exit_code(&run), 0, "a false race: {:?}", run.outcome);
}

#[test]
fn unordered_pool_slot_writes_trap_race() {
    // The other shareable memory: pool slots in an unmoved region. Two tasks
    // write one slot with no ordering edge — the E1101/E1102 shape the
    // compiler rejects statically, detected exactly here.
    let source = r#"fn main() -> !int {
    region p: pool(int) {
        let pool = Pool[int]()
        let h = pool.reserve()
        pool.init(h, 0)
        scope s {
            s.spawn(fn() { pool.init(h, 1) })
            s.spawn(fn() { pool.init(h, 2) })
        }
    }
    0
}
"#;
    let run = run(source);
    match &run.outcome {
        Outcome::Trap(trap) => {
            assert_eq!(trap.kind, TrapKind::Race);
        }
        other => panic!("expected trap(race), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// channels: close semantics, deadlock honesty, scope capability
// ---------------------------------------------------------------------------

#[test]
fn send_after_close_returns_the_error_value_never_a_fault() {
    let source = r#"fn main() -> !int {
    let ch = channel[int](1)
    ch.close()
    let r = ch.send(1)
    0
}
"#;
    assert_eq!(exit_code(&run(source)), 0);
}

#[test]
fn a_channel_iteration_ends_at_drained_close() {
    let source = r#"fn main() -> !int {
    var sum = 0
    scope s {
        let ch = channel[int](2)
        s.spawn(fn() {
            ch.send(20)
            ch.send(22)
            ch.close()
        })
        for v in ch { sum += v }
    }
    if sum == 42 { 0 } else { 1 }
}
"#;
    assert_eq!(exit_code(&run(source)), 0);
}

#[test]
fn a_deadlocked_program_reports_honestly_instead_of_hanging() {
    // No verdict exists for nontermination and no trap kind for deadlock
    // (both filed as spec findings); the machine says so and stops.
    let source = r#"fn main() -> !int {
    let ch = channel[int](0)
    let v = ch.recv()?
    v
}
"#;
    let run = run(source);
    match &run.outcome {
        Outcome::Unsupported(reason) => {
            assert!(reason.contains("deadlock"), "{reason}");
        }
        other => panic!("expected the deadlock report, got {other:?}"),
    }
}

#[test]
fn a_scope_handle_is_a_passable_capability() {
    // D16 / [conc.task.scope]: a function that spawns into its caller's scope
    // takes the handle as a parameter — lifetime extension visible at the
    // call site (the njs nursery shape).
    let source = r#"fn fan_out(sc: int, ch: int) -> int {
    sc.spawn(fn() { ch.send(21) })
    sc.spawn(fn() { ch.send(21) })
    0
}

fn main() -> !int {
    var sum = 0
    scope s {
        let ch = channel[int](2)
        fan_out(s, ch)
        sum += ch.recv()?
        sum += ch.recv()?
    }
    if sum == 42 { 0 } else { 1 }
}
"#;
    assert_eq!(exit_code(&run(source)), 0);
}

// ---------------------------------------------------------------------------
// the schedule trace: snapshot + event-numbering shape
// ---------------------------------------------------------------------------

#[test]
fn the_schedule_trace_is_numbered_and_snapshot_stable() {
    // The stable textual trace the sprint asks for: every schedule decision
    // and sync event, one line each, numbered — is07's input format. The
    // snapshot pins the whole seed-0 decision stream of a supervision
    // program end to end.
    let source = r#"fn boom() -> !int { Boom }

fn race_them() -> !int {
    let ch = channel[int](0)
    scope s {
        s.spawn(fn() {
            let v = ch.recv()?
            v
        })
        s.spawn(fn() { boom() })
    }
    0
}

fn main() -> !int {
    let r = race_them() else |_| { 42 }
    if r == 42 { 0 } else { 1 }
}
"#;
    let run = run_seeded(source, 0);
    assert_eq!(exit_code(&run), 0);
    let sched: Vec<&str> = run
        .trace
        .iter()
        .filter(|line| line.contains(" ev#"))
        .map(String::as_str)
        .collect();
    // Event numbers are dense and ordered: the stream is the schedule.
    for (index, line) in sched.iter().enumerate() {
        assert!(
            line.contains(&format!("ev#{index} ")),
            "event {index} out of order: {line}"
        );
    }
    insta::assert_snapshot!("cancel_sibling_schedule_seed0", sched.join("\n"));
}

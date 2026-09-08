//! `[os.net.accept]` on this machine — is39, the clause that arrived with the
//! `v0.2.6` pin (wolf-lang#242).
//!
//! The clause names the checked machine directly: *"its budgeted accept polls
//! a non-blocking listener and retries `would_block` against the same budget,
//! its unbudgeted accept is the blocking syscall, and no other hand can share
//! its listener — it refuses an inherit set BY NAME."* Both halves are
//! asserted here, and the first one is asserted the hard way.
//!
//! **is38 deferred #242 because "the fair-accept re-wait … is an
//! implementation surface and not a mirror of anything in this release", and
//! because arranging a lost race wants two hands on one listener. That
//! reasoning does not survive contact: a lost race needs two HANDS, not two
//! PROCESSES, and this machine arranges two hands out of its own task tier.**
//! `corpus/net/accept_race.lu` reaches for `os_spawn_with` with a non-empty
//! inherit set — the one construct this machine refuses by name — but the
//! relation the clause is about needs no second process at all: two tasks in
//! one `scope`, one listener, one connection. One takes it; the other's
//! `net_accept` finds nothing there and, per the clause, **waits again
//! against the budget the call began with** rather than parking in the kernel
//! or re-arming a fresh one.
//!
//! Nothing in `eval::net` moved to meet the clause — `poll_accept` answers
//! `Poll::NotYet` on `WouldBlock` and `net_park` measures one `started`
//! against one `armed(fd)` — which is the finding: the posture was already
//! the clause's, and it was untested until the clause existed to name it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// No child in this file may outlive its budget.
///
/// The first cut of these tests used `Command::output()`, which waits
/// FOREVER. A child that parks — the exact failure `[os.net.accept]` is about
/// — then hangs the whole `cargo test` step rather than failing it, and it did:
/// wolf-interp CI, windows job, 1h46m in `cargo test` while the same suite went
/// green on linux and macOS in minutes. `corpus/net/accept_race.lu` states the
/// discipline in its own header — "a regression is a wrong line, never a hung
/// gauntlet" — and the test written to mirror that clause is the one place it
/// must not be broken.
const CHILD_BUDGET: Duration = Duration::from_secs(60);

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

/// Runs `lupin` and KILLS it if it outstays [`CHILD_BUDGET`], so a park is a
/// failing test with a message and never a hung runner.
///
/// The pipes are drained only after the child is gone, which is safe here and
/// only here: every program in this file prints one short line, far inside a
/// pipe buffer. A chattier child would need a reader thread per pipe.
fn run_bounded(mut command: Command, what: &str) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("lupin runs");
    let started = Instant::now();
    loop {
        match child.try_wait().expect("the child is waitable") {
            Some(_) => break,
            None => {
                assert!(
                    started.elapsed() < CHILD_BUDGET,
                    "{what}: `lupin` did not finish within {CHILD_BUDGET:?} — killed. \
                     A parked accept is the failure this file exists to catch, so read \
                     this as the finding, not as an infrastructure flake."
                );
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
    child.wait_with_output().expect("the child's output")
}

fn run_program(dir: &Path, source: &str) -> Output {
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("written");
    let mut command = Command::new(env!("CARGO_BIN_EXE_lupin"));
    command.arg("run").arg(&entry);
    run_bounded(command, &format!("run {}", entry.display()))
}

/// One hand of the race: accept under the listener's armed budget, then say
/// which side of it this hand came back on. `timeout` is the row the budget
/// owes a hand that lost — never a hang, never a forged handle.
const HAND: &str = "\
        s.spawn(fn() {\n\
        \x20           let c = net_accept(srv) else |e| match e {\n\
        \x20               timeout => -1,\n\
        \x20               _ => -2,\n\
        \x20           }\n\
        \x20           if c >= 0 {\n\
        \x20               net_close(c) else |_| print(\"close failed\")\n\
        \x20               ch.send(\"won\")\n\
        \x20           } else {\n\
        \x20               ch.send(\"lost\")\n\
        \x20           }\n\
        \x20           0\n\
        \x20       })\n";

#[test]
fn two_hands_on_one_listener_both_come_back_inside_one_budget() {
    // Two tasks accepting the SAME listener, one dial. Both are woken for
    // that connection — readiness is not exclusivity — and exactly one takes
    // it. The other is the thing #242 is about: before the fix (in the native
    // runtime) it parked in the kernel with its budget already spent, alive
    // and answering nothing; here it must come back as `timeout`.
    //
    // `one_budget` is the half a count alone would miss. The budget is 2000ms
    // and the assertion is that BOTH hands are home inside 3500ms: a runtime
    // that re-armed a fresh budget on the lost wake would answer at ~4000ms
    // and later, and the line would read false. 1500ms of slack sits between
    // the two, because the discriminator has to survive a cold, loaded CI
    // runner interpreting a debug build — the first cut allowed 800ms of slop
    // against a 1600ms floor, which is a flake waiting for a slow box.
    let dir = scratch("net-accept-race");
    let source = format!(
        "fn main() -> !int {{\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   let port = net_port(srv)?\n\
        \x20   net_deadline(srv, 2000)?\n\
        \x20   let ch = channel[str](4)\n\
        \x20   let started = time_now_ms()\n\
        \x20   scope s {{\n\
        \x20       {HAND}\
        \x20       {HAND}\
        \x20       s.spawn(fn() {{\n\
        \x20           let cli = net_connect(\"127.0.0.1:{{port}}\") else |_| {{ print(\"dial failed\"); -1 }}\n\
        \x20           net_close(cli) else |_| print(\"cli close failed\")\n\
        \x20           0\n\
        \x20       }})\n\
        \x20   }}\n\
        \x20   let a = ch.recv()?\n\
        \x20   let b = ch.recv()?\n\
        \x20   let elapsed = time_now_ms() - started\n\
        \x20   var won = 0\n\
        \x20   var lost = 0\n\
        \x20   if a == \"won\" {{ won = won + 1 }}\n\
        \x20   if b == \"won\" {{ won = won + 1 }}\n\
        \x20   if a == \"lost\" {{ lost = lost + 1 }}\n\
        \x20   if b == \"lost\" {{ lost = lost + 1 }}\n\
        \x20   net_close(srv)?\n\
        \x20   print(\"one_won {{won == 1}} loser_returned {{lost == 1}} one_budget {{elapsed < 3500}}\")\n\
        \x20   0\n\
        }}\n"
    );
    let out = run_program(&dir, &source);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        stdout.trim_end(),
        "one_won true loser_returned true one_budget true",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_budgeted_accept_with_no_connection_answers_its_own_row() {
    // The control for the test above: with nothing to take at all, the same
    // budget answers `timeout` and the program is home well inside twice it.
    // Without this, "the loser timed out" could just as well be "the accept
    // never sees anything and always times out" — the pair separates them.
    let dir = scratch("net-accept-budget");
    let source = "fn main() -> !int {\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   net_deadline(srv, 400)?\n\
        \x20   let started = time_now_ms()\n\
        \x20   let c = net_accept(srv) else |e| match e {\n\
        \x20       timeout => -1,\n\
        \x20       _ => -2,\n\
        \x20   }\n\
        \x20   let elapsed = time_now_ms() - started\n\
        \x20   net_close(srv)?\n\
        \x20   print(\"row {c == -1} inside {elapsed >= 400 && elapsed < 3000}\")\n\
        \x20   0\n\
        }\n";
    let out = run_program(&dir, source);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "stdout: {stdout}");
    assert_eq!(stdout.trim_end(), "row true inside true");
}

#[test]
fn the_corpus_witness_declines_by_name_and_not_by_absence() {
    // The clause's other half. `corpus/net/accept_race.lu` arranges its hands
    // with `os_spawn_with` and a NON-EMPTY inherit set, which this machine
    // refuses with s137's own construct string — the verdict `unsupported`,
    // never the `unsupported` ROW, because the row would be a claim about the
    // HOST and both hosts this machine runs on serve the handoff. A refusal
    // is a verdict, not an absence, and the ledger sorts it out-of-scope.
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus/net/accept_race.lu");
    let mut command = Command::new(env!("CARGO_BIN_EXE_lupin"));
    command.arg("conform-run").arg(&corpus).arg("--json");
    let out = run_bounded(command, "conform-run net/accept_race.lu");
    let record: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("the record is JSON");
    assert_eq!(record["verdict"], "unsupported");
    assert_eq!(
        record["x-unsupported"],
        "fd inheritance across os_spawn_with in checked execution"
    );
}

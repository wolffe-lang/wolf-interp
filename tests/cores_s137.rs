//! is38 — s137's five anchors in the mirror, at their edges.
//!
//! The four corpus witnesses (`net/reuse_port.lu`, `net/inherit_listener.lu`,
//! `net/wait_readiness.lu`, `os/cpus.lu`) carry the happy paths and the walk
//! checks them on every commit. This file carries what the corpus cannot:
//! the clause sentences a portable witness has no way to write down.
//!
//! - `[os.net.wait]`'s `io` row for an EMPTY set with an unbounded deadline
//!   — "nothing could ever end that wait", which a corpus file cannot assert
//!   without risking a hang on the machine that gets it wrong.
//! - the same clause's empty ANSWER, which is not a failure, and its `0`
//!   deadline, which asks and returns at once.
//! - `[os.net.listen.opts]`'s own equality: `net_listen_with(addr, false, 0)`
//!   IS `net_listen(addr)`, call for call.
//! - the two constructs this machine refuses BY NAME, asserted on the
//!   `x-unsupported` string itself, because "refused by name" is a claim
//!   about the WORDS and nothing else checks them.
//! - `[os.proc.inherit]`'s served half: an EMPTY inherit set is `os_spawn`
//!   with the program named apart, so it reaches the spawn and answers the
//!   spawn's own row rather than the refusal.
//! - `[os.cpus]`'s relations at the builtin, since the witness's are the
//!   run's.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

fn write_program(dir: &Path, source: &str) -> PathBuf {
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("written");
    entry
}

fn run_program(dir: &Path, source: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("run")
        .arg(write_program(dir, source))
        .output()
        .expect("lupin runs")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8 stdout")
}

/// The `x-unsupported` string of a program this machine declines, or a panic
/// naming the verdict that came back instead.
fn refusal(dir: &Path, source: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(write_program(dir, source))
        .arg("--json")
        .output()
        .expect("lupin observes");
    let record: serde_json::Value = serde_json::from_str(&stdout_of(&output)).expect("json");
    assert_eq!(record["verdict"], "unsupported", "{record}");
    // `[proto.record.phase]`: an `unsupported` verdict never claims `run`,
    // and it names the deepest rung this machine COMPLETED. That is
    // `resolve` here — typecheck, mem and wir are the compiler's half — even
    // though the refusal itself happens at a builtin call. The counterparty
    // reports `mem` for the same two constructs because it refuses them at
    // lowering; the rungs differ, the verdict does not, and
    // `[proto.cmp.defined-divergence]` compares neither.
    assert_eq!(record["phase_reached"], "resolve", "{record}");
    record["x-unsupported"]
        .as_str()
        .unwrap_or_else(|| panic!("no x-unsupported on {record}"))
        .to_owned()
}

// ---------------------------------------------------------------------------
// `[os.cpus]` and s90's `os_exe`
// ---------------------------------------------------------------------------

#[test]
fn the_machine_answers_its_own_size_and_its_own_path() {
    // Relations only, never a value: the count is the runner's, and the path
    // is the host's. `os_cpus` is always `>= 1` when it answers and two reads
    // in one run agree, because it is the machine's size and not a sample.
    let dir = scratch("cores-cpus");
    let output = run_program(
        &dir,
        "fn main() -> !int {\n\
         \x20   let n = os_cpus()?\n\
         \x20   let again = os_cpus()?\n\
         \x20   let exe = os_exe()?\n\
         \x20   let at_least_one = n >= 1\n\
         \x20   let stable = again == n\n\
         \x20   let named = exe.len > 0\n\
         \x20   print(\"cpus {at_least_one} {stable} exe {named}\")\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(stdout_of(&output), "cpus true true exe true\n");
    assert_eq!(output.status.code(), Some(0));
}

// ---------------------------------------------------------------------------
// `[os.net.wait]`
// ---------------------------------------------------------------------------

#[test]
fn an_empty_set_is_the_io_row_only_when_the_deadline_is_unbounded() {
    // The clause's two halves, which a corpus witness cannot write together:
    // an empty set with an unbounded deadline is `io` (nothing could ever end
    // that wait — `time_sleep_ms` is the call that means to sleep), while an
    // empty set with a BOUNDED deadline is the ordinary empty ANSWER, and a
    // `0` deadline asks and returns at once.
    let dir = scratch("cores-wait-empty");
    let output = run_program(
        &dir,
        "fn main() -> !int {\n\
         \x20   let empty = List[int]()\n\
         \x20   var unbounded_is_io = false\n\
         \x20   let bad = net_wait(empty, 0 - 1) else |_| {\n\
         \x20       unbounded_is_io = true\n\
         \x20       List[int]()\n\
         \x20   }\n\
         \x20   let _ = bad.len\n\
         \x20   let bounded = net_wait(empty, 5)?\n\
         \x20   let at_once = net_wait(empty, 0)?\n\
         \x20   print(\"io {unbounded_is_io} bounded {bounded.len} at_once {at_once.len}\")\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(stdout_of(&output), "io true bounded 0 at_once 0\n");
}

#[test]
fn the_answer_is_the_subset_in_the_callers_order() {
    // Two listeners, both dialled, one set: the answer names both and it
    // names them in the order the caller gave, not the order they woke.
    // Then the same set reversed answers reversed, which is the sentence
    // "the SUBSET, in the caller's order" with the only test that can tell
    // an ordered answer from a sorted one.
    let dir = scratch("cores-wait-order");
    let output = run_program(
        &dir,
        "fn main() -> !int {\n\
         \x20   let a = net_listen(\"127.0.0.1:0\")?\n\
         \x20   let b = net_listen(\"127.0.0.1:0\")?\n\
         \x20   let pa = net_port(a)?\n\
         \x20   let pb = net_port(b)?\n\
         \x20   let ca = net_connect(\"127.0.0.1:{pa}\")?\n\
         \x20   let cb = net_connect(\"127.0.0.1:{pb}\")?\n\
         \x20   let forward = List[int]()\n\
         \x20   (mut forward).push(a)\n\
         \x20   (mut forward).push(b)\n\
         \x20   let back = List[int]()\n\
         \x20   (mut back).push(b)\n\
         \x20   (mut back).push(a)\n\
         \x20   let one = net_wait(forward, 5000)?\n\
         \x20   let two = net_wait(back, 5000)?\n\
         \x20   var ordered = false\n\
         \x20   if one.len == 2 && two.len == 2 {\n\
         \x20       ordered = one[0] == a && one[1] == b && two[0] == b && two[1] == a\n\
         \x20   }\n\
         \x20   print(\"ordered {ordered}\")\n\
         \x20   net_close(ca)?\n\
         \x20   net_close(cb)?\n\
         \x20   net_close(a)?\n\
         \x20   net_close(b)?\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(stdout_of(&output), "ordered true\n");
}

// ---------------------------------------------------------------------------
// `[os.net.listen.opts]`
// ---------------------------------------------------------------------------

#[test]
fn the_option_less_shape_is_net_listen_call_for_call() {
    // The clause's own equality. The handle is the ordinary listener, so
    // `net_port`, `net_deadline`, `net_accept` and `net_close` serve it
    // without knowing which of the two calls minted it.
    let dir = scratch("cores-listen-with");
    let output = run_program(
        &dir,
        "fn main() -> !int {\n\
         \x20   let srv = net_listen_with(\"127.0.0.1:0\", false, 16)?\n\
         \x20   let port = net_port(srv)?\n\
         \x20   net_deadline(srv, 5000)?\n\
         \x20   let cli = net_connect(\"127.0.0.1:{port}\")?\n\
         \x20   let conn = net_accept(srv)?\n\
         \x20   net_write(conn, \"howl\")?\n\
         \x20   let heard = net_read(cli, 16)?\n\
         \x20   net_close(cli)?\n\
         \x20   net_close(conn)?\n\
         \x20   net_close(srv)?\n\
         \x20   let bound = port > 0\n\
         \x20   print(\"served {bound} {heard}\")\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(stdout_of(&output), "served true howl\n");
}

#[test]
fn a_non_loopback_bind_refuses_by_name_through_the_option_call_too() {
    // The v0 surface's discipline does not lapse because the call grew two
    // options: a non-loopback address is refused BY NAME here exactly as
    // `net_listen` refuses it, and never observed.
    let dir = scratch("cores-listen-with-outside");
    let reason = refusal(
        &dir,
        "fn main() -> !int {\n\
         \x20   let srv = net_listen_with(\"0.0.0.0:0\", false, 0)?\n\
         \x20   net_close(srv)?\n\
         \x20   0\n\
         }\n",
    );
    assert!(reason.contains("non-loopback"), "{reason}");
}

// ---------------------------------------------------------------------------
// The two refusals by name — the WORDS are the claim
// ---------------------------------------------------------------------------

/// The construct strings s137 published for the checked machine, mirrored
/// here byte for byte. `[proto.cmp.defined-divergence]` never compares an
/// `unsupported`, so nothing else would ever notice these drifting — which
/// is exactly why they are asserted.
#[test]
fn the_two_declined_constructs_are_named_in_s137s_own_words() {
    let dir = scratch("cores-adopt");
    assert_eq!(
        refusal(
            &dir,
            "fn main() -> !int {\n\
             \x20   let l = net_adopt_listener(3)?\n\
             \x20   net_close(l)?\n\
             \x20   0\n\
             }\n",
        ),
        "listener adoption in checked execution"
    );

    let dir = scratch("cores-inherit");
    assert_eq!(
        refusal(
            &dir,
            "fn main() -> !int {\n\
             \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
             \x20   let argv = List[str]()\n\
             \x20   let inherit = List[int]()\n\
             \x20   (mut inherit).push(srv)\n\
             \x20   let kid = os_spawn_with(os_exe()?, argv, inherit)?\n\
             \x20   let code = os_wait(kid)?\n\
             \x20   print(\"{code}\")\n\
             \x20   0\n\
             }\n",
        ),
        "fd inheritance across os_spawn_with in checked execution"
    );
}

// ---------------------------------------------------------------------------
// `[os.proc.inherit]`'s served half
// ---------------------------------------------------------------------------

#[test]
fn an_empty_inherit_set_is_os_spawn_with_the_program_named_apart() {
    // The clause serves the empty set everywhere, including on the machine
    // that refuses the non-empty one — so the call must REACH the spawn and
    // answer the spawn's own row. A program no host has is `not_found`, and
    // no child is ever started, which is what keeps this deterministic on
    // every runner (`corpus/os/spawn_rows.lu`'s discipline).
    let dir = scratch("cores-spawn-with-empty");
    let output = run_program(
        &dir,
        "fn main() -> !int {\n\
         \x20   let argv = List[str]()\n\
         \x20   let inherit = List[int]()\n\
         \x20   (mut argv).push(\"--version\")\n\
         \x20   os_spawn_with(\"lupin-no-such-program-a1b2c3\", argv, inherit) else |e| match e {\n\
         \x20       not_found => { print(\"not_found\"); -1 },\n\
         \x20       _ => { print(\"other\"); -1 },\n\
         \x20   }\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(stdout_of(&output), "not_found\n");
}

//! Six programs lupin 0.1.62 ran to an answer the specification does not
//! give, with no diagnostic (wolf-interp #325, #229, #256, #228 and #117's
//! spawn row). Each row drives the built binary over a fixture under
//! `tests/fixtures/silent_answers_l1/` and states the clause it holds lupin
//! to. Beside each row is what the released wolf 0.2.40 answers on the same
//! fixture, measured black-box on aarch64-apple-darwin (`wolf run` for the
//! native tier, `wolf conform-run FILE --json --checked` for the checked
//! machine); those answers are data, never compiler source.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/silent_answers_l1")
        .join(name)
}

/// `lupin ARGS… main.lu`, run in the fixture's own directory (a directory
/// is a module, D32) with the null device for stdin.
fn lupin(name: &str, args: &[&str]) -> Output {
    let dir = fixture_dir(name);
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(args)
        .arg("main.lu")
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .expect("lupin runs")
}

/// The observation record `lupin conform-run --json` prints last.
fn record(name: &str) -> serde_json::Value {
    let out = lupin(name, &["conform-run", "--json"]);
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .rev()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .unwrap_or_else(|| panic!("{name}: no record in {out:?}"))
}

fn assert_exit0_stdout(name: &str, stdout: &str) {
    let r = record(name);
    assert_eq!(r["verdict"], "exit(0)", "{name}: {r}");
    assert_eq!(r["stdout_inline"], stdout, "{name}: {r}");
}

/// #325, `[gram.expr.variant]`: `Line.Void` is a value of `Line`, and
/// `.cents()` on it is an ordinary method call, whether the receiver is a
/// binding, a payload variant built by a call, or the payload-less variant
/// written as a path. wolf 0.2.40: native `0 0 3`, checked `exit(0)` with
/// the same three lines. lupin 0.1.62 printed `Line.Void` for the second.
#[test]
fn variant_path_method_call() {
    assert_exit0_stdout("variant_method", "0\n0\n3\n");
}

/// #229, `[mem.static]`: module state is "one value for the whole run of the
/// program, shared by every task".
///
/// - `module_state_race`: two scope tasks write a module `var` with no order
///   between them. That is a data race (`[conc.mm.race.1]`), and a machine
///   that detects races halts with `trap(race)` (`[conc.mm.race.3]`) at the
///   second task's read of the variable. wolf 0.2.40: checked `trap(race)`,
///   `x-trap-pos` `[6, 16]` (`conc.mm.race.3`); native `2`. lupin 0.1.62
///   gave each task a private copy: `exit(0)`, `0`, the writes lost.
/// - `module_state_joined`: the same two writes, each ordered by its own
///   scope's join (`[conc.mm.hb]`). wolf 0.2.40: native `2`, checked
///   `exit(0)` `2`. lupin 0.1.62: `0`.
/// - `module_state_proc`: a proc writes, `main` reads after `join`
///   (`[conc.mm.hb.proc]`). wolf 0.2.40: native `7 1`, checked `exit(0)`
///   `7 1`. lupin 0.1.62: `7 0`.
#[test]
fn task_writes_module_state() {
    let r = record("module_state_race");
    assert_eq!(r["verdict"], "trap(race)", "module_state_race: {r}");
    assert_eq!(
        r["x-trap-span"],
        serde_json::json!([136, 140]),
        "the race is reported at the second task's read of `hits`: {r}"
    );
    assert_exit0_stdout("module_state_joined", "2\n");
    assert_exit0_stdout("module_state_proc", "7 1\n");
}

/// #229, `[conc.task.par.cost]`: "tasks run with the process root as their
/// ambient region" (and `[exec.checked.task]`: a task's allocations land in
/// its proc's region, or the run's root for the root domain). A scope task
/// spawned inside `region r` therefore does not allocate in `r`. wolf
/// 0.2.40: native and checked `built 100`, `grew false`. lupin 0.1.62
/// seeded the task with the spawner's ambient region: `grew true`.
#[test]
fn task_allocates_in_its_own_region() {
    assert_exit0_stdout("task_region", "built 100\ngrew false\n");
}

/// #256, `[type.numlit.propagate]`: adoption reaches through an arithmetic
/// term, and "into a call it reaches only the declared parameter type" — a
/// call argument is a literal term's context, as an annotation is.
///
/// - `literal_arg_wrapping`: `id(1 << 12)` with `id(x: wrapping[u8])`: the
///   count masks to the width (`[type.int.shift]` (6)), so `16`. wolf
///   0.2.40: native and checked `16`. lupin 0.1.62: `4096`.
/// - `literal_arg_method`: the same through an impl method,
///   `p.id(1 << 12)`. wolf 0.2.40: `16` on both. lupin 0.1.62: `4096`.
/// - `literal_arg_plain`: `id(255 + 1 - 1)` with `id(x: u8)`: the term is
///   `u8` arithmetic and `255 + 1` leaves the type (X3). wolf 0.2.40: native
///   `wolf-trap: overflow` (exit 134), checked `trap(overflow)`. lupin
///   0.1.62: `255`.
/// - `literal_arg_float`: `half(1 / 2)` with `half(x: f64)`: both literals
///   adopt `f64` before `/` runs, the clause's own pinned consequence. wolf
///   0.2.40: `0.5` on both. lupin 0.1.62: `0`.
#[test]
fn call_argument_types_a_literal_term() {
    assert_exit0_stdout("literal_arg_wrapping", "16\n");
    assert_exit0_stdout("literal_arg_method", "16\n");
    let r = record("literal_arg_plain");
    assert_eq!(r["verdict"], "trap(overflow)", "literal_arg_plain: {r}");
    assert_exit0_stdout("literal_arg_float", "0.5\n");
}

/// #228, `[conc.proc.exit]`: a proc contains a TRAP as `fault(kind)`, whose
/// `kind` is one name from `[conf.trap.set]`; a UB finding is no trap kind,
/// so no exit reason carries it, and it ends the run with its own verdict
/// (`[exec.checked.task]`: "a proc does not contain a UB finding").
/// wolf 0.2.40: checked `ub(mem.ub)`, row P1, `mem.prov.state`, at the read
/// after `c.free`; native `value 7` (the UB, run). lupin 0.1.62 handed the
/// finding to the join as `error`: `exit(0)`, `joined: error`.
#[test]
fn ub_inside_a_proc_ends_the_program() {
    let r = record("ub_in_proc");
    assert_eq!(r["verdict"], "ub(mem.ub)", "ub_in_proc: {r}");
    assert_eq!(r["x-ub-row"], "P1", "ub_in_proc: {r}");
    assert_eq!(r["x-ub-clause"], "mem.prov.state", "ub_in_proc: {r}");
    assert_eq!(
        r["stdout_inline"],
        serde_json::Value::Null,
        "ub_in_proc: {r}"
    );
}

/// #228's twin, in the same match: a refusal on a proc's task. This machine
/// refuses a non-empty `os_spawn_with` inherit set by name, so it cannot
/// observe this program; `[conc.proc.exit]`'s `error(value)` is an error
/// value the program made crossing the boundary, and a refusal is none, so
/// the honest record is `unsupported` (`[proto.record.unsupported]`), as
/// the checked machine answers (`[exec.checked.task]`: "a refusal ... on
/// any task ends the run with that answer at once"). wolf 0.2.40: checked
/// `unsupported`; native `refused io`, `value 1`. lupin 0.1.62 delivered
/// the refusal to the join as `error`: `exit(0)`, `joined: error` — an
/// observation it never made.
#[test]
fn refusal_inside_a_proc_ends_the_program() {
    let r = record("refusal_in_proc");
    assert_eq!(r["verdict"], "unsupported", "refusal_in_proc: {r}");
}

/// #117's spawn row, `[os.proc.spawn]`: a child started by `os_spawn` gets
/// descriptor 0 wired to the null device and descriptors 1 and 2
/// inherited, on every tier. The child here prints `OUT`, `ERR` on its
/// stderr, then reads end-of-file and prints `READ:`. wolf 0.2.40: native
/// `OUT`, `READ:`, `status 0` on stdout and `ERR` on stderr; the checked
/// machine the same, with the child's bytes on the host stream ahead of the
/// record and `stdout_inline` `status 0` (the clause's stated asymmetry).
/// lupin 0.1.62 wired all three to the null device: no `OUT`, `ERR` or
/// `READ:` anywhere.
#[cfg(unix)]
#[test]
fn spawned_child_keeps_standard_streams() {
    let out = lupin("spawn_streams", &["run"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "OUT\nREAD:\nstatus 0\n",
        "{out:?}"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("ERR\n"),
        "{out:?}"
    );

    let out = lupin("spawn_streams", &["conform-run", "--json"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.starts_with("OUT\nREAD:\n{"), "{out:?}");
    let r = record("spawn_streams");
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(r["stdout_inline"], "status 0\n", "{r}");
}

//! is65 — wolf-interp#163 (wave 52): a flow out of a call's argument list
//! withdraws every claim and every protector of that call, the receiver's
//! included.
//!
//! `[mem.tier0.excl.4]` (ruling #17): a `mut` argument's claim "takes effect
//! when the call is entered"; `[mem.prov.tag]`: parameter entry "is
//! protector-equivalent: the tag is protected for the whole call". A list
//! that a `?`, a `return`, a `break`, a `continue` or a trap leaves early
//! never enters its call, so there is no extent for the claim to take effect
//! for and nothing for the protector to protect. Through 0.1.43 the
//! protected Reserved child stayed in the tree and the next write through
//! the place was a foreign write to a protected tag: `ub(mem.ub)`
//! (`mem.prov.state`, row P1) on a program wolf 0.2.19 runs on checked,
//! native and release — and the claim stayed held until its scope popped,
//! which the REPL's top level never does.
//!
//! One directory per shape under `tests/rulings_is65/`, each with an
//! `expected.toml` whose `[expected]` lupin cell is the ruled answer and
//! whose `[trunk]` table is what lupin trunk `6d6cde5` and wolf 0.2.19
//! answered. The leak rows print wolf's bytes; the controls (a trap, a
//! `take`, a field, an element, an impl and a view-set receiver) are
//! unchanged on both sides.
//!
//! The runner is is62's (`tests/rulings_is62.rs`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is65")
        .join(name)
}

/// The ruled lupin cell: the verdict, the stdout the run must print, and for
/// a trap the clause and span its record must carry.
#[derive(Debug)]
struct Ruled {
    verdict: String,
    stdout: Option<String>,
    clause: Option<String>,
    span: Option<[u64; 2]>,
}

/// Reads `[expected]`'s `lupin = { … }` line — one inline table on one line,
/// the shape every file here has; a file that stops having it fails loudly.
fn ruled(name: &str) -> Ruled {
    let text = std::fs::read_to_string(witness_dir(name).join("expected.toml"))
        .expect("the witness carries its expected.toml");
    let mut in_expected = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_expected = line == "[expected]";
            continue;
        }
        if !in_expected || !line.starts_with("lupin") {
            continue;
        }
        let span = line.split("span = [").nth(1).map(|rest| {
            let list = rest.split(']').next().expect("a closed span");
            let mut ends = list
                .split(',')
                .map(|n| n.trim().parse::<u64>().expect("an offset"));
            [ends.next().expect("a start"), ends.next().expect("an end")]
        });
        return Ruled {
            verdict: quoted_after(line, "verdict = ").expect("a verdict"),
            stdout: quoted_after(line, "stdout = ").map(|s| s.replace("\\n", "\n")),
            clause: quoted_after(line, "clause = "),
            span,
        };
    }
    panic!("{name}/expected.toml has no [expected] lupin cell");
}

fn quoted_after(line: &str, key: &str) -> Option<String> {
    let rest = line.split(key).nth(1)?;
    let rest = rest.strip_prefix('"')?;
    Some(rest.split('"').next()?.to_owned())
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("rulings_is65")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    std::fs::copy(witness_dir(name).join("main.lu"), dir.join("main.lu")).expect("copied");
    dir
}

/// `lupin conform-run main.lu --json` from `dir`: the whole record.
fn conform_run(dir: &Path) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["conform-run", "main.lu", "--json"])
        .current_dir(dir)
        .output()
        .expect("lupin runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "the tool succeeded: {output:?}"
    );
    serde_json::from_slice(&output.stdout).expect("one JSON record")
}

fn run(name: &str) {
    let ruled = ruled(name);
    let record = conform_run(&scratch(name));
    let verdict = record["verdict"].as_str().expect("a verdict");
    assert_eq!(
        verdict, ruled.verdict,
        "{name}: lupin answered `{verdict}`, ruled `{}` — record {record}",
        ruled.verdict
    );
    if let Some(stdout) = &ruled.stdout {
        assert_eq!(
            record["stdout_inline"].as_str().unwrap_or(""),
            stdout.as_str(),
            "{name}: the ruled stdout — record {record}"
        );
    }
    if let Some(clause) = &ruled.clause {
        assert_eq!(
            record["x-trap-clause"].as_str(),
            Some(clause.as_str()),
            "{name}: the trap's clause — record {record}"
        );
    }
    if let Some([start, end]) = ruled.span {
        assert_eq!(
            record["x-trap-span"],
            serde_json::json!([start, end]),
            "{name}: the trap's span — record {record}"
        );
    }
}

// -- the argument list: each exit kind ------------------------------------

/// wolf-interp#163's program: `put(mut xs, v(ok)?)`, then `(mut xs).push(3)` in the caller.
#[test]
fn try_arg() {
    run("try_arg");
}

/// As `try_arg`, with the next write made by a `mut` argument, `put(mut xs, 3)`.
#[test]
fn try_arg_then_mut_arg() {
    run("try_arg_then_mut_arg");
}

/// wolf-interp#163's `while`: `put(mut xs, if i == 2 { continue } else { i })`.
#[test]
fn continue_arg() {
    run("continue_arg");
}

/// The same `continue` in a `for`.
#[test]
fn continue_for() {
    run("continue_for");
}

/// `put(mut xs, if stop { return 7 } else { 5 })`.
#[test]
fn return_arg() {
    run("return_arg");
}

/// `put(mut xs, if i == 2 { break } else { i })`, then a push after the loop.
#[test]
fn break_arg() {
    run("break_arg");
}

/// `put(mut xs, ys[9])`: `trap(bounds)` on every machine; nothing after it runs (control).
#[test]
fn trap_arg() {
    run("trap_arg");
}

// -- a nested call inside an argument --------------------------------------

/// `outer(mut xs, inner(mut ys, v(ok)?))`: both lists abandoned, both claims withdrawn.
#[test]
fn try_nested_inner_claim() {
    run("try_nested_inner_claim");
}

/// `outer(mut xs, inner(mut ys, 1), v(ok)?)`: the nested call returned; the outer list is abandoned.
#[test]
fn try_after_nested_done() {
    run("try_after_nested_done");
}

/// `outer(inner(mut ys, v(ok)?), mut xs)`: the inner claim withdrawn, the outer never made.
#[test]
fn try_nested_first_arg() {
    run("try_nested_first_arg");
}

// -- the other claims of a list ------------------------------------------

/// `both(ys, v(ok)?)`: a `read` lend's Frozen child is withdrawn too.
#[test]
fn read_lend_try() {
    run("read_lend_try");
}

/// `bump(mut n, v(ok)?)` on a scalar place.
#[test]
fn scalar_mut_try() {
    run("scalar_mut_try");
}

/// `pair(mut xs, mut ys, v(ok)?)`: every claim of the list.
#[test]
fn two_muts_try() {
    run("two_muts_try");
}

/// `eat(take xs, v(ok)?)`: the move at the call site stands on every machine (control).
#[test]
fn take_then_try() {
    run("take_then_try");
}

/// `put(mut b.xs, v(ok)?)`: a field place (control).
#[test]
fn field_mut_try() {
    run("field_mut_try");
}

/// `bump(mut xs[0], v(ok)?)`: an element place (control).
#[test]
fn elem_mut_try() {
    run("elem_mut_try");
}

// -- the receiver's claim ------------------------------------------------

/// wolf-interp#163's receiver form: `(mut xs).push(v(ok)?)`.
#[test]
fn recv_try() {
    run("recv_try");
}

/// `(mut xs).push(if i == 2 { continue } else { i })`.
#[test]
fn recv_continue() {
    run("recv_continue");
}

/// `(mut xs).push(if stop { return 7 } else { 5 })`.
#[test]
fn recv_return() {
    run("recv_return");
}

/// `(mut xs).push(if i == 2 { break } else { i })`.
#[test]
fn recv_break() {
    run("recv_break");
}

/// `(mut xs).push(inner(mut ys, v(ok)?))`: the receiver's claim and the nested list's.
#[test]
fn recv_nested_try() {
    run("recv_nested_try");
}

/// `(mut c).add(v(ok)?)` on an impl receiver, a field write after (control).
#[test]
fn impl_recv_try() {
    run("impl_recv_try");
}

/// `(mut p).set_x(v(ok)?)` on a view-set receiver, field writes after (control).
#[test]
fn viewset_recv_try() {
    run("viewset_recv_try");
}

// -- a trap caught by nothing: the REPL survives it -------------------------

/// A program ends at a trap, so nothing after `put(mut xs, ys[9])` can look;
/// a REPL session outlives it (`[repl.trap.alive]`, "the world is as the
/// fault left it"). The call was never entered, so its claim on `xs` is not
/// part of that world: the next line's push runs and `xs` answers `[1, 3]`.
/// Through 0.1.43 the claim stayed held — `trap(exclusivity): xs is held as
/// mut` on both following lines — with the protector beneath it.
#[test]
fn repl_trap_then_write() {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("repl")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(
            b"fn put(mut xs: List[int], k: int) { (mut xs).push(k) }\n\
              var xs = [1]\n\
              let ys = [0]\n\
              put(mut xs, ys[9])\n\
              (mut xs).push(3)\n\
              xs\n",
        )
        .expect("stdin accepts the session");
    let output = child.wait_with_output().expect("the session ends");
    assert!(output.status.success(), "the session exits 0");
    let out = String::from_utf8(output.stdout).expect("utf-8");
    assert!(out.contains("trap(bounds): index 9 is outside"), "{out}");
    assert!(out.contains("[repl.trap.alive]"), "{out}");
    assert!(
        !out.contains("trap(exclusivity)"),
        "the abandoned list's claim on `xs` outlived the trap:\n{out}"
    );
    assert!(out.contains("[1, 3]"), "the push after the trap landed:\n{out}");
}

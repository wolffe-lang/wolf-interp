//! s208 — ruling #34 = A (the maintainer, 2026-10-03; wolf-lang#541 part 2),
//! lupin's half: wolf-interp#179.
//!
//! `[type.unit.context]` lists "the then-block of an `if` with no `else`, and
//! every block of an `if … else if …` chain that ends without one" as unit
//! context without exception, and `[type.unit.discard]` says a `!T` tail
//! there is discarded, its row with it. is68 (#103) made such an `if` answer
//! `()` but kept one exception: a raise out of the taken block left as the
//! `if`'s value, because every compiler tier did that at a fallible fn's
//! tail. The ruling removes the exception on all four machines, so the raise
//! is discarded wherever the `if` sits — at a fallible fn's tail
//! (`tail_*`), and bound by a `let` (#179's own program, `if_value_179`). A
//! `?` or a `return` inside the block still leaves: those are not the
//! block's value (`tail_leaves`).
//!
//! One directory per row under `tests/rulings_s208/`, each with an
//! `expected.toml` whose `[expected]` lupin cell is the ruled answer and whose
//! `[trunk]` table is what lupin trunk `9f4e4a1` (the published 0.1.45) and
//! wolf-lang trunk `8e36bc1a` (the published 0.2.22) answered on kasumi. The
//! programs are the compiler's s208 corpus rows, byte for byte.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_s208")
        .join(name)
}

fn quoted_after(line: &str, key: &str) -> Option<String> {
    let rest = line.split(key).nth(1)?;
    let rest = rest.strip_prefix('"')?;
    Some(rest.split('"').next()?.to_owned())
}

/// `[expected]`'s `lupin = { verdict, stdout }` — one inline table, one line.
fn ruled(name: &str) -> (String, String) {
    let text = std::fs::read_to_string(witness(name).join("expected.toml"))
        .expect("the witness carries its expected.toml");
    let mut in_expected = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_expected = line == "[expected]";
            continue;
        }
        if in_expected && line.starts_with("lupin") {
            return (
                quoted_after(line, "verdict = ").expect("a verdict"),
                quoted_after(line, "stdout = ")
                    .expect("a stdout")
                    .replace("\\n", "\n"),
            );
        }
    }
    panic!("{name}/expected.toml has no [expected] lupin cell");
}

fn run(name: &str) {
    let (verdict, stdout) = ruled(name);
    let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["conform-run", "main.lu", "--json"])
        .current_dir(witness(name))
        .output()
        .expect("lupin runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "the tool succeeded: {output:?}"
    );
    let record: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON record");
    assert_eq!(
        (
            record["verdict"].as_str().unwrap_or(""),
            record["stdout_inline"].as_str().unwrap_or("")
        ),
        (verdict.as_str(), stdout.as_str()),
        "{name} (wolf-interp#179): record {record}"
    );
}

/// The plain tail, a method, a nested else-less `if`.
/// Red at trunk 9f4e4a1: `a true` … `d true`.
#[test]
fn tail_if() {
    run("tail_if");
}

/// The closure form: a closure checked against `fn(bool) -> () ! {bad}`.
/// Red at trunk: `run true`.
#[test]
fn tail_closure() {
    run("tail_closure");
}

/// An else-if chain with no final `else` at the tail. Red at trunk: `g true`
/// (native and release already answered `g false`).
#[test]
fn tail_chain() {
    run("tail_chain");
}

/// A value-carrying row at the tail. Red at trunk: `g true`.
#[test]
fn tail_value_row() {
    run("tail_value_row");
}

/// A bare tag as the then-block's tail: discarded, as in the statement form.
/// Red at trunk: `g true`.
#[test]
fn tail_bare_tag() {
    run("tail_bare_tag");
}

/// #179's program: an else-less `if` bound by `let` is `()` when it raises.
/// Red at trunk: `() none`.
#[test]
fn if_value_179() {
    run("if_value_179");
}

/// Control: `?`, `return bad`, an `else`, a `?` statement. Green at trunk, and
/// it must stay green.
#[test]
fn tail_leaves() {
    run("tail_leaves");
}

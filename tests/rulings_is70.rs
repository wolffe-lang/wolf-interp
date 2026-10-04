//! is70 — the gaps the PAX lanes filed in lupin (wolf-interp#174, #175,
//! #178, #179, #180, #181, #182, #184), each a row where the compiler at
//! wolf-lang trunk `50830027` answers on all three of its lanes and lupin
//! answered otherwise.
//!
//! One directory per row under `tests/rulings_is70/`, each with a `main.lu`
//! and an `expected.toml`. `[expected]`'s `lupin` cell is the answer this
//! machine must give: the verdict, the stdout, and — where the compiler
//! refuses — the first diagnostic's code and span (`[proto.record.first]`
//! makes the first one comparable). An optional `args` line adds
//! `conform-run` arguments. `[trunk]` records what lupin trunk `8d82031`
//! and the compiler's `--checked`/`--native`/`--release` answered on
//! kasumi (`~/lanes/is70/`), so every row's red-at-trunk is on file beside
//! it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is70")
        .join(name)
}

fn quoted_after(line: &str, key: &str) -> Option<String> {
    let rest = line.split(key).nth(1)?;
    let rest = rest.strip_prefix('"')?;
    Some(rest.split('"').next()?.replace("\\n", "\n"))
}

struct Ruled {
    verdict: String,
    stdout: String,
    /// `CODE@[lo,hi]`, empty when the row runs.
    first: String,
    args: Vec<String>,
}

/// `[expected]`'s `lupin = { verdict, stdout, first }` — one inline table on
/// one line — and an optional top-level `args = "…"`.
fn ruled(name: &str) -> Ruled {
    let text = std::fs::read_to_string(witness(name).join("expected.toml"))
        .expect("the witness carries its expected.toml");
    let mut section = String::new();
    let mut args = Vec::new();
    let mut cell = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line.to_owned();
            continue;
        }
        if section.is_empty() && line.starts_with("args") {
            args = quoted_after(line, "args = ")
                .expect("args is a string")
                .split_whitespace()
                .map(str::to_owned)
                .collect();
        }
        if section == "[expected]" && line.starts_with("lupin") {
            cell = Some(line.to_owned());
        }
    }
    let cell = cell.unwrap_or_else(|| panic!("{name}/expected.toml has no [expected] lupin cell"));
    Ruled {
        verdict: quoted_after(&cell, "verdict = ").expect("a verdict"),
        stdout: quoted_after(&cell, "stdout = ").expect("a stdout"),
        first: quoted_after(&cell, "first = ").unwrap_or_default(),
        args,
    }
}

fn run(name: &str, issue: &str) {
    let want = ruled(name);
    let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["conform-run", "main.lu", "--json"])
        .args(&want.args)
        .current_dir(witness(name))
        .output()
        .expect("lupin runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{name} ({issue}): the tool answers with a record: {output:?}"
    );
    let record: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON record");
    let first = record["diagnostics"]
        .as_array()
        .and_then(|ds| ds.first())
        .map(|d| {
            let span = &d["span"];
            format!(
                "{}@[{},{}]",
                d["code"].as_str().unwrap_or(""),
                span[0],
                span[1]
            )
        })
        .unwrap_or_default();
    assert_eq!(
        (
            record["verdict"].as_str().unwrap_or(""),
            record["stdout_inline"].as_str().unwrap_or(""),
            first.as_str(),
        ),
        (
            want.verdict.as_str(),
            want.stdout.as_str(),
            want.first.as_str()
        ),
        "{name} ({issue}): record {record}"
    );
}

// ---- wolf-interp#184: `*p` is `p[0]`, under the same ring (E1301) --------

/// `*p = 3` in safe code. Red at trunk: `exit(0)`.
#[test]
fn deref_write_outside() {
    run("deref_write_outside", "wolf-interp#184");
}

/// `let a = *p` in safe code. Red at trunk: `exit(0)`, `0`.
#[test]
fn deref_read_outside() {
    run("deref_read_outside", "wolf-interp#184");
}

/// `*p += 2` in safe code. Red at trunk: `exit(0)`.
#[test]
fn deref_compound_outside() {
    run("deref_compound_outside", "wolf-interp#184");
}

/// The control: all three spellings inside `unsafe` run on every machine.
#[test]
fn deref_inside_ok() {
    run("deref_inside_ok", "wolf-interp#184");
}

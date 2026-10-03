//! is69 — wolffe-lang/wolf-interp#176: a `-> !T` function's private row is
//! what its body can raise (`01-grammar.md`: "`-> !T` error union with
//! inferred private row"; the compiler seals it per module, s15's
//! `wolf_sema::rows`), and `[type.row.match]` (ruling #21) judges a `match`
//! over its result against that row. lupin 0.1.44 read every such row as the
//! open row `{..}`, so a match with no `_` was E0801 whatever the body raised
//! — s202's `rows/eu_bind_empty_row_handled.lu` among them.
//!
//! One directory per shape under `tests/rulings_is69/`, each with an
//! `expected.toml` whose `[expected]` lupin cell is the ruled answer (with
//! `names`, a string the first `lupin check` diagnostic line must contain)
//! and whose `[trunk]` table is what lupin 0.1.44 and wolf-lang r26
//! `dfcc2f13` answered on kasumi
//! (`~/lanes/is69/evidence/probes-archives-0.1.44-0.1.43-wolf-dfcc2f13.log`).
//! The one control is named.
//!
//! The runner is is67's (`tests/rulings_is67.rs`), with `names` added.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is69")
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
    /// A string the first diagnostic line of `lupin check main.lu` must
    /// contain: the tag an E0801 names.
    names: Option<String>,
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
            names: quoted_after(line, "names = "),
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
        .join("rulings_is69")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    copy_tree(&witness_dir(name), &dir);
    dir
}

/// The witness's whole directory (a sibling module would ride along).
fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("the witness directory") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            std::fs::create_dir_all(&target).expect("module dir created");
            copy_tree(&entry.path(), &target);
        } else if entry.path().extension().is_some_and(|ext| ext == "lu") {
            std::fs::copy(entry.path(), &target).expect("copied");
        }
    }
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

/// `lupin check main.lu` from `dir`: the first line that carries a code.
fn first_diagnostic(dir: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["check", "main.lu"])
        .current_dir(dir)
        .output()
        .expect("lupin runs");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    stderr
        .lines()
        .find(|line| line.contains(": E"))
        .unwrap_or("")
        .to_owned()
}

fn run(name: &str) {
    let ruled = ruled(name);
    let dir = scratch(name);
    let record = conform_run(&dir);
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
    if let Some(names) = &ruled.names {
        let line = first_diagnostic(&dir);
        assert!(
            line.contains(names.as_str()),
            "{name}: the diagnostic names {names:?} — `lupin check` said {line:?}"
        );
    }
}

/// s202's row: `f`'s inferred row is empty, so `{ v => v + 1 }` covers the match (`43 42 42`).
#[test]
fn i69_issue_row() {
    run("i69_issue_row");
}

/// One raised tag: `{ Neg => -1, v => v }` over `{Neg}` (`4 -1`).
#[test]
fn i69_one_tag() {
    run("i69_one_tag");
}

/// `{ v => v }` over `{Neg}`: E0801 names `Neg`.
#[test]
fn i69_one_tag_missing() {
    run("i69_one_tag_missing");
}

/// The row through `?` from a declared callee: `{bad}` (`8 -1`).
#[test]
fn i69_try_callee() {
    run("i69_try_callee");
}

/// `{bad}` uncovered: E0801 names `bad`.
#[test]
fn i69_try_callee_missing() {
    run("i69_try_callee_missing");
}

/// A declared row flowing out of the tail: `{none}` (`5 -1`).
#[test]
fn i69_tail_call_row() {
    run("i69_tail_call_row");
}

/// `{none}` from the tail uncovered: E0801 names `none`.
#[test]
fn i69_tail_call_row_missing() {
    run("i69_tail_call_row_missing");
}

/// Recursion and mutual recursion: `{Under}`, `{Far, Near}` (`0 -1 10 -2`).
#[test]
fn i69_recursive() {
    run("i69_recursive");
}

/// `Near` reaches the row only through the cycle: E0801 names `Near`.
#[test]
fn i69_recursive_missing() {
    run("i69_recursive_missing");
}

/// Before the edit: `{Low}` covered (`4`).
#[test]
fn i69_row_grows_before() {
    run("i69_row_grows_before");
}

/// After the edit the row is `{Low, High}`; the unchanged match: E0801 names `High`.
#[test]
fn i69_row_grows_after() {
    run("i69_row_grows_after");
}

/// The edit answered with an arm for `High` (`4 -2`).
#[test]
fn i69_row_grows_covered() {
    run("i69_row_grows_covered");
}

/// A generic fn: `wrap[T]`'s row is `{Empty}` at `int` and `str` (`4 -1 x`).
#[test]
fn i69_generic() {
    run("i69_generic");
}

/// `wrap[T]`'s `{Empty}` uncovered: E0801 names `Empty`.
#[test]
fn i69_generic_missing() {
    run("i69_generic_missing");
}

/// A binding annotated `!int` takes its initializer's (empty) row (`43`).
#[test]
fn i69_let_annotated() {
    run("i69_let_annotated");
}

/// A tail `match` consuming a row and returning a binder raises nothing (`8`).
#[test]
fn i69_match_binder_tail() {
    run("i69_match_binder_tail");
}

/// A tail `xs.len` raises nothing (`1`).
#[test]
fn i69_member_tail() {
    run("i69_member_tail");
}

/// A tail `p.x` raises nothing (`3`).
#[test]
fn i69_field_tail() {
    run("i69_field_tail");
}

/// A `?` inside a closure is the closure's own (`1`).
#[test]
fn i69_closure_try() {
    run("i69_closure_try");
}

/// Control: a tail method call is not inferable; the row stays `{..}` and E0801 stands, as on 0.1.44.
#[test]
fn i69_unknown_method() {
    run("i69_unknown_method");
}

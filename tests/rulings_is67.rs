//! is67 — the maintainer's rulings #21 and #19 of 2026-10-02 (wave 52):
//!
//! - **#21, `[type.row.match]`** (wolffe-lang/wolf-lang#497): a `match` over a
//!   `T ! {row}` has two halves. A row arm names a tag of the row (binding
//!   its payload: `Io(e) => …`), a value arm is any pattern over `T`; an
//!   identifier that names a tag of the scrutinee's row is a row arm,
//!   anything else a value pattern; `_` covers what is left on both halves;
//!   a non-exhaustive match is E0801 naming the missing tag or the uncovered
//!   value half; the row is consumed; a tag that is also a constructor name
//!   reachable from `T` is refused by name. lupin 0.1.43 ran the miss path
//!   and BOUND the row arm's name on the hit path (`none => -1` took `5`).
//! - **#19, `[type.row.defer]`** (wolffe-lang/wolf-lang#498, s196's E0611): a
//!   `?` inside a `defer` or `errdefer` expression is refused at the resolve
//!   rung, never run. lupin 0.1.43 ran it and made the row the function's
//!   result.
//!
//! One directory per shape under `tests/rulings_is67/`, each with an
//! `expected.toml` whose `[expected]` lupin cell is the ruled answer and
//! whose `[trunk]` table is what lupin trunk `6d6cde5` (= 0.1.43) and wolf
//! 0.2.20 answered on kasumi (`~/lanes/is67/evidence/probes-*.log`). The
//! `m21_*` rows are #21's, the `d19_*` rows #19's; the controls are named.
//!
//! The runner is is65's (`tests/rulings_is65.rs`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is67")
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
        .join("rulings_is67")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    copy_tree(&witness_dir(name), &dir);
    dir
}

/// The witness's whole directory: `m21_imported_row` carries a sibling
/// module (`store/find.lu`) beside its `main.lu`.
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

// -- the row half and the value half (#21) ---------------------------------

/// The issue's shape, miss then hit: `match look(m, k) { none => -1, v => v }` prints `-1` then `5`.
#[test]
fn m21_call_tag_value() {
    run("m21_call_tag_value");
}

/// s191's m2: the fallible value bound to a local first, `{ none => -1, _ => 1 }`.
#[test]
fn m21_local_scrutinee() {
    run("m21_local_scrutinee");
}

/// Payload tags: `Io(code)` binds the payload, `Timeout` is bare, `v` is the value half.
#[test]
fn m21_payload_tags() {
    run("m21_payload_tags");
}

/// `_` covers what is left on the value half: `{ none => -1, _ => 1 }`.
#[test]
fn m21_wild_value_half() {
    run("m21_wild_value_half");
}

/// `_` covers what is left on the row half: `{ v => v, _ => -1 }`.
#[test]
fn m21_wild_row_half() {
    run("m21_wild_row_half");
}

/// Control: `{ _ => 7 }` covers both halves (unchanged).
#[test]
fn m21_wild_both() {
    run("m21_wild_both");
}

/// A row match inside a row arm and inside a value arm.
#[test]
fn m21_nested() {
    run("m21_nested");
}

/// The #492 shape: a `?` propagating out of the scrutinee leaves the function (`[type.row.else]`).
#[test]
fn m21_try_in_scrutinee() {
    run("m21_try_in_scrutinee");
}

/// The scrutinee is a `Map` index, `V ! {none}` by `[mem.map.absent]`.
#[test]
fn m21_map_index() {
    run("m21_map_index");
}

/// A two-tag row with every tag named and a value binder: exhaustive with no `_`.
#[test]
fn m21_two_tags_named() {
    run("m21_two_tags_named");
}

/// Control: `T` an enum, every variant named (already right on 0.1.43).
#[test]
fn m21_enum_value_half() {
    run("m21_enum_value_half");
}

/// `T` is `bool`: `true` and `false` cover the value half.
#[test]
fn m21_bool_value_half() {
    run("m21_bool_value_half");
}

/// The row is consumed: `(match …) + 1`.
#[test]
fn m21_result_consumed() {
    run("m21_result_consumed");
}

/// The row comes from a `use`d module's signature (`store.find`).
#[test]
fn m21_imported_row() {
    run("m21_imported_row");
}

/// Control: s191's m3, `else |e| match e { … }` keeps working beside the new form.
#[test]
fn m21_else_match_control() {
    run("m21_else_match_control");
}

// -- the two refusals of #21 -----------------------------------------------

/// Non-exhaustive, the row half: `{ v => v }` — E0801 names `none`.
#[test]
fn m21_missing_tag() {
    run("m21_missing_tag");
}

/// Non-exhaustive, the value half: `{ none => -1 }` — E0801 names the value half.
#[test]
fn m21_uncovered_value() {
    run("m21_uncovered_value");
}

/// A guarded row arm counts for nothing: `{ none if flag => -1, v => v }` is E0801.
#[test]
fn m21_guard_no_cover() {
    run("m21_guard_no_cover");
}

/// `{ Io(e) => e, v => v }` over `{Io(int), Timeout}` — E0801 names `Timeout`.
#[test]
fn m21_payload_missing_tag() {
    run("m21_payload_missing_tag");
}

/// `T` an enum with one variant unnamed — E0801 names `Green`.
#[test]
fn m21_enum_missing_variant() {
    run("m21_enum_missing_variant");
}

/// A tag that is also a constructor name reachable from `T`: refused by name before running.
#[test]
fn m21_tag_collision() {
    run("m21_tag_collision");
}

// -- a `?` under a `defer` (#19) -------------------------------------------

/// s191's p2: `defer print("deferred {key(ok)?}")` — E0611.
#[test]
fn d19_try_in_defer() {
    run("d19_try_in_defer");
}

/// s191's p3: a `?` in a call argument under an `else` inside an `errdefer` — E0611.
#[test]
fn d19_try_in_errdefer() {
    run("d19_try_in_errdefer");
}

/// A `?` in a binding inside a `defer` block — E0611.
#[test]
fn d19_try_in_defer_block() {
    run("d19_try_in_defer_block");
}

/// Control (PR #509's `defer_else_handles`): the row handled with `else` inside the deferred expression.
#[test]
fn d19_defer_else_control() {
    run("d19_defer_else_control");
}

/// Control: a `?` inside a closure under the `defer` is the closure's own propagation.
#[test]
fn d19_try_in_closure_in_defer() {
    run("d19_try_in_closure_in_defer");
}

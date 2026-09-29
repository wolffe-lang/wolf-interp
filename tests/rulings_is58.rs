//! is58 — the corpus rows wolf 0.2.18's gates pin lupin by version on,
//! run against their RULED lupin verdict.
//!
//! wolf-lang `ec56a08f` (0.2.18) asserts lupin's answer on these rows in
//! `crates/wolf_driver/tests/element_places_lanes.rs`,
//! `mut_param_return_lanes.rs` and `store_order_lanes.rs`, and pins 0.1.41's
//! measured answer by version where lupin had not mirrored the clause yet:
//! wolffe-lang/wolf-interp#144 (a `Map` value read out stays in the map),
//! #145 (a nested store evaluates its outer operands twice) and #146 (a whole
//! `mut` parameter moved out is not seen by the caller). Each row is copied
//! verbatim as `main.lu`, one directory each under `tests/rulings_is58/`, with
//! an `expected.toml` written from its gate: `[expected]` the ruled cell,
//! `[trunk]` lupin 0.1.41's measured one.
//!
//! Five rows rule an answer 0.1.41 did not give — `elem_key_reassigned_no_revive`,
//! `mut_param_moveout_whole`, `mut_param_moveout_one_path`,
//! `mut_param_moveout_map` (the trap) and `ctl_store_order_nested_index`
//! (each operand once). The others hold 0.1.41's answer and stand here as
//! the pins that the fixes do not reach past them: the stored-back twins, the
//! map rows that revive through the same key, and the header and sibling
//! element reads beside a moved part (#143).
//!
//! The runner is s182's and is56's: the built binary's
//! `conform-run main.lu --json`, from a scratch copy of the row's own
//! directory, the oracle read from the file's `[expected]` lupin cell.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is58")
        .join(name)
}

/// The ruled lupin cell: one verdict or a set of conformant ones, and the
/// stdout an `exit` must print.
#[derive(Debug)]
struct Ruled {
    verdicts: Vec<String>,
    stdout: Option<String>,
}

/// Reads `[expected]`'s `lupin = { … }` line. The files are s182's and
/// their shape is fixed (one inline table per machine, one line each), so a
/// reader for exactly that shape is enough — and a file that stops having
/// it fails here loudly rather than being half-read.
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
        let verdicts = if let Some(rest) = line.split("verdict = [").nth(1) {
            let list = rest.split(']').next().expect("a closed verdict list");
            list.split(',')
                .map(|v| v.trim().trim_matches('"').to_owned())
                .collect()
        } else {
            vec![quoted_after(line, "verdict = ").expect("a verdict")]
        };
        let stdout = quoted_after(line, "stdout = ").map(|s| s.replace("\\n", "\n"));
        return Ruled { verdicts, stdout };
    }
    panic!("{name}/expected.toml has no [expected] lupin cell");
}

fn quoted_after(line: &str, key: &str) -> Option<String> {
    let rest = line.split(key).nth(1)?;
    let rest = rest.strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_owned())
}

/// A fresh copy of the witness's program in its own scratch directory.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("rulings_is58")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    std::fs::copy(witness_dir(name).join("main.lu"), dir.join("main.lu")).expect("copied");
    dir
}

/// `lupin conform-run main.lu --json` from `dir`: the verdict and the inline
/// stdout of the record.
fn conform_run(dir: &Path) -> (String, Option<String>, serde_json::Value) {
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
    let record: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON record");
    let verdict = record["verdict"].as_str().expect("a verdict").to_owned();
    let stdout = record["stdout_inline"].as_str().map(str::to_owned);
    (verdict, stdout, record)
}

fn check(name: &str, dir: &Path) {
    let ruled = ruled(name);
    let (verdict, stdout, record) = conform_run(dir);
    assert!(
        ruled.verdicts.contains(&verdict),
        "{name}: lupin answered `{verdict}`, ruled {:?} — record {record}",
        ruled.verdicts
    );
    if verdict.starts_with("exit(") {
        assert_eq!(
            stdout.as_deref(),
            ruled.stdout.as_deref(),
            "{name}: the ruled stdout — record {record}"
        );
    } else if verdict == "unsupported" {
        // A decline is BY NAME: the record says why, and nothing ran to a
        // row (`[os.fs.path.domain]`: "never by a row").
        assert!(
            record["x-unsupported"]
                .as_str()
                .is_some_and(|r| !r.is_empty()),
            "{name}: an unsupported record names its reason — {record}"
        );
    }
}

fn run(name: &str) {
    let dir = scratch(name);
    check(name, &dir);
}

// -- ruled answers 0.1.41 did not give --------------------------------

/// `m[k] else …` moves the value; `k` changes; `m[k] = take v` stores at
/// `m["b"]`, so `m["a"]` stays read out (#144).
#[test]
fn elem_key_reassigned_no_revive() {
    run("elem_key_reassigned_no_revive");
}

/// The whole `mut` parameter moved out and never stored back (#146).
#[test]
fn mut_param_moveout_whole() {
    run("mut_param_moveout_whole");
}

/// Stored back only under `if c`, called with `c = false` (#146).
#[test]
fn mut_param_moveout_one_path() {
    run("mut_param_moveout_one_path");
}

/// A map value of the parameter read out and never stored back (#144 and
/// #146 together: the read-out moves, the writeback carries it).
#[test]
fn mut_param_moveout_map() {
    run("mut_param_moveout_map");
}

/// Two- and three-level stores: every operand once (#145).
#[test]
fn ctl_store_order_nested_index() {
    run("ctl_store_order_nested_index");
}

// -- ruled as 0.1.41 answers: the fixes stop here ---------------------

/// A field of the parameter moved out — the mark rode in the value already.
#[test]
fn mut_param_moveout_field() {
    run("mut_param_moveout_field");
}

/// An element of the parameter moved out — #141's trap, since 0.1.41.
#[test]
fn mut_param_moveout_elem() {
    run("mut_param_moveout_elem");
}

/// The whole parameter stored back before the return.
#[test]
fn mut_param_restore_whole() {
    run("mut_param_restore_whole");
}

/// A field stored back.
#[test]
fn mut_param_restore_field() {
    run("mut_param_restore_field");
}

/// An element stored back through the same literal.
#[test]
fn mut_param_restore_elem() {
    run("mut_param_restore_elem");
}

/// A map value read out and stored back through the same key.
#[test]
fn mut_param_restore_map() {
    run("mut_param_restore_map");
}

/// R3 through a `str` key, as a parameter and as a local: read out, stored
/// back, read again.
#[test]
fn elem_str_key_revive() {
    run("elem_str_key_revive");
}

/// R3 through `char` and `bool` keys.
#[test]
fn elem_char_bool_key_revive() {
    run("elem_char_bool_key_revive");
}

/// Two literal keys, each read out once.
#[test]
fn elem_key_move_map() {
    run("elem_key_move_map");
}

/// `move xs[0]`, then `xs.len` — the header is no element, and no whole
/// read (#143).
#[test]
fn elem_len_after_move() {
    run("elem_len_after_move");
}

/// `move xs[0]`, read `xs[1].len` — a sibling element, not the whole (#143).
#[test]
fn elem_const_move_heap() {
    run("elem_const_move_heap");
}

/// `move t.0`, read `t.1` — a sibling position, not the whole (#143).
#[test]
fn elem_tuple_pos_move() {
    run("elem_tuple_pos_move");
}

/// A nested store under a field, a `mut` parameter and a growing value:
/// the order s183 pinned, unchanged by #145's fix.
#[test]
fn ctl_store_order_nested() {
    run("ctl_store_order_nested");
}

/// `m[key()] = val()` and `m[key()] = grow(mut m)`: key first, once.
#[test]
fn ctl_store_order_map() {
    run("ctl_store_order_map");
}

#[test]
fn every_row_has_a_runner_here() {
    // Nineteen rows, nineteen tests: a row added to `tests/rulings_is58/`
    // without a test above is a ruling nobody runs.
    let mut names: Vec<String> = std::fs::read_dir(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("rulings_is58"),
    )
    .expect("tests/rulings_is58/")
    .map(|e| {
        e.expect("an entry")
            .file_name()
            .to_string_lossy()
            .into_owned()
    })
    .collect();
    names.sort();
    let source = include_str!("rulings_is58.rs");
    for name in &names {
        assert!(
            source.contains(&format!("fn {name}()")),
            "tests/rulings_is58/{name} has no test in this file"
        );
    }
    assert_eq!(names.len(), 19, "{names:?}");
}

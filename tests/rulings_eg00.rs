//! is56 — eg00's element witnesses, run against their RULED lupin verdict.
//!
//! wolf-lang `38d5eeab` (eg00, wolffe-lang/wolf-lang#461) landed
//! `[mem.model.place.elem]` and parked thirteen witnesses in the planning
//! repo (`wolf/sprints/compiler/90-element-granularity/witnesses/`), each with
//! an `expected.toml` whose `[expected]` table is the ruled verdict per
//! machine and whose `[trunk]` table is what each machine answered the day
//! the clause landed. They are copied here verbatim, one directory each under
//! `tests/rulings_eg00/` (s182's `tests/rulings/` counts its own ten), with one
//! more: the corpus row
//! `memory/elem_move_same_const_read.lu`, which has no `expected.toml`
//! upstream and carries one here written from eg00's gate.
//!
//! Five of the fourteen rule `trap(use-after-move)` for lupin — the moved
//! element read back (wolffe-lang/wolf-interp#141) and the four
//! `elem_{const,dyn}_store_no_revive_{int,heap}` rows (item 3: a store to
//! `xs[1]` revives nothing about `xs[0]`; wolffe-lang/wolf-lang#460 is the
//! compiler's half). The other nine rule the answer lupin already gives, and
//! stand here as the pins that the fix does not reach past the moved slot.
//!
//! The runner is s182's (`tests/rulings_s182.rs`): the built binary's
//! `conform-run main.lu --json`, from a scratch copy of the witness's own
//! directory, the oracle read from the file's `[expected]` lupin cell.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_eg00")
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
        .join("rulings_eg00")
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

// -- ruled trap(use-after-move): the moved element stays unreadable ---

/// The witness: `move xs[0]`, then `xs[0].len`.
#[test]
fn elem_move_same_const_read() {
    run("elem_move_same_const_read");
}

/// `move xs[0]; xs[1] = 9`, then `xs[0]` — item 3, `Copy` elements.
#[test]
fn elem_const_store_no_revive_int() {
    run("elem_const_store_no_revive_int");
}

/// wolf-lang#460's witness: `move xs[0]; xs[1] = [5]`, then `xs[0].len`.
#[test]
fn elem_const_store_no_revive_heap() {
    run("elem_const_store_no_revive_heap");
}

/// The run-time-index store twin, `Copy` elements.
#[test]
fn elem_dyn_store_no_revive_int() {
    run("elem_dyn_store_no_revive_int");
}

/// The run-time-index store twin, `List` elements.
#[test]
fn elem_dyn_store_no_revive_heap() {
    run("elem_dyn_store_no_revive_heap");
}

// -- ruled as lupin answers today: the fix stops at the moved slot ----

/// wolf-lang#446's row: `move xs[0]`, read `xs[1]`.
#[test]
fn elem_move_one_place() {
    run("elem_move_one_place");
}

/// `move xs[0]` on `List[List[int]]`, read `xs[1]`.
#[test]
fn elem_const_move_heap() {
    run("elem_const_move_heap");
}

/// `move xs[0].tag`, read `xs[1].tag`.
#[test]
fn elem_const_field_of_elem() {
    run("elem_const_field_of_elem");
}

/// A map value moved at `m["a"]`, read at `m["b"]`.
#[test]
fn elem_key_move_map() {
    run("elem_key_move_map");
}

/// `move xs[0]`, read `xs.len` — a header, not an element.
#[test]
fn elem_len_after_move() {
    run("elem_len_after_move");
}

/// `f(mut xs[0], mut xs[1])`.
#[test]
fn elem_const_mut_pair() {
    run("elem_const_mut_pair");
}

/// `f(mut g[0][1], mut g[1][0])`.
#[test]
fn elem_const_nested_mut() {
    run("elem_const_nested_mut");
}

/// `f(mut xs[i], mut xs[i + 1])`.
#[test]
fn elem_offset_mut_pair() {
    run("elem_offset_mut_pair");
}

/// `for i in 1..3 { f(mut xs[0], mut xs[i]) }`.
#[test]
fn elem_loop_induction_mut() {
    run("elem_loop_induction_mut");
}

#[test]
fn every_parked_witness_has_a_runner_here() {
    // Fourteen witnesses, fourteen tests: a witness added to
    // `tests/rulings_eg00/` without a test above is a ruling nobody runs.
    let mut names: Vec<String> = std::fs::read_dir(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("rulings_eg00"),
    )
    .expect("tests/rulings_eg00/")
    .map(|e| {
        e.expect("an entry")
            .file_name()
            .to_string_lossy()
            .into_owned()
    })
    .collect();
    names.sort();
    let source = include_str!("rulings_eg00.rs");
    for name in &names {
        assert!(
            source.contains(&format!("fn {name}()")),
            "tests/rulings_eg00/{name} has no test in this file"
        );
    }
    assert_eq!(names.len(), 14, "{names:?}");
}

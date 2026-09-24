//! is55 — s182's ten parked witnesses, run against their RULED lupin verdict.
//!
//! wolf-lang `0468dead` landed two rulings as clauses and parked ten
//! witnesses in the planning repo (`wolf/sprints/compiler/88-the-rulings-prose/
//! witnesses/`), each with an `expected.toml` whose `[expected]` table is the
//! ruled verdict per machine and whose `[trunk]` table is what each machine
//! answered the day the clause landed. They are copied here verbatim, one
//! directory each under `tests/rulings/`, and this file is their runner:
//!
//! - `[mem.region.edge.elem]` + `[gram.expr.assign]` (wolf-lang#438): a plain
//!   index store COPIES a non-`Copy` value, `xs[i] = take v` MOVES it, and
//!   `take` is admitted in exactly that position — `index_store_*`.
//! - `[os.fs.path.domain]` (wolf-lang#386): a path is the host's; a
//!   confining implementation declines BY NAME, never by a row, and the
//!   boundary it declines at is RESOLVED, not lexical — `fs_path_*`.
//!
//! Every witness runs the way s182's `run.sh` runs it: the built binary's
//! `conform-run main.lu --json`, from the witness's own directory, because
//! the fs witnesses name paths relative to it. The directory is a scratch
//! copy under `CARGO_TARGET_TMPDIR`, so a run never writes into the source
//! tree, and the two symlink witnesses' `setup.sh` is done here in Rust —
//! the same links, with the OUTSIDE target a scratch directory beside the
//! copy rather than `/tmp/s182_out`, so parallel runs and other users of the
//! host cannot collide. Symlinks need a privilege windows does not grant a
//! test by default, so those two are unix-only; every other witness runs on
//! all three hosts.
//!
//! The oracle is the `lupin` cell of each file's `[expected]` table, read
//! from the file rather than restated here, so a re-ruling edits one place.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings")
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
        .join("rulings")
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
    assert_eq!(output.status.code(), Some(0), "the tool succeeded: {output:?}");
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
            record["x-unsupported"].as_str().is_some_and(|r| !r.is_empty()),
            "{name}: an unsupported record names its reason — {record}"
        );
    }
}

fn run(name: &str) {
    let dir = scratch(name);
    check(name, &dir);
}

// -- wolf-lang#438: the index store follows `push` -----------------------

#[test]
fn index_store_copies_list() {
    run("index_store_copies_list");
}

#[test]
fn index_store_copies_map() {
    run("index_store_copies_map");
}

#[test]
fn index_store_take_list() {
    run("index_store_take_list");
}

#[test]
fn index_store_take_map() {
    run("index_store_take_map");
}

#[test]
fn index_store_read_param() {
    run("index_store_read_param");
}

// -- wolf-lang#386: the domain is the host's; a decline is resolved ------

#[test]
fn fs_path_absolute() {
    run("fs_path_absolute");
}

#[test]
fn fs_path_climbs_out() {
    // Run from a directory of its own, so a machine that serves the climb
    // writes one level up inside the scratch tree and removes it again.
    let dir = scratch("fs_path_climbs_out");
    check("fs_path_climbs_out", &dir);
    assert!(
        !dir.parent()
            .expect("a parent")
            .join("s182_probe_up.txt")
            .exists(),
        "a served climb removes its own file"
    );
}

#[test]
fn fs_path_inside_after_dotdot() {
    run("fs_path_inside_after_dotdot");
}

/// `setup.sh`: `target/s182_in_link -> s182_in`, a link that stays inside.
#[cfg(unix)]
#[test]
fn fs_path_symlink_in() {
    let dir = scratch("fs_path_symlink_in");
    std::fs::create_dir_all(dir.join("target").join("s182_in")).expect("the link's target");
    std::os::unix::fs::symlink("s182_in", dir.join("target").join("s182_in_link"))
        .expect("the inside link");
    check("fs_path_symlink_in", &dir);
    assert!(
        !dir.join("target")
            .join("s182_in")
            .join("s182_probe_sym.txt")
            .exists(),
        "the program removed what it wrote through the link"
    );
}

/// `setup.sh`: `target/s182_link -> <a directory outside the tree>`. The
/// target is a scratch sibling of the witness's directory, so the link
/// resolves outside the cwd exactly as `/tmp/s182_out` does in s182's setup.
#[cfg(unix)]
#[test]
fn fs_path_symlink_out() {
    let dir = scratch("fs_path_symlink_out");
    let outside = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rulings-outside");
    std::fs::create_dir_all(&outside).expect("the outside directory");
    std::fs::create_dir_all(dir.join("target")).expect("target/");
    std::os::unix::fs::symlink(&outside, dir.join("target").join("s182_link"))
        .expect("the outside link");
    check("fs_path_symlink_out", &dir);
    assert!(
        !outside.join("s182_probe_sym.txt").exists(),
        "nothing is left outside the tree, served or declined"
    );
}

#[test]
fn every_parked_witness_has_a_runner_here() {
    // Ten witnesses, ten tests: a witness added to `tests/rulings/` without
    // a test above is a ruling nobody runs.
    let mut names: Vec<String> = std::fs::read_dir(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("rulings"),
    )
    .expect("tests/rulings/")
    .map(|e| e.expect("an entry").file_name().to_string_lossy().into_owned())
    .collect();
    names.sort();
    let source = include_str!("rulings_s182.rs");
    for name in &names {
        assert!(
            source.contains(&format!("fn {name}()")),
            "tests/rulings/{name} has no test in this file"
        );
    }
    assert_eq!(names.len(), 10, "{names:?}");
}

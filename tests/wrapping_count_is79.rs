//! is79 (wolf-interp#273; wolf-lang s252, #822's fifth finding, yew's
//! `wrapping-infer` witness; `[type.int.shift]` (5), `[type.numlit.value]`)
//! — a shift's count has its operand's type.
//!
//! `x << n` with `x: u64` and `n: int` is E0401 (`[type.int.shift]` (5)),
//! and a `wrapping[u64]` operand is no exception. lupin 0.1.55 refused it
//! only where both types were a parameter's or a cast's (is76); an
//! annotated local's `int` count ran, so it read an unannotated counter as
//! an `int` too: `var i = 15` whose first deciding use is the count of a
//! `wrapping[u64]` shift is `wrapping[u64]` on every compiling machine
//! (`[type.numlit.value]`: a later use in the same body types the binding),
//! `i >= 0` never fails there, and s252's countdown row runs forty turns —
//! sixteen on lupin 0.1.55.
//!
//! `tests/fixtures/small_gaps_is79/numlit_wrapping_countdown.lu` is s252's
//! row (branch `s252` at `50e8d0f8`) byte for byte (sha256 `3402259d…`):
//! `turns=40 acc=3` then `turns=16 acc=13` on checked and native at 0.2.34
//! and at s252 (is79 `runs/m3/`, `runs/m4/`). The probes below were measured
//! the same way; every E0401 is the compiler's code at the count's span.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

/// One of s252's corpus rows, byte for byte.
fn row(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/small_gaps_is79")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn observe(entry: &Path, dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("lupin observes")
}

fn record(out: &Output) -> serde_json::Value {
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .filter(|r| r.get("verdict").is_some())
        .unwrap_or_else(|| panic!("a record: {out:?}"))
}

/// What a row's `//! check:` header declares: `exit(0)` and its stdout for
/// a run row, `fail(CODE)` for a refusal.
fn declared(entry: &Path) -> (String, Option<String>) {
    let text = std::fs::read_to_string(entry).expect("the row reads");
    let line = text
        .lines()
        .find(|l| l.starts_with("//! check: "))
        .unwrap_or_else(|| panic!("{} declares a check", entry.display()));
    if let Some(start) = line.find("stdout=\"") {
        let start = start + "stdout=\"".len();
        let end = line.rfind("\")").expect("the directive closes");
        return (
            "exit(0)".to_owned(),
            Some(line[start..end].replace("\\n", "\n")),
        );
    }
    let code = line
        .trim_start_matches("//! check: ")
        .trim()
        .to_owned();
    (code, None)
}

/// The row `name`, observed in a scratch directory of its own.
fn observe_row(name: &str) -> serde_json::Value {
    let dir = scratch(&format!("{}_{}", module_path!(), name.replace('.', "_")));
    record(&observe(&row(name), &dir))
}

/// A program of this test's own, written to a scratch directory of its
/// own (a directory is a module) and observed there.
fn observe_text(slug: &str, program: &str) -> serde_json::Value {
    let dir = scratch(&format!("{}_{slug}", module_path!()));
    let entry = dir.join("main.lu");
    std::fs::write(&entry, program).expect("the program is written");
    record(&observe(&entry, &dir))
}

fn verdict(rec: &serde_json::Value) -> String {
    rec["verdict"].as_str().unwrap_or("").to_owned()
}

fn stdout(rec: &serde_json::Value) -> String {
    rec["stdout_inline"].as_str().unwrap_or("").to_owned()
}

/// The row runs as its header says the compiling machines do (s252's
/// `small_gaps_lanes.rs` asserts the same of checked, native and release).
fn runs_as_declared(name: &str) {
    let (want, out) = declared(&row(name));
    let rec = observe_row(name);
    assert_eq!(verdict(&rec), want, "{name}: {rec}");
    if let Some(out) = out {
        assert_eq!(stdout(&rec), out, "{name}: {rec}");
    }
}

/// The record is `fail(E0401)` at the count, the compiler's span.
fn refused_at_the_count(slug: &str, program: &str) {
    let rec = observe_text(slug, program);
    assert_eq!(verdict(&rec), "fail(E0401)", "{slug}: {rec}");
    let at = program.rfind(">> n").or_else(|| program.rfind("<< n")).expect("a count") + 3;
    assert_eq!(
        rec["diagnostics"][0]["span"],
        serde_json::json!([at, at + 1]),
        "{slug}: the count's span: {rec}"
    );
}

/// s252's row: the inferred counter is `wrapping[u64]` and wraps; the
/// annotated one, cast at its use, ends after sixteen.
#[test]
fn the_countdown_row_runs_forty_turns_then_sixteen() {
    runs_as_declared("numlit_wrapping_countdown.lu");
}

/// The same rule at its smallest: the counter's first deciding use is the
/// count of a `wrapping[u64]` shift, so `i -= 2` from 1 wraps and `i >= 0`
/// holds (`2 true` on the compilers; `2 false` on 0.1.55).
#[test]
fn an_unannotated_counter_takes_the_wrapping_type_of_the_shift_it_counts() {
    let rec = observe_text(
        "infer_small",
        "fn main() {
    let h: wrapping[u64] = 5
    var i = 1
    let x = h >> i
    i -= 2
    print(\"{x as int} {i >= 0}\")
}
",
    );
    assert_eq!(
        (verdict(&rec), stdout(&rec)),
        ("exit(0)".to_owned(), "2 true\n".to_owned()),
        "{rec}"
    );
}

/// An annotated `int` count on a `wrapping[u64]` and on a `u64`, and an
/// `int` parameter (is76's case, kept): E0401 at the count.
#[test]
fn an_int_count_on_a_shift_of_another_type_is_e0401() {
    refused_at_the_count(
        "wrap_local",
        "fn main() {
    let h: wrapping[u64] = 5
    let n: int = 1
    print(\"{(h >> n) as int}\")
}
",
    );
    refused_at_the_count(
        "u64_local",
        "fn main() {
    let h: u64 = 5
    let n: int = 1
    print(\"{h >> n}\")
}
",
    );
    refused_at_the_count(
        "wrap_param",
        "fn sh(h: wrapping[u64], n: int) -> wrapping[u64] {
    h << n
}

fn main() {
    let h: wrapping[u64] = 5
    print(\"{sh(h, 2) as int}\")
}
",
    );
}

/// The counts that agree: a literal adopts the operand's type, a count of
/// the operand's own type, and a count at the width (masked, `[type.int.shift]` (6)).
#[test]
fn a_literal_count_and_a_count_of_the_operands_type_run() {
    let rec = observe_text(
        "agree",
        "fn main() {
    let h: wrapping[u64] = 5
    let k: wrapping[u64] = 2
    print(\"{(h << 1) as int} {(h << k) as int} {(h >> 64) as int}\")
}
",
    );
    assert_eq!(
        (verdict(&rec), stdout(&rec)),
        ("exit(0)".to_owned(), "10 20 5\n".to_owned()),
        "{rec}"
    );
}

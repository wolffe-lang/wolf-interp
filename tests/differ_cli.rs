//! The differential runner through the process contract.
//!
//! Two lanes: the **self-differential** — this binary against itself, which
//! must be divergence-free by construction (two identical implementations
//! cannot disagree; a divergence here is a bug in the *comparison*, and this
//! is the test that keeps the comparison honest before it is pointed at a
//! real counterparty) — and the **SKIP lane**: without a counterparty the
//! runner detects the absence, says so in `notice:` lines, and exits green
//! unless `--require-counterparty` promises one (the ls00 pattern).
//!
//! The fault-injection drill for the comparison itself lives in
//! `src/differ.rs`'s unit tests, where every mutated-record shape (wrong
//! verdict, wrong span, UB against defined, malformed record, timeout) is
//! driven through `compare_deep`/`compare_invocations` directly — a
//! deliberately bugged counterparty *process* would have to be a second
//! binary, which is exactly what CI cannot have.

use std::path::Path;
use std::process::{Command, Output};

fn wolf_interp(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wolf-interp"))
        .args(args)
        .output()
        .expect("the binary runs")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn grammar_corpus() -> String {
    format!("{}/corpus/grammar", wolf_interp::upstream_root())
}

#[test]
fn the_self_differential_is_empty() {
    // The binary against itself over the grammar tier — pass and fail files
    // both. Same implementation on both ends: any divergence indicts the
    // comparison logic, not an implementation.
    let corpus = grammar_corpus();
    let output = wolf_interp(&[
        "diff-run",
        "--corpus",
        &corpus,
        "--compiler",
        env!("CARGO_BIN_EXE_wolf-interp"),
    ]);
    let stdout = stdout_of(&output);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("divergences: 0"), "{stdout}");
}

#[test]
fn the_jsonl_report_of_a_self_differential_is_an_empty_file() {
    // [proto.harness.differ]: "empty report = green".
    let corpus = grammar_corpus();
    let report = std::env::temp_dir().join("wolf-interp-selfdiff-report.jsonl");
    let output = wolf_interp(&[
        "diff-run",
        "--corpus",
        &corpus,
        "--compiler",
        env!("CARGO_BIN_EXE_wolf-interp"),
        "--report",
        &report.to_string_lossy(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    let text = std::fs::read_to_string(&report).expect("the report was written");
    assert!(text.is_empty(), "an agreeing pair wrote lines: {text}");
    let _ = std::fs::remove_file(&report);
}

#[test]
fn a_missing_counterparty_skips_loudly_and_green() {
    let corpus = grammar_corpus();
    let output = wolf_interp(&[
        "diff-run",
        "--corpus",
        &corpus,
        "--compiler",
        "does/not/exist/wolf",
    ]);
    assert_eq!(output.status.code(), Some(0), "a skip is green by default");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("notice: differential lane SKIPPED"),
        "{stderr}"
    );
    assert!(stderr.contains("cargo build -p wolf_driver"), "{stderr}");
    assert!(stderr.contains("--require-counterparty"), "{stderr}");
}

#[test]
fn require_counterparty_turns_the_skip_into_a_failure() {
    let corpus = grammar_corpus();
    let output = wolf_interp(&[
        "diff-run",
        "--corpus",
        &corpus,
        "--compiler",
        "does/not/exist/wolf",
        "--require-counterparty",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--require-counterparty"), "{stderr}");
}

#[test]
fn the_fuzz_self_oracle_runs_green_without_a_counterparty() {
    // A bounded, seeded slice of the campaign: no counterparty, so defined
    // programs must exit 0 and no unsafe-free program may report UB. The
    // notice lines say the lane degraded, and to what.
    let out = std::env::temp_dir().join("wolf-interp-fuzz-smoke");
    let output = wolf_interp(&[
        "fuzz",
        "--count",
        "25",
        "--seed",
        "5381",
        "--compiler",
        "does/not/exist/wolf",
        "--out",
        &out.to_string_lossy(),
    ]);
    let stdout = stdout_of(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stderr.contains("self-oracle"), "{stderr}");
    assert!(stdout.contains("0 finding(s)"), "{stdout}");
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn fuzz_replays_identically_from_a_seed() {
    // The replay contract: same seed, same campaign, byte-identical summary.
    let out_a = std::env::temp_dir().join("wolf-interp-fuzz-replay-a");
    let out_b = std::env::temp_dir().join("wolf-interp-fuzz-replay-b");
    let run = |out: &Path| {
        stdout_of(&wolf_interp(&[
            "fuzz",
            "--count",
            "10",
            "--seed",
            "424242",
            "--compiler",
            "does/not/exist/wolf",
            "--out",
            &out.to_string_lossy(),
        ]))
    };
    let a = run(&out_a);
    let b = run(&out_b);
    assert_eq!(a, b, "a seeded campaign must replay byte-identically");
    let _ = std::fs::remove_dir_all(&out_a);
    let _ = std::fs::remove_dir_all(&out_b);
}

//! s220 — lupin's half of wolf-lang#551, #538 and #553: the eighteen
//! `int_truth_*` rows of wolf-lang's corpus (`corpus/typecheck/` and
//! `corpus/faults/`, copied verbatim into `tests/int_truth_s220/`), each
//! held to its own `check:` header.
//!
//! lupin answered seventeen of them before s220, measured at 0.1.49
//! (kasumi). The eighteenth, `int_truth_wrap_mul_wide`, is the defect
//! s220's four-machine sweep found here: a `wrapping[u64]` product whose
//! exact value passes 2^127 overflowed lupin's `i128` multiply and
//! trapped `overflow`, where a wrapping product keeps its low bits on
//! every compiler machine (`(2^64 - 2)^2` is 4).

use std::path::{Path, PathBuf};
use std::process::Command;

fn rows() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("int_truth_s220")
}

/// `(verdict, stdout)` from a row's `check:` header:
/// `run(exit=0, stdout="…")` (JSON escapes) or `run(exit=trap(kind))`.
fn want(text: &str, name: &str) -> (String, String) {
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("//! check: "))
        .unwrap_or_else(|| panic!("{name} has no check: header"));
    if let Some(rest) = line.strip_prefix("run(exit=trap(") {
        let kind = rest.split(')').next().expect("a trap kind");
        return (format!("trap({kind})"), String::new());
    }
    let lit = line
        .strip_prefix("run(exit=0, stdout=")
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{name}: unread check header {line:?}"));
    let out: String = serde_json::from_str(lit).expect("a JSON string literal");
    ("exit(0)".to_owned(), out)
}

#[test]
fn every_int_truth_row_answers_its_header() {
    let mut names: Vec<String> = std::fs::read_dir(rows())
        .expect("the rows directory")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".lu"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 18, "the eighteen rows: {names:?}");
    let mut bad = Vec::new();
    for name in &names {
        let text = std::fs::read_to_string(rows().join(name)).expect("row reads");
        let (verdict, stdout) = want(&text, name);
        let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
            .args(["conform-run", name, "--json"])
            .current_dir(rows())
            .output()
            .expect("lupin runs");
        let record: serde_json::Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|e| panic!("{name}: one JSON record ({e}): {output:?}"));
        let got = (
            record["verdict"].as_str().unwrap_or("").to_owned(),
            record["stdout_inline"].as_str().unwrap_or("").to_owned(),
        );
        if got != (verdict.clone(), stdout.clone()) {
            bad.push(format!("{name}: want {verdict} {stdout:?}; got {got:?}"));
        }
    }
    assert!(bad.is_empty(), "{} of {} rows:\n{}", bad.len(), names.len(), bad.join("\n"));
}

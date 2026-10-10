//! s224 — lupin's half of wolf-lang#124 (ruling B22, wolf
//! `[os.json.dup]`): a repeated json object key is last-wins, one
//! member per key at its first position. The seven `dup_*` rows of
//! wolf-lang's `corpus/json/`, copied verbatim into
//! `tests/json_dup_s224/`, each held to its own `check:` header.
//!
//! Measured at 0.1.49 (`f516a5f`, hasu): lupin answered all seven
//! FIRST-wins and counted every repeat, exactly as the three compiler
//! machines did before wolf-lang s224 — four machines agreeing with
//! each other and parting from the ruling.

use std::path::{Path, PathBuf};
use std::process::Command;

fn rows() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("json_dup_s224")
}

/// The stdout a row's `check: run(exit=0, stdout="…")` header names.
fn want(text: &str, name: &str) -> String {
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("//! check: "))
        .unwrap_or_else(|| panic!("{name} has no check: header"));
    let lit = line
        .strip_prefix("run(exit=0, stdout=")
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{name}: unread check header {line:?}"));
    serde_json::from_str(lit).expect("a JSON string literal")
}

#[test]
fn every_json_dup_row_answers_its_header() {
    let mut names: Vec<String> = std::fs::read_dir(rows())
        .expect("the rows directory")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".lu"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 7, "the seven rows: {names:?}");
    let mut bad = Vec::new();
    for name in &names {
        let text = std::fs::read_to_string(rows().join(name)).expect("row reads");
        let stdout = want(&text, name);
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
        if got != ("exit(0)".to_owned(), stdout.clone()) {
            bad.push(format!("{name}: want exit(0) {stdout:?}; got {got:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} rows:\n{}",
        bad.len(),
        names.len(),
        bad.join("\n")
    );
}

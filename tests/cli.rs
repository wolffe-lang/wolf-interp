//! CLI-level contract tests: exit codes and stdout shape.
//!
//! `[proto.invoke.exit]` is a *process* contract — it cannot be tested from
//! inside the library, so it is tested from outside the binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use wolf_interp::schema;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn wolf_interp(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wolf-interp"))
        .current_dir(manifest_dir())
        .args(args)
        .output()
        .expect("wolf-interp runs")
}

fn stdout_of(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("stdout is utf-8")
}

#[test]
fn conform_run_emits_a_valid_record_and_exits_zero() {
    let hello = format!("{}/corpus/hello.lu", wolf_interp::upstream_root());
    let output = wolf_interp(&["conform-run", &hello, "--json"]);
    assert_eq!(output.status.code(), Some(0));

    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&output).trim()).expect("stdout is one JSON object");
    assert_eq!(schema::validate(&value), Ok(()));
    assert_eq!(value["protocol"], 1);
    assert_eq!(value["impl"], "wolf-interp");
    assert_eq!(value["phase_reached"], "none");
    assert_eq!(value["verdict"], "unsupported");
    assert_eq!(value["seeded"], false);
    assert_eq!(value["file"], serde_json::json!(hello));
}

#[test]
fn conform_run_stays_honest_under_phase_and_seed() {
    let output = wolf_interp(&[
        "conform-run",
        &format!(
            "{}/corpus/conc/select_seeded.lu",
            wolf_interp::upstream_root()
        ),
        "--phase=run",
        "--seed=7",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(0));

    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&output).trim()).expect("stdout is one JSON object");
    // A deeper `--phase` never buys a deeper claim, and `--seed` never buys
    // `seeded: true` (`[proto.seed.flag]`).
    assert_eq!(value["phase_reached"], "none");
    assert_eq!(value["seeded"], false);

    // The unhonored knobs are announced on stderr, never on stdout.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--phase=run"), "{stderr}");
    assert!(stderr.contains("--seed=7"), "{stderr}");
}

#[test]
fn a_missing_program_is_a_tool_error_with_no_record() {
    let missing = format!("{}/corpus/not-a-file.lu", wolf_interp::upstream_root());
    let output = wolf_interp(&["conform-run", &missing, "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stdout_of(&output).is_empty(),
        "a tool error emits no record"
    );
}

#[test]
fn a_bad_phase_is_a_tool_error() {
    let output = wolf_interp(&[
        "conform-run",
        &format!("{}/corpus/hello.lu", wolf_interp::upstream_root()),
        "--phase=codegen",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout_of(&output).is_empty());
}

#[test]
fn the_corpus_walk_is_green_over_the_pinned_corpus() {
    let output = wolf_interp(&["corpus"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout_of(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = stdout_of(&output);
    assert!(stdout.contains("69 file(s)"), "{stdout}");
    assert!(stdout.contains("0 failure(s)"), "{stdout}");
}

#[test]
fn the_corpus_walk_has_a_machine_mode() {
    let output = wolf_interp(&["corpus", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_str(stdout_of(&output)).expect("json");
    assert_eq!(value["total"], 69);
    assert_eq!(value["failures"], 0);
    assert_eq!(value["green"], true);
    assert_eq!(value["files"][0]["interpreter_status"], "unsupported");
}

#[test]
fn a_missing_corpus_root_is_a_tool_error_that_names_the_submodule() {
    let output = wolf_interp(&["corpus", "--root", "does/not/exist"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("git submodule update --init upstream"),
        "{stderr}"
    );
}

#[test]
fn protocol_validate_accepts_and_rejects_the_fixtures_as_named() {
    let dir = Path::new(wolf_interp::upstream_root()).join("corpus/protocol");
    let dir = dir.as_path();
    for accepted in ["valid.json", "with-extensions.json"] {
        let path = dir.join(accepted);
        let output = wolf_interp(&["protocol", "validate", &path.to_string_lossy()]);
        assert_eq!(output.status.code(), Some(0), "{accepted}");
        assert!(stdout_of(&output).starts_with("accept"), "{accepted}");
    }
    for rejected in ["wrong-version.json", "missing-field.json"] {
        let path = dir.join(rejected);
        let output = wolf_interp(&["protocol", "validate", &path.to_string_lossy()]);
        assert_eq!(output.status.code(), Some(1), "{rejected}");
        assert!(stdout_of(&output).starts_with("reject"), "{rejected}");
    }
}

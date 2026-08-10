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
    // is02: `hello.lu` runs. `[proto.record.fields]` then requires the digest
    // and the inline text, because the program wrote output.
    assert_eq!(value["phase_reached"], "run");
    assert_eq!(value["verdict"], "exit(0)");
    assert_eq!(value["seeded"], false);
    assert_eq!(value["file"], serde_json::json!(hello));
    assert_eq!(value["stdout_inline"], "hello, wolf\n");
    assert_eq!(
        value["stdout_sha256"],
        serde_json::json!(wolf_interp::sha256::hex(b"hello, wolf\n"))
    );
}

#[test]
fn conform_run_honors_seed_and_declares_it() {
    // is06: `--seed=N` requests the spec/03 §5 deterministic schedule and the
    // record declares `"seeded": true` (`[proto.seed.flag]`); the program runs
    // under the sim scheduler now, so the concurrency file reaches `run`.
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
    assert_eq!(value["phase_reached"], "run");
    assert_eq!(value["verdict"], "exit(0)");
    assert_eq!(value["seeded"], true);

    // The same seed replays the identical record ([conc.det.seed]).
    let again = wolf_interp(&[
        "conform-run",
        &format!(
            "{}/corpus/conc/select_seeded.lu",
            wolf_interp::upstream_root()
        ),
        "--phase=run",
        "--seed=7",
        "--json",
    ]);
    assert_eq!(stdout_of(&output), stdout_of(&again));

    // Unseeded, the record says so ([proto.seed.flag]).
    let unseeded = wolf_interp(&[
        "conform-run",
        &format!(
            "{}/corpus/conc/select_seeded.lu",
            wolf_interp::upstream_root()
        ),
        "--json",
    ]);
    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&unseeded).trim()).expect("one JSON object");
    assert_eq!(value["seeded"], false);
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
    assert!(stdout.contains("149 file(s)"), "{stdout}");
    assert!(stdout.contains("0 failure(s)"), "{stdout}");
}

#[test]
fn the_corpus_walk_has_a_machine_mode() {
    let output = wolf_interp(&["corpus", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_str(stdout_of(&output)).expect("json");
    assert_eq!(value["total"], 149);
    assert_eq!(value["failures"], 0);
    assert_eq!(value["green"], true);
    // The first entry in slash-path order is still `comptime.lu` (`.` precedes
    // `/`): it parses and resolves, and comptime evaluation is the compiler's
    // s16 engine, so it stops at the deepest *completed* rung.
    assert_eq!(value["files"][0]["file"], "comptime.lu");
    assert_eq!(
        value["files"][0]["interpreter_status"],
        "unsupported@resolve"
    );
    assert_eq!(value["files"][0]["judgement"]["class"], "out-of-scope");
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

#[test]
fn the_lex_and_parse_rungs_are_real_and_pass_the_canonical_program() {
    let wordcount = format!("{}/corpus/wordcount.lu", wolf_interp::upstream_root());
    for phase in ["lex", "parse"] {
        let output = wolf_interp(&[
            "conform-run",
            &wordcount,
            &format!("--phase={phase}"),
            "--json",
        ]);
        assert_eq!(output.status.code(), Some(0));
        let value: serde_json::Value =
            serde_json::from_str(stdout_of(&output).trim()).expect("one JSON object");
        assert_eq!(schema::validate(&value), Ok(()));
        assert_eq!(value["verdict"], "pass", "{phase}");
        assert_eq!(value["phase_reached"], phase);
    }
}

#[test]
fn a_rejected_program_is_a_record_and_the_tool_still_exits_zero() {
    // `[proto.invoke.exit]`: the *record* carries the program's outcome.
    let file = format!(
        "{}/corpus/grammar/newline_leading.lu",
        wolf_interp::upstream_root()
    );
    let output = wolf_interp(&["conform-run", &file, "--phase=parse", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&output).trim()).expect("one JSON object");
    assert_eq!(value["verdict"], "fail(E0001)");
    assert_eq!(value["phase_reached"], "parse");
    assert_eq!(value["diagnostics"][0]["code"], "E0001");
    assert_eq!(value["diagnostics"][0]["severity"], "error");
    assert!(
        value["diagnostics"][1].is_null(),
        "no recovery, no second diagnostic"
    );
}

#[test]
fn the_human_frontend_doors_exit_65_on_a_rejection() {
    let file = format!(
        "{}/corpus/grammar/when_reserved.lu",
        wolf_interp::upstream_root()
    );
    // Lexically fine, syntactically not: `when` is reserved.
    assert_eq!(wolf_interp(&["lex", &file]).status.code(), Some(0));
    let parsed = wolf_interp(&["parse", &file]);
    assert_eq!(parsed.status.code(), Some(65));
    let stderr = String::from_utf8_lossy(&parsed.stderr);
    assert!(stderr.contains("E0008"), "{stderr}");
    assert!(
        stderr.contains("gram."),
        "the clause anchor rides along: {stderr}"
    );
}

#[test]
fn the_dumps_are_ours_and_cite_clauses() {
    let hello = format!("{}/corpus/hello.lu", wolf_interp::upstream_root());
    let tokens = wolf_interp(&["lex", &hello, "--dump"]);
    assert_eq!(tokens.status.code(), Some(0));
    let tokens = stdout_of(&tokens);
    assert!(tokens.contains("str-open plain"), "{tokens}");
    assert!(tokens.contains("interp-open"), "{tokens}");
    assert!(tokens.contains("term nl"), "{tokens}");

    let trace = wolf_interp(&["parse", &hello, "--dump"]);
    assert_eq!(trace.status.code(), Some(0));
    let trace = stdout_of(&trace);
    assert!(trace.contains("[gram.item.unit] unit"), "{trace}");
    assert!(trace.contains("[gram.expr.block]"), "{trace}");
}

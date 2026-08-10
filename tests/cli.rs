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

fn lupin(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .current_dir(manifest_dir())
        .args(args)
        .output()
        .expect("lupin runs")
}

fn stdout_of(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("stdout is utf-8")
}

#[test]
fn conform_run_emits_a_valid_record_and_exits_zero() {
    let hello = format!("{}/corpus/hello.lu", wolf_interp::upstream_root());
    let output = lupin(&["conform-run", &hello, "--json"]);
    assert_eq!(output.status.code(), Some(0));

    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&output).trim()).expect("stdout is one JSON object");
    assert_eq!(schema::validate(&value), Ok(()));
    assert_eq!(value["protocol"], 1);
    assert_eq!(value["impl"], "lupin");
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
    let output = lupin(&[
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
    let again = lupin(&[
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
    let unseeded = lupin(&[
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
    let output = lupin(&["conform-run", &missing, "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stdout_of(&output).is_empty(),
        "a tool error emits no record"
    );
}

#[test]
fn a_bad_phase_is_a_tool_error() {
    let output = lupin(&[
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
    let output = lupin(&["corpus"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        stdout_of(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = stdout_of(&output);
    assert!(stdout.contains("175 file(s)"), "{stdout}");
    assert!(stdout.contains("0 failure(s)"), "{stdout}");
}

#[test]
fn the_corpus_walk_has_a_machine_mode() {
    let output = lupin(&["corpus", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_str(stdout_of(&output)).expect("json");
    assert_eq!(value["total"], 175);
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
fn conform_run_explore_reports_and_gates_on_stability() {
    // is07: `--explore=N` runs the DPOR explorer instead of one observation.
    // A schedule-independent litmus is green (exit 0) and the report carries
    // the counts; the JSON form is machine-readable.
    let file = format!(
        "{}/corpus/conc/cancel_sibling.lu",
        wolf_interp::upstream_root()
    );
    let output = lupin(&["conform-run", &file, "--explore=100"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout_of(&output));
    let text = stdout_of(&output);
    assert!(text.contains("explored 2 schedule(s)"), "{text}");
    assert!(text.contains("observably deterministic"), "{text}");

    let output = lupin(&["conform-run", &file, "--explore=100", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&output).trim()).expect("one JSON object");
    assert_eq!(value["schedules"], 2);
    assert_eq!(value["stable"], true);
    assert_eq!(value["green"], true);
    assert_eq!(value["frontier_open"], false);
    assert_eq!(value["mode"], "dpor");
}

#[test]
fn conform_run_replays_an_explicit_decision_stream() {
    // The explorer's counterexample spelling: `--schedule=ev:…` replays one
    // exact schedule and the record declares `seeded: true` — deterministic
    // replay is what the flag means ([proto.seed.equal]).
    let file = format!(
        "{}/corpus/conc/select_seeded.lu",
        wolf_interp::upstream_root()
    );
    let one = lupin(&["conform-run", &file, "--schedule=ev:0", "--json"]);
    assert_eq!(one.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&one).trim()).expect("one JSON object");
    assert_eq!(value["seeded"], true);
    assert_eq!(value["verdict"], "exit(0)");
    // Byte-identical on replay.
    let two = lupin(&["conform-run", &file, "--schedule=ev:0", "--json"]);
    assert_eq!(stdout_of(&one), stdout_of(&two));

    // A malformed stream is a tool error with no record.
    let bad = lupin(&["conform-run", &file, "--schedule=ev:1,x"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(stdout_of(&bad).is_empty());
}

#[test]
fn a_missing_corpus_root_is_a_tool_error_that_names_the_submodule() {
    let output = lupin(&["corpus", "--root", "does/not/exist"]);
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
        let output = lupin(&["protocol", "validate", &path.to_string_lossy()]);
        assert_eq!(output.status.code(), Some(0), "{accepted}");
        assert!(stdout_of(&output).starts_with("accept"), "{accepted}");
    }
    for rejected in ["wrong-version.json", "missing-field.json"] {
        let path = dir.join(rejected);
        let output = lupin(&["protocol", "validate", &path.to_string_lossy()]);
        assert_eq!(output.status.code(), Some(1), "{rejected}");
        assert!(stdout_of(&output).starts_with("reject"), "{rejected}");
    }
}

#[test]
fn the_lex_and_parse_rungs_are_real_and_pass_the_canonical_program() {
    let wordcount = format!("{}/corpus/wordcount.lu", wolf_interp::upstream_root());
    for phase in ["lex", "parse"] {
        let output = lupin(&[
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
    let output = lupin(&["conform-run", &file, "--phase=parse", "--json"]);
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
    assert_eq!(lupin(&["lex", &file]).status.code(), Some(0));
    let parsed = lupin(&["parse", &file]);
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
    let tokens = lupin(&["lex", &hello, "--dump"]);
    assert_eq!(tokens.status.code(), Some(0));
    let tokens = stdout_of(&tokens);
    assert!(tokens.contains("str-open plain"), "{tokens}");
    assert!(tokens.contains("interp-open"), "{tokens}");
    assert!(tokens.contains("term nl"), "{tokens}");

    let trace = lupin(&["parse", &hello, "--dump"]);
    assert_eq!(trace.status.code(), Some(0));
    let trace = stdout_of(&trace);
    assert!(trace.contains("[gram.item.unit] unit"), "{trace}");
    assert!(trace.contains("[gram.expr.block]"), "{trace}");
}

// ---------------------------------------------------------------------------
// is12 — the lupin front door: dispatch and the documented exit codes
// ---------------------------------------------------------------------------

/// As [`lupin`], with stdin piped and a working directory of the caller's
/// choice — the stdin door (`lupin -`) and the collision rule need both.
fn lupin_with_stdin(args: &[&str], dir: &Path, input: &[u8]) -> Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .current_dir(dir)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("lupin runs");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input)
        .expect("stdin written");
    child.wait_with_output().expect("lupin exits")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn the_front_door_runs_a_file_and_the_exit_code_is_the_programs() {
    // `lupin FILE.lu` — no subcommand — runs the file; stdout passes through.
    let output = lupin(&["examples/squares.lu"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout_of(&output), "sum of squares: 30\n");
    assert!(stderr_of(&output).is_empty(), "{}", stderr_of(&output));

    // A program's own exit(N) is the process exit code — wordcount's usage
    // path exits 2 by its own choice, not as a diagnostic.
    let wordcount = format!("{}/corpus/wordcount.lu", wolf_interp::upstream_root());
    let output = lupin(&["run", &wordcount]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stdout_of(&output).contains("usage: wc"),
        "{}",
        stdout_of(&output)
    );
    assert!(stderr_of(&output).is_empty(), "{}", stderr_of(&output));
}

#[test]
fn the_front_door_trap_prints_its_diagnostic_and_exits_3() {
    let output = lupin(&["examples/overflow.lu"]);
    assert_eq!(output.status.code(), Some(3));
    let stderr = stderr_of(&output);
    assert!(stderr.contains("trap(overflow)"), "{stderr}");
    assert!(stderr.contains("[arith.checked]"), "{stderr}");
}

#[test]
fn the_front_door_static_rejection_exits_2() {
    let file = format!(
        "{}/corpus/grammar/structlit_cond.lu",
        wolf_interp::upstream_root()
    );
    let output = lupin(&["run", &file]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr_of(&output).contains("E0006"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn the_front_door_unsupported_explains_itself_and_exits_4() {
    let file = format!("{}/corpus/comptime.lu", wolf_interp::upstream_root());
    let output = lupin(&["run", &file]);
    assert_eq!(output.status.code(), Some(4));
    assert!(
        stderr_of(&output).contains("unsupported:"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn the_stdin_door_reads_a_program_with_file_semantics() {
    let squares =
        std::fs::read(manifest_dir().join("examples/squares.lu")).expect("the example exists");
    let output = lupin_with_stdin(&["-"], &manifest_dir(), &squares);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout_of(&output), "sum of squares: 30\n");

    // Same semantics as a file: a bare expression is not a compilation unit,
    // so the diagnostic prints and the static-rejection code reports it.
    let output = lupin_with_stdin(&["-"], &manifest_dir(), b"1 + 1\n");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr_of(&output).contains("E0201"),
        "{}",
        stderr_of(&output)
    );

    // The explicit spelling routes identically, and a trap exits 3 here too.
    let overflow =
        std::fs::read(manifest_dir().join("examples/overflow.lu")).expect("the example exists");
    let output = lupin_with_stdin(&["run", "-"], &manifest_dir(), &overflow);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        stderr_of(&output).contains("trap(overflow)"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn run_json_is_the_record_surface_and_the_tool_exits_zero() {
    // `run --json` = what conform-run does at the front door: the record
    // carries the outcome, the tool exits 0 ([proto.invoke.exit]).
    let output = lupin(&["run", "examples/overflow.lu", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&output).trim()).expect("one JSON object");
    assert_eq!(schema::validate(&value), Ok(()));
    assert_eq!(value["impl"], "lupin");
    assert_eq!(value["impl_version"], "0.1.2");
    assert_eq!(value["verdict"], "trap(overflow)");

    // The stdin record reports `-` as the file — the only spelling it had.
    let squares =
        std::fs::read(manifest_dir().join("examples/squares.lu")).expect("the example exists");
    let output = lupin_with_stdin(&["run", "-", "--json"], &manifest_dir(), &squares);
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&output).trim()).expect("one JSON object");
    assert_eq!(schema::validate(&value), Ok(()));
    assert_eq!(value["file"], "-");
    assert_eq!(value["verdict"], "exit(0)");
}

#[test]
fn run_surfaces_the_scheduler_controls() {
    let file = format!(
        "{}/corpus/conc/select_seeded.lu",
        wolf_interp::upstream_root()
    );
    let seeded = lupin(&["run", &file, "--seed", "7", "--json"]);
    assert_eq!(seeded.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&seeded).trim()).expect("one JSON object");
    assert_eq!(value["seeded"], true);

    let replayed = lupin(&["run", &file, "--schedule", "ev:0", "--json"]);
    assert_eq!(replayed.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&replayed).trim()).expect("one JSON object");
    assert_eq!(value["seeded"], true);
}

#[test]
fn eval_prints_the_value_like_the_repl_and_reports_on_the_front_door_scale() {
    let output = lupin(&["eval", "1 + 1"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout_of(&output), "2 : i32\n");

    // `-e` is the short spelling of the same door.
    let output = lupin(&["-e", "1 + 1"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout_of(&output), "2 : i32\n");

    // A trap renders exactly as the REPL renders it, and exits 3.
    let output = lupin(&["eval", "1 / 0"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        stdout_of(&output).contains("trap(div-zero)"),
        "{}",
        stdout_of(&output)
    );

    // A snippet the frontend rejects exits 2.
    let output = lupin(&["eval", "let x ="]);
    assert_eq!(output.status.code(), Some(2));

    // Outside scope exits 4 with the reason printed.
    let output = lupin(&["eval", "acquire()"]);
    assert_eq!(output.status.code(), Some(4));
    assert!(
        stdout_of(&output).contains("unsupported:"),
        "{}",
        stdout_of(&output)
    );
}

#[test]
fn check_is_the_frontend_only_fast_path() {
    let hello = format!("{}/corpus/hello.lu", wolf_interp::upstream_root());
    let output = lupin(&["check", &hello]);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stdout_of(&output).ends_with("hello.lu: ok\n"),
        "{}",
        stdout_of(&output)
    );

    let bad = format!(
        "{}/corpus/grammar/structlit_cond.lu",
        wolf_interp::upstream_root()
    );
    // One rejection turns the whole invocation red (exit 2), diagnostics on
    // stderr, and the clean file still reports ok.
    let output = lupin(&["check", &hello, &bad]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stdout_of(&output).contains("hello.lu: ok"),
        "{}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("E0006"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn bare_lupin_opens_the_repl() {
    let output = lupin_with_stdin(&[], &manifest_dir(), b"1 + 1\n");
    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("wolf> "), "{stdout}");
    assert!(stdout.contains("2 : i32"), "{stdout}");
}

#[test]
fn a_subcommand_name_wins_over_a_file_of_the_same_name() {
    // The documented collision rule: `lupin repl` opens the REPL even in a
    // directory holding a file named `repl`; that file runs as
    // `lupin run repl`.
    let dir = std::env::temp_dir().join("lupin-collision-test");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let squares =
        std::fs::read(manifest_dir().join("examples/squares.lu")).expect("the example exists");
    std::fs::write(dir.join("repl"), &squares).expect("the collision file writes");

    let output = lupin_with_stdin(&["repl"], &dir, b"");
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stdout_of(&output).starts_with("wolf> "),
        "{}",
        stdout_of(&output)
    );

    let output = lupin_with_stdin(&["run", "repl"], &dir, b"");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout_of(&output), "sum of squares: 30\n");
}

#[test]
fn version_names_the_binary_the_package_and_the_pin() {
    let output = lupin(&["--version"]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout_of(&output).trim_end();
    let prefix = "lupin 0.1.2 (wolf-interp, pin ";
    assert!(text.starts_with(prefix), "{text}");
    let pin = text
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(')'))
        .expect("the version line closes its parenthesis");
    assert_eq!(pin.len(), 7, "{text}");
    assert!(pin.chars().all(|c| c.is_ascii_hexdigit()), "{text}");

    // `-V` is the short spelling.
    let short = lupin(&["-V"]);
    assert_eq!(stdout_of(&short), stdout_of(&output));
}

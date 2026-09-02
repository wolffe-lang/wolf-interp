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
    // 345 -> 351 at the 21b129e pin: s111's wrapping-family wave lands
    // (sha256_block, the K-table witness, the narrowing wrapping cast)
    // plus s110's header-promotion pair and the #133 field-lend witness.
    // 351 -> 361 at the da8582d pin (is22): s112's constant-time tier lands
    // eight `ct/` taint witnesses plus the two `kernels/ct_*` flagships.
    // 361 -> 374 at the 77466a3 pin (is23): s113's D54 tier lands nine
    // `typecheck/numlit_*` witnesses, `typecheck/cast_int_to_float`, and three
    // `faults/cast_float_*` cast witnesses (#138).
    // 374 -> 385 at the 90c90df pin (is24): the s114-s116 wave lands the D56
    // wrap-cast trio, s116's imported-element pair (entry + member), the
    // diverging-else witness, and the out-of-scope signal/byte std pairs.
    // 385 -> 403 at the a900b8c pin (is26): the s117-s121 wave — the seven
    // char witnesses this sprint exists for (char_battery/order/interp,
    // chars_walk, the three faults/char_cast_* twins), the boundary battery,
    // the memory/conc cluster witnesses and the carried-quotient pair, and
    // the out-of-scope os.random trio + comptime sandbox witness.
    // 403 -> 422 at the e561c6f pin (is27): the s122-s125 wave — the D59
    // resolve/ membership witnesses (including the corpus's first four BARE
    // members, legible once [conf.directive.standalone]'s plain-member
    // default landed), the s125 overflow-on-pop twins, the s123 match/str
    // cluster, and the four numlit witnesses.
    // 422 -> 430 at the addcd7f pin (is28): the s126 wave — the eight D61
    // index_origin_* witnesses (six run-reaching, the E0813/E0211 fail
    // pins), the executable contract this sprint's origin-marker half is
    // implemented against.
    // 430 -> 445 at the c88ab64 pin (is30): the s127/s128/r03 wave to
    // v0.2.0 — the D63 let_group quartet, the s128 destructure trio, the
    // #171 slice quartet, and the D62 concat quartet.
    // 445 -> 455 at the 83f83bb pin (is30's second bump, the s129 merge):
    // the six [gram.pat.struct] witnesses and #184's slice quartet.
    // 455 -> 463 at the b80d239 pin (is31, the s130 merge): the eight match
    // ARM witnesses — the struct/tuple twins, product nesting, `@`-bindings,
    // the arm-boundary move, the E0801/E0802 product pins, and the two c06
    // residue files (deep trees, str at product depth).
    // 463 -> 474 at the e6cf24e pin (is32, the s131 merge + the 2026-09-01
    // ledger ritual): the two [mem.region.account] witnesses this sprint
    // exists for, D67's three comma refusals, #196's two or-pattern residue
    // pins, r04's seven-digit escape witness, and the region/lint trio
    // (region_call_allocates, region_unit_tail_call, defer_loop_turn).
    // 474 -> 479 at the 2bfbe5e pin (is33, the s132 merge): the three
    // [mem.region.cap] witnesses this sprint exists for (the breach, the
    // boundary cases, the proc-join fault) and D69's two separator refusals.
    // 479 -> 482 at the 8cda3aa pin (is34, wolf-lang v0.2.2): r05's three
    // letters land — `faults/trap_skips_root_defers.lu` (#209's witness, the
    // one this sprint exists for: it arrived a MEASURED divergence and flips
    // in the same commit that takes the ruling) and #198's string half,
    // `grammar/str_uni_seven_digits.lu` + `strings/str_uni_leading_zeros.lu`,
    // which this lexer answered at first sight, at E0101 and at the same
    // column its `char` twin uses.
    // Moved with the pin, per the export.rs rule.
    assert!(stdout.contains("482 file(s)"), "{stdout}");
    assert!(stdout.contains("0 failure(s)"), "{stdout}");
}

#[test]
fn the_corpus_walk_has_a_machine_mode() {
    let output = lupin(&["corpus", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_str(stdout_of(&output)).expect("json");
    assert_eq!(value["total"], 482);
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

/// A planted schedule-dependent program: the exit code is whichever send
/// committed first. Two distinct outcomes, each with its own decision stream.
const ORDER_DEPENDENT: &str = "//! member: false\n\
     fn main() -> !int {\n\
     \x20   let ch = channel[int](2)\n\
     \x20   scope s {\n\
     \x20       s.spawn(fn() { ch.send(1) })\n\
     \x20       s.spawn(fn() { ch.send(2) })\n\
     \x20   }\n\
     \x20   let first = ch.recv() else |_| { return 9 }\n\
     \x20   first\n\
     }\n";

#[test]
fn a_replay_artifact_is_built_from_one_explore_and_one_self_contained_record() {
    // wolf-interp#53, both gaps, as the consumer that filed them: lobo's
    // `tools/lobo-replay` writes a `.loborace` artifact {case, seed,
    // schedule, verdict, stdout_sha256} and replays it. Before this the tool
    // had to run the exploration TWICE — once for `--json`'s seeds, once for
    // the human report's "decision stream:" lines — and pair the two by
    // adjacency in the text, then carry the seed out of band because the
    // record it saved as the artifact body did not name its own schedule.
    //
    // Below: ONE explore, no text scraping, and a record that is a complete
    // artifact on its own.
    let dir = std::env::temp_dir().join("lupin-replay-artifact-test");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let case = dir.join("race.lu");
    std::fs::write(&case, ORDER_DEPENDENT).expect("the case writes");
    let case = case.to_string_lossy().into_owned();

    let output = lupin(&["conform-run", &case, "--explore=200", "--json"]);
    // A schedule-dependent finding exits 1 — that is the gate, not an error.
    assert_eq!(output.status.code(), Some(1), "{}", stdout_of(&output));
    let report: serde_json::Value =
        serde_json::from_str(stdout_of(&output).trim()).expect("one JSON object");
    assert_eq!(report["stable"], false);
    let outcomes = report["outcomes"].as_array().expect("outcomes");
    assert_eq!(outcomes.len(), 2);

    for outcome in outcomes {
        // Gap 1: the decision stream rides the record, per outcome, in the
        // spelling `--schedule=` takes. `seed` is the packed handle beside it.
        let stream = outcome["schedule"].as_str().expect("a schedule member");
        assert!(stream.starts_with("ev:"), "{stream}");
        let seed = outcome["seed"].as_u64().expect("these streams pack");
        assert_eq!(outcome["replay"], format!("--seed={seed}"));

        // Replay by SEED: the record is the artifact body, and gap 2 makes it
        // self-contained — the schedule it replays is IN it.
        let by_seed = lupin(&["conform-run", &case, "--json", &format!("--seed={seed}")]);
        let record: serde_json::Value =
            serde_json::from_str(stdout_of(&by_seed).trim()).expect("one JSON object");
        assert_eq!(schema::validate(&record), Ok(()));
        assert_eq!(record["seeded"], true);
        assert_eq!(record["x-seed"], serde_json::json!(seed));
        assert_eq!(record["verdict"], outcome["verdict"]);

        // Replay by STREAM: the same observation, and the record names the
        // stream instead of a seed — whichever spelling the artifact used.
        let by_stream = lupin(&[
            "conform-run",
            &case,
            "--json",
            &format!("--schedule={stream}"),
        ]);
        let replayed: serde_json::Value =
            serde_json::from_str(stdout_of(&by_stream).trim()).expect("one JSON object");
        assert_eq!(replayed["x-schedule"], serde_json::json!(stream));
        assert_eq!(replayed["verdict"], outcome["verdict"]);
        assert_eq!(replayed["stdout_sha256"], record["stdout_sha256"]);
    }

    // An unseeded record stands behind neither key — honest-absent, and
    // `[proto.cmp.defined-divergence]` never counts an absent `x-` key.
    let plain = lupin(&["conform-run", &case, "--json"]);
    let record: serde_json::Value =
        serde_json::from_str(stdout_of(&plain).trim()).expect("one JSON object");
    assert_eq!(record["seeded"], false);
    assert!(record.get("x-seed").is_none());
    assert!(record.get("x-schedule").is_none());
}

#[test]
fn conform_run_explore_refuses_a_statically_rejected_program() {
    // Issue #22: `--explore` runs the SAME admission ladder as `run`. A
    // program the run door rejects with E1101 gets no exploration and no
    // certificate — before 0.1.7 this exact invocation printed "observably
    // deterministic" for a program the same binary refused to run.
    let file = format!(
        "{}/corpus/conc/store_buffer.lu",
        wolf_interp::upstream_root()
    );
    let output = lupin(&["conform-run", &file, "--explore=100"]);
    assert_eq!(output.status.code(), Some(2), "{}", stdout_of(&output));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E1101"), "{stderr}");
    assert!(stderr.contains("same admission ladder"), "{stderr}");
    assert!(
        !stdout_of(&output).contains("observably deterministic"),
        "a refused program must not be certified"
    );
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
fn the_trap_names_its_site_line_col_and_json_keeps_the_byte_span() {
    // `[conf.trap.render]`: the human line spells the location `line:col`
    // (1-based, character columns — the fault snapshots' own spelling) and
    // the raw byte span stays on `--json`'s `x-trap-span`. The program is
    // the cross-machine site witness: its trap expression sits at 6:5, the
    // s125 witness shape, so the compiled tier's report line for the same
    // program reads `  at <file>:6:5` (`[conf.trap.report]`) and both
    // machines name the same place.
    let output = lupin(&["tests/faults/trap_site.lu"]);
    assert_eq!(output.status.code(), Some(3));
    let stderr = stderr_of(&output);
    assert!(stderr.contains("trap(bounds)"), "{stderr}");
    assert!(stderr.contains("[mem.ub.defined] at 6:5"), "{stderr}");
    assert!(
        !stderr.contains(".."),
        "byte offsets left the human line at is27: {stderr}"
    );

    let output = lupin(&["run", "--json", "tests/faults/trap_site.lu"]);
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_str(stdout_of(&output).trim()).expect("one JSON record");
    assert_eq!(value["verdict"], "trap(bounds)");
    assert_eq!(value["x-trap-span"][0], 145, "{value}");
    assert_eq!(value["x-trap-span"][1], 150, "{value}");
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
    // The package is the authority; a literal here rots every release
    // (wolf-lang#87 is the same complaint about the binary's own line).
    assert_eq!(value["impl_version"], env!("CARGO_PKG_VERSION"));
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
fn a_scratch_directory_of_member_false_programs_runs_each_alone() {
    // The wolf-interp#49 live reproducer (D59): three programs share one
    // directory, each carrying exactly one `//! member: false` line, and
    // each builds and runs alone — no E0302 collision on `main`.
    let dir = std::env::temp_dir().join("lupin-d59-scratch-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    for (name, word) in [
        ("hello.lu", "one"),
        ("world.lu", "two"),
        ("third.lu", "three"),
    ] {
        std::fs::write(
            dir.join(name),
            format!("//! member: false\nfn main() -> !int {{\n    print(\"{word}\")\n    0\n}}\n"),
        )
        .expect("fixture writes");
    }
    for (name, word) in [
        ("hello.lu", "one"),
        ("world.lu", "two"),
        ("third.lu", "three"),
    ] {
        let output = lupin_with_stdin(&["run", name], &dir, b"");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{name}: {}",
            stderr_of(&output)
        );
        assert_eq!(stdout_of(&output), format!("{word}\n"), "{name}");
    }
}

#[test]
fn a_standalone_mark_never_shrinks_the_module_a_build_forms_around_it() {
    // D59's asymmetry: plain siblings are shared members of EVERY build, so
    // a plain `main` beside a standalone `main` still collides (E0302), and
    // the note names the escape.
    let dir = std::env::temp_dir().join("lupin-d59-asymmetry-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(
        dir.join("prog.lu"),
        "//! member: false\nfn main() -> !int {\n    print(\"s\")\n    0\n}\n",
    )
    .expect("fixture writes");
    std::fs::write(
        dir.join("plain.lu"),
        "fn main() -> !int {\n    print(\"plain\")\n    0\n}\n",
    )
    .expect("fixture writes");
    let output = lupin_with_stdin(&["run", "prog.lu"], &dir, b"");
    assert_eq!(output.status.code(), Some(2), "{}", stderr_of(&output));
    let stderr = stderr_of(&output);
    assert!(stderr.contains("E0302"), "{stderr}");
    assert!(
        stderr.contains("`//! member: false`"),
        "escape named: {stderr}"
    );
}

#[test]
fn a_name_defined_in_a_standalone_sibling_is_taught_not_just_missed() {
    // The compiler's E0301 situation (a), this machine's voice: the missing
    // name IS defined next door, in a file that opted out — the note names
    // the file, the marker, and the fix.
    let dir = std::env::temp_dir().join("lupin-d59-teachnote-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(
        dir.join("main.lu"),
        "fn main() -> !int {\n    print(\"{helper()}\")\n    0\n}\n",
    )
    .expect("fixture writes");
    std::fs::write(
        dir.join("util_test.lu"),
        "fn helper() -> int { 7 }\nfn main() -> !int { 0 }\n",
    )
    .expect("fixture writes");
    let output = lupin_with_stdin(&["run", "main.lu"], &dir, b"");
    assert_eq!(output.status.code(), Some(4), "{}", stderr_of(&output));
    let stderr = stderr_of(&output);
    assert!(stderr.contains("`util_test.lu`"), "{stderr}");
    assert!(stderr.contains("`_test.lu` file name"), "{stderr}");
    assert!(stderr.contains("conf.directive.standalone"), "{stderr}");
}

#[test]
fn version_names_the_binary_the_package_and_the_pairing() {
    // r01 row 7: the version line names the pairing posture — the binary,
    // the package, and "reference interpreter at pin <sha>". D57 (r02)
    // adds the build's honesty: a build made exactly at its release tag
    // prints the bare version; any other build carries `+dev.<commit>`,
    // so an off-tag build never claims to be the release.
    let output = lupin(&["--version"]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout_of(&output).trim_end();
    let base = concat!("lupin ", env!("CARGO_PKG_VERSION"));
    let rest = text
        .strip_prefix(base)
        .unwrap_or_else(|| panic!("the version line opens with the crate version: {text}"));
    let (suffix, tail) = rest
        .split_once(" (wolf-interp, reference interpreter at pin ")
        .unwrap_or_else(|| panic!("the version line names the pairing posture: {text}"));
    if !suffix.is_empty() {
        let build = suffix.strip_prefix("+dev.").unwrap_or_else(|| {
            panic!("an off-tag build spells its suffix `+dev.<commit>`: {text}")
        });
        assert!(
            build == "unknown"
                || (build.len() == 7 && build.chars().all(|c| c.is_ascii_hexdigit())),
            "{text}"
        );
    }
    let pin = tail
        .strip_suffix(')')
        .expect("the version line closes its parenthesis");
    assert_eq!(pin.len(), 7, "{text}");
    assert!(pin.chars().all(|c| c.is_ascii_hexdigit()), "{text}");

    // `-V` is the short spelling.
    let short = lupin(&["-V"]);
    assert_eq!(stdout_of(&short), stdout_of(&output));
}

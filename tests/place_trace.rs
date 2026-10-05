//! The place trace (is72): `lupin --trace-places`.
//!
//! Three things are held here:
//!
//! 1. **The goldens.** Each `tests/place_trace/<name>/<name>.lu` runs with the
//!    trace on, and the lines must equal `<name>.trace` byte for byte.
//!    `pack` is the maintainer's chapter-7 question (`~/scratch/wolf/ch7/
//!    pack.lu`, copied unchanged); `tree` and `movedleaf` are wolf-book
//!    e7772338 §7.2's program and its moved-leaf variant; `take_arg`, `copy`,
//!    `plain_move`, `elements` and `granules` are the contract's other
//!    shapes. `PLACE_TRACE_BLESS=1 cargo test --test place_trace` rewrites
//!    the goldens — then READ the diff.
//! 2. **The schema.** Every golden line parses as JSON and carries version 1's
//!    keys and closed vocabularies (`docs/manual/06-place-trace.md`).
//! 3. **Behaviour identity.** The observation with the trace on equals the one
//!    without it — over the goldens, and over the vendored corpus's
//!    `memory/*move*` programs in process. The whole corpus, through the real
//!    binary, is `ci/place-trace-identity.sh`'s (the ladder runs it).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use wolf_interp::eval::PlaceTrace;
use wolf_interp::frontend::{self, Observation};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A sink the test can read back after the run.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("unpoisoned").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().expect("unpoisoned").clone()).expect("the trace is UTF-8")
    }
}

/// Runs `file` with the trace on: the observation and the trace text.
fn traced(file: &Path) -> (Observation, String) {
    let source = std::fs::read(file).expect("the program exists");
    let sink = Captured::default();
    let tracer = PlaceTrace::new(
        Box::new(sink.clone()),
        String::from_utf8_lossy(&source).into_owned(),
    );
    let observation = frontend::observe_file_traced(file, &source, tracer);
    (observation, sink.text())
}

fn untraced(file: &Path) -> Observation {
    let source = std::fs::read(file).expect("the program exists");
    frontend::observe_file(
        file,
        &source,
        None,
        wolf_interp::eval::Trace::Off,
        &wolf_interp::eval::SchedRequest::Default,
        None,
    )
}

/// Everything about an observation a person or a record could see.
fn behaviour(observation: &Observation) -> String {
    format!(
        "phase={:?}\nverdict={:?}\nstdout={:?}\ntrap={:?}\nub={:?}\nreason={:?}\ndiagnostics={:?}\nleaks={:?}\nhost_leaks={:?}\nforest={:?}",
        observation.phase_reached,
        observation.verdict,
        String::from_utf8_lossy(&observation.stdout),
        observation.trap,
        observation.ub,
        observation.reason,
        observation.diagnostics,
        observation.leaks,
        observation.host_leaks,
        observation.forest,
    )
}

/// The golden programs, sorted: `(name, program path)`.
fn goldens() -> Vec<(String, PathBuf)> {
    let root = manifest_dir().join("tests/place_trace");
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(&root)
        .expect("tests/place_trace exists")
        .map(|entry| entry.expect("readable").path())
        .filter(|path| path.is_dir())
        .map(|dir| {
            let name = dir
                .file_name()
                .expect("named")
                .to_string_lossy()
                .into_owned();
            let program = dir.join(format!("{name}.lu"));
            (name, program)
        })
        .collect();
    found.sort();
    found
}

#[test]
fn the_goldens_hold() {
    let bless = std::env::var_os("PLACE_TRACE_BLESS").is_some();
    let all = goldens();
    assert!(all.len() >= 9, "the goldens went missing: {all:?}");
    let mut failures = Vec::new();
    for (name, program) in &all {
        let (_, trace) = traced(program);
        let golden = program.with_extension("trace");
        if bless {
            std::fs::write(&golden, &trace).expect("golden writable");
            continue;
        }
        let expected = std::fs::read_to_string(&golden)
            .unwrap_or_else(|e| panic!("{}: {e}", golden.display()))
            .replace("\r\n", "\n");
        if trace != expected {
            failures.push(format!(
                "{name}: the trace moved\n--- expected\n{expected}--- got\n{trace}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_trace_never_changes_a_golden_programs_behaviour() {
    for (name, program) in goldens() {
        let (on, _) = traced(&program);
        let off = untraced(&program);
        assert_eq!(behaviour(&on), behaviour(&off), "{name}");
    }
}

/// `[mem.tier0.move.*]` over the vendored corpus's move programs (every
/// `memory/` file whose name says `move`), in process: every observation
/// byte-identical with the trace on and off. The whole family took 25
/// minutes in a debug build on kasumi (the trace renders the frame after
/// every statement of every loop), so the whole corpus is the release
/// binary's, in `ci/place-trace-identity.sh`.
#[test]
fn the_trace_never_changes_the_move_corpus() {
    let root = PathBuf::from(wolf_interp::upstream_root()).join("corpus/memory");
    let mut files: Vec<PathBuf> = walk(&root)
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains("move"))
        })
        .collect();
    files.sort();
    assert!(
        files.len() >= 30,
        "memory/*move* went missing: {}",
        files.len()
    );
    let mut lines = 0usize;
    for file in &files {
        let (on, trace) = traced(file);
        let off = untraced(file);
        assert_eq!(behaviour(&on), behaviour(&off), "{}", file.display());
        lines += trace.lines().count();
    }
    // The family moves things: a trace that stayed silent over all of it
    // would be a hook that never ran.
    assert!(lines > 100, "only {lines} trace lines over memory/*move*");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("readable") {
        let path = entry.expect("readable").path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|ext| ext == "lu") {
            out.push(path);
        }
    }
    out
}

const STATES: [&str; 3] = ["live", "moved", "uninit"];
const MOVE_BY: [&str; 6] = ["take", "move", "plain", "match", "freeze", "mut"];
const COPY_BY: [&str; 2] = ["plain", "copy"];

fn is_position(text: &str) -> bool {
    let mut parts = text.split(':');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(line), Some(col), None)
            if line.parse::<usize>().is_ok_and(|n| n > 0) && col.parse::<usize>().is_ok_and(|n| n > 0)
    )
}

/// Every golden line is version 1: the line keys, the place keys and the
/// event keys, each from its closed set.
#[test]
fn every_golden_line_is_schema_version_1() {
    for (name, program) in goldens() {
        let (_, trace) = traced(&program);
        for line in trace.lines() {
            let value: serde_json::Value =
                serde_json::from_str(line).unwrap_or_else(|e| panic!("{name}: {e}: {line}"));
            let object = value.as_object().expect("a line is an object");
            for key in object.keys() {
                assert!(
                    [
                        "trace", "task", "fn", "depth", "at", "tail", "trap", "places", "events"
                    ]
                    .contains(&key.as_str()),
                    "{name}: unknown line key {key}"
                );
            }
            assert_eq!(object["trace"], 1, "{name}");
            assert!(
                object["task"].is_u64() && object["depth"].is_u64(),
                "{name}"
            );
            assert!(object["fn"].is_string(), "{name}");
            assert!(is_position(object["at"].as_str().expect("at")), "{name}");
            for place in object["places"].as_array().expect("places") {
                let place = place.as_object().expect("a place is an object");
                for key in place.keys() {
                    assert!(
                        [
                            "path", "state", "type", "value", "len", "elided", "of", "by", "at",
                            "reinit"
                        ]
                        .contains(&key.as_str()),
                        "{name}: unknown place key {key}"
                    );
                }
                let state = place["state"].as_str().expect("state");
                assert!(STATES.contains(&state), "{name}: state {state}");
                assert!(place["type"].is_string(), "{name}");
                match state {
                    "live" => {
                        assert!(!place.contains_key("by") && !place.contains_key("of"));
                    }
                    "moved" => {
                        assert!(MOVE_BY.contains(&place["by"].as_str().expect("by")));
                        assert!(is_position(place["at"].as_str().expect("at")));
                        assert!(!place.contains_key("value"));
                    }
                    _ => {
                        assert!(place["of"].is_string() && !place.contains_key("value"));
                        assert!(is_position(place["at"].as_str().expect("at")));
                    }
                }
            }
            for event in object["events"].as_array().expect("events") {
                let event = event.as_object().expect("an event is an object");
                let ev = event["ev"].as_str().expect("ev");
                match ev {
                    "move" => assert!(MOVE_BY.contains(&event["by"].as_str().expect("by"))),
                    "copy" => assert!(COPY_BY.contains(&event["by"].as_str().expect("by"))),
                    "reinit" => assert!(!event.contains_key("by")),
                    other => panic!("{name}: unknown event {other}"),
                }
                assert!(event["path"].is_string());
                assert!(is_position(event["at"].as_str().expect("at")));
            }
        }
    }
}

/// The lines of `trace` whose `at` is `at`, parsed.
fn line_at(trace: &str, at: &str) -> serde_json::Value {
    trace
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json"))
        .find(|line| line["at"] == at && line["fn"] == "main")
        .unwrap_or_else(|| panic!("no line at {at}"))
}

fn place<'a>(line: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    line["places"]
        .as_array()
        .expect("places")
        .iter()
        .find(|place| place["path"] == path)
        .unwrap_or_else(|| panic!("no place {path} in {line}"))
}

/// The maintainer's question, answered by the machine: `take p.lead`
/// moves the field, `p.lead = "lin"` re-initializes it, and `let c =
/// p.lead` COPIES it (`str` is `Copy`), so `p.lead` stays live after.
#[test]
fn pack_take_moves_the_store_revives_and_the_let_copies() {
    let (_, trace) = traced(&manifest_dir().join("tests/place_trace/pack/pack.lu"));
    let taken = line_at(&trace, "10:2");
    assert_eq!(place(&taken, "p.lead")["state"], "moved");
    assert_eq!(place(&taken, "p.lead")["by"], "take");
    assert_eq!(place(&taken, "p.tail")["state"], "live");
    let stored = line_at(&trace, "11:2");
    assert_eq!(place(&stored, "p.lead")["state"], "live");
    assert_eq!(place(&stored, "p.lead")["reinit"], "11:2");
    let copied = line_at(&trace, "12:2");
    assert_eq!(place(&copied, "p.lead")["state"], "live");
    assert_eq!(place(&copied, "c")["value"], "lin");
    assert_eq!(copied["events"][0]["ev"], "copy");
    assert_eq!(copied["events"][0]["to"], "c");
    for at in ["13:2", "14:2", "17:2"] {
        assert_eq!(
            place(&line_at(&trace, at), "p.lead")["state"],
            "live",
            "{at}"
        );
    }
}

/// The trace and the trap read one machine: the moved place's `at` in the
/// trapping line is the move site the trap message names.
#[test]
fn the_trace_and_the_trap_name_the_same_move() {
    let (observation, trace) =
        traced(&manifest_dir().join("tests/place_trace/movedleaf/movedleaf.lu"));
    let fault = observation.trap.expect("movedleaf traps");
    let last = trace.lines().last().expect("a line");
    let last: serde_json::Value = serde_json::from_str(last).expect("json");
    assert_eq!(last["trap"], "use-after-move");
    let moved = place(&last, "d.meta.author");
    assert_eq!(moved["state"], "moved");
    let source =
        std::fs::read_to_string(manifest_dir().join("tests/place_trace/movedleaf/movedleaf.lu"))
            .expect("readable");
    let (span, _) = fault.secondary.expect("the trap names the move");
    let (line, col) = wolf_interp::diag::line_col(&source, span.start);
    assert_eq!(moved["at"], format!("{line}:{col}"));
}

// -- the command line ---------------------------------------------------------

fn lupin(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(args)
        .current_dir(manifest_dir())
        .output()
        .expect("lupin runs")
}

/// Bare `--trace-places` writes to stderr and nothing else moves: stdout and
/// the exit code are the untraced run's, and every stderr line but the
/// program's own fault line is a trace line.
#[test]
fn the_flag_writes_stderr_and_leaves_stdout_and_the_exit_code_alone() {
    for name in ["pack", "movedleaf"] {
        let program = format!("tests/place_trace/{name}/{name}.lu");
        let off = lupin(&[&program]);
        let on = lupin(&["--trace-places", &program]);
        assert_eq!(on.stdout, off.stdout, "{name}");
        assert_eq!(on.status.code(), off.status.code(), "{name}");
        let on_err = String::from_utf8_lossy(&on.stderr);
        let kept: String = on_err
            .lines()
            .filter(|line| !line.starts_with("{\"trace\":1,"))
            .map(|line| format!("{line}\n"))
            .collect();
        assert_eq!(kept, String::from_utf8_lossy(&off.stderr), "{name}");
        assert!(on_err.lines().count() > 3, "{name}: no trace on stderr");
        let run = lupin(&["run", "--trace-places", &program]);
        assert_eq!(run.stdout, off.stdout, "{name}: run");
        assert_eq!(run.stderr, on.stderr, "{name}: run");
    }
}

#[test]
fn the_flag_with_a_path_writes_the_file_and_stderr_stays_the_programs() {
    let out = manifest_dir().join("target/place-trace-cli-test.jsonl");
    let _ = std::fs::remove_file(&out);
    let program = "tests/place_trace/pack/pack.lu";
    let flag = format!("--trace-places={}", out.display());
    let on = lupin(&[&flag, program]);
    let off = lupin(&[program]);
    assert_eq!(on.stdout, off.stdout);
    assert_eq!(on.stderr, off.stderr);
    assert_eq!(on.status.code(), Some(0));
    let written = std::fs::read_to_string(&out).expect("the trace file exists");
    let golden = std::fs::read_to_string(manifest_dir().join("tests/place_trace/pack/pack.trace"))
        .expect("golden")
        .replace("\r\n", "\n");
    assert_eq!(written, golden);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn the_flag_is_refused_where_it_means_nothing() {
    // The record surface prints a record, not a run.
    let json = lupin(&[
        "run",
        "--json",
        "--trace-places",
        "tests/place_trace/pack/pack.lu",
    ]);
    assert_eq!(json.status.code(), Some(2));
    // Before a subcommand it would be silently dropped; it is refused.
    let sub = lupin(&["--trace-places", "check", "tests/place_trace/pack/pack.lu"]);
    assert_eq!(sub.status.code(), Some(2));
    assert!(sub.stdout.is_empty());
    // An unwritable path is a tool error, and the program never runs.
    let bad = lupin(&[
        "--trace-places=target/no-such-dir/x/trace.jsonl",
        "tests/place_trace/pack/pack.lu",
    ]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(bad.stdout.is_empty(), "the program ran: {:?}", bad.stdout);
}

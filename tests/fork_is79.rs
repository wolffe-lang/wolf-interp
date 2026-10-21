//! is79 (wolf-interp#269; wolf-lang s247, `[os.proc.fork]`; ruling #95
//! R5 A) — the fork, refused by name.
//!
//! `os_fork() -> (int, int) ! {busy, io, unsupported}` makes a child that
//! keeps running wolf code. lupin admits no `unsafe` and Rust's standard
//! library has no fork, so a fork is refused BY NAME on a host that has one
//! — the record's `x-unsupported` names `os_fork` — and windows, which has
//! none, answers the `unsupported` ROW, as every machine does there. Until
//! 0.1.55 the name did not resolve at all.
//!
//! `busy` (a fork from a task, or with a task live) is the compiled
//! runtime's protocol for its own threads; this machine never forks, so it
//! has no such rule to apply and refuses before it — as the checked
//! machine, which answers its `unsupported` row on every host (ruling #95
//! R4 C), answers both of `tasks.lu`'s `busy` rows (measured on wolf
//! 0.2.34's archive, is79 `runs/m1/`: `tasks: a fork with a task parked:
//! unsupported`).
//!
//! The programs under `tests/fixtures/fork_is79/` are wolf-lang `9da7bdcf`'s
//! `crates/wolf_driver/tests/fixtures/fork/main.lu` and `tasks.lu` byte for
//! byte (sha256 `fdbba1c4…`, `f4df4b92…`): `fork_lanes.rs` holds lupin's
//! answer on `main.lu` to a refusal naming the call.

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

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fork_is79")
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

fn observe_fixture(name: &str) -> serde_json::Value {
    let dir = scratch(&format!("fork_is79_{}", name.replace('.', "_")));
    record(&observe(&fixture(name), &dir))
}

/// `main.lu`: a fork from `main` with nothing started. Refused by name on
/// unix (never "does not resolve"); on windows the `unsupported` row,
/// which the program prints and ends 1 on, as the checked machine does on
/// every host.
#[test]
fn a_fork_from_main_is_refused_by_name() {
    let rec = observe_fixture("main.lu");
    if cfg!(unix) {
        assert_eq!(rec["verdict"], "unsupported", "{rec}");
        let why = rec["x-unsupported"].as_str().unwrap_or("");
        assert!(
            why.contains("`os_fork`") && !why.contains("does not resolve"),
            "refused by name: {why}"
        );
        assert!(
            why.contains("[os.proc.fork]"),
            "the refusal cites the clause: {why}"
        );
    } else {
        assert_eq!(
            (rec["verdict"].as_str(), rec["stdout_inline"].as_str()),
            (Some("exit(1)"), Some("fork: unsupported\n")),
            "{rec}"
        );
    }
}

/// `tasks.lu`: the first fork is made with a task parked, where a compiled
/// tier answers `busy`. This machine refuses there, by name, before any
/// rule about threads it does not have; windows answers the row.
#[test]
fn a_fork_beside_a_task_is_refused_by_name_too() {
    let rec = observe_fixture("tasks.lu");
    if cfg!(unix) {
        assert_eq!(rec["verdict"], "unsupported", "{rec}");
        let why = rec["x-unsupported"].as_str().unwrap_or("");
        assert!(
            why.contains("`os_fork`") && !why.contains("does not resolve"),
            "refused by name: {why}"
        );
    } else {
        let out = rec["stdout_inline"].as_str().unwrap_or("");
        assert!(
            out.starts_with("tasks: a fork with a task parked: unsupported\n"),
            "windows answers the row: {rec}"
        );
    }
}

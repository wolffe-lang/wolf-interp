//! s242 (wolf-lang's `[os.proc.poll]`, `[os.proc.wait.note]` R9;
//! wolf-lang#680) — the mirror of the poll of the children, at its
//! witnesses.
//!
//! `tests/fixtures/job_wait_s242/` holds wolf-lang s242's corpus row
//! (`job_poll_rows.lu`) and three of its fixtures (`live.lu`, `stops.lu`,
//! `interrupt.lu`), byte for byte, so the answers here are the ones the
//! compiler's gate (`job_wait_lanes.rs`) asserts of lupin.
//!
//! **Nothing here stops, continues or signals a process.** With no child
//! left the poll's answer is decided without the host, `(-1, 0)`; with a
//! child not yet reaped it is refused BY NAME (std's `try_wait` sees no
//! stop, and lupin forbids `unsafe`). `stops.lu` and `interrupt.lu`
//! listen for `CHILD` first, which this machine never does: they stop
//! there, before any child is started. `live.lu` starts one `sh` that
//! exits 7.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(dir.join("target")).expect("scratch created");
    dir
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/job_wait_s242")
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

fn declared_stdout(entry: &Path) -> String {
    let text = std::fs::read_to_string(entry).expect("the row reads");
    let line = text
        .lines()
        .find(|l| l.starts_with("//! check: run("))
        .unwrap_or_else(|| panic!("{} declares a run", entry.display()));
    let start = line.find("stdout=\"").expect("a declared stdout") + "stdout=\"".len();
    let end = line.rfind("\")").expect("the directive closes");
    line[start..end].replace("\\n", "\n") + "\n"
}

/// The poll with no child: the row's declared stdout, as on every
/// compiler machine, windows included.
#[test]
fn the_poll_with_no_child() {
    let entry = fixture("job_poll_rows.lu");
    let rec = record(&observe(&entry, &scratch("job_wait_s242_rows")));
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(rec["stdout_inline"], declared_stdout(&entry), "{rec}");
}

/// A child not yet reaped: the poll is refused by name, saying why.
#[cfg(unix)]
#[test]
fn the_poll_with_a_live_child_is_refused_by_name() {
    let entry = fixture("live.lu");
    let rec = record(&observe(&entry, &scratch("job_wait_s242_live")));
    let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
    assert_eq!(rec["verdict"], "unsupported", "{rec}");
    assert!(
        why.contains("`os_wait_poll`") && why.contains("no `unsafe`") && why.contains("stop"),
        "the refusal names the poll and says why: {why}"
    );
}

/// The compiler's witnesses that listen for `CHILD` stop at the listen,
/// which this machine never serves, before any child is started.
#[test]
fn the_compilers_listening_fixtures_stop_at_the_listen() {
    for file in ["stops.lu", "interrupt.lu"] {
        let entry = fixture(file);
        let rec = record(&observe(&entry, &scratch(&format!("job_wait_s242_{file}"))));
        let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
        assert_eq!(rec["verdict"], "unsupported", "{file}: {rec}");
        assert!(
            why.contains("`os_signal_listen`"),
            "{file}: the refusal names `os_signal_listen`: {why}"
        );
    }
}

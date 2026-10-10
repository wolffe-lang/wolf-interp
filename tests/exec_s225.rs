//! s225 (wolf-lang's `[os.proc.exec]`, `[os.env.unset]`; wolf-lang#534) —
//! the mirror, at its witnesses.
//!
//! The programs under `tests/fixtures/exec_s225/` are wolf-lang s225's two
//! corpus rows and four unix fixtures, byte for byte, so the answers here
//! are the ones the compiler's gate (`exec_lanes.rs`) asserts of every
//! machine. Each runs under `lupin conform-run --json` in a scratch
//! directory of its own.
//!
//! An exec that SUCCEEDS replaces this interpreter: no record follows, the
//! stream is the program's bytes and then the new image's, and the new
//! image's pid is the pid this test started. What this machine does NOT
//! mirror is asserted on its words: a map that names a descriptor above 2,
//! or closes one, is refused BY NAME, as `os_spawn_fds` refuses it.

use std::io::Write as _;
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
        .join("tests/fixtures/exec_s225")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// `lupin conform-run <entry> --json` in `dir`, `stdin` on 0, 1 and 2
/// piped, `WOLF_S225_HOST=1` in its environment; the pid and the output.
fn observe(entry: &Path, dir: &Path, stdin: &str) -> (u32, Output) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .current_dir(dir)
        .env("WOLF_S225_HOST", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("lupin observes");
    let pid = child.id();
    let mut w = child.stdin.take().expect("stdin");
    let _ = w.write_all(stdin.as_bytes());
    drop(w);
    (pid, child.wait_with_output().expect("wait"))
}

fn record(out: &Output) -> Option<serde_json::Value> {
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .filter(|r| r.get("verdict").is_some())
}

fn runs(name: &str, stdin: &str, want: &str) {
    let dir = scratch(&format!("s225-{}", name.replace('.', "_")));
    let (_, out) = observe(&fixture(name), &dir, stdin);
    let rec = record(&out).unwrap_or_else(|| panic!("{name}: a record: {out:?}"));
    assert_eq!(rec["verdict"], "exit(0)", "{name}: {rec}");
    assert_eq!(rec["stdout_inline"], want, "{name}: {rec}");
}

// --------------------------------------------- the corpus rows (every host)

#[test]
fn an_exec_of_a_bad_shape_is_invalid_and_returns() {
    runs(
        "exec_rows.lu",
        "",
        "no_argv=invalid\nno_equals=invalid\nempty_name=invalid\nodd_map=invalid\n\
out_of_range=invalid\nrepeated=invalid\nbad_source=invalid\nstill_here\n",
    );
}

#[test]
fn env_unset_round_trips() {
    runs(
        "env_unset.lu",
        "",
        "got: tarn\nafter: <missing>\nabsent: ok\nempty: invalid\nequals: invalid\nagain: fell\n",
    );
}

// ------------------------------------------------------- the unix fixtures

/// The image replaced: the new `sh` prints the pid this test started, a
/// line of this test's input, its arguments, the one variable it was
/// handed (not the host's), and the open descriptors a control `sh`
/// started here has — and no record follows.
#[cfg(unix)]
#[test]
fn exec_replaces_the_interpreter_and_keeps_the_pid() {
    let dir = scratch("s225-image");
    let scan = "n=; for f in 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19; do [ -e /dev/fd/$f ] && n=$n$f,; done; echo open=$n";
    let control = Command::new("/bin/sh")
        .args(["-c", scan])
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("control sh");
    let control = String::from_utf8_lossy(&control.stdout).trim().to_string();
    let (pid, out) = observe(&fixture("image.lu"), &dir, "from-harness\n");
    assert!(record(&out).is_none(), "no record after an exec: {out:?}");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let want = format!(
        "before exec handle_above_2=true\npid={pid}\nstdin=from-harness\n\
zero=zero one=one two=two words\nenv=fell\nhost=[]\n{control}\n"
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), want);
}

/// A map placing descriptor 5 (and closing 2) is refused by name.
#[cfg(unix)]
#[test]
fn a_map_above_2_is_refused_by_name() {
    let dir = scratch("s225-fd-map");
    let (_, out) = observe(&fixture("fd_map.lu"), &dir, "");
    let rec = record(&out).unwrap_or_else(|| panic!("a record: {out:?}"));
    assert_eq!(rec["verdict"], "unsupported", "{rec}");
    let words = rec.to_string();
    assert!(
        words.contains("names a descriptor above 2 in os_exec"),
        "the refusal names the construct: {words}"
    );
}

/// The rows that return, and a failed exec that mapped a file onto 0
/// leaves 0 as it was.
#[cfg(unix)]
#[test]
fn a_failed_exec_answers_its_row_and_puts_the_process_back() {
    runs(
        "fail.lu",
        "from-harness\n",
        "abs_missing=not_found\nbare_missing=not_found\ndenied=denied\n\
mapped_then_failed=not_found\nstdin=from-harness\n",
    );
}

/// pelt's `pending/unset_env`: a child no longer sees a removed variable,
/// one the program set and one the host did.
#[cfg(unix)]
#[test]
fn a_child_no_longer_sees_a_removed_variable() {
    runs(
        "unset_child.lu",
        "",
        "set=1\nunset=0\nhost=1\nhost_unset=0\nset_again=1\n",
    );
}

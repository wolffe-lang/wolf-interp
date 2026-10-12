//! s232 (wolf-lang's `[os.signal.set]`, `[os.signal.ignored]`,
//! `[os.signal.send]`, `[os.signal.number]`, `[os.proc.self]`;
//! wolf-lang#679, #681, #691, #141) — the mirror, at its witnesses.
//!
//! The programs under `tests/fixtures/signal_shell_s232/` are wolf-lang
//! s232's corpus row (`signal_shell_rows.lu`), the row it moved
//! (`job_rows.lu`) and its nine unix fixtures, byte for byte, so the
//! answers here are the ones the compiler's gate (`signal_shell_lanes.rs`)
//! asserts of every machine. Each runs under `lupin conform-run --json` in
//! a scratch directory of its own.
//!
//! What this machine serves: this process's id and its parent's (std's
//! `process::id` and unix `parent_id`), a meaning's number (the linux and
//! macOS numberings), and every set's, send's and number's SHAPE row.
//! What it does NOT mirror is asserted on its words: the send and the
//! disposition read are refused BY NAME (std has no `kill(pid, sig)` and
//! no `sigaction`), as the dispositions already were; and it never
//! listens (`os_signal_listen` is declined, ruling #57 R8), so the
//! fixtures that listen are declined by that name.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(dir.join("target")).expect("scratch created");
    dir
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/signal_shell_s232")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// `lupin conform-run <entry> --json` in `dir`.
fn observe(entry: &Path, dir: &Path) -> (Output, u32) {
    let child = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("lupin observes");
    let pid = child.id();
    (child.wait_with_output().expect("lupin ends"), pid)
}

fn record(out: &Output) -> serde_json::Value {
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .filter(|r| r.get("verdict").is_some())
        .unwrap_or_else(|| panic!("a record: {out:?}"))
}

/// The stdout a corpus row's own header declares.
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

fn row(name: &str) {
    let entry = fixture(name);
    let dir = scratch(&format!("signal_shell_s232_{}", name.replace('.', "_")));
    let (out, _) = observe(&entry, &dir);
    let rec = record(&out);
    assert_eq!(rec["verdict"], "exit(0)", "{name}: {rec}");
    assert_eq!(
        rec["stdout_inline"],
        declared_stdout(&entry),
        "{name}: {rec}"
    );
}

/// A program this machine declines by `name`: `unsupported`, and the
/// refusal names it.
#[cfg(unix)]
fn refused(fixture_name: &str, name: &str) {
    let entry = fixture(fixture_name);
    let dir = scratch(&format!(
        "signal_shell_s232_{}",
        fixture_name.replace('.', "_")
    ));
    let (out, _) = observe(&entry, &dir);
    let rec = record(&out);
    assert_eq!(rec["verdict"], "unsupported", "{fixture_name}: {rec}");
    let why = format!(
        "{} {} {}",
        rec["x-unsupported"].as_str().unwrap_or(""),
        rec["x-unsupported-construct"].as_str().unwrap_or(""),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        why.contains(&format!("`{name}`")),
        "{fixture_name}: the refusal names `{name}`: {why}"
    );
}

/// The shape rows, the number's two hosts' answers and the ids: the row's
/// declared stdout, as on every compiler machine.
#[test]
fn the_shell_rows() {
    row("signal_shell_rows.lu");
}

/// s219's job rows with the outside bit moved to `KILL`: unchanged answers.
#[test]
fn the_job_rows_with_the_moved_bit() {
    row("job_rows.lu");
}

/// `os_pid` is this process (the one the harness started), `os_ppid` the
/// harness, and a child's `$PPID` is `os_pid` (unix).
#[cfg(unix)]
#[test]
fn this_process_ids() {
    let entry = fixture("ids.lu");
    let dir = scratch("signal_shell_s232_ids");
    let (out, pid) = observe(&entry, &dir);
    let rec = record(&out);
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(
        rec["stdout_inline"],
        format!(
            "pid={pid}\nppid={}\nchild_parent_is_me=true\n",
            std::process::id()
        ),
        "{rec}"
    );
}

/// Every meaning's number, held against the host's numbering by name — the
/// numbers POSIX's XSI option fixes, and the ones linux and macOS part on.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn every_number_is_the_hosts() {
    let entry = fixture("number.lu");
    let dir = scratch("signal_shell_s232_number");
    let rec = record(&observe(&entry, &dir).0);
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    let want = if cfg!(target_os = "macos") {
        "1 15 3 31 2 18 21 22 20 30 14 13 6 19 28 16 26 27 24 25 9 17 11 10 8 4 5 12\n"
    } else {
        "1 15 3 12 2 20 21 22 17 10 14 13 6 18 28 23 26 27 24 25 9 19 11 7 8 4 5 31\n"
    };
    assert_eq!(rec["stdout_inline"], want, "{rec}");
}

/// The send and the disposition read are refused by name; the listening
/// fixtures by the listen's name (unix fixtures).
#[cfg(unix)]
#[test]
fn what_this_machine_refuses_it_names() {
    refused("send.lu", "os_signal_send");
    refused("ignored_at_start.lu", "os_signal_ignored");
    refused("ignore_user.lu", "os_signal_ignore");
    refused("broken_pipe.lu", "os_signal_ignore");
    for f in ["listen_shape.lu", "trap_each.lu", "poll_exact.lu"] {
        refused(f, "os_signal_listen");
    }
}

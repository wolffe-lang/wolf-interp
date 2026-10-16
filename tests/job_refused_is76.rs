//! is76 (wolf-interp#249) — a run lupin ends by refusing a call ends the
//! processes it started, and its record says where the refusal came.
//!
//! `tests/fixtures/job_refused_is76/job_refused.lu` starts a job that leads
//! its own process group (`sh -c "sleep 30; exit 0"`: the `sh` and its
//! `sleep`), prints the job's pid, then calls `os_signal_send`, which lupin
//! refuses by name (ruling #79 R8). lupin 0.1.52 and 0.1.53 left both
//! processes running in their group after they exited (s240's measurement),
//! and the record said only `unsupported` at `resolve`.
//!
//! Now: the verdict and `phase_reached` are unchanged — `resolve` is the
//! deepest phase this machine COMPLETED, which is what `[proto.record.phase]`
//! asks for (the checked machine answers `mem`, its own deepest static rung,
//! for a run-time refusal) — and the record adds `x-unsupported-at: "run"`
//! and `x-ended` (the job's pid). Nothing of the group is left: no process
//! whose group is the job's pid, read from `ps`.
//!
//! **What this test signals:** only the group lupin itself made for the job
//! this test asked it to start (SIGKILL, by lupin, on its way out). It runs
//! on a CI runner (`CI` set) or where `LUPIN_SIGNAL_TESTS=1` says the host
//! is a throwaway (the lane ran it in a podman container on kasumi);
//! anywhere else it says SKIP, loudly, and passes.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn allowed() -> bool {
    std::env::var_os("CI").is_some()
        || std::env::var("LUPIN_SIGNAL_TESTS").is_ok_and(|v| v == "1")
}

fn alone(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "is76-job-{}",
        name.trim_end_matches(".lu")
    ));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    let entry = dir.join(name);
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/job_refused_is76")
            .join(name),
        &entry,
    )
    .expect("copied");
    entry
}

/// The pids whose process group is `pgid`, from `ps` (both hosts' `ps`
/// take `-A -o pid=,pgid=`).
fn group_members(pgid: u32) -> Vec<u32> {
    let out = Command::new("ps")
        .args(["-A", "-o", "pid=,pgid="])
        .stdin(Stdio::null())
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let pid: u32 = words.next()?.parse().ok()?;
            let group: u32 = words.next()?.parse().ok()?;
            (group == pgid).then_some(pid)
        })
        .collect()
}

#[test]
fn a_refusal_ends_the_job_it_left_running() {
    if !allowed() {
        eprintln!(
            "SKIP: a_refusal_ends_the_job_it_left_running ends a process group lupin starts; \
             it runs where CI is set or LUPIN_SIGNAL_TESTS=1 (a throwaway host)"
        );
        return;
    }
    let entry = alone("job_refused.lu");
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(&entry)
        .arg("--json")
        .current_dir(entry.parent().expect("a directory"))
        .stdin(Stdio::null())
        .output()
        .expect("lupin observes");
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "one record ({e}): {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    assert_eq!(rec["verdict"], "unsupported", "{rec}");
    assert_eq!(rec["phase_reached"], "resolve", "{rec}");
    assert!(
        rec["x-unsupported"]
            .as_str()
            .is_some_and(|why| why.contains("os_signal_send")),
        "refused by name: {rec}"
    );
    assert_eq!(rec["x-unsupported-at"], "run", "{rec}");
    let ended: Vec<u32> = rec["x-ended"]
        .as_array()
        .unwrap_or_else(|| panic!("x-ended: {rec}"))
        .iter()
        .filter_map(|v| v.as_u64().and_then(|v| u32::try_from(v).ok()))
        .collect();
    assert_eq!(ended.len(), 1, "the one job: {rec}");
    let pgid = ended[0];
    // The `sleep` was its group's; SIGKILL has no handler to wait for, but
    // init reaps the orphan on its own schedule, so the look is given a
    // moment before it is a failure.
    let started = Instant::now();
    let mut left = group_members(pgid);
    while !left.is_empty() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
        left = group_members(pgid);
    }
    assert!(
        left.is_empty(),
        "the job's group {pgid} still has {left:?} after lupin exited: {rec}"
    );
    println!("ended group {pgid}; nothing of it left; record {rec}");
}

/// A run that ENDS (an exit) leaves its children as a native process does:
/// nothing is ended, and the record carries neither key.
#[test]
fn an_exit_ends_nothing() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("is76-job-exit");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    let entry = dir.join("exit.lu");
    std::fs::write(
        &entry,
        "fn main() -> !int {\n    let h = os_spawn_job(\"true\", List[str](), List[int](), 0, -1, 0)?\n    \
         let s = os_wait_status(h)?\n    print(\"{s}\")\n    0\n}\n",
    )
    .expect("written");
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(&entry)
        .arg("--json")
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .expect("lupin observes");
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a record");
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(rec["stdout_inline"], "0\n", "{rec}");
    assert!(rec.get("x-ended").is_none(), "{rec}");
    assert!(rec.get("x-unsupported-at").is_none(), "{rec}");
}

/// A refusal before the run (a static one) is not "at run".
#[test]
fn a_static_refusal_is_not_at_run() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("is76-job-static");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    let entry = dir.join("static.lu");
    std::fs::write(&entry, "fn main() {\n    let x = 1\n    print(\"{x}\")\n}\n").expect("written");
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(&entry)
        .arg("--json")
        .arg("--phase=typecheck")
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .expect("lupin observes");
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a record");
    assert_eq!(rec["verdict"], "unsupported", "{rec}");
    assert!(rec.get("x-unsupported-at").is_none(), "{rec}");
}

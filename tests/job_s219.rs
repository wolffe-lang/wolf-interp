//! s219 (wolf-lang's `[os.signal.disp]`, `[os.signal.poll]`,
//! `[os.proc.job]`, `[os.proc.status]`, `[os.term]`; wolf-lang#622, pelt's
//! H3) — the mirror, at its witnesses.
//!
//! `tests/fixtures/job_s219/` holds wolf-lang s219's corpus row
//! (`job_rows.lu`) and its no-terminal fixture (`status.lu`) byte for
//! byte, so the answers here are the ones the compiler's gate
//! (`ctrl_c_lanes.rs`) asserts of every machine.
//!
//! What this machine does NOT mirror is asserted on its words: setting a
//! disposition needs `sigaction`, handing a child the terminal needs
//! `tcsetpgrp`, the terminal's mode `tcgetattr`/`tcsetattr` and this
//! process's group `getpgrp` — none in std, and this crate forbids
//! `unsafe` (`[os.term.note]` R8). A process group for a child IS in std
//! (`CommandExt::process_group`), so that, the pid and the status are
//! served.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
        .join("tests/fixtures/job_s219")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn observe(entry: &Path, dir: &Path) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("lupin observes");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str(l.trim()).ok())
        .unwrap_or_else(|| panic!("a record: {text}"))
}

fn source(dir: &Path, text: &str) -> PathBuf {
    let p = dir.join("main.lu");
    std::fs::write(&p, text).expect("written");
    p
}

/// A program's record, from source text in a scratch directory.
fn observe_text(name: &str, text: &str) -> serde_json::Value {
    let dir = scratch(name);
    observe(&source(&dir, text), &dir)
}

#[test]
fn the_shape_rows_on_every_host() {
    let dir = scratch("s219-job_rows");
    let rec = observe(&fixture("job_rows.lu"), &dir);
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(
        rec["stdout_inline"],
        "group=invalid\ntty=invalid\ndefaults=invalid\nmap=invalid\npid=io\nstatus=io\n\
poll_empty=io\npoll_none=0\nignore_empty=io\ndefault_outside=io\n",
        "{rec}"
    );
}

#[cfg(unix)]
#[test]
fn a_group_leader_a_joiner_and_their_status() {
    let dir = scratch("s219-status");
    let rec = observe(&fixture("status.lu"), &dir);
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(
        rec["stdout_inline"], "pid>0=true a=-9 b=-9 false=1 again=io\n",
        "{rec}"
    );
}

/// Each call this machine cannot make is refused BY NAME, the call named.
#[cfg(unix)]
#[test]
fn what_needs_unsafe_is_refused_by_name() {
    for (name, body, word) in [
        ("ignore", "os_signal_ignore(16)?", "os_signal_ignore"),
        ("default", "os_signal_default(16)?", "os_signal_default"),
        ("pgid", "let _p = os_pgid()?", "os_pgid"),
        (
            "fg",
            "let _p = os_term_foreground(0)?",
            "os_term_foreground",
        ),
        (
            "setfg",
            "os_term_set_foreground(0, 1)?",
            "os_term_set_foreground",
        ),
        ("mode", "let _m = os_term_mode(0)?", "os_term_mode"),
        ("setmode", "os_term_set_mode(0, 1)?", "os_term_set_mode"),
        (
            "tty",
            "let _h = os_spawn_job(\"true\", List[str](), List[int](), 0, 0, 0)?",
            "terminal",
        ),
        (
            "defaults",
            "let _h = os_spawn_job(\"true\", List[str](), List[int](), 0, -1, 16)?",
            "dispositions",
        ),
    ] {
        let rec = observe_text(
            &format!("s219-refuse-{name}"),
            &format!("fn main() -> !int {{\n    {body}\n    0\n}}\n"),
        );
        assert_eq!(rec["verdict"], "unsupported", "{name}: {rec}");
        let why = rec["x-unsupported"].as_str().unwrap_or("");
        assert!(
            why.contains(word),
            "{name} refused for another reason: {why}"
        );
        assert!(
            why.contains("unsafe"),
            "{name}: the reason names the limit: {why}"
        );
    }
}

/// A mode's shape is read before the refusal, as on every machine.
#[test]
fn a_bad_mode_is_invalid_first() {
    let rec = observe_text(
        "s219-badmode",
        "fn main() -> !int {\n    var t = \"ok\"\n    os_term_set_mode(0, 8) else |e| {\n        \
t = match e {\n            invalid => \"invalid\",\n            io => \"io\",\n            \
unsupported => \"unsupported\",\n        }\n    }\n    print(t)\n    0\n}\n",
    );
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(rec["stdout_inline"], "invalid\n", "{rec}");
}

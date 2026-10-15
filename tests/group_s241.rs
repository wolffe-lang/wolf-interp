//! s241 (wolf-lang's `[os.signal.send.group]`, `[os.signal.send.all]`,
//! `[os.term.group]`, `[os.signal.host]`, `[os.signal.text]`;
//! wolf-lang#739, #740, #741, #762; ruling #79 R8) — the mirror, at its
//! witnesses.
//!
//! `tests/fixtures/group_s241/` holds wolf-lang s241's corpus row
//! (`group_rows.lu`) and two of its fixtures (`table.lu`, `pgid.lu`), byte
//! for byte, so the answers here are the ones the compiler's gate
//! (`group_lanes.rs`) asserts of lupin. The gate's two other programs
//! (`job.lu`, `all.lu`) start children before their first refused call,
//! so they are run only by that gate, inside its throwaway container.
//!
//! **Nothing here sends a signal or moves a process.** This machine
//! answers each call's SHAPE rows and, on windows, the `unsupported` row;
//! every well-formed call on linux and macOS is refused BY NAME before
//! any host call (lupin forbids `unsafe` and links no libc), which is
//! asserted on its words.

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
        .join("tests/fixtures/group_s241")
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

/// A program of this test's own, written to a scratch directory and
/// observed there.
fn observe_text(slug: &str, program: &str) -> serde_json::Value {
    let dir = scratch(&format!("group_s241_{slug}"));
    let entry = dir.join("main.lu");
    std::fs::write(&entry, program).expect("the program is written");
    record(&observe(&entry, &dir))
}

/// The shape rows of the six calls: the row's declared stdout, as on
/// every compiler machine (windows included: a far host number answers
/// `unsupported` there and `invalid` elsewhere, which the row allows).
#[test]
fn the_shape_rows() {
    let entry = fixture("group_rows.lu");
    let rec = record(&observe(&entry, &scratch("group_s241_rows")));
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(rec["stdout_inline"], declared_stdout(&entry), "{rec}");
}

/// `-N` in `os_signal_send`'s shape (s241): past the host's last number
/// it is `invalid` before anything else, as a group of 1 is for the group
/// send; windows has no numbers, so there a `-N` is past the shape and
/// the row is `unsupported`.
#[test]
fn a_host_number_past_the_last_is_invalid() {
    let past = if cfg!(target_os = "macos") { 32 } else { 65 };
    let program = format!(
        "fn main() -> !int {{\n    \
         os_signal_send(1, -{past}) else |e| {{\n        \
         print(match e {{ invalid => \"invalid\", unsupported => \"unsupported\", \
         not_found => \"not_found\", denied => \"denied\", io => \"io\" }})\n        \
         return 0\n    }}\n    \
         print(\"sent\")\n    0\n}}\n"
    );
    let rec = observe_text("past", &program);
    let want = if cfg!(windows) {
        "unsupported\n"
    } else {
        "invalid\n"
    };
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(rec["stdout_inline"], want, "{rec}");
}

/// Each well-formed call, past its shape: refused BY NAME on linux and
/// macOS — before any host call, so nothing is sent or moved — and the
/// `unsupported` row on windows.
#[test]
fn each_well_formed_call_is_refused_by_name() {
    for (name, call) in [
        (
            "os_signal_send_group",
            "os_signal_send_group(2147483647, 0)",
        ),
        ("os_signal_send_all", "os_signal_send_all(0)"),
        ("os_set_pgid", "os_set_pgid(0)"),
        ("os_signal_last", "let _n = os_signal_last()"),
        ("os_signal_realtime", "let _r = os_signal_realtime()"),
        ("os_signal_text", "let _t = os_signal_text(-9)"),
        ("os_signal_send", "os_signal_send(2147483647, -9)"),
    ] {
        let program = format!(
            "fn main() -> !int {{\n    {call} else |e| {{\n        \
             print(\"row\")\n        return 0\n    }}\n    print(\"answered\")\n    0\n}}\n"
        );
        let rec = observe_text(name, &program);
        if cfg!(windows) {
            assert_eq!(rec["verdict"], "exit(0)", "{name}: {rec}");
            assert_eq!(rec["stdout_inline"], "row\n", "{name}: {rec}");
            continue;
        }
        let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
        assert_eq!(rec["verdict"], "unsupported", "{name}: {rec}");
        assert!(
            why.contains(&format!("`{name}`")) && why.contains("no `unsafe`"),
            "{name}: the refusal names it and says why: {why}"
        );
    }
}

/// wolf-lang's two fixtures that start nothing before their first call:
/// the table is refused at `os_signal_last`, the move at `os_pgid` (as
/// before s241), on a unix host.
#[cfg(unix)]
#[test]
fn the_compilers_fixtures_are_refused_at_their_first_call() {
    for (file, name) in [("table.lu", "os_signal_last"), ("pgid.lu", "os_pgid")] {
        let entry = fixture(file);
        let rec = record(&observe(&entry, &scratch(&format!("group_s241_{file}"))));
        let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
        assert_eq!(rec["verdict"], "unsupported", "{file}: {rec}");
        assert!(
            why.contains(&format!("`{name}`")),
            "{file}: the refusal names `{name}`: {why}"
        );
    }
}

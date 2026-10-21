//! s234 (wolf-lang's `[os.term.size]`, `[os.term.chars]`,
//! `[os.term.mode]`'s FLOW and EXTENDED, `[os.signal.wait.fd]`;
//! wolf-lang#719, W05) — the mirror, at its witnesses.
//!
//! The programs under `tests/fixtures/term_editor_s234/` are wolf-lang
//! s234's corpus row (`term_editor_rows.lu`) and its three pseudo-terminal
//! fixtures, byte for byte, so the answers here are the ones the
//! compiler's gate (`term_editor_lanes.rs`) asserts of every machine. Each
//! runs under `lupin conform-run --json` in a scratch directory of its
//! own.
//!
//! What this machine serves: every SHAPE row — a set the shape rule
//! refuses is `io` for the wait, a mode with a bit that is no field (bit
//! 24 and above, since s248 made bits 5..7 fields — is79 re-synced the
//! row to wolf-lang `9da7bdcf`, sha256 `811f1fc5…`, where bit 5's row has
//! left for `editor_wait_lanes.rs`) or a negative one is `invalid`. What it
//! does NOT mirror is asserted on its words: lupin forbids `unsafe` and
//! links no libc, and std has no `ioctl`, `tcgetattr` or `poll`, so the
//! window's size, the special characters and the wait itself are refused
//! BY NAME (`[os.term.editor.note]` R6). Each fixture makes its s234 call
//! before it opens the terminal, so the refusal names that call.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(dir.join("target")).expect("scratch created");
    dir
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/term_editor_s234")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// `lupin conform-run <entry> --json` in `dir`, standard input the null
/// device.
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

/// The shape rows: the row's declared stdout, as on every compiler
/// machine (on windows too: each row is decided before the host).
#[test]
fn the_terminal_rows() {
    let entry = fixture("term_editor_rows.lu");
    let rec = record(&observe(&entry, &scratch("term_editor_s234_rows")));
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(rec["stdout_inline"], declared_stdout(&entry), "{rec}");
}

/// A fixture this machine declines by `name`: `unsupported`, and the
/// refusal names it (and, on unix, says why: no `unsafe`).
fn refused(fixture_name: &str, name: &str) {
    let entry = fixture(fixture_name);
    let dir = scratch(&format!(
        "term_editor_s234_{}",
        fixture_name.replace('.', "_")
    ));
    let out = observe(&entry, &dir);
    let rec = record(&out);
    let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
    if cfg!(windows) {
        // The shape passes, so windows answers the declared
        // `unsupported` ROW — carried out of `main` by the fixture's `?`
        // (exit 1) — or, where the fixture reaches a call this machine
        // declines on every host first (`os_signal_listen`), that
        // refusal. Never a pass.
        let v = rec["verdict"].as_str().unwrap_or("");
        assert!(
            v == "exit(1)" || v == "unsupported",
            "{fixture_name} on windows: {rec}"
        );
        eprintln!("windows {fixture_name}: {v} {why}");
        return;
    }
    assert_eq!(rec["verdict"], "unsupported", "{fixture_name}: {rec}");
    assert!(
        why.contains(&format!("`{name}`")) && why.contains("no `unsafe`"),
        "{fixture_name}: the refusal names `{name}` and its reason: {why}"
    );
}

/// The window's size is `TIOCGWINSZ`: refused by name.
#[test]
fn the_size_is_refused_by_name() {
    refused("size.lu", "os_term_size");
}

/// The wait needs `poll(2)` and a listen this machine never makes:
/// refused by name after its shape.
#[test]
fn the_wait_is_refused_by_name() {
    refused("wait_key.lu", "os_signal_wait_fd");
}

/// The special characters are `tcgetattr`'s: refused by name.
#[test]
fn the_characters_are_refused_by_name() {
    refused("chars.lu", "os_term_chars");
}

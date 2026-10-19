//! is77 (wolf-interp#262; wolf-lang s246, #787; `[os.signal.text]`) —
//! signal 0's words, the mirror at its witness.
//!
//! `tests/fixtures/signal_text_is77/text_zero.lu` is wolf-lang
//! `98d0d16b`'s `crates/wolf_driver/tests/fixtures/signal_gaps/text_zero.lu`
//! byte for byte (sha256 `e59e09f9…`), the per-machine witness s246 moved
//! the row to when 0 stopped being a shape row.
//!
//! **What the compiler answers, measured:** the checked machine of
//! wolf-lang `98d0d16b` prints `text_zero=Unknown signal 0` in a throwaway
//! ubuntu:24.04 container on kasumi (glibc's `strsignal(0)`; is77,
//! `runs/m1.txt`), and macOS's words are `Unknown signal: 0` (CI's runner,
//! s246, run 39215738496). wolf 0.2.32 and lupin 0.1.54 answer
//! `text_zero=invalid` (0 was outside the shape): lupin 0.1.54's archive on
//! kasumi and on macOS 26.4.1 (arm64), is77.
//!
//! **What this machine answers, and why** (`docs/approximation-contract.md`
//! §6.20): 0 passes the shape now, as on every compiler machine; windows
//! answers the `unsupported` row after it; on linux and macOS the call is
//! refused BY NAME, exactly as for every other meaning: the words are the
//! host C library's `strsignal`, std has none, and this machine has no
//! `unsafe` to call one. A table of this machine's own words for 0 would
//! be a guess at the host (glibc, musl and macOS each word it
//! differently), so none is kept.

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

fn record(out: &Output) -> serde_json::Value {
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .filter(|r| r.get("verdict").is_some())
        .unwrap_or_else(|| panic!("a record: {out:?}"))
}

/// The compiler's witness: 0 is in the shape — never `invalid` — then
/// `unsupported` on windows, and on linux and macOS a refusal by name of
/// `os_signal_text` that says why. Red at 0.1.54: `text_zero=invalid`.
#[test]
fn signal_zero_passes_the_shape_and_is_refused_by_name() {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/signal_text_is77/text_zero.lu");
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(&entry)
        .arg("--json")
        .current_dir(scratch("signal_text_is77_zero"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("lupin observes");
    let rec = record(&out);
    if cfg!(windows) {
        assert_eq!(rec["verdict"], "exit(0)", "{rec}");
        assert_eq!(rec["stdout_inline"], "text_zero=unsupported\n", "{rec}");
        return;
    }
    let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
    assert_eq!(rec["verdict"], "unsupported", "0 passes the shape: {rec}");
    assert!(
        why.contains("`os_signal_text`") && why.contains("strsignal") && why.contains("no `unsafe`"),
        "the refusal names `os_signal_text` and says why: {why}"
    );
}

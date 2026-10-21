//! is79 (wolf-interp#272; wolf-lang s252, #814, yew G25; `[mem.static.3]`)
//! — module state of a `wrapping[T]` type is static data: "the integers"
//! include it, as they do everywhere else.
//!
//! `tests/fixtures/small_gaps_is79/static_wrapping.lu` is wolf-lang s252's
//! row (branch `s252` at `50e8d0f8`, PR #824) byte for byte (sha256
//! `4bfae37d…`): a `const` of `wrapping[u64]` past `i64::MAX`, an unread
//! one, a `let` of `wrapping[u32]`, a `const` that folds with its type's
//! wrap (`200 + 100` at `u8` is `44`), a `wrapping[i8]` at its minimum and a
//! `var` of `wrapping[u32]` stepped past its top inside `unsafe`. s252's
//! build answers `63 0 44 127 1` on checked and native (is79 `runs/m4/`;
//! s252's gate `small_gaps_lanes.rs` on all three); wolf 0.2.34 and lupin
//! 0.1.55 refused the package by name, "module state of a type that is not
//! static data". A `wrapping` item never faults at compile time — its
//! operators wrap — so the E0706 fold (is76) says nothing about one.

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

/// One of s252's corpus rows, byte for byte.
fn row(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/small_gaps_is79")
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

/// What a row's `//! check:` header declares: `exit(0)` and its stdout for
/// a run row, `fail(CODE)` for a refusal.
fn declared(entry: &Path) -> (String, Option<String>) {
    let text = std::fs::read_to_string(entry).expect("the row reads");
    let line = text
        .lines()
        .find(|l| l.starts_with("//! check: "))
        .unwrap_or_else(|| panic!("{} declares a check", entry.display()));
    if let Some(start) = line.find("stdout=\"") {
        let start = start + "stdout=\"".len();
        let end = line.rfind("\")").expect("the directive closes");
        return (
            "exit(0)".to_owned(),
            Some(line[start..end].replace("\\n", "\n")),
        );
    }
    let code = line
        .trim_start_matches("//! check: ")
        .trim()
        .to_owned();
    (code, None)
}

/// The row `name`, observed in a scratch directory of its own.
fn observe_row(name: &str) -> serde_json::Value {
    let dir = scratch(&format!("{}_{}", module_path!(), name.replace('.', "_")));
    record(&observe(&row(name), &dir))
}

/// A program of this test's own, written to a scratch directory of its
/// own (a directory is a module) and observed there.
fn observe_text(slug: &str, program: &str) -> serde_json::Value {
    let dir = scratch(&format!("{}_{slug}", module_path!()));
    let entry = dir.join("main.lu");
    std::fs::write(&entry, program).expect("the program is written");
    record(&observe(&entry, &dir))
}

fn verdict(rec: &serde_json::Value) -> String {
    rec["verdict"].as_str().unwrap_or("").to_owned()
}

fn stdout(rec: &serde_json::Value) -> String {
    rec["stdout_inline"].as_str().unwrap_or("").to_owned()
}

/// The row runs as its header says the compiling machines do (s252's
/// `small_gaps_lanes.rs` asserts the same of checked, native and release).
fn runs_as_declared(name: &str) {
    let (want, out) = declared(&row(name));
    let rec = observe_row(name);
    assert_eq!(verdict(&rec), want, "{name}: {rec}");
    if let Some(out) = out {
        assert_eq!(stdout(&rec), out, "{name}: {rec}");
    }
}

#[test]
fn wrapping_module_state_is_static_data() {
    runs_as_declared("static_wrapping.lu");
}

/// What stays refused by name: module state of a type that is still not
/// static data (a `List`), and a `var` of `str`.
#[test]
fn the_rest_of_module_state_is_still_declined_by_name() {
    for (slug, program) in [
        (
            "list",
            "let XS: List[int] = List[int]()\n\nfn main() {\n    print(\"{XS.len}\")\n}\n",
        ),
        (
            "str_var",
            "var S: str = \"a\"\n\nfn main() {\n    unsafe {\n        print(S)\n    }\n}\n",
        ),
        (
            "wrapping_list",
            "let W: List[wrapping[u8]] = List[wrapping[u8]]()\n\nfn main() {\n    print(\"{W.len}\")\n}\n",
        ),
    ] {
        let rec = observe_text(slug, program);
        assert_eq!(verdict(&rec), "unsupported", "{slug}: {rec}");
        assert!(
            rec["x-unsupported"]
                .as_str()
                .unwrap_or("")
                .contains("not static data ([mem.static.3])"),
            "{slug}: {rec}"
        );
    }
}

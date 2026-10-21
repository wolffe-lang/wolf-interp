//! is79 (wolf-interp#271; wolf-lang s252, #816, #801, #822's first finding;
//! `[mem.region.scalar]`) — a value whose type reaches no storage names no
//! region: a scalar, unit, an adapter of one, and a tuple, struct, enum or
//! row made only of them. Built in a region and held outside, stored into
//! another parameter's region, passed `mut` inside one, bound by a `for`,
//! or read out of a frozen region, it is its own inline words.
//!
//! The rows under `tests/fixtures/small_gaps_is79/` are wolf-lang s252's
//! (branch `s252` at `50e8d0f8`, PR #824) byte for byte (sha256 `d9c88df5…`
//! region_scalar_value, `743b10d3…` _mut, `bbf8a4a3…` _store, `a1e0db7e…`
//! _freeze, `7e355327…` _str_escape, `01d3b7de…` _list_store, `dd4e27f2…`
//! _list_mut, and s252's List-holding `cd3043ca…` region_conflict_params,
//! `7c4cd883…` region_escape_local, `8877d291…` region_freeze_write). s252's
//! build answers each run row's header on checked and native (is79
//! `runs/m4/`; its gate `small_gaps_lanes.rs` on all three). lupin 0.1.55
//! trapped `region-fault` on `_value`, `_store` (at `stash`) and `_freeze`.
//!
//! This machine is dynamic, so a region refusal is the fault at the access
//! (`[conf.trap.map]`: E1004 and E1010 are `region-fault` here): what still
//! faults is storage — a struct with a built `str` field held past its
//! region. The two List rows the compilers refuse statically (`fail(E1004)`,
//! `fail(E1010)`: "never unsound, sometimes strict", s252) run here as at
//! 0.1.55: this machine tracks the list itself, and the list is the
//! caller's.

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
fn a_scalar_only_value_leaves_its_region_like_an_int() {
    runs_as_declared("region_scalar_value.lu");
}

#[test]
fn a_scalar_only_mut_argument_gains_nothing_from_its_region() {
    runs_as_declared("region_scalar_mut.lu");
}

#[test]
fn a_scalar_only_store_into_another_parameters_region_ties_nothing() {
    runs_as_declared("region_scalar_store.lu");
}

#[test]
fn a_scalar_only_value_read_out_of_a_frozen_region_is_the_bindings_own() {
    runs_as_declared("region_scalar_freeze.lu");
}

/// What still faults: storage. A struct with a built `str` field held past
/// its region, and s252's List-holding rewrites of the three rows whose
/// scalar-only stand-ins the rule now admits.
#[test]
fn storage_still_faults_at_the_access() {
    for name in [
        "region_scalar_str_escape.lu",
        "region_conflict_params.lu",
        "region_escape_local.lu",
        "region_freeze_write.lu",
    ] {
        let rec = observe_row(name);
        assert_eq!(verdict(&rec), "trap(region-fault)", "{name}: {rec}");
    }
}

/// The compilers' static conservatism over a list this machine tracks
/// exactly: the list is the caller's, so the run is clean here, as it was
/// at 0.1.55.
#[test]
fn a_list_this_machine_tracks_is_answered_as_before() {
    for name in ["region_scalar_list_store.lu", "region_scalar_list_mut.lu"] {
        let rec = observe_row(name);
        assert_eq!(
            (verdict(&rec), stdout(&rec)),
            ("exit(0)".to_owned(), "1\n".to_owned()),
            "{name}: {rec}"
        );
    }
}

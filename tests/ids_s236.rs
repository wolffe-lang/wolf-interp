//! s236 (wolf-lang's `[os.proc.ids]`, `[os.proc.ids.set]`,
//! `[os.fs.owner]`; wolf-lang#633, #701, #742) — the mirror, at its
//! witnesses.
//!
//! The programs under `tests/fixtures/ids_s236/` are wolf-lang s236's two
//! corpus rows (`ids_rows.lu`, `ids_read.lu`) and its fixtures, byte for
//! byte, so the answers here are the ones the compiler's gate
//! (`ids_lanes.rs`) asserts of every machine; `set_ids_refused.lu` is
//! this repository's own. Each runs under `lupin conform-run --json` in a
//! scratch directory of its own.
//!
//! **Nothing here changes an id or an owner.** What this machine serves
//! is the two owner calls — the standard library's `chown`, `lchown` and
//! `fchown` — and the tests ask them only for changes that change
//! nothing (`-1 -1`, the owner a file already has) and for refusals.
//! What it does NOT mirror is asserted on its words: the five reads and
//! `os_set_ids` are refused BY NAME on a host that has ids (lupin forbids
//! `unsafe` and links no libc; `[os.proc.ids.note]` R10). wolf-lang's
//! three container programs (`drop.lu`, `refused.lu`, `owner.lu`) ask who
//! they are before anything else, so here each is refused at that
//! question and never starts.

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
        .join("tests/fixtures/ids_s236")
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

fn slug(name: &str) -> String {
    format!("ids_s236_{}", name.replace('.', "_"))
}

/// The shape rows and the owner calls that change nothing: the row's
/// declared stdout, as on every compiler machine — `ok` where the host
/// has owners, `unsupported` where it has none (windows).
#[test]
fn the_rows_that_change_nothing() {
    let entry = fixture("ids_rows.lu");
    let rec = record(&observe(&entry, &scratch(&slug("ids_rows.lu"))));
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(rec["stdout_inline"], declared_stdout(&entry), "{rec}");
}

/// A fixture this machine declines at its first id call, `name`:
/// `unsupported`, and the refusal names the call and says why.
#[cfg(unix)]
fn refused(fixture_name: &str, name: &str) {
    let entry = fixture(fixture_name);
    let rec = record(&observe(&entry, &scratch(&slug(fixture_name))));
    let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
    assert_eq!(rec["verdict"], "unsupported", "{fixture_name}: {rec}");
    assert!(
        why.contains(&format!("`{name}`")) && why.contains("no `unsafe`"),
        "{fixture_name}: the refusal names `{name}` and its reason: {why}"
    );
}

/// Who the process runs as. On a host that has ids the read is refused
/// by name; windows has none and answers the `unsupported` row, which
/// the row's own stdout allows for — so there it runs as declared.
#[test]
fn who_it_runs_as() {
    #[cfg(unix)]
    {
        refused("ids_read.lu", "os_uid");
        refused("who.lu", "os_uid");
    }
    #[cfg(not(unix))]
    {
        let entry = fixture("ids_read.lu");
        let rec = record(&observe(&entry, &scratch(&slug("ids_read.lu"))));
        assert_eq!(rec["verdict"], "exit(0)", "{rec}");
        assert_eq!(rec["stdout_inline"], declared_stdout(&entry), "{rec}");
    }
}

/// This test process's effective user id, by the host's own `id`.
#[cfg(unix)]
fn host_uid() -> String {
    let out = Command::new("id").arg("-u").output().expect("id runs");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The owner calls on a host, each served: both ids left alone, the
/// owner the file already has (by path, on a link itself, on a handle),
/// a missing path, a dangling link followed and not, a closed handle,
/// and a file that may not be given to the super-user. The program
/// removes its tree; no owner changed.
#[cfg(unix)]
#[test]
fn owner_calls_change_nothing_on_a_host() {
    let to_root = if host_uid() == "0" {
        "to root: not asked (this is the super-user)"
    } else {
        "to root: denied errno=1 by handle=denied"
    };
    let want = format!(
        "leave: ok same: ok a link itself: ok\n\
missing: not_found errno=2 dangling: followed=not_found itself=ok\n\
by handle: leave=ok same=ok closed=io errno=0\n\
{to_root}\n\
owner unchanged: true\n"
    );
    let dir = scratch(&slug("owner_host.lu"));
    let rec = record(&observe(&fixture("owner_host.lu"), &dir));
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(rec["stdout_inline"], want.as_str(), "{rec}");
    assert!(!dir.join("h").exists(), "the program removed its tree");
}

/// The drop asked by a user: the program asks who it is first, which
/// this machine refuses by name.
#[cfg(unix)]
#[test]
fn the_drop_asked_on_a_host_is_refused_at_who_it_is() {
    refused("set_host.lu", "os_euid");
}

/// wolf-lang's container programs never start here: each asks
/// `os_euid` before it touches anything.
#[cfg(unix)]
#[test]
fn the_container_programs_never_start() {
    for name in ["drop.lu", "refused.lu", "owner.lu"] {
        refused(name, "os_euid");
    }
}

/// A well-formed `os_set_ids`, past its shape: refused by name on linux
/// (no `setgroups`, `setresgid` or `setresuid` without `unsafe`), and
/// the `unsupported` row on every other host — the compiler's own
/// answer there. Nothing is carried out on any host.
#[test]
fn a_well_formed_drop_is_refused_or_unsupported() {
    let entry = fixture("set_ids_refused.lu");
    let rec = record(&observe(&entry, &scratch(&slug("set_ids_refused.lu"))));
    if cfg!(target_os = "linux") {
        let why = rec["x-unsupported"].as_str().unwrap_or("");
        assert_eq!(rec["verdict"], "unsupported", "{rec}");
        assert!(
            why.contains("`os_set_ids`") && why.contains("no `unsafe`"),
            "the refusal names `os_set_ids` and its reason: {why}"
        );
    } else {
        assert_eq!(rec["verdict"], "exit(0)", "{rec}");
        assert_eq!(rec["stdout_inline"], "set ids: unsupported\n", "{rec}");
    }
}

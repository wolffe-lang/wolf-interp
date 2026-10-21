//! is79 (wolf-interp#274; seen by wolf-lang s252 measuring
//! `[mem.tier0.move.3]`) — the adapter casts: `type A = distinct B` has
//! B's layout (D28), and since s252 B's copy semantics when B is a Copy
//! scalar; an adapter of `str`, a container or a struct moves.
//!
//! Three gaps lupin 0.1.55 had, each with its witness below, measured
//! first on wolf 0.2.34's archive and on s252's build (`50e8d0f8`), which
//! agree on all of them (is79 `runs/m3/`, `runs/m4/`):
//!
//! 1. a cast out of an adapter of a non-Copy base did not move it: `let x =
//!    a as P` twice ran (`exit(0)`) where the compilers are `fail(E1001)` —
//!    here the second read traps `use-after-move`, E1001's dynamic meaning
//!    (`[conf.trap.map]`);
//! 2. `as bool` on an adapter of `bool` was E0805 — it runs (`ready not
//!    ready`);
//! 3. a cast TO an adapter of `str` was E0805 — it runs (`3 3`), and the
//!    adapter of `str` moves at its cast out, as 1 says.
//!
//! s252's two adapter rows are here byte for byte too (sha256 `86f8ff31…`
//! distinct_copy_scalar, `83bab917…` distinct_container_moves): 0.1.55
//! already answered them.

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

/// (1) A cast out of an adapter of a `List`, a struct and a `str` moves the
/// adapter: the second cast reads a moved place.
#[test]
fn a_cast_out_of_an_adapter_of_a_non_copy_base_moves_it() {
    for (slug, program) in [
        (
            "list",
            "type Ids = distinct List[int]

fn twice(a: Ids) -> int {
    let x = a as List[int]
    let y = a as List[int]
    x.len + y.len
}

fn main() {
    var xs = List[int]()
    (mut xs).push(1)
    print(\"{twice(xs as Ids)}\")
}
",
        ),
        (
            "struct",
            "struct P {
    x: int,
}

type Q = distinct P

fn twice(a: Q) -> int {
    let x = a as P
    let y = a as P
    x.x + y.x
}

fn main() {
    print(\"{twice(P { x: 3 } as Q)}\")
}
",
        ),
        (
            "str",
            "type Name = distinct str

fn twice(a: Name) -> int {
    let x = a as str
    let y = a as str
    x.len + y.len
}

fn main() {
    print(\"{twice(\"ab\" as Name)}\")
}
",
        ),
    ] {
        let rec = observe_text(slug, program);
        assert_eq!(verdict(&rec), "trap(use-after-move)", "{slug}: {rec}");
    }
}

/// (1)'s controls: one cast out of an adapter of a `List` is its list, and
/// an adapter of a Copy scalar copies (s252's row).
#[test]
fn one_cast_out_and_a_copy_scalar_adapter_still_run() {
    let rec = observe_text(
        "once",
        "type Ids = distinct List[int]

fn once(a: Ids) -> int {
    let x = a as List[int]
    x.len
}

fn main() {
    var xs = List[int]()
    (mut xs).push(1)
    (mut xs).push(2)
    print(\"{once(xs as Ids)}\")
}
",
    );
    assert_eq!((verdict(&rec), stdout(&rec)), ("exit(0)".to_owned(), "2\n".to_owned()), "{rec}");
    runs_as_declared("distinct_copy_scalar.lu");
    let rec = observe_row("distinct_container_moves.lu");
    assert_eq!(verdict(&rec), "trap(use-after-move)", "{rec}");
}

/// (2) `as bool` on an adapter of `bool`, both ways.
#[test]
fn as_bool_on_an_adapter_of_bool_runs() {
    let rec = observe_text(
        "bool",
        "type Ready = distinct bool

fn show(r: Ready) -> str {
    if r as bool { \"ready\" } else { \"not\" }
}

fn main() {
    let a = true as Ready
    let b = false as Ready
    print(\"{show(a)} {show(b)} {show(a)}\")
}
",
    );
    assert_eq!(
        (verdict(&rec), stdout(&rec)),
        ("exit(0)".to_owned(), "ready not ready\n".to_owned()),
        "{rec}"
    );
}

/// (3) A cast to an adapter of `str`, from a literal and from a built
/// `str`, and back.
#[test]
fn a_cast_to_an_adapter_of_str_runs() {
    let rec = observe_text(
        "str_cast",
        "type Name = distinct str

fn len_of(n: Name) -> int {
    (n as str).len
}

fn main() {
    let n = \"abc\" as Name
    let s = \"de\" + \"f\"
    let m = s as Name
    print(\"{len_of(n)} {len_of(m)}\")
}
",
    );
    assert_eq!(
        (verdict(&rec), stdout(&rec)),
        ("exit(0)".to_owned(), "3 3\n".to_owned()),
        "{rec}"
    );
}

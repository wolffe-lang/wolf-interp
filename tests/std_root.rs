//! The std root (issue #6, wolf-std F-0010), and the module-tree provenance
//! regressions that ride the same staging (issue #7, wolf-std F-0013).
//!
//! `--std-root DIR` / `LUPIN_STD` resolve `use std.X[.Y]` against
//! `<DIR>/X[/Y]/`, mirroring the compiler's s26 `--std-root`/`WOLF_STD`
//! loader. These are process-level tests because the flag and the
//! environment variable are process surfaces; the trees are staged under
//! `target/` so a run leaves the checkout clean (the doc-truth rule).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

fn write(root: &Path, relative: &str, source: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
    std::fs::write(path, source).expect("written");
}

fn lupin(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lupin"));
    cmd.args(args);
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.output().expect("lupin runs")
}

fn record(output: &Output) -> serde_json::Value {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    serde_json::from_str(std::str::from_utf8(&output.stdout).expect("utf-8").trim())
        .expect("one JSON record")
}

/// The tree the wolf-std rig stages: nested modules included
/// (`std/x/deque_int` ships at that depth and the compiler resolves it).
fn stage_std(dir: &Path) -> PathBuf {
    let root = dir.join("std");
    write(
        &root,
        "prelude/prelude.lu",
        "pub fn least(a: int, b: int) -> int {\n    if a < b { a } else { b }\n}\n",
    );
    write(
        &root,
        "x/deque_int/deque_int.lu",
        "pub fn twice(n: int) -> int { n * 2 }\n",
    );
    root
}

const ENTRY: &str = "\
use std.prelude
use std.x.deque_int

fn main() -> !int {
    let a = prelude.least(3, 7)
    let b = deque_int.twice(a)
    print(\"a {a} b {b}\")
    0
}
";

#[test]
fn the_flag_resolves_flat_and_nested_std_paths() {
    let dir = scratch("std-root-flag");
    let root = stage_std(&dir);
    write(&dir, "pkg/main.lu", ENTRY);
    let entry = dir.join("pkg/main.lu");
    let value = record(&lupin(
        &[
            "conform-run",
            entry.to_str().expect("utf-8 path"),
            "--std-root",
            root.to_str().expect("utf-8 path"),
            "--json",
        ],
        &[],
    ));
    assert_eq!(value["phase_reached"], "run", "{value}");
    assert_eq!(value["verdict"], "exit(0)", "{value}");
    assert_eq!(value["stdout_inline"], "a 3 b 6\n", "{value}");
}

#[test]
fn the_environment_variable_is_the_flagless_spelling() {
    let dir = scratch("std-root-env");
    let root = stage_std(&dir);
    write(&dir, "pkg/main.lu", ENTRY);
    let entry = dir.join("pkg/main.lu");
    let output = lupin(
        &["run", entry.to_str().expect("utf-8 path")],
        &[("LUPIN_STD", root.to_str().expect("utf-8 path"))],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        std::str::from_utf8(&output.stdout).expect("utf-8"),
        "a 3 b 6\n"
    );
}

#[test]
fn without_a_root_the_answer_stays_the_honest_unsupported() {
    // The pre-#6 behavior is the fallback, not an error: no std root means
    // `use std.prelude` binds nothing and the call site reports itself.
    let dir = scratch("std-root-none");
    let _root = stage_std(&dir);
    write(&dir, "pkg/main.lu", ENTRY);
    let entry = dir.join("pkg/main.lu");
    let value = record(&lupin(
        &["conform-run", entry.to_str().expect("utf-8 path"), "--json"],
        &[],
    ));
    assert_eq!(value["phase_reached"], "resolve", "{value}");
    assert_eq!(value["verdict"], "unsupported", "{value}");
}

// -- issue #7: the false-`ub(mem.ub)` pair, staged as filed ----------------

/// Shape (a): a `mut` argument inside an f-string interpolation, then a
/// second `mut` call whose parameter name differs, then read-mode calls
/// whose parameter name collides with the first call's. Before 0.1.2 the
/// stale callee binding (`drop_frame` never dropped it) resolved `len`'s
/// read through the first call's Disabled tag: `ub(mem.ub) §7/P1: read
/// through tag#… (t0:1:xs), which is Disabled` — wolf-std F-0013's exact
/// message. Ordinary sequential borrows; the program is defined.
#[test]
fn a_mut_argument_inside_an_interpolation_leaves_no_stale_tag() {
    let dir = scratch("prov-interp-mut");
    let root = dir.join("std");
    write(
        &root,
        "m/m.lu",
        "pub fn len[T](xs: List[T]) -> int {\n\
         \x20   var n = 0\n\
         \x20   for _x in xs { n = n + 1 }\n\
         \x20   n\n\
         }\n\
         \n\
         pub fn nth[T](xs: List[T], i: int) -> !T {\n\
         \x20   var n = 0\n\
         \x20   for x in xs {\n\
         \x20       if n == i { return x }\n\
         \x20       n = n + 1\n\
         \x20   }\n\
         \x20   OutOfBounds\n\
         }\n\
         \n\
         pub fn pop[T](mut xs: List[T]) -> !T {\n\
         \x20   let count = len(xs)\n\
         \x20   if count == 0 { return Empty }\n\
         \x20   let last = nth(xs, count - 1)?\n\
         \x20   var rebuilt = List[T]()\n\
         \x20   var n = 0\n\
         \x20   for x in xs {\n\
         \x20       if n < count - 1 { (mut rebuilt).push(x) }\n\
         \x20       n = n + 1\n\
         \x20   }\n\
         \x20   xs = rebuilt\n\
         \x20   last\n\
         }\n\
         \n\
         pub fn reverse[T](mut items: List[T]) {\n\
         \x20   var rebuilt = List[T]()\n\
         \x20   var n = len(items)\n\
         \x20   while n > 0 {\n\
         \x20       (mut rebuilt).push(nth(items, n - 1) else |_| { return })\n\
         \x20       n = n - 1\n\
         \x20   }\n\
         \x20   items = rebuilt\n\
         }\n\
         \n\
         pub fn last[T](xs: List[T]) -> !T {\n\
         \x20   nth(xs, len(xs) - 1)\n\
         }\n",
    );
    write(
        &dir,
        "pkg/main.lu",
        "use std.m\n\
         fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(1)\n\
         \x20   print(\"popped {m.pop(mut xs) else 0}\")\n\
         \x20   m.reverse(mut xs)\n\
         \x20   print(\"len {m.len(xs)} last {m.last(xs) else 0}\")\n\
         \x20   0\n\
         }\n",
    );
    let entry = dir.join("pkg/main.lu");
    let value = record(&lupin(
        &[
            "conform-run",
            entry.to_str().expect("utf-8 path"),
            "--std-root",
            root.to_str().expect("utf-8 path"),
            "--json",
        ],
        &[],
    ));
    assert_eq!(value["verdict"], "exit(0)", "{value}");
    assert_eq!(
        value["stdout_inline"], "popped 1\nlen 0 last 0\n",
        "{value}"
    );
}

/// Shape (b): a `mut`-mode call followed by a read-mode call whose body
/// allocates. Before 0.1.2 the stale binding poisoned the read call's
/// receiver retag: `foreign write … while tag#… is PROTECTED for a call's
/// extent` — F-0013's second message. Also defined.
#[test]
fn a_read_call_that_allocates_after_a_mut_call_is_not_a_foreign_write() {
    let dir = scratch("prov-mut-then-read");
    let root = dir.join("std");
    write(
        &root,
        "map/map.lu",
        "pub fn set[K, V](mut m: Map[K, V], k: K, v: V) {\n\
         \x20   m[k] = v\n\
         }\n\
         \n\
         pub fn keys[K, V](m: Map[K, V]) -> List[K] {\n\
         \x20   var out = List[K]()\n\
         \x20   for pair in m.pairs() { (mut out).push(pair.0) }\n\
         \x20   out\n\
         }\n",
    );
    write(
        &dir,
        "pkg/main.lu",
        "use std.map\n\
         fn main() -> !int {\n\
         \x20   var m = Map[str, int]()\n\
         \x20   map.set(mut m, \"a\", 1)\n\
         \x20   let ks = map.keys(m)\n\
         \x20   ks.len() - 1\n\
         }\n",
    );
    let entry = dir.join("pkg/main.lu");
    let value = record(&lupin(
        &[
            "conform-run",
            entry.to_str().expect("utf-8 path"),
            "--std-root",
            root.to_str().expect("utf-8 path"),
            "--json",
        ],
        &[],
    ));
    assert_eq!(value["verdict"], "exit(0)", "{value}");
}

// -- #39: module identity is the full path ----------------------------------

/// The leaf_twins shape, staged from scratch: two modules whose leaf is
/// `float` coexist because identity is the FULL dotted path — `use
/// fmt.float` is `<root>/fmt/float` and `use math.float as mfloat` is
/// `<root>/math/float` — and the fmt twin itself imports the math twin
/// (same leaf on both sides of the import; the shape #39's original filing
/// read back as a self-cycle).
#[test]
fn two_modules_with_the_same_leaf_resolve_by_their_full_paths() {
    let dir = scratch("leaf-twins");
    write(
        &dir,
        "math/float/float.lu",
        "pub fn probe(v: int) -> int { v + 2 }\n",
    );
    write(
        &dir,
        "fmt/float/float.lu",
        "use math.float as mf\n\npub fn probe(v: int) -> int { mf.probe(v) + 10 }\n",
    );
    write(
        &dir,
        "main.lu",
        "use fmt.float\nuse math.float as mfloat\n\nfn main() -> !int {\n    print(\"{float.probe(1)} {mfloat.probe(1)}\")\n    if float.probe(1) == 13 && mfloat.probe(1) == 3 { 0 } else { 1 }\n}\n",
    );
    let entry = dir.join("main.lu");
    let output = lupin(
        &["conform-run", entry.to_str().expect("utf-8"), "--json"],
        &[],
    );
    let record = record(&output);
    assert_eq!(record["verdict"], "exit(0)", "{record}");
    assert_eq!(record["stdout_inline"], "13 3\n", "{record}");
}

/// #39's minimum ask, delivered as the maximum: the silent duplicate-leaf
/// single-binding is GONE. Two imports that want the same bound name for
/// different directories are an honest E0306 naming both paths and the
/// `use … as` fix — never the first-wins silent misresolution.
#[test]
fn a_duplicate_leaf_binding_is_an_honest_error_not_a_silent_first_wins() {
    let dir = scratch("leaf-collision");
    write(
        &dir,
        "math/float/float.lu",
        "pub fn probe(v: int) -> int { v + 2 }\n",
    );
    write(
        &dir,
        "fmt/float/float.lu",
        "pub fn probe(v: int) -> int { v + 12 }\n",
    );
    write(
        &dir,
        "main.lu",
        "use fmt.float\nuse math.float\n\nfn main() -> !int {\n    if float.probe(1) == 13 { 0 } else { 1 }\n}\n",
    );
    let entry = dir.join("main.lu");
    let output = lupin(
        &["conform-run", entry.to_str().expect("utf-8"), "--json"],
        &[],
    );
    let record = record(&output);
    assert_eq!(record["verdict"], "fail(E0306)", "{record}");
}

/// The flat fallback stays: a single-segment `use` still binds the sibling
/// directory by its own name, and the same module imported from two files
/// under the same path is the ordinary legal case, never a collision.
#[test]
fn the_flat_fallback_and_repeated_same_path_imports_stay_legal() {
    let dir = scratch("flat-fallback");
    write(&dir, "util/util.lu", "pub fn one() -> int { 1 }\n");
    write(
        &dir,
        "other/other.lu",
        "use util\n\npub fn two() -> int { util.one() + 1 }\n",
    );
    write(
        &dir,
        "main.lu",
        "use util\nuse other\n\nfn main() -> !int {\n    if util.one() + other.two() == 3 { 0 } else { 1 }\n}\n",
    );
    let entry = dir.join("main.lu");
    let output = lupin(
        &["conform-run", entry.to_str().expect("utf-8"), "--json"],
        &[],
    );
    let record = record(&output);
    assert_eq!(record["verdict"], "exit(0)", "{record}");
}

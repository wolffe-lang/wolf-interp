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

// -- wolf-interp#96 / #97: a trait reached through its module ---------------

/// wolf-std sc44's twelve-line reduction: `std/tiny/tiny.lu` declares
/// `Eq` and `impl Eq for str`; the entry dispatches `tiny.Eq.eq(a, b)`
/// under `[K: tiny.Eq]`. lupin 0.1.33 answered `unsupported` at resolve
/// ("`Eq` is a trait; … no dynamic semantics here") for the imported
/// trait while the byte-identical trait in the entry file ran — one trait,
/// two verdicts, decided by which FILE declared it (#96). Both wolf tiers
/// print `true false`.
#[test]
fn a_trait_reached_through_its_module_dispatches_like_a_local_one() {
    let dir = scratch("std-root-imported-trait");
    let root = dir.join("std");
    write(
        &root,
        "tiny/tiny.lu",
        "//! member: true\npub trait Eq {\n    fn eq(self, other: Self) -> bool\n}\n\n\
         impl Eq for str {\n    fn eq(self, other: Self) -> bool {\n        self == other\n    }\n}\n",
    );
    write(
        &dir,
        "pkg/m.lu",
        "use std.tiny\n\nfn same[K: tiny.Eq](a: K, b: K) -> bool {\n    tiny.Eq.eq(a, b)\n}\n\n\
         fn main() -> !int {\n    print(\"{same(\"x\", \"x\")} {same(\"x\", \"y\")}\")\n    0\n}\n",
    );
    let entry = dir.join("pkg/m.lu");
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
    assert_eq!(value["stdout_inline"], "true false\n", "{value}");
}

/// wolf-std sc44's `std.ops` shape: the file's ONLY mention of the import
/// is the qualified trait in a generic bound, `fn total[T: ops.Add]`. lupin
/// 0.1.33 answered `fail(E0305)` "the import `ops` is never used" with a
/// machine-applicable fix-it that would have made the bound unresolvable;
/// both wolf tiers resolve the file and print `3` (#97). The bound is a
/// use of the import; `acc + x` under `[T: Add]` dispatches through
/// `Add.add` (`[type.trait.op]`), which at `int` is the machine add.
#[test]
fn a_qualified_trait_in_a_generic_bound_counts_as_a_use_of_its_import() {
    let dir = scratch("std-root-bound-use");
    let root = dir.join("std");
    write(
        &root,
        "ops/ops.lu",
        "//! member: true\npub trait Add {\n    fn add(self, other: Self) -> Self\n}\n\n\
         impl Add for int {\n    fn add(self, other: Self) -> Self {\n        self + other\n    }\n}\n",
    );
    write(
        &dir,
        "pkg/o.lu",
        "use std.ops\n\nfn total[T: ops.Add](xs: List[T], zero: T) -> T {\n    var acc = zero\n    \
         for x in xs { acc = acc + x }\n    acc\n}\n\nfn main() -> !int {\n    var a = List[int]()\n    \
         (mut a).push(1)\n    (mut a).push(2)\n    print(\"{total(a, 0)}\")\n    0\n}\n",
    );
    let entry = dir.join("pkg/o.lu");
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
    assert_eq!(value["stdout_inline"], "3\n", "{value}");
}

/// wolf-interp#102: a trait ALIAS reached through an import expands its bound.
///
/// The issue's witness is `fn twice[T: tiny.Num]` over `pub trait Num = Add`
/// in `std/tiny`, E0501 on 0.1.36 while `[T: tiny.Add]` through the same
/// import ran. Here: the alias names an alias (`Num = Small + Mul`,
/// `Small = Add`) and the import is renamed (`use std.tiny as t`), so the
/// head of the bound is the BOUND name and the chain is read in the alias's
/// own module; and the negative twin, `t.Small` with a `*`, is still E0501 at
/// the operator. Both measured on wolf 0.2.14 `--checked` (pin 30731a6):
/// `42\n`, and `fail(E0501)` at `[73, 74]`.
#[test]
fn an_imported_trait_alias_expands_its_bound_through_the_import() {
    let dir = scratch("std-root-imported-alias");
    let std = dir.join("std");
    write(
        &std,
        "tiny/tiny.lu",
        "//! member: true\n\n\
         pub trait Add {\n    fn add(self, other: Self) -> Self\n}\n\n\
         pub trait Mul {\n    fn mul(self, other: Self) -> Self\n}\n\n\
         pub trait Small = Add\n\
         pub trait Num = Small + Mul\n\n\
         impl Add for int {\n    fn add(self, other: Self) -> Self {\n        self + other\n    }\n}\n\n\
         impl Mul for int {\n    fn mul(self, other: Self) -> Self {\n        self * other\n    }\n}\n",
    );
    let program = |bound: &str| {
        format!(
            "//! member: true\n\
             use std.tiny as t\n\n\
             fn poly[T: t.{bound}](a: T) -> T {{\n    a * a + a\n}}\n\n\
             fn main() {{\n    let n: int = 6\n    print(\"{{poly(n)}}\")\n}}\n"
        )
    };
    write(&dir, "pos/main.lu", &program("Num"));
    write(&dir, "neg/main.lu", &program("Small"));
    let std_arg = std.to_str().expect("utf-8 path");
    let pos = dir.join("pos/main.lu");
    let accepted = record(&lupin(
        &[
            "conform-run",
            "--std-root",
            std_arg,
            pos.to_str().expect("utf-8"),

// -- [type.method]: methods on std data reach their home module (is51) -----

/// A std tree with the three home modules this suite needs. `is_empty` is
/// DELIBERATELY wrong in std so the test can see that step (1)'s builtin
/// answers the method spelling and the std function the qualified one.
fn stage_homes(dir: &Path) -> PathBuf {
    let root = dir.join("std");
    write(
        &root,
        "list/list.lu",
        "pub fn any[T](xs: List[T], pred: fn(T) -> bool) -> bool {\n\
         \x20   for x in xs {\n\
         \x20       if pred(x) { return true }\n\
         \x20   }\n\
         \x20   false\n\
         }\n\
         \n\
         pub fn map[T, U](xs: List[T], f: fn(T) -> U) -> List[U] {\n\
         \x20   var out = List[U]()\n\
         \x20   for x in xs { (mut out).push(f(x)) }\n\
         \x20   out\n\
         }\n\
         \n\
         pub fn sum(xs: List[int]) -> int {\n\
         \x20   var total: int = 0\n\
         \x20   for v in xs { total = total + v }\n\
         \x20   total\n\
         }\n\
         \n\
         pub fn sort_by[T](mut xs: List[T], less: fn(T, T) -> bool) {\n\
         \x20   var out = List[T]()\n\
         \x20   for x in xs {\n\
         \x20       var placed = List[T]()\n\
         \x20       var done = false\n\
         \x20       for y in out {\n\
         \x20           if !done && less(x, y) {\n\
         \x20               (mut placed).push(x)\n\
         \x20               done = true\n\
         \x20           }\n\
         \x20           (mut placed).push(y)\n\
         \x20       }\n\
         \x20       if !done { (mut placed).push(x) }\n\
         \x20       out = placed\n\
         \x20   }\n\
         \x20   xs = out\n\
         }\n\
         \n\
         pub fn is_empty[T](xs: List[T]) -> bool { false }\n",
    );
    write(
        &root,
        "str/str.lu",
        "pub fn shout(s: str) -> str { s.upper() }\n",
    );
    write(
        &root,
        "map/map.lu",
        "pub fn size[K, V](m: Map[K, V]) -> int { m.len }\n",
    );
    root
}

/// Runs `source` as `pkg/main.lu` under `dir`, with the staged homes as the
/// std root when `with_root` holds.
fn run_homes(name: &str, source: &str, with_root: bool) -> Output {
    let dir = scratch(name);
    let root = stage_homes(&dir);
    write(&dir, "pkg/main.lu", source);
    let entry = dir.join("pkg/main.lu");
    let entry = entry.to_str().expect("utf-8 path");
    let root = root.to_str().expect("utf-8 path");
    if with_root {
        lupin(&["run", entry, "--std-root", root], &[])
    } else {
        lupin(&["run", entry], &[])
    }
}

fn stdout_text(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("utf-8")
}

fn stderr_text(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).expect("utf-8")
}

#[test]
fn a_method_call_on_std_data_is_the_free_call_with_no_use() {
    // `[type.method.resolve]` step (2) on all three home types, and
    // `[type.method.home]`'s "reaches it with no `use`".
    let output = run_homes(
        "homes-methods",
        "fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(3)\n\
         \x20   (mut xs).push(1)\n\
         \x20   (mut xs).push(2)\n\
         \x20   let tens = xs.map(fn(x) x * 10)\n\
         \x20   (mut xs).sort_by(fn(a, b) a < b)\n\
         \x20   var m = Map[str, int]()\n\
         \x20   m[\"k\"] = 1\n\
         \x20   let s = \"wolf\"\n\
         \x20   print(\"{xs.any(fn(x) x > 2)} {tens.sum()} {xs[0]}{xs[1]}{xs[2]} {m.size()} {s.shout()}\")\n\
         \x20   0\n\
         }\n",
        true,
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_text(&output), "true 60 123 1 WOLF\n");
}

#[test]
fn the_method_and_the_qualified_spelling_are_one_call() {
    // "`xs.any(p)` and `list.any(xs, p)` the same call" — and a name step (1)
    // answers never reaches step (2): the staged std `is_empty` says `false`
    // on an empty list, the builtin says `true`.
    let output = run_homes(
        "homes-one-call",
        "use std.list\n\
         \n\
         fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   let e = xs.is_empty()\n\
         \x20   let q = list.is_empty(xs)\n\
         \x20   (mut xs).push(5)\n\
         \x20   print(\"{xs.any(fn(x) x == 5)} {list.any(xs, fn(x) x == 5)} {e} {q}\")\n\
         \x20   0\n\
         }\n",
        true,
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_text(&output), "true true true false\n");
}

#[test]
fn a_home_module_binds_no_name() {
    // `[type.method.home]`: "`list` is still not in scope there".
    let output = run_homes(
        "homes-no-binding",
        "fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(5)\n\
         \x20   let a = xs.sum()\n\
         \x20   let b = list.sum(xs)\n\
         \x20   print(\"{a} {b}\")\n\
         \x20   0\n\
         }\n",
        true,
    );
    assert_ne!(output.status.code(), Some(0), "{output:?}");
    assert!(stdout_text(&output).is_empty(), "{output:?}");
}

#[test]
fn step_two_with_no_std_root_is_e0301_and_a_builtin_needs_none() {
    // `[type.method.root]`: E0301 naming the home module, never a stand-in;
    // step (1) — `par` included — runs on a bare run.
    let refused = run_homes(
        "homes-no-root",
        "fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(5)\n\
         \x20   print(\"{xs.sum()}\")\n\
         \x20   0\n\
         }\n",
        false,
    );
    assert_ne!(refused.status.code(), Some(0), "{refused:?}");
    let reason = stderr_text(&refused);
    assert!(
        reason.contains("E0301") && reason.contains("std.list"),
        "{reason}"
    );
    let bare = run_homes(
        "homes-no-root-builtin",
        "fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(5)\n\
         \x20   let ys = xs.par(fn(x) x + 1)\n\
         \x20   print(\"{xs.is_empty()} {ys[0]}\")\n\
         \x20   0\n\
         }\n",
        false,
    );
    assert_eq!(bare.status.code(), Some(0), "{bare:?}");
    assert_eq!(stdout_text(&bare), "false 6\n");
}

#[test]
fn a_candidate_that_does_not_fit_is_e0403_and_a_wrong_count_is_e0402() {
    let mismatch = run_homes(
        "homes-e0403",
        "fn main() -> !int {\n\
         \x20   var names = List[str]()\n\
         \x20   (mut names).push(\"a\")\n\
         \x20   print(\"{names.sum()}\")\n\
         \x20   0\n\
         }\n",
        true,
    );
    assert!(stderr_text(&mismatch).contains("E0403"), "{mismatch:?}");
    let absent = run_homes(
        "homes-e0403-absent",
        "fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   print(\"{xs.frob()}\")\n\
         \x20   0\n\
         }\n",
        true,
    );
    assert!(stderr_text(&absent).contains("E0403"), "{absent:?}");
    let arity = run_homes(
        "homes-e0402",
        "fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(1)\n\
         \x20   print(\"{xs.sum(2)}\")\n\
         \x20   0\n\
         }\n",
        true,
    );
    assert!(stderr_text(&arity).contains("E0402"), "{arity:?}");
}

#[test]
fn the_receiver_mode_is_the_first_parameters() {
    // X1: a `mut` candidate spelled bare traps `exclusivity` (E0804's
    // dynamic meaning); a `mut` spelled where the candidate takes none is
    // E0804 by name.
    let dir = scratch("homes-mode-bare");
    let root = stage_homes(&dir);
    write(
        &dir,
        "pkg/main.lu",
        "fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(2)\n\
         \x20   xs.sort_by(fn(a, b) a < b)\n\
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
    assert_eq!(accepted["verdict"], "exit(0)", "{accepted}");
    assert_eq!(accepted["stdout_inline"], "42\n", "{accepted}");
    let neg = dir.join("neg/main.lu");
    let refused = record(&lupin(
        &[
            "conform-run",
            "--std-root",
            std_arg,
            neg.to_str().expect("utf-8"),
            "--json",
        ],
        &[],
    ));
    assert_eq!(refused["verdict"], "fail(E0501)", "{refused}");
    assert_eq!(
        refused["diagnostics"][0]["span"],
        serde_json::json!([73, 74])

    assert_eq!(value["verdict"], "trap(exclusivity)", "{value}");
    let over = run_homes(
        "homes-mode-over",
        "fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(2)\n\
         \x20   print(\"{(mut xs).sum()}\")\n\
         \x20   0\n\
         }\n",
        true,
    );
    assert!(stderr_text(&over).contains("E0804"), "{over:?}");
}

#[test]
fn take_is_never_a_method_and_the_refusal_names_the_slices() {
    // `[type.method.take]`.
    let output = run_homes(
        "homes-take",
        "fn main() -> !int {\n\
         \x20   var xs = List[int]()\n\
         \x20   (mut xs).push(1)\n\
         \x20   let ys = xs.take(1)\n\
         \x20   0\n\
         }\n",
        true,
    );
    assert_ne!(output.status.code(), Some(0), "{output:?}");
    let reason = stderr_text(&output);
    assert!(
        reason.contains("xs[..n]") && reason.contains("xs[n..]"),
        "{reason}"
    );
}

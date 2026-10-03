//! is68 — wave 53's P0s on lupin and the issues beside them (t04's triage,
//! wolffe-lang/wolf#6): wolf-interp#126, #103 (row 1), #125, #138, #169.
//!
//! - **#126** `[mem.region.escape]` (s171): a `[mem.str.view]` product carries
//!   its RECEIVER's sites. A view of a region-built `str` that outlives the
//!   region is the clause's `region-fault` here — never a read of freed
//!   bytes, which is what 0.1.43 printed.
//! - **#103** `[type.unit.context]`: the then-block of an `if` with no
//!   `else` (and every block of a chain that ends without one), a loop body
//!   and a unit fn body are unit contexts — "the block's value is `()`
//!   whatever the tail's type". 0.1.43 bound `"*"`. Where this machine can
//!   type the tail from syntax it refuses as the compiler does (E0401 at the
//!   tail); where it cannot, the value is `()`.
//! - **#125** `[mem.str.ws]`: "the family takes no argument" — E0402 at the
//!   call where the receiver is known to be a `str`, declined by name where
//!   it is not; never a cutset.
//! - **#138**: `str` and `bool` join the declared-scalar lattice — E0401 at
//!   the operand, in a sibling module with that module's file.
//! - **#169**: a nested fn's parameter modes are a module fn's — E1007 at a
//!   call that disagrees, and the call runs with the module fn's convention.
//!
//! One directory per row under `tests/rulings_is68/`, each with an
//! `expected.toml` whose `[expected]` lupin cell is the ruled answer and
//! whose `[trunk]` table is what lupin trunk `6ce7bc8` and wolf-lang trunk
//! `12a56b22` (= the published 0.2.20 on every row) answered. A `fail`
//! cell names the first error's code, span and, outside the entry, file —
//! `[proto.cmp.rung]`'s agreement. The runner is is65's, widened for those.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is68")
        .join(name)
}

/// The ruled lupin cell: the verdict, and what the record must carry with it.
#[derive(Debug)]
struct Ruled {
    verdict: String,
    stdout: Option<String>,
    clause: Option<String>,
    code: Option<String>,
    diag: Option<[u64; 2]>,
    file: Option<String>,
}

/// Reads `[expected]`'s `lupin = { … }` line — one inline table on one line,
/// the shape every file here has; a file that stops having it fails loudly.
fn ruled(name: &str) -> Ruled {
    let text = std::fs::read_to_string(witness_dir(name).join("expected.toml"))
        .expect("the witness carries its expected.toml");
    let mut in_expected = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_expected = line == "[expected]";
            continue;
        }
        if !in_expected || !line.starts_with("lupin") {
            continue;
        }
        let diag = line.split("diag = [").nth(1).map(|rest| {
            let list = rest.split(']').next().expect("a closed span");
            let mut ends = list
                .split(',')
                .map(|n| n.trim().parse::<u64>().expect("an offset"));
            [ends.next().expect("a start"), ends.next().expect("an end")]
        });
        return Ruled {
            verdict: quoted_after(line, "verdict = ").expect("a verdict"),
            stdout: quoted_after(line, "stdout = ").map(|s| s.replace("\\n", "\n")),
            clause: quoted_after(line, "clause = "),
            code: quoted_after(line, "code = "),
            diag,
            file: quoted_after(line, "file = "),
        };
    }
    panic!("{name}/expected.toml has no [expected] lupin cell");
}

fn quoted_after(line: &str, key: &str) -> Option<String> {
    let rest = line.split(key).nth(1)?;
    let rest = rest.strip_prefix('"')?;
    Some(rest.split('"').next()?.to_owned())
}

/// Copies the witness's program — every `.lu` under it, sibling module
/// directories included — into a fresh scratch directory.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("rulings_is68")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    copy_lu(&witness_dir(name), &dir);
    dir
}

fn copy_lu(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("scratch created");
    for entry in std::fs::read_dir(from).expect("the witness directory reads") {
        let path = entry.expect("an entry").path();
        let target = to.join(path.file_name().expect("a name"));
        if path.is_dir() {
            copy_lu(&path, &target);
        } else if path.extension().is_some_and(|ext| ext == "lu") {
            std::fs::copy(&path, &target).expect("copied");
        }
    }
}

/// `lupin conform-run main.lu --json` from `dir`: the whole record.
fn conform_run(dir: &Path) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["conform-run", "main.lu", "--json"])
        .current_dir(dir)
        .output()
        .expect("lupin runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "the tool succeeded: {output:?}"
    );
    serde_json::from_slice(&output.stdout).expect("one JSON record")
}

fn run(name: &str) {
    let ruled = ruled(name);
    let record = conform_run(&scratch(name));
    let verdict = record["verdict"].as_str().expect("a verdict");
    assert_eq!(
        verdict, ruled.verdict,
        "{name}: lupin answered `{verdict}`, ruled `{}` — record {record}",
        ruled.verdict
    );
    if let Some(stdout) = &ruled.stdout {
        assert_eq!(
            record["stdout_inline"].as_str().unwrap_or(""),
            stdout.as_str(),
            "{name}: the ruled stdout — record {record}"
        );
    }
    if let Some(clause) = &ruled.clause {
        assert_eq!(
            record["x-trap-clause"].as_str(),
            Some(clause.as_str()),
            "{name}: the trap's clause — record {record}"
        );
    }
    if let Some(code) = &ruled.code {
        // `[proto.cmp.rung]`: the FIRST error is the one compared — its
        // code, its span, and the file it names (wolf-lang#437).
        let first = record["diagnostics"]
            .as_array()
            .and_then(|all| all.iter().find(|d| d["severity"] != "warning"))
            .unwrap_or_else(|| panic!("{name}: a fail carries its error — record {record}"));
        assert_eq!(
            first["code"].as_str(),
            Some(code.as_str()),
            "{name}: {record}"
        );
        if let Some([start, end]) = ruled.diag {
            assert_eq!(
                first["span"],
                serde_json::json!([start, end]),
                "{name}: the error's span — record {record}"
            );
        }
        let file = first["file"]
            .as_u64()
            .filter(|index| *index > 0)
            .map(|index| {
                record["files"][usize::try_from(index).expect("an index")]
                    .as_str()
                    .expect("the files table names the index")
                    .to_owned()
            });
        assert_eq!(
            file, ruled.file,
            "{name}: the error's file — record {record}"
        );
    }
}

// -- wolf-interp#126: a view product carries its receiver's region -------------

/// The issue's program: `s.trim()` of a region-built `str`, bound and returned.
#[test]
fn v126_issue() {
    run("v126_issue");
}

/// `trim`, bound and returned.
#[test]
fn v126_trim() {
    run("v126_trim");
}

/// `trim` as the region block's tail.
#[test]
fn v126_trim_tail() {
    run("v126_trim_tail");
}

/// `trim_start`.
#[test]
fn v126_trim_start() {
    run("v126_trim_start");
}

/// `trim_end`.
#[test]
fn v126_trim_end() {
    run("v126_trim_end");
}

/// `get(2..6)`.
#[test]
fn v126_get_range() {
    run("v126_get_range");
}

/// `get(2..)` (read off the syntax).
#[test]
fn v126_get_open() {
    run("v126_get_open");
}

/// `get(2..=5)`.
#[test]
fn v126_get_inclusive() {
    run("v126_get_inclusive");
}

/// `strip_prefix`, bound.
#[test]
fn v126_strip_prefix() {
    run("v126_strip_prefix");
}

/// `strip_prefix` as the tail.
#[test]
fn v126_strip_prefix_tail() {
    run("v126_strip_prefix_tail");
}

/// `strip_suffix`.
#[test]
fn v126_strip_suffix() {
    run("v126_strip_suffix");
}

/// A view of a view: `s.trim().trim_end()`.
#[test]
fn v126_view_of_view() {
    run("v126_view_of_view");
}

/// `copy s.trim()` inside the region: the copy lands in the same region.
#[test]
fn v126_copy_in_region() {
    run("v126_copy_in_region");
}

/// The slice `s[2..6]`.
#[test]
fn v126_slice() {
    run("v126_slice");
}

/// A `split` piece read by index, `ps[0]`.
#[test]
fn v126_split_index() {
    run("v126_split_index");
}

/// `keep = s.trim()` into a binding declared outside the region.
#[test]
fn v126_held_outside() {
    run("v126_held_outside");
}

/// The view as a `region` block's value.
#[test]
fn v126_block_value() {
    run("v126_block_value");
}

/// `d.title.trim()` of a region-local struct's built field.
#[test]
fn v126_field_view() {
    run("v126_field_view");
}

/// `out.send(s.trim())` from a `spawn proc` whose region built `s`.
#[test]
fn v126_proc_send() {
    run("v126_proc_send");
}

/// A `for` piece of `words()` returned (the compiler runs it: filed upstream).
#[test]
fn v126_for_words() {
    run("v126_for_words");
}

/// A `for` piece of `lines()` returned (the compiler runs it: filed upstream).
#[test]
fn v126_for_lines() {
    run("v126_for_lines");
}

/// A `for` piece of `split(",")` returned (the compiler runs it: filed upstream).
#[test]
fn v126_for_split() {
    run("v126_for_split");
}

/// A `for` piece of `words()` held outside the region (the compiler runs it: filed upstream).
#[test]
fn v126_for_words_held() {
    run("v126_for_words_held");
}

/// Control: a view of a literal is site-free.
#[test]
fn v126_ctl_literal() {
    run("v126_ctl_literal");
}

/// Control: a view of a literal-bound local.
#[test]
fn v126_ctl_literal_local() {
    run("v126_ctl_literal_local");
}

/// Control: only the receiver's sites flow, never the needle's.
#[test]
fn v126_ctl_needle() {
    run("v126_ctl_needle");
}

/// Control: a view of a parameter, returned out of a region in the callee.
#[test]
fn v126_ctl_param() {
    run("v126_ctl_param");
}

/// Control: a view of an outer local, held outside a region.
#[test]
fn v126_ctl_outer_held() {
    run("v126_ctl_outer_held");
}

/// Control: views used inside the region that owns their bytes.
#[test]
fn v126_ctl_inside() {
    run("v126_ctl_inside");
}

/// Control: `s.bytes()` returned is a region allocation already.
#[test]
fn v126_ctl_bytes() {
    run("v126_ctl_bytes");
}

/// Control: `s.trim().upper()` returned is a site already.
#[test]
fn v126_ctl_upper() {
    run("v126_ctl_upper");
}

// -- wolf-interp#103: a unit context's value is `()` ---------------------------

/// Row 1: `let mark = if cents > 300 { "*" }`.
#[test]
fn u103_issue() {
    run("u103_issue");
}

/// An else-less `if` statement with a `str` tail.
#[test]
fn u103_stmt_str() {
    run("u103_stmt_str");
}

/// An else-less `if` statement with an `int` tail.
#[test]
fn u103_stmt_int() {
    run("u103_stmt_int");
}

/// A chain of two ending without `else`: the trailing else-less `if` first.
#[test]
fn u103_chain2() {
    run("u103_chain2");
}

/// A chain of three ending without `else`.
#[test]
fn u103_chain3() {
    run("u103_chain3");
}

/// A `while` body whose tail is a local `int`.
#[test]
fn u103_while_tail() {
    run("u103_while_tail");
}

/// A `loop` body with a `str` tail.
#[test]
fn u103_loop_tail() {
    run("u103_loop_tail");
}

/// A literal-bound local as the tail.
#[test]
fn u103_local_tail() {
    run("u103_local_tail");
}

/// An interpolated `str` tail.
#[test]
fn u103_interp_tail() {
    run("u103_interp_tail");
}

/// An arithmetic tail.
#[test]
fn u103_arith_tail() {
    run("u103_arith_tail");
}

/// A `bool` literal tail.
#[test]
fn u103_bool_tail() {
    run("u103_bool_tail");
}

/// A comparison tail.
#[test]
fn u103_cmp_tail() {
    run("u103_cmp_tail");
}

/// A list literal tail.
#[test]
fn u103_list_tail() {
    run("u103_list_tail");
}

/// A struct literal tail.
#[test]
fn u103_struct_tail() {
    run("u103_struct_tail");
}

/// A `char` tail, then a float tail.
#[test]
fn u103_char_float_tail() {
    run("u103_char_float_tail");
}

/// A concatenation tail.
#[test]
fn u103_concat_tail() {
    run("u103_concat_tail");
}

/// A call to `fn seven() -> int` as the tail.
#[test]
fn u103_call_tail() {
    run("u103_call_tail");
}

/// A `str` parameter as the tail, inside a unit fn.
#[test]
fn u103_param_tail() {
    run("u103_param_tail");
}

/// An `if … else` as the tail: the whole `if`.
#[test]
fn u103_if_else_tail() {
    run("u103_if_else_tail");
}

/// `fn total() { 1 }`.
#[test]
fn u103_unit_fn() {
    run("u103_unit_fn");
}

/// `fn main() { …; 3 }` (exited 3).
#[test]
fn u103_unit_main() {
    run("u103_unit_main");
}

/// `fn f() -> () { 5 }`.
#[test]
fn u103_unit_explicit() {
    run("u103_unit_explicit");
}

/// A nested `fn helper() { 5 }`.
#[test]
fn u103_unit_nested() {
    run("u103_unit_nested");
}

/// The dynamic half: `let v = if n > 1 { maybe(n) }` binds `()` (native and release).
#[test]
fn u103_row_tail_value() {
    run("u103_row_tail_value");
}

/// The dynamic half: a unit `main` whose tail raises discards it (native and release).
#[test]
fn u103_unit_main_row() {
    run("u103_unit_main_row");
}

/// Residue: `{ xs.len }` is a member read this machine cannot type; the value is now `()`.
#[test]
fn u103_res_member() {
    run("u103_res_member");
}

/// Residue: a `for` element this machine cannot type.
#[test]
fn u103_res_for() {
    run("u103_res_for");
}

/// Residue: a method's `self.n` tail.
#[test]
fn u103_res_method() {
    run("u103_res_method");
}

/// Control: an else-less `if` whose tail is `()`.
#[test]
fn u103_ctl_unit() {
    run("u103_ctl_unit");
}

/// Control: an `if … else` value.
#[test]
fn u103_ctl_if_else() {
    run("u103_ctl_if_else");
}

/// A raise out of an else-less `if` at a fallible fn's tail: is68's control, which
/// ruling #34 = A (s208, #179) turned into a discard on every machine.
#[test]
fn u103_ctl_raise_tail() {
    run("u103_ctl_raise_tail");
}

/// Control: a `T ! row` tail in a statement `if` is W0601's discard.
#[test]
fn u103_ctl_w0601() {
    run("u103_ctl_w0601");
}

/// Control: `match` arms of unit calls in a loop body.
#[test]
fn u103_ctl_match_loop() {
    run("u103_ctl_match_loop");
}

/// Control: a closure with no result type infers it.
#[test]
fn u103_ctl_closure() {
    run("u103_ctl_closure");
}

/// Control: a raise in a statement else-less `if` is discarded.
#[test]
fn u103_ctl_stmt_raise() {
    run("u103_ctl_stmt_raise");
}

/// Control: a discarded raise in a fallible fn's statement `if`.
#[test]
fn u103_ctl_body_stmt() {
    run("u103_ctl_body_stmt");
}

// -- wolf-interp#125: the `[mem.str.ws]` family takes no argument --------------

/// The issue's program: `s.trim(".,!?")`.
#[test]
fn c125_issue() {
    run("c125_issue");
}

/// `trim_start("!")`.
#[test]
fn c125_trim_start() {
    run("c125_trim_start");
}

/// `trim_end("?")`.
#[test]
fn c125_trim_end() {
    run("c125_trim_end");
}

/// A cutset in a variable.
#[test]
fn c125_var_cutset() {
    run("c125_var_cutset");
}

/// Two arguments.
#[test]
fn c125_two_args() {
    run("c125_two_args");
}

/// `words(" ")`.
#[test]
fn c125_words() {
    run("c125_words");
}

/// `lines("x")` (outside the clause's family; the compiler refuses it too).
#[test]
fn c125_lines() {
    run("c125_lines");
}

/// A literal receiver.
#[test]
fn c125_lit_receiver() {
    run("c125_lit_receiver");
}

/// A `str` parameter receiver.
#[test]
fn c125_param_receiver() {
    run("c125_param_receiver");
}

/// A receiver this machine cannot type: declined by name.
#[test]
fn c125_for_piece() {
    run("c125_for_piece");
}

/// Control: the family with no argument.
#[test]
fn c125_ctl_family() {
    run("c125_ctl_family");
}

/// Control: a user `trim(k)` method.
#[test]
fn c125_ctl_user_trim() {
    run("c125_ctl_user_trim");
}

// -- wolf-interp#138: `str` and `bool` in the declared-scalar lattice ----------

/// S181's program: `let s: str = side` in a sibling module.
#[test]
fn s138_issue() {
    run("s138_issue");
}

/// The same in a module of two files.
#[test]
fn s138_three_files() {
    run("s138_three_files");
}

/// The same body in the root module.
#[test]
fn s138_root() {
    run("s138_root");
}

/// `let s: str = 3`.
#[test]
fn s138_str_from_lit() {
    run("s138_str_from_lit");
}

/// `let n: int = t`, `t` a `str`.
#[test]
fn s138_int_from_str() {
    run("s138_int_from_str");
}

/// `let b: bool = n`, `n: int`.
#[test]
fn s138_bool_from_int() {
    run("s138_bool_from_int");
}

/// `let b: bool = 1`.
#[test]
fn s138_bool_from_lit() {
    run("s138_bool_from_lit");
}

/// `let n: int = true`.
#[test]
fn s138_int_from_bool() {
    run("s138_int_from_bool");
}

/// `let s: str = 'x'`.
#[test]
fn s138_str_from_char() {
    run("s138_str_from_char");
}

/// `let k: int = "{n}"`.
#[test]
fn s138_int_from_interp() {
    run("s138_int_from_interp");
}

/// `-> str` returning `7`.
#[test]
fn s138_return() {
    run("s138_return");
}

/// `greet(5)` for `s: str`.
#[test]
fn s138_arg_str() {
    run("s138_arg_str");
}

/// `flag("yes")` for `b: bool`.
#[test]
fn s138_arg_bool() {
    run("s138_arg_bool");
}

/// `Doc { title: 3 }`.
#[test]
fn s138_field() {
    run("s138_field");
}

/// `s = n` into a `str` local.
#[test]
fn s138_assign() {
    run("s138_assign");
}

/// Control: `let s: int = side` in the sibling.
#[test]
fn s138_ctl_sibling_int() {
    run("s138_ctl_sibling_int");
}

/// Control: `let s: str = t`, `t: str`.
#[test]
fn s138_ctl_str_param() {
    run("s138_ctl_str_param");
}

/// Control: a `str` call result, a comparison into `bool`, an interpolation into `str`.
#[test]
fn s138_ctl_ok_shapes() {
    run("s138_ctl_ok_shapes");
}

/// Control: a generic result is not typed here.
#[test]
fn s138_ctl_generic() {
    run("s138_ctl_generic");
}

/// Control: a `List[str]` element.
#[test]
fn s138_ctl_list_elem() {
    run("s138_ctl_list_elem");
}

// -- wolf-interp#169: a nested fn's parameter modes ----------------------------

/// `memory/nested_fn_mut_param.lu` (v0.2.20).
#[test]
fn n169_param() {
    run("n169_param");
}

/// A `mut` parameter moved out and stored back.
#[test]
fn n169_restored() {
    run("n169_restored");
}

/// `memory/nested_fn_mut_omitted.lu` (v0.2.20).
#[test]
fn n169_omitted() {
    run("n169_omitted");
}

/// A `take` parameter called bare.
#[test]
fn n169_take_omitted() {
    run("n169_take_omitted");
}

/// `mut` spelled on a plain parameter.
#[test]
fn n169_mut_on_read() {
    run("n169_mut_on_read");
}

/// `memory/nested_fn_mut_moveout.lu` (v0.2.20): the dynamic counterpart of E1001.
#[test]
fn n169_moveout() {
    run("n169_moveout");
}

/// Control: a capture stays refused.
#[test]
fn n169_capture() {
    run("n169_capture");
}

/// Control: a nested fn naming itself is out of the scoped v1.
#[test]
fn n169_recursive() {
    run("n169_recursive");
}

/// A moded nested fn bound as a value and never called.
#[test]
fn n169_as_value() {
    run("n169_as_value");
}

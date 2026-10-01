//! is62 — four lupin mirrors (wave 52):
//!
//! - wolf-interp#157: every operand of a place expression runs once — a slice
//!   of an indexed element (`g[gi()][lo()..hi()]`) ran `gi` two or three times.
//! - wolf-interp#162: a flow out of a method receiver's index (`?`, `return`,
//!   `break`, `continue`) leaves once — `rows[idx()?].count()` ran `idx` twice.
//! - wolf-interp#159: the struct field shorthand is the longhand (s190,
//!   wolffe-lang/wolf-lang#486) — `W { xs }` copied `xs` (`1 2`) where
//!   `W { xs: xs }` moves it.
//! - wolffe-lang/wolf-lang#487, lupin's half: a `(mut …)` receiver's claim is
//!   pending for its own call while the arguments run (`[mem.tier0.excl.4]`),
//!   so a write, move, re-claim or lend of it inside its own argument traps
//!   `exclusivity` (0.1.42 answered `ub(mem.ub)`, or ran), and a read runs.
//!
//! One directory per shape under `tests/rulings_is62/`, each with an
//! `expected.toml` whose `[expected]` lupin cell is the ruled answer and
//! whose `[trunk]` table is what lupin trunk `1e96e1f` and wolf-lang trunk
//! `57805e35` answered. `once_*` rows print wolf's bytes; `shorthand_*` rows
//! are also run beside their longhand twin (the row's text with each
//! shorthand spelled out, the twin built here as wolf-lang's
//! `field_shorthand_lanes.rs` builds it), and the two must answer alike:
//! verdict, diagnostic codes and stdout.
//!
//! The runner is is63's (`tests/rulings_is63.rs`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is62")
        .join(name)
}

/// The ruled lupin cell: the verdict, the stdout the run must print, and for
/// a trap the clause and span its record must carry.
#[derive(Debug)]
struct Ruled {
    verdict: String,
    stdout: Option<String>,
    clause: Option<String>,
    span: Option<[u64; 2]>,
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
        let span = line.split("span = [").nth(1).map(|rest| {
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
            span,
        };
    }
    panic!("{name}/expected.toml has no [expected] lupin cell");
}

fn quoted_after(line: &str, key: &str) -> Option<String> {
    let rest = line.split(key).nth(1)?;
    let rest = rest.strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_owned())
}

/// A fresh copy of the witness's program in its own scratch directory.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("rulings_is62")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    std::fs::copy(witness_dir(name).join("main.lu"), dir.join("main.lu")).expect("copied");
    dir
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
    if let Some([start, end]) = ruled.span {
        assert_eq!(
            record["x-trap-span"],
            serde_json::json!([start, end]),
            "{name}: the trap's span — record {record}"
        );
    }
}

/// The row's longhand twin, in a scratch directory of its own: each
/// `(shorthand, longhand)` spelling replaced in the program text (the `//!`
/// header is prose and is left alone), each exactly once.
fn longhand_twin(name: &str, spellings: &[(&str, &str)]) -> PathBuf {
    let src = std::fs::read_to_string(witness_dir(name).join("main.lu")).expect("the row reads");
    let lines: Vec<&str> = src.lines().collect();
    let n = lines.iter().take_while(|l| l.starts_with("//!")).count();
    let mut code = lines[n..].join("\n");
    for (short, long) in spellings {
        assert_eq!(
            code.matches(short).count(),
            1,
            "`{short}` appears exactly once in the program text of {name}"
        );
        code = code.replace(short, long);
    }
    let mut text = lines[..n].join("\n");
    text.push('\n');
    text.push_str(&code);
    text.push('\n');
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("rulings_is62")
        .join(format!("{name}-longhand"));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    std::fs::write(dir.join("main.lu"), text).expect("twin written");
    dir
}

/// What a record answers, but for the implementation's own version.
fn answer(record: &serde_json::Value) -> (String, Vec<String>, String) {
    let codes = record["diagnostics"]
        .as_array()
        .map(|ds| {
            ds.iter()
                .filter_map(|d| d["code"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    (
        record["verdict"].as_str().unwrap_or("").to_owned(),
        codes,
        record["stdout_inline"].as_str().unwrap_or("").to_owned(),
    )
}

/// The ruled cell, and then the shorthand row and its longhand twin answer
/// alike (equality alone would pass with both spellings wrong together; the
/// ruled cell alone would not say they agree).
fn shorthand_is_longhand(name: &str, spellings: &[(&str, &str)]) {
    run(name);
    let short = conform_run(&scratch(name));
    let long = conform_run(&longhand_twin(name, spellings));
    assert_eq!(
        answer(&short),
        answer(&long),
        "{name}: the shorthand and its longhand twin answer differently — \
         shorthand {short}, longhand {long}"
    );
}

// -- #157: a slice of an indexed element runs each index once -------------

/// wolf-lang `corpus/memory/ctl_slice_endpoints_indexed_base.lu` (s187), the issue's shape: bound, as a member base, in a hole.
#[test]
fn once_slice_indexed_base() {
    run("once_slice_indexed_base");
}

/// wolf-lang `corpus/memory/ctl_slice_try_once.lu` (s189): `g[gi()?][lo()?..hi()]`, the base index once whether or not it propagates.
#[test]
fn once_slice_try_base() {
    run("once_slice_try_base");
}

/// the slice as a `count()`/`is_empty()` receiver, a `read` argument and a `for` iterable.
#[test]
fn once_slice_count_arg_for() {
    run("once_slice_count_arg_for");
}

/// a two-deep index, a field's element and a `str` element under the slice.
#[test]
fn once_slice_deep_field_str() {
    run("once_slice_deep_field_str");
}

/// open, inclusive and `^` endpoints.
#[test]
fn once_slice_open_ends() {
    run("once_slice_open_ends");
}

/// the slice inside a later argument beside a pending `mut` claim (`read_elem_under_claim`'s route).
#[test]
fn once_slice_under_claim() {
    run("once_slice_under_claim");
}

// -- #162: a flow out of a receiver's index leaves once --------------------

/// wolf-lang `corpus/memory/ctl_index_try_once_receivers.lu` (s189), the issue's shapes.
#[test]
fn once_receivers_try() {
    run("once_receivers_try");
}

/// `upper()`, an impl method, a `List` `.len` operand and a parenthesized receiver.
#[test]
fn once_receivers_more() {
    run("once_receivers_more");
}

/// `return`, `break` and `continue` inside a receiver's index.
#[test]
fn once_receivers_flows() {
    run("once_receivers_flows");
}

// -- #159: the shorthand is the longhand ----------------------------------

/// wolf-lang `field_shorthand_moves.lu` (s190): `W { xs }` moves `xs`; the later `xs.len` traps.
#[test]
fn shorthand_moves() {
    shorthand_is_longhand("shorthand_moves", &[("W { xs }", "W { xs: xs }")]);
}

/// wolf-lang `field_shorthand_mixed.lu`: shorthand and longhand in one literal.
#[test]
fn shorthand_mixed() {
    shorthand_is_longhand(
        "shorthand_mixed",
        &[("M { xs, n, ys: ys }", "M { xs: xs, n: n, ys: ys }")],
    );
}

/// wolf-lang `field_shorthand_nested.lu`: the shorthand inside a nested literal.
#[test]
fn shorthand_nested() {
    shorthand_is_longhand(
        "shorthand_nested",
        &[("O { w: W { xs }, n }", "O { w: W { xs: xs }, n: n }")],
    );
}

/// wolf-lang `field_shorthand_nested_bound.lu`: a shorthand local built by a shorthand.
#[test]
fn shorthand_nested_bound() {
    shorthand_is_longhand(
        "shorthand_nested_bound",
        &[
            ("W { xs }", "W { xs: xs }"),
            ("O { w, n }", "O { w: w, n: n }"),
        ],
    );
}

/// wolf-lang `field_shorthand_return_mut.lu`: `return W { xs }` moves a `mut` parameter out.
#[test]
fn shorthand_return_mut() {
    shorthand_is_longhand("shorthand_return_mut", &[("W { xs }", "W { xs: xs }")]);
}

/// wolf-lang `field_shorthand_closure_borrow.lu`: a closure capturing through the shorthand borrows; the write while it is live traps `exclusivity` with W1102.
#[test]
fn shorthand_closure_borrow() {
    shorthand_is_longhand("shorthand_closure_borrow", &[("P { n }", "P { n: n }")]);
}

/// wolf-lang `field_shorthand_copy.lu` (agreed at trunk): `Copy` fields copy.
#[test]
fn shorthand_copy() {
    shorthand_is_longhand("shorthand_copy", &[("P { n, s }", "P { n: n, s: s }")]);
}

/// wolf-lang `field_shorthand_mixed_runs.lu` (agreed at trunk): no later use, it runs.
#[test]
fn shorthand_mixed_runs() {
    shorthand_is_longhand(
        "shorthand_mixed_runs",
        &[("M { xs, n, ys: ys }", "M { xs: xs, n: n, ys: ys }")],
    );
}

/// wolf-lang `field_shorthand_nested_runs.lu` (agreed at trunk).
#[test]
fn shorthand_nested_runs() {
    shorthand_is_longhand(
        "shorthand_nested_runs",
        &[("O { w: W { xs }, n }", "O { w: W { xs: xs }, n: n }")],
    );
}

/// wolf-lang `field_shorthand_return_take.lu` (agreed at trunk): a `take` parameter returned through the shorthand.
#[test]
fn shorthand_return_take() {
    shorthand_is_longhand("shorthand_return_take", &[("W { xs }", "W { xs: xs }")]);
}

/// wolf-lang `field_shorthand_closure.lu` (agreed at trunk): lupin's copy-capture reading, the same for both spellings.
#[test]
fn shorthand_closure() {
    shorthand_is_longhand("shorthand_closure", &[("W { xs }", "W { xs: xs }")]);
}

/// wolf-lang `field_shorthand_task.lu` (agreed at trunk).
#[test]
fn shorthand_task() {
    shorthand_is_longhand("shorthand_task", &[("W { xs }", "W { xs: xs }")]);
}

/// s190's comment: `P { n }` naming a module-level `let` runs, as `P { n: n }` does.
#[test]
fn shorthand_module_let() {
    shorthand_is_longhand("shorthand_module_let", &[("P { n }", "P { n: n }")]);
}

/// `P { n }` with no `n` in scope answers what `P { n: n }` answers.
#[test]
fn shorthand_unknown_name() {
    shorthand_is_longhand("shorthand_unknown_name", &[("P { n }", "P { n: n }")]);
}

/// s190's `P { nope }` and its true twin `P { nope: nope }`: the missing field, both ways.
#[test]
fn shorthand_unknown_field() {
    shorthand_is_longhand(
        "shorthand_unknown_field",
        &[("P { nope }", "P { nope: nope }")],
    );
}

// -- wolf-lang#487: a `mut` receiver inside its own argument --------------

/// the issue's shape: `(mut xs).push({ xs = [9]; 5 })`.
#[test]
fn recv_write() {
    run("recv_write");
}

/// an element of the receiver written: `(mut xs).push({ xs[0] = 9; 5 })`.
#[test]
fn recv_elem_write() {
    run("recv_elem_write");
}

/// a field receiver written: `(mut h.xs).push({ h.xs = [9]; 5 })`.
#[test]
fn recv_field_write() {
    run("recv_field_write");
}

/// an impl method's `mut self` receiver written in its argument.
#[test]
fn recv_impl_write() {
    run("recv_impl_write");
}

/// the receiver claimed again one call down: `(mut xs).push(grow(mut xs))`.
#[test]
fn recv_reclaim_nested() {
    run("recv_reclaim_nested");
}

/// the receiver claimed again as another `(mut xs)` receiver inside its argument.
#[test]
fn recv_reclaim_receiver() {
    run("recv_reclaim_receiver");
}

/// the receiver moved: `(mut xs).push(eat(take xs))`.
#[test]
fn recv_move() {
    run("recv_move");
}

/// the receiver lent into its own call: `(mut c).absorb(c)`.
#[test]
fn recv_lend_impl() {
    run("recv_lend_impl");
}

/// a closure capturing the receiver lent into its own call: `(mut c).apply(fn() { c.n })`.
#[test]
fn recv_closure_lend() {
    run("recv_closure_lend");
}

/// reads of the receiver in its own argument run: `xs.len`, `xs[0] + 10`, `total(xs)`, `xs.count()`.
#[test]
fn recv_reads_run() {
    run("recv_reads_run");
}

/// a `Copy` field of the `mut self` receiver read in its argument runs: `(mut c).add(c.n)`.
#[test]
fn recv_copy_read_impl() {
    run("recv_copy_read_impl");
}

/// a disjoint sibling written inside a field receiver's argument runs: `(mut h.xs).push({ h.k = 9; 5 })`.
#[test]
fn recv_sibling_write() {
    run("recv_sibling_write");
}

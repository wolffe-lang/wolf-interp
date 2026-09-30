//! is63 — ruling #17 (the maintainer, 2026-09-30; wolf `sprints/STATUS.md`
//! item 17): "within one call, arguments are evaluated left to right, and a
//! `mut` argument's claim takes effect when the call is entered, not when its
//! argument is evaluated." A later argument may READ the claimed place — a
//! `Copy` value, an operand, a header or member read, a whole read such as
//! string interpolation, or a nested call whose result lends nothing from the
//! place — and runs; a later argument that WRITES the place, MOVES it, CLAIMS
//! it again or LENDS it into the same call still traps `exclusivity`. `take`
//! arguments are unchanged. The spec clause is s186's (wolffe-lang/wolf-lang#485),
//! its anchor left to the re-pin.
//!
//! One directory per shape under `tests/rulings_is63/`, each with an
//! `expected.toml` whose `[expected]` lupin cell is the ruled answer and whose
//! `[trunk]` table is what the published lupin 0.1.42 and wolf 0.2.19
//! answered. `read_*` rows run with the bytes wolf 0.2.19 prints where it runs
//! them; `stay_*` rows keep trunk's trap, clause and span.
//!
//! The runner is is60's (`tests/rulings_is60.rs`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is63")
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
        .join("rulings_is63")
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

// -- the reads #17 allows: they run -------------------------------------

/// a `Copy` read of the claimed element one call down: `bump(mut xs[0], id(xs[0]))`.
#[test]
fn read_copy_nested() {
    run("read_copy_nested");
}

/// a header method under a claim on the WHOLE container: `grow(mut xs, xs.count())`, s186's `mut_claim_nested_header.lu`.
#[test]
fn read_count_whole_claim() {
    run("read_count_whole_claim");
}

/// a `for` loop over the claimed container inside a later argument's block: its read claim ends with the loop, before the call.
#[test]
fn read_for_in_block() {
    run("read_for_in_block");
}

/// `get` reads the whole container beside `mut xs[0]`: s186's `mut_claim_nested_get.lu`.
#[test]
fn read_get_elem_claim() {
    run("read_get_elem_claim");
}

/// the claimed place read in an `if` condition and arm of a later argument.
#[test]
fn read_if_arm() {
    run("read_if_arm");
}

/// an impl method's `self` reads the claimed struct whole in a later argument: `bump(mut p.y, p.count())` (is60's `twin_impl_count_under_field_claim`).
#[test]
fn read_impl_self() {
    run("read_impl_self");
}

/// lobo's `fail(mut fl, "{fl.store}")`: string interpolation reads the claimed place whole.
#[test]
fn read_interp_whole() {
    run("read_interp_whole");
}

/// the `Map` twin: `mput(mut m, msize(m))`, s186's `mut_claim_nested_map.lu`.
#[test]
fn read_map_nested() {
    run("read_map_nested");
}

/// boreutils' `ensure(mut f, f.len + n)`: a member read of the claimed struct in an operand.
#[test]
fn read_member_ensure() {
    run("read_member_ensure");
}

/// a header member read under a claim on the WHOLE container: `grow(mut xs, xs.len)`.
#[test]
fn read_member_whole_claim() {
    run("read_member_whole_claim");
}

/// a nested method call on ANOTHER place reads the claimed place in its argument: `bump(mut a, (mut ys).pop_or(a))`-style, via push.
#[test]
fn read_method_mut_receiver_arg() {
    run("read_method_mut_receiver_arg");
}

/// an inner call's arguments read the OUTER call's claimed place: `outer(mut a, inner(mut b, a + b))`.
#[test]
fn read_nested_claims() {
    run("read_nested_claims");
}

/// lobo's `ev_log(mut el, level, shell.ev_seq(body, el.seq))`: a field of the claimed struct read one call down.
#[test]
fn read_nested_field_seq() {
    run("read_nested_field_seq");
}

/// boreutils' `bore.ring_drop(mut r, bore.ring_len(r))`: a nested call reads the whole claimed struct.
#[test]
fn read_nested_ring_len() {
    run("read_nested_ring_len");
}

/// an operand reads the claimed place: `bump(mut a, a + 1)`, s186's `mut_claim_operand_read.lu`.
#[test]
fn read_operand() {
    run("read_operand");
}

/// left to right: the later argument reads the value the place holds BEFORE the call, and runs once.
#[test]
fn read_order_effects() {
    run("read_order_effects");
}

/// a slice of the claimed container read one call down: `grow(mut xs, total(xs[0..2]))`.
#[test]
fn read_slice_nested() {
    run("read_slice_nested");
}

/// wolf-lang#476's first shape: `bump(mut xs[0], total(xs))`.
#[test]
fn read_total_elem_claim() {
    run("read_total_elem_claim");
}

/// wolf-lang#476's second shape: `grow(mut xs, total(xs))`.
#[test]
fn read_total_whole_claim() {
    run("read_total_whole_claim");
}

/// under two earlier claims (EG2's pair), a later operand reads both claimed elements.
#[test]
fn read_two_claims() {
    run("read_two_claims");
}

/// a bare `Copy` place passed `read` is a read, not a lend (`[mem.tier0.excl.4]`): `bump(mut a, a)`, D39's direct form.
#[test]
fn read_copy_direct() {
    run("read_copy_direct");
}

/// a bare `Copy` element of the claimed container passed `read`: `grow(mut xs, xs[1])`.
#[test]
fn read_copy_elem_direct() {
    run("read_copy_elem_direct");
}

/// a bare `Copy` field of the claimed struct passed `read`: `ring_drop(mut r, r.head)`.
#[test]
fn read_copy_field_direct() {
    run("read_copy_field_direct");
}

/// a bare `str` field (a `Copy` value) of the claimed struct passed `read`: `fail(mut fl, fl.store)`.
#[test]
fn read_copy_str_direct() {
    run("read_copy_str_direct");
}

// -- the writes, moves, re-claims and lends #17 keeps refused -------------

/// still a trap: a later argument WRITES the claimed place, `bump(mut a, { a = 5; 1 })`.
#[test]
fn stay_block_write() {
    run("stay_block_write");
}

/// still a trap: a borrow of the claimed place inside a later argument is a lend held past the argument.
#[test]
fn stay_borrow_lend() {
    run("stay_borrow_lend");
}

/// still a trap: a later argument writes the claimed element through the container, `xs[0] += 1`.
#[test]
fn stay_elem_write() {
    run("stay_elem_write");
}

/// still a trap: a lend into the call BEFORE the claim, `second(xs, mut xs)` — the read lend is held for the whole call.
#[test]
fn stay_lend_before_claim() {
    run("stay_lend_before_claim");
}




/// still a trap: the whole container lent into the same call as its claimed element, `both(mut xs[0], xs)`.
#[test]
fn stay_lend_whole_elem_claim() {
    run("stay_lend_whole_elem_claim");
}

/// still refused: a later argument's block moves the claimed place out (`move`).
#[test]
fn stay_move_block() {
    run("stay_move_block");
}

/// still refused: a later argument moves a FIELD of the claimed struct into a nested call.
#[test]
fn stay_move_field_nested() {
    run("stay_move_field_nested");
}

/// still a trap: a later argument moves the claimed place into a nested call.
#[test]
fn stay_move_nested() {
    run("stay_move_nested");
}

/// still a trap: inside an inner call's argument, a write of the OUTER call's claimed place.
#[test]
fn stay_nested_write_outer() {
    run("stay_nested_write_outer");
}

/// still a trap: a later argument mutates the claimed container through a `mut` receiver.
#[test]
fn stay_push_write() {
    run("stay_push_write");
}

/// still a trap: the same place claimed `mut` twice in one call.
#[test]
fn stay_reclaim_direct() {
    run("stay_reclaim_direct");
}

/// still a trap: a nested call claims the outer call's claimed place again, `bump(mut a, inc(mut a))`.
#[test]
fn stay_reclaim_nested() {
    run("stay_reclaim_nested");
}

/// still a trap: a nested call claims the WHOLE container while its element is claimed.
#[test]
fn stay_reclaim_prefix_nested() {
    run("stay_reclaim_prefix_nested");
}

/// still a trap: a later `take` argument moves the claimed place (`take` arguments are unchanged).
#[test]
fn stay_take_arg() {
    run("stay_take_arg");
}

/// still a trap: a non-`Copy` field of the claimed struct lent into the same call, `fail(mut fl, fl.store)` with `store: List[int]`.
#[test]
fn stay_lend_list_field() {
    run("stay_lend_list_field");
}

/// still a trap: the whole struct lent into the same call beside its claimed field, `bump(mut p.x, p)`.
#[test]
fn stay_lend_struct() {
    run("stay_lend_struct");
}

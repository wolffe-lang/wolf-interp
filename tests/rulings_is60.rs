//! is60 — the maintainer's ruling of 2026-09-30 on wolffe-lang/wolf-lang#474
//! and wolffe-lang/wolf-interp#149: `len`, `count` and `is_empty` read only a
//! container's header (`[mem.model.place.elem]` 1(c)); every other method —
//! `push`, `get`, `pop`, a slice, an impl method's `self` — reads the whole
//! container. With it, two issues of the same neighbourhood: #152 (an element
//! read inside an expression checks the element's own path, 1(a)) and #151
//! (a member read's base operand runs once, `[mem.model.order]`).
//!
//! One directory per shape under `tests/rulings_is60/`, each with an
//! `expected.toml` whose `[expected]` lupin cell is the ruled answer and whose
//! `[trunk]` table is what lupin at trunk `c68c4d3` and wolf 0.2.18 answered.
//! The `twin_*` rows keep trunk's trap, clause and span. A cell's `stdout` is
//! asserted whatever the verdict (an empty one means nothing was printed), so
//! the output before a trap and an operand's evaluation count are pinned too.
//!
//! The runner is is59's (`tests/rulings_is59.rs`), with that one widening.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is60")
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
        .join("rulings_is60")
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

// -- H: the header methods under an element claim (wolf-lang#474) ------

/// `bump(mut xs[0], xs.count())` — wolf-lang#474's witness.
#[test]
fn header_count_under_elem_claim() {
    run("header_count_under_elem_claim");
}

/// `flag(mut xs[0], xs.is_empty())`.
#[test]
fn header_is_empty_under_elem_claim() {
    run("header_is_empty_under_elem_claim");
}

/// `xs.len()`, lupin's method spelling of the header (wolf: E0301).
#[test]
fn header_len_method_under_elem_claim() {
    run("header_len_method_under_elem_claim");
}

/// a `Map`'s header: `bump(mut m["a"], m.count())`.
#[test]
fn header_map_count_under_key_claim() {
    run("header_map_count_under_key_claim");
}

/// `flag(mut m["a"], m.is_empty())`.
#[test]
fn header_map_is_empty_under_key_claim() {
    run("header_map_is_empty_under_key_claim");
}

/// one level up: `bump(mut g[1][2], g.count())`.
#[test]
fn header_outer_count_under_nested_claim() {
    run("header_outer_count_under_nested_claim");
}

/// through a field: `bump(mut b.xs[2], b.xs.count())`.
#[test]
fn header_field_list_count_under_claim() {
    run("header_field_list_count_under_claim");
}

/// inside an operand: `xs.count() * 10 + xs.count()`.
#[test]
fn header_count_in_expr_under_claim() {
    run("header_count_in_expr_under_claim");
}

/// under a borrow's claim: `&mut xs[0]`, then `xs.count()`.
#[test]
fn header_count_under_borrow_claim() {
    run("header_count_under_borrow_claim");
}

/// under EG2's pair: `add2(mut xs[0], mut xs[1], xs.count())`.
#[test]
fn header_count_under_two_claims() {
    run("header_count_under_two_claims");
}

/// s185's corpus row `elem_header_methods_under_claim.lu` (wolf-lang `ecaf655a`), verbatim below this line.
#[test]
fn header_methods_under_claim_s185() {
    run("header_methods_under_claim_s185");
}

/// control, green at trunk: `g[0].count()` under `mut g[1][0]`.
#[test]
fn header_elem_count_sibling_claim() {
    run("header_elem_count_sibling_claim");
}

/// twin, still a trap: `xs.count()` while the WHOLE `xs` is claimed.
#[test]
fn twin_count_whole_claim() {
    run("twin_count_whole_claim");
}

/// twin, still a trap: `g[0].count()` while `g[0]` itself is claimed.
#[test]
fn twin_elem_count_same_elem_claim() {
    run("twin_elem_count_same_elem_claim");
}

/// twin, still a trap: `get` reads the whole container, under `mut xs[0]`.
#[test]
fn twin_get_under_elem_claim() {
    run("twin_get_under_elem_claim");
}

/// twin, still a trap: `last` reads the whole container, under `mut xs[0]`.
#[test]
fn twin_last_under_elem_claim() {
    run("twin_last_under_elem_claim");
}

/// twin, still a trap: an impl method NAMED `count` takes the whole `self`.
#[test]
fn twin_impl_count_under_field_claim() {
    run("twin_impl_count_under_field_claim");
}

// -- E: an element read inside an expression (wolf-interp#152) ---------

/// `bump(mut xs[1], xs[0] + 1)` — wolf-interp#152's witness.
#[test]
fn elem_sibling_in_expr() {
    run("elem_sibling_in_expr");
}

/// two siblings in one operand: `xs[0] * 10 + xs[2]`.
#[test]
fn elem_two_siblings_in_expr() {
    run("elem_two_siblings_in_expr");
}

/// nested, the other row: `g[0][1] + 1` under `mut g[1][0]`.
#[test]
fn elem_nested_sibling_row() {
    run("elem_nested_sibling_row");
}

/// nested, the same row: `g[0][1] + 1` under `mut g[0][0]`.
#[test]
fn elem_nested_same_row() {
    run("elem_nested_same_row");
}

/// run-time indices: `xs[j] + 1` under `mut xs[i]`, `i != j`.
#[test]
fn elem_dyn_indices() {
    run("elem_dyn_indices");
}

/// interpolation holes: `"{xs[0]}{xs[2]}"` under `mut xs[1]`.
#[test]
fn elem_in_interp() {
    run("elem_in_interp");
}

/// through a field: `b.xs[0] + 1` under `mut b.xs[1]`.
#[test]
fn elem_field_list_sibling() {
    run("elem_field_list_sibling");
}

/// under a borrow's claim: `&mut xs[1]`, then `xs[0] + 1`.
#[test]
fn elem_under_borrow_claim() {
    run("elem_under_borrow_claim");
}

/// an index operand with an effect: `xs[f()] + 1`, `f` → 0, once.
#[test]
fn elem_effect_index() {
    run("elem_effect_index");
}

/// beside EG2's pair: `add2(mut xs[0], mut xs[1], xs[2] + 1)`.
#[test]
fn elem_two_claims_third() {
    run("elem_two_claims_third");
}

/// under `#![index(1)]`: `xs[1] + 1` under `mut xs[3]`.
#[test]
fn elem_origin1_sibling() {
    run("elem_origin1_sibling");
}

/// a `Map`: `"{m["b"]}"` under `mut m["a"]`.
#[test]
fn elem_map_sibling_interp() {
    run("elem_map_sibling_interp");
}

/// twin, still a trap: `xs[1] + 1` under `mut xs[1]`.
#[test]
fn twin_same_elem_in_expr() {
    run("twin_same_elem_in_expr");
}

/// twin, still a trap: `xs[f()] + 1`, `f` → 1 — the index runs first, as the bare form's does.
#[test]
fn twin_effect_index_same_elem() {
    run("twin_effect_index_same_elem");
}

/// twin, still a trap: `g[0][1] + 1` under `mut g[0]`.
#[test]
fn twin_row_claim_elem_read() {
    run("twin_row_claim_elem_read");
}

/// twin, still a trap: `xs[0] + 1` under `mut xs`.
#[test]
fn twin_whole_claim_elem_read() {
    run("twin_whole_claim_elem_read");
}

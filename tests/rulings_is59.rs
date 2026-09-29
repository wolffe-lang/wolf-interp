//! is59 — wolffe-lang/wolf-lang#472, ruled A (2026-09-29): a member read of a
//! container is a read of the MEMBER, not of the whole container.
//!
//! `[mem.model.place.elem]` item 1(c), at wolf-lang `d3bd49cc`: "An index step
//! and a member step on the same container: `xs[i]` and `xs.len`, whatever
//! `i` is — an element is never the container's header." The clause applies
//! its distinct shapes to every rule that asks whether two paths conflict,
//! `[mem.tier0.excl]` included; the ruling says it holds under a claim too.
//! Through 0.1.41 lupin read `xs.len` by reading `xs` first, so
//! `bump(mut xs[0], xs.len)` trapped `exclusivity`.
//!
//! One directory per shape under `tests/rulings_is59/`, each with an
//! `expected.toml` whose `[expected]` lupin cell is the ruled answer and whose
//! `[trunk]` table is what 0.1.41 and wolf 0.2.18 answered. The `member_*`
//! rows run (both routes a member is read by — a dotted path `xs.len`, and a
//! projected base `g[0].len` / `(xs).len` — on a `List` and a `Map`, under a
//! call's claim and a borrow's); the `twin_*` rows still trap, and their cell
//! pins trunk's record (clause and span) so the fix is seen not to reach them.
//!
//! The runner is s182's (`tests/rulings_s182.rs`) by way of is56's
//! (`tests/rulings_eg00.rs`): the built binary's `conform-run main.lu --json`
//! from a scratch copy of the witness's own directory.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is59")
        .join(name)
}

/// The ruled lupin cell: the verdict, the stdout an `exit` must print, and
/// for a trap the clause and span its record must carry.
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
        .join("rulings_is59")
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
    if verdict.starts_with("exit(") {
        assert_eq!(
            record["stdout_inline"].as_str(),
            ruled.stdout.as_deref(),
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

// -- ruled A: the member is read, not the container ---------------------

/// wolf-lang#472's witness, eg02's corpus row verbatim:
/// `bump(mut xs[0], xs.len)`.
#[test]
fn elem_member_read_after_mut() {
    run("elem_member_read_after_mut");
}

/// A `Map`'s header: `bump(mut m["a"], m.len)`.
#[test]
fn member_map_len_under_key_claim() {
    run("member_map_len_under_key_claim");
}

/// The member route: `bump(mut g[0][1], g[0].len)`.
#[test]
fn member_elem_len_under_nested_claim() {
    run("member_elem_len_under_nested_claim");
}

/// The member route, the sibling claimed: `bump(mut g[1][0], g[0].len)`.
#[test]
fn member_sibling_elem_len_under_claim() {
    run("member_sibling_elem_len_under_claim");
}

/// One level up: `bump(mut g[1][2], g.len)`.
#[test]
fn member_outer_len_under_nested_claim() {
    run("member_outer_len_under_nested_claim");
}

/// Through a field: `bump(mut b.xs[2], b.xs.len)`.
#[test]
fn member_field_list_len_under_claim() {
    run("member_field_list_len_under_claim");
}

/// "Whatever `i` is": `bump(mut xs[i], xs.len)`.
#[test]
fn member_len_under_dyn_index_claim() {
    run("member_len_under_dyn_index_claim");
}

/// Under EG2's pair: `add2(mut xs[0], mut xs[1], xs.len)`.
#[test]
fn member_len_under_two_claims() {
    run("member_len_under_two_claims");
}

/// Inside an argument expression: `xs.len * 10 + xs.len`.
#[test]
fn member_len_in_expr_under_claim() {
    run("member_len_in_expr_under_claim");
}

/// The member route with a grouped base: `(xs).len`.
#[test]
fn member_grouped_base_under_claim() {
    run("member_grouped_base_under_claim");
}

/// Inside an interpolation: `"{xs.len}{xs.len}"`.
#[test]
fn member_len_in_interp_under_claim() {
    run("member_len_in_interp_under_claim");
}

/// A borrow's claim, not a call's: `let r = &mut xs[0]`, then `xs.len`.
#[test]
fn member_len_under_borrow_claim() {
    run("member_len_under_borrow_claim");
}

/// Control: `xs.len` read before `xs[0]` is claimed ran before the ruling.
#[test]
fn member_len_before_claim() {
    run("member_len_before_claim");
}

// -- twins: still one place, still trunk's trap -------------------------

/// The whole container claimed: `grow(mut xs, xs.len)`.
#[test]
fn twin_whole_mut_member_read() {
    run("twin_whole_mut_member_read");
}

/// The element itself claimed, its member read: `trim(mut g[0], g[0].len)`.
#[test]
fn twin_elem_mut_its_member_read() {
    run("twin_elem_mut_its_member_read");
}

/// A field claimed, its member read: `grow(mut b.xs, b.xs.len)`.
#[test]
fn twin_field_mut_member_read() {
    run("twin_field_mut_member_read");
}

/// No member: `bump(mut xs[0], xs[0])`.
#[test]
fn twin_same_elem_read() {
    run("twin_same_elem_read");
}

/// No member, the whole read: `sum(mut xs[0], xs)`.
#[test]
fn twin_whole_read_under_elem_claim() {
    run("twin_whole_read_under_elem_claim");
}

//! is74 — lupin's halves of kw07 (wolf-interp#185: `read_volatile`,
//! `write_volatile`, E1307, row L3) and kw11 (wolf-interp#194: the nine
//! raw-pointer atomics, `Order` as syntax, `fence`, E1308, E1309, row L4),
//! each a row where wolf 0.2.24 (wolf-lang `294d626d`, the release that
//! carries both) answers on its lanes and lupin at trunk `5f8715e` answered
//! otherwise — or a control both answer alike.
//!
//! One directory per row under `tests/rulings_is74/`, each with a `main.lu`
//! and an `expected.toml`, the shape is70 set. `[expected]`'s `lupin` cell is
//! the answer this machine must give: the compiler's checked answer; where
//! the checked machine runs no tasks (C1), the compiled tiers' answer, or
//! `trap(race)` for a data race; a `ub(mem.ub)` cell names its row and
//! clause and no diagnostic (lupin's ub records carry none — wolf-lang's
//! `raw_align_lanes.rs`, `L4_LUPIN`). Only the first diagnostic is compared
//! (`[proto.record.first]`). `[trunk]` records what lupin at trunk and the
//! compiler's `--checked`/`--native`/`--release` answered on kasumi
//! (`~/lanes/is74/scripts/mkexp.py`), so every row's red-at-trunk is on file
//! beside it. Rows copied from wolf 0.2.24's corpus keep their names; the
//! rest are this lane's probes.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is74")
        .join(name)
}

fn quoted_after(line: &str, key: &str) -> Option<String> {
    let rest = line.split(key).nth(1)?;
    let rest = rest.strip_prefix('"')?;
    Some(rest.split('"').next()?.replace("\\n", "\n"))
}

struct Ruled {
    verdict: String,
    stdout: String,
    /// `CODE@[lo,hi]`, empty when the row runs.
    first: String,
    /// A substring `x-unsupported` must carry, when the row is refused by
    /// name.
    named: String,
    /// Every diagnostic, `CODE@[lo,hi]` comma-joined, when the compiler
    /// reports more than one; empty otherwise.
    all: String,
    /// A `ub(mem.ub)` record's `x-ub-row` and `x-ub-clause`.
    row: String,
    clause: String,
    args: Vec<String>,
}

/// `[expected]`'s `lupin = { verdict, stdout, first }` — one inline table on
/// one line — and an optional top-level `args = "…"`.
fn ruled(name: &str) -> Ruled {
    let text = std::fs::read_to_string(witness(name).join("expected.toml"))
        .expect("the witness carries its expected.toml");
    let mut section = String::new();
    let mut args = Vec::new();
    let mut cell = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line.to_owned();
            continue;
        }
        if section.is_empty() && line.starts_with("args") {
            args = quoted_after(line, "args = ")
                .expect("args is a string")
                .split_whitespace()
                .map(str::to_owned)
                .collect();
        }
        if section == "[expected]" && line.starts_with("lupin") {
            cell = Some(line.to_owned());
        }
    }
    let cell = cell.unwrap_or_else(|| panic!("{name}/expected.toml has no [expected] lupin cell"));
    Ruled {
        verdict: quoted_after(&cell, "verdict = ").expect("a verdict"),
        stdout: quoted_after(&cell, "stdout = ").expect("a stdout"),
        first: quoted_after(&cell, "first = ").unwrap_or_default(),
        named: quoted_after(&cell, "named = ").unwrap_or_default(),
        all: quoted_after(&cell, "all = ").unwrap_or_default(),
        row: quoted_after(&cell, "row = ").unwrap_or_default(),
        clause: quoted_after(&cell, "clause = ").unwrap_or_default(),
        args,
    }
}

fn run(name: &str, issue: &str) {
    let want = ruled(name);
    let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["conform-run", "main.lu", "--json"])
        .args(&want.args)
        .current_dir(witness(name))
        .output()
        .expect("lupin runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{name} ({issue}): the tool answers with a record: {output:?}"
    );
    let record: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON record");
    let first = record["diagnostics"]
        .as_array()
        .and_then(|ds| ds.first())
        .map(|d| {
            let span = &d["span"];
            format!(
                "{}@[{},{}]",
                d["code"].as_str().unwrap_or(""),
                span[0],
                span[1]
            )
        })
        .unwrap_or_default();
    assert_eq!(
        (
            record["verdict"].as_str().unwrap_or(""),
            record["stdout_inline"].as_str().unwrap_or(""),
            first.as_str(),
        ),
        (
            want.verdict.as_str(),
            want.stdout.as_str(),
            want.first.as_str()
        ),
        "{name} ({issue}): record {record}"
    );
    if !want.all.is_empty() {
        let all = record["diagnostics"]
            .as_array()
            .map(|ds| {
                ds.iter()
                    .map(|d| {
                        format!(
                            "{}@[{},{}]",
                            d["code"].as_str().unwrap_or(""),
                            d["span"][0],
                            d["span"][1]
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        assert_eq!(all, want.all, "{name} ({issue}): every diagnostic");
    }
    if !want.row.is_empty() {
        assert_eq!(
            (
                record["x-ub-row"].as_str().unwrap_or(""),
                record["x-ub-clause"].as_str().unwrap_or("")
            ),
            (want.row.as_str(), want.clause.as_str()),
            "{name} ({issue}): the UB row and its clause"
        );
    }
    let reason = record["x-unsupported"].as_str().unwrap_or("");
    assert!(
        reason.contains(&want.named),
        "{name} ({issue}): x-unsupported {reason:?} names {:?}",
        want.named
    );
}

fn each(names: &[&str], issue: &str) {
    for name in names {
        run(name, issue);
    }
}

// ---- wolf-interp#185: kw07's volatile access ------------------------------

/// `[mem.unsafe.volatile]`, `.1`, `.3`: every admitted pointee written and
/// read back through one allocation, the widths overlapping, the two
/// spellings mixed; `*byte`; an unused read; aligned offsets. Red at trunk:
/// `unsupported` ("`*T` has no method `write_volatile`").
#[test]
fn every_admitted_pointee_reads_back_what_it_wrote() {
    each(
        &[
            "volatile_widths",
            "volatile_byte_roundtrip",
            "volatile_unused_read",
            "volatile_aligned_ok",
        ],
        "wolf-interp#185",
    );
}

/// `[mem.unsafe.volatile]`: either method outside the ring is E1301 at the
/// whole call. Red at trunk: `unsupported`.
#[test]
fn a_volatile_access_needs_the_ring() {
    each(
        &["volatile_outside_unsafe", "volatile_write_outside_unsafe"],
        "wolf-interp#185",
    );
}

/// `[mem.unsafe.volatile.1]`: `int`, `uint`, `bool` and a float are not
/// one machine access — E1307 at the method's name, the pointee read off a
/// local's cast, a parameter's type, or the receiver's own cast. Red at
/// trunk: `unsupported`.
#[test]
fn a_pointee_that_is_not_one_access_is_e1307() {
    each(
        &[
            "volatile_pointee_bool",
            "volatile_pointee_int",
            "volatile_pointee_uint",
            "volatile_pointee_f64",
            "volatile_param_int",
            "volatile_cast_bool",
        ],
        "wolf-interp#185",
    );
}

/// `[mem.unsafe.volatile.3]`: on an allocation a volatile access is an
/// ordinary access — a read after `c.free` is P1, one past the end P3.
#[test]
fn on_an_allocation_a_volatile_access_is_ordinary() {
    each(
        &["volatile_ub_uaf", "volatile_out_of_bounds"],
        "wolf-interp#185",
    );
}

/// `[mem.unsafe.volatile.3]`: an address that is not a multiple of the
/// pointee's size is row L3, clause `mem.unsafe.volatile`, read or write.
#[test]
fn a_misaligned_volatile_access_is_row_l3() {
    each(
        &["volatile_ub_misaligned", "volatile_misaligned_write"],
        "wolf-interp#185",
    );
}

// ---- wolf-interp#194: kw11's atomics ---------------------------------------

/// `[conc.mm.atomic.raw]`, `.2`, `.5`: every operation on every width,
/// every admitted order (each answers what `seq_cst` answers here), the
/// operands left to right after the receiver, aligned offsets. Red at
/// trunk: `unsupported` ("`Order.seq_cst` does not resolve").
#[test]
fn every_operation_answers_on_every_width_and_order() {
    each(
        &[
            "atomic_widths",
            "atomic_orders",
            "atomic_operands_in_order",
            "atomic_aligned_ok",
        ],
        "wolf-interp#194",
    );
}

/// `[conc.mm.fence]`: the four admitted fences; `seq_cst` in a fn with no
/// `unsafe`; a module fn named `fence` shadows the builtin (W0304).
#[test]
fn every_admitted_fence_runs() {
    each(
        &["atomic_fence", "fence_seq_cst_safe", "fence_shadowed"],
        "wolf-interp#194",
    );
}

/// `[conc.mm.atomic.raw]`, `[conc.mm.fence]`: an atomic outside the ring,
/// and a fence weaker than `seq_cst`, are E1301 at the whole call — in
/// source order across fns. Red at trunk: `unsupported`, or `exit(0)` for
/// an uncalled fn.
#[test]
fn atomics_and_weak_fences_need_the_ring() {
    each(
        &[
            "atomic_outside_unsafe",
            "atomic_store_outside_unsafe",
            "atomic_cas_outside_unsafe",
            "fence_release_outside_unsafe",
            "fence_acq_rel_outside_unsafe",
        ],
        "wolf-interp#194",
    );
}

/// `[conc.mm.atomic.raw.1]`: `int`, `uint`, `byte`, `bool` and a float
/// are E1308 at the method's name, before an order on the same call.
#[test]
fn a_pointee_that_is_not_a_fixed_width_integer_is_e1308() {
    each(
        &[
            "atomic_pointee",
            "pointee_int_alone",
            "pointee_uint_alone",
            "pointee_byte_alone",
            "pointee_bool_alone",
            "pointee_f64_alone",
            "pointee_param_int",
            "pointee_before_order",
        ],
        "wolf-interp#194",
    );
}

/// `[conc.mm.atomic.order]`, `[conc.mm.atomic.raw.2]`, `[conc.mm.fence]`:
/// each order an operation does not admit, an operand that is not a mark,
/// a mark that is not one of the five, and `Order` used as a value — E1309
/// at the compiler's span for each.
#[test]
fn an_order_the_operation_does_not_admit_is_e1309() {
    each(
        &[
            "atomic_order_misuse",
            "order_load_release",
            "order_load_acq_rel",
            "order_store_acquire",
            "order_store_acq_rel",
            "order_cas_fail_stronger",
            "order_cas_fail_acquire_unacquiring",
            "order_cas_fail_release",
            "order_cas_fail_acq_rel",
            "order_cas_fail_seq_cst_under_acq_rel",
            "order_cas_success_bad_mark",
            "order_fence_relaxed",
            "order_fence_unknown_mark",
            "order_as_value",
            "order_from_variable",
            "order_from_call",
            "order_unknown_mark",
        ],
        "wolf-interp#194",
    );
}

/// `[conc.mm.atomic.raw.4]`: a misaligned atomic is row L4, clause
/// `mem.unsafe.raw.4` — load, store, add, compare-and-swap.
#[test]
fn a_misaligned_atomic_is_row_l4() {
    each(
        &[
            "atomic_ub_misaligned",
            "atomic_misaligned_u16_load",
            "atomic_misaligned_u32_store",
            "atomic_misaligned_i64_cas",
        ],
        "wolf-interp#194",
    );
}

/// `[conc.mm.atomic.raw]`: an atomic is an access through `p` for every
/// `[mem.ub]` row — P1 after `c.free`, P3 past the end.
#[test]
fn on_an_allocation_an_atomic_is_an_access() {
    each(
        &["atomic_ub_uaf", "atomic_out_of_bounds"],
        "wolf-interp#194",
    );
}

/// `[conc.mm.atomic.raw.5]`: tasks running atomics under this machine's
/// scheduler give the compiled tiers' exact counts — four tasks'
/// `atomic_add`s and four tasks' CAS loops; two tasks churning one word.
#[test]
fn tasks_count_exactly() {
    each(
        &["atomic_counter", "atomic_two_tasks_one_word"],
        "wolf-interp#194",
    );
}

/// `[conc.mm.atomic.order]`: a plain write published by a release store and
/// read after the acquire load that saw it is ordered — not a race.
#[test]
fn an_atomic_publication_orders_plain_accesses() {
    run("atomic_publish_no_race", "wolf-interp#194");
}

/// `[conc.mm.race.1]`: the plain-increment twin still traps `race`, and so
/// does a plain read unordered with an atomic store — only two ATOMIC
/// accesses never race.
#[test]
fn the_racy_twins_still_trap() {
    each(
        &["atomic_race_plain", "atomic_vs_plain_race"],
        "wolf-interp#194",
    );
}

/// Every witness directory is named by a test above.
#[test]
fn every_witness_is_run() {
    let source = include_str!("rulings_is74.rs");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is74");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(root).expect("the witnesses") {
        let name = entry
            .expect("an entry")
            .file_name()
            .into_string()
            .expect("utf-8");
        if !source.contains(&format!("\"{name}\"")) {
            missing.push(name);
        }
    }
    assert!(missing.is_empty(), "witnesses no test runs: {missing:?}");
}

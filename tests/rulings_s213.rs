//! s213 — lupin's half of the kernel papercuts (wolf-interp#200): wolf-lang
//! #579 (`[mem.static.4]`, a module item read and written through its
//! module's name), #577 (`[mem.unsafe.raw.5]`, a field of a raw element
//! read and stored in place), #575 (`[type.int.not]`, `!` on an integer is
//! its complement — a spelling the maintainer has yet to rule on) and #572
//! (`[type.fn.never]`, `-> never` — likewise a ruling owed).
//!
//! One directory per row under `tests/rulings_s213/`, each with a `main.lu`
//! (and a module directory where the row needs one) and an `expected.toml`,
//! the shape is70 set. Nineteen rows are wolf-lang s213's corpus rows
//! verbatim (`13b5315a`); three are this lane's probes. `[expected]`'s
//! `lupin` cell is the compiler's checked answer (the s213 head build,
//! `d53e801b…`), a `ub` record held to its verdict and row (lupin's record
//! carries no diagnostic on a `ub` verdict); `[trunk]` records what lupin
//! 0.1.47 and the compiler's checked machine answered on kasumi
//! (`~/lanes/s213/lupin-side/mkexp.py`), so every red-at-trunk is on file.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_s213")
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
    /// The `x-ub-row` a `ub(mem.ub)` record must carry (s213).
    row: String,
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
    assert_eq!(
        record["x-ub-row"].as_str().unwrap_or(""),
        want.row,
        "{name} ({issue}): the UB row, record {record}"
    );
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

// ---- wolf-lang#579: `m.K` is the module's item ([mem.static.4]) ---------

/// A `pub var` read and written through its module's name, inside
/// `unsafe`; and the in-module write of a non-root module's `var` before
/// any read of it. Red at trunk: "`counter` is not a local place",
/// "`COUNT` is not a local place". The `const` and `let` rows ran at trunk
/// and stay (controls).
#[test]
fn a_module_item_reads_and_writes_through_its_module_name() {
    each(
        &[
            "static_qualified_var",
            "static_var_own_module_write",
            "static_qualified",
            "static_qualified_let",
        ],
        "wolf-lang#579",
    );
}

/// A qualified `var` read outside `unsafe` is E1301 at the qualified
/// name, as the bare name is. Red at trunk: `exit(0)`, no ring.
#[test]
fn a_qualified_module_var_outside_unsafe_is_e1301() {
    run("static_qualified_var_outside_unsafe", "wolf-lang#579");
}

// ---- wolf-lang#577: a field of a raw element ([mem.unsafe.raw.5]) --------

/// `p[i].f = v`, compound, nested, `(*p).f`, a packed `u64` at offset 2:
/// stored and read in place at the C layout. Red at trunk: "does not
/// denote a place at run time" (0.1.47: the packed row E0817).
#[test]
fn a_field_of_a_raw_element_is_stored_in_place() {
    each(
        &[
            "raw_field_store",
            "raw_field_store_compound",
            "raw_field_store_nested",
            "raw_field_store_packed",
        ],
        "wolf-lang#577",
    );
}

/// The store asks row L4 of the ELEMENT at the struct's alignment: a
/// plain `#[repr(c)]` `{u32, u64}` four bytes past an aligned base is L4
/// at `s[0].a = 1`. Red at trunk: no place.
#[test]
fn a_misaligned_element_field_store_is_row_l4() {
    run("raw_ub_misaligned_repr_c_field_store", "wolf-lang#577");
}

/// Controls: outside `unsafe` the store is E1301 (as at trunk); a whole
/// `#[repr(c)]` element through `p[i]` is refused by name (at trunk it
/// read one byte and failed at the member).
#[test]
fn the_ring_and_the_whole_aggregate_stay_refused() {
    run("raw_field_store_outside_unsafe", "wolf-lang#577");
    run("raw_struct_whole_load", "wolf-lang#577");
}

// ---- wolf-lang#575: `!` on an integer ([type.int.not], ruling owed) ------

/// The complement at the operand's width: the page-table masks at `u32`,
/// signed widths, a byte widened to `int`, a complemented literal adopting
/// an unsigned binding. Red at trunk: "`!` needs a bool".
#[test]
fn bang_on_an_integer_is_its_complement() {
    each(
        &[
            "int_not_mask",
            "int_not_signed",
            "int_not_byte",
            "int_not_unsigned_binding",
        ],
        "wolf-lang#575",
    );
}

/// `!` on a float is E0409 at the operand. Red at trunk: `unsupported`.
#[test]
fn bang_on_a_float_is_e0409() {
    run("int_not_float", "wolf-lang#575");
}

// ---- wolf-lang#572: `-> never` ([type.fn.never], ruling owed) ------------

/// A call to a `-> never` fn fits a handler arm where a value is wanted;
/// its trap twin stops at the `never` body's `assert(false)`; a bodyless
/// `extern "c" fn` declared `-> never` (a control: it ran at trunk). Red
/// at trunk: E0301 at `never`.
#[test]
fn a_never_fn_call_is_bottom() {
    each(
        &["fn_never_handler_arm", "fn_never_trap", "fn_never_extern"],
        "wolf-lang#572",
    );
}

/// A `-> never` body that can reach its end, or that holds a `return`, is
/// E0401 at the tail or at the `return`. Red at trunk: E0301.
#[test]
fn a_never_body_that_returns_is_e0401() {
    each(
        &["fn_never_reaches_end", "fn_never_return"],
        "wolf-lang#572",
    );
}

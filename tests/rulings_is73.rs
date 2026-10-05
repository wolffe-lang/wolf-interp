//! is73 — the re-pin blockers: lupin's halves of kw09 (wolf-interp#190:
//! module state, `extern "c" let`, `#[section]`) and kw08 (wolf-interp#188:
//! `packed`, `align(N)`, the layout queries, E0819, E0820), each a row where
//! wolf 0.2.23 (wolf-lang `8edac3ee`, the release lupin re-pins on) answers on
//! all three of its lanes and lupin 0.1.46 answered otherwise — or a control
//! both answer alike.
//!
//! One directory per row under `tests/rulings_is73/`, each with a `main.lu`
//! and an `expected.toml`, the shape is70 set (`tests/rulings_is70.rs`).
//! `[expected]`'s `lupin` cell is the answer this machine must give — the
//! compiler's checked answer, or a refusal by name where it is
//! `unsupported`; `[trunk]` records what lupin 0.1.46 and the compiler's
//! `--checked`/`--native`/`--release` answered on kasumi
//! (`~/lanes/is73/scripts/mkexp.py`), so every row's red-at-trunk is on file
//! beside it. Rows copied from wolf 0.2.23's corpus keep their names; the
//! rest are this lane's probes.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is73")
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

// ---- wolf-interp#190: a module initializer is comptime (E0705) ------------

/// A cycle among initializers is E0705 at every reference that closes it —
/// once per evaluation that reaches it, as the compiler's engine reports.
/// Red at trunk: lupin 0.1.46 overflowed its stack (SIGABRT, no record).
#[test]
fn an_initializer_cycle_is_e0705() {
    each(
        &[
            "static_init_cycle",
            "static_init_three",
            "static_init_self",
            "static_init_depends_on_cycle",
            "static_init_const_cycle",
            "static_init_cycle_and_var",
        ],
        "wolf-interp#190",
    );
}

/// A cycle the static walk cannot see — through a fn's body — is declined
/// by name at run time, never a recursion into the stack's end. Red at
/// trunk: `unsupported`, the 512-frame call rail, naming no cycle.
#[test]
fn a_cycle_through_a_fn_is_declined_by_name() {
    run("static_init_through_fn", "wolf-interp#190");
}

/// An initializer that reads a `var` is E0705; one that calls a plain fn
/// runs (the compiler's engine folds it). Red at trunk: `exit(0)`.
#[test]
fn reading_a_var_in_an_initializer_is_e0705() {
    each(
        &[
            "static_init_not_comptime",
            "static_var_reads_var",
            "static_init_runtime_fn",
        ],
        "wolf-interp#190",
    );
}

/// No initializer graph overflows the stack: a chain longer than the
/// machine's rails, declared so each item needs the next, and a cycle too
/// long for the static walk to judge, are each answered with a record.
#[test]
fn no_initializer_graph_overflows_the_stack() {
    for (name, n, cyclic) in [("chain", 2000, false), ("cycle", 2000, true)] {
        // One directory each: a directory is a module (D32).
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("rulings_is73_deep_init")
            .join(name);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let mut source = String::new();
        for i in 0..n {
            let next = if i + 1 < n {
                format!("A{} + 1", i + 1)
            } else if cyclic {
                "A0 + 1".to_owned()
            } else {
                "1".to_owned()
            };
            source.push_str(&format!("let A{i}: int = {next}\n\n"));
        }
        source.push_str("fn main() -> int {\n    A0 - A0\n}\n");
        let file = dir.join(format!("{name}.lu"));
        std::fs::write(&file, source).expect("write");
        let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
            .args(["conform-run", file.to_str().expect("utf-8"), "--json"])
            .output()
            .expect("lupin runs");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{name}: a record, not a crash: {output:?}"
        );
        let record: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("one JSON record");
        let verdict = record["verdict"].as_str().unwrap_or("");
        assert!(
            verdict == "unsupported" || verdict == "fail(E0705)" || verdict == "exit(0)",
            "{name}: {record}"
        );
    }
}

// ---- wolf-interp#190: a module `var` is raw-tier (E1301) ------------------

/// Every read and write of a module `var` outside `unsafe` is E1301 at the
/// name; a local of the same name shadows it. Red at trunk: `exit(1)`,
/// `exit(0)`.
#[test]
fn a_module_var_outside_unsafe_is_e1301() {
    each(
        &[
            "static_var_outside_unsafe",
            "static_var_mut_arg",
            "static_var_two_fns",
            "static_var_shadowed",
            "static_var",
            "static_const_let",
        ],
        "wolf-interp#190",
    );
}

/// Module state holds the integers, `byte`, `bool`, the floats and `str` in
/// a `let`: a `List`, a struct, a `str` `var` are refused by name. Red at
/// trunk: `exit(0)`.
#[test]
fn module_state_that_is_not_static_data_is_refused_by_name() {
    each(
        &["static_list_let", "static_struct_let", "static_str_var"],
        "wolf-interp#190",
    );
}

// ---- wolf-interp#190: `extern "c" let` (E0821, refused by name) -----------

/// A program that declares a link-time symbol is refused by name — named or
/// not. Red at trunk: `fail(E0201)` at parse.
#[test]
fn a_link_time_symbol_is_refused_by_name() {
    each(
        &["extern_let_image", "extern_let_unnamed"],
        "wolf-interp#190",
    );
}

/// Every misuse of the form is E0821 at the compiler's span; another ABI
/// string is E0818; `#[section]` on one is E0817. Red at trunk: E0201.
#[test]
fn an_extern_let_misused_is_e0821() {
    each(
        &[
            "extern_let_not_ptr",
            "extern_let_init",
            "extern_var",
            "extern_const",
            "extern_let_in_fn",
            "extern_let_list",
            "extern_let_abi",
            "extern_let_section",
        ],
        "wolf-interp#190",
    );
}

// ---- wolf-interp#190: `#[section]` (refused by name, E0817) ---------------

/// A placed section needs an image: refused by name, the attribute's
/// misuses E0817 as before. Red at trunk: E0817 for the placements.
#[test]
fn a_placed_section_is_refused_by_name() {
    each(
        &[
            "hosted_sections",
            "section_uncalled",
            "section_var",
            "section_bad_args",
            "section_struct",
            "section_local",
            "attr_section",
        ],
        "wolf-interp#190",
    );
}

// ---- wolf-interp#188: the layout queries (E0708, E0403) -------------------

/// `size_of`, `align_of`, `offset_of` answer the C layout at comptime.
/// Red at trunk: E0817 (the attribute) or `unsupported` ("does not
/// resolve").
#[test]
fn the_queries_answer_the_c_layout() {
    each(
        &[
            "layout_query_repr_c",
            "packed_fields_at_offset_of",
            "query_char",
            "query_let_not_const",
            "query_ptr_field",
            "query_mixed_scalars",
            "query_scalars",
            "query_padded_tail",
            "query_in_arithmetic",
            "packed_in_packed_ok",
            "align_in_align_ok",
            "align_smaller_ok",
            "align_2_28_ok",
        ],
        "wolf-interp#188",
    );
}

/// A query on a native layout is E0708 at the call, every one listed; an
/// `offset_of` field the struct lacks is E0403. Red at trunk:
/// `unsupported`.
#[test]
fn a_query_without_a_comptime_layout_is_refused() {
    each(
        &[
            "size_of_layout",
            "align_of_layout",
            "offset_of_layout",
            "query_native_field",
            "query_native_struct_field",
            "query_str",
            "query_enum",
            "query_two_e0708",
            "query_offset_missing_field",
            "query_offset_int_arg",
            "query_offset_of_scalar",
        ],
        "wolf-interp#188",
    );
}

// ---- wolf-interp#188: a packed field is never lent (E0819) ----------------

/// A `mut` lend of a packed field, or a `read` lend of an aggregate one, is
/// E0819 at the place; a copy, a `take`, a scalar by value, a whole packed
/// struct and an aligned struct's field are not lends of a packed field.
/// Red at trunk: E0817.
#[test]
fn a_packed_field_is_never_lent() {
    each(
        &[
            "packed_field_lend",
            "lend_mut_limit",
            "lend_read_aggregate",
            "lend_nested_in_c",
            "lend_two_sites",
            "lend_scalar_by_value",
            "lend_whole_struct",
            "lend_field_write_read",
            "lend_aligned_field",
            "lend_take_field",
            "lend_copy_field",
        ],
        "wolf-interp#188",
    );
}

// ---- wolf-interp#188: a representation that cannot be laid out (E0820) ----

/// E0820 at the compiler's spans, one per struct, beside the closed set's
/// E0817s. Red at trunk: E0817 (or `exit(0)` for `repr(c, c)`).
#[test]
fn an_unlayable_representation_is_e0820() {
    each(
        &[
            "attr_repr_unlayable",
            "unlayable_packed_first",
            "unlayable_align_first",
            "unlayable_align_no_c",
            "unlayable_generic",
            "unlayable_repeat",
            "unlayable_repeat_c",
            "unlayable_align_2_29",
            "unlayable_aligned_in_packed",
            "unlayable_aligned_in_packed_deep",
            "unlayable_mixed_with_e0817",
            "unlayable_align_string",
            "unlayable_align_two_args",
            "unlayable_align_bare",
            "packed_n_e0817",
            "repr_c_generic_ok",
            "repr_two_attrs_ok",
        ],
        "wolf-interp#188",
    );
}

/// The whole-aggregate raw store stays refused by name (kw08's machines
/// clause): this machine has no byte-level aggregate. Red at trunk: E0817.
#[test]
fn a_whole_aggregate_raw_store_is_refused_by_name() {
    each(
        &["raw_repr_packed_layout", "raw_repr_align_layout"],
        "wolf-interp#188",
    );
}

/// Every witness directory is named by a test above.
#[test]
fn every_witness_is_run() {
    let source = include_str!("rulings_is73.rs");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is73");
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

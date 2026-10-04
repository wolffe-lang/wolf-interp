//! is70 — the gaps the PAX lanes filed in lupin (wolf-interp#174, #175,
//! #178, #179, #180, #181, #182, #184), each a row where the compiler at
//! wolf-lang trunk `50830027` answers on all three of its lanes and lupin
//! answered otherwise.
//!
//! One directory per row under `tests/rulings_is70/`, each with a `main.lu`
//! and an `expected.toml`. `[expected]`'s `lupin` cell is the answer this
//! machine must give: the verdict, the stdout, and — where the compiler
//! refuses — the first diagnostic's code and span (`[proto.record.first]`
//! makes the first one comparable). An optional `args` line adds
//! `conform-run` arguments. `[trunk]` records what lupin trunk `8d82031`
//! and the compiler's `--checked`/`--native`/`--release` answered on
//! kasumi (`~/lanes/is70/`), so every row's red-at-trunk is on file beside
//! it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn witness(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("rulings_is70")
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

// ---- wolf-interp#184: `*p` is `p[0]`, under the same ring (E1301) --------

/// `*p = 3` in safe code. Red at trunk: `exit(0)`.
#[test]
fn deref_write_outside() {
    run("deref_write_outside", "wolf-interp#184");
}

/// `let a = *p` in safe code. Red at trunk: `exit(0)`, `0`.
#[test]
fn deref_read_outside() {
    run("deref_read_outside", "wolf-interp#184");
}

/// `*p += 2` in safe code. Red at trunk: `exit(0)`.
#[test]
fn deref_compound_outside() {
    run("deref_compound_outside", "wolf-interp#184");
}

/// The control: all three spellings inside `unsafe` run on every machine.
#[test]
fn deref_inside_ok() {
    run("deref_inside_ok", "wolf-interp#184");
}

// ---- wolf-interp#184: the provenance methods and the integer side ---------
// The `prov_*` and `raw_deref*` programs are the compiler's kw06 corpus rows
// (`corpus/memory/`), byte for byte; `f9_exposed` is kw00's probe from the
// compiler gate `int_ptr_lanes.rs`.

/// `with_addr` keeps the receiver's provenance. Red at trunk: `unsupported`.
#[test]
fn prov_addr_with_addr() {
    run("prov_addr_with_addr", "wolf-interp#184");
}

/// `expose` / `with_exposed` round-trip. Red at trunk: `unsupported`.
#[test]
fn prov_expose_round_trip() {
    run("prov_expose_round_trip", "wolf-interp#184");
}

/// kw00's F9 probe. Red at trunk: `unsupported`.
#[test]
fn f9_exposed() {
    run("f9_exposed", "wolf-interp#184");
}

/// `N as *T` widens by `N`'s sign; `*T as N` is the address. Red at trunk:
/// `0 0 0`.
#[test]
fn prov_narrow_cast() {
    run("prov_narrow_cast", "wolf-interp#184");
}

/// `300 as *u8 as u8` is out of range. Red at trunk: `exit(0)`.
#[test]
fn prov_narrow_cast_trap() {
    run("prov_narrow_cast_trap", "wolf-interp#184");
}

/// Signed widening into the address word, `is_null` on a non-zero address
/// no allocation owns. Red at trunk: `0 0 0 true true`.
#[test]
fn ptr_from_signed() {
    run("ptr_from_signed", "wolf-interp#184");
}

/// The all-ones address read as `i32` is out of range as `uint`. Red at
/// trunk: `exit(0)`, `0`.
#[test]
fn ptr_narrow_signed_trap() {
    run("ptr_narrow_signed_trap", "wolf-interp#184");
}

/// The methods on a pointer no allocation owns. Red at trunk: `unsupported`.
#[test]
fn ptr_offset_foreign() {
    run("ptr_offset_foreign", "wolf-interp#184");
}

/// An access through an address no allocation owns is UB row L2 on a hosted
/// target, as on the checked machine (`[mem.prov.device]`; the gate's
/// `foreign_memory_on_a_hosted_target_is_ub_row_l2`). Green at trunk, and
/// kept green now that such a pointer keeps its address.
#[test]
fn device_hosted() {
    run("device_hosted", "wolf-interp#184");
}

/// Controls, green at trunk and kept green: the round trip into an
/// allocation, `is_null`, prefix `*p`, a signed pointee, and `addr` on a C
/// allocation's pointer whose address is cast back (every machine answers
/// `1`: the C pointer behaves as exposed).
#[test]
fn prov_controls() {
    for row in [
        "prov_cast_round_trip",
        "prov_is_null",
        "raw_deref",
        "raw_deref_signed",
        "addr_no_expose",
    ] {
        run(row, "wolf-interp#184");
    }
}

// ---- wolf-interp#182: `conform-run --target` ------------------------------

/// kw00's `f1_no_main` probe under the freestanding target. Red at trunk: a
/// clap usage error, exit 2, no record.
#[test]
fn target_none() {
    run("target_none", "wolf-interp#182");
}

/// The gate's device program under the freestanding target: refused by
/// name, never `ub`. Red at trunk: no record.
#[test]
fn device_freestanding() {
    run("device_freestanding", "wolf-interp#182");
}

/// The host's own triple runs as if `--target` were absent; any other triple
/// is a tool error (exit 2, no record) naming the two this machine accepts,
/// as the compiler's `unknown target` is.
#[test]
fn target_host_runs_and_others_are_tool_errors() {
    let dir = witness("target_none");
    let host = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["conform-run", "../deref_inside_ok/main.lu", "--json"])
        .args(["--target", wolf_interp::HOST_TRIPLE])
        .current_dir(&dir)
        .output()
        .expect("lupin runs");
    let record: serde_json::Value = serde_json::from_slice(&host.stdout).expect("a record");
    assert_eq!(record["verdict"], "exit(0)", "{record}");
    assert_eq!(record["stdout_inline"], "7\n", "{record}");
    for other in [
        "aarch64-unknown-none",
        "bogus-triple",
        "x86_64-unknown-none-elf",
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
            .args(["conform-run", "main.lu", "--json", "--target", other])
            .current_dir(&dir)
            .output()
            .expect("lupin runs");
        assert_eq!(out.status.code(), Some(2), "{other}: {out:?}");
        assert!(out.stdout.is_empty(), "{other}: no record");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains(&format!("unknown target `{other}`"))
                && err.contains(wolf_interp::HOST_TRIPLE)
                && err.contains(wolf_interp::FREESTANDING_TRIPLE),
            "{other}: {err}"
        );
    }
}

// ---- wolf-interp#178: a `return` out of a region block --------------------
// `[mem.region.escape]`: E1010 on the compiler's three lanes; lupin's answer
// is the dynamic counterpart, `trap(region-fault)` (`[conf.trap.map]`).

/// The issue's program: `return s`. Red at trunk: `exit(0)`, `[regions]`.
#[test]
fn region_return_s() {
    run("region_return_s", "wolf-interp#178");
}

/// `return s.trim()`, a view of the region's bytes. Red at trunk.
#[test]
fn region_return_trim() {
    run("region_return_trim", "wolf-interp#178");
}

/// `for w in s.words() { return w }`. Red at trunk.
#[test]
fn region_return_for_word() {
    run("region_return_for_word", "wolf-interp#178");
}

/// The compiler gate's row, `corpus/memory/region_str_view_for_return.lu`
/// (`region_view_for_lanes::a_piece_returned_from_inside_the_loop_is_refused`).
/// Red at trunk.
#[test]
fn region_view_for_return() {
    run("region_view_for_return", "wolf-interp#178");
}

/// The control: returning a literal, or an `int` read from a region-built
/// `str`, out of a region block runs on every machine.
#[test]
fn region_return_ok() {
    run("region_return_ok", "wolf-interp#178");
}

// ---- wolf-interp#179, the `errdefer` half ----------------------------------
// `[err.errdefer]`: `errdefer` in a function whose result carries no row is
// E0607 at the keyword on the compiler's three lanes.

/// The issue's shape: a unit fn. Red at trunk: `exit(0)`, both deferrals ran.
#[test]
fn errdefer_unit_fn() {
    run("errdefer_unit_fn", "wolf-interp#179");
}

/// An `-> int` fn. Red at trunk: `exit(0)`, `2`.
#[test]
fn errdefer_int_fn() {
    run("errdefer_int_fn", "wolf-interp#179");
}

/// `fn main() -> int`. Red at trunk: `exit(0)`, `body`.
#[test]
fn errdefer_main_int() {
    run("errdefer_main_int", "wolf-interp#179");
}

/// Inside an `if` block of a unit fn. Red at trunk: `exit(0)`, `then`.
#[test]
fn errdefer_nested_block() {
    run("errdefer_nested_block", "wolf-interp#179");
}

/// The control: in a fallible fn `errdefer` runs on the raise, before `defer`.
#[test]
fn errdefer_fallible_ok() {
    run("errdefer_fallible_ok", "wolf-interp#179");
}

// ---- wolf-interp#181: the C membrane (kw02, `[mem.unsafe.sig]`) ------------
// The `raw_ptr_*`, `unsafe_sig` and `extern_*` programs are the compiler's
// kw02 corpus rows (`corpus/memory/`, `corpus/membrane/`), byte for byte.

/// A module-private fn takes `*T` and writes through it; the argument is a
/// copy of the pointer, never a Frozen retag. Red at trunk: `fail(E1302)`.
#[test]
fn raw_ptr_private_sig() {
    run("raw_ptr_private_sig", "wolf-interp#181");
}

/// `mut p: *u8`: the caller's pointer variable. Red at trunk: `fail(E1302)`.
#[test]
fn raw_ptr_mut_param() {
    run("raw_ptr_mut_param", "wolf-interp#181");
}

/// A private fn returning `*T`. Red at trunk: `fail(E1302)` at the type.
#[test]
fn sig_private_ret_raw() {
    run("sig_private_ret_raw", "wolf-interp#181");
}

/// An `export fn` taking `*T` sits at the membrane. Red at trunk:
/// `fail(E1302)`.
#[test]
fn sig_export_raw() {
    run("sig_export_raw", "wolf-interp#181");
}

/// A hand-declared C call outside `unsafe` is E1301 at the call. Red at
/// trunk: `unsupported` (no body).
#[test]
fn extern_c_outside_unsafe() {
    run("extern_c_outside_unsafe", "wolf-interp#181");
}

/// libc through hand-declared externs inside `unsafe`: lupin has no C ABI and
/// refuses the call by name. Red at trunk: `fail(E1302)`.
#[test]
fn extern_libc() {
    run("extern_libc", "wolf-interp#181");
}

/// Controls: a `pub` signature and an impl method still refuse `*T`.
#[test]
fn sig_controls() {
    run("unsafe_sig", "wolf-interp#181");
    run("sig_method_raw", "wolf-interp#181");
}

// ---- wolf-interp#180: E0602 at an annotated binding -----------------------
// `[err.rows]`: a binding whose declared row cannot hold its initializer's
// is refused at the initializer, before any `match` reads it.

/// The issue's shape: `let a: !int = half([8])` where `half`'s inferred row
/// is `{none}`, raised through `first`'s declared row. Red at trunk: E0801
/// at the later `match`. (When `half` raises through `xs.get(0)?` directly,
/// lupin's row inference reads no method row off a local and keeps `{..}`,
/// so it still answers E0801 at the match where the compiler answers E0602
/// — refused on both, by a different code; reported, not mirrored here.)
#[test]
fn bind_row_wider() {
    run("bind_row_wider", "wolf-interp#180");
}

/// A spelled row wider than the annotation's. Red at trunk: `exit(0)`, `5`.
#[test]
fn bind_row_named_wider() {
    run("bind_row_named_wider", "wolf-interp#180");
}

/// The same under `var`. Red at trunk: `exit(0)`, `7`.
#[test]
fn bind_row_var_wider() {
    run("bind_row_var_wider", "wolf-interp#180");
}

/// Controls: a declared row that holds the initializer's, an empty inferred
/// row under `!int`, and an open declared row.
#[test]
fn bind_row_controls() {
    for row in ["bind_row_fits", "bind_row_empty_ok", "bind_row_open_ok"] {
        run(row, "wolf-interp#180");
    }
}

// ---- wolf-interp#174: the closed attribute set, cfg(target), the ABI -----
// `[gram.item.attr.set]`, `[gram.item.attr.cfg]`, `[abi.c.seams]` (KWC K13,
// K7). The `attr_*`, `cfg_target_*`, `cfg_predicate_unknown` and
// `extern_abi_interrupt` programs are the compiler's kw01 corpus rows
// (`corpus/grammar/`), byte for byte; the compiler gate is
// `attr_closed_set_lanes.rs`.

/// Each refused attribute, red at trunk: `exit(0)` (the attribute ignored).
#[test]
fn attributes_nothing_reads_are_e0817() {
    for row in [
        "attr_unknown",
        "attr_contract_unimplemented",
        "attr_repr_unimplemented",
        "attr_section",
        "attr_thread_local",
        "cfg_target_unknown",
        "cfg_predicate_unknown",
    ] {
        run(row, "wolf-interp#174");
    }
}

/// Every bad item of a `repr` line, each its own E0817. Red at trunk.
#[test]
fn attr_repr_bogus() {
    run("attr_repr_bogus", "wolf-interp#174");
}

/// `repr(c)` on a fn and `consttime` on a struct. Red at trunk.
#[test]
fn attr_misplaced() {
    run("attr_misplaced", "wolf-interp#174");
}

/// `#[trusted]` on a field and `#[noalloc]` on a statement. Red at trunk.
#[test]
fn attr_stmt_field() {
    run("attr_stmt_field", "wolf-interp#174");
}

/// `cfg(not(…))` and two predicates in one `cfg`. Red at trunk.
#[test]
fn cfg_malformed() {
    run("cfg_malformed", "wolf-interp#174");
}

/// `extern "x86-interrupt" fn` is E0818 at the string. Red at trunk.
#[test]
fn extern_abi_interrupt() {
    run("extern_abi_interrupt", "wolf-interp#174");
}

/// Exactly one of two arch-gated definitions survives on every host. Red at
/// trunk: `fail(E0302)`, both kept.
#[test]
fn cfg_target_arch() {
    run("cfg_target_arch", "wolf-interp#174");
}

/// What the freestanding target gates — an ill-typed item and a statement
/// naming what does not exist — is never resolved or typed on a hosted run.
/// Red at trunk: `fail(E0401)`.
#[test]
fn cfg_target_freestanding() {
    run("cfg_target_freestanding", "wolf-interp#174");
}

/// `cfg` on fields, impl members and a statement. Red at trunk:
/// `unsupported` (the gated statement's name does not resolve).
#[test]
fn cfg_field_member() {
    run("cfg_field_member", "wolf-interp#174");
}

/// The control: the implemented set earns no E0817. lupin still declines
/// this row's `comptime fn` by name — its own construct gap, which the
/// compiler gate pins and #174 does not cover.
#[test]
fn attr_implemented_set() {
    run("attr_implemented_set", "wolf-interp#174");
}

// ---- wolf-interp#175: the first diagnostic, `[proto.record.first]` --------
// Ruling #28: the diagnostic at the earliest byte offset wins, the earlier
// phase at one offset. The `first_*` rows from `boundary_before_lex` to
// `union_toplevel` are the compiler's s203 rows (`corpus/rows/negative/`)
// and its gate fixtures (`first_diagnostic_lanes.rs`), byte for byte; the
// rest are is70's own calibration of the boundary rule on the compiler.

/// A parse error before a lex error is first. Red at trunk: E0102.
#[test]
fn first_parse_before_lex() {
    run("first_parse_before_lex", "wolf-interp#175");
}

/// An unclosed `(` one byte before the unterminated string inside it:
/// E0202 at the opener. Red at trunk: E0102.
#[test]
fn first_boundary_before_lex() {
    run("first_boundary_before_lex", "wolf-interp#175");
}

/// A character that begins no token is E0107, over the whole run. Red at
/// trunk: E0101 at the first backtick.
#[test]
fn first_stray_characters() {
    run("first_stray_backtick", "wolf-interp#175");
    run("first_stray_runs", "wolf-interp#175");
}

/// "Expected a pattern" is E0207. Red at trunk: E0201.
#[test]
fn first_expected_pattern() {
    run("first_keyword_pattern", "wolf-interp#175");
    run("first_parse_before_resolve", "wolf-interp#175");
}

/// A declaration keyword inside an error row: E0202 at the row's `{`. Red at
/// trunk: E0008 at `var`.
#[test]
fn first_row_brace_unclosed() {
    run("first_row_brace_unclosed", "wolf-interp#175");
}

/// A top-level `union` is E0203. Red at trunk: E0201.
#[test]
fn first_union_toplevel() {
    run("first_union_toplevel", "wolf-interp#175");
}

/// The lex/parse tie at one offset is the lexer's E0102, spanning the
/// string from its quote. Red at trunk: E0102 at the line end.
#[test]
fn first_same_offset_lex() {
    run("first_same_offset_lex", "wolf-interp#175");
}

/// A group a later closer of another kind leaves open: E0202 at the
/// opener — a call's `(` met by `}`, a list's `[` met by `}` after a stray
/// `)`. Red at trunk: E0201 at the later token.
#[test]
fn first_boundary_at_the_opener() {
    run("first_boundary_call", "wolf-interp#175");
    run("first_boundary_list", "wolf-interp#175");
}

/// Controls: a group that does close later keeps E0201 at the bad token; a
/// closer that matches nothing open is E0201 at the closer; #377's list
/// literal runs everywhere.
#[test]
fn first_controls() {
    run("first_group_closed_later", "wolf-interp#175");
    run("first_stray_closer", "wolf-interp#175");
    run("first_list_literal_sum", "wolf-interp#175");
}

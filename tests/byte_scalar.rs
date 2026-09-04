//! is36 — the `byte` scalar (`[type.byte]`, D72/s135) and the producers that
//! speak it (wolf-lang#231, #232).
//!
//! sc32/sc33 measured what every byte buffer costs as a `List[int]`: a 64 KiB
//! read charged ~1 MiB of region ledger, which made a region cap on io-heavy
//! code a fiction. D72's answer is one new scalar — 8-bit, unsigned, `char`'s
//! posture rather than an integer's — and `List[byte]` strides by 1.
//!
//! The corpus carries the end-to-end witnesses (`typecheck/byte_casts.lu`,
//! `byte_shapes.lu`, `strings/bytes_roundtrip.lu`, `memory/byte_list_ledger.lu`,
//! `memory/consumed_walk_charges_nothing.lu`). These are the unit half: the
//! clause sentence by sentence, including the ones no corpus file reaches.

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

fn run(name: &str, body: &str) -> Output {
    let dir = scratch(name);
    let entry = dir.join("main.lu");
    std::fs::write(&entry, format!("fn main() -> !int {{\n{body}\n    0\n}}\n")).expect("written");
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("run")
        .arg(&entry)
        .output()
        .expect("lupin runs")
}

/// Runs `body` and returns its stdout, asserting it exited 0.
fn out(name: &str, body: &str) -> String {
    let output = run(name, body);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

/// Runs `body` expecting the by-name refusal (exit 4) and returns stderr.
fn refused(name: &str, body: &str) -> String {
    let output = run(name, body);
    assert_eq!(
        output.status.code(),
        Some(4),
        "wanted the by-name refusal, got {output:?}"
    );
    String::from_utf8(output.stderr).expect("utf8 stderr")
}

/// Runs `body` expecting a STATIC rejection (exit 2) and returns stderr.
///
/// is36 answered every one of these with the by-name refusal above, because a
/// tree-walk had no static rung for a type error. is37 gave it one for this
/// one type (wolf-interp#62, `sema::byte_check`): the domain of `[type.byte]`
/// is refused where the compilers refuse it, with their code and their span.
/// The four tests that moved from `refused` to here are the visible half of
/// that change, and the by-name refusals they used to make are still the
/// answer wherever the static pass cannot see the types — see
/// [`a_byte_place_still_refuses_dynamically_when_the_pass_cannot_see_it`].
fn refused_statically(name: &str, body: &str) -> String {
    let output = run(name, body);
    assert_eq!(
        output.status.code(),
        Some(2),
        "wanted a static rejection, got {output:?}"
    );
    String::from_utf8(output.stderr).expect("utf8 stderr")
}

// ---------------------------------------------------------------------------
// `[type.byte.cast]` — the two bridges, and the only ones.
// ---------------------------------------------------------------------------

#[test]
fn int_as_byte_truncates_at_every_boundary_and_never_traps() {
    // "the only narrowing `as` in the language that never traps": it keeps
    // the low eight bits. `0`, `255`, `256`, `-1` are the clause's own
    // witness values, and W0401 does not fire — truncation is the clause,
    // not an accident.
    let stdout = out(
        "byte-trunc",
        r#"    print("{0 as byte} {255 as byte} {256 as byte} {(0 - 1) as byte} {300 as byte} {(0 - 256) as byte}")"#,
    );
    assert_eq!(stdout, "0 255 0 255 44 0\n");
}

#[test]
fn byte_as_int_widens_by_zero_extension() {
    // Never a sign-extension: the domain has no negatives, so `200 as byte
    // as int` is 200 and never -56.
    let stdout = out(
        "byte-widen",
        r#"    print("{200 as byte as int} {255 as byte as int} {128 as byte as int}")"#,
    );
    assert_eq!(stdout, "200 255 128\n");
}

#[test]
fn a_byte_cast_to_byte_is_the_identity() {
    let stdout = out(
        "byte-ident",
        r#"    let b = 200 as byte
    print("{b as byte}")"#,
    );
    assert_eq!(stdout, "200\n");
}

#[test]
fn there_is_no_byte_to_float_bridge() {
    // "there is no `byte as f64`"; other widths cast through `int`.
    let stderr = refused(
        "byte-nofloat",
        r#"    let b = 65 as byte
    let f = b as f64
    print("{f}")"#,
    );
    assert!(stderr.contains("outside the cast set"), "{stderr}");
}

#[test]
fn char_and_byte_bridge_through_int() {
    // "`byte` and `char` bridge through `int` too (`b as int as char`)".
    let stdout = out(
        "byte-char",
        r#"    print("{65 as byte as int as char} {'A' as int as byte}")"#,
    );
    assert_eq!(stdout, "A 65\n");
}

// ---------------------------------------------------------------------------
// `[type.byte.op]` — every operator widens; the comparisons do not.
// ---------------------------------------------------------------------------

#[test]
fn every_arithmetic_operator_widens_to_int_and_yields_int() {
    // `200 + 200` is 400 — the term is int's, never an 8-bit overflow — and
    // `0 - 1` on a zero byte is `-1`, neither a trap nor a wrap. `-b` is
    // int's negation.
    let stdout = out(
        "byte-arith",
        r#"    let b = 200 as byte
    let z = 0 as byte
    print("{b + 1} {b + b} {z - 1} {-b} {b * 2} {b / 8} {b % 7}")"#,
    );
    assert_eq!(stdout, "201 400 -1 -200 400 25 4\n");
}

#[test]
fn a_mixed_term_with_an_int_is_legal_and_int() {
    // "a mixed term `b + n` with `n: int` is legal and `int`", and a
    // `{integer}` literal beside a byte operand adopts `int`.
    let stdout = out(
        "byte-mixed",
        r#"    let b = 200 as byte
    let n = 1000
    print("{b + n} {n - b}")"#,
    );
    assert_eq!(stdout, "1200 800\n");
}

#[test]
fn comparisons_are_octet_order_which_is_unsigned() {
    // The point of the type: `200 as byte > 100 as byte` is true where a
    // signed `i8` reading of the same eight bits says `-56 < 100`. `<=>`
    // yields `int`.
    let stdout = out(
        "byte-order",
        r#"    let hi = 200 as byte
    let lo = 100 as byte
    print("{hi > lo} {lo < hi} {hi >= hi} {hi == hi} {hi != lo} {hi <=> lo} {lo <=> hi} {hi <=> hi}")"#,
    );
    assert_eq!(stdout, "true true true true true 1 -1 0\n");
}

#[test]
fn a_byte_compares_only_with_a_byte() {
    // "`byte` against `int` is the ordinary type mismatch — widen the byte."
    // A quiet `false` here would run a program the compiler rejects, which
    // is the permissive divergence and the harder kind to notice.
    let stderr = refused_statically(
        "byte-mismatch",
        r#"    let b = 119 as byte
    print("{b == 119}")"#,
    );
    assert!(stderr.contains("E0401"), "{stderr}");
    assert!(
        stderr.contains("comparison"),
        "the note names the position: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// `[type.byte]` — no literal adoption, in either direction.
// ---------------------------------------------------------------------------

#[test]
fn a_byte_adopts_no_numeric_literal() {
    // `let b: byte = 65` is not a narrowing — it is a literal that has no
    // byte to become. The counterparty answers E0401, and since is37 so does
    // this machine, at the counterparty's span (`typecheck/byte_narrow_fail.lu`
    // `[588,590]`, the `65`).
    let stderr = refused_statically("byte-adopt", "    let b: byte = 65\n    print(\"{b}\")");
    assert!(stderr.contains("E0401"), "{stderr}");
    assert!(stderr.contains("adopts no numeric literal"), "{stderr}");
    assert!(
        stderr.contains("as byte"),
        "the note names the fix: {stderr}"
    );
}

#[test]
fn a_byte_value_satisfies_no_numeric_expectation() {
    let stderr = refused_statically(
        "byte-reverse",
        "    let b = 65 as byte\n    let n: int = b\n    print(\"{n}\")",
    );
    assert!(stderr.contains("E0401"), "{stderr}");
    assert!(stderr.contains("not an integer type"), "{stderr}");
    assert!(
        stderr.contains("as int"),
        "the note names the fix: {stderr}"
    );
}

#[test]
fn a_byte_is_copy_shaped_so_assignment_does_not_move_it() {
    // `[mem.tier0.move.3]`'s "POD-shaped types only": D72 rules `byte` an
    // 8-bit unsigned SCALAR — "an `i8`-shaped storage cell at every tier" —
    // so `let b = a` copies and `a` stays live. This is wolf-interp#50's
    // shape one type over (`char` assignment moved here through 0.1.16 while
    // the compiler printed the char twice), caught the day the type landed
    // instead of a release later, because the permissive direction reversed
    // is the one this machine must never take on its own. Measured against
    // `wolf 0.2.4 --checked`: `65 65`.
    let stdout = out(
        "byte-copy",
        r#"    let a = 65 as byte
    let b = a
    var xs = List[byte]()
    (mut xs).push(a)
    (mut xs).push(b)
    print("{a} {b} {xs.len} {xs[0]}")"#,
    );
    assert_eq!(stdout, "65 65 2 65\n");
}

#[test]
fn a_byte_scrutinee_takes_no_literal_arm() {
    // `[type.byte]`: no literal adoption "in every position, a `match` arm
    // included: a `byte` scrutinee binds or wildcards, and a literal arm is
    // spelled over `b as int`". Silence here is not an option — a literal
    // that merely FAILS to match a byte does not just run a program the
    // compiler rejects, it runs it down the WRONG ARM and prints a wrong
    // answer ("other" for a byte that is 65). wolfc answers E0401 at the arm.
    let stderr = refused(
        "byte-match-lit",
        r#"    let b = 65 as byte
    let s = match b {
        65 => "sixtyfive",
        _ => "other",
    }
    print("{s}")"#,
    );
    assert!(stderr.contains("takes no literal arm"), "{stderr}");
    assert!(
        stderr.contains("b as int"),
        "the note names the fix: {stderr}"
    );
}

#[test]
fn a_byte_scrutinee_binds_and_wildcards_and_matches_over_the_widening() {
    // The two spellings the clause leaves: widen the scrutinee, or bind it.
    let stdout = out(
        "byte-match-ok",
        r#"    let b = 65 as byte
    let s = match b as int {
        65 => "sixtyfive",
        _ => "other",
    }
    let t = match b {
        x => if x as int > 127 { "high" } else { "low" },
    }
    print("{s} {t}")"#,
    );
    assert_eq!(stdout, "sixtyfive low\n");
}

#[test]
fn a_byte_place_takes_no_int_and_has_no_compound_assignment() {
    // `[type.byte.op]` draws the consequence by name: "`byte` has no compound
    // assignment — `b += 1` is the E0401 an `int` assigned to a `byte` is,
    // because `b + 1` is an `int`." Both spellings retyped the VARIABLE here
    // before this rule: `var b = 65 as byte` then `b = 66` left an `int` in a
    // byte's place, and every later `{b}` printed an int's rendering of
    // whatever the arithmetic produced. wolfc answers E0401 (and E0409 for
    // the compound form).
    let plain = refused_statically(
        "byte-assign-int",
        "    var b = 65 as byte\n    b = 66\n    print(\"{b}\")",
    );
    assert!(plain.contains("E0401"), "{plain}");
    let compound = refused_statically(
        "byte-compound",
        "    var b = 65 as byte\n    b += 1\n    print(\"{b}\")",
    );
    assert!(compound.contains("E0401"), "{compound}");
    assert!(
        compound.contains("as byte"),
        "the note names the spelling: {compound}"
    );

    // And the spelling the clause names does work.
    let stdout = out(
        "byte-narrow-back",
        "    var b = 65 as byte\n    b = (b + 1) as byte\n    print(\"{b}\")",
    );
    assert_eq!(stdout, "66\n");
}

// ---------------------------------------------------------------------------
// `[type.byte.interp]` — `{b}` prints the NUMBER.
// ---------------------------------------------------------------------------

#[test]
fn a_byte_hole_prints_the_number_and_takes_the_integer_spec_surface() {
    // "never a character: a byte is a quantity, and the character it might
    // encode is `str`'s business". A spec on the hole is `int`'s, because
    // the hole widens the byte before formatting.
    let stdout = out(
        "byte-interp",
        r#"    let a = 65 as byte
    let f = 255 as byte
    print("{a} {a:x} {f:x} {f:02X} {a:>5}|")"#,
    );
    assert_eq!(stdout, "65 41 ff FF    65|\n");
}

// ---------------------------------------------------------------------------
// The producers (wolf-lang#231) and the ledger (#203, #232).
// ---------------------------------------------------------------------------

#[test]
fn bytes_and_str_from_utf8_agree_on_the_element_type() {
    // The round trip is the witness that producer and consumer agree: no
    // conversion sits between `s.bytes()` and `str_from_utf8`.
    let stdout = out(
        "byte-roundtrip",
        r#"    let s = "wolf é"
    let bs = s.bytes()
    let back = str_from_utf8(bs)?
    print("{back} {bs.len} {back == s} {bs[5] as int} {bs[6] as int}")"#,
    );
    assert_eq!(stdout, "wolf é 7 true 195 169\n");
}

#[test]
fn a_list_of_bytes_strides_by_one_where_a_list_of_ints_strides_by_a_slot() {
    // wolf-lang#203's relation, not its units: the same 4096 pushes charge
    // at least seven times more as `List[int]` than as `List[byte]`, and the
    // byte buffer stays inside twice its payload plus a header (the growth
    // history is all that is left once the width multiplier is gone).
    let stdout = out(
        "byte-ledger",
        r#"    let payload = 4096
    var hdr = 0
    var bytes = 0
    var ints = 0
    region r0 {
        var e = List[byte]()
        hdr = region_bytes(r0)
    }
    region r1 {
        var xs = List[byte]()
        for i in 0..payload {
            (mut xs).push((i % 256) as byte)
        }
        bytes = region_bytes(r1)
    }
    region r2 {
        var ys = List[int]()
        for i in 0..payload {
            (mut ys).push(i % 256)
        }
        ints = region_bytes(r2)
    }
    print("holds {bytes >= payload} bound {bytes <= 2 * payload + hdr + 64} width {ints >= 7 * bytes}")"#,
    );
    assert_eq!(stdout, "holds true bound true width true\n");
}

#[test]
fn a_consumed_byte_view_charges_nothing_and_a_bound_one_charges_the_payload() {
    // wolf-lang#232's phantom 16×: `s.bytes()` consumed on the spot —
    // iterated, indexed, asked for `len` — is the receiver's own storage and
    // allocates nothing, while `let bs = s.bytes()` is the materializing
    // position. Without the distinction a `region r(cap: n)` derived on one
    // tier mis-fires by an order of magnitude on the other, on the exact
    // idiom `std.bytes` teaches.
    let stdout = out(
        "byte-view",
        r#"    var s = "w"
    for _ in 0..10 {
        s = "{s}{s}"
    }
    var walk = 0
    var index = 0
    var len = 0
    var bound = 0
    var sum = 0
    region r1 {
        for b in s.bytes() {
            sum = sum + b
        }
        walk = region_bytes(r1)
    }
    region r2 {
        let pick = s.bytes()[0] as int
        index = region_bytes(r2)
        if pick != 119 { return 3 }
    }
    region r3 {
        let m = s.bytes().len
        len = region_bytes(r3)
        if m != 1024 { return 4 }
    }
    region r4 {
        let bs = s.bytes()
        bound = region_bytes(r4)
        if bs.len != 1024 { return 5 }
    }
    print("walk {walk == 0} index {index == 0} len {len == 0} bound {bound >= 1024} sum {sum}")"#,
    );
    assert_eq!(
        stdout,
        "walk true index true len true bound true sum 121856\n"
    );
}

#[test]
fn a_byte_producer_mints_at_exact_capacity() {
    // A builtin that already knows its length has no push history to pay,
    // so the buffer is at most the payload plus a header — the `read_tight`
    // relation `memory/byte_producers_ledger.lu` pins for the fs readers
    // this machine declines, measured here on the one producer it serves.
    let stdout = out(
        "byte-exact",
        r#"    var s = "w"
    for _ in 0..12 {
        s = "{s}{s}"
    }
    var hdr = 0
    var view = 0
    region r0 {
        var e = List[byte]()
        hdr = region_bytes(r0)
    }
    region r1 {
        let bs = s.bytes()
        view = region_bytes(r1)
        if bs.len != 4096 { return 2 }
    }
    print("holds {view >= 4096} tight {view <= 4096 + hdr + 64}")"#,
    );
    assert_eq!(stdout, "holds true tight true\n");
}

// ---------------------------------------------------------------------------
// The dynamic boundary, where the static pass declines to guess (is37).
// ---------------------------------------------------------------------------

#[test]
fn a_byte_place_still_refuses_dynamically_when_the_pass_cannot_see_it() {
    // `sema::byte_check` refuses only where BOTH sides are known. A `List()`
    // with no element annotation types nothing, so `xs[0]` is not an `int`
    // this machine can name at `resolve` — and the write into a byte place
    // still must not land. is36's dynamic rule is what catches it, which is
    // why is37 kept it rather than letting the static pass inherit the job:
    // `[type.byte]`'s domain has to hold at BOTH rungs, since the static one
    // is deliberately partial.
    let stderr = refused(
        "byte-place-dynamic",
        "    var b = 65 as byte\n    var xs = List()\n    (mut xs).push(66)\n    b = xs[0]\n    print(\"{b}\")",
    );
    assert!(stderr.contains("byte"), "{stderr}");
}

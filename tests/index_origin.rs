//! is28 — the D61 half: `#![index(0|1)]` / `#[index(0|1)]` per
//! `[gram.attr.index]` and the shift per `[gram.expr.index.origin]`,
//! reimplemented from the D61 ruling text (the independence doctrine — the
//! compiler's source was never opened).
//!
//! Measured at 0.1.16 before this landed: the file form failed E0101 at lex
//! (`#` begins no token — loud), and the statement form was a SILENTLY
//! IGNORED unknown attribute — `grammar/index_origin_scopes.lu` ran to the
//! wrong answer (trap(bounds) where wolfc exits 0). Silent-wrong is the bug
//! class this file exists to keep dead.

use wolf_interp::frontend;
use wolf_interp::lex::{self, Tok};
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn observe(source: &str) -> frontend::Observation {
    frontend::observe(source.as_bytes(), None)
}

fn prints(source: &str, expected: &str) {
    let obs = observe(source);
    assert_eq!(obs.verdict, Verdict::Exit(0), "source:\n{source}");
    assert_eq!(
        String::from_utf8_lossy(&obs.stdout),
        expected,
        "source:\n{source}"
    );
}

fn fails(source: &str, code: &str, phase: Phase) -> frontend::Observation {
    let obs = observe(source);
    assert_eq!(
        obs.verdict,
        Verdict::Fail(code.to_owned()),
        "source:\n{source}\ndetail: {:?}",
        obs.detail
    );
    assert_eq!(obs.phase_reached, phase, "source:\n{source}");
    obs
}

fn traps(source: &str, kind: TrapKind) -> frontend::Observation {
    let obs = observe(source);
    assert_eq!(
        obs.verdict,
        Verdict::Trap(kind),
        "source:\n{source}\nreason: {:?}",
        obs.reason
    );
    obs
}

// -- the lexical tier ------------------------------------------------------

#[test]
fn hash_bang_bracket_is_one_token_and_the_shebang_narrowed_around_it() {
    // `#![` is the file-wide attribute opener, one dedicated token.
    let lexed = lex::lex("#![index(1)]\n");
    assert!(lexed.errors.is_empty(), "{:?}", lexed.errors);
    assert!(
        lexed
            .tokens
            .iter()
            .any(|t| matches!(t.tok, Tok::HashBangBracket)),
        "{:?}",
        lexed.tokens
    );

    // A real interpreter line is still trivia — the narrowing must not
    // break existing `#!` scripts (`[gram.lex.shebang]`).
    let lexed = lex::lex("#!/usr/bin/env lupin\nfn main() -> int { 0 }\n");
    assert!(lexed.errors.is_empty(), "{:?}", lexed.errors);
    assert!(
        !lexed
            .tokens
            .iter()
            .any(|t| matches!(t.tok, Tok::HashBangBracket)),
        "a shebang is not an attribute"
    );
}

#[test]
fn a_shebang_script_with_a_marker_below_it_still_runs() {
    // The file form is legal after the shebang — the script witness stays
    // green (`[gram.attr.index]`: "after the shebang and any `//!` header
    // lines").
    prints(
        "#!/usr/bin/env lupin\n#![index(1)]\nfn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(7)\n    print(\"{xs[1]}\")\n    0\n}\n",
        "7\n",
    );
}

// -- position: E0211 -------------------------------------------------------

#[test]
fn a_misplaced_file_wide_attribute_is_e0211_by_position() {
    // Below the first declaration (`grammar/index_origin_misplaced.lu`'s
    // shape).
    fails(
        "fn f() -> int {\n    1\n}\n\n#![index(1)]\n\nfn main() -> !int {\n    0\n}\n",
        "E0211",
        Phase::Parse,
    );
    // Inside a block.
    fails(
        "fn main() -> !int {\n    #![index(1)]\n    0\n}\n",
        "E0211",
        Phase::Parse,
    );
    // A second consecutive group is already later than the first
    // non-trivia construct.
    fails(
        "#![index(1)]\n#![index(0)]\nfn main() -> !int {\n    0\n}\n",
        "E0211",
        Phase::Parse,
    );
}

// -- argument and name validation: E0813, by name, never ignored -----------

#[test]
fn bad_marker_arguments_are_e0813_and_take_no_effect() {
    // 0 and 1 are the origins (D61 rejected arbitrary origins).
    for source in [
        "#![index(7)]\nfn main() -> !int {\n    0\n}\n",
        "#![index]\nfn main() -> !int {\n    0\n}\n",
        "#![index(0, 1)]\nfn main() -> !int {\n    0\n}\n",
        "#![index = 1]\nfn main() -> !int {\n    0\n}\n",
        // The statement form validates identically.
        "fn main() -> !int {\n    #[index(7)]\n    let x = 1\n    x - 1\n}\n",
        "fn main() -> !int {\n    #[index]\n    let x = 1\n    x - 1\n}\n",
        // Duplicated `index` items on one node.
        "fn main() -> !int {\n    #[index(1), index(0)]\n    let x = 1\n    x - 1\n}\n",
    ] {
        let obs = fails(source, "E0813", Phase::Resolve);
        drop(obs);
    }
}

#[test]
fn an_unknown_inner_attribute_is_refused_by_name() {
    // The inner form is strict from birth: a file-wide marker an
    // implementation silently skipped would silently change how every
    // subscript in the file reads.
    let obs = fails(
        "#![feature(dreams)]\nfn main() -> !int {\n    0\n}\n",
        "E0813",
        Phase::Resolve,
    );
    let detail = obs.detail.expect("a diagnostic").message;
    assert!(detail.contains("`feature`"), "by name: {detail}");
}

// -- the shift, exactly per `[gram.expr.index.origin]` ---------------------

#[test]
fn the_worked_table_holds_on_a_five_byte_string() {
    // D61's worked table, against `s.len == 5`: first element, last
    // element, the origin-free `^1`, full slice, first three, drop the
    // first, drop the last, empty at the front.
    prints(
        "#![index(1)]\nfn main() -> !int {\n    let s = \"abcde\"\n    print(\"{s[1..1]}|{s[s.len..s.len]}|{s[^1..]}|{s[1..s.len]}|{s[1..3]}|{s[2..]}|{s[..^1]}|{s[1..0]}|\")\n    0\n}\n",
        "a|e|e|abcde|abc|bcde|abcd||\n",
    );
}

#[test]
fn subscripts_count_from_one_and_zero_is_out_of_range_by_the_shift() {
    prints(
        "#![index(1)]\nfn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(10)\n    (mut xs).push(20)\n    (mut xs).push(30)\n    print(\"{xs[1]} {xs[xs.len]}\")\n    0\n}\n",
        "10 30\n",
    );
    let obs = traps(
        "#![index(1)]\nfn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(10)\n    let v = xs[0]\n    v\n}\n",
        TrapKind::Bounds,
    );
    // The writer's-mode duty: the human line renders the index the writer
    // wrote, never the lowered one.
    let message = obs.trap.expect("a trap").message;
    assert!(message.contains("index 0"), "writer's number: {message}");
}

#[test]
fn inclusive_is_the_coupling_and_dotdoteq_is_redundant_but_legal() {
    prints(
        "#![index(1)]\nfn main() -> !int {\n    let s = \"wolfish\"\n    print(\"{s[1..4]}|{s[1..=4]}|{s[..=^4]}|{s[2..^1]}\")\n    0\n}\n",
        "wolf|wolf|wolf|olfis\n",
    );
}

#[test]
fn the_origin_free_forms_do_not_move() {
    // `^n` (in its supported range positions — the direct `xs[^n]` element
    // read stays this machine's named refusal in EVERY mode, unchanged),
    // `.len`, bare ranges, `.get`, map keys, tuple members — the table's
    // unchanged rows, one program.
    prints(
        "#![index(1)]\nfn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(5)\n    (mut xs).push(6)\n    var total = 0\n    for i in 1..3 { total += i }\n    var m = Map[str, int]()\n    m[\"k\"] = 9\n    let pair = (10, 20)\n    var j = 0\n    let g = xs.get(j) else { 0 - 1 }\n    let s = \"wolfish\"\n    print(\"{s[^3..]} {xs.len} {total} {m[\"k\"]} {pair.0} {g}\")\n    0\n}\n",
        "ish 2 3 9 10 5\n",
    );
}

#[test]
fn statement_scopes_nest_and_the_innermost_marker_wins() {
    // `index_origin_scopes.lu`'s shape, self-contained: the statement form
    // scopes the annotated statement's full lexical extent, `#[index(0)]`
    // restores the default inside it, and code textually outside is
    // untouched.
    prints(
        "fn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(10)\n    (mut xs).push(20)\n    var sum = 0\n    #[index(1)]\n    {\n        sum += xs[1]\n        #[index(0)]\n        {\n            sum += xs[0]\n        }\n        sum += xs[2]\n    }\n    sum += xs[0]\n    print(\"{sum}\")\n    0\n}\n",
        "50\n",
    );
}

#[test]
fn interpolations_and_closures_are_lexically_in_scope() {
    prints(
        "#![index(1)]\nfn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(7)\n    (mut xs).push(8)\n    let f = fn(i) xs[i]\n    print(\"{xs[1]} {f(2)}\")\n    0\n}\n",
        "7 8\n",
    );
}

#[test]
fn indexed_writes_shift_exactly_as_reads_do() {
    prints(
        "#![index(1)]\nfn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(7)\n    (mut xs).push(8)\n    xs[1] = 9\n    xs[2] += 1\n    print(\"{xs[1]} {xs[2]}\")\n    0\n}\n",
        "9 9\n",
    );
}

#[test]
fn the_checked_shift_corner_traps_overflow_before_the_bounds_question() {
    // `xs[int.min]` under origin 1: the one index with no representable
    // predecessor traps `overflow` (D56's kind, X3 posture) — not bounds.
    let obs = traps(
        "#![index(1)]\nfn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(10)\n    let i: int = 0 - 9223372036854775807 - 1\n    let v = xs[i]\n    v\n}\n",
        TrapKind::Overflow,
    );
    let message = obs.trap.expect("a trap").message;
    assert!(message.contains("1-origin shift"), "{message}");
}

#[test]
fn a_slice_bounds_trap_renders_the_writers_range() {
    let obs = traps(
        "#![index(1)]\nfn main() -> !int {\n    let s = \"wolf\"\n    let bad = s[2..9]\n    print(\"{bad}\")\n    0\n}\n",
        TrapKind::Bounds,
    );
    let message = obs.trap.expect("a trap").message;
    assert!(
        message.contains("2..9") && message.contains("origin 1"),
        "writer's spelling: {message}"
    );
}

#[test]
fn origin_zero_restates_the_default_and_is_inert() {
    // `#![index(0)]` is legal, redundant, and byte-for-byte today's
    // language.
    prints(
        "#![index(0)]\nfn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(7)\n    print(\"{xs[0]}\")\n    0\n}\n",
        "7\n",
    );
}

#[test]
fn zero_cost_when_absent_a_marker_free_file_is_unchanged() {
    // The absent-marker default is origin 0, exactly today's language —
    // the whole pre-existing corpus is the real witness (verdict-identical
    // before and after); this is the same fact in miniature.
    prints(
        "fn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(7)\n    (mut xs).push(8)\n    let s = \"wolf\"\n    print(\"{xs[0]} {s[^2..]} {s[1..3]}\")\n    0\n}\n",
        "7 lf ol\n",
    );
}

//! wolf-interp#69 (s142) — `str.to_int` at the edges of `int`.
//!
//! `int` is 64 bits, signed and checked. `to_int` parsed into an `i128` and
//! then wrapped the result as an `int`, so `"9223372036854775808"` — one past
//! `i64::MAX` — answered a `Value::Int` holding a number no `int` can hold.
//! Nothing rejected it at the mint site; the program's first arithmetic on it
//! trapped with an overflow the program never wrote, one statement or one
//! function away from the string that caused it.
//!
//! The compiler (s142, wolf-lang#263) answers the row for that input: there is
//! no `int` the text names, and X3 forbids a quiet wrap. The row already means
//! "this string is not an `int`", which is true of a number that does not fit
//! in one, so both sides answer it now. wolf-lang kept the overflow input in
//! its own crate tests until this landed, so the corpus pinned neither side;
//! it came home at the e9a17cb pin.
//!
//! The row's SPELLING is `[mem.str.parse]`'s as of s143 (wolf-lang#265): the
//! lowercase payload-free mark `parse`, not the `NotAnInt` both sides copied
//! from this implementation. `the_row_is_the_mark_parse` below is the test
//! that pins it by name — the range tests deliberately take the row through a
//! plain `else`, so without one the rename would be asserted by prose only.
//!
//! The cases that matter are the boundary and one past it, in both
//! directions: a fix that narrowed too far would take `i64::MAX` and
//! `i64::MIN` with it, and only the arithmetic afterwards shows the
//! difference between a value that fits and one that was minted anyway.

use wolf_interp::frontend;
use wolf_interp::protocol::Verdict;

fn stdout_of(source: &str) -> String {
    let observation = frontend::observe(source.as_bytes(), None);
    assert!(
        matches!(observation.verdict, Verdict::Exit(0)),
        "the program must run to a clean exit, got {} ({:?})",
        observation.verdict,
        observation.detail
    );
    String::from_utf8(observation.stdout).expect("utf8 stdout")
}

#[test]
fn the_two_extremes_of_int_parse_and_survive_arithmetic() {
    // The boundary itself. The arithmetic is here because the bug was never
    // visible at the parse — it was visible one operation later.
    let source = "\
fn main() -> !int {
    let hi = \"9223372036854775807\".to_int() else 0
    let lo = \"-9223372036854775808\".to_int() else 0
    print(\"{hi}\")
    print(\"{lo}\")
    print(\"{hi - 1}\")
    print(\"{lo + 1}\")
    0
}
";
    assert_eq!(
        stdout_of(source),
        "9223372036854775807\n-9223372036854775808\n9223372036854775806\n-9223372036854775807\n"
    );
}

#[test]
fn one_past_each_extreme_raises_and_the_program_runs_on() {
    // #69's own two inputs. The assertion is that the call RAISES — the
    // `else` fallback is what the binding takes — and that the program then
    // runs on normally, because the defect was a value that flowed onward and
    // detonated elsewhere. "The program reaches its own exit, having done
    // arithmetic" is half of what is being tested.
    //
    // The row is taken through the plain `else` rather than a tag arm, and
    // deliberately: what #69 is about is the RANGE, and `else` sees the raise
    // whatever the tag is called, so this assertion does not acquire a second
    // thing to be wrong about. The tag's own spelling is
    // `the_row_is_the_mark_parse`'s.
    let source = "\
fn main() -> !int {
    let over = \"9223372036854775808\".to_int() else -1
    let under = \"-9223372036854775809\".to_int() else -2
    print(\"{over} {under}\")
    print(\"{over + under}\")
    let still_fine = \"9223372036854775807\".to_int() else -3
    print(\"{still_fine}\")
    0
}
";
    assert_eq!(stdout_of(source), "-1 -2\n-3\n9223372036854775807\n");
}

#[test]
fn the_ordinary_readings_are_untouched() {
    // The regression guard on the other side of the change: whitespace still
    // trims, a sign still parses, and a string that is not a number at all is
    // the same row it always was.
    let source = "\
fn main() -> !int {
    let a = \"  42  \".to_int() else -1
    let b = \"-7\".to_int() else -1
    let c = \"0\".to_int() else -1
    let d = \"twelve\".to_int() else -1
    let e = \"\".to_int() else -1
    print(\"{a} {b} {c} {d} {e}\")
    0
}
";
    assert_eq!(stdout_of(source), "42 -7 0 -1 -1\n");
}

#[test]
fn the_row_is_the_mark_parse() {
    // `[mem.str.parse]` (s143, wolf-lang#265): `to_int`'s row is the lowercase
    // payload-free mark `parse`. Every implementation spelled it `NotAnInt` —
    // this one since before 0.1.13, the compiler from s142 by copying it — and
    // it was the one CapCase payload-free tag on the builtin surface, the exact
    // shape W0603 warns a program about (CapCase names a payload's type;
    // payload-free marks are lowercase bare words). The json builtins already
    // answer `parse` for the same condition, text that is not the shape asked
    // for, and one mark per condition is the wolf-std taxonomy.
    //
    // The tag is read two ways, because they fail differently: a NAMED arm
    // resolves the mark as a tag pattern (a rename that missed would take the
    // `else 0` fallback instead), and `{err}` renders it as its name
    // (`[type.interp.row]`), which is the byte `corpus/rows/to_int_parse.lu`
    // and `corpus/grammar/else_default.lu` pin from the other side.
    let source = "\
fn main() -> !int {
    let a = \"twelve\".to_int() else |err| { print(\"rendered {err}\"); 0 }
    let b = match \"twelve\".to_int() {
        parse => \"arm parse\",
        _ => \"arm other\",
    }
    print(b)
    let c = \"9223372036854775808\".to_int() else |err| { print(\"over {err}\"); 0 }
    if a == 0 && c == 0 { 0 } else { 1 }
}
";
    assert_eq!(stdout_of(source), "rendered parse\narm parse\nover parse\n");
}

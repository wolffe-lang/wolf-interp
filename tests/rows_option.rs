//! Postfix rows and row-tag resolution (issue #12 — the interpreter half of
//! wolf-lang#3 and #4, wolf-std F-0002/F-0023).
//!
//! Three realignments land together at the s27 pin:
//!
//! - **postfix rows in every type position** (`type ::= type '!' error_row`,
//!   `[gram.type]`): `fn or(v: int ! {none}, d: int)` and
//!   `let a: int ! {none} = miss()` parse, type at sema-lite depth, and RUN;
//! - **lowercase bare tags resolve at raise sites**: `return none` under
//!   `-> int ! {none}` raises the tag, and a lowercase identifier pattern
//!   that names a declared row tag dispatches on the tag in `match`;
//! - **raise-site resolution is eager** (the sc02 correction): an
//!   unresolvable tag is refused at the resolve rung whatever the input —
//!   an untaken `return bogus` can no longer certify falsely.
//!
//! The acceptance is wolf-std's `std.option`: the six helpers whose reviewed
//! signatures were unwritable while rows lived only in return position
//! (F-0002) now execute under lupin, lowercase `none` included.

use wolf_interp::frontend;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;

fn observe(source: &str) -> frontend::Observation {
    frontend::observe(source.as_bytes(), None)
}

// -- postfix rows parse and run -----------------------------------------

#[test]
fn postfix_rows_in_param_and_let_positions_run() {
    let source = "\
fn miss() -> int ! {none} {
    return none
}

fn or(v: int ! {none}, d: int) -> int {
    v else d
}

fn main() -> !int {
    let a: int ! {none} = miss()
    if or(a, 7) != 7 { return 1 }
    if or(42, 7) != 42 { return 2 }
    0
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
}

// -- std.option's six helpers, the F-0002 acceptance --------------------

/// The six reviewed signatures from `std/option/option.lu`, verbatim in
/// shape (`or` and `expect` in their generic spellings), executing on both
/// the hit and the miss path. `expect`'s miss traps `assert` with the
/// message, so it gets its own test below.
#[test]
fn std_options_six_helpers_execute() {
    let source = "\
fn or[T](v: T ! {none}, default: T) -> T {
    v else default
}

fn flatten(v: (int ! {none}) ! {none}) -> int ! {none} {
    let inner = v?
    inner
}

fn to_list(v: int ! {none}) -> List[int] {
    var out = List[int]()
    let x = v else { return out }
    out.push(x)
    out
}

fn exists(v: int ! {none}) -> bool {
    let probe = v else { return false }
    let keep = probe
    true
}

fn is_none(v: int ! {none}) -> bool {
    let probe = v else { return true }
    let keep = probe
    false
}

fn miss() -> int ! {none} {
    return none
}

fn hit() -> int ! {none} {
    42
}

fn main() -> !int {
    if or(hit(), 7) != 42 { return 1 }
    if or(miss(), 7) != 7 { return 2 }
    if (flatten(hit()) else 9) != 42 { return 3 }
    if (flatten(miss()) else 9) != 9 { return 4 }
    if to_list(hit()).len != 1 { return 5 }
    if to_list(miss()).len != 0 { return 6 }
    if !exists(hit()) { return 7 }
    if exists(miss()) { return 8 }
    if is_none(hit()) { return 9 }
    if !is_none(miss()) { return 10 }
    0
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
}

/// `expect[T](v, msg)` — the trap-on-miss helper: the hit path returns the
/// value, the miss path renders `msg` and traps `assert`
/// (`[conf.trap.assert]`).
#[test]
fn std_option_expect_traps_on_the_miss() {
    let helper = "\
fn expect[T](v: T ! {none}, msg: str) -> T {
    v else |_| {
        assert(false, msg)
        v?
    }
}

fn miss() -> int ! {none} {
    return none
}
";
    let hit = format!(
        "{helper}\nfn main() -> !int {{\n    if expect(41 + 1, \"unreachable\") == 42 {{ 0 }} else {{ 1 }}\n}}\n"
    );
    let observation = observe(&hit);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );

    let miss =
        format!("{helper}\nfn main() -> !int {{\n    expect(miss(), \"the absent case\")\n}}\n");
    let observation = observe(&miss);
    assert_eq!(
        observation.verdict,
        Verdict::Trap(wolf_interp::trap::TrapKind::Assert),
        "reason: {:?}",
        observation.reason
    );
    assert_eq!(
        String::from_utf8_lossy(&observation.stdout).trim(),
        "the absent case"
    );
}

// -- lowercase tags: raise sites and match arms -------------------------

/// The qmark_defer dispatch shape: lowercase identifiers that name declared
/// row tags are row-tag patterns, not first-arm binders — and an undeclared
/// lowercase name (`else |err|`) still binds.
#[test]
fn declared_lowercase_tags_dispatch_in_match() {
    let source = "\
fn pick(n: int) -> int ! {empty, negative} {
    if n == 0 { return empty }
    if n < 0 { return negative }
    n
}

fn main() -> !int {
    let a = pick(0) else |err| {
        match err {
            empty => 90,
            negative => 91,
        }
    }
    let b = pick(0 - 5) else |err| {
        match err {
            empty => 90,
            negative => 91,
        }
    }
    if a == 90 && b == 91 { 0 } else { 1 }
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
}

/// The sc02 trap, closed: a raise site whose tag resolves nowhere is
/// refused at **resolve**, even though the input never takes the branch.
/// Lazy resolution let exactly this program certify falsely.
#[test]
fn an_untaken_unresolvable_raise_is_refused_eagerly() {
    let source = "\
fn cut(s: str, sep: str) -> (str, str) ! {none} {
    if s.starts_with(sep) { return (s, s) }
    return bogus
}

fn main() -> !int {
    let p = cut(\"a\", \"a\") else (\"\", \"\")
    if p.0 != \"a\" { return 1 }
    0
}
";
    let observation = observe(source);
    assert_eq!(observation.verdict, Verdict::Unsupported);
    assert_eq!(observation.phase_reached, Phase::Resolve);
    let reason = observation.reason.unwrap_or_default();
    assert!(reason.contains("raise site"), "reason: {reason}");
    assert!(reason.contains("bogus"), "reason: {reason}");
}

/// The declared tag on the same untaken branch is *not* refused — the
/// check resolves the name, it does not prove reachability.
#[test]
fn a_declared_tag_on_an_untaken_branch_runs() {
    let source = "\
fn cut(s: str, sep: str) -> (str, str) ! {none} {
    if s.starts_with(sep) { return (s, s) }
    return none
}

fn main() -> !int {
    let p = cut(\"a\", \"a\") else (\"\", \"\")
    if p.0 != \"a\" { return 1 }
    0
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
}

/// `return v` where `v` is an ordinary binding is untouched by the raise
/// check — only bare names that resolve *nowhere* are refused.
#[test]
fn ordinary_returns_pass_the_raise_check() {
    let source = "\
fn double(n: int) -> int {
    let doubled = n * 2
    return doubled
}

fn main() -> !int {
    if double(21) == 42 { 0 } else { 1 }
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
}

// -- issue #16: an enum variant is a value, not a raise ------------------

/// wolf-interp#16 (wolf-std F-0037), the whole shape.
///
/// A declared enum's variant and a structural row tag are both tag-shaped,
/// and the machine used to build them as the same thing. So `fn id(v: W) -> W
/// ! {none} { v }` — a function that raises nothing — had its ordinary return
/// read as an error by the caller: `?` propagated it and `else` fired on the
/// VALUE path, so the miss ALWAYS won and the two paths were
/// indistinguishable. The worst class of bug: no diagnostic, wrong answer.
///
/// Verified against wolfgang at pin `613c3dc`, whose `--native` and
/// `--release` lanes both print `kind num` for this program.
#[test]
fn an_enum_variant_returned_through_a_row_is_not_an_error() {
    let source = "\
enum W {
    Num(int),
    Obj(int),
}

fn mknum(n: int) -> W {
    W.Num(n)
}

fn mkobj() -> W {
    W.Obj(0)
}

fn id(v: W) -> W ! {none} {
    v
}

fn kindof(v: W) -> str {
    match v {
        Num(n) => \"num\",
        Obj(m) => \"obj\",
    }
}

fn main() -> int {
    let n = mknum(3)
    let a = id(n) else mkobj()
    print(\"kind {kindof(a)}\")
    0
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "{:?}",
        observation.reason
    );
    assert_eq!(
        String::from_utf8_lossy(&observation.stdout).trim(),
        "kind num",
        "the `else` fired on the value path — wolf-interp#16 is back"
    );
}

/// The fix must not cost the error paths. One program, four of them: a raise
/// defaulted by `else`, a value passed through `else` untouched, a `?` that
/// unwraps an ok, and a `?` that propagates a raise.
#[test]
fn raises_and_values_stay_distinguishable_through_rows() {
    let source = "\
enum W {
    Num(int),
    Obj(int),
}

fn raises(flag: bool) -> W ! {none} {
    if flag { return none }
    W.Num(7)
}

fn viaq(flag: bool) -> int ! {none} {
    let w = raises(flag)?
    match w {
        Num(n) => n,
        Obj(m) => 0,
    }
}

fn main() -> int {
    let a = raises(true) else W.Obj(1)
    let ka = match a { Num(n) => \"num\", Obj(m) => \"obj\" }
    let b = raises(false) else W.Obj(1)
    let kb = match b { Num(n) => \"num\", Obj(m) => \"obj\" }
    let c = viaq(false) else 99
    let d = viaq(true) else 99
    print(\"{ka} {kb} {c} {d}\")
    0
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "{:?}",
        observation.reason
    );
    // obj: the raise was defaulted. num: the value survived `else`.
    // 7: `?` unwrapped an ok. 99: `?` propagated a raise.
    assert_eq!(
        String::from_utf8_lossy(&observation.stdout).trim(),
        "obj num 7 99"
    );
}

/// A tag that is NOT a declared variant keeps its error reading — the D30
/// structural row needs no declaration (`[err.rows]`), and narrowing that
/// would trade one silent wrong answer for another.
#[test]
fn an_undeclared_tag_is_still_an_error() {
    let source = "\
fn misses() -> int ! {TooShort} {
    return TooShort
}

fn main() -> int {
    let v = misses() else 42
    print(\"{v}\")
    0
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "{:?}",
        observation.reason
    );
    assert_eq!(String::from_utf8_lossy(&observation.stdout).trim(), "42");
}

/// Equality ignores where a tag's name resolved: it is a fact about the
/// program's declarations, not about the value.
#[test]
fn variant_equality_does_not_depend_on_the_construction_site() {
    let source = "\
enum W {
    Num(int),
    Obj(int),
}

fn mk(n: int) -> W {
    W.Num(n)
}

fn main() -> !int {
    if mk(3) == W.Num(3) { 0 } else { 1 }
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "{:?}",
        observation.reason
    );
}

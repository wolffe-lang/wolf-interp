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
    (mut out).push(x)
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

/// wolf-interp#29 (wolf-std F-0079), the cross-module half of the test
/// above — and the one that was wrong for as long as the rule existed.
///
/// The arm-resolution question ("is this lowercase name a tag or a
/// binder?") used to be put to the **matching module's own signatures**.
/// A handler in the entry file over a row raised in an imported module
/// therefore found no declared tag, read every arm's pattern as a fresh
/// binding, and took its FIRST ARM for every tag. No diagnostic, exit 0,
/// wrong answer: `fwd` printed `10 10 10` and `rev` printed `30 30 30`,
/// while wolfgang printed `10 20 30` on both rungs.
///
/// The row now travels with the value it was raised through, so the
/// answer is the same on either side of a module boundary. The two arm
/// orders are the experiment: under first-arm-wins, reversing the arms
/// reverses the output, which is what separates "dispatch works" from
/// "one tag happened to be right". `rest` and `binder` are the
/// counterweight — a lowercase name the row does not declare still binds.
///
/// The corpus carries the same witness as `rows/cross_module_arms`, on
/// both lanes; this is its unit-sized half, which does not wait on a pin.
#[test]
fn a_row_raised_across_a_module_boundary_still_dispatches_by_tag() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cross_module_arms");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(dir.join("scan")).expect("scratch created");
    std::fs::write(
        dir.join("scan/scan.lu"),
        "\
//! member: true

pub fn miss(k: int) -> int ! {deep, overflow, syntax} {
    if k == 0 { return syntax }
    if k == 1 { return deep }
    overflow
}

pub fn hop(k: int) -> int ! {deep, overflow, syntax} {
    let v = miss(k)?
    v
}
",
    )
    .expect("the member module is written");

    let source = "\
use scan

fn fwd(k: int) -> int {
    scan.miss(k) else |e| match e {
        syntax => 10,
        deep => 20,
        overflow => 30,
    }
}

fn rev(k: int) -> int {
    scan.miss(k) else |e| match e {
        overflow => 30,
        deep => 20,
        syntax => 10,
    }
}

fn hop(k: int) -> int {
    scan.hop(k) else |e| match e {
        syntax => 10,
        deep => 20,
        overflow => 30,
    }
}

fn rest(k: int) -> int {
    scan.miss(k) else |e| match e {
        deep => 20,
        other => 99,
    }
}

fn binder() -> int {
    scan.miss(0) else |err| 77
}

fn main() -> !int {
    print(\"fwd: {fwd(0)} {fwd(1)} {fwd(2)}\")
    print(\"rev: {rev(0)} {rev(1)} {rev(2)}\")
    print(\"hop: {hop(0)} {hop(1)} {hop(2)}\")
    print(\"rest: {rest(0)} {rest(1)} {rest(2)}\")
    print(\"binder: {binder()}\")
    0
}
";
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("the entry is written");

    let observation = wolf_interp::frontend::observe_file(
        &entry,
        source.as_bytes(),
        None,
        wolf_interp::eval::Trace::Off,
        &wolf_interp::eval::SchedRequest::Default,
        None,
    );
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
    assert_eq!(
        String::from_utf8(observation.stdout).expect("utf-8"),
        "fwd: 10 20 30\nrev: 10 20 30\nhop: 10 20 30\nrest: 99 20 99\nbinder: 77\n",
    );
}

/// wolf-interp#33 (wolf-std F-0084), #29's sequel one layer deeper: the
/// row travels with the value, but `?` never WIDENED it. A tag raised in
/// a sub-row (`just_overflow`'s `{overflow}`) and lifted by `?` into a
/// wider row (`miss`'s `{syntax, deep, overflow}`) arrived at the far
/// handler still carrying the one-tag vocabulary, so `syntax` and `deep`
/// read as binders and the FIRST ARM swallowed the widened tag — `1 2 1`
/// where the checker's lanes print `1 2 3`. `[err.propagate]`'s own
/// comment in the evaluator promised widening-by-union; now the code
/// keeps the promise at the propagation site. The reversed arm order is
/// the same experiment as the test above: under first-arm-wins it flips
/// the answer, under tag dispatch it cannot.
#[test]
fn a_tag_widened_from_a_sub_row_still_dispatches_by_name() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("widened_sub_row");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(dir.join("scan")).expect("scratch created");
    std::fs::write(
        dir.join("scan/scan.lu"),
        "\
//! member: true

fn just_overflow(k: int) -> int ! {overflow} {
    if k == 3 { return overflow }
    k
}

pub fn miss(k: int) -> int ! {syntax, deep, overflow} {
    if k == 1 { return syntax }
    if k == 2 { return deep }
    let v = just_overflow(k)?
    v
}
",
    )
    .expect("the member module is written");

    let source = "\
use scan

fn fwd(k: int) -> int {
    scan.miss(k) else |e| match e {
        syntax => 1,
        deep => 2,
        overflow => 3,
    }
}

fn rev(k: int) -> int {
    scan.miss(k) else |e| match e {
        overflow => 3,
        deep => 2,
        syntax => 1,
    }
}

fn main() -> !int {
    print(\"fwd: {fwd(1)} {fwd(2)} {fwd(3)}\")
    print(\"rev: {rev(1)} {rev(2)} {rev(3)}\")
    0
}
";
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("the entry is written");

    let observation = wolf_interp::frontend::observe_file(
        &entry,
        source.as_bytes(),
        None,
        wolf_interp::eval::Trace::Off,
        &wolf_interp::eval::SchedRequest::Default,
        None,
    );
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "reason: {:?}",
        observation.reason
    );
    assert_eq!(
        String::from_utf8(observation.stdout).expect("utf-8"),
        "fwd: 1 2 3\nrev: 1 2 3\n",
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

// -- D52: a declared row resolves its tags, one position wider -----------
//
// `[gram.expr.tagident]` (ruled 2026-08-26, D52 / wolf-lang#38): a bare
// lowercase identifier in a *checked position* whose expected declared row
// spells the tag resolves as the tag — the raise-site rule generalized to
// call arguments and annotated `let`/`var` initializers. Locals shadow;
// module items, imports and prelude names LOSE to the declared tag. The
// corpus witnesses are `rows/tag_arg_position.lu`, `tag_let_position.lu`,
// `tag_shadow_local.lu` and `negative/tag_undeclared_arg.lu`; these are the
// same shapes at unit size, plus the module-item-loses corner the corpus
// does not pin.

#[test]
fn a_declared_parameter_row_resolves_a_bare_tag_argument() {
    // std.option's motivating shape: `or(none, 9)` injects against the
    // parameter's declared row; `or(4, 9)` is the ok-injection twin.
    let source = "\
fn or(v: int ! {none}, d: int) -> int {
    v else d
}

fn main() -> !int {
    if or(none, 9) == 9 { if or(4, 9) == 4 { return 0 } }
    1
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

#[test]
fn an_annotation_row_resolves_a_bare_tag_initializer() {
    // Both binding spellings: the clause says `let`/`var` alike.
    for kind in ["let", "var"] {
        let source = format!(
            "\
fn main() -> !int {{
    {kind} v: int ! {{none}} = none
    let w = v else 5
    if w == 5 {{ 0 }} else {{ 1 }}
}}
"
        );
        let observation = observe(&source);
        assert_eq!(
            observation.verdict,
            Verdict::Exit(0),
            "{kind}: {:?}",
            observation.reason
        );
    }
}

#[test]
fn a_local_shadows_the_declared_tag_and_the_value_is_the_locals() {
    // D52's priced hazard (rows/tag_shadow_local.lu): the local wins, the
    // callee sees an ok 3, the fallback never fires. W0305's fire-at-use is
    // asserted in `lint::tests`.
    let source = "\
fn or(v: int ! {none}, d: int) -> int {
    v else d
}

fn main() -> !int {
    let none = 3
    if or(none, 9) == 3 { 0 } else { 1 }
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

#[test]
fn an_undeclared_bare_name_keeps_its_refusal_in_both_new_positions() {
    // The rule is exactly as wide as the declared row: `gone` is not a
    // candidate tag, resolves as an ordinary name, misses everything in
    // scope, and the refusal stands (the compiler's E0301; this machine's
    // honest unsupported at resolve). The deferral never trades a typo for
    // silence.
    for source in [
        "\
fn or(v: int ! {none}, d: int) -> int {
    v else d
}

fn main() -> !int {
    if or(gone, 9) == 9 { 0 } else { 1 }
}
",
        "\
fn main() -> !int {
    let v: int ! {none} = gone
    let w = v else 5
    if w == 5 { 0 } else { 1 }
}
",
    ] {
        let observation = observe(source);
        assert_eq!(observation.verdict, Verdict::Unsupported, "{source}");
        assert_eq!(observation.phase_reached, Phase::Resolve, "{source}");
    }
}

#[test]
fn a_module_item_loses_to_the_declared_tag_at_return_position() {
    // The clause's s37 silent-wrong fix, at unit size: `return hex` under
    // `! {hex}` is the TAG even though a module-level `fn hex` is in scope
    // — module items lose, only locals shadow. The raise lands in the
    // handler: exit 3.
    let source = "\
fn hex() -> int {
    7
}

fn f() -> int ! {hex} {
    return hex
}

fn main() -> !int {
    f() else 3
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(3),
        "{:?}",
        observation.reason
    );
}

// -- builtin-raised rows discriminate (issue #47) -----------------------

#[test]
fn a_builtin_raised_row_discriminates_in_both_arm_orders_hermetically() {
    // Issue #47's mechanism without a socket: `env_get` answers the
    // `invalid` row for a name containing `=` (its declared row is
    // `{invalid, missing}`), and a two-arm discriminating handler must land
    // the `invalid` arm in BOTH orders. Before the fix the value carried no
    // row, every lowercase arm read as a binding, and the first arm won —
    // this program answered 1 (the `missing` arm) on the first handler.
    let source = "\
fn main() -> !int {
    let a = env_get(\"no=good\") else |e| match e {
        missing => \"miss\",
        invalid => \"bad\",
    }
    let b = env_get(\"no=good\") else |e| match e {
        invalid => \"bad\",
        missing => \"miss\",
    }
    if a == \"bad\" && b == \"bad\" { 0 } else { 1 }
}
";
    let observation = observe(source);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "the tag finds its own arm in either order; reason: {:?}",
        observation.reason
    );
}

#[test]
fn an_unset_name_lands_the_missing_arm_in_both_orders() {
    // The sibling tag of the same row: the env overlay starts empty (the
    // checked-lane posture), so any well-formed unset name is `missing`
    // deterministically on every host.
    let source = "\
fn main() -> !int {
    let a = env_get(\"IS29_SURELY_UNSET\") else |e| match e {
        invalid => \"bad\",
        missing => \"miss\",
    }
    let b = env_get(\"IS29_SURELY_UNSET\") else |e| match e {
        missing => \"miss\",
        invalid => \"bad\",
    }
    if a == \"miss\" && b == \"miss\" { 0 } else { 1 }
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

//! wolf-lang s216 (#612, `[mem.region.copyout]`): `copy region { … }` on the
//! reference machine — the block's value is copied into the region the block
//! was entered from, and only then is the block's region freed.
//!
//! The programs are wolf-lang's corpus rows `memory/region_copyout_*.lu`
//! verbatim (less their headers), carried here because this machine's
//! vendored corpus is pinned behind them; they join the corpus walk at the
//! re-pin that carries them. Before this mirror lupin read the spelling as a
//! plain `copy` of a plain block, so every copy-out row trapped
//! `region-fault` at the `}` (lupin 0.1.47 and 0.1.48, measured on kasumi).

use wolf_interp::frontend::observe;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn runs(name: &str, source: &str, want: &str) {
    let obs = observe(source.as_bytes(), None);
    assert_eq!(
        obs.verdict,
        Verdict::Exit(0),
        "{name}: {:?} {:?}",
        obs.trap,
        obs.reason
    );
    assert_eq!(String::from_utf8_lossy(&obs.stdout), want, "{name} stdout");
    assert!(obs.leaks.is_empty(), "{name} leaked {:?}", obs.leaks);
}

fn faults(name: &str, source: &str) {
    let obs = observe(source.as_bytes(), None);
    assert_eq!(
        obs.verdict,
        Verdict::Trap(TrapKind::RegionFault),
        "{name}: the compiler's E1010 has region-fault for its dynamic half"
    );
}

/// #612's loop: one `str` kept per turn, the turn's scratch freed.
#[test]
fn the_loop_keeps_each_turn_s_result() {
    runs("loop", LOOP, "piece 49 3-49 50\n");
}

/// The copy is deep: a struct of a `List[str]`, a view and an `int`.
#[test]
fn a_struct_of_strings_and_a_list_copies_out_deep() {
    runs("struct", STRUCT, "plan-7 8 w7-0 w7-2 3 26890\n");
}

/// Maps with string keys and values, a `List[str]`, a tuple.
#[test]
fn maps_and_tuples_copy_out() {
    runs("map", MAP, "value-20 value-30 v1 3 4 (1, two) 26890\n");
}

/// `continue`, `break` and `?` leave without a copy, freeing the region.
#[test]
fn every_other_exit_frees_and_copies_nothing() {
    runs("exits", EXITS, "12 7 none\n");
}

/// A `return` and an outer binding still carry the block's region out.
#[test]
fn every_other_escape_still_faults() {
    faults("return", RETURN);
    faults("binding", BINDING);
}

/// A channel and a fn value have no copy independent of the region.
#[test]
fn a_value_with_no_independent_copy_faults() {
    faults("handle", HANDLE);
    faults("closure", CLOSURE);
}

/// The copy is charged to the ENCLOSING region: the block's own ledger
/// is freed with it, and a named outer region's grows by the copy.
#[test]
fn the_copy_is_charged_to_the_enclosing_region() {
    let source = r#"
fn main() -> !int {
    var n = 0
    region outer {
        let before = region_bytes(outer)
        let v = copy region scratch {
            "built-" + "{7}"
        }
        let after = region_bytes(outer)
        if after > before {
            n = v.len
        }
    }
    print("{n}")
    0
}
"#;
    runs("charge", source, "7\n");
}

const LOOP: &str = r#"
struct St {
    vals: Map[str, str],
    n: int,
}

fn build(i: int) -> str {
    var parts = List[str]()
    var k = 0
    while k < 20 {
        (mut parts).push("piece {i} {k}")
        k += 1
    }
    "{parts[3]}-{i}"
}

fn step(mut st: St, i: int) {
    let v = copy region scratch {
        build(i)
    }
    st.vals["x"] = v
    st.n += 1
}

fn main() -> !int {
    var st = St { vals: Map[str, str](), n: 0 }
    var i = 0
    while i < 50 {
        step(mut st, i)
        i += 1
    }
    print("{st.vals["x"] else "?"} {st.n}")
    0
}
"#;

const STRUCT: &str = r#"
struct Plan {
    words: List[str],
    name: str,
    n: int,
}

fn clobber() -> int {
    var t = 0
    region other {
        var k = 0
        while k < 1000 {
            let s = "ZZZZZZZZZZZZZZZZZZZZZZZZ{k}"
            t += s.len
            k += 1
        }
    }
    t
}

fn plan(i: int) -> Plan {
    var ws = List[str]()
    var k = 0
    while k < 3 {
        (mut ws).push("w{i}-{k}")
        k += 1
    }
    Plan { words: ws, name: "plan-" + "{i}", n: i }
}

fn main() -> !int {
    let p = copy region scratch {
        let q = plan(7)
        let t = "  {q.name}  "
        Plan { words: q.words, name: t.trim(), n: q.n + 1 }
    }
    let t = clobber()
    print("{p.name} {p.n} {p.words[0]} {p.words[2]} {p.words.len} {t}")
    0
}
"#;

const MAP: &str = r#"
struct Env {
    vars: Map[str, str],
    order: List[str],
    hits: Map[str, int],
    pair: (int, str),
}

fn clobber() -> int {
    var t = 0
    region other {
        var k = 0
        while k < 1000 {
            let s = "ZZZZZZZZZZZZZZZZZZZZZZZZ{k}"
            t += s.len
            k += 1
        }
    }
    t
}

fn main() -> !int {
    let e = copy region scratch {
        var vars = Map[str, str]()
        var order = List[str]()
        var hits = Map[str, int]()
        var k = 0
        while k < 4 {
            let name = "v" + "{k}"
            vars[name] = "value-{k * 10}"
            (mut order).push(name)
            hits["h{k}"] = k
            k += 1
        }
        Env { vars: vars, order: order, hits: hits, pair: (1, "t" + "wo") }
    }
    let t = clobber()
    let (a, b) = e.pair
    print(
        "{e.vars["v2"] else "?"} {e.vars["v3"] else "?"} {e.order[1]} {e.hits["h3"] else -1} {e.vars.len} ({a}, {b}) {t}",
    )
    0
}
"#;

const EXITS: &str = r#"
fn first_big(xs: List[int]) -> int ! {none} {
    var i = 0
    while i < xs.len {
        if xs[i] > 100 {
            return xs[i]
        }
        i += 1
    }
    none
}

fn probe() -> str ! {none} {
    let v = copy region scratch {
        let n = first_big([1, 2, 3])?
        "big {n}"
    }
    v
}

fn main() -> !int {
    var total = 0
    var i = 0
    while i < 10 {
        let n = copy region scratch {
            let s = "n{i}"
            if i == 3 {
                i += 1
                continue
            }
            if i == 7 {
                break
            }
            s.len
        }
        total += n
        i += 1
    }
    let r = probe() else "none"
    print("{total} {i} {r}")
    0
}
"#;

const RETURN: &str = r#"
fn f(c: bool) -> str {
    let v = copy region scratch {
        let s = "built-" + "{7}"
        if c {
            return s
        }
        s
    }
    v
}

fn main() -> !int {
    print(f(true))
    0
}
"#;

const BINDING: &str = r#"
fn main() -> !int {
    var out = ""
    let v = copy region scratch {
        out = "built-" + "{7}"
        5
    }
    print("{out} {v}")
    0
}
"#;

const HANDLE: &str = r#"
fn main() -> !int {
    let ch = copy region scratch {
        channel[int](1)
    }
    print("made")
    0
}
"#;

const CLOSURE: &str = r#"
fn main() -> !int {
    let f = copy region scratch {
        let s = "x{1}"
        let g = fn() { s.len }
        g
    }
    print("{f()}")
    0
}
"#;

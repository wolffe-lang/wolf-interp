//! is55 — the index store follows `push` (wolf-lang#438, ruled 2026-09-24;
//! `[mem.region.edge.elem]`, `[gram.expr.assign]` at wolf-lang `0468dead`).
//!
//! s182's witnesses (`tests/rulings/index_store_*`, run by
//! `tests/rulings_s182.rs`) pin the two headline rows. These are the edges
//! around them the clauses also state: `take` is admitted at exactly one
//! store position and nowhere else in an assignment; a moded store on a
//! `read` parameter answers what `push(take v)` answers; a moved binding is
//! usable again once re-initialized; and the copy is a copy at any depth of
//! subscript and for any non-`Copy` value, not only a list.

use wolf_interp::frontend;

/// The verdict and the stdout of one in-process observation.
fn observe(source: &str) -> (String, String) {
    let observation = frontend::observe(source.as_bytes(), None);
    (
        observation.verdict.to_string(),
        String::from_utf8_lossy(&observation.stdout).into_owned(),
    )
}

fn program(body: &str) -> String {
    format!("fn main() -> !int {{\n{body}\n    0\n}}\n")
}

#[test]
fn take_is_refused_everywhere_else_in_an_assignment() {
    // "nowhere else in an assignment may a mode appear … `x = take v`,
    // `s.f = take v` and every compound operator's right-hand side fail to
    // parse exactly as before (E0201)" — `[gram.expr.assign]`.
    for (what, line) in [
        ("a local", "    var y = List[int]()\n    y = take xs"),
        (
            "a field",
            "    var b = Box { items: List[int]() }\n    b.items = take xs",
        ),
        (
            "a compound store",
            "    var n = List[int]()\n    n[0] += take k",
        ),
        (
            "a generic application",
            "    var outs = List[List[int]]()\n    outs[mut 0] = take xs",
        ),
    ] {
        let source = format!(
            "struct Box {{ items: List[int] }}\n{}",
            program(&format!(
                "    var xs = List[int]()\n    var k = 1\n{line}\n    print(\"{{xs.len}}\")"
            ))
        );
        let (verdict, _) = observe(&source);
        assert_eq!(verdict, "fail(E0201)", "{what}: {source}");
    }
}

#[test]
fn the_plain_store_copies_and_the_moded_store_moves() {
    // The pair in one program each, the list and the map: the plain store
    // leaves `xs` live and independent; `take` leaves it dead.
    let (verdict, stdout) = observe(&program(
        "    var xs = List[int]()\n\
         \x20   (mut xs).push(1)\n\
         \x20   var outs = List[List[int]]()\n\
         \x20   (mut outs).push(List[int]())\n\
         \x20   outs[0] = xs\n\
         \x20   (mut xs).push(2)\n\
         \x20   (mut outs[0]).push(9)\n\
         \x20   (mut outs[0]).push(9)\n\
         \x20   print(\"outs0={outs[0].len} xs={xs.len}\")",
    ));
    assert_eq!(
        (verdict.as_str(), stdout.as_str()),
        ("exit(0)", "outs0=3 xs=2\n")
    );

    let (verdict, stdout) = observe(&program(
        "    var xs = List[int]()\n\
         \x20   (mut xs).push(1)\n\
         \x20   var m = Map[str, List[int]]()\n\
         \x20   m[\"k\"] = take xs\n\
         \x20   print(\"stored\")\n\
         \x20   print(\"{xs.len}\")",
    ));
    assert_eq!(verdict, "trap(use-after-move)");
    assert_eq!(
        stdout, "stored\n",
        "the trap is at the read, after the store"
    );
}

#[test]
fn a_moved_binding_is_usable_again_once_reinitialized() {
    // `[mem.tier0.move.4]`: assigning to a moved-from place re-initializes it.
    let (verdict, stdout) = observe(&program(
        "    var xs = List[int]()\n\
         \x20   (mut xs).push(1)\n\
         \x20   var outs = List[List[int]]()\n\
         \x20   (mut outs).push(List[int]())\n\
         \x20   outs[0] = take xs\n\
         \x20   xs = List[int]()\n\
         \x20   (mut xs).push(5)\n\
         \x20   (mut xs).push(6)\n\
         \x20   print(\"outs0={outs[0].len} xs={xs.len}\")",
    ));
    assert_eq!(
        (verdict.as_str(), stdout.as_str()),
        ("exit(0)", "outs0=1 xs=2\n")
    );
}

#[test]
fn take_of_a_value_that_is_not_a_place_is_just_the_value() {
    let (verdict, stdout) = observe(&program(
        "    var outs = List[List[int]]()\n\
         \x20   (mut outs).push(List[int]())\n\
         \x20   outs[0] = take List[int]()\n\
         \x20   print(\"{outs.len} {outs[0].len}\")",
    ));
    assert_eq!((verdict.as_str(), stdout.as_str()), ("exit(0)", "1 0\n"));
}

#[test]
fn a_moded_store_of_a_read_parameter_is_push_take_s_answer() {
    // `[mem.region.edge.elem]`: "`m[k] = take v` on a `read` `v` is E1014,
    // `push(take v)`'s answer". This machine's answer to `push(take v)` on a
    // read parameter is the dynamic `exclusivity` trap (s157); the store
    // gives the same one, from the same code.
    let store = observe(
        "fn set_g(mut m: Map[str, List[int]], v: List[int]) {\n\
         \x20   m[\"l\"] = take v\n\
         }\n\
         fn main() -> !int {\n\
         \x20   var m = Map[str, List[int]]()\n\
         \x20   set_g(mut m, List[int]())\n\
         \x20   0\n\
         }\n",
    );
    let push = observe(
        "fn put_g(mut xs: List[List[int]], v: List[int]) {\n\
         \x20   (mut xs).push(take v)\n\
         }\n\
         fn main() -> !int {\n\
         \x20   var xs = List[List[int]]()\n\
         \x20   put_g(mut xs, List[int]())\n\
         \x20   0\n\
         }\n",
    );
    assert_eq!(store.0, "trap(exclusivity)");
    assert_eq!(store, push);
}

#[test]
fn the_copy_holds_at_any_subscript_depth_and_for_a_struct() {
    // A nested element is an index place too (`index_place ::= expr '['
    // expr ']'` with any `expr` on the left), and a struct holding a list
    // is as non-`Copy` as the list.
    let (verdict, stdout) = observe(&format!(
        "struct Bag {{ items: List[int] }}\n{}",
        program(
            "    var xs = List[int]()\n\
             \x20   (mut xs).push(1)\n\
             \x20   var grid = List[List[List[int]]]()\n\
             \x20   (mut grid).push(List[List[int]]())\n\
             \x20   (mut grid[0]).push(List[int]())\n\
             \x20   grid[0][0] = xs\n\
             \x20   (mut xs).push(2)\n\
             \x20   var b = Bag { items: List[int]() }\n\
             \x20   var bags = Map[int, Bag]()\n\
             \x20   bags[1] = b\n\
             \x20   (mut b.items).push(3)\n\
             \x20   let got = bags[1] else Bag { items: List[int]() }\n\
             \x20   print(\"grid={grid[0][0].len} xs={xs.len} bag={got.items.len} b={b.items.len}\")"
        )
    ));
    assert_eq!(
        (verdict.as_str(), stdout.as_str()),
        ("exit(0)", "grid=1 xs=2 bag=0 b=1\n")
    );
}

#[test]
fn a_field_store_still_moves() {
    // Only the INDEX store changed: `s.f = v` is an initializer of the
    // field and consumes `v` as before (`[mem.tier0.move.1]`).
    let (verdict, _) = observe(&format!(
        "struct Box {{ items: List[int] }}\n{}",
        program(
            "    var xs = List[int]()\n\
             \x20   var b = Box { items: List[int]() }\n\
             \x20   b.items = xs\n\
             \x20   print(\"{xs.len}\")"
        )
    ));
    assert_eq!(verdict, "trap(use-after-move)");
}

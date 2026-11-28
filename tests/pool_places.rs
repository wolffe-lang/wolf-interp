//! Pool places, generation checks and the deliberately bounded accessor tier.
//! Positive rows must execute; unsupported is never a substitute for a trap.

use std::path::Path;
use std::process::{Command, Stdio};
use wolf_interp::eval::SchedRequest;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn corpus(name: &str, stdout: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("pool-home-{name}"));
    std::fs::create_dir_all(&home).unwrap();
    let file = root
        .join(wolf_interp::upstream_root())
        .join("corpus/memory")
        .join(format!("{name}.lu"));
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["conform-run", "--json"])
        .arg(file)
        .current_dir(root)
        .env("HOME", home)
        .env("SHELL", "/bin/sh")
        .stdin(Stdio::null())
        .output()
        .expect("conform-run executes");
    assert!(out.status.success(), "{name}: {out:?}");
    let r: serde_json::Value = serde_json::from_slice(&out.stdout).expect("one JSON record");
    assert_eq!(r["verdict"], "exit(0)", "{name}: {r}");
    assert_eq!(r["phase_reached"], "run", "{name}: {r}");
    assert_eq!(r["seeded"], false, "{name}: {r}");
    assert_eq!(r["diagnostics"], serde_json::json!([]), "{name}: {r}");
    assert!(r.get("x-unsupported").is_none(), "{name}: {r}");
    assert_eq!(
        r["stdout_inline"].as_str().unwrap_or_default(),
        stdout,
        "{name}: {r}"
    );
    if !stdout.is_empty() {
        assert_eq!(
            r["stdout_sha256"],
            wolf_interp::sha256::hex(stdout.as_bytes()),
            "{name}: {r}"
        );
    }
}

macro_rules! corpus_row {
    ($test:ident, $file:literal, $out:literal) => {
        #[test]
        fn $test() {
            corpus($file, $out);
        }
    };
}
corpus_row!(
    pool_header_after_move_corpus,
    "elem_header_methods_after_move_pool",
    "1 1 false\n"
);
corpus_row!(pool_move_revive_corpus, "elem_pool_handle_revive", "2\n");
corpus_row!(pool_place_write_corpus, "pool_place_write", "");
corpus_row!(pool_rhs_growth_corpus, "store_rhs_first_pool", "7 65\n");
corpus_row!(pool_accessors_corpus, "pool_accessors", "");

fn observe(
    source: &str,
) -> (
    wolf_interp::protocol::ObservationRecord,
    wolf_interp::Observed,
) {
    wolf_interp::observe_record_stdin(source.as_bytes(), &SchedRequest::Default)
}
fn ints(body: &str) -> String {
    format!("fn main() -> !int {{ region r: pool(int) {{ var p = Pool[int]()\n{body}\n0 }} }}")
}
fn initialized(body: &str) -> String {
    ints(&format!(
        "let h = (mut p).reserve()\n(mut p).init(h, 10)\n{body}"
    ))
}
fn nodes(body: &str) -> String {
    format!(
        "struct Node {{ tags: List[int] }}\nfn main() -> !int {{ region r: pool(Node) {{ var p = Pool[Node]()\nlet h = (mut p).reserve()\n(mut p).init(h, Node {{ tags: [1] }})\n{body}\n0 }} }}"
    )
}
fn exits(source: &str, stdout: &str) {
    let (r, out) = observe(source);
    assert_eq!(r.verdict, Verdict::Exit(0), "{r:?}\n{out:?}");
    assert_eq!(out.stdout, stdout, "{r:?}");
}
fn traps(source: &str, kind: TrapKind) {
    let (r, out) = observe(source);
    assert_eq!(r.verdict, Verdict::Trap(kind), "{r:?}\n{out:?}");
}
fn unsupported(source: &str, reason: &str) {
    let (r, out) = observe(source);
    assert_eq!(r.verdict, Verdict::Unsupported, "{r:?}\n{out:?}");
    let json = serde_json::to_value(r).unwrap();
    assert_eq!(json["x-unsupported"], reason, "{json}\n{out:?}");
}

#[test]
fn whole_slot_and_compound_store_use_copied_handle_identity() {
    exits(
        &initialized("let k = h\np[h] = 12\np[k] += 3\nprint(\"{p[h]}\")"),
        "15\n",
    );
}
#[test]
fn a_store_does_not_touch_the_adjacent_slot() {
    exits(
        &initialized(
            "let k = (mut p).reserve()\n(mut p).init(k, 20)\np[h] = 30\nprint(\"{p[k]} {p[h]}\")",
        ),
        "20 30\n",
    );
}
#[test]
fn moved_payload_is_unreadable_through_a_copied_handle() {
    traps(
        &nodes("let k = h\nlet value = move p[h]\nprint(\"{p[k].tags.len}\")"),
        TrapKind::UseAfterMove,
    );
}
#[test]
fn a_whole_pool_method_cannot_read_a_moved_payload() {
    traps(
        &nodes(
            "let k = (mut p).reserve()\n(mut p).init(k, Node { tags: [2] })\nlet value = move p[h]\np.get(k)",
        ),
        TrapKind::UseAfterMove,
    );
}
#[test]
fn reserved_payload_read_is_uninitialized_not_stale() {
    traps(
        &ints("let h = (mut p).reserve()\nprint(\"{p[h]}\")"),
        TrapKind::UseAfterMove,
    );
}
#[test]
fn stale_after_reuse_read() {
    traps(
        &initialized(
            "(mut p).remove(h)\nlet k = (mut p).reserve()\n(mut p).init(k, 40)\nprint(\"{p[h]}\")",
        ),
        TrapKind::StaleHandle,
    );
}
#[test]
fn stale_after_reuse_store() {
    traps(
        &initialized(
            "(mut p).remove(h)\nlet k = (mut p).reserve()\n(mut p).init(k, 40)\np[h] = 99",
        ),
        TrapKind::StaleHandle,
    );
}
#[test]
fn stale_after_clear_and_reuse_read() {
    traps(
        &initialized(
            "(mut p).clear()\nlet k = (mut p).reserve()\n(mut p).init(k, 40)\nprint(\"{p[h]}\")",
        ),
        TrapKind::StaleHandle,
    );
}
#[test]
fn stale_after_clear_and_reuse_store() {
    traps(
        &initialized("(mut p).clear()\nlet k = (mut p).reserve()\n(mut p).init(k, 40)\np[h] = 99"),
        TrapKind::StaleHandle,
    );
}
#[test]
fn handle_operand_runs_once_before_rhs_and_store_uses_grown_pool() {
    let source = r#"
fn key(h: handle int, mut calls: int) -> handle int { calls += 1; print("key"); h }
fn grow(mut p: Pool[int]) -> int { let k = (mut p).reserve(); (mut p).init(k, 22); print("rhs"); 77 }
fn main() -> !int { region r: pool(int) {
 var p = Pool[int]()
 let h = (mut p).reserve()
 (mut p).init(h, 10)
 var calls = 0
 p[key(h, mut calls)] = grow(mut p)
 print("{calls} {p[h]} {p.len()}")
 0
} }
"#;
    exits(source, "key\nrhs\n1 77 2\n");
}

const RESERVED_ALIVE: &str =
    "Pool.alive on a reserved slot is outside the pinned accessor contract";
const RESERVED_HAS: &str = "Pool.has on a reserved slot is outside the pinned accessor contract";
const RESERVED_CLEAR: &str =
    "Pool.clear with reserved slots is outside the pinned accessor contract";
const OTHER_ALIVE: &str =
    "Pool.alive with a handle from another pool is outside the pinned accessor contract";
const OTHER_HAS: &str =
    "Pool.has with a handle from another pool is outside the pinned accessor contract";

#[test]
fn reserved_alive_is_explicitly_outside_scope() {
    unsupported(
        &ints("let h = (mut p).reserve()\np.alive(h)"),
        RESERVED_ALIVE,
    );
}
#[test]
fn reserved_has_is_explicitly_outside_scope() {
    unsupported(&ints("let h = (mut p).reserve()\np.has(h)"), RESERVED_HAS);
}
#[test]
fn clear_with_reservations_is_explicitly_outside_scope() {
    unsupported(
        &ints("let h = (mut p).reserve()\n(mut p).clear()"),
        RESERVED_CLEAR,
    );
}
#[test]
fn wrong_pool_alive_is_explicitly_outside_scope() {
    unsupported(&initialized("var q = Pool[int]()\nq.alive(h)"), OTHER_ALIVE);
}
#[test]
fn wrong_pool_has_is_explicitly_outside_scope() {
    unsupported(&initialized("var q = Pool[int]()\nq.has(h)"), OTHER_HAS);
}
#[test]
fn wrong_pool_store_cannot_overwrite_a_matching_index() {
    unsupported(
        &initialized(
            "var q = Pool[int]()\nlet k = (mut q).reserve()\n(mut q).init(k, 22)\nq[h] = 99",
        ),
        "Pool place handle belongs to a different pool",
    );
}
#[test]
fn wrong_pool_read_cannot_redirect_to_the_handles_owner() {
    let (r, out) = observe(&initialized(
        "var q = Pool[int]()\nlet k = (mut q).reserve()\n(mut q).init(k, 22)\nprint(\"{q[h]}\")",
    ));
    assert_eq!(r.verdict, Verdict::Unsupported, "{r:?}\n{out:?}");
    let json = serde_json::to_value(r).unwrap();
    assert!(
        json["x-unsupported"].as_str().unwrap().contains("one pool"),
        "{json}"
    );
}
#[test]
fn a_nested_payload_method_writes_back_to_the_pool() {
    exits(
        &nodes("(mut p[h].tags).push(2)\nprint(\"{p[h].tags.len}\")"),
        "2\n",
    );
}

fn outside_pool(body: &str) -> String {
    format!(
        "fn main() -> !int {{\nlet r = region(pool(int))\nvar p = in r {{ Pool[int]() }}\nlet h = in r {{ let h = (mut p).reserve(); (mut p).init(h, 10); h }}\n{body}\n0 }}"
    )
}
#[test]
fn a_pool_place_write_checks_the_suspended_region() {
    traps(&outside_pool("p[h] = 12"), TrapKind::RegionFault);
}
#[test]
fn a_pool_place_write_checks_the_frozen_region() {
    traps(
        &outside_pool("let frozen = freeze r\np[h] = 12"),
        TrapKind::RegionFault,
    );
}
#[test]
fn a_frozen_pool_payload_remains_readable() {
    exits(
        &outside_pool("let frozen = freeze r\nprint(\"{p[h]}\")"),
        "10\n",
    );
}
#[test]
fn a_freed_pool_place_store_stays_a_region_fault() {
    traps(
        r#"
struct Node { value: int }
fn main() -> !int {
 var p = Pool[Node]()
 var h = (mut p).reserve()
 region r: pool(Node) {
  var inner = Pool[Node]()
  h = (mut inner).reserve()
  (mut inner).init(h, Node { value: 1 })
  p = move inner
 }
 p[h].value = 99
 0
}
"#,
        TrapKind::RegionFault,
    );
}
#[test]
fn a_pool_field_store_checks_cross_region_edges() {
    traps(
        r#"
struct Node { value: int, link: handle Node }
fn main() -> !int {
 region a: pool(Node) {
  var pa = Pool[Node]()
  let ha = (mut pa).reserve()
  (mut pa).init(ha, Node { value: 1, link: ha })
  region b: pool(Node) {
   var pb = Pool[Node]()
   let hb = (mut pb).reserve()
   (mut pb).init(hb, Node { value: 2, link: hb })
   pb[hb].link = ha
  }
 }
 0
}
"#,
        TrapKind::RegionFault,
    );
}
#[test]
fn copied_handles_cannot_evade_one_calls_exclusivity() {
    let source = format!(
        "fn set(mut a: int, mut b: int) {{ a = 1; b = 2 }}\n{}",
        initialized("let k = h\nset(mut p[h], mut p[k])")
    );
    traps(&source, TrapKind::Exclusivity);
}
#[test]
fn separate_slots_can_be_lent_to_the_same_call() {
    let source = format!(
        "fn set(mut a: int, mut b: int) {{ a = 1; b = 2 }}\n{}",
        initialized(
            "let k = (mut p).reserve()\n(mut p).init(k, 20)\nset(mut p[h], mut p[k])\nprint(\"{p[h]} {p[k]}\")"
        )
    );
    exits(&source, "1 2\n");
}
#[test]
fn read_parameters_do_not_gain_pool_write_permission() {
    let source = format!(
        "fn inspect(p: Pool[int], h: handle int) {{ p[h] = 22 }}\n{}",
        initialized("inspect(p, h)")
    );
    traps(&source, TrapKind::Exclusivity);
}
#[test]
fn unordered_pool_place_writes_are_a_simulated_race() {
    traps(
        r#"
fn main() -> !int {
 region r: pool(int) {
  var p = Pool[int]()
  let h = (mut p).reserve()
  (mut p).init(h, 0)
  scope s {
   s.spawn(fn() { p[h] = 1 })
   s.spawn(fn() { p[h] = 2 })
  }
 }
 0
}
"#,
        TrapKind::Race,
    );
}

#[test]
fn a_handle_is_not_admitted_as_a_map_key() {
    let (r, out) = observe(&initialized("var m = Map[int, int]()\nm[h] = 9"));
    assert_eq!(r.verdict, Verdict::Unsupported, "{r:?}\n{out:?}");
    let json = serde_json::to_value(r).unwrap();
    assert!(
        json["x-unsupported"]
            .as_str()
            .unwrap()
            .contains("[type.map.key]"),
        "{json}"
    );
}

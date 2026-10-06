//! The provenance machine, observed through whole programs.
//!
//! `src/eval/prov.rs`'s own tests drive the machine directly — the transition
//! table, the protector escalation, angelic resolution. This file asserts the
//! things only a *program* can show: that the corpus's two provenance litmuses
//! are provenance-checked rather than merely executed, that the Tree-Borrows
//! choice is demonstrably encoded (the SB-regression suite), that
//! `--trace=prov` is exactly the Tier-3 subset of `--trace`, and that a UB
//! report renders the two spans and the borrow tree a reader needs.

use std::path::{Path, PathBuf};

use wolf_interp::eval::Trace;
use wolf_interp::eval::prov::{AccessKind, Prov, Provenance, RetagKind, UbRow};
use wolf_interp::eval::rules::Rule;
use wolf_interp::frontend;
use wolf_interp::protocol::Verdict;

fn corpus(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
        .join(name);
    std::fs::read_to_string(path).expect("the pinned corpus is readable")
}

fn ub_program(name: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("ub")
        .join(name);
    std::fs::read_to_string(path).expect("readable")
}

fn trace_of(source: &str, filter: Trace) -> Vec<String> {
    let program = wolf_interp::sema::load_source("t.lu", source).expect("parses");
    wolf_interp::eval::Machine::new(&program)
        .tracing(filter)
        .run()
        .trace
}

// -- the corpus litmuses, now provenance-checked ---------------------------

#[test]
fn the_two_phase_litmus_keeps_its_receiver_reserved_across_the_argument_read() {
    // `corpus/memory/prov_two_phase.lu`: "the receiver's mut tag is Reserved
    // while the argument reads the same container, activating only at the
    // write." Running it green is not evidence — a machine with no provenance
    // at all runs it green. The evidence is in the trace: a Reserved child, a
    // foreign read that does not disturb it, then the activation.
    let source = corpus("memory/prov_two_phase.lu");
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(observation.verdict, Verdict::Exit(0));
    assert!(observation.ub.is_none());

    let trace = trace_of(&source, Trace::Provenance);
    let retags: Vec<&String> = trace
        .iter()
        .filter(|line| line.contains("retag `t0:1:xs`") && line.contains("Reserved"))
        .collect();
    assert!(
        !retags.is_empty(),
        "the receiver's retag is Reserved, not Active — creation is not a use:\n{trace:#?}"
    );
    assert!(
        trace.iter().any(|line| line.contains("PROTECTED")),
        "and it is protected for the call's extent:\n{trace:#?}"
    );
    // Every line the `prov` filter kept cites a Tier-3 clause.
    for line in &trace {
        assert!(
            line.contains("[mem.prov")
                || line.contains("[mem.unsafe")
                || line.contains("[mem.ub")
                || line.contains("[mem.boundary"),
            "a non-Tier-3 rule survived the `prov` filter: {line}"
        );
    }
}

#[test]
fn the_holy_grail_litmus_freezes_its_read_parameter_for_the_whole_call() {
    // `corpus/memory/prov_holy_grail.lu`: a `read` parameter is immutable for
    // the whole call, so the second load may be hoisted across the opaque
    // callback (§7/O2). The machine's side of that promise is a **Frozen,
    // protected** child tag for the call's extent — which is what makes a
    // foreign write during the extent UB rather than merely invalidating.
    let source = corpus("memory/prov_holy_grail.lu");
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(observation.verdict, Verdict::Exit(0));

    let trace = trace_of(&source, Trace::Provenance);
    assert!(
        trace
            .iter()
            .any(|line| line.contains("Frozen") && line.contains("PROTECTED")),
        "no Frozen protector for the `read` parameter:\n{trace:#?}"
    );
    assert!(
        trace
            .iter()
            .any(|line| line.contains("protector") && line.contains("released")),
        "the protector must lift when the call ends:\n{trace:#?}"
    );
}

#[test]
fn the_unsafe_litmuses_run_and_the_uaf_one_is_the_oracle_first_verdict() {
    let noalias = corpus("memory/unsafe_noalias.lu");
    let observation = frontend::observe(noalias.as_bytes(), None);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "a *true* `assume noalias` is defined behavior: {:?}",
        observation.reason
    );

    let uaf = corpus("memory/unsafe_ub_uaf.lu");
    let observation = frontend::observe(uaf.as_bytes(), None);
    assert_eq!(observation.verdict, Verdict::Ub("mem.ub".to_owned()));
    let finding = observation.ub.expect("a finding");
    // The file's own comment names the row and the clause: "§7/P1 …
    // `[mem.prov.region]`".
    assert_eq!(finding.row, UbRow::P1);
    assert!(finding.message.contains("Disabled"), "{}", finding.message);
    assert!(
        finding.tree.iter().any(|line| line.contains("FREED")),
        "the tree slice shows the freed allocation:\n{:#?}",
        finding.tree
    );
}

// -- the SB-regression suite -----------------------------------------------

#[test]
fn references_created_but_never_used_do_not_poison_later_ones() {
    // The sprint's SB-regression suite. Stacked Borrows' most-reported false
    // rejection is the reference that is *created* and then made unusable by an
    // access through its parent that happens before it is ever used; Tree
    // Borrows leaves Reserved alone on a foreign read, and reports/01 measures
    // −54% false rejections for the change.
    //
    // Written with the re-entry door, because that is the retag this machine
    // performs *without* a protector — a parameter retag is protected for the
    // call's extent, and a protector turns the very same foreign access into
    // UB by design (`tests/ub/p1_protector.lu` is that program). What is under
    // test here is the unprotected reborrow: created, not yet used, and read
    // past.
    let source = "\
import c \"stdlib.h\"

fn main() -> !int {
    let r = region()
    unsafe {
        let p = in r { c.malloc(4) as *u8 }
        c.memset(p, 1, 4)
        let b = borrow r from p
        let seen = p[0] as int
        b[0] = 9
        let after = p[0] as int
        c.free(p)
        if seen == 1 && after == 9 { 0 } else { 1 }
    }
}
";
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "a Reserved reborrow must survive a foreign read: {:?} {:?}",
        observation.reason,
        observation.ub.map(|f| f.to_string()),
    );
}

#[test]
fn a_reserved_reborrow_still_activates_after_several_foreign_reads() {
    // The same property under repetition: no number of foreign reads turns a
    // Reserved tag into a dead one. (A stack-based machine pops it on the
    // first.)
    let source = "\
import c \"stdlib.h\"

fn main() -> !int {
    let r = region()
    var total = 0
    unsafe {
        let p = in r { c.malloc(4) as *u8 }
        c.memset(p, 2, 4)
        let b = borrow r from p
        total = total + p[0] as int
        total = total + p[0] as int
        total = total + p[0] as int
        b[0] = 4
        total = total + p[0] as int
        c.free(p)
    }
    if total == 10 { 0 } else { 1 }
}
";
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "{:?}",
        observation.ub.map(|f| f.to_string())
    );
}

// -- the protector guarantee, traced ---------------------------------------

#[test]
fn the_retag_then_opaque_call_program_demonstrates_the_protector() {
    // The sprint's named acceptance program: "interpreter trace demonstrates the
    // protector guarantee (foreign write during protected extent → UB), and the
    // paired licensed-optimization note names the fold."
    //
    // Through 0.1.45 the program below was that evidence, run frontend-free:
    // its `read` parameter `a: *u8` took a Frozen, PROTECTED child at entry,
    // so `b[0] = 3` through the caller's other pointer was §7/P1. kw02's
    // `[mem.unsafe.sig]` (R1; wolf-interp#181, is70) rules the opposite for a
    // raw pointer: "passed by value is a copy of the pointer and nothing
    // more: no retag, no freeze of the pointee". So the program now runs —
    // `a[0]` reads the 3 — and the trace says why. The protector itself is
    // unchanged and still serves Tier-0 places; it is demonstrated below on
    // the provenance machine directly, the shape a parameter entry mints.
    let source = "\
import c \"stdlib.h\"

fn observe(a: *u8, mut b: *u8) -> int {
    b[0] = 3
    a[0] as int
}

fn main() -> !int {
    unsafe {
        let p = c.malloc(8) as *u8
        p[0] = 1
        var q = p
        let n = observe(p, mut q)
        c.free(p)
        n
    }
}
";
    let program = wolf_interp::sema::load_source("t.lu", source).expect("parses");
    let run = wolf_interp::eval::Machine::new(&program)
        .tracing(Trace::All)
        .run();
    assert!(
        matches!(run.outcome, wolf_interp::eval::Outcome::Exit(3)),
        "a raw pointer argument is a copy: {:?}",
        run.outcome
    );
    let joined = run.trace.join("\n");
    assert!(
        joined.contains("passes as a copy of the pointer: no retag at parameter entry"),
        "{joined}"
    );
    assert!(!joined.contains("PROTECTED"), "{joined}");

    // The protector, as a parameter entry mints it: a protected child, then a
    // foreign write during the extent is §7/P1 at the write; after the extent
    // the same write merely Disables the tag.
    let span = wolf_interp::diag::Span::new(0, 1);
    let mut prov = Provenance::new();
    let root = prov.host_alloc(4, 0, span);
    prov.init_range(root, 4);
    let (Some(alloc), Prov::Tag(root_tag)) = (root.alloc, root.prov) else {
        panic!("a host allocation carries a root tag: {root:?}")
    };
    let child = prov.retag(alloc, root_tag, RetagKind::Shared, true, "parameter", span);
    let finding = prov
        .access(root, 1, AccessKind::Write, span)
        .expect_err("a foreign write past a protected tag is UB");
    assert_eq!(finding.row, UbRow::P1);
    assert!(finding.message.contains("PROTECTED"), "{}", finding.message);
    assert!(
        UbRow::P1.optimization().contains("noalias"),
        "the row names the fold it licenses: {}",
        UbRow::P1.optimization()
    );
    prov.unprotect(child, span);
    prov.access(root, 1, AccessKind::Write, span)
        .expect("the extent ended: the write only Disables the tag");
}

// -- the trace namespace ----------------------------------------------------

#[test]
fn the_prov_filter_is_exactly_the_tier_three_subset_of_the_full_trace() {
    // The same contract is03 established for `--trace=mem`: the filter is
    // derived from the anchor, so it cannot drift away from the rule registry.
    let source = ub_program("l2_dangling.lu");
    let all = trace_of(&source, Trace::All);
    let prov = trace_of(&source, Trace::Provenance);
    let expected: Vec<String> = all
        .iter()
        .filter(|line| {
            Rule::ALL
                .iter()
                .filter(|rule| rule.is_provenance())
                .any(|rule| line.contains(&format!("[{}]", rule.anchor())))
        })
        .cloned()
        .collect();
    assert_eq!(prov, expected);
    assert!(!prov.is_empty());
    assert!(prov.len() < all.len(), "the filter must filter something");

    // `mem` is the wider net: every Tier-3 rule cites a `mem.*` clause, so the
    // provenance trace is a subset of the memory one.
    let mem = trace_of(&source, Trace::Memory);
    for line in &prov {
        assert!(mem.contains(line), "not in the `mem` trace: {line}");
    }
}

#[test]
fn the_trace_filter_names_are_the_ones_the_cli_documents() {
    for (spelling, want) in [
        ("all", Trace::All),
        ("", Trace::All),
        ("mem", Trace::Memory),
        ("memory", Trace::Memory),
        ("prov", Trace::Provenance),
        ("provenance", Trace::Provenance),
        ("off", Trace::Off),
    ] {
        assert_eq!(spelling.parse::<Trace>(), Ok(want), "{spelling}");
    }
    assert!("tags".parse::<Trace>().is_err());
}

// -- the report -------------------------------------------------------------

#[test]
fn every_ub_report_carries_two_spans_a_tree_and_the_optimization_it_licenses() {
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    let _guard = settings.bind_to_scope();

    // One snapshot per row, rendered the way a human reads it. Spans are shown
    // as the source they cover rather than as byte offsets — the is03 rule:
    // a snapshot full of offsets churns on whitespace and tells nobody
    // anything, while slicing the source with those offsets keeps them exact.
    // P1's program is the corpus's own witness (`memory/unsafe_ub_uaf.lu`):
    // the protector-form trigger retired at pin f0da6e6 — its `*u8`
    // parameters are E1302 now — and T1's trigger left with the bool cast
    // column (E0805); both retirements are `Coverage::Unreachable` entries
    // with their reasons in `UbRow::coverage`.
    for name in [
        "unsafe_ub_uaf.lu",
        "p2_frozen_write.lu",
        "p3_out_of_bounds.lu",
        "p4_region_freed.lu",
        "p5_false_noalias.lu",
        "p6_false_door.lu",
        "l1_uninitialized.lu",
        "l2_dangling.lu",
        "l3_misaligned_volatile.lu",
        "l4_misaligned.lu",
    ] {
        let source = if name == "unsafe_ub_uaf.lu" {
            corpus("memory/unsafe_ub_uaf.lu")
        } else {
            ub_program(name)
        };
        let observation = frontend::observe(source.as_bytes(), None);
        let finding = observation
            .ub
            .unwrap_or_else(|| panic!("{name}: {}", observation.verdict));
        let slice = |span: wolf_interp::diag::Span| {
            source
                .get(span.start..span.end)
                .unwrap_or("<span outside the source>")
                .to_owned()
        };
        let mut out = String::new();
        out.push_str(&format!("program:   {name}\n"));
        out.push_str(&format!("verdict:   ub({})\n", finding.anchor()));
        out.push_str(&format!("row:       §7/{}\n", finding.row));
        out.push_str(&format!("clause:    [{}]\n", finding.row.clause()));
        out.push_str(&format!("what:      {}\n", finding.row.what()));
        out.push_str(&format!("licenses:  {}\n", finding.row.optimization()));
        out.push_str(&format!("message:   {}\n", finding.message));
        out.push_str(&format!("access:    `{}`\n", slice(finding.span)));
        match finding.tag_span {
            Some(span) => out.push_str(&format!("tag from:  `{}`\n", slice(span))),
            None => out.push_str("tag from:  (no tag — a value-representation row)\n"),
        }
        out.push_str("tree:\n");
        for line in &finding.tree {
            out.push_str(&format!("  {line}\n"));
        }
        insta::assert_snapshot!(name.trim_end_matches(".lu").to_owned(), out);
    }
}

#[test]
fn a_ub_record_carries_the_row_and_the_pairing_on_extension_keys() {
    // `[proto.record.ub]`: "`ub(anchor)` cites the s04 §7 row (e.g.,
    // `ub(mem.ub)` with the row id in `x-ub-row`, or the specific clause)".
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("ub")
        .join("p4_region_freed.lu");
    let source = std::fs::read(&path).expect("readable");
    let (record, _) = wolf_interp::observe_record(&path, &source, None);
    assert_eq!(record.verdict, Verdict::Ub("mem.ub".to_owned()));
    assert_eq!(
        record.extensions.get("x-ub-row").and_then(|v| v.as_str()),
        Some("P4")
    );
    assert_eq!(
        record
            .extensions
            .get("x-ub-clause")
            .and_then(|v| v.as_str()),
        Some("mem.prov.region")
    );
    assert!(record.extensions.contains_key("x-ub-span"));
    assert!(record.extensions.contains_key("x-ub-tag-span"));
    assert!(record.extensions.contains_key("x-ub-tree"));
    let licenses = record
        .extensions
        .get("x-ub-licenses")
        .and_then(|v| v.as_str())
        .expect("the D2 pairing rides the record");
    assert!(licenses.contains("O3b"), "{licenses}");

    // And the record this implementation emits must pass its own validator.
    let value = serde_json::to_value(&record).expect("serializes");
    assert_eq!(wolf_interp::schema::validate(&value), Ok(()));
}

// -- the C-intrinsic approximation ------------------------------------------

#[test]
fn a_c_function_outside_the_modelled_set_is_declined_not_guessed() {
    // The approximation contract's rule, as a test: the host-intrinsic set is
    // small and closed, and a C name outside it is `unsupported` with a reason.
    // Inventing a body for a real libc call would put guessed behavior into a
    // differential comparison.
    let source = "\
import c \"string.h\"

fn main() -> !int {
    unsafe {
        let n = c.strlen(0) as int
        n
    }
}
";
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(observation.verdict, Verdict::Unsupported);
    let reason = observation.reason.expect("a reason");
    assert!(reason.contains("c.strlen"), "{reason}");
    assert!(reason.contains("approximation-contract"), "{reason}");
}

#[test]
fn a_c_call_exposes_its_pointers_rather_than_retagging_them() {
    // `[mem.prov.expose]`: "Wildcard pointers from FFI behave as exposed."
    // A C function has no wolf parameters and no modes, so retagging its
    // arguments would invent a `read` borrow — and then report the callee's own
    // write through it as §7/P2, which is `corpus/ffi.lu`'s shape exactly.
    let source = "\
import c \"stdlib.h\"

fn main() -> !int {
    unsafe {
        let p = c.malloc(8) as *u8
        c.memset(p, 3, 8)
        let v = p[0] as int
        c.free(p)
        if v == 3 { 0 } else { 1 }
    }
}
";
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "{:?}",
        observation.ub.map(|f| f.to_string())
    );
    let trace = trace_of(source, Trace::Provenance);
    assert!(
        trace
            .iter()
            .any(|line| line.contains("crosses the C membrane") && line.contains("exposed")),
        "{trace:#?}"
    );
}

#[test]
fn calloc_allocates_count_times_size_bytes_and_zeroes_all_of_them() {
    // Issue #13, found by s29's native differential: `c.calloc(n, size)` is
    // `n * size` bytes in C, and the model gave it `n`. `calloc(8, 8)` must
    // be a 64-byte block — writable at byte 63, zeroed over the whole range
    // (the zeroing is observable: reading byte 63 uninitialized would be
    // §7/L1). `corpus/memory/unsafe_c_alloc_native.lu` pins the same law
    // against real glibc; this litmus pins it without the corpus.
    let source = "\
import c \"stdlib.h\"
import c \"string.h\"

fn main() -> !int {
    unsafe {
        let q = c.calloc(8, 8) as *u8
        let zero = q[63] as int
        c.memset(q, 5, 64)
        let five = q[63] as int
        c.free(q)
        if zero == 0 && five == 5 { 0 } else { 1 }
    }
}
";
    let observation = frontend::observe(source.as_bytes(), None);
    assert_eq!(
        observation.verdict,
        Verdict::Exit(0),
        "{:?} {:?}",
        observation.reason,
        observation.ub.map(|f| f.to_string())
    );
}

#[test]
fn unfreed_c_allocations_are_reported_and_never_faulted() {
    // `[mem.ub.defined]`: "Memory leak (`shared` kept alive, region never freed)
    // → defined, safe". The C heap is no different — the report exists so
    // is06's crash-cleanup oracle can read it, not so anything can fault on it.
    let source = "\
import c \"stdlib.h\"

fn main() -> !int {
    unsafe {
        let p = c.malloc(8) as *u8
        c.memset(p, 1, 8)
        p[0] as int
    }
}
";
    let program = wolf_interp::sema::load_source("t.lu", source).expect("parses");
    let run = wolf_interp::eval::Machine::new(&program).run();
    assert_eq!(run.outcome, wolf_interp::eval::Outcome::Exit(1));
    assert_eq!(run.host_leaks.len(), 1, "one allocation was never freed");
}

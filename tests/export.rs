//! is09 — the conformance bundle's own gates.
//!
//! Determinism (re-export is byte-identical — the I10 spirit), integrity
//! (a tampered bundle is refused), the consumption dry-run (pull → verify →
//! diff, with recorded results standing in for a second implementation), and
//! the coverage **ratchet**: the covered-anchor count may grow, never
//! shrink. The cross-OS half of determinism — linux/mac/windows bundles
//! hashing identical — is a CI assertion (`bundle determinism` +
//! `bundle-identical` in `.github/workflows/ci.yml`), because one machine
//! cannot test three.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use wolf_interp::export::{self, CheckImpl, ExportOptions, ExportSummary};

/// The coverage ratchet's floor: the covered-anchor count at the is09
/// export, pin `cbde620`. A PR may move this number **up** (backfill, new
/// corpus files, a pin bump that adds cited clauses) and must then raise the
/// floor; a PR that drops it below the floor — deleting a tagged test,
/// losing a `conforms:` line — fails here, which is the ratchet doing its
/// job. Never lower this without the closeout-level negotiation the sprint
/// requires. Raised 86 → 90 at pin `f0da6e6` (0.1.5): the strings tier
/// cites `mem.str.get` and the lints tier `diag.level.attr` and
/// `mem.boundary.doc`, among the wave's new clauses. Raised 90 → 95 at pin
/// `13b811f` (0.1.6): the s68 lint fixtures, the s34 proc pair, the
/// P-project witnesses and the issue #20 freeze-read twin cite
/// `conc.when.nonest`, `conc.chan.default`, `mem.region.freeze.4` and the
/// wave's other new clauses. Raised 96 → 97 at pin `e94b879` (0.1.7): the
/// wave-six corpus (s69 idiom lints, s70 match tier + X3 value paths, s40
/// os/time/json) lands with its `conforms:` citations. Raised 97 → 102
/// at pin `26fa98e` (0.1.8): the s71 witnesses cite `[mem.str.empty]`,
/// `[mem.str.repeat]`, the s72 mode-teeth fail-files cite
/// `[mem.iter.excl.*]`, and the suite's D39/D40 programs land with
/// theirs. Raised 102 → 103 at pin `613c3dc` (0.1.10): the mid-end pin's
/// `conc/select_two_timeouts.lu` — the #64 regression litmus, two timeout
/// selects in one body — is the first program in the whole corpus to cite
/// `[conc.select.timeout]`, and it runs clean here at first sight. Raised
/// 103 → 107 at pin `f8dca42` (0.1.11): four clauses gain their first
/// citing program in the s74…s78 wave — `[gram.lex.shebang]` (the new
/// clause itself, by `grammar/shebang.lu`), `[conc.chan.close]` and
/// `[mem.iter.range]` (both by `conc/chan_drain_after_inclusive_loop.lu`,
/// the ch12 router shape) and `[mem.tier0.move]` (by
/// `memory/mut_param_aggregate_store.lu`). All four run clean here.
/// RATCHETED to 114 by pin `02c1e88`: the s88/s89/s90 + is13 wave. s89
/// wrote `[mem.str.view.lend]` before the code and cites it from two new
/// corpus files; s90's fs builtins and s88's three holes bring the rest.
/// A ratchet is a floor, not a target — it moves up when coverage does,
/// and never down without the gap list on the table.
// 114 → 118 at c9da6d9: the wave's four newly covered clauses —
// generics.mono/.nominal/.row-tail witnesses and ty.dyn.unsize.place — each
// cited by the new corpus files alone. Raised in the bump commit per the
// test's own instruction.
// 118 → 121 at b522b8a: the c25 wave's newly covered clauses —
// [abi.native.closure] (cited by closure_escape_refused/closure_value_paths),
// plus the region-value and explicit-application citations the new corpus
// files bring. Raised in the bump commit per the test's own instruction.
// 121 → 125 at 1b149ba: the s108 wave's newly covered clauses — the
// gram.lex.str.raw citation lands on run-reaching files (raw_fences +
// the advanced raw_interp_braces), and the nested-row/diverging-handler/
// entry-shape files bring the rest. Raised in the bump commit per the
// test's own instruction.
// 125 → 127 at 87405ac: the s109 ruling wave's two new clauses gain their
// first citing programs in the same wave — [gram.expr.tagident] (the D52
// quartet) and [gram.type.row.flatten] (the D51 pair plus the advanced
// nested_row_param/return). Raised in the bump commit per the test's own
// instruction.
// HELD at 127 by 21b129e (is21): the s110/s111 wave adds no anchor — the
// wrapping family still has no clause, so its six new files cite nothing new.
// 127 → 133 at da8582d (is22): the s112 constant-time tier's six covered
// clauses gain their first citing programs in the same wave — ct.attr.fn,
// ct.attr.public, ct.taint.prop, ct.taint.declassify (the two kernel
// flagships + public_len), ct.taint.sink (the five ct/ sink witnesses) and
// ct.taint.membrane (membrane.lu). Raised in the bump commit per the test's
// own instruction.
// 133 → 140 at 77466a3 (is23): the s113 D54 literal tier's seven covered
// clauses gain their first citing programs in the same wave — type.numlit.adopt,
// type.numlit.propagate, type.numlit.default, type.numlit.value, type.numlit.ambig
// (the nine numlit_* witnesses) and type.numlit.cast.widen / type.numlit.cast.trunc
// (the #138 cast witnesses). Raised in the bump commit per the test's own
// instruction.
// 140 → 144 at 90c90df (is24): the s114–s116 wave's four newly covered
// clauses — type.numlit.cast.wrap (the D56 trio: wrap_as_int_in_range +
// the two faults/ trap witnesses) and os.signal.listen / os.signal.wait /
// os.signal.raise (cited by the signal pair, which this machine declines
// but the coverage table counts by citation, honestly). Raised in the
// bump commit per the test's own instruction.
// 144 → 153 at a900b8c (is26): the s117–s121 wave's nine newly covered
// clauses — the char family the sprint implements (type.char and its
// cast/order/interp children, cited by char_battery/char_order/char_interp
// and the three faults/char_cast_* twins; mem.str.chars, cited by
// char_battery and chars_walk) and the entropy family this machine
// declines by name (os.random, os.random.checked, os.random.fill,
// os.random.trap — counted by citation, honestly). Raised in the bump
// commit per the test's own instruction.
// 153 → 157 at e561c6f (is27): the s122–s125 wave's four newly covered
// clauses — conf.trap.report (oob_bounds cites the two-line report),
// conf.trap.render (the suite's trap_site.lu, this sprint's own witness),
// and the D59 membership pair conf.directive.standalone /
// conf.directive.member (the resolve/ witnesses). Raised in the bump
// commit per the test's own instruction.
// 157 → 159 at addcd7f (is28): the s126 wave's two newly covered clauses —
// gram.attr.index and gram.expr.index.origin, cited by the eight
// index_origin_* witnesses this sprint implements. Raised in the bump
// commit per the test's own instruction.
// 159 → 163 at c88ab64 (is30): the s127/s128/r03 wave's four newly covered
// clauses — mem.list.slice (the #171 slice quartet: list_slice,
// list_slice_edges, and the oob/reversed fault twins) and the D62 concat
// family type.str.concat / type.str.concat.mix / type.str.concat.cost
// (concat_plus runs; the three mix refusals count by citation, honestly —
// this machine declines them by name at resolve). Raised in the bump
// commit per the test's own instruction.
// 163 → 164 at 83f83bb (is30's second bump, the s129 merge): the one
// newly covered clause is gram.pat.struct itself, cited by all six
// struct-pattern witnesses — the clause this sprint implemented from.
// Raised in the bump commit per the test's own instruction.
// 164 → 165 at b80d239 (is31, the s130 merge): the one newly covered
// clause is ty.match.reachable, cited by
// typecheck/match_arm_product_unreachable — E0802 over a product
// scrutinee, which this machine's reachability walk now judges
// column-wise. Raised in the bump commit per the test's own instruction.
// 165 → 168 at e6cf24e (is32, the s131 merge + the ledger ritual): three
// newly covered clauses, measured against the previous matrix — the two
// this sprint implemented, mem.region.account.1 (both witnesses) and
// mem.region.account.2 (the query witness), plus gram.lex.char, which
// the corpus had never cited until r04's char_uni_seven_digits.lu (the
// char battery cites the type.char family, not the lexical clause).
// Raised in the bump commit per the test's own instruction.
const RATCHET_FLOOR: usize = 168;

/// The registry size at pin `26fa98e` (306 → 315: `mem.str.empty`,
/// `mem.str.repeat`, §10's `gram.version` family ×4 — s71/r01's
/// grammar/1 declaration — and s72's `mem.iter.excl` family ×3). Held at
/// 315 by pin `613c3dc` (0.1.10), whose range touches no `spec/` file:
/// s42/s43 are compiler-internal and s63's diagnostic catalog lives
/// outside the spec tree. 315 → 316 at pin `f8dca42` (0.1.11): s53's
/// `[gram.lex.shebang]`, the ONLY `spec/` delta in the whole wave — `#!`
/// at byte offset zero, and at no other offset, is trivia, so an
/// executable script is an ordinary translation unit. MOVED to 323 by pin
/// `02c1e88`, the first bump in a while whose range does touch `spec/`:
/// s89 added `[mem.str.view.lend]` to 02-memory-model.md — written before
/// the lend analysis it governs — and the rest of the delta is the s90 fs
/// surface. Moves only with a pin bump, and then deliberately.
// 323 → 343 at c9da6d9: taskenv/procenv/dyn (c19/c22/s96), [mem.dyn.unsize]
// (s98), [mem.str.view.lend] retitles ride along, and s51's sixteen pkg.*
// rows — registered while [conf.anchor.ns] still lacks the namespace, the
// filed finding wolf-lang#120 (see export::FILED_REGISTRY_FINDINGS).
// 343 → 344 at b522b8a: [abi.native.closure], the wave's one spec/ delta —
// c25's closure ABI (the two-word pair, the borrowing env).
// HELD at 344 by 1b149ba: the s108 range touches no spec/ file at all —
// twelve corpus files, zero clauses; the nested-row meaning stays an open
// D-question by design.
// 344 → 346 at 87405ac: the s109 ruling wave writes both open D-questions
// into spec/01 — [gram.expr.tagident] (D52's declared-row-first resolution,
// the return-position rule recorded for the first time) and
// [gram.type.row.flatten] (D51's union semantics, E0609 the counter).
// HELD at 346 by 21b129e: the s110/s111 range touches no spec/ file — six
// corpus files, zero clauses. The wrapping family the wave completes still
// has no clause of its own (no arith.* namespace exists); the shift-count
// gap is recorded on wolf-interp#42 as owed upstream.
// 346 → 360 at da8582d (is22): the s112 constant-time tier writes
// spec/09-constant-time.md — fourteen `ct.*` anchors ([ct.attr] and its
// six children, [ct.taint] and its seven). The `ct` namespace is REGISTERED
// here now (the spec exists, every anchor is in anchors.json).
// 360 → 370 at 77466a3 (is23): the s113 D54 literal tier writes
// spec/10-types.md — ten `type.*` anchors ([type.numlit] and its four children
// adopt/propagate/default/value, [type.numlit.kind], [type.numlit.ambig], and
// [type.numlit.cast] with its widen/trunc pair). The `type` namespace is
// REGISTERED here now (the spec exists, every anchor is in anchors.json).
// 370 → 380 at 90c90df (is24): the s114 signal tier writes spec/11-os.md —
// nine `os.*` anchors ([os.signal] and its eight children), the `os`
// namespace REGISTERED here now (the spec exists, every anchor is in
// anchors.json) — and s113's D56 follow-up adds [type.numlit.cast.wrap].
// The `pkg` namespace is REGISTERED too: s115 amended [conf.anchor.ns]
// for #120, so the FILED_REGISTRY_FINDINGS waiver dies with the filing.
// 380 → 393 at a900b8c (is26): the s117–s121 wave writes thirteen anchors —
// s121's char tier ([gram.lex.char], [type.char] and its lit/order/cast/
// interp children), s120's [mem.str.chars] (restated over List[char] by
// s121), and s118's entropy family ([os.random] and its five children).
// 393 → 396 at e561c6f (is27): the s124/s125 pair writes three —
// [conf.directive.standalone] (D59 membership), [conf.trap.report] (the
// native two-line report) and [conf.trap.render] (this sprint's contract
// anchor: the interpreter's line:col rendering).
// 396 → 397 at addcd7f (is28): s126 writes [gram.attr.index] and
// [gram.expr.index.origin] (+2) while its regen DROPPED [gram.lex.ident]
// (−1) — the hole is an upstream finding, wolf-lang#177, carried as a
// FILED_REGISTRY_HOLES notice until the registry re-gains the anchor.
// 397 → 403 at c88ab64 (is30): the s127/s128/r03 wave to v0.2.0 —
// [gram.item.let] grows D63's grouped-binder clause, [mem.list.slice]
// lands (#171), [type.str.concat] and its mix/cost children land (D62),
// and r03's spec-extract scanner fix RE-GAINS [gram.lex.ident]: the
// wolf-lang#177 hole closes, the FILED_REGISTRY_HOLES waiver dies, and
// the cross-check gates the token like any other again.
// 403 → 404 at 83f83bb (is30's second bump, the s129 merge):
// [gram.pat.struct] — struct patterns exist, and this machine's parser
// cites the clause on every StructPat node. Nothing dropped.
// HELD at 404 by b80d239 (is31, the s130 merge): the spec tree is
// BYTE-IDENTICAL to the 83f83bb pin — s130's whole delta is lowering,
// corpus and CHANGELOG, so the registry neither gains nor drops a row.
// 404 → 407 at e6cf24e (is32, the s131 merge): [mem.region.account] and
// its two children land in spec/02 §3 — the byte ledger and the
// process-wide live total. The key SETS were diffed both ways: nothing
// dropped (the wolf-lang#177 lesson). The pin's other spec edits grow no
// anchor — D67's comma sentence tightens [gram.pat]'s existing
// production and r04's UNI_ESC amends [gram.lex.char]'s EBNF in place.
const ANCHORS_TOTAL: usize = 407;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn options(out: &str) -> ExportOptions {
    let root = crate_root();
    let upstream = root.join(wolf_interp::upstream_root());
    ExportOptions {
        corpus: upstream.join("corpus"),
        spec: upstream.join("spec"),
        suites: root.join("tests"),
        repl_doc: root.join("docs/repl.md"),
        pin: root.join("vendor/upstream/PIN"),
        out: root.join("target").join(out),
    }
}

/// One bundle, exported once, shared by every test in this binary.
fn bundle() -> &'static (ExportOptions, ExportSummary) {
    static BUNDLE: OnceLock<(ExportOptions, ExportSummary)> = OnceLock::new();
    BUNDLE.get_or_init(|| {
        let options = options("test-bundle");
        let summary = export::export(&options).expect("the export must succeed");
        (options, summary)
    })
}

/// Every file under a directory, as (relative slash path, bytes), sorted.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .expect("readable")
            .map(|e| e.expect("entry").path())
            .collect();
        entries.sort();
        for entry in entries {
            if entry.is_dir() {
                walk(&entry, root, out);
            } else {
                let relative = entry
                    .strip_prefix(root)
                    .expect("under root")
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((relative, std::fs::read(&entry).expect("readable")));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn re_export_is_byte_identical() {
    let (first_options, first) = bundle();
    let second_options = options("test-bundle-again");
    let second = export::export(&second_options).expect("the re-export must succeed");
    assert_eq!(first.bundle_sha256, second.bundle_sha256);

    // The root hash covers every file except the manifest; compare the whole
    // trees byte-for-byte so the manifest is held to the same standard.
    let a = tree(&first_options.out);
    let b = tree(&second_options.out);
    assert_eq!(a.len(), b.len(), "the two exports differ in file count");
    for ((path_a, bytes_a), (path_b, bytes_b)) in a.iter().zip(&b) {
        assert_eq!(path_a, path_b);
        assert_eq!(bytes_a, bytes_b, "{path_a} differs between two exports");
    }
}

#[test]
fn the_bundle_opens_verified_and_its_records_reference_bundled_programs() {
    let (options, summary) = bundle();
    let verified = export::open_bundle(&options.out).expect("a fresh bundle verifies");
    assert_eq!(verified.pin, summary.pin);
    assert_eq!(verified.bundle_sha256, summary.bundle_sha256);
    assert_eq!(verified.expected.len(), summary.records);

    // Every record names a program that ships in the bundle, by its
    // bundle-relative slash path, in sorted order (two exports of the same
    // trees diff cleanly).
    let mut previous = String::new();
    for record in &verified.expected {
        assert!(
            options.out.join(&record.file).is_file(),
            "{} is recorded but not bundled",
            record.file
        );
        assert!(
            record.file.starts_with("corpus/") || record.file.starts_with("suite/"),
            "{}",
            record.file
        );
        assert!(!record.file.contains('\\'), "{}", record.file);
        assert!(previous < record.file, "records must be sorted by file");
        previous.clone_from(&record.file);
    }
}

#[test]
fn the_pin_and_the_counts_are_the_ones_this_sprint_recorded() {
    // The count ledger, lupin 0.1.12 edition (pin `4e316ad`): 283 corpus
    // files (261 entries + 22 members) + 37 suite programs (7 UB triggers,
    // 8 twins, 9 fault litmuses + 7 twins, 5 witnesses — the T1 pair and
    // P1's protector form retired with issue #18: their `as bool` and `*u8`
    // signatures are outside the language now, see `UbRow::coverage` and
    // `tests/ub/`, plus the issue #20 freeze-read twin) = 320 programs,
    // 298 entries conform-run into reference records. The pin brought three
    // compiler sprints and no spec delta: s79 (bench integrity), s80 (a
    // real miscompile — `region.foreign` roots are role-scoped now) and
    // s81 (str equality without a call, and `str_from_utf8`, the
    // language's first bytes-to-str path) — 3 new corpus files, one per
    // sprint. Two ran clean here at first sight; the third needed the
    // `str_from_utf8` builtin, this pin's only implementation work — see
    // the corpus-harness ledger. A pin bump or a new suite file moves
    // these numbers — move them *deliberately*, here and in the
    // corpus-harness ledger.
    // 340/317 → 364/338 at b522b8a (is17): the c25/s105/front-end wave's
    // 24 corpus files (21 entries) land; the suite programs are unmoved.
    // 364/338 → 376/348 at 1b149ba (is18): the s108 wave's 12 corpus
    // files (10 entries) land; the suite programs are unmoved.
    // 376/348 → 382/354 at 87405ac (is19): the s109 ruling wave's 6 corpus
    // files (all entries) land; the suite programs are unmoved.
    // 382/354 → 388/360 at 21b129e (is21): the s110/s111 wave's 6 corpus
    // files (all entries) land; the suite programs are unmoved.
    // 388/360 → 398/370 at da8582d (is22): the s112 constant-time tier's 10
    // corpus files (all entries) land; the suite programs are unmoved.
    // 398/370 → 411/383 at 77466a3 (is23): the s113 D54 literal tier's 13
    // corpus files (all entries) land; the suite programs are unmoved.
    // 411/383 → 422/393 at 90c90df (is24): the s114–s116 wave's 11 corpus
    // files (10 entries, 1 member) land; the suite programs are unmoved.
    // 422/393 → 440/411 at a900b8c (is26): the s117–s121 wave's 18 corpus
    // files (all entries) land; the suite programs are unmoved.
    // 440/411 → 441/412 (is27): the suite grows one — `trap_site.lu`, the
    // `[conf.trap.render]` cross-machine site witness (6:5 on both
    // machines); the corpus is unmoved.
    // 441/412 → 460/427 at e561c6f (is27): the s122–s125 wave's 19 corpus
    // files land — 15 entries (the D59 resolve witnesses, the s125
    // overflow-on-pop twins, the s123 match/str cluster, the four numlit
    // witnesses) and the 4 bare members `[conf.directive.standalone]`
    // makes legible; the suite is unmoved.
    // 460/427 → 468/435 at addcd7f (is28): the s126 wave's 8 corpus files
    // land, all entries — the D61 index_origin_* witnesses this sprint
    // implements against (six run-reaching, the E0813 and E0211 fail
    // pins); the suite is unmoved.
    // 468/435 → 483/450 at c88ab64 (is30): the s127/s128/r03 wave to
    // v0.2.0 — 15 corpus files, all entries (the D63 let_group quartet,
    // the s128 destructure trio, the #171 slice quartet, the D62 concat
    // quartet); the suite is unmoved.
    // 483/450 → 493/460 at 83f83bb (is30's second bump, the s129 merge):
    // 10 corpus files, all entries — the six [gram.pat.struct] witnesses
    // and #184's slice quartet; the suite is unmoved.
    // 493/460 → 501/468 at b80d239 (is31, the s130 merge): 8 corpus files,
    // all entries — the match ARM witness set (#179's table); the suite is
    // unmoved.
    // 501/468 → 512/479 at e6cf24e (is32, the s131 merge + the ledger
    // ritual): 11 corpus files, all entries — the two [mem.region.account]
    // relation witnesses, D67's three comma refusals, #196's two
    // or-pattern residue pins, r04's seven-digit escape witness, and the
    // region/lint/defer trio; the suite is unmoved.
    let (_, summary) = bundle();
    assert_eq!(summary.pin, "e6cf24e7c03f25c8ac6676fc9cb743827524bc8e");
    assert_eq!(summary.programs, 512);
    assert_eq!(summary.records, 479);
    assert_eq!(summary.anchors_total, ANCHORS_TOTAL);
}

#[test]
fn coverage_is_ratcheted() {
    let (options, summary) = bundle();
    assert_eq!(summary.anchors_total, ANCHORS_TOTAL);
    assert!(
        summary.anchors_covered >= RATCHET_FLOOR,
        "coverage DROPPED: {} covered anchors, the ratchet floor is {RATCHET_FLOOR}. \
         A tagged test or a `conforms:` line was lost — restore it, or renegotiate \
         the floor with the gap list on the table (never silently).",
        summary.anchors_covered
    );
    // The floor tracks the count exactly: growth is recorded by raising it,
    // so the *next* drop is caught at the new water line, not the old one.
    assert_eq!(
        summary.anchors_covered, RATCHET_FLOOR,
        "coverage GREW (good!) — raise RATCHET_FLOOR to {} in the same commit",
        summary.anchors_covered
    );

    // The red path, demonstrated: dropping one covered clause's tests leaves
    // a count below the floor. This is the "red-test PR" of the acceptance
    // criterion, run against the real matrix rather than planted in a mock.
    let matrix = std::fs::read_to_string(options.out.join("coverage/matrix.jsonl"))
        .expect("the matrix ships in the bundle");
    let covered_lines = matrix
        .lines()
        .filter(|line| line.contains("\"status\":\"covered\""))
        .count();
    assert_eq!(covered_lines, summary.anchors_covered, "matrix vs manifest");
    assert!(
        covered_lines - 1 < RATCHET_FLOOR,
        "losing any single covered anchor must fall below the floor"
    );
}

#[test]
fn the_consumption_dry_run_is_green() {
    // The acceptance criterion: "a harness stands in for the compiler
    // (replaying recorded interpreter results) and exercises the full
    // pull → run → diff path". Replaying the bundle's own records must be
    // perfect agreement — anything else means the pipeline, not a program,
    // is broken.
    let (options, summary) = bundle();
    let verified = export::open_bundle(&options.out).expect("verifies");
    let outcome = export::check(
        &verified,
        &CheckImpl::Replay(options.out.join("expected/records.jsonl")),
        Duration::from_millis(30_000),
    )
    .expect("the dry run runs");
    assert_eq!(outcome.compared, summary.records);
    assert_eq!(outcome.divergences, Vec::new());
}

#[test]
fn a_tampered_bundle_is_refused() {
    let tampered = options("test-bundle-tampered");
    export::export(&tampered).expect("exports");
    let hello = tampered.out.join("corpus/hello.lu");
    let mut bytes = std::fs::read(&hello).expect("readable");
    bytes.extend_from_slice(b"\n// tampered\n");
    std::fs::write(&hello, bytes).expect("writable");
    let err = export::open_bundle(&tampered.out).expect_err("a tampered bundle must be refused");
    assert!(err.contains("integrity"), "{err}");
    assert!(err.contains("corpus/hello.lu"), "{err}");

    // A missing file is refused just as loudly as a modified one.
    let truncated = options("test-bundle-truncated");
    export::export(&truncated).expect("exports");
    std::fs::remove_file(truncated.out.join("vocab/traps.json")).expect("removable");
    let err = export::open_bundle(&truncated.out).expect_err("a truncated bundle must be refused");
    assert!(err.contains("vocab/traps.json"), "{err}");
}

#[test]
fn the_vocabularies_ship_closed() {
    let (options, _) = bundle();
    let traps: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(options.out.join("vocab/traps.json")).expect("ships"),
    )
    .expect("json");
    let kinds = traps["kinds"].as_array().expect("kinds");
    assert_eq!(kinds.len(), 12, "[conf.trap.set] is the closed twelve");
    assert_eq!(kinds[0], "overflow");
    assert_eq!(kinds[11], "deadlock");
    assert_eq!(traps["closed"], true);

    let rows: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(options.out.join("vocab/ub-rows.json")).expect("ships"),
    )
    .expect("json");
    let rows = rows["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 11, "[mem.ub] is the closed eleven");
    for row in rows {
        // D2 on the wire: every row names what it licenses, and its coverage
        // status is stated rather than implied ([proto.record.unsupported]'s
        // honesty contract, applied to the matrix).
        assert!(!row["licenses"].as_str().expect("licenses").is_empty());
        assert!(!row["coverage"].as_str().expect("coverage").is_empty());
    }
}

#[test]
fn the_anchor_registry_cross_check_holds_at_this_pin() {
    // Target 1 of the sprint: the pinned `anchors.json` is shared *data* —
    // consumed, and cross-checked against an independent extraction of the
    // spec markdown. A mismatch is an upstream finding; anything unfiled
    // refuses the export (`export::cross_check_registry`, red-tested in
    // the library). The pkg disagreement that stood here from c9da6d9 to
    // 77466a3 (wolf-lang#120) was fixed upstream by s115.
    let spec = crate_root().join(wolf_interp::upstream_root()).join("spec");
    let text = std::fs::read_to_string(spec.join("anchors.json")).expect("readable");
    let value: serde_json::Value = serde_json::from_str(&text).expect("json");
    let registry: std::collections::BTreeMap<String, String> = value["anchors"]
        .as_object()
        .expect("anchors")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
        .collect();
    assert_eq!(registry.len(), ANCHORS_TOTAL);
    let notices =
        export::cross_check_registry(&spec, &registry).expect("nothing unfiled disagrees");
    // Zero notices at c88ab64: the one FILED hole that stood from addcd7f —
    // c1f54f2's anchors regen dropped `gram.lex.ident` while spec/01 §1.3
    // still defined it (wolf-lang#177) — died at this pin, exactly as the
    // waiver contract promised: upstream r03 fixed the spec-extract scanner,
    // the v0.2.0 registry re-gained the anchor, the FILED_REGISTRY_HOLES row
    // came out, and the cross-check gates the token like any other again.
    assert_eq!(notices, Vec::<String>::new());
}

#[test]
fn the_coverage_table_is_the_honesty_document() {
    let (options, summary) = bundle();
    let rendered = std::fs::read_to_string(options.out.join("coverage/coverage.md"))
        .expect("the table ships IN the bundle");
    // The headline numbers are in the prose, so a human reading the artifact
    // sees the same figures the manifest carries.
    assert!(rendered.contains(&format!(
        "{} / {} registered anchors",
        summary.anchors_covered, summary.anchors_total
    )));
    // The UB enumeration's dedicated section (D2): all eleven rows, each
    // detected-and-paired or carrying its named reason.
    assert!(rendered.contains("The UB enumeration"));
    for id in [
        "P1", "P2", "P3", "P4", "P5", "P6", "L1", "L2", "T1", "T2", "C1",
    ] {
        assert!(rendered.contains(&format!("| {id} |")), "{id} missing");
    }
    assert!(rendered.contains("deferred(concurrency)"));
    assert!(rendered.contains("unreachable at this tier"));
    // The debt list is printed in full — part of the product, not a private
    // shame file.
    assert!(rendered.contains("Debt list"));
}

#[test]
fn the_cli_speaks_the_bundle_end_to_end() {
    // The process contract: export prints the summary and exits 0; check in
    // replay mode is the dry run and exits 0; check of a tampered bundle is
    // a *tool* error (exit 2) — corrupted expectations are never verdicts.
    let root = crate_root();
    let out = root.join("target/test-bundle-cli");
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_lupin"))
            .args(args)
            .current_dir(&root)
            .output()
            .expect("the binary runs")
    };
    let exported = run(&[
        "conformance",
        "export",
        "--out",
        out.to_str().expect("utf-8"),
        "--json",
    ]);
    assert!(exported.status.success());
    let summary: serde_json::Value =
        serde_json::from_slice(&exported.stdout).expect("the summary is JSON");
    assert_eq!(summary["bundle_sha256"], bundle().1.bundle_sha256);

    let records = out.join("expected/records.jsonl");
    let checked = run(&[
        "conformance",
        "check",
        out.to_str().expect("utf-8"),
        "--replay",
        records.to_str().expect("utf-8"),
    ]);
    assert!(checked.status.success(), "{checked:?}");
    let stdout = String::from_utf8_lossy(&checked.stdout);
    assert!(stdout.contains("divergences: 0"), "{stdout}");

    // Exactly one counterparty, or the tool refuses.
    let neither = run(&["conformance", "check", out.to_str().expect("utf-8")]);
    assert_eq!(neither.status.code(), Some(2));
}

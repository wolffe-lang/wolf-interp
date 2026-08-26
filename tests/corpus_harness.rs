//! The corpus harness over the pinned corpus.
//!
//! Green here means: every `//!` header in `upstream/corpus` is legible to
//! this implementation's own directive parser. It is a weak claim about wolf
//! and a strong claim about the two tracks agreeing on the directive grammar —
//! which is the only claim is00 is entitled to make.

use std::path::{Path, PathBuf};

use wolf_interp::anchor;
use wolf_interp::corpus::{self, CorpusReport, Outcome};

fn upstream() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(wolf_interp::upstream_root())
}

fn report() -> CorpusReport {
    let root = upstream().join("corpus");
    corpus::walk(&root, Some(&upstream().join("spec"))).unwrap_or_else(|e| {
        panic!(
            "could not walk {}: {e}\n\
             hint: the corpus is a pinned submodule — run `git submodule update --init upstream`",
            root.display()
        )
    })
}

#[test]
fn every_header_in_the_pinned_corpus_parses() {
    let report = report();
    let failures = report.failures();
    assert!(
        failures.is_empty(),
        "unparseable corpus headers:\n{}",
        failures
            .iter()
            .map(|(path, reason)| format!("  {path}: {reason}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_walk_is_green() {
    assert!(report().is_green());
}

#[test]
fn the_pin_holds_the_corpus_we_think_it_does() {
    // A pin bump that changes the corpus size is a deliberate act; this test
    // makes it deliberate rather than silent (s01's progress-ledger rule).
    //
    // Ledger of deliberate bumps:
    //   4a002aa → 74 files (is01: typecheck tier added 5)
    //   28ab5c9 → 91 files (is02: s14's `corpus/traits/` added 17 — 12 entries
    //             and 5 members across three module directories; no other tier
    //             moved, and no existing file changed).
    //   bd41920 → 103 files (is03: s15's `corpus/rows/` added 12 — 9 entries
    //             and 3 members, the members being `rows/propagate/`'s three
    //             module files. One existing file changed content without
    //             changing the count: `grammar/bang_errunion.lu`. s16's
    //             `corpus/comptime/` and `corpus/faults/` tiers had NOT landed
    //             at this pin, so no dedupe against `tests/faults/` was owed —
    //             see that directory's README.)
    //   ecea37c → 128 files (is04: s16's `corpus/comptime/` added 19 entries
    //             and `corpus/faults/` added 6 — the six fault-class programs
    //             this repo authored at is03 and upstreamed. Per
    //             `tests/faults/README.md`, the vendored copies are now the
    //             source of truth and the local twins of those six are gone;
    //             the local directory keeps only the region/handle programs the
    //             corpus still has no counterpart for. Two existing files
    //             changed content without changing the count: `regions.lu`
    //             gained `build_config` and `comptime.lu` was rewritten. All
    //             new files are entries, so the member count is unmoved.)
    //   8b04edf → 149 files (is05: s17's sema completion. `corpus/typecheck/`
    //             grew 16 (14 entries plus `typecheck/method_scope/`'s two
    //             members alongside its entry `main.lu`), `corpus/grammar/`
    //             added `receiver_moded.lu` + `intdot_exponent.lu`,
    //             `corpus/memory/` added `view_set_norm.lu`, and
    //             `corpus/rows/` added `open_row_growth.lu` +
    //             `negative/open_into_closed.lu`. One existing file changed
    //             content without changing the count: `regions.lu`'s tail now
    //             uses only specified semantics — `config.limit == 42` under
    //             `[mem.region.freeze.1]`'s "frozen data readable forever" —
    //             which is the change that finally made it RUN here.)
    //   79ceec6 → 158 files (is07: the is06 debts paid — `conc/
    //             freeze_publish.lu` reports through a channel and
    //             `conc/when_multi.lu` expects 223, closing DIV-2026-008/009;
    //             the same pin's compiler fixes E0210's span, closing
    //             DIV-2026-007. `corpus/memory/` grew 9: the three
    //             `region_infer_*` annotation-freedom demonstrations, the
    //             E1004/E1010 litmuses `region_conflict_params.lu` +
    //             `region_escape_local.lu`, plus `exclusivity.lu`,
    //             `mode_missing_mut.lu`, `mut_arg_temporary.lu` and
    //             `view_set_violation.lu`. 29 existing files changed only
    //             their `phase:` directive (typecheck → mem — the compiler's
    //             region-inference rung exists now), which moves nothing
    //             here.)
    //   843174f → 164 files (is08: the s20 S-batch pin. `corpus/memory/`
    //             grew 6 — the region-checker tier: `region_freeze_ok.lu`,
    //             `region_freeze_open.lu`, `region_freeze_write.lu`,
    //             `region_multiopen_values_ok.lu`, `region_open_ancestor.lu`,
    //             `region_transfer_open.lu`. Four existing files changed
    //             content without changing the count: `procs.lu` and
    //             `conc/proc_kill_defers.lu` are self-contained now (S-5
    //             resolved — they RUN), and `conc/message_passing.lu` +
    //             `conc/when_multi.lu` cite the new `[conc.chan.move]` /
    //             `[conc.when.*]` clauses in their `conforms:` lines.)
    //   cbde620 → 164 files (is09: s21's shared tier. No file added or
    //             removed — ten existing files changed only their `phase:`
    //             directive: nine advance to `mem` (`regions.lu`,
    //             `memory/{handle_stale,move_ok,oob_bounds,prov_two_phase,
    //             region_ambient_ok,region_multiopen_ok,region_multiopen_swap,
    //             shared_ok}.lu`) and `memory/prov_holy_grail.lu` to
    //             `typecheck`. The same pin renders the §3.2 operator climb
    //             into `spec/grammar.ebnf` — checked against our is01
    //             transcription in `tests/spec_extract.rs`.)
    //   a0c4564 → 175 files (0.1.2: the pin taken for issue #8's fail-files.
    //             `corpus/typecheck/` grew 3 — `let_reassign.lu` +
    //             `let_compound_assign.lu` pin E0410 and
    //             `let_shadow_var_ok.lu` is the ok-twin (wolf-lang#2's
    //             missing pin). `corpus/memory/` grew 8 — the unsafe/checked
    //             tier: `checked_unsigned.lu`, `unsafe_assume_malformed.lu`,
    //             `unsafe_creation_not_use.lu`, `unsafe_door_borrow.lu`,
    //             `unsafe_door_misuse.lu`, `unsafe_raw_outside.lu`,
    //             `unsafe_sig.lu`, `unsafe_trusted.lu` — plus a `wolf.pkg`
    //             manifest, which is not a source file and moves nothing
    //             here. A handful of existing files changed only their
    //             `phase:` directive as the compiler's wir rung landed.)
    //   d147a54 → 177 files (0.1.3: the s27+s28 pin — the spec grows
    //             `[mem.iter.*]`, `[mem.str.*]`, `[conf.trap.assert]` and the
    //             postfix-row type grammar (290 anchors). The corpus grows 2:
    //             `rows/qmark_defer.lu` (the s27 control witness — `?`, defer
    //             chains, or-patterns under guards) and
    //             `faults/assert_msg_holds.lu` (the #19 regression witness —
    //             a two-arg assert whose message is not a second condition).
    //             Many existing files advance their `phase:` to `wir` as the
    //             compiler's backend landed.)
    //   ad6cef7 → 183 files (0.1.4: the s29+s30 pin. The corpus grows 6:
    //             `memory/unsafe_c_alloc_native.lu` (the C-set native litmus —
    //             the c.calloc(n,size) witness, issue #13),
    //             `rows/eu_main_err_exit.lu` (the erring-`!int`-main pin:
    //             `error: <tag>` + exit 1), `typecheck/float_nan_cmp.lu`
    //             (IEEE ordered/unordered comparison — `nan != nan` is TRUE;
    //             wolf-lang#22), and `resolve/same_name/` — an entry plus two
    //             members whose `len`s share a name and a signature
    //             (wolf-lang#26). The two E0410 fail-files re-pin their
    //             `phase:` resolve → parse as s29 moves the emission to the
    //             resolve rung — DIV-2026-010 closes.)
    //   f0da6e6 → 199 files (0.1.5: the five-lane fan-out pin — s32 tasks,
    //             s33 channels, s37 str core, s38 fmt/io/fs, s67 warnings.
    //             The corpus grows 16, all entries: `strings/` ×9 (the s37
    //             builtin surface + `[mem.str.get]`, E0411, and the §7.4
    //             format-spec suite — E0412/E0413 and three rendering pins),
    //             `lints/` ×4 (the s67 `#[allow]` granularity files and the
    //             W1301 safety-comment lint, carrying the new `warns:`
    //             directive), `fs/` ×2 and `io/` ×1 (the s38 fs/io tier —
    //             fs is unsupported-by-design here; `eprint` runs). The
    //             spec grows the `diag` namespace (303 anchors),
    //             `[mem.str.get]`, `[proto.record.warn]`/`[proto.cmp.warn]`,
    //             and the directive-matcher trailing-newline allowance in
    //             `[conf.directive.check]`.)
    //   13b811f → 221 files (0.1.6: the wave-four pin — #41 capture law,
    //             s34 procs, s35 io reactor, s39/s40/#40 native str/List/fs.
    //             The corpus grows 22: `lints/` ×12 (the s68 shared-analysis
    //             wave, every file a `warns:` ledger), `conc/` ×3
    //             (`when_nested.lu` E1103 and the s34 proc pair),
    //             `projects/` ×3 (count, rpn, wordtree — the P-project
    //             witnesses), `net/` ×2 (s35; sockets are outside this
    //             machine by design), `comptime/sandbox_net_socket.lu` and
    //             `test/assert_test.lu` (the s39 `test` namespace). The
    //             spec grows `[mem.region.freeze.4]`, `[conc.chan.default]`
    //             and the s34/s35 schedule points.)
    //   e94b879 → 254 files (0.1.7: the wave-six pin — s40 os/time/json,
    //             s70 match tier + X3 value paths, s69 idiom lints. The
    //             corpus grows 33: `lints/` ×15 (the s69 idiom-arbiter wave
    //             — W0310–W0316, W0603/W0604, W1002/W1003 — plus the
    //             literal-precise E0802 str-match file), `os/` ×4 and
    //             `time/` ×1 and `json/` ×2 (the s40 tier), `faults/` ×5
    //             (the X3 value-path overflow litmuses — issue #21's
    //             witnesses), `memory/list_elem_assign.lu` (#55),
    //             `strings/match_str_dispatch.lu` +
    //             `typecheck/match_str_nonexhaustive.lu` (#54),
    //             `rows/handler_match_tags.lu` (#48),
    //             `traits/coherence_orphan/` ×3 and
    //             `typecheck/method_scope/` ×2 members(+entry counted),
    //             `comptime/sandbox_exec.lu` (the s40 exec category). The
    //             spec grows `[proto.cmp.rung]` — 306 anchors.)
    //   26fa98e → 262 files (0.1.8: the wave-seven + s72 pin, the v0.1.0
    //             pairing — one lawful mid-pass re-pin when s72 merged.
    //             s71 grows 5: `strings/empty_needle.lu` +
    //             `faults/repeat_negative.lu` (the `[mem.str.empty]`/
    //             `[mem.str.repeat]` rulings, #56/#57),
    //             `rows/else_tag_payload.lu` +
    //             `rows/negative/handler_uncovered.lu` (the `else |Tag(p)|`
    //             row-coverage rule E0809, #43/#59), and
    //             `comptime/fold_reaches_lane.lu` (the ctfe fold table —
    //             the compiler's engine; this machine still declines
    //             comptime by name). s72 grows 3 — the D39/D40 mode-teeth
    //             fail-files `memory/read_param_write.lu` (E1014),
    //             `memory/mut_read_overlap.lu` (E1002) and
    //             `memory/list_mutate_while_iter.lu` (E1013), each this
    //             machine's trap(exclusivity) dynamic counterpart. The
    //             spec grows `[mem.str.empty]`, `[mem.str.repeat]`,
    //             §10 `[gram.version]` (grammar/1) and `[mem.iter.excl]`.)
    //   0b4e79c → 263 files (0.1.9: the c09-wave pin — s41 release tier,
    //             s51 package manager, s73 NATIVE CONCURRENCY. The corpus
    //             grows 1: `test/conc_schedules_test.lu` (the s73
    //             `--schedules=N` dogfood witness). Nine files' headers
    //             advance to phase `run` (8× `conc/` + `procs.lu` — the
    //             compiler executes conc natively now; this machine's
    //             claims were already `run`, so both machines execute the
    //             tier), `memory/prov_holy_grail.lu` moves typecheck → mem,
    //             and `conc/proc_cancel_defers.lu`/`conc/proc_link.lu`
    //             gain `-> !int` rows (`?` needs the row, E0604/D30). The
    //             spec's only delta: `[conf.trap.map]`'s exclusivity row
    //             now names E1014's read-mode write barrier — 0.1.8's
    //             filed nit, closed upstream. Anchors stay 315.)
    //   613c3dc → 267 files (0.1.10: the mid-end/whole-program pin — s42
    //             (the optimizer), s43 (clusters, body dedup, the frozen
    //             summary index) and s63 (diagnostics polish: 144 codes /
    //             30 warnings, the E-cascade cap and `--error-limit=N`).
    //             The corpus grows 4, all entries, all `phase: run` with
    //             `check: run(exit=0)`: `conc/select_two_timeouts.lu` (the
    //             #64 GVN cross-arm-dominance regression litmus — two
    //             timeout selects in one body) and the s42 kernel tier
    //             `kernels/hot_counter.lu`, `kernels/hot_scale_versioned.lu`
    //             and `kernels/churn_b3.lu` (the checked-arith elimination
    //             and region-promotion shapes the mid-end's gates read).
    //             All four run clean on this machine at first sight, so the
    //             run ledger grows 4 and coverage ratchets 102 → 103:
    //             `[conc.select.timeout]` is covered for the first time,
    //             cited by the new select litmus alone. The spec is
    //             UNCHANGED in this range — s42/s43 are compiler-internal
    //             and s63's catalog lives outside `spec/` — so anchors stay
    //             315 and no new clause needs a reading.)
    //   f8dca42 → 280 files (0.1.11: the semantics wave — s74 (the
    //             correctness cluster), s75 (List element access lowers to
    //             a load), s76 (containers allocate in the AMBIENT region,
    //             dynamically scoped per D12), s77 (`s.bytes()` is a view
    //             over the receiver's own storage) and s78 (an affine
    //             relational channel in the range analysis), plus s53's
    //             script mode. The corpus grows 13. Eleven run clean on
    //             this machine at first sight — including all four of the
    //             wave's semantic witnesses (`memory/region_container_
    //             reclaim.lu`, `memory/region_container_freeze_ok.lu`,
    //             `strings/byte_view.lu`, `strings/slice_boundary_sweep.lu`),
    //             which is the differential's real finding: the compiler
    //             moved its lowering and this machine's independent reading
    //             already agreed. Two land as static conservatism
    //             (`memory/region_escape_container.lu`'s E1010, a compile-
    //             time region judgement this machine does not make; and
    //             `conc/capture_mut_arg.lu`/`capture_mut_lend.lu` became
    //             MATCHES this round when the capture law grew its lend
    //             spellings — see `lint::Walk::capture_lend`). Coverage
    //             ratchets 103 → 107 and anchors 315 → 316: s53's
    //             `[gram.lex.shebang]` is the wave's only spec delta, and
    //             it needed a reading here — a `#!` line at byte offset
    //             zero is trivia to the lexer AND to the directive header
    //             parser, which is why `grammar/shebang.lu` arriving broke
    //             all 14 sibling files in `grammar/` until it was read.)
    //   4e316ad → 283 files (0.1.12: three compiler sprints, one spec-silent
    //             wave. s79 (bench integrity — the release runtime is now
    //             actually linked into what the benchmarks measure), s80 (a
    //             REAL MISCOMPILE fixed: `region.foreign` roots are
    //             role-scoped, and the release tier had been folding a
    //             program whose answer is `x=5 y=7` down to `x=5 y=5`), and
    //             s81 (str equality lowers without a call; `str_from_utf8`
    //             is the language's first bytes-to-str path, and it
    //             validates). The corpus grows 3, one witness per sprint.
    //             `memory/foreign_root_aliasing.lu` and
    //             `strings/equality_lanes.lu` ran clean here at first sight
    //             — the miscompile witness in particular, which is the
    //             differential's finding in miniature: this machine never
    //             had the bug there was to fix, on any tier.
    //             `strings/from_utf8_border.lu` needed the builtin (see
    //             `builtin::call`'s `str_from_utf8` arm), which is the only
    //             implementation work this pin asked for. `spec/` is
    //             UNCHANGED in the range — `str_from_utf8` has NO clause,
    //             and is specified only by its prelude signature, its doc
    //             comments and that witness — so anchors stay 316 and the
    //             ratchet stays 107: the three new files cite `str.views`,
    //             `mem.str.get` and `mem.region.intra`, all already covered.)
    //   c9da6d9 → 303 files (is14: the compiler's biggest fortnight —
    //             c19 native conc closes (loop-spawned procs own a COPY
    //             of their argument record), c21 monomorphization, c22
    //             dispatch (static, `call.ind`, the dyn pair), s98's D47
    //             (`v as dyn Trait`, places only), c23 range facts. The
    //             corpus grows 9: two conc loop witnesses, six generics
    //             witnesses, and `traits/dyn_temp_refused.lu`. EIGHT of
    //             nine run clean here at first sight — every generics and
    //             conc witness byte-matches, which is the differential's
    //             finding again: the compiler built monomorphization and
    //             this machine's tree-walking reading already agreed on
    //             every answer. The ninth is the D47 fail-pin (E0810, a
    //             place judgement this machine does not make): static
    //             conservatism, the designed class, and wolf-interp#31's
    //             disposition in one ledger row.)
    //   b522b8a → 327 files (is17: the c25/s105/front-end wave. The
    //             corpus grows 24 — 21 entries and 3 members: the closure
    //             build's seven-file family (six legal shapes plus
    //             `closure_borrow_write.lu`, wolf-interp#36's witness), the
    //             region-value tier (`region_value_return.lu` is #35's
    //             witness; pass/container/elem beside it), explicit generic
    //             application (`explicit_apply.lu` + its E0812 arity
    //             fail-pin, #34's first shape), the prim-impl pair
    //             (`prim_impl.lu` and the orphan module, #34's survivor),
    //             the variant-value and fn-value-import module witnesses,
    //             two s105 kernels, and two net files for the tier this
    //             machine declines whole. Existing files changed content
    //             without changing the count, two of them check-visible:
    //             `brackets_generic_call.lu` pass → run(exit=0) (#111
    //             landed explicit application) and `prov_holy_grail.lu`
    //             phase mem → run (the capture-free callback lambda-lifts
    //             now).)
    //   1b149ba → 339 files (is18: the s108 front-end wave. The corpus
    //             grows 12 — 10 entries and 2 members: the nested-fn
    //             twins (wolf-interp#38's witness pair), the leaf_twins
    //             module-identity witness with its two same-leaf members
    //             (wolf-interp#39), the nested-row pair (#34 — parses
    //             upstream now, sema refuses the meaning by name), the
    //             diverging-handler pair (#35 narrowed; the trap twin is
    //             the corpus's first `stdout=` pin on a trap verdict),
    //             `raw_fences.lu` (#76 — the fence-width decode family),
    //             and the E0414 entry-shape pair. One existing file
    //             changed check-visibly: `raw_interp_braces.lu` advanced
    //             `phase: wir` → `run` (the shared raw-decode debt paid;
    //             lupin's side was fixed at v0.1.10).)
    //   87405ac → 345 files (is19: the s109 ruling wave — D51 and D52 land
    //             upstream. The corpus grows 6, all entries, all `rows/`:
    //             the D51 pair `nested_row_merge_payload.lu` (same tag, same
    //             payload, both layers — one tag, runs) and
    //             `negative/nested_row_conflict.lu` (same tag, DIFFERENT
    //             payloads — E0609, the priced cost), and the D52 quartet
    //             `tag_arg_position.lu`, `tag_let_position.lu`,
    //             `tag_shadow_local.lu` (warns: W0305 at the use) and
    //             `negative/tag_undeclared_arg.lu` (the E0301 counter). Two
    //             existing files changed check-visibly without moving the
    //             count: `nested_row_param.lu` and `nested_row_return.lu`
    //             advance `phase: resolve` → `run` — the ruled flattening is
    //             what this machine always executed, so the pins now gate
    //             both machines. The spec grows `[gram.expr.tagident]` and
    //             `[gram.type.row.flatten]` — 346 anchors.)
    let report = report();
    assert_eq!(
        report.total(),
        345,
        "corpus size changed — was the pin bumped?"
    );
    assert_eq!(report.entries() + report.members(), report.total());
    assert_eq!(report.entries(), 317);
    assert_eq!(report.members(), 28);
}

#[test]
fn entries_carry_check_and_phase_and_members_carry_neither() {
    for file in &report().files {
        match &file.outcome {
            Outcome::Entry(directives) => {
                assert!(directives.check.is_some(), "{} has no check:", file.path);
                assert!(directives.phase.is_some(), "{} has no phase:", file.path);
                assert!(directives.is_entry());
            }
            Outcome::Member(directives) => {
                assert!(
                    directives.check.is_none(),
                    "{} is a member with check:",
                    file.path
                );
                assert!(
                    directives.phase.is_none(),
                    "{} is a member with phase:",
                    file.path
                );
                assert!(!directives.is_entry());
            }
            Outcome::Failed(reason) => panic!("{}: {reason}", file.path),
        }
    }
}

#[test]
fn member_files_live_beside_an_entry_file() {
    // `member: true` means "compiled through this directory's entry file"
    // (directory = module). A member with no entry anywhere up its package
    // would be unreachable — nothing would ever exercise it.
    let report = report();
    let entry_dirs: Vec<&str> = report
        .files
        .iter()
        .filter(|f| matches!(f.outcome, Outcome::Entry(_)))
        .map(|f| f.path.rsplit_once('/').map_or("", |(dir, _)| dir))
        .collect();

    for file in &report.files {
        if !matches!(file.outcome, Outcome::Member(_)) {
            continue;
        }
        let dir = file.path.rsplit_once('/').map_or("", |(dir, _)| dir);
        let reachable = entry_dirs
            .iter()
            .any(|entry_dir| dir == *entry_dir || dir.starts_with(&format!("{entry_dir}/")));
        assert!(
            reachable,
            "{} is a member with no entry file in or above its directory",
            file.path
        );
    }
}

#[test]
fn every_conforms_tag_is_a_well_formed_anchor() {
    for file in &report().files {
        let Some(directives) = file.directives() else {
            continue;
        };
        for tag in &directives.conforms {
            anchor::classify(tag).unwrap_or_else(|e| panic!("{}: {e}", file.path));
        }
    }
}

#[test]
fn registered_namespace_tags_resolve_against_the_pinned_anchors_json() {
    let report = report();
    assert!(
        report.anchors_checked,
        "upstream/spec/anchors.json was not read — is the submodule sparse-checkout missing spec/?"
    );
    assert!(
        report.unknown_anchors.is_empty(),
        "corpus cites anchors absent from spec/anchors.json: {:?}",
        report.unknown_anchors
    );
}

#[test]
fn the_walk_is_deterministic_and_sorted() {
    // `read_dir` order is platform noise; the harness must not inherit it.
    let first = report();
    let paths: Vec<&str> = first.files.iter().map(|f| f.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted);

    let again: Vec<String> = report().files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths, again);
}

#[test]
fn paths_never_leak_platform_separators() {
    for file in &report().files {
        assert!(
            !file.path.contains('\\'),
            "{} carries a raw separator",
            file.path
        );
    }
}

/// The vendored snapshot must be byte-identical to the submodule at its
/// pin — verified whenever the submodule is initialized (locally always;
/// CI has no submodule and skips).
#[test]
fn vendor_matches_submodule() {
    use std::path::Path;
    if !Path::new("upstream/corpus").is_dir() {
        eprintln!("submodule absent; vendored snapshot not cross-checked here");
        return;
    }
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    for tree in ["spec", "corpus"] {
        let mut live = Vec::new();
        walk(Path::new("upstream").join(tree).as_path(), &mut live);
        for lp in live {
            let rel = lp.strip_prefix("upstream").unwrap();
            let vp = Path::new("vendor/upstream").join(rel);
            let a = std::fs::read(&lp).unwrap();
            let b = std::fs::read(&vp)
                .unwrap_or_else(|_| panic!("vendored file missing: {}", vp.display()));
            assert_eq!(a, b, "vendor drift: {}", rel.display());
        }
    }
}

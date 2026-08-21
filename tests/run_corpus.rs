//! The `run` rung against the pinned corpus — is02's acceptance test.
//!
//! Nothing here hardcodes an outcome except the **first-run ledger**, which is
//! deliberately a written list: the sprint asks for "the exact set of files that
//! reach `run` with their outcomes", and a set that is only ever computed cannot
//! regress visibly. Everything else is driven by the corpus's own directives.
//!
//! The load-bearing assertion is `no_corpus_file_mismatches_its_expectation`:
//! `[proto.cmp.triage]` makes a disagreement a *finding*, triaged by a human
//! with the spec as defendant — never silently absorbed by moving a number here
//! or, worse, by editing the corpus.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wolf_interp::corpus::{self, Outcome};
use wolf_interp::directive::{Check, Directives, ExitSpec};
use wolf_interp::ledger::{self, Judgement};
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
}

struct Entry {
    path: String,
    directives: Directives,
    verdict: Verdict,
    phase: Phase,
    stdout: String,
    judgement: Judgement,
    /// The record's `warnings` array (`[proto.record.warn]`): `None` when
    /// the analyses did not run (the program never loaded).
    warnings: Option<Vec<wolf_interp::protocol::Warning>>,
}

fn entries() -> Vec<Entry> {
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("the pinned corpus is walkable");
    report
        .files
        .into_iter()
        .filter_map(|file| {
            let Outcome::Entry(directives) = file.outcome else {
                return None;
            };
            let full = root.join(&file.path);
            let source = std::fs::read(&full).expect("readable");
            let (record, _) = wolf_interp::observe_record(&full, &source, None);
            let stdout = record.stdout_inline.clone().unwrap_or_default();
            let judgement = ledger::judge(
                directives.check.as_ref().expect("an entry pins a check"),
                &record,
                &stdout,
            );
            Some(Entry {
                path: file.path,
                directives: *directives,
                verdict: record.verdict,
                phase: record.phase_reached,
                stdout,
                judgement,
                warnings: record.warnings,
            })
        })
        .collect()
}

/// The files this implementation evaluates end to end, with what they produce.
///
/// **This list is the sprint's run ledger.** Adding to it is progress and
/// belongs in a commit message; losing an entry is a regression. A file leaves
/// this list only when the corpus drops it.
const RUN_LEDGER: &[(&str, &str)] = &[
    // The seed corpus.
    ("hello.lu", "exit(0)"),
    ("overflow.lu", "trap(overflow)"),
    ("wordcount.lu", "exit(2)"),
    // Tier-0 memory litmuses.
    ("memory/defer_order.lu", "exit(0)"),
    ("memory/div_zero.lu", "trap(div-zero)"),
    ("memory/excl_disjoint_ok.lu", "exit(0)"),
    ("memory/excl_overlap.lu", "trap(exclusivity)"),
    ("memory/move_ok.lu", "exit(0)"),
    ("memory/move_use_after.lu", "trap(use-after-move)"),
    ("memory/oob_bounds.lu", "trap(bounds)"),
    ("memory/prov_holy_grail.lu", "exit(0)"),
    ("memory/prov_two_phase.lu", "exit(0)"),
    ("memory/shared_cycle.lu", "exit(0)"),
    // Tier-1/2 litmuses, newly reaching `run` at is03 (the dynamic region
    // machine). Every one matches its `check:` exactly.
    ("memory/handle_stale.lu", "trap(stale-handle)"),
    ("memory/region_ambient_ok.lu", "exit(0)"),
    ("memory/region_iso_edge_ok.lu", "exit(0)"),
    ("memory/region_multiopen_ok.lu", "exit(0)"),
    ("memory/region_multiopen_swap.lu", "exit(0)"),
    ("memory/shared_ok.lu", "exit(0)"),
    // The `rows/` tier, newly present at pin bd41920. Nothing in the evaluator
    // moved for these: D30's error rows are values, and is02's machine already
    // ran them — they arrived with the pin, not with the sprint.
    ("rows/coarsen.lu", "exit(0)"),
    ("rows/hof_tail.lu", "exit(0)"),
    ("rows/inferred_private.lu", "exit(0)"),
    ("rows/negative/dup_tags.lu", "exit(7)"),
    ("rows/negative/errdefer_infallible.lu", "exit(1)"),
    ("rows/negative/missing_tag.lu", "exit(0)"),
    ("rows/negative/payload_mismatch.lu", "exit(0)"),
    ("rows/negative/pub_inferred.lu", "exit(1)"),
    ("rows/propagate/main.lu", "exit(0)"),
    // is13's witness: three arms over an IMPORTED module's row, in both
    // orders. lupin used to answer the FIRST arm for every tag here,
    // silently and exit 0, because it asked the matching module's own
    // fn headers what counted as a tag. Nothing else in the corpus
    // takes a cross-module handler with named arms — rows/propagate
    // above uses a wildcard binder — which is why it went unseen.
    ("rows/cross_module_arms/main.lu", "exit(0)"),
    // The s88/s89/s90 wave, reaching `run` as the pin advanced.
    //
    // Two of s88's: a `main` with no return type, and equality on two
    // bools. lupin ran both before they were corpus files; they are
    // here because wolfgang can now compile them too.
    ("entry_no_return.lu", "exit(0)"),
    ("typecheck/bool_cmp.lu", "exit(0)"),
    ("strings/split_clause.lu", "exit(0)"),
    ("kernels/hot_sum_reduce.lu", "exit(0)"),
    // s89's escape witness, and it is NOT a clean pass: wolfgang
    // REFUSES this file at `mem` with E1015 (a byte view outliving the
    // call it was lent to), and lupin runs it to exit(0). Unlike the
    // litmuses above — move_use_after traps use-after-move,
    // excl_overlap traps exclusivity — E1015 has no dynamic
    // counterpart here, so nothing catches it at either end. Same class
    // as wolf-interp#25 and tracked with it; recorded as exit(0)
    // because that is the truth, not because it is right.
    ("memory/byte_view_escape.lu", "exit(0)"),
    // The grammar annex's accepted readings, now executed rather than parsed.
    ("grammar/bang_errunion.lu", "exit(0)"),
    ("grammar/bang_not.lu", "exit(0)"),
    ("grammar/brackets_generic_call.lu", "exit(0)"),
    ("grammar/brackets_index.lu", "exit(0)"),
    ("grammar/else_chain.lu", "exit(0)"),
    ("grammar/else_default.lu", "exit(0)"),
    ("grammar/intdot_range.lu", "exit(0)"),
    ("grammar/interp_fmtcolon.lu", "exit(0)"),
    ("grammar/interp_nested.lu", "exit(0)"),
    ("grammar/newline_trailing.lu", "exit(0)"),
    ("grammar/structlit_paren.lu", "exit(0)"),
    // D32's module graph. `cycle` and `unused` left this ledger at is06: the
    // resolve rung enforces the module laws now (E0303/E0305, closing
    // DIV-2026-002 and DIV-2026-005), so both fail at `resolve` exactly as
    // the corpus pins instead of running.
    ("resolve/forward/main.lu", "exit(0)"),
    ("resolve/pkgvis/main.lu", "exit(0)"),
    ("resolve/two_mod/main.lu", "exit(0)"),
    // Programs the type checker rejects and this machine runs anyway.
    ("typecheck/ambiguous.lu", "exit(0)"),
    ("typecheck/if_branch.lu", "exit(0)"),
    // Runs since pin 67c977f put the grammar's required commas into the file
    // (the upstream DIV-2026-001 repair).
    ("typecheck/match_exhaustive.lu", "exit(0)"),
    // -- is06: the sim scheduler (spec/03) ----------------------------------
    // The conc litmus tier runs. `freeze_publish` and `when_multi` produced
    // exit(1) against the corpus's exit=0 through pin `67c977f`; pin `79ceec6`
    // paid both debts (DIV-2026-008: the file reports through a channel now;
    // DIV-2026-009: the expected total is 223) and both run exit(0).
    // `chan_unsendable` and `store_buffer` ran clean through 0.1.5 where the
    // compiler rejects statically (E1102/E1101 conservatism); since 0.1.6
    // (the 13b811f re-pin, issue #19) this machine rejects them at its
    // resolve rung with the counterparty's codes and spans, so they left
    // this ledger for the fail column — the conservatism rows became
    // agreements.
    ("conc/cancel_sibling.lu", "exit(0)"),
    ("conc/freeze_publish.lu", "exit(0)"),
    ("conc/message_passing.lu", "exit(0)"),
    ("conc/select_seeded.lu", "exit(0)"),
    ("conc/when_multi.lu", "exit(0)"),
    // `channel` exists now, so the E1005 dynamic-counterpart case finally
    // RUNS: the open region cannot be transferred, trap(region-fault) citing
    // the clause — [conf.trap.map]'s E1005 → region-fault row, exercised.
    ("memory/region_move_while_open.lu", "trap(region-fault)"),
    // A local borrow sent into a channel: E1003 is the *static* borrow
    // checker's; under MVS the borrow is a value and the send copies it, so
    // the program runs clean — the standing conservatism class.
    ("memory/borrow_escape.lu", "exit(0)"),
    // Trait-tier programs whose `main` is Tier-0.
    ("traits/coherence_orphan/main.lu", "exit(0)"),
    ("traits/coherence_overlap.lu", "exit(0)"),
    ("traits/coherence_uncovered.lu", "exit(0)"),
    ("traits/dyn_assoc_escape.lu", "exit(0)"),
    ("traits/dyn_generic_method.lu", "exit(0)"),
    ("traits/dyn_ok.lu", "exit(0)"),
    ("traits/dyn_self_position.lu", "exit(0)"),
    ("traits/golden_arith.lu", "exit(0)"),
    ("traits/golden_eq.lu", "exit(0)"),
    ("traits/golden_missing_bound.lu", "exit(0)"),
    // -- is04 -------------------------------------------------------------
    // The unsafe litmuses. `unsafe_noalias.lu` runs *defined*: the assertion is
    // true, so `[mem.unsafe.raw.2]` is discharged and O5's `noalias` treatment
    // is licensed. `unsafe_ub_uaf.lu` is the oracle's first verdict — §7/P1
    // through a freed C allocation, satisfying the file's `run(exit=trap(ub))`
    // per `[conf.trap.map]`'s `ub` kind (see `ledger::ub_is_the_oracle_verdict`).
    ("memory/unsafe_noalias.lu", "exit(0)"),
    ("memory/unsafe_ub_uaf.lu", "ub(mem.ub)"),
    // The six fault-class programs this repo authored at is03 and upstreamed;
    // they arrived in `corpus/faults/` with pin `ecea37c` and the machine
    // already reproduced every one (`tests/faults/README.md`).
    ("faults/assert_fails.lu", "trap(assert)"),
    ("faults/bounds_slice.lu", "trap(bounds)"),
    ("faults/div_zero_rem.lu", "trap(div-zero)"),
    ("faults/exclusivity_nested_path.lu", "trap(exclusivity)"),
    ("faults/overflow_add.lu", "trap(overflow)"),
    ("faults/use_after_move_field.lu", "trap(use-after-move)"),
    // Const-generic normalization: `Buf[N + 1]` needs `type_arg ::= expr` to
    // read as an expression, which is01's parser only did for a *leading*
    // literal. Both files now parse, resolve and run.
    ("comptime/norm_linear.lu", "exit(0)"),
    ("comptime/norm_witness.lu", "exit(0)"),
    // -- is05 (pin 8b04edf) -------------------------------------------------
    // `regions.lu` RUNS AT LAST: the tail now reads `config.limit` under
    // `[mem.region.freeze.1]`'s "frozen data readable forever" instead of the
    // unspecified `frozen.get(config).is_valid()` — the is03 §5.5 finding,
    // closed by the pin rather than by this machine guessing.
    ("regions.lu", "exit(0)"),
    // s17's typecheck tier: the Tier-0 bodies run; the static rejections the
    // files pin (E0401/E0801/E0808…) are the compiler's half and ledger as
    // conservatism.
    ("rows/negative/open_into_closed.lu", "exit(1)"),
    ("rows/open_row_growth.lu", "exit(0)"),
    ("typecheck/cast_set.lu", "exit(0)"),
    ("typecheck/coerce_no_widening.lu", "exit(0)"),
    // exit(1) since 0.1.2: `Signal.Slow` dispatches to the `Slow` arm now
    // that bare identifiers resolve as variant patterns (issue #5) — the old
    // exit(0) was the first-arm-always bug's coincidence. The pinned
    // fail(E0801) stays the compiler's half (conservatism).
    ("typecheck/match_missing.lu", "exit(1)"),
    ("typecheck/match_unreachable.lu", "exit(0)"),
    ("typecheck/pattern_shape.lu", "exit(0)"),
    // -- is07 (pin 79ceec6) --------------------------------------------------
    // The three region-inference demonstrations run with zero annotations —
    // the machine's dynamic region semantics needed nothing new. The E1004/
    // E1010 litmuses pin *static* rejections (`fail(E1004)`/`fail(E1010)`);
    // their Tier-0 bodies run here and ledger as conservatism.
    // (`mode_missing_mut.lu` ran `exit(1)` here from is07 through 0.1.3 —
    // the X1 disagreement executing to a silently wrong answer. Since 0.1.4
    // it stops at `resolve` with E1007 (issue #15) and leaves this ledger,
    // exactly as the E0410 fail-files did.)
    ("memory/region_conflict_params.lu", "exit(0)"),
    ("memory/region_escape_local.lu", "exit(0)"),
    ("memory/region_infer_list_builder.lu", "exit(0)"),
    ("memory/region_infer_request_handler.lu", "exit(0)"),
    ("memory/region_infer_tree_transform.lu", "exit(0)"),
    // -- is08 (pin 843174f) --------------------------------------------------
    // The s20 S-batch made `procs.lu` and `conc/proc_kill_defers.lu`
    // self-contained (S-5 resolved): the supervision showcase finally RUNS,
    // and kill-skips-defers prints exactly `released`. The region-checker
    // tier's pass files run; the fail litmuses' dynamic counterparts land on
    // `[conf.trap.map]`'s region-fault row (E1005/E1011).
    // `region_freeze_write`'s write used to land on a value-semantics copy
    // (the standing conservatism class, wolf-interp#2); struct values now
    // carry their allocation-site region, so the write through the frozen
    // value traps `[mem.region.freeze.1]` like every other frozen path.
    ("procs.lu", "exit(0)"),
    ("conc/proc_kill_defers.lu", "exit(0)"),
    ("memory/region_freeze_ok.lu", "exit(0)"),
    ("memory/region_freeze_open.lu", "trap(region-fault)"),
    ("memory/region_freeze_write.lu", "trap(region-fault)"),
    ("memory/region_multiopen_values_ok.lu", "exit(0)"),
    ("memory/region_open_ancestor.lu", "trap(region-fault)"),
    ("memory/region_transfer_open.lu", "trap(region-fault)"),
    // -- 0.1.2 (pin a0c4564) -------------------------------------------------
    // The pin's new unsafe/checked litmuses ran clean here through 0.1.4
    // (`unsafe_raw_outside` and `unsafe_sig` pin static rejections
    // E1301/E1302, which this machine did not perform — their Tier-0
    // bodies ran as ledgered conservatism). At 0.1.5 issue #18 closed the
    // ring: both files now stop at resolve with the counterparty's codes
    // and spans, and leave this ledger (DIV-2026-012 carries the
    // rung-placement residue). `let_shadow_var_ok` is E0410's ok-twin: it
    // runs `exit(0)` now that same-scope `let` shadowing reads the *latest*
    // binding (the `rposition` repair) — its fail-twins `let_reassign` and
    // `let_compound_assign` stop at `resolve` with E0410 and so never enter
    // this ledger.
    ("memory/checked_unsigned.lu", "exit(0)"),
    ("memory/unsafe_creation_not_use.lu", "exit(0)"),
    ("memory/unsafe_trusted.lu", "exit(0)"),
    ("typecheck/let_shadow_var_ok.lu", "exit(0)"),
    // -- 0.1.3 (pin d147a54) -------------------------------------------------
    // The s27 realignment's run-rung gains, two families:
    //
    // The pin's own new witnesses run to their pinned exit —
    // `rows/qmark_defer.lu` (postfix rows, `?` across defer chains, lowercase
    // declared tags dispatching in `match`) and `faults/assert_msg_holds.lu`
    // (the two-arg assert whose holding message stays cold).
    //
    // Impl-block methods dispatch now (`[mem.iter.for]` needed `next`; the
    // rest came with it): the method-shaped files that were `unsupported`
    // reach `run`. `method_inherent` prints both resolution orders
    // (inherent wins; `Speak.speak(d)` reaches the shadowed trait method);
    // `receiver_modes`, `exclusivity`, `view_set_norm`, `assoc_rewrite`
    // (`exit(7)` is its own pinned arithmetic) and `show_bound` run clean.
    // `method_ambiguous`, `method_scope/main`, `view_set_violation` and
    // `receiver_bare_mut` run where the compiler statically rejects — the
    // standing conservatism class, not divergences.
    ("faults/assert_msg_holds.lu", "exit(0)"),
    ("memory/exclusivity.lu", "exit(0)"),
    ("memory/view_set_norm.lu", "exit(0)"),
    ("memory/view_set_violation.lu", "exit(0)"),
    ("rows/qmark_defer.lu", "exit(0)"),
    ("traits/assoc_rewrite.lu", "exit(7)"),
    ("traits/show_bound.lu", "exit(0)"),
    ("typecheck/method_ambiguous.lu", "exit(0)"),
    ("typecheck/method_inherent.lu", "exit(0)"),
    ("typecheck/method_scope/main.lu", "exit(0)"),
    ("typecheck/receiver_bare_mut.lu", "exit(42)"),
    ("typecheck/receiver_modes.lu", "exit(0)"),
    // -- 0.1.4 (pin ad6cef7) -------------------------------------------------
    // The s29+s30 witnesses run to their pinned outcomes on this machine's
    // rungs too: the native C-set litmus (`c.calloc(8, 8)` is 64 bytes —
    // issue #13's fix; the modelled heap now agrees with real glibc), the
    // erring-`!int` main (`error: Boom` + exit 1, bit-for-bit with the
    // native rung), the IEEE comparison witness (`nan != nan` is TRUE —
    // wolf-lang#22 was the compiler's half; this machine's f64 model was
    // already IEEE), and the same-name two-module program (wolf-lang#26 —
    // `lista.len` and `stra.len` stay distinct functions here as they do
    // as mangled WIR names).
    ("memory/unsafe_c_alloc_native.lu", "exit(0)"),
    ("resolve/same_name/main.lu", "exit(0)"),
    ("rows/eu_main_err_exit.lu", "exit(1)"),
    ("typecheck/float_nan_cmp.lu", "exit(0)"),
    // -- 0.1.5 (pin f0da6e6) -------------------------------------------------
    // The five-lane fan-out pin's witnesses, on this machine's rungs:
    //
    // The strings tier runs — the s37 builtin surface with `[mem.str.get]`
    // and `^n` end-relative offsets (`builtin_methods`), the §7.4 format
    // specs rendering byte-exact with the corpus pins (`format_spec_width`,
    // `format_spec_full`, `float_format` — shortest-round-trip floats,
    // zero-pad after the sign, sign-magnitude bases), the value-position
    // interpolation the native lane still refuses (`interp_value_position`),
    // and D25's first defined fault (`slice_oob_trap` traps `bounds`).
    //
    // The lints tier runs its programs: `#[allow(…)]` attributes parse and
    // the bodies execute. This machine runs no warning analyses, so its
    // records carry no `warnings` array — `[proto.record.warn]`'s
    // honest-absent, never a divergence — and the `warns:` directives are
    // accepted, enforced only for analyses an implementation has.
    //
    // `io/eprint.lu` runs: `eprint`/`eprint_raw` render through the same
    // fmt machinery onto stderr, and stdout stays clean — which is the pin.
    // The fs tier stays out by design (no filesystem in this machine); both
    // fs files are `unsupported` naming their construct.
    ("io/eprint.lu", "exit(0)"),
    ("lints/allow_item.lu", "exit(0)"),
    ("lints/allow_nothing.lu", "exit(0)"),
    ("lints/allow_unknown_code.lu", "exit(0)"),
    ("lints/safety_comment_missing.lu", "exit(0)"),
    ("strings/builtin_methods.lu", "exit(0)"),
    ("strings/float_format.lu", "exit(0)"),
    ("strings/format_spec_full.lu", "exit(0)"),
    ("strings/format_spec_width.lu", "exit(0)"),
    ("strings/interp_value_position.lu", "exit(0)"),
    ("strings/slice_oob_trap.lu", "trap(bounds)"),
    // -- 0.1.6 (pin 13b811f) -------------------------------------------------
    // The wave-four pin's witnesses, on this machine's rungs:
    //
    // The s68 lint fixtures run their programs — every one is advisory, the
    // program legal — and this machine now populates the `warnings` array
    // over them (the eleven shared-analysis lints plus W0302/W0303; the
    // compiler-only four stay honest-absent, so `binder_capitalized`,
    // `discarded_result`, `float_zero_minus` and `region_never_allocates`
    // run warning-clean *here* while their `warns:` ledgers name codes this
    // machine does not observe).
    //
    // The s34 proc pair runs: cancellation defers fire under kill teardown,
    // and a linked exit delivers through the mailbox channel.
    //
    // Two of the three P-project witnesses run natively (`rpn`, `wordtree`);
    // `count` needs the fs tier this machine declines by design.
    ("conc/proc_cancel_defers.lu", "exit(0)"),
    ("conc/proc_link.lu", "exit(0)"),
    ("lints/assume_reassigned.lu", "exit(0)"),
    ("lints/binder_capitalized.lu", "exit(0)"),
    ("lints/discarded_result.lu", "exit(0)"),
    ("lints/else_comparison.lu", "exit(0)"),
    ("lints/float_zero_minus.lu", "exit(0)"),
    ("lints/mut_in_interp.lu", "exit(0)"),
    ("lints/narrowing_literal.lu", "exit(0)"),
    ("lints/prefix_statement.lu", "exit(0)"),
    ("lints/raw_interp_braces.lu", "exit(0)"),
    ("lints/region_never_allocates.lu", "exit(0)"),
    ("lints/shadow_prelude.lu", "exit(0)"),
    ("lints/tag_name_collision.lu", "exit(0)"),
    ("projects/rpn.lu", "exit(0)"),
    ("projects/wordtree.lu", "exit(0)"),
    // -- 0.1.7 (pin e94b879) -------------------------------------------------
    // The wave-six pin's witnesses, on this machine's rungs:
    //
    // The five X3 value-path overflow litmuses trap — issue #21's fix:
    // container-element literals adopt the element's checking context
    // (`List[i32]`'s annotation travels on the value; a pushed literal with
    // no context adopts `int`, 64-bit, like every other literal), so
    // element loads, compound element writes, call results and field loads
    // all feed checked arithmetic at the sema width.
    //
    // The s69 idiom-lint fixtures run their programs (all advisory); the
    // eleven idiom analyses are compiler-side, honest-absent here (see
    // `lint::HONEST_ABSENT`), so the files run warning-clean on this
    // machine while their `warns:` ledgers name compiler observations.
    //
    // The s70 match tier: str-literal matches dispatch by equality
    // (`match_str_dispatch`), a duplicated literal arm is advisory
    // (`match_str_arm_unreachable`), and `match_str_nonexhaustive` runs
    // here as conservatism (E0801 exhaustiveness is the compiler's);
    // `handler_match_tags` pins the #48 tag-before-binding rule this
    // machine's handler matches already implement.
    ("faults/overflow_call_result.lu", "trap(overflow)"),
    ("faults/overflow_elem_read.lu", "trap(overflow)"),
    ("faults/overflow_elem_write.lu", "trap(overflow)"),
    ("faults/overflow_field_read.lu", "trap(overflow)"),
    ("faults/overflow_interp_operand.lu", "trap(overflow)"),
    ("lints/as_view_consuming.lu", "exit(0)"),
    ("lints/get_prefix.lu", "exit(0)"),
    ("lints/get_without_row.lu", "exit(0)"),
    ("lints/match_str_arm_unreachable.lu", "exit(0)"),
    ("lints/mut_param_unwritten.lu", "exit(0)"),
    ("lints/one_item_module/main.lu", "exit(0)"),
    ("lints/pkg_item_unused/main.lu", "exit(0)"),
    ("lints/predicate_shape.lu", "exit(0)"),
    ("lints/pub_undocumented.lu", "exit(0)"),
    ("lints/tag_case_payload.lu", "exit(0)"),
    ("lints/take_returned.lu", "exit(0)"),
    ("memory/list_elem_assign.lu", "exit(0)"),
    // The s40 env/time tier runs (0.1.7): overlay env (never the host's),
    // empty argv (the stdin posture mirrored), cwd as process state, X12
    // monotonic time, and `os_exit`'s defer-skipping termination. The
    // process trio and the json kernels decline honestly (exec surface /
    // the counterparty's reference parser) and stay out of scope.
    ("os/args_cwd.lu", "exit(0)"),
    ("os/env_roundtrip.lu", "exit(0)"),
    ("os/exit_code.lu", "exit(7)"),
    ("time/monotonic.lu", "exit(0)"),
    ("rows/handler_match_tags.lu", "exit(0)"),
    ("strings/match_str_dispatch.lu", "exit(0)"),
    ("typecheck/match_str_nonexhaustive.lu", "exit(0)"),
    // The wave-seven pin, `3d5cee6` (0.1.8, s71): the empty needle is
    // DEFINED (`[mem.str.empty]` — count 0, split one whole piece, replace
    // identity; two former declines run now), a negative repeat traps
    // `assert` per `[mem.str.repeat]`, and `else |Tag(p)|` binds the
    // payload on a total single-tag row (the run half of the E0809 pair —
    // its negative twin fails at resolve and never reaches this ledger).
    ("faults/repeat_negative.lu", "trap(assert)"),
    ("rows/else_tag_payload.lu", "exit(0)"),
    ("strings/empty_needle.lu", "exit(0)"),
    // The s72 mode-teeth fail-files (same pin): each pins a static code
    // whose dynamic meaning is exclusivity, and this machine traps it —
    // D39's write barrier (E1014), the caller-side overlap rule (E1002),
    // and D40's iteration claim (E1013). Dynamic counterparts, all three.
    ("memory/list_mutate_while_iter.lu", "trap(exclusivity)"),
    ("memory/mut_read_overlap.lu", "trap(exclusivity)"),
    ("memory/read_param_write.lu", "trap(exclusivity)"),
    // The c09-wave pin, `0b4e79c` (0.1.9, s73): the corpus grows one —
    // the `--schedules=N` dogfood witness. Both select arms are conforming
    // ([conc.det.events]); either seed's choice exits 0 on this machine
    // exactly as it does natively now that wolfgang runs conc too.
    ("test/conc_schedules_test.lu", "exit(0)"),
    // The mid-end/whole-program pin, `613c3dc` (0.1.10, s42/s43/s63): four
    // new files, and all four reach `run` here at first sight — no new
    // semantics were needed, which is the point. `select_two_timeouts.lu`
    // is wolfgang's #64 regression litmus (GVN hash-consing a second
    // select's timeout sentinel onto the first's non-dominating
    // definition); this machine has no GVN, so it is a plain oracle for
    // the answer the optimizer must not change. The three `kernels/` files
    // are s42's checked-arith and region-promotion shapes: every one of
    // them is an optimization *target* upstream and an ordinary program
    // here, which is exactly the differential's leverage over the
    // mid-end.
    ("conc/select_two_timeouts.lu", "exit(0)"),
    ("kernels/churn_b3.lu", "exit(0)"),
    ("kernels/hot_counter.lu", "exit(0)"),
    ("kernels/hot_scale_versioned.lu", "exit(0)"),
    // The semantics pin, `f8dca42` (0.1.11, s74…s78 + s53). The largest
    // semantic movement the compiler has had in one wave, and ten of the
    // thirteen new files reach `run` here at FIRST SIGHT — no new
    // semantics were written on this side for any of them. That is the
    // pass's headline, so be precise about what each one confirms:
    //
    // s76 (containers allocate in the AMBIENT region, dynamically scoped
    // per D12): `region_container_reclaim.lu` builds a `List` in a callee
    // that lands in its CALLER's region, grows it past the first chunk,
    // and frees it wholesale sixteen times. This machine models regions
    // dynamically and has always placed a callee's allocation in the
    // ambient region, so the compiler's move — from "containers opt out
    // of the region story" to "the ambient region at the allocation site"
    // — is a move TOWARD this machine's reading, and the answer (2096128
    // per round) is unchanged. `region_container_freeze_ok.lu` is the
    // `freeze`-outlives half ([mem.region.freeze.1]).
    // `region_escape_container.lu` runs to `exit(0)` here and is E1010
    // upstream: the escape is a COMPILE-TIME region judgement this
    // machine does not make, so it ledgers as static conservatism — the
    // honest pairing, not a divergence. See the approximation contract.
    //
    // s77 (`s.bytes()` is a view over the receiver's own storage):
    // `byte_view.lu` pins bytes UNSIGNED (`é` is 195, 169 — never
    // negative) and the view's length as the byte length;
    // `slice_boundary_sweep.lu` sweeps all 49 endpoint pairs of `é€` and
    // counts exactly 6 defined and 15 in-range misses, which is the
    // `lo <=u hi <=u len` domain plus a boundary probe per endpoint. Both
    // agree here without a line of new code — the strongest evidence in
    // the wave that the two slice domains are the same domain.
    //
    // s74 (the correctness cluster): `chan_drain_after_inclusive_loop.lu`,
    // `select_single_arm_loop.lu`, `mut_param_aggregate_store.lu` and
    // `match_narrow_scrutinee.lu` are all WIR/backend defects upstream —
    // a cascading Braun trivial-φ, a printing-order block walk, a
    // whole-aggregate store, an arm constant at the wrong width. This
    // machine is a tree-walker with none of those mechanisms, so it is a
    // clean oracle for every one of them, and it answers what the headers
    // pin.
    //
    // s53: `shebang.lu` is the one file in the wave that DID need a
    // reading here — `[gram.lex.shebang]`, the wave's only spec delta.
    ("conc/chan_drain_after_inclusive_loop.lu", "exit(0)"),
    // The c9da6d9 pin (is14): the compiler's c19-close/c21/c22/s98 wave.
    // All nine new files reach run here; eight match their pins outright,
    // and the ninth (dyn_temp_refused) runs where the corpus pins E0810 —
    // static conservatism by design (D47's place rule is a sema judgement
    // this machine does not make; the ledger row is wolf-interp#31's
    // disposition).
    ("conc/proc_spawn_loop.lu", "exit(0)"),
    ("conc/spawn_fanout_loop.lu", "exit(0)"),
    ("generics/box_method.lu", "exit(0)"),
    ("generics/first_of_list.lu", "exit(0)"),
    ("generics/hundred_shapes.lu", "exit(0)"),
    ("generics/pair_of.lu", "exit(0)"),
    ("generics/two_instances.lu", "exit(0)"),
    ("generics/two_level_raise.lu", "exit(0)"),
    ("traits/dyn_temp_refused.lu", "exit(0)"),
    // is14's own two movers (#32): the dispatch floor under an impl is
    // the trait's default, and an adapter cast moves the nominal
    // identity. Both print what the checker's lanes print.
    ("traits/adapter_distinct/main.lu", "exit(0)"),
    ("typecheck/trait_default.lu", "exit(0)"),
    ("conc/select_single_arm_loop.lu", "exit(0)"),
    ("grammar/shebang.lu", "exit(0)"),
    ("memory/mut_param_aggregate_store.lu", "exit(0)"),
    ("memory/region_container_freeze_ok.lu", "exit(0)"),
    ("memory/region_container_reclaim.lu", "exit(0)"),
    ("memory/region_escape_container.lu", "exit(0)"),
    ("strings/byte_view.lu", "exit(0)"),
    ("strings/slice_boundary_sweep.lu", "exit(0)"),
    ("typecheck/match_narrow_scrutinee.lu", "exit(0)"),
    // The 0.1.12 pin, `4e316ad` (s79 + s80 + s81). Three files, one per
    // sprint, and two of them reach `run` here at first sight.
    //
    // s80 (`region.foreign` roots are role-scoped, not region-scoped):
    // `foreign_root_aliasing.lu` is a REAL MISCOMPILE's witness — the
    // release tier was answering `x=5 y=5` for a program whose answer is
    // `x=5 y=7`. This machine prints `x=5 y=7` and always did, on the one
    // engine it has; there was never a bug here to fix, because the
    // aliasing question the optimizer got wrong is one an interpreter
    // never has to ask. It is the cleanest possible demonstration of what
    // the oracle is FOR, and it now agrees on all three counterparty
    // tiers including `--release`.
    //
    // s81 (the str-construction border): `equality_lanes.lu` pins that
    // `==` on `str` is the same answer however it is lowered, and ran
    // clean here at first sight. `from_utf8_border.lu` is the one file in
    // this wave that needed a reading — `str_from_utf8` is a new prelude
    // builtin with a `{utf8}` row and NO spec clause, so the implementation
    // here is written against the prelude signature, the counterparty's
    // doc comments and this witness (see `builtin::call`). Both machines
    // agree on all 38 ugly inputs probed beyond the witness: lone
    // continuations, truncations, overlongs, surrogates, past-U+10FFFF,
    // never-bytes, non-byte elements (256, -1), interior NUL and the
    // empty list.
    ("memory/foreign_root_aliasing.lu", "exit(0)"),
    ("strings/equality_lanes.lu", "exit(0)"),
    ("strings/from_utf8_border.lu", "exit(0)"),
];

#[test]
fn no_corpus_file_mismatches_its_expectation() {
    // A mismatch that has been triaged and filed in `docs/divergence-log.md`
    // (mirrored by `differ::FILED_DIVERGENCES`) is a *known* disagreement:
    // still visible in every differential report, no longer re-asserted here.
    // The waiver dies with the filing — resolve the entry and this test
    // resumes gating the file.
    let mismatches: Vec<String> = entries()
        .iter()
        .filter(|entry| entry.judgement.is_mismatch())
        .filter(|entry| wolf_interp::differ::filed(&entry.path).is_none())
        .map(|entry| format!("  {}: {}", entry.path, entry.judgement))
        .collect();
    assert!(
        mismatches.is_empty(),
        "the corpus and this implementation disagree. `[proto.cmp.triage]`: the spec document \
         is the defendant first — triage each of these, do NOT edit the corpus:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn the_warns_ledger_is_enforced_for_the_analyses_this_machine_runs() {
    // `warns:` is the exact set of warning codes an entry is expected to
    // produce; its absence is the empty set (a file with no `warns:` must be
    // warning-clean). An implementation enforces the directive only for the
    // analyses it actually runs — `[proto.cmp.warn]`'s honest-absent rule
    // makes a missing analysis a scope gap, never a divergence — so both
    // sides of the comparison are filtered to `lint::IMPLEMENTED` here.
    let implemented: std::collections::BTreeSet<&str> =
        wolf_interp::lint::IMPLEMENTED.iter().copied().collect();
    let mut problems = Vec::new();
    for entry in entries() {
        let Some(warnings) = &entry.warnings else {
            // The analyses never ran (the program did not load) — nothing
            // to enforce, honestly.
            continue;
        };
        let expected: std::collections::BTreeSet<&str> = entry
            .directives
            .warns
            .iter()
            .map(String::as_str)
            .filter(|code| implemented.contains(code))
            .collect();
        let observed: std::collections::BTreeSet<&str> = warnings
            .iter()
            .map(|warning| warning.code.as_str())
            .filter(|code| implemented.contains(code))
            .collect();
        if expected != observed {
            problems.push(format!(
                "  {}: warns ledger {expected:?}, observed {observed:?}",
                entry.path
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "the corpus `warns:` ledgers and this machine's lint pass disagree over the \
         implemented codes:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_run_ledger_is_exactly_what_reaches_run() {
    let observed: BTreeMap<String, String> = entries()
        .into_iter()
        .filter(|entry| entry.phase == Phase::Run)
        .map(|entry| (entry.path, entry.verdict.to_string()))
        .collect();
    let expected: BTreeMap<String, String> = RUN_LEDGER
        .iter()
        .map(|(path, verdict)| ((*path).to_owned(), (*verdict).to_owned()))
        .collect();

    let gained: Vec<&String> = observed
        .keys()
        .filter(|k| !expected.contains_key(*k))
        .collect();
    let lost: Vec<&String> = expected
        .keys()
        .filter(|k| !observed.contains_key(*k))
        .collect();
    assert!(
        gained.is_empty() && lost.is_empty(),
        "the set of files reaching `run` moved.\n  newly running (progress — add them): {gained:?}\
         \n  no longer running (regression): {lost:?}"
    );
    assert_eq!(observed, expected, "a file's run outcome changed");
}

#[test]
fn the_four_seed_files_are_accounted_for() {
    // The sprint's corpus-coverage target names `hello.lu`, `errors.lu`,
    // `overflow.lu`, `strings.lu`. Two run; two do not, and the reason is
    // recorded here rather than left as an absence.
    let by_path: BTreeMap<String, Entry> = entries()
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();

    let hello = &by_path["hello.lu"];
    assert_eq!(hello.verdict, Verdict::Exit(0));
    assert_eq!(hello.stdout, "hello, wolf\n");
    assert!(matches!(hello.judgement, Judgement::Match(_)));

    let overflow = &by_path["overflow.lu"];
    assert_eq!(overflow.verdict, Verdict::Trap(TrapKind::Overflow));

    // `errors.lu` and `strings.lu` need std surface that no pinned document
    // specifies (`acquire`/`release`/`Resource.text`; `re"…"` literals and
    // `^n` from-end indexing). Declining is the honest answer — see the
    // `builtin` module's two rules — and the reason travels on `x-unsupported`.
    for name in ["errors.lu", "strings.lu"] {
        let entry = &by_path[name];
        assert_eq!(entry.verdict, Verdict::Unsupported, "{name}");
        assert_eq!(entry.phase, Phase::Resolve, "{name}");
        assert!(
            matches!(entry.judgement, Judgement::OutOfScope(_)),
            "{name}"
        );
    }
}

#[test]
fn every_run_expectation_this_machine_reaches_is_met_exactly() {
    // The narrow claim, stated separately from the broad one: for every corpus
    // file whose `check:` is a *run* expectation and which this machine
    // evaluated, the termination and the output agree.
    let mut checked = 0usize;
    for entry in entries() {
        let Some(Check::Run { exit, stdout }) = &entry.directives.check else {
            continue;
        };
        if entry.phase != Phase::Run {
            continue;
        }
        // A filed divergence (docs/divergence-log.md) is a known disagreement:
        // visible in every differential report, waived here until resolved.
        if wolf_interp::differ::filed(&entry.path).is_some() {
            continue;
        }
        checked += 1;
        match (exit, &entry.verdict) {
            (ExitSpec::Code(want), Verdict::Exit(got)) => {
                assert_eq!(want, got, "{}", entry.path);
            }
            (ExitSpec::Trap(Some(want)), Verdict::Trap(got)) => {
                // `[conf.trap.exit]`: compare the kind, never a status number.
                assert_eq!(want, got, "{}", entry.path);
            }
            (ExitSpec::Trap(None), Verdict::Trap(_)) => {}
            // `run(exit=trap(ub))` against `ub(anchor)`: one event, two lenses.
            // `[conf.trap.map]` routes the `ub` kind's comparison semantics to
            // `[proto.record.ub]` precisely so the two must agree.
            (ExitSpec::Trap(want), Verdict::Ub(anchor)) => {
                assert!(
                    ledger::ub_is_the_oracle_verdict(*want),
                    "{}: expected {}, observed ub({anchor})",
                    entry.path,
                    ExitSpec::Trap(*want)
                );
            }
            (want, got) => panic!("{}: expected {want}, observed {got}", entry.path),
        }
        if let Some(want) = stdout {
            assert!(
                ledger::stdout_matches(want, &entry.stdout),
                "{}: expected stdout {want:?}, observed {:?}",
                entry.path,
                entry.stdout
            );
        }
    }
    assert!(
        checked >= 20,
        "only {checked} run expectations were exercised"
    );
}

#[test]
fn phase_reached_is_never_inflated() {
    // `[proto.record.phase]`, the s13 convention: `phase_reached` names the
    // deepest phase that COMPLETED. An `unsupported` verdict must therefore
    // never claim `run`, and a `fail` must name the rung that failed.
    for entry in entries() {
        match &entry.verdict {
            Verdict::Unsupported => assert!(
                entry.phase < Phase::Run,
                "{}: unsupported at {}",
                entry.path,
                entry.phase
            ),
            Verdict::Fail(_) => assert!(
                entry.phase <= Phase::Resolve,
                "{}: this implementation fails at lex/parse/resolve only, not {}",
                entry.path,
                entry.phase
            ),
            // A `ub` verdict reaches `run` exactly as a trap does: the
            // execution *completed*, at the defined stopping point "this has no
            // defined behavior". Inflating or deflating it would both be lies.
            Verdict::Exit(_) | Verdict::Trap(_) | Verdict::Ub(_) => {
                assert_eq!(entry.phase, Phase::Run, "{}", entry.path);
            }
            Verdict::Pass => {
                panic!("{}: unexpected verdict {}", entry.path, entry.verdict)
            }
        }
    }
}

#[test]
fn unsafe_free_corpus_programs_never_produce_a_ub_verdict() {
    // The sprint's headline claim as a CI assertion: **zero UB in safe code**.
    //
    // `[mem.ub]` states it — "Safe-tier programs cannot reach any row — every
    // row requires Tier 3 (or an FFI boundary) in the execution" — and this is
    // that sentence checked against every program in the pinned corpus rather
    // than argued. The filter is the source text: a file with no `unsafe`
    // block, no `asm`, and no `import c` is safe-tier by construction, and its
    // verdict must never be `ub(…)`.
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("walkable");
    let mut safe = 0usize;
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let text = String::from_utf8_lossy(&source);
        if text.contains("unsafe") || text.contains("import c ") || text.contains("asm ") {
            continue;
        }
        safe += 1;
        let (record, observed) = wolf_interp::observe_record(&full, &source, None);
        assert!(
            !matches!(record.verdict, Verdict::Ub(_)),
            "{}: a safe-tier program reached §7/{} — either the machine is wrong or `[mem.ub]`'s \
             \"safe-tier programs cannot reach any row\" is\n{}",
            file.path,
            observed
                .ub
                .as_ref()
                .map_or_else(|| "?".to_owned(), |finding| finding.row.to_string()),
            observed
                .ub
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
        );
    }
    assert!(safe > 90, "only {safe} safe-tier programs were checked");
}

#[test]
fn every_unsupported_file_says_why() {
    // "the protocol verdict carries no payload — `[proto.record.verdict]`; the
    // reason rides an `x-` extension key". A reasonless `unsupported` is a hole
    // in the conservatism ledger.
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("walkable");
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let (record, _) = wolf_interp::observe_record(&full, &source, None);
        if record.verdict != Verdict::Unsupported {
            continue;
        }
        let reason = record
            .extensions
            .get("x-unsupported")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        assert!(
            reason.len() > 10,
            "{}: `unsupported` with no useful reason ({reason:?})",
            file.path
        );
    }
}

#[test]
fn every_record_is_schema_valid_including_the_running_ones() {
    // A record with stdout has to carry both the digest and the inline text
    // (`[proto.record.fields]`), and this implementation must never emit one
    // its own validator would reject.
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("walkable");
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let (record, _) = wolf_interp::observe_record(&full, &source, None);
        let value = serde_json::to_value(&record).expect("serializes");
        assert_eq!(
            wolf_interp::schema::validate(&value),
            Ok(()),
            "{}",
            file.path
        );
        if let Verdict::Exit(_) = record.verdict
            && record.stdout_inline.as_ref().is_some_and(|s| !s.is_empty())
        {
            let digest = record.stdout_sha256.as_ref().expect("a digest");
            assert_eq!(digest.len(), 64, "{}", file.path);
        }
    }
}

#[test]
fn the_stdout_digest_is_the_digest_of_the_stdout() {
    // The digest is a comparison surface: two implementations agree on output
    // by agreeing on this string, so it had better be computed from the bytes.
    let root = corpus_root();
    let path = root.join("hello.lu");
    let source = std::fs::read(&path).expect("readable");
    let (record, _) = wolf_interp::observe_record(&path, &source, None);
    assert_eq!(record.stdout_inline.as_deref(), Some("hello, wolf\n"));
    assert_eq!(
        record.stdout_sha256.as_deref(),
        Some(wolf_interp::sha256::hex(b"hello, wolf\n").as_str())
    );
}

#[test]
fn evaluation_is_deterministic_over_the_corpus() {
    // Two runs, same verdict, same bytes. Cheap, and it catches the whole class
    // of bugs where a result depends on hash iteration or allocation order —
    // which would make every differential comparison meaningless.
    let root = corpus_root();
    let report = corpus::walk(&root, None).expect("walkable");
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let (first, _) = wolf_interp::observe_record(&full, &source, None);
        let (second, _) = wolf_interp::observe_record(&full, &source, None);
        assert_eq!(first, second, "{} observed differently twice", file.path);
    }
}

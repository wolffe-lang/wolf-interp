//! Spec conformance at the `lex` and `parse` rungs.
//!
//! This is the sprint's acceptance test, and it is driven entirely by the
//! pinned corpus's own ledger rather than by a list maintained here:
//!
//! - `phase:` names the deepest rung that succeeds today
//!   (`[conf.directive.phase]`). A file whose ledger phase is `parse` or deeper
//!   **must parse clean**; a file whose ledger phase is `lex` lexes clean and
//!   then **must fail at parse**.
//! - `check: fail(CODE)` matches the failing code **exactly**
//!   (`[conf.directive.check]`), so the four `corpus/grammar/` counter-examples
//!   pin four of our codes to the byte.
//! - `member: true` files are compiled through their module's entry file and
//!   are never conform-run directly (`[conf.directive.member]`) — but they are
//!   still wolf, so they still have to parse.
//!
//! Nothing here hardcodes a file name. A pin bump that adds a corpus file adds
//! a case; a pin bump that moves a file's ledger phase moves the expectation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wolf_interp::corpus::{self, Outcome};
use wolf_interp::directive::Check;
use wolf_interp::frontend;
use wolf_interp::parse;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;

fn upstream() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(wolf_interp::upstream_root())
}

struct Case {
    /// Corpus-relative, `/`-separated.
    path: String,
    source: Vec<u8>,
    /// `None` for `member: true` files.
    ledger_phase: Option<Phase>,
    check: Option<Check>,
    /// The file's own `conforms:` tags. is37 reads them: E0401 is pinned both
    /// by files this rung owns and by files it does not, and `type.byte` is
    /// the corpus's own statement of which is which (wolf-interp#58's rule —
    /// derive the set from the directive, never hand-write it).
    conforms: Vec<String>,
}

fn cases() -> Vec<Case> {
    let root = upstream().join("corpus");
    let report = corpus::walk(&root, None).expect("the pinned corpus is walkable");
    report
        .files
        .into_iter()
        .map(|file| {
            let source = std::fs::read(root.join(&file.path)).expect("readable");
            let (ledger_phase, check, conforms) = match &file.outcome {
                Outcome::Entry(d) => (d.phase, d.check.clone(), d.conforms.clone()),
                Outcome::Member(_) => (None, None, Vec::new()),
                Outcome::Failed(reason) => panic!("{}: {reason}", file.path),
            };
            Case {
                path: file.path,
                source,
                ledger_phase,
                check,
                conforms,
            }
        })
        .collect()
}

/// The code a `check: fail(CODE)` directive pins, if it pins one.
fn pinned_code(check: Option<&Check>) -> Option<&str> {
    match check {
        Some(Check::Fail(code)) => Some(code.as_str()),
        _ => None,
    }
}

/// Does this file pin the E0401 the **resolve** rung owns?
///
/// E0401 is the one code the two rungs share. Since is37 (wolf-interp#62)
/// resolve owns `[type.byte]`'s domain — an `int` in a byte slot, refused at
/// the counterparty's own span — while the D54 numeric-literal files pinning
/// the same code stay the conservatism class, because their rule is the
/// checker's. The corpus says which is which: a byte-domain witness tags
/// `type.byte`.
fn is_byte_domain_case(case: &Case) -> bool {
    pinned_code(case.check.as_ref()) == Some("E0401")
        && case.conforms.iter().any(|tag| tag == "type.byte")
}

/// A corpus file whose disagreement with this implementation is already
/// filed and triaged in `docs/divergence-log.md`.
///
/// A filed divergence stays visible in every differential report
/// (`x-filed`); what it stops doing is failing *these* tests, which assert
/// the corpus and this implementation agree — for a filed finding they
/// documentedly do not, and re-asserting the disagreement every run adds no
/// information. The entry leaves `differ::FILED_DIVERGENCES` only when the
/// resolving commit lands, and then these tests resume asserting.
fn is_filed_divergence(path: &str) -> bool {
    wolf_interp::differ::filed(path).is_some()
}

/// A `member` file in a directory whose entry's disagreement is filed — the
/// member is the filed pin's *subject*, not a case of its own. D59's
/// broken-sibling witness is the shape: `mangled.lu` is deliberately
/// unparseable and `entry.lu` pins the module's parse failure
/// (DIV-2026-019), so asserting the member parses would re-assert the filed
/// disagreement one file over. The waiver dies with the filing, exactly as
/// [`is_filed_divergence`]'s does.
fn is_member_of_filed_module(case: &Case) -> bool {
    case.ledger_phase.is_none()
        && case.path.rsplit_once('/').is_some_and(|(dir, _)| {
            wolf_interp::differ::FILED_DIVERGENCES
                .iter()
                .any(|(file, _, _)| file.starts_with(&format!("{dir}/")))
        })
}

#[test]
fn every_corpus_file_lexes_clean() {
    // Every ledger phase in the corpus is `lex` or deeper *except* the files
    // whose ledger is `phase: none` — a corpus file that is refused BY the
    // lexer, and whose own directive says so. Everything else is not allowed
    // a lexical fault, the counter-examples included: they are syntax
    // errors, not lexical ones.
    //
    // The exemption arrived at e6cf24e (is32) with r04's
    // `grammar/char_uni_seven_digits.lu` — the corpus's first `phase: none`
    // entry, pinning `fail(E0101)` for a `\u{…}` escape whose digit count
    // leaves the production's one-to-six.
    let mut failures = Vec::new();
    for case in cases() {
        if case.ledger_phase == Some(Phase::None) {
            let observation = frontend::observe(&case.source, Some(Phase::Lex));
            assert!(
                matches!(observation.verdict, Verdict::Fail(_)),
                "{}: ledger says `phase: none`, so the lexer must reject it; got {}",
                case.path,
                observation.verdict
            );
            let expected = pinned_code(case.check.as_ref())
                .unwrap_or_else(|| panic!("{}: a none-ledger file must pin a code", case.path));
            let Verdict::Fail(code) = &observation.verdict else {
                unreachable!("checked above")
            };
            assert_eq!(code, expected, "{}", case.path);
            continue;
        }
        let observation = frontend::observe(&case.source, Some(Phase::Lex));
        if observation.verdict != Verdict::Pass {
            failures.push(format!(
                "  {}: {} ({:?})",
                case.path, observation.verdict, observation.detail
            ));
        }
        assert_eq!(observation.phase_reached, Phase::Lex, "{}", case.path);
    }
    assert!(failures.is_empty(), "lex rung:\n{}", failures.join("\n"));
}

#[test]
fn files_that_reach_parse_or_deeper_parse_clean() {
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for case in cases() {
        // Members carry no ledger phase but are compiled through their entry,
        // so they must parse too.
        let must_parse = match case.ledger_phase {
            None => true,
            Some(phase) => phase >= Phase::Parse,
        };
        if !must_parse || is_filed_divergence(&case.path) || is_member_of_filed_module(&case) {
            continue;
        }
        checked += 1;
        let observation = frontend::observe(&case.source, Some(Phase::Parse));
        if observation.verdict != Verdict::Pass {
            failures.push(format!(
                "  {}: {} — {}",
                case.path,
                observation.verdict,
                observation
                    .detail
                    .map_or_else(|| "no detail".to_owned(), |d| d.to_string())
            ));
        }
    }
    assert!(failures.is_empty(), "parse rung:\n{}", failures.join("\n"));
    assert!(checked > 50, "expected most of the corpus to reach parse");
}

#[test]
fn files_whose_ledger_stops_at_lex_fail_at_parse_with_their_pinned_code() {
    let mut seen = BTreeMap::new();
    for case in cases() {
        if case.ledger_phase != Some(Phase::Lex) {
            continue;
        }
        let observation = frontend::observe(&case.source, Some(Phase::Parse));
        assert_eq!(
            observation.phase_reached,
            Phase::Parse,
            "{}: a parse failure reports the phase that failed",
            case.path
        );
        let Verdict::Fail(code) = &observation.verdict else {
            panic!(
                "{}: ledger says `phase: lex`, so parse must reject it; got {}",
                case.path, observation.verdict
            );
        };
        // `[conf.directive.check]`: a `fail(CODE)` expectation matches exactly.
        let expected = pinned_code(case.check.as_ref())
            .unwrap_or_else(|| panic!("{}: a lex-ledger file must pin a code", case.path));
        assert_eq!(code, expected, "{}", case.path);
        seen.insert(case.path.clone(), code.clone());
    }

    // The `[gram.amb]` counter-examples are the reason this rung exists; if a
    // pin bump drops one, the loss should be loud. E0210 joined at 8b04edf:
    // the §3.3 receiver ruling's detached-moded-receiver error, and the first
    // *spec-pinned* E02xx code (the rest of that family is implementation
    // choice per `[mem.codes]`). E0201 joined with wolf-lang's s88, which
    // gave the corpus `grammar/range_bare.lu` — a range with no endpoint —
    // and so took that code out of this implementation's UNPINNED_CODES.
    // E0211 joined at addcd7f (is28): `grammar/index_origin_misplaced.lu`
    // pins the D61 position rule — a `#![…]` below the first declaration
    // is refused by position (`[gram.attr.index]`); it sorts first because
    // the map is path-ordered.
    // The let_group pair joined at c88ab64 (is30): D63's two refusals-by-
    // name — the bare-tuple spelling (`let a, b = 1, 2`) and the
    // one-initializer-for-several-names spelling (`var i, c = 0`) — both
    // E0201 at the token where the group's grammar breaks, exactly the
    // parse this machine has answered since is28's D63 rider.
    // The comma trio joined at e6cf24e (is32): D67's tightening
    // (wolf-lang#190) makes the separator required throughout the pattern
    // family — `Point { x .. }`, `Point { x y }` and `(a b)` are E0201 at
    // the token where the list breaks. All three answered at first sight:
    // this parser's letter was the measured one, and the compiler's
    // comma-less acceptance was the accident the clause closed.
    // The comma PAIR joined at 2bfbe5e (is33): D69 carries D67's method into
    // the struct-LITERAL and closure-parameter loops
    // (`grammar/struct_literal_no_separator.lu`,
    // `grammar/closure_params_no_separator.lu`), and this parser answered
    // both at first sight and at the same span — `Point { x: 1 y: 2 }`
    // reports at `y` (18:26) and `fn(a b)` at `b` (14:18), the token the
    // missing comma should precede, which is byte-for-byte where s132
    // measured wolfc pointing.
    assert_eq!(
        seen.values().cloned().collect::<Vec<_>>(),
        vec![
            "E0201", "E0211", "E0201", "E0201", "E0001", "E0201", "E0210", "E0002", "E0201",
            "E0201", "E0201", "E0006", "E0201", "E0008"
        ],
        "the pinned grammar-tier codes changed: {seen:?}"
    );
}

#[test]
fn the_spans_of_pinned_failures_point_at_the_offending_source() {
    // Codes travel with spans (`[proto.record.diag]`, `[proto.cmp.phase]`), so
    // a code that is right at the wrong offset is still a divergence. Each
    // assertion below names the source text the span must cover.
    let expectations: &[(&str, &str)] = &[
        ("grammar/newline_leading.lu", "+"),
        ("grammar/semicolon.lu", ";"),
        // The amended §9 reservation pins E0006's primary span to the opening
        // `{` — not the whole literal, which is what is01 reported.
        ("grammar/structlit_cond.lu", "{"),
        ("grammar/when_reserved.lu", "when"),
    ];
    for (relative, text) in expectations {
        let source = std::fs::read(upstream().join("corpus").join(relative)).expect("readable");
        let observation = frontend::observe(&source, Some(Phase::Parse));
        let [start, end] = observation.diagnostics[0].span;
        let slice = std::str::from_utf8(&source[start as usize..end as usize]).expect("utf-8");
        assert_eq!(slice, *text, "{relative}");
    }
}

#[test]
fn every_parseable_file_resolves_under_sema_lite() {
    // `resolve` is this implementation's sema-lite rung (see `frontend`'s
    // ladder mapping). It takes signatures at face value and checks nothing a
    // type checker would, so *every* file that parses must also resolve —
    // with two carve-outs: since 0.1.2 the rung owns E0410 (`let`
    // reassignment, issue #8), since 0.1.4 it owns E1007 (the X1
    // call-site mode law, issue #15 — running the disagreement computes a
    // wrong answer, and `[conf.trap.map]` gives it no dynamic meaning), and
    // since 0.1.5 it owns the pin-f0da6e6 tier statics (issue #18):
    // E1301/E1302 (the unsafe ring and its signature boundary — DIV-2026-012
    // while it was open; the [proto.cmp.rung] closure at 0.1.7 emptied the
    // filed list, so the two files assert here directly now), E0805 (the cast
    // matrix's bool column), E0411 (`s[i]`), E0412/E0413 (format specs,
    // comptime-known at the literal). Since 0.1.6 (the 13b811f re-pin,
    // issue #19) it owns E0004 (`1.e5` — `int` has no member `e5`) and the
    // E11xx capture law: E1101 (a task writes to a captured name), E1102
    // (a visibly unsendable channel payload), E1103 (nested `when`). Since
    // 0.1.8 (the 3d5cee6 re-pin) it owns E0809 (s71: an `else` handler
    // pattern must cover the operand's whole row). Since is19 it owns E0812
    // (explicit generic application arity, wolf-lang#111 — both counts are
    // syntax, `lint::Walk::explicit_apply`). A file
    // Since is28 it owns E0813 (the D61 origin-marker validation,
    // `[gram.attr.index]` — bad args, duplicates, unknown inner attributes,
    // refused by name). A file
    // that *pins* one of these codes fails here exactly as the corpus says. Any other resolve failure
    // would mean the module machinery broke, not that a program is
    // ill-typed.
    for case in cases() {
        if case.ledger_phase.is_some_and(|p| p < Phase::Parse)
            || is_filed_divergence(&case.path)
            || is_member_of_filed_module(&case)
        {
            continue;
        }
        let observation = frontend::observe(&case.source, Some(Phase::Resolve));
        if is_byte_domain_case(&case) {
            assert_eq!(
                observation.verdict,
                Verdict::Fail("E0401".to_owned()),
                "{}",
                case.path
            );
            assert_eq!(observation.phase_reached, Phase::Resolve, "{}", case.path);
            continue;
        }
        if let Some(
            code @ ("E0410" | "E1007" | "E0805" | "E0411" | "E0412" | "E0413" | "E0004" | "E0809"
            | "E0812" | "E0813" | "E1101" | "E1102" | "E1103" | "E1301" | "E1302"),
        ) = pinned_code(case.check.as_ref())
        {
            assert_eq!(
                observation.verdict,
                Verdict::Fail(code.to_owned()),
                "{}",
                case.path
            );
            assert_eq!(observation.phase_reached, Phase::Resolve, "{}", case.path);
            continue;
        }
        // E0414 — what `main` may return (wolf-lang#106) — is a DECLARATION
        // fact, and since wolf-interp#57 this implementation decides it on
        // the admission ladder rather than from the value `main` handed back.
        // So the resolve rung completes and then declines, which is the
        // accept-set boundary `[proto.record.unsupported]` exists for: the
        // code itself is wolfc's typecheck rung, which this machine never
        // performs. Before the move the program RAN first and printed.
        if pinned_code(case.check.as_ref()) == Some("E0414") {
            assert_eq!(observation.verdict, Verdict::Unsupported, "{}", case.path);
            assert_eq!(observation.phase_reached, Phase::Resolve, "{}", case.path);
            continue;
        }
        assert_eq!(observation.verdict, Verdict::Pass, "{}", case.path);
        assert_eq!(observation.phase_reached, Phase::Resolve, "{}", case.path);
        assert!(observation.diagnostics.is_empty(), "{}", case.path);
    }
}

#[test]
fn the_static_rungs_this_implementation_does_not_perform_are_declared() {
    // `[proto.record.phase]`: never claim the incomplete phase. `typecheck`,
    // `mem` and `wir` are the compiler's half of the split — asked for them,
    // this implementation reports the deepest rung it *did* complete and says
    // `unsupported`, which is what keeps the conservatism ledger truthful.
    // A file the resolve rung itself rejects (E0410, E1007, and the 0.1.5
    // tier statics) never gets that far: it fails at resolve whatever
    // deeper rung was requested.
    for case in cases() {
        if case.ledger_phase.is_some_and(|p| p < Phase::Parse)
            || is_filed_divergence(&case.path)
            || is_member_of_filed_module(&case)
        {
            continue;
        }
        for rung in [Phase::Typecheck, Phase::Mem, Phase::Wir] {
            let observation = frontend::observe(&case.source, Some(rung));
            if is_byte_domain_case(&case) {
                assert_eq!(
                    observation.verdict,
                    Verdict::Fail("E0401".to_owned()),
                    "{}",
                    case.path
                );
                assert_eq!(observation.phase_reached, Phase::Resolve, "{}", case.path);
                continue;
            }
            if let Some(
                code @ ("E0410" | "E1007" | "E0805" | "E0411" | "E0412" | "E0413" | "E0004"
                | "E0809" | "E0812" | "E0813" | "E1101" | "E1102" | "E1103" | "E1301"
                | "E1302"),
            ) = pinned_code(case.check.as_ref())
            {
                assert_eq!(
                    observation.verdict,
                    Verdict::Fail(code.to_owned()),
                    "{}",
                    case.path
                );
                assert_eq!(observation.phase_reached, Phase::Resolve, "{}", case.path);
                continue;
            }
            assert_eq!(observation.verdict, Verdict::Unsupported, "{}", case.path);
            assert_eq!(observation.phase_reached, Phase::Resolve, "{}", case.path);
            assert!(observation.diagnostics.is_empty(), "{}", case.path);
        }
    }
}

#[test]
fn the_ambiguity_annex_pairs_read_the_way_section_eight_says() {
    // §8 documents each ambiguity class with paired files: the accepted reading
    // and, where one exists, the counter-example. Both halves of every pair are
    // exercised here by name, because the *pairing* is the claim.
    let pairs: &[(&str, &[&str], &[&str])] = &[
        (
            "gram.amb.brackets",
            &[
                "grammar/brackets_index.lu",
                "grammar/brackets_generic_call.lu",
            ],
            &[],
        ),
        (
            "gram.amb.intdot",
            &["grammar/intdot_member.lu", "grammar/intdot_range.lu"],
            &[],
        ),
        (
            "gram.amb.fmtcolon",
            &["grammar/interp_fmtcolon.lu", "grammar/interp_nested.lu"],
            &[],
        ),
        (
            "gram.amb.else",
            &["grammar/else_default.lu", "grammar/else_chain.lu"],
            &[],
        ),
        (
            "gram.amb.bang",
            &["grammar/bang_not.lu", "grammar/bang_errunion.lu"],
            &[],
        ),
        ("gram.amb.closure", &["grammar/closure_extent.lu"], &[]),
        (
            "gram.amb.structlit",
            &["grammar/structlit_paren.lu"],
            &["grammar/structlit_cond.lu"],
        ),
        ("gram.amb.when", &[], &["grammar/when_reserved.lu"]),
        (
            "gram.amb.newline",
            &["grammar/newline_trailing.lu"],
            &["grammar/newline_leading.lu"],
        ),
    ];

    for (anchor, accepted, rejected) in pairs {
        for relative in *accepted {
            let source = std::fs::read(upstream().join("corpus").join(relative)).expect("readable");
            let observation = frontend::observe(&source, Some(Phase::Parse));
            assert_eq!(
                observation.verdict,
                Verdict::Pass,
                "[{anchor}] {relative} must parse: {:?}",
                observation.detail
            );
        }
        for relative in *rejected {
            let source = std::fs::read(upstream().join("corpus").join(relative)).expect("readable");
            let observation = frontend::observe(&source, Some(Phase::Parse));
            assert!(
                matches!(observation.verdict, Verdict::Fail(_)),
                "[{anchor}] {relative} must be rejected"
            );
        }
    }
}

#[test]
fn the_corpus_is_the_only_source_of_pinned_codes() {
    // Every code this implementation invents is published in `diag::UNPINNED_CODES`;
    // every code the corpus pins must NOT be in that table. If they ever
    // overlap, one of the two lists is lying.
    let pinned: Vec<String> = cases()
        .iter()
        .filter_map(|c| pinned_code(c.check.as_ref()).map(ToOwned::to_owned))
        .collect();
    for (code, _, _) in wolf_interp::diag::UNPINNED_CODES {
        assert!(
            !pinned.contains(&(*code).to_owned()),
            "{code} is pinned by the corpus and must not be listed as an implementation choice"
        );
    }
}

#[test]
fn the_parser_is_deterministic_over_the_corpus() {
    // Two runs, same verdict, same span. Cheap, and it catches the entire class
    // of bugs where a parse depends on allocator or hash iteration order.
    for case in cases() {
        let Ok(text) = std::str::from_utf8(&case.source) else {
            continue;
        };
        let first = parse::parse_source(text).map(|p| p.unit);
        let second = parse::parse_source(text).map(|p| p.unit);
        assert!(first == second, "{} parsed differently twice", case.path);
    }
}

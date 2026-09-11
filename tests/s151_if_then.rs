//! is45 — `[gram.expr.if]`, the two spellings of `if` (wolf-lang s151,
//! #307; wolf-interp#90). The twelve witnesses under `tests/s151/` are
//! wolf-lang's `corpus/grammar/if_then_*.lu` at trunk `f608487`, byte for
//! byte — the ruling landed AFTER the v0.2.10 release this repo is pinned
//! to, so the shared corpus cannot carry them until the next pin. They are
//! kept in-repo the way `tests/d62/` kept D62's, judged against their own
//! headers in the corpus dialect, and they retire from here when the pin
//! that carries them lands and `run_corpus.rs` takes them over.
//!
//! Nine run, and their stdout is the pinned one. Three refuse by E0201 at
//! the parse rung — and at the compiler's own spans: `wolf conform-run` at
//! `f608487` answers `[246,249]` (`let` in a bare branch), `[294,296]` (the
//! `29` after a condition with neither `{` nor `then`) and `[282,283]` (the
//! `{` of a braced `else` after a bare `then`), measured 2026-09-11. Under
//! lupin 0.1.32 every one of the nine stopped at `then`.

use std::path::{Path, PathBuf};

use wolf_interp::directive::{self, Check};
use wolf_interp::ledger::{self, Judgement};
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;

const RUNNING: [&str; 9] = [
    "if_then_let.lu",
    "if_then_arm.lu",
    "if_then_stmt.lu",
    "if_then_chain.lu",
    "if_then_paren_default.lu",
    "if_then_block.lu",
    "if_then_ident.lu",
    "if_then_member.lu",
    "if_then_width.lu",
];

/// The three refusals, with the span the compiler answers at `f608487`.
const REFUSED: [(&str, (usize, usize), &str); 3] = [
    ("if_then_let_body.lu", (246, 249), "let"),
    ("if_then_missing.lu", (294, 296), "29"),
    ("if_then_mixed.lu", (282, 283), "{"),
];

fn witness(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("s151")
        .join(name)
}

struct Observed {
    record: wolf_interp::protocol::ObservationRecord,
    stdout: String,
    check: Check,
    source: String,
}

fn observe(name: &str) -> Observed {
    let path = witness(name);
    let bytes = std::fs::read(&path).expect("readable");
    let source = String::from_utf8(bytes.clone()).expect("utf-8");
    let header =
        directive::parse_header(&source).expect("the witness header parses in the corpus dialect");
    let (record, observed) = wolf_interp::observe_record(&path, &bytes, None);
    Observed {
        record,
        stdout: observed.stdout,
        check: header.check.expect("an entry pins a check"),
        source,
    }
}

#[test]
fn the_nine_positive_witnesses_run_to_the_pinned_stdout() {
    for name in RUNNING {
        let observed = observe(name);
        assert_eq!(observed.record.verdict, Verdict::Exit(0), "{name}");
        assert_eq!(observed.record.phase_reached, Phase::Run, "{name}");
        let Check::Run {
            stdout: Some(pinned),
            ..
        } = &observed.check
        else {
            panic!("{name} pins its stdout");
        };
        assert_eq!(&observed.stdout, pinned, "{name}");
        assert!(
            matches!(
                ledger::judge(&observed.check, &observed.record, &observed.stdout),
                Judgement::Match(_)
            ),
            "{name} matches its own pin"
        );
    }
}

#[test]
fn the_three_refusals_are_e0201_at_the_compilers_span() {
    for (name, (start, end), at) in REFUSED {
        let observed = observe(name);
        assert_eq!(observed.record.verdict, Verdict::Fail("E0201".to_owned()), "{name}");
        assert_eq!(observed.record.phase_reached, Phase::Parse, "{name}");
        let diag = observed.record.diagnostics.first().expect("one diagnostic");
        assert_eq!(diag.span, [start as u64, end as u64], "{name}");
        assert_eq!(&observed.source[start..end], at, "{name}");
        assert!(
            matches!(
                ledger::judge(&observed.check, &observed.record, &observed.stdout),
                Judgement::Match(_)
            ),
            "{name} matches its own pin"
        );
    }
}

#[test]
fn the_twelve_are_the_clauses_own_and_nothing_else_lives_here() {
    let mut names: Vec<String> = std::fs::read_dir(witness(""))
        .expect("tests/s151")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut expected: Vec<String> = RUNNING
        .iter()
        .map(|n| (*n).to_owned())
        .chain(REFUSED.iter().map(|(n, _, _)| (*n).to_owned()))
        .collect();
    expected.sort();
    assert_eq!(names, expected);
    for name in &names {
        let source = std::fs::read_to_string(witness(name)).expect("readable");
        assert!(source.contains("//! conforms: gram.expr.if"), "{name}");
    }
}

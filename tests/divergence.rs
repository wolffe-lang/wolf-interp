//! The divergence-filing rule, exercised end to end.
//!
//! The compiler is not on this machine and never will be in CI — the two tracks
//! meet through the protocol, not through a build. So the pipeline is proved
//! with a **seeded** counterparty: a record shaped exactly as `spec/06` §2
//! specifies, standing in for the other implementation, run through the real
//! comparison (`[proto.cmp]`) to produce the real report line
//! (`[proto.harness.differ]`).
//!
//! That is the acceptance criterion's own instruction — "prove the pipeline,
//! not the luck" — and it is the honest one: a filing rule that has only ever
//! been described is a filing rule that does not work.
//!
//! The standing rule this serves is in CONTRIBUTING.md: **any input where this
//! parser and the compiler disagree becomes a wolf-lang spec issue with both
//! readings attached.** The two parsers never reconcile privately.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wolf_interp::compare::{self, Class};
use wolf_interp::phase::Phase;
use wolf_interp::protocol::{Diagnostic, ObservationRecord, PROTOCOL_VERSION, Verdict};

fn corpus(relative: &str) -> (PathBuf, Vec<u8>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
        .join(relative);
    let bytes = std::fs::read(&path).expect("readable");
    (path, bytes)
}

/// A record from the seeded counterparty. Everything except the fields under
/// test is copied from ours, because those fields are not what is being
/// compared (`[proto.cmp]` names exactly what is).
fn counterparty(ours: &ObservationRecord) -> ObservationRecord {
    ObservationRecord {
        protocol: PROTOCOL_VERSION,
        impl_name: "seeded-counterparty".to_owned(),
        impl_version: "0.0.0".to_owned(),
        commit: "0000000".to_owned(),
        file: ours.file.clone(),
        phase_reached: ours.phase_reached,
        seeded: false,
        diagnostics: ours.diagnostics.clone(),
        verdict: ours.verdict.clone(),
        stdout_sha256: None,
        stdout_inline: None,
        extensions: BTreeMap::new(),
    }
}

#[test]
fn identical_records_file_nothing() {
    let (path, source) = corpus("wordcount.lu");
    let (ours, _) = wolf_interp::observe_record(&path, &source, Some(Phase::Parse));
    let theirs = counterparty(&ours);
    assert_eq!(compare::compare(&ours, &theirs), None);
}

#[test]
fn a_seeded_code_disagreement_files_a_span_or_code_divergence() {
    // The shape the sprint expects to see most often between two independent
    // frontends: same rejection, different code. Our E0006 against a
    // counterparty that reserved a different number for the same clause.
    let (path, source) = corpus("grammar/structlit_cond.lu");
    let (ours, _) = wolf_interp::observe_record(&path, &source, Some(Phase::Parse));
    assert_eq!(ours.verdict, Verdict::Fail("E0006".to_owned()));

    let mut theirs = counterparty(&ours);
    theirs.verdict = Verdict::Fail("E0006".to_owned());
    theirs.diagnostics = vec![Diagnostic {
        code: "E0006".to_owned(),
        // Same code, but pointing at the `{` alone rather than at the whole
        // would-be literal — the span choice `parse::CHOICES` records.
        span: [ours.diagnostics[0].span[0] + 8, ours.diagnostics[0].span[1]],
        severity: "error".to_owned(),
    }];

    let divergence = compare::compare(&ours, &theirs).expect("a span disagreement is a divergence");
    assert_eq!(divergence.class, Class::SpanOrCodeMismatch);
    assert!(divergence.file.ends_with("structlit_cond.lu"));

    // And it renders as the JSONL line `[proto.harness.differ]` specifies:
    // one object per file, `{file, class, a, b}`.
    let line = divergence.to_json();
    assert!(line["file"].is_string());
    assert_eq!(line["class"], "span-or-code");
    assert!(line["a"].is_string() && line["b"].is_string());
    // Our own commentary rides an `x-` key, so it participates in equality only
    // when both sides carry it (`[proto.record.ext]`).
    assert!(line["x-detail"].is_string());
    println!("{}", serde_json::to_string(&line).expect("serializes"));
}

#[test]
fn a_seeded_verdict_disagreement_outranks_a_span_one() {
    // A counterparty that *accepts* what we reject is the more serious finding,
    // and the report must order it above a mere span difference
    // (`[proto.cmp.severity]`).
    let (path, source) = corpus("grammar/when_reserved.lu");
    let (ours, _) = wolf_interp::observe_record(&path, &source, Some(Phase::Parse));
    let mut theirs = counterparty(&ours);
    theirs.verdict = Verdict::Pass;
    theirs.diagnostics.clear();

    let divergence = compare::compare(&ours, &theirs).expect("acceptance vs rejection diverges");
    assert_eq!(divergence.class, Class::VerdictMismatch);
    assert!(Class::VerdictMismatch < Class::SpanOrCodeMismatch);
}

#[test]
fn our_unsupported_verdicts_never_file_against_a_deeper_counterparty() {
    // The whole corpus, compared against a counterparty that runs everything.
    // Not one line should be filed: `unsupported` is a scope gap, tracked in
    // the conservatism ledger (`[proto.record.unsupported]`), never a
    // divergence. This is the property that lets is01 ship against a compiler
    // that is six phases ahead.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus");
    let report = wolf_interp::corpus::walk(&root, None).expect("walkable");
    let mut filed = Vec::new();
    for file in &report.files {
        let path = root.join(&file.path);
        let source = std::fs::read(&path).expect("readable");
        // No `--phase`: we go as deep as we can, which is `parse`/unsupported
        // for everything well-formed.
        let (ours, _) = wolf_interp::observe_record(&path, &source, None);
        if ours.verdict != Verdict::Unsupported {
            continue;
        }
        let mut theirs = counterparty(&ours);
        theirs.phase_reached = Phase::Run;
        theirs.verdict = Verdict::Exit(0);
        theirs.diagnostics.clear();
        if let Some(divergence) = compare::compare(&ours, &theirs) {
            filed.push(format!("  {}: {}", file.path, divergence.class));
        }
    }
    assert!(
        filed.is_empty(),
        "unsupported must never file a divergence:\n{}",
        filed.join("\n")
    );
}

#[test]
fn the_whole_corpus_agrees_with_a_counterparty_that_mirrors_us() {
    // The negative control for the test above: when the counterparty says what
    // we say, the report is empty — which is what a green differ run looks
    // like (`[proto.harness.differ]`: "empty report = green").
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus");
    let report = wolf_interp::corpus::walk(&root, None).expect("walkable");
    for file in &report.files {
        let path = root.join(&file.path);
        let source = std::fs::read(&path).expect("readable");
        for rung in [Some(Phase::Lex), Some(Phase::Parse)] {
            let (ours, _) = wolf_interp::observe_record(&path, &source, rung);
            let theirs = counterparty(&ours);
            assert_eq!(compare::compare(&ours, &theirs), None, "{}", file.path);
        }
    }
}

//! is38 — DIV-2026-021's locus, gated.
//!
//! `let a, b = 1, 2` is `fail(E0201)@parse` on both machines and they point
//! ten bytes apart: this parser at the comma, the counterparty at the end of
//! the initializer list. `[gram.item.let]` says what a D63 let-group IS and
//! what the bare-tuple shape is not; it does not say where refusing it
//! reports, so the row is the spec's (wolf-lang#228) and neither
//! implementation may move to the other's locus before the clause speaks
//! (CONTRIBUTING, the standing divergence-filing rule).
//!
//! What a row like that needs meanwhile is a GATE, and it had none. The
//! corpus directive is `check: fail(E0201)`: the walk compares CODES, so
//! nothing in `cargo test` ever asserted that this parser points at the
//! comma at all. A drift to byte 374 would close the divergence silently and
//! turn `docs/divergence-log.md`'s entry into a lie — the failure mode this
//! repository keeps meeting, one layer down from wolf-lang#177's.
//!
//! So: this machine's half is pinned here, hermetically, against the pinned
//! corpus file; the counterparty's half is measured beside it when a
//! counterparty binary exists, and SKIPs loudly when it does not, exactly as
//! the differential lane does.

use std::process::Command;

use wolf_interp::differ::{self, Counterparty};
use wolf_interp::frontend;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;

/// The witness, and the two loci as `docs/divergence-log.md` records them.
const WITNESS: &str = "corpus/grammar/let_group_bare_tuple.lu";
const OURS: [usize; 2] = [364, 365];
const THEIRS: [usize; 2] = [374, 375];

fn witness() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join(WITNESS)
}

fn source() -> String {
    std::fs::read_to_string(witness()).expect("the pinned witness is readable")
}

#[test]
fn this_machine_refuses_the_bare_tuple_at_the_comma() {
    // The code AND the span, with the byte sliced back out of the source:
    // "the right code" is half a claim, and the half this row is about is
    // the other one.
    let text = source();
    let observed = frontend::observe(text.as_bytes(), Some(Phase::Parse));
    assert_eq!(observed.verdict, Verdict::Fail("E0201".to_owned()));
    let diag = observed.detail.expect("a diagnostic carries the fail");
    assert_eq!(diag.code, "E0201");
    assert_eq!([diag.span.start, diag.span.end], OURS);
    assert_eq!(
        &text[diag.span.start..diag.span.end],
        ",",
        "the locus this machine defends is the COMMA — the first byte at \
         which the input stops being a legal `let`"
    );
}

#[test]
fn the_two_loci_are_the_ones_the_ledger_records() {
    // The waiver's own numbers, checked against the measurement rather than
    // trusted. `differ::filed` is the machine half of the ledger, so the
    // summary is where a reader meets the finding.
    let (id, summary) = differ::filed(WITNESS).expect("the row is filed");
    assert_eq!(id, "DIV-2026-021");
    assert!(summary.contains("364"), "{summary}");
    assert!(summary.contains("374"), "{summary}");
    assert!(summary.contains("wolf-lang#228"), "{summary}");
    assert_ne!(OURS, THEIRS, "a locus row needs two loci");
}

#[test]
fn the_counterparty_still_points_ten_bytes_away() {
    // The other half of the gate: the divergence is a claim about the
    // COUNTERPARTY, so it is re-measured whenever one is present. If this
    // fails because the two now agree, that is the good news — retire
    // DIV-2026-021 and `differ::FILED_DIVERGENCES` with it (which
    // `lupin diff-run` will also demand, per `differ::retired_waivers`).
    let counterparty = match differ::detect_counterparty(None) {
        Counterparty::Found(path) => path,
        Counterparty::Missing { tried } => {
            eprint!("{}", differ::skip_notice(&tried));
            return;
        }
    };
    let output = Command::new(&counterparty)
        .arg("conform-run")
        .arg(witness())
        .arg("--json")
        .arg("--checked")
        .output()
        .expect("the counterparty runs");
    let record: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("a record");
    assert_eq!(record["verdict"], "fail(E0201)", "{record}");
    assert_eq!(record["phase_reached"], "parse", "{record}");
    let span = &record["diagnostics"][0]["span"];
    assert_eq!(
        [
            usize::try_from(span[0].as_u64().expect("a start")).expect("fits"),
            usize::try_from(span[1].as_u64().expect("an end")).expect("fits"),
        ],
        THEIRS,
        "the counterparty's locus moved; re-triage DIV-2026-021"
    );
}

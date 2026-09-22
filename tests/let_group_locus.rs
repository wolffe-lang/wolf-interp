//! is38 — the let-group locus, gated. DIV-2026-021 **RETIRED at is53**.
//!
//! `let a, b = 1, 2` is `fail(E0201)@parse` on both machines. For eight
//! releases they pointed ten bytes apart — this parser at the comma, the
//! counterparty at the end of the initializer list — because
//! `[gram.item.let]` says what a D63 let-group IS and what the bare-tuple
//! shape is not, and did not say where refusing it reports. wolf-lang#228
//! ruled, s163 moved the compiler onto the comma, and at the `2e4ca769` pin
//! (wolf-lang v0.2.15) the two loci are one byte range.
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
/// Both, now: `[511, 512]` at the `2e4ca769` pin. The numbers moved twice at
/// once — the corpus file's header grew by 147 bytes in the same pin bump, so
/// `364 -> 511` is the file and `374 -> 511` is the ruling.
const OURS: [usize; 2] = [511, 512];
const THEIRS: [usize; 2] = [511, 512];

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
fn the_waiver_is_retired_and_the_two_loci_are_one() {
    // The other direction of the same gate. A waiver that outlives its
    // divergence is a green report that means nothing (wolf-lang#177's lesson
    // in this shape), so the retirement is asserted rather than assumed: no
    // filing id for this witness, and the two recorded loci are equal.
    assert_eq!(
        differ::filed(WITNESS),
        None,
        "DIV-2026-021 is retired at the 2e4ca769 pin"
    );
    assert_eq!(OURS, THEIRS, "the locus row closed because the loci met");
}

#[test]
fn the_counterparty_points_at_the_same_comma() {
    // The other half of the gate: the agreement is a claim about the
    // COUNTERPARTY, so it is re-measured whenever one is present, and a
    // resolved row needs that as much as an open one did. If this fails
    // because the loci part again, the row comes back — file it, do not move
    // this machine onto the counterparty's byte.
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
        "the counterparty's locus moved off the comma; re-file the locus row"
    );
}

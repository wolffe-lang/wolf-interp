//! is28 — the D62 rider: `+`/`+=` on two `str` operands is the language,
//! and this machine already conforms (the ruling adopted the interpreter's
//! behavior). Nothing to build; these are the WITNESSES — the legal chain
//! runs, and the three refused mixes (`str + int`, `str + char`,
//! `int + str`) stay refusals by name — kept in-repo (`tests/d62/`, corpus
//! directive dialect, upstream-ready as-is) so the compiler half
//! (wolf-lang#172) has its differential counterpart waiting. wolf-interp#51
//! closes from the compiler's side; the shared-corpus comparison lands at
//! the pin bump that carries the wolf-lang witnesses.

use std::path::{Path, PathBuf};

use wolf_interp::directive::{self, Check};
use wolf_interp::ledger::{self, Judgement};
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;

fn witness(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("d62")
        .join(name)
}

fn observe(name: &str) -> (wolf_interp::protocol::ObservationRecord, String, Check) {
    let path = witness(name);
    let source = std::fs::read(&path).expect("readable");
    let header = directive::parse_header(std::str::from_utf8(&source).expect("utf-8"))
        .expect("the witness header parses in the corpus dialect");
    let (record, observed) = wolf_interp::observe_record(&path, &source, None);
    (
        record,
        observed.stdout,
        header.check.expect("an entry pins a check"),
    )
}

#[test]
fn the_legal_chain_runs_and_means_the_interpolation() {
    // `s + u` == `"{s}{u}"`, left-associative chains, `s += u` is
    // `s = s + u` — one program, judged against its own pin.
    let (record, stdout, check) = observe("str_append_chain.lu");
    assert_eq!(record.verdict, Verdict::Exit(0));
    assert_eq!(record.phase_reached, Phase::Run);
    assert_eq!(stdout, "abcdef abcdef xy4\n");
    assert!(
        matches!(ledger::judge(&check, &record, &stdout), Judgement::Match(_)),
        "the chain witness matches its own pin"
    );
}

#[test]
fn the_three_mixes_stay_refused_by_name() {
    // D62: mixed operands stay E0409 on both machines. This machine's
    // refusal is dynamic and by name — `unsupported`, the reason naming
    // both operand kinds — which the ledger classes as the compiler's
    // static-refusal conservatism, never a silent run.
    for (name, lhs, rhs) in [
        ("str_append_mix_int.lu", "str", "i32"),
        ("str_append_mix_char.lu", "str", "char"),
        ("str_append_mix_int_lhs.lu", "i32", "str"),
    ] {
        let (record, stdout, check) = observe(name);
        assert_eq!(record.verdict, Verdict::Unsupported, "{name}");
        let reason = record
            .extensions
            .get("x-unsupported")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        assert!(
            reason.contains(&format!("`+` is not defined on {lhs} and {rhs}")),
            "{name}: the refusal names the operands: {reason}"
        );
        let judgement = ledger::judge(&check, &record, &stdout);
        assert!(
            !judgement.is_mismatch(),
            "{name}: a named refusal against the compiler's fail(E0409) pin is \
             conservatism, not a finding: {judgement:?}"
        );
    }
}

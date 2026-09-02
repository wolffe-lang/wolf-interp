//! The corpus-wide differential runner — is05, the sprint the two-track
//! design exists for.
//!
//! Two independent implementations of one spec, mechanically compared through
//! the only artifact they share: the spec/06 observation record. This module
//! owns the *deep* comparison — the pre-M1, phase-aware extension of
//! [`crate::compare`] — plus the conservatism ledger, the severity ordering,
//! and the filing template the triage workflow starts from.
//!
//! # Comparing across unequal depth (the pre-M1 posture)
//!
//! The compiler answers `conform-run` truthfully through `typecheck` today
//! (s17); this interpreter completes `lex, parse, resolve` statically and
//! `run` dynamically (`crate::frontend`'s ladder mapping). A naive record
//! comparison would call every corpus file a `phase_reached` mismatch. The
//! honest comparison instead keys on **what each record claims, rung by
//! rung**, per `[proto.record.phase]`:
//!
//! - `fail(CODE)` at phase *p* claims: every performed rung below *p* passed,
//!   and *p* rejected with that code and span.
//! - `pass`/`unsupported` at *p* claims: every performed rung through *p*
//!   passed. (`unsupported` additionally lands in the conservatism ledger —
//!   `[proto.record.unsupported]` — and is never a divergence by itself.)
//! - `exit/trap/ub` claims: every performed *static* rung passed and the run
//!   completed with that outcome.
//!
//! The two sides are compared at the **deepest rung both have a claim
//! about**. A claim gap — the compiler rejecting at `typecheck`, a rung this
//! machine never performs — is the accept-set boundary is02 §1 drew, and it
//! goes to the **static-conservatism ledger**: expected by construction,
//! tracked, never a CI failure by itself. A disagreement at a mutually
//! performed rung is a real divergence and files a bug.
//!
//! "Performed rungs" comes from each implementation's *documented ladder*,
//! never from error-code ranges: ours is `crate::frontend`'s mapping shipped
//! in this binary; the compiler's pipeline is contiguous by construction, so
//! its record's `phase_reached` bounds its claims exactly. The compiler-track
//! finding is05 inherited: comparison keys on the protocol's `phase_reached`
//! plus verdict, and the E-code families are diagnostics taxonomy, never
//! comparison heuristics.
//!
//! # UB and the soundness-candidate class
//!
//! `[proto.record.ub]`: one side reporting `ub(…)` where the other runs
//! defined is the highest-severity class. Pre-M1 the compiler cannot run, so
//! the cross-implementation form of is04's "zero UB in safe code" is: an
//! interpreter `ub(*)` verdict on an **unsafe-free** program that the
//! compiler accepts as far as it checks is a soundness candidate regardless.
//! A `ub(*)` on a program that *required* `unsafe` compares only when both
//! sides actually run it.
//!
//! # Legitimate binary acquisition (integrator ruling, is05)
//!
//! Building and executing the pinned compiler is legitimate: the counterparty
//! **binary** is data the differ consumes through the protocol, exactly like
//! the corpus. Reading or studying its *source* remains forbidden — the
//! independence doctrine is about shared code and shared blind spots, not
//! about refusing to run the thing we compare against. Locally,
//! `cargo build -p wolf_driver` inside `upstream/` produces `wolf`; in CI the
//! vendored snapshot has no `crates/` and the private submodule cannot clone,
//! so the differential lane detects the absence, says so loudly, and SKIPs —
//! `--require-counterparty` turns that skip into a hard failure for
//! environments that promise a compiler.

use std::fmt;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::phase::Phase;
use crate::protocol::{Diagnostic, ObservationRecord, Verdict};
use crate::schema;

// ---------------------------------------------------------------------------
// Filed divergences — the machine-readable mirror of docs/divergence-log.md
// ---------------------------------------------------------------------------

/// Divergences that have been triaged and filed, keyed by corpus-relative
/// path. A filed divergence still appears in every report (hiding it would
/// un-file it); it stops *gating* because its entry in
/// `docs/divergence-log.md` carries the triage verdict and the owed fix.
///
/// Every entry is `(file, id, one-line summary)`; the id resolves in the log.
///
/// DIV-2026-010 (the E0410 fail-files) CLOSED at pin `ad6cef7`: s29 moved
/// wolfc's emission to the resolve rung and re-pinned the corpus `phase:`
/// directives resolve → parse, so both sides now reject at `resolve` with
/// the same code and span — the eighth round compares clean and the
/// entries are retired from this table.
///
/// DIV-2026-011 (0.1.4, pin `ad6cef7`): `memory/mode_missing_mut.lu`. Both
/// sides reject with E1007 at the same span ([405,408], the argument); this
/// machine's only static tier is resolve (sema-lite, where the callee's
/// signature is visible — issue #15's fix), while wolfc's emission lives at
/// its mem rung. Rung placement only; the verdicts agree. Routed upstream
/// for a `[proto.cmp]` ruling on same-code-same-span rejections at
/// different rungs across implementations of unequal pipeline depth.
/// DIV-2026-012 (0.1.5, pin `f0da6e6`): the two unsafe-tier fail-files.
/// Issue #18's fix put E1301/E1302 at this machine's resolve rung (sema-lite
/// is its only static tier) with the counterparty's codes and spans, while
/// wolfc's emissions live at its mem rung — the DIV-2026-011 shape again,
/// same open `[proto.cmp]` question, one filing covering both files.
///
/// DIV-2026-013 CLOSED at pin `13b811f` (0.1.6): wolfc's conform-run no
/// longer E0301-rejects its own s38 fs/io files — it reports
/// `unsupported@wir` there, which is never a divergence, so the three
/// entries retired. DIV-2026-014's wiring half closed the same way (wolfc
/// emits E0411/E0412/E0413 now); its residue is the rung question, and the
/// entries stay with updated summaries.
///
/// DIV-2026-015 (0.1.6, pin `13b811f`): the E11xx capture-law fail files
/// plus `intdot_exponent.lu` — issue #19's realignment put E1101/E1102/
/// E1103/E0004 at this machine's resolve rung with the counterparty's codes
/// and spans (byte-identical, observed at the pin), while wolfc's emissions
/// live at its typecheck rung. The DIV-2026-011 `[proto.cmp]` question,
/// fourth filing; one ruling will resolve all four families at once.
///
/// THE RULING LANDED at pin `e94b879` (0.1.7): `[proto.cmp.rung]` — fail
/// parity (code + span) at any shared-ladder rung is agreement, exactly one
/// verdict wide. `compare_deep` implements the clause, the eleven
/// rung-placement divergences (DIV-2026-011/-012/-014/-015) compare clean,
/// and all four families closed in `docs/divergence-log.md` with the clause
/// cited.
///
/// DIV-2026-016 CLOSED at pin `0b4e79c` (0.1.9): wolfgang's conform-run
/// answers `fail(E0809)@typecheck`, same span `[518,523]`, on
/// `rows/negative/handler_uncovered.lu` — the E0806 answer does not
/// reproduce at the release sha or the c09-wave pin (wolf-lang#61's own
/// reproduction attempt had already failed at the s72 trunk; this
/// round's CLEAN build confirms). Codes agree; the rung difference
/// (resolve vs typecheck) is `[proto.cmp.rung]` agreement. The 0.1.8
/// observation is attributed to a wolfgang built at the intermediate
/// first-pin state (`3d5cee6`) or a stale scratch build — an evidence
/// lesson, not a semantics one. The list is empty: every filing in the
/// log's history is resolved.
///
/// DIV-2026-017 FILED at pin `613c3dc` (0.1.10): the first finding from a
/// run-reaching counterparty lane. `lints/raw_interp_braces.lu` prints
/// `{who}` here and `"{who}` on the compiler — the `r` prefix's opening
/// quote survives its raw-literal decode. `[gram.lex.str.raw]` is explicit
/// that `r"…"` carries the bytes between the quotes, the corpus header's own
/// `stdout="{who}"` agrees with this machine, so the spec is clear and the
/// compiler is the defendant (triage case 2). Identical on all three of the
/// counterparty's run-reaching tiers (`--checked`, `--native`, `--release`),
/// which is what makes it a front-end decode bug rather than anything the
/// mid-end did. Filed upstream; the file's `phase: wir` pin — written when
/// BOTH executors had the bug — should advance to `run` with the fix.
/// RE-CONFIRMED at pin `f8dca42` (0.1.11) against a counterparty rebuilt
/// from a deleted `target/`: byte-identical stdout shas to the original
/// filing, on all three tiers. RE-CONFIRMED AGAIN at pin `4e316ad`
/// (0.1.12) — same two shas, same three tiers, and it is now the ONLY
/// divergence any tier reports. wolf-lang#76 stays open across three
/// more compiler sprints (s79, s80, s81).
///
/// DIV-2026-018 (0.1.11) is NOT in this list and cannot be: the list is
/// keyed by corpus file and no corpus file witnesses it. `s[..]` — a bare
/// range with neither endpoint — is `fail(E0201)`@parse here and `exit(0)`
/// on all three counterparty tiers, where `[gram.expr.primary]`'s
/// `range_expr` admits `a..`, `..b` and `a..b` but never a bare `..`. It
/// lives in `docs/divergence-log.md` until a witness exists, as
/// wolf-lang#71's stdout finding did. Filed upstream as wolf-lang#88.
///
/// DIV-2026-017 CLOSED at pin `3befc3e` (0.1.24): wolf-lang#76 is closed and
/// `lints/raw_interp_braces.lu` answers `{who}\n` on `--checked`, `--native`
/// and `--release` alike, byte-identical to this machine. DIV-2026-020
/// CLOSED at the same pin by D71 + s134: the span IS the offending token, so
/// seven of its eight files are byte-identical and the eighth was never a
/// width question — it is DIV-2026-021 now, and it is about where a D63
/// let-group refusal points rather than how wide the pointing is.
pub const FILED_DIVERGENCES: &[(&str, &str, &str)] = &[
    (
        "resolve/broken_sibling/entry.lu",
        "DIV-2026-019",
        "which parse error fires on the unparseable module sibling: the \
         corpus pins the counterparty's fail(E0202) (EOF inside the mangled \
         item) where this machine stops at the first bad token, fail(E0201) \
         at `{` in the parameter list; same rung, span-or-code class — the \
         spec assigns neither code to junk recovery",
    ),
    (
        "grammar/let_group_bare_tuple.lu",
        "DIV-2026-021",
        "where a D63 let-group refusal POINTS: both machines answer E0201 at \
         parse, this one at the first comma (byte 364) and the counterparty \
         at the end of the initializer list (byte 374), which is a locus \
         disagreement and never was the span-WIDTH question D71 settled; \
         `[gram.item.let]` says what the shape is and not where refusing it \
         reports. Filed upstream as wolf-lang#228",
    ),
];

// DIV-2026-017 stood here from is05 to is34 —
// `lints/raw_interp_braces.lu`, the compiler's raw-literal decode keeping
// the `r"` prefix's opening quote (`"{who}` there, `{who}` here). It retires
// at the 3befc3e pin: wolf-lang#76 is CLOSED and all four lanes now answer
// `{who}\n`, measured at this pin on `--checked`, `--native` and `--release`.
// It had already stopped diverging at v0.2.2; the waiver outliving the
// divergence is the thing wolf-lang#177 taught, so it goes.
//
// DIV_2026_020_FILES stood beside it from is34 — the eight E02xx parse
// refusals where both machines agreed on the code and the starting byte and
// differed on the span's WIDTH. D71 ruled the strong form (the span IS the
// offending token), s134 aligned wolfc, and wolf-lang#220's own closing
// comment assigns the waiver's retirement to this lane's next pin bump.
// Seven of the eight are byte-identical here now. The eighth was never a
// width question and is carried above under its own id.

/// The filing id for a corpus file, when its divergence is already filed.
#[must_use]
pub fn filed(file: &str) -> Option<(&'static str, &'static str)> {
    FILED_DIVERGENCES
        .iter()
        .find(|(f, _, _)| file.ends_with(f))
        .map(|(_, id, summary)| (*id, *summary))
}

// ---------------------------------------------------------------------------
// Divergence classes and ledger entries
// ---------------------------------------------------------------------------

/// Divergence classes, descending in severity.
///
/// The first four are `[proto.cmp.severity]`'s, in its order. The last two
/// are runner-level findings the protocol does not classify: a record the
/// schema rejects, and a side that never answered. Both are real findings
/// against the offending *implementation* (not the program), so they gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeepClass {
    /// `ub(…)` against a defined execution (`[proto.record.ub]`).
    SoundnessCandidate,
    /// Verdict disagreement at a mutually performed rung — including the
    /// accept-set divergence (one side rejects what the other accepts).
    Verdict,
    /// Same failure, different first-diagnostic code or span.
    SpanOrCode,
    /// Same exit, different bytes.
    Stdout,
    /// A side emitted something the spec/06 schema rejects.
    Protocol,
    /// A side exceeded the time budget. "A timeout is a verdict, not an
    /// error" — it is compared like one: against anything else it diverges.
    Timeout,
}

impl DeepClass {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DeepClass::SoundnessCandidate => "soundness-candidate",
            DeepClass::Verdict => "verdict",
            DeepClass::SpanOrCode => "span-or-code",
            DeepClass::Stdout => "stdout",
            DeepClass::Protocol => "protocol",
            DeepClass::Timeout => "timeout",
        }
    }
}

impl fmt::Display for DeepClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One divergence, in `[proto.harness.differ]`'s JSONL shape plus our
/// `x-` extensions (detail, comparison rung, filing id when known).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepDivergence {
    pub file: String,
    pub class: DeepClass,
    /// What each side said, rendered `verdict@phase_reached`.
    pub a: String,
    pub b: String,
    /// The rung the comparison fired at.
    pub rung: Option<Phase>,
    /// Why — ours, never compared.
    pub detail: String,
    /// The divergence-log id, when this finding is already filed.
    pub filed: Option<&'static str>,
}

impl DeepDivergence {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "file": self.file,
            "class": self.class.as_str(),
            "a": self.a,
            "b": self.b,
            "x-detail": self.detail,
        });
        if let Some(rung) = self.rung {
            value["x-rung"] = serde_json::Value::from(rung.as_str());
        }
        if let Some(id) = self.filed {
            value["x-filed"] = serde_json::Value::from(id);
        }
        value
    }

    /// The filing template `[proto.cmp.triage]`'s workflow starts from: the
    /// program, both verdicts, the class, and the three suspects with the
    /// spec listed first — the spec document is the defendant first.
    #[must_use]
    pub fn filing(&self) -> String {
        format!(
            "### {file} — {class}\n\
             - a (interp): `{a}`\n\
             - b (counterparty): `{b}`\n\
             - rung: {rung}\n\
             - detail: {detail}\n\
             - triage (pick one, spec first — `[proto.cmp.triage]`):\n\
             - [ ] spec bug: clause silent/ambiguous → clause PR to s04/s05, both implementations follow\n\
             - [ ] compiler bug: spec clear, interpreter matches it → file in wolf-lang + corpus regression\n\
             - [ ] interpreter bug: spec clear, compiler matches it → fix here + corpus regression\n",
            file = self.file,
            class = self.class,
            a = self.a,
            b = self.b,
            rung = self.rung.map_or("-", Phase::as_str),
            detail = self.detail,
        )
    }
}

/// Which implementation a ledger entry is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Interp,
    Counterparty,
}

impl Side {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Interp => "interp",
            Side::Counterparty => "counterparty",
        }
    }
}

/// One conservatism-ledger entry — tracked, reported, never a divergence
/// (`[proto.record.unsupported]`; the accept-set boundary of is02 §1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerEntry {
    /// A side reported `unsupported`: the file is outside its scope, with the
    /// deepest completed rung and (ours) the reason.
    Unsupported {
        file: String,
        side: Side,
        phase: Phase,
        reason: String,
    },
    /// A side rejected at a rung the other does not perform — the accept-set
    /// gap, the false-rejection metric s18–s21 exist to drive down.
    RejectsBeyond {
        file: String,
        side: Side,
        phase: Phase,
        code: String,
    },
    /// This machine produced a run-tier outcome the counterparty cannot yet
    /// check against (no M1). The shrinking of this set *is* M1 arriving.
    RunUnmatched { file: String, verdict: String },
}

impl LedgerEntry {
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            LedgerEntry::Unsupported { .. } => "unsupported",
            LedgerEntry::RejectsBeyond { .. } => "rejects-beyond",
            LedgerEntry::RunUnmatched { .. } => "run-unmatched",
        }
    }

    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            LedgerEntry::Unsupported {
                file,
                side,
                phase,
                reason,
            } => serde_json::json!({
                "file": file, "kind": "unsupported", "side": side.as_str(),
                "phase_reached": phase.as_str(), "reason": reason,
            }),
            LedgerEntry::RejectsBeyond {
                file,
                side,
                phase,
                code,
            } => serde_json::json!({
                "file": file, "kind": "rejects-beyond", "side": side.as_str(),
                "phase": phase.as_str(), "code": code,
            }),
            LedgerEntry::RunUnmatched { file, verdict } => serde_json::json!({
                "file": file, "kind": "run-unmatched", "verdict": verdict,
            }),
        }
    }
}

/// What deep comparison made of one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileComparison {
    pub divergence: Option<DeepDivergence>,
    pub ledger: Vec<LedgerEntry>,
}

// ---------------------------------------------------------------------------
// Rung-by-rung claims
// ---------------------------------------------------------------------------

/// What one record claims about one rung.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Claim {
    /// The rung completed clean.
    Passed,
    /// The rung rejected the program; the first diagnostic travels.
    Rejected(String, Option<Diagnostic>),
    /// The run rung completed with this outcome.
    Ran(Verdict),
    /// The implementation makes no claim here (outside its ladder, beyond its
    /// coverage, or beyond the phase where it stopped).
    Silent,
}

/// The static rungs this interpreter performs — `crate::frontend`'s ladder
/// mapping, shipped in the same binary as the machine it describes.
fn interp_performs(rung: Phase) -> bool {
    matches!(
        rung,
        Phase::Lex | Phase::Parse | Phase::Resolve | Phase::Run
    )
}

/// The compiler's pipeline is contiguous: it performs every static rung it
/// reports having completed or failed, in ladder order.
fn counterparty_performs(_rung: Phase) -> bool {
    true
}

/// The diagnostic that carries a `fail` verdict: the first **error**-severity
/// entry. `[proto.record.warn]` lets warning observations ride `diagnostics`
/// at warning severity, and `[proto.cmp.warn]` owns their comparison — so a
/// counterparty that interleaves lints ahead of its rejection (source order)
/// must not have a lint's span compared against this machine's rejection.
fn first_rejection(record: &ObservationRecord) -> Option<Diagnostic> {
    record
        .diagnostics
        .iter()
        .find(|d| d.severity != "warning")
        .cloned()
}

fn claim(record: &ObservationRecord, rung: Phase, performs: impl Fn(Phase) -> bool) -> Claim {
    if !performs(rung) || rung == Phase::None {
        return Claim::Silent;
    }
    match &record.verdict {
        Verdict::Fail(code) => {
            if rung == record.phase_reached {
                Claim::Rejected(code.clone(), first_rejection(record))
            } else if rung < record.phase_reached {
                Claim::Passed
            } else {
                Claim::Silent
            }
        }
        Verdict::Pass | Verdict::Unsupported => {
            if rung <= record.phase_reached {
                Claim::Passed
            } else {
                Claim::Silent
            }
        }
        Verdict::Exit(_) | Verdict::Trap(_) | Verdict::Ub(_) => {
            if rung == Phase::Run {
                Claim::Ran(record.verdict.clone())
            } else if rung < record.phase_reached {
                Claim::Passed
            } else {
                Claim::Silent
            }
        }
    }
}

fn render(record: &ObservationRecord) -> String {
    format!("{}@{}", record.verdict, record.phase_reached)
}

// ---------------------------------------------------------------------------
// The deep comparison
// ---------------------------------------------------------------------------

/// Compares two records of one program across unequal pipeline depth.
///
/// `a` is this interpreter's record; `b` is the counterparty's.
/// `source_has_unsafe` feeds the pre-M1 soundness-candidate rule (is04's
/// "zero UB in safe code", cross-implementation).
#[must_use]
pub fn compare_deep(
    a: &ObservationRecord,
    b: &ObservationRecord,
    source_has_unsafe: bool,
) -> FileComparison {
    let mut out = FileComparison::default();
    let file = a.file.clone();

    // The conservatism ledger first: `unsupported` on either side is never a
    // divergence and always a ledger entry ([proto.record.unsupported]).
    for (record, side) in [(a, Side::Interp), (b, Side::Counterparty)] {
        if matches!(record.verdict, Verdict::Unsupported) {
            let reason = record
                .extensions
                .get("x-unsupported")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("no reason given")
                .to_owned();
            out.ledger.push(LedgerEntry::Unsupported {
                file: file.clone(),
                side,
                phase: record.phase_reached,
                reason,
            });
        }
    }

    // `[proto.cmp.rung]` (s70, the DIV-2026-011 family's ruling): when both
    // records reject with `fail` and the FIRST diagnostic's code and span
    // agree, the records AGREE even when `phase_reached` names different
    // rungs of the shared ladder — where on the ladder an implementation
    // discovers a rejection is an architecture fact, not a semantic
    // observation. The tolerance is exactly one verdict wide: any pairing
    // that is not fail-against-fail falls through to the ladder walk below,
    // so fail-vs-pass, fail-vs-run-outcome and fail-vs-silence keep their
    // existing adjudication.
    if let (Verdict::Fail(code_a), Verdict::Fail(code_b)) = (&a.verdict, &b.verdict)
        && let (Some(da), Some(db)) = (first_rejection(a), first_rejection(b))
        && code_a == code_b
        && da.code == db.code
        && da.span == db.span
    {
        return out;
    }

    // Walk the ladder shallowest-first; the first disagreement wins, exactly
    // as the first diagnostic does ([proto.cmp.phase]).
    let mut divergence = None;
    for rung in Phase::LADDER {
        let ca = claim(a, rung, interp_performs);
        let cb = claim(b, rung, counterparty_performs);
        match (ca, cb) {
            (Claim::Silent, Claim::Silent) => {}
            (Claim::Passed, Claim::Passed) => {}

            // A rejection at a rung the other side performed and passed: the
            // accept-set divergence, at a mutually performed rung — real.
            (Claim::Rejected(code, diag), Claim::Passed | Claim::Ran(_)) => {
                divergence = Some(DeepDivergence {
                    file: file.clone(),
                    class: DeepClass::Verdict,
                    a: render(a),
                    b: render(b),
                    rung: Some(rung),
                    detail: format!(
                        "interp rejects at {rung} with {code}{} where the counterparty's \
                         record claims {rung} completed",
                        diag.as_ref()
                            .map(|d| format!(" at {:?}", d.span))
                            .unwrap_or_default()
                    ),
                    filed: filed(&file).map(|(id, _)| id),
                });
                break;
            }
            (Claim::Passed | Claim::Ran(_), Claim::Rejected(code, diag)) => {
                divergence = Some(DeepDivergence {
                    file: file.clone(),
                    class: DeepClass::Verdict,
                    a: render(a),
                    b: render(b),
                    rung: Some(rung),
                    detail: format!(
                        "counterparty rejects at {rung} with {code}{} where this machine's \
                         record claims {rung} completed",
                        diag.as_ref()
                            .map(|d| format!(" at {:?}", d.span))
                            .unwrap_or_default()
                    ),
                    filed: filed(&file).map(|(id, _)| id),
                });
                break;
            }

            // Both reject at this rung: agreement unless code or span differ.
            (Claim::Rejected(code_a, diag_a), Claim::Rejected(code_b, diag_b)) => {
                let span_a = diag_a.as_ref().map(|d| d.span);
                let span_b = diag_b.as_ref().map(|d| d.span);
                if code_a != code_b || span_a != span_b {
                    divergence = Some(DeepDivergence {
                        file: file.clone(),
                        class: DeepClass::SpanOrCode,
                        a: format!("{code_a}@{:?}", span_a.unwrap_or_default()),
                        b: format!("{code_b}@{:?}", span_b.unwrap_or_default()),
                        rung: Some(rung),
                        detail: "both reject here; the first diagnostic's code or span differs"
                            .to_owned(),
                        filed: filed(&file).map(|(id, _)| id),
                    });
                }
                break;
            }

            // A rejection the other side has no claim about: the accept-set
            // *boundary* — the conservatism ledger, not a divergence.
            (Claim::Rejected(code, _), Claim::Silent) => {
                out.ledger.push(LedgerEntry::RejectsBeyond {
                    file: file.clone(),
                    side: Side::Interp,
                    phase: rung,
                    code,
                });
                break;
            }
            (Claim::Silent, Claim::Rejected(code, _)) => {
                out.ledger.push(LedgerEntry::RejectsBeyond {
                    file: file.clone(),
                    side: Side::Counterparty,
                    phase: rung,
                    code,
                });
                break;
            }

            // Run outcomes on both sides (M1+): [proto.cmp.phase] at `run`.
            (Claim::Ran(va), Claim::Ran(vb)) => {
                divergence = compare_run(&file, a, b, &va, &vb);
                break;
            }

            // A run outcome the counterparty cannot check yet (pre-M1), or —
            // symmetric — one this machine could not produce. Ledger.
            (Claim::Ran(verdict), Claim::Silent) => {
                // The pre-M1 soundness rule: `ub(*)` on an unsafe-free program
                // the compiler accepted end-to-end is a candidate regardless.
                if matches!(verdict, Verdict::Ub(_))
                    && !source_has_unsafe
                    && !matches!(b.verdict, Verdict::Fail(_))
                {
                    divergence = Some(DeepDivergence {
                        file: file.clone(),
                        class: DeepClass::SoundnessCandidate,
                        a: render(a),
                        b: render(b),
                        rung: Some(Phase::Run),
                        detail: "the oracle reports UB in an unsafe-free program the \
                                 counterparty accepts as far as it checks — is04's `zero UB \
                                 in safe code`, cross-implementation ([proto.record.ub])"
                            .to_owned(),
                        filed: filed(&file).map(|(id, _)| id),
                    });
                } else {
                    out.ledger.push(LedgerEntry::RunUnmatched {
                        file: file.clone(),
                        verdict: verdict.to_string(),
                    });
                }
                break;
            }
            (Claim::Silent, Claim::Ran(_)) => {
                // The counterparty ran a program this machine did not (its
                // own `unsupported` entry is already ledgered above).
                break;
            }

            // Passed against Silent: the sides simply stop at different
            // depths with nothing further to say to each other.
            (Claim::Passed, Claim::Silent) | (Claim::Silent, Claim::Passed) => {}

            // `pass@run` against a run outcome: one record claims it merely
            // *stopped* at `run` where the other reports how running ended —
            // a protocol-shape disagreement, reported as a verdict mismatch
            // rather than guessed away.
            (Claim::Passed, Claim::Ran(_)) | (Claim::Ran(_), Claim::Passed) => {
                divergence = Some(DeepDivergence {
                    file: file.clone(),
                    class: DeepClass::Verdict,
                    a: render(a),
                    b: render(b),
                    rung: Some(rung),
                    detail: "one side reports `pass` at the run rung where the other reports \
                             a run outcome"
                        .to_owned(),
                    filed: filed(&file).map(|(id, _)| id),
                });
                break;
            }
        }
    }

    out.divergence = divergence;
    out
}

/// `[proto.cmp.phase]` at `run`: `exit` compares status and stdout digest,
/// `trap` compares kind only, `ub` against defined is the soundness class.
fn compare_run(
    file: &str,
    a: &ObservationRecord,
    b: &ObservationRecord,
    va: &Verdict,
    vb: &Verdict,
) -> Option<DeepDivergence> {
    let ub_a = matches!(va, Verdict::Ub(_));
    let ub_b = matches!(vb, Verdict::Ub(_));
    if ub_a != ub_b {
        return Some(DeepDivergence {
            file: file.to_owned(),
            class: DeepClass::SoundnessCandidate,
            a: render(a),
            b: render(b),
            rung: Some(Phase::Run),
            detail: "one side reports UB where the other runs defined ([proto.record.ub]); \
                     output bytes on such programs are never compared"
                .to_owned(),
            filed: filed(file).map(|(id, _)| id),
        });
    }
    if va != vb {
        return Some(DeepDivergence {
            file: file.to_owned(),
            class: DeepClass::Verdict,
            a: render(a),
            b: render(b),
            rung: Some(Phase::Run),
            detail: "run outcomes differ".to_owned(),
            filed: filed(file).map(|(id, _)| id),
        });
    }
    // `exit` compares its output bytes by `[proto.cmp.phase]`, and since is35
    // so does `trap` — the widening wolf-lang#216 asks the clause for, applied
    // here as the *instrument* while the clause is decided. Two things make
    // that honest rather than private agreement. It can only ADD rows: a
    // widened comparison never hides a disagreement, it only surfaces one, so
    // the risk it carries is a false row on the report and never a missed one.
    // And it is gated on both sides HOLDING the observable — `None` on either
    // is `[proto.record.fields]`'s honest-absent and is never a row, exactly
    // as `[proto.cmp.warn]` treats a missing `warnings` array. Until 0.1.23
    // this machine reported `null` on every trap (wolf-interp#55), which is
    // why wolf-lang#209 — a divergence made of nothing but trap-path output —
    // survived unmeasured from D66 to r05 with the two records verdict-
    // identical. `src/compare.rs` holds the clause as WRITTEN; the difference
    // between the two is the question, and is meant to be visible.
    let comparable_stdout = match va {
        Verdict::Exit(_) => true,
        Verdict::Trap(_) => a.stdout_sha256.is_some() && b.stdout_sha256.is_some(),
        _ => false,
    };
    if comparable_stdout && a.stdout_sha256 != b.stdout_sha256 {
        let outcome = if matches!(va, Verdict::Trap(_)) {
            "same trap kind, different output bytes before the fault \
             (wolf-lang#216: the clause compares `trap` by kind only; this is the \
             widening proposed there, and it can only add rows)"
        } else {
            "same exit status, different output bytes (byte-exact, no normalization — \
             any needed normalization is a spec bug about underspecified output)"
        };
        return Some(DeepDivergence {
            file: file.to_owned(),
            class: DeepClass::Stdout,
            a: format!("{:?}", a.stdout_sha256),
            b: format!("{:?}", b.stdout_sha256),
            rung: Some(Phase::Run),
            detail: outcome.to_owned(),
            filed: filed(file).map(|(id, _)| id),
        });
    }
    None
}

// ---------------------------------------------------------------------------
// Invocation — both sides as processes, bounded by a timeout
// ---------------------------------------------------------------------------

/// How one `conform-run` invocation ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    Record(Box<ObservationRecord>),
    /// The process ran but its stdout is not a schema-valid record.
    Malformed(String),
    /// The process outlived the budget and was killed.
    TimedOut,
    /// The process could not be started or exited a tool error.
    ToolError(String),
}

/// Which of the counterparty's execution tiers a comparison drives.
///
/// wolfgang's `conform-run` is one process contract over *several* engines,
/// selected by flag, and the choice decides how deep its record goes:
///
/// - [`Default`](CounterpartyTier::Default) — no flag. The compiler walks its
///   static pipeline and stops: `unsupported` at `wir`. Every run-tier
///   outcome this machine produces then has no counterparty claim to compare
///   against and lands in the conservatism ledger. This was the ONLY lane the
///   harness could drive through 0.1.9, which is why the run tier compared
///   almost nowhere.
/// - [`Checked`](CounterpartyTier::Checked) — `--checked`. The checked
///   interpretation tier; reaches `run`.
/// - [`Native`](CounterpartyTier::Native) — `--native`. The owned debug
///   backend; reaches `run`.
/// - [`Release`](CounterpartyTier::Release) — `--release`. The LLVM release
///   backend, with s42's mid-end and s43's whole-program layer ON; reaches
///   `run`.
///
/// The last three are what make this machine an oracle for a *transforming*
/// compiler: an optimizer is only correct if it preserves observable
/// behavior, so the same corpus compared against `Release` and against this
/// machine is the falsifiable form of that claim. `--native`/`--release`
/// additionally need `libwolf_rt.a` beside the `wolf` binary (`cargo build -p
/// wolf_rt`); without it the compiler declines as a tool and the lane reports
/// `ToolError` rather than silently degrading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CounterpartyTier {
    /// No flag: the compiler's static pipeline only (`unsupported` at `wir`).
    #[default]
    Default,
    /// `--checked`: the checked interpretation tier.
    Checked,
    /// `--native`: the owned debug backend.
    Native,
    /// `--release`: the LLVM release backend (mid-end + whole-program ON).
    Release,
}

impl CounterpartyTier {
    /// The flags this tier adds to the `conform-run` invocation.
    #[must_use]
    pub fn flags(self) -> &'static [&'static str] {
        match self {
            Self::Default => &[],
            Self::Checked => &["--checked"],
            Self::Native => &["--native"],
            Self::Release => &["--release"],
        }
    }

    /// The lane's name for reports and notices.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Checked => "checked",
            Self::Native => "native",
            Self::Release => "release",
        }
    }
}

/// Runs `<program> conform-run <file> --json` with a wall-clock budget.
///
/// Timeouts bound both sides; a timeout is a verdict, not an error. The wait
/// is a poll loop over `try_wait` — portable across the three tier-1
/// platforms, no unix-only process groups.
#[must_use]
pub fn invoke(program: &Path, file: &Path, timeout: Duration) -> Invocation {
    invoke_tier(program, file, timeout, CounterpartyTier::Default)
}

/// [`invoke`], driving a named counterparty tier.
///
/// This machine has exactly one engine, so its own side is always invoked at
/// [`CounterpartyTier::Default`] — the tier selects which of the
/// *counterparty's* engines answers, never which of ours.
#[must_use]
pub fn invoke_tier(
    program: &Path,
    file: &Path,
    timeout: Duration,
    tier: CounterpartyTier,
) -> Invocation {
    let mut command = Command::new(program);
    command
        .arg("conform-run")
        .arg(file)
        .arg("--json")
        .args(tier.flags())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            return Invocation::ToolError(format!("could not start `{}`: {e}", program.display()));
        }
    };

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Invocation::TimedOut;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Invocation::ToolError(format!("wait failed: {e}")),
        }
    };

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut stdout);
    }
    if !status.success() {
        // [proto.invoke.exit]: nonzero from conform-run itself means the TOOL
        // failed and no record exists.
        return Invocation::ToolError(format!(
            "`{}` exited {:?} with no record",
            program.display(),
            status.code()
        ));
    }
    let value: serde_json::Value = match serde_json::from_str(stdout.trim()) {
        Ok(value) => value,
        Err(e) => return Invocation::Malformed(format!("stdout is not JSON: {e}")),
    };
    if let Err(errors) = schema::validate(&value) {
        return Invocation::Malformed(format!("record rejected by the spec/06 schema: {errors}"));
    }
    match serde_json::from_value::<ObservationRecord>(value) {
        Ok(record) => Invocation::Record(Box::new(record)),
        Err(e) => Invocation::Malformed(format!("schema-valid JSON did not decode: {e}")),
    }
}

/// Compares two invocations of one file, treating runner-level outcomes
/// (timeout, malformed record, tool error) as comparable verdicts.
#[must_use]
pub fn compare_invocations(
    file: &str,
    a: &Invocation,
    b: &Invocation,
    source_has_unsafe: bool,
) -> FileComparison {
    let label = |i: &Invocation| match i {
        Invocation::Record(r) => render(r),
        Invocation::Malformed(_) => "malformed-record".to_owned(),
        Invocation::TimedOut => "timeout".to_owned(),
        Invocation::ToolError(_) => "tool-error".to_owned(),
    };
    match (a, b) {
        (Invocation::Record(ra), Invocation::Record(rb)) => compare_deep(ra, rb, source_has_unsafe),
        (Invocation::TimedOut, Invocation::TimedOut) => FileComparison::default(),
        _ => {
            let (class, detail) = match (a, b) {
                (Invocation::Malformed(e), _) | (_, Invocation::Malformed(e)) => (
                    DeepClass::Protocol,
                    format!("a side violated the record protocol: {e}"),
                ),
                (Invocation::ToolError(e), _) | (_, Invocation::ToolError(e)) => (
                    DeepClass::Protocol,
                    format!("a side failed as a tool ([proto.invoke.exit]): {e}"),
                ),
                _ => (
                    DeepClass::Timeout,
                    "one side exceeded the time budget where the other answered".to_owned(),
                ),
            };
            FileComparison {
                divergence: Some(DeepDivergence {
                    file: file.to_owned(),
                    class,
                    a: label(a),
                    b: label(b),
                    rung: None,
                    detail,
                    filed: filed(file).map(|(id, _)| id),
                }),
                ledger: Vec::new(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Counterparty detection — the ls00 SKIP pattern
// ---------------------------------------------------------------------------

/// Where a counterparty `wolf` binary may be found, and why one was not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Counterparty {
    Found(PathBuf),
    /// Structured absence: each candidate that was tried, and the doctrine
    /// note. The differential lane SKIPs loudly on this.
    Missing {
        tried: Vec<PathBuf>,
    },
}

/// Detects the pinned compiler binary.
///
/// An explicit `--compiler` path wins; otherwise the conventional build
/// products of `cargo build -p wolf_driver` inside `upstream/` are tried.
/// The vendored snapshot ships no `crates/`, so in CI this is `Missing` by
/// construction and the lane reports SKIP.
#[must_use]
pub fn detect_counterparty(explicit: Option<&Path>) -> Counterparty {
    if let Some(path) = explicit {
        if path.is_file() {
            return Counterparty::Found(path.to_owned());
        }
        return Counterparty::Missing {
            tried: vec![path.to_owned()],
        };
    }
    let exe = std::env::consts::EXE_SUFFIX;
    let candidates = [
        PathBuf::from(format!("upstream/target/debug/wolf{exe}")),
        PathBuf::from(format!("upstream/target/release/wolf{exe}")),
    ];
    for candidate in &candidates {
        if candidate.is_file() {
            return Counterparty::Found(candidate.clone());
        }
    }
    Counterparty::Missing {
        tried: candidates.to_vec(),
    }
}

/// The loud SKIP: one `notice:` line per fact, so a log grep finds the lane
/// and its reason without parsing prose (the ls00 pattern).
#[must_use]
pub fn skip_notice(tried: &[PathBuf]) -> String {
    let mut out = String::new();
    out.push_str("notice: differential lane SKIPPED — no counterparty compiler binary\n");
    for path in tried {
        out.push_str(&format!("notice: tried {}\n", path.display()));
    }
    out.push_str(
        "notice: build one locally with `cargo build -p wolf_driver` inside upstream/ \
         (integrator ruling, is05: building and executing the pinned compiler is legitimate \
         binary acquisition; reading its source remains forbidden)\n\
         notice: in CI the vendored snapshot has no crates/ and the private submodule cannot \
         clone, so this skip is expected there\n\
         notice: pass --require-counterparty to make this skip a hard failure\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trap::TrapKind;
    use std::collections::BTreeMap;

    /// The tier is a flag mapping and nothing more — but it is the mapping
    /// that decides whether the run tier compares at all, so it is pinned.
    /// `Default` MUST stay flagless: it is the lane every earlier round's
    /// numbers were taken on, and silently giving it a flag would rewrite
    /// history rather than extend it.
    #[test]
    fn each_counterparty_tier_maps_to_its_conform_run_flag() {
        assert_eq!(CounterpartyTier::Default.flags(), &[] as &[&str]);
        assert_eq!(CounterpartyTier::Checked.flags(), &["--checked"]);
        assert_eq!(CounterpartyTier::Native.flags(), &["--native"]);
        assert_eq!(CounterpartyTier::Release.flags(), &["--release"]);
        assert_eq!(CounterpartyTier::default(), CounterpartyTier::Default);

        // Every tier is labelled, and labels are distinct — a report that
        // cannot name its lane is a report whose numbers cannot be compared.
        let labels = [
            CounterpartyTier::Default.label(),
            CounterpartyTier::Checked.label(),
            CounterpartyTier::Native.label(),
            CounterpartyTier::Release.label(),
        ];
        let unique: std::collections::BTreeSet<&str> = labels.iter().copied().collect();
        assert_eq!(unique.len(), labels.len(), "{labels:?}");
        assert!(labels.iter().all(|l| !l.is_empty()));
    }

    /// `invoke` is the tier-free spelling and must stay identical to driving
    /// `Default` explicitly: our own side is always invoked plainly, and a
    /// hundred call sites depend on that equivalence.
    #[test]
    fn plain_invoke_is_the_default_tier() {
        let missing = Path::new("does/not/exist/wolf");
        let file = Path::new("does/not/exist.lu");
        let budget = Duration::from_millis(50);
        let a = invoke(missing, file, budget);
        let b = invoke_tier(missing, file, budget, CounterpartyTier::Default);
        assert!(matches!(a, Invocation::ToolError(_)), "{a:?}");
        assert_eq!(a, b);
    }

    fn record(phase: Phase, verdict: Verdict) -> ObservationRecord {
        ObservationRecord {
            protocol: crate::protocol::PROTOCOL_VERSION,
            impl_name: "x".to_owned(),
            impl_version: "0".to_owned(),
            commit: "c".to_owned(),
            file: "corpus/t.lu".to_owned(),
            phase_reached: phase,
            seeded: false,
            diagnostics: Vec::new(),
            warnings: None,
            verdict,
            stdout_sha256: None,
            stdout_inline: None,
            extensions: BTreeMap::new(),
        }
    }

    fn with_diag(mut r: ObservationRecord, code: &str, span: [u64; 2]) -> ObservationRecord {
        r.diagnostics = vec![Diagnostic {
            code: code.to_owned(),
            span,
            severity: "error".to_owned(),
        }];
        r
    }

    /// The pre-M1 shape of nearly every corpus comparison: this machine runs
    /// the file, the compiler completes typecheck and declares the rest
    /// outside its coverage. Agreement through `resolve`; two ledger entries.
    #[test]
    fn a_run_against_a_pre_m1_unsupported_is_ledgered_agreement() {
        let a = record(Phase::Run, Verdict::Exit(0));
        let b = record(Phase::Typecheck, Verdict::Unsupported);
        let out = compare_deep(&a, &b, false);
        assert_eq!(out.divergence, None);
        assert_eq!(out.ledger.len(), 2, "{:?}", out.ledger);
        assert!(matches!(
            &out.ledger[0],
            LedgerEntry::Unsupported {
                side: Side::Counterparty,
                phase: Phase::Typecheck,
                ..
            }
        ));
        assert!(matches!(
            &out.ledger[1],
            LedgerEntry::RunUnmatched { verdict, .. } if verdict == "exit(0)"
        ));
    }

    #[test]
    fn a_compiler_rejection_at_a_rung_we_skip_is_the_conservatism_ledger() {
        // The accept-set gap: E04xx at typecheck, a rung this machine never
        // performs. Expected by construction; never a divergence.
        let a = record(Phase::Run, Verdict::Exit(0));
        let b = with_diag(
            record(Phase::Typecheck, Verdict::Fail("E0401".to_owned())),
            "E0401",
            [10, 12],
        );
        let out = compare_deep(&a, &b, false);
        assert_eq!(out.divergence, None);
        assert!(matches!(
            &out.ledger[..],
            [LedgerEntry::RejectsBeyond { side: Side::Counterparty, phase: Phase::Typecheck, code, .. }]
                if code == "E0401"
        ));
    }

    #[test]
    fn a_rejection_at_a_mutually_performed_rung_is_a_real_divergence() {
        // The compiler parses what this machine rejects (or vice versa):
        // parse is a rung both perform, so this is never ledger material.
        let a = with_diag(
            record(Phase::Parse, Verdict::Fail("E0201".to_owned())),
            "E0201",
            [42, 45],
        );
        let b = record(Phase::Typecheck, Verdict::Unsupported);
        let out = compare_deep(&a, &b, false);
        let d = out.divergence.expect("a divergence");
        assert_eq!(d.class, DeepClass::Verdict);
        assert_eq!(d.rung, Some(Phase::Parse));

        let out = compare_deep(&b, &a, false);
        assert_eq!(out.divergence.expect("symmetric").class, DeepClass::Verdict);
    }

    #[test]
    fn matched_failures_compare_first_diagnostic_code_and_span() {
        let a = with_diag(
            record(Phase::Parse, Verdict::Fail("E0002".to_owned())),
            "E0002",
            [7, 8],
        );
        let same = with_diag(
            record(Phase::Parse, Verdict::Fail("E0002".to_owned())),
            "E0002",
            [7, 8],
        );
        assert_eq!(compare_deep(&a, &same, false).divergence, None);

        let other_span = with_diag(
            record(Phase::Parse, Verdict::Fail("E0002".to_owned())),
            "E0002",
            [7, 9],
        );
        assert_eq!(
            compare_deep(&a, &other_span, false)
                .divergence
                .expect("span divergence")
                .class,
            DeepClass::SpanOrCode
        );
    }

    #[test]
    fn failures_at_different_rungs_fire_at_the_shallower_one() {
        // interp fails parse; compiler passed parse and failed typecheck. The
        // first disagreement is at parse: rejected vs completed.
        let a = with_diag(
            record(Phase::Parse, Verdict::Fail("E0201".to_owned())),
            "E0201",
            [1, 2],
        );
        let b = with_diag(
            record(Phase::Typecheck, Verdict::Fail("E0401".to_owned())),
            "E0401",
            [1, 2],
        );
        let d = compare_deep(&a, &b, false).divergence.expect("diverges");
        assert_eq!(d.class, DeepClass::Verdict);
        assert_eq!(d.rung, Some(Phase::Parse));
    }

    /// `[proto.cmp.rung]`: same fail code, same first-diagnostic span, at
    /// DIFFERENT rungs of the shared ladder — agreement, no ledger noise.
    /// The DIV-2026-011 family's exact shape (E1007 resolve vs mem).
    #[test]
    fn fail_parity_at_different_rungs_is_agreement_under_proto_cmp_rung() {
        let a = with_diag(
            record(Phase::Resolve, Verdict::Fail("E1007".to_owned())),
            "E1007",
            [405, 408],
        );
        let b = with_diag(
            record(Phase::Mem, Verdict::Fail("E1007".to_owned())),
            "E1007",
            [405, 408],
        );
        let out = compare_deep(&a, &b, false);
        assert_eq!(out.divergence, None);
        assert!(out.ledger.is_empty(), "{:?}", out.ledger);
    }

    /// The tolerance is exactly one verdict wide: same code but a different
    /// span keeps the divergence, and fail-vs-run-outcome is untouched.
    #[test]
    fn the_rung_tolerance_never_covers_span_drift_or_other_verdicts() {
        let a = with_diag(
            record(Phase::Resolve, Verdict::Fail("E1007".to_owned())),
            "E1007",
            [405, 408],
        );
        let b = with_diag(
            record(Phase::Mem, Verdict::Fail("E1007".to_owned())),
            "E1007",
            [500, 503],
        );
        assert!(compare_deep(&a, &b, false).divergence.is_some());

        let b = record(Phase::Run, Verdict::Exit(0));
        let d = compare_deep(&a, &b, false).divergence.expect("diverges");
        assert_eq!(d.class, DeepClass::Verdict);
    }

    /// `[proto.record.warn]` lets a counterparty interleave warning-severity
    /// entries ahead of its rejection in `diagnostics` (source order). The
    /// fail comparison reads the first ERROR, so the lint's span is never
    /// compared against the rejection's (the resolve/cycle finding, 0.1.7).
    #[test]
    fn a_warning_ahead_of_the_rejection_does_not_defeat_fail_parity() {
        let a = with_diag(
            record(Phase::Resolve, Verdict::Fail("E0303".to_owned())),
            "E0303",
            [18, 28],
        );
        let mut b = record(Phase::Resolve, Verdict::Fail("E0303".to_owned()));
        b.diagnostics = vec![
            Diagnostic {
                code: "W0314".to_owned(),
                span: [100, 104],
                severity: "warning".to_owned(),
            },
            Diagnostic {
                code: "E0303".to_owned(),
                span: [18, 28],
                severity: "error".to_owned(),
            },
        ];
        let out = compare_deep(&a, &b, false);
        assert_eq!(out.divergence, None);
    }

    #[test]
    fn ub_on_an_unsafe_free_program_the_compiler_accepts_is_a_soundness_candidate() {
        let a = record(Phase::Run, Verdict::Ub("mem.ub".to_owned()));
        let b = record(Phase::Typecheck, Verdict::Unsupported);
        let d = compare_deep(&a, &b, false).divergence.expect("candidate");
        assert_eq!(d.class, DeepClass::SoundnessCandidate);

        // The same verdict on a program that required `unsafe` is not: the
        // compiler never ran it, so there is no defined execution to indict.
        let out = compare_deep(&a, &b, true);
        assert_eq!(out.divergence, None);
        assert!(
            out.ledger
                .iter()
                .any(|e| matches!(e, LedgerEntry::RunUnmatched { .. }))
        );

        // And a compiler *rejection* of the unsafe-free program means it did
        // not accept it — ledger, not candidate.
        let rejected = with_diag(
            record(Phase::Typecheck, Verdict::Fail("E0401".to_owned())),
            "E0401",
            [0, 1],
        );
        let out = compare_deep(&a, &rejected, false);
        assert_eq!(out.divergence, None);
    }

    #[test]
    fn m1_run_outcomes_compare_per_the_protocol() {
        // exit: status + stdout digest.
        let mut a = record(Phase::Run, Verdict::Exit(0));
        let mut b = record(Phase::Run, Verdict::Exit(0));
        a.stdout_sha256 = Some("aa".to_owned());
        b.stdout_sha256 = Some("bb".to_owned());
        assert_eq!(
            compare_deep(&a, &b, false)
                .divergence
                .expect("stdout")
                .class,
            DeepClass::Stdout
        );

        // trap: kind, and — since is35, the widening wolf-lang#216 proposes —
        // the output bytes written before the fault, when BOTH sides hold
        // them. Two records carrying nothing still agree by kind alone.
        let a = record(Phase::Run, Verdict::Trap(TrapKind::Bounds));
        let b = record(Phase::Run, Verdict::Trap(TrapKind::Bounds));
        assert_eq!(compare_deep(&a, &b, false).divergence, None);
        let c = record(Phase::Run, Verdict::Trap(TrapKind::DivZero));
        assert_eq!(
            compare_deep(&a, &c, false).divergence.expect("kind").class,
            DeepClass::Verdict
        );

        // ub against defined, both running: the classic soundness candidate.
        let a = record(Phase::Run, Verdict::Ub("mem.ub".to_owned()));
        let b = record(Phase::Run, Verdict::Exit(0));
        assert_eq!(
            compare_deep(&a, &b, true).divergence.expect("ub").class,
            DeepClass::SoundnessCandidate
        );
    }

    #[test]
    fn a_trap_s_output_compares_when_both_sides_hold_it() {
        // wolf-lang#216, the comparator half. wolf-lang#209 was a divergence
        // made of nothing but trap-path output and it survived from D66 to r05
        // because `[proto.cmp.phase]` compares `trap` by kind alone. Same kind,
        // different bytes is a row now.
        let mut a = record(Phase::Run, Verdict::Trap(TrapKind::Assert));
        let mut b = record(Phase::Run, Verdict::Trap(TrapKind::Assert));
        a.stdout_sha256 = Some("aa".to_owned());
        b.stdout_sha256 = Some("bb".to_owned());
        let d = compare_deep(&a, &b, false).divergence.expect("stdout");
        assert_eq!(d.class, DeepClass::Stdout);
        assert!(d.detail.contains("before the fault"), "{}", d.detail);

        // Same bytes: no row, which is what both of is34's movers do.
        b.stdout_sha256 = Some("aa".to_owned());
        assert_eq!(compare_deep(&a, &b, false).divergence, None);

        // wolf-lang#209, as this comparator would have seen it. lupin 0.1.22
        // ran the root domain's pending defers on its way out and printed
        // `inner inner-defer before-trap root-defer`; every wolfc lane
        // printed `inner inner-defer before-trap`. Same verdict, same trap
        // kind, different bytes — invisible for the whole of D66..r05, and a
        // `stdout` row here. The digests are the real ones.
        let sha = |text: &str| Some(crate::sha256::hex(text.as_bytes()));
        let mut old = record(Phase::Run, Verdict::Trap(TrapKind::Assert));
        let mut now = record(Phase::Run, Verdict::Trap(TrapKind::Assert));
        old.stdout_sha256 = sha("inner inner-defer before-trap root-defer");
        now.stdout_sha256 = sha("inner inner-defer before-trap");
        assert_eq!(
            compare_deep(&old, &now, false)
                .divergence
                .expect("#209 is a row now")
                .class,
            DeepClass::Stdout
        );
        // And 0.1.23's answer, which is the one this machine gives: agreement.
        assert_eq!(compare_deep(&now, &now, false).divergence, None);

        // Honest-absent on EITHER side is never a row — the same posture
        // `[proto.cmp.warn]` takes to a missing `warnings` array, and the
        // reason widening the comparison cannot manufacture a divergence out
        // of a counterparty that simply does not report the field yet.
        b.stdout_sha256 = None;
        assert_eq!(compare_deep(&a, &b, false).divergence, None);
        assert_eq!(compare_deep(&b, &a, false).divergence, None);

        // And an `exit` is unmoved: its bytes were always compared, absent or
        // not, because the clause has always said so.
        let mut x = record(Phase::Run, Verdict::Exit(0));
        let y = record(Phase::Run, Verdict::Exit(0));
        x.stdout_sha256 = Some("aa".to_owned());
        assert_eq!(
            compare_deep(&x, &y, false)
                .divergence
                .expect("stdout")
                .class,
            DeepClass::Stdout
        );
    }

    #[test]
    fn interp_unsupported_against_compiler_rejection_still_compares_the_shared_rungs() {
        // This machine declines a concurrency file at resolve; the compiler
        // rejects it at typecheck. Both parsed and resolved it — agreement
        // through resolve, plus ledger entries on both counts.
        let a = record(Phase::Resolve, Verdict::Unsupported);
        let b = with_diag(
            record(Phase::Typecheck, Verdict::Fail("E1102".to_owned())),
            "E1102",
            [5, 9],
        );
        let out = compare_deep(&a, &b, false);
        assert_eq!(out.divergence, None);
        assert_eq!(out.ledger.len(), 2);
        assert!(matches!(
            &out.ledger[1],
            LedgerEntry::RejectsBeyond { side: Side::Counterparty, code, .. } if code == "E1102"
        ));
    }

    #[test]
    fn runner_level_outcomes_gate_as_protocol_or_timeout() {
        let good = Invocation::Record(Box::new(record(Phase::Run, Verdict::Exit(0))));
        let bad = Invocation::Malformed("stdout is not JSON: gibberish".to_owned());
        let out = compare_invocations("corpus/t.lu", &good, &bad, false);
        assert_eq!(out.divergence.expect("protocol").class, DeepClass::Protocol);

        let slow = Invocation::TimedOut;
        let out = compare_invocations("corpus/t.lu", &good, &slow, false);
        assert_eq!(out.divergence.expect("timeout").class, DeepClass::Timeout);

        // Two timeouts agree — a timeout is a verdict, not an error.
        let out = compare_invocations("corpus/t.lu", &slow, &slow, false);
        assert_eq!(out.divergence, None);
    }

    #[test]
    fn severity_orders_the_report_and_the_spec_classes_outrank_runner_ones() {
        assert!(DeepClass::SoundnessCandidate < DeepClass::Verdict);
        assert!(DeepClass::Verdict < DeepClass::SpanOrCode);
        assert!(DeepClass::SpanOrCode < DeepClass::Stdout);
        assert!(DeepClass::Stdout < DeepClass::Protocol);
        assert!(DeepClass::Protocol < DeepClass::Timeout);
    }

    #[test]
    fn the_filed_list_resolves_and_annotates() {
        // DIV-2026-001..009 all resolved: 001..006 at is06 (pin 67c977f + the
        // resolve-rung work), 007..009 at is07 (pin 79ceec6 paid the is06
        // debts). DIV-2026-010 CLOSED at the 0.1.4 re-pin (`ad6cef7`).
        // DIV-2026-011/-012/-014/-015 — the rung-placement family, eleven
        // entries at 0.1.6 — CLOSED at the 0.1.7 re-pin (`e94b879`):
        // `[proto.cmp.rung]` landed in spec/06 and `compare_deep` implements
        // it, so fail parity (code + span) at any shared-ladder rung is
        // agreement and the eleven files compare clean. DIV-2026-016 —
        // wolfgang's conform-run answering E0806 on the file its own corpus
        // pins E0809 for — CLOSED at the 0.1.9 pin (`0b4e79c`): the CLEAN
        // build answers E0809, and the file compares clean under
        // `[proto.cmp.rung]`. The list is empty; these asserts resume the
        // moment anything is filed.
        // DIV-2026-017 and DIV-2026-020 CLOSED at the 3befc3e pin (0.1.24),
        // both upstream: wolf-lang#76 fixed the raw-literal decode, and D71 +
        // s134 ruled the E02xx span the offending token, which retires the
        // eight-file waiver by name (wolf-lang#220's own closing comment
        // assigns that retirement to this lane's pin bump). Seven of those
        // eight are byte-identical now; the eighth is DIV-2026-021, a
        // different finding wearing the same file.
        assert_eq!(FILED_DIVERGENCES.len(), 2);
        let (id, _) = filed("upstream/corpus/resolve/broken_sibling/entry.lu")
            .expect("DIV-2026-019 is filed against the D59 broken-sibling witness");
        assert_eq!(id, "DIV-2026-019");
        let (id, _) = filed("upstream/corpus/grammar/let_group_bare_tuple.lu")
            .expect("DIV-2026-021 is filed against the D63 let-group witness");
        assert_eq!(id, "DIV-2026-021");
        // A retired waiver must actually be gone: a file whose divergence was
        // fixed upstream and still carries a filing id is a green report that
        // means nothing, which is the wolf-lang#177 lesson in this shape.
        assert_eq!(filed("upstream/corpus/lints/raw_interp_braces.lu"), None);
        for retired in [
            "grammar/closure_params_no_separator.lu",
            "grammar/let_group_one_init.lu",
            "grammar/range_bare.lu",
            "grammar/struct_literal_no_separator.lu",
            "grammar/struct_pattern_no_separator.lu",
            "grammar/struct_pattern_rest_bare.lu",
            "grammar/tuple_pattern_no_separator.lu",
        ] {
            assert_eq!(
                filed(&format!("upstream/corpus/{retired}")),
                None,
                "{retired}"
            );
        }
        assert_eq!(
            filed("upstream/corpus/rows/negative/handler_uncovered.lu"),
            None
        );
        assert_eq!(filed("upstream/corpus/conc/store_buffer.lu"), None);
        assert_eq!(filed("upstream/corpus/memory/mode_missing_mut.lu"), None);
        assert_eq!(filed("upstream/corpus/typecheck/cast_bad.lu"), None);
        assert_eq!(filed("upstream/corpus/hello.lu"), None);
    }

    #[test]
    fn the_filing_template_names_the_spec_as_first_defendant() {
        let d = DeepDivergence {
            file: "corpus/t.lu".to_owned(),
            class: DeepClass::Verdict,
            a: "fail(E0201)@parse".to_owned(),
            b: "unsupported@typecheck".to_owned(),
            rung: Some(Phase::Parse),
            detail: "detail".to_owned(),
            filed: None,
        };
        let filing = d.filing();
        let spec = filing.find("spec bug").expect("spec listed");
        let compiler = filing.find("compiler bug").expect("compiler listed");
        let interp = filing.find("interpreter bug").expect("interp listed");
        assert!(spec < compiler && compiler < interp);
    }

    #[test]
    fn counterparty_detection_reports_what_it_tried() {
        let missing = detect_counterparty(Some(Path::new("does/not/exist/wolf")));
        let Counterparty::Missing { tried } = missing else {
            panic!("expected Missing");
        };
        let notice = skip_notice(&tried);
        assert!(notice.contains("SKIPPED"));
        assert!(notice.contains("does/not/exist/wolf"));
        assert!(notice.contains("--require-counterparty"));
        assert!(notice.contains("cargo build -p wolf_driver"));
    }
}

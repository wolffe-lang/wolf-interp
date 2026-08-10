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
/// Empty at pin `79ceec6`: the third corpus differential ran **0** divergences.
/// DIV-2026-007 closed there (the compiler's E0210 span now matches the
/// amended §3.3), and DIV-2026-008/009 closed with the repaired corpus files
/// (`freeze_publish.lu` reports through a channel; `when_multi.lu` expects
/// 223). The resolved entries live in `docs/divergence-log.md`.
pub const FILED_DIVERGENCES: &[(&str, &str, &str)] = &[];

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

fn claim(record: &ObservationRecord, rung: Phase, performs: impl Fn(Phase) -> bool) -> Claim {
    if !performs(rung) || rung == Phase::None {
        return Claim::Silent;
    }
    match &record.verdict {
        Verdict::Fail(code) => {
            if rung == record.phase_reached {
                Claim::Rejected(code.clone(), record.diagnostics.first().cloned())
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
    if matches!(va, Verdict::Exit(_)) && a.stdout_sha256 != b.stdout_sha256 {
        return Some(DeepDivergence {
            file: file.to_owned(),
            class: DeepClass::Stdout,
            a: format!("{:?}", a.stdout_sha256),
            b: format!("{:?}", b.stdout_sha256),
            rung: Some(Phase::Run),
            detail: "same exit status, different output bytes (byte-exact, no normalization — \
                     any needed normalization is a spec bug about underspecified output)"
                .to_owned(),
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

/// Runs `<program> conform-run <file> --json` with a wall-clock budget.
///
/// Timeouts bound both sides; a timeout is a verdict, not an error. The wait
/// is a poll loop over `try_wait` — portable across the three tier-1
/// platforms, no unix-only process groups.
#[must_use]
pub fn invoke(program: &Path, file: &Path, timeout: Duration) -> Invocation {
    let mut command = Command::new(program);
    command
        .arg("conform-run")
        .arg(file)
        .arg("--json")
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

        // trap: kind only.
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
        // debts; the third differential ran 0 divergences). The list is empty
        // and `filed` answers `None` for everything — including the three
        // files whose filings just closed.
        assert!(FILED_DIVERGENCES.is_empty());
        assert_eq!(filed("upstream/corpus/grammar/receiver_moded.lu"), None);
        assert_eq!(filed("upstream/corpus/conc/freeze_publish.lu"), None);
        assert_eq!(filed("upstream/corpus/conc/when_multi.lu"), None);
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

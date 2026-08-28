//! wolf-interp — the wolf reference interpreter.
//!
//! # The independence doctrine
//!
//! This crate shares **no** code with the wolf compiler. Everything here is
//! reimplemented from `spec/` — the grammar, the conformance vocabulary, the
//! directive grammar, the differential protocol. The only coupling permitted
//! is shared *data*: the pinned `upstream/spec` and `upstream/corpus` trees.
//! Two implementations that quietly share a parser cannot disagree, and
//! disagreement is the entire product (`[proto.cmp.triage]`).
//!
//! At is03 wolf programs **run**, regions and all. [`lex`] and [`parse`] implement
//! `spec/01-grammar.md` in full (is01); [`sema`] is the "sema-lite" module
//! machinery a call needs to find its callee, and nothing more; [`eval`] is
//! `spec/02-memory-model.md` §2 as a dynamic machine, with every ownership rule
//! enforced as a runtime check and every fault citing its clause
//! ([`eval::rules`]). [`frontend`] speaks the result as a spec/06 observation
//! record and documents the ladder mapping — which rungs this implementation
//! completes, and which belong to the compiler.
//!
//! What is outside that scope reports the last *completed* phase with verdict
//! `unsupported`, so the conservatism ledger stays truthful
//! (`[proto.record.unsupported]`); [`ledger`] is where an observation is judged
//! against a corpus expectation, and it keeps "expected divergence" and
//! "finding" apart on purpose.

pub mod anchor;
pub mod ast;
pub mod compare;
pub mod corpus;
pub mod diag;
pub mod differ;
pub mod directive;
pub mod edit;
pub mod eval;
pub mod explore;
pub mod export;
pub mod fmtspec;
pub mod frontend;
pub mod fuzz;
pub mod json;
pub mod ledger;
pub mod lex;
pub mod lint;
pub mod parse;
pub mod phase;
pub mod protocol;
pub mod schema;
pub mod sema;
pub mod sha256;
pub mod trap;

use std::collections::BTreeMap;
use std::path::Path;

use crate::phase::Phase;
use crate::protocol::{ObservationRecord, PROTOCOL_VERSION, Verdict};

/// The `impl` field this implementation writes into every record: the binary
/// is named `lupin` as of v0.1.0 (is12), and v0.1.0 is the moment identity
/// settled — records and bundle manifests say `lupin` from here on.
/// Historical records naming `wolf-interp` stay valid; the protocol compares
/// everything *except* the identity fields.
pub const IMPL_NAME: &str = "lupin";

/// The `impl_version` field: this crate's version.
pub const IMPL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The `commit` field: the git revision this binary was built from, or
/// `unknown` when built outside a checkout.
pub const COMMIT: &str = env!("WOLF_INTERP_COMMIT");

/// The upstream spec/corpus pin, verbatim from `vendor/upstream/PIN` at
/// compile time (the tracked snapshot is byte-identical to the submodule, so
/// there is one answer). `--version` prints its short form.
pub const UPSTREAM_PIN: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/vendor/upstream/PIN"));

/// The pin's 7-character short spelling, for `--version`.
#[must_use]
pub fn upstream_pin_short() -> &'static str {
    let pin = UPSTREAM_PIN.trim();
    pin.get(..7).unwrap_or(pin)
}

/// Upstream root: the live submodule when initialized, else the tracked
/// vendored snapshot (vendor/README.md — private-submodule CI fallback).
#[must_use]
pub fn upstream_root() -> &'static str {
    if Path::new("upstream/corpus").is_dir() {
        "upstream"
    } else {
        "vendor/upstream"
    }
}

/// Renders a path with `/` separators on every platform.
///
/// Records travel between machines and get diffed; a Windows `\` in the `file`
/// field would make identical observations compare unequal (and, compiler-side,
/// once fed an unvetted seed space).
#[must_use]
pub fn slash_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Observes one program and builds its spec/06 record.
///
/// Unseeded: the strict-FIFO default schedule runs and the record declares
/// `seeded: false`. `--seed=N` goes through [`observe_record_seeded`], which
/// requests the spec/03 §5 deterministic schedule and declares `seeded: true`
/// (`[proto.seed.flag]`).
#[must_use]
pub fn observe_record(
    file: &Path,
    source: &[u8],
    requested_phase: Option<Phase>,
) -> (ObservationRecord, Observed) {
    observe_record_seeded(file, source, requested_phase, crate::eval::Trace::Off, None)
}

/// The human-facing half of an observation: never on the wire
/// (`[proto.record.diag]` — messages are a per-implementation quality concern).
#[derive(Debug, Clone)]
pub struct Observed {
    pub diagnostic: Option<crate::diag::Diag>,
    pub trap: Option<crate::eval::Trap>,
    /// The program's whole stdout, whatever the verdict. The record carries
    /// stdout only for `exit` (`[proto.record.fields]`), but the *directive*
    /// matcher compares "the program's stdout" (`[conf.directive.check]`) —
    /// a `run(exit=trap(assert), stdout="…")` pin judges the bytes printed
    /// before the trap, which the wire record deliberately drops.
    pub stdout: String,
    /// The `[mem.ub]` row behind a `ub(…)` verdict, with its two spans, the
    /// optimization it licenses, and the borrow-tree slice at the violation.
    pub ub: Option<crate::eval::prov::UbFinding>,
    pub trace: Vec<String>,
    /// Regions still holding memory at program exit — is03's leak assertion,
    /// exposed so CI can assert it over the whole corpus.
    pub leaks: Vec<crate::eval::region::RegionId>,
    /// `Err` when the region forest invariant broke during the run.
    pub forest: Result<(), String>,
}

impl Default for Observed {
    fn default() -> Observed {
        Observed {
            diagnostic: None,
            trap: None,
            stdout: String::new(),
            ub: None,
            trace: Vec::new(),
            leaks: Vec::new(),
            forest: Ok(()),
        }
    }
}

/// As [`observe_record`], with `--trace`'s rule log.
#[must_use]
pub fn observe_record_traced(
    file: &Path,
    source: &[u8],
    requested_phase: Option<Phase>,
    trace: crate::eval::Trace,
) -> (ObservationRecord, Observed) {
    observe_record_seeded(file, source, requested_phase, trace, None)
}

/// As [`observe_record`], with the trace log and `--seed=N`'s schedule
/// request (`[conc.det.seed]`): one seed selects the whole schedule, the
/// record declares `"seeded": true`, and the same seed replays the identical
/// decision stream (`[proto.seed.flag]`).
#[must_use]
pub fn observe_record_seeded(
    file: &Path,
    source: &[u8],
    requested_phase: Option<Phase>,
    trace: crate::eval::Trace,
    seed: Option<u64>,
) -> (ObservationRecord, Observed) {
    let request = seed.map_or(eval::SchedRequest::Default, eval::SchedRequest::Seed);
    observe_record_scheduled(file, source, requested_phase, trace, &request, None)
}

/// As [`observe_record_seeded`], with the full schedule-request surface:
/// `--seed=N` (generator or packed) or `--schedule=ev:…` (an explicit
/// decision stream — the is07 explorer's counterexample spelling when the
/// stream does not fit the 62-bit packed payload). Any explicit request is
/// deterministic replay, so the record declares `seeded: true`
/// (`[proto.seed.equal]`).
#[must_use]
pub fn observe_record_scheduled(
    file: &Path,
    source: &[u8],
    requested_phase: Option<Phase>,
    trace: crate::eval::Trace,
    request: &eval::SchedRequest,
    std_root: Option<&std::path::Path>,
) -> (ObservationRecord, Observed) {
    let observation =
        frontend::observe_file(file, source, requested_phase, trace, request, std_root);
    record_of(slash_path(file), observation, request)
}

/// is12's stdin door (`lupin run - --json`): the same record built from a
/// buffer with no module graph — a piped program is a one-file root module —
/// and `file` reported as `-`, the only spelling the invocation had.
#[must_use]
pub fn observe_record_stdin(
    source: &[u8],
    request: &eval::SchedRequest,
) -> (ObservationRecord, Observed) {
    let observation = frontend::observe_buffer(source, None, request);
    record_of("-".to_owned(), observation, request)
}

fn record_of(
    file: String,
    observation: frontend::Observation,
    request: &eval::SchedRequest,
) -> (ObservationRecord, Observed) {
    // `[proto.record.fields]`: the digest whenever the program wrote output,
    // the inline text up to 4096 bytes. Only an `exit` verdict has "the program
    // wrote output" to speak of — a trap's partial output is not a comparison
    // surface the protocol defines.
    let observed_stdout = String::from_utf8_lossy(&observation.stdout).into_owned();
    let wrote = matches!(observation.verdict, Verdict::Exit(_)) && !observation.stdout.is_empty();
    let (digest, inline) = if wrote {
        let text = String::from_utf8_lossy(&observation.stdout).into_owned();
        // The cap is in *bytes*; truncating mid-code-point would produce a
        // record this implementation's own validator rejects.
        let mut cut = schema::STDOUT_INLINE_LIMIT.min(text.len());
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        (
            Some(sha256::hex(&observation.stdout)),
            Some(text[..cut].to_owned()),
        )
    } else {
        (None, None)
    };

    // `[proto.record.ext]`: `x-` keys are ours, and participate in equality
    // only when both records carry them — which is exactly right for a reason
    // string the other implementation has no obligation to match.
    let mut extensions = BTreeMap::new();
    if let Some(reason) = &observation.reason {
        extensions.insert(
            "x-unsupported".to_owned(),
            serde_json::Value::from(reason.clone()),
        );
    }
    if let Some(trap) = &observation.trap {
        extensions.insert(
            "x-trap-clause".to_owned(),
            serde_json::Value::from(trap.rule.anchor()),
        );
        extensions.insert(
            "x-trap-span".to_owned(),
            serde_json::json!([trap.span.start, trap.span.end]),
        );
    }
    // `[proto.record.ub]`: "`ub(anchor)` cites the s04 §7 row (e.g., `ub(mem.ub)`
    // with the row id in `x-ub-row`, or the specific clause)". Both, then: the
    // verdict carries the document-level anchor so two implementations compare
    // like for like, and the row — the thing the two oracles must actually
    // agree on — rides the extension key the clause names.
    if let Some(finding) = &observation.ub {
        extensions.insert(
            "x-ub-row".to_owned(),
            serde_json::Value::from(finding.row.id()),
        );
        extensions.insert(
            "x-ub-clause".to_owned(),
            serde_json::Value::from(finding.row.clause()),
        );
        extensions.insert(
            "x-ub-span".to_owned(),
            serde_json::json!([finding.span.start, finding.span.end]),
        );
        if let Some(tag) = finding.tag_span {
            extensions.insert(
                "x-ub-tag-span".to_owned(),
                serde_json::json!([tag.start, tag.end]),
            );
        }
        // The D2 pairing on the wire: a row without a licensed optimization is
        // one this language does not have (`[mem.ub.closed]`).
        extensions.insert(
            "x-ub-licenses".to_owned(),
            serde_json::Value::from(finding.row.optimization()),
        );
        extensions.insert("x-ub-tree".to_owned(), serde_json::json!(finding.tree));
    }

    // `[proto.record.warn]`: since 0.1.6 this implementation runs warning
    // analyses (the s68 subset — `lint::IMPLEMENTED`), so a record whose
    // program loaded carries the array, and the same observations ride
    // `diagnostics` with `"severity": "warning"` — after the error, whose
    // first position `[proto.cmp.phase]` compares. A program that never
    // loaded (lex/parse failure) still omits the array: the analyses did
    // not run, and honest-absent is the posture.
    let mut diagnostics = observation.diagnostics;
    if let Some(warns) = &observation.warnings {
        diagnostics.extend(warns.iter().map(|warning| protocol::Diagnostic {
            code: warning.code.clone(),
            span: warning.span,
            severity: "warning".to_owned(),
        }));
    }

    let record = ObservationRecord {
        protocol: PROTOCOL_VERSION,
        impl_name: IMPL_NAME.to_owned(),
        impl_version: IMPL_VERSION.to_owned(),
        commit: COMMIT.to_owned(),
        file,
        phase_reached: observation.phase_reached,
        seeded: request.is_seeded(),
        diagnostics,
        warnings: observation.warnings.clone(),
        verdict: observation.verdict,
        stdout_sha256: digest,
        stdout_inline: inline,
        extensions,
    };
    (
        record,
        Observed {
            diagnostic: observation.detail,
            trap: observation.trap,
            stdout: observed_stdout,
            ub: observation.ub,
            trace: observation.trace,
            leaks: observation.leaks,
            forest: observation.forest,
        },
    )
}

/// Explores every inequivalent schedule of the program rooted at `file` —
/// `conform-run --explore=N` (is07's systematic interleaving search; see
/// [`explore`]). The program must clear the SAME admission ladder `run`
/// clears ([`frontend::admit`] — issue #22: the E11xx statics included); a
/// rejection comes back as `Err` with the diagnostic rendered, because a
/// schedule space only exists for an admitted program.
///
/// # Errors
///
/// The rendered admission refusal or module-graph failure.
pub fn explore_file(
    file: &Path,
    options: &explore::Options,
    std_root: Option<&Path>,
) -> Result<explore::Report, String> {
    let program = match sema::load_with(file, std_root) {
        Ok(program) => program,
        Err(sema::LoadError::Syntax { file, diag }) => return Err(format!("{file}: {diag}")),
        Err(sema::LoadError::Io(message)) => {
            return Err(format!("the module graph could not be read: {message}"));
        }
    };
    match frontend::admit(&program) {
        Some(frontend::Refusal::Reject(diag)) => Err(format!(
            "{diag} — statically rejected; the explorer runs the same admission ladder as `run`"
        )),
        Some(frontend::Refusal::Unsupported(reason)) => Err(format!("unsupported — {reason}")),
        None => Ok(explore::explore(&program, options)),
    }
}

/// The record for a program nothing has looked at — kept for callers that want
/// the shape without running the frontend.
#[must_use]
pub fn unsupported_record(file: &Path) -> ObservationRecord {
    ObservationRecord {
        protocol: PROTOCOL_VERSION,
        impl_name: IMPL_NAME.to_owned(),
        impl_version: IMPL_VERSION.to_owned(),
        commit: COMMIT.to_owned(),
        file: slash_path(file),
        phase_reached: Phase::None,
        seeded: false,
        diagnostics: Vec::new(),
        warnings: None,
        verdict: Verdict::Unsupported,
        stdout_sha256: None,
        stdout_inline: None,
        extensions: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stub_record_is_schema_valid_and_honest() {
        let record = unsupported_record(Path::new("upstream/corpus/hello.lu"));
        assert_eq!(record.phase_reached, Phase::None);
        assert_eq!(record.verdict, Verdict::Unsupported);
        assert!(!record.seeded);
        assert_eq!(record.impl_name, "lupin");

        let json: serde_json::Value =
            serde_json::from_str(&record.to_json_line().expect("serializes")).expect("is json");
        assert_eq!(schema::validate(&json), Ok(()));
    }

    #[test]
    fn record_paths_are_slash_separated() {
        let path = Path::new("upstream").join("corpus").join("hello.lu");
        assert_eq!(unsupported_record(&path).file, "upstream/corpus/hello.lu");
    }
}

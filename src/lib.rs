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
pub mod eval;
pub mod frontend;
pub mod fuzz;
pub mod ledger;
pub mod lex;
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

/// The `impl` field this implementation writes into every record.
pub const IMPL_NAME: &str = "wolf-interp";

/// The `impl_version` field: this crate's version.
pub const IMPL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The `commit` field: the git revision this binary was built from, or
/// `unknown` when built outside a checkout.
pub const COMMIT: &str = env!("WOLF_INTERP_COMMIT");

/// Renders a path with `/` separators on every platform.
///
/// Records travel between machines and get diffed; a Windows `\` in the `file`
/// field would make identical observations compare unequal (and, compiler-side,
/// once fed an unvetted seed space).
#[must_use]
/// Upstream root: the live submodule when initialized, else the tracked
/// vendored snapshot (vendor/README.md — private-submodule CI fallback).
pub fn upstream_root() -> &'static str {
    if Path::new("upstream/corpus").is_dir() {
        "upstream"
    } else {
        "vendor/upstream"
    }
}

pub fn slash_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Observes one program and builds its spec/06 record.
///
/// `seeded` is always `false`: there is no seeded scheduling yet, whatever
/// `--seed` was passed (`[proto.seed.flag]` — an implementation without it
/// *declares* so rather than lying).
#[must_use]
pub fn observe_record(
    file: &Path,
    source: &[u8],
    requested_phase: Option<Phase>,
) -> (ObservationRecord, Observed) {
    observe_record_traced(file, source, requested_phase, crate::eval::Trace::Off)
}

/// The human-facing half of an observation: never on the wire
/// (`[proto.record.diag]` — messages are a per-implementation quality concern).
#[derive(Debug, Clone)]
pub struct Observed {
    pub diagnostic: Option<crate::diag::Diag>,
    pub trap: Option<crate::eval::Trap>,
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
    let observation = frontend::observe_file(file, source, requested_phase, trace);

    // `[proto.record.fields]`: the digest whenever the program wrote output,
    // the inline text up to 4096 bytes. Only an `exit` verdict has "the program
    // wrote output" to speak of — a trap's partial output is not a comparison
    // surface the protocol defines.
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

    let record = ObservationRecord {
        protocol: PROTOCOL_VERSION,
        impl_name: IMPL_NAME.to_owned(),
        impl_version: IMPL_VERSION.to_owned(),
        commit: COMMIT.to_owned(),
        file: slash_path(file),
        phase_reached: observation.phase_reached,
        seeded: false,
        diagnostics: observation.diagnostics,
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
            ub: observation.ub,
            trace: observation.trace,
            leaks: observation.leaks,
            forest: observation.forest,
        },
    )
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
        assert_eq!(record.impl_name, "wolf-interp");

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

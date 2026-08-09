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
//! At is01 the frontend is real: [`lex`] and [`parse`] implement
//! `spec/01-grammar.md` in full, and [`frontend`] speaks the result as a
//! spec/06 observation record at the `lex` and `parse` rungs. Everything
//! deeper is still an honest `unsupported` — the conservatism ledger stays
//! truthful (`[proto.record.unsupported]`).

pub mod anchor;
pub mod ast;
pub mod compare;
pub mod corpus;
pub mod diag;
pub mod directive;
pub mod frontend;
pub mod lex;
pub mod parse;
pub mod phase;
pub mod protocol;
pub mod schema;
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
) -> (ObservationRecord, Option<crate::diag::Diag>) {
    let observation = frontend::observe(source, requested_phase);
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
        stdout_sha256: None,
        stdout_inline: None,
        extensions: BTreeMap::new(),
    };
    (record, observation.detail)
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

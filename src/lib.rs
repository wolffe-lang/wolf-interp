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
//! At sprint is00 there is no language work at all: no lexer, no parser, no
//! evaluator. What exists is the scaffolding that makes independence
//! mechanical — a directive parser, a corpus harness, and an honest
//! `unsupported` speaker of the spec/06 protocol.

pub mod anchor;
pub mod corpus;
pub mod directive;
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

/// Builds the honest observation record this sprint can produce: nothing has
/// been implemented, so nothing is claimed.
///
/// `phase_reached` is `none` and the verdict is `unsupported` — a legal
/// verdict per `[proto.record.unsupported]`, excluded from divergence counting
/// and visible in the conservatism ledger. `seeded` is `false` because there
/// is no seeded scheduling to speak of yet (`[proto.seed.flag]`), whatever
/// `--seed` was passed.
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

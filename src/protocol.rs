//! The differential protocol's wire types.
//!
//! Implemented directly from `spec/06-differential-protocol.md`
//! `[proto.record]` — `"protocol": 1`, no interim version of our own
//! invention. This repo is the protocol's second implementer; the schema is
//! the *only* thing the two implementations share, and it is a spec, not code.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::anchor;
use crate::phase::Phase;
use crate::trap::TrapKind;

/// The only protocol version this implementation speaks.
pub const PROTOCOL_VERSION: u64 = 1;

/// The program's outcome, per `[proto.record.verdict]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Stopped at the requested phase, clean.
    Pass,
    /// Rejected; carries the first diagnostic's code.
    Fail(String),
    /// Ran to completion with this exit status.
    Exit(u8),
    /// Faulted with this trap kind (`[conf.trap.set]`).
    Trap(TrapKind),
    /// Oracle-detected UB, citing a clause anchor. Highest-severity class.
    Ub(String),
    /// Outside this implementation's current scope. A legal verdict
    /// (`[proto.record.unsupported]`), excluded from divergence counting.
    Unsupported,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Pass => f.write_str("pass"),
            Verdict::Fail(code) => write!(f, "fail({code})"),
            Verdict::Exit(status) => write!(f, "exit({status})"),
            Verdict::Trap(kind) => write!(f, "trap({kind})"),
            Verdict::Ub(anchor) => write!(f, "ub({anchor})"),
            Verdict::Unsupported => f.write_str("unsupported"),
        }
    }
}

/// Why a verdict string is not a legal `[proto.record.verdict]` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictParseError(pub String);

impl fmt::Display for VerdictParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for VerdictParseError {}

impl FromStr for Verdict {
    type Err = VerdictParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn args<'a>(s: &'a str, name: &str) -> Option<&'a str> {
            s.strip_prefix(name)?.strip_prefix('(')?.strip_suffix(')')
        }

        match s {
            "pass" => return Ok(Verdict::Pass),
            "unsupported" => return Ok(Verdict::Unsupported),
            _ => {}
        }
        if let Some(code) = args(s, "fail") {
            if code.is_empty() || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err(VerdictParseError(format!(
                    "verdict `{s}`: `fail(CODE)` takes an alphanumeric diagnostic code"
                )));
            }
            return Ok(Verdict::Fail(code.to_owned()));
        }
        if let Some(status) = args(s, "exit") {
            let parsed: u8 = status.parse().map_err(|_| {
                VerdictParseError(format!(
                    "verdict `{s}`: `exit(N)` takes a status in 0..=255"
                ))
            })?;
            return Ok(Verdict::Exit(parsed));
        }
        if let Some(kind) = args(s, "trap") {
            let parsed = TrapKind::parse(kind).ok_or_else(|| {
                VerdictParseError(format!(
                    "verdict `{s}`: unknown trap kind `{kind}`; the vocabulary is closed: {}",
                    TrapKind::vocabulary()
                ))
            })?;
            return Ok(Verdict::Trap(parsed));
        }
        if let Some(tag) = args(s, "ub") {
            anchor::classify(tag).map_err(|e| VerdictParseError(format!("verdict `{s}`: {e}")))?;
            return Ok(Verdict::Ub(tag.to_owned()));
        }
        Err(VerdictParseError(format!(
            "unknown verdict `{s}`; expected pass, fail(CODE), exit(N), trap(kind), ub(anchor) or unsupported"
        )))
    }
}

impl Serialize for Verdict {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Verdict {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// A protocol diagnostic: `{code, span, severity}` and nothing else.
///
/// Messages are deliberately absent — `[proto.record.diag]` (D22): wording is a
/// per-implementation quality concern and is never compared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    /// Byte-offset half-open span `[start, end)`.
    pub span: [u64; 2],
    pub severity: String,
}

/// One observation record — the JSON object `conform-run --json` writes to
/// stdout (`[proto.record]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationRecord {
    pub protocol: u64,
    #[serde(rename = "impl")]
    pub impl_name: String,
    pub impl_version: String,
    pub commit: String,
    /// The program observed, with `/` separators on every platform.
    pub file: String,
    pub phase_reached: Phase,
    pub seeded: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub verdict: Verdict,
    #[serde(default)]
    pub stdout_sha256: Option<String>,
    #[serde(default)]
    pub stdout_inline: Option<String>,
    /// `x-` extension keys (`[proto.record.ext]`). Ordered so serialization is
    /// byte-stable; they participate in equality only when both records carry
    /// the same key.
    #[serde(flatten, default)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl ObservationRecord {
    /// Serializes to the single-line JSON form the protocol puts on stdout.
    ///
    /// # Errors
    ///
    /// Returns the serde error if the record cannot be serialized.
    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

impl Serialize for Phase {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Phase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verdict_shape_round_trips() {
        let verdicts = [
            Verdict::Pass,
            Verdict::Fail("E1002".to_owned()),
            Verdict::Exit(0),
            Verdict::Exit(255),
            Verdict::Trap(TrapKind::DivZero),
            Verdict::Ub("mem.ub".to_owned()),
            Verdict::Unsupported,
        ];
        for verdict in verdicts {
            let text = verdict.to_string();
            assert_eq!(text.parse::<Verdict>(), Ok(verdict.clone()), "{text}");
        }
    }

    #[test]
    fn verdict_spellings_match_the_spec_examples() {
        assert_eq!(Verdict::Exit(0).to_string(), "exit(0)");
        assert_eq!(
            Verdict::Trap(TrapKind::StaleHandle).to_string(),
            "trap(stale-handle)"
        );
        assert_eq!(Verdict::Ub("mem.ub".to_owned()).to_string(), "ub(mem.ub)");
    }

    #[test]
    fn bad_verdicts_are_rejected() {
        for bad in [
            "",
            "PASS",
            "exit()",
            "exit(999)",
            "exit(-1)",
            "trap()",
            "trap(segfault)",
            "ub(Nope)",
            "ub(wolf.made.up)",
            "fail()",
            "fail(E 2)",
            "crash",
        ] {
            assert!(bad.parse::<Verdict>().is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn records_round_trip_through_json() {
        let record = ObservationRecord {
            protocol: PROTOCOL_VERSION,
            impl_name: "wolf-interp".to_owned(),
            impl_version: "0.0.1".to_owned(),
            commit: "abc1234".to_owned(),
            file: "corpus/hello.lu".to_owned(),
            phase_reached: Phase::None,
            seeded: false,
            diagnostics: vec![Diagnostic {
                code: "E1002".to_owned(),
                span: [120, 133],
                severity: "error".to_owned(),
            }],
            verdict: Verdict::Unsupported,
            stdout_sha256: None,
            stdout_inline: None,
            extensions: BTreeMap::new(),
        };
        let line = record.to_json_line().expect("serializes");
        let back: ObservationRecord = serde_json::from_str(&line).expect("deserializes");
        assert_eq!(back, record);
    }

    #[test]
    fn extension_keys_survive_a_round_trip() {
        let json = r#"{"protocol":1,"impl":"example","impl_version":"0.0.1","commit":"abc1234",
            "file":"corpus/memory/unsafe_ub_uaf.lu","phase_reached":"run","seeded":false,
            "diagnostics":[],"verdict":"ub(mem.ub)","stdout_sha256":null,"stdout_inline":null,
            "x-ub-row":"P1","x-oracle-trace":["retag","free","read"]}"#;
        let record: ObservationRecord = serde_json::from_str(json).expect("deserializes");
        assert_eq!(record.verdict, Verdict::Ub("mem.ub".to_owned()));
        assert_eq!(record.extensions.len(), 2);
        assert_eq!(
            record.extensions.get("x-ub-row"),
            Some(&serde_json::Value::from("P1"))
        );
        let back: ObservationRecord =
            serde_json::from_str(&record.to_json_line().expect("serializes")).expect("re-reads");
        assert_eq!(back, record);
    }

    #[test]
    fn phase_reached_is_a_ladder_rung_on_the_wire() {
        let json = r#"{"protocol":1,"impl":"x","impl_version":"0","commit":"c","file":"f.lu",
            "phase_reached":"codegen","seeded":false,"diagnostics":[],"verdict":"pass",
            "stdout_sha256":null,"stdout_inline":null}"#;
        assert!(serde_json::from_str::<ObservationRecord>(json).is_err());
    }
}

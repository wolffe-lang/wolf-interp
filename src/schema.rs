//! The observation-record schema validator.
//!
//! Validates raw JSON against `spec/06-differential-protocol.md`
//! `[proto.record]` *before* it is trusted as a record. It works on
//! `serde_json::Value` rather than on the typed [`crate::protocol`] structs
//! deliberately: the interesting failures — a missing required field, a
//! protocol version from the future, a non-`x-` stranger key — are the ones a
//! lenient deserializer would paper over.
//!
//! `[proto.harness.fixtures]` makes this validator's behaviour testable:
//! `corpus/protocol/` ships canned records that *every* conforming validator
//! must accept or reject exactly as named. Those fixtures are this module's
//! acceptance test (`tests/protocol_fixtures.rs`).

use std::fmt;

use serde_json::Value;

use crate::phase::Phase;
use crate::protocol::{PROTOCOL_VERSION, Verdict};

/// Fields `[proto.record.fields]` marks required, in spec order.
pub const REQUIRED_FIELDS: [&str; 9] = [
    "protocol",
    "impl",
    "impl_version",
    "commit",
    "file",
    "phase_reached",
    "seeded",
    "diagnostics",
    "verdict",
];

/// Fields that are optional, but reserved: `null` or a value of the right
/// shape, never a stranger key. `warnings` is `[proto.record.warn]` (s67,
/// additive within protocol 1): validators accept records with or without it.
///
/// `trap_message` is `[proto.record.trap]` (wolf-lang s169, spec/06 §2;
/// wolf-interp#129), additive in the same way and for the same reason. It
/// carries the program's OWN words for its fault — today exactly the second
/// argument of `assert(cond, msg)`. wolfgang emits it on the `--checked`
/// lane from that PR, so until this entry existed every wolfgang record for a
/// failing two-argument `assert` read to this validator as MALFORMED rather
/// than as a record carrying a field it does not use. That is the cost of a
/// new bare field, and the clause now says so in its own text.
///
/// It is never COMPARED. `[proto.record.diag]` makes wording a
/// per-implementation quality concern, `[conf.trap.assert]` lets an
/// implementation drop the message entirely, and
/// `[proto.cmp.defined-divergence]` names it — so an implementation that
/// drops it must not become a divergence for doing so.
///
/// `files` is `[proto.record.diag]`'s file index (wolf-lang#437, ruled
/// 2026-09-24; the shape s181 fixed on the issue), additive in the same way:
/// a package-relative path per file a diagnostic's span lies in, the entry at
/// index 0, present only when some diagnostic lies outside the entry — so
/// every single-file record is byte-identical to before. Its partner is the
/// optional integer `file` on each diagnostic ([`validate_diagnostics`]).
/// 0.1.38 refused both keys, so until 0.1.39 every wolfgang record carrying
/// them read to this validator as malformed.
pub const OPTIONAL_FIELDS: [&str; 5] = [
    "stdout_sha256",
    "stdout_inline",
    "warnings",
    "trap_message",
    "files",
];

/// `[proto.record.fields]`: `stdout_inline` is included up to 4096 bytes.
pub const STDOUT_INLINE_LIMIT: usize = 4096;

/// One reason a record is not schema-valid, located by JSON pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError {
    /// JSON pointer to the offending value, e.g. `/diagnostics/0/span`.
    pub pointer: String,
    pub message: String,
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pointer, self.message)
    }
}

/// Every reason a record failed validation. Non-empty by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaErrors(pub Vec<SchemaError>);

impl fmt::Display for SchemaErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let joined = self
            .0
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        f.write_str(&joined)
    }
}

impl std::error::Error for SchemaErrors {}

struct Checker {
    errors: Vec<SchemaError>,
}

impl Checker {
    fn err(&mut self, pointer: impl Into<String>, message: impl Into<String>) {
        self.errors.push(SchemaError {
            pointer: pointer.into(),
            message: message.into(),
        });
    }

    fn string<'a>(&mut self, pointer: &str, value: &'a Value) -> Option<&'a str> {
        match value.as_str() {
            Some(s) => Some(s),
            None => {
                self.err(
                    pointer,
                    format!("expected a string, found {}", kind_of(value)),
                );
                None
            }
        }
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Validates one observation record.
///
/// # Errors
///
/// Returns every violation found, rather than the first: a record that is
/// wrong in three ways should say so once.
pub fn validate(value: &Value) -> Result<(), SchemaErrors> {
    let mut checker = Checker { errors: Vec::new() };

    let Some(object) = value.as_object() else {
        return Err(SchemaErrors(vec![SchemaError {
            pointer: String::new(),
            message: format!(
                "an observation record is a JSON object, found {}",
                kind_of(value)
            ),
        }]));
    };

    for field in REQUIRED_FIELDS {
        if !object.contains_key(field) {
            checker.err(format!("/{field}"), "required field is missing");
        }
    }

    for (key, entry) in object {
        if REQUIRED_FIELDS.contains(&key.as_str()) || OPTIONAL_FIELDS.contains(&key.as_str()) {
            continue;
        }
        // `[proto.record.ext]`: extensions must announce themselves.
        if !key.starts_with("x-") {
            checker.err(
                format!("/{key}"),
                "unknown field; implementation extensions must begin with `x-`",
            );
        } else if entry.is_null() {
            // Not fatal, but a null extension can never participate in
            // equality; flag it so it is not mistaken for data.
            checker.err(format!("/{key}"), "extension key is null");
        }
    }

    if let Some(protocol) = object.get("protocol") {
        match protocol.as_u64() {
            Some(PROTOCOL_VERSION) => {}
            Some(other) => checker.err(
                "/protocol",
                format!("unsupported protocol version {other}; this implementation speaks {PROTOCOL_VERSION}"),
            ),
            None => checker.err(
                "/protocol",
                format!("expected the integer {PROTOCOL_VERSION}, found {}", kind_of(protocol)),
            ),
        }
    }

    for field in ["impl", "impl_version", "commit", "file"] {
        if let Some(entry) = object.get(field) {
            let pointer = format!("/{field}");
            if let Some(text) = checker.string(&pointer, entry)
                && text.is_empty()
            {
                checker.err(pointer, "must not be empty");
            }
        }
    }

    if let Some(entry) = object.get("phase_reached") {
        let pointer = "/phase_reached".to_owned();
        if let Some(text) = checker.string(&pointer, entry)
            && Phase::parse(text).is_none()
        {
            checker.err(
                pointer,
                format!("`{text}` is not a rung of the canonical phase ladder"),
            );
        }
    }

    if let Some(entry) = object.get("seeded")
        && !entry.is_boolean()
    {
        checker.err(
            "/seeded",
            format!("expected a boolean, found {}", kind_of(entry)),
        );
    }

    if let Some(entry) = object.get("verdict") {
        let pointer = "/verdict".to_owned();
        if let Some(text) = checker.string(&pointer, entry)
            && let Err(e) = text.parse::<Verdict>()
        {
            checker.err(pointer, e.to_string());
        }
    }

    let files = validate_files(&mut checker, object.get("files"));
    validate_diagnostics(&mut checker, object.get("diagnostics"), files);
    validate_warnings(&mut checker, object.get("warnings"));
    validate_stdout(&mut checker, object);

    if checker.errors.is_empty() {
        Ok(())
    } else {
        Err(SchemaErrors(checker.errors))
    }
}

/// `[proto.record.diag]`'s file table (wolf-lang#437): an array of non-empty
/// path strings. Returns how many entries a diagnostic's `file` may index —
/// `None` when the key is absent, which makes any `file` an orphan.
fn validate_files(checker: &mut Checker, entry: Option<&Value>) -> Option<usize> {
    let entry = entry?;
    let Some(items) = entry.as_array() else {
        checker.err(
            "/files",
            format!("expected an array of package-relative paths, found {}", kind_of(entry)),
        );
        return Some(0);
    };
    if items.is_empty() {
        checker.err("/files", "a file table is present only when it indexes something; the entry is index 0");
    }
    for (index, item) in items.iter().enumerate() {
        let pointer = format!("/files/{index}");
        if let Some(text) = checker.string(&pointer, item)
            && text.is_empty()
        {
            checker.err(pointer, "must not be empty");
        }
    }
    Some(items.len())
}

fn validate_diagnostics(checker: &mut Checker, entry: Option<&Value>, files: Option<usize>) {
    let Some(entry) = entry else { return };
    let Some(items) = entry.as_array() else {
        checker.err(
            "/diagnostics",
            format!("expected an array, found {}", kind_of(entry)),
        );
        return;
    };

    for (index, item) in items.iter().enumerate() {
        let base = format!("/diagnostics/{index}");
        let Some(fields) = item.as_object() else {
            checker.err(
                base.clone(),
                format!("expected an object, found {}", kind_of(item)),
            );
            continue;
        };
        // `[proto.record.diag]`: exactly {code, span, severity}. A `message`
        // key here is the classic mistake (D22) and is caught by this loop.
        for key in fields.keys() {
            if !matches!(key.as_str(), "code" | "span" | "severity" | "file") {
                checker.err(
                    format!("{base}/{key}"),
                    "a protocol diagnostic carries only `code`, `span`, `severity` and, in a multi-file record, `file` (messages are never part of the protocol)",
                );
            }
        }
        // wolf-lang#437: `file` indexes the record's `files` table. With no
        // table it indexes nothing, and past the table's end it names no file
        // — both are malformed, never read as "the entry".
        if let Some(file) = fields.get("file") {
            let pointer = format!("{base}/file");
            match (file.as_u64(), files) {
                (None, _) => checker.err(
                    pointer,
                    format!("expected an integer index into `files`, found {}", kind_of(file)),
                ),
                (Some(_), None) => checker.err(
                    pointer,
                    "a `file` index with no `files` table indexes nothing",
                ),
                (Some(index), Some(len)) if usize::try_from(index).map_or(true, |i| i >= len) => {
                    checker.err(
                        pointer,
                        format!("index {index} is past the end of `files` ({len} entries)"),
                    );
                }
                _ => {}
            }
        } else if files.is_some() {
            // "When `files` is present, EVERY diagnostic carries `file`,
            // including 0" — so a missing index is not a quiet "entry".
            checker.err(
                format!("{base}/file"),
                "the record carries `files`, so every diagnostic carries a `file` index",
            );
        }
        for key in ["code", "severity"] {
            match fields.get(key) {
                None => checker.err(format!("{base}/{key}"), "required field is missing"),
                Some(value) => {
                    let pointer = format!("{base}/{key}");
                    if let Some(text) = checker.string(&pointer, value)
                        && text.is_empty()
                    {
                        checker.err(pointer, "must not be empty");
                    }
                }
            }
        }
        match fields.get("span") {
            None => checker.err(format!("{base}/span"), "required field is missing"),
            Some(span) => validate_span(checker, &format!("{base}/span"), span),
        }
    }
}

/// `[proto.record.warn]` — an optional array of `{code, span}` entries,
/// nothing else. Severity is never repeated here (the array is warnings by
/// definition), and honest-absent means omission: an implementation that runs
/// no warning analyses leaves the key out entirely.
fn validate_warnings(checker: &mut Checker, entry: Option<&Value>) {
    let Some(entry) = entry else { return };
    let Some(items) = entry.as_array() else {
        checker.err(
            "/warnings",
            format!(
                "expected an array of {{code, span}} entries, found {}",
                kind_of(entry)
            ),
        );
        return;
    };

    for (index, item) in items.iter().enumerate() {
        let base = format!("/warnings/{index}");
        let Some(fields) = item.as_object() else {
            checker.err(
                base.clone(),
                format!("expected an object, found {}", kind_of(item)),
            );
            continue;
        };
        for key in fields.keys() {
            if !matches!(key.as_str(), "code" | "span") {
                checker.err(
                    format!("{base}/{key}"),
                    "a warning observation carries only `code` and `span` — severity is \
                     not repeated ([proto.record.warn])",
                );
            }
        }
        match fields.get("code") {
            None => checker.err(format!("{base}/code"), "required field is missing"),
            Some(value) => {
                let pointer = format!("{base}/code");
                if let Some(text) = checker.string(&pointer, value)
                    && text.is_empty()
                {
                    checker.err(pointer, "must not be empty");
                }
            }
        }
        match fields.get("span") {
            None => checker.err(format!("{base}/span"), "required field is missing"),
            Some(span) => validate_span(checker, &format!("{base}/span"), span),
        }
    }
}

fn validate_span(checker: &mut Checker, pointer: &str, span: &Value) {
    let Some(bounds) = span.as_array() else {
        checker.err(
            pointer,
            format!(
                "expected a two-element byte-offset span, found {}",
                kind_of(span)
            ),
        );
        return;
    };
    if bounds.len() != 2 {
        checker.err(
            pointer,
            format!("a span is [start, end); found {} element(s)", bounds.len()),
        );
        return;
    }
    let mut parsed = [0u64; 2];
    for (index, bound) in bounds.iter().enumerate() {
        match bound.as_u64() {
            Some(n) => parsed[index] = n,
            None => {
                checker.err(
                    format!("{pointer}/{index}"),
                    format!(
                        "expected a non-negative byte offset, found {}",
                        kind_of(bound)
                    ),
                );
                return;
            }
        }
    }
    if parsed[0] > parsed[1] {
        checker.err(
            pointer,
            format!("span start {} is past its end {}", parsed[0], parsed[1]),
        );
    }
}

fn validate_stdout(checker: &mut Checker, object: &serde_json::Map<String, Value>) {
    let sha = object.get("stdout_sha256");
    if let Some(value) = sha
        && !value.is_null()
    {
        let pointer = "/stdout_sha256".to_owned();
        if let Some(text) = checker.string(&pointer, value) {
            let hex = text.len() == 64 && text.chars().all(|c| c.is_ascii_hexdigit());
            if !hex {
                checker.err(pointer, "expected a 64-character hex sha256 digest");
            }
        }
    }

    let inline = object.get("stdout_inline");
    if let Some(value) = inline
        && !value.is_null()
    {
        let pointer = "/stdout_inline".to_owned();
        if let Some(text) = checker.string(&pointer, value)
            && text.len() > STDOUT_INLINE_LIMIT
        {
            checker.err(
                pointer,
                format!(
                    "inline stdout is capped at {STDOUT_INLINE_LIMIT} bytes; found {}",
                    text.len()
                ),
            );
        }
    }

    // `[proto.record.fields]`: the hash is always present when the program
    // wrote output, so inline output without a digest is never well-formed.
    let has_inline = inline.is_some_and(|v| !v.is_null());
    let has_sha = sha.is_some_and(|v| !v.is_null());
    if has_inline && !has_sha {
        checker.err(
            "/stdout_sha256",
            "required whenever the program wrote output (`stdout_inline` is present)",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_record() -> Value {
        json!({
            "protocol": 1,
            "impl": "example",
            "impl_version": "0.0.1",
            "commit": "abc1234",
            "file": "corpus/hello.lu",
            "phase_reached": "run",
            "seeded": false,
            "diagnostics": [],
            "verdict": "exit(0)",
            "stdout_sha256": null,
            "stdout_inline": null
        })
    }

    fn reasons(value: &Value) -> String {
        validate(value).expect_err("must be rejected").to_string()
    }

    #[test]
    fn a_minimal_record_validates() {
        assert_eq!(validate(&valid_record()), Ok(()));
    }

    #[test]
    fn the_optional_stdout_fields_may_be_omitted_entirely() {
        let mut record = valid_record();
        let object = record.as_object_mut().expect("object");
        object.remove("stdout_sha256");
        object.remove("stdout_inline");
        assert_eq!(validate(&record), Ok(()));
    }

    #[test]
    fn every_required_field_is_required() {
        for field in REQUIRED_FIELDS {
            let mut record = valid_record();
            record.as_object_mut().expect("object").remove(field);
            let message = reasons(&record);
            assert!(
                message.contains(&format!("/{field}: required field is missing")),
                "{message}"
            );
        }
    }

    #[test]
    fn a_future_protocol_version_is_rejected() {
        let mut record = valid_record();
        record["protocol"] = json!(2);
        assert!(reasons(&record).contains("unsupported protocol version 2"));
    }

    #[test]
    fn a_trap_message_is_accepted_and_an_unknown_bare_key_is_still_refused() {
        // wolf-interp#129, `[proto.record.trap]`. The two halves must be
        // asserted together or the first one is not a finding: accepting
        // `trap_message` is only interesting if the seal it widens still
        // closes on everything else. The second half is the red one — remove
        // `trap_message` from OPTIONAL_FIELDS and the first assert fails;
        // replace the `!key.starts_with("x-")` guard with `true` and the
        // second does.
        let mut record = valid_record();
        record["verdict"] = json!("trap(assert)");
        record["trap_message"] = json!("balance must stay non-negative");
        assert_eq!(validate(&record), Ok(()));

        // Absent is still fine: the field is optional, not required.
        let mut record = valid_record();
        record["verdict"] = json!("trap(assert)");
        assert_eq!(validate(&record), Ok(()));

        // A bare stranger that merely LOOKS like the new field is not it.
        let mut record = valid_record();
        record["trap_msg"] = json!("balance must stay non-negative");
        let message = reasons(&record);
        assert!(message.contains("/trap_msg"), "{message}");
        assert!(message.contains("must begin with `x-`"), "{message}");
    }

    /// wolf-lang#437 (ruled 2026-09-24; the shape s181 fixed on the issue):
    /// a top-level optional `files` array of package-relative paths, index 0
    /// the entry, and an optional integer `file` on each diagnostic indexing
    /// it — both absent for a single-file package, a record without them
    /// unchanged. Accepted in the shape, refused out of it: a `file` with no
    /// `files` or out of its range is malformed, `files` must be strings, and
    /// `warnings` entries carry no `file`. The old seal still closes.
    #[test]
    fn a_file_index_on_diagnostics_is_admitted_in_its_shape_and_refused_out_of_it() {
        let mut record = valid_record();
        record["verdict"] = json!("fail(E0302)");
        record["phase_reached"] = json!("resolve");
        record["files"] = json!(["main.lu", "geometry/shapes.lu"]);
        record["diagnostics"] =
            json!([{"code": "E0302", "span": [245, 249], "severity": "error", "file": 1}]);
        assert_eq!(validate(&record), Ok(()));

        // Index 0 names the entry and is legal; every diagnostic may carry it.
        record["diagnostics"] = json!([
            {"code": "E0302", "span": [245, 249], "severity": "error", "file": 1},
            {"code": "W0313", "span": [0, 3], "severity": "warning", "file": 0}
        ]);
        assert_eq!(validate(&record), Ok(()));

        // Without the keys the record is exactly what it always was.
        assert_eq!(validate(&valid_record()), Ok(()));

        // A `file` with no `files` has nothing to index.
        let mut orphan = valid_record();
        orphan["diagnostics"] = json!([{"code": "E0302", "span": [1, 2], "severity": "error", "file": 0}]);
        let message = reasons(&orphan);
        assert!(message.contains("/diagnostics/0/file"), "{message}");

        // With a table present, a diagnostic without an index is malformed:
        // every diagnostic carries `file` once `files` exists.
        let mut partial = record.clone();
        partial["diagnostics"] = json!([
            {"code": "E0302", "span": [245, 249], "severity": "error", "file": 1},
            {"code": "W0313", "span": [0, 3], "severity": "warning"}
        ]);
        assert!(reasons(&partial).contains("/diagnostics/1/file"), "{}", reasons(&partial));

        // Out of range.
        let mut far = record.clone();
        far["diagnostics"] = json!([{"code": "E0302", "span": [1, 2], "severity": "error", "file": 2}]);
        assert!(reasons(&far).contains("/diagnostics/0/file"), "{}", reasons(&far));

        // Not an index.
        let mut named = record.clone();
        named["diagnostics"] =
            json!([{"code": "E0302", "span": [1, 2], "severity": "error", "file": "geometry/shapes.lu"}]);
        assert!(reasons(&named).contains("/diagnostics/0/file"), "{}", reasons(&named));

        // `files` holds paths.
        let mut bad_files = record.clone();
        bad_files["files"] = json!(["main.lu", 7]);
        assert!(reasons(&bad_files).contains("/files/1"), "{}", reasons(&bad_files));

        // `[proto.record.warn]` is unchanged: a warnings entry carries no file.
        let mut warned = record.clone();
        warned["warnings"] = json!([{"code": "W0313", "span": [0, 3], "file": 0}]);
        assert!(reasons(&warned).contains("/warnings/0/file"), "{}", reasons(&warned));

        // And a diagnostic key that is neither of the known four is still refused.
        let mut stranger = record.clone();
        stranger["diagnostics"] =
            json!([{"code": "E0302", "span": [1, 2], "severity": "error", "path": "x.lu"}]);
        assert!(reasons(&stranger).contains("/diagnostics/0/path"), "{}", reasons(&stranger));
    }

    #[test]
    fn non_extension_strangers_are_rejected_but_x_keys_pass() {
        let mut record = valid_record();
        record["oracle_trace"] = json!(["retag"]);
        assert!(reasons(&record).contains("must begin with `x-`"));

        let mut record = valid_record();
        record["x-oracle-trace"] = json!(["retag"]);
        record["x-ub-row"] = json!("P1");
        assert_eq!(validate(&record), Ok(()));
    }

    #[test]
    fn diagnostics_are_code_span_severity_and_nothing_else() {
        let mut record = valid_record();
        record["diagnostics"] =
            json!([{ "code": "E1002", "span": [120, 133], "severity": "error" }]);
        assert_eq!(validate(&record), Ok(()));

        record["diagnostics"] = json!([{
            "code": "E1002", "span": [120, 133], "severity": "error",
            "message": "cannot borrow"
        }]);
        assert!(reasons(&record).contains("messages are never part of the protocol"));
    }

    #[test]
    fn spans_are_two_non_negative_byte_offsets() {
        let mut record = valid_record();
        for (span, needle) in [
            (json!([1]), "found 1 element"),
            (json!([1, 2, 3]), "found 3 element"),
            (json!([-1, 5]), "non-negative byte offset"),
            (json!(["a", "b"]), "non-negative byte offset"),
            (json!([9, 4]), "past its end"),
            (json!("120..133"), "two-element byte-offset span"),
        ] {
            record["diagnostics"] = json!([{ "code": "E1", "span": span, "severity": "error" }]);
            let message = reasons(&record);
            assert!(message.contains(needle), "expected {needle}, got {message}");
        }
    }

    #[test]
    fn verdicts_and_phases_come_from_their_closed_sets() {
        let mut record = valid_record();
        record["verdict"] = json!("trap(segfault)");
        assert!(reasons(&record).contains("unknown trap kind"));

        let mut record = valid_record();
        record["phase_reached"] = json!("codegen");
        assert!(reasons(&record).contains("canonical phase ladder"));
    }

    #[test]
    fn stdout_digest_shape_is_enforced() {
        let mut record = valid_record();
        record["stdout_inline"] = json!("hello, wolf\n");
        assert!(reasons(&record).contains("required whenever the program wrote output"));

        record["stdout_sha256"] = json!("not-a-digest");
        assert!(reasons(&record).contains("64-character hex"));

        record["stdout_sha256"] =
            json!("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08");
        assert_eq!(validate(&record), Ok(()));
    }

    #[test]
    fn inline_stdout_is_capped() {
        let mut record = valid_record();
        record["stdout_inline"] = json!("x".repeat(STDOUT_INLINE_LIMIT + 1));
        record["stdout_sha256"] =
            json!("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08");
        assert!(reasons(&record).contains("capped at 4096 bytes"));
    }

    #[test]
    fn a_record_must_be_an_object() {
        assert!(reasons(&json!([1, 2])).contains("is a JSON object"));
    }

    #[test]
    fn all_violations_are_reported_at_once() {
        let record = json!({ "protocol": 2, "impl": "", "seeded": "no" });
        let SchemaErrors(errors) = validate(&record).expect_err("must be rejected");
        assert!(errors.len() > 3, "{errors:?}");
    }
}

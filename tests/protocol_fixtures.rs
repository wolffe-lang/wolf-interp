//! `[proto.harness.fixtures]` — the protocol's own tests.
//!
//! `upstream/corpus/protocol/` ships four canned observation records that
//! *every* conforming schema validator must accept or reject exactly as named.
//! This file is where wolf-interp proves it is one, independently of the
//! compiler's validator: same fixtures, different code.

use std::path::{Path, PathBuf};

use wolf_interp::protocol::{ObservationRecord, Verdict};
use wolf_interp::schema;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus/protocol")
}

fn fixture(name: &str) -> serde_json::Value {
    let path = fixtures_dir().join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read fixture {}: {e}\n\
             hint: the corpus is a pinned submodule — run `git submodule update --init upstream`",
            path.display()
        )
    });
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("fixture {name} is not JSON: {e}"))
}

#[test]
fn valid_json_is_accepted() {
    assert_eq!(schema::validate(&fixture("valid.json")), Ok(()));
}

#[test]
fn with_extensions_json_is_accepted() {
    // `[proto.record.ext]`: `x-ub-row` and `x-oracle-trace` are legal.
    assert_eq!(schema::validate(&fixture("with-extensions.json")), Ok(()));
}

#[test]
fn with_warnings_json_is_accepted() {
    // `[proto.record.warn]` (s67, additive within protocol 1): the fixture
    // carries the `warnings` array beside the same observations in
    // `diagnostics` with `"severity": "warning"` — validators accept
    // records with or without the array.
    assert_eq!(schema::validate(&fixture("with-warnings.json")), Ok(()));
}

#[test]
fn wrong_version_json_is_rejected() {
    let errors = schema::validate(&fixture("wrong-version.json")).expect_err("must be rejected");
    assert!(
        errors
            .to_string()
            .contains("unsupported protocol version 2"),
        "{errors}"
    );
}

#[test]
fn missing_field_json_is_rejected() {
    let errors = schema::validate(&fixture("missing-field.json")).expect_err("must be rejected");
    assert!(
        errors
            .to_string()
            .contains("/phase_reached: required field is missing"),
        "{errors}"
    );
}

#[test]
fn the_fixture_set_is_the_one_the_spec_names() {
    let mut names: Vec<String> = std::fs::read_dir(fixtures_dir())
        .expect("fixtures directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "missing-field.json",
            "valid.json",
            "with-extensions.json",
            "with-warnings.json",
            "wrong-version.json"
        ]
    );
}

#[test]
fn accepted_fixtures_round_trip_through_the_typed_record() {
    for name in ["valid.json", "with-extensions.json"] {
        let value = fixture(name);
        let record: ObservationRecord =
            serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));

        let reserialized: serde_json::Value =
            serde_json::to_value(&record).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(reserialized, value, "{name} did not survive a round trip");
        assert_eq!(schema::validate(&reserialized), Ok(()), "{name}");
    }
}

#[test]
fn the_extension_fixture_carries_its_extensions_through() {
    let record: ObservationRecord =
        serde_json::from_value(fixture("with-extensions.json")).expect("deserializes");
    assert_eq!(record.verdict, Verdict::Ub("mem.ub".to_owned()));
    assert_eq!(
        record.extensions.keys().collect::<Vec<_>>(),
        vec!["x-oracle-trace", "x-ub-row"]
    );
}

#[test]
fn our_own_records_validate() {
    // The record `conform-run` emits today, checked against the same validator
    // the fixtures exercise.
    let record = wolf_interp::unsupported_record(Path::new("upstream/corpus/hello.lu"));
    let value = serde_json::to_value(&record).expect("serializes");
    assert_eq!(schema::validate(&value), Ok(()));
}

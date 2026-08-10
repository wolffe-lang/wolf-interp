//! is09 — the conformance bundle's own gates.
//!
//! Determinism (re-export is byte-identical — the I10 spirit), integrity
//! (a tampered bundle is refused), the consumption dry-run (pull → verify →
//! diff, with recorded results standing in for a second implementation), and
//! the coverage **ratchet**: the covered-anchor count may grow, never
//! shrink. The cross-OS half of determinism — linux/mac/windows bundles
//! hashing identical — is a CI assertion (`bundle determinism` +
//! `bundle-identical` in `.github/workflows/ci.yml`), because one machine
//! cannot test three.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use wolf_interp::export::{self, CheckImpl, ExportOptions, ExportSummary};

/// The coverage ratchet's floor: the covered-anchor count at the is09
/// export, pin `cbde620`. A PR may move this number **up** (backfill, new
/// corpus files, a pin bump that adds cited clauses) and must then raise the
/// floor; a PR that drops it below the floor — deleting a tagged test,
/// losing a `conforms:` line — fails here, which is the ratchet doing its
/// job. Never lower this without the closeout-level negotiation the sprint
/// requires.
const RATCHET_FLOOR: usize = 83;

/// The registry size at pin `cbde620`. Moves only with a pin bump, and then
/// deliberately.
const ANCHORS_TOTAL: usize = 281;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn options(out: &str) -> ExportOptions {
    let root = crate_root();
    let upstream = root.join(wolf_interp::upstream_root());
    ExportOptions {
        corpus: upstream.join("corpus"),
        spec: upstream.join("spec"),
        suites: root.join("tests"),
        repl_doc: root.join("docs/repl.md"),
        pin: root.join("vendor/upstream/PIN"),
        out: root.join("target").join(out),
    }
}

/// One bundle, exported once, shared by every test in this binary.
fn bundle() -> &'static (ExportOptions, ExportSummary) {
    static BUNDLE: OnceLock<(ExportOptions, ExportSummary)> = OnceLock::new();
    BUNDLE.get_or_init(|| {
        let options = options("test-bundle");
        let summary = export::export(&options).expect("the export must succeed");
        (options, summary)
    })
}

/// Every file under a directory, as (relative slash path, bytes), sorted.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .expect("readable")
            .map(|e| e.expect("entry").path())
            .collect();
        entries.sort();
        for entry in entries {
            if entry.is_dir() {
                walk(&entry, root, out);
            } else {
                let relative = entry
                    .strip_prefix(root)
                    .expect("under root")
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((relative, std::fs::read(&entry).expect("readable")));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn re_export_is_byte_identical() {
    let (first_options, first) = bundle();
    let second_options = options("test-bundle-again");
    let second = export::export(&second_options).expect("the re-export must succeed");
    assert_eq!(first.bundle_sha256, second.bundle_sha256);

    // The root hash covers every file except the manifest; compare the whole
    // trees byte-for-byte so the manifest is held to the same standard.
    let a = tree(&first_options.out);
    let b = tree(&second_options.out);
    assert_eq!(a.len(), b.len(), "the two exports differ in file count");
    for ((path_a, bytes_a), (path_b, bytes_b)) in a.iter().zip(&b) {
        assert_eq!(path_a, path_b);
        assert_eq!(bytes_a, bytes_b, "{path_a} differs between two exports");
    }
}

#[test]
fn the_bundle_opens_verified_and_its_records_reference_bundled_programs() {
    let (options, summary) = bundle();
    let verified = export::open_bundle(&options.out).expect("a fresh bundle verifies");
    assert_eq!(verified.pin, summary.pin);
    assert_eq!(verified.bundle_sha256, summary.bundle_sha256);
    assert_eq!(verified.expected.len(), summary.records);

    // Every record names a program that ships in the bundle, by its
    // bundle-relative slash path, in sorted order (two exports of the same
    // trees diff cleanly).
    let mut previous = String::new();
    for record in &verified.expected {
        assert!(
            options.out.join(&record.file).is_file(),
            "{} is recorded but not bundled",
            record.file
        );
        assert!(
            record.file.starts_with("corpus/") || record.file.starts_with("suite/"),
            "{}",
            record.file
        );
        assert!(!record.file.contains('\\'), "{}", record.file);
        assert!(previous < record.file, "records must be sorted by file");
        previous.clone_from(&record.file);
    }
}

#[test]
fn the_pin_and_the_counts_are_the_ones_this_sprint_recorded() {
    // The count ledger, lupin 0.1.2 edition (pin `a0c4564`): 175 corpus
    // files (159 entries + 16 members) + 39 suite programs (9 UB triggers +
    // 9 twins, 9 fault litmuses + 7 twins, 5 witnesses) = 214 programs, 198
    // entries conform-run into reference records. The pin brought the E0410
    // fail-files and the unsafe/checked memory tier (see the corpus-harness
    // ledger). A pin bump or a new suite file moves these numbers — move
    // them *deliberately*, here and in the corpus-harness ledger.
    let (_, summary) = bundle();
    assert_eq!(summary.pin, "a0c4564f246e46dd14d66f82e7d106059b9d076a");
    assert_eq!(summary.programs, 214);
    assert_eq!(summary.records, 198);
    assert_eq!(summary.anchors_total, ANCHORS_TOTAL);
}

#[test]
fn coverage_is_ratcheted() {
    let (options, summary) = bundle();
    assert_eq!(summary.anchors_total, ANCHORS_TOTAL);
    assert!(
        summary.anchors_covered >= RATCHET_FLOOR,
        "coverage DROPPED: {} covered anchors, the ratchet floor is {RATCHET_FLOOR}. \
         A tagged test or a `conforms:` line was lost — restore it, or renegotiate \
         the floor with the gap list on the table (never silently).",
        summary.anchors_covered
    );
    // The floor tracks the count exactly: growth is recorded by raising it,
    // so the *next* drop is caught at the new water line, not the old one.
    assert_eq!(
        summary.anchors_covered, RATCHET_FLOOR,
        "coverage GREW (good!) — raise RATCHET_FLOOR to {} in the same commit",
        summary.anchors_covered
    );

    // The red path, demonstrated: dropping one covered clause's tests leaves
    // a count below the floor. This is the "red-test PR" of the acceptance
    // criterion, run against the real matrix rather than planted in a mock.
    let matrix = std::fs::read_to_string(options.out.join("coverage/matrix.jsonl"))
        .expect("the matrix ships in the bundle");
    let covered_lines = matrix
        .lines()
        .filter(|line| line.contains("\"status\":\"covered\""))
        .count();
    assert_eq!(covered_lines, summary.anchors_covered, "matrix vs manifest");
    assert!(
        covered_lines - 1 < RATCHET_FLOOR,
        "losing any single covered anchor must fall below the floor"
    );
}

#[test]
fn the_consumption_dry_run_is_green() {
    // The acceptance criterion: "a harness stands in for the compiler
    // (replaying recorded interpreter results) and exercises the full
    // pull → run → diff path". Replaying the bundle's own records must be
    // perfect agreement — anything else means the pipeline, not a program,
    // is broken.
    let (options, summary) = bundle();
    let verified = export::open_bundle(&options.out).expect("verifies");
    let outcome = export::check(
        &verified,
        &CheckImpl::Replay(options.out.join("expected/records.jsonl")),
        Duration::from_millis(30_000),
    )
    .expect("the dry run runs");
    assert_eq!(outcome.compared, summary.records);
    assert_eq!(outcome.divergences, Vec::new());
}

#[test]
fn a_tampered_bundle_is_refused() {
    let tampered = options("test-bundle-tampered");
    export::export(&tampered).expect("exports");
    let hello = tampered.out.join("corpus/hello.lu");
    let mut bytes = std::fs::read(&hello).expect("readable");
    bytes.extend_from_slice(b"\n// tampered\n");
    std::fs::write(&hello, bytes).expect("writable");
    let err = export::open_bundle(&tampered.out).expect_err("a tampered bundle must be refused");
    assert!(err.contains("integrity"), "{err}");
    assert!(err.contains("corpus/hello.lu"), "{err}");

    // A missing file is refused just as loudly as a modified one.
    let truncated = options("test-bundle-truncated");
    export::export(&truncated).expect("exports");
    std::fs::remove_file(truncated.out.join("vocab/traps.json")).expect("removable");
    let err = export::open_bundle(&truncated.out).expect_err("a truncated bundle must be refused");
    assert!(err.contains("vocab/traps.json"), "{err}");
}

#[test]
fn the_vocabularies_ship_closed() {
    let (options, _) = bundle();
    let traps: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(options.out.join("vocab/traps.json")).expect("ships"),
    )
    .expect("json");
    let kinds = traps["kinds"].as_array().expect("kinds");
    assert_eq!(kinds.len(), 12, "[conf.trap.set] is the closed twelve");
    assert_eq!(kinds[0], "overflow");
    assert_eq!(kinds[11], "deadlock");
    assert_eq!(traps["closed"], true);

    let rows: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(options.out.join("vocab/ub-rows.json")).expect("ships"),
    )
    .expect("json");
    let rows = rows["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 11, "[mem.ub] is the closed eleven");
    for row in rows {
        // D2 on the wire: every row names what it licenses, and its coverage
        // status is stated rather than implied ([proto.record.unsupported]'s
        // honesty contract, applied to the matrix).
        assert!(!row["licenses"].as_str().expect("licenses").is_empty());
        assert!(!row["coverage"].as_str().expect("coverage").is_empty());
    }
}

#[test]
fn the_anchor_registry_cross_check_holds_at_this_pin() {
    // Target 1 of the sprint: the pinned `anchors.json` is shared *data* —
    // consumed, and cross-checked against an independent extraction of the
    // spec markdown. A mismatch is an upstream finding; at this pin the two
    // agree exactly, and the export refuses to run when they stop agreeing
    // (`export::cross_check_registry`, red-tested in the library).
    let spec = crate_root().join(wolf_interp::upstream_root()).join("spec");
    let text = std::fs::read_to_string(spec.join("anchors.json")).expect("readable");
    let value: serde_json::Value = serde_json::from_str(&text).expect("json");
    let registry: std::collections::BTreeMap<String, String> = value["anchors"]
        .as_object()
        .expect("anchors")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
        .collect();
    assert_eq!(registry.len(), ANCHORS_TOTAL);
    export::cross_check_registry(&spec, &registry).expect("registry and extraction agree");
}

#[test]
fn the_coverage_table_is_the_honesty_document() {
    let (options, summary) = bundle();
    let rendered = std::fs::read_to_string(options.out.join("coverage/coverage.md"))
        .expect("the table ships IN the bundle");
    // The headline numbers are in the prose, so a human reading the artifact
    // sees the same figures the manifest carries.
    assert!(rendered.contains(&format!(
        "{} / {} registered anchors",
        summary.anchors_covered, summary.anchors_total
    )));
    // The UB enumeration's dedicated section (D2): all eleven rows, each
    // detected-and-paired or carrying its named reason.
    assert!(rendered.contains("The UB enumeration"));
    for id in [
        "P1", "P2", "P3", "P4", "P5", "P6", "L1", "L2", "T1", "T2", "C1",
    ] {
        assert!(rendered.contains(&format!("| {id} |")), "{id} missing");
    }
    assert!(rendered.contains("deferred(concurrency)"));
    assert!(rendered.contains("unreachable at this tier"));
    // The debt list is printed in full — part of the product, not a private
    // shame file.
    assert!(rendered.contains("Debt list"));
}

#[test]
fn the_cli_speaks_the_bundle_end_to_end() {
    // The process contract: export prints the summary and exits 0; check in
    // replay mode is the dry run and exits 0; check of a tampered bundle is
    // a *tool* error (exit 2) — corrupted expectations are never verdicts.
    let root = crate_root();
    let out = root.join("target/test-bundle-cli");
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_lupin"))
            .args(args)
            .current_dir(&root)
            .output()
            .expect("the binary runs")
    };
    let exported = run(&[
        "conformance",
        "export",
        "--out",
        out.to_str().expect("utf-8"),
        "--json",
    ]);
    assert!(exported.status.success());
    let summary: serde_json::Value =
        serde_json::from_slice(&exported.stdout).expect("the summary is JSON");
    assert_eq!(summary["bundle_sha256"], bundle().1.bundle_sha256);

    let records = out.join("expected/records.jsonl");
    let checked = run(&[
        "conformance",
        "check",
        out.to_str().expect("utf-8"),
        "--replay",
        records.to_str().expect("utf-8"),
    ]);
    assert!(checked.status.success(), "{checked:?}");
    let stdout = String::from_utf8_lossy(&checked.stdout);
    assert!(stdout.contains("divergences: 0"), "{stdout}");

    // Exactly one counterparty, or the tool refuses.
    let neither = run(&["conformance", "check", out.to_str().expect("utf-8")]);
    assert_eq!(neither.status.code(), Some(2));
}

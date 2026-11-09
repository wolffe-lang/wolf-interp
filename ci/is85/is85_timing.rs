//! is85's measurement harness: one walk of the pinned corpus,
//! `observe_record(path, source, None)` per file exactly as run_corpus's
//! `entries()` calls it, timed per file. Writes a TSV to $IS85_OUT.
//!
//! Not a test target here: copy it to `tests/` of a checkout at the head to
//! measure, and run `IS85_OUT=walk.tsv cargo test --test is85_timing`.
//! `ci/is85/local-walk-<head>.tsv` are its output.
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use wolf_interp::corpus::{self, Outcome};

#[test]
fn is85_time_every_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus");
    let out = std::env::var("IS85_OUT").expect("IS85_OUT names the output TSV");
    let t0 = Instant::now();
    let report = corpus::walk(&root, None).expect("walkable");
    let walk_us = t0.elapsed().as_micros();
    let mut f = std::fs::File::create(&out).expect("creatable");
    writeln!(f, "#walk_us\t{walk_us}\tfiles\t{}", report.files.len()).unwrap();
    let mut total = 0u128;
    for file in &report.files {
        let entry = matches!(file.outcome, Outcome::Entry(_));
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let t = Instant::now();
        let (record, _) = wolf_interp::observe_record(&full, &source, None);
        let us = t.elapsed().as_micros();
        total += us;
        writeln!(
            f,
            "{}\t{}\t{}\t{}\t{}",
            file.path, entry, us, record.verdict, record.phase_reached
        )
        .unwrap();
    }
    writeln!(f, "#total_us\t{total}").unwrap();
}

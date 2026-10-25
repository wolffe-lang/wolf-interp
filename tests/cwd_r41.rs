//! New a5378e3f rows must preserve cwd identity and host-specific refusals.
use std::path::PathBuf;
use std::process::Command;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;

fn corpus(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
        .join(path)
}

#[test]
fn chdir_round_trip_matches_the_corpus_and_replays_byte_for_byte() {
    let file = corpus("os/chdir_relative.lu");
    let source = std::fs::read(&file).unwrap();
    let (first, observed) = wolf_interp::observe_record(&file, &source, None);
    let (second, _) = wolf_interp::observe_record(&file, &source, None);
    assert_eq!(first, second, "sandbox names must not leak into records");
    assert_eq!(first.phase_reached, Phase::Run);
    assert_eq!(first.verdict, Verdict::Exit(0), "{first:?}");
    assert_eq!(
        observed.stdout,
        "moved=true\nrelative_read=found\ncwd_ends_inner=true\nup_sees_inner=true\nback=true\nmissing=not_found\nonto_a_file=io\ncleaned=true\n"
    );
}

#[test]
fn the_cli_replays_the_chdir_record_exactly() {
    let run = || {
        let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
            .args(["conform-run", "--json"])
            .arg(corpus("os/chdir_relative.lu"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        output.stdout
    };
    let first = run();
    assert_eq!(first, run());
    let record: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(record["verdict"], "exit(0)");
    assert!(record["stdout_inline"].as_str().unwrap().contains("back=true\n"));
}

#[test]
fn host_rows_are_run_on_windows_and_named_refusals_on_unix() {
    for (path, name) in [
        ("os/ids_read.lu", "os_uid"),
        ("os/signal_reserved_rows.lu", "os_signal_realtime"),
    ] {
        let file = corpus(path);
        let (record, _) = wolf_interp::observe_record(&file, &std::fs::read(&file).unwrap(), None);
        if cfg!(windows) {
            // The programs handle the host's unsupported ERROR ROW, which
            // is not an unsupported observation or a skipped test.
            assert_eq!(record.phase_reached, Phase::Run, "{path}: {record:?}");
            assert_eq!(record.verdict, Verdict::Exit(0), "{path}: {record:?}");
        } else {
            assert_eq!(record.verdict, Verdict::Unsupported, "{path}: {record:?}");
            assert_eq!(record.phase_reached, Phase::Resolve);
            let reason = record.extensions["x-unsupported"].as_str().unwrap();
            assert!(reason.contains(name), "{path}: {reason}");
        }
    }
}

//! s215 (wolf-lang's `[os.proc.fds]`, `[os.proc.pipe]`, `[os.fs.chdir]`,
//! `[os.fs.isatty]`; pelt's H2) — the mirror, at its witnesses.
//!
//! The programs under `tests/fixtures/proc_fd_s215/` are wolf-lang s215's
//! three corpus rows and six unix fixtures, byte for byte, so the answers
//! here are the ones the compiler's gate (`proc_fd_lanes.rs`) asserts of
//! every machine. Each runs under `lupin conform-run --json` in a scratch
//! directory of its own (a live run: relative paths land there).
//!
//! What this machine does NOT mirror is asserted on its words: a map that
//! names a descriptor above 2, or closes one, is refused BY NAME — placing
//! or closing a child's descriptor between fork and exec needs `unsafe`,
//! which this crate forbids, and stable std has no safe call for it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(dir.join("target")).expect("scratch created");
    dir
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/proc_fd_s215")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// `lupin conform-run <entry> --json` in `dir`, 0 the null device and 1, 2
/// piped; the record.
fn observe(entry: &Path, dir: &Path) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("lupin observes");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str(l.trim()).ok())
        .unwrap_or_else(|| panic!("a record: {text}"))
}

fn runs(name: &str, want: &str) {
    let dir = scratch(&format!("s215-{}", name.replace('.', "_")));
    let rec = observe(&fixture(name), &dir);
    assert_eq!(rec["verdict"], "exit(0)", "{name}: {rec}");
    assert_eq!(rec["stdout_inline"], want, "{name}: {rec}");
}

fn source(dir: &Path, text: &str) -> PathBuf {
    let p = dir.join("main.lu");
    std::fs::write(&p, text).expect("written");
    p
}

// --------------------------------------------- the corpus rows (every host)

#[test]
fn a_pipe_round_trips_inside_one_process() {
    runs(
        "pipe_round_trip.lu",
        "ends_above_2=true distinct=true\nread=howl\nthen=eof\n\
write_end_tty=false read_end_tty=false\nclosed_tty=io forged_tty=io\nwrite_after_close=io\n",
    );
}

#[test]
fn chdir_then_a_relative_open() {
    runs(
        "chdir_relative.lu",
        "moved=true\nrelative_read=found\ncwd_ends_inner=true\nup_sees_inner=true\nback=true\n\
missing=not_found\nonto_a_file=io\ncleaned=true\n",
    );
}

#[test]
fn a_bad_map_is_invalid_before_any_child() {
    runs(
        "spawn_fds_rows.lu",
        "odd=invalid\nout_of_range=invalid\nnegative_target=invalid\nrepeated=invalid\n\
bad_source=invalid\nempty_exe=not_found\nno_program=not_found\n",
    );
}

// ------------------------------------------------------- the unix fixtures

#[cfg(unix)]
#[test]
fn a_two_stage_pipeline_from_argv() {
    runs("pipeline.lu", "lines=3 printf=0 wc=0\n");
}

#[cfg(unix)]
#[test]
fn stderr_joins_stdout_on_a_pipe() {
    runs(
        "stderr_merge.lu",
        "merged named=true failed=true\nstdout_only named=false failed=true\n",
    );
}

#[cfg(unix)]
#[test]
fn a_child_starts_in_the_moved_directory() {
    runs("chdir_spawn.lu", "child_lists=mark.txt code=0\n");
}

#[cfg(unix)]
#[test]
fn isatty_is_false_under_a_pipe() {
    runs("isatty.lu", "in=false err=false pipe=false\n");
}

/// The child's `/dev/fd` equals a control's: nothing of this machine's leaks.
#[cfg(unix)]
#[test]
fn no_descriptor_leaks_into_a_child() {
    let dir = scratch("s215-no_leak");
    let control = Command::new("ls")
        .arg("/dev/fd/")
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .expect("ls runs");
    let listing: Vec<String> = String::from_utf8_lossy(&control.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let rec = observe(&fixture("no_leak.lu"), &dir);
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(
        rec["stdout_inline"],
        format!("listing={} code=0\n", listing.join(",")),
        "{rec}"
    );
}

/// fd 5 — and a close — are refused by name, with the clause's reason.
#[cfg(unix)]
#[test]
fn a_descriptor_above_two_or_a_close_is_refused_by_name() {
    let dir = scratch("s215-fd5");
    let rec = observe(&fixture("fd_five.lu"), &dir);
    assert_eq!(rec["verdict"], "unsupported", "{rec}");
    let words = rec["x-unsupported"].as_str().unwrap_or_default();
    assert!(
        words.contains("names a descriptor above 2 in os_spawn_fds"),
        "{words}"
    );
    let dir = scratch("s215-close");
    let rec = observe(
        &source(
            &dir,
            "fn main() -> !int {\n\
             \x20   let h = os_spawn_fds(\"true\", List[str](), [2, 0 - 1])?\n\
             \x20   os_wait(h)?\n\
             }\n",
        ),
        &dir,
    );
    assert_eq!(rec["verdict"], "unsupported", "{rec}");
    let words = rec["x-unsupported"].as_str().unwrap_or_default();
    assert!(
        words.contains("closes a descriptor in os_spawn_fds"),
        "{words}"
    );
}

/// A closed or forged source is `io`, with no child started; a map's shape
/// is decided first, so a forged source in an odd map is still `invalid`.
#[test]
fn a_forged_source_is_io_and_shape_comes_first() {
    let dir = scratch("s215-forged");
    let rec = observe(
        &source(
            &dir,
            "fn tag(map: List[int]) -> str {\n\
             \x20   let h = os_spawn_fds(\"lupin-s215-no-such-program\", List[str](), map) else |e| {\n\
             \x20       return match e {\n\
             \x20           unsupported => \"unsupported\",\n\
             \x20           invalid => \"invalid\",\n\
             \x20           not_found => \"not_found\",\n\
             \x20           denied => \"denied\",\n\
             \x20           io => \"io\",\n\
             \x20       }\n\
             \x20   }\n\
             \x20   \"spawned\"\n\
             }\n\
             fn main() -> !int {\n\
             \x20   let a = tag([1, 999999])\n\
             \x20   let b = tag([1, 999999, 2])\n\
             \x20   print(\"forged={a} odd={b}\")\n\
             \x20   0\n\
             }\n",
        ),
        &dir,
    );
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    let want = if cfg!(windows) {
        "forged=unsupported odd=invalid\n"
    } else {
        "forged=io odd=invalid\n"
    };
    assert_eq!(rec["stdout_inline"], want, "{rec}");
}

/// A directory outside the served tree is declined by name, as an open
/// there is (`[os.fs.path.domain]`); the working directory does not move.
#[test]
fn chdir_out_of_the_served_tree_is_declined_by_name() {
    let dir = scratch("s215-outside");
    let rec = observe(
        &source(
            &dir,
            "fn main() -> !int {\n\
             \x20   os_chdir(\"..\")?\n\
             \x20   0\n\
             }\n",
        ),
        &dir,
    );
    assert_eq!(rec["verdict"], "unsupported", "{rec}");
    let words = rec["x-unsupported"].as_str().unwrap_or_default();
    assert!(words.contains("os_chdir"), "{words}");
}

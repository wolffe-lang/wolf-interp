//! is79 (wolf-interp#268; wolf-lang s248, #794-#798, #802; ruling #93,
//! ruling #97) — what the editor asks of the runtime, the mirror at its
//! witnesses: `os_clock_ns` (`[os.clock.ns]`), `fs_sync` and `fs_sync_dir`
//! (`[os.fs.sync]`), `fs_realpath` (`[os.fs.realpath]`),
//! `os_term_restore_at_exit` (`[os.term.restore]`), `os_signal_wait_fds`
//! (`[os.signal.wait.fds]`) and the mode's bits 5..7 (`[os.term.mode]`).
//!
//! The programs under `tests/fixtures/editor_calls_is79/` are wolf-lang
//! `9da7bdcf`'s (v0.2.34) `crates/wolf_driver/tests/fixtures/editor_calls/`
//! and `editor_wait/` byte for byte (sha256 `52e8ffe7…` clock_ns,
//! `cdd693cb…` realpath, `686a2a42…` restore_rows, `29998eb6…` save,
//! `72f15e30…` sync_rows, `c2176395…` wait_fds_rows, `9339d1cd…` wait_fds,
//! `a74d4e4d…` wait_fds_race, `183da257…` mode_enter, `625626b7…`
//! mode_opost), so the answers here are the ones the compiler's gates
//! (`editor_calls_lanes.rs`, `editor_wait_lanes.rs`) hold a mirrored lupin
//! to. Each was measured first on wolf 0.2.34's published archive (checked,
//! native and release) in a throwaway container on kasumi (is79,
//! `runs/m1/`).
//!
//! What this machine serves through the standard library: the clock
//! (`Instant`, from `time_now_ms`'s own anchor), the two syncs
//! (`File::sync_all`, the directory opened by `read_dir` first so a file is
//! `invalid`), and the canonical path (`fs::canonicalize`) — inside the tree
//! it serves (`[os.fs.path.domain]`): a path that resolves outside it is
//! declined BY NAME, as `fs_exists("/tmp")` is. What it does not mirror is
//! asserted on its words: the restore needs `tcsetattr` and the wait
//! `poll(2)`, neither in std, and this machine admits no `unsafe` — so each
//! answers its SHAPE rows (and `io` for a handle that is closed, forged or
//! not a terminal, which needs no host call) and refuses the rest by name.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/editor_calls_is79")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// `lupin conform-run <entry> --json` in `dir`, standard input the null
/// device.
fn observe(entry: &Path, dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("lupin observes")
}

fn record(out: &Output) -> serde_json::Value {
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .filter(|r| r.get("verdict").is_some())
        .unwrap_or_else(|| panic!("a record: {out:?}"))
}

fn verdict(rec: &serde_json::Value) -> &str {
    rec["verdict"].as_str().unwrap_or("")
}

fn stdout(rec: &serde_json::Value) -> String {
    rec["stdout_inline"].as_str().unwrap_or("").replace('\r', "")
}

fn reason(rec: &serde_json::Value) -> &str {
    rec["x-unsupported"].as_str().unwrap_or("")
}

/// The fixture `name`, observed in a scratch directory of its own.
fn run_fixture(name: &str) -> serde_json::Value {
    let dir = scratch(&format!("editor_calls_is79_{}", name.replace('.', "_")));
    record(&observe(&fixture(name), &dir))
}

/// A program of this test's own, written to a scratch directory of its
/// own (a directory is a module) and observed there.
fn run_text(slug: &str, program: &str) -> serde_json::Value {
    let dir = scratch(&format!("editor_calls_is79_{slug}"));
    let entry = dir.join("main.lu");
    std::fs::write(&entry, program).expect("the program is written");
    record(&observe(&entry, &dir))
}

/// It ran to `exit(0)` printing `want`.
fn assert_runs(rec: &serde_json::Value, want: &str, what: &str) {
    assert_eq!(
        (verdict(rec), stdout(rec).as_str()),
        ("exit(0)", want),
        "{what}: {rec}"
    );
}

/// It was refused by name, the reason naming `call`, and never as a name
/// this machine does not know.
fn assert_refused_by_name(rec: &serde_json::Value, call: &str, what: &str) {
    assert_eq!(verdict(rec), "unsupported", "{what}: {rec}");
    let why = reason(rec);
    assert!(
        why.contains(&format!("`{call}`")) && !why.contains("does not resolve"),
        "{what}: refused by name, naming `{call}`: {why}"
    );
}

// ------------------------------------------------------------ #798 --

/// `[os.clock.ns]`: twenty rounds of about 100 us of work, each timed in
/// nanoseconds, none 0, and the clock is the ms clock's, from the same
/// anchor (`behind=0`). 0.1.55: "`os_clock_ns` does not resolve".
#[test]
fn the_clock_at_nanoseconds_is_never_zero_and_never_behind_the_ms_clock() {
    let rec = run_fixture("clock_ns.lu");
    assert_runs(
        &rec,
        "clock_ns zeros=0 of 20 behind=0 work=1199940\n",
        "clock_ns.lu",
    );
}

// ------------------------------------------------------------ #796 --

/// `[os.fs.sync]`'s rows: a written handle syncs, a closed, forged or
/// negative one and a pipe are `io`; the working directory syncs, a missing
/// name and the empty path are `not_found`, a file `invalid`; and the
/// exclusive create the save starts with (`fs_open_perm(p, 4, perm)`) is
/// `exists` for a taken name. windows: `fs_sync_dir` is `unsupported` by
/// name for every argument, and so is `fs_open_perm` (`[os.fs.create.mode]`).
/// 0.1.55: "`fs_sync` does not resolve".
#[test]
fn the_sync_rows_answer_as_the_compiled_tiers() {
    let rec = run_fixture("sync_rows.lu");
    if cfg!(unix) {
        assert_runs(
            &rec,
            "written=ok\nexcl_again=exists\nclosed=io\nforged=io\nnegative=io\npipe=io\n\
dir_dot=ok\ndir_missing=not_found\ndir_file=invalid\ndir_empty=not_found\nagain=-1\n",
            "sync_rows.lu",
        );
    } else {
        assert_eq!(verdict(&rec), "exit(0)", "sync_rows.lu (windows): {rec}");
        let out = stdout(&rec);
        assert!(
            out.starts_with("written=ok\nexcl_again=unsupported\nclosed=io\n")
                && out.contains("dir_dot=unsupported\ndir_missing=unsupported\n"),
            "sync_rows.lu (windows): {out}"
        );
    }
}

/// The atomic save whole (yew's invariant 1): the temporary file made
/// exclusively, written, `fs_sync`ed, renamed over `doc.txt` and the
/// directory `fs_sync_dir`ed. The boundaries under a fault shim are the
/// compiled tiers' (`editor_calls_lanes.rs`); here the whole save, as that
/// gate runs lupin.
#[test]
fn the_atomic_save_runs_whole() {
    let dir = scratch("editor_calls_is79_save");
    std::fs::write(dir.join("doc.txt"), "old\n").expect("the old file");
    let rec = record(&observe(&fixture("save.lu"), &dir));
    if cfg!(unix) {
        assert_runs(&rec, "saved\n", "save.lu");
        assert_eq!(
            std::fs::read_to_string(dir.join("doc.txt")).expect("doc.txt"),
            "new 0\nnew 1\nnew 2\nnew 3\n",
            "the save wrote the new bytes"
        );
        assert!(
            !dir.join("doc.txt.s248tmp").exists(),
            "the temporary file was renamed away"
        );
    } else {
        // windows: `fs_open_perm` is `unsupported` there, so the save's
        // first `?` ends it with that row (exit 1), as on every machine.
        assert_eq!(verdict(&rec), "exit(1)", "save.lu (windows): {rec}");
    }
}

// ------------------------------------------------------------ #797 --

/// `[os.fs.realpath]` inside the tree: `realpath.lu` less its two `/tmp`
/// lines (the one path it asks about outside its working directory, which
/// the next test holds) — a file, a directory with a trailing slash, a `..`
/// after a directory and after a link, a missing name, the empty path, a
/// dangling link (`not_found`), a name under a file and a loop (`io`).
/// windows: no symbolic links (`fs_symlink` is `unsupported`), and the
/// host's own separator.
#[test]
fn the_canonical_path_inside_the_tree_answers_as_the_compiled_tiers() {
    let text = std::fs::read_to_string(fixture("realpath.lu")).expect("realpath.lu");
    let without_tmp: String = text
        .lines()
        .filter(|l| !l.contains("fs_realpath(\"/tmp\")") && !l.contains("print(\"tmp={tmp}\")"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(
        text.lines().count() - 2,
        without_tmp.lines().count(),
        "exactly the two /tmp lines are left out"
    );
    let rec = run_text("realpath_inside", &without_tmp);
    if cfg!(unix) {
        assert_runs(
            &rec,
            "file=rp/a/f\ndir_slash=rp/a\ndot_dot=rp/a/f\nmissing=not_found\n\
under_file=io\nempty=not_found\ndot=.\nlinks=made\nlink=rp/a/b\n\
link_dot_dot=rp/a\ndangling=not_found\nloop=io\n",
            "realpath.lu inside the tree",
        );
    } else {
        assert_eq!(verdict(&rec), "exit(0)", "realpath (windows): {rec}");
        let out = stdout(&rec);
        assert!(
            out.starts_with("file=rp\\a\\f\n") && out.contains("links=unsupported\n"),
            "realpath (windows): {out}"
        );
    }
}

/// `realpath.lu` byte for byte: every row inside the tree answers, and
/// `fs_realpath("/tmp")` — a path that resolves outside the working
/// directory — is declined BY NAME (`[os.fs.path.domain]`), as every fs
/// path call of this machine declines one (`fs_exists("/tmp")` included).
#[test]
fn the_canonical_path_outside_the_tree_is_declined_by_name() {
    let rec = run_fixture("realpath.lu");
    assert_refused_by_name(&rec, "fs_realpath(\"/tmp\")", "realpath.lu");
    assert!(
        reason(&rec).contains("resolves outside the working directory"),
        "the containment decline: {rec}"
    );
}

// ------------------------------------------------------------ #794 --

/// `[os.term.restore]`'s registration rows that need no terminal: a mode
/// outside the bits is `invalid` before the handle is read (windows too),
/// then windows `unsupported`, then a forged handle and a file `io`.
/// 0.1.55: "`os_term_restore_at_exit` does not resolve".
#[test]
fn the_restore_rows_answer_as_the_compiled_tiers() {
    let rec = run_fixture("restore_rows.lu");
    let want = if cfg!(unix) {
        "bit24=invalid negative=invalid forged=io file=io\n"
    } else {
        "bit24=invalid negative=invalid forged=unsupported file=unsupported\n"
    };
    assert_runs(&rec, want, "restore_rows.lu");
}

/// A registration on a terminal needs `tcsetattr` at the exit: refused by
/// name (unix). Asked of a handle that IS a terminal only when the test has
/// one; the mode with bits 5..7 set passes the shape (`RAW_CR`,
/// `RAW_OUTPUT`, `STRIP8` are fields since s248), so a forged handle with
/// that mode is `io`, not `invalid`.
#[test]
fn the_restore_takes_the_new_bits_and_reads_the_handle_after_them() {
    let rec = run_text(
        "restore_bits",
        "fn row(fd: int, mode: int) -> str {
    os_term_restore_at_exit(fd, mode, List[byte]()) else |e| {
        return match e {
            invalid => \"invalid\",
            io => \"io\",
            unsupported => \"unsupported\",
        }
    }
    \"ok\"
}

fn main() {
    print(\"raw={row(1000000, 32 | 64 | 128 | 256)} vtime={row(1000000, 16711680)} bit24={row(1000000, 16777216)}\")
}
",
    );
    let want = if cfg!(unix) {
        "raw=io vtime=io bit24=invalid\n"
    } else {
        "raw=unsupported vtime=unsupported bit24=invalid\n"
    };
    assert_runs(&rec, want, "the restore's shape with bits 5..7");
}

// ------------------------------------------------------------ #795 --

/// `[os.signal.wait.fds]`'s rows that need no terminal: a set the shape
/// rule refuses is `io`, a deadline below -1 `invalid`, each before any
/// handle is looked at, on every host. 0.1.55: "`os_signal_wait_fds` does
/// not resolve".
#[test]
fn the_wait_fds_rows_answer_as_the_compiled_tiers() {
    let rec = run_fixture("wait_fds_rows.lu");
    assert_runs(
        &rec,
        "empty=io send_only=io past_table=io below=invalid far_below=invalid\n",
        "wait_fds_rows.lu",
    );
}

/// After the shape: windows `unsupported`; a closed or forged handle `io`
/// (decided without the host); and a well-formed wait, which needs
/// `poll(2)` and a listener this machine never arms, refused by name.
#[test]
fn a_well_formed_wait_over_descriptors_is_refused_by_name() {
    let forged = run_text(
        "wait_fds_forged",
        "fn main() {
    let r = os_signal_wait_fds([1000000], 16384, 0) else |e| {
        print(match e {
            invalid => \"invalid\",
            io => \"io\",
            unsupported => \"unsupported\",
        })
        return
    }
    print(\"{r.0} {r.1}\")
}
",
    );
    assert_runs(
        &forged,
        if cfg!(unix) { "io\n" } else { "unsupported\n" },
        "a forged handle",
    );
    let live = run_text(
        "wait_fds_live",
        "fn main() -> !int {
    let p = os_pipe()?
    let r = os_signal_wait_fds([p.0], 16384, 25) else |e| {
        print(match e {
            invalid => \"invalid\",
            io => \"io\",
            unsupported => \"unsupported\",
        })
        return 0
    }
    print(\"{r.0} {r.1}\")
    0
}
",
    );
    if cfg!(unix) {
        assert_refused_by_name(&live, "os_signal_wait_fds", "a well-formed wait");
    } else {
        assert_runs(&live, "unsupported\n", "a well-formed wait (windows)");
    }
}

// ------------------------------------------------------------ #802 --

/// `[os.term.mode]`'s bits 5..7 (ruling #97: each names the setting a
/// terminal does NOT usually have, so 0 is the usual one) are fields: a set
/// with them passes the shape, and only bit 24 and above, or a negative
/// mode, is `invalid`. Past the shape the set is refused by name on unix
/// as it always was (`tcsetattr`), and windows answers `unsupported`.
/// 0.1.55: `invalid` (its shape row before s248).
#[test]
fn the_mode_bits_five_to_seven_pass_the_shape() {
    let rec = run_text(
        "mode_bits",
        "fn main() {
    os_term_set_mode(0, 32 | 64 | 128 | 256) else |e| {
        print(match e {
            invalid => \"invalid\",
            io => \"io\",
            unsupported => \"unsupported\",
        })
        return
    }
    print(\"ok\")
}
",
    );
    if cfg!(unix) {
        assert_refused_by_name(&rec, "os_term_set_mode", "bits 5..7 set");
    } else {
        assert_runs(&rec, "unsupported\n", "bits 5..7 set (windows)");
    }
}

// ------------------------------------- the editor_wait fixtures (unix) --

/// `editor_wait_lanes.rs`'s fixtures, run as that gate runs lupin but with
/// no terminal (each makes its first call before it opens `/dev/tty`):
/// `wait_fds.lu`'s first wait is a shape row (an empty set, `io`) and its
/// listen is then refused by name, as `wait_fds_race.lu`'s is; the two mode
/// fixtures stop at `os_term_mode`, refused by name. None says "does not
/// resolve".
#[cfg(unix)]
#[test]
fn the_editor_wait_fixtures_are_refused_by_name_at_their_first_host_call() {
    for (name, call) in [
        ("wait_fds.lu", "os_signal_listen"),
        ("wait_fds_race.lu", "os_signal_listen"),
        ("mode_enter.lu", "os_term_mode"),
        ("mode_opost.lu", "os_term_mode"),
    ] {
        let rec = run_fixture(name);
        assert_refused_by_name(&rec, call, name);
    }
}

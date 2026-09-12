//! The s38/s90 fs family through the wolf surface, against REAL files
//! (is48, `[os.fs]` / `spec/11-os.md` §6).
//!
//! `src/eval/fs.rs`'s unit tests exercise the table directly; these are the
//! halves only the language surface can assert — that a row reaches a
//! program's `else` arms by TAG, that `?` carries it out of `main` as
//! `error: <tag>` and exit 1, that the infallible predicates really are
//! infallible, and that a path this machine will not look at is refused by
//! name rather than answered.
//!
//! Every test runs with the scratch directory as the process working
//! directory, because the tier is rooted at the interpreter's cwd and the
//! containment rule admits only a relative path that does not climb out of
//! it. That is also what the corpus witnesses assume.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

/// Run a program with `dir` as the working directory, so the relative paths
/// it writes land in the scratch and nowhere else.
fn run_program(dir: &Path, source: &str) -> Output {
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("written");
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("run")
        .arg("main.lu")
        .current_dir(dir)
        .output()
        .expect("lupin runs")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn a_write_read_remove_round_trip_over_rows() {
    // `corpus/fs/roundtrip.lu`'s shape, with the file under the test's own
    // directory instead of the repository's `target/`.
    let dir = scratch("fs-roundtrip");
    let source = "fn main() -> !int {\n\
        \x20   let path = \"out.txt\"\n\
        \x20   fs_write_text(path, \"three wolves\\n\")?\n\
        \x20   let text = fs_read_text(path)?\n\
        \x20   print(\"read: {text.trim()}\")\n\
        \x20   fs_remove(path)?\n\
        \x20   print(\"gone: {!fs_exists(path)}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "read: three wolves\ngone: true\n");
    // The file really was there and really is gone.
    assert!(!dir.join("out.txt").exists());
}

#[test]
fn the_file_a_program_writes_is_a_real_file_on_the_host() {
    // The claim the whole lane rests on: no mock. A separate process wrote
    // these bytes and this test reads them with `std::fs`.
    let dir = scratch("fs-really-real");
    let source = "fn main() -> !int {\n\
        \x20   fs_create_dir_all(\"sub\")?\n\
        \x20   fs_write_text(\"sub/hello.txt\", \"three wolves\")?\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        std::fs::read_to_string(dir.join("sub").join("hello.txt")).expect("a real file"),
        "three wolves"
    );
}

#[test]
fn a_missing_file_is_the_not_found_row_on_both_the_else_and_the_try_paths() {
    // `corpus/fs/error_row.lu`: io errors are ROWS, not traps (D30) —
    // handleable with `else`, or propagated with `?` to the documented
    // process outcome (the tag on stdout, exit 1).
    let dir = scratch("fs-not-found");
    let source = "fn main() -> !int {\n\
        \x20   let path = \"absent.txt\"\n\
        \x20   let text = fs_read_text(path) else |_| \"fallback\"\n\
        \x20   print(\"handled: {text}\")\n\
        \x20   let strict = fs_read_text(path)?\n\
        \x20   print(\"unreachable: {strict}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(stdout_of(&output), "handled: fallback\nerror: not_found\n");
}

#[test]
fn the_row_tags_discriminate_in_a_handler_s_arms() {
    // This is the test that proves `builtin::declared_row` is wired: a bare
    // lowercase arm resolves as a TAG only when the scrutinee's row names
    // it, so a wrong row table shows up here as an unresolved arm and not as
    // a wrong answer.
    let dir = scratch("fs-tag-arms");
    // Each arm answers a DISTINCT number, so the printed triple names which
    // arm fired: 1 not_found, 2 denied, 3 exists, 4 invalid, 5 io, 9 none of
    // them. An arm that did not resolve would be a parse error, which is the
    // other half of what this test is for.
    let source = "fn main() -> !int {\n\
        \x20   fs_write_text(\"there.txt\", \"x\")?\n\
        \x20   let missing = fs_open_mode(\"absent.txt\", 0) else |e| match e {\n\
        \x20       not_found => 1,\n\
        \x20       denied => 2,\n\
        \x20       exists => 3,\n\
        \x20       invalid => 4,\n\
        \x20       io => 5,\n\
        \x20   }\n\
        \x20   let bad = fs_open_mode(\"there.txt\", 99) else |e| match e {\n\
        \x20       invalid => 4,\n\
        \x20       _ => 9,\n\
        \x20   }\n\
        \x20   let twice = fs_open_mode(\"there.txt\", 4) else |e| match e {\n\
        \x20       exists => 3,\n\
        \x20       _ => 9,\n\
        \x20   }\n\
        \x20   print(\"{missing} {bad} {twice}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    // not_found, invalid, exists.
    assert_eq!(stdout_of(&output), "1 4 3\n");
}

#[test]
fn a_read_past_the_end_reaches_a_program_as_the_eof_row() {
    // The row no corpus witness exercises, asserted where a program would
    // meet it. `[os.fs.open]`'s mode-5 prose is the clause that names it.
    let dir = scratch("fs-eof-surface");
    let source = "fn main() -> !int {\n\
        \x20   fs_write_text(\"a.txt\", \"12345\")?\n\
        \x20   let fd = fs_open(\"a.txt\")?\n\
        \x20   let first = fs_read(fd, 16)?\n\
        \x20   let ended = fs_read(fd, 16) else |e| match e {\n\
        \x20       eof => \"eof\",\n\
        \x20       utf8 => \"utf8\",\n\
        \x20       io => \"io\",\n\
        \x20   }\n\
        \x20   fs_close(fd)?\n\
        \x20   print(\"first={first} then={ended}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "first=12345 then=eof\n");
}

#[test]
fn bytes_that_are_not_text_refuse_as_a_row_and_survive_a_byte_round_trip() {
    // `corpus/fs/bytes_dirs.lu`'s "a file no text reader can hold", with the
    // tag the witness takes as `_` spelled out.
    let dir = scratch("fs-bytes");
    let source = "fn main() -> !int {\n\
        \x20   var b = List[byte]()\n\
        \x20   (mut b).push(128 as byte)\n\
        \x20   (mut b).push(0 as byte)\n\
        \x20   (mut b).push(255 as byte)\n\
        \x20   (mut b).push(65 as byte)\n\
        \x20   fs_write_bytes(\"b.bin\", b)?\n\
        \x20   let tag = fs_read_text(\"b.bin\") else |e| match e {\n\
        \x20       utf8 => \"utf8\",\n\
        \x20       not_found => \"not_found\",\n\
        \x20       denied => \"denied\",\n\
        \x20       io => \"io\",\n\
        \x20   }\n\
        \x20   let back = fs_read_bytes(\"b.bin\")?\n\
        \x20   print(\"{tag} | {back.len} bytes | first={back[0]} last={back[3]}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "utf8 | 4 bytes | first=128 last=65\n");
}

#[test]
fn a_listing_is_sorted_and_a_program_can_depend_on_the_order() {
    // The sort is a PROMISE (`corpus/fs/bytes_dirs.lu` says so), which is
    // what makes a listing program's stdout the same on ext4, APFS and NTFS.
    let dir = scratch("fs-listing-surface");
    let source = "fn main() -> !int {\n\
        \x20   fs_create_dir_all(\"d/zsub\")?\n\
        \x20   fs_write_text(\"d/b.txt\", \"x\")?\n\
        \x20   fs_write_text(\"d/A.txt\", \"x\")?\n\
        \x20   fs_write_text(\"d/_u.txt\", \"x\")?\n\
        \x20   var line = \"\"\n\
        \x20   for n in fs_read_dir(\"d\")? {\n\
        \x20       line = \"{line}[{n}]\"\n\
        \x20   }\n\
        \x20   print(line)\n\
        \x20   fs_remove_dir_all(\"d\")?\n\
        \x20   print(\"cleaned={!fs_exists(\"d\")}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        stdout_of(&output),
        "[A.txt][_u.txt][b.txt][zsub]\ncleaned=true\n"
    );
}

#[test]
fn mode_five_is_mode_zero_on_a_regular_file_and_fstat_classifies_the_handle() {
    // `corpus/fs/open_nonblock.lu`'s claim, which is the one a program
    // depends on: the parity. The fifo half is `eval::fs`'s unit test,
    // because the language has no `mkfifo`.
    let dir = scratch("fs-mode-five");
    let source = "fn main() -> !int {\n\
        \x20   fs_write_text(\"a.txt\", \"12345\")?\n\
        \x20   let plain = fs_open(\"a.txt\")?\n\
        \x20   let a = fs_read(plain, 16)?\n\
        \x20   fs_close(plain)?\n\
        \x20   let nb = fs_open_mode(\"a.txt\", 5)?\n\
        \x20   let b = fs_read(nb, 16)?\n\
        \x20   let st = fs_fstat(nb)?\n\
        \x20   fs_close(nb)?\n\
        \x20   print(\"same_bytes={a == b} kind={st[0]} same_size={st[1] == fs_size(\"a.txt\")?}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        stdout_of(&output),
        "same_bytes=true kind=0 same_size=true\n"
    );
}

#[test]
fn the_predicates_never_raise_and_need_no_question_mark() {
    // `corpus/fs/roundtrip.lu` calls `fs_exists` with no `?`, so the three
    // predicates answer a bool and never a row — including on a path whose
    // parent does not exist.
    let dir = scratch("fs-predicates");
    let source = "fn main() -> !int {\n\
        \x20   fs_create_dir_all(\"d\")?\n\
        \x20   fs_write_text(\"f.txt\", \"x\")?\n\
        \x20   print(\"missing: {fs_exists(\"no/such\")} {fs_is_dir(\"no/such\")} {fs_is_file(\"no/such\")}\")\n\
        \x20   print(\"dir: {fs_exists(\"d\")} {fs_is_dir(\"d\")} {fs_is_file(\"d\")}\")\n\
        \x20   print(\"file: {fs_exists(\"f.txt\")} {fs_is_dir(\"f.txt\")} {fs_is_file(\"f.txt\")}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        stdout_of(&output),
        "missing: false false false\ndir: true true false\nfile: true false true\n"
    );
}

#[test]
fn a_path_outside_the_working_directory_is_refused_by_name() {
    // The containment rule, at the surface. `unsupported` exits 4 through
    // the front door and names the construct on stderr; it is deliberately
    // NOT a row, because a row would be a claim about the host rather than
    // about this implementation's surface.
    let dir = scratch("fs-containment");
    for path in ["/tmp/is48-escape.txt", "../is48-escape.txt"] {
        let source = format!(
            "fn main() -> !int {{\n\
             \x20   fs_write_text(\"{path}\", \"x\")?\n\
             \x20   0\n\
             }}\n"
        );
        let output = run_program(dir.as_path(), &source);
        assert_eq!(output.status.code(), Some(4), "{path}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("outside the working directory"),
            "{path}: {stderr}"
        );
    }
    // And nothing was written on the way to the refusal.
    assert!(!Path::new("/tmp/is48-escape.txt").exists());
}

#[test]
fn read_line_is_still_declined_and_says_so() {
    // The one name in the old fs block that did NOT land at is48: stdin is
    // not a file, no clause names an injectable one, and nothing in the
    // pinned corpus calls it. The name still resolves, so the refusal reads
    // "unsupported feature" and never "unknown name".
    let dir = scratch("fs-read-line");
    let source = "fn main() -> !int {\n\
        \x20   let s = read_line()\n\
        \x20   print(\"{s}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(4), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("injectable stdin"), "{stderr}");
}

#[test]
fn an_append_open_never_reads_what_was_already_there() {
    // `corpus/fs/bytes_dirs.lu`'s append arm: two opens, neither of them a
    // read of the existing contents.
    let dir = scratch("fs-append");
    let source = "fn main() -> !int {\n\
        \x20   fs_write_text(\"log.txt\", \"\")?\n\
        \x20   let a = fs_open_mode(\"log.txt\", 2)?\n\
        \x20   fs_write(a, \"one|\")?\n\
        \x20   fs_close(a)?\n\
        \x20   let b = fs_open_mode(\"log.txt\", 2)?\n\
        \x20   fs_write(b, \"two\")?\n\
        \x20   fs_close(b)?\n\
        \x20   print(\"log={fs_read_text(\"log.txt\")?} size={fs_size(\"log.txt\")?}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "log=one|two size=7\n");
}

#[test]
fn a_closed_or_forged_handle_is_the_io_row_at_the_surface() {
    // `corpus/fs/fstat.lu`'s `closed_is_io` and `forged_is_io`, through the
    // language rather than the table.
    let dir = scratch("fs-handle-rows");
    let source = "fn main() -> !int {\n\
        \x20   fs_write_text(\"a.txt\", \"12345\")?\n\
        \x20   let fd = fs_open(\"a.txt\")?\n\
        \x20   fs_close(fd)?\n\
        \x20   var closed = false\n\
        \x20   fs_fstat(fd) else |e| match e {\n\
        \x20       io => {\n\
        \x20           closed = true\n\
        \x20           List[int]()\n\
        \x20       },\n\
        \x20       _ => List[int](),\n\
        \x20   }\n\
        \x20   var forged = false\n\
        \x20   fs_fstat(999999) else |e| match e {\n\
        \x20       io => {\n\
        \x20           forged = true\n\
        \x20           List[int]()\n\
        \x20       },\n\
        \x20       _ => List[int](),\n\
        \x20   }\n\
        \x20   print(\"closed_is_io={closed} forged_is_io={forged}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "closed_is_io=true forged_is_io=true\n");
}

#[test]
fn a_rename_moves_bytes_no_text_reader_could_hold() {
    // `corpus/fs/bytes_dirs.lu`'s last claim: the move carries the
    // un-decodable bytes without reading them.
    let dir = scratch("fs-rename");
    let source = "fn main() -> !int {\n\
        \x20   var b = List[byte]()\n\
        \x20   (mut b).push(128 as byte)\n\
        \x20   (mut b).push(255 as byte)\n\
        \x20   fs_write_bytes(\"from.bin\", b)?\n\
        \x20   fs_rename(\"from.bin\", \"to.bin\")?\n\
        \x20   let back = fs_read_bytes(\"to.bin\")?\n\
        \x20   print(\"moved={fs_is_file(\"to.bin\")} gone={!fs_exists(\"from.bin\")} first={back[0]}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "moved=true gone=true first=128\n");
}

#[test]
fn a_live_run_writes_in_the_users_directory_and_an_observed_one_does_not() {
    // is48's working-directory rule, as the two halves a user can see.
    //
    // `lupin run` is the front door (is12): the program writes where the user
    // is standing, exactly as `wolf run` does. `lupin conform-run` is an
    // OBSERVATION, and an observation gets a private project root, because
    // this crate runs many programs at once and a shared directory makes them
    // interfere — measured, before the root existed: two concurrent exports
    // of one corpus appended to one `log.txt` twice.
    let dir = scratch("fs-live-vs-observed");
    let source = "fn main() -> !int {\n\
        \x20   fs_write_text(\"witness.txt\", \"here\")?\n\
        \x20   print(\"wrote={fs_exists(\"witness.txt\")}\")\n\
        \x20   0\n\
        }\n";
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("written");

    // Observed: the program sees its own file, and the user's directory does
    // not gain one.
    let observed = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg("main.lu")
        .current_dir(&dir)
        .output()
        .expect("lupin observes");
    assert_eq!(observed.status.code(), Some(0), "{observed:?}");
    assert!(
        String::from_utf8_lossy(&observed.stdout).contains("wrote=true"),
        "the observed program must really write its file: {}",
        String::from_utf8_lossy(&observed.stdout)
    );
    assert!(
        !dir.join("witness.txt").exists(),
        "an observation wrote into the user's directory"
    );

    // Live: the same program, the same directory, and now the file is there.
    let live = run_program(dir.as_path(), source);
    assert_eq!(live.status.code(), Some(0), "{live:?}");
    assert_eq!(stdout_of(&live), "wrote=true\n");
    assert_eq!(
        std::fs::read_to_string(dir.join("witness.txt")).expect("a real file"),
        "here"
    );
}

#[test]
fn two_concurrent_observations_of_one_program_do_not_interfere() {
    // The defect the private root exists to remove, as a regression test: the
    // same program, observed from several processes at once, against the same
    // working directory. Before is48's root this raced — one run removed the
    // file another was about to read (`error: not_found`), or two appends
    // landed in one file (`log=one|one|twotwo`).
    let dir = scratch("fs-concurrent-observations");
    // Write, append twice, read back: the shape that showed the interference.
    let source = "fn main() -> !int {\n\
        \x20   fs_write_text(\"log.txt\", \"\")?\n\
        \x20   let a = fs_open_mode(\"log.txt\", 2)?\n\
        \x20   fs_write(a, \"one|\")?\n\
        \x20   fs_close(a)?\n\
        \x20   let b = fs_open_mode(\"log.txt\", 2)?\n\
        \x20   fs_write(b, \"two\")?\n\
        \x20   fs_close(b)?\n\
        \x20   print(\"log={fs_read_text(\"log.txt\")?}\")\n\
        \x20   fs_remove(\"log.txt\")?\n\
        \x20   0\n\
        }\n";
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("written");

    let children: Vec<_> = (0..6)
        .map(|_| {
            Command::new(env!("CARGO_BIN_EXE_lupin"))
                .arg("conform-run")
                .arg("main.lu")
                .current_dir(&dir)
                // `spawn` inherits stdout unless told otherwise, and
                // `wait_with_output` would then hand back nothing.
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("lupin observes")
        })
        .collect();
    for child in children {
        let output = child.wait_with_output().expect("a child finishes");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("log=one|two"),
            "concurrent observations interfered: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[cfg(unix)]
#[test]
fn an_observed_programs_socket_lands_beside_its_files_and_not_in_the_users_directory() {
    // The coherence bug the private root introduced and this pins shut: a
    // unix socket is a filesystem object, so `net_listen_unix` has to resolve
    // it exactly where `fs_exists` looks. `corpus/net/unix_echo.lu` is the
    // witness that depends on it — it sweeps a stale socket with `fs_remove`,
    // binds it, and ends with `cleaned = !fs_exists(path)`. When the two
    // families disagreed, that sweep swept a path nothing bound, `cleaned`
    // was vacuously true, and a stale socket left in the real directory made
    // the bind fail at random.
    let dir = scratch("fs-socket-coherence");
    std::fs::create_dir_all(dir.join("target")).expect("a target/ to bind under");
    let source = "fn main() -> !int {\n\
        \x20   let path = \"target/probe.sock\"\n\
        \x20   let before = fs_exists(path)\n\
        \x20   let srv = net_listen_unix(path)?\n\
        \x20   let during = fs_exists(path)\n\
        \x20   net_close(srv)?\n\
        \x20   print(\"before={before} during={during} after={fs_exists(path)}\")\n\
        \x20   0\n\
        }\n";
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("written");

    // A stale socket in the USER's directory must not reach an observation.
    std::fs::write(dir.join("target").join("probe.sock"), b"stale").expect("stale planted");

    let observed = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg("main.lu")
        .current_dir(&dir)
        .output()
        .expect("lupin observes");
    assert_eq!(observed.status.code(), Some(0), "{observed:?}");
    let text = String::from_utf8_lossy(&observed.stdout);
    // `before=false` is the whole point: the observation never saw the stale
    // file, because it is not in the observation's directory. `during=true`
    // is the other half — the bind really made a file, and `fs_exists` really
    // found it, so the two families agree.
    assert!(
        text.contains("before=false during=true after=false"),
        "the fs and net families disagree about where a socket path is: {text}"
    );
    // And the user's own stale file is untouched.
    assert_eq!(
        std::fs::read_to_string(dir.join("target").join("probe.sock")).expect("still there"),
        "stale"
    );
}

#[test]
fn the_repl_front_door_writes_in_the_users_directory_and_keeps_the_file() {
    // `lupin eval` and the REPL are one door (`repl::Session`), and a person
    // is standing in a directory of their own when they use it. Before is48
    // gave that door its own flag, a session's `fs_write_text` landed in a
    // private observation root and was DELETED when the session ended —
    // data loss dressed as isolation. The flag is deliberately not
    // `live_stdout`: stdout pass-through and "this is somebody's real
    // directory" are different properties.
    let dir = scratch("fs-repl-door");
    let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("eval")
        .arg("fs_write_text(\"notes.txt\", \"kept\")?\nprint(\"here={fs_exists(\"notes.txt\")}\")")
        .current_dir(&dir)
        .output()
        .expect("lupin evaluates");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        stdout_of(&output).contains("here=true"),
        "{}",
        stdout_of(&output)
    );
    // The point: it is still there afterwards, in the user's own directory.
    assert_eq!(
        std::fs::read_to_string(dir.join("notes.txt")).expect("the file survives the session"),
        "kept"
    );
}

#[test]
fn a_windows_device_name_is_refused_by_name_at_the_surface() {
    // Refused on every host, because the corpus and the manual are shared
    // across the matrix: a program admitted here on unix would be a program
    // that cannot run on windows.
    let dir = scratch("fs-device-names");
    for name in ["NUL", "con.txt", "COM1"] {
        let source = format!(
            "fn main() -> !int {{\n\
             \x20   fs_write_text(\"{name}\", \"x\")?\n\
             \x20   0\n\
             }}\n"
        );
        let output = run_program(dir.as_path(), &source);
        assert_eq!(output.status.code(), Some(4), "{name}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("windows device"), "{name}: {stderr}");
    }
    // A name that merely starts the same way is untouched.
    let source = "fn main() -> !int {\n\
        \x20   fs_write_text(\"console.txt\", \"x\")?\n\
        \x20   print(\"ok={fs_exists(\"console.txt\")}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(dir.as_path(), source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "ok=true\n");
}

//! s199 (wolf-lang#426, #424) — the handle's offset and the three
//! descriptors a program did not open: `[os.fs.seek]`, `[os.fs.tell]`,
//! `[os.fs.read_at]`, `[os.fs.std]`, mirrored from wolf-lang's s199 (PR
//! wolf-lang#512).
//!
//! The first two tests are wolf-lang's corpus rows `fs/seek_tell.lu` and
//! `fs/read_at.lu` with their paths moved under the test's own directory;
//! the stdin tests are the fixtures under wolf-lang's
//! `crates/wolf_driver/tests/fixtures/fs_std/`, run with descriptor 0 a
//! regular file and a pipe — `lupin conform-run` and `lupin run` observe in
//! this process, so the test's stdin IS the program's.

use std::io::Write as _;
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

/// What the test hands the program on descriptor 0.
#[derive(Clone, Copy, Debug)]
enum Stdin {
    Null,
    File,
    Pipe,
}

/// 19 bytes.
const INPUT: &[u8] = b"wolves at the door\n";

/// `lupin run main.lu` in `dir`, descriptor 0 wired as `stdin` says.
fn run_program(dir: &Path, source: &str, stdin: Stdin) -> Output {
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("written");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lupin"));
    cmd.arg("run")
        .arg("main.lu")
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match stdin {
        Stdin::Null => {
            cmd.stdin(Stdio::null());
        }
        Stdin::File => {
            let p = dir.join("input.txt");
            std::fs::write(&p, INPUT).expect("input written");
            cmd.stdin(std::fs::File::open(&p).expect("input opened"));
        }
        Stdin::Pipe => {
            cmd.stdin(Stdio::piped());
        }
    }
    let mut child = cmd.spawn().expect("lupin runs");
    if let Some(mut w) = child.stdin.take() {
        // The program never reads descriptor 0 — it asks what it IS — so it
        // may exit before the write lands; EPIPE then is the race, not a
        // failure (wolf-lang CI run 37062798818 met it on linux).
        if let Err(e) = w.write_all(INPUT) {
            assert_eq!(
                e.kind(),
                std::io::ErrorKind::BrokenPipe,
                "pipe written: {e}"
            );
        }
    }
    child.wait_with_output().expect("lupin exits")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

const SEEK_TELL: &str = r#"fn seek_row(fd: int, off: int, whence: int) -> str {
    let at = fs_seek(fd, off, whence) else |e| {
        return match e {
            invalid => "invalid",
            unseekable => "unseekable",
            io => "io",
        }
    }
    "{at}"
}

fn main() -> !int {
    let path = "seek.tmp"
    fs_write_text(path, "0123456789")?
    let fd = fs_open(path)?
    print("first_handle_above_2={fd > 2}")
    let a = fs_read_chunk(fd, 3)?
    let b = fs_read_chunk(fd, 4)?
    let ab = str_from_utf8(a)? + str_from_utf8(b)?
    print("tell_after_reads={fs_tell(fd)?} read={ab}")
    let end = fs_seek(fd, 0, 2)?
    var at_end_is_eof = false
    fs_read_chunk(fd, 4) else |e| match e {
        eof => {
            at_end_is_eof = true
            List[byte]()
        },
        _ => List[byte](),
    }
    print("end={end} size={fs_size(path)?} at_end_is_eof={at_end_is_eof}")
    let back = fs_seek(fd, 0 - 3, 2)?
    let last = fs_read_chunk(fd, 16)?
    print("back={back} last={str_from_utf8(last)?}")
    let start = fs_seek(fd, 2, 0)?
    let cur = fs_seek(fd, 3, 1)?
    let next = fs_read_chunk(fd, 2)?
    print("start={start} cur={cur} next={str_from_utf8(next)?}")
    let past = fs_seek(fd, 4, 2)?
    var past_end_is_eof = false
    fs_read_chunk(fd, 4) else |e| match e {
        eof => {
            past_end_is_eof = true
            List[byte]()
        },
        _ => List[byte](),
    }
    print("past_end={past} past_end_is_eof={past_end_is_eof}")
    let bad_whence = seek_row(fd, 0, 3)
    let before_start = seek_row(fd, 0 - 1, 0)
    fs_close(fd)?
    let closed = seek_row(fd, 0, 0)
    let forged = seek_row(999999, 0, 0)
    print("bad_whence={bad_whence} before_start={before_start} closed={closed} forged={forged}")
    fs_remove(path)?
    print("cleaned={!fs_exists(path)}")
    0
}
"#;

const READ_AT: &str = r#"fn read_at_row(fd: int, off: int, max: int) -> str {
    let got = fs_read_at(fd, off, max) else |e| {
        return match e {
            eof => "eof",
            invalid => "invalid",
            unseekable => "unseekable",
            io => "io",
        }
    }
    str_from_utf8(got) else "utf8"
}

fn main() -> !int {
    let path = "read-at.tmp"
    fs_write_text(path, "0123456789")?
    let fd = fs_open(path)?
    let head = fs_read_chunk(fd, 2)?
    let at5 = fs_read_at(fd, 5, 3)?
    let tell = fs_tell(fd)?
    let next = fs_read_chunk(fd, 2)?
    print("head={str_from_utf8(head)?} at5={str_from_utf8(at5)?} tell={tell} next={str_from_utf8(next)?}")
    let at_end = read_at_row(fd, 10, 4)
    let empty = fs_read_at(fd, 3, 0)?
    let negative = read_at_row(fd, 0 - 1, 4)
    print("at_end={at_end} empty_len={empty.len} negative={negative}")
    let tell_unmoved = fs_tell(fd)?
    fs_close(fd)?
    let closed = read_at_row(fd, 0, 4)
    let forged = read_at_row(999999, 0, 4)
    print("tell_unmoved={tell_unmoved} closed={closed} forged={forged}")
    fs_remove(path)?
    print("cleaned={!fs_exists(path)}")
    0
}
"#;

const STD_DESCRIPTORS: &str = r#"fn fstat_line(fd: int) -> str {
    let st = fs_fstat(fd) else |e| {
        return match e {
            not_found => "not_found",
            denied => "denied",
            io => "io",
        }
    }
    if st[0] == 0 {
        "kind=0 size={st[1]}"
    } else {
        "kind={st[0]}"
    }
}

fn seek_line(fd: int) -> str {
    let at = fs_seek(fd, 0, 2) else |e| {
        return match e {
            invalid => "invalid",
            unseekable => "unseekable",
            io => "io",
        }
    }
    let back = fs_seek(fd, 0, 0) else |e| {
        return match e {
            invalid => "invalid",
            unseekable => "unseekable",
            io => "io",
        }
    }
    "end={at} back={back}"
}

fn tell_line(fd: int) -> str {
    let at = fs_tell(fd) else |e| {
        return match e {
            unseekable => "unseekable",
            io => "io",
        }
    }
    "{at}"
}

fn read_at_line(fd: int) -> str {
    let got = fs_read_at(fd, 0, 4) else |e| {
        return match e {
            eof => "eof",
            invalid => "invalid",
            unseekable => "unseekable",
            io => "io",
        }
    }
    str_from_utf8(got) else "utf8"
}

fn main() -> int {
    print("fstat0 {fstat_line(0)}")
    print("seek0 {seek_line(0)}")
    print("read_at0 {read_at_line(0)}")
    print("tell0 {tell_line(0)}")
    print("fstat2 {fstat_line(2)}")
    0
}
"#;

fn assert_runs(output: &Output, want: &str) {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(output), want);
}

#[test]
fn seek_and_tell_move_and_read_the_offset() {
    let dir = scratch("fs-offset-seek");
    assert_runs(
        &run_program(&dir, SEEK_TELL, Stdin::Null),
        "first_handle_above_2=true\ntell_after_reads=7 read=0123456\n\
end=10 size=10 at_end_is_eof=true\nback=7 last=789\nstart=2 cur=5 next=56\n\
past_end=14 past_end_is_eof=true\n\
bad_whence=invalid before_start=invalid closed=io forged=io\ncleaned=true\n",
    );
}

#[test]
fn read_at_leaves_the_cursor_alone() {
    let dir = scratch("fs-offset-read-at");
    assert_runs(
        &run_program(&dir, READ_AT, Stdin::Null),
        "head=01 at5=567 tell=2 next=23\nat_end=eof empty_len=0 negative=invalid\n\
tell_unmoved=4 closed=io forged=io\ncleaned=true\n",
    );
}

#[cfg(unix)]
#[test]
fn the_standard_streams_with_stdin_a_file() {
    let dir = scratch("fs-offset-std-file");
    assert_runs(
        &run_program(&dir, STD_DESCRIPTORS, Stdin::File),
        "fstat0 kind=0 size=19\nseek0 end=19 back=0\nread_at0 wolv\ntell0 0\nfstat2 kind=2\n",
    );
}

#[cfg(unix)]
#[test]
fn the_standard_streams_with_stdin_a_pipe() {
    let dir = scratch("fs-offset-std-pipe");
    assert_runs(
        &run_program(&dir, STD_DESCRIPTORS, Stdin::Pipe),
        "fstat0 kind=2\nseek0 unseekable\nread_at0 unseekable\ntell0 unseekable\nfstat2 kind=2\n",
    );
}

/// windows: the four calls on a standard stream are declined BY NAME
/// (`[os.fs.std]`) — `GetFileType` is the only honest classifier there and
/// this crate admits no `unsafe` to call it.
#[cfg(windows)]
#[test]
fn the_standard_streams_are_declined_by_name_on_windows() {
    let dir = scratch("fs-offset-std-windows");
    let output = run_program(&dir, STD_DESCRIPTORS, Stdin::File);
    assert_eq!(output.status.code(), Some(4), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported"), "{stderr}");
    assert!(stderr.contains("GetFileType"), "{stderr}");
}

//! s200 (wolf-lang#405, #411, #417, #407) — the byte surface, mirrored
//! from wolf-lang's s200: bytes on descriptors 0, 1 and 2 through the
//! descriptors themselves (`[os.fs.std]`), the bulk byte scan
//! (`[mem.list.bytes]`), the fused chunk copy (`[os.fs.copy]`) and the
//! host's number beside the row (`[os.fs.error]`).
//!
//! The first four programs are wolf-lang's corpus rows (`fs/std_write_bytes.lu`,
//! `fs/copy_chunk.lu`, `fs/os_error.lu`, `memory/bytes_scan.lu`) run from a
//! scratch project root with its own `target/`; the stdin tests are the
//! fixtures under wolf-lang's `crates/wolf_driver/tests/fixtures/byte_surface/`,
//! run with descriptor 0 a regular file, a pipe, and a file the test has
//! already read into — `lupin run` serves this process's descriptor 0, so
//! the offset is shared and the test sees where the program stopped.

use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(dir.join("target")).expect("scratch created");
    dir
}

/// What the test hands the program on descriptor 0.
#[derive(Clone, Copy, Debug)]
enum Stdin<'a> {
    Null,
    File(&'a [u8]),
    /// A file of which the test has read the first `n` bytes through the
    /// description it hands over.
    FileAt(&'a [u8], u64),
    Pipe(&'a [u8]),
}

/// 19 bytes.
const INPUT: &[u8] = b"wolves at the door\n";

/// `lupin run main.lu` in `dir`; the output, and where a `FileAt` input's
/// offset stood afterwards.
fn run_program(dir: &Path, source: &str, stdin: Stdin<'_>) -> (Output, Option<u64>) {
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("written");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lupin"));
    cmd.arg("run")
        .arg("main.lu")
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut held = None;
    let mut pipe = None;
    match stdin {
        Stdin::Null => {
            cmd.stdin(Stdio::null());
        }
        Stdin::File(bytes) | Stdin::FileAt(bytes, _) => {
            let p = dir.join("input.bin");
            std::fs::write(&p, bytes).expect("input written");
            let mut f = std::fs::File::open(&p).expect("input opened");
            if let Stdin::FileAt(_, n) = stdin {
                let mut skip = vec![0u8; usize::try_from(n).expect("small")];
                f.read_exact(&mut skip).expect("the test reads first");
                held = Some(f.try_clone().expect("dup the input"));
            }
            cmd.stdin(f);
        }
        Stdin::Pipe(bytes) => {
            cmd.stdin(Stdio::piped());
            pipe = Some(bytes);
        }
    }
    let mut child = cmd.spawn().expect("lupin runs");
    if let (Some(mut w), Some(bytes)) = (child.stdin.take(), pipe)
        && let Err(e) = w.write_all(bytes)
    {
        assert_eq!(
            e.kind(),
            std::io::ErrorKind::BrokenPipe,
            "pipe written: {e}"
        );
    }
    let out = child.wait_with_output().expect("lupin exits");
    let at = held.map(|mut f| f.stream_position().expect("the shared offset"));
    (out, at)
}

fn assert_runs(output: &Output, want: &[u8]) {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        output.stdout == want,
        "stdout {:?} against {:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(want)
    );
}

const STD_WRITE_BYTES: &str = r#"fn close_row(fd: int) -> str {
    fs_close(fd) else |e| {
        return match e {
            io => "io",
        }
    }
    "closed"
}

fn main() -> !int {
    print("one")
    fs_write_chunk(1, "two\n".bytes())?
    print("three")
    fs_write(1, "four\n")?
    let close1 = close_row(1)
    let close2 = close_row(2)
    print("close1={close1} close2={close2}")
    0
}
"#;

const COPY_CHUNK: &str = r#"fn copy_all(src: int, dst: int, max: int) -> int ! {io} {
    var total = 0
    loop {
        let n = fs_copy_chunk(src, dst, max) else |e| match e {
            eof => { break },
            io => { return io },
        }
        total = total + n
    }
    total
}

fn copy_row(src: int, dst: int) -> str {
    let n = fs_copy_chunk(src, dst, 4) else |e| {
        return match e {
            eof => "eof",
            io => "io",
        }
    }
    "moved {n}"
}

fn main() -> !int {
    let a = "target/s200-copy-a.tmp"
    let b = "target/s200-copy-b.tmp"
    fs_write_text(a, "the quick brown fox\n")?
    let src = fs_open(a)?
    let dst = fs_create(b)?
    let total = copy_all(src, dst, 7)?
    fs_close(dst)?
    let back = fs_read_text(b)?
    print("total={total} same={back == "the quick brown fox\n"}")

    let again = fs_open(a)?
    print("before")
    let shown = copy_all(again, 1, 1048576)?
    let zero = fs_copy_chunk(again, 1, 0)?
    print("after zero={zero}")

    let at_end = copy_row(again, 1)
    fs_close(src)?
    let closed = copy_row(src, 1)
    let forged_src = copy_row(999999, 1)
    let forged_dst = copy_row(again, 999999)
    print("at_end={at_end} closed={closed} forged_src={forged_src} forged_dst={forged_dst}")
    fs_close(again)?

    fs_remove(a)?
    fs_remove(b)?
    print("cleaned={!fs_exists(a) && !fs_exists(b) && shown == 20}")
    0
}
"#;

const OS_ERROR: &str = r#"fn open_code(path: str) -> int {
    let fd = fs_open(path) else |e| {
        return os_error()
    }
    fs_close(fd) else |e| {
        return 0 - 1
    }
    os_error()
}

fn main() -> !int {
    print("start={os_error()}")
    let missing = open_code("target/s200-no-such-file.tmp")
    let text = os_error_text(missing)
    let exists = fs_exists("target/s200-no-such-file.tmp")
    print("missing={missing} text_nonempty={text.len > 0} kept={os_error()}")

    let path = "target/s200-os-error.tmp"
    fs_write_text(path, "x")?
    let success = os_error()
    let forged = fs_read_chunk(999999, 4) else |e| {
        List[byte]()
    }
    let forged_code = os_error()
    let fd = fs_open(path)?
    let first = fs_read_chunk(fd, 8)?
    let eof_row = fs_read_chunk(fd, 8) else |e| {
        List[byte]()
    }
    let at_eof = os_error()
    fs_close(fd)?
    print("success={success} forged={forged_code} at_eof={at_eof}")
    print("zero_text={os_error_text(0) == ""} negative_text={os_error_text(0 - 7) == ""}")

    fs_remove(path)?
    print(
        "cleaned={!fs_exists(path) && !exists && first.len == 1 && forged.len + eof_row.len == 0}",
    )
    0
}
"#;

const BYTES_SCAN: &str = r#"fn find_row(xs: List[byte], b: byte, from: int) -> str {
    let i = bytes_find(xs, b, from) else |e| {
        return match e {
            none => "none",
        }
    }
    "{i}"
}

fn main() -> int {
    let nl = 10 as byte
    let text = "ab\ncd\n\nefg".bytes()
    print(
        "count={bytes_count(text, nl)} first={find_row(text, nl, 0)} next={find_row(text, nl, 3)} adjacent={find_row(text, nl, 6)} after={find_row(text, nl, 7)}",
    )
    print(
        "at_len={find_row(text, nl, text.len)} past={find_row(text, nl, 99)} negative={find_row(text, nl, 0 - 1)} absent={find_row(text, 122 as byte, 0)}",
    )
    let empty = List[byte]()
    print("empty_count={bytes_count(empty, nl)} empty_find={find_row(empty, nl, 0)}")

    var big = List[byte]()
    for i in 0..20000 {
        let v = if i % 7 == 6 { 10 } else { 97 }
        (mut big).push(v as byte)
    }
    var lines = 0
    var at = 0
    loop {
        let hit = bytes_find(big, nl, at) else |e| { break }
        lines = lines + 1
        at = hit + 1
    }
    print(
        "big_count={bytes_count(big, nl)} last={find_row(big, nl, 19998)} tail={find_row(big, nl, 19999)} lines={lines}",
    )
    0
}
"#;

const CAT_BYTES: &str = r#"fn main() -> int {
    loop {
        let chunk = fs_read_chunk(0, 65536) else |e| match e {
            eof => { break },
            io => { return 4 },
        }
        fs_write_chunk(1, chunk) else |e| {
            return 3
        }
    }
    0
}
"#;

const CAT_COPY: &str = r#"fn main() -> int {
    loop {
        let _ = fs_copy_chunk(0, 1, 1073741824) else |e| match e {
            eof => { break },
            io => {
                let code = os_error()
                eprint("copy: {code} {os_error_text(code)}")
                return 3
            },
        }
    }
    0
}
"#;

#[test]
fn byte_writes_to_stdout_keep_program_order() {
    let dir = scratch("byte-surface-std-write");
    let (out, _) = run_program(&dir, STD_WRITE_BYTES, Stdin::Null);
    assert_runs(&out, b"one\ntwo\nthree\nfour\nclose1=io close2=io\n");
}

#[test]
fn copy_chunk_moves_every_byte_once() {
    let dir = scratch("byte-surface-copy");
    let (out, _) = run_program(&dir, COPY_CHUNK, Stdin::Null);
    assert_runs(&out, b"total=20 same=true\nbefore\nthe quick brown fox\nafter zero=0\nat_end=eof closed=io forged_src=io forged_dst=io\ncleaned=true\n");
}

#[test]
fn os_error_is_the_hosts_number_for_the_last_fs_call() {
    let dir = scratch("byte-surface-os-error");
    let (out, _) = run_program(&dir, OS_ERROR, Stdin::Null);
    assert_runs(&out, b"start=0\nmissing=2 text_nonempty=true kept=2\nsuccess=0 forged=0 at_eof=0\nzero_text=true negative_text=true\ncleaned=true\n");
}

#[test]
fn the_byte_scan_answers_the_scalar_definition() {
    let dir = scratch("byte-surface-scan");
    let (out, _) = run_program(&dir, BYTES_SCAN, Stdin::Null);
    assert_runs(&out, b"count=3 first=2 next=5 adjacent=6 after=none\nat_len=none past=none negative=none absent=none\nempty_count=0 empty_find=none\nbig_count=2857 last=19998 tail=none lines=2857\n");
}

/// Every octet twice and a newline: what a byte-exact `cat` hands back.
fn binary_input() -> Vec<u8> {
    let mut v: Vec<u8> = (0..=255u8).chain(0..=255u8).collect();
    v.push(b'\n');
    v
}

#[test]
fn cat_reads_stdin_and_writes_stdout_as_bytes() {
    let bin = binary_input();
    for (name, program) in [("bytes", CAT_BYTES), ("copy", CAT_COPY)] {
        let dir = scratch(&format!("byte-surface-cat-{name}"));
        let (out, _) = run_program(&dir, program, Stdin::File(INPUT));
        assert_runs(&out, INPUT);
        let (out, _) = run_program(&dir, program, Stdin::Pipe(INPUT));
        assert_runs(&out, INPUT);
        let (out, _) = run_program(&dir, program, Stdin::File(&bin));
        assert_runs(&out, &bin);
        // The offset is shared: the program starts at byte 2, and the test
        // sees the end afterwards.
        let (out, at) = run_program(&dir, program, Stdin::FileAt(INPUT, 2));
        assert_runs(&out, &INPUT[2..]);
        assert_eq!(at, Some(INPUT.len() as u64), "{name}: the shared offset");
    }
}

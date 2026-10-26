//! r41e (wolf-interp#307; wolf-lang's `[os.proc.exec]`,
//! `[os.signal.disp]`) — the signal state a program lupin starts begins
//! with.
//!
//! lupin is a Rust program, and Rust's runtime sets SIGPIPE to ignored
//! before `main`. The wolf program lupin runs never asked for that, so the
//! image `os_exec` puts in lupin's place, and every child a spawn starts,
//! begins with SIGPIPE at its default — a pipeline stage whose reader has
//! gone stops, as GNU's tools do. lupin 0.1.57 handed the exec'd image an
//! IGNORED SIGPIPE (its direct `execve` keeps an ignored signal ignored, as
//! POSIX says); 0.1.56, through std's exec, and every spawn path did not.
//!
//! The mask is passed through unchanged: a lupin started with signals
//! blocked hands the image and every child the same blocked set, as 0.1.56
//! did and as a native image does (R-r41e-2, recommended A).
//!
//! **The witness is the new program's own report, never a signal sent.**
//! On Linux the program is `cat /proc/self/status` (or `cp` of it to a
//! file, for the spawn that wires its stdout to the null device): the
//! kernel's `SigBlk`, `SigIgn` and `SigCgt` lines for the new program. macOS
//! has no `/proc`, and its `ps` names the mask but no ignored-signal
//! column, so there the program is `tests/fixtures/exec_sigpipe_r41e/sigprobe.c`,
//! compiled here with the host's `cc` (the linker every Rust build on macOS
//! already calls): it reads its own dispositions with `sigaction` and its
//! mask with `sigprocmask`, changes neither, and prints the same three
//! lines. Bit `n - 1` of each word is signal `n` on both hosts.
//!
//! A lupin started "blocked" inherits SIGPIPE and SIGUSR1 in its mask: this
//! test's thread blocks them (a safe `pthread_sigmask`), starts lupin, and
//! puts its own mask back; std's spawn keeps the calling thread's mask.

#![cfg(unix)]

use nix::sys::signal::{SigSet, SigmaskHow, Signal, pthread_sigmask};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

/// The bit SIGPIPE has in a status word, on every unix host.
const PIPE: u64 = 1 << (Signal::SIGPIPE as i32 - 1);
/// The bit SIGUSR1 has here (10 on Linux, 30 on macOS).
const USR1: u64 = 1 << (Signal::SIGUSR1 as i32 - 1);

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("r41e-{name}"));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

/// A wolf string literal holding `text` (a path).
fn literal(text: &str) -> String {
    serde_json::to_string(text).expect("a JSON string")
}

/// The probe built once for this test binary, on a host with no `/proc`.
#[cfg(not(target_os = "linux"))]
fn sigprobe() -> &'static str {
    static PROBE: OnceLock<String> = OnceLock::new();
    PROBE.get_or_init(|| {
        let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("r41e-sigprobe");
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/exec_sigpipe_r41e/sigprobe.c");
        let built = Command::new("cc")
            .arg("-o")
            .arg(&out)
            .arg(&source)
            .output()
            .expect("the host's cc runs (it is the linker of this very build)");
        assert!(built.status.success(), "the probe compiles: {built:?}");
        out.to_str().expect("a UTF-8 path").to_owned()
    })
}

/// A wolf `List[str]` literal of `items`.
fn list(items: &[&str]) -> String {
    if items.is_empty() {
        return "List[str]()".to_owned();
    }
    let parts: Vec<String> = items.iter().map(|i| literal(i)).collect();
    format!("[{}]", parts.join(", "))
}

/// A program that prints its own signal state on stdout: the program and
/// its arguments (argv[0] not included).
fn reporter() -> (&'static str, Vec<&'static str>) {
    #[cfg(target_os = "linux")]
    {
        ("cat", vec!["/proc/self/status"])
    }
    #[cfg(not(target_os = "linux"))]
    {
        (sigprobe(), Vec::new())
    }
}

/// The whole argv of a program that writes its own signal state to `file`.
fn reporter_to(file: &str) -> Vec<&str> {
    #[cfg(target_os = "linux")]
    {
        vec!["cp", "/proc/self/status", file]
    }
    #[cfg(not(target_os = "linux"))]
    {
        vec![sigprobe(), file]
    }
}

/// Blocks SIGPIPE and SIGUSR1 on this thread until dropped.
struct Blocked(SigSet);

impl Blocked {
    fn pipe_and_usr1() -> Self {
        let mut set = SigSet::empty();
        set.add(Signal::SIGPIPE);
        set.add(Signal::SIGUSR1);
        let mut old = SigSet::empty();
        pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&set), Some(&mut old))
            .expect("this thread's mask is set");
        Self(old)
    }
}

impl Drop for Blocked {
    fn drop(&mut self) {
        pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&self.0), None)
            .expect("this thread's mask is put back");
    }
}

/// `lupin run main.lu` in `dir`, with `source` as the program; started
/// with SIGPIPE and SIGUSR1 blocked when `blocked`.
fn run(dir: &Path, source: &str, blocked: bool) -> Output {
    let program = dir.join("main.lu");
    std::fs::write(&program, source).expect("program written");
    let mut command = Command::new(env!("CARGO_BIN_EXE_lupin"));
    command
        .arg("run")
        .arg(&program)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = {
        let _mask = blocked.then(Blocked::pipe_and_usr1);
        command.spawn().expect("lupin starts")
    };
    child.wait_with_output().expect("lupin ends")
}

/// One report's `SigBlk`, `SigIgn` and `SigCgt` words.
#[derive(Debug, Clone, Copy)]
struct State {
    blk: u64,
    ign: u64,
    cgt: u64,
}

fn state(report: &str) -> State {
    let word = |name: &str| {
        let line = report
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .unwrap_or_else(|| panic!("no {name} line in the report: {report:?}"));
        u64::from_str_radix(line.trim(), 16).unwrap_or_else(|e| panic!("{name} {line:?}: {e}"))
    };
    State {
        blk: word("SigBlk:"),
        ign: word("SigIgn:"),
        cgt: word("SigCgt:"),
    }
}

/// The reports in `out`'s stdout, each under its `== name` header.
fn reports(out: &Output) -> Vec<(String, State)> {
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.split("== ")
        .filter(|part| !part.trim().is_empty())
        .map(|part| {
            let (name, body) = part.split_once('\n').unwrap_or((part, ""));
            (name.trim().to_owned(), state(body))
        })
        .collect()
}

/// The program that execs the reporter. It prints a header first, so the
/// image's lines follow it on the same stream.
fn exec_program() -> String {
    let (exe, args) = reporter();
    let argv: Vec<&str> = std::iter::once(exe).chain(args).collect();
    let (exe, argv) = (literal(exe), list(&argv));
    format!(
        r#"//! member: false
fn main() -> !int {{
    print("== image")
    os_exec({exe}, {argv}, ["PATH=/usr/bin:/bin"], List[int]())?
    1
}}
"#
    )
}

/// THE regression (#307): the image `os_exec` starts begins with SIGPIPE
/// at its default, and (lupin started plainly) not blocked. Red at
/// `fb054c8b` (0.1.57): SigIgn `0x1000` on Linux, the probe's SigIgn bit
/// 12 on macOS.
#[test]
fn exec_hands_the_image_sigpipe_at_default() {
    let dir = scratch("exec-plain");
    let out = run(&dir, &exec_program(), false);
    assert_eq!(out.status.code(), Some(0), "the image ran: {out:?}");
    let got = reports(&out);
    assert_eq!(got.len(), 1, "one report: {out:?}");
    let image = got[0].1;
    assert_eq!(
        image.ign & PIPE,
        0,
        "SIGPIPE ignored in the exec'd image ({image:x?}); lupin ignores it, the program never asked to"
    );
    assert_eq!(
        image.cgt & PIPE,
        0,
        "SIGPIPE caught in a fresh image ({image:x?})"
    );
    assert_eq!(
        image.blk & PIPE,
        0,
        "SIGPIPE blocked in the exec'd image ({image:x?})"
    );
}

/// A lupin started with SIGPIPE and SIGUSR1 blocked: the image still has
/// SIGPIPE at its default, and the inherited mask passes through whole
/// (R-r41e-2 A: 0.1.56's exec, every spawn path and a native image keep
/// it). Red at `fb054c8b` on the disposition.
#[test]
fn exec_passes_an_inherited_mask_through() {
    let dir = scratch("exec-blocked");
    let out = run(&dir, &exec_program(), true);
    assert_eq!(out.status.code(), Some(0), "the image ran: {out:?}");
    let got = reports(&out);
    assert_eq!(got.len(), 1, "one report: {out:?}");
    let image = got[0].1;
    assert_eq!(
        image.ign & PIPE,
        0,
        "SIGPIPE ignored in the exec'd image ({image:x?})"
    );
    assert_eq!(
        image.cgt & PIPE,
        0,
        "SIGPIPE caught in a fresh image ({image:x?})"
    );
    assert_eq!(
        image.blk & (PIPE | USR1),
        PIPE | USR1,
        "the mask lupin inherited reaches the image unchanged ({image:x?})"
    );
}

/// The program that starts the reporter through each spawn path and
/// prints each child's report under its name.
fn spawn_program() -> String {
    let (exe, args) = reporter();
    let (exe, args) = (literal(exe), list(&args));
    let to_file = list(&reporter_to("spawn-report.txt"));
    format!(
        r#"//! member: false
fn drain(fd: int) -> str {{
    var out = ""
    loop {{
        var stop = false
        let chunk = fs_read(fd, 4096) else |e| {{
            stop = true
            match e {{
                eof => "",
                io => "<io>",
                utf8 => "<utf8>",
            }}
        }}
        out = out + chunk
        if stop {{
            break
        }}
    }}
    out
}}

fn main() -> !int {{
    let (r1, w1) = os_pipe()?
    let a = os_spawn_fds({exe}, {args}, [1, w1])?
    fs_close(w1)?
    let one = drain(r1)
    fs_close(r1)?
    let sa = os_wait(a)?
    print("== os_spawn_fds exit={{sa}}\n{{one}}")
    let (r2, w2) = os_pipe()?
    let b = os_spawn_job({exe}, {args}, [1, w2], 0, -1, 0)?
    fs_close(w2)?
    let two = drain(r2)
    fs_close(r2)?
    let sb = os_wait(b)?
    print("== os_spawn_job (a new group) exit={{sb}}\n{{two}}")
    let c = os_spawn({to_file})?
    let sc = os_wait(c)?
    let three = fs_read_text("spawn-report.txt")?
    print("== os_spawn exit={{sc}}\n{{three}}")
    0
}}
"#
    )
}

fn spawn_paths(blocked: bool) -> Vec<(String, State)> {
    let dir = scratch(if blocked {
        "spawn-blocked"
    } else {
        "spawn-plain"
    });
    let out = run(&dir, &spawn_program(), blocked);
    assert_eq!(out.status.code(), Some(0), "the spawns ran: {out:?}");
    let got = reports(&out);
    let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "os_spawn_fds exit=0",
            "os_spawn_job (a new group) exit=0",
            "os_spawn exit=0"
        ],
        "{out:?}"
    );
    got
}

/// Every spawn path (std's `Command`, which sets SIGPIPE back to the
/// default in the child) hands its child SIGPIPE at its default and, with
/// a plain start, not blocked — before and after #307's repair alike.
#[test]
fn every_spawn_path_hands_the_child_sigpipe_at_default() {
    for (name, child) in spawn_paths(false) {
        assert_eq!(
            child.ign & PIPE,
            0,
            "{name}: SIGPIPE ignored in the child ({child:x?})"
        );
        assert_eq!(
            child.cgt & PIPE,
            0,
            "{name}: SIGPIPE caught in the child ({child:x?})"
        );
        assert_eq!(
            child.blk & PIPE,
            0,
            "{name}: SIGPIPE blocked in the child ({child:x?})"
        );
    }
}

/// With a blocked start every spawn path passes the mask through, as the
/// exec does (R-r41e-2 A), and the disposition is still the default.
#[test]
fn every_spawn_path_passes_an_inherited_mask_through() {
    for (name, child) in spawn_paths(true) {
        assert_eq!(
            child.ign & PIPE,
            0,
            "{name}: SIGPIPE ignored in the child ({child:x?})"
        );
        assert_eq!(
            child.blk & (PIPE | USR1),
            PIPE | USR1,
            "{name}: the inherited mask reaches the child ({child:x?})"
        );
    }
}

/// A failed exec puts the process back (`[os.proc.exec]`): lupin's own
/// SIGPIPE is never left at the default, which would end lupin at its next
/// write to a pipe whose reader has gone, and its mask is what it was.
/// Read from lupin's own `/proc/<pid>/status`, by a child, before and after
/// the failed exec. (Ignored at `fb054c8b`; caught after the repair, by a
/// handler that does nothing — the program still sees `io`.)
#[cfg(target_os = "linux")]
#[test]
fn a_failed_exec_never_leaves_lupin_sigpipe_at_default() {
    let dir = scratch("exec-failed");
    let source = r#"//! member: false
fn drain(fd: int) -> str {
    var out = ""
    loop {
        var stop = false
        let chunk = fs_read(fd, 4096) else |e| {
            stop = true
            match e {
                eof => "",
                io => "<io>",
                utf8 => "<utf8>",
            }
        }
        out = out + chunk
        if stop {
            break
        }
    }
    out
}

fn own_status() -> !str {
    let (r, w) = os_pipe()?
    let h = os_spawn_fds("cat", ["/proc/{os_pid()}/status"], [1, w])?
    fs_close(w)?
    let text = drain(r)
    fs_close(r)?
    let _ = os_wait(h)?
    text
}

fn main() -> !int {
    let before = own_status()?
    let row = os_exec("/nonexistent/r41e", ["r41e"], ["A=1"], List[int]()) else |e| match e {
        unsupported => "unsupported",
        invalid => "invalid",
        not_found => "not_found",
        denied => "denied",
        io => "io",
    }
    let after = own_status()?
    print("== before\n{before}== after {row}\n{after}")
    0
}
"#;
    for blocked in [false, true] {
        let out = run(&dir, source, blocked);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let got = reports(&out);
        assert_eq!(got.len(), 2, "{out:?}");
        let (before, (row, after)) = (got[0].1, (&got[1].0, got[1].1));
        assert_eq!(row, "after not_found", "the failed exec's row");
        assert_ne!(
            before.ign & PIPE,
            0,
            "lupin starts with SIGPIPE ignored ({before:x?})"
        );
        assert_ne!(
            (after.ign | after.cgt) & PIPE,
            0,
            "a failed exec left lupin's SIGPIPE at the default ({after:x?})"
        );
        assert_eq!(
            after.blk, before.blk,
            "a failed exec put the mask back (blocked start: {blocked})"
        );
    }
}

/// Where this host may see a SIGPIPE the kernel raises in lupin, a child
/// of this test (a CI runner, or `LUPIN_SIGNAL_TESTS=1` on a throwaway
/// host, the convention of `job_refused_is76.rs`).
fn signals_allowed() -> bool {
    std::env::var_os("CI").is_some() || std::env::var("LUPIN_SIGNAL_TESTS").is_ok_and(|v| v == "1")
}

/// What the status words above stand for: after a failed exec, a write to
/// a pipe whose reader has gone is still the `io` row (EPIPE) for the
/// program, and lupin lives — before the exec and after it alike.
#[test]
fn a_failed_exec_still_answers_a_readerless_pipe_with_io() {
    if !signals_allowed() {
        eprintln!(
            "SKIP: a_failed_exec_still_answers_a_readerless_pipe_with_io writes to a pipe with no \
             reader, which raises SIGPIPE in lupin; it runs where CI is set or \
             LUPIN_SIGNAL_TESTS=1 (a throwaway host)"
        );
        return;
    }
    let dir = scratch("exec-failed-epipe");
    let source = r#"//! member: false
fn readerless() -> !str {
    let (r, w) = os_pipe()?
    fs_close(r)?
    var row = "written"
    fs_write(w, "lost") else |e| match e {
        io => {
            row = "io"
        },
    }
    fs_close(w)?
    row
}

fn main() -> !int {
    let first = readerless()?
    print("before={first}")
    let row = os_exec("/nonexistent/r41e", ["r41e"], ["A=1"], List[int]()) else |e| match e {
        unsupported => "unsupported",
        invalid => "invalid",
        not_found => "not_found",
        denied => "denied",
        io => "io",
    }
    let second = readerless()?
    print("exec={row} after={second}")
    0
}
"#;
    let out = run(&dir, source, false);
    assert_eq!(out.status.code(), Some(0), "lupin lived: {out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "before=io\nexec=not_found after=io\n",
        "{out:?}"
    );
}

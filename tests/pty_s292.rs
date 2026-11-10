//! is87 (wolf-lang#1091; wolf-lang s292, `[os.term.pty]`,
//! `[os.term.size.set]`, `[os.term.attr]`, `[os.term.pty.note]`) — a
//! pseudo-terminal, its slave's name, the window's size set, and one setting
//! of the terminal at a time by RFC 4254's opcode, mirrored.
//!
//! `os_term_open_pty() -> (int, int) ! {io, unsupported}`,
//! `os_term_pty_name(fd) -> str ! {io, unsupported}`,
//! `os_term_set_size(fd, rows, columns, xpixels, ypixels) -> () ! {invalid,
//! io, unsupported}`, `os_term_attr(fd, op) -> int ! {invalid, io,
//! unsupported}` and `os_term_set_attr(fd, op, value) -> () ! {invalid, io,
//! unsupported}`. lupin 0.1.60 resolved none of the five. This machine
//! answers every row the clauses decide before a terminal is reached, in
//! their order: the SHAPE (`invalid`) on every host; then the HOST
//! (`unsupported`: a windows target for all five, an op the target lacks, a
//! set speed the runtime's table does not name); then the HANDLE (`io`: a
//! closed or forged one, and for the three that take a terminal one that is
//! not a terminal). Every call past those rows is refused BY NAME: Rust's
//! standard library has no `posix_openpt`, `tcgetattr`, `tcsetattr` or
//! window-size `ioctl`, and this crate admits no `unsafe`.
//!
//! `fixtures/pty_s292/pty_rows.lu` is wolf-lang's `corpus/os/pty_rows.lu` at
//! `5e89fdbe`, byte for byte (blob `0d74b900`, sha256 `3bcacf54…3c24`): it
//! runs to `exit(0)` with its declared stdout on every host.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use wolf_interp::attrs::with_build_target;
use wolf_interp::eval::SchedRequest;
use wolf_interp::protocol::Verdict;

const WINDOWS: &str = "x86_64-pc-windows-msvc";
const LINUX: &str = "x86_64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const UNIX_TARGETS: &[&str] = &[LINUX, DARWIN, wolf_interp::export::BUNDLE_TARGET];

/// A handle no program here ever opens.
const FORGED: &str = "1000000";

/// RFC 4254 section 8's opcodes, with RFC 8160's 42: the 56 of
/// `[os.term.attr]`.
fn vocabulary() -> Vec<i128> {
    let mut ops: Vec<i128> = Vec::new();
    ops.extend(1..=18);
    ops.extend(30..=42);
    ops.extend(50..=62);
    ops.extend(70..=75);
    ops.extend(90..=93);
    ops.extend([128, 129]);
    ops
}

/// The ops each target's `termios` lacks (the section's lists).
fn lacking(target: &str) -> &'static [i128] {
    if target.ends_with("-darwin") {
        &[15, 16, 37, 52, 71]
    } else {
        &[11, 15, 17]
    }
}

/// The runtime's speed table, bits per second.
const SPEEDS: &[i128] = &[
    0, 50, 75, 110, 134, 150, 200, 300, 600, 1200, 1800, 2400, 4800, 9600, 19200, 38400, 57600,
    115200, 230400,
];

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("is87_{name}"));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(dir.join("target")).expect("scratch created");
    dir
}

fn lupin(args: &[&str], entry: &Path, dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(args)
        .arg(entry)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("lupin runs")
}

/// The observation record `conform-run --json` writes last.
fn record(entry: &Path, dir: &Path) -> serde_json::Value {
    let out = lupin(&["conform-run", "--json"], entry, dir);
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .unwrap_or_else(|| panic!("no record: {out:?}"))
}

/// One program, observed by the binary on this host, stdin `/dev/null`.
fn observe(name: &str, source: &str) -> serde_json::Value {
    let dir = scratch(name);
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("program written");
    record(&entry, &dir)
}

/// One program, observed in this process as `target` (the bundle's lever).
fn observe_as(target: &'static str, source: &str) -> (Verdict, String, Option<String>) {
    with_build_target(target, || {
        let (r, out) = wolf_interp::observe_record_stdin(source.as_bytes(), &SchedRequest::Default);
        let why = r
            .extensions
            .get("x-unsupported")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        (r.verdict, out.stdout, why)
    })
}

/// Whether the call answers a value (bound with `let`) or `()`.
fn answers_a_value(call: &str) -> bool {
    !(call.starts_with("os_term_set_size(") || call.starts_with("os_term_set_attr("))
}

/// A program that prints the call's row, or `ok`.
fn handled(call: &str) -> String {
    let bind = if answers_a_value(call) {
        "let _v = "
    } else {
        ""
    };
    format!(
        "fn main() -> int {{\n    {bind}{call} else |e| {{\n        print(\"{{e}}\")\n        return 0\n    }}\n    print(\"ok\")\n    0\n}}\n"
    )
}

/// A program that lets the call's refusal (or row) end `main`.
fn bare(call: &str) -> String {
    let bind = if answers_a_value(call) {
        "let _v = "
    } else {
        ""
    };
    format!("fn main() -> !int {{\n    {bind}{call}?\n    0\n}}\n")
}

fn assert_by_name(name: &str, why: &str, context: &str) {
    assert!(
        why.contains(&format!("`{name}`")),
        "{context}: the reason must name `{name}`: {why}"
    );
    assert!(
        !why.contains("does not resolve"),
        "{context}: `{name}` must resolve: {why}"
    );
}

/// The SHAPE rows: a size field outside 0..65535, an op outside the
/// vocabulary, a set value outside its op's range.
fn shapes() -> Vec<String> {
    let mut calls = Vec::new();
    for (rows, columns, xp, yp) in [
        (-1, 80, 0, 0),
        (65536, 80, 0, 0),
        (24, -1, 0, 0),
        (24, 65536, 0, 0),
        (24, 80, -1, 0),
        (24, 80, 65536, 0),
        (24, 80, 0, -1),
        (24, 80, 0, 70000),
        (-65536, -1, 65536, 4_294_967_296_i64),
    ] {
        calls.push(format!(
            "os_term_set_size({FORGED}, {rows}, {columns}, {xp}, {yp})"
        ));
    }
    for op in [
        0,
        19,
        29,
        43,
        49,
        63,
        69,
        76,
        89,
        94,
        127,
        130,
        160,
        -1,
        4_294_967_296_i64,
    ] {
        calls.push(format!("os_term_attr({FORGED}, {op})"));
        calls.push(format!("os_term_set_attr({FORGED}, {op}, 0)"));
    }
    for (op, value) in [
        (1, 256),
        (1, -2),
        (18, 256),
        (11, -2),
        (30, 2),
        (30, -1),
        (42, 2),
        (53, 2),
        (62, -1),
        (75, 2),
        (90, 2),
        (91, -1),
        (92, 2),
        (93, -1),
        (128, -1),
        (129, -1),
        (129, 4_294_967_296_i64),
    ] {
        calls.push(format!("os_term_set_attr({FORGED}, {op}, {value})"));
    }
    calls
}

#[test]
fn the_shapes_are_invalid_on_this_host() {
    for (i, call) in shapes().iter().enumerate() {
        let r = observe(&format!("shape_{i}"), &handled(call));
        assert_eq!(r["verdict"], "exit(0)", "{call}: {r}");
        assert_eq!(r["stdout_inline"], "invalid\n", "{call}: {r}");
    }
}

#[test]
fn the_shapes_precede_the_host_on_every_target() {
    for call in shapes() {
        let source = handled(&call);
        for target in std::iter::once(&WINDOWS).chain(UNIX_TARGETS) {
            let (verdict, stdout, why) = observe_as(target, &source);
            assert_eq!(verdict, Verdict::Exit(0), "{call} as {target}: {why:?}");
            assert_eq!(stdout, "invalid\n", "{call} as {target}");
        }
    }
}

/// Well-formed calls on a forged handle: past every shape.
fn well_formed() -> Vec<String> {
    let mut calls = vec![
        "os_term_open_pty()".to_owned(),
        format!("os_term_pty_name({FORGED})"),
        format!("os_term_set_size({FORGED}, 24, 80, 0, 0)"),
        format!("os_term_set_size({FORGED}, 65535, 65535, 65535, 65535)"),
    ];
    for op in [1, 11, 15, 53, 90, 128, 129] {
        calls.push(format!("os_term_attr({FORGED}, {op})"));
    }
    for (op, value) in [
        (1, -1),
        (18, 255),
        (53, 1),
        (91, 0),
        (129, 9600),
        (129, 12345),
    ] {
        calls.push(format!("os_term_set_attr({FORGED}, {op}, {value})"));
    }
    calls
}

#[test]
fn windows_answers_the_unsupported_row_after_the_shape() {
    for call in well_formed() {
        let (verdict, stdout, why) = observe_as(WINDOWS, &handled(&call));
        assert_eq!(verdict, Verdict::Exit(0), "{call} as windows: {why:?}");
        assert_eq!(stdout, "unsupported\n", "{call} as windows");
    }
}

/// One program that prints `op=read/set` for every op of the vocabulary on
/// a forged handle, each set with a value inside its op's range.
fn every_op_program() -> String {
    let mut body = String::new();
    for op in vocabulary() {
        let value = if op >= 128 { 9600 } else { 1 };
        body.push_str(&format!(
            "    print(\"{op}={{attr({op})}}/{{set_attr({op}, {value})}}\")\n"
        ));
    }
    format!(
        "fn attr(op: int) -> str {{\n    let _v = os_term_attr({FORGED}, op) else |e| {{ return \"{{e}}\" }}\n    \"ok\"\n}}\n\
         fn set_attr(op: int, value: int) -> str {{\n    os_term_set_attr({FORGED}, op, value) else |e| {{ return \"{{e}}\" }}\n    \"ok\"\n}}\n\
         fn main() -> int {{\n{body}    0\n}}\n"
    )
}

fn every_op_expected(target: &str) -> String {
    let lacks = lacking(target);
    vocabulary()
        .into_iter()
        .map(|op| {
            let tag = if lacks.contains(&op) {
                "unsupported"
            } else {
                "io"
            };
            format!("{op}={tag}/{tag}\n")
        })
        .collect()
}

#[test]
fn each_target_lacks_its_own_ops_and_reads_the_handle_for_the_rest() {
    let source = every_op_program();
    for target in UNIX_TARGETS {
        let (verdict, stdout, why) = observe_as(target, &source);
        assert_eq!(verdict, Verdict::Exit(0), "as {target}: {why:?}");
        assert_eq!(stdout, every_op_expected(target), "as {target}");
    }
}

#[cfg(unix)]
#[test]
fn this_host_lacks_its_own_ops() {
    let r = observe("every_op", &every_op_program());
    let host = if cfg!(target_os = "macos") {
        DARWIN
    } else {
        LINUX
    };
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(r["stdout_inline"], every_op_expected(host), "{r}");
}

#[test]
fn a_set_speed_outside_the_table_is_unsupported_before_the_handle() {
    for target in UNIX_TARGETS {
        for op in [128, 129] {
            for speed in SPEEDS {
                let call = format!("os_term_set_attr({FORGED}, {op}, {speed})");
                let (verdict, stdout, why) = observe_as(target, &handled(&call));
                assert_eq!(verdict, Verdict::Exit(0), "{call} as {target}: {why:?}");
                assert_eq!(stdout, "io\n", "{call} as {target}");
            }
            for speed in [1, 49, 51, 12345, 7200, 460800, 921600, 4_294_967_295_i64] {
                let call = format!("os_term_set_attr({FORGED}, {op}, {speed})");
                let (verdict, stdout, why) = observe_as(target, &handled(&call));
                assert_eq!(verdict, Verdict::Exit(0), "{call} as {target}: {why:?}");
                assert_eq!(stdout, "unsupported\n", "{call} as {target}");
            }
            let call = format!("os_term_attr({FORGED}, {op})");
            let (verdict, stdout, why) = observe_as(target, &handled(&call));
            assert_eq!(verdict, Verdict::Exit(0), "{call} as {target}: {why:?}");
            assert_eq!(stdout, "io\n", "{call} as {target}");
        }
    }
}

const HANDLE_ROWS: &str = r#"fn name(h: int) -> str {
    let _n = os_term_pty_name(h) else |e| { return "{e}" }
    "ok"
}
fn size(h: int) -> str {
    os_term_set_size(h, 24, 80, 0, 0) else |e| { return "{e}" }
    "ok"
}
fn attr(h: int) -> str {
    let _v = os_term_attr(h, 53) else |e| { return "{e}" }
    "ok"
}
fn set_attr(h: int) -> str {
    os_term_set_attr(h, 53, 1) else |e| { return "{e}" }
    "ok"
}
fn main() -> !int {
    fs_write_text("target/f", "x")?
    let h = fs_open("target/f")?
    fs_close(h)?
    for k in [h, 1000000, -1, 7] {
        print("{name(k)} {size(k)} {attr(k)} {set_attr(k)}")
    }
    0
}
"#;

#[test]
fn a_closed_or_forged_handle_is_io() {
    let r = observe("closed_or_forged", HANDLE_ROWS);
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    let expected = if cfg!(windows) {
        "unsupported unsupported unsupported unsupported\n".repeat(4)
    } else {
        "io io io io\n".repeat(4)
    };
    assert_eq!(r["stdout_inline"], expected, "{r}");
}

#[test]
fn a_closed_or_forged_handle_is_io_on_every_unix_target() {
    let source = "fn name(h: int) -> str {\n    let _n = os_term_pty_name(h) else |e| { return \"{e}\" }\n    \"ok\"\n}\n\
                  fn size(h: int) -> str {\n    os_term_set_size(h, 24, 80, 0, 0) else |e| { return \"{e}\" }\n    \"ok\"\n}\n\
                  fn attr(h: int) -> str {\n    let _v = os_term_attr(h, 53) else |e| { return \"{e}\" }\n    \"ok\"\n}\n\
                  fn set_attr(h: int) -> str {\n    os_term_set_attr(h, 53, 1) else |e| { return \"{e}\" }\n    \"ok\"\n}\n\
                  fn main() -> int {\n    for k in [1000000, -1, 7] {\n        print(\"{name(k)} {size(k)} {attr(k)} {set_attr(k)}\")\n    }\n    0\n}\n";
    for target in UNIX_TARGETS {
        let (verdict, stdout, why) = observe_as(target, source);
        assert_eq!(verdict, Verdict::Exit(0), "as {target}: {why:?}");
        assert_eq!(stdout, "io io io io\n".repeat(3), "as {target}");
    }
}

#[cfg(unix)]
#[test]
fn a_handle_that_is_not_a_terminal_is_io_and_the_name_on_it_is_refused() {
    // A file of the table, and stdin wired to `/dev/null`: neither is a
    // terminal. The size and the settings answer `io`; the name, which the
    // section answers `io` only for a closed or forged handle, is refused by
    // name. A record carries no stdout for a refusal, so the lines are read
    // from `lupin run`.
    let dir = scratch("not_a_terminal");
    let entry = dir.join("main.lu");
    std::fs::write(
        &entry,
        r#"fn size(h: int) -> str {
    os_term_set_size(h, 24, 80, 0, 0) else |e| { return "{e}" }
    "ok"
}
fn attr(h: int) -> str {
    let _v = os_term_attr(h, 53) else |e| { return "{e}" }
    "ok"
}
fn set_attr(h: int) -> str {
    os_term_set_attr(h, 129, 9600) else |e| { return "{e}" }
    "ok"
}
fn main() -> !int {
    fs_write_text("target/f", "x")?
    let h = fs_open("target/f")?
    for k in [h, 0] {
        print("{size(k)} {attr(k)} {set_attr(k)}")
    }
    let _n = os_term_pty_name(h)?
    0
}
"#,
    )
    .expect("program written");
    let r = record(&entry, &dir);
    assert_eq!(r["verdict"], "unsupported", "{r}");
    assert_eq!(r["x-unsupported-at"], "run", "{r}");
    assert_by_name(
        "os_term_pty_name",
        r["x-unsupported"].as_str().unwrap_or_default(),
        "the name on a file",
    );
    let run = lupin(&["run"], &entry, &dir);
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "io io io\nio io io\n",
        "{stderr}"
    );
    assert_by_name("os_term_pty_name", &stderr, "the name on a file");
    assert_eq!(run.status.code(), Some(4), "{stderr}");
}

#[test]
fn the_name_on_a_standard_stream_is_refused_by_name_on_every_unix_target() {
    for handle in [0, 1, 2] {
        let source = bare(&format!("os_term_pty_name({handle})"));
        for target in UNIX_TARGETS {
            let (verdict, stdout, why) = observe_as(target, &source);
            assert_eq!(
                verdict,
                Verdict::Unsupported,
                "{handle} as {target}: {why:?}"
            );
            assert_eq!(stdout, "", "{handle} as {target}");
            assert_by_name(
                "os_term_pty_name",
                why.as_deref().unwrap_or_default(),
                target,
            );
        }
    }
}

#[test]
fn the_open_is_refused_by_name_on_every_unix_target() {
    let source = bare("os_term_open_pty()");
    for target in UNIX_TARGETS {
        let (verdict, stdout, why) = observe_as(target, &source);
        assert_eq!(verdict, Verdict::Unsupported, "as {target}: {why:?}");
        assert_eq!(stdout, "", "as {target}");
        assert_by_name(
            "os_term_open_pty",
            why.as_deref().unwrap_or_default(),
            target,
        );
    }
}

#[test]
fn the_open_on_this_host() {
    let r = observe("open", &handled("os_term_open_pty()"));
    if cfg!(windows) {
        assert_eq!(r["verdict"], "exit(0)", "{r}");
        assert_eq!(r["stdout_inline"], "unsupported\n", "{r}");
        return;
    }
    assert_eq!(r["verdict"], "unsupported", "{r}");
    assert_eq!(r["x-unsupported-at"], "run", "{r}");
    assert_by_name(
        "os_term_open_pty",
        r["x-unsupported"].as_str().unwrap_or_default(),
        "the open",
    );
}

/// Each call on a live terminal: the rows decided before the terminal print
/// first, then the call is refused by name.
#[cfg(unix)]
fn on_a_terminal(name: &str, call: &str) {
    let dir = scratch(&format!("terminal_{name}"));
    let entry = dir.join("main.lu");
    std::fs::write(
        &entry,
        format!(
            "fn main() -> !int {{\n    os_term_set_size(0, -1, 80, 0, 0) else |e| {{ print(\"shape={{e}}\") }}\n    os_term_set_attr(0, 15, 1) else |e| {{ print(\"lacking={{e}}\") }}\n    os_term_set_attr(0, 129, 12345) else |e| {{ print(\"speed={{e}}\") }}\n    {}{call}?\n    0\n}}\n",
            if answers_a_value(call) { "let _v = " } else { "" }
        ),
    )
    .expect("program written");
    // `lupin run` on a pty the test opens: stdin, stdout and stderr are the
    // terminal. A record carries no stdout for a refusal, so the run's own
    // output is read: the three rows, then the refusal on stderr.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lupin"));
    cmd.arg("run").arg(&entry).current_dir(&dir);
    let mut child = rexpect::session::spawn_command(cmd, Some(20_000)).expect("lupin on a pty");
    let out = child.exp_eof().expect("lupin ends").replace("\r\n", "\n");
    assert!(
        out.contains("shape=invalid\nlacking=unsupported\nspeed=unsupported\n"),
        "{call}: the rows before the terminal print first: {out}"
    );
    assert!(out.contains("unsupported: "), "{call}: {out}");
    assert_by_name(name, &out, call);
}

#[cfg(unix)]
#[test]
fn a_call_that_reaches_a_terminal_is_refused_by_name() {
    on_a_terminal("os_term_set_size", "os_term_set_size(0, 24, 80, 0, 0)");
    on_a_terminal("os_term_attr", "os_term_attr(0, 53)");
    on_a_terminal("os_term_attr", "os_term_attr(0, 129)");
    on_a_terminal("os_term_set_attr", "os_term_set_attr(0, 53, 1)");
    on_a_terminal("os_term_set_attr", "os_term_set_attr(0, 129, 9600)");
    on_a_terminal("os_term_pty_name", "os_term_pty_name(0)");
}

#[test]
fn the_declared_rows_discriminate_a_handler_s_arms() {
    // Each arm's bare name tests a tag of the call's declared row; with no
    // row it would bind, and the first arm would answer for every tag.
    let cases = [
        (
            "os_term_set_size(1000000, -1, 0, 0, 0)",
            "io => 1, unsupported => 2, invalid => 3",
            LINUX,
            3,
        ),
        (
            "os_term_set_size(1000000, 0, 0, 0, 0)",
            "invalid => 3, unsupported => 2, io => 1",
            LINUX,
            1,
        ),
        (
            "os_term_set_size(1000000, 0, 0, 0, 0)",
            "invalid => 3, io => 1, unsupported => 2",
            WINDOWS,
            2,
        ),
        (
            "os_term_attr(1000000, 15)",
            "io => 1, invalid => 3, unsupported => 2",
            LINUX,
            2,
        ),
        (
            "os_term_attr(1000000, 0)",
            "io => 1, unsupported => 2, invalid => 3",
            DARWIN,
            3,
        ),
        (
            "os_term_set_attr(1000000, 53, 2)",
            "unsupported => 2, io => 1, invalid => 3",
            LINUX,
            3,
        ),
        (
            "os_term_set_attr(1000000, 53, 1)",
            "unsupported => 2, invalid => 3, io => 1",
            DARWIN,
            1,
        ),
        (
            "os_term_pty_name(1000000)",
            "unsupported => 2, io => 1",
            LINUX,
            1,
        ),
        (
            "os_term_pty_name(1000000)",
            "io => 1, unsupported => 2",
            WINDOWS,
            2,
        ),
        (
            "os_term_open_pty()",
            "io => 1, unsupported => 2",
            WINDOWS,
            2,
        ),
    ];
    for (call, arms, target, code) in cases {
        let bind = if answers_a_value(call) {
            "let _v = "
        } else {
            ""
        };
        let source = format!(
            "fn main() -> int {{\n    {bind}{call} else |e| {{\n        return match e {{\n            {arms}\n        }}\n    }}\n    0\n}}\n"
        );
        let (verdict, _, why) = observe_as(target, &source);
        assert_eq!(
            verdict,
            Verdict::Exit(code),
            "{call} [{arms}] as {target}: {why:?}"
        );
    }
}

fn fixture() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pty_s292/pty_rows.lu");
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// The stdout the file's directive declares (its first line).
fn declared_stdout(entry: &Path) -> String {
    let text = std::fs::read_to_string(entry).expect("fixture read");
    let first = text.lines().next().expect("a directive line");
    let encoded = first
        .split("stdout=")
        .nth(1)
        .and_then(|s| s.strip_suffix(')'))
        .expect("a stdout= directive");
    let mut stdout: String = serde_json::from_str(encoded).expect("a JSON string");
    stdout.push('\n');
    stdout
}

#[test]
fn the_corpus_rows_run_to_exit_zero_with_the_declared_stdout() {
    let entry = fixture();
    let dir = scratch("corpus_rows");
    let want = declared_stdout(&entry);
    let r = record(&entry, &dir);
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(r["phase_reached"], "run", "{r}");
    assert_eq!(r["stdout_inline"], want, "{r}");
    let run = lupin(&["run"], &entry, &dir);
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    assert_eq!(run.status.code(), Some(0), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&run.stdout), want, "{stderr}");
}

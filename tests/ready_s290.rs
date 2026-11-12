//! li-wait (wolf-lang#1102; wolf-lang s290's second lane, `[os.ready.wait]`,
//! `[os.ready.set]`, `[os.ready.facts]`, `[os.ready.note]`,
//! `[os.net.write.some]`, `[os.net.peer.cred]`) — one wait over files,
//! sockets and signals, the readiness set the kernel holds, a send that says
//! what it took, and a Unix-domain peer's credentials, mirrored.
//!
//! lupin 0.1.61 resolved none of the seven names. This machine answers every
//! row the clauses decide without a host call it lacks, in their order, and
//! refuses the rest BY NAME: Rust's standard library has no `poll`, `epoll`
//! or `kqueue`, its `peer_cred` is unstable, and this crate admits no
//! `unsafe`. `net_write_some` is served: one write on a stream this machine
//! already holds non-blocking.
//!
//! - `os_wait_ready`: a set the shape rule refuses `io`, a deadline below -1
//!   `invalid`, a windows target `unsupported`, a wait nothing could end
//!   `io`, a negative, forged or closed element of either table `io`; every
//!   other wait refused by name.
//! - `os_ready_open`: windows `unsupported`, else refused by name. No set can
//!   be made here, so every set number is forged: `os_ready_set` (after
//!   `invalid` and windows) and `os_ready_wait` (after the set's shape, the
//!   deadline and windows) answer `io`, and `os_ready_close` `io` everywhere.
//! - `net_write_some`: a forged, closed or negative handle and a listener
//!   `io`; an empty list 0; one write's count, `busy` (the host's EAGAIN in
//!   `os_error()`) and `closed` (EPIPE or ECONNRESET).
//! - `net_peer_cred`: windows `unsupported`; a forged or closed handle, a
//!   TCP stream and a listener `io`; a Unix-domain stream refused by name.
//!
//! `fixtures/ready_s290/ready_rows.lu` is wolf-lang's `corpus/os/ready_rows.lu`
//! at `ead70b82`, byte for byte (blob `7624a22d`, sha256 `c68193f1…76b7`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use wolf_interp::attrs::with_build_target;
use wolf_interp::eval::SchedRequest;
use wolf_interp::protocol::Verdict;

const WINDOWS: &str = "x86_64-pc-windows-msvc";
const LINUX: &str = "x86_64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const UNIX_TARGETS: &[&str] = &[LINUX, DARWIN, wolf_interp::export::BUNDLE_TARGET];
const EVERY_TARGET: &[&str] = &[LINUX, DARWIN, wolf_interp::export::BUNDLE_TARGET, WINDOWS];

/// Bit 61: the element names a net handle (`[os.ready.wait]`).
const NET: i64 = 1 << 61;
/// Bit 62: the element asks for write readiness.
const WRITE: i64 = 1 << 62;
/// A meaning outside the twenty listenable ones (`[os.signal.set]`).
const KILL: i64 = 1 << 20;

/// The host's EAGAIN, EPIPE and ECONNRESET (`os_error()` after a refusal).
#[cfg(target_os = "linux")]
const EAGAIN: i64 = 11;
#[cfg(all(unix, not(target_os = "linux")))]
const EAGAIN: i64 = 35;
#[cfg(unix)]
const EPIPE: i64 = 32;
#[cfg(target_os = "linux")]
const ECONNRESET: i64 = 104;
#[cfg(all(unix, not(target_os = "linux")))]
const ECONNRESET: i64 = 54;

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("li_wait_{name}"));
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

/// One program run live by the binary on this host (`lupin run`), in its
/// own scratch directory, so a relative socket path stays short and inside
/// it: stdout, stderr and the exit code.
#[cfg(unix)]
fn run_live(name: &str, source: &str) -> (String, String, Option<i32>) {
    let dir = scratch(name);
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("program written");
    let out = lupin(&["run"], &entry, &dir);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    )
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
    !(call.starts_with("os_ready_set(") || call.starts_with("os_ready_close("))
}

/// A program that runs `setup`, then prints the call's row, or `ok`.
fn handled_after(setup: &str, call: &str) -> String {
    let bind = if answers_a_value(call) {
        "let _v = "
    } else {
        ""
    };
    format!(
        "fn main() -> !int {{\n    {setup}\n    {bind}{call} else |e| {{\n        print(\"{{e}}\")\n        return 0\n    }}\n    print(\"ok\")\n    0\n}}\n"
    )
}

fn handled(call: &str) -> String {
    handled_after("", call)
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

/// The row each call answers, as a unix target and as the windows target.
fn assert_rows(cases: &[(String, &str, &str)]) {
    for (call, unix, windows) in cases {
        for target in EVERY_TARGET {
            let want = if *target == WINDOWS { windows } else { unix };
            let (verdict, stdout, why) = observe_as(target, &handled(call));
            assert_eq!(
                verdict,
                Verdict::Exit(0),
                "{call} as {target}: {why:?} {stdout}"
            );
            assert_eq!(stdout, format!("{want}\n"), "{call} as {target}");
        }
    }
}

#[test]
fn the_wait_s_shape_and_host_rows_on_every_target() {
    let forged_net = NET + 1_000_000;
    let forged_write = WRITE + 1_000_000;
    let forged_both = NET + WRITE + 1_000_000;
    let high = 1_i64 << 60;
    let cases: Vec<(String, &str, &str)> = vec![
        // A set the shape rule refuses, before everything else.
        (format!("os_wait_ready([0], {KILL}, 0)"), "io", "io"),
        ("os_wait_ready([0], -1, 0)".into(), "io", "io"),
        (format!("os_wait_ready([0], {KILL}, -2)"), "io", "io"),
        ("os_wait_ready([1000000], -5, -9)".into(), "io", "io"),
        // A deadline below -1, before the host and the elements.
        ("os_wait_ready([0], 0, -2)".into(), "invalid", "invalid"),
        ("os_wait_ready([0], 1, -2)".into(), "invalid", "invalid"),
        (
            "os_wait_ready(List[int](), 0, -2)".into(),
            "invalid",
            "invalid",
        ),
        ("os_wait_ready([-1], 0, -3)".into(), "invalid", "invalid"),
        // Then the host; then a wait nothing could end; then the elements.
        (
            "os_wait_ready(List[int](), 0, -1)".into(),
            "io",
            "unsupported",
        ),
        ("os_wait_ready([1000000], 0, 0)".into(), "io", "unsupported"),
        ("os_wait_ready([-1], 0, 0)".into(), "io", "unsupported"),
        (
            format!("os_wait_ready([{forged_net}], 0, 0)"),
            "io",
            "unsupported",
        ),
        (format!("os_wait_ready([{NET}], 0, 0)"), "io", "unsupported"),
        (
            format!("os_wait_ready([{forged_write}], 0, 0)"),
            "io",
            "unsupported",
        ),
        (
            format!("os_wait_ready([{forged_both}], 0, 0)"),
            "io",
            "unsupported",
        ),
        (
            "os_wait_ready([0, 1000000], 0, 0)".into(),
            "io",
            "unsupported",
        ),
        (
            format!("os_wait_ready([{high}], 0, 0)"),
            "io",
            "unsupported",
        ),
        (
            "os_wait_ready([1000000], 1, -1)".into(),
            "io",
            "unsupported",
        ),
    ];
    assert_rows(&cases);
}

#[cfg(unix)]
#[test]
fn a_closed_handle_of_either_table_is_io_in_the_wait() {
    let cases = [
        (
            "let p = os_pipe()?\n    fs_close(p.0)?".to_owned(),
            "os_wait_ready([p.0], 0, 0)".to_owned(),
        ),
        (
            "let p = os_pipe()?\n    fs_close(p.1)?".to_owned(),
            format!("os_wait_ready([p.1 + {WRITE}], 0, 0)"),
        ),
        (
            "let l = net_listen(\"127.0.0.1:0\")?\n    net_close(l)?".to_owned(),
            format!("os_wait_ready([l + {NET}], 0, 0)"),
        ),
        (
            "let p = os_pipe()?\n    let l = net_listen(\"127.0.0.1:0\")?\n    net_close(l)?"
                .to_owned(),
            format!("os_wait_ready([p.0, p.1 + {WRITE}, l + {NET}], 0, 0)"),
        ),
    ];
    for (setup, call) in cases {
        for target in EVERY_TARGET {
            let want = if *target == WINDOWS {
                "unsupported"
            } else {
                "io"
            };
            let (verdict, stdout, why) = observe_as(target, &handled_after(&setup, &call));
            assert_eq!(verdict, Verdict::Exit(0), "{call} as {target}: {why:?}");
            assert_eq!(stdout, format!("{want}\n"), "{call} as {target}");
        }
    }
}

#[cfg(unix)]
#[test]
fn a_well_formed_wait_is_refused_by_name_on_every_unix_target() {
    let tcp = "let l = net_listen(\"127.0.0.1:0\")?\n    let port = net_port(l)?\n    \
               let c = net_connect(\"127.0.0.1:{port}\")?\n    let a = net_accept(l)?";
    let cases = [
        (
            "let p = os_pipe()?\n    fs_write(p.1, \"x\")?".to_owned(),
            format!("os_wait_ready([p.0, p.1 + {WRITE}], 0, 0)"),
        ),
        (String::new(), "os_wait_ready([0], 0, 0)".to_owned()),
        (String::new(), "os_wait_ready([0, 1, 2], 0, 25)".to_owned()),
        (String::new(), "os_wait_ready(List[int](), 0, 0)".to_owned()),
        (
            String::new(),
            "os_wait_ready(List[int](), 0, 25)".to_owned(),
        ),
        (String::new(), "os_wait_ready(List[int](), 1, 0)".to_owned()),
        (String::new(), "os_wait_ready([0], 1, -1)".to_owned()),
        (
            "let l = net_listen(\"127.0.0.1:0\")?".to_owned(),
            format!("os_wait_ready([l + {NET}], 0, -1)"),
        ),
        (
            tcp.to_owned(),
            format!("os_wait_ready([c + {NET} + {WRITE}, a + {NET}, c + {NET}], 0, 0)"),
        ),
    ];
    for (setup, call) in &cases {
        let source = handled_after(setup, call);
        for target in UNIX_TARGETS {
            let (verdict, stdout, why) = observe_as(target, &source);
            assert_eq!(verdict, Verdict::Unsupported, "{call} as {target}: {why:?}");
            assert_eq!(stdout, "", "{call} as {target}");
            assert_by_name("os_wait_ready", why.as_deref().unwrap_or_default(), call);
        }
        let (verdict, stdout, why) = observe_as(WINDOWS, &source);
        assert_eq!(verdict, Verdict::Exit(0), "{call} as windows: {why:?}");
        assert_eq!(stdout, "unsupported\n", "{call} as windows");
    }
}

/// The corpus file's `once`, beside a Unix-domain stream: the rows decided
/// first print, then the well-formed wait is refused by name.
#[cfg(unix)]
#[test]
fn the_wait_on_this_host_answers_its_rows_then_refuses_by_name() {
    let source = format!(
        r#"const NET: int = {NET}

const WRITE: int = {WRITE}

fn once(entries: List[int], set: int, deadline: int) -> str {{
    let r = os_wait_ready(entries, set, deadline) else |e| {{
        return match e {{
            invalid => "invalid",
            io => "io",
            unsupported => "unsupported",
        }}
    }}
    "{{r.0}}/{{r.1}}"
}}

fn main() -> !int {{
    let srv = net_listen_unix("w.sock")?
    let cli = net_connect_unix("w.sock")?
    let conn = net_accept(srv)?
    net_write(conn, "x")?
    print("bad_set={{once([cli + NET], {KILL}, 0)}}")
    print("deadline_below={{once([cli + NET], 0, -2)}}")
    net_close(srv)?
    print("closed_listener={{once([srv + NET], 0, 0)}}")
    print("wait={{once([cli + NET, conn + NET + WRITE], 0, 0)}}")
    0
}}
"#
    );
    let (stdout, stderr, code) = run_live("wait_host", &source);
    assert_eq!(
        stdout, "bad_set=io\ndeadline_below=invalid\nclosed_listener=io\n",
        "{stderr}"
    );
    assert_eq!(code, Some(4), "{stderr}");
    assert!(stderr.contains("unsupported: "), "{stderr}");
    assert_by_name("os_wait_ready", &stderr, "the wait on this host");
}

#[test]
fn the_set_rows_on_every_target() {
    let net_one = NET + 1;
    let cases: Vec<(String, &str, &str)> = vec![
        // `os_ready_set`: the interest and the key, then the host, then the
        // set, which no call here can have made.
        ("os_ready_set(0, 0, 4, 1)".into(), "invalid", "invalid"),
        ("os_ready_set(0, 0, -1, 1)".into(), "invalid", "invalid"),
        ("os_ready_set(0, 0, 1, -1)".into(), "invalid", "invalid"),
        ("os_ready_set(-1, -1, 9, -9)".into(), "invalid", "invalid"),
        ("os_ready_set(0, 0, 1, 0)".into(), "io", "unsupported"),
        ("os_ready_set(0, 0, 0, 0)".into(), "io", "unsupported"),
        (
            "os_ready_set(-1, 1000000, 3, 7)".into(),
            "io",
            "unsupported",
        ),
        (
            format!("os_ready_set(1000000, {net_one}, 2, 0)"),
            "io",
            "unsupported",
        ),
        // `os_ready_wait`: the set's shape, the deadline, the host, then the
        // forged set, at once even with no deadline.
        (format!("os_ready_wait(0, {KILL}, 0)"), "io", "io"),
        ("os_ready_wait(0, -1, -2)".into(), "io", "io"),
        ("os_ready_wait(0, 0, -2)".into(), "invalid", "invalid"),
        ("os_ready_wait(0, 1, -5)".into(), "invalid", "invalid"),
        ("os_ready_wait(0, 0, 0)".into(), "io", "unsupported"),
        ("os_ready_wait(0, 0, -1)".into(), "io", "unsupported"),
        ("os_ready_wait(-1, 1, 100)".into(), "io", "unsupported"),
        ("os_ready_wait(1000000, 0, 0)".into(), "io", "unsupported"),
        // `os_ready_close`: every number is forged, on every host.
        ("os_ready_close(0)".into(), "io", "io"),
        ("os_ready_close(-1)".into(), "io", "io"),
        ("os_ready_close(1000000)".into(), "io", "io"),
    ];
    assert_rows(&cases);
}

#[test]
fn the_open_is_refused_by_name_on_every_unix_target() {
    let source = handled("os_ready_open()");
    for target in UNIX_TARGETS {
        let (verdict, stdout, why) = observe_as(target, &source);
        assert_eq!(verdict, Verdict::Unsupported, "as {target}: {why:?}");
        assert_eq!(stdout, "", "as {target}");
        assert_by_name("os_ready_open", why.as_deref().unwrap_or_default(), target);
    }
    let (verdict, stdout, why) = observe_as(WINDOWS, &source);
    assert_eq!(verdict, Verdict::Exit(0), "as windows: {why:?}");
    assert_eq!(stdout, "unsupported\n", "as windows");
    let r = observe("open", &source);
    if cfg!(windows) {
        assert_eq!(r["verdict"], "exit(0)", "{r}");
        assert_eq!(r["stdout_inline"], "unsupported\n", "{r}");
        return;
    }
    assert_eq!(r["verdict"], "unsupported", "{r}");
    assert_eq!(r["x-unsupported-at"], "run", "{r}");
    assert_by_name(
        "os_ready_open",
        r["x-unsupported"].as_str().unwrap_or_default(),
        "the open on this host",
    );
}

/// The corpus file's `some`, for the send rows on this host.
#[cfg(unix)]
const SOME: &str = r#"fn some(fd: int, bytes: List[byte]) -> str {
    let n = net_write_some(fd, bytes) else |e| {
        return match e {
            busy => "busy",
            closed => "closed",
            invalid => "invalid",
            io => "io",
        }
    }
    "{n}"
}
"#;

#[cfg(unix)]
#[test]
fn the_send_answers_io_for_a_handle_it_cannot_send_on() {
    let source = format!(
        r#"{SOME}
fn main() -> !int {{
    let forged = some(1000000, [120 as byte])
    print("forged={{forged}} {{os_error()}}")
    let negative = some(-1, [120 as byte])
    print("negative={{negative}} {{os_error()}}")
    print("zero={{some(0, [120 as byte])}}")
    print("forged_empty={{some(1000000, List[byte]())}}")
    let l = net_listen("127.0.0.1:0")?
    let port = net_port(l)?
    let listener = some(l, [120 as byte])
    print("listener={{listener}} {{os_error()}}")
    print("listener_empty={{some(l, List[byte]())}}")
    let u = net_listen_unix("io.sock")?
    print("unix_listener={{some(u, [120 as byte])}}")
    print("unix_listener_empty={{some(u, List[byte]())}}")
    let c = net_connect("127.0.0.1:{{port}}")?
    let a = net_accept(l)?
    net_close(c)?
    let closed = some(c, [120 as byte])
    print("closed={{closed}} {{os_error()}}")
    print("closed_empty={{some(c, List[byte]())}}")
    net_close(a)?
    0
}}
"#
    );
    let (stdout, stderr, code) = run_live("send_io", &source);
    assert_eq!(
        stdout,
        "forged=io 0\nnegative=io 0\nzero=io\nforged_empty=io\nlistener=io 0\n\
         listener_empty=io\nunix_listener=io\nunix_listener_empty=io\nclosed=io 0\n\
         closed_empty=io\n",
        "{stderr}"
    );
    assert_eq!(code, Some(0), "{stderr}");
}

/// One send of five bytes on each family: the count is the list's length,
/// the peer reads exactly those bytes, and an empty list sends nothing (the
/// peer's read with a 50 ms budget answers `timeout`).
#[cfg(unix)]
#[test]
fn one_send_answers_its_count_and_the_peer_reads_it_whole() {
    for (family, setup) in [
        (
            "tcp",
            "let l = net_listen(\"127.0.0.1:0\")?\n    let port = net_port(l)?\n    \
             let c = net_connect(\"127.0.0.1:{port}\")?\n    let a = net_accept(l)?",
        ),
        (
            "unix",
            "let l = net_listen_unix(\"one.sock\")?\n    let c = net_connect_unix(\"one.sock\")?\n    \
             let a = net_accept(l)?",
        ),
    ] {
        let source = format!(
            r#"{SOME}
fn main() -> !int {{
    {setup}
    let n = net_write_some(c, "hello".bytes())?
    print("count={{n}} next={{n + 1}} {{os_error()}}")
    let got = net_read_bytes(a, 16)?
    print("read={{got.len}} {{got[0] == 104 as byte}} {{got[4] == 111 as byte}}")
    print("empty={{some(c, List[byte]())}} {{os_error()}}")
    net_deadline(a, 50)?
    var nothing = "data"
    let _none = net_read_bytes(a, 16) else |e| {{
        nothing = match e {{
            closed => "closed",
            timeout => "timeout",
            io => "io",
        }}
        List[byte]()
    }}
    print("after_empty={{nothing}}")
    0
}}
"#
        );
        let (stdout, stderr, code) = run_live(&format!("one_send_{family}"), &source);
        assert_eq!(
            stdout, "count=5 next=6 0\nread=5 true true\nempty=0 0\nafter_empty=timeout\n",
            "{family}: {stderr}"
        );
        assert_eq!(code, Some(0), "{family}: {stderr}");
    }
}

/// Write 64 KiB lists until the stream answers `busy`: every answer before
/// it is a count of 1..65536, `os_error()` reads the host's EAGAIN, the
/// peer drains exactly what was counted, and a send is taken again.
#[cfg(unix)]
fn fill_until_busy(family: &str, setup: &str, again_is_level: bool) {
    let source = format!(
        r#"{SOME}
fn main() -> !int {{
    let chunk = "x".repeat(65536).bytes()
    {setup}
    var taken = 0
    var writes = 0
    var each = true
    var last = ""
    while writes < 100000 {{
        let got = some(c, chunk)
        let n = got.to_int() else -1
        if n < 0 {{
            last = got
            break
        }}
        if n < 1 || n > 65536 {{
            each = false
        }}
        taken += n
        writes += 1
    }}
    let errno = os_error()
    print("full={{last}}")
    print("errno={{errno}}")
    print("each={{each}} taken={{taken > 0}} writes={{writes >= 1}}")
    let again = some(c, [120 as byte])
    print("again={{again}} {{os_error()}}")
    net_deadline(a, 10000)?
    var drained = 0
    let extra = if again == "1" then 1 else 0
    while drained < taken + extra {{
        let got = net_read_bytes(a, 65536)?
        drained += got.len
    }}
    print("drained={{drained == taken + extra}}")
    print("room={{some(c, [121 as byte])}} {{os_error()}}")
    let one = net_read_bytes(a, 16)?
    print("one={{one.len}} {{one[0] == 121 as byte}}")
    0
}}
"#
    );
    let (stdout, stderr, code) = run_live(&format!("busy_{family}"), &source);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(code, Some(0), "{family}: {stdout}{stderr}");
    assert_eq!(lines.len(), 8, "{family}: {stdout}{stderr}");
    assert_eq!(lines[0], "full=busy", "{family}: {stdout}");
    assert_eq!(lines[1], format!("errno={EAGAIN}"), "{family}: {stdout}");
    assert_eq!(
        lines[2], "each=true taken=true writes=true",
        "{family}: {stdout}"
    );
    if again_is_level {
        assert_eq!(
            lines[3],
            format!("again=busy {EAGAIN}"),
            "{family}: {stdout}"
        );
    } else {
        assert!(
            lines[3] == format!("again=busy {EAGAIN}") || lines[3] == "again=1 0",
            "{family}: {stdout}"
        );
    }
    assert_eq!(lines[4], "drained=true", "{family}: {stdout}");
    assert_eq!(lines[5], "room=1 0", "{family}: {stdout}");
    assert_eq!(lines[6], "one=1 true", "{family}: {stdout}");
}

#[cfg(unix)]
#[test]
fn a_full_unix_stream_answers_busy_and_takes_again_after_the_drain() {
    // Nothing moves a Unix-domain stream's bytes but the peer's read, so a
    // second ask with nothing read between is `busy` again.
    fill_until_busy(
        "unix",
        "let l = net_listen_unix(\"busy.sock\")?\n    let c = net_connect_unix(\"busy.sock\")?\n    \
         let a = net_accept(l)?",
        true,
    );
}

#[cfg(unix)]
#[test]
fn a_full_tcp_stream_answers_busy_and_takes_again_after_the_drain() {
    // A loopback TCP stream may move queued bytes between two asks, so the
    // second ask may be taken; whatever it takes is drained with the rest.
    fill_until_busy(
        "tcp",
        "let l = net_listen(\"127.0.0.1:0\")?\n    let port = net_port(l)?\n    \
         let c = net_connect(\"127.0.0.1:{port}\")?\n    let a = net_accept(l)?",
        false,
    );
}

/// The peer closes: a send answers `closed` (a TCP stream may take one or
/// more first, as the reset travels back), `os_error()` reads EPIPE or
/// ECONNRESET, and an empty list still answers 0.
#[cfg(unix)]
#[test]
fn a_send_after_the_peer_left_answers_closed() {
    for (family, setup) in [
        (
            "unix",
            "let l = net_listen_unix(\"gone.sock\")?\n    let c = net_connect_unix(\"gone.sock\")?\n    \
             let a = net_accept(l)?",
        ),
        (
            "tcp",
            "let l = net_listen(\"127.0.0.1:0\")?\n    let port = net_port(l)?\n    \
             let c = net_connect(\"127.0.0.1:{port}\")?\n    let a = net_accept(l)?",
        ),
    ] {
        let source = format!(
            r#"{SOME}
fn main() -> !int {{
    {setup}
    net_close(a)?
    var gone = some(c, [121 as byte])
    var spins = 0
    while gone == "1" && spins < 200 {{
        time_sleep_ms(5)
        gone = some(c, [121 as byte])
        spins += 1
    }}
    let errno = os_error()
    print("gone={{gone}}")
    print("errno={{errno}}")
    print("empty={{some(c, List[byte]())}}")
    0
}}
"#
        );
        let (stdout, stderr, code) = run_live(&format!("gone_{family}"), &source);
        assert_eq!(code, Some(0), "{family}: {stdout}{stderr}");
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(lines.len(), 3, "{family}: {stdout}{stderr}");
        assert_eq!(lines[0], "gone=closed", "{family}: {stdout}");
        assert!(
            lines[1] == format!("errno={EPIPE}") || lines[1] == format!("errno={ECONNRESET}"),
            "{family}: {stdout}"
        );
        assert_eq!(lines[2], "empty=0", "{family}: {stdout}");
    }
}

/// The corpus file's credential handler, as a function.
#[cfg(unix)]
const CRED: &str = r#"fn cred(fd: int) -> str {
    let c = net_peer_cred(fd) else |e| {
        return match e {
            io => "io",
            unsupported => "unsupported",
        }
    }
    "{c.0} {c.1} {c.2}"
}
"#;

#[cfg(unix)]
#[test]
fn the_credentials_answer_io_for_every_handle_but_a_unix_stream() {
    let source = format!(
        r#"{CRED}
fn main() -> !int {{
    print("forged={{cred(1000000)}}")
    print("negative={{cred(-1)}}")
    let l = net_listen("127.0.0.1:0")?
    let port = net_port(l)?
    print("tcp_listener={{cred(l)}}")
    let c = net_connect("127.0.0.1:{{port}}")?
    let a = net_accept(l)?
    print("tcp_dialed={{cred(c)}}")
    print("tcp_accepted={{cred(a)}}")
    let u = net_listen_unix("cred.sock")?
    print("unix_listener={{cred(u)}}")
    let uc = net_connect_unix("cred.sock")?
    let ua = net_accept(u)?
    net_close(ua)?
    print("closed={{cred(ua)}}")
    print("unix_dialed={{cred(uc)}}")
    0
}}
"#
    );
    let (stdout, stderr, code) = run_live("cred_host", &source);
    assert_eq!(
        stdout,
        "forged=io\nnegative=io\ntcp_listener=io\ntcp_dialed=io\ntcp_accepted=io\n\
         unix_listener=io\nclosed=io\n",
        "{stderr}"
    );
    assert_eq!(code, Some(4), "{stderr}");
    assert_by_name("net_peer_cred", &stderr, "a dialed Unix stream");
}

#[cfg(unix)]
#[test]
fn an_accepted_unix_stream_s_credentials_are_refused_by_name() {
    let source = format!(
        r#"{CRED}
fn main() -> !int {{
    let u = net_listen_unix("acc.sock")?
    let _uc = net_connect_unix("acc.sock")?
    let ua = net_accept(u)?
    print("accepted={{cred(ua)}}")
    0
}}
"#
    );
    let (stdout, stderr, code) = run_live("cred_accepted", &source);
    assert_eq!(stdout, "", "{stderr}");
    assert_eq!(code, Some(4), "{stderr}");
    assert_by_name("net_peer_cred", &stderr, "an accepted Unix stream");
}

#[cfg(unix)]
#[test]
fn the_credentials_answer_unsupported_on_a_windows_target_whatever_the_handle() {
    let source = format!(
        r#"{CRED}
fn main() -> !int {{
    print("forged={{cred(1000000)}}")
    let l = net_listen("127.0.0.1:0")?
    let port = net_port(l)?
    let c = net_connect("127.0.0.1:{{port}}")?
    print("tcp_dialed={{cred(c)}}")
    print("listener={{cred(l)}}")
    0
}}
"#
    );
    let (verdict, stdout, why) = observe_as(WINDOWS, &source);
    assert_eq!(verdict, Verdict::Exit(0), "{why:?}");
    assert_eq!(
        stdout,
        "forged=unsupported\ntcp_dialed=unsupported\nlistener=unsupported\n"
    );
    for target in UNIX_TARGETS {
        let (verdict, stdout, why) = observe_as(target, &source);
        assert_eq!(verdict, Verdict::Exit(0), "as {target}: {why:?}");
        assert_eq!(
            stdout, "forged=io\ntcp_dialed=io\nlistener=io\n",
            "as {target}"
        );
    }
}

#[test]
fn the_declared_rows_discriminate_a_handler_s_arms() {
    // Each arm's bare name tests a tag of the call's declared row; with no
    // row it would bind, and the first arm would answer for every tag.
    let cases = [
        (
            format!("os_wait_ready([0], {KILL}, 0)"),
            "invalid => 1, unsupported => 2, io => 3",
            LINUX,
            3,
        ),
        (
            "os_wait_ready([0], 0, -2)".to_owned(),
            "io => 3, unsupported => 2, invalid => 1",
            DARWIN,
            1,
        ),
        (
            "os_wait_ready([0], 0, 0)".to_owned(),
            "io => 3, invalid => 1, unsupported => 2",
            WINDOWS,
            2,
        ),
        (
            "os_ready_open()".to_owned(),
            "io => 3, unsupported => 2",
            WINDOWS,
            2,
        ),
        (
            "os_ready_set(0, 0, 9, 0)".to_owned(),
            "io => 3, unsupported => 2, invalid => 1",
            LINUX,
            1,
        ),
        (
            "os_ready_set(0, 0, 1, 0)".to_owned(),
            "invalid => 1, unsupported => 2, io => 3",
            DARWIN,
            3,
        ),
        (
            "os_ready_set(0, 0, 1, 0)".to_owned(),
            "invalid => 1, io => 3, unsupported => 2",
            WINDOWS,
            2,
        ),
        (
            "os_ready_wait(0, 0, -2)".to_owned(),
            "io => 3, unsupported => 2, invalid => 1",
            LINUX,
            1,
        ),
        (
            "os_ready_wait(0, 0, 0)".to_owned(),
            "invalid => 1, unsupported => 2, io => 3",
            LINUX,
            3,
        ),
        (
            "os_ready_wait(0, 0, 0)".to_owned(),
            "io => 3, invalid => 1, unsupported => 2",
            WINDOWS,
            2,
        ),
        ("os_ready_close(0)".to_owned(), "io => 3", LINUX, 3),
        (
            "net_write_some(1000000, [1 as byte])".to_owned(),
            "busy => 4, closed => 5, invalid => 1, io => 3",
            LINUX,
            3,
        ),
        (
            "net_peer_cred(1000000)".to_owned(),
            "unsupported => 2, io => 3",
            LINUX,
            3,
        ),
        (
            "net_peer_cred(1000000)".to_owned(),
            "io => 3, unsupported => 2",
            WINDOWS,
            2,
        ),
    ];
    for (call, arms, target, code) in cases {
        let bind = if answers_a_value(&call) {
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
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ready_s290/ready_rows.lu");
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

/// The corpus file on this host: its first wait is well-formed, so a unix
/// host refuses it by name before a line prints; a windows host answers
/// every call's windows row and every line is `true`.
#[test]
fn the_corpus_rows_on_this_host() {
    let entry = fixture();
    let dir = scratch("corpus_rows");
    let r = record(&entry, &dir);
    let run = lupin(&["run"], &entry, &dir);
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    if cfg!(windows) {
        let want = declared_stdout(&entry);
        assert_eq!(r["verdict"], "exit(0)", "{r}");
        assert_eq!(r["stdout_inline"], want, "{r}");
        assert_eq!(run.status.code(), Some(0), "{stderr}");
        return;
    }
    assert_eq!(r["verdict"], "unsupported", "{r}");
    assert_eq!(r["x-unsupported-at"], "run", "{r}");
    assert_by_name(
        "os_wait_ready",
        r["x-unsupported"].as_str().unwrap_or_default(),
        "the corpus rows",
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "", "{stderr}");
    assert_eq!(run.status.code(), Some(4), "{stderr}");
    assert_by_name("os_wait_ready", &stderr, "the corpus rows, run");
}

/// The corpus file with its host reading fixed, observed in process: read
/// as windows by the windows target, every line is `true` (exit 0, the
/// declared stdout); read as unix by each unix target, its first wait is
/// refused by name.
#[test]
fn the_corpus_rows_read_as_each_target() {
    let entry = fixture();
    let text = std::fs::read_to_string(&entry).expect("fixture read");
    let reading = "    let windows = os_name() == \"windows\"\n";
    assert_eq!(
        text.matches(reading).count(),
        1,
        "the file reads its host once"
    );
    let as_windows = text.replace(reading, "    let windows = true\n");
    let (verdict, stdout, why) = observe_as(WINDOWS, &as_windows);
    assert_eq!(verdict, Verdict::Exit(0), "as windows: {why:?} {stdout}");
    assert_eq!(stdout, declared_stdout(&entry), "as windows");
    let as_unix = text.replace(reading, "    let windows = false\n");
    for target in UNIX_TARGETS {
        let (verdict, stdout, why) = observe_as(target, &as_unix);
        assert_eq!(verdict, Verdict::Unsupported, "as {target}: {why:?}");
        assert_eq!(stdout, "", "as {target}");
        assert_by_name("os_wait_ready", why.as_deref().unwrap_or_default(), target);
    }
}

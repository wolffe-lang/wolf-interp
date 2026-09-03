//! is36 — the unix-domain family (`[os.net.unix]`, s136/wolf-lang#227).
//!
//! The clause's whole point is that a caller can tell the HOST apart from the
//! PATH: `unsupported` means the runtime does not serve `AF_UNIX` here,
//! refused by name and never a bare `io`, and every other row is about the
//! path the program named. Everything else in the tier serves a unix fd call
//! for call — `net_accept`, `net_read`/`net_write`, the byte pair,
//! `net_deadline`, `net_close` — with two stated exceptions: `net_port` on
//! one is `io` (a path has no port), and `net_close` of a unix LISTENER
//! unlinks its path (the runtime created the socket file, so the runtime
//! removes it).
//!
//! `corpus/net/unix_echo.lu` is the corpus's witness for the same rules, and
//! this machine cannot run it: its first statement is `fs_exists(path)`, and
//! the s38 fs surface is declined here by design (wolf-interp#18 item 6 —
//! an interpreter observing the HOST's filesystem puts the host into a
//! differential comparison). So the family is exercised here instead, on the
//! same shapes, and the corpus row stays out-of-scope for the fs tier rather
//! than for the sockets.
//!
//! Every program below runs with its scratch directory as the working
//! directory, because this machine admits only a RELATIVE socket path that
//! does not climb out of it — the path-shaped twin of the TCP family's
//! "loopback + port 0" discipline.

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

/// Runs `source` with `dir` as the working directory, so a relative socket
/// path lands inside the scratch.
fn run_in(dir: &Path, source: &str) -> Output {
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("written");
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("run")
        .arg(&entry)
        .current_dir(dir)
        .output()
        .expect("lupin runs")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

#[cfg(unix)]
#[test]
fn the_family_serves_an_echo_and_the_listener_close_unlinks_its_path() {
    // `corpus/net/unix_echo.lu`'s three lanes, minus its two `fs_*` calls:
    // the echo over a socket PATH, `net_port` answering `io` for a socket
    // that has no port, and the cleanup posture the clause states.
    let dir = scratch("unix-echo");
    let source = r#"
fn main() -> !int {
    let path = "echo.sock"
    let srv = net_listen_unix(path)?
    var port_is_io = false
    let _p = net_port(srv) else |_| {
        port_is_io = true
        0
    }
    let cli = net_connect_unix(path)?
    net_write(cli, "ping")?
    let conn = net_accept(srv)?
    let got = net_read(conn, 16)?
    net_write(conn, "pong {got}")?
    let reply = net_read(cli, 16)?
    let echo = got == "ping" && reply == "pong ping"
    net_close(cli)?
    net_close(conn)?
    net_close(srv)?
    print("echo {echo} | port_is_io {port_is_io}")
    0
}
"#;
    let output = run_in(&dir, source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "echo true | port_is_io true\n");
    assert!(
        !dir.join("echo.sock").exists(),
        "`net_close` of a unix LISTENER unlinks its path ([os.net.unix])"
    );
}

#[cfg(unix)]
#[test]
fn the_byte_pair_serves_a_unix_stream_call_for_call() {
    // "the byte pair … serve it call for call": one `List[byte]` over a
    // socket path, un-decodable on purpose, compared octet for octet.
    let dir = scratch("unix-bytes");
    let source = r#"
fn main() -> !int {
    let srv = net_listen_unix("b.sock")?
    let cli = net_connect_unix("b.sock")?
    var payload = List[byte]()
    (mut payload).push(255 as byte)
    (mut payload).push(0 as byte)
    (mut payload).push(128 as byte)
    net_write_bytes(cli, payload)?
    let conn = net_accept(srv)?
    let got = net_read_bytes(conn, 16)?
    var line = "got {got.len}:"
    for b in got {
        line = "{line} {b}"
    }
    print(line)
    net_close(cli)?
    net_close(conn)?
    net_close(srv)?
    0
}
"#;
    let output = run_in(&dir, source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "got 3: 255 0 128\n");
}

#[cfg(unix)]
#[test]
fn every_bind_and_dial_row_is_about_the_path() {
    // The rows that are NOT the host: an existing path is `exists` at bind
    // (a stale socket file is the operator's to remove — the runtime never
    // clobbers a path it did not bind), a missing directory is `not_found`,
    // no socket file at dial is `not_found`, and a socket file nobody
    // listens on is `refused`.
    //
    // The fourth shape is the clause's own — a real `AF_UNIX` path whose
    // listener is gone, which is what a process that died without closing
    // leaves behind — so the stale file is made by BINDING and dropping,
    // not by writing bytes. A plain regular file at dial is neither "no
    // socket file" nor "a file nobody listens on": the kernel answers
    // `ENOTSOCK` and both machines call it `io` (measured against wolfc at
    // pin 4230b00: `dial-plain io | bind-over-plain exists`).
    let dir = scratch("unix-rows");
    {
        let stale = std::os::unix::net::UnixListener::bind(dir.join("stale.sock"))
            .expect("a stale socket file");
        drop(stale);
    }
    std::fs::write(dir.join("plain"), b"x").expect("plain file");
    let source = r#"
fn main() -> !int {
    var exists = "none"
    let a = net_listen_unix("stale.sock") else |e| match e {
        exists => { exists = "exists"; 0 },
        _ => { exists = "other"; 0 },
    }
    var missing_dir = "none"
    let b = net_listen_unix("nodir/x.sock") else |e| match e {
        not_found => { missing_dir = "not_found"; 0 },
        _ => { missing_dir = "other"; 0 },
    }
    var no_file = "none"
    let c = net_connect_unix("absent.sock") else |e| match e {
        not_found => { no_file = "not_found"; 0 },
        _ => { no_file = "other"; 0 },
    }
    var not_listening = "none"
    let d = net_connect_unix("stale.sock") else |e| match e {
        refused => { not_listening = "refused"; 0 },
        _ => { not_listening = "other"; 0 },
    }
    var non_socket = "none"
    let f = net_connect_unix("plain") else |e| match e {
        io => { non_socket = "io"; 0 },
        _ => { non_socket = "other"; 0 },
    }
    print("{exists} {missing_dir} {no_file} {not_listening} {non_socket} {a}{b}{c}{d}{f}")
    0
}
"#;
    let output = run_in(&dir, source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        stdout_of(&output),
        "exists not_found not_found refused io 00000\n"
    );
    assert!(
        dir.join("stale.sock").exists(),
        "a bind refused with `exists` never clobbers the path it did not bind"
    );
}

#[cfg(unix)]
#[test]
fn a_socket_path_that_climbs_out_of_the_working_directory_is_refused_by_name() {
    // Not a ROW: the `unsupported` row means the host does not serve the
    // family, and claiming it on a host that does would be a lie. A path
    // outside the working directory is outside this machine's declared
    // surface, so it is the by-name refusal — exit 4, like every other
    // "outside the v0 surface" answer in the tier.
    let dir = scratch("unix-escape");
    let output = run_in(
        &dir,
        "fn main() -> !int {\n    let s = net_listen_unix(\"/tmp/x.sock\")?\n    print(\"{s}\")\n    0\n}\n",
    );
    assert_eq!(output.status.code(), Some(4), "{output:?}");
    let stderr = String::from_utf8(output.stderr.clone()).expect("utf8 stderr");
    assert!(
        stderr.contains("outside the working directory"),
        "the refusal names the shape: {stderr}"
    );
}

#[cfg(unix)]
#[test]
fn a_stream_close_never_touches_the_path() {
    // "a stream's close never touches the path" — the other half of the
    // cleanup posture, and the half a careless unlink-on-every-close breaks.
    let dir = scratch("unix-stream-close");
    let source = r#"
fn main() -> !int {
    let srv = net_listen_unix("s.sock")?
    let cli = net_connect_unix("s.sock")?
    let conn = net_accept(srv)?
    net_close(cli)?
    net_close(conn)?
    print("streams closed")
    0
}
"#;
    let output = run_in(&dir, source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "streams closed\n");
    assert!(
        dir.join("s.sock").exists(),
        "only the LISTENER's close unlinks; the listener here was never closed"
    );
}

#[cfg(not(unix))]
#[test]
fn a_host_that_does_not_serve_the_family_refuses_by_name() {
    // `[os.net.unix]`'s host posture on the other side: windows answers the
    // `unsupported` ROW — refused BY NAME and never a bare `io`, which is
    // the distinction wolf-lang#227 exists to make measurable.
    let dir = scratch("unix-unsupported");
    let source = r#"
fn main() -> !int {
    var row = "none"
    let fd = net_listen_unix("x.sock") else |e| match e {
        unsupported => { row = "unsupported"; 0 },
        _ => { row = "other"; 0 },
    }
    print("{row} {fd}")
    0
}
"#;
    let output = run_in(&dir, source);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(stdout_of(&output), "unsupported 0\n");
}

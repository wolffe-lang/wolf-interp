//! The s39 net family through the wolf surface — the probed edge shapes,
//! asserted against lupin exactly as they were probed against the compiled
//! lanes (the corpus witnesses under `corpus/net/` carry the happy paths;
//! these are the edges the census never walks).

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

fn run_program(dir: &Path, source: &str) -> Output {
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).expect("written");
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("run")
        .arg(&entry)
        .output()
        .expect("lupin runs")
}

#[test]
fn the_probed_edge_rows_answer_as_the_compiled_lanes_do() {
    // One program, every probed edge: unparseable listen is `io`, a dial
    // at a dead port is a row, double close is `io`, the peer's finish is
    // `closed` on read, the first write after a peer close succeeds and
    // the second is the row, accept on a stream is `io`, a forged fd is
    // `io`, clearing a deadline is legal. Byte-for-byte the transcript the
    // compiled lanes answered under probe.
    let dir = scratch("net-edges");
    let source = "fn main() -> !int {\n\
        \x20   net_listen(\"garbage\") else |_| { print(\"listen-garbage\"); -1 }\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   let port = net_port(srv)?\n\
        \x20   let cli = net_connect(\"127.0.0.1:{port}\")?\n\
        \x20   let conn = net_accept(srv)?\n\
        \x20   let cp = net_port(cli)?\n\
        \x20   if cp > 0 { print(\"cli-port-pos\") } else { print(\"cli-port-nonpos\") }\n\
        \x20   net_close(cli)?\n\
        \x20   net_close(cli) else |_| print(\"double-close\")\n\
        \x20   net_read(conn, 8) else |_| { print(\"read-after-peer-close\"); \"?\" }\n\
        \x20   net_write(conn, \"x\") else |_| print(\"write-after-peer-close-1\")\n\
        \x20   net_write(conn, \"x\") else |_| print(\"write-after-peer-close-2\")\n\
        \x20   net_accept(conn) else |_| { print(\"accept-on-stream\"); -1 }\n\
        \x20   net_read(99, 4) else |_| { print(\"read-forged\"); \"?\" }\n\
        \x20   net_deadline(srv, -5)?\n\
        \x20   print(\"deadline-clear-ok\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(&dir, source);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{output:?}");
    assert_eq!(
        stdout,
        "listen-garbage\ncli-port-pos\ndouble-close\nread-after-peer-close\n\
         write-after-peer-close-2\naccept-on-stream\nread-forged\ndeadline-clear-ok\n"
    );
}

#[test]
fn the_probed_row_tags_are_exact_on_the_propagation_path() {
    // `?` makes the tag the documented process outcome — the transcript
    // the compiled lanes answered: `closed` for the EOF read, `io` for the
    // double close.
    let dir = scratch("net-tags");
    let source = "fn main() -> !int {\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   let port = net_port(srv)?\n\
        \x20   let cli = net_connect(\"127.0.0.1:{port}\")?\n\
        \x20   let conn = net_accept(srv)?\n\
        \x20   net_close(cli)?\n\
        \x20   let m = net_read(conn, 8)?\n\
        \x20   0\n\
        }\n";
    let output = run_program(&dir, source);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "error: closed\n");
}

#[test]
fn a_zero_length_read_answers_the_empty_string_never_a_false_closed() {
    let dir = scratch("net-zero-read");
    let source = "fn main() -> !int {\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   let port = net_port(srv)?\n\
        \x20   let cli = net_connect(\"127.0.0.1:{port}\")?\n\
        \x20   let empty = net_read(cli, 0)?\n\
        \x20   if empty == \"\" { print(\"empty\") } else { return 1 }\n\
        \x20   net_close(cli)?\n\
        \x20   net_close(srv)?\n\
        \x20   0\n\
        }\n";
    let output = run_program(&dir, source);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{output:?}");
    assert_eq!(stdout, "empty\n");
}

#[test]
fn shapes_outside_the_declared_surface_refuse_by_name() {
    // Non-loopback and fixed listen ports are OUTSIDE the v0 surface this
    // machine implements: the verdict is `unsupported` naming the shape —
    // never a guessed row, never a socket on the open network.
    let dir = scratch("net-outside");
    for (source, needle) in [
        (
            "fn main() -> !int {\n    net_listen(\"0.0.0.0:0\")?\n    0\n}\n",
            "non-loopback",
        ),
        (
            "fn main() -> !int {\n    net_listen(\"127.0.0.1:8080\")?\n    0\n}\n",
            "FIXED port",
        ),
        (
            "fn main() -> !int {\n    net_connect(\"8.8.8.8:53\")?\n    0\n}\n",
            "non-loopback",
        ),
    ] {
        let output = run_program(&dir, source);
        // The front door exits 4 for `unsupported`.
        assert_eq!(output.status.code(), Some(4), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(needle), "{stderr}");
    }
}

#[test]
fn a_deadline_resolves_an_accept_park_as_the_timeout_row() {
    // The corpus pins the READ half (`read_deadline.lu`); this is the
    // accept half of the same s106 rule — the armed budget fires and the
    // parking call resolves as its row's `timeout` tag.
    let dir = scratch("net-accept-deadline");
    let source = "fn main() -> !int {\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   net_deadline(srv, 40)?\n\
        \x20   let conn = net_accept(srv)?\n\
        \x20   print(\"unreachable: {conn}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(&dir, source);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "error: timeout\n");
}

//! is40 — the s141 pair (`[os.net.writev]`, `[os.net.nodelay]`) and the one
//! thing `[os.net.io]` moved, at the edges the corpus witnesses do not walk.
//!
//! `corpus/net/{writev_gather,nodelay,syscall_first}.lu` carry the happy
//! paths and the four relations each clause states, and this machine runs all
//! three (`tests/run_corpus.rs`'s ledger). What is here instead:
//!
//! - the two degenerate gathers through the LANGUAGE — no parts at all, and
//!   parts that are all empty. The witness carries the second; the first is
//!   the one that walks straight off the end of the cursor loop.
//! - `[os.net.nodelay]`'s unix-domain row. The clause says a unix-domain
//!   stream is `io` ("the option is TCP's"), and the corpus witness cannot
//!   reach it: `corpus/net/unix_echo.lu` opens with `fs_exists`, which this
//!   machine declines by design (wolf-interp#18 item 6), so the unix family
//!   is exercised here exactly as `tests/net_unix.rs` does. The gather, by
//!   contrast, DOES serve a unix stream — one clause excludes the family and
//!   the other does not, and the difference is asserted rather than assumed.
//! - the closed-handle `io` on both new calls, which is a different arm of
//!   `NetTable::slot` than a forged one.
//! - the budget row `[os.net.io]` coarsens. `corpus/net/syscall_first.lu`
//!   asserts a budgeted large `net_write` answers `io`; this asserts the same
//!   coarsening reaches `net_writev`, and that `net_read` — which DOES
//!   declare `timeout` — still answers `timeout`. The coarsening is a rule
//!   about the declared row, so it is tested on both sides of that rule.
//!
//! The resumption path — a kernel that stops in the middle of a part — is
//! NOT here, and deliberately. Reaching it needs a gather larger than the
//! send buffer, which is megabytes of `List[byte]` in the interpreter's own
//! value representation, and that is the wolf-interp#63 cliff: a debug lupin
//! spends minutes on 64 KiB of it. `eval::net`'s
//! `a_trickling_writer_resumes_mid_part_and_the_gather_arrives_in_order`
//! drives the same `drive_writev` against a writer that stops where a kernel
//! would, for free.

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

const BYTES_OF: &str = "fn bytes_of(s: str) -> List[byte] {\n\
    \x20   var out = List[byte]()\n\
    \x20   for b in s.bytes() { (mut out).push(b) }\n\
    \x20   out\n\
    }\n";

#[test]
fn the_degenerate_gathers_complete_and_send_nothing() {
    // "Empty parts are permitted and send nothing; a call whose parts are all
    // empty is a completed write with no syscall." Both are followed by a
    // real gather, and the peer reads exactly that and nothing else — which
    // is what "sends nothing" has to mean from outside.
    let dir = scratch("net-s141-writev-empty");
    let source = format!(
        "{BYTES_OF}\
         fn main() -> !int {{\n\
         \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
         \x20   let port = net_port(srv)?\n\
         \x20   let cli = net_connect(\"127.0.0.1:{{port}}\")?\n\
         \x20   let conn = net_accept(srv)?\n\
         \x20   var none = List[List[byte]]()\n\
         \x20   net_writev(conn, none)?\n\
         \x20   print(\"no-parts-ok\")\n\
         \x20   var empties = List[List[byte]]()\n\
         \x20   (mut empties).push(List[byte]())\n\
         \x20   (mut empties).push(List[byte]())\n\
         \x20   net_writev(conn, empties)?\n\
         \x20   print(\"all-empty-ok\")\n\
         \x20   var parts = List[List[byte]]()\n\
         \x20   (mut parts).push(bytes_of(\"ab\"))\n\
         \x20   (mut parts).push(List[byte]())\n\
         \x20   (mut parts).push(bytes_of(\"cd\"))\n\
         \x20   net_writev(conn, parts)?\n\
         \x20   net_deadline(cli, 5000)?\n\
         \x20   var got = \"\"\n\
         \x20   while got.len < 4 {{\n\
         \x20       let piece = net_read(cli, 16)?\n\
         \x20       got = \"{{got}}{{piece}}\"\n\
         \x20   }}\n\
         \x20   print(\"got {{got}}\")\n\
         \x20   net_close(conn)?\n\
         \x20   net_close(cli)?\n\
         \x20   net_close(srv)?\n\
         \x20   0\n\
         }}\n"
    );
    let output = run_in(&dir, &source);
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{output:?}");
    assert_eq!(stdout, "no-parts-ok\nall-empty-ok\ngot abcd\n");
}

#[cfg(unix)]
#[test]
fn nodelay_on_a_unix_domain_stream_is_io_and_the_gather_still_serves_it() {
    // `[os.net.nodelay]`: "a listener, a unix-domain stream (the option is
    // TCP's), a forged or a closed handle is `io`". The corpus witness
    // asserts the listener and the forged handle; the unix-domain row is only
    // reachable where the family is.
    //
    // `[os.net.writev]` names a LISTENER and a FORGED handle as `io` and says
    // nothing against the second address family, so `net_writev` serves a
    // unix stream call for call, exactly as `[os.net.unix]` says the rest of
    // the tier does.
    let dir = scratch("net-s141-nodelay-unix");
    let source = format!(
        "{BYTES_OF}\
         fn main() -> !int {{\n\
         \x20   let srv = net_listen_unix(\"s141.sock\")?\n\
         \x20   let cli = net_connect_unix(\"s141.sock\")?\n\
         \x20   let conn = net_accept(srv)?\n\
         \x20   net_nodelay(conn, true) else |e| match e {{\n\
         \x20       io => print(\"unix-stream-io\"),\n\
         \x20   }}\n\
         \x20   net_nodelay(srv, true) else |e| match e {{\n\
         \x20       io => print(\"unix-listener-io\"),\n\
         \x20   }}\n\
         \x20   var parts = List[List[byte]]()\n\
         \x20   (mut parts).push(bytes_of(\"uni\"))\n\
         \x20   (mut parts).push(bytes_of(\"x!\"))\n\
         \x20   net_writev(conn, parts)?\n\
         \x20   net_deadline(cli, 5000)?\n\
         \x20   var got = \"\"\n\
         \x20   while got.len < 5 {{\n\
         \x20       let piece = net_read(cli, 16)?\n\
         \x20       got = \"{{got}}{{piece}}\"\n\
         \x20   }}\n\
         \x20   print(\"gathered {{got}}\")\n\
         \x20   net_close(conn)?\n\
         \x20   net_close(cli)?\n\
         \x20   net_close(srv)?\n\
         \x20   0\n\
         }}\n"
    );
    let output = run_in(&dir, &source);
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{output:?}");
    assert_eq!(stdout, "unix-stream-io\nunix-listener-io\ngathered unix!\n");
}

#[test]
fn a_closed_handle_is_io_on_both_new_calls() {
    // The fourth `io` in each clause's list, and the one the corpus witnesses
    // skip because a spent fd is a shape they never build. A closed handle is
    // NOT a forged one: the slot exists and has been taken, which is a
    // different arm of `NetTable::slot`/`sock` than an out-of-range index.
    let dir = scratch("net-s141-closed");
    let source = format!(
        "{BYTES_OF}\
         fn main() -> !int {{\n\
         \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
         \x20   let port = net_port(srv)?\n\
         \x20   let cli = net_connect(\"127.0.0.1:{{port}}\")?\n\
         \x20   let conn = net_accept(srv)?\n\
         \x20   net_close(conn)?\n\
         \x20   net_nodelay(conn, true) else |e| match e {{\n\
         \x20       io => print(\"nodelay-closed-io\"),\n\
         \x20   }}\n\
         \x20   var parts = List[List[byte]]()\n\
         \x20   (mut parts).push(bytes_of(\"x\"))\n\
         \x20   net_writev(conn, parts) else |e| match e {{\n\
         \x20       io => print(\"writev-closed-io\"),\n\
         \x20       _ => print(\"writev-closed-OTHER\"),\n\
         \x20   }}\n\
         \x20   net_close(cli)?\n\
         \x20   net_close(srv)?\n\
         \x20   0\n\
         }}\n"
    );
    let output = run_in(&dir, &source);
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{output:?}");
    assert_eq!(stdout, "nodelay-closed-io\nwritev-closed-io\n");
}

#[test]
fn a_fired_budget_answers_the_row_the_call_declares_and_not_a_wider_tag() {
    // `[os.net.io]`, the only source motion the clause cost this machine.
    // A write's budget covers the whole drain, so a `net_deadline` armed on a
    // stream whose peer never reads fires INSIDE a write — and `net_writev`
    // declares `{closed, io}`, with no `timeout` in the row. The clause: "the
    // row is `net_write`'s `io` (its `timeout` coarsened, as the call
    // declares)".
    //
    // Both sides of the rule are asserted in one program. `net_read` DOES
    // declare `timeout`, so the same budget on the same socket answers
    // `timeout` there — which is what makes this a rule about the declared
    // row rather than a blanket rewrite of the tag.
    //
    // The send buffer is filled with `net_write` and a `str`, not with a
    // `List[byte]`: a list is one interpreter value per octet, and filling a
    // socket with freshly built megabytes of them is the wolf-interp#63
    // cliff. The gather that follows is ONE 4 KiB part, built once and
    // re-sent: a write that comes back at its budget has not necessarily left
    // the buffer with zero slack — the kernel keeps moving the last segments
    // across loopback while the call is parked — and a five-byte gather slid
    // into that slack on the first attempt. Four kilobytes against at most 64
    // attempts exhausts it in one or two, and both loops are bounded so a
    // host whose buffers never fill is a failing test, not a hung runner.
    let dir = scratch("net-s141-budget-row");
    let source = format!(
        "fn fill(c: str, n: int) -> str {{\n\
         \x20   var s = c\n\
         \x20   while s.len < n {{ s = \"{{s}}{{s}}\" }}\n\
         \x20   s[..n]\n\
         }}\n\
         {BYTES_OF}\
         fn main() -> !int {{\n\
         \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
         \x20   let port = net_port(srv)?\n\
         \x20   let cli = net_connect(\"127.0.0.1:{{port}}\")?\n\
         \x20   let conn = net_accept(srv)?\n\
         \x20   let chunk = fill(\"x\", 65536)\n\
         \x20   net_deadline(conn, 300)?\n\
         \x20   var filled = false\n\
         \x20   var n = 0\n\
         \x20   while n < 8192 && filled == false {{\n\
         \x20       net_write(conn, chunk) else |_| {{ filled = true }}\n\
         \x20       n = n + 1\n\
         \x20   }}\n\
         \x20   if filled == false {{ print(\"BUFFER-NEVER-FILLED\") }}\n\
         \x20   var parts = List[List[byte]]()\n\
         \x20   (mut parts).push(bytes_of(fill(\"y\", 4096)))\n\
         \x20   var vstopped = false\n\
         \x20   var m = 0\n\
         \x20   while m < 64 && vstopped == false {{\n\
         \x20       net_writev(conn, parts) else |e| match e {{\n\
         \x20           io => {{ vstopped = true; print(\"writev-io\") }},\n\
         \x20           _ => {{ vstopped = true; print(\"writev-OTHER\") }},\n\
         \x20       }}\n\
         \x20       m = m + 1\n\
         \x20   }}\n\
         \x20   if vstopped == false {{ print(\"WRITEV-NEVER-STOPPED\") }}\n\
         \x20   net_deadline(conn, 40)?\n\
         \x20   net_read(conn, 8) else |e| match e {{\n\
         \x20       timeout => {{ print(\"read-timeout\"); \"?\" }},\n\
         \x20       _ => {{ print(\"read-OTHER\"); \"?\" }},\n\
         \x20   }}\n\
         \x20   net_close(conn)?\n\
         \x20   net_close(cli)?\n\
         \x20   net_close(srv)?\n\
         \x20   0\n\
         }}\n"
    );
    let output = run_in(&dir, &source);
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{output:?}");
    assert_eq!(stdout, "writev-io\nread-timeout\n");
}

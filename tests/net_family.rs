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
    // a later write is the row, accept on a stream is `io`, a forged fd is
    // `io`, clearing a deadline is legal. Byte-for-byte the transcript the
    // compiled lanes answered under probe — with one timing honesty: the
    // failing write happens only once the peer's RST has been processed,
    // and *when* that lands is the kernel's business (on Linux loopback the
    // very next write fails; macOS needs a few ms), so the program retries
    // the post-close write on a bounded clock instead of asserting the
    // race. The transcript is unchanged: the row prints exactly once.
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
        \x20   var tries = 0\n\
        \x20   while tries < 400 {\n\
        \x20       time_sleep_ms(5)\n\
        \x20       net_write(conn, \"x\") else |_| {\n\
        \x20           tries = 1000000\n\
        \x20           print(\"write-after-peer-close-2\")\n\
        \x20       }\n\
        \x20       tries = tries + 1\n\
        \x20   }\n\
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

#[test]
fn a_sleeping_sibling_does_not_starve_an_armed_deadline() {
    // wolf-interp#40: a sibling task parked in `time_sleep_ms(500)` held
    // the baton through a raw `thread::sleep`, so a 60ms `net_deadline` in
    // the reading task was observed at ~503ms and resolved as the peer's
    // `closed` instead of its own `timeout`. The is20 fix parks the TASK
    // and makes the scheduler's park bound the earliest pending wakeup.
    //
    // The witness: the deadline fires as `timeout` within its declared
    // 60ms window plus a stated epsilon of 340ms — process spawn, program
    // load, the ~1ms poll cadence and the cancelled sibling's teardown —
    // chosen to stay decisively below the sibling's 500ms sleep (the base
    // bug cannot pass it). Five runs per spawn ordering: ten runs total,
    // all stable.
    let sleeper_first = "fn main() -> !int {\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   let port = net_port(srv)?\n\
        \x20   scope s {\n\
        \x20       s.spawn(fn() {\n\
        \x20           let conn = net_accept(srv) else |_| -1\n\
        \x20           time_sleep_ms(500)\n\
        \x20           net_close(conn) else |_| {}\n\
        \x20       })\n\
        \x20       let cli = net_connect(\"127.0.0.1:{port}\")?\n\
        \x20       net_deadline(cli, 60)?\n\
        \x20       let got = net_read(cli, 64)?\n\
        \x20       print(\"read-ok: {got.len}\")\n\
        \x20       net_close(cli)?\n\
        \x20   }\n\
        \x20   net_close(srv)?\n\
        \x20   0\n\
        }\n";
    let dialer_first = "fn main() -> !int {\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   let port = net_port(srv)?\n\
        \x20   scope s {\n\
        \x20       let cli = net_connect(\"127.0.0.1:{port}\")?\n\
        \x20       s.spawn(fn() {\n\
        \x20           let conn = net_accept(srv) else |_| -1\n\
        \x20           time_sleep_ms(500)\n\
        \x20           net_close(conn) else |_| {}\n\
        \x20       })\n\
        \x20       net_deadline(cli, 60)?\n\
        \x20       let got = net_read(cli, 64)?\n\
        \x20       print(\"read-ok: {got.len}\")\n\
        \x20       net_close(cli)?\n\
        \x20   }\n\
        \x20   net_close(srv)?\n\
        \x20   0\n\
        }\n";
    for (name, source) in [
        ("net-deadline-sleeper-first", sleeper_first),
        ("net-deadline-dialer-first", dialer_first),
    ] {
        let dir = scratch(name);
        for round in 0..5 {
            let started = std::time::Instant::now();
            let output = run_program(&dir, source);
            let elapsed = started.elapsed();
            assert_eq!(output.status.code(), Some(1), "{name}#{round}: {output:?}");
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                "error: timeout\n",
                "{name}#{round}: the deadline's own row, never the sleeping peer's"
            );
            assert!(
                elapsed < std::time::Duration::from_millis(400),
                "{name}#{round}: the 60ms deadline was observed at {elapsed:?} — \
                 a sleeping sibling is starving the timer again (#40)"
            );
        }
    }
}

#[test]
fn a_lone_concurrent_sleeper_parks_the_world_for_exactly_its_duration() {
    // The other face of the #40 park bound: when the sleeper is the ONLY
    // runnable task, the scheduler must wait for its real wakeup — not
    // declare deadlock (nothing ready, no virtual timer) and not spin.
    let dir = scratch("net-lone-sleeper");
    let source = "fn main() -> !int {\n\
        \x20   scope s {\n\
        \x20       s.spawn(fn() {\n\
        \x20           time_sleep_ms(80)\n\
        \x20           print(\"slept\")\n\
        \x20       })\n\
        \x20   }\n\
        \x20   0\n\
        }\n";
    let started = std::time::Instant::now();
    let output = run_program(&dir, source);
    let elapsed = started.elapsed();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "slept\n");
    assert!(
        elapsed >= std::time::Duration::from_millis(80),
        "a sleep is real: {elapsed:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(400),
        "the join woke promptly after the sleep: {elapsed:?}"
    );
}

#[test]
fn a_handler_over_a_builtin_raised_row_discriminates_in_both_arm_orders() {
    // Issue #47 (wolf-std F-0097): a multi-arm `else |e| match e` over a row
    // a BUILTIN raised took its first arm for every tag — the value carried
    // no row, so every lowercase arm read as a binding and a binding matches
    // anything. The issue's own reproducer, verbatim in shape: one dead
    // port, dialed twice, the discriminating handler spelled in BOTH orders.
    // The tag really is `refused` (nothing listens on a just-closed port 0
    // pick), so both orders must land the `refused` arm — before the fix
    // this printed `-1` then `-9`, the answer following the arm order.
    let dir = scratch("net-builtin-row-arms");
    let source = "fn main() -> !int {\n\
        \x20   let fd = net_listen(\"127.0.0.1:0\")?\n\
        \x20   let p = net_port(fd)?\n\
        \x20   net_close(fd)?\n\
        \x20   let s = net_connect(\"127.0.0.1:{p}\") else |e| match e {\n\
        \x20       refused => 0 - 1,\n\
        \x20       timeout => 0 - 8,\n\
        \x20       io => 0 - 9,\n\
        \x20   }\n\
        \x20   print(\"refused-first: {s}\")\n\
        \x20   let t = net_connect(\"127.0.0.1:{p}\") else |e| match e {\n\
        \x20       io => 0 - 9,\n\
        \x20       timeout => 0 - 8,\n\
        \x20       refused => 0 - 1,\n\
        \x20   }\n\
        \x20   print(\"refused-last: {t}\")\n\
        \x20   0\n\
        }\n";
    let output = run_program(&dir, source);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{output:?}");
    assert_eq!(
        stdout, "refused-first: -1\nrefused-last: -1\n",
        "the tag finds its own arm in either order — never the first arm"
    );
}

#[test]
fn the_byte_tier_round_trips_raw_bytes_including_non_utf8() {
    // The s106 byte pair (is30; wolf-interp#52 / wolf-std F-0102): a
    // `List[int]` round-trip over loopback, dial-first, carrying bytes no
    // `str` could — `0x80` alone is mis-encoded UTF-8 and the byte tier
    // has NO utf8 row to trip. The transcript is the issue's own repro,
    // and the compiled lanes' answer at v0.2.0: `got: 0 255 128 7`.
    let dir = scratch("net-bytes-round-trip");
    let source = "fn main() -> !int {\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   let p = net_port(srv)?\n\
        \x20   let cli = net_connect(\"127.0.0.1:{p}\")?\n\
        \x20   var msg = List[int]()\n\
        \x20   (mut msg).push(0)\n\
        \x20   (mut msg).push(255)\n\
        \x20   (mut msg).push(128)\n\
        \x20   (mut msg).push(7)\n\
        \x20   net_write_bytes(cli, msg)?\n\
        \x20   let conn = net_accept(srv)?\n\
        \x20   let got = net_read_bytes(conn, 16)?\n\
        \x20   print(\"got: {got[0]} {got[1]} {got[2]} {got[3]}\")\n\
        \x20   net_close(cli)?\n\
        \x20   net_close(conn)?\n\
        \x20   net_close(srv)?\n\
        \x20   0\n\
        }\n";
    let output = run_program(&dir, source);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert_eq!(stdout, "got: 0 255 128 7\n");
}

#[test]
fn a_non_byte_element_is_the_invalid_row_and_nothing_reaches_the_wire() {
    // `net_write_bytes`' pre-write check is WHOLE (wolf-std's
    // write_bytes_invalid_row witness): an element outside 0..=255 is the
    // `invalid` row by name, and no prefix is sent — the peer then reads
    // only what the LEGAL write carried. Both directions of the domain
    // edge (256 and -1) answer the same tag.
    let dir = scratch("net-bytes-invalid");
    let source = "fn main() -> !int {\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   let p = net_port(srv)?\n\
        \x20   let cli = net_connect(\"127.0.0.1:{p}\")?\n\
        \x20   var bad = List[int]()\n\
        \x20   (mut bad).push(65)\n\
        \x20   (mut bad).push(256)\n\
        \x20   net_write_bytes(cli, bad) else |e| match e {\n\
        \x20       closed => { print(\"handled: closed\") },\n\
        \x20       invalid => { print(\"handled: invalid\") },\n\
        \x20       io => { print(\"handled: io\") },\n\
        \x20   }\n\
        \x20   var worse = List[int]()\n\
        \x20   (mut worse).push(0 - 1)\n\
        \x20   net_write_bytes(cli, worse) else |_| print(\"handled: negative\")\n\
        \x20   var ok = List[int]()\n\
        \x20   (mut ok).push(66)\n\
        \x20   net_write_bytes(cli, ok)?\n\
        \x20   let conn = net_accept(srv)?\n\
        \x20   let got = net_read_bytes(conn, 8)?\n\
        \x20   print(\"len: {got.len} first: {got[0]}\")\n\
        \x20   net_close(cli)?\n\
        \x20   net_close(conn)?\n\
        \x20   net_close(srv)?\n\
        \x20   0\n\
        }\n";
    let output = run_program(&dir, source);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert_eq!(
        stdout,
        "handled: invalid\nhandled: negative\nlen: 1 first: 66\n"
    );
}

#[test]
fn a_zero_length_byte_read_answers_the_empty_list_never_a_false_closed() {
    // The str tier's own n<=0 rule, mirrored: the socket is never asked.
    let dir = scratch("net-bytes-zero");
    let source = "fn main() -> !int {\n\
        \x20   let srv = net_listen(\"127.0.0.1:0\")?\n\
        \x20   let p = net_port(srv)?\n\
        \x20   let cli = net_connect(\"127.0.0.1:{p}\")?\n\
        \x20   let conn = net_accept(srv)?\n\
        \x20   let none = net_read_bytes(conn, 0)?\n\
        \x20   print(\"len: {none.len}\")\n\
        \x20   net_close(cli)?\n\
        \x20   net_close(conn)?\n\
        \x20   net_close(srv)?\n\
        \x20   0\n\
        }\n";
    let output = run_program(&dir, source);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert_eq!(stdout, "len: 0\n");
}

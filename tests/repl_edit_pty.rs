//! is25 end-to-end: the line editor is REALLY wired, proven by keystrokes
//! against the built binary under a PTY — not by the crate's documentation.
//!
//! The three headline claims of the sprint, each the exact failure the base
//! audit measured at 7b13638:
//!
//! 1. **Up recalls** — at base, `Up` arrived as literal `^[[A` and wedged
//!    the session into an inescapable `....>` continuation (wolf-interp#46's
//!    easy path in).
//! 2. **`Alt-.` yanks the last argument, cycling on repeat** — at base it
//!    inserted literal escape bytes.
//! 3. **`Ctrl-C` abandons the input and the session survives** — at base it
//!    killed the whole process (default SIGINT). This is #46's fix: the
//!    continuation has an escape, and the world survives it.
//!
//! Platform gating, stated plainly: this file is `#[cfg(unix)]` because it
//! drives a POSIX PTY (`rexpect` is a unix-only dev-dependency). The Windows
//! path is covered otherwise: rustyline's console backend is that crate's
//! tier-1 surface, our own layer (completer, validator, history policy,
//! yank-last-arg) is platform-pure and unit-tested in `src/edit.rs` on every
//! OS, and the `%APPDATA%`-shaped history resolution is unit-tested by
//! injection (`windows_resolves_appdata_shaped`). The exhaustive
//! binding-by-binding evidence run is `tools/replkeys_probe.py`.
//!
//! Timeouts double as assertions here: at base, "recall" leaves the session
//! in `....>` and the expected value line never arrives, so `exp_string`
//! times out and the test fails — exactly the regression this guards.
#![cfg(unix)]

use std::process::Command;

use rexpect::session::{PtySession, spawn_command};

const TIMEOUT_MS: u64 = 20_000;

/// A fresh interactive session under a PTY, with history isolated to a
/// per-test temp file so a developer's real history never leaks in.
fn spawn(test: &str) -> PtySession {
    let history = std::env::temp_dir().join(format!("lupin-pty-{}-{test}", std::process::id()));
    let _ = std::fs::remove_file(&history);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lupin"));
    cmd.current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("TERM", "xterm-256color")
        .env("LUPIN_HISTORY", &history);
    spawn_command(cmd, Some(TIMEOUT_MS)).expect("lupin spawns under a PTY")
}

fn send(session: &mut PtySession, bytes: &str) {
    session.send(bytes).expect("PTY accepts input");
    session.flush().expect("PTY flushes");
}

/// Sends after a beat. Keystroke pacing matters for `Ctrl-C`: the editor's
/// cancel contract holds AT THE PROMPT (raw mode); a byte raced in while the
/// session is between reads meets the terminal's normal signal handling.
/// A human cannot type between reads; a PTY writer can, so the test paces.
fn send_paced(session: &mut PtySession, bytes: &str) {
    std::thread::sleep(std::time::Duration::from_millis(400));
    send(session, bytes);
}

#[test]
fn up_arrow_recalls_history_instead_of_inserting_escape_bytes() {
    let mut lupin = spawn("up-recall");
    send(&mut lupin, "1 + 1\r");
    lupin.exp_string("2 : i32").expect("the first evaluation");
    // The keystroke under test: Up, then Enter. At base this became the
    // literal program text `^[[A` and an inescapable continuation.
    send(&mut lupin, "\u{1b}[A");
    send(&mut lupin, "\r");
    lupin
        .exp_string("2 : i32")
        .expect("Up recalled `1 + 1` and it re-evaluated — not a literal ^[[A");
    send(&mut lupin, ":quit\r");
    lupin.exp_eof().expect("`:quit` still leaves");
}

#[test]
fn alt_dot_yanks_the_last_arg_and_cycles_to_older_entries() {
    let mut lupin = spawn("alt-dot");
    send(&mut lupin, ":schedule 111\r");
    lupin.exp_string("re-seeded").expect("first entry");
    send(&mut lupin, ":schedule 222\r");
    lupin.exp_string("re-seeded").expect("second entry");
    // Alt-. inserts the newest entry's last argument (`222`); pressed again
    // it CYCLES to the older entry's (`111`) — GNU readline's yank-last-arg,
    // which rustyline does not ship (our ConditionalEventHandler provides it).
    send(&mut lupin, "\u{1b}.");
    send(&mut lupin, "\u{1b}.");
    send(&mut lupin, "\r");
    lupin
        .exp_string("111 : i32")
        .expect("the second Alt-. replaced 222 with the older 111");
    send(&mut lupin, ":quit\r");
    lupin.exp_eof().expect("clean exit");
}

#[test]
fn ctrl_c_mid_continuation_abandons_the_input_and_the_session_survives() {
    let mut lupin = spawn("ctrl-c");
    // World state that must survive the cancel.
    send_paced(&mut lupin, "let alive = 7\r");
    // Enter the continuation the base audit called inescapable (#46) …
    send_paced(&mut lupin, "fn f() {\r");
    // … and escape it. At base, Ctrl-C killed the whole process here.
    send_paced(&mut lupin, "\u{03}");
    // A fresh `wolf> ` accepts new input, and the world is intact.
    send_paced(&mut lupin, "alive\r");
    lupin
        .exp_string("7 : i32")
        .expect("the session and its bindings survived the cancelled edit");
    // No parse-error fallout: the abandoned text never reached the session.
    send(&mut lupin, ":quit\r");
    lupin
        .exp_eof()
        .expect("`:quit` at the fresh prompt leaves — Ctrl-C did not exit");
}

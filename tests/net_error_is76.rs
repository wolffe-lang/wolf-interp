//! is76 (wolf-interp#250) — lupin's half of `[os.net.error]` (wolf-lang
//! s239, #751; ruling #80): every net call sets `os_error()` as the fs family
//! does — cleared on entry, the host's number when the host refused the
//! call, 0 when it succeeded or its failure was decided before the host was
//! asked, and 0 for the two answers that are the runtime's (a fired budget,
//! a peer's orderly close).
//!
//! `tests/fixtures/net_error_is76/park_at_limit.lu` is wolf-lang's
//! `crates/wolf_driver/tests/fixtures/reactor/park_at_limit.lu` (trunk
//! `56929c63`) byte for byte: the program the compiler's `reactor_lanes` gate
//! runs under a lowered descriptor limit. `net_rows.lu` and `unix_rows.lu`
//! are lupin's own; every number below is the checked machine's
//! (`wolf conform-run --checked --json`, kasumi, linux). lupin 0.1.53 printed
//! 0 on every line.
//!
//! The limit is lowered for lupin alone, by `sh`'s `ulimit -n` before an
//! `exec` — never the test runner's, never the host's.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/net_error_is76")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// `name` copied alone into a fresh directory (a directory is a module).
fn alone(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "is76-net-{}",
        name.trim_end_matches(".lu")
    ));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    let entry = dir.join(name);
    std::fs::copy(fixture(name), &entry).expect("copied");
    entry
}

/// lupin's record for `entry`, its descriptor limit lowered to `limit`.
fn observe(entry: &Path, limit: u32) -> serde_json::Value {
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "ulimit -n {limit} && exec \"$0\" conform-run \"$1\" --json"
        ))
        .arg(env!("CARGO_BIN_EXE_lupin"))
        .arg(entry)
        .current_dir(entry.parent().expect("a directory"))
        .stdin(Stdio::null())
        .output()
        .expect("lupin observes");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "one record ({e}): {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn stdout(rec: &serde_json::Value) -> String {
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    rec["stdout_inline"].as_str().unwrap_or("").to_owned()
}

/// The descriptor-limit park: linux refuses the accept on an empty queue at
/// the limit at once, EMFILE (24) beside the `io` row — the gate's `accept`
/// line; macOS answers would-block, so the take waits out its budget
/// (`timeout`, 0). The other lines read as they always did.
#[test]
fn an_accept_refused_at_the_limit_leaves_emfile() {
    let rec = observe(&alone("park_at_limit.lu"), 32);
    let out = stdout(&rec);
    let accept = if cfg!(target_os = "linux") {
        "accept row=io errno=24"
    } else {
        "accept row=timeout errno=0"
    };
    assert!(out.contains(&format!("\n{accept}\n")), "{out}");
    assert!(out.contains("\npark row=timeout errno=0\n"), "{out}");
    assert!(out.contains("\nafter row=timeout errno=0\n"), "{out}");
    assert!(
        out.starts_with("full held=") && out.contains(" errno=24\n"),
        "{out}"
    );
}

/// Every row of the TCP family beside its number. On linux the numbers are
/// the checked machine's to the digit; elsewhere the host's own numbers
/// differ (ECONNREFUSED is 61 on macOS), so the rows the host refused are
/// held to non-zero and every other row to 0.
#[test]
fn each_net_row_carries_the_hosts_number_or_zero() {
    let rec = observe(&alone("net_rows.lu"), 1024);
    let out = stdout(&rec);
    let want = [
        ("listen ok", Some(0)),
        ("refused row=refused", Some(111)),
        ("port-after", Some(0)),
        ("double-close row=io", Some(0)),
        ("forged row=err", Some(0)),
        ("garbage row=io", Some(0)),
        ("in-use row=exists", Some(98)),
        ("unix-missing row=not_found", Some(2)),
        ("budget row=timeout", Some(0)),
        ("eof row=closed", Some(0)),
        ("write-after-close row=closed", Some(32)),
    ];
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), want.len(), "{out}");
    for ((head, number), line) in want.iter().zip(&lines) {
        let errno: i64 = line
            .strip_prefix(head)
            .and_then(|rest| rest.strip_prefix(" errno="))
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("`{head}` expected, got `{line}`\n{out}"));
        match number {
            Some(0) => assert_eq!(errno, 0, "{line}"),
            Some(n) if cfg!(target_os = "linux") => assert_eq!(errno, *n, "{line}"),
            _ => assert_ne!(errno, 0, "{line}: the host refused this call"),
        }
    }
}

/// The unix family: a listen on an existing path is the bind's own
/// EADDRINUSE (98 on linux; lupin 0.1.53 answered `exists` from a look at
/// the path and left 0), a dial of a plain file is ECONNREFUSED.
#[test]
fn the_unix_rows_carry_the_binds_and_the_dials_number() {
    let rec = observe(&alone("unix_rows.lu"), 1024);
    let out = stdout(&rec);
    let (in_use, refused) = if cfg!(target_os = "linux") {
        (98, 111)
    } else {
        (48, 61)
    };
    assert_eq!(
        out,
        format!(
            "listen-on-file row=exists errno={in_use}\ndial-file row=refused errno={refused}\n\
             live ok errno=0\nlisten-on-live row=exists errno={in_use}\n"
        ),
        "{rec}"
    );
}

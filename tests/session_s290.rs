//! is86 (wolf-lang#1089; wolf-lang s290, `[os.proc.session]`,
//! `[os.term.session]`, `[os.proc.session.note]`) — a child in a new
//! session, this process into one, and its session id, mirrored.
//!
//! `os_spawn_session(exe, args, map, defaults) -> int ! {denied, invalid, io,
//! not_found, unsupported}`, `os_setsid() -> () ! {denied, io, unsupported}`
//! and `os_sid() -> int ! {unsupported}`. lupin 0.1.60 resolved none of the
//! three. This machine answers what it can decide without a host call, in
//! the clauses' order: the spawn's SHAPE (`invalid`: a map `[os.proc.fds]`
//! calls invalid, a `defaults` below 0 or with a bit outside the twenty
//! listenable meanings) on every host; then, for a windows observation
//! target, the `unsupported` row for all three; and it refuses every other
//! call BY NAME — Rust's standard library has no stable new-session spawn,
//! no `setsid` on the calling process and no `getsid`, and this crate admits
//! no `unsafe`.
//!
//! `fixtures/session_s290/session_rows.lu` is wolf-lang's
//! `corpus/os/session_rows.lu` at `cc0274c2`, byte for byte (blob
//! `f669cf4c`, sha256 `b950c1ec…91c6`): on linux and macOS it prints its four
//! `invalid` lines and stops at the forged-source spawn, refused by name.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use wolf_interp::attrs::with_build_target;
use wolf_interp::eval::SchedRequest;
use wolf_interp::protocol::Verdict;

const WINDOWS: &str = "x86_64-pc-windows-msvc";
const UNIX_TARGETS: &[&str] = &["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"];

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("is86_{name}"));
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

/// One program, observed by the binary on this host.
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

/// A map of `pairs` pairs, targets 0.. in order, every source 0.
fn map_of(pairs: usize) -> String {
    let items: Vec<String> = (0..pairs).map(|t| format!("{t}, 0")).collect();
    format!("[{}]", items.join(", "))
}

/// The spawn's SHAPE rows: each map `[os.proc.fds]` calls invalid, and each
/// `defaults` outside the twenty listenable meanings.
fn shapes() -> Vec<(&'static str, String, &'static str)> {
    vec![
        ("map_odd", "[1]".to_owned(), "0"),
        ("map_target_high", "[256, 0]".to_owned(), "0"),
        ("map_target_negative", "[-1, 0]".to_owned(), "0"),
        ("map_target_twice", "[1, 1, 1, 2]".to_owned(), "0"),
        ("map_source_below", "[1, -2]".to_owned(), "0"),
        ("map_65_pairs", map_of(65), "0"),
        ("defaults_negative", "List[int]()".to_owned(), "-1"),
        ("defaults_outside", "List[int]()".to_owned(), "1048576"),
        (
            "defaults_outside_and_in",
            "List[int]()".to_owned(),
            "1048577",
        ),
        (
            "defaults_far_negative",
            "List[int]()".to_owned(),
            "-1048576",
        ),
    ]
}

/// Well-formed calls: every one past the shape.
fn well_formed() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "plain",
            "os_spawn_session",
            "\"true\", List[str](), List[int](), 0".to_owned(),
        ),
        (
            "forged_source",
            "os_spawn_session",
            "\"true\", List[str](), [1, 4096], 0".to_owned(),
        ),
        (
            "empty_program",
            "os_spawn_session",
            "\"\", List[str](), List[int](), 0".to_owned(),
        ),
        (
            "high_target_defaults",
            "os_spawn_session",
            "\"true\", [\"x\"], [3, 0], 1".to_owned(),
        ),
        (
            "widest_shape",
            "os_spawn_session",
            format!("\"true\", List[str](), {}, 1048575", map_of(64)),
        ),
        ("setsid", "os_setsid", String::new()),
        ("sid", "os_sid", String::new()),
    ]
}

fn handled(call: &str) -> String {
    format!(
        "fn main() -> int {{\n    {call} else |e| {{\n        print(\"{{e}}\")\n        return 0\n    }}\n    1\n}}\n"
    )
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

#[test]
fn the_spawn_s_shape_is_invalid_on_this_host() {
    for (row, map, defaults) in shapes() {
        let r = observe(
            &format!("shape_{row}"),
            &handled(&format!(
                "os_spawn_session(\"true\", List[str](), {map}, {defaults})"
            )),
        );
        assert_eq!(r["verdict"], "exit(0)", "{row}: {r}");
        assert_eq!(r["stdout_inline"], "invalid\n", "{row}: {r}");
    }
}

#[test]
fn the_spawn_s_shape_precedes_the_host_on_every_target() {
    for (row, map, defaults) in shapes() {
        let source = handled(&format!(
            "os_spawn_session(\"true\", List[str](), {map}, {defaults})"
        ));
        for target in std::iter::once(&WINDOWS).chain(UNIX_TARGETS) {
            let (verdict, stdout, why) = observe_as(target, &source);
            assert_eq!(verdict, Verdict::Exit(0), "{row} as {target}: {why:?}");
            assert_eq!(stdout, "invalid\n", "{row} as {target}");
        }
    }
}

#[test]
fn well_formed_calls_are_refused_by_name_on_this_host() {
    for (row, name, args) in well_formed() {
        let r = observe(
            &format!("refused_{row}"),
            &format!("fn main() -> !int {{\n    {name}({args})?\n    0\n}}\n"),
        );
        if cfg!(windows) {
            // The `unsupported` row, propagated out of `main`.
            assert_eq!(r["verdict"], "exit(1)", "{row}: {r}");
            continue;
        }
        assert_eq!(r["verdict"], "unsupported", "{row}: {r}");
        assert_eq!(r["x-unsupported-at"], "run", "{row}: {r}");
        let why = r["x-unsupported"].as_str().unwrap_or_default();
        assert_by_name(name, why, row);
    }
}

#[test]
fn windows_answers_the_unsupported_row_after_the_shape() {
    for (row, name, args) in well_formed() {
        let source = handled(&format!("{name}({args})"));
        let (verdict, stdout, why) = observe_as(WINDOWS, &source);
        assert_eq!(verdict, Verdict::Exit(0), "{row} as windows: {why:?}");
        assert_eq!(stdout, "unsupported\n", "{row} as windows");
    }
}

#[test]
fn every_unix_target_and_the_bundle_refuse_by_name() {
    for (row, name, args) in well_formed() {
        let source = handled(&format!("{name}({args})"));
        for target in UNIX_TARGETS
            .iter()
            .copied()
            .chain([wolf_interp::export::BUNDLE_TARGET])
        {
            let (verdict, stdout, why) = observe_as(target, &source);
            assert_eq!(verdict, Verdict::Unsupported, "{row} as {target}: {why:?}");
            assert_eq!(stdout, "", "{row} as {target}");
            assert_by_name(name, why.as_deref().unwrap_or_default(), row);
        }
    }
}

#[test]
fn the_declared_rows_discriminate_a_handler_s_arms() {
    // Each arm's bare name tests a tag of the call's declared row; with no
    // row it would bind, and the first arm would answer for every tag.
    let cases = [
        (
            "os_spawn_session(\"true\", List[str](), [1], 0)",
            "denied => 1, not_found => 2, io => 3, unsupported => 4, invalid => 5",
            WINDOWS,
            5,
        ),
        (
            "os_spawn_session(\"true\", List[str](), List[int](), 0)",
            "denied => 1, not_found => 2, io => 3, invalid => 5, unsupported => 4",
            WINDOWS,
            4,
        ),
        (
            "os_setsid()",
            "denied => 1, io => 2, unsupported => 3",
            WINDOWS,
            3,
        ),
        ("os_sid()", "unsupported => 6", WINDOWS, 6),
    ];
    for (call, arms, target, code) in cases {
        let source = format!(
            "fn main() -> int {{\n    {call} else |e| {{\n        return match e {{\n            {arms}\n        }}\n    }}\n    0\n}}\n"
        );
        let (verdict, _, why) = observe_as(target, &source);
        assert_eq!(verdict, Verdict::Exit(code), "{call} [{arms}]: {why:?}");
    }
}

fn fixture() -> PathBuf {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/session_s290/session_rows.lu");
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

const FOUR_INVALID: &str = "map_odd=invalid\nmap_target_twice=invalid\ndefaults_outside=invalid\ndefaults_negative=invalid\n";

#[test]
fn the_corpus_rows_stop_at_the_forged_source_spawn_by_name() {
    let entry = fixture();
    let dir = scratch("corpus_rows");
    let r = record(&entry, &dir);
    let run = lupin(&["run"], &entry, &dir);
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    if cfg!(windows) {
        assert_eq!(r["verdict"], "exit(0)", "{r}");
        assert_eq!(r["stdout_inline"], declared_stdout(&entry), "{r}");
        return;
    }
    assert_eq!(r["verdict"], "unsupported", "{r}");
    assert_eq!(r["x-unsupported-at"], "run", "{r}");
    assert_by_name(
        "os_spawn_session",
        r["x-unsupported"].as_str().unwrap_or_default(),
        "session_rows.lu",
    );
    assert_eq!(
        stdout, FOUR_INVALID,
        "the four shape rows, then the refusal: {stderr}"
    );
    assert!(stderr.contains("`os_spawn_session`"), "{stderr}");
    assert_eq!(run.status.code(), Some(4), "{stderr}");
}

#[test]
fn the_corpus_rows_print_the_same_four_lines_in_process() {
    let source = std::fs::read(fixture()).expect("fixture read");
    for target in UNIX_TARGETS
        .iter()
        .copied()
        .chain([wolf_interp::export::BUNDLE_TARGET])
    {
        let (verdict, stdout, why) = with_build_target(target, || {
            let (r, out) = wolf_interp::observe_record_stdin(&source, &SchedRequest::Default);
            let why = r
                .extensions
                .get("x-unsupported")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            (r.verdict, out.stdout, why)
        });
        assert_eq!(verdict, Verdict::Unsupported, "as {target}: {why:?}");
        assert_eq!(stdout, FOUR_INVALID, "as {target}");
        assert_by_name(
            "os_spawn_session",
            why.as_deref().unwrap_or_default(),
            target,
        );
    }
}

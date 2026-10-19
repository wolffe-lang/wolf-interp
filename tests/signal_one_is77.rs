//! is77 (wolf-interp#261; wolf-lang s246, #783, #784; ruling #88 R1 A,
//! R2 A; `[os.signal.disp.one]`, `[os.signal.exit]`) — one meaning's
//! disposition and the end by a signal, the mirror at its witnesses.
//!
//! `tests/fixtures/signal_one_is77/signal_one_rows.lu` is wolf-lang
//! `98d0d16b`'s `corpus/os/signal_one_rows.lu` byte for byte (sha256
//! `2d119642…`), so the answer here is the one the compiler's gate
//! (`signal_gaps_lanes.rs`, `the_one_meaning_shape_rows_on_every_host`)
//! holds a mirrored lupin to. The other rows were measured first on the
//! checked machine of wolf-lang `98d0d16b` (`wolf conform-run --checked
//! --json`) in a throwaway container on kasumi (is77, `runs/m1.txt`).
//!
//! **Nothing here sends a signal or changes a disposition.** This machine
//! answers each call's SHAPE rows — `invalid` for 0, for anything the
//! sending shape refuses and, for the two dispositions, for `KILL` and
//! `STOP` by bit or by host number — and, on windows, the `unsupported`
//! row; every well-formed call on linux and macOS is refused BY NAME
//! before any host call (lupin forbids `unsafe` and links no libc: std has
//! no `sigaction` and no `raise`), which is asserted on its words.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/signal_one_is77")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn observe(entry: &Path, dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("lupin observes")
}

fn record(out: &Output) -> serde_json::Value {
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .filter(|r| r.get("verdict").is_some())
        .unwrap_or_else(|| panic!("a record: {out:?}"))
}

fn declared_stdout(entry: &Path) -> String {
    let text = std::fs::read_to_string(entry).expect("the row reads");
    let line = text
        .lines()
        .find(|l| l.starts_with("//! check: run("))
        .unwrap_or_else(|| panic!("{} declares a run", entry.display()));
    let start = line.find("stdout=\"").expect("a declared stdout") + "stdout=\"".len();
    let end = line.rfind("\")").expect("the directive closes");
    line[start..end].replace("\\n", "\n") + "\n"
}

/// A program of this test's own, written to a scratch directory of its
/// own (a directory is a module) and observed there.
fn observe_text(slug: &str, program: &str) -> serde_json::Value {
    let dir = scratch(&format!("signal_one_is77_{slug}"));
    let entry = dir.join("main.lu");
    std::fs::write(&entry, program).expect("the program is written");
    record(&observe(&entry, &dir))
}

/// The two helpers every program below calls: a disposition's row and
/// the end's, each as a word.
const HELPERS: &str = "fn one(ignore: bool, m: int) -> str {\n\
    \x20   if ignore {\n\
    \x20       os_signal_ignore_one(m) else |e| {\n\
    \x20           return match e {\n\
    \x20               invalid => \"invalid\",\n\
    \x20               unsupported => \"unsupported\",\n\
    \x20               io => \"io\",\n\
    \x20           }\n\
    \x20       }\n\
    \x20       return \"ok\"\n\
    \x20   }\n\
    \x20   os_signal_default_one(m) else |e| {\n\
    \x20       return match e {\n\
    \x20           invalid => \"invalid\",\n\
    \x20           unsupported => \"unsupported\",\n\
    \x20           io => \"io\",\n\
    \x20       }\n\
    \x20   }\n\
    \x20   \"ok\"\n\
    }\n\
    \n\
    fn end(m: int) -> str {\n\
    \x20   os_exit_signal(m) else |e| {\n\
    \x20       return match e {\n\
    \x20           invalid => \"invalid\",\n\
    \x20           unsupported => \"unsupported\",\n\
    \x20       }\n\
    \x20   }\n\
    \x20   \"returned\"\n\
    }\n";

/// The compiler's corpus row: every shape row of the three calls, one
/// truth on every host (a far host number is `invalid` where the host
/// has numbers and `unsupported` on windows, which the row allows).
/// Red at 0.1.54: "`os_signal_ignore_one` does not resolve".
#[test]
fn the_one_meaning_shape_rows() {
    let entry = fixture("signal_one_rows.lu");
    let rec = record(&observe(&entry, &scratch("signal_one_is77_rows")));
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(rec["stdout_inline"], declared_stdout(&entry), "{rec}");
}

/// `KILL` and `STOP` by their host numbers are `invalid` for the two
/// dispositions, as by their bits (`[os.signal.disp.one]`: "by bit or by
/// number"), and a host number one past the host's last is `invalid` for
/// all three calls — measured on the checked machine (linux: 9, 19, 65).
/// Windows has no numbers: there a `-N` is past the shape, and the row is
/// the corpus file's `far_host_number`.
#[cfg(unix)]
#[test]
fn kill_and_stop_by_number_and_a_number_past_the_last_are_invalid() {
    let (kill, stop, past) = if cfg!(target_os = "macos") {
        (9, 17, 32)
    } else {
        (9, 19, 65)
    };
    let program = format!(
        "{HELPERS}\nfn main() -> int {{\n    \
         print(\"ignore_one_kill_number={{one(true, -{kill})}}\")\n    \
         print(\"ignore_one_stop_number={{one(true, -{stop})}}\")\n    \
         print(\"default_one_kill_number={{one(false, -{kill})}}\")\n    \
         print(\"default_one_stop_number={{one(false, -{stop})}}\")\n    \
         print(\"ignore_one_past_last={{one(true, -{past})}}\")\n    \
         print(\"default_one_past_last={{one(false, -{past})}}\")\n    \
         print(\"exit_past_last={{end(-{past})}}\")\n    \
         0\n}}\n"
    );
    let rec = observe_text("numbers", &program);
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(
        rec["stdout_inline"],
        "ignore_one_kill_number=invalid\nignore_one_stop_number=invalid\n\
         default_one_kill_number=invalid\ndefault_one_stop_number=invalid\n\
         ignore_one_past_last=invalid\ndefault_one_past_last=invalid\n\
         exit_past_last=invalid\n",
        "{rec}"
    );
}

/// Each well-formed call, past its shape: refused BY NAME on linux and
/// macOS, before any host call — nothing is ignored, defaulted or ended
/// — and the `unsupported` row on windows. The checked machine answers
/// `ok` for `USER` and for a host number, refuses the two fault meanings
/// by name (its own stack guard holds them) and refuses every well-formed
/// `os_exit_signal` by name (it does not end the process it runs in).
#[test]
fn each_well_formed_call_is_refused_by_name() {
    for (name, call) in [
        ("os_signal_ignore_one", "one(true, 512)"),
        ("os_signal_default_one", "one(false, 512)"),
        ("os_signal_ignore_one", "one(true, 4194304)"),
        ("os_signal_default_one", "one(false, 8388608)"),
        ("os_signal_ignore_one", "one(true, -16)"),
        ("os_exit_signal", "end(2)"),
        ("os_exit_signal", "end(1048576)"),
        ("os_exit_signal", "end(256)"),
        ("os_exit_signal", "end(-16)"),
    ] {
        let program =
            format!("{HELPERS}\nfn main() -> int {{\n    print(\"word={{{call}}}\")\n    0\n}}\n");
        let rec = observe_text(&format!("wf_{name}"), &program);
        if cfg!(windows) {
            assert_eq!(rec["verdict"], "exit(0)", "{call}: {rec}");
            assert_eq!(rec["stdout_inline"], "word=unsupported\n", "{call}: {rec}");
            continue;
        }
        let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
        assert_eq!(rec["verdict"], "unsupported", "{call}: {rec}");
        assert!(
            why.contains(&format!("`{name}`")) && why.contains("no `unsafe`"),
            "{call}: the refusal names `{name}` and says why: {why}"
        );
    }
}

//! is77 (wolf-interp#259; `[os.signal.listen]`, `[os.signal.wait]`,
//! `[os.signal.raise]`, `[os.signal.set]`'s one shape rule, s232) — the
//! three signal calls lupin 0.1.54 did not resolve now answer their SHAPE
//! rows and refuse every well-formed call BY NAME, as the rest of the
//! family does (ruling #79 R8), instead of "does not resolve".
//!
//! The rows were measured first on the checked machine of wolf-lang
//! `98d0d16b` and of wolf 0.2.32 (`wolf conform-run --checked --json`) in a
//! throwaway container on kasumi (is77, `runs/m1.txt`): a set the shape
//! rule refuses — empty, holding a bit that is not a listenable meaning,
//! negative — is `io` for a listen and a wait, and anything but one
//! listenable meaning is `io` for a raise. Past the shape the checked
//! machine listens (its in-machine queue, and the host armed), raises, and
//! refuses a wait with nothing pending by name.
//!
//! **Nothing here listens, raises or waits.** This machine never listens
//! (`[os.term.note]` R8: std has no `sigaction` or `raise`, and lupin
//! forbids `unsafe`), so a well-formed listen, raise or wait is refused
//! before any host call, on every host.

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

fn observe_text(slug: &str, program: &str) -> serde_json::Value {
    let dir = scratch(&format!("signal_listen_is77_{slug}"));
    let entry = dir.join("main.lu");
    std::fs::write(&entry, program).expect("the program is written");
    record(&observe(&entry, &dir))
}

const HELPERS: &str = "fn listen(s: int) -> str {\n\
    \x20   os_signal_listen(s) else |e| {\n\
    \x20       return match e {\n\
    \x20           io => \"io\",\n\
    \x20       }\n\
    \x20   }\n\
    \x20   \"ok\"\n\
    }\n\
    \n\
    fn wait(s: int) -> str {\n\
    \x20   let m = os_signal_wait(s) else |e| {\n\
    \x20       return match e {\n\
    \x20           io => \"io\",\n\
    \x20       }\n\
    \x20   }\n\
    \x20   \"got {m}\"\n\
    }\n\
    \n\
    fn raise(m: int) -> str {\n\
    \x20   os_signal_raise(m) else |e| {\n\
    \x20       return match e {\n\
    \x20           io => \"io\",\n\
    \x20       }\n\
    \x20   }\n\
    \x20   \"ok\"\n\
    }\n";

/// The shape rows, one truth on every host (the shape precedes the
/// platform). Red at 0.1.54: "`os_signal_listen` does not resolve".
#[test]
fn the_listen_wait_and_raise_shape_rows() {
    let program = format!(
        "{HELPERS}\nfn main() -> int {{\n    \
         print(\"listen_zero={{listen(0)}}\")\n    \
         print(\"listen_kill={{listen(1048576)}}\")\n    \
         print(\"listen_negative={{listen(-1)}}\")\n    \
         print(\"listen_past_table={{listen(268435456)}}\")\n    \
         print(\"wait_zero={{wait(0)}}\")\n    \
         print(\"wait_kill={{wait(1048576)}}\")\n    \
         print(\"wait_negative={{wait(-1)}}\")\n    \
         print(\"raise_zero={{raise(0)}}\")\n    \
         print(\"raise_two_bits={{raise(3)}}\")\n    \
         print(\"raise_kill={{raise(1048576)}}\")\n    \
         print(\"raise_segv={{raise(4194304)}}\")\n    \
         print(\"raise_host={{raise(-16)}}\")\n    \
         0\n}}\n"
    );
    let rec = observe_text("rows", &program);
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(
        rec["stdout_inline"],
        "listen_zero=io\nlisten_kill=io\nlisten_negative=io\nlisten_past_table=io\n\
         wait_zero=io\nwait_kill=io\nwait_negative=io\n\
         raise_zero=io\nraise_two_bits=io\nraise_kill=io\nraise_segv=io\nraise_host=io\n",
        "{rec}"
    );
}

/// Each well-formed call is refused BY NAME, on every host: the refusal
/// names the call and says why. Red at 0.1.54: "does not resolve".
#[test]
fn each_well_formed_call_is_refused_by_name() {
    for (name, call) in [
        ("os_signal_listen", "listen(1)"),
        ("os_signal_listen", "listen(1048575)"),
        ("os_signal_wait", "wait(1)"),
        ("os_signal_raise", "raise(512)"),
    ] {
        let program =
            format!("{HELPERS}\nfn main() -> int {{\n    print(\"word={{{call}}}\")\n    0\n}}\n");
        let rec = observe_text(&format!("wf_{name}"), &program);
        let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
        assert_eq!(rec["verdict"], "unsupported", "{call}: {rec}");
        assert!(
            why.contains(&format!("`{name}`"))
                && why.contains("no `unsafe`")
                && !why.contains("does not resolve"),
            "{call}: the refusal names `{name}` and says why: {why}"
        );
    }
}

/// The vendored corpus's loopback (`os/signal_loopback.lu`: listen, raise,
/// wait for RELOAD) stops at its first call, the listen, by name.
#[test]
fn the_corpus_loopback_stops_at_the_listen_by_name() {
    let entry =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/upstream/corpus/os/signal_loopback.lu");
    let rec = record(&observe(&entry, &scratch("signal_listen_is77_loopback")));
    let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
    assert_eq!(rec["verdict"], "unsupported", "{rec}");
    assert!(
        why.contains("`os_signal_listen`") && !why.contains("does not resolve"),
        "the loopback is refused at its listen, by name: {why}"
    );
}

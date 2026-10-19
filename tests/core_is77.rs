//! is77 (wolf-interp#263; wolf-lang s246, #785; ruling #88 R3 D;
//! `[os.proc.status]`, `[os.proc.poll]`) — the two siblings that carry a
//! core, the mirror at its witnesses.
//!
//! `os_wait_status_core(h)` is `os_wait_status(h)`, every answer and row
//! the same, except that a child whose end dumped core answers
//! `-(256 + N)` (std's `ExitStatusExt::core_dumped()`, the host's
//! `WCOREDUMP`); `os_wait_status` keeps `-N`, core or not. `os_wait_poll_core()`
//! is `os_wait_poll()`: `(-1, 0)` with no child left, decided without the
//! host, and refused BY NAME while a child is not yet reaped (ruling #84
//! R9: std's `try_wait` sees no stop, and lupin forbids `unsafe`).
//!
//! `tests/fixtures/core_is77/core.lu` is wolf-lang `98d0d16b`'s
//! `crates/wolf_driver/tests/fixtures/signal_gaps/core.lu` byte for byte
//! (sha256 `6483054b…`): a child that dumps core (`sh -c 'kill -s SEGV
//! $$'`) and one ended by `TERM`, which dumps none, each read by the
//! released wait and by its sibling. The checked machine of `98d0d16b`
//! printed `segv -11 / term -15 / segv core -267 / term core -15` in a
//! throwaway container on kasumi, whose `core_pattern` pipes to
//! `systemd-coredump` (a pipe dumps whatever `RLIMIT_CORE` says, core(5);
//! the box's core limit was 0, so the handler stored nothing) — is77,
//! `runs/m1.txt`.
//!
//! **Where the live rows run, and what they signal.** Only the fixture's
//! own children, which signal THEMSELVES, and only in a throwaway place:
//! on linux a `--network none` container of their own (docker or podman;
//! `LUPIN_CORE_ENGINE` names one, `LUPIN_CORE_IMAGE` the image, default
//! `docker.io/library/ubuntu:24.04`), the test's scratch directory its
//! working directory, the core limit set by the row; on macOS CI's
//! throwaway runner only (`GITHUB_ACTIONS`), cores off. A core is written
//! only where the host's pattern is a FILE, in that directory, and removed
//! there; a host whose pattern pipes to a program would write it outside,
//! so the cores-on row SKIPs there, by name. **`LUPIN_CORE_REQUIRE=1`
//! (CI's linux jobs, which set a file pattern on their runner) makes every
//! such skip a failure.**

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

fn record_of(bytes: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(bytes).into_owned();
    text.lines()
        .rev()
        .find_map(|l| {
            l.find('{')
                .and_then(|i| serde_json::from_str::<serde_json::Value>(l[i..].trim()).ok())
        })
        .filter(|r| r.get("verdict").is_some())
        .unwrap_or_else(|| panic!("a record: {text}"))
}

/// A program of this test's own, in a scratch directory of its own.
fn observe_text(slug: &str, program: &str) -> serde_json::Value {
    let dir = scratch(&format!("core_is77_{slug}"));
    let entry = dir.join("main.lu");
    std::fs::write(&entry, program).expect("the program is written");
    record_of(&observe(&entry, &dir).stdout)
}

fn required() -> bool {
    std::env::var("LUPIN_CORE_REQUIRE").is_ok_and(|v| v == "1")
}

/// With no child the two polls answer `(-1, 0)`, decided without the
/// host, on every machine and every host (windows included). Red at
/// 0.1.54: "`os_wait_poll_core` does not resolve".
#[test]
fn the_polls_with_no_child_answer_minus_one_zero() {
    let rec = observe_text(
        "poll_none",
        "fn main() -> !int {\n    let r = os_wait_poll_core()?\n    \
         print(\"poll_core=({r.0}, {r.1})\")\n    let q = os_wait_poll()?\n    \
         print(\"poll=({q.0}, {q.1})\")\n    0\n}\n",
    );
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(
        rec["stdout_inline"], "poll_core=(-1, 0)\npoll=(-1, 0)\n",
        "{rec}"
    );
}

/// The sibling wait's other answers are the released wait's: an exit
/// code, then `io` for the spent handle, a forged one and 0 — measured on
/// the checked machine (`exit7=7 again=io forged=io zero=io`). Nothing is
/// signalled. Red at 0.1.54: "`os_wait_status_core` does not resolve".
#[cfg(unix)]
#[test]
fn the_sibling_wait_answers_as_the_released_one() {
    let rec = observe_text(
        "status_rows",
        "fn word(core: bool, h: int) -> str {\n    if core {\n        \
         let s = os_wait_status_core(h) else |e| {\n            \
         return match e {\n                io => \"io\",\n            }\n        }\n        \
         return \"{s}\"\n    }\n    \
         let s = os_wait_status(h) else |e| {\n        \
         return match e {\n            io => \"io\",\n        }\n    }\n    \"{s}\"\n}\n\n\
         fn main() -> !int {\n    for core in [false, true] {\n        \
         let h = os_spawn([\"sh\", \"-c\", \"exit 7\"])?\n        \
         print(\"core={core} exit7={word(core, h)} again={word(core, h)} \
         forged={word(core, 99)} zero={word(core, 0)}\")\n    }\n    0\n}\n",
    );
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(
        rec["stdout_inline"],
        "core=false exit7=7 again=io forged=io zero=io\n\
         core=true exit7=7 again=io forged=io zero=io\n",
        "{rec}"
    );
}

/// A child not yet reaped (it exited 7; nothing is signalled): the sibling
/// poll is refused BY NAME, as the released poll is (ruling #84 R9). The
/// checked machine answers `(-1, 1)` for a live child, measured.
#[cfg(unix)]
#[test]
fn the_sibling_poll_with_a_child_not_yet_reaped_is_refused_by_name() {
    let rec = observe_text(
        "poll_live",
        "fn main() -> !int {\n    let c = os_spawn([\"sh\", \"-c\", \"exit 7\"])?\n    \
         time_sleep_ms(100)\n    let r = os_wait_poll_core()?\n    \
         print(\"({r.0}, {r.1}) of {c}\")\n    0\n}\n",
    );
    let why = rec["x-unsupported"].as_str().unwrap_or("").to_string();
    assert_eq!(rec["verdict"], "unsupported", "{rec}");
    assert!(
        why.contains("`os_wait_poll_core`") && why.contains("no `unsafe`"),
        "the refusal names `os_wait_poll_core` and says why: {why}"
    );
}

/// Where the live rows run.
#[cfg(unix)]
struct Live {
    /// `Some(engine)` in a container; `None` on CI's macOS runner.
    engine: Option<String>,
    dir: PathBuf,
}

#[cfg(unix)]
fn image() -> String {
    std::env::var("LUPIN_CORE_IMAGE").unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
}

#[cfg(unix)]
impl Live {
    fn new(test: &str) -> Option<Live> {
        let dir = scratch(test);
        if cfg!(target_os = "macos") {
            if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
                eprintln!(
                    "SKIP: {test}: not CI's throwaway runner — the live rows start children that \
                     signal themselves, and run on macOS only there"
                );
                return None;
            }
            return Some(Live { engine: None, dir });
        }
        let candidates: Vec<String> = match std::env::var("LUPIN_CORE_ENGINE") {
            Ok(e) => vec![e],
            Err(_) => vec!["docker".into(), "podman".into()],
        };
        let mut why = Vec::new();
        for e in &candidates {
            let out = Command::new(e)
                .args(["run", "--rm", "--network", "none"])
                .arg(image())
                .args(["sh", "-c", "echo $$"])
                .stdin(Stdio::null())
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    return Some(Live {
                        engine: Some(e.clone()),
                        dir,
                    });
                }
                Ok(o) => why.push(format!(
                    "{e}: {}",
                    String::from_utf8_lossy(&o.stderr)
                        .lines()
                        .next()
                        .unwrap_or("")
                )),
                Err(err) => why.push(format!("{e}: {err}")),
            }
        }
        assert!(
            !required(),
            "LUPIN_CORE_REQUIRE is set and no engine ran {} — {test} did not run: {why:?}",
            image()
        );
        eprintln!(
            "SKIP: {test}: no container engine ran {} ({why:?}) — the live rows did not run",
            image()
        );
        None
    }

    /// The host's `kernel.core_pattern` pipes cores to a program.
    fn cores_piped() -> bool {
        cfg!(target_os = "linux")
            && std::fs::read_to_string("/proc/sys/kernel/core_pattern")
                .is_ok_and(|p| p.trim_start().starts_with('|'))
    }

    /// `script` under `sh -c` in the test's directory: in a throwaway
    /// container (no network; with `zero`, a core limit of 0 as its hard
    /// limit too, else the engine's own; the test's directory at `/w` and
    /// its working directory, the fixture and lupin read-only), or on the
    /// runner.
    fn sh(&self, zero: bool, script: &str) -> Output {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_is77");
        let mut cmd = match &self.engine {
            Some(e) => {
                let mut c = Command::new(e);
                c.args(["run", "--rm", "--network", "none"]);
                if zero {
                    c.args(["--ulimit", "core=0:0"]);
                }
                c.arg("-v")
                    .arg(format!("{}:/w", self.dir.display()))
                    .arg("-v")
                    .arg(format!("{}:/fx:ro", fixture.display()))
                    .arg("-v")
                    .arg(format!(
                        "{}:/machines/lupin:ro",
                        env!("CARGO_BIN_EXE_lupin")
                    ))
                    .args(["-w", "/w"])
                    .arg(image())
                    .args(["sh", "-c", script]);
                c
            }
            None => {
                let script = script
                    .replace("/machines/lupin", env!("CARGO_BIN_EXE_lupin"))
                    .replace("/fx/", &format!("{}/", fixture.display()));
                let mut c = Command::new("sh");
                c.args(["-c", &script]).current_dir(&self.dir);
                c
            }
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.output().expect("the engine (or sh) runs")
    }
}

/// The core rides the sibling and never the released wait (ruling #88 R3
/// D): cores OFF (`ulimit -c 0`) — `segv core -11` where the host's
/// pattern is a file or on macOS, `-267` where it pipes to a program,
/// which dumps whatever the limit says — and cores ON (`ulimit -c
/// unlimited`, two core files found and removed in the test's directory)
/// `-267`, only where the pattern is a file. The released wait answers
/// `-11` and `TERM` `-15` in every row. Red at 0.1.54: "`os_wait_status_core`
/// does not resolve".
#[cfg(unix)]
#[test]
fn a_core_rides_the_sibling_status() {
    let test = "core_is77_live";
    let Some(live) = Live::new(test) else {
        return;
    };
    let piped = Live::cores_piped();
    let mut legs = vec![("off", true, piped)];
    if cfg!(target_os = "macos") {
        eprintln!("SKIP: core (on): macOS writes cores to /cores, outside the test's directory");
    } else if piped {
        assert!(
            !required(),
            "LUPIN_CORE_REQUIRE is set and this host pipes cores to a program — the cores-on \
             row did not run"
        );
        eprintln!(
            "SKIP: core (on): this host's core_pattern pipes cores to a program, which would \
             write the core outside the test's directory"
        );
    } else {
        legs.push(("on", false, true));
    }
    for (leg, zero, core) in legs {
        let segv = if core { -267 } else { -11 };
        let want = format!("segv -11\nterm -15\nsegv core {segv}\nterm core -15\n");
        let sub = format!("{leg}-wait");
        std::fs::create_dir_all(live.dir.join(&sub)).expect("the row's directory");
        std::fs::write(live.dir.join(&sub).join("s246.how"), "wait").expect("the setting");
        let at = match live.engine {
            Some(_) => format!("/w/{sub}"),
            None => live.dir.join(&sub).display().to_string(),
        };
        let ulimit = if zero { "0" } else { "unlimited" };
        let out = live.sh(
            zero,
            &format!(
                "ulimit -c {ulimit}; cd {at} && /machines/lupin conform-run /fx/core.lu --json; \
                 echo; ls | grep -c '^core'; rm -f core*"
            ),
        );
        let text = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        eprintln!(
            "== lupin, cores {leg}{}: {text}{}",
            if piped { " (piped pattern)" } else { "" },
            String::from_utf8_lossy(&out.stderr)
        );
        let rec = record_of(text.as_bytes());
        assert_eq!(
            (rec["verdict"].as_str(), rec["stdout_inline"].as_str()),
            (Some("exit(0)"), Some(want.as_str())),
            "#263: cores {leg}: {rec}"
        );
        let cores_here = if core && !piped { "2" } else { "0" };
        assert_eq!(
            text.trim_end().lines().last(),
            Some(cores_here),
            "#263: cores {leg}: the core files in the test's directory"
        );
    }
}

//! Human argv is a process boundary: preserve words, bytes and source spelling.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const SOURCE: &str = "//! member: false\nfn main() {\n print(env_arg0())\n for arg in env_args() { print(\"[{arg}]\") }\n}\n";
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Rig(PathBuf);
impl Rig {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/is84")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let rig = Self(root);
        rig.write("tool.lu", SOURCE);
        rig
    }
    fn write(&self, file: impl AsRef<Path>, source: &str) {
        std::fs::write(self.0.join(file), source).unwrap();
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lupin"));
        cmd.current_dir(&self.0);
        cmd
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn success(out: Output, expected: &str) {
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, expected.as_bytes());
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn words_after_file_reach_env_args_on_both_front_doors() {
    let rig = Rig::new();
    success(rig.run(&["tool.lu", "a", "b"]), "tool.lu\n[a]\n[b]\n");
    success(
        rig.run(&["run", "tool.lu", "a", "b"]),
        "tool.lu\n[a]\n[b]\n",
    );
}
#[test]
fn argv0_is_file_as_given_even_without_trailing_words() {
    let rig = Rig::new();
    success(rig.run(&["./tool.lu"]), "./tool.lu\n");
    success(rig.run(&["run", "./tool.lu"]), "./tool.lu\n");
}
#[test]
fn after_file_all_option_spellings_are_program_words() {
    let rig = Rig::new();
    for prefix in [&[][..], &["run"][..]] {
        let mut args = prefix.to_vec();
        args.extend([
            "tool.lu",
            "--help",
            "--json",
            "--seed=7",
            "--argv-hex=ff",
            "-e",
            "run",
            "--",
            "b",
        ]);
        success(
            rig.run(&args),
            "tool.lu\n[--help]\n[--json]\n[--seed=7]\n[--argv-hex=ff]\n[-e]\n[run]\n[--]\n[b]\n",
        );
    }
}
#[test]
fn double_dash_ends_options_and_allows_a_hyphen_file() {
    let rig = Rig::new();
    rig.write("-x.lu", SOURCE);
    success(
        rig.run(&["--", "-x.lu", "a", "--", "b"]),
        "-x.lu\n[a]\n[--]\n[b]\n",
    );
    success(rig.run(&["run", "--", "-x.lu", "a"]), "-x.lu\n[a]\n");
    success(rig.run(&["--", "tool.lu", "a"]), "tool.lu\n[a]\n");
}
#[test]
fn empty_and_unicode_arguments_survive() {
    let rig = Rig::new();
    success(
        rig.run(&["tool.lu", "", "é", "two words"]),
        "tool.lu\n[]\n[é]\n[two words]\n",
    );
}
#[test]
fn lupin_options_before_file_still_control_the_run() {
    let rig = Rig::new();
    let out = rig.run(&["run", "--json", "--seed=7", "tool.lu", "a"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let record: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(record["stdout_inline"], "tool.lu\n[a]\n");
    assert_eq!(record["seeded"], true);
}
#[test]
fn explicit_hex_replaces_whole_argv_and_mixing_is_refused_by_name() {
    let rig = Rig::new();
    success(
        rig.run(&["run", "--argv-hex=7368", "--argv-hex=61", "tool.lu"]),
        "sh\n[a]\n",
    );
    for command in ["run", "conform-run"] {
        let out = rig.run(&[command, "--argv-hex=7368", "tool.lu", "a"]);
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("--argv-hex"),
            "{out:?}"
        );
        assert!(out.stdout.is_empty());
    }
}
#[test]
fn conform_run_keeps_protocol_options_and_absent_argv() {
    let rig = Rig::new();
    let out = rig.run(&["conform-run", "tool.lu", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let record: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(record["stdout_inline"], "\n");
    assert_eq!(
        rig.run(&["conform-run", "tool.lu", "a"]).status.code(),
        Some(2)
    );
}
#[test]
fn stdin_source_accepts_trailing_arguments_without_using_stdin_for_argv() {
    use std::io::Write;
    let rig = Rig::new();
    let mut child = rig
        .command()
        .args(["-", "a", ""])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(SOURCE.as_bytes())
        .unwrap();
    success(child.wait_with_output().unwrap(), "-\n[a]\n[]\n");
}
#[test]
fn subcommand_collision_can_be_escaped_with_double_dash() {
    let rig = Rig::new();
    rig.write("repl", SOURCE);
    success(rig.run(&["--", "repl", "a"]), "repl\n[a]\n");
    success(rig.run(&["run", "repl", "a"]), "repl\n[a]\n");
}
#[cfg(unix)]
#[test]
fn non_utf8_argument_bytes_pass_verbatim_and_text_traps_at_os_index() {
    use std::os::unix::ffi::OsStringExt;
    let rig = Rig::new();
    rig.write("tool.lu", "fn main() { let args = env_args_bytes(); for arg in args { for b in arg { print(\"{b as int}\") }; print(\"end\") } }");
    let bad = OsString::from_vec(vec![b'x', 255, b'y']);
    success(
        rig.command()
            .arg("tool.lu")
            .arg(&bad)
            .arg("")
            .output()
            .unwrap(),
        "116\n111\n111\n108\n46\n108\n117\nend\n120\n255\n121\nend\nend\n",
    );
    rig.write("tool.lu", SOURCE);
    let out = rig.command().arg("tool.lu").arg(&bad).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("argument 1 is not UTF-8; read it with env_args_bytes()"),
        "{out:?}"
    );
}
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_file_name_is_preserved_in_argv0_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let rig = Rig::new();
    let file = OsString::from_vec(vec![b'x', 255, b'.', b'l', b'u']);
    rig.write(
        &file,
        "fn main() { let a = env_args_bytes(); for b in a[0] { print(\"{b as int}\") } }",
    );
    success(
        rig.command().arg(&file).output().unwrap(),
        "120\n255\n46\n108\n117\n",
    );
}
/// Writes `text` as the rig's executable `name`, which the test then runs,
/// without this process ever holding a write descriptor on it
/// (wolf-interp#360). Every test here spawns children from its own thread,
/// and a child forked while this process holds such a descriptor keeps a
/// copy until its own exec closes it; linux refuses to exec a file that any
/// process holds open for writing (`ETXTBSY`). So the text goes to a
/// side file, which is never run, and `cp` writes the script under a name
/// removed first (a new file, never opened here): `cp`'s descriptor lives
/// only in `cp`, which no sibling's child can inherit, and `cp` is reaped
/// before the script runs.
#[cfg(unix)]
fn write_script(rig: &Rig, name: &str, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    let text_file = format!("{name}.text");
    rig.write(&text_file, text);
    let script = rig.0.join(name);
    match std::fs::remove_file(&script) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("{}: {e}", script.display()),
    }
    let status = Command::new("cp")
        .arg(&text_file)
        .arg(name)
        .current_dir(&rig.0)
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "cp {text_file} {name}: {status}");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// wolf-interp#360: a script the test writes and runs at once, while other
/// threads of the process start children. A child forked while the
/// script's write descriptor is open in this process holds a copy of it
/// until its own exec, and linux refuses to exec a file a process holds
/// open for writing (`ETXTBSY`). Siblings spawn in a loop here so that a
/// fork lands inside a write often; every script written must run.
#[cfg(unix)]
#[test]
fn a_script_written_while_siblings_spawn_always_runs() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    const SIBLINGS: usize = 4;
    const TRIES: usize = 200;
    let rig = Rig::new();
    let stop = Arc::new(AtomicBool::new(false));
    let siblings: Vec<_> = (0..SIBLINGS)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut spawned = 0usize;
                while !stop.load(Ordering::Relaxed) {
                    Command::new("true")
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .unwrap();
                    spawned += 1;
                }
                spawned
            })
        })
        .collect();
    let mut busy = Vec::new();
    for at in 0..TRIES {
        let name = format!("script-{at}.sh");
        write_script(&rig, &name, "#!/bin/sh\nexit 0\n");
        match Command::new(format!("./{name}"))
            .current_dir(&rig.0)
            .stdin(Stdio::null())
            .status()
        {
            Ok(status) => assert!(status.success(), "{name}: {status}"),
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => busy.push(at),
            Err(e) => panic!("{name}: {e}"),
        }
    }
    stop.store(true, Ordering::Relaxed);
    let spawned: usize = siblings.into_iter().map(|t| t.join().unwrap()).sum();
    assert!(
        busy.is_empty(),
        "{} of {TRIES} scripts were refused with ETXTBSY (tries {busy:?}) while {SIBLINGS} siblings spawned {spawned} children",
        busy.len()
    );
}

#[cfg(unix)]
#[test]
fn executable_env_shebang_sees_script_name_and_args() {
    let rig = Rig::new();
    write_script(&rig, "tool.lu", &format!("#!/usr/bin/env lupin\n{SOURCE}"));
    let mut paths = vec![
        Path::new(env!("CARGO_BIN_EXE_lupin"))
            .parent()
            .unwrap()
            .to_path_buf(),
    ];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    success(
        Command::new("./tool.lu")
            .current_dir(&rig.0)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .args(["a", "b"])
            .output()
            .unwrap(),
        "./tool.lu\n[a]\n[b]\n",
    );
}

#[cfg(windows)]
#[test]
fn windows_non_unicode_argument_is_wtf8_and_text_traps() {
    use std::os::windows::ffi::OsStringExt;
    let rig = Rig::new();
    let bad = OsString::from_wide(&[120, 0xd800, 121]);
    rig.write(
        "tool.lu",
        "fn main() { let a = env_args_bytes(); for b in a[1] { print(\"{b as int}\") } }",
    );
    success(
        rig.command().arg("tool.lu").arg(&bad).output().unwrap(),
        "120\n237\n160\n128\n121\n",
    );
    rig.write("tool.lu", SOURCE);
    let out = rig.command().arg("tool.lu").arg(&bad).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("argument 1 is not UTF-8; read it with env_args_bytes()"),
        "{out:?}"
    );
}

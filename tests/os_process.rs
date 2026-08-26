//! The s40 process trio through the wolf surface, against a REAL child —
//! this binary's own `lupin`, which exists on every tier-1 host the matrix
//! runs (the corpus witness `os/spawn_rows.lu` deliberately spawns nothing;
//! these are the live halves, and the zombie discipline is asserted through
//! the same rows the language surface answers).

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

/// The lupin binary's own path, forward-slashed so it embeds into a wolf
/// string literal on every host (no escape sequences in the spelling).
fn lupin_path() -> String {
    env!("CARGO_BIN_EXE_lupin").replace('\\', "/")
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
fn a_real_child_spawns_waits_to_its_code_and_the_handle_is_spent() {
    // `lupin --version` is a quick, portable child. The first wait answers
    // its exit code and REAPS; the second wait and a late kill answer `io`
    // because the handle is spent — the #50 zombie lesson as language
    // surface.
    let dir = scratch("process-live");
    let source = format!(
        "fn main() -> !int {{\n\
         \x20   var argv = List[str]()\n\
         \x20   (mut argv).push(\"{}\")\n\
         \x20   (mut argv).push(\"--version\")\n\
         \x20   let h = os_spawn(argv)?\n\
         \x20   let code = os_wait(h)?\n\
         \x20   print(\"code {{code}}\")\n\
         \x20   os_wait(h) else |_| {{ print(\"spent\"); -1 }}\n\
         \x20   os_kill(h) else |_| print(\"kill-spent\")\n\
         \x20   0\n\
         }}\n",
        lupin_path()
    );
    let output = run_program(&dir, &source);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{output:?}");
    assert_eq!(stdout, "code 0\nspent\nkill-spent\n");
}

#[test]
fn kill_never_tombstones_and_the_wait_still_reaps() {
    // The child is a lupin running a 30s sleep; the kill lands while it is
    // alive, and the wait after it still owns the reap. On unix the killed
    // child has no exit code (the `signal` row); windows gives every
    // termination a code — either way the wait ANSWERS, and the handle is
    // spent after it.
    let dir = scratch("process-kill");
    // The sleeper lives in its own directory: a sibling `.lu` would join
    // the caller's module (D32) and collide on `main`.
    let child_dir = scratch("process-kill-child");
    std::fs::write(
        child_dir.join("sleepy.lu"),
        "fn main() -> !int {\n    time_sleep_ms(30000)\n    0\n}\n",
    )
    .expect("written");
    let sleepy = child_dir
        .join("sleepy.lu")
        .to_string_lossy()
        .replace('\\', "/");
    let source = format!(
        "fn main() -> !int {{\n\
         \x20   var argv = List[str]()\n\
         \x20   (mut argv).push(\"{}\")\n\
         \x20   (mut argv).push(\"run\")\n\
         \x20   (mut argv).push(\"{sleepy}\")\n\
         \x20   let h = os_spawn(argv)?\n\
         \x20   os_kill(h)?\n\
         \x20   let code = os_wait(h) else |_| {{ print(\"no-code\"); -9 }}\n\
         \x20   print(\"answered\")\n\
         \x20   os_wait(h) else |_| print(\"spent\")\n\
         \x20   0\n\
         }}\n",
        lupin_path()
    );
    let output = run_program(&dir, &source);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{output:?}");
    let unix_shape = stdout == "no-code\nanswered\nspent\n";
    let coded_shape = stdout == "answered\nspent\n";
    assert!(
        unix_shape || coded_shape,
        "wait-after-kill answered neither shape: {stdout:?}"
    );
}

#[test]
fn a_denied_or_io_spawn_answers_a_row_never_a_trap() {
    // A directory is not an executable program on any tier-1 host; the
    // spawn refusal is a ROW the program handles, whatever errno spelling
    // the host chose.
    let dir = scratch("process-denied");
    let target = dir.to_string_lossy().replace('\\', "/");
    let source = format!(
        "fn main() -> !int {{\n\
         \x20   var argv = List[str]()\n\
         \x20   (mut argv).push(\"{target}\")\n\
         \x20   os_spawn(argv) else |_| {{ print(\"row\"); -1 }}\n\
         \x20   0\n\
         }}\n",
    );
    let output = run_program(&dir, &source);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{output:?}");
    assert_eq!(stdout, "row\n");
}

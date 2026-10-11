//! s230 (wolf-lang's `[os.fs.mode]`, `[os.fs.create.mode]`,
//! `[os.fs.symlink]`, `[os.fs.link]`, `[os.fs.times]`, `[os.fs.truncate]`,
//! `[os.fs.umask]`; wolf-lang#346, #698, #699, #700, #702, #704) — the
//! mirror, at its witnesses; and wolf-interp#235.
//!
//! The programs under `tests/fixtures/fs_change_s230/` are wolf-lang s230's
//! six corpus rows and five unix fixtures, byte for byte (`umask_row.lu` is
//! its `corpus/os/umask.lu`), so the answers here are the ones the
//! compiler's gate (`fs_change_lanes.rs`) asserts of every machine. Each
//! runs under `lupin conform-run --json` in a scratch directory of its own,
//! and every file it changes is under that directory. No test asks who it
//! runs as.
//!
//! What this machine does NOT mirror is asserted on its words: `os_umask`
//! (the standard library has no `umask`) and `fs_set_times` on a link
//! itself (it has no `utimensat`) are refused BY NAME, never answered with
//! a row.
//!
//! **The umask.** No test here sets this process's mask. `modes.lu` is
//! started through `sh` with `umask 027`, so the mask is the child's from
//! its first instruction; nothing a test does to a mask reaches a test
//! running beside it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Give the owner every bit on a directory tree, so a tree a stopped run
/// left locked (`denied.lu`) can be removed. Links are not followed.
#[cfg(unix)]
fn unlock(p: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let Ok(md) = std::fs::symlink_metadata(p) else {
        return;
    };
    if !md.is_dir() {
        return;
    }
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700));
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            unlock(&e.path());
        }
    }
}

#[cfg(not(unix))]
fn unlock(_p: &Path) {}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        unlock(&dir);
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(dir.join("target")).expect("scratch created");
    dir
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fs_change_s230")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// `lupin conform-run <entry> --json` in `dir`, under `mask` when one is
/// given — through `sh`, which sets its own mask and then becomes lupin.
fn observe(entry: &Path, dir: &Path, mask: Option<&str>) -> Output {
    let lupin = env!("CARGO_BIN_EXE_lupin");
    let mut cmd = match mask {
        None => Command::new(lupin),
        Some(m) => {
            let mut c = Command::new("/bin/sh");
            c.arg("-c")
                .arg(format!("umask {m}; exec \"$@\""))
                .arg("sh")
                .arg(lupin);
            c
        }
    };
    cmd.arg("conform-run")
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

/// The stdout a corpus row's own header declares.
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

fn runs_in(name: &str, dir: &Path, mask: Option<&str>, want: &str) {
    let rec = record(&observe(&fixture(name), dir, mask));
    assert_eq!(rec["verdict"], "exit(0)", "{name}: {rec}");
    assert_eq!(rec["stdout_inline"], want, "{name}: {rec}");
}

/// A corpus row: the stdout its own header declares.
fn the_row_runs(name: &str) {
    let dir = scratch(&format!("s230-{}", name.replace('.', "_")));
    let want = declared_stdout(&fixture(name));
    runs_in(name, &dir, None, &want);
}

/// Refused by name: `unsupported`, the reason naming `call`.
fn refuses(name: &str, call: &str) -> serde_json::Value {
    let dir = scratch(&format!("s230-{}", name.replace('.', "_")));
    let rec = record(&observe(&fixture(name), &dir, None));
    assert_eq!(rec["verdict"], "unsupported", "{name}: {rec}");
    let reason = rec["x-unsupported"].as_str().unwrap_or("");
    assert!(
        reason.contains(call),
        "{name}: the refusal names `{call}`: {rec}"
    );
    rec
}

// --------------------------------------------- the corpus rows (every host)

#[test]
fn a_bad_shape_is_invalid_before_the_host() {
    the_row_runs("change_rows.lu");
}

#[test]
fn a_hard_link_is_a_second_name() {
    the_row_runs("hard_link.lu");
}

#[test]
fn truncate_sets_the_length_and_keeps_the_offset() {
    the_row_runs("truncate.lu");
}

#[test]
fn set_times_sets_each_time_or_leaves_it() {
    the_row_runs("set_times.lu");
}

#[test]
fn a_mode_and_a_symlink_or_unsupported() {
    the_row_runs("mode_symlink.lu");
}

/// wolf-interp#235: `exists` carries the host's number, and a success
/// clears it. Red at 0.1.50: `host_number=false` on the first two lines.
#[test]
fn exists_carries_the_hosts_number() {
    let dir = scratch("s230-os_error_rows");
    runs_in(
        "os_error_rows.lu",
        &dir,
        None,
        "mkdir again: exists host_number=true\n\
mkdir over a file: exists host_number=true\n\
a success clears it: ok host_number=0\n",
    );
}

/// `os_umask`: a well-formed call is refused by name on every host — this
/// machine has no host call for it — and the refusal comes at that call.
#[test]
fn the_mask_is_refused_by_name() {
    refuses("umask_row.lu", "os_umask");
}

// ------------------------------------------------------- the unix fixtures

/// Modes under the mask 0o027 (decimal: 416 is 0o640, 488 0o750, 384
/// 0o600, 448 0o700, 493 0o755, 2505 0o4711, 420 0o644, 1000 0o1750).
#[cfg(unix)]
#[test]
fn modes_under_a_known_mask() {
    use std::os::unix::fs::MetadataExt as _;
    let dir = scratch("s230-modes");
    runs_in(
        "modes.lu",
        &dir,
        Some("027"),
        "plain: file=416 dir=488\n\
open_perm: ok private=384 ok exe=488 existing ok kept=384\n\
open_perm rows: taken=exists errno=17 read_absent=not_found errno=2\n\
create_dir_mode: ok sub=448 ok wide=488 again=exists orphan=not_found\n\
set_mode: 384 493 2505 0 420 sticky_dir=1000\n\
through a link: target=384 link_unchanged=true\n\
set_mode rows: dangling=not_found errno=2 missing=not_found errno=2\n",
    );
    let mode = |n: &str| std::fs::metadata(dir.join("m").join(n)).unwrap().mode() & 0o7777;
    assert_eq!(
        [
            mode("plain"),
            mode("d"),
            mode("private"),
            mode("exe"),
            mode("sub"),
            mode("wide")
        ],
        [0o600, 0o1750, 0o600, 0o750, 0o700, 0o750],
        "the host's own metadata for what the program left"
    );
}

#[cfg(unix)]
#[test]
fn links_made_and_read_back() {
    use std::os::unix::fs::MetadataExt as _;
    let dir = scratch("s230-links");
    runs_in(
        "links.lu",
        &dir,
        None,
        "symlink: ok target=a kind=3 size=1 reads=12345\n\
dangling: ok target=../no/such/place kind=3 exists=false\n\
to a directory: ok is_dir=true\n\
symlink rows: over_link=exists errno=17 over_dangling=exists over_file=exists orphan=not_found errno=2\n\
link: ok same_file=true nlink=2,2\n\
link of a link: ok kind=3 target=a file_nlink=2\n\
link rows: taken=exists errno=17 missing=not_found errno=2 directory=denied errno=1\n",
    );
    let l = dir.join("l");
    assert_eq!(std::fs::read_link(l.join("ln")).unwrap(), Path::new("a"));
    assert_eq!(
        std::fs::read_link(l.join("dangling")).unwrap(),
        Path::new("../no/such/place")
    );
    assert_eq!(std::fs::read_link(l.join("hl")).unwrap(), Path::new("a"));
    let (a, b) = (
        std::fs::metadata(l.join("a")).unwrap(),
        std::fs::metadata(l.join("b")).unwrap(),
    );
    assert_eq!((a.dev(), a.ino(), a.nlink()), (b.dev(), b.ino(), 2));
}

/// The `denied` row of each path call inside a locked directory, where the
/// host says the lock binds this process; served where it does not.
#[cfg(unix)]
#[test]
fn denied_rows_inside_a_locked_directory() {
    let dir = scratch("s230-denied");
    runs_in(
        "denied.lu",
        &dir,
        None,
        "denied rows hold: true; nothing made where it binds: true\n",
    );
    assert!(!dir.join("x").exists(), "the program removed its tree");
}

/// A link's OWN times need `utimensat`: refused by name, and the refusal
/// comes at that call — the lines before it ran.
#[cfg(unix)]
#[test]
fn a_links_own_times_are_refused_by_name() {
    let rec = refuses("times_link.lu", "fs_set_times");
    let reason = rec["x-unsupported"].as_str().unwrap_or("");
    assert!(reason.contains("AT_SYMLINK_NOFOLLOW"), "{rec}");
}

/// The mask a program sets: refused by name at the first call.
#[cfg(unix)]
#[test]
fn the_mask_a_program_sets_is_refused_by_name() {
    refuses("umask.lu", "os_umask");
}

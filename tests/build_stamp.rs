//! wolf-interp#68 — the D57 suffix must not survive the tree it describes.
//!
//! r10 found `target/release/lupin` announcing the bare `lupin 0.1.27` — a
//! D57 *release* claim — from a tree that had moved off the tag: `build.rs`'s
//! cached output held `BUILD_SUFFIX=` from a build made while the branch sat
//! on `v0.1.27`, and nothing invalidated it. Against a fresh pairing stamp
//! that reads as "a release build declaring the wrong pin", and it reddens
//! gauntlets with nothing wrong in them.
//!
//! Which build produced r10's particular binary was not reconstructed. What
//! was found instead is a hole that is total and reproducible on demand: in a
//! git WORKTREE — how every lane in this org builds — the whole
//! `rerun-if-changed` set was **empty**. The probe was
//! `Path::new(".git/HEAD").exists()`, and a worktree's `.git` is a FILE
//! (`gitdir: …`), so the probe missed and the loop `continue`d without
//! declaring anything.
//!
//! So the test is the sequence #68 asks for, run against the real `build.rs`
//! rather than a description of it: build at a tag, move HEAD, rebuild, and
//! see `+dev` appear. It runs the sequence twice — once in a plain checkout,
//! once from a worktree, which is the shape that failed — and once more for
//! the vendored `PIN`, the other file whose change is a change of identity.
//!
//! The scratch crate is a five-line binary that prints what `build.rs`
//! stamped into it. It shares nothing with this package but the build script
//! itself, which is copied in: the point is to exercise THAT file, and a
//! reimplementation of it here would be a test of the test.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A directory the scratch repository is built in, fresh every run.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

fn git_at(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "is40")
        .env("GIT_AUTHOR_EMAIL", "is40@example.invalid")
        .env("GIT_COMMITTER_NAME", "is40")
        .env("GIT_COMMITTER_EMAIL", "is40@example.invalid")
        .output()
        .is_ok_and(|out| out.status.success())
}

fn have_git() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Lays down the scratch package and commits it, leaving HEAD on a branch
/// with `v9.9.9` pointing at it.
fn seed_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("src");
    std::fs::create_dir_all(root.join("vendor/upstream")).expect("vendor");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\n\
         name = \"stamp-probe\"\n\
         version = \"9.9.9\"\n\
         edition = \"2024\"\n\
         build = \"build.rs\"\n\
         \n\
         [workspace]\n",
    )
    .expect("manifest");
    std::fs::write(
        root.join("src/main.rs"),
        "fn main() {\n\
         \x20   println!(\"{}\", env!(\"WOLF_INTERP_BUILD_SUFFIX\"));\n\
         }\n",
    )
    .expect("main");
    std::fs::write(root.join("vendor/upstream/PIN"), "0000000\n").expect("pin");
    std::fs::write(root.join(".gitignore"), "/target\n/build-out\n").expect("gitignore");
    // THE build script, byte for byte. Copying it is the whole point.
    let ours = Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs");
    std::fs::copy(&ours, root.join("build.rs")).expect("build.rs copied");

    assert!(
        git_at(root, &["init", "--quiet", "-b", "trunk"]),
        "git init"
    );
    assert!(git_at(root, &["add", "."]), "git add");
    assert!(
        git_at(root, &["commit", "--quiet", "-m", "seed"]),
        "git commit"
    );
    assert!(git_at(root, &["tag", "v9.9.9"]), "git tag");
}

/// The two executable paths come from Cargo's documented JSON messages; neither
/// is inferred from the layout of its target directory.
#[derive(Debug)]
struct ProbeBuild {
    executable: PathBuf,
    script_ran: bool,
}

/// `build-script-executed` messages are emitted even for cached scripts. Only
/// the verbose `Running` command establishes that the script actually ran.
/// Its executable name comes from `compiler-artifact`, scoped to this manifest
/// and then its package ID, so another package cannot supply that evidence.
/// See Cargo's reference chapters "External tools" and "Build scripts".
fn probe_build_report(stdout: &[u8], stderr: &str, manifest: &Path) -> Result<ProbeBuild, String> {
    let messages: Vec<serde_json::Value> = stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(serde_json::from_slice)
        .collect::<Result<_, _>>()
        .map_err(|err| format!("invalid Cargo JSON: {err}"))?;
    let script = messages
        .iter()
        .find(|message| {
            message["reason"] == "compiler-artifact"
                && message["target"]["kind"][0] == "custom-build"
                && message["manifest_path"].as_str().is_some_and(|path| {
                    let path = Path::new(path);
                    path == manifest
                        || path
                            .canonicalize()
                            .is_ok_and(|resolved| resolved == manifest)
                })
        })
        .ok_or("Cargo did not report the probe's build-script artifact")?;
    let package = script["package_id"]
        .as_str()
        .ok_or("the probe's build-script artifact has no package ID")?;
    let script_executable = script["filenames"][0]
        .as_str()
        .ok_or("the probe's build-script artifact has no executable filename")?;
    if !messages.iter().any(|message| {
        message["reason"] == "build-script-executed" && message["package_id"] == package
    }) {
        return Err("Cargo did not report the probe's build-script output".to_owned());
    }
    let binary = messages
        .iter()
        .find(|message| {
            message["reason"] == "compiler-artifact"
                && message["package_id"] == package
                && message["target"]["kind"][0] == "bin"
                && message["target"]["name"] == "stamp-probe"
        })
        .ok_or("Cargo did not report the probe binary's artifact")?;
    let executable = binary["executable"]
        .as_str()
        .ok_or("the probe binary's artifact has no executable")?;
    let script_ran = stderr.lines().any(|line| {
        line.trim()
            .strip_prefix("Running `")
            .and_then(|command| command.strip_suffix('`'))
            .is_some_and(|command| {
                command == script_executable
                    || command
                        .strip_prefix('\'')
                        .and_then(|c| c.strip_suffix('\''))
                        == Some(script_executable)
                    || command.strip_prefix('"').and_then(|c| c.strip_suffix('"'))
                        == Some(script_executable)
            })
    });
    // Fail loudly on an unrecognized verbose format rather than silently
    // mistaking a script whose output did not change for a cached script.
    let fresh = format!(
        "Fresh stamp-probe v9.9.9 ({})",
        Path::new(script["manifest_path"].as_str().expect("selected manifest"))
            .parent()
            .ok_or("the probe manifest has no parent")?
            .display()
    );
    if !script_ran && !stderr.lines().any(|line| line.trim() == fresh) {
        return Err("Cargo reported neither a fresh probe nor its running build script".to_owned());
    }
    Ok(ProbeBuild {
        executable: PathBuf::from(executable),
        script_ran,
    })
}

struct ProbeStamp {
    suffix: String,
    script_ran: bool,
}

/// Builds the scratch package, observes whether its script ran and executes
/// the binary Cargo reported. The copied build.rs is unchanged.
///
/// The target directory is per-build-root and NOT this package's, because a
/// nested cargo sharing the outer build's directory would deadlock on its lock.
fn build_probe(manifest_dir: &Path) -> ProbeStamp {
    let manifest_dir = manifest_dir.canonicalize().expect("the probe's root");
    let target = manifest_dir.join("build-out");
    let cargo = Command::new(env!("CARGO"))
        .arg("build")
        .arg("--verbose")
        .arg("--message-format=json")
        .arg("--offline")
        .current_dir(&manifest_dir)
        .env("CARGO_TARGET_DIR", &target)
        .env("CARGO_TERM_COLOR", "never")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("CARGO_BUILD_TARGET")
        .output()
        .expect("cargo runs");
    let stderr = String::from_utf8(cargo.stderr).expect("Cargo stderr is utf8");
    assert!(cargo.status.success(), "the scratch crate builds: {stderr}");
    let report = probe_build_report(&cargo.stdout, &stderr, &manifest_dir.join("Cargo.toml"))
        .unwrap_or_else(|err| panic!("{err}\n{stderr}"));
    let out = Command::new(&report.executable)
        .output()
        .expect("the probe runs");
    assert!(out.status.success(), "the probe exits 0");
    ProbeStamp {
        suffix: String::from_utf8(out.stdout)
            .expect("utf8")
            .trim()
            .to_owned(),
        script_ran: false,
    }
}

fn stamp(manifest_dir: &Path) -> String {
    build_probe(manifest_dir).suffix
}

/// Model the documented stream, not any Cargo cache directory layout.
fn reported_probe() -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        json!({
            "reason": "compiler-artifact", "package_id": "probe-id",
            "manifest_path": "probe/Cargo.toml",
            "target": {"kind": ["custom-build"]},
            "filenames": ["script with spaces.exe"]
        }),
        json!({"reason": "build-script-executed", "package_id": "probe-id"}),
        json!({
            "reason": "compiler-artifact", "package_id": "probe-id",
            "target": {"kind": ["bin"], "name": "stamp-probe"},
            "executable": "reported-probe.exe"
        }),
    ]
}

fn report(messages: &[serde_json::Value], stderr: &str) -> Result<ProbeBuild, String> {
    let stream = messages
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    probe_build_report(stream.as_bytes(), stderr, Path::new("probe/Cargo.toml"))
}

#[test]
fn cached_script_output_is_not_a_script_execution() {
    let built = report(
        &reported_probe(),
        "       Fresh stamp-probe v9.9.9 (probe)\n",
    )
    .expect("a fresh probe with its cached build-script-executed message");
    assert!(!built.script_ran);
    assert_eq!(built.executable, Path::new("reported-probe.exe"));
}

#[test]
fn the_reported_script_command_is_rerun_evidence() {
    for command in [
        "script with spaces.exe",
        "'script with spaces.exe'",
        "\"script with spaces.exe\"",
    ] {
        let built = report(&reported_probe(), &format!("     Running `{command}`\n"))
            .expect("the reported script ran");
        assert!(built.script_ran);
    }
}

#[test]
fn another_packages_output_cannot_supply_probe_evidence() {
    use serde_json::json;
    let mut messages = reported_probe();
    let mut other = messages[0].clone();
    other["package_id"] = json!("other-id");
    other["manifest_path"] = json!("other/Cargo.toml");
    other["filenames"] = json!(["other-script"]);
    messages.insert(0, other);
    messages.push(json!({"reason": "build-script-executed", "package_id": "other-id"}));
    let built = report(
        &messages,
        "     Running `other-script`\n       Fresh stamp-probe v9.9.9 (probe)\n",
    )
    .expect("the other script ran, but the probe is fresh");
    assert!(!built.script_ran);

    messages.retain(|message| {
        message["reason"] != "build-script-executed" || message["package_id"] != "probe-id"
    });
    assert!(
        report(&messages, "     Running `script with spaces.exe`\n")
            .expect_err("the other package cannot supply the probe's script output")
            .contains("probe's build-script output")
    );
}

#[test]
fn missing_probe_messages_fail_loudly() {
    let messages = reported_probe();
    for missing in 0..messages.len() {
        let mut incomplete = messages.clone();
        incomplete.remove(missing);
        assert!(report(&incomplete, "     Running `script with spaces.exe`\n").is_err());
    }
    assert!(report(&[], "").is_err());
    assert!(probe_build_report(b"not JSON\n", "", Path::new("probe/Cargo.toml")).is_err());
}

#[test]
fn unrecognized_freshness_does_not_silently_mean_no_rerun() {
    assert!(report(&reported_probe(), "").is_err());
    assert!(
        report(
            &reported_probe(),
            "       Fresh stamp-probe v9.9.9 (other)\n"
        )
        .is_err()
    );
}

/// A beat between two builds.
///
/// `rerun-if-changed` is an mtime comparison, and a change written within the
/// same filesystem tick as the cached output can read as not-newer. That can
/// only make this test fail spuriously, never pass spuriously — but a second
/// is cheaper than a flake.
fn settle() {
    std::thread::sleep(std::time::Duration::from_millis(1100));
}

#[test]
fn a_branch_move_off_the_tag_makes_the_suffix_reappear() {
    if !have_git() {
        eprintln!("SKIP: git is not on PATH, so the stamp's invalidation cannot be exercised");
        return;
    }
    let root = scratch("build-stamp-plain");
    seed_repo(&root);

    // At the tag: the bare version, which is the only place D57 permits it.
    assert_eq!(
        stamp(&root),
        "",
        "a build made exactly at `v9.9.9` carries no suffix"
    );

    settle();
    // The move #68 describes: the same tree, one commit further on, no longer
    // the release. Nothing but `.git` changed — no source edit at all — which
    // is exactly the case the old probe could not see.
    assert!(
        git_at(&root, &["checkout", "--quiet", "-b", "moved"]),
        "branch"
    );
    assert!(
        git_at(&root, &["commit", "--quiet", "--allow-empty", "-m", "move"]),
        "empty commit"
    );

    let after = stamp(&root);
    assert!(
        after.starts_with("+dev."),
        "the tree moved off `v9.9.9` and the stamp still says `{after}` — the D57 suffix went \
         stale, which is wolf-interp#68"
    );
}

#[test]
fn the_same_holds_from_a_worktree_where_dot_git_is_a_file() {
    if !have_git() {
        eprintln!("SKIP: git is not on PATH, so the stamp's invalidation cannot be exercised");
        return;
    }
    let root = scratch("build-stamp-worktree");
    seed_repo(&root);
    let tree = root.join("wt");

    // A worktree checked out AT the tag: `.git` here is a file, and the old
    // probe declared nothing from it. The release claim itself is still
    // correct — this is a build made at `v9.9.9`.
    assert!(
        git_at(
            &root,
            &["worktree", "add", "--quiet", "--detach", "wt", "v9.9.9"]
        ),
        "worktree add"
    );
    assert!(
        tree.join(".git").is_file(),
        "the shape under test: a worktree's `.git` is a FILE, not a directory"
    );
    assert_eq!(
        stamp(&tree),
        "",
        "a worktree at the tag is still the release"
    );

    settle();
    // And the move, made inside the worktree.
    assert!(
        git_at(&tree, &["checkout", "--quiet", "-b", "wt-moved"]),
        "branch in the worktree"
    );
    assert!(
        git_at(
            &tree,
            &["commit", "--quiet", "--allow-empty", "-m", "move-in-wt"]
        ),
        "empty commit in the worktree"
    );

    let after = stamp(&tree);
    assert!(
        after.starts_with("+dev."),
        "a worktree moved off `v9.9.9` still stamps `{after}` — the probe is not finding the \
         worktree's git directory, which is the root cause of wolf-interp#68"
    );
}

#[test]
fn a_script_rerun_with_unchanged_output_is_observed() {
    if !have_git() {
        eprintln!("SKIP: git is not on PATH, so the stamp's invalidation cannot be exercised");
        return;
    }
    let root = scratch("build-stamp same-output");
    seed_repo(&root);
    assert_eq!(stamp(&root), "", "at the tag");

    settle();
    let unchanged = build_probe(&root);
    assert_eq!(unchanged.suffix, "", "unchanged tree, unchanged answer");
    assert!(!unchanged.script_ran, "the unchanged build is fresh");

    // An edit that changes none of the script's emitted values must still
    // be distinguishable from Cargo replaying those values from its cache.
    settle();
    let script = root.join("build.rs");
    let mut source = std::fs::read_to_string(&script).expect("the copied build script");
    source.push_str("\n// Recompile and rerun without changing the emitted values.\n");
    std::fs::write(script, source).expect("touch the copied build script");
    let rebuilt = build_probe(&root);
    assert_eq!(
        rebuilt.suffix, "",
        "the script still computes the same suffix"
    );
    assert!(
        rebuilt.script_ran,
        "the build script changed but its same-output rerun went unnoticed"
    );
}

#[test]
fn a_re_vendor_invalidates_the_stamp_too() {
    if !have_git() {
        eprintln!("SKIP: git is not on PATH, so the stamp's invalidation cannot be exercised");
        return;
    }
    let root = scratch("build-stamp-pin");
    seed_repo(&root);
    assert_eq!(stamp(&root), "", "at the tag");

    // The negative control, and it is load-bearing: a script that re-ran on
    // every build would make the assertion below vacuous, and "re-runs
    // always" is its own defect (a `rerun-if-changed` on a path that does not
    // exist does exactly that).
    settle();
    let unchanged = build_probe(&root);
    assert_eq!(unchanged.suffix, "", "unchanged tree, unchanged answer");
    assert!(
        !unchanged.script_ran,
        "nothing changed and the build script re-ran anyway — the invalidation set names a \
         path that is not there"
    );

    // A re-vendor with no commit: the pin the binary announces beside its
    // version has moved, so the stamp must be recomputed even though HEAD has
    // not. #68's binary announced a stale PIN beside a stale suffix.
    settle();
    std::fs::write(root.join("vendor/upstream/PIN"), "1111111\n").expect("re-vendored");
    let rebuilt = build_probe(&root);
    assert_eq!(
        rebuilt.suffix, "",
        "the tree is still at `v9.9.9`, so the ANSWER is still the bare version"
    );
    assert!(
        rebuilt.script_ran,
        "`vendor/upstream/PIN` moved and the build script did not re-run — a re-vendor is a \
         change of identity, and invalidation is not the same thing as a changed answer"
    );
}

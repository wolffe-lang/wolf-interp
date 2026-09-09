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

/// Builds the scratch package rooted at `manifest_dir` and returns the suffix
/// its binary prints.
///
/// The target directory is per-build-root and NOT this package's, because a
/// nested cargo sharing `target/` would deadlock on its lock.
fn stamp(manifest_dir: &Path) -> String {
    let target = manifest_dir.join("build-out");
    let status = Command::new(env!("CARGO"))
        .arg("build")
        .arg("--quiet")
        .arg("--offline")
        .current_dir(manifest_dir)
        .env("CARGO_TARGET_DIR", &target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("CARGO_BUILD_TARGET")
        .status()
        .expect("cargo runs");
    assert!(status.success(), "the scratch crate builds");

    let exe = target.join("debug").join(if cfg!(windows) {
        "stamp-probe.exe"
    } else {
        "stamp-probe"
    });
    let out = Command::new(&exe).output().expect("the probe runs");
    assert!(out.status.success(), "the probe exits 0");
    String::from_utf8(out.stdout)
        .expect("utf8")
        .trim()
        .to_owned()
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

/// When the build script last ran, read from the cache file cargo writes its
/// emitted lines into (`<target>/debug/build/stamp-probe-*/output`).
///
/// This is the only direct evidence that a script RE-RAN as opposed to
/// answering the same thing twice, and #68 is precisely a case where the two
/// look identical from outside.
fn script_ran_at(target: &Path) -> std::time::SystemTime {
    let builds = target.join("debug").join("build");
    let mut newest = None;
    for entry in std::fs::read_dir(&builds).expect("the build directory exists") {
        let dir = entry.expect("readable").path();
        let output = dir.join("output");
        if !output.is_file() {
            continue;
        }
        let at = output
            .metadata()
            .and_then(|meta| meta.modified())
            .expect("an mtime");
        newest = Some(newest.map_or(at, |seen: std::time::SystemTime| seen.max(at)));
    }
    newest.expect("the build script wrote an output file")
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
fn a_re_vendor_invalidates_the_stamp_too() {
    if !have_git() {
        eprintln!("SKIP: git is not on PATH, so the stamp's invalidation cannot be exercised");
        return;
    }
    let root = scratch("build-stamp-pin");
    let target = root.join("build-out");
    seed_repo(&root);
    assert_eq!(stamp(&root), "", "at the tag");
    let first = script_ran_at(&target);

    // The negative control, and it is load-bearing: a script that re-ran on
    // every build would make the assertion below vacuous, and "re-runs
    // always" is its own defect (a `rerun-if-changed` on a path that does not
    // exist does exactly that).
    settle();
    assert_eq!(stamp(&root), "", "unchanged tree, unchanged answer");
    assert_eq!(
        script_ran_at(&target),
        first,
        "nothing changed and the build script re-ran anyway — the invalidation set names a \
         path that is not there"
    );

    // A re-vendor with no commit: the pin the binary announces beside its
    // version has moved, so the stamp must be recomputed even though HEAD has
    // not. #68's binary announced a stale PIN beside a stale suffix.
    settle();
    std::fs::write(root.join("vendor/upstream/PIN"), "1111111\n").expect("re-vendored");
    assert_eq!(
        stamp(&root),
        "",
        "the tree is still at `v9.9.9`, so the ANSWER is still the bare version"
    );
    assert!(
        script_ran_at(&target) > first,
        "`vendor/upstream/PIN` moved and the build script did not re-run — a re-vendor is a \
         change of identity, and invalidation is not the same thing as a changed answer"
    );
}

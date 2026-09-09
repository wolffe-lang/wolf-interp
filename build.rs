//! Records the build's git commit so `conform-run` records can honestly
//! report which interpreter produced them (spec/06 `[proto.record.fields]`),
//! and decides D57's release/dev suffix.
//!
//! # What invalidates this stamp, and why the set is what it is
//!
//! A build script runs once and its output is cached; cargo re-runs it only
//! when a path named by `cargo:rerun-if-changed` is newer than that cache.
//! Everything this file computes is a claim about the *tree* — which commit,
//! which pin, release or dev — so every file that can change one of those
//! answers has to be in the set, or the binary keeps announcing an identity
//! it no longer has. That is the shape of wolf-interp#68:
//! `target/release/lupin` announcing the bare `lupin 0.1.27` (a D57 release
//! claim) from a tree that had moved off the tag, against a fresh pairing
//! stamp, which reads as a release build declaring the wrong pin and reddens
//! gauntlets with nothing wrong in them. Which build produced that particular
//! binary was not reconstructed; the hole below was, and it is total.
//!
//! ## The `.git` a worktree has is a FILE
//!
//! The set used to be probed as `Path::new(".git/HEAD").exists()`. In a git
//! WORKTREE — which is how every lane in this org builds — `.git` is not a
//! directory but a one-line file (`gitdir: …/.git/worktrees/<name>`), so that
//! probe finds nothing, `continue`s, and the script declares **no path at
//! all** beyond itself. Not a weak set: an empty one. The stamp then survives
//! a branch move, a tag landing, a re-vendor and a checkout alike, silently,
//! and the only thing that ever refreshed it was an unrelated edit to this
//! file. So the metadata directory is now ASKED FOR (`git rev-parse
//! --absolute-git-dir`) rather than guessed, and a worktree's split between
//! its own HEAD and the repository's shared refs is followed through
//! `commondir`.
//!
//! ## What cargo cannot be made to re-run on
//!
//! `rerun-if-changed` is an mtime comparison against a path, not a hook on a
//! git event, and D57's release decision asks a question no path answers
//! directly: *does a tag point at HEAD right now?* The paths below cover the
//! ways that answer changes in practice — HEAD moves, the branch's ref moves,
//! refs are packed or unpacked, a tag is created or deleted — because each
//! writes into `.git`. What they do not cover:
//!
//! - a change whose mtime is not NEWER than the cached output. A fresh clone,
//!   a `git checkout` that restores an older stamp, a tarball unpacked with
//!   preserved times, or a filesystem with second-granularity mtimes can all
//!   present a changed repository that cargo reads as unchanged.
//! - anything outside `.git` and this package: a tag that exists only on a
//!   remote, or a `git gc` on the shared directory of a repository this
//!   worktree does not name.
//!
//! The honest fallback is therefore not a cleverer path list. It is that
//! **the release build is made fresh, at the tag, in a clean tree** (the r01
//! ritual, and CI's release job runs on a runner with no `target/`), and that
//! `WOLF_INTERP_STAMP_NONCE` exists as a declared `rerun-if-env-changed`
//! escape hatch for a human who knows the stamp is stale and does not want to
//! reason about mtimes: set it to anything new and the script re-runs.
//! `touch build.rs` remains the same lever by another name.
//!
//! `tests/build_stamp.rs` builds a scratch crate against THIS file at a tag,
//! moves HEAD, rebuilds, and asserts `+dev` appears — including from a
//! worktree, which is the shape that failed.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `git <args>` in the package root, trimmed, or `None` when git is absent,
/// the directory is not a repository, or the answer is empty.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Declares `path` as an invalidator, if it is there.
///
/// A `rerun-if-changed` on a path that does not exist forces the script to
/// re-run on every single invocation, so absence is silence rather than a
/// declaration.
fn watch(path: &Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

/// Every file in the git metadata whose change can move this stamp's answer.
///
/// `git_dir` is this worktree's own metadata (its `HEAD` is per-worktree);
/// `common` is the repository's shared directory, where `refs/` and
/// `packed-refs` live for every worktree at once. In a plain checkout the two
/// are the same directory and the duplicate declarations collapse.
fn watch_git_metadata() {
    let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]).map(PathBuf::from) else {
        // No git, no claim: `unverifiable is dev` below, and nothing to watch.
        return;
    };
    // `commondir` holds a path to the shared directory, relative to this
    // worktree's own; a non-worktree repository has no such file.
    let common = match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(rel) => {
            let joined = git_dir.join(rel.trim());
            joined.canonicalize().unwrap_or(joined)
        }
        Err(_) => git_dir.clone(),
    };

    // HEAD itself: a branch switch, and a detached HEAD's every move.
    watch(&git_dir.join("HEAD"));

    // The ref HEAD names, so a COMMIT on the current branch invalidates —
    // `HEAD` on a branch holds `ref: refs/heads/<branch>` and does not change
    // when a commit lands. Both directories are tried: `refs/heads` is shared,
    // but a worktree can hold its own refs, and neither costs anything when it
    // is not there.
    if let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD"))
        && let Some(reference) = head.trim().strip_prefix("ref:")
    {
        let reference = reference.trim();
        watch(&git_dir.join(reference));
        watch(&common.join(reference));
    }

    // The packed form, for every ref that has no loose file — and the two
    // directories whose mtime moves when a loose ref is created or deleted,
    // which is how a tag landing or leaving is seen at all (D57 reads the tags
    // pointing at HEAD).
    watch(&common.join("packed-refs"));
    watch(&common.join("refs/heads"));
    watch(&common.join("refs/tags"));
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // The vendored pin. `lib.rs` compiles it in with `include_str!`, so the
    // CRATE already rebuilds when it moves — but this script's cache does not,
    // and #68's binary announced a stale pin beside a stale suffix. A
    // re-vendor is a change of identity; it belongs in this set.
    println!("cargo:rerun-if-changed=vendor/upstream/PIN");
    // The declared escape hatch (see the module docs): change it and the
    // script re-runs, whatever the mtimes say.
    println!("cargo:rerun-if-env-changed=WOLF_INTERP_STAMP_NONCE");
    watch_git_metadata();

    let commit = git(&["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=WOLF_INTERP_COMMIT={commit}");

    // D57 (r02): an off-tag build never claims to be the release. Only a
    // build made exactly at its own release tag — `v{version}` pointing at
    // HEAD — prints the bare crate version; every other build (trunk, a
    // branch, a tarball with no git at all) carries `+dev.<commit>` in
    // `--version`, so two different interpreters can never answer with the
    // same identity again (58 commits and eight re-vendors all reported
    // `lupin 0.1.13`, which is the rot this suffix exists to kill).
    // Unverifiable is dev: no git means no release claim.
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets the version");
    let at_release_tag = git(&["tag", "--points-at", "HEAD"])
        .is_some_and(|tags| tags.lines().any(|tag| tag.trim() == format!("v{version}")));
    let suffix = if at_release_tag {
        String::new()
    } else {
        format!("+dev.{commit}")
    };
    println!("cargo:rustc-env=WOLF_INTERP_BUILD_SUFFIX={suffix}");
}

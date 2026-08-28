//! Records the build's git commit so `conform-run` records can honestly
//! report which interpreter produced them (spec/06 `[proto.record.fields]`).

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for probe in [".git/HEAD", "../.git/HEAD"] {
        if !Path::new(probe).exists() {
            continue;
        }
        println!("cargo:rerun-if-changed={probe}");
        // `.git/HEAD` on a branch holds `ref: refs/heads/<branch>` and does
        // NOT change when a commit lands — only when the branch itself is
        // switched. Watching it alone left the stamp frozen at whatever
        // commit last happened to trigger a rebuild, so a record could claim
        // an interpreter revision that was not the one that produced it:
        // exactly the dishonesty `[proto.record.fields]` asks this file to
        // prevent, and an evidence-hygiene hazard for the differential (the
        // 0.1.9 round learned the counterparty half of the same lesson).
        // Watch the ref the symbolic HEAD points at, so a commit invalidates.
        let Ok(head) = std::fs::read_to_string(probe) else {
            continue;
        };
        if let Some(reference) = head.trim().strip_prefix("ref:") {
            let dir = Path::new(probe).parent().unwrap_or(Path::new("."));
            // The packed form too: a packed ref has no loose file. Neither
            // path is declared unless it exists — `rerun-if-changed` on a
            // missing path forces a rebuild on every invocation.
            // `refs/tags` too (D57): the release/dev decision below reads
            // the tags pointing at HEAD, so a tag landing or leaving must
            // invalidate the stamp exactly like a commit does.
            for path in [
                dir.join(reference.trim()),
                dir.join("packed-refs"),
                dir.join("refs/tags"),
            ] {
                if path.exists() {
                    println!("cargo:rerun-if-changed={}", path.display());
                }
            }
        }
    }

    let commit = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=WOLF_INTERP_COMMIT={commit}");

    // D57 (r02): an off-tag build never claims to be the release. Only a
    // build made exactly at its own release tag — `v{version}` pointing at
    // HEAD — prints the bare crate version; every other build (trunk, a
    // branch, a tarball with no git at all) carries `+dev.<commit>` in
    // `--version`, so two different interpreters can never answer with the
    // same identity again (58 commits and eight re-vendors all reported
    // `lupin 0.1.13`, which is the rot this suffix exists to kill).
    // Unverifiable is dev: no git means no release claim. A tag landing on
    // HEAD invalidates the stamp via the `refs/tags`/`packed-refs` probes
    // above; the release build is made fresh at the tag anyway (r01 ritual).
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets the version");
    let at_release_tag = Command::new("git")
        .args(["tag", "--points-at", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .is_some_and(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .any(|tag| tag.trim() == format!("v{version}"))
        });
    let suffix = if at_release_tag {
        String::new()
    } else {
        format!("+dev.{commit}")
    };
    println!("cargo:rustc-env=WOLF_INTERP_BUILD_SUFFIX={suffix}");
}

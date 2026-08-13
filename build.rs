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
            for path in [dir.join(reference.trim()), dir.join("packed-refs")] {
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
}

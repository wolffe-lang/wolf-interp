//! Records the build's git commit so `conform-run` records can honestly
//! report which interpreter produced them (spec/06 `[proto.record.fields]`).

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for probe in [".git/HEAD", "../.git/HEAD"] {
        if Path::new(probe).exists() {
            println!("cargo:rerun-if-changed={probe}");
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

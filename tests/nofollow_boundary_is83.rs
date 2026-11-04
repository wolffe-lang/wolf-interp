//! No-follow checks the in-tree final entry, not the link's outside target.
use std::path::Path;
use std::process::{Command, Stdio};

fn observe(name: &str, body: &str) -> serde_json::Value {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("is83_boundary_{name}"));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    std::fs::create_dir_all(dir.join("target")).unwrap();
    let entry = dir.join("main.lu");
    std::fs::write(&entry, body).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["conform-run", "--json"])
        .arg(entry)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .rev()
        .find_map(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| panic!("missing record: {out:?}"))
}

#[cfg(unix)]
#[test]
fn nofollow_refuses_the_final_link_without_resolving_its_outside_target() {
    let r = observe(
        "outside",
        r#"fn row(mode: int, perm: bool) -> str {
    if perm {
        let f = fs_open_perm("target/link", mode, 0o600) else |e| {
            let n = os_error()
            return "{e} {n}"
        }
        fs_close(f) else |_| {}
    } else {
        let f = fs_open_mode("target/link", mode) else |e| {
            let n = os_error()
            return "{e} {n}"
        }
        fs_close(f) else |_| {}
    }
    "opened"
}
fn main() -> !int {
    fs_symlink("/is83-not-created-external-target", "target/link")?
    print(row(16, false))
    print(row(16, true))
    print(row(48, false))
    print(row(48, true))
    0
}
"#,
    );
    let loop_code = if cfg!(target_os = "freebsd") {
        nix::errno::Errno::EMLINK as i32
    } else {
        nix::errno::Errno::ELOOP as i32
    };
    let not_dir = nix::errno::Errno::ENOTDIR as i32;
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(
        r["stdout_inline"],
        format!("invalid {loop_code}\ninvalid {loop_code}\ninvalid {not_dir}\ninvalid {not_dir}\n"),
        "{r}"
    );
}

#[cfg(unix)]
#[test]
fn nofollow_raw_directory_suffixes_still_resolve_the_target() {
    for (i, suffix) in ["/", "/.", "/.."].iter().enumerate() {
        for perm in [false, true] {
            let call = if perm {
                format!("fs_open_perm(\"target/link{suffix}\", 16, 0o600)")
            } else {
                format!("fs_open_mode(\"target/link{suffix}\", 16)")
            };
            let body = format!(
                "fn main() -> !int {{\n    fs_symlink(\"/is83-not-created-external-target\", \"target/link\")?\n    {call}?\n    0\n}}\n"
            );
            let r = observe(&format!("outside_suffix_{i}_{perm}"), &body);
            assert_eq!(r["verdict"], "unsupported", "{suffix}: {r}");
            assert!(
                r["x-unsupported"]
                    .as_str()
                    .unwrap()
                    .contains("resolves outside"),
                "{r}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn nofollow_keeps_raw_directory_suffixes_when_opening_inside_links() {
    let r = observe(
        "inside_suffix",
        r#"fn one(p: str, perm: bool) -> bool {
    let f = if perm {
        fs_open_perm(p, 16, 0o600) else |_| { return false }
    } else {
        fs_open_mode(p, 16) else |_| { return false }
    }
    let r = fs_fstat_record(f) else |_| { return false }
    fs_close(f) else |_| { return false }
    r[0] == 1
}
fn main() -> !int {
    fs_create_dir_all("target/d")?
    fs_symlink("d", "target/link")?
    for suffix in ["/", "/.", "/.."] {
        print(one("target/link" + suffix, false))
        print(one("target/link" + suffix, true))
    }
    0
}
"#,
    );
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(
        r["stdout_inline"], "true\ntrue\ntrue\ntrue\ntrue\ntrue\n",
        "{r}"
    );
}

#[cfg(windows)]
#[test]
fn windows_rejects_nofollow_before_path_resolution() {
    let r = observe(
        "windows",
        r#"fn main() {
    fs_open_mode("/is83-not-created-external-target", 16) else |e| {
        let n = os_error()
        print("{e} {n}")
    }
    fs_open_perm("/is83-not-created-external-target", 16, 0o600) else |e| {
        let n = os_error()
        print("{e} {n}")
    }
}
"#,
    );
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(r["stdout_inline"], "invalid 0\ninvalid 0\n", "{r}");
}

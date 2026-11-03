//! #315/#321: shape before named refusal; safe std-backed descriptor operations.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn observe(name: &str, source: &str) -> serde_json::Value {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("is83_{name}"));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    std::fs::create_dir_all(dir.join("target")).unwrap();
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).unwrap();
    run(&entry, &dir)
}

fn run(entry: &Path, dir: &Path) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["conform-run", "--json"])
        .arg(entry)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .unwrap_or_else(|| panic!("no record: {out:?}"))
}

fn invalid(name: &str, call: &str) {
    let source = format!(
        "fn main() -> int {{\n    {call} else |e| {{\n        print(\"{{e}}\")\n        return 0\n    }}\n    1\n}}\n"
    );
    let r = observe(name, &source);
    assert_eq!(r["verdict"], "exit(0)", "{call}: {r}");
    assert_eq!(r["stdout_inline"], "invalid\n", "{call}: {r}");
}

#[test]
fn descriptor_shapes_precede_refusal() {
    let cases = [
        "fs_open_at(100000, \"x\", 33, 0)",
        "fs_open_at(100000, \"x\", 64, 0)",
        "fs_open_at(100000, \"x\", 0, 0o1000)",
        "fs_open_at(100000, \"\", 0, 0)",
        "fs_open_at(100000, \"/x\", 0, 0)",
        "fs_stat_at(100000, \"\")",
        "fs_stat_at(100000, \"/x\")",
        "fs_lstat_at(100000, \"\")",
        "fs_lstat_at(100000, \"/x\")",
        "fs_remove_at(100000, \"\")",
        "fs_remove_at(100000, \"/x\")",
        "fs_remove_dir_at(100000, \"\")",
        "fs_remove_dir_at(100000, \"/x\")",
        "fs_create_dir_at(100000, \"x\", -1)",
        "fs_create_dir_at(100000, \"x\", 0o1000)",
        "fs_create_dir_at(100000, \"\", 0)",
        "fs_create_dir_at(100000, \"/x\", 0)",
        "fs_symlink_at(\"\", 100000, \"x\")",
        "fs_symlink_at(\"t\", 100000, \"\")",
        "fs_symlink_at(\"t\", 100000, \"/x\")",
        "fs_link_at(100000, \"\", 100000, \"x\", false)",
        "fs_link_at(100000, \"x\", 100000, \"/x\", true)",
        "fs_link_at(100000, \"/x\", 100000, \"x\", false)",
        "fs_link_at(100000, \"x\", 100000, \"\", false)",
        "fs_set_mode_fd(100000, -1)",
        "fs_set_mode_fd(100000, 0o10000)",
    ];
    for (i, call) in cases.iter().enumerate() {
        invalid(&format!("shape_{i}"), call);
    }
}

#[test]
fn well_formed_calls_are_refused_by_name() {
    let cases = [
        ("fs_open_at", "100000, \"../x\", 0, 0"),
        ("fs_stat_at", "100000, \"x\""),
        ("fs_lstat_at", "100000, \"x\""),
        ("fs_remove_at", "100000, \"x\""),
        ("fs_remove_dir_at", "100000, \"x\""),
        ("fs_create_dir_at", "100000, \"x\", 0o777"),
        ("fs_symlink_at", "\"/verbatim/target\", 100000, \"x\""),
        ("fs_link_at", "100000, \"x\", 100000, \"y\", true"),
        ("fs_read_dir_entries_at", "100000"),
        ("fs_statfs", "\".\""),
        ("os_chroot", "\".\""),
        ("os_getpriority", ""),
        ("os_setpriority", "0"),
        ("os_signal_action_one", "1"),
    ];
    for (name, args) in cases {
        let r = observe(
            name,
            &format!("fn main() -> !int {{\n    {name}({args})?\n    0\n}}\n"),
        );
        if cfg!(windows) && name != "fs_statfs" {
            assert_eq!(r["verdict"], "exit(1)", "{name}: {r}");
        } else {
            assert_eq!(r["verdict"], "unsupported", "{name}: {r}");
            let s = r.to_string();
            assert!(s.contains(name) && !s.contains("does not resolve"), "{r}");
        }
    }
}

#[test]
fn signal_action_sending_shape() {
    for (i, n) in [0, 3, 1 << 28].iter().enumerate() {
        invalid(
            &format!("signal_{i}"),
            &format!("os_signal_action_one({n})"),
        );
    }
    if cfg!(any(target_os = "linux", target_os = "macos")) {
        invalid("signal_far", "os_signal_action_one(-1000)");
    }
}

#[test]
fn directory_bit_write_modes_are_invalid_before_paths() {
    for mode in 1..=4 {
        invalid(
            &format!("directory_write_{mode}"),
            &format!("fs_open_mode(\"/outside-is83\", {})", mode + 32),
        );
        invalid(
            &format!("directory_perm_{mode}"),
            &format!("fs_open_perm(\"/outside-is83\", {}, 0)", mode + 32),
        );
    }
}

#[test]
fn nofollow_bit_refuses_a_final_link() {
    if !cfg!(unix) {
        return;
    }
    let r = observe(
        "nofollow",
        "fn main() -> !int {\n    fs_write_text(\"target/f\", \"kept\")?\n    fs_symlink(\"f\", \"target/link\")?\n    fs_open_mode(\"target/link\", 16) else |e| {\n        print(\"{e}\")\n        print(\"{os_error() > 0}\")\n        return 0\n    }\n    1\n}\n",
    );
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(r["stdout_inline"], "invalid\ntrue\n", "{r}");
}

#[test]
fn corpus_rows_at_the_mirror() {
    let cases = [
        ("open_directory", None),
        ("mode_fd", None),
        ("at_rows", Some("fs_remove_at")),
        ("mkdir_at", Some("fs_create_dir_at")),
        ("symlink_at", Some("fs_symlink_at")),
        ("link_at", Some("fs_link_at")),
        ("statfs_rows", Some("fs_statfs")),
        ("chroot_rows", Some("os_chroot")),
        ("priority_rows", Some("os_getpriority")),
        ("signal_action_rows", Some("os_signal_action_one")),
        ("term_lf_rows", None),
    ];
    for (name, refusal) in cases {
        let entry = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/os_mirror_is83")
            .join(format!("{name}.lu"));
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("is83_corpus_{name}"));
        std::fs::create_dir_all(dir.join("target")).unwrap();
        let r = run(&entry, &dir);
        println!("{name}: {r}");
        if let Some(call) = refusal {
            if cfg!(windows) && name != "statfs_rows" {
                assert_eq!(r["verdict"], "exit(0)", "{name}: {r}");
                let expected = match name {
                    "chroot_rows" => "missing: true\nmissing_number: true\ndot=unsupported\ndot_number=0\n".to_owned(),
                    "priority_rows" => "read=unsupported\nzero=unsupported\n".to_owned(),
                    "signal_action_rows" => "zero=invalid\ntwo_bits=invalid\npast_table=invalid\nfar_host_number: true\nterm=unsupported\n".to_owned(),
                    _ => declared_stdout(&entry),
                };
                assert_eq!(r["stdout_inline"], expected, "{name}: {r}");
                continue;
            }
            assert_eq!(r["verdict"], "unsupported", "{name}: {r}");
            assert!(
                r.to_string().contains(call) && !r.to_string().contains("does not resolve"),
                "{name}: {r}"
            );
        } else {
            assert_eq!(r["verdict"], "exit(0)", "{name}: {r}");
            let mut expected = declared_stdout(&entry);
            if cfg!(windows) && name == "term_lf_rows" {
                expected = expected.replace("=io", "=unsupported");
            }
            assert_eq!(r["stdout_inline"], expected, "{name}: {r}");
        }
    }
}

#[test]
fn statfs_missing_path_keeps_the_host_number() {
    let r = observe(
        "statfs_missing",
        "fn main() -> int {\n    fs_statfs(\"target/no-parent/x\") else |e| {\n        print(\"{e}\")\n        print(\"{os_error() > 0}\")\n        return 0\n    }\n    1\n}\n",
    );
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(r["stdout_inline"], "not_found\ntrue\n", "{r}");
}

#[test]
fn full_stat_record_uses_the_held_handle() {
    let r = observe(
        "full_record",
        r#"fn main() -> !int {
    fs_write_text("target/held", "old")?
    let fd = fs_open("target/held")?
    let before = fs_fstat_record(fd)?
    fs_write_text("target/new", "replacement")?
    fs_rename("target/new", "target/held")?
    let after = fs_fstat_record(fd)?
    print("{before.len} {after.len} {after[0]} {after[1]}")
    print("{before[9] == after[9] && before[10] == after[10]}")
    fs_close(fd)?
    fs_fstat_record(fd) else |e| { print("{e}") }
    fs_fstat_record(1000000) else |e| { print("{e}") }
    0
}
"#,
    );
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(r["stdout_inline"], "20 20 0 3\ntrue\nio\nio\n", "{r}");
}

#[test]
fn directory_read_modes_and_flags_compose() {
    if !cfg!(unix) {
        return;
    }
    let r = observe(
        "directory_read",
        r#"fn main() -> !int {
    fs_create_dir_all("target/d")?
    for m in [32, 37, 48, 53] {
        let h = fs_open_mode("target/d", m)?
        let r = fs_fstat_record(h)?
        print("{r[0] == 1}")
        fs_close(h)?
        let p = fs_open_perm("target/d", m, 0o777)?
        fs_close(p)?
    }
    0
}
"#,
    );
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(r["stdout_inline"], "true\ntrue\ntrue\ntrue\n", "{r}");
}

#[test]
fn held_mode_boundaries_and_closed_handles() {
    let r = observe(
        "held_modes",
        r#"fn row(h: int, m: int) -> str {
    fs_set_mode_fd(h, m) else |e| { return "{e}" }
    "ok"
}
fn main() -> !int {
    fs_write_text("target/f", "x")?
    let h = fs_open("target/f")?
    print("{row(h, 0)} {row(h, 0o7777)}")
    fs_close(h)?
    print(row(h, 0o600))
    print(row(h, 0o10000))
    0
}
"#,
    );
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    let expected = if cfg!(unix) {
        "ok ok\nio\ninvalid\n"
    } else {
        "unsupported unsupported\nunsupported\ninvalid\n"
    };
    assert_eq!(r["stdout_inline"], expected, "{r}");
}

fn declared_stdout(entry: &Path) -> String {
    let text = std::fs::read_to_string(entry).unwrap();
    let first = text.lines().next().unwrap();
    let encoded = first
        .split("stdout=")
        .nth(1)
        .unwrap()
        .strip_suffix(')')
        .unwrap();
    let mut result: String = serde_json::from_str(encoded).unwrap();
    result.push('\n');
    result
}

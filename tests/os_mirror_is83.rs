//! #315/#321: shape before named refusal; safe std-backed descriptor operations.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn observe(name: &str, source: &str) -> serde_json::Value {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("is83_{name}"));
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
        if cfg!(windows) && name.starts_with("os_") {
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
            if cfg!(windows) {
                continue;
            }
            assert_eq!(r["verdict"], "unsupported", "{name}: {r}");
            assert!(
                r.to_string().contains(call) && !r.to_string().contains("does not resolve"),
                "{name}: {r}"
            );
        } else {
            assert_eq!(r["verdict"], "exit(0)", "{name}: {r}");
            let text = std::fs::read_to_string(entry).unwrap();
            let first = text.lines().next().unwrap();
            let encoded = first
                .split("stdout=")
                .nth(1)
                .unwrap()
                .strip_suffix(')')
                .unwrap();
            let mut expected: String = serde_json::from_str(encoded).unwrap();
            if cfg!(windows) && name == "term_lf_rows" {
                expected = expected.replace("=io", "=unsupported");
            }
            expected.push('\n');
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

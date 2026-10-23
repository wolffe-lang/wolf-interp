//! Mirrors of wolf 0.2.35 and s260. Fixtures are specification data, not
//! compiler implementation. The fs/exec rows run in isolated subprocesses.
use std::process::Command;
use wolf_interp::frontend::observe;
use wolf_interp::protocol::Verdict;

macro_rules! runs {
    ($name:ident, $want:expr) => {
        #[test]
        fn $name() {
            let obs = observe(
                include_bytes!(concat!("fixtures/is81/", stringify!($name), ".lu")),
                None,
            );
            assert_eq!(
                obs.verdict,
                Verdict::Exit(0),
                "{:?} {:?}",
                obs.trap,
                obs.reason
            );
            assert_eq!(String::from_utf8_lossy(&obs.stdout), $want);
        }
    };
}
runs!(region_copy_shares_str, "28\nbuilt in pass, 0 uses\n");
runs!(region_outer_write_field, "t9 1 9 x1 2 7\n");
runs!(region_outer_write_map, "40 v7 v39 true\n");
runs!(region_outer_write_str_push, "100 turn 0 turn 99 42 true\n");
runs!(region_outer_write_struct, "50 r0 r49 3 49 98 147\n");
runs!(region_str_call_index_store, "[piece 7 3-7]\n");

runs!(list_reserve, "0 1000 0 999 3 7 8 9 true\n");
runs!(list_reset, "5 0 3 10 11 12 2 b2 true\n");
runs!(region_outer_write_reset, "300 3 299 0 6\n");

#[test]
fn row_members_and_methods_require_handling() {
    for source in [
        include_str!("fixtures/is81/row_operand_member.lu"),
        include_str!("fixtures/is81/row_operand_method.lu"),
    ] {
        let obs = observe(source.as_bytes(), None);
        assert_eq!(
            obs.verdict,
            Verdict::Fail("E0409".into()),
            "{:?}",
            obs.reason
        );
        let diagnostic = &obs.diagnostics[0];
        let start = source.rfind("rest.").unwrap();
        assert_eq!(diagnostic.span, [start as u64, (start + 4) as u64]);
    }
}

#[test]
fn format_mark_beats_the_module_function_in_a_tuple_result() {
    let source = r#"
fn format() -> int { 77 }
fn fields() -> (int, int) ! {format} { format }
fn main() -> !int {
    let value = fields() else |_| (3, 4)
    let (a, b) = value
    print("{a} {b} {format()}")
    0
}
"#;
    let obs = observe(source.as_bytes(), None);
    assert_eq!(obs.verdict, Verdict::Exit(0), "{:?}", obs.reason);
    assert_eq!(obs.stdout, b"3 4 77\n");
}

#[test]
fn failed_copy_writes_rewind_and_preserve_errno() {
    let root = std::env::temp_dir().join(format!("is81-copy-{}", std::process::id()));
    std::fs::create_dir_all(root.join("target")).unwrap();
    {
        let name = "copy_chunk_failed_write";
        let want = "copy=io took_nothing=true os_error_set=true offset_after=0\nretry=17 same=true\ncleaned=true\n";
        let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
            .args([
                "run",
                &format!(
                    "{}/tests/fixtures/is81/{name}.lu",
                    env!("CARGO_MANIFEST_DIR")
                ),
            ])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), want, "{name}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn dev_full_corpus_witness_names_the_existing_path_domain_limit() {
    let root = std::env::temp_dir().join(format!("is81-full-{}", std::process::id()));
    std::fs::create_dir_all(root.join("target")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args([
            "run",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/is81/copy_chunk_dev_full.lu"
            ),
        ])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("/dev/full") && error.contains("[os.fs.path.domain]"),
        "{error}"
    );
    eprintln!(
        "LIMIT: fs/copy_chunk_dev_full.lu is declined by [os.fs.path.domain]; Rust-level counted-write and in-tree failed-write rows cover the copy rule"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn exec_no_shebang_is_io_with_enoexec_and_never_runs_the_file() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("is81-exec-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let executable = root.join("noshebang");
    std::fs::write(&executable, "echo unexpectedly-ran\nexit 71\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let program = root.join("main.lu");
    std::fs::write(&program, format!(r#"
fn main() -> !int {{
    let answer = os_exec("{}", ["custom-zero"], ["PATH=/bin:/usr/bin"], List[int]()) else |e| match e {{
        unsupported => 1, invalid => 2, not_found => 3, denied => 4, io => 5,
    }}
    print("{{answer}} {{os_error()}}")
    0
}}
"#, executable.display())).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["run", program.to_str().unwrap()])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"5 8\n");
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn s260_spawn_and_exec_witnesses_keep_the_host_number() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("is81-spawn-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, bytes) in [
        ("noshebang", b"echo ran > ran-marker\n".as_slice()),
        ("junk", b"\0\0\0\0"),
        ("control", b"#!/bin/sh\nexit 7\n"),
    ] {
        let path = root.join(name);
        std::fs::write(&path, bytes).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    for name in ["spawn_rows", "exec_row"] {
        let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
            .args([
                "run",
                &format!(
                    "{}/tests/fixtures/is81/{name}.lu",
                    env!("CARGO_MANIFEST_DIR")
                ),
            ])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success(), "{name}: {output:?}");
        let got = String::from_utf8_lossy(&output.stdout);
        if name == "exec_row" {
            assert_eq!(got, "exec: io os_error=8 marker=false\n");
        } else {
            let mut want = String::new();
            for exe in ["./noshebang", "./junk", "./control"] {
                for call in ["spawn_fds[1,1]", "spawn_fds[]", "spawn", "spawn_job[1,1]"] {
                    let answer = if exe == "./control" {
                        "RAN status 7"
                    } else {
                        "io os_error=8"
                    };
                    want.push_str(&format!("{exe} {call}: {answer}\n"));
                }
            }
            want.push_str("marker=false\n");
            assert_eq!(got, want);
        }
        assert!(!root.join("ran-marker").exists());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn copied_keys_and_values_charge_the_maps_birth_region() {
    let source = r#"
fn main() -> !int {
    let keep = region()
    in keep {
        var m = Map[str, str]()
        let before = region_bytes(keep)
        region turn {
            let key = "key{3}"
            let value = "value{4}"
            m[copy key] = copy value
        }
        let after = region_bytes(keep)
        // Each string has its own charge; a fresh map slot is tier units.
        assert(after - before >= 128)
        print("{m["key3"] else "?"}")
    }
    0
}
"#;
    let obs = observe(source.as_bytes(), None);
    assert_eq!(
        obs.verdict,
        Verdict::Exit(0),
        "{:?} {:?}",
        obs.trap,
        obs.reason
    );
    assert_eq!(obs.stdout, b"value4\n");
}

#[test]
fn ordinary_copies_still_share_strings_and_outer_copies_charge_the_destination_cap() {
    for source in [
        r#"
struct Box { title: str }
fn main() -> !int {
    let kept = copy region turn {
        let s = "built{7}"
        var v = Box { title: "" }
        v.title = copy s
        v
    }
    print("{kept.title}")
    0
}
"#,
        r#"
fn main() -> !int {
    region keep(cap: 0) {
        var text = ""
        region turn { text = copy "built{7}" }
        print("{text}")
    }
    0
}
"#,
    ] {
        let obs = observe(source.as_bytes(), None);
        // First program is the plain-copy control: copy-out makes the
        // leaving value independent. Second must breach KEEP's zero cap.
        if source.contains("cap:") {
            assert_eq!(
                obs.verdict,
                Verdict::Trap(wolf_interp::trap::TrapKind::AllocContract),
                "{:?}",
                obs.reason
            );
        } else {
            assert_eq!(obs.verdict, Verdict::Exit(0), "{:?}", obs.reason);
            assert_eq!(obs.stdout, b"built7\n");
        }
    }
}

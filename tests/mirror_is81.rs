//! Mirrors of wolf 0.2.35 and s260. Fixtures are specification data, not
//! compiler implementation. The fs/exec rows run in isolated subprocesses.
use std::process::Command;
use wolf_interp::frontend::observe;
use wolf_interp::protocol::Verdict;

macro_rules! runs {
    ($name:ident, $want:expr) => {
        #[test]
        fn $name() {
            let obs = observe(include_bytes!(concat!("fixtures/is81/", stringify!($name), ".lu")), None);
            assert_eq!(obs.verdict, Verdict::Exit(0), "{:?} {:?}", obs.trap, obs.reason);
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

#[test]
fn row_members_and_methods_require_handling() {
    for source in [include_str!("fixtures/is81/row_operand_member.lu"), include_str!("fixtures/is81/row_operand_method.lu")] {
        let obs = observe(source.as_bytes(), None);
        assert_eq!(obs.verdict, Verdict::Fail("E0409".into()), "{:?}", obs.reason);
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
    for (name, want) in [
        ("copy_chunk_failed_write", "copy=io took_nothing=true os_error_set=true offset_after=0\nretry=17 same=true\ncleaned=true\n"),
        ("copy_chunk_dev_full", "copy=io os_error_set=true offset_after=0 left_to_read=17 of 17\ncleaned=true\n"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_lupin"))
            .args(["run", &format!("{}/tests/fixtures/is81/{name}.lu", env!("CARGO_MANIFEST_DIR"))])
            .current_dir(&root).output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), want, "{name}");
    }
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
        .args(["run", program.to_str().unwrap()]).current_dir(&root).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"5 8\n");
    std::fs::remove_dir_all(root).unwrap();
}

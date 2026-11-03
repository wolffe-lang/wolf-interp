//! The reference target, not the exporting host, chooses pure host refusals.
use wolf_interp::attrs::with_build_target;
use wolf_interp::eval::SchedRequest;
use wolf_interp::protocol::Verdict;

#[test]
fn declined_os_calls_use_the_reference_target() {
    let calls = [
        ("os_chroot", "\".\""),
        ("os_getpriority", ""),
        ("os_setpriority", "0"),
        ("os_signal_action_one", "1"),
        ("os_signal_ignored", ""),
        ("fs_open_at", "100000, \"x\", 0, 0"),
        ("fs_open_at", "100000, \"x\", 32, 0"),
        ("fs_open_at", "100000, \"x\", 48, 0"),
        ("fs_stat_at", "100000, \"x\""),
        ("fs_lstat_at", "100000, \"x\""),
        ("fs_remove_at", "100000, \"x\""),
        ("fs_remove_dir_at", "100000, \"x\""),
        ("fs_create_dir_at", "100000, \"x\", 0"),
        ("fs_symlink_at", "\"t\", 100000, \"x\""),
        ("fs_link_at", "100000, \"x\", 100000, \"y\", false"),
        ("fs_read_dir_entries_at", "100000"),
    ];
    for (name, args) in calls {
        let source =
            format!("fn main() -> int {{\n    {name}({args}) else |_| {{ return 0 }}\n    1\n}}\n");
        for (target, expected) in [
            ("x86_64-pc-windows-msvc", Verdict::Exit(0)),
            ("x86_64-unknown-linux-gnu", Verdict::Unsupported),
        ] {
            with_build_target(target, || {
                let r =
                    wolf_interp::observe_record_stdin(source.as_bytes(), &SchedRequest::Default).0;
                assert_eq!(r.verdict, expected, "{name} for {target}: {r:?}");
                let bundled = with_build_target(wolf_interp::export::BUNDLE_TARGET, || {
                    wolf_interp::observe_record_stdin(source.as_bytes(), &SchedRequest::Default).0
                });
                assert_eq!(bundled.verdict, Verdict::Unsupported, "{name}: {bundled:?}");
            });
        }
    }
}

#[test]
fn raw_lf_nonterminal_rows_use_the_reference_target() {
    for call in [
        "os_term_set_mode(1000000, 16777216)",
        "os_term_restore_at_exit(1000000, 16777216, List[byte]())",
    ] {
        let source = format!("fn main() {{\n    {call} else |e| {{ print(\"{{e}}\") }}\n}}\n");
        for (target, expected) in [
            ("x86_64-pc-windows-msvc", "unsupported\n"),
            ("x86_64-unknown-linux-gnu", "io\n"),
        ] {
            with_build_target(target, || {
                let (r, out) =
                    wolf_interp::observe_record_stdin(source.as_bytes(), &SchedRequest::Default);
                assert_eq!(r.verdict, Verdict::Exit(0), "{call}: {r:?}");
                assert_eq!(out.stdout, expected, "{call} for {target}");
                let (_, bundled) = with_build_target(wolf_interp::export::BUNDLE_TARGET, || {
                    wolf_interp::observe_record_stdin(source.as_bytes(), &SchedRequest::Default)
                });
                assert_eq!(bundled.stdout, "io\n");
            });
        }
    }
}

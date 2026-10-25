//! Specification-data mirror of [abi.c.variadic]; never execute native varargs.
use std::collections::BTreeSet;
use wolf_interp::{attrs, frontend::observe, phase::Phase, protocol::Verdict};

#[test]
fn pinned_anchor_keys_agree_both_ways() {
    let published: serde_json::Value =
        serde_json::from_str(include_str!("../vendor/upstream/spec/anchors.json")).unwrap();
    let actual: BTreeSet<&str> = published["anchors"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let expected: BTreeSet<&str> = include_str!("../ci/is81-variadic/anchor-keys.txt")
        .lines()
        .collect();
    let lost: Vec<_> = expected.difference(&actual).collect();
    let extra: Vec<_> = actual.difference(&expected).collect();
    assert!(
        lost.is_empty() && extra.is_empty(),
        "lost anchors: {lost:?}; unexpected anchors: {extra:?}"
    );
    assert_eq!(
        wolf_interp::UPSTREAM_PIN.trim(),
        "a5378e3fd5b6347f4a89a7d77dbe43aa576fee76"
    );
}

#[test]
fn variadic_corpus_parses_and_resolves() {
    let source = include_bytes!("../vendor/upstream/corpus/membrane/variadic_declarations.lu");
    for phase in [Phase::Parse, Phase::Resolve] {
        let obs =
            attrs::with_build_target("x86_64-unknown-linux-gnu", || observe(source, Some(phase)));
        assert_eq!(obs.verdict, Verdict::Pass, "{obs:?}");
    }
    let source = include_bytes!("../vendor/upstream/corpus/membrane/variadic_position.lu");
    let obs = observe(source, Some(Phase::Parse));
    assert_eq!(obs.verdict, Verdict::Fail("E0822".into()), "{obs:?}");
    let at = std::str::from_utf8(source).unwrap().rfind("...").unwrap() as u64;
    assert_eq!(obs.diagnostics[0].span, [at, at + 3]);
}

#[test]
fn ellipsis_is_only_a_final_unnamed_c_tail() {
    for source in [
        "extern \"c\" fn f(n: i32, ...) -> i64\n",
        "export fn f(n: i32, ...) -> i64 { 0 }",
        "extern \"c\" fn f(n: i32, ...) -> i64 { 0 }",
    ] {
        let obs = observe(source.as_bytes(), Some(Phase::Parse));
        assert_eq!(obs.verdict, Verdict::Pass, "{obs:?}");
    }
    for source in [
        "fn f(n: i32, ...) {}",
        "export fn f(...) {}",
        "export fn f(n: i32, ..., last: i32) {}",
        "export fn f(n: i32, ...,) {}",
        "export fn f(n: i32, ...: i64) {}",
        "export fn f(n: i32, mut ...) {}",
        "export fn f(n: i32, take ...) {}",
        "export fn f(n: i32 ...) {}",
        "export fn f(n: i32, tail: ...) {}",
    ] {
        let obs = observe(source.as_bytes(), Some(Phase::Parse));
        assert_eq!(
            obs.verdict,
            Verdict::Fail("E0822".into()),
            "{source}: {obs:?}"
        );
    }
}

#[test]
fn valist_is_a_free_type_name_including_a_decayed_c_parameter() {
    for source in [
        "fn consume(ap: VaList) {}",
        "extern \"c\" fn vprintf(fmt: *u8, ap: *VaList) -> i32\n",
    ] {
        let obs = observe(source.as_bytes(), Some(Phase::Resolve));
        assert_eq!(obs.verdict, Verdict::Pass, "{obs:?}");
    }
}

#[test]
fn checked_variadic_calls_are_named_refusals() {
    for (source, name) in [
        (
            "extern \"c\" fn sink(n: i32, ...) -> i64\nfn main() -> !int { unsafe { sink(1, 2) }; 0 }",
            "sink",
        ),
        (
            "export fn sum(n: i32, ...) -> i64 { print(\"must not execute\"); 0 }\nfn main() -> !int { sum(1, 2); 0 }",
            "sum",
        ),
    ] {
        let obs = attrs::with_build_target("x86_64-unknown-linux-gnu", || {
            observe(source.as_bytes(), None)
        });
        assert_eq!(obs.verdict, Verdict::Unsupported, "{obs:?}");
        assert!(obs.reason.as_deref().unwrap().contains(name), "{obs:?}");
        assert!(
            obs.reason.as_deref().unwrap().contains("variadic"),
            "{obs:?}"
        );
        assert!(obs.stdout.is_empty(), "a variadic frame was executed");
    }
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/variadic_is81")
            .join(name)
            .join("main.lu"),
    )
    .unwrap()
}

#[test]
fn declaration_diagnostics_match_the_measured_code_span_and_message() {
    for (name, code, span, message) in [
        (
            "bad_start",
            "E0824",
            [27, 37],
            "`va_start` needs the current function's C variadic frame",
        ),
        (
            "nested_frame",
            "E0824",
            [61, 71],
            "`va_start` needs the current function's C variadic frame",
        ),
        (
            "closure_frame",
            "E0824",
            [51, 61],
            "`va_start` needs the current function's C variadic frame",
        ),
        (
            "second_start",
            "E0825",
            [66, 76],
            "`va_start` is already spelled in this function — use `va_copy` for another cursor",
        ),
        (
            "function_value",
            "E0823",
            [58, 61],
            "`sum` is variadic — call it by name, not through a function value",
        ),
        (
            "bad_tail",
            "E0401",
            [76, 79],
            "an unnamed C argument must have a C-compatible value type",
        ),
        (
            "bad_arg",
            "E0401",
            [44, 62],
            "`va_arg` reads the promoted type — use `i32` for narrow integers or `f64` for `f32`",
        ),
        (
            "generic_list",
            "E0812",
            [15, 26],
            "`VaList` is one C-layout record and takes no type arguments",
        ),
        (
            "out_of_ring",
            "E1301",
            [29, 43],
            "the C call `va_end` needs an `unsafe` block",
        ),
        (
            "call_few_fixed",
            "E0402",
            [60, 65],
            "`sum` takes 1 argument, but this call passes 0",
        ),
    ] {
        let obs =
            attrs::with_build_target("x86_64-unknown-linux-gnu", || observe(&fixture(name), None));
        assert_eq!(obs.verdict, Verdict::Fail(code.into()), "{name}: {obs:?}");
        let detail = obs.detail.unwrap();
        assert_eq!(
            [detail.span.start as u64, detail.span.end as u64],
            span,
            "{name}"
        );
        assert_eq!(detail.message, message, "{name}");
    }
}

#[test]
fn checked_calls_name_exactly_the_measured_construct() {
    for (name, expected) in [
        (
            "call_c",
            "a C variadic call into C `sink` (the checked machine has no variadic membrane)",
        ),
        (
            "call_wolf",
            "a C variadic call to a wolf definition `sum` (the checked machine has no variadic membrane)",
        ),
        (
            "call_zero_tail",
            "a C variadic call to a wolf definition `sum` (the checked machine has no variadic membrane)",
        ),
        (
            "start",
            "a C variadic call to a wolf definition `sum` (the checked machine has no variadic membrane)",
        ),
        (
            "copy_raw",
            "the va_copy C variadic cursor builtin on the checked machine",
        ),
    ] {
        let obs =
            attrs::with_build_target("x86_64-unknown-linux-gnu", || observe(&fixture(name), None));
        assert_eq!(obs.verdict, Verdict::Unsupported, "{name}: {obs:?}");
        assert_eq!(obs.reason.as_deref(), Some(expected), "{name}: {obs:?}");
    }
}

#[test]
fn ordinary_consumers_and_shadowing_do_not_require_a_variadic_frame() {
    for name in ["arg", "arg_pointer", "copy", "end", "shadow_builtin"] {
        let obs =
            attrs::with_build_target("x86_64-unknown-linux-gnu", || observe(&fixture(name), None));
        assert_eq!(obs.verdict, Verdict::Exit(0), "{name}: {obs:?}");
    }
    for source in [
        "fn main() -> !int { fn va_end(x: int) -> int { x }\n va_end(0) }",
        "struct VaList[T] { x: T }\nfn consume(ap: VaList[int]) {}\nfn main() {}",
    ] {
        let obs = observe(source.as_bytes(), None);
        assert_eq!(obs.verdict, Verdict::Exit(0), "{source}: {obs:?}");
    }
}

#[test]
fn variadic_target_matrix_and_cfg_filtered_corpus() {
    for target in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-freebsd",
        "x86_64-unknown-none",
    ] {
        let obs = attrs::with_build_target(target, || {
            observe(
                include_bytes!("../vendor/upstream/corpus/membrane/variadic_declarations.lu"),
                Some(Phase::Resolve),
            )
        });
        assert_eq!(obs.verdict, Verdict::Pass, "{target}: {obs:?}");
        let source = b"export fn sum(n: i32, ...) -> i64 { 0 }\n";
        let obs = attrs::with_build_target(target, || observe(source, Some(Phase::Resolve)));
        if matches!(target, "x86_64-unknown-linux-gnu" | "x86_64-unknown-none") {
            assert_eq!(obs.verdict, Verdict::Pass, "{target}: {obs:?}");
        } else {
            assert_eq!(
                obs.verdict,
                Verdict::Fail("E0826".into()),
                "{target}: {obs:?}"
            );
            let detail = obs.detail.unwrap();
            assert_eq!(
                detail.message,
                format!("target `{target}` does not support C's unnamed arguments")
            );
        }
    }
}

#[test]
fn valist_has_the_published_record_layout() {
    let obs = observe(b"fn main() -> !int { assert(size_of(VaList) == 24); assert(align_of(VaList) == 8); assert(offset_of(VaList, gp_offset) == 0); assert(offset_of(VaList, fp_offset) == 4); assert(offset_of(VaList, overflow_arg_area) == 8); assert(offset_of(VaList, reg_save_area) == 16); 0 }", None);
    assert_eq!(obs.verdict, Verdict::Exit(0), "{obs:?}");
}

#[test]
fn every_cursor_dispatch_refuses_before_reading_a_cursor() {
    // The low-level engine deliberately bypasses admission here: no frame or
    // argument storage may be touched even if a caller bypasses sema. The
    // valid source-level va_start row instead refuses its containing call.
    for (callee, name) in [
        ("va_start", "va_start"),
        ("va_arg[i64]", "va_arg"),
        ("va_copy", "va_copy"),
        ("va_end", "va_end"),
    ] {
        let run =
            wolf_interp::eval::run_source("dispatch.lu", &format!("fn main() {{ {callee}() }}"))
                .unwrap();
        let wolf_interp::eval::Outcome::Unsupported(reason) = run.outcome else {
            panic!("{}: {:?}", name, run.outcome)
        };
        assert_eq!(
            reason,
            format!("the {name} C variadic cursor builtin on the checked machine")
        );
    }
}

#[test]
fn list_signatures_and_non_c_tail_shapes_are_checked_before_refusal() {
    for (name, code, span) in [
        ("arg_mode", "E1007", [53, 55]),
        ("end_mode", "E1007", [41, 43]),
        ("copy_mode", "E1007", [60, 62]),
        ("arg_arity", "E0402", [45, 67]),
        ("arg_generic", "E0812", [45, 59]),
        ("arg_type", "E0401", [58, 60]),
        ("bad_fixed", "E0401", [73, 76]),
        ("char_tail", "E0401", [76, 79]),
        ("fn_tail", "E0401", [86, 87]),
        ("list_tail", "E0401", [98, 100]),
    ] {
        let obs =
            attrs::with_build_target("x86_64-unknown-linux-gnu", || observe(&fixture(name), None));
        assert_eq!(obs.verdict, Verdict::Fail(code.into()), "{name}: {obs:?}");
        assert_eq!(obs.diagnostics[0].span, span, "{name}: {obs:?}");
    }
}

#[test]
fn the_six_old_soundness_rows_keep_the_closed_289_answer_at_the_new_pin() {
    for (name, stdout) in [
        ("region_copy_shares_str", "28\nbuilt in pass, 0 uses\n"),
        ("region_outer_write_field", "t9 1 9 x1 2 7\n"),
        ("region_outer_write_map", "40 v7 v39 true\n"),
        (
            "region_outer_write_str_push",
            "100 turn 0 turn 99 42 true\n",
        ),
        ("region_outer_write_struct", "50 r0 r49 3 49 98 147\n"),
        ("region_str_call_index_store", "[piece 7 3-7]\n"),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/upstream/corpus/memory")
            .join(format!("{name}.lu"));
        let obs = observe(&std::fs::read(path).unwrap(), None);
        assert_eq!(obs.verdict, Verdict::Exit(0), "{name}: {obs:?}");
        assert_eq!(obs.stdout, stdout.as_bytes(), "{name}");
    }
}

#[test]
fn command_line_records_the_named_refusal_and_the_spec_identity() {
    let version = std::process::Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&version.stdout).contains("pin a5378e3"));
    // CLI follows the real host. Linux is the supported executable row;
    // unsupported hosts must reject the declaration, not borrow SysV.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lupin"))
        .args(["conform-run", "--json"])
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/variadic_is81/call_wolf/main.lu"),
        )
        .output()
        .unwrap();
    let record: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    if wolf_interp::HOST_TRIPLE == "x86_64-unknown-linux-gnu" {
        assert_eq!(record["verdict"], "unsupported");
        assert_eq!(
            record["x-unsupported"],
            "a C variadic call to a wolf definition `sum` (the checked machine has no variadic membrane)"
        );
    } else {
        assert_eq!(record["verdict"], "fail(E0826)");
    }
}

#[test]
fn the_new_pin_charges_cwd_strings_and_catches_their_escape() {
    let obs = observe(
        include_bytes!("../vendor/upstream/corpus/memory/region_os_cwd_charged.lu"),
        None,
    );
    assert_eq!(obs.verdict, Verdict::Exit(0), "{obs:?}");
    assert_eq!(obs.stdout, b"charged true | reclaimed true | got true");
    let obs = observe(
        include_bytes!("../vendor/upstream/corpus/memory/region_os_cwd_return.lu"),
        None,
    );
    assert_eq!(
        obs.verdict,
        Verdict::Trap(wolf_interp::trap::TrapKind::RegionFault),
        "{obs:?}"
    );
}

#[test]
fn a_qualified_static_read_is_still_refused_with_its_module_context() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("vendor/upstream/corpus/memory/static_qualified_var_outside_unsafe/main.lu");
    let (record, _) = wolf_interp::observe_record(&path, &std::fs::read(&path).unwrap(), None);
    assert_eq!(record.verdict, Verdict::Fail("E1301".into()), "{record:?}");
}

#[test]
fn a_pattern_binding_shadows_the_variadic_function_value() {
    for body in [
        "for sum in 0..2 { let x = sum }\n0",
        "match 2 { sum => { let x = sum } }\n0",
        "let x = (fail() else |sum| { let x = sum; 0 })\n0",
    ] {
        let source = format!(
            "extern \"c\" fn sum(n: i32, ...) -> i64\nfn fail() -> int ! {{bad}} {{ bad }}\nfn main() -> !int {{ {body} }}"
        );
        let obs = attrs::with_build_target("x86_64-unknown-linux-gnu", || {
            observe(source.as_bytes(), None)
        });
        assert_eq!(obs.verdict, Verdict::Exit(0), "{source}: {obs:?}");
    }
}

#[test]
fn aliases_are_not_guessed_to_be_non_c_shapes() {
    for source in [
        "type I = i32\nextern \"c\" fn sink(n: I, ...) -> i64\nfn main() -> !int { unsafe { sink(1, 2) }; 0 }",
        "type I = i32\nextern \"c\" fn sink(n: i32, ...) -> i64\nfn main() -> !int { let n: I = 2; unsafe { sink(1, n) }; 0 }",
    ] {
        let obs = attrs::with_build_target("x86_64-unknown-linux-gnu", || {
            observe(source.as_bytes(), None)
        });
        assert_eq!(obs.verdict, Verdict::Unsupported, "{source}: {obs:?}");
        assert_eq!(
            obs.reason.as_deref(),
            Some("a C variadic call into C `sink` (the checked machine has no variadic membrane)")
        );
    }
}

#[test]
fn local_prelude_shadows_use_the_pinned_metadata_and_honor_allow() {
    let source = include_bytes!("../vendor/upstream/corpus/lints/shadow_prelude_local.lu");
    let obs = observe(source, None);
    assert_eq!(obs.verdict, Verdict::Exit(0), "{obs:?}");
    assert_eq!(obs.stdout, b"5 7\n");
    let shadows: Vec<_> = obs
        .warnings
        .unwrap()
        .into_iter()
        .filter(|w| w.code == "W0304")
        .collect();
    assert_eq!(shadows.len(), 1, "{shadows:?}");
    let at = std::str::from_utf8(source)
        .unwrap()
        .rfind("let clock_ms = 5")
        .unwrap()
        + 4;
    assert_eq!(shadows[0].span, [at as u64, at as u64 + 8]);
    for (source, count) in [
        ("fn main() { let VaList = 1 }", 1),
        ("fn main() { let (va_start, other) = (1, 2) }", 1),
        ("#[allow(w0304)]\nfn main() { let VaList = 1 }", 0),
        ("fn main() { let range = 1 }", 0),
    ] {
        let obs = observe(source.as_bytes(), None);
        assert_eq!(obs.verdict, Verdict::Exit(0), "{source}: {obs:?}");
        assert_eq!(
            obs.warnings
                .unwrap()
                .iter()
                .filter(|w| w.code == "W0304")
                .count(),
            count,
            "{source}"
        );
    }
}

#[test]
fn an_imported_declaration_shadows_the_cursor_builtin() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/variadic_is81/shadow_import/main.lu");
    let (record, _) = wolf_interp::observe_record(&path, &std::fs::read(&path).unwrap(), None);
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
}

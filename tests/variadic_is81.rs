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

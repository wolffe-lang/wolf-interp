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

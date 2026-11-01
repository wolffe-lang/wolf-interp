//! is82: lupin mirrors wolf-lang trunk (`b16353e8`) on the items
//! wolf-interp#304, #302, #299, #295 and #277 name, plus #310's index-store
//! twin. Every expected answer below is the compiler's, measured black-box
//! with wolf 0.2.36 (`conform-run --checked --json`, the first diagnostic by
//! `[proto.record.first]`'s earliest offset); the fixtures are specification
//! data, never compiler source. #310's two corpus rows are pinned in
//! `tests/run_corpus.rs`'s run ledger.

use std::path::{Path, PathBuf};
use std::process::Command;

use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/is82")
        .join(rel)
}

fn corpus(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
        .join(rel)
}

/// The record and the whole stdout of the program at `path`, its module
/// graph loaded (a directory is a module, D32).
fn observe(path: &Path) -> (wolf_interp::protocol::ObservationRecord, String) {
    let source = std::fs::read(path).expect("fixture readable");
    let (record, observed) = wolf_interp::observe_record(path, &source, None);
    (record, observed.stdout)
}

fn first_error(record: &wolf_interp::protocol::ObservationRecord) -> Option<(String, [u64; 2])> {
    record
        .diagnostics
        .iter()
        .find(|d| d.severity == "error")
        .map(|d| (d.code.clone(), d.span))
}

// -- wolf-interp#304: E0202 at the opener the compiler reports ------------

/// `(fixture, code, start, end)`: the compiler's first diagnostic. The
/// shapes, by family: `a*` a `{` opening an interpolation in a plain string
/// (a JSON-looking literal swallows the call's `)`); `d*` closers inside an
/// interpolation; `e*`, `f*`, `h*` nested groups the parser wrecks (the
/// innermost opener is reported); `g*` a statement after a broken `(`;
/// `j*` a parse error inside a group that does or does not close; `k*` the
/// end of the file.
const DELIMS: &[(&str, &str, u64, u64)] = &[
    ("a1", "E0102", 24, 36),
    ("a2", "E0202", 21, 22),
    ("a3", "E0102", 24, 27),
    ("a4", "E0202", 21, 22),
    ("a5", "E0202", 35, 36),
    ("a6", "E0102", 44, 76),
    ("a7", "E0202", 46, 47),
    ("d1", "E0202", 47, 48),
    ("d2", "E0202", 21, 22),
    ("d3", "E0202", 24, 25),
    ("d4", "E0202", 25, 26),
    ("d5", "E0202", 21, 22),
    ("d6", "E0202", 22, 23),
    ("e1", "E0202", 29, 30),
    ("e2", "E0202", 25, 26),
    ("e3", "E0202", 29, 30),
    ("e4", "E0202", 22, 23),
    ("e5", "E0202", 23, 24),
    ("f1", "E0202", 29, 30),
    ("f2", "E0202", 29, 30),
    ("f3", "E0202", 24, 25),
    ("g1", "E0202", 24, 25),
    ("g2", "E0202", 24, 25),
    ("g3", "E0202", 24, 25),
    ("g4", "E0202", 24, 25),
    ("g5", "E0202", 24, 25),
    ("g6", "E0202", 24, 25),
    ("g7", "E0202", 24, 25),
    ("g8", "E0202", 24, 25),
    ("g9", "E0202", 24, 25),
    ("g10", "E0202", 10, 11),
    ("h1", "E0202", 24, 25),
    ("h2", "E0202", 25, 26),
    ("h3", "E0202", 25, 26),
    ("h4", "E0202", 28, 29),
    ("j1", "E0201", 58, 59),
    ("j2", "E0201", 29, 30),
    ("j3", "E0202", 24, 25),
    ("j4", "E0201", 30, 31),
    ("k1", "E0202", 24, 25),
    ("k2", "E0202", 24, 25),
    ("k3", "E0202", 24, 25),
    ("k4", "E0202", 10, 11),
    ("k5", "E0202", 19, 20),
    ("k6", "E0202", 24, 25),
];

#[test]
fn the_first_frontend_diagnostic_is_the_compilers_code_and_span() {
    let mut parted = Vec::new();
    for (name, code, start, end) in DELIMS {
        let source = std::fs::read(fixture(&format!("delims/{name}.lu"))).unwrap();
        let obs = wolf_interp::frontend::observe(&source, None);
        let first = obs.diagnostics.first().map(|d| (d.code.clone(), d.span));
        let want = ((*code).to_owned(), [*start, *end]);
        if obs.verdict != Verdict::Fail((*code).to_owned()) || first.as_ref() != Some(&want) {
            parted.push(format!("{name}: want {want:?}, got {:?} {first:?}", obs.verdict));
        }
    }
    assert!(parted.is_empty(), "{}", parted.join("\n"));
}

// -- wolf-interp#304: E0406, a call of a value that is not a function ------

/// The call rows lupin's resolve walk can class: a local bound to a
/// literal. `b6` (a struct FIELD called, `p.x()`) needs field types this
/// walk does not carry and is the named remainder (`ci/is82.md`).
const CALLS: &[(&str, &str, u64, u64)] = &[
    ("b1", "E0412", 47, 54),
    ("b2", "E0412", 39, 45),
    ("b3", "E0406", 51, 54),
    ("b4", "E0413", 41, 46),
    ("b5", "E0412", 72, 76),
    ("b7", "E0406", 40, 41),
];

#[test]
fn calling_a_classed_local_is_e0406_at_the_callee() {
    let mut parted = Vec::new();
    for (name, code, start, end) in CALLS {
        let source = std::fs::read(fixture(&format!("calls/{name}.lu"))).unwrap();
        let obs = wolf_interp::frontend::observe(&source, None);
        let first = obs.diagnostics.first().map(|d| (d.code.clone(), d.span));
        let want = ((*code).to_owned(), [*start, *end]);
        if obs.verdict != Verdict::Fail((*code).to_owned()) || first.as_ref() != Some(&want) {
            parted.push(format!("{name}: want {want:?}, got {:?} {first:?}", obs.verdict));
        }
    }
    assert!(parted.is_empty(), "{}", parted.join("\n"));
}

// -- wolf-interp#302: a mark raised through an imported error-set alias ----

#[test]
fn a_qualified_foreign_alias_is_its_tags_at_the_raise_site() {
    // clef as01's probe: `-> int ! errors.Failures`, `return broken`.
    let (record, stdout) = observe(&fixture("alias_rows/qualified/main.lu"));
    assert_eq!(record.verdict, Verdict::Exit(1), "{record:?}");
    assert_eq!(stdout, "error: broken\n");
}

#[test]
fn a_use_bound_foreign_alias_is_its_tags_at_the_raise_site() {
    let (record, stdout) = observe(&fixture("alias_rows/use_bound/main.lu"));
    assert_eq!(record.verdict, Verdict::Exit(1), "{record:?}");
    assert_eq!(stdout, "error: lost\n");
}

#[test]
fn a_foreign_alias_row_propagates_and_is_matched_by_its_tags() {
    let (record, stdout) = observe(&fixture("alias_rows/handled/main.lu"));
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
    assert_eq!(stdout, "broken lost 6\n");
}

#[test]
fn a_private_foreign_alias_is_e0304_at_the_qualified_path() {
    let (record, _) = observe(&fixture("alias_rows/private/main.lu"));
    assert_eq!(record.verdict, Verdict::Fail("E0304".into()), "{record:?}");
    assert_eq!(first_error(&record), Some(("E0304".into(), [31, 46])));
}

// -- wolf-interp#299: `use m as alias` ----------------------------------------

#[test]
fn a_module_alias_resolves_to_the_module() {
    // scent ct04's witness, byte for byte.
    let (record, stdout) = observe(&fixture("alias_use/renamed/main.lu"));
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
    assert_eq!(stdout, "42\n");
    let (record, stdout) = observe(&fixture("alias_use/beside_list/main.lu"));
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
    assert_eq!(stdout, "42 84\n");
    // The control that already resolved: a dotted path under an alias.
    let (record, stdout) = observe(&fixture("alias_use/dotted/main.lu"));
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
    assert_eq!(stdout, "7\n");
}

#[test]
fn an_aliased_module_is_not_also_bound_under_its_own_name() {
    // `use math as renamed` binds `renamed` only: `math.value()` is the
    // compiler's E0301, behind E0305 for the alias nobody used.
    let (record, _) = observe(&fixture("alias_use/head_unbound/main.lu"));
    assert_eq!(record.verdict, Verdict::Fail("E0305".into()), "{record:?}");
    assert_eq!(first_error(&record), Some(("E0305".into(), [12, 19])));
}

// -- wolf-interp#295: s261's argv readers and the explicit argv input -------

#[test]
fn the_two_corpus_argv_rows_run() {
    let (record, stdout) = observe(&corpus("os/arg0.lu"));
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
    assert_eq!(stdout, "true\n");
    let (record, stdout) = observe(&corpus("os/args_bytes.lu"));
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
    assert_eq!(stdout, "args: 0\n");
}

#[test]
fn the_program_name_is_a_region_escape_site() {
    // wolf: fail(E1010) at mem; this machine: the read after the free.
    let (record, _) = observe(&fixture("argv/name_escape/main.lu"));
    assert_eq!(
        record.verdict,
        Verdict::Trap(TrapKind::RegionFault),
        "{record:?}"
    );
}

fn conform_run(program: &str, argv: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lupin"));
    cmd.args(["conform-run", "--json"])
        .arg(fixture(&format!("argv/{program}/main.lu")));
    for arg in argv {
        cmd.arg(format!("--argv-hex={arg}"));
    }
    cmd.output().expect("lupin runs")
}

fn record_of(out: &std::process::Output) -> serde_json::Value {
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("one record")
}

#[test]
fn argv_hex_supplies_argv_and_the_readers_answer_the_checked_machines_bytes() {
    let rec = record_of(&conform_run("bytes", &["7368", "78ff79", "", "c3a9"]));
    assert_eq!(rec["verdict"], "exit(0)");
    assert_eq!(
        rec["stdout_inline"],
        "115\n104\nend\n120\n255\n121\nend\nend\n195\n169\nend\n"
    );
    let rec = record_of(&conform_run("count", &["7368", "6f6b", "78ff79"]));
    assert_eq!(rec["verdict"], "trap(argument-utf8)");
    assert_eq!(
        rec["trap_message"],
        "argument 2 is not UTF-8; read it with env_args_bytes()"
    );
    let rec = record_of(&conform_run("count", &["ff", "6f6b"]));
    assert_eq!(rec["stdout_inline"], "1\n", "env_args never decodes argv[0]");
    let rec = record_of(&conform_run("name", &["ff", "6f6b"]));
    assert_eq!(rec["verdict"], "trap(argument-utf8)");
    assert_eq!(
        rec["trap_message"],
        "argument 0 is not UTF-8; read it with env_args_bytes()"
    );
    let rec = record_of(&conform_run("name", &["7368", "ff"]));
    assert_eq!(rec["stdout_inline"], "sh\n", "env_arg0 decodes argv[0] only");
    let rec = record_of(&conform_run("count", &["7368", ""]));
    assert_eq!(rec["stdout_inline"], "1\n", "an empty HEX is an empty argument");
    let rec = record_of(&conform_run("count", &["AB"]));
    assert_eq!(rec["stdout_inline"], "0\n", "upper-case digits are hex");
}

#[test]
fn without_argv_hex_argv_is_absent() {
    let rec = record_of(&conform_run("name", &[]));
    assert_eq!(rec["verdict"], "exit(0)");
    assert_eq!(rec["stdout_inline"], "\n", "an absent name is empty text");
    let rec = record_of(&conform_run("bytes_len", &[]));
    assert_eq!(rec["stdout_inline"], "0\n");
    let rec = record_of(&conform_run("count", &[]));
    assert_eq!(rec["stdout_inline"], "0\n");
}

#[test]
fn a_malformed_argv_hex_is_a_tool_error_naming_the_option() {
    for bad in ["f", "gg", "+1", "00", "é", "ff00"] {
        let out = conform_run("count", &[bad]);
        assert_eq!(out.status.code(), Some(2), "--argv-hex={bad}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("--argv-hex"),
            "--argv-hex={bad}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty(), "no record for a refused input");
    }
}

#[test]
fn the_trap_vocabulary_is_the_specs_fourteen() {
    // `[conf.trap.set]` at the pin: twelve kinds, `stack-overflow` and
    // `argument-utf8` (s261).
    for kind in ["stack-overflow", "argument-utf8"] {
        assert!(TrapKind::parse(kind).is_some(), "{kind}");
    }
    assert_eq!(TrapKind::ALL.len(), 14);
}

// -- wolf-interp#277: the host's name and architecture -----------------------

/// `[os.uname]`'s words for this host: the kernel's `uname` lowercased, with
/// Darwin as `macos` and `arm64`/`amd64` spelled as linux spells them; on
/// windows the build's architecture.
fn host_words() -> (String, String) {
    if cfg!(windows) {
        return ("windows".into(), std::env::consts::ARCH.into());
    }
    let ask = |flag: &str| {
        let out = Command::new("uname").arg(flag).output().expect("uname");
        String::from_utf8(out.stdout).unwrap().trim().to_lowercase()
    };
    let name = match ask("-s").as_str() {
        "darwin" => "macos".to_owned(),
        other => other.to_owned(),
    };
    let arch = match ask("-m").as_str() {
        "arm64" => "aarch64".to_owned(),
        "amd64" => "x86_64".to_owned(),
        other => other.to_owned(),
    };
    (name, arch)
}

#[test]
fn os_name_and_os_arch_answer_the_hosts_uname() {
    let (record, stdout) = observe(&fixture("host/main.lu"));
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
    let (name, arch) = host_words();
    assert_eq!(stdout, format!("{name} {arch}\n"));
}

#[test]
fn the_core_limit_corpus_row_runs() {
    let (record, stdout) = observe(&corpus("os/core_limit_rows.lu"));
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
    assert_eq!(
        stdout,
        "soft_below=invalid\nhard_below=invalid\nsoft_above_hard=invalid\n\
         soft_unlimited_under_a_finite_hard=invalid\nfar_below=invalid\n\
         read_is_a_limit: true\nset_as_it_is: true\n"
    );
}

// -- wolf-interp#310's twin: a plain INDEX store copies in the ambient -----

#[test]
fn a_plain_index_store_of_an_outer_list_in_a_block_is_the_blocks_copy() {
    // wolf: fail(E1004) (the cross-region store); this machine: the read
    // after the block's free, E1004's dynamic counterpart.
    let (record, _) = observe(&fixture("stores/index_store/main.lu"));
    assert_eq!(
        record.verdict,
        Verdict::Trap(TrapKind::RegionFault),
        "{record:?}"
    );
}

#[test]
fn a_plain_push_of_a_str_holding_struct_still_shares() {
    // No List or Map in the element: nothing is copied into the block.
    let (record, stdout) = observe(&fixture("stores/str_struct/main.lu"));
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
    assert_eq!(stdout, "1 outer 1\n");
    let (record, stdout) = observe(&fixture("stores/no_block/main.lu"));
    assert_eq!(record.verdict, Verdict::Exit(0), "{record:?}");
    assert_eq!(stdout, "1 3 0\n");
}

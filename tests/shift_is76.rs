//! is76 (wolf-interp#251) — lupin's half of `[type.int.shift]` (wolf-lang
//! s237, ruling #77 A), where lupin 0.1.53 disagreed with the rule.
//!
//! `tests/fixtures/shift_is76/` holds wolf-lang's shift corpus rows byte for
//! byte (trunk `56929c63`): `comptime/shift_overflow.lu`,
//! `typecheck/int_shift_value.lu` and the four `faults/int_shift_*` rows,
//! each held to its own `check:` header. The other programs are lupin's own
//! witnesses, each answer measured on the compiler's checked machine
//! (`wolf conform-run --checked --json`, the trunk above, kasumi):
//!
//! 1. A count of 128 or more on a plain integer is no case of its own:
//!    `255 >> 200` at `u8` is 0 (lupin 0.1.53 trapped: its 128-bit shift
//!    ran out of bits), and a set bit shifted past every bit lupin holds
//!    traps (0.1.53 printed 0 for `2 << 127` at `u64`).
//! 2. A module `const` or `let` initialiser is checked against its type:
//!    `const K: u8 = 200 << 1` is E0706 at the operator (0.1.53 printed
//!    400).
//! 3. A literal-only term in a declared integer context computes at that
//!    type: `let r: wrapping[u8] = 1 << 9` is 2 (0.1.53 computed 512 and
//!    trapped).
//! 4. `x << n` with `x: u64` and `n: int` is E0401 (0.1.53 ran it).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shift_is76")
}

/// A fresh directory: a directory is a module (D32), so every program
/// here gets one of its own.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

fn observe(entry: &Path) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .current_dir(entry.parent().expect("a directory"))
        .stdin(Stdio::null())
        .output()
        .expect("lupin observes");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "one record ({e}): {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// The record of `text`, run alone in its own directory.
fn run(name: &str, text: &str) -> serde_json::Value {
    let dir = scratch(&format!("is76-shift-{name}"));
    let entry = dir.join(format!("{name}.lu"));
    std::fs::write(&entry, text).expect("written");
    observe(&entry)
}

/// `(verdict, stdout)` of a record.
fn answer(rec: &serde_json::Value) -> (String, String) {
    (
        rec["verdict"].as_str().unwrap_or("").to_owned(),
        rec["stdout_inline"].as_str().unwrap_or("").to_owned(),
    )
}

/// The first diagnostic's code and span.
fn first_diag(rec: &serde_json::Value) -> (String, Vec<u64>) {
    let d = &rec["diagnostics"][0];
    (
        d["code"].as_str().unwrap_or("").to_owned(),
        d["span"]
            .as_array()
            .map(|s| s.iter().filter_map(serde_json::Value::as_u64).collect())
            .unwrap_or_default(),
    )
}

/// What a corpus row's `check:` header asks: `("fail(CODE)", "")`,
/// `("trap(kind)", "")` or `("exit(0)", stdout)`.
fn header(text: &str, name: &str) -> (String, String) {
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("//! check: "))
        .unwrap_or_else(|| panic!("{name} has no check: header"));
    if let Some(code) = line.strip_prefix("fail(") {
        return (format!("fail({}", code), String::new());
    }
    if let Some(rest) = line.strip_prefix("run(exit=trap(") {
        let kind = rest.split(')').next().expect("a trap kind");
        return (format!("trap({kind})"), String::new());
    }
    let lit = line
        .strip_prefix("run(exit=0, stdout=")
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{name}: unread check header {line:?}"));
    let out: String = serde_json::from_str(lit).expect("a JSON string literal");
    ("exit(0)".to_owned(), out)
}

/// The compiler's corpus rows, each to its header — one directory each.
#[test]
fn the_compilers_shift_rows_answer_their_headers() {
    let mut names: Vec<String> = std::fs::read_dir(fixtures())
        .expect("the fixtures")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".lu"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 6, "the six rows: {names:?}");
    let mut bad = Vec::new();
    for name in &names {
        let text = std::fs::read_to_string(fixtures().join(name)).expect("row reads");
        let (verdict, stdout) = header(&text, name);
        let dir = scratch(&format!("is76-row-{}", name.trim_end_matches(".lu")));
        let entry = dir.join(name);
        std::fs::write(&entry, &text).expect("copied");
        let rec = observe(&entry);
        let got = answer(&rec);
        if got != (verdict.clone(), stdout.clone()) {
            bad.push(format!("{name}: want {verdict} {stdout:?}; got {got:?}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// #251.1: a count past 127 is still the value's shift; a set bit shifted
/// past the 128 bits lupin computes in traps rather than vanishing.
#[test]
fn a_count_past_127_is_the_value() {
    let rec = run(
        "past_127",
        "fn r8(x: u8, n: u8) -> u8 {\n    x >> n\n}\n\n\
         fn l8(x: u8, n: u8) -> u8 {\n    x << n\n}\n\n\
         fn r16(x: i16, n: i16) -> i16 {\n    x >> n\n}\n\n\
         fn rint(x: int, n: int) -> int {\n    x >> n\n}\n\n\
         fn main() {\n    print(\"{r8(255, 200)} {l8(0, 200)} {r16(-5, 130)} {rint(-1, 1000)}\")\n}\n",
    );
    assert_eq!(
        answer(&rec),
        ("exit(0)".into(), "0 0 -1 -1\n".into()),
        "{rec}"
    );
    let rec = run(
        "lost_127",
        "fn l64(x: u64, n: u64) -> u64 {\n    x << n\n}\n\n\
         fn main() {\n    print(\"{l64(2, 127)}\")\n}\n",
    );
    assert_eq!(
        answer(&rec),
        ("trap(overflow)".into(), String::new()),
        "{rec}"
    );
    let rec = run(
        "lost_200",
        "fn l8(x: u8, n: u8) -> u8 {\n    x << n\n}\n\n\
         fn main() {\n    print(\"{l8(1, 200)}\")\n}\n",
    );
    assert_eq!(
        answer(&rec),
        ("trap(overflow)".into(), String::new()),
        "{rec}"
    );
}

/// #251.2: a module item's initialiser is evaluated by the rule, and a
/// fault there is E0706 at the operator that faulted — the checked
/// machine's code and span on every row (the record's span is bytes).
#[test]
fn a_module_initialiser_that_faults_is_e0706() {
    for (name, text, span) in [
        ("const_u8", "const K: u8 = 200 << 1\n", [14, 22]),
        ("const_int", "const K: int = 1 << 63\n", [15, 22]),
        ("const_plus", "const K: u8 = 200 + 100\n", [14, 23]),
        ("let_mod", "let K: u8 = 200 << 1\n", [12, 20]),
        ("const_nested", "const K: u8 = (1 << 9) >> 2\n", [15, 21]),
        ("const_inter", "const K: u8 = 255 + 1 - 1\n", [14, 21]),
        ("const_neg", "const K: int = 8 >> -1\n", [15, 22]),
        ("const_div", "const K: int = 1 / 0\n", [15, 20]),
        (
            "const_ref",
            "const A: u8 = 200\n\nconst K: u8 = A << 1\n",
            [33, 39],
        ),
        (
            "const_second",
            "const A: u8 = 1\n\nconst K: u8 = 200 << 1\n",
            [31, 39],
        ),
    ] {
        let rec = run(
            name,
            &format!("{text}\nfn main() {{\n    print(\"{{K}}\")\n}}\n"),
        );
        assert_eq!(rec["verdict"], "fail(E0706)", "{name}: {rec}");
        assert_eq!(
            first_diag(&rec),
            ("E0706".to_owned(), span.to_vec()),
            "{name}: {rec}"
        );
    }
    // What fits is the value, and what the evaluator cannot read is left
    // to the run (a call: the run answers).
    let rec = run(
        "const_ok",
        "const A: u8 = 1 << 7\n\nconst B: u8 = 255 >> 8\n\nconst C: int = -1 << 63\n\n\
         const D: u64 = (1 << 63) | 1\n\n\
         fn main() {\n    print(\"{A} {B} {C} {D}\")\n}\n",
    );
    assert_eq!(
        answer(&rec),
        (
            "exit(0)".into(),
            "128 0 -9223372036854775808 9223372036854775809\n".into()
        ),
        "{rec}"
    );
}

/// #251.3: two literal operands meet the declared type before the operator
/// runs, so a `wrapping[T]` shift masks its count — "2 255" on the checked
/// machine — and a plain type's term traps where it leaves the type.
#[test]
fn a_literal_term_computes_at_its_declared_type() {
    let rec = run(
        "wrap_let",
        "fn main() {\n    let r: wrapping[u8] = 1 << 9\n    let s: wrapping[u8] = 255 >> 8\n    \
         print(\"{r} {s}\")\n}\n",
    );
    assert_eq!(answer(&rec), ("exit(0)".into(), "2 255\n".into()), "{rec}");
    let rec = run(
        "wrap_places",
        "fn f() -> wrapping[u8] {\n    1 << 9\n}\n\n\
         fn g() -> wrapping[i8] {\n    return -128 >> 9\n}\n\n\
         fn main() {\n    var v: wrapping[u8] = 3\n    v = 1 << 10\n    var w: wrapping[u8] = 0\n    \
         w += 1 << 9\n    print(\"{f()} {g()} {v} {w}\")\n}\n",
    );
    assert_eq!(
        answer(&rec),
        ("exit(0)".into(), "2 -64 4 2\n".into()),
        "{rec}"
    );
    let rec = run(
        "plain_inter",
        "fn main() {\n    let x: u8 = 255 + 1 - 1\n    print(\"{x}\")\n}\n",
    );
    assert_eq!(
        answer(&rec),
        ("trap(overflow)".into(), String::new()),
        "{rec}"
    );
    let rec = run(
        "plain_ok",
        "fn main() {\n    let x: i8 = -128 >> 1\n    let y: int = -9223372036854775807 - 1\n    \
         let z: u64 = 1 << 63\n    print(\"{x} {y} {z}\")\n}\n",
    );
    assert_eq!(
        answer(&rec),
        (
            "exit(0)".into(),
            "-64 -9223372036854775808 9223372036854775808\n".into()
        ),
        "{rec}"
    );
}

/// #251.4 (`[type.int.shift]` (5)): the count has the operand's type, so
/// two operands of different declared integer types are E0401, spanned on
/// the right operand as the checked machine spans it. An inferred binding
/// is not a declared type and says nothing.
#[test]
fn a_shift_takes_one_integer_type() {
    for (name, text, span) in [
        (
            "mixed_l",
            "fn f(x: u64, n: int) -> u64 {\n    x << n\n}\n\nfn main() {\n    print(\"{f(1, 3)}\")\n}\n",
            [39, 40],
        ),
        (
            "mixed_r",
            "fn f(x: u8, n: i32) -> u8 {\n    x >> n\n}\n\nfn main() {\n    print(\"{f(64, 3)}\")\n}\n",
            [37, 38],
        ),
    ] {
        let rec = run(name, text);
        assert_eq!(rec["verdict"], "fail(E0401)", "{name}: {rec}");
        assert_eq!(
            first_diag(&rec),
            ("E0401".to_owned(), span.to_vec()),
            "{name}: {rec}"
        );
    }
    let rec = run(
        "infer_let",
        "fn f(n: u8) -> u8 {\n    let a = 1\n    a << n\n}\n\n\
         fn g(x: u64, n: u64) -> u64 {\n    let m: u64 = n\n    x << m\n}\n\n\
         fn main() {\n    print(\"{f(3)} {g(1, 4)}\")\n}\n",
    );
    assert_eq!(answer(&rec), ("exit(0)".into(), "8 16\n".into()), "{rec}");
}

/// The initialiser fold stops judging past 256 open items, as the E0705
/// walk does: `rulings_is73`'s 2000-item chain overflowed windows' 1 MiB
/// main stack through the fold (the windows smoke of run 38975226904) while
/// linux's 8 MiB hid it. Here the chain runs under a 1 MiB stack on unix,
/// set by `sh`'s `ulimit -s` for lupin alone, and must answer a record.
#[cfg(unix)]
#[test]
fn a_deep_initialiser_chain_folds_on_a_small_stack() {
    let n = 2000;
    let mut text = String::new();
    for i in 0..n {
        let next = if i + 1 < n {
            format!("A{} + 1", i + 1)
        } else {
            "1".to_owned()
        };
        text.push_str(&format!("let A{i}: int = {next}\n\n"));
    }
    text.push_str("fn main() -> int {\n    A0 - A0\n}\n");
    let dir = scratch("is76-shift-deep_chain");
    let entry = dir.join("deep_chain.lu");
    std::fs::write(&entry, text).expect("written");
    let out = Command::new("sh")
        .arg("-c")
        .arg("ulimit -s 1024 && exec \"$0\" conform-run \"$1\" --json")
        .arg(env!("CARGO_BIN_EXE_lupin"))
        .arg(&entry)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .expect("lupin observes");
    assert_eq!(
        out.status.code(),
        Some(0),
        "a record, not a crash: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("one record");
    assert!(
        rec["verdict"] == "unsupported" || rec["verdict"] == "exit(0)",
        "{rec}"
    );
}

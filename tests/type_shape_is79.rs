//! is79 (wolf-interp#275; wolf-std sc67, wolf-std#81) — a type's shape
//! goes red here too: a struct literal whose type does not exist, a
//! misspelled field of an imported struct, and a parameter typed with a
//! missing type are refused, as the compilers refuse them. The qualified
//! and literal-head half of wolf-interp#79 (an annotation's name) and #94
//! (a literal's fields), which covered single-segment annotations and the
//! structs a module declares itself.
//!
//! The programs under `tests/fixtures/type_shape_is79/` are wolf-std
//! `e036bec`'s `docs/sc67-witness/loose/` byte for byte (sha256 `c161975d…`
//! no_such_type, `3c3ddbc4…` wrong_field, `6680db0a…` param_type,
//! `30245e2b…` local_no_such_type). Each `use std.net`; here they resolve
//! against a std root of this test's own whose `net` module declares `Addr`
//! as std's does (`version: int`, `text: str`), so the test needs no std
//! tree. wolf 0.2.34's archive (is79 `runs/m3/`, the real std `e036bec`):
//! checked and native `fail(E0301)` at `NoSuchType` (49..59), `fail(E0408)`
//! over the literal (45..86, with two E0403 after it), `fail(E0301)` at the
//! parameter's `NoSuchType` (28..38) — and `unsupported` ("this
//! struct-literal head") for the local `Fake { … }`, where this machine
//! answers the clause's E0301 at the name. lupin 0.1.55 ran all four.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("stale scratch removed");
    }
    std::fs::create_dir_all(&dir).expect("scratch created");
    dir
}

/// A std root holding one module, `net`, with std's `Addr`.
fn std_root(dir: &Path) -> PathBuf {
    let root = dir.join("stdroot");
    std::fs::create_dir_all(root.join("net")).expect("the std root");
    std::fs::write(
        root.join("net/net.lu"),
        "pub struct Addr {\n    version: int,\n    text: str,\n}\n\npub fn loopback(port: int) -> str {\n    \"127.0.0.1:{port}\"\n}\n",
    )
    .expect("net.lu");
    root
}

/// `program` as `main.lu` in a scratch directory of its own (a directory is
/// a module), observed against the test's std root.
fn observe_text(slug: &str, program: &str) -> serde_json::Value {
    let dir = scratch(&format!("type_shape_is79_{slug}"));
    let work = dir.join("w");
    std::fs::create_dir_all(&work).expect("the program's directory");
    let entry = work.join("main.lu");
    std::fs::write(&entry, program).expect("the program is written");
    let root = std_root(&dir);
    let out: Output = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .arg("conform-run")
        .arg(&entry)
        .arg("--json")
        .arg("--std-root")
        .arg(&root)
        .env_remove("LUPIN_STD")
        .current_dir(&work)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("lupin observes");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .filter(|r| r.get("verdict").is_some())
        .unwrap_or_else(|| panic!("a record: {out:?}"))
}

fn witness(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/type_shape_is79")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The record is `fail(code)` with its first diagnostic at `span`.
fn assert_refused(rec: &serde_json::Value, code: &str, span: [u64; 2], what: &str) {
    assert_eq!(rec["verdict"], format!("fail({code})"), "{what}: {rec}");
    let first = &rec["diagnostics"][0];
    assert_eq!(first["code"], code, "{what}: {rec}");
    assert_eq!(
        first["span"],
        serde_json::json!(span),
        "{what}: the compiler's span: {rec}"
    );
}

/// `net.NoSuchType { … }`: the module is real and has no such type —
/// E0301 at the name, `[mod]`'s resolve rung.
#[test]
fn a_literal_of_a_type_the_module_does_not_have_is_e0301() {
    let rec = observe_text("no_such_type", &witness("no_such_type.lu"));
    assert_refused(&rec, "E0301", [49, 59], "no_such_type.lu");
    assert_eq!(rec["phase_reached"], "resolve", "{rec}");
}

/// `net.Addr { versio: 4, … }`: the literal leaves `version` out —
/// `[type.struct.literal]`'s E0408 over the whole literal, as #94 answers a
/// local struct's.
#[test]
fn a_misspelled_field_of_an_imported_struct_is_e0408() {
    let rec = observe_text("wrong_field", &witness("wrong_field.lu"));
    assert_refused(&rec, "E0408", [45, 86], "wrong_field.lu");
    let message = rec["diagnostics"][0]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("`version`"),
        "the missing field named: {rec}"
    );
}

/// `fn show(a: net.NoSuchType)`: the parameter's type is the first site —
/// E0301 at its name, before the literal in `main`.
#[test]
fn a_parameter_typed_with_a_missing_type_is_e0301() {
    let rec = observe_text("param_type", &witness("param_type.lu"));
    assert_refused(&rec, "E0301", [28, 38], "param_type.lu");
}

/// `Fake { y: 2 }` with nothing named `Fake` anywhere: E0301 at the name
/// (the compiled tiers answer `unsupported` there at 0.2.34).
#[test]
fn a_literal_of_a_local_name_that_does_not_exist_is_e0301() {
    let rec = observe_text("local_no_such_type", &witness("local_no_such_type.lu"));
    assert_refused(&rec, "E0301", [87, 91], "local_no_such_type.lu");
}

/// What still runs: the literal and the parameter of the type the module
/// has, every field written, and a local struct's literal.
#[test]
fn the_types_that_exist_still_run() {
    let rec = observe_text(
        "controls",
        "use std.net

struct Real {
    x: int,
}

fn show(a: net.Addr) -> str {
    a.text
}

fn main() -> !int {
    let r = Real { x: 1 }
    let a = net.Addr { version: 4, text: \"127.0.0.1\" }
    print(\"{r.x} {a.version} {show(a)} {net.loopback(0)}\")
    0
}
",
    );
    assert_eq!(rec["verdict"], "exit(0)", "{rec}");
    assert_eq!(
        rec["stdout_inline"],
        "1 4 127.0.0.1 127.0.0.1:0\n",
        "{rec}"
    );
}

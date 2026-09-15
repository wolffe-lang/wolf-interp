//! `[conf.resolve.ambient]` — **a declaration wins its own name** (s157,
//! wolf-lang#44, wolf-std F-0047; wolf-interp#107).
//!
//! lupin 0.1.34 already answered `corpus/lints/shadow_prelude_call.lu`
//! correctly, and #107 asks for it to be pinned so the healing does not
//! regress. The corpus file is one file; the clause says "from any file of
//! that module, at any tier", and names the one exception — the comptime
//! intrinsics, `assert` first, which a declaration cannot shadow. Those two
//! shapes are the corpus's blind spots and are pinned here, each measured on
//! wolf 0.2.14 `--checked` (pin 30731a6) with the same verdict, stdout and
//! W0304 span this file asserts.

use std::path::PathBuf;
use std::process::Command;

fn module(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "wolf-interp-resolve-ambient-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp module dir");
    for (file, source) in files {
        std::fs::write(dir.join(file), source).expect("write module file");
    }
    dir
}

fn conform_run(dir: &PathBuf) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_lupin"))
        .current_dir(dir)
        .args(["conform-run", "main.lu", "--json"])
        .output()
        .expect("lupin runs");
    let text = String::from_utf8(out.stdout).expect("utf-8 record");
    let line = text.lines().last().expect("one record");
    serde_json::from_str(line).expect("a JSON observation record")
}

#[test]
fn a_sibling_files_declaration_wins_the_ambient_call() {
    // The F-0047 shape at its real address: a std module declares
    // `read_line` in one file and calls it from another. The ambient
    // `read_line` (`str ! {eof, io, utf8}`) must be unreachable by that
    // spelling from anywhere in the module; recursing into it, or typing the
    // call against its row, is the bug the clause was written for.
    let dir = module(
        "sibling",
        &[
            (
                "main.lu",
                "fn main() -> !int {\n    print(read_line())\n    0\n}\n",
            ),
            (
                "other.lu",
                "//! member: true\nfn read_line() -> str {\n    \"from the sibling\"\n}\n",
            ),
        ],
    );
    let record = conform_run(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(record["verdict"], "exit(0)", "{record}");
    assert_eq!(record["stdout_inline"], "from the sibling\n", "{record}");
    assert_eq!(record["warnings"][0]["code"], "W0304", "{record}");
    assert_eq!(record["warnings"][0]["span"], serde_json::json!([20, 29]));
}

#[test]
fn assert_is_a_primitive_a_declaration_cannot_shadow() {
    // `[conf.trap.assert]`'s carve-out: `fn assert` is declared (and warned
    // about, W0304 at the name) but the call still reaches the intrinsic,
    // so the declaration's body never runs and nothing prints.
    let dir = module(
        "assert",
        &[(
            "main.lu",
            "fn assert(b: bool) {\n    print(\"mine\")\n}\n\
             fn main() -> !int {\n    assert(true)\n    0\n}\n",
        )],
    );
    let record = conform_run(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(record["verdict"], "exit(0)", "{record}");
    assert!(record["stdout_inline"].is_null(), "{record}");
    assert_eq!(record["warnings"][0]["code"], "W0304", "{record}");
    assert_eq!(record["warnings"][0]["span"], serde_json::json!([3, 9]));
}

#[test]
fn a_declared_host_name_wins_over_the_ambient_one() {
    // A capability name rather than a library one: `time_now_ms` declared in
    // the module answers the module's own `42`, never the host clock.
    let dir = module(
        "host",
        &[(
            "main.lu",
            "fn time_now_ms() -> int {\n    42\n}\n\
             fn main() -> !int {\n    print(\"{time_now_ms()}\")\n    0\n}\n",
        )],
    );
    let record = conform_run(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(record["verdict"], "exit(0)", "{record}");
    assert_eq!(record["stdout_inline"], "42\n", "{record}");
    assert_eq!(record["warnings"][0]["span"], serde_json::json!([3, 14]));
}

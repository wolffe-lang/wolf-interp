//! [os.random] at wolf v0.2.34: OS entropy, including under --seed.
//! The three fixtures are byte-for-byte copies of that tag's corpus/os.
use std::path::Path;
use std::process::Command;

fn observe(path: &Path, seeded: bool) -> serde_json::Value {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lupin"));
    cmd.arg("conform-run").arg(path).arg("--json");
    if seeded {
        cmd.arg("--seed=42");
    }
    let out = cmd.output().expect("observe");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("not a record: {out:?}"))
}

#[test]
fn canonical_random_rows_match_the_checked_machine() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/os_random_is80");
    for seeded in [false, true] {
        for (name, verdict, stdout) in [
            (
                "random_differs",
                "exit(0)",
                Some("len=32/32 range=true same=false\n"),
            ),
            (
                "random_edges",
                "exit(0)",
                Some("z=0 big=65536 range=true\n"),
            ),
            ("random_negative_trap", "trap(assert)", None),
        ] {
            let r = observe(&root.join(format!("{name}.lu")), seeded);
            assert_eq!(r["verdict"], verdict, "{name}, seeded={seeded}: {r}");
            assert_eq!(r["stdout_inline"].as_str(), stdout, "{r}");
        }
    }
}

#[test]
fn a_schedule_seed_never_seeds_key_material() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("os_random_is80_seed");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.lu");
    std::fs::write(
        &path,
        "fn main() -> !int { let b = os_random(32); for x in b { print(\"{x}\") }; 0 }\n",
    )
    .unwrap();
    let a = observe(&path, true);
    let b = observe(&path, true);
    assert_eq!(a["verdict"], "exit(0)", "{a}");
    assert_eq!(b["verdict"], "exit(0)", "{b}");
    assert_eq!(a["seeded"], true);
    assert_eq!(b["seeded"], true);
    assert_ne!(
        a["stdout_inline"], b["stdout_inline"],
        "[os.random.sole]: not a seeded PRNG"
    );
}

#[test]
fn the_name_is_a_first_class_builtin() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("os_random_is80_alias");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.lu");
    std::fs::write(
        &path,
        "fn main() -> !int { let draw = os_random; let b = draw(16); print(\"{b.len}\"); 0 }\n",
    )
    .unwrap();
    let r = observe(&path, false);
    assert_eq!(r["verdict"], "exit(0)", "{r}");
    assert_eq!(r["stdout_inline"], "16\n");
}

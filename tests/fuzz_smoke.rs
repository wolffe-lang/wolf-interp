//! Totality smoke: the frontend must not panic or hang on anything.
//!
//! The acceptance bar is "60s of garbage bytes and token soup: no panics, no
//! hangs — rejects are fine, crashes are bugs". A wall-clock budget is a bad CI
//! citizen (three OSes, wildly different runners), so this runs a **fixed,
//! deterministic** corpus of the same shapes instead: same inputs on every
//! machine, same failures, and a seed printed on the way in so a crash is
//! reproducible from the test output alone.
//!
//! Three generators, in rising order of how much they resemble wolf:
//!
//! 1. uniform random bytes — the lexer's UTF-8 and error paths;
//! 2. token soup — random draws from the real token vocabulary, which is the
//!    only way to reach deep parser states that random bytes never will;
//! 3. corpus mutations — byte flips, truncations and splices of real programs,
//!    which is where the interesting near-miss inputs live.
//!
//! Every case must terminate with either a tree or one diagnostic. Nothing here
//! asserts *which*: this is a liveness test, not a conformance one.
//!
//! # is02: the run rung is in scope too
//!
//! > Fuzz smoke over generated Tier-0 programs: no interpreter panics; all
//! > terminations are verdicts.
//!
//! `exercise` now drives the *whole* ladder, evaluator included, because "an
//! interpreter-internal Rust panic is by definition an interpreter bug" and the
//! cheapest place to find one is here. Two properties are asserted at that rung
//! and nowhere else:
//!
//! - **every termination is a verdict.** `[proto.record.verdict]` enumerates
//!   six; a fuzzed program must land on one of them, never on a panic and never
//!   on a hang (`Machine::FUEL` bounds the third case).
//! - **a trap is always one of the closed eleven kinds** (`[conf.trap.set]`).
//!   The vocabulary is the comparison alphabet of spec/06; a kind outside it
//!   would make a differential report meaningless.

use std::path::{Path, PathBuf};

use wolf_interp::frontend;
use wolf_interp::phase::Phase;

/// xorshift64*, so the corpus is identical on every platform and needs no
/// dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// The seed. Printed by every test so a failure is reproducible by hand.
const SEED: u64 = 0x0100_1F43_A57E_ED01;
const CASES: usize = 3000;

/// Runs the whole frontend and throws the answer away. Returning `()` is the
/// point: the assertion is that this returns at all.
fn exercise(source: &[u8]) {
    for rung in [
        Some(Phase::Lex),
        Some(Phase::Parse),
        Some(Phase::Resolve),
        None,
    ] {
        let observation = frontend::observe(source, rung);
        // `[proto.record.verdict]` is a closed set of six shapes, and a fuzzed
        // program must land on one. All six are reachable from is04 on: a
        // mutation that plants an `unsafe` block and a raw pointer can and does
        // reach the oracle, and when it does the verdict has to cite a
        // well-formed anchor rather than an ad-hoc string.
        match &observation.verdict {
            wolf_interp::protocol::Verdict::Trap(kind) => {
                assert!(
                    wolf_interp::trap::TrapKind::ALL.contains(kind),
                    "a trap outside the closed vocabulary: {kind}"
                );
            }
            wolf_interp::protocol::Verdict::Ub(anchor) => {
                assert!(
                    wolf_interp::anchor::classify(anchor).is_ok(),
                    "a UB verdict must cite a well-formed anchor: ub({anchor})"
                );
                let finding = observation
                    .ub
                    .as_ref()
                    .expect("a ub verdict carries its row");
                assert!(
                    !finding.row.optimization().is_empty(),
                    "`[mem.ub.closed]`: zero rows without a named licensed optimization"
                );
            }
            _ => {}
        }
        // Whatever happened, the record must be well-formed: a `fail` carries
        // exactly one diagnostic, everything else carries none.
        match &observation.verdict {
            wolf_interp::protocol::Verdict::Fail(code) => {
                assert_eq!(
                    observation.diagnostics.len(),
                    1,
                    "no recovery, one diagnostic"
                );
                assert_eq!(&observation.diagnostics[0].code, code);
                let [start, end] = observation.diagnostics[0].span;
                assert!(start <= end, "spans are half-open and ordered");
                assert!(
                    end as usize <= source.len(),
                    "a span must point inside the source: {start}..{end} of {}",
                    source.len()
                );
            }
            _ => assert!(observation.diagnostics.is_empty()),
        }
    }
}

#[test]
fn random_bytes_never_crash_the_frontend() {
    println!("seed = {SEED:#x}");
    let mut rng = Rng(SEED);
    for _ in 0..CASES {
        let len = rng.below(256);
        let bytes: Vec<u8> = (0..len).map(|_| (rng.next() & 0xff) as u8).collect();
        exercise(&bytes);
    }
}

#[test]
fn random_bytes_biased_towards_the_syntax_never_crash() {
    // Uniform bytes almost never produce a quote or a brace in a useful place;
    // this alphabet is weighted so they do.
    println!("seed = {SEED:#x}");
    const ALPHABET: &[u8] = b"{}[]()\"\"\"''\\rn#.,:;=!?^|&<>+-*/% \n\tabxyz019_";
    let mut rng = Rng(SEED ^ 0xA5A5_A5A5);
    for _ in 0..CASES {
        let len = rng.below(200);
        let bytes: Vec<u8> = (0..len).map(|_| *rng.pick(ALPHABET)).collect();
        exercise(&bytes);
    }
}

#[test]
fn token_soup_never_crashes_the_parser() {
    println!("seed = {SEED:#x}");
    // Real tokens, in no order at all — the only generator that reliably drives
    // the parser deep enough to matter.
    const VOCABULARY: &[&str] = &[
        "fn",
        "let",
        "var",
        "const",
        "if",
        "else",
        "match",
        "for",
        "while",
        "loop",
        "return",
        "break",
        "continue",
        "struct",
        "enum",
        "trait",
        "impl",
        "use",
        "import",
        "pub",
        "type",
        "region",
        "in",
        "freeze",
        "scope",
        "spawn",
        "proc",
        "select",
        "when",
        "unsafe",
        "asm",
        "assume",
        "borrow",
        "move",
        "copy",
        "shared",
        "handle",
        "weak",
        "distinct",
        "dyn",
        "mut",
        "take",
        "as",
        "defer",
        "errdefer",
        "extern",
        "export",
        "comptime",
        "true",
        "false",
        "x",
        "y",
        "Point",
        "self",
        "c",
        "rc",
        "pool",
        "from",
        "timeout",
        "noalias",
        "_",
        "0",
        "1",
        "42",
        "1.0",
        "0xff",
        "1_000",
        "\"s\"",
        "\"{x}\"",
        "\"{x:>8}\"",
        "r#\"raw\"#",
        "re\"[a-z]\"",
        "(",
        ")",
        "[",
        "]",
        "{",
        "}",
        "#[",
        ";",
        ",",
        ".",
        "..",
        "..=",
        ":",
        "->",
        "=>",
        "?",
        "=",
        "+",
        "-",
        "*",
        "/",
        "%",
        "==",
        "!=",
        "<",
        ">",
        "<=",
        ">=",
        "<=>",
        "&&",
        "||",
        "&",
        "|",
        "^",
        "!",
        "@",
        "\n",
        " ",
    ];
    let mut rng = Rng(SEED ^ 0x5EED_0001);
    for _ in 0..CASES {
        let count = rng.below(60);
        let mut source = String::new();
        for _ in 0..count {
            source.push_str(rng.pick(VOCABULARY));
            source.push(' ');
        }
        exercise(source.as_bytes());
    }
}

#[test]
fn mutated_corpus_files_never_crash() {
    println!("seed = {SEED:#x}");
    let files = corpus_files();
    assert!(!files.is_empty());
    let mut rng = Rng(SEED ^ 0xC0FF_EE00);
    for _ in 0..CASES {
        let original = rng.pick(&files).clone();
        if original.is_empty() {
            continue;
        }
        let mut bytes = original;
        match rng.below(4) {
            // A flipped byte.
            0 => {
                let at = rng.below(bytes.len());
                bytes[at] ^= (rng.next() & 0xff) as u8;
            }
            // A truncation — every one of these ends mid-production.
            1 => {
                let at = rng.below(bytes.len());
                bytes.truncate(at);
            }
            // A deleted run: unbalanced delimiters, orphaned arms.
            2 => {
                let at = rng.below(bytes.len());
                let len = rng.below(bytes.len() - at).min(32);
                bytes.drain(at..at + len);
            }
            // A spliced-in fragment from the token vocabulary.
            _ => {
                let at = rng.below(bytes.len());
                let insert = *rng.pick(&[
                    &b"\"{"[..],
                    &b"}\""[..],
                    &b"r#\""[..],
                    &b"\"\"\""[..],
                    &b"#["[..],
                    &b";;"[..],
                    &b"=>"[..],
                ]);
                bytes.splice(at..at, insert.iter().copied());
            }
        }
        exercise(&bytes);
    }
}

#[test]
fn pathological_nesting_terminates() {
    // Deep nesting is where a recursive-descent parser goes to die. This test
    // is why `[gram.lex.rails]` exists: is01 ran it, found an unrailed parser
    // meets the stack instead of the user, and filed the gap; the amendment
    // made expression/statement recursion depth **256** normative alongside the
    // lexer's depth-32 string rail (`[gram.lex.str]`, E0108). The job here is
    // still to prove the *frontend answers* — the rail's exact value is
    // `tests/spec_extract.rs`'s business, which reads it out of the document.
    for depth in [8usize, 64, 512] {
        let nested_strings: String = {
            let mut out = String::from("x");
            for _ in 0..depth {
                out = format!("\"{{{out}}}\"");
            }
            format!("fn f() -> int {{ let s = {out}\n0 }}\n")
        };
        exercise(nested_strings.as_bytes());

        let brackets = format!(
            "fn f() -> int {{ let v = {}1{}\n0 }}\n",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        exercise(brackets.as_bytes());

        let unclosed = format!("fn f() -> int {{ {}", "(".repeat(depth));
        exercise(unclosed.as_bytes());
    }
}

fn corpus_files() -> Vec<Vec<u8>> {
    fn walk(dir: &Path, out: &mut Vec<Vec<u8>>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .expect("readable")
            .flatten()
            .collect();
        entries.sort_by_key(std::fs::DirEntry::path);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "lu") {
                out.push(std::fs::read(&path).expect("readable"));
            }
        }
    }
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus");
    let mut out = Vec::new();
    walk(&root, &mut out);
    out
}

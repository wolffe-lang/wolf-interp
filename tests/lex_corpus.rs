//! The `lex` rung over the pinned corpus.
//!
//! Every `.lu` file in the corpus is well-formed *lexically* — including the
//! `corpus/grammar/` counter-examples, whose ledger `phase:` is `lex`
//! precisely because they tokenize and then die at parse. A lexer diagnostic
//! anywhere in this walk is either a bug here or a spec divergence; both are
//! worth stopping the build for.
//!
//! The exceptions are the corpus's own lexical counter-examples, and they are
//! not listed here. A file whose ledger says `phase: none` **is** the
//! exemption — `phase: none` means no rung completed, and the shallowest rung
//! is `lex`, so the file is refused BY the lexer and its `check: fail(CODE)`
//! names the code the corpus pins. wolf-interp#58: this walk and the
//! workflow's `conform-run rungs` step each kept a hand-written list of those
//! files, and every by-design lex refusal that arrived with a pin reddened CI
//! twice before someone remembered to grow both. Both readers now derive the
//! set from the directive header, which is the corpus's own statement of it.

use std::path::{Path, PathBuf};

use wolf_interp::directive::{self, Check};
use wolf_interp::lex;
use wolf_interp::phase::Phase;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
}

fn lu_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .flatten()
            .collect();
        entries.sort_by_key(std::fs::DirEntry::path);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "lu") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&corpus_root(), &mut out);
    out
}

/// The code this file's own ledger says the LEXER refuses it with, if any.
///
/// `phase: none` is the exemption (`[conf.directive]`'s ladder starts at
/// `lex`, so "no rung completed" means the lexer stopped it), and the
/// accompanying `check: fail(CODE)` is the code the corpus pins. Every other
/// file must lex clean. Nothing is listed by name: the corpus states it, both
/// readers derive it (wolf-interp#58).
fn pinned_lex_refusal(source: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(source).ok()?;
    let directives = directive::parse_header(text).ok()?;
    if directives.phase != Some(Phase::None) {
        return None;
    }
    match directives.check {
        Some(Check::Fail(code)) => Some(code),
        _ => None,
    }
}

#[test]
fn every_corpus_file_tokenizes_without_a_lex_diagnostic() {
    let mut failures = Vec::new();
    let mut refused = Vec::new();
    for path in lu_files() {
        let relative = path
            .strip_prefix(corpus_root())
            .expect("under the corpus root")
            .to_string_lossy()
            .replace('\\', "/");
        let source = std::fs::read(&path).expect("readable");
        let expected = pinned_lex_refusal(&source);
        let lexed = lex::lex_bytes(&source);
        match (lexed.first_error(), expected.as_deref()) {
            (Some(first), Some(code)) => {
                assert_eq!(first.code, code, "{relative}");
                refused.push(relative);
            }
            (Some(first), None) => failures.push(format!("  {}: {first}", path.display())),
            (None, Some(code)) => {
                failures.push(format!("  {relative}: pins {code} and lexed clean"));
            }
            (None, None) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "the pinned corpus must lex clean:\n{}",
        failures.join("\n")
    );
    // The derivation is only load-bearing while the corpus still carries such
    // a file; if the class empties, this walk has quietly stopped testing the
    // thing it exists for and should be re-read rather than left green.
    assert!(
        !refused.is_empty(),
        "the corpus carries no `phase: none` entry — the lexical counter-example class is gone"
    );
}

#[test]
fn no_corpus_file_needs_deep_interpolation_nesting() {
    // The depth-8 limit is generous by construction; if the corpus ever needs
    // more, `[gram.lex.str]` has a problem and this test is where we hear about
    // it first.
    for path in lu_files() {
        let source = std::fs::read_to_string(&path).expect("utf-8");
        let lexed = lex::lex(&source);
        assert!(
            lexed.deferred.is_empty(),
            "{} tripped a deferred lex diagnostic: {:?}",
            path.display(),
            lexed.deferred
        );
        assert!(lexed.max_str_depth <= 2, "{}", path.display());
    }
}

// ---------------------------------------------------------------------------
// `[gram.lex.shebang]` (s53, pin `f8dca42`)
//
// The corpus witness `grammar/shebang.lu` pins the positive half: a `#!` line
// at byte zero is trivia and the file underneath means what it always meant.
// The clause's *other* half — "and at no other offset" — has no corpus file,
// because a corpus file that carried a second `#!` would be a lex failure by
// construction. Its own header says so: "A `#!` anywhere else in a file is
// still the stray-byte error it always was, which is why this file has
// exactly one." These pin the half the corpus cannot.
// ---------------------------------------------------------------------------

#[test]
fn a_shebang_at_byte_zero_is_trivia() {
    let lexed = lex::lex("#!/usr/bin/env -S wolf run\nfn main() -> !int { 0 }\n");
    assert!(
        lexed.first_error().is_none(),
        "a shebang at offset 0 is trivia: {:?}",
        lexed.first_error()
    );
    // Trivia produces no token, so the first one is the `fn` keyword — and its
    // span starts after the shebang line, which is what keeps every span in an
    // executable script honest.
    let first = lexed.tokens.first().expect("a token");
    assert_eq!(first.span.start, 27, "the shebang line is consumed whole");
}

#[test]
fn a_shebang_is_only_a_shebang_at_byte_zero() {
    // One line down: `#` begins no token, so it is E0101 exactly as before.
    let lexed = lex::lex("\n#!/usr/bin/env wolf\nfn main() -> !int { 0 }\n");
    let first = lexed
        .first_error()
        .expect("a stray `#` off byte zero errors");
    assert_eq!(first.code, wolf_interp::diag::E_UNEXPECTED_BYTE);

    // Mid-file, after real code, likewise.
    let lexed = lex::lex("fn main() -> !int { 0 }\n#!/usr/bin/env wolf\n");
    assert_eq!(
        lexed
            .first_error()
            .expect("a stray `#` mid-file errors")
            .code,
        wolf_interp::diag::E_UNEXPECTED_BYTE
    );

    // Not even indented by one space: "byte offset 0" is the whole domain.
    let lexed = lex::lex(" #!/usr/bin/env wolf\nfn main() -> !int { 0 }\n");
    assert_eq!(
        lexed.first_error().expect("an indented `#!` errors").code,
        wolf_interp::diag::E_UNEXPECTED_BYTE
    );
}

#[test]
fn a_byte_order_mark_pushes_the_shebang_off_byte_zero() {
    // The BOM is rejected on its own account (`[gram.lex.source]`), and it
    // also means the `#!` now starts at offset 3 — so it is NOT a shebang,
    // and the stray `#` is a second diagnostic rather than a swallowed line.
    let lexed = lex::lex("\u{feff}#!/usr/bin/env wolf\nfn main() -> !int { 0 }\n");
    let codes: Vec<&str> = lexed.errors.iter().map(|d| d.code).collect();
    assert_eq!(
        codes.first().copied(),
        Some(wolf_interp::diag::E_BYTE_ORDER_MARK)
    );
    assert!(
        codes.contains(&wolf_interp::diag::E_UNEXPECTED_BYTE),
        "the `#!` after a BOM is not at byte zero, so it stays a stray byte: {codes:?}"
    );
}

#[test]
fn a_shebang_only_file_is_empty_and_a_shebang_needs_no_trailing_newline() {
    let lexed = lex::lex("#!/usr/bin/env wolf");
    assert!(lexed.first_error().is_none());
    assert!(
        lexed.tokens.is_empty(),
        "a file that is only a shebang has no tokens: {:?}",
        lexed.tokens
    );
    // And the `\n` is left for the newline machinery rather than eaten, so a
    // shebang inserts no terminator of its own before the first real token.
    let lexed = lex::lex("#!/usr/bin/env wolf\nfn main() -> !int { 0 }\n");
    assert!(
        !matches!(
            lexed.tokens.first().map(|t| &t.tok),
            Some(lex::Tok::Term { .. })
        ),
        "no terminator is inserted before the first token: {:?}",
        lexed.tokens.first()
    );
}

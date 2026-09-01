//! The `lex` rung over the pinned corpus.
//!
//! Every `.lu` file in the corpus is well-formed *lexically* — including the
//! `corpus/grammar/` counter-examples, whose ledger `phase:` is `lex`
//! precisely because they tokenize and then die at parse. A lexer diagnostic
//! anywhere in this walk is either a bug here or a spec divergence; both are
//! worth stopping the build for.
//!
//! The one exception is the corpus's own lexical counter-example, listed by
//! name below: a file whose ledger `phase:` is `none` is refused BY the
//! lexer, and its diagnostic is the thing the corpus pins. It arrived at
//! the e6cf24e pin with r04's `[gram.lex.char]` amendment (wolf-lang#189).

use std::path::{Path, PathBuf};

use wolf_interp::lex;

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

/// The corpus entries whose ledger `phase:` is `none` — the ones the LEXER
/// refuses, with the code the corpus pins. Every other file must lex clean.
const LEXICAL_COUNTER_EXAMPLES: &[(&str, &str)] = &[("grammar/char_uni_seven_digits.lu", "E0101")];

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
        let expected = LEXICAL_COUNTER_EXAMPLES
            .iter()
            .find(|(file, _)| *file == relative)
            .map(|(_, code)| *code);
        let source = std::fs::read(&path).expect("readable");
        let lexed = lex::lex_bytes(&source);
        match (lexed.first_error(), expected) {
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
    assert_eq!(
        refused.len(),
        LEXICAL_COUNTER_EXAMPLES.len(),
        "a listed lexical counter-example left the corpus"
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

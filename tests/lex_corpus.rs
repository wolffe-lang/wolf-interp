//! The `lex` rung over the pinned corpus.
//!
//! Every `.lu` file in the corpus is well-formed *lexically* — including the
//! four `corpus/grammar/` counter-examples, whose ledger `phase:` is `lex`
//! precisely because they tokenize and then die at parse. A lexer diagnostic
//! anywhere in this walk is either a bug here or a spec divergence; both are
//! worth stopping the build for.

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

#[test]
fn every_corpus_file_tokenizes_without_a_lex_diagnostic() {
    let mut failures = Vec::new();
    for path in lu_files() {
        let source = std::fs::read(&path).expect("readable");
        let lexed = lex::lex_bytes(&source);
        if let Some(first) = lexed.first_error() {
            failures.push(format!("  {}: {first}", path.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "the pinned corpus must lex clean:\n{}",
        failures.join("\n")
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

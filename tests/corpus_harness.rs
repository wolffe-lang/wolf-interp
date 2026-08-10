//! The corpus harness over the pinned corpus.
//!
//! Green here means: every `//!` header in `upstream/corpus` is legible to
//! this implementation's own directive parser. It is a weak claim about wolf
//! and a strong claim about the two tracks agreeing on the directive grammar —
//! which is the only claim is00 is entitled to make.

use std::path::{Path, PathBuf};

use wolf_interp::anchor;
use wolf_interp::corpus::{self, CorpusReport, Outcome};

fn upstream() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(wolf_interp::upstream_root())
}

fn report() -> CorpusReport {
    let root = upstream().join("corpus");
    corpus::walk(&root, Some(&upstream().join("spec"))).unwrap_or_else(|e| {
        panic!(
            "could not walk {}: {e}\n\
             hint: the corpus is a pinned submodule — run `git submodule update --init upstream`",
            root.display()
        )
    })
}

#[test]
fn every_header_in_the_pinned_corpus_parses() {
    let report = report();
    let failures = report.failures();
    assert!(
        failures.is_empty(),
        "unparseable corpus headers:\n{}",
        failures
            .iter()
            .map(|(path, reason)| format!("  {path}: {reason}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_walk_is_green() {
    assert!(report().is_green());
}

#[test]
fn the_pin_holds_the_corpus_we_think_it_does() {
    // A pin bump that changes the corpus size is a deliberate act; this test
    // makes it deliberate rather than silent (s01's progress-ledger rule).
    //
    // Ledger of deliberate bumps:
    //   4a002aa → 74 files (is01: typecheck tier added 5)
    //   28ab5c9 → 91 files (is02: s14's `corpus/traits/` added 17 — 12 entries
    //             and 5 members across three module directories; no other tier
    //             moved, and no existing file changed).
    //   bd41920 → 103 files (is03: s15's `corpus/rows/` added 12 — 9 entries
    //             and 3 members, the members being `rows/propagate/`'s three
    //             module files. One existing file changed content without
    //             changing the count: `grammar/bang_errunion.lu`. s16's
    //             `corpus/comptime/` and `corpus/faults/` tiers had NOT landed
    //             at this pin, so no dedupe against `tests/faults/` was owed —
    //             see that directory's README.)
    //   ecea37c → 128 files (is04: s16's `corpus/comptime/` added 19 entries
    //             and `corpus/faults/` added 6 — the six fault-class programs
    //             this repo authored at is03 and upstreamed. Per
    //             `tests/faults/README.md`, the vendored copies are now the
    //             source of truth and the local twins of those six are gone;
    //             the local directory keeps only the region/handle programs the
    //             corpus still has no counterpart for. Two existing files
    //             changed content without changing the count: `regions.lu`
    //             gained `build_config` and `comptime.lu` was rewritten. All
    //             new files are entries, so the member count is unmoved.)
    //   8b04edf → 149 files (is05: s17's sema completion. `corpus/typecheck/`
    //             grew 16 (14 entries plus `typecheck/method_scope/`'s two
    //             members alongside its entry `main.lu`), `corpus/grammar/`
    //             added `receiver_moded.lu` + `intdot_exponent.lu`,
    //             `corpus/memory/` added `view_set_norm.lu`, and
    //             `corpus/rows/` added `open_row_growth.lu` +
    //             `negative/open_into_closed.lu`. One existing file changed
    //             content without changing the count: `regions.lu`'s tail now
    //             uses only specified semantics — `config.limit == 42` under
    //             `[mem.region.freeze.1]`'s "frozen data readable forever" —
    //             which is the change that finally made it RUN here.)
    let report = report();
    assert_eq!(
        report.total(),
        149,
        "corpus size changed — was the pin bumped?"
    );
    assert_eq!(report.entries() + report.members(), report.total());
    assert_eq!(report.entries(), 133);
    assert_eq!(report.members(), 16);
}

#[test]
fn entries_carry_check_and_phase_and_members_carry_neither() {
    for file in &report().files {
        match &file.outcome {
            Outcome::Entry(directives) => {
                assert!(directives.check.is_some(), "{} has no check:", file.path);
                assert!(directives.phase.is_some(), "{} has no phase:", file.path);
                assert!(directives.is_entry());
            }
            Outcome::Member(directives) => {
                assert!(
                    directives.check.is_none(),
                    "{} is a member with check:",
                    file.path
                );
                assert!(
                    directives.phase.is_none(),
                    "{} is a member with phase:",
                    file.path
                );
                assert!(!directives.is_entry());
            }
            Outcome::Failed(reason) => panic!("{}: {reason}", file.path),
        }
    }
}

#[test]
fn member_files_live_beside_an_entry_file() {
    // `member: true` means "compiled through this directory's entry file"
    // (directory = module). A member with no entry anywhere up its package
    // would be unreachable — nothing would ever exercise it.
    let report = report();
    let entry_dirs: Vec<&str> = report
        .files
        .iter()
        .filter(|f| matches!(f.outcome, Outcome::Entry(_)))
        .map(|f| f.path.rsplit_once('/').map_or("", |(dir, _)| dir))
        .collect();

    for file in &report.files {
        if !matches!(file.outcome, Outcome::Member(_)) {
            continue;
        }
        let dir = file.path.rsplit_once('/').map_or("", |(dir, _)| dir);
        let reachable = entry_dirs
            .iter()
            .any(|entry_dir| dir == *entry_dir || dir.starts_with(&format!("{entry_dir}/")));
        assert!(
            reachable,
            "{} is a member with no entry file in or above its directory",
            file.path
        );
    }
}

#[test]
fn every_conforms_tag_is_a_well_formed_anchor() {
    for file in &report().files {
        let Some(directives) = file.directives() else {
            continue;
        };
        for tag in &directives.conforms {
            anchor::classify(tag).unwrap_or_else(|e| panic!("{}: {e}", file.path));
        }
    }
}

#[test]
fn registered_namespace_tags_resolve_against_the_pinned_anchors_json() {
    let report = report();
    assert!(
        report.anchors_checked,
        "upstream/spec/anchors.json was not read — is the submodule sparse-checkout missing spec/?"
    );
    assert!(
        report.unknown_anchors.is_empty(),
        "corpus cites anchors absent from spec/anchors.json: {:?}",
        report.unknown_anchors
    );
}

#[test]
fn the_walk_is_deterministic_and_sorted() {
    // `read_dir` order is platform noise; the harness must not inherit it.
    let first = report();
    let paths: Vec<&str> = first.files.iter().map(|f| f.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted);

    let again: Vec<String> = report().files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths, again);
}

#[test]
fn paths_never_leak_platform_separators() {
    for file in &report().files {
        assert!(
            !file.path.contains('\\'),
            "{} carries a raw separator",
            file.path
        );
    }
}

/// The vendored snapshot must be byte-identical to the submodule at its
/// pin — verified whenever the submodule is initialized (locally always;
/// CI has no submodule and skips).
#[test]
fn vendor_matches_submodule() {
    use std::path::Path;
    if !Path::new("upstream/corpus").is_dir() {
        eprintln!("submodule absent; vendored snapshot not cross-checked here");
        return;
    }
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    for tree in ["spec", "corpus"] {
        let mut live = Vec::new();
        walk(Path::new("upstream").join(tree).as_path(), &mut live);
        for lp in live {
            let rel = lp.strip_prefix("upstream").unwrap();
            let vp = Path::new("vendor/upstream").join(rel);
            let a = std::fs::read(&lp).unwrap();
            let b = std::fs::read(&vp)
                .unwrap_or_else(|_| panic!("vendored file missing: {}", vp.display()));
            assert_eq!(a, b, "vendor drift: {}", rel.display());
        }
    }
}

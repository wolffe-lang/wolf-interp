//! Fault output, snapshot-tested per program.
//!
//! > Every trap identity has a corpus program producing it; fault output
//! > (spans + clause anchor) snapshot-tested per program.
//!
//! The programs live in `tests/faults/` in the corpus's own directive dialect
//! (see that directory's README for why they are not in `upstream/corpus`).
//! Each snapshot renders the trap the way a human reads it — kind, message,
//! clause anchor, and **both** spans with the source they cover — because the
//! sprint's ownership faults are specified to print two: the fault site and
//! the move/conflict that caused it.
//!
//! The spans are rendered as `line:col` plus the covered text rather than as
//! byte offsets, deliberately: a snapshot full of raw offsets churns on every
//! whitespace edit and tells the reviewer nothing. Byte-exactness is still
//! asserted — the covered text is sliced out of the source with those offsets,
//! so a wrong offset produces wrong text.

use std::path::{Path, PathBuf};

use wolf_interp::directive::{Check, ExitSpec};
use wolf_interp::eval::Trap;
use wolf_interp::eval::rules::Rule;
use wolf_interp::frontend;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn faults_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("faults")
}

/// The corpus's own fault tier, which arrived with pin `ecea37c`.
///
/// Six of the programs this repo authored at is03 were upstreamed into
/// `corpus/faults/`, and `tests/faults/README.md`'s dedupe rule then applies:
/// **the vendored copies are the source of truth** and the local twins retire.
/// They are still snapshot-tested — from here — because retiring the file is
/// not the same as retiring the evidence.
fn corpus_faults_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("corpus")
        .join("faults")
}

fn programs_in(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "lu"))
        .map(|path| {
            let name = path
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned();
            (name, std::fs::read_to_string(&path).expect("readable"))
        })
        .collect()
}

fn programs() -> Vec<(String, String)> {
    let mut out = programs_in(&faults_dir());
    out.extend(programs_in(&corpus_faults_dir()));
    out.sort();
    // The dedupe is a property, not an accident: a program that exists in both
    // places would be snapshot-tested twice under one name and the second run
    // would silently overwrite the first.
    let mut names: Vec<&str> = out.iter().map(|(name, _)| name.as_str()).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(
        before,
        names.len(),
        "a fault program exists both locally and upstream; the vendored copy is the source of          truth (tests/faults/README.md)"
    );
    out
}

/// `line:col` (1-based, columns in characters) for a byte offset.
fn position(source: &str, offset: usize) -> String {
    let before = &source[..offset.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before, |(_, tail)| tail)
        .chars()
        .count()
        + 1;
    format!("{line}:{column}")
}

fn render(name: &str, source: &str, trap: &Trap) -> String {
    let slice = |span: wolf_interp::diag::Span| {
        source
            .get(span.start..span.end)
            .unwrap_or("<span outside the source>")
            .to_owned()
    };
    let mut out = String::new();
    out.push_str(&format!("program: {name}\n"));
    out.push_str(&format!("trap:    {}\n", trap.kind));
    out.push_str(&format!("clause:  [{}]\n", trap.rule.anchor()));
    out.push_str(&format!("rule:    {:?}\n", trap.rule));
    out.push_str(&format!("message: {}\n", trap.message));
    out.push_str(&format!(
        "at:      {} `{}`\n",
        position(source, trap.span.start),
        slice(trap.span)
    ));
    match &trap.secondary {
        Some((span, note)) => out.push_str(&format!(
            "because: {} `{}` — {note}\n",
            position(source, span.start),
            slice(*span)
        )),
        None => out.push_str("because: (single-span fault)\n"),
    }
    out
}

#[test]
fn every_fault_program_renders_its_trap() {
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    let _guard = settings.bind_to_scope();

    for (name, source) in programs() {
        let observation = frontend::observe(source.as_bytes(), None);
        let trap = match &observation.trap {
            Some(trap) => trap,
            None => panic!(
                "{name}: expected a trap, observed {} at {}",
                observation.verdict, observation.phase_reached
            ),
        };
        let stem = name.trim_end_matches(".lu").to_owned();
        insta::assert_snapshot!(stem, render(&name, &source, trap));
    }
}

#[test]
fn each_program_produces_the_trap_its_own_directive_pins() {
    // The programs are written in the corpus's directive dialect so they are
    // upstream-ready; that dialect is also what makes them self-checking.
    for (name, source) in programs() {
        let directives =
            wolf_interp::directive::parse_header(&source).unwrap_or_else(|e| panic!("{name}: {e}"));
        let Some(Check::Run {
            exit: ExitSpec::Trap(Some(want)),
            ..
        }) = directives.check
        else {
            panic!("{name}: a fault program pins `run(exit=trap(kind))`");
        };
        let observation = frontend::observe(source.as_bytes(), None);
        assert_eq!(observation.verdict, Verdict::Trap(want), "{name}");
        assert_eq!(observation.phase_reached, Phase::Run, "{name}");
    }
}

#[test]
fn there_is_one_program_per_trap_identity_this_tier_can_raise() {
    // `[conf.trap.set]` is closed at twelve kinds. **Eight** are reachable at
    // is03 — the dynamic region machine added `region-fault` and
    // `stale-handle`; the other three belong to tiers this sprint does not
    // implement, and claiming them would be the same lie as an inflated
    // `phase_reached`.
    let reachable: std::collections::BTreeSet<TrapKind> = programs()
        .iter()
        .filter_map(|(_, source)| {
            frontend::observe(source.as_bytes(), None)
                .trap
                .map(|t| t.kind)
        })
        .collect();
    assert_eq!(
        reachable,
        std::collections::BTreeSet::from([
            TrapKind::Overflow,
            TrapKind::DivZero,
            TrapKind::Bounds,
            TrapKind::UseAfterMove,
            TrapKind::Exclusivity,
            TrapKind::RegionFault,
            TrapKind::StaleHandle,
            TrapKind::Assert,
        ])
    );

    // `ub` left this list at is04, but it left it *sideways*: the oracle's
    // finding is the verdict `ub(anchor)`, not a `Trap`, so no program here
    // produces `TrapKind::Ub` and none should. The kind stays in the closed
    // vocabulary for the checked build (`[conf.trap.map]`), and
    // `tests/ub_coverage.rs` is where its programs live.
    // `deadlock` — the deliberate twelfth kind, `[conc.deadlock.trap]` —
    // needs a scheduler, so its programs live in `tests/conc_machine.rs`,
    // not this single-task tier.
    let deferred = [
        TrapKind::AllocContract,
        TrapKind::Race,
        TrapKind::Ub,
        TrapKind::Deadlock,
    ];
    assert_eq!(
        reachable.len() + deferred.len(),
        TrapKind::ALL.len(),
        "the trap vocabulary moved; it is closed and requires a spec revision"
    );
    for kind in deferred {
        assert!(
            !reachable.contains(&kind),
            "{kind} is not a trap identity these programs raise"
        );
    }
}

#[test]
fn every_ownership_fault_prints_two_spans() {
    // "any later read faults `[mem.tier0.use-after-move]` with the move site
    // and the use site both spanned"; "Any overlapping conflicting access
    // faults `[mem.tier0.exclusivity]`, both spans printed."
    //
    // The promise belongs to the **rule**, not to the trap kind: is03's
    // two-phase fault reports `use-after-move` for a slot that was reserved
    // and never initialized, and there is no second site to point at — no
    // move happened. Listing the rules that owe a second span keeps the claim
    // exact instead of approximately right.
    let owes_two_spans = [Rule::UseAfterMove, Rule::Exclusivity, Rule::PathDisjoint];
    for (name, source) in programs() {
        let observation = frontend::observe(source.as_bytes(), None);
        let trap = observation.trap.expect("a trap");
        if owes_two_spans.contains(&trap.rule) {
            assert!(
                trap.secondary.is_some(),
                "{name}: {:?} owes a second span",
                trap.rule
            );
        }
        if let Some((span, _)) = trap.secondary {
            assert!(span.start < source.len(), "{name}");
            assert!(span.start <= span.end, "{name}");
        }
    }
}

#[test]
fn the_use_after_free_fault_points_at_the_region_that_died() {
    // `[mem.region.intra.2]`'s detection is exact — region id plus generation —
    // and the message has to say *which* region, at *which* creation site, or
    // the exactness is invisible to the reader.
    let source = std::fs::read_to_string(faults_dir().join("region_uaf.lu")).expect("readable");
    let trap = frontend::observe(source.as_bytes(), None)
        .trap
        .expect("a trap");
    assert_eq!(trap.kind, TrapKind::RegionFault);
    assert_eq!(trap.rule.anchor(), "mem.region.intra.2");
    assert!(trap.message.contains("freed wholesale"), "{}", trap.message);
}

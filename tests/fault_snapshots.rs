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
use wolf_interp::frontend;
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::trap::TrapKind;

fn faults_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("faults")
}

fn programs() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(faults_dir())
        .expect("the fault programs are readable")
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
        .collect();
    out.sort();
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
    // `[conf.trap.set]` is closed at eleven kinds. Six are reachable here; the
    // other five belong to tiers this sprint does not implement, and claiming
    // them would be the same lie as an inflated `phase_reached`.
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
            TrapKind::Assert,
        ])
    );

    let deferred = [
        TrapKind::RegionFault,
        TrapKind::StaleHandle,
        TrapKind::AllocContract,
        TrapKind::Race,
        TrapKind::Ub,
    ];
    assert_eq!(
        reachable.len() + deferred.len(),
        TrapKind::ALL.len(),
        "the trap vocabulary moved; it is closed and requires a spec revision"
    );
    for kind in deferred {
        assert!(!reachable.contains(&kind), "{kind} is not an is02 identity");
    }
}

#[test]
fn every_ownership_fault_prints_two_spans() {
    // "any later read faults `[mem.tier0.use-after-move]` with the move site
    // and the use site both spanned"; "Any overlapping conflicting access
    // faults `[mem.tier0.exclusivity]`, both spans printed."
    for (name, source) in programs() {
        let observation = frontend::observe(source.as_bytes(), None);
        let trap = observation.trap.expect("a trap");
        let two_spans_required =
            matches!(trap.kind, TrapKind::UseAfterMove | TrapKind::Exclusivity);
        assert_eq!(
            trap.secondary.is_some(),
            two_spans_required,
            "{name}: {} spans",
            trap.kind
        );
        if let Some((span, _)) = trap.secondary {
            assert!(span.start < source.len(), "{name}");
            assert!(span.start <= span.end, "{name}");
        }
    }
}

//! The rule registry against the pinned spec — is02's clause-citation gate.
//!
//! > Rule-registry test proves 100% anchor coverage against the pinned s04
//! > draft; a planted anchorless rule demonstrably fails CI.
//!
//! Three claims, each a test:
//!
//! 1. every rule the evaluator implements cites a clause anchor, and every
//!    anchor in a *registered* namespace exists in the pinned
//!    `spec/anchors.json` (`[conf.tag.valid]`);
//! 2. every anchor in a *reserved forward* namespace is legal and counted, so
//!    the debt is visible rather than disguised (`[conf.anchor.ns]`);
//! 3. a planted rule with no anchor fails the same validator — the negative
//!    control, without which claim 1 proves nothing.
//!
//! Plus a fourth that the sprint implies but does not spell: a rule nobody
//! fires is not a rule. The `--trace` log over the corpus is walked to check
//! the registry against the evaluator's *behaviour*, not just its source.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use wolf_interp::anchor::{self, Namespace};
use wolf_interp::eval::rules::{self, Row, RowError, Rule};

fn upstream() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(wolf_interp::upstream_root())
}

/// Every anchor the pinned `spec/anchors.json` publishes.
fn pinned_anchors() -> BTreeSet<String> {
    let text = std::fs::read_to_string(upstream().join("spec").join("anchors.json"))
        .expect("the pinned anchor index must be readable");
    let value: serde_json::Value = serde_json::from_str(&text).expect("anchors.json is JSON");
    value["anchors"]
        .as_object()
        .expect("the registry is an object")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn every_implemented_rule_cites_a_clause_the_pinned_spec_defines() {
    let known = pinned_anchors();
    let errors = rules::validate(&rules::registry(), Some(&known));
    assert!(
        errors.is_empty(),
        "the rule registry and the pinned spec disagree:\n{}",
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_registry_is_one_hundred_percent_covered() {
    // "100% anchor coverage of implemented rules", stated as a number so a
    // silently-dropped citation is visible rather than merely absent.
    let known = pinned_anchors();
    let rows = rules::registry();
    let cited = rows
        .iter()
        .filter(|row| !row.anchor.is_empty())
        .filter(|row| match anchor::classify(row.anchor) {
            Ok(Namespace::Registered) => known.contains(row.anchor),
            Ok(Namespace::Reserved) => true,
            Err(_) => false,
        })
        .count();
    assert_eq!(
        cited,
        rows.len(),
        "{cited}/{} rules cite a clause",
        rows.len()
    );
    assert_eq!(rows.len(), Rule::ALL.len());
    // is02 registered 36 rules; is03 added 23 for `spec/02` §3 and §4 plus
    // `[mem.model.order]`. The floor moves up with each sprint, never down.
    assert!(rows.len() >= 71, "the registry lost rules: {}", rows.len());
}

#[test]
fn forward_namespace_citations_are_named_so_the_debt_is_visible() {
    // `[conf.anchor.ns]`: tags in a reserved forward namespace are legal and
    // "reported as *forward*". The documents that will own `arith`, `err` and
    // `str` are not written; when they are, these rows become checkable and
    // this list is the work item.
    let forward: Vec<&'static str> = rules::registry()
        .iter()
        .filter(|row| anchor::classify(row.anchor) == Ok(Namespace::Reserved))
        .map(|row| row.anchor)
        .collect();
    let unique: BTreeSet<&str> = forward.iter().copied().collect();
    assert_eq!(
        unique,
        BTreeSet::from([
            "arith.checked",
            "arith.literal.default",
            "arith.wrapping",
            "err.else",
            "err.errdefer",
            "err.propagate",
            "err.rows",
            "err.union",
            "str.interp",
            // is26, transitional: `Rule::CharCast` cites the closed cast
            // set's forward tag until the pin bump lands the s121 spec,
            // whose registered `[type.char.cast]` then owns the rule — the
            // same commit that vendors the anchor flips the citation.
            "ty.cast.closed-set",
            // is08: is06's `sync.when.*` forward citations are retired —
            // the s20 S-batch wrote `[conc.when.*]` into spec/03 and the
            // rules cite the registered namespace now (findings S-1..S-8
            // resolved; see docs/divergence-log.md).
        ]),
        "the forward-namespace debt list moved; if a document now owns one of \
         these, the anchor should be checkable"
    );
}

#[test]
fn a_planted_anchorless_rule_fails_the_same_gate() {
    // The negative control. Without it, "every rule cites a clause" could be
    // true because the validator never says no.
    let known = pinned_anchors();
    let planted = [
        Row {
            rule: Rule::Move,
            anchor: "",
            description: "a rule that forgot to cite anything",
        },
        Row {
            rule: Rule::Exclusivity,
            anchor: "mem.tier0.excl.99",
            description: "a clause nobody published",
        },
        Row {
            rule: Rule::Flow,
            anchor: "made.up.namespace",
            description: "a namespace outside [conf.anchor.ns]",
        },
    ];
    let errors = rules::validate(&planted, Some(&known));
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(matches!(errors[0], RowError::NoAnchor(_)));
    assert!(matches!(errors[1], RowError::UnknownAnchor { .. }));
    assert!(matches!(errors[2], RowError::BadAnchor { .. }));
}

#[test]
fn the_memory_model_rules_cite_the_clause_that_states_them() {
    // Spot-checks that read like the document, so a copy-paste slip between two
    // neighbouring anchors is caught rather than merely "an anchor".
    let expected: &[(Rule, &str)] = &[
        (Rule::Move, "mem.tier0.move.1"),
        (Rule::UseAfterMove, "mem.tier0.move.2"),
        (Rule::Copy, "mem.tier0.move.3"),
        (Rule::Reinit, "mem.tier0.move.4"),
        (Rule::ModeRead, "mem.tier0.mode.read"),
        (Rule::ModeMut, "mem.tier0.mode.mut"),
        (Rule::ModeTake, "mem.tier0.mode.take"),
        (Rule::Exclusivity, "mem.tier0.excl.1"),
        (Rule::ExclusivityDisjoint, "mem.tier0.excl.2"),
        (Rule::ViewSet, "mem.tier0.excl.3"),
        (Rule::PathDisjoint, "mem.model.path.disjoint"),
        (Rule::Borrow, "mem.tier0.borrow.1"),
        (Rule::BorrowExtent, "mem.tier0.borrow.2"),
        (Rule::DivZero, "mem.ub.defined"),
        (Rule::Bounds, "mem.ub.defined"),
        // is03: `spec/02` §3, clause by clause.
        (Rule::RegionCreate, "mem.region.create.1"),
        (Rule::RegionAffine, "mem.region.create.2"),
        (Rule::RegionAmbient, "mem.region.create.3"),
        (Rule::RegionIdentity, "mem.region.create.4"),
        (Rule::RegionIntra, "mem.region.intra.1"),
        (Rule::RegionFree, "mem.region.intra.2"),
        (Rule::RegionEdge, "mem.region.edge"),
        (Rule::RegionEdgeIso, "mem.region.edge.iso"),
        (Rule::RegionEdgeImm, "mem.region.edge.imm"),
        (Rule::RegionOpen, "mem.region.open.1"),
        (Rule::RegionMultiopen, "mem.region.multiopen"),
        (Rule::RegionSuspended, "mem.region.open.3"),
        (Rule::RegionFreeze, "mem.region.freeze.1"),
        (Rule::RegionTransfer, "mem.region.freeze.2"),
        (Rule::RegionClosedSubtree, "mem.region.freeze.3"),
        // is03: `spec/02` §4.
        (Rule::SharedRc, "mem.shared.rc.1"),
        (Rule::SharedAcyclic, "mem.shared.rc.2"),
        (Rule::SharedWeak, "mem.shared.rc.3"),
        (Rule::SharedDrop, "mem.shared.drop.3"),
        (Rule::HandleTwoPhase, "mem.shared.handle.1"),
        (Rule::HandleStale, "mem.shared.handle.2"),
        (Rule::HandleAccess, "mem.shared.handle.3"),
        // The evaluation-order clause the pin bump published.
        (Rule::EvalStrictOrder, "mem.model.order"),
        // is04: `spec/02` §5-§7, the unsafe tier.
        (Rule::UnsafeRaw, "mem.unsafe.raw.1"),
        (Rule::AssumeNoalias, "mem.unsafe.raw.2"),
        (Rule::UnsafeDoor, "mem.unsafe.door"),
        (Rule::UnsafeScope, "mem.unsafe.scope"),
        (Rule::BoundaryFfi, "mem.boundary.ffi"),
        (Rule::ProvTag, "mem.prov.tag"),
        (Rule::ProvState, "mem.prov.state"),
        (Rule::ProvExpose, "mem.prov.expose"),
        (Rule::ProvRegion, "mem.prov.region"),
        (Rule::Ub, "mem.ub"),
        (Rule::UbLicensed, "mem.ub.closed"),
        (Rule::UbVerdict, "proto.record.ub"),
    ];
    for (rule, anchor) in expected {
        assert_eq!(rule.anchor(), *anchor, "{rule:?}");
    }
}

#[test]
fn the_trace_namespaces_are_derived_from_the_anchors_not_from_a_list() {
    // is03 established the contract for `--trace=mem` and is04 extends it to
    // `--trace=prov`: both filters are predicates over the *anchor*, so a rule
    // that joins one of those clause families is traced by construction and the
    // filter cannot drift away from the registry.
    for row in rules::registry() {
        assert_eq!(
            row.rule.is_memory(),
            row.anchor.starts_with("mem."),
            "{:?} cites `{}`",
            row.rule,
            row.anchor
        );
        let tier3 = row.anchor.starts_with("mem.prov")
            || row.anchor.starts_with("mem.unsafe")
            || row.anchor.starts_with("mem.ub")
            || row.anchor.starts_with("mem.boundary");
        assert_eq!(
            row.rule.is_provenance(),
            tier3,
            "{:?} cites `{}`",
            row.rule,
            row.anchor
        );
        // Tier 3 is a *subset* of the memory model, never a sibling of it.
        assert!(!row.rule.is_provenance() || row.rule.is_memory());
    }

    // The registry actually populates both namespaces, so the assertions above
    // are not vacuously true.
    assert!(
        rules::registry()
            .iter()
            .filter(|r| r.rule.is_memory())
            .count()
            > 30
    );
    assert!(
        rules::registry()
            .iter()
            .filter(|r| r.rule.is_provenance())
            .count()
            >= 11
    );
}

#[test]
fn every_cited_memory_clause_actually_appears_in_the_document() {
    // `anchors.json` is generated; the document is the source. Checking the
    // markdown too means a stale index cannot make a bad citation look fine.
    let document = std::fs::read_to_string(upstream().join("spec").join("02-memory-model.md"))
        .expect("the pinned memory model must be readable");
    for row in rules::registry() {
        if !row.anchor.starts_with("mem.") {
            continue;
        }
        assert!(
            document.contains(&format!("[{}]", row.anchor)),
            "{:?} cites `{}`, which spec/02 does not contain",
            row.rule,
            row.anchor
        );
    }
}

#[test]
fn the_trace_only_ever_names_registered_rules() {
    // The behavioural half: run every corpus file that evaluates, with
    // `--trace` on, and check that every line names a rule from the registry
    // with its anchor attached. A fault site citing something the registry does
    // not know would show up here and nowhere else.
    let root = upstream().join("corpus");
    let report = wolf_interp::corpus::walk(&root, None).expect("walkable");
    let anchors: BTreeSet<&str> = rules::registry().iter().map(|row| row.anchor).collect();
    let names: BTreeSet<String> = Rule::ALL.iter().map(|r| format!("{r:?}")).collect();

    let mut fired: BTreeSet<String> = BTreeSet::new();
    for file in &report.files {
        if !matches!(file.outcome, wolf_interp::corpus::Outcome::Entry(_)) {
            continue;
        }
        let full = root.join(&file.path);
        let source = std::fs::read(&full).expect("readable");
        let (_, observed) =
            wolf_interp::observe_record_traced(&full, &source, None, wolf_interp::eval::Trace::All);
        for line in observed.trace {
            // `<start>..<end> <Rule> [<anchor>] <detail>`
            let rule = line
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_owned();
            let anchor = line
                .split('[')
                .nth(1)
                .and_then(|rest| rest.split(']').next())
                .unwrap_or_default();
            assert!(
                names.contains(&rule),
                "{}: unknown rule in trace: {line}",
                file.path
            );
            assert!(
                anchors.contains(anchor),
                "{}: unregistered anchor in trace: {line}",
                file.path
            );
            fired.insert(rule);
        }
    }

    // The corpus does not exercise every rule — local borrows and view sets have
    // no runnable litmus yet, and the rules that *only* appear on a fault path
    // are carried on the `Trap` rather than in the trace — but it should
    // exercise most, and is03's region machine moved that number a long way.
    assert!(
        fired.len() >= 35,
        "only {} of {} rules ever fired over the corpus: {fired:?}",
        fired.len(),
        Rule::ALL.len()
    );

    // Specifically: the Tier-1 rules now fire over the *pinned* corpus, which
    // is the behavioural half of "the region machine is implemented".
    for rule in [
        "RegionCreate",
        "RegionAffine",
        "RegionAmbient",
        "RegionOpen",
        "RegionMultiopen",
        "RegionSuspended",
        "RegionFree",
        "RegionFreeze",
        "RegionEdgeIso",
        "HandleTwoPhase",
        "SharedRc",
    ] {
        assert!(
            fired.contains(rule),
            "{rule} never fired over the pinned corpus"
        );
    }
}

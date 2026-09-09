//! `[conf.anchor.ns.admit]`, read BOTH ways — is39, the mirror of the gate
//! wolf-lang's s139 landed on its own side (#246).
//!
//! The clause binds every implementation track: "that same change appends the
//! namespace to the registered list above AND to the registered set of the
//! anchor tooling on every implementation track". Nothing here read the
//! clause's list until now — `anchor::REGISTERED_NAMESPACES` was a hand-copy,
//! kept honest by whoever bumped the pin noticing. That is exactly the defect
//! #120, #239 and #246 each name, three times over, and it is silent in both
//! directions:
//!
//! - **permissive** (#239, this machine's own side of it): the tooling admits
//!   a namespace the clause has not registered, so it goes on publishing and
//!   passing CI. `diag`, `ct`, `type` and `os` sat here for four pins;
//!   v0.2.6's `[conf.anchor.ns]` amendment is the clause catching up.
//! - **restrictive** (#246): the clause registers a namespace the tooling does
//!   not, and the tooling rejects a tag that is legal upstream.
//! - and the **quiet** one s139 found: a document declares anchors in a
//!   namespace neither side registers, so nothing publishes them and no gate
//!   anywhere holds an opinion.
//!
//! Four assertions close all three: the clause's registered list and this
//! machine's list are the same set BOTH WAYS; every registered namespace
//! actually publishes anchors in the pinned `anchors.json`; every namespace
//! `anchors.json` publishes is registered here.
//!
//! ## The `sched` deferral, and how it ended (kept, because it is the record)
//!
//! wolf-lang s139 ruled `spec/07-schedule-points.md` NORMATIVE and admitted
//! `sched` (anchors 424 → 431). That landed at `ed8f526`, **one merge after
//! the `v0.2.6` tag is39 pinned**, so at pin `398e5f5` the clause registered
//! eleven namespaces, `anchors.json` published zero `sched.*`, and this
//! machine's eleven were exactly right. Registering a twelfth THERE would
//! have put this side back on the permissive half of the same silence, which
//! is the thing the clause was amended to stop.
//!
//! There was nothing to reject yet either, and the proof was in the pinned
//! corpus: `test/conc_schedules_test.lu` named `[sched.stable]` **in prose**,
//! because a `conforms:` tag citing it was a CI failure while the anchor went
//! unpublished — F-0099's shape one namespace over. wolf-lang `60e0bd2`
//! promoted that comment to a real `conforms: … sched.stable` tag, and *that*
//! is the tag this machine would have rejected. It arrived with `ed8f526`,
//! and `v0.2.8` — the pin this file now reads — is past it: the corpus file
//! in `vendor/upstream/` carries the tag today.
//!
//! So the admission was deferred to the pin that carries it. is40 takes that
//! pin (`v0.2.8`, `5c729e8`): `anchors.json` publishes seven `sched.*`, the clause
//! registers twelve namespaces, and `anchor::REGISTERED_NAMESPACES` grew
//! `sched` in the SAME change, which is the whole of what
//! `[conf.anchor.ns.admit]` asks. The two gates that named the gap —
//! `the_clause_s_registered_list_is_this_machine_s_list` and
//! `every_published_anchor_sits_in_a_namespace_this_machine_registers` — went
//! red on the pin bump alone and are green again on the admission;
//! `the_planted_post_s139_clause_is_now_covered` keeps the planted clause and
//! flips its assertion, so the control still exercises the parser against a
//! body that is not the pinned file's.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use wolf_interp::anchor::{REGISTERED_NAMESPACES, RESERVED_NAMESPACES};

fn upstream() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(wolf_interp::upstream_root())
}

fn clause_text() -> String {
    std::fs::read_to_string(upstream().join("spec").join("05-conformance.md"))
        .expect("the pinned spec/05-conformance.md must be readable")
}

/// The backticked token at each `` `ns` → owner `` in `text`.
///
/// `split('`')` alternates outside/inside: odd indices are the spans between
/// backticks, and the chunk that follows one is what decides whether it is a
/// registration or prose. An arrow is the registration.
fn owners(text: &str) -> BTreeSet<String> {
    let parts: Vec<&str> = text.split('`').collect();
    parts
        .iter()
        .enumerate()
        .filter(|(at, _)| at % 2 == 1)
        .filter(|(at, token)| {
            !token.is_empty()
                && token.chars().all(|c| c.is_ascii_lowercase())
                && parts
                    .get(at + 1)
                    .is_some_and(|after| after.trim_start().starts_with('→'))
        })
        .map(|(_, token)| (*token).to_owned())
        .collect()
}

/// Every backticked all-lowercase token in `text`.
fn tokens(text: &str) -> BTreeSet<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_lowercase()))
        .map(ToOwned::to_owned)
        .collect()
}

/// The `[conf.anchor.ns]` bullet, split at the reserved-list heading.
fn clause_lists(text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let bullet = text
        .split_once("- `[conf.anchor.ns]`")
        .expect("the pinned spec must carry a `[conf.anchor.ns]` bullet")
        .1;
    let (registered_half, reserved_half) = bullet
        .split_once("**Reserved forward namespaces**")
        .expect("the bullet must carry the reserved-namespace heading");
    let reserved_half = reserved_half
        .split_once("*forward*):")
        .expect("the reserved list must follow the `*forward*):` parenthetical")
        .1
        .split_once("A tag outside")
        .expect("the reserved list must end at the `A tag outside` sentence")
        .0;
    (owners(registered_half), tokens(reserved_half))
}

/// The s139 clause text, verbatim from wolf-lang `ed8f526` — the pin AFTER
/// the one this repository takes. Shared *data*, exactly like the corpus, and
/// here so the deferral below is checkable before the pin moves.
const POST_S139_CLAUSE: &str = "\
- `[conf.anchor.ns]` Registered namespaces and owners:
  `gram` → 01-grammar.md · `diag` → 01-grammar.md (§9) ·
  `mem` → 02-memory-model.md · `conc` → 03-concurrency.md ·
  `abi` → 04-abi.md · `conf` → 05-conformance.md ·
  `proto` → 06-differential-protocol.md ·
  `sched` → 07-schedule-points.md · `pkg` → 08-package.md ·
  `ct` → 09-constant-time.md · `type` → 10-types.md ·
  `os` → 11-os.md.
  **Reserved forward namespaces** (owned by spec documents not yet
  written; tags in them are legal, reported as *forward*): `str`, `err`,
  `task`, `proc`, `sync`, `generics`, `arith`, `ffi`, `unsafe`,
  `comptime`, `perf`, `mod`, `std`, `ty`, `test`. A tag outside all
  registered and reserved namespaces is a CI failure.
";

fn pinned_namespaces() -> BTreeSet<String> {
    let text = std::fs::read_to_string(upstream().join("spec").join("anchors.json"))
        .expect("the pinned anchor index must be readable");
    let value: serde_json::Value = serde_json::from_str(&text).expect("anchors.json is JSON");
    value["anchors"]
        .as_object()
        .expect("the registry is an object")
        .keys()
        .map(|k| k.split('.').next().unwrap_or_default().to_owned())
        .collect()
}

fn ours() -> BTreeSet<String> {
    REGISTERED_NAMESPACES
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

#[test]
fn the_parse_finds_the_clause_rather_than_silently_finding_nothing() {
    // The negative control the whole file rests on: a parser that matched
    // nothing would make every assertion below vacuously true, which is
    // #246's own defect wearing a test's clothes.
    let (registered, reserved) = clause_lists(&clause_text());
    assert!(
        registered.len() >= 11,
        "the registered list parsed to {registered:?} — the clause's shape moved"
    );
    assert!(registered.contains("gram") && registered.contains("mem"));
    assert!(
        reserved.len() >= 15,
        "the reserved list parsed to {reserved:?} — the clause's shape moved"
    );
    assert!(reserved.contains("str") && reserved.contains("test"));
}

#[test]
fn the_clause_s_registered_list_is_this_machine_s_list() {
    let (clause, _) = clause_lists(&clause_text());
    let ours = ours();
    let missing_here: Vec<_> = clause.difference(&ours).collect();
    let extra_here: Vec<_> = ours.difference(&clause).collect();
    assert!(
        missing_here.is_empty(),
        "`[conf.anchor.ns.admit]`: the pinned clause registers {missing_here:?} and \
         `anchor::REGISTERED_NAMESPACES` does not — this machine now REJECTS a tag that \
         is legal upstream. Append them (and the length in the type)."
    );
    assert!(
        extra_here.is_empty(),
        "`[conf.anchor.ns.admit]`: `anchor::REGISTERED_NAMESPACES` admits {extra_here:?} and \
         the pinned clause does not — the permissive half of the same silence (#239). Either \
         the clause is behind and the gap is a filing, or the entry is wrong."
    );
    assert_eq!(clause.len(), REGISTERED_NAMESPACES.len());
}

#[test]
fn the_clause_s_reserved_list_is_carried_here_plus_this_repository_s_own() {
    let (_, clause) = clause_lists(&clause_text());
    let ours: BTreeSet<String> = RESERVED_NAMESPACES
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let missing_here: Vec<_> = clause.difference(&ours).collect();
    assert!(
        missing_here.is_empty(),
        "the pinned clause reserves {missing_here:?} and `anchor::RESERVED_NAMESPACES` does not"
    );
    // The one extra is this repository's own: `repl` owns `docs/repl.md`,
    // whose incremental-definition rules are deliberately NOT spec clauses
    // (is08/is09). Any OTHER extra is an invention and must be justified here.
    let extra_here: Vec<&String> = ours.difference(&clause).collect();
    assert_eq!(
        extra_here,
        vec![&"repl".to_owned()],
        "an unexplained reserved namespace: {extra_here:?}"
    );
}

#[test]
fn every_registered_namespace_publishes_anchors_at_this_pin() {
    // The forward half of the both-ways read: a registered namespace whose
    // document publishes nothing is a document nothing extracts (#246's
    // 07-schedule-points.md, seven anchors for a year).
    let published = pinned_namespaces();
    let barren: Vec<&&str> = REGISTERED_NAMESPACES
        .iter()
        .filter(|ns| !published.contains(**ns))
        .collect();
    assert!(
        barren.is_empty(),
        "registered but publishing NO anchor in the pinned anchors.json: {barren:?} — either \
         the namespace is reserved, not registered, or the extractor is not reading its document"
    );
}

#[test]
fn every_published_anchor_sits_in_a_namespace_this_machine_registers() {
    // The reverse half: `[conf.tag.valid]` makes citing an anchor whose
    // namespace this side does not register a hard error, so a namespace the
    // registry publishes and this list omits is a tag rejected here and legal
    // upstream. This is the assertion the `sched` admission will trip.
    let ours = ours();
    let orphans: Vec<String> = pinned_namespaces()
        .into_iter()
        .filter(|ns| !ours.contains(ns))
        .collect();
    assert!(
        orphans.is_empty(),
        "the pinned anchors.json publishes {orphans:?}, which `anchor::REGISTERED_NAMESPACES` \
         does not register — `[conf.anchor.ns.admit]`, restrictive half. Append them and the \
         length in the type."
    );
}

#[test]
fn the_planted_post_s139_clause_is_now_covered() {
    // The deferral's teeth, now spent — and kept, because a control that is
    // deleted the moment it goes green proves nothing about the next one.
    // `sched` was admitted at wolf-lang `ed8f526`, one merge after the
    // `v0.2.6` tag is39 pinned; is40 takes `v0.2.8`, which carries it, so
    // the gap this test named is CLOSED and the assertion is the flip the
    // is39 comment promised: nothing was deleted, the direction reversed.
    // The planted text stays verbatim so the parser is still exercised
    // against a clause body that is not the pinned file's.
    let (clause, _) = clause_lists(POST_S139_CLAUSE);
    assert!(
        clause.contains("sched"),
        "the planted clause parsed to {clause:?}"
    );
    let ours = ours();
    let gap: Vec<&String> = clause.difference(&ours).collect();
    assert!(
        gap.is_empty(),
        "the s139 clause registers {gap:?} and this machine does not — the pin that \
         carries the admission is taken, so the gap must be closed, not deferred"
    );
}

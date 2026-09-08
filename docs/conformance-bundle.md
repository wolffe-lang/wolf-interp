# The conformance bundle: schema version 1 (frozen)

The publishable form of the wolf conformance suite (is09). It is a directory
that maps **spec clause ↔ tests ↔ expected observables**, attested by the
reference interpreter. The directory is self-contained and its bytes are
deterministic; the sections below say what buys each property.

It is a `[proto]` **extension**. Everything on the wire inside it is plain
spec/06: protocol-1 observation records, `[proto.cmp]` comparison semantics,
the closed `[conf.trap.set]` vocabulary. The packaging is what lets a third
machine hold one implementation against another without building either's
test harness. Schema version 1 is **frozen**: any change to the layout or the
manifest contract below is version 2, never a quiet edit.

Produced by `lupin conformance export`. Consumed by
`lupin conformance check <bundle> --impl <cmd>`, or by any tool that
reimplements this document. Publication is a tarball of this directory on a
GitHub release. There is no hosted infrastructure.

## Layout

```
MANIFEST.json            identity, counts, per-file sha256, root hash
README.md                the two-paragraph orientation
corpus/**                the pinned wolf-lang corpus, whole tree
                         (directive headers per [conf.directive.*];
                         includes corpus/protocol/, the protocol's own
                         fixture records, [proto.harness.fixtures])
suite/ub/**              is04's [mem.ub] triggers + near-miss twins
suite/faults/**          is03's fault litmuses + defined twins
suite/witness/**         is07's model-check witness programs
expected/records.jsonl   one protocol-1 observation record per entry
                         program (the REFERENCE OUTCOMES)
vocab/traps.json         [conf.trap.set]: the closed twelve, in order
vocab/ub-rows.json       [mem.ub]: the closed eleven, each with its D2
                         licensed optimization and coverage status
anchors/anchors.json     the pinned clause-anchor registry, verbatim
coverage/matrix.jsonl    one record per registered anchor
                         ([conf.cover.format] shape + x- extensions)
coverage/coverage.md     the rendered honesty document: per-document
                         percentages, the UB section, the full debt list
docs/repl.md             the is08 [repl.*] notes, riding along
```

## The programs

Every `.lu` file is in the corpus directive dialect: `check:` states the
expected observable in exactly the corpus vocabulary (`pass`,
`fail(CODE)`, `run(exit=N|trap|trap(kind) [, stdout="…"])`), `phase:`
the deepest compiler rung, `conforms:` the clause anchors the file is
evidence for, `member: true` the files exercised only through their
module's entry. There is **no second expectation language**: the bundle
adds nothing to the directive grammar.

`corpus/**` is byte-identical to the upstream pin named in the manifest
(after newline normalization, below). `suite/**` are the interpreter's
own upstream-ready litmuses. They carry the same headers and are
conform-run the same way.

## The reference outcomes

`expected/records.jsonl` holds one spec/06 observation record per entry
program (members are never conform-run directly), sorted by `file`,
observed **by the reference interpreter, from inside the bundle**. A
multi-file module resolves its members from the bundled tree, which is
what proves the bundle self-contained. The `file` field is the
bundle-relative slash path (`corpus/hello.lu`), never an exporter-local
one. Records are unseeded (`"seeded": false`, the strict-FIFO default
schedule). Every schedule question the suite asks is closed separately
by the is07 exploration record.

A consumer compares an implementation against these records with the
spec/06 deep comparison exactly as the is05 differ does: rung-by-rung
claims, the conservatism ledger for `unsupported` and accept-set
boundaries, `[proto.cmp.severity]` ordering. The counterparty is invoked
per `[proto.invoke]`: `<cmd> conform-run <file> --json`.
Interpreter-only observables (`ub(anchor)`) compare per
`[proto.record.ub]`.

## The manifest

`MANIFEST.json`, pretty-printed JSON with sorted keys, LF line endings:

| key | meaning |
|---|---|
| `bundle_schema` | `1`, this document |
| `protocol` | `1`, the spec/06 record version inside |
| `impl`, `impl_version`, `impl_commit` | the attesting exporter |
| `pin` | the upstream wolf-lang commit `corpus/` + `anchors/` are at |
| `style_version` | the corpus formatter's style version (s13 finding: corpus bytes are formatter-canonical; a style bump is expected churn, made visible here) |
| `counts` | `files` (excluding the manifest), `programs`, `records` |
| `coverage` | `anchors_total`, `anchors_covered`, `forward_tags` |
| `files` | bundle-relative path → sha256, every file except this one |
| `bundle_sha256` | sha256 of the sorted `"<hash>  <path>\n"` listing (`sha256sum` shape). **The one number two exports compare.** |

Attestation: the exporter refuses to emit any record its own spec/06
schema validator rejects, and the exporting commit rides `impl_commit`.
A suite that is not green under the reference interpreter does not
publish. (Package identity is deliberately absent: upstream package
naming is a stub until s51, and the attestation must not lean on it.)

## Determinism (the I10 spirit)

Re-export at the same (interpreter, pin) commits is byte-identical,
across runs *and* across linux/macOS/windows. The rules that buy this,
all normative for schema 1:

- **Newlines**: every bundled file is CRLF→LF normalized **before**
  observation, so recorded byte-offset spans index the bytes that ship
  and a `core.autocrlf` checkout cannot move a hash.
- **Paths**: `/`-separated everywhere, in manifest keys, record `file`
  fields, and matrix entries.
- **Order**: directory walks and JSON maps sort by the relative slash
  path; records and matrix lines sort by their key field.
- **Integrity**: a consumer verifies every per-file hash and the root
  hash before comparing anything; a mismatch is a refusal (exit 2),
  never a verdict.

CI enforces this three ways: re-export twice and byte-compare; check the
bundle against its own replayed records (the consumption dry-run, which
is the pull → verify → diff path with recorded results standing in for a
second implementation); and hash-compare the manifests exported on the
three tier-1 OSes.

## The counterparty CI cannot check out (wolf-lang#253, is39)

wolf-lang **#253** names this from the compiler's side: no CI job on
either track checks out a sibling, so **the one gate that compares the
two implementations is the one gate neither CI can run.** Both
`PAIRING` tables — wolf-lang's and this repository's release notes —
are hand-measured on a developer's box and go stale silently between
measurements.

**This side of it, named exactly.** The gate is `lupin diff-run`, and
it wants a `wolf` binary. Three things stand between CI and one:

1. `upstream/` is a **private** submodule and the checkout step
   deliberately does not initialize it (deploy keys are disabled
   org-side; `vendor/README.md`);
2. the vendored snapshot carries `spec/` and `corpus/` **only** — never
   `crates/`, because the independence doctrine forbids reading them —
   so even with the tree there is nothing to build;
3. building it would be a second toolchain build inside a CI that is
   already a three-host matrix.

So `.github/workflows/ci.yml`'s differential job asserts the **skip**:
that `diff-run` declines loudly with no counterparty, and that
`--require-counterparty` hard-fails. CI's only claim about the
differential runner is that it correctly refuses to run.

**It can be closed here without a second toolchain build, and the door
is already open.** `lupin conformance check <bundle> --replay <RECORDS>`
consumes a JSONL of `conform-run` records in place of a live
`--impl <CMD>` — the pull → verify → diff path with recorded results
standing in for a second implementation, which the determinism section
above already describes. Today CI feeds it **this machine's own**
`expected/records.jsonl`, which is a tautology, and
`differ::retired_waivers` knows it: a self-replay passes an empty
compared-list precisely so that no waiver can be retired by this
machine agreeing with itself.

Feed the same door the **compiler's** records and the same job becomes
a real cross-implementation gate for the price of one file read. The
producing side is free — wolf-lang's CI already builds `wolf` and
already runs `conform-run` over the pinned corpus — and the transport
is the one `spec/` and `corpus/` already use: the lane that bumps the
pin has the private submodule and a local `cargo build -p wolf_driver`,
and drops `records.jsonl` into `vendor/upstream/` beside `PIN`. No
private clone in CI, no second toolchain, no new secret.

Three limits, each with the mechanization that keeps it honest:

- **Staleness.** A recorded counterparty is a snapshot and can go
  inert. Bind it: refuse the lane when the records' pin is not
  `vendor/upstream/PIN`, so a pin bump forces a re-record exactly as it
  already forces a re-vendor — and `retired_waivers` runs against a
  non-empty compared-list in CI for the first time.
- **Host posture.** The os/net/fs tier's records are host-dependent by
  design (`[os.net.accept]` names three reactor hosts; `unsupported`
  rows differ per OS), so one recorded set replayed on three runners
  would forge divergences. Record per host — wolf-lang's matrix is
  already three-host, which makes this three artifacts and not three
  builds — or scope the replay lane to the host-independent rungs
  (`lex`, `parse`, `resolve`), where most filed divergences live.
- **Scope.** It closes the regression half, not the exploratory half: a
  recorded counterparty answers only the programs it was recorded over,
  so `diff-explore` and the fuzz campaign stay a developer-box lane.
  That boundary is the point. CI asserts nothing about agreement today;
  this makes it assert agreement over the pinned corpus, which is the
  artifact both `PAIRING` tables are actually made of.

## Coverage: the honesty document

`coverage/matrix.jsonl` carries one line per registered anchor in the
`[conf.cover.format]` shape, `{"clause", "tests", "status":
"covered"|"debt", "commit"}`, plus `x-doc` (owning document) and
`x-cited-by` (the citing programs with their `check:` expectations).
`coverage/coverage.md` renders the same data for humans: per-document
percentages ranked by chapter weight, the **full** debt list, the
forward (reserved-namespace) tags, and a dedicated `[mem.ub]` section in
which every row is detected-and-paired or carries its named reason
(D2: an untested UB item is an unlicensed optimization). The covered
count is ratcheted in the exporter's own CI
(`tests/export.rs::coverage_is_ratcheted`): it may grow, never shrink.

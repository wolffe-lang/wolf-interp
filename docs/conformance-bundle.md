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

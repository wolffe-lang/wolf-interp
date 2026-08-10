# Contributing to wolf-interp

## Orientation

What lives where: `src/` is the implementation (frontend, sema-lite, the
evaluator with its region and provenance machines, the differential tools);
`tests/` is the suite that makes it defensible; `upstream/` and
`vendor/upstream/` are the pinned spec and corpus, consumed as data;
`docs/` holds the engineering documents and the user manual
(`docs/README.md` is the index). The package is `wolf-interp`; the binary
it builds is named `lupin` (is12). The independence doctrine in two
sentences: this repository shares no code with the wolf compiler, and the
two implementations meet only through the pinned spec, the corpus, and the
observation protocol. Disagreement between them is the product.

The gates, all green before any push:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- corpus
```

`cargo test` includes the doc-truth harness (`tests/doc_truth.rs`): every
command/output pair in README.md and `docs/manual/` runs against the built
binary and is byte-compared, so a doc edit that invents output fails CI.

## The independence doctrine

**If you are tempted to import it, reimplement it.**

wolf-interp shares no code with the wolf compiler. Not the lexer, not the
parser, not the type checker, not the directive parser, not the protocol
types — nothing. The two implementations are comparable *only* through
`spec/06-differential-protocol.md`, and that comparison is worthless if both
sides are running the same code. A divergence is the product. Convenience
sharing is how the product dies.

What that means in practice:

- Never add a dependency on a `wolf_*` crate or a git dependency on the
  wolf-lang repository. CI enforces this against `Cargo.lock`.
- Never read `upstream/crates/` or `upstream/xtask/` — not for reference, not
  "just to check the edge case", not to settle an argument. Read `upstream/spec`
  and reimplement. If the spec is silent or ambiguous, that ambiguity is the
  finding: `[proto.cmp.triage]` makes the spec document the defendant first.
- The only permitted coupling is shared *data*: the pinned `upstream/spec` and
  `upstream/corpus` trees, and the protocol schema they define.
- No shared test infrastructure with wolf-lang, either. Especially not "just
  the directive parser" — that parser is precisely the sort of thing whose
  independent reimplementation catches a grammar drift.

When your reimplementation disagrees with the compiler, do not reach for the
compiler's source to find out why. Write the disagreement down; that is a
sprint deliverable, not an inconvenience.

## The divergence-filing rule (standing)

**Any input where this parser and the compiler disagree becomes a wolf-lang
spec issue, with both readings attached.** The grammar document is amended and
both implementations follow the amendment. The two parsers never reconcile by
private agreement, and neither one is patched to match the other before the
clause is fixed.

Triage order is normative (`[proto.cmp.triage]`): **the spec document is the
defendant first.** An ambiguous clause is presumed the root cause until the
clause is shown unambiguous; only then is the implementation that disagrees
with it the defendant. This is not politeness — it is the mechanism by which
differential testing hardens the spec, and skipping it converts a spec bug into
two implementations that quietly agree on something undocumented.

What to attach to the issue:

1. the input, reduced;
2. both records, verbatim (`conform-run --json` from each side);
3. the clause each reading claims to follow, by anchor;
4. the `class` the comparison assigned (`compare::Class`).

Two lists exist so that filing is cheap rather than archaeological, and both
are code, not prose:

- `parse::CHOICES` — every place `spec/01` underdetermines the parse and what
  this implementation chose instead. A divergence that lands on one of these
  is a spec gap with a candidate amendment already written.
- `parse::CHOICES_RESOLVED` — the choices a spec amendment has since **closed**,
  each naming the clause that now decides it. Filed gaps get a visible fate;
  deleting a resolved row would erase the evidence that the pipeline works.
- `diag::UNPINNED_CODES` — every diagnostic code this implementation invented.
  The corpus pins a handful (`check: fail(CODE)`); everything else is a guess
  that the s10 catalog will eventually overrule. Disagreement here is expected,
  not a bug, and it is the intended input to that catalog.
- `eval::rules::Rule` — every *dynamic* rule, with the clause it enforces. A
  divergence at the `run` rung names one of these, and the anchor is already
  attached (`conform-run --trace`, and `x-trap-clause` on trap records).

The pipeline itself is exercised in `tests/divergence.rs` against a seeded
counterparty, because a filing rule nobody has run is a filing rule that does
not work.

### Verdict classes that are not divergences

`src/ledger.rs` sorts an observation against a corpus expectation into five
classes, and three of them are *expected*:

- **dynamic-counterpart** — the corpus pins a static code whose dynamic meaning
  spec/02 states, and this machine trapped with exactly that kind. `E1001` ⇄
  `use-after-move` and `E1002` ⇄ `exclusivity` are the only two
  (`[conf.trap.map]`); nothing else may be added without a clause to cite.
- **conservatism** — the corpus expects a static rejection this implementation
  does not perform, and the program ran. Expected by construction (the sema
  boundary); ledgered, never green-washed.
- **out-of-scope** — verdict `unsupported`, with the reason on `x-unsupported`.

A **mismatch** is a finding and is never explained away in code. Do not "fix" a
mismatch by editing the corpus: the corpus is a pinned input, and
`[proto.cmp.triage]` puts the spec in the dock first.

## The sema boundary

The interpreter implements full dynamic semantics and only the static analysis
needed to run programs — no type checker, no borrow checker, no region checker.
Consequences worth stating for anyone adding code:

- **A property the static tier owns never becomes a trap.** The trap vocabulary
  is closed at eleven kinds and every one of them is a fault of a *defined*
  execution (`[conf.trap.map]`). Wrong arity, a wrong-typed operand, an unknown
  field: those are `unsupported`, with a reason naming the code that owns them.
  Turning one into a trap would put a static rejection in the run rung and
  corrupt the differential comparison in the worst direction.
- **The dynamic machine is deliberately more permissive** than the compiler's
  static checkers. The approximation is one-way: the compiler accepting a
  program obliges this machine not to fault on it; the converse is never
  implied.
- **A std name with no pinned semantics is declined, not guessed.** The ambient
  prelude (`eval::builtin`) mirrors the compiler's names-only stub. Inventing
  behaviour for `acquire()` or `re"…"` would put this implementation's guesses
  into a differential comparison against the compiler's guesses, and the
  resulting "divergence" would be about nothing.

## Commits

Mirroring the compiler track's conventions:

- **Commit in chunks.** One logical change per commit. A refactor and the
  feature it enables are two commits. A toolchain bump or a submodule pin bump
  is always its own commit.
- **Terse, imperative subjects**, under ~250 characters unless the change
  genuinely needs elaboration: `directive: reject unknown trap kinds`, not
  `Added some validation for the trap kinds so that we can...`.
- **No trailers.** No `Co-Authored-By`, no `Generated with`, no tool
  attribution of any kind.
- Never `git checkout` files that were just written but not yet committed —
  stage or stash first. (This has eaten work before.)

## Tests are first class

- Every behaviour worth having is worth a test; every bug fix arrives with the
  test that would have caught it.
- Unit tests live beside the code in `#[cfg(test)] mod tests`. Tests that need
  the pinned corpus, the built binary, or a snapshot live in `tests/`.
- CI is first class too: a red CI is a stop-the-line event, never a thing to
  merge around.

### The snapshot ritual (insta)

Snapshots live in `tests/snapshots/`. When a snapshot legitimately changes:

```sh
INSTA_UPDATE=always cargo test        # rewrite the .snap files
git diff tests/snapshots/             # READ the diff — this is the review
cargo test                            # verify-clean run against the new snaps
```

The middle step is the whole point. `INSTA_UPDATE=always` makes any diff
disappear, including the one that was a real regression; the diff review is
what turns that from a hazard into a workflow. Never commit a `.snap.new` or
`.pending-snap` file.

**Design snapshot content deliberately.** Corpus bytes are canonical output of
`wolf fmt` (STYLE_VERSION 1) — a snapshot embedding corpus text will churn
wholesale when the style version bumps, for reasons having nothing to do with
this repo. Prefer test-owned inputs, as `tests/snapshot_exemplar.rs` does.

`tests/frontend_snapshots.rs` splits the difference on purpose: its fixtures
are test-owned (and so churn only when the lexer changes), while a handful of
corpus-derived snapshots are kept deliberately, because "the tokenizer agrees
with the canonical program" is worth a regression test. When those churn on a
STYLE_VERSION bump, review the diff for span shifts only, then accept.

### Spec-derived tests

`tests/spec_extract.rs` re-reads the pinned `spec/01-grammar.md` at test time
and diffs it against the transcriptions in `lex.rs` and `parse.rs` — the
keyword list and its checksum, the contextual keywords, the §3.2 precedence
table, the counter-examples, the §9 code reservations. **Do not "fix" a failure
there by editing the test.** A failure means the pinned spec and this
implementation disagree, which is either a pin bump you have not absorbed or a
transcription error. Absorb it or fix the transcription.

## Platform lessons (inherited from the compiler track)

These were paid for once already. Do not re-derive them.

- **Line endings are protocol surface.** `.gitattributes` pins `* text=auto
  eol=lf` and it stays that way: spans are byte-exact, and a Windows CRLF
  checkout silently shifts every offset in the file.
- **Never seed an RNG from a raw file path.** Normalize separators first. A
  Windows `\` explored an unvetted seed space compiler-side. More generally:
  no raw platform path ever reaches a record, a report key, or a hash — see
  `wolf_interp::slash_path`.
- **Sort `read_dir` output** before it can influence any generated artifact.
  Directory order is platform noise; the corpus walk sorts at every level and
  again at the end.
- **Pin CI toolchains explicitly.** `rust-toolchain.toml` names the toolchain;
  CI installs it with an explicit `rustup` step rather than trusting the
  runner image's default or an implicit proxy install.
- **The corpus is read-only.** wolf-interp never reformats, rewrites, or
  "fixes" a file under `upstream/`. If a corpus file looks wrong, that is a
  finding to report upstream, not a patch to apply locally.

## Before you push

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- corpus
```

All four green, on a clean tree, with the submodule initialized.

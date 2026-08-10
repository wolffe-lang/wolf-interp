# docs/

Two audiences, two shelves.

## The manual — `manual/`

User-facing: how to build the interpreter, run programs, use the REPL, and
read what the tools print. Start at [manual/README.md](manual/README.md).
The spec is normative; the manual explains how to drive this implementation
of it, and never defines language semantics.

## Engineering documents

The record of how this implementation relates to the spec and to the
compiler. These are contracts and ledgers, not tutorials.

- [approximation-contract.md](approximation-contract.md) — what the dynamic
  machine checks, what it deliberately approximates, and every finding filed
  against `spec/02` and `spec/03`.
- [divergence-log.md](divergence-log.md) — every divergence the differential
  runner has found, with its triage and its fate.
- [conformance-bundle.md](conformance-bundle.md) — the format of the
  exported conformance bundle (schema 1).
- [repl.md](repl.md) — the REPL's session semantics, the `[repl.*]` notes,
  and the transcript format the book replays.

## Doc-truth

Every `console` fence in README.md and `manual/*.md` is executed by
`tests/doc_truth.rs`: `$`-prefixed lines are commands run against the built
binary, everything else is expected output — stdout then stderr —
byte-compared after the repo's normalizations (LF line endings,
`vendor/upstream/` reported as `upstream/`). `…` is the only placeholder:
alone on a line it stands for zero or more elided lines; inside a line it
stands for a span that legitimately varies (a build commit, a bundle
hash). Blocks fenced `sh` or `text` are not executed. A `console` block
whose command is `wolf-interp repl` is replayed as a piped session and
compared as a transcript.

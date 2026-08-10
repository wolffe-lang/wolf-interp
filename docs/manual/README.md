# The lupin manual

How to build the reference interpreter (the wolf-interp package builds a
binary named `lupin`), run wolf programs on it, and read what its tools
print. One sentence of ground rules: the
[spec](../../vendor/upstream/spec) is normative, this implementation is one
reading of it, and where a manual page explains semantics it cites the
clause rather than defining anything.

## Chapters

0. [Installing & building](00-building.md) — toolchain, clone, the pinned
   spec and corpus, verifying the build.
1. [Running programs](01-running-programs.md) — the front door and its exit
   codes, `conform-run`, traps and what they mean.
2. [The REPL](02-repl.md) — a session walkthrough; `:type`, `:regions`,
   `:trace`, `:schedule`; multi-line input; `eval`.
3. [Phases & records](03-phases.md) — the phase ladder, what `unsupported`
   means, reading an observation record.
4. [The differential tools](04-differential.md) — `diff-run`, conformance
   bundles, what a divergence report says.
5. [Troubleshooting](05-troubleshooting.md) — missing corpus, submodule
   problems, platform notes.

## Commands

One line per command, kept identical to `lupin --help`
(`tests/doc_truth.rs` enforces it):

| command | description |
|---|---|
| `run` | Run one program; output passes through live and the exit code is the program's |
| `eval` | Evaluate a snippet in a fresh session and print its value as the REPL would |
| `check` | Check files through the frontend only (lex, parse, resolve) and report diagnostics |
| `corpus` | Walk the pinned corpus and check every directive against this implementation |
| `conformance` | Export the versioned conformance bundle, or check an implementation against one |
| `conform-run` | Observe one program and emit a spec/06 observation record |
| `lex` | Tokenize one program (`spec/01` §1) |
| `parse` | Parse one program (`spec/01` §2-§6) |
| `protocol` | Validate observation records against the spec/06 schema |
| `diff-run` | Compare this implementation against the pinned compiler, corpus-wide |
| `fuzz` | Differential testing over generated programs, with reduction of anything divergent |
| `repl` | Interactive session; `--script` replays a recorded transcript |

`lupin FILE.lu` — no subcommand — is `run`'s short spelling; bare `lupin`
opens the REPL; `-e CODE` is `eval`'s. A subcommand name wins over a file of
the same name (run a file literally named `repl` as `lupin run repl`).

## Command/output pairs

Every `console` fence in these pages is real: the commands run against the
built binary in CI and the output is byte-compared (the conventions are in
[../README.md](../README.md)). If a page disagrees with the binary, the
build is red, not the reader.

# 5 — Troubleshooting

## The binary cannot find the corpus

The pinned trees are looked up relative to the current directory, so run
the tools from the repository root. When neither `upstream/corpus` nor
`vendor/upstream/corpus` is visible, the corpus-consuming commands exit `2`
and say what to do:

```console
$ lupin corpus --root does/not/exist
lupin: corpus root `does/not/exist` is not a directory
  hint: run `git submodule update --init upstream` (or use the tracked vendor/upstream snapshot)
```

A bare clone already contains `vendor/upstream/`, so this normally means
the working directory is wrong, not the checkout.

## Submodule not initialized

`upstream/` is optional. The vendored snapshot covers every workflow except
building the counterparty compiler for `diff-run`. If you want the
submodule, `git submodule update --init upstream` needs read access to the
wolf-lang repository, which is private until v1. That is why
`vendor/upstream/` exists. When both are present the test suite asserts
they are byte-identical, and a mismatch means a half-finished pin bump
(`vendor/README.md` has the re-vendoring commands).

## `diff-run` reports SKIPPED

`diff-run` needs a counterparty binary and looks for the conventional build
products of `cargo build -p wolf_driver` inside `upstream/`. Without one it
SKIPs and exits `0`, loudly, with `notice:` lines naming what was tried. A
missing compiler is an environment fact, not a divergence. Pass
`--require-counterparty` to hard-fail instead, or `--compiler <path>` to
name a binary explicitly.

## Platform notes

Linux x86-64/aarch64, macOS aarch64 and Windows x86-64 are tier-1, and CI
runs the full suite on all three. Line endings are protocol surface:
`.gitattributes` forces LF for every text file, because spans are
byte-exact and a CRLF checkout would shift every offset. If a Windows
checkout shows span-shaped test failures, check `git config core.autocrlf`
against the repository's `.gitattributes` before suspecting the code.
Observation records always spell paths with `/`, on every platform.

## A corpus expectation looks wrong

The corpus is a pinned, read-only input. If a file's directive seems
incorrect, that is a finding to file upstream with both readings attached,
never a local edit. The triage order is normative and the spec document is
the defendant first; [CONTRIBUTING.md](../../CONTRIBUTING.md) walks the
filing rule.

## A refusal that names the fix

Most diagnostics say what was wrong. The separator refusals say what to
write, and where. `[gram.expr.primary]`, `[gram.pat.struct]`, `[gram.pat]`,
`[gram.expr.closure]` and `[gram.expr.unsafe]` all require a `,` between
members (D67/D69), and a program that omits one gets a second line:

```console
$ lupin check upstream/corpus/grammar/struct_literal_no_separator.lu
upstream/corpus/grammar/struct_literal_no_separator.lu: E0201: expected `}`, found identifier `y`; the members of a struct literal are separated — add the comma [gram.expr.primary] at 18:26
  the comma goes here at 18:25
```

The first line's `line:col` is the offending token and is protocol surface
(code + span); the second is the zero-width insertion point where the comma
belongs — after the previous member, which in the multi-line layout is on
the line above. Both the sentence and the pointer are quality concerns and
never reach the observation record: `[proto.record.diag]` (D22) carries
`{code, span, severity}` and nothing else, so growing a teach-note can
never move a differential result (wolf-interp#56).

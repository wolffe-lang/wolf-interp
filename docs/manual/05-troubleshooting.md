# 5 — Troubleshooting

## The binary cannot find the corpus

The pinned trees are looked up relative to the current directory: run the
tools from the repository root. When neither `upstream/corpus` nor
`vendor/upstream/corpus` is visible, the corpus-consuming commands exit `2`
and say what to do:

```console
$ wolf-interp corpus --root does/not/exist
wolf-interp: corpus root `does/not/exist` is not a directory
  hint: run `git submodule update --init upstream` (or use the tracked vendor/upstream snapshot)
```

A bare clone already contains `vendor/upstream/`, so this normally means
the working directory is wrong, not the checkout.

## Submodule not initialized

`upstream/` is optional: the vendored snapshot covers every workflow except
building the counterparty compiler for `diff-run`. If you want the
submodule, `git submodule update --init upstream` needs read access to the
wolf-lang repository — it is private until v1, which is exactly why
`vendor/upstream/` exists. When both are present the test suite asserts
they are byte-identical; a mismatch means a half-finished pin bump
(`vendor/README.md` has the re-vendoring commands).

## `diff-run` reports SKIPPED

`diff-run` needs a counterparty binary and looks for the conventional build
products of `cargo build -p wolf_driver` inside `upstream/`. Without one it
SKIPs — loudly, with `notice:` lines naming what was tried — and exits `0`,
because a missing compiler is an environment fact, not a divergence. Pass
`--require-counterparty` to hard-fail instead, or `--compiler <path>` to
name a binary explicitly.

## Platform notes

Linux x86-64/aarch64, macOS aarch64 and Windows x86-64 are tier-1; CI runs
the full suite on all three. Line endings are protocol surface:
`.gitattributes` forces LF for every text file, because spans are
byte-exact and a CRLF checkout would shift every offset. If a Windows
checkout shows span-shaped test failures, check `git config core.autocrlf`
against the repository's `.gitattributes` before suspecting the code.
Observation records always spell paths with `/`, on every platform.

## A corpus expectation looks wrong

The corpus is a pinned, read-only input. If a file's directive seems
incorrect, that is a finding to file upstream with both readings attached —
never a local edit. The triage order is normative and the spec document is
the defendant first; [CONTRIBUTING.md](../../CONTRIBUTING.md) walks the
filing rule.

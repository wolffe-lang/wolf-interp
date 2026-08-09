# wolf-interp

The wolf reference interpreter: an independent implementation of the wolf
language specification, and the compiler's differential-testing oracle.

Independence is the point: this repo shares **no** frontend or semantics
code with the compiler ([wolf-lang](https://github.com/tenseleyFlow/wolf-lang)).
The only shared artifacts are the spec and corpus it pins, and the
differential protocol (spec/06) both implementations speak.

Dual-licensed MIT or Apache-2.0.

---

## Status: is00 — repo & harness

There is no language work here yet. No lexer, no parser, no evaluator. What
exists is the scaffolding that makes independence mechanical:

- a directive parser for the corpus `//!` header grammar, written from the
  spec rather than from the compiler's parser;
- a corpus harness that walks the pinned corpus and reports what each file
  claims;
- an honest `unsupported` speaker of the spec/06 differential protocol, plus
  the schema validator that keeps it honest.

Every corpus entry is `unsupported`. That is the correct answer today, and
`[proto.record.unsupported]` makes it a legal one — it lands in the
conservatism ledger instead of quietly counting as agreement.

## Getting the pin

`upstream/` is a git submodule pinned to an exact wolf-lang revision. Only
`upstream/spec` and `upstream/corpus` are ever consumed — data, never code.

```sh
git clone https://github.com/tenseleyFlow/wolf-interp
cd wolf-interp
git submodule update --init upstream
```

The submodule carries the whole wolf-lang repository. To keep the compiler's
sources out of your working tree entirely — recommended, and the shape CI
should be read as having — sparse-check it out:

```sh
git -C upstream sparse-checkout init --cone
git -C upstream sparse-checkout set spec corpus
```

### Bumping the pin

A pin bump is a deliberate act, in its own commit, landing CI-green:

```sh
git -C upstream fetch origin trunk
git -C upstream checkout <rev>          # an explicit revision, never a branch
cargo test                              # the corpus-size and anchor tests speak
git add upstream
git commit -m "pin: bump wolf-lang to <rev>"
```

`tests/corpus_harness.rs` asserts the corpus file count and that every
`conforms:` tag in a registered namespace resolves against the pinned
`spec/anchors.json`. Those assertions are the point of the bump commit: if the
upstream corpus grew or a clause anchor moved, the bump is where you find out.

## Commands

```sh
cargo run -- corpus [--root <dir>] [--spec <dir>] [--json]
cargo run -- conform-run <file.lu> [--phase=<p>] [--seed=N] [--json]
cargo run -- protocol validate <record.json>...
```

`conform-run` implements `spec/06-differential-protocol.md` `[proto.invoke]`.
It exits `0` whenever it produced a well-formed observation record — the
*record* carries the program's outcome — and `2` when the tool itself could
not run. `1` means the work ran and failed its check (a red corpus walk, a
rejected record).

Today's record, in full:

```json
{"protocol":1,"impl":"wolf-interp","impl_version":"0.0.1","commit":"…",
 "file":"upstream/corpus/hello.lu","phase_reached":"none","seeded":false,
 "diagnostics":[],"verdict":"unsupported","stdout_sha256":null,"stdout_inline":null}
```

`phase_reached` is `none` no matter what `--phase` asks for, and `seeded` is
`false` no matter what `--seed` asks for. Both are true statements about this
implementation; the requests are acknowledged on stderr.

## The directive grammar

The leading `//!` block of a corpus file. A line is a directive when its first
`:`-delimited token is one of the four keys; every other `//!` line is prose
(prose contains colons all the time, so an unknown key is never an error).

```text
check:    pass
        | fail(CODE)
        | run( exit=N | exit=trap | exit=trap(kind) [, stdout="…"] )
phase:    none | lex | parse | resolve | typecheck | mem | wir | run
conforms: anchor, anchor, …
member:   true | false
```

- `kind` is one of the closed eleven in `[conf.trap.set]`; `phase` is a rung of
  the canonical ladder. Unknown values are errors.
- Duplicate keys are errors.
- `member: true` marks a file belonging to a multi-file module case (directory
  = module; the package root is the entry file's directory). It is exercised
  through its directory's entry file and never conform-run directly, so it
  carries neither `check:` nor `phase:` — it may carry `conforms:`.
- Every other file is an entry and must carry both `check:` and `phase:`.

`spec/05` does not yet publish this grammar in full; the amendment is queued
upstream. Until it lands, the above is the contract this repo implements —
independently, on purpose. A divergence from the compiler's parser is a
finding, not a bug to paper over.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the independence doctrine, the
snapshot ritual, and the commit conventions.

# wolf-interp

The wolf reference interpreter: an independent implementation of the wolf
language specification, and the compiler's differential-testing oracle.

Independence is the point: this repo shares **no** frontend or semantics
code with the compiler ([wolf-lang](https://github.com/tenseleyFlow/wolf-lang)).
The only shared artifacts are the spec and corpus it pins, and the
differential protocol (spec/06) both implementations speak.

Dual-licensed MIT or Apache-2.0.

---

## Status: is01 — the frontend

The lexer and parser are real and cover the whole of
`spec/01-grammar.md`, written from that document and nothing else:

- **lexer** (`src/lex.rs`) — the f-string mode stack (interpolation, format
  specs, nesting to the spec's depth rails), `"""` closing-column dedent, raw
  and generalized literals, and byte-exact terminator insertion per
  `[gram.lex.newline]`;
- **parser** (`src/parse.rs`) — full surface, §3.2's precedence climb kept as
  data so a spec-extraction test can diff it against the pinned table, and
  **no error recovery**: the first error wins, with a span and the
  `[gram.…]` clause that failed;
- **AST** (`src/ast.rs`) — designed for is02's evaluator, every node carrying
  its span and the clause anchor of the production that built it;
- **conform-run** speaks `--phase=lex` and `--phase=parse` for real. Deeper
  rungs report `phase_reached: parse` with verdict `unsupported`, per
  `[proto.record.phase]`'s completed-phase rule.

Against the pinned corpus: every file lexes clean; every file whose ledger
`phase:` is `parse` or deeper parses clean; the four `corpus/grammar/`
counter-examples fail at parse with the codes the corpus pins — E0001, E0002,
E0006, E0008.

No name resolution, no types, no evaluation. Those rungs stay `unsupported`,
which `[proto.record.unsupported]` makes a legal answer that lands in the
conservatism ledger instead of quietly counting as agreement.

### Error codes

`spec/01` §9 reserves E0001–E0008 and `[gram.lex.str]` names E0108; those are
not ours to choose and we emit them where the spec says. Every other code this
implementation emits is **our invention**, listed in `diag::UNPINNED_CODES`
with the clause it serves. Cross-implementation disagreement on unpinned codes
is expected, and it is what drives codes into the spec — see the standing rule
in [CONTRIBUTING.md](CONTRIBUTING.md).

`parse::CHOICES` is the companion list: places where `spec/01` does not
determine the parse, and what this implementation does instead.

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
cargo run -- lex   <file.lu> [--dump]
cargo run -- parse <file.lu> [--dump]
cargo run -- protocol validate <record.json>...
```

`lex --dump` prints the token stream and `parse --dump` prints a production
trace, each line citing the `[gram.…]` clause that built it. Both formats are
**ours**; nothing in the protocol consumes them, and they are deliberately not
modelled on anything the compiler prints. In human mode a rejected program
exits `65`; under `conform-run` a rejection is a *record* and the tool still
exits `0`.

`conform-run` implements `spec/06-differential-protocol.md` `[proto.invoke]`.
It exits `0` whenever it produced a well-formed observation record — the
*record* carries the program's outcome — and `2` when the tool itself could
not run. `1` means the work ran and failed its check (a red corpus walk, a
rejected record).

Today's records, in full:

```json
{"protocol":1,"impl":"wolf-interp","impl_version":"0.0.1","commit":"…",
 "file":"upstream/corpus/hello.lu","phase_reached":"parse","seeded":false,
 "diagnostics":[],"verdict":"unsupported","stdout_sha256":null,"stdout_inline":null}

{"protocol":1,"impl":"wolf-interp","impl_version":"0.0.1","commit":"…",
 "file":"upstream/corpus/grammar/semicolon.lu","phase_reached":"parse","seeded":false,
 "diagnostics":[{"code":"E0002","span":[222,223],"severity":"error"}],
 "verdict":"fail(E0002)","stdout_sha256":null,"stdout_inline":null}
```

`phase_reached` never exceeds the deepest rung that *completed* — `parse`, no
matter what `--phase` asks for — and `seeded` is `false` no matter what
`--seed` asks for. Both are true statements about this implementation; the
requests are acknowledged on stderr. A `fail` carries exactly one diagnostic,
because there is no recovery and `[proto.cmp.phase]` compares only the first.

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

As of the current pin this grammar **is** normative: `spec/05` §2a publishes it
as `[conf.directive.*]`, which is the amendment this repo's independent
implementation was written against and helped queue. The parser here matches
the published clauses; a divergence from the compiler's parser is still a
finding, not a bug to paper over.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- corpus
```

`cargo test` includes the suites that make the frontend defensible:
`conformance.rs` (every corpus file's expectation at the lex and parse rungs),
`spec_extract.rs` (tests re-derived from the pinned markdown on every run — a
spec edit that moves the keyword list, the precedence table or a
counter-example fails here), `fuzz_smoke.rs` (totality over garbage bytes,
token soup and mutated corpus files) and `divergence.rs` (the filing pipeline,
seeded).

See [CONTRIBUTING.md](CONTRIBUTING.md) for the independence doctrine, the
snapshot ritual, and the commit conventions.
